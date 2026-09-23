//! Viewing rules: filter the views of a display according to the color
//! space (or its encoding) of the image (port of `ViewingRules.cpp`).

use super::colorspace::{ColorSpace, ColorSpaceSet};
use super::logging::log_info;
use super::tokens::{CustomKeys, TokensManager};
use super::utils::{compare, lower, trim};
use crate::error::{Error, Result};
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
struct ViewingRule {
    name: String,
    color_spaces: TokensManager,
    encodings: TokensManager,
    custom_keys: CustomKeys,
}

impl ViewingRule {
    fn validate<'a>(&self, cs_accessor: &dyn Fn(&str) -> Option<&'a ColorSpace>, colorspaces: &ColorSpaceSet) -> Result<()> {
        for cs in self.color_spaces.tokens() {
            if cs_accessor(cs).is_none() {
                return Err(Error::msg(format!(
                    "The rule '{}' refers to color space '{}' which is not defined.",
                    self.name, cs
                )));
            }
        }
        for enc in self.encodings.tokens() {
            let t = lower(enc);
            if !colorspaces.iter().any(|c| lower(c.encoding()) == t) {
                log_info(&format!(
                    "The rule '{}' refers to encoding '{}' that is not used by any of the color spaces.",
                    self.name, enc
                ));
            }
        }
        let num_cs = self.color_spaces.num_tokens();
        let num_enc = self.encodings.num_tokens();
        if num_cs + num_enc == 0 {
            return Err(Error::msg(format!(
                "The rule '{}' must have either a color space or an encoding.",
                self.name
            )));
        } else if num_cs != 0 && num_enc != 0 {
            return Err(Error::msg(format!(
                "The rule '{}' cannot refer to both a color space and an encoding.",
                self.name
            )));
        }
        Ok(())
    }
}

/// The ordered list of viewing rules.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ViewingRules {
    rules: Vec<ViewingRule>,
}

impl ViewingRules {
    /// No rules.
    pub fn new() -> Self {
        Self::default()
    }

    fn validate_position(&self, idx: usize) -> Result<()> {
        if idx >= self.rules.len() {
            return Err(Error::msg(format!(
                "Viewing rules: rule index '{}' invalid. There are only '{}' rules.",
                idx,
                self.rules.len()
            )));
        }
        Ok(())
    }

    fn validate_new_rule(&self, name: &str) -> Result<()> {
        if name.is_empty() {
            return Err(Error::msg("Viewing rules: rule must have a non-empty name."));
        }
        if self.rules.iter().any(|r| compare(&r.name, name)) {
            return Err(Error::msg(format!("Viewing rules: A rule named '{name}' already exists.")));
        }
        Ok(())
    }

    /// Validate all the rules. `cs_accessor` resolves color space names (and
    /// roles).
    pub fn validate<'a>(&self, cs_accessor: &dyn Fn(&str) -> Option<&'a ColorSpace>, colorspaces: &ColorSpaceSet) -> Result<()> {
        for r in &self.rules {
            r.validate(cs_accessor, colorspaces)?;
        }
        Ok(())
    }

    pub fn num_entries(&self) -> usize {
        self.rules.len()
    }

    /// Index of the rule named `name` (case insensitive).
    pub fn index_for_rule(&self, name: &str) -> Result<usize> {
        self.rules
            .iter()
            .position(|r| compare(&r.name, name))
            .ok_or_else(|| Error::msg(format!("Viewing rules: rule name '{name}' not found.")))
    }

    pub fn name(&self, idx: usize) -> Result<&str> {
        self.validate_position(idx)?;
        Ok(&self.rules[idx].name)
    }

    pub fn num_color_spaces(&self, idx: usize) -> Result<usize> {
        self.validate_position(idx)?;
        Ok(self.rules[idx].color_spaces.num_tokens())
    }

    pub fn color_space(&self, idx: usize, cs_idx: usize) -> Result<&str> {
        self.validate_position(idx)?;
        let r = &self.rules[idx];
        let n = r.color_spaces.num_tokens();
        r.color_spaces.token(cs_idx).ok_or_else(|| {
            Error::msg(format!(
                "Viewing rules: rule '{}' at index '{}': colorspace index '{}' is invalid. There are only '{}' colorspaces.",
                r.name, idx, cs_idx, n
            ))
        })
    }

    pub fn add_color_space(&mut self, idx: usize, cs: &str) -> Result<()> {
        self.validate_position(idx)?;
        let r = &mut self.rules[idx];
        if cs.is_empty() {
            return Err(Error::msg(format!(
                "Viewing rules: rule '{}' at index '{}': colorspace should have a non-empty name.",
                r.name, idx
            )));
        }
        if r.encodings.num_tokens() != 0 {
            return Err(Error::msg(format!(
                "Viewing rules: rule '{}' at index '{}': colorspace can't be added if there are encodings.",
                r.name, idx
            )));
        }
        r.color_spaces.add_token(cs);
        Ok(())
    }

    pub fn remove_color_space(&mut self, idx: usize, cs_idx: usize) -> Result<()> {
        let cs = self.color_space(idx, cs_idx)?.to_string();
        self.rules[idx].color_spaces.remove_token(&cs);
        Ok(())
    }

    pub fn num_encodings(&self, idx: usize) -> Result<usize> {
        self.validate_position(idx)?;
        Ok(self.rules[idx].encodings.num_tokens())
    }

    pub fn encoding(&self, idx: usize, enc_idx: usize) -> Result<&str> {
        self.validate_position(idx)?;
        let r = &self.rules[idx];
        let n = r.encodings.num_tokens();
        r.encodings.token(enc_idx).ok_or_else(|| {
            Error::msg(format!(
                "Viewing rules: rule '{}' at index '{}': encoding index '{}' is invalid. There are only '{}' encodings.",
                r.name, idx, enc_idx, n
            ))
        })
    }

    pub fn add_encoding(&mut self, idx: usize, enc: &str) -> Result<()> {
        self.validate_position(idx)?;
        let r = &mut self.rules[idx];
        if enc.is_empty() {
            return Err(Error::msg(format!(
                "Viewing rules: rule '{}' at index '{}': encoding should have a non-empty name.",
                r.name, idx
            )));
        }
        if r.color_spaces.num_tokens() != 0 {
            return Err(Error::msg(format!(
                "Viewing rules: rule '{}' at index '{}': encoding can't be added if there are colorspaces.",
                r.name, idx
            )));
        }
        r.encodings.add_token(enc);
        Ok(())
    }

    pub fn remove_encoding(&mut self, idx: usize, enc_idx: usize) -> Result<()> {
        let e = self.encoding(idx, enc_idx)?.to_string();
        self.rules[idx].encodings.remove_token(&e);
        Ok(())
    }

    pub fn num_custom_keys(&self, idx: usize) -> Result<usize> {
        self.validate_position(idx)?;
        Ok(self.rules[idx].custom_keys.len())
    }

    pub fn custom_key_name(&self, idx: usize, key: usize) -> Result<&str> {
        self.validate_position(idx)?;
        let r = &self.rules[idx];
        r.custom_keys
            .name(key)
            .map_err(|e| Error::msg(format!("Viewing rules: rule named '{}' error: {}", r.name, e)))
    }

    pub fn custom_key_value(&self, idx: usize, key: usize) -> Result<&str> {
        self.validate_position(idx)?;
        let r = &self.rules[idx];
        r.custom_keys
            .value(key)
            .map_err(|e| Error::msg(format!("Viewing rules: rule named '{}' error: {}", r.name, e)))
    }

    pub fn set_custom_key(&mut self, idx: usize, key: &str, value: &str) -> Result<()> {
        self.validate_position(idx)?;
        let r = &mut self.rules[idx];
        let name = r.name.clone();
        r.custom_keys
            .set(key, value)
            .map_err(|e| Error::msg(format!("Viewing rules: rule named '{name}' error: {e}")))
    }

    /// Insert a new rule at `idx` (which may be the number of rules).
    pub fn insert_rule(&mut self, idx: usize, name: &str) -> Result<()> {
        let name = trim(name).to_string();
        self.validate_new_rule(&name)?;
        let rule = ViewingRule {
            name,
            color_spaces: TokensManager::new(),
            encodings: TokensManager::new(),
            custom_keys: CustomKeys::default(),
        };
        if idx == self.rules.len() {
            self.rules.push(rule);
        } else {
            self.validate_position(idx)?;
            self.rules.insert(idx, rule);
        }
        Ok(())
    }

    pub fn remove_rule(&mut self, idx: usize) -> Result<()> {
        self.validate_position(idx)?;
        self.rules.remove(idx);
        Ok(())
    }

    /// Find a rule by name (`FindRule`).
    pub fn find_rule(&self, name: &str) -> Option<usize> {
        self.rules.iter().position(|r| compare(&r.name, name))
    }
}

impl fmt::Display for ViewingRules {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let n = self.rules.len();
        for (i, r) in self.rules.iter().enumerate() {
            write!(f, "<ViewingRule name={}", r.name)?;
            if r.color_spaces.num_tokens() > 0 {
                write!(f, ", colorspaces=[{}]", r.color_spaces.tokens().join(", "))?;
            }
            if r.encodings.num_tokens() > 0 {
                write!(f, ", encodings=[{}]", r.encodings.tokens().join(", "))?;
            }
            if !r.custom_keys.is_empty() {
                let keys: Vec<String> = r.custom_keys.iter().map(|(k, v)| format!("({k}, {v})")).collect();
                write!(f, ", customKeys=[{}]", keys.join(", "))?;
            }
            write!(f, ">")?;
            if i + 1 != n {
                writeln!(f)?;
            }
        }
        Ok(())
    }
}
