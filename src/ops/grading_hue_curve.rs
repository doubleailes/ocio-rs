//! Hue curve grading op (port of `GradingHueCurve.cpp`, `GradingHueCurveOpData.cpp`,
//! `GradingHueCurveOp.cpp`, `GradingHueCurveOpCPU.cpp` and
//! `GradingHueCurveTransform.cpp`).
//!
//! The RGB to HSY conversions (OCIO's `RGB_TO_HSY_*` / `HSY_*_TO_RGB` fixed
//! function renderers) used by the op are ported here as private helpers so
//! that the op does not depend on the fixed function op.

use super::grading_primary::{std_max, std_min, GradingValue};
use super::grading_rgb_curve::bspline::KnotsCoefs;
use super::grading_tone::log_lin;
use crate::config::Config;
use crate::context::Context;
use crate::dynamic_property::DynamicProperty;
use crate::error::Result;
use crate::format_metadata::FormatMetadata;
use crate::ops::{Op, OpRc, OpVec, Pixel};
use crate::transforms::grading::GradingHueCurve;
use crate::transforms::{BuildOps, GradingHueCurveTransform, Transform, Validate};
use crate::types::{
    DynamicPropertyType, GradingStyle, HsyTransformStyle, HueCurveType, OptimizationFlags,
    TransformDirection,
};
use std::any::Any;
use std::fmt;
use std::sync::Arc;

/// Validate the curves and compute their knots and coefficients.
fn precompute(v: &GradingHueCurve) -> Result<KnotsCoefs> {
    v.validate()?;
    KnotsCoefs::from_hue_curve(v)
}

// ---------------------------------------------------------------------------
// RGB <-> HSY conversions (port of `applyRGBToHSY` / `applyHSYToRGB`).

/// OCIO's `CLAMP` macro (NaN is preserved).
#[inline]
fn clamp_macro(a: f32, min: f32, max: f32) -> f32 {
    if a > max {
        max
    } else if min > a {
        min
    } else {
        a
    }
}

fn rgb_to_hsy(style: GradingStyle, px: &mut Pixel) {
    let red = px[0];
    let grn = px[1];
    let blu = px[2];

    let rgb_min = std_min(std_min(red, grn), blu);
    let rgb_max = std_max(std_max(red, grn), blu);

    let luma = 0.2126 * red + 0.7152 * grn + 0.0722 * blu;

    let rm = red - luma;
    let gm = grn - luma;
    let bm = blu - luma;

    let dist_rgb = rm.abs() + gm.abs() + bm.abs();

    let sat = match style {
        GradingStyle::Lin => {
            let sum_rgb = red + grn + blu;
            let k = 0.15f32;
            let sat_hi = dist_rgb / std_max(0.07 * dist_rgb + 1e-6, k + sum_rgb);
            let lo_gain = 5.0f32;
            let sat_lo = dist_rgb * lo_gain;
            let max_lum = 0.01f32;
            let min_lum = max_lum * 0.1;
            let alpha = clamp_macro((luma - min_lum) / (max_lum - min_lum), 0.0, 1.0);
            (sat_lo + alpha * (sat_hi - sat_lo)) * 1.4
        }
        GradingStyle::Log => dist_rgb * 4.0,
        GradingStyle::Video => dist_rgb * 1.25,
    };

    // NB: Unlike typical HSV, HSY maps magenta rather than red to a hue of zero.
    let mut hue = 0.0f32;
    if rgb_min != rgb_max {
        let delta = rgb_max - rgb_min;
        hue = if red == rgb_max {
            1.0 + (grn - blu) / delta
        } else if grn == rgb_max {
            3.0 + (blu - red) / delta
        } else {
            5.0 + (red - grn) / delta
        };
        #[allow(clippy::excessive_precision)]
        {
            hue *= 0.16666666666666666;
        }
    }

    px[0] = hue;
    px[1] = sat;
    px[2] = luma;
}

fn hsy_to_rgb(style: GradingStyle, px: &mut Pixel) {
    // Make magenta 0 hue, rather than red.
    let mut hue = px[0] - 1.0 / 6.0;
    let mut sat = px[1];
    let luma = px[2];

    // Rotate hue 180 deg. for negative luma values.
    if luma < 0.0 {
        hue += 0.5;
    }
    hue = (hue - hue.floor()) * 6.0;

    let mut red = clamp_macro((hue - 3.0).abs() - 1.0, 0.0, 1.0);
    let mut grn = clamp_macro(2.0 - (hue - 2.0).abs(), 0.0, 1.0);
    let mut blu = clamp_macro(2.0 - (hue - 4.0).abs(), 0.0, 1.0);

    let curr_y = 0.2126 * red + 0.7152 * grn + 0.0722 * blu;
    red *= luma / curr_y;
    grn *= luma / curr_y;
    blu *= luma / curr_y;

    let dist_rgb = (red - luma).abs() + (grn - luma).abs() + (blu - luma).abs();

    let gain_s = match style {
        GradingStyle::Lin => {
            let sum_rgb = red + grn + blu;

            let k = 0.15f32;
            let lo_gain = 5.0f32;

            sat /= 1.4;
            let mut tmp = -sat * sum_rgb + sat * 3.0 * luma + dist_rgb;
            // Don't allow tmp to go negative, which would cause a negative gainS.
            tmp = std_max(1e-6, tmp);

            let mut s1 = sat * (k + 3.0 * luma) / tmp;
            // Prevent gainS from becoming too extreme.
            s1 = std_min(s1, 50.0);

            let s0 = sat / std_max(1e-10, dist_rgb * lo_gain);

            let max_lum = 0.01f32;
            let min_lum = max_lum * 0.1;
            let alpha = clamp_macro((luma - min_lum) / (max_lum - min_lum), 0.0, 1.0);

            if alpha == 1.0 {
                s1
            } else if alpha == 0.0 {
                s0
            } else {
                let a = dist_rgb * lo_gain * (1.0 - alpha) * (sum_rgb - 3.0 * luma);
                let b = dist_rgb * lo_gain * (1.0 - alpha) * (k + 3.0 * luma) + dist_rgb * alpha
                    - sat * (sum_rgb - 3.0 * luma);
                let c = -sat * (k + 3.0 * luma);
                let discrim = (b * b - 4.0 * a * c).sqrt();
                let denom = -discrim - b;
                let g = (2.0 * c) / denom;
                if g >= 0.0 {
                    g
                } else {
                    (2.0 * c) / (denom + discrim * 2.0)
                }
            }
        }
        GradingStyle::Log => sat / std_max(1e-10, dist_rgb * 4.0),
        GradingStyle::Video => sat / std_max(1e-10, dist_rgb * 1.25),
    };

    px[0] = luma + gain_s * (red - luma);
    px[1] = luma + gain_s * (grn - luma);
    px[2] = luma + gain_s * (blu - luma);
}

// ---------------------------------------------------------------------------
// The op

/// Hue curve grading op (port of `GradingHueCurveOp` / `GradingHueCurveOpData`).
#[derive(Debug, Clone)]
pub struct GradingHueCurveOp {
    style: GradingStyle,
    direction: TransformDirection,
    rgb_to_hsy: HsyTransformStyle,
    value: GradingValue<GradingHueCurve, KnotsCoefs>,
    metadata: FormatMetadata,
}

impl GradingHueCurveOp {
    /// Create the op. The curves are validated and fitted (an error is
    /// returned if they need too many knots).
    pub fn new(
        style: GradingStyle,
        value: GradingHueCurve,
        direction: TransformDirection,
        rgb_to_hsy: HsyTransformStyle,
        dynamic: bool,
    ) -> Result<Self> {
        let kc = precompute(&value)?;
        Ok(Self {
            style,
            direction,
            rgb_to_hsy,
            value: GradingValue::new(value, kc, dynamic),
            metadata: FormatMetadata::default(),
        })
    }

    /// Identity op of the style.
    pub fn identity(style: GradingStyle) -> Self {
        let value = GradingHueCurve::new(style);
        let kc = KnotsCoefs::from_hue_curve(&value).unwrap_or_else(|_| {
            let mut kc = KnotsCoefs::new(8);
            kc.local_bypass = true;
            kc
        });
        Self {
            style,
            direction: TransformDirection::Forward,
            rgb_to_hsy: HsyTransformStyle::Hsy1,
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
    pub fn rgb_to_hsy(&self) -> HsyTransformStyle {
        self.rgb_to_hsy
    }
    pub fn metadata(&self) -> &FormatMetadata {
        &self.metadata
    }
    pub fn set_metadata(&mut self, metadata: FormatMetadata) {
        self.metadata = metadata;
    }

    fn state(&self) -> Arc<(GradingHueCurve, KnotsCoefs)> {
        self.value.current(precompute)
    }

    /// The current value (of the dynamic property if dynamic).
    pub fn value(&self) -> GradingHueCurve {
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
    pub fn is_inverse(&self, other: &GradingHueCurveOp) -> bool {
        if self.is_dynamic() || other.is_dynamic() {
            return false;
        }
        self.style == other.style
            && (self.style != GradingStyle::Lin || self.rgb_to_hsy == other.rgb_to_hsy)
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
        if self.rgb_to_hsy != HsyTransformStyle::Hsy1 {
            s.push_str(" bypassRGBToHSY ");
        }
        if !self.is_dynamic() {
            s.push_str(&format!("{:.7}", self.state().0));
        }
        s
    }
}

/// Create a hue curve op (port of `CreateGradingHueCurveOp`): the op is
/// inverted if `direction` is inverse.
pub fn create_grading_hue_curve_op(
    ops: &mut OpVec,
    style: GradingStyle,
    value: &GradingHueCurve,
    op_direction: TransformDirection,
    rgb_to_hsy: HsyTransformStyle,
    dynamic: bool,
    direction: TransformDirection,
) -> Result<()> {
    let op = GradingHueCurveOp::new(
        style,
        value.clone(),
        op_direction.combine(direction),
        rgb_to_hsy,
        dynamic,
    )?;
    ops.push(Arc::new(op));
    Ok(())
}

const HUE_HUE: usize = HueCurveType::HueHue as usize;
const HUE_SAT: usize = HueCurveType::HueSat as usize;
const HUE_LUM: usize = HueCurveType::HueLum as usize;
const LUM_SAT: usize = HueCurveType::LumSat as usize;
const SAT_SAT: usize = HueCurveType::SatSat as usize;
const LUM_LUM: usize = HueCurveType::LumLum as usize;
const SAT_LUM: usize = HueCurveType::SatLum as usize;
const HUE_FX: usize = HueCurveType::HueFx as usize;

impl GradingHueCurveOp {
    fn apply_fwd(&self, kc: &KnotsCoefs, out: &mut Pixel) {
        let hsy = self.rgb_to_hsy != HsyTransformStyle::None;
        let lin = self.style == GradingStyle::Lin;
        let is_log = self.style == GradingStyle::Log;

        if hsy {
            rgb_to_hsy(self.style, out);
        }
        if lin {
            out[2] = log_lin::lin_log(out[2]);
        }

        // HUE-SAT
        let hue_sat_gain = std_max(0.0, kc.eval_curve(HUE_SAT, out[0], 1.0));
        // HUE-LUM
        let mut hue_lum_gain = std_max(0.0, kc.eval_curve(HUE_LUM, out[0], 1.0));
        // HUE-HUE
        out[0] = kc.eval_curve(HUE_HUE, out[0], out[0]);
        // SAT-SAT
        out[1] = std_max(0.0, kc.eval_curve(SAT_SAT, out[1], out[1]));
        // LUM-SAT
        let lum_sat_gain = std_max(0.0, kc.eval_curve(LUM_SAT, out[2], 1.0));

        // Apply sat gain.
        let sat_gain = lum_sat_gain * hue_sat_gain;
        out[1] *= sat_gain;

        // SAT-LUM
        let sat_lum_gain = std_max(0.0, kc.eval_curve(SAT_LUM, out[1], 1.0));
        // LUM-LUM
        out[2] = kc.eval_curve(LUM_LUM, out[2], out[2]);

        if lin {
            out[2] = log_lin::log_lin(out[2]);
        }

        // Limit hue-lum gain at low sat, since the hue is more noisy, and when sat is 0
        // the hue becomes unknown (and is not invertible).
        hue_lum_gain = 1.0 - (1.0 - hue_lum_gain) * std_min(out[1], 1.0);

        // Apply lum gain.
        out[2] = if is_log {
            out[2] + (hue_lum_gain + sat_lum_gain - 2.0) * 0.1
        } else {
            out[2] * hue_lum_gain * sat_lum_gain
        };

        // HUE-FX
        out[0] -= out[0].floor(); // wrap to [0,1)
        out[0] += kc.eval_curve(HUE_FX, out[0], 0.0);

        if hsy {
            hsy_to_rgb(self.style, out);
        }
    }

    fn apply_rev(&self, kc: &KnotsCoefs, out: &mut Pixel) {
        let hsy = self.rgb_to_hsy != HsyTransformStyle::None;
        let lin = self.style == GradingStyle::Lin;
        let is_log = self.style == GradingStyle::Log;

        if hsy {
            rgb_to_hsy(self.style, out);
        }

        // Invert HUE-FX.
        out[0] = kc.eval_curve_rev_hue(HUE_FX, out[0]);
        // Invert HUE-HUE.
        out[0] = kc.eval_curve_rev_hue(HUE_HUE, out[0]);

        // Use the inverted hue to calculate the HUE-SAT & HUE-LUM gains.
        out[0] -= out[0].floor(); // wrap to [0,1)
        let hue_sat_gain = std_max(0.0, kc.eval_curve(HUE_SAT, out[0], 1.0));
        let mut hue_lum_gain = std_max(0.0, kc.eval_curve(HUE_LUM, out[0], 1.0));

        // Use the output sat to calculate the SAT-LUM gain.
        out[1] = std_max(0.0, out[1]); // guard against negative saturation
        let sat_lum_gain = std_max(0.0, kc.eval_curve(SAT_LUM, out[1], 1.0));

        hue_lum_gain = 1.0 - (1.0 - hue_lum_gain) * std_min(out[1], 1.0);

        // Invert the lum gain.
        let lum_gain = hue_lum_gain * sat_lum_gain;
        out[2] = if is_log {
            out[2] - (hue_lum_gain + sat_lum_gain - 2.0) * 0.1
        } else {
            out[2] / std_max(0.01, lum_gain)
        };

        if lin {
            out[2] = log_lin::lin_log(out[2]);
        }

        // Invert LUM-LUM.
        out[2] = kc.eval_curve_rev(LUM_LUM, out[2]);

        // Use it to calc the LUM-SAT gain.
        let lum_sat_gain = std_max(0.0, kc.eval_curve(LUM_SAT, out[2], 1.0));

        if lin {
            out[2] = log_lin::log_lin(out[2]);
        }

        // Invert the sat gain.
        let sat_gain = lum_sat_gain * hue_sat_gain;
        out[1] /= std_max(0.01, sat_gain);

        // Invert SAT-SAT.
        out[1] = std_max(0.0, kc.eval_curve_rev(SAT_SAT, out[1]));

        if hsy {
            hsy_to_rgb(self.style, out);
        }
    }
}

impl Op for GradingHueCurveOp {
    fn name(&self) -> &'static str {
        "GradingHueCurve"
    }

    fn apply(&self, pixels: &mut [Pixel]) {
        let state = self.state();
        let (v, kc) = (&state.0, &state.1);

        if v.draw_curve_only {
            // In drawCurveOnly mode, only evaluate the HueSat curve, with no RGB-to-HSY or
            // LogLin (the direction and the local bypass are ignored).
            for out in pixels.iter_mut() {
                for o in out.iter_mut().take(3) {
                    *o = kc.eval_curve(HUE_SAT, *o, 1.0);
                }
            }
            return;
        }

        if kc.local_bypass {
            return;
        }
        match self.direction {
            TransformDirection::Forward => pixels.iter_mut().for_each(|p| self.apply_fwd(kc, p)),
            TransformDirection::Inverse => pixels.iter_mut().for_each(|p| self.apply_rev(kc, p)),
        }
    }

    fn is_no_op(&self) -> bool {
        self.is_identity()
    }

    fn is_identity(&self) -> bool {
        !self.is_dynamic() && self.state().0.is_identity()
    }

    fn has_channel_crosstalk(&self) -> bool {
        true
    }

    fn cache_id(&self) -> String {
        format!("<GradingHueCurveOp {}>", self.data_cache_id())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn combine_with(&self, next: &dyn Op, flags: OptimizationFlags) -> Option<OpVec> {
        if !flags.contains(OptimizationFlags::PAIR_IDENTITY_GRADING) {
            return None;
        }
        let other = next.downcast_ref::<GradingHueCurveOp>()?;
        self.is_inverse(other).then(OpVec::new)
    }

    fn is_dynamic(&self) -> bool {
        self.value.is_dynamic()
    }

    fn dynamic_property(&self, ty: DynamicPropertyType) -> Option<DynamicProperty> {
        if ty != DynamicPropertyType::GradingHueCurve {
            return None;
        }
        self.value
            .property()
            .map(|p| DynamicProperty::GradingHueCurve(p.clone()))
    }

    fn replace_dynamic_property(&mut self, prop: &DynamicProperty) {
        if let Some(p) = prop.as_grading_hue_curve() {
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
        Some(Transform::GradingHueCurve(GradingHueCurveTransform {
            direction: self.direction,
            style: self.style,
            value: self.value(),
            rgb_to_hsy: self.rgb_to_hsy,
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

impl GradingHueCurveTransform {
    /// Change the style (curves are reset to the defaults of the new style).
    pub fn set_style(&mut self, style: GradingStyle) {
        if style != self.style {
            self.style = style;
            self.value = GradingHueCurve::new(style);
        }
    }

    /// Set the curves (they are validated first).
    pub fn set_value(&mut self, value: GradingHueCurve) -> Result<()> {
        precompute(&value)?;
        self.value = value;
        Ok(())
    }

    /// Slope of a control point of a curve.
    pub fn slope(&self, c: HueCurveType, index: usize) -> Result<f32> {
        self.value.curve(c).try_slope(index)
    }

    /// Set the slope of a control point of a curve.
    pub fn set_slope(&mut self, c: HueCurveType, index: usize, slope: f32) -> Result<()> {
        let mut value = self.value.clone();
        value.curve_mut(c).try_set_slope(index, slope)?;
        self.set_value(value)
    }

    /// True if the slopes of a curve are the default ones.
    pub fn slopes_are_default(&self, c: HueCurveType) -> bool {
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

impl Validate for GradingHueCurveTransform {
    fn validate(&self) -> Result<()> {
        self.value
            .validate()
            .map_err(|e| e.prefixed("GradingHueCurveTransform validation failed: "))
    }
}

impl BuildOps for GradingHueCurveTransform {
    fn build_ops(
        &self,
        ops: &mut OpVec,
        _config: &Config,
        _context: &Context,
        dir: TransformDirection,
    ) -> Result<()> {
        let mut op = GradingHueCurveOp::new(
            self.style,
            self.value.clone(),
            self.direction.combine(dir),
            self.rgb_to_hsy,
            self.dynamic,
        )?;
        op.set_metadata(self.metadata.clone());
        ops.push(Arc::new(op));
        Ok(())
    }
}

impl fmt::Display for GradingHueCurveTransform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "<GradingHueCurveTransform direction={}, style={}, values={}",
            self.direction.as_str(),
            self.style.as_str(),
            self.value
        )?;
        if self.rgb_to_hsy == HsyTransformStyle::None {
            f.write_str(", hsy_transform=none")?;
        }
        if self.dynamic {
            f.write_str(", dynamic")?;
        }
        f.write_str(">")
    }
}

#[cfg(test)]
mod tests;
