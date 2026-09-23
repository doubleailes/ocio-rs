//! Conversion of transforms into ops.
//!
//! Every transform type implements [`BuildOps`] and [`Validate`]. The
//! implementations live next to the ops they create (`crate::ops::*`) or,
//! for config-dependent transforms, in `crate::config`.

use crate::config::Config;
use crate::context::Context;
use crate::error::{Error, Result};
use crate::ops::OpVec;
use crate::transforms::Transform;
use crate::types::TransformDirection;
use std::cell::Cell;

/// Conversion of a transform into ops.
pub trait BuildOps {
    /// Append the ops implementing `self` to `ops`. `dir` is the direction
    /// requested by the caller; implementations must combine it with the
    /// transform's own direction (`self.direction.combine(dir)`).
    fn build_ops(
        &self,
        ops: &mut OpVec,
        config: &Config,
        context: &Context,
        dir: TransformDirection,
    ) -> Result<()>;
}

/// Parameter validation of a transform.
pub trait Validate {
    /// Return an error if the parameters are invalid.
    fn validate(&self) -> Result<()>;
}

thread_local! {
    static BUILD_DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// Build the ops of any transform, in the requested direction.
pub fn build_ops(
    ops: &mut OpVec,
    config: &Config,
    context: &Context,
    transform: &Transform,
    dir: TransformDirection,
) -> Result<()> {
    // Guard against cycles in the color space / look / view transform graph.
    let depth = BUILD_DEPTH.with(|d| {
        let v = d.get() + 1;
        d.set(v);
        v
    });
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            BUILD_DEPTH.with(|d| d.set(d.get() - 1));
        }
    }
    let _guard = Guard;
    if depth > 64 {
        return Err(Error::msg(
            "Cycle detected while building ops for transforms.",
        ));
    }
    crate::for_each_transform!(transform, t => t.build_ops(ops, config, context, dir))
}

// ---------------------------------------------------------------------------
// GroupTransform

use crate::transforms::GroupTransform;

impl Validate for GroupTransform {
    fn validate(&self) -> Result<()> {
        for t in &self.transforms {
            t.validate()?;
        }
        Ok(())
    }
}

impl BuildOps for GroupTransform {
    fn build_ops(
        &self,
        ops: &mut OpVec,
        config: &Config,
        context: &Context,
        dir: TransformDirection,
    ) -> Result<()> {
        match self.direction.combine(dir) {
            TransformDirection::Forward => {
                for t in &self.transforms {
                    build_ops(ops, config, context, t, TransformDirection::Forward)?;
                }
            }
            TransformDirection::Inverse => {
                for t in self.transforms.iter().rev() {
                    build_ops(ops, config, context, t, TransformDirection::Inverse)?;
                }
            }
        }
        Ok(())
    }
}
