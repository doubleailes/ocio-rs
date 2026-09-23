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

/// Compare two strings line by line (as OCIO's tests do).
pub fn check_lines(actual: &str, expected: &str) {
    let a: Vec<&str> = actual.lines().collect();
    let e: Vec<&str> = expected.lines().collect();
    for (i, (x, y)) in a.iter().zip(e.iter()).enumerate() {
        assert_eq!(x, y, "line {} differs:\nactual:\n{}\nexpected:\n{}", i + 1, actual, expected);
    }
    assert_eq!(a.len(), e.len(), "line count differs:\nactual:\n{}\nexpected:\n{}", actual, expected);
}
