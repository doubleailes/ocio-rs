// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Copyright (c) 2026 Philippe Llerena.

//! The CPU renderers follow the SSE2 builds of OCIO (the PyOpenColorIO
//! wheels, Nuke, ...): the fast `log2` / `exp2` / `pow` renderers used under
//! `OptimizationFlags::FAST_LOG_EXP_POW` (part of the default flags), and the
//! renderers that always use the SSE2 code. The expected values are the
//! exact results of PyOpenColorIO 2.6.0 (x86-64).

use ocio::*;

fn cpu(t: Transform, flags: OptimizationFlags) -> CpuProcessor {
    Config::create_raw()
        .get_processor_for_transform(&t, TransformDirection::Forward)
        .unwrap()
        .optimized_cpu_processor(flags)
}

/// Apply `t` to `input` and compare the result with the bit patterns of
/// `expected` (any NaN matches a NaN).
fn check(t: &Transform, flags: OptimizationFlags, input: [f32; 4], expected: [u32; 4]) {
    let mut px = input;
    cpu(t.clone(), flags).apply_rgba(&mut px);
    for c in 0..4 {
        let e = f32::from_bits(expected[c]);
        assert!(
            px[c].to_bits() == expected[c] || (px[c].is_nan() && e.is_nan()),
            "{flags:?} {input:?} channel {c}: {} ({:#010x}) != {e} ({:#010x})",
            px[c],
            px[c].to_bits(),
            expected[c],
        );
    }
}

const DEFAULT: OptimizationFlags = OptimizationFlags::DEFAULT;
const LOSSLESS: OptimizationFlags = OptimizationFlags::LOSSLESS;

#[test]
fn default_flags_include_fast_log_exp_pow() {
    assert!(DEFAULT.contains(OptimizationFlags::FAST_LOG_EXP_POW));
    assert!(!LOSSLESS.contains(OptimizationFlags::FAST_LOG_EXP_POW));
}

#[test]
fn gamma() {
    let t = Transform::Exponent(ExponentTransform {
        value: [2.2, 2.2, 2.2, 1.0],
        ..Default::default()
    });
    let input = [0.5, 0.18, 1.0, 1.0];
    // The fast power also processes alpha: 1^1 gives 1.0000116.
    check(
        &t,
        DEFAULT,
        input,
        [0x3e5edd92, 0x3cbc5b54, 0x3f8000bb, 0x3f800061],
    );
    check(
        &t,
        LOSSLESS,
        input,
        [0x3e5edc66, 0x3cbc5c3f, 0x3f800000, 0x3f800000],
    );
}

#[test]
fn moncurve_mirror_inverse() {
    let t = Transform::ExponentWithLinear(ExponentWithLinearTransform {
        direction: TransformDirection::Inverse,
        gamma: [2.4, 2.4, 2.4, 1.0],
        offset: [0.055, 0.055, 0.055, 0.0],
        negative_style: NegativeStyle::Mirror,
        ..Default::default()
    });
    let input = [-0.5, 0.0031, 0.8, 1.0];
    check(
        &t,
        DEFAULT,
        input,
        [0xbf3c406d, 0x3d24132a, 0x3f68057f, 0x3f800000],
    );
    check(
        &t,
        LOSSLESS,
        input,
        [0xbf3c405a, 0x3d24135e, 0x3f68055b, 0x3f800000],
    );
}

#[test]
fn camera_log_to_lin() {
    // S-Log3.
    let t = Transform::LogCamera(LogCameraTransform {
        direction: TransformDirection::Inverse,
        base: 10.0,
        log_side_slope: [261.5 / 1023.0; 3],
        log_side_offset: [420.0 / 1023.0; 3],
        lin_side_slope: [1.0 / (0.18 + 0.01); 3],
        lin_side_offset: [0.01 / (0.18 + 0.01); 3],
        lin_side_break: [0.01125; 3],
        linear_slope: Some([((171.2102946929 - 95.0) / 0.01125) / 1023.0; 3]),
        metadata: Default::default(),
    });
    // The linear segment (first channel) depends on the break point, which
    // the Linux builds compute in double precision (see
    // `get_log_side_break`): it is only checked on Linux.
    let input = [0.0928, 0.5, f32::NAN, 1.0];
    for (flags, expected) in [
        (DEFAULT, [0xb722789f, 0x3ed49d5c, 0x7fc00000, 0x3f800000]),
        (LOSSLESS, [0xb722789f, 0x3ed49d68, 0x7fc00000, 0x3f800000]),
    ] {
        if cfg!(target_os = "linux") {
            check(&t, flags, input, expected);
        } else {
            let mut px = input;
            cpu(t.clone(), flags).apply_rgba(&mut px);
            assert_eq!(px[1].to_bits(), expected[1]);
            assert!(px[2].is_nan());
        }
    }
}

#[test]
fn cdl_no_clamp() {
    let t = Transform::Cdl(CdlTransform {
        style: CdlStyle::NoClamp,
        slope: [1.1, 0.9, 1.2],
        offset: [0.01, -0.02, 0.03],
        power: [1.2, 0.8, 1.05],
        sat: 0.85,
        ..Default::default()
    });
    let input = [0.2, 0.4, 0.6, 1.0];
    check(
        &t,
        DEFAULT,
        input,
        [0x3e515979, 0x3ed5ab8c, 0x3f2fe728, 0x3f800000],
    );
    check(
        &t,
        LOSSLESS,
        input,
        [0x3e51597b, 0x3ed5aba3, 0x3f2fe79a, 0x3f800000],
    );
    // The fast renderer includes `alpha * 0` in the luma sum, so a non finite
    // alpha makes RGB NaN.
    let input = [0.2, 0.4, 0.6, f32::INFINITY];
    check(
        &t,
        DEFAULT,
        input,
        [0xffc00000, 0xffc00000, 0xffc00000, 0x7f800000],
    );
    check(
        &t,
        LOSSLESS,
        input,
        [0x3e51597b, 0x3ed5aba3, 0x3f2fe79a, 0x7f800000],
    );
}

#[test]
fn lin_to_pq() {
    let t = Transform::FixedFunction(FixedFunctionTransform {
        direction: TransformDirection::Forward,
        style: FixedFunctionStyle::LinToPq,
        params: Vec::new(),
        metadata: Default::default(),
    });
    let input = [1.0, -2.0, 100.0, 0.5];
    check(
        &t,
        DEFAULT,
        input,
        [0x3f0213d8, 0xbf1447cd, 0x3f801725, 0x3f000000],
    );
    check(
        &t,
        LOSSLESS,
        input,
        [0x3f021164, 0xbf144227, 0x3f800000, 0x3f000000],
    );
}

#[test]
fn exposure_contrast_always_uses_the_sse_power() {
    let t = Transform::ExposureContrast(ExposureContrastTransform {
        style: ExposureContrastStyle::Linear,
        exposure: 0.5,
        contrast: 1.5,
        gamma: 1.0,
        pivot: 0.18,
        ..Default::default()
    });
    let input = [0.18, 0.5, 2.0, 1.0];
    let expected = [0x3e9afe15, 0x3fb363de, 0x413363de, 0x3f800000];
    check(&t, DEFAULT, input, expected);
    check(&t, LOSSLESS, input, expected);
}

/// A forward 1D LUT (not half domain, no hue adjust) uses OCIO's SSE2
/// `linear1D` renderer when more than one pixel is processed (an image
/// line), and the scalar renderer for a single pixel (`apply_rgba`). The
/// expected values are the results of PyOpenColorIO 2.5.2 forced on its
/// SSE2 path (x86-64 CPU without AVX); its AVX2 / AVX-512 paths use fused
/// multiply-adds and can differ in the last bit.
#[test]
fn lut1d_sse2_renderer_for_several_pixels() {
    let mut lut = Lut1DTransform::default();
    let values = [
        (0.0, 0.0, 0.0),
        (0.1, 0.3, 0.7),
        (0.7, 0.3, 0.2),
        (3e38, 1.0, 0.9),
        (-3e38, 1.1, 1.3),
    ];
    lut.set_length(values.len());
    for (i, (r, g, b)) in values.into_iter().enumerate() {
        lut.set_value(i, r, g, b);
    }
    let cpu = cpu(Transform::Lut1D(lut), DEFAULT);
    // (input, image result, single pixel result)
    let cases: [([u32; 4], [u32; 4], [u32; 4]); 4] = [
        (
            [0x3daf68d5, 0x3e727e74, 0x3f4d2053, 0x3f800000],
            [0x3d0c53de, 0x3e917f13, 0x3f7b66eb, 0x3f800000],
            [0x3d0c53dc, 0x3e917f13, 0x3f7b66ea, 0x3f800000],
        ),
        (
            [0x3dc0c685, 0x3eddc2d0, 0x3ef54636, 0x3f800000],
            [0x3d1a386b, 0x3e99999a, 0x3e77b3f4, 0x3f800000],
            [0x3d1a386a, 0x3e99999a, 0x3e77b3f5, 0x3f800000],
        ),
        (
            [0x3edc7b47, 0x3f16386e, 0x3f3ce2f0, 0x3f800000],
            [0x3f0893ef, 0x3f0b0467, 0x3f5daea0, 0x3f800000],
            [0x3f0893ef, 0x3f0b0468, 0x3f5daea0, 0x3f800000],
        ),
        // Red on an index whose next value overflows `n - p`: the SSE2
        // renderer computes `-Inf * 0`.
        (
            [0x3f400000, 0x3e000000, 0x3f600000, 0x3f000000],
            [0xffc00000, 0x3e19999a, 0x3f8ccccc, 0x3f000000],
            [0x7f61b1e6, 0x3e19999a, 0x3f8ccccc, 0x3f000000],
        ),
    ];
    let bits = |p: &[f32; 4]| p.map(f32::to_bits);
    let mut image: Vec<[f32; 4]> = cases.iter().map(|c| c.0.map(f32::from_bits)).collect();
    let mut data: Vec<f32> = image.iter().flatten().copied().collect();
    let n = image.len();
    cpu.apply(&mut PackedImageDesc::new(ImageData::F32(&mut data), n, 1, 4).unwrap());
    cpu.apply_pixels(&mut image);
    for (i, (input, expected_image, expected_single)) in cases.iter().enumerate() {
        let from_image = [0, 1, 2, 3].map(|c| data[i * 4 + c]);
        assert_eq!(bits(&from_image), *expected_image, "image, pixel {i}");
        assert_eq!(bits(&image[i]), *expected_image, "apply_pixels, pixel {i}");
        let mut single = input.map(f32::from_bits);
        cpu.apply_rgba(&mut single);
        assert_eq!(bits(&single), *expected_single, "apply_rgba, pixel {i}");
    }
}
