//! log op (TODO: port from OCIO).

use crate::config::Config;
use crate::context::Context;
use crate::error::{Error, Result};
use crate::ops::OpVec;
use crate::transforms::{BuildOps, Validate, LogTransform, LogAffineTransform, LogCameraTransform};
use crate::types::TransformDirection;

impl Validate for LogTransform {
    fn validate(&self) -> Result<()> {
        Ok(())
    }
}

impl BuildOps for LogTransform {
    fn build_ops(&self, _ops: &mut OpVec, _config: &Config, _context: &Context, _dir: TransformDirection) -> Result<()> {
        Err(Error::msg("LogTransform: not implemented"))
    }
}

impl Validate for LogAffineTransform {
    fn validate(&self) -> Result<()> {
        Ok(())
    }
}

impl BuildOps for LogAffineTransform {
    fn build_ops(&self, _ops: &mut OpVec, _config: &Config, _context: &Context, _dir: TransformDirection) -> Result<()> {
        Err(Error::msg("LogAffineTransform: not implemented"))
    }
}

impl Validate for LogCameraTransform {
    fn validate(&self) -> Result<()> {
        Ok(())
    }
}

impl BuildOps for LogCameraTransform {
    fn build_ops(&self, _ops: &mut OpVec, _config: &Config, _context: &Context, _dir: TransformDirection) -> Result<()> {
        Err(Error::msg("LogCameraTransform: not implemented"))
    }
}
