// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of the OpenColorIO GPU unit tests `tests/gpu/GradingRGBCurveOp_test.cpp`,
//! running the WGSL shaders (see `wgsl_common::gpu_unit`).

mod wgsl_common;

use ocio::gpu::{GpuShaderCreator, GpuShaderDesc};
use ocio::{
    BSplineType, DynamicPropertyType, GradingBSplineCurve, GradingRgbCurve,
    GradingRgbCurveTransform, GradingStyle, GroupTransform, Processor, RgbCurveType, SharedValue,
    Transform, TransformDirection,
};
use wgsl_common::gpu_unit::{gpu_test, GpuTest};

/// Port of `GradingBSplineCurve::Create` (B-spline type).
fn curve(points: &[(f32, f32)]) -> GradingBSplineCurve {
    GradingBSplineCurve::new(points, BSplineType::BSpline)
}

fn rgb_curve_transform(
    style: GradingStyle,
    curves: GradingRgbCurve,
    dir: TransformDirection,
    dynamic: bool,
) -> GradingRgbCurveTransform {
    let mut gc = GradingRgbCurveTransform::new(style);
    gc.value = curves;
    gc.direction = dir;
    if dynamic {
        gc.dynamic = true;
    }
    gc
}

/// The log curves of `GradingRGBCurveLog` (also used by the retests).
fn log_curves() -> GradingRgbCurve {
    let r = curve(&[
        (0.0, 0.0),
        (0.785, 0.231),
        (0.809, 0.631),
        (0.948, 0.704),
        (1.0, 1.0),
    ]);
    let g = curve(&[(0.1, 0.15), (0.55, 0.35), (0.9, 1.1)]);
    let b = curve(&[(-6., -8.), (-2., -5.), (2., 4.), (5., 6.)]);
    let m = curve(&[(-0.1, 0.1), (1.1, 1.3)]);
    GradingRgbCurve::from_curves(r, g, b, m)
}

fn grading_rgb_curve_log(test: &mut GpuTest, dir: TransformDirection, dynamic: bool) {
    let gc = rgb_curve_transform(GradingStyle::Log, log_curves(), dir, dynamic);
    test.set_processor_transform(Transform::GradingRgbCurve(gc));

    test.set_error_threshold(2e-5);
    test.set_expected_minimal_value(1.0);
    test.set_relative_comparison(true);
    test.set_test_wide_range(true);
    test.set_test_infinity(true);
    test.set_test_nan(true);
}

#[test]
fn style_log_fwd() {
    gpu_test("GradingRGBCurve/style_log_fwd", |t| {
        grading_rgb_curve_log(t, TransformDirection::Forward, false)
    });
}

#[test]
fn style_log_fwd_dynamic() {
    gpu_test("GradingRGBCurve/style_log_fwd_dynamic", |t| {
        grading_rgb_curve_log(t, TransformDirection::Forward, true)
    });
}

#[test]
fn style_log_rev() {
    gpu_test("GradingRGBCurve/style_log_rev", |t| {
        grading_rgb_curve_log(t, TransformDirection::Inverse, false)
    });
}

#[test]
fn style_log_rev_dynamic() {
    gpu_test("GradingRGBCurve/style_log_rev_dynamic", |t| {
        grading_rgb_curve_log(t, TransformDirection::Inverse, true)
    });
}

fn grading_rgb_curve_lin(test: &mut GpuTest, dir: TransformDirection, dynamic: bool) {
    let r = curve(&[
        (0.0, 0.0),
        (0.785, 0.231),
        (0.809, 0.631),
        (0.948, 0.704),
        (1.0, 1.0),
    ]);
    let g = curve(&[(0.1, 0.15), (0.55, 0.35), (0.9, 0.8)]);
    let b = curve(&[(-6., -4.), (-2., -1.), (2., 2.), (5., 4.)]);
    let m = curve(&[(-0.1, 0.1), (1.1, 0.9)]);
    let curves = GradingRgbCurve::from_curves(r, g, b, m);

    let gc = rgb_curve_transform(GradingStyle::Lin, curves, dir, dynamic);
    test.set_processor_transform(Transform::GradingRgbCurve(gc));

    test.set_error_threshold(1.5e-4);
    test.set_expected_minimal_value(1.0);
    test.set_relative_comparison(true);
    test.set_test_wide_range(true);
    test.set_test_infinity(false);
    test.set_test_nan(true);
}

#[test]
fn style_lin_fwd() {
    gpu_test("GradingRGBCurve/style_lin_fwd", |t| {
        grading_rgb_curve_lin(t, TransformDirection::Forward, false)
    });
}

#[test]
fn style_lin_fwd_dynamic() {
    gpu_test("GradingRGBCurve/style_lin_fwd_dynamic", |t| {
        grading_rgb_curve_lin(t, TransformDirection::Forward, true)
    });
}

#[test]
fn style_lin_rev() {
    gpu_test("GradingRGBCurve/style_lin_rev", |t| {
        grading_rgb_curve_lin(t, TransformDirection::Inverse, false)
    });
}

#[test]
fn style_lin_rev_dynamic() {
    gpu_test("GradingRGBCurve/style_lin_rev_dynamic", |t| {
        grading_rgb_curve_lin(t, TransformDirection::Inverse, true)
    });
}

// The S-curve values are the upstream float literals, kept verbatim.
#[allow(clippy::excessive_precision)]
fn grading_rgb_s_curve(test: &mut GpuTest, dir: TransformDirection, dynamic: bool) {
    // Create an S-curve with 0 slope at each end.
    let mut s_curve = curve(&[
        (-5.26017743, -4.),
        (-3.75502745, -3.57868829),
        (-2.24987747, -1.82131329),
        (-0.74472749, 0.68124124),
        (1.06145248, 2.87457742),
        (2.86763245, 3.83406206),
        (4.67381243, 4.),
    ]);
    let slopes: [f32; 7] = [0., 0.55982688, 1.77532247, 1.55, 0.8787017, 0.18374463, 0.];
    for (i, &slope) in slopes.iter().enumerate() {
        s_curve.set_slope(i, slope);
    }

    let m = s_curve;
    // Adjust scaling to ensure the test vector for the inverse hits the flat areas.
    let z = curve(&[(-5., 0.), (5., 1.)]);
    let curves = GradingRgbCurve::from_curves(m.clone(), m.clone(), m, z);

    let gc = rgb_curve_transform(GradingStyle::Log, curves, dir, dynamic);
    test.set_processor_transform(Transform::GradingRgbCurve(gc));

    test.set_error_threshold(1.5e-4);
    test.set_expected_minimal_value(1.0);
    test.set_relative_comparison(true);
    test.set_test_wide_range(true);
    test.set_test_infinity(false);
    test.set_test_nan(true);
}

#[test]
fn scurve_fwd() {
    gpu_test("GradingRGBCurve/scurve_fwd", |t| {
        grading_rgb_s_curve(t, TransformDirection::Forward, false)
    });
}

#[test]
fn scurve_fwd_dynamic() {
    gpu_test("GradingRGBCurve/scurve_fwd_dynamic", |t| {
        grading_rgb_s_curve(t, TransformDirection::Forward, true)
    });
}

#[test]
fn scurve_rev() {
    gpu_test("GradingRGBCurve/scurve_rev", |t| {
        grading_rgb_s_curve(t, TransformDirection::Inverse, false)
    });
}

#[test]
fn scurve_rev_dynamic() {
    gpu_test("GradingRGBCurve/scurve_rev_dynamic", |t| {
        grading_rgb_s_curve(t, TransformDirection::Inverse, true)
    });
}

/// Port of upstream `GCRetest`: the harness creates a new CPU processor for
/// each retest (but keeps the shader). Dynamic properties are decoupled
/// between the processor and the shader, so each retest updates both: the
/// processor property (for the CPU) and the shader description property (for
/// the GPU).
fn update_dynamic_rgb_curve(
    processor: &Processor,
    shader_desc: &GpuShaderDesc,
    update: impl FnOnce(&mut GradingRgbCurve),
) {
    let dyn_prop = processor
        .dynamic_property(DynamicPropertyType::GradingRgbCurve)
        .expect("processor grading RGB curve dynamic property");
    let dyn_prop: &SharedValue<GradingRgbCurve> = dyn_prop.as_grading_rgb_curve().unwrap();
    let dyn_prop_gpu = shader_desc
        .dynamic_property(DynamicPropertyType::GradingRgbCurve)
        .expect("shader grading RGB curve dynamic property");
    let dyn_prop_gpu = dyn_prop_gpu.as_grading_rgb_curve().unwrap();

    let mut vals = dyn_prop.get();
    update(&mut vals);
    dyn_prop.set(vals.clone());
    dyn_prop_gpu.set(vals);
}

/// The retests of upstream `MyGPRetest` (`retest1` to `retest4`).
fn add_rgb_curve_retests(test: &mut GpuTest) {
    // retest1
    test.add_retest(|p, d| {
        update_dynamic_rgb_curve(p, d, |vals| {
            vals.curve_mut(RgbCurveType::Red)
                .control_point_mut(1)
                .unwrap()
                .y += 0.1;
        })
    });
    // retest2
    test.add_retest(|p, d| {
        update_dynamic_rgb_curve(p, d, |vals| {
            vals.curve_mut(RgbCurveType::Green)
                .control_point_mut(1)
                .unwrap()
                .y -= 0.1;
        })
    });
    // retest3
    test.add_retest(|p, d| {
        update_dynamic_rgb_curve(p, d, |vals| {
            let mc = vals.curve_mut(RgbCurveType::Master);
            mc.set_num_control_points(3);
            mc.control_point_mut(1).unwrap().x = 0.2;
            mc.control_point_mut(1).unwrap().y = 0.5;
            mc.control_point_mut(2).unwrap().x = 1.1;
            mc.control_point_mut(2).unwrap().y = 1.3;
        })
    });
    // retest4
    test.add_retest(|p, d| {
        update_dynamic_rgb_curve(p, d, |vals| *vals = GradingRgbCurve::new(GradingStyle::Log))
    });
}

#[test]
fn style_log_dynamic_retests() {
    gpu_test("GradingRGBCurve/style_log_dynamic_retests", |t| {
        let gc = rgb_curve_transform(
            GradingStyle::Log,
            log_curves(),
            TransformDirection::Forward,
            true,
        );
        t.set_processor_transform(Transform::GradingRgbCurve(gc));

        add_rgb_curve_retests(t);

        t.set_error_threshold(5e-5);
        t.set_expected_minimal_value(1.0);
        t.set_relative_comparison(true);
        t.set_test_wide_range(true);
        t.set_test_infinity(true);
        t.set_test_nan(true);
    });
}

#[test]
fn two_transforms_retests() {
    gpu_test("GradingRGBCurve/two_transforms_retests", |t| {
        let gc_dyn = rgb_curve_transform(
            GradingStyle::Log,
            log_curves(),
            TransformDirection::Forward,
            true,
        );

        let c1 = curve(&[(0.0, 0.0), (0.2, 0.2), (0.5, 0.7), (1.0, 1.0)]);
        let c2 = curve(&[(0.0, 0.5), (0.3, 0.7), (0.5, 1.1), (1.0, 1.5)]);
        let c3 = curve(&[
            (0.0, -0.5),
            (0.2, -0.4),
            (0.3, 0.1),
            (0.5, 0.4),
            (0.7, 0.9),
            (1.0, 1.1),
        ]);
        let c4 = curve(&[(-1.0, 0.0), (0.2, 0.2), (0.8, 0.8), (2.0, 1.0)]);
        let curves2 = GradingRgbCurve::from_curves(c1, c2, c3, c4);

        let gc_non_dyn = rgb_curve_transform(
            GradingStyle::Lin,
            curves2,
            TransformDirection::Forward,
            false,
        );

        let mut group = GroupTransform::new();
        group.append(Transform::GradingRgbCurve(gc_dyn));
        group.append(Transform::GradingRgbCurve(gc_non_dyn));
        t.set_processor_transform(Transform::Group(group));

        add_rgb_curve_retests(t);

        t.set_error_threshold(5e-5);
        t.set_expected_minimal_value(1.0);
        t.set_relative_comparison(true);
        t.set_test_wide_range(true);
        t.set_test_infinity(false);
        t.set_test_nan(true);
    });
}
