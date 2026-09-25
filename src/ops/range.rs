//! Range op: affine remap with clamping (port of `RangeOpData`, `RangeOp`,
//! `RangeOpCPU` and the op building part of `RangeTransform.cpp`).
//!
//! A range always clamps: the `noClamp` style of `RangeTransform` is built
//! as a matrix op. Unset bounds are represented by NaN (as in OCIO, see
//! [`RangeOpData::EMPTY`]). Ops are stored in the forward direction (OCIO
//! does it in `RangeOp::finalize`).

use crate::config::Config;
use crate::context::Context;
use crate::error::{Error, Result};
use crate::format_metadata::FormatMetadata;
use crate::math_utils::clamp_f32;
use crate::ops::matrix::float_format::format_g;
use crate::ops::matrix::{create_matrix_op_from_data, MatrixOpData};
use crate::ops::{Op, OpVec, Pixel};
use crate::transforms::{BuildOps, RangeTransform, Transform, Validate};
use crate::types::{BitDepth, OptimizationFlags, RangeStyle, TransformDirection};
use std::any::Any;
use std::sync::Arc;

/// Parameters of a [`RangeOp`] (port of OCIO's `RangeOpData`).
#[derive(Debug, Clone)]
pub struct RangeOpData {
    /// Lower bound of the domain (NaN if unset).
    pub min_in: f64,
    /// Upper bound of the domain (NaN if unset).
    pub max_in: f64,
    /// Lower bound of the range (NaN if unset).
    pub min_out: f64,
    /// Upper bound of the range (NaN if unset).
    pub max_out: f64,
    /// Direction.
    pub direction: TransformDirection,
    /// Bit depth used for file I/O (informational).
    pub file_input_bit_depth: BitDepth,
    /// Bit depth used for file I/O (informational).
    pub file_output_bit_depth: BitDepth,
    /// Metadata.
    pub metadata: FormatMetadata,
}

impl Default for RangeOpData {
    /// All the bounds unset (not valid).
    fn default() -> Self {
        Self {
            min_in: Self::EMPTY,
            max_in: Self::EMPTY,
            min_out: Self::EMPTY,
            max_out: Self::EMPTY,
            direction: TransformDirection::Forward,
            file_input_bit_depth: BitDepth::Unknown,
            file_output_bit_depth: BitDepth::Unknown,
            metadata: FormatMetadata::default(),
        }
    }
}

impl PartialEq for RangeOpData {
    /// Equality as defined by OCIO (with a small tolerance, ignoring the
    /// metadata and the file bit depths).
    fn eq(&self, other: &Self) -> bool {
        if self.direction != other.direction {
            return false;
        }
        if self.min_is_empty() != other.min_is_empty()
            || self.max_is_empty() != other.max_is_empty()
        {
            return false;
        }
        if !self.min_is_empty()
            && !other.min_is_empty()
            && (floats_differ(self.min_in, other.min_in)
                || floats_differ(self.min_out, other.min_out))
        {
            return false;
        }
        if !self.max_is_empty()
            && !other.max_is_empty()
            && (floats_differ(self.max_in, other.max_in)
                || floats_differ(self.max_out, other.max_out))
        {
            return false;
        }
        true
    }
}

/// True if the value represents an unset bound (NaN in single precision).
fn is_empty(v: f64) -> bool {
    (v as f32).is_nan()
}

/// Hybrid absolute/relative comparison used by the range op.
fn floats_differ(x1: f64, x2: f64) -> bool {
    if x1.abs() < 1e-3 {
        (x1 - x2).abs() > 1e-6
    } else {
        (1.0 - (x2 / x1)).abs() > 1e-6
    }
}

/// Convert an optional bound to the NaN convention.
fn opt_to_value(v: Option<f64>) -> f64 {
    v.unwrap_or(RangeOpData::EMPTY)
}

/// Convert a value using the NaN convention to an optional bound.
fn value_to_opt(v: f64) -> Option<f64> {
    if is_empty(v) {
        None
    } else {
        Some(v)
    }
}

impl RangeOpData {
    /// Value of an unset bound.
    pub const EMPTY: f64 = f64::NAN;

    /// Value of an unset bound (port of `RangeOpData::EmptyValue`).
    pub fn empty_value() -> f64 {
        Self::EMPTY
    }

    /// A validated forward range (use [`RangeOpData::EMPTY`] for unset
    /// bounds).
    pub fn new(min_in: f64, max_in: f64, min_out: f64, max_out: f64) -> Result<Self> {
        Self::with_direction(
            min_in,
            max_in,
            min_out,
            max_out,
            TransformDirection::Forward,
        )
    }

    /// A validated range in the given direction.
    pub fn with_direction(
        min_in: f64,
        max_in: f64,
        min_out: f64,
        max_out: f64,
        direction: TransformDirection,
    ) -> Result<Self> {
        let r = Self {
            min_in,
            max_in,
            min_out,
            max_out,
            direction,
            ..Self::default()
        };
        r.validate()?;
        Ok(r)
    }

    /// Build from the parameters of a [`RangeTransform`] (not validated).
    pub fn from_transform(t: &RangeTransform) -> Self {
        Self {
            min_in: opt_to_value(t.min_in),
            max_in: opt_to_value(t.max_in),
            min_out: opt_to_value(t.min_out),
            max_out: opt_to_value(t.max_out),
            direction: t.direction,
            file_input_bit_depth: t.file_input_bit_depth,
            file_output_bit_depth: t.file_output_bit_depth,
            metadata: t.metadata.clone(),
        }
    }

    /// True if the lower input bound is set.
    pub fn has_min_in(&self) -> bool {
        !is_empty(self.min_in)
    }
    /// True if the upper input bound is set.
    pub fn has_max_in(&self) -> bool {
        !is_empty(self.max_in)
    }
    /// True if the lower output bound is set.
    pub fn has_min_out(&self) -> bool {
        !is_empty(self.min_out)
    }
    /// True if the upper output bound is set.
    pub fn has_max_out(&self) -> bool {
        !is_empty(self.max_out)
    }
    /// True if there is no lower bound.
    pub fn min_is_empty(&self) -> bool {
        is_empty(self.min_in)
    }
    /// True if there is no upper bound.
    pub fn max_is_empty(&self) -> bool {
        is_empty(self.max_in)
    }

    /// The `id` metadata attribute.
    pub fn id(&self) -> &str {
        self.metadata.id()
    }

    /// Validate the bounds (and that the scale can be computed).
    pub fn validate(&self) -> Result<()> {
        let min_in_empty = is_empty(self.min_in);
        let max_in_empty = is_empty(self.max_in);
        let min_out_empty = is_empty(self.min_out);
        let max_out_empty = is_empty(self.max_out);

        if min_in_empty != min_out_empty {
            crate::bail!("In and out minimum limits must be both set or both missing in Range.");
        }
        if max_in_empty {
            if !max_out_empty {
                crate::bail!(
                    "In and out maximum limits must be both set or both missing in Range."
                );
            }
            if min_in_empty {
                crate::bail!("At least minimum or maximum limits must be set in Range.");
            }
        } else if max_out_empty {
            crate::bail!("In and out maximum limits must be both set or both missing in Range.");
        }

        // Currently not allowing polarity inversion so enforce max > min.
        if !min_in_empty && !max_in_empty {
            if self.min_in > self.max_in {
                crate::bail!("Range maximum input value is less than minimum input value");
            }
            if self.min_out > self.max_out {
                crate::bail!("Range maximum output value is less than minimum output value");
            }
        }

        // A one-sided clamp must have matching in & out values.
        if max_in_empty && !min_in_empty && floats_differ(self.min_out, self.min_in) {
            crate::bail!(
                "In and out minimum limits must be equal if maximum values are missing in Range."
            );
        }
        if min_in_empty && !max_in_empty && floats_differ(self.max_out, self.max_in) {
            crate::bail!(
                "In and out maximum limits must be equal if minimum values are missing in Range."
            );
        }

        self.scale_offset()?;
        Ok(())
    }

    /// Scale and offset such that `out = in * scale + offset`.
    pub fn scale_offset(&self) -> Result<(f64, f64)> {
        if self.min_is_empty() || self.max_is_empty() {
            return Ok((1.0, 0.0));
        }
        let denom = self.max_in - self.min_in;
        if denom.abs() < 1e-6 {
            crate::bail!("Range maxInValue is too close to minInValue");
        }
        // NB: Allowing out min == max as it could be useful to create a constant.
        let scale = (self.max_out - self.min_out) / denom;
        let offset = self.min_out - scale * self.min_in;
        Ok((scale, offset))
    }

    /// The scale (1 if it cannot be computed).
    pub fn scale(&self) -> f64 {
        self.scale_offset().map(|s| s.0).unwrap_or(1.0)
    }

    /// The offset (0 if it cannot be computed).
    pub fn offset(&self) -> f64 {
        self.scale_offset().map(|s| s.1).unwrap_or(0.0)
    }

    /// A range always clamps, so it is never a no-op.
    pub fn is_no_op(&self) -> bool {
        false
    }

    /// An identity range does not modify values in `[0, 1]` but may clamp
    /// values outside that domain.
    pub fn is_identity(&self) -> bool {
        if self.scales() {
            return false;
        }
        if !self.min_is_empty() && self.min_in > 0.0 {
            return false;
        }
        if !self.max_is_empty() && self.max_in < 1.0 {
            return false;
        }
        true
    }

    /// True if the range clamps to (a subset of) `[0, 1]`.
    pub fn clamps_to_lut_domain(&self) -> bool {
        if self.min_is_empty() || self.min_in < 0.0 {
            return false;
        }
        if self.max_is_empty() || self.max_in > 1.0 {
            return false;
        }
        true
    }

    /// True if the range only clamps negative values.
    pub fn is_clamp_negs(&self) -> bool {
        self.max_is_empty() && !self.min_is_empty() && self.min_in == 0.0
    }

    /// True if the scale is not 1 or the offset not 0.
    pub fn scales(&self) -> bool {
        let (scale, offset) = self.scale_offset().unwrap_or((1.0, 0.0));
        if offset.abs() > 1e-6 {
            return true;
        }
        floats_differ(scale, 1.0)
    }

    /// Range never has channel crosstalk.
    pub fn has_channel_crosstalk(&self) -> bool {
        false
    }

    /// The equivalent forward range (in and out bounds swapped if inverse).
    pub fn get_as_forward(&self) -> Result<RangeOpData> {
        if self.direction == TransformDirection::Forward {
            return Ok(self.clone());
        }
        let inv = RangeOpData {
            min_in: self.min_out,
            max_in: self.max_out,
            min_out: self.min_in,
            max_out: self.max_in,
            direction: TransformDirection::Forward,
            file_input_bit_depth: self.file_output_bit_depth,
            file_output_bit_depth: self.file_input_bit_depth,
            metadata: self.metadata.clone(),
        };
        inv.validate()?;
        Ok(inv)
    }

    /// The equivalent matrix of a non-clamping range (both bounds must be
    /// set).
    pub fn convert_to_matrix(&self) -> Result<MatrixOpData> {
        if self.min_is_empty() || self.max_is_empty() {
            crate::bail!("Non-clamping Range min & max values have to be set.");
        }
        let fwd = self.get_as_forward()?;
        let (scale, offset) = fwd.scale_offset()?;
        let mut mtx = MatrixOpData::new();
        mtx.metadata = fwd.metadata.clone();
        mtx.file_input_bit_depth = fwd.file_input_bit_depth;
        mtx.file_output_bit_depth = fwd.file_output_bit_depth;
        mtx.matrix[0] = scale;
        mtx.matrix[5] = scale;
        mtx.matrix[10] = scale;
        mtx.offsets = [offset, offset, offset, 0.0];
        mtx.validate()?;
        Ok(mtx)
    }

    /// Compose `self` followed by `r` (both forward).
    pub fn compose(&self, r: &RangeOpData) -> Result<RangeOpData> {
        let (scale, offset) = self.scale_offset()?;
        let (r_scale, r_offset) = r.scale_offset()?;

        let mut min_in_new = self.min_in;
        let mut max_in_new = self.max_in;
        let mut min_out_new = r.min_out;
        let mut max_out_new = r.max_out;

        if !self.min_is_empty() {
            if !r.max_is_empty() && self.min_out >= r.max_in {
                // Range outputting a constant value.
                return RangeOpData::new(self.min_in, self.max_in, r.max_out, r.max_out);
            } else if !r.min_is_empty() {
                if self.min_out >= r.min_in {
                    // Transform min_out with r.
                    min_out_new = self.min_out * r_scale + r_offset;
                } else {
                    // Transform r.min_in with the inverse of self.
                    min_in_new = (r.min_in - offset) / scale;
                }
            } else {
                min_out_new = self.min_out;
            }
        } else if !r.min_is_empty() {
            min_in_new = r.min_in;
        }

        if !self.max_is_empty() {
            if !r.min_is_empty() && self.max_out <= r.min_in {
                // Range outputting a constant value.
                return RangeOpData::new(self.min_in, self.max_in, r.min_out, r.min_out);
            } else if !r.max_is_empty() {
                if self.max_out <= r.max_in {
                    // Transform max_out with r.
                    max_out_new = self.max_out * r_scale + r_offset;
                } else {
                    // Transform r.max_in with the inverse of self.
                    max_in_new = (r.max_in - offset) / scale;
                }
            } else {
                max_out_new = self.max_out;
            }
        } else if !r.max_is_empty() {
            max_in_new = r.max_in;
        }

        RangeOpData::new(min_in_new, max_in_new, min_out_new, max_out_new)
    }

    /// Scale the bounds from the file bit depths to normalized values.
    pub fn normalize(&mut self) {
        let in_scale = 1.0 / self.file_input_bit_depth.max_value();
        let out_scale = 1.0 / self.file_output_bit_depth.max_value();
        if !self.min_is_empty() {
            self.min_in *= in_scale;
            self.min_out *= out_scale;
        }
        if !self.max_is_empty() {
            self.max_in *= in_scale;
            self.max_out *= out_scale;
        }
    }

    /// Cache id of the parameters.
    pub fn cache_id(&self) -> String {
        let mut s = String::new();
        if !self.id().is_empty() {
            s.push_str(self.id());
            s.push(' ');
        }
        s.push_str(self.direction.as_str());
        s.push(' ');
        s.push_str(&format!(
            "[{}, {}, {}, {}]",
            format_g(self.min_in, 7),
            format_g(self.max_in, 7),
            format_g(self.min_out, 7),
            format_g(self.max_out, 7)
        ));
        s
    }
}

// ---------------------------------------------------------------------------
// CPU renderers.

/// CPU renderer chosen from the range parameters (as `GetRangeRenderer`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RangeRenderer {
    /// Scale, offset and clamp at both ends.
    ScaleMinMax {
        scale: f32,
        offset: f32,
        lower: f32,
        upper: f32,
    },
    /// Clamp at both ends.
    MinMax { lower: f32, upper: f32 },
    /// Clamp the low end.
    Min { lower: f32 },
    /// Clamp the high end.
    Max { upper: f32 },
}

impl RangeRenderer {
    /// Select and initialize the renderer for a forward range.
    pub fn new(range: &RangeOpData) -> Result<Self> {
        if range.direction == TransformDirection::Inverse {
            return Err(Error::msg("Op::finalize has to be called."));
        }
        let (scale, offset) = range.scale_offset()?;
        let lower = range.min_out as f32;
        let upper = range.max_out as f32;
        Ok(if range.min_is_empty() {
            RangeRenderer::Max { upper }
        } else if range.max_is_empty() {
            RangeRenderer::Min { lower }
        } else if !range.scales() {
            RangeRenderer::MinMax { lower, upper }
        } else {
            RangeRenderer::ScaleMinMax {
                scale: scale as f32,
                offset: offset as f32,
                lower,
                upper,
            }
        })
    }

    /// Process pixels in place (alpha is not modified).
    pub fn apply(&self, pixels: &mut [Pixel]) {
        match *self {
            RangeRenderer::ScaleMinMax {
                scale,
                offset,
                lower,
                upper,
            } => {
                for p in pixels.iter_mut() {
                    for v in p.iter_mut().take(3) {
                        // NaNs become the lower bound.
                        *v = clamp_f32(*v * scale + offset, lower, upper);
                    }
                }
            }
            RangeRenderer::MinMax { lower, upper } => {
                for p in pixels.iter_mut() {
                    for v in p.iter_mut().take(3) {
                        *v = clamp_f32(*v, lower, upper);
                    }
                }
            }
            RangeRenderer::Min { lower } => {
                for p in pixels.iter_mut() {
                    for v in p.iter_mut().take(3) {
                        // std::max(lower, v): NaNs become the lower bound.
                        *v = if lower < *v { *v } else { lower };
                    }
                }
            }
            RangeRenderer::Max { upper } => {
                for p in pixels.iter_mut() {
                    for v in p.iter_mut().take(3) {
                        // std::min(upper, v): NaNs become the upper bound.
                        *v = if *v < upper { *v } else { upper };
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The op.

/// Range op (port of OCIO's `RangeOp`). The data is always forward.
#[derive(Debug, Clone)]
pub struct RangeOp {
    data: RangeOpData,
    renderer: RangeRenderer,
}

impl RangeOp {
    /// Create the op; an inverse range is converted to forward.
    pub fn new(data: RangeOpData) -> Result<Self> {
        data.validate()?;
        let data = data.get_as_forward()?;
        let renderer = RangeRenderer::new(&data)?;
        Ok(Self { data, renderer })
    }

    /// The (forward) parameters.
    pub fn data(&self) -> &RangeOpData {
        &self.data
    }

    /// The CPU renderer.
    pub fn renderer(&self) -> &RangeRenderer {
        &self.renderer
    }
}

/// True if `op` is a forward Lut1D (not half-domain) or a forward Lut3D, the
/// ops an identity range can be removed in front of.
fn is_lut_accepting_identity_range(op: &dyn Op) -> bool {
    let name = op.name().to_ascii_lowercase();
    if !name.contains("lut1d") && !name.contains("lut3d") {
        return false;
    }
    match op.to_transform() {
        Some(Transform::Lut1D(l)) => {
            !l.input_half_domain && l.direction == TransformDirection::Forward
        }
        Some(Transform::Lut3D(l)) => l.direction == TransformDirection::Forward,
        _ => false,
    }
}

impl Op for RangeOp {
    fn name(&self) -> &'static str {
        "Range"
    }

    fn apply(&self, pixels: &mut [Pixel]) {
        self.renderer.apply(pixels);
    }

    fn is_no_op(&self) -> bool {
        false
    }

    fn has_channel_crosstalk(&self) -> bool {
        false
    }

    fn cache_id(&self) -> String {
        format!("<RangeOp {} >", self.data.cache_id())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn combine_with(&self, next: &dyn Op, flags: OptimizationFlags) -> Option<OpVec> {
        if !flags.contains(OptimizationFlags::COMP_RANGE) {
            return None;
        }
        if let Some(other) = next.downcast_ref::<RangeOp>() {
            let res = self.data.compose(&other.data).ok()?;
            let op = RangeOp::new(res).ok()?;
            return Some(vec![Arc::new(op)]);
        }
        // An identity range in front of a LUT can be removed (the LUT clamps
        // to its domain).
        if self.data.is_identity() && is_lut_accepting_identity_range(next) {
            return Some(vec![Arc::from(next.clone_box())]);
        }
        None
    }

    fn to_transform(&self) -> Option<Transform> {
        Some(Transform::Range(RangeTransform {
            direction: TransformDirection::Forward,
            style: RangeStyle::Clamp,
            min_in: value_to_opt(self.data.min_in),
            max_in: value_to_opt(self.data.max_in),
            min_out: value_to_opt(self.data.min_out),
            max_out: value_to_opt(self.data.max_out),
            file_input_bit_depth: self.data.file_input_bit_depth,
            file_output_bit_depth: self.data.file_output_bit_depth,
            metadata: self.data.metadata.clone(),
        }))
    }

    fn clone_box(&self) -> Box<dyn Op> {
        Box::new(self.clone())
    }
}

// ---------------------------------------------------------------------------
// Op builders.

/// Append a range op built from `data` in direction `dir` (combined with the
/// data direction).
pub fn create_range_op_from_data(
    ops: &mut OpVec,
    data: &RangeOpData,
    dir: TransformDirection,
) -> Result<()> {
    let mut r = data.clone();
    r.direction = r.direction.combine(dir);
    ops.push(Arc::new(RangeOp::new(r)?));
    Ok(())
}

/// Append a range op (use [`RangeOpData::EMPTY`] for unset bounds).
pub fn create_range_op(
    ops: &mut OpVec,
    min_in: f64,
    max_in: f64,
    min_out: f64,
    max_out: f64,
    dir: TransformDirection,
) -> Result<()> {
    let data = RangeOpData::new(min_in, max_in, min_out, max_out)?;
    create_range_op_from_data(ops, &data, dir)
}

// ---------------------------------------------------------------------------
// RangeTransform.

impl RangeTransform {
    /// Equality as defined by OCIO (`RangeTransform::equals`): style,
    /// direction and bounds (with a small tolerance); metadata and file bit
    /// depths are ignored.
    pub fn equals(&self, other: &RangeTransform) -> bool {
        self.style == other.style
            && RangeOpData::from_transform(self) == RangeOpData::from_transform(other)
    }
}

impl Validate for RangeTransform {
    fn validate(&self) -> Result<()> {
        let check = || -> Result<()> {
            let data = RangeOpData::from_transform(self);
            data.validate()?;
            if self.style == RangeStyle::NoClamp && (data.min_is_empty() || data.max_is_empty()) {
                crate::bail!(
                    "RangeTransform validation failed: non clamping range must have min and max values defined."
                );
            }
            Ok(())
        };
        check().map_err(|e| e.prefixed("RangeTransform validation failed: "))
    }
}

impl BuildOps for RangeTransform {
    fn build_ops(
        &self,
        ops: &mut OpVec,
        _config: &Config,
        _context: &Context,
        dir: TransformDirection,
    ) -> Result<()> {
        self.validate()?;
        let data = RangeOpData::from_transform(self);
        match self.style {
            RangeStyle::Clamp => create_range_op_from_data(ops, &data, dir),
            RangeStyle::NoClamp => {
                let m = data.convert_to_matrix()?;
                create_matrix_op_from_data(ops, &m, dir)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::matrix::test_utils::*;
    use crate::ops::matrix::MatrixOp;
    use crate::processor::optimize_ops;

    const E: f64 = RangeOpData::EMPTY;
    const G_ERROR: f64 = 1e-7;

    /// Port of OCIO's `FloatsDiffer` (ULP comparison, no denorm compression).
    fn ulp_differ(expected: f32, actual: f32, tolerance: u32) -> bool {
        fn for_compare(bits: u32) -> u32 {
            if bits < 0x8000_0000 {
                0x8000_0000u32.wrapping_add(bits)
            } else {
                0x8000_0000u32.wrapping_sub(bits & 0x7FFF_FFFF)
            }
        }
        let e = for_compare(expected.to_bits());
        let a = for_compare(actual.to_bits());
        let diff = e.abs_diff(a);
        diff > tolerance
    }

    fn render(r: &RangeOpData, image: &[f32]) -> Vec<f32> {
        let rend = RangeRenderer::new(r).unwrap();
        let mut px = to_pixels(image);
        rend.apply(&mut px);
        flatten(&px)
    }

    // RangeOpData_tests.cpp

    #[test]
    fn data_accessors() {
        {
            let mut r = RangeOpData::default();
            assert!(
                r.min_in.is_nan() && r.max_in.is_nan() && r.min_out.is_nan() && r.max_out.is_nan()
            );
            assert!(!r.is_no_op());
            assert!(r.is_identity());
            let e = r.validate().unwrap_err();
            assert!(e.message().contains("At least minimum or maximum limits"));
            r.min_in = 1.0;
            r.max_in = 10.0;
            r.min_out = 2.0;
            r.max_out = 20.0;
            assert!(r.validate().is_ok());
        }
        {
            let mut range = RangeOpData::new(0.0, 1.0, 0.5, 1.5).unwrap();
            range.min_in = -0.05432;
            assert!(range.validate().is_ok());
            range.max_in = 1.05432;
            assert!(range.validate().is_ok());
            range.min_out = 0.05432;
            assert!(range.validate().is_ok());
            range.max_out = 2.05432;
            assert!(range.validate().is_ok());
            assert_close(range.scale(), 1.804012123, G_ERROR);
            assert_close(range.offset(), 0.1523139385, G_ERROR);
        }
        {
            let mut range = RangeOpData::new(0.0, 1.0, 0.5, 1.5).unwrap();
            assert_eq!(range.direction, TransformDirection::Forward);
            range.file_input_bit_depth = BitDepth::UInt8;
            range.file_output_bit_depth = BitDepth::F32;
            range.direction = TransformDirection::Inverse;
            assert_eq!(range.min_in, 0.0);
            assert_eq!(range.max_out, 1.5);
            let r = range.get_as_forward().unwrap();
            assert_eq!(r.direction, TransformDirection::Forward);
            assert_eq!(r.file_input_bit_depth, BitDepth::F32);
            assert_eq!(r.file_output_bit_depth, BitDepth::UInt8);
            assert_eq!(r.min_in, 0.5);
            assert_eq!(r.max_in, 1.5);
            assert_eq!(r.min_out, 0.0);
            assert_eq!(r.max_out, 1.0);
        }
    }

    #[test]
    fn data_range_identity() {
        let cases = [
            ((0., 1., 0., 1.), true, true, false),
            ((0.1, 1.2, -0.5, 2.), false, false, false),
            ((-0.1, 1.0, -0.5, 2.), false, false, false),
            ((0., 1., 0.01, 1.), true, false, false),
            ((0.1, 1., -0.01, 1.), true, false, false),
            ((-0.1, 1.1, -0.1, 1.1), false, true, false),
            ((0., E, 0., E), false, true, true),
            ((E, 1., E, 1.), false, true, false),
        ];
        for ((a, b, c, d), lut_domain, identity, clamp_negs) in cases {
            let r = RangeOpData::new(a, b, c, d).unwrap();
            assert_eq!(r.clamps_to_lut_domain(), lut_domain);
            assert_eq!(r.is_identity(), identity);
            assert_eq!(r.is_clamp_negs(), clamp_negs);
        }
    }

    #[test]
    fn data_identity() {
        let r4 = RangeOpData::new(0., E, 0., E).unwrap();
        assert!(r4.is_identity());
        assert!(!r4.is_no_op());
        assert!(!r4.has_channel_crosstalk());
        assert!(!r4.scales());
        assert!(!r4.min_is_empty());
        assert!(r4.max_is_empty());

        let r5 = RangeOpData::new(0., 1., 0., 1.).unwrap();
        assert!(!r5.scales());
        assert!(r5.is_identity());
        assert!(!r5.min_is_empty());
        assert!(!r5.max_is_empty());

        let r6 = RangeOpData::new(0., 1., -1., 1.).unwrap();
        assert!(!r6.is_identity());
        assert!(!r6.is_no_op());
        assert_eq!(r6.min_out, -1.);
        assert_eq!(r6.max_out, 1.);
        assert!(r6.scales());
    }

    #[test]
    fn data_equality() {
        let r1 = RangeOpData::new(0., 1., -1., 1.).unwrap();
        assert!(r1 != RangeOpData::new(0.123, 1., -1., 1.).unwrap());
        assert!(r1 != RangeOpData::new(0., 0.99, -1., 1.).unwrap());
        assert!(r1 != RangeOpData::new(0., 1., -12., 1.).unwrap());
        assert!(r1 == RangeOpData::new(0., 1., -1., 1.).unwrap());
    }

    #[test]
    fn data_validation() {
        let mut r = RangeOpData::default();
        r.min_in = 16.;
        r.max_in = 235.;
        r.max_out = 2.;
        assert!(r
            .validate()
            .unwrap_err()
            .message()
            .contains("must be both set or both missing"));

        let mut r = RangeOpData::default();
        r.min_in = 0.0;
        r.min_out = 0.00001;
        assert!(r
            .validate()
            .unwrap_err()
            .message()
            .contains("In and out minimum limits must be equal"));

        let mut r = RangeOpData::new(0.0, 1.0, 0.5, 1.5).unwrap();
        r.min_in = E;
        assert!(r
            .validate()
            .unwrap_err()
            .message()
            .contains("In and out minimum limits must be both set or both missing"));

        let mut r = RangeOpData::new(0.0, 1.0, 0.5, 1.5).unwrap();
        r.min_in = E;
        r.min_out = E;
        assert!(r
            .validate()
            .unwrap_err()
            .message()
            .contains("In and out maximum limits must be equal"));
        r.max_in = r.max_out;
        assert!(r.validate().is_ok());

        let mut r = RangeOpData::new(0.0, 1.0, 0.5, 1.5).unwrap();
        r.max_in = E;
        assert!(r
            .validate()
            .unwrap_err()
            .message()
            .contains("In and out maximum limits must be both set or both missing"));

        let mut r = RangeOpData::new(0.0, 1.0, 0.5, 1.5).unwrap();
        r.max_in = E;
        r.max_out = E;
        assert!(r
            .validate()
            .unwrap_err()
            .message()
            .contains("In and out minimum limits must be equal"));
        r.min_in = r.min_out;
        assert!(r.validate().is_ok());

        let mut r = RangeOpData::new(0.0, 1.0, 0.5, 1.5).unwrap();
        r.max_in = -125.;
        assert!(r
            .validate()
            .unwrap_err()
            .message()
            .contains("Range maximum input value is less than minimum input value"));

        let mut r = RangeOpData::new(0.0, 1.0, 0.5, 1.5).unwrap();
        r.max_out = -125.;
        assert!(r
            .validate()
            .unwrap_err()
            .message()
            .contains("Range maximum output value is less than minimum output value"));
    }

    fn check_inverse(fwd: (f64, f64, f64, f64), rev: (f64, f64, f64, f64)) {
        let r =
            RangeOpData::with_direction(fwd.0, fwd.1, fwd.2, fwd.3, TransformDirection::Inverse)
                .unwrap();
        let inv = r.get_as_forward().unwrap();
        let check = |a: f64, b: f64| {
            if b.is_nan() {
                assert!(a.is_nan());
            } else {
                assert_eq!(a, b);
            }
        };
        check(inv.min_in, rev.0);
        check(inv.max_in, rev.1);
        check(inv.min_out, rev.2);
        check(inv.max_out, rev.3);

        let fwd_scale = r.scale() as f32;
        let fwd_offset = r.offset() as f32;
        let rev_scale = inv.scale() as f32;
        let rev_offset = inv.offset() as f32;
        assert!(!ulp_differ(1.0, fwd_scale * rev_scale, 10));
        assert!(!ulp_differ(fwd_offset * rev_scale, -rev_offset, 500));
    }

    #[test]
    fn data_inverse() {
        check_inverse((0.064, 0.940, 0.032, 0.235), (0.032, 0.235, 0.064, 0.940));
        check_inverse((E, 0.235, E, 0.235), (E, 0.235, E, 0.235));
        check_inverse((0.64, E, 0.64, E), (0.64, E, 0.64, E));
    }

    #[test]
    fn data_compose() {
        let r1 = RangeOpData::new(0., 1., 0., 1.).unwrap();
        let r2 = RangeOpData::new(0.1, 0.9, 0.1, 0.9).unwrap();
        let res = r1.compose(&r2).unwrap();
        assert_eq!(
            (res.min_in, res.max_in, res.min_out, res.max_out),
            (0.1, 0.9, 0.1, 0.9)
        );

        let r3 = RangeOpData::new(0.1, 1.9, 0.1, 1.9).unwrap();
        let res = r1.compose(&r3).unwrap();
        assert_eq!(
            (res.min_in, res.max_in, res.min_out, res.max_out),
            (0.1, 1.0, 0.1, 1.0)
        );

        let r4 = RangeOpData::new(0.1, 1.9, 0.2, 1.8).unwrap();
        let res = r1.compose(&r4).unwrap();
        assert_eq!((res.min_in, res.max_in, res.min_out), (0.1, 1.0, 0.2));
        assert_close(res.max_out, 1.0, 1e-15);

        let r6 = RangeOpData::new(-1.0, 1.0, 0., 1.2).unwrap();
        let res = r1.compose(&r6).unwrap();
        assert_eq!(
            (res.min_in, res.max_in, res.min_out, res.max_out),
            (0., 1., 0.6, 1.2)
        );

        let r7 = RangeOpData::new(E, 0.5, E, 0.5).unwrap();
        let res = r7.compose(&r4).unwrap();
        assert_eq!((res.min_in, res.max_in, res.min_out), (0.1, 0.5, 0.2));
        assert_close(
            res.max_out,
            (0.5 * 1.6 + 0.2 * 1.8 - 0.1 * 1.6) / 1.8,
            1e-15,
        );

        let res = r4.compose(&r7).unwrap();
        assert_eq!(res.min_in, 0.1);
        assert_close(res.max_in, 0.4375, 1e-15);
        assert_eq!((res.min_out, res.max_out), (0.2, 0.5));

        let r8 = RangeOpData::new(0.5, E, 0.5, E).unwrap();
        let res = r8.compose(&r3).unwrap();
        assert_eq!(
            (res.min_in, res.max_in, res.min_out, res.max_out),
            (0.5, 1.9, 0.5, 1.9)
        );

        let res = r4.compose(&r8).unwrap();
        assert_close(res.min_in, 0.4375, 1e-15);
        assert_eq!((res.max_in, res.min_out, res.max_out), (1.9, 0.5, 1.8));

        let r9 = RangeOpData::new(1.1, 1.9, 1.2, 1.5).unwrap();
        let res = r1.compose(&r9).unwrap();
        assert_eq!(
            (res.min_in, res.max_in, res.min_out, res.max_out),
            (0.0, 1.0, 1.2, 1.2)
        );

        let r10 = RangeOpData::new(-1.1, -0.1, 1.1, 1.9).unwrap();
        let res = r1.compose(&r10).unwrap();
        assert_eq!(
            (res.min_in, res.max_in, res.min_out, res.max_out),
            (0., 1., 1.9, 1.9)
        );
    }

    #[test]
    fn data_computed_identifier() {
        let mut range = RangeOpData::new(0.0, 1.0, 0.5, 1.5).unwrap();
        let id1 = range.cache_id();
        range.max_in = E;
        range.max_out = E;
        range.min_out = range.min_in;
        let id2 = range.cache_id();
        assert_ne!(id1, id2);
        assert_eq!(range.cache_id(), id2);
        assert_eq!(id1, "forward [0, 1, 0.5, 1.5]");
        assert_eq!(id2, "forward [0, nan, 0, nan]");
    }

    // RangeOpCPU_tests.cpp

    #[test]
    fn cpu_identity() {
        let r = RangeOpData::new(0., E, 0., E).unwrap();
        assert!(r.is_identity());
        assert!(!r.is_no_op());
        assert!(matches!(
            RangeRenderer::new(&r).unwrap(),
            RangeRenderer::Min { .. }
        ));
    }

    #[test]
    fn cpu_scale_with_low_and_high_clippings() {
        let r = RangeOpData::new(0., 1., 0.5, 1.5).unwrap();
        assert!(matches!(
            RangeRenderer::new(&r).unwrap(),
            RangeRenderer::ScaleMinMax { .. }
        ));
        let qnan = f32::NAN;
        let inf = f32::INFINITY;
        let image = [
            -0.50f32, -0.25, 0.50, 0.0, 0.75, 1.00, 1.25, 1.0, 1.25, 1.50, 1.75, 0.0, qnan, qnan,
            qnan, 0.0, 0.0, 0.0, 0.0, qnan, inf, inf, inf, 0.0, 0.0, 0.0, 0.0, inf, -inf, -inf,
            -inf, 0.0, 0.0, 0.0, 0.0, -inf,
        ];
        let out = render(&r, &image);
        let expected = [
            0.5f32, 0.5, 1.0, 0.0, 1.25, 1.5, 1.5, 1.0, 1.5, 1.5, 1.5, 0.0,
        ];
        for i in 0..12 {
            assert_close(out[i] as f64, expected[i] as f64, G_ERROR);
        }
        assert_eq!(&out[12..16], &[0.5, 0.5, 0.5, 0.0]);
        assert_eq!(&out[16..19], &[0.5, 0.5, 0.5]);
        assert!(out[19].is_nan());
        assert_eq!(&out[20..24], &[1.5, 1.5, 1.5, 0.0]);
        assert_eq!(&out[24..28], &[0.5, 0.5, 0.5, inf]);
        assert_eq!(&out[28..32], &[0.5, 0.5, 0.5, 0.0]);
        assert_eq!(&out[32..36], &[0.5, 0.5, 0.5, -inf]);
    }

    const IMG3: [f32; 12] = [
        -0.50, -0.25, 0.50, 0.0, 0.75, 1.00, 1.25, 1.0, 1.25, 1.50, 1.75, 0.0,
    ];

    fn check_render(r: &RangeOpData, image: &[f32], expected: &[f32]) {
        let out = render(r, image);
        for i in 0..expected.len() {
            assert_close(out[i] as f64, expected[i] as f64, G_ERROR);
        }
    }

    #[test]
    fn cpu_scale_with_low_and_high_clippings_2() {
        let r = RangeOpData::new(0., 1., 0., 1.5).unwrap();
        assert!(matches!(
            RangeRenderer::new(&r).unwrap(),
            RangeRenderer::ScaleMinMax { .. }
        ));
        check_render(
            &r,
            &IMG3,
            &[0., 0., 0.75, 0., 1.125, 1.5, 1.5, 1., 1.5, 1.5, 1.5, 0.],
        );
    }

    #[test]
    fn cpu_offset_with_low_and_high_clippings() {
        let r = RangeOpData::new(0., 1., 1., 2.).unwrap();
        assert!(matches!(
            RangeRenderer::new(&r).unwrap(),
            RangeRenderer::ScaleMinMax { .. }
        ));
        check_render(
            &r,
            &IMG3,
            &[1., 1., 1.5, 0., 1.75, 2., 2., 1., 2., 2., 2., 0.],
        );
    }

    #[test]
    fn cpu_low_and_high_clippings() {
        let r = RangeOpData::new(1., 2., 1., 2.).unwrap();
        assert!(matches!(
            RangeRenderer::new(&r).unwrap(),
            RangeRenderer::MinMax { .. }
        ));
        let mut image = IMG3.to_vec();
        image.extend_from_slice(&[2.00, 2.50, 2.75, 1.0]);
        check_render(
            &r,
            &image,
            &[
                1., 1., 1., 0., 1., 1., 1.25, 1., 1.25, 1.5, 1.75, 0., 2., 2., 2., 1.,
            ],
        );
    }

    #[test]
    fn cpu_low_clipping() {
        let r = RangeOpData::new(-0.1, E, -0.1, E).unwrap();
        assert!(matches!(
            RangeRenderer::new(&r).unwrap(),
            RangeRenderer::Min { .. }
        ));
        check_render(
            &r,
            &IMG3,
            &[-0.1, -0.1, 0.5, 0., 0.75, 1., 1.25, 1., 1.25, 1.5, 1.75, 0.],
        );
    }

    #[test]
    fn cpu_high_clipping() {
        let r = RangeOpData::new(E, 1.1, E, 1.1).unwrap();
        assert!(matches!(
            RangeRenderer::new(&r).unwrap(),
            RangeRenderer::Max { .. }
        ));
        check_render(
            &r,
            &IMG3,
            &[-0.5, -0.25, 0.5, 0., 0.75, 1., 1.1, 1., 1.1, 1.1, 1.1, 0.],
        );
    }

    #[test]
    fn cpu_inverse() {
        let mut r = RangeOpData::new(0., 1.5, 0., 1.).unwrap();
        r.direction = TransformDirection::Inverse;
        assert!(r.validate().is_ok());
        assert!(RangeRenderer::new(&r)
            .unwrap_err()
            .message()
            .contains("Op::finalize has to be called"));
        let f = r.get_as_forward().unwrap();
        assert!(matches!(
            RangeRenderer::new(&f).unwrap(),
            RangeRenderer::ScaleMinMax { .. }
        ));
        check_render(
            &f,
            &IMG3,
            &[0., 0., 0.75, 0., 1.125, 1.5, 1.5, 1., 1.5, 1.5, 1.5, 0.],
        );
    }

    // RangeOp_tests.cpp

    #[test]
    fn op_apply_arbitrary() {
        let r = RangeOp::new(RangeOpData::new(-0.101, 0.95, 0.194, 1.001).unwrap()).unwrap();
        let image = [
            -0.50f32, 0.25, 0.50, 0.0, 0.75, 1.00, 1.25, 1.0, 1.25, 1.50, 1.75, 0.0,
        ];
        let out = apply_op(&r, &image);
        let expected = [
            0.194f32,
            0.4635119438,
            0.6554719806,
            0.0,
            0.8474320173,
            1.001,
            1.001,
            1.0,
            1.001,
            1.001,
            1.001,
            0.0,
        ];
        for i in 0..12 {
            assert_close(out[i] as f64, expected[i] as f64, G_ERROR);
        }
    }

    #[test]
    fn op_combining() {
        let mut ops = OpVec::new();
        create_range_op(&mut ops, 0., 0.5, 0.5, 1.0, TransformDirection::Forward).unwrap();
        create_range_op(&mut ops, 0., 1., 0.5, 1.5, TransformDirection::Forward).unwrap();
        let c = ops[0]
            .combine_with(ops[1].as_ref(), OptimizationFlags::DEFAULT)
            .unwrap();
        assert_eq!(c.len(), 1);
        // Range [0, 0.5] -> [0.5, 1] followed by [0, 1] -> [0.5, 1.5].
        let r = c[0].downcast_ref::<RangeOp>().unwrap().data();
        assert_eq!((r.min_in, r.max_in), (0.0, 0.5));
        assert_eq!((r.min_out, r.max_out), (1.0, 1.5));
        let src = [-1.0f32, 0.25, 0.75, 0.3];
        assert_eq!(apply_ops(&ops, &src), apply_ops(&c, &src));
        assert!(ops[0]
            .combine_with(ops[1].as_ref(), OptimizationFlags::NONE)
            .is_none());
    }

    #[test]
    fn op_combining_with_inverse() {
        let mut ops = OpVec::new();
        create_range_op(&mut ops, 0., 1., 0.5, 1.5, TransformDirection::Forward).unwrap();
        create_range_op(&mut ops, 0., 1., 0.5, 1.5, TransformDirection::Inverse).unwrap();
        let c = ops[0]
            .combine_with(ops[1].as_ref(), OptimizationFlags::DEFAULT)
            .unwrap();
        assert_eq!(c.len(), 1);
        // A range followed by its inverse is a clamp to [0, 1].
        let r = c[0].downcast_ref::<RangeOp>().unwrap().data();
        assert_eq!(
            (r.min_in, r.max_in, r.min_out, r.max_out),
            (0.0, 1.0, 0.0, 1.0)
        );
        // The optimizer keeps the clamping range.
        let opt = optimize_ops(&ops, OptimizationFlags::DEFAULT);
        assert_eq!(opt.len(), 1);
        assert_eq!(opt[0].name(), "Range");
    }

    #[test]
    fn op_computed_identifier() {
        let mut ops = OpVec::new();
        create_range_op(&mut ops, 0., 0.5, 0.5, 1.0, TransformDirection::Forward).unwrap();
        create_range_op(&mut ops, 0., 0.5, 0.5, 1.0, TransformDirection::Forward).unwrap();
        create_range_op(&mut ops, 0.1, 1., 0.3, 1.9, TransformDirection::Forward).unwrap();
        create_range_op(&mut ops, 0.1, 1., 0.3, 1.9, TransformDirection::Inverse).unwrap();
        let ids: Vec<String> = ops.iter().map(|o| o.cache_id()).collect();
        assert_eq!(ids[0], ids[1]);
        assert_ne!(ids[0], ids[2]);
        assert_ne!(ids[1], ids[2]);
        assert_ne!(ids[2], ids[3]);
        create_range_op(&mut ops, 0.1, 1., 0.3, 1.90001, TransformDirection::Forward).unwrap();
        let id4 = ops[4].cache_id();
        assert_ne!(ids[2], id4);
        assert_ne!(ids[3], id4);
    }

    #[test]
    fn op_create_transform() {
        let mut range =
            RangeOpData::with_direction(0.1, 0.9, 0.2, 0.7, TransformDirection::Inverse).unwrap();
        range.metadata.add_attribute("name", "test");
        range.file_input_bit_depth = BitDepth::UInt10;
        range.file_output_bit_depth = BitDepth::UInt8;
        let mut ops = OpVec::new();
        create_range_op_from_data(&mut ops, &range, TransformDirection::Forward).unwrap();
        let t = match ops[0].to_transform().unwrap() {
            Transform::Range(t) => t,
            _ => panic!("expected a range transform"),
        };
        // The op is stored forward: bounds and bit depths are swapped.
        assert_eq!(t.file_input_bit_depth, BitDepth::UInt8);
        assert_eq!(t.file_output_bit_depth, BitDepth::UInt10);
        assert_eq!(
            t.metadata.attributes,
            vec![("name".to_string(), "test".to_string())]
        );
        assert_eq!(t.direction, TransformDirection::Forward);
        assert_eq!(t.style, RangeStyle::Clamp);
        assert_eq!(
            (t.min_in, t.max_in, t.min_out, t.max_out),
            (Some(0.2), Some(0.7), Some(0.1), Some(0.9))
        );
        // Equivalent to the original inverse transform.
        let mut orig = RangeTransform::new(Some(0.1), Some(0.9), Some(0.2), Some(0.7));
        orig.direction = TransformDirection::Inverse;
        let config = Config::create_raw();
        let ctx = Context::new();
        let mut a = OpVec::new();
        orig.build_ops(&mut a, &config, &ctx, TransformDirection::Forward)
            .unwrap();
        let mut b = OpVec::new();
        t.build_ops(&mut b, &config, &ctx, TransformDirection::Forward)
            .unwrap();
        let src = [0.0f32, 0.35, 0.8, 0.5];
        assert_eq!(apply_ops(&a, &src), apply_ops(&b, &src));
    }

    #[test]
    fn transform_no_clamp_converts_to_matrix() {
        let config = Config::create_raw();
        let ctx = Context::new();
        let mut ops = OpVec::new();
        let mut range = RangeTransform::default();
        assert_eq!(range.direction, TransformDirection::Forward);
        range.max_in = Some(1.);
        range.max_out = Some(1.);
        assert_eq!(range.style, RangeStyle::Clamp);
        range
            .build_ops(&mut ops, &config, &ctx, TransformDirection::Forward)
            .unwrap();
        assert_eq!(ops.len(), 1);
        assert!(ops[0].downcast_ref::<RangeOp>().is_some());
        assert!(!ops[0].is_no_op());
        ops.clear();

        range.min_in = Some(0.0);
        range.max_in = Some(0.5);
        range.min_out = Some(0.5);
        range.max_out = Some(1.5);
        range
            .build_ops(&mut ops, &config, &ctx, TransformDirection::Forward)
            .unwrap();
        let rd = ops[0].downcast_ref::<RangeOp>().unwrap().data().clone();
        assert_eq!(rd.min_in, 0.0);
        assert_eq!(rd.max_in, 0.5);
        assert_eq!(rd.min_out, 0.5);
        assert_eq!(rd.max_out, 1.5);

        range.style = RangeStyle::NoClamp;
        range
            .build_ops(&mut ops, &config, &ctx, TransformDirection::Forward)
            .unwrap();
        assert_eq!(ops.len(), 2);
        let m = ops[1].downcast_ref::<MatrixOp>().unwrap().data();
        assert_eq!(m.offsets[0], rd.offset());
        assert_eq!(m.direction, TransformDirection::Forward);
        assert_eq!(m.offsets, [0.5, 0.5, 0.5, 0.0]);
        assert!(m.is_diagonal());
        assert_eq!(m.matrix[0], rd.scale());
        assert_eq!(
            (m.matrix[0], m.matrix[5], m.matrix[10], m.matrix[15]),
            (2.0, 2.0, 2.0, 1.0)
        );

        // Range is forward, build an inverse (stored as the inverted matrix).
        range
            .build_ops(&mut ops, &config, &ctx, TransformDirection::Inverse)
            .unwrap();
        let m = ops[2].downcast_ref::<MatrixOp>().unwrap().data();
        assert_eq!(m.offsets, [-0.25, -0.25, -0.25, 0.0]);
        assert!(m.is_diagonal());
        assert_eq!(
            (m.matrix[0], m.matrix[5], m.matrix[10], m.matrix[15]),
            (0.5, 0.5, 0.5, 1.0)
        );

        // Range is inverse, build a forward.
        range.direction = TransformDirection::Inverse;
        range
            .build_ops(&mut ops, &config, &ctx, TransformDirection::Forward)
            .unwrap();
        let m = ops[3].downcast_ref::<MatrixOp>().unwrap().data();
        assert_eq!(m.direction, TransformDirection::Forward);
        assert_eq!(m.offsets, [-0.25, -0.25, -0.25, 0.0]);
        assert_eq!(
            (m.matrix[0], m.matrix[5], m.matrix[10], m.matrix[15]),
            (0.5, 0.5, 0.5, 1.0)
        );

        // Non-clamping range needs both bounds.
        let mut r = RangeTransform::new(Some(0.0), None, Some(0.0), None);
        r.style = RangeStyle::NoClamp;
        let e = r.validate().unwrap_err();
        assert!(e
            .message()
            .contains("non clamping range must have min and max values defined"));
    }

    // RangeTransform_tests.cpp

    #[test]
    fn transform_basic() {
        let mut range = RangeTransform::default();
        assert_eq!(range.direction, TransformDirection::Forward);
        assert_eq!(range.style, RangeStyle::Clamp);
        assert!(range.min_in.is_none() && range.max_in.is_none());
        assert!(range.min_out.is_none() && range.max_out.is_none());
        range.direction = TransformDirection::Inverse;
        range.style = RangeStyle::NoClamp;
        range.min_in = Some(-0.5);

        let mut range2 = RangeTransform::default();
        range2.direction = TransformDirection::Inverse;
        range2.min_in = Some(-0.5);
        range2.style = RangeStyle::NoClamp;
        assert!(range2.equals(&range));

        range2.style = RangeStyle::Clamp;
        assert!(!range2.equals(&range));

        // Validation.
        let e = RangeTransform::default().validate().unwrap_err();
        assert_eq!(
            e.message(),
            "RangeTransform validation failed: At least minimum or maximum limits must be set in Range."
        );
        assert!(
            RangeTransform::new(Some(-1.5), Some(-0.5), Some(1.5), Some(4.5))
                .validate()
                .is_ok()
        );
    }

    #[test]
    fn optimizer_identity_range_is_kept() {
        let mut ops = OpVec::new();
        create_range_op(&mut ops, 0., 1., 0., 1., TransformDirection::Forward).unwrap();
        // A range identity still clamps: it is not removed.
        assert_eq!(optimize_ops(&ops, OptimizationFlags::ALL).len(), 1);
    }
}
