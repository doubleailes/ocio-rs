//! Small containers shared by the config objects: category tokens (port of
//! `TokensManager.h`) and custom keys (port of `CustomKeys.h`).

use super::utils::{lower, trim};
use crate::error::{Error, Result};
use std::collections::BTreeMap;

/// A list of case-insensitive tokens (categories, encodings, ...). Tokens are
/// compared after trimming and lower-casing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TokensManager {
    tokens: Vec<String>,
}

impl TokensManager {
    /// An empty list.
    pub fn new() -> Self {
        Self::default()
    }

    fn find(&self, token: &str) -> Option<usize> {
        if token.is_empty() {
            return None;
        }
        let r = lower(trim(token));
        self.tokens.iter().position(|t| lower(trim(t)) == r)
    }

    /// True if the token exists.
    pub fn has_token(&self, token: &str) -> bool {
        self.find(token).is_some()
    }

    /// Add a token (trimmed) if not already present.
    pub fn add_token(&mut self, token: &str) {
        if token.is_empty() {
            return;
        }
        if self.find(token).is_none() {
            self.tokens.push(trim(token).to_string());
        }
    }

    /// Remove a token.
    pub fn remove_token(&mut self, token: &str) {
        if let Some(i) = self.find(token) {
            self.tokens.remove(i);
        }
    }

    /// Number of tokens.
    pub fn num_tokens(&self) -> usize {
        self.tokens.len()
    }

    /// Token at `index`, `None` if out of range.
    pub fn token(&self, index: usize) -> Option<&str> {
        self.tokens.get(index).map(|s| s.as_str())
    }

    /// All the tokens.
    pub fn tokens(&self) -> &[String] {
        &self.tokens
    }

    /// Remove all the tokens.
    pub fn clear_tokens(&mut self) {
        self.tokens.clear();
    }
}

/// Sorted key / value pairs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CustomKeys {
    keys: BTreeMap<String, String>,
}

impl CustomKeys {
    /// Number of keys.
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// True if there are no keys.
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    fn validate_index(&self, idx: usize) -> Result<()> {
        if idx >= self.keys.len() {
            return Err(Error::msg(format!(
                "Key index '{}' is invalid, there are '{}' custom keys.",
                idx,
                self.keys.len()
            )));
        }
        Ok(())
    }

    /// Name of the key at `idx`.
    pub fn name(&self, idx: usize) -> Result<&str> {
        self.validate_index(idx)?;
        Ok(self.keys.keys().nth(idx).map(|s| s.as_str()).unwrap_or(""))
    }

    /// Value of the key at `idx`.
    pub fn value(&self, idx: usize) -> Result<&str> {
        self.validate_index(idx)?;
        Ok(self.keys.values().nth(idx).map(|s| s.as_str()).unwrap_or(""))
    }

    /// Set (or remove when `value` is empty) a key.
    pub fn set(&mut self, key: &str, value: &str) -> Result<()> {
        if key.is_empty() {
            return Err(Error::msg("Key has to be a non-empty string."));
        }
        if value.is_empty() {
            self.keys.remove(key);
        } else {
            self.keys.insert(key.to_string(), value.to_string());
        }
        Ok(())
    }

    /// True if the key exists.
    pub fn has_key(&self, key: &str) -> bool {
        !key.is_empty() && self.keys.contains_key(key)
    }

    /// Value of a key.
    pub fn value_for_key(&self, key: &str) -> Result<&str> {
        if key.is_empty() {
            return Err(Error::msg("Key has to be a non-empty string."));
        }
        self.keys
            .get(key)
            .map(|s| s.as_str())
            .ok_or_else(|| Error::msg(format!("Key '{key}' not found.")))
    }

    /// Iterate over (key, value) in key order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.keys.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens() {
        let mut t = TokensManager::new();
        t.add_token(" Linear ");
        t.add_token("linear");
        assert_eq!(t.num_tokens(), 1);
        assert_eq!(t.token(0), Some("Linear"));
        assert!(t.has_token("LINEAR"));
        t.remove_token(" linear");
        assert_eq!(t.num_tokens(), 0);
        assert_eq!(t.token(0), None);
    }

    #[test]
    fn custom_keys() {
        let mut k = CustomKeys::default();
        k.set("b", "2").unwrap();
        k.set("a", "1").unwrap();
        assert_eq!(k.name(0).unwrap(), "a");
        assert_eq!(k.value(1).unwrap(), "2");
        assert_eq!(k.name(2).unwrap_err().to_string(), "Key index '2' is invalid, there are '2' custom keys.");
        k.set("a", "").unwrap();
        assert_eq!(k.len(), 1);
        assert!(k.set("", "x").is_err());
    }
}
