//! Integration tests of `ociocheck`.

mod common;
use common::*;

const EXE: &str = env!("CARGO_BIN_EXE_ociocheck");

#[test]
fn help_and_bad_options() {
    let r = run(EXE, &["--help"]);
    assert_eq!(r.code, 1);
    assert!(r
        .stdout
        .starts_with("ociocheck -- validate an OpenColorIO configuration\n\n"));
    assert!(r
        .stdout
        .contains("    --iconfig %s  Input .ocio configuration file (default: $OCIO)\n"));
    assert!(r.stdout.contains("Ociocheck is useful to validate"));

    let r = run(EXE, &["--bogus"]);
    assert_eq!(r.code, 1);
    assert!(r.stdout.starts_with("Invalid option \"--bogus\"\n"));

    let r = run(EXE, &["--iconfig"]);
    assert_eq!(r.code, 1);
    assert!(r
        .stdout
        .starts_with("Missing parameter 1 from option \"--iconfig\"\n"));
}

#[test]
fn missing_config() {
    let r = run(EXE, &[]);
    assert_eq!(r.code, 1);
    assert!(r.stdout.contains(
        "ERROR: You must specify an input OCIO configuration (either with --iconfig or $OCIO).\n"
    ));

    let r = run(EXE, &["--iconfig", "/does/not/exist.ocio"]);
    assert_eq!(r.code, 1);
    assert!(r.stdout.contains("ERROR: "));
    assert!(r.stdout.contains("exist.ocio"));
}

#[test]
fn valid_config() {
    let dir = temp_dir("ociocheck_valid");
    let config = write_test_config(&dir, false);
    let out = dir.join("out.ocio").to_string_lossy().into_owned();

    let r = run(EXE, &["--iconfig", &config, "--oconfig", &out]);
    assert_eq!(r.code, 0, "{}", r.stdout);
    let s = &r.stdout;
    assert!(s.starts_with(&format!(
        "\nOpenColorIO Library Version: {}\nOpenColorIO Library VersionHex: {}\n\nLoading {}\n",
        ocio::OCIO_VERSION_FULL_STR,
        ocio::OCIO_VERSION_HEX,
        config
    )));
    assert!(s.contains("** General **\nEnvironment: {}\nSearch Path: luts\n"));
    assert!(s.contains("Default Display: sRGB\nDefault View: Raw\n"));
    assert!(s.contains("** (Display, View) pairs **\n(sRGB, Raw)\n(sRGB, Film)\n"));
    assert!(s.contains("** Roles **\n"));
    assert!(s.contains("lin (scene_linear)\n"));
    assert!(s.contains("log (color_timing)\n"));
    assert!(!s.contains("WARNING: NOT DEFINED"));
    assert!(s.contains("** ColorSpaces **\nraw\nlin\nlog\nfilm\n"));
    assert!(s.contains("** Named Transforms **\noffset\n"));
    assert!(s.contains("** Looks **\nno looks defined\n"));
    assert!(s.contains("** Validation **\nValidation: passed\n"));
    assert!(s.contains("** Miscellaneous **\nCacheID: "));
    assert!(s.contains("Archivable: yes\n"));
    assert!(s.contains(&format!("Wrote {out}\n")));
    assert!(s.ends_with("\nTests complete.\n\n"));

    // The written config can be loaded again.
    let c = ocio::Config::create_from_file(&out).unwrap();
    assert_eq!(c.num_color_spaces(), 4);

    // Using $OCIO.
    let r = run_with_ocio(EXE, &[], &config);
    assert_eq!(r.code, 0);
    assert!(r.stdout.contains(&format!("Loading $OCIO {config}\n")));
}

#[test]
fn config_with_errors() {
    let dir = temp_dir("ociocheck_errors");
    let config = write_test_config(&dir, true);

    let r = run(EXE, &["--iconfig", &config]);
    assert_eq!(r.code, 1);
    let s = &r.stdout;
    // The (display, view) pair and the color space fail to load the LUT.
    assert!(s.contains("** (Display, View) pairs **\n(sRGB, Raw)\nERROR: "));
    assert!(s.contains("film -- error\n\t"));
    assert!(s.contains("missing_lut.spi1d"));
    assert!(s.ends_with("\n2 tests failed.\n\n"), "{s}");
}

#[test]
fn warnings() {
    let dir = temp_dir("ociocheck_warnings");
    let text = r#"ocio_profile_version: 2

roles:
  default: raw

displays:
  sRGB:
    - !<View> {name: Raw, colorspace: raw}

colorspaces:
  - !<ColorSpace>
    name: raw
    isdata: true
    categories: [file-io]
    interop_id: not_a_cif_id

  - !<ColorSpace>
    name: lin
"#;
    let path = dir.join("config.ocio");
    std::fs::write(&path, text).unwrap();
    let r = run(EXE, &["--iconfig", path.to_str().unwrap()]);
    let s = &r.stdout;
    assert!(s.contains("WARNING: NOT DEFINED (scene_linear)\n"));
    assert!(s.contains("WARNING: InteropID 'not_a_cif_id' is not valid. It should either be one of the Color Interop Forum standard IDs"));
    assert!(s.contains(
        "\nWARNING: The config has some color spaces where the categories are not set.\n"
    ));
    assert!(s.contains("\nWarnings encountered: 7\n"), "{s}");
}
