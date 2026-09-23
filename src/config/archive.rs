//! OCIOZ archives: configs packaged with their LUT files in a zip file (port
//! of `OCIOZArchive.cpp`).
//!
//! Reading an archive gives the config text (`config.ocio`) and the LUT
//! files. As the file transforms locate their files through the context,
//! the LUT files of an archive are extracted into a private temporary
//! directory which becomes the working directory of the config.

use super::utils::compare;
use super::Config;
use crate::error::{Error, Result};
use crate::fileformats::FormatRegistry;
use crate::types::{OCIO_CONFIG_DEFAULT_FILE_EXT, OCIO_CONFIG_DEFAULT_NAME};
use std::collections::BTreeMap;
use std::io::{Read, Write};

/// Maximum size of an archive entry (256 MB).
const MAX_ENTRY_SIZE: u64 = 256 * 1024 * 1024;

fn normalize(p: &str) -> String {
    crate::path_utils::normpath(&p.replace('\\', "/"))
}

fn path_equal(a: &str, b: &str) -> bool {
    compare(&normalize(a), &normalize(b))
}

/// An OCIOZ archive opened for reading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OciozArchive {
    path: String,
    /// Full path of the entries in the archive => hash (path + CRC32).
    entries: BTreeMap<String, String>,
}

impl OciozArchive {
    /// Open an archive and read its table of contents.
    pub fn open(path: &str) -> Result<Self> {
        let file = std::fs::File::open(path)
            .map_err(|_| Error::msg(format!("Error could not read OCIOZ archive: {path}")))?;
        let mut zip = zip::ZipArchive::new(file)
            .map_err(|_| Error::msg(format!("Could not open {path} in order to get the entries.")))?;
        let mut entries = BTreeMap::new();
        for i in 0..zip.len() {
            if let Ok(f) = zip.by_index_raw(i) {
                entries.insert(f.name().to_string(), format!("{}{}", f.name(), f.crc32()));
            }
        }
        Ok(Self { path: path.to_string(), entries })
    }

    /// Path of the archive.
    pub fn archive_path(&self) -> &str {
        &self.path
    }

    /// Names of the entries of the archive.
    pub fn entry_names(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(|s| s.as_str())
    }

    fn read_entry(&self, filepath: &str) -> Result<Vec<u8>> {
        let file = std::fs::File::open(&self.path).map_err(|_| {
            Error::msg(format!("Could not open {} in order to get the file: {}", self.path, filepath))
        })?;
        let mut zip = zip::ZipArchive::new(file).map_err(|_| {
            Error::msg(format!("Could not open {} in order to get the file: {}", self.path, filepath))
        })?;
        for i in 0..zip.len() {
            let mut f = match zip.by_index(i) {
                Ok(f) => f,
                Err(_) => continue,
            };
            if path_equal(filepath, f.name()) {
                if f.size() == 0 || f.size() > MAX_ENTRY_SIZE {
                    return Err(Error::msg(
                        "OCIOZ archive entry size is invalid or exceeds maximum allowed size.",
                    ));
                }
                let mut buf = Vec::with_capacity(f.size() as usize);
                f.read_to_end(&mut buf)?;
                return Ok(buf);
            }
        }
        Ok(Vec::new())
    }

    /// Content of a (LUT) file of the archive, empty if not found.
    pub fn lut_data(&self, filepath: &str) -> Result<Vec<u8>> {
        self.read_entry(filepath)
    }

    /// The config text (`config.ocio`), empty if missing.
    pub fn config_data(&self) -> Result<String> {
        let name = format!("{OCIO_CONFIG_DEFAULT_NAME}{OCIO_CONFIG_DEFAULT_FILE_EXT}");
        let data = self.read_entry(&name)?;
        Ok(String::from_utf8_lossy(&data).into_owned())
    }

    /// Fast hash of a file of the archive (path + CRC32), empty if missing.
    pub fn fast_lut_file_hash(&self, filepath: &str) -> String {
        let mut hash = String::new();
        for (k, v) in &self.entries {
            if path_equal(k, filepath) {
                hash = v.clone();
            }
        }
        hash
    }

    /// Make the LUT files available to the config: they are extracted in a
    /// private temporary directory used as the config working directory.
    pub(crate) fn prepare_context(&self, config: &mut Config) -> Result<()> {
        let has_luts = self
            .entries
            .keys()
            .any(|k| !k.ends_with('/') && !path_equal(k, &format!("{OCIO_CONFIG_DEFAULT_NAME}{OCIO_CONFIG_DEFAULT_FILE_EXT}")));
        if !has_luts {
            return Ok(());
        }
        let meta = std::fs::metadata(&self.path)?;
        let stamp = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let abs = crate::path_utils::absolute(&self.path);
        let id = format!("{:x}", md5::compute(format!("{abs}{stamp}{}", meta.len()).as_bytes()));
        let dir = std::env::temp_dir().join(format!("ocio-ocioz-{id}"));
        let dir_str = dir.to_string_lossy().replace('\\', "/");
        if !dir.join(".complete").exists() {
            extract_ocioz_archive(&self.path, &dir_str)?;
            std::fs::write(dir.join(".complete"), b"")?;
        }
        config.set_working_dir(&dir_str);
        Ok(())
    }
}

/// Extract all the files of an OCIOZ archive into `destination`.
pub fn extract_ocioz_archive(archive_path: &str, destination: &str) -> Result<()> {
    let file = std::fs::File::open(archive_path)
        .map_err(|_| Error::msg(format!("Could not open {archive_path} for reading.")))?;
    let mut zip =
        zip::ZipArchive::new(file).map_err(|_| Error::msg(format!("Could not open {archive_path} for reading.")))?;
    if zip.is_empty() {
        return Err(Error::msg("No files in archive."));
    }
    let dest = std::path::PathBuf::from(crate::path_utils::normpath(destination));
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).map_err(|_| Error::msg(format!("Could not extract: {archive_path}")))?;
        let rel = match f.enclosed_name() {
            Some(p) => p,
            None => return Err(Error::msg(format!("Could not extract: {archive_path}"))),
        };
        let out = dest.join(rel);
        if f.is_dir() {
            std::fs::create_dir_all(&out)?;
            continue;
        }
        if f.size() > MAX_ENTRY_SIZE {
            return Err(Error::msg("OCIOZ archive entry size is invalid or exceeds maximum allowed size."));
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut buf = Vec::new();
        f.read_to_end(&mut buf).map_err(|_| Error::msg(format!("Could not extract: {archive_path}")))?;
        std::fs::write(&out, buf)?;
    }
    Ok(())
}

fn add_supported_files<W: Write + std::io::Seek>(
    zip: &mut zip::ZipWriter<W>,
    dir: &std::path::Path,
    root: &std::path::Path,
    options: zip::write::SimpleFileOptions,
) -> Result<()> {
    let mut entries: Vec<_> = match std::fs::read_dir(dir) {
        Ok(r) => r.filter_map(|e| e.ok()).collect(),
        Err(_) => return Ok(()),
    };
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let p = e.path();
        if p.is_dir() {
            add_supported_files(zip, &p, root, options)?;
        } else {
            let name = e.file_name().to_string_lossy().into_owned();
            let ext = match name.rfind('.') {
                Some(i) if i > 0 => name[i + 1..].to_string(),
                _ => String::new(),
            };
            if !ext.is_empty() && FormatRegistry::instance().is_format_extension_supported(&ext) {
                let rel = p.strip_prefix(root).unwrap_or(&p).to_string_lossy().replace('\\', "/");
                let data = std::fs::read(&p)?;
                zip.start_file(rel.as_str(), options).map_err(|_| {
                    Error::msg(format!("Could not write LUT file {} to in-memory archive.", p.display()))
                })?;
                zip.write_all(&data)?;
            }
        }
    }
    Ok(())
}

/// Archive a config (and the LUT files found under its working directory).
pub fn archive_config(config: &Config, working_dir: &str) -> Result<Vec<u8>> {
    if !config.is_archivable() {
        return Err(Error::msg("Config is not archivable."));
    }
    let text = config.serialize()?;
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .compression_level(Some(9));
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let name = format!("{OCIO_CONFIG_DEFAULT_NAME}{OCIO_CONFIG_DEFAULT_FILE_EXT}");
    zip.start_file(name.as_str(), options)
        .map_err(|_| Error::msg("Could not prepare an entry for writing."))?;
    zip.write_all(text.as_bytes())
        .map_err(|_| Error::msg("Could not write config to in-memory archive."))?;
    let root = std::path::PathBuf::from(working_dir);
    add_supported_files(&mut zip, &root, &root, options)?;
    let cursor = zip.finish().map_err(|e| Error::msg(format!("Could not write the archive: {e}")))?;
    Ok(cursor.into_inner())
}

impl Config {
    /// True if the config can be archived: an absolute working directory,
    /// relative search paths and file transform sources not going above the
    /// working directory.
    pub fn is_archivable(&self) -> bool {
        let wd = self.working_dir();
        if wd.is_empty() || !crate::path_utils::is_absolute(wd) {
            return false;
        }
        let valid = |path: &str| -> bool {
            let norm = crate::path_utils::normpath(path);
            !(crate::path_utils::is_absolute(&norm)
                || norm.starts_with("..")
                || (super::utils::contains_context_variables(path) && (path.starts_with('$') || path.starts_with('%'))))
        };
        for i in 0..self.num_search_paths() {
            if !valid(self.search_path_by_index(i)) {
                return false;
            }
        }
        let mut files = std::collections::BTreeSet::new();
        for t in self.all_internal_transforms() {
            super::get_file_references(&mut files, t);
        }
        files.iter().all(|f| valid(f))
    }

    /// Archive the config into an OCIOZ buffer.
    pub fn archive(&self) -> Result<Vec<u8>> {
        archive_config(self, self.working_dir())
    }

    /// The archive the config was read from, if any.
    pub fn ocioz_archive(&self) -> Option<&OciozArchive> {
        self.archive.as_ref()
    }
}
