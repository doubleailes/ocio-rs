//! Unit tests of the gamma op (ports of `GammaOpData_tests.cpp`,
//! `GammaOp_tests.cpp`, `GammaOpCPU_tests.cpp`, `GammaOpUtils_tests.cpp` and
//! `ExponentWithLinearTransform_tests.cpp`).

use super::*;
use crate::ops::matrix::test_utils::*;
use crate::processor::optimize_ops;
use crate::types::{METADATA_ID, METADATA_NAME};

const QNAN: f32 = f32::NAN;
const INF: f32 = f32::INFINITY;

fn g4(style: GammaStyle, r: &[f64], g: &[f64], b: &[f64], a: &[f64]) -> GammaOpData {
    GammaOpData::new(style, r.to_vec(), g.to_vec(), b.to_vec(), a.to_vec())
}

// GammaOpData_tests.cpp

#[test]
fn data_accessors() {
    let pr = [2.4, 0.1];
    let pg = [2.2, 0.2];
    let pb = [2.0, 0.4];
    let pa = [1.8, 0.6];
    let mut g1 = g4(GammaStyle::MoncurveFwd, &pr, &pg, &pb, &pa);
    assert_eq!(g1.red, pr);
    assert_eq!(g1.style, GammaStyle::MoncurveFwd);
    assert!(!g1.are_all_components_equal());
    assert!(!g1.is_non_channel_dependent());
    assert!(!g1.is_alpha_component_identity());

    g1.set_params(&pr);
    assert!(!g1.are_all_components_equal());
    assert!(g1.is_non_channel_dependent());
    assert!(g1.is_alpha_component_identity());
    assert_eq!(g1.green, pr);
    assert!(GammaOpData::is_identity_parameters(&g1.alpha, g1.style));

    g1.alpha = pr.to_vec();
    assert!(g1.are_all_components_equal());
    g1.blue = pb.to_vec();
    assert!(!g1.are_all_components_equal());
}

#[test]
fn data_identity_style_basic() {
    let id = GammaOpData::identity_parameters(GammaStyle::BasicFwd);
    let g = g4(GammaStyle::BasicFwd, &id, &id, &id, &id);
    assert!(g.is_identity());
    assert!(!g.is_no_op());
    assert!(g.is_channel_independent());

    let mut g = GammaOpData::default();
    g.set_params(&id);
    assert!(g.validate().is_ok());
    assert_eq!(g.style, GammaStyle::BasicFwd);
    assert!(g.is_identity());
    assert!(!g.is_no_op());

    let g = g4(GammaStyle::BasicFwd, &[1.2], &[1.6], &[2.0], &[3.1]);
    assert!(!g.is_identity());
    assert!(!g.is_no_op());

    let mut g = GammaOpData::default();
    assert!(g.is_identity());
    assert!(!g.is_no_op()); // Basic style clamps, so it isn't a no-op.
    g.set_params(&[1.2]);
    assert!(g.validate().is_ok());
    assert!(!g.is_identity());
    assert!(!g.is_no_op());
}

#[test]
fn data_identity_style_moncurve() {
    let id = GammaOpData::identity_parameters(GammaStyle::MoncurveFwd);
    let g = g4(GammaStyle::MoncurveFwd, &id, &id, &id, &id);
    assert!(g.is_identity());
    assert!(g.is_no_op());

    let mut g = GammaOpData {
        style: GammaStyle::MoncurveFwd,
        ..Default::default()
    };
    g.set_params(&id);
    assert!(g.validate().is_ok());
    assert!(g.is_identity());
    assert!(g.is_no_op());

    let g = g4(
        GammaStyle::MoncurveFwd,
        &[1.2, 0.2],
        &[1.6, 0.7],
        &[2.0, 0.5],
        &[3.1, 0.1],
    );
    assert!(!g.is_identity());
    assert!(!g.is_no_op());

    let mut g = GammaOpData {
        style: GammaStyle::MoncurveFwd,
        ..Default::default()
    };
    g.set_params(&[1.2, 0.2]);
    assert!(g.validate().is_ok());
    assert!(!g.is_identity());
    assert!(!g.is_no_op());
}

#[test]
fn data_validate() {
    let params = [2.6];
    let pr = [2.4, 0.1];
    let pg = [2.2, 0.2];
    let pb = [2.0, 0.4];
    let pa = [1.8, 0.6];
    let msg = |g: GammaOpData| g.validate().unwrap_err().message().to_string();
    assert!(msg(g4(GammaStyle::MoncurveFwd, &pr, &pg, &params, &pa))
        .contains("GammaOp: Wrong number of parameters"));
    assert!(msg(g4(GammaStyle::BasicFwd, &pb, &pb, &pb, &pb))
        .contains("GammaOp: Wrong number of parameters"));
    let p = [0.006];
    assert_eq!(
        msg(g4(GammaStyle::BasicFwd, &p, &p, &p, &p)),
        "Parameter 0.006 is less than lower bound 0.01"
    );
    let p = [110.];
    assert_eq!(
        msg(g4(GammaStyle::BasicFwd, &p, &p, &p, &p)),
        "Parameter 110 is greater than upper bound 100"
    );
    let p = [1., 11.];
    assert_eq!(
        msg(g4(GammaStyle::MoncurveFwd, &p, &p, &p, &p)),
        "Parameter 11 is greater than upper bound 0.9"
    );
    let p = [1., 0.];
    assert!(g4(GammaStyle::MoncurveFwd, &p, &p, &p, &p)
        .validate()
        .is_ok());
    let p = [1., -1e-6];
    assert_eq!(
        msg(g4(GammaStyle::MoncurveFwd, &p, &p, &p, &p)),
        "Parameter -1e-06 is less than lower bound 0"
    );
}

#[test]
fn data_equality() {
    let pr1 = [2.4, 0.1];
    let pg1 = [2.2, 0.2];
    let pb1 = [2.0, 0.4];
    let pa1 = [1.8, 0.6];
    let g1 = g4(GammaStyle::MoncurveFwd, &pr1, &pg1, &pb1, &pa1);
    let g2 = g4(GammaStyle::MoncurveFwd, &[2.6, 0.1], &pg1, &pb1, &pa1);
    assert!(g1 != g2);
    let mut g3 = g4(GammaStyle::MoncurveRev, &pr1, &pg1, &pb1, &pa1);
    assert!(g3 != g1);
    g3.style = g1.style;
    assert!(g3 == g1);
    let g4_ = g4(GammaStyle::MoncurveFwd, &pr1, &pg1, &pb1, &pa1);
    assert!(g4_ == g1);
}

fn check_gamma_inverse(ref_style: GammaStyle, p: [&[f64]; 4], inv_style: GammaStyle) {
    let r = g4(ref_style, p[0], p[1], p[2], p[3]);
    let inv = r.inverse();
    assert_eq!(inv.style, inv_style);
    assert_eq!(inv.red, p[0]);
    assert_eq!(inv.green, p[1]);
    assert_eq!(inv.blue, p[2]);
    assert_eq!(inv.alpha, p[3]);
    assert!(r.is_inverse(&inv));
    assert!(inv.is_inverse(&r));
    assert!(!r.is_inverse(&r));
    assert!(!inv.is_inverse(&inv));
}

#[test]
fn data_basic_inverse() {
    let p: [&[f64]; 4] = [&[2.2], &[2.4], &[2.6], &[2.8]];
    check_gamma_inverse(GammaStyle::BasicFwd, p, GammaStyle::BasicRev);
    check_gamma_inverse(GammaStyle::BasicRev, p, GammaStyle::BasicFwd);
}

#[test]
fn data_moncurve_inverse() {
    let p: [&[f64]; 4] = [&[2.4, 0.1], &[2.2, 0.2], &[2.0, 0.4], &[1.8, 0.6]];
    check_gamma_inverse(GammaStyle::MoncurveFwd, p, GammaStyle::MoncurveRev);
    check_gamma_inverse(GammaStyle::MoncurveRev, p, GammaStyle::MoncurveFwd);
}

#[test]
fn data_is_inverse() {
    let pr = [2.4];
    let pg = [2.41];
    let g1 = g4(GammaStyle::BasicFwd, &pr, &pg, &pr, &pr);
    let g2 = g4(GammaStyle::BasicRev, &pr, &pg, &pr, &pr);
    let g3 = g4(GammaStyle::BasicRev, &pr, &pg, &pg, &pr);
    assert!(g1.is_inverse(&g2));
    assert!(!g1.is_inverse(&g3));

    let pr = [2.4, 0.1];
    let pg = [2.41, 0.1];
    let g1 = g4(GammaStyle::MoncurveFwd, &pr, &pg, &pr, &pr);
    let g2 = g4(GammaStyle::MoncurveRev, &pr, &pg, &pr, &pr);
    let g3 = g4(GammaStyle::MoncurveRev, &pr, &pg, &pg, &pr);
    assert!(g1.is_inverse(&g2));
    assert!(!g1.is_inverse(&g3));
}

#[test]
fn data_may_compose() {
    use GammaStyle::*;
    let cases = [
        (BasicFwd, BasicFwd, true),
        (BasicFwd, BasicRev, true),
        (BasicRev, BasicRev, true),
        (BasicFwd, BasicMirrorFwd, true),
        (BasicFwd, BasicMirrorRev, true),
        (BasicRev, BasicMirrorFwd, true),
        (BasicRev, BasicMirrorRev, true),
        (BasicFwd, BasicPassThruFwd, true),
        (BasicFwd, BasicPassThruRev, true),
        (BasicRev, BasicPassThruFwd, true),
        (BasicRev, BasicPassThruRev, true),
        (BasicMirrorFwd, BasicMirrorFwd, true),
        (BasicMirrorRev, BasicMirrorRev, true),
        (BasicMirrorRev, BasicMirrorFwd, true),
        (BasicPassThruFwd, BasicPassThruFwd, true),
        (BasicPassThruRev, BasicPassThruRev, true),
        (BasicPassThruFwd, BasicPassThruRev, true),
        (BasicMirrorFwd, BasicPassThruFwd, false),
        (BasicMirrorFwd, BasicPassThruRev, false),
        (BasicMirrorRev, BasicPassThruFwd, false),
        (BasicMirrorRev, BasicPassThruRev, false),
    ];
    let p = [2.0];
    for (s1, s2, expected) in cases {
        let g1 = g4(s1, &p, &p, &p, &p);
        let g2 = g4(s2, &p, &p, &p, &p);
        assert_eq!(g1.may_compose(&g2), expected, "{s1:?} {s2:?}");
        assert_eq!(g2.may_compose(&g1), expected, "{s2:?} {s1:?}");
    }

    let p1 = [1.];
    let p2 = [2.2];
    let g1 = g4(BasicFwd, &p2, &p2, &p1, &p1);
    let g2 = g4(BasicFwd, &p2, &p2, &p2, &p1);
    assert!(g1.may_compose(&g2));

    let g1 = g4(BasicFwd, &p2, &p2, &p2, &p1);
    let g2 = g4(
        MoncurveFwd,
        &[2.6, 0.1],
        &[2.6, 0.1],
        &[2.6, 0.1],
        &[1.0, 0.0],
    );
    assert!(!g1.may_compose(&g2));
}

fn check_gamma_compose(
    s1: GammaStyle,
    p1: f64,
    s2: GammaStyle,
    p2: f64,
    ref_style: GammaStyle,
    ref_p: f64,
) {
    let a = [1.0];
    let g1 = g4(s1, &[p1], &[p1], &[p1], &a);
    let g2 = g4(s2, &[p2], &[p2], &[p2], &a);
    let g3 = g1.compose(&g2).unwrap();
    assert_eq!(g3.style, ref_style);
    assert_eq!(g3.red, vec![ref_p]);
    assert_eq!(g3.green, vec![ref_p]);
    assert_eq!(g3.blue, vec![ref_p]);
    assert_eq!(g3.alpha, vec![1.0]);
}

#[test]
fn data_compose() {
    use GammaStyle::*;
    check_gamma_compose(BasicFwd, 2., BasicFwd, 3., BasicFwd, 6.);
    check_gamma_compose(BasicRev, 2., BasicRev, 4., BasicRev, 8.);
    check_gamma_compose(BasicRev, 4., BasicFwd, 2., BasicRev, 2.);
    check_gamma_compose(BasicRev, 2., BasicFwd, 4., BasicFwd, 2.);
    check_gamma_compose(BasicFwd, 2., BasicRev, 4., BasicRev, 2.);
    check_gamma_compose(
        BasicPassThruFwd,
        2.,
        BasicPassThruRev,
        4.,
        BasicPassThruRev,
        2.,
    );
    check_gamma_compose(BasicMirrorFwd, 2., BasicMirrorRev, 4., BasicMirrorRev, 2.);
    check_gamma_compose(BasicMirrorFwd, 2., BasicRev, 4., BasicRev, 2.);
    check_gamma_compose(BasicPassThruFwd, 2., BasicRev, 4., BasicRev, 2.);

    let a = [1.0];
    let g1 = g4(BasicMirrorFwd, &[4.], &[4.], &[4.], &a);
    let g2 = g4(BasicPassThruFwd, &[2.], &[2.], &[2.], &a);
    assert_eq!(
        g1.compose(&g2).unwrap_err().message(),
        "GammaOp can only be combined with some GammaOps"
    );

    let g1 = g4(BasicRev, &[4.], &[4.], &[4.], &a);
    let p = [2., 0.1];
    let g2 = g4(MoncurveRev, &p, &p, &p, &[1.0, 0.0]);
    assert_eq!(
        g1.compose(&g2).unwrap_err().message(),
        "GammaOp can only be combined with some GammaOps"
    );
}

#[test]
fn data_styles() {
    for s in GammaStyle::ALL {
        assert_eq!(GammaStyle::parse(s.as_str()).unwrap(), s);
        assert_eq!(s.inverse().inverse(), s);
        assert_ne!(s.inverse().direction(), s.direction());
    }
    assert_eq!(
        GammaStyle::parse("MONCURVEFWD").unwrap(),
        GammaStyle::MoncurveFwd
    );
    assert_eq!(
        GammaStyle::parse("").unwrap_err().message(),
        "Missing gamma style."
    );
    assert_eq!(
        GammaStyle::parse("x").unwrap_err().message(),
        "Unknown gamma style: 'x'."
    );
}

// GammaOpUtils_tests.cpp

#[test]
fn utils_compute_params_forward() {
    let p = [2.0f32 as f64, 0.1f32 as f64];
    let r = compute_params_fwd(&p);
    assert_eq!(r.gamma, 2.0);
    assert_eq!(r.offset, (0.1 / (1. + 0.1)) as f32);
    assert_eq!(r.break_pnt, (0.1 / (2. - 1.)) as f32);
    assert_eq!(r.scale, (1. / (1. + 0.1)) as f32);
    assert!((r.slope - 0.33057851f32).abs() <= 1e-7);
}

#[test]
fn utils_compute_params_reverse() {
    let p = [2.0f32 as f64, 0.1f32 as f64];
    let r = compute_params_rev(&p);
    assert_eq!(r.gamma, 0.5);
    assert_eq!(r.offset, 0.1);
    assert_eq!(r.scale, 1.0f32 + 0.1f32);
    assert!((r.break_pnt - 0.03305785f32).abs() <= 1e-7);
    assert!((r.slope - 3.02499986f32).abs() <= 1e-7);
}

// GammaOp_tests.cpp

#[test]
fn op_combining() {
    let mut d1 = g4(GammaStyle::BasicFwd, &[1.201], &[1.201], &[1.201], &[1.]);
    d1.metadata.add_attribute(METADATA_NAME, "gamma1");
    d1.metadata.add_attribute(METADATA_ID, "ID1");
    d1.metadata.add_attribute("Attrib", "1");
    d1.metadata.add_attribute("Attrib1", "10");
    d1.metadata.add_child_element("Gamma1Child", "Some content");
    let mut d2 = g4(GammaStyle::BasicFwd, &[2.345], &[2.345], &[2.345], &[1.]);
    d2.metadata.add_attribute(METADATA_NAME, "gamma2");
    d2.metadata.add_attribute(METADATA_ID, "ID2");
    d2.metadata.add_attribute("Attrib", "2");
    d2.metadata.add_attribute("Attrib2", "20");
    d2.metadata
        .add_child_element("Gamma2Child", "Other content");

    let mut ops = OpVec::new();
    create_gamma_op(&mut ops, &d1, TransformDirection::Forward).unwrap();
    create_gamma_op(&mut ops, &d2, TransformDirection::Forward).unwrap();
    let c = ops[0]
        .combine_with(ops[1].as_ref(), OptimizationFlags::DEFAULT)
        .unwrap();
    assert_eq!(c.len(), 1);
    let g = c[0].downcast_ref::<GammaOp>().unwrap().data();
    assert_eq!(g.metadata.name(), "gamma1 + gamma2");
    assert_eq!(g.metadata.id(), "ID1 + ID2");
    assert_eq!(g.metadata.attributes.len(), 5);
    assert_eq!(
        g.metadata.attributes[2],
        ("Attrib".to_string(), "1 + 2".to_string())
    );
    assert_eq!(
        g.metadata.attributes[3],
        ("Attrib1".to_string(), "10".to_string())
    );
    assert_eq!(
        g.metadata.attributes[4],
        ("Attrib2".to_string(), "20".to_string())
    );
    assert_eq!(g.metadata.children.len(), 2);
    assert_eq!(g.metadata.children[0], d1.metadata.children[0]);
    assert_eq!(g.metadata.children[1], d2.metadata.children[0]);
    assert_eq!(g.red[0], 1.201 * 2.345);
    assert_eq!(g.green[0], 1.201 * 2.345);
    assert_eq!(g.blue[0], 1.201 * 2.345);
    assert_eq!(g.alpha[0], 1.0);

    // No composition without the flag.
    assert!(ops[0]
        .combine_with(
            ops[1].as_ref(),
            OptimizationFlags::DEFAULT & !OptimizationFlags::COMP_GAMMA
        )
        .is_none());
    // Moncurve can't be composed.
    let m = g4(
        GammaStyle::MoncurveFwd,
        &[2.0, 0.1],
        &[2.0, 0.1],
        &[2.0, 0.1],
        &[1.0, 0.0],
    );
    let mut ops2 = OpVec::new();
    create_gamma_op(&mut ops2, &m, TransformDirection::Forward).unwrap();
    create_gamma_op(&mut ops2, &m, TransformDirection::Forward).unwrap();
    assert!(ops2[0]
        .combine_with(ops2[1].as_ref(), OptimizationFlags::ALL)
        .is_none());
}

#[test]
fn op_basic() {
    let (r, g, b, a) = ([1.001], [1.], [2.], [1.]);
    let d = g4(GammaStyle::BasicFwd, &r, &g, &b, &a);
    let op0 = GammaOp::new(d).unwrap();
    assert_eq!(op0.data().style, GammaStyle::BasicFwd);
    assert_eq!(op0.data().red, r);
    let d2 = g4(GammaStyle::BasicRev, &r, &g, &b, &a);
    let mut ops = OpVec::new();
    create_gamma_op(&mut ops, &d2, TransformDirection::Forward).unwrap();
    let op1 = ops[0].downcast_ref::<GammaOp>().unwrap();
    assert!(op0.data().is_inverse(op1.data()));

    // The optimizer replaces the pair by a range clamping negative values.
    let pair: OpVec = vec![Arc::new(op0.clone()), ops[0].clone()];
    let opt = optimize_ops(&pair, OptimizationFlags::DEFAULT);
    assert_eq!(opt.len(), 1);
    let rd = opt[0].downcast_ref::<RangeOp>().unwrap().data();
    assert_eq!(rd.min_in, 0.0);
    assert!(rd.max_is_empty());
    // Without the flag, the pair is composed into a clamping identity gamma,
    // itself replaced by the range.
    let opt = optimize_ops(
        &pair,
        OptimizationFlags::DEFAULT & !OptimizationFlags::PAIR_IDENTITY_GAMMA,
    );
    assert_eq!(opt.len(), 1);
    assert_eq!(opt[0].name(), "Range");
    // With no identity replacement, the composed identity gamma is kept.
    let flags = OptimizationFlags::COMP_GAMMA;
    let opt = optimize_ops(&pair, flags);
    assert_eq!(opt.len(), 1);
    assert_eq!(opt[0].name(), "Gamma");
    assert!(opt[0]
        .downcast_ref::<GammaOp>()
        .unwrap()
        .data()
        .is_identity());
}

#[test]
fn op_mirror_pair_removed() {
    let d = g4(GammaStyle::BasicMirrorFwd, &[2.2], &[2.2], &[2.2], &[1.]);
    let mut ops = OpVec::new();
    create_gamma_op(&mut ops, &d, TransformDirection::Forward).unwrap();
    create_gamma_op(&mut ops, &d, TransformDirection::Inverse).unwrap();
    assert!(optimize_ops(&ops, OptimizationFlags::DEFAULT).is_empty());
    let m = g4(
        GammaStyle::MoncurveFwd,
        &[2.4, 0.055],
        &[2.4, 0.055],
        &[2.4, 0.055],
        &[1.0, 0.0],
    );
    let mut ops = OpVec::new();
    create_gamma_op(&mut ops, &m, TransformDirection::Inverse).unwrap();
    create_gamma_op(&mut ops, &m, TransformDirection::Forward).unwrap();
    assert!(optimize_ops(&ops, OptimizationFlags::DEFAULT).is_empty());
    assert_eq!(optimize_ops(&ops, OptimizationFlags::NONE).len(), 2);
}

#[test]
fn op_identity_replacement() {
    let id = [1.0];
    let d = g4(GammaStyle::BasicFwd, &id, &id, &id, &id);
    let mut ops = OpVec::new();
    create_gamma_op(&mut ops, &d, TransformDirection::Forward).unwrap();
    assert!(!ops[0].is_no_op());
    let opt = optimize_ops(&ops, OptimizationFlags::DEFAULT);
    assert_eq!(opt.len(), 1);
    assert_eq!(opt[0].name(), "Range");
    // Kept without IDENTITY_GAMMA.
    let opt = optimize_ops(
        &ops,
        OptimizationFlags::DEFAULT & !OptimizationFlags::IDENTITY_GAMMA,
    );
    assert_eq!(opt[0].name(), "Gamma");

    // A non-clamping identity is a no-op.
    let d = g4(GammaStyle::BasicPassThruRev, &id, &id, &id, &id);
    let mut ops = OpVec::new();
    create_gamma_op(&mut ops, &d, TransformDirection::Forward).unwrap();
    assert!(ops[0].is_no_op());
    assert!(optimize_ops(&ops, OptimizationFlags::DEFAULT).is_empty());
}

#[test]
fn op_computed_identifier() {
    let r = [1.001];
    let mut g = vec![1.];
    let b = [1.];
    let a = [1.];
    let mut ops = OpVec::new();
    create_gamma_op(
        &mut ops,
        &g4(GammaStyle::BasicFwd, &r, &g, &b, &a),
        TransformDirection::Forward,
    )
    .unwrap();
    g[0] = 1.001;
    let gamma2 = g4(GammaStyle::BasicFwd, &r, &g, &b, &a);
    create_gamma_op(&mut ops, &gamma2, TransformDirection::Forward).unwrap();
    assert_ne!(ops[0].cache_id(), ops[1].cache_id());
    create_gamma_op(&mut ops, &gamma2, TransformDirection::Forward).unwrap();
    assert_ne!(ops[0].cache_id(), ops[2].cache_id());
    assert_eq!(ops[1].cache_id(), ops[2].cache_id());
    create_gamma_op(
        &mut ops,
        &g4(GammaStyle::BasicRev, &r, &g, &b, &a),
        TransformDirection::Forward,
    )
    .unwrap();
    for i in 0..3 {
        assert_ne!(ops[i].cache_id(), ops[3].cache_id());
    }
    assert_eq!(
        ops[0].cache_id(),
        "<GammaOp basicFwd r:1.001 g:1 b:1 a:1  >"
    );
}

#[test]
fn op_create_transform() {
    let (r, g, b, a) = ([2., 0.2], [3., 0.3], [4., 0.4], [2.5, 0.25]);
    let mut gamma = g4(GammaStyle::MoncurveFwd, &r, &g, &b, &a);
    gamma.metadata.add_attribute("name", "test");
    let mut ops = OpVec::new();
    create_gamma_op(&mut ops, &gamma, TransformDirection::Forward).unwrap();
    match ops[0].to_transform().unwrap() {
        Transform::ExponentWithLinear(t) => {
            assert_eq!(t.negative_style, NegativeStyle::Linear);
            assert_eq!(
                t.metadata.attributes,
                vec![("name".to_string(), "test".to_string())]
            );
            assert_eq!(t.direction, TransformDirection::Forward);
            assert_eq!(t.gamma, [2., 3., 4., 2.5]);
            assert_eq!(t.offset, [0.2, 0.3, 0.4, 0.25]);
        }
        _ => panic!("expected an exponent with linear transform"),
    }

    let mut gamma0 = g4(GammaStyle::BasicRev, &[2.], &[3.], &[4.], &[2.5]);
    gamma0.metadata.add_attribute("name", "test");
    create_gamma_op(&mut ops, &gamma0, TransformDirection::Forward).unwrap();
    match ops[1].to_transform().unwrap() {
        Transform::Exponent(t) => {
            assert_eq!(
                t.metadata.attributes,
                vec![("name".to_string(), "test".to_string())]
            );
            assert_eq!(t.direction, TransformDirection::Inverse);
            assert_eq!(t.value, [2., 3., 4., 2.5]);
            assert_eq!(t.negative_style, NegativeStyle::Clamp);
        }
        _ => panic!("expected an exponent transform"),
    }

    let gamma1 = g4(GammaStyle::MoncurveMirrorFwd, &r, &g, &b, &a);
    create_gamma_op(&mut ops, &gamma1, TransformDirection::Forward).unwrap();
    match ops[2].to_transform().unwrap() {
        Transform::ExponentWithLinear(t) => assert_eq!(t.negative_style, NegativeStyle::Mirror),
        _ => panic!("expected an exponent with linear transform"),
    }
}

// GammaOpCPU_tests.cpp

fn apply_gamma(
    style: GammaStyle,
    params: [&[f64]; 4],
    input: &[f32],
    expected: &[f32],
    threshold: f32,
) {
    let d = g4(style, params[0], params[1], params[2], params[3]);
    let mut ops = OpVec::new();
    create_gamma_op(&mut ops, &d, TransformDirection::Forward).unwrap();
    let ops = optimize_ops(&ops, OptimizationFlags::DEFAULT);
    assert_eq!(ops.len(), 1);
    let out = apply_ops(&ops, input);
    for i in 0..input.len() {
        if expected[i].is_nan() {
            assert!(out[i].is_nan(), "index {i}: {} is not NaN", out[i]);
            continue;
        }
        assert!(
            equal_with_safe_rel_error(out[i], expected[i], threshold, 1.0),
            "index {i}: {} vs {} (threshold {threshold})",
            out[i],
            expected[i]
        );
    }
}

const INPUT_7: [f32; 28] = [
    -1.0, -0.75, -0.25, 0.0, -0.0025, 0.0, 0.00005, 0.5, 0.0005, 0.005, 0.05, 0.75, 0.25, 0.5,
    0.75, 1.0, 0.80, 0.95, 1.0, 1.5, 1.005, 1.05, 1.5, -0.25, -INF, INF, QNAN, 0.0,
];

const INPUT_9: [f32; 36] = [
    0.0005, 0.005, 0.05, 0.75, -0.0005, -0.005, -0.05, -0.75, 0.25, 0.5, 0.75, 1.0, -0.25, -0.5,
    -0.75, -1.0, 0.80, 0.95, 1.0, 1.5, -0.80, -0.95, -1.0, -1.5, 1.005, 1.05, 1.5, 0.25, -1.005,
    -1.05, -1.5, -0.25, -INF, INF, QNAN, 0.0,
];

#[test]
fn cpu_apply_basic_style_fwd() {
    let gv = [1.2, 2.12, 1., 1.05];
    let i = &INPUT_7;
    let p = |k: usize, g: f64| i[k].powf(g as f32);
    let expected = [
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.00005,
        0.48296818,
        0.00010933,
        0.00001323,
        0.05,
        0.73928916,
        p(12, gv[0]),
        p(13, gv[1]),
        p(14, gv[2]),
        p(15, gv[3]),
        p(16, gv[0]),
        p(17, gv[1]),
        p(18, gv[2]),
        p(19, gv[3]),
        1.00600302,
        1.10897374,
        1.5,
        0.0,
        0.0,
        INF,
        0.0,
        0.0,
    ];
    apply_gamma(
        GammaStyle::BasicFwd,
        [&[gv[0]], &[gv[1]], &[gv[2]], &[gv[3]]],
        i,
        &expected,
        1e-7,
    );
}

#[test]
fn cpu_apply_basic_style_rev() {
    let gv = [1.2, 2.12, 1.123, 1.05];
    let i = &INPUT_7;
    let p = |k: usize, g: f64| i[k].powf((1. / g) as f32);
    let expected = [
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.00014792,
        0.51677888,
        0.00177476,
        0.08215017,
        0.06941755,
        0.76034504,
        p(12, gv[0]),
        p(13, gv[1]),
        p(14, gv[2]),
        p(15, gv[3]),
        p(16, gv[0]),
        p(17, gv[1]),
        p(18, gv[2]),
        p(19, gv[3]),
        1.00416493,
        1.02328109,
        1.43484282,
        0.0,
        0.0,
        INF,
        0.0,
        0.0,
    ];
    apply_gamma(
        GammaStyle::BasicRev,
        [&[gv[0]], &[gv[1]], &[gv[2]], &[gv[3]]],
        i,
        &expected,
        1e-7,
    );
}

fn mirror_expected(inv: bool, pass_thru: bool) -> Vec<f32> {
    let gv = [1.2, 2.12, 1.123, 1.05];
    let i = &INPUT_9;
    let mut e = Vec::new();
    for block in [0usize, 8, 16, 24] {
        for c in 0..4 {
            let g = if inv { 1.0 / gv[c] } else { gv[c] };
            e.push(i[block + c].powf(g as f32));
        }
        for c in 0..4 {
            let g = if inv { 1.0 / gv[c] } else { gv[c] };
            e.push(if pass_thru {
                i[block + 4 + c]
            } else {
                -(i[block + c].powf(g as f32))
            });
        }
    }
    e.extend_from_slice(&[-INF, INF, QNAN, 0.0]);
    e
}

#[test]
fn cpu_apply_basic_mirror_style() {
    let gv = [1.2, 2.12, 1.123, 1.05];
    let params: [&[f64]; 4] = [&[gv[0]], &[gv[1]], &[gv[2]], &[gv[3]]];
    apply_gamma(
        GammaStyle::BasicMirrorFwd,
        params,
        &INPUT_9,
        &mirror_expected(false, false),
        1e-7,
    );
    apply_gamma(
        GammaStyle::BasicMirrorRev,
        params,
        &INPUT_9,
        &mirror_expected(true, false),
        1e-7,
    );
}

#[test]
fn cpu_apply_basic_pass_thru_style() {
    let gv = [1.2, 2.12, 1.123, 1.05];
    let params: [&[f64]; 4] = [&[gv[0]], &[gv[1]], &[gv[2]], &[gv[3]]];
    apply_gamma(
        GammaStyle::BasicPassThruFwd,
        params,
        &INPUT_9,
        &mirror_expected(false, true),
        1e-7,
    );
    apply_gamma(
        GammaStyle::BasicPassThruRev,
        params,
        &INPUT_9,
        &mirror_expected(true, true),
        1e-7,
    );
}

#[test]
fn cpu_apply_moncurve_style_fwd() {
    let expected = [
        -0.07738015,
        -0.33144456,
        -0.25,
        0.0,
        -0.00019345,
        0.0,
        0.00005,
        0.49101364,
        0.00003869,
        0.00220963,
        0.05,
        0.73652046,
        0.05087607,
        0.30550399,
        0.75,
        1.0,
        0.60382729,
        0.91061854,
        1.0,
        1.63146877,
        1.01141202,
        1.09396457,
        1.5,
        -0.24550682,
        -INF,
        INF,
        QNAN,
        0.0,
    ];
    apply_gamma(
        GammaStyle::MoncurveFwd,
        [&[2.4, 0.055], &[2.2, 0.2], &[1.0, 0.0], &[1.8, 0.6]],
        &INPUT_7,
        &expected,
        1e-7,
    );
}

#[test]
fn cpu_apply_moncurve_style_rev() {
    let expected = [
        -6.18606853,
        -1.69711625,
        -0.25,
        0.0,
        -0.01546517,
        0.0,
        0.00005,
        0.50915080,
        0.00309303,
        0.01131410,
        0.05,
        0.76367092,
        0.51735413,
        0.67568808,
        0.75,
        1.0,
        0.90233647,
        0.97234553,
        1.0,
        1.40423429,
        1.00228834,
        1.02691006,
        1.5,
        -0.25457540,
        -INF,
        INF,
        QNAN,
        0.0,
    ];
    apply_gamma(
        GammaStyle::MoncurveRev,
        [&[2.4, 0.1], &[2.2, 0.2], &[1.0, 0.0], &[1.8, 0.6]],
        &INPUT_7,
        &expected,
        1e-6,
    );
}

#[test]
fn cpu_apply_moncurve_mirror_style_fwd() {
    let mut input = INPUT_9;
    input[27] = 1.0;
    input[31] = -1.0;
    let expected = [
        0.00003869,
        0.00220963,
        0.04081632,
        0.73652046,
        -0.00003869,
        -0.00220963,
        -0.04081632,
        -0.73652046,
        0.05087607,
        0.30550399,
        0.67474484,
        1.0,
        -0.05087607,
        -0.30550399,
        -0.67474484,
        -1.0,
        0.60382729,
        0.91061854,
        1.0,
        1.63146877,
        -0.60382729,
        -0.91061854,
        -1.0,
        -1.63146877,
        1.01141202,
        1.09396457,
        1.84183657,
        1.0,
        -1.01141202,
        -1.09396457,
        -1.84183657,
        -1.0,
        -INF,
        INF,
        QNAN,
        0.0,
    ];
    apply_gamma(
        GammaStyle::MoncurveMirrorFwd,
        [&[2.4, 0.055], &[2.2, 0.2], &[2.0, 0.4], &[1.8, 0.6]],
        &input,
        &expected,
        1e-7,
    );
}

#[test]
fn cpu_apply_moncurve_mirror_style_rev() {
    let mut input = INPUT_9;
    input[19] = 0.75;
    input[23] = -0.75;
    input[27] = 1.0;
    input[31] = -1.0;
    let expected = [
        0.00309303,
        0.01131410,
        0.06125000,
        0.76367092,
        -0.00309303,
        -0.01131410,
        -0.06125000,
        -0.76367092,
        0.51735413,
        0.67568808,
        0.81243550,
        1.0,
        -0.51735413,
        -0.67568808,
        -0.81243550,
        -1.0,
        0.90233647,
        0.97234553,
        1.0,
        0.76367092,
        -0.90233647,
        -0.97234553,
        -1.0,
        -0.76367092,
        1.00228834,
        1.02691006,
        1.31464290,
        1.0,
        -1.00228834,
        -1.02691006,
        -1.31464290,
        -1.0,
        -INF,
        INF,
        QNAN,
        0.0,
    ];
    apply_gamma(
        GammaStyle::MoncurveMirrorRev,
        [&[2.4, 0.1], &[2.2, 0.2], &[2.0, 0.4], &[1.8, 0.6]],
        &input,
        &expected,
        1e-6,
    );
}

// ExponentWithLinearTransform_tests.cpp

#[test]
fn exponent_with_linear_transform_basic() {
    let mut t = ExponentWithLinearTransform::default();
    assert_eq!(t.direction, TransformDirection::Forward);
    assert_eq!(t.gamma, [1.0; 4]);
    assert_eq!(t.offset, [0.0; 4]);
    assert_eq!(t.negative_style, NegativeStyle::Linear);
    assert!(t.validate().is_ok());
    t.negative_style = NegativeStyle::Mirror;
    assert!(t.validate().is_ok());
    t.negative_style = NegativeStyle::PassThru;
    assert!(t
        .validate()
        .unwrap_err()
        .message()
        .contains("Pass thru negative extrapolation is not valid for MonCurve"));
    t.negative_style = NegativeStyle::Clamp;
    assert!(t
        .validate()
        .unwrap_err()
        .message()
        .contains("Clamp negative extrapolation is not valid"));
    t.negative_style = NegativeStyle::Linear;
    t.gamma = [0.5, 1.0, 1.0, 1.0];
    assert_eq!(
        t.validate().unwrap_err().message(),
        "ExponentWithLinearTransform validation failed: Parameter 0.5 is less than lower bound 1"
    );
}

#[test]
fn exponent_with_linear_transform_build_ops() {
    let config = Config::create_raw();
    let ctx = Context::new();
    // sRGB curve.
    let t = ExponentWithLinearTransform {
        gamma: [2.4, 2.4, 2.4, 1.0],
        offset: [0.055, 0.055, 0.055, 0.0],
        direction: TransformDirection::Inverse,
        ..Default::default()
    };
    let mut ops = OpVec::new();
    t.build_ops(&mut ops, &config, &ctx, TransformDirection::Forward)
        .unwrap();
    let d = ops[0].downcast_ref::<GammaOp>().unwrap().data();
    assert_eq!(d.style, GammaStyle::MoncurveRev);
    // Linear 0.18 to sRGB.
    let out = apply_ops(&ops, &[0.18, 0.0, 1.0, 1.0]);
    assert_close_f(out[0], 0.46135613, 1e-6);
    assert_eq!(out[1], 0.0);
    assert_close_f(out[2], 1.0, 1e-6);
    t.build_ops(&mut ops, &config, &ctx, TransformDirection::Inverse)
        .unwrap();
    assert_eq!(
        ops[1].downcast_ref::<GammaOp>().unwrap().data().style,
        GammaStyle::MoncurveFwd
    );
    let out2 = apply_ops(&ops, &[0.18, 0.5, 1.0, 1.0]);
    assert_close_f(out2[0], 0.18, 1e-6);
    assert_close_f(out2[1], 0.5, 1e-6);
    let t2 = t.clone();
    assert!(t.equals(&t2));
}
