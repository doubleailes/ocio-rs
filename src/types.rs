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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub enum LoggingLevel {
    /// No logging.
    None = 0,
    /// Only warnings.
    Warning = 1,
    /// Informational messages (default).
    #[default]
    Info = 2,
    /// Debug messages.
    Debug = 3,
    /// Unrecognized level.
    Unknown = 255,
}

impl LoggingLevel {
    /// Lowercase name (`"none"`, `"warning"`, `"info"`, `"debug"`, `"unknown"`).
    pub fn as_str(&self) -> &'static str {
        match self {
            LoggingLevel::None => "none",
            LoggingLevel::Warning => "warning",
            LoggingLevel::Info => "info",
            LoggingLevel::Debug => "debug",
            LoggingLevel::Unknown => "unknown",
        }
    }
    /// Parse a name or its numeric value `"0"`..`"3"` (case insensitive); anything else is `Unknown`.
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
    /// Scene-referred reference space (default).
    #[default]
    Scene,
    /// Display-referred reference space.
    Display,
}

/// Which reference space to search in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SearchReferenceSpaceType {
    /// Search the scene-referred reference space (default).
    #[default]
    Scene,
    /// Search the display-referred reference space.
    Display,
    /// Search both reference spaces.
    All,
}

/// Color space visibility filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ColorSpaceVisibility {
    /// Only active color spaces (default).
    #[default]
    Active,
    /// Only inactive color spaces.
    Inactive,
    /// All color spaces.
    All,
}

/// Named transform visibility filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum NamedTransformVisibility {
    /// Only active named transforms (default).
    #[default]
    Active,
    /// Only inactive named transforms.
    Inactive,
    /// All named transforms.
    All,
}

/// Shared vs display-defined views.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ViewType {
    /// Views from the shared `views` list (default).
    #[default]
    Shared,
    /// Views defined directly under a display.
    DisplayDefined,
}

/// Direction of a color space transform relative to the reference space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ColorSpaceDirection {
    /// From the color space to the reference space (default).
    #[default]
    ToReference,
    /// From the reference space to the color space.
    FromReference,
}

/// Direction of a view transform relative to the reference space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ViewTransformDirection {
    /// From the view transform's space to the reference space (default).
    #[default]
    ToReference,
    /// From the reference space to the view transform's space.
    FromReference,
}

/// Direction of a transform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TransformDirection {
    /// Forward direction (default).
    #[default]
    Forward,
    /// Inverse direction.
    Inverse,
}

impl TransformDirection {
    /// Lowercase name, `"forward"` or `"inverse"`.
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
            _ => Err(Error::msg(format!(
                "Unrecognized transform direction: '{s}'."
            ))),
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
pub fn combine_transform_directions(
    d1: TransformDirection,
    d2: TransformDirection,
) -> TransformDirection {
    d1.combine(d2)
}

/// Free function equivalent of [`TransformDirection::inverse`].
pub fn get_inverse_transform_direction(d: TransformDirection) -> TransformDirection {
    d.inverse()
}

/// Kind of transform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransformType {
    /// `AllocationTransform`.
    Allocation,
    /// `BuiltinTransform`.
    Builtin,
    /// `CDLTransform`.
    Cdl,
    /// `ColorSpaceTransform`.
    ColorSpace,
    /// `DisplayViewTransform`.
    DisplayView,
    /// `ExponentTransform`.
    Exponent,
    /// `ExponentWithLinearTransform`.
    ExponentWithLinear,
    /// `ExposureContrastTransform`.
    ExposureContrast,
    /// `FileTransform`.
    File,
    /// `FixedFunctionTransform`.
    FixedFunction,
    /// `GradingHueCurveTransform`.
    GradingHueCurve,
    /// `GradingPrimaryTransform`.
    GradingPrimary,
    /// `GradingRGBCurveTransform`.
    GradingRgbCurve,
    /// `GradingToneTransform`.
    GradingTone,
    /// `GroupTransform`.
    Group,
    /// `LogAffineTransform`.
    LogAffine,
    /// `LogCameraTransform`.
    LogCamera,
    /// `LogTransform`.
    Log,
    /// `LookTransform`.
    Look,
    /// `Lut1DTransform`.
    Lut1D,
    /// `Lut3DTransform`.
    Lut3D,
    /// `MatrixTransform`.
    Matrix,
    /// `RangeTransform`.
    Range,
}

/// LUT interpolation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Interpolation {
    /// Unrecognized interpolation (`"unknown"`).
    Unknown = 0,
    /// Nearest neighbor (`"nearest"`).
    Nearest = 1,
    /// Linear / trilinear (`"linear"`).
    Linear = 2,
    /// Tetrahedral, 3D LUTs only (`"tetrahedral"`).
    Tetrahedral = 3,
    /// Cubic (`"cubic"`).
    Cubic = 4,
    /// Default interpolation of the LUT type (`"default"`).
    #[default]
    Default = 254,
    /// Best available interpolation (`"best"`).
    Best = 255,
}

impl Interpolation {
    /// Lowercase name, e.g. `"linear"` or `"tetrahedral"`.
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
    /// Unknown bit depth (default, `"unknown"`).
    Unknown,
    /// 8-bit unsigned integer (`"8ui"`).
    UInt8,
    /// 10-bit unsigned integer (`"10ui"`).
    UInt10,
    /// 12-bit unsigned integer (`"12ui"`).
    UInt12,
    /// 14-bit unsigned integer (`"14ui"`).
    UInt14,
    /// 16-bit unsigned integer (`"16ui"`).
    UInt16,
    /// 32-bit unsigned integer (`"32ui"`).
    UInt32,
    /// 16-bit half float (`"16f"`).
    F16,
    /// 32-bit float (`"32f"`).
    F32,
}

impl BitDepth {
    /// OCIO name of the bit depth, e.g. `"8ui"` or `"32f"`.
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
    /// Parse an OCIO bit depth name (case insensitive); anything else is `Unknown`.
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
    /// True for `F16` and `F32`.
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
    /// No hue adjustment (default).
    #[default]
    None,
    /// `HUE_DW3`: restore the hue after per-channel LUT application.
    Dw3,
    /// `HUE_WYPN`: hue adjustment variant (not implemented; rejected by the Lut1D op).
    Wypn,
}

/// Channel ordering of packed images.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ChannelOrdering {
    /// R, G, B, A (default).
    #[default]
    Rgba,
    /// B, G, R, A.
    Bgra,
    /// A, B, G, R.
    Abgr,
    /// R, G, B (no alpha).
    Rgb,
    /// B, G, R (no alpha).
    Bgr,
}

impl ChannelOrdering {
    /// Number of channels per pixel (3 for `Rgb`/`Bgr`, otherwise 4).
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
    /// Unrecognized allocation (`"unknown"`).
    Unknown,
    /// Uniform allocation (default, `"uniform"`).
    #[default]
    Uniform,
    /// Base 2 logarithmic allocation (`"lg2"`).
    Lg2,
}

impl Allocation {
    /// Lowercase name, `"uniform"`, `"lg2"` or `"unknown"`.
    pub fn as_str(&self) -> &'static str {
        match self {
            Allocation::Uniform => "uniform",
            Allocation::Lg2 => "lg2",
            Allocation::Unknown => "unknown",
        }
    }
    /// Parse `"uniform"` / `"lg2"` (case insensitive); anything else is `Unknown`.
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
    /// Nvidia Cg (`"cg"`).
    Cg,
    /// GLSL 1.2 (`"glsl_1.2"`).
    Glsl1_2,
    /// GLSL 1.3 (`"glsl_1.3"`).
    Glsl1_3,
    /// GLSL 4.0 (default, `"glsl_4.0"`).
    #[default]
    Glsl4_0,
    /// GLSL 4.6 for Vulkan (`"glsl_vk_4.6"`).
    GlslVk4_6,
    /// HLSL shader model 5.0 (`"hlsl_sm_5.0"`).
    HlslSm5_0,
    /// Open Shading Language 1 (`"osl_1"`).
    Osl1,
    /// GLSL ES 1.0 (`"glsl_es_1.0"`).
    GlslEs1_0,
    /// GLSL ES 3.0 (`"glsl_es_3.0"`).
    GlslEs3_0,
    /// Metal Shading Language 2.0 (`"msl_2"`).
    Msl2_0,
}

impl GpuLanguage {
    /// OCIO name of the language, e.g. `"glsl_4.0"`.
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
    /// Parse an OCIO language name (case insensitive); errors on unknown names.
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
            _ => {
                return Err(Error::msg(format!(
                    "Unsupported GPU shader language: '{s}'."
                )))
            }
        })
    }
}
impl_display_as_str!(GpuLanguage);

/// How environment variables are loaded into a context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum EnvironmentMode {
    /// Unrecognized mode (`"unknown"`).
    Unknown,
    /// Load only the variables declared in the config (default, `"loadpredefined"`).
    #[default]
    LoadPredefined,
    /// Load all environment variables (`"loadall"`).
    LoadAll,
}

impl EnvironmentMode {
    /// Lowercase name, `"loadpredefined"`, `"loadall"` or `"unknown"`.
    pub fn as_str(&self) -> &'static str {
        match self {
            EnvironmentMode::LoadPredefined => "loadpredefined",
            EnvironmentMode::LoadAll => "loadall",
            EnvironmentMode::Unknown => "unknown",
        }
    }
    /// Parse `"loadpredefined"` / `"loadall"` (case insensitive); anything else is `Unknown`.
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
    /// Scale and offset without clamping (`"noClamp"`).
    NoClamp,
    /// Clamp to the output range (default, `"Clamp"`).
    #[default]
    Clamp,
}

impl RangeStyle {
    /// CLF style name, `"noClamp"` or `"Clamp"`.
    pub fn as_str(&self) -> &'static str {
        match self {
            RangeStyle::NoClamp => "noClamp",
            RangeStyle::Clamp => "Clamp",
        }
    }
    /// Parse `"noClamp"` / `"Clamp"` (case insensitive); errors otherwise.
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
    /// ACES 0.3/0.7 red modifier (`"ACES_RedMod03"`).
    AcesRedMod03,
    /// ACES 1.0 red modifier (`"ACES_RedMod10"`).
    AcesRedMod10,
    /// ACES 0.3/0.7 glow (`"ACES_Glow03"`).
    AcesGlow03,
    /// ACES 1.0 glow (`"ACES_Glow10"`).
    AcesGlow10,
    /// ACES 1.0 dark to dim surround (`"ACES_DarkToDim10"`).
    AcesDarkToDim10,
    /// Rec.2100 surround correction (`"REC2100_Surround"`).
    Rec2100Surround,
    /// RGB to HSV (`"RGB_TO_HSV"`).
    RgbToHsv,
    /// CIE XYZ to xyY (`"XYZ_TO_xyY"`).
    XyzToXyy,
    /// CIE XYZ to u'v'Y (`"XYZ_TO_uvY"`).
    XyzToUvy,
    /// CIE XYZ to L*u*v* (`"XYZ_TO_LUV"`).
    XyzToLuv,
    /// ACES 0.2 gamut mapping (`"ACES_GamutMap02"`).
    AcesGamutMap02,
    /// ACES 0.7 gamut mapping (`"ACES_GamutMap07"`).
    AcesGamutMap07,
    /// ACES 1.3 reference gamut compression (`"ACES_GamutComp13"`).
    AcesGamutComp13,
    /// Linear to SMPTE ST 2084 PQ (`"Lin_TO_PQ"`).
    LinToPq,
    /// Linear to gamma/log curve (`"Lin_TO_GammaLog"`).
    LinToGammaLog,
    /// Linear to double log curve (`"Lin_TO_DoubleLog"`).
    LinToDoubleLog,
    /// ACES 2 output transform (`"ACES2_OutputTransform"`).
    AcesOutputTransform20,
    /// ACES 2 RGB to JMh (`"ACES2_RGB_TO_JMh"`).
    AcesRgbToJmh20,
    /// ACES 2 tonescale and chroma compression (`"ACES2_TonescaleCompress"`).
    AcesTonescaleCompress20,
    /// ACES 2 gamut compression (`"ACES2_GamutCompress"`).
    AcesGamutCompress20,
    /// ACES 2 RGB to HMJ (`"ACES2_RGB_TO_HMJ"`).
    AcesRgbToHmj20,
    /// RGB to HSY for linear encodings (`"RGB_TO_HSY_LIN"`).
    RgbToHsyLin,
    /// RGB to HSY for log encodings (`"RGB_TO_HSY_LOG"`).
    RgbToHsyLog,
    /// RGB to HSY for video encodings (`"RGB_TO_HSY_VID"`).
    RgbToHsyVid,
}

impl FixedFunctionStyle {
    /// CLF style name, e.g. `"ACES_RedMod10"` or `"RGB_TO_HSV"`.
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
    /// Parse a CLF style name (case insensitive); errors on unknown names.
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
            _ => {
                return Err(Error::msg(format!(
                    "Unknown Fixed FunctionOp style: '{s}'."
                )))
            }
        })
    }
}
impl_display_as_str!(FixedFunctionStyle);

/// Exposure/contrast styles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ExposureContrastStyle {
    /// Linear (scene-linear) style (default, `"linear"`).
    #[default]
    Linear,
    /// Video (gamma-encoded) style (`"video"`).
    Video,
    /// Logarithmic style (`"log"`).
    Logarithmic,
}

impl ExposureContrastStyle {
    /// Lowercase name, `"linear"`, `"video"` or `"log"`.
    pub fn as_str(&self) -> &'static str {
        match self {
            ExposureContrastStyle::Video => "video",
            ExposureContrastStyle::Logarithmic => "log",
            ExposureContrastStyle::Linear => "linear",
        }
    }
    /// Parse `"linear"` / `"video"` / `"log"` (case insensitive); errors otherwise.
    pub fn parse(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "linear" => Ok(ExposureContrastStyle::Linear),
            "video" => Ok(ExposureContrastStyle::Video),
            "log" => Ok(ExposureContrastStyle::Logarithmic),
            _ => Err(Error::msg(format!(
                "Unknown exposure contrast style: '{s}'."
            ))),
        }
    }
}
impl_display_as_str!(ExposureContrastStyle);

/// CDL styles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CdlStyle {
    /// ASC CDL with clamping to [0, 1] (`"asc"`).
    Asc,
    /// Default style of `CDLTransform` (`CDL_TRANSFORM_DEFAULT`).
    #[default]
    NoClamp,
}

impl CdlStyle {
    /// Lowercase/CLF name, `"asc"` or `"noClamp"`.
    pub fn as_str(&self) -> &'static str {
        match self {
            CdlStyle::Asc => "asc",
            CdlStyle::NoClamp => "noClamp",
        }
    }
    /// Parse `"asc"` / `"noClamp"` (case insensitive); errors otherwise.
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
    /// Clamp negative values to zero (default, `"clamp"`).
    #[default]
    Clamp,
    /// Mirror the curve around the origin (`"mirror"`).
    Mirror,
    /// Pass negative values through unchanged (`"pass_thru"`).
    PassThru,
    /// Linear segment for negative values (`"linear"`).
    Linear,
}

impl NegativeStyle {
    /// Lowercase name, e.g. `"clamp"` or `"pass_thru"`.
    pub fn as_str(&self) -> &'static str {
        match self {
            NegativeStyle::Clamp => "clamp",
            NegativeStyle::Mirror => "mirror",
            NegativeStyle::PassThru => "pass_thru",
            NegativeStyle::Linear => "linear",
        }
    }
    /// Parse `"clamp"` / `"mirror"` / `"pass_thru"` / `"linear"` (case insensitive); errors otherwise.
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
    /// Logarithmic style (default, `"log"`).
    #[default]
    Log,
    /// Linear style (`"linear"`).
    Lin,
    /// Video style (`"video"`).
    Video,
}

impl GradingStyle {
    /// Lowercase name, `"log"`, `"linear"` or `"video"`.
    pub fn as_str(&self) -> &'static str {
        match self {
            GradingStyle::Lin => "linear",
            GradingStyle::Log => "log",
            GradingStyle::Video => "video",
        }
    }
    /// Parse `"log"` / `"linear"` / `"video"` (case insensitive); errors otherwise.
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
    /// Exposure of an `ExposureContrastTransform`.
    Exposure,
    /// Contrast of an `ExposureContrastTransform`.
    Contrast,
    /// Gamma of an `ExposureContrastTransform`.
    Gamma,
    /// Values of a `GradingPrimaryTransform`.
    GradingPrimary,
    /// Curves of a `GradingRGBCurveTransform`.
    GradingRgbCurve,
    /// Values of a `GradingToneTransform`.
    GradingTone,
    /// Curves of a `GradingHueCurveTransform`.
    GradingHueCurve,
}

/// RGB curve selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RgbCurveType {
    /// Red channel curve.
    Red = 0,
    /// Green channel curve.
    Green = 1,
    /// Blue channel curve.
    Blue = 2,
    /// Master curve applied to all channels.
    Master = 3,
}

impl RgbCurveType {
    /// All curve types, in index order.
    pub const ALL: [RgbCurveType; 4] = [
        RgbCurveType::Red,
        RgbCurveType::Green,
        RgbCurveType::Blue,
        RgbCurveType::Master,
    ];
}

/// Hue curve selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HueCurveType {
    /// Hue as a function of hue.
    HueHue = 0,
    /// Saturation as a function of hue.
    HueSat,
    /// Luminance as a function of hue.
    HueLum,
    /// Saturation as a function of luminance.
    LumSat,
    /// Saturation as a function of saturation.
    SatSat,
    /// Luminance as a function of luminance.
    LumLum,
    /// Luminance as a function of saturation.
    SatLum,
    /// Hue-dependent effect curve (`HUE_FX`).
    HueFx,
}

impl HueCurveType {
    /// All curve types, in index order.
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
    /// No conversion; the curves operate on the input values directly.
    None,
    /// HSY conversion (default).
    #[default]
    Hsy1,
}

/// B-spline curve type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BSplineType {
    /// Standard B-spline (default).
    #[default]
    BSpline,
    /// Diagonal B-spline (identity is y = x).
    DiagonalBSpline,
    /// Hue-hue B-spline (periodic, identity is y = x).
    HueHueBSpline,
    /// Periodic B-spline with identity value 1.
    Periodic1BSpline,
    /// Periodic B-spline with identity value 0.
    Periodic0BSpline,
    /// Horizontal B-spline with identity value 1.
    Horizontal1BSpline,
}

/// Optimization flags (bit field, see `OptimizationFlags` in OCIO).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OptimizationFlags(pub u32);

impl OptimizationFlags {
    /// No optimization.
    pub const NONE: Self = Self(0x0000_0000);
    /// Remove identity ops.
    pub const IDENTITY: Self = Self(0x0000_0001);
    /// Remove identity gamma ops.
    pub const IDENTITY_GAMMA: Self = Self(0x0000_0002);
    /// Remove pairs of inverse CDL ops.
    pub const PAIR_IDENTITY_CDL: Self = Self(0x0000_0040);
    /// Remove pairs of inverse exposure/contrast ops.
    pub const PAIR_IDENTITY_EXPOSURE_CONTRAST: Self = Self(0x0000_0080);
    /// Remove pairs of inverse fixed function ops.
    pub const PAIR_IDENTITY_FIXED_FUNCTION: Self = Self(0x0000_0100);
    /// Remove pairs of inverse gamma ops.
    pub const PAIR_IDENTITY_GAMMA: Self = Self(0x0000_0200);
    /// Remove pairs of inverse 1D LUT ops.
    pub const PAIR_IDENTITY_LUT1D: Self = Self(0x0000_0400);
    /// Remove pairs of inverse 3D LUT ops.
    pub const PAIR_IDENTITY_LUT3D: Self = Self(0x0000_0800);
    /// Remove pairs of inverse log ops.
    pub const PAIR_IDENTITY_LOG: Self = Self(0x0000_1000);
    /// Remove pairs of inverse grading ops.
    pub const PAIR_IDENTITY_GRADING: Self = Self(0x0000_2000);
    /// Compose adjacent exponent ops.
    pub const COMP_EXPONENT: Self = Self(0x0004_0000);
    /// Compose adjacent gamma ops.
    pub const COMP_GAMMA: Self = Self(0x0008_0000);
    /// Compose adjacent matrix ops.
    pub const COMP_MATRIX: Self = Self(0x0010_0000);
    /// Compose adjacent 1D LUT ops.
    pub const COMP_LUT1D: Self = Self(0x0020_0000);
    /// Compose adjacent 3D LUT ops.
    pub const COMP_LUT3D: Self = Self(0x0040_0000);
    /// Compose adjacent range ops.
    pub const COMP_RANGE: Self = Self(0x0080_0000);
    /// Replace a separable prefix of ops with a 1D LUT.
    pub const COMP_SEPARABLE_PREFIX: Self = Self(0x0100_0000);
    /// Use a fast approximation for inverse LUTs.
    pub const LUT_INV_FAST: Self = Self(0x0200_0000);
    /// Use fast approximations of log, exp and pow.
    pub const FAST_LOG_EXP_POW: Self = Self(0x0400_0000);
    /// Simplify ops (e.g. replace complex ops by simpler equivalents).
    pub const SIMPLIFY_OPS: Self = Self(0x0800_0000);
    /// Bake dynamic properties as static values.
    pub const NO_DYNAMIC_PROPERTIES: Self = Self(0x1000_0000);
    /// All optimizations.
    pub const ALL: Self = Self(0xFFFF_FFFF);

    /// Optimizations that do not change the result.
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
    /// Lossless plus near-lossless approximations (the default).
    pub const VERY_GOOD: Self = Self(
        Self::LOSSLESS.0
            | Self::COMP_LUT1D.0
            | Self::LUT_INV_FAST.0
            | Self::FAST_LOG_EXP_POW.0
            | Self::COMP_SEPARABLE_PREFIX.0,
    );
    /// `VERY_GOOD` plus 3D LUT composition.
    pub const GOOD: Self = Self(Self::VERY_GOOD.0 | Self::COMP_LUT3D.0);
    /// All optimizations.
    pub const DRAFT: Self = Self::ALL;
    /// Default optimization level (`VERY_GOOD`).
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
    /// Caching disabled.
    pub const OFF: Self = Self(0x00);
    /// Processor caching enabled.
    pub const ENABLED: Self = Self(0x01);
    /// Share dynamic properties between cached processors.
    pub const SHARE_DYN_PROPERTIES: Self = Self(0x02);
    /// Caching enabled with shared dynamic properties.
    pub const DEFAULT: Self = Self(0x03);
}

impl Default for ProcessorCacheFlags {
    fn default() -> Self {
        Self::DEFAULT
    }
}

// ---------------------------------------------------------------------------
// Constants

/// Environment variable naming the config file (`OCIO`).
pub const OCIO_CONFIG_ENVVAR: &str = "OCIO";
/// Environment variable overriding the active displays.
pub const OCIO_ACTIVE_DISPLAYS_ENVVAR: &str = "OCIO_ACTIVE_DISPLAYS";
/// Environment variable overriding the active views.
pub const OCIO_ACTIVE_VIEWS_ENVVAR: &str = "OCIO_ACTIVE_VIEWS";
/// Environment variable overriding the inactive color spaces.
pub const OCIO_INACTIVE_COLORSPACES_ENVVAR: &str = "OCIO_INACTIVE_COLORSPACES";
/// Environment variable overriding the optimization flags.
pub const OCIO_OPTIMIZATION_FLAGS_ENVVAR: &str = "OCIO_OPTIMIZATION_FLAGS";
/// Environment variable listing the user categories.
pub const OCIO_USER_CATEGORIES_ENVVAR: &str = "OCIO_USER_CATEGORIES";
/// Environment variable setting the logging level.
pub const OCIO_LOGGING_LEVEL_ENVVAR: &str = "OCIO_LOGGING_LEVEL";

/// Default config file name, without extension.
pub const OCIO_CONFIG_DEFAULT_NAME: &str = "config";
/// Config file extension.
pub const OCIO_CONFIG_DEFAULT_FILE_EXT: &str = ".ocio";
/// Config archive file extension.
pub const OCIO_CONFIG_ARCHIVE_FILE_EXT: &str = ".ocioz";
/// View name placeholder replaced by the display name.
pub const OCIO_VIEW_USE_DISPLAY_NAME: &str = "<USE_DISPLAY_NAME>";
/// URI prefix of built-in configs.
pub const OCIO_BUILTIN_URI_PREFIX: &str = "ocio://";

/// Environment variable disabling all caches.
pub const OCIO_DISABLE_ALL_CACHES: &str = "OCIO_DISABLE_ALL_CACHES";
/// Environment variable disabling processor caches.
pub const OCIO_DISABLE_PROCESSOR_CACHES: &str = "OCIO_DISABLE_PROCESSOR_CACHES";
/// Environment variable disabling the cache fallback.
pub const OCIO_DISABLE_CACHE_FALLBACK: &str = "OCIO_DISABLE_CACHE_FALLBACK";

/// Default role.
pub const ROLE_DEFAULT: &str = "default";
/// Reference role.
pub const ROLE_REFERENCE: &str = "reference";
/// Data (non-color) role.
pub const ROLE_DATA: &str = "data";
/// Color picking role.
pub const ROLE_COLOR_PICKING: &str = "color_picking";
/// Scene-linear role.
pub const ROLE_SCENE_LINEAR: &str = "scene_linear";
/// Compositing log role.
pub const ROLE_COMPOSITING_LOG: &str = "compositing_log";
/// Color timing role.
pub const ROLE_COLOR_TIMING: &str = "color_timing";
/// Texture painting role.
pub const ROLE_TEXTURE_PAINT: &str = "texture_paint";
/// Matte painting role.
pub const ROLE_MATTE_PAINT: &str = "matte_paint";
/// Rendering role.
pub const ROLE_RENDERING: &str = "rendering";
/// Scene-referred interchange role (ACES2065-1).
pub const ROLE_INTERCHANGE_SCENE: &str = "aces_interchange";
/// Display-referred interchange role (CIE XYZ D65).
pub const ROLE_INTERCHANGE_DISPLAY: &str = "cie_xyz_d65_interchange";

/// Metadata key for a description.
pub const METADATA_DESCRIPTION: &str = "Description";
/// Metadata key for info.
pub const METADATA_INFO: &str = "Info";
/// Metadata key for the input descriptor.
pub const METADATA_INPUT_DESCRIPTOR: &str = "InputDescriptor";
/// Metadata key for the output descriptor.
pub const METADATA_OUTPUT_DESCRIPTOR: &str = "OutputDescriptor";
/// Metadata attribute for a name.
pub const METADATA_NAME: &str = "name";
/// Metadata attribute for an id.
pub const METADATA_ID: &str = "id";
/// Metadata element for an id.
pub const METADATA_ID_ELEMENT: &str = "Id";
