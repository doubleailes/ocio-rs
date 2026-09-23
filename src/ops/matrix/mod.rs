//! Matrix op: 4x4 matrix plus offset (port of `MatrixOpData`, `MatrixOp`,
//! `MatrixOpCPU` and the op building part of `MatrixTransform.cpp`).
//!
//! The parameters are kept in double precision, the CPU evaluation is done in
//! single precision, as in OCIO. Ops are always stored in the forward
//! direction: an inverse matrix is inverted when the op is created (OCIO does
//! it in `MatrixOffsetOp::finalize`).

pub mod float_format;

use crate::config::Config;
use crate::context::Context;
use crate::error::{Error, Result};
use crate::format_metadata::FormatMetadata;
use crate::math_utils::{equal_with_abs_error, is_scalar_equal_to_zero};
use crate::ops::{hash_f64, Op, OpRc, OpVec, Pixel};
use crate::transforms::{BuildOps, MatrixTransform, Transform, Validate, IDENTITY_MATRIX44};
use crate::types::{BitDepth, OptimizationFlags, TransformDirection};
use std::any::Any;
use std::sync::Arc;

/// The matrix parameters of a [`MatrixOp`] (port of OCIO's `MatrixOpData`).
///
/// The matrix is row major: `out = matrix * in + offsets`.
#[derive(Debug, Clone)]
pub struct MatrixOpData {
    /// Row-major 4x4 matrix.
    pub matrix: [f64; 16],
    /// RGBA offsets.
    pub offsets: [f64; 4],
    /// Direction of the matrix.
    pub direction: TransformDirection,
    /// Bit depth of the file the matrix was read from (informational).
    pub file_input_bit_depth: BitDepth,
    /// Bit depth of the file the matrix was read from (informational).
    pub file_output_bit_depth: BitDepth,
    /// Metadata (id, name, descriptions).
    pub metadata: FormatMetadata,
}

impl Default for MatrixOpData {
    fn default() -> Self {
        Self::new()
    }
}

impl PartialEq for MatrixOpData {
    /// Equality as defined by OCIO: metadata and file bit depths are ignored.
    fn eq(&self, other: &Self) -> bool {
        self.direction == other.direction
            && self.offsets == other.offsets
            && self.matrix == other.matrix
    }
}

impl MatrixOpData {
    /// Identity matrix, no offset, forward.
    pub fn new() -> Self {
        Self {
            matrix: IDENTITY_MATRIX44,
            offsets: [0.0; 4],
            direction: TransformDirection::Forward,
            file_input_bit_depth: BitDepth::Unknown,
            file_output_bit_depth: BitDepth::Unknown,
            metadata: FormatMetadata::default(),
        }
    }

    /// Matrix and offsets in the given direction.
    pub fn from_values(m44: &[f64; 16], offset4: &[f64; 4], direction: TransformDirection) -> Self {
        Self {
            matrix: *m44,
            offsets: *offset4,
            direction,
            ..Self::new()
        }
    }

    /// Diagonal matrix with `value` on the diagonal (alpha included), no
    /// offset (port of `CreateDiagonalMatrix`).
    pub fn diagonal(value: f64) -> Self {
        let mut m = Self::new();
        m.matrix[0] = value;
        m.matrix[5] = value;
        m.matrix[10] = value;
        m.matrix[15] = value;
        m
    }

    /// Build from the parameters of a [`MatrixTransform`].
    pub fn from_transform(t: &MatrixTransform) -> Self {
        Self {
            matrix: t.matrix,
            offsets: t.offset,
            direction: t.direction,
            file_input_bit_depth: t.file_input_bit_depth,
            file_output_bit_depth: t.file_output_bit_depth,
            metadata: t.metadata.clone(),
        }
    }

    /// Set the RGB part from a row-major 3x3 matrix (alpha row / column are
    /// set to identity).
    pub fn set_rgb(&mut self, m33: &[f64; 9]) {
        let v = &mut self.matrix;
        v[0] = m33[0];
        v[1] = m33[1];
        v[2] = m33[2];
        v[3] = 0.0;
        v[4] = m33[3];
        v[5] = m33[4];
        v[6] = m33[5];
        v[7] = 0.0;
        v[8] = m33[6];
        v[9] = m33[7];
        v[10] = m33[8];
        v[11] = 0.0;
        v[12] = 0.0;
        v[13] = 0.0;
        v[14] = 0.0;
        v[15] = 1.0;
    }

    /// The `id` metadata attribute.
    pub fn id(&self) -> &str {
        self.metadata.id()
    }

    /// Validate the parameters: an inverse matrix must be invertible.
    pub fn validate(&self) -> Result<()> {
        if self.direction == TransformDirection::Inverse {
            self.get_as_forward()?;
        }
        Ok(())
    }

    /// True if the matrix is exactly the identity (strict comparison).
    pub fn is_unity_diagonal(&self) -> bool {
        (0..16).all(|i| self.matrix[i] == if i % 5 == 0 { 1.0 } else { 0.0 })
    }

    /// True if all the off-diagonal values are exactly 0.
    pub fn is_diagonal(&self) -> bool {
        (0..16).all(|i| i % 5 == 0 || self.matrix[i] == 0.0)
    }

    /// True if any offset is not 0.
    pub fn has_offsets(&self) -> bool {
        self.offsets.iter().any(|&o| o != 0.0)
    }

    /// True if the alpha channel is modified or used.
    pub fn has_alpha(&self) -> bool {
        let m = &self.matrix;
        m[3] != 0.0
            || m[7] != 0.0
            || m[11] != 0.0
            || !equal_with_abs_error(m[15], 1.0, 1e-6)
            || m[12] != 0.0
            || m[13] != 0.0
            || m[14] != 0.0
            || self.offsets[3] != 0.0
    }

    /// True if the matrix is an identity (diagonal values within 1e-6 of 1,
    /// no offsets, no alpha change).
    pub fn is_identity(&self) -> bool {
        if self.has_offsets() || self.has_alpha() || !self.is_diagonal() {
            return false;
        }
        (0..4).all(|i| equal_with_abs_error(self.matrix[5 * i], 1.0, 1e-6))
    }

    /// A matrix op is a no-op when it is an identity.
    pub fn is_no_op(&self) -> bool {
        self.is_identity()
    }

    /// True if an output channel depends on other input channels.
    pub fn has_channel_crosstalk(&self) -> bool {
        !self.is_diagonal()
    }

    /// Inverse of the 4x4 matrix (Gauss-Jordan elimination with partial
    /// pivoting, from Imath's `gjInverse`).
    pub fn inverse_matrix(&self) -> Result<[f64; 16]> {
        const DIM: usize = 4;
        let mut t = self.matrix;
        let mut s = IDENTITY_MATRIX44;

        // Forward elimination.
        for i in 0..3 {
            let mut pivot = i;
            let mut pivotsize = t[i * DIM + i].abs();
            for j in (i + 1)..4 {
                let tmp = t[j * DIM + i].abs();
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
                    t.swap(i * DIM + j, pivot * DIM + j);
                    s.swap(i * DIM + j, pivot * DIM + j);
                }
            }
            for j in (i + 1)..4 {
                let f = t[j * DIM + i] / t[i * DIM + i];
                for k in 0..4 {
                    t[j * DIM + k] -= f * t[i * DIM + k];
                    s[j * DIM + k] -= f * s[i * DIM + k];
                }
            }
        }

        // Backward substitution.
        for i in (0..4).rev() {
            let f = t[i * DIM + i];
            if f == 0.0 {
                return Err(Error::msg("Singular Matrix can't be inverted."));
            }
            for j in 0..4 {
                t[i * DIM + j] /= f;
                s[i * DIM + j] /= f;
            }
            for j in 0..i {
                let f = t[j * DIM + i];
                for k in 0..4 {
                    t[j * DIM + k] -= f * t[i * DIM + k];
                    s[j * DIM + k] -= f * s[i * DIM + k];
                }
            }
        }
        Ok(s)
    }

    /// The equivalent forward matrix (inverts the matrix and the offsets if
    /// the direction is inverse). Fails for singular matrices.
    pub fn get_as_forward(&self) -> Result<MatrixOpData> {
        if self.direction == TransformDirection::Forward {
            return Ok(self.clone());
        }
        let inv = self.inverse_matrix()?;
        let mut inv_offsets = [0.0; 4];
        if self.has_offsets() {
            inv_offsets = mult_vec(&inv, &self.offsets);
            inv_offsets.iter_mut().for_each(|v| *v *= -1.0);
        }
        Ok(MatrixOpData {
            matrix: inv,
            offsets: inv_offsets,
            direction: TransformDirection::Forward,
            file_input_bit_depth: self.file_output_bit_depth,
            file_output_bit_depth: self.file_input_bit_depth,
            metadata: self.metadata.clone(),
        })
    }

    /// Compose `self` followed by `b` (both must be forward): the result is
    /// `b * self`.
    pub fn compose(&self, b: &MatrixOpData) -> Result<MatrixOpData> {
        if self.direction == TransformDirection::Inverse
            || b.direction == TransformDirection::Inverse
        {
            return Err(Error::msg("Op::finalize has to be called."));
        }
        let mut metadata = self.metadata.clone();
        metadata.combine(&b.metadata);

        let matrix = mult_matrix(&b.matrix, &self.matrix);
        let mut offs = mult_vec(&b.matrix, &self.offsets);

        // Determine overall scaling of the offsets prior to any catastrophic
        // cancellation that may occur during the add.
        let mut max_val: f64 = 0.0;
        for i in 0..4 {
            let v = offs[i].abs();
            max_val = if max_val > v { max_val } else { v };
            let v = b.offsets[i].abs();
            max_val = if max_val > v { max_val } else { v };
        }
        for i in 0..4 {
            offs[i] += b.offsets[i];
        }

        let mut out = MatrixOpData {
            matrix,
            offsets: offs,
            direction: TransformDirection::Forward,
            file_input_bit_depth: self.file_input_bit_depth,
            file_output_bit_depth: b.file_output_bit_depth,
            metadata,
        };
        out.clean_up(max_val);
        Ok(out)
    }

    /// Replace values very close to integers by exact integers (used after
    /// compositions so that a matrix and its inverse give an identity).
    pub fn clean_up(&mut self, offset_scale: f64) {
        let mut max_val: f64 = 0.0;
        for v in &self.matrix {
            let a = v.abs();
            max_val = if max_val > a { max_val } else { a };
        }
        let scale = if max_val > 1e-4 { max_val } else { 1e-4 };
        let abs_tol = scale * 1e-7;
        for v in self.matrix.iter_mut() {
            let r = v.round();
            if (*v - r).abs() < abs_tol {
                *v = r;
            }
        }

        let scale2 = if offset_scale > 1e-4 {
            offset_scale
        } else {
            1e-4
        };
        let abs_tol2 = scale2 * 1e-7;
        for v in self.offsets.iter_mut() {
            let r = v.round();
            if (*v - r).abs() < abs_tol2 {
                *v = r;
            }
        }
    }

    /// Scale the matrix by `in_scale * out_scale` and the offsets by
    /// `out_scale`.
    pub fn scale(&mut self, in_scale: f64, out_scale: f64) {
        let combined = in_scale * out_scale;
        self.matrix.iter_mut().for_each(|v| *v *= combined);
        self.offsets.iter_mut().for_each(|v| *v *= out_scale);
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
        let mut values = self.matrix.to_vec();
        values.extend_from_slice(&self.offsets);
        s.push_str(&hash_f64(&values));
        s
    }
}

/// `a * b` for row-major 4x4 matrices.
fn mult_matrix(a: &[f64; 16], b: &[f64; 16]) -> [f64; 16] {
    let mut out = [0.0; 16];
    for row in 0..4 {
        for col in 0..4 {
            let mut accum = 0.0;
            for i in 0..4 {
                accum += a[row * 4 + i] * b[i * 4 + col];
            }
            out[row * 4 + col] = accum;
        }
    }
    out
}

/// `m * v`.
fn mult_vec(m: &[f64; 16], v: &[f64; 4]) -> [f64; 4] {
    let mut out = [0.0; 4];
    for i in 0..4 {
        let mut accum = 0.0;
        for j in 0..4 {
            accum += m[i * 4 + j] * v[j];
        }
        out[i] = accum;
    }
    out
}

// ---------------------------------------------------------------------------
// CPU renderers.

/// CPU renderer chosen from the matrix parameters (as `GetMatrixRenderer`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MatrixRenderer {
    /// Diagonal matrix, no offset.
    Scale { scale: [f32; 4] },
    /// Diagonal matrix with offsets.
    ScaleWithOffset { scale: [f32; 4], offset: [f32; 4] },
    /// Full matrix, no offset (columns of the matrix).
    Matrix { columns: [[f32; 4]; 4] },
    /// Full matrix with offsets.
    MatrixWithOffset {
        columns: [[f32; 4]; 4],
        offset: [f32; 4],
    },
}

impl MatrixRenderer {
    /// Select and initialize the renderer for a forward matrix.
    pub fn new(mat: &MatrixOpData) -> Result<Self> {
        if mat.direction == TransformDirection::Inverse {
            return Err(Error::msg("Op::finalize has to be called."));
        }
        let m = &mat.matrix;
        let o = &mat.offsets;
        let offset = [o[0] as f32, o[1] as f32, o[2] as f32, o[3] as f32];
        if mat.is_diagonal() {
            let scale = [m[0] as f32, m[5] as f32, m[10] as f32, m[15] as f32];
            if mat.has_offsets() {
                Ok(MatrixRenderer::ScaleWithOffset { scale, offset })
            } else {
                Ok(MatrixRenderer::Scale { scale })
            }
        } else {
            let mut columns = [[0.0f32; 4]; 4];
            for (c, col) in columns.iter_mut().enumerate() {
                for (r, v) in col.iter_mut().enumerate() {
                    *v = m[r * 4 + c] as f32;
                }
            }
            if mat.has_offsets() {
                Ok(MatrixRenderer::MatrixWithOffset { columns, offset })
            } else {
                Ok(MatrixRenderer::Matrix { columns })
            }
        }
    }

    /// Process pixels in place.
    pub fn apply(&self, pixels: &mut [Pixel]) {
        match self {
            MatrixRenderer::Scale { scale } => {
                for p in pixels.iter_mut() {
                    for c in 0..4 {
                        p[c] *= scale[c];
                    }
                }
            }
            MatrixRenderer::ScaleWithOffset { scale, offset } => {
                for p in pixels.iter_mut() {
                    for c in 0..4 {
                        p[c] = p[c] * scale[c] + offset[c];
                    }
                }
            }
            MatrixRenderer::Matrix { columns } => {
                for p in pixels.iter_mut() {
                    let (r, g, b, a) = (p[0], p[1], p[2], p[3]);
                    for c in 0..4 {
                        // Same evaluation order as the SSE path.
                        p[c] = (r * columns[0][c] + g * columns[1][c])
                            + (b * columns[2][c] + a * columns[3][c]);
                    }
                }
            }
            MatrixRenderer::MatrixWithOffset { columns, offset } => {
                for p in pixels.iter_mut() {
                    let (r, g, b, a) = (p[0], p[1], p[2], p[3]);
                    for c in 0..4 {
                        p[c] = ((r * columns[0][c] + g * columns[1][c])
                            + (b * columns[2][c] + a * columns[3][c]))
                            + offset[c];
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The op.

/// Matrix + offset op (port of OCIO's `MatrixOffsetOp`). The data is always
/// forward.
#[derive(Debug, Clone)]
pub struct MatrixOp {
    /// Forward parameters (used for evaluation and optimization).
    data: MatrixOpData,
    /// Parameters as given (possibly inverse), returned by `to_transform`.
    original: MatrixOpData,
    renderer: MatrixRenderer,
}

impl MatrixOp {
    /// Create the op; an inverse matrix is inverted (fails if singular).
    pub fn new(data: MatrixOpData) -> Result<Self> {
        let original = data.clone();
        let data = data.get_as_forward()?;
        let renderer = MatrixRenderer::new(&data)?;
        Ok(Self { data, original, renderer })
    }

    /// The parameters as given at creation (possibly in inverse direction).
    pub fn original_data(&self) -> &MatrixOpData {
        &self.original
    }

    /// The (forward) parameters.
    pub fn data(&self) -> &MatrixOpData {
        &self.data
    }

    /// The CPU renderer.
    pub fn renderer(&self) -> &MatrixRenderer {
        &self.renderer
    }
}

impl Op for MatrixOp {
    fn name(&self) -> &'static str {
        "Matrix"
    }

    fn apply(&self, pixels: &mut [Pixel]) {
        self.renderer.apply(pixels);
    }

    fn is_no_op(&self) -> bool {
        self.data.is_no_op()
    }

    fn has_channel_crosstalk(&self) -> bool {
        self.data.has_channel_crosstalk()
    }

    fn cache_id(&self) -> String {
        format!("<MatrixOffsetOp {} >", self.data.cache_id())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn combine_with(&self, next: &dyn Op, flags: OptimizationFlags) -> Option<OpVec> {
        if !flags.contains(OptimizationFlags::COMP_MATRIX) {
            return None;
        }
        let other = next.downcast_ref::<MatrixOp>()?;
        let composed = self.data.compose(&other.data).ok()?;
        let mut ops = OpVec::new();
        if !composed.is_no_op() {
            ops.push(Arc::new(MatrixOp::new(composed).ok()?));
        }
        Some(ops)
    }

    fn to_transform(&self) -> Option<Transform> {
        let d = &self.original;
        Some(Transform::Matrix(MatrixTransform {
            direction: d.direction,
            matrix: d.matrix,
            offset: d.offsets,
            file_input_bit_depth: d.file_input_bit_depth,
            file_output_bit_depth: d.file_output_bit_depth,
            metadata: d.metadata.clone(),
        }))
    }

    fn finalize(&self) -> Option<OpRc> {
        if self.original.direction == TransformDirection::Forward {
            return None;
        }
        let mut op = self.clone();
        op.original = op.data.clone();
        Some(Arc::new(op))
    }

    fn clone_box(&self) -> Box<dyn Op> {
        Box::new(self.clone())
    }
}

// ---------------------------------------------------------------------------
// Op builders (port of the `Create*Op` functions of `MatrixOp.cpp`).

/// Append a matrix op built from `data` in direction `dir` (combined with the
/// data direction).
pub fn create_matrix_op_from_data(
    ops: &mut OpVec,
    data: &MatrixOpData,
    dir: TransformDirection,
) -> Result<()> {
    let mut mat = data.clone();
    mat.direction = mat.direction.combine(dir);
    ops.push(Arc::new(MatrixOp::new(mat)?));
    Ok(())
}

/// Append a matrix + offset op.
pub fn create_matrix_offset_op(
    ops: &mut OpVec,
    m44: &[f64; 16],
    offset4: &[f64; 4],
    dir: TransformDirection,
) -> Result<()> {
    create_matrix_op_from_data(
        ops,
        &MatrixOpData::from_values(m44, offset4, TransformDirection::Forward),
        dir,
    )
}

/// Append a matrix op with an offset (same as [`create_matrix_offset_op`]).
pub fn create_matrix_op(
    ops: &mut OpVec,
    m44: &[f64; 16],
    offset4: &[f64; 4],
    dir: TransformDirection,
) -> Result<()> {
    create_matrix_offset_op(ops, m44, offset4, dir)
}

/// Append a matrix op without offset.
pub fn create_matrix_op_no_offset(
    ops: &mut OpVec,
    m44: &[f64; 16],
    dir: TransformDirection,
) -> Result<()> {
    create_matrix_offset_op(ops, m44, &[0.0; 4], dir)
}

/// Append a diagonal scale + offset op.
pub fn create_scale_offset_op(
    ops: &mut OpVec,
    scale4: &[f64; 4],
    offset4: &[f64; 4],
    dir: TransformDirection,
) -> Result<()> {
    let mut m44 = [0.0; 16];
    m44[0] = scale4[0];
    m44[5] = scale4[1];
    m44[10] = scale4[2];
    m44[15] = scale4[3];
    create_matrix_offset_op(ops, &m44, offset4, dir)
}

/// Append a diagonal scale op.
pub fn create_scale_op(ops: &mut OpVec, scale4: &[f64; 4], dir: TransformDirection) -> Result<()> {
    create_scale_offset_op(ops, scale4, &[0.0; 4], dir)
}

/// Append an offset op.
pub fn create_offset_op(
    ops: &mut OpVec,
    offset4: &[f64; 4],
    dir: TransformDirection,
) -> Result<()> {
    create_scale_offset_op(ops, &[1.0; 4], offset4, dir)
}

/// Append a saturation op (see [`MatrixTransform::sat`]).
pub fn create_saturation_op(
    ops: &mut OpVec,
    sat: f64,
    luma_coef3: &[f64; 3],
    dir: TransformDirection,
) -> Result<()> {
    let (m, o) = MatrixTransform::sat(sat, luma_coef3);
    create_matrix_offset_op(ops, &m, &o, dir)
}

/// Matrix and offsets mapping `[oldmin, oldmax]` to `[newmin, newmax]` per
/// channel (port of `MatrixTransform::Fit`).
pub fn fit_matrix(
    oldmin4: &[f64; 4],
    oldmax4: &[f64; 4],
    newmin4: &[f64; 4],
    newmax4: &[f64; 4],
) -> Result<([f64; 16], [f64; 4])> {
    let mut m44 = [0.0; 16];
    let mut offset4 = [0.0; 4];
    for i in 0..4 {
        let denom = oldmax4[i] - oldmin4[i];
        if is_scalar_equal_to_zero(denom) {
            crate::bail!(
                "Cannot create Fit operator. Max value equals min value '{}' in channel index {}.",
                float_format::format_g(oldmax4[i], 6),
                i
            );
        }
        m44[5 * i] = (newmax4[i] - newmin4[i]) / denom;
        offset4[i] = (newmin4[i] * oldmax4[i] - newmax4[i] * oldmin4[i]) / denom;
    }
    Ok((m44, offset4))
}

/// Append a fit op mapping `[oldmin, oldmax]` to `[newmin, newmax]`.
pub fn create_fit_op(
    ops: &mut OpVec,
    oldmin4: &[f64; 4],
    oldmax4: &[f64; 4],
    newmin4: &[f64; 4],
    newmax4: &[f64; 4],
    dir: TransformDirection,
) -> Result<()> {
    let (m, o) = fit_matrix(oldmin4, oldmax4, newmin4, newmax4)?;
    create_matrix_offset_op(ops, &m, &o, dir)
}

/// Append an identity matrix op.
pub fn create_identity_matrix_op(ops: &mut OpVec) -> Result<()> {
    create_matrix_op_from_data(
        ops,
        &MatrixOpData::diagonal(1.0),
        TransformDirection::Forward,
    )
}

/// Append an op mapping `[from_min, from_max]` to `[0, 1]` for RGB (nothing is
/// appended if the op would be an identity).
pub fn create_min_max_op(
    ops: &mut OpVec,
    from_min3: &[f64; 3],
    from_max3: &[f64; 3],
    dir: TransformDirection,
) -> Result<()> {
    let mut scale4 = [1.0; 4];
    let mut offset4 = [0.0; 4];
    let mut something_to_do = false;
    for i in 0..3 {
        let range = from_max3[i] - from_min3[i];
        if range == 0.0 {
            crate::bail!("CreateMinMaxOp: from_min and from_max must not be equal.");
        }
        scale4[i] = 1.0 / range;
        offset4[i] = -from_min3[i] * scale4[i];
        something_to_do |= scale4[i] != 1.0 || offset4[i] != 0.0;
    }
    if something_to_do {
        create_scale_offset_op(ops, &scale4, &offset4, dir)?;
    }
    Ok(())
}

/// Same as [`create_min_max_op`] with the same bounds for all channels.
pub fn create_min_max_op_scalar(
    ops: &mut OpVec,
    from_min: f32,
    from_max: f32,
    dir: TransformDirection,
) -> Result<()> {
    let min = [from_min as f64; 3];
    let max = [from_max as f64; 3];
    create_min_max_op(ops, &min, &max, dir)
}

// ---------------------------------------------------------------------------
// MatrixTransform.

impl MatrixTransform {
    /// Equality as defined by OCIO (`MatrixTransform::equals`): direction,
    /// matrix and offsets; metadata and file bit depths are ignored.
    pub fn equals(&self, other: &MatrixTransform) -> bool {
        MatrixOpData::from_transform(self) == MatrixOpData::from_transform(other)
    }
}

impl Validate for MatrixTransform {
    fn validate(&self) -> Result<()> {
        MatrixOpData::from_transform(self)
            .validate()
            .map_err(|e| e.prefixed("MatrixTransform validation failed: "))
    }
}

impl BuildOps for MatrixTransform {
    fn build_ops(
        &self,
        ops: &mut OpVec,
        _config: &Config,
        _context: &Context,
        dir: TransformDirection,
    ) -> Result<()> {
        self.validate()?;
        create_matrix_op_from_data(ops, &MatrixOpData::from_transform(self), dir)
    }
}

/// Downcast helper: the matrix data of an op, if it is a [`MatrixOp`].
pub fn as_matrix_op(op: &OpRc) -> Option<&MatrixOp> {
    op.downcast_ref::<MatrixOp>()
}

#[cfg(test)]
pub(crate) mod test_utils {
    //! Helpers shared by the unit tests of the basic ops.

    use crate::ops::{Op, OpRc, Pixel};

    /// Relative comparison with a minimum expected value (port of OCIO's
    /// `EqualWithSafeRelError`).
    pub fn equal_with_safe_rel_error(
        value: f32,
        expected: f32,
        eps: f32,
        min_expected: f32,
    ) -> bool {
        if value == expected {
            return true;
        }
        if value.is_nan() && expected.is_nan() {
            return true;
        }
        let div = if expected > 0.0 {
            if expected < min_expected {
                min_expected
            } else {
                expected
            }
        } else if -expected < min_expected {
            min_expected
        } else {
            -expected
        };
        let err = (if value > expected {
            value - expected
        } else {
            expected - value
        }) / div;
        err <= eps
    }

    /// `|a - b| < tol` (port of `OCIO_CHECK_CLOSE`).
    #[track_caller]
    pub fn assert_close(a: f64, b: f64, tol: f64) {
        assert!((a - b).abs() < tol, "{a} != {b} (tolerance {tol})");
    }

    /// `|a - b| < tol` computed in single precision (port of
    /// `OCIO_CHECK_CLOSE` on floats).
    #[track_caller]
    pub fn assert_close_f(a: f32, b: f64, tol: f64) {
        let d = (a - b as f32).abs();
        assert!((d as f64) < tol, "{a} != {b} (tolerance {tol})");
    }

    /// Convert a flat RGBA array into pixels.
    pub fn to_pixels(values: &[f32]) -> Vec<Pixel> {
        values.chunks(4).map(|c| [c[0], c[1], c[2], c[3]]).collect()
    }

    /// Flatten pixels.
    pub fn flatten(pixels: &[Pixel]) -> Vec<f32> {
        pixels.iter().flat_map(|p| p.iter().copied()).collect()
    }

    /// Apply one op to a flat RGBA array.
    pub fn apply_op(op: &dyn Op, values: &[f32]) -> Vec<f32> {
        let mut px = to_pixels(values);
        op.apply(&mut px);
        flatten(&px)
    }

    /// Apply ops to a flat RGBA array.
    pub fn apply_ops(ops: &[OpRc], values: &[f32]) -> Vec<f32> {
        let mut px = to_pixels(values);
        crate::ops::apply_ops(ops, &mut px);
        flatten(&px)
    }
}

#[cfg(test)]
mod tests {
    use super::test_utils::*;
    use super::*;
    use crate::ops::log::create_log_op;
    use crate::ops::noop::create_file_no_op;
    use crate::processor::optimize_ops;
    use crate::types::METADATA_NAME;

    fn mat_op(ops: &OpVec, i: usize) -> &MatrixOp {
        ops[i].downcast_ref::<MatrixOp>().expect("matrix op")
    }

    // MatrixOpCPU_tests.cpp

    #[test]
    fn cpu_scale_renderer() {
        let mat = MatrixOpData::diagonal(2.0);
        let r = MatrixRenderer::new(&mat).unwrap();
        assert!(matches!(r, MatrixRenderer::Scale { .. }));
        let mut px = [[4.0f32, 3.0, 2.0, 1.0]];
        r.apply(&mut px);
        assert_eq!(px[0], [8.0, 6.0, 4.0, 2.0]);
    }

    #[test]
    fn cpu_scale_with_offset_renderer() {
        let mut mat = MatrixOpData::diagonal(2.0);
        mat.offsets = [1.0, 2.0, 3.0, 4.0];
        let r = MatrixRenderer::new(&mat).unwrap();
        assert!(matches!(r, MatrixRenderer::ScaleWithOffset { .. }));
        let mut px = [[4.0f32, 3.0, 2.0, 1.0]];
        r.apply(&mut px);
        assert_eq!(px[0], [9.0, 8.0, 7.0, 6.0]);
    }

    #[test]
    fn cpu_matrix_with_offset_renderer() {
        let mut mat = MatrixOpData::diagonal(2.0);
        mat.offsets = [1.0, 2.0, 3.0, 4.0];
        mat.matrix[3] = 0.5;
        let r = MatrixRenderer::new(&mat).unwrap();
        assert!(matches!(r, MatrixRenderer::MatrixWithOffset { .. }));
        let mut px = [[4.0f32, 3.0, 2.0, 1.0]];
        r.apply(&mut px);
        assert_eq!(px[0], [9.5, 8.0, 7.0, 6.0]);
    }

    #[test]
    fn cpu_matrix_renderer() {
        let mut mat = MatrixOpData::diagonal(2.0);
        mat.matrix[3] = 0.5;
        let r = MatrixRenderer::new(&mat).unwrap();
        assert!(matches!(r, MatrixRenderer::Matrix { .. }));
        let mut px = [[4.0f32, 3.0, 2.0, 1.0]];
        r.apply(&mut px);
        assert_eq!(px[0], [8.5, 6.0, 4.0, 2.0]);
    }

    // MatrixOpData_tests.cpp

    #[test]
    fn data_empty() {
        let m = MatrixOpData::new();
        assert!(m.is_no_op());
        assert!(m.is_unity_diagonal());
        assert!(m.is_diagonal());
        assert!(m.validate().is_ok());
    }

    #[test]
    fn data_accessors() {
        let mut m = MatrixOpData::new();
        assert!(m.is_no_op());
        assert!(m.is_unity_diagonal());
        assert!(m.is_diagonal());
        assert!(m.is_identity());

        m.matrix[15] = (1.0f32 + 1e-5f32) as f64;
        assert!(!m.is_no_op());
        assert!(!m.is_unity_diagonal());
        assert!(m.is_diagonal());
        assert!(!m.is_identity());
        assert!(m.validate().is_ok());

        m.matrix[1] = 1e-5f32 as f64;
        m.matrix[15] = 1.0;
        assert!(!m.is_no_op());
        assert!(!m.is_unity_diagonal());
        assert!(!m.is_diagonal());
        assert!(!m.is_identity());

        assert_eq!(m.file_input_bit_depth, BitDepth::Unknown);
        assert_eq!(m.file_output_bit_depth, BitDepth::Unknown);
        m.file_input_bit_depth = BitDepth::UInt10;
        m.file_output_bit_depth = BitDepth::UInt8;
        let m1 = m.clone();
        assert_eq!(m1.file_input_bit_depth, BitDepth::UInt10);
        assert_eq!(m1.file_output_bit_depth, BitDepth::UInt8);
    }

    #[test]
    fn data_offsets() {
        let mut m = MatrixOpData::new();
        assert!(!m.has_offsets());
        m.offsets[2] = 1.0;
        assert!(!m.is_no_op());
        assert!(m.is_unity_diagonal());
        assert!(m.is_diagonal());
        assert!(m.has_offsets());
        assert_eq!(m.offsets[2], 1.0);

        let mut m = MatrixOpData::new();
        m.offsets[3] = -1e-6f32 as f64;
        assert!(!m.is_no_op());
        assert!(m.is_unity_diagonal());
        assert!(m.has_offsets());
        assert_eq!(m.offsets[3], -1e-6f32 as f64);
    }

    #[test]
    fn data_diagonal() {
        let m = MatrixOpData::diagonal(0.5);
        assert!(m.is_diagonal());
        assert!(!m.has_offsets());
        assert!(m.validate().is_ok());
        assert_eq!(m.matrix[0], 0.5);
        assert_eq!(m.matrix[5], 0.5);
        assert_eq!(m.matrix[10], 0.5);
        assert_eq!(m.matrix[15], 0.5);
    }

    #[test]
    fn data_has_alpha() {
        let mut mat = MatrixOpData::new();
        assert!(!mat.has_alpha());
        for (id, val) in [
            (3, 0.0),
            (7, 0.0),
            (11, 0.0),
            (12, 0.0),
            (13, 0.0),
            (14, 0.0),
            (15, 1.0),
        ] {
            mat.matrix[id] = val + 0.001;
            assert!(mat.has_alpha());
            mat.matrix[id] = val;
            assert!(!mat.has_alpha());
        }
        mat.offsets[3] = 0.001;
        assert!(mat.has_alpha());
        mat.offsets[3] = 0.0;
        assert!(!mat.has_alpha());
    }

    #[test]
    fn data_clone() {
        let mut r = MatrixOpData::new();
        r.offsets = [1.0, 2.0, 3.0, 4.0];
        r.matrix[0] = 2.0;
        let c = r.clone();
        assert!(!c.is_no_op());
        assert!(!c.is_unity_diagonal());
        assert!(c.is_diagonal());
        assert_eq!(c.offsets, [1.0, 2.0, 3.0, 4.0]);
        assert_eq!(c.matrix, r.matrix);
    }

    #[test]
    fn data_construct() {
        let m = MatrixOpData::new();
        assert_eq!(m.id(), "");
        assert!(m.metadata.children.is_empty());
        assert_eq!(m.offsets, [0.0; 4]);
        assert_eq!(m.matrix, IDENTITY_MATRIX44);
    }

    #[test]
    fn data_composition() {
        // Compose 2 forward matrices.
        {
            let mut a = MatrixOpData::from_values(
                &[
                    1., 2., 3., 4., 4., 5., 6., 7., 7., 8., 9., 10., 11., 12., 13., 14.,
                ],
                &[10., 11., 12., 13.],
                TransformDirection::Forward,
            );
            a.file_input_bit_depth = BitDepth::UInt8;
            a.file_output_bit_depth = BitDepth::F16;
            let mut b = MatrixOpData::from_values(
                &[
                    21., 22., 23., 24., 24., 25., 26., 27., 27., 28., 29., 30., 31., 32., 33., 34.,
                ],
                &[30., 31., 32., 33.],
                TransformDirection::Forward,
            );
            b.file_input_bit_depth = BitDepth::F16;
            b.file_output_bit_depth = BitDepth::UInt10;

            let aim = [
                534., 624., 714., 804., 603., 705., 807., 909., 672., 786., 900., 1014., 764.,
                894., 1024., 1154.,
            ];
            let aim_offs = [1040. + 30., 1178. + 31., 1316. + 32., 1500. + 33.];
            let r = a.compose(&b).unwrap();
            assert_eq!(r.file_input_bit_depth, BitDepth::UInt8);
            assert_eq!(r.file_output_bit_depth, BitDepth::UInt10);
            assert_eq!(r.matrix, aim);
            assert_eq!(r.offsets, aim_offs);
        }
        // Compose inverse with forward.
        {
            let mut a = MatrixOpData::from_values(
                &[
                    2., 0., 0., 0., 0., 4., 0., 0., 0., 0., 0.5, 0., 0., 0., 0., 1.,
                ],
                &[1.0, 2.0, 0.0, 0.5],
                TransformDirection::Forward,
            );
            a.direction = TransformDirection::Inverse;
            let b = MatrixOpData::from_values(
                &[
                    2., 0., 0., 0., 0., 1.5, 0., 0., 0., 0., 3., 0., 0., 0., 0., 1.,
                ],
                &[2.0, 4.0, 0.0, 0.5],
                TransformDirection::Forward,
            );
            let r = a.get_as_forward().unwrap().compose(&b).unwrap();
            assert_eq!(
                r.matrix,
                [1., 0., 0., 0., 0., 0.375, 0., 0., 0., 0., 6., 0., 0., 0., 0., 1.]
            );
            assert_eq!(r.offsets, [1.0, 3.25, 0.0, 0.0]);
        }
        // Compose forward with inverse.
        {
            let a = MatrixOpData::from_values(
                &[
                    2., 0., 0., 0., 0., 4., 0., 0., 0., 0., 0.5, 0., 0., 0., 0., 1.,
                ],
                &[1.0, 2.0, 0.0, 0.5],
                TransformDirection::Forward,
            );
            let b = MatrixOpData::from_values(
                &[
                    2., 0., 0., 0., 0., 0.25, 0., 0., 0., 0., 4., 0., 0., 0., 0., 1.,
                ],
                &[2.0, 4.0, 0.0, 0.5],
                TransformDirection::Inverse,
            );
            assert!(a.compose(&b).is_err());
            let r = a.compose(&b.get_as_forward().unwrap()).unwrap();
            assert_eq!(
                r.matrix,
                [1., 0., 0., 0., 0., 16., 0., 0., 0., 0., 0.125, 0., 0., 0., 0., 1.]
            );
            assert_eq!(r.offsets, [-0.5, -8.0, 0.0, 0.0]);
        }
    }

    #[test]
    fn data_equality() {
        let mut m1 = MatrixOpData::new();
        m1.matrix[0] = 2.0;
        let mut m2 = MatrixOpData::new();
        m2.metadata.set_id("invalid_u_id_test");
        m2.matrix[0] = 2.0;
        // Metadata is ignored.
        assert!(m1 == m2);
        // File bit-depth is ignored.
        m1.file_input_bit_depth = BitDepth::UInt8;
        assert!(m1 == m2);
        let mut m3 = MatrixOpData::new();
        m3.matrix[0] = 6.0;
        assert!(m1 != m3);
        let mut m4 = MatrixOpData::new();
        m4.matrix[0] = 2.0;
        assert!(m1 == m4);
        m4.offsets[3] = 1e-5f32 as f64;
        assert!(m1 != m4);
    }

    #[test]
    fn data_rgb() {
        let mut m = MatrixOpData::new();
        m.set_rgb(&[0., 1., 2., 3., 4., 5., 6., 7., 8.]);
        assert_eq!(
            m.matrix,
            [0., 1., 2., 0., 3., 4., 5., 0., 6., 7., 8., 0., 0., 0., 0., 1.]
        );
    }

    #[test]
    fn data_rgba() {
        let v = [
            0., 1., 2., 3., 4., 5., 6., 7., 8., 9., 10., 11., 12., 13., 15., 0.,
        ];
        let m = MatrixOpData::from_values(&v, &[0.0; 4], TransformDirection::Forward);
        assert_eq!(m.matrix, v);
        assert!(!m.is_no_op());
        assert!(m.has_channel_crosstalk());
        assert!(!m.is_diagonal());
        assert!(!m.is_identity());
    }

    #[test]
    fn data_inverse_identity() {
        let mut r = MatrixOpData::new();
        r.file_input_bit_depth = BitDepth::F32;
        r.file_output_bit_depth = BitDepth::UInt12;
        r.direction = TransformDirection::Inverse;
        assert!(r.is_no_op());
        assert!(!r.has_channel_crosstalk());
        assert!(r.is_diagonal());
        assert!(r.is_identity());
        assert!(!r.has_offsets());

        let f = r.get_as_forward().unwrap();
        assert_eq!(f.direction, TransformDirection::Forward);
        // The bit-depths are swapped.
        assert_eq!(f.file_input_bit_depth, BitDepth::UInt12);
        assert_eq!(f.file_output_bit_depth, BitDepth::F32);
        assert!(f.is_diagonal());
        assert!(f.is_identity());
        assert!(!f.has_offsets());
    }

    #[test]
    fn data_inverse_singular() {
        let m = [
            1.0,
            0.,
            0.,
            0.2,
            0.,
            0.,
            0.,
            0.,
            0.,
            0.,
            0.,
            0.,
            0.2,
            0.,
            0.,
            1.0f32 as f64,
        ];
        let mut s = MatrixOpData::from_values(&m, &[0.0; 4], TransformDirection::Inverse);
        s.matrix[3] = 0.2f32 as f64;
        s.matrix[12] = 0.2f32 as f64;
        assert!(!s.is_no_op());
        assert!(s.has_channel_crosstalk());
        assert!(!s.is_unity_diagonal());
        assert!(!s.is_diagonal());
        assert!(!s.is_identity());
        assert!(!s.has_offsets());
        let e = s.get_as_forward().unwrap_err();
        assert!(e.message().contains("Singular Matrix can't be inverted"));
    }

    #[test]
    fn data_inverse() {
        let m: [f32; 16] = [
            0.9, 0.8, -0.7, 0.6, -0.4, 0.5, 0.3, 0.2, 0.1, -0.2, 0.4, 0.3, -0.5, 0.6, 0.7, 0.8,
        ];
        let o: [f32; 4] = [-0.1, 0.2, -0.3, 0.4];
        let mut r = MatrixOpData::new();
        for i in 0..16 {
            r.matrix[i] = m[i] as f64;
        }
        for i in 0..4 {
            r.offsets[i] = o[i] as f64;
        }
        assert!(!r.is_no_op());
        assert!(r.has_channel_crosstalk());
        let f = r.get_as_forward().unwrap();
        assert!(r == f);

        r.direction = TransformDirection::Inverse;
        let inv = r.get_as_forward().unwrap();
        let expected: [f32; 16] = [
            0.75,
            3.5,
            3.5,
            -2.75,
            0.546296296296297,
            3.90740740740741,
            1.31481481481482,
            -1.87962962962963,
            0.12037037037037,
            4.75925925925926,
            4.01851851851852,
            -2.78703703703704,
            -0.0462962962962963,
            -4.90740740740741,
            -2.31481481481482,
            3.37962962962963,
        ];
        let expected_offsets: [f32; 4] = [
            1.525,
            0.419444444444445,
            1.38055555555556,
            -1.06944444444444,
        ];
        for i in 0..16 {
            assert_close(inv.matrix[i], expected[i] as f64, 1e-6);
        }
        for i in 0..4 {
            assert_close(inv.offsets[i], expected_offsets[i] as f64, 1e-6);
        }
    }

    #[test]
    fn data_channel_crosstalk() {
        let mut r = MatrixOpData::new();
        assert!(!r.has_channel_crosstalk());
        r.offsets = [-0.1, 0.2, -0.3, 0.4];
        assert!(!r.has_channel_crosstalk());
        r.matrix = [
            0.9, 0., 0., 0., 0., 0.5, 0., 0., 0., 0., -0.4, 0., 0., 0., 0., 0.8,
        ];
        assert!(!r.has_channel_crosstalk());
        r.matrix = IDENTITY_MATRIX44;
        r.matrix[11] = 0.000000001f32 as f64;
        assert!(r.has_channel_crosstalk());
    }

    // MatrixOp_tests.cpp

    #[test]
    fn op_scale() {
        let mut ops = OpVec::new();
        let scale = [1.1, 1.3, 0.3, -1.0];
        create_scale_op(&mut ops, &scale, TransformDirection::Forward).unwrap();
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].name(), "Matrix");
        create_scale_op(&mut ops, &scale, TransformDirection::Inverse).unwrap();
        assert_eq!(ops.len(), 2);
        assert!(!ops[0].cache_id().is_empty());

        let src = [
            0.1004f32, 0.2, 0.3, 0.4, -0.1008, -0.2, 5.001, 0.1234, 1.0090, 1.0, 1.0, 1.0,
        ];
        let dst = [
            0.11044f32, 0.26, 0.090, -0.4, -0.11088, -0.26, 1.5003, -0.1234, 1.10990, 1.30, 0.300,
            -1.0,
        ];
        let tmp = apply_op(ops[0].as_ref(), &src);
        for i in 0..12 {
            assert_close(dst[i] as f64, tmp[i] as f64, 1e-6);
        }
        let tmp = apply_op(ops[1].as_ref(), &tmp);
        for i in 0..12 {
            assert_close(src[i] as f64, tmp[i] as f64, 1e-6);
        }
    }

    #[test]
    fn op_offset() {
        let mut ops = OpVec::new();
        let offset = [1.1, -1.3, 0.3, -1.0];
        create_offset_op(&mut ops, &offset, TransformDirection::Forward).unwrap();
        create_offset_op(&mut ops, &offset, TransformDirection::Inverse).unwrap();
        assert_eq!(ops.len(), 2);
        let src = [
            0.1004f32, 0.2, 0.3, 0.4, -0.1008, -0.2, 5.01, 0.1234, 1.0090, 1.0, 1.0, 1.0,
        ];
        let dst = [
            1.2004f32, -1.1, 0.60, -0.6, 0.9992, -1.5, 5.31, -0.8766, 2.1090, -0.3, 1.30, 0.0,
        ];
        let tmp = apply_op(ops[0].as_ref(), &src);
        for i in 0..12 {
            assert_close(dst[i] as f64, tmp[i] as f64, 1e-6);
        }
        let tmp = apply_op(ops[1].as_ref(), &tmp);
        for i in 0..12 {
            assert_close(src[i] as f64, tmp[i] as f64, 1e-6);
        }
    }

    const M1: [f64; 16] = [
        1.1, 0.2, 0.3, 0.4, 0.5, 1.6, 0.7, 0.8, 0.2, 0.1, 1.1, 0.2, 0.3, 0.4, 0.5, 1.6,
    ];

    #[test]
    fn op_matrix() {
        let mut ops = OpVec::new();
        create_matrix_op_no_offset(&mut ops, &M1, TransformDirection::Forward).unwrap();
        create_matrix_op_no_offset(&mut ops, &M1, TransformDirection::Inverse).unwrap();
        assert_eq!(ops.len(), 2);
        let src = [
            0.1004f32, 0.201, 0.303, 0.408, -0.1008, -0.207, 5.002, 0.123422, 1.0090, 1.009, 1.044,
            1.001,
        ];
        let dst = [
            0.40474f64, 0.91030, 0.45508, 0.914820, 1.3976888, 3.2185376, 5.4860244, 2.5854352,
            2.02530, 3.65050, 1.65130, 2.829900,
        ];
        let tmp = apply_op(ops[0].as_ref(), &src);
        for i in 0..12 {
            assert!(equal_with_safe_rel_error(dst[i] as f32, tmp[i], 1e-6, 1.0));
        }
        let tmp = apply_op(ops[1].as_ref(), &tmp);
        for i in 0..12 {
            assert!(equal_with_safe_rel_error(src[i], tmp[i], 1e-6, 1.0));
        }
    }

    #[test]
    fn op_arbitrary() {
        let offset = [-0.5, -0.25, 0.25, 0.1];
        let mut ops = OpVec::new();
        create_matrix_offset_op(&mut ops, &M1, &offset, TransformDirection::Forward).unwrap();
        create_matrix_offset_op(&mut ops, &M1, &offset, TransformDirection::Inverse).unwrap();
        let src = [
            0.1004f32, 0.201, 0.303, 0.408, -0.1008, -0.207, 5.02, 0.123422, 1.0090, 1.009, 1.044,
            1.001,
        ];
        let dst = [
            -0.09526f32,
            0.660300,
            0.70508,
            1.014820,
            0.9030888,
            2.9811376,
            5.7558244,
            2.6944352,
            1.52530,
            3.400500,
            1.90130,
            2.929900,
        ];
        let tmp = apply_op(ops[0].as_ref(), &src);
        for i in 0..12 {
            assert!(equal_with_safe_rel_error(dst[i], tmp[i], 1e-6, 1.0));
        }
        let tmp = apply_op(ops[1].as_ref(), &tmp);
        for i in 0..12 {
            assert!(equal_with_safe_rel_error(src[i], tmp[i], 1e-6, 1.0));
        }
        let cloned = ops[1].clone_box();
        assert!(!cloned.cache_id().is_empty());
        assert_eq!(cloned.cache_id(), ops[1].cache_id());
        assert_ne!(ops[0].cache_id(), ops[1].cache_id());
    }

    #[test]
    fn op_create_fit_op() {
        let oldmin4 = [0.0, 1.0, 1.0, 4.0];
        let oldmax4 = [1.0, 3.0, 4.0, 8.0];
        let newmin4 = [0.0, 2.0, 0.0, 4.0];
        let newmax4 = [1.0, 6.0, 9.0, 20.0];
        let mut ops = OpVec::new();
        create_fit_op(
            &mut ops,
            &oldmin4,
            &oldmax4,
            &newmin4,
            &newmax4,
            TransformDirection::Forward,
        )
        .unwrap();
        create_fit_op(
            &mut ops,
            &oldmin4,
            &oldmax4,
            &newmin4,
            &newmax4,
            TransformDirection::Inverse,
        )
        .unwrap();
        assert_eq!(ops.len(), 2);
        let src = [
            0.1004f32, 0.201, 0.303, 0.408, -0.10, -2.10, 0.5, 1.0, 42.0, 1.0, -1.11, -0.001,
        ];
        let dst = [
            0.1004f32, 0.402, -2.091, -10.368, -0.10, -4.20, -1.50, -8.0, 42.0, 2.0, -6.33, -12.004,
        ];
        let tmp = apply_op(ops[0].as_ref(), &src);
        for i in 0..12 {
            assert_close(dst[i] as f64, tmp[i] as f64, 1e-6);
        }
        let tmp = apply_op(ops[1].as_ref(), &tmp);
        for i in 0..12 {
            assert_close(src[i] as f64, tmp[i] as f64, 1e-6);
        }
    }

    #[test]
    fn op_create_saturation_op() {
        let luma = [1.0, 0.5, 0.1];
        let mut ops = OpVec::new();
        create_saturation_op(&mut ops, 0.9, &luma, TransformDirection::Forward).unwrap();
        create_saturation_op(&mut ops, 0.9, &luma, TransformDirection::Inverse).unwrap();
        let src = [
            0.1004f32, 0.201, 0.303, 0.408, -0.10, -2.1, 0.5, 1.0, 42.0, 1.0, -1.11, -0.001,
        ];
        let dst = [
            0.11348f32, 0.20402, 0.29582, 0.408, -0.2, -2.0, 0.34, 1.0, 42.0389, 5.1389, 3.2399,
            -0.001,
        ];
        let tmp = apply_op(ops[0].as_ref(), &src);
        for i in 0..12 {
            assert_close(dst[i] as f64, tmp[i] as f64, 1e-6);
        }
        let tmp = apply_op(ops[1].as_ref(), &tmp);
        for i in 0..12 {
            assert_close(src[i] as f64, tmp[i] as f64, 1e-5);
        }
    }

    #[test]
    fn op_create_min_max_op() {
        let mut ops = OpVec::new();
        create_min_max_op(
            &mut ops,
            &[1.0, 2.0, 3.0],
            &[2.0, 4.0, 6.0],
            TransformDirection::Forward,
        )
        .unwrap();
        assert_eq!(ops.len(), 1);
        let src = [
            1.0f32, 2.0, 3.0, 1.0, 1.5, 2.5, 3.15, 1.0, 0.0, 0.0, 0.0, 1.0, 3.0, 5.0, 6.3, 1.0,
            2.0, 4.0, 6.0, 1.0,
        ];
        let dst = [
            0.0f32, 0.0, 0.0, 1.0, 0.5, 0.25, 0.05, 1.0, -1.0, -1.0, -1.0, 1.0, 2.0, 1.5, 1.1, 1.0,
            1.0, 1.0, 1.0, 1.0,
        ];
        let tmp = apply_op(ops[0].as_ref(), &src);
        for i in 0..20 {
            assert_close(dst[i] as f64, tmp[i] as f64, 1e-6);
        }
        // Identity: nothing is created.
        let mut ops = OpVec::new();
        create_min_max_op(&mut ops, &[0.0; 3], &[1.0; 3], TransformDirection::Forward).unwrap();
        assert!(ops.is_empty());
        assert!(
            create_min_max_op(&mut ops, &[1.0; 3], &[1.0; 3], TransformDirection::Forward).is_err()
        );
    }

    #[test]
    fn op_combining() {
        let v1 = [-0.5, -0.25, 0.25, 0.0];
        let m2 = [
            1.1, -0.1, -0.1, 0.0, 0.1, 0.9, -0.2, 0.0, 0.05, 0.0, 1.1, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        let v2 = [-0.2, -0.1, -0.1, -0.2];
        let source = [
            0.1f32, 0.2, 0.3, 0.4, -0.1, -0.2, 50.0, 123.4, 1.0, 1.0, 1.0, 1.0,
        ];
        let flags = OptimizationFlags::DEFAULT;

        let check = |ops: &OpVec, combined: &OpVec| {
            for test in 0..3 {
                let px = &source[4 * test..4 * test + 4];
                let tmp = apply_ops(ops, px);
                let tmp2 = apply_ops(combined, px);
                for i in 0..4 {
                    assert_close(tmp2[i] as f64, tmp[i] as f64, 1e-4);
                }
            }
        };

        {
            let mut mat1 = MatrixOpData::from_values(&M1, &v1, TransformDirection::Forward);
            mat1.metadata.add_attribute(METADATA_NAME, "mat1");
            mat1.metadata.add_attribute("Attrib", "1");
            let mut mat2 = MatrixOpData::from_values(&m2, &v2, TransformDirection::Forward);
            mat2.metadata
                .add_attribute(crate::types::METADATA_ID, "ID2");
            mat2.metadata.add_attribute("Attrib", "2");
            let mut ops = OpVec::new();
            create_matrix_op_from_data(&mut ops, &mat1, TransformDirection::Forward).unwrap();
            create_matrix_op_from_data(&mut ops, &mat2, TransformDirection::Forward).unwrap();
            let combined = ops[0].combine_with(ops[1].as_ref(), flags).unwrap();
            assert_eq!(combined.len(), 1);
            let cd = mat_op(&combined, 0).data();
            assert_eq!(cd.metadata.name(), "mat1");
            assert_eq!(cd.metadata.id(), "ID2");
            assert_eq!(cd.metadata.attributes.len(), 3);
            assert_eq!(cd.metadata.attributes[1].0, "Attrib");
            assert_eq!(cd.metadata.attributes[1].1, "1 + 2");
            let cache_combined = combined[0].cache_id();
            assert!(!cache_combined.is_empty());
            check(&ops, &combined);

            // Now use the optimizer.
            let optimized = optimize_ops(&ops, flags);
            assert_eq!(optimized.len(), 1);
            assert_eq!(optimized[0].cache_id(), cache_combined);
            check(&ops, &optimized);

            // Composition is not done without the COMP_MATRIX flag.
            assert!(ops[0]
                .combine_with(ops[1].as_ref(), OptimizationFlags::NONE)
                .is_none());
            assert_eq!(optimize_ops(&ops, OptimizationFlags::NONE).len(), 2);
        }

        for (d1, d2) in [
            (TransformDirection::Forward, TransformDirection::Inverse),
            (TransformDirection::Inverse, TransformDirection::Forward),
            (TransformDirection::Inverse, TransformDirection::Inverse),
        ] {
            let mut ops = OpVec::new();
            create_matrix_offset_op(&mut ops, &M1, &v1, d1).unwrap();
            create_matrix_offset_op(&mut ops, &m2, &v2, d2).unwrap();
            let combined = ops[0].combine_with(ops[1].as_ref(), flags).unwrap();
            assert_eq!(combined.len(), 1);
            check(&ops, &combined);
        }

        {
            let offset = [1.1, -1.3, 0.3, 0.0];
            let offset_inv = [-1.1, 1.3, -0.3, 0.0];
            let mut ops = OpVec::new();
            create_offset_op(&mut ops, &offset, TransformDirection::Forward).unwrap();
            create_offset_op(&mut ops, &offset, TransformDirection::Inverse).unwrap();
            create_offset_op(&mut ops, &offset_inv, TransformDirection::Forward).unwrap();
            // Offset (FWD) and offset (INV) combine into an identity which is removed.
            let c = ops[0].combine_with(ops[1].as_ref(), flags).unwrap();
            assert!(c.is_empty());
            let c = ops[0].combine_with(ops[2].as_ref(), flags).unwrap();
            assert!(c.is_empty());
        }
    }

    #[test]
    fn op_throw_create() {
        let mut ops = OpVec::new();
        let e = create_fit_op(
            &mut ops,
            &[1.0, 0.0, 0.0, 0.0],
            &[1.0, 2.0, 3.0, 4.0],
            &[0.0; 4],
            &[1.0, 4.0, 9.0, 16.0],
            TransformDirection::Forward,
        )
        .unwrap_err();
        assert!(e
            .message()
            .starts_with("Cannot create Fit operator. Max value equals min value"));
        assert_eq!(
            e.message(),
            "Cannot create Fit operator. Max value equals min value '1' in channel index 0."
        );
    }

    #[test]
    fn op_throw_validate() {
        // A matrix that can't be inverted can't be used in the inverse direction.
        let mut ops = OpVec::new();
        let e = create_scale_op(&mut ops, &[0.0, 1.3, 0.3, 1.0], TransformDirection::Inverse)
            .unwrap_err();
        assert!(e.message().contains("Singular Matrix can't be inverted"));
        assert!(ops.is_empty());
    }

    #[test]
    fn op_throw_combine() {
        let mut ops = OpVec::new();
        create_offset_op(
            &mut ops,
            &[1.1, -1.3, 0.3, 0.0],
            TransformDirection::Forward,
        )
        .unwrap();
        create_file_no_op(&mut ops, "NoOp");
        assert!(ops[0]
            .combine_with(ops[1].as_ref(), OptimizationFlags::ALL)
            .is_none());
    }

    #[test]
    fn op_no_op() {
        let offset = [0.0; 4];
        let scale = [1.0; 4];
        let m = IDENTITY_MATRIX44;
        let oldmin4 = [0.0; 4];
        let oldmax4 = [1.0, 2.0, 3.0, 4.0];
        for dir in [TransformDirection::Forward, TransformDirection::Inverse] {
            let mut ops = OpVec::new();
            create_offset_op(&mut ops, &offset, dir).unwrap();
            create_scale_op(&mut ops, &scale, dir).unwrap();
            create_matrix_op_no_offset(&mut ops, &m, dir).unwrap();
            create_matrix_offset_op(&mut ops, &m, &offset, dir).unwrap();
            create_fit_op(&mut ops, &oldmin4, &oldmax4, &oldmin4, &oldmax4, dir).unwrap();
            create_saturation_op(&mut ops, 1.0, &[1.0, 1.0, 1.0], dir).unwrap();
            assert_eq!(ops.len(), 6);
            for op in &ops {
                assert!(op.is_no_op());
                assert!(op.is_identity());
                assert!(optimize_ops(&[op.clone()], OptimizationFlags::DEFAULT).is_empty());
            }
        }
        let mut ops = OpVec::new();
        create_identity_matrix_op(&mut ops).unwrap();
        assert_eq!(ops.len(), 1);
        assert!(ops[0].is_no_op());
    }

    #[test]
    fn op_is_same_type() {
        let mut ops = OpVec::new();
        create_saturation_op(&mut ops, 0.9, &[1.0, 0.5, 0.1], TransformDirection::Forward).unwrap();
        create_scale_op(&mut ops, &[1.1, 1.3, 0.3, 1.0], TransformDirection::Forward).unwrap();
        create_log_op(
            &mut ops,
            10.0,
            &[0.18, 0.5, 0.3],
            &[1.0, 1.0, 1.0],
            &[2.0, 4.0, 8.0],
            &[0.1, 0.1, 0.1],
            TransformDirection::Forward,
        )
        .unwrap();
        assert_eq!(ops.len(), 3);
        assert!(ops[0].downcast_ref::<MatrixOp>().is_some());
        assert!(ops[1].downcast_ref::<MatrixOp>().is_some());
        assert!(ops[2].downcast_ref::<MatrixOp>().is_none());
    }

    #[test]
    fn op_has_channel_crosstalk() {
        let mut ops = OpVec::new();
        create_scale_op(&mut ops, &[1.1, 1.3, 0.3, 1.0], TransformDirection::Forward).unwrap();
        create_saturation_op(&mut ops, 0.9, &[1.0, 0.5, 0.1], TransformDirection::Forward).unwrap();
        assert!(!ops[0].has_channel_crosstalk());
        assert!(ops[1].has_channel_crosstalk());
    }

    #[test]
    fn op_removing_red_green() {
        let mut mat = MatrixOpData::new();
        mat.matrix[0] = 0.0;
        mat.matrix[5] = 0.0;
        let mut ops = OpVec::new();
        create_matrix_op_from_data(&mut ops, &mat, TransformDirection::Forward).unwrap();
        let src = [
            0.1004f32, 0.201, 0.303, 0.408, -0.1008, -0.207, 0.502, 0.123422, 1.0090, 1.009, 1.044,
            1.001, 1.1, 1.2, 1.3, 1.0, 1.4, 1.5, 1.6, 0.0, 1.7, 1.8, 1.9, 1.0,
        ];
        let tmp = apply_op(ops[0].as_ref(), &src);
        for idx in (0..24).step_by(4) {
            assert_eq!(tmp[idx], 0.0);
            assert_eq!(tmp[idx + 1], 0.0);
            assert_eq!(tmp[idx + 2], src[idx + 2]);
            assert_eq!(tmp[idx + 3], src[idx + 3]);
        }
    }

    #[test]
    fn op_create_transform() {
        let mut mat = MatrixOpData::new();
        mat.metadata.add_attribute("name", "test");
        mat.offsets = [1., 2., 3., 4.];
        mat.matrix = M1;
        let mut ops = OpVec::new();
        create_matrix_op_from_data(&mut ops, &mat, TransformDirection::Forward).unwrap();
        let t = match ops[0].to_transform().unwrap() {
            Transform::Matrix(t) => t,
            _ => panic!("expected a matrix transform"),
        };
        assert_eq!(t.metadata.attributes.len(), 1);
        assert_eq!(
            t.metadata.attributes[0],
            ("name".to_string(), "test".to_string())
        );
        assert_eq!(t.direction, TransformDirection::Forward);
        assert_eq!(t.offset, mat.offsets);
        assert_eq!(t.matrix, mat.matrix);

        let config = Config::create_raw();
        let ctx = Context::new();
        let mut back = OpVec::new();
        t.build_ops(&mut back, &config, &ctx, TransformDirection::Forward)
            .unwrap();
        t.build_ops(&mut back, &config, &ctx, TransformDirection::Inverse)
            .unwrap();
        assert_eq!(back.len(), 2);
        let m0 = mat_op(&back, 0).data();
        let m1 = mat_op(&back, 1).data();
        assert_eq!(m0.direction, TransformDirection::Forward);
        assert_eq!(m0.matrix, mat.matrix);
        assert_eq!(m0.offsets, mat.offsets);
        // The inverse op holds the inverted (forward) matrix.
        let inv = MatrixOpData {
            direction: TransformDirection::Inverse,
            ..mat.clone()
        }
        .get_as_forward()
        .unwrap();
        assert_eq!(m1.direction, TransformDirection::Forward);
        assert_eq!(m1.matrix, inv.matrix);
        assert_eq!(m1.offsets, inv.offsets);
    }

    // MatrixTransform_tests.cpp

    #[test]
    fn transform_basic() {
        let mut matrix = MatrixTransform::default();
        assert_eq!(matrix.direction, TransformDirection::Forward);
        assert_eq!(matrix.matrix, IDENTITY_MATRIX44);
        assert_eq!(matrix.offset, [0.0; 4]);
        assert_eq!(matrix.file_input_bit_depth, BitDepth::Unknown);
        assert_eq!(matrix.file_output_bit_depth, BitDepth::Unknown);
        matrix.direction = TransformDirection::Inverse;
        assert!(matrix.validate().is_ok());
        matrix.matrix = [0.0; 16];
        let e = matrix.validate().unwrap_err();
        assert_eq!(
            e.message(),
            "MatrixTransform validation failed: Singular Matrix can't be inverted."
        );
        matrix.direction = TransformDirection::Forward;
        assert!(matrix.validate().is_ok());
    }

    #[test]
    fn transform_equals() {
        let mut m1 = MatrixTransform::default();
        let m2 = MatrixTransform::default();
        assert!(m1.equals(&m2));
        m1.direction = TransformDirection::Inverse;
        assert!(!m1.equals(&m2));
        m1.direction = TransformDirection::Forward;
        m1.matrix[0] = 1.0 + 1e-6;
        assert!(!m1.equals(&m2));
        m1.matrix[0] = 1.0;
        assert!(m1.equals(&m2));
        m1.offset[0] = 1e-6;
        assert!(!m1.equals(&m2));
        m1.offset[0] = 0.0;
        m1.metadata.set_id("id");
        m1.file_input_bit_depth = BitDepth::UInt8;
        assert!(m1.equals(&m2));
    }

    #[test]
    fn transform_build_and_optimize() {
        let config = Config::create_raw();
        let ctx = Context::new();
        let t = MatrixTransform::new(M1, [0.1, 0.2, 0.3, 0.4]);
        let mut ops = OpVec::new();
        t.build_ops(&mut ops, &config, &ctx, TransformDirection::Forward)
            .unwrap();
        t.build_ops(&mut ops, &config, &ctx, TransformDirection::Inverse)
            .unwrap();
        assert_eq!(ops.len(), 2);
        // A matrix followed by its inverse combines into an identity, removed.
        assert!(optimize_ops(&ops, OptimizationFlags::DEFAULT).is_empty());
        assert_eq!(optimize_ops(&ops, OptimizationFlags::NONE).len(), 2);
    }
}
