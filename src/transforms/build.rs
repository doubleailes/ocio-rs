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

// ---------------------------------------------------------------------------
// FileTransform: validated here whatever the features; its ops are built by
// `crate::fileformats::file_transform`, or refused without `file-formats`.

use crate::transforms::FileTransform;

impl Validate for FileTransform {
    fn validate(&self) -> Result<()> {
        // NB: Not validating the interpolation since v1 configs such as the
        // spi examples use interpolation=unknown. So that is a legal usage,
        // even if it makes no sense.
        if self.src.is_empty() {
            crate::bail!("FileTransform: empty file path");
        }
        Ok(())
    }
}

/// Without the `file-formats` feature no file can be read: a config that
/// uses a `FileTransform` still loads, and the processors that need one are
/// an error.
#[cfg(not(feature = "file-formats"))]
impl BuildOps for FileTransform {
    fn build_ops(
        &self,
        _ops: &mut OpVec,
        _config: &Config,
        context: &Context,
        _dir: TransformDirection,
    ) -> Result<()> {
        if self.src.is_empty() {
            crate::bail!("The transform file has not been specified.");
        }
        // A file that cannot be located is the same error with the feature.
        let filepath = context.resolve_file_location(&self.src)?;
        crate::bail!(
            "The transform file: {filepath} cannot be read: the ocio crate was built without \
             its `file-formats` feature."
        )
    }
}

/// The format queries without the `file-formats` feature: no format is
/// readable.
#[cfg(not(feature = "file-formats"))]
impl FileTransform {
    /// Number of file formats that can be read: none.
    pub fn num_formats() -> usize {
        0
    }

    /// Name of the readable format at `index`: always `""`.
    pub fn format_name_by_index(_index: usize) -> &'static str {
        ""
    }

    /// Extension of the readable format at `index`: always `""`.
    pub fn format_extension_by_index(_index: usize) -> &'static str {
        ""
    }

    /// True if a format handles the extension: never.
    pub fn is_format_extension_supported(_extension: &str) -> bool {
        false
    }
}

/// Loading a CDL file without the `file-formats` feature: an error.
#[cfg(not(feature = "file-formats"))]
impl crate::transforms::CdlTransform {
    /// Load a CDL from a `.cc`, `.ccc` or `.cdl` file: an error without the
    /// `file-formats` feature.
    pub fn create_from_file(src: &str, _ccc_id: &str) -> Result<crate::transforms::CdlTransform> {
        crate::bail!("{src}: the ocio crate was built without its `file-formats` feature.")
    }

    /// Load all the CDLs of a `.cc`, `.ccc` or `.cdl` file: an error without
    /// the `file-formats` feature.
    pub fn create_group_from_file(src: &str) -> Result<GroupTransform> {
        crate::bail!("{src}: the ocio crate was built without its `file-formats` feature.")
    }
}
