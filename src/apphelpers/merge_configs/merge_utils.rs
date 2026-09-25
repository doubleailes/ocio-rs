//! Reference space conversion and color space fingerprints used by the
//! config merger (port of the merge related parts of `ConfigUtils.cpp`:
//! `simplifyTransform`, `getRefSpaceConverter`, `updateReferenceColorspace`,
//! `updateReferenceView`, `initializeRefSpaceConverters` and the color space
//! fingerprints).

use crate::config::{ColorSpace, Config, ViewTransform};
use crate::error::{Error, Result};
use crate::transforms::{ColorSpaceTransform, GroupTransform, MatrixTransform, Transform};
use crate::types::{
    ColorSpaceDirection, ColorSpaceVisibility, OptimizationFlags, ProcessorCacheFlags,
    ReferenceSpaceType, SearchReferenceSpaceType, TransformDirection, ViewTransformDirection,
};
use std::sync::OnceLock;

const BUILTIN_CG_LATEST: &str = "ocio://cg-config-latest";

/// The latest CG builtin config (parsed once).
fn builtin_config() -> Result<&'static Config> {
    static BUILTIN: OnceLock<std::result::Result<Config, Error>> = OnceLock::new();
    BUILTIN
        .get_or_init(|| Config::create_from_builtin_config(BUILTIN_CG_LATEST))
        .as_ref()
        .map_err(|e| e.clone())
}

/// Temporarily deactivate the processor cache of a config.
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

fn search_type(t: ReferenceSpaceType) -> SearchReferenceSpaceType {
    match t {
        ReferenceSpaceType::Scene => SearchReferenceSpaceType::Scene,
        ReferenceSpaceType::Display => SearchReferenceSpaceType::Display,
    }
}

/// Simplify a transform by removing nested group transforms and identities
/// (`simplifyTransform`).
pub fn simplify_transform(gt: &GroupTransform) -> Result<Transform> {
    let config = Config::create_raw();
    let p = config
        .get_processor_for_transform(&Transform::Group(gt.clone()), TransformDirection::Forward)?;
    let opt = p.optimized(OptimizationFlags::DEFAULT);
    let mut final_gt = opt.create_group_transform();
    if final_gt.transforms.len() == 1 {
        return Ok(final_gt.transforms.remove(0));
    }
    Ok(Transform::Group(final_gt))
}

/// Copy of the transform using the inverse direction (`invertTransform`).
pub fn invert_transform(t: &Transform) -> Transform {
    let mut e = t.clone();
    e.set_direction(TransformDirection::Inverse);
    e
}

/// The transform of the color space in the requested direction (inverting
/// the other one if needed), or an identity matrix if the color space has no
/// transform (`getTransformForDir`).
pub fn transform_for_dir(cs: &ColorSpace, dir: ColorSpaceDirection) -> Transform {
    let other = match dir {
        ColorSpaceDirection::ToReference => ColorSpaceDirection::FromReference,
        ColorSpaceDirection::FromReference => ColorSpaceDirection::ToReference,
    };
    if let Some(t) = cs.transform(dir) {
        return t.clone();
    }
    if let Some(t) = cs.transform(other) {
        return invert_transform(t);
    }
    // If it's the reference space, it won't have a transform, so return an identity matrix.
    Transform::Matrix(MatrixTransform::default())
}

fn color_space_of_ref_type(config: &Config, ref_type: ReferenceSpaceType) -> Result<String> {
    // Just return the first one, doesn't matter if it's inactive or a data space.
    let st = search_type(ref_type);
    if config.num_color_spaces_filtered(st, ColorSpaceVisibility::All) > 0 {
        let name = config.color_space_name_by_index_filtered(st, ColorSpaceVisibility::All, 0);
        if let Some(cs) = config.get_color_space(name) {
            return Ok(cs.name().to_string());
        }
    }
    Err(Error::msg(
        "Config is lacking any color spaces of the requested reference space type.",
    ))
}

/// A transform converting from the reference space of `src_config` to the
/// reference space of `dst_config` (scene or display referred depending on
/// `ref_space_type`), `getRefSpaceConverter`.
pub fn ref_space_converter(
    src_config: &Config,
    dst_config: &Config,
    ref_space_type: ReferenceSpaceType,
) -> Result<Transform> {
    let builtin = builtin_config()?;

    // Identify an interchange space for the src config (always a linear color space).
    let (src_interchange, src_builtin_interchange) = Config::identify_interchange_space(
        src_config,
        &color_space_of_ref_type(src_config, ref_space_type)?,
        builtin,
        &color_space_of_ref_type(builtin, ref_space_type)?,
    )?;

    // Identify an interchange space for the dst config.
    let (dst_interchange, dst_builtin_interchange) = Config::identify_interchange_space(
        dst_config,
        &color_space_of_ref_type(dst_config, ref_space_type)?,
        builtin,
        &color_space_of_ref_type(builtin, ref_space_type)?,
    )?;

    // Get the from_ref transform from the src interchange space.
    let src_cs = src_config
        .get_color_space(&src_interchange)
        .ok_or_else(|| Error::msg(format!("Could not find color space '{src_interchange}'.")))?;
    let src_from_ref = transform_for_dir(src_cs, ColorSpaceDirection::FromReference);

    // Get a conversion from one builtin interchange to another.
    let mut src_builtin_to_dst_builtin = None;
    if !src_builtin_interchange.is_empty() && !dst_builtin_interchange.is_empty() {
        let cst = ColorSpaceTransform::new(&src_builtin_interchange, &dst_builtin_interchange);
        src_builtin_to_dst_builtin = Some(
            builtin
                .get_processor_for_transform(
                    &Transform::ColorSpace(cst),
                    TransformDirection::Forward,
                )?
                .create_group_transform(),
        );
    }

    // Append the to_ref transform from the dst interchange space.
    let dst_cs = dst_config
        .get_color_space(&dst_interchange)
        .ok_or_else(|| Error::msg(format!("Could not find color space '{dst_interchange}'.")))?;
    let dst_to_ref = transform_for_dir(dst_cs, ColorSpaceDirection::ToReference);

    // Combine into a group transform. If the src or dst contain file transforms, resolve them
    // so there is no dependence on the search_path of the original configs.
    let mut gt = GroupTransform::new();
    gt.append(
        src_config
            .get_processor_for_transform(&src_from_ref, TransformDirection::Forward)?
            .create_group_transform(),
    );
    if let Some(g) = src_builtin_to_dst_builtin {
        gt.append(g);
    }
    gt.append(
        dst_config
            .get_processor_for_transform(&dst_to_ref, TransformDirection::Forward)?
            .create_group_transform(),
    );

    simplify_transform(&gt)
}

/// True for an empty group transform (`transformIsEmpty`).
pub fn transform_is_empty(tr: &Transform) -> bool {
    matches!(tr, Transform::Group(g) if g.transforms.is_empty())
}

/// Update the reference space used by the transforms of a color space. The
/// argument converts from the current to the new reference space
/// (`updateReferenceColorspace`).
pub fn update_reference_colorspace(cs: &mut ColorSpace, to_new_reference: &Transform) {
    if transform_is_empty(to_new_reference) {
        return;
    }

    let transform_to = cs.transform(ColorSpaceDirection::ToReference).cloned();
    if let Some(t) = &transform_to {
        // NB: Not simplified since it would expand builtin or file transforms.
        let gt = GroupTransform::from_transforms(vec![t.clone(), to_new_reference.clone()]);
        cs.set_transform(Some(Transform::Group(gt)), ColorSpaceDirection::ToReference);
    }

    let transform_from = cs.transform(ColorSpaceDirection::FromReference).cloned();
    if let Some(t) = &transform_from {
        let inv = invert_transform(to_new_reference);
        let gt = GroupTransform::from_transforms(vec![inv, t.clone()]);
        cs.set_transform(
            Some(Transform::Group(gt)),
            ColorSpaceDirection::FromReference,
        );
    }

    if transform_to.is_none() && transform_from.is_none() && !cs.is_data() {
        let gt = GroupTransform::from_transforms(vec![to_new_reference.clone()]);
        cs.set_transform(Some(Transform::Group(gt)), ColorSpaceDirection::ToReference);
    }
}

/// Update the transforms of a view transform to adapt the reference spaces
/// (`updateReferenceView`). Note that the from_ref transform converts from
/// the scene-referred reference space to the display-referred one.
pub fn update_reference_view(
    vt: &mut ViewTransform,
    to_new_scene_reference: &Transform,
    to_new_display_reference: &Transform,
) {
    let empty_scene = transform_is_empty(to_new_scene_reference);
    let empty_display = transform_is_empty(to_new_display_reference);

    if empty_scene && empty_display {
        return;
    }

    let display_ref = vt.reference_space_type() == ReferenceSpaceType::Display;

    if let Some(t) = vt.transform(ViewTransformDirection::ToReference).cloned() {
        let mut gt = GroupTransform::new();
        if !empty_display {
            gt.append(invert_transform(to_new_display_reference));
        }
        gt.append(t);
        if display_ref {
            // Use the converter to display reference on both sides.
            if !empty_display {
                gt.append(to_new_display_reference.clone());
            }
        } else if !empty_scene {
            gt.append(to_new_scene_reference.clone());
        }
        vt.set_transform(
            Some(Transform::Group(gt)),
            ViewTransformDirection::ToReference,
        );
    }

    if let Some(t) = vt.transform(ViewTransformDirection::FromReference).cloned() {
        let mut gt = GroupTransform::new();
        if display_ref {
            // Use the converter to display reference on both sides.
            if !empty_display {
                gt.append(invert_transform(to_new_display_reference));
            }
        } else if !empty_scene {
            gt.append(invert_transform(to_new_scene_reference));
        }
        gt.append(t);
        if !empty_display {
            gt.append(to_new_display_reference.clone());
        }
        vt.set_transform(
            Some(Transform::Group(gt)),
            ViewTransformDirection::FromReference,
        );
    }

    // Note that Config::add_view_transform prevents creating a view transform that has no
    // transforms, so at least one direction will be present.
}

fn has_color_space_ref_type(config: &Config, ref_type: ReferenceSpaceType) -> bool {
    config.num_color_spaces_filtered(search_type(ref_type), ColorSpaceVisibility::All) > 0
}

/// The (scene, display) transforms converting the reference spaces of the
/// input config to the ones of the base config (`initializeRefSpaceConverters`).
/// A converter is an empty group transform if either config lacks that
/// reference space.
pub fn initialize_ref_space_converters(
    base_config: &Config,
    input_config: &Config,
) -> Result<(Transform, Transform)> {
    // Note: The base config reference space is always used, regardless of strategy.

    let scene = if has_color_space_ref_type(base_config, ReferenceSpaceType::Scene)
        && has_color_space_ref_type(input_config, ReferenceSpaceType::Scene)
    {
        ref_space_converter(input_config, base_config, ReferenceSpaceType::Scene)?
    } else {
        // Always need to initialize both transforms, even if they're empty.
        Transform::Group(GroupTransform::new())
    };

    let display = if has_color_space_ref_type(base_config, ReferenceSpaceType::Display)
        && has_color_space_ref_type(input_config, ReferenceSpaceType::Display)
    {
        ref_space_converter(input_config, base_config, ReferenceSpaceType::Display)?
    } else {
        Transform::Group(GroupTransform::new())
    };

    Ok((scene, display))
}

/// The fingerprint of a color space: the test values processed through the
/// `from_reference` direction of the color space.
#[derive(Debug, Clone, PartialEq)]
pub struct Fingerprint {
    /// Color space name.
    pub cs_name: String,
    /// Reference space type of the color space.
    pub ref_type: ReferenceSpaceType,
    /// The processed test values (RGBA).
    pub vals: Vec<f32>,
}

/// The fingerprints of all the color spaces of a (base) config.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ColorSpaceFingerprints {
    /// The fingerprints.
    pub vec: Vec<Fingerprint>,
    /// Test values for scene-referred color spaces (RGBA).
    pub scene_ref_test_vals: Vec<f32>,
    /// Test values for display-referred color spaces (RGBA).
    pub display_ref_test_vals: Vec<f32>,
}

/// Send the test values through the color space. `None` if the color space
/// should not be considered (`calcColorSpaceFingerprint`).
pub fn calc_color_space_fingerprint(
    fingerprints: &ColorSpaceFingerprints,
    config: &Config,
    cs: &ColorSpace,
) -> Option<Vec<f32>> {
    let from_ref = transform_for_dir(cs, ColorSpaceDirection::FromReference);

    // If the transform doesn't validate (singular matrix, etc.), don't consider it.
    let cpu = config
        .get_processor_for_transform(&from_ref, TransformDirection::Forward)
        .ok()?
        .optimized_cpu_processor(OptimizationFlags::NONE);

    let mut vals = if cs.reference_space_type() == ReferenceSpaceType::Display {
        fingerprints.display_ref_test_vals.clone()
    } else {
        fingerprints.scene_ref_test_vals.clone()
    };
    cpu.apply_rgba_slice(&mut vals);
    Some(vals)
}

/// Process `vals` through the `to_reference` direction of the first color
/// space found, trying the names then the builtin config heuristics.
fn to_reference_vals(
    config: &Config,
    names: &[&str],
    builtin_name: &str,
    vals: &[f32],
) -> Result<Vec<f32>> {
    // First check if the config recognizes one of the common names.
    let mut cs = names.iter().find_map(|n| config.get_color_space(n));
    let found_name;
    if cs.is_none() {
        // Otherwise, see if it's present using a different name.
        let builtin = builtin_config()?;
        // This fails if it cannot find the requested space.
        found_name = Config::identify_builtin_color_space(config, builtin, builtin_name)?;
        cs = config.get_color_space(&found_name);
    }
    let cs = cs.ok_or_else(|| Error::msg("Color space not found."))?;

    let to_ref = transform_for_dir(cs, ColorSpaceDirection::ToReference);
    let cpu = config
        .get_processor_for_transform(&to_ref, TransformDirection::Forward)?
        .optimized_cpu_processor(OptimizationFlags::NONE);
    let mut out = vals.to_vec();
    cpu.apply_rgba_slice(&mut out);
    Ok(out)
}

// Define a set of test values to use for a config and store them in the fingerprints struct.
// An attempt is made to convert them to the reference spaces of the config being used.
// There are separate values for scene-referred and display-referred color spaces.
fn initialize_test_vals(fingerprints: &mut ColorSpaceFingerprints, config: &Config) {
    // Test values slightly inside the Rec.709 gamut for the most common scene-referred and
    // display-referred reference spaces.

    #[rustfmt::skip]
    let aces_vals: Vec<f32> = vec![
        0.408933127871, 0.106169822808, 0.027842572707, 0.0, // lin_rec709 {0.9, 0.03, 0.01}
        0.374615373650, 0.739417755017, 0.118862613721, 0.0, // lin_rec709 {0.06, 0.9, 0.02}
        0.171696591718, 0.104272268468, 0.786227391453, 0.0, // lin_rec709 {0.01, 0.02, 0.9}
        0.0,            0.0,            0.0,            0.5,
        0.037018876439, 0.030827687576, 0.021641700645, 0.0, // lin_rec709 {0.05, 0.03, 0.02}
        1.0,            1.0,            1.0,            1.0,
    ];

    #[rustfmt::skip]
    let xyz_vals: Vec<f32> = vec![
        // Adjusted to keep it inside both Rec.601 and Rec.601 PAL.
        0.383684057405, 0.213552088801, 0.030478901760, 0.0, // lin_rec709 {0.9, 0.03, 0.01}
        0.350178969169, 0.657853997550, 0.127445793983, 0.0, // lin_rec709 {0.06, 0.9, 0.02}
        0.173708304342, 0.081402847459, 0.858056140808, 0.0, // lin_rec709 {0.01, 0.02, 0.9}
        0.0,            0.0,            0.0,            0.5,
        0.034956685913, 0.033530856964, 0.023553027375, 0.0, // lin_rec709 {0.05, 0.03, 0.02}
        0.950455927052, 1.0,            1.089057750760, 1.0,
    ];

    // Try to convert to the actual reference spaces of the config.

    fingerprints.scene_ref_test_vals = aces_vals.clone();
    fingerprints.display_ref_test_vals = xyz_vals.clone();

    fingerprints.scene_ref_test_vals = to_reference_vals(
        config,
        &["aces_interchange", "ACES2065-1", "lin_ap0_scene"],
        "aces_interchange",
        &aces_vals,
    )
    .unwrap_or(aces_vals);

    if config
        .num_color_spaces_filtered(SearchReferenceSpaceType::Display, ColorSpaceVisibility::All)
        == 0
    {
        return;
    }

    fingerprints.display_ref_test_vals = to_reference_vals(
        config,
        &["cie_xyz_d65_interchange", "CIE-XYZ-D65", "CIE XYZ-D65"],
        "cie_xyz_d65_interchange",
        &xyz_vals,
    )
    .unwrap_or(xyz_vals);
}

/// Calculate a fingerprint for every color space of a (base) config
/// (`initializeColorSpaceFingerprints`). Data color spaces, color spaces
/// with the `is-unique` category and color spaces having both directions are
/// skipped.
pub fn initialize_color_space_fingerprints(config: &Config) -> ColorSpaceFingerprints {
    let _guard = SuspendCacheGuard::new(config);

    let mut fingerprints = ColorSpaceFingerprints::default();
    initialize_test_vals(&mut fingerprints, config);

    let n =
        config.num_color_spaces_filtered(SearchReferenceSpaceType::All, ColorSpaceVisibility::All);
    for i in 0..n {
        let name = config.color_space_name_by_index_filtered(
            SearchReferenceSpaceType::All,
            ColorSpaceVisibility::All,
            i,
        );
        let cs = match config.get_color_space(name) {
            Some(cs) if !cs.is_data() => cs,
            // Don't put data color spaces in the collection.
            _ => continue,
        };
        if cs.has_category("is-unique") {
            // Don't fingerprint color spaces with this category. This provides a way to
            // identify color spaces that must not be replaced.
            continue;
        }
        if cs.transform(ColorSpaceDirection::FromReference).is_some()
            && cs.transform(ColorSpaceDirection::ToReference).is_some()
        {
            // Don't bother with color spaces that have both directions defined, these are more
            // complicated and less likely to be duplicates.
            continue;
        }
        if let Some(vals) = calc_color_space_fingerprint(&fingerprints, config, cs) {
            fingerprints.vec.push(Fingerprint {
                cs_name: cs.name().to_string(),
                ref_type: cs.reference_space_type(),
                vals,
            });
        }
    }
    fingerprints
}

/// The name of the fingerprinted (base) color space equivalent to
/// `input_cs`, `""` if none is found within the tolerance
/// (`findEquivalentColorspace`). `input_cs` must use the same reference
/// space as the base config.
pub fn find_equivalent_colorspace(
    fingerprints: &ColorSpaceFingerprints,
    input_config: &Config,
    input_cs: &ColorSpace,
) -> String {
    if input_cs.is_data() {
        return String::new();
    }

    let input_vals = match calc_color_space_fingerprint(fingerprints, input_config, input_cs) {
        Some(v) => v,
        None => return String::new(),
    };

    // Increased from 1e-3 to 5e-3 to allow for use of either Bradford or CAT02 adaptation.
    let abs_tolerance = 5e-3f32;

    for fp in &fingerprints.vec {
        // Only compare color spaces that are using the same reference space type.
        if fp.ref_type != input_cs.reference_space_type() {
            continue;
        }
        let matched = input_vals.iter().enumerate().all(|(i, v)| {
            fp.vals
                .get(i)
                .map(|w| {
                    let d = if *v > *w { *v - *w } else { *w - *v };
                    d <= abs_tolerance
                })
                .unwrap_or(false)
        });
        if matched {
            return fp.cs_name.clone();
        }
    }
    String::new()
}
