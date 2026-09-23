//! exponent op (TODO: port from OCIO).

use crate::config::Config;
use crate::context::Context;
use crate::error::{Error, Result};
use crate::ops::OpVec;
use crate::transforms::{BuildOps, Validate, ExponentTransform};
use crate::types::TransformDirection;

impl Validate for ExponentTransform {
    fn validate(&self) -> Result<()> {
        Ok(())
    }
}

impl BuildOps for ExponentTransform {
    fn build_ops(&self, _ops: &mut OpVec, _config: &Config, _context: &Context, _dir: TransformDirection) -> Result<()> {
        Err(Error::msg("ExponentTransform: not implemented"))
    }
}
