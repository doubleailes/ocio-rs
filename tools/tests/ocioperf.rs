//! Integration tests of `ocioperf`.

mod common;
use common::*;

const EXE: &str = env!("CARGO_BIN_EXE_ocioperf");

#[test]
fn usage() {
    let r = run(EXE, &["--help"]);
    assert_eq!(r.code, 0);
    assert!(r.stdout.starts_with(
        "ocioperf -- apply and measure a color transformation processing\n\n\
         usage: ocioperf [options] --transform /path/to/file.clf\n\n\n"
    ));
    assert!(r.stdout.contains("    --iter %d"));

    let r = run(EXE, &["--test"]);
    assert_eq!(r.code, 1);
    assert!(r
        .stderr
        .starts_with("Missing parameter 1 from option \"--test\"\n"));
}

#[test]
fn measure_transform_file() {
    let lut = data_file("matrix_example4x4.ctf");
    let r = run(
        EXE,
        &[
            "--transform",
            &lut,
            "--iter",
            "1",
            "--test",
            "1",
            "--verbose",
        ],
    );
    assert_eq!(r.code, 0, "{}", r.stderr);
    let s = &r.stdout;
    assert!(s.starts_with(&format!(
        "\nOCIO Version: {}\n\nProcessing using '{lut}'\n\n\n\nProcessing statistics:\n\n",
        ocio::OCIO_VERSION_FULL_STR
    )));
    assert!(s.contains("Create the processor:\t\t\tFor 1 iterations, it took: ["));
    assert!(s.contains("Create the optimized processor:\t\tFor 1 iterations, it took: ["));
    assert!(s.contains("Create the CPU processor:\t\tFor 1 iterations, it took: ["));
    assert!(s.contains("\n\nImage processing statistics:\n\n"));
    assert!(s.contains(
        "Process the complete image (in place) but line by line:\t\tFor 1 iterations, it took: ["
    ));
    assert!(!s.contains("pixel per pixel"));
    assert!(s.ends_with("] ms\n\n\n"));
}

#[test]
fn measure_config_processors() {
    let dir = temp_dir("ocioperf");
    let config = write_test_config(&dir, false);

    // A test type without image processing.
    let r = run(
        EXE,
        &[
            "--iconfig",
            &config,
            "--colorspaces",
            "lin",
            "log",
            "--iter",
            "3",
            "--test",
            "9",
            "--nocache",
            "--bitdepths",
            "ui16",
            "f32",
        ],
    );
    assert_eq!(r.code, 0, "{}", r.stderr);
    let s = &r.stdout;
    assert!(s.contains(&format!("\nLoading {config}\n")));
    assert!(s.contains("Create the config identifier:\t\tFor 3 iterations, it took: ["));
    assert!(s.contains("Create the context identifier:\t\tFor 3 iterations, it took: ["));
    assert!(s.contains("Create the colorspaces processor:\tFor 3 iterations, it took: ["));

    let r = run_with_ocio(
        EXE,
        &[
            "--view",
            "lin",
            "sRGB",
            "Film",
            "--iter",
            "2",
            "--test",
            "9",
            "--verbose",
        ],
        &config,
    );
    assert_eq!(r.code, 0, "{}", r.stderr);
    let s = &r.stdout;
    assert!(s.contains(&format!("\nLoading $OCIO {config}\n")));
    assert!(s.contains("OCIO Config. version: 2.0\nOCIO search_path:     luts\n"));
    assert!(s.contains("Processing from 'lin' to '(sRGB, Film)'\n"));
    assert!(s.contains("Create the (display, view) processor:\tFor 2 iterations, it took: ["));

    let r = run(
        EXE,
        &[
            "--iconfig",
            &config,
            "--invertview",
            "sRGB",
            "Film",
            "lin",
            "--iter",
            "1",
            "--test",
            "9",
        ],
    );
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(r.stdout.contains("Create the colorspaces processor:\t"));
}

#[test]
fn errors() {
    let r = run(EXE, &[]);
    assert_eq!(r.code, 1);
    assert_eq!(
        r.stderr,
        "OCIO ERROR: Missing color transformation description.\n"
    );

    let r = run(EXE, &["--colorspaces", "a", "b"]);
    assert_eq!(r.code, 1);
    assert_eq!(
        r.stderr,
        "OCIO ERROR: You must specify an input OCIO configuration (either with --iconfig or $OCIO).\n\n"
    );

    let dir = temp_dir("ocioperf_errors");
    let config = write_test_config(&dir, false);
    let r = run(
        EXE,
        &[
            "--iconfig",
            &config,
            "--colorspaces",
            "lin",
            "log",
            "--view",
            "lin",
            "sRGB",
            "Film",
            "--iter",
            "1",
        ],
    );
    assert_eq!(r.code, 1);
    assert_eq!(
        r.stderr,
        "OCIO ERROR: Any combinations of --colorspaces, --view or --invertview is invalid.\n"
    );

    let r = run(
        EXE,
        &[
            "--transform",
            &data_file("matrix_example4x4.ctf"),
            "--iter",
            "1",
            "--bitdepths",
            "f32",
            "u8",
        ],
    );
    assert_eq!(r.code, 1);
    assert_eq!(r.stderr, "OCIO ERROR: Unsupported bit-depth: u8\n");

    let r = run(
        EXE,
        &["--transform", &data_file("missing.ctf"), "--iter", "1"],
    );
    assert_eq!(r.code, 1);
    assert!(r.stderr.starts_with("OCIO ERROR: "));
}
