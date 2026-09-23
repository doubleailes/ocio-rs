//! GPU renderer of the matrix op (port of `MatrixOpGPU.cpp`).

use crate::error::Result;
use crate::gpu::{GpuShaderCreator, GpuShaderText};
use crate::nl;
use crate::ops::matrix::{MatrixOp, MatrixOpData};

/// Add the shader program of a (forward) matrix (port of
/// `GetMatrixGPUShaderProgram`).
pub(crate) fn matrix_shader_program(
    shader_creator: &mut dyn GpuShaderCreator,
    matrix: &MatrixOpData,
) -> Result<()> {
    let mut ss = GpuShaderText::new(shader_creator.language());
    ss.indent();

    nl!(ss, "");
    nl!(ss, "// Add Matrix processing");
    nl!(ss, "");

    nl!(ss, "{");
    ss.indent();

    let values = &matrix.matrix;
    let offs = &matrix.offsets;

    let pxl = shader_creator.pixel_name().to_string();

    nl!(
        ss,
        ss.float4_decl("res")?,
        " = ",
        ss.float4_const(
            format!("{pxl}.rgb.r"),
            format!("{pxl}.rgb.g"),
            format!("{pxl}.rgb.b"),
            format!("{pxl}.a")
        ),
        ";"
    );

    if !matrix.is_unity_diagonal() {
        if matrix.is_diagonal() {
            nl!(
                ss,
                "res = ",
                ss.float4_const(
                    values[0] as f32,
                    values[5] as f32,
                    values[10] as f32,
                    values[15] as f32
                ),
                " * res;"
            );
        } else {
            // NOTE: The in-place matrix computation is not supported by OSL
            // so, a temporary variable is needed.
            nl!(ss, ss.float4_decl("tmp")?, " = res;");
            nl!(ss, "res = ", ss.mat4f_mul(&values[..], "tmp")?, ";");
        }
    }

    if matrix.has_offsets() {
        nl!(
            ss,
            "res = ",
            ss.float4_const(
                offs[0] as f32,
                offs[1] as f32,
                offs[2] as f32,
                offs[3] as f32
            ),
            " + res;"
        );
    }

    nl!(
        ss,
        pxl,
        ".rgb = ",
        ss.float3_const("res.x", "res.y", "res.z"),
        ";"
    );
    nl!(ss, pxl, ".a = res.w;");

    ss.dedent();
    nl!(ss, "}");

    shader_creator.add_to_function_shader_code(ss.as_str());
    Ok(())
}

/// Port of `MatrixOffsetOp::extractGpuShaderInfo` (the op data is always
/// forward once the op is created).
pub(crate) fn extract(op: &MatrixOp, shader_creator: &mut dyn GpuShaderCreator) -> Result<()> {
    matrix_shader_program(shader_creator, op.data())
}
