//! Builtin transforms (ACES, camera log curves, display curves) and builtin
//! configs (TODO: port `transforms/builtins` and `builtinconfigs`).

pub mod configs;

use crate::config::Config;
use crate::context::Context;
use crate::error::{Error, Result};
use crate::ops::OpVec;
use crate::transforms::{BuildOps, BuiltinTransform, Validate};
use crate::types::TransformDirection;

impl Validate for BuiltinTransform {
    fn validate(&self) -> Result<()> {
        Ok(())
    }
}

impl BuildOps for BuiltinTransform {
    fn build_ops(&self, _ops: &mut OpVec, _config: &Config, _context: &Context, _dir: TransformDirection) -> Result<()> {
        Err(Error::msg("BuiltinTransform: not implemented"))
    }
}
