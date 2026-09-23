//! Rich metadata attached to transforms (port of `FormatMetadata`).
//!
//! Mirrors the XML-like structure used by CLF/CTF: an element has a name, a
//! value, attributes and child elements.

use crate::types::{METADATA_ID, METADATA_NAME};
use std::fmt;

/// Name of the root metadata element.
pub const METADATA_ROOT: &str = "ROOT";

/// Hierarchical metadata element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatMetadata {
    pub element_name: String,
    pub element_value: String,
    pub attributes: Vec<(String, String)>,
    pub children: Vec<FormatMetadata>,
}

impl Default for FormatMetadata {
    fn default() -> Self {
        Self::new(METADATA_ROOT, "")
    }
}

impl FormatMetadata {
    /// Create an element with a name and a value.
    pub fn new(name: &str, value: &str) -> Self {
        Self {
            element_name: name.to_string(),
            element_value: value.to_string(),
            attributes: Vec::new(),
            children: Vec::new(),
        }
    }

    pub fn element_name(&self) -> &str {
        &self.element_name
    }
    pub fn set_element_name(&mut self, name: &str) {
        self.element_name = name.to_string();
    }
    pub fn element_value(&self) -> &str {
        &self.element_value
    }
    pub fn set_element_value(&mut self, value: &str) {
        self.element_value = value.to_string();
    }

    /// Value of attribute `name`, or `""` if absent.
    pub fn attribute_value(&self, name: &str) -> &str {
        self.attributes
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
            .unwrap_or("")
    }

    /// Add or replace an attribute.
    pub fn add_attribute(&mut self, name: &str, value: &str) {
        if let Some(a) = self.attributes.iter_mut().find(|(n, _)| n == name) {
            a.1 = value.to_string();
        } else {
            self.attributes.push((name.to_string(), value.to_string()));
        }
    }

    /// Append a child element and return a mutable reference to it.
    pub fn add_child_element(&mut self, name: &str, value: &str) -> &mut FormatMetadata {
        self.children.push(FormatMetadata::new(name, value));
        self.children.last_mut().unwrap()
    }

    /// All children with the given element name.
    pub fn children_named<'a>(
        &'a self,
        name: &'a str,
    ) -> impl Iterator<Item = &'a FormatMetadata> + 'a {
        self.children.iter().filter(move |c| c.element_name == name)
    }

    /// Index of first child with the given name.
    pub fn first_child_index(&self, name: &str) -> Option<usize> {
        self.children.iter().position(|c| c.element_name == name)
    }

    /// Remove value, attributes and children (keeps the element name).
    pub fn clear(&mut self) {
        self.element_value.clear();
        self.attributes.clear();
        self.children.clear();
    }

    /// The `name` attribute.
    pub fn name(&self) -> &str {
        self.attribute_value(METADATA_NAME)
    }
    pub fn set_name(&mut self, name: &str) {
        self.add_attribute(METADATA_NAME, name);
    }
    /// The `id` attribute.
    pub fn id(&self) -> &str {
        self.attribute_value(METADATA_ID)
    }
    pub fn set_id(&mut self, id: &str) {
        self.add_attribute(METADATA_ID, id);
    }

    /// Merge `rhs` into `self` (used when combining ops). Attributes of `rhs`
    /// that already exist are concatenated with ` + `, children are appended.
    pub fn combine(&mut self, rhs: &FormatMetadata) {
        if std::ptr::eq(self, rhs) {
            return;
        }
        for (n, v) in &rhs.attributes {
            if let Some(a) = self.attributes.iter_mut().find(|(an, _)| an == n) {
                if !v.is_empty() && a.1 != *v {
                    if a.1.is_empty() {
                        a.1 = v.clone();
                    } else {
                        a.1 = format!("{} + {}", a.1, v);
                    }
                }
            } else {
                self.attributes.push((n.clone(), v.clone()));
            }
        }
        self.children.extend(rhs.children.iter().cloned());
    }

    /// True if the element has no value, attributes, nor children.
    pub fn is_empty(&self) -> bool {
        self.element_value.is_empty() && self.attributes.is_empty() && self.children.is_empty()
    }
}

impl fmt::Display for FormatMetadata {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<{}", self.element_name)?;
        for (n, v) in &self.attributes {
            write!(f, " {}=\"{}\"", n, v)?;
        }
        write!(f, ">")?;
        if !self.element_value.is_empty() {
            write!(f, "{}", self.element_value)?;
        }
        for c in &self.children {
            write!(f, "{}", c)?;
        }
        write!(f, "</{}>", self.element_name)
    }
}
