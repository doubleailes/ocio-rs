//! Collection of the context variables used by a transform (port of
//! `CollectContextVariables` from `ContextVariableUtils.cpp` and of the
//! transform specific versions), used to build processor cache keys.

use super::look_parse::LookParseResult;
use super::{ColorSpace, Config, Look, NamedTransform};
use crate::context::Context;
use crate::error::{Error, Result};
use crate::transforms::{ColorSpaceTransform, DisplayViewTransform, FileTransform, LookTransform, Transform};
use crate::types::{ColorSpaceDirection, TransformDirection, ViewTransformDirection};
use std::cell::Cell;

thread_local! {
    static DEPTH: Cell<u32> = const { Cell::new(0) };
}

struct DepthGuard;

impl DepthGuard {
    fn enter() -> Result<Self> {
        let d = DEPTH.with(|d| d.get());
        if d > 32 {
            return Err(Error::msg("Cycle detected while collecting context variables."));
        }
        DEPTH.with(|c| c.set(d + 1));
        Ok(DepthGuard)
    }
}

impl Drop for DepthGuard {
    fn drop(&mut self) {
        DEPTH.with(|c| c.set(c.get().saturating_sub(1)));
    }
}

/// Collect in `used` the context variables needed by `transform`. Returns
/// true if some are used.
pub fn collect_context_variables(
    config: &Config,
    context: &Context,
    transform: &Transform,
    used: &mut Context,
) -> Result<bool> {
    let _guard = DepthGuard::enter()?;
    match transform {
        Transform::ColorSpace(t) => collect_color_space_transform(config, context, t, used),
        Transform::DisplayView(t) => collect_display_view(config, context, t, used),
        Transform::File(t) => Ok(collect_file(context, t, used)),
        Transform::Group(g) => {
            let mut found = false;
            for c in &g.transforms {
                if collect_context_variables(config, context, c, used)? {
                    found = true;
                }
            }
            Ok(found)
        }
        Transform::Look(t) => collect_look_transform(config, context, t, used),
        _ => Ok(false),
    }
}

fn collect_opt(config: &Config, context: &Context, t: Option<&Transform>, used: &mut Context) -> Result<bool> {
    match t {
        Some(t) => collect_context_variables(config, context, t, used),
        None => Ok(false),
    }
}

fn collect_color_space(config: &Config, context: &Context, cs: Option<&ColorSpace>, used: &mut Context) -> Result<bool> {
    let mut found = false;
    if let Some(cs) = cs {
        if collect_opt(config, context, cs.transform(ColorSpaceDirection::ToReference), used)? {
            found = true;
        }
        if collect_opt(config, context, cs.transform(ColorSpaceDirection::FromReference), used)? {
            found = true;
        }
    }
    Ok(found)
}

fn collect_named_transform(
    config: &Config,
    context: &Context,
    nt: Option<&NamedTransform>,
    used: &mut Context,
) -> Result<bool> {
    let mut found = false;
    if let Some(nt) = nt {
        if collect_opt(config, context, nt.transform(TransformDirection::Forward), used)? {
            found = true;
        }
        if collect_opt(config, context, nt.transform(TransformDirection::Inverse), used)? {
            found = true;
        }
    }
    Ok(found)
}

fn collect_color_space_transform(
    config: &Config,
    context: &Context,
    t: &ColorSpaceTransform,
    used: &mut Context,
) -> Result<bool> {
    let mut found = false;
    let src = context.resolve_string_var_with_used(&t.src, used);
    if src != t.src {
        found = true;
    }
    let dst = context.resolve_string_var_with_used(&t.dst, used);
    if dst != t.dst {
        found = true;
    }
    for name in [&src, &dst] {
        match config.get_color_space(name) {
            Some(cs) => {
                if collect_color_space(config, context, Some(cs), used)? {
                    found = true;
                }
            }
            None => {
                if collect_named_transform(config, context, config.get_named_transform(name), used)? {
                    found = true;
                }
            }
        }
    }
    Ok(found)
}

/// Context variables used by a look in a direction.
pub(crate) fn collect_look(
    config: &Config,
    context: &Context,
    dir: TransformDirection,
    look: &Look,
    used: &mut Context,
) -> Result<bool> {
    let mut found = false;
    let (first, second) = match dir {
        TransformDirection::Forward => (look.transform(), look.inverse_transform()),
        TransformDirection::Inverse => (look.inverse_transform(), look.transform()),
    };
    if first.is_some() {
        if collect_opt(config, context, first, used)? {
            found = true;
        }
    } else if collect_opt(config, context, second, used)? {
        found = true;
    }
    if let Some(cs) = config.get_color_space(look.process_space()) {
        if collect_color_space(config, context, Some(cs), used)? {
            found = true;
        }
    }
    Ok(found)
}

fn collect_looks_str(config: &Config, context: &Context, looks: &str, used: &mut Context) -> Result<bool> {
    let mut found = false;
    let mut parse = LookParseResult::new();
    parse.parse(looks);
    for tokens in parse.options() {
        for token in tokens {
            if let Some(look) = config.look(&token.name) {
                if collect_look(config, context, token.dir, look, used)? {
                    found = true;
                }
            }
        }
    }
    Ok(found)
}

fn collect_display_view(config: &Config, context: &Context, t: &DisplayViewTransform, used: &mut Context) -> Result<bool> {
    let mut found = false;
    if collect_color_space(config, context, config.get_color_space(&t.src), used)? {
        found = true;
    }
    let cs_name = config.display_view_color_space_name(&t.display, &t.view);
    if !cs_name.is_empty() && collect_color_space(config, context, config.get_color_space(cs_name), used)? {
        found = true;
    }
    let vt_name = config.display_view_transform_name(&t.display, &t.view);
    if !vt_name.is_empty() {
        if let Some(vt) = config.view_transform(vt_name) {
            if collect_opt(config, context, vt.transform(ViewTransformDirection::ToReference), used)? {
                found = true;
            }
            if collect_opt(config, context, vt.transform(ViewTransformDirection::FromReference), used)? {
                found = true;
            }
        }
    }
    if !t.looks_bypass {
        let looks = config.display_view_looks(&t.display, &t.view).to_string();
        if collect_looks_str(config, context, &looks, used)? {
            found = true;
        }
    }
    Ok(found)
}

fn collect_look_transform(config: &Config, context: &Context, t: &LookTransform, used: &mut Context) -> Result<bool> {
    let mut found = false;
    if collect_color_space(config, context, config.get_color_space(&t.src), used)? {
        found = true;
    }
    if collect_color_space(config, context, config.get_color_space(&t.dst), used)? {
        found = true;
    }
    if !t.looks.is_empty() && collect_looks_str(config, context, &t.looks, used)? {
        found = true;
    }
    Ok(found)
}

fn new_context_like(context: &Context) -> Context {
    let mut c = Context::new();
    c.set_search_path(&context.search_path());
    c.set_working_dir(context.working_dir());
    c
}

fn collect_file(context: &Context, t: &FileTransform, used: &mut Context) -> bool {
    let src = &t.src;
    if src.is_empty() {
        return false;
    }
    let mut found = false;
    let mut ctx_filename = new_context_like(context);
    let resolved = context.resolve_string_var_with_used(src, &mut ctx_filename);
    if resolved != *src {
        found = true;
        used.add_string_vars(&ctx_filename);
    }
    let empty = new_context_like(context);
    let mut ctx_filepath = new_context_like(context);
    match context.resolve_file_location_with_used(&resolved, &mut ctx_filepath) {
        Ok(path) => {
            let same = empty.resolve_file_location(&resolved).map(|p| p == path).unwrap_or(false);
            if !same {
                found = true;
                used.add_string_vars(&ctx_filepath);
            }
        }
        Err(_) => {
            found = true;
            used.add_string_vars(&ctx_filepath);
        }
    }
    let mut ctx_cccid = Context::new();
    let resolved_cccid = context.resolve_string_var_with_used(&t.ccc_id, &mut ctx_cccid);
    if resolved_cccid != t.ccc_id {
        found = true;
        used.add_string_vars(&ctx_cccid);
    }
    found
}
