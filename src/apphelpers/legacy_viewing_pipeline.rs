//! A complete viewing pipeline around a display / view transform (port of
//! `apphelpers/LegacyViewingPipeline.cpp`).

use crate::config::transform_display::format_transform;
use crate::config::{get_looks_result_color_space, Config};
use crate::context::Context;
use crate::error::{Error, Result};
use crate::processor::Processor;
use crate::transforms::{
    ColorSpaceTransform, DisplayViewTransform, GroupTransform, LookTransform, Transform,
};
use crate::types::{
    TransformDirection, OCIO_VIEW_USE_DISPLAY_NAME, ROLE_COLOR_TIMING, ROLE_SCENE_LINEAR,
};
use std::fmt;

/// Whereas the display / view transform simply applies a view of a display,
/// the legacy viewing pipeline adds, around it, optional color corrections
/// and a channel view (same functionality as the OCIO v1 `DisplayTransform`).
/// The direction of the display / view transform is the direction of the
/// pipeline:
///
/// * start in the display transform input color space,
/// * if a linear CC is provided, go to `scene_linear` and apply it,
/// * if a color timing CC is provided, go to `color_timing` and apply it,
/// * apply the looks (from the display / view or from the looks override),
/// * apply the channel view,
/// * apply the display / view transform (without the looks),
/// * apply the display CC.
///
/// Looks are applied even if the display transform involves data color
/// spaces.
#[derive(Debug, Clone, Default)]
pub struct LegacyViewingPipeline {
    linear_cc: Option<Transform>,
    color_timing_cc: Option<Transform>,
    channel_view: Option<Transform>,
    display_cc: Option<Transform>,
    display_view_transform: Option<DisplayViewTransform>,
    // Looks from the display / view transform are applied separately.
    dt_original_looks_bypass: bool,
    looks_override_enabled: bool,
    looks_override: String,
}

impl LegacyViewingPipeline {
    /// An empty pipeline (`LegacyViewingPipeline::Create`).
    pub fn new() -> Self {
        Self::default()
    }

    /// The display / view transform (its looks bypass is always on, the
    /// looks being applied by the pipeline).
    pub fn display_view_transform(&self) -> Option<&DisplayViewTransform> {
        self.display_view_transform.as_ref()
    }

    /// Set (a copy of) the display / view transform.
    pub fn set_display_view_transform(&mut self, dt: Option<&DisplayViewTransform>) {
        match dt {
            Some(dt) => {
                let mut dt = dt.clone();
                self.dt_original_looks_bypass = dt.looks_bypass;
                dt.looks_bypass = true;
                self.display_view_transform = Some(dt);
            }
            None => self.display_view_transform = None,
        }
    }

    /// The linear color correction.
    pub fn linear_cc(&self) -> Option<&Transform> {
        self.linear_cc.as_ref()
    }

    /// Set (a copy of) the linear color correction.
    pub fn set_linear_cc(&mut self, cc: Option<&Transform>) {
        self.linear_cc = cc.cloned();
    }

    /// The color timing color correction.
    pub fn color_timing_cc(&self) -> Option<&Transform> {
        self.color_timing_cc.as_ref()
    }

    /// Set (a copy of) the color timing color correction.
    pub fn set_color_timing_cc(&mut self, cc: Option<&Transform>) {
        self.color_timing_cc = cc.cloned();
    }

    /// The channel view transform.
    pub fn channel_view(&self) -> Option<&Transform> {
        self.channel_view.as_ref()
    }

    /// Set (a copy of) the channel view transform.
    pub fn set_channel_view(&mut self, transform: Option<&Transform>) {
        self.channel_view = transform.cloned();
    }

    /// The display color correction.
    pub fn display_cc(&self) -> Option<&Transform> {
        self.display_cc.as_ref()
    }

    /// Set (a copy of) the display color correction.
    pub fn set_display_cc(&mut self, cc: Option<&Transform>) {
        self.display_cc = cc.cloned();
    }

    /// Enable the use of the looks override (a separate flag, since it is
    /// often useful to override the looks with an empty string).
    pub fn set_looks_override_enabled(&mut self, enable: bool) {
        self.looks_override_enabled = enable;
    }

    /// True if the looks override is used.
    pub fn looks_override_enabled(&self) -> bool {
        self.looks_override_enabled
    }

    /// Looks to use instead of the ones of the (display, view): a comma (or
    /// colon) separated list of look names with optional `+` / `-` prefixes.
    pub fn set_looks_override(&mut self, looks: &str) {
        self.looks_override = looks.to_string();
    }

    /// The looks override.
    pub fn looks_override(&self) -> &str {
        &self.looks_override
    }

    fn validate(&self) -> Result<&DisplayViewTransform> {
        let dt = self.display_view_transform.as_ref().ok_or_else(|| {
            Error::msg(
                "LegacyViewingPipeline: can't create a processor without a display transform.",
            )
        })?;

        let check = || -> Result<()> {
            Transform::DisplayView(dt.clone()).validate()?;
            for t in [
                &self.linear_cc,
                &self.color_timing_cc,
                &self.channel_view,
                &self.display_cc,
            ]
            .into_iter()
            .flatten()
            {
                t.validate()?;
            }
            Ok(())
        };
        check().map_err(|e| {
            Error::msg(format!(
                "LegacyViewingPipeline is not valid: {}",
                e.message()
            ))
        })?;
        Ok(dt)
    }

    /// The processor of the pipeline using the current context of the config.
    pub fn get_processor(&self, config: &Config) -> Result<Processor> {
        self.get_processor_with_context(config, config.current_context())
    }

    /// The processor of the pipeline.
    pub fn get_processor_with_context(
        &self,
        config: &Config,
        context: &Context,
    ) -> Result<Processor> {
        let display_view_transform = self.validate()?;

        // Get direction from display transform.
        let dir = display_view_transform.direction;

        let input_name = display_view_transform.src.clone();
        let input_cs = config.get_color_space(&input_name).ok_or_else(|| {
            if input_name.is_empty() {
                Error::msg("LegacyViewingPipeline error: InputColorSpaceName is unspecified.")
            } else {
                Error::msg(format!(
                    "LegacyViewingPipeline error: Cannot find inputColorSpace, named '{input_name}'."
                ))
            }
        })?;

        let display = display_view_transform.display.clone();
        let view = display_view_transform.view.clone();

        // NB: If the view has a view transform, then the display color space is a true display
        // color space rather than a traditional color space.
        let name = config.display_view_color_space_name(&display, &view);
        // A shared view containing a view transform may set the color space to
        // USE_DISPLAY_NAME, in which case we look for a display color space with the same name
        // as the display.
        let display_cs_name = if name == OCIO_VIEW_USE_DISPLAY_NAME {
            display.as_str()
        } else {
            name
        };
        // If this is not a color space it can be a named transform. Error handling (missing
        // color space or named transform) is handled by the display view transform.
        let display_cs = config.get_color_space(display_cs_name);

        let data_bypass = display_view_transform.data_bypass;
        let display_data = display_cs.map(|cs| cs.is_data()).unwrap_or(true);
        let mut skip_cs_conversions = data_bypass && (input_cs.is_data() || display_data);

        if data_bypass {
            // If we're viewing alpha, also skip all color space conversions.
            if let Some(Transform::Matrix(m)) = &self.channel_view {
                let m44 = m.matrix;
                if m44[3] > 0.0 || m44[7] > 0.0 || m44[11] > 0.0 {
                    skip_cs_conversions = true;
                }
            }
        }

        let mut current_cs_name = input_name.clone();
        let mut dt_input_cs = input_cs;

        let mut group = GroupTransform::new();

        if let Some(linear_cc) = &self.linear_cc {
            let proc = config.get_processor_with_context(context, linear_cc, dir)?;
            // If it is a no-op, dont bother doing the colorspace conversion.
            if !proc.is_no_op() {
                dt_input_cs = config.get_color_space(ROLE_SCENE_LINEAR).ok_or_else(|| {
                    Error::msg(format!(
                        "DisplayViewTransform error: LinearCC requires '{ROLE_SCENE_LINEAR}' role to be defined."
                    ))
                })?;

                if !skip_cs_conversions {
                    group.append(ColorSpaceTransform::new(
                        &current_cs_name,
                        ROLE_SCENE_LINEAR,
                    ));
                    current_cs_name = ROLE_SCENE_LINEAR.to_string();
                }
                group.append(linear_cc.clone());
            }
        }

        if let Some(color_timing_cc) = &self.color_timing_cc {
            let proc = config.get_processor_with_context(context, color_timing_cc, dir)?;
            // If it is a no-op, dont bother doing the colorspace conversion.
            if !proc.is_no_op() {
                dt_input_cs = config.get_color_space(ROLE_COLOR_TIMING).ok_or_else(|| {
                    Error::msg(format!(
                        "DisplayViewTransform error: ColorTimingCC requires '{ROLE_COLOR_TIMING}' role to be defined."
                    ))
                })?;

                if !skip_cs_conversions {
                    group.append(ColorSpaceTransform::new(
                        &current_cs_name,
                        ROLE_COLOR_TIMING,
                    ));
                    current_cs_name = ROLE_COLOR_TIMING.to_string();
                }
                group.append(color_timing_cc.clone());
            }
        }

        let mut dt = display_view_transform.clone();
        dt.direction = TransformDirection::Forward;

        // Adjust display transform input color space.
        dt.src = current_cs_name;

        // NB: If looksOverrideEnabled is true, always apply the look, even to data color
        // spaces. In other cases, follow what the DisplayViewTransform would do, except skip
        // color space conversions to the process space for Look transforms for data spaces
        // (DisplayViewTransform never skips).
        let looks = if self.looks_override_enabled {
            self.looks_override.clone()
        } else if !self.dt_original_looks_bypass && !skip_cs_conversions {
            config.display_view_looks(&display, &view).to_string()
        } else {
            String::new()
        };

        if !looks.is_empty() {
            let in_cs = dt_input_cs.name().to_string();
            let out_cs = if skip_cs_conversions {
                in_cs.clone()
            } else {
                get_looks_result_color_space(config, context, &looks)?
            };

            // Resulting color space could be empty in case of a noop look.
            if !out_cs.is_empty() {
                let mut lt = LookTransform::new(&in_cs, &out_cs, &looks);
                lt.skip_color_space_conversion = skip_cs_conversions;
                group.append(lt);

                // Adjust display transform input color space.
                dt.src = out_cs;
            }
        }

        if let Some(cv) = &self.channel_view {
            group.append(cv.clone());
        }

        // If there is no display color space it should be a named transform and it has to be
        // applied.
        if !skip_cs_conversions || display_cs.is_none() {
            group.append(dt);
        }

        if let Some(cc) = &self.display_cc {
            group.append(cc.clone());
        }

        config.get_processor_with_context(context, &Transform::Group(group), dir)
    }
}

impl fmt::Display for LegacyViewingPipeline {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts: Vec<String> = Vec::new();
        if let Some(dt) = &self.display_view_transform {
            parts.push(format!(
                "DisplayViewTransform: {}",
                format_transform(&Transform::DisplayView(dt.clone()))
            ));
        }
        if let Some(t) = &self.linear_cc {
            parts.push(format!("LinearCC: {}", format_transform(t)));
        }
        if let Some(t) = &self.color_timing_cc {
            parts.push(format!("ColorTimingCC: {}", format_transform(t)));
        }
        if let Some(t) = &self.channel_view {
            parts.push(format!("ChannelView: {}", format_transform(t)));
        }
        if let Some(t) = &self.display_cc {
            parts.push(format!("DisplayCC: {}", format_transform(t)));
        }
        if self.looks_override_enabled {
            parts.push("LooksOverrideEnabled".to_string());
        }
        if !self.looks_override.is_empty() {
            parts.push(format!("LooksOverride: {}", self.looks_override));
        }
        f.write_str(&parts.join(", "))
    }
}
