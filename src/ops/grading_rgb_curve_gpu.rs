//! GPU renderer of the RGB curve grading op (port of
//! `GradingRGBCurveOpGPU.cpp` and of the shader parts of
//! `GradingBSplineCurve.cpp`).
//!
//! The curve evaluation is done using a piecewise quadratic polynomial
//! function. The shader may handle a dynamic number of curves and a dynamic
//! number of knots and coefficients per curve.
//!
//! For optimization, the knots of ALL the curves are packed in one single
//! array. This is exactly the same for coefficients. In order to access the
//! knots of a specific curve in this single array, the position of the first
//! knot and the number of knots of each curve is stored in an offset array
//! (`{Curve1StartPos, Curve1NumKnots, Curve2StartPos, Curve2NumKnots, ...}`).
//!
//! The coefficients array contains the polynomial coefficients which are
//! stored as all the quadratic terms for the first curve, then all the linear
//! terms for the first curve, then all the constant terms for the first
//! curve. The number of coefficient sets is the number of knots minus one.

use std::sync::Arc;

use crate::config::logging::log_warning;
use crate::dynamic_property::{DynamicProperty, SharedValue};
use crate::error::Result;
use crate::gpu::shader_text::{
    add_lin_to_log_shader, add_log_to_lin_shader, build_resource_name, replace_all,
};
use crate::gpu::{GpuShaderCreator, GpuShaderText};
use crate::nl;
use crate::ops::grading_rgb_curve::bspline::KnotsCoefs;
use crate::ops::grading_rgb_curve::GradingRgbCurveOp;
use crate::ops::Op;
use crate::types::{GpuLanguage, GradingStyle, TransformDirection};

/// Names of the shader resources of a curve op.
pub(crate) struct GcProperties {
    pub(crate) knots_offsets: String,
    pub(crate) knots: String,
    pub(crate) coefs_offsets: String,
    pub(crate) coefs: String,
    pub(crate) local_bypass: String,
    pub(crate) eval: String,
}

impl Default for GcProperties {
    fn default() -> Self {
        Self {
            knots_offsets: "knotsOffsets".into(),
            knots: "knots".into(),
            coefs_offsets: "coefsOffsets".into(),
            coefs: "coefs".into(),
            local_bypass: "localBypass".into(),
            eval: "evalBSplineCurve".into(),
        }
    }
}

/// Add a float array uniform (declared if new).
pub(crate) fn add_uniform_vector_float(
    shader_creator: &mut dyn GpuShaderCreator,
    get_size: impl Fn() -> i32 + Send + Sync + 'static,
    getter: impl Fn() -> Vec<f32> + Send + Sync + 'static,
    max_size: u32,
    name: &str,
) -> Result<()> {
    // Add the uniform if it does not already exist.
    if shader_creator.add_uniform_vector_float(
        name,
        Arc::new(get_size),
        Arc::new(getter),
        max_size,
    )? {
        // Declare uniform.
        let mut st_decl = GpuShaderText::new(shader_creator.language());
        st_decl.declare_uniform_array_float(name, max_size);
        shader_creator.add_to_parameter_declare_shader_code(st_decl.as_str());
    }
    Ok(())
}

/// Add an int array uniform of `array_len` values (declared if new).
pub(crate) fn add_uniform_vector_int(
    shader_creator: &mut dyn GpuShaderCreator,
    get_size: impl Fn() -> i32 + Send + Sync + 'static,
    getter: impl Fn() -> Vec<i32> + Send + Sync + 'static,
    array_len: u32,
    name: &str,
) -> Result<()> {
    // Add the uniform if it does not already exist.
    if shader_creator.add_uniform_vector_int(
        name,
        Arc::new(get_size),
        Arc::new(getter),
        array_len,
    )? {
        // Declare uniform.
        let mut st_decl = GpuShaderText::new(shader_creator.language());
        // Need 2 ints for each curve.
        st_decl.declare_uniform_array_int(name, array_len);
        shader_creator.add_to_parameter_declare_shader_code(st_decl.as_str());
    }
    Ok(())
}

/// Add a bool uniform (declared if new).
pub(crate) fn add_uniform_bool(
    shader_creator: &mut dyn GpuShaderCreator,
    getter: impl Fn() -> bool + Send + Sync + 'static,
    name: &str,
) -> Result<()> {
    // Add the uniform if it does not already exist.
    if shader_creator.add_uniform_bool(name, Arc::new(getter))? {
        // Declare uniform.
        let mut st_decl = GpuShaderText::new(shader_creator.language());
        st_decl.declare_uniform_bool(name);
        shader_creator.add_to_parameter_declare_shader_code(st_decl.as_str());
    }
    Ok(())
}

/// Port of `BuildResourceNameIndexed`.
pub(crate) fn build_resource_name_indexed(
    shader_creator: &dyn GpuShaderCreator,
    prefix: &str,
    base: &str,
    index: u32,
) -> String {
    let name = format!(
        "{}_{}",
        build_resource_name(shader_creator, prefix, base),
        index
    );
    // Note: Remove potentially problematic double underscores from GLSL
    // resource names.
    replace_all(&name, "__", "_")
}

const OP_PREFIX: &str = "grading_rgbcurve";

fn set_gc_properties(
    shader_creator: &mut dyn GpuShaderCreator,
    dynamic: bool,
    p: &mut GcProperties,
) {
    if dynamic {
        // If there are several dynamic ops, they will use the same names for
        // uniforms.
        for n in [
            &mut p.knots_offsets,
            &mut p.knots,
            &mut p.coefs_offsets,
            &mut p.coefs,
            &mut p.local_bypass,
            &mut p.eval,
        ] {
            *n = build_resource_name(shader_creator, OP_PREFIX, n);
        }
    } else {
        // Non-dynamic ops need an helper function for each op.
        let res_index = shader_creator.next_resource_index();
        for n in [
            &mut p.knots_offsets,
            &mut p.knots,
            &mut p.coefs_offsets,
            &mut p.coefs,
            &mut p.eval,
        ] {
            *n = build_resource_name_indexed(shader_creator, OP_PREFIX, n, res_index);
        }
    }
}

/// Uniforms of the knots & coefs of a dynamic curve property. `kc` computes
/// the knots & coefs of the current value of the property.
pub(crate) fn add_gc_properties_uniforms(
    shader_creator: &mut dyn GpuShaderCreator,
    kc: Arc<dyn Fn() -> KnotsCoefs + Send + Sync>,
    num_offset_values: i32,
    p: &GcProperties,
) -> Result<()> {
    // Note: No need to add an index to the name to avoid collisions as the
    // dynamic properties are unique.
    let (k1, k2, k3, k4, k5, k6, k7) = (
        kc.clone(),
        kc.clone(),
        kc.clone(),
        kc.clone(),
        kc.clone(),
        kc.clone(),
        kc,
    );

    // Uniforms are added if they are not already there (added by another op).
    add_uniform_vector_int(
        shader_creator,
        move || num_offset_values,
        move || k1().knots_offsets,
        num_offset_values as u32,
        &p.knots_offsets,
    )?;
    add_uniform_vector_float(
        shader_creator,
        move || k2().num_knots,
        move || {
            let kc = k3();
            let n = (kc.num_knots.max(0) as usize).min(kc.knots.len());
            kc.knots[..n].to_vec()
        },
        KnotsCoefs::MAX_NUM_KNOTS as u32,
        &p.knots,
    )?;
    add_uniform_vector_int(
        shader_creator,
        move || num_offset_values,
        move || k4().coefs_offsets,
        num_offset_values as u32,
        &p.coefs_offsets,
    )?;
    add_uniform_vector_float(
        shader_creator,
        move || k5().num_coefs,
        move || {
            let kc = k6();
            let n = (kc.num_coefs.max(0) as usize).min(kc.coefs.len());
            kc.coefs[..n].to_vec()
        },
        KnotsCoefs::MAX_NUM_COEFS as u32,
        &p.coefs,
    )?;
    add_uniform_bool(shader_creator, move || k7().local_bypass, &p.local_bypass)?;
    Ok(())
}

/// Declare the knots & coefs of a non-dynamic curve as constant arrays.
pub(crate) fn declare_knots_coefs_arrays(
    st: &mut GpuShaderText,
    kc: &KnotsCoefs,
    p: &GcProperties,
) -> Result<()> {
    let num_knots = (kc.num_knots.max(0) as usize).min(kc.knots.len());
    let num_coefs = (kc.num_coefs.max(0) as usize).min(kc.coefs.len());
    // 2 ints for each curve.
    nl!(st, "");
    st.declare_int_array_const(&p.knots_offsets, &kc.knots_offsets)?;
    st.declare_float_array_const(&p.knots, &kc.knots[..num_knots])?;
    st.declare_int_array_const(&p.coefs_offsets, &kc.coefs_offsets)?;
    st.declare_float_array_const(&p.coefs, &kc.coefs[..num_coefs])?;
    Ok(())
}

fn add_curve_eval_method_text_to_shader_program(
    shader_creator: &mut dyn GpuShaderCreator,
    op: &GradingRgbCurveOp,
    p: &GcProperties,
    dyn_: bool,
) -> Result<()> {
    let lang = shader_creator.language();
    let mut st = GpuShaderText::new(lang);

    // Dynamic version uses uniforms declared globally. Non-dynamic version
    // declares local variables in the op specific helper function.
    if !dyn_ {
        declare_knots_coefs_arrays(&mut st, &op.knots_coefs(), p)?;
    }

    nl!(st, "");
    if lang == GpuLanguage::Osl1 || lang == GpuLanguage::Msl2_0 {
        nl!(
            st,
            st.float_keyword(),
            " ",
            p.eval,
            "(int curveIdx, float x, float identity_x)"
        );
    } else {
        nl!(
            st,
            st.float_keyword(),
            " ",
            p.eval,
            "(in int curveIdx, in float x, in float identity_x)"
        );
    }
    nl!(st, "{");
    st.indent();
    if op.direction() == TransformDirection::Inverse {
        add_shader_eval_rev(
            &mut st,
            &p.knots_offsets,
            &p.coefs_offsets,
            &p.knots,
            &p.coefs,
        );
    } else {
        add_shader_eval_fwd(
            &mut st,
            &p.knots_offsets,
            &p.coefs_offsets,
            &p.knots,
            &p.coefs,
        );
    }
    st.dedent();
    nl!(st, "}");

    shader_creator.add_to_helper_shader_code(st.as_str());
    Ok(())
}

fn add_gc_shader(
    shader_creator: &dyn GpuShaderCreator,
    st: &mut GpuShaderText,
    p: &GcProperties,
    dyn_: bool,
    do_lin_to_log: bool,
    forward: bool,
) -> Result<()> {
    if dyn_ {
        nl!(st, "if (!", st.cast_to_bool(&p.local_bypass), ")");
        nl!(st, "{");
        st.indent();
    }

    if do_lin_to_log {
        // NB: Although the linToLog and logToLin are correct inverses, the
        // limits of floating-point arithmetic cause errors in the lowest bit
        // of the round trip.
        nl!(st, "// Convert from lin to log.");
        add_lin_to_log_shader(shader_creator, st)?;
        nl!(st, "");
    }

    let pix = shader_creator.pixel_name();
    let e = &p.eval;

    let rgb = |st: &mut GpuShaderText| {
        nl!(
            st,
            pix,
            ".rgb.r = ",
            e,
            "(0, ",
            pix,
            ".rgb.r, ",
            pix,
            ".rgb.r);"
        ); // RED
        nl!(
            st,
            pix,
            ".rgb.g = ",
            e,
            "(1, ",
            pix,
            ".rgb.g, ",
            pix,
            ".rgb.g);"
        ); // GREEN
        nl!(
            st,
            pix,
            ".rgb.b = ",
            e,
            "(2, ",
            pix,
            ".rgb.b, ",
            pix,
            ".rgb.b);"
        ); // BLUE
    };
    let master = |st: &mut GpuShaderText| {
        nl!(
            st,
            pix,
            ".rgb.r = ",
            e,
            "(3, ",
            pix,
            ".rgb.r, ",
            pix,
            ".rgb.r);"
        ); // MASTER
        nl!(
            st,
            pix,
            ".rgb.g = ",
            e,
            "(3, ",
            pix,
            ".rgb.g, ",
            pix,
            ".rgb.g);"
        ); // MASTER
        nl!(
            st,
            pix,
            ".rgb.b = ",
            e,
            "(3, ",
            pix,
            ".rgb.b, ",
            pix,
            ".rgb.b);"
        ); // MASTER
    };

    // Call the curve evaluation method for each curve.
    if forward {
        rgb(st);
        master(st);
    } else {
        master(st);
        rgb(st);
    }

    if do_lin_to_log {
        nl!(st, "");
        nl!(st, "// Convert from log to lin.");
        add_log_to_lin_shader(shader_creator, st)?;
    }

    if dyn_ {
        st.dedent();
        nl!(st, "}");
    }
    Ok(())
}

/// Port of `GetGradingRGBCurveGPUShaderProgram`.
pub(crate) fn extract(
    op: &GradingRgbCurveOp,
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
    nl!(
        st,
        "// Add GradingRGBCurve '",
        style.as_str(),
        "' ",
        dir.as_str(),
        " processing"
    );
    nl!(st, "");
    nl!(st, "{");
    st.indent();

    let mut p = GcProperties::default();
    set_gc_properties(shader_creator, dyn_, &mut p);

    if dyn_ {
        // Add the dynamic property to the shader creator (decoupled).
        let prop = SharedValue::new(op.value());
        shader_creator.add_dynamic_property(DynamicProperty::GradingRgbCurve(prop.clone()))?;

        // Add uniforms only if needed.
        let kc: Arc<dyn Fn() -> KnotsCoefs + Send + Sync> = Arc::new(move || {
            KnotsCoefs::from_rgb_curve(&prop.get()).unwrap_or_else(|_| KnotsCoefs::new(4))
        });
        add_gc_properties_uniforms(shader_creator, kc, 8, &p)?;
    }

    // Add the helper function (plus the global variables if not dynamic).
    add_curve_eval_method_text_to_shader_program(shader_creator, op, &p, dyn_)?;

    let do_lin_to_log = style == GradingStyle::Lin && !op.bypass_lin_to_log();
    add_gc_shader(
        shader_creator,
        &mut st,
        &p,
        dyn_,
        do_lin_to_log,
        dir == TransformDirection::Forward,
    )?;

    st.dedent();
    nl!(st, "}");

    st.dedent();
    shader_creator.add_to_function_shader_code(st.as_str());
    Ok(())
}

// Shader parts of GradingBSplineCurve.cpp.

/// Port of `GradingBSplineCurveImpl::AddShaderEvalFwd`.
///
/// The input arguments are:
///      curveIdx -- The index of the curve being evaluated.
///             x -- The input value.
///    identity_x -- The desired output if there is no curve to evaluate.
pub(crate) fn add_shader_eval_fwd(
    st: &mut GpuShaderText,
    knots_offsets: &str,
    coefs_offsets: &str,
    knots: &str,
    coefs: &str,
) {
    nl!(st, "int knotsOffs = ", knots_offsets, "[curveIdx * 2];");
    nl!(st, "int knotsCnt = ", knots_offsets, "[curveIdx * 2 + 1];");
    nl!(st, "int coefsOffs = ", coefs_offsets, "[curveIdx * 2];");
    nl!(st, "int coefsCnt = ", coefs_offsets, "[curveIdx * 2 + 1];");
    nl!(st, "int coefsSets = coefsCnt / 3;");
    // If the curve has the default/identity values, the coef data is empty,
    // return the identity.
    nl!(st, "if (coefsSets == 0)");
    nl!(st, "{");
    nl!(st, "  return identity_x;");
    nl!(st, "}");

    nl!(st, "float knStart = ", knots, "[knotsOffs];");
    nl!(st, "float knEnd = ", knots, "[knotsOffs + knotsCnt - 1];");

    nl!(st, "if (x <= knStart)");
    nl!(st, "{");
    nl!(st, "  float B = ", coefs, "[coefsOffs + coefsSets];");
    nl!(st, "  float C = ", coefs, "[coefsOffs + coefsSets * 2];");
    nl!(st, "  return (x - knStart) * B + C;");
    nl!(st, "}");

    nl!(st, "else if (x >= knEnd)");
    nl!(st, "{");
    nl!(st, "  float A = ", coefs, "[coefsOffs + coefsSets - 1];");
    nl!(
        st,
        "  float B = ",
        coefs,
        "[coefsOffs + coefsSets * 2 - 1];"
    );
    nl!(
        st,
        "  float C = ",
        coefs,
        "[coefsOffs + coefsSets * 3 - 1];"
    );
    nl!(st, "  float kn = ", knots, "[knotsOffs + knotsCnt - 2];");
    nl!(st, "  float t = knEnd - kn;");
    nl!(st, "  float slope = 2. * A * t + B;");
    nl!(st, "  float offs = ( A * t + B ) * t + C;");
    nl!(st, "  return (x - knEnd) * slope + offs;");
    nl!(st, "}");

    // else
    nl!(st, "int i = 0;");
    nl!(st, "for (i = 0; i < knotsCnt - 2; ++i)");
    nl!(st, "{");
    nl!(st, "  if (x < ", knots, "[knotsOffs + i + 1])");
    nl!(st, "  {");
    nl!(st, "    break;");
    nl!(st, "  }");
    nl!(st, "}");

    nl!(st, "float A = ", coefs, "[coefsOffs + i];");
    nl!(st, "float B = ", coefs, "[coefsOffs + coefsSets + i];");
    nl!(st, "float C = ", coefs, "[coefsOffs + coefsSets * 2 + i];");
    nl!(st, "float kn = ", knots, "[knotsOffs + i];");
    nl!(st, "float t = x - kn;");
    nl!(st, "return ( A * t + B ) * t + C;");
}

/// Port of `GradingBSplineCurveImpl::AddShaderEvalRev`.
///
/// The input arguments are:
///      curveIdx -- The index of the curve being evaluated.
///             x -- The input value.
pub(crate) fn add_shader_eval_rev(
    st: &mut GpuShaderText,
    knots_offsets: &str,
    coefs_offsets: &str,
    knots: &str,
    coefs: &str,
) {
    nl!(st, "int knotsOffs = ", knots_offsets, "[curveIdx * 2];");
    nl!(st, "int knotsCnt = ", knots_offsets, "[curveIdx * 2 + 1];");
    nl!(st, "int coefsOffs = ", coefs_offsets, "[curveIdx * 2];");
    nl!(st, "int coefsCnt = ", coefs_offsets, "[curveIdx * 2 + 1];");
    nl!(st, "int coefsSets = coefsCnt / 3;");

    nl!(st, "if (coefsSets == 0)");
    nl!(st, "{");
    nl!(st, "  return x;");
    nl!(st, "}");

    nl!(st, "float knStart = ", knots, "[knotsOffs];");
    nl!(st, "float knEnd = ", knots, "[knotsOffs + knotsCnt - 1];");
    nl!(
        st,
        "float knStartY = ",
        coefs,
        "[coefsOffs + coefsSets * 2];"
    );
    nl!(st, "float knEndY;");
    nl!(st, "{");
    nl!(st, "  float A = ", coefs, "[coefsOffs + coefsSets - 1];");
    nl!(
        st,
        "  float B = ",
        coefs,
        "[coefsOffs + coefsSets * 2 - 1];"
    );
    nl!(
        st,
        "  float C = ",
        coefs,
        "[coefsOffs + coefsSets * 3 - 1];"
    );
    nl!(st, "  float kn = ", knots, "[knotsOffs + knotsCnt - 2];");
    nl!(st, "  float t = knEnd - kn;");
    nl!(st, "  knEndY = ( A * t + B ) * t + C;");
    nl!(st, "}");

    nl!(st, "if (x <= knStartY)");
    nl!(st, "{");
    nl!(st, "  float B = ", coefs, "[coefsOffs + coefsSets];");
    nl!(st, "  float C = ", coefs, "[coefsOffs + coefsSets * 2];");
    nl!(
        st,
        "  return abs(B) < 1e-5 ? knStart : (x - C) / B + knStart;"
    );
    nl!(st, "}");

    nl!(st, "else if (x >= knEndY)");
    nl!(st, "{");
    nl!(st, "  float A = ", coefs, "[coefsOffs + coefsSets - 1];");
    nl!(
        st,
        "  float B = ",
        coefs,
        "[coefsOffs + coefsSets * 2 - 1];"
    );
    nl!(
        st,
        "  float C = ",
        coefs,
        "[coefsOffs + coefsSets * 3 - 1];"
    );
    nl!(st, "  float kn = ", knots, "[knotsOffs + knotsCnt - 2];");
    nl!(st, "  float t = knEnd - kn;");
    nl!(st, "  float slope = 2. * A * t + B;");
    nl!(st, "  float offs = ( A * t + B ) * t + C;");
    nl!(
        st,
        "  return abs(slope) < 1e-5 ? knEnd : (x - offs) / slope + knEnd;"
    );
    nl!(st, "}");

    // else
    nl!(st, "int i = 0;");
    nl!(st, "for (i = 0; i < knotsCnt - 2; ++i)");
    nl!(st, "{");
    nl!(
        st,
        "  if (x < ",
        coefs,
        "[coefsOffs + coefsSets * 2 + i + 1])"
    );
    nl!(st, "  {");
    nl!(st, "    break;");
    nl!(st, "  }");
    nl!(st, "}");

    nl!(st, "float A = ", coefs, "[coefsOffs + i];");
    nl!(st, "float B = ", coefs, "[coefsOffs + coefsSets + i];");
    nl!(st, "float C = ", coefs, "[coefsOffs + coefsSets * 2 + i];");
    nl!(st, "float kn = ", knots, "[knotsOffs + i];");
    nl!(st, "float C0 = C - x;");
    nl!(st, "float discrim = sqrt(B * B - 4. * A * C0);");
    nl!(st, "float denom = discrim + B;");
    nl!(st, "if (abs(denom) < 1e-5)");
    nl!(st, "{");
    nl!(st, "  return abs(B) < 1e-5 ? kn : kn + (-C0 / B);");
    nl!(st, "}");
    nl!(st, "return kn + (-2. * C0) / denom;");
}

/// Port of `GradingBSplineCurveImpl::AddShaderEvalRevHue`.
pub(crate) fn add_shader_eval_rev_hue(
    st: &mut GpuShaderText,
    knots_offsets: &str,
    coefs_offsets: &str,
    knots: &str,
    coefs: &str,
) {
    nl!(st, "int knotsOffs = ", knots_offsets, "[curveIdx * 2];");
    nl!(st, "int knotsCnt = ", knots_offsets, "[curveIdx * 2 + 1];");
    nl!(st, "int coefsOffs = ", coefs_offsets, "[curveIdx * 2];");
    nl!(st, "int coefsCnt = ", coefs_offsets, "[curveIdx * 2 + 1];");
    nl!(st, "int coefsSets = coefsCnt / 3;");

    nl!(st, "if (coefsSets == 0)");
    nl!(st, "{");
    nl!(st, "  return x;");
    nl!(st, "}");

    nl!(st, "float knStart = ", knots, "[knotsOffs];");
    nl!(st, "float knEnd = ", knots, "[knotsOffs + knotsCnt - 1];");
    nl!(
        st,
        "float knStartY = ",
        coefs,
        "[coefsOffs + coefsSets * 2];"
    );
    nl!(st, "float knEndY;");
    nl!(st, "{");
    nl!(st, "  float A = ", coefs, "[coefsOffs + coefsSets - 1];");
    nl!(
        st,
        "  float B = ",
        coefs,
        "[coefsOffs + coefsSets * 2 - 1];"
    );
    nl!(
        st,
        "  float C = ",
        coefs,
        "[coefsOffs + coefsSets * 3 - 1];"
    );
    nl!(st, "  float kn = ", knots, "[knotsOffs + knotsCnt - 2];");
    nl!(st, "  float t = knEnd - kn;");
    nl!(st, "  knEndY = ( A * t + B ) * t + C;");
    // The HUE-FX curve is index 7 and requires special handling.
    nl!(st, "  knEndY = (curveIdx == 7) ? knEndY + knEnd : knEndY;");
    nl!(st, "}");

    nl!(st, "if (x < knStartY)");
    nl!(st, "{");
    nl!(st, "  x = x + ceil(knStartY - x);");
    nl!(st, "}");

    nl!(st, "else if (x > knEndY)");
    nl!(st, "{");
    nl!(st, "  x = x - ceil(x - knEndY);");
    nl!(st, "}");

    nl!(st, "int i = 0;");
    nl!(st, "for (i = 0; i < knotsCnt - 2; ++i)");
    nl!(st, "{");
    nl!(
        st,
        "  float curve_x = ",
        coefs,
        "[coefsOffs + coefsSets * 2 + i + 1];"
    );
    nl!(
        st,
        "  curve_x = (curveIdx == 7) ? curve_x + ",
        knots,
        "[knotsOffs + i + 1] : curve_x;"
    );
    nl!(st, "  if (x < curve_x)");
    nl!(st, "  {");
    nl!(st, "    break;");
    nl!(st, "  }");
    nl!(st, "}");

    nl!(st, "float A = ", coefs, "[coefsOffs + i];");
    nl!(st, "float B = ", coefs, "[coefsOffs + coefsSets + i];");
    nl!(st, "float C = ", coefs, "[coefsOffs + coefsSets * 2 + i];");
    nl!(st, "float kn = ", knots, "[knotsOffs + i];");
    nl!(st, "if (curveIdx == 7)");
    nl!(st, "{");
    nl!(st, "  C = C + kn;");
    nl!(st, "  B = B + 1.;");
    nl!(st, "}");
    nl!(st, "float C0 = C - x;");
    nl!(st, "float discrim = sqrt(B * B - 4. * A * C0);");
    nl!(st, "float denom = discrim + B;");
    nl!(st, "if (abs(denom) < 1e-5)");
    nl!(st, "{");
    nl!(st, "  return abs(B) < 1e-5 ? kn : kn + (-C0 / B);");
    nl!(st, "}");
    nl!(st, "return kn + (-2. * C0) / denom;");
}
