//! Regression tests for the differences found by differential testing
//! against the C++ OpenColorIO library. The expected values were produced by
//! the C++ reference implementation (OCIO 2.5+ `main`, scalar CPU path).

// The reference values are kept as printed by OCIO (`%.9g`).
#![allow(clippy::excessive_precision)]

use ocio::*;

fn data_file(name: &str) -> String {
    format!("{}/tests/data/files/{}", env!("CARGO_MANIFEST_DIR"), name)
}

fn file_processor(name: &str, dir: TransformDirection, interp: Interpolation) -> Result<Processor> {
    let config = Config::create_raw();
    let t = FileTransform {
        src: data_file(name),
        interpolation: interp,
        ..Default::default()
    };
    config.get_processor_for_transform(&Transform::File(t), dir)
}

fn transform_names(g: &GroupTransform) -> Vec<&'static str> {
    g.transforms
        .iter()
        .map(|t| match t {
            Transform::Allocation(_) => "Allocation",
            Transform::Builtin(_) => "Builtin",
            Transform::Cdl(_) => "CDL",
            Transform::ColorSpace(_) => "ColorSpace",
            Transform::DisplayView(_) => "DisplayView",
            Transform::Exponent(_) => "Exponent",
            Transform::ExponentWithLinear(_) => "ExponentWithLinear",
            Transform::ExposureContrast(_) => "ExposureContrast",
            Transform::File(_) => "File",
            Transform::FixedFunction(_) => "FixedFunction",
            Transform::GradingHueCurve(_) => "GradingHueCurve",
            Transform::GradingPrimary(_) => "GradingPrimary",
            Transform::GradingRgbCurve(_) => "GradingRGBCurve",
            Transform::GradingTone(_) => "GradingTone",
            Transform::Group(_) => "Group",
            Transform::LogAffine(_) => "LogAffine",
            Transform::LogCamera(_) => "LogCamera",
            Transform::Log(_) => "Log",
            Transform::Look(_) => "Look",
            Transform::Lut1D(_) => "Lut1D",
            Transform::Lut3D(_) => "Lut3D",
            Transform::Matrix(_) => "Matrix",
            Transform::Range(_) => "Range",
        })
        .collect()
}

fn optimized_names(p: &Processor) -> Vec<&'static str> {
    transform_names(
        &p.optimized(OptimizationFlags::DEFAULT)
            .create_group_transform(),
    )
}

fn assert_rgba(cpu: &CpuProcessor, input: [f32; 4], expected: [f32; 4], tol: f32) {
    let mut px = input;
    cpu.apply_rgba(&mut px);
    for c in 0..4 {
        let err = (px[c] - expected[c]).abs() / expected[c].abs().max(1.0);
        assert!(
            err <= tol,
            "input {input:?}: got {px:?}, expected {expected:?} (channel {c})"
        );
    }
}

/// The optimizer replaces identity LUTs before combining ops, as OCIO does:
/// a LUT followed by an identity LUT becomes a LUT followed by a clamp.
#[test]
fn optimizer_replaces_identity_lut_before_combining() {
    let p = file_processor(
        "difftest/lut_then_identity_lut.ctf",
        TransformDirection::Forward,
        Interpolation::Default,
    )
    .unwrap();
    assert_eq!(optimized_names(&p), ["Lut1D", "Range"]);
    // Values from OCIO.
    for flags in [OptimizationFlags::DEFAULT, OptimizationFlags::NONE] {
        let cpu = p.optimized_cpu_processor(flags);
        assert_rgba(
            &cpu,
            [-0.5, 0.0, 0.1, 1.0],
            [0.0, 0.0, 0.049999997, 1.0],
            1e-6,
        );
        assert_rgba(
            &cpu,
            [0.25, 0.5, 0.75, 1.0],
            [0.125, 0.25, 0.625, 1.0],
            1e-6,
        );
        assert_rgba(&cpu, [1.0, 1.5, 2.0, 0.5], [1.0, 1.0, 1.0, 0.5], 1e-6);
        assert_rgba(
            &cpu,
            [0.18, 0.01, 100.0, 1.0],
            [0.0900000036, 0.00499999523, 1.0, 1.0],
            1e-6,
        );
    }
}

/// Like OCIO, a processor keeps the identity ops (e.g. identity matrices)
/// until it is optimized; only the no-op types (file / look markers,
/// allocation no-ops) are removed.
#[test]
fn processor_keeps_identity_ops_until_optimized() {
    let mut g = GroupTransform::new();
    g.append(MatrixTransform::default());
    g.append(ExponentTransform::new([2.0, 2.0, 2.0, 1.0]));
    let config = Config::create_raw();
    let p = config
        .get_processor_for_transform(&Transform::Group(g), TransformDirection::Forward)
        .unwrap();
    assert_eq!(
        transform_names(&p.create_group_transform()),
        ["Matrix", "Exponent"]
    );
    assert_eq!(
        transform_names(
            &p.optimized(OptimizationFlags::NONE)
                .create_group_transform()
        ),
        ["Matrix", "Exponent"]
    );
    assert_eq!(optimized_names(&p), ["Exponent"]);
}

/// A no-op processor must not be shared (through the processor cache) with
/// another processor made of different identity ops.
#[test]
fn processor_cache_does_not_share_different_no_op_processors() {
    let yaml = r#"ocio_profile_version: 1
roles:
  scene_linear: cs

displays:
  disp1:
    - !<View>
      name: view1
      colorspace: cs
      looks: cdl

looks:
  - !<Look>
    name: cdl
    process_space: cs
    transform: !<CDLTransform> {}

colorspaces:
  - !<ColorSpace>
    name: cs
"#;
    let config = Config::create_from_str(yaml).unwrap();
    let p = config.get_processor("cs", "scene_linear").unwrap();
    assert!(p.create_group_transform().transforms.is_empty());

    let p = config
        .get_display_view_processor("scene_linear", "disp1", "view1")
        .unwrap();
    // OCIO: the v1 CDL gives a matrix, a basic gamma of 1 and a matrix.
    assert_eq!(
        transform_names(&p.create_group_transform()),
        ["Matrix", "Exponent", "Matrix"]
    );
    assert!(optimized_names(&p).is_empty());
    // The basic gamma clamps negative values.
    let cpu = p.optimized_cpu_processor(OptimizationFlags::NONE);
    assert_rgba(
        &cpu,
        [-1.0, 0.5, f32::NEG_INFINITY, 1.0],
        [0.0, 0.5, 0.0, 1.0],
        0.0,
    );
}

/// As in OCIO, a format is tried once per format info using the file
/// extension: the 3DL format ("flame" and "lustre") is tried twice.
#[test]
fn file_load_error_lists_formats_per_extension() {
    let e = file_processor(
        "error_truncated_file.3dl",
        TransformDirection::Forward,
        Interpolation::Default,
    )
    .unwrap_err();
    let err = "'flame' failed with: Cannot infer 3D LUT size. 369 element(s) does not \
               correspond to a unform cube edge length. (nearest edge length is 7).";
    let expected = format!(
        "could not be loaded.\nAll formats have been tried. (Enable debug log for errors from \
         all formats.) The formats for the file's extension gave the errors:\n\n    {err}    {err}"
    );
    assert!(e.message().ends_with(&expected), "{}", e.message());
}

/// The matrix renderer uses the evaluation order of the OCIO scalar path.
#[test]
fn matrix_renderer_matches_scalar_evaluation_order() {
    let p = file_processor(
        "matrix_example_1_3_alpha_offsets.ctf",
        TransformDirection::Inverse,
        Interpolation::Default,
    )
    .unwrap();
    // Values from OCIO (exact float results).
    let cases = [
        (
            [0.25, 0.5, 0.75, 1.0],
            [-0.0877165198, -0.0821949095, 0.0160389207, 0.526439071],
        ),
        (
            [-0.5, 0.0, 0.1, 1.0],
            [0.453652084, 0.578843236, 0.851581573, -1.25939536],
        ),
        (
            [0.18, 0.01, 100.0, 1.0],
            [-58.9104233, -75.0311661, -7.64636374, 119.691956],
        ),
    ];
    for flags in [OptimizationFlags::DEFAULT, OptimizationFlags::NONE] {
        let cpu = p.optimized_cpu_processor(flags);
        for (input, expected) in cases {
            let mut px = input;
            cpu.apply_rgba(&mut px);
            assert_eq!(px, expected, "input {input:?}");
        }
    }
}

/// A 1D LUT read from a file with one color component stays a one component
/// LUT: the flattening of the inverse half-domain LUT (which, as in OCIO,
/// only affects the active channels) is used for all the channels and the
/// LUT is written with one component.
#[test]
fn inverse_half_domain_lut_keeps_one_component() {
    let p = file_processor(
        "lut1d_inverse_halfdom_slog_fclut.ctf",
        TransformDirection::Forward,
        Interpolation::Default,
    )
    .unwrap();
    let g = p.create_group_transform();
    let Transform::Lut1D(lut) = &g.transforms[0] else {
        panic!("expected a Lut1D");
    };
    // -infinity entry, flattened (the file has 16384).
    assert_eq!(&lut.values[64512 * 3..64512 * 3 + 3], &[0.0, 0.0, 0.0]);
    let ctf = g
        .write(&Config::create_raw(), "Color Transform Format")
        .unwrap();
    // Output of OCIO.
    assert!(ctf.contains(r#"<Array dim="65536 1">"#));
    let lines: Vec<&str> = ctf.lines().collect();
    let first = lines.iter().position(|l| l.contains("<Array")).unwrap() + 1;
    assert_eq!(lines[first].trim(), "17830");
    assert_eq!(lines[first + 64512].trim(), "0");
}

/// OCIO drops the format metadata of the op list when the optimizer replaces
/// an op by simpler ones (`ReplaceOps` rebuilds the list), e.g. a CDL
/// without power becoming matrices.
#[test]
fn optimized_processor_drops_metadata_when_ops_are_replaced() {
    let p = file_processor(
        "clf/cdl_missing_sop.clf",
        TransformDirection::Forward,
        Interpolation::Default,
    )
    .unwrap();
    let config = Config::create_raw();
    let ctf = p
        .create_group_transform()
        .write(&config, "Color Transform Format")
        .unwrap();
    assert!(ctf.contains("<Description>"));
    let opt = p.optimized(OptimizationFlags::DEFAULT);
    let ctf = opt
        .create_group_transform()
        .write(&config, "Color Transform Format")
        .unwrap();
    assert!(ctf.contains(r#" id="urn:uuid:"#), "{ctf}");
    assert!(!ctf.contains("Missing SOP"), "{ctf}");
    // No replacement: the metadata is kept.
    let flags =
        OptimizationFlags(OptimizationFlags::DEFAULT.0 & !OptimizationFlags::SIMPLIFY_OPS.0);
    let ctf = p
        .optimized(flags)
        .create_group_transform()
        .write(&config, "Color Transform Format")
        .unwrap();
    assert!(!ctf.contains("urn:uuid:"), "{ctf}");
}

/// With an integer input, OCIO looks the input code values up in the first
/// 1D LUT (here the fast forward LUT replacing the inverse LUT, with hue
/// adjust) instead of interpolating it.
#[test]
fn integer_input_uses_lut_lookup() {
    let p = file_processor(
        "lut1d_1024_hue_adjust_test.ctf",
        TransformDirection::Inverse,
        Interpolation::Default,
    )
    .unwrap();
    let cpu = p.optimized_cpu_processor_with_bit_depths(
        BitDepth::UInt16,
        BitDepth::UInt16,
        OptimizationFlags::DEFAULT,
    );
    let mut src = [36494u16, 29041, 974, 36494];
    let mut dst = [0u16; 4];
    let si = PackedImageDesc::new(ImageData::U16(&mut src), 1, 1, 4).unwrap();
    let mut di = PackedImageDesc::new(ImageData::U16(&mut dst), 1, 1, 4).unwrap();
    cpu.apply_src_dst(&si, &mut di).unwrap();
    drop(di);
    // Value from OCIO (the interpolation gives 28849 for green).
    assert_eq!(dst, [35168, 28850, 5056, 36494]);

    // The look-up table values are sanitized: +inf becomes FLT_MAX.
    let p = file_processor(
        "lut3by1d_nan_infinity_example.clf",
        TransformDirection::Forward,
        Interpolation::Default,
    )
    .unwrap();
    let cpu = p.optimized_cpu_processor_with_bit_depths(
        BitDepth::UInt8,
        BitDepth::F32,
        OptimizationFlags::DEFAULT,
    );
    let mut src = [1u8, 254, 97, 255];
    let mut dst = [0f32; 4];
    let si = PackedImageDesc::new(ImageData::U8(&mut src), 1, 1, 4).unwrap();
    let mut di = PackedImageDesc::new(ImageData::F32(&mut dst), 1, 1, 4).unwrap();
    cpu.apply_src_dst(&si, &mut di).unwrap();
    drop(di);
    assert_eq!(dst, [0.0, 4.00326056e+36, f32::MAX, 1.0]);
}
