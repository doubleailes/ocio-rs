//! Dispatch of the GPU shader generation to the op renderers (the
//! `extractGpuShaderInfo` / `supportedByLegacyShader` methods of the OCIO
//! ops).

use std::any::Any;

use crate::config::AllocationNoOp;
use crate::error::{Error, Result};
use crate::ops::cdl::CdlOp;
use crate::ops::exponent::ExponentOp;
use crate::ops::exposure_contrast::ExposureContrastOp;
use crate::ops::fixed_function::FixedFunctionOp;
use crate::ops::gamma::GammaOp;
use crate::ops::grading_hue_curve::GradingHueCurveOp;
use crate::ops::grading_primary::GradingPrimaryOp;
use crate::ops::grading_rgb_curve::GradingRgbCurveOp;
use crate::ops::grading_tone::GradingToneOp;
use crate::ops::log::LogOp;
use crate::ops::lut1d::Lut1DOp;
use crate::ops::lut3d::Lut3DOp;
use crate::ops::matrix::MatrixOp;
use crate::ops::noop::{MarkerNoOp, MetadataNoOp};
use crate::ops::range::RangeOp;
use crate::ops::{self, Op};

use super::GpuShaderCreator;

/// Add the shader program of an op of this crate to `shader_creator` (the
/// default implementation of [`Op::extract_gpu_shader_info`]). Fails for the
/// op types unknown to the crate.
pub fn default_extract_gpu_shader_info(
    op: &dyn Any,
    shader_creator: &mut dyn GpuShaderCreator,
) -> Result<()> {
    if let Some(op) = op.downcast_ref::<MatrixOp>() {
        return ops::matrix_gpu::extract(op, shader_creator);
    }
    if let Some(op) = op.downcast_ref::<RangeOp>() {
        return ops::range_gpu::extract(op, shader_creator);
    }
    if let Some(op) = op.downcast_ref::<ExponentOp>() {
        return ops::exponent_gpu::extract(op, shader_creator);
    }
    if let Some(op) = op.downcast_ref::<GammaOp>() {
        return ops::gamma_gpu::extract(op, shader_creator);
    }
    if let Some(op) = op.downcast_ref::<LogOp>() {
        return ops::log_gpu::extract(op, shader_creator);
    }
    if let Some(op) = op.downcast_ref::<CdlOp>() {
        return ops::cdl_gpu::extract(op, shader_creator);
    }
    if let Some(op) = op.downcast_ref::<ExposureContrastOp>() {
        return ops::exposure_contrast_gpu::extract(op, shader_creator);
    }
    if let Some(op) = op.downcast_ref::<FixedFunctionOp>() {
        return ops::fixed_function_gpu::extract(op, shader_creator);
    }
    if let Some(op) = op.downcast_ref::<Lut1DOp>() {
        return ops::lut1d_gpu::extract(op, shader_creator);
    }
    if let Some(op) = op.downcast_ref::<Lut3DOp>() {
        return ops::lut3d_gpu::extract(op, shader_creator);
    }
    if let Some(op) = op.downcast_ref::<GradingPrimaryOp>() {
        return ops::grading_primary_gpu::extract(op, shader_creator);
    }
    if let Some(op) = op.downcast_ref::<GradingRgbCurveOp>() {
        return ops::grading_rgb_curve_gpu::extract(op, shader_creator);
    }
    if let Some(op) = op.downcast_ref::<GradingToneOp>() {
        return ops::grading_tone_gpu::extract(op, shader_creator);
    }
    if let Some(op) = op.downcast_ref::<GradingHueCurveOp>() {
        return ops::grading_hue_curve_gpu::extract(op, shader_creator);
    }
    if op.is::<MarkerNoOp>() || op.is::<MetadataNoOp>() || op.is::<AllocationNoOp>() {
        // The no-ops do not add anything to the shader program.
        return Ok(());
    }
    Err(Error::msg(
        "The op does not support the GPU shader generation.",
    ))
}

/// Add the shader program implementing `op` to `shader_creator` (port of
/// `Op::extractGpuShaderInfo`).
pub fn extract_op_gpu_shader_info(
    op: &dyn Op,
    shader_creator: &mut dyn GpuShaderCreator,
) -> Result<()> {
    op.extract_gpu_shader_info(shader_creator)
}

/// True if the op can be translated to shader text by the legacy GPU
/// processor, otherwise it is baked into its 3D LUT (port of
/// `Op::supportedByLegacyShader`: only the 1D and 3D LUTs are not).
pub fn supported_by_legacy_shader(op: &dyn Op) -> bool {
    let any = op.as_any();
    !(any.is::<Lut1DOp>() || any.is::<Lut3DOp>())
}
