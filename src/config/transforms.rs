//! Op building for the config-dependent transforms (TODO: port
//! `ColorSpaceTransform.cpp`, `DisplayViewTransform.cpp`, `LookTransform.cpp`).

use crate::config::Config;
use crate::context::Context;
use crate::error::{Error, Result};
use crate::ops::OpVec;
use crate::transforms::{BuildOps, ColorSpaceTransform, DisplayViewTransform, LookTransform, Validate};
use crate::types::TransformDirection;

macro_rules! stub {
    ($t:ty) => {
        impl Validate for $t {
            fn validate(&self) -> Result<()> {
                Ok(())
            }
        }
        impl BuildOps for $t {
            fn build_ops(&self, _ops: &mut OpVec, _config: &Config, _context: &Context, _dir: TransformDirection) -> Result<()> {
                Err(Error::msg(concat!(stringify!($t), ": not implemented")))
            }
        }
    };
}

stub!(ColorSpaceTransform);
stub!(DisplayViewTransform);
stub!(LookTransform);
