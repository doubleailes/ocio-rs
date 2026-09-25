//! Tests ported from `GradingTone_tests.cpp`, `GradingToneOpData_tests.cpp`,
//! `GradingToneOp_tests.cpp`, `GradingToneOpCPU_tests.cpp`,
//! `GradingToneTransform_tests.cpp` and the grading tone parts of
//! `DynamicProperty_tests.cpp`.

#![allow(clippy::excessive_precision)]

use super::*;
use crate::ops::grading_primary::tests::{processor, to_pixels};
use crate::processor::optimize_ops;

/// Port of the non SSE `ValidateImage` of `GradingToneOpCPU_tests.cpp`
/// (relative error for values above 1).
fn validate_image(expected: &[f32], res: &[Pixel], line: u32) {
    const ERROR: f32 = 1e-6;
    for (i, px) in res.iter().enumerate() {
        for j in 0..4 {
            let e = expected[i * 4 + j];
            let r = px[j];
            if e.is_nan() {
                assert!(
                    r.is_nan(),
                    "line {line}: pixel {i} channel {j}: expected NaN, got {r}"
                );
            } else if e != r {
                let factor = if e.abs() < 1.0 { 1.0 } else { e.abs() };
                assert!(
                    (e - r).abs() <= ERROR * factor,
                    "line {line}: pixel {i} channel {j}: expected {e}, got {r}"
                );
            }
        }
    }
}

fn apply_op(op: &dyn Op, input: &[f32]) -> Vec<Pixel> {
    let mut px = to_pixels(input);
    op.apply(&mut px);
    px
}

// GradingTone_tests.cpp

#[test]
fn rgbmsw_channel() {
    let rgbm1 = GradingRgbmsw::new(1., 2., 3., 4., 5., 6.);
    assert_eq!(channel_value(&rgbm1, RgbmChannel::R), 1.);
    assert_eq!(channel_value(&rgbm1, RgbmChannel::G), 2.);
    assert_eq!(channel_value(&rgbm1, RgbmChannel::B), 3.);
    assert_eq!(channel_value(&rgbm1, RgbmChannel::M), 4.);
}

#[test]
fn prerender_style() {
    let p = GradingTonePreRender::new(GradingStyle::Lin);
    assert_eq!((p.top, p.top_sc, p.bottom, p.pivot), (7.5, 6.5, -5.5, 0.0));
    let mut p = GradingTonePreRender::new(GradingStyle::Log);
    assert_eq!((p.top, p.top_sc, p.bottom, p.pivot), (1.0, 1.0, 0.0, 0.4));
    p.set_style(GradingStyle::Video);
    assert_eq!((p.top, p.top_sc, p.bottom, p.pivot), (1.0, 1.0, 0.0, 0.4));
    p.update(&GradingTone::new(GradingStyle::Video));
    assert!(p.local_bypass);
    let mut v = GradingTone::new(GradingStyle::Video);
    v.s_contrast = 1.2;
    p.update(&v);
    assert!(!p.local_bypass);
}

// GradingToneOpData_tests.cpp

#[test]
fn op_data_accessors() {
    let op = GradingToneOp::identity(GradingStyle::Lin);
    assert_eq!(op.style(), GradingStyle::Lin);
    assert_eq!(op.value(), GradingTone::new(GradingStyle::Lin));
    assert_eq!(op.direction(), TransformDirection::Forward);

    let gt = GradingToneOp::new(
        GradingStyle::Log,
        GradingTone::new(GradingStyle::Lin),
        TransformDirection::Inverse,
        false,
    )
    .unwrap();
    assert!(gt.is_no_op());
    assert!(gt.is_identity());
    assert!(!gt.has_channel_crosstalk());
    let expected = "<GradingToneOp log inverse \
                    <blacks=<red=1 green=1 blue=1 master=1 start=0 width=4> \
                    shadows=<red=1 green=1 blue=1 master=1 start=2 width=-7> \
                    midtones=<red=1 green=1 blue=1 master=1 start=0 width=8> \
                    highlights=<red=1 green=1 blue=1 master=1 start=-2 width=9> \
                    whites=<red=1 green=1 blue=1 master=1 start=0 width=8> \
                    s_contrast=1>>";
    assert_eq!(gt.cache_id(), expected);

    let mut v1 = GradingTone::new(GradingStyle::Log);
    v1.midtones.red += 0.1;
    v1.s_contrast += 0.1;
    let gt1 =
        GradingToneOp::new(GradingStyle::Log, v1, TransformDirection::Inverse, false).unwrap();
    assert!(!gt1.is_identity());

    // Check inverse.
    let gt1_inv = gt1.inverse();
    assert!(gt1.is_inverse(&gt1_inv));
    assert_eq!(gt1.direction(), TransformDirection::Inverse);
    assert_eq!(gt1_inv.direction(), TransformDirection::Forward);
    assert_eq!(gt1_inv.value(), v1);

    // Not inverse: other style, other value, same direction, or dynamic.
    let other =
        GradingToneOp::new(GradingStyle::Video, v1, TransformDirection::Forward, false).unwrap();
    assert!(!gt1.is_inverse(&other));
    let mut v2 = v1;
    v2.s_contrast += 0.1;
    let other =
        GradingToneOp::new(GradingStyle::Log, v2, TransformDirection::Forward, false).unwrap();
    assert!(!gt1.is_inverse(&other));
    assert!(!gt1.is_inverse(&gt1));
    let dyn_op =
        GradingToneOp::new(GradingStyle::Log, v1, TransformDirection::Forward, true).unwrap();
    assert!(!gt1.is_inverse(&dyn_op));
}

#[test]
fn op_data_validate() {
    let mut v = GradingTone::new(GradingStyle::Log);
    v.blacks.red = 2.0;
    let err =
        GradingToneOp::new(GradingStyle::Log, v, TransformDirection::Forward, false).unwrap_err();
    assert!(err
        .message()
        .contains("GradingTone blacks '<red=2 green=1 blue=1 master=1 start=0.4 width=0.4>' are above upper bound (1.9)"));
    v.blacks.red = 1.5;
    assert!(GradingToneOp::new(GradingStyle::Log, v, TransformDirection::Forward, false).is_ok());
}

#[test]
fn op_data_dynamic() {
    let op = GradingToneOp::new(
        GradingStyle::Lin,
        GradingTone::new(GradingStyle::Lin),
        TransformDirection::Forward,
        true,
    )
    .unwrap();
    assert!(op.is_dynamic());
    let dp = op
        .dynamic_property(DynamicPropertyType::GradingTone)
        .unwrap();
    assert_eq!(dp.property_type(), DynamicPropertyType::GradingTone);
    assert!(op
        .dynamic_property(DynamicPropertyType::GradingPrimary)
        .is_none());

    let mut val = GradingTone::new(GradingStyle::Lin);
    val.s_contrast = 1.1;
    dp.as_grading_tone().unwrap().set(val);
    assert_eq!(op.value().s_contrast, 1.1);

    let non_dyn = op.make_non_dynamic().unwrap();
    assert!(!non_dyn.is_dynamic());
    assert_eq!(
        non_dyn
            .downcast_ref::<GradingToneOp>()
            .unwrap()
            .value()
            .s_contrast,
        1.1
    );
}

// GradingToneOp_tests.cpp

#[test]
fn op_create() {
    let mut ops = OpVec::new();
    let v = GradingTone::new(GradingStyle::Log);
    let fwd = TransformDirection::Forward;
    create_grading_tone_op(&mut ops, GradingStyle::Log, &v, fwd, false, fwd).unwrap();
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].name(), "GradingTone");
    assert!(ops[0].is_identity());
    assert!(ops[0].is_no_op());

    create_grading_tone_op(&mut ops, GradingStyle::Log, &v, fwd, true, fwd).unwrap();
    assert_eq!(ops.len(), 2);
    assert!(!ops[1].is_identity());
    assert!(!ops[1].is_no_op());
}

#[test]
fn op_create_transform() {
    let v = GradingTone::new(GradingStyle::Log);
    let op = GradingToneOp::new(GradingStyle::Log, v, TransformDirection::Forward, true).unwrap();
    match op.to_transform().unwrap() {
        Transform::GradingTone(t) => {
            assert_eq!(t.style, GradingStyle::Log);
            assert!(t.is_dynamic());
        }
        _ => panic!("wrong transform type"),
    }
}

#[test]
fn op_build_ops() {
    let config = Config::create_raw();
    let context = Context::new();
    let mut gt_transform = GradingToneTransform::new(GradingStyle::Log);

    let mut ops = OpVec::new();
    gt_transform
        .build_ops(&mut ops, &config, &context, TransformDirection::Forward)
        .unwrap();
    assert_eq!(ops.len(), 1);
    assert!(ops[0].is_identity());
    assert!(ops[0].is_no_op());
    ops.clear();

    gt_transform.make_dynamic();
    gt_transform
        .build_ops(&mut ops, &config, &context, TransformDirection::Forward)
        .unwrap();
    assert_eq!(ops.len(), 1);
    let gto = ops[0].downcast_ref::<GradingToneOp>().unwrap();
    assert!(gto.is_dynamic());
    assert_eq!(gto.value().s_contrast, 1.0);

    // Changing the source does not change the op.
    let mut vals = GradingTone::new(GradingStyle::Log);
    vals.s_contrast = 1.1;
    gt_transform.set_value(vals).unwrap();
    assert_eq!(gto.value().s_contrast, 1.0);

    let proc = processor(gt_transform, TransformDirection::Forward);
    assert!(proc.has_dynamic_property(DynamicPropertyType::GradingTone));
    assert!(!proc.has_dynamic_property(DynamicPropertyType::Exposure));

    let cpu = proc.default_cpu_processor();
    let dp = cpu
        .dynamic_property(DynamicPropertyType::GradingTone)
        .unwrap();
    let dpgt = dp.as_grading_tone().unwrap();

    // Set identity value in dynamic property.
    vals.s_contrast = 1.0;
    dpgt.set(vals);

    let mut pixel = [0.0f32, 0.2, 2.0];
    cpu.apply_rgb(&mut pixel);
    assert_eq!(pixel, [0.0, 0.2, 2.0]);

    // Change values and update dynamic property.
    vals.s_contrast = 1.1;
    vals.midtones.red = 1.1;
    dpgt.set(vals);
    cpu.apply_rgb(&mut pixel);
    let error = 1e-5f32;
    assert!((pixel[0] - 0.0).abs() < error);
    assert!((pixel[1] - 0.18729).abs() < error);
    assert!((pixel[2] - 1.91875).abs() < error);
}

#[test]
fn pair_identity_optimization() {
    let mut v = GradingTone::new(GradingStyle::Log);
    v.midtones.red = 1.3;
    let fwd = GradingToneOp::new(GradingStyle::Log, v, TransformDirection::Forward, false).unwrap();
    let ops: OpVec = vec![Arc::new(fwd.clone()), Arc::new(fwd.inverse())];
    assert!(optimize_ops(&ops, OptimizationFlags::DEFAULT).is_empty());
    assert_eq!(optimize_ops(&ops, OptimizationFlags::NONE).len(), 2);
}

// GradingToneOpCPU_tests.cpp

#[test]
fn cpu_identity() {
    let qnan = f32::NAN;
    let inf = f32::INFINITY;
    // inf is clamp, so inverse would fail.
    #[rustfmt::skip]
    let image = [
        -0.50, -0.25, 0.50, 0.0,
         0.75, 1.00, 1.25, 1.0,
      65000.0, 1.50, -65000.0, 0.0,
         qnan, qnan, qnan, 0.0,
          0.0, 0.0, 0.0, qnan,
          0.0, 0.0, 0.0, inf,
         -inf, -inf, -inf, 0.0,
          0.0, 0.0, 0.0, -inf,
    ];
    for style in [GradingStyle::Log, GradingStyle::Lin, GradingStyle::Video] {
        let op = GradingToneOp::identity(style);
        validate_image(&image, &apply_op(&op, &image), line!());
        let op = op.inverse();
        validate_image(&image, &apply_op(&op, &image), line!());
    }
}

fn check_fwd_inv(
    style: GradingStyle,
    gtd: GradingTone,
    input: &[f32],
    expected: &[f32],
    line: u32,
) {
    let op = GradingToneOp::new(style, gtd, TransformDirection::Forward, true).unwrap();
    validate_image(expected, &apply_op(&op, input), line);
    let op = GradingToneOp::new(style, gtd, TransformDirection::Inverse, true).unwrap();
    validate_image(input, &apply_op(&op, expected), line);
}

#[test]
fn cpu_log_midtones() {
    let style = GradingStyle::Log;
    let mut gtd = GradingTone::new(style);
    gtd.midtones = GradingRgbmsw::new(0.3, 1.0, 1.8, 1.2, 0.47, 0.6);
    #[rustfmt::skip]
    let input = [
        0.1, -0.4, 0.9, 1.0,
        0.3, 0.6, 0.7, 0.5,
        0.8, 2.2, 0.5, 0.0];
    #[rustfmt::skip]
    let expected = [
        0.09440361, -0.40000000, 0.90645507, 1.0,
        0.23564218, 0.62838000, 0.76080927, 0.5,
        0.78783701, 2.20000000, 0.67159981, 0.0];
    check_fwd_inv(style, gtd, &input, &expected, line!());
}

#[test]
fn cpu_log_highlights() {
    let style = GradingStyle::Log;
    let mut gtd = GradingTone::new(style);
    gtd.highlights = GradingRgbmsw::new(0.3, 1.0, 1.8, 1.4, -0.1, 0.9);
    #[rustfmt::skip]
    let input = [
         0.8, 0.2, -0.05, 1.0,
        -0.4, 0.7, 0.8, 0.5,
         0.5, 1.0, 2.2, 0.0];
    #[rustfmt::skip]
    let expected = [
         0.75833820, 0.21800000, -0.04847980, 1.0,
        -0.40000000, 0.75600000, 0.88018560, 0.5,
         0.46114011, 0.96000000, 1.05600000, 0.0];
    check_fwd_inv(style, gtd, &input, &expected, line!());
}

#[test]
fn cpu_video_shadows() {
    let style = GradingStyle::Video;
    let mut gtd = GradingTone::new(style);
    gtd.shadows = GradingRgbmsw::new(0.3, 1., 1.79, 0.6, 0.8, -0.1);
    #[rustfmt::skip]
    let input = [
        -0.05, -0.3, -0.05, 1.0,
         0.20, 0.2, 0.10, 0.5,
         0.50, 1.2, 0.40, 0.0];
    #[rustfmt::skip]
    let expected = [
        -0.08903600, -0.22000000, -0.0101064, 1.0,
         0.04235000, 0.14000000, 0.158287734, 0.5,
         0.44006111, 1.20000000, 0.426106364, 0.0];
    check_fwd_inv(style, gtd, &input, &expected, line!());
}

#[test]
fn cpu_video_white_details() {
    let style = GradingStyle::Video;
    let mut gtd = GradingTone::new(style);
    gtd.whites = GradingRgbmsw::new(0.3, 1., 1.9, 0.6, -0.2, 1.4);
    #[rustfmt::skip]
    let input = [
        0.9, -0.4, 0.8, 1.0,
        1.2, 0.8, 1.0, 0.5,
        8.0, 4.0, 2.0, 0.0];
    #[rustfmt::skip]
    let expected = [
        0.50664196, -0.40000000, 0.85713846, 1.0,
        0.59170000, 0.65714286, 1.11661389, 0.5,
        1.85000000, 2.60000000, 17.73099488, 0.0];
    check_fwd_inv(style, gtd, &input, &expected, line!());
}

#[test]
fn cpu_log_black_details() {
    let style = GradingStyle::Log;
    let mut gtd = GradingTone::new(style);
    gtd.blacks = GradingRgbmsw::new(0.3, 1., 1.9, 0.6, 0.8, 0.9);
    #[rustfmt::skip]
    let input = [
        -0.05, -0.5, -0.20, 1.0,
         0.05, 0.0, -0.05, 0.5,
         0.40, 1.2, 0.40, 0.0];
    #[rustfmt::skip]
    let expected = [
        -0.88574485, -0.99166667, 0.23906196, 1.0,
        -0.50105701, -0.16583916, 0.25926968, 0.5,
         0.30488108, 1.20000000, 0.45937302, 0.0];
    check_fwd_inv(style, gtd, &input, &expected, line!());
}

#[test]
fn cpu_log_scontrast() {
    let style = GradingStyle::Log;
    #[rustfmt::skip]
    let input = [
         0.15, 0.3, 0.42, 1.0,
        -0.1, 0.6, 1.2, 0.5,
         0.8, 0.0, 1.0, 0.0];
    #[rustfmt::skip]
    let expected = [
         0.05250000, 0.15283050, 0.45714286, 1.0,
        -0.03500000, 0.83910667, 1.07000000, 0.5,
         0.93000000, 0.00000000, 1.00000000, 0.0];
    #[rustfmt::skip]
    let input2 = [
         0.04, 0.3, 0.15, 1.0,
        -0.1, 0.6, 1.2, 0.5,
         0.8, 0.0, 1.0, 0.0];
    #[rustfmt::skip]
    let expected2 = [
         0.08050314, 0.35031250, 0.26213396, 1.0,
        -0.20125786, 0.49937500, 1.40251572, 0.5,
         0.63561388, 0.00000000, 1.00000000, 0.0];

    let mut gtd = GradingTone::new(style);
    gtd.s_contrast = 1.8;
    check_fwd_inv(style, gtd, &input, &expected, line!());
    gtd.s_contrast = 0.3;
    check_fwd_inv(style, gtd, &input2, &expected2, line!());
}

#[test]
fn cpu_lin_midtones() {
    let style = GradingStyle::Lin;
    let mut gtd = GradingTone::new(style);
    gtd.midtones = GradingRgbmsw::new(0.3, 1.4, 1.8, 1., 1., 8.);
    #[rustfmt::skip]
    let input = [
        0.1, -0.1, 0.90, 1.0,
        0.3, 0.6, 0.70, 0.5,
        0.8, 1.5, 0.05, 0.0];
    #[rustfmt::skip]
    let expected = [
        0.04102994, -0.10000000, 3.07542735, 1.0,
        0.08530666, 1.19569300, 2.65221218, 0.5,
        0.26080380, 2.37429354, 0.08896667, 0.0];
    check_fwd_inv(style, gtd, &input, &expected, line!());
}

#[test]
fn cpu_dynamic_update() {
    // The precomputed values follow the dynamic property.
    let style = GradingStyle::Log;
    let op = GradingToneOp::new(
        style,
        GradingTone::new(style),
        TransformDirection::Forward,
        true,
    )
    .unwrap();
    let dp = op
        .dynamic_property(DynamicPropertyType::GradingTone)
        .unwrap();
    let input = [0.8f32, 0.2, -0.05, 1.0];
    validate_image(&input, &apply_op(&op, &input), line!());

    let mut gtd = GradingTone::new(style);
    gtd.highlights = GradingRgbmsw::new(0.3, 1.0, 1.8, 1.4, -0.1, 0.9);
    dp.as_grading_tone().unwrap().set(gtd);
    let expected = [0.75833820f32, 0.21800000, -0.04847980, 1.0];
    validate_image(&expected, &apply_op(&op, &input), line!());

    // An invalid value is ignored.
    let mut bad = gtd;
    bad.s_contrast = 5.0;
    dp.as_grading_tone().unwrap().set(bad);
    validate_image(&expected, &apply_op(&op, &input), line!());
}

// GradingToneTransform_tests.cpp

#[test]
fn transform_basic() {
    let mut gtt_lin = GradingToneTransform::new(GradingStyle::Lin);
    assert_eq!(gtt_lin.style, GradingStyle::Lin);
    assert_eq!(gtt_lin.value, GradingTone::new(GradingStyle::Lin));

    let mut tone = GradingTone::new(GradingStyle::Lin);
    tone.s_contrast += 0.123;
    tone.blacks.red += 0.321;
    tone.blacks.start += 0.1;
    gtt_lin.set_value(tone).unwrap();
    assert_eq!(gtt_lin.value, tone);
    assert_eq!(gtt_lin.direction, TransformDirection::Forward);
    gtt_lin.direction = TransformDirection::Inverse;

    assert!(!gtt_lin.is_dynamic());
    gtt_lin.make_dynamic();
    assert!(gtt_lin.is_dynamic());
    gtt_lin.make_non_dynamic();
    assert!(!gtt_lin.is_dynamic());
    assert!(gtt_lin.validate().is_ok());

    tone.blacks.width = 0.0001;
    assert!(gtt_lin
        .set_value(tone)
        .unwrap_err()
        .message()
        .contains("is below lower bound (0.01)"));
    tone.blacks.width = 1.;
    tone.blacks.red = 2.1;
    assert!(gtt_lin
        .set_value(tone)
        .unwrap_err()
        .message()
        .contains("are above upper bound (1.9)"));

    let gtt_log = GradingToneTransform::new(GradingStyle::Log);
    assert_eq!(gtt_log.value, GradingTone::new(GradingStyle::Log));
    let gtt_vid = GradingToneTransform::new(GradingStyle::Video);
    assert_eq!(gtt_vid.value, GradingTone::new(GradingStyle::Video));

    // Changing the style resets the values.
    let mut t = GradingToneTransform::new(GradingStyle::Lin);
    t.set_value(GradingTone {
        s_contrast: 1.5,
        ..GradingTone::new(GradingStyle::Lin)
    })
    .unwrap();
    t.set_style(GradingStyle::Lin);
    assert_eq!(t.value.s_contrast, 1.5);
    t.set_style(GradingStyle::Log);
    assert_eq!(t.value, GradingTone::new(GradingStyle::Log));

    // Validation errors are prefixed.
    let mut t = GradingToneTransform::new(GradingStyle::Log);
    t.value.s_contrast = 2.5;
    assert_eq!(
        t.validate().unwrap_err().message(),
        "GradingToneTransform validation failed: GradingTone s-contrast '2.5' is above upper bound (1.99)."
    );
    let mut ops = OpVec::new();
    assert!(t
        .build_ops(
            &mut ops,
            &Config::create_raw(),
            &Context::new(),
            TransformDirection::Forward
        )
        .is_err());
}

#[test]
fn transform_serialization() {
    let mut data = GradingTone::new(GradingStyle::Lin);
    data.s_contrast += 0.123;
    data.blacks.red += 0.321;
    data.blacks.start += 0.1;
    let mut tone = GradingToneTransform::new(GradingStyle::Lin);
    tone.set_value(data).unwrap();
    let expected = "<GradingToneTransform direction=forward, style=linear, values=<\
                    blacks=<red=1.321 green=1 blue=1 master=1 start=0.1 width=4> \
                    shadows=<red=1 green=1 blue=1 master=1 start=2 width=-7> \
                    midtones=<red=1 green=1 blue=1 master=1 start=0 width=8> \
                    highlights=<red=1 green=1 blue=1 master=1 start=-2 width=9> \
                    whites=<red=1 green=1 blue=1 master=1 start=0 width=8> s_contrast=1.123>>";
    assert_eq!(format!("{tone}"), expected);
}

#[test]
fn transform_local_bypass() {
    let mut transform = GradingToneTransform::new(GradingStyle::Log);
    transform.make_dynamic();
    let proc = processor(transform, TransformDirection::Forward);
    let cpu = proc.optimized_cpu_processor(OptimizationFlags::NONE);
    let error = 1e-6f32;
    let close = |a: f32, b: f32| assert!((a - b).abs() <= error, "{a} != {b}");

    // Values are unchanged (to within 32f precision).
    let mut v1 = [0.3f32, 0.4, 0.5];
    cpu.apply_rgb(&mut v1);
    close(v1[0], 0.30000001192092896);
    close(v1[1], 0.4000000059604645);
    assert_eq!(v1[2], 0.5);

    // A value > HalfMax = 65504 is not clamped: localBypass is being used.
    let mut v2 = [0.3f32, 0.4, 65550.0];
    cpu.apply_rgb(&mut v2);
    assert_eq!(v2[2], 65550.0);

    // Set the midtones control so it is no longer an identity.
    let mut valst = GradingTone::new(GradingStyle::Log);
    valst.midtones = GradingRgbmsw::new(0.3, 1.0, 1.8, 1.2, 0.37, 0.6);
    let dp = cpu
        .dynamic_property(DynamicPropertyType::GradingTone)
        .unwrap();
    let prop = dp.as_grading_tone().unwrap();
    prop.set(valst);

    let mut v3 = [0.3f32, 0.4, 0.5];
    cpu.apply_rgb(&mut v3);
    close(v3[0], 0.20410963892936707);
    close(v3[1], 0.4343799948692322);
    close(v3[2], 0.6093657612800598);

    // The max value is now clamped, so localBypass is not being used.
    let mut v4 = [0.3f32, 0.4, 65550.0];
    cpu.apply_rgb(&mut v4);
    close(v4[0], 0.20410963892936707);
    close(v4[1], 0.4343799948692322);
    assert_eq!(v4[2], 65504.0);

    let mut v5 = [0.3f32, 0.4, 65500.0];
    cpu.apply_rgb(&mut v5);
    assert_eq!(v5[2], 65500.0);

    // Set the midtones values back to their default.
    valst.midtones = GradingRgbmsw::new(1., 1.0, 1., 1., 0.4, 0.6);
    prop.set(valst);
    let mut v6 = [0.3f32, 0.4, 0.5];
    cpu.apply_rgb(&mut v6);
    assert_eq!(v6, [0.3, 0.4, 0.5]);
    let mut v7 = [0.3f32, 0.4, 65550.0];
    cpu.apply_rgb(&mut v7);
    assert_eq!(v7[2], 65550.0);
}

#[test]
fn transform_create_group_transform() {
    let mut t = GradingToneTransform::new(GradingStyle::Video);
    t.value.whites.red = 1.2;
    t.direction = TransformDirection::Inverse;
    let proc = processor(t.clone(), TransformDirection::Forward);
    let grp = proc.create_group_transform();
    assert_eq!(grp.transforms, vec![Transform::GradingTone(t.clone())]);

    // Inverse processor inverts the direction.
    let proc = processor(t.clone(), TransformDirection::Inverse);
    let grp = proc.create_group_transform();
    t.direction = TransformDirection::Forward;
    assert_eq!(grp.transforms, vec![Transform::GradingTone(t)]);
}
