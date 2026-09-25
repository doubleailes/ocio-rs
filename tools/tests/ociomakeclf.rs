//! Integration tests of `ociomakeclf`.

mod common;
use common::*;

const EXE: &str = env!("CARGO_BIN_EXE_ociomakeclf");

#[test]
fn usage_and_list() {
    let r = run(EXE, &["--help"]);
    assert_eq!(r.code, 0);
    assert!(r
        .stdout
        .starts_with("ociomakeclf -- Convert a LUT into CLF format"));
    assert!(r
        .stdout
        .contains("    --csc %s      The color space that the input LUT expects and produces\n"));

    let r = run(EXE, &["--list"]);
    assert_eq!(r.code, 0);
    assert!(r.stdout.starts_with(
        "The list of supported color spaces converting to ACES2065-1, is:\n\tACEScct\n"
    ));
    assert!(r.stdout.contains("\n\tACEScg\n"));
    assert!(r.stdout.ends_with("\n\n"));

    let r = run(EXE, &["a.spi1d"]);
    assert_eq!(r.code, 1);
    assert!(r
        .stderr
        .starts_with("ERROR: Expecting 2 arguments, found 1.\n"));

    let r = run(EXE, &["--bogus"]);
    assert_eq!(r.code, 1);
    assert_eq!(r.stderr, "\nInvalid option \"--bogus\"\n\n");
}

#[test]
fn convert() {
    let dir = temp_dir("ociomakeclf");
    let out = dir.join("out.clf").to_string_lossy().into_owned();
    let lut = data_file("lut1d_green.ctf");

    let r = run(EXE, &[&lut, &out]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    let text = std::fs::read_to_string(&out).unwrap();
    assert!(text.contains(&format!(
        "<Description>Original LUT name: {lut}</Description>"
    )));
    assert!(text.contains("<LUT1D"));
    assert!(!text.contains("<Id>"));

    let r = run(
        EXE,
        &[
            &lut,
            &out,
            "--csc",
            "acescct",
            "--generateid",
            "--verbose",
            "--measure",
        ],
    );
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(r.stdout.starts_with(&format!(
        "OCIO Version: {}\nBuilding the transformation.\n\nCreating the CLF lut file\n  Processing took: ",
        ocio::OCIO_VERSION_FULL_STR
    )));
    let text = std::fs::read_to_string(&out).unwrap();
    assert!(text.contains("<Id>urn:uuid:"));
    assert!(text.contains(
        "<Description>ACES LMT transform built from a look LUT expecting color space: acescct</Description>"
    ));
    assert!(text.contains("<InputDescriptor>ACES2065-1</InputDescriptor>"));
    assert!(text.contains("<OutputDescriptor>ACES2065-1</OutputDescriptor>"));
    assert!(text.contains("<Matrix"));
    assert!(text.contains("<Log"));

    // The result can be read back and is an LMT (ACES2065-1 in & out).
    let t = ocio::Transform::File(ocio::FileTransform::new(&out));
    ocio::Config::create_raw()
        .get_processor_for_transform(&t, ocio::TransformDirection::Forward)
        .unwrap();

    // Inverse LUTs are replaced by fast forward LUTs.
    let inv = dir.join("inv.clf").to_string_lossy().into_owned();
    let r = run(EXE, &[&data_file("lut1d_inv.ctf"), &inv]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    let text = std::fs::read_to_string(&inv).unwrap();
    assert!(!text.contains("InverseLUT1D"));
}

#[test]
fn errors() {
    let dir = temp_dir("ociomakeclf_errors");
    let lut = data_file("lut1d_green.ctf");

    let r = run(EXE, &[&lut, "out.clf", "--csc", "unknown"]);
    assert_eq!(r.code, 1);
    assert_eq!(
        r.stderr,
        "ERROR: The LUT color space name 'unknown' is not supported.\n"
    );

    let r = run(EXE, &[&lut, "out.ctf"]);
    assert_eq!(r.code, 1);
    assert_eq!(
        r.stderr,
        "ERROR: The output LUT file path 'out.ctf' must have a .clf extension.\n"
    );

    let out = dir.join("out.clf").to_string_lossy().into_owned();
    let r = run(EXE, &[&data_file("missing.spi1d"), &out]);
    assert_eq!(r.code, 1);
    assert!(r.stderr.starts_with("OCIO ERROR: "));
    assert!(!std::path::Path::new(&out).exists());
}
