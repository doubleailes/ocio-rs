//! Logging (port of `Logging.cpp`).
//!
//! Messages are written to stderr, prefixed by `[OpenColorIO <Level>]: `.
//! The level comes from `$OCIO_LOGGING_LEVEL` (default `info`) and can be
//! changed with [`set_logging_level`].
//!
//! For tests, [`LogGuard`] captures the messages logged by the current
//! thread (at debug level) instead of printing them.

use crate::types::{LoggingLevel, OCIO_LOGGING_LEVEL_ENVVAR};
use std::cell::RefCell;
use std::sync::{Mutex, OnceLock};

struct Global {
    level: LoggingLevel,
    env_override: bool,
}

fn global() -> &'static Mutex<Global> {
    static G: OnceLock<Mutex<Global>> = OnceLock::new();
    G.get_or_init(|| {
        let (level, env_override) = match std::env::var(OCIO_LOGGING_LEVEL_ENVVAR) {
            Ok(s) if !s.is_empty() => {
                let l = LoggingLevel::from_str_lossy(&s);
                if l == LoggingLevel::Unknown {
                    eprintln!(
                        "[OpenColorIO Warning]: Invalid $OCIO_LOGGING_LEVEL specified. \
                         Options: none (0), warning (1), info (2), debug (3)"
                    );
                    (LoggingLevel::Info, true)
                } else {
                    (l, true)
                }
            }
            _ => (LoggingLevel::Info, false),
        };
        Mutex::new(Global {
            level,
            env_override,
        })
    })
}

thread_local! {
    static CAPTURE: RefCell<Option<(LoggingLevel, String)>> = const { RefCell::new(None) };
}

/// Current logging level.
pub fn logging_level() -> LoggingLevel {
    let captured = CAPTURE.with(|c| c.borrow().as_ref().map(|(l, _)| *l));
    if let Some(l) = captured {
        return l;
    }
    global()
        .lock()
        .map(|g| g.level)
        .unwrap_or(LoggingLevel::Info)
}

/// Set the logging level (ignored when `$OCIO_LOGGING_LEVEL` is set).
pub fn set_logging_level(level: LoggingLevel) {
    if let Ok(mut g) = global().lock() {
        if !g.env_override {
            g.level = level;
        }
    }
}

fn log_message(prefix: &str, text: &str) {
    let trimmed = super::utils::right_trim(text);
    let parts = super::utils::split_by_lines(trimmed);
    let captured = CAPTURE.with(|c| {
        let mut c = c.borrow_mut();
        if let Some((_, out)) = c.as_mut() {
            for p in &parts {
                out.push_str(prefix);
                out.push_str(p);
                out.push('\n');
            }
            true
        } else {
            false
        }
    });
    if !captured {
        for p in &parts {
            eprintln!("{prefix}{p}");
        }
    }
}

/// Log an error (shown at the warning level and above).
pub fn log_error(text: &str) {
    if logging_level() >= LoggingLevel::Warning && logging_level() != LoggingLevel::Unknown {
        log_message("[OpenColorIO Error]: ", text);
    }
}

/// Log a warning.
pub fn log_warning(text: &str) {
    if logging_level() >= LoggingLevel::Warning && logging_level() != LoggingLevel::Unknown {
        log_message("[OpenColorIO Warning]: ", text);
    }
}

/// Log an information message.
pub fn log_info(text: &str) {
    if logging_level() >= LoggingLevel::Info && logging_level() != LoggingLevel::Unknown {
        log_message("[OpenColorIO Info]: ", text);
    }
}

/// Log a debug message.
pub fn log_debug(text: &str) {
    if logging_level() >= LoggingLevel::Debug && logging_level() != LoggingLevel::Unknown {
        log_message("[OpenColorIO Debug]: ", text);
    }
}

/// Captures the log messages of the current thread while alive (port of the
/// unit test `LogGuard`).
pub struct LogGuard {
    previous: Option<(LoggingLevel, String)>,
}

impl LogGuard {
    /// Capture at the debug level.
    pub fn new() -> Self {
        Self::with_level(LoggingLevel::Debug)
    }

    /// Capture at the given level.
    pub fn with_level(level: LoggingLevel) -> Self {
        let previous = CAPTURE.with(|c| c.borrow_mut().replace((level, String::new())));
        Self { previous }
    }

    /// Captured output.
    pub fn output(&self) -> String {
        CAPTURE.with(|c| {
            c.borrow()
                .as_ref()
                .map(|(_, s)| s.clone())
                .unwrap_or_default()
        })
    }

    /// True if nothing was captured.
    pub fn is_empty(&self) -> bool {
        self.output().is_empty()
    }

    /// Clear the captured output.
    pub fn clear(&self) {
        CAPTURE.with(|c| {
            if let Some((_, s)) = c.borrow_mut().as_mut() {
                s.clear();
            }
        });
    }

    /// Find `line` (followed by a line break) in the output and remove it.
    pub fn find_and_remove(&self, line: &str) -> bool {
        CAPTURE.with(|c| {
            let mut c = c.borrow_mut();
            if let Some((_, s)) = c.as_mut() {
                let pat = format!("{}[\r\n]+", regex::escape(line));
                if let Ok(re) = regex::Regex::new(&pat) {
                    if let Some(m) = re.find(s) {
                        let r = m.range();
                        s.replace_range(r, "");
                        return true;
                    }
                }
            }
            false
        })
    }

    /// Remove all the matches of the regular expression `pattern`.
    pub fn find_all_and_remove(&self, pattern: &str) -> bool {
        CAPTURE.with(|c| {
            let mut c = c.borrow_mut();
            let mut found = false;
            if let Some((_, s)) = c.as_mut() {
                if let Ok(re) = regex::Regex::new(pattern) {
                    while let Some(m) = re.find(s) {
                        found = true;
                        let r = m.range();
                        s.replace_range(r, "");
                    }
                }
            }
            found
        })
    }
}

impl Default for LogGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for LogGuard {
    fn drop(&mut self) {
        let prev = self.previous.take();
        CAPTURE.with(|c| *c.borrow_mut() = prev);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture() {
        let g = LogGuard::new();
        log_warning("abc\ndef");
        assert_eq!(
            g.output(),
            "[OpenColorIO Warning]: abc\n[OpenColorIO Warning]: def\n"
        );
        assert!(g.find_and_remove("[OpenColorIO Warning]: abc"));
        assert_eq!(g.output(), "[OpenColorIO Warning]: def\n");
        g.clear();
        assert!(g.is_empty());
        {
            let g2 = LogGuard::with_level(LoggingLevel::None);
            log_warning("x");
            assert!(g2.is_empty());
        }
        log_info("y");
        assert_eq!(g.output(), "[OpenColorIO Info]: y\n");
    }
}
