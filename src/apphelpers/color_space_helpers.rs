//! Color space menus (port of `apphelpers/ColorSpaceHelpers.cpp`):
//! [`ColorSpaceInfo`], [`ColorSpaceMenuParameters`], [`ColorSpaceMenuHelper`]
//! and [`add_color_space`] (`ColorSpaceHelpers::AddColorSpace`).

use super::category_helpers::{
    extract_items, find_color_space_infos, find_color_space_names, Categories, Encodings, Infos,
};
use crate::config::logging::log_warning;
use crate::config::utils::{compare, contain, split, trim};
use crate::config::{ColorSpace, Config, NamedTransform};
use crate::error::{Error, Result};
use crate::transforms::{FileTransform, GroupTransform, Transform};
use crate::types::{
    ColorSpaceDirection, ColorSpaceVisibility, SearchReferenceSpaceType, OCIO_DISABLE_ALL_CACHES,
    OCIO_USER_CATEGORIES_ENVVAR,
};
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex, OnceLock};

/// A menu entry: a color space, a role or a named transform. The family is
/// split into hierarchy levels using the family separator of the config. The
/// UI name is an alternative name; when not provided it is the same as the
/// name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColorSpaceInfo {
    name: String,
    ui_name: String,
    family: String,
    description: String,
    // Extracted from the family attribute to be used for a hierarchical menu.
    hierarchy_levels: Vec<String>,
}

impl ColorSpaceInfo {
    /// Create an entry (`ColorSpaceInfo::Create`). An empty `ui_name` means
    /// the UI name is the name.
    pub fn new(
        config: &Config,
        name: &str,
        ui_name: &str,
        family: &str,
        description: &str,
    ) -> Self {
        let ui_name = if ui_name.is_empty() { name } else { ui_name };
        let sep = config.family_separator();
        let vals = if sep != '\0' && !family.is_empty() {
            split(family, sep)
        } else {
            vec![family.to_string()]
        };
        let hierarchy_levels = vals
            .iter()
            .map(|v| trim(v).to_string())
            .filter(|v| !v.is_empty())
            .collect();
        Self {
            name: name.to_string(),
            ui_name: ui_name.to_string(),
            family: family.to_string(),
            description: description.to_string(),
            hierarchy_levels,
        }
    }

    /// Entry of a color space.
    pub fn from_color_space(config: &Config, cs: &ColorSpace) -> Self {
        Self::new(config, cs.name(), "", cs.family(), cs.description())
    }

    /// Entry of a named transform.
    pub fn from_named_transform(config: &Config, nt: &NamedTransform) -> Self {
        Self::new(config, nt.name(), "", nt.family(), nt.description())
    }

    /// Entry of a role: the name is the role name and the UI name is
    /// `"<role> (<color space>)"` (`CreateFromRole`). `None` if the role does
    /// not exist.
    pub fn from_role(config: &Config, role: &str, family: &str) -> Option<Self> {
        if !config.has_role(role) {
            return None;
        }
        let cs = config.get_color_space(role)?;
        let ui_name = format!("{} ({})", role, cs.name());
        Some(Self::new(config, role, &ui_name, family, ""))
    }

    /// Entry of a role used alone in a menu: the name is the color space name
    /// and the UI name is `"<role> (<color space>)"`
    /// (`CreateFromSingleRole`). `None` if the role does not exist.
    pub fn from_single_role(config: &Config, role: &str) -> Option<Self> {
        if !config.has_role(role) {
            return None;
        }
        let cs = config.get_color_space(role)?;
        let ui_name = format!("{} ({})", role, cs.name());
        Some(Self::new(config, cs.name(), &ui_name, "", ""))
    }

    /// The name used in the config.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The name to use in the menu UI.
    pub fn ui_name(&self) -> &str {
        &self.ui_name
    }

    /// The description.
    pub fn description(&self) -> &str {
        &self.description
    }

    /// The family.
    pub fn family(&self) -> &str {
        &self.family
    }

    /// Number of hierarchy levels (the family split with the separator).
    pub fn num_hierarchy_levels(&self) -> usize {
        self.hierarchy_levels.len()
    }

    /// Hierarchy level `i` (`""` if out of range).
    pub fn hierarchy_level(&self, i: usize) -> &str {
        self.hierarchy_levels
            .get(i)
            .map(|s| s.as_str())
            .unwrap_or("")
    }
}

/// Parameters controlling which color spaces appear in menus (see the
/// algorithm description of OCIO's `ColorSpaceMenuParameters`):
///
/// 1. If the role is a role of the config, only that color space is used.
/// 2. The color spaces (of the searched reference space type) and / or named
///    transforms are the candidates.
/// 3. They are filtered by the app categories, encodings and user categories
///    (with fall-backs so that the menu is never emptied by the filtering).
/// 4. Roles are appended if requested.
/// 5. Additional color spaces are appended.
#[derive(Debug, Clone)]
pub struct ColorSpaceMenuParameters {
    config: Option<Arc<Config>>,
    role: String,
    app_categories: String,
    user_categories: String,
    encodings: String,
    include_color_spaces: bool,
    include_roles: bool,
    include_named_transforms: bool,
    treat_no_category_as_any: bool,
    color_space_type: SearchReferenceSpaceType,
    additional_color_spaces: Vec<String>,
}

impl ColorSpaceMenuParameters {
    /// Parameters using `config` (`ColorSpaceMenuParameters::Create`).
    pub fn new(config: Arc<Config>) -> Self {
        Self {
            config: Some(config),
            role: String::new(),
            app_categories: String::new(),
            user_categories: String::new(),
            encodings: String::new(),
            include_color_spaces: true,
            include_roles: false,
            include_named_transforms: false,
            treat_no_category_as_any: true,
            color_space_type: SearchReferenceSpaceType::All,
            additional_color_spaces: Vec::new(),
        }
    }

    /// Set the config (required to create a menu helper).
    pub fn set_config(&mut self, config: Arc<Config>) {
        self.config = Some(config);
    }

    /// The config.
    pub fn config(&self) -> Option<&Arc<Config>> {
        self.config.as_ref()
    }

    /// If `role` is a role of the config, the other parameters are ignored and
    /// the menu only contains that role.
    pub fn set_role(&mut self, role: &str) {
        self.role = role.to_string();
    }

    /// The role.
    pub fn role(&self) -> &str {
        &self.role
    }

    /// Include the color spaces (default is true).
    pub fn set_include_color_spaces(&mut self, include: bool) {
        self.include_color_spaces = include;
    }

    /// True if the color spaces are included.
    pub fn include_color_spaces(&self) -> bool {
        self.include_color_spaces
    }

    /// The reference space type of the searched color spaces (no effect on
    /// roles and named transforms).
    pub fn search_reference_space_type(&self) -> SearchReferenceSpaceType {
        self.color_space_type
    }

    /// Restrict the search to a reference space type.
    pub fn set_search_reference_space_type(&mut self, t: SearchReferenceSpaceType) {
        self.color_space_type = t;
    }

    /// Include the named transforms (default is false).
    pub fn set_include_named_transforms(&mut self, include: bool) {
        self.include_named_transforms = include;
    }

    /// True if the named transforms are included.
    pub fn include_named_transforms(&self) -> bool {
        self.include_named_transforms
    }

    /// Treat items without categories as if they had any category (default
    /// is true).
    pub fn set_treat_no_category_as_any(&mut self, value: bool) {
        self.treat_no_category_as_any = value;
    }

    /// True if items without categories match any category.
    pub fn treat_no_category_as_any(&self) -> bool {
        self.treat_no_category_as_any
    }

    /// Comma separated list of app categories.
    pub fn set_app_categories(&mut self, app_categories: &str) {
        self.app_categories = app_categories.to_string();
    }

    /// The app categories.
    pub fn app_categories(&self) -> &str {
        &self.app_categories
    }

    /// Comma separated list of encodings.
    pub fn set_encodings(&mut self, encodings: &str) {
        self.encodings = encodings.to_string();
    }

    /// The encodings.
    pub fn encodings(&self) -> &str {
        &self.encodings
    }

    /// Comma separated list of user categories (overridden by
    /// `$OCIO_USER_CATEGORIES` when set and not empty).
    pub fn set_user_categories(&mut self, user_categories: &str) {
        self.user_categories = user_categories.to_string();
    }

    /// The user categories.
    pub fn user_categories(&self) -> &str {
        &self.user_categories
    }

    /// Include the roles (default is false), with the "Roles" family.
    pub fn set_include_roles(&mut self, include: bool) {
        self.include_roles = include;
    }

    /// True if the roles are included.
    pub fn include_roles(&self) -> bool {
        self.include_roles
    }

    /// Add an additional color space (or named transform, or role) to the
    /// menu. Added only once (case insensitive).
    pub fn add_color_space(&mut self, name: &str) {
        if !name.is_empty() && !contain(&self.additional_color_spaces, name) {
            self.additional_color_spaces.push(name.to_string());
        }
    }

    /// Number of additional color spaces.
    pub fn num_added_color_spaces(&self) -> usize {
        self.additional_color_spaces.len()
    }

    /// Additional color space at `index` (`""` if out of range).
    pub fn added_color_space(&self, index: usize) -> &str {
        self.additional_color_spaces
            .get(index)
            .map(|s| s.as_str())
            .unwrap_or("")
    }

    /// Remove all the additional color spaces.
    pub fn clear_added_color_spaces(&mut self) {
        self.additional_color_spaces.clear();
    }
}

impl fmt::Display for ColorSpaceMenuParameters {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.config {
            Some(c) => write!(f, "config: {}", c.cache_id())?,
            None => write!(f, "config: missing")?,
        }
        if !self.role.is_empty() {
            write!(f, ", role: {}", self.role)?;
        }
        if !self.app_categories.is_empty() {
            write!(f, ", appCategories: {}", self.app_categories)?;
        }
        if !self.user_categories.is_empty() {
            write!(f, ", userCategories: {}", self.user_categories)?;
        }
        if !self.encodings.is_empty() {
            write!(f, ", encodings: {}", self.encodings)?;
        }
        let b = |v: bool| if v { "true" } else { "false" };
        write!(f, ", includeColorSpaces: {}", b(self.include_color_spaces))?;
        write!(f, ", includeRoles: {}", b(self.include_roles))?;
        write!(
            f,
            ", includeNamedTransforms: {}",
            b(self.include_named_transforms)
        )?;
        write!(
            f,
            ", treatNoCategoryAsAny: {}",
            b(self.treat_no_category_as_any)
        )?;
        match self.color_space_type {
            SearchReferenceSpaceType::Scene => write!(f, ", colorSpaceType: scene")?,
            SearchReferenceSpaceType::Display => write!(f, ", colorSpaceType: display")?,
            SearchReferenceSpaceType::All => {}
        }
        let n = self.additional_color_spaces.len();
        if n == 1 {
            write!(f, ", addedSpaces: {}", self.additional_color_spaces[0])?;
        } else if n > 1 {
            write!(
                f,
                ", addedSpaces: [{}]",
                self.additional_color_spaces.join(", ")
            )?;
        }
        Ok(())
    }
}

/// Helper to create menus for the content of a config: color spaces, roles
/// and named transforms, each with a name, a UI name, a description and a
/// family (also exposed as hierarchy levels).
#[derive(Debug)]
pub struct ColorSpaceMenuHelper {
    parameters: ColorSpaceMenuParameters,
    // All the entries, including the additional color spaces.
    entries: Infos,
}

type MenuCache = Mutex<HashMap<String, Arc<ColorSpaceMenuHelper>>>;

fn menu_cache() -> &'static MenuCache {
    static CACHE: OnceLock<MenuCache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn is_cache_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os(OCIO_DISABLE_ALL_CACHES).is_none())
}

impl ColorSpaceMenuHelper {
    /// Create (or get from the cache) the menu helper for the parameters
    /// (`ColorSpaceMenuHelper::Create`).
    pub fn create(p: &ColorSpaceMenuParameters) -> Result<Arc<ColorSpaceMenuHelper>> {
        let config = p
            .config
            .as_ref()
            .ok_or_else(|| Error::msg("ColorSpaceMenuHelper needs parameters with a config."))?;

        if let Err(e) = config.validate() {
            log_warning(&format!(
                "ColorSpaceMenuHelper needs a valid config. Validation warning is: {}",
                e.message()
            ));
        }

        if !p.include_color_spaces && p.include_roles {
            return Err(Error::msg(
                "ColorSpaceMenuHelper needs to include color spaces if roles are included.",
            ));
        }

        // User categories from the environment variable override what is specified by the
        // application.
        let mut parameters = p.clone();
        if let Ok(env) = std::env::var(OCIO_USER_CATEGORIES_ENVVAR) {
            let user = trim(&env);
            if !user.is_empty() {
                parameters.set_user_categories(user);
            }
        }

        if is_cache_enabled() {
            let key = parameters.to_string();
            let mut cache = menu_cache().lock().unwrap_or_else(|e| e.into_inner());
            if let Some(entry) = cache.get(&key) {
                return Ok(entry.clone());
            }
            let entry = Arc::new(ColorSpaceMenuHelper::new(parameters)?);
            cache.insert(key, entry.clone());
            return Ok(entry);
        }

        Ok(Arc::new(ColorSpaceMenuHelper::new(parameters)?))
    }

    fn new(parameters: ColorSpaceMenuParameters) -> Result<Self> {
        let mut helper = Self {
            parameters,
            entries: Vec::new(),
        };
        helper.refresh()?;
        Ok(helper)
    }

    fn refresh(&mut self) -> Result<()> {
        self.entries.clear();
        let params = &self.parameters;
        let config = match &params.config {
            Some(c) => c.clone(),
            None => {
                return Err(Error::msg(
                    "ColorSpaceMenuHelper needs parameters with a config.",
                ))
            }
        };

        // 1) If the role exists, use only that space.
        if !params.role.is_empty() && config.has_role(&params.role) {
            if let Some(info) = ColorSpaceInfo::from_single_role(&config, &params.role) {
                self.entries.push(info);
            }
            return Ok(());
        }

        // 2) & 3) Identify potential menu items and then filter them by category and encoding.
        let app: Categories = extract_items(&params.app_categories);
        let user: Categories = extract_items(&params.user_categories);
        let encodings: Encodings = extract_items(&params.encodings);
        let num_nt = config.num_named_transforms();
        let num_cs =
            config.num_color_spaces_filtered(params.color_space_type, ColorSpaceVisibility::Active);
        if (params.include_color_spaces && num_cs != 0)
            || (params.include_named_transforms && num_nt != 0)
        {
            self.entries = find_color_space_infos(
                &config,
                &app,
                &user,
                params.include_color_spaces,
                params.include_named_transforms,
                params.treat_no_category_as_any,
                &encodings,
                params.color_space_type,
            );
        }

        // 4) Include roles if requested.
        if params.include_roles {
            for idx in 0..config.num_roles() {
                if let Some(info) =
                    ColorSpaceInfo::from_role(&config, config.role_name(idx), "Roles")
                {
                    self.entries.push(info);
                }
            }
        }

        // 5) Add additional color spaces.
        let mut additional: Infos = Vec::new();
        for name in &params.additional_color_spaces {
            let already_there = |n: &str, entries: &Infos, added: &Infos| {
                entries.iter().any(|e| compare(n, e.name()))
                    || added.iter().any(|e| compare(n, e.name()))
            };
            if let Some(cs) = config.get_color_space(name) {
                if !already_there(cs.name(), &self.entries, &additional) {
                    additional.push(ColorSpaceInfo::from_color_space(&config, cs));
                }
            } else if let Some(nt) = config.get_named_transform(name) {
                if !already_there(nt.name(), &self.entries, &additional) {
                    additional.push(ColorSpaceInfo::from_named_transform(&config, nt));
                }
            } else {
                return Err(Error::msg(format!(
                    "Element '{name}' is neither a color space not a named transform."
                )));
            }
        }
        self.entries.extend(additional);
        Ok(())
    }

    /// The parameters used to build the menu.
    pub fn parameters(&self) -> &ColorSpaceMenuParameters {
        &self.parameters
    }

    /// Number of menu entries.
    pub fn num_color_spaces(&self) -> usize {
        self.entries.len()
    }

    /// Config name of the entry (`""` if out of range).
    pub fn name(&self, idx: usize) -> &str {
        self.entries.get(idx).map(|e| e.name()).unwrap_or("")
    }

    /// UI name of the entry (`""` if out of range).
    pub fn ui_name(&self, idx: usize) -> &str {
        self.entries.get(idx).map(|e| e.ui_name()).unwrap_or("")
    }

    /// Index of the entry with this name (case insensitive), `None` if not
    /// found or if `name` is empty.
    pub fn index_from_name(&self, name: &str) -> Option<usize> {
        if name.is_empty() {
            return None;
        }
        self.entries.iter().position(|e| compare(e.name(), name))
    }

    /// Index of the entry with this UI name (case insensitive).
    pub fn index_from_ui_name(&self, name: &str) -> Option<usize> {
        if name.is_empty() {
            return None;
        }
        self.entries.iter().position(|e| compare(e.ui_name(), name))
    }

    /// Description of the entry (`""` if out of range).
    pub fn description(&self, idx: usize) -> &str {
        self.entries.get(idx).map(|e| e.description()).unwrap_or("")
    }

    /// Family of the entry (`""` if out of range).
    pub fn family(&self, idx: usize) -> &str {
        self.entries.get(idx).map(|e| e.family()).unwrap_or("")
    }

    /// Number of hierarchy levels of the entry (0 if out of range).
    pub fn num_hierarchy_levels(&self, idx: usize) -> usize {
        self.entries
            .get(idx)
            .map(|e| e.num_hierarchy_levels())
            .unwrap_or(0)
    }

    /// Hierarchy level `i` of the entry (`""` if out of range).
    pub fn hierarchy_level(&self, idx: usize, i: usize) -> &str {
        self.entries
            .get(idx)
            .map(|e| e.hierarchy_level(i))
            .unwrap_or("")
    }

    /// Name of the entry with this UI name (`""` if not found).
    pub fn name_from_ui_name(&self, ui_name: &str) -> &str {
        if ui_name.is_empty() {
            return "";
        }
        self.entries
            .iter()
            .find(|e| compare(ui_name, e.ui_name()))
            .map(|e| e.name())
            .unwrap_or("")
    }

    /// UI name of the entry with this name (`""` if not found).
    pub fn ui_name_from_name(&self, name: &str) -> &str {
        if name.is_empty() {
            return "";
        }
        self.entries
            .iter()
            .find(|e| compare(name, e.name()))
            .map(|e| e.ui_name())
            .unwrap_or("")
    }
}

impl fmt::Display for ColorSpaceMenuHelper {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.parameters)?;
        let names: Vec<&str> = self.entries.iter().map(|e| e.name()).collect();
        write!(f, ", color spaces = [{}]", names.join(", "))
    }
}

/// Add `color_space` to the config with a `to_reference` transform made of
/// the user transform followed by the connection color space to reference.
fn add_color_space_impl(
    config: &mut Config,
    color_space: &mut ColorSpace,
    user_transform: FileTransform,
    connection_color_space_name: &str,
) -> Result<()> {
    if connection_color_space_name.is_empty() {
        return Err(Error::msg("Invalid connection color space name."));
    }

    // Check for a role and an active or inactive color space.
    if config.get_color_space(color_space.name()).is_some() {
        return Err(Error::msg(format!(
            "Color space name '{}' already exists.",
            color_space.name()
        )));
    }

    // Step 1 - Create the color transformation.

    let mut grp = GroupTransform::new();
    grp.append(user_transform);

    // Check for an active or inactive color space.
    let connection_cs = config
        .get_color_space(connection_color_space_name)
        .ok_or_else(|| {
            Error::msg(format!(
                "Connection color space name '{connection_color_space_name}' does not exist."
            ))
        })?;

    if let Some(tr) = connection_cs.transform(ColorSpaceDirection::ToReference) {
        grp.append(tr.clone());
    } else if let Some(tr) = connection_cs.transform(ColorSpaceDirection::FromReference) {
        grp.append(tr.inverted());
    }

    let grp = Transform::Group(grp);
    grp.validate()?;

    // Step 2 - Add the color space to the config.

    color_space.set_transform(Some(grp), ColorSpaceDirection::ToReference);
    config.add_color_space(color_space)
}

/// Add a new color space to the config: the output of the user transform
/// (a file) must be in the connection color space
/// (`ColorSpaceHelpers::AddColorSpace`).
///
/// If the config does not already use the categories, they are not added
/// since that would change how the existing color spaces show up in menus.
pub fn add_color_space(
    config: &mut Config,
    name: &str,
    transform_file_path: &str,
    categories: &str,
    connection_color_space_name: &str,
) -> Result<()> {
    let info = ColorSpaceInfo::new(config, name, "", "", "");

    let mut color_space = ColorSpace::default();
    color_space.set_name(info.name());
    color_space.set_family(info.family());
    color_space.set_description(info.description());

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
    add_color_space_impl(config, &mut color_space, file, connection_color_space_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apphelpers::tests_data::CATEGORY_TEST_CONFIG;

    #[test]
    fn color_space_info_read_values() {
        let config = Config::create_from_str(CATEGORY_TEST_CONFIG).unwrap();
        config.validate().unwrap();

        // Tests with 'in_1'.
        let cs = config.get_color_space("in_1").unwrap();
        let info = ColorSpaceInfo::from_color_space(&config, cs);
        assert_eq!(cs.name(), "in_1");
        assert_eq!(info.name(), "in_1");
        assert_eq!(cs.family(), "Input / Camera/Acme");
        assert_eq!(info.num_hierarchy_levels(), 3);
        assert_eq!(info.hierarchy_level(0), "Input");
        assert_eq!(info.hierarchy_level(1), "Camera");
        assert_eq!(info.hierarchy_level(2), "Acme");
        assert_eq!(info.family(), cs.family());
        assert_eq!(
            cs.description(),
            "An input color space.\nFor the Acme camera."
        );
        assert_eq!(info.description(), cs.description());

        // Tests with 'lin_1'.
        let cs = config.get_color_space("lin_1").unwrap();
        let info = ColorSpaceInfo::from_color_space(&config, cs);
        assert_eq!(cs.name(), "lin_1");
        assert_eq!(info.name(), "lin_1");
        assert_eq!(cs.family(), "");
        assert_eq!(info.num_hierarchy_levels(), 0);
        assert_eq!(info.family(), "");
        assert_eq!(cs.description(), "");
    }

    #[test]
    fn color_space_info_change_values() {
        let config = Config::create_raw();
        config.validate().unwrap();

        let mut cs = config.get_color_space("raw").unwrap().clone();
        let info = ColorSpaceInfo::from_color_space(&config, &cs);
        assert_eq!(cs.name(), "raw");
        assert_eq!(info.name(), "raw");
        assert_eq!(cs.family(), "raw");
        assert_eq!(info.num_hierarchy_levels(), 1);
        assert_eq!(info.hierarchy_level(0), "raw");
        assert_eq!(info.family(), cs.family());
        assert_eq!(
            cs.description(),
            "A raw color space. Conversions to and from this space are no-ops."
        );
        assert_eq!(info.description(), cs.description());

        // Change the family.
        cs.set_family("");
        assert_eq!(cs.family(), "");
        let info = ColorSpaceInfo::from_color_space(&config, &cs);
        assert_eq!(info.num_hierarchy_levels(), 0);
        assert_eq!(info.family(), "");

        cs.set_family("Acme     /   Camera");
        assert_eq!(cs.family(), "Acme     /   Camera");

        let mut cfg = config.create_editable_copy();

        // No family separator.
        cfg.set_family_separator('\0').unwrap();
        let info = ColorSpaceInfo::from_color_space(&cfg, &cs);
        assert_eq!(info.num_hierarchy_levels(), 1);
        assert_eq!(info.hierarchy_level(0), cs.family());

        // '/' is the new family separator.
        cfg.set_family_separator('/').unwrap();
        let info = ColorSpaceInfo::from_color_space(&cfg, &cs);
        assert_eq!(info.num_hierarchy_levels(), 2);
        assert_eq!(info.hierarchy_level(0), "Acme");
        assert_eq!(info.hierarchy_level(1), "Camera");
        assert_eq!(info.family(), cs.family());

        // '-' is the new family separator.
        cfg.set_family_separator('-').unwrap();
        let info = ColorSpaceInfo::from_color_space(&cfg, &cs);
        assert_eq!(info.num_hierarchy_levels(), 1);
        assert_eq!(info.hierarchy_level(0), cs.family());

        // Reset to the v2 default family separator i.e. default to '/'.
        cfg.set_family_separator(Config::default_family_separator())
            .unwrap();
        let info = ColorSpaceInfo::from_color_space(&cfg, &cs);
        assert_eq!(info.num_hierarchy_levels(), 2);
        assert_eq!(info.hierarchy_level(0), "Acme");
        assert_eq!(info.hierarchy_level(1), "Camera");
        assert_eq!(info.family(), cs.family());

        // Change the description.
        cs.set_description("desc 1\n\n\n desc 2");
        assert_eq!(cs.description(), "desc 1\n\n\n desc 2");
        let info = ColorSpaceInfo::from_color_space(&cfg, &cs);
        assert_eq!(info.description(), cs.description());
    }
}
