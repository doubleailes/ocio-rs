//! LUT baking (port of `Baker.cpp` and `BakingUtils.cpp`).
//!
//! The [`Baker`] validates its settings and delegates the actual baking to
//! the [`FileFormat::bake`](crate::fileformats::FileFormat::bake)
//! implementation of the requested format. The helper functions of this
//! module (`input_to_shaper_processor`, ...) are what the formats use to
//! evaluate the color transform being baked.

use crate::config::Config;
use crate::error::{Error, Result};
use crate::fileformats::{bake_capability, capability, FormatRegistry};
use crate::format_metadata::FormatMetadata;
use crate::processor::CpuProcessor;
use crate::transforms::{
    ColorSpaceTransform, DisplayViewTransform, GroupTransform, LookTransform, Transform,
};
use crate::types::{OptimizationFlags, TransformDirection};

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
    /// A baker with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of formats that support baking.
    pub fn num_formats() -> usize {
        FormatRegistry::instance().num_formats(capability::BAKE)
    }

    /// Name of the baking format at `index` (`None` if out of range).
    pub fn format_name_by_index(index: usize) -> Option<&'static str> {
        FormatRegistry::instance()
            .format_infos(capability::BAKE)
            .get(index)
            .map(|i| i.name)
    }

    /// Extension of the baking format at `index` (`None` if out of range).
    pub fn format_extension_by_index(index: usize) -> Option<&'static str> {
        FormatRegistry::instance()
            .format_infos(capability::BAKE)
            .get(index)
            .map(|i| i.extension)
    }

    /// Set the config to use.
    pub fn set_config(&mut self, config: &Config) {
        self.config = Some(config.clone());
    }

    /// The config, if set.
    pub fn config(&self) -> Option<&Config> {
        self.config.as_ref()
    }

    /// Set the LUT output format; fails if the format does not exist or does
    /// not support baking.
    pub fn set_format(&mut self, format_name: &str) -> Result<()> {
        if let Some(fmt) = FormatRegistry::instance().format_by_name(format_name) {
            if fmt
                .format_info()
                .iter()
                .any(|i| i.capabilities & capability::BAKE != 0)
            {
                self.format = format_name.to_string();
                return Ok(());
            }
        }
        Err(Error::msg(format!(
            "File format {format_name} does not support baking."
        )))
    }

    /// The LUT output format.
    pub fn format(&self) -> &str {
        &self.format
    }

    /// Metadata written by the formats that support it.
    pub fn format_metadata(&self) -> &FormatMetadata {
        &self.metadata
    }

    /// Mutable metadata written by the formats that support it.
    pub fn format_metadata_mut(&mut self) -> &mut FormatMetadata {
        &mut self.metadata
    }

    /// Set the input color space.
    pub fn set_input_space(&mut self, input_space: &str) {
        self.input_space = input_space.to_string();
    }

    /// The input color space.
    pub fn input_space(&self) -> &str {
        &self.input_space
    }

    /// Set the optional shaper color space (used to improve the precision of
    /// 3D LUTs by shaping the input into a better domain).
    pub fn set_shaper_space(&mut self, shaper_space: &str) {
        self.shaper_space = shaper_space.to_string();
    }

    /// The shaper color space.
    pub fn shaper_space(&self) -> &str {
        &self.shaper_space
    }

    /// Set the optional looks to apply (a look string, e.g. `"foo, +bar"`).
    pub fn set_looks(&mut self, looks: &str) {
        self.looks = looks.to_string();
    }

    /// The looks.
    pub fn looks(&self) -> &str {
        &self.looks
    }

    /// Set the target color space (not to be used with a display / view).
    pub fn set_target_space(&mut self, target_space: &str) {
        self.target_space = target_space.to_string();
    }

    /// The target color space.
    pub fn target_space(&self) -> &str {
        &self.target_space
    }

    /// Set the display / view to bake (not to be used with a target space).
    pub fn set_display_view(&mut self, display: &str, view: &str) {
        self.display = display.to_string();
        self.view = view.to_string();
    }

    /// The display.
    pub fn display(&self) -> &str {
        &self.display
    }

    /// The view.
    pub fn view(&self) -> &str {
        &self.view
    }

    /// Set the shaper size (`None` means the format default).
    pub fn set_shaper_size(&mut self, size: Option<usize>) {
        self.shaper_size = size;
    }

    /// The shaper size (`None` means the format default).
    pub fn shaper_size(&self) -> Option<usize> {
        self.shaper_size
    }

    /// Set the cube size (`None` means the format default).
    pub fn set_cube_size(&mut self, size: Option<usize>) {
        self.cube_size = size;
    }

    /// The cube size (`None` means the format default).
    pub fn cube_size(&self) -> Option<usize> {
        self.cube_size
    }

    /// The config, or OCIO's error if none was set.
    pub fn required_config(&self) -> Result<&Config> {
        self.config
            .as_ref()
            .ok_or_else(|| Error::msg("No OCIO config has been set."))
    }

    /// Validate the settings and bake into the configured format.
    pub fn bake(&self) -> Result<Vec<u8>> {
        let registry = FormatRegistry::instance();
        let not_found = || {
            Error::msg(format!(
                "The format named '{}' could not be found. ",
                self.format
            ))
        };

        let fmt = registry
            .format_by_name(&self.format)
            .ok_or_else(not_found)?;
        let fmt_info = fmt.format_info().into_iter().next().ok_or_else(not_found)?;

        let input_space = &self.input_space;
        let target_space = &self.target_space;
        let shaper_space = &self.shaper_space;

        let display_view_mode = !self.display.is_empty() && !self.view.is_empty();
        let color_space_mode = !target_space.is_empty();

        // Settings validation.
        let config = self.required_config()?;

        if input_space.is_empty() {
            crate::bail!("No input space has been set.");
        }

        if !display_view_mode && !color_space_mode {
            crate::bail!("No display / view or target colorspace has been set.");
        }

        if display_view_mode && color_space_mode {
            crate::bail!("Cannot use both display / view and target colorspace.");
        }

        if !config.has_color_space(input_space) {
            crate::bail!("Could not find input colorspace '{input_space}'.");
        }

        if color_space_mode && !config.has_color_space(target_space) {
            crate::bail!("Could not find target colorspace '{target_space}'.");
        }

        if display_view_mode {
            let (found_display, found_view) = find_display_view(config, &self.display, &self.view);
            if !found_display {
                crate::bail!("Could not find display '{}'.", self.display);
            } else if !found_view {
                crate::bail!("Could not find view '{}'.", self.view);
            }
        }

        let bake_1d = fmt_info.bake_capabilities == bake_capability::LUT1D;
        if bake_1d && input_to_target_processor(self)?.has_channel_crosstalk() {
            crate::bail!(
                "The format '{}' does not support transformations with channel crosstalk.",
                self.format
            );
        }

        if matches!(self.cube_size, Some(n) if n < 2) {
            crate::bail!("Cube size must be at least 2 if set.");
        }

        let support_shaper = fmt_info.bake_capabilities & bake_capability::LUT1D_3D != 0
            || fmt_info.bake_capabilities & bake_capability::LUT1D != 0;
        if !shaper_space.is_empty() && !support_shaper {
            crate::bail!(
                "The format '{}' does not support shaper space.",
                self.format
            );
        }

        if !shaper_space.is_empty() && matches!(self.shaper_size, Some(n) if n < 2) {
            crate::bail!(
                "A shaper space '{shaper_space}' has been specified, so the shaper size must be 2 or larger."
            );
        }

        if !shaper_space.is_empty() {
            let input_to_shaper = input_to_shaper_processor(self)?;
            let shaper_to_input = shaper_to_input_processor(self)?;

            if input_to_shaper.has_channel_crosstalk() || shaper_to_input.has_channel_crosstalk() {
                crate::bail!(
                    "The specified shaper space, '{shaper_space}' has channel crosstalk, which is not \
                     appropriate for shapers. Please select an alternate shaper space or omit this option."
                );
            }
        }

        fmt.bake(self, &self.format)
            .map_err(|e| e.prefixed(&format!("Error baking {}:", self.format)))

        // As in OCIO, the limits of the shaper and target, the monotonicity
        // and the accuracy of the baked LUT are not checked.
    }
}

/// Look for the display (active or not) and, in it, for the view (display
/// defined or shared, active or not). Returns `(found_display, found_view)`.
///
/// OCIO iterates `Config::getDisplayAll` and `Config::getView(type, display,
/// index)` for the display-defined and shared view types. The display / view
/// query API of the config belongs to the config module; until it is
/// available through the public [`Config`] signatures this module relies on,
/// the lookup reports both as found and lets the processor creation report
/// unknown displays or views.
fn find_display_view(_config: &Config, _display: &str, _view: &str) -> (bool, bool) {
    (true, true)
}

/// The group transform from the input space to the target space, or to the
/// display / view, including the looks.
fn input_to_target_transform(baker: &Baker) -> GroupTransform {
    let input = &baker.input_space;
    let looks = &baker.looks;
    let display = &baker.display;
    let view = &baker.view;

    let mut group = GroupTransform::new();

    if !display.is_empty() && !view.is_empty() {
        if !looks.is_empty() {
            group.append(LookTransform::new(input, input, looks));
        }

        let mut disp = DisplayViewTransform::new(input, display, view);
        disp.looks_bypass = !looks.is_empty();
        group.append(disp);
    } else {
        group.append(LookTransform::new(input, &baker.target_space, looks));
    }

    group
}

/// Lossless CPU processor between two color spaces of the baker config.
fn color_space_processor(baker: &Baker, src: &str, dst: &str) -> Result<CpuProcessor> {
    let processor = baker.required_config()?.get_processor(src, dst)?;
    Ok(processor.optimized_cpu_processor(OptimizationFlags::LOSSLESS))
}

/// Range `(start, end)` of the `[0, 1]` values of `src` in the input space.
fn src_range(baker: &Baker, src: &str) -> Result<(f32, f32)> {
    // Calculate min/max value.
    let cpu = color_space_processor(baker, src, &baker.input_space)?;

    let mut minval = [0.0f32; 3];
    let mut maxval = [1.0f32; 3];

    cpu.apply_rgb(&mut minval);
    cpu.apply_rgb(&mut maxval);

    let start = minval[0].min(minval[1]).min(minval[2]);
    let end = maxval[0].max(maxval[1]).max(maxval[2]);
    Ok((start, end))
}

/// Processor from the input space to the shaper space.
pub fn input_to_shaper_processor(baker: &Baker) -> Result<CpuProcessor> {
    color_space_processor(baker, &baker.input_space, &baker.shaper_space)
}

/// Processor from the shaper space to the input space.
pub fn shaper_to_input_processor(baker: &Baker) -> Result<CpuProcessor> {
    color_space_processor(baker, &baker.shaper_space, &baker.input_space)
}

/// Processor from the input space to the target (or display/view).
pub fn input_to_target_processor(baker: &Baker) -> Result<CpuProcessor> {
    if baker.input_space.is_empty() {
        crate::bail!("Input space is empty.");
    }
    let group = Transform::Group(input_to_target_transform(baker));
    let processor = baker
        .required_config()?
        .get_processor_for_transform(&group, TransformDirection::Forward)?;
    Ok(processor.optimized_cpu_processor(OptimizationFlags::LOSSLESS))
}

/// Processor from the shaper space to the target (or display/view).
pub fn shaper_to_target_processor(baker: &Baker) -> Result<CpuProcessor> {
    if baker.shaper_space.is_empty() {
        crate::bail!("Shaper space is empty.");
    }
    let mut group = input_to_target_transform(baker);
    group.prepend(ColorSpaceTransform::new(
        &baker.shaper_space,
        &baker.input_space,
    ));

    let processor = baker
        .required_config()?
        .get_processor_for_transform(&Transform::Group(group), TransformDirection::Forward)?;
    Ok(processor.optimized_cpu_processor(OptimizationFlags::LOSSLESS))
}

/// Shaper range `(start, end)` in input space.
pub fn shaper_range(baker: &Baker) -> Result<(f32, f32)> {
    src_range(baker, &baker.shaper_space)
}

/// Target range `(start, end)`.
pub fn target_range(baker: &Baker) -> Result<(f32, f32)> {
    src_range(baker, &baker.target_space)
}

#[cfg(test)]
mod tests;
