//! Color spaces and sets of color spaces (port of `ColorSpace.cpp` and
//! `ColorSpaceSet.cpp`).

use super::tokens::TokensManager;
use super::transform_display::format_transform;
use super::utils::{compare, contain, fmt_f32, join, lower, remove};
use crate::error::{Error, Result};
use crate::transforms::Transform;
use crate::types::{Allocation, BitDepth, ColorSpaceDirection, ReferenceSpaceType};
use std::collections::BTreeMap;
use std::fmt;

const KNOWN_INTERCHANGE_NAMES: [&str; 2] = ["amf_transform_ids", "icc_profile_name"];

/// Look up a known interchange attribute name (case insensitive).
pub(crate) fn find_interchange_key(known: &[&'static str], name: &str) -> Result<&'static str> {
    known
        .iter()
        .find(|k| compare(k, name))
        .copied()
        .ok_or_else(|| Error::msg(format!("Unknown attribute name '{name}'.")))
}

/// A color space: a named encoding of colors plus the transforms to and from
/// the reference space.
#[derive(Debug, Clone, PartialEq)]
pub struct ColorSpace {
    name: String,
    family: String,
    equality_group: String,
    description: String,
    encoding: String,
    interop_id: String,
    aliases: Vec<String>,
    interchange: BTreeMap<String, String>,
    bit_depth: BitDepth,
    is_data: bool,
    reference_space: ReferenceSpaceType,
    allocation: Allocation,
    allocation_vars: Vec<f32>,
    to_reference: Option<Transform>,
    from_reference: Option<Transform>,
    categories: TokensManager,
}

impl Default for ColorSpace {
    fn default() -> Self {
        Self::new(ReferenceSpaceType::Scene)
    }
}

impl ColorSpace {
    /// An empty color space using the given reference space.
    pub fn new(reference_space: ReferenceSpaceType) -> Self {
        Self {
            name: String::new(),
            family: String::new(),
            equality_group: String::new(),
            description: String::new(),
            encoding: String::new(),
            interop_id: String::new(),
            aliases: Vec::new(),
            interchange: BTreeMap::new(),
            bit_depth: BitDepth::Unknown,
            is_data: false,
            reference_space,
            allocation: Allocation::Uniform,
            allocation_vars: Vec::new(),
            to_reference: None,
            from_reference: None,
            categories: TokensManager::new(),
        }
    }

    /// A color space with a name, using the scene reference space.
    pub fn with_name(name: &str) -> Self {
        let mut cs = Self::new(ReferenceSpaceType::Scene);
        cs.set_name(name);
        cs
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

    /// Alias at `idx` (`""` if out of range).
    pub fn alias(&self, idx: usize) -> &str {
        self.aliases.get(idx).map(|s| s.as_str()).unwrap_or("")
    }

    pub fn aliases(&self) -> &[String] {
        &self.aliases
    }

    pub fn has_alias(&self, alias: &str) -> bool {
        self.aliases.iter().any(|a| compare(a, alias))
    }

    /// Add an alias (ignored if empty, equal to the name or already present).
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
    pub fn set_family(&mut self, family: &str) {
        self.family = family.to_string();
    }
    pub fn equality_group(&self) -> &str {
        &self.equality_group
    }
    pub fn set_equality_group(&mut self, group: &str) {
        self.equality_group = group.to_string();
    }
    pub fn description(&self) -> &str {
        &self.description
    }
    pub fn set_description(&mut self, description: &str) {
        self.description = description.to_string();
    }

    pub fn interop_id(&self) -> &str {
        &self.interop_id
    }

    /// Set the interop id, validating its syntax.
    pub fn set_interop_id(&mut self, id: &str) -> Result<()> {
        if !id.is_empty() {
            let allowed = |c: char| {
                c.is_ascii_digit()
                    || c.is_ascii_lowercase()
                    || matches!(
                        c,
                        '.' | '-'
                            | '_'
                            | '~'
                            | '/'
                            | '*'
                            | '#'
                            | '%'
                            | '^'
                            | '+'
                            | '('
                            | ')'
                            | '['
                            | ']'
                            | '|'
                            | ':'
                    )
            };
            if !id.chars().all(allowed) {
                return Err(Error::msg(format!(
                    "InteropID '{id}' contains invalid characters. Only lowercase a-z, 0-9 and . - _ ~ / * # % ^ + ( ) [ ] | are allowed.\n"
                )));
            }
            if let Some(pos) = id.find(':') {
                let ns = &id[..pos];
                let cs = &id[pos + 1..];
                if ns.is_empty() || cs.is_empty() {
                    return Err(Error::msg(format!(
                        "InteropID '{id}' is not valid. If ':' is used, both the namespace and the color space parts must be non-empty.\n"
                    )));
                }
                if cs.contains(':') {
                    return Err(Error::msg(format!(
                        "ERROR: InteropID '{id}' is not valid. Only one ':' is allowed to separate the namespace and the color space.\n"
                    )));
                }
            }
        }
        self.interop_id = id.to_string();
        Ok(())
    }

    /// Value of a known interchange attribute (`amf_transform_ids`,
    /// `icc_profile_name`).
    pub fn interchange_attribute(&self, name: &str) -> Result<&str> {
        let key = find_interchange_key(&KNOWN_INTERCHANGE_NAMES, name)?;
        Ok(self.interchange.get(key).map(|s| s.as_str()).unwrap_or(""))
    }

    /// Set (or remove if `value` is empty) a known interchange attribute.
    pub fn set_interchange_attribute(&mut self, name: &str, value: &str) -> Result<()> {
        let key = find_interchange_key(&KNOWN_INTERCHANGE_NAMES, name)?;
        if value.is_empty() {
            self.interchange.remove(key);
        } else {
            self.interchange.insert(key.to_string(), value.to_string());
        }
        Ok(())
    }

    /// All the interchange attributes.
    pub fn interchange_attributes(&self) -> &BTreeMap<String, String> {
        &self.interchange
    }

    pub fn bit_depth(&self) -> BitDepth {
        self.bit_depth
    }
    pub fn set_bit_depth(&mut self, bd: BitDepth) {
        self.bit_depth = bd;
    }
    pub fn reference_space_type(&self) -> ReferenceSpaceType {
        self.reference_space
    }

    pub fn has_category(&self, category: &str) -> bool {
        self.categories.has_token(category)
    }
    pub fn add_category(&mut self, category: &str) {
        self.categories.add_token(category);
    }
    pub fn remove_category(&mut self, category: &str) {
        self.categories.remove_token(category);
    }
    pub fn num_categories(&self) -> usize {
        self.categories.num_tokens()
    }
    /// Category at `index` (`None` if out of range).
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
    pub fn set_encoding(&mut self, encoding: &str) {
        self.encoding = encoding.to_string();
    }
    pub fn is_data(&self) -> bool {
        self.is_data
    }
    pub fn set_is_data(&mut self, is_data: bool) {
        self.is_data = is_data;
    }
    pub fn allocation(&self) -> Allocation {
        self.allocation
    }
    pub fn set_allocation(&mut self, allocation: Allocation) {
        self.allocation = allocation;
    }
    pub fn allocation_num_vars(&self) -> usize {
        self.allocation_vars.len()
    }
    pub fn allocation_vars(&self) -> &[f32] {
        &self.allocation_vars
    }
    pub fn set_allocation_vars(&mut self, vars: &[f32]) {
        self.allocation_vars = vars.to_vec();
    }

    /// The transform in the given direction, if any.
    pub fn transform(&self, dir: ColorSpaceDirection) -> Option<&Transform> {
        match dir {
            ColorSpaceDirection::ToReference => self.to_reference.as_ref(),
            ColorSpaceDirection::FromReference => self.from_reference.as_ref(),
        }
    }

    /// Set (or remove with `None`) the transform in the given direction.
    pub fn set_transform(&mut self, t: Option<Transform>, dir: ColorSpaceDirection) {
        match dir {
            ColorSpaceDirection::ToReference => self.to_reference = t,
            ColorSpaceDirection::FromReference => self.from_reference = t,
        }
    }
}

impl fmt::Display for ColorSpace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<ColorSpace referenceSpaceType=")?;
        match self.reference_space {
            ReferenceSpaceType::Scene => write!(f, "scene, ")?,
            ReferenceSpaceType::Display => write!(f, "display, ")?,
        }
        write!(f, "name={}, ", self.name)?;
        if self.aliases.len() == 1 {
            write!(f, "alias= {}, ", self.aliases[0])?;
        } else if self.aliases.len() > 1 {
            write!(f, "aliases=[{}], ", self.aliases.join(", "))?;
        }
        if !self.interop_id.is_empty() {
            write!(f, "interop_id={}, ", self.interop_id)?;
        }
        if !self.family.is_empty() {
            write!(f, "family={}, ", self.family)?;
        }
        if !self.equality_group.is_empty() {
            write!(f, "equalityGroup={}, ", self.equality_group)?;
        }
        if self.bit_depth != BitDepth::Unknown {
            write!(f, "bitDepth={}, ", self.bit_depth.as_str())?;
        }
        write!(f, "isData={}", crate::types::bool_to_string(self.is_data))?;
        if !self.allocation_vars.is_empty() {
            write!(f, ", allocation={}, ", self.allocation.as_str())?;
            let vars: Vec<String> = self.allocation_vars.iter().map(|v| fmt_f32(*v)).collect();
            write!(f, "vars={}", vars.join(" "))?;
        }
        if self.num_categories() > 0 {
            write!(f, ", categories={}", join(self.categories.tokens(), ','))?;
        }
        if !self.encoding.is_empty() {
            write!(f, ", encoding={}", self.encoding)?;
        }
        if !self.description.is_empty() {
            write!(f, ", description={}", self.description)?;
        }
        for (k, v) in &self.interchange {
            write!(f, ", {k}={v}")?;
        }
        if let Some(t) = &self.to_reference {
            write!(f, ",\n    {} --> Reference", self.name)?;
            write!(f, "\n        {}", format_transform(t))?;
        }
        if let Some(t) = &self.from_reference {
            write!(f, ",\n    Reference --> {}", self.name)?;
            write!(f, "\n        {}", format_transform(t))?;
        }
        write!(f, ">")
    }
}

/// An ordered set of color spaces (names and aliases are unique, case
/// insensitive).
#[derive(Debug, Clone, Default)]
pub struct ColorSpaceSet {
    color_spaces: Vec<ColorSpace>,
}

impl PartialEq for ColorSpaceSet {
    /// Only the names are compared.
    fn eq(&self, other: &Self) -> bool {
        self.color_spaces.len() == other.color_spaces.len()
            && self
                .color_spaces
                .iter()
                .all(|cs| other.has_color_space(cs.name()))
    }
}

impl ColorSpaceSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn num_color_spaces(&self) -> usize {
        self.color_spaces.len()
    }

    /// Name of the color space at `index`.
    pub fn color_space_name_by_index(&self, index: usize) -> Option<&str> {
        self.color_spaces.get(index).map(|c| c.name())
    }

    pub fn color_space_by_index(&self, index: usize) -> Option<&ColorSpace> {
        self.color_spaces.get(index)
    }

    /// Color space by name or alias.
    pub fn color_space(&self, name: &str) -> Option<&ColorSpace> {
        self.color_space_index(name)
            .and_then(|i| self.color_spaces.get(i))
    }

    /// Index of the color space by name or alias.
    pub fn color_space_index(&self, name: &str) -> Option<usize> {
        if name.is_empty() {
            return None;
        }
        let s = lower(name);
        self.color_spaces
            .iter()
            .position(|cs| compare(cs.name(), &s) || cs.aliases.iter().any(|a| compare(a, &s)))
    }

    pub fn has_color_space(&self, name: &str) -> bool {
        self.color_space_index(name).is_some()
    }

    /// Add a copy of the color space, replacing a color space with the same
    /// name.
    pub fn add_color_space(&mut self, cs: &ColorSpace) -> Result<()> {
        let cs_name = cs.name();
        if cs_name.is_empty() {
            return Err(Error::msg("Cannot add a color space with an empty name."));
        }
        let mut replace = None;
        if let Some(idx) = self.color_space_index(cs_name) {
            if !compare(self.color_spaces[idx].name(), cs_name) {
                return Err(Error::msg(format!(
                    "Cannot add '{}' color space, existing color space, '{}' is using this name as an alias.",
                    cs_name,
                    self.color_spaces[idx].name()
                )));
            }
            replace = Some(idx);
        }
        for alias in &cs.aliases {
            if let Some(idx) = self.color_space_index(alias) {
                if Some(idx) != replace {
                    return Err(Error::msg(format!(
                        "Cannot add '{}' color space, it has '{}' alias and existing color space, '{}' is using the same alias.",
                        cs_name,
                        alias,
                        self.color_spaces[idx].name()
                    )));
                }
            }
        }
        match replace {
            Some(i) => self.color_spaces[i] = cs.clone(),
            None => self.color_spaces.push(cs.clone()),
        }
        Ok(())
    }

    /// Add all the color spaces of another set.
    pub fn add_color_spaces(&mut self, other: &ColorSpaceSet) -> Result<()> {
        for cs in &other.color_spaces {
            self.add_color_space(cs)?;
        }
        Ok(())
    }

    /// Remove a color space by name (aliases are not considered).
    pub fn remove_color_space(&mut self, name: &str) {
        if name.is_empty() {
            return;
        }
        let n = lower(name);
        if let Some(pos) = self
            .color_spaces
            .iter()
            .position(|cs| lower(cs.name()) == n)
        {
            self.color_spaces.remove(pos);
        }
    }

    /// Remove all the color spaces of another set.
    pub fn remove_color_spaces(&mut self, other: &ColorSpaceSet) {
        for cs in &other.color_spaces {
            self.remove_color_space(cs.name());
        }
    }

    pub fn clear_color_spaces(&mut self) {
        self.color_spaces.clear();
    }

    /// Iterate over the color spaces.
    pub fn iter(&self) -> impl Iterator<Item = &ColorSpace> {
        self.color_spaces.iter()
    }

    /// Union (`operator||`).
    pub fn union(&self, other: &ColorSpaceSet) -> Result<ColorSpaceSet> {
        let mut s = self.clone();
        s.add_color_spaces(other)?;
        Ok(s)
    }

    /// Intersection (`operator&&`): the color spaces of `other` present in `self`.
    pub fn intersection(&self, other: &ColorSpaceSet) -> Result<ColorSpaceSet> {
        let mut s = ColorSpaceSet::new();
        for cs in &other.color_spaces {
            if self.has_color_space(cs.name()) {
                s.add_color_space(cs)?;
            }
        }
        Ok(s)
    }

    /// Difference (`operator-`): the color spaces of `self` not in `other`.
    pub fn difference(&self, other: &ColorSpaceSet) -> Result<ColorSpaceSet> {
        let mut s = ColorSpaceSet::new();
        for cs in &self.color_spaces {
            if !other.has_color_space(cs.name()) {
                s.add_color_space(cs)?;
            }
        }
        Ok(s)
    }
}
