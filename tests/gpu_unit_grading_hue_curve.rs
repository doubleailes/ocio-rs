// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Copyright (c) 2026 Philippe Llerena.

//! Port of the OpenColorIO GPU unit tests
//! `tests/gpu/GradingHueCurveOp_test.cpp`, running the WGSL shaders (see
//! `wgsl_common::gpu_unit`).

mod wgsl_common;

use ocio::{
    BSplineType, GradingBSplineCurve, GradingControlPoint, GradingHueCurve,
    GradingHueCurveTransform, GradingStyle, HsyTransformStyle, HueCurveType, Transform,
    TransformDirection,
};
use wgsl_common::gpu_unit::{gpu_test, GpuTest};

fn generate_identity_lut3d(edge_len: usize, min: f32, max: f32) -> Vec<f32> {
    let num_channels = 4;
    let num_samples = edge_len * edge_len * edge_len;
    let mut img = vec![0.0f32; num_samples * num_channels];

    let scale = max - min;
    let c = 1.0f32 / (edge_len as f32 - 1.0f32);
    for i in 0..num_samples {
        img[num_channels * i] = scale * (i % edge_len) as f32 * c + min;
        img[num_channels * i + 1] = scale * ((i / edge_len) % edge_len) as f32 * c + min;
        img[num_channels * i + 2] = scale * ((i / edge_len / edge_len) % edge_len) as f32 * c + min;
    }
    img
}

fn curve(points: &[(f32, f32)], c: HueCurveType) -> GradingBSplineCurve {
    GradingBSplineCurve::new_for_hue_curve(points, c)
}

/// The hue curves of the log and video tests.
fn log_vid_curves() -> GradingHueCurve {
    // All curves are non-identities.
    let hh = curve(
        &[
            (0.05, 0.15),
            (0.2, 0.3),
            (0.35, 0.4),
            (0.45, 0.45),
            (0.6, 0.7),
            (0.8, 0.85),
        ],
        HueCurveType::HueHue,
    );
    let hs = curve(
        &[
            (-0.1, 1.2),
            (0.2, 0.7),
            (0.4, 1.5),
            (0.5, 0.5),
            (0.6, 1.4),
            (0.8, 0.7),
        ],
        HueCurveType::HueSat,
    );
    let hl = curve(
        &[(0.1, 1.5), (0.2, 0.7), (0.4, 1.4), (0.5, 0.8), (0.8, 0.5)],
        HueCurveType::HueLum,
    );
    let ls = curve(&[(0.05, 1.5), (0.5, 0.9), (1.1, 1.4)], HueCurveType::LumSat);
    let ss = curve(&[(0., 0.1), (0.5, 0.45), (1., 1.1)], HueCurveType::SatSat);
    let ll = curve(
        &[(-0.02, -0.04), (0.2, 0.1), (0.8, 0.95), (1.1, 1.2)],
        HueCurveType::LumLum,
    );
    let sl = curve(&[(0., 1.2), (0.6, 0.8), (0.9, 1.1)], HueCurveType::SatLum);
    let hfx = curve(
        &[
            (0.2, 0.05),
            (0.4, -0.09),
            (0.6, -0.2),
            (0.8, 0.05),
            (0.99, -0.02),
        ],
        HueCurveType::HueFx,
    );

    GradingHueCurve::from_curves(hh, hs, hl, ls, ss, ll, sl, hfx)
        .expect("Cannot create GradingHueCurve.")
}

fn set_hue_curve_processor(
    test: &mut GpuTest,
    mut hc: GradingHueCurveTransform,
    dir: TransformDirection,
    dynamic: bool,
) {
    hc.direction = dir;
    if dynamic {
        hc.dynamic = true;
    }

    test.set_processor_transform(Transform::GradingHueCurve(hc));
}

fn set_grid_custom_values(test: &mut GpuTest) {
    // Set up a grid of RGBA custom values.
    let lut_size = 21;
    // Choose values so that there is a grid point at 0.
    test.set_custom_values(generate_identity_lut3d(lut_size, -0.075, 1.425));
}

fn grading_hue_curve_log(test: &mut GpuTest, dir: TransformDirection, dynamic: bool) {
    let mut hc = GradingHueCurveTransform::new(GradingStyle::Log);
    hc.value = log_vid_curves();
    set_hue_curve_processor(test, hc, dir, dynamic);

    set_grid_custom_values(test);

    test.set_error_threshold(2e-5);
    test.set_expected_minimal_value(1.0);
}

#[test]
fn style_log_fwd() {
    gpu_test("GradingHueCurve/style_log_fwd", |t| {
        grading_hue_curve_log(t, TransformDirection::Forward, false)
    });
}

#[test]
fn style_log_fwd_dynamic() {
    gpu_test("GradingHueCurve/style_log_fwd_dynamic", |t| {
        grading_hue_curve_log(t, TransformDirection::Forward, true)
    });
}

#[test]
fn style_log_rev() {
    gpu_test("GradingHueCurve/style_log_rev", |t| {
        grading_hue_curve_log(t, TransformDirection::Inverse, false)
    });
}

#[test]
fn style_log_rev_dynamic() {
    gpu_test("GradingHueCurve/style_log_rev_dynamic", |t| {
        grading_hue_curve_log(t, TransformDirection::Inverse, true)
    });
}

fn grading_hue_curve_vid(test: &mut GpuTest, dir: TransformDirection, dynamic: bool) {
    let mut hc = GradingHueCurveTransform::new(GradingStyle::Video);
    hc.value = log_vid_curves();
    set_hue_curve_processor(test, hc, dir, dynamic);

    set_grid_custom_values(test);

    test.set_error_threshold(2e-5);
    test.set_expected_minimal_value(1.0);
}

#[test]
fn style_vid_fwd() {
    gpu_test("GradingHueCurve/style_vid_fwd", |t| {
        grading_hue_curve_vid(t, TransformDirection::Forward, false)
    });
}

#[test]
fn style_vid_fwd_dynamic() {
    gpu_test("GradingHueCurve/style_vid_fwd_dynamic", |t| {
        grading_hue_curve_vid(t, TransformDirection::Forward, true)
    });
}

#[test]
fn style_vid_rev() {
    gpu_test("GradingHueCurve/style_vid_rev", |t| {
        grading_hue_curve_vid(t, TransformDirection::Inverse, false)
    });
}

#[test]
fn style_vid_rev_dynamic() {
    gpu_test("GradingHueCurve/style_vid_rev_dynamic", |t| {
        grading_hue_curve_vid(t, TransformDirection::Inverse, true)
    });
}

fn hue_curve_lin(test: &mut GpuTest, dir: TransformDirection, dynamic: bool) {
    // All curves are non-identities.
    let hh = curve(
        &[
            (0.05, 0.15),
            (0.2, 0.3),
            (0.35, 0.4),
            (0.45, 0.45),
            (0.6, 0.7),
            (0.8, 0.85),
        ],
        HueCurveType::HueHue,
    );
    let hs = curve(
        &[
            (-0.1, 1.2),
            (0.2, 0.7),
            (0.4, 1.5),
            (0.5, 0.5),
            (0.6, 1.4),
            (0.8, 0.7),
        ],
        HueCurveType::HueSat,
    );
    let hl = curve(
        &[(0.1, 1.5), (0.2, 0.7), (0.4, 1.4), (0.5, 0.8), (0.8, 0.5)],
        HueCurveType::HueLum,
    );
    let ss = curve(&[(0., 0.1), (0.5, 0.45), (1., 1.1)], HueCurveType::SatSat);
    let hfx = curve(
        &[
            (0.2, 0.05),
            (0.4, -0.09),
            (0.6, -0.2),
            (0.8, 0.05),
            (0.99, -0.02),
        ],
        HueCurveType::HueFx,
    );
    // Adjust these two, relative to previous test, to work in f-stops.
    let ll = curve(
        &[(-8., -7.), (-2., -3.), (2., 3.5), (8., 7.)],
        HueCurveType::LumLum,
    );
    let ls = curve(
        &[
            (-6., 0.9),
            (-3., 0.95),
            (0., 1.1),
            (2., 1.),
            (4., 0.6),
            (6., 0.55),
        ],
        HueCurveType::LumSat,
    );
    // Adjusted this one, relative to above, to avoid some artifacts due to high sat boost.
    let sl = curve(
        &[(0., 1.2), (0.6, 0.8), (0.9, 1.05), (1., 1.1)],
        HueCurveType::SatLum,
    );

    let curve = GradingHueCurve::from_curves(hh, hs, hl, ls, ss, ll, sl, hfx)
        .expect("Cannot create GradingHueCurve.");
    let mut hc = GradingHueCurveTransform::new(GradingStyle::Lin);
    hc.value = curve;
    set_hue_curve_processor(test, hc, dir, dynamic);

    set_grid_custom_values(test);

    // This test produces some large linear values due to the sat boost, needs a large tolerance.
    // Metal is worse than GLSL.
    test.set_error_threshold(4e-4);
    test.set_expected_minimal_value(1.0);
    test.set_relative_comparison(true);
}

#[test]
fn style_lin_fwd() {
    gpu_test("GradingHueCurve/style_lin_fwd", |t| {
        hue_curve_lin(t, TransformDirection::Forward, false)
    });
}

#[test]
fn style_lin_fwd_dynamic() {
    gpu_test("GradingHueCurve/style_lin_fwd_dynamic", |t| {
        hue_curve_lin(t, TransformDirection::Forward, true)
    });
}

#[test]
fn style_lin_rev() {
    gpu_test("GradingHueCurve/style_lin_rev", |t| {
        hue_curve_lin(t, TransformDirection::Inverse, false)
    });
}

#[test]
fn style_lin_rev_dynamic() {
    gpu_test("GradingHueCurve/style_lin_rev_dynamic", |t| {
        hue_curve_lin(t, TransformDirection::Inverse, true)
    });
}

fn draw_curve(test: &mut GpuTest, dir: TransformDirection, dynamic: bool) {
    let mut hc = GradingHueCurveTransform::new(GradingStyle::Lin);
    let mut hue_curve = hc.value.clone();

    // Enable drawCurveOnly mode.  This should only evaluate the HUE-SAT spline for use
    // in a user interface.
    hue_curve.draw_curve_only = true;

    let huesat = hue_curve.curve_mut(HueCurveType::HueSat);
    huesat.spline_type = BSplineType::DiagonalBSpline;

    huesat.set_num_control_points(3);
    *huesat.control_point_mut(0).unwrap() = GradingControlPoint::new(0.0, 0.0);
    *huesat.control_point_mut(1).unwrap() = GradingControlPoint::new(0.5, 0.7);
    *huesat.control_point_mut(2).unwrap() = GradingControlPoint::new(1.0, 1.0);

    hue_curve.validate().unwrap();
    hc.value = hue_curve;
    set_hue_curve_processor(test, hc, dir, dynamic);

    test.set_error_threshold(1e-5);
}

#[test]
fn draw_lin_fwd() {
    gpu_test("GradingHueCurve/draw_lin_fwd", |t| {
        draw_curve(t, TransformDirection::Forward, false)
    });
}

#[test]
fn draw_lin_fwd_dynamic() {
    gpu_test("GradingHueCurve/draw_lin_fwd_dynamic", |t| {
        draw_curve(t, TransformDirection::Forward, true)
    });
}

#[test]
fn draw_lin_rev() {
    gpu_test("GradingHueCurve/draw_lin_rev", |t| {
        draw_curve(t, TransformDirection::Inverse, false)
    });
}

#[test]
fn draw_lin_rev_dynamic() {
    gpu_test("GradingHueCurve/draw_lin_rev_dynamic", |t| {
        draw_curve(t, TransformDirection::Inverse, true)
    });
}

fn bypass_rgb_to_hsy(test: &mut GpuTest, dir: TransformDirection, dynamic: bool) {
    let mut hc = GradingHueCurveTransform::new(GradingStyle::Lin);
    let mut hue_curve = hc.value.clone();
    let satsat = hue_curve.curve_mut(HueCurveType::SatSat);
    *satsat.control_point_mut(1).unwrap() = GradingControlPoint::new(0.4, 0.6);

    hc.rgb_to_hsy = HsyTransformStyle::None;

    hue_curve.validate().unwrap();
    hc.value = hue_curve;
    set_hue_curve_processor(test, hc, dir, dynamic);

    test.set_error_threshold(1e-5);
    test.set_test_infinity(false);
}

#[test]
fn bypass_rgbtohsy_fwd() {
    gpu_test("GradingHueCurve/bypass_rgbtohsy_fwd", |t| {
        bypass_rgb_to_hsy(t, TransformDirection::Forward, false)
    });
}

#[test]
fn bypass_rgbtohsy_fwd_dynamic() {
    gpu_test("GradingHueCurve/bypass_rgbtohsy_fwd_dynamic", |t| {
        bypass_rgb_to_hsy(t, TransformDirection::Forward, true)
    });
}

#[test]
fn bypass_rgbtohsy_rev() {
    gpu_test("GradingHueCurve/bypass_rgbtohsy_rev", |t| {
        bypass_rgb_to_hsy(t, TransformDirection::Inverse, false)
    });
}

#[test]
fn bypass_rgbtohsy_rev_dynamic() {
    gpu_test("GradingHueCurve/bypass_rgbtohsy_rev_dynamic", |t| {
        bypass_rgb_to_hsy(t, TransformDirection::Inverse, true)
    });
}
