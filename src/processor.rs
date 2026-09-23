//! Processors: an optimized list of ops ready to be applied to pixels (port
//! of `Processor.cpp`, `CPUProcessor.cpp` and `OpOptimizers.cpp`).

use crate::config::Config;
use crate::context::Context;
use crate::dynamic_property::DynamicProperty;
use crate::error::Result;
use crate::format_metadata::FormatMetadata;
use crate::image_desc::ImageDesc;
use crate::ops::noop::{MarkerKind, MarkerNoOp};
use crate::ops::{self, OpRc, OpVec, Pixel};
use crate::transforms::build::build_ops;
use crate::transforms::{GroupTransform, Transform};
use crate::types::{BitDepth, DynamicPropertyType, OptimizationFlags, TransformDirection};
use std::sync::Arc;

/// Files and looks used to build a processor.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProcessorMetadata {
    files: Vec<String>,
    looks: Vec<String>,
}

impl ProcessorMetadata {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn files(&self) -> &[String] {
        &self.files
    }
    pub fn looks(&self) -> &[String] {
        &self.looks
    }
    pub fn num_files(&self) -> usize {
        self.files.len()
    }
    pub fn num_looks(&self) -> usize {
        self.looks.len()
    }
    /// Add a file (duplicates are ignored).
    pub fn add_file(&mut self, f: &str) {
        if !self.files.iter().any(|x| x == f) {
            self.files.push(f.to_string());
        }
    }
    pub fn add_look(&mut self, l: &str) {
        self.looks.push(l.to_string());
    }
}

/// A color processor.
#[derive(Debug, Clone)]
pub struct Processor {
    ops: OpVec,
    metadata: ProcessorMetadata,
    format_metadata: FormatMetadata,
    transform_metadata: Vec<FormatMetadata>,
    /// The ops including the no-ops (e.g. the GPU allocations), needed by the
    /// legacy GPU processor.
    legacy_ops: OpVec,
    /// Cache of the GPU processors by optimization flags.
    gpu_cache: crate::gpu::processor::GpuProcessorCache,
}

impl Processor {
    /// Build a processor from an op list.
    pub fn from_ops(ops: OpVec) -> Self {
        let mut p = Self {
            ops: Vec::new(),
            metadata: ProcessorMetadata::new(),
            format_metadata: FormatMetadata::default(),
            transform_metadata: Vec::new(),
            legacy_ops: OpVec::new(),
            gpu_cache: Default::default(),
        };
        p.set_ops(ops);
        p.legacy_ops = p.ops.clone();
        p
    }

    fn set_ops(&mut self, ops: OpVec) {
        for op in &ops {
            if let Some(m) = op.downcast_ref::<ops::noop::MetadataNoOp>() {
                self.format_metadata.combine(&m.metadata);
            }
            if let Some(m) = op.downcast_ref::<MarkerNoOp>() {
                match m.kind {
                    MarkerKind::File => self.metadata.add_file(&m.value),
                    MarkerKind::Look => self.metadata.add_look(&m.value),
                }
            }
        }
        let ops = ops.into_iter().map(|o| o.finalize().unwrap_or(o)).collect();
        self.ops = unify_dynamic_properties(ops);
    }

    /// Build a processor for `transform` in `dir`, using `config` and `context`
    /// to resolve color spaces, looks and files.
    pub fn from_transform(
        config: &Config,
        context: &Context,
        transform: &Transform,
        dir: TransformDirection,
    ) -> Result<Self> {
        let mut ops = OpVec::new();
        build_ops(&mut ops, config, context, transform, dir)?;
        let mut p = Self::from_ops(ops);
        if let Transform::Group(g) = transform {
            p.format_metadata.combine(&g.metadata);
            p.transform_metadata = g
                .transforms
                .iter()
                .map(|t| t.format_metadata().cloned().unwrap_or_default())
                .collect();
        }
        // Remove the markers and exact no-ops, keeping the processor lean.
        p.ops.retain(|o| !o.is_no_op());
        Ok(p)
    }

    /// The (unoptimized) ops.
    pub fn ops(&self) -> &[OpRc] {
        &self.ops
    }

    /// True if the processor does nothing.
    pub fn is_no_op(&self) -> bool {
        ops::ops_are_no_op(&self.ops)
    }

    /// True if an output channel depends on other input channels.
    pub fn has_channel_crosstalk(&self) -> bool {
        self.ops.iter().any(|o| o.has_channel_crosstalk())
    }

    /// Cache identifier.
    pub fn cache_id(&self) -> String {
        if self.is_no_op() {
            return "<NOOP>".to_string();
        }
        format!(
            "{:x}",
            md5::compute(ops::ops_cache_id(&self.ops).as_bytes())
        )
    }

    pub fn processor_metadata(&self) -> &ProcessorMetadata {
        &self.metadata
    }

    /// Metadata of the group transform the processor was built from.
    pub fn format_metadata(&self) -> &FormatMetadata {
        &self.format_metadata
    }

    /// Number of transforms of the source group transform (0 if not a group).
    pub fn num_transforms(&self) -> usize {
        self.transform_metadata.len()
    }

    pub fn transform_format_metadata(&self, index: usize) -> Option<&FormatMetadata> {
        self.transform_metadata.get(index)
    }

    /// Convert the ops back to transforms.
    pub fn create_group_transform(&self) -> GroupTransform {
        let mut g = GroupTransform::new();
        g.metadata = self.format_metadata.clone();
        for op in &self.ops {
            if let Some(t) = op.to_transform() {
                g.transforms.push(t);
            }
        }
        g
    }

    /// True if any op has a dynamic property.
    pub fn is_dynamic(&self) -> bool {
        self.ops.iter().any(|o| o.is_dynamic())
    }

    pub fn has_dynamic_property(&self, ty: DynamicPropertyType) -> bool {
        self.dynamic_property(ty).is_some()
    }

    /// The dynamic property of the given type (shared by all ops using it).
    pub fn dynamic_property(&self, ty: DynamicPropertyType) -> Option<DynamicProperty> {
        self.ops.iter().find_map(|o| o.dynamic_property(ty))
    }

    /// Processor with optimized ops.
    pub fn optimized(&self, flags: OptimizationFlags) -> Processor {
        let mut p = self.clone();
        p.ops = optimize_ops(&self.ops, flags);
        p
    }

    /// CPU processor with the default optimization.
    pub fn default_cpu_processor(&self) -> CpuProcessor {
        self.optimized_cpu_processor(OptimizationFlags::DEFAULT)
    }

    /// CPU processor with the given optimization.
    pub fn optimized_cpu_processor(&self, flags: OptimizationFlags) -> CpuProcessor {
        CpuProcessor {
            ops: optimize_ops(&self.ops, flags),
            input_bit_depth: BitDepth::F32,
            output_bit_depth: BitDepth::F32,
        }
    }

    /// CPU processor with bit depths (informational: images are converted
    /// according to their own bit depth in `apply`).
    pub fn optimized_cpu_processor_with_bit_depths(
        &self,
        input: BitDepth,
        output: BitDepth,
        flags: OptimizationFlags,
    ) -> CpuProcessor {
        let mut ops = optimize_ops(&self.ops, flags);
        optimize_for_bit_depth(&mut ops, input, output, flags);
        CpuProcessor {
            ops,
            input_bit_depth: input,
            output_bit_depth: output,
        }
    }
}

// GPU processors (port of the GPU parts of `Processor.cpp`).
impl Processor {
    fn gpu_processor(
        &self,
        gpu_ops: &[OpRc],
        flags: OptimizationFlags,
        cached: bool,
    ) -> Result<Arc<crate::gpu::GpuProcessor>> {
        let flags = crate::gpu::processor::environment_override(flags)?;
        let create = || crate::gpu::GpuProcessor::from_ops(gpu_ops, flags);
        if cached {
            self.gpu_cache.get_or_create(flags, create)
        } else {
            Ok(Arc::new(create()?))
        }
    }

    /// GPU processor with the default optimization.
    pub fn default_gpu_processor(&self) -> Result<Arc<crate::gpu::GpuProcessor>> {
        self.optimized_gpu_processor(OptimizationFlags::DEFAULT)
    }

    /// GPU processor with the given optimization (the processors are cached
    /// by optimization flags).
    pub fn optimized_gpu_processor(
        &self,
        flags: OptimizationFlags,
    ) -> Result<Arc<crate::gpu::GpuProcessor>> {
        self.gpu_processor(&self.ops, flags, true)
    }

    /// Legacy GPU processor (OCIO v1 approach): the ops not supported by the
    /// legacy shaders (the 1D and 3D LUTs and the ops in between) are baked
    /// into a single 3D LUT of edge length `edgelen`.
    pub fn optimized_legacy_gpu_processor(
        &self,
        flags: OptimizationFlags,
        edgelen: u32,
    ) -> Result<Arc<crate::gpu::GpuProcessor>> {
        // The legacy ops keep the GPU allocations; only use them if they
        // still describe the current ops.
        let legacy: OpVec = self
            .legacy_ops
            .iter()
            .filter(|o| !o.is_no_op())
            .cloned()
            .collect();
        let current: OpVec = self.ops.iter().filter(|o| !o.is_no_op()).cloned().collect();
        let raw = if ops::ops_cache_id(&legacy) == ops::ops_cache_id(&current) {
            &self.legacy_ops
        } else {
            &self.ops
        };
        let gpu_ops = crate::gpu::processor::legacy_gpu_ops(raw, edgelen)?;
        self.gpu_processor(&gpu_ops, flags, false)
    }
}

fn is_identity_range(op: &OpRc) -> bool {
    op.downcast_ref::<ops::range::RangeOp>()
        .is_some_and(|r| r.data().is_identity())
}

/// Bit-depth specific optimizations (port of `OpRcPtrVec::optimizeForBitdepth`):
/// integer inputs / outputs are already in [0, 1], so leading / trailing
/// identity clamps are useless.
pub fn optimize_for_bit_depth(
    ops: &mut OpVec,
    input: BitDepth,
    output: BitDepth,
    flags: OptimizationFlags,
) {
    if ops.is_empty() {
        return;
    }
    if !input.is_float() {
        let n = ops.iter().take_while(|o| is_identity_range(o)).count();
        ops.drain(..n);
    }
    if !output.is_float() {
        while ops.last().is_some_and(is_identity_range) {
            ops.pop();
        }
    }
    if flags.contains(OptimizationFlags::COMP_SEPARABLE_PREFIX) {
        // On error, keep the unoptimized ops (still correct).
        let mut candidate = ops.clone();
        if ops::lut1d::optimize_separable_prefix(&mut candidate, input).is_ok() {
            *ops = candidate;
        }
    }
}

/// Share one dynamic property instance per type across all ops.
fn unify_dynamic_properties(ops: OpVec) -> OpVec {
    use DynamicPropertyType as T;
    let types = [
        T::Exposure,
        T::Contrast,
        T::Gamma,
        T::GradingPrimary,
        T::GradingRgbCurve,
        T::GradingTone,
        T::GradingHueCurve,
    ];
    if !ops.iter().any(|o| o.is_dynamic()) {
        return ops;
    }
    let mut out = ops;
    for ty in types {
        let mut first: Option<DynamicProperty> = None;
        for op in out.iter_mut() {
            if let Some(p) = op.dynamic_property(ty) {
                match &first {
                    None => first = Some(p),
                    Some(f) => {
                        let mut b = op.clone_box();
                        b.replace_dynamic_property(f);
                        *op = Arc::from(b);
                    }
                }
            }
        }
    }
    out
}

/// Optimize an op list (port of the main loop of `OpRcPtrVec::optimize`).
pub fn optimize_ops(ops: &[OpRc], flags: OptimizationFlags) -> OpVec {
    let mut v: OpVec = ops.iter().filter(|o| !o.is_no_op()).cloned().collect();

    if flags.contains(OptimizationFlags::NO_DYNAMIC_PROPERTIES) {
        v = v
            .into_iter()
            .map(|o| o.make_non_dynamic().unwrap_or(o))
            .collect();
    }

    if flags == OptimizationFlags::NONE {
        return v;
    }

    let mut inverse_luts_replaced = false;
    // Limit the number of passes, as in OCIO.
    for _pass in 0..8 {
        let before = v.len();
        let mut changed = false;

        if flags.contains(OptimizationFlags::IDENTITY) {
            let n = v.len();
            v.retain(|o| !o.is_identity());
            changed |= v.len() != n;
        }

        // Replace ops by simpler ones (SIMPLIFY_OPS, identity replacements).
        let mut j = 0;
        while j < v.len() {
            if let Some(repl) = v[j].simplify(flags) {
                let repl: OpVec = repl.into_iter().filter(|o| !o.is_no_op()).collect();
                let n = repl.len();
                v.splice(j..j + 1, repl);
                changed = true;
                j += n;
            } else {
                j += 1;
            }
        }

        let mut i = 0;
        while i + 1 < v.len() {
            if let Some(repl) = v[i].combine_with(v[i + 1].as_ref(), flags) {
                let repl: OpVec = repl.into_iter().filter(|o| !o.is_no_op()).collect();
                v.splice(i..i + 2, repl);
                changed = true;
                i = i.saturating_sub(1);
            } else {
                i += 1;
            }
        }

        if flags.contains(OptimizationFlags::IDENTITY) {
            changed |= ops::lut1d::replace_identity_luts(&mut v, flags) > 0;
        }

        if !changed && v.len() == before {
            // Once nothing else can be optimized, replace the inverse LUTs by
            // fast forward approximations (as OCIO does), then try again.
            if !inverse_luts_replaced {
                inverse_luts_replaced = true;
                if matches!(ops::lut1d::replace_inverse_luts(&mut v, flags), Ok(n) if n > 0) {
                    continue;
                }
            }
            break;
        }
    }
    v
}

/// A processor for CPU evaluation.
#[derive(Debug, Clone)]
pub struct CpuProcessor {
    ops: OpVec,
    input_bit_depth: BitDepth,
    output_bit_depth: BitDepth,
}

/// Number of pixels processed per chunk.
const CHUNK: usize = 1024;

impl CpuProcessor {
    /// Build directly from ops (no optimization).
    pub fn from_ops(ops: OpVec) -> Self {
        Self {
            ops,
            input_bit_depth: BitDepth::F32,
            output_bit_depth: BitDepth::F32,
        }
    }

    pub fn ops(&self) -> &[OpRc] {
        &self.ops
    }
    pub fn is_no_op(&self) -> bool {
        self.ops.is_empty() || ops::ops_are_no_op(&self.ops)
    }
    pub fn is_identity(&self) -> bool {
        self.ops.iter().all(|o| o.is_identity())
    }
    pub fn has_channel_crosstalk(&self) -> bool {
        self.ops.iter().any(|o| o.has_channel_crosstalk())
    }
    pub fn cache_id(&self) -> String {
        if self.is_no_op() {
            return "<NOOP>".to_string();
        }
        format!(
            "{:x}",
            md5::compute(ops::ops_cache_id(&self.ops).as_bytes())
        )
    }
    pub fn input_bit_depth(&self) -> BitDepth {
        self.input_bit_depth
    }
    pub fn output_bit_depth(&self) -> BitDepth {
        self.output_bit_depth
    }
    pub fn is_dynamic(&self) -> bool {
        self.ops.iter().any(|o| o.is_dynamic())
    }
    pub fn has_dynamic_property(&self, ty: DynamicPropertyType) -> bool {
        self.dynamic_property(ty).is_some()
    }
    pub fn dynamic_property(&self, ty: DynamicPropertyType) -> Option<DynamicProperty> {
        self.ops.iter().find_map(|o| o.dynamic_property(ty))
    }

    /// Apply to RGBA pixels in place.
    pub fn apply_pixels(&self, pixels: &mut [Pixel]) {
        for chunk in pixels.chunks_mut(CHUNK) {
            ops::apply_ops(&self.ops, chunk);
        }
    }

    /// Apply to one RGB pixel (alpha = 1).
    pub fn apply_rgb(&self, rgb: &mut [f32; 3]) {
        let mut p = [[rgb[0], rgb[1], rgb[2], 1.0]];
        ops::apply_ops(&self.ops, &mut p);
        rgb.copy_from_slice(&p[0][..3]);
    }

    /// Apply to one RGBA pixel.
    pub fn apply_rgba(&self, rgba: &mut [f32; 4]) {
        let mut p = [*rgba];
        ops::apply_ops(&self.ops, &mut p);
        *rgba = p[0];
    }

    /// Apply to a packed RGB `f32` buffer (length multiple of 3).
    pub fn apply_rgb_slice(&self, data: &mut [f32]) {
        let mut buf = vec![[0.0f32; 4]; CHUNK];
        for chunk in data.chunks_mut(3 * CHUNK) {
            let n = chunk.len() / 3;
            for i in 0..n {
                buf[i] = [chunk[3 * i], chunk[3 * i + 1], chunk[3 * i + 2], 1.0];
            }
            ops::apply_ops(&self.ops, &mut buf[..n]);
            for i in 0..n {
                chunk[3 * i..3 * i + 3].copy_from_slice(&buf[i][..3]);
            }
        }
    }

    /// Apply to a packed RGBA `f32` buffer (length multiple of 4).
    pub fn apply_rgba_slice(&self, data: &mut [f32]) {
        let mut buf = vec![[0.0f32; 4]; CHUNK];
        for chunk in data.chunks_mut(4 * CHUNK) {
            let n = chunk.len() / 4;
            for i in 0..n {
                buf[i].copy_from_slice(&chunk[4 * i..4 * i + 4]);
            }
            ops::apply_ops(&self.ops, &mut buf[..n]);
            for i in 0..n {
                chunk[4 * i..4 * i + 4].copy_from_slice(&buf[i]);
            }
        }
    }

    /// Apply in place to an image.
    pub fn apply(&self, img: &mut dyn ImageDesc) {
        let w = img.width();
        let mut row = vec![[0.0f32; 4]; w];
        for y in 0..img.height() {
            img.read_row(y, &mut row);
            self.apply_pixels(&mut row);
            img.write_row(y, &row);
        }
    }

    /// Apply from `src` into `dst` (images must have the same dimensions;
    /// bit depths may differ).
    pub fn apply_src_dst(&self, src: &dyn ImageDesc, dst: &mut dyn ImageDesc) -> Result<()> {
        if src.width() != dst.width() || src.height() != dst.height() {
            crate::bail!("Dimension mismatch between source and destination images.");
        }
        let w = src.width();
        let mut row = vec![[0.0f32; 4]; w];
        for y in 0..src.height() {
            src.read_row(y, &mut row);
            self.apply_pixels(&mut row);
            dst.write_row(y, &row);
        }
        Ok(())
    }
}
