//! Tests ported from `Lut1DOpData_tests.cpp`, `Lut1DOp_tests.cpp`,
//! `Lut1DOpCPU_tests.cpp` and `Lut1DTransform_tests.cpp` (CPU path with 32f
//! pixels: the integer / half pixel formats of the OCIO renderers are
//! emulated by scaling the inputs and quantizing the outputs).

#![allow(clippy::excessive_precision, clippy::needless_range_loop)]

use super::*;
use crate::processor::{optimize_ops, Processor};
use crate::types::{METADATA_DESCRIPTION, METADATA_ID, METADATA_NAME};

fn assert_close(a: f32, b: f32, tol: f32) {
    assert!((a - b).abs() <= tol, "{a} != {b} (tolerance {tol})");
}

fn assert_err_contains<T: fmt::Debug>(r: Result<T>, what: &str) {
    match r {
        Ok(v) => panic!("expected an error containing '{what}', got {v:?}"),
        Err(e) => assert!(
            e.message().contains(what),
            "'{}' does not contain '{what}'",
            e.message()
        ),
    }
}

/// Render pixels through a (cloned and finalized) LUT.
fn render(lut: &Lut1DOpData, pixels: &mut [Pixel]) {
    Lut1DOp::new(lut.clone()).unwrap().apply(pixels);
}

fn to_pixels(v: &[f32]) -> Vec<Pixel> {
    v.chunks_exact(4)
        .map(|c| [c[0], c[1], c[2], c[3]])
        .collect()
}

fn flat(p: &[Pixel]) -> Vec<f32> {
    p.iter().flat_map(|x| x.iter().copied()).collect()
}

/// Quantize a normalized value as OCIO's `Converter<UINTn>::CastValue`.
fn quantize(v: f32, max: f32) -> u32 {
    clamp_ocio(v * max + 0.5, 0.0, max) as u32
}

fn fast_from_inverse(inv: &mut Lut1DOpData) -> Lut1DOpData {
    inv.validate().unwrap();
    inv.finalize().unwrap();
    make_fast_lut1d_from_inverse(inv).unwrap()
}

/// ULP comparison (port of `FloatsDiffer` without denorm compression).
fn floats_differ(expected: f32, actual: f32, tolerance: u32) -> bool {
    if expected.is_nan() || actual.is_nan() {
        return expected.is_nan() != actual.is_nan();
    }
    fn ord(f: f32) -> u32 {
        let b = f.to_bits();
        if b < 0x8000_0000 {
            0x8000_0000 + b
        } else {
            0x8000_0000 - (b & 0x7FFF_FFFF)
        }
    }
    ord(expected).abs_diff(ord(actual)) > tolerance
}

fn create_square_lut() -> Lut1DOpData {
    // Make a LUT that squares the input.
    const SIZE: usize = 256;
    let mut lut = Lut1DOpData::new(SIZE).unwrap();
    for i in 0..SIZE {
        let x = i as f32 / (SIZE - 1) as f32;
        for c in 0..3 {
            lut.array_mut()[c + i * 3] = x * x;
        }
    }
    lut
}

fn set_values(lut: &mut Lut1DOpData, values: &[f32]) {
    lut.array_mut().values_mut()[..values.len()].copy_from_slice(values);
}

/// Set the three channels of each entry to `values[i]`.
fn set_gray_values(lut: &mut Lut1DOpData, values: &[f32]) {
    for (i, v) in values.iter().enumerate() {
        for c in 0..3 {
            lut.array_mut()[i * 3 + c] = *v;
        }
    }
}

// ---------------------------------------------------------------------------
// Lut1DOpData

#[test]
fn opdata_get_lut_ideal_size() {
    assert_eq!(
        Lut1DOpData::get_lut_ideal_size(BitDepth::UInt8).unwrap(),
        256
    );
    assert_eq!(
        Lut1DOpData::get_lut_ideal_size(BitDepth::UInt16).unwrap(),
        65536
    );
    assert_eq!(
        Lut1DOpData::get_lut_ideal_size(BitDepth::F16).unwrap(),
        65536
    );
    assert_eq!(
        Lut1DOpData::get_lut_ideal_size(BitDepth::F32).unwrap(),
        65536
    );
    assert_err_contains(
        Lut1DOpData::get_lut_ideal_size(BitDepth::UInt32),
        "Bit-depth is not supported",
    );
}

#[test]
fn opdata_constructor() {
    let lut = Lut1DOpData::new(2).unwrap();
    assert!(!lut.is_no_op());
    assert!(lut.is_identity());
    assert_eq!(lut.array().length(), 2);
    assert_eq!(lut.interpolation(), Interpolation::Default);
    assert!(lut.validate().is_ok());

    assert_err_contains(Lut1DOpData::new(0), "at least 2");
    assert_err_contains(Lut1DOpData::new(1), "at least 2");
}

#[test]
fn opdata_accessors() {
    let mut l = Lut1DOpData::new(17).unwrap();
    l.set_interpolation(Interpolation::Linear);

    assert_eq!(l.interpolation(), Interpolation::Linear);
    assert!(!l.is_no_op());
    assert!(l.is_identity());
    assert!(l.validate().is_ok());

    assert_eq!(l.hue_adjust(), Lut1DHueAdjust::None);
    l.set_hue_adjust(Lut1DHueAdjust::Dw3).unwrap();
    assert_eq!(l.hue_adjust(), Lut1DHueAdjust::Dw3);

    // Note: Hue adjust does not affect identity status.
    assert!(l.is_identity());
    l.finalize().unwrap();
    assert_eq!(l.array().num_color_components(), 1);

    // Restore the number of components.
    l.array_mut().set_num_color_components(3);
    l.array_mut()[1] = 1.0;
    assert!(!l.is_no_op());
    assert!(!l.is_identity());
    assert!(l.validate().is_ok());

    l.set_interpolation(Interpolation::Best);
    assert_eq!(l.interpolation(), Interpolation::Best);

    assert_eq!(l.array().length(), 17);
    assert_eq!(l.array().num_values(), 17 * 3);
    assert_eq!(l.array().num_color_components(), 3);

    assert_err_contains(l.array_mut().resize(0, 3), "at least 2");
    assert_err_contains(l.array_mut().resize(1, 3), "at least 2");

    l.array_mut().resize(65, 3).unwrap();
    assert_eq!(l.array().length(), 65);
    assert_eq!(l.array().num_values(), 65 * 3);
    assert_eq!(l.array().num_color_components(), 3);
    assert!(l.validate().is_ok());

    l.finalize().unwrap();
    assert_eq!(l.array().num_color_components(), 3);

    // Restore value.
    l.array_mut()[1] = 0.0;
    l.finalize().unwrap();
    // Finalize sets the number of color components to 1 if the three
    // channels are equal.
    assert_eq!(l.array().num_color_components(), 1);

    // Number of components using NaN.
    l.array_mut().set_num_color_components(3);
    l.array_mut()[0] = f32::NAN;
    l.array_mut()[1] = f32::NAN;
    l.array_mut()[2] = 0.0;
    l.finalize().unwrap();
    assert_eq!(l.array().num_color_components(), 3);

    l.array_mut()[2] = f32::NAN;
    l.finalize().unwrap();
    assert_eq!(l.array().num_color_components(), 1);
}

#[test]
fn opdata_is_identity() {
    let mut l1 = Lut1DOpData::new_with_flags(HalfFlags::STANDARD, 1024, false).unwrap();
    assert!(l1.is_identity());

    // The tolerance is 1e-5.
    let last_id = l1.array().values().len() - 1;
    let first = l1.array()[0];
    let last = l1.array()[last_id];

    l1.array_mut()[0] = first + 0.9e-5;
    l1.array_mut()[last_id] = last + 0.9e-5;
    assert!(l1.is_identity());

    l1.array_mut()[0] = first + 1.1e-5;
    l1.array_mut()[last_id] = last;
    assert!(!l1.is_identity());

    l1.array_mut()[0] = first;
    l1.array_mut()[last_id] = last + 1.1e-5;
    assert!(!l1.is_identity());

    let mut l2 = Lut1DOpData::new_with_flags(HalfFlags::INPUT_HALF_CODE, 65536, false).unwrap();
    let id2 = 31700 * 3;
    let first2 = l2.array()[0];
    let last2 = l2.array()[id2];

    // (float)half(1) - (float)half(0)
    const ERROR_0: f32 = 5.960_464_5e-8;
    // (float)half(31701) - (float)half(31700)
    const ERROR_31700: f32 = 32.0;

    assert!(l2.is_identity());

    l2.array_mut()[0] = first2 + ERROR_0;
    l2.array_mut()[id2] = last2 + ERROR_31700;
    assert!(l2.is_identity());

    l2.array_mut()[0] = first2 + 2.0 * ERROR_0;
    l2.array_mut()[id2] = last2;
    assert!(!l2.is_identity());

    l2.array_mut()[0] = first2;
    l2.array_mut()[id2] = last2 + 2.0 * ERROR_31700;
    assert!(!l2.is_identity());
}

#[test]
fn opdata_clone() {
    let mut r = Lut1DOpData::new(20).unwrap();
    r.array_mut()[1] = 0.5;
    r.set_hue_adjust(Lut1DHueAdjust::Dw3).unwrap();

    let c = r.clone();
    assert!(!c.is_no_op());
    assert!(!c.is_identity());
    assert!(c.validate().is_ok());
    assert!(c.array() == r.array());
    assert_eq!(c.hue_adjust(), Lut1DHueAdjust::Dw3);
}

#[test]
fn opdata_equality() {
    let mut l1 = Lut1DOpData::new_with_flags(HalfFlags::STANDARD, 1024, false).unwrap();
    let mut l2 = Lut1DOpData::new_with_flags(HalfFlags::STANDARD, 1024, false).unwrap();
    l2.set_interpolation(Interpolation::Nearest);

    // LUT 1D only implements 1 style of interpolation.
    assert!(l1 == l2);

    let l3 = Lut1DOpData::new_with_flags(HalfFlags::STANDARD, 65536, false).unwrap();
    assert!(!(l1 == l3) && !(l3 == l2));

    let mut l4 = Lut1DOpData::new_with_flags(HalfFlags::STANDARD, 1024, false).unwrap();
    assert!(l1 == l4);

    l1.set_hue_adjust(Lut1DHueAdjust::Dw3).unwrap();
    assert!(!(l1 == l4));

    l4.set_hue_adjust(Lut1DHueAdjust::Dw3).unwrap();
    assert!(l1 == l4);

    let l5 = l1.inverse();
    let l6 = l4.inverse();
    assert!(l5 == l6);
}

#[test]
fn opdata_channel() {
    let mut l1 = Lut1DOpData::new_with_flags(HalfFlags::STANDARD, 17, false).unwrap();
    let l2 = Lut1DOpData::new_with_flags(HalfFlags::STANDARD, 20, false).unwrap();

    // False: identity.
    assert!(!l1.has_channel_crosstalk());
    assert!(l1.may_compose(&l2));

    l1.set_hue_adjust(Lut1DHueAdjust::Dw3).unwrap();
    // True: hue restore is on (identity is not tested for efficiency).
    assert!(l1.has_channel_crosstalk());
    assert!(!l1.may_compose(&l2));
    assert!(!l2.may_compose(&l1));

    l1.set_hue_adjust(Lut1DHueAdjust::None).unwrap();
    l1.array_mut()[1] = 3.0;
    // False: non-identity.
    assert!(!l1.has_channel_crosstalk());

    l1.set_hue_adjust(Lut1DHueAdjust::Dw3).unwrap();
    // True: non-identity with hue restore.
    assert!(l1.has_channel_crosstalk());
}

#[test]
fn opdata_interpolation() {
    let mut l = Lut1DOpData::new(17).unwrap();

    l.set_interpolation(Interpolation::Linear);
    assert_eq!(l.interpolation(), Interpolation::Linear);
    assert_eq!(l.concrete_interpolation(), Interpolation::Linear);
    assert!(l.validate().is_ok());

    l.set_interpolation(Interpolation::Best);
    assert_eq!(l.interpolation(), Interpolation::Best);
    assert_eq!(l.concrete_interpolation(), Interpolation::Linear);
    assert!(l.validate().is_ok());

    l.set_interpolation(Interpolation::Cubic);
    assert_eq!(l.interpolation(), Interpolation::Cubic);
    assert_eq!(l.concrete_interpolation(), Interpolation::Linear);
    assert_err_contains(l.validate(), "does not support interpolation algorithm");

    l.set_interpolation(Interpolation::Default);
    assert_eq!(l.concrete_interpolation(), Interpolation::Linear);
    assert!(l.validate().is_ok());

    // Nearest is implemented as linear.
    l.set_interpolation(Interpolation::Nearest);
    assert_eq!(l.interpolation(), Interpolation::Nearest);
    assert_eq!(l.concrete_interpolation(), Interpolation::Linear);
    assert!(l.validate().is_ok());

    l.set_interpolation(Interpolation::Unknown);
    assert_eq!(l.interpolation(), Interpolation::Unknown);
    assert_eq!(l.concrete_interpolation(), Interpolation::Linear);
    assert_err_contains(l.validate(), "does not support interpolation algorithm");

    l.set_interpolation(Interpolation::Tetrahedral);
    assert_eq!(l.concrete_interpolation(), Interpolation::Linear);
    assert_err_contains(
        l.validate(),
        " does not support interpolation algorithm: tetrahedral.",
    );
}

#[test]
fn opdata_lut_1d_compose() {
    let mut lut1 = Lut1DOpData::new(10).unwrap();
    lut1.format_metadata_mut()
        .add_attribute(METADATA_ID, "lut1");
    lut1.format_metadata_mut()
        .add_child_element(METADATA_DESCRIPTION, "description of 'lut1'");
    lut1.array_mut().resize(8, 3).unwrap();
    #[rustfmt::skip]
    set_values(&mut lut1, &[
        0.0,      0.0,      0.002333,
        0.0,      0.291341, 0.015624,
        0.106521, 0.334331, 0.462431,
        0.515851, 0.474151, 0.624611,
        0.658791, 0.527381, 0.685071,
        0.908501, 0.707951, 0.886331,
        0.926671, 0.846431, 1.0,
        1.0,      1.0,      1.0,
    ]);

    let mut lut2 = Lut1DOpData::new(10).unwrap();
    lut2.format_metadata_mut()
        .add_attribute(METADATA_ID, "lut2");
    lut2.format_metadata_mut()
        .add_child_element(METADATA_DESCRIPTION, "description of 'lut2'");
    lut2.array_mut().resize(8, 3).unwrap();
    #[rustfmt::skip]
    set_values(&mut lut2, &[
        0.0,        0.0,       0.0023303,
        0.0,        0.0029134, 0.015624,
        0.00010081, 0.0059806, 0.023362,
        0.0045628,  0.024229,  0.05822,
        0.0082598,  0.033831,  0.074063,
        0.028595,   0.075003,  0.13552,
        0.69154,    0.9213,    1.0,
        0.76038,    1.0,       1.0,
    ]);

    {
        let result = Lut1DOpData::compose(&lut1, &lut2, ComposeMethod::ResampleNo).unwrap();

        let md = result.format_metadata();
        assert_eq!(md.attributes.len(), 1);
        assert_eq!(md.attributes[0].0, METADATA_ID);
        assert_eq!(md.attributes[0].1, "lut1 + lut2");
        assert_eq!(md.children.len(), 2);
        assert_eq!(md.children[0].element_name(), METADATA_DESCRIPTION);
        assert_eq!(md.children[0].element_value(), "description of 'lut1'");
        assert_eq!(md.children[1].element_name(), METADATA_DESCRIPTION);
        assert_eq!(md.children[1].element_value(), "description of 'lut2'");

        let v = result.array().values();
        assert_eq!(result.array().length(), 8);
        #[rustfmt::skip]
        let expected = [
            0.0, 0.0, 0.00254739914,
            0.0, 0.00669934973, 0.00378420483,
            0.0, 0.0121908365, 0.0619750582,
            0.00682150759, 0.0272925831, 0.096942015,
            0.0206955168, 0.0308703855, 0.12295182,
            0.716288447, 0.0731772855, 1.0,
            0.725044191, 0.857842028, 1.0,
        ];
        for (i, e) in expected.iter().enumerate() {
            assert_close(v[i], *e, 1e-6);
        }
    }
    {
        let result = Lut1DOpData::compose(&lut1, &lut2, ComposeMethod::ResampleBig).unwrap();
        let v = result.array().values();
        assert_eq!(result.array().length(), 65536);
        let expected = [
            (0, 0.0),
            (1, 0.0),
            (2, 0.00254739914),
            (3, 0.0),
            (4, 6.34463504e-07),
            (5, 0.00254753046),
            (6, 0.0),
            (7, 1.26915984e-06),
            (8, 0.00254766271),
            (9, 0.0),
            (10, 1.90362334e-06),
            (11, 0.00254779495),
            (12, 0.0),
            (13, 2.53855251e-06),
            (14, 0.0025479272),
            (15, 0.0),
            (16, 3.17324884e-06),
            (17, 0.00254805945),
            (300, 0.0),
            (301, 6.3463347e-05),
            (302, 0.00256060902),
            (900, 0.0),
            (901, 0.000190390972),
            (902, 0.00258703064),
            (2700, 0.0),
            (2701, 0.000571172219),
            (2702, 0.00266629551),
        ];
        for (i, e) in expected {
            assert_close(v[i], e, 1e-6);
        }
    }
}

#[test]
fn opdata_lut_1d_compose_sc() {
    let mut lut1 = Lut1DOpData::new(2).unwrap();
    lut1.array_mut().resize(2, 3).unwrap();
    set_values(&mut lut1, &[64.0, 64.0, 64.0, 196.0, 196.0, 196.0]);
    lut1.scale(1.0 / 255.0);

    let mut lut2 = Lut1DOpData::new(2).unwrap();
    lut2.array_mut().resize(32, 3).unwrap();
    #[rustfmt::skip]
    set_values(&mut lut2, &[
        0.0000000, 0.0000000, 0.0023303,
        0.0000000, 0.0001869, 0.0052544,
        0.0000000, 0.0010572, 0.0096338,
        0.0000000, 0.0029134, 0.0156240,
        0.0001008, 0.0059806, 0.0233620,
        0.0007034, 0.0104480, 0.0329680,
        0.0021120, 0.0164810, 0.0445540,
        0.0045628, 0.0242290, 0.0582200,
        0.0082598, 0.0338310, 0.0740630,
        0.0133870, 0.0454150, 0.0921710,
        0.0201130, 0.0591010, 0.1126300,
        0.0285950, 0.0750030, 0.1355200,
        0.0389830, 0.0932290, 0.1609100,
        0.0514180, 0.1138800, 0.1888800,
        0.0660340, 0.1370600, 0.2195000,
        0.0829620, 0.1628600, 0.2528300,
        0.1023300, 0.1913800, 0.2889500,
        0.1242500, 0.2227000, 0.3279000,
        0.1488500, 0.2569100, 0.3697600,
        0.1762300, 0.2940900, 0.4145900,
        0.2065200, 0.3343300, 0.4624300,
        0.2398200, 0.3777000, 0.5133400,
        0.2762200, 0.4242800, 0.5673900,
        0.3158500, 0.4741500, 0.6246100,
        0.3587900, 0.5273800, 0.6850700,
        0.4051500, 0.5840400, 0.7488100,
        0.4550200, 0.6442100, 0.8158800,
        0.5085000, 0.7079500, 0.8863300,
        0.5656900, 0.7753400, 0.9602100,
        0.6266700, 0.8464300, 1.0000000,
        0.6915400, 0.9213000, 1.0000000,
        0.7603800, 1.0000000, 1.0000000,
    ]);

    {
        let c = Lut1DOpData::compose(&lut1, &lut2, ComposeMethod::ResampleNo).unwrap();
        assert_eq!(c.array().length(), 2);
        let v = c.array().values();
        let e = [
            0.00744791, 0.03172233, 0.07058375, 0.3513808, 0.51819527, 0.67463773,
        ];
        for i in 0..6 {
            assert_close(v[i], e[i], 1e-6);
        }
    }
    {
        let c = Lut1DOpData::compose(&lut1, &lut2, ComposeMethod::ResampleBig).unwrap();
        assert_eq!(c.array().length(), 65536);
        let v = c.array().values();
        let e = [
            (0, 0.00744791),
            (1, 0.03172233),
            (2, 0.07058375),
            (98688, 0.0991418),
            (98689, 0.1866853),
            (98690, 0.2830042),
            (196605, 0.3513808),
            (196606, 0.51819527),
            (196607, 0.67463773),
        ];
        for (i, x) in e {
            assert_close(v[i], x, 1e-6);
        }
    }
}

#[test]
fn opdata_compose_half_domain_method() {
    let mut lut = Lut1DOpData::new(10).unwrap();
    lut.array_mut()[0] = 0.1;
    let id = Lut1DOpData::new(10).unwrap();
    let c = Lut1DOpData::compose(&lut, &id, ComposeMethod::ResampleHd).unwrap();
    assert!(c.is_input_half_domain());
    assert_eq!(c.array().length(), 65536);
    assert_close(c.array()[0], 0.1, 1e-6);
}

#[test]
fn opdata_inverse_hueadjust() {
    let mut r = Lut1DOpData::new_with_flags(HalfFlags::STANDARD, 65536, false).unwrap();
    r.format_metadata_mut().add_attribute(METADATA_ID, "uid");
    r.set_hue_adjust(Lut1DHueAdjust::Dw3).unwrap();
    let inv = r.inverse();
    assert_eq!(inv.hue_adjust(), Lut1DHueAdjust::Dw3);
}

#[test]
fn opdata_is_inverse() {
    let mut l1 = Lut1DOpData::new_with_flags(HalfFlags::STANDARD, 5, false).unwrap();
    l1.format_metadata_mut().add_attribute(METADATA_ID, "uid");
    // Make it not an identity.
    l1.array_mut().values_mut()[0] = 20.0;
    assert!(!l1.is_identity());

    // Create an inverse LUT with same basics.
    let l2 = l1.inverse();
    assert!(!(l1 == l2));
    assert!(l1.is_inverse(&l2));
    assert!(l2.is_inverse(&l1));
}

fn set_lut_array(op: &mut Lut1DOpData, dimension: usize, channels: usize, data: &[f32]) {
    op.array_mut().resize(dimension, channels).unwrap();
    let values = op.array_mut().values_mut();
    if channels == 3 {
        values[..dimension * 3].copy_from_slice(&data[..dimension * 3]);
    } else {
        // Set the red component, fill the others with zeros.
        for i in 0..dimension {
            values[i * 3] = data[i];
            values[i * 3 + 1] = 0.0;
            values[i * 3 + 2] = 0.0;
        }
    }
}

fn check_inverse_increasing_effective_domain(
    dimension: usize,
    channels: usize,
    fwd: &[f32],
    exp: [(bool, usize, usize); 3],
) {
    let mut op = Lut1DOpData::new_with_flags(HalfFlags::STANDARD, 1024, false).unwrap();
    op.format_metadata_mut().add_attribute(METADATA_ID, "uid");
    set_lut_array(&mut op, dimension, channels, fwd);
    op.set_direction(TransformDirection::Inverse);
    op.validate().unwrap();
    op.finalize().unwrap();

    let props = [
        op.red_properties(),
        op.green_properties(),
        op.blue_properties(),
    ];
    for (p, e) in props.iter().zip(exp) {
        assert_eq!(p.is_increasing, e.0);
        assert_eq!(p.start_domain, e.1);
        assert_eq!(p.end_domain, e.2);
    }
}

#[test]
fn opdata_inverse_increasing_effective_domain() {
    #[rustfmt::skip]
    let fwd = [
        0.1, 0.8, 0.1, // 0
        0.1, 0.7, 0.1,
        0.1, 0.6, 0.1, // 2
        0.2, 0.5, 0.1, // 3
        0.3, 0.4, 0.2,
        0.4, 0.3, 0.3,
        0.5, 0.1, 0.4, // 6
        0.6, 0.1, 0.5, // 7
        0.7, 0.1, 0.5,
        0.8, 0.1, 0.5, // 9
    ];
    check_inverse_increasing_effective_domain(
        10,
        3,
        &fwd,
        [
            (true, 2, 9),  // increasing, flat [0, 2]
            (false, 0, 6), // decreasing, flat [6, 9]
            (true, 3, 7),  // increasing, flat [0, 3] and [7, 9]
        ],
    );

    let fwd = [0.3, 0.3, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.8, 0.8];
    check_inverse_increasing_effective_domain(10, 1, &fwd, [(true, 2, 7); 3]);

    let fwd = [0.5; 10];
    check_inverse_increasing_effective_domain(10, 1, &fwd, [(false, 0, 0); 3]);

    let fwd = [0.8, 0.9, 0.8, 0.5, 0.4, 0.3, 0.2, 0.1, 0.1, 0.2];
    check_inverse_increasing_effective_domain(10, 1, &fwd, [(false, 2, 7); 3]);
}

#[test]
fn opdata_inverse_flatten() {
    #[rustfmt::skip]
    let fwd = [
        0.10, 0.90, 0.25, // 0
        0.20, 0.80, 0.30,
        0.30, 0.70, 0.40,
        0.40, 0.60, 0.50,
        0.35, 0.50, 0.60, // 4
        0.30, 0.55, 0.50, // 5
        0.45, 0.60, 0.40, // 6
        0.50, 0.65, 0.30, // 7
        0.60, 0.45, 0.20, // 8
        0.70, 0.50, 0.10, // 9
    ];
    // red is increasing, with a reversal [4, 5]
    // green is decreasing, with reversals [4, 5] and [9]
    // blue is decreasing, with reversals [0, 8]
    #[rustfmt::skip]
    let exp = [
        0.10, 0.90, 0.25,
        0.20, 0.80, 0.25,
        0.30, 0.70, 0.25,
        0.40, 0.60, 0.25,
        0.40, 0.50, 0.25,
        0.40, 0.50, 0.25,
        0.45, 0.50, 0.25,
        0.50, 0.50, 0.25,
        0.60, 0.45, 0.20,
        0.70, 0.45, 0.10,
    ];
    let mut op = Lut1DOpData::new_with_flags(HalfFlags::STANDARD, 65536, false).unwrap();
    set_lut_array(&mut op, 10, 3, &fwd);
    op.set_direction(TransformDirection::Inverse);
    op.validate().unwrap();
    op.finalize().unwrap();
    assert_eq!(&op.array().values()[..30], &exp[..]);
}

fn set_lut_array_half(op: &mut Lut1DOpData, channels: usize) {
    op.array_mut().resize(65536, channels).unwrap();
    let values = op.array_mut().values_mut();
    for j in 0..channels {
        for i in 0..65536usize {
            let mut f = f16::from_bits(i as u16).to_f32();
            if j == 0 {
                // Negative domain overlaps positive with a reversal.
                f = if i < 32768 {
                    2.0 * f - 0.1
                } else {
                    3.0 * f + 0.1
                };
                if (25000..32760).contains(&i) {
                    f = 10000.0; // flat spot at positive end
                }
                if i >= 60000 {
                    f = -10000.0; // flat spot at neg end
                }
                if i > 15000 && i < 20000 {
                    f = 0.5; // reversal in positive side
                }
                if i > 50000 && i < 55000 {
                    f = -2.0; // reversal in negative side
                }
            } else if j == 1 {
                // Decreasing function, gap between pos & neg at zero.
                f = if i < 32768 {
                    -0.5 * f + 0.02
                } else {
                    -0.4 * f + 0.05
                };
                if (25000..32760).contains(&i) {
                    f = -400.0;
                }
                if i >= 60000 {
                    f = 2000.0;
                }
                if i > 15000 && i < 20000 {
                    f = -0.1;
                }
                if i > 50000 && i < 55000 {
                    f = 1.4;
                }
            } else {
                f = if i < 32768 {
                    f.powf(1.5)
                } else {
                    -(-f).powf(0.9)
                };
                if i <= 11878 || (32768..=44646).contains(&i) {
                    f = -0.01; // flat spot around zero
                }
            }
            values[i * 3 + j] = f;
        }
    }
}

#[test]
fn opdata_inverse_half_domain() {
    let mut op = Lut1DOpData::new_with_flags(HalfFlags::INPUT_HALF_CODE, 65536, false).unwrap();
    op.format_metadata_mut().add_attribute(METADATA_ID, "uid");
    set_lut_array_half(&mut op, 3);
    op.set_direction(TransformDirection::Inverse);
    op.validate().unwrap();
    op.finalize().unwrap();

    let r = *op.red_properties();
    let g = *op.green_properties();
    let b = *op.blue_properties();
    let inv = op.array().values();

    assert!(r.is_increasing);
    assert_eq!(r.start_domain, 0);
    assert_eq!(r.end_domain, 25000);
    assert_eq!(r.neg_start_domain, 44100); // -0.2/3 (flattened to remove overlap)
    assert_eq!(r.neg_end_domain, 60000);

    assert!(!g.is_increasing);
    assert_eq!(g.start_domain, 0);
    assert_eq!(g.end_domain, 25000);
    assert_eq!(g.neg_start_domain, 32768);
    assert_eq!(g.neg_end_domain, 60000);

    assert!(b.is_increasing);
    assert_eq!(b.start_domain, 11878);
    assert_eq!(b.end_domain, 31743);
    assert_eq!(b.neg_start_domain, 44646);
    assert_eq!(b.neg_end_domain, 64511);

    // Check reversals are removed.
    assert_eq!(f16::from_f32(inv[16000 * 3]).to_bits(), 15922);
    assert_eq!(f16::from_f32(inv[52000 * 3]).to_bits(), 51567);
    assert_eq!(f16::from_f32(inv[16000 * 3 + 1]).to_bits(), 46662);
    assert_eq!(f16::from_f32(inv[52000 * 3 + 1]).to_bits(), 15885);

    assert!((1..31745).all(|i| inv[i * 3] >= inv[(i - 1) * 3])); // increasing red
    assert!(inv[0] >= inv[32768 * 3]); // no overlap at +0 and -0
    assert!((1..31745).all(|i| inv[i * 3 + 1] <= inv[(i - 1) * 3 + 1])); // decreasing green
    assert!(inv[1] <= inv[32768 * 3 + 1]);
    assert!((1..31745).all(|i| inv[i * 3 + 2] >= inv[(i - 1) * 3 + 2])); // increasing blue
    assert!(inv[2] >= inv[32768 * 3 + 2]);
}

#[test]
fn opdata_make_fast_from_inverse_extended_domain() {
    // Synthetic version of make_fast_from_inverse_gpu_extented_domain: the
    // LUT has values outside [0,1], so the fast LUT needs a half domain even
    // with a 10i file depth.
    let mut lut = Lut1DOpData::new(32).unwrap();
    for v in lut.array_mut().values_mut().iter_mut() {
        *v = *v * 1.2 - 0.1;
    }
    lut.set_file_output_bit_depth(BitDepth::UInt10);
    let mut inv = lut.inverse();
    inv.finalize().unwrap();
    let fast = make_fast_lut1d_from_inverse(&inv).unwrap();
    assert_eq!(fast.array().length(), 65536);
    assert!(fast.is_input_half_domain());
    assert_eq!(fast.direction(), TransformDirection::Forward);

    // Not an inverse LUT.
    assert_err_contains(
        make_fast_lut1d_from_inverse(&lut),
        "expects an inverse 1D LUT",
    );
}

#[test]
fn opdata_make_fast_from_inverse_f32_and_int_depths() {
    // 32f file depth: half domain.
    let mut lut = Lut1DOpData::new(17).unwrap();
    for v in lut.array_mut().values_mut().iter_mut() {
        *v = v.sqrt();
    }
    lut.set_file_output_bit_depth(BitDepth::F32);
    let fast = make_fast_lut1d_from_inverse(&lut.inverse()).unwrap();
    assert_eq!(fast.array().length(), 65536);
    assert!(fast.is_input_half_domain());

    // Unknown depth: 12i look-up domain.
    lut.set_file_output_bit_depth(BitDepth::Unknown);
    let fast = make_fast_lut1d_from_inverse(&lut.inverse()).unwrap();
    assert_eq!(fast.array().length(), 4096);
    assert!(!fast.is_input_half_domain());

    // 8i depth: 8i look-up domain.
    lut.set_file_output_bit_depth(BitDepth::UInt8);
    let fast = make_fast_lut1d_from_inverse(&lut.inverse()).unwrap();
    assert_eq!(fast.array().length(), 256);
}

#[test]
fn opdata_make_fast_from_inverse_half_domain() {
    // Source LUT has an extended range, so the fast LUT has a half domain.
    let mut lut = Lut1DOpData::new_with_flags(HalfFlags::INPUT_HALF_CODE, 65536, true).unwrap();
    for v in lut.array_mut().values_mut().iter_mut() {
        *v *= 2.0;
    }
    let mut inv = lut.inverse();
    inv.finalize().unwrap();
    let fast = make_fast_lut1d_from_inverse(&inv).unwrap();
    assert_eq!(fast.array().length(), 65536);
    assert!(fast.is_input_half_domain());
}

#[test]
fn opdata_compose_inverse_luts() {
    let lut_ref = Lut1DOpData::new(17).unwrap();
    let mut lut = Lut1DOpData::new(17).unwrap();
    for v in lut.array_mut().values_mut().iter_mut() {
        *v *= *v;
    }

    let lut_fwd1 = lut.clone();
    let lut_fwd2 = lut_fwd1.clone();

    // Forward + forward.
    let comp_fwd_fwd =
        Lut1DOpData::compose(&lut_fwd1, &lut_fwd2, ComposeMethod::ResampleNo).unwrap();
    assert_eq!(comp_fwd_fwd.direction(), TransformDirection::Forward);

    // Inverse + inverse.
    let mut lut_inv1 = lut.inverse();
    lut_inv1.finalize().unwrap();
    let mut lut_inv2 = lut.inverse();
    lut_inv2.finalize().unwrap();
    let comp_inv_inv =
        Lut1DOpData::compose(&lut_inv1, &lut_inv2, ComposeMethod::ResampleNo).unwrap();
    assert_eq!(comp_inv_inv.direction(), TransformDirection::Inverse);
    assert_eq!(comp_fwd_fwd.array().values(), comp_inv_inv.array().values());

    // Forward + inverse.
    let comp_fwd_inv =
        Lut1DOpData::compose(&lut_fwd1, &lut_inv1, ComposeMethod::ResampleNo).unwrap();
    assert_eq!(comp_fwd_inv.direction(), TransformDirection::Forward);
    assert_eq!(comp_fwd_inv.array().values(), lut_ref.array().values());

    // Inverse + forward.
    let comp_inv_fwd =
        Lut1DOpData::compose(&lut_inv1, &lut_fwd1, ComposeMethod::ResampleNo).unwrap();
    assert_eq!(comp_inv_fwd.direction(), TransformDirection::Forward);
    assert!(comp_inv_fwd.is_input_half_domain());
    assert_eq!(comp_inv_fwd.array().length(), 65536);
    assert_close(comp_inv_fwd.array()[14336 * 3], 0.5, 1e-7);
}

#[test]
fn opdata_pair_identity_replacement() {
    let mut lut = Lut1DOpData::new(5).unwrap();
    set_gray_values(&mut lut, &[0.1, 0.1, 0.4, 0.9, 0.9]);
    let mut inv = lut.inverse();
    inv.finalize().unwrap();

    // Fwd -> Inv: clamp by the flat regions relative to [0,1].
    assert_eq!(
        lut.pair_identity_replacement(&inv),
        IdentityReplacement::Clamp {
            min: 0.25,
            max: 0.75
        }
    );
    // Inv -> Fwd: clamp to the output range of the forward LUT.
    assert_eq!(
        inv.pair_identity_replacement(&lut),
        IdentityReplacement::Clamp {
            min: 0.1f32 as f64,
            max: 0.9f32 as f64
        }
    );

    let half = Lut1DOpData::new_with_flags(HalfFlags::INPUT_HALF_CODE, 65536, false).unwrap();
    assert_eq!(
        half.pair_identity_replacement(&half.inverse()),
        IdentityReplacement::NoOp
    );
    assert_eq!(half.identity_replacement(), IdentityReplacement::NoOp);
    assert_eq!(
        lut.identity_replacement(),
        IdentityReplacement::Clamp { min: 0.0, max: 1.0 }
    );
}

#[test]
fn opdata_validate_errors() {
    let mut lut = Lut1DOpData::new(10).unwrap();
    lut.set_input_half_domain(true);
    assert_err_contains(
        lut.validate(),
        "10 entries found, 65536 required for halfDomain 1D LUT",
    );
    assert!(Lut1DOp::new(lut.clone()).is_err());
    lut.set_input_half_domain(false);
    assert!(lut.validate().is_ok());

    assert_err_contains(
        lut.set_hue_adjust(Lut1DHueAdjust::Wypn),
        "HUE_WYPN hue adjust style is not implemented",
    );

    lut.array_mut().values_mut().pop();
    assert_err_contains(
        lut.validate(),
        "1D LUT content array issue: Array contains: 29 values, but 30 are expected.",
    );
    assert!(Lut1DOp::new(lut).is_err());

    assert_err_contains(
        Lut1DArray::new(HalfFlags::STANDARD, 2, 10, false),
        "channels needs to be 1 or 3",
    );
    let mut a = Lut1DArray::new(HalfFlags::STANDARD, 3, 10, false).unwrap();
    assert_err_contains(
        a.resize(1024 * 1024 + 1, 3),
        "must not be greater than 1024x1024 (1048576)",
    );
}

#[test]
fn opdata_cache_id() {
    let mut l1 = Lut1DOpData::new(5).unwrap();
    let l2 = Lut1DOpData::new(5).unwrap();
    assert_eq!(l1.cache_id(), l2.cache_id());
    assert!(l1
        .cache_id()
        .ends_with("forward default standard domain none"));
    l1.format_metadata_mut().set_id("myid");
    assert!(l1.cache_id().starts_with("myid "));
    assert_ne!(l1.cache_id(), l1.inverse().cache_id());
    let h = Lut1DOpData::new_with_flags(HalfFlags::INPUT_HALF_CODE, 65536, false).unwrap();
    assert!(h.cache_id().contains("half domain"));
}

#[test]
fn opdata_half_domain_nan_filtering() {
    let unfiltered = Lut1DOpData::new_with_flags(HalfFlags::INPUT_HALF_CODE, 65536, false).unwrap();
    assert!(unfiltered.array()[32256 * 3].is_nan());
    let filtered = Lut1DOpData::make_lookup_domain(BitDepth::F16).unwrap();
    assert_eq!(filtered.array()[32256 * 3], 0.0);
    assert!(filtered.is_input_half_domain());
    let d10 = Lut1DOpData::make_lookup_domain(BitDepth::UInt10).unwrap();
    assert_eq!(d10.array().length(), 1024);
    assert!(d10.may_lookup(BitDepth::UInt10));
    assert!(!d10.may_lookup(BitDepth::UInt8));
    assert!(filtered.may_lookup(BitDepth::F16));
    assert!(!filtered.may_lookup(BitDepth::F32));
}

// ---------------------------------------------------------------------------
// Lut1DOp

#[test]
fn op_extrapolation_errors() {
    let mut lut = Lut1DOpData::new(3).unwrap();
    // Simple y = x + 0.1 LUT.
    for v in lut.array_mut().values_mut().iter_mut() {
        *v += 0.1;
    }
    assert!(!lut.is_no_op());

    let mut ops = OpVec::new();
    create_lut1d_op_from_data(&mut ops, &lut, TransformDirection::Forward).unwrap();

    #[rustfmt::skip]
    let mut px = to_pixels(&[
        -0.1,  -0.2,  -10.0, 0.0,
         0.5,   1.0,    1.1, 0.0,
        10.1,  55.0,    2.3, 0.0,
         9.1,  1.0e6, 1.0e9, 0.0,
        4.0e9, 9.5e7,   0.5, 0.0,
    ]);
    #[rustfmt::skip]
    let expected = [
        0.1, 0.1, 0.1, 0.0,
        0.6, 1.1, 1.1, 0.0,
        1.1, 1.1, 1.1, 0.0,
        1.1, 1.1, 1.1, 0.0,
        1.1, 1.1, 0.6, 0.0,
    ];
    ops[0].apply(&mut px);
    for (a, e) in flat(&px).iter().zip(expected) {
        assert_close(*a, e, 1e-5);
    }
}

fn lut_op(op: &OpRc) -> &Lut1DOp {
    op.downcast_ref::<Lut1DOp>().unwrap()
}

#[test]
fn op_inverse() {
    let mut luta = Lut1DOpData::new(3).unwrap();
    luta.array_mut()[0] = 0.1;
    let lutb = luta.clone();
    let mut lutc = luta.clone();
    lutc.array_mut()[0] = 0.2;

    let mut ops = OpVec::new();
    for l in [&luta, &lutb, &lutc] {
        create_lut1d_op_from_data(&mut ops, l, TransformDirection::Forward).unwrap();
        create_lut1d_op_from_data(&mut ops, l, TransformDirection::Inverse).unwrap();
    }
    assert_eq!(ops.len(), 6);

    let d = |i: usize| lut_op(&ops[i]).data();
    assert!(d(0).is_inverse(d(1)));
    assert!(d(2).is_inverse(d(3)));
    assert!(d(4).is_inverse(d(5)));

    assert!(!d(0).is_inverse(d(2)));
    assert!(d(0).is_inverse(d(3)));
    assert!(d(1).is_inverse(d(2)));
    assert!(!d(1).is_inverse(d(3)));

    assert!(!d(0).is_inverse(d(4)));
    assert!(!d(0).is_inverse(d(5)));
    assert!(!d(1).is_inverse(d(4)));
    assert!(!d(1).is_inverse(d(5)));

    let ids: Vec<String> = ops.iter().map(|o| o.cache_id()).collect();
    assert_eq!(ids[0], ids[2]);
    assert_eq!(ids[1], ids[3]);
    assert_ne!(ids[0], ids[4]);
    assert_ne!(ids[0], ids[5]);
    assert_ne!(ids[1], ids[4]);
    assert_ne!(ids[1], ids[5]);
    assert!(ids[0].starts_with("<Lut1D "));
}

#[test]
fn op_inverse_pairs_optimized_to_range() {
    let mut luta = Lut1DOpData::new(3).unwrap();
    luta.array_mut()[0] = 0.1;
    let lutb = luta.clone();
    let mut lutc = luta.clone();
    lutc.array_mut()[0] = 0.2;

    let mut ops = OpVec::new();
    for l in [&luta, &lutb, &lutc] {
        create_lut1d_op_from_data(&mut ops, l, TransformDirection::Forward).unwrap();
        create_lut1d_op_from_data(&mut ops, l, TransformDirection::Inverse).unwrap();
    }
    // Optimize removes the LUT forward and inverse pairs and replaces them
    // by a clamping range.
    let opt = optimize_ops(&ops, OptimizationFlags::DEFAULT);
    assert_eq!(opt.len(), 1);
    assert_eq!(opt[0].name(), "Range");
}

#[test]
fn op_half_domain_inverse_pair_removed() {
    // (NaN entries would prevent the arrays from comparing equal.)
    let mut lut = Lut1DOpData::new_with_flags(HalfFlags::INPUT_HALF_CODE, 65536, true).unwrap();
    lut.scale(1.5);
    let mut ops = OpVec::new();
    create_lut1d_op_from_data(&mut ops, &lut, TransformDirection::Forward).unwrap();
    create_lut1d_op_from_data(&mut ops, &lut, TransformDirection::Inverse).unwrap();
    let opt = optimize_ops(&ops, OptimizationFlags::DEFAULT);
    assert!(opt.is_empty());

    // Without the pair flag, the LUTs are composed.
    let flags =
        OptimizationFlags(OptimizationFlags::DEFAULT.0 & !OptimizationFlags::PAIR_IDENTITY_LUT1D.0);
    let opt = optimize_ops(&ops, flags);
    assert_eq!(opt.len(), 1);
    assert_eq!(opt[0].name(), "Lut1D");
}

#[test]
fn op_half_domain_identity_is_no_op() {
    let lut = Lut1DOpData::new_with_flags(HalfFlags::INPUT_HALF_CODE, 65536, false).unwrap();
    let mut ops = OpVec::new();
    create_lut1d_op_from_data(&mut ops, &lut, TransformDirection::Forward).unwrap();
    assert!(ops[0].is_no_op());
    assert!(ops[0].is_identity());
    assert!(optimize_ops(&ops, OptimizationFlags::NONE).is_empty());

    // A standard domain identity still clamps.
    let lut = Lut1DOpData::new(10).unwrap();
    let mut ops = OpVec::new();
    create_lut1d_op_from_data(&mut ops, &lut, TransformDirection::Forward).unwrap();
    assert!(!ops[0].is_no_op());
    assert!(!ops[0].is_identity());
    assert!(lut_op(&ops[0]).data().is_identity());
}

#[test]
fn op_replace_identity_luts() {
    let lut = Lut1DOpData::new(10).unwrap();
    let mut ops = OpVec::new();
    create_lut1d_op_from_data(&mut ops, &lut, TransformDirection::Forward).unwrap();
    assert_eq!(
        replace_identity_luts(&mut ops, OptimizationFlags::DEFAULT),
        1
    );
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].name(), "Range");
}

#[test]
fn op_finite_value() {
    let lut = create_square_lut();
    let mut ops = OpVec::new();
    create_lut1d_op_from_data(&mut ops, &lut, TransformDirection::Forward).unwrap();
    create_lut1d_op_from_data(&mut ops, &lut, TransformDirection::Inverse).unwrap();
    assert_eq!(ops.len(), 2);

    let mut px = [[0.5, 0.6, 0.7, 0.5]];
    ops[0].apply(&mut px);
    for (a, e) in px[0].iter().zip([0.25, 0.36, 0.49, 0.5]) {
        assert_close(*a, e, 1e-5);
    }

    let mut px = [[0.25, 0.36, 0.49, 0.5]];
    ops[1].apply(&mut px);
    for (a, e) in px[0].iter().zip([0.5, 0.6, 0.7, 0.5]) {
        assert_close(*a, e, 1e-5);
    }
}

#[test]
fn op_identity_lut_1d() {
    let mut data = vec![0.0f32; 3 * 2];
    generate_identity_lut1d(&mut data, 3, 2);
    assert_eq!(data, [0.0, 0.0, 0.5, 0.5, 1.0, 1.0]);

    let mut data = vec![0.0f32; 4 * 3];
    generate_identity_lut1d(&mut data, 4, 3);
    for c in 0..3 {
        assert_eq!(data[c], 0.0);
        assert_eq!(data[3 + c], 0.33333333);
        assert_eq!(data[6 + c], 0.66666667);
        assert_eq!(data[9 + c], 1.0);
    }

    let mut data = vec![0.0f32; 5 * 4];
    generate_linear_scale_lut1d(&mut data, 5, 4, -1.0, 1.0);
    assert_eq!(&data[..3], &[-1.0, -1.0, -1.0]);
    assert_eq!(data[3], 0.0);
    assert_eq!(&data[8..11], &[0.0, 0.0, 0.0]);
    assert_eq!(&data[16..19], &[1.0, 1.0, 1.0]);
}

#[test]
fn op_finite_value_hue_adjust() {
    // Make a LUT that squares the input.
    let mut lut_data = create_square_lut();
    lut_data.set_hue_adjust(Lut1DHueAdjust::Dw3).unwrap();
    let lut = Lut1DOp::new(lut_data.clone()).unwrap();
    lut_data.finalize().unwrap();
    assert!(!lut.data().is_identity());
    assert!(lut.has_channel_crosstalk());

    let mut px = [[0.5, 0.6, 0.7, 0.5]];
    lut.apply(&mut px);
    // (Hue adjust modifies green here.)
    for (a, e) in px[0].iter().zip([0.25, 0.37, 0.49, 0.5]) {
        assert_close(*a, e, 1e-5);
    }

    let inv_data = lut_data.inverse();
    let mut ops_fast = OpVec::new();
    let mut ops_exact = OpVec::new();
    create_lut1d_op_from_data(&mut ops_fast, &inv_data, TransformDirection::Forward).unwrap();
    create_lut1d_op_from_data(&mut ops_exact, &inv_data, TransformDirection::Forward).unwrap();

    assert_eq!(
        replace_inverse_luts(&mut ops_fast, OptimizationFlags::LUT_INV_FAST).unwrap(),
        1
    );
    assert_eq!(
        replace_inverse_luts(&mut ops_exact, OptimizationFlags::NONE).unwrap(),
        0
    );
    assert_eq!(ops_fast.len(), 1);
    assert_eq!(ops_exact.len(), 1);
    assert_eq!(
        lut_op(&ops_fast[0]).data().direction(),
        TransformDirection::Forward
    );
    assert_eq!(
        lut_op(&ops_exact[0]).data().direction(),
        TransformDirection::Inverse
    );

    let mut fast = [[0.25, 0.37, 0.49, 0.5]];
    let mut exact = [[0.25, 0.37, 0.49, 0.5]];
    ops_fast[0].apply(&mut fast);
    ops_exact[0].apply(&mut exact);
    for i in 0..4 {
        let e = [0.5, 0.6, 0.7, 0.5][i];
        assert_close(fast[0][i], e, 1e-5);
        assert_close(exact[0][i], e, 1e-5);
    }
}

#[test]
fn op_compose_only_forward() {
    let l1 = create_square_lut();
    let mut ops = OpVec::new();
    create_lut1d_op_from_data(&mut ops, &l1, TransformDirection::Forward).unwrap();
    create_lut1d_op_from_data(&mut ops, &l1, TransformDirection::Forward).unwrap();
    create_lut1d_op_from_data(&mut ops, &l1, TransformDirection::Inverse).unwrap();
    create_lut1d_op_from_data(&mut ops, &l1, TransformDirection::Inverse).unwrap();
    assert_eq!(ops.len(), 4);

    let flags = OptimizationFlags::COMP_LUT1D;
    // Forward + forward.
    assert!(ops[0].combine_with(ops[1].as_ref(), flags).is_some());
    // Inverse + inverse.
    assert!(ops[2].combine_with(ops[3].as_ref(), flags).is_some());
    // Forward + inverse.
    assert!(ops[0].combine_with(ops[3].as_ref(), flags).is_some());
    // Inverse + forward.
    assert!(ops[2].combine_with(ops[1].as_ref(), flags).is_some());
    // Not without the flag.
    assert!(ops[0]
        .combine_with(ops[1].as_ref(), OptimizationFlags::NONE)
        .is_none());
}

#[test]
fn op_compose_big_domain() {
    let mut lut1 = Lut1DOpData::new(10).unwrap();
    let lut2 = Lut1DOpData::new(10).unwrap();
    lut1.array_mut()[9 * 3] = 1.0001;

    let mut ops = OpVec::new();
    create_lut1d_op_from_data(&mut ops, &lut1, TransformDirection::Forward).unwrap();
    create_lut1d_op_from_data(&mut ops, &lut2, TransformDirection::Forward).unwrap();

    let combined = ops[0]
        .combine_with(ops[1].as_ref(), OptimizationFlags::COMP_LUT1D)
        .unwrap();
    assert_eq!(combined.len(), 1);
    let lut = lut_op(&combined[0]).data();
    assert_eq!(lut.array().length(), 65536);
    assert!(!lut.is_input_half_domain());
}

#[test]
fn op_hue_adjust_not_composed() {
    let mut l1 = create_square_lut();
    l1.set_hue_adjust(Lut1DHueAdjust::Dw3).unwrap();
    let mut ops = OpVec::new();
    create_lut1d_op_from_data(&mut ops, &l1, TransformDirection::Forward).unwrap();
    create_lut1d_op_from_data(&mut ops, &create_square_lut(), TransformDirection::Forward).unwrap();
    assert!(ops[0]
        .combine_with(ops[1].as_ref(), OptimizationFlags::ALL)
        .is_none());
}

#[test]
fn op_inverse_twice() {
    // Make a LUT that squares the input.
    let lut = create_square_lut();

    let mut ops = OpVec::new();
    create_lut1d_op_from_data(&mut ops, &lut, TransformDirection::Inverse).unwrap();
    assert_eq!(ops.len(), 1);

    let reference = [0.25, 0.36, 0.49, 0.5];
    let mut px = [reference];
    ops[0].apply(&mut px);
    for (a, e) in px[0].iter().zip([0.5, 0.6, 0.7, 0.5]) {
        assert_close(*a, e, 1e-5);
    }

    // Inverse the inverse.
    let lut_data = lut_op(&ops[0]).data().inverse();
    create_lut1d_op_from_data(&mut ops, &lut_data, TransformDirection::Forward).unwrap();
    assert_eq!(ops.len(), 2);

    // Apply the inverse: back to the input.
    ops[1].apply(&mut px);
    for (a, e) in px[0].iter().zip(reference) {
        assert_close(*a, e, 1e-5);
    }
}

#[test]
fn op_create_transform() {
    let mut lut = Lut1DOpData::new_with_flags(HalfFlags::STANDARD, 3, false).unwrap();
    lut.set_file_output_bit_depth(BitDepth::UInt10);
    lut.array_mut()[3] = 0.51;
    lut.array_mut()[4] = 0.52;
    lut.array_mut()[5] = 0.53;
    lut.format_metadata_mut()
        .add_attribute(METADATA_NAME, "test");

    let mut ops = OpVec::new();
    create_lut1d_op_from_data(&mut ops, &lut, TransformDirection::Forward).unwrap();
    assert_eq!(ops.len(), 1);

    let t = match ops[0].to_transform() {
        Some(Transform::Lut1D(t)) => t,
        t => panic!("unexpected transform {t:?}"),
    };
    assert_eq!(t.metadata.attributes.len(), 1);
    assert_eq!(t.metadata.attributes[0].0, METADATA_NAME);
    assert_eq!(t.metadata.attributes[0].1, "test");
    assert_eq!(t.direction, TransformDirection::Forward);
    assert_eq!(t.length(), 3);
    assert_eq!(t.file_output_bit_depth, BitDepth::UInt10);
    assert_eq!(t.value(1), [0.51, 0.52, 0.53]);
}

#[test]
fn transform_build_op() {
    let mut lut = Lut1DTransform::default();
    lut.set_length(3);
    lut.set_value(1, 0.51, 0.52, 0.53);

    let config = Config::create_raw();
    let mut ops = OpVec::new();
    crate::transforms::build::build_ops(
        &mut ops,
        &config,
        config.current_context(),
        &Transform::Lut1D(lut),
        TransformDirection::Forward,
    )
    .unwrap();
    assert_eq!(ops.len(), 1);
    let data = lut_op(&ops[0]).data();
    assert_eq!(data.array().length(), 3);
    assert_eq!(data.array()[3], 0.51);
    assert_eq!(data.array()[4], 0.52);
    assert_eq!(data.array()[5], 0.53);
}

#[test]
fn transform_build_op_directions() {
    let mut lut = Lut1DTransform::new(5, false);
    lut.set_value(2, 0.1, 0.1, 0.1);
    lut.direction = TransformDirection::Inverse;
    let mut ops = OpVec::new();
    create_lut1d_op(&mut ops, &lut, TransformDirection::Forward).unwrap();
    create_lut1d_op(&mut ops, &lut, TransformDirection::Inverse).unwrap();
    assert_eq!(
        lut_op(&ops[0]).data().direction(),
        TransformDirection::Inverse
    );
    assert_eq!(
        lut_op(&ops[1]).data().direction(),
        TransformDirection::Forward
    );

    // Invalid transforms are rejected.
    lut.interpolation = Interpolation::Tetrahedral;
    assert_err_contains(
        create_lut1d_op(&mut ops, &lut, TransformDirection::Forward),
        "does not support",
    );
}

// ---------------------------------------------------------------------------
// CPU renderers

#[test]
fn cpu_order3() {
    let posinf = f32::INFINITY;
    let qnan = f32::NAN;
    // (min, mid, max)
    let cases: [([f32; 3], (usize, usize, usize)); 12] = [
        // { A, NaN, B } with A > B test (used to be a crash).
        ([65504.0, -qnan, 0.0], (0, 1, 2)),
        // Triple NaN test.
        ([qnan, qnan, -qnan], (0, 1, 2)),
        // -Inf test.
        ([65504.0, -posinf, 0.0], (1, 2, 0)),
        // Inf test.
        ([0.0, posinf, -65504.0], (2, 0, 1)),
        // Double Inf test.
        ([posinf, posinf, -65504.0], (2, 0, 1)),
        // Equal values.
        ([0.0, 0.0, 0.0], (0, 1, 2)),
        // The six typical possibilities.
        ([3.0, 2.0, 1.0], (2, 1, 0)),
        ([-3.0, -2.0, 1.0], (0, 1, 2)),
        ([-3.0, 2.0, 1.0], (0, 2, 1)),
        ([-0.3, 2.0, -1.0], (2, 0, 1)),
        ([3.0, -2.0, 1.0], (1, 2, 0)),
        ([3.0, -2.0, 10.0], (1, 0, 2)),
    ];
    for (rgb, expected) in cases {
        assert_eq!(order3(&rgb), expected, "{rgb:?}");
    }
}

#[test]
fn cpu_nan_test() {
    let mut lut = Lut1DOpData::new(8).unwrap();
    #[rustfmt::skip]
    let values = [
        0.0,      0.0,      0.002333,
        0.0,      0.291341, 0.015624,
        0.106521, 0.334331, 0.462431,
        0.515851, 0.474151, 0.624611,
        0.658791, 0.527381, 0.685071,
        0.908501, 0.707951, 0.886331,
        0.926671, 0.846431, 1.0,
        1.0,      1.0,      1.0,
    ];
    set_values(&mut lut, &values);

    let qnan = f32::NAN;
    let inf = f32::INFINITY;
    #[rustfmt::skip]
    let mut px = to_pixels(&[
        qnan, 0.5,  0.3,  -0.2,
        0.5,  qnan, 0.3,  0.2,
        0.5,  0.3,  qnan, 1.2,
        0.5,  0.3,  0.2,  qnan,
        inf,  inf,  inf,  inf,
        -inf, -inf, -inf, -inf,
    ]);
    render(&lut, &mut px);
    let p = flat(&px);

    assert_close(p[0], values[0], 1e-7);
    assert_close(p[5], values[1], 1e-7);
    assert_close(p[10], values[2], 1e-7);
    assert!(p[15].is_nan());
    assert_close(p[16], values[21], 1e-7);
    assert_close(p[17], values[22], 1e-7);
    assert_close(p[18], values[23], 1e-7);
    assert_eq!(p[19], inf);
    assert_close(p[20], values[0], 1e-7);
    assert_close(p[21], values[1], 1e-7);
    assert_close(p[22], values[2], 1e-7);
    assert_eq!(p[23], -inf);
}

#[test]
fn cpu_nan_half_test() {
    let mut lut = Lut1DOpData::new_with_flags(HalfFlags::INPUT_HALF_CODE, 65536, false).unwrap();
    // Changed values for NaN input.
    const NAN_ID_RED: usize = 32256 * 3;
    lut.array_mut()[NAN_ID_RED] = -1.0;
    lut.array_mut()[NAN_ID_RED + 1] = -2.0;
    lut.array_mut()[NAN_ID_RED + 2] = -3.0;

    let qnan = f32::NAN;
    #[rustfmt::skip]
    let mut px = to_pixels(&[
        qnan, 0.5,  0.3,  -0.2,
        0.5,  qnan, 0.3,  0.2,
        0.5,  0.3,  qnan, 1.2,
        0.5,  0.3,  0.2,  qnan,
    ]);
    render(&lut, &mut px);
    let p = flat(&px);

    // A half-domain Lut1D can map NaNs to whatever the LUT author wants.
    assert_close(p[0], -1.0, 1e-7);
    assert_close(p[5], -2.0, 1e-7);
    assert_close(p[10], -3.0, 1e-7);
    assert!(p[15].is_nan());
}

#[rustfmt::skip]
const LOGTOLIN_8TO8: [f32; 256] = [
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 2.0, 2.0, 3.0, 3.0, 4.0,
    5.0, 5.0, 6.0, 6.0, 7.0, 8.0, 8.0, 9.0, 10.0, 10.0, 11.0, 12.0, 12.0, 13.0, 14.0, 15.0,
    15.0, 16.0, 17.0, 18.0, 18.0, 19.0, 20.0, 21.0, 22.0, 22.0, 23.0, 24.0, 25.0, 26.0, 27.0, 28.0,
    29.0, 30.0, 30.0, 31.0, 32.0, 33.0, 34.0, 35.0, 36.0, 37.0, 39.0, 40.0, 41.0, 42.0, 43.0, 44.0,
    45.0, 46.0, 48.0, 49.0, 50.0, 51.0, 52.0, 54.0, 55.0, 56.0, 58.0, 59.0, 60.0, 62.0, 63.0, 64.0,
    66.0, 67.0, 69.0, 70.0, 72.0, 73.0, 75.0, 76.0, 78.0, 80.0, 81.0, 83.0, 85.0, 86.0, 88.0, 90.0,
    92.0, 94.0, 95.0, 97.0, 99.0, 101.0, 103.0, 105.0, 107.0, 109.0, 111.0, 113.0, 115.0, 117.0, 120.0, 122.0,
    124.0, 126.0, 129.0, 131.0, 133.0, 136.0, 138.0, 140.0, 143.0, 145.0, 148.0, 151.0, 153.0, 156.0, 159.0, 161.0,
    164.0, 167.0, 170.0, 173.0, 176.0, 179.0, 182.0, 185.0, 188.0, 191.0, 194.0, 198.0, 201.0, 204.0, 208.0, 211.0,
    214.0, 218.0, 222.0, 225.0, 229.0, 233.0, 236.0, 240.0, 244.0, 248.0, 252.0, 255.0, 255.0, 255.0, 255.0, 255.0,
    255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0,
    255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0,
    255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0,
    255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0,
    255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0,
];

#[test]
fn cpu_bit_depth_support() {
    // Pixel bit depth processing with the 1D LUT (logtolin_8to8.lut),
    // emulated with 32f pixels.
    let mut lut = Lut1DOpData::new(256).unwrap();
    set_gray_values(&mut lut, &LOGTOLIN_8TO8);
    lut.array_mut().scale(1.0 / 255.0);

    let uint8_in: [u8; 16] = [
        0, 1, 2, 0, 50, 51, 52, 255, 150, 151, 152, 0, 230, 240, 250, 255,
    ];
    let float_in: Vec<f32> = uint8_in.iter().map(|&v| v as f32 / 255.0).collect();
    let uint16_out: [u32; 16] = [
        0, 0, 0, 0, 4369, 4626, 4626, 65535, 46774, 47545, 48316, 0, 65535, 65535, 65535, 65535,
    ];

    let mut px = to_pixels(&float_in);
    render(&lut, &mut px);
    let out = flat(&px);

    // 8i output.
    let exp8: [u32; 16] = [
        0, 0, 0, 0, 17, 18, 18, 255, 182, 185, 188, 0, 255, 255, 255, 255,
    ];
    for i in 0..16 {
        assert_eq!(quantize(out[i], 255.0), exp8[i], "index {i}");
        assert_eq!(quantize(out[i], 65535.0), uint16_out[i], "index {i}");
    }
    // 32f output.
    #[rustfmt::skip]
    let expf: [f32; 16] = [
        0.0, 0.0, 0.0, 0.0,
        0.06666666666666667, 0.07058823529411765, 0.07058823529411765, 1.0,
        0.7137254901960784, 0.7254901960784313, 0.7372549019607844, 0.0,
        1.0, 1.0, 1.0, 1.0,
    ];
    for i in 0..16 {
        assert_close(out[i], expf[i], 1e-6);
    }

    // Using the (fast) inverse LUT.
    let mut inv = lut.inverse();
    let fast = fast_from_inverse(&mut inv);
    let mut px = to_pixels(&float_in);
    render(&fast, &mut px);
    let out = flat(&px);
    let exp_inv8: [u32; 16] = [
        24, 25, 27, 0, 84, 85, 86, 255, 139, 139, 140, 0, 164, 167, 170, 255,
    ];
    for i in 0..16 {
        let q = quantize(out[i], 255.0);
        assert!(
            q.abs_diff(exp_inv8[i]) <= 1,
            "index {i}: {q} != {}",
            exp_inv8[i]
        );
    }
}

#[test]
fn cpu_basic() {
    // An identity LUT.
    let mut lut = Lut1DOpData::new_with_flags(HalfFlags::STANDARD, 65536, false).unwrap();
    lut.set_file_output_bit_depth(BitDepth::F32);
    lut.validate().unwrap();
    lut.finalize().unwrap();

    let step = 1.0f32 / (lut.array().length() as f32 - 1.0);
    let input = [[0.0, 0.0, 0.0, 1.0], [0.0, 0.0, step, 1.0]];

    let mut px = input;
    render(&lut, &mut px);
    let e = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, step, 1.0];
    for (a, b) in flat(&px).iter().zip(e) {
        assert_close(*a, b, 1e-6);
    }

    // No more an identity LUT.
    const ARBITRARY: f32 = 0.123456;
    lut.array_mut()[5] = ARBITRARY;
    lut.validate().unwrap();
    lut.finalize().unwrap();
    assert!(!lut.is_identity());

    let mut px = input;
    render(&lut, &mut px);
    let e = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, ARBITRARY, 1.0];
    for (a, b) in flat(&px).iter().zip(e) {
        assert_close(*a, b, 1e-6);
    }
}

#[test]
fn cpu_half() {
    let mut lut = Lut1DOpData::new_with_flags(HalfFlags::STANDARD, 65536, false).unwrap();
    let step = 1.0f32 / (lut.array().length() as f32 - 1.0);
    const ARBITRARY: f32 = 0.123456;
    lut.array_mut()[5] = ARBITRARY;
    assert!(!lut.is_identity());

    // Half input values.
    let h = |v: f32| f16::from_f32(v).to_f32();
    let input = [
        h(0.1),
        h(0.3),
        h(0.4),
        h(1.0),
        h(0.0),
        h(0.9),
        h(step),
        h(0.0),
    ];
    let mut px = to_pixels(&input);
    render(&lut, &mut px);
    let out = flat(&px);
    for i in [0, 1, 2, 3, 4, 5, 7] {
        assert_close(out[i], input[i], 1e-6);
    }
    assert_close(out[6], ARBITRARY, 1e-5);
}

#[test]
fn cpu_nan() {
    let lut = Lut1DOpData::new_with_flags(HalfFlags::STANDARD, 65536, false).unwrap();
    let step = 1.0f32 / (lut.array().length() as f32 - 1.0);
    let mut px = [[f32::NAN, 0.0, 0.0, 1.0], [0.0, 0.0, step, 1.0]];
    render(&lut, &mut px);
    assert_eq!(px[0], [0.0, 0.0, 0.0, 1.0]);
    assert_eq!(&px[1][..2], &[0.0, 0.0]);
    assert_close(px[1][2], step, 1e-12);
    assert_eq!(px[1][3], 1.0);
}

fn ramp_lut(channel: usize) -> Lut1DOpData {
    let mut lut = Lut1DOpData::new_with_flags(HalfFlags::STANDARD, 32, false).unwrap();
    for i in 0..32 {
        for c in 0..3 {
            lut.array_mut()[i * 3 + c] = if c == channel {
                (33 * i) as f32 / 1023.0
            } else {
                0.0
            };
        }
    }
    lut
}

#[test]
fn cpu_lut_1d_red_green_blue() {
    for channel in 0..3 {
        let lut = ramp_lut(channel);

        // 32f in, 16i out.
        const STEP: f32 = 1.0 / 31.0;
        #[rustfmt::skip]
        let input = [
            0.0,  0.0,  0.0,  0.0,
            STEP, 0.0,  0.0,  0.0,
            0.0,  STEP, 0.0,  0.0,
            0.0,  0.0,  STEP, 0.0,
            STEP, STEP, STEP, 0.0,
        ];
        let mut px = to_pixels(&input);
        render(&lut, &mut px);
        let out = flat(&px);
        let scaled_step = (STEP * 65535.0).round() as u32;
        for p in 0..5 {
            for c in 0..4 {
                let expected = if c == channel && (p == channel + 1 || p == 4) {
                    scaled_step
                } else {
                    0
                };
                assert_eq!(
                    quantize(out[p * 4 + c], 65535.0),
                    expected,
                    "pixel {p} channel {c}"
                );
            }
        }

        // 16i in, 32f out.
        const STEP16: u32 = 65535 / 31;
        let input16: Vec<f32> = input
            .iter()
            .map(|&v| {
                if v > 0.0 {
                    STEP16 as f32 / 65535.0
                } else {
                    0.0
                }
            })
            .collect();
        let mut px = to_pixels(&input16);
        render(&lut, &mut px);
        let out = flat(&px);
        let scaled = STEP16 as f32 / 65535.0;
        for p in 0..5 {
            for c in 0..4 {
                let expected = if c == channel && (p == channel + 1 || p == 4) {
                    scaled
                } else {
                    0.0
                };
                assert_close(out[p * 4 + c], expected, 1e-6);
                // 16i out.
                let e16 = if expected > 0.0 { STEP16 } else { 0 };
                assert_eq!(quantize(out[p * 4 + c], 65535.0), e16);
            }
        }
    }
}

#[test]
fn cpu_lut_1d_hd_above_half_max() {
    // Float values greater than HALF_MAX but rounding down to HALF_MAX
    // (65504 < x < 65520) and values rounding to Inf use the last finite
    // entry of a half-domain LUT.
    let mut lut = Lut1DOpData::new_with_flags(HalfFlags::INPUT_HALF_CODE, 65536, true).unwrap();
    lut.array_mut()[31743 * 3] = 0.7785763;
    lut.array_mut()[31743 * 3 + 1] = 0.7785763;
    lut.array_mut()[31743 * 3 + 2] = 0.7785763;
    lut.array_mut()[64511 * 3] = 0.0;
    lut.array_mut()[64511 * 3 + 1] = 0.0;
    lut.array_mut()[64511 * 3 + 2] = 0.0;

    let mut px = [
        [65505.0, 65519.0, 65520.0, 0.0],
        [-65505.0, -65519.0, -65520.0, 1.0],
    ];
    render(&lut, &mut px);
    for c in 0..3 {
        assert_close(px[0][c], 0.7785763, 1e-5);
        assert_close(px[1][c], 0.0, 1e-5);
    }
    assert_eq!(px[0][3], 0.0);
    assert_eq!(px[1][3], 1.0);
}

#[test]
fn cpu_lut_1d_hue_adjust_round_trip() {
    // (Synthetic version of lut_1d_inv_hue_adjust.)
    let mut lut = Lut1DOpData::new(1024).unwrap();
    for i in 0..1024 {
        let x = i as f32 / 1023.0;
        let y = 0.5 + 0.5 * (3.0 * (x - 0.5)).tanh() / 1.5f32.tanh();
        for c in 0..3 {
            lut.array_mut()[i * 3 + c] = y;
        }
    }
    lut.set_hue_adjust(Lut1DHueAdjust::Dw3).unwrap();
    lut.set_file_output_bit_depth(BitDepth::UInt16);
    lut.finalize().unwrap();

    #[rustfmt::skip]
    let input = [
        0.1,  0.25, 0.7,  0.0,
        0.66, 0.25, 0.81, 0.5,
        0.18, 0.99, 0.45, 1.0,
    ];
    let mut out = to_pixels(&input);
    render(&lut, &mut out);

    // Hue adjust modifies the middle channel.
    let mut plain = lut.clone();
    plain.set_hue_adjust(Lut1DHueAdjust::None).unwrap();
    let mut out_plain = to_pixels(&input);
    render(&plain, &mut out_plain);
    assert!((out[0][1] - out_plain[0][1]).abs() > 1e-4);

    // Inverse using FAST.
    let mut inv = lut.inverse();
    let fast = fast_from_inverse(&mut inv);
    let mut back = out.clone();
    render(&fast, &mut back);
    for (a, e) in flat(&back).iter().zip(input) {
        assert_close(*a, e, 2e-4);
    }

    // Repeat with EXACT.
    let mut back = out.clone();
    render(&inv, &mut back);
    for (a, e) in flat(&back).iter().zip(input) {
        assert!(!floats_differ(e, *a, 1000), "{e} != {a}");
    }
}

#[test]
fn cpu_lut_1d_half_domain_hue_adjust_round_trip() {
    let mut lut = Lut1DOpData::new_with_flags(HalfFlags::INPUT_HALF_CODE, 65536, false).unwrap();
    for v in lut.array_mut().values_mut().iter_mut() {
        if v.is_finite() {
            *v = v.signum() * v.abs().powf(0.8) * 2.0;
        }
    }
    lut.set_hue_adjust(Lut1DHueAdjust::Dw3).unwrap();

    #[rustfmt::skip]
    let input = [
        0.1,  0.25, 0.7,  0.0,
        0.66, 0.25, 0.81, 0.5,
        0.18, 0.99, 0.45, 1.0,
    ];
    let mut out = to_pixels(&input);
    render(&lut, &mut out);

    let mut inv = lut.inverse();
    inv.finalize().unwrap();
    let mut back = out.clone();
    render(&inv, &mut back);
    for (a, e) in flat(&back).iter().zip(input) {
        assert_close(*a, e, 1e-5);
    }

    let fast = make_fast_lut1d_from_inverse(&inv).unwrap();
    assert!(fast.is_input_half_domain());
    let mut back = out.clone();
    render(&fast, &mut back);
    for (a, e) in flat(&back).iter().zip(input) {
        assert_close(*a, e, 1e-3);
    }
}

#[test]
fn cpu_lut_1d_identity_half() {
    // The 64k 16f identity 1D LUT.
    let lut = Lut1DOpData::new_with_flags(HalfFlags::INPUT_OUTPUT_HALF_CODE, 65536, false).unwrap();
    lut.validate().unwrap();

    let mut px: Vec<Pixel> = (0..65536u32)
        .map(|i| {
            let v = f16::from_bits(i as u16).to_f32();
            [v, v, v, 1.0]
        })
        .collect();
    render(&lut, &mut px);

    for (i, p) in px.iter().enumerate() {
        let h = f16::from_bits(i as u16);
        if h.is_nan() {
            assert_eq!(&p[..3], &[0.0, 0.0, 0.0]);
        } else if h.is_infinite() {
            // (32f input: interpolation uses the largest finite entry.)
            for v in &p[..3] {
                assert_eq!(v.abs(), 65504.0);
            }
        } else {
            // (Compare values: -0 is rendered as +0 by the 32f interpolation.)
            for v in &p[..3] {
                assert_eq!(f16::from_f32(*v).to_f32(), h.to_f32(), "half {i}");
            }
        }
        assert_eq!(p[3], 1.0);
    }
}

#[test]
fn cpu_lut_1d_identity_half_code() {
    let lut = Lut1DOpData::new_with_flags(HalfFlags::INPUT_OUTPUT_HALF_CODE, 65536, false).unwrap();
    let mut input = vec![[0.0f32, 0.0, 0.0, 1.0]];
    // Use values between points to test the interpolation code.
    for i in (4..20u16).step_by(4) {
        let h1 = f16::from_bits(i).to_f32();
        let h2 = f16::from_bits(i + 1).to_f32();
        let delta = (h2 - h1).abs();
        let min = h1.min(h2);
        let v = f16::from_f32(min + delta / i as f32).to_f32();
        input.push([v, v, v, 1.0]);
    }
    let mut px = input.clone();
    render(&lut, &mut px);
    for (p, i) in px.iter().zip(&input) {
        for c in 0..3 {
            assert_eq!(f16::from_f32(p[c]).to_bits(), f16::from_f32(i[c]).to_bits());
        }
        assert_eq!(p[3], 1.0);
    }
}

#[test]
fn cpu_lut_1d_inv_identity() {
    let dim = Lut1DOpData::get_lut_ideal_size(BitDepth::UInt10).unwrap();
    let mut lut = Lut1DOpData::new_with_flags(HalfFlags::STANDARD, dim, false).unwrap();
    lut.set_file_output_bit_depth(BitDepth::UInt10);
    let mut inv = lut.inverse();
    let fast = fast_from_inverse(&mut inv);

    const STEPUI: f32 = 700.0;
    let step = STEPUI / 1023.0;
    #[rustfmt::skip]
    let input = [
        0.0,  0.0,  0.0,  0.0,
        step, 0.0,  0.0,  0.0,
        0.0,  step, 0.0,  0.0,
        0.0,  0.0,  step, 0.0,
        step, step, step, 0.0,
    ];
    // Inverse of identity should still be identity.
    for l in [&fast, &inv] {
        let mut px = to_pixels(&input);
        render(l, &mut px);
        for (a, e) in flat(&px).iter().zip(input) {
            assert_close(*a, e, 1e-6);
        }
    }
}

#[test]
fn cpu_lut_1d_inv_increasing() {
    let mut lut = Lut1DOpData::new(32).unwrap();
    lut.set_file_output_bit_depth(BitDepth::UInt10);
    // A typical "easy" LUT with a simple power function.
    let codes = [
        0.0, 215.0, 294.0, 354.0, 403.0, 446.0, 485.0, 520.0, 553.0, 583.0, 612.0, 639.0, 665.0,
        689.0, 713.0, 735.0, 757.0, 779.0, 799.0, 819.0, 838.0, 857.0, 875.0, 893.0, 911.0, 928.0,
        944.0, 961.0, 977.0, 992.0, 1008.0, 1023.0,
    ];
    let v: Vec<f32> = codes.iter().map(|c| c / 1023.0).collect();
    set_gray_values(&mut lut, &v);

    let mut inv = lut.inverse();
    inv.validate().unwrap();
    inv.finalize().unwrap();

    // The first 2 rows are actual LUT entries, the others are intermediate
    // values (10i scaled to 32f).
    #[rustfmt::skip]
    let input10: [f32; 20] = [
        0.0, 215.0, 446.0, 0.0,
        639.0, 944.0, 1023.0, 445.0, // also test alpha
        40.0, 190.0, 260.0, 685.0,
        380.0, 540.0, 767.0, 1023.0,
        888.0, 1000.0, 1018.0, 0.0,
    ];
    let input: Vec<f32> = input10.iter().map(|v| v / 1023.0).collect();
    #[rustfmt::skip]
    let expected: [u32; 20] = [
        0, 2114, 10570, 0,
        23254, 54965, 65535, 28507,
        393, 1868, 3318, 43882,
        7464, 16079, 34785, 65535,
        48036, 62364, 64830, 0,
    ];

    let mut px = to_pixels(&input);
    render(&inv, &mut px);
    for (i, (a, e)) in flat(&px).iter().zip(expected).enumerate() {
        assert_eq!(quantize(*a, 65535.0), e, "index {i}");
    }

    // Repeat with FAST.
    let fast = make_fast_lut1d_from_inverse(&inv).unwrap();
    let mut px = to_pixels(&input);
    render(&fast, &mut px);
    for (i, (a, e)) in flat(&px).iter().zip(expected).enumerate() {
        let q = quantize(*a, 65535.0);
        assert!(q.abs_diff(e) <= 1, "index {i}: {q} != {e}");
    }
}

#[test]
fn cpu_lut_1d_inv_decreasing_reversals() {
    let mut lut = Lut1DOpData::new(12).unwrap();
    lut.set_file_output_bit_depth(BitDepth::UInt8);
    // A more "difficult" LUT that is decreasing and has reversals and values
    // outside the typical range.
    let codes = [
        90.0, 90.0, 100.0, 80.0, 70.0, 50.0, 60.0, 70.0, 40.0, 20.0, -10.0, -10.0,
    ];
    let v: Vec<f32> = codes.iter().map(|c| c / 255.0).collect();
    set_gray_values(&mut lut, &v);

    let mut inv = lut.inverse();
    inv.validate().unwrap();
    inv.finalize().unwrap();

    let s = 1.0f32 / 255.0;
    #[rustfmt::skip]
    let input = [
        100.0 * s, 90.0 * s, 85.0 * s, 0.0,
        75.0 * s, 60.0 * s, 50.0 * s, 0.0,
        45.0 * s, 30.0 * s, -10.0 * s, 0.0,
        -20.0 * s, 75.0 * s, 30.0 * s, 0.0,
    ];
    #[rustfmt::skip]
    let mut expected: [u32; 16] = [
        11915, 11915, 14894, 0,
        20852, 26810, 29789, 0,
        44683, 50641, 59577, 0,
        59577, 20852, 50641, 0,
    ];

    let mut px = to_pixels(&input);
    render(&inv, &mut px);
    for (i, (a, e)) in flat(&px).iter().zip(expected).enumerate() {
        assert_eq!(quantize(*a, 65535.0), e, "index {i}");
    }

    // Repeat with FAST.
    let fast = make_fast_lut1d_from_inverse(&inv).unwrap();
    let mut px = to_pixels(&input);
    render(&fast, &mut px);
    // When there are flat spots in the original LUT, the approximate inverse
    // used in FAST mode has vertical jumps.
    expected[1] = 11924;
    expected[6] = 38433;
    for (i, (a, e)) in flat(&px).iter().zip(expected).enumerate() {
        assert_eq!(quantize(*a, 65535.0), e, "index {i}");
    }
}

#[test]
fn cpu_lut_1d_inv_clamp_to_range() {
    let mut lut = Lut1DOpData::new(12).unwrap();
    lut.set_file_output_bit_depth(BitDepth::UInt8);
    // The start and end values do not span the full [0,255] range.
    let codes = [
        30.0, 40.0, 60.0, 65.0, 70.0, 50.0, 60.0, 70.0, 100.0, 190.0, 200.0, 210.0,
    ];
    let v: Vec<f32> = codes.iter().map(|c| c / 255.0).collect();
    set_gray_values(&mut lut, &v);

    let mut inv = lut.inverse();
    inv.validate().unwrap();
    inv.finalize().unwrap();

    let s = 1.0f32 / 255.0;
    #[rustfmt::skip]
    let input = [
        0.0 * s, 10.0 * s, 30.0 * s, 0.0,
        35.0 * s, 202.0 * s, 210.0 * s, 0.0,
        -10.0 * s, 255.0 * s, 355.0 * s, 0.0,
    ];
    #[rustfmt::skip]
    let expected: [u32; 12] = [
        0, 0, 0, 0,
        2979, 60769, 65535, 0,
        0, 65535, 65535, 0,
    ];

    let fast = make_fast_lut1d_from_inverse(&inv).unwrap();
    for l in [&inv, &fast] {
        let mut px = to_pixels(&input);
        render(l, &mut px);
        for (i, (a, e)) in flat(&px).iter().zip(expected).enumerate() {
            assert_eq!(quantize(*a, 65535.0), e, "index {i}");
        }
    }
}

#[test]
fn cpu_lut_1d_inv_flat_start_or_end() {
    let mut lut = Lut1DOpData::new(9).unwrap();
    lut.set_file_output_bit_depth(BitDepth::UInt10);
    // Flat spots at beginning and end of various lengths for increasing and
    // decreasing LUTs (with different R, G, B values).
    #[rustfmt::skip]
    let codes = [
        900.0,  70.0,  70.0,
        900.0,  70.0, 120.0,
        900.0, 120.0, 300.0,
        900.0, 300.0, 450.0,
        450.0, 450.0, 900.0,
        300.0, 900.0, 900.0,
        120.0, 900.0, 900.0,
         70.0, 900.0, 900.0,
         70.0, 900.0, 900.0,
    ];
    let v: Vec<f32> = codes.iter().map(|c| c / 1023.0).collect();
    set_values(&mut lut, &v);

    let mut inv = lut.inverse();
    inv.validate().unwrap();
    inv.finalize().unwrap();

    let in10 = [
        1023.0, 900.0, 800.0, 500.0, 450.0, 330.0, 150.0, 120.0, 80.0, 70.0, 60.0, 0.0,
    ];
    let input: Vec<f32> = in10
        .iter()
        .flat_map(|&v: &f32| [v / 1023.0, v / 1023.0, v / 1023.0, 0.0])
        .collect();
    #[rustfmt::skip]
    let expected: [u32; 48] = [
        24576, 40959, 32768, 0,
        24576, 40959, 32768, 0,
        26396, 39139, 30947, 0,
        31857, 33678, 25486, 0,
        32768, 32768, 24576, 0,
        39321, 26214, 18022, 0,
        47786, 17749,  9557, 0,
        49151, 16384,  8192, 0,
        55705,  9830,  1638, 0,
        57343,  8192,     0, 0,
        57343,  8192,     0, 0,
        57343,  8192,     0, 0,
    ];

    let mut px = to_pixels(&input);
    render(&inv, &mut px);
    for (i, (a, e)) in flat(&px).iter().zip(expected).enumerate() {
        let q = quantize(*a, 65535.0);
        assert!(q.abs_diff(e) <= 1, "index {i}: {q} != {e}");
    }

    // Repeat with FAST.
    let fast = make_fast_lut1d_from_inverse(&inv).unwrap();
    let mut px = to_pixels(&input);
    render(&fast, &mut px);
    for (i, (a, e)) in flat(&px).iter().zip(expected).enumerate() {
        let q = quantize(*a, 65535.0);
        assert!(q.abs_diff(e) <= 1, "index {i}: {q} != {e}");
    }
}

#[test]
fn cpu_lut_1d_inv_half_input() {
    const DIM: usize = 15;
    let mut lut = Lut1DOpData::new(DIM).unwrap();
    lut.set_file_output_bit_depth(BitDepth::UInt8);
    let entries = [
        0.00, 0.05, 0.10, 0.15, 0.20, 0.30, 0.40, 0.50, 0.60, 0.70, 0.80, 0.85, 0.90, 0.95, 1.00,
    ];
    lut.array_mut().resize(DIM, 1).unwrap();
    set_gray_values(&mut lut, &entries);

    let mut inv = lut.inverse();
    inv.validate().unwrap();
    inv.finalize().unwrap();

    let h = |v: f32| f16::from_f32(v).to_f32();
    #[rustfmt::skip]
    let input: Vec<f32> = [
        1.00, 0.91, 0.85, 0.0,
        0.75, 0.02, 0.53, 0.0,
        0.47, 0.30, 0.21, 0.0,
        0.50, 0.11, 0.00, 0.0,
    ].iter().map(|&v| h(v)).collect();
    // (dist + (val-low)/(high-low)) / (dim-1)
    #[rustfmt::skip]
    let expected = [
        1.0000000000, 0.8714285714, 0.7857142857, 0.0,
        0.6785714285, 0.0285714285, 0.5214285714, 0.0,
        0.4785714285, 0.3571428571, 0.2928571428, 0.0,
        0.5000000000, 0.1571428571, 0.0000000000, 0.0,
    ];

    let fast = make_fast_lut1d_from_inverse(&inv).unwrap();
    for l in [&inv, &fast] {
        let mut px = to_pixels(&input);
        render(l, &mut px);
        for (a, e) in flat(&px).iter().zip(expected) {
            let a = f16::from_f32(*a).to_bits();
            let e = f16::from_f32(e).to_bits();
            assert!(a.abs_diff(e) <= 1, "{a} != {e}");
        }
    }
}

#[test]
fn cpu_lut_1d_inv_half_identity() {
    const STEPUI: u32 = 700;
    let step = STEPUI as f32 / 1023.0;

    // 10i to 32f bit-depths.
    {
        let mut lut =
            Lut1DOpData::new_with_flags(HalfFlags::INPUT_HALF_CODE, 65536, false).unwrap();
        lut.set_file_output_bit_depth(BitDepth::UInt10);
        let mut inv = lut.inverse();
        inv.validate().unwrap();
        inv.finalize().unwrap();

        #[rustfmt::skip]
        let input = [
            0.0,  0.0,  0.0,  0.0,
            step, 0.0,  0.0,  0.0,
            0.0,  step, 0.0,  0.0,
            0.0,  0.0,  step, 0.0,
            step, step, step, 0.0,
        ];
        let fast = make_fast_lut1d_from_inverse(&inv).unwrap();
        for l in [&inv, &fast] {
            let mut px = to_pixels(&input);
            render(l, &mut px);
            // Inverse of identity should still be identity.
            for (a, e) in flat(&px).iter().zip(input) {
                assert_close(*a, e, 1e-6);
            }
        }
    }
    // 32f to 10i bit-depths.
    {
        let mut lut =
            Lut1DOpData::new_with_flags(HalfFlags::INPUT_HALF_CODE, 65536, false).unwrap();
        lut.set_file_output_bit_depth(BitDepth::F32);
        let mut inv = lut.inverse();
        inv.validate().unwrap();
        inv.finalize().unwrap();

        #[rustfmt::skip]
        let input = [
            0.0,  0.0,  0.0,  0.0,
            step, 0.0,  0.0,  0.0,
            0.0,  step, 0.0,  0.0,
            0.0,  0.0,  step, 0.0,
            step, step, step, 0.0,
        ];
        let fast = make_fast_lut1d_from_inverse(&inv).unwrap();
        for l in [&inv, &fast] {
            let mut px = to_pixels(&input);
            render(l, &mut px);
            for (a, e) in flat(&px).iter().zip(input) {
                assert_eq!(quantize(*a, 1023.0), quantize(e, 1023.0));
            }
        }
    }
}

#[test]
fn cpu_lut_1d_inv_half_round_trip() {
    // (Synthetic version of lut_1d_inv_half_ctf: increasing R & B channels,
    // decreasing G channel, with flat spots.)
    let mut lut = Lut1DOpData::new_with_flags(HalfFlags::INPUT_HALF_CODE, 65536, false).unwrap();
    for i in 0..65536usize {
        let x = f16::from_bits(i as u16).to_f32();
        if !x.is_finite() {
            continue;
        }
        let r = if x < -1.0 { -1.0 } else { x * 1.5 + 0.01 };
        let g = -0.5 * x + 0.25;
        let b = if x > 100.0 {
            100.0
        } else {
            x.signum() * x.abs().powf(1.1)
        };
        lut.array_mut()[i * 3] = r;
        lut.array_mut()[i * 3 + 1] = g;
        lut.array_mut()[i * 3 + 2] = b;
    }
    #[rustfmt::skip]
    let input = [
        1.0,   1.0,  0.5,   0.0,
        0.001, 0.1,  4.0,   0.5,  // positive half domain of R, G, B channels
        -0.08, -1.0, -10.0, 1.0,  // negative half domain of R, G, B channels
    ];
    let mut out = to_pixels(&input);
    render(&lut, &mut out);

    let mut inv = lut.inverse();
    inv.finalize().unwrap();
    let mut back = out.clone();
    render(&inv, &mut back);
    for (a, e) in flat(&back).iter().zip(input) {
        assert!(!floats_differ(e, *a, 50), "{e} != {a}");
    }

    let fast = make_fast_lut1d_from_inverse(&inv).unwrap();
    let mut back = out.clone();
    render(&fast, &mut back);
    for (a, e) in flat(&back).iter().zip(input) {
        assert_close(*a, e, 1e-3);
    }
}

#[test]
fn cpu_lut_1d_inv_half_fclut_like() {
    // A half-domain LUT mapping all positive halfs to unique 16-bit ints:
    // the exact inverse restores the halfs losslessly.
    let mut lut = Lut1DOpData::new_with_flags(HalfFlags::INPUT_HALF_CODE, 65536, true).unwrap();
    for i in 0..65536usize {
        let v = if i < 31744 {
            i as f32 / 65535.0
        } else if i < 32768 {
            31744.0 / 65535.0
        } else {
            0.0
        };
        for c in 0..3 {
            lut.array_mut()[i * 3 + c] = v;
        }
    }
    let input: Vec<Pixel> = (0..31744u16)
        .map(|i| {
            let v = f16::from_bits(i).to_f32();
            [v, v, v, v]
        })
        .collect();
    let mut out = input.clone();
    render(&lut, &mut out);
    let mut inv = lut.inverse();
    inv.finalize().unwrap();
    render(&inv, &mut out);
    for (p, i) in out.iter().zip(&input) {
        for c in 0..4 {
            assert_eq!(f16::from_f32(p[c]).to_bits(), f16::from_f32(i[c]).to_bits());
        }
    }
}

// ---------------------------------------------------------------------------
// Separable prefix & baking helpers

#[test]
fn bake_ops_to_lut1d_and_separable_prefix() {
    let lut = create_square_lut();
    let mut ops = OpVec::new();
    create_lut1d_op_from_data(&mut ops, &lut, TransformDirection::Forward).unwrap();
    create_lut1d_op_from_data(&mut ops, &lut, TransformDirection::Forward).unwrap();

    let baked = bake_ops_to_lut1d(&ops, BitDepth::UInt10).unwrap();
    assert_eq!(baked.array().length(), 1024);
    let x = 512.0f32 / 1023.0;
    assert_close(baked.array()[512 * 3], x * x * x * x, 1e-4);

    let mut ops2 = ops.clone();
    optimize_separable_prefix(&mut ops2, BitDepth::UInt10).unwrap();
    assert_eq!(ops2.len(), 1);
    assert_eq!(lut_op(&ops2[0]).data().array().length(), 1024);

    // Nothing is done for 32f.
    let mut ops3 = ops.clone();
    optimize_separable_prefix(&mut ops3, BitDepth::F32).unwrap();
    assert_eq!(ops3.len(), 2);

    // A single forward LUT is left alone.
    let mut ops4 = OpVec::new();
    create_lut1d_op_from_data(&mut ops4, &lut, TransformDirection::Forward).unwrap();
    optimize_separable_prefix(&mut ops4, BitDepth::UInt8).unwrap();
    assert_eq!(lut_op(&ops4[0]).data().array().length(), 256);

    // A single inverse LUT is replaced (by a 16f half-domain LUT).
    let mut ops5 = OpVec::new();
    create_lut1d_op_from_data(&mut ops5, &lut, TransformDirection::Inverse).unwrap();
    optimize_separable_prefix(&mut ops5, BitDepth::F16).unwrap();
    assert_eq!(
        lut_op(&ops5[0]).data().direction(),
        TransformDirection::Forward
    );
    assert!(lut_op(&ops5[0]).data().is_input_half_domain());

    assert_err_contains(
        Lut1DOpData::compose_vec(&mut create_square_lut(), &[]),
        "nothing to compose",
    );
}

#[test]
fn processor_compose_luts() {
    let lut = create_square_lut();
    let mut ops = OpVec::new();
    create_lut1d_op_from_data(&mut ops, &lut, TransformDirection::Forward).unwrap();
    create_lut1d_op_from_data(&mut ops, &lut, TransformDirection::Forward).unwrap();
    let proc = Processor::from_ops(ops);
    let cpu = proc.default_cpu_processor();
    assert_eq!(cpu.ops().len(), 1);
    let mut rgb = [0.5, 0.25, 1.0];
    cpu.apply_rgb(&mut rgb);
    assert_close(rgb[0], 0.0625, 1e-4);
    assert_close(rgb[1], 0.00390625, 1e-4);
    assert_close(rgb[2], 1.0, 1e-6);
    assert!(!cpu.has_channel_crosstalk());
}

// ---------------------------------------------------------------------------
// Lut1DTransform

#[test]
fn transform_basic() {
    let mut lut = Lut1DTransform::default();
    assert_eq!(lut.length(), 2);
    assert_eq!(lut.direction, TransformDirection::Forward);
    assert_eq!(lut.hue_adjust, Lut1DHueAdjust::None);
    assert!(!lut.input_half_domain);
    assert!(!lut.output_raw_halfs);
    assert_eq!(lut.value(0), [0.0, 0.0, 0.0]);
    assert_eq!(lut.value(1), [1.0, 1.0, 1.0]);

    lut.direction = TransformDirection::Inverse;
    lut.set_length(3);
    assert_eq!(lut.length(), 3);
    assert_eq!(lut.value(0), [0.0, 0.0, 0.0]);
    assert_eq!(lut.value(1), [0.5, 0.5, 0.5]);
    assert_eq!(lut.value(2), [1.0, 1.0, 1.0]);

    lut.set_value(1, 0.51, 0.52, 0.53);
    assert_eq!(lut.value(1), [0.51, 0.52, 0.53]);

    assert_eq!(lut.file_output_bit_depth, BitDepth::Unknown);
    lut.file_output_bit_depth = BitDepth::UInt8;
    // File out bit-depth does not affect values.
    assert_eq!(lut.value(1), [0.51, 0.52, 0.53]);

    assert!(lut.validate().is_ok());

    lut.input_half_domain = true;
    assert_err_contains(lut.validate(), "65536 required for halfDomain 1D LUT");
    lut.input_half_domain = false;

    lut.set_value(0, -0.2, 0.1, -0.3);
    lut.set_value(2, 1.2, 1.3, 0.8);

    assert_eq!(
        lut.to_string(),
        "<Lut1DTransform direction=inverse, fileoutdepth=8ui, interpolation=default, inputhalf=0, \
         outputrawhalf=0, hueadjust=0, length=3, minrgb=[-0.2, 0.1, -0.3], maxrgb=[1.2, 1.3, 0.8]>"
    );

    let lut2 = lut.clone();
    assert_eq!(lut2.to_string(), lut.to_string());
    assert_eq!(lut, lut2);

    let mut big = Lut1DTransform::new(2, false);
    big.values = vec![0.0; (1024 * 1024 + 1) * 3];
    assert_err_contains(big.validate(), "must not be greater than");

    let mut odd = Lut1DTransform::new(4, false);
    odd.values.pop();
    assert_err_contains(
        odd.validate(),
        "Lut1DTransform validation failed: 1D LUT content array issue: Array contains",
    );
}

#[test]
fn transform_create_with_parameters() {
    let lut0 = Lut1DTransform::new(65536, true);
    assert_eq!(lut0.length(), 65536);
    assert_eq!(lut0.direction, TransformDirection::Forward);
    assert_eq!(lut0.hue_adjust, Lut1DHueAdjust::None);
    assert!(lut0.input_half_domain);
    assert!(lut0.validate().is_ok());

    let lut1 = Lut1DTransform::new(10, true);
    assert_eq!(lut1.length(), 10);
    assert!(lut1.input_half_domain);
    assert_err_contains(lut1.validate(), "65536 required for halfDomain 1D LUT");

    let lut2 = Lut1DTransform::new(8, false);
    assert_eq!(lut2.length(), 8);
    assert!(!lut2.input_half_domain);
    assert!(lut2.validate().is_ok());
}

#[test]
fn transform_non_monotonic() {
    let mut lut = Lut1DTransform::default();
    // Make a non-monotonic LUT.
    lut.set_length(5);
    lut.set_value(2, 0.1, 0.1, 0.1);
    assert!(lut.validate().is_ok());

    let config = Config::create_raw();

    // Processor from forward LUT.
    let proc = config
        .get_processor_for_transform(&Transform::Lut1D(lut.clone()), TransformDirection::Forward)
        .unwrap();
    let group = proc.create_group_transform();
    assert_eq!(group.transforms.len(), 1);
    let t = match &group.transforms[0] {
        Transform::Lut1D(t) => t,
        t => panic!("unexpected {t:?}"),
    };
    // Transform is still a non-monotonic LUT.
    assert_eq!(t.value(2), [0.1, 0.1, 0.1]);

    // Now with inverse LUT.
    lut.direction = TransformDirection::Inverse;
    let proc = config
        .get_processor_for_transform(&Transform::Lut1D(lut), TransformDirection::Forward)
        .unwrap();
    let group = proc.create_group_transform();
    assert_eq!(group.transforms.len(), 1);
    let t = match &group.transforms[0] {
        Transform::Lut1D(t) => t,
        t => panic!("unexpected {t:?}"),
    };
    // LUT has been made monotonic.
    assert_eq!(t.value(2), [0.25, 0.25, 0.25]);
}

#[test]
fn transform_hue_adjust() {
    let mut lut = Lut1DTransform::default();
    assert_eq!(lut.hue_adjust, Lut1DHueAdjust::None);
    lut.hue_adjust = Lut1DHueAdjust::Dw3;
    assert!(lut.validate().is_ok());
    lut.hue_adjust = Lut1DHueAdjust::Wypn;
    assert_err_contains(
        lut.validate(),
        "1D LUT HUE_WYPN hue adjust style is not implemented.",
    );
}

#[test]
fn transform_format_metadata() {
    let mut lut = Lut1DTransform::default();
    lut.metadata.set_name("test LUT");
    lut.metadata.set_id("LUTID");
    assert_eq!(lut.metadata.name(), "test LUT");
    assert_eq!(lut.metadata.id(), "LUTID");

    // Metadata is kept by the op and the transform built from it.
    let mut ops = OpVec::new();
    create_lut1d_op(&mut ops, &lut, TransformDirection::Forward).unwrap();
    match ops[0].to_transform() {
        Some(Transform::Lut1D(t)) => assert_eq!(t.metadata.id(), "LUTID"),
        t => panic!("unexpected {t:?}"),
    }
}

#[test]
fn format_float_like_cpp() {
    assert_eq!(format_float_g(-0.2), "-0.2");
    assert_eq!(format_float_g(1.0), "1");
    assert_eq!(format_float_g(0.0), "0");
    assert_eq!(format_float_g(1234567.0), "1.23457e+06");
    assert_eq!(format_float_g(0.0001), "0.0001");
    assert_eq!(format_float_g(0.00001), "1e-05");
    assert_eq!(format_float_g(65504.0), "65504");
    assert_eq!(format_float_g(0.123456789), "0.123457");
}
