//! Tests ported from `GradingPrimary_tests.cpp`, `GradingPrimaryOpData_tests.cpp`,
//! `GradingPrimaryOp_tests.cpp`, `GradingPrimaryOpCPU_tests.cpp`,
//! `GradingPrimaryTransform_tests.cpp` and the grading primary parts of
//! `DynamicProperty_tests.cpp`.

#![allow(clippy::excessive_precision)]

use super::*;
use crate::processor::Processor;
use crate::transforms::grading::GradingRgbm;
use crate::transforms::GroupTransform;

pub(crate) fn processor(t: impl Into<Transform>, dir: TransformDirection) -> Processor {
    Processor::from_transform(&Config::create_raw(), &Context::new(), &t.into(), dir).unwrap()
}

/// Compare images like OCIO's `ValidateImage` (non SSE tolerance is 1e-6, a
/// slightly larger absolute tolerance is used to be robust to libm differences).
pub(crate) fn validate_image(expected: &[f32], res: &[Pixel], error: f32, line: u32) {
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
                assert!(
                    (e - r).abs() <= error,
                    "line {line}: pixel {i} channel {j}: expected {e}, got {r}"
                );
            }
        }
    }
}

pub(crate) fn to_pixels(data: &[f32]) -> Vec<Pixel> {
    data.chunks(4).map(|c| [c[0], c[1], c[2], c[3]]).collect()
}

const ERROR: f32 = 1e-6;

fn apply_op(op: &dyn Op, input: &[f32]) -> Vec<Pixel> {
    let mut px = to_pixels(input);
    op.apply(&mut px);
    px
}

// GradingPrimary_tests.cpp

#[test]
fn precompute() {
    let mut gp = GradingPrimary::new(GradingStyle::Log);
    let fwd = TransformDirection::Forward;

    let mut comp = GradingPrimaryPreRender::default();
    comp.update(GradingStyle::Log, fwd, &gp);
    assert_eq!(comp.brightness(), [0.0, 0.0, 0.0]);
    assert_eq!(comp.contrast(), [1.0, 1.0, 1.0]);
    assert_eq!(comp.gamma(), [1.0, 1.0, 1.0]);
    assert!((comp.pivot() - 0.4).abs() < 1e-6);
    assert!(comp.local_bypass());
    assert!(comp.is_gamma_identity());

    gp.saturation = 0.5;
    comp.update(GradingStyle::Log, fwd, &gp);
    assert!(!comp.local_bypass());
    gp.saturation = 1.0;
    comp.update(GradingStyle::Log, fwd, &gp);
    assert!(comp.local_bypass());

    gp.brightness.green = 0.1 * 1023.0 / 6.25;
    comp.update(GradingStyle::Log, fwd, &gp);
    assert_eq!(comp.brightness(), [0.0, 0.1, 0.0]);
    assert!(!comp.local_bypass());
    assert!(comp.is_gamma_identity());

    gp.brightness.red = 0.1 * 1023.0 / 6.25;
    gp.brightness.green = 0.0;
    gp.contrast.red = 0.0; // Inverse will be 1.
    gp.contrast.green = 1.25;
    gp.gamma.blue = 0.8;
    gp.pivot = 1.0;
    comp.update(GradingStyle::Log, fwd, &gp);
    assert_eq!(comp.brightness(), [0.1, 0.0, 0.0]);
    assert_eq!(comp.contrast(), [0.0, 1.25, 1.0]);
    assert_eq!(comp.gamma(), [1.0, 1.0, 1.25]);
    assert!((comp.pivot() - 1.0).abs() < 1e-6);
    assert!(!comp.is_gamma_identity());

    comp.update(GradingStyle::Log, TransformDirection::Inverse, &gp);
    assert_eq!(comp.brightness(), [-0.1, 0.0, 0.0]);
    assert_eq!(comp.contrast(), [1.0, 0.8, 1.0]);
    assert_eq!(comp.gamma(), [1.0, 1.0, 0.8]);

    let mut gp = GradingPrimary::new(GradingStyle::Log);

    // Identity checks for GRADING_LOG.
    gp.gamma.red = 0.8;
    comp.update(GradingStyle::Log, fwd, &gp);
    assert!(!comp.is_gamma_identity());
    assert!(!comp.local_bypass());
    gp.gamma.red = 1.0;
    comp.update(GradingStyle::Log, fwd, &gp);
    assert!(comp.is_gamma_identity());
    assert!(comp.local_bypass());

    // Identity checks for GRADING_LIN.
    gp.contrast.red = 0.8;
    comp.update(GradingStyle::Lin, fwd, &gp);
    assert!(!comp.is_contrast_identity());
    assert!(!comp.local_bypass());
    gp.contrast.red = 1.0;
    comp.update(GradingStyle::Lin, fwd, &gp);
    assert!(comp.is_contrast_identity());
    assert!(comp.local_bypass());

    // Identity checks for GRADING_VIDEO.
    gp.gamma.red = 0.8;
    comp.update(GradingStyle::Video, fwd, &gp);
    assert!(!comp.is_gamma_identity());
    assert!(!comp.local_bypass());
    gp.gamma.red = 1.0;
    comp.update(GradingStyle::Video, fwd, &gp);
    assert!(comp.is_gamma_identity());
    assert!(comp.local_bypass());
}

// GradingPrimaryOpData_tests.cpp

#[test]
fn op_data_accessors() {
    let op = GradingPrimaryOp::identity(GradingStyle::Lin);
    assert_eq!(op.style(), GradingStyle::Lin);
    assert_eq!(op.value(), GradingPrimary::new(GradingStyle::Lin));
    assert_eq!(op.direction(), TransformDirection::Forward);

    let gp = GradingPrimaryOp::new(
        GradingStyle::Log,
        GradingPrimary::new(GradingStyle::Log),
        TransformDirection::Inverse,
        false,
    )
    .unwrap();
    assert_eq!(gp.direction(), TransformDirection::Inverse);
    assert!(gp.is_no_op());
    assert!(gp.is_identity());
    assert!(!gp.has_channel_crosstalk());

    let expected = "<GradingPrimaryOp log inverse <brightness=<r=0, g=0, b=0, m=0>, \
                    contrast=<r=1, g=1, b=1, m=1>, gamma=<r=1, g=1, b=1, m=1>, \
                    offset=<r=0, g=0, b=0, m=0>, exposure=<r=0, g=0, b=0, m=0>, \
                    lift=<r=0, g=0, b=0, m=0>, gain=<r=1, g=1, b=1, m=1>, \
                    saturation=1, pivot=<contrast=-0.2, black=0, white=1>>>";
    assert_eq!(gp.cache_id(), expected);

    // Id of the metadata is part of the cache id.
    let mut gp_id = gp.clone();
    let mut md = FormatMetadata::default();
    md.set_id("uid");
    gp_id.set_metadata(md);
    assert!(gp_id
        .cache_id()
        .starts_with("<GradingPrimaryOp uid log inverse <brightness="));

    // IsIdentity.
    let mut v1 = GradingPrimary::new(GradingStyle::Log);
    v1.brightness.red += 0.1;
    v1.pivot_black += 0.1;
    let gp1 =
        GradingPrimaryOp::new(GradingStyle::Log, v1, TransformDirection::Inverse, false).unwrap();
    assert!(!gp1.is_identity());

    let mut v3 = GradingPrimary::new(GradingStyle::Lin);
    assert!(
        GradingPrimaryOp::new(GradingStyle::Lin, v3, TransformDirection::Forward, false)
            .unwrap()
            .is_identity()
    );
    v3.clamp_black = 0.5;
    let gp3 =
        GradingPrimaryOp::new(GradingStyle::Lin, v3, TransformDirection::Forward, false).unwrap();
    assert!(!gp3.is_identity());

    // Channel crosstalk.
    assert!(!gp3.has_channel_crosstalk());
    v3.saturation = 0.5;
    let gp3 =
        GradingPrimaryOp::new(GradingStyle::Lin, v3, TransformDirection::Forward, false).unwrap();
    assert!(gp3.has_channel_crosstalk());

    // isInverse.
    let fwd =
        GradingPrimaryOp::new(GradingStyle::Log, v1, TransformDirection::Forward, false).unwrap();
    assert!(gp1.is_inverse(&fwd));
    assert!(fwd.inverse().cache_id() == gp1.cache_id());
    let mut v1b = v1;
    v1b.pivot_black += 0.1;
    let other =
        GradingPrimaryOp::new(GradingStyle::Log, v1b, TransformDirection::Inverse, false).unwrap();
    assert!(!other.is_inverse(&fwd));
    assert!(!fwd.is_inverse(&fwd));
    // Different styles.
    let lin =
        GradingPrimaryOp::new(GradingStyle::Lin, v1, TransformDirection::Inverse, false).unwrap();
    assert!(!lin.is_inverse(&fwd));
    // Dynamic ops are never inverse.
    let dyn_op =
        GradingPrimaryOp::new(GradingStyle::Log, v1, TransformDirection::Inverse, true).unwrap();
    assert!(!dyn_op.is_inverse(&fwd));
}

#[test]
fn op_data_validate() {
    let op = GradingPrimaryOp::identity(GradingStyle::Log);
    let mut v = op.value();

    let check = |v: GradingPrimary, msg: &str| {
        let err = GradingPrimaryOp::new(GradingStyle::Log, v, TransformDirection::Forward, false)
            .unwrap_err();
        assert!(err.message().contains(msg), "{}", err.message());
    };

    v.gamma.red = 0.0001;
    check(
        v,
        "GradingPrimary gamma '<r=0.0001, g=1, b=1, m=1>' are below lower bound (0.01)",
    );
    v.gamma.red = 1.0;
    v.gamma.green = 0.0001;
    check(v, "are below lower bound (0.01)");
    v.gamma.green = 1.0;
    v.gamma.blue = 0.0001;
    check(v, "are below lower bound (0.01)");
    v.gamma.blue = 1.0;
    v.gamma.master = 0.0001;
    check(v, "are below lower bound (0.01)");
    v.gamma.master = 1.0;
    assert!(
        GradingPrimaryOp::new(GradingStyle::Log, v, TransformDirection::Forward, false).is_ok()
    );

    v.pivot_black = 0.5;
    v.pivot_white = 0.4;
    check(v, "black pivot should be smaller than white pivot");
    v.pivot_black = 0.0;
    assert!(
        GradingPrimaryOp::new(GradingStyle::Log, v, TransformDirection::Forward, false).is_ok()
    );

    v.clamp_black = 0.5;
    v.clamp_white = 0.4;
    check(v, "black clamp should be smaller than white clamp");
    v.clamp_black = 0.0;
    assert!(
        GradingPrimaryOp::new(GradingStyle::Log, v, TransformDirection::Forward, false).is_ok()
    );
}

#[test]
fn op_data_dynamic() {
    let op = GradingPrimaryOp::new(
        GradingStyle::Lin,
        GradingPrimary::new(GradingStyle::Lin),
        TransformDirection::Forward,
        true,
    )
    .unwrap();
    assert!(op.is_dynamic());
    let dp = op
        .dynamic_property(DynamicPropertyType::GradingPrimary)
        .unwrap();
    assert_eq!(dp.property_type(), DynamicPropertyType::GradingPrimary);
    assert!(op.dynamic_property(DynamicPropertyType::Exposure).is_none());

    let mut gdp = GradingPrimary::new(GradingStyle::Lin);
    gdp.pivot_black = 0.01;
    dp.as_grading_primary().unwrap().set(gdp);
    assert_eq!(op.value().pivot_black, 0.01);

    let non_dyn = op.make_non_dynamic().unwrap();
    assert!(!non_dyn.is_dynamic());
    let non_dyn = non_dyn.downcast_ref::<GradingPrimaryOp>().unwrap();
    assert_eq!(non_dyn.value().pivot_black, 0.01);
    // Changing the property no longer changes the non dynamic op.
    gdp.pivot_black = 0.02;
    dp.as_grading_primary().unwrap().set(gdp);
    assert_eq!(non_dyn.value().pivot_black, 0.01);
    assert_eq!(op.value().pivot_black, 0.02);

    // A non dynamic op has no property.
    assert!(GradingPrimaryOp::identity(GradingStyle::Lin)
        .make_non_dynamic()
        .is_none());
}

// GradingPrimaryOp_tests.cpp

#[test]
fn op_create() {
    let mut ops = OpVec::new();
    let value = GradingPrimary::new(GradingStyle::Log);
    let fwd = TransformDirection::Forward;
    create_grading_primary_op(&mut ops, GradingStyle::Log, &value, fwd, false, fwd).unwrap();
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].name(), "GradingPrimary");
    assert!(ops[0].is_identity());
    assert!(ops[0].is_no_op());

    create_grading_primary_op(&mut ops, GradingStyle::Log, &value, fwd, true, fwd).unwrap();
    assert_eq!(ops.len(), 2);
    assert!(!ops[1].is_identity());
    assert!(!ops[1].is_no_op());

    // The direction is combined with the op direction.
    create_grading_primary_op(
        &mut ops,
        GradingStyle::Log,
        &value,
        TransformDirection::Inverse,
        false,
        TransformDirection::Inverse,
    )
    .unwrap();
    assert_eq!(
        ops[2]
            .downcast_ref::<GradingPrimaryOp>()
            .unwrap()
            .direction(),
        fwd
    );
    create_grading_primary_op(
        &mut ops,
        GradingStyle::Log,
        &value,
        fwd,
        false,
        TransformDirection::Inverse,
    )
    .unwrap();
    assert_eq!(
        ops[3]
            .downcast_ref::<GradingPrimaryOp>()
            .unwrap()
            .direction(),
        TransformDirection::Inverse
    );
}

#[test]
fn op_create_transform() {
    let value = GradingPrimary::new(GradingStyle::Log);
    let op =
        GradingPrimaryOp::new(GradingStyle::Log, value, TransformDirection::Forward, true).unwrap();
    let t = op.to_transform().unwrap();
    match t {
        Transform::GradingPrimary(t) => {
            assert_eq!(t.style, GradingStyle::Log);
            assert!(t.is_dynamic());
            assert_eq!(t.value, value);
        }
        _ => panic!("wrong transform type"),
    }
}

#[test]
fn op_build_ops() {
    let config = Config::create_raw();
    let context = Context::new();
    let mut gp_transform = GradingPrimaryTransform::new(GradingStyle::Log);

    // Identity does create an op.
    let mut ops = OpVec::new();
    gp_transform
        .build_ops(&mut ops, &config, &context, TransformDirection::Forward)
        .unwrap();
    assert_eq!(ops.len(), 1);
    assert!(ops[0].is_identity());
    assert!(ops[0].is_no_op());
    ops.clear();

    // Make it dynamic and keep default values.
    gp_transform.make_dynamic();
    gp_transform
        .build_ops(&mut ops, &config, &context, TransformDirection::Forward)
        .unwrap();
    assert_eq!(ops.len(), 1);
    assert!(ops[0].is_dynamic());
    let gpo = ops[0].downcast_ref::<GradingPrimaryOp>().unwrap();
    assert_eq!(gpo.value().pivot_black, 0.0);

    // Changing the source does not change the op.
    let mut vals = GradingPrimary::new(GradingStyle::Log);
    vals.pivot_black = 0.1;
    gp_transform.set_value(vals).unwrap();
    assert_eq!(gpo.value().pivot_black, 0.0);

    let proc = processor(gp_transform.clone(), TransformDirection::Forward);
    assert!(proc.has_dynamic_property(DynamicPropertyType::GradingPrimary));
    assert!(!proc.has_dynamic_property(DynamicPropertyType::Exposure));

    let cpu = proc.default_cpu_processor();
    let dp = cpu
        .dynamic_property(DynamicPropertyType::GradingPrimary)
        .unwrap();
    let dpgp = dp.as_grading_primary().unwrap();
    assert!(cpu
        .dynamic_property(DynamicPropertyType::Exposure)
        .is_none());

    let mut pixel = [0.0f32, 0.2, 2.0];
    cpu.apply_rgb(&mut pixel);
    // Default values are identity.
    let error = 1e-5f32;
    assert!((pixel[0] - 0.0).abs() < error);
    assert!((pixel[1] - 0.2).abs() < error);
    assert!((pixel[2] - 2.0).abs() < 5.0 * error);

    // Add clamping and update dynamic property.
    vals.clamp_black = 0.1;
    vals.clamp_white = 1.0;
    dpgp.set(vals);

    cpu.apply_rgb(&mut pixel);
    assert!((pixel[0] - 0.1).abs() < error);
    assert!((pixel[1] - 0.2).abs() < error);
    assert!((pixel[2] - 1.0).abs() < error);

    // An invalid value is ignored (the last valid one is kept).
    let mut bad = vals;
    bad.gamma.red = 0.0;
    dpgp.set(bad);
    let mut pixel = [0.0f32, 0.2, 2.0];
    cpu.apply_rgb(&mut pixel);
    assert!((pixel[0] - 0.1).abs() < error);
    assert!((pixel[2] - 1.0).abs() < error);
}

#[test]
fn build_ops_validates() {
    let mut t = GradingPrimaryTransform::new(GradingStyle::Log);
    t.value.gamma.red = 0.0;
    let mut ops = OpVec::new();
    let err = t
        .build_ops(
            &mut ops,
            &Config::create_raw(),
            &Context::new(),
            TransformDirection::Forward,
        )
        .unwrap_err();
    assert_eq!(
        err.message(),
        "GradingPrimary gamma '<r=0, g=1, b=1, m=1>' are below lower bound (0.01)."
    );
    assert!(ops.is_empty());
    assert_eq!(
        t.validate().unwrap_err().message(),
        "GradingPrimaryTransform validation failed: GradingPrimary gamma '<r=0, g=1, b=1, m=1>' are below \
         lower bound (0.01)."
    );
}

#[test]
fn pair_identity_optimization() {
    let mut v = GradingPrimary::new(GradingStyle::Log);
    v.gamma = GradingRgbm::new(1.1, 1.2, 1.3, 1.0);
    let fwd =
        GradingPrimaryOp::new(GradingStyle::Log, v, TransformDirection::Forward, false).unwrap();
    let inv = fwd.inverse();
    let ops: OpVec = vec![Arc::new(fwd.clone()), Arc::new(inv.clone())];

    let opt = crate::processor::optimize_ops(&ops, OptimizationFlags::DEFAULT);
    assert!(opt.is_empty());
    let opt = crate::processor::optimize_ops(&ops, OptimizationFlags::NONE);
    assert_eq!(opt.len(), 2);
    let opt = crate::processor::optimize_ops(
        &ops,
        OptimizationFlags(
            OptimizationFlags::DEFAULT.0 & !OptimizationFlags::PAIR_IDENTITY_GRADING.0,
        ),
    );
    assert_eq!(opt.len(), 2);

    // Dynamic ops are not removed.
    let dyn_fwd =
        GradingPrimaryOp::new(GradingStyle::Log, v, TransformDirection::Forward, true).unwrap();
    let ops: OpVec = vec![Arc::new(dyn_fwd), Arc::new(inv)];
    assert_eq!(
        crate::processor::optimize_ops(&ops, OptimizationFlags::DEFAULT).len(),
        2
    );
}

#[test]
#[ignore = "needs-merge"]
fn pair_identity_optimization_with_clamp() {
    // The pair is replaced by a range emulating the clamps.
    let mut v = GradingPrimary::new(GradingStyle::Log);
    v.gamma = GradingRgbm::new(1.1, 1.2, 1.3, 1.0);
    v.clamp_black = 0.1;
    v.clamp_white = 0.9;
    let fwd =
        GradingPrimaryOp::new(GradingStyle::Log, v, TransformDirection::Forward, false).unwrap();
    let ops: OpVec = vec![Arc::new(fwd.clone()), Arc::new(fwd.inverse())];
    let opt = crate::processor::optimize_ops(&ops, OptimizationFlags::DEFAULT);
    assert_eq!(opt.len(), 1);
    assert!(opt[0].downcast_ref::<GradingPrimaryOp>().is_none());
    let mut px = [[0.0f32, 0.5, 1.0, 1.0]];
    opt[0].apply(&mut px);
    assert_eq!(px[0], [0.1, 0.5, 0.9, 1.0]);
}

#[test]
fn pair_with_clamp_kept_without_range() {
    // Without a range op available, the pair is not removed but the result is still right.
    let mut v = GradingPrimary::new(GradingStyle::Lin);
    v.clamp_black = 0.1;
    v.clamp_white = 0.9;
    v.exposure.master = 0.5;
    let fwd =
        GradingPrimaryOp::new(GradingStyle::Lin, v, TransformDirection::Forward, false).unwrap();
    let ops: OpVec = vec![Arc::new(fwd.clone()), Arc::new(fwd.inverse())];
    let opt = crate::processor::optimize_ops(&ops, OptimizationFlags::DEFAULT);
    let is_range = opt.len() == 1 && opt[0].downcast_ref::<GradingPrimaryOp>().is_none();
    assert!(opt.len() == 2 || is_range);
    if opt.len() == 2 {
        let mut px = [[0.0f32, 0.5, 1.0, 1.0]];
        crate::ops::apply_ops(&opt, &mut px);
        assert!((px[0][0] - 0.1 / 2.0f32.sqrt()).abs() < 1e-6);
        assert!((px[0][2] - 0.9 / 2.0f32.sqrt()).abs() < 1e-6);
    }
}

// GradingPrimaryOpCPU_tests.cpp

#[test]
fn cpu_identity() {
    let qnan = f32::NAN;
    let inf = f32::INFINITY;
    #[rustfmt::skip]
    let image = [
        -0.50, -0.25, 0.50, 0.0,
         0.75,  1.00, 1.25, 1.0,
         1.25,  1.50, 1.75, 0.0,
         qnan,  qnan, qnan, 0.0,
          0.0,   0.0,  0.0, qnan,
          inf,   inf,  inf, 0.0,
          0.0,   0.0,  0.0,  inf,
         -inf,  -inf, -inf, 0.0,
          0.0,   0.0,  0.0, -inf,
    ];
    for style in [GradingStyle::Log, GradingStyle::Lin, GradingStyle::Video] {
        for dir in [TransformDirection::Forward, TransformDirection::Inverse] {
            let op = GradingPrimaryOp::new(style, GradingPrimary::new(style), dir, false).unwrap();
            let res = apply_op(&op, &image);
            validate_image(&image, &res, ERROR, line!());
        }
    }
}

mod ts1 {
    use super::*;
    pub const STYLE: GradingStyle = GradingStyle::Log;
    pub const BRIGHTNESS: GradingRgbm = GradingRgbm::new(-10., 45., -5., 50.);
    pub const CONTRAST: GradingRgbm = GradingRgbm::new(0.9, 1.4, 0.7, 0.75);
    pub const GAMMA: GradingRgbm = GradingRgbm::new(1.1, 0.7, 1.05, 1.15);
    pub const PIVOT: f64 = -0.3;
    pub const SATURATION: f64 = 1.21;
    pub const CLAMP_BLACK: f64 = -0.05;
    pub const CLAMP_WHITE: f64 = 1.50;
    pub const PIVOT_BLACK: f64 = 0.05;
    pub const PIVOT_WHITE: f64 = 0.9;
    #[rustfmt::skip]
    pub const INPUT: [f32; 8] = [
         0.1, 0.9, 1.2, 1.0,
        -0.4, 0.2, 1.2, 0.5];
    #[rustfmt::skip]
    pub const EXPECTED: [f32; 8] = [
         0.23327083, 1.77384381, 0.86027701, 1.0,
        -0.10117631, 0.79016840, 1.02051931, 0.5];
    #[rustfmt::skip]
    pub const EXPECTED_CLAMP: [f32; 8] = [
         0.23327083, 1.50000000, 0.86027701, 1.0,
        -0.05000000, 0.79016840, 1.02051931, 0.5];
    #[rustfmt::skip]
    pub const EXPECTED_WBPIVOT: [f32; 8] = [
         0.21137053, 1.82456972, 0.83339811, 1.0,
        -0.16370305, 0.81365125, 0.99945772, 0.5];
}

#[test]
fn cpu_log() {
    use ts1::*;
    let mut gdp = GradingPrimary::new(STYLE);
    gdp.brightness = BRIGHTNESS;
    gdp.contrast = CONTRAST;
    gdp.gamma = GAMMA;
    gdp.pivot = PIVOT;
    gdp.saturation = SATURATION;

    // Forward direction, dynamic.
    let op = GradingPrimaryOp::new(STYLE, gdp, TransformDirection::Forward, true).unwrap();
    validate_image(&EXPECTED, &apply_op(&op, &INPUT), ERROR, line!());

    let dp = op
        .dynamic_property(DynamicPropertyType::GradingPrimary)
        .unwrap();
    let dpgp = dp.as_grading_primary().unwrap();

    gdp.clamp_black = CLAMP_BLACK;
    gdp.clamp_white = CLAMP_WHITE;
    dpgp.set(gdp);
    validate_image(&EXPECTED_CLAMP, &apply_op(&op, &INPUT), ERROR, line!());

    gdp.clamp_black = -100.0;
    gdp.clamp_white = 100.0;
    gdp.pivot_black = PIVOT_BLACK;
    gdp.pivot_white = PIVOT_WHITE;
    dpgp.set(gdp);
    validate_image(&EXPECTED_WBPIVOT, &apply_op(&op, &INPUT), ERROR, line!());

    // Inverse direction.
    let mut gdp = GradingPrimary::new(STYLE);
    gdp.brightness = BRIGHTNESS;
    gdp.contrast = CONTRAST;
    gdp.gamma = GAMMA;
    gdp.pivot = PIVOT;
    gdp.saturation = SATURATION;
    let op = GradingPrimaryOp::new(STYLE, gdp, TransformDirection::Inverse, true).unwrap();
    validate_image(&INPUT, &apply_op(&op, &EXPECTED), ERROR, line!());

    let dp = op
        .dynamic_property(DynamicPropertyType::GradingPrimary)
        .unwrap();
    let dpgp = dp.as_grading_primary().unwrap();
    // Clamping prevents full inversion. Skip.
    gdp.pivot_black = PIVOT_BLACK;
    gdp.pivot_white = PIVOT_WHITE;
    dpgp.set(gdp);
    validate_image(&INPUT, &apply_op(&op, &EXPECTED_WBPIVOT), ERROR, line!());
}

mod ts2 {
    use super::*;
    pub const STYLE: GradingStyle = GradingStyle::Lin;
    pub const EXPOSURE: GradingRgbm = GradingRgbm::new(0.5, -0.2, 0.4, -0.25);
    pub const OFFSET: GradingRgbm = GradingRgbm::new(-0.03, 0.02, 0.1, -0.1);
    pub const CONTRAST: GradingRgbm = GradingRgbm::new(0.9, 1.4, 0.7, 0.75);
    pub const PIVOT: f64 = 0.5;
    pub const SATURATION: f64 = 1.33;
    pub const CLAMP_BLACK: f64 = -0.40;
    pub const CLAMP_WHITE: f64 = 1.05;
    #[rustfmt::skip]
    pub const INPUT: [f32; 8] = [
         0.1, 0.9, 1.2, 1.0,
        -0.1, 0.9, 3.2, 0.5];
    #[rustfmt::skip]
    pub const EXPECTED: [f32; 8] = [
        -0.24746465, 0.67575505, 0.64940625, 1.0,
        -0.50871492, 0.68002410, 1.19721858, 0.5];
    #[rustfmt::skip]
    pub const EXPECTED_CLAMP: [f32; 8] = [
        -0.24746465, 0.67575505, 0.64940625, 1.0,
        -0.40000000, 0.68002410, 1.05000000, 0.5];
}

#[test]
fn cpu_lin() {
    use ts2::*;
    let mut gdp = GradingPrimary::new(STYLE);
    gdp.exposure = EXPOSURE;
    gdp.offset = OFFSET;
    gdp.contrast = CONTRAST;
    gdp.pivot = PIVOT;
    gdp.saturation = SATURATION;

    let op = GradingPrimaryOp::new(STYLE, gdp, TransformDirection::Forward, false).unwrap();
    validate_image(&EXPECTED, &apply_op(&op, &INPUT), ERROR, line!());

    gdp.clamp_black = CLAMP_BLACK;
    gdp.clamp_white = CLAMP_WHITE;
    let op = GradingPrimaryOp::new(STYLE, gdp, TransformDirection::Forward, false).unwrap();
    validate_image(&EXPECTED_CLAMP, &apply_op(&op, &INPUT), ERROR, line!());

    // Inverse direction.
    gdp.clamp_black = -100.0;
    gdp.clamp_white = 100.0;
    let op = GradingPrimaryOp::new(STYLE, gdp, TransformDirection::Inverse, false).unwrap();
    validate_image(&INPUT, &apply_op(&op, &EXPECTED), ERROR, line!());
}

mod ts3 {
    use super::*;
    pub const STYLE: GradingStyle = GradingStyle::Video;
    pub const LIFT: GradingRgbm = GradingRgbm::new(0.05, -0.04, 0.02, 0.05);
    pub const GAMMA: GradingRgbm = GradingRgbm::new(0.9, 1.4, 0.7, 0.75);
    pub const GAIN: GradingRgbm = GradingRgbm::new(1.2, 1.1, 1.25, 0.8);
    pub const OFFSET: GradingRgbm = GradingRgbm::new(-0.03, 0.02, 0.1, -0.1);
    pub const SATURATION: f64 = 1.2;
    pub const CLAMP_BLACK: f64 = -0.15;
    pub const CLAMP_WHITE: f64 = 1.50;
    pub const PIVOT_BLACK: f64 = 0.05;
    pub const PIVOT_WHITE: f64 = 0.9;
    #[rustfmt::skip]
    pub const INPUT: [f32; 8] = [
         0.1, 0.9, 1.2, 1.0,
        -0.1, 0.9, 1.2, 0.5];
    #[rustfmt::skip]
    pub const EXPECTED: [f32; 8] = [
        -0.10667760, 0.75643484, 1.53729499, 1.0,
        -0.17148458, 0.75881552, 1.53967567, 0.5];
    #[rustfmt::skip]
    pub const EXPECTED_CLAMP: [f32; 8] = [
        -0.10667760, 0.75643484, 1.50000000, 1.0,
        -0.15000000, 0.75881552, 1.50000000, 0.5];
    #[rustfmt::skip]
    pub const EXPECTED_WBPIVOT: [f32; 8] = [
        -0.06553329, 0.74984638, 1.67741281, 1.0,
        -0.14759934, 0.75286107, 1.68042750, 0.5];
}

#[test]
fn cpu_video() {
    use ts3::*;
    let base = || {
        let mut gdp = GradingPrimary::new(STYLE);
        gdp.lift = LIFT;
        gdp.gamma = GAMMA;
        gdp.gain = GAIN;
        gdp.offset = OFFSET;
        gdp.saturation = SATURATION;
        gdp
    };
    let mut gdp = base();
    let op = GradingPrimaryOp::new(STYLE, gdp, TransformDirection::Forward, false).unwrap();
    validate_image(&EXPECTED, &apply_op(&op, &INPUT), ERROR, line!());

    gdp.clamp_black = CLAMP_BLACK;
    gdp.clamp_white = CLAMP_WHITE;
    let op = GradingPrimaryOp::new(STYLE, gdp, TransformDirection::Forward, false).unwrap();
    validate_image(&EXPECTED_CLAMP, &apply_op(&op, &INPUT), ERROR, line!());

    gdp.clamp_black = -100.0;
    gdp.clamp_white = 100.0;
    gdp.pivot_black = PIVOT_BLACK;
    gdp.pivot_white = PIVOT_WHITE;
    let op = GradingPrimaryOp::new(STYLE, gdp, TransformDirection::Forward, false).unwrap();
    validate_image(&EXPECTED_WBPIVOT, &apply_op(&op, &INPUT), ERROR, line!());

    // Inverse direction.
    let mut gdp = base();
    let op = GradingPrimaryOp::new(STYLE, gdp, TransformDirection::Inverse, false).unwrap();
    validate_image(&INPUT, &apply_op(&op, &EXPECTED), ERROR, line!());

    // Clamping prevents full inversion. Skip.
    gdp.pivot_black = PIVOT_BLACK;
    gdp.pivot_white = PIVOT_WHITE;
    let op = GradingPrimaryOp::new(STYLE, gdp, TransformDirection::Inverse, false).unwrap();
    validate_image(&INPUT, &apply_op(&op, &EXPECTED_WBPIVOT), ERROR, line!());
}

// GradingPrimaryTransform_tests.cpp

#[test]
fn transform_basic() {
    assert_eq!(GradingPrimary::NO_CLAMP_WHITE, f64::MAX);
    assert_eq!(GradingPrimary::NO_CLAMP_BLACK, -f64::MAX);

    let gdp_lin = GradingPrimary::new(GradingStyle::Lin);
    assert_eq!(gdp_lin.brightness, GradingRgbm::new(0.0, 0.0, 0.0, 0.0));
    assert_eq!(gdp_lin.contrast, GradingRgbm::new(1.0, 1.0, 1.0, 1.0));
    assert_eq!(gdp_lin.gamma, GradingRgbm::new(1.0, 1.0, 1.0, 1.0));
    assert_eq!(gdp_lin.offset, GradingRgbm::new(0.0, 0.0, 0.0, 0.0));
    assert_eq!(gdp_lin.exposure, GradingRgbm::new(0.0, 0.0, 0.0, 0.0));
    assert_eq!(gdp_lin.lift, GradingRgbm::new(0.0, 0.0, 0.0, 0.0));
    assert_eq!(gdp_lin.gain, GradingRgbm::new(1.0, 1.0, 1.0, 1.0));
    assert_eq!(gdp_lin.pivot, 0.18);
    assert_eq!(gdp_lin.saturation, 1.0);
    assert_eq!(gdp_lin.clamp_white, GradingPrimary::NO_CLAMP_WHITE);
    assert_eq!(gdp_lin.clamp_black, GradingPrimary::NO_CLAMP_BLACK);
    assert_eq!(gdp_lin.pivot_white, 1.0);
    assert_eq!(gdp_lin.pivot_black, 0.0);

    let gdp_log = GradingPrimary::new(GradingStyle::Log);
    assert_ne!(gdp_log, gdp_lin);
    assert_eq!(gdp_log.pivot, -0.2);
    let mut edit = gdp_log;
    assert_eq!(gdp_log, edit);
    edit.pivot = gdp_lin.pivot;
    assert_eq!(edit, gdp_lin);

    let gdp_vid = GradingPrimary::new(GradingStyle::Video);
    assert_eq!(gdp_vid, gdp_lin);

    let gpt_lin = GradingPrimaryTransform::new(GradingStyle::Lin);
    assert_eq!(gpt_lin.style, GradingStyle::Lin);
    assert_eq!(gpt_lin.direction, TransformDirection::Forward);
    assert_eq!(gpt_lin.value, gdp_lin);
    assert!(gpt_lin.validate().is_ok());
    let gpt_log = GradingPrimaryTransform::new(GradingStyle::Log);
    assert_eq!(gpt_log.value, gdp_log);
    assert!(gpt_log.validate().is_ok());
    let gpt_vid = GradingPrimaryTransform::new(GradingStyle::Video);
    assert_eq!(gpt_vid.value, gdp_vid);
    assert!(gpt_vid.validate().is_ok());

    let mut gpt = gpt_lin.clone();
    assert_eq!(gpt, gpt_lin);
    gpt.direction = TransformDirection::Inverse;
    gpt.set_style(GradingStyle::Video);
    assert_eq!(gpt.style, GradingStyle::Video);

    let mut v = gpt.value;
    v.pivot = 0.24;
    gpt.set_value(v).unwrap();
    assert_eq!(gpt.value.pivot, 0.24);

    // Changing the style resets the values.
    gpt.set_style(GradingStyle::Log);
    assert_eq!(gpt.value, gdp_log);
    gpt.direction = TransformDirection::Forward;
    v.gamma = GradingRgbm::new(0.00001, 1.0, 1.0, 1.0);
    assert!(gpt
        .set_value(v)
        .unwrap_err()
        .message()
        .contains("GradingPrimary gamma '<r=1e-05, g=1, b=1, m=1>' are below lower bound (0.01)"));
}

#[test]
fn transform_dynamic() {
    let mut gpt = GradingPrimaryTransform::new(GradingStyle::Log);
    assert!(!gpt.is_dynamic());
    gpt.make_dynamic();
    assert!(gpt.is_dynamic());
    gpt.make_non_dynamic();
    assert!(!gpt.is_dynamic());
}

fn apply_rgb(proc: &Processor, px: &[f32; 3]) -> [f32; 3] {
    let cpu = proc.default_cpu_processor();
    let mut p = *px;
    cpu.apply_rgb(&mut p);
    p
}

fn close3(a: &[f32; 3], b: &[f32; 3], error: f32) {
    for i in 0..3 {
        assert!((a[i] - b[i]).abs() <= error, "{a:?} != {b:?}");
    }
}

#[test]
fn transform_processor_several_transforms() {
    let src = [0.2f32, 0.3, 0.4];

    let mut gpa = GradingPrimary::new(GradingStyle::Log);
    gpa.gamma = GradingRgbm::new(1.1, 1.2, 1.3, 1.0);
    let mut gpta = GradingPrimaryTransform::new(GradingStyle::Log);
    gpta.set_value(gpa).unwrap();

    let proc_a = processor(gpta.clone(), TransformDirection::Forward);
    let pixel_a = apply_rgb(&proc_a, &src);
    let pixel_aa = apply_rgb(&proc_a, &pixel_a);

    let mut gpb = GradingPrimary::new(GradingStyle::Log);
    gpb.gamma = GradingRgbm::new(1.2, 1.4, 1.1, 1.0);
    gpb.saturation = 1.5;
    let mut gptb = GradingPrimaryTransform::new(GradingStyle::Log);
    gptb.set_value(gpb).unwrap();
    let proc_b = processor(gptb.clone(), TransformDirection::Forward);
    let pixel_ab = apply_rgb(&proc_b, &pixel_a);

    // Make second transform dynamic.
    gptb.make_dynamic();
    let error = 1e-6f32;

    // Two grading primary transforms where only the second one is dynamic.
    gptb.set_value(gpa).unwrap();
    let mut grp1 = GroupTransform::new();
    grp1.append(gpta.clone());
    grp1.append(gptb.clone());
    {
        let proc = processor(grp1, TransformDirection::Forward);
        let cpu = proc.default_cpu_processor();
        let dp = cpu
            .dynamic_property(DynamicPropertyType::GradingPrimary)
            .unwrap();
        let dp_val = dp.as_grading_primary().unwrap();

        let mut pixel = src;
        cpu.apply_rgb(&mut pixel);
        close3(&pixel, &pixel_aa, error);

        // Change the 2nd values to gpb.
        dp_val.set(gpb);
        let mut pixel = src;
        cpu.apply_rgb(&mut pixel);
        close3(&pixel, &pixel_ab, error);
    }

    // Both transforms dynamic: the processor can still be created.
    gpta.make_dynamic();
    let mut grp2 = GroupTransform::new();
    grp2.append(gpta);
    grp2.append(gptb);
    let _ = processor(grp2, TransformDirection::Forward);
}

#[test]
fn transform_several_transforms_switch() {
    let src = [0.2f32, 0.3, 0.4];

    let mut gpa = GradingPrimary::new(GradingStyle::Log);
    gpa.gamma = GradingRgbm::new(1.1, 1.2, 1.3, 1.0);
    let mut gpta = GradingPrimaryTransform::new(GradingStyle::Log);
    gpta.set_value(gpa).unwrap();

    let mut gpb = GradingPrimary::new(GradingStyle::Log);
    gpb.gamma = GradingRgbm::new(1.2, 1.4, 1.1, 1.0);
    gpb.saturation = 1.5;
    let mut gptb = GradingPrimaryTransform::new(GradingStyle::Log);
    gptb.set_value(gpb).unwrap();

    gpta.make_dynamic();

    let mut grp = GroupTransform::new();
    grp.append(gpta.clone());
    grp.append(gptb.clone());
    let proc = processor(grp, TransformDirection::Forward);
    let cpu = proc.default_cpu_processor();

    let error = 1e-6f32;
    let mut pixel = src;
    let dp = cpu
        .dynamic_property(DynamicPropertyType::GradingPrimary)
        .unwrap();
    let dp_val = dp.as_grading_primary().unwrap();
    {
        let mut val = dp_val.get();
        val.saturation += 0.15;
        dp_val.set(val);
        let mut temp = src;
        cpu.apply_rgb(&mut temp);

        val.brightness.red += 0.11;
        val.gamma.master += 0.123456;
        dp_val.set(val);
        cpu.apply_rgb(&mut pixel);

        assert!((0..3).any(|i| (temp[i] - pixel[i]).abs() > error * pixel[i].abs()));
    }

    // Make the first transform non-dynamic and the second transform dynamic.
    let val = dp_val.get();
    gpta.set_value(val).unwrap();
    gpta.make_non_dynamic();
    gptb.make_dynamic();

    let mut grp = GroupTransform::new();
    grp.append(gpta);
    grp.append(gptb);
    let proc = processor(grp, TransformDirection::Forward);
    let cpu = proc.default_cpu_processor();

    let mut pixel2 = src;
    cpu.apply_rgb(&mut pixel2);
    close3(&pixel, &pixel2, error);

    let dp = cpu
        .dynamic_property(DynamicPropertyType::GradingPrimary)
        .unwrap();
    let dp_val = dp.as_grading_primary().unwrap();
    let mut val = dp_val.get();
    val.saturation += 0.15;
    dp_val.set(val);
    let mut temp = src;
    cpu.apply_rgb(&mut temp);
    assert!((0..3).any(|i| (temp[i] - pixel[i]).abs() > error * pixel[i].abs()));
}

#[test]
fn transform_serialization() {
    let mut data = GradingPrimary::new(GradingStyle::Log);
    data.gamma = GradingRgbm::new(1.1, 1.2, 1.3, 1.0);
    let mut primary = GradingPrimaryTransform::new(GradingStyle::Log);
    primary.set_value(data).unwrap();

    let expected = "<GradingPrimaryTransform direction=forward, style=log, \
                    values=<brightness=<r=0, g=0, b=0, m=0>, contrast=<r=1, g=1, b=1, m=1>, \
                    gamma=<r=1.1, g=1.2, b=1.3, m=1>, offset=<r=0, g=0, b=0, m=0>, \
                    exposure=<r=0, g=0, b=0, m=0>, lift=<r=0, g=0, b=0, m=0>, \
                    gain=<r=1, g=1, b=1, m=1>, saturation=1, pivot=<contrast=-0.2, black=0, white=1>>>";
    assert_eq!(format!("{primary}"), expected);
    primary.make_dynamic();
    assert!(format!("{primary}").ends_with(">, dynamic>"));
}

#[test]
fn transform_log_contrast_inverse_apply() {
    let mut data = GradingPrimary::new(GradingStyle::Log);
    data.contrast = GradingRgbm::new(1.1, 0.9, 1.2, 1.0);
    let mut primary = GradingPrimaryTransform::new(GradingStyle::Log);
    primary.set_value(data).unwrap();

    let pixel_ref = [0.0f32; 3];
    let proc = processor(primary.clone(), TransformDirection::Forward);
    let proc2 = processor(primary.clone(), TransformDirection::Inverse);
    let pixel = apply_rgb(&proc2, &apply_rgb(&proc, &pixel_ref));
    let error = 1e-6f32;
    close3(&pixel, &pixel_ref, error);

    let pixel = apply_rgb(&proc, &pixel_ref);
    primary.direction = TransformDirection::Inverse;
    let proc3 = processor(primary, TransformDirection::Forward);
    let pixel = apply_rgb(&proc3, &pixel);
    close3(&pixel, &pixel_ref, error);
}

#[test]
fn transform_create_group_transform() {
    let mut data = GradingPrimary::new(GradingStyle::Video);
    data.gain = GradingRgbm::new(1.1, 1.2, 1.3, 1.0);
    let mut primary = GradingPrimaryTransform::new(GradingStyle::Video);
    primary.set_value(data).unwrap();
    primary.direction = TransformDirection::Inverse;
    let proc = processor(primary.clone(), TransformDirection::Forward);
    let grp = proc.create_group_transform();
    assert_eq!(grp.transforms.len(), 1);
    assert_eq!(grp.transforms[0], Transform::GradingPrimary(primary));
}

// DynamicProperty_tests.cpp (grading primary parts)

#[test]
fn dynamic_property_grading_primary() {
    let mut gp_transform = GradingPrimaryTransform::new(GradingStyle::Log);
    gp_transform.make_dynamic();

    let proc = processor(gp_transform.clone(), TransformDirection::Forward);
    let cpu = proc.default_cpu_processor();
    let dp = cpu
        .dynamic_property(DynamicPropertyType::GradingPrimary)
        .unwrap();
    let dp_val = dp.as_grading_primary().unwrap();

    let mut pixel = [0.5f32, 0.4, 0.2];
    cpu.apply_rgb(&mut pixel);
    close3(&pixel, &[0.5, 0.4, 0.2], 1e-5);

    let mut v = dp_val.get();
    v.saturation = 1.5;
    v.brightness.master = 5.0;
    dp_val.set(v);

    let mut pixel = [0.5f32, 0.4, 0.2];
    cpu.apply_rgb(&mut pixel);
    let static_t = {
        let mut t = gp_transform.clone();
        t.make_non_dynamic();
        t.set_value(v).unwrap();
        t
    };
    let expected = apply_rgb(
        &processor(static_t, TransformDirection::Forward),
        &[0.5, 0.4, 0.2],
    );
    close3(&pixel, &expected, 1e-7);

    // NO_DYNAMIC_PROPERTIES makes the processor non dynamic with the current values.
    let cpu_static = proc.optimized_cpu_processor(OptimizationFlags(
        OptimizationFlags::DEFAULT.0 | OptimizationFlags::NO_DYNAMIC_PROPERTIES.0,
    ));
    assert!(!cpu_static.is_dynamic());
    let mut pixel2 = [0.5f32, 0.4, 0.2];
    cpu_static.apply_rgb(&mut pixel2);
    close3(&pixel2, &expected, 1e-7);
}

#[test]
fn dynamic_property_get_as() {
    // Port of DynamicProperty_tests.cpp get_as: typed accessors of the property.
    let mut gplog = GradingPrimary::new(GradingStyle::Log);
    gplog.saturation = 1.21;
    let op =
        GradingPrimaryOp::new(GradingStyle::Log, gplog, TransformDirection::Forward, true).unwrap();
    let dp = op
        .dynamic_property(DynamicPropertyType::GradingPrimary)
        .unwrap();
    assert!(dp.as_double().is_none());
    assert!(dp.as_grading_tone().is_none());
    assert!(dp.as_grading_rgb_curve().is_none());
    assert!(dp.as_grading_hue_curve().is_none());
    let as_primary = dp.as_grading_primary().unwrap();
    assert_eq!(as_primary.get(), gplog);
    gplog.pivot = 0.12;
    as_primary.set(gplog);
    assert_eq!(op.value(), gplog);

    // Replacing the property shares it between ops.
    let mut other = GradingPrimaryOp::new(
        GradingStyle::Log,
        GradingPrimary::new(GradingStyle::Log),
        TransformDirection::Forward,
        true,
    )
    .unwrap();
    other.replace_dynamic_property(&dp);
    assert_eq!(other.value(), gplog);
    let p2 = other
        .dynamic_property(DynamicPropertyType::GradingPrimary)
        .unwrap();
    assert!(p2.as_grading_primary().unwrap().ptr_eq(as_primary));

    // A non dynamic op ignores the replacement.
    let mut static_op = GradingPrimaryOp::identity(GradingStyle::Log);
    static_op.replace_dynamic_property(&dp);
    assert!(!static_op.is_dynamic());
}
