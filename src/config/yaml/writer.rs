//! Writing of configs to YAML (the `save` functions of `OCIOYaml.cpp`).

use super::emitter::{Emitter, Manip};
use super::sanitize_newlines;
use crate::config::display::View;
use crate::config::file_rules::keys as fr_keys;
use crate::config::logging::log_warning;
use crate::config::utils::split_string_env_style_lossy;
use crate::config::{ColorSpace, Config, Look, NamedTransform, ViewTransform};
use crate::error::{Error, Result};
use crate::math_utils::{is_scalar_equal_to_one, is_vec_equal_to_one, is_vec_equal_to_zero};
use crate::transforms::grading::*;
use crate::transforms::*;
use crate::types::*;
use std::collections::BTreeMap;

use Manip::*;

fn save_description(out: &mut Emitter, desc: &str) {
    if desc.is_empty() {
        return;
    }
    let d = sanitize_newlines(desc);
    out.key("description");
    if d.contains('\n') {
        out.manip(Literal);
    }
    out.string(&d);
}

fn save_interchange(out: &mut Emitter, map: &BTreeMap<String, String>) {
    if map.is_empty() {
        return;
    }
    out.key("interchange").manip(BeginMap);
    for (k, v) in map {
        let v = sanitize_newlines(v);
        out.key(k);
        if v.contains('\n') {
            out.manip(Literal);
        }
        out.string(&v);
    }
    out.manip(EndMap);
}

fn save_view(out: &mut Emitter, v: &View) {
    out.verbatim_tag("View").manip(Flow).manip(BeginMap);
    out.key("name").string(&v.name);
    if v.view_transform.is_empty() {
        out.key("colorspace").string(&v.colorspace);
    } else {
        out.key("view_transform").string(&v.view_transform);
        out.key("display_colorspace").string(&v.colorspace);
    }
    if !v.looks.is_empty() {
        out.key("looks").string(&v.looks);
    }
    if !v.rule.is_empty() {
        out.key("rule").string(&v.rule);
    }
    save_description(out, &v.description);
    out.manip(EndMap);
}

fn emit_direction(out: &mut Emitter, dir: TransformDirection) {
    if dir == TransformDirection::Inverse {
        out.key("direction").manip(Flow).string(dir.as_str());
    }
}

fn emit_name(out: &mut Emitter, md: &crate::format_metadata::FormatMetadata) {
    let name = md.name();
    if !name.is_empty() {
        out.key("name").string(name);
    }
}

fn save_log_param(out: &mut Emitter, p: &[f64; 3], default: f64, name: &str) {
    if p[0] == p[1] && p[0] == p[2] {
        if p[0] != default {
            out.key(name).double(p[0]);
        }
    } else {
        out.key(name).double_seq(p);
    }
}

fn save_rgbm(out: &mut Emitter, name: &str, v: &GradingRgbm, def: &GradingRgbm) {
    if v != def {
        out.key(name).manip(Flow).manip(BeginMap);
        out.key("rgb")
            .manip(Flow)
            .double_seq(&[v.red, v.green, v.blue]);
        out.key("master").manip(Flow).double(v.master);
        out.manip(EndMap);
    }
}

fn save_double(out: &mut Emitter, name: &str, v: f64, def: f64) {
    if v != def {
        out.key(name).manip(Flow).double(v);
    }
}

fn save_pivot(
    out: &mut Emitter,
    v: f64,
    save_contrast: bool,
    black: f64,
    def_black: f64,
    white: f64,
    def_white: f64,
) {
    if save_contrast || black != def_black || white != def_white {
        out.key("pivot").manip(Flow).manip(BeginMap);
        if save_contrast {
            out.key("contrast").manip(Flow).double(v);
        }
        save_double(out, "black", black, def_black);
        save_double(out, "white", white, def_white);
        out.manip(EndMap);
    }
}

fn save_clamp(out: &mut Emitter, black: f64, def_black: f64, white: f64, def_white: f64) {
    if black != def_black || white != def_white {
        out.key("clamp").manip(Flow).manip(BeginMap);
        save_double(out, "black", black, def_black);
        save_double(out, "white", white, def_white);
        out.manip(EndMap);
    }
}

fn save_curve(out: &mut Emitter, name: &str, c: &GradingBSplineCurve) {
    let pts: Vec<f32> = c.control_points.iter().flat_map(|p| [p.x, p.y]).collect();
    out.key(name).manip(Flow).manip(BeginMap);
    out.key("control_points").float_seq(&pts);
    if !c.slopes_are_default() {
        let slopes: Vec<f32> = (0..c.num_control_points()).map(|i| c.slope(i)).collect();
        out.key("slopes").float_seq(&slopes);
    }
    out.manip(EndMap);
}

fn save_rgbmsw(
    out: &mut Emitter,
    name: &str,
    v: &GradingRgbmsw,
    def: &GradingRgbmsw,
    center: bool,
    pivot: bool,
) {
    if v != def {
        out.key(name).manip(Flow).manip(BeginMap);
        out.key("rgb")
            .manip(Flow)
            .double_seq(&[v.red, v.green, v.blue]);
        out.key("master").manip(Flow).double(v.master);
        out.key(if center { "center" } else { "start" })
            .manip(Flow)
            .double(v.start);
        out.key(if pivot { "pivot" } else { "width" })
            .manip(Flow)
            .double(v.width);
        out.manip(EndMap);
    }
}

fn is_experimental_ff(s: FixedFunctionStyle) -> bool {
    use FixedFunctionStyle as S;
    matches!(
        s,
        S::AcesOutputTransform20
            | S::AcesRgbToJmh20
            | S::AcesTonescaleCompress20
            | S::AcesGamutCompress20
    )
}

fn fixed_function_style_to_string(s: FixedFunctionStyle) -> Result<&'static str> {
    match s {
        FixedFunctionStyle::AcesGamutMap02 | FixedFunctionStyle::AcesGamutMap07 => Err(Error::msg(
            "Unimplemented fixed function types: FIXED_FUNCTION_ACES_GAMUTMAP_02, FIXED_FUNCTION_ACES_GAMUTMAP_07.",
        )),
        _ => Ok(s.as_str()),
    }
}

fn is_m44_identity(m: &[f64; 16]) -> bool {
    (0..16).all(|i| {
        if i % 5 == 0 {
            is_scalar_equal_to_one(m[i])
        } else {
            crate::math_utils::is_scalar_equal_to_zero(m[i])
        }
    })
}

/// Write a transform (`save(YAML::Emitter&, ConstTransformRcPtr, majorVersion)`).
pub fn save_transform(out: &mut Emitter, t: &Transform, major: u32) -> Result<()> {
    match t {
        Transform::Allocation(t) => {
            out.verbatim_tag("AllocationTransform")
                .manip(Flow)
                .manip(BeginMap);
            out.key("allocation")
                .manip(Flow)
                .string(t.allocation.as_str());
            if !t.vars.is_empty() {
                let vars: Vec<f32> = t.vars.iter().map(|v| *v as f32).collect();
                out.key("vars").manip(Flow).float_seq(&vars);
            }
            emit_direction(out, t.direction);
            out.manip(EndMap);
        }
        Transform::Builtin(t) => {
            out.verbatim_tag("BuiltinTransform")
                .manip(Flow)
                .manip(BeginMap);
            out.key("style").manip(Flow).string(&t.style);
            emit_direction(out, t.direction);
            out.manip(EndMap);
        }
        Transform::Cdl(t) => {
            out.verbatim_tag("CDLTransform").manip(Flow).manip(BeginMap);
            if major >= 2 {
                emit_name(out, &t.metadata);
            }
            if !is_vec_equal_to_one(&t.slope) {
                out.key("slope").manip(Flow).double_seq(&t.slope);
            }
            if !is_vec_equal_to_zero(&t.offset) {
                out.key("offset").manip(Flow).double_seq(&t.offset);
            }
            if !is_vec_equal_to_one(&t.power) {
                out.key("power").manip(Flow).double_seq(&t.power);
            }
            if !is_scalar_equal_to_one(t.sat) {
                out.key("sat").double(t.sat);
            }
            if t.style != CdlStyle::NoClamp {
                out.key("style").string(t.style.as_str());
            }
            emit_direction(out, t.direction);
            out.manip(EndMap);
        }
        Transform::ColorSpace(t) => {
            out.verbatim_tag("ColorSpaceTransform")
                .manip(Flow)
                .manip(BeginMap);
            out.key("src").string(&t.src);
            out.key("dst").string(&t.dst);
            if !t.data_bypass {
                out.key("data_bypass").boolean(false);
            }
            emit_direction(out, t.direction);
            out.manip(EndMap);
        }
        Transform::DisplayView(t) => {
            out.verbatim_tag("DisplayViewTransform")
                .manip(Flow)
                .manip(BeginMap);
            out.key("src").string(&t.src);
            out.key("display").string(&t.display);
            out.key("view").string(&t.view);
            if t.looks_bypass {
                out.key("looks_bypass").boolean(true);
            }
            if !t.data_bypass {
                out.key("data_bypass").boolean(false);
            }
            emit_direction(out, t.direction);
            out.manip(EndMap);
        }
        Transform::Exponent(t) => {
            out.verbatim_tag("ExponentTransform")
                .manip(Flow)
                .manip(BeginMap);
            if major >= 2 {
                emit_name(out, &t.metadata);
            }
            let v = t.value;
            if major >= 2 && v[0] == v[1] && v[0] == v[2] && v[3] == 1.0 {
                out.key("value").double(v[0]);
            } else {
                out.key("value").manip(Flow).double_seq(&v);
            }
            if t.negative_style != NegativeStyle::Clamp {
                out.key("style")
                    .manip(Flow)
                    .string(t.negative_style.as_str());
            }
            emit_direction(out, t.direction);
            out.manip(EndMap);
        }
        Transform::ExponentWithLinear(t) => {
            out.verbatim_tag("ExponentWithLinearTransform")
                .manip(Flow)
                .manip(BeginMap);
            emit_name(out, &t.metadata);
            let g = t.gamma;
            if g[0] == g[1] && g[0] == g[2] && g[3] == 1.0 {
                out.key("gamma").double(g[0]);
            } else {
                out.key("gamma").manip(Flow).double_seq(&g);
            }
            let o = t.offset;
            if o[0] == o[1] && o[0] == o[2] && o[3] == 0.0 {
                out.key("offset").double(o[0]);
            } else {
                out.key("offset").manip(Flow).double_seq(&o);
            }
            if t.negative_style != NegativeStyle::Linear {
                out.key("style")
                    .manip(Flow)
                    .string(t.negative_style.as_str());
            }
            emit_direction(out, t.direction);
            out.manip(EndMap);
        }
        Transform::ExposureContrast(t) => {
            out.verbatim_tag("ExposureContrastTransform")
                .manip(Flow)
                .manip(BeginMap);
            emit_name(out, &t.metadata);
            out.key("style").manip(Flow).string(t.style.as_str());
            if !t.exposure_dynamic {
                out.key("exposure").manip(Flow).double(t.exposure);
            }
            if !t.contrast_dynamic {
                out.key("contrast").manip(Flow).double(t.contrast);
            }
            if !t.gamma_dynamic {
                out.key("gamma").manip(Flow).double(t.gamma);
            }
            out.key("pivot").manip(Flow).double(t.pivot);
            if t.log_exposure_step != 0.088 {
                out.key("log_exposure_step")
                    .manip(Flow)
                    .double(t.log_exposure_step);
            }
            if t.log_mid_gray != 0.435 {
                out.key("log_midway_gray")
                    .manip(Flow)
                    .double(t.log_mid_gray);
            }
            emit_direction(out, t.direction);
            out.manip(EndMap);
        }
        Transform::File(t) => {
            out.verbatim_tag("FileTransform")
                .manip(Flow)
                .manip(BeginMap);
            out.key("src").string(&t.src);
            if !t.ccc_id.is_empty() {
                out.key("cccid").string(&t.ccc_id);
            }
            if t.cdl_style != CdlStyle::NoClamp {
                out.key("cdl_style").string(t.cdl_style.as_str());
            }
            let mut interp = t.interpolation;
            if major == 1 && interp == Interpolation::Default {
                interp = Interpolation::Linear;
            }
            if interp != Interpolation::Default {
                out.key("interpolation").string(interp.as_str());
            }
            emit_direction(out, t.direction);
            out.manip(EndMap);
        }
        Transform::FixedFunction(t) => {
            out.verbatim_tag("FixedFunctionTransform")
                .manip(Flow)
                .manip(BeginMap);
            emit_name(out, &t.metadata);
            let style = fixed_function_style_to_string(t.style)?;
            out.key("style").manip(Flow).string(style);
            if is_experimental_ff(t.style) {
                log_warning(&format!(
                    "FixedFunction style is experimental and may be removed in a future release: '{style}'."
                ));
            }
            if !t.params.is_empty() {
                out.key("params").manip(Flow).double_seq(&t.params);
            }
            emit_direction(out, t.direction);
            out.manip(EndMap);
        }
        Transform::GradingPrimary(t) => {
            out.verbatim_tag("GradingPrimaryTransform");
            let style = t.style;
            let v = &t.value;
            let d = GradingPrimary::new(style);
            if *v == d {
                out.manip(Flow);
            }
            out.manip(BeginMap);
            emit_name(out, &t.metadata);
            out.key("style").manip(Flow).string(style.as_str());
            match style {
                GradingStyle::Log => {
                    save_rgbm(out, "brightness", &v.brightness, &d.brightness);
                    save_rgbm(out, "contrast", &v.contrast, &d.contrast);
                    save_rgbm(out, "gamma", &v.gamma, &d.gamma);
                    save_double(out, "saturation", v.saturation, d.saturation);
                    let force = v.contrast != d.contrast || v.pivot != d.pivot;
                    save_pivot(
                        out,
                        v.pivot,
                        force,
                        v.pivot_black,
                        d.pivot_black,
                        v.pivot_white,
                        d.pivot_white,
                    );
                }
                GradingStyle::Lin => {
                    save_rgbm(out, "offset", &v.offset, &d.offset);
                    save_rgbm(out, "exposure", &v.exposure, &d.exposure);
                    save_rgbm(out, "contrast", &v.contrast, &d.contrast);
                    save_double(out, "saturation", v.saturation, d.saturation);
                    let force = v.contrast != d.contrast || v.pivot != d.pivot;
                    save_pivot(out, v.pivot, force, 0.0, 0.0, 0.0, 0.0);
                }
                GradingStyle::Video => {
                    save_rgbm(out, "lift", &v.lift, &d.lift);
                    save_rgbm(out, "gamma", &v.gamma, &d.gamma);
                    save_rgbm(out, "gain", &v.gain, &d.gain);
                    save_rgbm(out, "offset", &v.offset, &d.offset);
                    save_double(out, "saturation", v.saturation, d.saturation);
                    save_pivot(
                        out,
                        0.0,
                        false,
                        v.pivot_black,
                        d.pivot_black,
                        v.pivot_white,
                        d.pivot_white,
                    );
                }
            }
            save_clamp(
                out,
                v.clamp_black,
                d.clamp_black,
                v.clamp_white,
                d.clamp_white,
            );
            emit_direction(out, t.direction);
            out.manip(EndMap);
        }
        Transform::GradingRgbCurve(t) => {
            let def = default_rgb_curve(t.style);
            let line_breaks = t.value.curves.iter().any(|c| *c != def);
            out.verbatim_tag("GradingRGBCurveTransform");
            if !line_breaks {
                out.manip(Flow);
            }
            out.manip(BeginMap);
            emit_name(out, &t.metadata);
            out.key("style").manip(Flow).string(t.style.as_str());
            if t.bypass_lin_to_log {
                out.key("lintolog_bypass").manip(Flow).boolean(true);
            }
            for (i, name) in ["red", "green", "blue", "master"].iter().enumerate() {
                let c = &t.value.curves[i];
                if *c != def || !c.slopes_are_default() {
                    save_curve(out, name, c);
                }
            }
            emit_direction(out, t.direction);
            out.manip(EndMap);
        }
        Transform::GradingHueCurve(t) => {
            let defs: Vec<GradingBSplineCurve> = HueCurveType::ALL
                .iter()
                .map(|c| default_hue_curve(*c, t.style))
                .collect();
            let line_breaks = t.value.curves.iter().zip(&defs).any(|(c, d)| c != d);
            out.verbatim_tag("GradingHueCurveTransform");
            if !line_breaks {
                out.manip(Flow);
            }
            out.manip(BeginMap);
            emit_name(out, &t.metadata);
            out.key("style").manip(Flow).string(t.style.as_str());
            if t.rgb_to_hsy == HsyTransformStyle::None {
                out.key("hsy_transform").manip(Flow).string("none");
            }
            const NAMES: [&str; 8] = [
                "hue_hue", "hue_sat", "hue_lum", "lum_sat", "sat_sat", "lum_lum", "sat_lum",
                "hue_fx",
            ];
            for (i, name) in NAMES.iter().enumerate() {
                let c = &t.value.curves[i];
                if *c != defs[i] || !c.slopes_are_default() {
                    save_curve(out, name, c);
                }
            }
            emit_direction(out, t.direction);
            out.manip(EndMap);
        }
        Transform::GradingTone(t) => {
            out.verbatim_tag("GradingToneTransform");
            let v = &t.value;
            let d = GradingTone::new(t.style);
            if *v == d {
                out.manip(Flow);
            }
            out.manip(BeginMap);
            emit_name(out, &t.metadata);
            out.key("style").manip(Flow).string(t.style.as_str());
            save_rgbmsw(out, "blacks", &v.blacks, &d.blacks, false, false);
            save_rgbmsw(out, "shadows", &v.shadows, &d.shadows, false, true);
            save_rgbmsw(out, "midtones", &v.midtones, &d.midtones, true, false);
            save_rgbmsw(out, "highlights", &v.highlights, &d.highlights, false, true);
            save_rgbmsw(out, "whites", &v.whites, &d.whites, false, false);
            save_double(out, "s_contrast", v.s_contrast, d.s_contrast);
            emit_direction(out, t.direction);
            out.manip(EndMap);
        }
        Transform::Group(g) => {
            out.verbatim_tag("GroupTransform").manip(BeginMap);
            if major >= 2 {
                emit_name(out, &g.metadata);
            }
            emit_direction(out, g.direction);
            out.key("children").manip(BeginSeq);
            for c in &g.transforms {
                save_transform(out, c, major)?;
            }
            out.manip(EndSeq);
            out.manip(EndMap);
        }
        Transform::LogAffine(t) => {
            out.verbatim_tag("LogAffineTransform")
                .manip(Flow)
                .manip(BeginMap);
            emit_name(out, &t.metadata);
            if t.base != 2.0 {
                out.key("base").double(t.base);
            }
            save_log_param(out, &t.log_side_slope, 1.0, "log_side_slope");
            save_log_param(out, &t.log_side_offset, 0.0, "log_side_offset");
            save_log_param(out, &t.lin_side_slope, 1.0, "lin_side_slope");
            save_log_param(out, &t.lin_side_offset, 0.0, "lin_side_offset");
            emit_direction(out, t.direction);
            out.manip(EndMap);
        }
        Transform::LogCamera(t) => {
            out.verbatim_tag("LogCameraTransform")
                .manip(Flow)
                .manip(BeginMap);
            emit_name(out, &t.metadata);
            if t.base != 2.0 {
                out.key("base").double(t.base);
            }
            save_log_param(out, &t.log_side_slope, 1.0, "log_side_slope");
            save_log_param(out, &t.log_side_offset, 0.0, "log_side_offset");
            save_log_param(out, &t.lin_side_slope, 1.0, "lin_side_slope");
            save_log_param(out, &t.lin_side_offset, 0.0, "lin_side_offset");
            save_log_param(out, &t.lin_side_break, f64::NAN, "lin_side_break");
            if let Some(ls) = &t.linear_slope {
                save_log_param(out, ls, f64::NAN, "linear_slope");
            }
            emit_direction(out, t.direction);
            out.manip(EndMap);
        }
        Transform::Log(t) => {
            out.verbatim_tag("LogTransform").manip(Flow).manip(BeginMap);
            if major >= 2 {
                emit_name(out, &t.metadata);
            }
            if t.base != 2.0 || major < 2 {
                out.key("base").double(t.base);
            }
            emit_direction(out, t.direction);
            out.manip(EndMap);
        }
        Transform::Look(t) => {
            out.verbatim_tag("LookTransform")
                .manip(Flow)
                .manip(BeginMap);
            out.key("src").string(&t.src);
            out.key("dst").string(&t.dst);
            out.key("looks").string(&t.looks);
            emit_direction(out, t.direction);
            out.manip(EndMap);
        }
        Transform::Matrix(t) => {
            out.verbatim_tag("MatrixTransform")
                .manip(Flow)
                .manip(BeginMap);
            if major >= 2 {
                emit_name(out, &t.metadata);
            }
            if !is_m44_identity(&t.matrix) {
                out.key("matrix").manip(Flow).double_seq(&t.matrix);
            }
            if !is_vec_equal_to_zero(&t.offset) {
                out.key("offset").manip(Flow).double_seq(&t.offset);
            }
            emit_direction(out, t.direction);
            out.manip(EndMap);
        }
        Transform::Range(t) => {
            out.verbatim_tag("RangeTransform")
                .manip(Flow)
                .manip(BeginMap);
            emit_name(out, &t.metadata);
            if let Some(v) = t.min_in {
                out.key("min_in_value").manip(Flow).double(v);
            }
            if let Some(v) = t.max_in {
                out.key("max_in_value").manip(Flow).double(v);
            }
            if let Some(v) = t.min_out {
                out.key("min_out_value").manip(Flow).double(v);
            }
            if let Some(v) = t.max_out {
                out.key("max_out_value").manip(Flow).double(v);
            }
            if t.style != RangeStyle::Clamp {
                out.key("style").manip(Flow).string(t.style.as_str());
            }
            emit_direction(out, t.direction);
            out.manip(EndMap);
        }
        Transform::Lut1D(_) | Transform::Lut3D(_) => {
            return Err(Error::msg(
                "Unsupported Transform() type for serialization.",
            ));
        }
    }
    Ok(())
}

fn save_color_space(out: &mut Emitter, cs: &ColorSpace, major: u32) -> Result<()> {
    out.verbatim_tag("ColorSpace").manip(BeginMap);
    out.key("name").string(cs.name());
    if major >= 2 && cs.num_aliases() > 0 {
        out.key("aliases").manip(Flow).string_seq(cs.aliases());
    }
    if !cs.interop_id().is_empty() {
        out.key("interop_id").string(cs.interop_id());
    }
    out.key("family").string(cs.family());
    out.key("equalitygroup").string(cs.equality_group());
    out.key("bitdepth").string(cs.bit_depth().as_str());
    save_description(out, cs.description());
    out.key("isdata").boolean(cs.is_data());
    if cs.num_categories() > 0 {
        out.key("categories")
            .manip(Flow)
            .string_seq(cs.categories());
    }
    if !cs.encoding().is_empty() {
        out.key("encoding").string(cs.encoding());
    }
    save_interchange(out, cs.interchange_attributes());
    out.key("allocation").string(cs.allocation().as_str());
    if cs.allocation_num_vars() > 0 {
        out.key("allocationvars")
            .manip(Flow)
            .float_seq(cs.allocation_vars());
    }
    let display = cs.reference_space_type() == ReferenceSpaceType::Display;
    if let Some(t) = cs.transform(ColorSpaceDirection::ToReference) {
        let key = if display {
            "to_display_reference"
        } else if major < 2 {
            "to_reference"
        } else {
            "to_scene_reference"
        };
        out.key(key);
        save_transform(out, t, major)?;
    }
    if let Some(t) = cs.transform(ColorSpaceDirection::FromReference) {
        let key = if display {
            "from_display_reference"
        } else if major < 2 {
            "from_reference"
        } else {
            "from_scene_reference"
        };
        out.key(key);
        save_transform(out, t, major)?;
    }
    out.manip(EndMap).manip(Newline);
    Ok(())
}

fn save_look(out: &mut Emitter, look: &Look, major: u32) -> Result<()> {
    out.verbatim_tag("Look").manip(BeginMap);
    out.key("name").string(look.name());
    out.key("process_space").string(look.process_space());
    save_description(out, look.description());
    save_interchange(out, look.interchange_attributes());
    if let Some(t) = look.transform() {
        out.key("transform");
        save_transform(out, t, major)?;
    }
    if let Some(t) = look.inverse_transform() {
        out.key("inverse_transform");
        save_transform(out, t, major)?;
    }
    out.manip(EndMap).manip(Newline);
    Ok(())
}

fn save_view_transform(out: &mut Emitter, vt: &ViewTransform, major: u32) -> Result<()> {
    out.verbatim_tag("ViewTransform").manip(BeginMap);
    out.key("name").string(vt.name());
    if !vt.family().is_empty() {
        out.key("family").string(vt.family());
    }
    save_description(out, vt.description());
    save_interchange(out, vt.interchange_attributes());
    if vt.num_categories() > 0 {
        out.key("categories")
            .manip(Flow)
            .string_seq(vt.categories());
    }
    let display = vt.reference_space_type() == ReferenceSpaceType::Display;
    if let Some(t) = vt.transform(ViewTransformDirection::ToReference) {
        out.key(if display {
            "to_display_reference"
        } else {
            "to_scene_reference"
        });
        save_transform(out, t, major)?;
    }
    if let Some(t) = vt.transform(ViewTransformDirection::FromReference) {
        out.key(if display {
            "from_display_reference"
        } else {
            "from_scene_reference"
        });
        save_transform(out, t, major)?;
    }
    out.manip(EndMap).manip(Newline);
    Ok(())
}

fn save_named_transform(out: &mut Emitter, nt: &NamedTransform, major: u32) -> Result<()> {
    out.verbatim_tag("NamedTransform").manip(BeginMap);
    out.key("name").string(nt.name());
    if major >= 2 && nt.num_aliases() > 0 {
        out.key("aliases").manip(Flow).string_seq(nt.aliases());
    }
    save_description(out, nt.description());
    if !nt.family().is_empty() {
        out.key("family").string(nt.family());
    }
    if nt.num_categories() > 0 {
        out.key("categories")
            .manip(Flow)
            .string_seq(nt.categories());
    }
    if !nt.encoding().is_empty() {
        out.key("encoding").string(nt.encoding());
    }
    if let Some(t) = nt.transform(TransformDirection::Forward) {
        out.key("transform");
        save_transform(out, t, major)?;
    }
    if let Some(t) = nt.transform(TransformDirection::Inverse) {
        out.key("inverse_transform");
        save_transform(out, t, major)?;
    }
    out.manip(EndMap).manip(Newline);
    Ok(())
}

fn save_file_rule(out: &mut Emitter, config: &Config, pos: usize) -> Result<()> {
    let fr = config.file_rules();
    out.verbatim_tag("Rule").manip(Flow).manip(BeginMap);
    out.key(fr_keys::NAME).string(fr.name(pos)?);
    let cs = fr.color_space(pos)?;
    if !cs.is_empty() {
        out.key(fr_keys::COLOR_SPACE).string(cs);
    }
    let regex = fr.regex(pos)?;
    if !regex.is_empty() {
        out.key(fr_keys::REGEX).string(regex);
    }
    let pattern = fr.pattern(pos)?;
    if !pattern.is_empty() {
        out.key(fr_keys::PATTERN).string(pattern);
    }
    let ext = fr.extension(pos)?;
    if !ext.is_empty() {
        out.key(fr_keys::EXTENSION).string(ext);
    }
    let n = fr.num_custom_keys(pos)?;
    if n > 0 {
        out.key(fr_keys::CUSTOM_KEY).manip(BeginMap);
        for i in 0..n {
            out.key(fr.custom_key_name(pos, i)?)
                .string(fr.custom_key_value(pos, i)?);
        }
        out.manip(EndMap);
    }
    out.manip(EndMap);
    Ok(())
}

fn save_viewing_rule(out: &mut Emitter, config: &Config, pos: usize) -> Result<()> {
    let vr = config.viewing_rules();
    out.verbatim_tag("Rule").manip(Flow).manip(BeginMap);
    out.key("name").string(vr.name(pos)?);
    let ncs = vr.num_color_spaces(pos)?;
    if ncs == 1 {
        out.key("colorspaces").string(vr.color_space(pos, 0)?);
    } else if ncs > 1 {
        let v: Vec<String> = (0..ncs)
            .map(|i| vr.color_space(pos, i).unwrap_or("").to_string())
            .collect();
        out.key("colorspaces").manip(Flow).string_seq(&v);
    }
    let nenc = vr.num_encodings(pos)?;
    if nenc == 1 {
        out.key("encodings").string(vr.encoding(pos, 0)?);
    } else if nenc > 1 {
        let v: Vec<String> = (0..nenc)
            .map(|i| vr.encoding(pos, i).unwrap_or("").to_string())
            .collect();
        out.key("encodings").manip(Flow).string_seq(&v);
    }
    let n = vr.num_custom_keys(pos)?;
    if n > 0 {
        out.key("custom").manip(BeginMap);
        for i in 0..n {
            out.key(vr.custom_key_name(pos, i)?)
                .string(vr.custom_key_value(pos, i)?);
        }
        out.manip(EndMap);
    }
    out.manip(EndMap);
    Ok(())
}

fn view_of(config: &Config, display: &str, name: &str) -> View {
    View::new(
        name,
        config.display_view_transform_name(display, name),
        config.display_view_color_space_name(display, name),
        config.display_view_looks(display, name),
        config.display_view_rule(display, name),
        config.display_view_description(display, name),
    )
}

fn save_config(out: &mut Emitter, config: &Config) -> Result<()> {
    let major = config.major_version();
    let minor = config.minor_version();
    let version = if minor != 0 {
        format!("{major}.{minor}")
    } else {
        format!("{major}")
    };

    out.manip(Block).manip(BeginMap);
    out.key("ocio_profile_version").string(&version);
    out.manip(Newline).manip(Newline);

    if major >= 2 || config.num_environment_vars() > 0 {
        out.key("environment").manip(BeginMap);
        for i in 0..config.num_environment_vars() {
            let name = config.environment_var_name_by_index(i);
            out.key(name).string(config.environment_var_default(name));
        }
        out.manip(EndMap).manip(Newline);
    }

    if major < 2 {
        out.key("search_path").string(&config.search_path());
    } else {
        let n = config.num_search_paths();
        let paths: Vec<String> = (0..n)
            .map(|i| config.search_path_by_index(i).to_string())
            .collect();
        match n {
            0 => {
                out.key("search_path").string("");
            }
            1 => {
                out.key("search_path").string(&paths[0]);
            }
            _ => {
                out.key("search_path").string_seq(&paths);
            }
        }
    }
    out.key("strictparsing")
        .boolean(config.is_strict_parsing_enabled());

    if major >= 2 {
        let sep = config.family_separator();
        if sep != '/' {
            out.key("family_separator").character(sep);
        }
    }

    out.key("luma")
        .manip(Flow)
        .double_seq(&config.default_luma_coefs());

    if major >= 2 && !config.name().is_empty() {
        out.key("name").string(config.name());
    }
    save_description(out, config.description());

    // Roles.
    out.manip(Newline).manip(Newline);
    out.key("roles").manip(BeginMap);
    for i in 0..config.num_roles() {
        let role = config.role_name(i);
        if !role.is_empty() {
            out.key(role).string(config.role_color_space_by_index(i));
        }
    }
    out.manip(EndMap).manip(Newline);

    // File rules.
    if major >= 2 {
        out.manip(Newline);
        out.key("file_rules").manip(BeginSeq);
        for i in 0..config.file_rules().num_entries() {
            save_file_rule(out, config, i)?;
        }
        out.manip(EndSeq).manip(Newline);
    }

    // Viewing rules.
    if major >= 2 {
        let n = config.viewing_rules().num_entries();
        if n > 0 {
            out.manip(Newline);
            out.key("viewing_rules").manip(BeginSeq);
            for i in 0..n {
                save_viewing_rule(out, config, i)?;
            }
            out.manip(EndSeq).manip(Newline);
        }
    }

    // Shared views.
    let n_shared = config.num_views_by_type(ViewType::Shared, "");
    if n_shared > 0 {
        out.manip(Newline);
        out.key("shared_views").manip(BeginSeq);
        for v in 0..n_shared {
            let name = config.view_by_type(ViewType::Shared, "", v).to_string();
            save_view(out, &view_of(config, "", &name));
        }
        out.manip(EndSeq).manip(Newline);
    }

    // Displays.
    out.manip(Newline);
    out.key("displays").manip(BeginMap);
    for i in 0..config.num_displays_all() {
        if config.is_display_temporary(i) {
            continue;
        }
        let display = config.display_all(i).to_string();
        out.key(&display).manip(BeginSeq);
        for v in 0..config.num_views_by_type(ViewType::DisplayDefined, &display) {
            let name = config
                .view_by_type(ViewType::DisplayDefined, &display, v)
                .to_string();
            save_view(out, &view_of(config, &display, &name));
        }
        let shared: Vec<String> = (0..config.num_views_by_type(ViewType::Shared, &display))
            .map(|v| {
                config
                    .view_by_type(ViewType::Shared, &display, v)
                    .to_string()
            })
            .collect();
        if !shared.is_empty() {
            out.verbatim_tag("Views").manip(Flow).string_seq(&shared);
        }
        out.manip(EndSeq);
    }
    out.manip(EndMap);

    // Virtual display.
    let n_virtual = config.virtual_display_num_views(ViewType::DisplayDefined)
        + config.virtual_display_num_views(ViewType::Shared);
    if major >= 2 && n_virtual > 0 {
        out.manip(Newline).manip(Newline);
        out.key("virtual_display").manip(BeginSeq);
        for i in 0..config.virtual_display_num_views(ViewType::DisplayDefined) {
            let name = config
                .virtual_display_view(ViewType::DisplayDefined, i)
                .to_string();
            let v = View::new(
                &name,
                config.virtual_display_view_transform_name(&name),
                config.virtual_display_view_color_space_name(&name),
                config.virtual_display_view_looks(&name),
                config.virtual_display_view_rule(&name),
                config.virtual_display_view_description(&name),
            );
            save_view(out, &v);
        }
        let shared: Vec<String> = (0..config.virtual_display_num_views(ViewType::Shared))
            .map(|i| config.virtual_display_view(ViewType::Shared, i).to_string())
            .collect();
        if !shared.is_empty() {
            out.verbatim_tag("Views").manip(Flow).string_seq(&shared);
        }
        out.manip(EndSeq);
    }

    out.manip(Newline).manip(Newline);
    let active_displays: Vec<String> = (0..config.num_active_displays())
        .filter_map(|i| config.active_display(i).map(|s| s.to_string()))
        .collect();
    out.key("active_displays")
        .manip(Flow)
        .string_seq(&active_displays);
    let active_views: Vec<String> = (0..config.num_active_views())
        .filter_map(|i| config.active_view(i).map(|s| s.to_string()))
        .collect();
    out.key("active_views")
        .manip(Flow)
        .string_seq(&active_views);

    let inactive = config.inactive_color_spaces();
    if !inactive.is_empty() {
        let v = split_string_env_style_lossy(inactive);
        out.key("inactive_colorspaces").manip(Flow).string_seq(&v);
    }
    out.manip(Newline);

    // Looks.
    if config.num_looks() > 0 {
        out.manip(Newline);
        out.key("looks").manip(BeginSeq);
        for i in 0..config.num_looks() {
            let name = config.look_name_by_index(i);
            if let Some(l) = config.look(name) {
                save_look(out, l, major)?;
            }
        }
        out.manip(EndSeq).manip(Newline);
    }

    // View transforms.
    let def_vt = config.default_view_transform_name();
    if !def_vt.is_empty() {
        out.manip(Newline);
        out.key("default_view_transform").string(def_vt);
        out.manip(Newline);
    }
    if config.num_view_transforms() > 0 {
        out.manip(Newline);
        out.key("view_transforms").manip(BeginSeq);
        for i in 0..config.num_view_transforms() {
            let name = config.view_transform_name_by_index(i);
            if let Some(vt) = config.view_transform(name) {
                save_view_transform(out, vt, major)?;
            }
        }
        out.manip(EndSeq);
    }

    let mut scene_cs = Vec::new();
    let mut display_cs = Vec::new();
    let all =
        config.num_color_spaces_filtered(SearchReferenceSpaceType::All, ColorSpaceVisibility::All);
    for i in 0..all {
        let name = config.color_space_name_by_index_filtered(
            SearchReferenceSpaceType::All,
            ColorSpaceVisibility::All,
            i,
        );
        if let Some(cs) = config.get_color_space(name) {
            if cs.reference_space_type() == ReferenceSpaceType::Display {
                match config.display_all_by_name(name) {
                    Some(idx) if config.is_display_temporary(idx) => {}
                    _ => display_cs.push(cs),
                }
            } else {
                scene_cs.push(cs);
            }
        }
    }

    if !display_cs.is_empty() {
        out.manip(Newline);
        out.key("display_colorspaces").manip(BeginSeq);
        for cs in &display_cs {
            save_color_space(out, cs, major)?;
        }
        out.manip(EndSeq);
    }

    out.manip(Newline);
    out.key("colorspaces").manip(BeginSeq);
    for cs in &scene_cs {
        save_color_space(out, cs, major)?;
    }
    out.manip(EndSeq);

    let n_nt = config.num_named_transforms_filtered(NamedTransformVisibility::All);
    if n_nt > 0 {
        out.manip(Newline);
        out.key("named_transforms").manip(BeginSeq);
        for i in 0..n_nt {
            let name =
                config.named_transform_name_by_index_filtered(NamedTransformVisibility::All, i);
            if let Some(nt) = config.get_named_transform(name) {
                save_named_transform(out, nt, major)?;
            }
        }
        out.manip(EndSeq);
    }

    out.manip(EndMap);
    Ok(())
}

/// Serialize a config (`OCIOYaml::Write`).
pub fn write(config: &Config) -> Result<String> {
    let mut out = Emitter::new();
    out.set_double_precision(15);
    out.set_float_precision(7);
    save_config(&mut out, config)?;
    Ok(out.into_string())
}
