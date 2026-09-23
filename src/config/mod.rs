//! The configuration: color spaces, roles, displays / views, looks, rules
//! (TODO: port `Config.cpp`, `OCIOYaml.cpp`, `ColorSpace.cpp`, `Look.cpp`,
//! `FileRules.cpp`, `ViewingRules.cpp`, `NamedTransform.cpp`,
//! `ViewTransform.cpp`, `LookParse.cpp`, ...).
//!
//! The public signatures below are relied upon by other modules and must be
//! kept.

mod transforms;

use crate::context::Context;
use crate::error::{Error, Result};
use crate::processor::Processor;
use crate::transforms::Transform;
use crate::types::TransformDirection;

/// An OCIO configuration.
#[derive(Debug, Clone, Default)]
pub struct Config {
    context: Context,
}

impl Config {
    /// A minimal config with a single `raw` color space.
    pub fn create_raw() -> Config {
        Config::default()
    }

    /// Load the config pointed to by the `$OCIO` environment variable (or a
    /// raw config if unset).
    pub fn create_from_env() -> Result<Config> {
        Err(Error::msg("Config::create_from_env: not implemented"))
    }

    /// Load a config file (`.ocio`, `.ocioz` or `ocio://` builtin URI).
    pub fn create_from_file(_path: &str) -> Result<Config> {
        Err(Error::msg("Config::create_from_file: not implemented"))
    }

    /// Parse a config from a YAML string.
    pub fn create_from_str(_yaml: &str) -> Result<Config> {
        Err(Error::msg("Config::create_from_str: not implemented"))
    }

    /// Load one of the builtin configs (see `crate::builtins::configs`).
    pub fn create_from_builtin_config(_name: &str) -> Result<Config> {
        Err(Error::msg("Config::create_from_builtin_config: not implemented"))
    }

    /// Serialize to YAML.
    pub fn serialize(&self) -> Result<String> {
        Err(Error::msg("Config::serialize: not implemented"))
    }

    /// Validate the config.
    pub fn validate(&self) -> Result<()> {
        Ok(())
    }

    /// The config context (search paths, working dir, environment).
    pub fn current_context(&self) -> &Context {
        &self.context
    }

    /// True if a color space (or role) with this name exists.
    pub fn has_color_space(&self, _name: &str) -> bool {
        false
    }

    /// Processor converting between two color spaces (or roles).
    pub fn get_processor(&self, src: &str, dst: &str) -> Result<Processor> {
        let t = crate::transforms::ColorSpaceTransform::new(src, dst);
        self.get_processor_for_transform(&Transform::ColorSpace(t), TransformDirection::Forward)
    }

    /// Processor for a transform, using the current context.
    pub fn get_processor_for_transform(&self, transform: &Transform, dir: TransformDirection) -> Result<Processor> {
        self.get_processor_with_context(self.current_context(), transform, dir)
    }

    /// Processor for a transform, using the given context.
    pub fn get_processor_with_context(
        &self,
        context: &Context,
        transform: &Transform,
        dir: TransformDirection,
    ) -> Result<Processor> {
        Processor::from_transform(self, context, transform, dir)
    }

    /// Processor from a color space to a display / view.
    pub fn get_display_view_processor(&self, src: &str, display: &str, view: &str) -> Result<Processor> {
        let t = crate::transforms::DisplayViewTransform::new(src, display, view);
        self.get_processor_for_transform(&Transform::DisplayView(t), TransformDirection::Forward)
    }
}
