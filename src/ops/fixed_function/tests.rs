//! Tests ported from `FixedFunctionOpData_tests.cpp`, `FixedFunctionOp_tests.cpp`
//! and `FixedFunctionTransform_tests.cpp`.

use super::*;
use crate::processor::optimize_ops;
use crate::transforms::GroupTransform;

fn data(style: FixedFunctionOpStyle, params: &[f64]) -> FixedFunctionOpData {
    FixedFunctionOpData::new(style, params).unwrap()
}

fn err_msg<T: fmt::Debug>(r: Result<T>) -> String {
    r.expect_err("expected an error").message().to_string()
}

#[test]
fn format_g_matches_ostream() {
    assert_eq!(format_g(1.0, 6), "1");
    assert_eq!(format_g(65535.0, 6), "65535");
    assert_eq!(format_g(65504.0, 6), "65504");
    assert_eq!(format_g(1.001, 6), "1.001");
    assert_eq!(format_g(0.9995, 6), "0.9995");
    assert_eq!(format_g(-0.1, 6), "-0.1");
    assert_eq!(format_g(0.00001, 6), "1e-05");
    assert_eq!(format_g(120.0, 6), "120");
    assert_eq!(format_g(0.0, 6), "0");
    assert_eq!(format_g(1234567.0, 6), "1.23457e+06");
    assert_eq!(format_g(0.0001, 6), "0.0001");
    assert_eq!(format_g(1.147, 7), "1.147");
    assert_eq!(format_g(std::f64::consts::E, 7), "2.718282");
    assert_eq!(format_g(0.807825590164, 7), "0.8078256");
}

// ---------------------------------------------------------------------------
// FixedFunctionOpData

#[test]
fn op_data_aces_red_mod_style() {
    let mut func = data(S::AcesRedMod03Fwd, &[]);
    assert_eq!(func.style, S::AcesRedMod03Fwd);
    assert!(func.params.is_empty());
    assert!(func.validate().is_ok());
    let cache_id = func.cache_id();

    func.style = S::AcesRedMod10Fwd;
    assert_eq!(func.style, S::AcesRedMod10Fwd);
    assert!(func.validate().is_ok());

    let cache_id_updated = func.cache_id();
    assert_ne!(cache_id, cache_id_updated);

    let inv = func.inverse();
    assert_eq!(inv.style, S::AcesRedMod10Inv);
    assert!(inv.params.is_empty());
    assert_ne!(cache_id, inv.cache_id());

    func.params.push(1.0);
    assert_eq!(
        err_msg(func.validate()),
        "The style 'ACES_RedMod10 (Forward)' must have zero parameters but 1 found."
    );
}

#[test]
fn op_data_aces_dark_to_dim10_style() {
    let mut func = data(S::AcesDarkToDim10Fwd, &[]);
    assert_eq!(func.style, S::AcesDarkToDim10Fwd);
    assert!(func.params.is_empty());
    assert!(func.validate().is_ok());
    let cache_id = func.cache_id();

    let inv = func.inverse();
    assert_eq!(inv.style, S::AcesDarkToDim10Inv);
    assert!(inv.params.is_empty());
    assert_ne!(cache_id, inv.cache_id());

    func.params.push(1.0);
    assert_eq!(
        err_msg(func.validate()),
        "The style 'ACES_DarkToDim10 (Forward)' must have zero parameters but 1 found."
    );
}

#[test]
fn op_data_aces_gamut_comp_13_style() {
    let params = [1.147, 1.264, 1.312, 0.815, 0.803, 0.880, 1.2];
    let mut func = data(S::AcesGamutComp13Fwd, &params);
    assert!(func.validate().is_ok());
    let cache_id = func.cache_id();
    assert_eq!(func.params, params);

    let inv = func.inverse();
    assert_eq!(inv.params[0], func.params[0]);
    assert_eq!(inv.style, S::AcesGamutComp13Inv);
    assert_ne!(cache_id, inv.cache_id());

    assert!(func == func);
    assert!(func != inv);

    let mut test_params = params.to_vec();
    test_params.push(12.0);
    func.params = test_params;
    assert_eq!(
        err_msg(func.validate()),
        "The style 'ACES_GamutComp13 (Forward)' must have seven parameters but 8 found."
    );

    let mut test_params = params.to_vec();
    test_params.pop();
    func.params = test_params;
    assert_eq!(
        err_msg(func.validate()),
        "The style 'ACES_GamutComp13 (Forward)' must have seven parameters but 6 found."
    );

    func.params.clear();
    assert_eq!(
        err_msg(func.validate()),
        "The style 'ACES_GamutComp13 (Forward)' must have seven parameters but 0 found."
    );

    let check = |idx: usize, value: f64, msg: &str| {
        let mut f = func.clone();
        let mut p = params.to_vec();
        p[idx] = value;
        f.params = p;
        assert_eq!(err_msg(f.validate()), msg);
    };
    check(
        0,
        1.0,
        "Parameter 1 (lim_cyan) is outside valid range [1.001,65504]",
    );
    check(
        0,
        65535.0,
        "Parameter 65535 (lim_cyan) is outside valid range [1.001,65504]",
    );
    check(
        1,
        1.0,
        "Parameter 1 (lim_magenta) is outside valid range [1.001,65504]",
    );
    check(
        1,
        65535.0,
        "Parameter 65535 (lim_magenta) is outside valid range [1.001,65504]",
    );
    check(
        2,
        1.0,
        "Parameter 1 (lim_yellow) is outside valid range [1.001,65504]",
    );
    check(
        2,
        65535.0,
        "Parameter 65535 (lim_yellow) is outside valid range [1.001,65504]",
    );
    check(
        3,
        -0.1,
        "Parameter -0.1 (thr_cyan) is outside valid range [0,0.9995]",
    );
    check(
        3,
        1.0,
        "Parameter 1 (thr_cyan) is outside valid range [0,0.9995]",
    );
    check(
        4,
        -0.1,
        "Parameter -0.1 (thr_magenta) is outside valid range [0,0.9995]",
    );
    check(
        4,
        1.0,
        "Parameter 1 (thr_magenta) is outside valid range [0,0.9995]",
    );
    check(
        5,
        -0.1,
        "Parameter -0.1 (thr_yellow) is outside valid range [0,0.9995]",
    );
    check(
        5,
        1.0,
        "Parameter 1 (thr_yellow) is outside valid range [0,0.9995]",
    );
    check(
        6,
        0.0,
        "Parameter 0 (power) is outside valid range [1,65504]",
    );
    check(
        6,
        65535.0,
        "Parameter 65535 (power) is outside valid range [1,65504]",
    );
}

#[test]
fn op_data_rec2100_surround_style() {
    let params = [2.0];
    let mut func = data(S::Rec2100SurroundFwd, &params);
    assert!(func.validate().is_ok());
    let cache_id = func.cache_id();
    assert_eq!(func.params, params);

    let inv = func.inverse();
    assert_eq!(inv.params[0], func.params[0]);
    assert_eq!(inv.style, S::Rec2100SurroundInv);
    assert_ne!(cache_id, inv.cache_id());

    assert!(func == func);
    assert!(func != inv);

    func.params[0] = 120.0;
    assert_eq!(
        err_msg(func.validate()),
        "Parameter 120 is greater than upper bound 100"
    );

    func.params[0] = 0.00001;
    assert_eq!(
        err_msg(func.validate()),
        "Parameter 1e-05 is less than lower bound 0.01"
    );

    func.params.push(12.0);
    assert_eq!(
        err_msg(func.validate()),
        "The style 'REC2100_Surround (Forward)' must have one parameter but 2 found."
    );

    func.params.clear();
    assert_eq!(
        err_msg(func.validate()),
        "The style 'REC2100_Surround (Forward)' must have one parameter but 0 found."
    );
}

#[test]
fn op_data_aces_lin_to_doublelog_style() {
    let params = [
        10.0, 0.25, 0.5, -1.0, 0.0, -1.0, 1.25, 1.0, 1.0, 1.0, 0.5, 1.0, 0.0,
    ];
    let mut func = data(S::LinToDoubleLog, &params);
    assert!(func.validate().is_ok());
    let cache_id = func.cache_id();
    assert_eq!(func.params, params);

    let inv = func.inverse();
    assert_eq!(inv.params[0], func.params[0]);
    assert_eq!(inv.style, S::DoubleLogToLin);
    assert_ne!(cache_id, inv.cache_id());

    assert!(func == func);
    assert!(func != inv);

    let mut test_params = params.to_vec();
    test_params.push(12.0);
    func.params = test_params;
    assert_eq!(
        err_msg(func.validate()),
        "The style 'Lin_TO_DoubleLog' must have 13 parameters but 14 found."
    );

    let mut test_params = params.to_vec();
    test_params.pop();
    func.params = test_params;
    assert_eq!(
        err_msg(func.validate()),
        "The style 'Lin_TO_DoubleLog' must have 13 parameters but 12 found."
    );

    func.params.clear();
    assert_eq!(
        err_msg(func.validate()),
        "The style 'Lin_TO_DoubleLog' must have 13 parameters but 0 found."
    );

    let mut test_params = params.to_vec();
    test_params[1] = 1.0;
    func.params = test_params;
    assert_eq!(
        err_msg(func.validate()),
        "First break point 1 is larger than the second break point 0.5."
    );

    let mut test_params = params.to_vec();
    test_params[0] = 0.0;
    func.params = test_params;
    assert_eq!(
        err_msg(func.validate()),
        "Log base 0 is not greater than zero."
    );
}

#[test]
fn op_data_aces_lin_to_gammalog_style() {
    let params = [
        0.0,
        0.25,
        0.5,
        1.0,
        0.0,
        2.718,
        0.17883277,
        0.807825590164,
        1.0,
        -0.07116723,
    ];
    let mut func = data(S::LinToGammaLog, &params);
    assert!(func.validate().is_ok());
    let cache_id = func.cache_id();
    assert_eq!(func.params, params);

    let inv = func.inverse();
    assert_eq!(inv.params[0], func.params[0]);
    assert_eq!(inv.style, S::GammaLogToLin);
    assert_ne!(cache_id, inv.cache_id());

    assert!(func == func);
    assert!(func != inv);

    let mut test_params = params.to_vec();
    test_params.push(12.0);
    func.params = test_params;
    assert_eq!(
        err_msg(func.validate()),
        "The style 'Lin_TO_GammaLog' must have 10 parameters but 11 found."
    );

    let mut test_params = params.to_vec();
    test_params.pop();
    func.params = test_params;
    assert_eq!(
        err_msg(func.validate()),
        "The style 'Lin_TO_GammaLog' must have 10 parameters but 9 found."
    );

    func.params.clear();
    assert_eq!(
        err_msg(func.validate()),
        "The style 'Lin_TO_GammaLog' must have 10 parameters but 0 found."
    );

    let mut test_params = params.to_vec();
    test_params[0] = 1.0;
    func.params = test_params;
    assert_eq!(
        err_msg(func.validate()),
        "Mirror point 1 is not smaller than the break point 0.25."
    );

    let mut test_params = params.to_vec();
    test_params[5] = -1.0;
    func.params = test_params;
    assert_eq!(
        err_msg(func.validate()),
        "Log base -1 is not greater than zero."
    );

    let mut test_params = params.to_vec();
    test_params[2] = 0.0;
    func.params = test_params;
    assert_eq!(err_msg(func.validate()), "Gamma power is zero.");
}

#[test]
fn op_data_aces2_styles() {
    let p8 = [
        0.7347, 0.2653, 0.0000, 1.0000, 0.0001, -0.0770, 0.32168, 0.33767,
    ];
    assert!(FixedFunctionOpData::new(S::AcesRgbToJmh20, &p8).is_ok());
    assert_eq!(
        err_msg(FixedFunctionOpData::new(S::AcesJmhToRgb20, &p8[..7])),
        "The style 'JMh_TO_RGB_20' must have 8 parameters but 7 found."
    );

    assert!(FixedFunctionOpData::new(S::AcesTonescaleCompress20Fwd, &[1000.0]).is_ok());
    assert_eq!(
        err_msg(FixedFunctionOpData::new(S::AcesTonescaleCompress20Inv, &[])),
        "The style 'ACES_ToneScaleCompress20 (Inverse)' must have 1 parameters but 0 found."
    );
    assert_eq!(
        err_msg(FixedFunctionOpData::new(
            S::AcesTonescaleCompress20Fwd,
            &[0.5]
        )),
        "Parameter 0.5 (peak_luminance) is outside valid range [1,10000]"
    );
    assert_eq!(
        err_msg(FixedFunctionOpData::new(
            S::AcesTonescaleCompress20Fwd,
            &[100.5]
        )),
        "Parameter 100.5 (peak_luminance) cannot include any fractional component"
    );

    let p9 = [
        1000.0, 0.680, 0.320, 0.265, 0.690, 0.150, 0.060, 0.3127, 0.3290,
    ];
    assert!(FixedFunctionOpData::new(S::AcesOutputTransform20Fwd, &p9).is_ok());
    assert!(FixedFunctionOpData::new(S::AcesGamutCompress20Inv, &p9).is_ok());
    assert_eq!(
        err_msg(FixedFunctionOpData::new(S::AcesOutputTransform20Inv, &p8)),
        "The style 'ACES_OutputTransform20 (Inverse)' must have 9 parameters but 8 found."
    );
    let mut bad = p9;
    bad[0] = 20000.0;
    assert_eq!(
        err_msg(FixedFunctionOpData::new(S::AcesGamutCompress20Fwd, &bad)),
        "Parameter 20000 (peak_luminance) is outside valid range [1,10000]"
    );
}

#[test]
fn op_data_is_inverse() {
    let f_s = data(S::Rec2100SurroundFwd, &[2.0]);
    let f_s_inv1 = data(S::Rec2100SurroundFwd, &[0.5]);
    let f_s_inv2 = data(S::Rec2100SurroundInv, &[2.0]);

    assert!(f_s.is_inverse(&f_s_inv1));
    assert!(f_s.is_inverse(&f_s_inv2));

    assert!(!f_s.is_inverse(&f_s));
    assert!(!f_s_inv1.is_inverse(&f_s_inv1));
    assert!(!f_s_inv2.is_inverse(&f_s_inv2));
    assert!(!f_s_inv1.is_inverse(&f_s_inv2));

    let f_g = data(S::AcesGlow03Fwd, &[]);
    let f_g_inv = data(S::AcesGlow03Inv, &[]);
    assert!(f_g.is_inverse(&f_g_inv));
    assert!(f_g_inv.is_inverse(&f_g));
    assert!(!f_g.is_inverse(&f_g));
    assert!(!f_g_inv.is_inverse(&f_g_inv));
    assert!(!f_g.is_inverse(&f_s));

    let f_r = data(S::AcesRedMod03Fwd, &[]);
    let f_r_inv = data(S::AcesRedMod03Inv, &[]);
    assert!(f_r.is_inverse(&f_r_inv));
    assert!(f_r_inv.is_inverse(&f_r));
    assert!(!f_r.is_inverse(&f_r));
    assert!(!f_r_inv.is_inverse(&f_r_inv));
    assert!(!f_r.is_inverse(&f_g));

    let mut p7 = [1.147, 1.264, 1.312, 0.815, 0.803, 0.880, 1.2];
    let f_gm = data(S::AcesGamutComp13Fwd, &p7);
    let f_gm_inv = data(S::AcesGamutComp13Inv, &p7);
    assert!(f_gm.is_inverse(&f_gm_inv));
    assert!(f_gm_inv.is_inverse(&f_gm));
    assert!(!f_gm.is_inverse(&f_gm));
    assert!(!f_gm_inv.is_inverse(&f_gm_inv));
    assert!(!f_gm.is_inverse(&f_r));

    p7[6] += 0.01;
    let f_gm_inv = data(S::AcesGamutComp13Inv, &p7);
    assert!(!f_gm_inv.is_inverse(&f_gm));
    assert!(!f_gm.is_inverse(&f_gm_inv));

    let p8 = [
        0.7347, 0.2653, 0.0000, 1.0000, 0.0001, -0.0770, 0.32168, 0.33767,
    ];
    let f_hmj = data(S::AcesRgbToHmj20, &p8);
    let f_hmj_inv = data(S::AcesHmjToRgb20, &p8);
    assert!(f_hmj.is_inverse(&f_hmj_inv));
    assert!(f_hmj_inv.is_inverse(&f_hmj));
    assert!(!f_hmj.is_inverse(&f_hmj));
    assert!(!f_hmj_inv.is_inverse(&f_hmj_inv));
    assert!(!f_hmj.is_inverse(&f_gm));
}

#[test]
fn op_data_style_names() {
    for style in FixedFunctionOpStyle::all() {
        // CTF names round trip (case insensitive).
        assert_eq!(
            FixedFunctionOpStyle::from_name(style.as_str()).unwrap(),
            style
        );
        assert_eq!(
            FixedFunctionOpStyle::from_name(&style.as_str().to_ascii_lowercase()).unwrap(),
            style
        );
        // Inverse is an involution and flips the direction.
        assert_eq!(style.inverse().inverse(), style);
        assert_ne!(style.inverse().direction(), style.direction());
        assert_eq!(style.inverse().transform_style(), style.transform_style());
        // Transform style + direction round trips.
        let ts = style.transform_style();
        let fwd =
            FixedFunctionOpStyle::from_transform_style(ts, TransformDirection::Forward).unwrap();
        let mut d = FixedFunctionOpData {
            style: fwd,
            params: vec![],
            metadata: FormatMetadata::default(),
        };
        d.set_direction(style.direction());
        assert_eq!(d.style, style);
    }
    assert_eq!(
        FixedFunctionOpStyle::from_name("Surround").unwrap(),
        S::Rec2100SurroundFwd
    );
    assert_eq!(
        err_msg(FixedFunctionOpStyle::from_name("foo")),
        "Unknown FixedFunction style: foo"
    );
    assert_eq!(S::AcesGlow10Inv.detailed_str(), "ACES_Glow10 (Inverse)");
    assert_eq!(S::AcesGlow10Inv.as_str(), "Glow10Rev");
    assert_eq!(S::XyyToXyz.detailed_str(), "xyY_TO_XYZ");
}

#[test]
fn op_data_cache_id() {
    let mut d = data(
        S::AcesGamutComp13Fwd,
        &[1.147, 1.264, 1.312, 0.815, 0.803, 0.880, 1.2],
    );
    assert_eq!(
        d.cache_id(),
        "ACES_GamutComp13 (Forward) 1.147 1.264 1.312 0.815 0.803 0.88 1.2"
    );
    d.metadata.set_id("abc");
    assert_eq!(
        d.cache_id(),
        "abc ACES_GamutComp13 (Forward) 1.147 1.264 1.312 0.815 0.803 0.88 1.2"
    );
}

// ---------------------------------------------------------------------------
// FixedFunctionOp

fn ff_op(op: &OpRc) -> &FixedFunctionOp {
    op.downcast_ref::<FixedFunctionOp>().unwrap()
}

use crate::ops::OpRc;

fn check_inverse_pair(ops: &OpVec) {
    assert_eq!(ops.len(), 2);
    let op0 = ff_op(&ops[0]);
    let op1 = ff_op(&ops[1]);
    assert!(!op0.is_identity());
    assert!(!op1.is_identity());
    assert!(op0.is_inverse(op1));
    assert!(op1.is_inverse(op0));

    // The optimizer removes the pair.
    assert!(optimize_ops(ops, OptimizationFlags::DEFAULT).is_empty());
    assert_eq!(
        optimize_ops(
            ops,
            OptimizationFlags::DEFAULT & !OptimizationFlags::PAIR_IDENTITY_FIXED_FUNCTION
        )
        .len(),
        2
    );
}

#[test]
fn op_basic() {
    let mut ops = OpVec::new();
    create_fixed_function_op(
        &mut ops,
        S::AcesRedMod10Fwd,
        &[],
        TransformDirection::Forward,
    )
    .unwrap();
    assert_eq!(ops.len(), 1);
    let func = ff_op(&ops[0]);
    assert!(!func.is_no_op());
    assert!(!func.is_identity());
    assert!(func.has_channel_crosstalk());
    assert_eq!(func.data().style, S::AcesRedMod10Fwd);
    assert!(func.data().params.is_empty());
    assert_eq!(func.cache_id(), "<FixedFunctionOp ACES_RedMod10 (Forward)>");

    // Inverse direction.
    create_fixed_function_op(
        &mut ops,
        S::AcesRedMod10Fwd,
        &[],
        TransformDirection::Inverse,
    )
    .unwrap();
    assert_eq!(ff_op(&ops[1]).data().style, S::AcesRedMod10Inv);

    // Invalid parameters.
    assert!(create_fixed_function_op(
        &mut ops,
        S::AcesRedMod10Fwd,
        &[1.0],
        TransformDirection::Forward
    )
    .is_err());
    assert_eq!(ops.len(), 2);
}

#[test]
fn op_glow03_cpu_engine() {
    let op = FixedFunctionOp::new(data(S::AcesGlow03Fwd, &[])).unwrap();
    assert!(op.renderer_name().contains("Renderer_ACES_Glow03_Fwd"));
}

#[test]
fn op_darktodim10_cpu_engine() {
    let op = FixedFunctionOp::new(data(S::AcesDarkToDim10Fwd, &[])).unwrap();
    assert!(op.renderer_name().contains("Renderer_ACES_DarkToDim10_Fwd"));
}

fn pair(s0: FixedFunctionOpStyle, s1: FixedFunctionOpStyle, params: &[f64]) -> OpVec {
    let mut ops = OpVec::new();
    create_fixed_function_op(&mut ops, s0, params, TransformDirection::Forward).unwrap();
    create_fixed_function_op(&mut ops, s1, params, TransformDirection::Forward).unwrap();
    ops
}

#[test]
fn op_aces_red_mod_inv() {
    check_inverse_pair(&pair(S::AcesRedMod03Inv, S::AcesRedMod03Fwd, &[]));
}

#[test]
fn op_aces_glow_inv() {
    check_inverse_pair(&pair(S::AcesGlow03Inv, S::AcesGlow03Fwd, &[]));
}

#[test]
fn op_aces_darktodim10_inv() {
    check_inverse_pair(&pair(S::AcesDarkToDim10Inv, S::AcesDarkToDim10Fwd, &[]));
}

#[test]
fn op_aces_gamutmap13_inv() {
    let params = [1.147, 1.264, 1.312, 0.815, 0.803, 0.880, 1.2];
    check_inverse_pair(&pair(S::AcesGamutComp13Inv, S::AcesGamutComp13Fwd, &params));
}

#[test]
fn op_rec2100_surround_inv() {
    let mut ops = OpVec::new();
    let fwd = TransformDirection::Forward;
    create_fixed_function_op(&mut ops, S::Rec2100SurroundFwd, &[2.0], fwd).unwrap();
    create_fixed_function_op(&mut ops, S::Rec2100SurroundFwd, &[1.0 / 2.0], fwd).unwrap();
    create_fixed_function_op(&mut ops, S::Rec2100SurroundInv, &[2.0], fwd).unwrap();
    assert_eq!(ops.len(), 3);
    {
        let op0 = ff_op(&ops[0]);
        let op1 = ff_op(&ops[1]);
        let op2 = ff_op(&ops[2]);
        assert!(!op0.is_identity());
        assert!(!op1.is_identity());
        assert!(!op2.is_identity());
        assert!(op0.is_inverse(op1));
        assert!(op1.is_inverse(op0));
        assert!(op0.is_inverse(op2));
        assert!(op2.is_inverse(op0));
    }
    create_fixed_function_op(&mut ops, S::Rec2100SurroundFwd, &[2.01], fwd).unwrap();
    assert_eq!(ops.len(), 4);
    {
        let op0 = ff_op(&ops[0]);
        let op1 = ff_op(&ops[1]);
        let op3 = ff_op(&ops[3]);
        assert!(!op0.is_inverse(op3));
        assert!(!op1.is_inverse(op3));
    }
}

#[test]
fn op_create_transform() {
    let mut func_data = data(S::Rec2100SurroundInv, &[0.5]);
    assert_eq!(func_data.style, S::Rec2100SurroundInv);
    // Direction is already inverse, this does nothing.
    func_data.set_direction(TransformDirection::Inverse);
    assert_eq!(func_data.style, S::Rec2100SurroundInv);
    // Changing the direction is changing the style.
    func_data.set_direction(TransformDirection::Forward);
    assert_eq!(func_data.style, S::Rec2100SurroundFwd);
    func_data.set_direction(TransformDirection::Inverse);
    assert_eq!(func_data.style, S::Rec2100SurroundInv);

    func_data.metadata.add_attribute("name", "test");

    let mut ops = OpVec::new();
    create_fixed_function_op_from_data(&mut ops, &func_data, TransformDirection::Forward).unwrap();
    assert_eq!(ops.len(), 1);

    let mut group = GroupTransform::new();
    group.append(ops[0].to_transform().unwrap());
    assert_eq!(group.num_transforms(), 1);
    let Transform::FixedFunction(ff) = &group.transforms[0] else {
        panic!("expected a FixedFunctionTransform");
    };
    assert_eq!(ff.metadata.attribute_value("name"), "test");
    assert_eq!(ff.direction, TransformDirection::Inverse);
    assert_eq!(ff.style, FixedFunctionStyle::Rec2100Surround);
    assert_eq!(ff.params, vec![0.5]);
}

fn check_renderer_pair(
    s0: FixedFunctionOpStyle,
    s1: FixedFunctionOpStyle,
    params: &[f64],
    name: &str,
) {
    let ops = pair(s0, s1, params);
    check_inverse_pair(&ops);
    assert!(ff_op(&ops[0]).renderer_name().contains(name));
}

#[test]
fn ops_rgb_to_hsv() {
    check_renderer_pair(S::RgbToHsv, S::HsvToRgb, &[], "Renderer_RGB_TO_HSV");
}

#[test]
fn ops_rgb_to_hsy_lin() {
    check_renderer_pair(
        S::RgbToHsyLin,
        S::HsyLinToRgb,
        &[],
        "Renderer_RGB_TO_HSY_LIN",
    );
}

#[test]
fn ops_rgb_to_hsy_log() {
    check_renderer_pair(
        S::RgbToHsyLog,
        S::HsyLogToRgb,
        &[],
        "Renderer_RGB_TO_HSY_LOG",
    );
}

#[test]
fn ops_rgb_to_hsy_vid() {
    check_renderer_pair(
        S::RgbToHsyVid,
        S::HsyVidToRgb,
        &[],
        "Renderer_RGB_TO_HSY_VID",
    );
}

#[test]
fn ops_xyz_to_xyy() {
    check_renderer_pair(S::XyzToXyy, S::XyyToXyz, &[], "Renderer_XYZ_TO_xyY");
}

#[test]
fn ops_xyz_to_uvy() {
    check_renderer_pair(S::XyzToUvy, S::UvyToXyz, &[], "Renderer_XYZ_TO_uvY");
}

#[test]
fn ops_xyz_to_luv() {
    check_renderer_pair(S::XyzToLuv, S::LuvToXyz, &[], "Renderer_XYZ_TO_LUV");
}

#[test]
fn ops_lin_to_pq() {
    check_renderer_pair(S::PqToLin, S::LinToPq, &[], "Renderer_PQ_TO_LIN");
}

/// Parameters for the Rec.2100 HLG curve.
const HLG_PARAMS: [f64; 10] = [
    0.0,  // mirror point
    0.25, // break point
    // Gamma segment.
    0.5, // gamma power
    1.0, // post-power scale
    0.0, // pre-power offset
    // Log segment.
    std::f64::consts::E, // log base (e)
    0.17883277,          // log-side slope
    0.807825590164,      // log-side offset
    1.0,                 // lin-side slope
    -0.07116723,         // lin-side offset
];

#[test]
fn ops_lin_to_gamma_log() {
    check_renderer_pair(
        S::GammaLogToLin,
        S::LinToGammaLog,
        &HLG_PARAMS,
        "Renderer_GAMMA_LOG_TO_LIN",
    );
}

#[test]
fn ops_lin_to_double_log() {
    let params = [
        10.0, // base for the log
        0.5,  // break point between log1 and linear segments
        0.5,  // break point between linear and log2 segments
        1.0, 0.0, 1.0, 0.0, // log curve 1
        1.0, 0.0, 1.0, 0.0, // log curve 2
        1.0, 0.0, // linear segment slope and offset
    ];
    check_renderer_pair(
        S::LinToDoubleLog,
        S::DoubleLogToLin,
        &params,
        "Renderer_LIN_TO_DOUBLE_LOG",
    );
}

#[test]
fn ops_aces2_renderer_names() {
    let p8 = [
        0.7347, 0.2653, 0.0000, 1.0000, 0.0001, -0.0770, 0.32168, 0.33767,
    ];
    let p9 = [100.0, 0.64, 0.33, 0.30, 0.60, 0.15, 0.06, 0.3127, 0.3290];
    let names = [
        (S::AcesRgbToJmh20, &p8[..], "Renderer_ACES_RGB_TO_JMh_20"),
        (S::AcesHmjToRgb20, &p8[..], "Renderer_ACES_RGB_TO_HMJ_20"),
        (
            S::AcesTonescaleCompress20Inv,
            &[100.0][..],
            "Renderer_ACES_TONESCALE_COMPRESS_20",
        ),
        (
            S::AcesGamutCompress20Fwd,
            &p9[..],
            "Renderer_ACES_GAMUT_COMPRESS_20",
        ),
        (
            S::AcesOutputTransform20Inv,
            &p9[..],
            "Renderer_ACES_OutputTransform20",
        ),
    ];
    for (style, params, name) in names {
        let op = FixedFunctionOp::new(data(style, params)).unwrap();
        assert_eq!(op.renderer_name(), name);
    }
    let pairs = pair(
        S::AcesOutputTransform20Fwd,
        S::AcesOutputTransform20Inv,
        &p9,
    );
    check_inverse_pair(&pairs);
}

#[test]
fn op_to_transform_and_back() {
    let params = [1.147, 1.264, 1.312, 0.815, 0.803, 0.880, 1.2];
    let op = FixedFunctionOp::new(data(S::AcesGamutComp13Inv, &params)).unwrap();
    let Some(Transform::FixedFunction(t)) = op.to_transform() else {
        panic!("expected a FixedFunctionTransform");
    };
    assert_eq!(t.style, FixedFunctionStyle::AcesGamutComp13);
    assert_eq!(t.direction, TransformDirection::Inverse);

    let mut ops = OpVec::new();
    t.build_ops(
        &mut ops,
        &Config::create_raw(),
        &Context::new(),
        TransformDirection::Forward,
    )
    .unwrap();
    assert_eq!(ff_op(&ops[0]).data(), op.data());
    t.build_ops(
        &mut ops,
        &Config::create_raw(),
        &Context::new(),
        TransformDirection::Inverse,
    )
    .unwrap();
    assert_eq!(ff_op(&ops[1]).data().style, S::AcesGamutComp13Fwd);
    assert!(optimize_ops(&ops, OptimizationFlags::DEFAULT).is_empty());
}

// ---------------------------------------------------------------------------
// FixedFunctionTransform

#[test]
fn transform_basic() {
    let mut func = FixedFunctionTransform::new(FixedFunctionStyle::AcesRedMod03, &[]);
    assert_eq!(func.direction, TransformDirection::Forward);
    assert_eq!(func.style, FixedFunctionStyle::AcesRedMod03);
    assert!(func.params.is_empty());
    assert!(func.validate().is_ok());

    func.direction = TransformDirection::Inverse;
    assert_eq!(func.direction, TransformDirection::Inverse);
    assert_eq!(func.style, FixedFunctionStyle::AcesRedMod03);
    assert!(func.validate().is_ok());

    func.style = FixedFunctionStyle::AcesRedMod10;
    assert_eq!(func.direction, TransformDirection::Inverse);
    assert!(func.validate().is_ok());

    func.style = FixedFunctionStyle::AcesGamutComp13;
    assert_eq!(
        err_msg(func.validate()),
        "FixedFunctionTransform validation failed: The style 'ACES_GamutComp13 (Inverse)' must have \
         seven parameters but 0 found."
    );
    let values_7 = [1.147, 1.264, 1.312, 0.815, 0.803, 0.880, 1.2];
    func.params = values_7.to_vec();
    assert_eq!(func.params.len(), 7);
    assert!(func.validate().is_ok());

    func.params.clear();
    func.style = FixedFunctionStyle::Rec2100Surround;
    assert_eq!(
        err_msg(func.validate()),
        "FixedFunctionTransform validation failed: The style 'REC2100_Surround (Inverse)' must have \
         one parameter but 0 found."
    );

    func.params = vec![1.0];
    assert_eq!(func.params, vec![1.0]);
    assert!(func.validate().is_ok());

    func.style = FixedFunctionStyle::AcesDarkToDim10;
    assert_eq!(
        err_msg(func.validate()),
        "FixedFunctionTransform validation failed: The style 'ACES_DarkToDim10 (Inverse)' must have \
         zero parameters but 1 found."
    );

    // Note: as in OCIO, the RGB_TO_HSV style is converted ignoring the
    // direction, then inverted to match the transform direction.
    func.style = FixedFunctionStyle::RgbToHsv;
    assert_eq!(
        err_msg(func.validate()),
        "FixedFunctionTransform validation failed: The style 'HSV_TO_RGB' must have \
         zero parameters but 1 found."
    );

    func.style = FixedFunctionStyle::AcesGamutMap02;
    assert!(err_msg(func.validate()).ends_with(
        "Unimplemented fixed function types: FIXED_FUNCTION_ACES_GAMUTMAP_02, FIXED_FUNCTION_ACES_GAMUTMAP_07."
    ));
    assert_eq!(
        err_msg(FixedFunctionOpStyle::from_transform_style(
            FixedFunctionStyle::AcesGamutMap07,
            TransformDirection::Forward
        )),
        "Unimplemented fixed function types: FIXED_FUNCTION_ACES_GAMUTMAP_02, FIXED_FUNCTION_ACES_GAMUTMAP_07."
    );
    let mut ops = OpVec::new();
    assert!(func
        .build_ops(
            &mut ops,
            &Config::create_raw(),
            &Context::new(),
            TransformDirection::Forward
        )
        .is_err());

    func.params = values_7.to_vec();
    func.style = FixedFunctionStyle::AcesRgbToHmj20;
    func.direction = TransformDirection::Inverse;
    assert_eq!(
        err_msg(func.validate()),
        "FixedFunctionTransform validation failed: The style 'HMJ_TO_RGB_20' must have \
         8 parameters but 7 found."
    );

    let values_8 = [
        0.7347, 0.2653, 0.0000, 1.0000, 0.0001, -0.0770, 0.32168, 0.33767,
    ];
    func.params = values_8.to_vec();
    assert_eq!(func.params.len(), 8);
    assert!(func.validate().is_ok());

    func.direction = TransformDirection::Forward;
    assert_eq!(func.style, FixedFunctionStyle::AcesRgbToHmj20);
    assert!(func.validate().is_ok());
}

#[test]
fn transform_create_editable_copy() {
    let func = FixedFunctionTransform::new(FixedFunctionStyle::AcesRedMod03, &[]);
    let copy = func.clone();
    assert_eq!(copy, func);

    let func = FixedFunctionTransform::new(FixedFunctionStyle::Rec2100Surround, &[1.0]);
    let copy = func.clone();
    assert_eq!(copy, func);
    assert!(copy.validate().is_ok());
}

#[test]
fn transform_display() {
    let mut func = FixedFunctionTransform::new(FixedFunctionStyle::Rec2100Surround, &[0.78]);
    assert_eq!(
        func.to_string(),
        "<FixedFunction direction=forward, style=REC2100_Surround, params=[0.78]>"
    );
    func.direction = TransformDirection::Inverse;
    func.style = FixedFunctionStyle::AcesGlow10;
    func.params.clear();
    assert_eq!(
        func.to_string(),
        "<FixedFunction direction=inverse, style=ACES_Glow10>"
    );
}

#[test]
fn transform_build_ops_directions() {
    let config = Config::create_raw();
    let context = Context::new();
    let mut t = FixedFunctionTransform::new(FixedFunctionStyle::XyzToLuv, &[]);
    let mut ops = OpVec::new();
    t.build_ops(&mut ops, &config, &context, TransformDirection::Forward)
        .unwrap();
    t.build_ops(&mut ops, &config, &context, TransformDirection::Inverse)
        .unwrap();
    t.direction = TransformDirection::Inverse;
    t.build_ops(&mut ops, &config, &context, TransformDirection::Forward)
        .unwrap();
    t.build_ops(&mut ops, &config, &context, TransformDirection::Inverse)
        .unwrap();
    let styles: Vec<_> = ops.iter().map(|o| ff_op(o).data().style).collect();
    assert_eq!(
        styles,
        vec![S::XyzToLuv, S::LuvToXyz, S::LuvToXyz, S::XyzToLuv]
    );

    // Parameter validation happens when building the ops.
    let t = FixedFunctionTransform::new(FixedFunctionStyle::LinToPq, &[1.0]);
    assert_eq!(
        err_msg(t.build_ops(&mut ops, &config, &context, TransformDirection::Forward)),
        "The style 'Lin_TO_PQ' must have zero parameters but 1 found."
    );
}

mod cpu_tests;
