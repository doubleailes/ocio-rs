//! Op building for the config-dependent transforms (port of
//! `ColorSpaceTransform.cpp`, `DisplayViewTransform.cpp` and
//! `LookTransform.cpp`).

use super::look_parse::{serialize_tokens, LookParseResult, LookTokens};
use super::named_transform::get_named_transforms_transform;
use super::utils::compare;
use super::{ColorSpace, Config, NamedTransform, View, ViewTransform};
use crate::context::Context;
use crate::error::{Error, Result};
use crate::ops::noop::create_look_no_op;
use crate::ops::{Op, OpVec, Pixel};
use crate::transforms::build::build_ops;
use crate::transforms::{BuildOps, ColorSpaceTransform, DisplayViewTransform, LookTransform, Validate};
use crate::types::{
    Allocation, ColorSpaceDirection, ReferenceSpaceType, TransformDirection, ViewTransformDirection,
};
use std::any::Any;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// AllocationNoOp

/// No-op recording the GPU allocation of a color space (port of the
/// `AllocationNoOp` of `NoOps.cpp`). It is removed by the optimizer.
#[derive(Debug, Clone, PartialEq)]
pub struct AllocationNoOp {
    /// The allocation.
    pub allocation: Allocation,
    /// The allocation variables.
    pub vars: Vec<f32>,
}

impl Op for AllocationNoOp {
    fn name(&self) -> &'static str {
        "AllocationNoOp"
    }
    fn apply(&self, _pixels: &mut [Pixel]) {}
    fn is_no_op(&self) -> bool {
        true
    }
    fn has_channel_crosstalk(&self) -> bool {
        false
    }
    fn cache_id(&self) -> String {
        // Port of AllocationData::getCacheID().
        let mut s = format!("{} ", self.allocation.as_str());
        for v in &self.vars {
            s.push_str(&super::utils::format_g(f64::from(*v), 7));
            s.push(' ');
        }
        s
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn clone_box(&self) -> Box<dyn Op> {
        Box::new(self.clone())
    }
}

/// Append an allocation no-op (port of `CreateGpuAllocationNoOp`).
fn create_gpu_allocation_no_op(ops: &mut OpVec, cs: &ColorSpace) {
    ops.push(Arc::new(AllocationNoOp {
        allocation: cs.allocation(),
        vars: cs.allocation_vars().to_vec(),
    }));
}

// ---------------------------------------------------------------------------
// ColorSpaceTransform

impl Validate for ColorSpaceTransform {
    fn validate(&self) -> Result<()> {
        if self.src.is_empty() {
            return Err(Error::msg("ColorSpaceTransform: empty source color space name."));
        }
        if self.dst.is_empty() {
            return Err(Error::msg("ColorSpaceTransform: empty destination color space name."));
        }
        Ok(())
    }
}

impl BuildOps for ColorSpaceTransform {
    fn build_ops(&self, ops: &mut OpVec, config: &Config, context: &Context, dir: TransformDirection) -> Result<()> {
        let forward = dir.combine(self.direction) == TransformDirection::Forward;
        let (src_name, dst_name) = if forward { (&self.src, &self.dst) } else { (&self.dst, &self.src) };
        let src_resolved = context.resolve_string_var(src_name);
        let dst_resolved = context.resolve_string_var(dst_name);
        let src = config.get_color_space(&src_resolved);
        let dst = config.get_color_space(&dst_resolved);
        let mut src_nt = None;
        let mut dst_nt = None;
        if src.is_none() {
            src_nt = config.get_named_transform(&src_resolved);
            if src_nt.is_none() {
                return Err(missing_cs(src_name));
            }
        }
        if dst.is_none() {
            dst_nt = config.get_named_transform(&dst_resolved);
            if dst_nt.is_none() {
                return Err(missing_cs(dst_name));
            }
        }
        if src_nt.is_some() || dst_nt.is_some() {
            let t = get_named_transforms_transform(src_nt, dst_nt)?;
            return build_ops(ops, config, context, &t, TransformDirection::Forward);
        }
        match (src, dst) {
            (Some(s), Some(d)) => build_color_space_ops(ops, config, context, s, d, self.data_bypass),
            _ => Ok(()),
        }
    }
}

fn missing_cs(name: &str) -> Error {
    Error::msg(format!("Color space '{name}' could not be found."))
}

fn same_equality_group(a: &ColorSpace, b: &ColorSpace) -> bool {
    if compare(a.name(), b.name()) {
        return true;
    }
    let ga = a.equality_group();
    !ga.is_empty() && ga == b.equality_group()
}

/// Helper building the ops between two color spaces.
pub struct BuildColorSpaceOps;

impl BuildColorSpaceOps {
    /// Build the ops converting `src` to `dst` (`BuildColorSpaceOps`).
    pub fn build(
        ops: &mut OpVec,
        config: &Config,
        context: &Context,
        src: &ColorSpace,
        dst: &ColorSpace,
        data_bypass: bool,
    ) -> Result<()> {
        build_color_space_ops(ops, config, context, src, dst, data_bypass)
    }

    /// Build the ops converting `src` to its reference space
    /// (`BuildColorSpaceToReferenceOps`).
    pub fn to_reference(
        ops: &mut OpVec,
        config: &Config,
        context: &Context,
        src: &ColorSpace,
        data_bypass: bool,
    ) -> Result<()> {
        build_color_space_to_reference_ops(ops, config, context, src, data_bypass)
    }

    /// Build the ops converting the reference space to `dst`
    /// (`BuildColorSpaceFromReferenceOps`).
    pub fn from_reference(
        ops: &mut OpVec,
        config: &Config,
        context: &Context,
        dst: &ColorSpace,
        data_bypass: bool,
    ) -> Result<()> {
        build_color_space_from_reference_ops(ops, config, context, dst, data_bypass)
    }

    /// Build the ops converting between the two reference spaces
    /// (`BuildReferenceConversionOps`).
    pub fn reference_conversion(
        ops: &mut OpVec,
        config: &Config,
        context: &Context,
        src_ref: ReferenceSpaceType,
        dst_ref: ReferenceSpaceType,
    ) -> Result<()> {
        build_reference_conversion_ops(ops, config, context, src_ref, dst_ref)
    }
}

pub(crate) fn build_color_space_ops(
    ops: &mut OpVec,
    config: &Config,
    context: &Context,
    src: &ColorSpace,
    dst: &ColorSpace,
    data_bypass: bool,
) -> Result<()> {
    if same_equality_group(src, dst) {
        return Ok(());
    }
    if data_bypass && (dst.is_data() || src.is_data()) {
        return Ok(());
    }
    build_color_space_to_reference_ops(ops, config, context, src, data_bypass)?;
    build_reference_conversion_ops(ops, config, context, src.reference_space_type(), dst.reference_space_type())?;
    build_color_space_from_reference_ops(ops, config, context, dst, data_bypass)
}

pub(crate) fn build_color_space_to_reference_ops(
    ops: &mut OpVec,
    config: &Config,
    context: &Context,
    src: &ColorSpace,
    data_bypass: bool,
) -> Result<()> {
    if data_bypass && src.is_data() {
        return Ok(());
    }
    create_gpu_allocation_no_op(ops, src);
    if let Some(t) = src.transform(ColorSpaceDirection::ToReference) {
        build_ops(ops, config, context, t, TransformDirection::Forward)?;
    } else if let Some(t) = src.transform(ColorSpaceDirection::FromReference) {
        build_ops(ops, config, context, t, TransformDirection::Inverse)?;
    }
    Ok(())
}

pub(crate) fn build_color_space_from_reference_ops(
    ops: &mut OpVec,
    config: &Config,
    context: &Context,
    dst: &ColorSpace,
    data_bypass: bool,
) -> Result<()> {
    if data_bypass && dst.is_data() {
        return Ok(());
    }
    if let Some(t) = dst.transform(ColorSpaceDirection::FromReference) {
        build_ops(ops, config, context, t, TransformDirection::Forward)?;
    } else if let Some(t) = dst.transform(ColorSpaceDirection::ToReference) {
        build_ops(ops, config, context, t, TransformDirection::Inverse)?;
    }
    create_gpu_allocation_no_op(ops, dst);
    Ok(())
}

/// Convert between the scene-referred and the display-referred reference
/// spaces using the default view transform.
pub(crate) fn build_reference_conversion_ops(
    ops: &mut OpVec,
    config: &Config,
    context: &Context,
    src_ref: ReferenceSpaceType,
    dst_ref: ReferenceSpaceType,
) -> Result<()> {
    if src_ref == dst_ref {
        return Ok(());
    }
    let view = config.default_scene_to_display_view_transform().ok_or_else(|| {
        Error::msg("There is no view transform between the main scene-referred space and the display-referred space.")
    })?;
    let (primary, secondary) = if src_ref == ReferenceSpaceType::Scene {
        (ViewTransformDirection::FromReference, ViewTransformDirection::ToReference)
    } else {
        (ViewTransformDirection::ToReference, ViewTransformDirection::FromReference)
    };
    if let Some(t) = view.transform(primary) {
        build_ops(ops, config, context, t, TransformDirection::Forward)?;
    } else if let Some(t) = view.transform(secondary) {
        build_ops(ops, config, context, t, TransformDirection::Inverse)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// LookTransform

impl Validate for LookTransform {
    fn validate(&self) -> Result<()> {
        if self.src.is_empty() {
            return Err(Error::msg("LookTransform: empty source color space name."));
        }
        if self.dst.is_empty() {
            return Err(Error::msg("LookTransform: empty destination color space name."));
        }
        Ok(())
    }
}

fn run_look_tokens<'a>(
    ops: &mut OpVec,
    current: &mut Option<&'a ColorSpace>,
    skip_cs_conversion: bool,
    config: &'a Config,
    context: &Context,
    tokens: &LookTokens,
) -> Result<()> {
    for token in tokens {
        let name = &token.name;
        if name.is_empty() {
            continue;
        }
        let look = config.look(name).ok_or_else(|| {
            let mut s = format!("RunLookTokens error. The specified look, '{name}', cannot be found. ");
            if config.num_looks() == 0 {
                s.push_str(" (No looks defined in config).");
            } else {
                let names: Vec<&str> = (0..config.num_looks()).map(|i| config.look_name_by_index(i)).collect();
                s.push_str(&format!(" (looks: {}).", names.join(", ")));
            }
            Error::msg(s)
        })?;
        let mut tmp = OpVec::new();
        match token.dir {
            TransformDirection::Forward => {
                create_look_no_op(&mut tmp, name);
                if let Some(t) = look.transform() {
                    build_ops(&mut tmp, config, context, t, TransformDirection::Forward)?;
                } else if let Some(t) = look.inverse_transform() {
                    build_ops(&mut tmp, config, context, t, TransformDirection::Inverse)?;
                }
            }
            TransformDirection::Inverse => {
                create_look_no_op(&mut tmp, &format!("-{name}"));
                if let Some(t) = look.inverse_transform() {
                    build_ops(&mut tmp, config, context, t, TransformDirection::Forward)?;
                } else if let Some(t) = look.transform() {
                    build_ops(&mut tmp, config, context, t, TransformDirection::Inverse)?;
                }
            }
        }
        let process = config.get_color_space(look.process_space()).ok_or_else(|| {
            Error::msg(format!(
                "RunLookTokens error. The specified look, '{}', requires processing in the ColorSpace, '{}' which is not defined.",
                token.name,
                look.process_space()
            ))
        })?;
        let cur = *current.get_or_insert(process);
        if !skip_cs_conversion && cur.name() != process.name() {
            build_color_space_ops(ops, config, context, cur, process, true)?;
            *current = Some(process);
        }
        ops.extend(tmp);
    }
    Ok(())
}

/// Build the ops of looks, updating `current` to the process space of the
/// last applied look (`BuildLookOps`).
pub(crate) fn build_look_ops_from_result<'a>(
    ops: &mut OpVec,
    current: &mut Option<&'a ColorSpace>,
    skip_cs_conversion: bool,
    config: &'a Config,
    context: &Context,
    looks: &LookParseResult,
) -> Result<()> {
    let options = looks.options();
    if options.is_empty() {
        return Ok(());
    }
    if options.len() == 1 {
        return run_look_tokens(ops, current, skip_cs_conversion, config, context, &options[0]);
    }
    let mut msg = String::new();
    for (i, option) in options.iter().enumerate() {
        let mut cs = *current;
        let mut tmp = OpVec::new();
        match run_look_tokens(&mut tmp, &mut cs, skip_cs_conversion, config, context, option) {
            Ok(()) => {
                *current = cs;
                ops.extend(tmp);
                return Ok(());
            }
            Err(e @ Error::MissingFile(_)) => {
                if i != 0 {
                    msg.push_str("  ...  ");
                }
                msg.push_str(&format!("({}) {}", serialize_tokens(option), e.message()));
            }
            Err(e) => return Err(e),
        }
    }
    Err(Error::missing_file(msg))
}

impl BuildOps for LookTransform {
    fn build_ops(&self, ops: &mut OpVec, config: &Config, context: &Context, dir: TransformDirection) -> Result<()> {
        let mut src = config.get_color_space(&self.src).ok_or_else(|| {
            Error::msg(format!(
                "BuildLookOps error.The specified lookTransform specifies a src colorspace, '{}', which is not defined.",
                self.src
            ))
        })?;
        let mut dst = config.get_color_space(&self.dst).ok_or_else(|| {
            Error::msg(format!(
                "BuildLookOps error.The specified lookTransform specifies a dst colorspace, '{}', which is not defined.",
                self.dst
            ))
        })?;
        let mut looks = LookParseResult::new();
        looks.parse(&self.looks);
        if dir.combine(self.direction) == TransformDirection::Inverse {
            std::mem::swap(&mut src, &mut dst);
            looks.reverse();
        }
        let skip = self.skip_color_space_conversion;
        let mut current = Some(src);
        build_look_ops_from_result(ops, &mut current, skip, config, context, &looks)?;
        if let Some(cur) = current {
            if !skip && cur.name() != dst.name() {
                build_color_space_ops(ops, config, context, cur, dst, true)?;
            }
        }
        Ok(())
    }
}

/// Name of the color space resulting from applying the looks (the process
/// space of the last applied look), `""` if none.
pub(crate) fn looks_result_color_space(config: &Config, context: &Context, looks: &LookParseResult) -> Result<String> {
    if looks.is_empty() {
        return Ok(String::new());
    }
    let mut current: Option<&ColorSpace> = None;
    let mut tmp = OpVec::new();
    build_look_ops_from_result(&mut tmp, &mut current, false, config, context, looks)?;
    Ok(current.map(|c| c.name().to_string()).unwrap_or_default())
}

/// `LookTransform::GetLooksResultColorSpace`: the color space resulting from
/// applying the looks of `looks_str`.
pub fn get_looks_result_color_space(config: &Config, context: &Context, looks_str: &str) -> Result<String> {
    if looks_str.is_empty() {
        return Ok(String::new());
    }
    let mut looks = LookParseResult::new();
    looks.parse(looks_str);
    looks_result_color_space(config, context, &looks)
}

// ---------------------------------------------------------------------------
// DisplayViewTransform

impl Validate for DisplayViewTransform {
    fn validate(&self) -> Result<()> {
        if self.src.is_empty() {
            return Err(Error::msg("DisplayViewTransform: empty source color space name."));
        }
        if self.display.is_empty() {
            return Err(Error::msg("DisplayViewTransform: empty display name."));
        }
        if self.view.is_empty() {
            return Err(Error::msg("DisplayViewTransform: empty view name."));
        }
        Ok(())
    }
}

fn view_transform_error(vt: &ViewTransform) -> Error {
    Error::msg(format!(
        "View transform named '{}' needs either a transform from or to reference.",
        vt.name()
    ))
}

fn build_source_to_display(
    ops: &mut OpVec,
    config: &Config,
    context: &Context,
    source: &ColorSpace,
    vt: &ViewTransform,
    display_cs: &ColorSpace,
    data_bypass: bool,
) -> Result<()> {
    build_color_space_to_reference_ops(ops, config, context, source, data_bypass)?;
    build_reference_conversion_ops(ops, config, context, source.reference_space_type(), vt.reference_space_type())?;
    if let Some(t) = vt.transform(ViewTransformDirection::FromReference) {
        build_ops(ops, config, context, t, TransformDirection::Forward)?;
    } else if let Some(t) = vt.transform(ViewTransformDirection::ToReference) {
        build_ops(ops, config, context, t, TransformDirection::Inverse)?;
    } else {
        return Err(view_transform_error(vt));
    }
    build_color_space_from_reference_ops(ops, config, context, display_cs, data_bypass)
}

fn build_display_to_source(
    ops: &mut OpVec,
    config: &Config,
    context: &Context,
    display_cs: &ColorSpace,
    vt: &ViewTransform,
    source: &ColorSpace,
    data_bypass: bool,
) -> Result<()> {
    build_color_space_to_reference_ops(ops, config, context, display_cs, data_bypass)?;
    if let Some(t) = vt.transform(ViewTransformDirection::ToReference) {
        build_ops(ops, config, context, t, TransformDirection::Forward)?;
    } else if let Some(t) = vt.transform(ViewTransformDirection::FromReference) {
        build_ops(ops, config, context, t, TransformDirection::Inverse)?;
    } else {
        return Err(view_transform_error(vt));
    }
    build_reference_conversion_ops(ops, config, context, vt.reference_space_type(), source.reference_space_type())?;
    build_color_space_from_reference_ops(ops, config, context, source, data_bypass)
}

impl BuildOps for DisplayViewTransform {
    fn build_ops(&self, ops: &mut OpVec, config: &Config, context: &Context, dir: TransformDirection) -> Result<()> {
        let src_name = &self.src;
        let src_cs = config.get_color_space(src_name).ok_or_else(|| {
            if src_name.is_empty() {
                Error::msg("DisplayViewTransform error. The source color space is unspecified.")
            } else {
                Error::msg(format!(
                    "DisplayViewTransform error. Cannot find source color space named '{src_name}'."
                ))
            }
        })?;
        let display = &self.display;
        if config.num_views(display) == 0 {
            return Err(Error::msg(format!("DisplayViewTransform error. Display '{display}' not found.")));
        }
        let view = &self.view;
        let vt_name = config.display_view_transform_name(display, view);
        let mut view_transform: Option<&ViewTransform> = None;
        let mut view_nt: Option<&NamedTransform> = None;
        if !vt_name.is_empty() {
            view_transform = config.view_transform(vt_name);
            if view_transform.is_none() {
                view_nt = config.get_named_transform(vt_name);
                if view_nt.is_none() {
                    return Err(Error::msg(format!(
                        "DisplayViewTransform error. The view transform '{vt_name}' is neither a view transform nor a named transform."
                    )));
                }
            }
        }
        let cs_name = config.display_view_color_space_name(display, view);
        let display_cs_name: &str = if View::use_display_name(cs_name) { display } else { cs_name };
        let display_cs = config.get_color_space(display_cs_name);
        let mut cs_nt: Option<&NamedTransform> = None;
        if display_cs.is_none() {
            if display_cs_name.is_empty() {
                return Err(Error::msg(format!(
                    "DisplayViewTransform error. The display '{display}' does not have view '{view}'."
                )));
            }
            if view_transform.is_some() || view_nt.is_some() {
                return Err(Error::msg(format!(
                    "DisplayViewTransform error. The view '{view}' refers to a display color space '{display_cs_name}' that can't be found."
                )));
            }
            cs_nt = config.get_named_transform(display_cs_name);
            if cs_nt.is_none() {
                return Err(Error::msg(format!(
                    "DisplayViewTransform error. Cannot find color space or named transform with name '{display_cs_name}'."
                )));
            }
        }

        let data_bypass = self.data_bypass;
        let display_data = display_cs.map(|c| c.is_data()).unwrap_or(false);
        if data_bypass && (src_cs.is_data() || display_data) {
            return Ok(());
        }

        let mut looks = LookParseResult::new();
        if !self.looks_bypass {
            looks.parse(config.display_view_looks(display, view));
        }

        match dir.combine(self.direction) {
            TransformDirection::Forward => {
                let mut current = Some(src_cs);
                if !looks.is_empty() {
                    build_look_ops_from_result(ops, &mut current, false, config, context, &looks)?;
                }
                let current = current.unwrap_or(src_cs);
                if let Some(nt) = cs_nt {
                    let t = NamedTransform::get_transform(nt, TransformDirection::Forward)?;
                    build_ops(ops, config, context, &t, TransformDirection::Forward)?;
                } else if let Some(nt) = view_nt {
                    let t = NamedTransform::get_transform(nt, TransformDirection::Forward)?;
                    build_ops(ops, config, context, &t, TransformDirection::Forward)?;
                    if let Some(dcs) = display_cs {
                        build_color_space_from_reference_ops(ops, config, context, dcs, data_bypass)?;
                    }
                } else if let Some(vt) = view_transform {
                    if let Some(dcs) = display_cs {
                        build_source_to_display(ops, config, context, current, vt, dcs, data_bypass)?;
                    }
                } else if let Some(dcs) = display_cs {
                    build_color_space_ops(ops, config, context, current, dcs, data_bypass)?;
                }
            }
            TransformDirection::Inverse => {
                let mut vt_source: Option<&ColorSpace> = Some(src_cs);
                if !looks.is_empty() {
                    let res = looks_result_color_space(config, context, &looks)?;
                    vt_source = config.get_color_space(&res);
                }
                if let Some(nt) = cs_nt {
                    let t = NamedTransform::get_transform(nt, TransformDirection::Inverse)?;
                    build_ops(ops, config, context, &t, TransformDirection::Forward)?;
                } else if let Some(nt) = view_nt {
                    if let Some(dcs) = display_cs {
                        build_color_space_to_reference_ops(ops, config, context, dcs, data_bypass)?;
                    }
                    let t = NamedTransform::get_transform(nt, TransformDirection::Inverse)?;
                    build_ops(ops, config, context, &t, TransformDirection::Forward)?;
                } else if let Some(vt) = view_transform {
                    if let (Some(dcs), Some(src)) = (display_cs, vt_source) {
                        build_display_to_source(ops, config, context, dcs, vt, src, data_bypass)?;
                    } else {
                        return Err(Error::msg("BuildColorSpaceOps failed, null colorSpace."));
                    }
                } else if let (Some(dcs), Some(src)) = (display_cs, vt_source) {
                    build_color_space_ops(ops, config, context, dcs, src, data_bypass)?;
                } else {
                    return Err(Error::msg("BuildColorSpaceOps failed, null dstColorSpace."));
                }
                if !looks.is_empty() {
                    looks.reverse();
                    let mut current = vt_source;
                    build_look_ops_from_result(ops, &mut current, false, config, context, &looks)?;
                    match current {
                        Some(cur) => build_color_space_ops(ops, config, context, cur, src_cs, data_bypass)?,
                        None => return Err(Error::msg("BuildColorSpaceOps failed, null srcColorSpace.")),
                    }
                }
            }
        }
        Ok(())
    }
}
