//! LUT baking (TODO: port `Baker.cpp` and `BakingUtils.cpp`).

use crate::config::Config;
use crate::error::{Error, Result};
use crate::format_metadata::FormatMetadata;
use crate::processor::CpuProcessor;

/// Bakes a color transform from a config into a LUT file.
#[derive(Debug, Clone, Default)]
pub struct Baker {
    pub config: Option<Config>,
    pub format: String,
    pub metadata: FormatMetadata,
    pub input_space: String,
    pub shaper_space: String,
    pub looks: String,
    pub target_space: String,
    pub display: String,
    pub view: String,
    /// `None` means format default.
    pub shaper_size: Option<usize>,
    /// `None` means format default.
    pub cube_size: Option<usize>,
}

impl Baker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Bake into the configured format.
    pub fn bake(&self) -> Result<Vec<u8>> {
        Err(Error::msg("Baker::bake: not implemented"))
    }
}

/// Processor from the input space to the shaper space.
pub fn input_to_shaper_processor(_baker: &Baker) -> Result<CpuProcessor> {
    Err(Error::msg("not implemented"))
}

/// Processor from the shaper space to the input space.
pub fn shaper_to_input_processor(_baker: &Baker) -> Result<CpuProcessor> {
    Err(Error::msg("not implemented"))
}

/// Processor from the input space to the target (or display/view).
pub fn input_to_target_processor(_baker: &Baker) -> Result<CpuProcessor> {
    Err(Error::msg("not implemented"))
}

/// Processor from the shaper space to the target (or display/view).
pub fn shaper_to_target_processor(_baker: &Baker) -> Result<CpuProcessor> {
    Err(Error::msg("not implemented"))
}

/// Shaper range `(start, end)` in input space.
pub fn shaper_range(_baker: &Baker) -> Result<(f32, f32)> {
    Err(Error::msg("not implemented"))
}

/// Target range `(start, end)`.
pub fn target_range(_baker: &Baker) -> Result<(f32, f32)> {
    Err(Error::msg("not implemented"))
}
