//! Builtin configs (port of `builtinconfigs/BuiltinConfigRegistry.cpp`,
//! `CGConfig.cpp` and `StudioConfig.cpp`).
//!
//! The configs are the ACES CG and Studio configs shipped with OCIO,
//! embedded verbatim from the `.ocio` files of this directory.

use crate::error::{Error, Result};
use crate::types::OCIO_BUILTIN_URI_PREFIX;
use std::sync::OnceLock;

const OUT_OF_RANGE_EXCEPTION_TEXT: &str = "Config index is out of range.";

/// URI of the default builtin config.
pub const DEFAULT_BUILTIN_CONFIG_URI: &str = "ocio://cg-config-v4.0.0_aces-v2.0_ocio-v2.5";
/// URI of the latest CG builtin config.
pub const LATEST_CG_BUILTIN_CONFIG_URI: &str = "ocio://cg-config-v4.0.0_aces-v2.0_ocio-v2.5";
/// URI of the latest Studio builtin config.
pub const LATEST_STUDIO_BUILTIN_CONFIG_URI: &str =
    "ocio://studio-config-v4.0.0_aces-v2.0_ocio-v2.5";

/// Alias of the default builtin config.
pub const BUILTIN_DEFAULT_NAME: &str = "default";
/// Alias of the latest CG builtin config.
pub const BUILTIN_LATEST_CG_NAME: &str = "cg-config-latest";
/// Alias of the latest Studio builtin config.
pub const BUILTIN_LATEST_STUDIO_NAME: &str = "studio-config-latest";

/// `cg-config-v1.0.0_aces-v1.3_ocio-v2.1`.
pub const CG_CONFIG_V100_ACES_V13_OCIO_V21: &str =
    include_str!("cg-config-v1.0.0_aces-v1.3_ocio-v2.1.ocio");
/// `cg-config-v2.1.0_aces-v1.3_ocio-v2.3`.
pub const CG_CONFIG_V210_ACES_V13_OCIO_V23: &str =
    include_str!("cg-config-v2.1.0_aces-v1.3_ocio-v2.3.ocio");
/// `cg-config-v2.2.0_aces-v1.3_ocio-v2.4`.
pub const CG_CONFIG_V220_ACES_V13_OCIO_V24: &str =
    include_str!("cg-config-v2.2.0_aces-v1.3_ocio-v2.4.ocio");
/// `cg-config-v4.0.0_aces-v2.0_ocio-v2.5`.
pub const CG_CONFIG_V400_ACES_V20_OCIO_V25: &str =
    include_str!("cg-config-v4.0.0_aces-v2.0_ocio-v2.5.ocio");
/// `studio-config-v1.0.0_aces-v1.3_ocio-v2.1`.
pub const STUDIO_CONFIG_V100_ACES_V13_OCIO_V21: &str =
    include_str!("studio-config-v1.0.0_aces-v1.3_ocio-v2.1.ocio");
/// `studio-config-v2.1.0_aces-v1.3_ocio-v2.3`.
pub const STUDIO_CONFIG_V210_ACES_V13_OCIO_V23: &str =
    include_str!("studio-config-v2.1.0_aces-v1.3_ocio-v2.3.ocio");
/// `studio-config-v2.2.0_aces-v1.3_ocio-v2.4`.
pub const STUDIO_CONFIG_V220_ACES_V13_OCIO_V24: &str =
    include_str!("studio-config-v2.2.0_aces-v1.3_ocio-v2.4.ocio");
/// `studio-config-v4.0.0_aces-v2.0_ocio-v2.5`.
pub const STUDIO_CONFIG_V400_ACES_V20_OCIO_V25: &str =
    include_str!("studio-config-v4.0.0_aces-v2.0_ocio-v2.5.ocio");

/// A registered builtin config.
#[derive(Debug, Clone, PartialEq, Eq)]
struct BuiltinConfigData {
    name: String,
    ui_name: String,
    config: &'static str,
    is_recommended: bool,
}

/// A registry of builtin configs (port of `BuiltinConfigRegistryImpl`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BuiltinConfigRegistry {
    configs: Vec<BuiltinConfigData>,
}

fn out_of_range() -> Error {
    Error::msg(OUT_OF_RANGE_EXCEPTION_TEXT)
}

impl BuiltinConfigRegistry {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// The global registry holding OCIO's builtin configs.
    pub fn get() -> &'static BuiltinConfigRegistry {
        static REGISTRY: OnceLock<BuiltinConfigRegistry> = OnceLock::new();
        REGISTRY.get_or_init(|| {
            let mut r = BuiltinConfigRegistry::new();
            r.init();
            r
        })
    }

    /// Register OCIO's builtin configs if the registry is empty.
    pub fn init(&mut self) {
        if self.configs.is_empty() {
            register_cg_configs(self);
            register_studio_configs(self);
        }
    }

    /// Add a builtin config; an existing config with the same (case
    /// insensitive) name is replaced.
    pub fn add_builtin(
        &mut self,
        name: &str,
        ui_name: &str,
        config: &'static str,
        is_recommended: bool,
    ) {
        let data = BuiltinConfigData {
            name: name.to_string(),
            ui_name: ui_name.to_string(),
            config,
            is_recommended,
        };
        if let Some(c) = self
            .configs
            .iter_mut()
            .find(|c| c.name.eq_ignore_ascii_case(name))
        {
            *c = data;
        } else {
            self.configs.push(data);
        }
    }

    /// Number of builtin configs.
    pub fn num_builtin_configs(&self) -> usize {
        self.configs.len()
    }

    /// Name of the config at `index`.
    pub fn builtin_config_name(&self, index: usize) -> Result<&str> {
        self.configs
            .get(index)
            .map(|c| c.name.as_str())
            .ok_or_else(out_of_range)
    }

    /// User-friendly name of the config at `index`.
    pub fn builtin_config_ui_name(&self, index: usize) -> Result<&str> {
        self.configs
            .get(index)
            .map(|c| c.ui_name.as_str())
            .ok_or_else(out_of_range)
    }

    /// YAML text of the config at `index`.
    pub fn builtin_config(&self, index: usize) -> Result<&'static str> {
        self.configs
            .get(index)
            .map(|c| c.config)
            .ok_or_else(out_of_range)
    }

    /// YAML text of the config with the given (case insensitive) name.
    pub fn builtin_config_by_name(&self, name: &str) -> Result<&'static str> {
        self.configs
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(name))
            .map(|c| c.config)
            .ok_or_else(|| {
                Error::msg(format!(
                    "Could not find '{name}' in the built-in configurations."
                ))
            })
    }

    /// True if the config at `index` is recommended.
    pub fn is_builtin_config_recommended(&self, index: usize) -> Result<bool> {
        self.configs
            .get(index)
            .map(|c| c.is_recommended)
            .ok_or_else(out_of_range)
    }

    /// Name of the default builtin config.
    pub fn default_builtin_config_name(&self) -> &'static str {
        default_builtin_config_name()
    }
}

// If a new builtin config is added, do not forget to update the
// LATEST_CG_BUILTIN_CONFIG_URI / LATEST_STUDIO_BUILTIN_CONFIG_URI constants.

fn register_cg_configs(registry: &mut BuiltinConfigRegistry) {
    // For backwards compatibility, previous versions are kept in the registry
    // but the recommended flag is set to false.
    registry.add_builtin(
        "cg-config-v1.0.0_aces-v1.3_ocio-v2.1",
        "Academy Color Encoding System - CG Config [COLORSPACES v1.0.0] [ACES v1.3] [OCIO v2.1]",
        CG_CONFIG_V100_ACES_V13_OCIO_V21,
        false,
    );
    registry.add_builtin(
        "cg-config-v2.1.0_aces-v1.3_ocio-v2.3",
        "Academy Color Encoding System - CG Config [COLORSPACES v2.0.0] [ACES v1.3] [OCIO v2.3]",
        CG_CONFIG_V210_ACES_V13_OCIO_V23,
        false,
    );
    registry.add_builtin(
        "cg-config-v2.2.0_aces-v1.3_ocio-v2.4",
        "Academy Color Encoding System - CG Config [COLORSPACES v2.2.0] [ACES v1.3] [OCIO v2.4]",
        CG_CONFIG_V220_ACES_V13_OCIO_V24,
        false,
    );
    registry.add_builtin(
        "cg-config-v4.0.0_aces-v2.0_ocio-v2.5",
        "Academy Color Encoding System - CG Config [COLORSPACES v4.0.0] [ACES v2.0] [OCIO v2.5]",
        CG_CONFIG_V400_ACES_V20_OCIO_V25,
        true,
    );
}

fn register_studio_configs(registry: &mut BuiltinConfigRegistry) {
    registry.add_builtin(
        "studio-config-v1.0.0_aces-v1.3_ocio-v2.1",
        "Academy Color Encoding System - Studio Config [COLORSPACES v1.0.0] [ACES v1.3] [OCIO v2.1]",
        STUDIO_CONFIG_V100_ACES_V13_OCIO_V21,
        false,
    );
    registry.add_builtin(
        "studio-config-v2.1.0_aces-v1.3_ocio-v2.3",
        "Academy Color Encoding System - Studio Config [COLORSPACES v2.0.0] [ACES v1.3] [OCIO v2.3]",
        STUDIO_CONFIG_V210_ACES_V13_OCIO_V23,
        false,
    );
    registry.add_builtin(
        "studio-config-v2.2.0_aces-v1.3_ocio-v2.4",
        "Academy Color Encoding System - Studio Config [COLORSPACES v2.2.0] [ACES v1.3] [OCIO v2.4]",
        STUDIO_CONFIG_V220_ACES_V13_OCIO_V24,
        false,
    );
    registry.add_builtin(
        "studio-config-v4.0.0_aces-v2.0_ocio-v2.5",
        "Academy Color Encoding System - Studio Config [COLORSPACES v4.0.0] [ACES v2.0] [OCIO v2.5]",
        STUDIO_CONFIG_V400_ACES_V20_OCIO_V25,
        true,
    );
}

/// The builtin config name following an `ocio://` prefix found in `s`
/// (port of the `ocio:\/\/([^\s]+)` regex search).
fn uri_match(s: &str) -> Option<&str> {
    let mut start = 0;
    while let Some(pos) = s[start..].find(OCIO_BUILTIN_URI_PREFIX) {
        let begin = start + pos + OCIO_BUILTIN_URI_PREFIX.len();
        let rest = &s[begin..];
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        if end > 0 {
            return Some(&rest[..end]);
        }
        start = begin;
    }
    None
}

/// Resolve the aliases `ocio://default`, `ocio://cg-config-latest` and
/// `ocio://studio-config-latest` into the URI of the corresponding builtin
/// config. Any other path is returned unmodified (port of
/// `ResolveConfigPath`).
pub fn resolve_config_path(original_path: &str) -> &str {
    if let Some(name) = uri_match(original_path) {
        if name.eq_ignore_ascii_case(BUILTIN_DEFAULT_NAME) {
            return DEFAULT_BUILTIN_CONFIG_URI;
        } else if name.eq_ignore_ascii_case(BUILTIN_LATEST_CG_NAME) {
            return LATEST_CG_BUILTIN_CONFIG_URI;
        } else if name.eq_ignore_ascii_case(BUILTIN_LATEST_STUDIO_NAME) {
            return LATEST_STUDIO_BUILTIN_CONFIG_URI;
        }
    }
    original_path
}

/// The registry name of a builtin config given as a name, an alias or an
/// `ocio://` URI (the normalization of `Config::CreateFromBuiltinConfig`).
pub fn resolve_builtin_config_name(name: &str) -> String {
    // Normalize the input to the URI format.
    let uri = if name.starts_with(OCIO_BUILTIN_URI_PREFIX) {
        name.to_string()
    } else {
        format!("{OCIO_BUILTIN_URI_PREFIX}{name}")
    };

    // Resolve the URI if needed.
    let resolved = resolve_config_path(&uri);

    // Store config path without the "ocio://" prefix, if present.
    match uri_match(resolved) {
        Some(n) => n.to_string(),
        None => name.to_string(),
    }
}

/// YAML text of the builtin config `name`, which may be a registry name
/// (e.g. `"cg-config-v2.2.0_aces-v1.3_ocio-v2.4"`, case insensitive), an
/// alias (`"default"`, `"cg-config-latest"`, `"studio-config-latest"`) or an
/// `ocio://` URI of either. Errors with OCIO's message if unknown.
pub fn builtin_config(name: &str) -> Result<&'static str> {
    BuiltinConfigRegistry::get().builtin_config_by_name(&resolve_builtin_config_name(name))
}

/// YAML text of the builtin config `name` (see [`builtin_config`]), or
/// `None` if unknown.
pub fn get_builtin_config(name: &str) -> Option<&'static str> {
    builtin_config(name).ok()
}

/// True if `path` is an `ocio://` builtin config URI (known or not).
pub fn is_builtin_config_uri(path: &str) -> bool {
    uri_match(path).is_some()
}

/// Names of all the builtin configs, in registry order.
pub fn builtin_config_names() -> Vec<&'static str> {
    BuiltinConfigRegistry::get()
        .configs
        .iter()
        .map(|c| c.name.as_str())
        .collect()
}

/// User-friendly names of all the builtin configs, in registry order.
pub fn builtin_config_ui_names() -> Vec<&'static str> {
    BuiltinConfigRegistry::get()
        .configs
        .iter()
        .map(|c| c.ui_name.as_str())
        .collect()
}

/// True if the builtin config `name` (name, alias or URI) is recommended.
pub fn is_builtin_config_recommended(name: &str) -> Option<bool> {
    let n = resolve_builtin_config_name(name);
    BuiltinConfigRegistry::get()
        .configs
        .iter()
        .find(|c| c.name.eq_ignore_ascii_case(&n))
        .map(|c| c.is_recommended)
}

/// Name of the default builtin config.
pub fn default_builtin_config_name() -> &'static str {
    DEFAULT_BUILTIN_CONFIG_URI.trim_start_matches(OCIO_BUILTIN_URI_PREFIX)
}

#[cfg(test)]
mod tests;
