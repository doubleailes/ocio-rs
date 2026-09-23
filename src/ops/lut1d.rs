//! lut1d op (TODO: port from OCIO).

use crate::config::Config;
use crate::context::Context;
use crate::error::{Error, Result};
use crate::ops::OpVec;
use crate::transforms::{BuildOps, Validate, Lut1DTransform};
use crate::types::TransformDirection;

impl Validate for Lut1DTransform {
    fn validate(&self) -> Result<()> {
        Ok(())
    }
}

impl BuildOps for Lut1DTransform {
    fn build_ops(&self, _ops: &mut OpVec, _config: &Config, _context: &Context, _dir: TransformDirection) -> Result<()> {
        Err(Error::msg("Lut1DTransform: not implemented"))
    }
}
