//! Helpers to convert color spaces between configs, using the interchange
//! roles or heuristics to identify the reference spaces (port of the parts of
//! `ConfigUtils.cpp` used by `Config`).

use super::utils::lower;
use super::{ColorSpace, Config};
use crate::error::{Error, Result};
use crate::processor::Processor;
use crate::transforms::Transform;
use crate::types::*;

/// Name of the sRGB texture space of the builtin default config.
pub const SRGB_COLOR_SPACE_NAME: &str = "sRGB - Texture";

/// The candidate linear spaces of the builtin default config.
pub const BUILTIN_LINEAR_SPACES: [&str; 5] =
    ["ACES2065-1", "ACEScg", "Linear Rec.709 (sRGB)", "Linear P3-D65", "Linear Rec.2020"];

/// Temporarily disable the processor cache of a config.
struct SuspendCacheGuard<'a> {
    config: &'a Config,
    flags: ProcessorCacheFlags,
}

impl<'a> SuspendCacheGuard<'a> {
    fn new(config: &'a Config) -> Self {
        let flags = config.processor_cache_flags();
        config.set_processor_cache_flags(ProcessorCacheFlags::OFF);
        Self { config, flags }
    }
}

impl Drop for SuspendCacheGuard<'_> {
    fn drop(&mut self) {
        self.config.set_processor_cache_flags(self.flags);
    }
}

/// Use the interchange roles to find the color spaces to use to convert
/// `src_name` (from `src_config`) to `dst_name` (from `dst_config`).
///
/// Returns the pair of interchange color space names if both roles exist
/// (`None` otherwise) and the reference space type of the interchange.
pub fn get_interchange_roles_for_color_space_conversion(
    src_config: &Config,
    src_name: &str,
    dst_config: &Config,
    dst_name: &str,
) -> Result<(Option<(String, String)>, ReferenceSpaceType)> {
    let dst_cs = dst_config
        .get_color_space(dst_name)
        .ok_or_else(|| Error::msg(format!("Could not find destination color space '{dst_name}'.")))?;
    let mut ty = ReferenceSpaceType::Scene;
    if src_name.is_empty() {
        if dst_cs.reference_space_type() == ReferenceSpaceType::Display {
            ty = ReferenceSpaceType::Display;
        }
    } else {
        let src_cs = src_config
            .get_color_space(src_name)
            .ok_or_else(|| Error::msg(format!("Could not find source color space '{src_name}'.")))?;
        if src_cs.reference_space_type() == ReferenceSpaceType::Display
            && dst_cs.reference_space_type() == ReferenceSpaceType::Display
        {
            ty = ReferenceSpaceType::Display;
        }
    }
    let role = if ty == ReferenceSpaceType::Scene { ROLE_INTERCHANGE_SCENE } else { ROLE_INTERCHANGE_DISPLAY };
    if !src_config.has_role(role) {
        return Ok((None, ty));
    }
    let src_ex = src_config.get_color_space(role).ok_or_else(|| {
        Error::msg(format!("The role '{role}' refers to a color space that is missing in the source config."))
    })?;
    if !dst_config.has_role(role) {
        return Ok((None, ty));
    }
    let dst_ex = dst_config.get_color_space(role).ok_or_else(|| {
        Error::msg(format!("The role '{role}' refers to a color space that is missing in the destination config."))
    })?;
    Ok((Some((src_ex.name().to_string(), dst_ex.name().to_string())), ty))
}

fn contains_srgb(cs: &ColorSpace) -> bool {
    lower(cs.name()).contains("srgb") || cs.aliases().iter().any(|a| a.contains("srgb"))
}

fn ref_space_name(cfg: &Config) -> String {
    let n = cfg.num_color_spaces_filtered(SearchReferenceSpaceType::Scene, ColorSpaceVisibility::All);
    for i in 0..n {
        let name = cfg.color_space_name_by_index_filtered(SearchReferenceSpaceType::Scene, ColorSpaceVisibility::All, i);
        if let Some(cs) = cfg.get_color_space(name) {
            if cs.is_data()
                || cs.transform(ColorSpaceDirection::ToReference).is_some()
                || cs.transform(ColorSpaceDirection::FromReference).is_some()
            {
                continue;
            }
            return name.to_string();
        }
    }
    String::new()
}

fn data_space_name(cfg: &Config) -> String {
    let n = cfg.num_color_spaces_filtered(SearchReferenceSpaceType::Scene, ColorSpaceVisibility::All);
    for i in 0..n {
        let name = cfg.color_space_name_by_index_filtered(SearchReferenceSpaceType::Scene, ColorSpaceVisibility::All, i);
        if cfg.get_color_space(name).map(|c| c.is_data()).unwrap_or(false) {
            return name.to_string();
        }
    }
    String::new()
}

fn is_identity_transform(proc: &Processor, vals: &[[f32; 4]], tol: f32) -> bool {
    let cpu = proc.optimized_cpu_processor(OptimizationFlags::NONE);
    let mut out = vals.to_vec();
    cpu.apply_pixels(&mut out);
    vals.iter().zip(&out).all(|(a, b)| (0..4).all(|c| (a[c] - b[c]).abs() <= tol))
}

fn has_non_trivial_matrix(proc: &Processor) -> bool {
    let gt = proc.create_group_transform();
    gt.transforms.iter().any(|t| match t {
        Transform::Matrix(m) => (0..3).any(|j| (0..3).any(|k| j != k && m.matrix[j * 4 + k].abs() > 0.1)),
        _ => false,
    })
}

fn contains_blocked_transform(t: &Transform) -> bool {
    match t {
        Transform::Group(g) => g.transforms.iter().any(contains_blocked_transform),
        Transform::File(f) => {
            let ext = crate::path_utils::extension(&f.src);
            ext != "spi1d" && ext != "spimtx"
        }
        Transform::ColorSpace(_) | Transform::DisplayView(_) | Transform::Look(_) => true,
        Transform::Lut3D(_) => true,
        _ => false,
    }
}

fn exclude_from_heuristics(cs: &ColorSpace, ref_type: ReferenceSpaceType, block_ref_spaces: bool) -> bool {
    if cs.is_data() || cs.reference_space_type() != ref_type {
        return true;
    }
    if let Some(t) = cs.transform(ColorSpaceDirection::ToReference) {
        return contains_blocked_transform(t);
    }
    if let Some(t) = cs.transform(ColorSpaceDirection::FromReference) {
        return contains_blocked_transform(t);
    }
    block_ref_spaces
}

const TEST_VALS: [[f32; 4]; 5] = [
    [0.7, 0.4, 0.02, 0.0],
    [0.02, 0.6, 0.2, 0.0],
    [0.3, 0.02, 0.5, 0.0],
    [0.0, 0.0, 0.0, 0.0],
    [1.0, 1.0, 1.0, 0.0],
];

fn reference_space_from_linear_space(
    src_config: &Config,
    src_ref: &str,
    cs: &ColorSpace,
    builtin: &Config,
) -> Result<Option<usize>> {
    let vals: [[f32; 4]; 5] = [
        [0.7, 0.4, 0.02, 0.0],
        [0.02, 0.6, -0.2, 0.0],
        [0.3, 0.02, 1.5, 0.0],
        [0.0, 0.0, 0.0, 0.0],
        [1.0, 1.0, 1.0, 0.0],
    ];
    for i in 0..BUILTIN_LINEAR_SPACES.len() {
        for j in 0..BUILTIN_LINEAR_SPACES.len() {
            if i != j {
                let proc = Config::get_processor_from_configs_interchange(
                    src_config,
                    cs.name(),
                    src_ref,
                    builtin,
                    BUILTIN_LINEAR_SPACES[i],
                    BUILTIN_LINEAR_SPACES[j],
                )?;
                if is_identity_transform(&proc, &vals, 1e-3) {
                    return Ok(Some(j));
                }
            }
        }
    }
    Ok(None)
}

fn reference_space_from_srgb_space(
    src_config: &Config,
    src_ref: &str,
    cs: &ColorSpace,
    builtin: &Config,
) -> Result<Option<usize>> {
    let to_ref = if let Some(t) = cs.transform(ColorSpaceDirection::ToReference) {
        t.clone()
    } else if let Some(t) = cs.transform(ColorSpaceDirection::FromReference) {
        let mut t = t.clone();
        t.set_direction(TransformDirection::Inverse);
        t
    } else {
        return Ok(None);
    };
    let vals: [f32; 18] = [0.5, 0.5, 0.5, 0.03, 0.03, 0.03, 0.25, 0.25, 0.25, 0.75, 0.75, 0.75, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0];
    let proc = src_config.get_processor_for_transform(&to_ref, TransformDirection::Forward)?;
    if !has_non_trivial_matrix(&proc) {
        return Ok(None);
    }
    let cpu = proc.optimized_cpu_processor(OptimizationFlags::NONE);
    let mut out = vals;
    cpu.apply_rgb_slice(&mut out);
    for i in 0..out.len() {
        let v = if out[i] <= 0.003_039_934_6_f32 {
            out[i] * 12.923_21_f32
        } else {
            1.055f32 * out[i].powf(1.0 / 2.4) - 0.055
        };
        if (vals[i] - v).abs() > 1e-3 {
            return Ok(None);
        }
    }
    for (i, lin) in BUILTIN_LINEAR_SPACES.iter().enumerate() {
        let proc = Config::get_processor_from_configs_interchange(
            src_config,
            cs.name(),
            src_ref,
            builtin,
            SRGB_COLOR_SPACE_NAME,
            lin,
        )?;
        if is_identity_transform(&proc, &TEST_VALS, 1e-3) {
            return Ok(Some(i));
        }
    }
    Ok(None)
}

/// Identify the interchange spaces (source config, builtin config) to use to
/// convert between the two color spaces, using the interchange roles or
/// heuristics (`IdentifyInterchangeSpace`).
pub fn identify_interchange_space(
    src_config: &Config,
    src_name: &str,
    builtin: &Config,
    builtin_name: &str,
) -> Result<(String, String)> {
    if let (Some(pair), _) =
        get_interchange_roles_for_color_space_conversion(src_config, src_name, builtin, builtin_name)?
    {
        return Ok(pair);
    }
    let builtin_cs = builtin
        .get_color_space(builtin_name)
        .ok_or_else(|| Error::msg(format!("Could not find destination color space '{builtin_name}'.")))?;
    if builtin_cs.reference_space_type() == ReferenceSpaceType::Display {
        return Err(Error::msg(
            "The heuristics currently only support scene-referred color spaces. Please set the interchange roles.",
        ));
    }
    let src_ref = ref_space_name(src_config);
    if src_ref.is_empty() {
        return Err(Error::msg("The supplied config does not have a color space for the reference."));
    }
    let _g1 = SuspendCacheGuard::new(src_config);
    let _g2 = SuspendCacheGuard::new(builtin);

    let mut found = None;
    for i in 0..src_config.num_color_spaces() {
        let name = src_config.color_space_name_by_index(i);
        let cs = match src_config.get_color_space(name) {
            Some(c) => c,
            None => continue,
        };
        if contains_srgb(cs) {
            if exclude_from_heuristics(cs, ReferenceSpaceType::Scene, true) {
                continue;
            }
            found = reference_space_from_srgb_space(src_config, &src_ref, cs, builtin)?;
            if found.is_some() {
                break;
            }
        }
    }
    if found.is_none() {
        for i in 0..src_config.num_color_spaces() {
            let name = src_config.color_space_name_by_index(i);
            let cs = match src_config.get_color_space(name) {
                Some(c) => c,
                None => continue,
            };
            if exclude_from_heuristics(cs, ReferenceSpaceType::Scene, true) {
                continue;
            }
            if src_config.is_color_space_linear(cs.name(), ReferenceSpaceType::Scene)? {
                found = reference_space_from_linear_space(src_config, &src_ref, cs, builtin)?;
                if found.is_some() {
                    break;
                }
            }
        }
    }
    match found {
        Some(i) => Ok((src_ref, BUILTIN_LINEAR_SPACES[i].to_string())),
        None => Err(Error::msg(
            "Heuristics were not able to find a known color space in the provided config. Please set the interchange roles.",
        )),
    }
}

/// Name of the color space of `src_config` equivalent to `builtin_name` of
/// `builtin` (`IdentifyBuiltinColorSpace`).
pub fn identify_builtin_color_space(src_config: &Config, builtin: &Config, builtin_name: &str) -> Result<String> {
    let builtin_cs = builtin.get_color_space(builtin_name).ok_or_else(|| {
        Error::msg(format!("Built-in config does not contain the requested color space: {builtin_name}."))
    })?;
    if builtin_cs.is_data() {
        let d = data_space_name(src_config);
        if d.is_empty() {
            return Err(Error::msg(
                "The requested space is a data space but the supplied config does not have a data space.",
            ));
        }
        return Ok(d);
    }
    let ref_type = builtin_cs.reference_space_type();
    let (src_ex, builtin_ex) = identify_interchange_space(src_config, "", builtin, builtin_name)?;
    let _g1 = SuspendCacheGuard::new(src_config);
    let _g2 = SuspendCacheGuard::new(builtin);
    if !builtin_ex.is_empty() {
        for i in 0..src_config.num_color_spaces() {
            let name = src_config.color_space_name_by_index(i);
            let cs = match src_config.get_color_space(name) {
                Some(c) => c,
                None => continue,
            };
            if exclude_from_heuristics(cs, ref_type, false) {
                continue;
            }
            let proc = Config::get_processor_from_configs_interchange(
                src_config,
                cs.name(),
                &src_ex,
                builtin,
                builtin_name,
                &builtin_ex,
            )?;
            if is_identity_transform(&proc, &TEST_VALS, 5e-3) {
                return Ok(cs.name().to_string());
            }
        }
    }
    Err(Error::msg(format!(
        "Heuristics were not able to find an equivalent to the requested color space: {builtin_name}."
    )))
}

/// Processor between a color space of `src_config` and a color space of the
/// default builtin config.
pub(crate) fn get_processor_to_builtin_cs(
    src_config: &Config,
    src_name: &str,
    builtin_name: &str,
    dir: TransformDirection,
) -> Result<Processor> {
    let builtin = Config::create_from_file("ocio://default")?;
    if builtin.get_color_space(builtin_name).is_none() {
        return Err(Error::msg(format!(
            "Built-in config does not contain the requested color space: {builtin_name}."
        )));
    }
    let (src_ex, builtin_ex) = identify_interchange_space(src_config, src_name, &builtin, builtin_name)?;
    if builtin_ex.is_empty() {
        return Err(Error::msg(
            "Heuristics were not able to find a known color space in the provided config.\nPlease set the interchange roles.",
        ));
    }
    match dir {
        TransformDirection::Forward => Config::get_processor_from_configs_interchange(
            src_config,
            src_name,
            &src_ex,
            &builtin,
            builtin_name,
            &builtin_ex,
        ),
        TransformDirection::Inverse => Config::get_processor_from_configs_interchange(
            &builtin,
            builtin_name,
            &builtin_ex,
            src_config,
            src_name,
            &src_ex,
        ),
    }
}
