//! GPU renderer of the log op (port of `LogOpGPU.cpp`).

use crate::error::{Error, Result};
use crate::gpu::{GpuShaderCreator, GpuShaderText};
use crate::nl;
use crate::ops::log::log_utils::{get_linear_offset, get_linear_slope, get_log_side_break};
use crate::ops::log::{
    LogOp, LogOpData, LIN_SIDE_BREAK, LIN_SIDE_OFFSET, LIN_SIDE_SLOPE, LOG_SIDE_OFFSET,
    LOG_SIDE_SLOPE,
};
use crate::types::TransformDirection;

fn begin(st: &mut GpuShaderText, title: &str) {
    st.indent();
    nl!(st, "");
    nl!(st, title);
    nl!(st, "");
    nl!(st, "{");
    st.indent();
}

fn end(st: &mut GpuShaderText, shader_creator: &mut dyn GpuShaderCreator) {
    st.dedent();
    nl!(st, "}");
    shader_creator.add_to_function_shader_code(st.as_str());
}

fn add_log_shader(shader_creator: &mut dyn GpuShaderCreator, base: f32) -> Result<()> {
    let min_value = f32::MIN_POSITIVE;

    let mut st = GpuShaderText::new(shader_creator.language());
    begin(&mut st, "// Add Log processing");

    let pixrgb = format!("{}.rgb", shader_creator.pixel_name());

    nl!(
        st,
        pixrgb,
        " = max( ",
        st.float3_const1(min_value),
        ", ",
        pixrgb,
        ");"
    );

    if base == 2.0 {
        nl!(st, pixrgb, " = log2(", pixrgb, ");");
    } else {
        // base 10
        let one_over_log10 = 1.0f32 / base.ln();
        nl!(
            st,
            pixrgb,
            " = log(",
            pixrgb,
            ") * ",
            st.float3_const1(one_over_log10),
            ";"
        );
    }

    end(&mut st, shader_creator);
    Ok(())
}

fn add_anti_log_shader(shader_creator: &mut dyn GpuShaderCreator, base: f32) -> Result<()> {
    let mut st = GpuShaderText::new(shader_creator.language());
    begin(&mut st, "// Add Log 'Anti-Log' processing");

    let pixrgb = format!("{}.rgb", shader_creator.pixel_name());

    nl!(
        st,
        pixrgb,
        " = pow( ",
        st.float3_const1(base),
        ", ",
        pixrgb,
        ");"
    );

    end(&mut st, shader_creator);
    Ok(())
}

/// The parameters of the three channels.
fn channel_params(log_data: &LogOpData) -> Result<[&[f64]; 3]> {
    let p = [log_data.params(0), log_data.params(1), log_data.params(2)];
    if p.iter().any(|c| c.len() <= LIN_SIDE_OFFSET) {
        return Err(Error::msg("Log: missing parameters."));
    }
    Ok(p)
}

fn add_log_to_lin_shader(
    shader_creator: &mut dyn GpuShaderCreator,
    log_data: &LogOpData,
) -> Result<()> {
    let [pr, pg, pb] = channel_params(log_data)?;
    let base = log_data.base;

    let log_slope_inv = [
        1.0f32 / pr[LOG_SIDE_SLOPE] as f32,
        1.0f32 / pg[LOG_SIDE_SLOPE] as f32,
        1.0f32 / pb[LOG_SIDE_SLOPE] as f32,
    ];
    let lin_slope_inv = [
        1.0f32 / pr[LIN_SIDE_SLOPE] as f32,
        1.0f32 / pg[LIN_SIDE_SLOPE] as f32,
        1.0f32 / pb[LIN_SIDE_SLOPE] as f32,
    ];

    let mut st = GpuShaderText::new(shader_creator.language());
    begin(&mut st, "// Add Log 'Log to Lin' processing");

    let pixrgb = format!("{}.rgb", shader_creator.pixel_name());

    st.declare_float3(
        "log_slopeinv",
        log_slope_inv[0],
        log_slope_inv[1],
        log_slope_inv[2],
    )?;
    st.declare_float3(
        "lin_slopeinv",
        lin_slope_inv[0],
        lin_slope_inv[1],
        lin_slope_inv[2],
    )?;
    st.declare_float3(
        "lin_offset",
        pr[LIN_SIDE_OFFSET],
        pg[LIN_SIDE_OFFSET],
        pb[LIN_SIDE_OFFSET],
    )?;
    st.declare_float3("log_base", base, base, base)?;
    st.declare_float3(
        "log_offset",
        pr[LOG_SIDE_OFFSET],
        pg[LOG_SIDE_OFFSET],
        pb[LOG_SIDE_OFFSET],
    )?;
    // Decompose into 3 steps:
    // 1) (x - logOffset) * logSlopeInv
    // 2) pow(base, x)
    // 3) linSlopeInv * (x - linOffset)
    nl!(st, pixrgb, " = (", pixrgb, " - log_offset) * log_slopeinv;");
    nl!(st, pixrgb, " = pow(log_base, ", pixrgb, ");");
    nl!(st, pixrgb, " = lin_slopeinv * (", pixrgb, " - lin_offset);");

    end(&mut st, shader_creator);
    Ok(())
}

fn add_lin_to_log_shader(
    shader_creator: &mut dyn GpuShaderCreator,
    log_data: &LogOpData,
) -> Result<()> {
    // logSlope * log(linSlope * x + linOffset, base) + logOffset

    let [pr, pg, pb] = channel_params(log_data)?;
    let base = log_data.base;

    let min_value = f32::MIN_POSITIVE;

    let mut st = GpuShaderText::new(shader_creator.language());
    begin(&mut st, "// Add Log 'Lin to Log' processing");

    let pixrgb = format!("{}.rgb", shader_creator.pixel_name());

    st.declare_float3("minValue", min_value, min_value, min_value)?;
    st.declare_float3(
        "lin_slope",
        pr[LIN_SIDE_SLOPE],
        pg[LIN_SIDE_SLOPE],
        pb[LIN_SIDE_SLOPE],
    )?;
    st.declare_float3(
        "lin_offset",
        pr[LIN_SIDE_OFFSET],
        pg[LIN_SIDE_OFFSET],
        pb[LIN_SIDE_OFFSET],
    )?;
    // We account for the change of base by rolling the multiplier in with log
    // slope.
    let log_slope_new = [
        (pr[LOG_SIDE_SLOPE] / base.ln()) as f32,
        (pg[LOG_SIDE_SLOPE] / base.ln()) as f32,
        (pb[LOG_SIDE_SLOPE] / base.ln()) as f32,
    ];
    st.declare_float3(
        "log_slope",
        log_slope_new[0],
        log_slope_new[1],
        log_slope_new[2],
    )?;
    st.declare_float3(
        "log_offset",
        pr[LOG_SIDE_OFFSET],
        pg[LOG_SIDE_OFFSET],
        pb[LOG_SIDE_OFFSET],
    )?;
    // Decompose into 2 steps:
    // 1) clamp(fltmin, linSlope * x + linOffset)
    // 2) logSlopeNew * log(x) + logOffset
    nl!(
        st,
        pixrgb,
        " = max( minValue, (",
        pixrgb,
        " * lin_slope + lin_offset) );"
    );
    nl!(
        st,
        pixrgb,
        " = log_slope * log(",
        pixrgb,
        " ) + log_offset;"
    );

    end(&mut st, shader_creator);
    Ok(())
}

/// Linear slope, log side break and linear offset of the three channels.
fn camera_params(p: &[&[f64]; 3], base: f64) -> Result<([f32; 3], [f32; 3], [f32; 3])> {
    if p.iter().any(|c| c.len() <= LIN_SIDE_BREAK) {
        return Err(Error::msg("Log: missing camera parameters."));
    }
    let slope = [
        get_linear_slope(p[0], base),
        get_linear_slope(p[1], base),
        get_linear_slope(p[2], base),
    ];
    let brk = [
        get_log_side_break(p[0], base),
        get_log_side_break(p[1], base),
        get_log_side_break(p[2], base),
    ];
    let offset = [
        get_linear_offset(p[0], slope[0], brk[0]),
        get_linear_offset(p[1], slope[1], brk[1]),
        get_linear_offset(p[2], slope[2], brk[2]),
    ];
    Ok((slope, brk, offset))
}

fn add_camera_log_to_lin_shader(
    shader_creator: &mut dyn GpuShaderCreator,
    log_data: &LogOpData,
) -> Result<()> {
    // if in <= logBreak
    //  out = ( in - linearOffset ) / linearSlope
    // else
    //  out = ( pow( base, (in - logOffset) / logSlope ) - linOffset ) / linSlope;

    let params = channel_params(log_data)?;
    let [pr, pg, pb] = params;
    let base = log_data.base;

    let (linear_slope, log_side_break, linear_offset) = camera_params(&params, base)?;

    let log_slope_inv = [
        1.0f32 / pr[LOG_SIDE_SLOPE] as f32,
        1.0f32 / pg[LOG_SIDE_SLOPE] as f32,
        1.0f32 / pb[LOG_SIDE_SLOPE] as f32,
    ];
    let lin_slope_inv = [
        1.0f32 / pr[LIN_SIDE_SLOPE] as f32,
        1.0f32 / pg[LIN_SIDE_SLOPE] as f32,
        1.0f32 / pb[LIN_SIDE_SLOPE] as f32,
    ];

    let mut st = GpuShaderText::new(shader_creator.language());
    begin(&mut st, "// Add Log 'Camera Log to Lin' processing");

    let pixrgb = format!("{}.rgb", shader_creator.pixel_name());

    st.declare_float3(
        "log_break",
        log_side_break[0],
        log_side_break[1],
        log_side_break[2],
    )?;
    st.declare_float3(
        "linear_segment_offset",
        linear_offset[0],
        linear_offset[1],
        linear_offset[2],
    )?;
    st.declare_float3(
        "linear_segment_slopeinv",
        1.0f32 / linear_slope[0],
        1.0f32 / linear_slope[1],
        1.0f32 / linear_slope[2],
    )?;
    st.declare_float3(
        "lin_slopeinv",
        lin_slope_inv[0],
        lin_slope_inv[1],
        lin_slope_inv[2],
    )?;
    st.declare_float3(
        "lin_offset",
        pr[LIN_SIDE_OFFSET],
        pg[LIN_SIDE_OFFSET],
        pb[LIN_SIDE_OFFSET],
    )?;
    st.declare_float3(
        "log_slopeinv",
        log_slope_inv[0],
        log_slope_inv[1],
        log_slope_inv[2],
    )?;
    st.declare_float3("log_base", base, base, base)?;
    st.declare_float3(
        "log_offset",
        pr[LOG_SIDE_OFFSET],
        pg[LOG_SIDE_OFFSET],
        pb[LOG_SIDE_OFFSET],
    )?;

    nl!(
        st,
        st.float3_decl("isAboveBreak")?,
        " = ",
        st.float3_greater_than(&pixrgb, "log_break"),
        ";"
    );

    // Compute linear segment.
    nl!(
        st,
        st.float3_decl("linSeg")?,
        " = ( ",
        pixrgb,
        " - linear_segment_offset ) * linear_segment_slopeinv;"
    );

    // Decompose log segment into 3 steps:
    // 1) (x - logOffset) * logSlopeInv
    // 2) pow(base, x)
    // 3) linSlopeInv * (x - linOffset)
    nl!(
        st,
        st.float3_decl("logSeg")?,
        " = (",
        pixrgb,
        " - log_offset) * log_slopeinv;"
    );
    nl!(st, "logSeg = pow(log_base, logSeg);");
    nl!(st, "logSeg = lin_slopeinv * (logSeg - lin_offset);");

    // Combine linear and log segments.
    nl!(
        st,
        pixrgb,
        " = isAboveBreak * logSeg + ( ",
        st.float3_const1(1.0f32),
        " - isAboveBreak ) * linSeg;"
    );

    end(&mut st, shader_creator);
    Ok(())
}

fn add_camera_lin_to_log_shader(
    shader_creator: &mut dyn GpuShaderCreator,
    log_data: &LogOpData,
) -> Result<()> {
    // if in <= linBreak
    //  out = linearSlope * in + linearOffset
    // else
    //  out = ( logSlope * log( base, max( minValue, (in*linSlope + linOffset) ) ) + logOffset )

    let params = channel_params(log_data)?;
    let [pr, pg, pb] = params;
    let base = log_data.base;

    let (linear_slope, _log_side_break, linear_offset) = camera_params(&params, base)?;

    // We account for the change of base by rolling the multiplier in with log
    // slope.
    let log_slope_new = [
        (pr[LOG_SIDE_SLOPE] / base.ln()) as f32,
        (pg[LOG_SIDE_SLOPE] / base.ln()) as f32,
        (pb[LOG_SIDE_SLOPE] / base.ln()) as f32,
    ];

    let min_value = f32::MIN_POSITIVE;

    let mut st = GpuShaderText::new(shader_creator.language());
    begin(&mut st, "// Add Log 'Camera Lin to Log' processing");

    let pixrgb = format!("{}.rgb", shader_creator.pixel_name());

    st.declare_float3("minValue", min_value, min_value, min_value)?;
    st.declare_float3(
        "linear_break",
        pr[LIN_SIDE_BREAK],
        pg[LIN_SIDE_BREAK],
        pb[LIN_SIDE_BREAK],
    )?;
    st.declare_float3(
        "linear_segment_slope",
        linear_slope[0],
        linear_slope[1],
        linear_slope[2],
    )?;
    st.declare_float3(
        "linear_segment_offset",
        linear_offset[0],
        linear_offset[1],
        linear_offset[2],
    )?;
    st.declare_float3(
        "lin_slope",
        pr[LIN_SIDE_SLOPE],
        pg[LIN_SIDE_SLOPE],
        pb[LIN_SIDE_SLOPE],
    )?;
    st.declare_float3(
        "lin_offset",
        pr[LIN_SIDE_OFFSET],
        pg[LIN_SIDE_OFFSET],
        pb[LIN_SIDE_OFFSET],
    )?;
    st.declare_float3(
        "log_slope",
        log_slope_new[0],
        log_slope_new[1],
        log_slope_new[2],
    )?;
    st.declare_float3(
        "log_offset",
        pr[LOG_SIDE_OFFSET],
        pg[LOG_SIDE_OFFSET],
        pb[LOG_SIDE_OFFSET],
    )?;

    nl!(
        st,
        st.float3_decl("isAboveBreak")?,
        " = ",
        st.float3_greater_than(&pixrgb, "linear_break"),
        ";"
    );

    // Compute linear segment.
    nl!(
        st,
        st.float3_decl("linSeg")?,
        " = ",
        pixrgb,
        " * linear_segment_slope + linear_segment_offset;"
    );

    // Decompose log into 2 steps:
    // 1) clamp(fltmin, linSlope * x + linOffset)
    // 2) logSlopeNew * log(x) + logOffset
    nl!(
        st,
        st.float3_decl("logSeg")?,
        " = max( minValue, (",
        pixrgb,
        " * lin_slope + lin_offset) );"
    );
    nl!(st, "logSeg = log_slope * log( logSeg ) + log_offset;");

    // Combine linear and log segments.
    nl!(
        st,
        pixrgb,
        " = isAboveBreak * logSeg + ( ",
        st.float3_const1(1.0f32),
        " - isAboveBreak ) * linSeg;"
    );

    end(&mut st, shader_creator);
    Ok(())
}

/// Port of `GetLogGPUShaderProgram`.
pub(crate) fn log_shader_program(
    shader_creator: &mut dyn GpuShaderCreator,
    log_data: &LogOpData,
) -> Result<()> {
    let dir = log_data.direction;
    let fwd = dir == TransformDirection::Forward;
    if log_data.is_log2() {
        if fwd {
            add_log_shader(shader_creator, 2.0)
        } else {
            add_anti_log_shader(shader_creator, 2.0)
        }
    } else if log_data.is_log10() {
        if fwd {
            add_log_shader(shader_creator, 10.0)
        } else {
            add_anti_log_shader(shader_creator, 10.0)
        }
    } else if log_data.is_camera() {
        if fwd {
            add_camera_lin_to_log_shader(shader_creator, log_data)
        } else {
            add_camera_log_to_lin_shader(shader_creator, log_data)
        }
    } else if fwd {
        add_lin_to_log_shader(shader_creator, log_data)
    } else {
        add_log_to_lin_shader(shader_creator, log_data)
    }
}

/// Port of `LogOp::extractGpuShaderInfo`.
pub(crate) fn extract(op: &LogOp, shader_creator: &mut dyn GpuShaderCreator) -> Result<()> {
    log_shader_program(shader_creator, op.data())
}
