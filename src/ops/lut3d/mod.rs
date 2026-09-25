//! 3D LUT op (port of `Lut3DOpData.cpp`, `Lut3DOp.cpp`, `Lut3DOpCPU.cpp` and
//! `Lut3DTransform.cpp`).
//!
//! Values are stored with the **blue** index changing fastest:
//! `index = ((r * n + g) * n + b) * 3`.
//!
//! Forward evaluation is trilinear or tetrahedral. The exact inverse (CPU
//! only) searches the LUT cubes containing the value with a range tree and
//! inverts the tetrahedral interpolation; the fast inverse bakes the exact
//! inverse into a forward 48^3 LUT (see [`make_fast_lut3d_from_inverse`]).

mod cpu;
#[cfg(test)]
mod tests;

use self::cpu::Lut3DRenderer;
use crate::config::Config;
use crate::context::Context;
use crate::error::{Error, Result};
use crate::format_metadata::FormatMetadata;
use crate::ops::lut1d::{
    eval_transform, format_float_g, identity_replacement_ops, rgb_min_max, IdentityReplacement,
};
use crate::ops::{hash_f32, Op, OpRc, OpVec, Pixel};
use crate::transforms::{BuildOps, Lut3DTransform, Transform, Validate};
use crate::types::{BitDepth, Interpolation, OptimizationFlags, TransformDirection};
use std::any::Any;
use std::fmt;
use std::ops::{Index, IndexMut};
use std::sync::{Arc, OnceLock};

/// Maximum grid size of a 3D LUT (`Max3DLUTLength` in OCIO).
pub const MAX_3D_LUT_LENGTH: usize = 129;

/// Grid size of the forward LUT used by the fast inverse.
pub const FAST_INVERSE_GRID_SIZE: usize = 48;

// ---------------------------------------------------------------------------
// Index helpers.

/// Index of a red-fastest 3D LUT entry.
pub fn get_lut3d_index_red_fast(
    r: usize,
    g: usize,
    b: usize,
    size_r: usize,
    size_g: usize,
    _size_b: usize,
) -> usize {
    3 * (r + size_r * (g + size_g * b))
}

/// Index of a blue-fastest 3D LUT entry.
pub fn get_lut3d_index_blue_fast(
    r: usize,
    g: usize,
    b: usize,
    _size_r: usize,
    size_g: usize,
    size_b: usize,
) -> usize {
    3 * (b + size_b * (g + size_g * r))
}

/// Ordering of 3D LUT values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lut3DOrder {
    /// Red changes fastest.
    FastRed,
    /// Blue changes fastest.
    FastBlue,
}

/// Fill `img` with an identity 3D LUT of `edge_len`^3 pixels of
/// `num_channels` (at least 3) in the given order.
pub fn generate_identity_lut3d(
    img: &mut [f32],
    edge_len: usize,
    num_channels: usize,
    order: Lut3DOrder,
) -> Result<()> {
    if num_channels < 3 {
        crate::bail!("Cannot generate idenitity 3d LUT with less than 3 channels.");
    }
    let total = edge_len * edge_len * edge_len;
    if img.len() < total * num_channels {
        crate::bail!(
            "Cannot generate identity 3d LUT: buffer of {} values is too small for {} values.",
            img.len(),
            total * num_channels
        );
    }
    let c = 1.0f32 / (edge_len as f32 - 1.0);
    for i in 0..total {
        let (r, g, b) = match order {
            Lut3DOrder::FastRed => (
                i % edge_len,
                (i / edge_len) % edge_len,
                (i / edge_len / edge_len) % edge_len,
            ),
            Lut3DOrder::FastBlue => (
                (i / edge_len / edge_len) % edge_len,
                (i / edge_len) % edge_len,
                i % edge_len,
            ),
        };
        img[num_channels * i] = r as f32 * c;
        img[num_channels * i + 1] = g as f32 * c;
        img[num_channels * i + 2] = b as f32 * c;
    }
    Ok(())
}

/// Infer the edge length of a cube from its number of pixels.
pub fn get_3d_lut_edge_len_from_num_pixels(num_pixels: usize) -> Result<usize> {
    let dim = (num_pixels as f32).powf(1.0 / 3.0).round() as usize;
    if dim.checked_mul(dim).and_then(|d| d.checked_mul(dim)) != Some(num_pixels) {
        crate::bail!(
            "Cannot infer 3D LUT size. {num_pixels} element(s) does not correspond to a unform cube edge length. \
             (nearest edge length is {dim})."
        );
    }
    Ok(dim)
}

// ---------------------------------------------------------------------------
// Array.

/// The values of a 3D LUT (port of `Lut3DOpData::Lut3DArray`), blue fastest.
#[derive(Clone, PartialEq)]
pub struct Lut3DArray {
    length: usize,
    values: Vec<f32>,
}

impl fmt::Debug for Lut3DArray {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Lut3DArray")
            .field("length", &self.length)
            .finish_non_exhaustive()
    }
}

impl Lut3DArray {
    /// Identity array of the given grid size.
    pub fn new(length: usize) -> Result<Self> {
        let mut a = Self {
            length: 0,
            values: Vec::new(),
        };
        a.resize(length, 3)?;
        a.fill();
        Ok(a)
    }

    fn fill(&mut self) {
        let length = self.length;
        let step = 1.0f32 / (length as f32 - 1.0);
        let max_entries = length * length * length;
        for idx in 0..max_entries {
            self.values[3 * idx] = ((idx / length / length) % length) as f32 * step;
            self.values[3 * idx + 1] = ((idx / length) % length) as f32 * step;
            self.values[3 * idx + 2] = (idx % length) as f32 * step;
        }
    }

    /// Resize the array (new values are zeros). The number of color
    /// components is always 3.
    pub fn resize(&mut self, length: usize, _num_color_components: usize) -> Result<()> {
        if length > MAX_3D_LUT_LENGTH {
            crate::bail!(
                "LUT 3D: Grid size '{length}' must not be greater than '{MAX_3D_LUT_LENGTH}'."
            );
        }
        self.length = length;
        self.values.resize(self.num_values(), 0.0);
        Ok(())
    }

    /// Grid size.
    pub fn length(&self) -> usize {
        self.length
    }

    /// Always 3.
    pub fn num_color_components(&self) -> usize {
        3
    }

    /// Always 3.
    pub fn max_color_components(&self) -> usize {
        3
    }

    /// Expected number of values (`length^3 * 3`).
    pub fn num_values(&self) -> usize {
        self.length * self.length * self.length * 3
    }

    pub fn values(&self) -> &[f32] {
        &self.values
    }

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

    /// RGB value at grid indices `(i, j, k)` (red, green, blue).
    pub(crate) fn rgb(&self, i: usize, j: usize, k: usize) -> [f32; 3] {
        let off = (i * self.length * self.length + j * self.length + k) * 3;
        [self.values[off], self.values[off + 1], self.values[off + 2]]
    }

    /// Set the RGB value at grid indices `(i, j, k)`.
    pub(crate) fn set_rgb(&mut self, i: usize, j: usize, k: usize, rgb: [f32; 3]) {
        let off = (i * self.length * self.length + j * self.length + k) * 3;
        self.values[off..off + 3].copy_from_slice(&rgb);
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

impl Index<usize> for Lut3DArray {
    type Output = f32;
    fn index(&self, i: usize) -> &f32 {
        &self.values[i]
    }
}

impl IndexMut<usize> for Lut3DArray {
    fn index_mut(&mut self, i: usize) -> &mut f32 {
        &mut self.values[i]
    }
}

// ---------------------------------------------------------------------------
// Op data.

/// Parameters of a 3D LUT (port of `Lut3DOpData`).
#[derive(Clone)]
pub struct Lut3DOpData {
    interpolation: Interpolation,
    array: Lut3DArray,
    direction: TransformDirection,
    file_out_bit_depth: BitDepth,
    metadata: FormatMetadata,
}

impl fmt::Debug for Lut3DOpData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Lut3DOpData")
            .field("direction", &self.direction)
            .field("interpolation", &self.interpolation)
            .field("array", &self.array)
            .field("file_out_bit_depth", &self.file_out_bit_depth)
            .finish_non_exhaustive()
    }
}

/// Equality as OCIO's `Lut3DOpData::equals`: same direction, interpolation
/// and array (metadata ignored).
impl PartialEq for Lut3DOpData {
    fn eq(&self, other: &Self) -> bool {
        self.direction == other.direction
            && self.interpolation == other.interpolation
            && self.array == other.array
    }
}

impl Lut3DOpData {
    /// Identity LUT of the given grid size.
    pub fn new(grid_size: usize) -> Result<Self> {
        Self::with_interpolation(Interpolation::Default, grid_size)
    }

    /// Identity LUT with the given direction.
    pub fn with_direction(grid_size: usize, dir: TransformDirection) -> Result<Self> {
        let mut l = Self::new(grid_size)?;
        l.direction = dir;
        Ok(l)
    }

    /// Identity LUT with the given interpolation.
    pub fn with_interpolation(interpolation: Interpolation, grid_size: usize) -> Result<Self> {
        Ok(Self {
            interpolation,
            array: Lut3DArray::new(grid_size)?,
            direction: TransformDirection::Forward,
            file_out_bit_depth: BitDepth::Unknown,
            metadata: FormatMetadata::default(),
        })
    }

    /// Build from a transform (no validation of the interpolation).
    pub fn from_transform(t: &Lut3DTransform) -> Result<Self> {
        if t.grid_size > MAX_3D_LUT_LENGTH {
            crate::bail!(
                "LUT 3D: Grid size '{}' must not be greater than '{MAX_3D_LUT_LENGTH}'.",
                t.grid_size
            );
        }
        Ok(Self {
            interpolation: t.interpolation,
            array: Lut3DArray {
                length: t.grid_size,
                values: t.values.clone(),
            },
            direction: t.direction,
            file_out_bit_depth: t.file_output_bit_depth,
            metadata: t.metadata.clone(),
        })
    }

    /// Convert to a transform.
    pub fn to_transform(&self) -> Lut3DTransform {
        Lut3DTransform {
            direction: self.direction,
            grid_size: self.array.length(),
            values: self.array.values.clone(),
            interpolation: self.interpolation,
            file_output_bit_depth: self.file_out_bit_depth,
            metadata: self.metadata.clone(),
        }
    }

    pub fn interpolation(&self) -> Interpolation {
        self.interpolation
    }

    /// The interpolation actually used.
    pub fn concrete_interpolation(&self) -> Interpolation {
        Self::get_concrete_interpolation(self.interpolation)
    }

    /// Best and tetrahedral map to tetrahedral, everything else to linear
    /// (OCIO v2 implements nearest as trilinear; invalid interpolations make
    /// `validate` fail).
    pub fn get_concrete_interpolation(interp: Interpolation) -> Interpolation {
        match interp {
            Interpolation::Best | Interpolation::Tetrahedral => Interpolation::Tetrahedral,
            _ => Interpolation::Linear,
        }
    }

    pub fn set_interpolation(&mut self, interpolation: Interpolation) {
        self.interpolation = interpolation;
    }

    /// Best, tetrahedral, default, linear and nearest are valid.
    pub fn is_valid_interpolation(interpolation: Interpolation) -> bool {
        matches!(
            interpolation,
            Interpolation::Best
                | Interpolation::Tetrahedral
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

    pub fn array(&self) -> &Lut3DArray {
        &self.array
    }

    pub fn array_mut(&mut self) -> &mut Lut3DArray {
        &mut self.array
    }

    /// Grid size.
    pub fn grid_size(&self) -> usize {
        self.array.length()
    }

    /// Set the values from a red-fastest array.
    pub fn set_array_from_red_fastest_order(&mut self, lut: &[f32]) -> Result<()> {
        let n = self.array.length();
        if n * n * n * 3 != lut.len() {
            crate::bail!(
                "Lut3D length '{n} * {n} * {n} * 3' does not match the vector size '{}'.",
                lut.len()
            );
        }
        if self.array.values.len() != n * n * n * 3 {
            self.array.values.resize(n * n * n * 3, 0.0);
        }
        for b in 0..n {
            for g in 0..n {
                for r in 0..n {
                    let blue_fast = 3 * ((r * n + g) * n + b);
                    let red_fast = 3 * ((b * n + g) * n + r);
                    self.array.values[blue_fast..blue_fast + 3]
                        .copy_from_slice(&lut[red_fast..red_fast + 3]);
                }
            }
        }
        Ok(())
    }

    /// Structural checks needed before evaluating the LUT.
    ///
    /// Note: unlike OCIO, a grid size of 1 is rejected (the inverse
    /// evaluation would not terminate).
    pub fn check_structure(&self) -> Result<()> {
        if let Err(e) = self.array.validate() {
            crate::bail!("Lut3D content array issue: {}", e.message());
        }
        if self.array.length() > MAX_3D_LUT_LENGTH {
            crate::bail!("Lut3D length: {} is not supported. ", self.array.length());
        }
        if self.array.length() < 2 {
            crate::bail!("Lut3D grid size needs to be at least 2.");
        }
        Ok(())
    }

    /// Validate the parameters.
    pub fn validate(&self) -> Result<()> {
        if !Self::is_valid_interpolation(self.interpolation) {
            crate::bail!(
                "Lut3D does not support interpolation algorithm: {}.",
                self.interpolation.as_str()
            );
        }
        self.check_structure()
    }

    /// A 3D LUT clamps to its domain, so it is never a no-op.
    pub fn is_no_op(&self) -> bool {
        false
    }

    /// A 3D LUT clamps to its domain, so it is never an identity.
    pub fn is_identity(&self) -> bool {
        false
    }

    /// Always true.
    pub fn has_channel_crosstalk(&self) -> bool {
        true
    }

    /// Replacement of an identity 3D LUT (a clamp to `[0, 1]`).
    pub fn identity_replacement(&self) -> IdentityReplacement {
        IdentityReplacement::Clamp { min: 0.0, max: 1.0 }
    }

    /// Copy with the direction flipped.
    pub fn inverse(&self) -> Lut3DOpData {
        let mut inv = self.clone();
        inv.direction = self.direction.inverse();
        inv
    }

    /// True if `other` is the inverse of `self` (same array, opposite
    /// direction).
    pub fn is_inverse(&self, other: &Lut3DOpData) -> bool {
        self.direction != other.direction && self.array == other.array
    }

    pub fn file_output_bit_depth(&self) -> BitDepth {
        self.file_out_bit_depth
    }

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

    /// Set the `name` metadata attribute.
    pub fn set_name(&mut self, name: &str) {
        self.metadata.set_name(name);
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
        s.push_str(&format!(
            "{} {} {} ",
            hash_f32(self.array.values()),
            self.interpolation.as_str(),
            self.direction.as_str()
        ));
        s
    }

    /// Functional composition of two 3D LUTs (port of
    /// `Lut3DOpData::Compose`). The result is at least as big as either LUT.
    pub fn compose(lutc1: &Lut3DOpData, lutc2: &Lut3DOpData) -> Result<Lut3DOpData> {
        let mut lut1 = lutc1.clone();
        let mut lut2 = lutc2.clone();
        let mut restore_inverse = false;
        if lut1.direction == TransformDirection::Inverse
            && lut2.direction == TransformDirection::Inverse
        {
            // Using the fact that: inv(l2 x l1) = inv(l1) x inv(l2).
            std::mem::swap(&mut lut1, &mut lut2);
            lut1.direction = TransformDirection::Forward;
            lut2.direction = TransformDirection::Forward;
            restore_inverse = true;
        }

        let min_sz = lut2.array.length();
        let n = lut1.array.length();
        let domain_size = min_sz.max(n);
        let mut ops = OpVec::new();

        let mut result = if n >= min_sz && lut1.direction != TransformDirection::Inverse {
            // The range of the first LUT becomes the domain to interp in the
            // second. Use the original domain.
            lut1.clone()
        } else {
            // Since the 2nd LUT is more finely sampled, use its grid size and
            // interpolate through both LUTs.
            let mut r = Lut3DOpData::with_interpolation(lut1.interpolation, domain_size)?;
            r.metadata = lut1.metadata.clone();
            ops.push(Arc::new(Lut3DOp::new(lut1.clone())?));
            r
        };

        ops.push(Arc::new(Lut3DOp::new(lut2.clone())?));

        let file_out_bd = lut1.file_out_bit_depth;
        result.metadata.combine(&lut2.metadata);
        result.file_out_bit_depth = file_out_bd;

        eval_transform(&mut result.array.values, &ops);

        if restore_inverse {
            result.direction = TransformDirection::Inverse;
        }
        Ok(result)
    }
}

/// Make a forward 3D LUT (48^3) approximating the exact inverse of an inverse
/// LUT, for the fast inversion style (`OptimizationFlags::LUT_INV_FAST`).
pub fn make_fast_lut3d_from_inverse(lut: &Lut3DOpData) -> Result<Lut3DOpData> {
    if lut.direction() != TransformDirection::Inverse {
        crate::bail!("MakeFastLut3DFromInverse expects an inverse LUT");
    }
    // Note (OCIO TODO): the fast LUT limits inputs to [0,1].
    let mut new_domain = Lut3DOpData::new(FAST_INVERSE_GRID_SIZE)?;
    new_domain.set_file_output_bit_depth(lut.file_output_bit_depth());
    // Compose the new domain with the inverse LUT (exact inversion).
    Lut3DOpData::compose(&new_domain, lut)
}

// ---------------------------------------------------------------------------
// Op.

/// A 3D LUT op. The CPU renderer is built on first use.
#[derive(Clone)]
pub struct Lut3DOp {
    data: Arc<Lut3DOpData>,
    renderer: OnceLock<Arc<Lut3DRenderer>>,
}

impl fmt::Debug for Lut3DOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Lut3DOp").field("data", &self.data).finish()
    }
}

impl Lut3DOp {
    /// Create an op (the data structure is checked).
    pub fn new(data: Lut3DOpData) -> Result<Self> {
        data.check_structure()?;
        Ok(Self {
            data: Arc::new(data),
            renderer: OnceLock::new(),
        })
    }

    /// The LUT data.
    pub fn data(&self) -> &Lut3DOpData {
        &self.data
    }

    fn renderer(&self) -> &Lut3DRenderer {
        self.renderer
            .get_or_init(|| Arc::new(Lut3DRenderer::new(&self.data)))
    }
}

impl Op for Lut3DOp {
    fn name(&self) -> &'static str {
        "Lut3D"
    }

    fn apply(&self, pixels: &mut [Pixel]) {
        self.renderer().apply(pixels);
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
        format!("<Lut3D {}>", self.data.cache_id())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn combine_with(&self, next: &dyn Op, flags: OptimizationFlags) -> Option<OpVec> {
        let other = next.downcast_ref::<Lut3DOp>()?;
        if flags.contains(OptimizationFlags::PAIR_IDENTITY_LUT3D)
            && self.data.is_inverse(&other.data)
        {
            // A forward + inverse pair still clamps to the LUT domain.
            if let Some(ops) = identity_replacement_ops(self.data.identity_replacement()) {
                return Some(ops);
            }
        }
        if flags.contains(OptimizationFlags::COMP_LUT3D) {
            let composed = Lut3DOpData::compose(&self.data, &other.data).ok()?;
            let op = Lut3DOp::new(composed).ok()?;
            return Some(vec![Arc::new(op)]);
        }
        None
    }

    fn to_transform(&self) -> Option<Transform> {
        Some(Transform::Lut3D(self.data.to_transform()))
    }

    fn clone_box(&self) -> Box<dyn Op> {
        Box::new(self.clone())
    }
}

// ---------------------------------------------------------------------------
// Builders & helpers.

/// Append a 3D LUT op built from op data (port of `CreateLut3DOp`). With
/// `TransformDirection::Inverse`, the LUT direction is flipped.
pub fn create_lut3d_op_from_data(
    ops: &mut OpVec,
    lut: &Lut3DOpData,
    dir: TransformDirection,
) -> Result<()> {
    let data = match dir {
        TransformDirection::Forward => lut.clone(),
        TransformDirection::Inverse => lut.inverse(),
    };
    ops.push(Arc::new(Lut3DOp::new(data)?));
    Ok(())
}

/// Append the op of a [`Lut3DTransform`] (validated) in the requested
/// direction (port of `BuildLut3DOp`).
pub fn create_lut3d_op(
    ops: &mut OpVec,
    lut: &Lut3DTransform,
    dir: TransformDirection,
) -> Result<()> {
    let data = Lut3DOpData::from_transform(lut)?;
    data.validate()?;
    create_lut3d_op_from_data(ops, &data, dir)
}

/// Bake an op list into a 3D LUT of the given grid size by evaluating an
/// identity lattice through the ops.
pub fn bake_ops_to_lut3d(ops: &[OpRc], grid_size: usize) -> Result<Lut3DOpData> {
    let mut lut = Lut3DOpData::new(grid_size)?;
    lut.check_structure()?;
    eval_transform(&mut lut.array.values, ops);
    Ok(lut)
}

// ---------------------------------------------------------------------------
// Transform.

impl Validate for Lut3DTransform {
    fn validate(&self) -> Result<()> {
        let res = Lut3DOpData::from_transform(self).and_then(|d| d.validate());
        res.map_err(|e| Error::msg(format!("Lut3DTransform validation failed: {}", e.message())))
    }
}

impl BuildOps for Lut3DTransform {
    fn build_ops(
        &self,
        ops: &mut OpVec,
        _config: &Config,
        _context: &Context,
        dir: TransformDirection,
    ) -> Result<()> {
        create_lut3d_op(ops, self, dir)
    }
}

impl fmt::Display for Lut3DTransform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "<Lut3DTransform direction={}, fileoutdepth={}, interpolation={}, ",
            self.direction.as_str(),
            self.file_output_bit_depth.as_str(),
            self.interpolation.as_str()
        )?;
        let l = self.grid_size;
        write!(f, "gridSize={l}, ")?;
        if l > 0 {
            let n = (l * l * l * 3).min(self.values.len());
            let (mn, mx) = rgb_min_max(&self.values[..n]);
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
