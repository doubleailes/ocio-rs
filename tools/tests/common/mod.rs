//! Helpers shared by the integration tests of the command line tools.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Directory of the OCIO test files.
pub fn data_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/data/files")
}

/// Path of an OCIO test file.
pub fn data_file(name: &str) -> String {
    data_dir().join(name).to_string_lossy().into_owned()
}

/// A new, empty, temporary directory.
pub fn temp_dir(name: &str) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir =
        std::env::temp_dir().join(format!("ocio_tools_{}_{}_{}", name, std::process::id(), n));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Result of a tool run.
pub struct Run {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl From<Output> for Run {
    fn from(o: Output) -> Self {
        Run {
            code: o.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
        }
    }
}

/// Run a tool with `$OCIO` unset.
pub fn run(exe: &str, args: &[&str]) -> Run {
    Command::new(exe)
        .args(args)
        .env_remove("OCIO")
        .env_remove("OCIO_LOGGING_LEVEL")
        .output()
        .expect("failed to run the tool")
        .into()
}

/// Run a tool with `$OCIO` set to `config`.
pub fn run_with_ocio(exe: &str, args: &[&str], config: &str) -> Run {
    Command::new(exe)
        .args(args)
        .env("OCIO", config)
        .env_remove("OCIO_LOGGING_LEVEL")
        .output()
        .expect("failed to run the tool")
        .into()
}

/// A small test config using a LUT file, written in `dir` (the LUT is copied
/// in `dir/luts`). Returns the config path.
pub fn write_test_config(dir: &Path, with_missing_lut: bool) -> String {
    let luts = dir.join("luts");
    std::fs::create_dir_all(&luts).unwrap();
    std::fs::copy(data_file("lut1d_green.ctf"), luts.join("lut1d_green.ctf")).unwrap();

    let lut = if with_missing_lut {
        "missing_lut.spi1d"
    } else {
        "lut1d_green.ctf"
    };

    let text = format!(
        r#"ocio_profile_version: 2

environment: {{}}
search_path: luts
strictparsing: true
luma: [0.2126, 0.7152, 0.0722]

roles:
  default: raw
  scene_linear: lin
  color_timing: log
  compositing_log: log
  aces_interchange: lin
  cie_xyz_d65_interchange: lin

file_rules:
  - !<Rule> {{name: Default, colorspace: default}}

displays:
  sRGB:
    - !<View> {{name: Raw, colorspace: raw}}
    - !<View> {{name: Film, colorspace: film}}

active_displays: []
active_views: []

colorspaces:
  - !<ColorSpace>
    name: raw
    family: raw
    isdata: true
    categories: [file-io]

  - !<ColorSpace>
    name: lin
    categories: [file-io]

  - !<ColorSpace>
    name: log
    categories: [file-io]
    from_scene_reference: !<LogTransform> {{base: 2}}

  - !<ColorSpace>
    name: film
    categories: [file-io]
    from_scene_reference: !<FileTransform> {{src: {lut}, interpolation: linear}}

named_transforms:
  - !<NamedTransform>
    name: offset
    categories: [file-io]
    transform: !<MatrixTransform> {{offset: [0.1, 0.2, 0.3, 0]}}
"#
    );
    let path = dir.join("config.ocio");
    std::fs::write(&path, text).unwrap();
    path.to_string_lossy().into_owned()
}
