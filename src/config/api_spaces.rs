//! Config API for color spaces, roles, looks, view transforms, named
//! transforms and file rules.

use super::utils::{compare, contains_context_variable_token, lower, split};
use super::*;

impl Config {
    // -----------------------------------------------------------------------
    // Color spaces

    /// Active color spaces having the category (all active ones if empty).
    pub fn color_spaces(&self, category: &str) -> ColorSpaceSet {
        let mut res = ColorSpaceSet::new();
        for name in &self.active_color_space_names {
            if let Some(cs) = self.all_color_spaces.color_space(name) {
                if category.is_empty() || cs.has_category(category) {
                    let _ = res.add_color_space(cs);
                }
            }
        }
        res
    }

    /// All the color spaces (active and inactive).
    pub fn all_color_spaces(&self) -> &ColorSpaceSet {
        &self.all_color_spaces
    }

    /// Number of color spaces matching the reference type and visibility.
    pub fn num_color_spaces_filtered(&self, search: SearchReferenceSpaceType, visibility: ColorSpaceVisibility) -> usize {
        match visibility {
            ColorSpaceVisibility::All => {
                if search == SearchReferenceSpaceType::All {
                    return self.all_color_spaces.num_color_spaces();
                }
                self.all_color_spaces.iter().filter(|cs| match_reference_type(search, cs.reference_space_type())).count()
            }
            ColorSpaceVisibility::Active | ColorSpaceVisibility::Inactive => {
                let names = if visibility == ColorSpaceVisibility::Active {
                    &self.active_color_space_names
                } else {
                    &self.inactive_color_space_names
                };
                if search == SearchReferenceSpaceType::All {
                    return names.len();
                }
                names
                    .iter()
                    .filter(|n| {
                        self.get_color_space(n)
                            .map(|cs| match_reference_type(search, cs.reference_space_type()))
                            .unwrap_or(false)
                    })
                    .count()
            }
        }
    }

    /// Name of the color space at `index` among the ones matching the
    /// reference type and visibility (`""` if out of range).
    pub fn color_space_name_by_index_filtered(
        &self,
        search: SearchReferenceSpaceType,
        visibility: ColorSpaceVisibility,
        index: usize,
    ) -> &str {
        match visibility {
            ColorSpaceVisibility::All => {
                if search == SearchReferenceSpaceType::All {
                    return self.all_color_spaces.color_space_name_by_index(index).unwrap_or("");
                }
                self.all_color_spaces
                    .iter()
                    .filter(|cs| match_reference_type(search, cs.reference_space_type()))
                    .nth(index)
                    .map(|cs| cs.name())
                    .unwrap_or("")
            }
            ColorSpaceVisibility::Active | ColorSpaceVisibility::Inactive => {
                let names = if visibility == ColorSpaceVisibility::Active {
                    &self.active_color_space_names
                } else {
                    &self.inactive_color_space_names
                };
                if search == SearchReferenceSpaceType::All {
                    return names.get(index).map(|s| s.as_str()).unwrap_or("");
                }
                names
                    .iter()
                    .filter_map(|n| self.get_color_space(n))
                    .filter(|cs| match_reference_type(search, cs.reference_space_type()))
                    .nth(index)
                    .map(|cs| cs.name())
                    .unwrap_or("")
            }
        }
    }

    /// Number of active color spaces.
    pub fn num_color_spaces(&self) -> usize {
        self.num_color_spaces_filtered(SearchReferenceSpaceType::All, ColorSpaceVisibility::Active)
    }

    /// Name of the active color space at `index` (`""` if out of range).
    pub fn color_space_name_by_index(&self, index: usize) -> &str {
        self.color_space_name_by_index_filtered(SearchReferenceSpaceType::All, ColorSpaceVisibility::Active, index)
    }

    /// Index of a color space (name, alias or role) among the active color
    /// spaces.
    pub fn index_for_color_space(&self, name: &str) -> Option<usize> {
        let cs = self.get_color_space(name)?;
        self.active_color_space_names.iter().position(|n| n == cs.name())
    }

    /// Color space by name, alias or role (searching all color spaces).
    pub fn get_color_space(&self, name: &str) -> Option<&ColorSpace> {
        self.all_color_spaces.color_space(name).or_else(|| {
            let csname = self.lookup_role(name);
            self.all_color_spaces.color_space(csname)
        })
    }

    /// True if a color space (or role) with this name exists.
    pub fn has_color_space(&self, name: &str) -> bool {
        self.get_color_space(name).is_some()
    }

    /// Canonical name of a color space or named transform (name, alias or
    /// role), `""` if not found.
    pub fn canonical_name(&self, name: &str) -> &str {
        if let Some(cs) = self.get_color_space(name) {
            return cs.name();
        }
        if let Some(nt) = self.get_named_transform(name) {
            return nt.name();
        }
        ""
    }

    /// Add a copy of a color space (replacing the one with the same name).
    pub fn add_color_space(&mut self, cs: &ColorSpace) -> Result<()> {
        let name = cs.name();
        if name.is_empty() {
            return Err(Error::msg("Color space must have a non-empty name."));
        }
        if self.has_role(name) {
            return Err(Error::msg(format!(
                "Cannot add '{name}' color space, there is already a role with this name."
            )));
        }
        if let Some(nt) = self.get_named_transform(name) {
            return Err(Error::msg(format!(
                "Cannot add '{}' color space, there is already a named transform using this name as a name or as an alias: '{}'.",
                name,
                nt.name()
            )));
        }
        if self.major_version >= 2 && contains_context_variable_token(name) {
            return Err(Error::msg(format!(
                "A color space name '{name}' cannot contain a context variable reserved token i.e. % or $."
            )));
        }
        for alias in cs.aliases() {
            if self.has_role(alias) {
                return Err(Error::msg(format!(
                    "Cannot add '{name}' color space, it has an alias '{alias}' and there is already a role with this name."
                )));
            }
            if let Some(nt) = self.get_named_transform(alias) {
                return Err(Error::msg(format!(
                    "Cannot add '{}' color space, it has an alias '{}' and there is already a named transform using this name as a name or as an alias: '{}'.",
                    name,
                    alias,
                    nt.name()
                )));
            }
            if contains_context_variable_token(alias) {
                return Err(Error::msg(format!(
                    "Cannot add '{name}' color space, it has an alias '{alias}' that cannot contain a context variable reserved token i.e. % or $."
                )));
            }
        }
        self.all_color_spaces.add_color_space(cs)?;
        self.reset_cache_ids();
        self.refresh_active_color_spaces();
        Ok(())
    }

    /// Remove a color space by name.
    pub fn remove_color_space(&mut self, name: &str) {
        self.all_color_spaces.remove_color_space(name);
        self.reset_cache_ids();
        self.refresh_active_color_spaces();
    }

    /// Remove all the color spaces.
    pub fn clear_color_spaces(&mut self) {
        self.all_color_spaces.clear_color_spaces();
        self.reset_cache_ids();
        self.refresh_active_color_spaces();
    }

    /// True if the color space is used somewhere in the config (transforms,
    /// roles, views, looks, file rules).
    pub fn is_color_space_used(&self, name: &str) -> bool {
        if name.is_empty() {
            return false;
        }
        let mut names = BTreeSet::new();
        for t in self.all_internal_transforms() {
            get_color_space_references(&mut names, t, &self.context);
        }
        if names.iter().any(|n| compare(n, name)) {
            return true;
        }
        if self.roles.values().any(|cs| compare(cs, name)) {
            return true;
        }
        if self.shared_views.iter().any(|v| compare(&v.colorspace, name)) {
            return true;
        }
        for (disp, d) in &self.displays {
            for v in &d.views {
                if compare(self.display_view_color_space_name(disp, &v.name), name) {
                    return true;
                }
            }
            for sv in &d.shared_views {
                if let Some(i) = find_view(&self.shared_views, sv) {
                    let v = &self.shared_views[i];
                    if !v.view_transform.is_empty() && v.use_display_name_for_colorspace() && compare(disp, name) {
                        return true;
                    }
                }
            }
        }
        if self.looks.iter().any(|l| compare(l.process_space(), name)) {
            return true;
        }
        for i in 0..self.file_rules.num_entries() {
            if compare(self.file_rules.color_space(i).unwrap_or(""), name) {
                return true;
            }
        }
        false
    }

    // -----------------------------------------------------------------------
    // Inactive color spaces

    /// Set the list of inactive color spaces (and named transforms).
    pub fn set_inactive_color_spaces(&mut self, list: &str) {
        self.inactive_names_conf = trim(list).to_string();
        self.inactive_names_api = self.inactive_names_conf.clone();
        self.reset_cache_ids();
        self.refresh_active_color_spaces();
    }

    /// The list of inactive color spaces from the config (or API).
    pub fn inactive_color_spaces(&self) -> &str {
        &self.inactive_names_conf
    }

    /// True if the name is in the config list of inactive color spaces.
    pub fn is_inactive_color_space(&self, cs: &str) -> bool {
        self.inactive_names_conf.split(", ").any(|s| compare(cs, s))
    }

    pub(crate) fn build_inactive_names_list(&self, ty: InactiveType) -> Vec<String> {
        let list = if !self.inactive_names_api.is_empty() {
            split(&self.inactive_names_api, ',')
        } else if !self.inactive_names_env.is_empty() {
            split(&self.inactive_names_env, ',')
        } else if !self.inactive_names_conf.is_empty() {
            split(&self.inactive_names_conf, ',')
        } else {
            Vec::new()
        };
        let mut res = Vec::new();
        for v in list {
            let v = trim(&v).to_string();
            match ty {
                InactiveType::ColorSpace => {
                    if let Some(cs) = self.get_color_space(&v) {
                        res.push(cs.name().to_string());
                    }
                }
                InactiveType::NamedTransform => {
                    if let Some(nt) = self.get_named_transform(&v) {
                        res.push(nt.name().to_string());
                    }
                }
                InactiveType::All => res.push(v),
            }
        }
        res
    }

    pub(crate) fn refresh_active_color_spaces(&mut self) {
        self.inactive_color_space_names = self.build_inactive_names_list(InactiveType::ColorSpace);
        self.active_color_space_names = self
            .all_color_spaces
            .iter()
            .filter(|cs| !self.inactive_color_space_names.iter().any(|n| n == cs.name()))
            .map(|cs| cs.name().to_string())
            .collect();
        self.inactive_named_transform_names = self.build_inactive_names_list(InactiveType::NamedTransform);
        self.active_named_transform_names = self
            .all_named_transforms
            .iter()
            .filter(|nt| !self.inactive_named_transform_names.iter().any(|n| n == nt.name()))
            .map(|nt| nt.name().to_string())
            .collect();
    }

    // -----------------------------------------------------------------------
    // Roles

    pub(crate) fn lookup_role(&self, role: &str) -> &str {
        self.roles.get(&lower(role)).map(|s| s.as_str()).unwrap_or("")
    }

    /// Set (or unset with `None`) a role.
    pub fn set_role(&mut self, role: &str, color_space: Option<&str>) -> Result<()> {
        if role.is_empty() {
            return Err(Error::msg("The role name is null."));
        }
        match color_space {
            Some(cs) => {
                if !self.has_role(role) {
                    if self.get_color_space(role).is_some() {
                        return Err(Error::msg(format!(
                            "Cannot add '{role}' role, there is already a color space using this as a name or an alias."
                        )));
                    }
                    if self.get_named_transform(role).is_some() {
                        return Err(Error::msg(format!(
                            "Cannot add '{role}' role, there is already a named transform using this as a name or an alias."
                        )));
                    }
                    if self.major_version >= 2 && contains_context_variable_token(role) {
                        return Err(Error::msg(format!(
                            "Role name '{role}' cannot contain a context variable reserved token i.e. % or $."
                        )));
                    }
                }
                self.roles.insert(lower(role), cs.to_string());
            }
            None => {
                self.roles.remove(&lower(role));
            }
        }
        self.reset_cache_ids();
        Ok(())
    }

    pub fn num_roles(&self) -> usize {
        self.roles.len()
    }

    pub fn has_role(&self, role: &str) -> bool {
        !role.is_empty() && !self.lookup_role(role).is_empty()
    }

    /// Role name at `index` (`""` if out of range).
    pub fn role_name(&self, index: usize) -> &str {
        self.roles.keys().nth(index).map(|s| s.as_str()).unwrap_or("")
    }

    /// Color space of the role at `index` (`""` if out of range).
    pub fn role_color_space_by_index(&self, index: usize) -> &str {
        self.roles.values().nth(index).map(|s| s.as_str()).unwrap_or("")
    }

    /// Color space of a role (`""` if not a role).
    pub fn role_color_space(&self, role: &str) -> &str {
        if role.is_empty() {
            return "";
        }
        self.lookup_role(role)
    }

    // -----------------------------------------------------------------------
    // Luma

    pub fn default_luma_coefs(&self) -> [f64; 3] {
        self.default_luma_coefs
    }

    pub fn set_default_luma_coefs(&mut self, rgb: &[f64; 3]) {
        self.default_luma_coefs = *rgb;
        self.reset_cache_ids();
    }

    // -----------------------------------------------------------------------
    // Looks

    /// Look by name (case insensitive).
    pub fn look(&self, name: &str) -> Option<&Look> {
        let n = lower(name);
        self.looks.iter().find(|l| lower(l.name()) == n)
    }

    pub fn num_looks(&self) -> usize {
        self.looks.len()
    }

    /// Look name at `index` (`""` if out of range).
    pub fn look_name_by_index(&self, index: usize) -> &str {
        self.looks.get(index).map(|l| l.name()).unwrap_or("")
    }

    /// Add a copy of the look (replacing the look with the same name).
    pub fn add_look(&mut self, look: &Look) -> Result<()> {
        if look.name().is_empty() {
            return Err(Error::msg("Cannot addLook with an empty name."));
        }
        let n = lower(look.name());
        match self.looks.iter().position(|l| lower(l.name()) == n) {
            Some(i) => self.looks[i] = look.clone(),
            None => self.looks.push(look.clone()),
        }
        self.reset_cache_ids();
        Ok(())
    }

    pub fn clear_looks(&mut self) {
        self.looks.clear();
        self.reset_cache_ids();
    }

    // -----------------------------------------------------------------------
    // View transforms

    pub fn num_view_transforms(&self) -> usize {
        self.view_transforms.len()
    }

    /// View transform by name (case insensitive).
    pub fn view_transform(&self, name: &str) -> Option<&ViewTransform> {
        let n = lower(name);
        self.view_transforms.iter().find(|v| lower(v.name()) == n)
    }

    /// View transform name at `index` (`""` if out of range).
    pub fn view_transform_name_by_index(&self, index: usize) -> &str {
        self.view_transforms.get(index).map(|v| v.name()).unwrap_or("")
    }

    /// Add a copy of a view transform (replacing the one with the same name).
    pub fn add_view_transform(&mut self, vt: &ViewTransform) -> Result<()> {
        let name = vt.name();
        if name.is_empty() {
            return Err(Error::msg("Cannot add view transform with an empty name."));
        }
        if vt.transform(ViewTransformDirection::ToReference).is_none()
            && vt.transform(ViewTransformDirection::FromReference).is_none()
        {
            return Err(Error::msg(format!("Cannot add view transform '{name}' with no transform.")));
        }
        let n = lower(name);
        match self.view_transforms.iter().position(|v| lower(v.name()) == n) {
            Some(i) => self.view_transforms[i] = vt.clone(),
            None => self.view_transforms.push(vt.clone()),
        }
        self.reset_cache_ids();
        Ok(())
    }

    /// The default view transform (or the first scene-referred one).
    pub fn default_scene_to_display_view_transform(&self) -> Option<&ViewTransform> {
        if !self.default_view_transform.is_empty() {
            if let Some(vt) = self.view_transform(&self.default_view_transform) {
                if vt.reference_space_type() == ReferenceSpaceType::Scene {
                    return Some(vt);
                }
            }
        }
        self.view_transforms.iter().find(|v| v.reference_space_type() == ReferenceSpaceType::Scene)
    }

    pub fn default_view_transform_name(&self) -> &str {
        &self.default_view_transform
    }

    pub fn set_default_view_transform_name(&mut self, name: &str) {
        self.default_view_transform = name.to_string();
        self.reset_cache_ids();
    }

    pub fn clear_view_transforms(&mut self) {
        self.view_transforms.clear();
        self.reset_cache_ids();
    }

    // -----------------------------------------------------------------------
    // Named transforms

    fn named_transform_index(&self, name: &str) -> Option<usize> {
        if name.is_empty() {
            return None;
        }
        let s = lower(name);
        self.all_named_transforms
            .iter()
            .position(|nt| lower(nt.name()) == s || nt.aliases().iter().any(|a| lower(a) == s))
    }

    /// Number of named transforms with the given visibility.
    pub fn num_named_transforms_filtered(&self, visibility: NamedTransformVisibility) -> usize {
        match visibility {
            NamedTransformVisibility::All => self.all_named_transforms.len(),
            NamedTransformVisibility::Active => self.active_named_transform_names.len(),
            NamedTransformVisibility::Inactive => self.inactive_named_transform_names.len(),
        }
    }

    /// Named transform name at `index` for the given visibility.
    pub fn named_transform_name_by_index_filtered(&self, visibility: NamedTransformVisibility, index: usize) -> &str {
        match visibility {
            NamedTransformVisibility::All => self.all_named_transforms.get(index).map(|n| n.name()).unwrap_or(""),
            NamedTransformVisibility::Active => {
                self.active_named_transform_names.get(index).map(|s| s.as_str()).unwrap_or("")
            }
            NamedTransformVisibility::Inactive => {
                self.inactive_named_transform_names.get(index).map(|s| s.as_str()).unwrap_or("")
            }
        }
    }

    /// Number of active named transforms.
    pub fn num_named_transforms(&self) -> usize {
        self.num_named_transforms_filtered(NamedTransformVisibility::Active)
    }

    /// Name of the active named transform at `index`.
    pub fn named_transform_name_by_index(&self, index: usize) -> &str {
        self.named_transform_name_by_index_filtered(NamedTransformVisibility::Active, index)
    }

    /// Index of a named transform among the active ones.
    pub fn index_for_named_transform(&self, name: &str) -> Option<usize> {
        let nt = self.get_named_transform(name)?;
        self.active_named_transform_names.iter().position(|n| n == nt.name())
    }

    /// Named transform by name or alias (all named transforms).
    pub fn get_named_transform(&self, name: &str) -> Option<&NamedTransform> {
        self.named_transform_index(name).map(|i| &self.all_named_transforms[i])
    }

    /// Add a copy of a named transform (replacing the one with the same name).
    pub fn add_named_transform(&mut self, nt: &NamedTransform) -> Result<()> {
        let name = nt.name().to_string();
        if name.is_empty() {
            return Err(Error::msg("Named transform must have a non-empty name."));
        }
        if nt.transform(TransformDirection::Forward).is_none() && nt.transform(TransformDirection::Inverse).is_none() {
            return Err(Error::msg("Named transform must define at least one transform."));
        }
        if self.has_role(&name) {
            return Err(Error::msg(format!(
                "Cannot add '{name}' named transform, there is already a role with this name."
            )));
        }
        if let Some(cs) = self.get_color_space(&name) {
            return Err(Error::msg(format!(
                "Cannot add '{}' named transform, there is already a color space using this name as a name or as an alias: '{}'.",
                name,
                cs.name()
            )));
        }
        if contains_context_variable_token(&name) {
            return Err(Error::msg(format!(
                "A named transform name '{name}' cannot contain a context variable reserved token i.e. % or $."
            )));
        }
        let mut replace = None;
        if let Some(existing) = self.named_transform_index(&name) {
            let existing_name = self.all_named_transforms[existing].name();
            if !compare(existing_name, &name) {
                return Err(Error::msg(format!(
                    "Cannot add '{name}' named transform, existing named transform, '{existing_name}' is using this name as an alias."
                )));
            }
            replace = Some(existing);
        }
        for alias in nt.aliases() {
            if self.has_role(alias) {
                return Err(Error::msg(format!(
                    "Cannot add '{name}' named transform, it has an alias '{alias}' and there is already a role with this name."
                )));
            }
            if let Some(cs) = self.get_color_space(alias) {
                return Err(Error::msg(format!(
                    "Cannot add '{}' named transform, it has an alias '{}' and there is already a color space using this name as a name or as an alias: '{}'.",
                    name,
                    alias,
                    cs.name()
                )));
            }
            if contains_context_variable_token(alias) {
                return Err(Error::msg(format!(
                    "Cannot add '{name}' named transform, it has an alias '{alias}' that cannot contain a context variable reserved token i.e. % or $."
                )));
            }
            if let Some(existing) = self.named_transform_index(alias) {
                if Some(existing) != replace {
                    return Err(Error::msg(format!(
                        "Cannot add '{}' named transform, it has '{}' alias and existing named transform, '{}' is using the same alias.",
                        name,
                        alias,
                        self.all_named_transforms[existing].name()
                    )));
                }
            }
        }
        match replace {
            Some(i) => self.all_named_transforms[i] = nt.clone(),
            None => self.all_named_transforms.push(nt.clone()),
        }
        self.reset_cache_ids();
        self.refresh_active_color_spaces();
        Ok(())
    }

    /// Remove a named transform by name.
    pub fn remove_named_transform(&mut self, name: &str) {
        let n = lower(name);
        if n.is_empty() {
            return;
        }
        if let Some(i) = self.all_named_transforms.iter().position(|nt| lower(nt.name()) == n) {
            self.all_named_transforms.remove(i);
            // Note: as in OCIO, the caches are not refreshed here.
            return;
        }
        self.reset_cache_ids();
        self.refresh_active_color_spaces();
    }

    pub fn clear_named_transforms(&mut self) {
        self.all_named_transforms.clear();
        self.reset_cache_ids();
        self.refresh_active_color_spaces();
    }

    // -----------------------------------------------------------------------
    // File rules

    pub fn file_rules(&self) -> &FileRules {
        &self.file_rules
    }

    /// Replace the file rules.
    pub fn set_file_rules(&mut self, rules: &FileRules) {
        self.file_rules = rules.clone();
        self.reset_cache_ids();
    }

    /// Color space of a file according to the file rules.
    pub fn color_space_from_filepath(&self, path: &str) -> String {
        self.file_rules.color_space_from_filepath(self, path).0
    }

    /// Color space of a file according to the file rules and the index of
    /// the matching rule.
    pub fn color_space_from_filepath_with_index(&self, path: &str) -> (String, usize) {
        self.file_rules.color_space_from_filepath(self, path)
    }

    /// True if only the default rule matches the path.
    pub fn filepath_only_matches_default_rule(&self, path: &str) -> bool {
        self.file_rules.filepath_only_matches_default_rule(self, path)
    }

    /// Deprecated v1 behavior: right-most color space name found in the
    /// string (falls back to the default role if strict parsing is off).
    pub fn parse_color_space_from_string(&self, s: &str) -> &str {
        if let Some(idx) = file_rules::parse_color_space_from_string(self, s) {
            return self.all_color_spaces.color_space_name_by_index(idx).unwrap_or("");
        }
        if !self.strict_parsing {
            let csname = self.lookup_role(ROLE_DEFAULT);
            if !csname.is_empty() {
                if let Some(i) = self.all_color_spaces.color_space_index(csname) {
                    return self.all_color_spaces.color_space_name_by_index(i).unwrap_or("");
                }
            }
        }
        ""
    }

    pub fn is_strict_parsing_enabled(&self) -> bool {
        self.strict_parsing
    }

    pub fn set_strict_parsing_enabled(&mut self, enabled: bool) {
        self.strict_parsing = enabled;
        self.reset_cache_ids();
    }

    // -----------------------------------------------------------------------
    // Internal transforms

    /// All the transforms of color spaces, looks, view transforms and named
    /// transforms.
    pub(crate) fn all_internal_transforms(&self) -> Vec<&Transform> {
        let mut v = Vec::new();
        for cs in self.all_color_spaces.iter() {
            v.extend(cs.transform(ColorSpaceDirection::ToReference));
            v.extend(cs.transform(ColorSpaceDirection::FromReference));
        }
        for l in &self.looks {
            v.extend(l.transform());
            v.extend(l.inverse_transform());
        }
        for vt in &self.view_transforms {
            v.extend(vt.transform(ViewTransformDirection::ToReference));
            v.extend(vt.transform(ViewTransformDirection::FromReference));
        }
        for nt in &self.all_named_transforms {
            v.extend(nt.transform(TransformDirection::Forward));
            v.extend(nt.transform(TransformDirection::Inverse));
        }
        v
    }
}
