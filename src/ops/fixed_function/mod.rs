//! Fixed function op: hard-coded color math such as the ACES red modifier,
//! glow, gamut compression, the ACES 2 output transform, HSV / HSY, CIE
//! conversions and PQ / HLG style curves (port of `FixedFunctionOpData.cpp`,
//! `FixedFunctionOp.cpp`, `FixedFunctionOpCPU.cpp`, the `ACES2` folder and
//! `FixedFunctionTransform.cpp`).
//!
//! Numerical constants are kept exactly as written in OCIO (hence the
//! precision lints are disabled).

#![allow(clippy::excessive_precision, clippy::approx_constant)]

pub mod aces2;
mod cpu;

use crate::config::Config;
use crate::context::Context;
use crate::error::{Error, Result};
use crate::format_metadata::FormatMetadata;
use crate::ops::{Op, OpVec, Pixel};
use crate::transforms::{BuildOps, FixedFunctionTransform, Transform, Validate};
use crate::types::{FixedFunctionStyle, OptimizationFlags, TransformDirection};
use cpu::RendererRc;
use std::any::Any;
use std::fmt;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Number formatting (C++ ostream compatible)

/// Format like a C++ `std::ostream` using `precision(prec)` and the default
/// float field (i.e. `printf("%.<prec>g")`).
pub(crate) fn format_g(v: f64, prec: usize) -> String {
    if v.is_nan() {
        return if v.is_sign_negative() {
            "-nan".into()
        } else {
            "nan".into()
        };
    }
    if v.is_infinite() {
        return if v < 0.0 { "-inf".into() } else { "inf".into() };
    }
    let prec = prec.max(1);
    if v == 0.0 {
        return if v.is_sign_negative() {
            "-0".into()
        } else {
            "0".into()
        };
    }

    // Round to `prec` significant digits to find the decimal exponent.
    let sci = format!("{:.*e}", prec - 1, v);
    let (mantissa, exp) = match sci.split_once('e') {
        Some((m, e)) => (m.to_string(), e.parse::<i32>().unwrap_or(0)),
        None => (sci.clone(), 0),
    };

    fn strip_zeros(s: &str) -> String {
        if s.contains('.') {
            let t = s.trim_end_matches('0');
            t.trim_end_matches('.').to_string()
        } else {
            s.to_string()
        }
    }

    if exp < -4 || exp >= prec as i32 {
        let m = strip_zeros(&mantissa);
        let sign = if exp < 0 { '-' } else { '+' };
        format!("{m}e{sign}{:02}", exp.abs())
    } else {
        let decimals = (prec as i32 - 1 - exp).max(0) as usize;
        strip_zeros(&format!("{:.*}", decimals, v))
    }
}

// ---------------------------------------------------------------------------
// Styles

/// Internal fixed function styles: a [`FixedFunctionStyle`] combined with a
/// direction (port of `FixedFunctionOpData::Style`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FixedFunctionOpStyle {
    /// Red modifier (ACES 0.3/0.7).
    AcesRedMod03Fwd,
    /// Red modifier inverse (ACES 0.3/0.7).
    AcesRedMod03Inv,
    /// Red modifier (ACES 1.0).
    AcesRedMod10Fwd,
    /// Red modifier inverse (ACES 1.0).
    AcesRedMod10Inv,
    /// Glow function (ACES 0.3/0.7).
    AcesGlow03Fwd,
    /// Glow function inverse (ACES 0.3/0.7).
    AcesGlow03Inv,
    /// Glow function (ACES 1.0).
    AcesGlow10Fwd,
    /// Glow function inverse (ACES 1.0).
    AcesGlow10Inv,
    /// Dark to dim surround correction (ACES 1.0).
    AcesDarkToDim10Fwd,
    /// Dim to dark surround correction (ACES 1.0).
    AcesDarkToDim10Inv,
    /// Parametric gamut compression (ACES 1.3).
    AcesGamutComp13Fwd,
    /// Parametric gamut compression inverse (ACES 1.3).
    AcesGamutComp13Inv,
    /// Rec.2100 surround correction (one gamma parameter).
    Rec2100SurroundFwd,
    /// Rec.2100 surround correction inverse (one gamma parameter).
    Rec2100SurroundInv,
    /// Classic RGB to HSV.
    RgbToHsv,
    /// Classic HSV to RGB.
    HsvToRgb,
    /// CIE XYZ to 1931 xy chromaticity coordinates.
    XyzToXyy,
    /// Inverse of `XyzToXyy`.
    XyyToXyz,
    /// CIE XYZ to 1976 u'v' chromaticity coordinates.
    XyzToUvy,
    /// Inverse of `XyzToUvy`.
    UvyToXyz,
    /// CIE XYZ to 1976 CIELUV (D65 white).
    XyzToLuv,
    /// Inverse of `XyzToLuv`.
    LuvToXyz,
    /// Linear to Perceptual Quantizer curve.
    LinToPq,
    /// Inverse of `LinToPq`.
    PqToLin,
    /// Curve with gamma and log segments (10 parameters).
    LinToGammaLog,
    /// Inverse of `LinToGammaLog`.
    GammaLogToLin,
    /// Curve with two log and one linear segments (13 parameters).
    LinToDoubleLog,
    /// Inverse of `LinToDoubleLog`.
    DoubleLogToLin,
    /// ACES 2 output transform.
    AcesOutputTransform20Fwd,
    /// ACES 2 output transform (inverse).
    AcesOutputTransform20Inv,
    /// ACES 2 RGB to JMh.
    AcesRgbToJmh20,
    /// ACES 2 JMh to RGB.
    AcesJmhToRgb20,
    /// ACES 2 RGB to HMJ (h/360, M/200, J/100).
    AcesRgbToHmj20,
    /// ACES 2 HMJ to RGB.
    AcesHmjToRgb20,
    /// ACES 2 tonescale and chroma compression.
    AcesTonescaleCompress20Fwd,
    /// ACES 2 tonescale and chroma compression (inverse).
    AcesTonescaleCompress20Inv,
    /// ACES 2 gamut compression.
    AcesGamutCompress20Fwd,
    /// ACES 2 gamut compression (inverse).
    AcesGamutCompress20Inv,
    /// RGB to HSY for linear spaces.
    RgbToHsyLin,
    /// RGB to HSY for log spaces.
    RgbToHsyLog,
    /// RGB to HSY for video spaces.
    RgbToHsyVid,
    /// HSY to RGB for linear spaces.
    HsyLinToRgb,
    /// HSY to RGB for log spaces.
    HsyLogToRgb,
    /// HSY to RGB for video spaces.
    HsyVidToRgb,
}

use FixedFunctionOpStyle as S;

/// All the internal styles with their CTF names (as used by the CLF / CTF
/// readers and writers).
const STYLE_NAMES: &[(FixedFunctionOpStyle, &str)] = &[
    (S::AcesRedMod03Fwd, "RedMod03Fwd"),
    (S::AcesRedMod03Inv, "RedMod03Rev"),
    (S::AcesRedMod10Fwd, "RedMod10Fwd"),
    (S::AcesRedMod10Inv, "RedMod10Rev"),
    (S::AcesGlow03Fwd, "Glow03Fwd"),
    (S::AcesGlow03Inv, "Glow03Rev"),
    (S::AcesGlow10Fwd, "Glow10Fwd"),
    (S::AcesGlow10Inv, "Glow10Rev"),
    (S::AcesDarkToDim10Fwd, "DarkToDim10"),
    (S::AcesDarkToDim10Inv, "DimToDark10"),
    (S::AcesGamutComp13Fwd, "GamutComp13Fwd"),
    (S::AcesGamutComp13Inv, "GamutComp13Rev"),
    (S::AcesOutputTransform20Fwd, "ACESOutputTransform20Fwd"),
    (S::AcesOutputTransform20Inv, "ACESOutputTransform20Inv"),
    (S::AcesRgbToJmh20, "RGB_TO_JMh_20"),
    (S::AcesJmhToRgb20, "JMh_TO_RGB_20"),
    (S::AcesRgbToHmj20, "RGB_TO_HMJ_20"),
    (S::AcesHmjToRgb20, "HMJ_TO_RGB_20"),
    (S::AcesTonescaleCompress20Fwd, "ToneScaleCompress20Fwd"),
    (S::AcesTonescaleCompress20Inv, "ToneScaleCompress20Inv"),
    (S::AcesGamutCompress20Fwd, "GamutCompress20Fwd"),
    (S::AcesGamutCompress20Inv, "GamutCompress20Inv"),
    (S::Rec2100SurroundFwd, "Rec2100SurroundFwd"),
    (S::Rec2100SurroundInv, "Rec2100SurroundRev"),
    (S::RgbToHsv, "RGB_TO_HSV"),
    (S::HsvToRgb, "HSV_TO_RGB"),
    (S::XyzToXyy, "XYZ_TO_xyY"),
    (S::XyyToXyz, "xyY_TO_XYZ"),
    (S::XyzToUvy, "XYZ_TO_uvY"),
    (S::UvyToXyz, "uvY_TO_XYZ"),
    (S::XyzToLuv, "XYZ_TO_LUV"),
    (S::LuvToXyz, "LUV_TO_XYZ"),
    (S::LinToPq, "Lin_TO_PQ"),
    (S::PqToLin, "PQ_TO_Lin"),
    (S::LinToGammaLog, "Lin_TO_GammaLog"),
    (S::GammaLogToLin, "GammaLog_TO_Lin"),
    (S::LinToDoubleLog, "Lin_TO_DoubleLog"),
    (S::DoubleLogToLin, "DoubleLog_TO_Lin"),
    (S::RgbToHsyLin, "RGB_TO_HSY_LIN"),
    (S::RgbToHsyLog, "RGB_TO_HSY_LOG"),
    (S::RgbToHsyVid, "RGB_TO_HSY_VID"),
    (S::HsyLinToRgb, "HSY_LIN_TO_RGB"),
    (S::HsyLogToRgb, "HSY_LOG_TO_RGB"),
    (S::HsyVidToRgb, "HSY_VID_TO_RGB"),
];

impl FixedFunctionOpStyle {
    /// Every internal style.
    pub fn all() -> impl Iterator<Item = FixedFunctionOpStyle> {
        STYLE_NAMES.iter().map(|(s, _)| *s)
    }

    /// The CTF attribute name of the style (port of `ConvertStyleToString`
    /// with `detailed == false`).
    pub fn as_str(self) -> &'static str {
        STYLE_NAMES
            .iter()
            .find(|(s, _)| *s == self)
            .map(|(_, n)| *n)
            .unwrap_or("")
    }

    /// A more verbose human readable name (port of `ConvertStyleToString`
    /// with `detailed == true`).
    pub fn detailed_str(self) -> &'static str {
        match self {
            S::AcesRedMod03Fwd => "ACES_RedMod03 (Forward)",
            S::AcesRedMod03Inv => "ACES_RedMod03 (Inverse)",
            S::AcesRedMod10Fwd => "ACES_RedMod10 (Forward)",
            S::AcesRedMod10Inv => "ACES_RedMod10 (Inverse)",
            S::AcesGlow03Fwd => "ACES_Glow03 (Forward)",
            S::AcesGlow03Inv => "ACES_Glow03 (Inverse)",
            S::AcesGlow10Fwd => "ACES_Glow10 (Forward)",
            S::AcesGlow10Inv => "ACES_Glow10 (Inverse)",
            S::AcesDarkToDim10Fwd => "ACES_DarkToDim10 (Forward)",
            S::AcesDarkToDim10Inv => "ACES_DarkToDim10 (Inverse)",
            S::AcesGamutComp13Fwd => "ACES_GamutComp13 (Forward)",
            S::AcesGamutComp13Inv => "ACES_GamutComp13 (Inverse)",
            S::AcesOutputTransform20Fwd => "ACES_OutputTransform20 (Forward)",
            S::AcesOutputTransform20Inv => "ACES_OutputTransform20 (Inverse)",
            S::AcesTonescaleCompress20Fwd => "ACES_ToneScaleCompress20 (Forward)",
            S::AcesTonescaleCompress20Inv => "ACES_ToneScaleCompress20 (Inverse)",
            S::AcesGamutCompress20Fwd => "ACES_GamutCompress20 (Forward)",
            S::AcesGamutCompress20Inv => "ACES_GamutCompress20 (Inverse)",
            S::Rec2100SurroundFwd => "REC2100_Surround (Forward)",
            S::Rec2100SurroundInv => "REC2100_Surround (Inverse)",
            other => other.as_str(),
        }
    }

    /// Parse a CTF attribute name, case insensitive (port of `GetStyle`).
    /// `"Surround"` is accepted as the old name of `Rec2100SurroundFwd`.
    pub fn from_name(name: &str) -> Result<Self> {
        if !name.is_empty() {
            if name.eq_ignore_ascii_case("Surround") {
                return Ok(S::Rec2100SurroundFwd);
            }
            if let Some((s, _)) = STYLE_NAMES
                .iter()
                .find(|(_, n)| n.eq_ignore_ascii_case(name))
            {
                return Ok(*s);
            }
        }
        Err(Error::msg(format!("Unknown FixedFunction style: {name}")))
    }

    /// Combine a transform style and a direction (port of the first
    /// `ConvertStyle`). Fails for the unimplemented ACES gamut map styles.
    pub fn from_transform_style(
        style: FixedFunctionStyle,
        dir: TransformDirection,
    ) -> Result<Self> {
        use FixedFunctionStyle as T;
        let fwd = dir == TransformDirection::Forward;
        let pick = |f: S, i: S| if fwd { f } else { i };
        Ok(match style {
            T::AcesRedMod03 => pick(S::AcesRedMod03Fwd, S::AcesRedMod03Inv),
            T::AcesRedMod10 => pick(S::AcesRedMod10Fwd, S::AcesRedMod10Inv),
            T::AcesGlow03 => pick(S::AcesGlow03Fwd, S::AcesGlow03Inv),
            T::AcesGlow10 => pick(S::AcesGlow10Fwd, S::AcesGlow10Inv),
            T::AcesDarkToDim10 => pick(S::AcesDarkToDim10Fwd, S::AcesDarkToDim10Inv),
            T::AcesGamutComp13 => pick(S::AcesGamutComp13Fwd, S::AcesGamutComp13Inv),
            T::AcesOutputTransform20 => {
                pick(S::AcesOutputTransform20Fwd, S::AcesOutputTransform20Inv)
            }
            T::AcesRgbToJmh20 => pick(S::AcesRgbToJmh20, S::AcesJmhToRgb20),
            T::AcesRgbToHmj20 => pick(S::AcesRgbToHmj20, S::AcesHmjToRgb20),
            T::AcesTonescaleCompress20 => {
                pick(S::AcesTonescaleCompress20Fwd, S::AcesTonescaleCompress20Inv)
            }
            T::AcesGamutCompress20 => pick(S::AcesGamutCompress20Fwd, S::AcesGamutCompress20Inv),
            T::Rec2100Surround => pick(S::Rec2100SurroundFwd, S::Rec2100SurroundInv),
            // Note: as in OCIO, the direction is ignored for the following
            // styles (the direction is applied afterwards by inverting).
            T::RgbToHsv => S::RgbToHsv,
            T::XyzToXyy => S::XyzToXyy,
            T::XyzToUvy => S::XyzToUvy,
            T::XyzToLuv => S::XyzToLuv,
            T::RgbToHsyLin => pick(S::RgbToHsyLin, S::HsyLinToRgb),
            T::RgbToHsyLog => pick(S::RgbToHsyLog, S::HsyLogToRgb),
            T::RgbToHsyVid => pick(S::RgbToHsyVid, S::HsyVidToRgb),
            T::AcesGamutMap02 | T::AcesGamutMap07 => {
                return Err(Error::msg(
                    "Unimplemented fixed function types: \
                     FIXED_FUNCTION_ACES_GAMUTMAP_02, \
                     FIXED_FUNCTION_ACES_GAMUTMAP_07.",
                ))
            }
            T::LinToPq => pick(S::LinToPq, S::PqToLin),
            T::LinToGammaLog => pick(S::LinToGammaLog, S::GammaLogToLin),
            T::LinToDoubleLog => pick(S::LinToDoubleLog, S::DoubleLogToLin),
        })
    }

    /// The transform style (port of the second `ConvertStyle`).
    pub fn transform_style(self) -> FixedFunctionStyle {
        use FixedFunctionStyle as T;
        match self {
            S::AcesRedMod03Fwd | S::AcesRedMod03Inv => T::AcesRedMod03,
            S::AcesRedMod10Fwd | S::AcesRedMod10Inv => T::AcesRedMod10,
            S::AcesGlow03Fwd | S::AcesGlow03Inv => T::AcesGlow03,
            S::AcesGlow10Fwd | S::AcesGlow10Inv => T::AcesGlow10,
            S::AcesDarkToDim10Fwd | S::AcesDarkToDim10Inv => T::AcesDarkToDim10,
            S::AcesGamutComp13Fwd | S::AcesGamutComp13Inv => T::AcesGamutComp13,
            S::AcesOutputTransform20Fwd | S::AcesOutputTransform20Inv => T::AcesOutputTransform20,
            S::AcesRgbToJmh20 | S::AcesJmhToRgb20 => T::AcesRgbToJmh20,
            S::AcesRgbToHmj20 | S::AcesHmjToRgb20 => T::AcesRgbToHmj20,
            S::AcesTonescaleCompress20Fwd | S::AcesTonescaleCompress20Inv => {
                T::AcesTonescaleCompress20
            }
            S::AcesGamutCompress20Fwd | S::AcesGamutCompress20Inv => T::AcesGamutCompress20,
            S::Rec2100SurroundFwd | S::Rec2100SurroundInv => T::Rec2100Surround,
            S::RgbToHsv | S::HsvToRgb => T::RgbToHsv,
            S::XyzToXyy | S::XyyToXyz => T::XyzToXyy,
            S::XyzToUvy | S::UvyToXyz => T::XyzToUvy,
            S::XyzToLuv | S::LuvToXyz => T::XyzToLuv,
            S::LinToPq | S::PqToLin => T::LinToPq,
            S::LinToGammaLog | S::GammaLogToLin => T::LinToGammaLog,
            S::LinToDoubleLog | S::DoubleLogToLin => T::LinToDoubleLog,
            S::RgbToHsyLin | S::HsyLinToRgb => T::RgbToHsyLin,
            S::RgbToHsyLog | S::HsyLogToRgb => T::RgbToHsyLog,
            S::RgbToHsyVid | S::HsyVidToRgb => T::RgbToHsyVid,
        }
    }

    /// The direction encoded in the style.
    pub fn direction(self) -> TransformDirection {
        match self {
            S::AcesRedMod03Fwd
            | S::AcesRedMod10Fwd
            | S::AcesGlow03Fwd
            | S::AcesGlow10Fwd
            | S::AcesDarkToDim10Fwd
            | S::AcesGamutComp13Fwd
            | S::AcesOutputTransform20Fwd
            | S::AcesRgbToJmh20
            | S::AcesRgbToHmj20
            | S::AcesTonescaleCompress20Fwd
            | S::AcesGamutCompress20Fwd
            | S::Rec2100SurroundFwd
            | S::RgbToHsv
            | S::RgbToHsyLog
            | S::RgbToHsyLin
            | S::RgbToHsyVid
            | S::XyzToXyy
            | S::XyzToUvy
            | S::XyzToLuv
            | S::LinToPq
            | S::LinToGammaLog
            | S::LinToDoubleLog => TransformDirection::Forward,
            _ => TransformDirection::Inverse,
        }
    }

    /// The style of the inverse function.
    pub fn inverse(self) -> Self {
        match self {
            S::AcesRedMod03Fwd => S::AcesRedMod03Inv,
            S::AcesRedMod03Inv => S::AcesRedMod03Fwd,
            S::AcesRedMod10Fwd => S::AcesRedMod10Inv,
            S::AcesRedMod10Inv => S::AcesRedMod10Fwd,
            S::AcesGlow03Fwd => S::AcesGlow03Inv,
            S::AcesGlow03Inv => S::AcesGlow03Fwd,
            S::AcesGlow10Fwd => S::AcesGlow10Inv,
            S::AcesGlow10Inv => S::AcesGlow10Fwd,
            S::AcesDarkToDim10Fwd => S::AcesDarkToDim10Inv,
            S::AcesDarkToDim10Inv => S::AcesDarkToDim10Fwd,
            S::AcesGamutComp13Fwd => S::AcesGamutComp13Inv,
            S::AcesGamutComp13Inv => S::AcesGamutComp13Fwd,
            S::AcesOutputTransform20Fwd => S::AcesOutputTransform20Inv,
            S::AcesOutputTransform20Inv => S::AcesOutputTransform20Fwd,
            S::AcesRgbToJmh20 => S::AcesJmhToRgb20,
            S::AcesJmhToRgb20 => S::AcesRgbToJmh20,
            S::AcesRgbToHmj20 => S::AcesHmjToRgb20,
            S::AcesHmjToRgb20 => S::AcesRgbToHmj20,
            S::AcesTonescaleCompress20Fwd => S::AcesTonescaleCompress20Inv,
            S::AcesTonescaleCompress20Inv => S::AcesTonescaleCompress20Fwd,
            S::AcesGamutCompress20Fwd => S::AcesGamutCompress20Inv,
            S::AcesGamutCompress20Inv => S::AcesGamutCompress20Fwd,
            S::Rec2100SurroundFwd => S::Rec2100SurroundInv,
            S::Rec2100SurroundInv => S::Rec2100SurroundFwd,
            S::RgbToHsv => S::HsvToRgb,
            S::HsvToRgb => S::RgbToHsv,
            S::RgbToHsyLog => S::HsyLogToRgb,
            S::HsyLogToRgb => S::RgbToHsyLog,
            S::RgbToHsyLin => S::HsyLinToRgb,
            S::HsyLinToRgb => S::RgbToHsyLin,
            S::RgbToHsyVid => S::HsyVidToRgb,
            S::HsyVidToRgb => S::RgbToHsyVid,
            S::XyzToXyy => S::XyyToXyz,
            S::XyyToXyz => S::XyzToXyy,
            S::XyzToUvy => S::UvyToXyz,
            S::UvyToXyz => S::XyzToUvy,
            S::XyzToLuv => S::LuvToXyz,
            S::LuvToXyz => S::XyzToLuv,
            S::LinToPq => S::PqToLin,
            S::PqToLin => S::LinToPq,
            S::LinToGammaLog => S::GammaLogToLin,
            S::GammaLogToLin => S::LinToGammaLog,
            S::LinToDoubleLog => S::DoubleLogToLin,
            S::DoubleLogToLin => S::LinToDoubleLog,
        }
    }
}

impl fmt::Display for FixedFunctionOpStyle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// Op data

/// Parameters of a fixed function op (port of `FixedFunctionOpData`).
///
/// The fields are public so that invalid values can be set; use
/// [`FixedFunctionOpData::validate`] to check them ([`FixedFunctionOpData::new`]
/// validates).
#[derive(Debug, Clone)]
pub struct FixedFunctionOpData {
    /// The style (function and direction).
    pub style: FixedFunctionOpStyle,
    /// The style-dependent parameters.
    pub params: Vec<f64>,
    /// Format metadata (name, id, description, ...).
    pub metadata: FormatMetadata,
}

impl PartialEq for FixedFunctionOpData {
    /// Two op data are equal when their style and parameters are equal
    /// (metadata are ignored, as in OCIO).
    fn eq(&self, other: &Self) -> bool {
        self.style == other.style && self.params == other.params
    }
}

fn check_param_bounds(name: &str, val: f64, low: f64, high: f64) -> Result<()> {
    if val < low || val > high {
        return Err(Error::msg(format!(
            "Parameter {} ({}) is outside valid range [{},{}]",
            format_g(val, 6),
            name,
            format_g(low, 6),
            format_g(high, 6)
        )));
    }
    Ok(())
}

fn check_param_no_frac(name: &str, val: f64) -> Result<()> {
    if val.floor() != val {
        return Err(Error::msg(format!(
            "Parameter {} ({}) cannot include any fractional component",
            format_g(val, 6),
            name
        )));
    }
    Ok(())
}

impl FixedFunctionOpData {
    /// Build and validate.
    pub fn new(style: FixedFunctionOpStyle, params: &[f64]) -> Result<Self> {
        let data = Self {
            style,
            params: params.to_vec(),
            metadata: FormatMetadata::default(),
        };
        data.validate()?;
        Ok(data)
    }

    fn count_error(&self, expected: &str) -> Error {
        Error::msg(format!(
            "The style '{}' must have {} but {} found.",
            self.style.detailed_str(),
            expected,
            self.params.len()
        ))
    }

    /// Validate the parameters for the style.
    pub fn validate(&self) -> Result<()> {
        let p = &self.params;
        match self.style {
            S::AcesGamutComp13Fwd | S::AcesGamutComp13Inv => {
                if p.len() != 7 {
                    return Err(self.count_error("seven parameters"));
                }
                // Clamped to the smallest increment above 1 in half float
                // precision for numerical stability.
                const LIM_LOW_BOUND: f64 = 1.001;
                const LIM_HI_BOUND: f64 = 65504.0;
                check_param_bounds("lim_cyan", p[0], LIM_LOW_BOUND, LIM_HI_BOUND)?;
                check_param_bounds("lim_magenta", p[1], LIM_LOW_BOUND, LIM_HI_BOUND)?;
                check_param_bounds("lim_yellow", p[2], LIM_LOW_BOUND, LIM_HI_BOUND)?;

                const THR_LOW_BOUND: f64 = 0.0;
                // Clamped to the smallest increment below 1 in half float
                // precision for numerical stability.
                const THR_HI_BOUND: f64 = 0.9995;
                check_param_bounds("thr_cyan", p[3], THR_LOW_BOUND, THR_HI_BOUND)?;
                check_param_bounds("thr_magenta", p[4], THR_LOW_BOUND, THR_HI_BOUND)?;
                check_param_bounds("thr_yellow", p[5], THR_LOW_BOUND, THR_HI_BOUND)?;

                check_param_bounds("power", p[6], 1.0, 65504.0)?;
            }
            S::AcesOutputTransform20Fwd | S::AcesOutputTransform20Inv => {
                if p.len() != 9 {
                    return Err(self.count_error("9 parameters"));
                }
                check_param_bounds("peak_luminance", p[0], 1.0, 10000.0)?;
                check_param_no_frac("peak_luminance", p[0])?;
            }
            S::AcesRgbToJmh20 | S::AcesJmhToRgb20 | S::AcesRgbToHmj20 | S::AcesHmjToRgb20 => {
                if p.len() != 8 {
                    return Err(self.count_error("8 parameters"));
                }
            }
            S::AcesTonescaleCompress20Fwd | S::AcesTonescaleCompress20Inv => {
                if p.len() != 1 {
                    return Err(self.count_error("1 parameters"));
                }
                check_param_bounds("peak_luminance", p[0], 1.0, 10000.0)?;
                check_param_no_frac("peak_luminance", p[0])?;
            }
            S::AcesGamutCompress20Fwd | S::AcesGamutCompress20Inv => {
                if p.len() != 9 {
                    return Err(self.count_error("9 parameters"));
                }
                check_param_bounds("peak_luminance", p[0], 1.0, 10000.0)?;
                check_param_no_frac("peak_luminance", p[0])?;
            }
            S::Rec2100SurroundFwd | S::Rec2100SurroundInv => {
                if p.len() != 1 {
                    return Err(self.count_error("one parameter"));
                }
                let v = p[0];
                const LOW_BOUND: f64 = 0.01;
                const HI_BOUND: f64 = 100.0;
                if v < LOW_BOUND {
                    return Err(Error::msg(format!(
                        "Parameter {} is less than lower bound {}",
                        format_g(v, 6),
                        format_g(LOW_BOUND, 6)
                    )));
                } else if v > HI_BOUND {
                    return Err(Error::msg(format!(
                        "Parameter {} is greater than upper bound {}",
                        format_g(v, 6),
                        format_g(HI_BOUND, 6)
                    )));
                }
            }
            S::DoubleLogToLin | S::LinToDoubleLog => {
                if p.len() != 13 {
                    return Err(self.count_error("13 parameters"));
                }
                let base = p[0];
                let break1 = p[1];
                let break2 = p[2];
                if base <= 0.0 {
                    return Err(Error::msg(format!(
                        "Log base {} is not greater than zero.",
                        format_g(base, 6)
                    )));
                }
                if break1 > break2 {
                    return Err(Error::msg(format!(
                        "First break point {} is larger than the second break point {}.",
                        format_g(break1, 6),
                        format_g(break2, 6)
                    )));
                }
            }
            S::LinToGammaLog | S::GammaLogToLin => {
                if p.len() != 10 {
                    return Err(self.count_error("10 parameters"));
                }
                let mirror_pt = p[0];
                let break_pt = p[1];
                let gamma_seg_power = p[2];
                let log_seg_base = p[5];
                if log_seg_base <= 0.0 {
                    return Err(Error::msg(format!(
                        "Log base {} is not greater than zero.",
                        format_g(log_seg_base, 6)
                    )));
                }
                if mirror_pt >= break_pt {
                    return Err(Error::msg(format!(
                        "Mirror point {} is not smaller than the break point {}.",
                        format_g(mirror_pt, 6),
                        format_g(break_pt, 6)
                    )));
                }
                if gamma_seg_power == 0.0 {
                    return Err(Error::msg("Gamma power is zero."));
                }
            }
            _ => {
                if !p.is_empty() {
                    return Err(self.count_error("zero parameters"));
                }
            }
        }
        Ok(())
    }

    /// The direction encoded in the style.
    pub fn direction(&self) -> TransformDirection {
        self.style.direction()
    }

    /// Change the direction (inverting the style if needed).
    pub fn set_direction(&mut self, dir: TransformDirection) {
        if self.direction() != dir {
            self.invert();
        }
    }

    /// Invert in place (assumes the data are valid).
    fn invert(&mut self) {
        // Note that any existing metadata could become stale at this point.
        self.style = self.style.inverse();
    }

    /// Copy with the inverse style.
    pub fn inverse(&self) -> Self {
        let mut d = self.clone();
        d.invert();
        d
    }

    /// True if `other` is the inverse of `self`.
    pub fn is_inverse(&self, other: &FixedFunctionOpData) -> bool {
        if matches!(self.style, S::Rec2100SurroundFwd | S::Rec2100SurroundInv)
            && self.style == other.style
        {
            // Check for the case where the styles are the same but the
            // parameter is inverted.
            return match (self.params.first(), other.params.first()) {
                (Some(a), Some(b)) => *a == 1.0 / *b,
                _ => false,
            };
        }
        *other == self.inverse()
    }

    /// Cache id of the data (port of `FixedFunctionOpData::getCacheID`).
    pub fn cache_id(&self) -> String {
        let mut s = String::new();
        let id = self.metadata.id();
        if !id.is_empty() {
            s.push_str(id);
            s.push(' ');
        }
        s.push_str(self.style.detailed_str());
        for p in &self.params {
            s.push(' ');
            s.push_str(&format_g(*p, 7));
        }
        s
    }
}

// ---------------------------------------------------------------------------
// Op

/// The fixed function op (port of `FixedFunctionOp` with its CPU renderer).
#[derive(Clone)]
pub struct FixedFunctionOp {
    data: FixedFunctionOpData,
    renderer: RendererRc,
}

impl fmt::Debug for FixedFunctionOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FixedFunctionOp")
            .field("data", &self.data)
            .field("renderer", &self.renderer.name())
            .finish()
    }
}

impl FixedFunctionOp {
    /// Validate the data and build the op (including its CPU renderer, which
    /// for the ACES 2 styles precomputes the gamut tables).
    pub fn new(data: FixedFunctionOpData) -> Result<Self> {
        data.validate()?;
        let renderer = cpu::get_fixed_function_cpu_renderer(&data)?;
        Ok(Self { data, renderer })
    }

    /// The op data.
    pub fn data(&self) -> &FixedFunctionOpData {
        &self.data
    }

    /// Name of the OCIO CPU renderer class used by this op (e.g.
    /// `"Renderer_ACES_Glow03_Fwd"`).
    pub fn renderer_name(&self) -> &'static str {
        self.renderer.name()
    }

    /// True if `other` is a fixed function op computing the inverse of
    /// `self`.
    pub fn is_inverse(&self, other: &dyn Op) -> bool {
        other
            .downcast_ref::<FixedFunctionOp>()
            .is_some_and(|o| self.data.is_inverse(&o.data))
    }
}

impl Op for FixedFunctionOp {
    fn name(&self) -> &'static str {
        "FixedFunction"
    }

    fn apply(&self, pixels: &mut [Pixel]) {
        self.renderer.apply(pixels);
    }

    fn is_no_op(&self) -> bool {
        false
    }

    fn is_identity(&self) -> bool {
        false
    }

    fn has_channel_crosstalk(&self) -> bool {
        true
    }

    fn cache_id(&self) -> String {
        format!("<FixedFunctionOp {}>", self.data.cache_id())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn combine_with(&self, next: &dyn Op, flags: OptimizationFlags) -> Option<OpVec> {
        // Fixed functions never combine, but a pair of inverse ops cancels out.
        if flags.contains(OptimizationFlags::PAIR_IDENTITY_FIXED_FUNCTION) && self.is_inverse(next)
        {
            return Some(Vec::new());
        }
        None
    }

    fn to_transform(&self) -> Option<Transform> {
        Some(Transform::FixedFunction(FixedFunctionTransform {
            direction: self.data.direction(),
            style: self.data.style.transform_style(),
            params: self.data.params.clone(),
            metadata: self.data.metadata.clone(),
        }))
    }

    fn clone_box(&self) -> Box<dyn Op> {
        Box::new(self.clone())
    }
}

/// Append a fixed function op of the given style and parameters, in the
/// requested direction (port of `CreateFixedFunctionOp`).
pub fn create_fixed_function_op(
    ops: &mut OpVec,
    style: FixedFunctionOpStyle,
    params: &[f64],
    dir: TransformDirection,
) -> Result<()> {
    let data = FixedFunctionOpData::new(style, params)?;
    create_fixed_function_op_from_data(ops, &data, dir)
}

/// Append a fixed function op built from `data`, in the requested direction
/// (port of the `CreateFixedFunctionOp` overload taking op data).
pub fn create_fixed_function_op_from_data(
    ops: &mut OpVec,
    data: &FixedFunctionOpData,
    dir: TransformDirection,
) -> Result<()> {
    let data = match dir {
        TransformDirection::Forward => data.clone(),
        TransformDirection::Inverse => data.inverse(),
    };
    ops.push(Arc::new(FixedFunctionOp::new(data)?));
    Ok(())
}

// ---------------------------------------------------------------------------
// Transform

impl FixedFunctionTransform {
    /// The op data equivalent to the transform (style and direction
    /// combined).
    pub fn op_data(&self) -> Result<FixedFunctionOpData> {
        let style =
            FixedFunctionOpStyle::from_transform_style(self.style, TransformDirection::Forward)?;
        let mut data = FixedFunctionOpData {
            style,
            params: self.params.clone(),
            metadata: self.metadata.clone(),
        };
        data.set_direction(self.direction);
        Ok(data)
    }
}

impl Validate for FixedFunctionTransform {
    fn validate(&self) -> Result<()> {
        self.op_data()
            .and_then(|d| d.validate())
            .map_err(|e| e.prefixed("FixedFunctionTransform validation failed: "))
    }
}

impl BuildOps for FixedFunctionTransform {
    fn build_ops(
        &self,
        ops: &mut OpVec,
        _config: &Config,
        _context: &Context,
        dir: TransformDirection,
    ) -> Result<()> {
        let data = self.op_data()?;
        data.validate()?;
        create_fixed_function_op_from_data(ops, &data, dir)
    }
}

impl fmt::Display for FixedFunctionTransform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "<FixedFunction direction={}, style={}",
            self.direction, self.style
        )?;
        if let Some((first, rest)) = self.params.split_first() {
            write!(f, ", params=[{}", format_g(*first, 6))?;
            for p in rest {
                write!(f, ", {}", format_g(*p, 6))?;
            }
            write!(f, "]")?;
        }
        write!(f, ">")
    }
}

#[cfg(test)]
mod tests;
