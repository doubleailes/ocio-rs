//! Internal representation of the CLF/CTF process nodes (port of the parts
//! of OCIO's `OpData` classes used by the CTF reader and writer: styles,
//! string conversions, parameter validation, bit-depth scaling, and the
//! conversions from and to the public transforms).

use crate::error::{Error, Result};
use crate::format_metadata::FormatMetadata;
use crate::transforms::grading::{
    bspline_type_for_hue_curve_type, GradingBSplineCurve, GradingHueCurve, GradingPrimary,
    GradingRgbCurve, GradingRgbm, GradingRgbmsw, GradingTone,
};
use crate::transforms::*;
use crate::types::*;

use super::xml::{fmt_g, DEFAULT_PRECISION};

/// Maximum length of a 1D LUT in a CLF/CTF file.
pub(crate) const MAX_1D_LUT_LENGTH: usize = 300_000;
/// Maximum grid size of a 3D LUT.
pub(crate) const MAX_3D_LUT_LENGTH: usize = 129;
/// Number of entries of a half-domain 1D LUT.
pub(crate) const HALF_DOMAIN_REQUIRED_ENTRIES: usize = 65536;

/// `%g` formatting with the default stream precision (6).
fn g6(v: f64) -> String {
    fmt_g(v, DEFAULT_PRECISION)
}

// ---------------------------------------------------------------------------
// CDL

/// CDL op styles (style and direction combined).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CdlOpStyle {
    V12Fwd,
    V12Rev,
    NoClampFwd,
    NoClampRev,
}

impl CdlOpStyle {
    /// Parse a CTF/CLF CDL style name (case insensitive).
    pub(crate) fn from_name(name: &str) -> Result<Self> {
        let n = name.to_ascii_lowercase();
        Ok(match n.as_str() {
            "v1.2_fwd" | "fwd" => CdlOpStyle::V12Fwd,
            "v1.2_rev" | "rev" => CdlOpStyle::V12Rev,
            "noclampfwd" | "fwdnoclamp" => CdlOpStyle::NoClampFwd,
            "noclamprev" | "revnoclamp" => CdlOpStyle::NoClampRev,
            _ => return Err(Error::msg("Unknown style for CDL.")),
        })
    }

    /// CLF style name.
    pub(crate) fn name(self) -> &'static str {
        match self {
            CdlOpStyle::V12Fwd => "Fwd",
            CdlOpStyle::V12Rev => "Rev",
            CdlOpStyle::NoClampFwd => "FwdNoClamp",
            CdlOpStyle::NoClampRev => "RevNoClamp",
        }
    }

    /// Combine a transform style and direction.
    pub(crate) fn from_transform(style: CdlStyle, dir: TransformDirection) -> Self {
        let fwd = dir == TransformDirection::Forward;
        match style {
            CdlStyle::Asc => {
                if fwd {
                    CdlOpStyle::V12Fwd
                } else {
                    CdlOpStyle::V12Rev
                }
            }
            CdlStyle::NoClamp => {
                if fwd {
                    CdlOpStyle::NoClampFwd
                } else {
                    CdlOpStyle::NoClampRev
                }
            }
        }
    }

    /// The transform style.
    pub(crate) fn transform_style(self) -> CdlStyle {
        match self {
            CdlOpStyle::V12Fwd | CdlOpStyle::V12Rev => CdlStyle::Asc,
            CdlOpStyle::NoClampFwd | CdlOpStyle::NoClampRev => CdlStyle::NoClamp,
        }
    }

    /// The direction.
    pub(crate) fn direction(self) -> TransformDirection {
        match self {
            CdlOpStyle::V12Fwd | CdlOpStyle::NoClampFwd => TransformDirection::Forward,
            CdlOpStyle::V12Rev | CdlOpStyle::NoClampRev => TransformDirection::Inverse,
        }
    }
}

/// ASC CDL process node.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CdlData {
    pub style: CdlOpStyle,
    pub slope: [f64; 3],
    pub offset: [f64; 3],
    pub power: [f64; 3],
    pub sat: f64,
    pub metadata: FormatMetadata,
}

impl Default for CdlData {
    fn default() -> Self {
        Self {
            style: CdlOpStyle::NoClampFwd,
            slope: [1.0; 3],
            offset: [0.0; 3],
            power: [1.0; 3],
            sat: 1.0,
            metadata: FormatMetadata::default(),
        }
    }
}

/// Validate the CDL parameters (slope >= 0, power > 0, sat >= 0).
pub(crate) fn validate_cdl_params(slope: &[f64; 3], power: &[f64; 3], sat: f64) -> Result<()> {
    for &v in slope {
        if !(v >= 0.0) {
            crate::bail!("CDL: Invalid 'slope' {} should be greater than 0.", g6(v));
        }
    }
    for &v in power {
        if !(v > 0.0) {
            crate::bail!(
                "CDLOpData: Invalid 'power' {} should be greater than 0.",
                g6(v)
            );
        }
    }
    if !(sat >= 0.0) {
        crate::bail!(
            "CDL: Invalid 'saturation' {} should be greater than 0.",
            g6(sat)
        );
    }
    Ok(())
}

impl CdlData {
    pub(crate) fn validate(&self) -> Result<()> {
        validate_cdl_params(&self.slope, &self.power, self.sat)
    }
}

// ---------------------------------------------------------------------------
// Exposure contrast

/// Exposure / contrast op styles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EcOpStyle {
    Linear,
    LinearRev,
    Video,
    VideoRev,
    Log,
    LogRev,
}

impl EcOpStyle {
    pub(crate) fn from_name(s: &str) -> Result<Self> {
        if s.is_empty() {
            return Err(Error::msg("Missing exposure contrast style."));
        }
        Ok(match s.to_ascii_lowercase().as_str() {
            "linear" => EcOpStyle::Linear,
            "linearrev" => EcOpStyle::LinearRev,
            "video" => EcOpStyle::Video,
            "videorev" => EcOpStyle::VideoRev,
            "log" => EcOpStyle::Log,
            "logrev" => EcOpStyle::LogRev,
            _ => crate::bail!("Unknown exposure contrast style: '{}'.", s),
        })
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            EcOpStyle::Linear => "linear",
            EcOpStyle::LinearRev => "linearRev",
            EcOpStyle::Video => "video",
            EcOpStyle::VideoRev => "videoRev",
            EcOpStyle::Log => "log",
            EcOpStyle::LogRev => "logRev",
        }
    }

    pub(crate) fn from_transform(style: ExposureContrastStyle, dir: TransformDirection) -> Self {
        let fwd = dir == TransformDirection::Forward;
        match (style, fwd) {
            (ExposureContrastStyle::Linear, true) => EcOpStyle::Linear,
            (ExposureContrastStyle::Linear, false) => EcOpStyle::LinearRev,
            (ExposureContrastStyle::Video, true) => EcOpStyle::Video,
            (ExposureContrastStyle::Video, false) => EcOpStyle::VideoRev,
            (ExposureContrastStyle::Logarithmic, true) => EcOpStyle::Log,
            (ExposureContrastStyle::Logarithmic, false) => EcOpStyle::LogRev,
        }
    }

    pub(crate) fn transform_style(self) -> ExposureContrastStyle {
        match self {
            EcOpStyle::Linear | EcOpStyle::LinearRev => ExposureContrastStyle::Linear,
            EcOpStyle::Video | EcOpStyle::VideoRev => ExposureContrastStyle::Video,
            EcOpStyle::Log | EcOpStyle::LogRev => ExposureContrastStyle::Logarithmic,
        }
    }

    pub(crate) fn direction(self) -> TransformDirection {
        match self {
            EcOpStyle::Linear | EcOpStyle::Video | EcOpStyle::Log => TransformDirection::Forward,
            _ => TransformDirection::Inverse,
        }
    }
}

/// Default log exposure step.
pub(crate) const LOGEXPOSURESTEP_DEFAULT: f64 = 0.088;
/// Default log middle gray.
pub(crate) const LOGMIDGRAY_DEFAULT: f64 = 0.435;

/// Exposure / contrast process node.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EcData {
    pub style: EcOpStyle,
    pub exposure: f64,
    pub contrast: f64,
    pub gamma: f64,
    pub pivot: f64,
    pub log_exposure_step: f64,
    pub log_mid_gray: f64,
    pub exposure_dynamic: bool,
    pub contrast_dynamic: bool,
    pub gamma_dynamic: bool,
    pub metadata: FormatMetadata,
}

impl Default for EcData {
    fn default() -> Self {
        Self {
            style: EcOpStyle::Linear,
            exposure: 0.0,
            contrast: 1.0,
            gamma: 1.0,
            pivot: 0.18,
            log_exposure_step: LOGEXPOSURESTEP_DEFAULT,
            log_mid_gray: LOGMIDGRAY_DEFAULT,
            exposure_dynamic: false,
            contrast_dynamic: false,
            gamma_dynamic: false,
            metadata: FormatMetadata::default(),
        }
    }
}

// ---------------------------------------------------------------------------
// Fixed function

/// Parse a CTF fixed function style name into a transform style and a
/// direction (port of `FixedFunctionOpData::GetStyle`).
pub(crate) fn ff_style_from_name(name: &str) -> Result<(FixedFunctionStyle, TransformDirection)> {
    use FixedFunctionStyle::*;
    use TransformDirection::{Forward as F, Inverse as I};
    let n = name.to_ascii_lowercase();
    let r = match n.as_str() {
        "redmod03fwd" => (AcesRedMod03, F),
        "redmod03rev" => (AcesRedMod03, I),
        "redmod10fwd" => (AcesRedMod10, F),
        "redmod10rev" => (AcesRedMod10, I),
        "glow03fwd" => (AcesGlow03, F),
        "glow03rev" => (AcesGlow03, I),
        "glow10fwd" => (AcesGlow10, F),
        "glow10rev" => (AcesGlow10, I),
        "darktodim10" => (AcesDarkToDim10, F),
        "dimtodark10" => (AcesDarkToDim10, I),
        "gamutcomp13fwd" => (AcesGamutComp13, F),
        "gamutcomp13rev" => (AcesGamutComp13, I),
        "acesoutputtransform20fwd" => (AcesOutputTransform20, F),
        "acesoutputtransform20inv" => (AcesOutputTransform20, I),
        "rgb_to_jmh_20" => (AcesRgbToJmh20, F),
        "jmh_to_rgb_20" => (AcesRgbToJmh20, I),
        "rgb_to_hmj_20" => (AcesRgbToHmj20, F),
        "hmj_to_rgb_20" => (AcesRgbToHmj20, I),
        "tonescalecompress20fwd" => (AcesTonescaleCompress20, F),
        "tonescalecompress20inv" => (AcesTonescaleCompress20, I),
        "gamutcompress20fwd" => (AcesGamutCompress20, F),
        "gamutcompress20inv" => (AcesGamutCompress20, I),
        "surround" | "rec2100surroundfwd" => (Rec2100Surround, F),
        "rec2100surroundrev" => (Rec2100Surround, I),
        "rgb_to_hsv" => (RgbToHsv, F),
        "hsv_to_rgb" => (RgbToHsv, I),
        "xyz_to_xyy" => (XyzToXyy, F),
        "xyy_to_xyz" => (XyzToXyy, I),
        "xyz_to_uvy" => (XyzToUvy, F),
        "uvy_to_xyz" => (XyzToUvy, I),
        "xyz_to_luv" => (XyzToLuv, F),
        "luv_to_xyz" => (XyzToLuv, I),
        "lin_to_pq" => (LinToPq, F),
        "pq_to_lin" => (LinToPq, I),
        "lin_to_gammalog" => (LinToGammaLog, F),
        "gammalog_to_lin" => (LinToGammaLog, I),
        "lin_to_doublelog" => (LinToDoubleLog, F),
        "doublelog_to_lin" => (LinToDoubleLog, I),
        "rgb_to_hsy_lin" => (RgbToHsyLin, F),
        "rgb_to_hsy_log" => (RgbToHsyLog, F),
        "rgb_to_hsy_vid" => (RgbToHsyVid, F),
        "hsy_log_to_rgb" => (RgbToHsyLog, I),
        "hsy_lin_to_rgb" => (RgbToHsyLin, I),
        "hsy_vid_to_rgb" => (RgbToHsyVid, I),
        _ => crate::bail!("Unknown FixedFunction style: {}", name),
    };
    Ok(r)
}

/// CTF name of a fixed function style (port of
/// `FixedFunctionOpData::ConvertStyleToString`). Use `detailed` for a more
/// verbose human readable string.
pub(crate) fn ff_style_to_name(
    style: FixedFunctionStyle,
    dir: TransformDirection,
    detailed: bool,
) -> Result<&'static str> {
    use FixedFunctionStyle::*;
    let fwd = dir == TransformDirection::Forward;
    let pick =
        |f: &'static str, fd: &'static str, i: &'static str, id: &'static str| -> &'static str {
            match (fwd, detailed) {
                (true, false) => f,
                (true, true) => fd,
                (false, false) => i,
                (false, true) => id,
            }
        };
    Ok(match style {
        AcesRedMod03 => pick("RedMod03Fwd", "ACES_RedMod03 (Forward)", "RedMod03Rev", "ACES_RedMod03 (Inverse)"),
        AcesRedMod10 => pick("RedMod10Fwd", "ACES_RedMod10 (Forward)", "RedMod10Rev", "ACES_RedMod10 (Inverse)"),
        AcesGlow03 => pick("Glow03Fwd", "ACES_Glow03 (Forward)", "Glow03Rev", "ACES_Glow03 (Inverse)"),
        AcesGlow10 => pick("Glow10Fwd", "ACES_Glow10 (Forward)", "Glow10Rev", "ACES_Glow10 (Inverse)"),
        AcesDarkToDim10 => pick("DarkToDim10", "ACES_DarkToDim10 (Forward)", "DimToDark10", "ACES_DarkToDim10 (Inverse)"),
        AcesGamutComp13 => pick("GamutComp13Fwd", "ACES_GamutComp13 (Forward)", "GamutComp13Rev", "ACES_GamutComp13 (Inverse)"),
        AcesOutputTransform20 => pick(
            "ACESOutputTransform20Fwd",
            "ACES_OutputTransform20 (Forward)",
            "ACESOutputTransform20Inv",
            "ACES_OutputTransform20 (Inverse)",
        ),
        AcesRgbToJmh20 => pick("RGB_TO_JMh_20", "RGB_TO_JMh_20", "JMh_TO_RGB_20", "JMh_TO_RGB_20"),
        AcesRgbToHmj20 => pick("RGB_TO_HMJ_20", "RGB_TO_HMJ_20", "HMJ_TO_RGB_20", "HMJ_TO_RGB_20"),
        AcesTonescaleCompress20 => pick(
            "ToneScaleCompress20Fwd",
            "ACES_ToneScaleCompress20 (Forward)",
            "ToneScaleCompress20Inv",
            "ACES_ToneScaleCompress20 (Inverse)",
        ),
        AcesGamutCompress20 => pick(
            "GamutCompress20Fwd",
            "ACES_GamutCompress20 (Forward)",
            "GamutCompress20Inv",
            "ACES_GamutCompress20 (Inverse)",
        ),
        Rec2100Surround => pick(
            "Rec2100SurroundFwd",
            "REC2100_Surround (Forward)",
            "Rec2100SurroundRev",
            "REC2100_Surround (Inverse)",
        ),
        RgbToHsv => pick("RGB_TO_HSV", "RGB_TO_HSV", "HSV_TO_RGB", "HSV_TO_RGB"),
        XyzToXyy => pick("XYZ_TO_xyY", "XYZ_TO_xyY", "xyY_TO_XYZ", "xyY_TO_XYZ"),
        XyzToUvy => pick("XYZ_TO_uvY", "XYZ_TO_uvY", "uvY_TO_XYZ", "uvY_TO_XYZ"),
        XyzToLuv => pick("XYZ_TO_LUV", "XYZ_TO_LUV", "LUV_TO_XYZ", "LUV_TO_XYZ"),
        LinToPq => pick("Lin_TO_PQ", "Lin_TO_PQ", "PQ_TO_Lin", "PQ_TO_Lin"),
        LinToGammaLog => pick("Lin_TO_GammaLog", "Lin_TO_GammaLog", "GammaLog_TO_Lin", "GammaLog_TO_Lin"),
        LinToDoubleLog => pick("Lin_TO_DoubleLog", "Lin_TO_DoubleLog", "DoubleLog_TO_Lin", "DoubleLog_TO_Lin"),
        RgbToHsyLin => pick("RGB_TO_HSY_LIN", "RGB_TO_HSY_LIN", "HSY_LIN_TO_RGB", "HSY_LIN_TO_RGB"),
        RgbToHsyLog => pick("RGB_TO_HSY_LOG", "RGB_TO_HSY_LOG", "HSY_LOG_TO_RGB", "HSY_LOG_TO_RGB"),
        RgbToHsyVid => pick("RGB_TO_HSY_VID", "RGB_TO_HSY_VID", "HSY_VID_TO_RGB", "HSY_VID_TO_RGB"),
        AcesGamutMap02 | AcesGamutMap07 => {
            return Err(Error::msg(
                "Unimplemented fixed function types: FIXED_FUNCTION_ACES_GAMUTMAP_02, FIXED_FUNCTION_ACES_GAMUTMAP_07.",
            ))
        }
    })
}

/// Fixed function process node.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FfData {
    pub style: FixedFunctionStyle,
    pub dir: TransformDirection,
    pub params: Vec<f64>,
    pub metadata: FormatMetadata,
}

impl Default for FfData {
    fn default() -> Self {
        Self {
            style: FixedFunctionStyle::AcesRedMod03,
            dir: TransformDirection::Forward,
            params: Vec::new(),
            metadata: FormatMetadata::default(),
        }
    }
}

fn check_param_bounds(name: &str, val: f64, low: f64, high: f64) -> Result<()> {
    if val < low || val > high {
        crate::bail!(
            "Parameter {} ({}) is outside valid range [{},{}]",
            g6(val),
            name,
            g6(low),
            g6(high)
        );
    }
    Ok(())
}

fn check_param_no_frac(name: &str, val: f64) -> Result<()> {
    if val.floor() != val {
        crate::bail!(
            "Parameter {} ({}) cannot include any fractional component",
            g6(val),
            name
        );
    }
    Ok(())
}

impl FfData {
    /// Port of `FixedFunctionOpData::validate`.
    pub(crate) fn validate(&self) -> Result<()> {
        use FixedFunctionStyle::*;
        let name = ff_style_to_name(self.style, self.dir, true)?;
        let n = self.params.len();
        let p = &self.params;
        match self.style {
            AcesGamutComp13 => {
                if n != 7 {
                    crate::bail!(
                        "The style '{}' must have seven parameters but {} found.",
                        name,
                        n
                    );
                }
                const LIM_LOW: f64 = 1.001;
                const LIM_HI: f64 = 65504.0;
                check_param_bounds("lim_cyan", p[0], LIM_LOW, LIM_HI)?;
                check_param_bounds("lim_magenta", p[1], LIM_LOW, LIM_HI)?;
                check_param_bounds("lim_yellow", p[2], LIM_LOW, LIM_HI)?;
                const THR_LOW: f64 = 0.0;
                const THR_HI: f64 = 0.9995;
                check_param_bounds("thr_cyan", p[3], THR_LOW, THR_HI)?;
                check_param_bounds("thr_magenta", p[4], THR_LOW, THR_HI)?;
                check_param_bounds("thr_yellow", p[5], THR_LOW, THR_HI)?;
                check_param_bounds("power", p[6], 1.0, 65504.0)?;
            }
            AcesOutputTransform20 => {
                if n != 9 {
                    crate::bail!(
                        "The style '{}' must have 9 parameters but {} found.",
                        name,
                        n
                    );
                }
                check_param_bounds("peak_luminance", p[0], 1.0, 10000.0)?;
                check_param_no_frac("peak_luminance", p[0])?;
            }
            AcesRgbToJmh20 | AcesRgbToHmj20 => {
                if n != 8 {
                    crate::bail!(
                        "The style '{}' must have 8 parameters but {} found.",
                        name,
                        n
                    );
                }
            }
            AcesTonescaleCompress20 => {
                if n != 1 {
                    crate::bail!(
                        "The style '{}' must have 1 parameters but {} found.",
                        name,
                        n
                    );
                }
                check_param_bounds("peak_luminance", p[0], 1.0, 10000.0)?;
                check_param_no_frac("peak_luminance", p[0])?;
            }
            AcesGamutCompress20 => {
                if n != 9 {
                    crate::bail!(
                        "The style '{}' must have 9 parameters but {} found.",
                        name,
                        n
                    );
                }
                check_param_bounds("peak_luminance", p[0], 1.0, 10000.0)?;
                check_param_no_frac("peak_luminance", p[0])?;
            }
            Rec2100Surround => {
                if n != 1 {
                    crate::bail!(
                        "The style '{}' must have one parameter but {} found.",
                        name,
                        n
                    );
                }
                let v = p[0];
                const LOW: f64 = 0.01;
                const HI: f64 = 100.0;
                if v < LOW {
                    crate::bail!("Parameter {} is less than lower bound {}", g6(v), g6(LOW));
                } else if v > HI {
                    crate::bail!("Parameter {} is greater than upper bound {}", g6(v), g6(HI));
                }
            }
            LinToDoubleLog => {
                if n != 13 {
                    crate::bail!(
                        "The style '{}' must have 13 parameters but {} found.",
                        name,
                        n
                    );
                }
                let (base, break1, break2) = (p[0], p[1], p[2]);
                if base <= 0.0 {
                    crate::bail!("Log base {} is not greater than zero.", g6(base));
                }
                if break1 > break2 {
                    crate::bail!(
                        "First break point {} is larger than the second break point {}.",
                        g6(break1),
                        g6(break2)
                    );
                }
            }
            LinToGammaLog => {
                if n != 10 {
                    crate::bail!(
                        "The style '{}' must have 10 parameters but {} found.",
                        name,
                        n
                    );
                }
                let (mirror, brk, power, base) = (p[0], p[1], p[2], p[5]);
                if base <= 0.0 {
                    crate::bail!("Log base {} is not greater than zero.", g6(base));
                }
                if mirror >= brk {
                    crate::bail!(
                        "Mirror point {} is not smaller than the break point {}.",
                        g6(mirror),
                        g6(brk)
                    );
                }
                if power == 0.0 {
                    crate::bail!("Gamma power is zero.");
                }
            }
            _ => {
                if n != 0 {
                    crate::bail!(
                        "The style '{}' must have zero parameters but {} found.",
                        name,
                        n
                    );
                }
            }
        }
        Ok(())
    }

    /// Minimum CTF version required to write the style.
    pub(crate) fn min_version_2x(&self) -> (u32, u32) {
        use FixedFunctionStyle::*;
        match self.style {
            AcesGamutComp13 => (2, 1),
            LinToPq
            | LinToGammaLog
            | LinToDoubleLog
            | AcesOutputTransform20
            | AcesRgbToJmh20
            | AcesTonescaleCompress20
            | AcesGamutCompress20 => (2, 4),
            RgbToHsyLog | RgbToHsyLin | RgbToHsyVid => (2, 5),
            AcesRgbToHmj20 => (2, 6),
            _ => (2, 0),
        }
    }
}

// ---------------------------------------------------------------------------
// Gamma

/// Gamma (exponent) op styles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GammaStyle {
    BasicFwd,
    BasicRev,
    BasicMirrorFwd,
    BasicMirrorRev,
    BasicPassThruFwd,
    BasicPassThruRev,
    MonCurveFwd,
    MonCurveRev,
    MonCurveMirrorFwd,
    MonCurveMirrorRev,
}

impl GammaStyle {
    pub(crate) fn from_name(s: &str) -> Result<Self> {
        if s.is_empty() {
            return Err(Error::msg("Missing gamma style."));
        }
        use GammaStyle::*;
        Ok(match s.to_ascii_lowercase().as_str() {
            "basicfwd" => BasicFwd,
            "basicrev" => BasicRev,
            "basicmirrorfwd" => BasicMirrorFwd,
            "basicmirrorrev" => BasicMirrorRev,
            "basicpassthrufwd" => BasicPassThruFwd,
            "basicpassthrurev" => BasicPassThruRev,
            "moncurvefwd" => MonCurveFwd,
            "moncurverev" => MonCurveRev,
            "moncurvemirrorfwd" => MonCurveMirrorFwd,
            "moncurvemirrorrev" => MonCurveMirrorRev,
            _ => crate::bail!("Unknown gamma style: '{}'.", s),
        })
    }

    pub(crate) fn name(self) -> &'static str {
        use GammaStyle::*;
        match self {
            BasicFwd => "basicFwd",
            BasicRev => "basicRev",
            BasicMirrorFwd => "basicMirrorFwd",
            BasicMirrorRev => "basicMirrorRev",
            BasicPassThruFwd => "basicPassThruFwd",
            BasicPassThruRev => "basicPassThruRev",
            MonCurveFwd => "monCurveFwd",
            MonCurveRev => "monCurveRev",
            MonCurveMirrorFwd => "monCurveMirrorFwd",
            MonCurveMirrorRev => "monCurveMirrorRev",
        }
    }

    pub(crate) fn is_moncurve(self) -> bool {
        use GammaStyle::*;
        matches!(
            self,
            MonCurveFwd | MonCurveRev | MonCurveMirrorFwd | MonCurveMirrorRev
        )
    }

    pub(crate) fn direction(self) -> TransformDirection {
        use GammaStyle::*;
        match self {
            BasicFwd | BasicMirrorFwd | BasicPassThruFwd | MonCurveFwd | MonCurveMirrorFwd => {
                TransformDirection::Forward
            }
            _ => TransformDirection::Inverse,
        }
    }

    pub(crate) fn negative_style(self) -> NegativeStyle {
        use GammaStyle::*;
        match self {
            BasicFwd | BasicRev => NegativeStyle::Clamp,
            MonCurveFwd | MonCurveRev => NegativeStyle::Linear,
            BasicMirrorFwd | BasicMirrorRev | MonCurveMirrorFwd | MonCurveMirrorRev => {
                NegativeStyle::Mirror
            }
            BasicPassThruFwd | BasicPassThruRev => NegativeStyle::PassThru,
        }
    }

    /// Port of `GammaOpData::ConvertStyleBasic`.
    pub(crate) fn basic(neg: NegativeStyle, dir: TransformDirection) -> Result<Self> {
        let fwd = dir == TransformDirection::Forward;
        use GammaStyle::*;
        Ok(match neg {
            NegativeStyle::Clamp => {
                if fwd {
                    BasicFwd
                } else {
                    BasicRev
                }
            }
            NegativeStyle::Mirror => {
                if fwd {
                    BasicMirrorFwd
                } else {
                    BasicMirrorRev
                }
            }
            NegativeStyle::PassThru => {
                if fwd {
                    BasicPassThruFwd
                } else {
                    BasicPassThruRev
                }
            }
            NegativeStyle::Linear => {
                return Err(Error::msg(
                    "Linear negative extrapolation is not valid for basic exponent style.",
                ))
            }
        })
    }

    /// Port of `GammaOpData::ConvertStyleMonCurve`.
    pub(crate) fn moncurve(neg: NegativeStyle, dir: TransformDirection) -> Result<Self> {
        let fwd = dir == TransformDirection::Forward;
        use GammaStyle::*;
        Ok(match neg {
            NegativeStyle::Linear => {
                if fwd {
                    MonCurveFwd
                } else {
                    MonCurveRev
                }
            }
            NegativeStyle::Mirror => {
                if fwd {
                    MonCurveMirrorFwd
                } else {
                    MonCurveMirrorRev
                }
            }
            NegativeStyle::PassThru => {
                return Err(Error::msg(
                    "Pass thru negative extrapolation is not valid for MonCurve exponent style.",
                ))
            }
            NegativeStyle::Clamp => {
                return Err(Error::msg(
                    "Clamp negative extrapolation is not valid for MonCurve exponent style.",
                ))
            }
        })
    }

    /// Identity parameters of the style.
    pub(crate) fn identity_params(self) -> Vec<f64> {
        if self.is_moncurve() {
            vec![1.0, 0.0]
        } else {
            vec![1.0]
        }
    }
}

/// Gamma process node. `params` are the red, green, blue and alpha
/// parameters (gamma, and offset for the moncurve styles).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct GammaData {
    pub style: GammaStyle,
    pub params: [Vec<f64>; 4],
    pub metadata: FormatMetadata,
}

impl Default for GammaData {
    fn default() -> Self {
        let p = GammaStyle::BasicFwd.identity_params();
        Self {
            style: GammaStyle::BasicFwd,
            params: [p.clone(), p.clone(), p.clone(), p],
            metadata: FormatMetadata::default(),
        }
    }
}

fn validate_gamma_params(p: &[f64], reqd: usize, low: &[f64], high: &[f64]) -> Result<()> {
    if p.len() != reqd {
        return Err(Error::msg("GammaOp: Wrong number of parameters"));
    }
    for i in 0..reqd {
        if p[i] < low[i] {
            crate::bail!(
                "Parameter {} is less than lower bound {}",
                g6(p[i]),
                g6(low[i])
            );
        }
        if p[i] > high[i] {
            crate::bail!(
                "Parameter {} is greater than upper bound {}",
                g6(p[i]),
                g6(high[i])
            );
        }
    }
    Ok(())
}

impl GammaData {
    /// Set the red, green and blue parameters, alpha is set to identity.
    pub(crate) fn set_params(&mut self, p: Vec<f64>) {
        self.params[0] = p.clone();
        self.params[1] = p.clone();
        self.params[2] = p;
        self.params[3] = self.style.identity_params();
    }

    /// Port of `GammaOpData::validateParameters`.
    pub(crate) fn validate(&self) -> Result<()> {
        if self.style.is_moncurve() {
            for p in &self.params {
                validate_gamma_params(p, 2, &[1.0, 0.0], &[10.0, 0.9])?;
            }
        } else {
            for p in &self.params {
                validate_gamma_params(p, 1, &[0.01], &[100.0])?;
            }
        }
        Ok(())
    }

    pub(crate) fn is_identity_params(&self, p: &[f64]) -> bool {
        if self.style.is_moncurve() {
            p.len() == 2 && p[0] == 1.0 && p[1] == 0.0
        } else {
            p.len() == 1 && p[0] == 1.0
        }
    }

    pub(crate) fn is_alpha_identity(&self) -> bool {
        self.is_identity_params(&self.params[3])
    }

    pub(crate) fn is_non_channel_dependent(&self) -> bool {
        self.params[0] == self.params[1]
            && self.params[0] == self.params[2]
            && self.is_alpha_identity()
    }
}

// ---------------------------------------------------------------------------
// Grading

/// Parse a grading style (with the direction suffix `Rev`).
pub(crate) fn grading_style_from_name(s: &str) -> Result<(GradingStyle, TransformDirection)> {
    if s.is_empty() {
        return Err(Error::msg("Missing grading style."));
    }
    use TransformDirection::{Forward as F, Inverse as I};
    Ok(match s.to_ascii_lowercase().as_str() {
        "log" => (GradingStyle::Log, F),
        "logrev" => (GradingStyle::Log, I),
        "linear" => (GradingStyle::Lin, F),
        "linearrev" => (GradingStyle::Lin, I),
        "video" => (GradingStyle::Video, F),
        "videorev" => (GradingStyle::Video, I),
        _ => crate::bail!("Unknown grading style: '{}'.", s),
    })
}

/// CTF name of a grading style and direction.
pub(crate) fn grading_style_to_name(style: GradingStyle, dir: TransformDirection) -> &'static str {
    let fwd = dir == TransformDirection::Forward;
    match style {
        GradingStyle::Log => {
            if fwd {
                "log"
            } else {
                "logRev"
            }
        }
        GradingStyle::Lin => {
            if fwd {
                "linear"
            } else {
                "linearRev"
            }
        }
        GradingStyle::Video => {
            if fwd {
                "video"
            } else {
                "videoRev"
            }
        }
    }
}

fn fmt_rgbm(v: &GradingRgbm) -> String {
    format!(
        "<r={}, g={}, b={}, m={}>",
        g6(v.red),
        g6(v.green),
        g6(v.blue),
        g6(v.master)
    )
}

fn fmt_rgbmsw(v: &GradingRgbmsw) -> String {
    format!(
        "<red={} green={} blue={} master={} start={} width={}>",
        g6(v.red),
        g6(v.green),
        g6(v.blue),
        g6(v.master),
        g6(v.start),
        g6(v.width)
    )
}

/// Port of `GradingPrimary::validate`.
pub(crate) fn validate_grading_primary(v: &GradingPrimary, style: GradingStyle) -> Result<()> {
    const LOWER_BOUND: f64 = 0.01;
    const BOUND_ERROR: f64 = 0.000001;
    const MIN: f64 = LOWER_BOUND - BOUND_ERROR;
    let below = |x: &GradingRgbm| x.red < MIN || x.green < MIN || x.blue < MIN || x.master < MIN;
    if style != GradingStyle::Lin && below(&v.gamma) {
        crate::bail!(
            "GradingPrimary gamma '{}' are below lower bound ({}).",
            fmt_rgbm(&v.gamma),
            g6(LOWER_BOUND)
        );
    }
    if style == GradingStyle::Lin && below(&v.contrast) {
        crate::bail!(
            "GradingPrimary contrast '{}' are below lower bound ({}).",
            fmt_rgbm(&v.contrast),
            g6(LOWER_BOUND)
        );
    }
    if (v.pivot_white - v.pivot_black) < MIN {
        return Err(Error::msg(
            "GradingPrimary black pivot should be smaller than white pivot.",
        ));
    }
    if v.clamp_black > v.clamp_white {
        return Err(Error::msg(
            "GradingPrimary black clamp should be smaller than white clamp.",
        ));
    }
    Ok(())
}

/// Port of `GradingBSplineCurveImpl::validate`.
pub(crate) fn validate_bspline_curve(c: &GradingBSplineCurve) -> Result<()> {
    let n = c.control_points.len();
    if n < 2 {
        return Err(Error::msg("There must be at least 2 control points."));
    }
    if n != c.slopes.len() {
        return Err(Error::msg(
            "The slopes array must be the same length as the control points.",
        ));
    }
    let gf = |v: f32| g6(f64::from(v));
    let mut last_x = -f32::MAX;
    for (i, p) in c.control_points.iter().enumerate() {
        if p.x < last_x {
            crate::bail!(
                "Control point at index {} has a x coordinate '{}' that is less than previous control point x coordinate '{}'.",
                i,
                gf(p.x),
                gf(last_x)
            );
        }
        last_x = p.x;
    }
    if c.spline_type == BSplineType::HueHueBSpline {
        if c.control_points[0].x < 0.0 {
            return Err(Error::msg(
                "The HUE-HUE spline may not have negative x coordinates.",
            ));
        } else if c.control_points[n - 1].x > 1.0 {
            return Err(Error::msg(
                "The HUE-HUE spline may not have x coordinates greater than one.",
            ));
        }
    }
    if matches!(
        c.spline_type,
        BSplineType::BSpline | BSplineType::DiagonalBSpline | BSplineType::HueHueBSpline
    ) {
        let mut last_y = -f32::MAX;
        if c.spline_type == BSplineType::HueHueBSpline {
            last_y = c.control_points[n - 1].y - 1.0;
        }
        for (i, p) in c.control_points.iter().enumerate() {
            if p.y < last_y {
                crate::bail!(
                    "Control point at index {} has a y coordinate '{}' that is less than previous control point y coordinate '{}'.",
                    i,
                    gf(p.y),
                    gf(last_y)
                );
            }
            last_y = p.y;
        }
    }
    if n == 2
        && matches!(
            c.spline_type,
            BSplineType::Periodic1BSpline
                | BSplineType::Periodic0BSpline
                | BSplineType::HueHueBSpline
        )
    {
        let del_x = c.control_points[1].x - c.control_points[0].x;
        if (1.0 - del_x).abs() < 1e-3 {
            return Err(Error::msg(
                "The periodic spline x coordinates may not wrap to the same value.",
            ));
        }
    }
    Ok(())
}

/// Name of an RGB curve (for error messages).
fn rgb_curve_name(c: usize) -> &'static str {
    match c {
        0 => "red",
        1 => "green",
        2 => "blue",
        3 => "master",
        _ => "invalid",
    }
}

/// Name of a hue curve (for error messages).
fn hue_curve_name(c: usize) -> &'static str {
    match c {
        0 => "hue_hue",
        1 => "hue_sat",
        2 => "hue_lum",
        3 => "lum_sat",
        4 => "sat_sat",
        5 => "lum_lum",
        6 => "sat_lum",
        7 => "hue_fx",
        _ => "illegal",
    }
}

/// Port of `GradingRGBCurveImpl::validate`.
pub(crate) fn validate_rgb_curves(v: &GradingRgbCurve) -> Result<()> {
    for (c, curve) in v.curves.iter().enumerate() {
        if let Err(e) = validate_bspline_curve(curve) {
            crate::bail!(
                "GradingRGBCurve validation failed for '{}' curve with: {}",
                rgb_curve_name(c),
                e
            );
        }
        if curve.spline_type != BSplineType::BSpline {
            crate::bail!(
                "GradingRGBCurve validation failed: '{}' curve is of the wrong BSplineType.",
                rgb_curve_name(c)
            );
        }
    }
    Ok(())
}

/// Port of `GradingHueCurveImpl::validate`.
pub(crate) fn validate_hue_curves(v: &GradingHueCurve) -> Result<()> {
    for (c, curve) in v.curves.iter().enumerate() {
        if let Err(e) = validate_bspline_curve(curve) {
            crate::bail!(
                "GradingHueCurve validation failed for '{}' curve with: {}",
                hue_curve_name(c),
                e
            );
        }
        if !v.draw_curve_only
            && curve.spline_type != bspline_type_for_hue_curve_type(HueCurveType::ALL[c])
        {
            crate::bail!(
                "GradingHueCurve validation failed: '{}' curve is of the wrong BSplineType.",
                hue_curve_name(c)
            );
        }
    }
    Ok(())
}

/// Port of `GradingTone::validate`.
pub(crate) fn validate_grading_tone(t: &GradingTone) -> Result<()> {
    const MIN_BMW: f64 = 0.1;
    const MAX_BMW: f64 = 1.9;
    const MIN_SH: f64 = 0.2;
    const MAX_SH: f64 = 1.8;
    const MIN_WSC: f64 = 0.01;
    const MAX_SC: f64 = 1.99;
    const ERR: f64 = 0.000001;
    const MIN_BMW_TOL: f64 = MIN_BMW - ERR;
    const MAX_BMW_TOL: f64 = MAX_BMW + ERR;
    const MIN_SH_TOL: f64 = MIN_SH - ERR;
    const MAX_SH_TOL: f64 = MAX_SH + ERR;
    const MIN_WSC_TOL: f64 = MIN_WSC - ERR;
    const MAX_SC_TOL: f64 = MAX_SC + ERR;

    let below = |v: &GradingRgbmsw, m: f64| v.red < m || v.green < m || v.blue < m || v.master < m;
    let above = |v: &GradingRgbmsw, m: f64| v.red > m || v.green > m || v.blue > m || v.master > m;

    // Blacks, midtones, whites.
    for (name, above_name, v) in [
        ("blacks", "blacks", &t.blacks),
        ("midtones", "midtones", &t.midtones),
        ("whites", "white", &t.whites),
    ] {
        if below(v, MIN_BMW_TOL) {
            crate::bail!(
                "GradingTone {} '{}' are below lower bound ({}).",
                name,
                fmt_rgbmsw(v),
                g6(MIN_BMW)
            );
        }
        if v.width < MIN_WSC_TOL {
            crate::bail!(
                "GradingTone {} width '{}' is below lower bound ({}).",
                name,
                g6(v.width),
                g6(MIN_WSC)
            );
        }
        if above(v, MAX_BMW_TOL) {
            crate::bail!(
                "GradingTone {} '{}' are above upper bound ({}).",
                above_name,
                fmt_rgbmsw(v),
                g6(MAX_BMW)
            );
        }
    }
    {
        let v = &t.shadows;
        if below(v, MIN_SH_TOL) {
            crate::bail!(
                "GradingTone shadows '{}' are below lower bound ({}).",
                fmt_rgbmsw(v),
                g6(MIN_SH)
            );
        }
        if v.start < v.width + MIN_WSC_TOL {
            crate::bail!(
                "GradingTone shadows start '{}' is less than pivot ('{}' + {}).",
                g6(v.start),
                g6(v.width),
                g6(MIN_WSC)
            );
        }
        if above(v, MAX_SH_TOL) {
            crate::bail!(
                "GradingTone shadows '{}' are above upper bound ({}).",
                fmt_rgbmsw(v),
                g6(MAX_SH)
            );
        }
    }
    {
        let v = &t.highlights;
        if below(v, MIN_SH_TOL) {
            crate::bail!(
                "GradingTone highlights '{}' are below lower bound ({}).",
                fmt_rgbmsw(v),
                g6(MIN_SH)
            );
        }
        if v.start > v.width - MIN_WSC_TOL {
            crate::bail!(
                "GradingTone highlights start '{}' is greater than pivot ('{}' - {}).",
                g6(v.start),
                g6(v.width),
                g6(MIN_WSC)
            );
        }
        if above(v, MAX_SH_TOL) {
            crate::bail!(
                "GradingTone highlights '{}' are above upper bound ({}).",
                fmt_rgbmsw(v),
                g6(MAX_SH)
            );
        }
    }
    if t.s_contrast < MIN_WSC_TOL {
        crate::bail!(
            "GradingTone s-contrast '{}' is below lower bound ({}).",
            g6(t.s_contrast),
            g6(MIN_WSC)
        );
    }
    if t.s_contrast > MAX_SC_TOL {
        crate::bail!(
            "GradingTone s-contrast '{}' is above upper bound ({}).",
            g6(t.s_contrast),
            g6(MAX_SC)
        );
    }
    Ok(())
}

/// Grading primary process node.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct GradingPrimaryData {
    pub style: GradingStyle,
    pub dir: TransformDirection,
    pub value: GradingPrimary,
    pub dynamic: bool,
    pub metadata: FormatMetadata,
}

/// Grading RGB curve process node.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct GradingRgbCurveData {
    pub style: GradingStyle,
    pub dir: TransformDirection,
    pub value: GradingRgbCurve,
    pub bypass_lin_to_log: bool,
    pub dynamic: bool,
    pub metadata: FormatMetadata,
}

/// Grading hue curve process node.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct GradingHueCurveData {
    pub style: GradingStyle,
    pub dir: TransformDirection,
    pub value: GradingHueCurve,
    pub rgb_to_hsy: HsyTransformStyle,
    pub dynamic: bool,
    pub metadata: FormatMetadata,
}

/// Grading tone process node.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct GradingToneData {
    pub style: GradingStyle,
    pub dir: TransformDirection,
    pub value: GradingTone,
    pub dynamic: bool,
    pub metadata: FormatMetadata,
}

// ---------------------------------------------------------------------------
// Log

/// Index of the log side slope in the log parameters.
pub(crate) const LOG_SIDE_SLOPE: usize = 0;
/// Index of the log side offset.
pub(crate) const LOG_SIDE_OFFSET: usize = 1;
/// Index of the linear side slope.
pub(crate) const LIN_SIDE_SLOPE: usize = 2;
/// Index of the linear side offset.
pub(crate) const LIN_SIDE_OFFSET: usize = 3;
/// Index of the linear side break.
pub(crate) const LIN_SIDE_BREAK: usize = 4;
/// Index of the linear slope.
pub(crate) const LINEAR_SLOPE: usize = 5;

/// Log process node. Each channel has 4 to 6 parameters (see the index
/// constants).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LogData {
    pub base: f64,
    pub dir: TransformDirection,
    pub params: [Vec<f64>; 3],
    pub metadata: FormatMetadata,
}

impl LogData {
    /// Log of the given base with default (identity) affine parameters.
    pub(crate) fn new(base: f64, dir: TransformDirection) -> Self {
        let p = vec![1.0, 0.0, 1.0, 0.0];
        Self {
            base,
            dir,
            params: [p.clone(), p.clone(), p],
            metadata: FormatMetadata::default(),
        }
    }

    fn validate_channel(p: &[f64]) -> Result<()> {
        if p.len() < 4 {
            return Err(Error::msg("Log: expecting at least 4 parameters."));
        }
        if p.len() > 6 {
            return Err(Error::msg("Log: expecting at most 6 parameters."));
        }
        if crate::math_utils::is_scalar_equal_to_zero(p[LIN_SIDE_SLOPE]) {
            crate::bail!(
                "Log: Invalid linear side slope value '{}', linear side slope cannot be 0.",
                g6(p[LIN_SIDE_SLOPE])
            );
        }
        if crate::math_utils::is_scalar_equal_to_zero(p[LOG_SIDE_SLOPE]) {
            crate::bail!(
                "Log: Invalid log side slope value '{}', log side slope cannot be 0.",
                g6(p[LOG_SIDE_SLOPE])
            );
        }
        Ok(())
    }

    /// Port of `LogOpData::validate`.
    pub(crate) fn validate(&self) -> Result<()> {
        for p in &self.params {
            Self::validate_channel(p)?;
        }
        if self.params[0].len() != self.params[1].len()
            || self.params[0].len() != self.params[2].len()
        {
            return Err(Error::msg(
                "Log: Red, green & blue parameters must have the same size.",
            ));
        }
        if self.base == 1.0 {
            crate::bail!(
                "Log: Invalid base value '{}', base cannot be 1.",
                g6(self.base)
            );
        } else if self.base <= 0.0 {
            crate::bail!(
                "Log: Invalid base value '{}', base must be greater than 0.",
                g6(self.base)
            );
        }
        Ok(())
    }

    pub(crate) fn all_components_equal(&self) -> bool {
        self.params[0] == self.params[1] && self.params[0] == self.params[2]
    }

    pub(crate) fn is_simple_log(&self) -> bool {
        if self.all_components_equal() && self.params[0].len() == 4 {
            let p = &self.params[0];
            return p[LOG_SIDE_SLOPE] == 1.0
                && p[LIN_SIDE_SLOPE] == 1.0
                && p[LIN_SIDE_OFFSET] == 0.0
                && p[LOG_SIDE_OFFSET] == 0.0;
        }
        false
    }

    pub(crate) fn is_log2(&self) -> bool {
        self.is_simple_log() && self.base == 2.0
    }

    pub(crate) fn is_log10(&self) -> bool {
        self.is_simple_log() && self.base == 10.0
    }

    pub(crate) fn is_camera(&self) -> bool {
        self.params[0].len() > 4
    }
}

/// CTF log styles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LogStyle {
    Log10,
    Log2,
    AntiLog10,
    AntiLog2,
    LogToLin,
    LinToLog,
    CameraLogToLin,
    CameraLinToLog,
}

impl LogStyle {
    pub(crate) fn from_name(s: &str) -> Result<Self> {
        if s.is_empty() {
            return Err(Error::msg("Missing Log style."));
        }
        use LogStyle::*;
        Ok(match s.to_ascii_lowercase().as_str() {
            "log10" => Log10,
            "log2" => Log2,
            "antilog10" => AntiLog10,
            "antilog2" => AntiLog2,
            "logtolin" => LogToLin,
            "lintolog" => LinToLog,
            "cameralogtolin" => CameraLogToLin,
            "cameralintolog" => CameraLinToLog,
            _ => crate::bail!("Unknown Log style: '{}'.", s),
        })
    }

    pub(crate) fn name(self) -> &'static str {
        use LogStyle::*;
        match self {
            Log10 => "log10",
            Log2 => "log2",
            AntiLog10 => "antiLog10",
            AntiLog2 => "antiLog2",
            LogToLin => "logToLin",
            LinToLog => "linToLog",
            CameraLogToLin => "cameraLogToLin",
            CameraLinToLog => "cameraLinToLog",
        }
    }

    pub(crate) fn direction(self) -> TransformDirection {
        use LogStyle::*;
        match self {
            Log10 | Log2 | LinToLog | CameraLinToLog => TransformDirection::Forward,
            _ => TransformDirection::Inverse,
        }
    }
}

/// Kind of parameters used by a CTF log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CtfLogParamsType {
    Unknown,
    Cineon,
    Clf,
}

/// Legacy (Cineon-like) CTF log parameters: per channel gamma, refWhite,
/// refBlack, highlight and shadow.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CtfLogParams {
    pub style: LogStyle,
    pub params: [[f64; 5]; 3],
    kind: CtfLogParamsType,
}

impl Default for CtfLogParams {
    fn default() -> Self {
        Self {
            style: LogStyle::Log10,
            params: [[0.0; 5]; 3],
            kind: CtfLogParamsType::Unknown,
        }
    }
}

impl CtfLogParams {
    /// Set the type, returns false if a different type was already set.
    pub(crate) fn set_type(&mut self, t: CtfLogParamsType) -> bool {
        if self.kind == CtfLogParamsType::Unknown {
            self.kind = t;
        } else if t != self.kind {
            return false;
        }
        true
    }

    pub(crate) fn get_type(&self) -> CtfLogParamsType {
        self.kind
    }
}

fn validate_legacy_log_params(p: &[f64; 5]) -> Result<()> {
    let (gamma, ref_white, ref_black, highlight, shadow) = (p[0], p[1], p[2], p[3], p[4]);
    if !(gamma > f64::from(0.01f32)) {
        crate::bail!(
            "Log: Invalid gamma value '{}', gamma should be greater than 0.01.",
            g6(gamma)
        );
    }
    if !(ref_white > ref_black) {
        crate::bail!(
            "Log: Invalid refWhite '{}' and refBlack '{}', refWhite should be greater than refBlack.",
            g6(ref_white),
            g6(ref_black)
        );
    }
    if !(highlight > shadow) {
        crate::bail!(
            "Log: Invalid highlight '{}' and shadow '{}', highlight should be greater than shadow.",
            g6(highlight),
            g6(shadow)
        );
    }
    Ok(())
}

fn convert_from_ctf_to_ocio(p: &[f64; 5], out: &mut [f64]) {
    let range = 0.002 * 1023.0;
    let gamma = p[0];
    let ref_white = p[1] / 1023.0;
    let ref_black = p[2] / 1023.0;
    let highlight = p[3];
    let shadow = p[4];
    let mult_factor = range / gamma;
    let mut tmp = (ref_black - ref_white) * mult_factor;
    tmp = tmp.min(-0.0001);
    let gain = (highlight - shadow) / (1.0 - 10f64.powf(tmp));
    let offset = gain - (highlight - shadow);
    out[LOG_SIDE_SLOPE] = 1.0 / mult_factor;
    out[LIN_SIDE_SLOPE] = 1.0 / gain;
    out[LIN_SIDE_OFFSET] = (offset - shadow) / gain;
    out[LOG_SIDE_OFFSET] = ref_white;
}

/// Port of `LogUtil::ConvertLogParameters`: returns the base and the red,
/// green and blue parameters.
pub(crate) fn convert_log_parameters(ctf: &CtfLogParams) -> Result<(f64, [Vec<f64>; 3])> {
    let mut params = [
        vec![1.0, 0.0, 1.0, 0.0],
        vec![1.0, 0.0, 1.0, 0.0],
        vec![1.0, 0.0, 1.0, 0.0],
    ];
    let base = match ctf.style {
        LogStyle::Log10 | LogStyle::AntiLog10 => 10.0,
        LogStyle::Log2 | LogStyle::AntiLog2 => 2.0,
        LogStyle::LinToLog | LogStyle::LogToLin => {
            for p in &ctf.params {
                validate_legacy_log_params(p)?;
            }
            for c in 0..3 {
                convert_from_ctf_to_ocio(&ctf.params[c], &mut params[c]);
            }
            10.0
        }
        LogStyle::CameraLinToLog | LogStyle::CameraLogToLin => 2.0,
    };
    Ok((base, params))
}

// ---------------------------------------------------------------------------
// LUTs

/// 1D LUT process node. `values` holds `length * 3` values (normalized),
/// `num_components` is the number of components found in the file.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Lut1DData {
    pub dir: TransformDirection,
    pub interpolation: Interpolation,
    pub half_domain: bool,
    pub raw_halfs: bool,
    pub hue_adjust: Lut1DHueAdjust,
    pub file_output_bd: BitDepth,
    pub length: usize,
    pub num_components: usize,
    pub values: Vec<f32>,
    pub metadata: FormatMetadata,
}

impl Lut1DData {
    /// Identity LUT of the given length.
    pub(crate) fn new(length: usize, dir: TransformDirection) -> Self {
        let mut values = vec![0.0f32; length * 3];
        let step = 1.0f32 / (length as f32 - 1.0);
        for i in 0..length {
            let v = i as f32 * step;
            values[3 * i] = v;
            values[3 * i + 1] = v;
            values[3 * i + 2] = v;
        }
        Self {
            dir,
            interpolation: Interpolation::Default,
            half_domain: false,
            raw_halfs: false,
            hue_adjust: Lut1DHueAdjust::None,
            file_output_bd: BitDepth::Unknown,
            length,
            num_components: 3,
            values,
            metadata: FormatMetadata::default(),
        }
    }

    /// Resize (values are reset to zero, like OCIO's array resize of a new
    /// array which is then filled by the parser).
    pub(crate) fn resize(&mut self, length: usize, num_components: usize) {
        self.length = length;
        self.num_components = num_components;
        self.values.resize(length * 3, 0.0);
    }

    /// Multiply the values by `scale` (in single precision).
    pub(crate) fn scale(&mut self, scale: f32) {
        if scale != 1.0 {
            for v in &mut self.values {
                *v *= scale;
            }
        }
    }

    /// Port of `Lut1DOpData::IsValidInterpolation`.
    pub(crate) fn is_valid_interpolation(i: Interpolation) -> bool {
        matches!(
            i,
            Interpolation::Best
                | Interpolation::Default
                | Interpolation::Linear
                | Interpolation::Nearest
        )
    }

    /// Port of `Lut1DOpData::validate`.
    pub(crate) fn validate(&self) -> Result<()> {
        if self.hue_adjust == Lut1DHueAdjust::Wypn {
            return Err(Error::msg(
                "1D LUT HUE_WYPN hue adjust style is not implemented.",
            ));
        }
        if !Self::is_valid_interpolation(self.interpolation) {
            crate::bail!(
                "1D LUT does not support interpolation algorithm: {}.",
                self.interpolation.as_str()
            );
        }
        if self.length == 0 {
            return Err(Error::msg(
                "1D LUT content array issue: Array content is empty.",
            ));
        }
        if self.values.len() != self.length * 3 {
            crate::bail!(
                "1D LUT content array issue: Array contains: {} values, but {} are expected.",
                self.values.len(),
                self.length * 3
            );
        }
        if self.half_domain && self.length != HALF_DOMAIN_REQUIRED_ENTRIES {
            crate::bail!(
                "1D LUT: {} entries found, {} required for halfDomain 1D LUT.",
                self.length,
                HALF_DOMAIN_REQUIRED_ENTRIES
            );
        }
        Ok(())
    }
}

/// 3D LUT process node (blue fastest order, like the CLF array).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Lut3DData {
    pub dir: TransformDirection,
    pub interpolation: Interpolation,
    pub file_output_bd: BitDepth,
    pub grid_size: usize,
    pub num_components: usize,
    pub values: Vec<f32>,
    pub metadata: FormatMetadata,
}

impl Lut3DData {
    /// Identity LUT of the given grid size.
    pub(crate) fn new(n: usize, dir: TransformDirection) -> Self {
        let t = Lut3DTransform::new(n);
        Self {
            dir,
            interpolation: Interpolation::Default,
            file_output_bd: BitDepth::Unknown,
            grid_size: n,
            num_components: 3,
            values: t.values,
            metadata: FormatMetadata::default(),
        }
    }

    pub(crate) fn resize(&mut self, n: usize, num_components: usize) {
        self.grid_size = n;
        self.num_components = num_components;
        self.values.resize(n * n * n * 3, 0.0);
    }

    pub(crate) fn scale(&mut self, scale: f32) {
        if scale != 1.0 {
            for v in &mut self.values {
                *v *= scale;
            }
        }
    }

    /// Port of `Lut3DOpData::IsValidInterpolation`.
    pub(crate) fn is_valid_interpolation(i: Interpolation) -> bool {
        matches!(
            i,
            Interpolation::Best
                | Interpolation::Tetrahedral
                | Interpolation::Default
                | Interpolation::Linear
                | Interpolation::Nearest
        )
    }

    /// Port of `Lut3DOpData::validate`.
    pub(crate) fn validate(&self) -> Result<()> {
        if !Self::is_valid_interpolation(self.interpolation) {
            crate::bail!(
                "Lut3D does not support interpolation algorithm: {}.",
                self.interpolation.as_str()
            );
        }
        let n = self.grid_size;
        if n == 0 {
            return Err(Error::msg(
                "Lut3D content array issue: Array content is empty.",
            ));
        }
        if self.values.len() != n * n * n * 3 {
            crate::bail!(
                "Lut3D content array issue: Array contains: {} values, but {} are expected.",
                self.values.len(),
                n * n * n * 3
            );
        }
        if self.num_components != 3 {
            return Err(Error::msg(
                "Lut3D has an incorrect number of color components. ",
            ));
        }
        if n > MAX_3D_LUT_LENGTH {
            crate::bail!("Lut3D length: {} is not supported. ", n);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Matrix

/// Matrix process node (always 4x4 plus offsets, normalized values).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MatrixData {
    pub dir: TransformDirection,
    pub matrix: [f64; 16],
    pub offsets: [f64; 4],
    pub file_in_bd: BitDepth,
    pub file_out_bd: BitDepth,
    pub metadata: FormatMetadata,
}

impl Default for MatrixData {
    fn default() -> Self {
        Self {
            dir: TransformDirection::Forward,
            matrix: IDENTITY_MATRIX44,
            offsets: [0.0; 4],
            file_in_bd: BitDepth::Unknown,
            file_out_bd: BitDepth::Unknown,
            metadata: FormatMetadata::default(),
        }
    }
}

/// Inverse of a 4x4 matrix (port of `MatrixArray::inverse`, i.e. Imath's
/// `gjInverse`).
pub(crate) fn matrix_inverse(m: &[f64; 16]) -> Result<[f64; 16]> {
    let mut t = *m;
    let mut s = IDENTITY_MATRIX44;
    let dim = 4;
    for i in 0..3 {
        let mut pivot = i;
        let mut pivotsize = t[i * dim + i].abs();
        for j in (i + 1)..4 {
            let tmp = t[j * dim + i].abs();
            if tmp > pivotsize {
                pivot = j;
                pivotsize = tmp;
            }
        }
        if pivotsize == 0.0 {
            return Err(Error::msg("Singular Matrix can't be inverted."));
        }
        if pivot != i {
            for j in 0..4 {
                t.swap(i * dim + j, pivot * dim + j);
                s.swap(i * dim + j, pivot * dim + j);
            }
        }
        for j in (i + 1)..4 {
            let f = t[j * dim + i] / t[i * dim + i];
            for k in 0..4 {
                t[j * dim + k] -= f * t[i * dim + k];
                s[j * dim + k] -= f * s[i * dim + k];
            }
        }
    }
    for i in (0..4).rev() {
        let f = t[i * dim + i];
        if f == 0.0 {
            return Err(Error::msg("Singular Matrix can't be inverted."));
        }
        for j in 0..4 {
            t[i * dim + j] /= f;
            s[i * dim + j] /= f;
        }
        for j in 0..i {
            let f = t[j * dim + i];
            for k in 0..4 {
                t[j * dim + k] -= f * t[i * dim + k];
                s[j * dim + k] -= f * s[i * dim + k];
            }
        }
    }
    Ok(s)
}

impl MatrixData {
    /// Port of `MatrixOpData::hasAlpha`.
    pub(crate) fn has_alpha(&self) -> bool {
        let m = &self.matrix;
        m[3] != 0.0
            || m[7] != 0.0
            || m[11] != 0.0
            || !crate::math_utils::equal_with_abs_error(m[15], 1.0, 1e-6)
            || m[12] != 0.0
            || m[13] != 0.0
            || m[14] != 0.0
            || self.offsets[3] != 0.0
    }

    pub(crate) fn has_offsets(&self) -> bool {
        self.offsets.iter().any(|&o| o != 0.0)
    }

    /// Port of `MatrixOpData::getAsForward`.
    pub(crate) fn as_forward(&self) -> Result<MatrixData> {
        if self.dir == TransformDirection::Forward {
            return Ok(self.clone());
        }
        let inv = matrix_inverse(&self.matrix)?;
        let mut inv_off = [0.0; 4];
        if self.has_offsets() {
            for i in 0..4 {
                let mut acc = 0.0;
                for j in 0..4 {
                    acc += inv[i * 4 + j] * self.offsets[j];
                }
                inv_off[i] = -acc;
            }
        }
        Ok(MatrixData {
            dir: TransformDirection::Forward,
            matrix: inv,
            offsets: inv_off,
            file_in_bd: self.file_out_bd,
            file_out_bd: self.file_in_bd,
            metadata: self.metadata.clone(),
        })
    }

    /// Port of `MatrixOpData::validate` (the array is always 4x4 here).
    pub(crate) fn validate(&self) -> Result<()> {
        if self.dir == TransformDirection::Inverse {
            self.as_forward()?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Range

/// Range process node. Missing bounds are NaN (like OCIO's `EmptyValue`).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RangeData {
    pub dir: TransformDirection,
    pub min_in: f64,
    pub max_in: f64,
    pub min_out: f64,
    pub max_out: f64,
    pub file_in_bd: BitDepth,
    pub file_out_bd: BitDepth,
    pub metadata: FormatMetadata,
}

impl Default for RangeData {
    fn default() -> Self {
        Self {
            dir: TransformDirection::Forward,
            min_in: f64::NAN,
            max_in: f64::NAN,
            min_out: f64::NAN,
            max_out: f64::NAN,
            file_in_bd: BitDepth::Unknown,
            file_out_bd: BitDepth::Unknown,
            metadata: FormatMetadata::default(),
        }
    }
}

/// OCIO tests emptiness on the value converted to float.
fn is_empty_value(v: f64) -> bool {
    (v as f32).is_nan()
}

/// Port of `RangeOpData::FloatsDiffer`.
fn floats_differ(x1: f64, x2: f64) -> bool {
    if x1.abs() < 1e-3 {
        (x1 - x2).abs() > 1e-6
    } else {
        (1.0 - (x2 / x1)).abs() > 1e-6
    }
}

impl RangeData {
    pub(crate) fn min_is_empty(&self) -> bool {
        is_empty_value(self.min_in)
    }

    pub(crate) fn max_is_empty(&self) -> bool {
        is_empty_value(self.max_in)
    }

    /// Port of `RangeOpData::fillScaleOffset`: returns (scale, offset).
    pub(crate) fn scale_offset(&self) -> Result<(f64, f64)> {
        let mut scale = 1.0;
        let mut offset = 0.0;
        if !self.min_is_empty() && !self.max_is_empty() {
            let denom = self.max_in - self.min_in;
            if denom.abs() < 1e-6 {
                return Err(Error::msg("Range maxInValue is too close to minInValue"));
            }
            scale = (self.max_out - self.min_out) / denom;
            offset = self.min_out - scale * self.min_in;
        }
        Ok((scale, offset))
    }

    /// Port of `RangeOpData::validate`.
    pub(crate) fn validate(&self) -> Result<()> {
        const MIN_ERR: &str =
            "In and out minimum limits must be both set or both missing in Range.";
        const MAX_ERR: &str =
            "In and out maximum limits must be both set or both missing in Range.";
        if is_empty_value(self.min_in) != is_empty_value(self.min_out) {
            return Err(Error::msg(MIN_ERR));
        }
        if is_empty_value(self.max_in) {
            if !is_empty_value(self.max_out) {
                return Err(Error::msg(MAX_ERR));
            }
            if is_empty_value(self.min_in) {
                return Err(Error::msg(
                    "At least minimum or maximum limits must be set in Range.",
                ));
            }
        } else if is_empty_value(self.max_out) {
            return Err(Error::msg(MAX_ERR));
        }
        if !is_empty_value(self.min_in) && !is_empty_value(self.max_in) {
            if self.min_in > self.max_in {
                return Err(Error::msg(
                    "Range maximum input value is less than minimum input value",
                ));
            }
            if self.min_out > self.max_out {
                return Err(Error::msg(
                    "Range maximum output value is less than minimum output value",
                ));
            }
        }
        if is_empty_value(self.max_in)
            && !is_empty_value(self.min_in)
            && floats_differ(self.min_out, self.min_in)
        {
            return Err(Error::msg(
                "In and out minimum limits must be equal if maximum values are missing in Range.",
            ));
        }
        if is_empty_value(self.min_in)
            && !is_empty_value(self.max_in)
            && floats_differ(self.max_out, self.max_in)
        {
            return Err(Error::msg(
                "In and out maximum limits must be equal if minimum values are missing in Range.",
            ));
        }
        self.scale_offset()?;
        Ok(())
    }

    /// Port of `RangeOpData::normalize` (bit-depth scaling).
    pub(crate) fn normalize(&mut self) {
        let in_scale = 1.0 / self.file_in_bd.max_value();
        let out_scale = 1.0 / self.file_out_bd.max_value();
        if !self.min_is_empty() {
            self.min_in *= in_scale;
        }
        if !self.max_is_empty() {
            self.max_in *= in_scale;
        }
        if !self.min_is_empty() {
            self.min_out *= out_scale;
        }
        if !self.max_is_empty() {
            self.max_out *= out_scale;
        }
    }

    /// Port of `RangeOpData::getAsForward`.
    pub(crate) fn as_forward(&self) -> Result<RangeData> {
        if self.dir == TransformDirection::Forward {
            return Ok(self.clone());
        }
        let r = RangeData {
            dir: TransformDirection::Forward,
            min_in: self.min_out,
            max_in: self.max_out,
            min_out: self.min_in,
            max_out: self.max_in,
            file_in_bd: self.file_out_bd,
            file_out_bd: self.file_in_bd,
            metadata: self.metadata.clone(),
        };
        r.validate()?;
        Ok(r)
    }

    /// Port of `RangeOpData::convertToMatrix`.
    pub(crate) fn convert_to_matrix(&self) -> Result<MatrixData> {
        if self.min_is_empty() || self.max_is_empty() {
            return Err(Error::msg(
                "Non-clamping Range min & max values have to be set.",
            ));
        }
        let fwd = self.as_forward()?;
        let (scale, offset) = fwd.scale_offset()?;
        let mut m = MatrixData {
            metadata: fwd.metadata.clone(),
            file_in_bd: fwd.file_in_bd,
            file_out_bd: fwd.file_out_bd,
            ..Default::default()
        };
        m.matrix[0] = scale;
        m.matrix[5] = scale;
        m.matrix[10] = scale;
        m.offsets = [offset, offset, offset, 0.0];
        m.validate()?;
        Ok(m)
    }
}

// ---------------------------------------------------------------------------
// Index mapping

/// Index map of a LUT (port of `IndexMapping`). Only the first component is
/// used.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct IndexMapping {
    indices: Vec<(f32, f32)>,
}

impl IndexMapping {
    pub(crate) fn new(dimension: usize) -> Self {
        Self {
            indices: vec![(0.0, 0.0); dimension],
        }
    }

    pub(crate) fn dimension(&self) -> usize {
        self.indices.len()
    }

    pub(crate) fn resize(&mut self, dimension: usize) {
        self.indices.resize(dimension, (0.0, 0.0));
    }

    fn validate_index(&self, index: usize) -> Result<()> {
        if index >= self.indices.len() {
            crate::bail!(
                "IndexMapping: Index {} is invalid. Should be less than {}.",
                index,
                self.indices.len()
            );
        }
        Ok(())
    }

    pub(crate) fn pair(&self, index: usize) -> Result<(f32, f32)> {
        self.validate_index(index)?;
        Ok(self.indices[index])
    }

    pub(crate) fn set_pair(&mut self, index: usize, first: f32, second: f32) -> Result<()> {
        self.validate_index(index)?;
        self.indices[index] = (first, second);
        Ok(())
    }

    /// Both halves of the map must be increasing.
    pub(crate) fn validate(&self) -> Result<()> {
        for i in 1..self.indices.len() {
            let (f, s) = self.indices[i];
            let (pf, ps) = self.indices[i - 1];
            if f <= pf || s <= ps {
                return Err(Error::msg("Index values must be increasing."));
            }
        }
        Ok(())
    }
}

impl RangeData {
    /// Range equivalent to a 2 entry index map (port of the `RangeOpData`
    /// constructor from an `IndexMapping`).
    pub(crate) fn from_index_mapping(
        im: &IndexMapping,
        len: usize,
        bd: BitDepth,
    ) -> Result<RangeData> {
        if im.dimension() != 2 {
            return Err(Error::msg(
                "CTF/CLF parsing error. Only two entry IndexMaps are supported.",
            ));
        }
        let scale_in = 1.0 / bd.max_value();
        let (f0, s0) = im.pair(0)?;
        let (f1, s1) = im.pair(1)?;
        let r = RangeData {
            min_in: f64::from(f0) * scale_in,
            min_out: f64::from(s0) / (len as f64 - 1.0),
            max_in: f64::from(f1) * scale_in,
            max_out: f64::from(s1) / (len as f64 - 1.0),
            file_in_bd: bd,
            file_out_bd: bd,
            ..Default::default()
        };
        r.validate()?;
        Ok(r)
    }
}

// ---------------------------------------------------------------------------
// Reference

/// Reference process node (a path to another file or an alias).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ReferenceData {
    pub path: String,
    pub alias: String,
    pub dir: TransformDirection,
    pub metadata: FormatMetadata,
}

impl Default for ReferenceData {
    fn default() -> Self {
        Self {
            path: String::new(),
            alias: String::new(),
            dir: TransformDirection::Forward,
            metadata: FormatMetadata::default(),
        }
    }
}

// ---------------------------------------------------------------------------
// OpData

/// A CLF/CTF process node.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum OpData {
    Cdl(CdlData),
    ExposureContrast(EcData),
    FixedFunction(FfData),
    Gamma(GammaData),
    GradingPrimary(GradingPrimaryData),
    GradingRgbCurve(GradingRgbCurveData),
    GradingHueCurve(GradingHueCurveData),
    GradingTone(GradingToneData),
    Log(LogData),
    Lut1D(Lut1DData),
    Lut3D(Lut3DData),
    Matrix(MatrixData),
    Range(RangeData),
    Reference(ReferenceData),
}

impl OpData {
    /// Type name (port of `GetTypeName`).
    pub(crate) fn type_name(&self) -> &'static str {
        match self {
            OpData::Cdl(_) => "CDL",
            OpData::ExposureContrast(_) => "ExposureContrast",
            OpData::FixedFunction(_) => "FixedFunction",
            OpData::Gamma(_) => "Gamma",
            OpData::GradingPrimary(_) => "GradingPrimary",
            OpData::GradingRgbCurve(_) => "GradingRGBCurve",
            OpData::GradingHueCurve(_) => "GradingHueCurve",
            OpData::GradingTone(_) => "GradingTone",
            OpData::Log(_) => "Log",
            OpData::Lut1D(_) => "LUT1D",
            OpData::Lut3D(_) => "LUT3D",
            OpData::Matrix(_) => "Matrix",
            OpData::Range(_) => "Range",
            OpData::Reference(_) => "Reference",
        }
    }

    /// The format metadata of the process node.
    pub(crate) fn metadata(&self) -> &FormatMetadata {
        match self {
            OpData::Cdl(d) => &d.metadata,
            OpData::ExposureContrast(d) => &d.metadata,
            OpData::FixedFunction(d) => &d.metadata,
            OpData::Gamma(d) => &d.metadata,
            OpData::GradingPrimary(d) => &d.metadata,
            OpData::GradingRgbCurve(d) => &d.metadata,
            OpData::GradingHueCurve(d) => &d.metadata,
            OpData::GradingTone(d) => &d.metadata,
            OpData::Log(d) => &d.metadata,
            OpData::Lut1D(d) => &d.metadata,
            OpData::Lut3D(d) => &d.metadata,
            OpData::Matrix(d) => &d.metadata,
            OpData::Range(d) => &d.metadata,
            OpData::Reference(d) => &d.metadata,
        }
    }

    /// Validate the parameters (port of each `OpData::validate`).
    pub(crate) fn validate(&self) -> Result<()> {
        match self {
            OpData::Cdl(d) => d.validate(),
            OpData::ExposureContrast(_) => Ok(()),
            OpData::FixedFunction(d) => d.validate(),
            OpData::Gamma(d) => d.validate(),
            OpData::GradingPrimary(d) => validate_grading_primary(&d.value, d.style),
            OpData::GradingRgbCurve(d) => validate_rgb_curves(&d.value),
            OpData::GradingHueCurve(d) => validate_hue_curves(&d.value),
            OpData::GradingTone(d) => validate_grading_tone(&d.value),
            OpData::Log(d) => d.validate(),
            OpData::Lut1D(d) => d.validate(),
            OpData::Lut3D(d) => d.validate(),
            OpData::Matrix(d) => d.validate(),
            OpData::Range(d) => d.validate(),
            OpData::Reference(_) => Ok(()),
        }
    }

    /// Convert to a transform. References to a path become a
    /// `FileTransform`, alias references produce no transform.
    pub(crate) fn to_transform(&self) -> Option<Transform> {
        Some(match self {
            OpData::Cdl(d) => Transform::Cdl(CdlTransform {
                direction: d.style.direction(),
                style: d.style.transform_style(),
                slope: d.slope,
                offset: d.offset,
                power: d.power,
                sat: d.sat,
                metadata: d.metadata.clone(),
            }),
            OpData::ExposureContrast(d) => Transform::ExposureContrast(ExposureContrastTransform {
                direction: d.style.direction(),
                style: d.style.transform_style(),
                exposure: d.exposure,
                exposure_dynamic: d.exposure_dynamic,
                contrast: d.contrast,
                contrast_dynamic: d.contrast_dynamic,
                gamma: d.gamma,
                gamma_dynamic: d.gamma_dynamic,
                pivot: d.pivot,
                log_exposure_step: d.log_exposure_step,
                log_mid_gray: d.log_mid_gray,
                metadata: d.metadata.clone(),
            }),
            OpData::FixedFunction(d) => Transform::FixedFunction(FixedFunctionTransform {
                direction: d.dir,
                style: d.style,
                params: d.params.clone(),
                metadata: d.metadata.clone(),
            }),
            OpData::Gamma(d) => {
                let p = |c: usize, i: usize| {
                    d.params[c]
                        .get(i)
                        .copied()
                        .unwrap_or(if i == 0 { 1.0 } else { 0.0 })
                };
                if d.style.is_moncurve() {
                    Transform::ExponentWithLinear(ExponentWithLinearTransform {
                        direction: d.style.direction(),
                        gamma: [p(0, 0), p(1, 0), p(2, 0), p(3, 0)],
                        offset: [p(0, 1), p(1, 1), p(2, 1), p(3, 1)],
                        negative_style: d.style.negative_style(),
                        metadata: d.metadata.clone(),
                    })
                } else {
                    Transform::Exponent(ExponentTransform {
                        direction: d.style.direction(),
                        value: [p(0, 0), p(1, 0), p(2, 0), p(3, 0)],
                        negative_style: d.style.negative_style(),
                        metadata: d.metadata.clone(),
                    })
                }
            }
            OpData::GradingPrimary(d) => Transform::GradingPrimary(GradingPrimaryTransform {
                direction: d.dir,
                style: d.style,
                value: d.value,
                dynamic: d.dynamic,
                metadata: d.metadata.clone(),
            }),
            OpData::GradingRgbCurve(d) => Transform::GradingRgbCurve(GradingRgbCurveTransform {
                direction: d.dir,
                style: d.style,
                value: d.value.clone(),
                bypass_lin_to_log: d.bypass_lin_to_log,
                dynamic: d.dynamic,
                metadata: d.metadata.clone(),
            }),
            OpData::GradingHueCurve(d) => Transform::GradingHueCurve(GradingHueCurveTransform {
                direction: d.dir,
                style: d.style,
                value: d.value.clone(),
                rgb_to_hsy: d.rgb_to_hsy,
                dynamic: d.dynamic,
                metadata: d.metadata.clone(),
            }),
            OpData::GradingTone(d) => Transform::GradingTone(GradingToneTransform {
                direction: d.dir,
                style: d.style,
                value: d.value,
                dynamic: d.dynamic,
                metadata: d.metadata.clone(),
            }),
            OpData::Log(d) => {
                let chan = |i: usize| [d.params[0][i], d.params[1][i], d.params[2][i]];
                if d.is_camera() {
                    Transform::LogCamera(LogCameraTransform {
                        direction: d.dir,
                        base: d.base,
                        log_side_slope: chan(LOG_SIDE_SLOPE),
                        log_side_offset: chan(LOG_SIDE_OFFSET),
                        lin_side_slope: chan(LIN_SIDE_SLOPE),
                        lin_side_offset: chan(LIN_SIDE_OFFSET),
                        lin_side_break: chan(LIN_SIDE_BREAK),
                        linear_slope: if d.params[0].len() > LINEAR_SLOPE {
                            Some(chan(LINEAR_SLOPE))
                        } else {
                            None
                        },
                        metadata: d.metadata.clone(),
                    })
                } else if d.is_simple_log() {
                    Transform::Log(LogTransform {
                        direction: d.dir,
                        base: d.base,
                        metadata: d.metadata.clone(),
                    })
                } else {
                    Transform::LogAffine(LogAffineTransform {
                        direction: d.dir,
                        base: d.base,
                        log_side_slope: chan(LOG_SIDE_SLOPE),
                        log_side_offset: chan(LOG_SIDE_OFFSET),
                        lin_side_slope: chan(LIN_SIDE_SLOPE),
                        lin_side_offset: chan(LIN_SIDE_OFFSET),
                        metadata: d.metadata.clone(),
                    })
                }
            }
            OpData::Lut1D(d) => Transform::Lut1D(Lut1DTransform {
                direction: d.dir,
                values: d.values.clone(),
                input_half_domain: d.half_domain,
                output_raw_halfs: d.raw_halfs,
                hue_adjust: d.hue_adjust,
                interpolation: d.interpolation,
                file_output_bit_depth: d.file_output_bd,
                metadata: d.metadata.clone(),
            }),
            OpData::Lut3D(d) => Transform::Lut3D(Lut3DTransform {
                direction: d.dir,
                grid_size: d.grid_size,
                values: d.values.clone(),
                interpolation: d.interpolation,
                file_output_bit_depth: d.file_output_bd,
                metadata: d.metadata.clone(),
            }),
            OpData::Matrix(d) => Transform::Matrix(MatrixTransform {
                direction: d.dir,
                matrix: d.matrix,
                offset: d.offsets,
                file_input_bit_depth: d.file_in_bd,
                file_output_bit_depth: d.file_out_bd,
                metadata: d.metadata.clone(),
            }),
            OpData::Range(d) => {
                let opt = |v: f64| if is_empty_value(v) { None } else { Some(v) };
                Transform::Range(RangeTransform {
                    direction: d.dir,
                    style: RangeStyle::Clamp,
                    min_in: opt(d.min_in),
                    max_in: opt(d.max_in),
                    min_out: opt(d.min_out),
                    max_out: opt(d.max_out),
                    file_input_bit_depth: d.file_in_bd,
                    file_output_bit_depth: d.file_out_bd,
                    metadata: d.metadata.clone(),
                })
            }
            OpData::Reference(d) => {
                if d.path.is_empty() {
                    return None;
                }
                Transform::File(FileTransform {
                    direction: d.dir,
                    src: d.path.clone(),
                    interpolation: Interpolation::Default,
                    ..Default::default()
                })
            }
        })
    }

    /// Build the process node of a transform that is directly representable
    /// in CTF (port of the transform `BuildOps` + `Create*Op` functions).
    /// Returns `None` for transforms that need a config to be converted.
    pub(crate) fn from_transform(t: &Transform, dir: TransformDirection) -> Result<Option<OpData>> {
        let d = t.direction().combine(dir);
        Ok(Some(match t {
            Transform::Cdl(c) => {
                validate_cdl_params(&c.slope, &c.power, c.sat)
                    .map_err(|e| Error::msg(format!("CDLTransform validation failed: {e}")))?;
                OpData::Cdl(CdlData {
                    style: CdlOpStyle::from_transform(c.style, d),
                    slope: c.slope,
                    offset: c.offset,
                    power: c.power,
                    sat: c.sat,
                    metadata: c.metadata.clone(),
                })
            }
            Transform::Exponent(e) => {
                let style = GammaStyle::basic(e.negative_style, d)?;
                let g = GammaData {
                    style,
                    params: [
                        vec![e.value[0]],
                        vec![e.value[1]],
                        vec![e.value[2]],
                        vec![e.value[3]],
                    ],
                    metadata: e.metadata.clone(),
                };
                g.validate()?;
                OpData::Gamma(g)
            }
            Transform::ExponentWithLinear(e) => {
                let style = GammaStyle::moncurve(e.negative_style, d)?;
                let g = GammaData {
                    style,
                    params: [
                        vec![e.gamma[0], e.offset[0]],
                        vec![e.gamma[1], e.offset[1]],
                        vec![e.gamma[2], e.offset[2]],
                        vec![e.gamma[3], e.offset[3]],
                    ],
                    metadata: e.metadata.clone(),
                };
                g.validate()?;
                OpData::Gamma(g)
            }
            Transform::ExposureContrast(e) => OpData::ExposureContrast(EcData {
                style: EcOpStyle::from_transform(e.style, d),
                exposure: e.exposure,
                contrast: e.contrast,
                gamma: e.gamma,
                pivot: e.pivot,
                log_exposure_step: e.log_exposure_step,
                log_mid_gray: e.log_mid_gray,
                exposure_dynamic: e.exposure_dynamic,
                contrast_dynamic: e.contrast_dynamic,
                gamma_dynamic: e.gamma_dynamic,
                metadata: e.metadata.clone(),
            }),
            Transform::FixedFunction(f) => {
                // Check that the style can be represented.
                ff_style_to_name(f.style, d, false)?;
                let ff = FfData {
                    style: f.style,
                    dir: d,
                    params: f.params.clone(),
                    metadata: f.metadata.clone(),
                };
                ff.validate()?;
                OpData::FixedFunction(ff)
            }
            Transform::GradingPrimary(g) => {
                validate_grading_primary(&g.value, g.style)?;
                OpData::GradingPrimary(GradingPrimaryData {
                    style: g.style,
                    dir: d,
                    value: g.value,
                    dynamic: g.dynamic,
                    metadata: g.metadata.clone(),
                })
            }
            Transform::GradingRgbCurve(g) => {
                validate_rgb_curves(&g.value)?;
                OpData::GradingRgbCurve(GradingRgbCurveData {
                    style: g.style,
                    dir: d,
                    value: g.value.clone(),
                    bypass_lin_to_log: g.bypass_lin_to_log,
                    dynamic: g.dynamic,
                    metadata: g.metadata.clone(),
                })
            }
            Transform::GradingHueCurve(g) => {
                validate_hue_curves(&g.value)?;
                OpData::GradingHueCurve(GradingHueCurveData {
                    style: g.style,
                    dir: d,
                    value: g.value.clone(),
                    rgb_to_hsy: g.rgb_to_hsy,
                    dynamic: g.dynamic,
                    metadata: g.metadata.clone(),
                })
            }
            Transform::GradingTone(g) => {
                validate_grading_tone(&g.value)?;
                OpData::GradingTone(GradingToneData {
                    style: g.style,
                    dir: d,
                    value: g.value,
                    dynamic: g.dynamic,
                    metadata: g.metadata.clone(),
                })
            }
            Transform::Log(l) => {
                let mut log = LogData::new(l.base, d);
                log.metadata = l.metadata.clone();
                log.validate()?;
                OpData::Log(log)
            }
            Transform::LogAffine(l) => {
                let chan = |c: usize| {
                    vec![
                        l.log_side_slope[c],
                        l.log_side_offset[c],
                        l.lin_side_slope[c],
                        l.lin_side_offset[c],
                    ]
                };
                let log = LogData {
                    base: l.base,
                    dir: d,
                    params: [chan(0), chan(1), chan(2)],
                    metadata: l.metadata.clone(),
                };
                log.validate()?;
                OpData::Log(log)
            }
            Transform::LogCamera(l) => {
                let chan = |c: usize| {
                    let mut v = vec![
                        l.log_side_slope[c],
                        l.log_side_offset[c],
                        l.lin_side_slope[c],
                        l.lin_side_offset[c],
                        l.lin_side_break[c],
                    ];
                    if let Some(ls) = l.linear_slope {
                        v.push(ls[c]);
                    }
                    v
                };
                let log = LogData {
                    base: l.base,
                    dir: d,
                    params: [chan(0), chan(1), chan(2)],
                    metadata: l.metadata.clone(),
                };
                log.validate()?;
                OpData::Log(log)
            }
            Transform::Lut1D(l) => {
                let length = l.values.len() / 3;
                let mut lut = Lut1DData {
                    dir: d,
                    interpolation: l.interpolation,
                    half_domain: l.input_half_domain,
                    raw_halfs: l.output_raw_halfs,
                    hue_adjust: l.hue_adjust,
                    file_output_bd: l.file_output_bit_depth,
                    length,
                    num_components: 3,
                    values: l.values.clone(),
                    metadata: l.metadata.clone(),
                };
                lut.values.truncate(length * 3);
                lut.validate()?;
                OpData::Lut1D(lut)
            }
            Transform::Lut3D(l) => {
                let lut = Lut3DData {
                    dir: d,
                    interpolation: l.interpolation,
                    file_output_bd: l.file_output_bit_depth,
                    grid_size: l.grid_size,
                    num_components: 3,
                    values: l.values.clone(),
                    metadata: l.metadata.clone(),
                };
                lut.validate()?;
                OpData::Lut3D(lut)
            }
            Transform::Matrix(m) => {
                let mat = MatrixData {
                    dir: d,
                    matrix: m.matrix,
                    offsets: m.offset,
                    file_in_bd: m.file_input_bit_depth,
                    file_out_bd: m.file_output_bit_depth,
                    metadata: m.metadata.clone(),
                };
                OpData::Matrix(mat)
            }
            Transform::Range(r) => {
                let v = |o: Option<f64>| o.unwrap_or(f64::NAN);
                let range = RangeData {
                    dir: r.direction,
                    min_in: v(r.min_in),
                    max_in: v(r.max_in),
                    min_out: v(r.min_out),
                    max_out: v(r.max_out),
                    file_in_bd: r.file_input_bit_depth,
                    file_out_bd: r.file_output_bit_depth,
                    metadata: r.metadata.clone(),
                };
                range.validate()?;
                if r.style == RangeStyle::Clamp {
                    let mut range = range;
                    range.dir = r.direction.combine(dir);
                    OpData::Range(range)
                } else {
                    let mut m = range.convert_to_matrix()?;
                    m.dir = dir;
                    OpData::Matrix(m)
                }
            }
            _ => return Ok(None),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_mapping_accessors() {
        let mut r = IndexMapping::new(4);
        r.set_pair(0, 0.0, 0.0).unwrap();
        r.set_pair(1, 100.0, 1.0).unwrap();
        r.set_pair(2, 200.0, 2.0).unwrap();
        r.set_pair(3, 300.0, 3.0).unwrap();
        let e = r.set_pair(5, 300.0, 3.0).unwrap_err();
        assert!(e.message().contains("invalid. Should be less than"));
        r.validate().unwrap();
        assert_eq!(r.dimension(), 4);
        assert_eq!(r.pair(0).unwrap(), (0.0, 0.0));
        assert_eq!(r.pair(1).unwrap(), (100.0, 1.0));
        assert_eq!(r.pair(2).unwrap(), (200.0, 2.0));
        assert_eq!(r.pair(3).unwrap(), (300.0, 3.0));
        r.resize(8);
        assert_eq!(r.dimension(), 8);
    }

    #[test]
    fn index_mapping_range_validation() {
        let mut r = IndexMapping::new(4);
        r.set_pair(0, 0.0, 0.0).unwrap();
        r.set_pair(1, 100.0, 1.0).unwrap();
        r.set_pair(2, 200.0, 2.0).unwrap();
        r.set_pair(3, 200.0, 3.0).unwrap();
        assert!(r
            .validate()
            .unwrap_err()
            .message()
            .contains("Index values must be increasing"));
        r.set_pair(3, 300.0, 2.0).unwrap();
        assert!(r
            .validate()
            .unwrap_err()
            .message()
            .contains("Index values must be increasing"));
    }

    #[test]
    fn index_mapping_equality() {
        let mut r1 = IndexMapping::new(4);
        let mut r2 = IndexMapping::new(4);
        for (i, (a, b)) in [(0.0, 0.0), (100.0, 1.0), (200.0, 2.0), (300.0, 3.0)]
            .iter()
            .enumerate()
        {
            r1.set_pair(i, *a, *b).unwrap();
            r2.set_pair(i, *a, *b).unwrap();
        }
        assert_eq!(r1, r2);
        r2.set_pair(2, 200.0, 2.1).unwrap();
        assert_ne!(r1, r2);
    }

    #[test]
    fn matrix_inverse_test() {
        let m = [
            2.0, 0.0, 0.0, 0.0, 0.0, 4.0, 0.0, 0.0, 0.0, 0.0, 8.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        let inv = matrix_inverse(&m).unwrap();
        assert_eq!(inv[0], 0.5);
        assert_eq!(inv[5], 0.25);
        assert_eq!(inv[10], 0.125);
        assert!(matrix_inverse(&[0.0; 16]).is_err());
    }

    #[test]
    fn range_validation() {
        let r = RangeData {
            min_in: 0.0,
            min_out: 0.5,
            ..Default::default()
        };
        assert!(r
            .validate()
            .unwrap_err()
            .message()
            .contains("In and out minimum limits must be equal"));
        let r = RangeData::default();
        assert!(r
            .validate()
            .unwrap_err()
            .message()
            .contains("At least minimum or maximum limits must be set"));
    }
}
