//! GPU renderer of the tone grading op (port of `GradingToneOpGPU.cpp`).

use std::sync::Arc;

use crate::config::logging::log_warning;
use crate::dynamic_property::{DynamicProperty, SharedValue};
use crate::error::Result;
use crate::gpu::shader_text::{add_lin_to_log_shader, add_log_to_lin_shader, build_resource_name};
use crate::gpu::{GpuShaderCreator, GpuShaderText};
use crate::nl;
use crate::ops::grading_tone::{GradingToneOp, GradingTonePreRender};
use crate::ops::Op;
use crate::transforms::grading::GradingTone;
use crate::types::{GpuLanguage, GradingStyle, TransformDirection};

/// Names of the shader variables.
struct GtProperties {
    blacks_r: String,
    blacks_g: String,
    blacks_b: String,
    blacks_m: String,
    blacks_s: String,
    blacks_w: String,

    shadows_r: String,
    shadows_g: String,
    shadows_b: String,
    shadows_m: String,
    shadows_s: String,
    shadows_w: String,

    midtones_r: String,
    midtones_g: String,
    midtones_b: String,
    midtones_m: String,
    midtones_s: String,
    midtones_w: String,

    highlights_r: String,
    highlights_g: String,
    highlights_b: String,
    highlights_m: String,
    highlights_s: String,
    highlights_w: String,

    whites_r: String,
    whites_g: String,
    whites_b: String,
    whites_m: String,
    whites_s: String,
    whites_w: String,

    s_contrast: String,

    local_bypass: String,
}

impl Default for GtProperties {
    fn default() -> Self {
        Self {
            blacks_r: "blacksR".into(),
            blacks_g: "blacksG".into(),
            blacks_b: "blacksB".into(),
            blacks_m: "blacksM".into(),
            blacks_s: "blacksStart".into(),
            blacks_w: "blacksWidth".into(),
            shadows_r: "shadowsR".into(),
            shadows_g: "shadowsG".into(),
            shadows_b: "shadowsB".into(),
            shadows_m: "shadowsM".into(),
            shadows_s: "shadowsStart".into(),
            shadows_w: "shadowsWidth".into(),
            midtones_r: "midtonesR".into(),
            midtones_g: "midtonesG".into(),
            midtones_b: "midtonesB".into(),
            midtones_m: "midtonesM".into(),
            midtones_s: "midtonesStart".into(),
            midtones_w: "midtonesWidth".into(),
            highlights_r: "highlightsR".into(),
            highlights_g: "highlightsG".into(),
            highlights_b: "highlightsB".into(),
            highlights_m: "highlightsM".into(),
            highlights_s: "highlightsStart".into(),
            highlights_w: "highlightsWidth".into(),
            whites_r: "whitesR".into(),
            whites_g: "whitesG".into(),
            whites_b: "whitesB".into(),
            whites_m: "whitesM".into(),
            whites_s: "whitesStart".into(),
            whites_w: "whitesWidth".into(),
            s_contrast: "sContrast".into(),
            local_bypass: "localBypass".into(),
        }
    }
}

impl GtProperties {
    fn all_mut(&mut self) -> [&mut String; 32] {
        [
            &mut self.blacks_r,
            &mut self.blacks_g,
            &mut self.blacks_b,
            &mut self.blacks_m,
            &mut self.blacks_s,
            &mut self.blacks_w,
            &mut self.shadows_r,
            &mut self.shadows_g,
            &mut self.shadows_b,
            &mut self.shadows_m,
            &mut self.shadows_s,
            &mut self.shadows_w,
            &mut self.midtones_r,
            &mut self.midtones_g,
            &mut self.midtones_b,
            &mut self.midtones_m,
            &mut self.midtones_s,
            &mut self.midtones_w,
            &mut self.highlights_r,
            &mut self.highlights_g,
            &mut self.highlights_b,
            &mut self.highlights_m,
            &mut self.highlights_s,
            &mut self.highlights_w,
            &mut self.whites_r,
            &mut self.whites_g,
            &mut self.whites_b,
            &mut self.whites_m,
            &mut self.whites_s,
            &mut self.whites_w,
            &mut self.s_contrast,
            &mut self.local_bypass,
        ]
    }
}

const OP_PREFIX: &str = "grading_tone";

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

/// Getter of a value of the dynamic property.
fn value_getter(
    prop: &SharedValue<GradingTone>,
    f: impl Fn(&GradingTone) -> f64 + Send + Sync + 'static,
) -> impl Fn() -> f64 + Send + Sync + 'static {
    let prop = prop.clone();
    move || prop.with(|v| f(v))
}

/// Getter of a precomputed value of the dynamic property.
fn comp_getter<T>(
    prop: &SharedValue<GradingTone>,
    style: GradingStyle,
    f: impl Fn(&GradingTonePreRender) -> T + Send + Sync + 'static,
) -> impl Fn() -> T + Send + Sync + 'static {
    let prop = prop.clone();
    move || f(&GradingTonePreRender::from_value(style, &prop.get()))
}

fn add_gt_properties(
    shader_creator: &mut dyn GpuShaderCreator,
    st: &mut GpuShaderText,
    op: &GradingToneOp,
    p: &mut GtProperties,
    dyn_: bool,
) -> Result<()> {
    let style = op.style();
    if dyn_ {
        // Build names. No need to add an index to the name to avoid
        // collisions as the dynamic properties are unique.
        for n in p.all_mut() {
            *n = build_resource_name(shader_creator, OP_PREFIX, n);
        }

        // Property is decoupled and added to shader creator.
        let prop = SharedValue::new(op.value());
        shader_creator.add_dynamic_property(DynamicProperty::GradingTone(prop.clone()))?;

        // Add uniforms if they are not already there.
        let sc: &mut dyn GpuShaderCreator = shader_creator;
        add_uniform_double(sc, value_getter(&prop, |v| v.blacks.red), &p.blacks_r)?;
        add_uniform_double(sc, value_getter(&prop, |v| v.blacks.green), &p.blacks_g)?;
        add_uniform_double(sc, value_getter(&prop, |v| v.blacks.blue), &p.blacks_b)?;
        add_uniform_double(sc, value_getter(&prop, |v| v.blacks.master), &p.blacks_m)?;
        add_uniform_double(
            sc,
            comp_getter(&prop, style, |c| c.blacks_start),
            &p.blacks_s,
        )?;
        add_uniform_double(
            sc,
            comp_getter(&prop, style, |c| c.blacks_width),
            &p.blacks_w,
        )?;

        add_uniform_double(sc, value_getter(&prop, |v| v.shadows.red), &p.shadows_r)?;
        add_uniform_double(sc, value_getter(&prop, |v| v.shadows.green), &p.shadows_g)?;
        add_uniform_double(sc, value_getter(&prop, |v| v.shadows.blue), &p.shadows_b)?;
        add_uniform_double(sc, value_getter(&prop, |v| v.shadows.master), &p.shadows_m)?;
        add_uniform_double(
            sc,
            comp_getter(&prop, style, |c| c.shadows_start),
            &p.shadows_s,
        )?;
        add_uniform_double(
            sc,
            comp_getter(&prop, style, |c| c.shadows_width),
            &p.shadows_w,
        )?;

        add_uniform_double(sc, value_getter(&prop, |v| v.midtones.red), &p.midtones_r)?;
        add_uniform_double(sc, value_getter(&prop, |v| v.midtones.green), &p.midtones_g)?;
        add_uniform_double(sc, value_getter(&prop, |v| v.midtones.blue), &p.midtones_b)?;
        add_uniform_double(
            sc,
            value_getter(&prop, |v| v.midtones.master),
            &p.midtones_m,
        )?;
        add_uniform_double(sc, value_getter(&prop, |v| v.midtones.start), &p.midtones_s)?;
        add_uniform_double(sc, value_getter(&prop, |v| v.midtones.width), &p.midtones_w)?;

        add_uniform_double(
            sc,
            value_getter(&prop, |v| v.highlights.red),
            &p.highlights_r,
        )?;
        add_uniform_double(
            sc,
            value_getter(&prop, |v| v.highlights.green),
            &p.highlights_g,
        )?;
        add_uniform_double(
            sc,
            value_getter(&prop, |v| v.highlights.blue),
            &p.highlights_b,
        )?;
        add_uniform_double(
            sc,
            value_getter(&prop, |v| v.highlights.master),
            &p.highlights_m,
        )?;
        add_uniform_double(
            sc,
            comp_getter(&prop, style, |c| c.highlights_start),
            &p.highlights_s,
        )?;
        add_uniform_double(
            sc,
            comp_getter(&prop, style, |c| c.highlights_width),
            &p.highlights_w,
        )?;

        add_uniform_double(sc, value_getter(&prop, |v| v.whites.red), &p.whites_r)?;
        add_uniform_double(sc, value_getter(&prop, |v| v.whites.green), &p.whites_g)?;
        add_uniform_double(sc, value_getter(&prop, |v| v.whites.blue), &p.whites_b)?;
        add_uniform_double(sc, value_getter(&prop, |v| v.whites.master), &p.whites_m)?;
        add_uniform_double(
            sc,
            comp_getter(&prop, style, |c| c.whites_start),
            &p.whites_s,
        )?;
        add_uniform_double(
            sc,
            comp_getter(&prop, style, |c| c.whites_width),
            &p.whites_w,
        )?;

        add_uniform_double(sc, value_getter(&prop, |v| v.s_contrast), &p.s_contrast)?;

        add_uniform_bool(
            sc,
            comp_getter(&prop, style, |c| c.local_bypass),
            &p.local_bypass,
        )?;
    } else {
        let value = op.value();
        let comp = GradingTonePreRender::from_value(style, &value);

        let decls: [(&str, f64); 31] = [
            (&p.blacks_r, value.blacks.red),
            (&p.blacks_g, value.blacks.green),
            (&p.blacks_b, value.blacks.blue),
            (&p.blacks_m, value.blacks.master),
            (&p.blacks_s, comp.blacks_start),
            (&p.blacks_w, comp.blacks_width),
            (&p.shadows_r, value.shadows.red),
            (&p.shadows_g, value.shadows.green),
            (&p.shadows_b, value.shadows.blue),
            (&p.shadows_m, value.shadows.master),
            (&p.shadows_s, comp.shadows_start),
            (&p.shadows_w, comp.shadows_width),
            (&p.midtones_r, value.midtones.red),
            (&p.midtones_g, value.midtones.green),
            (&p.midtones_b, value.midtones.blue),
            (&p.midtones_m, value.midtones.master),
            (&p.midtones_s, value.midtones.start),
            (&p.midtones_w, value.midtones.width),
            (&p.highlights_r, value.highlights.red),
            (&p.highlights_g, value.highlights.green),
            (&p.highlights_b, value.highlights.blue),
            (&p.highlights_m, value.highlights.master),
            (&p.highlights_s, comp.highlights_start),
            (&p.highlights_w, comp.highlights_width),
            (&p.whites_r, value.whites.red),
            (&p.whites_g, value.whites.green),
            (&p.whites_b, value.whites.blue),
            (&p.whites_m, value.whites.master),
            (&p.whites_s, comp.whites_start),
            (&p.whites_w, comp.whites_width),
            (&p.s_contrast, value.s_contrast),
        ];
        for (name, v) in decls {
            st.declare_var_const(name, v as f32)?;
        }
    }
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Channel {
    R,
    G,
    B,
    M,
}

use Channel::{B, G, M, R};

fn channel_suffix(channel: Channel) -> &'static str {
    match channel {
        R => "rgb.r",
        G => "rgb.g",
        B => "rgb.b",
        M => "rgb",
    }
}

/// `std::to_string(float)`, i.e. `%f`.
fn to_string_f(v: f32) -> String {
    format!("{:.6}", f64::from(v))
}

fn add_mids_pre_shader(
    channel: Channel,
    st: &mut GpuShaderText,
    p: &GtProperties,
    style: GradingStyle,
) -> Result<()> {
    // TODO (OCIO): Everything in here should move to C++ (doesn't vary per
    // pixel).
    let channel_value = match channel {
        R => &p.midtones_r,
        G => &p.midtones_g,
        B => &p.midtones_b,
        M => &p.midtones_m,
    };

    nl!(st, "{"); // establish scope so local variable names won't conflict
    st.indent();

    let (top, _top_sc, bottom, _pivot) = GradingTonePreRender::from_style(style);

    let top_point = to_string_f(top);
    let bottom_point = to_string_f(bottom);

    nl!(st, st.float_keyword_const(), " halo = 0.4;");
    nl!(
        st,
        st.float_decl("mid_adj")?,
        " = clamp(",
        channel_value,
        ", 0.01, 1.99);"
    );

    nl!(st, "if (mid_adj != 1.)");
    nl!(st, "{");
    st.indent();

    nl!(st, st.float_keyword_const(), " x0 = ", bottom_point, ";");
    nl!(st, st.float_keyword_const(), " x5 = ", top_point, ";");

    nl!(
        st,
        st.float_keyword_const(),
        " max_width = (x5 - x0) * 0.95;"
    );
    nl!(
        st,
        st.float_decl("width")?,
        " = clamp(",
        p.midtones_w,
        ", 0.01, max_width);"
    );
    nl!(st, st.float_decl("min_cent")?, " = x0 + width * 0.51;");
    nl!(st, st.float_decl("max_cent")?, " = x5 - width * 0.51;");
    nl!(
        st,
        st.float_decl("center")?,
        " = clamp(",
        p.midtones_s,
        ", min_cent, max_cent);"
    );

    nl!(st, st.float_decl("x1")?, " = center - width * 0.5;");
    nl!(st, st.float_decl("x4")?, " = x1 + width;");

    nl!(st, st.float_decl("x2")?, " = x1 + (x4 - x1) * 0.25;");
    nl!(st, st.float_decl("x3")?, " = x1 + (x4 - x1) * 0.75;");
    nl!(st, st.float_decl("y0")?, " = x0;");
    nl!(st, st.float_keyword_const(), " m0 = 1.;");
    nl!(st, st.float_keyword_const(), " m5 = 1.;");

    nl!(st, st.float_keyword_const(), " min_slope = 0.1;");

    nl!(st, "mid_adj = mid_adj - 1.;");
    nl!(st, "mid_adj = mid_adj * (1. - min_slope);");

    nl!(st, st.float_decl("m2")?, " = 1. + mid_adj;");
    nl!(st, st.float_decl("m3")?, " = 1. - mid_adj;");
    nl!(st, st.float_decl("m1")?, " = 1. + mid_adj * halo;");
    nl!(st, st.float_decl("m4")?, " = 1. - mid_adj * halo;");

    nl!(st, "if (center <= (x5 + x0) * 0.5)");
    nl!(st, "{");
    st.indent();

    nl!(
        st,
        st.float_decl("area")?,
        " = (x1 - x0) * (m1 - m0) * 0.5 + "
    );
    nl!(
        st,
        "    (x2 - x1) * ((m1 - m0) + (m2 - m1)*0.5) + (center - x2) * (m2 - m0) * 0.5;"
    );
    nl!(
        st,
        "m4 = ( -0.5*(x5 - x4)*m5 + (x4 - x3) * (0.5*m3 - m5) + "
    );
    nl!(
        st,
        "    (x3 - center) * (m3 - m5) * 0.5 + area ) / ( -0.5*(x5 - x3) );"
    );

    st.dedent();
    nl!(st, "}");
    nl!(st, "else");
    nl!(st, "{");
    st.indent();

    nl!(
        st,
        st.float_decl("area")?,
        " = (x5 - x4) * (m4 - m5) * 0.5 + "
    );
    nl!(
        st,
        "    (x4 - x3) * ((m4 - m5) + (m3 - m4) * 0.5) + (x3 - center) * (m3 - m5) * 0.5;"
    );
    nl!(
        st,
        "m1 = ( -0.5*(x1 - x0)*m0 + (x2 - x1) * (0.5*m2 - m0) + "
    );
    nl!(
        st,
        "    (center - x2) * (m2 - m0) * 0.5 + area ) / ( -0.5*(x2 - x0) );"
    );

    st.dedent();
    nl!(st, "}");

    nl!(
        st,
        st.float_decl("y1")?,
        " = y0 + (m0 + m1) * (x1 - x0) * 0.5;"
    );
    nl!(
        st,
        st.float_decl("y2")?,
        " = y1 + (m1 + m2) * (x2 - x1) * 0.5;"
    );
    nl!(
        st,
        st.float_decl("y3")?,
        " = y2 + (m2 + m3) * (x3 - x2) * 0.5;"
    );
    nl!(
        st,
        st.float_decl("y4")?,
        " = y3 + (m3 + m4) * (x4 - x3) * 0.5;"
    );
    nl!(
        st,
        st.float_decl("y5")?,
        " = y4 + (m4 + m5) * (x5 - x4) * 0.5;"
    );
    Ok(())
}

/// The names of the six segments of the midtones curve.
const MID_SEGMENTS: [(&str, &str); 6] = [
    ("L", "tL"),
    ("M", "tM"),
    ("R", "tR"),
    ("R2", "tR2"),
    ("R3", "tR3"),
    ("", ""),
];

fn add_mids_fwd_shader(
    pix: &str,
    st: &mut GpuShaderText,
    channel: Channel,
    p: &GtProperties,
    style: GradingStyle,
) -> Result<()> {
    add_mids_pre_shader(channel, st, p, style)?;
    let suffix = channel_suffix(channel);

    if channel != M {
        nl!(st, st.float_decl("t")?, " = ", pix, ".", suffix, ";");
        for k in 0..5 {
            let (_, t) = MID_SEGMENTS[k];
            nl!(
                st,
                st.float_decl(t)?,
                " = (t - x",
                k,
                ") / (x",
                k + 1,
                " - x",
                k,
                ");"
            );
        }
        for k in 0..5 {
            let (s, t) = MID_SEGMENTS[k];
            nl!(
                st,
                st.float_decl(&format!("f{s}"))?,
                " = ",
                t,
                " * (x",
                k + 1,
                " - x",
                k,
                ") * ( ",
                t,
                " * 0.5 * (m",
                k + 1,
                " - m",
                k,
                ") + m",
                k,
                " ) + y",
                k,
                ";"
            );
        }

        nl!(st, st.float_decl("res")?, " = (t < x1) ? fL : fM;");
        nl!(st, "if (t > x2) res = fR;");
        nl!(st, "if (t > x3) res = fR2;");
        nl!(st, "if (t > x4) res = fR3;");
        nl!(st, "if (t < x0) res = y0 + (t - x0) * m0;");
        nl!(st, "if (t > x5) res = y5 + (t - x5) * m5;");
        nl!(st, pix, ".", suffix, " = res;");
    } else {
        nl!(st, st.color_decl("t")?, " = ", pix, ".rgb;");
        nl!(st, st.color_decl("res")?, ";");
        for k in 0..5 {
            let (_, t) = MID_SEGMENTS[k];
            nl!(
                st,
                st.color_decl(t)?,
                " = (t - x",
                k,
                ") / (x",
                k + 1,
                " - x",
                k,
                ");"
            );
        }
        for k in 0..5 {
            let (s, t) = MID_SEGMENTS[k];
            nl!(
                st,
                st.color_decl(&format!("f{s}"))?,
                " = ",
                t,
                " * (x",
                k + 1,
                " - x",
                k,
                ") * ( ",
                t,
                " * 0.5 * (m",
                k + 1,
                " - m",
                k,
                ") + m",
                k,
                " ) + y",
                k,
                ";"
            );
        }

        for c in ["r", "g", "b"] {
            nl!(
                st,
                "res.",
                c,
                " = (t.",
                c,
                " < x1) ? fL.",
                c,
                " : fM.",
                c,
                ";"
            );
        }
        for (x, f) in [("x2", "fR"), ("x3", "fR2"), ("x4", "fR3")] {
            for c in ["r", "g", "b"] {
                nl!(st, "res.", c, " = (t.", c, " > ", x, ") ? ", f, ".", c, " : res.", c, ";");
            }
        }
        for c in ["r", "g", "b"] {
            nl!(
                st,
                "res.",
                c,
                " = (t.",
                c,
                " < x0) ? y0 + (t.",
                c,
                " - x0) * m0 : res.",
                c,
                ";"
            );
        }
        for c in ["r", "g", "b"] {
            nl!(
                st,
                "res.",
                c,
                " = (t.",
                c,
                " > x5) ? y5 + (t.",
                c,
                " - x5) * m5 : res.",
                c,
                ";"
            );
        }
        nl!(st, pix, ".rgb = res;");
    }

    st.dedent();
    nl!(st, "}"); // if (mid_adj != 1.)

    st.dedent();
    nl!(st, "}"); // local scope
    Ok(())
}

fn add_mids_rev_shader(
    pix: &str,
    st: &mut GpuShaderText,
    channel: Channel,
    p: &GtProperties,
    style: GradingStyle,
) -> Result<()> {
    add_mids_pre_shader(channel, st, p, style)?;
    let suffix = channel_suffix(channel);

    if channel != M {
        nl!(st, st.float_keyword(), " t = ", pix, ".", suffix, ";");
        nl!(st, st.float_keyword(), " res;");

        nl!(st, "if (t >= y5)");
        nl!(st, "{");
        st.indent();
        nl!(st, "res = x5 + (t - y5) / m5;");
        st.dedent();
        nl!(st, "}");
        for k in (0..5).rev() {
            nl!(st, "else if (t >= y", k, ")");
            nl!(st, "{");
            st.indent();
            nl!(st, st.float_keyword(), " c = y", k, " - t;");
            nl!(
                st,
                st.float_keyword(),
                " b = m",
                k,
                " * (x",
                k + 1,
                " - x",
                k,
                ");"
            );
            nl!(
                st,
                st.float_keyword(),
                " a = 0.5 * (m",
                k + 1,
                " - m",
                k,
                ") * (x",
                k + 1,
                " - x",
                k,
                ");"
            );
            nl!(
                st,
                st.float_keyword(),
                " discrim = sqrt(b * b - 4. * a * c);"
            );
            nl!(st, st.float_keyword(), " tmp = (-2. * c) / (discrim + b);");
            nl!(st, "res =  tmp * (x", k + 1, " - x", k, ") + x", k, ";");
            st.dedent();
            nl!(st, "}");
        }
        nl!(st, "else");
        nl!(st, "{");
        st.indent();
        nl!(st, "res = x0 + (t - y0) / m0;");
        st.dedent();
        nl!(st, "}");

        nl!(st, pix, ".", suffix, " = res;");
    } else {
        let outs = ["outL", "outM", "outR", "outR2", "outR3"];

        nl!(st, st.color_decl("t")?, " = ", pix, ".rgb;");
        for o in outs {
            nl!(st, st.color_decl(o)?, ";");
        }

        // TODO (OCIO): Would probably be better to call the preceding
        // if-block 3 times rather than trying to do a float3 computation
        // here. Extra computation is done and it still doesn't avoid the
        // if/else.
        for k in (0..5).rev() {
            nl!(st, "{");
            st.indent();
            nl!(st, st.float3_decl("c")?, " = y", k, " - t;");
            nl!(
                st,
                st.float_decl("b")?,
                " = m",
                k,
                " * (x",
                k + 1,
                " - x",
                k,
                ");"
            );
            nl!(
                st,
                st.float_decl("a")?,
                " = 0.5 * (m",
                k + 1,
                " - m",
                k,
                ") * (x",
                k + 1,
                " - x",
                k,
                ");"
            );
            nl!(
                st,
                st.float3_decl("discrim")?,
                " = sqrt(b * b - 4. * a * c);"
            );
            nl!(st, st.float3_decl("tmp")?, " = (-2. * c) / (discrim + b);");
            nl!(
                st,
                outs[k],
                " =  tmp * (x",
                k + 1,
                " - x",
                k,
                ") + x",
                k,
                ";"
            );
            st.dedent();
            nl!(st, "}");
        }

        nl!(st, st.color_decl("res")?, ";");
        for c in ["r", "g", "b"] {
            nl!(
                st,
                "res.",
                c,
                " = (t.",
                c,
                " < y1) ? outL.",
                c,
                " : outM.",
                c,
                ";"
            );
        }
        for (y, o) in [("y2", "outR"), ("y3", "outR2"), ("y4", "outR3")] {
            for c in ["r", "g", "b"] {
                nl!(st, "res.", c, " = (t.", c, " > ", y, ") ? ", o, ".", c, " : res.", c, ";");
            }
        }
        for c in ["r", "g", "b"] {
            nl!(
                st,
                "res.",
                c,
                " = (t.",
                c,
                " < y0) ? x0 + (t.",
                c,
                " - y0) * m0 : res.",
                c,
                ";"
            );
        }
        for c in ["r", "g", "b"] {
            nl!(
                st,
                "res.",
                c,
                " = (t.",
                c,
                " > y5) ? x5 + (t.",
                c,
                " - y5) * m5 : res.",
                c,
                ";"
            );
        }
        nl!(st, pix, ".rgb = res;");
    }

    st.dedent();
    nl!(st, "}"); // if (mid_adj != 1.)

    st.dedent();
    nl!(st, "}"); // local scope
    Ok(())
}

fn add_highlight_shadow_pre_shader(
    st: &mut GpuShaderText,
    channel: Channel,
    p: &GtProperties,
    is_shadow: bool,
) -> Result<()> {
    // TODO (OCIO): Everything in here should move to C++ (doesn't vary per
    // pixel).
    let start = if is_shadow {
        &p.shadows_s
    } else {
        &p.highlights_s
    };
    let pivot = if is_shadow {
        &p.shadows_w
    } else {
        &p.highlights_w
    };

    let channel_value = match (channel, is_shadow) {
        (R, true) => &p.shadows_r,
        (R, false) => &p.highlights_r,
        (G, true) => &p.shadows_g,
        (G, false) => &p.highlights_g,
        (B, true) => &p.shadows_b,
        (B, false) => &p.highlights_b,
        (M, true) => &p.shadows_m,
        (M, false) => &p.highlights_m,
    };

    nl!(st, "{"); // establish scope so local variable names won't conflict
    st.indent();
    if is_shadow {
        nl!(st, st.float_decl("x0")?, " = ", pivot, ";");
        nl!(st, st.float_decl("x2")?, " = ", start, ";");
        st.declare_var("m2", 1.0f32)?;
    } else {
        nl!(st, st.float_decl("x0")?, " = ", start, ";");
        nl!(st, st.float_decl("x2")?, " = ", pivot, ";");
        st.declare_var("m0", 1.0f32)?;
    }
    nl!(st, st.float_decl("y0")?, " = x0;");
    nl!(st, st.float_decl("y2")?, " = x2;");
    nl!(st, st.float_decl("x1")?, " = x0 + (x2 - x0) * 0.5;");

    nl!(st, st.float_decl("val")?, " = ", channel_value, ";");
    if !is_shadow {
        nl!(st, "val = 2. - val;");
    }
    Ok(())
}

fn add_faux_cubic_fwd_eval_shader(
    pix: &str,
    st: &mut GpuShaderText,
    channel: Channel,
) -> Result<()> {
    let suffix = channel_suffix(channel);

    nl!(
        st,
        st.float_keyword(),
        " y1 = ( 0.5 / (x2 - x0) ) * ( (2.*y0 + m0 * (x1 - x0)) * (x2 - x1) + (2.*y2 - m2 * (x2 - x1)) * (x1 - x0) );"
    );

    if channel != M {
        nl!(st, st.float_keyword(), " t = ", pix, ".", suffix, ";");
        nl!(st, st.float_keyword(), " res, tL, tR, fL, fR;");
    } else {
        nl!(st, st.color_decl("t")?, " = ", pix, ".", suffix, ";");
        for n in ["res", "tL", "tR", "fL", "fR"] {
            nl!(st, st.color_decl(n)?, ";");
        }
    }

    nl!(st, "tL = (t - x0) / (x1 - x0);");
    nl!(st, "tR = (t - x1) / (x2 - x1);");
    nl!(
        st,
        "fL = y0 * (1. - tL*tL) + y1 * tL*tL + m0 * (1. - tL) * tL * (x1 - x0);"
    );
    nl!(
        st,
        "fR = y1 * (1. - tR)*(1. - tR) + y2 * (2. - tR)*tR + m2 * (tR - 1.)*tR * (x2 - x1);"
    );

    if channel != M {
        nl!(st, "res = (t < x1) ? fL : fR;");
        nl!(st, "res = (t < x0) ? y0 + (t - x0) * m0 : res;");
        nl!(st, "res = (t > x2) ? y2 + (t - x2) * m2 : res;");
    } else {
        for c in ["r", "g", "b"] {
            nl!(
                st,
                "res.",
                c,
                " = (t.",
                c,
                " < x1) ? fL.",
                c,
                " : fR.",
                c,
                ";"
            );
        }
        for c in ["r", "g", "b"] {
            nl!(
                st,
                "res.",
                c,
                " = (t.",
                c,
                " < x0) ? y0 + (t.",
                c,
                " - x0) * m0 : res.",
                c,
                ";"
            );
        }
        for c in ["r", "g", "b"] {
            nl!(
                st,
                "res.",
                c,
                " = (t.",
                c,
                " > x2) ? y2 + (t.",
                c,
                " - x2) * m2 : res.",
                c,
                ";"
            );
        }
    }
    nl!(st, pix, ".", suffix, " = res;");
    Ok(())
}

fn add_faux_cubic_rev_eval_shader(
    pix: &str,
    st: &mut GpuShaderText,
    channel: Channel,
) -> Result<()> {
    let suffix = channel_suffix(channel);

    nl!(
        st,
        st.float_keyword(),
        " y1 = ( 0.5 / (x2 - x0) ) * ( (2.*y0 + m0 * (x1 - x0)) * (x2 - x1) + (2.*y2 - m2 * (x2 - x1)) * (x1 - x0) );"
    );

    if channel != M {
        nl!(st, st.float_keyword(), " t = ", pix, ".", suffix, ";");
        nl!(
            st,
            st.float_keyword(),
            " res, cL, cR, discrimL, discrimR, outL, outR;"
        );
    } else {
        nl!(st, st.color_decl("t")?, " = ", pix, ".", suffix, ";");
        for n in ["res", "cL", "cR", "discrimL", "discrimR", "outL", "outR"] {
            nl!(st, st.color_decl(n)?, ";");
        }
    }

    nl!(st, "cL = y0 - t;");
    nl!(st, st.float_keyword(), " bL = m0 * (x1 - x0);");
    nl!(st, st.float_keyword(), " aL = y1 - y0 - m0 * (x1 - x0);");
    nl!(st, "discrimL = sqrt( bL * bL - 4. * aL * cL );");
    nl!(
        st,
        "outL = (-2. * cL) / ( discrimL + bL ) * (x1 - x0) + x0;"
    );
    nl!(st, "cR = y1 - t;");
    nl!(
        st,
        st.float_keyword(),
        " bR = 2.*y2 - 2.*y1 - m2 * (x2 - x1);"
    );
    nl!(st, st.float_keyword(), " aR = y1 - y2 + m2 * (x2 - x1);");
    nl!(st, "discrimR = sqrt( bR * bR - 4. * aR * cR );");
    nl!(
        st,
        "outR = (-2. * cR) / ( discrimR + bR ) * (x2 - x1) + x1;"
    );
    if channel != M {
        nl!(st, "res = (t < y1) ? outL : outR;");
        nl!(st, "res = (t < y0) ? x0 + (t - y0) / m0 : res;");
        nl!(st, "res = (t > y2) ? x2 + (t - y2) / m2 : res;");
    } else {
        for c in ["r", "g", "b"] {
            nl!(
                st,
                "res.",
                c,
                " = (t.",
                c,
                " < y1) ? outL.",
                c,
                " : outR.",
                c,
                ";"
            );
        }
        for c in ["r", "g", "b"] {
            nl!(
                st,
                "res.",
                c,
                " = (t.",
                c,
                " < y0) ? x0 + (t.",
                c,
                " - y0) / m0 : res.",
                c,
                ";"
            );
        }
        for c in ["r", "g", "b"] {
            nl!(
                st,
                "res.",
                c,
                " = (t.",
                c,
                " > y2) ? x2 + (t.",
                c,
                " - y2) / m2 : res.",
                c,
                ";"
            );
        }
    }
    nl!(st, pix, ".", suffix, " = res;");
    Ok(())
}

fn add_highlight_shadow_shader(
    pix: &str,
    st: &mut GpuShaderText,
    channel: Channel,
    is_shadow: bool,
    p: &GtProperties,
    forward: bool,
) -> Result<()> {
    add_highlight_shadow_pre_shader(st, channel, p, is_shadow)?;

    nl!(st, "if (val < 1.)");
    nl!(st, "{");
    st.indent();

    if is_shadow {
        nl!(st, st.float_keyword(), " m0 = max( 0.01, val );");
    } else {
        nl!(st, st.float_keyword(), " m2 = max( 0.01, val );");
    }
    if forward {
        add_faux_cubic_fwd_eval_shader(pix, st, channel)?;
    } else {
        add_faux_cubic_rev_eval_shader(pix, st, channel)?;
    }

    st.dedent();
    nl!(st, "}");

    nl!(st, "else if (val > 1.)");
    nl!(st, "{");
    st.indent();

    if is_shadow {
        nl!(st, st.float_keyword(), " m0 = max( 0.01, 2. - val );");
    } else {
        nl!(st, st.float_keyword(), " m2 = max( 0.01, 2. - val );");
    }
    if forward {
        add_faux_cubic_rev_eval_shader(pix, st, channel)?;
    } else {
        add_faux_cubic_fwd_eval_shader(pix, st, channel)?;
    }

    st.dedent();
    nl!(st, "}");

    st.dedent();
    nl!(st, "}"); // establish scope
    Ok(())
}

fn add_white_black_pre_shader(
    pix: &str,
    st: &mut GpuShaderText,
    channel: Channel,
    is_black: bool,
    p: &GtProperties,
) -> Result<()> {
    let start = if is_black { &p.blacks_s } else { &p.whites_s };
    let width = if is_black { &p.blacks_w } else { &p.whites_w };
    let channel_value = match (channel, is_black) {
        (R, true) => &p.blacks_r,
        (R, false) => &p.whites_r,
        (G, true) => &p.blacks_g,
        (G, false) => &p.whites_g,
        (B, true) => &p.blacks_b,
        (B, false) => &p.whites_b,
        (M, true) => &p.blacks_m,
        (M, false) => &p.whites_m,
    };
    let suffix = channel_suffix(channel);

    nl!(st, "{"); // establish scope so local variable names won't conflict
    st.indent();
    if !is_black {
        nl!(st, st.float_keyword(), " x0 = ", start, ";");
        nl!(st, st.float_keyword(), " x1 = x0 + ", width, ";");
        nl!(st, st.float_keyword_const(), " m0 = 1.;");
        nl!(st, st.float_keyword(), " y0 = x0;");
        nl!(st, st.float_keyword(), " m1 = ", channel_value, ";");
        nl!(st, st.float_keyword(), " mtest = m1;");
    } else {
        nl!(st, st.float_keyword(), " x1 = ", start, ";");
        nl!(st, st.float_keyword(), " x0 = x1 - ", width, ";");
        nl!(st, st.float_keyword_const(), " m1 = 1.;");
        nl!(st, st.float_keyword(), " y1 = x1;");
        nl!(st, st.float_keyword(), " m0 = ", channel_value, ";");
        nl!(st, "m0 = 2. - m0;"); // increasing blacks control should lighten
        nl!(st, st.float_keyword(), " mtest = m0;");
    }

    if channel != M {
        nl!(st, st.float_keyword(), " t = ", pix, ".", suffix, ";");
    } else {
        nl!(st, st.color_decl("t")?, " = ", pix, ".rgb;");
    }
    Ok(())
}

fn add_wb_fwd_shader(channel: Channel, linear_extrap: bool, st: &mut GpuShaderText) -> Result<()> {
    if channel != M {
        nl!(st, st.float_keyword(), " tlocal = (t - x0) / (x1 - x0);");
        nl!(
            st,
            st.float_keyword(),
            " res = tlocal * (x1 - x0) * ( tlocal * 0.5 * (m1 - m0) + m0 ) + y0;"
        );
        nl!(st, "res = (t < x0) ? y0 + (t - x0) * m0 : res;");
    } else {
        nl!(st, st.float3_decl("tlocal")?, " = (t - x0) / (x1 - x0);");
        nl!(
            st,
            st.color_decl("res")?,
            " = tlocal * (x1 - x0) * ( tlocal * 0.5 * (m1 - m0) + m0 ) + y0;"
        );
        for c in ["r", "g", "b"] {
            nl!(
                st,
                "res.",
                c,
                " = (t.",
                c,
                " < x0) ? y0 + (t.",
                c,
                " - x0) * m0 : res.",
                c,
                ";"
            );
        }
    }
    if linear_extrap {
        if channel != M {
            nl!(st, "res = (t > x1) ? y1 + (t - x1) * m1 : res;");
        } else {
            for c in ["r", "g", "b"] {
                nl!(
                    st,
                    "res.",
                    c,
                    " = (t.",
                    c,
                    " > x1) ? y1 + (t.",
                    c,
                    " - x1) * m1 : res.",
                    c,
                    ";"
                );
            }
        }
    }
    Ok(())
}

fn add_wb_rev_shader(channel: Channel, linear_extrap: bool, st: &mut GpuShaderText) -> Result<()> {
    nl!(st, st.float_keyword(), " a = 0.5 * (m1 - m0) * (x1 - x0);");
    nl!(st, st.float_keyword(), " b = m0 * (x1 - x0);");
    if channel != M {
        nl!(st, st.float_keyword(), " c = y0 - t;");
        nl!(
            st,
            st.float_keyword(),
            " discrim = sqrt( b * b - 4. * a * c );"
        );
        nl!(
            st,
            st.float_keyword(),
            " tmp = ( -2. * c ) / ( discrim + b );"
        );
        nl!(st, st.float_keyword(), " res = tmp * (x1 - x0) + x0;");
        nl!(st, "res = (t < y0) ? x0 + (t - y0) / m0 : res;");
    } else {
        nl!(st, st.float3_decl("c")?, " = y0 - t;");
        nl!(
            st,
            st.float3_decl("discrim")?,
            " = sqrt( b * b - 4. * a * c );"
        );
        nl!(
            st,
            st.float3_decl("tmp")?,
            " = ( -2. * c ) / ( discrim + b );"
        );
        nl!(st, st.color_decl("res")?, " = tmp * (x1 - x0) + x0;");
        for c in ["r", "g", "b"] {
            nl!(
                st,
                "res.",
                c,
                " = (t.",
                c,
                " < y0) ? x0 + (t.",
                c,
                " - y0) / m0 : res.",
                c,
                ";"
            );
        }
    }
    if linear_extrap {
        if channel != M {
            nl!(st, "res = (t > y1) ? x1 + (t - y1) / m1 : res;");
        } else {
            // TODO (OCIO): When m1 = 1., y1=x1, this becomes t.
            for c in ["r", "g", "b"] {
                nl!(
                    st,
                    "res.",
                    c,
                    " = (t.",
                    c,
                    " > y1) ? x1 + (t.",
                    c,
                    " - y1) / m1 : res.",
                    c,
                    ";"
                );
            }
        }
    }
    Ok(())
}

fn add_wb_extrap_pre_shader(st: &mut GpuShaderText) {
    nl!(st, "res = (res - x0) / gain + x0;");
    // Quadratic extrapolation for better HDR control.
    nl!(st, st.float_keyword(), " new_y1 = (x1 - x0) / gain + x0;");
    nl!(st, st.float_keyword(), " xd = x0 + (x1 - x0) * 0.99;");
    nl!(
        st,
        st.float_keyword(),
        " md = m0 + (xd - x0) * (m1 - m0) / (x1 - x0);"
    );
    nl!(st, "md = 1. / md;");
    nl!(
        st,
        st.float_keyword(),
        " aa = 0.5 * (1. / m1 - md) / (x1 - xd);"
    );
    nl!(st, st.float_keyword(), " bb = 1. / m1 - 2. * aa * x1;");
    nl!(
        st,
        st.float_keyword(),
        " cc = new_y1 - bb * x1 - aa * x1 * x1;"
    );
    nl!(st, "t = (t - x0) / gain + x0;");
}

fn write_wb_result(pix: &str, st: &mut GpuShaderText, channel: Channel) {
    if channel != M {
        nl!(st, pix, ".", channel_suffix(channel), " = res;");
    } else {
        nl!(st, pix, ".rgb = res;");
    }
}

/// The start of the decreasing slope case.
fn add_wb_decreasing_pre(st: &mut GpuShaderText, is_black: bool) {
    nl!(st, "if (mtest < 1.)");
    nl!(st, "{");
    st.indent();
    if !is_black {
        nl!(st, "m1 = max( 0.01, m1 );");
        nl!(
            st,
            st.float_keyword(),
            " y1 = y0 + (m0 + m1) * (x1 - x0) * 0.5;"
        );
    } else {
        nl!(st, "m0 = max( 0.01, m0 );");
        nl!(
            st,
            st.float_keyword(),
            " y0 = y1 - (m0 + m1) * (x1 - x0) * 0.5;"
        );
    }
}

/// The start of the increasing slope case.
fn add_wb_increasing_pre(st: &mut GpuShaderText, is_black: bool) {
    nl!(st, "else if (mtest > 1.)");
    nl!(st, "{");
    st.indent();
    if !is_black {
        nl!(st, "m1 = 2. - m1;");
        nl!(st, "m1 = max( 0.01, m1 );");
        nl!(st, st.float_keyword(), " gain = (m0 + m1) * 0.5;");
        nl!(st, "t = (t - x0) * gain + x0;");
    } else {
        nl!(st, "m0 = 2. - m0;");
        nl!(st, "m0 = max( 0.01, m0 );");
        nl!(
            st,
            st.float_keyword(),
            " y0 = y1 - (m0 + m1) * (x1 - x0) * 0.5;"
        );
        nl!(st, st.float_keyword(), " gain = (m0 + m1) * 0.5;");
        nl!(st, "t = (t - x1) * gain + x1;");
    }
}

fn add_white_black_fwd_shader(
    pix: &str,
    st: &mut GpuShaderText,
    channel: Channel,
    is_black: bool,
    p: &GtProperties,
) -> Result<()> {
    add_white_black_pre_shader(pix, st, channel, is_black, p)?;

    // Slope is decreasing case.
    add_wb_decreasing_pre(st, is_black);
    add_wb_fwd_shader(channel, true, st)?;
    write_wb_result(pix, st, channel);
    st.dedent();
    nl!(st, "}");

    // Slope is increasing case.
    add_wb_increasing_pre(st, is_black);
    add_wb_rev_shader(channel, is_black, st)?;

    if !is_black {
        add_wb_extrap_pre_shader(st);

        if channel != M {
            nl!(st, "if (t > x1) res = (aa * t  + bb) * t + cc;");
        } else {
            for c in ["r", "g", "b"] {
                nl!(
                    st,
                    "if (t.",
                    c,
                    " > x1) res.",
                    c,
                    " = (aa * t.",
                    c,
                    " + bb) * t.",
                    c,
                    " + cc;"
                );
            }
        }
    } else {
        nl!(st, "res = (res - x1) / gain + x1;");
    }

    write_wb_result(pix, st, channel);
    st.dedent();
    nl!(st, "}"); // else if (mtest > 1.)

    st.dedent();
    nl!(st, "}"); // establish scope so local variable names won't conflict
    Ok(())
}

fn add_white_black_rev_shader(
    pix: &str,
    st: &mut GpuShaderText,
    channel: Channel,
    is_black: bool,
    p: &GtProperties,
) -> Result<()> {
    add_white_black_pre_shader(pix, st, channel, is_black, p)?;

    // Slope is decreasing case.
    add_wb_decreasing_pre(st, is_black);
    add_wb_rev_shader(channel, true, st)?;
    write_wb_result(pix, st, channel);
    st.dedent();
    nl!(st, "}");

    // Slope is increasing case.
    add_wb_increasing_pre(st, is_black);
    add_wb_fwd_shader(channel, is_black, st)?;

    if !is_black {
        add_wb_extrap_pre_shader(st);

        if channel != M {
            nl!(st, st.float_keyword(), " c = cc - t;");
            nl!(
                st,
                st.float_keyword(),
                " discrim = sqrt( bb * bb - 4. * aa * c );"
            );
            nl!(
                st,
                st.float_keyword(),
                " res1 = ( -2. * c ) / ( discrim + bb );"
            );
            nl!(st, st.float_keyword(), " brk = (aa * x1 + bb) * x1 + cc;");
            nl!(st, "res = (t < brk) ? res : res1;");
        } else {
            nl!(st, st.float3_decl("c")?, " = cc - t;");
            nl!(
                st,
                st.float3_decl("discrim")?,
                " = sqrt( bb * bb - 4. * aa * c );"
            );
            nl!(
                st,
                st.color_decl("res1")?,
                " = ( -2. * c ) / ( discrim + bb );"
            );
            nl!(st, st.float_keyword(), " brk = (aa * x1 + bb) * x1 + cc;");
            for c in ["r", "g", "b"] {
                nl!(
                    st,
                    "res.",
                    c,
                    " = (t.",
                    c,
                    " < brk) ? res.",
                    c,
                    " : res1.",
                    c,
                    ";"
                );
            }
        }
    } else {
        nl!(st, "res = (res - x1) / gain + x1;");
    }

    write_wb_result(pix, st, channel);
    st.dedent();
    nl!(st, "}"); // else if (mtest > 1.)

    st.dedent();
    nl!(st, "}"); // establish scope so local variable names won't conflict
    Ok(())
}

fn add_s_contrast_top_pre_shader(
    pix: &str,
    st: &mut GpuShaderText,
    p: &GtProperties,
    style: GradingStyle,
) -> Result<()> {
    let (_top, top_sc, _bottom, pivot) = GradingTonePreRender::from_style(style);
    let top_point = to_string_f(top_sc);

    nl!(st, st.float_keyword(), " contrast = ", p.s_contrast, ";");
    nl!(st, "if (contrast != 1.)");
    nl!(st, "{");
    st.indent();

    // Limit the range of values to prevent reversals.
    nl!(
        st,
        "contrast = (contrast > 1.) ? 1. / (1.8125 - 0.8125 * min( contrast, 1.99 )) : 0.28125 + 0.71875 * max( contrast, 0.01 );"
    );
    nl!(
        st,
        st.float_keyword_const(),
        " pivot = ",
        to_string_f(pivot),
        ";"
    );

    nl!(st, st.color_decl("t")?, " = ", pix, ".rgb;");

    // Top end.
    nl!(st, "{"); // establish scope so local variable names won't conflict
    st.indent();
    nl!(st, st.float_keyword_const(), " x3 = ", top_point, ";");
    nl!(st, st.float_keyword_const(), " y3 = ", top_point, ";");
    nl!(
        st,
        st.float_keyword_const(),
        " y0 = pivot + (y3 - pivot) * 0.25;"
    );
    nl!(st, st.float_keyword(), " m0 = contrast;");
    nl!(st, st.float_keyword(), " x0 = pivot + (y0 - pivot) / m0;");
    nl!(st, st.float_keyword(), " min_width = (x3 - x0) * 0.3;");
    nl!(st, st.float_keyword(), " m3 = 1. / m0;");
    // NB: Due to the if (contrast != 1.) clause above, m0 != m3.
    nl!(
        st,
        st.float_keyword(),
        " center = (y3 - y0 - m3*x3 + m0*x0) / (m0 - m3);"
    );
    nl!(st, st.float_keyword(), " x1 = x0;");
    nl!(st, st.float_keyword(), " x2 = 2. * center - x1;");
    nl!(st, "if (x2 > x3)");
    nl!(st, "{");
    nl!(st, "  x2 = x3;");
    nl!(st, "  x1 = 2. * center - x2;");
    nl!(st, "}");
    nl!(st, "else if ((x2 - x1) < min_width)");
    nl!(st, "{");
    nl!(st, "  x2 = x1 + min_width;");
    nl!(st, "  float new_center = (x2 + x1) * 0.5;");
    nl!(
        st,
        "  m3 = (y3 - y0 + m0*x0 - new_center * m0) / (x3 - new_center);"
    );
    nl!(st, "}");
    nl!(st, st.float_keyword(), " y1 = y0;");
    nl!(
        st,
        st.float_keyword(),
        " y2 = y1 + (m0 + m3) * (x2 - x1) * 0.5;"
    );
    Ok(())
}

fn add_s_contrast_bottom_pre_shader(st: &mut GpuShaderText, style: GradingStyle) {
    let (_top, _top_sc, bottom, _pivot) = GradingTonePreRender::from_style(style);
    let bottom_point = to_string_f(bottom);

    // Bottom end.
    nl!(st, "{"); // establish scope so local variable names won't conflict
    st.indent();
    nl!(st, st.float_keyword_const(), " x0 = ", bottom_point, ";");
    nl!(st, st.float_keyword_const(), " y0 = ", bottom_point, ";");
    nl!(
        st,
        st.float_keyword_const(),
        " y3 = pivot - (pivot - y0) * 0.25;"
    );
    nl!(st, st.float_keyword(), " m3 = contrast;");
    nl!(st, st.float_keyword(), " x3 = pivot - (pivot - y3) / m3;");
    nl!(st, st.float_keyword(), " min_width = (x3 - x0) * 0.3;");
    nl!(st, st.float_keyword(), " m0 = 1. / m3;");
    nl!(
        st,
        st.float_keyword(),
        " center = (y3 - y0 - m3*x3 + m0*x0) / (m0 - m3);"
    );
    nl!(st, st.float_keyword(), " x2 = x3;");
    nl!(st, st.float_keyword(), " x1 = 2. * center - x2;");
    nl!(st, "if (x1 < x0)");
    nl!(st, "{");
    nl!(st, "  x1 = x0;");
    nl!(st, "  x2 = 2. * center - x1;");
    nl!(st, "}");
    nl!(st, "else if ((x2 - x1) < min_width)");
    nl!(st, "{");
    nl!(st, "  x1 = x2 - min_width;");
    nl!(st, "  float new_center = (x2 + x1) * 0.5;");
    nl!(
        st,
        "  m0 = (y3 - y0 - m3*x3 + new_center * m3) / (new_center - x0);"
    );
    nl!(st, "}");
    nl!(st, st.float_keyword(), " y2 = y3;");
    nl!(
        st,
        st.float_keyword(),
        " y1 = y2 - (m0 + m3) * (x2 - x1) * 0.5;"
    );
}

fn add_s_contrast_fwd_shader(
    pix: &str,
    st: &mut GpuShaderText,
    p: &GtProperties,
    style: GradingStyle,
) -> Result<()> {
    add_s_contrast_top_pre_shader(pix, st, p, style)?;

    nl!(st, pix, ".rgb = (t - pivot) * contrast + pivot;");

    nl!(st, st.float3_decl("tR")?, " = (t - x1) / (x2 - x1);");
    nl!(
        st,
        st.color_decl("res")?,
        " = tR * (x2 - x1) * ( tR * 0.5 * (m3 - m0) + m0 ) + y1;"
    );

    for c in ["r", "g", "b"] {
        nl!(
            st,
            pix,
            ".rgb.",
            c,
            " = (t.",
            c,
            " > x1) ? res.",
            c,
            " : ",
            pix,
            ".rgb.",
            c,
            ";"
        );
    }
    for c in ["r", "g", "b"] {
        nl!(
            st,
            pix,
            ".rgb.",
            c,
            " = (t.",
            c,
            " > x2) ? y2 + (t.",
            c,
            " - x2) * m3 : ",
            pix,
            ".rgb.",
            c,
            ";"
        );
    }
    st.dedent();
    nl!(st, "}"); // end local scope

    add_s_contrast_bottom_pre_shader(st, style);

    nl!(st, st.float3_decl("tR")?, " = (t - x1) / (x2 - x1);");
    nl!(
        st,
        st.color_decl("res")?,
        " = tR * (x2 - x1) * ( tR * 0.5 * (m3 - m0) + m0 ) + y1;"
    );

    for c in ["r", "g", "b"] {
        nl!(
            st,
            pix,
            ".rgb.",
            c,
            " = (t.",
            c,
            " < x2) ? res.",
            c,
            " : ",
            pix,
            ".rgb.",
            c,
            ";"
        );
    }
    for c in ["r", "g", "b"] {
        nl!(
            st,
            pix,
            ".rgb.",
            c,
            " = (t.",
            c,
            " < x1) ? y1 + (t.",
            c,
            " - x1) * m0 : ",
            pix,
            ".rgb.",
            c,
            ";"
        );
    }
    st.dedent();
    nl!(st, "}"); // end local scope

    st.dedent();
    nl!(st, "}"); // end if contrast != 1.
    Ok(())
}

fn add_s_contrast_rev_shader(
    pix: &str,
    st: &mut GpuShaderText,
    p: &GtProperties,
    style: GradingStyle,
) -> Result<()> {
    add_s_contrast_top_pre_shader(pix, st, p, style)?;

    nl!(st, pix, ".rgb = (t - pivot) / contrast + pivot;");

    nl!(st, st.float3_decl("c")?, " = y1 - t;");
    nl!(st, st.float_decl("b")?, " = m0 * (x2 - x1);");
    nl!(st, st.float_decl("a")?, " = (m3 - m0) * 0.5 * (x2 - x1);");
    nl!(
        st,
        st.float3_decl("discrim")?,
        " = sqrt( b * b - 4. * a * c );"
    );
    nl!(
        st,
        st.color_decl("res")?,
        " = (x2 - x1) * (-2. * c) / ( discrim + b ) + x1;"
    );

    for c in ["r", "g", "b"] {
        nl!(
            st,
            pix,
            ".rgb.",
            c,
            " = (t.",
            c,
            " > y1) ? res.",
            c,
            " : ",
            pix,
            ".rgb.",
            c,
            ";"
        );
    }
    for c in ["r", "g", "b"] {
        nl!(
            st,
            pix,
            ".rgb.",
            c,
            " = (t.",
            c,
            " > y2) ? x2 + (t.",
            c,
            " - y2) / m3 : ",
            pix,
            ".rgb.",
            c,
            ";"
        );
    }
    st.dedent();
    nl!(st, "}"); // end local scope

    add_s_contrast_bottom_pre_shader(st, style);

    nl!(st, st.float3_decl("c")?, " = y1 - t;");
    nl!(st, st.float_decl("b")?, " = m0 * (x2 - x1);");
    nl!(st, st.float_decl("a")?, " = (m3 - m0) * 0.5 * (x2 - x1);");
    nl!(
        st,
        st.float3_decl("discrim")?,
        " = sqrt( b * b - 4. * a * c );"
    );
    nl!(
        st,
        st.color_decl("res")?,
        " = (x2 - x1) * (-2. * c) / ( discrim + b ) + x1;"
    );

    for c in ["r", "g", "b"] {
        nl!(
            st,
            pix,
            ".rgb.",
            c,
            " = (t.",
            c,
            " > y2) ? ",
            pix,
            ".rgb.",
            c,
            " : res.",
            c,
            ";"
        );
    }
    for c in ["r", "g", "b"] {
        nl!(
            st,
            pix,
            ".rgb.",
            c,
            " = (t.",
            c,
            " > y1) ? ",
            pix,
            ".rgb.",
            c,
            " : x1 + (t.",
            c,
            " - y1) / m0;"
        );
    }
    st.dedent();
    nl!(st, "}"); // end local scope

    st.dedent();
    nl!(st, "}"); // end if contrast != 1.
    Ok(())
}

fn add_gt_forward_shader(
    shader_creator: &dyn GpuShaderCreator,
    st: &mut GpuShaderText,
    p: &GtProperties,
    style: GradingStyle,
) -> Result<()> {
    let pix = shader_creator.pixel_name();

    if style == GradingStyle::Lin {
        // NB: Although the linToLog and logToLin are correct inverses, the
        // limits of floating-point arithmetic cause errors in the lowest bit
        // of the round trip.
        add_lin_to_log_shader(shader_creator, st)?;
    }

    for c in [R, G, B, M] {
        add_mids_fwd_shader(pix, st, c, p, style)?;
    }
    for c in [R, G, B, M] {
        add_highlight_shadow_shader(pix, st, c, false, p, true)?;
    }
    for c in [R, G, B, M] {
        add_white_black_fwd_shader(pix, st, c, false, p)?;
    }
    for c in [R, G, B, M] {
        add_highlight_shadow_shader(pix, st, c, true, p, true)?;
    }
    for c in [R, G, B, M] {
        add_white_black_fwd_shader(pix, st, c, true, p)?;
    }

    add_s_contrast_fwd_shader(pix, st, p, style)?;

    if style == GradingStyle::Lin {
        add_log_to_lin_shader(shader_creator, st)?;
    }

    // Note (OCIO): The grading controls at high values are able to push
    // values above the max half-float at which point they overflow to
    // infinity.
    nl!(st, pix, " = min( ", pix, ", 65504. );");
    Ok(())
}

fn add_gt_inverse_shader(
    shader_creator: &dyn GpuShaderCreator,
    st: &mut GpuShaderText,
    p: &GtProperties,
    style: GradingStyle,
) -> Result<()> {
    let pix = shader_creator.pixel_name();

    if style == GradingStyle::Lin {
        // NB: Although the linToLog and logToLin are correct inverses, the
        // limits of floating-point arithmetic cause errors in the lowest bit
        // of the round trip.
        add_lin_to_log_shader(shader_creator, st)?;
    }

    add_s_contrast_rev_shader(pix, st, p, style)?;

    for c in [M, R, G, B] {
        add_white_black_rev_shader(pix, st, c, true, p)?;
    }
    for c in [M, R, G, B] {
        add_highlight_shadow_shader(pix, st, c, true, p, false)?;
    }
    for c in [M, R, G, B] {
        add_white_black_rev_shader(pix, st, c, false, p)?;
    }
    for c in [M, R, G, B] {
        add_highlight_shadow_shader(pix, st, c, false, p, false)?;
    }
    for c in [M, R, G, B] {
        add_mids_rev_shader(pix, st, c, p, style)?;
    }

    if style == GradingStyle::Lin {
        add_log_to_lin_shader(shader_creator, st)?;
    }

    // Note (OCIO): The grading controls at high values are able to push
    // values above the max half-float at which point they overflow to
    // infinity.
    nl!(st, pix, " = min( ", pix, ", 65504. );");
    Ok(())
}

/// Port of `GetGradingToneGPUShaderProgram`.
pub(crate) fn extract(op: &GradingToneOp, shader_creator: &mut dyn GpuShaderCreator) -> Result<()> {
    let lang = shader_creator.language();
    let is_dynamic = op.is_dynamic();
    let dyn_ = is_dynamic && lang != GpuLanguage::Osl1;
    let style = op.style();
    if !dyn_ && GradingTonePreRender::from_value(style, &op.value()).local_bypass {
        return Ok(());
    }

    if is_dynamic && lang == GpuLanguage::Osl1 {
        log_warning(&format!(
            "The dynamic properties are not yet supported by the 'Open Shading language (OSL)' \
             translation: The '{OP_PREFIX}' dynamic property is replaced by a local variable."
        ));
    }

    let dir = op.direction();

    let mut st = GpuShaderText::new(lang);
    st.indent();

    nl!(st, "");
    nl!(
        st,
        "// Add GradingTone '",
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
    let mut p = GtProperties::default();
    add_gt_properties(shader_creator, &mut st, op, &mut p, dyn_)?;

    if dyn_ {
        nl!(st, "if (!", st.cast_to_bool(&p.local_bypass), ")");
        nl!(st, "{");
        st.indent();
    }

    match dir {
        TransformDirection::Forward => add_gt_forward_shader(shader_creator, &mut st, &p, style)?,
        TransformDirection::Inverse => add_gt_inverse_shader(shader_creator, &mut st, &p, style)?,
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
