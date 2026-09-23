//! `FileTransform`: locating, reading (with caching) and building ops from
//! files (TODO: port `FileTransform.cpp`).

use crate::config::Config;
use crate::context::Context;
use crate::error::{Error, Result};
use crate::ops::OpVec;
use crate::transforms::{BuildOps, FileTransform, Validate};
use crate::types::TransformDirection;

impl Validate for FileTransform {
    fn validate(&self) -> Result<()> {
        Ok(())
    }
}

impl BuildOps for FileTransform {
    fn build_ops(&self, _ops: &mut OpVec, _config: &Config, _context: &Context, _dir: TransformDirection) -> Result<()> {
        Err(Error::msg("FileTransform: not implemented"))
    }
}
