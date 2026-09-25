//! Config merging (port of `apphelpers/mergeconfigs` and of the
//! `ConfigMergingParameters`, `ConfigMerger` and `ConfigMergingHelpers`
//! parts of `OpenColorAppHelpers.h`).
//!
//! A merge combines a *base* and an *input* config section by section
//! (general attributes, roles, file rules, displays / views, view
//! transforms, looks, color spaces and named transforms), each section
//! using a [`MergeStrategies`] value. The [`ConfigMerger`] may hold a series
//! of merges and can be read from / written to an OCIOM file.

pub mod merge_utils;
mod ociom_yaml;
pub(crate) mod section_merger;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_colorspaces;
#[cfg(test)]
mod tests_merges;
#[cfg(test)]
mod tests_named_transforms;
#[cfg(test)]
mod tests_sections;

use crate::config::utils::{split, trim};
use crate::config::{ColorSpace, Config};
use crate::error::{Error, Result};
use crate::path_utils;
use section_merger::{
    ColorspacesMerger, DisplayViewMerger, FileRulesMerger, GeneralMerger, LooksMerger,
    MergeHandlerOptions, NamedTransformsMerger, RolesMerger, SectionMerger, ViewTransformsMerger,
};
use std::fmt;

/// The merge strategy of a config section.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MergeStrategies {
    /// Combine elements of the base and input configs, with the input taking
    /// priority.
    PreferInput,
    /// Combine elements of the base and input configs, with the base taking
    /// priority.
    PreferBase,
    /// Use only the input elements for that section of the config.
    InputOnly,
    /// Use only the base elements for that section of the config.
    BaseOnly,
    /// The elements of the input config are removed from the base config (if
    /// the names match, the item is removed, even if the content differs).
    Remove,
    /// The strategy has not been set yet.
    Unspecified,
}

impl MergeStrategies {
    /// The OCIOM name of the strategy (`EnumToStrategyString`).
    pub fn as_str(&self) -> &'static str {
        match self {
            MergeStrategies::PreferInput => "PreferInput",
            MergeStrategies::PreferBase => "PreferBase",
            MergeStrategies::InputOnly => "InputOnly",
            MergeStrategies::BaseOnly => "BaseOnly",
            MergeStrategies::Remove => "Remove",
            MergeStrategies::Unspecified => "Unspecified",
        }
    }

    /// Parse an OCIOM strategy name (`StrategyStringToEnum`); unknown names
    /// give [`MergeStrategies::Unspecified`].
    pub fn from_str_lossy(s: &str) -> MergeStrategies {
        match s {
            "PreferInput" => MergeStrategies::PreferInput,
            "PreferBase" => MergeStrategies::PreferBase,
            "InputOnly" => MergeStrategies::InputOnly,
            "BaseOnly" => MergeStrategies::BaseOnly,
            "Remove" => MergeStrategies::Remove,
            _ => MergeStrategies::Unspecified,
        }
    }
}

impl fmt::Display for MergeStrategies {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The options controlling a merge (one of the merges of an OCIOM file).
#[derive(Debug, Clone)]
pub struct ConfigMergingParameters {
    // Names or paths identifying the base and input configs.
    base_config: String,
    input_config: String,
    // Name of the output config (may be used as the input or base config of a subsequent
    // merge).
    output_name: String,

    // Overrides.
    name: String,
    description: String,
    // Stores the overrides of search_path, environment, active_displays, active_views and
    // inactive_colorspaces.
    override_cfg: Config,

    // Options.
    input_family_prefix: String,
    base_family_prefix: String,
    input_first: bool,
    error_on_conflict: bool,
    avoid_duplicates: bool,
    adjust_input_reference_space: bool,

    // Merge strategy of each section (the default strategy is used for the unspecified ones).
    default_strategy: MergeStrategies,
    roles: MergeStrategies,
    file_rules: MergeStrategies,
    display_views: MergeStrategies,
    view_transforms: MergeStrategies,
    looks: MergeStrategies,
    colorspaces: MergeStrategies,
    named_transforms: MergeStrategies,
}

impl Default for ConfigMergingParameters {
    fn default() -> Self {
        Self::new()
    }
}

macro_rules! strategy_accessors {
    ($($(#[$doc:meta])* $field:ident, $setter:ident;)*) => {
        $(
            $(#[$doc])*
            ///
            /// The default strategy is returned if the strategy is unspecified.
            pub fn $field(&self) -> MergeStrategies {
                if self.$field == MergeStrategies::Unspecified {
                    return self.default_strategy;
                }
                self.$field
            }

            $(#[$doc])*
            pub fn $setter(&mut self, strategy: MergeStrategies) {
                self.$field = strategy;
            }
        )*
    };
}

impl ConfigMergingParameters {
    /// Default parameters (`ConfigMergingParameters::Create`).
    pub fn new() -> Self {
        let mut override_cfg = Config::new();
        override_cfg.clear_environment_vars();
        Self {
            base_config: String::new(),
            input_config: String::new(),
            output_name: "merged".to_string(),
            name: String::new(),
            description: String::new(),
            override_cfg,
            input_family_prefix: String::new(),
            base_family_prefix: String::new(),
            input_first: true,
            error_on_conflict: false,
            avoid_duplicates: true,
            adjust_input_reference_space: true,
            default_strategy: MergeStrategies::PreferInput,
            roles: MergeStrategies::Unspecified,
            file_rules: MergeStrategies::Unspecified,
            display_views: MergeStrategies::Unspecified,
            view_transforms: MergeStrategies::Unspecified,
            looks: MergeStrategies::Unspecified,
            colorspaces: MergeStrategies::Unspecified,
            named_transforms: MergeStrategies::Unspecified,
        }
    }

    /// Deep copy (`createEditableCopy`).
    pub fn create_editable_copy(&self) -> Self {
        self.clone()
    }

    /// Set the file name of the base config (used with the search path of
    /// the [`ConfigMerger`]).
    pub fn set_base_config_name(&mut self, base_config: &str) {
        self.base_config = base_config.to_string();
    }

    /// The file name of the base config.
    pub fn base_config_name(&self) -> &str {
        &self.base_config
    }

    /// Set the file name of the input config.
    pub fn set_input_config_name(&mut self, input_config: &str) {
        self.input_config = input_config.to_string();
    }

    /// The file name of the input config.
    pub fn input_config_name(&self) -> &str {
        &self.input_config
    }

    /// Set the name of this merge (usable as the input or base config name
    /// of a subsequent merge).
    pub fn set_output_name(&mut self, output_name: &str) {
        self.output_name = output_name.to_string();
    }

    /// The name of this merge.
    pub fn output_name(&self) -> &str {
        &self.output_name
    }

    /// Set the default strategy, used for the sections without strategy and
    /// for basic attributes such as the description (default is
    /// `PreferInput`).
    pub fn set_default_strategy(&mut self, strategy: MergeStrategies) {
        self.default_strategy = strategy;
    }

    /// The default strategy.
    pub fn default_strategy(&self) -> MergeStrategies {
        self.default_strategy
    }

    /// Set a prefix to add to the family of the input config items (using
    /// '/' as separator, replaced by the family separator of the config).
    pub fn set_input_family_prefix(&mut self, prefix: &str) {
        self.input_family_prefix = prefix.to_string();
    }

    /// The input family prefix.
    pub fn input_family_prefix(&self) -> &str {
        &self.input_family_prefix
    }

    /// Set a prefix to add to the family of the base config items.
    pub fn set_base_family_prefix(&mut self, prefix: &str) {
        self.base_family_prefix = prefix.to_string();
    }

    /// The base family prefix.
    pub fn base_family_prefix(&self) -> &str {
        &self.base_family_prefix
    }

    /// If true, items of the input config come first (default is true).
    pub fn set_input_first(&mut self, enabled: bool) {
        self.input_first = enabled;
    }

    /// True if the input items come first.
    pub fn is_input_first(&self) -> bool {
        self.input_first
    }

    /// If true, conflicts are errors rather than logged warnings (default is
    /// false).
    pub fn set_error_on_conflict(&mut self, enabled: bool) {
        self.error_on_conflict = enabled;
    }

    /// True if conflicts are errors.
    pub fn is_error_on_conflict(&self) -> bool {
        self.error_on_conflict
    }

    /// If true, input color spaces mathematically equivalent to a base color
    /// space are not added, their name and aliases being added to the base
    /// color space instead (default is true).
    pub fn set_avoid_duplicates(&mut self, enabled: bool) {
        self.avoid_duplicates = enabled;
    }

    /// True if duplicates are avoided.
    pub fn is_avoid_duplicates(&self) -> bool {
        self.avoid_duplicates
    }

    /// If true, the input color spaces are adjusted to use the reference
    /// space of the base config (default is true).
    pub fn set_adjust_input_reference_space(&mut self, enabled: bool) {
        self.adjust_input_reference_space = enabled;
    }

    /// True if the input reference space is adjusted.
    pub fn is_adjust_input_reference_space(&self) -> bool {
        self.adjust_input_reference_space
    }

    /// Override the name of the merged config.
    pub fn set_name(&mut self, name: &str) {
        self.name = name.to_string();
    }

    /// The name override.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Override the description of the merged config.
    pub fn set_description(&mut self, desc: &str) {
        self.description = desc.to_string();
    }

    /// The description override.
    pub fn description(&self) -> &str {
        &self.description
    }

    /// Override a context variable of the merged config.
    pub fn add_environment_var(&mut self, name: &str, default_value: &str) {
        self.override_cfg
            .add_environment_var(name, Some(default_value));
    }

    /// Number of context variable overrides.
    pub fn num_environment_vars(&self) -> usize {
        self.override_cfg.num_environment_vars()
    }

    /// Name of the context variable override at `index` (`""` if out of
    /// range).
    pub fn environment_var(&self, index: usize) -> &str {
        self.override_cfg.environment_var_name_by_index(index)
    }

    /// Value of the context variable override at `index`.
    pub fn environment_var_value(&self, index: usize) -> &str {
        let name = self.override_cfg.environment_var_name_by_index(index);
        self.override_cfg.environment_var_default(name)
    }

    /// Override the search path of the merged config.
    pub fn set_search_path(&mut self, path: &str) {
        self.override_cfg.set_search_path(path);
    }

    /// Add a path to the search path override.
    pub fn add_search_path(&mut self, path: &str) {
        self.override_cfg.add_search_path(path);
    }

    /// The search path override.
    pub fn search_path(&self) -> String {
        self.override_cfg.search_path()
    }

    /// Override the active displays of the merged config.
    pub fn set_active_displays(&mut self, displays: &str) -> Result<()> {
        self.override_cfg.set_active_displays(displays)
    }

    /// The active displays override.
    pub fn active_displays(&self) -> String {
        self.override_cfg.active_displays()
    }

    /// Override the active views of the merged config.
    pub fn set_active_views(&mut self, views: &str) -> Result<()> {
        self.override_cfg.set_active_views(views)
    }

    /// The active views override.
    pub fn active_views(&self) -> String {
        self.override_cfg.active_views()
    }

    /// Override the inactive color spaces of the merged config.
    pub fn set_inactive_color_spaces(&mut self, colorspaces: &str) {
        self.override_cfg.set_inactive_color_spaces(colorspaces);
    }

    /// The inactive color spaces override.
    pub fn inactive_color_spaces(&self) -> &str {
        self.override_cfg.inactive_color_spaces()
    }

    strategy_accessors! {
        /// Merge strategy of the roles section.
        roles, set_roles;
        /// Merge strategy of the file_rules section.
        file_rules, set_file_rules;
        /// Merge strategy of the displays / views section (shared_views, displays,
        /// viewing_rules, virtual_display, active_displays and active_views).
        display_views, set_display_views;
        /// Merge strategy of the view_transforms section (and default_view_transform).
        view_transforms, set_view_transforms;
        /// Merge strategy of the looks section.
        looks, set_looks;
        /// Merge strategy of the color spaces section (colorspaces, display_colorspaces,
        /// environment, search_path, family_separator and inactive_colorspaces).
        colorspaces, set_colorspaces;
        /// Merge strategy of the named_transforms section.
        named_transforms, set_named_transforms;
    }
}

impl fmt::Display for ConfigMergingParameters {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts: Vec<String> = Vec::new();
        let mut s = |label: &str, value: &str| {
            if !value.is_empty() {
                parts.push(format!("{label}: {value}"));
            }
        };
        s("base", self.base_config_name());
        s("input", self.input_config_name());
        s("output_name", self.output_name());
        s("input_family_prefix", self.input_family_prefix());
        s("base_family_prefix", self.base_family_prefix());
        let b = |v: bool| if v { "true" } else { "false" };
        parts.push(format!("input_first: {}", b(self.is_input_first())));
        parts.push(format!(
            "error_on_conflict: {}",
            b(self.is_error_on_conflict())
        ));
        parts.push(format!("default_strategy: {}", self.default_strategy()));
        parts.push(format!(
            "avoid_duplicates: {}",
            b(self.is_avoid_duplicates())
        ));
        parts.push(format!(
            "adjust_input_reference_space: {}",
            b(self.is_adjust_input_reference_space())
        ));
        let mut s = |label: &str, value: &str| {
            if !value.is_empty() {
                parts.push(format!("{label}: {value}"));
            }
        };
        s("name", self.name());
        s("description", self.description());
        s("search_path", &self.search_path());
        s("active_displays", &self.active_displays());
        s("active_views", &self.active_views());
        s("inactive_colorspaces", self.inactive_color_spaces());
        parts.push(format!("roles: {}", self.roles()));
        parts.push(format!("file_rules: {}", self.file_rules()));
        parts.push(format!("display-views: {}", self.display_views()));
        parts.push(format!("view_transforms: {}", self.view_transforms()));
        parts.push(format!("looks: {}", self.looks()));
        parts.push(format!("colorspaces: {}", self.colorspaces()));
        parts.push(format!("named_transforms: {}", self.named_transforms()));

        let n = self.num_environment_vars();
        if n > 0 {
            let vars: Vec<String> = (0..n)
                .map(|i| {
                    let v = self.environment_var_value(i);
                    if v.is_empty() {
                        self.environment_var(i).to_string()
                    } else {
                        format!("{}={}", self.environment_var(i), v)
                    }
                })
                .collect();
            parts.push(format!("environment: [{}]", vars.join(", ")));
        }
        write!(f, "<{}>", parts.join(", "))
    }
}

/// The controller of the merging process: the search paths to find the base
/// and input configs and the parameters of each merge. It may be read from
/// or written to an OCIOM file, for example:
///
/// ```yaml
/// ociom_version: 1.0
/// search_path:
///   - /usr/local/configs
///   - .
/// merge:
///   Merge_ADD_THIS:
///     base: base.ocio
///     input: input.ocio
///     options:
///       input_family_prefix: ""
///       base_family_prefix: ""
///       input_first: true
///       error_on_conflict: false
///       default_strategy: PreferInput
///       avoid_duplicates: true
///       adjust_input_reference_space: true
///     overrides:
///       name: ""
///       description: ""
///       search_path: ""
///       environment: {}
///       active_displays: []
///       active_views: []
///       inactive_colorspaces: []
///     params:
///       roles:
///         strategy: PreferBase
///       colorspaces:
///         strategy: PreferInput
/// ```
#[derive(Debug, Clone)]
pub struct ConfigMerger {
    // Search paths of the config files to merge (each config has its own search path for
    // its LUTs).
    search_paths: Vec<String>,
    working_dir: String,
    // Version of the OCIOM file format.
    major_version: u32,
    minor_version: u32,
    // The parameters of each merge.
    merge_params: Vec<ConfigMergingParameters>,
    // The output of each merge.
    merged_configs: Vec<Config>,
}

impl Default for ConfigMerger {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigMerger {
    /// An empty merger (`ConfigMerger::Create`).
    pub fn new() -> Self {
        Self {
            search_paths: Vec::new(),
            working_dir: String::new(),
            major_version: 1,
            minor_version: 0,
            merge_params: Vec::new(),
            merged_configs: Vec::new(),
        }
    }

    /// Parse an OCIOM file (`ConfigMerger::CreateFromFile`).
    pub fn create_from_file(filepath: &str) -> Result<ConfigMerger> {
        if filepath.is_empty() {
            return Err(Error::missing_file(
                "The merge options filepath is missing.",
            ));
        }
        let data = std::fs::read(filepath)
            .map_err(|_| Error::msg(format!("Error could not read '{filepath}' merge options.")))?;
        let text = String::from_utf8_lossy(&data);
        ociom_yaml::read(&text, filepath)
    }

    /// Deep copy (`createEditableCopy`).
    pub fn create_editable_copy(&self) -> Self {
        self.clone()
    }

    /// Set the search paths used to locate the input and base configs (':'
    /// separated).
    pub fn set_search_path(&mut self, path: &str) {
        self.search_paths = split(path, ':');
    }

    /// Add a single path to the search paths.
    pub fn add_search_path(&mut self, path: &str) {
        if !path.is_empty() {
            self.search_paths.push(path.to_string());
        }
    }

    /// Number of search paths.
    pub fn num_search_paths(&self) -> usize {
        self.search_paths.len()
    }

    /// Search path at `index` (`""` if out of range).
    pub fn search_path(&self, index: usize) -> &str {
        self.search_paths
            .get(index)
            .map(|s| s.as_str())
            .unwrap_or("")
    }

    /// Set the directory used to resolve relative search paths (defaults to
    /// the directory of the OCIOM file). It is also the fallback search path.
    pub fn set_working_dir(&mut self, dirname: &str) {
        self.working_dir = dirname.to_string();
    }

    /// The working directory.
    pub fn working_dir(&self) -> &str {
        &self.working_dir
    }

    /// The parameters of a merge (`None` if out of range).
    pub fn params(&self, index: usize) -> Option<&ConfigMergingParameters> {
        self.merge_params.get(index)
    }

    /// Mutable access to the parameters of a merge.
    pub fn params_mut(&mut self, index: usize) -> Option<&mut ConfigMergingParameters> {
        self.merge_params.get_mut(index)
    }

    /// Number of merges.
    pub fn num_config_merging_parameters(&self) -> usize {
        self.merge_params.len()
    }

    /// Add a merge.
    pub fn add_params(&mut self, params: ConfigMergingParameters) {
        self.merge_params.push(params);
    }

    // Load the config identified by `value`: using the search paths, then as a builtin
    // config, then as the output of a previous merge (`loadConfig`).
    fn load_config(&self, value: &str) -> Option<Config> {
        let mut searchpaths: Vec<String> = Vec::new();
        if self.search_paths.is_empty() {
            searchpaths.push(self.working_dir.clone());
        }
        for path in &self.search_paths {
            // Remove trailing "/", and spaces.
            let mut dirname = trim(path).trim_end_matches('/').to_string();
            if !path_utils::is_absolute(&dirname) {
                dirname = path_utils::join(&self.working_dir, &dirname);
            }
            searchpaths.push(path_utils::normpath(&dirname));
        }

        for sp in &searchpaths {
            // Normalize the path to prevent directory traversal via '../' sequences.
            let full = path_utils::normpath(&path_utils::join(sp, value));
            // Simply trying the various locations on the path.
            if let Ok(c) = Config::create_from_file(&full) {
                return Some(c);
            }
        }

        // Try to load the config as a builtin config.
        if let Ok(c) = Config::create_from_builtin_config(value) {
            return Some(c);
        }

        // Must be a reference to a config from a previous merge.
        for (i, p) in self.merge_params.iter().enumerate() {
            if p.output_name().eq_ignore_ascii_case(value) {
                return self.merged_configs.get(i).cloned();
            }
        }
        None
    }

    /// Execute the merges (`mergeConfigs`). The returned merger holds the
    /// merged configs.
    pub fn merge_configs(&self) -> Result<ConfigMerger> {
        let mut merger = self.create_editable_copy();
        for params in &self.merge_params {
            let base = merger.load_config(params.base_config_name());
            let input = merger.load_config(params.input_config_name());
            match (base, input) {
                (Some(base), Some(input)) => {
                    let merged = merge_configs(params, &base, &input)?;
                    // Keep the merged config to be used by the following merges.
                    merger.merged_configs.push(merged);
                }
                _ => return Err(Error::msg("Could not load the base or the input config")),
            }
        }
        Ok(merger)
    }

    /// The final merged config.
    pub fn merged_config(&self) -> Option<&Config> {
        self.merged_configs.last()
    }

    /// One of the merged configs (`None` if out of range).
    pub fn merged_config_at(&self, index: usize) -> Option<&Config> {
        self.merged_configs.get(index)
    }

    /// Number of merged configs.
    pub fn num_merged_configs(&self) -> usize {
        self.merged_configs.len()
    }

    /// Serialize to the OCIOM file format.
    pub fn serialize(&self) -> Result<String> {
        ociom_yaml::write(self)
            .map_err(|e| Error::msg(format!("Error building YAML: {}", e.message())))
    }

    /// Set the version of the OCIOM file format.
    pub fn set_version(&mut self, major: u32, minor: u32) {
        self.major_version = major;
        self.minor_version = minor;
    }

    /// Major version of the OCIOM file format.
    pub fn major_version(&self) -> u32 {
        self.major_version
    }

    /// Minor version of the OCIOM file format.
    pub fn minor_version(&self) -> u32 {
        self.minor_version
    }
}

impl fmt::Display for ConfigMerger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.serialize() {
            Ok(s) => f.write_str(&s),
            Err(_) => Err(fmt::Error),
        }
    }
}

/// Run all the section mergers.
fn run_all_mergers(options: &mut MergeHandlerOptions) -> Result<()> {
    GeneralMerger::new(options).merge()?;
    RolesMerger::new(options).merge()?;
    FileRulesMerger::new(options).merge()?;
    DisplayViewMerger::new(options).merge()?;
    ViewTransformsMerger::new(options)?.merge()?;
    LooksMerger::new(options).merge()?;
    ColorspacesMerger::new(options)?.merge()?;
    NamedTransformsMerger::new(options).merge()?;
    Ok(())
}

/// Merge the input config into the base config using the parameters
/// (`ConfigMergingHelpers::MergeConfigs`).
pub fn merge_configs(
    params: &ConfigMergingParameters,
    base_config: &Config,
    input_config: &Config,
) -> Result<Config> {
    // The merged config must be initialized with a copy of the base config.
    let mut merged = base_config.create_editable_copy();
    {
        let mut options = MergeHandlerOptions {
            base_config,
            input_config,
            params,
            merged_config: &mut merged,
        };
        run_all_mergers(&mut options)?;
    }
    Ok(merged)
}

/// Merge a single color space into the base config
/// (`ConfigMergingHelpers::MergeColorSpace`). The adjust input reference
/// space option is ignored (set to false): to use the automatic reference
/// space conversion, add the color space to an input config having the
/// necessary interchange roles.
pub fn merge_color_space(
    params: &ConfigMergingParameters,
    base_config: &Config,
    colorspace: &ColorSpace,
) -> Result<Config> {
    // Create an input config and add the color space.
    let mut input_config = Config::new();
    input_config.add_color_space(colorspace)?;

    // The merged config must be initialized with a copy of the base config.
    let mut merged = base_config.create_editable_copy();

    // With only the color space, the reference space is unknown, so turn off automatic
    // reference space conversion to the reference space of the base config.
    let mut e_params = params.create_editable_copy();
    e_params.set_adjust_input_reference_space(false);

    {
        let mut options = MergeHandlerOptions {
            base_config,
            input_config: &input_config,
            params: &e_params,
            merged_config: &mut merged,
        };
        ColorspacesMerger::new(&mut options)?.merge()?;
    }
    Ok(merged)
}
