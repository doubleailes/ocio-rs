//! GPU renderer of the gamma op (port of `GammaOpGPU.cpp`).

use crate::error::Result;
use crate::gpu::{GpuShaderCreator, GpuShaderText};
use crate::nl;
use crate::ops::gamma::{
    compute_params_fwd, compute_params_rev, GammaOp, GammaOpData, GammaStyle, RendererParams,
};

/// The gamma of each channel (inverted for the reverse styles).
fn basic_gammas(gamma: &GammaOpData, reverse: bool) -> [f64; 4] {
    let g = |c: usize| {
        let v = gamma.params(c).first().copied().unwrap_or(1.0);
        if reverse {
            1. / v
        } else {
            v
        }
    };
    [g(0), g(1), g(2), g(3)]
}

fn write_result(ss: &mut GpuShaderText, pxl: &str) {
    nl!(
        ss,
        pxl,
        ".rgb = ",
        ss.float3_const("res.x", "res.y", "res.z"),
        ";"
    );
    nl!(ss, pxl, ".a = res.w;");
}

// Create shader for basic gamma style.
fn add_basic_shader(
    pxl: &str,
    gamma: &GammaOpData,
    ss: &mut GpuShaderText,
    reverse: bool,
) -> Result<()> {
    let g = basic_gammas(gamma, reverse);

    ss.declare_float4("gamma", g[0], g[1], g[2], g[3])?;

    nl!(
        ss,
        ss.float4_decl("res")?,
        " = pow( max( ",
        ss.float4_const1(0.0f32),
        ", ",
        pxl,
        " ), gamma );"
    );

    write_result(ss, pxl);
    Ok(())
}

// Create shader for basic mirror gamma style.
fn add_basic_mirror_shader(
    pxl: &str,
    gamma: &GammaOpData,
    ss: &mut GpuShaderText,
    reverse: bool,
) -> Result<()> {
    let g = basic_gammas(gamma, reverse);

    ss.declare_float4("gamma", g[0], g[1], g[2], g[3])?;

    nl!(ss, ss.float4_decl("signcol")?, " = ", ss.sign(pxl), ";");
    nl!(
        ss,
        ss.float4_decl("res")?,
        " = signcol * pow( abs( ",
        pxl,
        " ), gamma );"
    );

    write_result(ss, pxl);
    Ok(())
}

// Create shader for basic pass thru gamma style.
fn add_basic_pass_thru_shader(
    pxl: &str,
    gamma: &GammaOpData,
    ss: &mut GpuShaderText,
    reverse: bool,
) -> Result<()> {
    let g = basic_gammas(gamma, reverse);

    ss.declare_float4("gamma", g[0], g[1], g[2], g[3])?;
    ss.declare_float4("breakPnt", 0.0f32, 0.0f32, 0.0f32, 0.0f32)?;

    nl!(
        ss,
        ss.float4_decl("isAboveBreak")?,
        " = ",
        ss.float4_greater_than(pxl, "breakPnt"),
        ";"
    );

    nl!(
        ss,
        ss.float4_decl("powSeg")?,
        " = pow(max( ",
        ss.float4_const1(0.0f32),
        ", ",
        pxl,
        " ), gamma);"
    );

    nl!(
        ss,
        ss.float4_decl("res")?,
        " = isAboveBreak * powSeg + ( ",
        ss.float4_const1(1.0f32),
        " - isAboveBreak ) * ",
        pxl,
        ";"
    );

    write_result(ss, pxl);
    Ok(())
}

fn moncurve_params(gamma: &GammaOpData, reverse: bool) -> [RendererParams; 4] {
    let f = |c: usize| {
        if reverse {
            compute_params_rev(gamma.params(c))
        } else {
            compute_params_fwd(gamma.params(c))
        }
    };
    [f(0), f(1), f(2), f(3)]
}

fn declare_moncurve_params(ss: &mut GpuShaderText, p: &[RendererParams; 4]) -> Result<()> {
    // Even if all components are the same, on OS X, a vec4 needs to be
    // declared. This code will work in both cases.
    let [red, green, blue, alpha] = p;
    ss.declare_float4(
        "breakPnt",
        red.break_pnt,
        green.break_pnt,
        blue.break_pnt,
        alpha.break_pnt,
    )?;
    ss.declare_float4("slope", red.slope, green.slope, blue.slope, alpha.slope)?;
    ss.declare_float4("scale", red.scale, green.scale, blue.scale, alpha.scale)?;
    ss.declare_float4(
        "offset",
        red.offset,
        green.offset,
        blue.offset,
        alpha.offset,
    )?;
    ss.declare_float4("gamma", red.gamma, green.gamma, blue.gamma, alpha.gamma)?;
    Ok(())
}

// Create shader for moncurveFwd style.
fn add_moncurve_fwd_shader(pxl: &str, gamma: &GammaOpData, ss: &mut GpuShaderText) -> Result<()> {
    declare_moncurve_params(ss, &moncurve_params(gamma, false))?;

    nl!(
        ss,
        ss.float4_decl("isAboveBreak")?,
        " = ",
        ss.float4_greater_than(pxl, "breakPnt"),
        ";"
    );

    nl!(ss, ss.float4_decl("linSeg")?, " = ", pxl, " * slope;");

    nl!(
        ss,
        ss.float4_decl("powSeg")?,
        " = pow( max( ",
        ss.float4_const1(0.0f32),
        ", scale * ",
        pxl,
        " + offset), gamma);"
    );

    nl!(
        ss,
        ss.float4_decl("res")?,
        " = isAboveBreak * powSeg + ( ",
        ss.float4_const1(1.0f32),
        " - isAboveBreak ) * linSeg;"
    );

    write_result(ss, pxl);
    Ok(())
}

// Create shader for moncurveRev style.
fn add_moncurve_rev_shader(pxl: &str, gamma: &GammaOpData, ss: &mut GpuShaderText) -> Result<()> {
    declare_moncurve_params(ss, &moncurve_params(gamma, true))?;

    nl!(
        ss,
        ss.float4_decl("isAboveBreak")?,
        " = ",
        ss.float4_greater_than(pxl, "breakPnt"),
        ";"
    );

    nl!(ss, ss.float4_decl("linSeg")?, " = ", pxl, " * slope;");
    nl!(
        ss,
        ss.float4_decl("powSeg")?,
        " = pow( max( ",
        ss.float4_const1(0.0f32),
        ", ",
        pxl,
        " ), gamma ) * scale - offset;"
    );

    nl!(
        ss,
        ss.float4_decl("res")?,
        " = isAboveBreak * powSeg + ( ",
        ss.float4_const1(1.0f32),
        " - isAboveBreak ) * linSeg;"
    );

    write_result(ss, pxl);
    Ok(())
}

// Create shader for moncurveMirrorFwd style.
fn add_moncurve_mirror_fwd_shader(
    pxl: &str,
    gamma: &GammaOpData,
    ss: &mut GpuShaderText,
) -> Result<()> {
    declare_moncurve_params(ss, &moncurve_params(gamma, false))?;

    nl!(ss, ss.float4_decl("signcol")?, " = ", ss.sign(pxl), ";");
    nl!(ss, pxl, " = abs( ", pxl, " );");

    nl!(
        ss,
        ss.float4_decl("isAboveBreak")?,
        " = ",
        ss.float4_greater_than(pxl, "breakPnt"),
        ";"
    );

    nl!(ss, ss.float4_decl("linSeg")?, " = ", pxl, " * slope;");

    // Max() not needed since offset cannot be negative.
    nl!(
        ss,
        ss.float4_decl("powSeg")?,
        " = pow( scale * ",
        pxl,
        " + offset, gamma);"
    );

    nl!(
        ss,
        ss.float4_decl("res")?,
        " = isAboveBreak * powSeg + ( ",
        ss.float4_const1(1.0f32),
        " - isAboveBreak ) * linSeg;"
    );

    nl!(ss, "res = signcol * res;");

    write_result(ss, pxl);
    Ok(())
}

// Create shader for moncurveMirrorRev style.
fn add_moncurve_mirror_rev_shader(
    pxl: &str,
    gamma: &GammaOpData,
    ss: &mut GpuShaderText,
) -> Result<()> {
    declare_moncurve_params(ss, &moncurve_params(gamma, true))?;

    nl!(ss, ss.float4_decl("signcol")?, " = ", ss.sign(pxl), ";");
    nl!(ss, pxl, " = abs( ", pxl, " );");

    nl!(
        ss,
        ss.float4_decl("isAboveBreak")?,
        " = ",
        ss.float4_greater_than(pxl, "breakPnt"),
        ";"
    );

    nl!(ss, ss.float4_decl("linSeg")?, " = ", pxl, " * slope;");
    nl!(
        ss,
        ss.float4_decl("powSeg")?,
        " = pow( ",
        pxl,
        ", gamma ) * scale - offset;"
    );

    nl!(
        ss,
        ss.float4_decl("res")?,
        " = isAboveBreak * powSeg + ( ",
        ss.float4_const1(1.0f32),
        " - isAboveBreak ) * linSeg;"
    );

    nl!(ss, "res = signcol * res;");

    write_result(ss, pxl);
    Ok(())
}

/// Port of `GetGammaGPUShaderProgram`.
pub(crate) fn gamma_shader_program(
    shader_creator: &mut dyn GpuShaderCreator,
    gamma_data: &GammaOpData,
) -> Result<()> {
    let mut ss = GpuShaderText::new(shader_creator.language());
    ss.indent();

    nl!(ss, "");
    nl!(
        ss,
        "// Add Gamma '",
        gamma_data.style.as_str(),
        "' processing"
    );
    nl!(ss, "");

    nl!(ss, "{");
    ss.indent();

    let pxl = shader_creator.pixel_name().to_string();

    match gamma_data.style {
        GammaStyle::MoncurveFwd => add_moncurve_fwd_shader(&pxl, gamma_data, &mut ss)?,
        GammaStyle::MoncurveRev => add_moncurve_rev_shader(&pxl, gamma_data, &mut ss)?,
        GammaStyle::MoncurveMirrorFwd => add_moncurve_mirror_fwd_shader(&pxl, gamma_data, &mut ss)?,
        GammaStyle::MoncurveMirrorRev => add_moncurve_mirror_rev_shader(&pxl, gamma_data, &mut ss)?,
        GammaStyle::BasicFwd => add_basic_shader(&pxl, gamma_data, &mut ss, false)?,
        GammaStyle::BasicRev => add_basic_shader(&pxl, gamma_data, &mut ss, true)?,
        GammaStyle::BasicMirrorFwd => add_basic_mirror_shader(&pxl, gamma_data, &mut ss, false)?,
        GammaStyle::BasicMirrorRev => add_basic_mirror_shader(&pxl, gamma_data, &mut ss, true)?,
        GammaStyle::BasicPassThruFwd => {
            add_basic_pass_thru_shader(&pxl, gamma_data, &mut ss, false)?
        }
        GammaStyle::BasicPassThruRev => {
            add_basic_pass_thru_shader(&pxl, gamma_data, &mut ss, true)?
        }
    }

    ss.dedent();
    nl!(ss, "}");
    ss.dedent();

    shader_creator.add_to_function_shader_code(ss.as_str());
    Ok(())
}

/// Port of `GammaOp::extractGpuShaderInfo`.
pub(crate) fn extract(op: &GammaOp, shader_creator: &mut dyn GpuShaderCreator) -> Result<()> {
    gamma_shader_program(shader_creator, op.data())
}
