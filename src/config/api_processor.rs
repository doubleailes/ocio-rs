//! Config API to build processors (`Config::getProcessor` variants,
//! `GetProcessorFromConfigs`, processor cache) and `isColorSpaceLinear`.

use super::context_vars::collect_context_variables;
use super::*;
use crate::ops::OpVec;
use crate::transforms::{ColorSpaceTransform, DisplayViewTransform};

/// Validate `transform` then build its processor (port of
/// `Processor::Impl::setTransform`, which validates the transform first).
fn create_processor(
    config: &Config,
    context: &Context,
    transform: &Transform,
    dir: TransformDirection,
) -> Result<Processor> {
    transform.validate()?;
    Processor::from_transform(config, context, transform, dir)
}

impl Config {
    /// Processor converting between two color spaces (or roles).
    pub fn get_processor(&self, src: &str, dst: &str) -> Result<Processor> {
        let t = ColorSpaceTransform::new(src, dst);
        self.get_processor_for_transform(&Transform::ColorSpace(t), TransformDirection::Forward)
    }

    /// Processor converting between two color spaces (or roles) using a
    /// context.
    pub fn get_processor_with_context_names(
        &self,
        context: &Context,
        src: &str,
        dst: &str,
    ) -> Result<Processor> {
        let t = ColorSpaceTransform::new(src, dst);
        self.get_processor_with_context(
            context,
            &Transform::ColorSpace(t),
            TransformDirection::Forward,
        )
    }

    /// Processor converting between two color space objects (by name).
    pub fn get_processor_color_spaces(
        &self,
        src: &ColorSpace,
        dst: &ColorSpace,
    ) -> Result<Processor> {
        self.get_processor_color_spaces_with_context(&self.context, src, dst)
    }

    /// Processor converting between two color space objects using a context.
    pub fn get_processor_color_spaces_with_context(
        &self,
        context: &Context,
        src: &ColorSpace,
        dst: &ColorSpace,
    ) -> Result<Processor> {
        let t = ColorSpaceTransform::new(src.name(), dst.name());
        self.get_processor_with_context(
            context,
            &Transform::ColorSpace(t),
            TransformDirection::Forward,
        )
    }

    /// Processor for a transform, using the current context.
    pub fn get_processor_for_transform(
        &self,
        transform: &Transform,
        dir: TransformDirection,
    ) -> Result<Processor> {
        self.get_processor_with_context(self.current_context(), transform, dir)
    }

    /// Processor from a color space to a display / view.
    pub fn get_display_view_processor(
        &self,
        src: &str,
        display: &str,
        view: &str,
    ) -> Result<Processor> {
        self.get_display_view_processor_dir(src, display, view, TransformDirection::Forward)
    }

    /// Processor from a color space to a display / view in a direction.
    pub fn get_display_view_processor_dir(
        &self,
        src: &str,
        display: &str,
        view: &str,
        dir: TransformDirection,
    ) -> Result<Processor> {
        self.get_display_view_processor_with_context(&self.context, src, display, view, dir)
    }

    /// Processor from a color space to a display / view using a context.
    pub fn get_display_view_processor_with_context(
        &self,
        context: &Context,
        src: &str,
        display: &str,
        view: &str,
        dir: TransformDirection,
    ) -> Result<Processor> {
        let t = DisplayViewTransform::new(src, display, view);
        crate::transforms::Validate::validate(&t)?;
        self.get_processor_with_context(context, &Transform::DisplayView(t), dir)
    }

    /// Processor of a named transform (by name) in a direction.
    pub fn get_processor_named_transform(
        &self,
        name: &str,
        dir: TransformDirection,
    ) -> Result<Processor> {
        self.get_processor_named_transform_with_context(&self.context, name, dir)
    }

    /// Processor of a named transform (by name) using a context.
    pub fn get_processor_named_transform_with_context(
        &self,
        context: &Context,
        name: &str,
        dir: TransformDirection,
    ) -> Result<Processor> {
        let nt = self
            .get_named_transform(name)
            .ok_or_else(|| Error::msg("Named transform: Unspecified TransformDirection."))?
            .clone();
        self.get_processor_for_named_transform_with_context(context, &nt, dir)
    }

    /// Processor of a named transform object in a direction.
    pub fn get_processor_for_named_transform(
        &self,
        nt: &NamedTransform,
        dir: TransformDirection,
    ) -> Result<Processor> {
        self.get_processor_for_named_transform_with_context(&self.context, nt, dir)
    }

    /// Processor of a named transform object using a context.
    pub fn get_processor_for_named_transform_with_context(
        &self,
        context: &Context,
        nt: &NamedTransform,
        dir: TransformDirection,
    ) -> Result<Processor> {
        let t = NamedTransform::get_transform(nt, dir)?;
        self.get_processor_with_context(context, &t, TransformDirection::Forward)
    }

    /// Processor built without using the processor cache.
    pub(crate) fn get_processor_without_caching(
        &self,
        transform: &Transform,
        dir: TransformDirection,
    ) -> Result<Processor> {
        create_processor(self, &self.context, transform, dir)
    }

    /// Processor for a transform, using the given context (uses the
    /// processor cache when enabled).
    pub fn get_processor_with_context(
        &self,
        context: &Context,
        transform: &Transform,
        dir: TransformDirection,
    ) -> Result<Processor> {
        let mut used = Context::new();
        used.set_search_path(&context.search_path());
        used.set_working_dir(context.working_dir());
        let need_context_vars = collect_context_variables(self, context, transform, &mut used)?;

        if !self.processor_cache_enabled() {
            return create_processor(self, context, transform, dir);
        }

        let key = format!(
            "{}{:?}{:?}",
            if need_context_vars {
                used.cache_id()
            } else {
                String::new()
            },
            transform,
            dir
        );
        if let Ok(cache) = self.processor_cache.lock() {
            if let Some(p) = cache.entries.get(&key) {
                return Ok(p.clone());
            }
        }
        let proc = create_processor(self, context, transform, dir)?;
        let mut result = proc;
        if let Ok(mut cache) = self.processor_cache.lock() {
            if !env_present(OCIO_DISABLE_CACHE_FALLBACK) {
                let id = result.cache_id();
                if let Some(p) = cache.entries.values().find(|p| p.cache_id() == id) {
                    result = p.clone();
                }
            }
            cache.entries.insert(key, result.clone());
        }
        Ok(result)
    }

    /// Processor converting a color space of `src_config` to a color space of
    /// `dst_config` using the interchange roles.
    pub fn get_processor_from_configs(
        src_config: &Config,
        src_name: &str,
        dst_config: &Config,
        dst_name: &str,
    ) -> Result<Processor> {
        Config::get_processor_from_configs_with_context(
            src_config.current_context(),
            src_config,
            src_name,
            dst_config.current_context(),
            dst_config,
            dst_name,
        )
    }

    /// Same as [`Config::get_processor_from_configs`] with explicit contexts.
    pub fn get_processor_from_configs_with_context(
        src_context: &Context,
        src_config: &Config,
        src_name: &str,
        dst_context: &Context,
        dst_config: &Config,
        dst_name: &str,
    ) -> Result<Processor> {
        match config_utils::get_interchange_roles_for_color_space_conversion(
            src_config, src_name, dst_config, dst_name,
        )? {
            (Some((src_ex, dst_ex)), _) => {
                Config::get_processor_from_configs_interchange_with_context(
                    src_context,
                    src_config,
                    src_name,
                    &src_ex,
                    dst_context,
                    dst_config,
                    dst_name,
                    &dst_ex,
                )
            }
            (None, ty) => {
                let role = if ty == ReferenceSpaceType::Scene {
                    ROLE_INTERCHANGE_SCENE
                } else {
                    ROLE_INTERCHANGE_DISPLAY
                };
                Err(Error::msg(format!(
                    "The required role '{role}' is missing from the source and/or destination config."
                )))
            }
        }
    }

    /// Processor converting between configs using explicit interchange color
    /// spaces.
    pub fn get_processor_from_configs_interchange(
        src_config: &Config,
        src_name: &str,
        src_interchange: &str,
        dst_config: &Config,
        dst_name: &str,
        dst_interchange: &str,
    ) -> Result<Processor> {
        Config::get_processor_from_configs_interchange_with_context(
            src_config.current_context(),
            src_config,
            src_name,
            src_interchange,
            dst_config.current_context(),
            dst_config,
            dst_name,
            dst_interchange,
        )
    }

    /// Same as [`Config::get_processor_from_configs_interchange`] with
    /// explicit contexts.
    #[allow(clippy::too_many_arguments)]
    pub fn get_processor_from_configs_interchange_with_context(
        src_context: &Context,
        src_config: &Config,
        src_name: &str,
        src_interchange: &str,
        dst_context: &Context,
        dst_config: &Config,
        dst_name: &str,
        dst_interchange: &str,
    ) -> Result<Processor> {
        let src_cs = src_config.get_color_space(src_name).ok_or_else(|| {
            Error::msg(format!("Could not find source color space '{src_name}'."))
        })?;
        let src_ex = src_config.get_color_space(src_interchange).ok_or_else(|| {
            Error::msg(format!(
                "Could not find source interchange color space '{src_interchange}'."
            ))
        })?;
        let dst_cs = dst_config.get_color_space(dst_name).ok_or_else(|| {
            Error::msg(format!(
                "Could not find destination color space '{dst_name}'."
            ))
        })?;
        let dst_ex = dst_config.get_color_space(dst_interchange).ok_or_else(|| {
            Error::msg(format!(
                "Could not find destination interchange color space '{dst_interchange}'."
            ))
        })?;
        let p1 = src_config.get_processor_color_spaces_with_context(src_context, src_cs, src_ex)?;
        let p2 = dst_config.get_processor_color_spaces_with_context(dst_context, dst_ex, dst_cs)?;
        if !src_cs.is_data() && !dst_cs.is_data() {
            Ok(concatenate(&p1, &p2))
        } else {
            Ok(Processor::from_ops(OpVec::new()))
        }
    }

    /// Processor converting a color space of `src_config` to a (display, view)
    /// of `dst_config` using the interchange roles.
    pub fn get_processor_from_configs_display_view(
        src_config: &Config,
        src_name: &str,
        dst_config: &Config,
        dst_display: &str,
        dst_view: &str,
        dir: TransformDirection,
    ) -> Result<Processor> {
        Config::get_processor_from_configs_display_view_with_context(
            src_config.current_context(),
            src_config,
            src_name,
            dst_config.current_context(),
            dst_config,
            dst_display,
            dst_view,
            dir,
        )
    }

    /// Same as [`Config::get_processor_from_configs_display_view`] with
    /// explicit contexts.
    #[allow(clippy::too_many_arguments)]
    pub fn get_processor_from_configs_display_view_with_context(
        src_context: &Context,
        src_config: &Config,
        src_name: &str,
        dst_context: &Context,
        dst_config: &Config,
        dst_display: &str,
        dst_view: &str,
        dir: TransformDirection,
    ) -> Result<Processor> {
        let src_cs = src_config.get_color_space(src_name).ok_or_else(|| {
            Error::msg(format!("Could not find source color space '{src_name}'."))
        })?;
        let scene = src_cs.reference_space_type() == ReferenceSpaceType::Scene;
        let role = if scene {
            ROLE_INTERCHANGE_SCENE
        } else {
            ROLE_INTERCHANGE_DISPLAY
        };
        let src_ex_name = src_config.lookup_role(role).to_string();
        if src_ex_name.is_empty() {
            bail!("The role '{}' is missing in the source config.", role);
        }
        if src_config.get_color_space(&src_ex_name).is_none() {
            bail!(
                "The role '{}' refers to color space '{}' that is missing in the source config.",
                role,
                src_ex_name
            );
        }
        let dst_ex_name = dst_config.lookup_role(role).to_string();
        if dst_ex_name.is_empty() {
            bail!("The role '{}' is missing in the destination config.", role);
        }
        if dst_config.get_color_space(&dst_ex_name).is_none() {
            bail!(
                "The role '{}' refers to color space '{}' that is missing in the destination config.",
                role,
                dst_ex_name
            );
        }
        Config::get_processor_from_configs_display_view_interchange_with_context(
            src_context,
            src_config,
            src_name,
            &src_ex_name,
            dst_context,
            dst_config,
            dst_display,
            dst_view,
            &dst_ex_name,
            dir,
        )
    }

    /// Processor converting a color space to a (display, view) of another
    /// config using explicit interchange color spaces.
    #[allow(clippy::too_many_arguments)]
    pub fn get_processor_from_configs_display_view_interchange(
        src_config: &Config,
        src_name: &str,
        src_interchange: &str,
        dst_config: &Config,
        dst_display: &str,
        dst_view: &str,
        dst_interchange: &str,
        dir: TransformDirection,
    ) -> Result<Processor> {
        Config::get_processor_from_configs_display_view_interchange_with_context(
            src_config.current_context(),
            src_config,
            src_name,
            src_interchange,
            dst_config.current_context(),
            dst_config,
            dst_display,
            dst_view,
            dst_interchange,
            dir,
        )
    }

    /// Same as [`Config::get_processor_from_configs_display_view_interchange`]
    /// with explicit contexts.
    #[allow(clippy::too_many_arguments)]
    pub fn get_processor_from_configs_display_view_interchange_with_context(
        src_context: &Context,
        src_config: &Config,
        src_name: &str,
        src_interchange: &str,
        dst_context: &Context,
        dst_config: &Config,
        dst_display: &str,
        dst_view: &str,
        dst_interchange: &str,
        dir: TransformDirection,
    ) -> Result<Processor> {
        let mut src_cs = src_config.get_color_space(src_name).ok_or_else(|| {
            Error::msg(format!("Could not find source color space '{src_name}'."))
        })?;
        let mut src_ex = src_config.get_color_space(src_interchange).ok_or_else(|| {
            Error::msg(format!(
                "Could not find source interchange color space '{src_interchange}'."
            ))
        })?;
        if dir == TransformDirection::Inverse {
            std::mem::swap(&mut src_cs, &mut src_ex);
        }
        // Note: as in OCIO, the data test uses the (possibly swapped) source.
        let src_is_data = src_cs.is_data();
        let p1 = src_config.get_processor_color_spaces_with_context(src_context, src_cs, src_ex)?;
        let cs_name = dst_config.display_view_color_space_name(dst_display, dst_view);
        let display_cs_name = if View::use_display_name(cs_name) {
            dst_display
        } else {
            cs_name
        };
        let display_cs = dst_config.get_color_space(display_cs_name).ok_or_else(|| {
            Error::msg("Can't create the processor for the destination config: display color space not found.")
        })?;
        let p2 = dst_config.get_display_view_processor_with_context(
            dst_context,
            dst_interchange,
            dst_display,
            dst_view,
            dir,
        )?;
        if !src_is_data && !display_cs.is_data() {
            if dir == TransformDirection::Inverse {
                Ok(concatenate(&p2, &p1))
            } else {
                Ok(concatenate(&p1, &p2))
            }
        } else {
            Ok(Processor::from_ops(OpVec::new()))
        }
    }

    /// Processor from a color space of `src_config` to a color space of the
    /// default builtin config.
    pub fn get_processor_to_builtin_color_space(
        src_config: &Config,
        src_name: &str,
        builtin_name: &str,
    ) -> Result<Processor> {
        config_utils::get_processor_to_builtin_cs(
            src_config,
            src_name,
            builtin_name,
            TransformDirection::Forward,
        )
    }

    /// Processor from a color space of the default builtin config to a color
    /// space of `src_config`.
    pub fn get_processor_from_builtin_color_space(
        builtin_name: &str,
        src_config: &Config,
        src_name: &str,
    ) -> Result<Processor> {
        config_utils::get_processor_to_builtin_cs(
            src_config,
            src_name,
            builtin_name,
            TransformDirection::Inverse,
        )
    }

    /// Name of the interchange spaces to use to convert between a color space
    /// of `src_config` and a color space of `builtin_config`
    /// (`IdentifyInterchangeSpace`).
    pub fn identify_interchange_space(
        src_config: &Config,
        src_name: &str,
        builtin_config: &Config,
        builtin_name: &str,
    ) -> Result<(String, String)> {
        config_utils::identify_interchange_space(src_config, src_name, builtin_config, builtin_name)
    }

    /// Name of the color space of `src_config` equivalent to a color space of
    /// the builtin config (`IdentifyBuiltinColorSpace`).
    pub fn identify_builtin_color_space(
        src_config: &Config,
        builtin_config: &Config,
        builtin_name: &str,
    ) -> Result<String> {
        config_utils::identify_builtin_color_space(src_config, builtin_config, builtin_name)
    }

    /// True if the color space is linear for the given reference space type.
    pub fn is_color_space_linear(
        &self,
        color_space: &str,
        ref_type: ReferenceSpaceType,
    ) -> Result<bool> {
        let cs = self.get_color_space(color_space).ok_or_else(|| {
            Error::msg(format!(
                "Could not test colorspace linearity. Colorspace {color_space} does not exist."
            ))
        })?;
        if cs.is_data() {
            return Ok(false);
        }
        if cs.reference_space_type() != ref_type {
            return Ok(false);
        }
        let enc = cs.encoding();
        if !enc.is_empty() {
            return Ok(
                (compare(enc, "scene-linear") && ref_type == ReferenceSpaceType::Scene)
                    || (compare(enc, "display-linear") && ref_type == ReferenceSpaceType::Display),
            );
        }
        let evaluate = |t: &Transform| -> Result<bool> {
            let img: [[f32; 3]; 8] = [
                [0.0625, 0.0625, 0.0625],
                [4.0, 4.0, 4.0],
                [0.0625, 0.0, 0.0],
                [4.0, 0.0, 0.0],
                [0.0, 0.0625, 0.0],
                [0.0, 4.0, 0.0],
                [0.0, 0.0, 0.0625],
                [0.0, 0.0, 4.0],
            ];
            let proc = self.get_processor_without_caching(t, TransformDirection::Forward)?;
            let cpu = proc.optimized_cpu_processor(OptimizationFlags::NONE);
            let mut pixels: Vec<[f32; 4]> = img.iter().map(|p| [p[0], p[1], p[2], 1.0]).collect();
            cpu.apply_pixels(&mut pixels);
            let abs_error = 1e-5f32;
            let mult = 64.0f32;
            let mut ret = true;
            for pair in pixels.as_chunks::<2>().0 {
                for c in 0..3 {
                    ret &= ((pair[0][c] * mult) - pair[1][c]).abs() <= abs_error;
                }
            }
            Ok(ret)
        };
        if let Some(t) = cs.transform(ColorSpaceDirection::ToReference) {
            return evaluate(t);
        }
        if let Some(t) = cs.transform(ColorSpaceDirection::FromReference) {
            return evaluate(t);
        }
        Ok(true)
    }
}

/// Concatenate the ops of two processors.
pub(crate) fn concatenate(p1: &Processor, p2: &Processor) -> Processor {
    let mut ops: OpVec = p1.ops().to_vec();
    ops.extend(p2.ops().iter().cloned());
    Processor::from_ops(ops)
}
