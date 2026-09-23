//! lut3d op (TODO: port from OCIO).

use crate::config::Config;
use crate::context::Context;
use crate::error::{Error, Result};
use crate::ops::OpVec;
use crate::transforms::{BuildOps, Validate, Lut3DTransform};
use crate::types::TransformDirection;

impl Validate for Lut3DTransform {
    fn validate(&self) -> Result<()> {
        Ok(())
    }
}

impl BuildOps for Lut3DTransform {
    fn build_ops(&self, _ops: &mut OpVec, _config: &Config, _context: &Context, _dir: TransformDirection) -> Result<()> {
        Err(Error::msg("Lut3DTransform: not implemented"))
    }
}
