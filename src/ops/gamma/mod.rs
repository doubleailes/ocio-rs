//! Gamma op (port of `GammaOpData`, `GammaOp`, `GammaOpCPU`, `GammaOpUtils`
//! and the op building part of `ExponentWithLinearTransform.cpp`).
//!
//! The gamma op implements both the "basic" power functions used by
//! `ExponentTransform` (with clamp, mirror or pass-thru handling of negative
//! values) and the "moncurve" power functions with a linear segment used by
//! `ExponentWithLinearTransform` (with linear or mirror handling of negative
//! values).

use crate::config::Config;
use crate::context::Context;
use crate::error::{Error, Result};
use crate::format_metadata::FormatMetadata;
use crate::ops::matrix::float_format::format_g;
use crate::ops::range::{RangeOp, RangeOpData};
use crate::ops::{Op, OpVec, Pixel};
use crate::transforms::{
    BuildOps, ExponentTransform, ExponentWithLinearTransform, Transform, Validate,
};
use crate::types::{NegativeStyle, OptimizationFlags, TransformDirection};
use std::any::Any;
use std::sync::Arc;

const FLOAT_DECIMALS: usize = 7;

/// Styles of the gamma op (port of `GammaOpData::Style`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GammaStyle {
    /// Power, negative values clamped to 0.
    BasicFwd,
    /// Inverse of [`GammaStyle::BasicFwd`].
    BasicRev,
    /// Power, mirrored for negative values.
    BasicMirrorFwd,
    /// Inverse of [`GammaStyle::BasicMirrorFwd`].
    BasicMirrorRev,
    /// Power, negative values passed through.
    BasicPassThruFwd,
    /// Inverse of [`GammaStyle::BasicPassThruFwd`].
    BasicPassThruRev,
    /// Power with a linear segment (sRGB-like curve).
    MoncurveFwd,
    /// Inverse of [`GammaStyle::MoncurveFwd`].
    MoncurveRev,
    /// Power with a linear segment, mirrored for negative values.
    MoncurveMirrorFwd,
    /// Inverse of [`GammaStyle::MoncurveMirrorFwd`].
    MoncurveMirrorRev,
}

impl GammaStyle {
    /// All the styles.
    pub const ALL: [GammaStyle; 10] = [
        GammaStyle::BasicFwd,
        GammaStyle::BasicRev,
        GammaStyle::BasicMirrorFwd,
        GammaStyle::BasicMirrorRev,
        GammaStyle::BasicPassThruFwd,
        GammaStyle::BasicPassThruRev,
        GammaStyle::MoncurveFwd,
        GammaStyle::MoncurveRev,
        GammaStyle::MoncurveMirrorFwd,
        GammaStyle::MoncurveMirrorRev,
    ];

    /// CTF name of the style.
    pub fn as_str(&self) -> &'static str {
        match self {
            GammaStyle::BasicFwd => "basicFwd",
            GammaStyle::BasicRev => "basicRev",
            GammaStyle::BasicMirrorFwd => "basicMirrorFwd",
            GammaStyle::BasicMirrorRev => "basicMirrorRev",
            GammaStyle::BasicPassThruFwd => "basicPassThruFwd",
            GammaStyle::BasicPassThruRev => "basicPassThruRev",
            GammaStyle::MoncurveFwd => "monCurveFwd",
            GammaStyle::MoncurveRev => "monCurveRev",
            GammaStyle::MoncurveMirrorFwd => "monCurveMirrorFwd",
            GammaStyle::MoncurveMirrorRev => "monCurveMirrorRev",
        }
    }

    /// Parse a CTF style name (case insensitive).
    pub fn parse(s: &str) -> Result<Self> {
        if s.is_empty() {
            crate::bail!("Missing gamma style.");
        }
        GammaStyle::ALL
            .iter()
            .copied()
            .find(|st| st.as_str().eq_ignore_ascii_case(s))
            .ok_or_else(|| Error::msg(format!("Unknown gamma style: '{s}'.")))
    }

    /// True for the basic styles.
    pub fn is_basic(&self) -> bool {
        !self.is_moncurve()
    }

    /// True for the moncurve styles.
    pub fn is_moncurve(&self) -> bool {
        matches!(
            self,
            GammaStyle::MoncurveFwd
                | GammaStyle::MoncurveRev
                | GammaStyle::MoncurveMirrorFwd
                | GammaStyle::MoncurveMirrorRev
        )
    }

    /// Direction encoded in the style.
    pub fn direction(&self) -> TransformDirection {
        match self {
            GammaStyle::BasicFwd
            | GammaStyle::BasicMirrorFwd
            | GammaStyle::BasicPassThruFwd
            | GammaStyle::MoncurveFwd
            | GammaStyle::MoncurveMirrorFwd => TransformDirection::Forward,
            _ => TransformDirection::Inverse,
        }
    }

    /// The style of the inverse op.
    pub fn inverse(&self) -> GammaStyle {
        match self {
            GammaStyle::BasicFwd => GammaStyle::BasicRev,
            GammaStyle::BasicRev => GammaStyle::BasicFwd,
            GammaStyle::BasicMirrorFwd => GammaStyle::BasicMirrorRev,
            GammaStyle::BasicMirrorRev => GammaStyle::BasicMirrorFwd,
            GammaStyle::BasicPassThruFwd => GammaStyle::BasicPassThruRev,
            GammaStyle::BasicPassThruRev => GammaStyle::BasicPassThruFwd,
            GammaStyle::MoncurveFwd => GammaStyle::MoncurveRev,
            GammaStyle::MoncurveRev => GammaStyle::MoncurveFwd,
            GammaStyle::MoncurveMirrorFwd => GammaStyle::MoncurveMirrorRev,
            GammaStyle::MoncurveMirrorRev => GammaStyle::MoncurveMirrorFwd,
        }
    }

    /// The negative style of the style (port of `GammaOpData::ConvertStyle`).
    pub fn negative_style(&self) -> NegativeStyle {
        match self {
            GammaStyle::BasicFwd | GammaStyle::BasicRev => NegativeStyle::Clamp,
            GammaStyle::MoncurveFwd | GammaStyle::MoncurveRev => NegativeStyle::Linear,
            GammaStyle::BasicMirrorFwd
            | GammaStyle::BasicMirrorRev
            | GammaStyle::MoncurveMirrorFwd
            | GammaStyle::MoncurveMirrorRev => NegativeStyle::Mirror,
            GammaStyle::BasicPassThruFwd | GammaStyle::BasicPassThruRev => NegativeStyle::PassThru,
        }
    }

    /// Basic style from a negative style and a direction (port of
    /// `GammaOpData::ConvertStyleBasic`).
    pub fn basic(neg: NegativeStyle, dir: TransformDirection) -> Result<GammaStyle> {
        let fwd = dir == TransformDirection::Forward;
        Ok(match neg {
            NegativeStyle::Clamp => {
                if fwd {
                    GammaStyle::BasicFwd
                } else {
                    GammaStyle::BasicRev
                }
            }
            NegativeStyle::Mirror => {
                if fwd {
                    GammaStyle::BasicMirrorFwd
                } else {
                    GammaStyle::BasicMirrorRev
                }
            }
            NegativeStyle::PassThru => {
                if fwd {
                    GammaStyle::BasicPassThruFwd
                } else {
                    GammaStyle::BasicPassThruRev
                }
            }
            NegativeStyle::Linear => {
                crate::bail!("Linear negative extrapolation is not valid for basic exponent style.")
            }
        })
    }

    /// Moncurve style from a negative style and a direction (port of
    /// `GammaOpData::ConvertStyleMonCurve`).
    pub fn moncurve(neg: NegativeStyle, dir: TransformDirection) -> Result<GammaStyle> {
        let fwd = dir == TransformDirection::Forward;
        Ok(match neg {
            NegativeStyle::Linear => {
                if fwd {
                    GammaStyle::MoncurveFwd
                } else {
                    GammaStyle::MoncurveRev
                }
            }
            NegativeStyle::Mirror => {
                if fwd {
                    GammaStyle::MoncurveMirrorFwd
                } else {
                    GammaStyle::MoncurveMirrorRev
                }
            }
            NegativeStyle::PassThru => {
                crate::bail!(
                    "Pass thru negative extrapolation is not valid for MonCurve exponent style."
                )
            }
            NegativeStyle::Clamp => {
                crate::bail!(
                    "Clamp negative extrapolation is not valid for MonCurve exponent style."
                )
            }
        })
    }
}

/// Parameters of a [`GammaOp`] (port of OCIO's `GammaOpData`).
///
/// Basic styles use one parameter per channel (the gamma), moncurve styles
/// two (the gamma and the offset).
#[derive(Debug, Clone)]
pub struct GammaOpData {
    /// Style (encodes the direction).
    pub style: GammaStyle,
    /// Red parameters.
    pub red: Vec<f64>,
    /// Green parameters.
    pub green: Vec<f64>,
    /// Blue parameters.
    pub blue: Vec<f64>,
    /// Alpha parameters.
    pub alpha: Vec<f64>,
    /// Metadata.
    pub metadata: FormatMetadata,
}

impl Default for GammaOpData {
    /// Identity basic forward gamma.
    fn default() -> Self {
        let p = Self::identity_parameters(GammaStyle::BasicFwd);
        Self {
            style: GammaStyle::BasicFwd,
            red: p.clone(),
            green: p.clone(),
            blue: p.clone(),
            alpha: p,
            metadata: FormatMetadata::default(),
        }
    }
}

impl PartialEq for GammaOpData {
    /// Equality as defined by OCIO (metadata ignored).
    fn eq(&self, other: &Self) -> bool {
        self.style == other.style
            && self.red == other.red
            && self.green == other.green
            && self.blue == other.blue
            && self.alpha == other.alpha
    }
}

fn validate_params(p: &[f64], reqd_size: usize, low: &[f64], high: &[f64]) -> Result<()> {
    if p.len() != reqd_size {
        crate::bail!("GammaOp: Wrong number of parameters");
    }
    for i in 0..reqd_size {
        if p[i] < low[i] {
            crate::bail!(
                "Parameter {} is less than lower bound {}",
                format_g(p[i], 6),
                format_g(low[i], 6)
            );
        }
        if p[i] > high[i] {
            crate::bail!(
                "Parameter {} is greater than upper bound {}",
                format_g(p[i], 6),
                format_g(high[i], 6)
            );
        }
    }
    Ok(())
}

fn params_string(p: &[f64]) -> String {
    p.iter()
        .map(|v| format_g(*v, FLOAT_DECIMALS))
        .collect::<Vec<_>>()
        .join(", ")
}

impl GammaOpData {
    /// Build from a style and the per channel parameters (not validated).
    pub fn new(
        style: GammaStyle,
        red: Vec<f64>,
        green: Vec<f64>,
        blue: Vec<f64>,
        alpha: Vec<f64>,
    ) -> Self {
        Self {
            style,
            red,
            green,
            blue,
            alpha,
            metadata: FormatMetadata::default(),
        }
    }

    /// Build from an [`ExponentTransform`] (basic styles).
    pub fn from_exponent_transform(t: &ExponentTransform) -> Result<Self> {
        let style = GammaStyle::basic(t.negative_style, t.direction)?;
        let v = &t.value;
        let mut d = Self::new(style, vec![v[0]], vec![v[1]], vec![v[2]], vec![v[3]]);
        d.metadata = t.metadata.clone();
        Ok(d)
    }

    /// Build from an [`ExponentWithLinearTransform`] (moncurve styles).
    pub fn from_exponent_with_linear_transform(t: &ExponentWithLinearTransform) -> Result<Self> {
        let style = GammaStyle::moncurve(t.negative_style, t.direction)?;
        let (g, o) = (&t.gamma, &t.offset);
        let mut d = Self::new(
            style,
            vec![g[0], o[0]],
            vec![g[1], o[1]],
            vec![g[2], o[2]],
            vec![g[3], o[3]],
        );
        d.metadata = t.metadata.clone();
        Ok(d)
    }

    /// The identity parameters of a style.
    pub fn identity_parameters(style: GammaStyle) -> Vec<f64> {
        if style.is_moncurve() {
            vec![1.0, 0.0]
        } else {
            vec![1.0]
        }
    }

    /// True if the parameters are the identity parameters of the style.
    pub fn is_identity_parameters(params: &[f64], style: GammaStyle) -> bool {
        if style.is_moncurve() {
            params.len() == 2 && params[0] == 1.0 && params[1] == 0.0
        } else {
            params.len() == 1 && params[0] == 1.0
        }
    }

    /// Set the RGB parameters to `p` and alpha to the identity.
    pub fn set_params(&mut self, p: &[f64]) {
        self.red = p.to_vec();
        self.green = p.to_vec();
        self.blue = p.to_vec();
        self.alpha = Self::identity_parameters(self.style);
    }

    /// The parameters of a channel (0: red ... 3: alpha).
    pub fn params(&self, channel: usize) -> &[f64] {
        match channel {
            0 => &self.red,
            1 => &self.green,
            2 => &self.blue,
            _ => &self.alpha,
        }
    }

    /// The `id` metadata attribute.
    pub fn id(&self) -> &str {
        self.metadata.id()
    }

    /// Validate the number of parameters and their ranges.
    pub fn validate(&self) -> Result<()> {
        let (size, low, high): (usize, &[f64], &[f64]) = if self.style.is_moncurve() {
            (2, &[1.0, 0.0], &[10.0, 0.9])
        } else {
            (1, &[0.01], &[100.0])
        };
        validate_params(&self.red, size, low, high)?;
        validate_params(&self.green, size, low, high)?;
        validate_params(&self.blue, size, low, high)?;
        validate_params(&self.alpha, size, low, high)?;
        Ok(())
    }

    /// True if the alpha parameters are the identity.
    pub fn is_alpha_component_identity(&self) -> bool {
        Self::is_identity_parameters(&self.alpha, self.style)
    }

    /// True if the four channels use the same parameters.
    pub fn are_all_components_equal(&self) -> bool {
        self.red == self.green && self.red == self.blue && self.red == self.alpha
    }

    /// True if R, G and B use the same parameters and alpha is the identity.
    pub fn is_non_channel_dependent(&self) -> bool {
        self.red == self.green && self.red == self.blue && self.is_alpha_component_identity()
    }

    /// Gamma ops are channel independent.
    pub fn is_channel_independent(&self) -> bool {
        true
    }

    /// True if the parameters are the identity (the op may still clamp).
    pub fn is_identity(&self) -> bool {
        if !self.are_all_components_equal() {
            return false;
        }
        if self.style.is_moncurve() {
            self.red.len() >= 2 && self.red[0] == 1.0 && self.red[1] == 0.0
        } else {
            !self.red.is_empty() && self.red[0] == 1.0
        }
    }

    /// True if the op clamps negative values.
    pub fn is_clamping(&self) -> bool {
        matches!(self.style, GammaStyle::BasicFwd | GammaStyle::BasicRev)
    }

    /// An identity that does not clamp.
    pub fn is_no_op(&self) -> bool {
        self.is_identity() && !self.is_clamping()
    }

    /// Gamma ops have no channel crosstalk.
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

    /// Copy with the inverse style.
    pub fn inverse(&self) -> GammaOpData {
        let mut g = self.clone();
        g.style = g.style.inverse();
        g
    }

    /// True if `b` is the inverse of `self` (inverse styles, same parameters).
    pub fn is_inverse(&self, b: &GammaOpData) -> bool {
        self.style.inverse() == b.style
            && self.red == b.red
            && self.green == b.green
            && self.blue == b.blue
            && self.alpha == b.alpha
    }

    /// True if `b` can be composed with `self` (basic styles only, with
    /// compatible negative handling).
    pub fn may_compose(&self, b: &GammaOpData) -> bool {
        use GammaStyle::*;
        match self.style {
            BasicFwd | BasicRev => b.style.is_basic(),
            BasicMirrorFwd | BasicMirrorRev => matches!(
                b.style,
                BasicFwd | BasicRev | BasicMirrorFwd | BasicMirrorRev
            ),
            BasicPassThruFwd | BasicPassThruRev => {
                matches!(
                    b.style,
                    BasicFwd | BasicRev | BasicPassThruFwd | BasicPassThruRev
                )
            }
            _ => false,
        }
    }

    /// Compose `self` followed by `b` (see [`GammaOpData::may_compose`]).
    pub fn compose(&self, b: &GammaOpData) -> Result<GammaOpData> {
        use GammaStyle::*;
        if !self.may_compose(b) {
            crate::bail!("GammaOp can only be combined with some GammaOps");
        }
        let is_rev = |s: GammaStyle| matches!(s, BasicRev | BasicMirrorRev | BasicPassThruRev);
        let get = |d: &GammaOpData| -> [f64; 4] {
            let v = [d.red[0], d.green[0], d.blue[0], d.alpha[0]];
            if is_rev(d.style) {
                [1.0 / v[0], 1.0 / v[1], 1.0 / v[2], 1.0 / v[3]]
            } else {
                v
            }
        };
        let p1 = get(self);
        let p2 = get(b);
        let mut out = [p1[0] * p2[0], p1[1] * p2[1], p1[2] * p2[2], p1[3] * p2[3]];

        // Prevent small rounding errors from not making an identity.
        for v in out.iter_mut() {
            if (*v - 1.0).abs() < 1e-6 {
                *v = 1.0;
            }
        }

        // NB: This always returns a forward style.
        let (a, bs) = (self.style, b.style);
        let mut style = if matches!(a, BasicFwd | BasicRev) || matches!(bs, BasicFwd | BasicRev) {
            BasicFwd
        } else if matches!(a, BasicMirrorFwd | BasicMirrorRev) {
            BasicMirrorFwd
        } else {
            BasicPassThruFwd
        };

        // By convention, try to keep the gamma parameter > 1.
        if out[0] < 1.0 && out[1] < 1.0 && out[2] < 1.0 {
            out = [1.0 / out[0], 1.0 / out[1], 1.0 / out[2], 1.0 / out[3]];
            style = match style {
                BasicPassThruFwd => BasicPassThruRev,
                BasicMirrorFwd => BasicMirrorRev,
                _ => BasicRev,
            };
        }

        let mut res = GammaOpData::new(
            style,
            vec![out[0]],
            vec![out[1]],
            vec![out[2]],
            vec![out[3]],
        );
        res.metadata = self.metadata.clone();
        res.metadata.combine(&b.metadata);
        Ok(res)
    }

    /// The op replacing an identity (or a pair of inverse gammas): a range
    /// clamping negative values for the clamping styles, `None` for an
    /// identity matrix otherwise.
    pub fn identity_replacement(&self) -> Option<RangeOpData> {
        if self.is_clamping() {
            // Don't clamp high end.
            // TODO in OCIO: Gamma processes alpha whereas Range does not.
            RangeOpData::new(0.0, RangeOpData::EMPTY, 0.0, RangeOpData::EMPTY).ok()
        } else {
            None
        }
    }

    /// Cache id of the parameters.
    pub fn cache_id(&self) -> String {
        let mut s = String::new();
        if !self.id().is_empty() {
            s.push_str(self.id());
            s.push(' ');
        }
        s.push_str(self.style.as_str());
        s.push(' ');
        s.push_str(&format!("r:{} ", params_string(&self.red)));
        s.push_str(&format!("g:{} ", params_string(&self.green)));
        s.push_str(&format!("b:{} ", params_string(&self.blue)));
        s.push_str(&format!("a:{} ", params_string(&self.alpha)));
        s
    }
}

// ---------------------------------------------------------------------------
// Moncurve render parameters (port of `GammaOpUtils`).

/// Precomputed coefficients of a moncurve renderer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RendererParams {
    /// Power.
    pub gamma: f32,
    /// Offset.
    pub offset: f32,
    /// Break point between the linear and the power segments.
    pub break_pnt: f32,
    /// Slope of the linear segment.
    pub slope: f32,
    /// Scale.
    pub scale: f32,
}

impl Default for RendererParams {
    fn default() -> Self {
        Self {
            gamma: 1.0,
            offset: 0.0,
            break_pnt: 0.0,
            slope: 1.0,
            scale: 1.0,
        }
    }
}

const MONCURVE_EPS: f64 = 1e-6;

fn cmax_f64(a: f64, b: f64) -> f64 {
    if a < b {
        b
    } else {
        a
    }
}

/// Coefficients of the forward moncurve renderer from `[gamma, offset]`.
pub fn compute_params_fwd(p: &[f64]) -> RendererParams {
    let gamma = cmax_f64(p[0], 1.0 + MONCURVE_EPS);
    let offset = cmax_f64(p[1], MONCURVE_EPS);
    let a = (gamma - 1.0) / offset;
    let b = offset * gamma / ((gamma - 1.0) * (1.0 + offset));
    RendererParams {
        gamma: gamma as f32,
        offset: (offset / (1.0 + offset)) as f32,
        break_pnt: (offset / (gamma - 1.0)) as f32,
        slope: (a * b.powf(gamma)) as f32,
        scale: (1.0 / (1.0 + offset)) as f32,
    }
}

/// Coefficients of the reverse moncurve renderer from `[gamma, offset]`.
pub fn compute_params_rev(p: &[f64]) -> RendererParams {
    let gamma = cmax_f64(p[0], 1.0 + MONCURVE_EPS);
    let offset = cmax_f64(p[1], MONCURVE_EPS);
    let brk = (offset * gamma / ((gamma - 1.0) * (1.0 + offset))).powf(gamma);
    let a = (gamma - 1.0) / offset;
    let b = (1.0 + offset) / gamma;
    RendererParams {
        gamma: (1.0 / gamma) as f32,
        offset: offset as f32,
        break_pnt: brk as f32,
        slope: (a.powf(gamma - 1.0) * b.powf(gamma)) as f32,
        scale: (1.0 + offset) as f32,
    }
}

// ---------------------------------------------------------------------------
// CPU renderers.

/// CPU renderer chosen from the gamma style (as `GetGammaRenderer`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GammaRenderer {
    /// `pow(max(0, x), g)`.
    Basic { gamma: [f32; 4] },
    /// `sign(x) * pow(|x|, g)`.
    BasicMirror { gamma: [f32; 4] },
    /// `x > 0 ? pow(x, g) : x`.
    BasicPassThru { gamma: [f32; 4] },
    /// Forward moncurve.
    MoncurveFwd { params: [RendererParams; 4] },
    /// Reverse moncurve.
    MoncurveRev { params: [RendererParams; 4] },
    /// Forward mirrored moncurve.
    MoncurveMirrorFwd { params: [RendererParams; 4] },
    /// Reverse mirrored moncurve.
    MoncurveMirrorRev { params: [RendererParams; 4] },
}

#[inline]
fn cmax(a: f32, b: f32) -> f32 {
    if a < b {
        b
    } else {
        a
    }
}

impl GammaRenderer {
    /// Select and initialize the renderer.
    pub fn new(gamma: &GammaOpData) -> Self {
        let basic_gamma = || -> [f32; 4] {
            let fwd = gamma.direction() == TransformDirection::Forward;
            let g = |c: usize| {
                let v = gamma.params(c)[0];
                (if fwd { v } else { 1.0 / v }) as f32
            };
            [g(0), g(1), g(2), g(3)]
        };
        let fwd_params = || [0, 1, 2, 3].map(|c| compute_params_fwd(gamma.params(c)));
        let rev_params = || [0, 1, 2, 3].map(|c| compute_params_rev(gamma.params(c)));
        match gamma.style {
            GammaStyle::BasicFwd | GammaStyle::BasicRev => GammaRenderer::Basic {
                gamma: basic_gamma(),
            },
            GammaStyle::BasicMirrorFwd | GammaStyle::BasicMirrorRev => GammaRenderer::BasicMirror {
                gamma: basic_gamma(),
            },
            GammaStyle::BasicPassThruFwd | GammaStyle::BasicPassThruRev => {
                GammaRenderer::BasicPassThru {
                    gamma: basic_gamma(),
                }
            }
            GammaStyle::MoncurveFwd => GammaRenderer::MoncurveFwd {
                params: fwd_params(),
            },
            GammaStyle::MoncurveRev => GammaRenderer::MoncurveRev {
                params: rev_params(),
            },
            GammaStyle::MoncurveMirrorFwd => GammaRenderer::MoncurveMirrorFwd {
                params: fwd_params(),
            },
            GammaStyle::MoncurveMirrorRev => GammaRenderer::MoncurveMirrorRev {
                params: rev_params(),
            },
        }
    }

    /// Process pixels in place (all four channels).
    pub fn apply(&self, pixels: &mut [Pixel]) {
        match self {
            GammaRenderer::Basic { gamma } => {
                for p in pixels.iter_mut() {
                    for c in 0..4 {
                        p[c] = cmax(0.0, p[c]).powf(gamma[c]);
                    }
                }
            }
            GammaRenderer::BasicMirror { gamma } => {
                for p in pixels.iter_mut() {
                    for c in 0..4 {
                        let sign = 1.0f32.copysign(p[c]);
                        p[c] = sign * p[c].abs().powf(gamma[c]);
                    }
                }
            }
            GammaRenderer::BasicPassThru { gamma } => {
                for p in pixels.iter_mut() {
                    for c in 0..4 {
                        if p[c] > 0.0 {
                            p[c] = p[c].powf(gamma[c]);
                        }
                    }
                }
            }
            GammaRenderer::MoncurveFwd { params } => {
                for p in pixels.iter_mut() {
                    for c in 0..4 {
                        let r = &params[c];
                        let x = p[c];
                        let data = (x * r.scale + r.offset).powf(r.gamma);
                        p[c] = if x <= r.break_pnt { x * r.slope } else { data };
                    }
                }
            }
            GammaRenderer::MoncurveRev { params } => {
                for p in pixels.iter_mut() {
                    for c in 0..4 {
                        let r = &params[c];
                        let x = p[c];
                        let data = x.powf(r.gamma) * r.scale - r.offset;
                        p[c] = if x <= r.break_pnt { x * r.slope } else { data };
                    }
                }
            }
            GammaRenderer::MoncurveMirrorFwd { params } => {
                for p in pixels.iter_mut() {
                    for c in 0..4 {
                        let r = &params[c];
                        let sign = 1.0f32.copysign(p[c]);
                        let x = p[c].abs();
                        let data = (x * r.scale + r.offset).powf(r.gamma);
                        p[c] = sign * if x <= r.break_pnt { x * r.slope } else { data };
                    }
                }
            }
            GammaRenderer::MoncurveMirrorRev { params } => {
                for p in pixels.iter_mut() {
                    for c in 0..4 {
                        let r = &params[c];
                        let sign = 1.0f32.copysign(p[c]);
                        let x = p[c].abs();
                        let data = x.powf(r.gamma) * r.scale - r.offset;
                        p[c] = sign * if x <= r.break_pnt { x * r.slope } else { data };
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The op.

/// Gamma op (port of OCIO's `GammaOp`).
#[derive(Debug, Clone)]
pub struct GammaOp {
    data: GammaOpData,
    renderer: GammaRenderer,
}

impl GammaOp {
    /// Create the op (the parameters are validated).
    pub fn new(data: GammaOpData) -> Result<Self> {
        data.validate()?;
        let renderer = GammaRenderer::new(&data);
        Ok(Self { data, renderer })
    }

    /// The parameters.
    pub fn data(&self) -> &GammaOpData {
        &self.data
    }

    /// The CPU renderer.
    pub fn renderer(&self) -> &GammaRenderer {
        &self.renderer
    }
}

/// Ops replacing an identity gamma (or a pair of inverse gammas).
fn identity_replacement_ops(data: &GammaOpData) -> Option<OpVec> {
    match data.identity_replacement() {
        Some(range) => Some(vec![Arc::new(RangeOp::new(range).ok()?)]),
        None => Some(vec![]),
    }
}

impl Op for GammaOp {
    fn name(&self) -> &'static str {
        "Gamma"
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
        format!("<GammaOp {} >", self.data.cache_id())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn combine_with(&self, next: &dyn Op, flags: OptimizationFlags) -> Option<OpVec> {
        let other = next.downcast_ref::<GammaOp>()?;
        if flags.contains(OptimizationFlags::PAIR_IDENTITY_GAMMA)
            && self.data.is_inverse(&other.data)
        {
            return identity_replacement_ops(&self.data);
        }
        if flags.contains(OptimizationFlags::COMP_GAMMA) && self.data.may_compose(&other.data) {
            let res = self.data.compose(&other.data).ok()?;
            return Some(vec![Arc::new(GammaOp::new(res).ok()?)]);
        }
        None
    }

    fn simplify(&self, flags: OptimizationFlags) -> Option<OpVec> {
        // Identities that clamp are replaced by a clamping range.
        if flags.contains(OptimizationFlags::IDENTITY_GAMMA) && self.data.is_identity() {
            return identity_replacement_ops(&self.data);
        }
        None
    }

    fn to_transform(&self) -> Option<Transform> {
        let d = &self.data;
        let p0 = |c: usize| d.params(c).first().copied().unwrap_or(1.0);
        let p1 = |c: usize| d.params(c).get(1).copied().unwrap_or(0.0);
        if d.style.is_moncurve() {
            Some(Transform::ExponentWithLinear(ExponentWithLinearTransform {
                direction: d.direction(),
                gamma: [p0(0), p0(1), p0(2), p0(3)],
                offset: [p1(0), p1(1), p1(2), p1(3)],
                negative_style: d.style.negative_style(),
                metadata: d.metadata.clone(),
            }))
        } else {
            Some(Transform::Exponent(ExponentTransform {
                direction: d.direction(),
                value: [p0(0), p0(1), p0(2), p0(3)],
                negative_style: d.style.negative_style(),
                metadata: d.metadata.clone(),
            }))
        }
    }

    fn clone_box(&self) -> Box<dyn Op> {
        Box::new(self.clone())
    }
}

// ---------------------------------------------------------------------------
// Op builders.

/// Append a gamma op built from `data` in direction `dir` (combined with the
/// direction of the data style).
pub fn create_gamma_op(ops: &mut OpVec, data: &GammaOpData, dir: TransformDirection) -> Result<()> {
    let g = match dir {
        TransformDirection::Forward => data.clone(),
        TransformDirection::Inverse => data.inverse(),
    };
    ops.push(Arc::new(GammaOp::new(g)?));
    Ok(())
}

// ---------------------------------------------------------------------------
// ExponentWithLinearTransform.

impl ExponentWithLinearTransform {
    /// Equality as defined by OCIO (metadata ignored).
    pub fn equals(&self, other: &ExponentWithLinearTransform) -> bool {
        match (
            GammaOpData::from_exponent_with_linear_transform(self),
            GammaOpData::from_exponent_with_linear_transform(other),
        ) {
            (Ok(a), Ok(b)) => a == b,
            _ => self == other,
        }
    }
}

impl Validate for ExponentWithLinearTransform {
    fn validate(&self) -> Result<()> {
        GammaOpData::from_exponent_with_linear_transform(self)
            .and_then(|d| d.validate())
            .map_err(|e| e.prefixed("ExponentWithLinearTransform validation failed: "))
    }
}

impl BuildOps for ExponentWithLinearTransform {
    fn build_ops(
        &self,
        ops: &mut OpVec,
        _config: &Config,
        _context: &Context,
        dir: TransformDirection,
    ) -> Result<()> {
        self.validate()?;
        let data = GammaOpData::from_exponent_with_linear_transform(self)?;
        create_gamma_op(ops, &data, dir)
    }
}

#[cfg(test)]
mod tests;
