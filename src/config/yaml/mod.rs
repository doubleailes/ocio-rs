//! YAML reading and writing of configs (port of `OCIOYaml.cpp`).
//!
//! Parsing uses `yaml-rust2` events to build a [`node::Node`] tree keeping
//! tags, key order and marks; writing uses a port of the `yaml-cpp` emitter
//! ([`emitter::Emitter`]) to reproduce OCIO's exact output.

pub mod emitter;
pub mod node;
mod reader;
mod writer;

pub use reader::{load_transform, read};
pub use writer::{save_transform, write};

/// Special file name used when a config is read from an archive (the working
/// directory is not set in that case).
pub const ARCHIVE_FILENAME: &str = "from Archive/ConfigIOProxy";

/// Remove the trailing newlines (`SanitizeNewlines`).
pub(crate) fn sanitize_newlines(s: &str) -> String {
    s.trim_end_matches('\n').to_string()
}
