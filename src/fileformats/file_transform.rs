//! `FileTransform`: locating, reading (with caching) and building ops from
//! files (port of `FileTransform.cpp`).
//!
//! * The file path is resolved with the context
//!   ([`Context::resolve_file_location`]).
//! * The file is read with a process-wide cache keyed by the resolved path
//!   and the interpolation (the readers apply the interpolation to their
//!   LUTs). [`clear_file_transform_caches`] flushes the cache.
//! * The reader is selected from the file extension first; all the other
//!   readers are tried next.
//! * The ops are a file marker followed by the ops of the transforms of the
//!   file (for CDL collections, the selected color correction).

use super::{
    capability, CachedFile, FileFormat, FormatRegistry, FILEFORMAT_COLOR_CORRECTION,
    FILEFORMAT_COLOR_DECISION_LIST,
};
use crate::config::Config;
use crate::context::Context;
use crate::error::{Error, Result};
use crate::ops::noop::create_file_no_op;
use crate::ops::OpVec;
use crate::transforms::build::build_ops;
use crate::transforms::{
    BuildOps, CdlTransform, FileTransform, GroupTransform, Transform, Validate,
};
use crate::types::{CdlStyle, Interpolation, TransformDirection, OCIO_DISABLE_ALL_CACHES};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

// ---------------------------------------------------------------------------
// Validation and format queries.

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

impl FileTransform {
    /// Number of file formats that can be read (port of
    /// `FileTransform::GetNumFormats`).
    pub fn num_formats() -> usize {
        FormatRegistry::instance()
            .format_infos(capability::READ)
            .len()
    }

    /// Name of the readable format at `index`, `""` if out of range (port of
    /// `FileTransform::GetFormatNameByIndex`).
    pub fn format_name_by_index(index: usize) -> &'static str {
        FormatRegistry::instance()
            .format_infos(capability::READ)
            .get(index)
            .map(|i| i.name)
            .unwrap_or("")
    }

    /// Extension of the readable format at `index`, `""` if out of range
    /// (port of `FileTransform::GetFormatExtensionByIndex`).
    pub fn format_extension_by_index(index: usize) -> &'static str {
        FormatRegistry::instance()
            .format_infos(capability::READ)
            .get(index)
            .map(|i| i.extension)
            .unwrap_or("")
    }

    /// True if a format handles the extension (case insensitive, with or
    /// without the leading dot) (port of
    /// `FileTransform::IsFormatExtensionSupported`).
    pub fn is_format_extension_supported(extension: &str) -> bool {
        // Early return false with an input of just the dot or empty.
        if extension.is_empty() || extension == "." {
            return false;
        }
        // If a dot is present at the start, ignore it.
        let ext = extension
            .strip_prefix('.')
            .unwrap_or(extension)
            .to_ascii_lowercase();
        FormatRegistry::instance()
            .formats()
            .iter()
            .any(|f| f.format_info().iter().any(|i| i.extension == ext))
    }
}

// ---------------------------------------------------------------------------
// Loading.

/// Port of `pystring::os::path::splitext`: the extension of `path`
/// (including the dot), or `""`. Leading dots of the file name do not start
/// an extension.
fn splitext_extension(path: &str) -> &str {
    let sep = path.rfind('/');
    #[cfg(windows)]
    let sep = match (sep, path.rfind('\\')) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    };
    let Some(dot) = path.rfind('.') else {
        return "";
    };
    let filename_start = sep.map(|s| s + 1).unwrap_or(0);
    if dot < filename_start {
        return "";
    }
    if path.as_bytes()[filename_start..dot]
        .iter()
        .any(|&c| c != b'.')
    {
        return &path[dot..];
    }
    ""
}

/// Read and parse `filepath` (port of `LoadFileUncached`): the formats
/// matching the file extension are tried first, then all the other formats.
fn load_file_uncached(
    filepath: &str,
    interp: Interpolation,
) -> Result<(&'static dyn FileFormat, CachedFile)> {
    let registry = FormatRegistry::instance();

    // Remove the leading '.'.
    let extension = splitext_extension(filepath).replacen('.', "", 1);
    let possible_formats = registry.formats_for_extension(&extension);

    let data = std::fs::read(filepath).map_err(|_| {
        Error::msg(format!(
            "The specified FileTransform srcfile, '{filepath}', could not be opened. Please confirm the file exists with appropriate read permissions."
        ))
    });

    // Try the initial formats.
    let mut primary_error_text = String::from("\n");
    for &format in &possible_formats {
        let res = data
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|d| format.read(d, filepath, interp));
        match res {
            Ok(cached) => return Ok((format, cached)),
            Err(e) => {
                primary_error_text.push_str("    '");
                primary_error_text.push_str(format.name());
                primary_error_text.push_str("' failed with: ");
                primary_error_text.push_str(e.message());
            }
        }
    }

    // If this fails, try all other formats.
    for alt in registry.formats() {
        let alt: &'static dyn FileFormat = alt.as_ref();
        // Do not try primary formats twice.
        if possible_formats
            .iter()
            .any(|&f| std::ptr::addr_eq(f as *const dyn FileFormat, alt as *const dyn FileFormat))
        {
            continue;
        }
        if let Ok(d) = &data {
            if let Ok(cached) = alt.read(d, filepath, interp) {
                return Ok((alt, cached));
            }
        }
    }

    // No formats succeeded. Error out with a sensible message.
    let mut os = format!(
        "The specified transform file '{filepath}' could not be loaded.\nAll formats have been tried. (Enable debug log for errors from all formats.) "
    );
    if !possible_formats.is_empty() {
        if possible_formats.len() == 1 {
            os.push_str("The format for the file's extension gave the error:\n");
        } else {
            os.push_str("The formats for the file's extension gave the errors:\n");
        }
        os.push_str(&primary_error_text);
    }
    Err(Error::msg(os))
}

/// A file loaded by [`get_cached_file`]: the format that read it and its
/// content.
type LoadResult = std::result::Result<(&'static dyn FileFormat, Arc<CachedFile>), String>;

/// Cache entries: each entry is initialized once (the loads of the *same*
/// file mutually block, the loads of other files do not).
type FileCache = Mutex<HashMap<(String, Interpolation), Arc<OnceLock<LoadResult>>>>;

fn file_cache() -> &'static FileCache {
    static CACHE: OnceLock<FileCache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The file cache is disabled when the `OCIO_DISABLE_ALL_CACHES`
/// environment variable is present (checked once, as in OCIO).
fn file_cache_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os(OCIO_DISABLE_ALL_CACHES).is_none())
}

/// Flush the file content cache (port of `ClearFileTransformCaches`).
pub fn clear_file_transform_caches() {
    if let Ok(mut cache) = file_cache().lock() {
        cache.clear();
    }
}

/// Read `filepath` (already resolved) with the process-wide cache (port of
/// `GetCachedFileAndFormat`). Returns the format that read the file and the
/// file content.
pub fn get_cached_file(
    filepath: &str,
    interp: Interpolation,
) -> Result<(&'static dyn FileFormat, Arc<CachedFile>)> {
    let entry = if file_cache_enabled() {
        let mut cache = file_cache().lock().unwrap_or_else(|p| p.into_inner());
        cache
            .entry((filepath.to_string(), interp))
            .or_default()
            .clone()
    } else {
        Arc::new(OnceLock::new())
    };

    // If this file has already been loaded, return the result immediately.
    let result = entry.get_or_init(|| {
        load_file_uncached(filepath, interp)
            .map(|(format, cached)| (format, Arc::new(cached)))
            .map_err(|e| e.message().to_string())
    });

    match result {
        Ok((format, cached)) => Ok((*format, cached.clone())),
        Err(msg) => Err(Error::msg(msg.clone())),
    }
}

// ---------------------------------------------------------------------------
// Op building.

thread_local! {
    /// The files being loaded by the current thread (used to detect
    /// references creating a recursion, e.g. CTF references).
    static FILES_BEING_LOADED: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// Pop the file being loaded when dropped.
struct LoadingGuard;

impl LoadingGuard {
    fn new(filepath: &str) -> Self {
        FILES_BEING_LOADED.with(|f| f.borrow_mut().push(filepath.to_string()));
        LoadingGuard
    }
}

impl Drop for LoadingGuard {
    fn drop(&mut self) {
        FILES_BEING_LOADED.with(|f| {
            f.borrow_mut().pop();
        });
    }
}

/// Select the color correction of a CDL based file (`.cc`, `.ccc`, `.cdl`)
/// using the `ccc_id` of the file transform, either as an id or as an index
/// (ports of the `buildFileOps` of the CC, CCC and CDL formats).
fn select_cdl(
    format: &dyn FileFormat,
    group: &GroupTransform,
    file_transform: &FileTransform,
    context: &Context,
) -> Result<CdlTransform> {
    let cdls: Vec<&CdlTransform> = group
        .transforms
        .iter()
        .filter_map(|t| match t {
            Transform::Cdl(c) => Some(c),
            _ => None,
        })
        .collect();

    let mut cdl = if format.name() == FILEFORMAT_COLOR_CORRECTION {
        // A .cc file holds a single color correction.
        match cdls.first() {
            Some(c) => (*c).clone(),
            None => crate::bail!("Cannot build .cc Op. Invalid cache type."),
        }
    } else {
        let file_kind = if format.name() == FILEFORMAT_COLOR_DECISION_LIST {
            "cdl"
        } else {
            "ccc"
        };

        // Below this point, ExceptionMissingFile is used on errors rather
        // than Exception (the file is valid, only the specified cccid can't
        // be found).
        let cccid = context.resolve_string_var(&file_transform.ccc_id);

        // Try to parse the cccid as a string id.
        if let Some(c) = cdls.iter().find(|c| !c.id().is_empty() && c.id() == cccid) {
            (*c).clone()
        } else {
            // Try to parse the cccid as an integer index. We want to be
            // strict, so fail if leftover chars in the parse. Use 0 for an
            // empty string.
            let index = if cccid.is_empty() {
                Some(0)
            } else {
                super::utils::string_to_int(&cccid, true)
            };
            match index {
                Some(cccindex) => {
                    let maxindex = cdls.len() as i64 - 1;
                    if cccindex < 0 || cccindex as i64 > maxindex {
                        return Err(Error::missing_file(format!(
                            "The specified cccindex {cccindex} is outside the valid range for this file [0,{maxindex}]"
                        )));
                    }
                    cdls[cccindex as usize].clone()
                }
                None => {
                    return Err(Error::missing_file(format!(
                        "You must specify a valid cccid to load from the {file_kind} file (either by name or index). id='{cccid}' is not found in the file, and is not parsable as an integer index."
                    )))
                }
            }
        }
    };

    if file_transform.cdl_style != CdlStyle::default() {
        cdl.style = file_transform.cdl_style;
    }
    Ok(cdl)
}

/// Append the ops of the content of a file (port of the formats'
/// `buildFileOps`).
fn build_file_ops(
    ops: &mut OpVec,
    config: &Config,
    context: &Context,
    format: &dyn FileFormat,
    cached: &CachedFile,
    file_transform: &FileTransform,
    dir: TransformDirection,
) -> Result<()> {
    let new_dir = file_transform.direction.combine(dir);

    if cached.is_cdl_collection {
        let cdl = select_cdl(format, &cached.group, file_transform, context)?;
        return build_ops(ops, config, context, &Transform::Cdl(cdl), new_dir);
    }

    // Equivalent to building the group, without copying its content.
    let group = &cached.group;
    match group.direction.combine(new_dir) {
        TransformDirection::Forward => {
            for t in &group.transforms {
                build_ops(ops, config, context, t, TransformDirection::Forward)?;
            }
        }
        TransformDirection::Inverse => {
            for t in group.transforms.iter().rev() {
                build_ops(ops, config, context, t, TransformDirection::Inverse)?;
            }
        }
    }
    Ok(())
}

impl BuildOps for FileTransform {
    fn build_ops(
        &self,
        ops: &mut OpVec,
        config: &Config,
        context: &Context,
        dir: TransformDirection,
    ) -> Result<()> {
        if self.src.is_empty() {
            crate::bail!("The transform file has not been specified.");
        }

        let filepath = context.resolve_file_location(&self.src)?;

        // Verify the recursion is valid: error if the file is still being
        // loaded and is the same as the one about to be loaded.
        let recursion = FILES_BEING_LOADED
            .with(|f| f.borrow().iter().any(|p| p.eq_ignore_ascii_case(&filepath)));
        if recursion {
            crate::bail!("Reference to: {filepath} is creating a recursion.");
        }

        let (format, cached) = get_cached_file(&filepath, self.interpolation)?;

        let _guard = LoadingGuard::new(&filepath);
        let res = (|| {
            // Add the file marker.
            create_file_no_op(ops, &filepath);
            // CLF/CTF put their ProcessList information into the processor
            // metadata.
            let name = format.name();
            if name == crate::fileformats::FILEFORMAT_CLF || name == crate::fileformats::FILEFORMAT_CTF {
                crate::ops::noop::create_metadata_no_op(ops, &cached.group.metadata);
            }
            build_file_ops(ops, config, context, format, &cached, self, dir)
        })();

        res.map_err(|e| {
            Error::msg(format!(
                "The transform file: {filepath} failed while building ops with this error: {}",
                e.message()
            ))
        })
    }
}

// ---------------------------------------------------------------------------
// Context variables.

/// Collect the context variables needed to resolve the file of `tr` (its
/// source, the search path and the CCC id) into `used_context_vars`.
/// Returns true if context variables are needed (port of
/// `CollectContextVariables`).
pub fn collect_context_variables(
    _config: &Config,
    context: &Context,
    tr: &FileTransform,
    used_context_vars: &mut Context,
) -> bool {
    let src = tr.src.as_str();
    if src.is_empty() {
        return false;
    }

    let mut found_context_vars = false;

    let new_context = || {
        let mut c = Context::new();
        c.set_search_path(&context.search_path());
        c.set_working_dir(context.working_dir());
        c
    };

    // Collect the context variables needed to resolve the src string itself
    // (not involving the search path yet).
    let mut ctx_filename = new_context();
    let resolved_string = context.resolve_string_var_with_used(src, &mut ctx_filename);
    if resolved_string != src {
        found_context_vars = true;
        used_context_vars.add_string_vars(&ctx_filename);
    }

    // Determine if any context variables are needed to resolve the file
    // name: compare the resolved location with and without using the
    // environment (the search path collection may contain some variables
    // that are not actually used).
    let empty_context = new_context();
    let mut ctx_filepath = new_context();

    let same = context
        .resolve_file_location_with_used(&resolved_string, &mut ctx_filepath)
        .and_then(|resolved| {
            empty_context
                .resolve_file_location(&resolved_string)
                .map(|e| e == resolved)
        });
    match same {
        Ok(true) => {}
        // A difference, or a failure to resolve (it's not the mandate of
        // this function to report that kind of problem): to be safe,
        // consider there is a context variable.
        _ => {
            found_context_vars = true;
            used_context_vars.add_string_vars(&ctx_filepath);
        }
    }

    // Check if the CCCID is using a context variable.
    let mut ctx_cccid = Context::new();
    let resolved_cccid = context.resolve_string_var_with_used(&tr.ccc_id, &mut ctx_cccid);
    if resolved_cccid != tr.ccc_id {
        found_context_vars = true;
        used_context_vars.add_string_vars(&ctx_cccid);
    }

    found_context_vars
}

// ---------------------------------------------------------------------------
// CDL files.

/// Port of `GetCDL`: select a CDL of `group` by id, or by index.
fn get_cdl(group: &GroupTransform, cdl_id: &str) -> Result<CdlTransform> {
    let cdls: Vec<&CdlTransform> = group
        .transforms
        .iter()
        .filter_map(|t| match t {
            Transform::Cdl(c) => Some(c),
            _ => None,
        })
        .collect();

    if cdl_id.is_empty() {
        // No cccid, return the first CDL.
        return match cdls.first() {
            Some(c) => Ok((*c).clone()),
            None => Err(Error::msg("File contains no CDL.")),
        };
    }

    // Try to parse the cccid as a string id (case sensitive).
    if let Some(c) = cdls.iter().find(|c| !c.id().is_empty() && c.id() == cdl_id) {
        return Ok((*c).clone());
    }

    // Try to parse the cccid as an integer index. We want to be strict, so
    // fail if leftover chars in the parse.
    if let Some(index) = super::utils::string_to_int(cdl_id, true) {
        let maxindex = cdls.len() as i64 - 1;
        if index < 0 || index as i64 > maxindex {
            return Err(Error::missing_file(format!(
                "The specified CDL index {index} is outside the valid range for this file [0,{maxindex}]"
            )));
        }
        return Ok(cdls[index as usize].clone());
    }

    crate::bail!("The specified CDL Id/Index '{cdl_id}' could not be loaded from the file.");
}

/// The CDL group of a file (port of `CachedFile::getCDLGroup`).
fn cdl_group_from_file(src: &str) -> Result<GroupTransform> {
    if src.is_empty() {
        crate::bail!("Error loading CDL. Source file not specified.");
    }
    let (_, cached) = get_cached_file(src, Interpolation::Default)?;
    if !cached.is_cdl_collection {
        crate::bail!("Not a CDL file format.");
    }
    Ok(cached.group.clone())
}

impl CdlTransform {
    /// Load a CDL from a `.cc`, `.ccc` or `.cdl` file. `ccc_id` selects the
    /// color correction by id or by index (the first one if empty) (port of
    /// `CDLTransform::CreateFromFile`).
    pub fn create_from_file(src: &str, ccc_id: &str) -> Result<CdlTransform> {
        let group = cdl_group_from_file(src)?;
        get_cdl(&group, ccc_id)
    }

    /// Load all the CDLs of a `.cc`, `.ccc` or `.cdl` file (port of
    /// `CDLTransform::CreateGroupFromFile`).
    pub fn create_group_from_file(src: &str) -> Result<GroupTransform> {
        cdl_group_from_file(src)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fileformats::FILEFORMAT_CLF;
    use crate::format_metadata::FormatMetadata;

    fn test_files_dir() -> String {
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/files").to_string()
    }

    fn test_file(name: &str) -> String {
        format!("{}/{}", test_files_dir(), name)
    }

    fn check_error<T>(res: Result<T>, what: &str) {
        match res {
            Ok(_) => panic!("expected an error containing '{what}'"),
            Err(e) => assert!(
                e.message().contains(what),
                "'{}' does not contain '{}'",
                e.message(),
                what
            ),
        }
    }

    /// Port of `GetFileTransformProcessor`.
    fn file_transform_processor(name: &str) -> Result<crate::processor::Processor> {
        let config = Config::create_raw();
        let ft = FileTransform::new(&test_file(name));
        crate::processor::Processor::from_transform(
            &config,
            &Context::new(),
            &Transform::File(ft),
            TransformDirection::Forward,
        )
    }

    /// Port of `BuildOpsTest`.
    fn build_ops_test(name: &str, dir: TransformDirection) -> Result<OpVec> {
        let config = Config::create_raw();
        let ft = FileTransform::new(&test_file(name));
        let mut ops = OpVec::new();
        ft.build_ops(&mut ops, &config, &Context::new(), dir)?;
        Ok(ops)
    }

    #[test]
    fn basic() {
        let mut ft = FileTransform::default();
        assert_eq!(ft.direction, TransformDirection::Forward);
        ft.direction = TransformDirection::Inverse;
        assert_eq!(ft.direction, TransformDirection::Inverse);
        assert_eq!(ft.src, "");
        assert_eq!(ft.ccc_id, "");
        assert_eq!(ft.cdl_style, CdlStyle::NoClamp);
        assert_eq!(ft.interpolation, Interpolation::Default);
    }

    #[test]
    fn validate() {
        let mut tr = FileTransform::new("lut3d_17x17x17_32f_12i.clf");
        assert!(tr.validate().is_ok());
        tr.src.clear();
        check_error(tr.validate(), "FileTransform: empty file path");
    }

    #[test]
    fn load_file_fail() {
        // Legacy Lustre 1D LUT files. Similar to supported formats but
        // actually are different formats.
        check_error(
            file_transform_processor("legacy_slog_to_log_v3_lustre.lut"),
            "could not be loaded",
        );
        check_error(
            file_transform_processor("legacy_flmlk_desat.lut"),
            "could not be loaded",
        );

        // Invalid ASCII file.
        check_error(
            file_transform_processor("error_unknown_format.txt"),
            "error_unknown_format.txt' could not be loaded",
        );

        // Unsupported file extension. It's in fact a binary jpg file i.e.
        // all readers must fail.
        check_error(
            file_transform_processor("rgb-cmy.jpg"),
            "rgb-cmy.jpg' could not be loaded",
        );

        // Missing file.
        let e = file_transform_processor("missing.file").unwrap_err();
        assert!(
            e.message().contains("missing.file' could not be located"),
            "{}",
            e.message()
        );
        assert!(matches!(e, Error::MissingFile(_)));
    }

    #[test]
    fn load_file_fail_message() {
        let path = test_file("legacy_slog_to_log_v3_lustre.lut");
        let e = get_cached_file(&path, Interpolation::Default)
            .err()
            .unwrap();
        let msg = e.message();
        assert!(msg.starts_with(&format!(
            "The specified transform file '{path}' could not be loaded.\nAll formats have been tried. (Enable debug log for errors from all formats.) The formats for the file's extension gave the errors:\n\n    'Discreet 1D LUT' failed with: "
        )), "{msg}");
        assert!(msg.contains("    'houdini' failed with: "), "{msg}");
    }

    #[test]
    fn load_file_fail_clf() {
        // Supported file extension with a wrong content. It's in fact a
        // binary png file i.e. all readers must fail.
        check_error(
            file_transform_processor("clf/illegal/image_png.clf"),
            "image_png.clf' could not be loaded",
        );
    }

    #[test]
    fn load_file_ok_read() {
        // The file readers owned by this module.
        for name in [
            "logtolin_8to8.lut",
            "houdini.lut",
            "discreet-3d-lut.3dl",
            "crosstalk.3dl",
            "lustre_33x33x33.3dl",
        ] {
            let (_, cached) = get_cached_file(&test_file(name), Interpolation::Default).unwrap();
            assert!(cached.group.num_transforms() > 0, "{name}");
        }

        // The format is selected from the extension.
        let (format, _) =
            get_cached_file(&test_file("logtolin_8to8.lut"), Interpolation::Default).unwrap();
        assert_eq!(format.name(), "Discreet 1D LUT");
        let (format, _) =
            get_cached_file(&test_file("houdini.lut"), Interpolation::Default).unwrap();
        assert_eq!(format.name(), "houdini");
        let (format, _) =
            get_cached_file(&test_file("iridas_3d.cube"), Interpolation::Default).unwrap();
        assert_eq!(format.name(), "iridas_cube");
        let (format, _) =
            get_cached_file(&test_file("resolve_1d3d.cube"), Interpolation::Default).unwrap();
        assert_eq!(format.name(), "resolve_cube");
    }

    #[test]
    fn load_file_ok() {
        for name in [
            "logtolin_8to8.lut",
            "houdini.lut",
            "discreet-3d-lut.3dl",
            "crosstalk.3dl",
            "lustre_33x33x33.3dl",
            "matrix_example4x4.ctf",
            "clf/range.clf",
            "clf/pre-smpte_only/matrix_example.clf",
            "clf/cdl_clamp_fwd.clf",
            "clf/lut1d_example.clf",
            "clf/lut3d_identity_12i_16f.clf",
            "fixed_function.ctf",
            "gamma_test1.ctf",
            "log_logtolin.ctf",
            "lut1d_inv.ctf",
            "lut3d_example_Inv.ctf",
        ] {
            let proc = file_transform_processor(name).unwrap();
            assert!(!proc.is_no_op(), "{name}");
        }
    }

    #[test]
    fn all_formats() {
        let registry = FormatRegistry::instance();
        assert_eq!(registry.formats().len(), 19);
        assert_eq!(registry.num_formats(capability::READ), 24);
        assert_eq!(registry.num_formats(capability::BAKE), 12);
        assert_eq!(registry.num_formats(capability::WRITE), 5);

        let name_found_by_extension = |ext: &str, name: &str| {
            registry
                .formats_for_extension(ext)
                .iter()
                .any(|f| f.name() == name)
        };
        assert!(name_found_by_extension("3dl", "flame"));
        assert!(name_found_by_extension("cc", "ColorCorrection"));
        assert!(name_found_by_extension("ccc", "ColorCorrectionCollection"));
        assert!(name_found_by_extension("cdl", "ColorDecisionList"));
        assert!(name_found_by_extension("clf", FILEFORMAT_CLF));
        assert!(name_found_by_extension("csp", "cinespace"));
        assert!(name_found_by_extension("cub", "truelight"));
        assert!(name_found_by_extension("cube", "iridas_cube"));
        assert!(name_found_by_extension("cube", "resolve_cube"));
        assert!(name_found_by_extension("itx", "iridas_itx"));
        assert!(name_found_by_extension(
            "icc",
            "International Color Consortium profile"
        ));
        assert!(name_found_by_extension("look", "iridas_look"));
        assert!(name_found_by_extension("lut", "houdini"));
        assert!(name_found_by_extension("lut", "Discreet 1D LUT"));
        assert!(name_found_by_extension("mga", "pandora_mga"));
        assert!(name_found_by_extension("spi1d", "spi1d"));
        assert!(name_found_by_extension("spi3d", "spi3d"));
        assert!(name_found_by_extension("spimtx", "spimtx"));
        assert!(name_found_by_extension("vf", "nukevf"));
        // When a format handles 2 "formats" it declares both names but only
        // exposes one name using the name() function.
        assert!(!name_found_by_extension("3dl", "lustre"));
        assert!(!name_found_by_extension("m3d", "pandora_m3d"));
        assert!(!name_found_by_extension("icm", "Image Color Matching"));
        assert!(!name_found_by_extension(
            "ctf",
            crate::fileformats::FILEFORMAT_CTF
        ));

        let extension_found_by_name = |ext: &str, name: &str| {
            registry
                .format_by_name(name)
                .map(|f| f.format_info().iter().any(|i| i.extension == ext))
                .unwrap_or(false)
        };
        assert!(extension_found_by_name("3dl", "flame"));
        assert!(extension_found_by_name("3dl", "lustre"));
        assert!(extension_found_by_name(
            "ctf",
            crate::fileformats::FILEFORMAT_CTF
        ));
        assert!(extension_found_by_name("cube", "iridas_cube"));
        assert!(extension_found_by_name("cube", "resolve_cube"));
        assert!(extension_found_by_name(
            "icm",
            "International Color Consortium profile"
        ));
        assert!(extension_found_by_name("m3d", "pandora_m3d"));
        assert!(extension_found_by_name("mga", "pandora_mga"));
        assert!(extension_found_by_name("vf", "nukevf"));
    }

    #[test]
    fn format_by_index() {
        let num = FileTransform::num_formats();
        assert_eq!(num, 24);
        assert_eq!(FileTransform::format_name_by_index(num), "");
        assert_eq!(FileTransform::format_extension_by_index(num), "");
        for i in 0..num {
            assert_ne!(FileTransform::format_name_by_index(i), "");
            assert_ne!(FileTransform::format_extension_by_index(i), "");
        }
    }

    #[test]
    fn is_format_extension_supported() {
        assert!(!FileTransform::is_format_extension_supported("foo"));
        assert!(!FileTransform::is_format_extension_supported("bar"));
        assert!(!FileTransform::is_format_extension_supported("."));
        assert!(!FileTransform::is_format_extension_supported(""));
        assert!(FileTransform::is_format_extension_supported("cdl"));
        assert!(FileTransform::is_format_extension_supported(".cdl"));
        assert!(FileTransform::is_format_extension_supported("Cdl"));
        assert!(FileTransform::is_format_extension_supported(".Cdl"));
        assert!(FileTransform::is_format_extension_supported("3dl"));
        assert!(FileTransform::is_format_extension_supported(".3dl"));
    }

    #[test]
    fn splitext() {
        assert_eq!(splitext_extension("/a/b/c.lut"), ".lut");
        assert_eq!(splitext_extension("/a/b.d/c"), "");
        assert_eq!(splitext_extension("/a/b/.bashrc"), "");
        assert_eq!(splitext_extension("/a/b/..x.cube"), ".cube");
        assert_eq!(splitext_extension("c.tar.gz"), ".gz");
        assert_eq!(splitext_extension("noext"), "");
    }

    #[test]
    fn interpolation_validity() {
        let mut ctx = Context::new();
        ctx.set_search_path(&test_files_dir());
        let config = Config::create_raw();
        let mut tr = FileTransform::new("lut1d_1.spi1d");
        assert!(tr.validate().is_ok());
        let build = |tr: &FileTransform| {
            crate::processor::Processor::from_transform(
                &config,
                &ctx,
                &Transform::File(tr.clone()),
                TransformDirection::Forward,
            )
        };
        // File transform with format requiring a valid interpolation using
        // default interpolation.
        assert!(build(&tr).is_ok());
        // UNKNOWN can't be used by a LUT file, so the interpolation of the
        // LUT is set to DEFAULT.
        tr.interpolation = Interpolation::Unknown;
        assert!(tr.validate().is_ok());
        assert!(build(&tr).is_ok());
        // TETRAHEDRAL can't be used for Spi1d, default is used instead.
        tr.interpolation = Interpolation::Tetrahedral;
        assert!(build(&tr).is_ok());
        // Matrices ignore interpolation.
        tr.interpolation = Interpolation::Unknown;
        tr.src = "camera_to_aces.spimtx".to_string();
        assert!(build(&tr).is_ok());
    }

    #[test]
    fn interpolation_on_luts() {
        // UNKNOWN / TETRAHEDRAL can't be used by a 1D LUT: DEFAULT is used.
        let path = test_file("lut1d_1.spi1d");
        for (interp, expected) in [
            (Interpolation::Default, Interpolation::Default),
            (Interpolation::Linear, Interpolation::Linear),
            (Interpolation::Unknown, Interpolation::Default),
            (Interpolation::Tetrahedral, Interpolation::Default),
        ] {
            let (_, cached) = get_cached_file(&path, interp).unwrap();
            let Some(Transform::Lut1D(lut)) = cached.group.transforms.last() else {
                panic!()
            };
            assert_eq!(lut.interpolation, expected);
        }
    }

    #[test]
    fn context_variables() {
        let mut used = Context::new();
        let config = Config::create_raw();
        let dir = test_files_dir();

        let mut ctx = Context::new();
        ctx.set_search_path(&dir);

        // Case 1 - The 'filename' contains a context variable.
        ctx.set_string_var("ENV1", Some("exposure_contrast_linear.ctf"));
        let mut file = FileTransform::new("$ENV1");
        assert!(collect_context_variables(&config, &ctx, &file, &mut used));
        assert_eq!(used.num_string_vars(), 1);
        assert_eq!(used.string_var_name_by_index(0), Some("ENV1"));
        assert_eq!(
            used.string_var_by_index(0),
            Some("exposure_contrast_linear.ctf")
        );

        // The 'filename' is *not* anymore a context variable.
        file.src = "exposure_contrast_linear.ctf".to_string();
        let mut used = Context::new();
        assert!(!collect_context_variables(&config, &ctx, &file, &mut used));
        assert_eq!(used.num_string_vars(), 0);

        // Case 2 - The 'search_path' now contains a context variable.
        let mut ctx = Context::new();
        ctx.set_search_path("$PATH1");
        ctx.set_string_var("PATH1", Some(&dir));
        let mut used = Context::new();
        assert!(collect_context_variables(&config, &ctx, &file, &mut used));
        assert_eq!(used.num_string_vars(), 1);
        assert_eq!(used.string_var_name_by_index(0), Some("PATH1"));
        assert_eq!(used.string_var_by_index(0), Some(dir.as_str()));

        // The 'search_path' is *not* anymore a context variable.
        let mut ctx = Context::new();
        ctx.set_search_path(&dir);
        let mut used = Context::new();
        assert!(!collect_context_variables(&config, &ctx, &file, &mut used));
        assert_eq!(used.num_string_vars(), 0);

        // Case 3 - The 'filename' and the 'search_path' now contain a
        // context variable.
        let mut ctx = Context::new();
        ctx.set_search_path("$PATH1");
        ctx.set_string_var("PATH1", Some(&dir));
        ctx.set_string_var("ENV1", Some("exposure_contrast_linear.ctf"));
        file.src = "$ENV1".to_string();
        let mut used = Context::new();
        assert!(collect_context_variables(&config, &ctx, &file, &mut used));
        assert_eq!(used.num_string_vars(), 2);
        let vars: Vec<(String, String)> = used
            .string_vars()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        assert!(vars.contains(&("PATH1".to_string(), dir.clone())));
        assert!(vars.contains(&(
            "ENV1".to_string(),
            "exposure_contrast_linear.ctf".to_string()
        )));

        // Case 4 - The 'cccid' contains context variables.
        let mut ctx = Context::new();
        ctx.set_search_path(&dir);
        ctx.set_string_var("CCPREFIX", Some("cc"));
        ctx.set_string_var("CCNUM", Some("01"));
        let mut file = FileTransform::new("cdl_test1.ccc");
        file.ccc_id = "$CCPREFIX00$CCNUM".to_string();
        let mut used = Context::new();
        assert!(collect_context_variables(&config, &ctx, &file, &mut used));
        assert_eq!(used.num_string_vars(), 2);
        assert_eq!(used.string_var("CCPREFIX"), "cc");
        assert_eq!(used.string_var("CCNUM"), "01");
    }

    #[test]
    fn build_ops_errors() {
        let config = Config::create_raw();
        let ctx = Context::new();
        let mut ops = OpVec::new();
        let e = FileTransform::default()
            .build_ops(&mut ops, &config, &ctx, TransformDirection::Forward)
            .unwrap_err();
        assert_eq!(e.message(), "The transform file has not been specified.");
        assert!(ops.is_empty());

        // Errors while building the ops are wrapped.
        let path = test_file("camera_to_aces.spimtx");
        if let Err(e) = FileTransform::new(&path).build_ops(
            &mut ops,
            &config,
            &ctx,
            TransformDirection::Forward,
        ) {
            assert!(e.message().starts_with(&format!(
                "The transform file: {path} failed while building ops with this error: "
            )));
        }
    }

    #[test]
    fn recursion() {
        let path = test_file("camera_to_aces.spimtx");
        let _guard = LoadingGuard::new(&path);
        let config = Config::create_raw();
        let mut ops = OpVec::new();
        let e = FileTransform::new(&path)
            .build_ops(
                &mut ops,
                &config,
                &Context::new(),
                TransformDirection::Forward,
            )
            .unwrap_err();
        assert_eq!(
            e.message(),
            format!("Reference to: {path} is creating a recursion.")
        );
    }

    #[test]
    fn caching() {
        // Use a temporary file whose content changes: the cache returns the
        // first content until the cache is cleared.
        let dir = std::env::temp_dir().join(format!("ocio_rs_file_cache_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cache_test.spimtx");
        let path_str = path.to_string_lossy().to_string();

        std::fs::write(&path, "1 0 0 0\n0 1 0 0\n0 0 1 0\n").unwrap();
        let (_, first) = get_cached_file(&path_str, Interpolation::Default).unwrap();

        std::fs::write(&path, "2 0 0 0\n0 2 0 0\n0 0 2 0\n").unwrap();
        let (_, second) = get_cached_file(&path_str, Interpolation::Default).unwrap();
        if file_cache_enabled() {
            assert!(Arc::ptr_eq(&first, &second));
        }

        // Another interpolation is another entry.
        let (_, third) = get_cached_file(&path_str, Interpolation::Linear).unwrap();
        let Transform::Matrix(m) = &third.group.transforms[0] else {
            panic!()
        };
        assert_eq!(m.matrix[0], 2.0);

        clear_file_transform_caches();
        let (_, fourth) = get_cached_file(&path_str, Interpolation::Default).unwrap();
        let Transform::Matrix(m) = &fourth.group.transforms[0] else {
            panic!()
        };
        assert_eq!(m.matrix[0], 2.0);

        // Errors are cached too.
        let missing = dir.join("missing.spimtx").to_string_lossy().to_string();
        let e1 = get_cached_file(&missing, Interpolation::Default)
            .err()
            .unwrap();
        let e2 = get_cached_file(&missing, Interpolation::Default)
            .err()
            .unwrap();
        assert_eq!(e1, e2);
        assert!(
            e1.message().contains("could not be opened"),
            "{}",
            e1.message()
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cdl_selection() {
        let mut group = GroupTransform::new();
        for (i, id) in ["cc0001", "", "cc0003"].iter().enumerate() {
            let mut cdl = CdlTransform::new();
            cdl.sat = i as f64;
            if !id.is_empty() {
                cdl.set_id(id);
            }
            group.append(cdl);
        }
        group.metadata = FormatMetadata::default();

        assert_eq!(get_cdl(&group, "").unwrap().sat, 0.0);
        assert_eq!(get_cdl(&group, "cc0003").unwrap().sat, 2.0);
        assert_eq!(get_cdl(&group, "1").unwrap().sat, 1.0);
        let e = get_cdl(&group, "3").unwrap_err();
        assert!(matches!(e, Error::MissingFile(_)));
        assert_eq!(
            e.message(),
            "The specified CDL index 3 is outside the valid range for this file [0,2]"
        );
        let e = get_cdl(&group, "cc0002").unwrap_err();
        assert_eq!(
            e.message(),
            "The specified CDL Id/Index 'cc0002' could not be loaded from the file."
        );
        let e = get_cdl(&GroupTransform::new(), "").unwrap_err();
        assert_eq!(e.message(), "File contains no CDL.");

        // FileTransform selection.
        let registry = FormatRegistry::instance();
        let ccc = registry
            .format_by_name(crate::fileformats::FILEFORMAT_COLOR_CORRECTION_COLLECTION)
            .unwrap();
        let cdl_format = registry
            .format_by_name(FILEFORMAT_COLOR_DECISION_LIST)
            .unwrap();
        let cc = registry
            .format_by_name(FILEFORMAT_COLOR_CORRECTION)
            .unwrap();
        let mut ctx = Context::new();
        ctx.set_string_var("NUM", Some("3"));

        let mut ft = FileTransform::new("file.ccc");
        assert_eq!(select_cdl(ccc, &group, &ft, &ctx).unwrap().sat, 0.0);
        ft.ccc_id = "cc000$NUM".to_string();
        let sel = select_cdl(ccc, &group, &ft, &ctx).unwrap();
        assert_eq!(sel.sat, 2.0);
        assert_eq!(sel.style, CdlStyle::NoClamp);
        ft.cdl_style = CdlStyle::Asc;
        assert_eq!(
            select_cdl(ccc, &group, &ft, &ctx).unwrap().style,
            CdlStyle::Asc
        );
        ft.ccc_id = "2".to_string();
        assert_eq!(select_cdl(ccc, &group, &ft, &ctx).unwrap().sat, 2.0);
        ft.ccc_id = "5".to_string();
        let e = select_cdl(ccc, &group, &ft, &ctx).unwrap_err();
        assert!(matches!(e, Error::MissingFile(_)));
        assert_eq!(
            e.message(),
            "The specified cccindex 5 is outside the valid range for this file [0,2]"
        );
        ft.ccc_id = "unknown".to_string();
        let e = select_cdl(cdl_format, &group, &ft, &ctx).unwrap_err();
        assert_eq!(
            e.message(),
            "You must specify a valid cccid to load from the cdl file (either by name or index). id='unknown' is not found in the file, and is not parsable as an integer index."
        );
        // A .cc file ignores the cccid.
        assert_eq!(select_cdl(cc, &group, &ft, &ctx).unwrap().sat, 0.0);
    }

    #[test]
    fn cdl_from_file_errors() {
        check_error(
            CdlTransform::create_from_file("", ""),
            "Error loading CDL. Source file not specified.",
        );
        check_error(
            CdlTransform::create_group_from_file(""),
            "Error loading CDL. Source file not specified.",
        );
        check_error(
            CdlTransform::create_from_file(&test_file("camera_to_aces.spimtx"), ""),
            "Not a CDL file format.",
        );
    }

    #[test]
    fn cdl_from_file() {
        let cdl = CdlTransform::create_from_file(&test_file("cdl_test1.ccc"), "cc0003").unwrap();
        assert_eq!(cdl.id(), "cc0003");
        let group = CdlTransform::create_group_from_file(&test_file("cdl_test1.ccc")).unwrap();
        assert_eq!(group.num_transforms(), 5);
    }

    #[test]
    fn cc_file_with_different_file_extension() {
        for name in [
            "cdl_test_cc_file_with_extension.cdl",
            "cdl_test_cc_file_with_extension.ccc",
        ] {
            assert!(file_transform_processor(name).is_ok(), "{name}");
        }
    }

    #[test]
    fn load_ops() {
        use crate::ops::noop::MarkerNoOp;
        // iridas_1d.cube: file marker, range matrix and 1D LUT.
        let ops = build_ops_test("iridas_1d.cube", TransformDirection::Forward).unwrap();
        assert_eq!(ops.len(), 3);
        assert!(ops[0].downcast_ref::<MarkerNoOp>().is_some());
        assert_eq!(ops[1].name(), "Matrix");
        assert_eq!(ops[2].name(), "Lut1D");

        // resolve_1d3d.cube in the inverse direction: the order is reversed.
        let ops = build_ops_test("resolve_1d3d.cube", TransformDirection::Inverse).unwrap();
        assert_eq!(ops.len(), 5);
        assert_eq!(ops[1].name(), "Lut3D");
        assert_eq!(ops[2].name(), "Matrix");
        assert_eq!(ops[3].name(), "Lut1D");
        assert_eq!(ops[4].name(), "Matrix");

        // icc-test-3.icm: file marker, 2 matrices and the inverse 1D LUT.
        let ops = build_ops_test("icc-test-3.icm", TransformDirection::Forward).unwrap();
        assert_eq!(ops.len(), 4);
        assert_eq!(ops[1].name(), "Matrix");
        assert_eq!(ops[2].name(), "Matrix");
        assert_eq!(ops[3].name(), "Lut1D");
    }

    #[test]
    fn file_marker() {
        use crate::ops::noop::MarkerNoOp;
        // A file whose transforms can't be built still adds its marker
        // before failing (the ops of the other modules are not available).
        let path = test_file("camera_to_aces.spimtx");
        let mut ops = OpVec::new();
        let _ = FileTransform::new(&path).build_ops(
            &mut ops,
            &Config::create_raw(),
            &Context::new(),
            TransformDirection::Forward,
        );
        assert!(!ops.is_empty());
        let marker = ops[0].downcast_ref::<MarkerNoOp>().unwrap();
        assert_eq!(marker.value, path);
    }
}
