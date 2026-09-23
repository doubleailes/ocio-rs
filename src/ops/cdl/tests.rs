//! Unit tests of the CDL op (ports of `CDLOpData_tests.cpp`, `CDLOp_tests.cpp`
//! and the CPU parts of `CDLTransform_tests.cpp`).

use super::*;
use crate::ops::exponent::ExponentOp;
use crate::ops::matrix::test_utils::*;
use crate::ops::matrix::MatrixOp;
use crate::ops::range::RangeOp;
use crate::processor::{optimize_ops, Processor};
use crate::transforms::{METADATA_INPUT_DESCRIPTION, METADATA_SAT_DESCRIPTION};
use crate::types::{METADATA_DESCRIPTION, METADATA_ID, METADATA_NAME};

const QNAN: f32 = f32::NAN;
const INF: f32 = f32::INFINITY;

fn cdl(style: CdlOpStyle, s: [f64; 3], o: [f64; 3], p: [f64; 3], sat: f64) -> CdlOpData {
    CdlOpData {
        style,
        slope: s,
        offset: o,
        power: p,
        saturation: sat,
        metadata: FormatMetadata::default(),
    }
}

// CDLOpData_tests.cpp

#[test]
fn data_accessors() {
    let mut c = cdl(
        CdlOpStyle::AscFwd,
        [1.35, 1.1, 0.71],
        [0.05, -0.23, 0.11],
        [0.93, 0.81, 1.27],
        1.23,
    );
    c.slope = [0.66; 3];
    assert!(channel_params_equal(&c.slope, &[0.66; 3]));
    assert!(channel_params_equal(&c.offset, &[0.05, -0.23, 0.11]));
    assert_eq!(c.saturation, 1.23);
    c.saturation = 0.99;
    assert_eq!(c.saturation, 0.99);
    assert!(c.validate().is_ok());
}

#[test]
fn data_constructors() {
    let d = CdlOpData::default();
    assert_eq!(d.id(), "");
    assert!(d.metadata.children.is_empty());
    assert_eq!(d.style, CdlOpStyle::NoClampFwd);
    assert!(!d.is_reverse());
    assert!(channel_params_equal(&d.slope, &[1.0; 3]));
    assert!(channel_params_equal(&d.offset, &[0.0; 3]));
    assert!(channel_params_equal(&d.power, &[1.0; 3]));
    assert_eq!(d.saturation, 1.0);

    let mut c = CdlOpData::new(
        CdlOpStyle::NoClampRev,
        [1.35, 1.1, 0.71],
        [0.05, -0.23, 0.11],
        [0.93, 0.81, 1.27],
        1.23,
    )
    .unwrap();
    c.metadata.add_attribute(METADATA_NAME, "cdl-name");
    c.metadata.add_attribute(METADATA_ID, "cdl-id");
    assert_eq!(c.metadata.name(), "cdl-name");
    assert_eq!(c.id(), "cdl-id");
    assert_eq!(c.style, CdlOpStyle::NoClampRev);
    assert!(c.is_reverse());
}

#[test]
fn data_inverse() {
    let mut c = cdl(
        CdlOpStyle::AscFwd,
        [1.35, 1.1, 0.71],
        [0.05, -0.23, 0.11],
        [0.93, 0.81, 1.27],
        1.23,
    );
    c.metadata.add_attribute(METADATA_ID, "test_id");
    c.metadata
        .add_child_element(METADATA_DESCRIPTION, "Inverse op test description");
    for (style, inv_style) in [
        (CdlOpStyle::AscFwd, CdlOpStyle::AscRev),
        (CdlOpStyle::AscRev, CdlOpStyle::AscFwd),
        (CdlOpStyle::NoClampFwd, CdlOpStyle::NoClampRev),
        (CdlOpStyle::NoClampRev, CdlOpStyle::NoClampFwd),
    ] {
        c.style = style;
        let inv = c.inverse();
        // Metadata is copied.
        assert_eq!(inv.id(), "test_id");
        assert_eq!(inv.metadata.children.len(), 1);
        assert_eq!(inv.metadata.children[0].element_name, METADATA_DESCRIPTION);
        assert_eq!(
            inv.metadata.children[0].element_value,
            "Inverse op test description"
        );
        assert_eq!(inv.style, inv_style);
        assert_eq!(inv.is_reverse(), inv_style.is_reverse());
        assert!(channel_params_equal(&inv.slope, &[1.35, 1.1, 0.71]));
        assert!(channel_params_equal(&inv.offset, &[0.05, -0.23, 0.11]));
        assert!(channel_params_equal(&inv.power, &[0.93, 0.81, 1.27]));
        assert_eq!(inv.saturation, 1.23);
    }
}

#[test]
fn data_style() {
    let mut c = CdlOpData::default();
    for style in [CdlOpStyle::AscFwd, CdlOpStyle::AscRev] {
        c.style = style;
        let r = c.identity_replacement().unwrap();
        assert!(r.has_min_in() && r.min_in == 0.);
        assert!(r.has_max_in() && r.max_in == 1.);
        assert!(r.has_min_out() && r.min_out == 0.);
        assert!(r.has_max_out() && r.max_out == 1.);
        assert!(!r.scales());
    }
    for style in [CdlOpStyle::NoClampFwd, CdlOpStyle::NoClampRev] {
        c.style = style;
        // Identity matrix.
        assert!(c.identity_replacement().is_none());
    }
    assert!(CdlOpStyle::parse("unknown_style")
        .unwrap_err()
        .message()
        .contains("Unknown style for CDL"));
    for (n, s) in [
        ("v1.2_Fwd", CdlOpStyle::AscFwd),
        ("Fwd", CdlOpStyle::AscFwd),
        ("v1.2_Rev", CdlOpStyle::AscRev),
        ("rev", CdlOpStyle::AscRev),
        ("noClampFwd", CdlOpStyle::NoClampFwd),
        ("FwdNoClamp", CdlOpStyle::NoClampFwd),
        ("noClampRev", CdlOpStyle::NoClampRev),
        ("RevNoClamp", CdlOpStyle::NoClampRev),
    ] {
        assert_eq!(CdlOpStyle::parse(n).unwrap(), s);
        assert_eq!(CdlOpStyle::parse(s.as_str()).unwrap(), s);
    }
}

#[test]
fn data_validation_success() {
    let mut c = CdlOpData {
        style: CdlOpStyle::AscFwd,
        ..Default::default()
    };
    c.slope = [1.15; 3];
    c.offset = [-0.02; 3];
    c.power = [0.97; 3];
    c.saturation = 1.22;
    assert!(!c.is_identity());
    assert!(!c.is_no_op());
    assert!(c.validate().is_ok());

    c.slope = [1.0; 3];
    c.offset = [0.0; 3];
    c.power = [1.0; 3];
    c.saturation = 1.0;
    assert!(c.is_identity());
    assert!(!c.is_no_op());
    c.style = CdlOpStyle::NoClampFwd;
    assert!(c.is_identity());
    assert!(c.is_no_op());
    assert!(c.validate().is_ok());

    c.slope = [0.0; 3];
    c.offset = [-0.02; 3];
    c.power = [0.97; 3];
    c.style = CdlOpStyle::AscFwd;
    assert!(!c.is_identity());
    assert!(c.validate().is_ok());

    c.slope = [1.15; 3];
    c.saturation = 0.0;
    assert!(!c.is_identity());
    assert!(c.validate().is_ok());
}

#[test]
fn data_validation_failure() {
    let mut c = CdlOpData {
        slope: [-0.9; 3],
        offset: [0.01; 3],
        power: [1.2; 3],
        saturation: 1.17,
        ..Default::default()
    };
    let e = c.validate().unwrap_err();
    assert!(e.message().contains("should be greater than 0"));
    assert_eq!(
        e.message(),
        "CDL: Invalid 'slope' -0.9 should be greater than 0."
    );
    c.slope = [0.9; 3];
    c.power = [-1.2; 3];
    assert_eq!(
        c.validate().unwrap_err().message(),
        "CDLOpData: Invalid 'power' -1.2 should be greater than 0."
    );
    c.power = [1.2; 3];
    c.saturation = -1.17;
    assert!(c
        .validate()
        .unwrap_err()
        .message()
        .contains("should be greater than 0"));
    c.slope = [0.7; 3];
    c.offset = [0.2; 3];
    c.power = [0.0; 3];
    c.saturation = 1.4;
    assert!(c
        .validate()
        .unwrap_err()
        .message()
        .contains("should be greater than 0"));
}

#[test]
fn data_channel() {
    assert!(!CdlOpData::default().has_channel_crosstalk());
    let c = CdlOpData {
        slope: [-0.9; 3],
        offset: [0.01; 3],
        power: [1.2; 3],
        ..Default::default()
    };
    assert!(!c.has_channel_crosstalk());
    let c = CdlOpData {
        saturation: 1.17,
        ..Default::default()
    };
    assert!(c.has_channel_crosstalk());
}

// CDLOp_tests.cpp

const D1_SLOPE: [f64; 3] = [1.35, 1.1, 0.071];
const D1_OFFSET: [f64; 3] = [0.05, -0.23, 0.11];
const D1_POWER: [f64; 3] = [0.93, 0.81, 1.27];
const D1_SAT: f64 = 1.23;

fn create_d1(ops: &mut OpVec, style: CdlOpStyle, sat: f64, dir: TransformDirection) {
    create_cdl_op(ops, style, &D1_SLOPE, &D1_OFFSET, &D1_POWER, sat, dir).unwrap();
}

#[test]
fn op_computed_identifier() {
    let f = TransformDirection::Forward;
    let mut ops = OpVec::new();
    create_d1(&mut ops, CdlOpStyle::AscFwd, D1_SAT, f);
    create_d1(&mut ops, CdlOpStyle::AscFwd, D1_SAT, f);
    assert_eq!(ops[0].cache_id(), ops[1].cache_id());

    let mut d = CdlOpData::new(CdlOpStyle::AscFwd, D1_SLOPE, D1_OFFSET, D1_POWER, D1_SAT).unwrap();
    d.metadata.add_attribute(METADATA_ID, "1");
    assert_eq!(d.id(), "1");
    create_cdl_op_from_data(&mut ops, &d, f).unwrap();
    assert_ne!(ops[0].cache_id(), ops[2].cache_id());

    let sat2 = D1_SAT + 0.002f32 as f64;
    create_d1(&mut ops, CdlOpStyle::AscFwd, sat2, f);
    create_d1(&mut ops, CdlOpStyle::AscFwd, sat2, f);
    for i in 0..3 {
        assert_ne!(ops[i].cache_id(), ops[3].cache_id());
        assert_ne!(ops[i].cache_id(), ops[4].cache_id());
    }
    assert_eq!(ops[3].cache_id(), ops[4].cache_id());
    create_d1(&mut ops, CdlOpStyle::NoClampFwd, sat2, f);
    assert_ne!(ops[3].cache_id(), ops[5].cache_id());
    assert_eq!(
        ops[0].cache_id(),
        "<CDLOp Fwd 1.35, 1.1, 0.071 0.05, -0.23, 0.11 0.93, 0.81, 1.27 1.23 >"
    );
}

fn cd(ops: &OpVec, i: usize) -> &CdlOpData {
    ops[i].downcast_ref::<CdlOp>().unwrap().data()
}

#[test]
fn op_is_inverse() {
    let f = TransformDirection::Forward;
    let i = TransformDirection::Inverse;
    let mut ops = OpVec::new();
    create_d1(&mut ops, CdlOpStyle::AscFwd, D1_SAT, f);
    create_d1(&mut ops, CdlOpStyle::AscFwd, D1_SAT, i);
    assert!(cd(&ops, 0).is_inverse(cd(&ops, 1)));
    assert!(cd(&ops, 1).is_inverse(cd(&ops, 0)));
    create_d1(&mut ops, CdlOpStyle::AscFwd, 1.30, i);
    assert!(!cd(&ops, 0).is_inverse(cd(&ops, 2)));
    assert!(!cd(&ops, 1).is_inverse(cd(&ops, 2)));
    assert!(!cd(&ops, 2).is_inverse(cd(&ops, 0)));
    assert!(!cd(&ops, 2).is_inverse(cd(&ops, 1)));
    create_d1(&mut ops, CdlOpStyle::AscRev, 1.30, i);
    assert!(cd(&ops, 2).is_inverse(cd(&ops, 3)));
    create_d1(&mut ops, CdlOpStyle::AscRev, 1.30, f);
    assert!(!cd(&ops, 2).is_inverse(cd(&ops, 4)));
    assert!(cd(&ops, 3).is_inverse(cd(&ops, 4)));
    create_d1(&mut ops, CdlOpStyle::NoClampFwd, 1.30, f);
    for k in 2..5 {
        assert!(!cd(&ops, k).is_inverse(cd(&ops, 5)));
    }
    create_d1(&mut ops, CdlOpStyle::NoClampFwd, 1.30, i);
    for k in 2..5 {
        assert!(!cd(&ops, k).is_inverse(cd(&ops, 6)));
    }
    assert!(cd(&ops, 5).is_inverse(cd(&ops, 6)));

    // The optimizer replaces a clamping pair by a range and removes a
    // non-clamping pair.
    let flags = OptimizationFlags::DEFAULT;
    let pair = vec![ops[0].clone(), ops[1].clone()];
    let opt = optimize_ops(&pair, flags);
    assert_eq!(opt.len(), 1);
    let r = opt[0].downcast_ref::<RangeOp>().unwrap().data();
    assert_eq!((r.min_in, r.max_in, r.min_out, r.max_out), (0., 1., 0., 1.));
    let pair = vec![ops[5].clone(), ops[6].clone()];
    assert!(optimize_ops(&pair, flags).is_empty());
    assert_eq!(
        optimize_ops(&pair, flags & !OptimizationFlags::PAIR_IDENTITY_CDL).len(),
        2
    );
}

fn apply_cdl(
    input: &[f32],
    expected: &[f32],
    s: &[f64; 3],
    o: &[f64; 3],
    p: &[f64; 3],
    sat: f64,
    style: CdlOpStyle,
    threshold: f32,
) {
    let data = CdlOpData::new(style, *s, *o, *p, sat).unwrap();
    let op = CdlOp::new(data).unwrap();
    let out = apply_op(&op, input);
    for i in 0..input.len() {
        assert!(
            equal_with_safe_rel_error(out[i], expected[i], threshold, 1.0),
            "index {i}: {} vs {} (threshold {threshold})",
            out[i],
            expected[i]
        );
    }
}

#[test]
fn op_apply_clamp_fwd() {
    let input = [
        QNAN, QNAN, QNAN, 0.0, 0.0, 0.0, 0.0, QNAN, INF, INF, INF, INF, -INF, -INF, -INF, -INF,
        0.3278, 0.01, 1.0, 0.0, 0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 0.75, -0.2, 0.5, 1.4, 0.0,
        -0.25, -0.5, -0.75, 0.25, 0.0, 0.8, 0.99, 0.5,
    ];
    let expected = [
        0.0, 0.0, 0.0, 0.0, 0.071827, 0.0, 0.070533, QNAN, 1.0, 1.0, 1.0, INF, 0.0, 0.0, 0.0, -INF,
        0.609399, 0.000000, 0.113130, 0.0, 0.422056, 0.401466, 0.035820, 1.0, 1.000000, 1.000000,
        0.000000, 0.75, 0.000000, 0.421096, 0.101225, 0.0, 0.000000, 0.000000, 0.031735, 0.25,
        0.000000, 0.746748, 0.018691, 0.5,
    ];
    apply_cdl(
        &input,
        &expected,
        &D1_SLOPE,
        &D1_OFFSET,
        &D1_POWER,
        D1_SAT,
        CdlOpStyle::AscFwd,
        2e-6,
    );
}

#[test]
fn op_apply_clamp_rev() {
    let input = [
        QNAN, QNAN, QNAN, 0.0, 0.0, 0.0, 0.0, QNAN, INF, INF, INF, INF, -INF, -INF, -INF, -INF,
        0.609399, 0.100000, 0.113130, 0.0, 0.001000, 0.746748, 0.018691, 0.5, 0.422056, 0.401466,
        0.035820, 1.0, -0.25, -0.5, -0.75, 0.25, 1.25, 1.5, 1.75, 0.75, -0.2, 0.5, 1.4, 0.0,
    ];
    let expected = [
        0.0, 0.209091, 0.0, 0.0, 0.0, 0.209091, 0.0, QNAN, 0.703713, 1.0, 1.0, INF, 0.0, 0.209091,
        0.0, -INF, 0.340710, 0.275726, 1.000000, 0.0, 0.025902, 0.801895, 1.000000, 0.5, 0.250000,
        0.500000, 0.750006, 1.0, 0.000000, 0.209091, 0.000000, 0.25, 0.703704, 1.000000, 1.000000,
        0.75, 0.012206, 0.582944, 1.000000, 0.0,
    ];
    apply_cdl(
        &input,
        &expected,
        &D1_SLOPE,
        &D1_OFFSET,
        &D1_POWER,
        D1_SAT,
        CdlOpStyle::AscRev,
        1e-5,
    );
}

#[test]
fn op_apply_noclamp_fwd() {
    let input = [
        QNAN, QNAN, QNAN, 0.0, 0.0, 0.0, 0.0, QNAN, INF, INF, INF, INF, -INF, -INF, -INF, -INF,
        0.3278, 0.01, 1.0, 0.0, 0.0, 0.8, 0.99, 0.5, 0.25, 0.5, 0.75, 1.0, -0.25, -0.5, -0.75,
        0.25, 1.25, 1.5, 1.75, 0.75, -0.2, 0.5, 1.4, 0.0,
    ];
    let expected = [
        0.0, 0.0, 0.0, 0.0, 0.109661, -0.249088, 0.108368, QNAN, QNAN, QNAN, QNAN, INF, QNAN, QNAN,
        QNAN, -INF, 0.645424, -0.260548, 0.149154, 0.0, -0.045094, 0.746748, 0.018691, 0.5,
        0.422056, 0.401466, 0.035820, 1.0, -0.211694, -0.817469, 0.174100, 0.25, 1.753162,
        1.331130, -0.108181, 0.75, -0.327485, 0.431854, 0.111983, 0.0,
    ];
    apply_cdl(
        &input,
        &expected,
        &D1_SLOPE,
        &D1_OFFSET,
        &D1_POWER,
        D1_SAT,
        CdlOpStyle::NoClampFwd,
        2e-6,
    );
}

#[test]
fn op_apply_noclamp_rev() {
    let input = [
        QNAN, QNAN, QNAN, 0.0, 0.0, 0.0, 0.0, QNAN, INF, INF, INF, INF, -INF, -INF, -INF, -INF,
        0.609399, 0.100000, 0.113130, 0.0, 0.001000, 0.746748, 0.018691, 0.5, 0.422056, 0.401466,
        0.035820, 1.0, -0.25, -0.5, -0.75, 0.25, 1.25, 1.5, 1.75, 0.75, -0.2, 0.5, 1.4, 0.0,
    ];
    let expected = [
        -0.037037, 0.209091, -1.549296, 0.0, -0.037037, 0.209091, -1.549296, QNAN, -0.037037,
        0.209091, -1.549296, INF, -0.037037, 0.209091, -1.549296, -INF, 0.340710, 0.275726,
        1.294827, 0.0, 0.025902, 0.801895, 1.022221, 0.5, 0.250000, 0.500000, 0.750006, 1.0,
        -0.251989, -0.239488, -11.361812, 0.25, 0.937160, 1.700692, 19.807237, 0.75, -0.099839,
        0.580528, 14.880301, 0.0,
    ];
    apply_cdl(
        &input,
        &expected,
        &D1_SLOPE,
        &D1_OFFSET,
        &D1_POWER,
        D1_SAT,
        CdlOpStyle::NoClampRev,
        1e-6,
    );
}

#[test]
fn op_apply_clamp_fwd_2() {
    let input = [
        QNAN, QNAN, QNAN, 0.0, 0.0, 0.0, 0.0, QNAN, INF, INF, INF, INF, -INF, -INF, -INF, -INF,
        0.65, 0.55, 0.20, 0.0, 0.41, 0.81, 0.39, 0.5, 0.25, 0.50, 0.75, 1.0,
    ];
    let expected = [
        0.0, 0.0, 0.0, 0.0, 0.027379, 0.024645, 0.046585, QNAN, 1.0, 1.0, 1.0, INF, 0.0, 0.0, 0.0,
        -INF, 0.745644, 0.639197, 0.264149, 0.0, 0.499594, 0.897554, 0.428591, 0.5, 0.305035,
        0.578779, 0.692558, 1.0,
    ];
    apply_cdl(
        &input,
        &expected,
        &[1.15, 1.10, 0.9],
        &[0.05, 0.02, 0.07],
        &[1.2, 0.95, 1.13],
        0.87,
        CdlOpStyle::AscFwd,
        1e-6,
    );
}

const D3_SLOPE: [f64; 3] = [3.405, 1.0, 1.0];
const D3_OFFSET: [f64; 3] = [-0.178, -0.178, -0.178];
const D3_POWER: [f64; 3] = [1.095, 1.095, 1.095];
const D3_SAT: f64 = 0.99;

const D3_INPUT: [f32; 80] = [
    QNAN, QNAN, QNAN, 0.0, 0.0, 0.0, 0.0, QNAN, INF, INF, INF, INF, -INF, -INF, -INF, -INF, //
    0.02, 0.0, 0.0, 0.0, 0.17, 0.0, 0.0, 0.0, 0.65, 0.0, 0.0, 0.0, 0.97, 0.0, 0.0, 0.0, //
    0.02, 0.13, 0.0, 0.0, 0.17, 0.13, 0.0, 0.0, 0.65, 0.13, 0.0, 0.0, 0.97, 0.13, 0.0, 0.0, //
    0.02, 0.23, 0.0, 0.0, 0.17, 0.23, 0.0, 0.0, 0.65, 0.23, 0.0, 0.0, 0.97, 0.23, 0.0, 0.0, //
    0.02, 0.13, 0.23, 0.0, 0.17, 0.13, 0.23, 0.0, 0.65, 0.13, 0.23, 0.0, 0.97, 0.13, 0.23, 0.0,
];

#[test]
fn op_apply_clamp_fwd_3() {
    let expected = [
        0.000000, 0.000000, 0.000000, 0.0, 0.000000, 0.000000, 0.000000, QNAN, 1.0, 1.0, 1.0, INF,
        0.0, 0.0, 0.0, -INF, //
        0.000000, 0.000000, 0.000000, 0.0, 0.364613, 0.000781, 0.000781, 0.0, 0.992126, 0.002126,
        0.002126, 0.0, 0.992126, 0.002126, 0.002126, 0.0, //
        0.000000, 0.000000, 0.000000, 0.0, 0.364613, 0.000781, 0.000781, 0.0, 0.992126, 0.002126,
        0.002126, 0.0, 0.992126, 0.002126, 0.002126, 0.0, //
        0.000281, 0.039155, 0.0002808, 0.0, 0.364894, 0.039936, 0.0010621, 0.0, 0.992407, 0.041281,
        0.0024068, 0.0, 0.992407, 0.041281, 0.0024068, 0.0, //
        0.000028, 0.000028, 0.0389023, 0.0, 0.364641, 0.000810, 0.0396836, 0.0, 0.992154, 0.002154,
        0.0410283, 0.0, 0.992154, 0.002154, 0.0410283, 0.0,
    ];
    apply_cdl(
        &D3_INPUT,
        &expected,
        &D3_SLOPE,
        &D3_OFFSET,
        &D3_POWER,
        D3_SAT,
        CdlOpStyle::AscFwd,
        1e-6,
    );
}

#[test]
fn op_apply_noclamp_fwd_3() {
    let expected = [
        0.0, 0.0, 0.0, 0.0, -0.178000, -0.178000, -0.178000, QNAN, QNAN, QNAN, QNAN, INF, QNAN,
        QNAN, QNAN, -INF, //
        -0.110436, -0.177855, -0.177855, 0.0, 0.363211, -0.176840, -0.176840, 0.0, 2.158845,
        -0.172992, -0.172992, 0.0, 3.453254, -0.170219, -0.170219, 0.0, //
        -0.109506, -0.048225, -0.176925, 0.0, 0.364141, -0.047210, -0.175910, 0.0, 2.159774,
        -0.043363, -0.172063, 0.0, 3.454184, -0.040589, -0.169289, 0.0, //
        -0.108882, 0.038793, -0.176301, 0.0, 0.364765, 0.039808, -0.175286, 0.0, 2.160399,
        0.043655, -0.171438, 0.0, 3.454808, 0.046429, -0.168665, 0.0, //
        -0.109350, -0.048069, 0.038325, 0.0, 0.364298, -0.047054, 0.039340, 0.0, 2.159931,
        -0.043206, 0.043188, 0.0, 3.454341, -0.040432, 0.045962, 0.0,
    ];
    apply_cdl(
        &D3_INPUT,
        &expected,
        &D3_SLOPE,
        &D3_OFFSET,
        &D3_POWER,
        D3_SAT,
        CdlOpStyle::NoClampFwd,
        1e-6,
    );
}

#[test]
fn op_create_transform() {
    let config = Config::create_raw();
    let ctx = Context::new();
    let check_values = |t: &CdlTransform| {
        assert_eq!(t.slope, D1_SLOPE);
        assert_eq!(t.offset, D1_OFFSET);
        assert_eq!(t.power, D1_POWER);
        assert_eq!(t.sat, D1_SAT);
    };
    let back = |t: &CdlTransform| -> (CdlOpStyle, CdlOpStyle) {
        let mut ops = OpVec::new();
        t.build_ops(&mut ops, &config, &ctx, TransformDirection::Forward)
            .unwrap();
        assert_eq!(ops.len(), 1);
        t.build_ops(&mut ops, &config, &ctx, TransformDirection::Inverse)
            .unwrap();
        assert_eq!(ops.len(), 2);
        (cd(&ops, 0).style, cd(&ops, 1).style)
    };
    let to_cdl = |op: &CdlOp| match op.to_transform().unwrap() {
        Transform::Cdl(t) => t,
        _ => panic!("expected a CDL transform"),
    };

    // Forward direction.
    let mut d = CdlOpData::new(CdlOpStyle::AscFwd, D1_SLOPE, D1_OFFSET, D1_POWER, D1_SAT).unwrap();
    d.metadata.add_attribute(METADATA_ID, "Test look: 01-A.");
    let t = to_cdl(&CdlOp::new(d).unwrap());
    assert_eq!(
        t.metadata.attributes,
        vec![(METADATA_ID.to_string(), "Test look: 01-A.".to_string())]
    );
    assert_eq!(t.direction, TransformDirection::Forward);
    assert_eq!(t.style, CdlStyle::Asc);
    check_values(&t);
    assert_eq!(back(&t), (CdlOpStyle::AscFwd, CdlOpStyle::AscRev));

    // Inverse direction.
    let d = CdlOpData::new(CdlOpStyle::AscRev, D1_SLOPE, D1_OFFSET, D1_POWER, D1_SAT).unwrap();
    let t = to_cdl(&CdlOp::new(d).unwrap());
    assert_eq!(t.direction, TransformDirection::Inverse);
    assert_eq!(t.style, CdlStyle::Asc);
    check_values(&t);
    assert_eq!(back(&t), (CdlOpStyle::AscRev, CdlOpStyle::AscFwd));

    // No clamp.
    let d = CdlOpData::new(
        CdlOpStyle::NoClampFwd,
        D1_SLOPE,
        D1_OFFSET,
        D1_POWER,
        D1_SAT,
    )
    .unwrap();
    let mut t = to_cdl(&CdlOp::new(d).unwrap());
    assert_eq!(t.style, CdlStyle::NoClamp);
    assert_eq!(t.direction, TransformDirection::Forward);
    t.direction = TransformDirection::Inverse;
    check_values(&t);
    assert_eq!(back(&t), (CdlOpStyle::NoClampRev, CdlOpStyle::NoClampFwd));
}

// CDLTransform_tests.cpp

#[test]
fn transform_equality() {
    let cdl1 = CdlTransform::default();
    let mut cdl2 = CdlTransform::default();
    assert!(cdl1.equals(&cdl1));
    assert!(cdl1.equals(&cdl2));
    assert!(cdl2.equals(&cdl1));
    let mut cdl3 = CdlTransform::default();
    cdl3.sat += 0.002f32 as f64;
    assert!(!cdl1.equals(&cdl3));
    assert!(!cdl2.equals(&cdl3));
    assert!(cdl3.equals(&cdl3));
    cdl2.style = CdlStyle::Asc;
    assert!(!cdl1.equals(&cdl2));
    // Tolerance of 1e-9 on the SOP values.
    let mut cdl4 = CdlTransform::default();
    cdl4.slope[0] += 1e-10;
    assert!(cdl1.equals(&cdl4));
}

#[test]
fn transform_buildops() {
    let mut t = CdlTransform::default();
    // For a v1 config, a CDL uses an exponent and two matrix ops rather than the CDL op
    // that was introduced in v2.
    let build = |t: &CdlTransform| {
        let mut ops = OpVec::new();
        build_cdl_ops(&mut ops, t, TransformDirection::Forward, 1).unwrap();
        assert_eq!(ops.len(), 3);
        optimize_ops(&ops, OptimizationFlags::DEFAULT)
    };
    assert!(build(&t).is_empty());

    t.power = [1.1, 1.0, 1.0];
    let ops = build(&t);
    assert_eq!(ops.len(), 1);
    assert!(ops[0].downcast_ref::<ExponentOp>().is_some());

    t.sat = 1.5;
    let ops = build(&t);
    assert_eq!(ops.len(), 2);
    assert!(ops[0].downcast_ref::<ExponentOp>().is_some());
    assert!(ops[1].downcast_ref::<MatrixOp>().is_some());

    t.offset = [0.0, 0.1, 0.0];
    let ops = build(&t);
    assert_eq!(ops.len(), 3);
    assert!(ops[0].downcast_ref::<MatrixOp>().is_some());
    assert!(ops[1].downcast_ref::<ExponentOp>().is_some());
    assert!(ops[2].downcast_ref::<MatrixOp>().is_some());

    // v1 inverse.
    let mut ops = OpVec::new();
    build_cdl_ops(&mut ops, &t, TransformDirection::Inverse, 1).unwrap();
    assert_eq!(ops.len(), 3);
    let mut fwd = OpVec::new();
    build_cdl_ops(&mut fwd, &t, TransformDirection::Forward, 1).unwrap();
    let px = apply_ops(&fwd, &[0.2, 0.3, 0.4, 1.0]);
    let px = apply_ops(&ops, &px);
    for (i, v) in [0.2, 0.3, 0.4, 1.0].iter().enumerate() {
        assert_close_f(px[i], *v, 1e-6);
    }

    // Testing v2 onward behavior.
    let mut ops = OpVec::new();
    build_cdl_ops(&mut ops, &t, TransformDirection::Forward, 2).unwrap();
    assert_eq!(ops.len(), 1);
    assert!(ops[0].downcast_ref::<CdlOp>().is_some());
}

#[test]
fn transform_description() {
    let mut t = CdlTransform::default();
    t.set_id("TestCDL");
    assert!(t.first_sop_description().is_empty());
    t.metadata.add_child_element(METADATA_DESCRIPTION, "Desc");
    t.metadata
        .add_child_element(METADATA_INPUT_DESCRIPTION, "Input Desc");
    t.metadata
        .add_child_element(METADATA_SOP_DESCRIPTION, "SOP Desc");
    t.metadata
        .add_child_element(METADATA_SAT_DESCRIPTION, "Sat Desc");
    t.metadata
        .add_child_element(METADATA_SOP_DESCRIPTION, "Additional SOP");
    assert_eq!(t.metadata.children.len(), 5);
    assert_eq!(t.first_sop_description(), "SOP Desc");
    t.set_first_sop_description("SOP Desc New");
    assert_eq!(t.first_sop_description(), "SOP Desc New");
    assert_eq!(t.metadata.children.len(), 5);
    t.clear_first_sop_description();
    assert_eq!(t.metadata.children.len(), 4);
    assert_eq!(t.first_sop_description(), "Additional SOP");
    t.clear_first_sop_description();
    assert_eq!(t.metadata.children.len(), 3);
    assert_eq!(t.first_sop_description(), "");
}

#[test]
fn transform_style() {
    let config = Config::create_raw();
    let ctx = Context::new();
    let mut t = CdlTransform::default();
    assert_eq!(t.style, CdlStyle::NoClamp);
    let style_of = |t: &CdlTransform, dir| {
        let mut ops = OpVec::new();
        t.build_ops(&mut ops, &config, &ctx, dir).unwrap();
        assert_eq!(ops.len(), 1);
        cd(&ops, 0).style
    };
    assert_eq!(
        style_of(&t, TransformDirection::Forward),
        CdlOpStyle::NoClampFwd
    );
    assert_eq!(
        style_of(&t, TransformDirection::Inverse),
        CdlOpStyle::NoClampRev
    );
    t.style = CdlStyle::Asc;
    assert_eq!(
        style_of(&t, TransformDirection::Forward),
        CdlOpStyle::AscFwd
    );
    assert_eq!(
        style_of(&t, TransformDirection::Inverse),
        CdlOpStyle::AscRev
    );

    t.power = [0.0; 3];
    assert_eq!(
        t.validate().unwrap_err().message(),
        "CDLTransform validation failed: CDLOpData: Invalid 'power' 0 should be greater than 0."
    );
}

fn simplify_check(t: &CdlTransform) {
    let config = Config::create_raw();
    let ctx = Context::new();
    let proc = Processor::from_transform(
        &config,
        &ctx,
        &Transform::Cdl(t.clone()),
        TransformDirection::Forward,
    )
    .unwrap();
    let no_simplify = OptimizationFlags::DEFAULT & !OptimizationFlags::SIMPLIFY_OPS;
    let cpu = proc.optimized_cpu_processor(no_simplify);
    assert_eq!(cpu.ops().len(), 1);
    assert!(cpu.ops()[0].downcast_ref::<CdlOp>().is_some());
    let source = [-0.1f32, 0.5, 1.5];
    let mut pix_no_simplify = source;
    cpu.apply_rgb(&mut pix_no_simplify);

    let cpu = proc.optimized_cpu_processor(OptimizationFlags::DEFAULT);
    assert!(cpu
        .ops()
        .iter()
        .all(|o| o.downcast_ref::<CdlOp>().is_none()));
    let mut pix_simplify = source;
    cpu.apply_rgb(&mut pix_simplify);
    for i in 0..3 {
        assert_close_f(pix_no_simplify[i], pix_simplify[i] as f64, 2e-5);
    }
}

#[test]
fn transform_apply_optimize_simplify() {
    let mut t = CdlTransform {
        slope: [0.8, 0.9, 1.1],
        offset: [0.1, 0.05, -0.2],
        sat: 1.23,
        ..Default::default()
    };
    simplify_check(&t);
    t.direction = TransformDirection::Inverse;
    simplify_check(&t);
    // Clamping style (forward only: as in OCIO, the simplified inverse ASC
    // CDL does not clamp after the inverse slope / offset).
    t.style = CdlStyle::Asc;
    t.direction = TransformDirection::Forward;
    simplify_check(&t);
}

#[test]
fn simpler_replacement_ops() {
    let d = cdl(
        CdlOpStyle::AscFwd,
        [0.8, 0.9, 1.1],
        [0.1, 0.05, -0.2],
        [1.0; 3],
        1.23,
    );
    let ops = d.simpler_replacement().unwrap();
    let names: Vec<&str> = ops.iter().map(|o| o.name()).collect();
    assert_eq!(names, vec!["Matrix", "Range", "Matrix", "Range"]);
    let ops = d.inverse().simpler_replacement().unwrap();
    let names: Vec<&str> = ops.iter().map(|o| o.name()).collect();
    assert_eq!(names, vec!["Range", "Matrix", "Range", "Matrix"]);
    let d = cdl(
        CdlOpStyle::NoClampFwd,
        [0.8, 0.9, 1.1],
        [0.1, 0.05, -0.2],
        [1.0; 3],
        1.0,
    );
    let ops = d.simpler_replacement().unwrap();
    assert_eq!(ops.len(), 1);
    // Not replaced when the power is used, nor for identities.
    let d = cdl(
        CdlOpStyle::NoClampFwd,
        [0.8, 0.9, 1.1],
        [0.1, 0.05, -0.2],
        [1.1; 3],
        1.0,
    );
    assert!(d.simpler_replacement().is_none());
    assert!(CdlOpData::default().simpler_replacement().is_none());

    // An identity ASC CDL is replaced by a clamp.
    let mut ops = OpVec::new();
    create_cdl_op(
        &mut ops,
        CdlOpStyle::AscFwd,
        &[1.0; 3],
        &[0.0; 3],
        &[1.0; 3],
        1.0,
        TransformDirection::Forward,
    )
    .unwrap();
    assert!(!ops[0].is_no_op());
    let opt = optimize_ops(&ops, OptimizationFlags::DEFAULT);
    assert_eq!(opt.len(), 1);
    assert_eq!(opt[0].name(), "Range");
    // A non-clamping identity is removed.
    let mut ops = OpVec::new();
    create_cdl_op(
        &mut ops,
        CdlOpStyle::NoClampRev,
        &[1.0; 3],
        &[0.0; 3],
        &[1.0; 3],
        1.0,
        TransformDirection::Forward,
    )
    .unwrap();
    assert!(ops[0].is_no_op());
    assert!(optimize_ops(&ops, OptimizationFlags::DEFAULT).is_empty());
}
