//! Context: search paths, working directory and context (environment)
//! variables used to resolve file references (port of `Context.cpp` and
//! `ContextVariableUtils.cpp`).

use crate::error::{Error, Result};
use crate::path_utils;
use crate::types::EnvironmentMode;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt;

/// Key ordering used by OCIO's `EnvMap`: longest names first, then
/// lexicographic. This guarantees `$TEST_$TESTING` resolves `TESTING` first.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EnvKey(pub String);

impl Ord for EnvKey {
    fn cmp(&self, other: &Self) -> Ordering {
        match other.0.len().cmp(&self.0.len()) {
            Ordering::Equal => self.0.cmp(&other.0),
            o => o,
        }
    }
}
impl PartialOrd for EnvKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Map of context variables, ordered as in OCIO.
pub type EnvMap = BTreeMap<EnvKey, String>;
/// Context variables that were used during a resolution.
pub type UsedEnvs = BTreeMap<String, String>;

/// True if the string contains `$` or `%`.
pub fn contains_context_variable_token(s: &str) -> bool {
    s.contains('$') || s.contains('%')
}

/// True if the string (probably) references a context variable.
pub fn contains_context_variables(s: &str) -> bool {
    if s.contains('$') {
        return true;
    }
    if let (Some(b), Some(e)) = (s.find('%'), s.rfind('%')) {
        if b != e {
            return true;
        }
    }
    false
}

/// Load the process environment into `map`. If `update` is true only
/// existing keys are updated.
pub fn load_environment(map: &mut EnvMap, update: bool) {
    for (name, value) in std::env::vars_os() {
        let (Some(name), Some(value)) = (name.to_str(), value.to_str()) else {
            continue;
        };
        let key = EnvKey(name.to_string());
        if update {
            if let Some(v) = map.get_mut(&key) {
                *v = value.to_string();
            }
        } else {
            map.entry(key).or_insert_with(|| value.to_string());
        }
    }
}

fn resolve_impl(s: &str, map: &EnvMap, used: &mut UsedEnvs, depth: u32) -> String {
    if depth > 32 || !contains_context_variables(s) {
        return s.to_string();
    }
    let mut newstr = s.to_string();
    for (k, v) in map {
        let k = &k.0;
        for pat in [format!("${{{k}}}"), format!("${k}"), format!("%{k}%")] {
            if newstr.contains(&pat) {
                newstr = newstr.replace(&pat, v);
                used.insert(k.clone(), v.clone());
            }
        }
    }
    if newstr != s {
        return resolve_impl(&newstr, map, used, depth + 1);
    }
    s.to_string()
}

/// Resolve `$VAR`, `${VAR}` and `%VAR%` tokens using `map`.
pub fn resolve_context_variables(s: &str, map: &EnvMap, used: &mut UsedEnvs) -> String {
    resolve_impl(s, map, used, 0)
}

/// A context: search paths, working directory and string variables.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Context {
    search_paths: Vec<String>,
    working_dir: String,
    env_mode: EnvironmentMode,
    env_map: EnvMap,
}

impl Context {
    /// Empty context (environment mode `LoadPredefined`).
    pub fn new() -> Self {
        Self::default()
    }

    /// Cache identifier of the context.
    pub fn cache_id(&self) -> String {
        let mut s = String::new();
        if !self.search_paths.is_empty() {
            s.push_str("Search Path ");
            for p in &self.search_paths {
                s.push_str(p);
                s.push(' ');
            }
        }
        s.push_str(&format!("Working Dir {} ", self.working_dir));
        s.push_str(&format!("Environment Mode {} ", self.env_mode as i32));
        for (k, v) in &self.env_map {
            s.push_str(&format!("{}={} ", k.0, v));
        }
        format!("{:x}", md5::compute(s.as_bytes()))
    }

    /// Set the search path from a colon (or semicolon on Windows) separated string.
    pub fn set_search_path(&mut self, path: &str) {
        self.search_paths.clear();
        for p in path_utils::split_search_path(path) {
            if !p.is_empty() {
                self.search_paths.push(p.to_string());
            }
        }
    }

    /// The concatenated search path.
    pub fn search_path(&self) -> String {
        self.search_paths.join(path_utils::SEARCH_PATH_SEPARATOR)
    }
    pub fn num_search_paths(&self) -> usize {
        self.search_paths.len()
    }
    pub fn search_paths(&self) -> &[String] {
        &self.search_paths
    }
    pub fn search_path_by_index(&self, i: usize) -> Option<&str> {
        self.search_paths.get(i).map(|s| s.as_str())
    }
    pub fn clear_search_paths(&mut self) {
        self.search_paths.clear();
    }
    pub fn add_search_path(&mut self, path: &str) {
        if !path.is_empty() {
            self.search_paths.push(path.to_string());
        }
    }

    pub fn set_working_dir(&mut self, dir: &str) {
        self.working_dir = dir.to_string();
    }
    pub fn working_dir(&self) -> &str {
        &self.working_dir
    }

    /// Set (or remove if `value` is `None`) a string variable.
    pub fn set_string_var(&mut self, name: &str, value: Option<&str>) {
        if name.is_empty() {
            return;
        }
        let key = EnvKey(name.to_string());
        match value {
            Some(v) => {
                self.env_map.insert(key, v.to_string());
            }
            None => {
                self.env_map.remove(&key);
            }
        }
    }
    /// Value of a string variable (`""` if missing).
    pub fn string_var(&self, name: &str) -> &str {
        self.env_map
            .get(&EnvKey(name.to_string()))
            .map(|s| s.as_str())
            .unwrap_or("")
    }
    pub fn has_string_var(&self, name: &str) -> bool {
        self.env_map.contains_key(&EnvKey(name.to_string()))
    }
    pub fn num_string_vars(&self) -> usize {
        self.env_map.len()
    }
    /// Iterate over (name, value) in resolution order.
    pub fn string_vars(&self) -> impl Iterator<Item = (&str, &str)> {
        self.env_map.iter().map(|(k, v)| (k.0.as_str(), v.as_str()))
    }
    pub fn string_var_name_by_index(&self, i: usize) -> Option<&str> {
        self.env_map.keys().nth(i).map(|k| k.0.as_str())
    }
    pub fn string_var_by_index(&self, i: usize) -> Option<&str> {
        self.env_map.values().nth(i).map(|v| v.as_str())
    }
    pub fn clear_string_vars(&mut self) {
        self.env_map.clear();
    }
    /// Add (or overwrite) all string vars of `other`.
    pub fn add_string_vars(&mut self, other: &Context) {
        for (k, v) in &other.env_map {
            self.env_map.insert(k.clone(), v.clone());
        }
    }
    pub fn env_map(&self) -> &EnvMap {
        &self.env_map
    }

    pub fn set_environment_mode(&mut self, mode: EnvironmentMode) {
        self.env_mode = mode;
    }
    pub fn environment_mode(&self) -> EnvironmentMode {
        self.env_mode
    }

    /// Load environment variables. In `LoadAll` mode, all variables are
    /// added; otherwise only already-defined keys are updated.
    pub fn load_environment(&mut self) {
        let update = self.env_mode != EnvironmentMode::LoadAll;
        load_environment(&mut self.env_map, update);
    }

    /// Resolve context variables in `s`.
    pub fn resolve_string_var(&self, s: &str) -> String {
        let mut used = UsedEnvs::new();
        resolve_context_variables(s, &self.env_map, &mut used)
    }

    /// Resolve context variables in `s` and collect the variables used.
    pub fn resolve_string_var_with_used(&self, s: &str, used: &mut Context) -> String {
        let mut envs = UsedEnvs::new();
        let r = resolve_context_variables(s, &self.env_map, &mut envs);
        for (k, v) in envs {
            used.set_string_var(&k, Some(&v));
        }
        r
    }

    /// Absolute search paths (resolved against the working dir).
    fn absolute_search_paths(&self, used: &mut UsedEnvs) -> Vec<String> {
        if self.search_paths.is_empty() {
            return vec![self.working_dir.clone()];
        }
        self.search_paths
            .iter()
            .map(|p| {
                let resolved = resolve_context_variables(p, &self.env_map, used);
                let mut dir = resolved.trim().trim_end_matches('/').to_string();
                if dir.is_empty() {
                    dir = resolved.trim().to_string();
                }
                if !path_utils::is_absolute(&dir) {
                    dir = path_utils::join(&self.working_dir, &dir);
                }
                path_utils::normpath(&dir)
            })
            .collect()
    }

    /// Locate a file: absolute paths are checked directly, relative paths are
    /// searched in the search paths (or the working directory).
    pub fn resolve_file_location(&self, filename: &str) -> Result<String> {
        let mut used = Context::new();
        self.resolve_file_location_with_used(filename, &mut used)
    }

    /// Same as [`Context::resolve_file_location`] and collect the context
    /// variables used.
    pub fn resolve_file_location_with_used(
        &self,
        filename: &str,
        used_vars: &mut Context,
    ) -> Result<String> {
        let resolved = self.resolve_string_var_with_used(filename, used_vars);
        if path_utils::is_absolute(&resolved) {
            if path_utils::file_exists(&resolved) {
                return Ok(path_utils::normpath(&resolved));
            }
            return Err(Error::missing_file(format!(
                "The specified absolute file reference '{resolved}' could not be located."
            )));
        }
        let mut envs = UsedEnvs::new();
        let paths = self.absolute_search_paths(&mut envs);
        let mut err = format!(
            "The specified file reference '{filename}' could not be located. The following attempts were made: "
        );
        for (i, sp) in paths.iter().enumerate() {
            let full = path_utils::join(sp, &resolved);
            if !contains_context_variables(&full) && path_utils::file_exists(&full) {
                for (k, v) in &envs {
                    used_vars.set_string_var(k, Some(v));
                }
                return Ok(path_utils::normpath(&full));
            }
            if i != 0 {
                err.push_str(" : ");
            }
            err.push_str(&format!("'{full}'"));
        }
        err.push('.');
        Err(Error::missing_file(err))
    }
}

impl fmt::Display for Context {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<Context searchPath=[")?;
        for (i, p) in self.search_paths.iter().enumerate() {
            if i != 0 {
                write!(f, ", ")?;
            }
            write!(f, "\"{p}\"")?;
        }
        write!(
            f,
            "], workingDir={}, environmentMode={}, environment=",
            self.working_dir,
            self.env_mode.as_str()
        )?;
        for (k, v) in &self.env_map {
            write!(f, "\n    {}: {}", k.0, v)?;
        }
        write!(f, ">")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_order() {
        let mut ctx = Context::new();
        ctx.set_string_var("TEST", Some("1"));
        ctx.set_string_var("TESTING", Some("2"));
        ctx.set_string_var("TE", Some("3"));
        assert_eq!(ctx.resolve_string_var("$TEST_$TESTING_$TE"), "1_2_3");
        assert_eq!(ctx.resolve_string_var("${TEST}_%TE%"), "1_3");
        assert_eq!(ctx.resolve_string_var("none"), "none");
    }

    #[test]
    fn recursion() {
        let mut ctx = Context::new();
        ctx.set_string_var("A", Some("$B"));
        ctx.set_string_var("B", Some("x"));
        assert_eq!(ctx.resolve_string_var("$A"), "x");
    }
}
