//! The GPU processor (port of `GPUProcessor.cpp` and of the GPU parts of
//! `Processor.cpp` / `NoOps.cpp`).

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};

use crate::config::logging::{log_debug, log_warning};
use crate::config::AllocationNoOp;
use crate::error::{Error, Result};
use crate::nl;
use crate::ops::allocation::{create_allocation_ops, AllocationData};
use crate::ops::lut3d::{
    create_lut3d_op_from_data, generate_identity_lut3d, Lut3DOpData, Lut3DOrder,
};
use crate::ops::{self, OpRc, OpVec};
use crate::processor::optimize_ops;
use crate::types::{
    DynamicPropertyType, GpuLanguage, OptimizationFlags, TransformDirection,
    OCIO_OPTIMIZATION_FLAGS_ENVVAR,
};

use super::op_gpu::{extract_op_gpu_shader_info, supported_by_legacy_shader};
use super::shader_text::GpuShaderText;
use super::{cache_id_hash, GpuShaderCreator};

/// A processor generating GPU shader programs (port of `GPUProcessor`).
#[derive(Debug, Clone)]
pub struct GpuProcessor {
    ops: OpVec,
    is_no_op: bool,
    has_channel_crosstalk: bool,
    cache_id: String,
}

fn write_shader_header(shader_creator: &mut dyn GpuShaderCreator) -> Result<()> {
    let fcn_name = shader_creator.function_name().to_string();
    let pixel_name = shader_creator.pixel_name().to_string();

    let mut ss = GpuShaderText::new(shader_creator.language());

    nl!(ss);
    nl!(ss, "// Declaration of the OCIO shader function");
    nl!(ss);

    if shader_creator.language() == GpuLanguage::Osl1 {
        nl!(ss, "color4 ", fcn_name, "(color4 inPixel)");
        nl!(ss, "{");
        ss.indent();
        nl!(ss, "color4 ", pixel_name, " = inPixel;");
    } else {
        nl!(
            ss,
            ss.float4_keyword(),
            " ",
            fcn_name,
            "(",
            ss.float4_keyword(),
            " inPixel)"
        );
        nl!(ss, "{");
        ss.indent();
        nl!(ss, ss.float4_decl(&pixel_name)?, " = inPixel;");
    }

    shader_creator.add_to_function_header_shader_code(ss.as_str());
    Ok(())
}

fn write_shader_footer(shader_creator: &mut dyn GpuShaderCreator) {
    let mut ss = GpuShaderText::new(shader_creator.language());

    nl!(ss);
    ss.indent();
    nl!(ss, "return ", shader_creator.pixel_name(), ";");
    ss.dedent();
    nl!(ss, "}");

    shader_creator.add_to_function_footer_shader_code(ss.as_str());
}

/// Check that each dynamic property type is used by only one op (port of
/// `OpRcPtrVec::validateDynamicProperties`, which only logs a warning).
fn validate_dynamic_properties(ops: &[OpRc]) {
    use DynamicPropertyType as T;
    let types = [
        (T::Exposure, "Exposure"),
        (T::Contrast, "Contrast"),
        (T::Gamma, "Gamma"),
        (T::GradingPrimary, "Grading primary"),
        (T::GradingRgbCurve, "Grading RGB curve"),
        (T::GradingHueCurve, "Grading hue curve"),
        (T::GradingTone, "Grading tone"),
    ];
    let mut found = [false; 7];
    for op in ops {
        for (i, (ty, name)) in types.iter().enumerate() {
            if op.dynamic_property(*ty).is_some() {
                if found[i] {
                    log_warning(&format!("{name} dynamic property can only be there once."));
                } else {
                    found[i] = true;
                }
            }
        }
    }
}

impl GpuProcessor {
    /// Build the GPU processor from (raw) ops, optimized with `flags` (port
    /// of `GPUProcessor::Impl::finalize`).
    pub fn from_ops(raw_ops: &[OpRc], flags: OptimizationFlags) -> Result<Self> {
        let finalized: OpVec = raw_ops
            .iter()
            .map(|o| o.finalize().unwrap_or_else(|| o.clone()))
            .collect();
        let ops = optimize_ops(&finalized, flags);
        validate_dynamic_properties(&ops);

        let is_no_op = ops::ops_are_no_op(&ops);
        let has_channel_crosstalk = ops.iter().any(|o| o.has_channel_crosstalk());

        let cache_id = format!(
            "GPU Processor: oFlags {} ops : {}",
            flags.0,
            ops::ops_cache_id(&ops)
        );

        Ok(Self {
            ops,
            is_no_op,
            has_channel_crosstalk,
            cache_id,
        })
    }

    /// The (optimized) ops.
    pub fn ops(&self) -> &[OpRc] {
        &self.ops
    }

    /// True if the processor does nothing.
    pub fn is_no_op(&self) -> bool {
        self.is_no_op
    }

    /// True if an output channel depends on other input channels.
    pub fn has_channel_crosstalk(&self) -> bool {
        self.has_channel_crosstalk
    }

    /// Cache identifier.
    pub fn cache_id(&self) -> &str {
        &self.cache_id
    }

    fn extract_impl(&self, shader_creator: &mut dyn GpuShaderCreator) -> Result<()> {
        // Create the shader program information.
        for op in &self.ops {
            extract_op_gpu_shader_info(op.as_ref(), shader_creator)?;
        }

        write_shader_header(shader_creator)?;
        write_shader_footer(shader_creator);

        shader_creator.finalize()
    }

    /// Extract and store the shader information implementing the color
    /// processing (port of `GPUProcessor::extractGpuShaderInfo`).
    ///
    /// Several generated shader programs could be in the same global program
    /// so a unique key, built from the shader creator and the processor
    /// cache ids, is given to [`GpuShaderCreator::begin`].
    pub fn extract_gpu_shader_info(&self, shader_creator: &mut dyn GpuShaderCreator) -> Result<()> {
        let tmp_key = format!("{}{}", shader_creator.cache_id(), self.cache_id);

        // Way too long uid for a resource name so shorten it.
        let mut key = cache_id_hash(&tmp_key);

        // Prepend a user defined uid if any.
        if !shader_creator.unique_id().is_empty() {
            key = format!("{}{}", shader_creator.unique_id(), key);
        }

        if !key.starts_with(|c: char| c.is_ascii_alphabetic()) {
            // A resource name must start with a letter.
            key = format!("k_{key}");
        }

        // A resource name only accepts alphanumeric characters.
        key.retain(|c| c.is_ascii_alphanumeric() || c == '_');

        // Extract the information to fully build the fragment shader program.
        shader_creator.begin(&key);
        let res = self.extract_impl(shader_creator);
        shader_creator.end();
        res
    }
}

/// Port of `EnvironmentOverride`: `$OCIO_OPTIMIZATION_FLAGS` overrides the
/// optimization flags (decimal, `0x` hexadecimal or `0` octal).
pub(crate) fn environment_override(flags: OptimizationFlags) -> Result<OptimizationFlags> {
    let env = std::env::var(OCIO_OPTIMIZATION_FLAGS_ENVVAR).unwrap_or_default();
    if env.is_empty() {
        return Ok(flags);
    }
    let s = env.trim_start();
    let parsed = if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u32::from_str_radix(hex, 16).ok()
    } else if s.len() > 1 && s.starts_with('0') {
        u32::from_str_radix(&s[1..], 8).ok()
    } else {
        s.parse::<u32>().ok()
    };
    parsed.map(OptimizationFlags).ok_or_else(|| {
        Error::msg(format!(
            "Illegal value for {OCIO_OPTIMIZATION_FLAGS_ENVVAR}: {env}"
        ))
    })
}

/// A cache of GPU processors by optimization flags (port of the
/// `m_gpuProcessorCache` of `Processor::Impl`). Cloning gives an empty
/// cache so that a modified copy of a processor never reuses the entries.
#[derive(Default)]
pub(crate) struct GpuProcessorCache {
    entries: Mutex<HashMap<u32, Arc<GpuProcessor>>>,
}

impl Clone for GpuProcessorCache {
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl fmt::Debug for GpuProcessorCache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("GpuProcessorCache")
    }
}

impl GpuProcessorCache {
    /// The cached processor for `flags`, created by `create` if missing.
    pub(crate) fn get_or_create(
        &self,
        flags: OptimizationFlags,
        create: impl FnOnce() -> Result<GpuProcessor>,
    ) -> Result<Arc<GpuProcessor>> {
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| Error::msg("GPU processor cache is poisoned."))?;
        if let Some(p) = entries.get(&flags.0) {
            return Ok(p.clone());
        }
        let p = Arc::new(create()?);
        entries.insert(flags.0, p.clone());
        Ok(p)
    }
}

/// Build a 3D LUT op baking `ops` (port of `Create3DLut`).
fn create_3d_lut(ops: &[OpRc], edgelen: usize) -> Result<OpVec> {
    if ops.is_empty() {
        return Ok(OpVec::new());
    }

    let num_pixels = edgelen * edgelen * edgelen;

    let mut lut = Lut3DOpData::new(edgelen)?;

    // Allocate the 3D LUT image, RGBA.
    let mut img = vec![0.0f32; num_pixels * 4];
    generate_identity_lut3d(&mut img, edgelen, 4, Lut3DOrder::FastBlue)?;

    // Apply the lattice ops to it.
    let mut pixels: Vec<ops::Pixel> = img
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| [c[0], c[1], c[2], c[3]])
        .collect();
    ops::apply_ops(ops, &mut pixels);

    // Convert the RGBA image to an RGB image.
    {
        let values = lut.array_mut().values_mut();
        for (i, p) in pixels.iter().enumerate() {
            values[3 * i] = p[0];
            values[3 * i + 1] = p[1];
            values[3 * i + 2] = p[2];
        }
    }

    let mut new_ops = OpVec::new();
    create_lut3d_op_from_data(&mut new_ops, &lut, TransformDirection::Forward)?;
    Ok(new_ops)
}

/// The GPU allocation defined by an op, if any.
fn gpu_allocation(op: &OpRc) -> Option<AllocationData> {
    op.downcast_ref::<AllocationNoOp>().map(|a| AllocationData {
        allocation: a.allocation,
        vars: a.vars.clone(),
    })
}

/// Find the minimal index range of the ops not supporting the shader text
/// generation (port of `GetGpuUnsupportedIndexRange`; the end is inclusive
/// and both indices are `-1` if all ops are supported).
fn gpu_unsupported_index_range(ops: &[OpRc]) -> (i64, i64) {
    let mut start: i64 = -1;
    let mut end: i64 = -1;

    for (i, op) in ops.iter().enumerate() {
        // We've found a gpu unsupported op. If it's the first, save it as our
        // start. Otherwise, update the end.
        if !supported_by_legacy_shader(op.as_ref()) {
            if start < 0 {
                start = i as i64;
            }
            end = i as i64;
        }
    }

    // Now that we've found a start index, walk back until we find one that
    // defines a GPU allocation (we can only upload to the GPU at a location
    // tagged with an allocation).
    while start > 0 {
        if gpu_allocation(&ops[start as usize]).is_some() {
            break;
        }
        start -= 1;
    }

    (start, end)
}

/// Partition the ops into the 3 stages of the legacy GPU pipe (port of
/// `PartitionGPUOps`): the pre-process shader text, the 3D LUT lattice and
/// the post-process shader text.
fn partition_gpu_ops(ops: &[OpRc]) -> Result<(OpVec, OpVec, OpVec)> {
    let mut pre = OpVec::new();
    let mut lattice = OpVec::new();
    let mut post = OpVec::new();

    let (start, end) = gpu_unsupported_index_range(ops);

    if start == -1 && end == -1 {
        // Write the entire shader using only shader text (3D LUT is unused).
        pre.extend(ops.iter().cloned());
        return Ok((pre, lattice, post));
    }

    // Analytical -> 3D LUT -> analytical.
    if start < 0 || start >= ops.len() as i64 {
        return Err(Error::msg(format!(
            "Invalid GpuUnsupportedIndexRange: gpuLut3DOpStartIndex: {} gpuLut3DOpEndIndex: {} cpuOps.size: {}",
            start,
            end,
            ops.len()
        )));
    }
    let start = start as usize;
    let end = end as usize;

    // Handle the analytical shader block before the start index.
    pre.extend(ops[..start].iter().cloned());

    // Get the GPU allocation at the cross-over point and create 2
    // symmetrically canceling allocation ops, where the shader text moves to
    // a nicely allocated LDR color space, and the lattice processing does the
    // inverse (making the overall operation a no-op color-wise).
    if let Some(allocation) = gpu_allocation(&ops[start]) {
        create_allocation_ops(&mut pre, &allocation, TransformDirection::Forward)?;
        create_allocation_ops(&mut lattice, &allocation, TransformDirection::Inverse)?;
    }

    // Handle the CPU lattice processing.
    lattice.extend(ops[start..=end].iter().cloned());

    // And then handle the GPU post processing.
    post.extend(ops[end + 1..].iter().cloned());

    Ok((pre, lattice, post))
}

/// The ops of the legacy GPU processor: the ops not supported by the legacy
/// shaders are baked in a 3D LUT of edge length `edgelen` (port of the ops
/// preparation of `Processor::Impl::getOptimizedLegacyGPUProcessor`).
pub(crate) fn legacy_gpu_ops(ops: &[OpRc], edgelen: u32) -> Result<OpVec> {
    let (pre, lattice, post) = partition_gpu_ops(ops)?;

    log_debug("Legacy GPU Ops: 3DLUT");
    let lattice: OpVec = lattice
        .iter()
        .map(|o| o.finalize().unwrap_or_else(|| o.clone()))
        .collect();
    let gpu_lut = create_3d_lut(&lattice, edgelen as usize)?;

    let mut gpu_ops = OpVec::new();
    gpu_ops.extend(pre);
    gpu_ops.extend(gpu_lut);
    gpu_ops.extend(post);

    Ok(gpu_ops
        .iter()
        .map(|o| o.finalize().unwrap_or_else(|| o.clone()))
        .collect())
}
