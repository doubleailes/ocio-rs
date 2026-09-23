//! Regression tests for the differences found by differential testing
//! against the C++ OpenColorIO library. The expected values were produced by
//! the C++ reference implementation (OCIO 2.5+ `main`, scalar CPU path).

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
