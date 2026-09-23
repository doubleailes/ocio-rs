//! Helpers shared by the config integration tests.
#![allow(dead_code)]

use std::sync::{Mutex, MutexGuard};

/// Assert that `r` is an error whose message contains `what`.
#[macro_export]
macro_rules! assert_err {
    ($r:expr, $what:expr) => {{
        match $r {
            Ok(_) => panic!("expected an error containing {:?}", $what),
            Err(e) => {
                let msg = e.to_string();
                assert!(msg.contains($what), "error {:?} does not contain {:?}", msg, $what);
            }
        }
    }};
}

/// Serialize the tests that modify environment variables.
pub fn env_lock() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Set (or unset) an environment variable while the guard is alive.
pub struct EnvGuard {
    name: String,
    old: Option<String>,
}

impl EnvGuard {
    pub fn set(name: &str, value: Option<&str>) -> Self {
        let old = std::env::var(name).ok();
        match value {
            Some(v) => std::env::set_var(name, v),
            None => std::env::remove_var(name),
        }
        Self { name: name.to_string(), old }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.old {
            Some(v) => std::env::set_var(&self.name, v),
            None => std::env::remove_var(&self.name),
        }
    }
}

/// Path of a test data file.
pub fn data_file(rel: &str) -> String {
    format!("{}/tests/data/files/{}", env!("CARGO_MANIFEST_DIR"), rel)
}

use ocio::config::logging::LogGuard;

/// Port of `checkAndMuteSceneLinearRoleError`.
pub fn mute_scene_linear_role_error(g: &LogGuard) -> bool {
    g.find_and_remove("[OpenColorIO Error]: The scene_linear role is required for a config version 2.2 or higher.")
}

/// Port of `checkAndMuteCompositingLogRoleError`.
pub fn mute_compositing_log_role_error(g: &LogGuard) -> bool {
    g.find_and_remove("[OpenColorIO Error]: The compositing_log role is required for a config version 2.2 or higher.")
}

/// Port of `checkAndMuteColorTimingRoleError`.
pub fn mute_color_timing_role_error(g: &LogGuard) -> bool {
    g.find_and_remove("[OpenColorIO Error]: The color_timing role is required for a config version 2.2 or higher.")
}

/// Port of `checkAndMuteAcesInterchangeRoleError`.
pub fn mute_aces_interchange_role_error(g: &LogGuard) -> bool {
    g.find_and_remove(
        "[OpenColorIO Error]: The aces_interchange role is required when there are scene-referred color spaces and the config version is 2.2 or higher.",
    )
}

/// Port of `checkAndMuteDisplayInterchangeRoleError`.
pub fn mute_display_interchange_role_error(g: &LogGuard) -> bool {
    g.find_and_remove(
        "[OpenColorIO Error]: The cie_xyz_d65_interchange role is required when there are display-referred color spaces and the config version is 2.2 or higher.",
    )
}

/// Check and mute the four "missing role" errors logged when validating
/// upgraded configs.
pub fn mute_missing_role_errors(g: &LogGuard) {
    assert!(mute_scene_linear_role_error(g));
    assert!(mute_compositing_log_role_error(g));
    assert!(mute_color_timing_role_error(g));
    assert!(mute_aces_interchange_role_error(g));
}

/// Port of `muteInactiveColorspaceInfo`.
pub fn mute_inactive_colorspace_info(g: &LogGuard) {
    g.find_all_and_remove(
        r"\[OpenColorIO Info\]: Inactive.*- Display' is neither a color space nor a named transform.[\r\n]+",
    );
}

/// Port of `checkAndMuteWarning`.
pub fn mute_warning(g: &LogGuard, s: &str) -> bool {
    g.find_all_and_remove(&format!(r"\[OpenColorIO Warning\]: {}[\.\r\n]+", s))
}

/// Port of `checkAndMuteError`.
pub fn mute_error(g: &LogGuard, s: &str) -> bool {
    g.find_all_and_remove(&format!(r"\[OpenColorIO Error\]: {}[\r\n]+", s))
}

/// Compare two strings line by line (as OCIO's tests do).
pub fn check_lines(actual: &str, expected: &str) {
    let a: Vec<&str> = actual.lines().collect();
    let e: Vec<&str> = expected.lines().collect();
    for (i, (x, y)) in a.iter().zip(e.iter()).enumerate() {
        assert_eq!(x, y, "line {} differs:\nactual:\n{}\nexpected:\n{}", i + 1, actual, expected);
    }
    assert_eq!(a.len(), e.len(), "line count differs:\nactual:\n{}\nexpected:\n{}", actual, expected);
}
