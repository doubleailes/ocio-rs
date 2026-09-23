//! GPU renderer of the exponent op (port of `ExponentOp::extractGpuShaderInfo`).

use crate::error::Result;
use crate::gpu::{GpuShaderCreator, GpuShaderText};
use crate::nl;
use crate::ops::exponent::ExponentOp;

/// Port of `ExponentOp::extractGpuShaderInfo`.
pub(crate) fn extract(op: &ExponentOp, shader_creator: &mut dyn GpuShaderCreator) -> Result<()> {
    let mut ss = GpuShaderText::new(shader_creator.language());
    ss.indent();

    nl!(ss, "");
    nl!(ss, "// Add an Exponent processing");
    nl!(ss, "");

    nl!(ss, "{");
    ss.indent();

    // outColor = pow(max(outColor, 0.), exp);

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

    let exp4 = &op.data().exp4;
    nl!(
        ss,
        "res = pow( ",
        "max( res, ",
        ss.float4_const1(0.0f32),
        " )",
        ", ",
        ss.float4_const(exp4[0], exp4[1], exp4[2], exp4[3]),
        " );"
    );

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
