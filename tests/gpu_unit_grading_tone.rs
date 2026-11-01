//! Port of the OpenColorIO GPU unit tests `tests/gpu/GradingToneOp_test.cpp`,
//! running the WGSL shaders (see `wgsl_common::gpu_unit`).

mod wgsl_common;

use ocio::gpu::{GpuShaderCreator, GpuShaderDesc};
use ocio::{
    DynamicPropertyType, GradingRgbmsw, GradingStyle, GradingTone, GradingToneTransform,
    GroupTransform, Processor, SharedValue, Transform, TransformDirection,
};
use wgsl_common::gpu_unit::{gpu_test, GpuTest};

mod gt_test1 {
    use super::*;
    pub const STYLE: GradingStyle = GradingStyle::Log;
    // These are {R, G, B, master, center, width};
    pub const MIDTONES: GradingRgbmsw = GradingRgbmsw::new(0.3, 1.0, 1.8, 1.2, 0.47, 0.6);
}

mod gt_test2 {
    use super::*;
    pub const STYLE: GradingStyle = GradingStyle::Log;
    // These are {R, G, B, master, start, pivot};
    pub const HIGHLIGHTS: GradingRgbmsw = GradingRgbmsw::new(0.3, 1.0, 1.8, 1.4, -0.1, 0.9);
}

mod gt_test3 {
    use super::*;
    pub const STYLE: GradingStyle = GradingStyle::Video;
    // These are {R, G, B, master, start, pivot};
    pub const SHADOWS: GradingRgbmsw = GradingRgbmsw::new(0.3, 1., 1.79, 0.6, 0.8, -0.1);
}

mod gt_test4 {
    use super::*;
    pub const STYLE: GradingStyle = GradingStyle::Video;
    // These are {R, G, B, master, start, width};
    pub const WHITES: GradingRgbmsw = GradingRgbmsw::new(0.3, 1., 1.9, 0.6, -0.2, 1.4);
}

mod gt_test5 {
    use super::*;
    pub const STYLE: GradingStyle = GradingStyle::Log;
    // These are {R, G, B, master, start, width};
    pub const BLACKS: GradingRgbmsw = GradingRgbmsw::new(0.3, 1., 1.9, 0.6, 0.8, 0.9);
}

mod gt_test6 {
    use super::*;
    pub const STYLE: GradingStyle = GradingStyle::Log;
    pub const SCONTRAST: f64 = 1.8;
    pub const SCONTRAST2: f64 = 0.3;
}

mod gt_test7 {
    use super::*;
    pub const STYLE: GradingStyle = GradingStyle::Lin;
    // These are {R, G, B, master, center, width};
    pub const MIDTONES: GradingRgbmsw = GradingRgbmsw::new(0.3, 1.4, 1.8, 1., 1., 8.);
}

/// Common setup of the single-transform tests: `edit` sets the values of the
/// tone grading.
fn grading_tone_test(
    test: &mut GpuTest,
    style: GradingStyle,
    dir: TransformDirection,
    dynamic: bool,
    edit: impl FnOnce(&mut GradingTone),
    error_threshold: f32,
) {
    let mut gt = GradingToneTransform::new(style);
    gt.direction = dir;
    if dynamic {
        gt.dynamic = true;
    }

    let mut tone = GradingTone::new(style);
    edit(&mut tone);
    gt.value = tone;
    test.set_processor_transform(Transform::GradingTone(gt));

    test.set_error_threshold(error_threshold);
    test.set_expected_minimal_value(1.0);
    test.set_relative_comparison(true);
    test.set_test_wide_range(true);
    test.set_test_infinity(false);
    test.set_test_nan(false);
}

fn grading_tone_log_midtones(test: &mut GpuTest, dir: TransformDirection, dynamic: bool) {
    grading_tone_test(
        test,
        gt_test1::STYLE,
        dir,
        dynamic,
        |t| t.midtones = gt_test1::MIDTONES,
        2e-5,
    );
}

#[test]
fn style_log_midtones_fwd() {
    gpu_test("GradingTone/style_log_midtones_fwd", |t| {
        grading_tone_log_midtones(t, TransformDirection::Forward, false)
    });
}

#[test]
fn style_log_midtones_fwd_dynamic() {
    gpu_test("GradingTone/style_log_midtones_fwd_dynamic", |t| {
        grading_tone_log_midtones(t, TransformDirection::Forward, true)
    });
}

#[test]
fn style_log_midtones_rev() {
    gpu_test("GradingTone/style_log_midtones_rev", |t| {
        grading_tone_log_midtones(t, TransformDirection::Inverse, false)
    });
}

#[test]
fn style_log_midtones_rev_dynamic() {
    gpu_test("GradingTone/style_log_midtones_rev_dynamic", |t| {
        grading_tone_log_midtones(t, TransformDirection::Inverse, true)
    });
}

fn grading_tone_log_highlights(test: &mut GpuTest, dir: TransformDirection, dynamic: bool) {
    grading_tone_test(
        test,
        gt_test2::STYLE,
        dir,
        dynamic,
        |t| t.highlights = gt_test2::HIGHLIGHTS,
        2e-5,
    );
}

#[test]
fn style_log_highlights_fwd() {
    gpu_test("GradingTone/style_log_highlights_fwd", |t| {
        grading_tone_log_highlights(t, TransformDirection::Forward, false)
    });
}

#[test]
fn style_log_highlights_fwd_dynamic() {
    gpu_test("GradingTone/style_log_highlights_fwd_dynamic", |t| {
        grading_tone_log_highlights(t, TransformDirection::Forward, true)
    });
}

#[test]
fn style_log_highlights_rev() {
    gpu_test("GradingTone/style_log_highlights_rev", |t| {
        grading_tone_log_highlights(t, TransformDirection::Inverse, false)
    });
}

#[test]
fn style_log_highlights_rev_dynamic() {
    gpu_test("GradingTone/style_log_highlights_rev_dynamic", |t| {
        grading_tone_log_highlights(t, TransformDirection::Inverse, true)
    });
}

fn grading_tone_video_shadows(test: &mut GpuTest, dir: TransformDirection, dynamic: bool) {
    grading_tone_test(
        test,
        gt_test3::STYLE,
        dir,
        dynamic,
        |t| t.shadows = gt_test3::SHADOWS,
        3e-5,
    );
}

#[test]
fn style_video_shadows_fwd() {
    gpu_test("GradingTone/style_video_shadows_fwd", |t| {
        grading_tone_video_shadows(t, TransformDirection::Forward, false)
    });
}

#[test]
fn style_video_shadows_fwd_dynamic() {
    gpu_test("GradingTone/style_video_shadows_fwd_dynamic", |t| {
        grading_tone_video_shadows(t, TransformDirection::Forward, true)
    });
}

#[test]
fn style_video_shadows_rev() {
    gpu_test("GradingTone/style_video_shadows_rev", |t| {
        grading_tone_video_shadows(t, TransformDirection::Inverse, false)
    });
}

#[test]
fn style_video_shadows_rev_dynamic() {
    gpu_test("GradingTone/style_video_shadows_rev_dynamic", |t| {
        grading_tone_video_shadows(t, TransformDirection::Inverse, true)
    });
}

fn grading_tone_video_whites(test: &mut GpuTest, dir: TransformDirection, dynamic: bool) {
    grading_tone_test(
        test,
        gt_test4::STYLE,
        dir,
        dynamic,
        |t| t.whites = gt_test4::WHITES,
        3e-5,
    );
}

#[test]
fn style_video_white_detail_fwd() {
    gpu_test("GradingTone/style_video_white_detail_fwd", |t| {
        grading_tone_video_whites(t, TransformDirection::Forward, false)
    });
}

#[test]
fn style_video_white_detail_fwd_dynamic() {
    gpu_test("GradingTone/style_video_white_detail_fwd_dynamic", |t| {
        grading_tone_video_whites(t, TransformDirection::Forward, true)
    });
}

#[test]
fn style_video_white_detail_rev() {
    gpu_test("GradingTone/style_video_white_detail_rev", |t| {
        grading_tone_video_whites(t, TransformDirection::Inverse, false)
    });
}

#[test]
fn style_video_white_detail_rev_dynamic() {
    gpu_test("GradingTone/style_video_white_detail_rev_dynamic", |t| {
        grading_tone_video_whites(t, TransformDirection::Inverse, true)
    });
}

fn grading_tone_log_blacks(test: &mut GpuTest, dir: TransformDirection, dynamic: bool) {
    grading_tone_test(
        test,
        gt_test5::STYLE,
        dir,
        dynamic,
        |t| t.blacks = gt_test5::BLACKS,
        3e-5,
    );
}

#[test]
fn style_log_black_detail_fwd() {
    gpu_test("GradingTone/style_log_black_detail_fwd", |t| {
        grading_tone_log_blacks(t, TransformDirection::Forward, false)
    });
}

#[test]
fn style_log_black_detail_fwd_dynamic() {
    gpu_test("GradingTone/style_log_black_detail_fwd_dynamic", |t| {
        grading_tone_log_blacks(t, TransformDirection::Forward, true)
    });
}

#[test]
fn style_log_black_detail_rev() {
    gpu_test("GradingTone/style_log_black_detail_rev", |t| {
        grading_tone_log_blacks(t, TransformDirection::Inverse, false)
    });
}

#[test]
fn style_log_black_detail_rev_dynamic() {
    gpu_test("GradingTone/style_log_black_detail_rev_dynamic", |t| {
        grading_tone_log_blacks(t, TransformDirection::Inverse, true)
    });
}

fn grading_tone_log_scontrast(
    test: &mut GpuTest,
    dir: TransformDirection,
    dynamic: bool,
    scontrast: f64,
) {
    grading_tone_test(
        test,
        gt_test6::STYLE,
        dir,
        dynamic,
        |t| t.s_contrast = scontrast,
        3e-5,
    );
}

#[test]
fn style_log_scontrast_fwd() {
    gpu_test("GradingTone/style_log_scontrast_fwd", |t| {
        grading_tone_log_scontrast(t, TransformDirection::Forward, false, gt_test6::SCONTRAST)
    });
}

#[test]
fn style_log_scontrast_fwd_dynamic() {
    gpu_test("GradingTone/style_log_scontrast_fwd_dynamic", |t| {
        grading_tone_log_scontrast(t, TransformDirection::Forward, true, gt_test6::SCONTRAST)
    });
}

#[test]
fn style_log_scontrast2_fwd() {
    gpu_test("GradingTone/style_log_scontrast2_fwd", |t| {
        grading_tone_log_scontrast(t, TransformDirection::Forward, false, gt_test6::SCONTRAST2)
    });
}

#[test]
fn style_log_scontrast2_fwd_dynamic() {
    gpu_test("GradingTone/style_log_scontrast2_fwd_dynamic", |t| {
        grading_tone_log_scontrast(t, TransformDirection::Forward, true, gt_test6::SCONTRAST2)
    });
}

#[test]
fn style_log_scontrast_rev() {
    gpu_test("GradingTone/style_log_scontrast_rev", |t| {
        grading_tone_log_scontrast(t, TransformDirection::Inverse, false, gt_test6::SCONTRAST)
    });
}

#[test]
fn style_log_scontrast_rev_dynamic() {
    gpu_test("GradingTone/style_log_scontrast_rev_dynamic", |t| {
        grading_tone_log_scontrast(t, TransformDirection::Inverse, true, gt_test6::SCONTRAST)
    });
}

#[test]
fn style_log_scontrast2_rev() {
    gpu_test("GradingTone/style_log_scontrast2_rev", |t| {
        grading_tone_log_scontrast(t, TransformDirection::Inverse, false, gt_test6::SCONTRAST2)
    });
}

#[test]
fn style_log_scontrast2_rev_dynamic() {
    gpu_test("GradingTone/style_log_scontrast2_rev_dynamic", |t| {
        grading_tone_log_scontrast(t, TransformDirection::Inverse, true, gt_test6::SCONTRAST2)
    });
}

fn grading_tone_lin_midtones(test: &mut GpuTest, dir: TransformDirection, dynamic: bool) {
    grading_tone_test(
        test,
        gt_test7::STYLE,
        dir,
        dynamic,
        |t| t.midtones = gt_test7::MIDTONES,
        5e-5,
    );
}

#[test]
fn style_lin_midtones_fwd() {
    gpu_test("GradingTone/style_lin_midtones_fwd", |t| {
        grading_tone_lin_midtones(t, TransformDirection::Forward, false)
    });
}

#[test]
fn style_lin_midtones_fwd_dynamic() {
    gpu_test("GradingTone/style_lin_midtones_fwd_dynamic", |t| {
        grading_tone_lin_midtones(t, TransformDirection::Forward, true)
    });
}

#[test]
fn style_lin_midtones_rev() {
    gpu_test("GradingTone/style_lin_midtones_rev", |t| {
        grading_tone_lin_midtones(t, TransformDirection::Inverse, false)
    });
}

#[test]
fn style_lin_midtones_rev_dynamic() {
    gpu_test("GradingTone/style_lin_midtones_rev_dynamic", |t| {
        grading_tone_lin_midtones(t, TransformDirection::Inverse, true)
    });
}

// TODO: add inverse tests.

/// The grading tone dynamic property of the processor (CPU side).
fn cpu_tone(processor: &Processor) -> SharedValue<GradingTone> {
    processor
        .dynamic_property(DynamicPropertyType::GradingTone)
        .expect("CPU grading tone dynamic property")
        .as_grading_tone()
        .expect("grading tone value")
        .clone()
}

/// The grading tone dynamic property of the shader (GPU side). Dynamic
/// properties are decoupled between processor and shader.
fn gpu_tone(shader_desc: &GpuShaderDesc) -> SharedValue<GradingTone> {
    shader_desc
        .dynamic_property(DynamicPropertyType::GradingTone)
        .expect("GPU grading tone dynamic property")
        .as_grading_tone()
        .expect("grading tone value")
        .clone()
}

/// Port of the `GTRetest` retests: `edit` updates the values read from the
/// processor and the result is set on both the processor and the shader.
fn tone_retest(
    edit: impl Fn(&mut GradingTone) + 'static,
) -> impl FnMut(&Processor, &GpuShaderDesc) {
    move |processor, shader_desc| {
        let dyn_prop = cpu_tone(processor);
        let dyn_prop_gpu = gpu_tone(shader_desc);
        let mut vals = dyn_prop.get();
        edit(&mut vals);
        dyn_prop.set(vals);
        dyn_prop_gpu.set(vals);
    }
}

fn add_tone_retests(test: &mut GpuTest) {
    test.add_retest(tone_retest(|vals| vals.midtones = gt_test1::MIDTONES));
    test.add_retest(tone_retest(|vals| {
        vals.highlights = gt_test2::HIGHLIGHTS;
        vals.whites = gt_test4::WHITES;
    }));
    test.add_retest(tone_retest(|vals| vals.blacks = gt_test5::BLACKS));
    test.add_retest(tone_retest(|vals| {
        *vals = GradingTone::new(GradingStyle::Log)
    }));
}

#[test]
fn style_log_dynamic_retests() {
    gpu_test("GradingTone/style_log_dynamic_retests", |test| {
        let mut gt = GradingToneTransform::new(GradingStyle::Log);
        gt.dynamic = true;

        gt.value = GradingTone::new(GradingStyle::Log);
        test.set_processor_transform(Transform::GradingTone(gt));

        add_tone_retests(test);

        test.set_error_threshold(5e-5);
        test.set_expected_minimal_value(1.0);
        test.set_relative_comparison(true);
        test.set_test_wide_range(true);

        test.set_test_infinity(false);
        test.set_test_nan(false);
    });
}

#[test]
fn two_transforms_retests() {
    gpu_test("GradingTone/two_transforms_retests", |test| {
        let mut gt_dyn = GradingToneTransform::new(GradingStyle::Log);
        gt_dyn.dynamic = true;

        gt_dyn.value = GradingTone::new(GradingStyle::Log);

        let mut gt_non_dyn = GradingToneTransform::new(gt_test6::STYLE);

        let mut tone = GradingTone::new(GradingStyle::Lin);
        tone.s_contrast = 1.8;
        tone.midtones = GradingRgbmsw::new(0.3, 1.4, 1.8, 1., 1., 8.);

        gt_non_dyn.value = tone;

        let group = GroupTransform::from_transforms(vec![
            Transform::GradingTone(gt_dyn),
            Transform::GradingTone(gt_non_dyn),
        ]);
        test.set_processor_transform(Transform::Group(group));

        add_tone_retests(test);

        test.set_error_threshold(5e-5);
        test.set_expected_minimal_value(1.0);
        test.set_relative_comparison(true);
        test.set_test_wide_range(true);

        test.set_test_infinity(false);
        test.set_test_nan(false);
    });
}
