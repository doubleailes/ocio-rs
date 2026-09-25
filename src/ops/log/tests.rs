//! Unit tests of the log op (ports of `LogOpData_tests.cpp`, `LogOp_tests.cpp`,
//! `LogOpCPU_tests.cpp` and the log transform tests).

use super::log_utils::*;
use super::*;
use crate::ops::matrix::test_utils::*;
use crate::processor::optimize_ops;

const QNAN: f32 = f32::NAN;
const INF: f32 = f32::INFINITY;

const RGBA_IMAGE: [f32; 32] = [
    0.0367126, 0.5, 1., 0., 0.2, 0., 0.99, 128., QNAN, QNAN, QNAN, 0., 0., 0., 0., QNAN, INF, INF,
    INF, 0., 0., 0., 0., INF, -INF, -INF, -INF, 0., 0., 0., 0., -INF,
];

fn render(log: &LogOpData, image: &[f32]) -> Vec<f32> {
    let r = LogRenderer::new(log);
    let mut px = to_pixels(image);
    r.apply(&mut px);
    flatten(&px)
}

fn ctf_ref_params() -> CtfParams {
    let mut p = CtfParams::default();
    p.params[CTF_RED] = vec![0.5, 685., 93., 0.8, 0.0004];
    p.params[CTF_GREEN] = vec![0.6, 684., 94., 0.9, 0.0005];
    p.params[CTF_BLUE] = vec![0.65, 683., 95., 1.0, 0.0003];
    p
}

// LogOpData_tests.cpp

#[test]
fn data_accessor() {
    let mut ctf = CtfParams {
        style: LogStyle::LogToLin,
        ..Default::default()
    };
    ctf.params[CTF_RED] = vec![2.4, 410., 256., 0.2, 0.1];
    ctf.params[CTF_GREEN] = vec![3.5, 620., 485., 0.7, 0.6];
    ctf.params[CTF_BLUE] = vec![4.6, 730., 558., 0.9, 0.7];

    let mut base = 1.0;
    let dir = get_log_direction(ctf.style);
    let p = convert_log_parameters(&ctf, &mut base).unwrap();
    let log = LogOpData::new(base, p[0].clone(), p[1].clone(), p[2].clone(), dir).unwrap();
    assert!(!log.all_components_equal());
    assert_eq!(log.base, base);
    assert_eq!(log.red, p[0]);
    assert_eq!(log.green, p[1]);
    assert_eq!(log.blue, p[2]);

    ctf.params[CTF_GREEN] = ctf.params[CTF_RED].clone();
    ctf.params[CTF_BLUE] = ctf.params[CTF_RED].clone();
    let p = convert_log_parameters(&ctf, &mut base).unwrap();
    let log2 = LogOpData::new(base, p[0].clone(), p[1].clone(), p[2].clone(), dir).unwrap();
    assert!(log2.all_components_equal());
    assert_eq!(log2.green, p[0]);

    ctf.params[CTF_RED] = vec![0.6, 358., 115., 0.7, 0.3];
    let p = convert_log_parameters(&ctf, &mut base).unwrap();
    let log3 = LogOpData::new(base, p[0].clone(), p[1].clone(), p[2].clone(), dir).unwrap();
    assert!(!log3.all_components_equal());

    let log6 = LogOpData::from_base(2.0, TransformDirection::Forward);
    assert!(log6.all_components_equal());
    assert_eq!(log6.base, 2.0);
    assert_eq!(log6.red[LOG_SIDE_SLOPE], 1.0);
    assert_eq!(log6.red[LIN_SIDE_SLOPE], 1.0);
    assert_eq!(log6.red[LIN_SIDE_OFFSET], 0.0);
    assert_eq!(log6.red[LOG_SIDE_OFFSET], 0.0);

    let log_slope = [1.5, 1.6, 1.7];
    let lin_slope = [1.1, 1.2, 1.3];
    let lin_offset = [1.0, 2.0, 3.0];
    let log_offset = [10.0, 20.0, 30.0];
    let log7 = LogOpData::from_affine(
        base,
        &log_slope,
        &log_offset,
        &lin_slope,
        &lin_offset,
        TransformDirection::Forward,
    );
    assert!(!log7.all_components_equal());
    for c in 0..3 {
        let p = log7.params(c);
        assert_eq!(p[LOG_SIDE_SLOPE], log_slope[c]);
        assert_eq!(p[LIN_SIDE_SLOPE], lin_slope[c]);
        assert_eq!(p[LIN_SIDE_OFFSET], lin_offset[c]);
        assert_eq!(p[LOG_SIDE_OFFSET], log_offset[c]);
    }

    // Channels must have the same style.
    let e = LogOpData::new(
        2.0,
        vec![1., 0., 1., 0.],
        vec![1.],
        vec![1.],
        TransformDirection::Forward,
    )
    .unwrap_err();
    assert_eq!(
        e.message(),
        "Cannot create Log op, all channels need to have the same style."
    );
}

#[test]
fn data_validation_fails() {
    let mut log_slope = [1.0; 3];
    let mut lin_slope = [1.0; 3];
    let lin_offset = [0.0; 3];
    let log_offset = [0.0; 3];
    for dir in [TransformDirection::Forward, TransformDirection::Inverse] {
        let l = LogOpData::from_affine(1.0, &log_slope, &log_offset, &lin_slope, &lin_offset, dir);
        assert!(l
            .validate()
            .unwrap_err()
            .message()
            .contains("base cannot be 1"));
    }
    lin_slope = [0.0; 3];
    for dir in [TransformDirection::Forward, TransformDirection::Inverse] {
        let l = LogOpData::from_affine(10.0, &log_slope, &log_offset, &lin_slope, &lin_offset, dir);
        assert!(l
            .validate()
            .unwrap_err()
            .message()
            .contains("linear side slope cannot be 0"));
    }
    lin_slope = [1.0; 3];
    log_slope = [0.0; 3];
    for dir in [TransformDirection::Forward, TransformDirection::Inverse] {
        let l = LogOpData::from_affine(10.0, &log_slope, &log_offset, &lin_slope, &lin_offset, dir);
        assert!(l
            .validate()
            .unwrap_err()
            .message()
            .contains("log side slope cannot be 0"));
    }
    let l = LogOpData::from_base(-1.0, TransformDirection::Forward);
    assert_eq!(
        l.validate().unwrap_err().message(),
        "Log: Invalid base value '-1', base must be greater than 0."
    );
}

#[test]
fn log_data_validate_nan() {
    // Deviation from OCIO: NaN base / parameters pass OCIO's
    // LogOpData::validate (the C++ build renders NaN), the port rejects them.
    let msg = |l: LogOpData| l.validate().unwrap_err().message().to_string();
    let l = LogOpData::from_base(f64::NAN, TransformDirection::Forward);
    assert_eq!(
        msg(l),
        "Log: Invalid base value 'nan', base must be greater than 0."
    );
    let names = [
        "log side slope",
        "log side offset",
        "linear side slope",
        "linear side offset",
    ];
    for (i, name) in names.iter().enumerate() {
        for dir in [TransformDirection::Forward, TransformDirection::Inverse] {
            // log slope, log offset, lin slope, lin offset (per channel).
            let mut p = [[1.0; 3], [0.0; 3], [1.0; 3], [0.0; 3]];
            p[i][1] = f64::NAN;
            let l = LogOpData::from_affine(2.0, &p[0], &p[1], &p[2], &p[3], dir);
            assert_eq!(
                msg(l),
                format!("Log: Invalid {name} value 'nan', {name} cannot be NaN.")
            );
        }
    }

    let mut t = LogCameraTransform::new([0.1; 3]);
    t.lin_side_break[2] = f64::NAN;
    assert_eq!(
        t.validate().unwrap_err().message(),
        "LogCameraTransform validation failed: Log: Invalid linear side break value 'nan', linear side break cannot be NaN."
    );
    t.lin_side_break = [0.1; 3];
    t.linear_slope = Some([1.0, f64::NAN, 1.0]);
    assert_eq!(
        t.validate().unwrap_err().message(),
        "LogCameraTransform validation failed: Log: Invalid linear slope value 'nan', linear slope cannot be NaN."
    );
    let mut t = LogTransform::default();
    t.base = f64::NAN;
    assert_eq!(
        t.validate().unwrap_err().message(),
        "LogTransform validation failed: Log: Invalid base value 'nan', base must be greater than 0."
    );
}

#[test]
fn data_log_inverse() {
    let pr = vec![1.5, 10.0, 1.1, 1.0];
    let pg = vec![1.6, 20.0, 1.2, 2.0];
    let pb = vec![1.7, 30.0, 1.3, 3.0];
    let log0 = LogOpData::new(10.0, pr.clone(), pg, pb, TransformDirection::Forward).unwrap();
    let inv0 = log0.inverse().unwrap();
    assert_eq!(log0.red, inv0.red);
    assert_eq!(log0.green, inv0.green);
    assert_eq!(log0.blue, inv0.blue);
    // When components are not equal, ops are not considered inverse.
    assert!(!log0.is_inverse(&inv0));

    let log1 = LogOpData::new(
        10.0,
        pr.clone(),
        pr.clone(),
        pr,
        TransformDirection::Forward,
    )
    .unwrap();
    let inv1 = log1.inverse().unwrap();
    assert!(log1.is_inverse(&inv1));
}

#[test]
fn data_identity_replacement() {
    let p = vec![1.5, 10.0, 2.0, 1.0];
    let l = LogOpData::new(
        2.0,
        p.clone(),
        p.clone(),
        p.clone(),
        TransformDirection::Inverse,
    )
    .unwrap();
    // Matrix identity: the pair is removed.
    assert!(l.identity_replacement().is_none());
    let l = LogOpData::new(2.0, p.clone(), p.clone(), p, TransformDirection::Forward).unwrap();
    let r = l.identity_replacement().unwrap();
    assert_eq!(r.min_in, -0.5);
    assert!(r.max_is_empty());
    assert!(LogOpData::from_base(2.0, TransformDirection::Forward)
        .identity_replacement()
        .is_some());
    assert!(LogOpData::from_base(2.0, TransformDirection::Inverse)
        .identity_replacement()
        .is_none());
}

// LogOp_tests.cpp

const BASE: f64 = 10.0;
const LOG_SLOPE: [f64; 3] = [0.18, 0.18, 0.18];
const LIN_SLOPE: [f64; 3] = [2.0, 2.0, 2.0];
const LIN_OFFSET: [f64; 3] = [0.1, 0.1, 0.1];
const LOG_OFFSET: [f64; 3] = [1.0, 1.0, 1.0];

const LIN: [f32; 8] = [0.01, 0.1, 1.0, 1.0, 10.0, 100.0, 1000.0, 1.0];
const LOG: [f32; 8] = [
    0.8342526242885725,
    0.90588182584953925,
    1.057999473052105462,
    1.0,
    1.23457529033568797,
    1.41422447595451795,
    1.59418930777214063,
    1.0,
];

#[test]
fn op_lin_to_log() {
    let mut ops = OpVec::new();
    create_log_op(
        &mut ops,
        BASE,
        &LOG_SLOPE,
        &LOG_OFFSET,
        &LIN_SLOPE,
        &LIN_OFFSET,
        TransformDirection::Forward,
    )
    .unwrap();
    assert_eq!(ops.len(), 1);
    let ops = optimize_ops(&ops, OptimizationFlags::DEFAULT);
    assert!(!ops[0].cache_id().is_empty());
    assert!(!ops[0].is_no_op());
    assert!(!ops[0].has_channel_crosstalk());
    let out = apply_ops(&ops, &LIN);
    for i in 0..8 {
        assert_close(out[i] as f64, LOG[i] as f64, 1e-3);
    }
}

#[test]
fn op_log_to_lin() {
    let mut ops = OpVec::new();
    create_log_op(
        &mut ops,
        BASE,
        &LOG_SLOPE,
        &LOG_OFFSET,
        &LIN_SLOPE,
        &LIN_OFFSET,
        TransformDirection::Inverse,
    )
    .unwrap();
    let ops = optimize_ops(&ops, OptimizationFlags::DEFAULT);
    let out = apply_ops(&ops, &LOG);
    for i in 0..8 {
        assert_close(out[i] as f64, LIN[i] as f64, 2e-3);
    }
}

fn log_op(ops: &OpVec, i: usize) -> &LogOp {
    ops[i].downcast_ref::<LogOp>().unwrap()
}

#[test]
fn op_inverse() {
    let log_slope = [0.5, 0.5, 0.5];
    let log_slope2 = [0.5, 1.0, 1.5];
    let mut ops = OpVec::new();
    let f = TransformDirection::Forward;
    let i = TransformDirection::Inverse;
    create_log_op(
        &mut ops,
        BASE,
        &log_slope,
        &LOG_OFFSET,
        &LIN_SLOPE,
        &LIN_OFFSET,
        f,
    )
    .unwrap();
    create_log_op(
        &mut ops,
        BASE,
        &log_slope,
        &LOG_OFFSET,
        &LIN_SLOPE,
        &LIN_OFFSET,
        i,
    )
    .unwrap();
    create_log_op(
        &mut ops,
        BASE + 1.0,
        &log_slope,
        &LOG_OFFSET,
        &LIN_SLOPE,
        &LIN_OFFSET,
        i,
    )
    .unwrap();
    create_log_op(
        &mut ops,
        BASE + 1.0,
        &log_slope,
        &LOG_OFFSET,
        &LIN_SLOPE,
        &LIN_OFFSET,
        f,
    )
    .unwrap();
    create_log_op(
        &mut ops,
        BASE + 1.0,
        &log_slope2,
        &LOG_OFFSET,
        &LIN_SLOPE,
        &LIN_OFFSET,
        i,
    )
    .unwrap();
    create_log_op(
        &mut ops,
        BASE + 1.0,
        &log_slope2,
        &LOG_OFFSET,
        &LIN_SLOPE,
        &LIN_OFFSET,
        f,
    )
    .unwrap();
    assert_eq!(ops.len(), 6);

    let d = |k: usize| log_op(&ops, k).data();
    assert!(!d(0).is_inverse(d(0)));
    assert!(d(0).is_inverse(d(1)));
    assert!(!d(0).is_inverse(d(2)));
    assert!(!d(0).is_inverse(d(3)));
    assert!(d(1).is_inverse(d(0)));
    assert!(!d(1).is_inverse(d(2)));
    assert!(!d(1).is_inverse(d(3)));
    assert!(!d(2).is_inverse(d(2)));
    assert!(d(2).is_inverse(d(3)));
    assert!(!d(3).is_inverse(d(3)));
    // When r, g & b are not equal, ops are not considered inverse even though they are.
    assert!(!d(4).is_inverse(d(5)));

    let result = [
        0.01f32, 0.1, 1.0, 1.0, 1.0, 10.0, 100.0, 1.0, 1000.0, 1.0, 0.5, 1.0,
    ];
    let data = apply_op(ops[0].as_ref(), &result);
    for k in [0, 1, 2, 4, 5, 6, 8, 9, 10] {
        assert_ne!(data[k], result[k]);
    }
    let data = apply_op(ops[1].as_ref(), &data);
    for k in 0..12 {
        assert_close(data[k] as f64, result[k] as f64, 1e-3);
    }

    // The optimizer removes the pairs of inverse logs.
    let flags = OptimizationFlags::DEFAULT;
    let pair = vec![ops[0].clone(), ops[1].clone()];
    let opt = optimize_ops(&pair, flags);
    // Lin-to-log then log-to-lin: replaced by a range clamping at -linOffset/linSlope.
    assert_eq!(opt.len(), 1);
    let r = opt[0].downcast_ref::<RangeOp>().unwrap().data();
    assert_eq!(r.min_in, -0.05);
    assert!(r.max_is_empty());
    let pair = vec![ops[2].clone(), ops[3].clone()];
    // Log-to-lin then lin-to-log: removed.
    assert!(optimize_ops(&pair, flags).is_empty());
    // Not removed without the flag.
    let pair = vec![ops[2].clone(), ops[3].clone()];
    assert_eq!(
        optimize_ops(&pair, flags & !OptimizationFlags::PAIR_IDENTITY_LOG).len(),
        2
    );
    // Not removed when the channels differ.
    let pair = vec![ops[4].clone(), ops[5].clone()];
    assert_eq!(optimize_ops(&pair, flags).len(), 2);
}

#[test]
fn op_cache_id() {
    let mut log_offset = LOG_OFFSET;
    let mut ops = OpVec::new();
    let f = TransformDirection::Forward;
    create_log_op(
        &mut ops,
        BASE,
        &LOG_SLOPE,
        &log_offset,
        &LIN_SLOPE,
        &LIN_OFFSET,
        f,
    )
    .unwrap();
    log_offset[0] += 1.0;
    create_log_op(
        &mut ops,
        BASE,
        &LOG_SLOPE,
        &log_offset,
        &LIN_SLOPE,
        &LIN_OFFSET,
        f,
    )
    .unwrap();
    log_offset[0] -= 1.0;
    create_log_op(
        &mut ops,
        BASE,
        &LOG_SLOPE,
        &log_offset,
        &LIN_SLOPE,
        &LIN_OFFSET,
        f,
    )
    .unwrap();
    assert_eq!(ops.len(), 3);
    let ops = optimize_ops(&ops, OptimizationFlags::DEFAULT);
    assert_eq!(ops[0].cache_id(), ops[2].cache_id());
    assert_ne!(ops[0].cache_id(), ops[1].cache_id());
    assert_eq!(
        ops[0].cache_id(),
        "<LogOp forward Base 10 LogSideSlope 0.18 LogSideOffset 1 LinSideSlope 2 LinSideOffset 0.1>"
    );
    assert_eq!(
        ops[1].cache_id(),
        "<LogOp forward Base 10 LogSideSlope 0.18, 0.18, 0.18 LogSideOffset 2, 1, 1 LinSideSlope 2, 2, 2 LinSideOffset 0.1, 0.1, 0.1>"
    );
}

#[test]
fn op_create_transform() {
    let dir = TransformDirection::Forward;
    let log_slope = [1.5, 1.6, 1.7];
    let lin_slope = [1.1, 1.2, 1.3];
    let lin_offset = [1.0, 2.0, 3.0];
    let log_offset = [10.0, 20.0, 30.0];
    // Note: OCIO uses a base of 1 (never validated in that test).
    let base = 3.0;
    let mut log =
        LogOpData::from_affine(base, &log_slope, &log_offset, &lin_slope, &lin_offset, dir);
    log.metadata.add_attribute("name", "test");

    let mut ops = OpVec::new();
    create_log_op_from_data(&mut ops, &log, dir).unwrap();
    let t = match ops[0].to_transform().unwrap() {
        Transform::LogAffine(t) => t,
        _ => panic!("expected a log affine transform"),
    };
    assert_eq!(
        t.metadata.attributes,
        vec![("name".to_string(), "test".to_string())]
    );
    assert_eq!(t.direction, dir);
    assert_eq!(t.base, base);
    assert_eq!(t.log_side_slope, log_slope);
    assert_eq!(t.log_side_offset, log_offset);
    assert_eq!(t.lin_side_slope, lin_slope);
    assert_eq!(t.lin_side_offset, lin_offset);

    let lin_break = [0.5, 0.4, 0.3];
    log.set_value(LIN_SIDE_BREAK, &lin_break);
    create_log_op_from_data(&mut ops, &log, dir).unwrap();
    let t = match ops[1].to_transform().unwrap() {
        Transform::LogCamera(t) => t,
        _ => panic!("expected a log camera transform"),
    };
    assert_eq!(t.lin_side_break, lin_break);
    assert!(t.linear_slope.is_none());

    let linear_slope = [0.9, 1.0, 1.1];
    log.set_value(LINEAR_SLOPE, &linear_slope);
    create_log_op_from_data(&mut ops, &log, dir).unwrap();
    let t = match ops[2].to_transform().unwrap() {
        Transform::LogCamera(t) => t,
        _ => panic!("expected a log camera transform"),
    };
    assert_eq!(t.lin_side_break, lin_break);
    assert_eq!(t.linear_slope, Some(linear_slope));

    // Simple logs give a LogTransform, in the op direction.
    let mut ops = OpVec::new();
    create_log_op_base(&mut ops, 2.0, TransformDirection::Inverse).unwrap();
    match ops[0].to_transform().unwrap() {
        Transform::Log(t) => {
            assert_eq!(t.base, 2.0);
            assert_eq!(t.direction, TransformDirection::Inverse);
        }
        _ => panic!("expected a log transform"),
    }
}

// LogOpCPU_tests.cpp

fn test_log(log_base: f32) {
    let log = LogOpData::from_base(log_base as f64, TransformDirection::Forward);
    let rgba = render(&log, &RGBA_IMAGE);
    let min_value = f32::MIN_POSITIVE;
    let error = 1e-5;
    for i in 0..8 {
        let mut expected = RGBA_IMAGE[i];
        if i % 4 != 3 {
            expected = min_value.max(expected).ln() / log_base.ln();
        }
        assert_close(rgba[i] as f64, expected as f64, error);
    }
    let res_min = (min_value.ln() / log_base.ln()) as f64;
    assert_close_f(rgba[8], res_min, error);
    assert_eq!(rgba[11], 0.0);
    assert_close_f(rgba[12], res_min, error);
    assert!(rgba[15].is_nan());
    assert_eq!(rgba[16], INF);
    assert_eq!(rgba[19], 0.0);
    assert_close_f(rgba[20], res_min, error);
    assert_eq!(rgba[23], INF);
    assert_close_f(rgba[24], res_min, error);
    assert_eq!(rgba[27], 0.0);
    assert_close_f(rgba[28], res_min, error);
    assert_eq!(rgba[31], -INF);
}

#[test]
fn cpu_log() {
    test_log(10.0);
    test_log(2.0);
}

fn test_anti_log(log_base: f32) {
    let log = LogOpData::from_base(log_base as f64, TransformDirection::Inverse);
    let rgba = render(&log, &RGBA_IMAGE);
    let rtol = 2f32.powf(-14.0);
    for i in 0..8 {
        let mut expected = RGBA_IMAGE[i];
        if i % 4 != 3 {
            expected = log_base.powf(expected);
        }
        assert!(
            equal_with_safe_rel_error(rgba[i], expected, rtol, 1.0),
            "{} vs {}",
            rgba[i],
            expected
        );
    }
    assert!(rgba[8].is_nan());
    assert_eq!(rgba[11], 0.0);
    assert_close_f(rgba[12], 1.0, rtol as f64);
    assert!(rgba[15].is_nan());
    assert_eq!(rgba[16], INF);
    assert_eq!(rgba[19], 0.0);
    assert_close_f(rgba[20], 1.0, rtol as f64);
    assert_eq!(rgba[23], INF);
    assert_eq!(rgba[24], 0.0);
    assert_eq!(rgba[27], 0.0);
    assert_close_f(rgba[28], 1.0, rtol as f64);
    assert_eq!(rgba[31], -INF);
}

#[test]
fn cpu_anti_log() {
    test_anti_log(10.0);
    test_anti_log(2.0);
}

fn compute_log2lin_eval(input: f32, params: &[f64]) -> f32 {
    let range = 0.002f32 * 1023.0;
    let gamma = params[0] as f32;
    let ref_white = params[1] as f32 / 1023.0;
    let ref_black = params[2] as f32 / 1023.0;
    let highlight = params[3] as f32;
    let shadow = params[4] as f32;
    let mult_factor = range / gamma;
    let tmp_value = ((ref_black - ref_white) * mult_factor).min(-0.0001);
    let gain = (highlight - shadow) / (1.0 - 10.0f32.powf(tmp_value));
    let offset = gain - (highlight - shadow);
    10.0f32.powf((input - ref_white) * mult_factor) * gain - offset + shadow
}

#[test]
fn cpu_log2lin() {
    let mut ctf = ctf_ref_params();
    ctf.style = LogStyle::LogToLin;
    let mut base = 1.0;
    let dir = get_log_direction(ctf.style);
    let p = convert_log_parameters(&ctf, &mut base).unwrap();
    let log = LogOpData::new(base, p[0].clone(), p[1].clone(), p[2].clone(), dir).unwrap();
    let rgba = render(&log, &RGBA_IMAGE);
    let rtol = 2f32.powf(-14.0);
    for i in 0..8 {
        let mut expected = RGBA_IMAGE[i];
        if i % 4 != 3 {
            expected = compute_log2lin_eval(expected, &ctf.params[i % 4]);
        }
        assert!(
            equal_with_safe_rel_error(rgba[i], expected, rtol, 1.0),
            "{} vs {}",
            rgba[i],
            expected
        );
    }
    let red = &ctf.params[CTF_RED];
    let res0 = compute_log2lin_eval(0.0, red) as f64;
    assert!(rgba[8].is_nan());
    assert_eq!(rgba[11], 0.0);
    assert_close_f(rgba[12], res0, rtol as f64);
    assert!(rgba[15].is_nan());
    assert_eq!(rgba[16], INF);
    assert_eq!(rgba[19], 0.0);
    assert_close_f(rgba[20], res0, rtol as f64);
    assert_eq!(rgba[23], INF);
    assert_close_f(
        rgba[24],
        compute_log2lin_eval(-INF, red) as f64,
        rtol as f64,
    );
    assert_eq!(rgba[27], 0.0);
    assert_close_f(rgba[28], res0, rtol as f64);
    assert_eq!(rgba[31], -INF);
}

fn compute_lin2log_eval(input: f32, params: &[f64]) -> f32 {
    let min_value = f32::MIN_POSITIVE;
    let gamma = params[0] as f32;
    let ref_white = params[1] as f32 / 1023.0;
    let ref_black = params[2] as f32 / 1023.0;
    let highlight = params[3] as f32;
    let shadow = params[4] as f32;
    let range = 0.002f32 * 1023.0;
    let mult_factor = range / gamma;
    let tmp_value = ((ref_black - ref_white) * mult_factor).min(-0.0001);
    let gain = (highlight - shadow) / (1.0 - 10.0f32.powf(tmp_value));
    let offset = gain - (highlight - shadow);
    let x = (input - shadow + offset) / gain;
    min_value.max(x).log10() / mult_factor + ref_white
}

#[test]
fn cpu_lin2log() {
    let mut ctf = ctf_ref_params();
    ctf.style = LogStyle::LinToLog;
    let mut base = 1.0;
    let dir = get_log_direction(ctf.style);
    let p = convert_log_parameters(&ctf, &mut base).unwrap();
    let log = LogOpData::new(base, p[0].clone(), p[1].clone(), p[2].clone(), dir).unwrap();
    let rgba = render(&log, &RGBA_IMAGE);
    let error = 1e-4;
    for i in 0..8 {
        let mut expected = RGBA_IMAGE[i];
        if i % 4 != 3 {
            expected = compute_lin2log_eval(expected, &ctf.params[i % 4]);
        }
        assert_close(rgba[i] as f64, expected as f64, error);
    }
    let red = &ctf.params[CTF_RED];
    let res0 = compute_lin2log_eval(0.0, red) as f64;
    let res_min = compute_lin2log_eval(-100.0, red) as f64;
    assert_close_f(rgba[8], res_min, error);
    assert_eq!(rgba[11], 0.0);
    assert_close_f(rgba[12], res0, error);
    assert!(rgba[15].is_nan());
    assert_eq!(rgba[16], INF);
    assert_eq!(rgba[19], 0.0);
    assert_close_f(rgba[20], res0, error);
    assert_eq!(rgba[23], INF);
    assert_close_f(rgba[24], res_min, error);
    assert_eq!(rgba[27], 0.0);
    assert_close_f(rgba[28], res0, error);
    assert_eq!(rgba[31], -INF);
}

#[test]
fn cpu_camera_lin2log() {
    let image = [
        -0.1f32, 0., 0.01, 0.0, 0.08, 0.16, 1.16, 0.0, -INF, INF, QNAN, 0.0,
    ];
    let mut params = vec![0.2, 0.6, 1.1, 0.05, 0.1, 1.2];
    let dir = TransformDirection::Forward;
    let log = LogOpData::new(2.0, params.clone(), params.clone(), params.clone(), dir).unwrap();
    let rgba = render(&log, &image);
    let error = 1e-7;
    assert_close_f(rgba[0], -0.168771237955, error);
    assert_close_f(rgba[1], -0.048771237955, error);
    assert_close_f(rgba[2], -0.036771237955, error);
    assert_close_f(rgba[4], 0.047228762045, error);
    assert_close_f(rgba[5], 0.170878935551, error);
    assert_close_f(rgba[6], 0.68141615509, error);
    assert_eq!(rgba[8], -INF);
    assert_eq!(rgba[9], INF);
    assert_close_f(rgba[10], -24.6, error);

    // Linear slope computed.
    params.pop();
    let log = LogOpData::new(2.0, params.clone(), params.clone(), params.clone(), dir).unwrap();
    let rgba = render(&log, &image);
    assert_close_f(rgba[0], -0.325512374199, error);
    assert_close_f(rgba[1], -0.127141806077, error);
    assert_close_f(rgba[2], -0.107304749265, error);
    assert_close_f(rgba[4], 0.031554648421, error);
    assert_close_f(rgba[5], 0.170878935551, error);
    assert_close_f(rgba[6], 0.68141615509, error);
    assert_eq!(rgba[8], -INF);
    assert_eq!(rgba[9], INF);
    assert_close_f(rgba[10], -24.6, error);

    // No break.
    params.pop();
    let log = LogOpData::new(2.0, params.clone(), params.clone(), params, dir).unwrap();
    let rgba = render(&log, &image);
    assert_close_f(rgba[0], -24.6, error);
    assert_close_f(rgba[1], -0.264385618977, error);
    assert_close_f(rgba[2], -0.20700938942, error);
    assert_close_f(rgba[4], 0.028548034423, error);
    assert_close_f(rgba[5], 0.170878935551, error);
    assert_close_f(rgba[6], 0.68141615509, error);
    assert_close_f(rgba[8], -24.6, error);
    assert_eq!(rgba[9], INF);
    assert_close_f(rgba[10], -24.6, error);
}

#[test]
fn cpu_camera_log2lin() {
    let image = [
        -0.168771237955f32,
        -0.048771237955,
        -0.036771237955,
        0.,
        0.047228762045,
        0.170878935551,
        0.68141615509,
        0.,
        -INF,
        INF,
        QNAN,
        0.0,
    ];
    let params = vec![0.2, 0.6, 1.1, 0.05, 0.1, 1.2];
    let log = LogOpData::new(
        2.0,
        params.clone(),
        params.clone(),
        params,
        TransformDirection::Inverse,
    )
    .unwrap();
    let rgba = render(&log, &image);
    let error = 1e-7;
    assert_close_f(rgba[0], -0.1, error);
    assert_close_f(rgba[1], 0.0, error);
    assert_close_f(rgba[2], 0.01, error);
    assert_close_f(rgba[4], 0.08, error);
    assert_close_f(rgba[5], 0.16, error);
    assert_close_f(rgba[6], 1.16, 10.0 * error);
    assert_eq!(rgba[8], -INF);
    assert_eq!(rgba[9], INF);
    assert!(rgba[10].is_nan());
}

// LogTransform_tests.cpp, LogAffineTransform_tests.cpp, LogCameraTransform_tests.cpp

#[test]
fn log_transform_basic() {
    let mut t = LogTransform::default();
    assert_eq!(t.base, 2.0);
    assert_eq!(t.direction, TransformDirection::Forward);
    assert!(t.validate().is_ok());
    t.base = 1.0;
    assert_eq!(
        t.validate().unwrap_err().message(),
        "LogTransform validation failed: Log: Invalid base value '1', base cannot be 1."
    );
    t.base = 10.0;
    let t2 = LogTransform::new(10.0);
    assert!(t.equals(&t2));
    t.direction = TransformDirection::Inverse;
    assert!(!t.equals(&t2));
}

#[test]
fn log_affine_transform_basic() {
    let mut t = LogAffineTransform::default();
    assert_eq!(t.base, 2.0);
    assert_eq!(t.log_side_slope, [1.0; 3]);
    assert_eq!(t.log_side_offset, [0.0; 3]);
    assert_eq!(t.lin_side_slope, [1.0; 3]);
    assert_eq!(t.lin_side_offset, [0.0; 3]);
    assert!(t.validate().is_ok());

    t.lin_side_slope = [0.0, 1.0, 1.0];
    assert!(t
        .validate()
        .unwrap_err()
        .message()
        .contains("linear side slope cannot be 0"));
    t.lin_side_slope = [1.0; 3];
    t.log_side_slope = [1.0, 0.0, 1.0];
    assert!(t
        .validate()
        .unwrap_err()
        .message()
        .contains("log side slope cannot be 0"));
    t.log_side_slope = [1.0; 3];
    t.base = 0.0;
    let e = t.validate().unwrap_err();
    assert!(e
        .message()
        .starts_with("LogAffineTransform validation failed: "));
    assert!(e.message().contains("base must be greater than 0"));
}

#[test]
fn log_camera_transform_basic() {
    let mut t = LogCameraTransform::new([0.1, 0.2, 0.3]);
    assert_eq!(t.base, 2.0);
    assert_eq!(t.lin_side_break, [0.1, 0.2, 0.3]);
    assert!(t.linear_slope.is_none());
    assert!(t.validate().is_ok());
    t.linear_slope = Some([1.0, 1.1, 1.2]);
    assert!(t.validate().is_ok());
    let d = LogOpData::from_log_camera_transform(&t);
    assert_eq!(d.red.len(), 6);
    assert_eq!(d.get_value(LINEAR_SLOPE), Some([1.0, 1.1, 1.2]));
    t.base = 1.0;
    assert!(t
        .validate()
        .unwrap_err()
        .message()
        .starts_with("LogCameraTransform validation failed: "));

    let t1 = LogCameraTransform::new([0.1, 0.2, 0.3]);
    let mut t2 = LogCameraTransform::new([0.1, 0.2, 0.3]);
    assert!(t1.equals(&t2));
    t2.linear_slope = Some([1.0; 3]);
    assert!(!t1.equals(&t2));
}

#[test]
fn log_transform_build_ops() {
    let config = Config::create_raw();
    let ctx = Context::new();
    let t = LogAffineTransform {
        base: 10.0,
        log_side_slope: [0.18; 3],
        log_side_offset: [1.0; 3],
        lin_side_slope: [2.0; 3],
        lin_side_offset: [0.1; 3],
        ..Default::default()
    };
    let mut ops = OpVec::new();
    t.build_ops(&mut ops, &config, &ctx, TransformDirection::Forward)
        .unwrap();
    t.build_ops(&mut ops, &config, &ctx, TransformDirection::Inverse)
        .unwrap();
    assert_eq!(ops.len(), 2);
    assert_eq!(
        log_op(&ops, 0).data().direction,
        TransformDirection::Forward
    );
    assert_eq!(
        log_op(&ops, 1).data().direction,
        TransformDirection::Inverse
    );
    let out = apply_ops(&ops[..1], &LIN);
    for i in 0..8 {
        assert_close(out[i] as f64, LOG[i] as f64, 1e-3);
    }

    // Inverse of an inverse transform is forward.
    let mut inv = LogTransform::new(10.0);
    inv.direction = TransformDirection::Inverse;
    let mut ops = OpVec::new();
    inv.build_ops(&mut ops, &config, &ctx, TransformDirection::Inverse)
        .unwrap();
    assert_eq!(
        log_op(&ops, 0).data().direction,
        TransformDirection::Forward
    );
    assert!(matches!(
        log_op(&ops, 0).renderer(),
        LogRenderer::Log { .. }
    ));

    // Camera log forward then inverse: removed by the optimizer.
    let cam = LogCameraTransform::new([0.1; 3]);
    let mut ops = OpVec::new();
    cam.build_ops(&mut ops, &config, &ctx, TransformDirection::Inverse)
        .unwrap();
    cam.build_ops(&mut ops, &config, &ctx, TransformDirection::Forward)
        .unwrap();
    assert!(optimize_ops(&ops, OptimizationFlags::DEFAULT).is_empty());
}
