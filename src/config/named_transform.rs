//! Named transforms (port of `NamedTransform.cpp`).

use super::tokens::TokensManager;
use super::transform_display::format_transform;
use super::utils::{compare, contain, join, remove};
use crate::error::{Error, Result};
use crate::transforms::{GroupTransform, Transform};
use crate::types::TransformDirection;
use std::fmt;

/// A named transform: a transform that can be used like a color space but
/// without reference to the config reference spaces.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NamedTransform {
    name: String,
    aliases: Vec<String>,
    forward: Option<Transform>,
    inverse: Option<Transform>,
    family: String,
    description: String,
    categories: TokensManager,
    encoding: String,
}

impl NamedTransform {
    /// An empty named transform.
    pub fn new() -> Self {
        Self::default()
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Set the name (an alias with the same name is removed).
    pub fn set_name(&mut self, name: &str) {
        self.name = name.to_string();
        let n = self.name.clone();
        remove(&mut self.aliases, &n);
    }

    pub fn num_aliases(&self) -> usize {
        self.aliases.len()
    }
    pub fn alias(&self, idx: usize) -> &str {
        self.aliases.get(idx).map(|s| s.as_str()).unwrap_or("")
    }
    pub fn aliases(&self) -> &[String] {
        &self.aliases
    }
    pub fn has_alias(&self, alias: &str) -> bool {
        !alias.is_empty() && self.aliases.iter().any(|a| compare(a, alias))
    }
    pub fn add_alias(&mut self, alias: &str) {
        if !alias.is_empty() && !compare(alias, &self.name) && !contain(&self.aliases, alias) {
            self.aliases.push(alias.to_string());
        }
    }
    pub fn remove_alias(&mut self, alias: &str) {
        if !alias.is_empty() {
            remove(&mut self.aliases, alias);
        }
    }
    pub fn clear_aliases(&mut self) {
        self.aliases.clear();
    }
    pub fn family(&self) -> &str {
        &self.family
    }
    pub fn set_family(&mut self, f: &str) {
        self.family = f.to_string();
    }
    pub fn description(&self) -> &str {
        &self.description
    }
    pub fn set_description(&mut self, d: &str) {
        self.description = d.to_string();
    }
    pub fn has_category(&self, c: &str) -> bool {
        self.categories.has_token(c)
    }
    pub fn add_category(&mut self, c: &str) {
        self.categories.add_token(c);
    }
    pub fn remove_category(&mut self, c: &str) {
        self.categories.remove_token(c);
    }
    pub fn num_categories(&self) -> usize {
        self.categories.num_tokens()
    }
    pub fn category(&self, index: usize) -> Option<&str> {
        self.categories.token(index)
    }
    pub fn categories(&self) -> &[String] {
        self.categories.tokens()
    }
    pub fn clear_categories(&mut self) {
        self.categories.clear_tokens();
    }
    pub fn encoding(&self) -> &str {
        &self.encoding
    }
    pub fn set_encoding(&mut self, e: &str) {
        self.encoding = e.to_string();
    }

    /// The transform defined for the given direction, if any.
    pub fn transform(&self, dir: TransformDirection) -> Option<&Transform> {
        match dir {
            TransformDirection::Forward => self.forward.as_ref(),
            TransformDirection::Inverse => self.inverse.as_ref(),
        }
    }

    /// Set (or remove) the transform of the given direction.
    pub fn set_transform(&mut self, t: Option<Transform>, dir: TransformDirection) {
        match dir {
            TransformDirection::Forward => self.forward = t,
            TransformDirection::Inverse => self.inverse = t,
        }
    }

    /// The transform to use for `dir`: the one defined for that direction or
    /// the inverse of the other one (`NamedTransform::GetTransform`).
    pub fn get_transform(nt: &NamedTransform, dir: TransformDirection) -> Result<Transform> {
        let (primary, other) = match dir {
            TransformDirection::Forward => (&nt.forward, &nt.inverse),
            TransformDirection::Inverse => (&nt.inverse, &nt.forward),
        };
        if let Some(t) = primary {
            return Ok(t.clone());
        }
        if let Some(t) = other {
            return Ok(t.inverted());
        }
        Err(Error::msg("Named transform: Unspecified TransformDirection."))
    }
}

/// Transform for a conversion involving named transforms (`GetTransform`).
pub fn get_named_transforms_transform(src: Option<&NamedTransform>, dst: Option<&NamedTransform>) -> Result<Transform> {
    match (src, dst) {
        (Some(s), Some(d)) => {
            let mut g = GroupTransform::new();
            g.transforms.push(NamedTransform::get_transform(s, TransformDirection::Forward)?);
            g.transforms.push(NamedTransform::get_transform(d, TransformDirection::Inverse)?);
            Ok(Transform::Group(g))
        }
        (Some(s), None) => NamedTransform::get_transform(s, TransformDirection::Forward),
        (None, Some(d)) => NamedTransform::get_transform(d, TransformDirection::Inverse),
        (None, None) => Err(Error::msg("GetTransform: one of the parameters has to be not null.")),
    }
}

impl fmt::Display for NamedTransform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<NamedTransform name={}", self.name)?;
        if self.aliases.len() == 1 {
            write!(f, ", alias= {}", self.aliases[0])?;
        } else if self.aliases.len() > 1 {
            write!(f, ", aliases=[{}]", self.aliases.join(", "))?;
        }
        if !self.family.is_empty() {
            write!(f, ", family={}", self.family)?;
        }
        if self.num_categories() > 0 {
            write!(f, ", categories=[{}]", join(self.categories.tokens(), ','))?;
        }
        if !self.description.is_empty() {
            write!(f, ", description={}", self.description)?;
        }
        if !self.encoding.is_empty() {
            write!(f, ", encoding={}", self.encoding)?;
        }
        if let Some(t) = &self.forward {
            write!(f, ",\n    forward=\n        {}", format_transform(t))?;
        }
        if let Some(t) = &self.inverse {
            write!(f, ",\n    inverse=\n        {}", format_transform(t))?;
        }
        write!(f, ">")
    }
}
