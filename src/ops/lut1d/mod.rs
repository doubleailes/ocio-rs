//! 1D LUT op (port of `Lut1DOpData.cpp`, `Lut1DOp.cpp`, `Lut1DOpCPU.cpp` and
//! `Lut1DTransform.cpp`).
//!
//! A 1D LUT holds `length` RGB entries. Its domain is either the standard
//! `[0, 1]` domain (entries evenly spaced) or the *half domain*: 65536
//! entries indexed by the bit pattern of the half-float input value.
//!
//! Forward evaluation is always linear (OCIO v2 implements `nearest` as
//! `linear`), optionally followed by the DW3 hue restoration. Inverse
//! evaluation is exact (inverse of the linear interpolation) once the LUT
//! has been made monotonic by [`Lut1DOpData::finalize`]; a faster
//! approximation is available through [`make_fast_lut1d_from_inverse`]
//! (`OptimizationFlags::LUT_INV_FAST`).

mod cpu;
#[cfg(test)]
mod tests;

pub use self::cpu::order3;
use self::cpu::Lut1DRenderer;
use crate::config::Config;
use crate::context::Context;
use crate::error::{Error, Result};
use crate::format_metadata::FormatMetadata;
use crate::ops::{hash_f32, Op, OpRc, OpVec, Pixel};
use crate::transforms::{BuildOps, Lut1DTransform, RangeTransform, Transform, Validate};
use crate::types::{
    BitDepth, Interpolation, Lut1DHueAdjust, OptimizationFlags, TransformDirection,
};
use half::f16;
use std::any::Any;
use std::fmt;
use std::ops::{Index, IndexMut};
use std::sync::{Arc, OnceLock};

/// Number of entries of a half-domain 1D LUT.
pub const HALF_DOMAIN_REQUIRED_ENTRIES: usize = 65536;

/// Maximum length of a 1D LUT.
pub const MAX_LUT1D_LENGTH: usize = 1024 * 1024;

// ---------------------------------------------------------------------------
// Shared helpers (also used by the 3D LUT module).

/// OCIO's `SanitizeFloat`: maps `-Inf` to `-f32::MAX`, `+Inf` to `f32::MAX`
/// and NaN to 0.
pub(crate) fn sanitize_float(f: f32) -> f32 {
    if f == f32::NEG_INFINITY {
        -f32::MAX
    } else if f == f32::INFINITY {
        f32::MAX
    } else if f.is_nan() {
        0.0
    } else {
        f
    }
}

/// OCIO's `lerpf`: `(b - a) * z + a`.
#[inline]
pub(crate) fn lerpf(a: f32, b: f32, z: f32) -> f32 {
    (b - a) * z + a
}

/// `std::max(a, b)` semantics (returns `a` when the comparison fails, e.g.
/// with NaN).
#[inline]
pub(crate) fn std_max(a: f32, b: f32) -> f32 {
    if a < b {
        b
    } else {
        a
    }
}

/// `std::min(a, b)` semantics (returns `a` when the comparison fails, e.g.
/// with NaN).
#[inline]
pub(crate) fn std_min(a: f32, b: f32) -> f32 {
    if b < a {
        b
    } else {
        a
    }
}

/// OCIO's `Clamp`: `std::min(std::max(min, a), max)`, NaN maps to `min`.
#[inline]
pub(crate) fn clamp_ocio(a: f32, min: f32, max: f32) -> f32 {
    std_min(std_max(min, a), max)
}

/// Compare two halfs as integers with a tolerance in ULPs (port of
/// `HalfsDiffer`). Returns true if they differ by more than `tolerance`.
pub fn halfs_differ(expected: f16, actual: f16, tolerance: i32) -> bool {
    fn half_for_compare(h: f16) -> i32 {
        // Map neg 0 and pos 0 to 32768, allowing tolerance-based comparison
        // of small numbers of mixed sign.
        let raw = h.to_bits() as i32;
        if raw < 32767 {
            raw + 32768
        } else {
            2 * 32768 - raw
        }
    }

    let aim_bits = half_for_compare(expected);
    let val_bits = half_for_compare(actual);

    if expected.is_nan() {
        !actual.is_nan()
    } else if actual.is_nan() {
        !expected.is_nan()
    } else if expected.is_infinite() || actual.is_infinite() {
        aim_bits != val_bits
    } else {
        (val_bits - aim_bits).abs() > tolerance
    }
}

/// Evaluate RGB triplets through an op list at 32f (port of `EvalTransform`).
/// Alpha is set to 1 during the evaluation.
pub fn eval_transform(values: &mut [f32], ops: &[OpRc]) {
    let num_pixels = values.len() / 3;
    let mut pixels: Vec<Pixel> = (0..num_pixels)
        .map(|i| [values[3 * i], values[3 * i + 1], values[3 * i + 2], 1.0])
        .collect();
    for op in ops {
        if !op.is_no_op() {
            op.apply(&mut pixels);
        }
    }
    for (i, p) in pixels.iter().enumerate() {
        values[3 * i..3 * i + 3].copy_from_slice(&p[..3]);
    }
}

/// What an identity (or a pair of inverse ops) can be replaced with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum IdentityReplacement {
    /// Nothing: the op (pair) can simply be removed (OCIO uses an identity
    /// matrix, which is a no-op).
    NoOp,
    /// A clamp to `[min, max]` (OCIO uses a `RangeOpData(min, max, min, max)`).
    Clamp { min: f64, max: f64 },
}

/// Build the ops of a clamping range `[min, max] -> [min, max]`. Returns
/// `None` if the range cannot be built.
pub(crate) fn create_clamp_ops(min: f64, max: f64) -> Option<OpVec> {
    let range = RangeTransform::new(Some(min), Some(max), Some(min), Some(max));
    let config = Config::create_raw();
    let mut ops = OpVec::new();
    crate::transforms::build::build_ops(
        &mut ops,
        &config,
        config.current_context(),
        &Transform::Range(range),
        TransformDirection::Forward,
    )
    .ok()?;
    Some(ops)
}

/// Ops implementing an [`IdentityReplacement`] (`None` if a required op
/// cannot be built).
pub fn identity_replacement_ops(replacement: IdentityReplacement) -> Option<OpVec> {
    match replacement {
        IdentityReplacement::NoOp => Some(OpVec::new()),
        IdentityReplacement::Clamp { min, max } => create_clamp_ops(min, max),
    }
}

// ---------------------------------------------------------------------------
// Flags & properties.

/// Flags describing the 1D LUT index and value encoding (port of
/// `Lut1DOpData::HalfFlags`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct HalfFlags(pub u8);

impl HalfFlags {
    /// Indices and values use standard encoding.
    pub const STANDARD: Self = Self(0x00);
    /// LUT indices are half float codes.
    pub const INPUT_HALF_CODE: Self = Self(0x01);
    /// LUT values are half float codes.
    pub const OUTPUT_HALF_CODE: Self = Self(0x02);
    /// Indices and values are half float codes.
    pub const INPUT_OUTPUT_HALF_CODE: Self = Self(0x03);

    /// True if the LUT is indexed by half float codes.
    pub fn is_input_half_domain(self) -> bool {
        (self.0 & Self::INPUT_HALF_CODE.0) == Self::INPUT_HALF_CODE.0
    }

    /// True if the LUT values are (originally) raw half float codes.
    pub fn is_output_raw_halfs(self) -> bool {
        (self.0 & Self::OUTPUT_HALF_CODE.0) == Self::OUTPUT_HALF_CODE.0
    }
}

/// Control of 1D LUT composition (port of `Lut1DOpData::ComposeMethod`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ComposeMethod {
    /// Preserve the original domain.
    ResampleNo,
    /// Minimum size is 65536.
    ResampleBig,
    /// Half domain.
    ResampleHd,
}

/// Properties needed for the inversion of one channel of a LUT (port of
/// `Lut1DOpData::ComponentProperties`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ComponentProperties {
    /// Overall increasing state.
    pub is_increasing: bool,
    /// Lowest index such that `LUT[start] != LUT[start + 1]`.
    pub start_domain: usize,
    /// Highest index such that `LUT[end - 1] != LUT[end]`.
    pub end_domain: usize,
    /// Start domain for the negative values of a half-domain LUT.
    pub neg_start_domain: usize,
    /// End domain for the negative values of a half-domain LUT.
    pub neg_end_domain: usize,
}

// ---------------------------------------------------------------------------
// Array.

/// The values of a 1D LUT (port of `Lut1DOpData::Lut3by1DArray`).
///
/// Values are always stored as `length * 3` interleaved RGB floats; the
/// number of color components (1 or 3) only records whether the three
/// channels are identical.
#[derive(Clone, PartialEq)]
pub struct Lut1DArray {
    length: usize,
    num_color_components: usize,
    values: Vec<f32>,
}

impl fmt::Debug for Lut1DArray {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Lut1DArray")
            .field("length", &self.length)
            .field("num_color_components", &self.num_color_components)
            .finish_non_exhaustive()
    }
}

impl Lut1DArray {
    /// Identity array of the given length and number of channels.
    /// `filter_nans` sets the NaN entries of a half domain to 0.
    pub fn new(
        half_flags: HalfFlags,
        num_channels: usize,
        length: usize,
        filter_nans: bool,
    ) -> Result<Self> {
        if length < 2 {
            crate::bail!("LUT 1D length needs to be at least 2.");
        }
        if num_channels != 1 && num_channels != 3 {
            crate::bail!("LUT 1D channels needs to be 1 or 3.");
        }
        let mut a = Self {
            length: 0,
            num_color_components: 0,
            values: Vec::new(),
        };
        a.resize(length, num_channels)?;
        a.fill(half_flags, filter_nans);
        Ok(a)
    }

    fn fill(&mut self, half_flags: HalfFlags, filter_nans: bool) {
        let dim = self.length;
        let max_channels = self.max_color_components();
        if half_flags.is_input_half_domain() {
            for idx in 0..dim {
                let mut v = f16::from_bits(idx as u16).to_f32();
                if v.is_nan() && filter_nans {
                    v = 0.0;
                }
                let row = max_channels * idx;
                self.values[row..row + max_channels].fill(v);
            }
        } else {
            let step = 1.0f32 / (dim as f32 - 1.0);
            for idx in 0..dim {
                let v = idx as f32 * step;
                let row = max_channels * idx;
                self.values[row..row + max_channels].fill(v);
            }
        }
    }

    /// Resize the array (new values are zeros).
    pub fn resize(&mut self, length: usize, num_color_components: usize) -> Result<()> {
        if length < 2 {
            crate::bail!("LUT 1D length needs to be at least 2.");
        } else if length > MAX_LUT1D_LENGTH {
            crate::bail!("LUT 1D: Length '{length}' must not be greater than 1024x1024 (1048576).");
        }
        self.length = length;
        self.num_color_components = num_color_components;
        self.values.resize(self.num_values(), 0.0);
        Ok(())
    }

    /// Number of entries.
    pub fn length(&self) -> usize {
        self.length
    }

    /// Number of active color components (1 when the three channels are
    /// identical, see [`Lut1DArray::adjust_color_component_number`]).
    pub fn num_color_components(&self) -> usize {
        self.num_color_components
    }

    /// Set the number of active color components.
    pub fn set_num_color_components(&mut self, n: usize) {
        if self.num_color_components != n {
            self.num_color_components = n;
            self.values.resize(self.num_values(), 0.0);
        }
    }

    /// Always 3.
    pub fn max_color_components(&self) -> usize {
        3
    }

    /// Expected number of values (`length * 3`).
    pub fn num_values(&self) -> usize {
        self.length * self.max_color_components()
    }

    /// The values `[r0, g0, b0, r1, g1, b1, ...]`.
    pub fn values(&self) -> &[f32] {
        &self.values
    }

    /// Mutable access to the values.
    pub fn values_mut(&mut self) -> &mut Vec<f32> {
        &mut self.values
    }

    /// Check the array consistency.
    pub fn validate(&self) -> Result<()> {
        if self.length == 0 {
            crate::bail!("Array content is empty.");
        }
        if self.values.len() != self.num_values() {
            crate::bail!(
                "Array contains: {} values, but {} are expected.",
                self.values.len(),
                self.num_values()
            );
        }
        Ok(())
    }

    /// True if the LUT is an identity (within 1e-5 for a standard domain,
    /// within 1 half ULP for a half domain).
    pub fn is_identity(&self, half_flags: HalfFlags) -> bool {
        let dim = self.length;
        let max_channels = self.max_color_components();
        if self.values.len() < dim * max_channels {
            return false;
        }
        if half_flags.is_input_half_domain() {
            for idx in 0..dim {
                let aim = f16::from_bits(idx as u16);
                let row = max_channels * idx;
                for channel in 0..max_channels {
                    let val = f16::from_f32(self.values[channel + row]);
                    // Must be different by at least two ULPs to not be an identity.
                    if halfs_differ(aim, val, 1) {
                        return false;
                    }
                }
            }
        } else {
            const ABS_TOL: f32 = 1e-5;
            let step = 1.0f32 / (dim as f32 - 1.0);
            for idx in 0..dim {
                let aim = idx as f32 * step;
                let row = max_channels * idx;
                for channel in 0..max_channels {
                    let err = self.values[channel + row] - aim;
                    if err.abs() > ABS_TOL {
                        return false;
                    }
                }
            }
        }
        true
    }

    /// Set the number of color components to 1 if the three channels are
    /// identical (NaN triplets are ignored).
    pub fn adjust_color_component_number(&mut self) {
        if self.num_color_components == 3 {
            let n = self.length.min(self.values.len() / 3);
            let same = (0..n).all(|idx| {
                let (r, g, b) = (
                    self.values[idx * 3],
                    self.values[idx * 3 + 1],
                    self.values[idx * 3 + 2],
                );
                (r.is_nan() && g.is_nan() && b.is_nan()) || (r == g && r == b)
            });
            if same {
                // But keep the three values.
                self.num_color_components = 1;
            }
        }
    }

    /// Multiply all the values.
    pub fn scale(&mut self, scale: f32) {
        if scale != 1.0 {
            for v in &mut self.values {
                *v *= scale;
            }
        }
    }
}

impl Index<usize> for Lut1DArray {
    type Output = f32;
    fn index(&self, i: usize) -> &f32 {
        &self.values[i]
    }
}

impl IndexMut<usize> for Lut1DArray {
    fn index_mut(&mut self, i: usize) -> &mut f32 {
        &mut self.values[i]
    }
}

// ---------------------------------------------------------------------------
// Op data.

/// Parameters of a 1D LUT (port of `Lut1DOpData`).
#[derive(Clone)]
pub struct Lut1DOpData {
    interpolation: Interpolation,
    array: Lut1DArray,
    half_flags: HalfFlags,
    hue_adjust: Lut1DHueAdjust,
    direction: TransformDirection,
    component_properties: [ComponentProperties; 3],
    file_out_bit_depth: BitDepth,
    metadata: FormatMetadata,
}

impl fmt::Debug for Lut1DOpData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Lut1DOpData")
            .field("direction", &self.direction)
            .field("interpolation", &self.interpolation)
            .field("half_flags", &self.half_flags)
            .field("hue_adjust", &self.hue_adjust)
            .field("array", &self.array)
            .field("file_out_bit_depth", &self.file_out_bit_depth)
            .finish_non_exhaustive()
    }
}

/// Equality as OCIO's `Lut1DOpData::equals`: same direction, same concrete
/// interpolation, same half flags, hue adjust and array (metadata ignored).
impl PartialEq for Lut1DOpData {
    fn eq(&self, other: &Self) -> bool {
        self.direction == other.direction
            && self.concrete_interpolation() == other.concrete_interpolation()
            && self.have_equal_basics(other)
    }
}

impl Lut1DOpData {
    /// Identity LUT with a standard domain.
    pub fn new(dimension: usize) -> Result<Self> {
        Self::new_with_flags(HalfFlags::STANDARD, dimension, false)
    }

    /// Identity LUT with a standard domain and the given direction.
    pub fn with_direction(dimension: usize, dir: TransformDirection) -> Result<Self> {
        let mut l = Self::new(dimension)?;
        l.direction = dir;
        Ok(l)
    }

    /// Identity LUT. For a half domain, `filter_nans` sets the 2048 NaN
    /// entries of the domain to 0.
    pub fn new_with_flags(
        half_flags: HalfFlags,
        dimension: usize,
        filter_nans: bool,
    ) -> Result<Self> {
        Ok(Self {
            interpolation: Interpolation::Default,
            array: Lut1DArray::new(half_flags, 3, dimension, filter_nans)?,
            half_flags,
            hue_adjust: Lut1DHueAdjust::None,
            direction: TransformDirection::Forward,
            component_properties: [ComponentProperties::default(); 3],
            file_out_bit_depth: BitDepth::Unknown,
            metadata: FormatMetadata::default(),
        })
    }

    /// Build from a transform (no validation).
    pub fn from_transform(t: &Lut1DTransform) -> Result<Self> {
        let length = t.values.len() / 3;
        if length < 2 {
            crate::bail!("LUT 1D length needs to be at least 2.");
        } else if length > MAX_LUT1D_LENGTH {
            crate::bail!("LUT 1D: Length '{length}' must not be greater than 1024x1024 (1048576).");
        }
        let mut half_flags = HalfFlags::STANDARD;
        if t.input_half_domain {
            half_flags.0 |= HalfFlags::INPUT_HALF_CODE.0;
        }
        if t.output_raw_halfs {
            half_flags.0 |= HalfFlags::OUTPUT_HALF_CODE.0;
        }
        Ok(Self {
            interpolation: t.interpolation,
            array: Lut1DArray {
                length,
                num_color_components: 3,
                values: t.values.clone(),
            },
            half_flags,
            hue_adjust: t.hue_adjust,
            direction: t.direction,
            component_properties: [ComponentProperties::default(); 3],
            file_out_bit_depth: t.file_output_bit_depth,
            metadata: t.metadata.clone(),
        })
    }

    /// Convert to a transform.
    pub fn to_transform(&self) -> Lut1DTransform {
        Lut1DTransform {
            direction: self.direction,
            values: self.array.values.clone(),
            input_half_domain: self.is_input_half_domain(),
            output_raw_halfs: self.is_output_raw_halfs(),
            hue_adjust: self.hue_adjust,
            interpolation: self.interpolation,
            file_output_bit_depth: self.file_out_bit_depth,
            metadata: self.metadata.clone(),
        }
    }

    /// The interpolation as set.
    pub fn interpolation(&self) -> Interpolation {
        self.interpolation
    }

    /// The interpolation actually used (always linear, see
    /// [`Lut1DOpData::get_concrete_interpolation`]).
    pub fn concrete_interpolation(&self) -> Interpolation {
        Self::get_concrete_interpolation(self.interpolation)
    }

    /// All interpolations are rendered as linear (OCIO v2 implements
    /// `nearest` with `linear` so that CPU and GPU agree; invalid
    /// interpolations make `validate` fail).
    pub fn get_concrete_interpolation(_interp: Interpolation) -> Interpolation {
        Interpolation::Linear
    }

    pub fn set_interpolation(&mut self, interpolation: Interpolation) {
        self.interpolation = interpolation;
    }

    /// Best, default, linear and nearest are valid.
    pub fn is_valid_interpolation(interpolation: Interpolation) -> bool {
        matches!(
            interpolation,
            Interpolation::Best
                | Interpolation::Default
                | Interpolation::Linear
                | Interpolation::Nearest
        )
    }

    pub fn direction(&self) -> TransformDirection {
        self.direction
    }

    pub fn set_direction(&mut self, dir: TransformDirection) {
        self.direction = dir;
    }

    /// A half-domain identity does nothing at all; a standard-domain identity
    /// still clamps to `[0, 1]`.
    pub fn is_no_op(&self) -> bool {
        self.is_input_half_domain() && self.is_identity()
    }

    /// True if the LUT values are an identity (it may still clamp).
    pub fn is_identity(&self) -> bool {
        self.array.is_identity(self.half_flags)
    }

    /// Only true with hue adjust.
    pub fn has_channel_crosstalk(&self) -> bool {
        self.hue_adjust != Lut1DHueAdjust::None
    }

    /// Replacement for an identity LUT.
    pub fn identity_replacement(&self) -> IdentityReplacement {
        if self.is_input_half_domain() {
            IdentityReplacement::NoOp
        } else {
            IdentityReplacement::Clamp { min: 0.0, max: 1.0 }
        }
    }

    /// Replacement of `self` followed by its inverse `lut2` (both finalized).
    pub fn pair_identity_replacement(&self, lut2: &Lut1DOpData) -> IdentityReplacement {
        if self.is_input_half_domain() {
            // TODO in OCIO: if a half-domain LUT has a flat spot, it would be
            // more appropriate to use a Range.
            return IdentityReplacement::NoOp;
        }
        // Only the op whose direction is inverse has been initialized from
        // the forward LUT (reversals flattened, component properties set).
        let inv_lut = if self.direction == TransformDirection::Inverse {
            self
        } else {
            lut2
        };
        let red = inv_lut.red_properties();
        let length = inv_lut.array.length();

        let (min_value, max_value) = match self.direction {
            // Fwd Lut1D -> Inv Lut1D: clamp based on where the flat regions
            // fall relative to the [0,1] input domain (red channel only).
            TransformDirection::Forward => (
                red.start_domain as f64 / (length - 1) as f64,
                red.end_domain as f64 / (length - 1) as f64,
            ),
            // Inv Lut1D -> Fwd Lut1D: clamp to the output range of the
            // forward LUT (red channel only).
            TransformDirection::Inverse => {
                let last = (length - 1) * inv_lut.array.max_color_components();
                let values = inv_lut.array.values();
                let (first_v, last_v) = (values[0] as f64, values[last] as f64);
                if red.is_increasing {
                    (first_v, last_v)
                } else {
                    (last_v, first_v)
                }
            }
        };
        IdentityReplacement::Clamp {
            min: min_value,
            max: max_value,
        }
    }

    /// True if the LUT is indexed by half float codes.
    pub fn is_input_half_domain(&self) -> bool {
        self.half_flags.is_input_half_domain()
    }

    pub fn set_input_half_domain(&mut self, is_half_domain: bool) {
        if is_half_domain {
            self.half_flags.0 |= HalfFlags::INPUT_HALF_CODE.0;
        } else {
            self.half_flags.0 &= !HalfFlags::INPUT_HALF_CODE.0;
        }
    }

    pub fn is_output_raw_halfs(&self) -> bool {
        self.half_flags.is_output_raw_halfs()
    }

    pub fn set_output_raw_halfs(&mut self, is_raw_halfs: bool) {
        if is_raw_halfs {
            self.half_flags.0 |= HalfFlags::OUTPUT_HALF_CODE.0;
        } else {
            self.half_flags.0 &= !HalfFlags::OUTPUT_HALF_CODE.0;
        }
    }

    pub fn half_flags(&self) -> HalfFlags {
        self.half_flags
    }

    pub fn hue_adjust(&self) -> Lut1DHueAdjust {
        self.hue_adjust
    }

    /// Set the hue adjust style (`Wypn` is not implemented).
    pub fn set_hue_adjust(&mut self, algo: Lut1DHueAdjust) -> Result<()> {
        if algo == Lut1DHueAdjust::Wypn {
            crate::bail!("1D LUT HUE_WYPN hue adjust style is not implemented.");
        }
        self.hue_adjust = algo;
        Ok(())
    }

    pub fn array(&self) -> &Lut1DArray {
        &self.array
    }

    pub fn array_mut(&mut self) -> &mut Lut1DArray {
        &mut self.array
    }

    /// Structural checks needed before evaluating the LUT (array size and
    /// half-domain length).
    pub fn check_structure(&self) -> Result<()> {
        if let Err(e) = self.array.validate() {
            crate::bail!("1D LUT content array issue: {}", e.message());
        }
        // If isHalfDomain is set, we need to make sure we have 65536 entries.
        if self.is_input_half_domain() && self.array.length() != HALF_DOMAIN_REQUIRED_ENTRIES {
            crate::bail!(
                "1D LUT: {} entries found, {} required for halfDomain 1D LUT.",
                self.array.length(),
                HALF_DOMAIN_REQUIRED_ENTRIES
            );
        }
        Ok(())
    }

    /// Validate the parameters.
    pub fn validate(&self) -> Result<()> {
        if self.hue_adjust == Lut1DHueAdjust::Wypn {
            crate::bail!("1D LUT HUE_WYPN hue adjust style is not implemented.");
        }
        if !Self::is_valid_interpolation(self.interpolation) {
            crate::bail!(
                "1D LUT does not support interpolation algorithm: {}.",
                self.interpolation.as_str()
            );
        }
        self.check_structure()
    }

    /// Copy with the direction flipped.
    pub fn inverse(&self) -> Lut1DOpData {
        let mut inv = self.clone();
        inv.direction = self.direction.inverse();
        inv
    }

    fn have_equal_basics(&self, other: &Lut1DOpData) -> bool {
        self.half_flags == other.half_flags
            && self.hue_adjust == other.hue_adjust
            && self.array == other.array
    }

    /// True if `other` is the inverse of `self`.
    ///
    /// Note: the finalization of an inverse LUT makes it monotonic, hence
    /// non-monotonic LUTs are not detected as inverses once finalized.
    pub fn is_inverse(&self, other: &Lut1DOpData) -> bool {
        self.direction != other.direction && self.have_equal_basics(other)
    }

    /// Composition is only allowed without hue adjust.
    pub fn may_compose(&self, other: &Lut1DOpData) -> bool {
        self.hue_adjust == Lut1DHueAdjust::None && other.hue_adjust == Lut1DHueAdjust::None
    }

    /// True if the same LUT applies to r, g and b (after finalization).
    pub fn has_single_lut(&self) -> bool {
        self.array.num_color_components() == 1
    }

    /// True if the LUT domain allows a look-up for the given input depth.
    pub fn may_lookup(&self, incoming_depth: BitDepth) -> bool {
        if self.is_input_half_domain() {
            incoming_depth == BitDepth::F16
        } else if !incoming_depth.is_float() {
            self.array.length() as f64 == incoming_depth.max_value() + 1.0
        } else {
            false
        }
    }

    /// Properties of the red channel (valid for inverse LUTs once finalized).
    pub fn red_properties(&self) -> &ComponentProperties {
        &self.component_properties[0]
    }

    /// Properties of the green channel.
    pub fn green_properties(&self) -> &ComponentProperties {
        &self.component_properties[1]
    }

    /// Properties of the blue channel.
    pub fn blue_properties(&self) -> &ComponentProperties {
        &self.component_properties[2]
    }

    /// True if the LUT has values outside `[0, 1]` (so the inverse LUT needs
    /// an extended domain).
    pub fn has_extended_range(&self) -> bool {
        const NORMAL_MIN: f32 = 0.0 - 1e-5;
        const NORMAL_MAX: f32 = 1.0 + 1e-5;
        self.array
            .values()
            .iter()
            .any(|&v| !v.is_nan() && !(NORMAL_MIN..=NORMAL_MAX).contains(&v))
    }

    pub fn file_output_bit_depth(&self) -> BitDepth {
        self.file_out_bit_depth
    }

    /// Record the original scaling of the LUT values (used by the fast
    /// inverse and file writers).
    pub fn set_file_output_bit_depth(&mut self, bd: BitDepth) {
        self.file_out_bit_depth = bd;
    }

    pub fn format_metadata(&self) -> &FormatMetadata {
        &self.metadata
    }

    pub fn format_metadata_mut(&mut self) -> &mut FormatMetadata {
        &mut self.metadata
    }

    /// The `id` metadata attribute.
    pub fn id(&self) -> &str {
        self.metadata.id()
    }

    /// The `name` metadata attribute.
    pub fn name(&self) -> &str {
        self.metadata.name()
    }

    /// Multiply all the values.
    pub fn scale(&mut self, scale: f32) {
        self.array.scale(scale);
    }

    /// Cache identifier.
    pub fn cache_id(&self) -> String {
        let mut s = String::new();
        if !self.id().is_empty() {
            s.push_str(self.id());
            s.push(' ');
        }
        let hue = match self.hue_adjust {
            Lut1DHueAdjust::None => "none",
            Lut1DHueAdjust::Dw3 => "dw3",
            Lut1DHueAdjust::Wypn => "wypn",
        };
        s.push_str(&format!(
            "{} {} {} {} {}",
            hash_f32(self.array.values()),
            self.direction.as_str(),
            self.interpolation.as_str(),
            if self.is_input_half_domain() {
                "half domain"
            } else {
                "standard domain"
            },
            hue
        ));
        s
    }

    /// Prepare the LUT for rendering: an inverse LUT is made monotonic and
    /// its effective domains are computed; the number of color components
    /// is reduced to 1 if the channels are identical.
    pub fn finalize(&mut self) -> Result<()> {
        self.check_structure()?;
        if self.direction == TransformDirection::Inverse {
            self.initialize_from_forward();
        }
        self.array.adjust_color_component_number();
        Ok(())
    }

    /// Make the array monotonic and prepare the parameters of the inverse
    /// renderer (port of `initializeFromForward`). Requires a valid array.
    fn initialize_from_forward(&mut self) {
        // Note: the half domain includes infinities and NaNs. The NaN part of
        // the domain is ignored by the inversion and the pre-processing.
        let length = self.array.length();
        let max_channels = self.array.max_color_components();
        let active_channels = self.array.num_color_components().min(3);
        let half = self.is_input_half_domain();
        let values = &mut self.array.values;

        for c in 0..active_channels {
            // Determine if the LUT is overall increasing or decreasing by
            // comparing the first and last entries (flat LUTs are
            // arbitrarily considered decreasing).
            let (low_ind, high_ind) = if half {
                // For half-domain LUTs, use 0 and 1 (15360 == 1.0).
                (c, 15360 * max_channels + c)
            } else {
                (c, (length - 1) * max_channels + c)
            };
            let props = &mut self.component_properties[c];
            props.is_increasing = values[low_ind] < values[high_ind];

            // Flatten reversals (there is no unique inverse otherwise and the
            // exact evaluation requires sorted values).
            let mut is_increasing = props.is_increasing;
            if !half {
                let mut prev = values[c];
                let mut idx = c + max_channels;
                while idx < length * max_channels {
                    if is_increasing != (values[idx] > prev) {
                        values[idx] = prev;
                    } else {
                        prev = values[idx];
                    }
                    idx += max_channels;
                }
            } else {
                // Positive numbers (31744 == +infinity).
                let start_ind = c;
                let end_ind = 31744 * max_channels;
                let mut prev = values[start_ind];
                let mut idx = start_ind + max_channels;
                while idx <= end_ind {
                    if is_increasing != (values[idx] > prev) {
                        values[idx] = prev;
                    } else {
                        prev = values[idx];
                    }
                    idx += max_channels;
                }

                // Negative numbers (32768 == -0, 64512 == -infinity).
                is_increasing = !is_increasing;
                let start_ind = 32768 * max_channels + c;
                let end_ind = 64512 * max_channels;
                // The previous value for -0 is +0 (disallow overlaps).
                let mut prev = values[c];
                let mut idx = start_ind;
                while idx <= end_ind {
                    if is_increasing != (values[idx] > prev) {
                        values[idx] = prev;
                    } else {
                        prev = values[idx];
                    }
                    idx += max_channels;
                }
            }

            // Determine the effective domain from the starting/ending flat
            // spots (the inverse of a flat spot is the value nearest the
            // center of the LUT). For constant LUTs, end == start == 0.
            let at = |i: usize| values[i * max_channels + c];
            if !half {
                let mut end_domain = length - 1;
                let end_value = at(end_domain);
                while end_domain > 0 && at(end_domain - 1) == end_value {
                    end_domain -= 1;
                }
                let mut start_domain = 0;
                let start_value = at(start_domain);
                while start_domain < end_domain && at(start_domain + 1) == start_value {
                    start_domain += 1;
                }
                props.start_domain = start_domain;
                props.end_domain = end_domain;
            } else {
                // +65504 is the largest half value below infinity (limiting
                // the effective domain allows 65504 to invert correctly with
                // the fast inverse).
                let mut end_domain = 31743;
                let end_value = at(end_domain);
                while end_domain > 0 && at(end_domain - 1) == end_value {
                    end_domain -= 1;
                }
                let mut start_domain = 0;
                let start_value = at(start_domain);
                while start_domain < end_domain && at(start_domain + 1) == start_value {
                    start_domain += 1;
                }
                props.start_domain = start_domain;
                props.end_domain = end_domain;

                // The negative half of the domain has its own start/end.
                let mut neg_end_domain = 64511; // -65504
                let neg_end_value = at(neg_end_domain);
                while neg_end_domain > 32768 && at(neg_end_domain - 1) == neg_end_value {
                    neg_end_domain -= 1;
                }
                let mut neg_start_domain = 32768; // -0
                let neg_start_value = at(neg_start_domain);
                while neg_start_domain < neg_end_domain
                    && at(neg_start_domain + 1) == neg_start_value
                {
                    neg_start_domain += 1;
                }
                props.neg_start_domain = neg_start_domain;
                props.neg_end_domain = neg_end_domain;
            }
        }

        if active_channels == 1 {
            self.component_properties[1] = self.component_properties[0];
            self.component_properties[2] = self.component_properties[0];
        }
    }

    // -----------------------------------------------------------------------
    // Composition.

    /// Number of entries needed to do a look-up for the given bit-depth
    /// (65536 for float depths).
    pub fn get_lut_ideal_size(incoming_bit_depth: BitDepth) -> Result<usize> {
        match incoming_bit_depth {
            BitDepth::UInt8
            | BitDepth::UInt10
            | BitDepth::UInt12
            | BitDepth::UInt14
            | BitDepth::UInt16 => Ok(incoming_bit_depth.max_value() as usize + 1),
            BitDepth::F16 | BitDepth::F32 => Ok(65536),
            BitDepth::Unknown | BitDepth::UInt32 => {
                crate::bail!(
                    "Bit-depth is not supported: {}",
                    incoming_bit_depth.as_str()
                )
            }
        }
    }

    fn get_lut_ideal_size_with_flags(
        input_bit_depth: BitDepth,
        half_flags: HalfFlags,
    ) -> Result<usize> {
        // For half domain always return 65536, since that is what fill() expects.
        if half_flags.is_input_half_domain() {
            return Ok(HALF_DOMAIN_REQUIRED_ENTRIES);
        }
        Self::get_lut_ideal_size(input_bit_depth)
    }

    /// Identity LUT with a domain suitable to do a look-up for the given
    /// input depth (half domain for float depths).
    pub fn make_lookup_domain(incoming_depth: BitDepth) -> Result<Lut1DOpData> {
        let domain_type = if incoming_depth.is_float() {
            HalfFlags::INPUT_HALF_CODE
        } else {
            HalfFlags::STANDARD
        };
        let ideal_size = Self::get_lut_ideal_size_with_flags(incoming_depth, domain_type)?;
        Self::new_with_flags(domain_type, ideal_size, true)
    }

    /// Evaluate the LUT values (used as domain) through `ops` (port of
    /// `ComposeVec`). The caller must ensure `ops` has no channel crosstalk;
    /// the domain is not resized, and hue adjust is not propagated.
    pub fn compose_vec(lut: &mut Lut1DOpData, ops: &[OpRc]) -> Result<()> {
        if ops.is_empty() {
            crate::bail!("There is nothing to compose the 1D LUT with");
        }
        let length = lut.array.length();
        lut.array.resize(length, 3)?;
        eval_transform(&mut lut.array.values, ops);
        Ok(())
    }

    /// Functional composition of two LUTs into one (port of
    /// `Lut1DOpData::Compose`). Callers should check
    /// [`Lut1DOpData::may_compose`] first since hue adjust is not composable.
    pub fn compose(
        lutc1: &Lut1DOpData,
        lutc2: &Lut1DOpData,
        method: ComposeMethod,
    ) -> Result<Lut1DOpData> {
        let mut lut1 = lutc1.clone();
        let mut lut2 = lutc2.clone();
        let mut restore_inverse = false;
        if lut1.direction == TransformDirection::Inverse
            && lut2.direction == TransformDirection::Inverse
        {
            // Using the fact that: inv(l2 x l1) = inv(l1) x inv(l2).
            // Compute l2 x l1 and invert the result.
            std::mem::swap(&mut lut1, &mut lut2);
            lut1.direction = TransformDirection::Forward;
            lut2.direction = TransformDirection::Forward;
            restore_inverse = true;
        }

        let (min_size, need_half_domain) = match method {
            ComposeMethod::ResampleNo => (0, false),
            ComposeMethod::ResampleBig => (65536, false),
            ComposeMethod::ResampleHd => (65536, true),
        };

        let lut1_size = lutc1.array.length();
        let good_domain =
            lut1.is_input_half_domain() || (lut1_size >= min_size && !need_half_domain);
        let use_orig_domain = method == ComposeMethod::ResampleNo;

        let mut ops = OpVec::new();
        let mut result;
        // When lut1 is an inverse LUT (and lut2 is not), interpolate through
        // both LUTs.
        if (!good_domain && !use_orig_domain) || lut1.direction == TransformDirection::Inverse {
            ops.push(Arc::new(Lut1DOp::new(lut1.clone())?));
            // Create an identity with a finer domain.
            result = if min_size == 0 || lut1.direction == TransformDirection::Inverse {
                Self::make_lookup_domain(BitDepth::F16)?
            } else {
                let flags = if need_half_domain {
                    HalfFlags::INPUT_HALF_CODE
                } else {
                    HalfFlags::STANDARD
                };
                Self::new_with_flags(flags, min_size, true)?
            };
            result.interpolation = lut1.interpolation;
            result.metadata = lut1.metadata.clone();
        } else {
            result = lut1.clone();
        }

        ops.push(Arc::new(Lut1DOp::new(lut2.clone())?));

        // Create the result LUT by composing the domain through the ops
        // (always using the exact inversion).
        Self::compose_vec(&mut result, &ops)?;

        result.metadata.combine(&lut2.metadata);

        // Taking these from lut2 since the common use case is for lut2 to be
        // the original LUT and lut1 to be a new domain.
        result.set_hue_adjust(lut2.hue_adjust)?;

        if restore_inverse {
            result.direction = TransformDirection::Inverse;
        }

        // The result needs to be ready for further composition or rendering.
        result.finalize()?;
        Ok(result)
    }
}

/// Make a forward LUT approximating the exact inverse of an inverse LUT
/// (used by the fast inversion style, `OptimizationFlags::LUT_INV_FAST`).
///
/// The domain is chosen from the file output bit-depth of the LUT (12i when
/// unknown) or a half domain if the LUT has values outside `[0, 1]`.
pub fn make_fast_lut1d_from_inverse(lut: &Lut1DOpData) -> Result<Lut1DOpData> {
    if lut.direction() != TransformDirection::Inverse {
        crate::bail!("MakeFastLut1DFromInverse expects an inverse 1D LUT");
    }
    let mut depth = lut.file_output_bit_depth();
    if matches!(
        depth,
        BitDepth::Unknown | BitDepth::UInt14 | BitDepth::UInt32
    ) {
        depth = BitDepth::UInt12;
    }
    // If the LUT has values outside [0,1], use a half-domain fast LUT.
    if lut.has_extended_range() {
        depth = BitDepth::F16;
    }
    let new_domain = Lut1DOpData::make_lookup_domain(depth)?;
    Lut1DOpData::compose(&new_domain, lut, ComposeMethod::ResampleNo)
}

// ---------------------------------------------------------------------------
// Op.

/// A 1D LUT op. The data is finalized at construction; the CPU renderer is
/// built on first use.
#[derive(Clone)]
pub struct Lut1DOp {
    data: Arc<Lut1DOpData>,
    renderer: OnceLock<Arc<Lut1DRenderer>>,
}

impl fmt::Debug for Lut1DOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Lut1DOp").field("data", &self.data).finish()
    }
}

impl Lut1DOp {
    /// Create an op (the data is finalized).
    pub fn new(data: Lut1DOpData) -> Result<Self> {
        let mut data = data;
        data.finalize()?;
        Ok(Self {
            data: Arc::new(data),
            renderer: OnceLock::new(),
        })
    }

    /// The (finalized) LUT data.
    pub fn data(&self) -> &Lut1DOpData {
        &self.data
    }

    fn renderer(&self) -> &Lut1DRenderer {
        self.renderer
            .get_or_init(|| Arc::new(Lut1DRenderer::new(&self.data)))
    }

    /// The ops replacing this op if it is an identity (`None` if it is not
    /// an identity or if the replacement cannot be built).
    pub fn identity_replacement_ops(&self) -> Option<OpVec> {
        if self.data.is_identity() {
            identity_replacement_ops(self.data.identity_replacement())
        } else {
            None
        }
    }
}

impl Op for Lut1DOp {
    fn name(&self) -> &'static str {
        "Lut1D"
    }

    fn apply(&self, pixels: &mut [Pixel]) {
        self.renderer().apply(pixels);
    }

    fn is_no_op(&self) -> bool {
        self.data.is_no_op()
    }

    /// Only half-domain identities are reported: a standard-domain identity
    /// still clamps to `[0, 1]` (see [`Lut1DOp::identity_replacement_ops`]).
    fn is_identity(&self) -> bool {
        self.data.is_no_op()
    }

    fn has_channel_crosstalk(&self) -> bool {
        self.data.has_channel_crosstalk()
    }

    fn cache_id(&self) -> String {
        format!("<Lut1D {}>", self.data.cache_id())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn combine_with(&self, next: &dyn Op, flags: OptimizationFlags) -> Option<OpVec> {
        let other = next.downcast_ref::<Lut1DOp>()?;
        if flags.contains(OptimizationFlags::PAIR_IDENTITY_LUT1D)
            && self.data.is_inverse(&other.data)
        {
            // Emulate the clamping done by the original pair.
            let replacement = self.data.pair_identity_replacement(&other.data);
            if let Some(ops) = identity_replacement_ops(replacement) {
                return Some(ops);
            }
        }
        if flags.contains(OptimizationFlags::COMP_LUT1D) && self.data.may_compose(&other.data) {
            // Upsample the LUTs to minimize precision loss.
            let result =
                Lut1DOpData::compose(&self.data, &other.data, ComposeMethod::ResampleBig).ok()?;
            let op = Lut1DOp::new(result).ok()?;
            return Some(vec![Arc::new(op)]);
        }
        None
    }

    fn to_transform(&self) -> Option<Transform> {
        Some(Transform::Lut1D(self.data.to_transform()))
    }

    fn clone_box(&self) -> Box<dyn Op> {
        Box::new(self.clone())
    }
}

// ---------------------------------------------------------------------------
// Builders & helpers.

/// Append a 1D LUT op built from op data (port of `CreateLut1DOp`). With
/// `TransformDirection::Inverse`, the LUT direction is flipped.
pub fn create_lut1d_op_from_data(
    ops: &mut OpVec,
    lut: &Lut1DOpData,
    dir: TransformDirection,
) -> Result<()> {
    let data = match dir {
        TransformDirection::Forward => lut.clone(),
        TransformDirection::Inverse => lut.inverse(),
    };
    ops.push(Arc::new(Lut1DOp::new(data)?));
    Ok(())
}

/// Append the op of a [`Lut1DTransform`] (validated) in the requested
/// direction (port of `BuildLut1DOp`).
pub fn create_lut1d_op(
    ops: &mut OpVec,
    lut: &Lut1DTransform,
    dir: TransformDirection,
) -> Result<()> {
    let data = Lut1DOpData::from_transform(lut)?;
    data.validate()?;
    create_lut1d_op_from_data(ops, &data, dir)
}

/// Fill `img` (`num_elements` pixels of `num_channels`) with an identity
/// ramp in `[0, 1]` (only the first 3 channels are written).
pub fn generate_identity_lut1d(img: &mut [f32], num_elements: usize, num_channels: usize) {
    let fill = num_channels.min(3);
    let scale = 1.0f32 / (num_elements as f32 - 1.0);
    for i in 0..num_elements {
        for c in 0..fill {
            if let Some(v) = img.get_mut(num_channels * i + c) {
                *v = scale * i as f32;
            }
        }
    }
}

/// Fill `img` with a linear ramp from `start` to `end`.
pub fn generate_linear_scale_lut1d(
    img: &mut [f32],
    num_elements: usize,
    num_channels: usize,
    start: f32,
    end: f32,
) {
    let fill = num_channels.min(3);
    for i in 0..num_elements {
        let x = (i as f64 / (num_elements as f64 - 1.0)) as f32;
        let val = lerpf(start, end, x);
        for c in 0..fill {
            if let Some(v) = img.get_mut(num_channels * i + c) {
                *v = val;
            }
        }
    }
}

/// Bake an op list (which must have no channel crosstalk) into a 1D LUT
/// sampled for a look-up at `in_bit_depth` (half domain for float depths),
/// as the separable prefix optimization does.
pub fn bake_ops_to_lut1d(ops: &[OpRc], in_bit_depth: BitDepth) -> Result<Lut1DOpData> {
    let mut lut = Lut1DOpData::make_lookup_domain(in_bit_depth)?;
    Lut1DOpData::compose_vec(&mut lut, ops)?;
    lut.finalize()?;
    Ok(lut)
}

/// Replace the 1D and 3D LUTs using inverse evaluation by faster forward
/// approximations when `flags` contains `LUT_INV_FAST` (port of
/// `ReplaceInverseLuts`). Returns the number of replaced ops.
pub fn replace_inverse_luts(ops: &mut OpVec, flags: OptimizationFlags) -> Result<usize> {
    if !flags.contains(OptimizationFlags::LUT_INV_FAST) {
        return Ok(0);
    }
    let mut count = 0;
    for op in ops.iter_mut() {
        if let Some(lut) = op.downcast_ref::<Lut1DOp>() {
            if lut.data().direction() == TransformDirection::Inverse {
                let fast = make_fast_lut1d_from_inverse(lut.data())?;
                *op = Arc::new(Lut1DOp::new(fast)?);
                count += 1;
            }
        } else if let Some(lut) = op.downcast_ref::<crate::ops::lut3d::Lut3DOp>() {
            if lut.data().direction() == TransformDirection::Inverse {
                let fast = crate::ops::lut3d::make_fast_lut3d_from_inverse(lut.data())?;
                *op = Arc::new(crate::ops::lut3d::Lut3DOp::new(fast)?);
                count += 1;
            }
        }
    }
    Ok(count)
}

/// Replace the identity LUTs by their simpler replacement (a clamp for a
/// standard-domain 1D LUT) when `flags` contains `IDENTITY` (the LUT part of
/// `ReplaceIdentityOps`). Returns the number of replaced ops.
pub fn replace_identity_luts(ops: &mut OpVec, flags: OptimizationFlags) -> usize {
    if !flags.contains(OptimizationFlags::IDENTITY) {
        return 0;
    }
    let mut count = 0;
    let mut out = OpVec::with_capacity(ops.len());
    for op in ops.drain(..) {
        let replacement = op
            .downcast_ref::<Lut1DOp>()
            .and_then(|l| l.identity_replacement_ops());
        match replacement {
            Some(r) => {
                out.extend(r);
                count += 1;
            }
            None => out.push(op),
        }
    }
    *ops = out;
    count
}

/// Replace the leading separable ops (no channel crosstalk, not dynamic) by
/// a single 1D LUT sampled for a look-up at `in_bit_depth` (port of
/// `OptimizeSeparablePrefix`). Nothing is done for 32f / 32i inputs or when
/// the prefix only contains cheap ops (matrices and ranges) or a single
/// forward 1D LUT.
pub fn optimize_separable_prefix(ops: &mut OpVec, in_bit_depth: BitDepth) -> Result<()> {
    if ops.is_empty() || in_bit_depth == BitDepth::F32 || in_bit_depth == BitDepth::UInt32 {
        return Ok(());
    }
    let prefix_len = ops
        .iter()
        .position(|op| op.has_channel_crosstalk() || op.is_dynamic())
        .unwrap_or(ops.len());
    if prefix_len == 0 {
        return Ok(());
    }
    // A single forward 1D LUT is left alone (an inverse one is replaced).
    if prefix_len == 1 {
        if let Some(lut) = ops[0].downcast_ref::<Lut1DOp>() {
            if lut.data().direction() == TransformDirection::Forward {
                return Ok(());
            }
        }
    }
    let has_expensive_ops = ops[..prefix_len]
        .iter()
        .any(|op| op.name() != "Matrix" && op.name() != "Range");
    if !has_expensive_ops {
        return Ok(());
    }
    let lut = bake_ops_to_lut1d(&ops[..prefix_len], in_bit_depth)?;
    let op: OpRc = Arc::new(Lut1DOp::new(lut)?);
    ops.splice(0..prefix_len, std::iter::once(op));
    Ok(())
}

// ---------------------------------------------------------------------------
// Transform.

impl Validate for Lut1DTransform {
    fn validate(&self) -> Result<()> {
        let res = Lut1DOpData::from_transform(self).and_then(|d| d.validate());
        res.map_err(|e| Error::msg(format!("Lut1DTransform validation failed: {}", e.message())))
    }
}

impl BuildOps for Lut1DTransform {
    fn build_ops(
        &self,
        ops: &mut OpVec,
        _config: &Config,
        _context: &Context,
        dir: TransformDirection,
    ) -> Result<()> {
        create_lut1d_op(ops, self, dir)
    }
}

/// Format a float like C++ `std::ostream` does by default (`%g`, 6
/// significant digits).
pub(crate) fn format_float_g(v: f32) -> String {
    let v = v as f64;
    if v.is_nan() {
        return "nan".to_string();
    }
    if v.is_infinite() {
        return if v > 0.0 {
            "inf".to_string()
        } else {
            "-inf".to_string()
        };
    }
    if v == 0.0 {
        return if v.is_sign_negative() {
            "-0".to_string()
        } else {
            "0".to_string()
        };
    }
    let sci = format!("{v:.5e}");
    let (mantissa, exp) = sci.split_once('e').unwrap_or((&sci, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    fn strip(s: &str) -> String {
        if s.contains('.') {
            s.trim_end_matches('0').trim_end_matches('.').to_string()
        } else {
            s.to_string()
        }
    }
    if !(-4..6).contains(&exp) {
        let sign = if exp < 0 { '-' } else { '+' };
        format!("{}e{}{:02}", strip(mantissa), sign, exp.abs())
    } else {
        let decimals = (5 - exp).max(0) as usize;
        strip(&format!("{v:.decimals$}"))
    }
}

/// Min and max of each channel, ignoring NaNs (as `std::min`/`std::max`).
pub(crate) fn rgb_min_max(values: &[f32]) -> ([f32; 3], [f32; 3]) {
    let mut mn = [f32::MAX; 3];
    let mut mx = [-f32::MAX; 3];
    for rgb in values.chunks_exact(3) {
        for c in 0..3 {
            mn[c] = std_min(mn[c], rgb[c]);
            mx[c] = std_max(mx[c], rgb[c]);
        }
    }
    (mn, mx)
}

impl fmt::Display for Lut1DTransform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let hue = match self.hue_adjust {
            Lut1DHueAdjust::None => 0,
            Lut1DHueAdjust::Dw3 => 1,
            Lut1DHueAdjust::Wypn => 2,
        };
        write!(
            f,
            "<Lut1DTransform direction={}, fileoutdepth={}, interpolation={}, inputhalf={}, outputrawhalf={}, \
             hueadjust={}, ",
            self.direction.as_str(),
            self.file_output_bit_depth.as_str(),
            self.interpolation.as_str(),
            self.input_half_domain as i32,
            self.output_raw_halfs as i32,
            hue
        )?;
        let l = self.length();
        write!(f, "length={l}, ")?;
        if l > 0 {
            let (mn, mx) = rgb_min_max(&self.values[..l * 3]);
            write!(
                f,
                "minrgb=[{}, {}, {}], maxrgb=[{}, {}, {}]",
                format_float_g(mn[0]),
                format_float_g(mn[1]),
                format_float_g(mn[2]),
                format_float_g(mx[0]),
                format_float_g(mx[1]),
                format_float_g(mx[2])
            )?;
        }
        write!(f, ">")
    }
}
