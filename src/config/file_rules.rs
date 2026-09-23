//! File rules: find the color space of a file from its path (port of
//! `FileRules.cpp` and `ParseColorSpaceFromString` of `PathUtils.cpp`).

use super::logging::log_warning;
use super::tokens::CustomKeys;
use super::utils::{compare, lower, trim};
use super::Config;
use crate::error::{Error, Result};
use crate::types::{ColorSpaceVisibility, SearchReferenceSpaceType, ROLE_DEFAULT};
use std::fmt;

/// Name of the default rule.
pub const DEFAULT_RULE_NAME: &str = "Default";
/// Name of the rule searching color space names in the file path.
pub const FILE_PATH_SEARCH_RULE_NAME: &str = "ColorSpaceNamePathSearch";

/// Keys used by the YAML file rules.
pub mod keys {
    pub const NAME: &str = "name";
    pub const COLOR_SPACE: &str = "colorspace";
    pub const PATTERN: &str = "pattern";
    pub const EXTENSION: &str = "extension";
    pub const REGEX: &str = "regex";
    pub const CUSTOM_KEY: &str = "custom";
}

fn sanitize_regular_expression(r: &str) -> String {
    // "*?" => "*", "?*" => "*" and "**" => "*".
    let re1 = regex::Regex::new(r"(\.\*\.^\*)+|(^\\\.\.\*)+").expect("valid regex");
    let r = re1.replace_all(r, ".*").into_owned();
    let re2 = regex::Regex::new(r"(\.\*)+").expect("valid regex");
    re2.replace_all(&r, ".*").into_owned()
}

fn invalid_regex(glob: &str, what: &str) -> Error {
    Error::msg(format!(
        "File rules: invalid regular expression '{glob}' with '{what}'."
    ))
}

fn convert_to_regular_expression(glob_pattern: &str, ignore_case: bool) -> Result<String> {
    let mut glob_string = String::new();
    if ignore_case {
        let mut respect_case = false;
        for c in glob_pattern.chars() {
            if c == '[' || c == '*' || c == '?' {
                respect_case = true;
                break;
            }
            if c.is_ascii_alphabetic() {
                glob_string.push('[');
                glob_string.push(c.to_ascii_lowercase());
                glob_string.push(c.to_ascii_uppercase());
                glob_string.push(']');
            } else {
                glob_string.push(c);
            }
        }
        if respect_case {
            glob_string = glob_pattern.to_string();
        }
    } else {
        glob_string = glob_pattern.to_string();
    }

    let g: Vec<char> = glob_string.chars().collect();
    let at = |i: usize| -> char { g.get(i).copied().unwrap_or('\0') };
    let rest = |i: usize| -> String { g[i.min(g.len())..].iter().collect() };
    let size = g.len();
    let mut regex = String::new();
    let mut idx = 0;
    while idx < size {
        let mut next = idx + 1;
        let c = g[idx];
        match c {
            '.' => regex.push_str("\\."),
            '?' => regex.push('.'),
            '*' => regex.push_str(".*"),
            '+' | '^' | '$' | '{' | '}' | '(' | ')' | '|' => {
                regex.push('\\');
                regex.push(c);
            }
            ']' => return Err(invalid_regex(glob_pattern, &rest(idx))),
            '[' => {
                let mut sub = String::from("[");
                let mut end = idx + 1;
                while at(end) != ']' && end < size {
                    let e = g[end];
                    match e {
                        '!' => sub.push('^'),
                        '+' | '^' | '$' | '{' | '}' | '(' | ')' | '|' => {
                            sub.push('\\');
                            sub.push(e);
                        }
                        '\\' => sub.push_str("\\\\"),
                        '.' | '?' | '*' => {
                            if at(end - 1) != '\\' {
                                return Err(invalid_regex(glob_pattern, &rest(idx)));
                            }
                            sub.push(e);
                        }
                        '[' => return Err(invalid_regex(glob_pattern, &rest(idx))),
                        _ => sub.push(e),
                    }
                    end += 1;
                }
                if at(end) == ']' {
                    sub.push(']');
                }
                if end >= size {
                    return Err(invalid_regex(glob_pattern, &rest(idx)));
                } else if sub == "[]" {
                    return Err(invalid_regex(glob_pattern, "[]"));
                } else if sub == "[^]" {
                    return Err(invalid_regex(glob_pattern, "[!]"));
                }
                regex.push_str(&sub);
                next = end + 1;
            }
            _ => regex.push(c),
        }
        idx = next;
    }
    Ok(regex)
}

fn build_regular_expression(pattern: &str, extension: &str) -> Result<String> {
    let mut s = String::from("^(");
    if pattern.is_empty() {
        s.push_str("(.*)");
    } else {
        s.push('(');
        s.push_str(&convert_to_regular_expression(pattern, false)?);
        s.push(')');
    }
    if extension.is_empty() {
        s.push_str("(\\..*)");
    } else {
        s.push_str("(\\.");
        s.push_str(&convert_to_regular_expression(extension, true)?);
        s.push(')');
    }
    s.push_str(")$");
    Ok(sanitize_regular_expression(&s))
}

fn compile_full_match(regex: &str) -> std::result::Result<regex::Regex, regex::Error> {
    regex::Regex::new(&format!("^(?:{regex})$"))
}

fn validate_regular_expression(regex: &str) -> Result<()> {
    if regex.is_empty() {
        return Err(Error::msg("File rules: regex is empty."));
    }
    compile_full_match(regex).map(|_| ()).map_err(|e| {
        Error::msg(format!(
            "File rules: invalid regular expression '{regex}': '{e}'."
        ))
    })
}

fn validate_glob(pattern: &str, extension: &str) -> Result<()> {
    let exp = build_regular_expression(pattern, extension)?;
    validate_regular_expression(&exp)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuleType {
    Default,
    ParseFilepath,
    Regex,
    Glob,
}

#[derive(Debug, Clone, PartialEq)]
struct FileRule {
    name: String,
    color_space: String,
    pattern: String,
    extension: String,
    regex: String,
    rule_type: RuleType,
    custom_keys: CustomKeys,
}

impl FileRule {
    fn new(name: &str) -> Result<Self> {
        if name.is_empty() {
            return Err(Error::msg("The file rule name is empty"));
        }
        let mut r = Self {
            name: name.to_string(),
            color_space: String::new(),
            pattern: String::new(),
            extension: String::new(),
            regex: String::new(),
            rule_type: RuleType::Glob,
            custom_keys: CustomKeys::default(),
        };
        if compare(name, DEFAULT_RULE_NAME) {
            r.name = DEFAULT_RULE_NAME.to_string();
            r.rule_type = RuleType::Default;
        } else if compare(name, FILE_PATH_SEARCH_RULE_NAME) {
            r.name = FILE_PATH_SEARCH_RULE_NAME.to_string();
            r.rule_type = RuleType::ParseFilepath;
        } else {
            r.pattern = "*".to_string();
            r.extension = "*".to_string();
        }
        Ok(r)
    }

    fn is_special(&self) -> bool {
        matches!(self.rule_type, RuleType::Default | RuleType::ParseFilepath)
    }

    fn pattern(&self) -> &str {
        if self.rule_type != RuleType::Glob {
            ""
        } else {
            &self.pattern
        }
    }

    fn set_pattern(&mut self, pattern: &str) -> Result<()> {
        if self.is_special() {
            if !pattern.is_empty() {
                return Err(Error::msg(
                    "File rules: Default and ColorSpaceNamePathSearch rules do not accept any pattern.",
                ));
            }
        } else {
            if pattern.is_empty() {
                return Err(Error::msg("File rules: The file name pattern is empty."));
            }
            validate_glob(pattern, &self.extension)?;
            self.pattern = pattern.to_string();
            self.regex.clear();
            self.rule_type = RuleType::Glob;
        }
        Ok(())
    }

    fn extension(&self) -> &str {
        if self.rule_type != RuleType::Glob {
            ""
        } else {
            &self.extension
        }
    }

    fn set_extension(&mut self, extension: &str) -> Result<()> {
        if self.is_special() {
            if !extension.is_empty() {
                return Err(Error::msg(
                    "File rules: Default and ColorSpaceNamePathSearch rules do not accept any extension.",
                ));
            }
        } else {
            if extension.is_empty() {
                return Err(Error::msg(
                    "File rules: The file extension pattern is empty.",
                ));
            }
            validate_glob(&self.pattern, extension)?;
            self.extension = extension.to_string();
            self.regex.clear();
            self.rule_type = RuleType::Glob;
        }
        Ok(())
    }

    fn regex(&self) -> &str {
        if self.rule_type != RuleType::Regex {
            ""
        } else {
            &self.regex
        }
    }

    fn set_regex(&mut self, regex: &str) -> Result<()> {
        if self.is_special() {
            if !regex.is_empty() {
                return Err(Error::msg(
                    "File rules: Default and ColorSpaceNamePathSearch rules do not accept any regex.",
                ));
            }
        } else {
            validate_regular_expression(regex)?;
            self.regex = regex.to_string();
            self.pattern.clear();
            self.extension.clear();
            self.rule_type = RuleType::Regex;
        }
        Ok(())
    }

    fn set_color_space(&mut self, cs: &str) -> Result<()> {
        if self.rule_type == RuleType::ParseFilepath {
            if !cs.is_empty() {
                return Err(Error::msg(
                    "File rules: ColorSpaceNamePathSearch rule does not accept any color space.",
                ));
            }
        } else {
            if cs.is_empty() {
                return Err(Error::msg("File rules: color space name can't be empty."));
            }
            self.color_space = cs.to_string();
        }
        Ok(())
    }

    /// Return the color space if the rule matches.
    fn matches(&self, config: &Config, path: &str) -> Option<String> {
        match self.rule_type {
            RuleType::Default => Some(self.color_space.clone()),
            RuleType::ParseFilepath => parse_color_space_from_string(config, path).map(|idx| {
                config
                    .color_space_name_by_index_filtered(
                        SearchReferenceSpaceType::All,
                        ColorSpaceVisibility::All,
                        idx,
                    )
                    .to_string()
            }),
            RuleType::Regex => match compile_full_match(&self.regex) {
                Ok(re) if re.is_match(path) => Some(self.color_space.clone()),
                _ => None,
            },
            RuleType::Glob => {
                let exp = build_regular_expression(&self.pattern, &self.extension).ok()?;
                match compile_full_match(&exp) {
                    Ok(re) if re.is_match(path) => Some(self.color_space.clone()),
                    _ => None,
                }
            }
        }
    }

    fn validate(&self, config: &Config) -> Result<()> {
        if self.rule_type != RuleType::ParseFilepath
            && config.get_color_space(&self.color_space).is_none()
            && config.get_named_transform(&self.color_space).is_none()
        {
            return Err(Error::msg(format!(
                "File rules: rule named '{}' is referencing '{}' that is neither a color space nor a named transform.",
                self.name, self.color_space
            )));
        }
        Ok(())
    }
}

/// The ordered list of file rules; the default rule is always the last one.
#[derive(Debug, Clone, PartialEq)]
pub struct FileRules {
    rules: Vec<FileRule>,
}

impl Default for FileRules {
    fn default() -> Self {
        Self::new()
    }
}

impl FileRules {
    /// Only the default rule, using the `default` role.
    pub fn new() -> Self {
        let mut def = FileRule::new(DEFAULT_RULE_NAME).expect("valid name");
        def.color_space = ROLE_DEFAULT.to_string();
        Self { rules: vec![def] }
    }

    fn validate_position(&self, idx: usize, allow_default: bool) -> Result<()> {
        let n = self.rules.len();
        if idx >= n {
            return Err(Error::msg(format!(
                "File rules: rule index '{idx}' invalid. There are only '{n}' rules."
            )));
        }
        if !allow_default && idx + 1 == n {
            return Err(Error::msg(format!(
                "File rules: rule index '{idx}' is the default rule."
            )));
        }
        Ok(())
    }

    fn validate_new_rule(&self, idx: usize, name: &str) -> Result<()> {
        if name.is_empty() {
            return Err(Error::msg("File rules: rule should have a non-empty name."));
        }
        if self.rules.iter().any(|r| compare(name, &r.name)) {
            return Err(Error::msg(format!(
                "File rules: A rule named '{name}' already exists."
            )));
        }
        self.validate_position(idx, true)?;
        if compare(name, DEFAULT_RULE_NAME) {
            return Err(Error::msg(format!(
                "File rules: Default rule already exists at index  '{}'.",
                self.rules.len() - 1
            )));
        }
        Ok(())
    }

    pub fn num_entries(&self) -> usize {
        self.rules.len()
    }

    /// Index of a rule by name (case insensitive).
    pub fn index_for_rule(&self, name: &str) -> Result<usize> {
        self.rules
            .iter()
            .position(|r| compare(name, &r.name))
            .ok_or_else(|| Error::msg(format!("File rules: rule name '{name}' not found.")))
    }

    pub fn name(&self, idx: usize) -> Result<&str> {
        self.validate_position(idx, true)?;
        Ok(&self.rules[idx].name)
    }

    pub fn pattern(&self, idx: usize) -> Result<&str> {
        self.validate_position(idx, true)?;
        Ok(self.rules[idx].pattern())
    }

    pub fn set_pattern(&mut self, idx: usize, pattern: &str) -> Result<()> {
        self.validate_position(idx, false)?;
        self.rules[idx].set_pattern(pattern)
    }

    pub fn extension(&self, idx: usize) -> Result<&str> {
        self.validate_position(idx, true)?;
        Ok(self.rules[idx].extension())
    }

    pub fn set_extension(&mut self, idx: usize, ext: &str) -> Result<()> {
        self.validate_position(idx, false)?;
        self.rules[idx].set_extension(ext)
    }

    pub fn regex(&self, idx: usize) -> Result<&str> {
        self.validate_position(idx, true)?;
        Ok(self.rules[idx].regex())
    }

    pub fn set_regex(&mut self, idx: usize, regex: &str) -> Result<()> {
        self.validate_position(idx, false)?;
        self.rules[idx].set_regex(regex)
    }

    /// Color space (or role) of the rule.
    pub fn color_space(&self, idx: usize) -> Result<&str> {
        self.validate_position(idx, true)?;
        Ok(&self.rules[idx].color_space)
    }

    pub fn set_color_space(&mut self, idx: usize, cs: &str) -> Result<()> {
        self.validate_position(idx, true)?;
        self.rules[idx].set_color_space(cs)
    }

    pub fn num_custom_keys(&self, idx: usize) -> Result<usize> {
        self.validate_position(idx, true)?;
        Ok(self.rules[idx].custom_keys.len())
    }

    pub fn custom_key_name(&self, idx: usize, key: usize) -> Result<&str> {
        self.validate_position(idx, true)?;
        let r = &self.rules[idx];
        r.custom_keys.name(key).map_err(|e| {
            Error::msg(format!(
                "File rules: the custom key access for file rule '{}' failed: {}",
                r.name, e
            ))
        })
    }

    pub fn custom_key_value(&self, idx: usize, key: usize) -> Result<&str> {
        self.validate_position(idx, true)?;
        let r = &self.rules[idx];
        r.custom_keys.value(key).map_err(|e| {
            Error::msg(format!(
                "File rules: the custom key access for file rule '{}' failed: {}",
                r.name, e
            ))
        })
    }

    pub fn set_custom_key(&mut self, idx: usize, key: &str, value: &str) -> Result<()> {
        self.validate_position(idx, true)?;
        let r = &mut self.rules[idx];
        let name = r.name.clone();
        r.custom_keys
            .set(key, value)
            .map_err(|e| Error::msg(format!("File rules: rule named '{name}' error: {e}")))
    }

    /// Insert a glob rule (pattern + extension) at `idx`.
    pub fn insert_rule(
        &mut self,
        idx: usize,
        name: &str,
        color_space: &str,
        pattern: &str,
        extension: &str,
    ) -> Result<()> {
        let name = trim(name).to_string();
        self.validate_new_rule(idx, &name)?;
        let mut r = FileRule::new(&name)?;
        r.set_color_space(color_space)?;
        r.set_pattern(pattern)?;
        r.set_extension(extension)?;
        self.rules.insert(idx, r);
        Ok(())
    }

    /// Insert a regex rule at `idx`.
    pub fn insert_rule_regex(
        &mut self,
        idx: usize,
        name: &str,
        color_space: &str,
        regex: &str,
    ) -> Result<()> {
        let name = trim(name).to_string();
        self.validate_new_rule(idx, &name)?;
        let mut r = FileRule::new(&name)?;
        r.set_color_space(color_space)?;
        r.set_regex(regex)?;
        self.rules.insert(idx, r);
        Ok(())
    }

    /// Insert the `ColorSpaceNamePathSearch` rule at `idx`.
    pub fn insert_path_search_rule(&mut self, idx: usize) -> Result<()> {
        self.insert_rule_regex(idx, FILE_PATH_SEARCH_RULE_NAME, "", "")
    }

    /// Set the color space of the default rule.
    pub fn set_default_rule_color_space(&mut self, cs: &str) -> Result<()> {
        match self.rules.last_mut() {
            Some(r) => r.set_color_space(cs),
            None => Ok(()),
        }
    }

    pub fn remove_rule(&mut self, idx: usize) -> Result<()> {
        self.validate_position(idx, false)?;
        self.rules.remove(idx);
        Ok(())
    }

    fn move_rule(&mut self, idx: usize, offset: i64) -> Result<()> {
        self.validate_position(idx, false)?;
        let new_idx = idx as i64 + offset;
        if new_idx < 0 || new_idx >= self.rules.len() as i64 - 1 {
            return Err(Error::msg(format!(
                "File rules: rule at index '{idx}' may not be moved to index '{new_idx}'."
            )));
        }
        let r = self.rules.remove(idx);
        self.rules.insert(new_idx as usize, r);
        Ok(())
    }

    /// Move the rule one position up (higher priority).
    pub fn increase_rule_priority(&mut self, idx: usize) -> Result<()> {
        self.move_rule(idx, -1)
    }

    /// Move the rule one position down (lower priority).
    pub fn decrease_rule_priority(&mut self, idx: usize) -> Result<()> {
        self.move_rule(idx, 1)
    }

    /// True if only the default rule using the default role exists.
    pub fn is_default(&self) -> bool {
        self.rules.len() == 1
            && self.rules[0].custom_keys.is_empty()
            && compare(&self.rules[0].color_space, ROLE_DEFAULT)
    }

    /// Color space and index of the first rule matching the path.
    pub fn color_space_from_filepath(&self, config: &Config, path: &str) -> (String, usize) {
        for (i, r) in self.rules.iter().enumerate() {
            if let Some(cs) = r.matches(config, path) {
                return (cs, i);
            }
        }
        (
            self.rules
                .last()
                .map(|r| r.color_space.clone())
                .unwrap_or_default(),
            self.rules.len() - 1,
        )
    }

    /// True if only the default rule matches the path.
    pub fn filepath_only_matches_default_rule(&self, config: &Config, path: &str) -> bool {
        let (_, idx) = self.color_space_from_filepath(config, path);
        idx + 1 == self.rules.len()
    }

    /// Validate the rules against a config.
    pub fn validate(&self, config: &Config) -> Result<()> {
        if config.major_version() >= 2 || (config.major_version() == 1 && self.rules.len() > 2) {
            for r in &self.rules {
                r.validate(config)?;
            }
        }
        Ok(())
    }
}

impl fmt::Display for FileRules {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let n = self.rules.len();
        for (i, r) in self.rules.iter().enumerate() {
            write!(f, "<FileRule name={}", r.name)?;
            if !r.color_space.is_empty() {
                write!(f, ", colorspace={}", r.color_space)?;
            }
            if !r.regex().is_empty() {
                write!(f, ", regex={}", r.regex())?;
            }
            if !r.pattern().is_empty() {
                write!(f, ", pattern={}", r.pattern())?;
            }
            if !r.extension().is_empty() {
                write!(f, ", extension={}", r.extension())?;
            }
            if !r.custom_keys.is_empty() {
                let keys: Vec<String> = r
                    .custom_keys
                    .iter()
                    .map(|(k, v)| format!("({k}, {v})"))
                    .collect();
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

/// Index (in all color spaces) of the right-most color space name (or alias)
/// found in `s` (`ParseColorSpaceFromString`).
pub fn parse_color_space_from_string(config: &Config, s: &str) -> Option<usize> {
    let full = lower(s);
    let mut right_most_pos: i64 = -1;
    let mut right_most_name = String::new();
    let mut right_most_index = None;
    let mut adjust = |name: &str, index: usize, pos: usize| {
        let p = (pos + name.len()) as i64;
        if p > right_most_pos || (p == right_most_pos && name.len() > right_most_name.len()) {
            right_most_pos = p;
            right_most_name = name.to_string();
            right_most_index = Some(index);
        }
    };
    let n =
        config.num_color_spaces_filtered(SearchReferenceSpaceType::All, ColorSpaceVisibility::All);
    for i in 0..n {
        let csname = lower(config.color_space_name_by_index_filtered(
            SearchReferenceSpaceType::All,
            ColorSpaceVisibility::All,
            i,
        ));
        if let Some(pos) = full.rfind(&csname) {
            adjust(&csname, i, pos);
        }
        if let Some(cs) = config.get_color_space(&csname) {
            for a in cs.aliases() {
                let alias = lower(a);
                if let Some(pos) = full.rfind(&alias) {
                    adjust(&alias, i, pos);
                }
            }
        }
    }
    right_most_index
}

/// Build valid v2 file rules for a v1 config (`UpdateFileRulesFromV1ToV2`).
pub fn update_file_rules_from_v1_to_v2(config: &Config, rules: &mut FileRules) -> Result<()> {
    if config.major_version() != 1 {
        return Ok(());
    }
    if rules.index_for_rule(FILE_PATH_SEARCH_RULE_NAME).is_err() {
        rules.insert_path_search_rule(0)?;
    }
    if config.get_color_space(ROLE_DEFAULT).is_some() {
        return Ok(());
    }
    if let Some(cs) = config.get_color_space("raw") {
        if cs.is_data() {
            return rules.set_color_space(1, cs.name());
        }
    }
    let n = config
        .num_color_spaces_filtered(SearchReferenceSpaceType::Scene, ColorSpaceVisibility::All);
    for i in 0..n {
        let name = config
            .color_space_name_by_index_filtered(
                SearchReferenceSpaceType::Scene,
                ColorSpaceVisibility::All,
                i,
            )
            .to_string();
        if config
            .get_color_space(&name)
            .map(|c| c.is_data())
            .unwrap_or(false)
        {
            return rules.set_color_space(1, &name);
        }
    }
    if config.num_color_spaces() > 0 {
        let name = config.color_space_name_by_index(0).to_string();
        rules.set_color_space(1, &name)
    } else {
        log_warning(
            "The default rule creation falls back to the first color space because no suitable color space exists.",
        );
        let name = config
            .color_space_name_by_index_filtered(
                SearchReferenceSpaceType::Scene,
                ColorSpaceVisibility::All,
                0,
            )
            .to_string();
        rules.set_color_space(1, &name)
    }
}
