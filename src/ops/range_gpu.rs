//! GPU renderer of the range op (port of `RangeOpGPU.cpp`).

use crate::error::Result;
use crate::gpu::{GpuShaderCreator, GpuShaderText};
use crate::nl;
use crate::ops::range::{RangeOp, RangeOpData};

/// Add the shader program of a (forward) range (port of
/// `GetRangeGPUShaderProgram`).
pub(crate) fn range_shader_program(
    shader_creator: &mut dyn GpuShaderCreator,
    range: &RangeOpData,
) -> Result<()> {
    let mut ss = GpuShaderText::new(shader_creator.language());
    ss.indent();

    nl!(ss, "");
    nl!(ss, "// Add Range processing");
    nl!(ss, "");
    nl!(ss, "{");
    ss.indent();

    let pix = shader_creator.pixel_name().to_string();
    let pixrgb = format!("{pix}.rgb");

    if range.scales() {
        let scale = range.scale();
        let offset = range.offset();
        nl!(
            ss,
            pixrgb,
            " = ",
            pixrgb,
            " * ",
            ss.float3_const(scale, scale, scale),
            " + ",
            ss.float3_const(offset, offset, offset),
            ";"
        );
    }

    if !range.min_is_empty() {
        let lower_bound = range.min_out;
        nl!(
            ss,
            pixrgb,
            " = ",
            "max(",
            ss.float3_const(lower_bound, lower_bound, lower_bound),
            ", ",
            pixrgb,
            ");"
        );
    }

    if !range.max_is_empty() {
        let upper_bound = range.max_out;
        nl!(
            ss,
            pixrgb,
            " = ",
            "min(",
            ss.float3_const(upper_bound, upper_bound, upper_bound),
            ", ",
            pixrgb,
            ");"
        );
    }

    ss.dedent();
    nl!(ss, "}");

    ss.dedent();
    shader_creator.add_to_function_shader_code(ss.as_str());
    Ok(())
}

/// Port of `RangeOp::extractGpuShaderInfo` (the op data is always forward
/// once the op is created).
pub(crate) fn extract(op: &RangeOp, shader_creator: &mut dyn GpuShaderCreator) -> Result<()> {
    range_shader_program(shader_creator, op.data())
}
