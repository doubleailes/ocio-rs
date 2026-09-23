//! Builtin configs (TODO: port `builtinconfigs`).

/// YAML text of the builtin config `name` (e.g.
/// `"cg-config-v2.2.0_aces-v1.3_ocio-v2.4"`, or the aliases
/// `"default"`, `"cg-config-latest"`, `"studio-config-latest"`).
pub fn get_builtin_config(_name: &str) -> Option<&'static str> {
    None
}
