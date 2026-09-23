//! gamma op (TODO: port from OCIO).

use crate::config::Config;
use crate::context::Context;
use crate::error::{Error, Result};
use crate::ops::OpVec;
use crate::transforms::{BuildOps, Validate, ExponentWithLinearTransform};
use crate::types::TransformDirection;

impl Validate for ExponentWithLinearTransform {
    fn validate(&self) -> Result<()> {
        Ok(())
    }
}

impl BuildOps for ExponentWithLinearTransform {
    fn build_ops(&self, _ops: &mut OpVec, _config: &Config, _context: &Context, _dir: TransformDirection) -> Result<()> {
        Err(Error::msg("ExponentWithLinearTransform: not implemented"))
    }
}
