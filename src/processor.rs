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
        // Remove the markers (OCIO only removes the no-op types here: ops
        // that are identities, e.g. an identity matrix, are kept until the
        // processor is optimized).
        p.ops.retain(|o| !is_no_op_type(o));
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

    /// Cache identifier: a hash of the op cache ids. As in OCIO, it is
    /// computed even for a no-op processor, so that different lists of
    /// identity ops do not share the same id (the config processor cache
    /// reuses a processor having the same id).
    pub fn cache_id(&self) -> String {
        // Note: an empty op list also gets a UUID, as in OCIO.
        crate::hash_utils::cache_id_hash_uuid(ops::ops_cache_id(&self.ops).as_bytes())
    }

    /// Identifier of the ops and of the GPU allocations used by the legacy
    /// GPU processor (the allocations are not part of
    /// [`cache_id`](Self::cache_id), as in OCIO).
    pub(crate) fn legacy_gpu_cache_id(&self) -> String {
        let ops: OpVec = self
            .legacy_ops
            .iter()
            .filter(|o| {
                !o.is_no_op_type() || o.downcast_ref::<crate::config::AllocationNoOp>().is_some()
            })
            .cloned()
            .collect();
        ops::ops_cache_id(&ops)
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
        let (ops, ops_replaced) = optimize_ops_impl(&self.ops, flags);
        p.ops = ops;
        if ops_replaced {
            // As in OCIO (see `optimize_ops_impl`).
            p.format_metadata = FormatMetadata::default();
        }
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
            bit_depth_luts: None,
        }
    }

    /// CPU processor with bit depths. Images are converted according to their
    /// own bit depth in `apply`; when they have the processor bit depths, the
    /// first / last 1D LUTs are rendered as OCIO does for these bit depths
    /// (look-up of the input code values, rounding of the scaled output).
    pub fn optimized_cpu_processor_with_bit_depths(
        &self,
        input: BitDepth,
        output: BitDepth,
        flags: OptimizationFlags,
    ) -> CpuProcessor {
        let mut ops = optimize_ops(&self.ops, flags);
        optimize_for_bit_depth(&mut ops, input, output, flags);
        let bit_depth_luts = BitDepthLuts::new(&ops, input, output).map(Arc::new);
        CpuProcessor {
            ops,
            input_bit_depth: input,
            output_bit_depth: output,
            bit_depth_luts,
        }
    }
}

/// True for the ops that only carry information (port of the
/// `OpData::NoOpType` test): they are always removed.
fn is_no_op_type(op: &OpRc) -> bool {
    op.is_no_op_type()
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
    optimize_ops_impl(ops, flags).0
}

/// [`optimize_ops`], also returning true when an op was replaced by simpler
/// ones (`ReplaceOps`). OCIO then rebuilds the op list, which drops its
/// format metadata (e.g. the CLF `ProcessList` description and id).
fn optimize_ops_impl(ops: &[OpRc], flags: OptimizationFlags) -> (OpVec, bool) {
    let mut ops_replaced = false;
    // RemoveNoOpTypes.
    let mut v: OpVec = ops.iter().filter(|o| !is_no_op_type(o)).cloned().collect();

    if flags.contains(OptimizationFlags::NO_DYNAMIC_PROPERTIES) {
        v = v
            .into_iter()
            .map(|o| o.make_non_dynamic().unwrap_or(o))
            .collect();
    }

    if flags == OptimizationFlags::NONE {
        return (v, false);
    }

    // `Op::simplify` handles both `ReplaceOps` (gated by `SIMPLIFY_OPS`) and
    // `ReplaceIdentityOps` (gated by `IDENTITY` / `IDENTITY_GAMMA`): the flags
    // are split to run them as two steps, in the OCIO order.
    let identity_mask = OptimizationFlags::IDENTITY.0 | OptimizationFlags::IDENTITY_GAMMA.0;
    let replace_flags = OptimizationFlags(flags.0 & !identity_mask);
    let identity_flags = OptimizationFlags(flags.0 & !OptimizationFlags::SIMPLIFY_OPS.0);

    // `Op::combine_with` handles both the removal of inverse pairs (gated by
    // the `PAIR_IDENTITY_*` flags) and the composition of ops (gated by the
    // `COMP_*` flags). OCIO runs them as two separate steps of each pass
    // (`RemoveInverseOps` over the whole list, then `CombineOps` on the first
    // combinable pair only), so the flags are split to reproduce its order.
    let pair_flags = OptimizationFlags(flags.0 & PAIR_IDENTITY_MASK);
    let comp_flags = OptimizationFlags(flags.0 & !PAIR_IDENTITY_MASK);

    // Same pass structure as `OpRcPtrVec::optimize`.
    for _pass in 1..=MAX_OPTIMIZATION_PASSES {
        // RemoveNoOps.
        let mut count = 0;
        if flags.contains(OptimizationFlags::IDENTITY) {
            let n = v.len();
            v.retain(|o| !o.is_identity() && !o.is_no_op());
            count += n - v.len();
        }

        // ReplaceOps, then ReplaceIdentityOps.
        let replaced = simplify_ops(&mut v, replace_flags);
        ops_replaced |= replaced > 0;
        count += replaced;
        count += simplify_ops(&mut v, identity_flags);
        count += ops::lut1d::replace_identity_luts(&mut v, flags);

        // RemoveInverseOps: the processed part of the list is used as a stack
        // so that nested pairs (A, B, B', A') are all removed in one pass.
        if pair_flags.0 != 0 {
            let mut out = OpVec::with_capacity(v.len());
            for op in v.drain(..) {
                let repl = out
                    .last()
                    .and_then(|last: &OpRc| last.combine_with(op.as_ref(), pair_flags));
                match repl {
                    Some(repl) => {
                        out.pop();
                        out.extend(repl.into_iter().filter(|o| !o.is_no_op()));
                        count += 1;
                    }
                    None => out.push(op),
                }
            }
            v = out;
        }

        // CombineOps: combine the first combinable pair only.
        let mut i = 0;
        while i + 1 < v.len() {
            if let Some(repl) = v[i].combine_with(v[i + 1].as_ref(), comp_flags) {
                let repl: OpVec = repl.into_iter().filter(|o| !o.is_no_op()).collect();
                v.splice(i..i + 2, repl);
                count += 1;
                break;
            }
            i += 1;
        }

        if count == 0 {
            // No optimization progress was made: replace the inverse LUTs by
            // fast forward approximations (if requested) and try again.
            if !matches!(ops::lut1d::replace_inverse_luts(&mut v, flags), Ok(n) if n > 0) {
                break;
            }
        }
    }
    (v, ops_replaced)
}

/// Replace the ops by their simpler replacement ([`Op::simplify`]); returns
/// the number of replaced ops.
fn simplify_ops(v: &mut OpVec, flags: OptimizationFlags) -> usize {
    let mut count = 0;
    let mut j = 0;
    while j < v.len() {
        if let Some(repl) = v[j].simplify(flags) {
            let repl: OpVec = repl.into_iter().filter(|o| !o.is_no_op()).collect();
            let n = repl.len();
            v.splice(j..j + 1, repl);
            count += 1;
            j += n;
        } else {
            j += 1;
        }
    }
    count
}

/// Maximum number of optimization passes (as in OCIO).
const MAX_OPTIMIZATION_PASSES: usize = 80;

/// All the `PAIR_IDENTITY_*` optimization flags.
const PAIR_IDENTITY_MASK: u32 = OptimizationFlags::PAIR_IDENTITY_CDL.0
    | OptimizationFlags::PAIR_IDENTITY_EXPOSURE_CONTRAST.0
    | OptimizationFlags::PAIR_IDENTITY_FIXED_FUNCTION.0
    | OptimizationFlags::PAIR_IDENTITY_GAMMA.0
    | OptimizationFlags::PAIR_IDENTITY_LUT1D.0
    | OptimizationFlags::PAIR_IDENTITY_LUT3D.0
    | OptimizationFlags::PAIR_IDENTITY_LOG.0
    | OptimizationFlags::PAIR_IDENTITY_GRADING.0;

/// A processor for CPU evaluation.
#[derive(Debug, Clone)]
pub struct CpuProcessor {
    ops: OpVec,
    input_bit_depth: BitDepth,
    output_bit_depth: BitDepth,
    /// Bit-depth specific renderers of the first / last 1D LUT (see
    /// [`BitDepthLuts`]), used when the images have the processor bit depths.
    bit_depth_luts: Option<Arc<BitDepthLuts>>,
}

/// Port of the bit-depth handling of `CreateCPUEngine`: when the first op is
/// a 1D LUT and the input is an integer or half bit depth, OCIO replaces the
/// interpolation by a look-up of the input code values; when the last op (of
/// several) is a 1D LUT and the output is an integer bit depth, the LUT
/// values are scaled to the output range before the interpolation and the
/// result is rounded. Only forward LUTs are handled this way (the inverse
/// ones use the generic conversions, which give the same result up to the
/// rounding).
#[derive(Debug)]
struct BitDepthLuts {
    /// Look-up tables (R, G, B) indexed by the input code value (or the half
    /// float bits), replacing `ops[0]`, and the hue adjust flag.
    input: Option<([Vec<f32>; 3], bool)>,
    /// Scaled renderer replacing the last op, and the output maximum value.
    output: Option<(ops::lut1d::cpu::ForwardRenderer, f32)>,
}

impl BitDepthLuts {
    fn new(ops: &[OpRc], input: BitDepth, output: BitDepth) -> Option<Self> {
        let lut_at = |i: usize| {
            ops.get(i)
                .and_then(|o| o.downcast_ref::<ops::lut1d::Lut1DOp>())
                .map(|l| l.data())
                .filter(|d| d.direction() == TransformDirection::Forward)
        };
        let input_luts = if input != BitDepth::F32 {
            lut_at(0).and_then(|lut| {
                let hue_adjust = lut.hue_adjust() != crate::types::Lut1DHueAdjust::None;
                Self::lookup_tables(lut, input)
                    .ok()
                    .map(|t| (t, hue_adjust))
            })
        } else {
            None
        };
        let output_lut = if ops.len() > 1 && !output.is_float() {
            lut_at(ops.len() - 1).map(|lut| {
                let out_max = output.max_value() as f32;
                (
                    ops::lut1d::cpu::ForwardRenderer::with_out_scale(lut, out_max),
                    out_max,
                )
            })
        } else {
            None
        };
        if input_luts.is_none() && output_lut.is_none() {
            return None;
        }
        Some(Self {
            input: input_luts,
            output: output_lut,
        })
    }

    /// The LUT values for a look-up at `input` bit depth (the LUT is
    /// resampled when its domain does not allow a look-up).
    fn lookup_tables(lut: &ops::lut1d::Lut1DOpData, input: BitDepth) -> Result<[Vec<f32>; 3]> {
        use ops::lut1d::{ComposeMethod, Lut1DOpData};
        let resampled;
        let lut = if lut.may_lookup(input) {
            lut
        } else {
            let domain = Lut1DOpData::make_lookup_domain(input)?;
            resampled = Lut1DOpData::compose(&domain, lut, ComposeMethod::ResampleNo)?;
            &resampled
        };
        let values = lut.array().values();
        let dim = lut.array().length();
        let make = |c: usize| {
            (0..dim)
                .map(|i| sanitize_float(values[i * 3 + c]))
                .collect::<Vec<f32>>()
        };
        Ok([make(0), make(1), make(2)])
    }

    fn apply(&self, cpu: &CpuProcessor, pixels: &mut [Pixel]) {
        let mut ops = &cpu.ops[..];
        if let Some((luts, hue_adjust)) = &self.input {
            let half = cpu.input_bit_depth == BitDepth::F16;
            let max = cpu.input_bit_depth.max_value() as f32;
            for p in pixels.iter_mut() {
                let mut codes = [0usize; 3];
                let mut rgb2 = [0.0f32; 3];
                for c in 0..3 {
                    // Recover the code value of the (exactly converted) input.
                    codes[c] = if half {
                        half::f16::from_f32(p[c]).to_bits() as usize
                    } else {
                        (p[c] * max).round() as usize
                    };
                    // Note: The 10, 12 and 14 bit images may hold values
                    // above the maximum code value. OCIO reads outside of
                    // its table for them; clamp to the last entry, as the
                    // renderer of the 32f values does.
                    let lut = &luts[c];
                    rgb2[c] = lut[codes[c].min(lut.len() - 1)];
                }
                if *hue_adjust {
                    // The hue is computed from the input values (the code
                    // values for integer inputs), as in OCIO.
                    let rgb = if half {
                        [p[0], p[1], p[2]]
                    } else {
                        [codes[0] as f32, codes[1] as f32, codes[2] as f32]
                    };
                    ops::lut1d::cpu::hue_restore(&rgb, &mut rgb2);
                }
                p[..3].copy_from_slice(&rgb2);
            }
            ops = &ops[1..];
        }
        if let Some((renderer, out_max)) = &self.output {
            let n = ops.len().saturating_sub(1);
            for chunk in pixels.chunks_mut(CHUNK) {
                ops::apply_ops(&ops[..n], chunk);
                renderer.apply(chunk);
                for p in chunk.iter_mut() {
                    p[3] *= *out_max;
                    for v in p.iter_mut() {
                        // `Converter::CastValue` (the result is converted
                        // back exactly by the image writer).
                        let q = *v + 0.5;
                        let q = if q.is_nan() || q < 0.0 {
                            0.0
                        } else if q > *out_max {
                            *out_max
                        } else {
                            q.floor()
                        };
                        *v = q / *out_max;
                    }
                }
            }
        } else {
            for chunk in pixels.chunks_mut(CHUNK) {
                ops::apply_ops(ops, chunk);
            }
        }
    }
}

/// Port of `SanitizeFloat`: infinities become +/-FLT_MAX and NaNs 0.
fn sanitize_float(f: f32) -> f32 {
    if f == f32::INFINITY {
        f32::MAX
    } else if f == f32::NEG_INFINITY {
        -f32::MAX
    } else if f.is_nan() {
        0.0
    } else {
        f
    }
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
            bit_depth_luts: None,
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
        // Note: an empty op list also gets a UUID, as in OCIO.
        crate::hash_utils::cache_id_hash_uuid(ops::ops_cache_id(&self.ops).as_bytes())
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
        let luts = self.bit_depth_luts_for(img.bit_depth(), img.bit_depth());
        let w = img.width();
        let mut row = vec![[0.0f32; 4]; w];
        for y in 0..img.height() {
            img.read_row(y, &mut row);
            match luts {
                Some(l) => l.apply(self, &mut row),
                None => self.apply_pixels(&mut row),
            }
            img.write_row(y, &row);
        }
    }

    /// The bit-depth specific LUT renderers, if the images have the
    /// processor bit depths.
    fn bit_depth_luts_for(&self, input: BitDepth, output: BitDepth) -> Option<&BitDepthLuts> {
        if input == self.input_bit_depth && output == self.output_bit_depth {
            self.bit_depth_luts.as_deref()
        } else {
            None
        }
    }

    /// Apply from `src` into `dst` (images must have the same dimensions;
    /// bit depths may differ).
    pub fn apply_src_dst(&self, src: &dyn ImageDesc, dst: &mut dyn ImageDesc) -> Result<()> {
        if src.width() != dst.width() || src.height() != dst.height() {
            crate::bail!("Dimension mismatch between source and destination images.");
        }
        let luts = self.bit_depth_luts_for(src.bit_depth(), dst.bit_depth());
        let w = src.width();
        let mut row = vec![[0.0f32; 4]; w];
        for y in 0..src.height() {
            src.read_row(y, &mut row);
            match luts {
                Some(l) => l.apply(self, &mut row),
                None => self.apply_pixels(&mut row),
            }
            dst.write_row(y, &row);
        }
        Ok(())
    }
}
