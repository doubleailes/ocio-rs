//! Integration tests of `ociowrite`.

mod common;
use common::*;

const EXE: &str = env!("CARGO_BIN_EXE_ociowrite");

#[test]
fn usage() {
    let r = run(EXE, &[]);
    assert_eq!(r.code, 1);
    assert!(r.stdout.starts_with(
        "ociowrite -- write a color transformation to a file\n\n\
         usage: ociowrite [options] --file outputfile\n\n\n"
    ));
    assert!(r.stdout.contains("Formats to write to:\n"));
    assert!(r.stdout.contains("Color Transform Format (.ctf)\n"));

    let r = run(EXE, &["--h"]);
    assert_eq!(r.code, 1);
    assert!(r.stdout.contains("    --colorspaces %s %s"));

    let r = run(EXE, &["--bogus"]);
    assert_eq!(r.code, 1);
    assert!(r.stderr.starts_with("Invalid option \"--bogus\"\n"));
}

#[test]
fn write_transforms() {
    let dir = temp_dir("ociowrite");
    let config = write_test_config(&dir, false);

    // --colorspaces
    let out = dir.join("lin_to_log.ctf").to_string_lossy().into_owned();
    let r = run_with_ocio(
        EXE,
        &["--v", "--colorspaces", "lin", "log", "--file", &out],
        &config,
    );
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(r
        .stdout
        .contains(&format!("OCIO Configuration: '{config}'\n")));
    assert!(r.stdout.contains("OCIO search_path:    luts\n"));
    assert!(r
        .stdout
        .contains("File format being used: Color Transform Format\n"));
    assert!(r.stdout.contains("Processing from 'lin' to 'log'\n"));
    assert!(r.stdout.contains("Config:  - version: 2\n"));
    let text = std::fs::read_to_string(&out).unwrap();
    assert!(
        text.contains("<Log inBitDepth=\"32f\" outBitDepth=\"32f\" style=\"log2\""),
        "{text}"
    );

    // --view
    let out = dir.join("view.clf").to_string_lossy().into_owned();
    let r = run_with_ocio(
        EXE,
        &["--view", "lin", "sRGB", "Film", "--file", &out],
        &config,
    );
    assert_eq!(r.code, 0, "{}", r.stderr);
    let text = std::fs::read_to_string(&out).unwrap();
    assert!(text.contains("<LUT1D"), "{text}");

    // --invertview
    let out = dir.join("inv.ctf").to_string_lossy().into_owned();
    let r = run_with_ocio(
        EXE,
        &["--invertview", "sRGB", "Film", "lin", "--file", &out],
        &config,
    );
    assert_eq!(r.code, 0, "{}", r.stderr);
    let text = std::fs::read_to_string(&out).unwrap();
    assert!(text.contains("<ProcessList"), "{text}");
}

#[test]
fn errors() {
    let dir = temp_dir("ociowrite_errors");
    let config = write_test_config(&dir, false);

    let r = run(EXE, &["--colorspaces", "lin", "log"]);
    assert_eq!(r.code, 1);
    assert_eq!(r.stderr, "\nThe output transform filepath is missing.\n");

    let r = run(EXE, &["--colorspaces", "lin", "log", "--file", "out.xyz"]);
    assert_eq!(r.code, 1);
    assert!(r
        .stderr
        .starts_with("\nCould not find a valid format from the extension of: 'out.xyz'. \n"));

    let r = run(EXE, &["--file", "out.ctf"]);
    assert_eq!(r.code, 1);
    assert_eq!(
        r.stderr,
        "\nColorspaces or (display,view) pair must be specified as source.\n"
    );

    let r = run(EXE, &["--colorspaces", "lin", "log", "--file", "out.ctf"]);
    assert_eq!(r.code, 1);
    assert_eq!(r.stderr, "\nMissing the ${OCIO} env. variable.\n");

    let r = run_with_ocio(
        EXE,
        &[
            "--colorspaces",
            "lin",
            "log",
            "--view",
            "lin",
            "sRGB",
            "Film",
            "--file",
            "out.ctf",
        ],
        &config,
    );
    assert_eq!(r.code, 1);
    assert_eq!(
        r.stderr,
        "\nAny combinations of --colorspaces, --view or --invertview is invalid.\n"
    );

    let out = dir.join("bad.ctf").to_string_lossy().into_owned();
    let r = run_with_ocio(
        EXE,
        &["--colorspaces", "lin", "unknown", "--file", &out],
        &config,
    );
    assert_eq!(r.code, 1);
    assert!(r.stderr.starts_with("OCIO Error: "));
}
