//! Loading of configs from YAML (the `load` functions of `OCIOYaml.cpp`).

use super::node::Node;
use super::{sanitize_newlines, ARCHIVE_FILENAME};
use crate::config::display::View;
use crate::config::file_rules::{keys as fr_keys, update_file_rules_from_v1_to_v2, FileRules};
use crate::config::file_rules::{DEFAULT_RULE_NAME, FILE_PATH_SEARCH_RULE_NAME};
use crate::config::logging::{log_debug, log_warning};
use crate::config::utils::{compare, join_string_env_style};
use crate::config::viewing_rules::ViewingRules;
use crate::config::{ColorSpace, Config, Look, NamedTransform, ViewTransform};
use crate::error::{Error, Result};
use crate::transforms::grading::*;
use crate::transforms::*;
use crate::types::*;
use std::collections::HashSet;

// ---------------------------------------------------------------------------
// Error helpers

fn throw_error(node: &Node, msg: &str) -> Error {
    Error::msg(format!("At line {}, '{}' parsing failed: {}", node.line + 1, node.tag, msg))
}

fn key_name(key: &Node) -> String {
    key.as_string().unwrap_or_default()
}

fn throw_value_error(node_name: &str, key: &Node, msg: &str) -> Error {
    Error::msg(format!(
        "At line {}, the value parsing of the key '{}' from '{}' failed: {}",
        key.line + 1,
        key_name(key),
        node_name,
        msg
    ))
}

fn throw_value_error_key(key: &Node, msg: &str) -> Error {
    Error::msg(format!(
        "At line {}, the value parsing of the key '{}' failed: {}",
        key.line + 1,
        key_name(key),
        msg
    ))
}

fn log_unknown_key(node: &Node, key: &Node) {
    log_warning(&format!(
        "At line {}, unknown key '{}' in '{}'.",
        key.line + 1,
        key_name(key),
        node.tag
    ));
}

fn log_unknown_key_named(name: &str, key: &Node) {
    log_warning(&format!("Unknown key in {}: '{}'.", name, key_name(key)));
}

fn wrap<T>(node: &Node, what: &str, r: Result<T>) -> Result<T> {
    r.map_err(|e| {
        Error::msg(format!(
            "At line {}, '{}' parsing {} failed with: {}",
            node.line + 1,
            node.tag,
            what,
            e.message()
        ))
    })
}

fn load_bool(node: &Node) -> Result<bool> {
    wrap(node, "boolean", node.as_bool())
}

fn load_f64(node: &Node) -> Result<f64> {
    wrap(node, "double", node.as_f64())
}

fn load_string(node: &Node) -> Result<String> {
    wrap(node, "string", node.as_string())
}

fn load_string_vec(node: &Node) -> Result<Vec<String>> {
    wrap(node, "StringVec", node.as_string_vec())
}

fn load_f32_vec(node: &Node) -> Result<Vec<f32>> {
    wrap(node, "vector<float>", node.as_f32_vec())
}

fn load_f64_vec(node: &Node) -> Result<Vec<f64>> {
    wrap(node, "vector<double>", node.as_f64_vec())
}

fn load_direction(node: &Node) -> Result<TransformDirection> {
    TransformDirection::parse(&load_string(node)?)
}

fn load_description(node: &Node) -> Result<String> {
    Ok(sanitize_newlines(&load_string(node)?))
}

fn check_duplicates(node: &Node) -> Result<()> {
    let mut keys = HashSet::new();
    for (k, _) in node.map_entries() {
        let key = k.as_string()?;
        if !keys.insert(key.clone()) {
            return Err(throw_value_error(
                &node.tag,
                k,
                &format!("Key-value pair with key '{key}' specified more than once. "),
            ));
        }
    }
    Ok(())
}

/// Iterate over the defined, non-null (key, value) entries of a map.
fn entries(node: &Node) -> Result<Vec<(String, &Node, &Node)>> {
    let mut v = Vec::new();
    for (k, val) in node.map_entries() {
        let key = k.as_string()?;
        if val.is_null() {
            continue;
        }
        v.push((key, k, val));
    }
    Ok(v)
}

fn load_custom_keys<'a>(node: &'a Node, section: &str) -> Result<Vec<(&'a Node, &'a Node)>> {
    if !node.is_map() {
        return Err(throw_error(node, &format!("Expected a YAML map in the {section} section.")));
    }
    Ok(node.map_entries().iter().map(|(k, v)| (k, v)).collect())
}

fn load_interchange(node: &Node, mut set: impl FnMut(&str, &str) -> Result<()>) -> Result<()> {
    if !node.is_map() {
        return Err(throw_error(node, "The 'interchange' content needs to be a map."));
    }
    for (k, v) in load_custom_keys(node, "interchange")? {
        let key = k.as_string()?;
        let val = sanitize_newlines(&v.as_string()?);
        if set(&key, &val).is_err() {
            log_unknown_key_named("interchange", k);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// View

fn load_view(node: &Node) -> Result<View> {
    let mut v = View::default();
    if node.tag != "View" {
        return Ok(v);
    }
    check_duplicates(node)?;
    let mut expecting_scene = false;
    let mut expecting_display = false;
    for (key, k, val) in entries(node)? {
        match key.as_str() {
            "name" => v.name = load_string(val)?,
            "view_transform" => {
                expecting_display = true;
                v.view_transform = load_string(val)?;
            }
            "colorspace" => {
                expecting_scene = true;
                v.colorspace = load_string(val)?;
            }
            "display_colorspace" => {
                expecting_display = true;
                v.colorspace = load_string(val)?;
            }
            "looks" | "look" => v.looks = load_string(val)?,
            "rule" => v.rule = load_string(val)?,
            "description" => v.description = load_string(val)?,
            _ => log_unknown_key(node, k),
        }
    }
    if v.name.is_empty() {
        return Err(throw_error(node, "View does not specify 'name'."));
    }
    if expecting_display == expecting_scene {
        return Err(throw_error(
            node,
            &format!(
                "View '{}' must specify colorspace or view_transform and display_colorspace.",
                v.name
            ),
        ));
    }
    if v.colorspace.is_empty() {
        return Err(throw_error(node, &format!("View '{}' does not specify colorspace.", v.name)));
    }
    Ok(v)
}

// ---------------------------------------------------------------------------
// Transforms

fn vec3(v: &[f64]) -> [f64; 3] {
    [v[0], v[1], v[2]]
}

fn load_allocation(node: &Node) -> Result<Transform> {
    let mut t = AllocationTransform::default();
    check_duplicates(node)?;
    for (key, k, val) in entries(node)? {
        match key.as_str() {
            "allocation" => t.allocation = Allocation::from_str_lossy(&load_string(val)?),
            "vars" => {
                let v = load_f32_vec(val)?;
                if !v.is_empty() {
                    t.vars = v.iter().map(|x| *x as f64).collect();
                }
            }
            "direction" => t.direction = load_direction(val)?,
            _ => log_unknown_key(node, k),
        }
    }
    Ok(Transform::Allocation(t))
}

fn load_builtin(node: &Node) -> Result<Transform> {
    let mut t = BuiltinTransform::default();
    for (key, k, val) in entries(node)? {
        match key.as_str() {
            "style" => t.style = load_string(val)?,
            "direction" => t.direction = load_direction(val)?,
            _ => log_unknown_key(node, k),
        }
    }
    Ok(Transform::Builtin(t))
}

fn load_cdl(node: &Node) -> Result<Transform> {
    let mut t = CdlTransform::default();
    check_duplicates(node)?;
    for (key, k, val) in entries(node)? {
        match key.as_str() {
            "slope" | "offset" | "power" => {
                let v = load_f64_vec(val)?;
                if v.len() != 3 {
                    return Err(throw_value_error(
                        &node.tag,
                        k,
                        &format!("'{}' values must be 3 floats. Found '{}'.", key, v.len()),
                    ));
                }
                match key.as_str() {
                    "slope" => t.slope = vec3(&v),
                    "offset" => t.offset = vec3(&v),
                    _ => t.power = vec3(&v),
                }
            }
            "saturation" | "sat" => t.sat = load_f64(val)?,
            "style" => t.style = CdlStyle::parse(&load_string(val)?)?,
            "direction" => t.direction = load_direction(val)?,
            "name" => t.metadata.set_name(&load_string(val)?),
            _ => log_unknown_key(node, k),
        }
    }
    Ok(Transform::Cdl(t))
}

fn load_color_space_transform(node: &Node) -> Result<Transform> {
    let mut t = ColorSpaceTransform::default();
    check_duplicates(node)?;
    for (key, k, val) in entries(node)? {
        match key.as_str() {
            "src" => t.src = load_string(val)?,
            "dst" => t.dst = load_string(val)?,
            "direction" => t.direction = load_direction(val)?,
            "data_bypass" => t.data_bypass = load_bool(val)?,
            _ => log_unknown_key(node, k),
        }
    }
    Ok(Transform::ColorSpace(t))
}

fn load_display_view(node: &Node) -> Result<Transform> {
    let mut t = DisplayViewTransform::default();
    for (key, k, val) in entries(node)? {
        match key.as_str() {
            "src" => t.src = load_string(val)?,
            "display" => t.display = load_string(val)?,
            "view" => t.view = load_string(val)?,
            "direction" => t.direction = load_direction(val)?,
            "looks_bypass" => t.looks_bypass = load_bool(val)?,
            "data_bypass" => t.data_bypass = load_bool(val)?,
            _ => log_unknown_key(node, k),
        }
    }
    Ok(Transform::DisplayView(t))
}

fn load_exponent(node: &Node) -> Result<Transform> {
    let mut t = ExponentTransform::default();
    check_duplicates(node)?;
    for (key, k, val) in entries(node)? {
        match key.as_str() {
            "value" => {
                let v = if val.is_seq() {
                    load_f64_vec(val)?
                } else {
                    let s = load_f64(val)?;
                    vec![s, s, s, 1.0]
                };
                if v.len() != 4 {
                    return Err(throw_value_error(
                        &node.tag,
                        k,
                        &format!("'value' values must be 4 floats. Found '{}'.", v.len()),
                    ));
                }
                t.value = [v[0], v[1], v[2], v[3]];
            }
            "style" => t.negative_style = NegativeStyle::parse(&load_string(val)?)?,
            "direction" => t.direction = load_direction(val)?,
            "name" => t.metadata.set_name(&load_string(val)?),
            _ => log_unknown_key(node, k),
        }
    }
    Ok(Transform::Exponent(t))
}

fn load_exponent_with_linear(node: &Node) -> Result<Transform> {
    let mut t = ExponentWithLinearTransform::default();
    let err = "ExponentWithLinear parse error, ";
    let mut gamma_found = false;
    let mut offset_found = false;
    check_duplicates(node)?;
    for (key, k, val) in entries(node)? {
        match key.as_str() {
            "gamma" | "offset" => {
                let is_gamma = key == "gamma";
                let v = if val.is_seq() {
                    load_f64_vec(val)?
                } else {
                    let s = load_f64(val)?;
                    vec![s, s, s, if is_gamma { 1.0 } else { 0.0 }]
                };
                if v.len() != 4 {
                    return Err(Error::msg(format!(
                        "{err}{key} field must be 4 floats. Found '{}'.",
                        v.len()
                    )));
                }
                let a = [v[0], v[1], v[2], v[3]];
                if is_gamma {
                    t.gamma = a;
                    gamma_found = true;
                } else {
                    t.offset = a;
                    offset_found = true;
                }
            }
            "style" => t.negative_style = NegativeStyle::parse(&load_string(val)?)?,
            "direction" => t.direction = load_direction(val)?,
            "name" => t.metadata.set_name(&load_string(val)?),
            _ => log_unknown_key_named(&node.tag, k),
        }
    }
    if !(gamma_found && offset_found) {
        let e = if !gamma_found && !offset_found {
            "gamma and offset fields are missing"
        } else if !gamma_found {
            "gamma field is missing"
        } else {
            "offset field is missing"
        };
        return Err(Error::msg(format!("{err}{e}")));
    }
    Ok(Transform::ExponentWithLinear(t))
}

fn load_exposure_contrast(node: &Node) -> Result<Transform> {
    let mut t = ExposureContrastTransform::default();
    check_duplicates(node)?;
    let (mut dyn_e, mut dyn_c, mut dyn_g) = (true, true, true);
    for (key, k, val) in entries(node)? {
        match key.as_str() {
            "exposure" => {
                t.exposure = load_f64(val)?;
                dyn_e = false;
            }
            "contrast" => {
                t.contrast = load_f64(val)?;
                dyn_c = false;
            }
            "gamma" => {
                t.gamma = load_f64(val)?;
                dyn_g = false;
            }
            "pivot" => t.pivot = load_f64(val)?,
            "log_exposure_step" => t.log_exposure_step = load_f64(val)?,
            "log_midway_gray" => t.log_mid_gray = load_f64(val)?,
            "style" => t.style = ExposureContrastStyle::parse(&load_string(val)?)?,
            "direction" => t.direction = load_direction(val)?,
            "name" => t.metadata.set_name(&load_string(val)?),
            _ => log_unknown_key(node, k),
        }
    }
    t.exposure_dynamic = dyn_e;
    t.contrast_dynamic = dyn_c;
    t.gamma_dynamic = dyn_g;
    Ok(Transform::ExposureContrast(t))
}

fn load_file(node: &Node) -> Result<Transform> {
    let mut t = FileTransform::default();
    check_duplicates(node)?;
    for (key, k, val) in entries(node)? {
        match key.as_str() {
            "src" => t.src = load_string(val)?,
            "cccid" => t.ccc_id = load_string(val)?,
            "cdl_style" => t.cdl_style = CdlStyle::parse(&load_string(val)?)?,
            "interpolation" => t.interpolation = Interpolation::from_str_lossy(&load_string(val)?),
            "direction" => t.direction = load_direction(val)?,
            _ => log_unknown_key(node, k),
        }
    }
    Ok(Transform::File(t))
}

fn is_experimental_ff(s: FixedFunctionStyle) -> bool {
    use FixedFunctionStyle as S;
    matches!(
        s,
        S::AcesOutputTransform20 | S::AcesRgbToJmh20 | S::AcesTonescaleCompress20 | S::AcesGamutCompress20
    )
}

fn load_fixed_function(node: &Node) -> Result<Transform> {
    let mut t = FixedFunctionTransform::new(FixedFunctionStyle::AcesRedMod03, &[]);
    check_duplicates(node)?;
    let mut style_found = false;
    for (key, k, val) in entries(node)? {
        match key.as_str() {
            "params" => {
                let p = load_f64_vec(val)?;
                if !p.is_empty() {
                    t.params = p;
                }
            }
            "style" => {
                let style = load_string(val)?;
                t.style = FixedFunctionStyle::parse(&style)?;
                style_found = true;
                if is_experimental_ff(t.style) {
                    log_warning(&format!(
                        "FixedFunction style is experimental and may be removed in a future release: '{style}'."
                    ));
                }
            }
            "direction" => t.direction = load_direction(val)?,
            "name" => t.metadata.set_name(&load_string(val)?),
            _ => log_unknown_key_named(&node.tag, k),
        }
    }
    if !style_found {
        return Err(throw_error(node, "style value is missing."));
    }
    Ok(Transform::FixedFunction(t))
}

fn load_rgbm(parent: &Node, node: &Node, rgbm: &mut GradingRgbm) -> Result<()> {
    if !node.is_map() {
        return Err(throw_value_error_key(parent, "The value needs to be a map."));
    }
    let (mut rgb_ok, mut master_ok) = (false, false);
    for (key, k, val) in entries(node)? {
        match key.as_str() {
            "rgb" => {
                let v = load_f64_vec(val)?;
                if v.len() != 3 {
                    return Err(throw_error(k, "The RGB value needs to be a 3 doubles."));
                }
                rgbm.red = v[0];
                rgbm.green = v[1];
                rgbm.blue = v[2];
                rgb_ok = true;
            }
            "master" => {
                rgbm.master = load_f64(val)?;
                master_ok = true;
            }
            _ => log_unknown_key(parent, k),
        }
    }
    if !rgb_ok || !master_ok {
        return Err(throw_value_error_key(parent, "Both rgb and master values are required."));
    }
    Ok(())
}

struct Loaded<T> {
    value: T,
    loaded: bool,
}

impl<T: Copy> Loaded<T> {
    fn new(v: T) -> Self {
        Self { value: v, loaded: false }
    }
}

fn load_pivot(
    parent: &Node,
    node: &Node,
    val: &mut Loaded<f64>,
    black: &mut Loaded<f64>,
    white: &mut Loaded<f64>,
) -> Result<()> {
    if !node.is_map() {
        return Err(throw_value_error_key(parent, "The value needs to be a map."));
    }
    for (key, k, v) in entries(node)? {
        match key.as_str() {
            "contrast" => {
                val.value = load_f64(v)?;
                val.loaded = true;
            }
            "black" => {
                black.value = load_f64(v)?;
                black.loaded = true;
            }
            "white" => {
                white.value = load_f64(v)?;
                white.loaded = true;
            }
            _ => log_unknown_key(node, k),
        }
    }
    if !val.loaded && !black.loaded && !white.loaded {
        return Err(throw_value_error_key(parent, "At least one of the pivot values must be provided."));
    }
    Ok(())
}

fn load_clamp(parent: &Node, node: &Node, black: &mut Loaded<f64>, white: &mut Loaded<f64>) -> Result<()> {
    if !node.is_map() {
        return Err(throw_value_error_key(parent, "The value needs to be a map."));
    }
    for (key, k, v) in entries(node)? {
        match key.as_str() {
            "black" => {
                black.value = load_f64(v)?;
                black.loaded = true;
            }
            "white" => {
                white.value = load_f64(v)?;
                white.loaded = true;
            }
            _ => log_unknown_key(node, k),
        }
    }
    if !black.loaded && !white.loaded {
        return Err(throw_value_error_key(parent, "At least one of the clamp values must be provided."));
    }
    Ok(())
}

fn load_grading_primary(node: &Node) -> Result<Transform> {
    check_duplicates(node)?;
    let mut t = GradingPrimaryTransform::new(GradingStyle::Log);
    let def = GradingPrimary::new(GradingStyle::Log);
    let mut brightness = Loaded::new(def.brightness);
    let mut contrast = Loaded::new(def.contrast);
    let mut gamma = Loaded::new(def.gamma);
    let mut offset = Loaded::new(def.offset);
    let mut exposure = Loaded::new(def.exposure);
    let mut lift = Loaded::new(def.lift);
    let mut gain = Loaded::new(def.gain);
    let mut saturation = Loaded::new(def.saturation);
    let mut pivot = Loaded::new(def.pivot);
    let mut pivot_black = Loaded::new(def.pivot_black);
    let mut pivot_white = Loaded::new(def.pivot_white);
    let mut clamp_black = Loaded::new(def.clamp_black);
    let mut clamp_white = Loaded::new(def.clamp_white);
    for (key, k, val) in entries(node)? {
        let rgbm_target = match key.as_str() {
            "brightness" => Some(&mut brightness),
            "contrast" => Some(&mut contrast),
            "gamma" => Some(&mut gamma),
            "offset" => Some(&mut offset),
            "exposure" => Some(&mut exposure),
            "lift" => Some(&mut lift),
            "gain" => Some(&mut gain),
            _ => None,
        };
        if let Some(target) = rgbm_target {
            target.loaded = true;
            load_rgbm(k, val, &mut target.value)?;
            continue;
        }
        match key.as_str() {
            "style" => t.style = GradingStyle::parse(&load_string(val)?)?,
            "direction" => t.direction = load_direction(val)?,
            "pivot" => load_pivot(k, val, &mut pivot, &mut pivot_black, &mut pivot_white)?,
            "saturation" => {
                saturation.loaded = true;
                saturation.value = load_f64(val)?;
            }
            "clamp" => load_clamp(k, val, &mut clamp_black, &mut clamp_white)?,
            "name" => t.metadata.set_name(&load_string(val)?),
            _ => log_unknown_key_named(&node.tag, k),
        }
    }
    let mut v = GradingPrimary::new(t.style);
    if brightness.loaded {
        v.brightness = brightness.value;
    }
    if contrast.loaded {
        v.contrast = contrast.value;
    }
    if gamma.loaded {
        v.gamma = gamma.value;
    }
    if offset.loaded {
        v.offset = offset.value;
    }
    if exposure.loaded {
        v.exposure = exposure.value;
    }
    if lift.loaded {
        v.lift = lift.value;
    }
    if gain.loaded {
        v.gain = gain.value;
    }
    if saturation.loaded {
        v.saturation = saturation.value;
    }
    if pivot.loaded {
        v.pivot = pivot.value;
    }
    if pivot_black.loaded {
        v.pivot_black = pivot_black.value;
    }
    if pivot_white.loaded {
        v.pivot_white = pivot_white.value;
    }
    if clamp_black.loaded {
        v.clamp_black = clamp_black.value;
    }
    if clamp_white.loaded {
        v.clamp_white = clamp_white.value;
    }
    t.value = v;
    Ok(Transform::GradingPrimary(t))
}

fn load_curve(parent: &Node, node: &Node, curve: &mut GradingBSplineCurve) -> Result<()> {
    if !node.is_map() {
        return Err(throw_value_error_key(parent, "The value needs to be a map."));
    }
    let mut cp_ok = false;
    for (key, k, val) in entries(node)? {
        match key.as_str() {
            "control_points" => {
                let v = load_f32_vec(val)?;
                if v.len() % 2 != 0 {
                    return Err(throw_value_error(&node.tag, k, "An even number of float values is required."));
                }
                let n = v.len() / 2;
                curve.set_num_control_points(n);
                for c in 0..n {
                    curve.control_points[c] = GradingControlPoint::new(v[2 * c], v[2 * c + 1]);
                }
                cp_ok = true;
            }
            "slopes" => {
                let v = load_f32_vec(val)?;
                if v.len() != curve.num_control_points() {
                    return Err(throw_value_error(
                        &node.tag,
                        k,
                        "Number of slopes must match number of control points.",
                    ));
                }
                for (i, s) in v.iter().enumerate() {
                    curve.set_slope(i, *s);
                }
            }
            _ => log_unknown_key(parent, k),
        }
    }
    if !cp_ok {
        return Err(throw_value_error_key(parent, "control_points is required."));
    }
    Ok(())
}

fn load_grading_rgb_curve(node: &Node) -> Result<Transform> {
    check_duplicates(node)?;
    let mut t = GradingRgbCurveTransform::new(GradingStyle::Log);
    let mut curves: [Option<GradingBSplineCurve>; 4] = [None, None, None, None];
    for (key, k, val) in entries(node)? {
        let idx = match key.as_str() {
            "red" => Some(0),
            "green" => Some(1),
            "blue" => Some(2),
            "master" => Some(3),
            _ => None,
        };
        if let Some(i) = idx {
            let mut c = GradingBSplineCurve::with_size(0, BSplineType::BSpline);
            load_curve(k, val, &mut c)?;
            curves[i] = Some(c);
            continue;
        }
        match key.as_str() {
            "style" => t.style = GradingStyle::parse(&load_string(val)?)?,
            "direction" => t.direction = load_direction(val)?,
            "lintolog_bypass" => t.bypass_lin_to_log = load_bool(val)?,
            "name" => t.metadata.set_name(&load_string(val)?),
            _ => log_unknown_key_named(&node.tag, k),
        }
    }
    let def = default_rgb_curve(t.style);
    let [r, g, b, m] = curves;
    t.value = GradingRgbCurve {
        curves: [
            r.unwrap_or_else(|| def.clone()),
            g.unwrap_or_else(|| def.clone()),
            b.unwrap_or_else(|| def.clone()),
            m.unwrap_or_else(|| def.clone()),
        ],
    };
    Ok(Transform::GradingRgbCurve(t))
}

const HUE_CURVE_NAMES: [&str; 8] = ["hue_hue", "hue_sat", "hue_lum", "lum_sat", "sat_sat", "lum_lum", "sat_lum", "hue_fx"];

fn load_grading_hue_curve(node: &Node) -> Result<Transform> {
    check_duplicates(node)?;
    let mut t = GradingHueCurveTransform::new(GradingStyle::Log);
    let mut curves: [Option<GradingBSplineCurve>; 8] = Default::default();
    for (key, k, val) in entries(node)? {
        if let Some(i) = HUE_CURVE_NAMES.iter().position(|n| *n == key) {
            let ty = bspline_type_for_hue_curve_type(HueCurveType::ALL[i]);
            let mut c = GradingBSplineCurve::with_size(0, ty);
            load_curve(k, val, &mut c)?;
            curves[i] = Some(c);
            continue;
        }
        match key.as_str() {
            "style" => t.style = GradingStyle::parse(&load_string(val)?)?,
            "direction" => t.direction = load_direction(val)?,
            "hsy_transform" => {
                if load_string(val)? != "none" {
                    return Err(throw_value_error(&node.tag, k, "Unknown hsy_transform value."));
                }
                t.rgb_to_hsy = HsyTransformStyle::None;
            }
            "name" => t.metadata.set_name(&load_string(val)?),
            _ => log_unknown_key_named(&node.tag, k),
        }
    }
    let style = t.style;
    let mut value = GradingHueCurve::new(style);
    for (i, c) in curves.into_iter().enumerate() {
        value.curves[i] = c.unwrap_or_else(|| default_hue_curve(HueCurveType::ALL[i], style));
    }
    t.value = value;
    Ok(Transform::GradingHueCurve(t))
}

fn load_rgbmsw(parent: &Node, node: &Node, v: &mut GradingRgbmsw, center: bool, pivot: bool) -> Result<()> {
    if !node.is_map() {
        return Err(throw_value_error_key(parent, "The value needs to be a map."));
    }
    let (mut rgb_ok, mut master_ok, mut start_ok, mut width_ok) = (false, false, false, false);
    let start_key = if center { "center" } else { "start" };
    let width_key = if pivot { "pivot" } else { "width" };
    for (key, k, val) in entries(node)? {
        if key == "rgb" {
            let x = load_f64_vec(val)?;
            if x.len() != 3 {
                return Err(throw_error(k, "The RGB value needs to be a 3 doubles."));
            }
            v.red = x[0];
            v.green = x[1];
            v.blue = x[2];
            rgb_ok = true;
        } else if key == "master" {
            v.master = load_f64(val)?;
            master_ok = true;
        } else if key == start_key {
            v.start = load_f64(val)?;
            start_ok = true;
        } else if key == width_key {
            v.width = load_f64(val)?;
            width_ok = true;
        } else {
            log_unknown_key(parent, k);
        }
    }
    if !rgb_ok || !master_ok || !start_ok || !width_ok {
        return Err(throw_value_error_key(
            parent,
            &format!("Rgb, master, {start_key}, and {width_key} values are required."),
        ));
    }
    Ok(())
}

fn load_grading_tone(node: &Node) -> Result<Transform> {
    check_duplicates(node)?;
    let mut t = GradingToneTransform::new(GradingStyle::Log);
    let mut zones: [Option<GradingRgbmsw>; 5] = [None; 5];
    let mut scontrast = 1.0;
    for (key, k, val) in entries(node)? {
        let zone = match key.as_str() {
            "blacks" => Some((0, false, false)),
            "shadows" => Some((1, false, true)),
            "midtones" => Some((2, true, false)),
            "highlights" => Some((3, false, true)),
            "whites" => Some((4, false, false)),
            _ => None,
        };
        if let Some((i, center, pivot)) = zone {
            let mut z = GradingRgbmsw::default();
            load_rgbmsw(k, val, &mut z, center, pivot)?;
            zones[i] = Some(z);
            continue;
        }
        match key.as_str() {
            "style" => t.style = GradingStyle::parse(&load_string(val)?)?,
            "direction" => t.direction = load_direction(val)?,
            "s_contrast" => scontrast = load_f64(val)?,
            "name" => t.metadata.set_name(&load_string(val)?),
            _ => log_unknown_key_named(&node.tag, k),
        }
    }
    let mut v = GradingTone::new(t.style);
    v.s_contrast = scontrast;
    if let Some(z) = zones[0] {
        v.blacks = z;
    }
    if let Some(z) = zones[1] {
        v.shadows = z;
    }
    if let Some(z) = zones[2] {
        v.midtones = z;
    }
    if let Some(z) = zones[3] {
        v.highlights = z;
    }
    if let Some(z) = zones[4] {
        v.whites = z;
    }
    t.value = v;
    Ok(Transform::GradingTone(t))
}

fn load_group(node: &Node) -> Result<Transform> {
    let mut t = GroupTransform::new();
    check_duplicates(node)?;
    for (key, k, val) in entries(node)? {
        match key.as_str() {
            "children" => {
                for child in val.seq_items() {
                    t.transforms.push(load_transform(child)?);
                }
            }
            "direction" => t.direction = load_direction(val)?,
            "name" => t.metadata.set_name(&load_string(val)?),
            _ => log_unknown_key(node, k),
        }
    }
    Ok(Transform::Group(t))
}

fn load_log_param(node: &Node, name: &str) -> Result<[f64; 3]> {
    if node.size() == 0 {
        let v = load_f64(node)?;
        Ok([v, v, v])
    } else {
        let v = load_f64_vec(node)?;
        if v.len() != 3 {
            return Err(Error::msg(format!(
                "LogAffine/CameraTransform parse error, {name} value field must have 3 components. Found '{}'.",
                v.len()
            )));
        }
        Ok(vec3(&v))
    }
}

fn load_base(node: &Node, what: &str, space: &str) -> Result<f64> {
    let nb = node.size();
    if nb == 0 {
        load_f64(node)
    } else {
        Err(Error::msg(format!("{what} parse error, base must be a {space}single double. Found {nb}.")))
    }
}

fn load_log_affine(node: &Node) -> Result<Transform> {
    let mut t = LogAffineTransform::default();
    check_duplicates(node)?;
    for (key, k, val) in entries(node)? {
        match key.as_str() {
            "base" => t.base = load_base(val, "LogAffineTransform", "")?,
            "lin_side_offset" => t.lin_side_offset = load_log_param(val, &key)?,
            "lin_side_slope" => t.lin_side_slope = load_log_param(val, &key)?,
            "log_side_offset" => t.log_side_offset = load_log_param(val, &key)?,
            "log_side_slope" => t.log_side_slope = load_log_param(val, &key)?,
            "direction" => t.direction = load_direction(val)?,
            "name" => t.metadata.set_name(&load_string(val)?),
            _ => log_unknown_key(node, k),
        }
    }
    Ok(Transform::LogAffine(t))
}

fn load_log_camera(node: &Node) -> Result<Transform> {
    let mut t = LogCameraTransform::new([0.0; 3]);
    check_duplicates(node)?;
    let mut lin_break_found = false;
    for (key, k, val) in entries(node)? {
        match key.as_str() {
            "base" => t.base = load_base(val, "LogCameraTransform", "")?,
            "lin_side_offset" => t.lin_side_offset = load_log_param(val, &key)?,
            "lin_side_slope" => t.lin_side_slope = load_log_param(val, &key)?,
            "log_side_offset" => t.log_side_offset = load_log_param(val, &key)?,
            "log_side_slope" => t.log_side_slope = load_log_param(val, &key)?,
            "lin_side_break" => {
                lin_break_found = true;
                t.lin_side_break = load_log_param(val, &key)?;
            }
            "linear_slope" => t.linear_slope = Some(load_log_param(val, &key)?),
            "direction" => t.direction = load_direction(val)?,
            "name" => t.metadata.set_name(&load_string(val)?),
            _ => log_unknown_key(node, k),
        }
    }
    if !lin_break_found {
        return Err(Error::msg("LogCameraTransform parse error: lin_side_break values are missing."));
    }
    Ok(Transform::LogCamera(t))
}

fn load_log(node: &Node) -> Result<Transform> {
    let mut t = LogTransform::default();
    check_duplicates(node)?;
    for (key, k, val) in entries(node)? {
        match key.as_str() {
            "base" => t.base = load_base(val, "LogTransform", " ")?,
            "direction" => t.direction = load_direction(val)?,
            "name" => t.metadata.set_name(&load_string(val)?),
            _ => log_unknown_key_named(&node.tag, k),
        }
    }
    Ok(Transform::Log(t))
}

fn load_look_transform(node: &Node) -> Result<Transform> {
    let mut t = LookTransform::default();
    check_duplicates(node)?;
    for (key, k, val) in entries(node)? {
        match key.as_str() {
            "src" => t.src = load_string(val)?,
            "dst" => t.dst = load_string(val)?,
            "looks" => t.looks = load_string(val)?,
            "direction" => t.direction = load_direction(val)?,
            _ => log_unknown_key(node, k),
        }
    }
    Ok(Transform::Look(t))
}

fn load_matrix(node: &Node) -> Result<Transform> {
    let mut t = MatrixTransform::default();
    check_duplicates(node)?;
    for (key, k, val) in entries(node)? {
        match key.as_str() {
            "matrix" => {
                let v = load_f64_vec(val)?;
                if v.len() != 16 {
                    return Err(throw_value_error(
                        &node.tag,
                        k,
                        &format!("'matrix' values must be 16 numbers. Found '{}'.", v.len()),
                    ));
                }
                t.matrix.copy_from_slice(&v);
            }
            "offset" => {
                let v = load_f64_vec(val)?;
                if v.len() != 4 {
                    return Err(throw_value_error(
                        &node.tag,
                        k,
                        &format!("'offset' values must be 4 numbers. Found '{}'.", v.len()),
                    ));
                }
                t.offset.copy_from_slice(&v);
            }
            "direction" => t.direction = load_direction(val)?,
            "name" => t.metadata.set_name(&load_string(val)?),
            _ => log_unknown_key(node, k),
        }
    }
    Ok(Transform::Matrix(t))
}

fn load_range(node: &Node) -> Result<Transform> {
    let mut t = RangeTransform::default();
    check_duplicates(node)?;
    for (key, k, val) in entries(node)? {
        match key.as_str() {
            "min_in_value" => t.min_in = Some(load_f64(val)?),
            "max_in_value" => t.max_in = Some(load_f64(val)?),
            "min_out_value" => t.min_out = Some(load_f64(val)?),
            "max_out_value" => t.max_out = Some(load_f64(val)?),
            "style" => t.style = RangeStyle::parse(&load_string(val)?)?,
            "direction" => t.direction = load_direction(val)?,
            "name" => t.metadata.set_name(&load_string(val)?),
            _ => log_unknown_key(node, k),
        }
    }
    Ok(Transform::Range(t))
}

/// Load a transform from a tagged YAML map (e.g. `!<MatrixTransform> {...}`).
pub fn load_transform(node: &Node) -> Result<Transform> {
    if !node.is_map() {
        return Err(throw_error(
            node,
            &format!(
                "Unsupported Transform type encountered: ({}) in OCIO profile. Only Mapping types supported.",
                node.type_id()
            ),
        ));
    }
    match node.tag.as_str() {
        "AllocationTransform" => load_allocation(node),
        "BuiltinTransform" => load_builtin(node),
        "CDLTransform" => load_cdl(node),
        "ColorSpaceTransform" => load_color_space_transform(node),
        "DisplayViewTransform" => load_display_view(node),
        "ExponentTransform" => load_exponent(node),
        "ExponentWithLinearTransform" => load_exponent_with_linear(node),
        "ExposureContrastTransform" => load_exposure_contrast(node),
        "FileTransform" => load_file(node),
        "FixedFunctionTransform" => load_fixed_function(node),
        "GradingPrimaryTransform" => load_grading_primary(node),
        "GradingRGBCurveTransform" => load_grading_rgb_curve(node),
        "GradingHueCurveTransform" => load_grading_hue_curve(node),
        "GradingToneTransform" => load_grading_tone(node),
        "GroupTransform" => load_group(node),
        "LogAffineTransform" => load_log_affine(node),
        "LogCameraTransform" => load_log_camera(node),
        "LogTransform" => load_log(node),
        "LookTransform" => load_look_transform(node),
        "MatrixTransform" => load_matrix(node),
        "RangeTransform" => load_range(node),
        other => Err(throw_error(node, &format!("Unsupported transform type !<{other}> in OCIO profile. "))),
    }
}

// ---------------------------------------------------------------------------
// Config objects

fn load_color_space(node: &Node, cs: &mut ColorSpace, major: u32) -> Result<()> {
    if node.tag != "ColorSpace" {
        return Ok(());
    }
    if !node.is_map() {
        return Err(throw_error(node, "The '!<ColorSpace>' content needs to be a map."));
    }
    check_duplicates(node)?;
    let display = cs.reference_space_type() == ReferenceSpaceType::Display;
    for (key, k, val) in entries(node)? {
        match key.as_str() {
            "name" => cs.set_name(&load_string(val)?),
            "aliases" => {
                for a in load_string_vec(val)? {
                    cs.add_alias(&a);
                }
            }
            "interop_id" => cs.set_interop_id(&load_string(val)?)?,
            "description" => cs.set_description(&load_description(val)?),
            "interchange" => load_interchange(val, |k, v| cs.set_interchange_attribute(k, v))?,
            "family" => cs.set_family(&load_string(val)?),
            "equalitygroup" => cs.set_equality_group(&load_string(val)?),
            "bitdepth" => cs.set_bit_depth(BitDepth::from_str_lossy(&load_string(val)?)),
            "isdata" => cs.set_is_data(load_bool(val)?),
            "categories" => {
                for c in load_string_vec(val)? {
                    cs.add_category(&c);
                }
            }
            "encoding" => cs.set_encoding(&load_string(val)?),
            "allocation" => cs.set_allocation(Allocation::from_str_lossy(&load_string(val)?)),
            "allocationvars" => {
                let v = load_f32_vec(val)?;
                if !v.is_empty() {
                    cs.set_allocation_vars(&v);
                }
            }
            k2 if k2 == "to_reference" || (major >= 2 && k2 == "to_scene_reference") => {
                if display {
                    return Err(throw_error(
                        node,
                        "'to_reference' or 'to_scene_reference' cannot be used for a display color space.",
                    ));
                }
                cs.set_transform(Some(load_transform(val)?), ColorSpaceDirection::ToReference);
            }
            "to_display_reference" => {
                if !display {
                    return Err(throw_error(node, "'to_display_reference' cannot be used for a non-display color space."));
                }
                cs.set_transform(Some(load_transform(val)?), ColorSpaceDirection::ToReference);
            }
            k2 if k2 == "from_reference" || (major >= 2 && k2 == "from_scene_reference") => {
                if display {
                    return Err(throw_error(
                        node,
                        "'from_reference' or 'from_scene_reference' cannot be used for a display color space.",
                    ));
                }
                cs.set_transform(Some(load_transform(val)?), ColorSpaceDirection::FromReference);
            }
            "from_display_reference" => {
                if !display {
                    return Err(throw_error(
                        node,
                        "'from_display_reference' cannot be used for a non-display color space.",
                    ));
                }
                cs.set_transform(Some(load_transform(val)?), ColorSpaceDirection::FromReference);
            }
            _ => log_unknown_key(node, k),
        }
    }
    Ok(())
}

fn load_look(node: &Node, look: &mut Look) -> Result<()> {
    if node.tag != "Look" {
        return Ok(());
    }
    check_duplicates(node)?;
    for (key, k, val) in entries(node)? {
        match key.as_str() {
            "name" => look.set_name(&load_string(val)?),
            "process_space" => look.set_process_space(&load_string(val)?),
            "transform" => look.set_transform(Some(load_transform(val)?)),
            "inverse_transform" => look.set_inverse_transform(Some(load_transform(val)?)),
            "description" => look.set_description(&load_description(val)?),
            "interchange" => load_interchange(val, |k, v| look.set_interchange_attribute(k, v))?,
            _ => log_unknown_key(node, k),
        }
    }
    Ok(())
}

fn peek_view_transform_reference_space(node: &Node) -> Result<ReferenceSpaceType> {
    if !node.is_map() {
        return Err(throw_error(node, "The '!<ViewTransform>' content needs to be a map."));
    }
    let (mut scene, mut display) = (false, false);
    for (key, _, _) in entries(node)? {
        match key.as_str() {
            "to_scene_reference" | "from_scene_reference" => scene = true,
            "to_display_reference" | "from_display_reference" => display = true,
            _ => {}
        }
    }
    if !scene && !display {
        return Err(throw_error(node, "The '!<ViewTransform>' needs to refer to a transform."));
    } else if scene && display {
        return Err(throw_error(
            node,
            "The '!<ViewTransform>' cannot have both to/from_reference and to/from_display_reference transforms.",
        ));
    }
    Ok(if display { ReferenceSpaceType::Display } else { ReferenceSpaceType::Scene })
}

fn load_view_transform(node: &Node, vt: &mut ViewTransform) -> Result<()> {
    if node.tag != "ViewTransform" {
        return Ok(());
    }
    if !node.is_map() {
        return Err(throw_error(node, "The '!<ViewTransform>' content needs to be a map."));
    }
    check_duplicates(node)?;
    for (key, k, val) in entries(node)? {
        match key.as_str() {
            "name" => vt.set_name(&load_string(val)?),
            "description" => vt.set_description(&load_description(val)?),
            "interchange" => load_interchange(val, |k, v| vt.set_interchange_attribute(k, v))?,
            "family" => vt.set_family(&load_string(val)?),
            "categories" => {
                for c in load_string_vec(val)? {
                    vt.add_category(&c);
                }
            }
            "to_scene_reference" | "to_display_reference" => {
                vt.set_transform(Some(load_transform(val)?), ViewTransformDirection::ToReference)
            }
            "from_scene_reference" | "from_display_reference" => {
                vt.set_transform(Some(load_transform(val)?), ViewTransformDirection::FromReference)
            }
            _ => log_unknown_key(node, k),
        }
    }
    Ok(())
}

fn load_named_transform(node: &Node, nt: &mut NamedTransform) -> Result<()> {
    if node.tag != "NamedTransform" {
        return Ok(());
    }
    if !node.is_map() {
        return Err(throw_error(node, "The '!<NamedTransform>' content needs to be a map."));
    }
    check_duplicates(node)?;
    for (key, k, val) in entries(node)? {
        match key.as_str() {
            "name" => nt.set_name(&load_string(val)?),
            "aliases" => {
                for a in load_string_vec(val)? {
                    nt.add_alias(&a);
                }
            }
            "description" => nt.set_description(&load_string(val)?),
            "family" => nt.set_family(&load_string(val)?),
            "categories" => {
                for c in load_string_vec(val)? {
                    nt.add_category(&c);
                }
            }
            "encoding" => nt.set_encoding(&load_string(val)?),
            "transform" => nt.set_transform(Some(load_transform(val)?), TransformDirection::Forward),
            "inverse_transform" => nt.set_transform(Some(load_transform(val)?), TransformDirection::Inverse),
            _ => log_unknown_key(node, k),
        }
    }
    Ok(())
}

fn load_file_rule(node: &Node, fr: &mut FileRules, default_found: &mut bool) -> Result<()> {
    if node.tag != "Rule" {
        return Ok(());
    }
    check_duplicates(node)?;
    let (mut name, mut colorspace, mut pattern, mut extension, mut regex) =
        (String::new(), String::new(), String::new(), String::new(), String::new());
    let mut key_vals: Vec<(&Node, &Node)> = Vec::new();
    for (key, k, val) in entries(node)? {
        match key.as_str() {
            fr_keys::NAME => name = load_string(val)?,
            fr_keys::COLOR_SPACE => colorspace = load_string(val)?,
            fr_keys::PATTERN => pattern = load_string(val)?,
            fr_keys::EXTENSION => extension = load_string(val)?,
            fr_keys::REGEX => regex = load_string(val)?,
            fr_keys::CUSTOM_KEY => key_vals = load_custom_keys(val, "file_rules custom attribute")?,
            _ => log_unknown_key(node, k),
        }
    }
    let r: Result<()> = (|| {
        let pos = fr.num_entries() - 1;
        if compare(&name, DEFAULT_RULE_NAME) {
            if !regex.is_empty() || !pattern.is_empty() || !extension.is_empty() {
                return Err(Error::msg(format!(
                    "'{DEFAULT_RULE_NAME}' rule can't use pattern, extension or regex."
                )));
            }
            if colorspace.is_empty() {
                return Err(Error::msg(format!(
                    "'{DEFAULT_RULE_NAME}' rule cannot have an empty color space name."
                )));
            }
            *default_found = true;
            fr.set_color_space(pos, &colorspace)?;
        } else if compare(&name, FILE_PATH_SEARCH_RULE_NAME) {
            if !regex.is_empty() || !pattern.is_empty() || !extension.is_empty() {
                return Err(Error::msg(format!(
                    "'{FILE_PATH_SEARCH_RULE_NAME}' rule can't use pattern, extension or regex."
                )));
            }
            fr.insert_path_search_rule(pos)?;
        } else {
            if !regex.is_empty() && (!pattern.is_empty() || !extension.is_empty()) {
                return Err(Error::msg(format!(
                    "File rule '{name}' can't use regex '{regex}' and pattern & extension '{pattern}' '{extension}'."
                )));
            }
            if colorspace.is_empty() {
                return Err(Error::msg(format!("File rule '{name}' cannot have an empty color space name.")));
            }
            if regex.is_empty() {
                fr.insert_rule(pos, &name, &colorspace, &pattern, &extension)?;
            } else {
                fr.insert_rule_regex(pos, &name, &colorspace, &regex)?;
            }
        }
        for (k, v) in &key_vals {
            fr.set_custom_key(pos, &k.as_string()?, &v.as_string()?)?;
        }
        Ok(())
    })();
    r.map_err(|e| throw_error(node, &format!("File rules: {}", e.message())))
}

fn load_viewing_rule(node: &Node, vr: &mut ViewingRules) -> Result<()> {
    if node.tag != "Rule" {
        return Ok(());
    }
    let mut name = String::new();
    let mut colorspaces = Vec::new();
    let mut encodings = Vec::new();
    let mut key_vals: Vec<(&Node, &Node)> = Vec::new();
    for (key, k, val) in entries(node)? {
        match key.as_str() {
            "name" => name = load_string(val)?,
            "colorspaces" => {
                if val.is_seq() {
                    colorspaces = load_string_vec(val)?;
                } else {
                    colorspaces.push(load_string(val)?);
                }
            }
            "encodings" => {
                if val.is_seq() {
                    encodings = load_string_vec(val)?;
                } else {
                    encodings.push(load_string(val)?);
                }
            }
            "custom" => key_vals = load_custom_keys(val, "viewing_rules custom attribute")?,
            _ => log_unknown_key(node, k),
        }
    }
    let r: Result<()> = (|| {
        let pos = vr.num_entries();
        vr.insert_rule(pos, &name)?;
        for cs in &colorspaces {
            vr.add_color_space(pos, cs)?;
        }
        for e in &encodings {
            vr.add_encoding(pos, e)?;
        }
        for (k, v) in &key_vals {
            vr.set_custom_key(pos, &k.as_string()?, &v.as_string()?)?;
        }
        Ok(())
    })();
    r.map_err(|e| throw_error(node, &format!("Viewing rules: {}", e.message())))
}

fn parse_version(s: &str) -> Option<(u32, u32)> {
    let parts: Vec<&str> = if s.is_empty() { vec![""] } else { s.split('.').collect() };
    // std::stoi: optional leading white spaces, sign and digits, rest ignored.
    fn stoi(p: &str) -> Option<i64> {
        let t = p.trim_start();
        let mut end = 0;
        let b = t.as_bytes();
        if end < b.len() && (b[end] == b'+' || b[end] == b'-') {
            end += 1;
        }
        let ds = end;
        while end < b.len() && b[end].is_ascii_digit() {
            end += 1;
        }
        if end == ds {
            return None;
        }
        t[..end].parse::<i64>().ok()
    }
    match parts.len() {
        1 => Some((stoi(parts[0])? as u32, 0)),
        2 => Some((stoi(parts[0])? as u32, stoi(parts[1])? as u32)),
        _ => None,
    }
}

fn load_config(node: &Node, config: &mut Config, filename: Option<&str>) -> Result<()> {
    let version_node = node.get("ocio_profile_version");
    let mut version = String::new();
    let mut parsed = None;
    if let Some(vn) = version_node {
        version = load_string(vn)?;
        parsed = parse_version(&version);
    }
    let (major, minor) = match parsed {
        Some(v) => v,
        None => {
            let f = filename.filter(|f| !f.is_empty()).unwrap_or("<null>");
            let v = if version.is_empty() { "<null>" } else { version.as_str() };
            return Err(throw_error(
                node,
                &format!("The specified OCIO configuration file {f} does not appear to have a valid version {v}."),
            ));
        }
    };
    if let Err(e) = config.set_version(major, minor) {
        let mut s = String::from("This .ocio config ");
        if let Some(f) = filename.filter(|f| !f.is_empty()) {
            s.push_str(&format!(" '{f}' "));
        }
        s.push_str(&format!(
            "is version {major}.{minor}. This version of the OpenColorIO library ({}) is not able to load that config version.\n{}",
            crate::OCIO_VERSION,
            e.message()
        ));
        return Err(Error::msg(s));
    }

    let mut file_rules_found = false;
    let mut default_rule_found = false;
    let mut file_rules = config.file_rules().clone();
    check_duplicates(node)?;
    let mut mode = EnvironmentMode::LoadAll;

    for (key, k, val) in entries(node)? {
        match key.as_str() {
            "ocio_profile_version" => {}
            "environment" => {
                mode = EnvironmentMode::LoadPredefined;
                if !val.is_map() {
                    return Err(throw_value_error(
                        &node.tag,
                        k,
                        "The value type of key 'environment' needs to be a map.",
                    ));
                }
                for (ek, ev) in val.map_entries() {
                    let name = ek.as_string()?;
                    let value = ev.as_string()?;
                    config.add_environment_var(&name, Some(&value));
                }
            }
            "search_path" | "resource_path" => {
                if val.size() == 0 {
                    config.set_search_path(&load_string(val)?);
                } else {
                    for p in load_string_vec(val)? {
                        config.add_search_path(&p);
                    }
                }
            }
            "strictparsing" => config.set_strict_parsing_enabled(load_bool(val)?),
            "name" => config.set_name(&load_description(val)?),
            "family_separator" => {
                if config.major_version() < 2 {
                    return Err(throw_error(k, "Config v1 can't have 'family_separator'."));
                }
                let s = load_string(val)?;
                let chars: Vec<char> = s.chars().collect();
                if s.len() != 1 {
                    return Err(throw_value_error(
                        &node.tag,
                        k,
                        &format!("'family_separator' value must be a single character. Found '{s}'."),
                    ));
                }
                config.set_family_separator(chars[0])?;
            }
            "description" => config.set_description(&load_description(val)?),
            "luma" => {
                let v = load_f64_vec(val)?;
                if v.len() != 3 {
                    return Err(throw_value_error(
                        &node.tag,
                        k,
                        &format!("'luma' values must be 3 floats. Found '{}'.", v.len()),
                    ));
                }
                config.set_default_luma_coefs(&[v[0], v[1], v[2]]);
            }
            "roles" => {
                if !val.is_map() {
                    return Err(throw_value_error(
                        &node.tag,
                        k,
                        "The value type of the key 'roles' needs to be a map.",
                    ));
                }
                for (rk, rv) in val.map_entries() {
                    config.set_role(&rk.as_string()?, Some(&rv.as_string()?))?;
                }
            }
            "file_rules" => {
                if config.major_version() < 2 {
                    return Err(throw_error(k, "Config v1 can't use 'file_rules'"));
                }
                if !val.is_seq() {
                    return Err(throw_error(val, "The 'file_rules' field needs to be a (- !<Rule>) list."));
                }
                for item in val.seq_items() {
                    if item.tag == "Rule" {
                        if default_rule_found {
                            return Err(throw_error(val, "The 'file_rules' Default rule has to be the last rule."));
                        }
                        load_file_rule(item, &mut file_rules, &mut default_rule_found)?;
                    } else {
                        log_warning(&format!(
                            "Unknown element found in file_rules:{}. Only Rule(s) are currently handled.",
                            item.tag
                        ));
                    }
                }
                if !default_rule_found {
                    return Err(throw_error(k, "The 'file_rules' does not contain a Default <Rule>."));
                }
                file_rules_found = true;
            }
            "viewing_rules" => {
                if !val.is_seq() {
                    return Err(throw_error(val, "The 'viewing_rules' field needs to be a (- !<Rule>) list."));
                }
                let mut vr = ViewingRules::new();
                for item in val.seq_items() {
                    if item.tag == "Rule" {
                        load_viewing_rule(item, &mut vr)?;
                    } else {
                        log_warning(&format!(
                            "Unknown element found in viewing_rules:{}. Only Rule(s) are currently handled.",
                            item.tag
                        ));
                    }
                }
                config.set_viewing_rules(&vr);
            }
            "shared_views" => {
                if !val.is_seq() {
                    return Err(throw_value_error(&node.tag, k, "The view list is a sequence."));
                }
                for item in val.seq_items() {
                    let v = load_view(item)?;
                    config.add_shared_view(&v.name, &v.view_transform, &v.colorspace, &v.looks, &v.rule, &v.description)?;
                }
            }
            "displays" => {
                if !val.is_map() {
                    return Err(throw_value_error(
                        &node.tag,
                        k,
                        "The value type of the key 'displays' needs to be a map.",
                    ));
                }
                for (dk, dv) in val.map_entries() {
                    let display = dk.as_string()?;
                    if !dv.is_seq() {
                        return Err(throw_value_error(&node.tag, k, "The view list is a sequence."));
                    }
                    for item in dv.seq_items() {
                        if item.tag == "View" {
                            let v = load_view(item)?;
                            config.add_display_view_full(
                                &display,
                                &v.name,
                                &v.view_transform,
                                &v.colorspace,
                                &v.looks,
                                &v.rule,
                                &v.description,
                            )?;
                        } else if item.tag == "Views" {
                            for sv in load_string_vec(item)? {
                                config.add_display_shared_view(&display, &sv)?;
                            }
                        }
                    }
                }
            }
            "virtual_display" => {
                if !val.is_seq() {
                    return Err(throw_value_error(&node.tag, k, "The view list is a sequence."));
                }
                for item in val.seq_items() {
                    if item.tag == "View" {
                        let v = load_view(item)?;
                        config.add_virtual_display_view(
                            &v.name,
                            &v.view_transform,
                            &v.colorspace,
                            &v.looks,
                            &v.rule,
                            &v.description,
                        )?;
                    } else if item.tag == "Views" {
                        for sv in load_string_vec(item)? {
                            config.add_virtual_display_shared_view(&sv)?;
                        }
                    } else {
                        log_warning(&format!("Unknown element found in virtual_display:{}.", item.tag));
                    }
                }
            }
            "active_displays" => {
                let v = load_string_vec(val)?;
                config.set_active_displays(&join_string_env_style(&v))?;
            }
            "active_views" => {
                let v = load_string_vec(val)?;
                config.set_active_views(&join_string_env_style(&v))?;
            }
            "inactive_colorspaces" => {
                let v = load_string_vec(val)?;
                config.set_inactive_color_spaces(&join_string_env_style(&v));
            }
            "colorspaces" | "display_colorspaces" => {
                let display = key == "display_colorspaces";
                if !val.is_seq() {
                    let msg = if display {
                        "'display_colorspaces' field needs to be a (- !<ColorSpace>) list."
                    } else {
                        "'colorspaces' field needs to be a (- !<ColorSpace>) list."
                    };
                    return Err(throw_error(val, msg));
                }
                for item in val.seq_items() {
                    if item.tag == "ColorSpace" {
                        let rst = if display { ReferenceSpaceType::Display } else { ReferenceSpaceType::Scene };
                        let mut cs = ColorSpace::new(rst);
                        load_color_space(item, &mut cs, config.major_version())?;
                        let n = config.num_color_spaces();
                        for i in 0..n {
                            if config.color_space_name_by_index(i) == cs.name() {
                                return Err(throw_error(
                                    val,
                                    &format!("Colorspace with name '{}' already defined.", cs.name()),
                                ));
                            }
                        }
                        config.add_color_space(&cs)?;
                    } else {
                        log_warning(&format!(
                            "Unknown element found in colorspaces:{}. Only ColorSpace(s) currently handled.",
                            item.tag
                        ));
                    }
                }
            }
            "looks" => {
                if !val.is_seq() {
                    return Err(throw_error(val, "'looks' field needs to be a (- !<Look>) list."));
                }
                for item in val.seq_items() {
                    if item.tag == "Look" {
                        let mut look = Look::new();
                        load_look(item, &mut look)?;
                        config.add_look(&look)?;
                    } else {
                        log_warning(&format!(
                            "Unknown element found in looks:{}. Only Look(s) currently handled.",
                            item.tag
                        ));
                    }
                }
            }
            "view_transforms" => {
                if !val.is_seq() {
                    return Err(throw_error(val, "'view_transforms' field needs to be a (- !<ViewTransform>) list."));
                }
                for item in val.seq_items() {
                    if item.tag == "ViewTransform" {
                        let rst = peek_view_transform_reference_space(item)?;
                        let mut vt = ViewTransform::new(rst);
                        load_view_transform(item, &mut vt)?;
                        config.add_view_transform(&vt)?;
                    } else {
                        log_warning(&format!(
                            "Unknown element found in view_transforms:{}. Only ViewTransform(s) currently handled.",
                            item.tag
                        ));
                    }
                }
            }
            "default_view_transform" => config.set_default_view_transform_name(&load_string(val)?),
            "named_transforms" => {
                if !val.is_seq() {
                    return Err(throw_error(val, "'named_transforms' field needs to be a (- !<NamedTransform>) list."));
                }
                for item in val.seq_items() {
                    if item.tag == "NamedTransform" {
                        let mut nt = NamedTransform::new();
                        load_named_transform(item, &mut nt)?;
                        if config.get_named_transform(nt.name()).is_some() {
                            return Err(Error::msg(format!(
                                "NamedTransform: There is already one NamedTransform named: '{}'.",
                                nt.name()
                            )));
                        }
                        config.add_named_transform(&nt)?;
                    } else {
                        log_warning(&format!(
                            "Unknown element found in named_transforms:{}. Only NamedTransform(s) currently handled.",
                            item.tag
                        ));
                    }
                }
            }
            _ => log_unknown_key_named("profile", k),
        }
    }

    if let Some(f) = filename {
        if !f.is_empty() && !compare(f, ARCHIVE_FILENAME) {
            let real = crate::path_utils::absolute(f);
            config.set_working_dir(&crate::path_utils::dirname(&real));
        }
    }

    if !file_rules_found {
        if config.major_version() >= 2 {
            if !config.has_role(ROLE_DEFAULT) {
                return Err(throw_error(
                    node,
                    "The config must contain either a Default file rule or the 'default' role.",
                ));
            }
        } else {
            update_file_rules_from_v1_to_v2(config, &mut file_rules)?;
            config.set_file_rules(&file_rules);
        }
    } else {
        if let Some(default_cs) = config.get_color_space(ROLE_DEFAULT) {
            let default_rule = file_rules.num_entries() - 1;
            let rule_cs = file_rules.color_space(default_rule).unwrap_or("").to_string();
            if rule_cs != ROLE_DEFAULT && rule_cs != default_cs.name() {
                log_warning(&format!(
                    "file_rules: defines a default rule using color-space '{}' that does not match the default role '{}'.",
                    rule_cs,
                    default_cs.name()
                ));
            }
        }
        config.set_file_rules(&file_rules);
    }

    config.set_environment_mode(mode);
    config.load_environment();

    if mode == EnvironmentMode::LoadAll {
        let mut s = String::from("This .ocio config ");
        if let Some(f) = filename.filter(|f| !f.is_empty()) {
            s.push_str(&format!(" '{f}' "));
        }
        s.push_str(&format!(
            "has no environment section defined. The default behaviour is to load all environment variables ({}), which reduces the efficiency of OCIO's caching. Consider predefining the environment variables used.",
            config.num_environment_vars()
        ));
        log_debug(&s);
    }
    Ok(())
}

/// Parse `text` into `config` (`OCIOYaml::Read`).
pub fn read(text: &str, config: &mut Config, filename: Option<&str>) -> Result<()> {
    let r = super::node::load(text).and_then(|node| load_config(&node, config, filename));
    r.map_err(|e| {
        let mut s = String::from("Error: Loading the OCIO profile ");
        if let Some(f) = filename {
            if !f.is_empty() && !compare(f, ARCHIVE_FILENAME) {
                s.push_str(&format!("'{f}' "));
            }
        }
        s.push_str("failed. ");
        s.push_str(e.message());
        match e {
            Error::MissingFile(_) => Error::missing_file(s),
            _ => Error::msg(s),
        }
    })
}
