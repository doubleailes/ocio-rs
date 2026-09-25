//! Displays and views (port of `Display.cpp`).

use super::utils::{compare, intersect_case_ignore};
use crate::types::OCIO_VIEW_USE_DISPLAY_NAME;

/// A view of a display (or a shared view).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct View {
    pub name: String,
    /// Might be empty.
    pub view_transform: String,
    /// The color space, or the display color space when a view transform is used.
    pub colorspace: String,
    /// Might be empty.
    pub looks: String,
    /// Might be empty.
    pub rule: String,
    /// Might be empty.
    pub description: String,
}

impl View {
    /// Build a view.
    pub fn new(
        name: &str,
        view_transform: &str,
        colorspace: &str,
        looks: &str,
        rule: &str,
        description: &str,
    ) -> Self {
        Self {
            name: name.to_string(),
            view_transform: view_transform.to_string(),
            colorspace: colorspace.to_string(),
            looks: looks.to_string(),
            rule: rule.to_string(),
            description: description.to_string(),
        }
    }

    /// True if `csname` is `<USE_DISPLAY_NAME>` (case insensitive).
    pub fn use_display_name(csname: &str) -> bool {
        compare(csname, OCIO_VIEW_USE_DISPLAY_NAME)
    }

    /// True if the color space is `<USE_DISPLAY_NAME>`.
    pub fn use_display_name_for_colorspace(&self) -> bool {
        Self::use_display_name(&self.colorspace)
    }
}

/// Find a view by name (case insensitive).
pub fn find_view(views: &[View], name: &str) -> Option<usize> {
    views.iter().position(|v| compare(&v.name, name))
}

/// Add a view or update the existing view with the same name.
pub fn add_view(
    views: &mut Vec<View>,
    name: &str,
    view_transform: &str,
    display_color_space: &str,
    looks: &str,
    rule: &str,
    description: &str,
) {
    let cs = if View::use_display_name(display_color_space) {
        OCIO_VIEW_USE_DISPLAY_NAME
    } else {
        display_color_space
    };
    match find_view(views, name) {
        None => views.push(View::new(
            name,
            view_transform,
            cs,
            looks,
            rule,
            description,
        )),
        Some(i) => {
            let v = &mut views[i];
            v.view_transform = view_transform.to_string();
            v.colorspace = cs.to_string();
            v.looks = looks.to_string();
            v.rule = rule.to_string();
            v.description = description.to_string();
        }
    }
}

/// A display: its own views and references to shared views.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Display {
    /// Displays instantiated from the virtual display are not saved.
    pub temporary: bool,
    /// Views defined by the display.
    pub views: Vec<View>,
    /// Names of the config shared views used by the display.
    pub shared_views: Vec<String>,
}

/// Ordered list of (display name, display).
pub type DisplayMap = Vec<(String, Display)>;

/// Find a display by name (case insensitive).
pub fn find_display(displays: &DisplayMap, name: &str) -> Option<usize> {
    displays.iter().position(|(n, _)| compare(n, name))
}

/// Compute the list of active displays (`ComputeDisplays`).
pub fn compute_displays(
    displays: &DisplayMap,
    active: &[String],
    active_env_override: &[String],
) -> Vec<String> {
    let master: Vec<String> = displays.iter().map(|(n, _)| n.clone()).collect();
    if !active_env_override.is_empty() {
        let r = intersect_case_ignore(active_env_override, &master);
        if !r.is_empty() {
            return r;
        }
    } else if !active.is_empty() {
        let r = intersect_case_ignore(active, &master);
        if !r.is_empty() {
            return r;
        }
    }
    master
}
