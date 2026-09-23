//! Exposure / contrast op (port of `ExposureContrastOpData`,
//! `ExposureContrastOp`, `ExposureContrastOpCPU` and the op building part of
//! `ExposureContrastTransform.cpp`).
//!
//! Exposure, contrast and gamma may be dynamic properties: their current
//! value is read on each `apply`, so they can be changed after the processor
//! is created.

use crate::config::Config;
use crate::context::Context;
use crate::dynamic_property::{DynamicProperty, SharedValue};
use crate::error::Result;
use crate::format_metadata::FormatMetadata;
use crate::ops::matrix::float_format::format_g;
use crate::ops::{Op, OpRc, OpVec, Pixel};
use crate::transforms::{BuildOps, ExposureContrastTransform, Transform, Validate};
use crate::types::{
    DynamicPropertyType, ExposureContrastStyle, OptimizationFlags, TransformDirection,
};
use std::any::Any;
use std::sync::Arc;

/// Minimum pivot value.
pub const MIN_PIVOT: f64 = 0.001;
/// Minimum contrast value.
pub const MIN_CONTRAST: f64 = 0.001;
/// Power of the video OETF (1 / 1.83).
pub const VIDEO_OETF_POWER: f64 = 0.54644808743169393;
/// Default log exposure step.
pub const LOGEXPOSURESTEP_DEFAULT: f64 = 0.088;
/// Default log mid gray.
pub const LOGMIDGRAY_DEFAULT: f64 = 0.435;

const FLOAT_DECIMALS: usize = 7;

/// Styles of the exposure contrast op (port of `ExposureContrastOpData::Style`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum EcStyle {
    /// E/C applied to a linear space image.
    #[default]
    Linear,
    /// Inverse of [`EcStyle::Linear`].
    LinearRev,
    /// E/C applied to a video space image.
    Video,
    /// Inverse of [`EcStyle::Video`].
    VideoRev,
    /// E/C applied to a log space image.
    Logarithmic,
    /// Inverse of [`EcStyle::Logarithmic`].
    LogarithmicRev,
}

/// CTF name of the linear style.
pub const EC_STYLE_LINEAR: &str = "linear";
/// CTF name of the inverse linear style.
pub const EC_STYLE_LINEAR_REV: &str = "linearRev";
/// CTF name of the video style.
pub const EC_STYLE_VIDEO: &str = "video";
/// CTF name of the inverse video style.
pub const EC_STYLE_VIDEO_REV: &str = "videoRev";
/// CTF name of the log style.
pub const EC_STYLE_LOGARITHMIC: &str = "log";
/// CTF name of the inverse log style.
pub const EC_STYLE_LOGARITHMIC_REV: &str = "logRev";

impl EcStyle {
    /// All the styles.
    pub const ALL: [EcStyle; 6] = [
        EcStyle::Linear,
        EcStyle::LinearRev,
        EcStyle::Video,
        EcStyle::VideoRev,
        EcStyle::Logarithmic,
        EcStyle::LogarithmicRev,
    ];

    /// CTF name of the style.
    pub fn as_str(&self) -> &'static str {
        match self {
            EcStyle::Linear => EC_STYLE_LINEAR,
            EcStyle::LinearRev => EC_STYLE_LINEAR_REV,
            EcStyle::Video => EC_STYLE_VIDEO,
            EcStyle::VideoRev => EC_STYLE_VIDEO_REV,
            EcStyle::Logarithmic => EC_STYLE_LOGARITHMIC,
            EcStyle::LogarithmicRev => EC_STYLE_LOGARITHMIC_REV,
        }
    }

    /// Parse a CTF style name (case insensitive).
    pub fn parse(s: &str) -> Result<Self> {
        if s.is_empty() {
            crate::bail!("Missing exposure contrast style.");
        }
        match EcStyle::ALL
            .iter()
            .find(|st| st.as_str().eq_ignore_ascii_case(s))
        {
            Some(st) => Ok(*st),
            None => crate::bail!("Unknown exposure contrast style: '{}'.", s),
        }
    }

    /// Style from the transform style and a direction.
    pub fn from_transform_style(style: ExposureContrastStyle, dir: TransformDirection) -> Self {
        let fwd = dir == TransformDirection::Forward;
        match (style, fwd) {
            (ExposureContrastStyle::Linear, true) => EcStyle::Linear,
            (ExposureContrastStyle::Linear, false) => EcStyle::LinearRev,
            (ExposureContrastStyle::Video, true) => EcStyle::Video,
            (ExposureContrastStyle::Video, false) => EcStyle::VideoRev,
            (ExposureContrastStyle::Logarithmic, true) => EcStyle::Logarithmic,
            (ExposureContrastStyle::Logarithmic, false) => EcStyle::LogarithmicRev,
        }
    }

    /// The transform style.
    pub fn transform_style(&self) -> ExposureContrastStyle {
        match self {
            EcStyle::Linear | EcStyle::LinearRev => ExposureContrastStyle::Linear,
            EcStyle::Video | EcStyle::VideoRev => ExposureContrastStyle::Video,
            EcStyle::Logarithmic | EcStyle::LogarithmicRev => ExposureContrastStyle::Logarithmic,
        }
    }

    /// Direction encoded in the style.
    pub fn direction(&self) -> TransformDirection {
        match self {
            EcStyle::Linear | EcStyle::Video | EcStyle::Logarithmic => TransformDirection::Forward,
            _ => TransformDirection::Inverse,
        }
    }

    /// The inverse style.
    pub fn inverse(&self) -> EcStyle {
        match self {
            EcStyle::Linear => EcStyle::LinearRev,
            EcStyle::LinearRev => EcStyle::Linear,
            EcStyle::Video => EcStyle::VideoRev,
            EcStyle::VideoRev => EcStyle::Video,
            EcStyle::Logarithmic => EcStyle::LogarithmicRev,
            EcStyle::LogarithmicRev => EcStyle::Logarithmic,
        }
    }
}

/// A double value that may be dynamic (port of `DynamicPropertyDoubleImpl`).
///
/// Cloning makes an independent copy (use [`DoubleProperty::handle`] to
/// share the value).
#[derive(Debug)]
pub struct DoubleProperty {
    value: SharedValue<f64>,
    dynamic: bool,
}

impl Clone for DoubleProperty {
    fn clone(&self) -> Self {
        Self {
            value: SharedValue::new(self.value.get()),
            dynamic: self.dynamic,
        }
    }
}

impl DoubleProperty {
    /// A new property.
    pub fn new(value: f64, dynamic: bool) -> Self {
        Self {
            value: SharedValue::new(value),
            dynamic,
        }
    }
    /// The current value.
    pub fn value(&self) -> f64 {
        self.value.get()
    }
    /// Set the value (seen by all the holders of the handle).
    pub fn set_value(&self, v: f64) {
        self.value.set(v);
    }
    /// True if the property is dynamic.
    pub fn is_dynamic(&self) -> bool {
        self.dynamic
    }
    /// Make the property dynamic.
    pub fn make_dynamic(&mut self) {
        self.dynamic = true;
    }
    /// Make the property static.
    pub fn make_non_dynamic(&mut self) {
        self.dynamic = false;
    }
    /// The shared value handle.
    pub fn handle(&self) -> SharedValue<f64> {
        self.value.clone()
    }
    /// Replace the shared value handle.
    pub fn set_handle(&mut self, h: SharedValue<f64>) {
        self.value = h;
    }
    /// Equality as defined by OCIO: two static properties with the same value
    /// (two dynamic properties are never considered equal).
    pub fn equals(&self, other: &DoubleProperty) -> bool {
        if std::ptr::eq(self, other) {
            return true;
        }
        !self.dynamic && !other.dynamic && self.value() == other.value()
    }
}

/// Parameters of an [`ExposureContrastOp`] (port of OCIO's
/// `ExposureContrastOpData`).
#[derive(Debug, Clone)]
pub struct ExposureContrastOpData {
    /// Style (encodes the direction).
    pub style: EcStyle,
    /// Exposure (in stops).
    pub exposure: DoubleProperty,
    /// Contrast.
    pub contrast: DoubleProperty,
    /// Gamma (multiplies the contrast).
    pub gamma: DoubleProperty,
    /// Pivot of the contrast.
    pub pivot: f64,
    /// Log exposure step (logarithmic style).
    pub log_exposure_step: f64,
    /// Log mid gray (logarithmic style).
    pub log_mid_gray: f64,
    /// Metadata.
    pub metadata: FormatMetadata,
}

impl Default for ExposureContrastOpData {
    fn default() -> Self {
        Self::new(EcStyle::Linear)
    }
}

impl PartialEq for ExposureContrastOpData {
    /// Equality as defined by OCIO (dynamic properties are never equal).
    fn eq(&self, other: &Self) -> bool {
        self.style == other.style
            && self.pivot == other.pivot
            && self.log_exposure_step == other.log_exposure_step
            && self.log_mid_gray == other.log_mid_gray
            && self.exposure.equals(&other.exposure)
            && self.contrast.equals(&other.contrast)
            && self.gamma.equals(&other.gamma)
    }
}

impl ExposureContrastOpData {
    /// Identity E/C of the given style.
    pub fn new(style: EcStyle) -> Self {
        Self {
            style,
            exposure: DoubleProperty::new(0.0, false),
            contrast: DoubleProperty::new(1.0, false),
            gamma: DoubleProperty::new(1.0, false),
            pivot: 0.18,
            log_exposure_step: LOGEXPOSURESTEP_DEFAULT,
            log_mid_gray: LOGMIDGRAY_DEFAULT,
            metadata: FormatMetadata::default(),
        }
    }

    /// Build from an [`ExposureContrastTransform`] (with new property values).
    pub fn from_transform(t: &ExposureContrastTransform) -> Self {
        Self {
            style: EcStyle::from_transform_style(t.style, t.direction),
            exposure: DoubleProperty::new(t.exposure, t.exposure_dynamic),
            contrast: DoubleProperty::new(t.contrast, t.contrast_dynamic),
            gamma: DoubleProperty::new(t.gamma, t.gamma_dynamic),
            pivot: t.pivot,
            log_exposure_step: t.log_exposure_step,
            log_mid_gray: t.log_mid_gray,
            metadata: t.metadata.clone(),
        }
    }

    /// Current exposure.
    pub fn exposure(&self) -> f64 {
        self.exposure.value()
    }
    /// Set the exposure.
    pub fn set_exposure(&self, v: f64) {
        self.exposure.set_value(v)
    }
    /// Current contrast.
    pub fn contrast(&self) -> f64 {
        self.contrast.value()
    }
    /// Set the contrast.
    pub fn set_contrast(&self, v: f64) {
        self.contrast.set_value(v)
    }
    /// Current gamma.
    pub fn gamma(&self) -> f64 {
        self.gamma.value()
    }
    /// Set the gamma.
    pub fn set_gamma(&self, v: f64) {
        self.gamma.set_value(v)
    }

    /// The `id` metadata attribute.
    pub fn id(&self) -> &str {
        self.metadata.id()
    }

    /// No validation is needed.
    pub fn validate(&self) -> Result<()> {
        Ok(())
    }

    /// True if any property is dynamic.
    pub fn is_dynamic(&self) -> bool {
        self.exposure.is_dynamic() || self.contrast.is_dynamic() || self.gamma.is_dynamic()
    }

    /// True if not dynamic and the values are the identity values.
    pub fn is_identity(&self) -> bool {
        !self.is_dynamic()
            && self.exposure() == 0.0
            && self.contrast() == 1.0
            && self.gamma() == 1.0
    }

    /// Same as [`ExposureContrastOpData::is_identity`].
    pub fn is_no_op(&self) -> bool {
        self.is_identity()
    }

    /// E/C has no channel crosstalk.
    pub fn has_channel_crosstalk(&self) -> bool {
        false
    }

    /// Direction encoded in the style.
    pub fn direction(&self) -> TransformDirection {
        self.style.direction()
    }

    /// Set the direction (inverts the style if needed).
    pub fn set_direction(&mut self, dir: TransformDirection) {
        if self.direction() != dir {
            self.style = self.style.inverse();
        }
    }

    /// Copy (with new properties) with the inverse style.
    pub fn inverse(&self) -> ExposureContrastOpData {
        let mut ec = self.clone();
        ec.style = ec.style.inverse();
        ec
    }

    /// True if `r` is the inverse of `self` (never when dynamic).
    pub fn is_inverse(&self, r: &ExposureContrastOpData) -> bool {
        if self.is_dynamic() || r.is_dynamic() {
            return false;
        }
        *r == self.inverse()
    }

    fn property(&self, ty: DynamicPropertyType) -> Option<&DoubleProperty> {
        match ty {
            DynamicPropertyType::Exposure => Some(&self.exposure),
            DynamicPropertyType::Contrast => Some(&self.contrast),
            DynamicPropertyType::Gamma => Some(&self.gamma),
            _ => None,
        }
    }

    fn property_mut(&mut self, ty: DynamicPropertyType) -> Option<&mut DoubleProperty> {
        match ty {
            DynamicPropertyType::Exposure => Some(&mut self.exposure),
            DynamicPropertyType::Contrast => Some(&mut self.contrast),
            DynamicPropertyType::Gamma => Some(&mut self.gamma),
            _ => None,
        }
    }

    /// True if the property of the given type is dynamic.
    pub fn has_dynamic_property(&self, ty: DynamicPropertyType) -> bool {
        self.property(ty).map(|p| p.is_dynamic()).unwrap_or(false)
    }

    /// The handle of a dynamic property.
    pub fn dynamic_property(&self, ty: DynamicPropertyType) -> Result<DynamicProperty> {
        let p = match self.property(ty) {
            Some(p) => p,
            None => crate::bail!("Dynamic property type not supported by ExposureContrast."),
        };
        if !p.is_dynamic() {
            crate::bail!("ExposureContrast property is not dynamic.");
        }
        Ok(match ty {
            DynamicPropertyType::Exposure => DynamicProperty::Exposure(p.handle()),
            DynamicPropertyType::Contrast => DynamicProperty::Contrast(p.handle()),
            _ => DynamicProperty::Gamma(p.handle()),
        })
    }

    /// Share the value of `prop` (the property of the same type must be
    /// dynamic).
    pub fn replace_dynamic_property(&mut self, prop: &DynamicProperty) -> Result<()> {
        let ty = prop.property_type();
        let handle = match prop.as_double() {
            Some(h) => h.clone(),
            None => crate::bail!("Dynamic property type not supported by ExposureContrast."),
        };
        let p = match self.property_mut(ty) {
            Some(p) => p,
            None => crate::bail!("Dynamic property type not supported by ExposureContrast."),
        };
        if !p.is_dynamic() {
            crate::bail!("ExposureContrast property is not dynamic.");
        }
        p.set_handle(handle);
        Ok(())
    }

    /// Make all the properties static (keeping their current values).
    pub fn remove_dynamic_properties(&mut self) {
        self.exposure.make_non_dynamic();
        self.contrast.make_non_dynamic();
        self.gamma.make_non_dynamic();
    }

    /// Cache id of the parameters (dynamic values are not included).
    pub fn cache_id(&self) -> String {
        let mut s = String::new();
        if !self.id().is_empty() {
            s.push_str(self.id());
            s.push(' ');
        }
        s.push_str(self.style.as_str());
        s.push(' ');
        if !self.exposure.is_dynamic() {
            s.push_str(&format!(
                "E: {} ",
                format_g(self.exposure(), FLOAT_DECIMALS)
            ));
        }
        if !self.contrast.is_dynamic() {
            s.push_str(&format!(
                "C: {} ",
                format_g(self.contrast(), FLOAT_DECIMALS)
            ));
        }
        if !self.gamma.is_dynamic() {
            s.push_str(&format!("G: {} ", format_g(self.gamma(), FLOAT_DECIMALS)));
        }
        s.push_str(&format!("P: {} ", format_g(self.pivot, FLOAT_DECIMALS)));
        s.push_str(&format!(
            "LES: {} ",
            format_g(self.log_exposure_step, FLOAT_DECIMALS)
        ));
        s.push_str(&format!(
            "LMG: {}",
            format_g(self.log_mid_gray, FLOAT_DECIMALS)
        ));
        s
    }
}

// ---------------------------------------------------------------------------
// CPU renderer.

/// `std::max(a, b)` (returns `a` if `b` is NaN).
#[inline]
fn cmax(a: f32, b: f32) -> f32 {
    if a < b {
        b
    } else {
        a
    }
}

#[inline]
fn cmax_f64(a: f64, b: f64) -> f64 {
    if a < b {
        b
    } else {
        a
    }
}

/// CPU renderer of the E/C op (as `GetExposureContrastCPURenderer`). It
/// holds the property handles and reads their current values on each apply.
#[derive(Debug, Clone)]
pub struct ExposureContrastRenderer {
    style: EcStyle,
    exposure: SharedValue<f64>,
    contrast: SharedValue<f64>,
    gamma: SharedValue<f64>,
    pivot: f32,
    log_exposure_step: f32,
}

impl ExposureContrastRenderer {
    /// Initialize the renderer (sharing the property values of `ec`).
    pub fn new(ec: &ExposureContrastOpData) -> Self {
        // NB: As in OCIO, the inverse log renderer does not read the log
        // exposure step and uses the default value.
        let mut log_exposure_step = 0.088f32;
        let pivot = match ec.style {
            EcStyle::Linear | EcStyle::LinearRev => cmax_f64(MIN_PIVOT, ec.pivot) as f32,
            EcStyle::Video | EcStyle::VideoRev => {
                (cmax_f64(MIN_PIVOT, ec.pivot) as f32).powf(VIDEO_OETF_POWER as f32)
            }
            EcStyle::Logarithmic | EcStyle::LogarithmicRev => {
                if ec.style == EcStyle::Logarithmic {
                    log_exposure_step = ec.log_exposure_step as f32;
                }
                let p = cmax_f64(MIN_PIVOT, ec.pivot) as f32;
                cmax_f64(
                    0.0,
                    (p as f64 / 0.18).log2() * ec.log_exposure_step + ec.log_mid_gray,
                ) as f32
            }
        };
        Self {
            style: ec.style,
            exposure: ec.exposure.handle(),
            contrast: ec.contrast.handle(),
            gamma: ec.gamma.handle(),
            pivot,
            log_exposure_step,
        }
    }

    /// Process pixels in place (alpha is not modified).
    pub fn apply(&self, pixels: &mut [Pixel]) {
        let exposure = self.exposure.get();
        let contrast = self.contrast.get();
        let gamma = self.gamma.get();
        let pivot = self.pivot;
        match self.style {
            EcStyle::Linear | EcStyle::Video => {
                let contrast_val = cmax_f64(MIN_CONTRAST, contrast * gamma) as f32;
                let mut exposure_val = 2.0f32.powf(exposure as f32);
                if self.style == EcStyle::Video {
                    exposure_val = exposure_val.powf(VIDEO_OETF_POWER as f32);
                }
                if contrast_val == 1.0 {
                    for p in pixels.iter_mut() {
                        for v in p.iter_mut().take(3) {
                            *v *= exposure_val;
                        }
                    }
                } else {
                    let exposure_over_pivot = exposure_val / pivot;
                    for p in pixels.iter_mut() {
                        for v in p.iter_mut().take(3) {
                            // Note: With std::max NaN becomes 0.
                            *v = cmax(0.0, *v * exposure_over_pivot).powf(contrast_val) * pivot;
                        }
                    }
                }
            }
            EcStyle::LinearRev | EcStyle::VideoRev => {
                let contrast_val = cmax_f64(MIN_CONTRAST, contrast * gamma) as f32;
                let inv_contrast_val = 1.0f32 / contrast_val;
                let mut e = 2.0f32.powf(exposure as f32);
                if self.style == EcStyle::VideoRev {
                    e = e.powf(VIDEO_OETF_POWER as f32);
                }
                let inv_exposure_val = 1.0f32 / e;
                if contrast_val == 1.0 {
                    for p in pixels.iter_mut() {
                        for v in p.iter_mut().take(3) {
                            *v *= inv_exposure_val;
                        }
                    }
                } else {
                    let pivot_over_exposure = pivot * inv_exposure_val;
                    let inv_pivot = 1.0f32 / pivot;
                    for p in pixels.iter_mut() {
                        for v in p.iter_mut().take(3) {
                            *v = cmax(0.0, *v * inv_pivot).powf(inv_contrast_val)
                                * pivot_over_exposure;
                        }
                    }
                }
            }
            EcStyle::Logarithmic => {
                let exposure_val = exposure as f32 * self.log_exposure_step;
                let contrast_val = cmax_f64(MIN_CONTRAST, contrast * gamma) as f32;
                let offset_val = (exposure_val - pivot) * contrast_val + pivot;
                for p in pixels.iter_mut() {
                    for v in p.iter_mut().take(3) {
                        *v = *v * contrast_val + offset_val;
                    }
                }
            }
            EcStyle::LogarithmicRev => {
                let exposure_val = exposure as f32 * self.log_exposure_step;
                let inv_contrast_val = cmax_f64(MIN_CONTRAST, 1.0 / (contrast * gamma)) as f32;
                let neg_offset_val = pivot - pivot * inv_contrast_val - exposure_val;
                for p in pixels.iter_mut() {
                    for v in p.iter_mut().take(3) {
                        *v = *v * inv_contrast_val + neg_offset_val;
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The op.

/// Exposure / contrast op (port of OCIO's `ExposureContrastOp`).
#[derive(Debug)]
pub struct ExposureContrastOp {
    data: ExposureContrastOpData,
    renderer: ExposureContrastRenderer,
}

impl Clone for ExposureContrastOp {
    /// Independent copy (the dynamic properties are not shared).
    fn clone(&self) -> Self {
        Self::new(self.data.clone())
    }
}

impl ExposureContrastOp {
    /// Create the op (the renderer shares the property values of `data`).
    pub fn new(data: ExposureContrastOpData) -> Self {
        let renderer = ExposureContrastRenderer::new(&data);
        Self { data, renderer }
    }

    /// The parameters.
    pub fn data(&self) -> &ExposureContrastOpData {
        &self.data
    }
}

impl Op for ExposureContrastOp {
    fn name(&self) -> &'static str {
        "ExposureContrast"
    }

    fn apply(&self, pixels: &mut [Pixel]) {
        self.renderer.apply(pixels);
    }

    fn is_no_op(&self) -> bool {
        self.data.is_no_op()
    }

    fn has_channel_crosstalk(&self) -> bool {
        false
    }

    fn cache_id(&self) -> String {
        format!("<ExposureContrastOp {}>", self.data.cache_id())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn combine_with(&self, next: &dyn Op, flags: OptimizationFlags) -> Option<OpVec> {
        if !flags.contains(OptimizationFlags::PAIR_IDENTITY_EXPOSURE_CONTRAST) {
            return None;
        }
        let other = next.downcast_ref::<ExposureContrastOp>()?;
        if self.data.is_inverse(&other.data) {
            // The identity replacement is an identity matrix: remove the pair.
            return Some(vec![]);
        }
        None
    }

    fn is_dynamic(&self) -> bool {
        self.data.is_dynamic()
    }

    fn dynamic_property(&self, ty: DynamicPropertyType) -> Option<DynamicProperty> {
        self.data.dynamic_property(ty).ok()
    }

    fn replace_dynamic_property(&mut self, prop: &DynamicProperty) {
        if self.data.replace_dynamic_property(prop).is_ok() {
            self.renderer = ExposureContrastRenderer::new(&self.data);
        }
    }

    fn make_non_dynamic(&self) -> Option<OpRc> {
        if !self.data.is_dynamic() {
            return None;
        }
        let mut data = self.data.clone();
        data.remove_dynamic_properties();
        Some(Arc::new(ExposureContrastOp::new(data)))
    }

    fn to_transform(&self) -> Option<Transform> {
        let d = &self.data;
        Some(Transform::ExposureContrast(ExposureContrastTransform {
            direction: d.direction(),
            style: d.style.transform_style(),
            exposure: d.exposure(),
            exposure_dynamic: d.exposure.is_dynamic(),
            contrast: d.contrast(),
            contrast_dynamic: d.contrast.is_dynamic(),
            gamma: d.gamma(),
            gamma_dynamic: d.gamma.is_dynamic(),
            pivot: d.pivot,
            log_exposure_step: d.log_exposure_step,
            log_mid_gray: d.log_mid_gray,
            metadata: d.metadata.clone(),
        }))
    }

    fn clone_box(&self) -> Box<dyn Op> {
        // Keep sharing the dynamic property values (the copy is typically
        // modified by `replace_dynamic_property`).
        Box::new(ExposureContrastOp {
            data: self.data_sharing_properties(),
            renderer: self.renderer.clone(),
        })
    }
}

impl ExposureContrastOp {
    fn data_sharing_properties(&self) -> ExposureContrastOpData {
        let mut d = self.data.clone();
        d.exposure.set_handle(self.data.exposure.handle());
        d.contrast.set_handle(self.data.contrast.handle());
        d.gamma.set_handle(self.data.gamma.handle());
        d
    }
}

// ---------------------------------------------------------------------------
// Op builders.

/// Append an E/C op built from `data` in direction `dir` (a copy of the data
/// is used: the dynamic properties are not shared with `data`).
pub fn create_exposure_contrast_op(
    ops: &mut OpVec,
    data: &ExposureContrastOpData,
    dir: TransformDirection,
) -> Result<()> {
    let ec = match dir {
        TransformDirection::Forward => data.clone(),
        TransformDirection::Inverse => data.inverse(),
    };
    ops.push(Arc::new(ExposureContrastOp::new(ec)));
    Ok(())
}

// ---------------------------------------------------------------------------
// ExposureContrastTransform.

impl ExposureContrastTransform {
    /// Equality as defined by OCIO (metadata ignored; dynamic properties are
    /// never equal).
    pub fn equals(&self, other: &ExposureContrastTransform) -> bool {
        ExposureContrastOpData::from_transform(self)
            == ExposureContrastOpData::from_transform(other)
    }
}

impl Validate for ExposureContrastTransform {
    fn validate(&self) -> Result<()> {
        ExposureContrastOpData::from_transform(self)
            .validate()
            .map_err(|e| e.prefixed("ExposureContrastTransform validation failed: "))
    }
}

impl BuildOps for ExposureContrastTransform {
    fn build_ops(
        &self,
        ops: &mut OpVec,
        _config: &Config,
        _context: &Context,
        dir: TransformDirection,
    ) -> Result<()> {
        self.validate()?;
        create_exposure_contrast_op(ops, &ExposureContrastOpData::from_transform(self), dir)
    }
}

#[cfg(test)]
mod tests;
