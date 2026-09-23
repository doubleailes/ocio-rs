//! View transforms (port of `ViewTransform.cpp`).

use super::colorspace::find_interchange_key;
use super::tokens::TokensManager;
use super::transform_display::format_transform;
use crate::error::Result;
use crate::transforms::Transform;
use crate::types::{ReferenceSpaceType, ViewTransformDirection};
use std::collections::BTreeMap;
use std::fmt;

const KNOWN_INTERCHANGE_NAMES: [&str; 1] = ["amf_transform_ids"];

/// A view transform: converts between the scene (or display) reference
/// space and the display reference space.
#[derive(Debug, Clone, PartialEq)]
pub struct ViewTransform {
    name: String,
    family: String,
    description: String,
    reference_space: ReferenceSpaceType,
    interchange: BTreeMap<String, String>,
    to_reference: Option<Transform>,
    from_reference: Option<Transform>,
    categories: TokensManager,
}

impl ViewTransform {
    /// An empty view transform using the given reference space.
    pub fn new(reference_space: ReferenceSpaceType) -> Self {
        Self {
            name: String::new(),
            family: String::new(),
            description: String::new(),
            reference_space,
            interchange: BTreeMap::new(),
            to_reference: None,
            from_reference: None,
            categories: TokensManager::new(),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn set_name(&mut self, name: &str) {
        self.name = name.to_string();
    }
    pub fn family(&self) -> &str {
        &self.family
    }
    pub fn set_family(&mut self, family: &str) {
        self.family = family.to_string();
    }
    pub fn description(&self) -> &str {
        &self.description
    }
    pub fn set_description(&mut self, d: &str) {
        self.description = d.to_string();
    }

    /// Value of a known interchange attribute (`amf_transform_ids`).
    pub fn interchange_attribute(&self, name: &str) -> Result<&str> {
        let key = find_interchange_key(&KNOWN_INTERCHANGE_NAMES, name)?;
        Ok(self.interchange.get(key).map(|s| s.as_str()).unwrap_or(""))
    }

    /// Set (or remove if empty) a known interchange attribute.
    pub fn set_interchange_attribute(&mut self, name: &str, value: &str) -> Result<()> {
        let key = find_interchange_key(&KNOWN_INTERCHANGE_NAMES, name)?;
        if value.is_empty() {
            self.interchange.remove(key);
        } else {
            self.interchange.insert(key.to_string(), value.to_string());
        }
        Ok(())
    }

    pub fn interchange_attributes(&self) -> &BTreeMap<String, String> {
        &self.interchange
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

    pub fn reference_space_type(&self) -> ReferenceSpaceType {
        self.reference_space
    }

    /// The transform in the given direction, if any.
    pub fn transform(&self, dir: ViewTransformDirection) -> Option<&Transform> {
        match dir {
            ViewTransformDirection::ToReference => self.to_reference.as_ref(),
            ViewTransformDirection::FromReference => self.from_reference.as_ref(),
        }
    }

    /// Set (or remove with `None`) the transform in the given direction.
    pub fn set_transform(&mut self, t: Option<Transform>, dir: ViewTransformDirection) {
        match dir {
            ViewTransformDirection::ToReference => self.to_reference = t,
            ViewTransformDirection::FromReference => self.from_reference = t,
        }
    }
}

impl fmt::Display for ViewTransform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let rst = match self.reference_space {
            ReferenceSpaceType::Scene => "scene",
            ReferenceSpaceType::Display => "display",
        };
        write!(f, "<ViewTransform name={}, family={}, referenceSpaceType={}", self.name, self.family, rst)?;
        if !self.description.is_empty() {
            write!(f, ", description={}", self.description)?;
        }
        for (k, v) in &self.interchange {
            write!(f, ", {k}={v}")?;
        }
        if let Some(t) = &self.to_reference {
            write!(f, ",\n    {} --> Reference\n        {}", self.name, format_transform(t))?;
        }
        if let Some(t) = &self.from_reference {
            write!(f, ",\n    Reference --> {}\n        {}", self.name, format_transform(t))?;
        }
        write!(f, ">")
    }
}
