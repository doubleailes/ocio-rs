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
    bake_capability, capability, CachedFile, FileFormat, FormatInfo, FormatRegistry,
    FILEFORMAT_COLOR_CORRECTION, FILEFORMAT_COLOR_CORRECTION_COLLECTION,
    FILEFORMAT_COLOR_DECISION_LIST,
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

/// Port of `LoadFileUncached`: read a file with the formats of its
/// extension, then with all the other formats.
fn load_file(filepath: &str) -> Result<CachedFile> {
    let registry = FormatRegistry::instance();
    let extension = std::path::Path::new(filepath)
        .extension()
        .map(|e| e.to_string_lossy().into_owned())
        .unwrap_or_default();
    let possible = registry.formats_for_extension(&extension);
    let open_error = || {
        Error::msg(format!(
            "The specified FileTransform srcfile, '{filepath}', could not be opened. Please confirm the file exists with appropriate read permissions."
        ))
    };

    // Add a separator for the first reader error.
    let mut primary_error_text = String::from("\n");
    for format in &possible {
        let result = std::fs::read(filepath)
            .map_err(|_| open_error())
            .and_then(|data| format.read(&data, filepath, Interpolation::Default));
        match result {
            Ok(cached) => return Ok(cached),
            Err(e) => {
                primary_error_text.push_str(&format!(
                    "    '{}' failed with: {}",
                    format.name(),
                    e.message()
                ));
            }
        }
    }

    // If this fails, try all other formats.
    for format in registry.formats() {
        let format: &dyn FileFormat = format.as_ref();
        if possible.iter().any(|p| {
            std::ptr::addr_eq(*p as *const dyn FileFormat, format as *const dyn FileFormat)
        }) {
            continue;
        }
        let result = std::fs::read(filepath)
            .map_err(|_| open_error())
            .and_then(|data| format.read(&data, filepath, Interpolation::Default));
        if let Ok(cached) = result {
            return Ok(cached);
        }
    }

    // No formats succeeded. Error out with a sensible message.
    let mut os = format!(
        "The specified transform file '{filepath}' could not be loaded.\nAll formats have been tried. (Enable debug log for errors from all formats.) "
    );
    if !possible.is_empty() {
        if possible.len() == 1 {
            os.push_str("The format for the file's extension gave the error:\n");
        } else {
            os.push_str("The formats for the file's extension gave the errors:\n");
        }
        os.push_str(&primary_error_text);
    }
    Err(Error::msg(os))
}

/// Port of `CachedFile::getCDLGroup`.
fn cdl_group(cached: CachedFile) -> Result<GroupTransform> {
    if !cached.is_cdl_collection {
        return Err(Error::msg("Not a CDL file format."));
    }
    Ok(cached.group)
}

/// Port of `GetCDL`: select a CDL by id or index.
fn get_cdl(group: GroupTransform, cdl_id: &str) -> Result<CdlTransform> {
    let cdl_at = |i: usize| match group.transforms.get(i) {
        Some(Transform::Cdl(c)) => Ok(c.clone()),
        _ => Err(Error::msg("Not a CDL file format.")),
    };
    if cdl_id.is_empty() {
        // No cccid, return first cdl.
        if group.transforms.is_empty() {
            return Err(Error::msg("File contains no CDL."));
        }
        return cdl_at(0);
    }

    // Try to parse the cccid as a string id (case sensitive).
    for t in &group.transforms {
        if let Transform::Cdl(c) = t {
            let id = c.metadata.id();
            if !id.is_empty() && id == cdl_id {
                return Ok(c.clone());
            }
        }
    }

    // Try to parse the cccid as an integer index. We want to be strict, so
    // fail if leftover chars in the parse.
    if let Some(index) = string_to_int(cdl_id) {
        let max_index = i64::try_from(group.transforms.len()).unwrap_or(i64::MAX) - 1;
        if index < 0 || index > max_index {
            return Err(Error::missing_file(format!(
                "The specified CDL index {index} is outside the valid range for this file [0,{max_index}]"
            )));
        }
        return cdl_at(usize::try_from(index).unwrap_or(0));
    }

    crate::bail!(
        "The specified CDL Id/Index '{}' could not be loaded from the file.",
        cdl_id
    )
}

/// Port of `StringToInt(..., failIfLeftoverChars = true)` (`strtol` like:
/// leading white spaces and a sign are accepted, no trailing characters).
fn string_to_int(s: &str) -> Option<i64> {
    let t = s.trim_start_matches([' ', '\t', '\n', '\r', '\x0b', '\x0c']);
    let (neg, digits) = match t.as_bytes().first() {
        Some(b'-') => (true, &t[1..]),
        Some(b'+') => (false, &t[1..]),
        _ => (false, t),
    };
    if digits.is_empty() || !digits.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let v: i64 = digits.parse().ok()?;
    let v = if neg { -v } else { v };
    i32::try_from(v).ok().map(i64::from)
}

impl CdlTransform {
    /// Load a CDL from a `.cc`, `.ccc` or `.cdl` file (port of
    /// `CDLTransform::CreateFromFile`). `cdl_id` selects the color
    /// correction by id or by index (empty: the first one).
    pub fn create_from_file(src: &str, cdl_id: &str) -> Result<CdlTransform> {
        if src.is_empty() {
            return Err(Error::msg("Error loading CDL. Source file not specified."));
        }
        let group = cdl_group(load_file(src)?)?;
        get_cdl(group, cdl_id)
    }

    /// Load all the CDLs of a `.cc`, `.ccc` or `.cdl` file (port of
    /// `CDLTransform::CreateGroupFromFile`).
    pub fn create_group_from_file(src: &str) -> Result<GroupTransform> {
        if src.is_empty() {
            return Err(Error::msg("Error loading CDL. Source file not specified."));
        }
        cdl_group(load_file(src)?)
    }
}
