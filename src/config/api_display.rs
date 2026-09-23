//! Config API for displays, views, shared views, the virtual display,
//! active displays / views and viewing rules.

use super::utils::{compare, lower, remove};
use super::*;

impl Config {
    fn view_ptrs<'a>(&'a self, display: &'a Display) -> Vec<&'a View> {
        let mut v: Vec<&View> = display.views.iter().collect();
        for s in &display.shared_views {
            if let Some(i) = find_view(&self.shared_views, s) {
                v.push(&self.shared_views[i]);
            }
        }
        v
    }

    pub(crate) fn find_view_def(&self, display: &str, view: &str) -> Option<&View> {
        if view.is_empty() {
            return None;
        }
        let mut search_shared = display.is_empty();
        let mut disp = None;
        if !search_shared {
            let i = find_display(&self.displays, display)?;
            let d = &self.displays[i].1;
            search_shared = contain(&d.shared_views, view);
            disp = Some(d);
        }
        let views = if search_shared { &self.shared_views } else { &disp?.views };
        find_view(views, view).map(|i| &views[i])
    }

    /// Filter / reorder views using the active views (if any).
    fn active_views_of(&self, views: &[String]) -> Vec<String> {
        let mut active = Vec::new();
        if !self.active_views_env_override.is_empty() {
            let o = intersect_case_ignore(&self.active_views_env_override, views);
            if !o.is_empty() {
                active = o;
            }
        } else if !self.active_views.is_empty() {
            let o = intersect_case_ignore(&self.active_views, views);
            if !o.is_empty() {
                active = o;
            }
        }
        if active.is_empty() {
            active = views.to_vec();
        }
        active
    }

    fn filtered_views(&self, views: &[&View], image_cs: &str) -> Result<(Vec<String>, Vec<String>)> {
        let cs = self
            .get_color_space(image_cs)
            .ok_or_else(|| Error::msg(format!("Could not find source color space '{image_cs}'.")))?;
        let encoding = cs.encoding().to_string();
        let names: Vec<String> = views.iter().map(|v| v.name.clone()).collect();
        let active = self.active_views_of(&names);
        let image_name = lower(image_cs);
        let mut filtered = Vec::new();
        for view in &active {
            let idx = match find_in_vec_case_ignore(&names, view) {
                Some(i) => i,
                None => continue,
            };
            let rule_name = &views[idx].rule;
            if rule_name.is_empty() {
                filtered.push(view.clone());
            } else if let Some(ri) = self.viewing_rules.find_rule(rule_name) {
                let num_cs = self.viewing_rules.num_color_spaces(ri).unwrap_or(0);
                let mut added = false;
                for ci in 0..num_cs {
                    let rolename = self.viewing_rules.color_space(ri, ci).unwrap_or("");
                    let csname = self.lookup_role(rolename);
                    let n = if csname.is_empty() { rolename } else { csname };
                    if lower(n) == image_name {
                        filtered.push(view.clone());
                        added = true;
                        break;
                    }
                }
                if !added && !encoding.is_empty() {
                    let num_enc = self.viewing_rules.num_encodings(ri).unwrap_or(0);
                    for ei in 0..num_enc {
                        let enc = self.viewing_rules.encoding(ri, ei).unwrap_or("");
                        if lower(enc) == encoding {
                            filtered.push(view.clone());
                            break;
                        }
                    }
                }
            }
        }
        Ok((names, filtered))
    }

    fn display_cache(&self) -> Vec<String> {
        compute_displays(&self.displays, &self.active_displays, &self.active_displays_env_override)
    }

    // -----------------------------------------------------------------------
    // Viewing rules

    pub fn viewing_rules(&self) -> &ViewingRules {
        &self.viewing_rules
    }

    pub fn set_viewing_rules(&mut self, rules: &ViewingRules) {
        self.viewing_rules = rules.clone();
        self.reset_cache_ids();
    }

    // -----------------------------------------------------------------------
    // Shared views

    /// True if the view is a shared view of the display (or a config shared
    /// view if `display` is empty).
    pub fn is_view_shared(&self, display: &str, view: &str) -> bool {
        if view.is_empty() {
            return false;
        }
        let n = self.num_views_by_type(ViewType::Shared, display);
        (0..n).any(|i| {
            let s = self.view_by_type(ViewType::Shared, display, i);
            !s.is_empty() && compare(s, view)
        })
    }

    /// Add (or replace) a shared view.
    pub fn add_shared_view(
        &mut self,
        view: &str,
        view_transform: &str,
        color_space: &str,
        looks: &str,
        rule: &str,
        description: &str,
    ) -> Result<()> {
        if view.is_empty() {
            return Err(Error::msg(
                "Shared view could not be added to config, view name has to be a non-empty name.",
            ));
        }
        if color_space.is_empty() {
            return Err(Error::msg(
                "Shared view could not be added to config, color space name has to be a non-empty name.",
            ));
        }
        add_view(&mut self.shared_views, view, view_transform, color_space, looks, rule, description);
        self.reset_cache_ids();
        Ok(())
    }

    /// Remove a shared view.
    pub fn remove_shared_view(&mut self, view: &str) -> Result<()> {
        if view.is_empty() {
            return Err(Error::msg(
                "Shared view could not be removed from config, view name has to be a non-empty name.",
            ));
        }
        match find_view(&self.shared_views, view) {
            Some(i) => {
                self.shared_views.remove(i);
                self.reset_cache_ids();
                Ok(())
            }
            None => Err(Error::msg(format!(
                "Shared view could not be removed from config. A shared view named '{view}' could not be found."
            ))),
        }
    }

    /// Remove all the shared views.
    pub fn clear_shared_views(&mut self) {
        let n = self.num_views_by_type(ViewType::Shared, "");
        for v in (0..n).rev() {
            let name = self.view_by_type(ViewType::Shared, "", v).to_string();
            if !name.is_empty() {
                let _ = self.remove_shared_view(&name);
            }
        }
    }

    // -----------------------------------------------------------------------
    // Displays and views

    /// The first active display.
    pub fn default_display(&self) -> String {
        self.display(0)
    }

    /// Number of active displays.
    pub fn num_displays(&self) -> usize {
        self.display_cache().len()
    }

    /// Active display at `index` (`""` if out of range).
    pub fn display(&self, index: usize) -> String {
        self.display_cache().get(index).cloned().unwrap_or_default()
    }

    /// The first active view of the display.
    pub fn default_view(&self, display: &str) -> String {
        self.view(display, 0)
    }

    /// The first view of the display for an image in `color_space` (see
    /// viewing rules).
    pub fn default_view_for_color_space(&self, display: &str, color_space: &str) -> Result<String> {
        self.view_for_color_space(display, color_space, 0)
    }

    /// Number of active views of the display.
    pub fn num_views(&self, display: &str) -> usize {
        if display.is_empty() {
            return 0;
        }
        let i = match find_display(&self.displays, display) {
            Some(i) => i,
            None => return 0,
        };
        let views = self.view_ptrs(&self.displays[i].1);
        let names: Vec<String> = views.iter().map(|v| v.name.clone()).collect();
        self.active_views_of(&names).len()
    }

    /// Active view of the display at `index` (`""` if out of range).
    pub fn view(&self, display: &str, index: usize) -> String {
        if display.is_empty() {
            return String::new();
        }
        let i = match find_display(&self.displays, display) {
            Some(i) => i,
            None => return String::new(),
        };
        let views = self.view_ptrs(&self.displays[i].1);
        let names: Vec<String> = views.iter().map(|v| v.name.clone()).collect();
        let active = self.active_views_of(&names);
        match active.get(index).and_then(|a| find_in_vec_case_ignore(&names, a)) {
            Some(idx) if idx < views.len() => views[idx].name.clone(),
            _ => String::new(),
        }
    }

    /// Number of active views of the display usable with an image in
    /// `color_space` (see viewing rules).
    pub fn num_views_for_color_space(&self, display: &str, color_space: &str) -> Result<usize> {
        if display.is_empty() || color_space.is_empty() {
            return Ok(0);
        }
        let i = match find_display(&self.displays, display) {
            Some(i) => i,
            None => return Ok(0),
        };
        let views = self.view_ptrs(&self.displays[i].1);
        Ok(self.filtered_views(&views, color_space)?.1.len())
    }

    /// View at `index` of the display usable with an image in `color_space`.
    pub fn view_for_color_space(&self, display: &str, color_space: &str, index: usize) -> Result<String> {
        if display.is_empty() || color_space.is_empty() {
            return Ok(String::new());
        }
        let i = match find_display(&self.displays, display) {
            Some(i) => i,
            None => return Ok(String::new()),
        };
        let views = self.view_ptrs(&self.displays[i].1);
        let (names, filtered) = self.filtered_views(&views, color_space)?;
        let mut idx = Some(index);
        if !filtered.is_empty() {
            match filtered.get(index) {
                None => return Ok(String::new()),
                Some(f) => idx = find_in_vec_case_ignore(&names, f),
            }
        }
        if let Some(i) = idx {
            if i < views.len() {
                return Ok(views[i].name.clone());
            }
        }
        Ok(views.first().map(|v| v.name.clone()).unwrap_or_default())
    }

    /// True if both configs have the (display, view) with the same content
    /// (description excepted).
    pub fn are_views_equal(first: &Config, second: &Config, display: &str, view: &str) -> bool {
        let cs1 = first.display_view_color_space_name(display, view);
        let cs2 = second.display_view_color_space_name(display, view);
        !cs1.is_empty()
            && !cs2.is_empty()
            && compare(cs1, cs2)
            && compare(first.display_view_looks(display, view), second.display_view_looks(display, view))
            && compare(
                first.display_view_transform_name(display, view),
                second.display_view_transform_name(display, view),
            )
            && compare(first.display_view_rule(display, view), second.display_view_rule(display, view))
    }

    /// View transform of the (display, view); an empty display means a
    /// config shared view.
    pub fn display_view_transform_name(&self, display: &str, view: &str) -> &str {
        self.find_view_def(display, view).map(|v| v.view_transform.as_str()).unwrap_or("")
    }

    /// Color space of the (display, view).
    pub fn display_view_color_space_name(&self, display: &str, view: &str) -> &str {
        self.find_view_def(display, view).map(|v| v.colorspace.as_str()).unwrap_or("")
    }

    /// Looks of the (display, view).
    pub fn display_view_looks(&self, display: &str, view: &str) -> &str {
        self.find_view_def(display, view).map(|v| v.looks.as_str()).unwrap_or("")
    }

    /// Viewing rule of the (display, view).
    pub fn display_view_rule(&self, display: &str, view: &str) -> &str {
        self.find_view_def(display, view).map(|v| v.rule.as_str()).unwrap_or("")
    }

    /// Description of the (display, view).
    pub fn display_view_description(&self, display: &str, view: &str) -> &str {
        self.find_view_def(display, view).map(|v| v.description.as_str()).unwrap_or("")
    }

    /// True if the (display, view) exists (active or not).
    pub fn has_view(&self, display: &str, view: &str) -> bool {
        !self.display_view_color_space_name(display, view).is_empty()
    }

    /// Add a reference to a shared view to a display.
    pub fn add_display_shared_view(&mut self, display: &str, shared_view: &str) -> Result<()> {
        if display.is_empty() {
            return Err(Error::msg(
                "Shared view could not be added to display: non-empty display name is needed.",
            ));
        }
        if shared_view.is_empty() {
            return Err(Error::msg("Shared view could not be added to display: non-empty view name is needed."));
        }
        let i = match find_display(&self.displays, display) {
            Some(i) => i,
            None => {
                self.displays.push((display.to_string(), Display::default()));
                self.displays.len() - 1
            }
        };
        let d = &mut self.displays[i].1;
        if find_view(&d.views, shared_view).is_some() {
            return Err(Error::msg(format!(
                "There is already a view named '{shared_view}' in the display '{display}'."
            )));
        }
        if contain(&d.shared_views, shared_view) {
            return Err(Error::msg(format!(
                "There is already a shared view named '{shared_view}' in the display '{display}'."
            )));
        }
        d.shared_views.push(shared_view.to_string());
        self.reset_cache_ids();
        Ok(())
    }

    /// Add (or replace) a view of a display using a color space.
    pub fn add_display_view(&mut self, display: &str, view: &str, color_space: &str, looks: &str) -> Result<()> {
        self.add_display_view_full(display, view, "", color_space, looks, "", "")
    }

    /// Add (or replace) a view of a display.
    #[allow(clippy::too_many_arguments)]
    pub fn add_display_view_full(
        &mut self,
        display: &str,
        view: &str,
        view_transform: &str,
        color_space: &str,
        looks: &str,
        rule: &str,
        description: &str,
    ) -> Result<()> {
        if display.is_empty() {
            return Err(Error::msg(
                "View could not be added to display in config: a non-empty display name is needed.",
            ));
        }
        if view.is_empty() {
            return Err(Error::msg("View could not be added to display in config: a non-empty view name is needed."));
        }
        if color_space.is_empty() {
            return Err(Error::msg(
                "View could not be added to display in config: a non-empty color space name is needed.",
            ));
        }
        match find_display(&self.displays, display) {
            None => {
                let d = Display {
                    views: vec![View::new(view, view_transform, color_space, looks, rule, description)],
                    ..Default::default()
                };
                self.displays.push((display.to_string(), d));
            }
            Some(i) => {
                let d = &mut self.displays[i].1;
                if contain(&d.shared_views, view) {
                    return Err(Error::msg(format!(
                        "There is already a shared view named '{view}' in the display '{display}'."
                    )));
                }
                add_view(&mut d.views, view, view_transform, color_space, looks, rule, description);
            }
        }
        self.reset_cache_ids();
        Ok(())
    }

    /// Remove a view (or shared view reference) from a display; the display
    /// is removed when it has no more views.
    pub fn remove_display_view(&mut self, display: &str, view: &str) -> Result<()> {
        if display.is_empty() {
            return Err(Error::msg("Can't remove a view from a display with an empty display name."));
        }
        if view.is_empty() {
            return Err(Error::msg("Can't remove a view from a display with an empty view name."));
        }
        let i = find_display(&self.displays, display).ok_or_else(|| {
            Error::msg(format!("Could not find a display named '{display}' to be removed from config."))
        })?;
        let d = &mut self.displays[i].1;
        if !remove(&mut d.shared_views, view) {
            match find_view(&d.views, view) {
                Some(vi) => {
                    d.views.remove(vi);
                }
                None => {
                    return Err(Error::msg(format!(
                        "Could not find a view named '{view} to be removed from the display named '{display}'."
                    )))
                }
            }
        }
        if d.views.is_empty() && d.shared_views.is_empty() {
            self.displays.remove(i);
        }
        self.reset_cache_ids();
        Ok(())
    }

    /// Remove all the displays.
    pub fn clear_displays(&mut self) {
        self.displays.clear();
        self.reset_cache_ids();
    }

    // -----------------------------------------------------------------------
    // Virtual display

    pub fn has_virtual_view(&self, view: &str) -> bool {
        !self.virtual_display_view_color_space_name(view).is_empty()
    }

    pub fn is_virtual_view_shared(&self, view: &str) -> bool {
        !view.is_empty() && self.virtual_display.shared_views.iter().any(|s| !s.is_empty() && compare(s, view))
    }

    /// Add a view to the virtual display.
    pub fn add_virtual_display_view(
        &mut self,
        view: &str,
        view_transform: &str,
        color_space: &str,
        looks: &str,
        rule: &str,
        description: &str,
    ) -> Result<()> {
        if view.is_empty() {
            return Err(Error::msg(
                "View could not be added to virtual_display in config: a non-empty view name is needed.",
            ));
        }
        if color_space.is_empty() {
            return Err(Error::msg(
                "View could not be added to virtual_display in config: a non-empty color space name is needed.",
            ));
        }
        if find_view(&self.virtual_display.views, view).is_some() {
            return Err(Error::msg(format!(
                "View could not be added to virtual_display in config: View '{view}' already exists."
            )));
        }
        self.virtual_display.views.push(View::new(view, view_transform, color_space, looks, rule, description));
        self.reset_cache_ids();
        Ok(())
    }

    /// Add a shared view reference to the virtual display.
    pub fn add_virtual_display_shared_view(&mut self, shared_view: &str) -> Result<()> {
        if shared_view.is_empty() {
            return Err(Error::msg(
                "Shared view could not be added to virtual_display: non-empty view name is needed.",
            ));
        }
        if contain(&self.virtual_display.shared_views, shared_view) {
            return Err(Error::msg(format!(
                "Shared view could not be added to virtual_display: There is already a shared view named '{shared_view}'."
            )));
        }
        self.virtual_display.shared_views.push(shared_view.to_string());
        self.reset_cache_ids();
        Ok(())
    }

    pub fn virtual_display_num_views(&self, ty: ViewType) -> usize {
        match ty {
            ViewType::DisplayDefined => self.virtual_display.views.len(),
            ViewType::Shared => self.virtual_display.shared_views.len(),
        }
    }

    /// View of the virtual display (`""` if out of range).
    pub fn virtual_display_view(&self, ty: ViewType, index: usize) -> &str {
        match ty {
            ViewType::DisplayDefined => self.virtual_display.views.get(index).map(|v| v.name.as_str()).unwrap_or(""),
            ViewType::Shared => self.virtual_display.shared_views.get(index).map(|s| s.as_str()).unwrap_or(""),
        }
    }

    /// True if both configs have the virtual view with the same content.
    pub fn are_virtual_views_equal(first: &Config, second: &Config, view: &str) -> bool {
        let cs1 = first.virtual_display_view_color_space_name(view);
        let cs2 = second.virtual_display_view_color_space_name(view);
        !cs1.is_empty()
            && !cs2.is_empty()
            && compare(cs1, cs2)
            && compare(first.virtual_display_view_looks(view), second.virtual_display_view_looks(view))
            && compare(
                first.virtual_display_view_transform_name(view),
                second.virtual_display_view_transform_name(view),
            )
            && compare(first.virtual_display_view_rule(view), second.virtual_display_view_rule(view))
    }

    fn virtual_view(&self, view: &str) -> Option<&View> {
        if self.is_virtual_view_shared(view) {
            return self.find_view_def("", view);
        }
        find_view(&self.virtual_display.views, view).map(|i| &self.virtual_display.views[i])
    }

    pub fn virtual_display_view_transform_name(&self, view: &str) -> &str {
        self.virtual_view(view).map(|v| v.view_transform.as_str()).unwrap_or("")
    }
    pub fn virtual_display_view_color_space_name(&self, view: &str) -> &str {
        self.virtual_view(view).map(|v| v.colorspace.as_str()).unwrap_or("")
    }
    pub fn virtual_display_view_looks(&self, view: &str) -> &str {
        self.virtual_view(view).map(|v| v.looks.as_str()).unwrap_or("")
    }
    pub fn virtual_display_view_rule(&self, view: &str) -> &str {
        self.virtual_view(view).map(|v| v.rule.as_str()).unwrap_or("")
    }
    pub fn virtual_display_view_description(&self, view: &str) -> &str {
        self.virtual_view(view).map(|v| v.description.as_str()).unwrap_or("")
    }

    /// Remove a view (or shared view) from the virtual display.
    pub fn remove_virtual_display_view(&mut self, view: &str) {
        if let Some(i) = self.virtual_display.views.iter().position(|v| compare(&v.name, view)) {
            self.virtual_display.views.remove(i);
            self.reset_cache_ids();
            return;
        }
        if remove(&mut self.virtual_display.shared_views, view) {
            self.reset_cache_ids();
        }
    }

    /// Remove all the views of the virtual display.
    pub fn clear_virtual_display(&mut self) {
        self.virtual_display.views.clear();
        self.virtual_display.shared_views.clear();
        self.reset_cache_ids();
    }

    fn instantiate_display(&mut self, monitor_name: &str, description: &str, icc_path: &str) -> Result<usize> {
        if icc_path.is_empty() {
            return Err(Error::msg("The ICC Profile filepath cannot be null."));
        }
        if description.is_empty() {
            return Err(Error::msg("The monitor description cannot be null."));
        }
        if let Ok(env) = std::env::var(OCIO_ACTIVE_DISPLAYS_ENVVAR) {
            return Err(Error::msg(format!(
                "Cannot instantiate a virtual display because the list of active displays is defined by {OCIO_ACTIVE_DISPLAYS_ENVVAR} = {env}."
            )));
        }
        let mut cs_name = description.to_string();
        if !monitor_name.is_empty() {
            cs_name.push_str(&format!(" [{monitor_name}]"));
        }
        cs_name = cs_name.replace(['$', '%'], "_");
        if self.virtual_display.views.is_empty() && self.virtual_display.shared_views.is_empty() {
            return Err(Error::msg("The virtual display information to instantiate a display is missing."));
        }
        let abs_index = match find_display(&self.displays, &cs_name) {
            None => {
                self.displays.push((cs_name.clone(), self.virtual_display.clone()));
                self.displays.len() - 1
            }
            Some(i) => {
                self.displays[i].1 = self.virtual_display.clone();
                i
            }
        };
        let mut cs = ColorSpace::new(ReferenceSpaceType::Display);
        cs.set_name(&cs_name);
        cs.set_description(&format!("Profile description: {description}"));
        cs.set_transform(
            Some(Transform::File(crate::transforms::FileTransform::new(icc_path))),
            ColorSpaceDirection::FromReference,
        );
        cs.set_encoding("sdr-video");
        if let Err(e) = self.all_color_spaces.add_color_space(&cs) {
            if let Some(i) = find_display(&self.displays, &cs_name) {
                self.displays.remove(i);
            }
            self.all_color_spaces.remove_color_space(&cs_name);
            return Err(e);
        }
        if !self.active_displays.is_empty()
            && !self.active_displays[0].is_empty()
            && !contain(&self.active_displays, &cs_name)
        {
            self.active_displays.push(cs_name.clone());
        }
        if !self.active_views.is_empty() && !self.active_views[0].is_empty() {
            let d = self.displays[abs_index].1.clone();
            for v in &d.views {
                if !contain(&self.active_views, &v.name) {
                    self.active_views.push(v.name.clone());
                }
            }
            for v in &d.shared_views {
                if !contain(&self.active_views, v) {
                    self.active_views.push(v.clone());
                }
            }
        }
        self.reset_cache_ids();
        self.refresh_active_color_spaces();
        self.display_cache()
            .iter()
            .position(|d| *d == cs_name)
            .ok_or_else(|| Error::msg("The instantiated display is not active."))
    }

    /// Instantiate a display (and its display color space) from the virtual
    /// display using an ICC profile. Returns the index of the display in the
    /// active displays.
    pub fn instantiate_display_from_icc_profile(&mut self, icc_path: &str) -> Result<usize> {
        if icc_path.is_empty() {
            return Err(Error::msg("The ICC profile filepath cannot be null."));
        }
        let desc = icc::profile_description(icc_path)?;
        self.instantiate_display("", &desc, icc_path)
    }

    /// Instantiate a display from the virtual display using a monitor name.
    ///
    /// The system monitor enumeration is platform specific and not available
    /// in this port: this always fails.
    pub fn instantiate_display_from_monitor_name(&mut self, monitor_name: &str) -> Result<usize> {
        if monitor_name.is_empty() {
            return Err(Error::msg("The system monitor name cannot be null."));
        }
        Err(Error::msg(format!(
            "Could not find the ICC profile of the monitor '{monitor_name}': system monitors are not supported."
        )))
    }

    // -----------------------------------------------------------------------
    // Active displays and views

    /// Set the active displays (comma separated list).
    pub fn set_active_displays(&mut self, displays: &str) -> Result<()> {
        let mut v = split_string_env_style(displays)?;
        if v.len() == 1 && v[0].is_empty() {
            v.clear();
        }
        self.active_displays = v;
        self.reset_cache_ids();
        Ok(())
    }

    /// The active displays as a comma separated list.
    pub fn active_displays(&self) -> String {
        join_string_env_style(&self.active_displays)
    }

    pub fn num_active_displays(&self) -> usize {
        self.active_displays.len()
    }

    /// Active display at `index`.
    pub fn active_display(&self, index: usize) -> Option<&str> {
        self.active_displays.get(index).map(|s| s.as_str())
    }

    pub fn add_active_display(&mut self, display: &str) -> Result<()> {
        if display.is_empty() {
            return Err(Error::msg("Active display could not be added to config, display name was empty"));
        }
        if self.active_displays.iter().any(|d| d == display) {
            return Ok(());
        }
        self.active_displays.push(display.to_string());
        self.reset_cache_ids();
        Ok(())
    }

    pub fn remove_active_display(&mut self, display: &str) -> Result<()> {
        if display.is_empty() {
            return Err(Error::msg("Active display could not be removed from config, display name was empty."));
        }
        match self.active_displays.iter().position(|d| d == display) {
            Some(i) => {
                self.active_displays.remove(i);
            }
            None => {
                return Err(Error::msg(format!(
                    "Active display could not be removed from config, display '{display}' was not found."
                )))
            }
        }
        self.reset_cache_ids();
        Ok(())
    }

    pub fn clear_active_displays(&mut self) {
        self.active_displays.clear();
        self.reset_cache_ids();
    }

    /// Set the active views (comma separated list).
    pub fn set_active_views(&mut self, views: &str) -> Result<()> {
        let mut v = split_string_env_style(views)?;
        if v.len() == 1 && v[0].is_empty() {
            v.clear();
        }
        self.active_views = v;
        self.reset_cache_ids();
        Ok(())
    }

    /// The active views as a comma separated list.
    pub fn active_views(&self) -> String {
        join_string_env_style(&self.active_views)
    }

    pub fn num_active_views(&self) -> usize {
        self.active_views.len()
    }

    pub fn active_view(&self, index: usize) -> Option<&str> {
        self.active_views.get(index).map(|s| s.as_str())
    }

    pub fn add_active_view(&mut self, view: &str) -> Result<()> {
        if view.is_empty() {
            return Err(Error::msg("Active view could not be added to config, view name was empty."));
        }
        if self.active_views.iter().any(|d| d == view) {
            return Ok(());
        }
        self.active_views.push(view.to_string());
        self.reset_cache_ids();
        Ok(())
    }

    pub fn remove_active_view(&mut self, view: &str) -> Result<()> {
        if view.is_empty() {
            return Err(Error::msg("Active view could not be removed from config, view name was empty."));
        }
        match self.active_views.iter().position(|d| d == view) {
            Some(i) => {
                self.active_views.remove(i);
            }
            None => {
                return Err(Error::msg(format!(
                    "Active view could not be removed from config, view '{view}' was not found."
                )))
            }
        }
        self.reset_cache_ids();
        Ok(())
    }

    pub fn clear_active_views(&mut self) {
        self.active_views.clear();
        self.reset_cache_ids();
    }

    // -----------------------------------------------------------------------
    // All displays

    /// Number of displays (active or not).
    pub fn num_displays_all(&self) -> usize {
        self.displays.len()
    }

    /// Display at `index` among all displays (`""` if out of range).
    pub fn display_all(&self, index: usize) -> &str {
        self.displays.get(index).map(|d| d.0.as_str()).unwrap_or("")
    }

    /// Index of a display among all displays (exact name).
    pub fn display_all_by_name(&self, name: &str) -> Option<usize> {
        if name.is_empty() {
            return None;
        }
        self.displays.iter().position(|d| d.0 == name)
    }

    pub fn is_display_temporary(&self, index: usize) -> bool {
        self.displays.get(index).map(|d| d.1.temporary).unwrap_or(false)
    }

    pub fn set_display_temporary(&mut self, index: usize, temporary: bool) {
        if index < self.displays.len() {
            self.displays[index].1.temporary = temporary;
            self.reset_cache_ids();
        }
    }

    /// Number of shared or display-defined views of a display (shared views
    /// of the config if `display` is empty).
    pub fn num_views_by_type(&self, ty: ViewType, display: &str) -> usize {
        if display.is_empty() {
            return self.shared_views.len();
        }
        match find_display(&self.displays, display) {
            None => 0,
            Some(i) => match ty {
                ViewType::Shared => self.displays[i].1.shared_views.len(),
                ViewType::DisplayDefined => self.displays[i].1.views.len(),
            },
        }
    }

    /// View by type and index (`""` if out of range).
    pub fn view_by_type(&self, ty: ViewType, display: &str, index: usize) -> &str {
        if display.is_empty() {
            return self.shared_views.get(index).map(|v| v.name.as_str()).unwrap_or("");
        }
        match find_display(&self.displays, display) {
            None => "",
            Some(i) => match ty {
                ViewType::Shared => self.displays[i].1.shared_views.get(index).map(|s| s.as_str()).unwrap_or(""),
                ViewType::DisplayDefined => self.displays[i].1.views.get(index).map(|v| v.name.as_str()).unwrap_or(""),
            },
        }
    }
}
