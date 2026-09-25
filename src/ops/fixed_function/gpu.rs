//! GPU renderer of the fixed function op (port of `FixedFunctionOpGPU.cpp`).

// The float literals are kept exactly as in OCIO.
#![allow(clippy::approx_constant)]

use crate::error::{Error, Result};
use crate::gpu::shader_text::replace_all;
use crate::gpu::{GpuShaderCreator, GpuShaderText, TextureDimensions, TextureType};
use crate::nl;
use crate::ops::fixed_function::aces2::common::{
    table, ChromaCompressParams, GamutCompressParams, JMhParams, SharedCompressionParameters,
    Table1D, ToneScaleParams, CAM_NL_OFFSET, COMPRESSION_THRESHOLD, CUSP_MID_BLEND,
    FOCUS_GAIN_BLEND, J_SCALE, REFERENCE_LUMINANCE, SMOOTH_CUSPS,
};
use crate::ops::fixed_function::aces2::matrix::{Primaries, ACES_AP0, ACES_AP1};
use crate::ops::fixed_function::aces2::{
    init_chroma_compress_params, init_gamut_compress_params, init_jmh_params,
    init_shared_compression_params, init_tone_scale_params,
};
use crate::ops::fixed_function::{FixedFunctionOp, FixedFunctionOpData, FixedFunctionOpStyle};
use crate::types::{GpuLanguage, Interpolation};

type S = FixedFunctionOpStyle;

/// `std::to_string(float)`, i.e. `%f`.
fn to_string_f(v: f32) -> String {
    format!("{:.6}", f64::from(v))
}

fn param(params: &[f64], i: usize) -> Result<f64> {
    params
        .get(i)
        .copied()
        .ok_or_else(|| Error::msg("FixedFunction: missing parameter."))
}

fn add_hue_weight_shader(pxl: &str, ss: &mut GpuShaderText, width: f32) -> Result<()> {
    // Convert from degrees to radians.
    const PI: f32 = 3.14159265358979;
    let width_r = width * PI / 180.0f32;
    // Actually want to multiply by (4/width).
    let inv_width = 4.0f32 / width_r;

    // Note (OCIO): See the CPU renderer for more info on the algorithm.
    nl!(
        ss,
        ss.float_decl("a")?,
        " = 2.0 * ",
        pxl,
        ".rgb.r - (",
        pxl,
        ".rgb.g + ",
        pxl,
        ".rgb.b);"
    );
    nl!(
        ss,
        ss.float_decl("b")?,
        " = 1.7320508075688772 * (",
        pxl,
        ".rgb.g - ",
        pxl,
        ".rgb.b);"
    );
    nl!(ss, ss.float_decl("hue")?, " = ", ss.atan2("b", "a"), ";");

    // Since center is currently zero, the hue centering lines are omitted as
    // a performance optimization (as in OCIO).

    nl!(
        ss,
        ss.float_decl("knot_coord")?,
        " = clamp(2. + hue * float(",
        inv_width,
        "), 0., 4.);"
    );
    nl!(ss, "int j = int(min(knot_coord, 3.));");
    nl!(ss, ss.float_decl("t")?, " = knot_coord - float(j);");
    nl!(
        ss,
        ss.float4_decl("monomials")?,
        " = ",
        ss.float4_const("t*t*t", "t*t", "t", "1."),
        ";"
    );
    nl!(
        ss,
        ss.float4_decl("m0")?,
        " = ",
        ss.float4_const(0.25f64, 0.00f64, 0.00f64, 0.00f64),
        ";"
    );
    nl!(
        ss,
        ss.float4_decl("m1")?,
        " = ",
        ss.float4_const(-0.75f64, 0.75f64, 0.75f64, 0.25f64),
        ";"
    );
    nl!(
        ss,
        ss.float4_decl("m2")?,
        " = ",
        ss.float4_const(0.75f64, -1.50f64, 0.00f64, 1.00f64),
        ";"
    );
    nl!(
        ss,
        ss.float4_decl("m3")?,
        " = ",
        ss.float4_const(-0.25f64, 0.75f64, -0.75f64, 0.25f64),
        ";"
    );
    nl!(
        ss,
        ss.float4_decl("coefs")?,
        " = ",
        ss.lerp("m0", "m1", "float(j == 1)"),
        ";"
    );
    nl!(ss, "coefs = ", ss.lerp("coefs", "m2", "float(j == 2)"), ";");
    nl!(ss, "coefs = ", ss.lerp("coefs", "m3", "float(j == 3)"), ";");
    nl!(ss, ss.float_decl("f_H")?, " = dot(coefs, monomials);");
    Ok(())
}

fn add_max_min(pxl: &str, ss: &mut GpuShaderText) -> Result<()> {
    nl!(
        ss,
        ss.float_decl("maxval")?,
        " = max( ",
        pxl,
        ".rgb.r, max( ",
        pxl,
        ".rgb.g, ",
        pxl,
        ".rgb.b));"
    );
    nl!(
        ss,
        ss.float_decl("minval")?,
        " = min( ",
        pxl,
        ".rgb.r, min( ",
        pxl,
        ".rgb.g, ",
        pxl,
        ".rgb.b));"
    );
    Ok(())
}

fn add_new_chroma(pxl: &str, ss: &mut GpuShaderText) -> Result<()> {
    nl!(
        ss,
        ss.float_decl("maxval2")?,
        " = max( ",
        pxl,
        ".rgb.r, max( ",
        pxl,
        ".rgb.g, ",
        pxl,
        ".rgb.b));"
    );
    nl!(ss, ss.float_decl("newChroma")?, " = maxval2 - minval;");
    nl!(ss, pxl, ".rgb = minval + delta * newChroma / oldChroma;");
    Ok(())
}

fn add_red_mod_fwd_shader(pxl: &str, ss: &mut GpuShaderText, is_03: bool) -> Result<()> {
    // (1. - scale) from the original ctl code.
    let one_minus_scale: f32 = if is_03 {
        1.0f32 - 0.85f32
    } else {
        1.0f32 - 0.82f32
    };
    let pivot: f32 = 0.03;

    add_hue_weight_shader(pxl, ss, if is_03 { 120.0 } else { 135.0 })?;

    add_max_min(pxl, ss)?;

    if is_03 {
        nl!(
            ss,
            ss.float_decl("oldChroma")?,
            " = max(1e-10, maxval - minval);"
        );
        nl!(ss, ss.float3_decl("delta")?, " = ", pxl, ".rgb - minval;");
    }

    nl!(
        ss,
        ss.float_decl("f_S")?,
        " = ( max(1e-10, maxval) - max(1e-10, minval) ) / max(1e-2, maxval);"
    );

    nl!(
        ss,
        pxl,
        ".rgb.r = ",
        pxl,
        ".rgb.r + f_H * f_S * (",
        pivot,
        " - ",
        pxl,
        ".rgb.r) * ",
        one_minus_scale,
        ";"
    );

    if is_03 {
        add_new_chroma(pxl, ss)?;
    }
    Ok(())
}

fn add_red_mod_inv_shader(pxl: &str, ss: &mut GpuShaderText, is_03: bool) -> Result<()> {
    // (1. - scale) from the original ctl code.
    let one_minus_scale: f32 = if is_03 {
        1.0f32 - 0.85f32
    } else {
        1.0f32 - 0.82f32
    };
    let pivot: f32 = 0.03;

    add_hue_weight_shader(pxl, ss, if is_03 { 120.0 } else { 135.0 })?;

    nl!(ss, "if (f_H > 0.)");
    nl!(ss, "{");
    ss.indent();

    if is_03 {
        add_max_min(pxl, ss)?;

        nl!(
            ss,
            ss.float_decl("oldChroma")?,
            " = max(1e-10, maxval - minval);"
        );
        nl!(ss, ss.float3_decl("delta")?, " = ", pxl, ".rgb - minval;");
    } else {
        nl!(
            ss,
            ss.float_decl("minval")?,
            " = min( ",
            pxl,
            ".rgb.g, ",
            pxl,
            ".rgb.b);"
        );
    }

    // Note: If f_H == 0, the following generally doesn't change the red
    //       value, but it does for R < 0, hence the need for the if-statement
    //       above.
    nl!(
        ss,
        ss.float_decl("ka")?,
        " = f_H * ",
        one_minus_scale,
        " - 1.;"
    );
    nl!(
        ss,
        ss.float_decl("kb")?,
        " = ",
        pxl,
        ".rgb.r - f_H * (",
        pivot,
        " + minval) * ",
        one_minus_scale,
        ";"
    );
    nl!(
        ss,
        ss.float_decl("kc")?,
        " = f_H * ",
        pivot,
        " * minval * ",
        one_minus_scale,
        ";"
    );
    nl!(
        ss,
        pxl,
        ".rgb.r = ( -kb - sqrt( kb * kb - 4. * ka * kc)) / ( 2. * ka);"
    );

    if is_03 {
        add_new_chroma(pxl, ss)?;
    }

    ss.dedent();
    nl!(ss, "}");
    Ok(())
}

fn add_glow_03_shader(
    pxl: &str,
    ss: &mut GpuShaderText,
    glow_gain: f32,
    glow_mid: f32,
    fwd: bool,
) -> Result<()> {
    nl!(
        ss,
        ss.float_decl("chroma")?,
        " = sqrt( ",
        pxl,
        ".rgb.b * (",
        pxl,
        ".rgb.b - ",
        pxl,
        ".rgb.g)",
        " + ",
        pxl,
        ".rgb.g * (",
        pxl,
        ".rgb.g - ",
        pxl,
        ".rgb.r)",
        " + ",
        pxl,
        ".rgb.r * (",
        pxl,
        ".rgb.r - ",
        pxl,
        ".rgb.b) );"
    );
    nl!(
        ss,
        ss.float_decl("YC")?,
        " = (",
        pxl,
        ".rgb.b + ",
        pxl,
        ".rgb.g + ",
        pxl,
        ".rgb.r + 1.75 * chroma) / 3.;"
    );

    add_max_min(pxl, ss)?;

    nl!(
        ss,
        ss.float_decl("sat")?,
        " = ( max(1e-10, maxval) - max(1e-10, minval) ) / max(1e-2, maxval);"
    );

    nl!(ss, ss.float_decl("x")?, " = (sat - 0.4) * 5.;");
    nl!(ss, ss.float_decl("t")?, " = max( 0., 1. - 0.5 * abs(x));");
    nl!(
        ss,
        ss.float_decl("s")?,
        " = 0.5 * (1. + sign(x) * (1. - t * t));"
    );

    nl!(ss, ss.float_decl("GlowGain")?, " = ", glow_gain, " * s;");
    nl!(ss, ss.float_decl("GlowMid")?, " = ", glow_mid, ";");
    if fwd {
        nl!(
            ss,
            ss.float_decl("glowGainOut")?,
            " = ",
            ss.lerp(
                "GlowGain",
                "GlowGain * (GlowMid / YC - 0.5)",
                "float( YC > GlowMid * 2. / 3. )"
            ),
            ";"
        );
    } else {
        nl!(
            ss,
            ss.float_decl("glowGainOut")?,
            " = ",
            ss.lerp(
                "-GlowGain / (1. + GlowGain)",
                "GlowGain * (GlowMid / YC - 0.5) / (GlowGain * 0.5 - 1.)",
                "float( YC > (1. + GlowGain) * GlowMid * 2. / 3. )"
            ),
            ";"
        );
    }
    nl!(
        ss,
        "glowGainOut = ",
        ss.lerp("glowGainOut", "0.", "float( YC > GlowMid * 2. )"),
        ";"
    );

    nl!(
        ss,
        pxl,
        ".rgb = ",
        pxl,
        ".rgb * glowGainOut + ",
        pxl,
        ".rgb;"
    );
    Ok(())
}

fn add_gamut_comp_13_shader_compress(
    ss: &mut GpuShaderText,
    dist: &str,
    cdist: &str,
    scl: f32,
    thr: f32,
    power: f32,
) -> Result<()> {
    // Only compress if greater or equal than threshold.
    nl!(ss, "if (", dist, " >= ", thr, ")");
    nl!(ss, "{");
    ss.indent();

    // Normalize distance outside threshold by scale factor.
    nl!(
        ss,
        ss.float_decl("nd")?,
        " = (",
        dist,
        " - ",
        thr,
        ") / ",
        scl,
        ";"
    );
    nl!(ss, ss.float_decl("p")?, " = pow(nd, ", power, ");");
    nl!(
        ss,
        cdist,
        " = ",
        thr,
        " + ",
        scl,
        " * nd / (pow(1.0 + p, ",
        1.0f32 / power,
        "));"
    );

    ss.dedent();
    nl!(ss, "}"); // if (dist >= thr)
    Ok(())
}

fn add_gamut_comp_13_shader_uncompress(
    ss: &mut GpuShaderText,
    dist: &str,
    cdist: &str,
    scl: f32,
    thr: f32,
    power: f32,
) -> Result<()> {
    // Only compress if greater or equal than threshold, avoid singularity.
    nl!(
        ss,
        "if (",
        dist,
        " >= ",
        thr,
        " && ",
        dist,
        " < ",
        thr + scl,
        " )"
    );
    nl!(ss, "{");
    ss.indent();

    // Normalize distance outside threshold by scale factor.
    nl!(
        ss,
        ss.float_decl("nd")?,
        " = (",
        dist,
        " - ",
        thr,
        ") / ",
        scl,
        ";"
    );
    nl!(ss, ss.float_decl("p")?, " = pow(nd, ", power, ");");
    nl!(
        ss,
        cdist,
        " = ",
        thr,
        " + ",
        scl,
        " * pow(-(p / (p - 1.0)), ",
        1.0f32 / power,
        ");"
    );

    ss.dedent();
    nl!(ss, "}"); // if (dist >= thr && dist < thr + scl)
    Ok(())
}

fn add_gamut_comp_13_shader(
    pix: &str,
    ss: &mut GpuShaderText,
    params: &[f64],
    fwd: bool,
) -> Result<()> {
    let p = |i: usize| param(params, i).map(|v| v as f32);
    let (lim_cyan, lim_magenta, lim_yellow) = (p(0)?, p(1)?, p(2)?);
    let (thr_cyan, thr_magenta, thr_yellow) = (p(3)?, p(4)?, p(5)?);
    let power = p(6)?;

    // Precompute scale factor for y = 1 intersect.
    let f_scale = |lim: f32, thr: f32| -> f32 {
        (lim - thr) / (((1.0f32 - thr) / (lim - thr)).powf(-power) - 1.0f32).powf(1.0f32 / power)
    };
    let scale_cyan = f_scale(lim_cyan, thr_cyan);
    let scale_magenta = f_scale(lim_magenta, thr_magenta);
    let scale_yellow = f_scale(lim_yellow, thr_yellow);

    // Achromatic axis.
    nl!(
        ss,
        ss.float_decl("ach")?,
        " = max( ",
        pix,
        ".rgb.r, max( ",
        pix,
        ".rgb.g, ",
        pix,
        ".rgb.b ) );"
    );

    nl!(ss, "if ( ach != 0. )");
    nl!(ss, "{");
    ss.indent();

    // Distance from the achromatic axis for each color component aka inverse
    // rgb ratios.
    nl!(
        ss,
        ss.float3_decl("dist")?,
        " = (ach - ",
        pix,
        ".rgb) / abs(ach);"
    );
    nl!(ss, ss.float3_decl("cdist")?, " = dist;");

    let f = if fwd {
        add_gamut_comp_13_shader_compress
    } else {
        add_gamut_comp_13_shader_uncompress
    };
    f(ss, "dist.x", "cdist.x", scale_cyan, thr_cyan, power)?;
    f(ss, "dist.y", "cdist.y", scale_magenta, thr_magenta, power)?;
    f(ss, "dist.z", "cdist.z", scale_yellow, thr_yellow, power)?;

    // Recalculate rgb from compressed distance and achromatic. Effectively
    // this scales each color component relative to achromatic axis by the
    // compressed distance.
    nl!(ss, pix, ".rgb = ach - cdist * abs(ach);");

    ss.dedent();
    nl!(ss, "}"); // if ( ach != 0.0f )
    Ok(())
}

fn add_wrap_hue_channel_shader(pxl: &str, ss: &mut GpuShaderText) -> Result<()> {
    nl!(ss, ss.float_decl("hwrap")?, " = ", pxl, ".b;");
    nl!(ss, "hwrap = hwrap - floor(hwrap / 360.0) * 360.0;");
    nl!(ss, "hwrap = (hwrap < 0.0) ? hwrap + 360.0 : hwrap;");
    nl!(ss, pxl, ".b = hwrap;");
    Ok(())
}

fn add_sin_cos_shader(pxl: &str, ss: &mut GpuShaderText) -> Result<()> {
    nl!(
        ss,
        ss.float_decl("h_rad")?,
        " = ",
        pxl,
        ".b * ",
        3.14159265358979f32 / 180.0f32,
        ";"
    );
    nl!(ss, ss.float_decl("cos_hr")?, " = cos(h_rad);");
    nl!(ss, ss.float_decl("sin_hr")?, " = sin(h_rad);");
    Ok(())
}

fn add_rgb_to_aab_shader(pxl: &str, ss: &mut GpuShaderText, p: &JMhParams) -> Result<()> {
    nl!(ss, "{");
    ss.indent();

    nl!(
        ss,
        ss.float3_decl("lms")?,
        " = ",
        ss.mat3f_mul(&p.matrix_rgb_to_cam16_c, &format!("{pxl}.rgb"))?,
        ";"
    );

    nl!(
        ss,
        ss.float3_decl("F_L_v")?,
        " = pow(abs(lms), ",
        ss.float3_const1(0.42f32),
        ");"
    );
    nl!(
        ss,
        ss.float3_decl("rgb_a")?,
        " = (sign(lms) * F_L_v) / ( ",
        CAM_NL_OFFSET,
        " + F_L_v);"
    );

    nl!(
        ss,
        "Aab = ",
        ss.mat3f_mul(&p.matrix_cone_response_to_aab, "rgb_a.rgb")?,
        ";"
    );

    ss.dedent();
    nl!(ss, "}");
    Ok(())
}

fn add_aab_to_jmh_shader(ss: &mut GpuShaderText, p: &JMhParams) -> Result<()> {
    nl!(ss, "{");
    ss.indent();

    nl!(ss, "if (Aab.r <= 0.0)");
    nl!(ss, "{");
    ss.indent();
    nl!(ss, "JMh.rgb = ", ss.float3_const1(0.0f64), ";");
    ss.dedent();
    nl!(ss, "}");

    nl!(ss, "else");
    nl!(ss, "{");
    ss.indent();
    nl!(
        ss,
        ss.float_decl("J")?,
        " = ",
        J_SCALE,
        " * pow(Aab.r, ",
        p.cz,
        ");"
    );

    nl!(
        ss,
        ss.float_decl("M")?,
        " = (J == 0.0) ? 0.0 : sqrt(Aab.g * Aab.g + Aab.b * Aab.b);"
    );

    nl!(
        ss,
        ss.float_decl("h")?,
        " = (Aab.g == 0.0) ? 0.0 : ",
        ss.atan2("Aab.b", "Aab.g"),
        " * ",
        180.0f64 / 3.14159265358979f64,
        ";"
    );
    nl!(ss, "h = h - floor(h / 360.0) * 360.0;");
    nl!(ss, "h = (h < 0.0) ? h + 360.0 : h;");

    nl!(ss, "JMh.rgb = ", ss.float3_const("J", "M", "h"), ";");
    ss.dedent();
    nl!(ss, "}");

    ss.dedent();
    nl!(ss, "}");
    Ok(())
}

fn add_rgb_to_jmh_shader_impl(pxl: &str, ss: &mut GpuShaderText, p: &JMhParams) -> Result<()> {
    // Note (OCIO): leaky abstraction, should really be explicit functions.
    nl!(ss, ss.float3_decl("JMh")?, ";");
    nl!(ss, ss.float3_decl("Aab")?, ";");

    nl!(ss, "{");
    ss.indent();

    add_rgb_to_aab_shader(pxl, ss, p)?;
    add_aab_to_jmh_shader(ss, p)?;

    nl!(ss, pxl, ".rgb = JMh;");

    ss.dedent();
    nl!(ss, "}");
    Ok(())
}

fn add_jmh_to_aab_shader(ss: &mut GpuShaderText, p: &JMhParams) -> Result<()> {
    nl!(ss, "{");
    ss.indent();

    nl!(
        ss,
        "Aab.r = pow(JMh.r * ",
        1.0f32 / J_SCALE,
        ", ",
        p.inv_cz,
        ");"
    );
    nl!(ss, "Aab.g = JMh.g * cos_hr;");
    nl!(ss, "Aab.b = JMh.g * sin_hr;");

    ss.dedent();
    nl!(ss, "}");
    Ok(())
}

fn add_aab_to_rgb_shader(ss: &mut GpuShaderText, p: &JMhParams) -> Result<()> {
    nl!(ss, "{");
    ss.indent();

    nl!(
        ss,
        ss.float3_decl("rgb_a")?,
        " = ",
        ss.mat3f_mul(&p.matrix_aab_to_cone_response, "Aab.rgb")?,
        ";"
    );
    nl!(
        ss,
        ss.float3_decl("rgb_a_lim")?,
        " = min( abs(rgb_a), ",
        ss.float3_const1(0.99f32),
        " );"
    );
    nl!(
        ss,
        ss.float3_decl("lms")?,
        " = sign(rgb_a) * pow( ",
        CAM_NL_OFFSET,
        " * rgb_a_lim / (1.0f - rgb_a_lim), ",
        ss.float3_const1(1.0f32 / 0.42f32),
        ");"
    );
    nl!(
        ss,
        "JMh.rgb = ",
        ss.mat3f_mul(&p.matrix_cam16_c_to_rgb, "lms")?,
        ";"
    );

    ss.dedent();
    nl!(ss, "}");
    Ok(())
}

fn add_jmh_to_rgb_shader_impl(pxl: &str, ss: &mut GpuShaderText, p: &JMhParams) -> Result<()> {
    nl!(ss, ss.float3_decl("JMh")?, " = ", pxl, ".rgb;");
    nl!(ss, ss.float3_decl("Aab")?, ";");
    add_jmh_to_aab_shader(ss, p)?;
    add_aab_to_rgb_shader(ss, p)?;

    nl!(ss, pxl, ".rgb = JMh;");
    Ok(())
}

/// Reserve a resource name (the double underscores are removed).
fn resource_name(shader_creator: &dyn GpuShaderCreator, base: &str) -> String {
    let name = format!("{}_{}", shader_creator.resource_prefix(), base);
    // Note: Remove potentially problematic double underscores from GLSL
    // resource names.
    replace_all(&name, "__", "_")
}

/// The dimensions of the ACES 2 table textures.
fn table_dimensions(shader_creator: &dyn GpuShaderCreator) -> TextureDimensions {
    let lang = shader_creator.language();
    if lang == GpuLanguage::GlslEs1_0
        || lang == GpuLanguage::GlslEs3_0
        || !shader_creator.allow_texture_1d()
    {
        TextureDimensions::Texture2D
    } else {
        TextureDimensions::Texture1D
    }
}

fn declare_table_texture(
    shader_creator: &mut dyn GpuShaderCreator,
    name: &str,
    dimensions: TextureDimensions,
    binding_index: u32,
) -> Result<()> {
    let mut ss = GpuShaderText::new(shader_creator.language());
    if dimensions == TextureDimensions::Texture1D {
        ss.declare_tex1d(name, shader_creator.descriptor_set_index(), binding_index)?;
    } else {
        ss.declare_tex2d(name, shader_creator.descriptor_set_index(), binding_index)?;
    }
    shader_creator.add_to_texture_declare_shader_code(ss.as_str());
    Ok(())
}

fn add_reach_table(
    shader_creator: &mut dyn GpuShaderCreator,
    resource_index: u32,
    tbl: &Table1D,
) -> Result<String> {
    // Reserve name.
    let name = resource_name(shader_creator, &format!("reach_m_table_{resource_index}"));

    // Determine texture dimensions.
    let dimensions = table_dimensions(shader_creator);

    // Copy the LUT into the shader creator as a texture object.
    let texture_shader_binding_index = shader_creator.add_texture(
        &name,
        &GpuShaderText::sampler_name(&name),
        table::TOTAL_SIZE as u32,
        1,
        TextureType::RedChannel,
        dimensions,
        Interpolation::Nearest,
        &tbl[..],
    )?;

    // Create the texture declaration.
    declare_table_texture(
        shader_creator,
        &name,
        dimensions,
        texture_shader_binding_index,
    )?;

    // Sampler function.
    let mut ss = GpuShaderText::new(shader_creator.language());

    nl!(ss, ss.float_keyword(), " ", name, "_sample(float h)");
    nl!(ss, "{");
    ss.indent();

    let total = table::TOTAL_SIZE.to_string();

    nl!(ss, ss.float_decl("i_base")?, " = floor(h);");
    nl!(
        ss,
        ss.float_decl("i_lo")?,
        " = i_base + ",
        ss.float_keyword(),
        "(",
        table::BASE_INDEX,
        ");"
    );
    nl!(ss, ss.float_decl("i_hi")?, " = i_lo + 1.0;");

    let lo = format!("(i_lo + 0.5) / {} ({})", ss.float_keyword(), total);
    let hi = format!("(i_hi + 0.5) / {} ({})", ss.float_keyword(), total);
    if dimensions == TextureDimensions::Texture1D {
        nl!(
            ss,
            ss.float_decl("lo")?,
            " = ",
            ss.sample_tex1d(&name, &lo)?,
            ".r;"
        );
        nl!(
            ss,
            ss.float_decl("hi")?,
            " = ",
            ss.sample_tex1d(&name, &hi)?,
            ".r;"
        );
    } else {
        nl!(
            ss,
            ss.float_decl("lo")?,
            " = ",
            ss.sample_tex2d(&name, &ss.float2_const(&lo, "0.0"))?,
            ".r;"
        );
        nl!(
            ss,
            ss.float_decl("hi")?,
            " = ",
            ss.sample_tex2d(&name, &ss.float2_const(&hi, "0.5"))?,
            ".r;"
        );
    }

    nl!(ss, ss.float_decl("t")?, " = h - i_base;"); // Hardcoded single degree spacing
    nl!(ss, "return ", ss.lerp("lo", "hi", "t"), ";");

    ss.dedent();
    nl!(ss, "}");

    shader_creator.add_to_helper_shader_code(ss.as_str());

    Ok(name)
}

fn add_toe_func(
    shader_creator: &mut dyn GpuShaderCreator,
    resource_index: u32,
    invert: bool,
) -> Result<String> {
    // Reserve name.
    let name = resource_name(
        shader_creator,
        &format!(
            "toe{}{}",
            if invert { "_inv" } else { "_fwd" },
            resource_index
        ),
    );

    let mut ss = GpuShaderText::new(shader_creator.language());

    nl!(
        ss,
        ss.float_keyword(),
        " ",
        name,
        "(float x, float limit, float k1_in, float k2_in)"
    );
    nl!(ss, "{");
    ss.indent();

    nl!(ss, ss.float_decl("k2")?, " = max(k2_in, 0.001);");
    nl!(
        ss,
        ss.float_decl("k1")?,
        " = sqrt(k1_in * k1_in + k2 * k2);"
    );
    nl!(ss, ss.float_decl("k3")?, " = (limit + k1) / (limit + k2);");

    if invert {
        nl!(
            ss,
            "return (x > limit) ? x : (x * x + k1 * x) / (k3 * (x + k2));"
        );
    } else {
        nl!(
            ss,
            "return (x > limit) ? x : 0.5 * (k3 * x - k1 + sqrt((k3 * x - k1) * (k3 * x - k1) + 4.0 * k2 * k3 * x));"
        );
    }

    ss.dedent();
    nl!(ss, "}");

    shader_creator.add_to_helper_shader_code(ss.as_str());

    Ok(name)
}

fn add_tonescale_func(
    shader_creator: &mut dyn GpuShaderCreator,
    resource_index: u32,
    invert: bool,
    p: &JMhParams,
    t: &ToneScaleParams,
) -> Result<String> {
    // Reserve name.
    let name = resource_name(
        shader_creator,
        &format!(
            "tonescale{}{}",
            if invert { "_inv" } else { "_fwd" },
            resource_index
        ),
    );

    let mut ss = GpuShaderText::new(shader_creator.language());

    nl!(ss, ss.float_keyword(), " ", name, "(float J)");
    nl!(ss, "{");
    ss.indent();

    // Tonescale applied in Y (convert to and from J).
    nl!(
        ss,
        ss.float_decl("A")?,
        " = ",
        p.a_w_j,
        " * pow(abs(J) * ",
        1.0f32 / J_SCALE,
        ", ",
        p.inv_cz,
        ");"
    );
    nl!(
        ss,
        ss.float_decl("Y")?,
        " = pow(( ",
        CAM_NL_OFFSET,
        " * A) / (1.0f - A), ",
        1.0f64 / 0.42f64,
        ");"
    );

    if invert {
        // Inverse Tonescale applied in Y (convert to and from J).
        nl!(
            ss,
            ss.float_decl("Y_i")?,
            " = Y / ",
            f64::from(p.f_l_n) * f64::from(REFERENCE_LUMINANCE),
            ";"
        );

        nl!(
            ss,
            ss.float_decl("Z")?,
            " = max(0.0, min(",
            t.inverse_limit,
            ", Y_i));"
        );
        nl!(
            ss,
            ss.float_decl("ht")?,
            " = 0.5 * (Z + sqrt(Z * (",
            4.0f64 * f64::from(t.t_1),
            " + Z)));"
        );
        nl!(
            ss,
            ss.float_decl("Yo")?,
            " = ",
            f64::from(p.f_l_n) * f64::from(t.s_2),
            " / (pow((",
            t.m_2,
            " / ht), (",
            1.0f64 / f64::from(t.g),
            ")) - 1.0);"
        );

        nl!(ss, ss.float_decl("F_L_Y")?, " = pow(abs(Yo), 0.42);");
    } else {
        // Tonescale applied in Y (convert to and from J).
        nl!(
            ss,
            ss.float_decl("f")?,
            " = ",
            t.m_2,
            " * pow(Y / (Y + ",
            f64::from(t.s_2) * f64::from(p.f_l_n),
            "), ",
            t.g,
            ");"
        );
        nl!(
            ss,
            ss.float_decl("Y_ts")?,
            " = max(0.0, f * f / (f + ",
            t.t_1,
            "));"
        );
        nl!(
            ss,
            ss.float_decl("F_L_Y")?,
            " = pow(",
            f64::from(p.f_l_n) * f64::from(REFERENCE_LUMINANCE),
            " * Y_ts, 0.42);"
        );
    }

    nl!(
        ss,
        ss.float_decl("J_ts")?,
        " = ",
        J_SCALE,
        " * pow((F_L_Y / ( ",
        CAM_NL_OFFSET,
        " + F_L_Y)) * ",
        p.inv_a_w_j,
        ", ",
        p.cz,
        ");"
    );
    nl!(ss, "return sign(J) * J_ts;");

    ss.dedent();
    nl!(ss, "}");

    shader_creator.add_to_helper_shader_code(ss.as_str());

    Ok(name)
}

fn add_chroma_compression_norm_shader(
    ss: &mut GpuShaderText,
    c: &ChromaCompressParams,
) -> Result<()> {
    let scale = f64::from(c.chroma_compress_scale);

    // Mnorm
    nl!(ss, ss.float_decl("Mnorm")?, ";");
    nl!(ss, "{");
    ss.indent();

    nl!(
        ss,
        ss.float_decl("cos_hr2")?,
        " = 2.0 * cos_hr * cos_hr - 1.0;"
    );
    nl!(ss, ss.float_decl("sin_hr2")?, " = 2.0 * cos_hr * sin_hr;");
    nl!(
        ss,
        ss.float_decl("cos_hr3")?,
        " = 4.0 * cos_hr * cos_hr * cos_hr - 3.0 * cos_hr;"
    );
    nl!(
        ss,
        ss.float_decl("sin_hr3")?,
        " = 3.0 * sin_hr - 4.0 * sin_hr * sin_hr * sin_hr;"
    );
    nl!(
        ss,
        ss.float3_decl("cosines")?,
        " = ",
        ss.float3_const("cos_hr", "cos_hr2", "cos_hr3"),
        ";"
    );
    nl!(
        ss,
        ss.float3_decl("cosine_weights")?,
        " = ",
        ss.float3_const(11.34072 * scale, 16.46899 * scale, 7.88380 * scale),
        ";"
    );
    nl!(
        ss,
        ss.float3_decl("sines")?,
        " = ",
        ss.float3_const("sin_hr", "sin_hr2", "sin_hr3"),
        ";"
    );
    nl!(
        ss,
        ss.float3_decl("sine_weights")?,
        " = ",
        ss.float3_const(14.66441 * scale, -6.37224 * scale, 9.19364 * scale),
        ";"
    );
    nl!(
        ss,
        "Mnorm = dot(cosines, cosine_weights) + dot(sines, sine_weights) + ",
        77.12896 * scale,
        ";"
    );

    ss.dedent();
    nl!(ss, "}");
    Ok(())
}

fn add_tonescale_compress_fwd_shader_impl(
    shader_creator: &mut dyn GpuShaderCreator,
    ss: &mut GpuShaderText,
    resource_index: u32,
    s: &SharedCompressionParameters,
    c: &ChromaCompressParams,
) -> Result<()> {
    let toe_name = add_toe_func(shader_creator, resource_index, false)?;

    let pxl = shader_creator.pixel_name().to_string();

    nl!(ss, ss.float_decl("J")?, " = ", pxl, ".r;");
    nl!(ss, ss.float_decl("M")?, " = ", pxl, ".g;");
    nl!(ss, ss.float_decl("h")?, " = ", pxl, ".b;");

    // ChromaCompress
    nl!(ss, ss.float_decl("M_cp")?, " = M;");

    nl!(ss, "if (M != 0.0)");
    nl!(ss, "{");
    ss.indent();

    nl!(ss, ss.float_decl("nJ")?, " = J_ts / ", s.limit_j_max, ";");
    nl!(ss, ss.float_decl("snJ")?, " = max(0.0, 1.0 - nJ);");

    add_chroma_compression_norm_shader(ss, c)?;

    nl!(
        ss,
        ss.float_decl("limit")?,
        " = pow(nJ, ",
        s.model_gamma_inv,
        ") * reachMaxM / Mnorm;"
    );
    nl!(ss, "M_cp = M * pow(J_ts / J, ", s.model_gamma_inv, ");");
    nl!(ss, "M_cp = M_cp / Mnorm;");

    nl!(
        ss,
        "M_cp = limit - ",
        toe_name,
        "(limit - M_cp, limit - 0.001, snJ * ",
        c.sat,
        ", sqrt(nJ * nJ + ",
        c.sat_thr,
        "));"
    );
    nl!(
        ss,
        "M_cp = ",
        toe_name,
        "(M_cp, limit, nJ * ",
        c.compr,
        ", snJ);"
    );
    nl!(ss, "M_cp = M_cp * Mnorm;");

    ss.dedent();
    nl!(ss, "}");

    nl!(
        ss,
        pxl,
        ".rgb = ",
        ss.float3_const("J_ts", "M_cp", "h"),
        ";"
    );
    Ok(())
}

fn add_tonescale_compress_inv_shader_impl(
    shader_creator: &mut dyn GpuShaderCreator,
    ss: &mut GpuShaderText,
    resource_index: u32,
    s: &SharedCompressionParameters,
    c: &ChromaCompressParams,
) -> Result<()> {
    let toe_name = add_toe_func(shader_creator, resource_index, true)?;

    let pxl = shader_creator.pixel_name().to_string();

    nl!(ss, ss.float_decl("J_ts")?, " = ", pxl, ".r;");
    nl!(ss, ss.float_decl("M_cp")?, " = ", pxl, ".g;");
    nl!(ss, ss.float_decl("h")?, " = ", pxl, ".b;");

    // ChromaCompress
    nl!(ss, ss.float_decl("M")?, " = M_cp;");

    nl!(ss, "if (M_cp != 0.0)");
    nl!(ss, "{");
    ss.indent();

    nl!(ss, ss.float_decl("nJ")?, " = J_ts / ", s.limit_j_max, ";");
    nl!(ss, ss.float_decl("snJ")?, " = max(0.0, 1.0 - nJ);");

    add_chroma_compression_norm_shader(ss, c)?;

    nl!(
        ss,
        ss.float_decl("limit")?,
        " = pow(nJ, ",
        s.model_gamma_inv,
        ") * reachMaxM / Mnorm;"
    );

    nl!(ss, "M = M_cp / Mnorm;");
    nl!(ss, "M = ", toe_name, "(M, limit, nJ * ", c.compr, ", snJ);");
    nl!(
        ss,
        "M = limit - ",
        toe_name,
        "(limit - M, limit - 0.001, snJ * ",
        c.sat,
        ", sqrt(nJ * nJ + ",
        c.sat_thr,
        "));"
    );
    nl!(ss, "M = M * Mnorm;");
    nl!(ss, "M = M * pow(J_ts / J, ", -s.model_gamma_inv, ");");

    ss.dedent();
    nl!(ss, "}");

    nl!(ss, pxl, ".rgb = ", ss.float3_const("J", "M", "h"), ";");
    Ok(())
}

fn add_cusp_table(
    shader_creator: &mut dyn GpuShaderCreator,
    resource_index: u32,
    g: &GamutCompressParams,
) -> Result<String> {
    // Reserve name.
    let name = resource_name(
        shader_creator,
        &format!("gamut_cusp_table_{resource_index}"),
    );

    // Determine texture dimensions.
    let dimensions = table_dimensions(shader_creator);

    let values: Vec<f32> = g.gamut_cusp_table.iter().flatten().copied().collect();

    // Copy the LUT into the shader creator as a texture object.
    let texture_shader_binding_index = shader_creator.add_texture(
        &name,
        &GpuShaderText::sampler_name(&name),
        table::TOTAL_SIZE as u32,
        1,
        TextureType::RgbChannel,
        dimensions,
        Interpolation::Nearest,
        &values,
    )?;

    // Create the texture declaration.
    declare_table_texture(
        shader_creator,
        &name,
        dimensions,
        texture_shader_binding_index,
    )?;

    // Sampler function.
    let mut ss = GpuShaderText::new(shader_creator.language());

    let hues_array_name = format!("{name}_hues_array");
    ss.declare_float_array_const(&hues_array_name, &g.hue_table[..])?;

    nl!(ss, ss.float3_keyword(), " ", name, "_sample(float h)");
    nl!(ss, "{");
    ss.indent();

    nl!(
        ss,
        ss.int_decl("i")?,
        " = ",
        ss.int_keyword(),
        "(h) + ",
        table::BASE_INDEX,
        ";"
    );

    nl!(
        ss,
        ss.int_decl("i_lo")?,
        " = ",
        ss.int_keyword(),
        "(max(",
        ss.float_keyword(),
        "(",
        table::LOWER_WRAP_INDEX,
        "), ",
        ss.float_keyword(),
        "(i + ",
        g.hue_linearity_search_range[0],
        ")));"
    );
    nl!(
        ss,
        ss.int_decl("i_hi")?,
        " = ",
        ss.int_keyword(),
        "(min(",
        ss.float_keyword(),
        "(",
        table::UPPER_WRAP_INDEX,
        "), ",
        ss.float_keyword(),
        "(i + ",
        g.hue_linearity_search_range[1],
        ")));"
    );

    nl!(ss, "while (i_lo + 1 < i_hi)");
    nl!(ss, "{");
    ss.indent();

    nl!(ss, ss.float_decl("hcur")?, " = ", hues_array_name, "[i];");

    nl!(ss, "if (h > hcur)");
    nl!(ss, "{");
    ss.indent();
    nl!(ss, "i_lo = i;");
    ss.dedent();
    nl!(ss, "}");
    nl!(ss, "else");
    nl!(ss, "{");
    ss.indent();
    nl!(ss, "i_hi = i;");
    ss.dedent();
    nl!(ss, "}");
    nl!(ss, "i = (i_lo + i_hi) / 2;");

    ss.dedent();
    nl!(ss, "}");

    let fk = ss.float_keyword();
    let total = table::TOTAL_SIZE.to_string();
    let lo = format!("({fk}(i_hi) - 1.0 + 0.5) / {fk}({total})");
    let hi = format!("({fk}(i_hi) + 0.5) / {fk}({total})");
    if dimensions == TextureDimensions::Texture1D {
        nl!(
            ss,
            ss.float3_decl("lo")?,
            " = ",
            ss.sample_tex1d(&name, &lo)?,
            ".rgb;"
        );
        nl!(
            ss,
            ss.float3_decl("hi")?,
            " = ",
            ss.sample_tex1d(&name, &hi)?,
            ".rgb;"
        );
    } else {
        nl!(
            ss,
            ss.float3_decl("lo")?,
            " = ",
            ss.sample_tex2d(&name, &ss.float2_const(&lo, "0.5"))?,
            ".rgb;"
        );
        nl!(
            ss,
            ss.float3_decl("hi")?,
            " = ",
            ss.sample_tex2d(&name, &ss.float2_const(&hi, "0.5"))?,
            ".rgb;"
        );
    }

    nl!(
        ss,
        ss.float_decl("t")?,
        " = (h - ",
        hues_array_name,
        "[i_hi - 1]) / (",
        hues_array_name,
        "[i_hi]",
        " - ",
        hues_array_name,
        "[i_hi - 1]);"
    );
    nl!(ss, "return ", ss.lerp("lo", "hi", "t"), ";");

    ss.dedent();
    nl!(ss, "}");

    shader_creator.add_to_helper_shader_code(ss.as_str());

    Ok(name)
}

fn add_focus_gain_func(
    shader_creator: &mut dyn GpuShaderCreator,
    resource_index: u32,
    s: &SharedCompressionParameters,
) -> Result<String> {
    // Reserve name.
    let name = resource_name(shader_creator, &format!("get_focus_gain{resource_index}"));

    let mut ss = GpuShaderText::new(shader_creator.language());

    nl!(ss, ss.float_keyword(), " ", name, "(float J, float cuspJ)");
    nl!(ss, "{");
    ss.indent();

    nl!(
        ss,
        ss.float_decl("thr")?,
        " = ",
        ss.lerp(
            "cuspJ",
            &to_string_f(s.limit_j_max),
            &to_string_f(FOCUS_GAIN_BLEND)
        ),
        ";"
    );

    nl!(ss, "if (J > thr)");
    nl!(ss, "{");
    ss.indent();
    nl!(
        ss,
        ss.float_decl("gain")?,
        " = ( ",
        s.limit_j_max,
        " - thr) / max(0.0001, ",
        s.limit_j_max,
        " - J);"
    );
    nl!(ss, "gain = log(gain)/log(10.0);");
    nl!(ss, "return gain * gain + 1.0;");
    ss.dedent();
    nl!(ss, "}");
    nl!(ss, "else");
    nl!(ss, "{");
    ss.indent();
    nl!(ss, "return 1.0;");
    ss.dedent();
    nl!(ss, "}");

    ss.dedent();
    nl!(ss, "}");

    shader_creator.add_to_helper_shader_code(ss.as_str());

    Ok(name)
}

fn add_solve_j_intersect_func(
    shader_creator: &mut dyn GpuShaderCreator,
    resource_index: u32,
    s: &SharedCompressionParameters,
) -> Result<String> {
    // Reserve name.
    let name = resource_name(
        shader_creator,
        &format!("solve_J_intersect{resource_index}"),
    );

    let mut ss = GpuShaderText::new(shader_creator.language());

    nl!(
        ss,
        ss.float_keyword(),
        " ",
        name,
        "(float J, float M, float focusJ, float slope_gain)"
    );
    nl!(ss, "{");
    ss.indent();

    nl!(ss, ss.float_decl("M_scaled")?, " = M / slope_gain;");
    nl!(ss, ss.float_decl("a")?, " = M_scaled / focusJ;");

    nl!(ss, "if (J < focusJ)");
    nl!(ss, "{");
    ss.indent();
    nl!(ss, ss.float_decl("b")?, " = 1.0 - M_scaled;");
    nl!(ss, ss.float_decl("c")?, " = -J;");
    nl!(ss, ss.float_decl("det")?, " =  b * b - 4.f * a * c;");
    nl!(ss, ss.float_decl("root")?, " =  sqrt(det);");
    nl!(ss, "return -2.0 * c / (b + root);");
    ss.dedent();
    nl!(ss, "}");
    nl!(ss, "else");
    nl!(ss, "{");
    ss.indent();
    nl!(
        ss,
        ss.float_decl("b")?,
        " = - (1.0 + M_scaled + ",
        s.limit_j_max,
        " * a);"
    );
    nl!(
        ss,
        ss.float_decl("c")?,
        " = ",
        s.limit_j_max,
        " * M_scaled + J;"
    );
    nl!(ss, ss.float_decl("det")?, " =  b * b - 4.f * a * c;");
    nl!(ss, ss.float_decl("root")?, " =  sqrt(det);");
    nl!(ss, "return -2.0 * c / (b - root);");
    ss.dedent();
    nl!(ss, "}");

    ss.dedent();
    nl!(ss, "}");

    shader_creator.add_to_helper_shader_code(ss.as_str());

    Ok(name)
}

fn add_find_gamut_boundary_intersection_func(
    shader_creator: &mut dyn GpuShaderCreator,
    resource_index: u32,
    s: &SharedCompressionParameters,
) -> Result<String> {
    // Reserve name.
    let name = resource_name(
        shader_creator,
        &format!("find_gamut_boundary_intersection{resource_index}"),
    );

    let mut ss = GpuShaderText::new(shader_creator.language());
    let lim = s.limit_j_max;

    nl!(
        ss,
        ss.float_keyword(),
        " ",
        name,
        "(",
        ss.float2_keyword(),
        " JM_cusp, float gamma_top_inv, float gamma_bottom_inv, float J_intersect_source, float J_intersect_cusp, float slope)"
    );
    nl!(ss, "{");
    ss.indent();

    nl!(
        ss,
        ss.float_decl("M_boundary_lower")?,
        " = J_intersect_cusp * pow(J_intersect_source / J_intersect_cusp, gamma_bottom_inv) / (JM_cusp.r / JM_cusp.g - slope);"
    );
    nl!(
        ss,
        ss.float_decl("M_boundary_upper")?,
        " = JM_cusp.g * (",
        lim,
        " - J_intersect_cusp) * pow((",
        lim,
        " - J_intersect_source) / (",
        lim,
        " - J_intersect_cusp), gamma_top_inv) / (slope * JM_cusp.g + ",
        lim,
        " - JM_cusp.r);"
    );

    nl!(ss, ss.float_decl("smin")?, " = 0.0;");
    nl!(ss, "{");
    ss.indent();
    nl!(ss, ss.float_decl("a")?, " = M_boundary_lower;");
    nl!(ss, ss.float_decl("b")?, " = M_boundary_upper;");
    nl!(
        ss,
        ss.float_decl("s")?,
        " = ",
        SMOOTH_CUSPS,
        " * JM_cusp.g;"
    );

    nl!(ss, ss.float_decl("h")?, " = max(s - abs(a - b), 0.0) / s;");
    nl!(
        ss,
        "smin = min(a, b) - h * h * h * s * ",
        1.0f64 / 6.0f64,
        ";"
    );

    ss.dedent();
    nl!(ss, "}");

    nl!(ss, "return smin;");

    ss.dedent();
    nl!(ss, "}");

    shader_creator.add_to_helper_shader_code(ss.as_str());

    Ok(name)
}

fn add_compression_func(
    shader_creator: &mut dyn GpuShaderCreator,
    resource_index: u32,
    invert: bool,
) -> Result<String> {
    // Reserve name.
    let name = resource_name(
        shader_creator,
        &format!(
            "remap_M{}{}",
            if invert { "_inv" } else { "_fwd" },
            resource_index
        ),
    );

    let mut ss = GpuShaderText::new(shader_creator.language());

    nl!(
        ss,
        ss.float_keyword(),
        " ",
        name,
        "(float M, float gamut_boundary_M, float reach_boundary_M)"
    );
    nl!(ss, "{");
    ss.indent();

    nl!(
        ss,
        ss.float_decl("boundary_ratio")?,
        " = gamut_boundary_M / reach_boundary_M;"
    );
    nl!(
        ss,
        ss.float_decl("proportion")?,
        " = max(boundary_ratio, ",
        COMPRESSION_THRESHOLD,
        ");"
    );
    nl!(
        ss,
        ss.float_decl("threshold")?,
        " = proportion * gamut_boundary_M;"
    );

    nl!(ss, "if (proportion >= 1.0f || M <= threshold)");
    nl!(ss, "{");
    ss.indent();
    nl!(ss, "return M;");
    ss.dedent();
    nl!(ss, "}");
    nl!(ss, ss.float_decl("m_offset")?, " = M - threshold;");
    nl!(
        ss,
        ss.float_decl("gamut_offset")?,
        " = gamut_boundary_M - threshold;"
    );
    nl!(
        ss,
        ss.float_decl("reach_offset")?,
        " = reach_boundary_M - threshold;"
    );

    nl!(
        ss,
        ss.float_decl("scale")?,
        " = reach_offset / ((reach_offset / gamut_offset) - 1.0f);"
    );
    nl!(ss, ss.float_decl("nd")?, " = m_offset / scale;");

    if invert {
        nl!(ss, "if (nd >= 1.0f)");
        nl!(ss, "{");
        ss.indent();
        nl!(ss, "return threshold + scale;");
        ss.dedent();
        nl!(ss, "}");
        nl!(ss, "else");
        nl!(ss, "{");
        ss.indent();
        nl!(ss, "return threshold + scale * -(nd / (nd - 1.0f));");
        ss.dedent();
        nl!(ss, "}");
    } else {
        nl!(ss, "return threshold + scale * nd / (1.0f + nd);");
    }

    ss.dedent();
    nl!(ss, "}");

    shader_creator.add_to_helper_shader_code(ss.as_str());

    Ok(name)
}

#[allow(clippy::too_many_arguments)]
fn add_compress_gamut_func(
    shader_creator: &mut dyn GpuShaderCreator,
    resource_index: u32,
    s: &SharedCompressionParameters,
    g: &GamutCompressParams,
    get_focus_gain_name: &str,
    find_gamut_boundary_intersection_name: &str,
    compression_name: &str,
    solve_j_intersect_name: &str,
) -> Result<String> {
    // Reserve name.
    let name = resource_name(shader_creator, &format!("gamut_compress{resource_index}"));

    let mut ss = GpuShaderText::new(shader_creator.language());
    let lim = s.limit_j_max;

    nl!(
        ss,
        ss.float3_keyword(),
        " ",
        name,
        "(",
        ss.float3_keyword(),
        " JMh, float Jx, ",
        ss.float3_keyword(),
        " JMGcusp, float reachMaxM)"
    );
    nl!(ss, "{");
    ss.indent();

    nl!(ss, ss.float_decl("J")?, " = JMh.r;");
    nl!(ss, ss.float_decl("M")?, " = JMh.g;");
    nl!(ss, ss.float_decl("h")?, " = JMh.b;");

    nl!(ss, "if (M <= 0.0 || J > ", lim, ")");
    nl!(ss, "{");
    ss.indent();
    nl!(ss, "return ", ss.float3_const("J", "0.0", "h"), ";");
    ss.dedent();
    nl!(ss, "}");
    nl!(ss, "else");
    nl!(ss, "{");
    ss.indent();

    nl!(ss, ss.float2_decl("JMcusp")?, " = JMGcusp.rg;");

    nl!(
        ss,
        ss.float_decl("focusJ")?,
        " = ",
        ss.lerp(
            "JMcusp.r",
            &to_string_f(g.mid_j),
            &format!(
                "min(1.0, {} - (JMcusp.r / {}",
                to_string_f(CUSP_MID_BLEND),
                to_string_f(lim)
            )
        ),
        "));"
    );
    nl!(
        ss,
        ss.float_decl("slope_gain")?,
        " = ",
        lim * g.focus_dist,
        " * ",
        get_focus_gain_name,
        "(Jx, JMcusp.r);"
    );
    nl!(
        ss,
        ss.float_decl("J_intersect_source")?,
        " = ",
        solve_j_intersect_name,
        "(JMh.r, JMh.g, focusJ, slope_gain);"
    );
    nl!(
        ss,
        ss.float_decl("gamut_slope")?,
        " = (J_intersect_source < focusJ) ? J_intersect_source : (",
        lim,
        " - J_intersect_source);"
    );
    nl!(
        ss,
        "gamut_slope = gamut_slope * (J_intersect_source - focusJ) / (focusJ * slope_gain);"
    );

    nl!(ss, ss.float_decl("gamma_top_inv")?, " = JMGcusp.b;");
    nl!(
        ss,
        ss.float_decl("gamma_bottom_inv")?,
        " = ",
        g.lower_hull_gamma_inv,
        ";"
    );

    nl!(
        ss,
        ss.float_decl("J_intersect_cusp")?,
        " = ",
        solve_j_intersect_name,
        "(JMcusp.r, JMcusp.g, focusJ, slope_gain);"
    );
    nl!(
        ss,
        ss.float_decl("gamutBoundaryM")?,
        " = ",
        find_gamut_boundary_intersection_name,
        "(JMcusp, gamma_top_inv, gamma_bottom_inv, J_intersect_source, J_intersect_cusp, gamut_slope);"
    );

    nl!(ss, "if (gamutBoundaryM <= 0.0)");
    nl!(ss, "{");
    ss.indent();
    nl!(ss, "return ", ss.float3_const("J", "0.0", "h"), ";");
    ss.dedent();
    nl!(ss, "}");

    nl!(
        ss,
        ss.float_decl("reachBoundaryM")?,
        " = ",
        lim,
        " * pow(J_intersect_source / ",
        lim,
        ",  ",
        s.model_gamma_inv,
        ");"
    );
    nl!(
        ss,
        "reachBoundaryM = reachBoundaryM / ((",
        lim,
        " / reachMaxM) - gamut_slope);"
    );

    nl!(
        ss,
        ss.float_decl("remapped_M")?,
        " = ",
        compression_name,
        "(M, gamutBoundaryM, reachBoundaryM);"
    );
    nl!(
        ss,
        ss.float_decl("remapped_J")?,
        " = J_intersect_source + remapped_M * gamut_slope;"
    );

    nl!(
        ss,
        "return ",
        ss.float3_const("remapped_J", "remapped_M", "h"),
        ";"
    );

    ss.dedent();
    nl!(ss, "}");

    ss.dedent();
    nl!(ss, "}");

    shader_creator.add_to_helper_shader_code(ss.as_str());

    Ok(name)
}

/// The helper functions of the gamut compression: (cusp table, gamut
/// compress function).
fn add_gamut_compress_helpers(
    shader_creator: &mut dyn GpuShaderCreator,
    resource_index: u32,
    s: &SharedCompressionParameters,
    g: &GamutCompressParams,
    invert: bool,
) -> Result<(String, String)> {
    let cusp_name = add_cusp_table(shader_creator, resource_index, g)?;
    let get_focus_gain_name = add_focus_gain_func(shader_creator, resource_index, s)?;
    let solve_j_intersect_name = add_solve_j_intersect_func(shader_creator, resource_index, s)?;
    let find_gamut_boundary_intersection_name =
        add_find_gamut_boundary_intersection_func(shader_creator, resource_index, s)?;
    let compression_name = add_compression_func(shader_creator, resource_index, invert)?;
    let gamut_compress_name = add_compress_gamut_func(
        shader_creator,
        resource_index,
        s,
        g,
        &get_focus_gain_name,
        &find_gamut_boundary_intersection_name,
        &compression_name,
        &solve_j_intersect_name,
    )?;
    Ok((cusp_name, gamut_compress_name))
}

fn add_gamut_compress_fwd_shader_impl(
    shader_creator: &mut dyn GpuShaderCreator,
    ss: &mut GpuShaderText,
    resource_index: u32,
    s: &SharedCompressionParameters,
    g: &GamutCompressParams,
) -> Result<()> {
    let (cusp_name, gamut_compress_name) =
        add_gamut_compress_helpers(shader_creator, resource_index, s, g, false)?;

    let pxl = shader_creator.pixel_name().to_string();

    nl!(
        ss,
        ss.float3_decl("JMGcusp")?,
        " = ",
        cusp_name,
        "_sample(",
        pxl,
        ".b);"
    );
    nl!(
        ss,
        pxl,
        ".rgb = ",
        gamut_compress_name,
        "(",
        pxl,
        ".rgb, ",
        pxl,
        ".r, JMGcusp, reachMaxM);"
    );
    Ok(())
}

fn add_gamut_compress_inv_shader_impl(
    shader_creator: &mut dyn GpuShaderCreator,
    ss: &mut GpuShaderText,
    resource_index: u32,
    s: &SharedCompressionParameters,
    g: &GamutCompressParams,
) -> Result<()> {
    let (cusp_name, gamut_compress_name) =
        add_gamut_compress_helpers(shader_creator, resource_index, s, g, true)?;

    let pxl = shader_creator.pixel_name().to_string();

    nl!(
        ss,
        ss.float3_decl("JMGcusp")?,
        " = ",
        cusp_name,
        "_sample(",
        pxl,
        ".b);"
    );

    nl!(ss, ss.float_decl("Jx")?, " = ", pxl, ".r;");
    nl!(ss, ss.float3_decl("unCompressedJMh")?, ";");

    // Analytic inverse below threshold.
    nl!(
        ss,
        "if (Jx <= ",
        ss.lerp(
            "JMGcusp.r",
            &to_string_f(s.limit_j_max),
            &to_string_f(FOCUS_GAIN_BLEND)
        ),
        ")"
    );
    nl!(ss, "{");
    ss.indent();
    nl!(
        ss,
        "unCompressedJMh = ",
        gamut_compress_name,
        "(",
        pxl,
        ".rgb, Jx, JMGcusp, reachMaxM);"
    );
    ss.dedent();
    nl!(ss, "}");
    // Approximation above threshold.
    nl!(ss, "else");
    nl!(ss, "{");
    ss.indent();
    nl!(
        ss,
        "Jx = ",
        gamut_compress_name,
        "(",
        pxl,
        ".rgb, Jx, JMGcusp, reachMaxM).r;"
    );
    nl!(
        ss,
        "unCompressedJMh = ",
        gamut_compress_name,
        "(",
        pxl,
        ".rgb, Jx, JMGcusp, reachMaxM);"
    );
    ss.dedent();
    nl!(ss, "}");

    nl!(ss, pxl, ".rgb = unCompressedJMh;");
    Ok(())
}

/// Read 8 float chromaticity coordinates starting at `offset`.
fn primaries_from_params(params: &[f64], offset: usize) -> Result<Primaries> {
    let mut v = [0.0f32; 8];
    for (i, x) in v.iter_mut().enumerate() {
        *x = param(params, offset + i)? as f32;
    }
    Ok(Primaries::from_f32(&v))
}

fn add_aces_output_transform_shader(
    shader_creator: &mut dyn GpuShaderCreator,
    ss: &mut GpuShaderText,
    params: &[f64],
    fwd: bool,
) -> Result<()> {
    let peak_luminance = param(params, 0)? as f32;
    let lim_primaries = primaries_from_params(params, 1)?;

    let p_in = init_jmh_params(&ACES_AP0)?;
    let p_lim = init_jmh_params(&lim_primaries)?;
    let t = init_tone_scale_params(peak_luminance);
    let reach_gamut = init_jmh_params(&ACES_AP1)?;
    let s = init_shared_compression_params(peak_luminance, &p_in, &reach_gamut);
    let c = init_chroma_compress_params(peak_luminance, &t);
    let g = init_gamut_compress_params(peak_luminance, &p_in, &p_lim, &t, &s, &reach_gamut);

    let resource_index = shader_creator.next_resource_index();

    let reach_name = add_reach_table(shader_creator, resource_index, &s.reach_m_table)?;
    let tonescale_name = add_tonescale_func(shader_creator, resource_index, !fwd, &p_in, &t)?;
    let pxl = shader_creator.pixel_name().to_string();

    nl!(ss, "");
    nl!(ss, "// Add RGB to JMh");
    nl!(ss, "");
    add_rgb_to_jmh_shader_impl(&pxl, ss, if fwd { &p_in } else { &p_lim })?;
    add_sin_cos_shader(&pxl, ss)?;

    if fwd {
        nl!(ss, "");
        nl!(ss, "// Add ToneScale and ChromaCompress (fwd)");
        nl!(ss, "");

        nl!(
            ss,
            ss.float_decl("J_ts")?,
            " = ",
            tonescale_name,
            "(",
            pxl,
            ".r);"
        );

        nl!(ss, "// Sample tables (fwd)");
        nl!(
            ss,
            ss.float_decl("reachMaxM")?,
            " = ",
            reach_name,
            "_sample(",
            pxl,
            ".b);"
        );

        nl!(ss, "");

        nl!(ss, "{");
        ss.indent();
        add_tonescale_compress_fwd_shader_impl(shader_creator, ss, resource_index, &s, &c)?;
        ss.dedent();
        nl!(ss, "}");

        nl!(ss, "");
        nl!(ss, "// Add GamutCompress (fwd)");
        nl!(ss, "");
        nl!(ss, "{");
        ss.indent();
        add_gamut_compress_fwd_shader_impl(shader_creator, ss, resource_index, &s, &g)?;
        ss.dedent();
        nl!(ss, "}");
    } else {
        nl!(
            ss,
            ss.float_decl("reachMaxM")?,
            " = ",
            reach_name,
            "_sample(",
            pxl,
            ".b);"
        );
        nl!(ss, "");
        nl!(ss, "// Add GamutCompress (inv)");
        nl!(ss, "");
        nl!(ss, "{");
        ss.indent();
        add_gamut_compress_inv_shader_impl(shader_creator, ss, resource_index, &s, &g)?;
        ss.dedent();
        nl!(ss, "}");

        nl!(ss, "");
        nl!(ss, "// Add ToneScale and ChromaCompress (inv)");
        nl!(ss, "");
        nl!(
            ss,
            ss.float_decl("J")?,
            " = ",
            tonescale_name,
            "(",
            pxl,
            ".r);"
        );
        nl!(ss, "{");
        ss.indent();
        add_tonescale_compress_inv_shader_impl(shader_creator, ss, resource_index, &s, &c)?;
        ss.dedent();
        nl!(ss, "}");
    }

    nl!(ss, "");
    nl!(ss, "// Add JMh to RGB");
    nl!(ss, "");
    nl!(ss, "{");
    ss.indent();
    add_jmh_to_rgb_shader_impl(&pxl, ss, if fwd { &p_lim } else { &p_in })?;
    ss.dedent();
    nl!(ss, "}");
    Ok(())
}

fn add_rgb_to_jmh_shader(pxl: &str, ss: &mut GpuShaderText, params: &[f64]) -> Result<()> {
    let p = init_jmh_params(&primaries_from_params(params, 0)?)?;
    add_rgb_to_jmh_shader_impl(pxl, ss, &p)
}

fn add_jmh_to_rgb_shader(pxl: &str, ss: &mut GpuShaderText, params: &[f64]) -> Result<()> {
    let p = init_jmh_params(&primaries_from_params(params, 0)?)?;
    add_wrap_hue_channel_shader(pxl, ss)?;
    add_sin_cos_shader(pxl, ss)?;
    add_jmh_to_rgb_shader_impl(pxl, ss, &p)
}

fn add_rgb_to_hmj_shader(pxl: &str, ss: &mut GpuShaderText, params: &[f64]) -> Result<()> {
    // Compute JMh (in pxl.rgb) and then repack it as HMJ, scaled for a
    // [0,1]-ish range.
    add_rgb_to_jmh_shader(pxl, ss, params)?;

    nl!(
        ss,
        pxl,
        ".rgb = ",
        ss.float3_const(
            format!("{pxl}.b / 360.0"),
            format!("{pxl}.g / 200.0"),
            format!("{pxl}.r / 100.0")
        ),
        ";"
    );
    Ok(())
}

fn add_hmj_to_rgb_shader(pxl: &str, ss: &mut GpuShaderText, params: &[f64]) -> Result<()> {
    // Unpack HMJ back into JMh (in pxl.rgb) and reuse the existing JMh to RGB
    // conversion.
    nl!(
        ss,
        pxl,
        ".rgb = ",
        ss.float3_const(
            format!("{pxl}.b * 100.0"),
            format!("{pxl}.g * 200.0"),
            format!("{pxl}.r * 360.0")
        ),
        ";"
    );

    add_jmh_to_rgb_shader(pxl, ss, params)
}

fn add_tonescale_compress_shader(
    shader_creator: &mut dyn GpuShaderCreator,
    ss: &mut GpuShaderText,
    params: &[f64],
    fwd: bool,
) -> Result<()> {
    let peak_luminance = param(params, 0)? as f32;

    let p = init_jmh_params(&ACES_AP0)?;
    let t = init_tone_scale_params(peak_luminance);
    let reach_gamut = init_jmh_params(&ACES_AP1)?;
    let s = init_shared_compression_params(peak_luminance, &p, &reach_gamut);
    let c = init_chroma_compress_params(peak_luminance, &t);

    let resource_index = shader_creator.next_resource_index();
    let pxl = shader_creator.pixel_name().to_string();

    let reach_name = add_reach_table(shader_creator, resource_index, &s.reach_m_table)?;
    let tonescale_name = add_tonescale_func(shader_creator, resource_index, !fwd, &p, &t)?;

    add_wrap_hue_channel_shader(&pxl, ss)?;
    add_sin_cos_shader(&pxl, ss)?;

    nl!(
        ss,
        ss.float_decl("reachMaxM")?,
        " = ",
        reach_name,
        "_sample(",
        pxl,
        ".b);"
    );
    if fwd {
        nl!(
            ss,
            ss.float_decl("J_ts")?,
            " = ",
            tonescale_name,
            "(",
            pxl,
            ".r);"
        );
        add_tonescale_compress_fwd_shader_impl(shader_creator, ss, resource_index, &s, &c)
    } else {
        nl!(
            ss,
            ss.float_decl("J")?,
            " = ",
            tonescale_name,
            "(",
            pxl,
            ".r);"
        );
        add_tonescale_compress_inv_shader_impl(shader_creator, ss, resource_index, &s, &c)
    }
}

fn add_gamut_compress_shader(
    shader_creator: &mut dyn GpuShaderCreator,
    ss: &mut GpuShaderText,
    params: &[f64],
    fwd: bool,
) -> Result<()> {
    let peak_luminance = param(params, 0)? as f32;
    let primaries = primaries_from_params(params, 1)?;

    let p_in = init_jmh_params(&ACES_AP0)?;
    let p_lim = init_jmh_params(&primaries)?;
    let t = init_tone_scale_params(peak_luminance);
    let reach_gamut = init_jmh_params(&ACES_AP1)?;
    let s = init_shared_compression_params(peak_luminance, &p_in, &reach_gamut);
    let g = init_gamut_compress_params(peak_luminance, &p_in, &p_lim, &t, &s, &reach_gamut);

    let resource_index = shader_creator.next_resource_index();
    let pxl = shader_creator.pixel_name().to_string();

    let reach_name = add_reach_table(shader_creator, resource_index, &s.reach_m_table)?;

    add_wrap_hue_channel_shader(&pxl, ss)?;
    add_sin_cos_shader(&pxl, ss)?;

    nl!(
        ss,
        ss.float_decl("reachMaxM")?,
        " = ",
        reach_name,
        "_sample(",
        pxl,
        ".b);"
    );

    if fwd {
        add_gamut_compress_fwd_shader_impl(shader_creator, ss, resource_index, &s, &g)
    } else {
        add_gamut_compress_inv_shader_impl(shader_creator, ss, resource_index, &s, &g)
    }
}

fn add_surround_10_fwd_shader(pxl: &str, ss: &mut GpuShaderText, gamma: f32) -> Result<()> {
    nl!(
        ss,
        ss.float_decl("Y")?,
        " = max( 1e-10, 0.27222871678091454 * ",
        pxl,
        ".rgb.r + ",
        "0.67408176581114831 * ",
        pxl,
        ".rgb.g + ",
        "0.053689517407937051 * ",
        pxl,
        ".rgb.b );"
    );

    nl!(
        ss,
        ss.float_decl("Ypow_over_Y")?,
        " = pow( Y, ",
        gamma - 1.0f32,
        ");"
    );

    nl!(ss, pxl, ".rgb = ", pxl, ".rgb * Ypow_over_Y;");
    Ok(())
}

fn add_rec2100_surround_shader(
    pxl: &str,
    ss: &mut GpuShaderText,
    gamma: f32,
    is_forward: bool,
) -> Result<()> {
    let mut gamma = gamma;
    let mut min_lum: f32 = 1e-4;
    if !is_forward {
        min_lum = min_lum.powf(gamma);
        gamma = 1.0f32 / gamma;
    }

    nl!(
        ss,
        ss.float_decl("Y")?,
        " = 0.2627 * ",
        pxl,
        ".rgb.r + ",
        "0.6780 * ",
        pxl,
        ".rgb.g + ",
        "0.0593 * ",
        pxl,
        ".rgb.b;"
    );

    nl!(ss, "Y = max( ", min_lum, ", abs(Y) );");

    nl!(
        ss,
        ss.float_decl("Ypow_over_Y")?,
        " = pow( Y, ",
        gamma - 1.0f32,
        ");"
    );

    nl!(ss, "", pxl, ".rgb = ", pxl, ".rgb * Ypow_over_Y;");
    Ok(())
}

fn add_rgb_to_hsv(pxl: &str, ss: &mut GpuShaderText) -> Result<()> {
    nl!(
        ss,
        ss.float_decl("minRGB")?,
        " = min( ",
        pxl,
        ".rgb.r, min( ",
        pxl,
        ".rgb.g, ",
        pxl,
        ".rgb.b ) );"
    );
    nl!(
        ss,
        ss.float_decl("maxRGB")?,
        " = max( ",
        pxl,
        ".rgb.r, max( ",
        pxl,
        ".rgb.g, ",
        pxl,
        ".rgb.b ) );"
    );
    nl!(ss, ss.float_decl("val")?, " = maxRGB;");

    nl!(ss, ss.float_decl("sat")?, " = 0.0, hue = 0.0;");
    nl!(ss, "if (minRGB != maxRGB)");
    nl!(ss, "{");
    ss.indent();

    nl!(ss, "if (val != 0.0) sat = (maxRGB - minRGB) / val;");
    nl!(
        ss,
        ss.float_decl("OneOverMaxMinusMin")?,
        " = 1.0 / (maxRGB - minRGB);"
    );
    nl!(
        ss,
        "if ( maxRGB == ",
        pxl,
        ".rgb.r ) hue = (",
        pxl,
        ".rgb.g - ",
        pxl,
        ".rgb.b) * OneOverMaxMinusMin;"
    );
    nl!(
        ss,
        "else if ( maxRGB == ",
        pxl,
        ".rgb.g ) hue = 2.0 + (",
        pxl,
        ".rgb.b - ",
        pxl,
        ".rgb.r) * OneOverMaxMinusMin;"
    );
    nl!(
        ss,
        "else hue = 4.0 + (",
        pxl,
        ".rgb.r - ",
        pxl,
        ".rgb.g) * OneOverMaxMinusMin;"
    );
    nl!(ss, "if ( hue < 0.0 ) hue += 6.0;");

    ss.dedent();
    nl!(ss, "}");

    nl!(ss, "if ( minRGB < 0.0 ) val += minRGB;");
    nl!(
        ss,
        "if ( -minRGB > maxRGB ) sat = (maxRGB - minRGB) / -minRGB;"
    );

    nl!(
        ss,
        pxl,
        ".rgb = ",
        ss.float3_const("hue * 1./6.", "sat", "val"),
        ";"
    );
    Ok(())
}

fn add_rgb_to_hsy(
    pxl: &str,
    ss: &mut GpuShaderText,
    func_style: FixedFunctionOpStyle,
) -> Result<()> {
    nl!(
        ss,
        ss.float3_decl("lumaWeights")?,
        " = ",
        ss.float3_const(0.2126f32, 0.7152f32, 0.0722f32),
        ";"
    );
    nl!(
        ss,
        ss.float3_decl("ones")?,
        " = ",
        ss.float3_const(1.0f32, 1.0f32, 1.0f32),
        ";"
    );
    nl!(ss, "float luma = dot(", pxl, ".rgb, lumaWeights);");
    nl!(
        ss,
        "float minRGB =  min( ",
        pxl,
        ".x, min( ",
        pxl,
        ".y, ",
        pxl,
        ".z ) );"
    );
    nl!(
        ss,
        "float maxRGB =  max( ",
        pxl,
        ".x, max( ",
        pxl,
        ".y, ",
        pxl,
        ".z ) );"
    );
    nl!(ss, ss.float3_decl("RGBm")?, " = ", pxl, ".rgb - luma;");
    nl!(ss, "float distRGB  = dot( abs(RGBm), ones );");
    if func_style == S::RgbToHsyLin {
        nl!(ss, "float sumRGB  = dot( ", pxl, ".rgb, ones );");
        nl!(
            ss,
            "float sat_hi  = distRGB / max(0.07 * distRGB + 1e-6, 0.15 + sumRGB);"
        );
        nl!(ss, "float sat_lo  = distRGB * 5.;");
        nl!(
            ss,
            "float alpha  = clamp( (luma - 0.001) / (0.01 - 0.001), 0., 1.);"
        );

        nl!(ss, "float sat = sat_lo + alpha * (sat_hi - sat_lo);");
        nl!(ss, "sat *= 1.4;");
    } else if func_style == S::RgbToHsyLog {
        nl!(ss, "float sat = distRGB * 4.;");
    } else {
        // RGB_TO_HSY_VID
        nl!(ss, "float sat = distRGB * 1.25;");
    }
    // NB: Unlike typical HSV, HSY maps magenta rather than red to a hue of
    // zero. (This allows for better placement of red when manipulating curves
    // in a UI.)
    nl!(ss, "float hue = 0.0;");
    nl!(ss, "if (minRGB != maxRGB) {");
    nl!(ss, "   float OneOverMaxMinusMin = 1.0 / (maxRGB - minRGB);");
    nl!(
        ss,
        "   if ( maxRGB == ",
        pxl,
        ".r ) hue = 1.0 + (",
        pxl,
        ".g - ",
        pxl,
        ".b) * OneOverMaxMinusMin;"
    );
    nl!(
        ss,
        "   else if ( maxRGB == ",
        pxl,
        ".g ) hue = 3.0 + (",
        pxl,
        ".b - ",
        pxl,
        ".r) * OneOverMaxMinusMin;"
    );
    nl!(
        ss,
        "   else hue = 5.0 + (",
        pxl,
        ".r - ",
        pxl,
        ".g) * OneOverMaxMinusMin;"
    );
    nl!(ss, "}");
    nl!(
        ss,
        "",
        pxl,
        ".r = hue * 1./6.; ",
        pxl,
        ".g = sat; ",
        pxl,
        ".b = luma;"
    );
    Ok(())
}

fn add_hsy_to_rgb(
    pxl: &str,
    ss: &mut GpuShaderText,
    func_style: FixedFunctionOpStyle,
) -> Result<()> {
    nl!(ss, "float luma = ", pxl, ".z;");
    nl!(ss, "float Hue = ", pxl, ".x - 1./6.;");
    nl!(ss, "Hue = (luma < 0.) ? Hue + 0.5 : Hue;");
    nl!(ss, "Hue = ( Hue - floor( Hue ) ) * 6.0;");
    nl!(ss, "float R = abs(Hue - 3.0) - 1.0;");
    nl!(ss, "float G = 2.0 - abs(Hue - 2.0);");
    nl!(ss, "float B = 2.0 - abs(Hue - 4.0);");
    nl!(
        ss,
        ss.float3_decl("RGB0")?,
        " = ",
        ss.float3_const("R", "G", "B"),
        ";"
    );
    nl!(ss, "RGB0 = clamp( RGB0, 0., 1. );");

    nl!(
        ss,
        ss.float3_decl("lumaWeights")?,
        " = ",
        ss.float3_const(0.2126f32, 0.7152f32, 0.0722f32),
        ";"
    );
    nl!(
        ss,
        ss.float3_decl("ones")?,
        " = ",
        ss.float3_const(1.0f32, 1.0f32, 1.0f32),
        ";"
    );
    nl!(ss, "float currY = dot(RGB0, lumaWeights);");
    nl!(ss, "RGB0 *= luma / currY;");

    nl!(ss, "float sat = ", pxl, ".y;");
    nl!(ss, "float distRGB = dot( abs(RGB0 - luma), ones );");
    if func_style == S::HsyLinToRgb {
        nl!(ss, "float sumRGB  = dot( RGB0, ones );");
        nl!(ss, "float k = 0.15;");
        nl!(ss, "float lo_gain = 5.;");
        nl!(ss, "sat /= 1.4;");
        nl!(ss, "float tmp = -sat * sumRGB + sat * 3. * luma + distRGB;");
        nl!(ss, "tmp = max(1e-6, tmp);");
        nl!(ss, "float s1 = sat * (k + 3. * luma) / tmp;");
        nl!(ss, "s1 = min(s1, 50.);");
        nl!(ss, "float s0 = sat / max(1e-10, distRGB * lo_gain);");
        nl!(
            ss,
            "float alpha  = clamp( (luma - 0.001) / (0.01 - 0.001), 0., 1.);"
        );
        nl!(
            ss,
            "float a = distRGB * lo_gain * (1. - alpha) * (sumRGB - 3. * luma);"
        );
        nl!(
            ss,
            "float b = distRGB * lo_gain * (1. - alpha) * (k + 3. * luma) + distRGB * alpha - sat * (sumRGB - 3. * luma);"
        );
        nl!(ss, "float c = -sat * (k + 3. * luma);");
        nl!(ss, "float discrim = sqrt( b * b - 4. * a * c );");
        nl!(ss, "float denom = -discrim - b;");
        nl!(ss, "float sm = (2. * c) / denom;");
        nl!(
            ss,
            "sm = (sm >= 0.) ? sm : (2. * c) / (denom + discrim * 2.);"
        );
        nl!(
            ss,
            "float gainS = (alpha == 1.) ? s1 : (alpha == 0.) ? s0 : sm;"
        );
    } else if func_style == S::HsyLogToRgb {
        nl!(ss, "float gainS = sat / max(1e-10, distRGB * 4.);");
    } else {
        // HSY_VID_TO_RGB
        nl!(ss, "float gainS = sat / max(1e-10, distRGB * 1.25);");
    }
    nl!(ss, "", pxl, ".rgb = luma + gainS * (RGB0 - luma);");
    Ok(())
}

fn add_hsv_to_rgb(pxl: &str, ss: &mut GpuShaderText) -> Result<()> {
    nl!(
        ss,
        ss.float_decl("Hue")?,
        " = ( ",
        pxl,
        ".rgb.r - floor( ",
        pxl,
        ".rgb.r ) ) * 6.0;"
    );
    nl!(
        ss,
        ss.float_decl("Sat")?,
        " = clamp( ",
        pxl,
        ".rgb.g, 0., 1.999 );"
    );
    nl!(ss, ss.float_decl("Val")?, " = ", pxl, ".rgb.b;");

    nl!(ss, ss.float_decl("R")?, " = abs(Hue - 3.0) - 1.0;");
    nl!(ss, ss.float_decl("G")?, " = 2.0 - abs(Hue - 2.0);");
    nl!(ss, ss.float_decl("B")?, " = 2.0 - abs(Hue - 4.0);");
    nl!(
        ss,
        ss.float3_decl("RGB")?,
        " = ",
        ss.float3_const("R", "G", "B"),
        ";"
    );
    nl!(ss, "RGB = clamp( RGB, 0., 1. );");

    nl!(ss, ss.float_keyword(), " rgbMax = Val;");
    nl!(ss, ss.float_keyword(), " rgbMin = Val * (1.0 - Sat);");

    nl!(ss, "if ( Sat > 1.0 )");
    nl!(ss, "{");
    ss.indent();
    nl!(ss, "rgbMin = Val * (1.0 - Sat) / (2.0 - Sat);");
    nl!(ss, "rgbMax = Val - rgbMin;");
    ss.dedent();
    nl!(ss, "}");
    nl!(ss, "if ( Val < 0.0 )");
    nl!(ss, "{");
    ss.indent();
    nl!(ss, "rgbMin = Val / (2.0 - Sat);");
    nl!(ss, "rgbMax = Val - rgbMin;");
    ss.dedent();
    nl!(ss, "}");

    nl!(ss, "RGB = RGB * (rgbMax - rgbMin) + rgbMin;");

    nl!(ss, "", pxl, ".rgb = RGB;");
    Ok(())
}

fn add_xyz_to_xyy(pxl: &str, ss: &mut GpuShaderText) -> Result<()> {
    nl!(
        ss,
        ss.float_decl("d")?,
        " = ",
        pxl,
        ".rgb.r + ",
        pxl,
        ".rgb.g + ",
        pxl,
        ".rgb.b;"
    );
    nl!(ss, "d = (d == 0.) ? 0. : 1. / d;");
    nl!(ss, pxl, ".rgb.b = ", pxl, ".rgb.g;");
    nl!(ss, pxl, ".rgb.r *= d;");
    nl!(ss, pxl, ".rgb.g *= d;");
    Ok(())
}

fn add_xyy_to_xyz(pxl: &str, ss: &mut GpuShaderText) -> Result<()> {
    nl!(
        ss,
        ss.float_decl("d")?,
        " = (",
        pxl,
        ".rgb.g == 0.) ? 0. : 1. / ",
        pxl,
        ".rgb.g;"
    );
    nl!(ss, ss.float_decl("Y")?, " = ", pxl, ".rgb.b;");
    nl!(
        ss,
        pxl,
        ".rgb.b = Y * (1. - ",
        pxl,
        ".rgb.r - ",
        pxl,
        ".rgb.g) * d;"
    );
    nl!(ss, pxl, ".rgb.r *= Y * d;");
    nl!(ss, pxl, ".rgb.g = Y;");
    Ok(())
}

fn add_xyz_to_uvy(pxl: &str, ss: &mut GpuShaderText) -> Result<()> {
    nl!(
        ss,
        ss.float_decl("d")?,
        " = ",
        pxl,
        ".rgb.r + 15. * ",
        pxl,
        ".rgb.g + 3. * ",
        pxl,
        ".rgb.b;"
    );
    nl!(ss, "d = (d == 0.) ? 0. : 1. / d;");
    nl!(ss, pxl, ".rgb.b = ", pxl, ".rgb.g;");
    nl!(ss, pxl, ".rgb.r *= 4. * d;");
    nl!(ss, pxl, ".rgb.g *= 9. * d;");
    Ok(())
}

fn add_uvy_to_xyz(pxl: &str, ss: &mut GpuShaderText) -> Result<()> {
    nl!(
        ss,
        ss.float_decl("d")?,
        " = (",
        pxl,
        ".rgb.g == 0.) ? 0. : 1. / ",
        pxl,
        ".rgb.g;"
    );
    nl!(ss, ss.float_decl("Y")?, " = ", pxl, ".rgb.b;");
    nl!(
        ss,
        pxl,
        ".rgb.b = (3./4.) * Y * (4. - ",
        pxl,
        ".rgb.r - 6.6666666666666667 * ",
        pxl,
        ".rgb.g) * d;"
    );
    nl!(ss, pxl, ".rgb.r *= (9./4.) * Y * d;");
    nl!(ss, pxl, ".rgb.g = Y;");
    Ok(())
}

fn add_xyz_to_luv(pxl: &str, ss: &mut GpuShaderText) -> Result<()> {
    nl!(
        ss,
        ss.float_decl("d")?,
        " = ",
        pxl,
        ".rgb.r + 15. * ",
        pxl,
        ".rgb.g + 3. * ",
        pxl,
        ".rgb.b;"
    );
    nl!(ss, "d = (d == 0.) ? 0. : 1. / d;");
    nl!(ss, ss.float_decl("u")?, " = ", pxl, ".rgb.r * 4. * d;");
    nl!(ss, ss.float_decl("v")?, " = ", pxl, ".rgb.g * 9. * d;");
    nl!(ss, ss.float_decl("Y")?, " = ", pxl, ".rgb.g;");

    nl!(
        ss,
        ss.float_decl("Lstar")?,
        " = ",
        ss.lerp(
            "1.16 * pow( max(0., Y), 1./3. ) - 0.16",
            "9.0329629629629608 * Y",
            "float(Y <= 0.008856451679)"
        ),
        ";"
    );
    nl!(
        ss,
        ss.float_decl("ustar")?,
        " = 13. * Lstar * (u - 0.19783001);"
    );
    nl!(
        ss,
        ss.float_decl("vstar")?,
        " = 13. * Lstar * (v - 0.46831999);"
    );

    nl!(
        ss,
        pxl,
        ".rgb = ",
        ss.float3_const("Lstar", "ustar", "vstar"),
        ";"
    );
    Ok(())
}

fn add_luv_to_xyz(pxl: &str, ss: &mut GpuShaderText) -> Result<()> {
    nl!(ss, ss.float_decl("Lstar")?, " = ", pxl, ".rgb.r;");
    nl!(
        ss,
        ss.float_decl("d")?,
        " = (Lstar == 0.) ? 0. : 0.076923076923076927 / Lstar;"
    );
    nl!(
        ss,
        ss.float_decl("u")?,
        " = ",
        pxl,
        ".rgb.g * d + 0.19783001;"
    );
    nl!(
        ss,
        ss.float_decl("v")?,
        " = ",
        pxl,
        ".rgb.b * d + 0.46831999;"
    );

    nl!(
        ss,
        ss.float_decl("tmp")?,
        " = (Lstar + 0.16) * 0.86206896551724144;"
    );
    nl!(
        ss,
        ss.float_decl("Y")?,
        " = ",
        ss.lerp(
            "tmp * tmp * tmp",
            "0.11070564598794539 * Lstar",
            "float(Lstar <= 0.08)"
        ),
        ";"
    );

    nl!(ss, ss.float_decl("dd")?, " = (v == 0.) ? 0. : 0.25 / v;");
    nl!(ss, pxl, ".rgb.r = 9. * Y * u * dd;");
    nl!(ss, pxl, ".rgb.b = Y * (12. - 3. * u - 20. * v) * dd;");
    nl!(ss, pxl, ".rgb.g = Y;");
    Ok(())
}

// ST 2084 constants.
const ST2084_M1: f64 = 0.25 * 2610. / 4096.;
const ST2084_M2: f64 = 128. * 2523. / 4096.;
const ST2084_C2: f64 = 32. * 2413. / 4096.;
const ST2084_C3: f64 = 32. * 2392. / 4096.;
const ST2084_C1: f64 = ST2084_C3 - ST2084_C2 + 1.;

fn add_lin_to_pq(pxl: &str, ss: &mut GpuShaderText) -> Result<()> {
    nl!(ss, ss.float3_decl("sign3")?, " = sign(", pxl, ".rgb);");
    nl!(ss, ss.float3_decl("L")?, " = abs(0.01 * ", pxl, ".rgb);");
    nl!(
        ss,
        ss.float3_decl("y")?,
        " = pow(L, ",
        ss.float3_const1(ST2084_M1),
        ");"
    );
    nl!(
        ss,
        ss.float3_decl("ratpoly")?,
        " = (",
        ss.float3_const1(ST2084_C1),
        " + ",
        ST2084_C2,
        " * y) / (",
        ss.float3_const1(1.0f64),
        " + ",
        ST2084_C3,
        " * y);"
    );
    nl!(
        ss,
        pxl,
        ".rgb = sign3 * pow(ratpoly, ",
        ss.float3_const1(ST2084_M2),
        ");"
    );

    // The sign transfer here is very slightly different than in the CPU
    // path, resulting in a PQ value of 0 at 0 rather than the true value of
    // 0.836^78.84 = 7.36e-07, however, this is well below visual threshold.
    Ok(())
}

fn add_pq_to_lin(pxl: &str, ss: &mut GpuShaderText) -> Result<()> {
    nl!(ss, ss.float3_decl("sign3")?, " = sign(", pxl, ".rgb);");
    nl!(
        ss,
        ss.float3_decl("x")?,
        " = pow(abs(",
        pxl,
        ".rgb), ",
        ss.float3_const1(1.0 / ST2084_M2),
        ");"
    );
    nl!(
        ss,
        pxl,
        ".rgb = 100. * sign3 * pow(max(",
        ss.float3_const1(0.0f64),
        ", x - ",
        ss.float3_const1(ST2084_C1),
        ") / (",
        ss.float3_const1(ST2084_C2),
        " - ",
        ST2084_C3,
        " * x), ",
        ss.float3_const1(1.0 / ST2084_M1),
        ");"
    );
    Ok(())
}

fn add_lin_to_gamma_log(pxl: &str, ss: &mut GpuShaderText, params: &[f64]) -> Result<()> {
    // Get parameters, baking the log base conversion into 'logSlope'.
    let mirror_pt = param(params, 0)?;
    let break_pt = param(params, 1)?;
    let gamma_seg_power = param(params, 2)?;
    let gamma_seg_slope = param(params, 3)?;
    let gamma_seg_off = param(params, 4)?;
    let log_seg_base = param(params, 5)?;
    let log_seg_log_slope = param(params, 6)? / log_seg_base.ln();
    let log_seg_log_off = param(params, 7)?;
    let log_seg_lin_slope = param(params, 8)?;
    let log_seg_lin_off = param(params, 9)?;

    nl!(
        ss,
        ss.float3_decl("mirrorin")?,
        " = ",
        pxl,
        ".rgb - ",
        ss.float3_const1(mirror_pt),
        ";"
    );
    nl!(ss, ss.float3_decl("sign3")?, " = sign(mirrorin);");
    nl!(
        ss,
        ss.float3_decl("E")?,
        " = abs(mirrorin) + ",
        ss.float3_const1(mirror_pt),
        ";"
    );
    nl!(
        ss,
        ss.float3_decl("isAboveBreak")?,
        " = ",
        ss.float3_greater_than("E", &ss.float3_const1(break_pt)),
        ";"
    );
    nl!(
        ss,
        ss.float3_decl("isAtOrBelowBreak")?,
        " = ",
        ss.float3_const1(1.0f32),
        " - isAboveBreak;"
    );

    nl!(
        ss,
        ss.float3_decl("Ep_gamma")?,
        " = ",
        ss.float3_const1(gamma_seg_slope),
        " * pow( E - ",
        ss.float3_const1(gamma_seg_off),
        ", ",
        ss.float3_const1(gamma_seg_power),
        ");"
    );

    // Avoid NaNs by clamping log input below 1 if the branch will not be
    // used.
    nl!(
        ss,
        ss.float3_decl("Ep_clamped")?,
        " = max( isAtOrBelowBreak, E * ",
        ss.float3_const1(log_seg_lin_slope),
        " + ",
        ss.float3_const1(log_seg_lin_off),
        " );"
    );
    nl!(
        ss,
        ss.float3_decl("Ep_log")?,
        " = ",
        ss.float3_const1(log_seg_log_slope),
        " * log( Ep_clamped ) + ",
        ss.float3_const1(log_seg_log_off),
        ";"
    );

    // Combine log and gamma parts.
    nl!(
        ss,
        pxl,
        ".rgb = sign3 * (isAboveBreak * Ep_log + ( ",
        ss.float3_const1(1.0f32),
        " - isAboveBreak ) * Ep_gamma);"
    );
    Ok(())
}

fn add_gamma_log_to_lin(pxl: &str, ss: &mut GpuShaderText, params: &[f64]) -> Result<()> {
    // Get parameters, baking the log base conversion into 'logSlope'.
    let mirror_pt = param(params, 0)?;
    let break_pt = param(params, 1)?;
    let gamma_seg_power = param(params, 2)?;
    let gamma_seg_slope = param(params, 3)?;
    let gamma_seg_off = param(params, 4)?;
    let log_seg_base = param(params, 5)?;
    let log_seg_log_slope = param(params, 6)? / log_seg_base.ln();
    let log_seg_log_off = param(params, 7)?;
    let log_seg_lin_slope = param(params, 8)?;
    let log_seg_lin_off = param(params, 9)?;

    let prime_break = gamma_seg_slope * (break_pt + gamma_seg_off).powf(gamma_seg_power);
    let prime_mirror = gamma_seg_slope * (mirror_pt + gamma_seg_off).powf(gamma_seg_power);

    nl!(
        ss,
        ss.float3_decl("mirrorin")?,
        " = ",
        pxl,
        ".rgb - ",
        ss.float3_const1(prime_mirror),
        ";"
    );
    nl!(ss, ss.float3_decl("sign3")?, " = sign(mirrorin);");
    nl!(
        ss,
        ss.float3_decl("Eprime")?,
        " = abs(mirrorin) + ",
        ss.float3_const1(prime_mirror),
        ";"
    );
    nl!(
        ss,
        ss.float3_decl("isAboveBreak")?,
        " = ",
        ss.float3_greater_than("Eprime", &ss.float3_const1(prime_break)),
        ";"
    );

    // Gamma Segment.
    nl!(
        ss,
        ss.float3_decl("E_gamma")?,
        " = pow( Eprime * ",
        ss.float3_const1(1.0 / gamma_seg_slope),
        ",",
        ss.float3_const1(1.0 / gamma_seg_power),
        ") - ",
        ss.float3_const1(gamma_seg_off),
        ";"
    );

    // Log Segment.
    nl!(
        ss,
        ss.float3_decl("E_log")?,
        " = (exp((Eprime - ",
        ss.float3_const1(log_seg_log_off),
        ") * ",
        ss.float3_const1(1.0 / log_seg_log_slope),
        ") - ",
        ss.float3_const1(log_seg_lin_off),
        ") * ",
        ss.float3_const1(1.0 / log_seg_lin_slope),
        ";"
    );

    // Combine log and gamma parts.
    nl!(
        ss,
        pxl,
        ".rgb = sign3 * (isAboveBreak * E_log + ( ",
        ss.float3_const1(1.0f32),
        " - isAboveBreak ) * E_gamma);"
    );
    Ok(())
}

/// The parameters of the double log styles.
struct DoubleLogParams {
    break1: f64,
    break2: f64,
    log_seg1_log_slope: f64,
    log_seg1_log_off: f64,
    log_seg1_lin_slope: f64,
    log_seg1_lin_off: f64,
    log_seg2_log_slope: f64,
    log_seg2_log_off: f64,
    log_seg2_lin_slope: f64,
    log_seg2_lin_off: f64,
    lin_seg_slope: f64,
    lin_seg_off: f64,
}

impl DoubleLogParams {
    /// Get the parameters, baking the log base conversion into 'logSlope'.
    fn new(params: &[f64]) -> Result<Self> {
        let base = param(params, 0)?;
        Ok(Self {
            break1: param(params, 1)?,
            break2: param(params, 2)?,
            log_seg1_log_slope: param(params, 3)? / base.ln(),
            log_seg1_log_off: param(params, 4)?,
            log_seg1_lin_slope: param(params, 5)?,
            log_seg1_lin_off: param(params, 6)?,
            log_seg2_log_slope: param(params, 7)? / base.ln(),
            log_seg2_log_off: param(params, 8)?,
            log_seg2_lin_slope: param(params, 9)?,
            log_seg2_lin_off: param(params, 10)?,
            lin_seg_slope: param(params, 11)?,
            lin_seg_off: param(params, 12)?,
        })
    }
}

fn add_lin_to_double_log(pix: &str, ss: &mut GpuShaderText, params: &[f64]) -> Result<()> {
    let p = DoubleLogParams::new(params)?;

    // Linear segment may not exist or be valid, thus we include the break
    // points in the log segments. Also passing zero or negative value to the
    // log functions are not guarded for, it should be guaranteed by the
    // parameters for the expected working range.
    let pix3 = format!("{pix}.rgb");

    nl!(
        ss,
        ss.float3_decl("isSegment1")?,
        " = ",
        ss.float3_greater_than_equal(&ss.float3_const1(p.break1), &pix3),
        ";"
    );
    nl!(
        ss,
        ss.float3_decl("isSegment3")?,
        " = ",
        ss.float3_greater_than_equal(&pix3, &ss.float3_const1(p.break2)),
        ";"
    );
    nl!(
        ss,
        ss.float3_decl("isSegment2")?,
        " = ",
        ss.float3_const1(1.0f32),
        " - isSegment1 - isSegment3;"
    );

    // Log Segment 1.
    nl!(ss);
    nl!(
        ss,
        ss.float3_decl("logSeg1")?,
        " = ",
        pix3,
        " * ",
        ss.float3_const1(p.log_seg1_lin_slope),
        " + ",
        ss.float3_const1(p.log_seg1_lin_off),
        ";"
    );

    // Clamp below 1 to avoid NaNs if the branch will not be used.
    nl!(
        ss,
        "logSeg1 = max( ",
        ss.float3_const1(1.0f64),
        " - isSegment1, logSeg1 );"
    );

    nl!(
        ss,
        "logSeg1 = ",
        ss.float3_const1(p.log_seg1_log_slope),
        " * log( logSeg1 ) + ",
        ss.float3_const1(p.log_seg1_log_off),
        ";"
    );

    // Log Segment 2.
    nl!(ss);
    nl!(
        ss,
        ss.float3_decl("logSeg2")?,
        " = ",
        pix3,
        " * ",
        ss.float3_const1(p.log_seg2_lin_slope),
        " + ",
        ss.float3_const1(p.log_seg2_lin_off),
        ";"
    );

    // Clamp below 1 to avoid NaNs if the branch will not be used.
    nl!(
        ss,
        "logSeg2 = max( ",
        ss.float3_const1(1.0f64),
        " - isSegment3, logSeg2 );"
    );

    nl!(
        ss,
        "logSeg2 = ",
        ss.float3_const1(p.log_seg2_log_slope),
        " * log( logSeg2 ) + ",
        ss.float3_const1(p.log_seg2_log_off),
        ";"
    );

    // Linear Segment.
    nl!(ss);
    nl!(
        ss,
        ss.float3_decl("linSeg")?,
        "= ",
        ss.float3_const1(p.lin_seg_slope),
        " * ",
        pix3,
        " + ",
        ss.float3_const1(p.lin_seg_off),
        ";"
    );

    // Combine segments.
    nl!(ss);
    nl!(
        ss,
        pix3,
        " = isSegment1 * logSeg1 + isSegment2 * linSeg + isSegment3 * logSeg2;"
    );
    Ok(())
}

fn add_double_log_to_lin(pix: &str, ss: &mut GpuShaderText, params: &[f64]) -> Result<()> {
    let p = DoubleLogParams::new(params)?;

    let break1_log = p.log_seg1_log_slope
        * (p.log_seg1_lin_slope * p.break1 + p.log_seg1_lin_off).ln()
        + p.log_seg1_log_off;
    let break2_log = p.log_seg2_log_slope
        * (p.log_seg2_lin_slope * p.break2 + p.log_seg2_lin_off).ln()
        + p.log_seg2_log_off;

    let pix3 = format!("{pix}.rgb");

    // This assumes the forward function is monotonically increasing.
    nl!(
        ss,
        ss.float3_decl("isSegment1")?,
        " = ",
        ss.float3_greater_than_equal(&ss.float3_const1(break1_log), &pix3),
        ";"
    );
    nl!(
        ss,
        ss.float3_decl("isSegment3")?,
        " = ",
        ss.float3_greater_than_equal(&pix3, &ss.float3_const1(break2_log)),
        ";"
    );
    nl!(
        ss,
        ss.float3_decl("isSegment2")?,
        " = ",
        ss.float3_const1(1.0f32),
        " - isSegment1 - isSegment3;"
    );

    // Log Segment 1.
    nl!(ss);
    nl!(
        ss,
        ss.float3_decl("logSeg1")?,
        " = (",
        pix3,
        " - ",
        ss.float3_const1(p.log_seg1_log_off),
        ") * ",
        ss.float3_const1(1.0 / p.log_seg1_log_slope),
        ";"
    );
    nl!(
        ss,
        "logSeg1 = (",
        "exp(logSeg1) - ",
        ss.float3_const1(p.log_seg1_lin_off),
        ") * ",
        ss.float3_const1(1.0 / p.log_seg1_lin_slope),
        ";"
    );

    // Log Segment 2.
    nl!(ss);
    nl!(
        ss,
        ss.float3_decl("logSeg2")?,
        " = (",
        pix3,
        " - ",
        ss.float3_const1(p.log_seg2_log_off),
        ") * ",
        ss.float3_const1(1.0 / p.log_seg2_log_slope),
        ";"
    );
    nl!(
        ss,
        "logSeg2 = (",
        "exp(logSeg2) - ",
        ss.float3_const1(p.log_seg2_lin_off),
        ") * ",
        ss.float3_const1(1.0 / p.log_seg2_lin_slope),
        ";"
    );

    // Linear Segment.
    nl!(ss);
    nl!(
        ss,
        ss.float3_decl("linSeg")?,
        " = (",
        pix3,
        " - ",
        ss.float3_const1(p.lin_seg_off),
        ") * ",
        ss.float3_const1(1.0 / p.lin_seg_slope),
        ";"
    );

    // Combine segments.
    nl!(ss);
    nl!(
        ss,
        pix3,
        " = isSegment1 * logSeg1 + isSegment2 * linSeg + isSegment3 * logSeg2;"
    );
    Ok(())
}

/// Add the shader text of a fixed function to `ss` (port of
/// `GetFixedFunctionGPUProcessingText`, also used by the hue curve op).
pub(crate) fn fixed_function_processing_text(
    shader_creator: &mut dyn GpuShaderCreator,
    ss: &mut GpuShaderText,
    func: &FixedFunctionOpData,
) -> Result<()> {
    ss.indent();

    nl!(ss, "");
    nl!(
        ss,
        "// Add FixedFunction '",
        func.style.detailed_str(),
        "' processing"
    );
    nl!(ss, "");
    nl!(ss, "{");
    ss.indent();

    let pxl = shader_creator.pixel_name().to_string();
    let params = &func.params;
    let fparam = |i: usize| param(params, i).map(|v| v as f32);

    match func.style {
        S::AcesRedMod03Fwd => add_red_mod_fwd_shader(&pxl, ss, true)?,
        S::AcesRedMod03Inv => add_red_mod_inv_shader(&pxl, ss, true)?,
        S::AcesRedMod10Fwd => add_red_mod_fwd_shader(&pxl, ss, false)?,
        S::AcesRedMod10Inv => add_red_mod_inv_shader(&pxl, ss, false)?,
        S::AcesGlow03Fwd => add_glow_03_shader(&pxl, ss, 0.075, 0.1, true)?,
        S::AcesGlow03Inv => add_glow_03_shader(&pxl, ss, 0.075, 0.1, false)?,
        // Use 03 renderer with different params.
        S::AcesGlow10Fwd => add_glow_03_shader(&pxl, ss, 0.05, 0.08, true)?,
        S::AcesGlow10Inv => add_glow_03_shader(&pxl, ss, 0.05, 0.08, false)?,
        S::AcesDarkToDim10Fwd => add_surround_10_fwd_shader(&pxl, ss, 0.9811)?,
        // Call forward renderer with the inverse gamma.
        S::AcesDarkToDim10Inv => add_surround_10_fwd_shader(&pxl, ss, 1.0192640913260627)?,
        S::AcesGamutComp13Fwd => add_gamut_comp_13_shader(&pxl, ss, params, true)?,
        S::AcesGamutComp13Inv => add_gamut_comp_13_shader(&pxl, ss, params, false)?,
        S::AcesOutputTransform20Fwd => {
            add_aces_output_transform_shader(shader_creator, ss, params, true)?
        }
        S::AcesOutputTransform20Inv => {
            add_aces_output_transform_shader(shader_creator, ss, params, false)?
        }
        S::AcesRgbToJmh20 => add_rgb_to_jmh_shader(&pxl, ss, params)?,
        S::AcesJmhToRgb20 => add_jmh_to_rgb_shader(&pxl, ss, params)?,
        S::AcesRgbToHmj20 => add_rgb_to_hmj_shader(&pxl, ss, params)?,
        S::AcesHmjToRgb20 => add_hmj_to_rgb_shader(&pxl, ss, params)?,
        S::AcesTonescaleCompress20Fwd => {
            add_tonescale_compress_shader(shader_creator, ss, params, true)?
        }
        S::AcesTonescaleCompress20Inv => {
            add_tonescale_compress_shader(shader_creator, ss, params, false)?
        }
        S::AcesGamutCompress20Fwd => add_gamut_compress_shader(shader_creator, ss, params, true)?,
        S::AcesGamutCompress20Inv => add_gamut_compress_shader(shader_creator, ss, params, false)?,
        S::Rec2100SurroundFwd => add_rec2100_surround_shader(&pxl, ss, fparam(0)?, true)?,
        S::Rec2100SurroundInv => add_rec2100_surround_shader(&pxl, ss, fparam(0)?, false)?,
        S::RgbToHsv => add_rgb_to_hsv(&pxl, ss)?,
        S::RgbToHsyLog | S::RgbToHsyLin | S::RgbToHsyVid => add_rgb_to_hsy(&pxl, ss, func.style)?,
        S::HsyLogToRgb | S::HsyLinToRgb | S::HsyVidToRgb => add_hsy_to_rgb(&pxl, ss, func.style)?,
        S::HsvToRgb => add_hsv_to_rgb(&pxl, ss)?,
        S::XyzToXyy => add_xyz_to_xyy(&pxl, ss)?,
        S::XyyToXyz => add_xyy_to_xyz(&pxl, ss)?,
        S::XyzToUvy => add_xyz_to_uvy(&pxl, ss)?,
        S::UvyToXyz => add_uvy_to_xyz(&pxl, ss)?,
        S::XyzToLuv => add_xyz_to_luv(&pxl, ss)?,
        S::LuvToXyz => add_luv_to_xyz(&pxl, ss)?,
        S::LinToPq => add_lin_to_pq(&pxl, ss)?,
        S::PqToLin => add_pq_to_lin(&pxl, ss)?,
        S::LinToGammaLog => add_lin_to_gamma_log(&pxl, ss, params)?,
        S::GammaLogToLin => add_gamma_log_to_lin(&pxl, ss, params)?,
        S::LinToDoubleLog => add_lin_to_double_log(&pxl, ss, params)?,
        S::DoubleLogToLin => add_double_log_to_lin(&pxl, ss, params)?,
    }

    ss.dedent();
    nl!(ss, "}");

    ss.dedent();
    Ok(())
}

/// Port of `GetFixedFunctionGPUShaderProgram`.
pub(crate) fn fixed_function_shader_program(
    shader_creator: &mut dyn GpuShaderCreator,
    func: &FixedFunctionOpData,
) -> Result<()> {
    let mut ss = GpuShaderText::new(shader_creator.language());
    fixed_function_processing_text(shader_creator, &mut ss, func)?;
    shader_creator.add_to_function_shader_code(ss.as_str());
    Ok(())
}

/// Port of `FixedFunctionOp::extractGpuShaderInfo`.
pub(crate) fn extract(
    op: &FixedFunctionOp,
    shader_creator: &mut dyn GpuShaderCreator,
) -> Result<()> {
    fixed_function_shader_program(shader_creator, op.data())
}
