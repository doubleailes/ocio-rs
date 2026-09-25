//! Integration tests of `ocioconvert`.

mod common;
use common::*;
use ocio::{BitDepth, ChannelOrdering};
use ocio_tools::imageio::{AttributeValue, ImageIO, PixelData};

const EXE: &str = env!("CARGO_BIN_EXE_ocioconvert");

/// Write a 4x2 float RGBA test image.
fn write_input(path: &str) -> Vec<f32> {
    let mut img = ImageIO::new(4, 2, ChannelOrdering::Rgba, BitDepth::F32).unwrap();
    let values: Vec<f32> = (0..32).map(|i| (i as f32 + 1.0) / 32.0).collect();
    img.data_f32_mut().unwrap().copy_from_slice(&values);
    img.write(path, BitDepth::Unknown).unwrap();
    values
}

#[test]
fn usage() {
    let r = run(EXE, &["--help"]);
    assert_eq!(r.code, 0);
    assert!(r
        .stdout
        .starts_with("ocioconvert -- apply colorspace transform to an image \n\n"));
    assert!(r.stdout.contains("\nOpenImageIO or OpenEXR options:\n"));
    assert!(r.stdout.contains("    --bitdepth %s"));

    let r = run(EXE, &["a", "b"]);
    assert_eq!(r.code, 1);
    assert!(r
        .stderr
        .starts_with("ERROR: Expecting 4 arguments, found 2.\n"));
    assert!(r
        .stdout
        .starts_with("ocioconvert -- apply colorspace transform"));

    let r = run(EXE, &["--lut", "a", "b"]);
    assert_eq!(r.code, 1);
    assert!(r
        .stderr
        .starts_with("ERROR: Expecting 3 arguments for --lut option, found 2.\n"));

    let r = run(EXE, &["--lut", "--view", "a", "b", "c"]);
    assert_eq!(r.code, 1);
    assert!(r
        .stderr
        .starts_with("ERROR: Options lut & view can't be used at the same time.\n"));

    let r = run(EXE, &["--bitdepth", "int7", "a", "b", "c", "d"]);
    assert_eq!(r.code, 1);
    assert!(r.stderr.starts_with(
        "ERROR: Unsupported output bitdepth, must be uint8, uint16, half or float.\n"
    ));

    let r = run(EXE, &["--gpu", "a", "b", "c", "d"]);
    assert_eq!(r.code, 1);
    assert_eq!(
        r.stderr,
        "Compiled without OpenGL support, GPU options are not available.\n"
    );
}

#[test]
fn convert_color_spaces() {
    let dir = temp_dir("ocioconvert_cs");
    let config = write_test_config(&dir, false);
    let input = dir.join("in.exr").to_string_lossy().into_owned();
    let output = dir.join("out.exr").to_string_lossy().into_owned();
    let values = write_input(&input);

    let r = run(
        EXE,
        &[
            "--iconfig",
            &config,
            &input,
            "lin",
            &output,
            "log",
            "--float-attribute",
            "myfloat=0.5",
            "--int-attribute",
            "myint=7",
            "--string-attribute",
            "mystring=hello",
        ],
    );
    assert_eq!(r.code, 0, "{}{}", r.stdout, r.stderr);
    assert_eq!(
        r.stdout,
        format!(
            "\nLoading {input}\n\nImage: [4x2] 32f R, G, B, A\n\nWrote {output}\n\nImage: [4x2] 32f R, G, B, A\n\n"
        )
    );

    let img = ImageIO::open(&output).unwrap();
    assert_eq!(img.bit_depth(), BitDepth::F32);
    let out = img.data_f32().unwrap();
    for (i, (o, v)) in out.iter().zip(values.iter()).enumerate() {
        let expected = if i % 4 == 3 { *v } else { v.log2() };
        assert!((o - expected).abs() < 1e-5, "{i}: {o} vs {expected}");
    }
    let attrs = img.attributes();
    assert!(attrs.contains(&("myfloat".to_string(), AttributeValue::Float(0.5))));
    assert!(attrs.contains(&("myint".to_string(), AttributeValue::Int(7))));
    assert!(attrs.contains(&("mystring".to_string(), AttributeValue::Str("hello".into()))));
    assert!(attrs.contains(&(
        "oiio:ColorSpace".to_string(),
        AttributeValue::Str("log".into())
    )));

    // Using $OCIO, with a half float output.
    let r = run_with_ocio(
        EXE,
        &["-v", "--bitdepth", "half", &input, "lin", &output, "log"],
        &config,
    );
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(r
        .stdout
        .contains(&format!("OCIO Config. file:    '{config}'\n")));
    assert!(r.stdout.contains("OCIO Config. version: 2.0\n"));
    assert!(r.stdout.contains("CPU processing took: "));
    assert!(r.stdout.ends_with("\nImage: [4x2] 16f R, G, B, A\n\n"));
    let img = ImageIO::open(&output).unwrap();
    assert_eq!(img.bit_depth(), BitDepth::F16);
}

#[test]
fn convert_other_modes() {
    let dir = temp_dir("ocioconvert_modes");
    let config = write_test_config(&dir, false);
    let input = dir.join("in.exr").to_string_lossy().into_owned();
    let values = write_input(&input);

    // --lut
    let output = dir.join("lut.tif").to_string_lossy().into_owned();
    let r = run(
        EXE,
        &["--lut", &data_file("lut1d_green.ctf"), &input, &output],
    );
    assert_eq!(r.code, 0, "{}", r.stderr);
    let img = ImageIO::open(&output).unwrap();
    let out = img.data_f32().unwrap();
    assert_eq!(out[0], 0.0);
    assert!((out[1] - values[1]).abs() < 1e-3);
    assert_eq!(out[2], 0.0);

    // --view to a PNG file (8-bit).
    let output = dir.join("view.png").to_string_lossy().into_owned();
    let r = run(
        EXE,
        &[
            "--iconfig",
            &config,
            "--view",
            &input,
            "lin",
            &output,
            "sRGB",
            "Film",
            "--bitdepth",
            "uint8",
        ],
    );
    assert_eq!(r.code, 0, "{}", r.stderr);
    let img = ImageIO::open(&output).unwrap();
    assert_eq!(img.bit_depth(), BitDepth::UInt8);
    assert_eq!(img.num_channels(), 4);
    if let PixelData::U8(v) = img.data() {
        assert_eq!(v[0], 0);
        assert_eq!(v[2], 0);
        assert_eq!(v[3], (values[3] * 255.0 + 0.5) as u8);
    } else {
        panic!("expecting 8-bit data");
    }

    // --invertview
    let output = dir.join("inv.exr").to_string_lossy().into_owned();
    let r = run(
        EXE,
        &[
            "--iconfig",
            &config,
            "--invertview",
            &input,
            "sRGB",
            "Raw",
            &output,
            "lin",
        ],
    );
    assert_eq!(r.code, 0, "{}", r.stderr);

    // --namedtransform & --invnamedtransform
    let output = dir.join("nt.exr").to_string_lossy().into_owned();
    let r = run(
        EXE,
        &[
            "--iconfig",
            &config,
            "--namedtransform",
            "offset",
            &input,
            &output,
        ],
    );
    assert_eq!(r.code, 0, "{}", r.stderr);
    let img = ImageIO::open(&output).unwrap();
    let out = img.data_f32().unwrap();
    assert!((out[0] - (values[0] + 0.1)).abs() < 1e-6);
    assert!((out[1] - (values[1] + 0.2)).abs() < 1e-6);

    let r = run(
        EXE,
        &[
            "--iconfig",
            &config,
            "--invnamedtransform",
            "offset",
            &input,
            &output,
        ],
    );
    assert_eq!(r.code, 0, "{}", r.stderr);
    let img = ImageIO::open(&output).unwrap();
    let out = img.data_f32().unwrap();
    assert!((out[2] - (values[2] - 0.3)).abs() < 1e-6);

    let r = run(
        EXE,
        &[
            "--iconfig",
            &config,
            "--namedtransform",
            "unknown",
            &input,
            &output,
        ],
    );
    assert_eq!(r.code, 1);
    assert_eq!(
        r.stdout.lines().last(),
        Some("ERROR: Could not get NamedTransform unknown")
    );
}

#[test]
fn errors() {
    let dir = temp_dir("ocioconvert_errors");
    let config = write_test_config(&dir, false);
    let input = dir.join("in.exr").to_string_lossy().into_owned();
    write_input(&input);

    let r = run(
        EXE,
        &["--iconfig", "/does/not/exist.ocio", "a", "b", "c", "d"],
    );
    assert_eq!(r.code, 1);
    assert!(r.stdout.starts_with("ERROR loading config file: "));

    let missing = dir.join("missing.exr").to_string_lossy().into_owned();
    let r = run(
        EXE,
        &["--iconfig", &config, &missing, "lin", "out.exr", "log"],
    );
    assert_eq!(r.code, 1);
    assert!(r
        .stderr
        .starts_with("ERROR: Loading file failed: Error: Could not read image: "));

    let out = dir.join("out.exr").to_string_lossy().into_owned();
    let r = run(EXE, &["--iconfig", &config, &input, "lin", &out, "unknown"]);
    assert_eq!(r.code, 1);
    assert!(r.stdout.contains("ERROR: OCIO failed with: "));

    let r = run(
        EXE,
        &[
            "--iconfig",
            &config,
            &input,
            "lin",
            &out,
            "log",
            "--int-attribute",
            "noequal",
        ],
    );
    assert_eq!(r.code, 1);
    assert_eq!(
        r.stderr,
        "ERROR: Attribute string 'noequal' should be in the form name=intvalue.\n"
    );

    let bad = dir.join("out.unknownext").to_string_lossy().into_owned();
    let r = run(EXE, &["--iconfig", &config, &input, "lin", &bad, "log"]);
    assert_eq!(r.code, 1);
    assert_eq!(r.stderr, format!("ERROR: Writing file \"{bad}\".\n"));
}
