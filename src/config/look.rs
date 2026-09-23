//! Looks (port of `Look.cpp`).

use super::colorspace::find_interchange_key;
use super::transform_display::format_transform;
use crate::error::Result;
use crate::transforms::Transform;
use std::collections::BTreeMap;
use std::fmt;

const KNOWN_INTERCHANGE_NAMES: [&str; 1] = ["amf_transform_ids"];

/// A look: a named creative transform applied in a process color space.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Look {
    name: String,
    process_space: String,
    description: String,
    interchange: BTreeMap<String, String>,
    transform: Option<Transform>,
    inverse_transform: Option<Transform>,
}

impl Look {
    /// An empty look.
    pub fn new() -> Self {
        Self::default()
    }

    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn set_name(&mut self, name: &str) {
        self.name = name.to_string();
    }
    pub fn process_space(&self) -> &str {
        &self.process_space
    }
    pub fn set_process_space(&mut self, ps: &str) {
        self.process_space = ps.to_string();
    }
    /// The forward transform.
    pub fn transform(&self) -> Option<&Transform> {
        self.transform.as_ref()
    }
    pub fn set_transform(&mut self, t: Option<Transform>) {
        self.transform = t;
    }
    /// The (optional) explicit inverse transform.
    pub fn inverse_transform(&self) -> Option<&Transform> {
        self.inverse_transform.as_ref()
    }
    pub fn set_inverse_transform(&mut self, t: Option<Transform>) {
        self.inverse_transform = t;
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
}

impl fmt::Display for Look {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<Look name={}, processSpace={}", self.name, self.process_space)?;
        if !self.description.is_empty() {
            write!(f, ", description={}", self.description)?;
        }
        for (k, v) in &self.interchange {
            write!(f, ", {k}={v}")?;
        }
        if let Some(t) = &self.transform {
            write!(f, ",\n    transform=\n        {}", format_transform(t))?;
        }
        if let Some(t) = &self.inverse_transform {
            write!(f, ",\n    inverseTransform=\n        {}", format_transform(t))?;
        }
        write!(f, ">")
    }
}
