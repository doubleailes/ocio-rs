//! Builtin transforms (ACES, camera log curves, display curves) and builtin
//! configs (port of `transforms/builtins/*`, `transforms/BuiltinTransform.cpp`
//! and `builtinconfigs/*`).
//!
//! OCIO creates the ops of a builtin directly; here every builtin is
//! expressed as a list of plain transforms (matrices, log curves, fixed
//! functions, LUTs computed exactly as OCIO computes them, ...) that is built
//! into ops with [`crate::transforms::build::build_ops`].

pub mod aces;
pub mod cameras;
pub mod color_matrix_helpers;
pub mod configs;
pub mod displays;
pub mod op_helpers;
pub mod registry;

pub use registry::{
    builtin_transform_description, builtin_transform_description_by_index, builtin_transform_style,
    builtin_transform_styles, builtin_transforms, num_builtin_transforms, BuiltinCreator,
    BuiltinTransformRegistry,
};

use crate::config::Config;
use crate::context::Context;
use crate::error::{Error, Result};
use crate::ops::OpVec;
use crate::transforms::build::build_ops;
use crate::transforms::{BuildOps, BuiltinTransform, GroupTransform, Transform, Validate};
use crate::types::TransformDirection;

/// Style of a default constructed builtin transform (and of an empty style).
pub const DEFAULT_BUILTIN_STYLE: &str = "IDENTITY";

fn invalid_style(style: &str) -> Error {
    Error::msg(format!(
        "BuiltinTransform: invalid built-in transform style '{style}'."
    ))
}

impl BuiltinTransform {
    /// Index of the style in the builtin registry (an empty style means
    /// `IDENTITY`, the style of a default OCIO `BuiltinTransform`).
    pub fn transform_index(&self) -> Result<usize> {
        let style = if self.style.is_empty() {
            DEFAULT_BUILTIN_STYLE
        } else {
            self.style.as_str()
        };
        BuiltinTransformRegistry::get()
            .index_of(style)
            .ok_or_else(|| invalid_style(style))
    }

    /// Set the style (case insensitive). The style is stored with the
    /// registry spelling, as OCIO's `getStyle` returns it.
    pub fn set_style(&mut self, style: &str) -> Result<()> {
        let reg = BuiltinTransformRegistry::get();
        let index = reg.index_of(style).ok_or_else(|| invalid_style(style))?;
        self.style = reg.builtin_style(index)?.to_string();
        Ok(())
    }

    /// The style with the registry spelling (`IDENTITY` for an empty style).
    pub fn canonical_style(&self) -> Result<&'static str> {
        BuiltinTransformRegistry::get().builtin_style(self.transform_index()?)
    }

    /// Description of the builtin.
    pub fn description(&self) -> Result<&'static str> {
        BuiltinTransformRegistry::get().builtin_description(self.transform_index()?)
    }

    /// The transforms implementing the builtin, in the transform direction.
    pub fn to_group_transform(&self) -> Result<GroupTransform> {
        let mut transforms = Vec::new();
        BuiltinTransformRegistry::get()
            .create_transforms(self.transform_index()?, &mut transforms)?;
        let mut group = GroupTransform::from_transforms(transforms);
        group.direction = self.direction;
        Ok(group)
    }
}

impl Validate for BuiltinTransform {
    fn validate(&self) -> Result<()> {
        self.transform_index().map(|_| ())
    }
}

impl BuildOps for BuiltinTransform {
    fn build_ops(
        &self,
        ops: &mut OpVec,
        config: &Config,
        context: &Context,
        dir: TransformDirection,
    ) -> Result<()> {
        let combined_dir = self.direction.combine(dir);

        let mut transforms = Vec::new();
        BuiltinTransformRegistry::get()
            .create_transforms(self.transform_index()?, &mut transforms)?;

        // Building the group in the inverse direction reverses the list and
        // inverts every transform, as OCIO's `OpRcPtrVec::invert`.
        let group = Transform::Group(GroupTransform::from_transforms(transforms));
        build_ops(ops, config, context, &group, combined_dir)
    }
}

#[cfg(test)]
mod tests;
