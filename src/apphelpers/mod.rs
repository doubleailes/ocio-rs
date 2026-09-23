//! Application helpers (port of `OpenColorAppHelpers.h` and
//! `src/OpenColorIO/apphelpers`): color space menus, (display, view)
//! helpers, the legacy viewing pipeline, color mixing helpers and config
//! merging.
//!
//! Shared C++ pointers are mapped to `Arc<Config>` where a helper keeps a
//! config (menus, mixing) and to plain references elsewhere.

pub mod category_helpers;
pub mod color_space_helpers;
pub mod display_view_helpers;
pub mod legacy_viewing_pipeline;
pub mod merge_configs;
pub mod mixing_helpers;

pub use color_space_helpers::{
    add_color_space, ColorSpaceInfo, ColorSpaceMenuHelper, ColorSpaceMenuParameters,
};
pub use legacy_viewing_pipeline::LegacyViewingPipeline;
pub use merge_configs::{
    merge_color_space, merge_configs, ConfigMerger, ConfigMergingParameters, MergeStrategies,
};
pub use mixing_helpers::{MixingColorSpaceManager, MixingSlider};

#[cfg(test)]
pub(crate) mod tests_data {
    /// The configuration used by the app helpers unit tests (`configs.data`).
    pub const CATEGORY_TEST_CONFIG: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/data/files/apphelpers/category_test_config.ocio"
    ));
}
