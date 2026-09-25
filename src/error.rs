//! Error types.

use std::fmt;

/// Errors raised by the library.
///
/// `MissingFile` mirrors OCIO's `ExceptionMissingFile` and is used when a file
/// referenced by a transform or config cannot be located.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Generic OCIO exception.
    Exception(String),
    /// A referenced file could not be found.
    MissingFile(String),
}

impl Error {
    /// Build a generic exception from anything that displays.
    pub fn msg<S: Into<String>>(s: S) -> Self {
        Error::Exception(s.into())
    }

    /// Build a missing-file exception.
    pub fn missing_file<S: Into<String>>(s: S) -> Self {
        Error::MissingFile(s.into())
    }

    /// The error message.
    pub fn message(&self) -> &str {
        match self {
            Error::Exception(s) | Error::MissingFile(s) => s,
        }
    }

    /// Return a copy of the error with `prefix` prepended to the message.
    pub fn prefixed(&self, prefix: &str) -> Self {
        match self {
            Error::Exception(s) => Error::Exception(format!("{prefix}{s}")),
            Error::MissingFile(s) => Error::MissingFile(format!("{prefix}{s}")),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Exception(e.to_string())
    }
}

/// Convenience result alias.
pub type Result<T> = std::result::Result<T, Error>;

/// Early-return with an [`Error::Exception`] built from a format string.
#[macro_export]
macro_rules! bail {
    ($($arg:tt)*) => {
        return Err($crate::Error::Exception(format!($($arg)*)))
    };
}
