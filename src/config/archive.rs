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
    crate::path_utils::normpath_posix(p)
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
    /// Private directory the LUT files were extracted to (see `prepare_context`).
    extracted: Option<std::sync::Arc<ExtractedDir>>,
}

impl OciozArchive {
    /// Open an archive and read its table of contents.
    pub fn open(path: &str) -> Result<Self> {
        let file = std::fs::File::open(path)
            .map_err(|_| Error::msg(format!("Error could not read OCIOZ archive: {path}")))?;
        let mut zip = zip::ZipArchive::new(file).map_err(|_| {
            Error::msg(format!(
                "Could not open {path} in order to get the entries."
            ))
        })?;
        let mut entries = BTreeMap::new();
        for i in 0..zip.len() {
            if let Ok(f) = zip.by_index_raw(i) {
                entries.insert(f.name().to_string(), format!("{}{}", f.name(), f.crc32()));
            }
        }
        Ok(Self {
            path: path.to_string(),
            entries,
            extracted: None,
        })
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
            Error::msg(format!(
                "Could not open {} in order to get the file: {}",
                self.path, filepath
            ))
        })?;
        let mut zip = zip::ZipArchive::new(file).map_err(|_| {
            Error::msg(format!(
                "Could not open {} in order to get the file: {}",
                self.path, filepath
            ))
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
    ///
    /// The directory is created exclusively with a unique name (and, on Unix,
    /// `0700` permissions) so that a directory prepared beforehand in the shared
    /// temporary directory can't be reused or redirect the extraction. It is
    /// removed when the last config referencing the archive is dropped.
    pub(crate) fn prepare_context(&mut self, config: &mut Config) -> Result<()> {
        let has_luts = self.entries.keys().any(|k| {
            !k.ends_with('/')
                && !path_equal(
                    k,
                    &format!("{OCIO_CONFIG_DEFAULT_NAME}{OCIO_CONFIG_DEFAULT_FILE_EXT}"),
                )
        });
        if !has_luts {
            return Ok(());
        }
        let dir = ExtractedDir::create()?;
        let dir_str = dir.0.to_string_lossy().replace('\\', "/");
        extract_ocioz_archive(&self.path, &dir_str)?;
        config.set_working_dir(&dir_str);
        // OCIO looks the LUT files up in the archive case insensitively
        // (CIOPOciozArchive, mz_path_compare_wc): keep that on case-sensitive
        // file systems.
        let files = self
            .entries
            .keys()
            .filter(|k| !k.ends_with('/'))
            .cloned()
            .collect();
        config.context.set_archive_files(&dir_str, files);
        self.extracted = Some(std::sync::Arc::new(dir));
        Ok(())
    }
}

/// A private temporary directory holding the extracted files of an archive,
/// removed when dropped.
#[derive(Debug, PartialEq, Eq)]
struct ExtractedDir(std::path::PathBuf);

impl ExtractedDir {
    fn create() -> Result<Self> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let tmp = std::env::temp_dir();
        for _ in 0..100 {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let seed = format!(
                "{}-{}-{}",
                std::process::id(),
                nanos,
                COUNTER.fetch_add(1, Ordering::Relaxed)
            );
            let dir = tmp.join(format!("ocio-ocioz-{:x}", md5::compute(seed.as_bytes())));
            #[cfg_attr(not(unix), allow(unused_mut))]
            let mut builder = std::fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            // Not recursive: fails if the path already exists (or is a symlink).
            match builder.create(&dir) {
                Ok(()) => return Ok(Self(dir)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e.into()),
            }
        }
        Err(Error::msg(
            "Could not create a temporary directory for the OCIOZ archive.",
        ))
    }
}

impl Drop for ExtractedDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Extract all the files of an OCIOZ archive into `destination`.
///
/// Unlike minizip-ng (used by OCIO's `ExtractOCIOZArchive`), the extraction
/// never writes through a symbolic link found beneath `destination`: an
/// existing symlink on the path of an entry makes the extraction fail, and
/// the files are created afresh (`O_EXCL`) so that a pre-existing link can't
/// redirect a write outside of the destination directory.
pub fn extract_ocioz_archive(archive_path: &str, destination: &str) -> Result<()> {
    let file = std::fs::File::open(archive_path)
        .map_err(|_| Error::msg(format!("Could not open {archive_path} for reading.")))?;
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|_| Error::msg(format!("Could not open {archive_path} for reading.")))?;
    if zip.is_empty() {
        return Err(Error::msg("No files in archive."));
    }
    let could_not_extract = || Error::msg(format!("Could not extract: {archive_path}"));
    let dest = std::path::PathBuf::from(crate::path_utils::normpath(destination));
    // The destination itself may be (or be under) a symlink, e.g. /tmp on macOS:
    // only the paths created beneath it are checked.
    std::fs::create_dir_all(&dest)?;
    let root = dest.canonicalize()?;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).map_err(|_| could_not_extract())?;
        let rel = match f.enclosed_name() {
            Some(p) => p,
            None => return Err(could_not_extract()),
        };
        let is_dir = f.is_dir();
        if !is_dir && f.size() > MAX_ENTRY_SIZE {
            return Err(Error::msg(
                "OCIOZ archive entry size is invalid or exceeds maximum allowed size.",
            ));
        }
        let comps: Vec<_> = rel.components().collect();
        if comps.is_empty() {
            continue;
        }
        let mut out = root.clone();
        for (idx, c) in comps.iter().enumerate() {
            out.push(c);
            let last = idx + 1 == comps.len();
            match std::fs::symlink_metadata(&out) {
                Ok(m) if m.file_type().is_symlink() => {
                    return Err(Error::msg(format!(
                        "Could not extract: {archive_path}. Refusing to write through the \
                         symbolic link '{}'.",
                        out.display()
                    )));
                }
                Ok(m) => {
                    if (!last || is_dir) && !m.is_dir() {
                        return Err(could_not_extract());
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    if !last || is_dir {
                        std::fs::create_dir(&out)?;
                    }
                }
                Err(e) => return Err(e.into()),
            }
        }
        if is_dir {
            continue;
        }
        // Belt and braces: the parent directory must resolve beneath the destination.
        match out.parent().map(|p| p.canonicalize()) {
            Some(Ok(parent)) if parent.starts_with(&root) => {}
            _ => return Err(could_not_extract()),
        }
        let mut buf = Vec::new();
        f.read_to_end(&mut buf).map_err(|_| could_not_extract())?;
        // Replace an existing file rather than writing into it, and create the new
        // one exclusively (O_EXCL does not follow a symlink raced into place).
        if out.exists() {
            std::fs::remove_file(&out)?;
        }
        let mut w = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&out)?;
        w.write_all(&buf)?;
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
                let rel = p
                    .strip_prefix(root)
                    .unwrap_or(&p)
                    .to_string_lossy()
                    .replace('\\', "/");
                let data = std::fs::read(&p)?;
                zip.start_file(rel.as_str(), options).map_err(|_| {
                    Error::msg(format!(
                        "Could not write LUT file {} to in-memory archive.",
                        p.display()
                    ))
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
    let cursor = zip
        .finish()
        .map_err(|e| Error::msg(format!("Could not write the archive: {e}")))?;
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
                || (super::utils::contains_context_variables(path)
                    && (path.starts_with('$') || path.starts_with('%'))))
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

#[cfg(test)]
mod tests {
    use super::*;

    const LUT: &str = "LUT_1D_SIZE 2\n0 0 0\n0.5 0.5 0.5\n";

    const CONFIG: &str = "ocio_profile_version: 2\n\nsearch_path: luts\n\
                          roles:\n  default: raw\n\
                          displays:\n  d:\n    - !<View> {name: v, colorspace: raw}\n\
                          colorspaces:\n  - !<ColorSpace>\n    name: raw\n\
                          \x20 - !<ColorSpace>\n    name: cs\n\
                          \x20   from_scene_reference: !<FileTransform> {src: lut.cube}\n";

    /// A unique scratch directory removed when dropped.
    fn scratch() -> ExtractedDir {
        ExtractedDir::create().unwrap()
    }

    fn write_zip(path: &std::path::Path, entries: &[(&str, &str)]) {
        let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        for (name, data) in entries {
            if name.ends_with('/') {
                zip.add_directory(*name, zip::write::SimpleFileOptions::default())
                    .unwrap();
            } else {
                zip.start_file(*name, zip::write::SimpleFileOptions::default())
                    .unwrap();
                zip.write_all(data.as_bytes()).unwrap();
            }
        }
        zip.finish().unwrap();
    }

    #[test]
    fn extract_creates_the_files() {
        let tmp = scratch();
        let archive = tmp.0.join("a.ocioz");
        write_zip(
            &archive,
            &[
                ("config.ocio", "cfg"),
                ("luts/", ""),
                ("luts/a/lut.cube", LUT),
            ],
        );
        let dest = tmp.0.join("x").join("out");
        extract_ocioz_archive(archive.to_str().unwrap(), dest.to_str().unwrap()).unwrap();
        assert_eq!(
            std::fs::read_to_string(dest.join("config.ocio")).unwrap(),
            "cfg"
        );
        assert_eq!(
            std::fs::read_to_string(dest.join("luts/a/lut.cube")).unwrap(),
            LUT
        );
        // Extracting again replaces the existing files.
        extract_ocioz_archive(archive.to_str().unwrap(), dest.to_str().unwrap()).unwrap();
        assert_eq!(
            std::fs::read_to_string(dest.join("config.ocio")).unwrap(),
            "cfg"
        );
    }

    #[cfg(unix)]
    #[test]
    fn extract_refuses_to_write_through_symlinks() {
        let tmp = scratch();
        let outside = tmp.0.join("outside");
        std::fs::create_dir(&outside).unwrap();
        let archive = tmp.0.join("a.ocioz");
        write_zip(&archive, &[("config.ocio", "evil"), ("luts/lut.cube", LUT)]);

        // A pre-existing symlinked directory beneath the destination.
        let dest = tmp.0.join("dest1");
        std::fs::create_dir(&dest).unwrap();
        std::os::unix::fs::symlink(&outside, dest.join("luts")).unwrap();
        let err =
            extract_ocioz_archive(archive.to_str().unwrap(), dest.to_str().unwrap()).unwrap_err();
        assert!(err.to_string().contains("symbolic link"), "{err}");
        assert!(!outside.join("lut.cube").exists());

        // A pre-existing symlinked file beneath the destination.
        let victim = outside.join("victim.txt");
        std::fs::write(&victim, "original").unwrap();
        let dest = tmp.0.join("dest2");
        std::fs::create_dir(&dest).unwrap();
        std::os::unix::fs::symlink(&victim, dest.join("config.ocio")).unwrap();
        let err =
            extract_ocioz_archive(archive.to_str().unwrap(), dest.to_str().unwrap()).unwrap_err();
        assert!(err.to_string().contains("symbolic link"), "{err}");
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "original");

        // The destination itself may be a symlink.
        let real = tmp.0.join("real");
        std::fs::create_dir(&real).unwrap();
        let link = tmp.0.join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        extract_ocioz_archive(archive.to_str().unwrap(), link.to_str().unwrap()).unwrap();
        assert!(real.join("luts/lut.cube").exists());
    }

    #[test]
    fn archive_lut_lookup_ignores_case() {
        // OCIO's CIOPOciozArchive matches the entries with mz_path_compare_wc(.., 1),
        // i.e. case insensitively, even on case-sensitive file systems.
        let tmp = scratch();
        let archive = tmp.0.join("a.ocioz");
        write_zip(&archive, &[("config.ocio", CONFIG), ("LUTS/LUT.CUBE", LUT)]);
        let config = Config::create_from_file(archive.to_str().unwrap()).unwrap();
        let p = config.get_processor("raw", "cs").unwrap();
        let mut px = [1.0f32, 1.0, 1.0];
        p.default_cpu_processor().apply_rgb(&mut px);
        assert_eq!(px, [0.5, 0.5, 0.5]);

        // Files that are not in the archive are still missing.
        let mut cfg = config.create_editable_copy();
        cfg.set_search_path("other");
        assert!(cfg.get_processor("raw", "cs").is_err());
    }

    #[test]
    fn archive_luts_are_extracted_in_a_private_directory() {
        let tmp = scratch();
        let archive = tmp.0.join("a.ocioz");
        write_zip(&archive, &[("config.ocio", CONFIG), ("luts/lut.cube", LUT)]);
        let config = Config::create_from_file(archive.to_str().unwrap()).unwrap();
        let wd = std::path::PathBuf::from(config.working_dir());
        assert!(wd.join("luts/lut.cube").exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&wd).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o700);
        }
        // Each load uses its own directory.
        let other = Config::create_from_file(archive.to_str().unwrap()).unwrap();
        assert_ne!(other.working_dir(), config.working_dir());
        // The directory is removed with the last config using it.
        let copy = config.clone();
        drop(config);
        assert!(wd.exists());
        drop(copy);
        assert!(!wd.exists());
    }
}
