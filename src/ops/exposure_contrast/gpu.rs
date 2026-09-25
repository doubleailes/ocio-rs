//! GPU renderer of the exposure / contrast op (port of
//! `ExposureContrastOpGPU.cpp`).

use std::sync::Arc;

use crate::config::logging::log_warning;
use crate::dynamic_property::{DynamicProperty, SharedValue};
use crate::error::Result;
use crate::gpu::shader_text::build_resource_name;
use crate::gpu::{GpuShaderCreator, GpuShaderText};
use crate::nl;
use crate::ops::exposure_contrast::{
    DoubleProperty, EcStyle, ExposureContrastOp, ExposureContrastOpData, MIN_CONTRAST, MIN_PIVOT,
    VIDEO_OETF_POWER,
};
use crate::types::{DynamicPropertyType, GpuLanguage};

const EC_EXPOSURE: &str = "exposureVal";
const EC_CONTRAST: &str = "contrastVal";
const EC_GAMMA: &str = "gammaVal";

fn add_uniform(
    shader_creator: &mut dyn GpuShaderCreator,
    value: SharedValue<f64>,
    name: &str,
) -> Result<()> {
    shader_creator.add_uniform_double(name, Arc::new(move || value.get()))?;
    // Declare uniform.
    let mut st_decl = GpuShaderText::new(shader_creator.language());
    st_decl.declare_uniform_float(name);
    shader_creator.add_to_parameter_declare_shader_code(st_decl.as_str());
    Ok(())
}

fn add_property(
    shader_creator: &mut dyn GpuShaderCreator,
    st: &mut GpuShaderText,
    prop: &DoubleProperty,
    ty: DynamicPropertyType,
    name: &str,
) -> Result<String> {
    if prop.is_dynamic() && shader_creator.language() != GpuLanguage::Osl1 {
        // Build the name for the uniform. The same type of property should
        // give the same name, so that the uniform is declared only once, but
        // multiple instances of the shader code can reference that name.
        // Note: No need to add an index to the name to avoid collisions as
        // the dynamic properties are unique.
        let final_name = build_resource_name(shader_creator, "exposure_contrast", name);

        // Property is decoupled and added to the shader creator.
        let shader_prop = SharedValue::new(prop.value());
        let new_prop = match ty {
            DynamicPropertyType::Exposure => DynamicProperty::Exposure(shader_prop.clone()),
            DynamicPropertyType::Contrast => DynamicProperty::Contrast(shader_prop.clone()),
            _ => DynamicProperty::Gamma(shader_prop.clone()),
        };
        shader_creator.add_dynamic_property(new_prop)?;

        // Uniform is added, connected to the shader creator instance of the
        // dynamic property.
        add_uniform(shader_creator, shader_prop, &final_name)?;
        Ok(final_name)
    } else {
        // Declare a local variable to be used by the shader code.
        st.declare_var(name, prop.value() as f32)?;

        if shader_creator.language() == GpuLanguage::Osl1 && prop.is_dynamic() {
            log_warning(&format!(
                "The dynamic properties are not yet supported by the 'Open Shading language \
                 (OSL)' translation: The '{name}' dynamic property is replaced by a local variable."
            ));
        }
        Ok(name.to_string())
    }
}

struct Names {
    exposure: String,
    contrast: String,
    gamma: String,
}

fn add_properties(
    shader_creator: &mut dyn GpuShaderCreator,
    st: &mut GpuShaderText,
    ec: &ExposureContrastOpData,
) -> Result<Names> {
    Ok(Names {
        exposure: add_property(
            shader_creator,
            st,
            &ec.exposure,
            DynamicPropertyType::Exposure,
            EC_EXPOSURE,
        )?,
        contrast: add_property(
            shader_creator,
            st,
            &ec.contrast,
            DynamicPropertyType::Contrast,
            EC_CONTRAST,
        )?,
        gamma: add_property(
            shader_creator,
            st,
            &ec.gamma,
            DynamicPropertyType::Gamma,
            EC_GAMMA,
        )?,
    })
}

/// `outColor = pow(max(0, outColor/pivot), contrast) * pivot;`
fn add_contrast_block(st: &mut GpuShaderText, pix: &str, pivot: f64) {
    nl!(st, "if (contrast != 1.0)");
    nl!(st, "{");
    st.indent();
    nl!(
        st,
        pix,
        ".rgb = ",
        "pow( ",
        "max( ",
        st.float3_const1(0.0f32),
        ", ",
        pix,
        ".rgb / ",
        st.float3_const1(pivot),
        " ), ",
        st.float3_const1("contrast"),
        " ) * ",
        st.float3_const1(pivot),
        ";"
    );
    st.dedent();
    nl!(st, "}");
}

fn add_ec_linear_shader(
    st: &mut GpuShaderText,
    pix: &str,
    ec: &ExposureContrastOpData,
    n: &Names,
) -> Result<()> {
    let pivot = MIN_PIVOT.max(ec.pivot);

    nl!(
        st,
        st.float_decl("exposure")?,
        " = pow( 2., ",
        n.exposure,
        " );"
    );
    nl!(
        st,
        st.float_decl("contrast")?,
        " = max( ",
        MIN_CONTRAST,
        ", ",
        "( ",
        n.contrast,
        " * ",
        n.gamma,
        " ) );"
    );
    nl!(st, pix, ".rgb = ", pix, ".rgb * exposure;");

    add_contrast_block(st, pix, pivot);
    Ok(())
}

fn add_ec_linear_rev_shader(
    st: &mut GpuShaderText,
    pix: &str,
    ec: &ExposureContrastOpData,
    n: &Names,
) -> Result<()> {
    let pivot = MIN_PIVOT.max(ec.pivot);

    nl!(
        st,
        st.float_decl("exposure")?,
        " = pow( 2., ",
        n.exposure,
        " );"
    );
    nl!(
        st,
        st.float_decl("contrast")?,
        " = 1. / max( ",
        MIN_CONTRAST,
        ", ",
        "( ",
        n.contrast,
        " * ",
        n.gamma,
        " ) );"
    );

    add_contrast_block(st, pix, pivot);

    nl!(st, pix, ".rgb = ", pix, ".rgb / exposure;");
    Ok(())
}

fn add_ec_video_shader(
    st: &mut GpuShaderText,
    pix: &str,
    ec: &ExposureContrastOpData,
    n: &Names,
) -> Result<()> {
    let pivot = MIN_PIVOT.max(ec.pivot).powf(VIDEO_OETF_POWER);

    nl!(
        st,
        st.float_decl("exposure")?,
        " = pow( pow( 2., ",
        n.exposure,
        " ), ",
        VIDEO_OETF_POWER,
        ");"
    );
    nl!(
        st,
        st.float_decl("contrast")?,
        " = max( ",
        MIN_CONTRAST,
        ", ",
        "( ",
        n.contrast,
        " * ",
        n.gamma,
        " ) );"
    );
    nl!(st, pix, ".rgb = ", pix, ".rgb * exposure;");

    add_contrast_block(st, pix, pivot);
    Ok(())
}

fn add_ec_video_rev_shader(
    st: &mut GpuShaderText,
    pix: &str,
    ec: &ExposureContrastOpData,
    n: &Names,
) -> Result<()> {
    let pivot = MIN_PIVOT.max(ec.pivot).powf(VIDEO_OETF_POWER);

    nl!(
        st,
        st.float_decl("exposure")?,
        " = pow( pow( 2., ",
        n.exposure,
        " ), ",
        VIDEO_OETF_POWER,
        ");"
    );
    nl!(
        st,
        st.float_decl("contrast")?,
        " = 1. / max( ",
        MIN_CONTRAST,
        ", ",
        "( ",
        n.contrast,
        " * ",
        n.gamma,
        " ) );"
    );

    add_contrast_block(st, pix, pivot);

    nl!(st, pix, ".rgb = ", pix, ".rgb / exposure;");
    Ok(())
}

fn log_pivot(ec: &ExposureContrastOpData) -> f32 {
    let pivot = MIN_PIVOT.max(ec.pivot);
    0.0f64.max((pivot / 0.18).log2() * ec.log_exposure_step + ec.log_mid_gray) as f32
}

fn add_ec_logarithmic_shader(
    st: &mut GpuShaderText,
    pix: &str,
    ec: &ExposureContrastOpData,
    n: &Names,
) -> Result<()> {
    let log_pivot = log_pivot(ec);

    nl!(
        st,
        st.float_decl("exposure")?,
        " = ",
        n.exposure,
        " * ",
        ec.log_exposure_step,
        ";"
    );
    nl!(
        st,
        st.float_decl("contrast")?,
        " = max( ",
        MIN_CONTRAST,
        ", ",
        "( ",
        n.contrast,
        " * ",
        n.gamma,
        " ) );"
    );
    nl!(
        st,
        st.float_decl("offset")?,
        " = ( exposure - ",
        log_pivot,
        " ) * contrast + ",
        log_pivot,
        ";"
    );

    nl!(st, pix, ".rgb = ", pix, ".rgb * contrast + offset;");
    Ok(())
}

fn add_ec_logarithmic_rev_shader(
    st: &mut GpuShaderText,
    pix: &str,
    ec: &ExposureContrastOpData,
    n: &Names,
) -> Result<()> {
    let log_pivot = log_pivot(ec);

    nl!(
        st,
        st.float_decl("exposure")?,
        " = ",
        n.exposure,
        " * ",
        ec.log_exposure_step,
        ";"
    );
    nl!(
        st,
        st.float_decl("contrast")?,
        " = max( ",
        MIN_CONTRAST,
        ", ",
        "( ",
        n.contrast,
        " * ",
        n.gamma,
        " ) );"
    );
    nl!(
        st,
        st.float_decl("offset")?,
        " = ",
        log_pivot,
        " - ",
        log_pivot,
        " / contrast - exposure;"
    );

    nl!(st, pix, ".rgb = ", pix, ".rgb / contrast + offset;");
    Ok(())
}

/// Port of `GetExposureContrastGPUShaderProgram`.
pub(crate) fn exposure_contrast_shader_program(
    shader_creator: &mut dyn GpuShaderCreator,
    ec: &ExposureContrastOpData,
) -> Result<()> {
    let mut st = GpuShaderText::new(shader_creator.language());
    st.indent();

    nl!(st, "");
    nl!(
        st,
        "// Add ExposureContrast '",
        ec.style.as_str(),
        "' processing"
    );
    nl!(st, "");
    nl!(st, "{");
    st.indent();

    let names = add_properties(shader_creator, &mut st, ec)?;
    let pix = shader_creator.pixel_name().to_string();

    match ec.style {
        EcStyle::Linear => add_ec_linear_shader(&mut st, &pix, ec, &names)?,
        EcStyle::LinearRev => add_ec_linear_rev_shader(&mut st, &pix, ec, &names)?,
        EcStyle::Video => add_ec_video_shader(&mut st, &pix, ec, &names)?,
        EcStyle::VideoRev => add_ec_video_rev_shader(&mut st, &pix, ec, &names)?,
        EcStyle::Logarithmic => add_ec_logarithmic_shader(&mut st, &pix, ec, &names)?,
        EcStyle::LogarithmicRev => add_ec_logarithmic_rev_shader(&mut st, &pix, ec, &names)?,
    }

    st.dedent();
    nl!(st, "}");

    st.dedent();
    shader_creator.add_to_function_shader_code(st.as_str());
    Ok(())
}

/// Port of `ExposureContrastOp::extractGpuShaderInfo`.
pub(crate) fn extract(
    op: &ExposureContrastOp,
    shader_creator: &mut dyn GpuShaderCreator,
) -> Result<()> {
    exposure_contrast_shader_program(shader_creator, op.data())
}
