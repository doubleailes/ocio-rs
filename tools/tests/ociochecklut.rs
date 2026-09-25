//! Integration tests of `ociochecklut`.

mod common;
use common::*;

const EXE: &str = env!("CARGO_BIN_EXE_ociochecklut");

#[test]
fn usage() {
    let r = run(EXE, &[]);
    assert_eq!(r.code, 1);
    assert!(r.stdout.starts_with(
        "\nociochecklut -- check any LUT file and optionally convert a pixel\n\n\
         usage:  ociochecklut <INPUTFILE> <R G B> or <R G B A>\n\nOptions:\n"
    ));
    assert!(r
        .stdout
        .contains("    -t           Test a set a predefined RGB values\n"));
    assert!(r
        .stdout
        .contains("OCIOCHECKLUT loads any LUT type supported by OCIO"));
    assert!(!r.stdout.contains("Formats supported:"));

    let r = run(EXE, &["--help"]);
    assert_eq!(r.code, 0);
    assert!(r.stdout.contains("Formats supported:\n"));
    assert!(r.stdout.contains("spi1d (.spi1d)\n"));
    assert!(r.stdout.contains("Academy/ASC Common LUT Format (.clf)\n"));

    let r = run(EXE, &["--nope", "x"]);
    assert_eq!(r.code, 1);
    assert!(r.stdout.starts_with("Invalid option \"--nope\"\n"));
}

#[test]
fn print_operators() {
    let r = run(EXE, &[&data_file("lut1d_1.spi1d")]);
    assert_eq!(r.code, 0);
    assert_eq!(
        r.stdout,
        "Transform operators: \n\t<Lut1DTransform direction=forward, fileoutdepth=32f, \
         interpolation=best, inputhalf=0, outputrawhalf=0, hueadjust=0, length=512, \
         minrgb=[0, 0, 0], maxrgb=[1, 1, 1]>\n"
    );

    let r = run(EXE, &["--inv", &data_file("matrix_example4x4.ctf")]);
    assert_eq!(r.code, 0);
    // The processor holds the inverted matrix.
    assert!(r
        .stdout
        .starts_with("Transform operators: \n\t<MatrixTransform direction=forward"));
    assert!(r.stdout.contains("offset=[-0.5294063301453357"));
}

#[test]
fn process_pixels() {
    let lut = data_file("lut1d_green.ctf");

    let r = run(EXE, &[&lut, "0.5", "0.5", "0.5"]);
    assert_eq!(r.code, 0);
    assert_eq!(r.stdout, "\n0 0.5 0\n");

    let r = run(EXE, &[&lut, "0.5", "0.5", "0.5", "0.25"]);
    assert_eq!(r.code, 0);
    assert_eq!(r.stdout, "\n0 0.5 0 0.25\n");

    let r = run(EXE, &["-v", &lut, "0.5", "0.5", "0.5"]);
    assert_eq!(r.code, 0);
    assert_eq!(
        r.stdout,
        format!(
            "\nOCIO Version: {}\n\n\nInput  [R G B]: [0.5 0.5 0.5]\nOutput [R G B]: [  0 0.5   0]\n",
            ocio::OCIO_VERSION_FULL_STR
        )
    );

    // Negative values are not options.
    let r = run(EXE, &[&data_file("lut1d_1.spi1d"), "-0.5", "0.25", "2"]);
    assert_eq!(r.code, 0);
    assert_eq!(r.stdout, "\n0 0.25 1\n");
}

#[test]
fn predefined_values() {
    let r = run(EXE, &["-t", &data_file("lut1d_1.spi1d")]);
    assert_eq!(r.code, 0);
    assert_eq!(
        r.stdout,
        "\n0 0 0\n\n0.18 0.18 0.18\n\n0.5 0.5 0.5\n\n1 1 1\n\n1 1 1\n\n1 1 1\n\n1 0 0\n\n0 1 0\n\n0 0 1\n"
    );

    let r = run(EXE, &["-t", "-v", &data_file("lut1d_1.spi1d")]);
    assert!(r
        .stdout
        .contains("Testing with predefined set of RGB pixels.\n"));
    assert!(r
        .stdout
        .contains("Input  [R G B]: [0.18 0.18 0.18]\nOutput [R G B]: [0.18 0.18 0.18]\n"));
}

#[test]
fn step_info() {
    let r = run(
        EXE,
        &[
            "-s",
            &data_file("clf/lut1d_lut3d_lut1d.clf"),
            "0.5",
            "0.4",
            "0.3",
        ],
    );
    assert_eq!(r.code, 0);
    let steps: Vec<&str> = r.stdout.matches("Output [R G B]: [").collect();
    assert_eq!(steps.len(), 3);
    assert!(r.stdout.contains("\n<Lut3DTransform direction=forward"));
    assert!(r
        .stdout
        .contains("Input  [R G B]: [      0.5       0.4       0.3]\n"));
}

#[test]
fn errors() {
    let r = run(EXE, &[&data_file("does_not_exist.clf")]);
    assert_eq!(r.code, 1);
    assert!(r.stderr.starts_with("ERROR: The specified "));
    assert!(r
        .stderr
        .contains("does_not_exist.clf' could not be located."));

    let r = run(EXE, &[&data_file("lut1d_1.spi1d"), "0.5", "0.5"]);
    assert_eq!(r.code, 1);
    assert_eq!(r.stderr, "ERROR: Expecting either RGB or RGBA pixel.\n");

    let r = run(
        EXE,
        &["-t", &data_file("lut1d_1.spi1d"), "0.5", "0.5", "0.5"],
    );
    assert_eq!(r.code, 1);
    assert_eq!(
        r.stderr,
        "ERROR: Expecting either RGB (or RGBA) pixel or predefined RGB values (i.e. -t).\n"
    );

    let r = run(EXE, &["--gpu", &data_file("lut1d_1.spi1d")]);
    assert_eq!(r.code, 1);
    assert_eq!(
        r.stderr,
        "Compiled without OpenGL support, GPU options are not available.\n"
    );
}
