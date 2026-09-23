//! CC / CCC / CDL (ASC CDL XML) file formats (port of OCIO
//! `fileformats/FileFormatCC.cpp`, `FileFormatCCC.cpp`, `FileFormatCDL.cpp`
//! and `fileformats/cdl/*`), plus the file loading helpers of
//! `CDLTransform` (`CreateFromFile`, `CreateGroupFromFile`).
//!
//! Reading produces a [`CachedFile`] whose group holds the color corrections
//! as [`CdlTransform`]s (with their id and descriptions in the metadata) and
//! `is_cdl_collection` set; a `FileTransform` selects one of them with its
//! `ccc_id`.

mod parser;
mod writer;

#[cfg(test)]
mod tests;

use super::{
    bake_capability, capability, CachedFile, FileFormat, FormatInfo, FILEFORMAT_COLOR_CORRECTION,
    FILEFORMAT_COLOR_CORRECTION_COLLECTION, FILEFORMAT_COLOR_DECISION_LIST,
};
use crate::config::Config;
use crate::context::Context;
use crate::error::{Error, Result};
use crate::transforms::{CdlTransform, GroupTransform, Transform};
use crate::types::Interpolation;

/// The `.cc` format (a single ColorCorrection).
struct CcFormat;

/// The `.ccc` format (a ColorCorrectionCollection).
struct CccFormat;

/// The `.cdl` format (a ColorDecisionList).
struct CdlFormat;

pub(crate) fn create_cc() -> Box<dyn FileFormat> {
    Box::new(CcFormat)
}

pub(crate) fn create_ccc() -> Box<dyn FileFormat> {
    Box::new(CccFormat)
}

pub(crate) fn create_cdl() -> Box<dyn FileFormat> {
    Box::new(CdlFormat)
}

fn info(name: &'static str, extension: &'static str) -> Vec<FormatInfo> {
    vec![FormatInfo {
        name,
        extension,
        capabilities: capability::READ | capability::WRITE,
        bake_capabilities: bake_capability::NONE,
    }]
}

/// The cached file of a collection (CCC or CDL): all the CDLs and the
/// descriptive elements of the root element.
fn read_collection(data: &[u8], file_name: &str) -> Result<CachedFile> {
    let parsed = parser::parse_cdl(data, file_name)?;
    parser::check_unique_ids(&parsed.info.transforms)?;
    let mut group = GroupTransform::from_transforms(
        parsed
            .info
            .transforms
            .into_iter()
            .map(Transform::Cdl)
            .collect(),
    );
    group.metadata = parsed.info.metadata;
    Ok(CachedFile {
        group,
        is_cdl_collection: true,
    })
}

/// Check that all the transforms of a group are CDLs (for the collection
/// writers).
fn collection_cdls<'a>(
    group: &'a GroupTransform,
    format_name: &str,
) -> Result<Vec<&'a CdlTransform>> {
    if group.transforms.is_empty() {
        crate::bail!(
            "Write to {}: there should be at least one CDL.",
            format_name
        );
    }
    group
        .transforms
        .iter()
        .map(|t| match t {
            Transform::Cdl(c) => Ok(c),
            _ => Err(Error::msg(format!(
                "Write to {format_name}: only CDL can be written."
            ))),
        })
        .collect()
}

impl FileFormat for CcFormat {
    fn format_info(&self) -> Vec<FormatInfo> {
        info(FILEFORMAT_COLOR_CORRECTION, "cc")
    }

    fn read(
        &self,
        data: &[u8],
        original_file_name: &str,
        _interp: Interpolation,
    ) -> Result<CachedFile> {
        let parsed = parser::parse_cdl(data, original_file_name).and_then(|p| {
            if p.info.transforms.is_empty() {
                Err(Error::msg("No transform found."))
            } else {
                Ok(p)
            }
        });
        let parsed = parsed.map_err(|e| {
            Error::msg(format!(
                "Error parsing .cc file. Does not appear to contain a valid ASC CDL XML:{}",
                e.message()
            ))
        })?;
        if !parsed.is_cc {
            crate::bail!("File '{}' is not a .cc file.", original_file_name);
        }
        // Only the first color correction is used.
        let first = parsed
            .info
            .transforms
            .into_iter()
            .next()
            .unwrap_or_default();
        Ok(CachedFile {
            group: GroupTransform::from_transforms(vec![Transform::Cdl(first)]),
            is_cdl_collection: true,
        })
    }

    fn write(
        &self,
        _config: &Config,
        _context: &Context,
        group: &GroupTransform,
        _format_name: &str,
    ) -> Result<String> {
        if group.transforms.len() != 1 {
            return Err(Error::msg("CDL write: there should be a single CDL."));
        }
        match &group.transforms[0] {
            Transform::Cdl(c) => Ok(writer::write_cc(c)),
            _ => Err(Error::msg("CDL write: only CDL can be written.")),
        }
    }
}

impl FileFormat for CccFormat {
    fn format_info(&self) -> Vec<FormatInfo> {
        info(FILEFORMAT_COLOR_CORRECTION_COLLECTION, "ccc")
    }

    fn read(
        &self,
        data: &[u8],
        original_file_name: &str,
        _interp: Interpolation,
    ) -> Result<CachedFile> {
        read_collection(data, original_file_name)
    }

    fn write(
        &self,
        _config: &Config,
        _context: &Context,
        group: &GroupTransform,
        format_name: &str,
    ) -> Result<String> {
        let cdls = collection_cdls(group, format_name)?;
        Ok(writer::write_ccc(&group.metadata, &cdls))
    }
}

impl FileFormat for CdlFormat {
    fn format_info(&self) -> Vec<FormatInfo> {
        info(FILEFORMAT_COLOR_DECISION_LIST, "cdl")
    }

    fn read(
        &self,
        data: &[u8],
        original_file_name: &str,
        _interp: Interpolation,
    ) -> Result<CachedFile> {
        read_collection(data, original_file_name)
    }

    fn write(
        &self,
        _config: &Config,
        _context: &Context,
        group: &GroupTransform,
        format_name: &str,
    ) -> Result<String> {
        let cdls = collection_cdls(group, format_name)?;
        Ok(writer::write_cdl(&group.metadata, &cdls))
    }
}
