//! Viewing pipeline helpers used by the tools (ports of the OCIO app helpers
//! `LegacyViewingPipeline` and `DisplayViewHelpers::GetProcessor`).

use ocio::config::get_looks_result_color_space;
use ocio::{
    ColorSpaceTransform, Config, Context, DisplayViewTransform, Error, ExposureContrastStyle,
    ExposureContrastTransform, GroupTransform, LookTransform, Processor, Result, Transform,
    TransformDirection, OCIO_VIEW_USE_DISPLAY_NAME, ROLE_COLOR_TIMING, ROLE_SCENE_LINEAR,
};

/// The viewing pipeline of OCIO v1 (port of `LegacyViewingPipeline`): a
/// display / view transform optionally preceded by color corrections in the
/// scene linear and color timing spaces, a channel view and followed by a
/// display color correction.
#[derive(Debug, Clone, Default)]
pub struct LegacyViewingPipeline {
    display_view_transform: Option<DisplayViewTransform>,
    dt_original_looks_bypass: bool,
    linear_cc: Option<Transform>,
    color_timing_cc: Option<Transform>,
    channel_view: Option<Transform>,
    display_cc: Option<Transform>,
    looks_override_enabled: bool,
    looks_override: String,
}

impl LegacyViewingPipeline {
    /// An empty pipeline.
    pub fn new() -> Self {
        Self::default()
    }

    /// The display / view transform.
    pub fn display_view_transform(&self) -> Option<&DisplayViewTransform> {
        self.display_view_transform.as_ref()
    }

    /// Set the display / view transform (its looks are applied by the
    /// pipeline, so the transform copy bypasses them).
    pub fn set_display_view_transform(&mut self, dt: Option<&DisplayViewTransform>) {
        match dt {
            Some(dt) => {
                let mut copy = dt.clone();
                self.dt_original_looks_bypass = copy.looks_bypass;
                copy.looks_bypass = true;
                self.display_view_transform = Some(copy);
            }
            None => self.display_view_transform = None,
        }
    }

    /// The scene linear color correction.
    pub fn linear_cc(&self) -> Option<&Transform> {
        self.linear_cc.as_ref()
    }

    /// Set the scene linear color correction.
    pub fn set_linear_cc(&mut self, cc: Option<Transform>) {
        self.linear_cc = cc;
    }

    /// The color timing color correction.
    pub fn color_timing_cc(&self) -> Option<&Transform> {
        self.color_timing_cc.as_ref()
    }

    /// Set the color timing color correction.
    pub fn set_color_timing_cc(&mut self, cc: Option<Transform>) {
        self.color_timing_cc = cc;
    }

    /// The channel view.
    pub fn channel_view(&self) -> Option<&Transform> {
        self.channel_view.as_ref()
    }

    /// Set the channel view.
    pub fn set_channel_view(&mut self, t: Option<Transform>) {
        self.channel_view = t;
    }

    /// The display color correction.
    pub fn display_cc(&self) -> Option<&Transform> {
        self.display_cc.as_ref()
    }

    /// Set the display color correction.
    pub fn set_display_cc(&mut self, cc: Option<Transform>) {
        self.display_cc = cc;
    }

    /// True if the looks override is used.
    pub fn looks_override_enabled(&self) -> bool {
        self.looks_override_enabled
    }

    /// Enable the looks override.
    pub fn set_looks_override_enabled(&mut self, enable: bool) {
        self.looks_override_enabled = enable;
    }

    /// The looks override.
    pub fn looks_override(&self) -> &str {
        &self.looks_override
    }

    /// Set the looks override.
    pub fn set_looks_override(&mut self, looks: &str) {
        self.looks_override = looks.to_string();
    }

    /// Validate the pipeline.
    pub fn validate(&self) -> Result<()> {
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
        })
    }

    /// Processor of the pipeline using the current context of the config.
    pub fn get_processor(&self, config: &Config) -> Result<Processor> {
        self.get_processor_with_context(config, config.current_context())
    }

    /// Processor of the pipeline.
    pub fn get_processor_with_context(
        &self,
        config: &Config,
        context: &Context,
    ) -> Result<Processor> {
        self.validate()?;
        let dvt = match &self.display_view_transform {
            Some(d) => d,
            None => {
                return Err(Error::msg(
                    "LegacyViewingPipeline: missing display transform.",
                ))
            }
        };
        let dir = dvt.direction;

        let input_name = dvt.src.clone();
        let input_cs = match config.get_color_space(&input_name) {
            Some(cs) => cs,
            None => {
                let msg = if input_name.is_empty() {
                    "InputColorSpaceName is unspecified.".to_string()
                } else {
                    format!("Cannot find inputColorSpace, named '{input_name}'.")
                };
                return Err(Error::msg(format!("LegacyViewingPipeline error: {msg}")));
            }
        };

        let display = dvt.display.clone();
        let view = dvt.view.clone();

        let name = config
            .display_view_color_space_name(&display, &view)
            .to_string();
        let display_cs_name = if name == OCIO_VIEW_USE_DISPLAY_NAME {
            display.clone()
        } else {
            name
        };
        let display_cs = config.get_color_space(&display_cs_name);

        let data_bypass = dvt.data_bypass;
        let display_data = display_cs.map(|cs| cs.is_data()).unwrap_or(true);
        let mut skip_conversions = data_bypass && (input_cs.is_data() || display_data);

        if data_bypass {
            if let Some(Transform::Matrix(m)) = &self.channel_view {
                if m.matrix[3] > 0.0 || m.matrix[7] > 0.0 || m.matrix[11] > 0.0 {
                    skip_conversions = true;
                }
            }
        }

        let mut current_cs = input_name.clone();
        let mut dt_input_cs_name = input_cs.name().to_string();

        let mut group = GroupTransform::new();

        let ccs = [
            (&self.linear_cc, ROLE_SCENE_LINEAR, "LinearCC"),
            (&self.color_timing_cc, ROLE_COLOR_TIMING, "ColorTimingCC"),
        ];
        for (cc, role, label) in ccs {
            if let Some(cc) = cc {
                let proc = config.get_processor_with_context(context, cc, dir)?;
                // If it is a no-op, don't bother doing the color space conversion.
                if !proc.is_no_op() {
                    match config.get_color_space(role) {
                        Some(cs) => dt_input_cs_name = cs.name().to_string(),
                        None => {
                            return Err(Error::msg(format!(
                                "DisplayViewTransform error: {label} requires '{role}' role to be defined."
                            )))
                        }
                    }
                    if !skip_conversions {
                        group.append(ColorSpaceTransform::new(&current_cs, role));
                        current_cs = role.to_string();
                    }
                    group.append(cc.clone());
                }
            }
        }

        let mut dt = dvt.clone();
        dt.direction = TransformDirection::Forward;
        // Adjust the display transform input color space.
        dt.src = current_cs.clone();

        // If the looks override is enabled, always apply the looks, even to
        // data color spaces. Otherwise follow the display / view transform
        // except for skipping the color space conversions for data spaces.
        let looks = if self.looks_override_enabled {
            self.looks_override.clone()
        } else if !self.dt_original_looks_bypass && !skip_conversions {
            config.display_view_looks(&display, &view).to_string()
        } else {
            String::new()
        };

        if !looks.is_empty() {
            let in_cs = dt_input_cs_name.clone();
            let out_cs = if skip_conversions {
                in_cs.clone()
            } else {
                get_looks_result_color_space(config, context, &looks)?
            };
            // The resulting color space could be empty for a no-op look.
            if !out_cs.is_empty() {
                let mut lt = LookTransform::new(&in_cs, &out_cs, &looks);
                lt.skip_color_space_conversion = skip_conversions;
                group.append(lt);
                dt.src = out_cs;
            }
        }

        if let Some(cv) = &self.channel_view {
            group.append(cv.clone());
        }

        // Without a display color space, it is a named transform which has to
        // be applied.
        if !skip_conversions || display_cs.is_none() {
            group.append(dt);
        }

        if let Some(cc) = &self.display_cc {
            group.append(cc.clone());
        }

        config.get_processor_with_context(context, &Transform::Group(group), dir)
    }
}

/// Processor of a (display, view) pair adding the dynamic exposure, contrast
/// and gamma controls a viewer needs (port of
/// `DisplayViewHelpers::GetProcessor`).
pub fn get_display_view_processor(
    config: &Config,
    working_name: &str,
    display: &str,
    view: &str,
    channel_view: Option<&Transform>,
    direction: TransformDirection,
) -> Result<Processor> {
    get_display_view_processor_with_context(
        config,
        config.current_context(),
        working_name,
        display,
        view,
        channel_view,
        direction,
    )
}

/// Same as [`get_display_view_processor`] with an explicit context.
pub fn get_display_view_processor_with_context(
    config: &Config,
    context: &Context,
    working_name: &str,
    display: &str,
    view: &str,
    channel_view: Option<&Transform>,
    direction: TransformDirection,
) -> Result<Processor> {
    let mut dvt = DisplayViewTransform::new(working_name, display, view);
    dvt.direction = direction;

    let processor = config.get_processor_with_context(
        context,
        &Transform::DisplayView(dvt.clone()),
        TransformDirection::Forward,
    )?;

    let mut need_exposure = true;
    let mut need_gamma = true;

    if processor.is_dynamic() {
        let group = processor.create_group_transform();
        for t in &group.transforms {
            if let Transform::ExposureContrast(ex) = t {
                if ex.exposure_dynamic {
                    need_exposure = false;
                }
                if ex.gamma_dynamic {
                    need_gamma = false;
                }
            }
        }
    }

    if !need_exposure && !need_gamma && channel_view.is_none() {
        return Ok(processor);
    }

    let mut pipeline = LegacyViewingPipeline::new();
    pipeline.set_display_view_transform(Some(&dvt));

    if need_exposure {
        let ex = ExposureContrastTransform {
            style: ExposureContrastStyle::Linear,
            pivot: 0.18,
            exposure_dynamic: true,
            contrast_dynamic: true,
            ..Default::default()
        };
        pipeline.set_linear_cc(Some(Transform::ExposureContrast(ex)));
    }

    if need_gamma {
        let ex = ExposureContrastTransform {
            style: ExposureContrastStyle::Video,
            pivot: 1.0,
            gamma_dynamic: true,
            ..Default::default()
        };
        pipeline.set_display_cc(Some(Transform::ExposureContrast(ex)));
    }

    if let Some(cv) = channel_view {
        pipeline.set_channel_view(Some(cv.clone()));
    }

    pipeline.get_processor_with_context(config, context)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ocio::DynamicPropertyType;

    const CONFIG: &str = r#"ocio_profile_version: 2

roles:
  default: raw
  scene_linear: lin
  color_timing: log

displays:
  sRGB:
    - !<View> {name: Film, colorspace: log, looks: plus}
    - !<View> {name: Raw, colorspace: raw}

looks:
  - !<Look>
    name: plus
    process_space: lin
    transform: !<MatrixTransform> {offset: [0.1, 0.1, 0.1, 0]}

colorspaces:
  - !<ColorSpace>
    name: raw
    isdata: true

  - !<ColorSpace>
    name: lin

  - !<ColorSpace>
    name: log
    from_scene_reference: !<LogTransform> {base: 2}
"#;

    #[test]
    fn display_view_processor() {
        let config = Config::create_from_str(CONFIG).unwrap();
        let p = get_display_view_processor(
            &config,
            "lin",
            "sRGB",
            "Film",
            None,
            TransformDirection::Forward,
        )
        .unwrap();
        assert!(p.is_dynamic());
        assert!(p.has_dynamic_property(DynamicPropertyType::Exposure));
        assert!(p.has_dynamic_property(DynamicPropertyType::Contrast));
        assert!(p.has_dynamic_property(DynamicPropertyType::Gamma));

        // With the default values of the dynamic properties, it matches the
        // display / view transform (the look is applied).
        let mut px = [0.4f32, 0.4, 0.4];
        p.default_cpu_processor().apply_rgb(&mut px);
        assert!((px[0] - 0.5f32.log2()).abs() < 1e-5, "{px:?}");

        // Raw is a data view.
        let p = get_display_view_processor(
            &config,
            "lin",
            "sRGB",
            "Raw",
            None,
            TransformDirection::Forward,
        )
        .unwrap();
        let mut px = [0.4f32, 0.4, 0.4];
        p.default_cpu_processor().apply_rgb(&mut px);
        assert!((px[0] - 0.4).abs() < 1e-6);
    }

    #[test]
    fn pipeline() {
        let config = Config::create_from_str(CONFIG).unwrap();
        let mut vp = LegacyViewingPipeline::new();
        assert_eq!(
            vp.get_processor(&config).unwrap_err().message(),
            "LegacyViewingPipeline: can't create a processor without a display transform."
        );

        vp.set_display_view_transform(Some(&DisplayViewTransform::new("lin", "sRGB", "Film")));
        vp.set_looks_override_enabled(true);
        vp.set_looks_override("");
        // The looks are overridden (none).
        let mut px = [0.5f32, 0.5, 0.5];
        vp.get_processor(&config)
            .unwrap()
            .default_cpu_processor()
            .apply_rgb(&mut px);
        assert!((px[0] + 1.0).abs() < 1e-5, "{px:?}");

        // A color timing correction is applied in the color_timing space.
        vp.set_color_timing_cc(Some(Transform::Matrix(ocio::MatrixTransform::new(
            ocio::MatrixTransform::identity().0,
            [0.5, 0.5, 0.5, 0.0],
        ))));
        let p = vp.get_processor(&config).unwrap();
        let mut px = [0.5f32, 0.5, 0.5];
        p.default_cpu_processor().apply_rgb(&mut px);
        // lin -> log gives -1, plus 0.5, then log -> log.
        assert!((px[0] + 0.5).abs() < 1e-5, "{px:?}");

        let mut vp2 = LegacyViewingPipeline::new();
        vp2.set_display_view_transform(Some(&DisplayViewTransform::new("unknown", "sRGB", "Film")));
        assert_eq!(
            vp2.get_processor(&config).unwrap_err().message(),
            "LegacyViewingPipeline error: Cannot find inputColorSpace, named 'unknown'."
        );
    }
}
