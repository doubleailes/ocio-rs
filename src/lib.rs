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
pub mod gpu;
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

/// Library version (matches the OCIO version this port tracks: `OCIO_VERSION`,
/// set by `project(OpenColorIO VERSION 2.6.0)` in OCIO's CMakeLists.txt).
pub const OCIO_VERSION: &str = "2.6.0";
/// Release type of the tracked OCIO version (`OCIO_VERSION_STATUS_STR`).
pub const OCIO_VERSION_STATUS_STR: &str = "dev";
/// Full version string (`OCIO_VERSION_FULL_STR`), as returned by `OCIO::GetVersion`.
pub const OCIO_VERSION_FULL_STR: &str = "2.6.0dev";
/// Version as an integer, `0xMMmmpp00` (`OCIO_VERSION_HEX`, `OCIO::GetVersionHex`).
pub const OCIO_VERSION_HEX: u32 = 0x0206_0000;

/// The library version, e.g. "2.6.0dev" (port of `GetVersion`).
pub fn get_version() -> &'static str {
    OCIO_VERSION_FULL_STR
}

/// The library version as an integer, `0xMMmmpp00` (port of `GetVersionHex`).
pub fn get_version_hex() -> u32 {
    OCIO_VERSION_HEX
}
/// Version of this crate.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod version_tests {
    use super::*;

    #[test]
    fn version_matches_ocio_2_6() {
        // Values reported by the OCIO 2.6 C++ library.
        assert_eq!(get_version(), "2.6.0dev");
        assert_eq!(get_version_hex(), 0x0206_0000);
        assert_eq!(
            OCIO_VERSION_FULL_STR,
            format!("{OCIO_VERSION}{OCIO_VERSION_STATUS_STR}")
        );
        let mut parts = OCIO_VERSION.split('.').map(|p| p.parse::<u32>().unwrap());
        let hex = (parts.next().unwrap() << 24)
            | (parts.next().unwrap() << 16)
            | (parts.next().unwrap() << 8);
        assert_eq!(hex, OCIO_VERSION_HEX);
        // The latest supported config version follows the library version.
        assert_eq!(
            config::LAST_SUPPORTED_MINOR_VERSION[1],
            (OCIO_VERSION_HEX >> 16) & 0xff
        );

        let err = Config::create_from_str("ocio_profile_version: 2.7\n").unwrap_err();
        assert!(
            err.to_string().contains(
                "This .ocio config is version 2.7. This version of the OpenColorIO library \
                 (2.6.0dev) is not able to load that config version.\n\
                 The minor version 7 is not supported for major version 2. Maximum minor \
                 version is 6."
            ),
            "{err}"
        );
    }
}
