//! RGB curve grading op (port of `GradingRGBCurve.cpp`, `GradingRGBCurveOpData.cpp`,
//! `GradingRGBCurveOp.cpp`, `GradingRGBCurveOpCPU.cpp` and
//! `GradingRGBCurveTransform.cpp`). The B-spline fitting and evaluation live
//! in [`bspline`].

pub mod bspline;

use self::bspline::KnotsCoefs;
use super::grading_primary::GradingValue;
use super::grading_tone::log_lin;
use crate::config::Config;
use crate::context::Context;
use crate::dynamic_property::DynamicProperty;
use crate::error::Result;
use crate::format_metadata::FormatMetadata;
use crate::ops::{Op, OpRc, OpVec, Pixel};
use crate::transforms::grading::GradingRgbCurve;
use crate::transforms::{BuildOps, GradingRgbCurveTransform, Transform, Validate};
use crate::types::{
    DynamicPropertyType, GradingStyle, OptimizationFlags, RgbCurveType, TransformDirection,
};
use std::any::Any;
use std::fmt;
use std::sync::Arc;

/// Validate the curves and compute their knots and coefficients.
fn precompute(v: &GradingRgbCurve) -> Result<KnotsCoefs> {
    v.validate()?;
    KnotsCoefs::from_rgb_curve(v)
}

/// RGB curve grading op (port of `GradingRGBCurveOp` / `GradingRGBCurveOpData`).
#[derive(Debug, Clone)]
pub struct GradingRgbCurveOp {
    style: GradingStyle,
    direction: TransformDirection,
    bypass_lin_to_log: bool,
    value: GradingValue<GradingRgbCurve, KnotsCoefs>,
    metadata: FormatMetadata,
}

impl GradingRgbCurveOp {
    /// Create the op. The curves are validated and fitted (an error is
    /// returned if they need too many knots).
    pub fn new(
        style: GradingStyle,
        value: GradingRgbCurve,
        direction: TransformDirection,
        bypass_lin_to_log: bool,
        dynamic: bool,
    ) -> Result<Self> {
        let kc = precompute(&value)?;
        Ok(Self {
            style,
            direction,
            bypass_lin_to_log,
            value: GradingValue::new(value, kc, dynamic),
            metadata: FormatMetadata::default(),
        })
    }

    /// Identity op of the style.
    pub fn identity(style: GradingStyle) -> Self {
        let value = GradingRgbCurve::new(style);
        let kc = KnotsCoefs::from_rgb_curve(&value).unwrap_or_else(|_| {
            let mut kc = KnotsCoefs::new(4);
            kc.local_bypass = true;
            kc
        });
        Self {
            style,
            direction: TransformDirection::Forward,
            bypass_lin_to_log: false,
            value: GradingValue::new(value, kc, false),
            metadata: FormatMetadata::default(),
        }
    }

    pub fn style(&self) -> GradingStyle {
        self.style
    }
    pub fn direction(&self) -> TransformDirection {
        self.direction
    }
    pub fn bypass_lin_to_log(&self) -> bool {
        self.bypass_lin_to_log
    }
    pub fn metadata(&self) -> &FormatMetadata {
        &self.metadata
    }
    pub fn set_metadata(&mut self, metadata: FormatMetadata) {
        self.metadata = metadata;
    }

    fn state(&self) -> Arc<(GradingRgbCurve, KnotsCoefs)> {
        self.value.current(precompute)
    }

    /// The current value (of the dynamic property if dynamic).
    pub fn value(&self) -> GradingRgbCurve {
        self.state().0.clone()
    }

    /// The current knots and coefficients.
    pub fn knots_coefs(&self) -> KnotsCoefs {
        self.state().1.clone()
    }

    /// The same op in the opposite direction.
    pub fn inverse(&self) -> Self {
        let mut res = self.clone();
        res.direction = self.direction.inverse();
        res
    }

    /// True if `other` is the inverse of `self` (never true for dynamic ops).
    pub fn is_inverse(&self, other: &GradingRgbCurveOp) -> bool {
        if self.is_dynamic() || other.is_dynamic() {
            return false;
        }
        self.style == other.style
            && (self.style != GradingStyle::Lin
                || self.bypass_lin_to_log == other.bypass_lin_to_log)
            && self.state().0 == other.state().0
            && self.direction.combine(other.direction) == TransformDirection::Inverse
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
        if self.bypass_lin_to_log {
            s.push_str(" bypassLinToLog");
        }
        if !self.is_dynamic() {
            s.push_str(&format!("{:.7}", self.state().0));
        }
        s
    }
}

/// Create an RGB curve op (port of `CreateGradingRGBCurveOp`): the op is
/// inverted if `direction` is inverse.
pub fn create_grading_rgb_curve_op(
    ops: &mut OpVec,
    style: GradingStyle,
    value: &GradingRgbCurve,
    op_direction: TransformDirection,
    bypass_lin_to_log: bool,
    dynamic: bool,
    direction: TransformDirection,
) -> Result<()> {
    let op = GradingRgbCurveOp::new(
        style,
        value.clone(),
        op_direction.combine(direction),
        bypass_lin_to_log,
        dynamic,
    )?;
    ops.push(Arc::new(op));
    Ok(())
}

const RED: usize = RgbCurveType::Red as usize;
const GREEN: usize = RgbCurveType::Green as usize;
const BLUE: usize = RgbCurveType::Blue as usize;
const MASTER: usize = RgbCurveType::Master as usize;

#[inline]
fn eval(kc: &KnotsCoefs, out: &mut Pixel) {
    out[0] = kc.eval_curve(RED, out[0], out[0]);
    out[1] = kc.eval_curve(GREEN, out[1], out[1]);
    out[2] = kc.eval_curve(BLUE, out[2], out[2]);
    out[0] = kc.eval_curve(MASTER, out[0], out[0]);
    out[1] = kc.eval_curve(MASTER, out[1], out[1]);
    out[2] = kc.eval_curve(MASTER, out[2], out[2]);
}

#[inline]
fn eval_rev(kc: &KnotsCoefs, out: &mut Pixel) {
    out[0] = kc.eval_curve_rev(MASTER, out[0]);
    out[1] = kc.eval_curve_rev(MASTER, out[1]);
    out[2] = kc.eval_curve_rev(MASTER, out[2]);
    out[0] = kc.eval_curve_rev(RED, out[0]);
    out[1] = kc.eval_curve_rev(GREEN, out[1]);
    out[2] = kc.eval_curve_rev(BLUE, out[2]);
}

impl Op for GradingRgbCurveOp {
    fn name(&self) -> &'static str {
        "GradingRGBCurve"
    }

    fn apply(&self, pixels: &mut [Pixel]) {
        let state = self.state();
        let kc = &state.1;
        if kc.local_bypass {
            return;
        }
        let lin_to_log = self.style == GradingStyle::Lin && !self.bypass_lin_to_log;
        let fwd = self.direction == TransformDirection::Forward;
        for out in pixels.iter_mut() {
            if lin_to_log {
                for o in out.iter_mut().take(3) {
                    *o = log_lin::lin_log(*o);
                }
            }
            if fwd {
                eval(kc, out);
            } else {
                eval_rev(kc, out);
            }
            if lin_to_log {
                for o in out.iter_mut().take(3) {
                    *o = log_lin::log_lin(*o);
                }
            }
        }
    }

    fn is_no_op(&self) -> bool {
        self.is_identity()
    }

    fn is_identity(&self) -> bool {
        !self.is_dynamic() && self.state().0.is_identity()
    }

    fn has_channel_crosstalk(&self) -> bool {
        false
    }

    fn cache_id(&self) -> String {
        format!("<GradingRGBCurveOp {}>", self.data_cache_id())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn combine_with(&self, next: &dyn Op, flags: OptimizationFlags) -> Option<OpVec> {
        if !flags.contains(OptimizationFlags::PAIR_IDENTITY_GRADING) {
            return None;
        }
        let other = next.downcast_ref::<GradingRgbCurveOp>()?;
        self.is_inverse(other).then(OpVec::new)
    }

    fn is_dynamic(&self) -> bool {
        self.value.is_dynamic()
    }

    fn dynamic_property(&self, ty: DynamicPropertyType) -> Option<DynamicProperty> {
        if ty != DynamicPropertyType::GradingRgbCurve {
            return None;
        }
        self.value
            .property()
            .map(|p| DynamicProperty::GradingRgbCurve(p.clone()))
    }

    fn replace_dynamic_property(&mut self, prop: &DynamicProperty) {
        if let Some(p) = prop.as_grading_rgb_curve() {
            self.value.replace_property(p);
        }
    }

    fn make_non_dynamic(&self) -> Option<OpRc> {
        if !self.is_dynamic() {
            return None;
        }
        let value = self.value.to_static(precompute);
        Some(Arc::new(Self {
            value,
            ..self.clone()
        }))
    }

    fn to_transform(&self) -> Option<Transform> {
        Some(Transform::GradingRgbCurve(GradingRgbCurveTransform {
            direction: self.direction,
            style: self.style,
            value: self.value(),
            bypass_lin_to_log: self.bypass_lin_to_log,
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

impl GradingRgbCurveTransform {
    /// Change the style (curves are reset to the defaults of the new style).
    pub fn set_style(&mut self, style: GradingStyle) {
        if style != self.style {
            self.style = style;
            self.value = GradingRgbCurve::new(style);
        }
    }

    /// Set the curves (they are validated first).
    pub fn set_value(&mut self, value: GradingRgbCurve) -> Result<()> {
        precompute(&value)?;
        self.value = value;
        Ok(())
    }

    /// Slope of a control point of a curve.
    pub fn slope(&self, c: RgbCurveType, index: usize) -> Result<f32> {
        self.value.curve(c).try_slope(index)
    }

    /// Set the slope of a control point of a curve.
    pub fn set_slope(&mut self, c: RgbCurveType, index: usize, slope: f32) -> Result<()> {
        let mut value = self.value.clone();
        value.curve_mut(c).try_set_slope(index, slope)?;
        self.set_value(value)
    }

    /// True if the slopes of a curve are the default ones.
    pub fn slopes_are_default(&self, c: RgbCurveType) -> bool {
        self.value.curve(c).slopes_are_default()
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

impl Validate for GradingRgbCurveTransform {
    fn validate(&self) -> Result<()> {
        self.value
            .validate()
            .map_err(|e| e.prefixed("GradingRGBCurveTransform validation failed: "))
    }
}

impl BuildOps for GradingRgbCurveTransform {
    fn build_ops(
        &self,
        ops: &mut OpVec,
        _config: &Config,
        _context: &Context,
        dir: TransformDirection,
    ) -> Result<()> {
        let mut op = GradingRgbCurveOp::new(
            self.style,
            self.value.clone(),
            self.direction.combine(dir),
            self.bypass_lin_to_log,
            self.dynamic,
        )?;
        op.set_metadata(self.metadata.clone());
        ops.push(Arc::new(op));
        Ok(())
    }
}

impl fmt::Display for GradingRgbCurveTransform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "<GradingRGBCurveTransform direction={}, style={}, values={}",
            self.direction.as_str(),
            self.style.as_str(),
            self.value
        )?;
        if self.bypass_lin_to_log {
            f.write_str(", bypass_lintolog")?;
        }
        if self.dynamic {
            f.write_str(", dynamic")?;
        }
        f.write_str(">")
    }
}

#[cfg(test)]
mod tests;
