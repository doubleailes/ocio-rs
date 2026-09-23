//! # ocio
//!
//! A pure Rust port of [OpenColorIO](https://opencolorio.org), the color
//! management framework for visual effects and animation.
//!
//! ```no_run
//! use ocio::Config;
//!
//! let config = Config::create_from_file("config.ocio").unwrap();
//! let processor = config.get_processor("ACEScg", "sRGB - Display").unwrap();
//! let cpu = processor.default_cpu_processor();
//! let mut pixel = [0.18f32, 0.18, 0.18];
//! cpu.apply_rgb(&mut pixel);
//! ```

// Lints that conflict with a faithful port of the C++ numerics: float
// literals are kept exactly as in OCIO, index loops mirror the original
// code, and negated comparisons preserve the NaN semantics of the C++.
#![allow(
    clippy::excessive_precision,
    clippy::needless_range_loop,
    clippy::neg_cmp_op_on_partial_ord,
    clippy::too_many_arguments,
    clippy::field_reassign_with_default,
    clippy::manual_clamp,
    clippy::neg_multiply,
    clippy::nonminimal_bool,
    clippy::enum_variant_names,
    clippy::blocks_in_conditions,
    clippy::redundant_closure_call,
    clippy::wrong_self_convention,
    clippy::ptr_arg
)]
#![cfg_attr(test, allow(clippy::approx_constant))]

pub mod apphelpers;
pub mod baker;
pub mod builtins;
pub mod config;
pub mod context;
pub mod dynamic_property;
pub mod error;
pub mod fileformats;
pub mod format_metadata;
pub mod hash_utils;
pub mod image_desc;
pub mod math_utils;
pub mod ops;
pub mod path_utils;
pub mod processor;
pub mod transforms;
pub mod types;

pub use baker::Baker;
pub use config::Config;
pub use context::Context;
pub use dynamic_property::{DynamicProperty, SharedValue};
pub use error::{Error, Result};
pub use format_metadata::FormatMetadata;
pub use image_desc::{ImageData, ImageDesc, PackedImageDesc, PlanarImageDesc};
pub use processor::{CpuProcessor, Processor, ProcessorMetadata};
pub use transforms::grading::*;
pub use transforms::*;
pub use types::*;

/// Clear all the caches of the library (port of `ClearAllCaches`): the file
/// transform cache (files are read again on the next use).
pub fn clear_all_caches() {
    fileformats::file_transform::clear_file_transform_caches();
}

/// Library version (matches the OCIO version this port tracks).
pub const OCIO_VERSION: &str = "2.5.0";
/// Version of this crate.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
