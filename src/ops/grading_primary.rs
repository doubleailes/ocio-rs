//! Primary grading op (port of `GradingPrimary.cpp`, `GradingPrimaryOpData.cpp`,
//! `GradingPrimaryOp.cpp`, `GradingPrimaryOpCPU.cpp` and
//! `GradingPrimaryTransform.cpp`).
//!
//! The module also hosts [`GradingValue`], the value holder shared by all the
//! grading ops: it keeps either a static value or a dynamic property, together
//! with the values precomputed for rendering, which are refreshed lazily when
//! the dynamic property changes.

use crate::config::Config;
use crate::context::Context;
use crate::dynamic_property::{DynamicProperty, SharedValue};
use crate::error::Result;
use crate::format_metadata::FormatMetadata;
use crate::ops::{Op, OpRc, OpVec, Pixel};
use crate::transforms::grading::GradingPrimary;
use crate::transforms::{BuildOps, GradingPrimaryTransform, RangeTransform, Transform, Validate};
use crate::types::{DynamicPropertyType, GradingStyle, OptimizationFlags, TransformDirection};
use std::any::Any;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};

// ---------------------------------------------------------------------------
// Shared value holder for the grading ops.

/// Value of a grading op: a static value, or a dynamic property, together with
/// the precomputed render values (`P`) of the last valid value.
///
/// When the value is dynamic, [`GradingValue::current`] compares the
/// property value with the cached one and, when it changed, validates and
/// precomputes the new value. An invalid value (which OCIO would reject in
/// `DynamicProperty*::setValue`) is ignored and the last valid value keeps
/// being used.
#[derive(Debug)]
pub(crate) struct GradingValue<V, P> {
    property: Option<SharedValue<V>>,
    state: Mutex<Arc<(V, P)>>,
}

impl<V: Clone + PartialEq, P> GradingValue<V, P> {
    /// A new holder. `pre` must be the precomputation of `value`.
    pub(crate) fn new(value: V, pre: P, dynamic: bool) -> Self {
        let property = if dynamic {
            Some(SharedValue::new(value.clone()))
        } else {
            None
        };
        Self {
            property,
            state: Mutex::new(Arc::new((value, pre))),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Arc<(V, P)>> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// True if the value is held by a dynamic property.
    pub(crate) fn is_dynamic(&self) -> bool {
        self.property.is_some()
    }

    /// The dynamic property handle, if dynamic.
    pub(crate) fn property(&self) -> Option<&SharedValue<V>> {
        self.property.as_ref()
    }

    /// Replace the dynamic property handle (only if already dynamic).
    pub(crate) fn replace_property(&mut self, prop: &SharedValue<V>) {
        if self.property.is_some() {
            self.property = Some(prop.clone());
        }
    }

    /// Current value and its precomputed render values. `compute` validates
    /// and precomputes a new value of the dynamic property.
    pub(crate) fn current(&self, compute: impl FnOnce(&V) -> Result<P>) -> Arc<(V, P)> {
        let mut guard = self.lock();
        if let Some(prop) = &self.property {
            let changed = prop.with(|v| *v != guard.0);
            if changed {
                let v = prop.get();
                if let Ok(p) = compute(&v) {
                    *guard = Arc::new((v, p));
                }
            }
        }
        guard.clone()
    }

    /// Static copy holding the current state.
    pub(crate) fn to_static(&self, compute: impl FnOnce(&V) -> Result<P>) -> Self {
        let state = self.current(compute);
        Self {
            property: None,
            state: Mutex::new(state),
        }
    }
}

impl<V, P> Clone for GradingValue<V, P> {
    fn clone(&self) -> Self {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner()).clone();
        Self {
            property: self.property.clone(),
            state: Mutex::new(state),
        }
    }
}

/// `std::min` semantic (returns `a` if the comparison fails, e.g. NaN).
#[inline]
pub(crate) fn std_min(a: f32, b: f32) -> f32 {
    if b < a {
        b
    } else {
        a
    }
}

/// `std::max` semantic (returns `a` if the comparison fails, e.g. NaN).
#[inline]
pub(crate) fn std_max(a: f32, b: f32) -> f32 {
    if a < b {
        b
    } else {
        a
    }
}

// ---------------------------------------------------------------------------
// Precomputed values

/// Values precomputed from a [`GradingPrimary`] for rendering (port of
/// `GradingPrimaryPreRender`). The values are already inverted according to
/// the direction.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct GradingPrimaryPreRender {
    brightness: [f32; 3],
    contrast: [f32; 3],
    gamma: [f32; 3],
    exposure: [f32; 3],
    offset: [f32; 3],
    slope: [f32; 3],
    pivot: f64,
    is_power_identity: bool,
    local_bypass: bool,
}

impl GradingPrimaryPreRender {
    /// Precomputed values for `v` in `style` and direction `dir`.
    pub fn new(style: GradingStyle, dir: TransformDirection, v: &GradingPrimary) -> Self {
        let mut p = Self::default();
        p.update(style, dir, v);
        p
    }

    /// Do not apply the op if all params are identity.
    pub fn local_bypass(&self) -> bool {
        self.local_bypass
    }
    pub fn brightness(&self) -> [f32; 3] {
        self.brightness
    }
    pub fn contrast(&self) -> [f32; 3] {
        self.contrast
    }
    pub fn gamma(&self) -> [f32; 3] {
        self.gamma
    }
    pub fn pivot(&self) -> f64 {
        self.pivot
    }
    pub fn is_gamma_identity(&self) -> bool {
        self.is_power_identity
    }
    pub fn exposure(&self) -> [f32; 3] {
        self.exposure
    }
    pub fn offset(&self) -> [f32; 3] {
        self.offset
    }
    pub fn is_contrast_identity(&self) -> bool {
        self.is_power_identity
    }
    pub fn slope(&self) -> [f32; 3] {
        self.slope
    }

    /// Recompute the values.
    pub fn update(&mut self, style: GradingStyle, dir: TransformDirection, v: &GradingPrimary) {
        self.local_bypass = v.saturation == 1.0
            && v.clamp_black == GradingPrimary::NO_CLAMP_BLACK
            && v.clamp_white == GradingPrimary::NO_CLAMP_WHITE;

        let fwd = dir == TransformDirection::Forward;
        match style {
            GradingStyle::Log => {
                let b = &v.brightness;
                let c = &v.contrast;
                let g = &v.gamma;
                let br = [
                    ((b.master + b.red) * 6.25 / 1023.0) as f32,
                    ((b.master + b.green) * 6.25 / 1023.0) as f32,
                    ((b.master + b.blue) * 6.25 / 1023.0) as f32,
                ];
                let cs = [c.master * c.red, c.master * c.green, c.master * c.blue];
                let gs = [g.master * g.red, g.master * g.green, g.master * g.blue];
                if fwd {
                    self.brightness = br;
                    self.contrast = cs.map(|x| x as f32);
                    self.gamma = gs.map(|x| (1.0 / x) as f32);
                } else {
                    self.brightness = br.map(|x| -x);
                    self.contrast = cs.map(|x| (1.0 / if x == 0.0 { 1.0 } else { x }) as f32);
                    self.gamma = gs.map(|x| x as f32);
                }
                self.is_power_identity = self.gamma == [1.0; 3];
                self.pivot = 0.5 + v.pivot * 0.5;
                self.local_bypass = self.local_bypass
                    && self.is_power_identity
                    && self.brightness == [0.0; 3]
                    && self.contrast == [1.0; 3];
            }
            GradingStyle::Lin => {
                let o = &v.offset;
                let e = &v.exposure;
                let c = &v.contrast;
                let os = [
                    (o.master + o.red) as f32,
                    (o.master + o.green) as f32,
                    (o.master + o.blue) as f32,
                ];
                let es = [
                    (e.master + e.red) as f32,
                    (e.master + e.green) as f32,
                    (e.master + e.blue) as f32,
                ];
                let cs = [c.master * c.red, c.master * c.green, c.master * c.blue];
                if fwd {
                    self.offset = os;
                    self.exposure = es.map(|x| 2.0f32.powf(x));
                    self.contrast = cs.map(|x| x as f32);
                } else {
                    self.offset = os.map(|x| -x);
                    self.exposure = es.map(|x| 1.0 / 2.0f32.powf(x));
                    // Validate ensures contrast is above a threshold.
                    self.contrast = cs.map(|x| (1.0 / x) as f32);
                }
                self.is_power_identity = self.contrast == [1.0; 3];
                self.pivot = 0.18 * 2.0f64.powf(v.pivot);
                self.local_bypass = self.local_bypass
                    && self.is_power_identity
                    && self.exposure == [1.0; 3]
                    && self.offset == [0.0; 3];
            }
            GradingStyle::Video => {
                let o = &v.offset;
                let l = &v.lift;
                let g = &v.gamma;
                let non_zero = |x: f64| if x == 0.0 { 1.0 } else { x };
                let gain = [
                    non_zero(v.gain.master * v.gain.red),
                    non_zero(v.gain.master * v.gain.green),
                    non_zero(v.gain.master * v.gain.blue),
                ];
                let lift = [l.master + l.red, l.master + l.green, l.master + l.blue];
                // Summed left to right, as in OCIO.
                let off = [
                    o.master + o.red + l.master + l.red,
                    o.master + o.green + l.master + l.green,
                    o.master + o.blue + l.master + l.blue,
                ];
                let gs = [g.master * g.red, g.master * g.green, g.master * g.blue];
                let pw = v.pivot_white;
                let pb = v.pivot_black;
                if fwd {
                    for i in 0..3 {
                        self.offset[i] = off[i] as f32;
                        let slope_den = pw / gain[i] + lift[i] - pb;
                        self.slope[i] = ((pw - pb) / non_zero(slope_den)) as f32;
                        self.gamma[i] = (1.0 / gs[i]) as f32;
                    }
                } else {
                    for i in 0..3 {
                        self.offset[i] = -(off[i] as f32);
                        self.slope[i] = ((pw / gain[i] + (lift[i] - pb)) / (pw - pb)) as f32;
                        self.gamma[i] = gs[i] as f32;
                    }
                }
                self.is_power_identity = self.gamma == [1.0; 3];
                self.local_bypass = self.local_bypass
                    && self.is_power_identity
                    && self.slope == [1.0; 3]
                    && self.offset == [0.0; 3];
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The op

/// Primary grading op (port of `GradingPrimaryOp` / `GradingPrimaryOpData`).
#[derive(Debug, Clone)]
pub struct GradingPrimaryOp {
    style: GradingStyle,
    direction: TransformDirection,
    value: GradingValue<GradingPrimary, GradingPrimaryPreRender>,
    metadata: FormatMetadata,
}

impl GradingPrimaryOp {
    /// Create the op. The value is validated.
    pub fn new(
        style: GradingStyle,
        value: GradingPrimary,
        direction: TransformDirection,
        dynamic: bool,
    ) -> Result<Self> {
        value.validate(style)?;
        let pre = compute_pre_render(style, direction, &value);
        Ok(Self {
            style,
            direction,
            value: GradingValue::new(value, pre, dynamic),
            metadata: FormatMetadata::default(),
        })
    }

    /// Identity op of the style.
    pub fn identity(style: GradingStyle) -> Self {
        let value = GradingPrimary::new(style);
        let pre = compute_pre_render(style, TransformDirection::Forward, &value);
        Self {
            style,
            direction: TransformDirection::Forward,
            value: GradingValue::new(value, pre, false),
            metadata: FormatMetadata::default(),
        }
    }

    pub fn style(&self) -> GradingStyle {
        self.style
    }
    pub fn direction(&self) -> TransformDirection {
        self.direction
    }
    pub fn metadata(&self) -> &FormatMetadata {
        &self.metadata
    }
    pub fn set_metadata(&mut self, metadata: FormatMetadata) {
        self.metadata = metadata;
    }

    /// The current value (of the dynamic property if dynamic).
    pub fn value(&self) -> GradingPrimary {
        self.state().0
    }

    fn state(&self) -> Arc<(GradingPrimary, GradingPrimaryPreRender)> {
        let (style, dir) = (self.style, self.direction);
        self.value.current(|v| {
            v.validate(style)?;
            Ok(compute_pre_render(style, dir, v))
        })
    }

    /// The same op in the opposite direction.
    pub fn inverse(&self) -> Self {
        let mut res = self.clone();
        res.direction = self.direction.inverse();
        let value = self.value();
        let pre = compute_pre_render(res.style, res.direction, &value);
        res.value = GradingValue {
            property: self.value.property.clone(),
            state: Mutex::new(Arc::new((value, pre))),
        };
        res
    }

    /// True if `other` is the inverse of `self` (never true for dynamic ops).
    pub fn is_inverse(&self, other: &GradingPrimaryOp) -> bool {
        if self.is_dynamic() || other.is_dynamic() {
            return false;
        }
        self.style == other.style
            && self.value() == other.value()
            && self.direction.combine(other.direction) == TransformDirection::Inverse
    }

    /// The ops replacing the pair `self` + inverse: a range emulating the
    /// clamps, or nothing when there are no clamps. `None` if the range op
    /// cannot be created.
    fn identity_replacement(&self) -> Option<OpVec> {
        let v = self.value();
        let low = (v.clamp_black != GradingPrimary::NO_CLAMP_BLACK).then_some(v.clamp_black);
        let high = (v.clamp_white != GradingPrimary::NO_CLAMP_WHITE).then_some(v.clamp_white);
        if low.is_none() && high.is_none() {
            return Some(OpVec::new());
        }
        let range = Transform::Range(RangeTransform::new(low, high, low, high));
        let mut ops = OpVec::new();
        crate::transforms::build::build_ops(
            &mut ops,
            &Config::create_raw(),
            &Context::new(),
            &range,
            TransformDirection::Forward,
        )
        .ok()?;
        Some(ops)
    }

    fn data_cache_id(&self) -> String {
        let mut s = String::new();
        if !self.metadata.id().is_empty() {
            s.push_str(self.metadata.id());
            s.push(' ');
        }
        s.push_str(self.style.as_str());
        s.push(' ');
        s.push_str(self.direction.as_str());
        s.push(' ');
        if !self.is_dynamic() {
            s.push_str(&format!("{:.7}", self.value()));
        }
        s
    }
}

fn compute_pre_render(
    style: GradingStyle,
    dir: TransformDirection,
    v: &GradingPrimary,
) -> GradingPrimaryPreRender {
    GradingPrimaryPreRender::new(style, dir, v)
}

/// Create a primary grading op (port of `CreateGradingPrimaryOp`): the op is
/// inverted if `direction` is inverse.
pub fn create_grading_primary_op(
    ops: &mut OpVec,
    style: GradingStyle,
    value: &GradingPrimary,
    op_direction: TransformDirection,
    dynamic: bool,
    direction: TransformDirection,
) -> Result<()> {
    let op = GradingPrimaryOp::new(style, *value, op_direction.combine(direction), dynamic)?;
    ops.push(Arc::new(op));
    Ok(())
}

// ---------------------------------------------------------------------------
// CPU rendering

#[inline]
fn apply_contrast(pix: &mut Pixel, contrast: &[f32; 3], pivot: f32) {
    for i in 0..3 {
        pix[i] = (pix[i] - pivot) * contrast[i] + pivot;
    }
}

#[inline]
fn apply_lin_contrast(pix: &mut Pixel, contrast: &[f32; 3], pivot: f32) {
    for i in 0..3 {
        pix[i] = (pix[i] / pivot).abs().powf(contrast[i]) * pivot.copysign(pix[i]);
    }
}

#[inline]
fn apply_slope(pix: &mut Pixel, slope: &[f32; 3]) {
    for i in 0..3 {
        pix[i] *= slope[i];
    }
}

#[inline]
fn apply_offset(pix: &mut Pixel, offset: &[f32; 3]) {
    for i in 0..3 {
        pix[i] += offset[i];
    }
}

#[inline]
fn apply_gamma(pix: &mut Pixel, gamma: &[f32; 3], black_pivot: f32, white_pivot: f32) {
    for i in 0..3 {
        pix[i] = ((pix[i] - black_pivot).abs() / (white_pivot - black_pivot)).powf(gamma[i])
            * 1.0f32.copysign(pix[i] - black_pivot)
            * (white_pivot - black_pivot)
            + black_pivot;
    }
}

#[inline]
fn apply_saturation(pix: &mut Pixel, saturation: f32) {
    if saturation != 1.0 {
        const LUMA_WEIGHTS: [f32; 3] = [0.2126, 0.7152, 0.0722];
        let src = [pix[0], pix[1], pix[2]];
        let luma = src[0] * LUMA_WEIGHTS[0] + src[1] * LUMA_WEIGHTS[1] + src[2] * LUMA_WEIGHTS[2];
        for i in 0..3 {
            pix[i] = luma + saturation * (src[i] - luma);
        }
    }
}

#[inline]
fn apply_clamp(pix: &mut Pixel, clamp_min: f32, clamp_max: f32) {
    for p in pix.iter_mut().take(3) {
        *p = std_min(std_max(*p, clamp_min), clamp_max);
    }
}

fn apply_log_fwd(v: &GradingPrimary, comp: &GradingPrimaryPreRender, pixels: &mut [Pixel]) {
    let brightness = comp.brightness;
    let contrast = comp.contrast;
    let gamma = comp.gamma;
    let actual_pivot = comp.pivot as f32;
    let saturation = v.saturation as f32;
    let pivot_black = v.pivot_black as f32;
    let pivot_white = v.pivot_white as f32;
    let clamp_black = v.clamp_black as f32;
    let clamp_white = v.clamp_white as f32;
    let gamma_identity = comp.is_gamma_identity();
    for out in pixels.iter_mut() {
        apply_offset(out, &brightness);
        apply_contrast(out, &contrast, actual_pivot);
        if !gamma_identity {
            apply_gamma(out, &gamma, pivot_black, pivot_white);
        }
        apply_saturation(out, saturation);
        apply_clamp(out, clamp_black, clamp_white);
    }
}

fn apply_log_rev(v: &GradingPrimary, comp: &GradingPrimaryPreRender, pixels: &mut [Pixel]) {
    let brightness_inv = comp.brightness;
    let contrast_inv = comp.contrast;
    let gamma_inv = comp.gamma;
    let pivot_black = v.pivot_black as f32;
    let pivot_white = v.pivot_white as f32;
    let clamp_black = v.clamp_black as f32;
    let clamp_white = v.clamp_white as f32;
    let actual_pivot = comp.pivot as f32;
    let sat = v.saturation as f32;
    let sat_inv = 1.0 / if sat != 0.0 { sat } else { 1.0 };
    let gamma_identity = comp.is_gamma_identity();
    for out in pixels.iter_mut() {
        apply_clamp(out, clamp_black, clamp_white);
        apply_saturation(out, sat_inv);
        if !gamma_identity {
            apply_gamma(out, &gamma_inv, pivot_black, pivot_white);
        }
        apply_contrast(out, &contrast_inv, actual_pivot);
        apply_offset(out, &brightness_inv);
    }
}

fn apply_lin_fwd(v: &GradingPrimary, comp: &GradingPrimaryPreRender, pixels: &mut [Pixel]) {
    let offset = comp.offset;
    let exposure = comp.exposure;
    let contrast = comp.contrast;
    let actual_pivot = comp.pivot as f32;
    let saturation = v.saturation as f32;
    let clamp_black = v.clamp_black as f32;
    let clamp_white = v.clamp_white as f32;
    let contrast_identity = comp.is_contrast_identity();
    for out in pixels.iter_mut() {
        apply_offset(out, &offset);
        apply_slope(out, &exposure);
        if !contrast_identity {
            apply_lin_contrast(out, &contrast, actual_pivot);
        }
        apply_saturation(out, saturation);
        apply_clamp(out, clamp_black, clamp_white);
    }
}

fn apply_lin_rev(v: &GradingPrimary, comp: &GradingPrimaryPreRender, pixels: &mut [Pixel]) {
    let offset_inv = comp.offset;
    let exposure_inv = comp.exposure;
    let contrast_inv = comp.contrast;
    let actual_pivot = comp.pivot as f32;
    let sat = v.saturation as f32;
    let sat_inv = 1.0 / if sat != 0.0 { sat } else { 1.0 };
    let clamp_black = v.clamp_black as f32;
    let clamp_white = v.clamp_white as f32;
    let contrast_identity = comp.is_contrast_identity();
    for out in pixels.iter_mut() {
        apply_clamp(out, clamp_black, clamp_white);
        apply_saturation(out, sat_inv);
        if !contrast_identity {
            apply_lin_contrast(out, &contrast_inv, actual_pivot);
        }
        apply_slope(out, &exposure_inv);
        apply_offset(out, &offset_inv);
    }
}

fn apply_vid_fwd(v: &GradingPrimary, comp: &GradingPrimaryPreRender, pixels: &mut [Pixel]) {
    let gamma = comp.gamma;
    let offset = comp.offset;
    let slope = comp.slope;
    let saturation = v.saturation as f32;
    let clamp_black = v.clamp_black as f32;
    let clamp_white = v.clamp_white as f32;
    let pivot_black = v.pivot_black as f32;
    let pivot_white = v.pivot_white as f32;
    let gamma_identity = comp.is_gamma_identity();
    for out in pixels.iter_mut() {
        apply_offset(out, &offset);
        apply_contrast(out, &slope, pivot_black);
        if !gamma_identity {
            apply_gamma(out, &gamma, pivot_black, pivot_white);
        }
        apply_saturation(out, saturation);
        apply_clamp(out, clamp_black, clamp_white);
    }
}

fn apply_vid_rev(v: &GradingPrimary, comp: &GradingPrimaryPreRender, pixels: &mut [Pixel]) {
    let gamma_inv = comp.gamma;
    let offset_inv = comp.offset;
    let slope_inv = comp.slope;
    let pivot_black = v.pivot_black as f32;
    let pivot_white = v.pivot_white as f32;
    let clamp_black = v.clamp_black as f32;
    let clamp_white = v.clamp_white as f32;
    let sat = v.saturation as f32;
    let sat_inv = 1.0 / if sat != 0.0 { sat } else { 1.0 };
    let gamma_identity = comp.is_gamma_identity();
    for out in pixels.iter_mut() {
        apply_clamp(out, clamp_black, clamp_white);
        apply_saturation(out, sat_inv);
        if !gamma_identity {
            apply_gamma(out, &gamma_inv, pivot_black, pivot_white);
        }
        apply_contrast(out, &slope_inv, pivot_black);
        apply_offset(out, &offset_inv);
    }
}

impl Op for GradingPrimaryOp {
    fn name(&self) -> &'static str {
        "GradingPrimary"
    }

    fn apply(&self, pixels: &mut [Pixel]) {
        let state = self.state();
        let (v, comp) = (&state.0, &state.1);
        if comp.local_bypass() {
            return;
        }
        match (self.style, self.direction) {
            (GradingStyle::Log, TransformDirection::Forward) => apply_log_fwd(v, comp, pixels),
            (GradingStyle::Log, TransformDirection::Inverse) => apply_log_rev(v, comp, pixels),
            (GradingStyle::Lin, TransformDirection::Forward) => apply_lin_fwd(v, comp, pixels),
            (GradingStyle::Lin, TransformDirection::Inverse) => apply_lin_rev(v, comp, pixels),
            (GradingStyle::Video, TransformDirection::Forward) => apply_vid_fwd(v, comp, pixels),
            (GradingStyle::Video, TransformDirection::Inverse) => apply_vid_rev(v, comp, pixels),
        }
    }

    fn is_no_op(&self) -> bool {
        self.is_identity()
    }

    fn is_identity(&self) -> bool {
        if self.is_dynamic() {
            return false;
        }
        let def = GradingPrimary::new(self.style);
        let v = self.value();
        if def.saturation == v.saturation
            && def.clamp_black == v.clamp_black
            && def.clamp_white == v.clamp_white
        {
            // Pivot values can be ignored if the other values are identity.
            match self.style {
                GradingStyle::Log => {
                    def.pivot_black == v.pivot_black
                        && def.pivot_white == v.pivot_white
                        && def.brightness == v.brightness
                        && def.contrast == v.contrast
                        && def.gamma == v.gamma
                }
                GradingStyle::Lin => {
                    def.contrast == v.contrast
                        && def.offset == v.offset
                        && def.exposure == v.exposure
                }
                GradingStyle::Video => {
                    def.gamma == v.gamma
                        && def.offset == v.offset
                        && def.lift == v.lift
                        && def.gain == v.gain
                }
            }
        } else {
            false
        }
    }

    fn has_channel_crosstalk(&self) -> bool {
        self.value().saturation != 1.0
    }

    fn cache_id(&self) -> String {
        format!("<GradingPrimaryOp {}>", self.data_cache_id())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn combine_with(&self, next: &dyn Op, flags: OptimizationFlags) -> Option<OpVec> {
        if !flags.contains(OptimizationFlags::PAIR_IDENTITY_GRADING) {
            return None;
        }
        let other = next.downcast_ref::<GradingPrimaryOp>()?;
        if self.is_inverse(other) {
            self.identity_replacement()
        } else {
            None
        }
    }

    fn is_dynamic(&self) -> bool {
        self.value.is_dynamic()
    }

    fn dynamic_property(&self, ty: DynamicPropertyType) -> Option<DynamicProperty> {
        if ty != DynamicPropertyType::GradingPrimary {
            return None;
        }
        self.value
            .property()
            .map(|p| DynamicProperty::GradingPrimary(p.clone()))
    }

    fn replace_dynamic_property(&mut self, prop: &DynamicProperty) {
        if let Some(p) = prop.as_grading_primary() {
            self.value.replace_property(p);
        }
    }

    fn make_non_dynamic(&self) -> Option<OpRc> {
        if !self.is_dynamic() {
            return None;
        }
        let (style, dir) = (self.style, self.direction);
        let value = self.value.to_static(|v| {
            v.validate(style)?;
            Ok(compute_pre_render(style, dir, v))
        });
        Some(Arc::new(Self {
            value,
            ..self.clone()
        }))
    }

    fn to_transform(&self) -> Option<Transform> {
        Some(Transform::GradingPrimary(GradingPrimaryTransform {
            direction: self.direction,
            style: self.style,
            value: self.value(),
            dynamic: self.is_dynamic(),
            metadata: self.metadata.clone(),
        }))
    }

    fn clone_box(&self) -> Box<dyn Op> {
        Box::new(self.clone())
    }
}

// ---------------------------------------------------------------------------
// Transform

impl GradingPrimaryTransform {
    /// Change the style (values are reset to the defaults of the new style).
    pub fn set_style(&mut self, style: GradingStyle) {
        if style != self.style {
            self.style = style;
            self.value = GradingPrimary::new(style);
        }
    }

    /// Set the values (they are validated first).
    pub fn set_value(&mut self, value: GradingPrimary) -> Result<()> {
        value.validate(self.style)?;
        self.value = value;
        Ok(())
    }

    pub fn is_dynamic(&self) -> bool {
        self.dynamic
    }
    pub fn make_dynamic(&mut self) {
        self.dynamic = true;
    }
    pub fn make_non_dynamic(&mut self) {
        self.dynamic = false;
    }
}

impl Validate for GradingPrimaryTransform {
    fn validate(&self) -> Result<()> {
        self.value
            .validate(self.style)
            .map_err(|e| e.prefixed("GradingPrimaryTransform validation failed: "))
    }
}

impl BuildOps for GradingPrimaryTransform {
    fn build_ops(
        &self,
        ops: &mut OpVec,
        _config: &Config,
        _context: &Context,
        dir: TransformDirection,
    ) -> Result<()> {
        let mut op = GradingPrimaryOp::new(
            self.style,
            self.value,
            self.direction.combine(dir),
            self.dynamic,
        )?;
        op.set_metadata(self.metadata.clone());
        ops.push(Arc::new(op));
        Ok(())
    }
}

impl fmt::Display for GradingPrimaryTransform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "<GradingPrimaryTransform direction={}, style={}, values={}",
            self.direction.as_str(),
            self.style.as_str(),
            self.value
        )?;
        if self.dynamic {
            f.write_str(", dynamic")?;
        }
        f.write_str(">")
    }
}

#[cfg(test)]
pub(crate) mod tests;
