//! Tests ported from `GradingHueCurveOpData_tests.cpp`, `GradingHueCurveOp_tests.cpp`,
//! `GradingHueCurveOpCPU_tests.cpp`, `GradingHueCurveTransform_tests.cpp` and
//! the hue curve parts of `DynamicProperty_tests.cpp`.

#![allow(clippy::excessive_precision)]

use super::*;
use crate::ops::grading_primary::tests::{processor, to_pixels, validate_image};
use crate::processor::optimize_ops;
use crate::transforms::grading::{GradingBSplineCurve, GradingControlPoint};
use crate::transforms::GroupTransform;
use crate::types::BSplineType;
use HueCurveType as H;

/// Tolerance of `GradingHueCurveOpCPU_tests.cpp`.
const ERROR: f32 = 2e-5;

fn hc(points: &[(f32, f32)], c: HueCurveType) -> GradingBSplineCurve {
    GradingBSplineCurve::new_for_hue_curve(points, c)
}

#[allow(clippy::too_many_arguments)]
fn curves(
    hh: &GradingBSplineCurve,
    hs: &GradingBSplineCurve,
    hl: &GradingBSplineCurve,
    ls: &GradingBSplineCurve,
    ss: &GradingBSplineCurve,
    ll: &GradingBSplineCurve,
    sl: &GradingBSplineCurve,
    hfx: &GradingBSplineCurve,
) -> Result<GradingHueCurve> {
    GradingHueCurve::from_curves(
        hh.clone(),
        hs.clone(),
        hl.clone(),
        ls.clone(),
        ss.clone(),
        ll.clone(),
        sl.clone(),
        hfx.clone(),
    )
}

fn apply_op(op: &dyn Op, input: &[f32]) -> Vec<Pixel> {
    let mut px = to_pixels(input);
    op.apply(&mut px);
    px
}

fn flatten(px: &[Pixel]) -> Vec<f32> {
    px.iter().flat_map(|p| p.iter().copied()).collect()
}

/// Forward then inverse (applied to the forward result), like the OCIO tests.
fn check_round_trip(
    style: GradingStyle,
    v: &GradingHueCurve,
    input: &[f32],
    expected: &[f32],
    line: u32,
) {
    let op = GradingHueCurveOp::new(
        style,
        v.clone(),
        TransformDirection::Forward,
        HsyTransformStyle::Hsy1,
        false,
    )
    .unwrap();
    let res = apply_op(&op, input);
    validate_image(expected, &res, ERROR, line);
    let op = GradingHueCurveOp::new(
        style,
        v.clone(),
        TransformDirection::Inverse,
        HsyTransformStyle::Hsy1,
        false,
    )
    .unwrap();
    let back = apply_op(&op, &flatten(&res));
    validate_image(input, &back, ERROR, line);
}

// GradingHueCurveOpData_tests.cpp

#[test]
fn op_data_accessors() {
    let gc = GradingHueCurveOp::identity(GradingStyle::Log);
    let expected = "<GradingHueCurveOp log forward \
        <hue_hue=<control_points=[<x=0, y=0><x=0.1666667, y=0.1666667><x=0.3333333, y=0.3333333><x=0.5, y=0.5><x=0.6666667, y=0.6666667><x=0.8333333, y=0.8333333>]>, \
        hue_sat=<control_points=[<x=0, y=1><x=0.1666667, y=1><x=0.3333333, y=1><x=0.5, y=1><x=0.6666667, y=1><x=0.8333333, y=1>]>, \
        hue_lum=<control_points=[<x=0, y=1><x=0.1666667, y=1><x=0.3333333, y=1><x=0.5, y=1><x=0.6666667, y=1><x=0.8333333, y=1>]>, \
        lum_sat=<control_points=[<x=0, y=1><x=0.5, y=1><x=1, y=1>]>, \
        sat_sat=<control_points=[<x=0, y=0><x=0.5, y=0.5><x=1, y=1>]>, \
        lum_lum=<control_points=[<x=0, y=0><x=0.5, y=0.5><x=1, y=1>]>, \
        sat_lum=<control_points=[<x=0, y=1><x=0.5, y=1><x=1, y=1>]>, \
        hue_fx=<control_points=[<x=0, y=0><x=0.1666667, y=0><x=0.3333333, y=0><x=0.5, y=0><x=0.6666667, y=0><x=0.8333333, y=0>]>>>";
    assert_eq!(gc.cache_id(), expected);
    assert_eq!(gc.style(), GradingStyle::Log);
    assert!(gc.value().is_identity());
    assert!(gc.is_identity());
    assert!(gc.is_no_op());
    assert!(gc.has_channel_crosstalk());
    assert_eq!(gc.rgb_to_hsy(), HsyTransformStyle::Hsy1);

    let gc = GradingHueCurveOp::new(
        GradingStyle::Lin,
        GradingHueCurve::new(GradingStyle::Lin),
        TransformDirection::Inverse,
        HsyTransformStyle::None,
        true,
    )
    .unwrap();
    assert_eq!(gc.rgb_to_hsy(), HsyTransformStyle::None);
    assert!(gc.is_dynamic());
    let dp = gc
        .dynamic_property(DynamicPropertyType::GradingHueCurve)
        .unwrap();
    assert_eq!(dp.property_type(), DynamicPropertyType::GradingHueCurve);
    assert_eq!(
        gc.cache_id(),
        "<GradingHueCurveOp linear inverse  bypassRGBToHSY >"
    );

    // Modified values.
    let mut v1 = GradingHueCurve::new(GradingStyle::Log);
    {
        let hue_hue = v1.curve_mut(H::HueHue);
        hue_hue.set_num_control_points(4);
        hue_hue.control_points[3] = GradingControlPoint::new(
            hue_hue.control_points[2].x + 0.25,
            hue_hue.control_points[2].y + 0.5,
        );
    }
    v1.curve_mut(H::HueSat).set_slope(2, 0.9);
    let gc1 = GradingHueCurveOp::new(
        GradingStyle::Log,
        v1,
        TransformDirection::Inverse,
        HsyTransformStyle::Hsy1,
        false,
    )
    .unwrap();
    assert_eq!(gc1.value().curve(H::HueSat).slope(2), 0.9);
    assert!(gc1.value().curve(H::HueLum).slopes_are_default());
    assert!(!gc1.value().curve(H::HueSat).slopes_are_default());
    assert!(!gc1.is_identity());
    assert!(gc1.has_channel_crosstalk());

    // isInverse.
    let mut v3 = GradingHueCurve::new(GradingStyle::Lin);
    {
        let spline = v3.curve_mut(H::HueLum);
        spline.set_num_control_points(2);
        spline.control_points[0] = GradingControlPoint::new(0.0, 2.0);
        spline.control_points[1] = GradingControlPoint::new(0.9, 2.0);
    }
    let fwd = GradingHueCurveOp::new(
        GradingStyle::Lin,
        v3.clone(),
        TransformDirection::Forward,
        HsyTransformStyle::Hsy1,
        false,
    )
    .unwrap();
    assert!(!fwd.is_identity());
    let inv = fwd.inverse();
    assert!(inv.is_inverse(&fwd));
    let mut v3b = v3.clone();
    v3b.curve_mut(H::HueLum).control_points[1].y += 0.25;
    let other = GradingHueCurveOp::new(
        GradingStyle::Lin,
        v3b,
        TransformDirection::Inverse,
        HsyTransformStyle::Hsy1,
        false,
    )
    .unwrap();
    assert!(!other.is_inverse(&fwd));
    let mut v3c = v3.clone();
    v3c.curve_mut(H::HueSat).set_slope(2, 0.9);
    let other = GradingHueCurveOp::new(
        GradingStyle::Lin,
        v3c,
        TransformDirection::Inverse,
        HsyTransformStyle::Hsy1,
        false,
    )
    .unwrap();
    assert!(!other.is_inverse(&fwd));
    assert!(!fwd.is_inverse(&fwd));
    let other = GradingHueCurveOp::new(
        GradingStyle::Lin,
        v3,
        TransformDirection::Inverse,
        HsyTransformStyle::None,
        false,
    )
    .unwrap();
    assert!(!other.is_inverse(&fwd));
}

#[test]
fn op_data_validate() {
    let c1 = GradingBSplineCurve::with_size(1, BSplineType::BSpline);
    let err = curves(&c1, &c1, &c1, &c1, &c1, &c1, &c1, &c1).unwrap_err();
    assert!(err
        .message()
        .contains("There must be at least 2 control points."));

    let c = hc(&[(0.0, 0.0), (1.0, 0.0)], H::HueFx);
    let err = curves(&c, &c, &c, &c, &c, &c, &c, &c).unwrap_err();
    assert!(err
        .message()
        .contains("The periodic spline x coordinates may not wrap to the same value."));

    let c = GradingBSplineCurve::new(
        &[(0.0, 0.0), (0.7, 0.3), (0.5, 0.7), (1.0, 1.0)],
        BSplineType::BSpline,
    );
    let err = curves(&c, &c, &c, &c, &c, &c, &c, &c).unwrap_err();
    assert!(err.message().contains(
        "has a x coordinate '0.5' that is less than previous control point x coordinate '0.7'."
    ));

    let mut c = hc(&[(0.1, 0.05), (1.1, 1.05)], H::HueHue);
    let err = curves(&c, &c, &c, &c, &c, &c, &c, &c).unwrap_err();
    assert!(err
        .message()
        .contains("The HUE-HUE spline may not have x coordinates greater than one."));
    c.control_points[1].x = 1.0;
    let err = curves(&c, &c, &c, &c, &c, &c, &c, &c).unwrap_err();
    assert!(err.message().contains(
        "GradingHueCurve validation failed: 'hue_sat' curve is of the wrong BSplineType."
    ));

    let c = GradingBSplineCurve::new(
        &[(0.0, 0.0), (0.3, 0.3), (0.5, 0.27), (1.0, 1.0)],
        BSplineType::BSpline,
    );
    let err = curves(&c, &c, &c, &c, &c, &c, &c, &c).unwrap_err();
    assert!(err
        .message()
        .contains("point at index 2 has a y coordinate '0.27' that is less than previous control point y coordinate '0.3'."));

    // For hue-hue this includes comparing the wrapped last point to the first point.
    let c = hc(&[(0.1, 0.05), (1.0, 1.1)], H::HueHue);
    let err = curves(&c, &c, &c, &c, &c, &c, &c, &c).unwrap_err();
    assert!(err.message().contains(
        "Control point at index 0 has a y coordinate '0.05' that is less than previous control point y coordinate '0.1'."
    ));
}

#[test]
fn op_data_dynamic() {
    let op = GradingHueCurveOp::new(
        GradingStyle::Log,
        GradingHueCurve::new(GradingStyle::Log),
        TransformDirection::Forward,
        HsyTransformStyle::Hsy1,
        true,
    )
    .unwrap();
    assert!(op
        .dynamic_property(DynamicPropertyType::GradingRgbCurve)
        .is_none());
    let dp = op
        .dynamic_property(DynamicPropertyType::GradingHueCurve)
        .unwrap();
    let mut v = GradingHueCurve::new(GradingStyle::Log);
    v.curve_mut(H::HueLum).control_points[0].y = 1.5;
    dp.as_grading_hue_curve().unwrap().set(v.clone());
    assert_eq!(op.value(), v);
    let non_dyn = op.make_non_dynamic().unwrap();
    assert!(!non_dyn.is_dynamic());
    assert_eq!(
        non_dyn.downcast_ref::<GradingHueCurveOp>().unwrap().value(),
        v
    );
}

// GradingHueCurveOp_tests.cpp

#[test]
fn op_create() {
    let mut ops = OpVec::new();
    let v = GradingHueCurve::new(GradingStyle::Log);
    let fwd = TransformDirection::Forward;
    create_grading_hue_curve_op(
        &mut ops,
        GradingStyle::Log,
        &v,
        fwd,
        HsyTransformStyle::Hsy1,
        false,
        fwd,
    )
    .unwrap();
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].name(), "GradingHueCurve");
    assert!(ops[0].is_identity());
    assert!(ops[0].is_no_op());

    create_grading_hue_curve_op(
        &mut ops,
        GradingStyle::Log,
        &v,
        fwd,
        HsyTransformStyle::Hsy1,
        true,
        fwd,
    )
    .unwrap();
    assert_eq!(ops.len(), 2);
    assert!(!ops[1].is_identity());
    assert!(!ops[1].is_no_op());
}

#[test]
fn op_create_transform() {
    let op = GradingHueCurveOp::new(
        GradingStyle::Log,
        GradingHueCurve::new(GradingStyle::Log),
        TransformDirection::Forward,
        HsyTransformStyle::Hsy1,
        true,
    )
    .unwrap();
    match op.to_transform().unwrap() {
        Transform::GradingHueCurve(t) => {
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
    let mut gc_transform = GradingHueCurveTransform::new(GradingStyle::Log);

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
    let gco = ops[0].downcast_ref::<GradingHueCurveOp>().unwrap();
    assert!(gco.is_dynamic());
    assert_eq!(6, gco.value().curve(H::HueHue).num_control_points());

    let proc = processor(gc_transform.clone(), TransformDirection::Forward);
    assert!(proc.has_dynamic_property(DynamicPropertyType::GradingHueCurve));
    assert!(!proc.has_dynamic_property(DynamicPropertyType::Exposure));
    let cpu = proc.default_cpu_processor();

    // Create a non-identity curve: add a constant offset to all hues.
    let mut hue_curve = GradingHueCurve::new(GradingStyle::Log);
    {
        let spline = hue_curve.curve_mut(H::HueLum);
        spline.set_num_control_points(2);
        spline.control_points[0] = GradingControlPoint::new(0.0, 2.0);
        spline.control_points[1] = GradingControlPoint::new(0.9, 2.0);
    }
    assert!(!hue_curve.is_identity());

    gc_transform.set_value(hue_curve.clone()).unwrap();
    // (Still has its original value.)
    assert_eq!(1.0, gco.value().curve(H::HueLum).control_points[0].y);

    let dp = cpu
        .dynamic_property(DynamicPropertyType::GradingHueCurve)
        .unwrap();
    let dpgc = dp.as_grading_hue_curve().unwrap();

    let mut pixel = [0.0f32, 0.2, 2.0];
    cpu.apply_rgb(&mut pixel);
    let error = 1e-5f32;
    assert!((pixel[0] - 0.0).abs() < error);
    assert!((pixel[1] - 0.2).abs() < error);
    assert!((pixel[2] - 2.0).abs() < error);

    dpgc.set(hue_curve);
    cpu.apply_rgb(&mut pixel);
    assert!((pixel[0] - 0.1).abs() < error, "{pixel:?}");
    assert!((pixel[1] - 0.3).abs() < error, "{pixel:?}");
    assert!((pixel[2] - 2.1).abs() < error, "{pixel:?}");
}

#[test]
fn pair_identity_optimization() {
    let mut v = GradingHueCurve::new(GradingStyle::Log);
    v.curve_mut(H::HueLum).control_points[0].y = 1.5;
    let fwd = GradingHueCurveOp::new(
        GradingStyle::Log,
        v,
        TransformDirection::Forward,
        HsyTransformStyle::Hsy1,
        false,
    )
    .unwrap();
    let ops: OpVec = vec![Arc::new(fwd.clone()), Arc::new(fwd.inverse())];
    assert!(optimize_ops(&ops, OptimizationFlags::DEFAULT).is_empty());
    assert_eq!(optimize_ops(&ops, OptimizationFlags::NONE).len(), 2);
}

// GradingHueCurveOpCPU_tests.cpp

#[test]
fn cpu_identity() {
    let qnan = f32::NAN;
    let inf = f32::INFINITY;
    #[rustfmt::skip]
    let image = [
        -0.50, -0.25, 0.50, 0.0,
         0.75, 1.00, 1.25, 1.0,
         1.25, 1.50, 1.75, 0.0,
          0.0, 0.0, 0.0, qnan,
          0.0, 0.0, 0.0, inf,
          0.0, 0.0, 0.0, -inf,
    ];
    for dir in [TransformDirection::Forward, TransformDirection::Inverse] {
        let op = GradingHueCurveOp::new(
            GradingStyle::Lin,
            GradingHueCurve::new(GradingStyle::Lin),
            dir,
            HsyTransformStyle::Hsy1,
            false,
        )
        .unwrap();
        validate_image(&image, &apply_op(&op, &image), ERROR, line!());
    }
}

#[test]
fn cpu_log_identity() {
    // Identity curves (for log or video) that are different from the default curves.
    let hh = hc(
        &[
            (0.0, 0.0),
            (0.1, 0.1),
            (0.2, 0.2),
            (0.4, 0.4),
            (0.6, 0.6),
            (0.8, 0.8),
        ],
        H::HueHue,
    );
    let hs = hc(
        &[
            (0.0, 1.0),
            (0.1, 1.0),
            (0.2, 1.0),
            (0.4, 1.0),
            (0.6, 1.0),
            (0.8, 1.0),
        ],
        H::HueSat,
    );
    let hl = hc(
        &[
            (0.0, 1.0),
            (0.1, 1.0),
            (0.2, 1.0),
            (0.4, 1.0),
            (0.6, 1.0),
            (0.8, 1.0),
        ],
        H::HueLum,
    );
    let ls = hc(&[(0.0, 1.0), (1.0, 1.0)], H::LumSat);
    let ss = hc(&[(0.0, 0.0), (0.25, 0.25), (1.0, 1.0)], H::SatSat);
    let ll = hc(
        &[(0.0, 0.0), (0.25, 0.25), (0.5, 0.5), (1.0, 1.0)],
        H::LumLum,
    );
    let sl = hc(
        &[(0.0, 1.0), (0.25, 1.0), (0.5, 1.0), (1.0, 1.0)],
        H::SatLum,
    );
    let hfx = hc(
        &[
            (0.0, 0.0),
            (0.1, 0.0),
            (0.2, 0.0),
            (0.4, 0.0),
            (0.6, 0.0),
            (0.8, 0.0),
        ],
        H::HueFx,
    );
    let v = curves(&hh, &hs, &hl, &ls, &ss, &ll, &sl, &hfx).unwrap();
    #[rustfmt::skip]
    let input = [
        -0.2, 0.2, 0.5, 0.0,
         0.8, 1.0, 2.0, 0.5];
    check_round_trip(GradingStyle::Log, &v, &input, &input, line!());
}

#[test]
fn cpu_hh_hfx_curves() {
    let hs = hc(&[(0.0, 1.0), (0.9, 1.0)], H::HueSat);
    let hl = hc(&[(0.0, 1.0), (0.9, 1.0)], H::HueLum);
    let ls = hc(&[(0.0, 1.0), (0.9, 1.0)], H::LumSat);
    let ss = hc(&[(0.0, 0.0), (0.9, 0.9)], H::SatSat);
    let ll = hc(&[(0.0, 0.0), (0.9, 0.9)], H::LumLum);
    let sl = hc(&[(0.0, 1.0), (0.9, 1.0)], H::SatLum);
    let hh = hc(
        &[
            (0.05, 0.15),
            (0.2, 0.3),
            (0.35, 0.4),
            (0.45, 0.45),
            (0.6, 0.7),
            (0.8, 0.85),
        ],
        H::HueHue,
    );
    let hfx = hc(
        &[
            (0.2, 0.05),
            (0.4, -0.09),
            (0.6, -0.2),
            (0.8, 0.05),
            (0.99, -0.02),
        ],
        H::HueFx,
    );
    let v = curves(&hh, &hs, &hl, &ls, &ss, &ll, &sl, &hfx).unwrap();
    #[rustfmt::skip]
    let input = [
        0.1, 0.5, 0.7, 0.0,
        0.6, 0.9, 0.8, 0.5,
        0.4, 0.35, 0.3, 0.0,
        0.0, 0.0, 0.0, 1.0];
    #[rustfmt::skip]
    let expected = [
        0.3984785676, 0.3790940642, 1.0187726020, 0.0,
        0.6117081642, 0.8883015513, 0.8814064860, 0.5,
        0.3847683966, 0.3567464352, 0.2780219615, 0.0,
        0.0, 0.0, 0.0, 1.0];
    check_round_trip(GradingStyle::Log, &v, &input, &expected, line!());
}

fn all_curves(lin: bool) -> GradingHueCurve {
    let hh = hc(
        &[
            (0.05, 0.15),
            (0.2, 0.3),
            (0.35, 0.4),
            (0.45, 0.45),
            (0.6, 0.7),
            (0.8, 0.85),
        ],
        H::HueHue,
    );
    let hs = hc(
        &[
            (-0.1, 1.2),
            (0.2, 0.7),
            (0.4, 1.5),
            (0.5, 0.5),
            (0.6, 1.4),
            (0.8, 0.7),
        ],
        H::HueSat,
    );
    let hl = hc(
        &[(0.1, 1.5), (0.2, 0.7), (0.4, 1.4), (0.5, 0.8), (0.8, 0.5)],
        H::HueLum,
    );
    let ss = hc(&[(0.0, 0.1), (0.5, 0.45), (1.0, 1.1)], H::SatSat);
    let sl = hc(&[(0.0, 1.2), (0.6, 0.8), (0.9, 1.1)], H::SatLum);
    let hfx = hc(
        &[
            (0.2, 0.05),
            (0.4, -0.09),
            (0.6, -0.2),
            (0.8, 0.05),
            (0.99, -0.02),
        ],
        H::HueFx,
    );
    let (ls, ll) = if lin {
        // Work in f-stops.
        (
            hc(
                &[
                    (-6.0, 0.9),
                    (-3.0, 0.8),
                    (0.0, 1.2),
                    (2.0, 1.0),
                    (4.0, 0.6),
                    (6.0, 0.55),
                ],
                H::LumSat,
            ),
            hc(
                &[(-8.0, -7.0), (-2.0, -3.0), (2.0, 3.5), (8.0, 7.0)],
                H::LumLum,
            ),
        )
    } else {
        (
            hc(&[(0.05, 1.5), (0.5, 0.9), (1.1, 1.4)], H::LumSat),
            hc(
                &[(-0.02, -0.04), (0.2, 0.1), (0.8, 0.95), (1.1, 1.2)],
                H::LumLum,
            ),
        )
    };
    curves(&hh, &hs, &hl, &ls, &ss, &ll, &sl, &hfx).unwrap()
}

#[test]
fn cpu_log_all_curves() {
    #[rustfmt::skip]
    let input = [
        0.1, 0.5, 0.7, 0.0,
        0.6, 0.9, 0.8, 0.5,
        0.4, 0.35, 0.3, 0.0,
        0.4, -0.2, -0.05, 0.0,
        0.0, 0.0, 0.0, 1.0];
    #[rustfmt::skip]
    let expected = [
        0.651269494808, 0.630018105394, 1.331314732772, 0.0,
        0.787401154155, 1.286561695129, 1.274118545611, 0.5,
        0.317389674917, 0.297787779440, 0.242718507572, 0.0,
        0.830653473122, 0.449246419743, -0.173027078802, 0.0,
        0.004989255546, -0.033773428950, -0.019725339077, 1.0];
    check_round_trip(
        GradingStyle::Log,
        &all_curves(false),
        &input,
        &expected,
        line!(),
    );
}

#[test]
fn cpu_lin_all_curves() {
    #[rustfmt::skip]
    let input = [
        0.1, 0.5, 0.7, 0.0,
        0.6, 0.9, 0.8, 0.5,
        2.4, 2.35, 2.3, 0.0,
        0.4, 0.2, -0.05, 0.0,
        0.0, 0.0, 0.0, 1.0];
    #[rustfmt::skip]
    let expected: [f32; 20] = [
        0.527229344453, 0.490778616791, 1.693653961874, 0.0,
        1.253512415394, 2.240034381083, 2.215442212203, 0.5,
        6.983003751281, 6.772174817271, 6.179875164501, 0.0,
        0.527554073346, 0.360480028655, -0.135388205576, 0.0,
        0.011308048228, -0.001711436982, 0.003006990049, 1.0];
    let v = all_curves(true);
    let op = GradingHueCurveOp::new(
        GradingStyle::Lin,
        v.clone(),
        TransformDirection::Forward,
        HsyTransformStyle::Hsy1,
        false,
    )
    .unwrap();
    let res = apply_op(&op, &input);
    // OCIO uses an absolute 2e-5 tolerance; values above 1 are compared relatively.
    for (i, px) in res.iter().enumerate() {
        for j in 0..4 {
            let e = expected[i * 4 + j];
            assert!(
                (px[j] - e).abs() <= ERROR * e.abs().max(1.0),
                "fwd {i} {j}: {} != {e}",
                px[j]
            );
        }
    }
    let op = GradingHueCurveOp::new(
        GradingStyle::Lin,
        v,
        TransformDirection::Inverse,
        HsyTransformStyle::Hsy1,
        false,
    )
    .unwrap();
    validate_image(&input, &apply_op(&op, &flatten(&res)), ERROR, line!());
}

#[test]
fn cpu_draw_curve_only() {
    let mut val = GradingHueCurve::new(GradingStyle::Log);
    val.curve_mut(H::HueSat).control_points[1] = GradingControlPoint::new(0.15, 1.4);
    assert!(!val.is_identity());
    // Only evaluate the HUE-SAT spline, for use in a user interface.
    val.draw_curve_only = true;

    #[rustfmt::skip]
    let input = [
        -0.2, 0.15, 0.15, 0.0,
         0.15, 1.0, 2.0, 0.5];
    #[rustfmt::skip]
    let expected = [
         1.0, 1.4, 1.4, 0.0,
         1.4, 1.0, 1.0, 0.5];
    // The direction is ignored in DrawCurveOnly mode.
    for dir in [TransformDirection::Forward, TransformDirection::Inverse] {
        let op = GradingHueCurveOp::new(
            GradingStyle::Log,
            val.clone(),
            dir,
            HsyTransformStyle::Hsy1,
            false,
        )
        .unwrap();
        validate_image(&expected, &apply_op(&op, &input), ERROR, line!());
    }
}

#[test]
fn cpu_bypass_rgb_to_hsy() {
    let mut val = GradingHueCurve::new(GradingStyle::Log);
    val.curve_mut(H::SatSat).control_points[1] = GradingControlPoint::new(0.4, 0.8);
    assert!(!val.is_identity());

    #[rustfmt::skip]
    let input = [
        0.2, 0.2, -0.1, 0.0,
        0.1, 0.4, 2.0, 0.5];
    // Only the green channel gets processed, using the sat-sat curve.
    #[rustfmt::skip]
    let expected = [
        0.2, 0.4475418, -0.1, 0.0,
        0.1, 0.8000000, 2.0, 0.5];
    let op = GradingHueCurveOp::new(
        GradingStyle::Log,
        val.clone(),
        TransformDirection::Forward,
        HsyTransformStyle::None,
        false,
    )
    .unwrap();
    let res = apply_op(&op, &input);
    validate_image(&expected, &res, ERROR, line!());
    let op = GradingHueCurveOp::new(
        GradingStyle::Log,
        val,
        TransformDirection::Inverse,
        HsyTransformStyle::None,
        false,
    )
    .unwrap();
    validate_image(&input, &apply_op(&op, &flatten(&res)), ERROR, line!());
}

// DynamicProperty_tests.cpp: setter_validation and grading_hue_curve_knots_coefs

#[test]
fn dynamic_property_setter_validation() {
    let mut gct = GradingHueCurveTransform::new(GradingStyle::Log);
    gct.make_dynamic();
    let cpu = processor(gct, TransformDirection::Forward).default_cpu_processor();

    let mut pixel = [0.4f32, 0.3, 0.2];
    cpu.apply_rgb(&mut pixel);
    let error = 1e-5f32;
    assert!(
        (pixel[0] - 0.4).abs() < error
            && (pixel[1] - 0.3).abs() < error
            && (pixel[2] - 0.2).abs() < error
    );

    let dp = cpu
        .dynamic_property(DynamicPropertyType::GradingHueCurve)
        .unwrap();
    let dp_val = dp.as_grading_hue_curve().unwrap();

    // Set a non-identity value.
    let mut hue_curve = dp_val.get();
    {
        let huehue = hue_curve.curve_mut(H::HueHue);
        huehue.set_num_control_points(3);
        huehue.control_points[0] = GradingControlPoint::new(0.0, -0.1);
        huehue.control_points[1] = GradingControlPoint::new(0.5, 0.5);
        huehue.control_points[2] = GradingControlPoint::new(0.8, 0.8);
    }
    dp_val.set(hue_curve.clone());
    cpu.apply_rgb(&mut pixel);
    assert!((pixel[0] - 0.4385873675).abs() < error, "{pixel:?}");
    assert!((pixel[1] - 0.2829087377).abs() < error, "{pixel:?}");
    assert!((pixel[2] - 0.2556785941).abs() < error, "{pixel:?}");

    // The last point is no longer monotonic with respect to the first point (because it is
    // periodic, the last point Y value becomes -0.05 when wrapped around to an X value of -0.2).
    hue_curve.curve_mut(H::HueHue).control_points[2] = GradingControlPoint::new(0.8, 0.95);
    assert_eq!(
        hue_curve.validate().unwrap_err().message(),
        "GradingHueCurve validation failed for 'hue_hue' curve with: Control point at index 0 has a y \
         coordinate '-0.1' that is less than previous control point y coordinate '-0.05'."
    );
    // Setting the invalid value in the property is ignored by the op.
    dp_val.set(hue_curve);
    let mut pixel2 = [0.4f32, 0.3, 0.2];
    cpu.apply_rgb(&mut pixel2);
    let mut expected = [0.4f32, 0.3, 0.2];
    cpu.apply_rgb(&mut expected);
    assert_eq!(pixel2, expected);
}

fn check_knots_and_coefs(
    kc: &KnotsCoefs,
    set: usize,
    true_knots: &[f32],
    true_a: &[f32],
    true_b: &[f32],
    true_c: &[f32],
    line: u32,
) {
    let num_knots = kc.knots_offsets[set * 2 + 1] as usize;
    let koff = kc.knots_offsets[set * 2] as usize;
    assert_eq!(num_knots, true_knots.len(), "line {line}");
    for i in 0..num_knots {
        assert!(
            (kc.knots[koff + i] - true_knots[i]).abs() <= 1e-6,
            "line {line}: knot {i}"
        );
    }
    let num_sets = (kc.coefs_offsets[set * 2 + 1] / 3) as usize;
    let coff = kc.coefs_offsets[set * 2] as usize;
    assert_eq!(num_sets, true_a.len(), "line {line}");
    for i in 0..num_sets {
        assert!(
            (kc.coefs[coff + i] - true_a[i]).abs() <= 3e-4,
            "line {line}: A {i}"
        );
        assert!(
            (kc.coefs[coff + num_sets + i] - true_b[i]).abs() <= 1e-5,
            "line {line}: B {i}"
        );
        assert!(
            (kc.coefs[coff + 2 * num_sets + i] - true_c[i]).abs() <= 1e-5,
            "line {line}: C {i}"
        );
    }
}

#[test]
fn knots_coefs() {
    let hh = hc(
        &[
            (0.1, 0.05),
            (0.2, 0.3),
            (0.5, 0.4),
            (0.8, 0.7),
            (0.9, 0.75),
            (1.0, 0.9),
        ],
        H::HueHue,
    );
    let hs = hc(
        &[
            (-0.15, 1.25),
            (0.0, 0.8),
            (0.2, 0.9),
            (0.4, 1.8),
            (0.6, 1.4),
            (0.8, 1.3),
            (0.9, 1.1),
            (1.1, 0.7),
        ],
        H::HueSat,
    );
    let hl = hc(
        &[
            (0.0, 0.0),
            (0.22, 0.077),
            (0.36, 0.092),
            (0.51, 0.27),
            (0.67, 0.0),
            (0.83, 0.0),
        ],
        H::HueLum,
    );
    // The rest are identities, but not the default curves.
    let ls = hc(&[(0.0, 1.0), (1.0, 1.0)], H::LumSat);
    let ss = hc(&[(0.0, 0.0), (0.25, 0.25), (1.0, 1.0)], H::SatSat);
    let ll = hc(
        &[(0.0, 0.0), (0.25, 0.25), (0.5, 0.5), (1.0, 1.0)],
        H::LumLum,
    );
    let sl = hc(
        &[(0.0, 1.0), (0.25, 1.0), (0.5, 1.0), (1.0, 1.0)],
        H::SatLum,
    );
    let hfx = hc(
        &[
            (0.0, 0.0),
            (0.1, 0.0),
            (0.2, 0.0),
            (0.4, 0.0),
            (0.6, 0.0),
            (0.8, 0.0),
        ],
        H::HueFx,
    );
    let mut v = curves(&hh, &hs, &hl, &ls, &ss, &ll, &sl, &hfx).unwrap();

    {
        let kc = KnotsCoefs::from_hue_curve(&v).unwrap();
        assert_eq!(46, kc.num_knots);
        assert_eq!(129, kc.num_coefs);
        assert_eq!(
            kc.knots_offsets,
            vec![0, 15, 15, 19, 34, 12, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0]
        );
        assert_eq!(
            kc.coefs_offsets,
            vec![0, 42, 42, 54, 96, 33, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0]
        );
    }

    // DrawCurveOnly mode yields identity knots and coefs for the identity curves.
    v.draw_curve_only = true;
    let kc = KnotsCoefs::from_hue_curve(&v).unwrap();
    assert_eq!(56, kc.num_knots);
    assert_eq!(144, kc.num_coefs);
    assert_eq!(
        kc.knots_offsets,
        vec![0, 15, 15, 19, 34, 12, 46, 2, 48, 2, 50, 2, 52, 2, 54, 2]
    );
    assert_eq!(
        kc.coefs_offsets,
        vec![0, 42, 42, 54, 96, 33, 129, 3, 132, 3, 135, 3, 138, 3, 141, 3]
    );

    {
        // Hue-Hue
        let knots = [
            -0.1,
            -0.06928571,
            0.0,
            0.05642857,
            0.1,
            0.17549634,
            0.2,
            0.33714286,
            0.5,
            0.62499860,
            0.8,
            0.85261905,
            0.9,
            0.93071429,
            1.0,
        ];
        let a = [
            15.95930233,
            -1.66237113,
            -1.44778481,
            6.17827869,
            10.39930009,
            -58.70626575,
            -1.54375789,
            1.03834397,
            3.7077401,
            -2.12344738,
            -3.54260935,
            4.81365159,
            15.95930233,
            -1.66237113,
        ];
        let b = [
            0.75, 1.73035714, 1.5, 1.33660714, 1.875, 3.44521825, 0.56818182, 0.14475108,
            0.48295455, 1.40987919, 0.66666667, 0.29384921, 0.75, 1.73035714,
        ];
        let c = [
            -0.25,
            -0.2119088,
            -0.1,
            -0.01996716,
            0.05,
            0.25082851,
            0.3,
            0.34888683,
            0.4,
            0.51830078,
            0.7,
            0.72527072,
            0.75,
            0.7880912,
        ];
        check_knots_and_coefs(&kc, 0, &knots, &a, &b, &c, line!());
    }
    {
        // Hue-Sat
        let knots = [
            -0.1,
            -0.03071429,
            0.0,
            0.0625,
            0.1,
            0.13333333,
            0.2,
            0.34913793,
            0.4,
            0.46896552,
            0.6,
            0.69,
            0.8,
            0.82770833,
            0.85,
            0.86535714,
            0.9,
            0.96928571,
            1.0,
        ];
        let a = [
            -3.32474227,
            31.91860465,
            3.5,
            14.16666667,
            32.30769231,
            4.61538462,
            13.9662072,
            -68.17470665,
            -25.2,
            10.21052632,
            2.92592593,
            -1.78787879,
            -5.32581454,
            -12.07165109,
            -63.8372093,
            6.64948454,
            -3.32474227,
            31.91860465,
        ];
        let b = [
            -3.0,
            -3.46071429,
            -1.5,
            -1.0625,
            0.0,
            2.15384615,
            2.76923077,
            6.93501326,
            0.0,
            -3.47586207,
            -0.8,
            -0.27333333,
            -0.66666667,
            -0.96180556,
            -1.5,
            -3.46071429,
            -3.0,
            -3.46071429,
        ];
        let c = [
            1.1, 0.8761824, 0.8, 0.71992187, 0.7, 0.73589744, 0.9, 1.62363544, 1.8, 1.68014269,
            1.4, 1.3517, 1.3, 1.27743887, 1.25, 1.2119088, 1.1, 0.8761824,
        ];
        check_knots_and_coefs(&kc, 1, &knots, &a, &b, &c, line!());
    }
    {
        // Hue-Lum: "Adjust slopes that are not shape-preserving" path in EstimateHueSlopes.
        let knots = [
            -0.17, 0.0, 0.07049104, 0.22, 0.29691485, 0.36, 0.435, 0.51, 0.59, 0.67, 0.83, 1.0,
        ];
        let a = [
            0.0,
            4.21997107,
            -1.47264319,
            -0.70657119,
            1.10402357,
            13.97025263,
            -15.20489902,
            -21.09375,
            21.09375,
            0.0,
            0.0,
        ];
        let b = [
            0.0, 0.0, 0.59494032, 0.15459362, 0.04590198, 0.18519696, 2.28073485, 0.0, -3.375, 0.0,
            0.0,
        ];
        let c = [
            0.0, 0.0, 0.02096898, 0.077, 0.08471054, 0.092, 0.18447244, 0.27, 0.135, 0.0, 0.0,
        ];
        check_knots_and_coefs(&kc, 2, &knots, &a, &b, &c, line!());
    }
    // Horizontal identities.
    check_knots_and_coefs(&kc, 3, &[0.0, 1.0], &[0.0], &[0.0], &[1.0], line!());
    check_knots_and_coefs(&kc, 6, &[0.0, 1.0], &[0.0], &[0.0], &[1.0], line!());
    check_knots_and_coefs(&kc, 7, &[0.0, 1.0], &[0.0], &[0.0], &[0.0], line!());
    // Diagonal identities.
    check_knots_and_coefs(&kc, 4, &[0.0, 1.0], &[0.0], &[1.0], &[0.0], line!());
    check_knots_and_coefs(&kc, 5, &[0.0, 1.0], &[0.0], &[1.0], &[0.0], line!());
}

#[test]
fn max_ctrl_pnts() {
    let mut v = GradingHueCurve::new(GradingStyle::Video);
    for c in HueCurveType::ALL {
        v.curve_mut(c).set_num_control_points(28);
    }
    // The fitting fails (like the OCIO dynamic property constructor, which does not validate).
    let err = KnotsCoefs::from_hue_curve(&v).unwrap_err();
    assert!(
        err.message()
            .contains("Hue curve: maximum number of control points reached"),
        "{}",
        err.message()
    );
    // Creating the op validates the curves first.
    assert!(GradingHueCurveOp::new(
        GradingStyle::Video,
        v,
        TransformDirection::Forward,
        HsyTransformStyle::Hsy1,
        false
    )
    .is_err());
}

// GradingHueCurveTransform_tests.cpp

#[test]
fn transform_basic() {
    let mut gct_lin = GradingHueCurveTransform::new(GradingStyle::Lin);
    assert_eq!(gct_lin.style, GradingStyle::Lin);
    assert_eq!(gct_lin.direction, TransformDirection::Forward);
    gct_lin.direction = TransformDirection::Inverse;
    assert_eq!(gct_lin.rgb_to_hsy, HsyTransformStyle::Hsy1);
    assert!(!gct_lin.is_dynamic());
    let crv = gct_lin.value.curve(H::LumSat);
    assert_eq!(
        crv.control_points,
        vec![
            GradingControlPoint::new(-7.0, 1.0),
            GradingControlPoint::new(0.0, 1.0),
            GradingControlPoint::new(7.0, 1.0)
        ]
    );
    assert_eq!(
        gct_lin.value.curve(H::HueSat),
        gct_lin.value.curve(H::HueLum)
    );
    assert!(gct_lin.validate().is_ok());

    for style in [GradingStyle::Log, GradingStyle::Video] {
        let t = GradingHueCurveTransform::new(style);
        assert_eq!(t.rgb_to_hsy, HsyTransformStyle::Hsy1);
        assert_eq!(
            t.value.curve(H::LumSat).control_points,
            vec![
                GradingControlPoint::new(0.0, 1.0),
                GradingControlPoint::new(0.5, 1.0),
                GradingControlPoint::new(1.0, 1.0)
            ]
        );
        assert_eq!(t.value.curve(H::HueSat), t.value.curve(H::HueLum));
        assert_eq!(t.value.curve(H::SatSat), t.value.curve(H::LumLum));
        assert!(t.validate().is_ok());
    }

    // Change values.
    let mut gct = GradingHueCurveTransform::new(GradingStyle::Video);
    gct.set_style(GradingStyle::Lin);
    assert_eq!(gct.style, GradingStyle::Lin);
    gct.direction = TransformDirection::Inverse;
    gct.rgb_to_hsy = HsyTransformStyle::None;
    gct.make_dynamic();
    assert!(gct.is_dynamic());
    gct.set_value(gct_lin.value.clone()).unwrap();
    let crv = gct.value.curve(H::LumLum);
    assert_eq!(crv.control_points[0], GradingControlPoint::new(-7.0, -7.0));
    assert!(gct.validate().is_ok());
    assert_eq!(
        crv.control_point(4).unwrap_err().message(),
        "There are '3' control points. '4' is out of bounds."
    );

    // X-coordinate has to be increasing.
    {
        let mut hct = GradingHueCurveTransform::new(GradingStyle::Video);
        let mut hue_curve = hct.value.clone();
        hue_curve.curve_mut(H::LumSat).control_points[0] = GradingControlPoint::new(0.7, 1.0);
        assert!(hct.set_value(hue_curve).unwrap_err().message().contains(
            "has a x coordinate '0.5' that is less than previous control point x coordinate '0.7'."
        ));
    }
    // Y-coordinate has to be increasing, for diagonal curves.
    {
        let mut hct = GradingHueCurveTransform::new(GradingStyle::Video);
        let mut hue_curve = hct.value.clone();
        hue_curve.curve_mut(H::LumLum).control_points[0] = GradingControlPoint::new(0.0, 0.6);
        assert!(hct.set_value(hue_curve).unwrap_err().message().contains(
            "has a y coordinate '0.5' that is less than previous control point y coordinate '0.6'."
        ));
    }

    // Check slopes.
    gct.set_slope(H::LumLum, 2, 0.9).unwrap();
    assert!(gct.validate().is_ok());
    assert_eq!(gct.slope(H::LumLum, 2).unwrap(), 0.9);
    assert_eq!(
        gct.set_slope(H::LumLum, 4, 2.0).unwrap_err().message(),
        "There are '3' control points. '4' is out of bounds."
    );
    assert!(gct.slopes_are_default(H::LumSat));
    assert!(!gct.slopes_are_default(H::LumLum));
}

#[test]
fn transform_processor_several_transforms() {
    let mut gcta = GradingHueCurveTransform::new(GradingStyle::Log);
    let mut hue_curve_a = gcta.value.clone();
    {
        // Shift all hues up by 0.1.
        let huefx = hue_curve_a.curve_mut(H::HueFx);
        huefx.set_num_control_points(2);
        huefx.control_points[0] = GradingControlPoint::new(0.0, 0.1);
        huefx.control_points[1] = GradingControlPoint::new(0.9, 0.1);
    }
    gcta.set_value(hue_curve_a.clone()).unwrap();
    assert!(gcta.validate().is_ok());
    assert!(!hue_curve_a.is_identity());

    let src = [0.2f32, 0.3, 0.4];
    let apply = |t: &GradingHueCurveTransform, px: [f32; 3]| {
        let cpu = processor(t.clone(), TransformDirection::Forward).default_cpu_processor();
        let mut p = px;
        cpu.apply_rgb(&mut p);
        p
    };
    let pixel_a = apply(&gcta, src);
    let pixel_aa = apply(&gcta, pixel_a);

    let mut gctb = GradingHueCurveTransform::new(GradingStyle::Log);
    let mut hue_curve_b = gctb.value.clone();
    {
        // Increase sat at all luminances by 1.5.
        let lumsat = hue_curve_b.curve_mut(H::LumSat);
        lumsat.set_num_control_points(2);
        lumsat.control_points[0] = GradingControlPoint::new(0.0, 1.5);
        lumsat.control_points[1] = GradingControlPoint::new(1.0, 1.5);
    }
    gctb.set_value(hue_curve_b.clone()).unwrap();
    assert!(!hue_curve_b.is_identity());
    let pixel_ab = apply(&gctb, pixel_a);

    gctb.make_dynamic();
    let error = 1e-6f32;
    let mut grp1 = GroupTransform::new();
    gctb.set_value(hue_curve_a).unwrap();
    grp1.append(gcta.clone());
    grp1.append(gctb.clone());
    {
        let cpu = processor(grp1, TransformDirection::Forward).default_cpu_processor();
        let dp = cpu
            .dynamic_property(DynamicPropertyType::GradingHueCurve)
            .unwrap();
        let dp_val = dp.as_grading_hue_curve().unwrap();
        let mut pixel = src;
        cpu.apply_rgb(&mut pixel);
        for i in 0..3 {
            assert!((pixel[i] - pixel_aa[i]).abs() <= error);
        }
        dp_val.set(hue_curve_b);
        let mut pixel = src;
        cpu.apply_rgb(&mut pixel);
        for i in 0..3 {
            assert!((pixel[i] - pixel_ab[i]).abs() <= error);
        }
    }

    gcta.make_dynamic();
    let mut grp2 = GroupTransform::new();
    grp2.append(gcta);
    grp2.append(gctb);
    let _ = processor(grp2, TransformDirection::Forward);
}

#[test]
fn transform_serialization() {
    let hh = hc(
        &[
            (0.1, -0.05),
            (0.2, 0.23),
            (0.5, 0.25),
            (0.8, 0.7),
            (0.85, 0.8),
            (0.95, 0.9),
        ],
        H::HueHue,
    );
    let hs = hc(
        &[
            (0.0, 1.2),
            (0.1, 1.2),
            (0.4, 0.7),
            (0.6, 0.3),
            (0.8, 0.5),
            (0.9, 0.8),
        ],
        H::HueSat,
    );
    let hl = hc(
        &[(0.1, 1.4), (0.2, 1.4), (0.4, 0.7), (0.6, 0.5), (0.8, 0.8)],
        H::HueLum,
    );
    let ls = hc(&[(0.0, 1.0), (0.5, 1.5), (1.0, 0.9), (1.1, 1.1)], H::LumSat);
    let mut ss = hc(&[(0.0, 0.05), (0.5, 0.8), (1.0, 1.05)], H::SatSat);
    let ll = hc(&[(0.0, -0.0005), (0.5, 0.3), (1.0, 0.9)], H::LumLum);
    let sl = hc(&[(0.05, 1.1), (0.3, 1.0), (1.2, 0.9)], H::SatLum);
    let hfx = hc(
        &[
            (-0.15, 0.1),
            (0.0, -0.05),
            (0.2, -0.1),
            (0.4, 0.3),
            (0.6, 0.25),
            (0.8, 0.2),
            (0.9, 0.05),
            (1.1, -0.07),
        ],
        H::HueFx,
    );
    ss.set_slope(0, 1.2);
    ss.set_slope(1, 0.8);
    ss.set_slope(2, 0.4);
    let data = curves(&hh, &hs, &hl, &ls, &ss, &ll, &sl, &hfx).unwrap();
    let mut curve = GradingHueCurveTransform::new(GradingStyle::Video);
    assert!(curve.validate().is_ok());
    curve.set_value(data).unwrap();
    let expected = "<GradingHueCurveTransform direction=forward, style=video, values=\
        <hue_hue=<control_points=[<x=0.1, y=-0.05><x=0.2, y=0.23><x=0.5, y=0.25><x=0.8, y=0.7>\
        <x=0.85, y=0.8><x=0.95, y=0.9>]>, hue_sat=<control_points=[<x=0, y=1.2><x=0.1, y=1.2>\
        <x=0.4, y=0.7><x=0.6, y=0.3><x=0.8, y=0.5><x=0.9, y=0.8>]>, hue_lum=<control_points=\
        [<x=0.1, y=1.4><x=0.2, y=1.4><x=0.4, y=0.7><x=0.6, y=0.5><x=0.8, y=0.8>]>, lum_sat=\
        <control_points=[<x=0, y=1><x=0.5, y=1.5><x=1, y=0.9><x=1.1, y=1.1>]>, sat_sat=<control_points=\
        [<x=0, y=0.05, slp=1.2><x=0.5, y=0.8, slp=0.8><x=1, y=1.05, slp=0.4>]>, lum_lum=<control_points=\
        [<x=0, y=-0.0005><x=0.5, y=0.3><x=1, y=0.9>]>, sat_lum=<control_points=[<x=0.05, y=1.1>\
        <x=0.3, y=1><x=1.2, y=0.9>]>, hue_fx=<control_points=[<x=-0.15, y=0.1><x=0, y=-0.05>\
        <x=0.2, y=-0.1><x=0.4, y=0.3><x=0.6, y=0.25><x=0.8, y=0.2><x=0.9, y=0.05><x=1.1, y=-0.07>]>>>";
    assert_eq!(format!("{curve}"), expected);
    curve.rgb_to_hsy = HsyTransformStyle::None;
    curve.make_dynamic();
    assert!(format!("{curve}").ends_with(">>, hsy_transform=none, dynamic>"));

    // The curves with custom slopes can be applied and inverted.
    let t = curve.clone();
    let mut t2 = t.clone();
    t2.make_non_dynamic();
    let fwd = processor(t2.clone(), TransformDirection::Forward).default_cpu_processor();
    let inv = processor(t2, TransformDirection::Inverse).default_cpu_processor();
    let mut px = [0.3f32, 0.4, 0.6];
    fwd.apply_rgb(&mut px);
    inv.apply_rgb(&mut px);
    for (a, b) in px.iter().zip([0.3f32, 0.4, 0.6]) {
        assert!((a - b).abs() < 1e-4, "{px:?}");
    }
}

#[test]
fn transform_create_group_transform() {
    let mut t = GradingHueCurveTransform::new(GradingStyle::Video);
    t.value.curve_mut(H::HueLum).control_points[2].y = 1.3;
    t.rgb_to_hsy = HsyTransformStyle::None;
    let proc = processor(t.clone(), TransformDirection::Forward);
    assert_eq!(
        proc.create_group_transform().transforms,
        vec![Transform::GradingHueCurve(t)]
    );
}
