//! Integration tests of `ociolutimage`.

mod common;
use common::*;
use ocio_tools::imageio::ImageIO;

const EXE: &str = env!("CARGO_BIN_EXE_ociolutimage");

#[test]
fn usage() {
    let r = run(EXE, &[]);
    assert_eq!(r.code, 1);
    assert!(r
        .stdout
        .starts_with("ociolutimage -- Convert a 3D LUT to or from an image\n\n"));
    assert!(r
        .stdout
        .contains("    --generate            Generate a lattice image\n"));

    let r = run(EXE, &["--cubesize", "8"]);
    assert_eq!(r.code, 1);
    assert_eq!(r.stderr, "Must specify either --generate or --extract.\n");
}

#[test]
fn generate_and_extract() {
    let dir = temp_dir("ociolutimage");
    let image = dir.join("lattice.exr").to_string_lossy().into_owned();
    let lut = dir.join("lut.spi3d").to_string_lossy().into_owned();

    let r = run(EXE, &["--generate", "--cubesize", "4", "--output", &image]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    let img = ImageIO::open(&image).unwrap();
    assert_eq!((img.width(), img.height(), img.num_channels()), (16, 4, 3));
    let data = img.data_f32().unwrap();
    // Red changes the fastest.
    assert_eq!(&data[0..6], &[0.0, 0.0, 0.0, 1.0 / 3.0, 0.0, 0.0]);

    let r = run(
        EXE,
        &[
            "--extract",
            "--cubesize",
            "4",
            "--input",
            &image,
            "--output",
            &lut,
        ],
    );
    assert_eq!(r.code, 0, "{}", r.stderr);
    let text = std::fs::read_to_string(&lut).unwrap();
    assert!(text.starts_with("SPILUT 1.0\n3 3\n4 4 4\n0 0 0 0 0 0\n0 0 1 0 0 0.333333\n"));
    assert!(text.ends_with("3 3 3 1 1 1\n"));
    assert_eq!(text.lines().count(), 3 + 64);

    // The extracted LUT is an identity.
    let t = ocio::Transform::File(ocio::FileTransform::new(&lut));
    let p = ocio::Config::create_raw()
        .get_processor_for_transform(&t, ocio::TransformDirection::Forward)
        .unwrap();
    let mut px = [0.25f32, 0.5, 0.75];
    p.default_cpu_processor().apply_rgb(&mut px);
    assert!((px[0] - 0.25).abs() < 1e-5 && (px[2] - 0.75).abs() < 1e-5);
}

#[test]
fn generate_with_color_conversion() {
    let dir = temp_dir("ociolutimage_cc");
    let config = write_test_config(&dir, false);
    let image = dir.join("lattice.tif").to_string_lossy().into_owned();
    let r = run(
        EXE,
        &[
            "--generate",
            "--cubesize",
            "3",
            "--maxwidth",
            "4",
            "--config",
            &config,
            "--colorconvert",
            "lin",
            "film",
            "--output",
            &image,
        ],
    );
    assert_eq!(r.code, 0, "{}", r.stderr);
    let img = ImageIO::open(&image).unwrap();
    // 27 pixels with a maximum width of 4.
    assert_eq!((img.width(), img.height()), (4, 7));
    let data = img.data_f32().unwrap();
    // The Film LUT only keeps the green channel.
    assert!(data.iter().step_by(3).all(|v| *v == 0.0));

    let r = run(
        EXE,
        &[
            "--generate",
            "--colorconvert",
            "lin",
            "film",
            "--output",
            &image,
        ],
    );
    assert_eq!(r.code, 1);
    assert_eq!(
        r.stderr,
        "Error generating image: You must specify an OCIO configuration (either with --config or $OCIO).\n"
    );
}

#[test]
fn errors() {
    let dir = temp_dir("ociolutimage_errors");
    let image = dir.join("lattice.exr").to_string_lossy().into_owned();
    let r = run(EXE, &["--generate", "--cubesize", "4", "--output", &image]);
    assert_eq!(r.code, 0);

    let r = run(
        EXE,
        &[
            "--extract",
            "--cubesize",
            "4",
            "--input",
            &image,
            "--output",
            "out.cube",
        ],
    );
    assert_eq!(r.code, 1);
    assert_eq!(
        r.stderr,
        "Error extracting LUT: Only .spi3d writing is currently supported. As a work around, \
         please write a .spi3d file, and then use ociobakelut for transcoding.\n"
    );

    let r = run(
        EXE,
        &[
            "--extract",
            "--cubesize",
            "5",
            "--input",
            &image,
            "--output",
            "out.spi3d",
        ],
    );
    assert_eq!(r.code, 1);
    assert_eq!(
        r.stderr,
        "Error extracting LUT: Image does not have expected dimensions. Expected 25x5, Found 16x4\n"
    );

    let r = run(
        EXE,
        &[
            "--generate",
            "--output",
            &dir.join("x.bad").to_string_lossy(),
        ],
    );
    assert_eq!(r.code, 1);
    assert!(r
        .stderr
        .starts_with("Error generating image: Error: Could not write image: "));
}
