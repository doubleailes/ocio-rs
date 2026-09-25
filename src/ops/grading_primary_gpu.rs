//! GPU renderer of the primary grading op (port of
//! `GradingPrimaryOpGPU.cpp`).

use std::sync::Arc;

use crate::config::logging::log_warning;
use crate::dynamic_property::{DynamicProperty, SharedValue};
use crate::error::Result;
use crate::gpu::shader_text::build_resource_name;
use crate::gpu::{GpuShaderCreator, GpuShaderText};
use crate::nl;
use crate::ops::grading_primary::{GradingPrimaryOp, GradingPrimaryPreRender};
use crate::ops::Op;
use crate::transforms::grading::GradingPrimary;
use crate::types::{GpuLanguage, GradingStyle, TransformDirection};

/// Names of the shader variables (undecorated names are suitable for local
/// variables, uniforms get decorated names).
struct GpProperties {
    brightness: String,
    contrast: String,
    gamma: String,
    exposure: String,
    offset: String,
    slope: String,

    pivot: String,
    pivot_black: String,
    pivot_white: String,
    clamp_black: String,
    clamp_white: String,
    saturation: String,

    local_bypass: String,
}

impl Default for GpProperties {
    fn default() -> Self {
        Self {
            brightness: "brightness".into(),
            contrast: "contrast".into(),
            gamma: "gamma".into(),
            exposure: "exposure".into(),
            offset: "offset".into(),
            slope: "slope".into(),
            pivot: "pivot".into(),
            pivot_black: "pivotBlack".into(),
            pivot_white: "pivotWhite".into(),
            clamp_black: "clampBlack".into(),
            clamp_white: "clampWhite".into(),
            saturation: "saturation".into(),
            local_bypass: "localBypass".into(),
        }
    }
}

const OP_PREFIX: &str = "grading_primary";

fn add_uniform_double(
    shader_creator: &mut dyn GpuShaderCreator,
    getter: impl Fn() -> f64 + Send + Sync + 'static,
    name: &str,
) -> Result<()> {
    // Add the uniform if it does not already exist.
    if shader_creator.add_uniform_double(name, Arc::new(getter))? {
        // Declare uniform.
        let mut st_decl = GpuShaderText::new(shader_creator.language());
        st_decl.declare_uniform_float(name);
        shader_creator.add_to_parameter_declare_shader_code(st_decl.as_str());
    }
    Ok(())
}

fn add_uniform_bool(
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

fn add_uniform_float3(
    shader_creator: &mut dyn GpuShaderCreator,
    getter: impl Fn() -> [f32; 3] + Send + Sync + 'static,
    name: &str,
) -> Result<()> {
    // Add the uniform if it does not already exist.
    if shader_creator.add_uniform_float3(name, Arc::new(getter))? {
        // Declare uniform.
        let mut st_decl = GpuShaderText::new(shader_creator.language());
        st_decl.declare_uniform_float3(name);
        shader_creator.add_to_parameter_declare_shader_code(st_decl.as_str());
    }
    Ok(())
}

/// The shader copy of the dynamic property and the getters of its values.
struct ShaderProp {
    prop: SharedValue<GradingPrimary>,
    style: GradingStyle,
    dir: TransformDirection,
}

impl ShaderProp {
    /// Getter of a precomputed value.
    fn comp<T>(
        &self,
        f: impl Fn(&GradingPrimaryPreRender) -> T + Send + Sync + 'static,
    ) -> impl Fn() -> T + Send + Sync + 'static {
        let (prop, style, dir) = (self.prop.clone(), self.style, self.dir);
        move || f(&GradingPrimaryPreRender::new(style, dir, &prop.get()))
    }

    /// Getter of a value.
    fn value(
        &self,
        f: impl Fn(&GradingPrimary) -> f64 + Send + Sync + 'static,
    ) -> impl Fn() -> f64 + Send + Sync + 'static {
        let prop = self.prop.clone();
        move || prop.with(|v| f(v))
    }
}

/// Decorate a name for a uniform.
fn decorate(shader_creator: &dyn GpuShaderCreator, name: &mut String) {
    *name = build_resource_name(shader_creator, OP_PREFIX, name);
}

/// Add the dynamic property to the shader creator (decoupled copy).
fn add_shader_prop(
    shader_creator: &mut dyn GpuShaderCreator,
    op: &GradingPrimaryOp,
) -> Result<ShaderProp> {
    let prop = SharedValue::new(op.value());
    shader_creator.add_dynamic_property(DynamicProperty::GradingPrimary(prop.clone()))?;
    Ok(ShaderProp {
        prop,
        style: op.style(),
        dir: op.direction(),
    })
}

fn add_gp_log_properties(
    shader_creator: &mut dyn GpuShaderCreator,
    st: &mut GpuShaderText,
    op: &GradingPrimaryOp,
    names: &mut GpProperties,
    dyn_: bool,
) -> Result<()> {
    if dyn_ {
        // Build names. No need to add an index to the name to avoid
        // collisions as the dynamic properties are unique.
        for n in [
            &mut names.brightness,
            &mut names.contrast,
            &mut names.gamma,
            &mut names.pivot,
            &mut names.pivot_black,
            &mut names.pivot_white,
            &mut names.clamp_black,
            &mut names.clamp_white,
            &mut names.saturation,
            &mut names.local_bypass,
        ] {
            decorate(shader_creator, n);
        }

        // Property is decoupled and added to shader creator.
        let sp = add_shader_prop(shader_creator, op)?;

        // Add uniforms if they are not already there.
        add_uniform_float3(
            shader_creator,
            sp.comp(|c| c.brightness()),
            &names.brightness,
        )?;
        add_uniform_float3(shader_creator, sp.comp(|c| c.contrast()), &names.contrast)?;
        add_uniform_float3(shader_creator, sp.comp(|c| c.gamma()), &names.gamma)?;
        add_uniform_double(shader_creator, sp.comp(|c| c.pivot()), &names.pivot)?;
        add_uniform_double(
            shader_creator,
            sp.value(|v| v.pivot_black),
            &names.pivot_black,
        )?;
        add_uniform_double(
            shader_creator,
            sp.value(|v| v.pivot_white),
            &names.pivot_white,
        )?;
        add_uniform_double(
            shader_creator,
            sp.value(|v| v.clamp_black),
            &names.clamp_black,
        )?;
        add_uniform_double(
            shader_creator,
            sp.value(|v| v.clamp_white),
            &names.clamp_white,
        )?;
        add_uniform_double(
            shader_creator,
            sp.value(|v| v.saturation),
            &names.saturation,
        )?;
        add_uniform_bool(
            shader_creator,
            sp.comp(|c| c.local_bypass()),
            &names.local_bypass,
        )?;
    } else {
        let value = op.value();
        let comp = GradingPrimaryPreRender::new(op.style(), op.direction(), &value);

        st.declare_float3_arr(&names.brightness, &comp.brightness())?;
        st.declare_float3_arr(&names.contrast, &comp.contrast())?;
        st.declare_float3_arr(&names.gamma, &comp.gamma())?;

        st.declare_var_const(&names.pivot, comp.pivot() as f32)?;
        st.declare_var_const(&names.pivot_black, value.pivot_black as f32)?;
        st.declare_var_const(&names.pivot_white, value.pivot_white as f32)?;
        st.declare_var_const(&names.clamp_black, value.clamp_black as f32)?;
        st.declare_var_const(&names.clamp_white, value.clamp_white as f32)?;
        st.declare_var_const(&names.saturation, value.saturation as f32)?;
    }
    Ok(())
}

fn add_gamma_block(
    st: &mut GpuShaderText,
    pxl: &str,
    p: &GpProperties,
    extra_indent: &str,
) -> Result<()> {
    // Not sure if the if helps performance, but it does allow out == in at
    // the default values.
    nl!(
        st,
        "if ( ",
        st.vector_compare_expression(&p.gamma, "!=", &st.float3_const1(1.0f32)),
        " )"
    );
    nl!(st, "{");
    st.indent();
    nl!(
        st,
        st.float3_decl("normalizedOut")?,
        " = abs(",
        pxl,
        ".rgb - ",
        p.pivot_black,
        ") / ",
        "(",
        p.pivot_white,
        " - ",
        p.pivot_black,
        ");"
    );
    // NB: The sign(outColor.rgb) is a vec3, preserving the sign of each
    // channel.
    nl!(
        st,
        st.float3_decl("scale")?,
        " = sign(",
        pxl,
        ".rgb - ",
        p.pivot_black,
        ") * ",
        "(",
        p.pivot_white,
        " - ",
        p.pivot_black,
        ");"
    );
    nl!(
        st,
        extra_indent,
        pxl,
        ".rgb = pow( normalizedOut, ",
        p.gamma,
        " ) * scale + ",
        p.pivot_black,
        ";"
    );
    st.dedent();
    nl!(st, "}");
    Ok(())
}

fn add_saturation_fwd(st: &mut GpuShaderText, pxl: &str, p: &GpProperties) -> Result<()> {
    st.declare_float3("lumaWgts", 0.2126f32, 0.7152f32, 0.0722f32)?;
    nl!(
        st,
        st.float_decl("luma")?,
        " = dot( ",
        pxl,
        ".rgb, lumaWgts );"
    );
    nl!(
        st,
        pxl,
        ".rgb = luma + ",
        p.saturation,
        " * (",
        pxl,
        ".rgb - luma);"
    );
    Ok(())
}

fn add_saturation_inv(st: &mut GpuShaderText, pxl: &str, p: &GpProperties) -> Result<()> {
    nl!(
        st,
        "if (",
        p.saturation,
        " != 0. && ",
        p.saturation,
        " != 1.)"
    );
    nl!(st, "{");
    st.indent();
    st.declare_float3("lumaWgts", 0.2126f32, 0.7152f32, 0.0722f32)?;
    nl!(
        st,
        st.float_decl("luma")?,
        " = dot( ",
        pxl,
        ".rgb, lumaWgts );"
    );
    nl!(
        st,
        pxl,
        ".rgb = luma + (",
        pxl,
        ".rgb - luma) / ",
        p.saturation,
        ";"
    );
    st.dedent();
    nl!(st, "}");
    Ok(())
}

fn add_clamp(st: &mut GpuShaderText, pxl: &str, p: &GpProperties) {
    nl!(
        st,
        pxl,
        ".rgb = clamp( ",
        pxl,
        ".rgb, ",
        p.clamp_black,
        ", ",
        p.clamp_white,
        " );"
    );
}

fn add_gp_log_forward_shader(st: &mut GpuShaderText, pxl: &str, p: &GpProperties) -> Result<()> {
    nl!(st, pxl, ".rgb += ", p.brightness, ";");

    nl!(
        st,
        pxl,
        ".rgb = ( ",
        pxl,
        ".rgb - ",
        p.pivot,
        " ) * ",
        p.contrast,
        " + ",
        p.pivot,
        ";"
    );

    add_gamma_block(st, pxl, p, "")?;
    add_saturation_fwd(st, pxl, p)?;
    add_clamp(st, pxl, p);
    Ok(())
}

fn add_gp_log_inverse_shader(st: &mut GpuShaderText, pxl: &str, p: &GpProperties) -> Result<()> {
    add_clamp(st, pxl, p);
    add_saturation_inv(st, pxl, p)?;
    add_gamma_block(st, pxl, p, "")?;

    nl!(
        st,
        pxl,
        ".rgb = ( ",
        pxl,
        ".rgb - ",
        p.pivot,
        " ) * ",
        p.contrast,
        " + ",
        p.pivot,
        ";"
    );

    nl!(st, pxl, ".rgb += ", p.brightness, ";");
    Ok(())
}

fn add_gp_lin_properties(
    shader_creator: &mut dyn GpuShaderCreator,
    st: &mut GpuShaderText,
    op: &GradingPrimaryOp,
    names: &mut GpProperties,
    dyn_: bool,
) -> Result<()> {
    if dyn_ {
        for n in [
            &mut names.offset,
            &mut names.exposure,
            &mut names.contrast,
            &mut names.pivot,
            &mut names.clamp_black,
            &mut names.clamp_white,
            &mut names.saturation,
            &mut names.local_bypass,
        ] {
            decorate(shader_creator, n);
        }

        let sp = add_shader_prop(shader_creator, op)?;

        add_uniform_float3(shader_creator, sp.comp(|c| c.offset()), &names.offset)?;
        add_uniform_float3(shader_creator, sp.comp(|c| c.exposure()), &names.exposure)?;
        add_uniform_float3(shader_creator, sp.comp(|c| c.contrast()), &names.contrast)?;
        add_uniform_double(shader_creator, sp.comp(|c| c.pivot()), &names.pivot)?;
        add_uniform_double(
            shader_creator,
            sp.value(|v| v.clamp_black),
            &names.clamp_black,
        )?;
        add_uniform_double(
            shader_creator,
            sp.value(|v| v.clamp_white),
            &names.clamp_white,
        )?;
        add_uniform_double(
            shader_creator,
            sp.value(|v| v.saturation),
            &names.saturation,
        )?;
        add_uniform_bool(
            shader_creator,
            sp.comp(|c| c.local_bypass()),
            &names.local_bypass,
        )?;
    } else {
        let value = op.value();
        let comp = GradingPrimaryPreRender::new(op.style(), op.direction(), &value);

        st.declare_float3_arr(&names.offset, &comp.offset())?;
        st.declare_float3_arr(&names.exposure, &comp.exposure())?;
        st.declare_float3_arr(&names.contrast, &comp.contrast())?;

        st.declare_var_const(&names.pivot, comp.pivot() as f32)?;
        st.declare_var_const(&names.clamp_black, value.clamp_black as f32)?;
        st.declare_var_const(&names.clamp_white, value.clamp_white as f32)?;
        st.declare_var_const(&names.saturation, value.saturation as f32)?;
    }
    Ok(())
}

fn add_lin_contrast_block(st: &mut GpuShaderText, pxl: &str, p: &GpProperties) {
    // Not sure if the if helps performance, but it does allow out == in at
    // the default values. Although note that the log-to-lin in Tone Op also
    // prevents out == in.
    nl!(
        st,
        "if ( ",
        st.vector_compare_expression(&p.contrast, "!=", &st.float3_const1(1.0f32)),
        " )"
    );
    nl!(st, "{");
    st.indent();

    // NB: The sign(outColor.rgb) is a vec3, preserving the sign of each
    // channel.
    nl!(
        st,
        pxl,
        ".rgb = pow( abs(",
        pxl,
        ".rgb / ",
        p.pivot,
        "), ",
        p.contrast,
        " ) * ",
        "sign(",
        pxl,
        ".rgb) * ",
        p.pivot,
        ";"
    );
    st.dedent();
    nl!(st, "}");
}

fn add_gp_lin_forward_shader(st: &mut GpuShaderText, pxl: &str, p: &GpProperties) -> Result<()> {
    nl!(st, pxl, ".rgb += ", p.offset, ";");
    nl!(st, pxl, ".rgb *= ", p.exposure, ";");

    add_lin_contrast_block(st, pxl, p);
    add_saturation_fwd(st, pxl, p)?;
    add_clamp(st, pxl, p);
    Ok(())
}

fn add_gp_lin_inverse_shader(st: &mut GpuShaderText, pxl: &str, p: &GpProperties) -> Result<()> {
    add_clamp(st, pxl, p);
    add_saturation_inv(st, pxl, p)?;
    add_lin_contrast_block(st, pxl, p);

    nl!(st, pxl, ".rgb *= ", p.exposure, ";");
    nl!(st, pxl, ".rgb += ", p.offset, ";");
    Ok(())
}

fn add_gp_video_properties(
    shader_creator: &mut dyn GpuShaderCreator,
    st: &mut GpuShaderText,
    op: &GradingPrimaryOp,
    names: &mut GpProperties,
    dyn_: bool,
) -> Result<()> {
    if dyn_ {
        for n in [
            &mut names.gamma,
            &mut names.offset,
            &mut names.slope,
            &mut names.pivot_black,
            &mut names.pivot_white,
            &mut names.clamp_black,
            &mut names.clamp_white,
            &mut names.saturation,
            &mut names.local_bypass,
        ] {
            decorate(shader_creator, n);
        }

        let sp = add_shader_prop(shader_creator, op)?;

        add_uniform_float3(shader_creator, sp.comp(|c| c.gamma()), &names.gamma)?;
        add_uniform_float3(shader_creator, sp.comp(|c| c.offset()), &names.offset)?;
        add_uniform_float3(shader_creator, sp.comp(|c| c.slope()), &names.slope)?;
        add_uniform_double(
            shader_creator,
            sp.value(|v| v.pivot_black),
            &names.pivot_black,
        )?;
        add_uniform_double(
            shader_creator,
            sp.value(|v| v.pivot_white),
            &names.pivot_white,
        )?;
        add_uniform_double(
            shader_creator,
            sp.value(|v| v.clamp_black),
            &names.clamp_black,
        )?;
        add_uniform_double(
            shader_creator,
            sp.value(|v| v.clamp_white),
            &names.clamp_white,
        )?;
        add_uniform_double(
            shader_creator,
            sp.value(|v| v.saturation),
            &names.saturation,
        )?;
        add_uniform_bool(
            shader_creator,
            sp.comp(|c| c.local_bypass()),
            &names.local_bypass,
        )?;
    } else {
        let value = op.value();
        let comp = GradingPrimaryPreRender::new(op.style(), op.direction(), &value);

        st.declare_float3_arr(&names.gamma, &comp.gamma())?;
        st.declare_float3_arr(&names.offset, &comp.offset())?;
        st.declare_float3_arr(&names.slope, &comp.slope())?;

        st.declare_var_const(&names.pivot_black, value.pivot_black as f32)?;
        st.declare_var_const(&names.pivot_white, value.pivot_white as f32)?;
        st.declare_var_const(&names.clamp_black, value.clamp_black as f32)?;
        st.declare_var_const(&names.clamp_white, value.clamp_white as f32)?;
        st.declare_var_const(&names.saturation, value.saturation as f32)?;
    }
    Ok(())
}

fn add_gp_video_forward_shader(st: &mut GpuShaderText, pxl: &str, p: &GpProperties) -> Result<()> {
    nl!(st, pxl, ".rgb += ", p.offset, ";");
    nl!(
        st,
        pxl,
        ".rgb = ( ",
        pxl,
        ".rgb - ",
        p.pivot_black,
        " ) * ",
        p.slope,
        " + ",
        p.pivot_black,
        ";"
    );

    add_gamma_block(st, pxl, p, "  ")?;
    add_saturation_fwd(st, pxl, p)?;
    add_clamp(st, pxl, p);
    Ok(())
}

fn add_gp_video_inverse_shader(st: &mut GpuShaderText, pxl: &str, p: &GpProperties) -> Result<()> {
    add_clamp(st, pxl, p);
    add_saturation_inv(st, pxl, p)?;
    add_gamma_block(st, pxl, p, "")?;

    nl!(
        st,
        pxl,
        ".rgb = ( ",
        pxl,
        ".rgb - ",
        p.pivot_black,
        " ) * ",
        p.slope,
        " + ",
        p.pivot_black,
        ";"
    );
    nl!(st, pxl, ".rgb += ", p.offset, ";");
    Ok(())
}

/// Port of `GetGradingPrimaryGPUShaderProgram`.
pub(crate) fn extract(
    op: &GradingPrimaryOp,
    shader_creator: &mut dyn GpuShaderCreator,
) -> Result<()> {
    let lang = shader_creator.language();
    let is_dynamic = op.is_dynamic();
    let dyn_ = is_dynamic && lang != GpuLanguage::Osl1;
    if !dyn_ {
        let comp = GradingPrimaryPreRender::new(op.style(), op.direction(), &op.value());
        if comp.local_bypass() {
            return Ok(());
        }
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
        "// Add GradingPrimary '",
        style.as_str(),
        "' ",
        dir.as_str(),
        " processing"
    );
    nl!(st, "");
    nl!(st, "{");
    st.indent();

    // Properties hold shader variables names and are initialized with
    // undecorated names suitable for local variables.
    let mut p = GpProperties::default();
    match style {
        GradingStyle::Log => add_gp_log_properties(shader_creator, &mut st, op, &mut p, dyn_)?,
        GradingStyle::Lin => add_gp_lin_properties(shader_creator, &mut st, op, &mut p, dyn_)?,
        GradingStyle::Video => add_gp_video_properties(shader_creator, &mut st, op, &mut p, dyn_)?,
    }

    if dyn_ {
        nl!(st, "if (!", st.cast_to_bool(&p.local_bypass), ")");
        nl!(st, "{");
        st.indent();
    }

    let pxl = shader_creator.pixel_name().to_string();
    let fwd = dir == TransformDirection::Forward;
    match (style, fwd) {
        (GradingStyle::Log, true) => add_gp_log_forward_shader(&mut st, &pxl, &p)?,
        (GradingStyle::Log, false) => add_gp_log_inverse_shader(&mut st, &pxl, &p)?,
        (GradingStyle::Lin, true) => add_gp_lin_forward_shader(&mut st, &pxl, &p)?,
        (GradingStyle::Lin, false) => add_gp_lin_inverse_shader(&mut st, &pxl, &p)?,
        (GradingStyle::Video, true) => add_gp_video_forward_shader(&mut st, &pxl, &p)?,
        (GradingStyle::Video, false) => add_gp_video_inverse_shader(&mut st, &pxl, &p)?,
    }

    if dyn_ {
        st.dedent();
        nl!(st, "}");
    }

    st.dedent();
    nl!(st, "}");

    st.dedent();
    shader_creator.add_to_function_shader_code(st.as_str());
    Ok(())
}
