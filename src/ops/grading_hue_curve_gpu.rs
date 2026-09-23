//! GPU renderer of the hue curve grading op (port of
//! `GradingHueCurveOpGPU.cpp`). See `grading_rgb_curve_gpu` for the layout
//! of the knots and coefficients arrays.

use std::sync::Arc;

use crate::config::logging::log_warning;
use crate::dynamic_property::{DynamicProperty, SharedValue};
use crate::error::Result;
use crate::gpu::shader_text::{
    add_lin_to_log_shader_channel_blue, add_log_to_lin_shader_channel_blue, build_resource_name,
};
use crate::gpu::{GpuShaderCreator, GpuShaderText};
use crate::nl;
use crate::ops::fixed_function::{FixedFunctionOpData, FixedFunctionOpStyle};
use crate::ops::fixed_function_gpu::fixed_function_processing_text;
use crate::ops::grading_hue_curve::GradingHueCurveOp;
use crate::ops::grading_rgb_curve::bspline::KnotsCoefs;
use crate::ops::grading_rgb_curve_gpu::{
    add_gc_properties_uniforms, add_shader_eval_fwd, add_shader_eval_rev, add_shader_eval_rev_hue,
    build_resource_name_indexed, declare_knots_coefs_arrays, GcProperties,
};
use crate::ops::Op;
use crate::types::{GpuLanguage, GradingStyle, HsyTransformStyle, TransformDirection};

const OP_PREFIX: &str = "grading_huecurve";

/// Number of values of the offsets arrays (8 curves x 2 values).
const NUM_OFFSET_VALUES: i32 = 16;

struct HueProperties {
    gc: GcProperties,
    eval_rev: String,
    eval_rev_hue: String,
}

impl Default for HueProperties {
    fn default() -> Self {
        Self {
            gc: GcProperties::default(),
            eval_rev: "evalBSplineCurveRev".into(),
            eval_rev_hue: "evalBSplineCurveRevHue".into(),
        }
    }
}

fn set_gc_properties(
    shader_creator: &mut dyn GpuShaderCreator,
    dynamic: bool,
    p: &mut HueProperties,
) {
    if dynamic {
        // If there are several dynamic ops, they will use the same names for
        // uniforms.
        for n in [
            &mut p.gc.knots_offsets,
            &mut p.gc.knots,
            &mut p.gc.coefs_offsets,
            &mut p.gc.coefs,
            &mut p.gc.local_bypass,
            &mut p.gc.eval,
            &mut p.eval_rev,
            &mut p.eval_rev_hue,
        ] {
            *n = build_resource_name(shader_creator, OP_PREFIX, n);
        }
    } else {
        // Non-dynamic ops need an helper function for each op.
        let res_index = shader_creator.next_resource_index();
        for n in [
            &mut p.gc.knots_offsets,
            &mut p.gc.knots,
            &mut p.gc.coefs_offsets,
            &mut p.gc.coefs,
            &mut p.gc.eval,
            &mut p.eval_rev,
            &mut p.eval_rev_hue,
        ] {
            *n = build_resource_name_indexed(shader_creator, OP_PREFIX, n, res_index);
        }
    }
}

fn add_curve_function_name(
    lang: GpuLanguage,
    st: &mut GpuShaderText,
    func_name: &str,
    is_fwd: bool,
) {
    nl!(st, "");
    let args = match (
        lang == GpuLanguage::Osl1 || lang == GpuLanguage::Msl2_0,
        is_fwd,
    ) {
        (true, true) => "(int curveIdx, float x, float identity_x)",
        (true, false) => "(int curveIdx, float x)",
        (false, true) => "(in int curveIdx, in float x, in float identity_x)",
        (false, false) => "(in int curveIdx, in float x)",
    };
    nl!(st, st.float_keyword(), " ", func_name, args);
}

fn add_curve_eval_method_text_to_shader_program(
    shader_creator: &mut dyn GpuShaderCreator,
    op: &GradingHueCurveOp,
    p: &HueProperties,
    dyn_: bool,
) -> Result<()> {
    let lang = shader_creator.language();
    let mut st = GpuShaderText::new(lang);
    let gc = &p.gc;

    // Dynamic version uses uniforms declared globally. Non-dynamic version
    // declares local variables in the op specific helper function.
    if !dyn_ {
        declare_knots_coefs_arrays(&mut st, &op.knots_coefs(), gc)?;
    }

    add_curve_function_name(lang, &mut st, &gc.eval, true);

    nl!(st, "{");
    st.indent();
    add_shader_eval_fwd(
        &mut st,
        &gc.knots_offsets,
        &gc.coefs_offsets,
        &gc.knots,
        &gc.coefs,
    );
    st.dedent();
    nl!(st, "}");

    if op.direction() == TransformDirection::Inverse {
        add_curve_function_name(lang, &mut st, &p.eval_rev, false);

        nl!(st, "{");
        st.indent();
        add_shader_eval_rev(
            &mut st,
            &gc.knots_offsets,
            &gc.coefs_offsets,
            &gc.knots,
            &gc.coefs,
        );
        st.dedent();
        nl!(st, "}");

        add_curve_function_name(lang, &mut st, &p.eval_rev_hue, false);

        nl!(st, "{");
        st.indent();
        add_shader_eval_rev_hue(
            &mut st,
            &gc.knots_offsets,
            &gc.coefs_offsets,
            &gc.knots,
            &gc.coefs,
        );
        st.dedent();
        nl!(st, "}");
    }

    shader_creator.add_to_helper_shader_code(st.as_str());
    Ok(())
}

/// Add the RGB <-> HSY conversion of `style`.
fn add_hsy(
    shader_creator: &mut dyn GpuShaderCreator,
    st: &mut GpuShaderText,
    style: GradingStyle,
    to_hsy: bool,
) -> Result<()> {
    let hsy_style = match (style, to_hsy) {
        (GradingStyle::Lin, true) => FixedFunctionOpStyle::RgbToHsyLin,
        (GradingStyle::Log, true) => FixedFunctionOpStyle::RgbToHsyLog,
        (GradingStyle::Video, true) => FixedFunctionOpStyle::RgbToHsyVid,
        (GradingStyle::Lin, false) => FixedFunctionOpStyle::HsyLinToRgb,
        (GradingStyle::Log, false) => FixedFunctionOpStyle::HsyLogToRgb,
        (GradingStyle::Video, false) => FixedFunctionOpStyle::HsyVidToRgb,
    };

    nl!(st, "{"); // establish scope so local variable names won't conflict
    st.indent();
    let func_op_data = FixedFunctionOpData::new(hsy_style, &[])?;
    fixed_function_processing_text(shader_creator, st, &func_op_data)?;
    st.dedent();
    nl!(st, "}");
    Ok(())
}

struct ShaderOptions {
    dyn_: bool,
    do_lin_to_log: bool,
    do_rgb_to_hsy: bool,
    draw_curve_only: bool,
    style: GradingStyle,
}

fn add_draw_curve_only(st: &mut GpuShaderText, pix: &str, eval: &str) {
    nl!(st, pix, ".r = ", eval, "(1, ", pix, ".r, 1.);"); // HUE-SAT
    nl!(st, pix, ".g = ", eval, "(1, ", pix, ".g, 1.);"); // HUE-SAT
    nl!(st, pix, ".b = ", eval, "(1, ", pix, ".b, 1.);"); // HUE-SAT
}

fn add_gc_forward_shader(
    shader_creator: &mut dyn GpuShaderCreator,
    st: &mut GpuShaderText,
    p: &HueProperties,
    o: &ShaderOptions,
) -> Result<()> {
    let pix = &shader_creator.pixel_name().to_string();
    let e = &p.gc.eval;
    if o.draw_curve_only {
        add_draw_curve_only(st, pix, e);
        return Ok(());
    }

    if o.dyn_ {
        nl!(st, "if (!", st.cast_to_bool(&p.gc.local_bypass), ")");
        nl!(st, "{");
        st.indent();
    }

    if o.do_rgb_to_hsy {
        add_hsy(shader_creator, st, o.style, true)?;
    }

    if o.do_lin_to_log {
        nl!(st, "// Convert from lin to log.");
        add_lin_to_log_shader_channel_blue(shader_creator, st)?;
        nl!(st, "");
    }

    nl!(st, "");
    nl!(
        st,
        "float hueSatGain = max(0., ",
        e,
        "(1, ",
        pix,
        ".r, 1.));"
    );
    nl!(
        st,
        "float hueLumGain = max(0., ",
        e,
        "(2, ",
        pix,
        ".r, 1.));"
    );
    nl!(st, pix, ".r = ", e, "(0, ", pix, ".r, ", pix, ".r);");
    nl!(
        st,
        "",
        pix,
        ".g = max(0., ",
        e,
        "(4, ",
        pix,
        ".g, ",
        pix,
        ".g));"
    );
    nl!(
        st,
        "float lumSatGain = max(0., ",
        e,
        "(3, ",
        pix,
        ".b, 1.));"
    );
    nl!(st, "float satGain = lumSatGain * hueSatGain;");
    nl!(st, "", pix, ".g = satGain * ", pix, ".g;");
    nl!(
        st,
        "float satLumGain = max(0., ",
        e,
        "(6, ",
        pix,
        ".g, 1.));"
    );
    nl!(st, pix, ".b = ", e, "(5, ", pix, ".b, ", pix, ".b);");
    nl!(st, "");

    if o.do_lin_to_log {
        nl!(st, "");
        nl!(st, "// Convert from log to lin.");
        add_log_to_lin_shader_channel_blue(shader_creator, st)?;
    }

    nl!(st, "");
    nl!(
        st,
        "hueLumGain = 1. - (1. - hueLumGain) * min( 1., ",
        pix,
        ".g );"
    );
    if o.style == GradingStyle::Log {
        nl!(
            st,
            pix,
            ".b = ",
            pix,
            ".b + (hueLumGain + satLumGain - 2.) * 0.1;"
        );
    } else {
        nl!(st, pix, ".b = ", pix, ".b * hueLumGain * satLumGain;");
    }
    nl!(st, "");

    nl!(st, pix, ".r = ", pix, ".r - floor( ", pix, ".r );");
    nl!(st, pix, ".r = ", pix, ".r + ", e, "(7, ", pix, ".r, 0.);");

    if o.do_rgb_to_hsy {
        add_hsy(shader_creator, st, o.style, false)?;
    }

    if o.dyn_ {
        st.dedent();
        nl!(st, "}");
    }
    Ok(())
}

fn add_gc_inverse_shader(
    shader_creator: &mut dyn GpuShaderCreator,
    st: &mut GpuShaderText,
    p: &HueProperties,
    o: &ShaderOptions,
) -> Result<()> {
    let pix = &shader_creator.pixel_name().to_string();
    let e = &p.gc.eval;

    if o.draw_curve_only {
        add_draw_curve_only(st, pix, e);
        return Ok(());
    }

    if o.dyn_ {
        nl!(st, "if (!", st.cast_to_bool(&p.gc.local_bypass), ")");
        nl!(st, "{");
        st.indent();
    }

    if o.do_rgb_to_hsy {
        add_hsy(shader_creator, st, o.style, true)?;
    }

    nl!(st, pix, ".r = ", p.eval_rev_hue, "(7, ", pix, ".r);");

    nl!(st, pix, ".r = ", p.eval_rev_hue, "(0, ", pix, ".r);");
    nl!(st, "");

    nl!(st, pix, ".r = ", pix, ".r - floor( ", pix, ".r );");
    nl!(
        st,
        "float hueSatGain = max(0., ",
        e,
        "(1, ",
        pix,
        ".r, 1.));"
    );
    nl!(
        st,
        "float hueLumGain = max(0., ",
        e,
        "(2, ",
        pix,
        ".r, 1.));"
    );

    nl!(st, "", pix, ".g = max(0., ", pix, ".g);");
    nl!(
        st,
        "float satLumGain = max(0., ",
        e,
        "(6, ",
        pix,
        ".g, 1.));"
    );

    nl!(st, "");
    nl!(
        st,
        "hueLumGain = 1. - (1. - hueLumGain) * min( 1., ",
        pix,
        ".g );"
    );

    if o.style == GradingStyle::Log {
        nl!(
            st,
            pix,
            ".b = ",
            pix,
            ".b - (hueLumGain + satLumGain - 2.) * 0.1;"
        );
    } else {
        nl!(
            st,
            pix,
            ".b = ",
            pix,
            ".b / max(0.01, hueLumGain * satLumGain);"
        );
    }
    nl!(st, "");

    if o.do_lin_to_log {
        nl!(st, "// Convert from lin to log.");
        add_lin_to_log_shader_channel_blue(shader_creator, st)?;
        nl!(st, "");
    }

    nl!(st, pix, ".b = ", p.eval_rev, "(5, ", pix, ".b);");
    nl!(st, "");

    nl!(
        st,
        "float lumSatGain = max(0., ",
        e,
        "(3, ",
        pix,
        ".b, 1.));"
    );

    if o.do_lin_to_log {
        nl!(st, "");
        nl!(st, "// Convert from log to lin.");
        add_log_to_lin_shader_channel_blue(shader_creator, st)?;
    }

    nl!(st, "float satGain = max(0.01, lumSatGain * hueSatGain);");
    nl!(st, "", pix, ".g = ", pix, ".g / satGain;");

    nl!(
        st,
        "",
        pix,
        ".g = max(0., ",
        p.eval_rev,
        "(4, ",
        pix,
        ".g));"
    );

    if o.do_rgb_to_hsy {
        add_hsy(shader_creator, st, o.style, false)?;
    }

    if o.dyn_ {
        st.dedent();
        nl!(st, "}");
    }
    Ok(())
}

/// Port of `GetGradingHueCurveGPUShaderProgram`.
pub(crate) fn extract(
    op: &GradingHueCurveOp,
    shader_creator: &mut dyn GpuShaderCreator,
) -> Result<()> {
    let lang = shader_creator.language();
    let is_dynamic = op.is_dynamic();
    let dyn_ = is_dynamic && lang != GpuLanguage::Osl1;
    if !dyn_ && op.knots_coefs().local_bypass {
        return Ok(());
    }

    if is_dynamic && lang == GpuLanguage::Osl1 {
        log_warning(&format!(
            "The dynamic properties are not yet supported by the 'Open Shading language (OSL)' \
             translation: The '{OP_PREFIX}' dynamic property is replaced by a local variable."
        ));
    }

    let style = op.style();
    let dir = op.direction();

    let mut st = GpuShaderText::new(lang);
    st.indent();

    nl!(st, "");
    nl!(st, "// Add GradingHueCurve ", dir.as_str(), " processing");
    nl!(st, "");
    nl!(st, "{");
    st.indent();

    let mut p = HueProperties::default();
    set_gc_properties(shader_creator, dyn_, &mut p);

    let value = op.value();

    if dyn_ {
        // Add the dynamic property to the shader creator (decoupled).
        let prop = SharedValue::new(value.clone());
        shader_creator.add_dynamic_property(DynamicProperty::GradingHueCurve(prop.clone()))?;

        // Add uniforms only if needed.
        let kc: Arc<dyn Fn() -> KnotsCoefs + Send + Sync> = Arc::new(move || {
            KnotsCoefs::from_hue_curve(&prop.get()).unwrap_or_else(|_| KnotsCoefs::new(8))
        });
        add_gc_properties_uniforms(shader_creator, kc, NUM_OFFSET_VALUES, &p.gc)?;
    }

    // Add the helper functions (plus the global variables if not dynamic).
    add_curve_eval_method_text_to_shader_program(shader_creator, op, &p, dyn_)?;

    let options = ShaderOptions {
        dyn_,
        do_lin_to_log: style == GradingStyle::Lin,
        do_rgb_to_hsy: op.rgb_to_hsy() == HsyTransformStyle::Hsy1,
        draw_curve_only: value.draw_curve_only,
        style,
    };
    match dir {
        TransformDirection::Forward => {
            add_gc_forward_shader(shader_creator, &mut st, &p, &options)?
        }
        TransformDirection::Inverse => {
            add_gc_inverse_shader(shader_creator, &mut st, &p, &options)?
        }
    }

    st.dedent();
    nl!(st, "}");

    st.dedent();
    shader_creator.add_to_function_shader_code(st.as_str());
    Ok(())
}
