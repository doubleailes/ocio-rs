//! Config validation (`Config::validate`) and version consistency checks.

use super::utils::compare;
use super::*;

impl Config {
    fn fail(&self, msg: String) -> Error {
        if let Ok(mut v) = self.validation.lock() {
            v.1 = msg.clone();
        }
        Error::msg(msg)
    }

    fn validate_view(
        &self,
        display: &str,
        view: &View,
        check_use_display_name: bool,
    ) -> Result<()> {
        if view.name.is_empty() {
            return Err(self.fail(prefix_error_msg(display, view)));
        }
        let shared_with_vt = display.is_empty() && !view.view_transform.is_empty();
        if view.colorspace.is_empty() {
            return Err(self.fail(format!(
                "{}does not refer to a color space.",
                prefix_error_msg(display, view)
            )));
        }
        if check_use_display_name && !shared_with_vt && view.use_display_name_for_colorspace() {
            return Err(self.fail(format!(
                "{}can not use '{}' keyword for the color space name.",
                prefix_error_msg(display, view),
                OCIO_VIEW_USE_DISPLAY_NAME
            )));
        }
        if !view.use_display_name_for_colorspace()
            && !self.all_color_spaces.has_color_space(&view.colorspace)
            && self.get_named_transform(&view.colorspace).is_none()
        {
            return Err(self.fail(format!(
                "{}that refers to a color space or a named transform, '{}', which is not defined.",
                prefix_error_msg(display, view),
                view.colorspace
            )));
        }
        if !view.view_transform.is_empty() {
            if self.get_named_transform(&view.view_transform).is_none()
                && self.view_transform(&view.view_transform).is_none()
            {
                return Err(self.fail(format!(
                    "{}that refers to a view transform, '{}', which is neither a view transform nor a named transform.",
                    prefix_error_msg(display, view),
                    view.view_transform
                )));
            }
            let display_cs = if view.use_display_name_for_colorspace() {
                display
            } else {
                view.colorspace.as_str()
            };
            if let Some(cs) = self.get_color_space(display_cs) {
                if cs.reference_space_type() != ReferenceSpaceType::Display {
                    return Err(self.fail(format!(
                        "{}refers to a color space, '{}', that is not a display-referred color space.",
                        prefix_error_msg(display, view),
                        display_cs
                    )));
                }
            }
        }
        let mut looks = LookParseResult::new();
        for option in looks.parse(&view.looks) {
            for token in option {
                if !token.name.is_empty() && self.look(&token.name).is_none() {
                    return Err(self.fail(format!(
                        "{}refers to a look, '{}', which is not defined.",
                        prefix_error_msg(display, view),
                        token.name
                    )));
                }
            }
        }
        if !view.rule.is_empty() && self.viewing_rules.find_rule(&view.rule).is_none() {
            return Err(self.fail(format!(
                "{}refers to a viewing rule, '{}', which is not defined.",
                prefix_error_msg(display, view),
                view.rule
            )));
        }
        Ok(())
    }

    fn validate_shared_view(
        &self,
        display: &str,
        views_of_display: &[View],
        shared_view: &str,
        check_use_display_name: bool,
    ) -> Result<()> {
        if find_view(views_of_display, shared_view).is_some() {
            return Err(self.fail(format!(
                "Config failed view validation. The display '{display}' contains a shared view '{shared_view}' that is already defined as a view."
            )));
        }
        match find_view(&self.shared_views, shared_view) {
            None => Err(self.fail(format!(
                "Config failed view validation. The display '{display}' contains a shared view '{shared_view}' that is not defined."
            ))),
            Some(i) => {
                let view = &self.shared_views[i];
                if check_use_display_name && !view.view_transform.is_empty() && view.use_display_name_for_colorspace() {
                    match self.get_color_space(display) {
                        None => {
                            return Err(self.fail(format!(
                                "Config failed view validation. The display '{}' contains a shared view '{}' which does not define a color space and there is no color space that matches the display name.",
                                display, view.name
                            )))
                        }
                        Some(cs) if cs.reference_space_type() != ReferenceSpaceType::Display => {
                            return Err(self.fail(format!(
                                "Config failed view validation. The display '{}' contains a shared view '{}' that refers to a color space, '{}', that is not a display-referred color space.",
                                display, view.name, display
                            )))
                        }
                        _ => {}
                    }
                }
                Ok(())
            }
        }
    }

    /// Validate the config (the result is cached until the config changes).
    pub fn validate(&self) -> Result<()> {
        {
            let v = self
                .validation
                .lock()
                .map(|v| v.clone())
                .unwrap_or((Validation::Unknown, String::new()));
            match v.0 {
                Validation::Passed => return Ok(()),
                Validation::Failed => return Err(Error::msg(v.1)),
                Validation::Unknown => {}
            }
        }
        if let Ok(mut v) = self.validation.lock() {
            *v = (Validation::Failed, String::new());
        }
        self.validate_impl()?;
        if let Ok(mut v) = self.validation.lock() {
            v.0 = Validation::Passed;
        }
        Ok(())
    }

    fn validate_impl(&self) -> Result<()> {
        // Predefined context variables.
        if self.major_version >= 2
            && self.context.environment_mode() == EnvironmentMode::LoadPredefined
        {
            for (k, v) in &self.env {
                if contains_context_variables(v) {
                    let valid = *v == format!("${k}")
                        || *v == format!("${{{k}}}")
                        || *v == format!("%{k}%");
                    if !valid {
                        return Err(Error::msg(format!(
                            "Unresolved context variable in environment declaration '{k} = {v}'."
                        )));
                    }
                }
            }
        }

        // Color spaces.
        let mut has_display_referred = false;
        let mut has_scene_referred = false;
        for cs in self.all_color_spaces.iter() {
            let name = cs.name();
            if self.major_version >= 2 && contains_context_variable_token(name) {
                return Err(self.fail(format!(
                    "Config failed color space validation. A color space name '{name}' cannot contain a context variable reserved token i.e. % or $."
                )));
            }
            if cs.num_aliases() > 0 && self.major_version < 2 {
                return Err(self.fail(format!(
                    "Config failed color space validation. Aliases may not be used in a v1 config.  Color space name: '{name}'."
                )));
            }
            let interop = cs.interop_id();
            if !interop.is_empty() && self.get_color_space(interop).is_none() {
                return Err(self.fail(format!(
                    "Config failed color space validation. The color space '{name}' refers to an interop ID, '{interop}', which is not a color space name or alias."
                )));
            }
            match cs.reference_space_type() {
                ReferenceSpaceType::Display => has_display_referred = true,
                ReferenceSpaceType::Scene => has_scene_referred = true,
            }
        }

        // Roles.
        for (role, cs) in &self.roles {
            if self.major_version >= 2 && contains_context_variable_token(role) {
                return Err(self.fail(format!(
                    "Config failed role validation. A role name '{role}' cannot contain a context variable reserved token i.e. % or $."
                )));
            }
            if !self.all_color_spaces.has_color_space(cs) {
                return Err(self.fail(format!(
                    "Config failed role validation. The role '{role}' refers to a color space, '{cs}', which is not defined."
                )));
            }
        }
        if self.version_hex() >= 0x0202_0000 {
            let mut scene_linear = false;
            let mut compositing_log = false;
            let mut color_timing = false;
            let mut aces_interchange = false;
            let mut aces_scene = false;
            let mut cie_interchange = false;
            let mut cie_display = false;
            for (role, csname) in &self.roles {
                if compare(role, ROLE_SCENE_LINEAR) {
                    scene_linear = true;
                } else if compare(role, ROLE_COMPOSITING_LOG) {
                    compositing_log = true;
                } else if compare(role, ROLE_COLOR_TIMING) {
                    color_timing = true;
                } else if compare(role, ROLE_INTERCHANGE_SCENE) {
                    aces_interchange = true;
                    aces_scene = self
                        .get_color_space(csname)
                        .map(|c| c.reference_space_type() == ReferenceSpaceType::Scene)
                        .unwrap_or(false);
                } else if compare(role, ROLE_INTERCHANGE_DISPLAY) {
                    cie_interchange = true;
                    cie_display = self
                        .get_color_space(csname)
                        .map(|c| c.reference_space_type() == ReferenceSpaceType::Display)
                        .unwrap_or(false);
                }
            }
            if !scene_linear {
                log_error("The scene_linear role is required for a config version 2.2 or higher.");
            }
            if !compositing_log {
                log_error(
                    "The compositing_log role is required for a config version 2.2 or higher.",
                );
            }
            if !color_timing {
                log_error("The color_timing role is required for a config version 2.2 or higher.");
            }
            if has_scene_referred && !aces_interchange {
                log_error(
                    "The aces_interchange role is required when there are scene-referred color spaces and the config version is 2.2 or higher.",
                );
            } else if aces_interchange && !aces_scene {
                log_error("The aces_interchange role must be a scene-referred color space.");
            }
            if has_display_referred && !cie_interchange {
                log_error(
                    "The cie_xyz_d65_interchange role is required when there are display-referred color spaces and the config version is 2.2 or higher.",
                );
            } else if cie_interchange && !cie_display {
                log_error(
                    "The cie_xyz_d65_interchange role must be a display-referred color space.",
                );
            }
        }

        // Inactive color spaces.
        for name in self.build_inactive_names_list(InactiveType::All) {
            if self.get_color_space(&name).is_none() && self.get_named_transform(&name).is_none() {
                log_info(&format!(
                    "Inactive '{name}' is neither a color space nor a named transform."
                ));
            }
        }

        // Viewing rules.
        let accessor = |n: &str| self.get_color_space(n);
        if let Err(e) = self
            .viewing_rules
            .validate(&accessor, &self.all_color_spaces)
        {
            return Err(self.fail(format!(
                "Config failed validation. Viewing rules failed validation with: {}",
                e.message()
            )));
        }

        // Shared views.
        for view in &self.shared_views {
            self.validate_view("", view, true)?;
        }

        // Displays.
        let mut num_displays = 0;
        for (display, d) in &self.displays {
            if d.views.is_empty() && d.shared_views.is_empty() {
                return Err(self.fail(format!(
                    "Config failed display validation. The display '{display}' does not define any views."
                )));
            }
            num_displays += 1;
            for sv in &d.shared_views {
                self.validate_shared_view(display, &d.views, sv, true)?;
            }
            for view in &d.views {
                self.validate_view(display, view, true)?;
            }
        }
        if num_displays == 0 {
            return Err(self
                .fail("Config failed display validation. No displays are specified.".to_string()));
        }

        // Virtual display.
        if self.major_version >= 2 {
            for sv in &self.virtual_display.shared_views {
                self.validate_shared_view(
                    "virtual_display",
                    &self.virtual_display.views,
                    sv,
                    false,
                )?;
            }
            for view in &self.virtual_display.views {
                self.validate_view("virtual_display", view, false)?;
            }
        }

        // Active displays.
        let displays: Vec<String> = self.displays.iter().map(|d| d.0.clone()).collect();
        if !self.active_displays_env_override.is_empty() {
            let all = self.active_displays_env_override.len() == 1
                && self.active_displays_env_override[0].is_empty();
            if !all {
                let ordered = intersect_case_ignore(&self.active_displays_env_override, &displays);
                if ordered.is_empty() {
                    return Err(self.fail(format!(
                        "The content of the env. variable for the list of active displays [{}] is invalid.",
                        join_string_env_style(&self.active_displays_env_override)
                    )));
                }
                if ordered.len() != self.active_displays_env_override.len() {
                    return Err(self.fail(format!(
                        "The content of the env. variable for the list of active displays [{}] contains invalid display name(s).",
                        join_string_env_style(&self.active_displays_env_override)
                    )));
                }
            }
        } else if !self.active_displays.is_empty() {
            let all = self.active_displays.len() == 1 && self.active_displays[0].is_empty();
            if !all {
                let ordered = intersect_case_ignore(&self.active_displays, &displays);
                if ordered.is_empty() {
                    return Err(self.fail(format!(
                        "The list of active displays [{}] from the config file is invalid.",
                        join_string_env_style(&self.active_displays)
                    )));
                }
                if ordered.len() != self.active_displays.len() {
                    return Err(self.fail(format!(
                        "The list of active displays [{}] from the config file contains invalid display name(s).",
                        join_string_env_style(&self.active_displays)
                    )));
                }
            }
        }

        // Transforms.
        {
            let mut names = BTreeSet::new();
            for t in self.all_internal_transforms() {
                t.validate()?;
                get_color_space_references(&mut names, t, &self.context);
            }
            for name in &names {
                if !self.all_color_spaces.has_color_space(name) {
                    if contains_context_variables(name) {
                        return Err(self.fail(format!(
                            "Config failed transform validation. This config references a color space '{name}' using an unknown context variable."
                        )));
                    }
                    let csname = self.lookup_role(name);
                    if csname.is_empty() {
                        if self.get_named_transform(name).is_none() {
                            return Err(self.fail(format!(
                                "Config failed transform validation. This config references a color space, '{name}', which is not defined."
                            )));
                        }
                    } else if !self.all_color_spaces.has_color_space(csname) {
                        return Err(self.fail(format!(
                            "Config failed transform validation. This config references a color space, '{csname}' (for role '{name}'), which is not defined."
                        )));
                    }
                }
            }
        }

        // Looks.
        for (i, look) in self.looks.iter().enumerate() {
            let name = look.name();
            if name.is_empty() {
                return Err(self.fail(format!(
                    "Config failed Look validation. The look at index '{i}' does not specify a name."
                )));
            }
            let ps = look.process_space();
            if ps.is_empty() {
                return Err(self.fail(format!(
                    "Config failed Look validation. The look '{name}' does not specify a process space."
                )));
            }
            if !self.all_color_spaces.has_color_space(ps) {
                let csname = self.lookup_role(ps);
                if csname.is_empty() {
                    return Err(self.fail(format!(
                        "Config failed Look validation. The look '{name}' specifies a process color space, '{ps}', which is not defined."
                    )));
                } else if !self.all_color_spaces.has_color_space(csname) {
                    return Err(self.fail(format!(
                        "Config failed Look validation. The look '{name}' specifies a process color space, '{csname}' (for role '{ps}'), which is not defined."
                    )));
                }
            }
        }

        // View transforms.
        if !self.view_transforms.is_empty() {
            if self.default_scene_to_display_view_transform().is_none() {
                return Err(self.fail(
                    "Config failed validation. If there are view_transforms, at least one must use the scene reference space."
                        .to_string(),
                ));
            }
        } else if has_display_referred {
            return Err(self.fail(
                "Config failed validation. If there are display-referred color spaces, there must be view_transforms."
                    .to_string(),
            ));
        }
        if !self.default_view_transform.is_empty() {
            let ok = self
                .default_scene_to_display_view_transform()
                .map(|vt| compare(vt.name(), &self.default_view_transform))
                .unwrap_or(false);
            if !ok {
                return Err(self.fail(format!(
                    "Config failed validation. Default view transform is defined as: '{}' but this does not correspond to an existing scene-referred view transform.",
                    self.default_view_transform
                )));
            }
        }

        // File rules.
        if let Err(e) = self.file_rules.validate(self) {
            return Err(self.fail(format!(
                "Config failed validation. File rules failed with: {}",
                e.message()
            )));
        }

        // File transforms.
        {
            let mut files = BTreeSet::new();
            for t in self.all_internal_transforms() {
                get_file_references(&mut files, t);
            }
            if !files.is_empty() {
                let mut found_one = false;
                let mut err = String::from("Config failed search path validation.");
                for idx in 0..self.context.num_search_paths() {
                    let path = self.context.search_path_by_index(idx).unwrap_or("");
                    if path.is_empty() {
                        err.push_str(" The search_path must not be an empty string if there are FileTransforms.");
                        continue;
                    }
                    let resolved = self.context.resolve_string_var(path);
                    if contains_context_variables(&resolved) {
                        err.push_str(&format!("  The search_path '{path}' cannot be resolved"));
                        if path != resolved {
                            err.push_str(&format!(" by '{resolved}'"));
                        }
                        err.push('.');
                        continue;
                    }
                    found_one = true;
                }
                if !found_one {
                    if self.context.num_search_paths() == 0 {
                        err.push_str(
                            " The search_path must not be empty if there are FileTransforms.",
                        );
                    }
                    return Err(self.fail(err));
                }
            }
            for file in &files {
                let resolved = self.context.resolve_string_var(file);
                if resolved.is_empty() || contains_context_variables(&resolved) {
                    let mut msg = String::from(
                        "Config failed validation expanding file transform paths. The file transform source cannot be resolved: '",
                    );
                    if *file != resolved {
                        msg.push_str(&format!("{file}' vs. '{resolved}'."));
                    } else {
                        msg.push_str(&format!("{file}'."));
                    }
                    return Err(self.fail(msg));
                }
            }
        }

        // Named transforms.
        for nt in &self.all_named_transforms {
            let name = nt.name();
            if self.look(name).is_some() {
                return Err(self.fail(format!(
                    "Config failed validation. NamedTransform can't be named '{name}'. This name is already used for a look."
                )));
            }
            if self.view_transform(name).is_some() {
                return Err(self.fail(format!(
                    "Config failed validation. NamedTransform can't be named '{name}'. This name is already used for a view transform."
                )));
            }
        }

        self.check_version_consistency()
    }

    fn check_transform_version(&self, t: &Transform) -> Result<()> {
        let (maj, min) = (self.major_version, self.minor_version);
        let style_in = |style: &str, list: &[&str]| list.iter().any(|s| compare(s, style));
        match t {
            Transform::Builtin(b) => {
                if maj < 2 {
                    bail!("Only config version 2 (or higher) can have BuiltinInTransform.");
                }
                let st = b.style.as_str();
                if maj == 2
                    && min < 1
                    && compare(st, "ACES-LMT - ACES 1.3 Reference Gamut Compression")
                {
                    bail!("Only config version 2.1 (or higher) can have BuiltinTransform style 'ACES-LMT - ACES 1.3 Reference Gamut Compression'.");
                }
                if maj == 2
                    && min < 2
                    && style_in(
                        st,
                        &[
                            "ARRI_LOGC4_to_ACES2065-1",
                            "CURVE - CANON_CLOG2_to_LINEAR",
                            "CURVE - CANON_CLOG3_to_LINEAR",
                        ],
                    )
                {
                    bail!(
                        "Only config version 2.2 (or higher) can have BuiltinTransform style '{}'.",
                        st
                    );
                }
                if maj == 2 && min < 3 && compare(st, "DISPLAY - CIE-XYZ-D65_to_DisplayP3") {
                    bail!("Only config version 2.3 (or higher) can have BuiltinTransform style 'DISPLAY - CIE-XYZ-D65_to_DisplayP3'.");
                }
                const V24: &[&str] = &[
                    "APPLE_LOG_to_ACES2065-1",
                    "CURVE - APPLE_LOG_to_LINEAR",
                    "CURVE - HLG-OETF",
                    "CURVE - HLG-OETF-INVERSE",
                    "DISPLAY - CIE-XYZ-D65_to_DCDM-D65",
                    "DISPLAY - CIE-XYZ-D65_to_ST2084-DCDM-D65",
                    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-REC709_2.0",
                    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-P3-D65_2.0",
                    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-108nit-P3-D65_2.0",
                    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-300nit-P3-D65_2.0",
                    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-500nit-P3-D65_2.0",
                    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-1000nit-P3-D65_2.0",
                    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-2000nit-P3-D65_2.0",
                    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-4000nit-P3-D65_2.0",
                    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-500nit-REC2020_2.0",
                    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-1000nit-REC2020_2.0",
                    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-2000nit-REC2020_2.0",
                    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-4000nit-REC2020_2.0",
                    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-REC709-D60-in-REC709-D65_2.0",
                    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-REC709-D60-in-P3-D65_2.0",
                    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-REC709-D60-in-REC2020-D65_2.0",
                    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-P3-D60-in-P3-D65_2.0",
                    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-P3-D60-in-XYZ-E_2.0",
                    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-108nit-P3-D60-in-P3-D65_2.0",
                    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-300nit-P3-D60-in-XYZ-E_2.0",
                    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-500nit-P3-D60-in-P3-D65_2.0",
                    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-1000nit-P3-D60-in-P3-D65_2.0",
                    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-2000nit-P3-D60-in-P3-D65_2.0",
                    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-4000nit-P3-D60-in-P3-D65_2.0",
                    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-500nit-P3-D60-in-REC2020-D65_2.0",
                    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-1000nit-P3-D60-in-REC2020-D65_2.0",
                    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-2000nit-P3-D60-in-REC2020-D65_2.0",
                    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-4000nit-P3-D60-in-REC2020-D65_2.0",
                    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-500nit-REC2020-D60-in-REC2020-D65_2.0",
                    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-1000nit-REC2020-D60-in-REC2020-D65_2.0",
                    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-2000nit-REC2020-D60-in-REC2020-D65_2.0",
                    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-4000nit-REC2020-D60-in-REC2020-D65_2.0",
                    "DISPLAY - CIE-XYZ-D65_to_DisplayP3-HDR",
                ];
                if maj == 2 && min < 4 && style_in(st, V24) {
                    bail!(
                        "Only config version 2.4 (or higher) can have BuiltinTransform style '{}'.",
                        st
                    );
                }
                const V25: &[&str] = &[
                    "DISPLAY - CIE-XYZ-D65_to_REC.1886-REC.709 - MIRROR NEGS",
                    "DISPLAY - CIE-XYZ-D65_to_REC.1886-REC.2020 - MIRROR NEGS",
                    "DISPLAY - CIE-XYZ-D65_to_G2.2-REC.709 - MIRROR NEGS",
                    "DISPLAY - CIE-XYZ-D65_to_sRGB - MIRROR NEGS",
                    "DISPLAY - CIE-XYZ-D65_to_G2.6-P3-D65 - MIRROR NEGS",
                ];
                if maj == 2 && min < 5 && style_in(st, V25) {
                    bail!(
                        "Only config version 2.5 (or higher) can have BuiltinTransform style '{}'.",
                        st
                    );
                }
                if maj == 2 && min < 6 && compare(st, "APPLE_LOG-APPLEWG_to_ACES2065-1") {
                    bail!(
                        "Only config version 2.6 (or higher) can have BuiltinTransform style '{}'.",
                        st
                    );
                }
            }
            Transform::Cdl(c) => {
                if maj < 2 && c.style != CdlStyle::NoClamp {
                    bail!("Only config version 2 (or higher) can have style for CDLTransform.");
                }
            }
            Transform::DisplayView(_) => {
                if maj < 2 {
                    bail!("Only config version 2 (or higher) can have DisplayViewTransform.");
                }
            }
            Transform::Exponent(e) => {
                if maj < 2 && e.negative_style != NegativeStyle::Clamp {
                    bail!("Config version 1 only supports ExponentTransform clamping negative values.");
                }
            }
            Transform::ExponentWithLinear(_) => {
                if maj < 2 {
                    bail!(
                        "Only config version 2 (or higher) can have ExponentWithLinearTransform."
                    );
                }
            }
            Transform::ExposureContrast(_) => {
                if maj < 2 {
                    bail!("Only config version 2 (or higher) can have ExposureContrastTransform.");
                }
            }
            Transform::File(f) => {
                if maj < 2 {
                    if f.interpolation == Interpolation::Cubic {
                        bail!("Only config version 2 (or higher) can use 'cubic' interpolation with FileTransform.");
                    }
                    if f.cdl_style != CdlStyle::NoClamp {
                        bail!("Only config version 2 (or higher) can use CDL style' for FileTransform.");
                    }
                }
            }
            Transform::FixedFunction(ff) => {
                use FixedFunctionStyle as S;
                let s = ff.style;
                if maj < 2 {
                    bail!("Only config version 2 (or higher) can have FixedFunctionTransform.");
                }
                if maj == 2 && min < 1 && s == S::AcesGamutComp13 {
                    bail!("Only config version 2.1 (or higher) can have FixedFunctionTransform style 'ACES_GAMUT_COMP_13'.");
                }
                if maj == 2
                    && min < 4
                    && matches!(
                        s,
                        S::LinToPq
                            | S::LinToGammaLog
                            | S::LinToDoubleLog
                            | S::AcesOutputTransform20
                            | S::AcesRgbToJmh20
                            | S::AcesTonescaleCompress20
                            | S::AcesGamutCompress20
                    )
                {
                    bail!("Only config version 2.4 (or higher) can have FixedFunctionTransform style '{}'.", s.as_str());
                }
                if maj == 2
                    && min < 5
                    && matches!(s, S::RgbToHsyLin | S::RgbToHsyLog | S::RgbToHsyVid)
                {
                    bail!("Only config version 2.5 (or higher) can have FixedFunctionTransform style '{}'.", s.as_str());
                }
                if maj == 2 && min < 6 && s == S::AcesRgbToHmj20 {
                    bail!("Only config version 2.6 (or higher) can have FixedFunctionTransform style '{}'.", s.as_str());
                }
            }
            Transform::GradingPrimary(_) => {
                if maj < 2 {
                    bail!("Only config version 2 (or higher) can have GradingPrimaryTransform.");
                }
            }
            Transform::GradingRgbCurve(_) => {
                if maj < 2 {
                    bail!("Only config version 2 (or higher) can have GradingRGBCurveTransform.");
                }
            }
            Transform::GradingHueCurve(_) => {
                if maj == 2 && min < 5 {
                    bail!("Only config version 2.5 (or higher) can have GradingHueCurveTransform.");
                }
            }
            Transform::GradingTone(_) => {
                if maj < 2 {
                    bail!("Only config version 2 (or higher) can have GradingToneTransform.");
                }
            }
            Transform::LogAffine(_) => {
                if maj < 2 {
                    bail!("Only config version 2 (or higher) can have LogAffineTransform.");
                }
            }
            Transform::LogCamera(_) => {
                if maj < 2 {
                    bail!("Only config version 2 (or higher) can have LogCameraTransform.");
                }
            }
            Transform::Range(_) => {
                if maj < 2 {
                    bail!("Only config version 2 (or higher) can have RangeTransform.");
                }
            }
            Transform::Group(g) => {
                for c in &g.transforms {
                    self.check_transform_version(c)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Check that no feature newer than the config version is used.
    pub(crate) fn check_version_consistency(&self) -> Result<()> {
        let hex = self.version_hex();
        for t in self.all_internal_transforms() {
            self.check_transform_version(t)?;
        }
        let maj = self.major_version;
        if maj < 2 && self.family_separator != '/' {
            bail!("Only version 2 (or higher) can have a family separator.");
        }
        if maj < 2 && self.file_rules.num_entries() > 2 {
            bail!("Only version 2 (or higher) can have file rules.");
        }
        if maj < 2 && !self.inactive_names_conf.is_empty() {
            bail!("Only version 2 (or higher) can have inactive color spaces.");
        }
        if maj < 2 && self.viewing_rules.num_entries() != 0 {
            bail!("Only version 2 (or higher) can have viewing rules.");
        }
        if maj < 2 {
            if !self.shared_views.is_empty() {
                bail!("Only version 2 (or higher) can have shared views.");
            }
            for (name, d) in &self.displays {
                if !d.shared_views.is_empty() {
                    bail!(
                        "Config failed validation. The display '{}' uses shared views and config version is less than 2.",
                        name
                    );
                }
            }
            if !self.virtual_display.views.is_empty()
                || !self.virtual_display.shared_views.is_empty()
            {
                bail!("Only version 2 (or higher) can have a virtual display.");
            }
        }
        for cs in self.all_color_spaces.iter() {
            if maj < 2 {
                if cs.reference_space_type() == ReferenceSpaceType::Display {
                    bail!("Only version 2 (or higher) can have DisplayColorSpaces.");
                }
                if !cs.interop_id().is_empty() {
                    bail!(
                        "Config failed validation. The color space '{}' has non-empty InteropID and config version is less than 2.0.",
                        cs.name()
                    );
                }
            }
            if hex < 0x0205_0000 && !cs.interchange_attributes().is_empty() {
                bail!(
                    "Config failed validation. The color space '{}' has non-empty interchange attributes and config version is less than 2.5.",
                    cs.name()
                );
            }
        }
        if maj < 2 && (!self.view_transforms.is_empty() || !self.default_view_transform.is_empty())
        {
            bail!("Only version 2 (or higher) can have ViewTransforms.");
        }
        if hex < 0x0205_0000 {
            for vt in &self.view_transforms {
                if !vt.interchange_attributes().is_empty() {
                    bail!(
                        "Config failed validation. The view transform '{}' has non-empty interchange attributes and config version is less than 2.5.",
                        vt.name()
                    );
                }
            }
            for l in &self.looks {
                if !l.interchange_attributes().is_empty() {
                    bail!(
                        "Config failed validation. The look '{}' has non-empty interchange attributes and config version is less than 2.5.",
                        l.name()
                    );
                }
            }
        }
        if maj < 2 && !self.all_named_transforms.is_empty() {
            bail!("Only version 2 (or higher) can have NamedTransforms.");
        }
        Ok(())
    }
}
