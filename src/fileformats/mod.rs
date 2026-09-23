//! LUT / transform file formats (port of `FileTransform.h` registry and the
//! `fileformats/` directory).
//!
//! Every format implements [`FileFormat`]. Reading a file produces a
//! [`CachedFile`], i.e. a [`GroupTransform`] of plain transforms (LUTs,
//! matrices, ranges, ...), so no format needs its own op building logic.

pub mod cdl;
pub mod ctf;
pub mod file_transform;
pub mod format_3dl;
pub mod format_csp;
pub mod format_discreet1dl;
pub mod format_hdl;
pub mod format_icc;
pub mod format_iridas_cube;
pub mod format_iridas_itx;
pub mod format_iridas_look;
pub mod format_pandora;
pub mod format_resolve_cube;
pub mod format_spi1d;
pub mod format_spi3d;
pub mod format_spimtx;
pub mod format_truelight;
pub mod format_vf;
pub mod utils;

use crate::baker::Baker;
use crate::config::Config;
use crate::context::Context;
use crate::error::{Error, Result};
use crate::transforms::GroupTransform;
use crate::types::Interpolation;
use std::sync::OnceLock;

pub const FILEFORMAT_CLF: &str = "Academy/ASC Common LUT Format";
pub const FILEFORMAT_CTF: &str = "Color Transform Format";
pub const FILEFORMAT_COLOR_CORRECTION: &str = "ColorCorrection";
pub const FILEFORMAT_COLOR_CORRECTION_COLLECTION: &str = "ColorCorrectionCollection";
pub const FILEFORMAT_COLOR_DECISION_LIST: &str = "ColorDecisionList";

/// Format capability bits.
pub mod capability {
    pub const NONE: u32 = 0;
    pub const READ: u32 = 1;
    pub const BAKE: u32 = 2;
    pub const WRITE: u32 = 4;
}

/// Bake capability bits.
pub mod bake_capability {
    pub const NONE: u32 = 0;
    pub const LUT3D: u32 = 1;
    pub const LUT1D: u32 = 2;
    pub const LUT1D_3D: u32 = 4;
}

/// Description of a format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatInfo {
    /// Globally unique name.
    pub name: &'static str,
    /// Lower-case extension (not unique).
    pub extension: &'static str,
    /// `capability::*` bits.
    pub capabilities: u32,
    /// `bake_capability::*` bits.
    pub bake_capabilities: u32,
}

/// The content of a file, as transforms.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CachedFile {
    /// The transforms of the file, in order (for CDL based formats, the
    /// color corrections, each a `Transform::Cdl` with its id in the
    /// metadata).
    pub group: GroupTransform,
    /// True for `.cc` / `.ccc` / `.cdl` files: `group` holds CDLs and a
    /// `FileTransform` selects one by `ccc_id` (id or index).
    pub is_cdl_collection: bool,
}

impl CachedFile {
    pub fn new(group: GroupTransform) -> Self {
        Self { group, is_cdl_collection: false }
    }
}

/// A file format.
pub trait FileFormat: Send + Sync {
    /// Names, extensions and capabilities handled by this reader.
    fn format_info(&self) -> Vec<FormatInfo>;

    /// Parse `data`. `original_file_name` is used for error messages and
    /// relative references; `interp` is the interpolation requested by the
    /// `FileTransform` (formats validate it and set it on their LUTs).
    fn read(&self, data: &[u8], original_file_name: &str, interp: Interpolation) -> Result<CachedFile>;

    /// Bake a LUT with `baker` into the format named `format_name`.
    fn bake(&self, _baker: &Baker, format_name: &str) -> Result<Vec<u8>> {
        Err(Error::msg(format!("Format {format_name} does not support baking.")))
    }

    /// Write a group transform into the format named `format_name`.
    fn write(&self, _config: &Config, _context: &Context, _group: &GroupTransform, format_name: &str) -> Result<String> {
        Err(Error::msg(format!("Format {format_name} does not support writing.")))
    }

    /// True for binary formats.
    fn is_binary(&self) -> bool {
        false
    }

    /// Name of the first format info.
    fn name(&self) -> &'static str {
        self.format_info().first().map(|i| i.name).unwrap_or("")
    }
}

/// The registry of all formats (port of `FormatRegistry`).
pub struct FormatRegistry {
    formats: Vec<Box<dyn FileFormat>>,
}

impl FormatRegistry {
    fn new() -> Self {
        let formats: Vec<Box<dyn FileFormat>> = vec![
            format_3dl::create(),
            cdl::create_cc(),
            cdl::create_ccc(),
            cdl::create_cdl(),
            ctf::create(),
            format_csp::create(),
            format_discreet1dl::create(),
            format_icc::create(),
            format_hdl::create(),
            format_iridas_itx::create(),
            format_iridas_cube::create(),
            format_iridas_look::create(),
            format_pandora::create(),
            format_resolve_cube::create(),
            format_spi1d::create(),
            format_spi3d::create(),
            format_spimtx::create(),
            format_truelight::create(),
            format_vf::create(),
        ];
        Self { formats }
    }

    /// The global registry.
    pub fn instance() -> &'static FormatRegistry {
        static REG: OnceLock<FormatRegistry> = OnceLock::new();
        REG.get_or_init(FormatRegistry::new)
    }

    /// All formats.
    pub fn formats(&self) -> &[Box<dyn FileFormat>] {
        &self.formats
    }

    /// Format providing `name` (case insensitive).
    pub fn format_by_name(&self, name: &str) -> Option<&dyn FileFormat> {
        let lname = name.to_ascii_lowercase();
        self.formats
            .iter()
            .find(|f| f.format_info().iter().any(|i| i.name.to_ascii_lowercase() == lname))
            .map(|b| b.as_ref())
    }

    /// Formats that can read the given (lower-case, no dot) extension.
    pub fn formats_for_extension(&self, ext: &str) -> Vec<&dyn FileFormat> {
        let ext = ext.to_ascii_lowercase();
        self.formats
            .iter()
            .filter(|f| {
                f.format_info()
                    .iter()
                    .any(|i| i.extension == ext && i.capabilities & capability::READ != 0)
            })
            .map(|b| b.as_ref())
            .collect()
    }

    /// All (name, extension) pairs having the capability.
    pub fn format_infos(&self, cap: u32) -> Vec<FormatInfo> {
        self.formats
            .iter()
            .flat_map(|f| f.format_info())
            .filter(|i| i.capabilities & cap != 0)
            .collect()
    }

    pub fn num_formats(&self, cap: u32) -> usize {
        self.format_infos(cap).len()
    }

    pub fn is_format_extension_supported(&self, ext: &str) -> bool {
        let ext = ext.trim_start_matches('.').to_ascii_lowercase();
        self.format_infos(capability::READ).iter().any(|i| i.extension == ext)
    }
}
