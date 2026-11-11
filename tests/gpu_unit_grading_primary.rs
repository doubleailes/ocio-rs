// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Copyright (c) 2026 Philippe Llerena.

//! Port of the OpenColorIO GPU unit tests `tests/gpu/GradingPrimaryOp_test.cpp`,
//! running the WGSL shaders (see `wgsl_common::gpu_unit`).

mod wgsl_common;

use ocio::gpu::{GpuShaderCreator, GpuShaderDesc};
use ocio::{
    DynamicPropertyType, GradingPrimary, GradingPrimaryTransform, GradingRgbm, GradingStyle,
    GroupTransform, Processor, SharedValue, Transform, TransformDirection,
};
use wgsl_common::gpu_unit::{gpu_test, GpuTest};

mod gp_test1 {
    use super::*;
    pub const STYLE: GradingStyle = GradingStyle::Log;
    pub const BRIGHTNESS: GradingRgbm = GradingRgbm::new(-10., 45., -5., 50.);
    pub const CONTRAST: GradingRgbm = GradingRgbm::new(0.9, 1.4, 0.7, 0.75);
    pub const GAMMA: GradingRgbm = GradingRgbm::new(1.1, 0.7, 1.05, 1.15);
    pub const SATURATION: f64 = 1.21;
    pub const PIVOT: f64 = -0.3;
    pub const PIVOT_BLACK: f64 = 0.05;
    pub const PIVOT_WHITE: f64 = 0.9;
    pub const CLAMP_BLACK: f64 = -0.05;
    pub const CLAMP_WHITE: f64 = 1.50;
}

fn grading_primary_log(test: &mut GpuTest, dir: TransformDirection, dynamic: bool) {
    let mut gp = GradingPrimaryTransform::new(gp_test1::STYLE);
    gp.direction = dir;
    if dynamic {
        gp.dynamic = true;
    }

    let mut gplog = GradingPrimary::new(gp_test1::STYLE);
    gplog.brightness = gp_test1::BRIGHTNESS;
    gplog.contrast = gp_test1::CONTRAST;
    gplog.gamma = gp_test1::GAMMA;
    gplog.saturation = gp_test1::SATURATION;
    gplog.pivot = gp_test1::PIVOT;
    gplog.pivot_black = gp_test1::PIVOT_BLACK;
    gplog.pivot_white = gp_test1::PIVOT_WHITE;
    gplog.clamp_black = gp_test1::CLAMP_BLACK;
    gplog.clamp_white = gp_test1::CLAMP_WHITE;
    gp.value = gplog;
    test.set_processor_transform(Transform::GradingPrimary(gp));

    test.set_error_threshold(2e-5);
    test.set_expected_minimal_value(1.0);
    test.set_relative_comparison(true);
    test.set_test_wide_range(true);
    test.set_test_infinity(false);
    test.set_test_nan(false);
}

#[test]
fn style_log_fwd() {
    gpu_test("GradingPrimary/style_log_fwd", |t| {
        grading_primary_log(t, TransformDirection::Forward, false)
    });
}

#[test]
fn style_log_fwd_dynamic() {
    gpu_test("GradingPrimary/style_log_fwd_dynamic", |t| {
        grading_primary_log(t, TransformDirection::Forward, true)
    });
}

#[test]
fn style_log_rev() {
    gpu_test("GradingPrimary/style_log_rev", |t| {
        grading_primary_log(t, TransformDirection::Inverse, false)
    });
}

#[test]
fn style_log_rev_dynamic() {
    gpu_test("GradingPrimary/style_log_rev_dynamic", |t| {
        grading_primary_log(t, TransformDirection::Inverse, true)
    });
}

mod gp_test2 {
    use super::*;
    pub const STYLE: GradingStyle = GradingStyle::Lin;
    pub const EXPOSURE: GradingRgbm = GradingRgbm::new(0.5, -0.2, 0.4, -0.25);
    pub const OFFSET: GradingRgbm = GradingRgbm::new(-0.03, 0.02, 0.1, -0.1);
    pub const CONTRAST: GradingRgbm = GradingRgbm::new(0.9, 1.4, 0.7, 0.75);
    pub const SATURATION: f64 = 1.33;
    pub const PIVOT: f64 = 0.5;
    pub const CLAMP_BLACK: f64 = -0.40;
    pub const CLAMP_WHITE: f64 = 1.05;
}

fn grading_primary_lin_transform() -> GradingPrimaryTransform {
    let mut gp = GradingPrimaryTransform::new(gp_test2::STYLE);
    let mut gplin = GradingPrimary::new(gp_test2::STYLE);
    gplin.exposure = gp_test2::EXPOSURE;
    gplin.contrast = gp_test2::CONTRAST;
    gplin.offset = gp_test2::OFFSET;
    gplin.pivot = gp_test2::PIVOT;
    gplin.saturation = gp_test2::SATURATION;
    gplin.clamp_black = gp_test2::CLAMP_BLACK;
    gplin.clamp_white = gp_test2::CLAMP_WHITE;
    gp.value = gplin;
    gp
}

fn grading_primary_lin(test: &mut GpuTest, dir: TransformDirection, dynamic: bool) {
    let mut gp = grading_primary_lin_transform();
    gp.direction = dir;
    if dynamic {
        gp.dynamic = true;
    }
    test.set_processor_transform(Transform::GradingPrimary(gp));

    test.set_error_threshold(2e-5);
    test.set_expected_minimal_value(1.0);
    test.set_relative_comparison(true);
    test.set_test_wide_range(true);
    test.set_test_infinity(false);
    test.set_test_nan(false);
}

#[test]
fn style_lin_fwd() {
    gpu_test("GradingPrimary/style_lin_fwd", |t| {
        grading_primary_lin(t, TransformDirection::Forward, false)
    });
}

#[test]
fn style_lin_fwd_dynamic() {
    gpu_test("GradingPrimary/style_lin_fwd_dynamic", |t| {
        grading_primary_lin(t, TransformDirection::Forward, true)
    });
}

#[test]
fn style_lin_rev() {
    gpu_test("GradingPrimary/style_lin_rev", |t| {
        grading_primary_lin(t, TransformDirection::Inverse, false)
    });
}

#[test]
fn style_lin_rev_dynamic() {
    gpu_test("GradingPrimary/style_lin_rev_dynamic", |t| {
        grading_primary_lin(t, TransformDirection::Inverse, true)
    });
}

mod gp_test3 {
    use super::*;
    pub const STYLE: GradingStyle = GradingStyle::Video;
    pub const LIFT: GradingRgbm = GradingRgbm::new(0.05, -0.04, 0.02, 0.05);
    pub const GAMMA: GradingRgbm = GradingRgbm::new(0.9, 1.4, 0.7, 0.75);
    pub const GAIN: GradingRgbm = GradingRgbm::new(1.2, 1.1, 1.25, 0.8);
    pub const OFFSET: GradingRgbm = GradingRgbm::new(-0.03, 0.02, 0.1, -0.1);
    pub const SATURATION: f64 = 1.2;
    pub const PIVOT_BLACK: f64 = 0.05;
    pub const PIVOT_WHITE: f64 = 0.9;
    pub const CLAMP_BLACK: f64 = -0.15;
    pub const CLAMP_WHITE: f64 = 1.50;
}

fn grading_primary_video(test: &mut GpuTest, dir: TransformDirection, dynamic: bool) {
    let mut gp = GradingPrimaryTransform::new(gp_test3::STYLE);
    gp.direction = dir;
    if dynamic {
        gp.dynamic = true;
    }

    let mut gpvideo = GradingPrimary::new(gp_test3::STYLE);
    gpvideo.lift = gp_test3::LIFT;
    gpvideo.gamma = gp_test3::GAMMA;
    gpvideo.gain = gp_test3::GAIN;
    gpvideo.offset = gp_test3::OFFSET;
    gpvideo.saturation = gp_test3::SATURATION;
    gpvideo.clamp_black = gp_test3::CLAMP_BLACK;
    gpvideo.clamp_white = gp_test3::CLAMP_WHITE;
    gpvideo.pivot_black = gp_test3::PIVOT_BLACK;
    gpvideo.pivot_white = gp_test3::PIVOT_WHITE;
    gp.value = gpvideo;
    test.set_processor_transform(Transform::GradingPrimary(gp));

    test.set_error_threshold(3e-5);
    test.set_expected_minimal_value(1.0);
    test.set_relative_comparison(true);
    test.set_test_wide_range(true);
    test.set_test_infinity(false);
    test.set_test_nan(false);
}

#[test]
fn style_video_fwd() {
    gpu_test("GradingPrimary/style_video_fwd", |t| {
        grading_primary_video(t, TransformDirection::Forward, false)
    });
}

#[test]
fn style_video_fwd_dynamic() {
    gpu_test("GradingPrimary/style_video_fwd_dynamic", |t| {
        grading_primary_video(t, TransformDirection::Forward, true)
    });
}

#[test]
fn style_video_rev() {
    gpu_test("GradingPrimary/style_video_rev", |t| {
        grading_primary_video(t, TransformDirection::Inverse, false)
    });
}

#[test]
fn style_video_rev_dynamic() {
    gpu_test("GradingPrimary/style_video_rev_dynamic", |t| {
        grading_primary_video(t, TransformDirection::Inverse, true)
    });
}

/// Port of upstream `GPRetest`: the harness creates a new CPU processor for
/// each retest (but keeps the shader). Dynamic properties are decoupled
/// between the processor and the shader, so each retest updates both: the
/// processor property (for the CPU) and the shader description property (for
/// the GPU).
fn update_dynamic_primary(
    processor: &Processor,
    shader_desc: &GpuShaderDesc,
    update: impl FnOnce(&mut GradingPrimary),
) {
    let dyn_prop = processor
        .dynamic_property(DynamicPropertyType::GradingPrimary)
        .expect("processor grading primary dynamic property");
    let dyn_prop: &SharedValue<GradingPrimary> = dyn_prop.as_grading_primary().unwrap();
    let dyn_prop_gpu = shader_desc
        .dynamic_property(DynamicPropertyType::GradingPrimary)
        .expect("shader grading primary dynamic property");
    let dyn_prop_gpu = dyn_prop_gpu.as_grading_primary().unwrap();

    let mut vals = dyn_prop.get();
    update(&mut vals);
    dyn_prop.set(vals);
    dyn_prop_gpu.set(vals);
}

/// The retests of upstream `MyGPRetest` (`retest_clamp`, `retest_pivot`,
/// `retest1` to `retest4`).
fn add_primary_retests(test: &mut GpuTest) {
    // retest_clamp
    test.add_retest(|p, d| {
        update_dynamic_primary(p, d, |vals| {
            vals.clamp_black = gp_test1::CLAMP_BLACK;
            vals.clamp_white = gp_test1::CLAMP_WHITE;
        })
    });
    // retest_pivot
    test.add_retest(|p, d| {
        update_dynamic_primary(p, d, |vals| {
            vals.clamp_black = -100.;
            vals.clamp_white = 100.;
            vals.pivot_black = gp_test1::PIVOT_BLACK;
            vals.pivot_white = gp_test1::PIVOT_WHITE;
        })
    });
    // retest1
    test.add_retest(|p, d| {
        update_dynamic_primary(p, d, |vals| {
            vals.gamma.red += 0.1;
            vals.gamma.master += 0.1;
        })
    });
    // retest2
    test.add_retest(|p, d| update_dynamic_primary(p, d, |vals| vals.saturation += 0.1));
    // retest3
    test.add_retest(|p, d| update_dynamic_primary(p, d, |vals| vals.clamp_white += 1.));
    // retest4
    test.add_retest(|p, d| {
        update_dynamic_primary(p, d, |vals| *vals = GradingPrimary::new(gp_test1::STYLE))
    });
}

fn dynamic_log_transform() -> GradingPrimaryTransform {
    let mut gp = GradingPrimaryTransform::new(gp_test1::STYLE);
    gp.dynamic = true;

    let mut gplog = GradingPrimary::new(gp_test1::STYLE);
    gplog.brightness = gp_test1::BRIGHTNESS;
    gplog.contrast = gp_test1::CONTRAST;
    gplog.gamma = gp_test1::GAMMA;
    gplog.saturation = gp_test1::SATURATION;
    gplog.pivot = gp_test1::PIVOT;
    gp.value = gplog;
    gp
}

// Test that a dynamic property may be updated and that the result still matches the CPU.
#[test]
fn style_log_dynamic_retests() {
    gpu_test("GradingPrimary/style_log_dynamic_retests", |t| {
        t.set_processor_transform(Transform::GradingPrimary(dynamic_log_transform()));

        add_primary_retests(t);

        t.set_error_threshold(5e-5);
        t.set_expected_minimal_value(1.0);
        t.set_relative_comparison(true);
        t.set_test_wide_range(true);

        t.set_test_infinity(false);
        t.set_test_nan(false);
    });
}

#[test]
fn two_transforms_retests() {
    gpu_test("GradingPrimary/two_transforms_retests", |t| {
        let gp_dyn = dynamic_log_transform();
        let gp_non_dyn = grading_primary_lin_transform();

        let mut group = GroupTransform::new();
        group.append(Transform::GradingPrimary(gp_dyn));
        group.append(Transform::GradingPrimary(gp_non_dyn));
        t.set_processor_transform(Transform::Group(group));

        add_primary_retests(t);

        t.set_error_threshold(1e-4);
        t.set_expected_minimal_value(1.0);
        t.set_relative_comparison(true);
        t.set_test_wide_range(true);

        t.set_test_infinity(false);
        t.set_test_nan(false);
    });
}
