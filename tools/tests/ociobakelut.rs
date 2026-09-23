//! Integration tests of `ociobakelut`.

mod common;
use common::*;

const EXE: &str = env!("CARGO_BIN_EXE_ociobakelut");
const CLF: &str = "Academy/ASC Common LUT Format";

#[test]
fn usage() {
    let r = run(EXE, &[]);
    assert_eq!(r.code, 1);
    assert!(r.stdout.starts_with(
        "ociobakelut -- create a new LUT or ICC profile from an OCIO config or LUT file(s)\n"
    ));
    assert!(r.stdout.contains("Using Existing OCIO Configurations\n"));
    assert!(r
        .stdout
        .contains("    --inputspace %s      Input OCIO ColorSpace (or Role)\n"));
    assert!(r.stdout.contains("the LUT format to bake: "));
    assert!(r.stdout.contains("Academy/ASC Common LUT Format (.clf)"));
    assert!(r.stdout.contains("ICC Options\n"));

    let r = run(EXE, &["--help"]);
    assert_eq!(r.code, 1);
    assert!(r.stdout.contains("Config-Free LUT Baking\n"));

    let r = run(EXE, &["--cubesize"]);
    assert_eq!(r.code, 1);
    assert!(r
        .stdout
        .starts_with("Missing parameter 1 from option \"--cubesize\"\n"));
}

#[test]
fn bake_luts_to_stdout() {
    let r = run(
        EXE,
        &[
            "--lut",
            &data_file("lut1d_1.spi1d"),
            "--slope",
            "2",
            "1",
            "1",
            "--format",
            CLF,
            "--cubesize",
            "3",
            "--stdout",
        ],
    );
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(r
        .stdout
        .starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"));
    assert!(r.stdout.contains(
        "        <Array dim=\"3 3\">\n          0           0           0\n          \
         1         0.5         0.5\n          2           1           1\n        </Array>\n"
    ));
}

#[test]
fn bake_luts_to_file() {
    let dir = temp_dir("bakelut_file");
    let out = dir.join("out.clf").to_string_lossy().into_owned();
    let r = run(
        EXE,
        &[
            "--v",
            "--lut",
            &data_file("lut1d_1.spi1d"),
            "--offset10",
            "10.23",
            "0",
            "0",
            "--format",
            CLF,
            "--cubesize",
            "2",
            &out,
        ],
    );
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(r.stdout.starts_with(
        "[OpenColorIO DEBUG]: Specified Transform:<GroupTransform direction=forward, transforms=\n"
    ));
    assert!(r.stdout.contains("offset=[0.01, 0, 0]"));
    assert!(r.stdout.contains(&format!(
        "[OpenColorIO INFO]: Baking '{CLF}' LUT\n[OpenColorIO INFO]: Wrote '{out}'\n"
    )));
    let text = std::fs::read_to_string(&out).unwrap();
    assert!(
        text.contains(
            "<Array dim=\"2 3\">\n0.0099999998            0            0\n        \
             1.01            1            1\n"
        ),
        "{text}"
    );

    // The LUT can be read back.
    let t = ocio::Transform::File(ocio::FileTransform::new(&out));
    let p = ocio::Config::create_raw()
        .get_processor_for_transform(&t, ocio::TransformDirection::Forward)
        .unwrap();
    let mut px = [0.5f32, 0.5, 0.5];
    p.default_cpu_processor().apply_rgb(&mut px);
    assert!((px[0] - 0.51).abs() < 1e-5 && (px[1] - 0.5).abs() < 1e-5);
}

#[test]
fn bake_from_config() {
    let dir = temp_dir("bakelut_config");
    let config = write_test_config(&dir, false);
    let out = dir.join("lin_to_log.clf").to_string_lossy().into_owned();
    let r = run(
        EXE,
        &[
            "--iconfig",
            &config,
            "--inputspace",
            "lin",
            "--outputspace",
            "log",
            "--format",
            CLF,
            "--cubesize",
            "3",
            &out,
        ],
    );
    assert_eq!(r.code, 0, "{}", r.stderr);
    let text = std::fs::read_to_string(&out).unwrap();
    // log2(0.5) = -1 and log2(1) = 0.
    assert!(
        text.contains("<Array dim=\"3 1\">\n       -126\n         -1\n          0\n"),
        "{text}"
    );

    // Display / view baking through $OCIO.
    let r = run_with_ocio(
        EXE,
        &[
            "--inputspace",
            "lin",
            "--displayview",
            "sRGB",
            "Film",
            "--format",
            CLF,
            "--cubesize",
            "2",
            "--stdout",
        ],
        &config,
    );
    assert_eq!(r.code, 0, "{}", r.stderr);
    // The green only LUT of the Film view.
    assert!(
        r.stdout.contains(
            "<Array dim=\"2 3\">\n          0           0           0\n          \
             0           1           0\n"
        ),
        "{}",
        r.stdout
    );
}

#[test]
fn errors() {
    let lut = data_file("lut1d_1.spi1d");
    let see = "See --help for more info.\n";

    let r = run(EXE, &["--lut", &lut, "--inputspace", "a", "out.clf"]);
    assert_eq!(r.code, 1);
    assert_eq!(
        r.stderr,
        format!("\nERROR: --inputspace is not allowed when using --lut\n\n{see}")
    );

    let r = run(EXE, &["--lut", &lut, "--displayview", "a", "b", "out.clf"]);
    assert_eq!(
        r.stderr,
        format!("\nERROR: --displayview is not allowed when using --lut\n\n{see}")
    );

    let r = run(EXE, &["--outputspace", "b", "out.clf"]);
    assert_eq!(
        r.stderr,
        format!("\nERROR: You must specify the --inputspace.\n\n{see}")
    );

    let r = run(EXE, &["--inputspace", "a", "out.clf"]);
    assert_eq!(
        r.stderr,
        format!("\nERROR: You must specify either --outputspace or --displayview.\n\n{see}")
    );

    let r = run(EXE, &["--inputspace", "a", "--outputspace", "b", "out.clf"]);
    assert_eq!(
        r.stderr,
        format!("\nERROR: You must specify the LUT format using --format.\n\n{see}")
    );

    let r = run(
        EXE,
        &[
            "--inputspace",
            "a",
            "--outputspace",
            "b",
            "--format",
            CLF,
            "out.clf",
        ],
    );
    assert_eq!(r.code, 1);
    assert!(r.stderr.starts_with(
        "ERROR: You must specify an input OCIO configuration (either with --iconfig or $OCIO).\n\n"
    ));

    let r = run(EXE, &["--lut", &lut, "--format", CLF]);
    assert_eq!(
        r.stderr,
        format!("\nERROR: You must specify the outputfile or --stdout.\n\n{see}")
    );

    let r = run(EXE, &["--lut", &lut, "--format", "icc", "out.icc"]);
    assert_eq!(r.code, 1);
    assert!(r.stderr.contains("ICC profile output is not supported"));

    let r = run(EXE, &["--lut", &lut, "--format", "nope", "--stdout"]);
    assert_eq!(r.code, 1);
    assert!(r.stderr.starts_with("OCIO Error: "));
    assert!(r.stderr.ends_with(see));

    let r = run(
        EXE,
        &["--lut", &lut, "--format", CLF, "/does/not/exist/out.clf"],
    );
    assert_eq!(r.code, 1);
    assert_eq!(
        r.stderr,
        "ERROR: Non-writable file path /does/not/exist/out.clf specified.\n"
    );

    let r = run(EXE, &["--format", CLF, "--stdout", "--lut"]);
    assert_eq!(r.code, 1);
    assert!(r
        .stdout
        .starts_with("Missing parameter 1 from option \"--lut\""));
}
