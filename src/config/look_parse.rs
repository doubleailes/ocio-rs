//! Parsing of look lists such as `"+cc, -onset | +cc"` (port of
//! `LookParse.cpp`).

use super::utils::{split, split_string_env_style_lossy, trim};
use crate::types::TransformDirection;

/// One look of a look list, with its direction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LookToken {
    pub name: String,
    pub dir: TransformDirection,
}

impl LookToken {
    /// Parse `"+name"`, `"-name"` or `"name"`.
    pub fn parse(s: &str) -> Self {
        if let Some(r) = s.strip_prefix('+') {
            Self { name: r.trim_start_matches('+').to_string(), dir: TransformDirection::Forward }
        } else if let Some(r) = s.strip_prefix('-') {
            Self { name: r.trim_start_matches('-').to_string(), dir: TransformDirection::Inverse }
        } else {
            Self { name: s.to_string(), dir: TransformDirection::Forward }
        }
    }

    /// Serialize (`-name` for inverse looks).
    pub fn serialize(&self) -> String {
        match self.dir {
            TransformDirection::Forward => self.name.clone(),
            TransformDirection::Inverse => format!("-{}", self.name),
        }
    }
}

/// A list of looks to apply in order.
pub type LookTokens = Vec<LookToken>;

/// Serialize tokens separated by `", "`.
pub fn serialize_tokens(tokens: &[LookToken]) -> String {
    tokens.iter().map(|t| t.serialize()).collect::<Vec<_>>().join(", ")
}

/// The options of a look list: each `|` separated option is a list of looks,
/// the first option that can be built is used.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LookParseResult {
    options: Vec<LookTokens>,
}

impl LookParseResult {
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse a look list and return the options.
    pub fn parse(&mut self, looks: &str) -> &[LookTokens] {
        self.options.clear();
        let stripped = trim(looks);
        if stripped.is_empty() {
            return &self.options;
        }
        for option in split(stripped, '|') {
            let tokens = split_string_env_style_lossy(&option).iter().map(|s| LookToken::parse(s)).collect();
            self.options.push(tokens);
        }
        &self.options
    }

    /// The parsed options.
    pub fn options(&self) -> &[LookTokens] {
        &self.options
    }

    /// True if there are no options.
    pub fn is_empty(&self) -> bool {
        self.options.is_empty()
    }

    /// Reverse each option (order and directions), keeping the option order.
    pub fn reverse(&mut self) {
        for option in self.options.iter_mut() {
            option.reverse();
            for t in option.iter_mut() {
                t.dir = t.dir.inverse();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use TransformDirection::{Forward as F, Inverse as I};

    fn check(r: &mut LookParseResult, s: &str, expected: &[&[(&str, TransformDirection)]]) {
        let opts = r.parse(s).to_vec();
        assert_eq!(opts.len(), expected.len(), "{s}");
        for (o, e) in opts.iter().zip(expected) {
            assert_eq!(o.len(), e.len(), "{s}");
            for (t, (n, d)) in o.iter().zip(e.iter()) {
                assert_eq!(t.name, *n);
                assert_eq!(t.dir, *d);
            }
        }
    }

    #[test]
    fn parse() {
        let mut r = LookParseResult::new();
        check(&mut r, "", &[]);
        assert!(r.is_empty());
        check(&mut r, "  ", &[]);
        check(&mut r, "cc", &[&[("cc", F)]]);
        assert!(!r.is_empty());
        check(&mut r, "+cc", &[&[("cc", F)]]);
        check(&mut r, "  +cc", &[&[("cc", F)]]);
        check(&mut r, "  +cc   ", &[&[("cc", F)]]);
        check(&mut r, "+cc,-di", &[&[("cc", F), ("di", I)]]);
        check(&mut r, "  +cc ,  -di", &[&[("cc", F), ("di", I)]]);
        check(&mut r, "  +cc :  -di", &[&[("cc", F), ("di", I)]]);
        check(&mut r, "+cc, -di |-cc", &[&[("cc", F), ("di", I)], &[("cc", I)]]);
        check(&mut r, "+cc, -di |-cc|   ", &[&[("cc", F), ("di", I)], &[("cc", I)], &[("", F)]]);
    }

    #[test]
    fn reverse() {
        let mut r = LookParseResult::new();
        r.parse("+cc, -di |-cc|   ");
        r.reverse();
        let o = r.options();
        assert_eq!(o.len(), 3);
        assert_eq!(o[0].len(), 2);
        assert_eq!(o[0][1], LookToken { name: "cc".into(), dir: I });
        assert_eq!(o[0][0], LookToken { name: "di".into(), dir: F });
        assert_eq!(o[1][0], LookToken { name: "cc".into(), dir: F });
        assert_eq!(o[2][0], LookToken { name: "".into(), dir: I });
        assert_eq!(serialize_tokens(&o[0]), "di, -cc");
    }
}
