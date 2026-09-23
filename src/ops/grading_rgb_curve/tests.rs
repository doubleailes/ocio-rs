//! Tests ported from `GradingRGBCurveOpData_tests.cpp`, `GradingRGBCurveOp_tests.cpp`,
//! `GradingRGBCurveOpCPU_tests.cpp`, `GradingRGBCurveTransform_tests.cpp` and
//! the RGB curve parts of `DynamicProperty_tests.cpp`.

#![allow(clippy::excessive_precision)]

use super::*;
use crate::ops::grading_primary::tests::{processor, to_pixels, validate_image};
use crate::processor::optimize_ops;
use crate::transforms::grading::{GradingBSplineCurve, GradingControlPoint};
use crate::transforms::GroupTransform;
use crate::types::BSplineType;

/// Non SSE tolerance of `GradingRGBCurveOpCPU_tests.cpp`.
const ERROR: f32 = 2e-5;

fn curve(points: &[(f32, f32)]) -> GradingBSplineCurve {
    GradingBSplineCurve::new(points, BSplineType::BSpline)
}

fn rgb(
    r: &GradingBSplineCurve,
    g: &GradingBSplineCurve,
    b: &GradingBSplineCurve,
    m: &GradingBSplineCurve,
) -> GradingRgbCurve {
    GradingRgbCurve::from_curves(r.clone(), g.clone(), b.clone(), m.clone())
}

fn apply_op(op: &dyn Op, input: &[f32]) -> Vec<Pixel> {
    let mut px = to_pixels(input);
    op.apply(&mut px);
    px
}

fn check_fwd_inv(
    style: GradingStyle,
    value: &GradingRgbCurve,
    bypass: bool,
    input: &[f32],
    expected: &[f32],
    line: u32,
) {
    let op = GradingRgbCurveOp::new(
        style,
        value.clone(),
        TransformDirection::Forward,
        bypass,
        false,
    )
    .unwrap();
    validate_image(expected, &apply_op(&op, input), ERROR, line);
    let op = GradingRgbCurveOp::new(
        style,
        value.clone(),
        TransformDirection::Inverse,
        bypass,
        false,
    )
    .unwrap();
    validate_image(input, &apply_op(&op, expected), ERROR, line);
}

// GradingRGBCurveOpData_tests.cpp

#[test]
fn op_data_accessors() {
    let gc = GradingRgbCurveOp::identity(GradingStyle::Log);
    let expected = "<GradingRGBCurveOp log forward \
                    <red=<control_points=[<x=0, y=0><x=0.5, y=0.5><x=1, y=1>]>, \
                    green=<control_points=[<x=0, y=0><x=0.5, y=0.5><x=1, y=1>]>, \
                    blue=<control_points=[<x=0, y=0><x=0.5, y=0.5><x=1, y=1>]>, \
                    master=<control_points=[<x=0, y=0><x=0.5, y=0.5><x=1, y=1>]>>>";
    assert_eq!(gc.cache_id(), expected);
    assert_eq!(gc.style(), GradingStyle::Log);
    assert!(gc.value().is_identity());
    assert!(gc.is_identity());
    assert!(gc.is_no_op());
    assert!(!gc.has_channel_crosstalk());
    assert!(!gc.bypass_lin_to_log());

    let gc = GradingRgbCurveOp::new(
        GradingStyle::Lin,
        GradingRgbCurve::new(GradingStyle::Lin),
        TransformDirection::Inverse,
        true,
        true,
    )
    .unwrap();
    assert!(gc.bypass_lin_to_log());
    assert!(gc.is_dynamic());
    assert_eq!(gc.direction(), TransformDirection::Inverse);
    let dp = gc
        .dynamic_property(DynamicPropertyType::GradingRgbCurve)
        .unwrap();
    assert_eq!(dp.property_type(), DynamicPropertyType::GradingRgbCurve);
    assert_eq!(
        gc.cache_id(),
        "<GradingRGBCurveOp linear inverse  bypassLinToLog>"
    );

    // Values with a custom red curve.
    let mut v1 = GradingRgbCurve::new(GradingStyle::Log);
    {
        let red = v1.curve_mut(RgbCurveType::Red);
        red.set_num_control_points(4);
        red.control_points[3] =
            GradingControlPoint::new(red.control_points[2].x + 1.0, red.control_points[2].y + 0.5);
    }
    v1.curve_mut(RgbCurveType::Blue).set_slope(2, 0.9);
    let gc1 = GradingRgbCurveOp::new(
        GradingStyle::Log,
        v1.clone(),
        TransformDirection::Inverse,
        false,
        false,
    )
    .unwrap();
    assert_eq!(gc1.value().curve(RgbCurveType::Blue).slope(2), 0.9);
    assert!(gc1.value().curve(RgbCurveType::Green).slopes_are_default());
    assert!(!gc1.value().curve(RgbCurveType::Blue).slopes_are_default());
    assert!(!gc1.is_identity());
    assert!(!gc1.has_channel_crosstalk());

    // isInverse.
    let mut v3 = GradingRgbCurve::new(GradingStyle::Lin);
    {
        let spline = v3.curve_mut(RgbCurveType::Red);
        spline.set_num_control_points(2);
        spline.control_points[0] = GradingControlPoint::new(0.0, 2.0);
        spline.control_points[1] = GradingControlPoint::new(0.9, 2.0);
    }
    let gc3 = GradingRgbCurveOp::new(
        GradingStyle::Lin,
        v3.clone(),
        TransformDirection::Forward,
        false,
        false,
    )
    .unwrap();
    assert!(!gc3.is_identity());
    let gc3_inv = gc3.inverse();
    assert!(gc3_inv.is_inverse(&gc3));

    // Change value of one: no longer an inverse.
    let mut v3b = v3.clone();
    v3b.curve_mut(RgbCurveType::Red).control_points[1].y += 0.25;
    let other = GradingRgbCurveOp::new(
        GradingStyle::Lin,
        v3b,
        TransformDirection::Inverse,
        false,
        false,
    )
    .unwrap();
    assert!(!other.is_inverse(&gc3));
    // Change slope of one: no longer an inverse.
    let mut v3c = v3.clone();
    v3c.curve_mut(RgbCurveType::Blue).set_slope(2, 0.9);
    let other = GradingRgbCurveOp::new(
        GradingStyle::Lin,
        v3c,
        TransformDirection::Inverse,
        false,
        false,
    )
    .unwrap();
    assert!(!other.is_inverse(&gc3));
    // Different bypass in linear style: no longer an inverse.
    let other = GradingRgbCurveOp::new(
        GradingStyle::Lin,
        v3.clone(),
        TransformDirection::Inverse,
        true,
        false,
    )
    .unwrap();
    assert!(!other.is_inverse(&gc3));
    // Same direction: no longer an inverse.
    assert!(!gc3.is_inverse(&gc3));
    // Bypass is ignored for other styles.
    let log = GradingRgbCurveOp::new(
        GradingStyle::Log,
        v3.clone(),
        TransformDirection::Forward,
        false,
        false,
    )
    .unwrap();
    let log_inv = GradingRgbCurveOp::new(
        GradingStyle::Log,
        v3,
        TransformDirection::Inverse,
        true,
        false,
    )
    .unwrap();
    assert!(log.is_inverse(&log_inv));
}

#[test]
fn op_data_validate() {
    let new = |v: GradingRgbCurve| {
        GradingRgbCurveOp::new(
            GradingStyle::Log,
            v,
            TransformDirection::Forward,
            false,
            false,
        )
    };
    assert!(new(GradingRgbCurve::new(GradingStyle::Log)).is_ok());

    let c = GradingBSplineCurve::with_size(1, BSplineType::BSpline);
    let err = new(rgb(&c, &c, &c, &c)).unwrap_err();
    assert!(err
        .message()
        .contains("There must be at least 2 control points."));

    let mut c = curve(&[(0.0, 0.0), (0.7, 0.3), (0.5, 0.7), (1.0, 1.0)]);
    let err = new(rgb(&c, &c, &c, &c)).unwrap_err();
    assert!(err.message().contains(
        "has a x coordinate '0.5' that is less than previous control point x coordinate '0.7'."
    ));
    c.control_points[1].x = 0.3;
    assert!(new(rgb(&c, &c, &c, &c)).is_ok());

    let c = curve(&[(0.0, 0.0), (0.3, 0.3), (0.5, 0.27), (1.0, 1.0)]);
    let err = new(rgb(&c, &c, &c, &c)).unwrap_err();
    assert!(err
        .message()
        .contains("point at index 2 has a y coordinate '0.27' that is less than previous control point y coordinate '0.3'."));

    let c = GradingBSplineCurve::new_for_hue_curve(
        &[(0.0, 0.0), (0.9, 0.0)],
        crate::types::HueCurveType::HueFx,
    );
    let err = new(rgb(&c, &c, &c, &c)).unwrap_err();
    assert!(err
        .message()
        .contains("validation failed: 'red' curve is of the wrong BSplineType."));
}

#[test]
fn op_data_dynamic() {
    let op = GradingRgbCurveOp::new(
        GradingStyle::Log,
        GradingRgbCurve::new(GradingStyle::Log),
        TransformDirection::Forward,
        false,
        true,
    )
    .unwrap();
    let dp = op
        .dynamic_property(DynamicPropertyType::GradingRgbCurve)
        .unwrap();
    assert!(op
        .dynamic_property(DynamicPropertyType::GradingTone)
        .is_none());
    let c = curve(&[(0.0, 0.1), (0.2, 0.3), (0.5, 0.8), (2.0, 1.5)]);
    dp.as_grading_rgb_curve().unwrap().set(rgb(&c, &c, &c, &c));
    assert_eq!(
        op.value().curve(RgbCurveType::Green).num_control_points(),
        4
    );
    assert!(op.knots_coefs().num_knots > 0);

    let non_dyn = op.make_non_dynamic().unwrap();
    assert!(!non_dyn.is_dynamic());
    let non_dyn = non_dyn.downcast_ref::<GradingRgbCurveOp>().unwrap();
    assert_eq!(
        non_dyn
            .value()
            .curve(RgbCurveType::Green)
            .num_control_points(),
        4
    );
}

// GradingRGBCurveOp_tests.cpp

#[test]
fn op_create() {
    let mut ops = OpVec::new();
    let v = GradingRgbCurve::new(GradingStyle::Log);
    let fwd = TransformDirection::Forward;
    create_grading_rgb_curve_op(&mut ops, GradingStyle::Log, &v, fwd, false, false, fwd).unwrap();
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].name(), "GradingRGBCurve");
    assert!(ops[0].is_identity());
    assert!(ops[0].is_no_op());

    create_grading_rgb_curve_op(&mut ops, GradingStyle::Log, &v, fwd, false, true, fwd).unwrap();
    assert_eq!(ops.len(), 2);
    assert!(!ops[1].is_identity());
    assert!(!ops[1].is_no_op());
}

#[test]
fn op_create_transform() {
    let op = GradingRgbCurveOp::new(
        GradingStyle::Log,
        GradingRgbCurve::new(GradingStyle::Log),
        TransformDirection::Forward,
        false,
        true,
    )
    .unwrap();
    match op.to_transform().unwrap() {
        Transform::GradingRgbCurve(t) => {
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
    let mut gc_transform = GradingRgbCurveTransform::new(GradingStyle::Log);

    let mut ops = OpVec::new();
    gc_transform
        .build_ops(&mut ops, &config, &context, TransformDirection::Forward)
        .unwrap();
    assert_eq!(ops.len(), 1);
    assert!(ops[0].is_identity());
    assert!(ops[0].is_no_op());
    ops.clear();

    gc_transform.make_dynamic();
    gc_transform
        .build_ops(&mut ops, &config, &context, TransformDirection::Forward)
        .unwrap();
    assert_eq!(ops.len(), 1);
    let gco = ops[0].downcast_ref::<GradingRgbCurveOp>().unwrap();
    assert!(gco.is_dynamic());
    assert_eq!(
        3,
        gco.value().curve(RgbCurveType::Green).num_control_points()
    );

    // Create processor with dynamic identity before changing the transform.
    let proc = processor(gc_transform.clone(), TransformDirection::Forward);
    assert!(proc.has_dynamic_property(DynamicPropertyType::GradingRgbCurve));
    assert!(!proc.has_dynamic_property(DynamicPropertyType::Exposure));
    let cpu = proc.default_cpu_processor();

    let c = curve(&[(0.0, 0.1), (0.2, 0.3), (0.5, 0.8), (2.0, 1.5)]);
    let rgb_curve = rgb(&c, &c, &c, &c);
    gc_transform.set_value(rgb_curve.clone()).unwrap();
    // Still use the default identity curves.
    assert_eq!(
        3,
        gco.value().curve(RgbCurveType::Green).num_control_points()
    );

    let dp = cpu
        .dynamic_property(DynamicPropertyType::GradingRgbCurve)
        .unwrap();
    let dpgc = dp.as_grading_rgb_curve().unwrap();

    let mut pixel = [0.0f32, 0.2, 2.0];
    cpu.apply_rgb(&mut pixel);
    let error = 1e-5f32;
    assert!((pixel[0] - 0.0).abs() < error);
    assert!((pixel[1] - 0.2).abs() < error);
    assert!((pixel[2] - 2.0).abs() < error);

    // Use other curve that has 4 control points.
    dpgc.set(rgb_curve);
    cpu.apply_rgb(&mut pixel);
    assert!((pixel[0] - 0.18597151).abs() < error, "{pixel:?}");
    assert!((pixel[1] - 0.47056902).abs() < error, "{pixel:?}");
    assert!((pixel[2] - 1.32527864).abs() < error, "{pixel:?}");
}

#[test]
fn pair_identity_optimization() {
    let c = curve(&[(0.0, 0.1), (0.2, 0.3), (0.5, 0.8), (2.0, 1.5)]);
    let v = rgb(&c, &c, &c, &c);
    let fwd = GradingRgbCurveOp::new(
        GradingStyle::Log,
        v,
        TransformDirection::Forward,
        false,
        false,
    )
    .unwrap();
    let ops: OpVec = vec![Arc::new(fwd.clone()), Arc::new(fwd.inverse())];
    assert!(optimize_ops(&ops, OptimizationFlags::DEFAULT).is_empty());
    assert_eq!(optimize_ops(&ops, OptimizationFlags::NONE).len(), 2);
}

// GradingRGBCurveOpCPU_tests.cpp

#[test]
fn cpu_identity() {
    let qnan = f32::NAN;
    let inf = f32::INFINITY;
    #[rustfmt::skip]
    let image = [
        -0.50, -0.25, 0.50, 0.0,
         0.75, 1.00, 1.25, 1.0,
         1.25, 1.50, 1.75, 0.0,
         qnan, qnan, qnan, 0.0,
          0.0, 0.0, 0.0, qnan,
          inf, inf, inf, 0.0,
          0.0, 0.0, 0.0, inf,
         -inf, -inf, -inf, 0.0,
          0.0, 0.0, 0.0, -inf,
    ];
    for style in [GradingStyle::Lin, GradingStyle::Video, GradingStyle::Log] {
        for bypass in [false, true] {
            for dir in [TransformDirection::Forward, TransformDirection::Inverse] {
                let op =
                    GradingRgbCurveOp::new(style, GradingRgbCurve::new(style), dir, bypass, false)
                        .unwrap();
                validate_image(&image, &apply_op(&op, &image), ERROR, line!());
            }
        }
    }
}

#[test]
fn cpu_log() {
    let r = curve(&[(0.1, 0.15), (0.55, 0.45), (0.9, 1.1)]);
    let g = curve(&[(0.1, 0.15), (0.55, 0.35), (0.9, 1.1)]);
    let b = curve(&[(0.1, 0.15), (0.55, 0.85), (0.9, 1.1)]);
    let m = curve(&[(-0.1, 0.1), (1.1, 1.3)]);
    #[rustfmt::skip]
    let input = [
        -0.2, 0.2, 0.5, 0.0,
         0.8, 1.0, 2.0, 0.5];
    #[rustfmt::skip]
    let expected = [
        0.25306581, 0.35779659, 0.98416632, 0.0,
        1.09451043, 1.54596428, 1.78067802, 0.5];
    check_fwd_inv(
        GradingStyle::Log,
        &rgb(&r, &g, &b, &m),
        false,
        &input,
        &expected,
        line!(),
    );
}

#[test]
fn cpu_log_partial_identity() {
    let r = curve(&[(0.1, 0.1), (0.9, 0.9)]);
    let g = curve(&[(0.1, 0.15), (0.55, 0.35), (0.9, 1.1)]);
    let b = curve(&[(0.0, 0.0), (0.5, 0.5), (1.0, 1.0)]);
    let m = curve(&[(0.1, 0.1), (1.1, 1.1)]);
    #[rustfmt::skip]
    let input = [
        -0.2, 0.2, 0.5, 0.0,
         0.8, 1.0, 2.0, 0.5];
    #[rustfmt::skip]
    let expected = [
        -0.2, 0.15779659, 0.5, 0.0,
         0.8, 1.34596419, 2.0, 0.5];
    check_fwd_inv(
        GradingStyle::Log,
        &rgb(&r, &g, &b, &m),
        false,
        &input,
        &expected,
        line!(),
    );
}

#[test]
fn cpu_monotonic() {
    let r = curve(&[
        (0.0, 0.0),
        (0.785, 0.231),
        (0.809, 0.631),
        (0.948, 0.704),
        (1.0, 1.0),
    ]);
    let g = curve(&[(-0.1, -0.1), (1.1, 1.1)]);
    #[rustfmt::skip]
    let input = [
        0.8, 0.2, 0.5, 0.0,
        0.9, 1.0, 2.0, 0.5];
    #[rustfmt::skip]
    let expected = [
        0.52230538, 0.2, 0.5, 0.0,
        0.68079938, 1.0, 2.0, 0.5];
    check_fwd_inv(
        GradingStyle::Log,
        &rgb(&r, &g, &g, &g),
        false,
        &input,
        &expected,
        line!(),
    );
}

#[test]
fn cpu_lin_bypass() {
    let c = curve(&[(-6.0, -8.0), (-2.0, -5.0), (2.0, 4.0), (5.0, 6.0)]);
    let m = curve(&[(0.0, 0.0), (0.5, 0.5), (1.0, 1.0)]);
    #[rustfmt::skip]
    let input = [
        -8.0, -3.0, -1.0, 0.0,
         1.0, 2.5, 4.0, 0.5];
    #[rustfmt::skip]
    let expected = [
        -8.50508935, -6.37181915, -3.01264257, 0.0,
         1.95205522, 4.76796850, 5.76796850, 0.5];
    check_fwd_inv(
        GradingStyle::Lin,
        &rgb(&c, &c, &c, &m),
        true,
        &input,
        &expected,
        line!(),
    );
}

#[test]
fn cpu_lin() {
    let c = curve(&[(-6.0, -8.0), (-2.0, -5.0), (2.0, 4.0), (5.0, 6.0)]);
    let m = curve(&[(0.0, 0.0), (0.5, 0.5), (1.0, 1.0)]);
    #[rustfmt::skip]
    let input = [
        -0.003, 0.02, 0.09, 0.0,
         0.360, 1.00, 3.00, 0.5];
    #[rustfmt::skip]
    let expected: [f32; 8] = [
        -4.20784139e-03, 1.26825221e-03, 2.23983977e-02, 0.0,
         6.96706128e-01, 4.79411018e+00, 9.95152432e+00, 0.5];
    // Larger values are compared with a relative error.
    let v = rgb(&c, &c, &c, &m);
    let op = GradingRgbCurveOp::new(
        GradingStyle::Lin,
        v.clone(),
        TransformDirection::Forward,
        false,
        false,
    )
    .unwrap();
    let res = apply_op(&op, &input);
    for (i, px) in res.iter().enumerate() {
        for j in 0..4 {
            let e = expected[i * 4 + j];
            let tol = ERROR * e.abs().max(1.0);
            assert!((px[j] - e).abs() <= tol, "fwd {i} {j}: {} != {e}", px[j]);
        }
    }
    let op = GradingRgbCurveOp::new(
        GradingStyle::Lin,
        v,
        TransformDirection::Inverse,
        false,
        false,
    )
    .unwrap();
    validate_image(&input, &apply_op(&op, &expected), ERROR, line!());
}

#[test]
fn cpu_slopes() {
    let mut m = curve(&[
        (-5.26017743, -4.0),
        (-3.75502745, -3.57868829),
        (-2.24987747, -1.82131329),
        (-0.74472749, 0.68124124),
        (1.06145248, 2.87457742),
        (2.86763245, 3.83406206),
        (4.67381243, 4.0),
    ]);
    let slopes = [
        0.0f32, 0.55982688, 1.77532247, 1.55, 0.8787017, 0.18374463, 0.0,
    ];
    for (i, s) in slopes.iter().enumerate() {
        m.set_slope(i, *s);
    }
    assert!(m.validate().is_ok());
    let z = curve(&[(0.0, 0.0), (1.0, 1.0)]);
    let v = rgb(&z, &z, &z, &m);

    #[rustfmt::skip]
    let input = [
        -3.0, -1.0, 1.0, 0.5,
        -7.0, 0.0, 7.0, 1.0];
    // The slopes are used (the values are significantly different without slopes).
    #[rustfmt::skip]
    let expected = [
        -2.92582282, 0.28069129, 2.81987724, 0.5,
        -4.0, 1.73250193, 4.0, 1.0];
    let op = GradingRgbCurveOp::new(
        GradingStyle::Log,
        v.clone(),
        TransformDirection::Forward,
        false,
        false,
    )
    .unwrap();
    validate_image(&expected, &apply_op(&op, &input), ERROR, line!());

    #[rustfmt::skip]
    let rev_input = [
        -2.92582282, 0.28069129, 2.81987724, 0.5,
        -7.0, 1.73250193, 7.0, 1.0];
    #[rustfmt::skip]
    let rev_expected = [
        -3.0, -1.0, 1.0, 0.5,
        -5.26017743, 0.0, 4.67381243, 1.0];
    let op = GradingRgbCurveOp::new(
        GradingStyle::Log,
        v,
        TransformDirection::Inverse,
        false,
        false,
    )
    .unwrap();
    validate_image(&rev_expected, &apply_op(&op, &rev_input), ERROR, line!());
}

// DynamicProperty_tests.cpp: grading_rgb_curve_knots_coefs

#[test]
fn knots_coefs() {
    let curve11 = curve(&[
        (0., 10.),
        (2., 10.),
        (3., 10.),
        (5., 10.),
        (6., 10.),
        (8., 10.),
        (9., 10.5),
        (11., 15.),
        (12., 50.),
        (14., 60.),
        (15., 85.),
    ]);
    // Identity curve.
    let id = curve(&[(0.0, 0.0), (1.0, 1.0)]);

    // 1 curve with 11 control points used for green.
    let kc = KnotsCoefs::from_rgb_curve(&rgb(&id, &curve11, &id, &id)).unwrap();
    assert_eq!(kc.coefs_offsets, vec![-1, 0, 0, 45, -1, 0, -1, 0]);
    assert_eq!(kc.knots_offsets, vec![-1, 0, 0, 16, -1, 0, -1, 0]);
    assert_eq!(kc.num_coefs, 45);
    assert_eq!(kc.num_knots, 16);

    #[rustfmt::skip]
    let true_coefs: [f32; 45] = [
        0., 0., 0., 0., 0., 0.337645531, 2.74714088, 0.081863299, 643.661987, 17.7471409,
        -37.0891609, -5.69135284, 3.83422971, 59.0043716, 1.69310224,
        0., 0., 0., 0., 0., 0., 0.499999881, 1.92619848, 2.25, 30.9619350, 48.7090759,
        11.6199141, 0.237208843, 7.90566826, 24.9999962,
        10., 10., 10., 10., 10., 10., 10.1851053, 10.5, 14.6296263, 15., 34.9177551, 50.,
        55.9285622, 60., 62.3833008,
    ];
    #[rustfmt::skip]
    let true_knots: [f32; 16] = [
        0., 2., 3., 5., 6., 8., 8.74042130, 9., 10.9776964, 11., 11.5, 12., 13., 14.,
        14.1448565, 15.,
    ];
    // OCIO uses an absolute 1e-6 tolerance: use a relative one for the large values.
    for (i, t) in true_coefs.iter().enumerate() {
        assert!(
            (kc.coefs[i] - t).abs() <= 1e-6 * t.abs().max(1.0),
            "coef {i}: {} != {t}",
            kc.coefs[i]
        );
    }
    for (i, t) in true_knots.iter().enumerate() {
        assert!(
            (kc.knots[i] - t).abs() <= 1e-6 * t.abs().max(1.0),
            "knot {i}: {} != {t}",
            kc.knots[i]
        );
    }

    // Using the 11 control points curve twice.
    let kc2 = KnotsCoefs::from_rgb_curve(&rgb(&curve11, &id, &curve11, &id)).unwrap();
    assert_eq!(kc2.coefs_offsets, vec![0, 45, -1, 0, 45, 45, -1, 0]);
    assert_eq!(kc2.num_coefs, 90);
    assert_eq!(kc2.num_knots, 32);
    for c in 0..45 {
        assert_eq!(kc.coefs[c], kc2.coefs[c]);
        assert_eq!(kc.coefs[c], kc2.coefs[45 + c]);
    }
    for k in 0..16 {
        assert_eq!(kc.knots[k], kc2.knots[k]);
        assert_eq!(kc.knots[k], kc2.knots[16 + k]);
    }
}

#[test]
fn max_ctrl_pnts() {
    let pts: Vec<(f32, f32)> = (0..26).map(|i| (i as f32, 10.0 + (i * i) as f32)).collect();
    let c = curve(&pts);
    let err = GradingRgbCurveOp::new(
        GradingStyle::Log,
        rgb(&c, &c, &c, &c),
        TransformDirection::Forward,
        false,
        false,
    )
    .unwrap_err();
    assert_eq!(
        err.message(),
        "RGB curve: maximum number of control points reached."
    );
}

// GradingRGBCurveTransform_tests.cpp

#[test]
fn transform_basic() {
    let gct_lin = GradingRgbCurveTransform::new(GradingStyle::Lin);
    assert_eq!(gct_lin.style, GradingStyle::Lin);
    assert_eq!(gct_lin.direction, TransformDirection::Forward);
    assert!(!gct_lin.bypass_lin_to_log);
    assert!(!gct_lin.is_dynamic());
    let red = gct_lin.value.curve(RgbCurveType::Red).clone();
    assert_eq!(red.num_control_points(), 3);
    assert_eq!(
        *red.control_point(0).unwrap(),
        GradingControlPoint::new(-7.0, -7.0)
    );
    assert_eq!(
        *red.control_point(1).unwrap(),
        GradingControlPoint::new(0.0, 0.0)
    );
    assert_eq!(
        *red.control_point(2).unwrap(),
        GradingControlPoint::new(7.0, 7.0)
    );
    for c in RgbCurveType::ALL {
        assert_eq!(*gct_lin.value.curve(c), red);
    }
    assert!(gct_lin.validate().is_ok());

    for style in [GradingStyle::Log, GradingStyle::Video] {
        let t = GradingRgbCurveTransform::new(style);
        let red = t.value.curve(RgbCurveType::Red);
        assert_eq!(
            red.control_points,
            vec![
                GradingControlPoint::new(0.0, 0.0),
                GradingControlPoint::new(0.5, 0.5),
                GradingControlPoint::new(1.0, 1.0)
            ]
        );
        assert!(t.validate().is_ok());
    }

    // Change values.
    let mut gct = GradingRgbCurveTransform::new(GradingStyle::Video);
    gct.set_style(GradingStyle::Lin);
    assert_eq!(gct.style, GradingStyle::Lin);
    gct.direction = TransformDirection::Inverse;
    gct.bypass_lin_to_log = true;
    gct.make_dynamic();
    assert!(gct.is_dynamic());
    gct.set_value(gct_lin.value.clone()).unwrap();
    let red = gct.value.curve(RgbCurveType::Red).clone();
    assert_eq!(
        *red.control_point(0).unwrap(),
        GradingControlPoint::new(-7.0, -7.0)
    );
    assert!(gct.validate().is_ok());

    assert_eq!(
        red.control_point(4).unwrap_err().message(),
        "There are '3' control points. '4' is out of bounds."
    );

    // X has to be increasing.
    let invalid = curve(&[(0.0, 0.0), (0.5, 0.2), (0.2, 0.7), (1.0, 1.0)]);
    let err = gct.set_value(rgb(&red, &red, &invalid, &red)).unwrap_err();
    assert!(err.message().contains(
        "has a x coordinate '0.2' that is less than previous control point x coordinate '0.5'."
    ));

    // Check slopes.
    gct.set_slope(RgbCurveType::Blue, 2, 0.9).unwrap();
    assert!(gct.validate().is_ok());
    assert_eq!(gct.slope(RgbCurveType::Blue, 2).unwrap(), 0.9);
    assert_eq!(
        gct.set_slope(RgbCurveType::Blue, 4, 2.0)
            .unwrap_err()
            .message(),
        "There are '3' control points. '4' is out of bounds."
    );
    assert!(gct.slopes_are_default(RgbCurveType::Green));
    assert!(!gct.slopes_are_default(RgbCurveType::Blue));

    // Validation errors are prefixed.
    let mut t = GradingRgbCurveTransform::new(GradingStyle::Log);
    t.value
        .curve_mut(RgbCurveType::Green)
        .control_points
        .truncate(1);
    t.value.curve_mut(RgbCurveType::Green).slopes.truncate(1);
    assert_eq!(
        t.validate().unwrap_err().message(),
        "GradingRGBCurveTransform validation failed: GradingRGBCurve validation failed for 'green' curve \
         with: There must be at least 2 control points."
    );
}

#[test]
fn transform_processor_several_transforms() {
    let src = [0.2f32, 0.3, 0.4];
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
    let c5 = curve(&[(0.0, 0.0), (1.0, 1.0)]);

    let curve_a = rgb(&c1, &c2, &c3, &c5);
    let mut gcta = GradingRgbCurveTransform::new(GradingStyle::Log);
    gcta.set_value(curve_a.clone()).unwrap();

    let apply = |t: &GradingRgbCurveTransform, px: [f32; 3]| {
        let cpu = processor(t.clone(), TransformDirection::Forward).default_cpu_processor();
        let mut p = px;
        cpu.apply_rgb(&mut p);
        p
    };
    let pixel_a = apply(&gcta, src);
    let pixel_aa = apply(&gcta, pixel_a);

    let curve_b = rgb(&c4, &c1, &c2, &c5);
    let mut gctb = GradingRgbCurveTransform::new(GradingStyle::Log);
    gctb.set_value(curve_b.clone()).unwrap();
    let pixel_ab = apply(&gctb, pixel_a);

    gctb.make_dynamic();
    let error = 1e-6f32;

    let mut grp1 = GroupTransform::new();
    gctb.set_value(curve_a).unwrap();
    grp1.append(gcta.clone());
    grp1.append(gctb.clone());
    {
        let cpu = processor(grp1, TransformDirection::Forward).default_cpu_processor();
        let dp = cpu
            .dynamic_property(DynamicPropertyType::GradingRgbCurve)
            .unwrap();
        let dp_val = dp.as_grading_rgb_curve().unwrap();

        let mut pixel = src;
        cpu.apply_rgb(&mut pixel);
        for i in 0..3 {
            assert!((pixel[i] - pixel_aa[i]).abs() <= error);
        }
        dp_val.set(curve_b);
        let mut pixel = src;
        cpu.apply_rgb(&mut pixel);
        for i in 0..3 {
            assert!((pixel[i] - pixel_ab[i]).abs() <= error);
        }
    }

    // Both dynamic: the processor can still be created.
    gcta.make_dynamic();
    let mut grp2 = GroupTransform::new();
    grp2.append(gcta);
    grp2.append(gctb);
    let _ = processor(grp2, TransformDirection::Forward);
}

#[test]
fn transform_serialization() {
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
    let c4 = curve(&[(0.0, 0.0), (1.0, 1.0)]);
    let mut t = GradingRgbCurveTransform::new(GradingStyle::Log);
    t.set_value(rgb(&c1, &c2, &c3, &c4)).unwrap();
    let expected = "<GradingRGBCurveTransform direction=forward, style=log, \
                    values=<red=<control_points=[<x=0, y=0><x=0.2, y=0.2><x=0.5, y=0.7><x=1, y=1>]>, \
                    green=<control_points=[<x=0, y=0.5><x=0.3, y=0.7><x=0.5, y=1.1><x=1, y=1.5>]>, \
                    blue=<control_points=[<x=0, y=-0.5><x=0.2, y=-0.4><x=0.3, y=0.1><x=0.5, y=0.4><x=0.7, y=0.9><x=1, y=1.1>]>, \
                    master=<control_points=[<x=0, y=0><x=1, y=1>]>>>";
    assert_eq!(format!("{t}"), expected);
    t.bypass_lin_to_log = true;
    t.make_dynamic();
    assert!(format!("{t}").ends_with(">>, bypass_lintolog, dynamic>"));
}

#[test]
fn transform_create_group_transform() {
    let c = curve(&[(0.0, 0.1), (0.2, 0.3), (0.5, 0.8), (2.0, 1.5)]);
    let mut t = GradingRgbCurveTransform::new(GradingStyle::Lin);
    t.set_value(rgb(&c, &c, &c, &c)).unwrap();
    t.bypass_lin_to_log = true;
    let proc = processor(t.clone(), TransformDirection::Forward);
    assert_eq!(
        proc.create_group_transform().transforms,
        vec![Transform::GradingRgbCurve(t)]
    );
}
