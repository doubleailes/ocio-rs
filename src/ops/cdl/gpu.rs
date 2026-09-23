//! GPU renderer of the CDL op (port of `CDLOpGPU.cpp`).

use crate::error::Result;
use crate::gpu::{GpuShaderCreator, GpuShaderText};
use crate::nl;
use crate::ops::cdl::{CdlOp, CdlOpData, CdlRenderParams};

fn add_power(ss: &mut GpuShaderText, pixrgb: &str, no_clamp: bool) -> Result<()> {
    if !no_clamp {
        nl!(ss, pixrgb, " = clamp(", pixrgb, ", 0.0, 1.0);");
        nl!(ss, pixrgb, " = pow(", pixrgb, ", power);");
    } else {
        nl!(ss, ss.float3_decl("posPix")?, " = step(0.0, ", pixrgb, ");");
        nl!(
            ss,
            ss.float3_decl("pixPower")?,
            " = pow(abs(",
            pixrgb,
            "), power);"
        );
        nl!(
            ss,
            pixrgb,
            " = ",
            ss.lerp(pixrgb, "pixPower", "posPix"),
            ";"
        );
    }
    Ok(())
}

/// Port of `GetCDLGPUShaderProgram`.
pub(crate) fn cdl_shader_program(
    shader_creator: &mut dyn GpuShaderCreator,
    cdl: &CdlOpData,
) -> Result<()> {
    let params = CdlRenderParams::new(cdl);

    let slope = params.slope;
    let offset = params.offset;
    let power = params.power;
    let saturation = params.saturation;

    let mut ss = GpuShaderText::new(shader_creator.language());
    ss.indent();

    nl!(ss, "");
    nl!(ss, "// Add CDL '", cdl.style.as_str(), "' processing");
    nl!(ss, "");

    nl!(ss, "{");
    ss.indent();

    let pix = shader_creator.pixel_name().to_string();
    let pixrgb = format!("{pix}.rgb");

    // Since alpha is not affected, only need to use the RGB components.
    ss.declare_float3("lumaWeights", 0.2126f32, 0.7152f32, 0.0722f32)?;
    ss.declare_float3("slope", slope[0], slope[1], slope[2])?;
    ss.declare_float3("offset", offset[0], offset[1], offset[2])?;
    ss.declare_float3("power", power[0], power[1], power[2])?;

    ss.declare_var("saturation", saturation)?;

    if !params.is_reverse {
        // Forward style.

        // Slope.
        nl!(ss, pixrgb, " = ", pixrgb, " * slope;");

        // Offset.
        nl!(ss, pixrgb, " = ", pixrgb, " + offset;");

        // Power.
        add_power(&mut ss, &pixrgb, params.is_no_clamp)?;

        // Saturation.
        nl!(ss, "float luma = dot(", pixrgb, ", lumaWeights);");
        nl!(ss, pixrgb, " = luma + saturation * (", pixrgb, " - luma);");

        // Post-saturation clamp.
        if !params.is_no_clamp {
            nl!(ss, pixrgb, " = clamp(", pixrgb, ", 0.0, 1.0);");
        }
    } else {
        // Reverse style.

        // Pre-saturation clamp.
        if !params.is_no_clamp {
            nl!(ss, pixrgb, "  = clamp(", pixrgb, ", 0.0, 1.0);");
        }

        // Saturation.
        nl!(ss, "float luma = dot(", pixrgb, ", lumaWeights);");
        nl!(ss, pixrgb, " = luma + saturation * (", pixrgb, " - luma);");

        // Power.
        add_power(&mut ss, &pixrgb, params.is_no_clamp)?;

        // Offset.
        nl!(ss, pixrgb, " = ", pixrgb, " + offset;");

        // Slope.
        nl!(ss, pixrgb, " = ", pixrgb, " * slope;");

        // Post-slope clamp.
        if !params.is_no_clamp {
            nl!(ss, pixrgb, " = clamp(", pixrgb, ", 0.0, 1.0);");
        }
    }

    ss.dedent();
    nl!(ss, "}");

    shader_creator.add_to_function_shader_code(ss.as_str());
    Ok(())
}

/// Port of `CDLOp::extractGpuShaderInfo`.
pub(crate) fn extract(op: &CdlOp, shader_creator: &mut dyn GpuShaderCreator) -> Result<()> {
    cdl_shader_program(shader_creator, op.data())
}
