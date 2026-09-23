//! The configuration: color spaces, roles, displays / views, looks, view
//! transforms, named transforms, file and viewing rules (port of
//! `Config.cpp` and of the config objects of `OpenColorIO.h`).
//!
//! The public signatures used by other modules (`create_*`, `serialize`,
//! `validate`, `current_context`, `has_color_space`, `get_processor*`) are
//! kept stable.

mod api_display;
mod api_processor;
mod api_spaces;
mod api_validate;
pub mod archive;
pub mod colorspace;
pub mod config_utils;
mod context_vars;
pub mod display;
pub mod file_rules;
mod icc;
pub mod logging;
pub mod look;
pub mod look_parse;
pub mod named_transform;
pub mod tokens;
pub mod transform_display;
mod transforms;
pub mod utils;
pub mod view_transform;
pub mod viewing_rules;
pub mod yaml;

pub use colorspace::{ColorSpace, ColorSpaceSet};
pub use display::View;
pub use file_rules::{FileRules, DEFAULT_RULE_NAME, FILE_PATH_SEARCH_RULE_NAME};
pub use look::Look;
pub use look_parse::{LookParseResult, LookToken};
pub use named_transform::NamedTransform;
pub use transforms::{get_looks_result_color_space, BuildColorSpaceOps};
pub use view_transform::ViewTransform;
pub use viewing_rules::ViewingRules;

use crate::bail;
use crate::context::Context;
use crate::error::{Error, Result};
use crate::processor::Processor;
use crate::transforms::Transform;
use crate::types::*;
use display::{add_view, compute_displays, find_display, find_view, Display, DisplayMap};
use logging::{log_error, log_info};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;
use std::sync::Mutex;
use utils::{
    compare, contain, contains_context_variable_token, contains_context_variables, find_in_vec_case_ignore,
    intersect_case_ignore, join_string_env_style, split_string_env_style, trim,
};

/// First supported major version.
pub const FIRST_SUPPORTED_MAJOR_VERSION: u32 = 1;
/// Last supported major version.
pub const LAST_SUPPORTED_MAJOR_VERSION: u32 = 2;
/// Most recent minor version for each major version.
pub const LAST_SUPPORTED_MINOR_VERSION: [u32; 2] = [0, 6];

/// Default family separator.
pub const DEFAULT_FAMILY_SEPARATOR: char = '/';

const DEFAULT_LUMA_COEFS: [f64; 3] = [0.2126, 0.7152, 0.0722];

/// The config used by `Config::create_raw`.
pub const INTERNAL_RAW_PROFILE: &str = "ocio_profile_version: 2\n\
strictparsing: false\n\
roles:\n\
  default: raw\n\
file_rules:\n\
  - !<Rule> {name: Default, colorspace: default}\n\
displays:\n\
  sRGB:\n\
  - !<View> {name: Raw, colorspace: raw}\n\
colorspaces:\n\
  - !<ColorSpace>\n\
      name: raw\n\
      family: raw\n\
      equalitygroup:\n\
      bitdepth: 32f\n\
      isdata: true\n\
      allocation: uniform\n\
      description: 'A raw color space. Conversions to and from this space are no-ops.'\n";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Validation {
    Unknown,
    Passed,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InactiveType {
    ColorSpace,
    NamedTransform,
    All,
}

#[derive(Debug, Default)]
struct CacheIds {
    ids: BTreeMap<String, String>,
    no_context: String,
}

#[derive(Debug, Default)]
struct ProcessorCache {
    entries: HashMap<String, Processor>,
}

fn env_present(name: &str) -> bool {
    std::env::var_os(name).is_some()
}

fn getenv(name: &str) -> String {
    std::env::var(name).unwrap_or_default()
}

/// An OCIO configuration.
#[derive(Debug)]
pub struct Config {
    major_version: u32,
    minor_version: u32,
    env: BTreeMap<String, String>,
    context: Context,
    name: String,
    family_separator: char,
    description: String,

    all_color_spaces: ColorSpaceSet,
    active_color_space_names: Vec<String>,
    inactive_color_space_names: Vec<String>,
    inactive_names_api: String,
    inactive_names_env: String,
    inactive_names_conf: String,

    roles: BTreeMap<String, String>,
    looks: Vec<Look>,

    displays: DisplayMap,
    active_displays: Vec<String>,
    active_displays_env_override: Vec<String>,
    active_views: Vec<String>,
    active_views_env_override: Vec<String>,
    shared_views: Vec<View>,
    viewing_rules: ViewingRules,
    virtual_display: Display,

    view_transforms: Vec<ViewTransform>,
    default_view_transform: String,

    all_named_transforms: Vec<NamedTransform>,
    active_named_transform_names: Vec<String>,
    inactive_named_transform_names: Vec<String>,

    default_luma_coefs: [f64; 3],
    strict_parsing: bool,

    validation: Mutex<(Validation, String)>,
    cache_ids: Mutex<CacheIds>,
    file_rules: FileRules,

    cache_flags: Mutex<ProcessorCacheFlags>,
    processor_cache: Mutex<ProcessorCache>,
    env_disable_processor_cache: bool,

    archive: Option<archive::OciozArchive>,
}

impl Clone for Config {
    fn clone(&self) -> Self {
        let validation = self.validation.lock().map(|v| v.clone()).unwrap_or((Validation::Unknown, String::new()));
        let (ids, no_context) = self
            .cache_ids
            .lock()
            .map(|c| (c.ids.clone(), c.no_context.clone()))
            .unwrap_or_default();
        Config {
            major_version: self.major_version,
            minor_version: self.minor_version,
            env: self.env.clone(),
            context: self.context.clone(),
            name: self.name.clone(),
            family_separator: self.family_separator,
            description: self.description.clone(),
            all_color_spaces: self.all_color_spaces.clone(),
            active_color_space_names: self.active_color_space_names.clone(),
            inactive_color_space_names: self.inactive_color_space_names.clone(),
            inactive_names_api: self.inactive_names_api.clone(),
            inactive_names_env: self.inactive_names_env.clone(),
            inactive_names_conf: self.inactive_names_conf.clone(),
            roles: self.roles.clone(),
            looks: self.looks.clone(),
            displays: self.displays.clone(),
            active_displays: self.active_displays.clone(),
            active_displays_env_override: self.active_displays_env_override.clone(),
            active_views: self.active_views.clone(),
            active_views_env_override: self.active_views_env_override.clone(),
            shared_views: self.shared_views.clone(),
            viewing_rules: self.viewing_rules.clone(),
            virtual_display: self.virtual_display.clone(),
            view_transforms: self.view_transforms.clone(),
            default_view_transform: self.default_view_transform.clone(),
            all_named_transforms: self.all_named_transforms.clone(),
            active_named_transform_names: self.active_named_transform_names.clone(),
            inactive_named_transform_names: self.inactive_named_transform_names.clone(),
            default_luma_coefs: self.default_luma_coefs,
            strict_parsing: self.strict_parsing,
            validation: Mutex::new(validation),
            cache_ids: Mutex::new(CacheIds { ids, no_context }),
            file_rules: self.file_rules.clone(),
            cache_flags: Mutex::new(self.processor_cache_flags()),
            // The processor cache is not copied.
            processor_cache: Mutex::new(ProcessorCache::default()),
            env_disable_processor_cache: self.env_disable_processor_cache,
            archive: self.archive.clone(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.serialize() {
            Ok(s) => f.write_str(&s),
            Err(_) => Err(fmt::Error),
        }
    }
}

fn prefix_error_msg(display: &str, view: &View) -> String {
    let mut s = String::from("Config failed display view validation. ");
    if display.is_empty() {
        s.push_str("Shared ");
    } else {
        s.push_str(&format!("Display '{display}' has a "));
    }
    if view.name.is_empty() {
        s.push_str("view with an empty name.");
    } else {
        s.push_str(&format!("view '{}' ", view.name));
    }
    s
}

fn match_reference_type(st: SearchReferenceSpaceType, t: ReferenceSpaceType) -> bool {
    match st {
        SearchReferenceSpaceType::Scene => t == ReferenceSpaceType::Scene,
        SearchReferenceSpaceType::Display => t == ReferenceSpaceType::Display,
        SearchReferenceSpaceType::All => true,
    }
}

/// Collect the file references of a transform (recursing in groups).
fn get_file_references(files: &mut BTreeSet<String>, t: &Transform) {
    match t {
        Transform::Group(g) => {
            for c in &g.transforms {
                get_file_references(files, c);
            }
        }
        Transform::File(f) => {
            files.insert(f.src.clone());
        }
        _ => {}
    }
}

/// Collect the color space names referenced by a transform (context
/// variables resolved).
fn get_color_space_references(names: &mut BTreeSet<String>, t: &Transform, context: &Context) {
    match t {
        Transform::Group(g) => {
            for c in &g.transforms {
                get_color_space_references(names, c, context);
            }
        }
        Transform::ColorSpace(c) => {
            names.insert(context.resolve_string_var(&c.src));
            names.insert(context.resolve_string_var(&c.dst));
        }
        Transform::DisplayView(d) => {
            names.insert(d.src.clone());
        }
        Transform::Look(l) => {
            names.insert(l.src.clone());
            names.insert(l.dst.clone());
        }
        _ => {}
    }
}

impl Config {
    // -----------------------------------------------------------------------
    // Creation

    /// A new, empty config using the latest version (`Config::Create`).
    pub fn new() -> Config {
        let mut active_displays_env_override = Vec::new();
        let d = trim(&getenv(OCIO_ACTIVE_DISPLAYS_ENVVAR)).to_string();
        if !d.is_empty() {
            active_displays_env_override = utils::split_string_env_style_lossy(&d);
        }
        let mut active_views_env_override = Vec::new();
        let v = trim(&getenv(OCIO_ACTIVE_VIEWS_ENVVAR)).to_string();
        if !v.is_empty() {
            active_views_env_override = utils::split_string_env_style_lossy(&v);
        }
        let inactive_env = trim(&getenv(OCIO_INACTIVE_COLORSPACES_ENVVAR)).to_string();
        let virtual_display = Display { temporary: true, ..Default::default() };
        Config {
            major_version: LAST_SUPPORTED_MAJOR_VERSION,
            minor_version: LAST_SUPPORTED_MINOR_VERSION[(LAST_SUPPORTED_MAJOR_VERSION - 1) as usize],
            env: BTreeMap::new(),
            context: Context::new(),
            name: String::new(),
            family_separator: DEFAULT_FAMILY_SEPARATOR,
            description: String::new(),
            all_color_spaces: ColorSpaceSet::new(),
            active_color_space_names: Vec::new(),
            inactive_color_space_names: Vec::new(),
            inactive_names_api: String::new(),
            inactive_names_env: inactive_env,
            inactive_names_conf: String::new(),
            roles: BTreeMap::new(),
            looks: Vec::new(),
            displays: Vec::new(),
            active_displays: Vec::new(),
            active_displays_env_override,
            active_views: Vec::new(),
            active_views_env_override,
            shared_views: Vec::new(),
            viewing_rules: ViewingRules::new(),
            virtual_display,
            view_transforms: Vec::new(),
            default_view_transform: String::new(),
            all_named_transforms: Vec::new(),
            active_named_transform_names: Vec::new(),
            inactive_named_transform_names: Vec::new(),
            default_luma_coefs: DEFAULT_LUMA_COEFS,
            strict_parsing: true,
            validation: Mutex::new((Validation::Unknown, String::new())),
            cache_ids: Mutex::new(CacheIds::default()),
            file_rules: FileRules::new(),
            cache_flags: Mutex::new(ProcessorCacheFlags::DEFAULT),
            processor_cache: Mutex::new(ProcessorCache::default()),
            env_disable_processor_cache: env_present(OCIO_DISABLE_ALL_CACHES)
                || env_present(OCIO_DISABLE_PROCESSOR_CACHES),
            archive: None,
        }
    }

    /// Same as [`Config::new`] (`Config::Create`).
    pub fn create() -> Config {
        Config::new()
    }

    /// A minimal config with a single `raw` color space.
    pub fn create_raw() -> Config {
        Config::create_from_str(INTERNAL_RAW_PROFILE).unwrap_or_default()
    }

    /// Load the config pointed to by the `$OCIO` environment variable (or a
    /// raw config if unset).
    pub fn create_from_env() -> Result<Config> {
        let file = getenv(OCIO_CONFIG_ENVVAR);
        if !file.is_empty() {
            return Config::create_from_file(&file);
        }
        log_info("Color management disabled. (Specify the $OCIO environment variable to enable.)");
        Ok(Config::create_raw())
    }

    /// Load a config file (`.ocio`, `.ocioz` or `ocio://` builtin URI).
    pub fn create_from_file(path: &str) -> Result<Config> {
        if path.is_empty() {
            return Err(Error::msg("The config filepath is missing."));
        }
        if let Some(pos) = path.find(OCIO_BUILTIN_URI_PREFIX) {
            let rest = &path[pos + OCIO_BUILTIN_URI_PREFIX.len()..];
            if rest.chars().next().map(|c| !c.is_whitespace()).unwrap_or(false) {
                return Config::create_from_builtin_config(path);
            }
        }
        if !std::path::Path::new(path).exists() {
            return Err(Error::missing_file(format!("'{path}' file does not exist.")));
        }
        let data = std::fs::read(path)
            .map_err(|_| Error::msg(format!("Error could not read '{path}' OCIO profile.")))?;
        if data.len() >= 2 && data[0] == b'P' && data[1] == b'K' {
            let archive = archive::OciozArchive::open(path)?;
            return Config::create_from_archive(archive);
        }
        let text = String::from_utf8_lossy(&data);
        Config::read(&text, Some(path))
    }

    /// Parse a config from a YAML string (`Config::CreateFromStream`).
    pub fn create_from_str(yaml: &str) -> Result<Config> {
        Config::read(yaml, None)
    }

    /// Load a config stored in an OCIOZ archive.
    pub fn create_from_archive(archive: archive::OciozArchive) -> Result<Config> {
        let text = archive.config_data()?;
        let mut config = Config::read(&text, Some(yaml::ARCHIVE_FILENAME))?;
        archive.prepare_context(&mut config)?;
        config.archive = Some(archive);
        Ok(config)
    }

    /// Load one of the builtin configs (see `crate::builtins::configs`).
    pub fn create_from_builtin_config(name: &str) -> Result<Config> {
        let mut builtin = name.to_string();
        if !builtin.starts_with(OCIO_BUILTIN_URI_PREFIX) {
            builtin = format!("{OCIO_BUILTIN_URI_PREFIX}{builtin}");
        }
        let short = builtin[OCIO_BUILTIN_URI_PREFIX.len()..].to_string();
        match crate::builtins::configs::get_builtin_config(&short) {
            Some(text) => Config::create_from_str(text),
            None => Err(Error::msg(format!("Could not find '{short}' in the built-in configurations."))),
        }
    }

    fn read(text: &str, filename: Option<&str>) -> Result<Config> {
        let mut config = Config::new();
        yaml::read(text, &mut config, filename)?;
        config.check_version_consistency()?;
        config.inactive_names_api.clear();
        config.refresh_active_color_spaces();
        Ok(config)
    }

    /// Deep copy of the config (`createEditableCopy`).
    pub fn create_editable_copy(&self) -> Config {
        self.clone()
    }

    // -----------------------------------------------------------------------
    // Versions

    pub fn major_version(&self) -> u32 {
        self.major_version
    }

    /// Set the major version (and the latest minor version of that major).
    pub fn set_major_version(&mut self, version: u32) -> Result<()> {
        if !(FIRST_SUPPORTED_MAJOR_VERSION..=LAST_SUPPORTED_MAJOR_VERSION).contains(&version) {
            return Err(Error::msg(format!(
                "The version is {version} where supported versions start at {FIRST_SUPPORTED_MAJOR_VERSION} and end at {LAST_SUPPORTED_MAJOR_VERSION}."
            )));
        }
        self.major_version = version;
        self.minor_version = LAST_SUPPORTED_MINOR_VERSION[(version - 1) as usize];
        self.reset_cache_ids();
        Ok(())
    }

    pub fn minor_version(&self) -> u32 {
        self.minor_version
    }

    pub fn set_minor_version(&mut self, version: u32) -> Result<()> {
        let max = LAST_SUPPORTED_MINOR_VERSION[(self.major_version - 1) as usize];
        if version > max {
            return Err(Error::msg(format!(
                "The minor version {version} is not supported for major version {}. Maximum minor version is {max}.",
                self.major_version
            )));
        }
        self.minor_version = version;
        Ok(())
    }

    pub fn set_version(&mut self, major: u32, minor: u32) -> Result<()> {
        self.set_major_version(major)?;
        self.set_minor_version(minor)
    }

    /// Upgrade the config to the latest supported version.
    pub fn upgrade_to_latest_version(&mut self) {
        if self.major_version != LAST_SUPPORTED_MAJOR_VERSION {
            if self.major_version == 1 {
                let mut rules = self.file_rules.clone();
                let _ = file_rules::update_file_rules_from_v1_to_v2(self, &mut rules);
                self.file_rules = rules;
                self.major_version = 2;
                self.minor_version = 0;
            }
            let _ = self.set_major_version(LAST_SUPPORTED_MAJOR_VERSION);
            let _ = self
                .set_minor_version(LAST_SUPPORTED_MINOR_VERSION[(LAST_SUPPORTED_MAJOR_VERSION - 1) as usize]);
        }
    }

    fn version_hex(&self) -> u32 {
        (self.major_version << 24) | (self.minor_version << 16)
    }

    // -----------------------------------------------------------------------
    // Name, description, family separator

    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn set_name(&mut self, name: &str) {
        self.name = name.to_string();
    }

    pub fn family_separator(&self) -> char {
        self.family_separator
    }

    /// The default family separator (`/`).
    pub fn default_family_separator() -> char {
        DEFAULT_FAMILY_SEPARATOR
    }

    /// Set the family separator (`'\0'` means no separator).
    pub fn set_family_separator(&mut self, sep: char) -> Result<()> {
        let v = sep as u32;
        if v != 0 && !(32..=126).contains(&v) {
            return Err(Error::msg(format!("Invalid family separator '{sep}'.")));
        }
        self.family_separator = sep;
        Ok(())
    }

    pub fn description(&self) -> &str {
        &self.description
    }
    pub fn set_description(&mut self, d: &str) {
        self.description = d.to_string();
    }

    // -----------------------------------------------------------------------
    // Caches

    fn reset_cache_ids(&mut self) {
        if let Ok(c) = self.cache_ids.get_mut() {
            c.ids.clear();
            c.no_context.clear();
        }
        if let Ok(v) = self.validation.get_mut() {
            *v = (Validation::Unknown, String::new());
        }
        if let Ok(p) = self.processor_cache.get_mut() {
            p.entries.clear();
        }
    }

    /// The processor cache flags.
    pub fn processor_cache_flags(&self) -> ProcessorCacheFlags {
        self.cache_flags.lock().map(|f| *f).unwrap_or(ProcessorCacheFlags::DEFAULT)
    }

    /// Set the processor cache flags.
    pub fn set_processor_cache_flags(&self, flags: ProcessorCacheFlags) {
        if let Ok(mut f) = self.cache_flags.lock() {
            *f = flags;
        }
    }

    /// Flush the processor cache.
    pub fn clear_processor_cache(&self) {
        if let Ok(mut p) = self.processor_cache.lock() {
            p.entries.clear();
        }
    }

    fn processor_cache_enabled(&self) -> bool {
        !self.env_disable_processor_cache
            && (self.processor_cache_flags().0 & ProcessorCacheFlags::ENABLED.0) == ProcessorCacheFlags::ENABLED.0
    }

    /// Cache id of the config using the current context.
    pub fn cache_id(&self) -> String {
        self.cache_id_with_context(Some(&self.context))
    }

    /// Cache id of the config using a context (`None` uses no context).
    pub fn cache_id_with_context(&self, context: Option<&Context>) -> String {
        let ctx_id = context.map(|c| c.cache_id()).unwrap_or_default();
        if let Ok(c) = self.cache_ids.lock() {
            if let Some(id) = c.ids.get(&ctx_id) {
                return id.clone();
            }
        }
        let no_context = {
            let cached = self.cache_ids.lock().map(|c| c.no_context.clone()).unwrap_or_default();
            if cached.is_empty() {
                let s = yaml::write(self).unwrap_or_default();
                format!("{:x}", md5::compute(s.as_bytes()))
            } else {
                cached
            }
        };
        let mut file_hash = String::new();
        if let Some(ctx) = context {
            let mut files = BTreeSet::new();
            for t in self.all_internal_transforms() {
                get_file_references(&mut files, t);
            }
            let mut s = String::new();
            for f in &files {
                if f.is_empty() {
                    continue;
                }
                s.push_str(f);
                s.push('=');
                match ctx.resolve_file_location(f) {
                    Ok(p) => {
                        let h = std::fs::read(&p).map(|d| format!("{:x}", md5::compute(&d))).unwrap_or_default();
                        s.push_str(&h);
                        s.push(' ');
                    }
                    Err(_) => s.push_str("? "),
                }
            }
            file_hash = format!("{:x}", md5::compute(s.as_bytes()));
        }
        let id = format!("{no_context}:{file_hash}");
        if let Ok(mut c) = self.cache_ids.lock() {
            c.no_context = no_context;
            c.ids.insert(ctx_id, id.clone());
        }
        id
    }

    // -----------------------------------------------------------------------
    // Serialization

    /// Serialize to YAML.
    pub fn serialize(&self) -> Result<String> {
        self.check_version_consistency()
            .and_then(|_| yaml::write(self))
            .map_err(|e| Error::msg(format!("Error building YAML: {}", e.message())))
    }

    // -----------------------------------------------------------------------
    // Environment, search paths, working dir

    /// The config context (search paths, working dir, environment).
    pub fn current_context(&self) -> &Context {
        &self.context
    }

    /// Add (or remove with `None`) a context variable with its default value.
    pub fn add_environment_var(&mut self, name: &str, default_value: Option<&str>) {
        if name.is_empty() {
            return;
        }
        match default_value {
            Some(v) => {
                self.env.insert(name.to_string(), v.to_string());
                self.context.set_string_var(name, Some(v));
            }
            None => {
                self.env.remove(name);
                self.context.set_string_var(name, None);
            }
        }
        self.reset_cache_ids();
    }

    pub fn num_environment_vars(&self) -> usize {
        self.env.len()
    }

    /// Name of the context variable at `index` (`""` if out of range).
    pub fn environment_var_name_by_index(&self, index: usize) -> &str {
        self.env.keys().nth(index).map(|s| s.as_str()).unwrap_or("")
    }

    /// Default value of a context variable (`""` if missing).
    pub fn environment_var_default(&self, name: &str) -> &str {
        self.env.get(name).map(|s| s.as_str()).unwrap_or("")
    }

    pub fn clear_environment_vars(&mut self) {
        self.env.clear();
        self.context.clear_string_vars();
        self.reset_cache_ids();
    }

    pub fn set_environment_mode(&mut self, mode: EnvironmentMode) {
        self.context.set_environment_mode(mode);
        self.reset_cache_ids();
    }

    pub fn environment_mode(&self) -> EnvironmentMode {
        self.context.environment_mode()
    }

    /// Load the environment variables into the context.
    pub fn load_environment(&mut self) {
        self.context.load_environment();
        self.reset_cache_ids();
    }

    /// The search path (colon separated).
    pub fn search_path(&self) -> String {
        self.context.search_path()
    }

    pub fn set_search_path(&mut self, path: &str) {
        self.context.set_search_path(path);
        self.reset_cache_ids();
    }

    pub fn num_search_paths(&self) -> usize {
        self.context.num_search_paths()
    }

    /// Search path at `index` (`""` if out of range).
    pub fn search_path_by_index(&self, index: usize) -> &str {
        self.context.search_path_by_index(index).unwrap_or("")
    }

    pub fn clear_search_paths(&mut self) {
        self.context.clear_search_paths();
        self.reset_cache_ids();
    }

    pub fn add_search_path(&mut self, path: &str) {
        if path.is_empty() {
            return;
        }
        self.context.add_search_path(path);
        self.reset_cache_ids();
    }

    pub fn working_dir(&self) -> &str {
        self.context.working_dir()
    }

    pub fn set_working_dir(&mut self, dir: &str) {
        self.context.set_working_dir(dir);
        self.reset_cache_ids();
    }
}
