//! Transforms: the user-facing descriptions of color operations (port of
//! `OpenColorTransforms.h`).
//!
//! Every transform is a plain struct with public fields plus convenience
//! accessors. The [`Transform`] enum wraps all of them. Transforms are turned
//! into ops by [`build::build_ops`].

pub mod build;
pub mod grading;

use crate::error::Result;
use crate::format_metadata::FormatMetadata;
use crate::types::*;
use grading::{GradingHueCurve, GradingPrimary, GradingRgbCurve, GradingTone};

pub use build::{BuildOps, Validate};

/// Default luma coefficients (Rec.709) used by CDL saturation.
pub const DEFAULT_LUMA_COEFS: [f64; 3] = [0.2126, 0.7152, 0.0722];

// ---------------------------------------------------------------------------
// Transform enum

/// Any transform.
#[derive(Debug, Clone, PartialEq)]
pub enum Transform {
    /// An [`AllocationTransform`].
    Allocation(AllocationTransform),
    /// A [`BuiltinTransform`].
    Builtin(BuiltinTransform),
    /// A [`CdlTransform`].
    Cdl(CdlTransform),
    /// A [`ColorSpaceTransform`].
    ColorSpace(ColorSpaceTransform),
    /// A [`DisplayViewTransform`].
    DisplayView(DisplayViewTransform),
    /// An [`ExponentTransform`].
    Exponent(ExponentTransform),
    /// An [`ExponentWithLinearTransform`].
    ExponentWithLinear(ExponentWithLinearTransform),
    /// An [`ExposureContrastTransform`].
    ExposureContrast(ExposureContrastTransform),
    /// A [`FileTransform`].
    File(FileTransform),
    /// A [`FixedFunctionTransform`].
    FixedFunction(FixedFunctionTransform),
    /// A [`GradingHueCurveTransform`].
    GradingHueCurve(GradingHueCurveTransform),
    /// A [`GradingPrimaryTransform`].
    GradingPrimary(GradingPrimaryTransform),
    /// A [`GradingRgbCurveTransform`].
    GradingRgbCurve(GradingRgbCurveTransform),
    /// A [`GradingToneTransform`].
    GradingTone(GradingToneTransform),
    /// A [`GroupTransform`].
    Group(GroupTransform),
    /// A [`LogAffineTransform`].
    LogAffine(LogAffineTransform),
    /// A [`LogCameraTransform`].
    LogCamera(LogCameraTransform),
    /// A [`LogTransform`].
    Log(LogTransform),
    /// A [`LookTransform`].
    Look(LookTransform),
    /// A [`Lut1DTransform`].
    Lut1D(Lut1DTransform),
    /// A [`Lut3DTransform`].
    Lut3D(Lut3DTransform),
    /// A [`MatrixTransform`].
    Matrix(MatrixTransform),
    /// A [`RangeTransform`].
    Range(RangeTransform),
}

/// Apply `$body` to the inner transform of every variant.
#[macro_export]
#[doc(hidden)]
macro_rules! for_each_transform {
    ($t:expr, $x:ident => $body:expr) => {
        match $t {
            $crate::Transform::Allocation($x) => $body,
            $crate::Transform::Builtin($x) => $body,
            $crate::Transform::Cdl($x) => $body,
            $crate::Transform::ColorSpace($x) => $body,
            $crate::Transform::DisplayView($x) => $body,
            $crate::Transform::Exponent($x) => $body,
            $crate::Transform::ExponentWithLinear($x) => $body,
            $crate::Transform::ExposureContrast($x) => $body,
            $crate::Transform::File($x) => $body,
            $crate::Transform::FixedFunction($x) => $body,
            $crate::Transform::GradingHueCurve($x) => $body,
            $crate::Transform::GradingPrimary($x) => $body,
            $crate::Transform::GradingRgbCurve($x) => $body,
            $crate::Transform::GradingTone($x) => $body,
            $crate::Transform::Group($x) => $body,
            $crate::Transform::LogAffine($x) => $body,
            $crate::Transform::LogCamera($x) => $body,
            $crate::Transform::Log($x) => $body,
            $crate::Transform::Look($x) => $body,
            $crate::Transform::Lut1D($x) => $body,
            $crate::Transform::Lut3D($x) => $body,
            $crate::Transform::Matrix($x) => $body,
            $crate::Transform::Range($x) => $body,
        }
    };
}

impl Transform {
    /// The transform direction.
    pub fn direction(&self) -> TransformDirection {
        crate::for_each_transform!(self, t => t.direction)
    }

    /// Set the transform direction.
    pub fn set_direction(&mut self, dir: TransformDirection) {
        crate::for_each_transform!(self, t => t.direction = dir)
    }

    /// Copy of the transform with the direction flipped.
    pub fn inverted(&self) -> Transform {
        let mut t = self.clone();
        t.set_direction(self.direction().inverse());
        t
    }

    /// Validate the transform parameters.
    pub fn validate(&self) -> Result<()> {
        crate::for_each_transform!(self, t => t.validate())
    }

    /// Format metadata, for transforms that carry it.
    pub fn format_metadata(&self) -> Option<&FormatMetadata> {
        match self {
            Transform::Cdl(t) => Some(&t.metadata),
            Transform::Exponent(t) => Some(&t.metadata),
            Transform::ExponentWithLinear(t) => Some(&t.metadata),
            Transform::ExposureContrast(t) => Some(&t.metadata),
            Transform::FixedFunction(t) => Some(&t.metadata),
            Transform::GradingHueCurve(t) => Some(&t.metadata),
            Transform::GradingPrimary(t) => Some(&t.metadata),
            Transform::GradingRgbCurve(t) => Some(&t.metadata),
            Transform::GradingTone(t) => Some(&t.metadata),
            Transform::Group(t) => Some(&t.metadata),
            Transform::LogAffine(t) => Some(&t.metadata),
            Transform::LogCamera(t) => Some(&t.metadata),
            Transform::Log(t) => Some(&t.metadata),
            Transform::Lut1D(t) => Some(&t.metadata),
            Transform::Lut3D(t) => Some(&t.metadata),
            Transform::Matrix(t) => Some(&t.metadata),
            Transform::Range(t) => Some(&t.metadata),
            _ => None,
        }
    }

    /// Mutable format metadata, for transforms that carry it.
    pub fn format_metadata_mut(&mut self) -> Option<&mut FormatMetadata> {
        match self {
            Transform::Cdl(t) => Some(&mut t.metadata),
            Transform::Exponent(t) => Some(&mut t.metadata),
            Transform::ExponentWithLinear(t) => Some(&mut t.metadata),
            Transform::ExposureContrast(t) => Some(&mut t.metadata),
            Transform::FixedFunction(t) => Some(&mut t.metadata),
            Transform::GradingHueCurve(t) => Some(&mut t.metadata),
            Transform::GradingPrimary(t) => Some(&mut t.metadata),
            Transform::GradingRgbCurve(t) => Some(&mut t.metadata),
            Transform::GradingTone(t) => Some(&mut t.metadata),
            Transform::Group(t) => Some(&mut t.metadata),
            Transform::LogAffine(t) => Some(&mut t.metadata),
            Transform::LogCamera(t) => Some(&mut t.metadata),
            Transform::Log(t) => Some(&mut t.metadata),
            Transform::Lut1D(t) => Some(&mut t.metadata),
            Transform::Lut3D(t) => Some(&mut t.metadata),
            Transform::Matrix(t) => Some(&mut t.metadata),
            Transform::Range(t) => Some(&mut t.metadata),
            _ => None,
        }
    }

    /// The kind of transform.
    pub fn transform_type(&self) -> TransformType {
        match self {
            Transform::Allocation(_) => TransformType::Allocation,
            Transform::Builtin(_) => TransformType::Builtin,
            Transform::Cdl(_) => TransformType::Cdl,
            Transform::ColorSpace(_) => TransformType::ColorSpace,
            Transform::DisplayView(_) => TransformType::DisplayView,
            Transform::Exponent(_) => TransformType::Exponent,
            Transform::ExponentWithLinear(_) => TransformType::ExponentWithLinear,
            Transform::ExposureContrast(_) => TransformType::ExposureContrast,
            Transform::File(_) => TransformType::File,
            Transform::FixedFunction(_) => TransformType::FixedFunction,
            Transform::GradingHueCurve(_) => TransformType::GradingHueCurve,
            Transform::GradingPrimary(_) => TransformType::GradingPrimary,
            Transform::GradingRgbCurve(_) => TransformType::GradingRgbCurve,
            Transform::GradingTone(_) => TransformType::GradingTone,
            Transform::Group(_) => TransformType::Group,
            Transform::LogAffine(_) => TransformType::LogAffine,
            Transform::LogCamera(_) => TransformType::LogCamera,
            Transform::Log(_) => TransformType::Log,
            Transform::Look(_) => TransformType::Look,
            Transform::Lut1D(_) => TransformType::Lut1D,
            Transform::Lut3D(_) => TransformType::Lut3D,
            Transform::Matrix(_) => TransformType::Matrix,
            Transform::Range(_) => TransformType::Range,
        }
    }
}

macro_rules! impl_from_transform {
    ($($variant:ident => $ty:ty),* $(,)?) => {
        $(
            impl From<$ty> for Transform {
                fn from(t: $ty) -> Self {
                    Transform::$variant(t)
                }
            }
        )*
    };
}

impl_from_transform! {
    Allocation => AllocationTransform,
    Builtin => BuiltinTransform,
    Cdl => CdlTransform,
    ColorSpace => ColorSpaceTransform,
    DisplayView => DisplayViewTransform,
    Exponent => ExponentTransform,
    ExponentWithLinear => ExponentWithLinearTransform,
    ExposureContrast => ExposureContrastTransform,
    File => FileTransform,
    FixedFunction => FixedFunctionTransform,
    GradingHueCurve => GradingHueCurveTransform,
    GradingPrimary => GradingPrimaryTransform,
    GradingRgbCurve => GradingRgbCurveTransform,
    GradingTone => GradingToneTransform,
    Group => GroupTransform,
    LogAffine => LogAffineTransform,
    LogCamera => LogCameraTransform,
    Log => LogTransform,
    Look => LookTransform,
    Lut1D => Lut1DTransform,
    Lut3D => Lut3DTransform,
    Matrix => MatrixTransform,
    Range => RangeTransform,
}

// ---------------------------------------------------------------------------
// Individual transforms

/// Allocation (uniform / lg2) transform, used by legacy GPU paths and
/// `allocation` in colorspaces.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AllocationTransform {
    /// The transform direction.
    pub direction: TransformDirection,
    /// The allocation type (uniform or lg2).
    pub allocation: Allocation,
    /// 0, 2 or 3 variables: min, max, [lg2 linear offset].
    pub vars: Vec<f64>,
}

/// A transform from the builtin registry, identified by its style name
/// (for instance `"ACEScct_to_ACES2065-1"`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BuiltinTransform {
    /// The transform direction.
    pub direction: TransformDirection,
    /// The builtin style name, looked up in the builtin registry.
    pub style: String,
}

impl BuiltinTransform {
    /// Builtin transform of the given style, in the forward direction.
    pub fn new(style: &str) -> Self {
        Self {
            direction: TransformDirection::Forward,
            style: style.to_string(),
        }
    }
}

/// ASC CDL (slope / offset / power / saturation).
#[derive(Debug, Clone, PartialEq)]
pub struct CdlTransform {
    /// The transform direction.
    pub direction: TransformDirection,
    /// The CDL style (clamping behavior).
    pub style: CdlStyle,
    /// Per-channel RGB slope (default 1).
    pub slope: [f64; 3],
    /// Per-channel RGB offset (default 0).
    pub offset: [f64; 3],
    /// Per-channel RGB power (default 1).
    pub power: [f64; 3],
    /// Saturation (default 1).
    pub sat: f64,
    /// Holds the `id` attribute and description children (`Description`,
    /// `InputDescription`, `ViewingDescription`, `SOPDescription`,
    /// `SATDescription`).
    pub metadata: FormatMetadata,
}

impl Default for CdlTransform {
    fn default() -> Self {
        Self {
            direction: TransformDirection::Forward,
            style: CdlStyle::NoClamp,
            slope: [1.0; 3],
            offset: [0.0; 3],
            power: [1.0; 3],
            sat: 1.0,
            metadata: FormatMetadata::default(),
        }
    }
}

/// Element name of the SOP description child.
pub const METADATA_SOP_DESCRIPTION: &str = "SOPDescription";
/// Element name of the SAT description child.
pub const METADATA_SAT_DESCRIPTION: &str = "SATDescription";
/// Element name of the input description child.
pub const METADATA_INPUT_DESCRIPTION: &str = "InputDescription";
/// Element name of the viewing description child.
pub const METADATA_VIEWING_DESCRIPTION: &str = "ViewingDescription";

impl CdlTransform {
    /// Identity CDL (same as `Default`).
    pub fn new() -> Self {
        Self::default()
    }
    /// The 9 SOP values: slope, offset, power.
    pub fn sop(&self) -> [f64; 9] {
        let mut v = [0.0; 9];
        v[..3].copy_from_slice(&self.slope);
        v[3..6].copy_from_slice(&self.offset);
        v[6..].copy_from_slice(&self.power);
        v
    }
    /// Set the 9 SOP values: slope, offset, power.
    pub fn set_sop(&mut self, v: &[f64; 9]) {
        self.slope.copy_from_slice(&v[..3]);
        self.offset.copy_from_slice(&v[3..6]);
        self.power.copy_from_slice(&v[6..]);
    }
    /// Luma coefficients used by the saturation.
    pub fn sat_luma_coefs(&self) -> [f64; 3] {
        DEFAULT_LUMA_COEFS
    }
    /// The `id` attribute of the metadata.
    pub fn id(&self) -> &str {
        self.metadata.id()
    }
    /// Set the `id` attribute of the metadata.
    pub fn set_id(&mut self, id: &str) {
        self.metadata.set_id(id);
    }
    /// The first `SOPDescription` child element value (`""` if none).
    pub fn first_sop_description(&self) -> &str {
        self.metadata
            .children_named(METADATA_SOP_DESCRIPTION)
            .next()
            .map(|c| c.element_value.as_str())
            .unwrap_or("")
    }
    /// Set (or add) the first `SOPDescription` child element value.
    pub fn set_first_sop_description(&mut self, desc: &str) {
        if let Some(i) = self.metadata.first_child_index(METADATA_SOP_DESCRIPTION) {
            self.metadata.children[i].element_value = desc.to_string();
        } else {
            self.metadata
                .add_child_element(METADATA_SOP_DESCRIPTION, desc);
        }
    }
    /// True if slope/offset/power/sat are the identity values.
    pub fn is_identity_values(&self) -> bool {
        self.slope == [1.0; 3]
            && self.offset == [0.0; 3]
            && self.power == [1.0; 3]
            && self.sat == 1.0
    }
}

/// Conversion between two color spaces of the config.
#[derive(Debug, Clone, PartialEq)]
pub struct ColorSpaceTransform {
    /// The transform direction.
    pub direction: TransformDirection,
    /// Source color space name.
    pub src: String,
    /// Destination color space name.
    pub dst: String,
    /// If true (the default), data color spaces are not converted.
    pub data_bypass: bool,
}

impl Default for ColorSpaceTransform {
    fn default() -> Self {
        Self {
            direction: TransformDirection::Forward,
            src: String::new(),
            dst: String::new(),
            data_bypass: true,
        }
    }
}

impl ColorSpaceTransform {
    /// Conversion from `src` to `dst`, in the forward direction.
    pub fn new(src: &str, dst: &str) -> Self {
        Self {
            src: src.to_string(),
            dst: dst.to_string(),
            ..Default::default()
        }
    }
}

/// Conversion from a color space to a (display, view) pair.
#[derive(Debug, Clone, PartialEq)]
pub struct DisplayViewTransform {
    /// The transform direction.
    pub direction: TransformDirection,
    /// Source color space name.
    pub src: String,
    /// Display name.
    pub display: String,
    /// View name.
    pub view: String,
    /// If true, the looks of the view are not applied (default false).
    pub looks_bypass: bool,
    /// If true (the default), data color spaces are not converted.
    pub data_bypass: bool,
}

impl Default for DisplayViewTransform {
    fn default() -> Self {
        Self {
            direction: TransformDirection::Forward,
            src: String::new(),
            display: String::new(),
            view: String::new(),
            looks_bypass: false,
            data_bypass: true,
        }
    }
}

impl DisplayViewTransform {
    /// Conversion from `src` to the (`display`, `view`) pair, in the forward direction.
    pub fn new(src: &str, display: &str, view: &str) -> Self {
        Self {
            src: src.to_string(),
            display: display.to_string(),
            view: view.to_string(),
            ..Default::default()
        }
    }
}

/// Power function `out = in ^ value` per channel (RGBA).
#[derive(Debug, Clone, PartialEq)]
pub struct ExponentTransform {
    /// The transform direction.
    pub direction: TransformDirection,
    /// Per-channel RGBA exponent (default 1).
    pub value: [f64; 4],
    /// How negative values are handled (default `Clamp`).
    pub negative_style: NegativeStyle,
    /// Format metadata.
    pub metadata: FormatMetadata,
}

impl Default for ExponentTransform {
    fn default() -> Self {
        Self {
            direction: TransformDirection::Forward,
            value: [1.0; 4],
            negative_style: NegativeStyle::Clamp,
            metadata: FormatMetadata::default(),
        }
    }
}

impl ExponentTransform {
    /// Exponent transform with the given RGBA exponents.
    pub fn new(value: [f64; 4]) -> Self {
        Self {
            value,
            ..Default::default()
        }
    }
}

/// Power function with a linear segment near zero (sRGB / Rec.709 style
/// "moncurve").
#[derive(Debug, Clone, PartialEq)]
pub struct ExponentWithLinearTransform {
    /// The transform direction.
    pub direction: TransformDirection,
    /// Per-channel RGBA gamma (default 1).
    pub gamma: [f64; 4],
    /// Per-channel RGBA offset (default 0).
    pub offset: [f64; 4],
    /// Only `Linear` and `Mirror` are valid.
    pub negative_style: NegativeStyle,
    /// Format metadata.
    pub metadata: FormatMetadata,
}

impl Default for ExponentWithLinearTransform {
    fn default() -> Self {
        Self {
            direction: TransformDirection::Forward,
            gamma: [1.0; 4],
            offset: [0.0; 4],
            negative_style: NegativeStyle::Linear,
            metadata: FormatMetadata::default(),
        }
    }
}

/// Exposure / contrast / gamma adjustment.
#[derive(Debug, Clone, PartialEq)]
pub struct ExposureContrastTransform {
    /// The transform direction.
    pub direction: TransformDirection,
    /// The exposure / contrast style (linear, video or log).
    pub style: ExposureContrastStyle,
    /// Exposure in stops (default 0).
    pub exposure: f64,
    /// If true, the exposure is a dynamic property.
    pub exposure_dynamic: bool,
    /// Contrast (default 1).
    pub contrast: f64,
    /// If true, the contrast is a dynamic property.
    pub contrast_dynamic: bool,
    /// Gamma (default 1).
    pub gamma: f64,
    /// If true, the gamma is a dynamic property.
    pub gamma_dynamic: bool,
    /// Linear value around which contrast and gamma pivot (default 0.18).
    pub pivot: f64,
    /// Log units per stop of exposure, for the log style (default 0.088).
    pub log_exposure_step: f64,
    /// Log-encoded value of mid-gray, for the log style (default 0.435).
    pub log_mid_gray: f64,
    /// Format metadata.
    pub metadata: FormatMetadata,
}

impl Default for ExposureContrastTransform {
    fn default() -> Self {
        Self {
            direction: TransformDirection::Forward,
            style: ExposureContrastStyle::Linear,
            exposure: 0.0,
            exposure_dynamic: false,
            contrast: 1.0,
            contrast_dynamic: false,
            gamma: 1.0,
            gamma_dynamic: false,
            pivot: 0.18,
            log_exposure_step: 0.088,
            log_mid_gray: 0.435,
            metadata: FormatMetadata::default(),
        }
    }
}

/// Transform loaded from an external file (LUT, CDL, CLF, ...).
#[derive(Debug, Clone, PartialEq)]
pub struct FileTransform {
    /// The transform direction.
    pub direction: TransformDirection,
    /// Path of the file (resolved against the config search path).
    pub src: String,
    /// CDL id (or index) selecting a correction in a `.ccc` / `.cdl` file.
    pub ccc_id: String,
    /// The CDL style used when the file holds a CDL.
    pub cdl_style: CdlStyle,
    /// The interpolation used for LUTs.
    pub interpolation: Interpolation,
}

impl Default for FileTransform {
    fn default() -> Self {
        Self {
            direction: TransformDirection::Forward,
            src: String::new(),
            ccc_id: String::new(),
            cdl_style: CdlStyle::NoClamp,
            interpolation: Interpolation::Default,
        }
    }
}

impl FileTransform {
    /// File transform for the given file path.
    pub fn new(src: &str) -> Self {
        Self {
            src: src.to_string(),
            ..Default::default()
        }
    }
}

/// Hard-coded color math (ACES red modifier, glow, HSV, PQ, ACES 2 ...).
#[derive(Debug, Clone, PartialEq)]
pub struct FixedFunctionTransform {
    /// The transform direction.
    pub direction: TransformDirection,
    /// The fixed function style.
    pub style: FixedFunctionStyle,
    /// Style-specific parameters.
    pub params: Vec<f64>,
    /// Format metadata.
    pub metadata: FormatMetadata,
}

impl FixedFunctionTransform {
    /// Fixed function of the given style and parameters, in the forward direction.
    pub fn new(style: FixedFunctionStyle, params: &[f64]) -> Self {
        Self {
            direction: TransformDirection::Forward,
            style,
            params: params.to_vec(),
            metadata: FormatMetadata::default(),
        }
    }
}

/// Primary grading (lift/gamma/gain, contrast, offset, exposure, saturation).
#[derive(Debug, Clone, PartialEq)]
pub struct GradingPrimaryTransform {
    /// The transform direction.
    pub direction: TransformDirection,
    /// The grading style (log, linear or video).
    pub style: GradingStyle,
    /// The grading values.
    pub value: GradingPrimary,
    /// If true, the values are a dynamic property.
    pub dynamic: bool,
    /// Format metadata.
    pub metadata: FormatMetadata,
}

impl GradingPrimaryTransform {
    /// Identity grading for the given style.
    pub fn new(style: GradingStyle) -> Self {
        Self {
            direction: TransformDirection::Forward,
            style,
            value: GradingPrimary::new(style),
            dynamic: false,
            metadata: FormatMetadata::default(),
        }
    }
}

/// Hue curve grading.
#[derive(Debug, Clone, PartialEq)]
pub struct GradingHueCurveTransform {
    /// The transform direction.
    pub direction: TransformDirection,
    /// The grading style (log, linear or video).
    pub style: GradingStyle,
    /// The hue curves.
    pub value: GradingHueCurve,
    /// The RGB to HSY conversion style (default `Hsy1`).
    pub rgb_to_hsy: HsyTransformStyle,
    /// If true, the values are a dynamic property.
    pub dynamic: bool,
    /// Format metadata.
    pub metadata: FormatMetadata,
}

impl GradingHueCurveTransform {
    /// Identity hue curves for the given style.
    pub fn new(style: GradingStyle) -> Self {
        Self {
            direction: TransformDirection::Forward,
            style,
            value: GradingHueCurve::new(style),
            rgb_to_hsy: HsyTransformStyle::Hsy1,
            dynamic: false,
            metadata: FormatMetadata::default(),
        }
    }
}

/// RGB curve grading.
#[derive(Debug, Clone, PartialEq)]
pub struct GradingRgbCurveTransform {
    /// The transform direction.
    pub direction: TransformDirection,
    /// The grading style (log, linear or video).
    pub style: GradingStyle,
    /// The RGB curves.
    pub value: GradingRgbCurve,
    /// If true, the linear style skips the lin-to-log conversion around the curves.
    pub bypass_lin_to_log: bool,
    /// If true, the values are a dynamic property.
    pub dynamic: bool,
    /// Format metadata.
    pub metadata: FormatMetadata,
}

impl GradingRgbCurveTransform {
    /// Identity RGB curves for the given style.
    pub fn new(style: GradingStyle) -> Self {
        Self {
            direction: TransformDirection::Forward,
            style,
            value: GradingRgbCurve::new(style),
            bypass_lin_to_log: false,
            dynamic: false,
            metadata: FormatMetadata::default(),
        }
    }
}

/// Tonal grading (blacks, shadows, midtones, highlights, whites).
#[derive(Debug, Clone, PartialEq)]
pub struct GradingToneTransform {
    /// The transform direction.
    pub direction: TransformDirection,
    /// The grading style (log, linear or video).
    pub style: GradingStyle,
    /// The tonal grading values.
    pub value: GradingTone,
    /// If true, the values are a dynamic property.
    pub dynamic: bool,
    /// Format metadata.
    pub metadata: FormatMetadata,
}

impl GradingToneTransform {
    /// Identity tonal grading for the given style.
    pub fn new(style: GradingStyle) -> Self {
        Self {
            direction: TransformDirection::Forward,
            style,
            value: GradingTone::new(style),
            dynamic: false,
            metadata: FormatMetadata::default(),
        }
    }
}

/// Ordered list of transforms.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GroupTransform {
    /// The transform direction.
    pub direction: TransformDirection,
    /// The transforms, applied in order.
    pub transforms: Vec<Transform>,
    /// Format metadata.
    pub metadata: FormatMetadata,
}

impl GroupTransform {
    /// Empty group.
    pub fn new() -> Self {
        Self::default()
    }
    /// Group of the given transforms.
    pub fn from_transforms(transforms: Vec<Transform>) -> Self {
        Self {
            transforms,
            ..Default::default()
        }
    }
    /// Number of transforms in the group.
    pub fn num_transforms(&self) -> usize {
        self.transforms.len()
    }
    /// Add a transform at the end.
    pub fn append(&mut self, t: impl Into<Transform>) {
        self.transforms.push(t.into());
    }
    /// Add a transform at the beginning.
    pub fn prepend(&mut self, t: impl Into<Transform>) {
        self.transforms.insert(0, t.into());
    }
}

/// `out = logSlope * log(linSlope * in + linOffset, base) + logOffset`.
#[derive(Debug, Clone, PartialEq)]
pub struct LogAffineTransform {
    /// The transform direction.
    pub direction: TransformDirection,
    /// Log base (default 2).
    pub base: f64,
    /// Per-channel log side slope (default 1).
    pub log_side_slope: [f64; 3],
    /// Per-channel log side offset (default 0).
    pub log_side_offset: [f64; 3],
    /// Per-channel linear side slope (default 1).
    pub lin_side_slope: [f64; 3],
    /// Per-channel linear side offset (default 0).
    pub lin_side_offset: [f64; 3],
    /// Format metadata.
    pub metadata: FormatMetadata,
}

impl Default for LogAffineTransform {
    fn default() -> Self {
        Self {
            direction: TransformDirection::Forward,
            base: 2.0,
            log_side_slope: [1.0; 3],
            log_side_offset: [0.0; 3],
            lin_side_slope: [1.0; 3],
            lin_side_offset: [0.0; 3],
            metadata: FormatMetadata::default(),
        }
    }
}

/// Camera log curve: log affine with a linear segment below `lin_side_break`.
#[derive(Debug, Clone, PartialEq)]
pub struct LogCameraTransform {
    /// The transform direction.
    pub direction: TransformDirection,
    /// Log base (default 2).
    pub base: f64,
    /// Per-channel log side slope (default 1).
    pub log_side_slope: [f64; 3],
    /// Per-channel log side offset (default 0).
    pub log_side_offset: [f64; 3],
    /// Per-channel linear side slope (default 1).
    pub lin_side_slope: [f64; 3],
    /// Per-channel linear side offset (default 0).
    pub lin_side_offset: [f64; 3],
    /// Per-channel linear value below which the linear segment is used.
    pub lin_side_break: [f64; 3],
    /// Optional; computed from the other parameters when `None`.
    pub linear_slope: Option<[f64; 3]>,
    /// Format metadata.
    pub metadata: FormatMetadata,
}

impl LogCameraTransform {
    /// Camera log with the given linear side break and default parameters.
    pub fn new(lin_side_break: [f64; 3]) -> Self {
        Self {
            direction: TransformDirection::Forward,
            base: 2.0,
            log_side_slope: [1.0; 3],
            log_side_offset: [0.0; 3],
            lin_side_slope: [1.0; 3],
            lin_side_offset: [0.0; 3],
            lin_side_break,
            linear_slope: None,
            metadata: FormatMetadata::default(),
        }
    }
}

/// `out = log(in, base)`.
#[derive(Debug, Clone, PartialEq)]
pub struct LogTransform {
    /// The transform direction.
    pub direction: TransformDirection,
    /// Log base (default 2).
    pub base: f64,
    /// Format metadata.
    pub metadata: FormatMetadata,
}

impl Default for LogTransform {
    fn default() -> Self {
        Self {
            direction: TransformDirection::Forward,
            base: 2.0,
            metadata: FormatMetadata::default(),
        }
    }
}

impl LogTransform {
    /// Log transform with the given base.
    pub fn new(base: f64) -> Self {
        Self {
            base,
            ..Default::default()
        }
    }
}

/// Apply looks (and color space conversions) from `src` to `dst`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LookTransform {
    /// The transform direction.
    pub direction: TransformDirection,
    /// Source color space name.
    pub src: String,
    /// Destination color space name.
    pub dst: String,
    /// Look names to apply (comma-separated, optionally `+`/`-` prefixed).
    pub looks: String,
    /// If true, only the looks are applied (no conversion to `dst`).
    pub skip_color_space_conversion: bool,
}

impl LookTransform {
    /// Look transform from `src` to `dst` applying `looks`.
    pub fn new(src: &str, dst: &str, looks: &str) -> Self {
        Self {
            src: src.to_string(),
            dst: dst.to_string(),
            looks: looks.to_string(),
            ..Default::default()
        }
    }
}

/// 1D LUT. `values` holds `length * 3` interleaved RGB values.
#[derive(Debug, Clone, PartialEq)]
pub struct Lut1DTransform {
    /// The transform direction.
    pub direction: TransformDirection,
    /// Interleaved RGB values (`length * 3`).
    pub values: Vec<f32>,
    /// If true, entries are indexed by half-float bit patterns (65536 entries).
    pub input_half_domain: bool,
    /// If true, values are stored as raw half-float bit patterns when written.
    pub output_raw_halfs: bool,
    /// The hue adjustment applied after the LUT.
    pub hue_adjust: Lut1DHueAdjust,
    /// The interpolation.
    pub interpolation: Interpolation,
    /// Bit depth used to scale the values when written to a file.
    pub file_output_bit_depth: BitDepth,
    /// Format metadata.
    pub metadata: FormatMetadata,
}

impl Default for Lut1DTransform {
    fn default() -> Self {
        Self::new(2, false)
    }
}

impl Lut1DTransform {
    /// Identity LUT of the given length. With `half_domain`, the length must
    /// be 65536 and entries are indexed by half-float bit patterns.
    pub fn new(length: usize, half_domain: bool) -> Self {
        let mut t = Self {
            direction: TransformDirection::Forward,
            values: Vec::new(),
            input_half_domain: half_domain,
            output_raw_halfs: false,
            hue_adjust: Lut1DHueAdjust::None,
            interpolation: Interpolation::Default,
            file_output_bit_depth: BitDepth::Unknown,
            metadata: FormatMetadata::default(),
        };
        t.set_length(length);
        t
    }

    /// Number of entries.
    pub fn length(&self) -> usize {
        self.values.len() / 3
    }

    /// Resize and reset to identity.
    pub fn set_length(&mut self, length: usize) {
        self.values = vec![0.0; length * 3];
        for i in 0..length {
            let v = if self.input_half_domain {
                half::f16::from_bits(i as u16).to_f32()
            } else if length > 1 {
                i as f32 / (length - 1) as f32
            } else {
                0.0
            };
            self.values[3 * i] = v;
            self.values[3 * i + 1] = v;
            self.values[3 * i + 2] = v;
        }
    }

    /// RGB value at `index` (panics if out of bounds).
    pub fn value(&self, index: usize) -> [f32; 3] {
        [
            self.values[3 * index],
            self.values[3 * index + 1],
            self.values[3 * index + 2],
        ]
    }

    /// Set the RGB value at `index` (panics if out of bounds).
    pub fn set_value(&mut self, index: usize, r: f32, g: f32, b: f32) {
        self.values[3 * index] = r;
        self.values[3 * index + 1] = g;
        self.values[3 * index + 2] = b;
    }
}

/// 3D LUT. `values` holds `grid_size^3 * 3` RGB values with the **blue**
/// index changing fastest: `index = ((r * n + g) * n + b) * 3`.
#[derive(Debug, Clone, PartialEq)]
pub struct Lut3DTransform {
    /// The transform direction.
    pub direction: TransformDirection,
    /// Number of entries along each axis.
    pub grid_size: usize,
    /// RGB values (`grid_size^3 * 3`, blue fastest).
    pub values: Vec<f32>,
    /// The interpolation.
    pub interpolation: Interpolation,
    /// Bit depth used to scale the values when written to a file.
    pub file_output_bit_depth: BitDepth,
    /// Format metadata.
    pub metadata: FormatMetadata,
}

impl Default for Lut3DTransform {
    fn default() -> Self {
        Self::new(2)
    }
}

impl Lut3DTransform {
    /// Identity LUT of the given grid size.
    pub fn new(grid_size: usize) -> Self {
        let mut t = Self {
            direction: TransformDirection::Forward,
            grid_size: 0,
            values: Vec::new(),
            interpolation: Interpolation::Default,
            file_output_bit_depth: BitDepth::Unknown,
            metadata: FormatMetadata::default(),
        };
        t.set_grid_size(grid_size);
        t
    }

    /// Resize and reset to identity.
    pub fn set_grid_size(&mut self, n: usize) {
        self.grid_size = n;
        self.values = vec![0.0; n * n * n * 3];
        let scale = if n > 1 { 1.0 / (n - 1) as f32 } else { 0.0 };
        for r in 0..n {
            for g in 0..n {
                for b in 0..n {
                    let i = ((r * n + g) * n + b) * 3;
                    self.values[i] = r as f32 * scale;
                    self.values[i + 1] = g as f32 * scale;
                    self.values[i + 2] = b as f32 * scale;
                }
            }
        }
    }

    /// RGB value at grid position (`r`, `g`, `b`) (panics if out of bounds).
    pub fn value(&self, r: usize, g: usize, b: usize) -> [f32; 3] {
        let n = self.grid_size;
        let i = ((r * n + g) * n + b) * 3;
        [self.values[i], self.values[i + 1], self.values[i + 2]]
    }

    /// Set the RGB value at grid position (`r`, `g`, `b`) (panics if out of bounds).
    pub fn set_value(&mut self, r: usize, g: usize, b: usize, rgb: [f32; 3]) {
        let n = self.grid_size;
        let i = ((r * n + g) * n + b) * 3;
        self.values[i..i + 3].copy_from_slice(&rgb);
    }
}

/// 4x4 matrix (row major) plus offset: `out = M * in + offset`.
#[derive(Debug, Clone, PartialEq)]
pub struct MatrixTransform {
    /// The transform direction.
    pub direction: TransformDirection,
    /// 4x4 row-major matrix (default identity).
    pub matrix: [f64; 16],
    /// RGBA offset (default 0).
    pub offset: [f64; 4],
    /// Input bit depth the values were scaled for in the file.
    pub file_input_bit_depth: BitDepth,
    /// Output bit depth the values were scaled for in the file.
    pub file_output_bit_depth: BitDepth,
    /// Format metadata.
    pub metadata: FormatMetadata,
}

/// The 4x4 identity matrix.
pub const IDENTITY_MATRIX44: [f64; 16] = [
    1.0, 0.0, 0.0, 0.0, //
    0.0, 1.0, 0.0, 0.0, //
    0.0, 0.0, 1.0, 0.0, //
    0.0, 0.0, 0.0, 1.0,
];

impl Default for MatrixTransform {
    fn default() -> Self {
        Self {
            direction: TransformDirection::Forward,
            matrix: IDENTITY_MATRIX44,
            offset: [0.0; 4],
            file_input_bit_depth: BitDepth::Unknown,
            file_output_bit_depth: BitDepth::Unknown,
            metadata: FormatMetadata::default(),
        }
    }
}

impl MatrixTransform {
    /// Matrix transform with the given matrix and offset.
    pub fn new(matrix: [f64; 16], offset: [f64; 4]) -> Self {
        Self {
            matrix,
            offset,
            ..Default::default()
        }
    }

    /// Matrix from a 3x3 row-major matrix.
    pub fn from_m33(m: &[f64; 9]) -> Self {
        let mut m44 = IDENTITY_MATRIX44;
        for r in 0..3 {
            for c in 0..3 {
                m44[r * 4 + c] = m[r * 3 + c];
            }
        }
        Self::new(m44, [0.0; 4])
    }

    /// Matrix/offset mapping `[oldmin, oldmax]` to `[newmin, newmax]` per channel.
    pub fn fit(
        oldmin: &[f64; 4],
        oldmax: &[f64; 4],
        newmin: &[f64; 4],
        newmax: &[f64; 4],
    ) -> Result<([f64; 16], [f64; 4])> {
        let mut m = [0.0; 16];
        let mut o = [0.0; 4];
        for i in 0..4 {
            let denom = oldmax[i] - oldmin[i];
            if denom == 0.0 {
                crate::bail!("Cannot create Fit operator. Max value equals min value '{}' in channel index {}.", oldmax[i], i);
            }
            m[5 * i] = (newmax[i] - newmin[i]) / denom;
            o[i] = (newmin[i] * oldmax[i] - newmax[i] * oldmin[i]) / denom;
        }
        Ok((m, o))
    }

    /// Identity matrix and zero offset.
    pub fn identity() -> ([f64; 16], [f64; 4]) {
        (IDENTITY_MATRIX44, [0.0; 4])
    }

    /// Saturation matrix.
    pub fn sat(sat: f64, luma: &[f64; 3]) -> ([f64; 16], [f64; 4]) {
        let mut m = [0.0; 16];
        let (l0, l1, l2) = (luma[0], luma[1], luma[2]);
        m[0] = (1.0 - sat) * l0 + sat;
        m[1] = (1.0 - sat) * l1;
        m[2] = (1.0 - sat) * l2;
        m[4] = (1.0 - sat) * l0;
        m[5] = (1.0 - sat) * l1 + sat;
        m[6] = (1.0 - sat) * l2;
        m[8] = (1.0 - sat) * l0;
        m[9] = (1.0 - sat) * l1;
        m[10] = (1.0 - sat) * l2 + sat;
        m[15] = 1.0;
        (m, [0.0; 4])
    }

    /// Diagonal scale matrix.
    pub fn scale(scale: &[f64; 4]) -> ([f64; 16], [f64; 4]) {
        let mut m = [0.0; 16];
        for i in 0..4 {
            m[5 * i] = scale[i];
        }
        (m, [0.0; 4])
    }

    /// Channel view matrix (port of `MatrixTransform::View`).
    pub fn view(channel_hot: &[i32; 4], luma: &[f64; 3]) -> ([f64; 16], [f64; 4]) {
        let mut m = [0.0; 16];
        if channel_hot.iter().all(|&h| h != 0) {
            return Self::identity();
        } else if channel_hot[3] != 0 {
            // If not all the channels are hot, but alpha is, just show it.
            for i in 0..4 {
                m[4 * i + 3] = 1.0;
            }
        } else {
            // Blend rgb as specified, place it in all 3 output channels.
            let mut values = [0.0; 3];
            for i in 0..3 {
                values[i] = luma[i] * if channel_hot[i] != 0 { 1.0 } else { 0.0 };
            }
            let sum: f64 = values.iter().sum();
            if sum.abs() > 1e-9 {
                values.iter_mut().for_each(|v| *v /= sum);
            }
            for row in 0..3 {
                for i in 0..3 {
                    m[4 * row + i] = values[i];
                }
            }
            m[15] = 1.0;
        }
        (m, [0.0; 4])
    }
}

/// Affine remap of `[min_in, max_in]` to `[min_out, max_out]` with optional
/// clamping. Each bound is optional.
#[derive(Debug, Clone, PartialEq)]
pub struct RangeTransform {
    /// The transform direction.
    pub direction: TransformDirection,
    /// Whether values are clamped to the bounds (default `Clamp`).
    pub style: RangeStyle,
    /// Lower input bound.
    pub min_in: Option<f64>,
    /// Upper input bound.
    pub max_in: Option<f64>,
    /// Lower output bound.
    pub min_out: Option<f64>,
    /// Upper output bound.
    pub max_out: Option<f64>,
    /// Input bit depth the values were scaled for in the file.
    pub file_input_bit_depth: BitDepth,
    /// Output bit depth the values were scaled for in the file.
    pub file_output_bit_depth: BitDepth,
    /// Format metadata.
    pub metadata: FormatMetadata,
}

impl Default for RangeTransform {
    fn default() -> Self {
        Self {
            direction: TransformDirection::Forward,
            style: RangeStyle::Clamp,
            min_in: None,
            max_in: None,
            min_out: None,
            max_out: None,
            file_input_bit_depth: BitDepth::Unknown,
            file_output_bit_depth: BitDepth::Unknown,
            metadata: FormatMetadata::default(),
        }
    }
}

impl RangeTransform {
    /// Range transform with the given bounds.
    pub fn new(
        min_in: Option<f64>,
        max_in: Option<f64>,
        min_out: Option<f64>,
        max_out: Option<f64>,
    ) -> Self {
        Self {
            min_in,
            max_in,
            min_out,
            max_out,
            ..Default::default()
        }
    }
}
