//! Helpers for (display, view) pairs (port of
//! `apphelpers/DisplayViewHelpers.cpp`, namespace `DisplayViewHelpers`).

use super::category_helpers::{extract_items, find_color_space_names};
use super::legacy_viewing_pipeline::LegacyViewingPipeline;
use crate::config::utils::{compare, contain, join, remove, split, trim};
use crate::config::{ColorSpace, Config};
use crate::context::Context;
use crate::error::{Error, Result};
use crate::processor::Processor;
use crate::transforms::{
    DisplayViewTransform, ExposureContrastTransform, FileTransform, GroupTransform,
    MatrixTransform, Transform,
};
use crate::types::{
    ColorSpaceDirection, ExposureContrastStyle, TransformDirection, OCIO_ACTIVE_DISPLAYS_ENVVAR,
    OCIO_ACTIVE_VIEWS_ENVVAR,
};

/// Exposure / contrast transform with dynamic exposure and contrast (linear
/// style, 0.18 pivot).
fn dynamic_exposure_contrast() -> ExposureContrastTransform {
    ExposureContrastTransform {
        style: ExposureContrastStyle::Linear,
        pivot: 0.18,
        exposure_dynamic: true,
        contrast_dynamic: true,
        ..Default::default()
    }
}

/// Exposure / contrast transform with a dynamic gamma (video style, 1.0
/// pivot).
fn dynamic_gamma() -> ExposureContrastTransform {
    ExposureContrastTransform {
        style: ExposureContrastStyle::Video,
        pivot: 1.0,
        gamma_dynamic: true,
        ..Default::default()
    }
}

/// Processor from the working color space (a color space or role name) to
/// the (display, view) pair (forward) or from the (display, view) pair to the
/// working color space (inverse), using the current context of the config.
///
/// If not already present, exposure / contrast transforms are added to allow
/// changing exposure, contrast and gamma through dynamic properties. The
/// channel view is optional.
pub fn get_processor(
    config: &Config,
    working_name: &str,
    display_name: &str,
    view_name: &str,
    channel_view: Option<&MatrixTransform>,
    direction: TransformDirection,
) -> Result<Processor> {
    get_processor_with_context(
        config,
        config.current_context(),
        working_name,
        display_name,
        view_name,
        channel_view,
        direction,
    )
}

/// Same as [`get_processor`] using an explicit context.
pub fn get_processor_with_context(
    config: &Config,
    context: &Context,
    working_name: &str,
    display_name: &str,
    view_name: &str,
    channel_view: Option<&MatrixTransform>,
    direction: TransformDirection,
) -> Result<Processor> {
    let mut display_transform = DisplayViewTransform::new(working_name, display_name, view_name);
    display_transform.direction = direction;

    let processor = config.get_processor_with_context(
        context,
        &Transform::DisplayView(display_transform.clone()),
        TransformDirection::Forward,
    )?;

    let mut need_exposure = true;
    let mut need_gamma = true;

    if processor.is_dynamic() {
        let grp = processor.create_group_transform();
        for tr in &grp.transforms {
            if let Transform::ExposureContrast(ex) = tr {
                if ex.exposure_dynamic {
                    need_exposure = false;
                }
                if ex.gamma_dynamic {
                    need_gamma = false;
                }
            }
        }
    }

    if !need_exposure && !need_gamma && channel_view.is_none() {
        return Ok(processor);
    }

    let mut pipeline = LegacyViewingPipeline::new();
    pipeline.set_display_view_transform(Some(&display_transform));

    // Upstream OCIO note: Need to update to allow for apps that are viewing non-scene-linear
    // images.
    if need_exposure {
        pipeline.set_linear_cc(Some(&Transform::ExposureContrast(
            dynamic_exposure_contrast(),
        )));
    }

    if need_gamma {
        pipeline.set_display_cc(Some(&Transform::ExposureContrast(dynamic_gamma())));
    }

    if let Some(cv) = channel_view {
        pipeline.set_channel_view(Some(&Transform::Matrix(cv.clone())));
    }

    pipeline.get_processor_with_context(config, context)
}

/// Identity processor containing only the exposure / contrast transforms
/// with dynamic properties.
pub fn get_identity_processor(config: &Config) -> Result<Processor> {
    let mut group = GroupTransform::new();
    group.append(dynamic_exposure_contrast());
    group.append(dynamic_gamma());
    config.get_processor_for_transform(&Transform::Group(group), TransformDirection::Forward)
}

/// Split a comma separated list and trim the items (`StringUtils::Split` +
/// `StringUtils::Trim`).
fn split_trim(list: &str) -> Vec<String> {
    split(list, ',')
        .iter()
        .map(|s| trim(s).to_string())
        .collect()
}

/// A non-empty value of the environment variable.
fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

fn add_active_display_view(config: &mut Config, display_name: &str, view_name: &str) -> Result<()> {
    // Add the display to the active display list only if possible.

    if let Some(env) = non_empty_env(OCIO_ACTIVE_DISPLAYS_ENVVAR) {
        let displays = split_trim(&env);
        let accept_all = displays.len() == 1 && displays[0].is_empty();
        if !accept_all {
            return Err(Error::msg(format!(
                "Forbidden to add an active display as '{OCIO_ACTIVE_DISPLAYS_ENVVAR}' controls the active list."
            )));
        }
    } else {
        let active = config.active_displays();
        if !active.is_empty() {
            let mut displays = split_trim(&active);
            let accept_all = displays.len() == 1 && displays[0].is_empty();
            if !accept_all && !contain(&displays, display_name) {
                displays.push(display_name.to_string());
                config.set_active_displays(&join(&displays, ','))?;
            }
        }
    }

    // Add the view to the active view list, only if needed.

    if let Some(env) = non_empty_env(OCIO_ACTIVE_VIEWS_ENVVAR) {
        let views = split_trim(&env);
        let accept_all = views.len() == 1 && views[0].is_empty();
        if !accept_all {
            return Err(Error::msg(format!(
                "Forbidden to add an active view as '{OCIO_ACTIVE_VIEWS_ENVVAR}' controls the active list."
            )));
        }
    } else {
        let active = config.active_views();
        if !active.is_empty() {
            let mut views = split_trim(&active);
            let accept_all = views.len() == 1 && views[0].is_empty();
            if !accept_all && !contain(&views, view_name) {
                views.push(view_name.to_string());
                config.set_active_views(&join(&views, ','))?;
            }
        }
    }
    Ok(())
}

fn remove_active_display_view(
    config: &mut Config,
    display_name: &str,
    view_name: &str,
) -> Result<()> {
    // Remove the display from the active display list only if possible.

    if let Some(env) = non_empty_env(OCIO_ACTIVE_DISPLAYS_ENVVAR) {
        let displays = split_trim(&env);
        let accept_all = displays.len() == 1 && displays[0].is_empty();
        if !accept_all {
            return Err(Error::msg(format!(
                "Forbidden to remove an active display as '{OCIO_ACTIVE_DISPLAYS_ENVVAR}' controls the active list."
            )));
        }
    } else {
        let active = config.active_displays();
        if !active.is_empty() {
            let mut displays = split_trim(&active);
            let accept_all = displays.len() == 1 && displays[0].is_empty();
            if !accept_all && contain(&displays, display_name) {
                // As the display is an active one, not finding it in the list means that it
                // could be removed from the 'active_displays' list.
                let to_remove =
                    !(0..config.num_displays()).any(|i| compare(&config.display(i), display_name));
                if to_remove {
                    remove(&mut displays, display_name);
                    config.set_active_displays(&join(&displays, ','))?;
                }
            }
        }
    }

    // Remove the view from the active view list only if possible.

    if let Some(env) = non_empty_env(OCIO_ACTIVE_VIEWS_ENVVAR) {
        let views = split_trim(&env);
        let accept_all = views.len() == 1 && views[0].is_empty();
        if !accept_all {
            return Err(Error::msg(format!(
                "Forbidden to remove an active view as '{OCIO_ACTIVE_VIEWS_ENVVAR}' controls the active list."
            )));
        }
    } else {
        let active = config.active_views();
        if !active.is_empty() {
            let mut views = split_trim(&active);
            let accept_all = views.len() == 1 && views[0].is_empty();
            if !accept_all && contain(&views, view_name) {
                // As the view is an active one, not finding it in the list means that it could
                // be removed. But the code needs to loop over all the displays i.e. active and
                // inactive ones, before validating the removal.

                // Enable all the displays while looking for the view.
                let saved_active_displays = config.active_displays();
                config.set_active_displays("")?;

                let mut to_remove = true;
                for disp_idx in 0..config.num_displays() {
                    if !to_remove {
                        break;
                    }
                    let disp = config.display(disp_idx);
                    for view_idx in 0..config.num_views(&disp) {
                        if compare(&config.view(&disp, view_idx), view_name) {
                            to_remove = false;
                            break;
                        }
                    }
                }

                config.set_active_displays(&saved_active_displays)?;

                if to_remove {
                    remove(&mut views, view_name);
                    config.set_active_views(&join(&views, ','))?;
                }
            }
        }
    }
    Ok(())
}

fn add_display_view_impl(
    config: &mut Config,
    display_name: &str,
    view_name: &str,
    look_definition: &str,
    color_space: &mut ColorSpace,
    user_transform: FileTransform,
    connection_color_space_name: &str,
) -> Result<()> {
    if display_name.is_empty() {
        return Err(Error::msg("Invalid display name."));
    }
    if view_name.is_empty() {
        return Err(Error::msg("Invalid view name."));
    }

    // Step 1 - Create the color transformation.

    let mut grp = GroupTransform::new();

    // Add the 'reference' to connection color space.
    {
        // Check for an active or inactive color space.
        let connection_cs = config
            .get_color_space(connection_color_space_name)
            .ok_or_else(|| {
                Error::msg(format!(
                    "Connection color space name '{connection_color_space_name}' does not exist."
                ))
            })?;

        if let Some(tr) = connection_cs.transform(ColorSpaceDirection::FromReference) {
            grp.append(tr.clone());
        } else if let Some(tr) = connection_cs.transform(ColorSpaceDirection::ToReference) {
            grp.append(tr.inverted());
        }
    }

    // Add the 'LUT' transform.
    grp.append(user_transform);

    let grp = Transform::Group(grp);
    grp.validate()?;

    // Step 2 - Add active display and view.

    add_active_display_view(config, display_name, view_name)?;

    // Step 3 - Add the color space to the config.

    color_space.set_transform(Some(grp), ColorSpaceDirection::FromReference);
    config.add_color_space(color_space)?;

    // Step 4 - Add a new active (display, view) pair.

    config.add_display_view(display_name, view_name, color_space.name(), look_definition)
}

/// Add a new (display, view) pair and its new color space to a config
/// (`DisplayViewHelpers::AddDisplayView`). The input of the user transform
/// (a file) must be in the connection color space. The look definition,
/// color space family, description and categories may be empty. Categories
/// are only added if they are already used by the config.
#[allow(clippy::too_many_arguments)]
pub fn add_display_view(
    config: &mut Config,
    display_name: &str,
    view_name: &str,
    look_definition: &str,
    color_space_name: &str,
    color_space_family: &str,
    color_space_description: &str,
    categories: &str,
    transform_file_path: &str,
    connection_color_space_name: &str,
) -> Result<()> {
    let mut color_space = ColorSpace::default();
    color_space.set_name(color_space_name);
    color_space.set_family(color_space_family);
    color_space.set_description(color_space_description);

    // Check if the name is already a color space or a role name.
    if config.get_color_space(color_space.name()).is_some() {
        return Err(Error::msg(format!(
            "Color space name '{}' already exists.",
            color_space.name()
        )));
    }

    // Add categories if any.
    if !categories.is_empty() {
        let cats = extract_items(categories);
        // Only add the categories if already used.
        if !find_color_space_names(config, &cats).is_empty() {
            for cat in &cats {
                color_space.add_category(cat);
            }
        }
    }

    let file = FileTransform::new(transform_file_path);

    add_display_view_impl(
        config,
        display_name,
        view_name,
        look_definition,
        &mut color_space,
        file,
        connection_color_space_name,
    )
}

/// Remove a (display, view) pair and its color space if not used anymore
/// (`DisplayViewHelpers::RemoveDisplayView`). The view is always removed but
/// the display is only removed if empty.
pub fn remove_display_view(config: &mut Config, display_name: &str, view_name: &str) -> Result<()> {
    let name = config
        .display_view_color_space_name(display_name, view_name)
        .to_string();
    let cs_name = if name.is_empty() {
        display_name.to_string()
    } else {
        name
    };
    if cs_name.is_empty() {
        return Err(Error::msg(format!(
            "Missing color space for '{display_name}' and '{view_name}'."
        )));
    }

    // Step 1 - Remove the (display, view) pair.

    config.remove_display_view(display_name, view_name)?;

    // Step 2 - Remove the (display, view) pair from active lists if possible.

    remove_active_display_view(config, display_name, view_name)?;

    // Step 3 - Remove the associated color space if not used.

    if !config.is_color_space_used(&cs_name) {
        config.remove_color_space(&cs_name);
    }
    Ok(())
}
