//! Core enumerations, string conversions and constants (port of `OpenColorTypes.h`
//! and the string helpers of `ParseUtils.cpp`).

use crate::error::{Error, Result};
use std::fmt;

/// Implement `Display` using an existing `as_str()` method.
macro_rules! impl_display_as_str {
    ($t:ty) => {
        impl fmt::Display for $t {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

/// Convert a bool to `"true"`/`"false"`.
pub fn bool_to_string(v: bool) -> &'static str {
    if v {
        "true"
    } else {
        "false"
    }
}

/// `"true"` or `"yes"` (case insensitive) are true, everything else is false.
pub fn bool_from_string(s: &str) -> bool {
    let s = s.to_ascii_lowercase();
    s == "true" || s == "yes"
}

// ---------------------------------------------------------------------------

/// Logging verbosity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LoggingLevel {
    None = 0,
    Warning = 1,
    Info = 2,
    Debug = 3,
    Unknown = 255,
}

impl Default for LoggingLevel {
    fn default() -> Self {
        LoggingLevel::Info
    }
}

impl LoggingLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            LoggingLevel::None => "none",
            LoggingLevel::Warning => "warning",
            LoggingLevel::Info => "info",
            LoggingLevel::Debug => "debug",
            LoggingLevel::Unknown => "unknown",
        }
    }
    pub fn from_str_lossy(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "0" | "none" => LoggingLevel::None,
            "1" | "warning" => LoggingLevel::Warning,
            "2" | "info" => LoggingLevel::Info,
            "3" | "debug" => LoggingLevel::Debug,
            _ => LoggingLevel::Unknown,
        }
    }
}
impl_display_as_str!(LoggingLevel);

/// Scene vs display referred reference space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ReferenceSpaceType {
    #[default]
    Scene,
    Display,
}

/// Which reference space to search in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SearchReferenceSpaceType {
    #[default]
    Scene,
    Display,
    All,
}

/// Color space visibility filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ColorSpaceVisibility {
    #[default]
    Active,
    Inactive,
    All,
}

/// Named transform visibility filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum NamedTransformVisibility {
    #[default]
    Active,
    Inactive,
    All,
}

/// Shared vs display-defined views.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ViewType {
    #[default]
    Shared,
    DisplayDefined,
}

/// Direction of a color space transform relative to the reference space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ColorSpaceDirection {
    #[default]
    ToReference,
    FromReference,
}

/// Direction of a view transform relative to the reference space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ViewTransformDirection {
    #[default]
    ToReference,
    FromReference,
}

/// Direction of a transform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TransformDirection {
    #[default]
    Forward,
    Inverse,
}

impl TransformDirection {
    pub fn as_str(&self) -> &'static str {
        match self {
            TransformDirection::Forward => "forward",
            TransformDirection::Inverse => "inverse",
        }
    }
    /// Parse `"forward"` / `"inverse"` (case insensitive).
    pub fn parse(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "forward" => Ok(TransformDirection::Forward),
            "inverse" => Ok(TransformDirection::Inverse),
            _ => Err(Error::msg(format!("Unrecognized transform direction: '{s}'."))),
        }
    }
    /// The opposite direction.
    pub fn inverse(self) -> Self {
        match self {
            TransformDirection::Forward => TransformDirection::Inverse,
            TransformDirection::Inverse => TransformDirection::Forward,
        }
    }
    /// Combine two directions (forward∘forward = forward, inverse∘inverse = forward).
    pub fn combine(self, other: TransformDirection) -> Self {
        if self == other {
            TransformDirection::Forward
        } else {
            TransformDirection::Inverse
        }
    }
}
impl_display_as_str!(TransformDirection);

/// Free function equivalent of [`TransformDirection::combine`].
pub fn combine_transform_directions(d1: TransformDirection, d2: TransformDirection) -> TransformDirection {
    d1.combine(d2)
}

/// Free function equivalent of [`TransformDirection::inverse`].
pub fn get_inverse_transform_direction(d: TransformDirection) -> TransformDirection {
    d.inverse()
}

/// Kind of transform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransformType {
    Allocation,
    Builtin,
    Cdl,
    ColorSpace,
    DisplayView,
    Exponent,
    ExponentWithLinear,
    ExposureContrast,
    File,
    FixedFunction,
    GradingHueCurve,
    GradingPrimary,
    GradingRgbCurve,
    GradingTone,
    Group,
    LogAffine,
    LogCamera,
    Log,
    Look,
    Lut1D,
    Lut3D,
    Matrix,
    Range,
}

/// LUT interpolation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Interpolation {
    Unknown = 0,
    Nearest = 1,
    Linear = 2,
    Tetrahedral = 3,
    Cubic = 4,
    #[default]
    Default = 254,
    Best = 255,
}

impl Interpolation {
    pub fn as_str(&self) -> &'static str {
        match self {
            Interpolation::Nearest => "nearest",
            Interpolation::Linear => "linear",
            Interpolation::Tetrahedral => "tetrahedral",
            Interpolation::Best => "best",
            Interpolation::Default => "default",
            Interpolation::Cubic => "cubic",
            Interpolation::Unknown => "unknown",
        }
    }
    /// Unknown strings map to [`Interpolation::Unknown`] (as in OCIO).
    pub fn from_str_lossy(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "nearest" => Interpolation::Nearest,
            "linear" => Interpolation::Linear,
            "tetrahedral" => Interpolation::Tetrahedral,
            "best" => Interpolation::Best,
            "cubic" => Interpolation::Cubic,
            "default" => Interpolation::Default,
            _ => Interpolation::Unknown,
        }
    }
}
impl_display_as_str!(Interpolation);

/// Pixel / file bit depths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BitDepth {
    #[default]
    Unknown,
    UInt8,
    UInt10,
    UInt12,
    UInt14,
    UInt16,
    UInt32,
    F16,
    F32,
}

impl BitDepth {
    pub fn as_str(&self) -> &'static str {
        match self {
            BitDepth::UInt8 => "8ui",
            BitDepth::UInt10 => "10ui",
            BitDepth::UInt12 => "12ui",
            BitDepth::UInt14 => "14ui",
            BitDepth::UInt16 => "16ui",
            BitDepth::UInt32 => "32ui",
            BitDepth::F16 => "16f",
            BitDepth::F32 => "32f",
            BitDepth::Unknown => "unknown",
        }
    }
    pub fn from_str_lossy(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "8ui" => BitDepth::UInt8,
            "10ui" => BitDepth::UInt10,
            "12ui" => BitDepth::UInt12,
            "14ui" => BitDepth::UInt14,
            "16ui" => BitDepth::UInt16,
            "32ui" => BitDepth::UInt32,
            "16f" => BitDepth::F16,
            "32f" => BitDepth::F32,
            _ => BitDepth::Unknown,
        }
    }
    pub fn is_float(&self) -> bool {
        matches!(self, BitDepth::F16 | BitDepth::F32)
    }
    /// Number of bits for integer depths, 0 for float / unknown.
    pub fn to_int(&self) -> i32 {
        match self {
            BitDepth::UInt8 => 8,
            BitDepth::UInt10 => 10,
            BitDepth::UInt12 => 12,
            BitDepth::UInt14 => 14,
            BitDepth::UInt16 => 16,
            BitDepth::UInt32 => 32,
            _ => 0,
        }
    }
    /// Maximum code value of the bit depth (1.0 for float depths), as in
    /// OCIO's `GetBitDepthMaxValue`.
    pub fn max_value(&self) -> f64 {
        match self {
            BitDepth::UInt8 => 255.0,
            BitDepth::UInt10 => 1023.0,
            BitDepth::UInt12 => 4095.0,
            BitDepth::UInt14 => 16383.0,
            BitDepth::UInt16 => 65535.0,
            BitDepth::UInt32 => 4294967295.0,
            BitDepth::F16 | BitDepth::F32 => 1.0,
            BitDepth::Unknown => 1.0,
        }
    }
}
impl_display_as_str!(BitDepth);

/// Hue adjustment for 1D LUTs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Lut1DHueAdjust {
    #[default]
    None,
    Dw3,
    Wypn,
}

/// Channel ordering of packed images.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ChannelOrdering {
    #[default]
    Rgba,
    Bgra,
    Abgr,
    Rgb,
    Bgr,
}

impl ChannelOrdering {
    pub fn num_channels(&self) -> usize {
        match self {
            ChannelOrdering::Rgb | ChannelOrdering::Bgr => 3,
            _ => 4,
        }
    }
}

/// Allocation for `AllocationTransform`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Allocation {
    Unknown,
    #[default]
    Uniform,
    Lg2,
}

impl Allocation {
    pub fn as_str(&self) -> &'static str {
        match self {
            Allocation::Uniform => "uniform",
            Allocation::Lg2 => "lg2",
            Allocation::Unknown => "unknown",
        }
    }
    pub fn from_str_lossy(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "uniform" => Allocation::Uniform,
            "lg2" => Allocation::Lg2,
            _ => Allocation::Unknown,
        }
    }
}
impl_display_as_str!(Allocation);

/// GPU shading languages (kept for API compatibility).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GpuLanguage {
    Cg,
    Glsl1_2,
    Glsl1_3,
    #[default]
    Glsl4_0,
    GlslVk4_6,
    HlslSm5_0,
    Osl1,
    GlslEs1_0,
    GlslEs3_0,
    Msl2_0,
}

impl GpuLanguage {
    pub fn as_str(&self) -> &'static str {
        match self {
            GpuLanguage::Cg => "cg",
            GpuLanguage::Glsl1_2 => "glsl_1.2",
            GpuLanguage::Glsl1_3 => "glsl_1.3",
            GpuLanguage::Glsl4_0 => "glsl_4.0",
            GpuLanguage::GlslVk4_6 => "glsl_vk_4.6",
            GpuLanguage::GlslEs1_0 => "glsl_es_1.0",
            GpuLanguage::GlslEs3_0 => "glsl_es_3.0",
            GpuLanguage::HlslSm5_0 => "hlsl_sm_5.0",
            GpuLanguage::Msl2_0 => "msl_2",
            GpuLanguage::Osl1 => "osl_1",
        }
    }
    pub fn parse(s: &str) -> Result<Self> {
        Ok(match s.to_ascii_lowercase().as_str() {
            "cg" => GpuLanguage::Cg,
            "glsl_1.2" => GpuLanguage::Glsl1_2,
            "glsl_1.3" => GpuLanguage::Glsl1_3,
            "glsl_4.0" => GpuLanguage::Glsl4_0,
            "glsl_vk_4.6" => GpuLanguage::GlslVk4_6,
            "glsl_es_1.0" => GpuLanguage::GlslEs1_0,
            "glsl_es_3.0" => GpuLanguage::GlslEs3_0,
            "hlsl_sm_5.0" => GpuLanguage::HlslSm5_0,
            "osl_1" => GpuLanguage::Osl1,
            "msl_2" => GpuLanguage::Msl2_0,
            _ => return Err(Error::msg(format!("Unsupported GPU shader language: '{s}'."))),
        })
    }
}
impl_display_as_str!(GpuLanguage);

/// How environment variables are loaded into a context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum EnvironmentMode {
    Unknown,
    #[default]
    LoadPredefined,
    LoadAll,
}

impl EnvironmentMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            EnvironmentMode::LoadPredefined => "loadpredefined",
            EnvironmentMode::LoadAll => "loadall",
            EnvironmentMode::Unknown => "unknown",
        }
    }
    pub fn from_str_lossy(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "loadpredefined" => EnvironmentMode::LoadPredefined,
            "loadall" => EnvironmentMode::LoadAll,
            _ => EnvironmentMode::Unknown,
        }
    }
}
impl_display_as_str!(EnvironmentMode);

/// Clamping behavior of a `RangeTransform`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum RangeStyle {
    NoClamp,
    #[default]
    Clamp,
}

impl RangeStyle {
    pub fn as_str(&self) -> &'static str {
        match self {
            RangeStyle::NoClamp => "noClamp",
            RangeStyle::Clamp => "Clamp",
        }
    }
    pub fn parse(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "noclamp" => Ok(RangeStyle::NoClamp),
            "clamp" => Ok(RangeStyle::Clamp),
            _ => Err(Error::msg(format!("Wrong Range style '{s}'."))),
        }
    }
}
impl_display_as_str!(RangeStyle);

/// Fixed function styles.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FixedFunctionStyle {
    AcesRedMod03,
    AcesRedMod10,
    AcesGlow03,
    AcesGlow10,
    AcesDarkToDim10,
    Rec2100Surround,
    RgbToHsv,
    XyzToXyy,
    XyzToUvy,
    XyzToLuv,
    AcesGamutMap02,
    AcesGamutMap07,
    AcesGamutComp13,
    LinToPq,
    LinToGammaLog,
    LinToDoubleLog,
    AcesOutputTransform20,
    AcesRgbToJmh20,
    AcesTonescaleCompress20,
    AcesGamutCompress20,
    AcesRgbToHmj20,
    RgbToHsyLin,
    RgbToHsyLog,
    RgbToHsyVid,
}

impl FixedFunctionStyle {
    pub fn as_str(&self) -> &'static str {
        use FixedFunctionStyle::*;
        match self {
            AcesRedMod03 => "ACES_RedMod03",
            AcesRedMod10 => "ACES_RedMod10",
            AcesGlow03 => "ACES_Glow03",
            AcesGlow10 => "ACES_Glow10",
            AcesDarkToDim10 => "ACES_DarkToDim10",
            AcesGamutComp13 => "ACES_GamutComp13",
            AcesOutputTransform20 => "ACES2_OutputTransform",
            AcesRgbToJmh20 => "ACES2_RGB_TO_JMh",
            AcesRgbToHmj20 => "ACES2_RGB_TO_HMJ",
            AcesTonescaleCompress20 => "ACES2_TonescaleCompress",
            AcesGamutCompress20 => "ACES2_GamutCompress",
            Rec2100Surround => "REC2100_Surround",
            RgbToHsv => "RGB_TO_HSV",
            XyzToXyy => "XYZ_TO_xyY",
            XyzToUvy => "XYZ_TO_uvY",
            XyzToLuv => "XYZ_TO_LUV",
            LinToPq => "Lin_TO_PQ",
            LinToGammaLog => "Lin_TO_GammaLog",
            LinToDoubleLog => "Lin_TO_DoubleLog",
            RgbToHsyLin => "RGB_TO_HSY_LIN",
            RgbToHsyLog => "RGB_TO_HSY_LOG",
            RgbToHsyVid => "RGB_TO_HSY_VID",
            AcesGamutMap02 => "ACES_GamutMap02",
            AcesGamutMap07 => "ACES_GamutMap07",
        }
    }
    pub fn parse(s: &str) -> Result<Self> {
        use FixedFunctionStyle::*;
        Ok(match s.to_ascii_lowercase().as_str() {
            "aces_redmod03" => AcesRedMod03,
            "aces_redmod10" => AcesRedMod10,
            "aces_glow03" => AcesGlow03,
            "aces_glow10" => AcesGlow10,
            "aces_darktodim10" => AcesDarkToDim10,
            "aces_gamutcomp13" => AcesGamutComp13,
            "aces2_outputtransform" => AcesOutputTransform20,
            "aces2_rgb_to_jmh" => AcesRgbToJmh20,
            "aces2_rgb_to_hmj" => AcesRgbToHmj20,
            "aces2_tonescalecompress" => AcesTonescaleCompress20,
            "aces2_gamutcompress" => AcesGamutCompress20,
            "rec2100_surround" => Rec2100Surround,
            "rgb_to_hsv" => RgbToHsv,
            "xyz_to_xyy" => XyzToXyy,
            "xyz_to_uvy" => XyzToUvy,
            "xyz_to_luv" => XyzToLuv,
            "lin_to_pq" => LinToPq,
            "lin_to_gammalog" => LinToGammaLog,
            "lin_to_doublelog" => LinToDoubleLog,
            "rgb_to_hsy_lin" => RgbToHsyLin,
            "rgb_to_hsy_log" => RgbToHsyLog,
            "rgb_to_hsy_vid" => RgbToHsyVid,
            _ => return Err(Error::msg(format!("Unknown Fixed FunctionOp style: '{s}'."))),
        })
    }
}
impl_display_as_str!(FixedFunctionStyle);

/// Exposure/contrast styles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ExposureContrastStyle {
    #[default]
    Linear,
    Video,
    Logarithmic,
}

impl ExposureContrastStyle {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExposureContrastStyle::Video => "video",
            ExposureContrastStyle::Logarithmic => "log",
            ExposureContrastStyle::Linear => "linear",
        }
    }
    pub fn parse(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "linear" => Ok(ExposureContrastStyle::Linear),
            "video" => Ok(ExposureContrastStyle::Video),
            "log" => Ok(ExposureContrastStyle::Logarithmic),
            _ => Err(Error::msg(format!("Unknown exposure contrast style: '{s}'."))),
        }
    }
}
impl_display_as_str!(ExposureContrastStyle);

/// CDL styles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CdlStyle {
    Asc,
    /// Default style of `CDLTransform` (`CDL_TRANSFORM_DEFAULT`).
    #[default]
    NoClamp,
}

impl CdlStyle {
    pub fn as_str(&self) -> &'static str {
        match self {
            CdlStyle::Asc => "asc",
            CdlStyle::NoClamp => "noClamp",
        }
    }
    pub fn parse(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "asc" => Ok(CdlStyle::Asc),
            "noclamp" => Ok(CdlStyle::NoClamp),
            _ => Err(Error::msg(format!("Wrong CDL style: '{s}'."))),
        }
    }
}
impl_display_as_str!(CdlStyle);

/// Negative value handling for exponent transforms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum NegativeStyle {
    #[default]
    Clamp,
    Mirror,
    PassThru,
    Linear,
}

impl NegativeStyle {
    pub fn as_str(&self) -> &'static str {
        match self {
            NegativeStyle::Clamp => "clamp",
            NegativeStyle::Mirror => "mirror",
            NegativeStyle::PassThru => "pass_thru",
            NegativeStyle::Linear => "linear",
        }
    }
    pub fn parse(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "mirror" => Ok(NegativeStyle::Mirror),
            "pass_thru" => Ok(NegativeStyle::PassThru),
            "clamp" => Ok(NegativeStyle::Clamp),
            "linear" => Ok(NegativeStyle::Linear),
            _ => Err(Error::msg(format!("Unknown exponent style: '{s}'."))),
        }
    }
}
impl_display_as_str!(NegativeStyle);

/// Grading styles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GradingStyle {
    #[default]
    Log,
    Lin,
    Video,
}

impl GradingStyle {
    pub fn as_str(&self) -> &'static str {
        match self {
            GradingStyle::Lin => "linear",
            GradingStyle::Log => "log",
            GradingStyle::Video => "video",
        }
    }
    pub fn parse(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "linear" => Ok(GradingStyle::Lin),
            "log" => Ok(GradingStyle::Log),
            "video" => Ok(GradingStyle::Video),
            _ => Err(Error::msg(format!("Unknown grading style: '{s}'."))),
        }
    }
}
impl_display_as_str!(GradingStyle);

/// Types of dynamic properties.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DynamicPropertyType {
    Exposure,
    Contrast,
    Gamma,
    GradingPrimary,
    GradingRgbCurve,
    GradingTone,
    GradingHueCurve,
}

/// RGB curve selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RgbCurveType {
    Red = 0,
    Green = 1,
    Blue = 2,
    Master = 3,
}

impl RgbCurveType {
    pub const ALL: [RgbCurveType; 4] =
        [RgbCurveType::Red, RgbCurveType::Green, RgbCurveType::Blue, RgbCurveType::Master];
}

/// Hue curve selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HueCurveType {
    HueHue = 0,
    HueSat,
    HueLum,
    LumSat,
    SatSat,
    LumLum,
    SatLum,
    HueFx,
}

impl HueCurveType {
    pub const ALL: [HueCurveType; 8] = [
        HueCurveType::HueHue,
        HueCurveType::HueSat,
        HueCurveType::HueLum,
        HueCurveType::LumSat,
        HueCurveType::SatSat,
        HueCurveType::LumLum,
        HueCurveType::SatLum,
        HueCurveType::HueFx,
    ];
}

/// RGB to HSY conversion used by the hue curve transform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum HsyTransformStyle {
    None,
    #[default]
    Hsy1,
}

/// B-spline curve type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BSplineType {
    #[default]
    BSpline,
    DiagonalBSpline,
    HueHueBSpline,
    Periodic1BSpline,
    Periodic0BSpline,
    Horizontal1BSpline,
}

/// Optimization flags (bit field, see `OptimizationFlags` in OCIO).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OptimizationFlags(pub u32);

impl OptimizationFlags {
    pub const NONE: Self = Self(0x0000_0000);
    pub const IDENTITY: Self = Self(0x0000_0001);
    pub const IDENTITY_GAMMA: Self = Self(0x0000_0002);
    pub const PAIR_IDENTITY_CDL: Self = Self(0x0000_0040);
    pub const PAIR_IDENTITY_EXPOSURE_CONTRAST: Self = Self(0x0000_0080);
    pub const PAIR_IDENTITY_FIXED_FUNCTION: Self = Self(0x0000_0100);
    pub const PAIR_IDENTITY_GAMMA: Self = Self(0x0000_0200);
    pub const PAIR_IDENTITY_LUT1D: Self = Self(0x0000_0400);
    pub const PAIR_IDENTITY_LUT3D: Self = Self(0x0000_0800);
    pub const PAIR_IDENTITY_LOG: Self = Self(0x0000_1000);
    pub const PAIR_IDENTITY_GRADING: Self = Self(0x0000_2000);
    pub const COMP_EXPONENT: Self = Self(0x0004_0000);
    pub const COMP_GAMMA: Self = Self(0x0008_0000);
    pub const COMP_MATRIX: Self = Self(0x0010_0000);
    pub const COMP_LUT1D: Self = Self(0x0020_0000);
    pub const COMP_LUT3D: Self = Self(0x0040_0000);
    pub const COMP_RANGE: Self = Self(0x0080_0000);
    pub const COMP_SEPARABLE_PREFIX: Self = Self(0x0100_0000);
    pub const LUT_INV_FAST: Self = Self(0x0200_0000);
    pub const FAST_LOG_EXP_POW: Self = Self(0x0400_0000);
    pub const SIMPLIFY_OPS: Self = Self(0x0800_0000);
    pub const NO_DYNAMIC_PROPERTIES: Self = Self(0x1000_0000);
    pub const ALL: Self = Self(0xFFFF_FFFF);

    pub const LOSSLESS: Self = Self(
        Self::IDENTITY.0
            | Self::IDENTITY_GAMMA.0
            | Self::PAIR_IDENTITY_CDL.0
            | Self::PAIR_IDENTITY_EXPOSURE_CONTRAST.0
            | Self::PAIR_IDENTITY_FIXED_FUNCTION.0
            | Self::PAIR_IDENTITY_GAMMA.0
            | Self::PAIR_IDENTITY_GRADING.0
            | Self::PAIR_IDENTITY_LOG.0
            | Self::PAIR_IDENTITY_LUT1D.0
            | Self::PAIR_IDENTITY_LUT3D.0
            | Self::COMP_EXPONENT.0
            | Self::COMP_GAMMA.0
            | Self::COMP_MATRIX.0
            | Self::COMP_RANGE.0
            | Self::SIMPLIFY_OPS.0,
    );
    pub const VERY_GOOD: Self = Self(
        Self::LOSSLESS.0
            | Self::COMP_LUT1D.0
            | Self::LUT_INV_FAST.0
            | Self::FAST_LOG_EXP_POW.0
            | Self::COMP_SEPARABLE_PREFIX.0,
    );
    pub const GOOD: Self = Self(Self::VERY_GOOD.0 | Self::COMP_LUT3D.0);
    pub const DRAFT: Self = Self::ALL;
    pub const DEFAULT: Self = Self::VERY_GOOD;

    /// True if every bit of `other` is set in `self`.
    pub fn contains(&self, other: OptimizationFlags) -> bool {
        (self.0 & other.0) == other.0
    }
    /// True if any bit of `other` is set in `self`.
    pub fn intersects(&self, other: OptimizationFlags) -> bool {
        (self.0 & other.0) != 0
    }
}

impl Default for OptimizationFlags {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl std::ops::BitOr for OptimizationFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl std::ops::BitAnd for OptimizationFlags {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self {
        Self(self.0 & rhs.0)
    }
}

impl std::ops::Not for OptimizationFlags {
    type Output = Self;
    fn not(self) -> Self {
        Self(!self.0)
    }
}

/// Processor cache flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProcessorCacheFlags(pub u32);

impl ProcessorCacheFlags {
    pub const OFF: Self = Self(0x00);
    pub const ENABLED: Self = Self(0x01);
    pub const SHARE_DYN_PROPERTIES: Self = Self(0x02);
    pub const DEFAULT: Self = Self(0x03);
}

impl Default for ProcessorCacheFlags {
    fn default() -> Self {
        Self::DEFAULT
    }
}

// ---------------------------------------------------------------------------
// Constants

pub const OCIO_CONFIG_ENVVAR: &str = "OCIO";
pub const OCIO_ACTIVE_DISPLAYS_ENVVAR: &str = "OCIO_ACTIVE_DISPLAYS";
pub const OCIO_ACTIVE_VIEWS_ENVVAR: &str = "OCIO_ACTIVE_VIEWS";
pub const OCIO_INACTIVE_COLORSPACES_ENVVAR: &str = "OCIO_INACTIVE_COLORSPACES";
pub const OCIO_OPTIMIZATION_FLAGS_ENVVAR: &str = "OCIO_OPTIMIZATION_FLAGS";
pub const OCIO_USER_CATEGORIES_ENVVAR: &str = "OCIO_USER_CATEGORIES";
pub const OCIO_LOGGING_LEVEL_ENVVAR: &str = "OCIO_LOGGING_LEVEL";

pub const OCIO_CONFIG_DEFAULT_NAME: &str = "config";
pub const OCIO_CONFIG_DEFAULT_FILE_EXT: &str = ".ocio";
pub const OCIO_CONFIG_ARCHIVE_FILE_EXT: &str = ".ocioz";
pub const OCIO_VIEW_USE_DISPLAY_NAME: &str = "<USE_DISPLAY_NAME>";
pub const OCIO_BUILTIN_URI_PREFIX: &str = "ocio://";

pub const OCIO_DISABLE_ALL_CACHES: &str = "OCIO_DISABLE_ALL_CACHES";
pub const OCIO_DISABLE_PROCESSOR_CACHES: &str = "OCIO_DISABLE_PROCESSOR_CACHES";
pub const OCIO_DISABLE_CACHE_FALLBACK: &str = "OCIO_DISABLE_CACHE_FALLBACK";

pub const ROLE_DEFAULT: &str = "default";
pub const ROLE_REFERENCE: &str = "reference";
pub const ROLE_DATA: &str = "data";
pub const ROLE_COLOR_PICKING: &str = "color_picking";
pub const ROLE_SCENE_LINEAR: &str = "scene_linear";
pub const ROLE_COMPOSITING_LOG: &str = "compositing_log";
pub const ROLE_COLOR_TIMING: &str = "color_timing";
pub const ROLE_TEXTURE_PAINT: &str = "texture_paint";
pub const ROLE_MATTE_PAINT: &str = "matte_paint";
pub const ROLE_RENDERING: &str = "rendering";
pub const ROLE_INTERCHANGE_SCENE: &str = "aces_interchange";
pub const ROLE_INTERCHANGE_DISPLAY: &str = "cie_xyz_d65_interchange";

pub const METADATA_DESCRIPTION: &str = "Description";
pub const METADATA_INFO: &str = "Info";
pub const METADATA_INPUT_DESCRIPTOR: &str = "InputDescriptor";
pub const METADATA_OUTPUT_DESCRIPTOR: &str = "OutputDescriptor";
pub const METADATA_NAME: &str = "name";
pub const METADATA_ID: &str = "id";
pub const METADATA_ID_ELEMENT: &str = "Id";
