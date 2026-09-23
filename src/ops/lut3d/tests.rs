//! Tests ported from `Lut3DOpData_tests.cpp`, `Lut3DOp_tests.cpp`,
//! `Lut3DOpCPU_tests.cpp` and `Lut3DTransform_tests.cpp` (CPU path; the
//! file-based tests use synthetic LUTs instead).

#![allow(clippy::excessive_precision, clippy::needless_range_loop)]

use super::*;
use crate::ops::lut1d::{create_lut1d_op_from_data, replace_inverse_luts, Lut1DOpData};
use crate::processor::optimize_ops;
use crate::types::{METADATA_DESCRIPTION, METADATA_NAME};

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

fn render(lut: &Lut3DOpData, pixels: &mut [Pixel]) {
    Lut3DOp::new(lut.clone()).unwrap().apply(pixels);
}

fn lut_op(op: &OpRc) -> &Lut3DOp {
    op.downcast_ref::<Lut3DOp>().unwrap()
}

/// A smooth, invertible, non-linear 3D LUT with some channel crosstalk.
fn synthetic_lut(n: usize) -> Lut3DOpData {
    let mut lut = Lut3DOpData::new(n).unwrap();
    let s = 1.0 / (n - 1) as f32;
    for r in 0..n {
        for g in 0..n {
            for b in 0..n {
                let (x, y, z) = (
                    (r as f32 * s).powf(0.8),
                    (g as f32 * s).powf(1.2),
                    (b as f32 * s).powf(0.9),
                );
                let rgb = [
                    0.8 * x + 0.15 * y + 0.05 * z,
                    0.1 * x + 0.8 * y + 0.1 * z,
                    0.05 * x + 0.1 * y + 0.85 * z,
                ];
                lut.array_mut().set_rgb(r, g, b, rgb);
            }
        }
    }
    lut
}

// ---------------------------------------------------------------------------
// Lut3DOpCPU

fn lut3d_renderer_nan_test(interp: Interpolation) {
    let mut lut = Lut3DOpData::with_interpolation(interp, 4).unwrap();
    // Change the LUT so that it is not an identity.
    lut.array_mut()[65] += 0.001;
    let values = lut.array().values().to_vec();

    let qnan = f32::NAN;
    let inf = f32::INFINITY;
    let mut px = [
        [qnan, qnan, qnan, 0.5],
        [0.5, 0.3, 0.2, qnan],
        [inf, inf, inf, inf],
        [-inf, -inf, -inf, -inf],
    ];
    render(&lut, &mut px);

    assert_close(px[0][0], values[0], 1e-7);
    assert_close(px[0][1], values[1], 1e-7);
    assert_close(px[0][2], values[2], 1e-7);
    assert!(px[1][3].is_nan());
    assert_close(px[2][0], 1.0, 1e-7);
    assert_close(px[2][1], 1.0, 1e-7);
    assert_close(px[2][2], 1.0, 1e-7);
    assert_eq!(px[2][3], inf);
    assert_close(px[3][0], 0.0, 1e-7);
    assert_close(px[3][1], 0.0, 1e-7);
    assert_close(px[3][2], 0.0, 1e-7);
    assert_eq!(px[3][3], -inf);
}

#[test]
fn cpu_nan_linear_test() {
    lut3d_renderer_nan_test(Interpolation::Linear);
}

#[test]
fn cpu_nan_tetra_test() {
    lut3d_renderer_nan_test(Interpolation::Tetrahedral);
}

#[test]
fn cpu_linear_and_tetrahedral_values() {
    // Both interpolations are exact for an affine LUT.
    let mut lut = Lut3DOpData::new(5).unwrap();
    for v in lut.array_mut().values_mut().iter_mut() {
        *v = *v * 0.5 + 0.25;
    }
    for interp in [
        Interpolation::Linear,
        Interpolation::Tetrahedral,
        Interpolation::Best,
        Interpolation::Nearest,
    ] {
        lut.set_interpolation(interp);
        let mut px = [
            [0.1, 0.7, 0.33, 1.0],
            [0.9, 0.2, 0.61, 0.5],
            [2.0, -1.0, 0.5, 0.0],
        ];
        render(&lut, &mut px);
        assert_close(px[0][0], 0.30, 1e-6);
        assert_close(px[0][1], 0.60, 1e-6);
        assert_close(px[0][2], 0.415, 1e-6);
        assert_close(px[1][0], 0.70, 1e-6);
        assert_close(px[1][1], 0.35, 1e-6);
        assert_close(px[1][2], 0.555, 1e-6);
        // Inputs are clamped to the domain.
        assert_close(px[2][0], 0.75, 1e-6);
        assert_close(px[2][1], 0.25, 1e-6);
        assert_close(px[2][2], 0.5, 1e-6);
        assert_eq!(px[0][3], 1.0);
        assert_eq!(px[1][3], 0.5);
    }

    // Tetrahedral differs from trilinear for non-linear LUTs.
    let mut lut = synthetic_lut(3);
    for v in lut.array_mut().values_mut().iter_mut() {
        *v = *v * *v;
    }
    lut.set_interpolation(Interpolation::Linear);
    let mut lin = [[0.3, 0.6, 0.8, 1.0]];
    render(&lut, &mut lin);
    lut.set_interpolation(Interpolation::Tetrahedral);
    let mut tet = [[0.3, 0.6, 0.8, 1.0]];
    render(&lut, &mut tet);
    assert!((lin[0][0] - tet[0][0]).abs() > 1e-5);
}

// ---------------------------------------------------------------------------
// Lut3DOpData

#[test]
fn opdata_empty() {
    let l = Lut3DOpData::new(2).unwrap();
    assert!(l.validate().is_ok());
    assert!(!l.is_identity());
    assert!(!l.is_no_op());
    assert_eq!(l.direction(), TransformDirection::Forward);
    assert!(l.has_channel_crosstalk());
}

#[test]
fn opdata_accessors() {
    let mut interp = Interpolation::Linear;
    let mut l = Lut3DOpData::with_interpolation(interp, 33).unwrap();
    l.format_metadata_mut().set_id("uid");
    assert_eq!(l.interpolation(), interp);

    l.array_mut()[0] = 1.0;
    assert!(!l.is_identity());
    assert!(l.validate().is_ok());

    interp = Interpolation::Tetrahedral;
    l.set_interpolation(interp);
    assert_eq!(l.interpolation(), interp);

    assert_eq!(l.array().length(), 33);
    assert_eq!(l.array().num_values(), 33 * 33 * 33 * 3);
    assert_eq!(l.array().num_color_components(), 3);

    l.array_mut().resize(17, 3).unwrap();
    assert_eq!(l.array().length(), 17);
    assert_eq!(l.array().num_values(), 17 * 17 * 17 * 3);
    assert_eq!(l.array().num_color_components(), 3);
    assert!(l.validate().is_ok());
}

#[test]
fn opdata_clone() {
    let mut r = Lut3DOpData::new(33).unwrap();
    r.array_mut()[1] = 0.1;
    let c = r.clone();
    assert!(!c.is_no_op());
    assert!(!c.is_identity());
    assert!(c.validate().is_ok());
    assert!(c.array() == r.array());
}

#[test]
fn opdata_not_supported_length() {
    assert!(Lut3DOpData::new(MAX_3D_LUT_LENGTH).is_ok());
    assert_err_contains(
        Lut3DOpData::new(MAX_3D_LUT_LENGTH + 1),
        "must not be greater",
    );
}

#[test]
fn opdata_equality() {
    let l1 = Lut3DOpData::with_interpolation(Interpolation::Linear, 33).unwrap();
    let l2 = Lut3DOpData::with_interpolation(Interpolation::Best, 33).unwrap();
    assert!(!(l1 == l2));
    let l3 = Lut3DOpData::with_interpolation(Interpolation::Linear, 33).unwrap();
    assert!(l1 == l3);
}

#[test]
fn opdata_interpolation() {
    let mut l = Lut3DOpData::new(2).unwrap();

    l.set_interpolation(Interpolation::Linear);
    assert_eq!(l.interpolation(), Interpolation::Linear);
    assert_eq!(l.concrete_interpolation(), Interpolation::Linear);
    assert!(l.validate().is_ok());

    l.set_interpolation(Interpolation::Cubic);
    assert_eq!(l.interpolation(), Interpolation::Cubic);
    assert_eq!(l.concrete_interpolation(), Interpolation::Linear);
    assert_err_contains(
        l.validate(),
        "does not support interpolation algorithm: cubic",
    );

    l.set_interpolation(Interpolation::Tetrahedral);
    assert_eq!(l.concrete_interpolation(), Interpolation::Tetrahedral);
    assert!(l.validate().is_ok());

    l.set_interpolation(Interpolation::Default);
    assert_eq!(l.concrete_interpolation(), Interpolation::Linear);
    assert!(l.validate().is_ok());

    l.set_interpolation(Interpolation::Best);
    assert_eq!(l.concrete_interpolation(), Interpolation::Tetrahedral);
    assert!(l.validate().is_ok());

    // Nearest is implemented as linear.
    l.set_interpolation(Interpolation::Nearest);
    assert_eq!(l.concrete_interpolation(), Interpolation::Linear);
    assert!(l.validate().is_ok());

    // Invalid interpolations are implemented as linear but validation fails.
    l.set_interpolation(Interpolation::Unknown);
    assert_eq!(l.concrete_interpolation(), Interpolation::Linear);
    assert_err_contains(
        l.validate(),
        "does not support interpolation algorithm: unknown.",
    );
}

#[test]
fn opdata_is_inverse() {
    // Create forward LUT.
    let mut l1 = Lut3DOpData::with_interpolation(Interpolation::Linear, 5).unwrap();
    l1.set_name("Forward");
    // Make it not an identity.
    l1.array_mut().values_mut()[0] = 20.0;
    assert!(!l1.is_identity());

    // Create an inverse LUT with same basics.
    let mut l2 = l1.inverse();
    l2.set_name("Inverse");
    assert!(!(l1 == l2));

    // Back to forward.
    let l3 = l2.inverse();
    assert!(l3 == l1);

    assert!(l1.is_inverse(&l2));
    assert!(l2.is_inverse(&l1));
}

#[test]
fn opdata_compose_metadata() {
    let mut lut1 = synthetic_lut(17);
    lut1.set_name("lut1");
    lut1.format_metadata_mut()
        .add_child_element(METADATA_DESCRIPTION, "description of lut1");
    let mut lut2 = Lut3DOpData::new(32).unwrap();
    for v in lut2.array_mut().values_mut().iter_mut() {
        *v = v.powf(2.2);
    }
    lut2.set_name("lut2");
    lut2.format_metadata_mut()
        .add_child_element(METADATA_DESCRIPTION, "description of lut2");

    let composed = Lut3DOpData::compose(&lut1, &lut2).unwrap();
    assert_eq!(composed.name(), "lut1 + lut2");
    let md = composed.format_metadata();
    assert_eq!(md.children.len(), 2);
    assert_eq!(md.children[0].element_name(), METADATA_DESCRIPTION);
    assert_eq!(md.children[0].element_value(), "description of lut1");
    assert_eq!(md.children[1].element_value(), "description of lut2");

    // The finer grid is used.
    assert_eq!(composed.array().length(), 32);
    assert_eq!(composed.array().num_values(), 32 * 32 * 32 * 3);

    // The result evaluates both LUTs.
    let mut a = [[0.3, 0.5, 0.8, 1.0]];
    render(&composed, &mut a);
    let mut b = [[0.3, 0.5, 0.8, 1.0]];
    render(&lut1, &mut b);
    render(&lut2, &mut b);
    for c in 0..3 {
        assert_close(a[0][c], b[0][c], 2e-3);
    }

    // The coarser first LUT domain is kept when it is big enough.
    let composed = Lut3DOpData::compose(&lut2, &lut1).unwrap();
    assert_eq!(composed.array().length(), 32);
}

#[test]
fn opdata_inv_lut3d_lut_size() {
    let mut fwd = synthetic_lut(17);
    fwd.set_file_output_bit_depth(BitDepth::UInt12);
    let inv = fwd.inverse();
    let fast = make_fast_lut3d_from_inverse(&inv).unwrap();
    assert_eq!(fast.file_output_bit_depth(), BitDepth::UInt12);
    assert_eq!(fast.array().length(), 48);
    assert_eq!(fast.direction(), TransformDirection::Forward);

    assert_err_contains(make_fast_lut3d_from_inverse(&fwd), "expects an inverse LUT");
}

#[test]
fn opdata_compose_inverse_luts() {
    let lut_ref = Lut3DOpData::new(5).unwrap();
    let mut lut = lut_ref.clone();
    for v in lut.array_mut().values_mut().iter_mut() {
        *v *= *v;
    }

    let lut_fwd1 = lut.clone();
    let lut_fwd2 = lut_fwd1.clone();

    // Forward + forward.
    let comp_fwd_fwd = Lut3DOpData::compose(&lut_fwd1, &lut_fwd2).unwrap();
    assert_eq!(comp_fwd_fwd.direction(), TransformDirection::Forward);

    // Inverse + inverse.
    lut.set_direction(TransformDirection::Inverse);
    let lut_inv1 = lut.clone();
    let comp_inv_inv = Lut3DOpData::compose(&lut_inv1, &lut_inv1).unwrap();
    assert_eq!(comp_inv_inv.direction(), TransformDirection::Inverse);
    assert_eq!(comp_fwd_fwd.array().values(), comp_inv_inv.array().values());

    // Forward + inverse.
    let comp_fwd_inv = Lut3DOpData::compose(&lut_fwd1, &lut_inv1).unwrap();
    assert_eq!(comp_fwd_inv.direction(), TransformDirection::Forward);
    assert_eq!(comp_fwd_inv.array().values(), lut_ref.array().values());

    // Inverse + forward.
    let comp_inv_fwd = Lut3DOpData::compose(&lut_inv1, &lut_fwd1).unwrap();
    assert_eq!(comp_inv_fwd.direction(), TransformDirection::Forward);
    let v = comp_inv_fwd.array().values();
    let r = lut_ref.array().values();
    for i in 0..v.len() / 3 {
        assert_close(v[i * 3], r[i * 3], 1e-5);
    }
}

#[test]
fn opdata_red_fastest_order() {
    let n = 3;
    let mut red_fast = vec![0.0f32; n * n * n * 3];
    generate_identity_lut3d(&mut red_fast, n, 3, Lut3DOrder::FastRed).unwrap();
    let mut lut = Lut3DOpData::new(n).unwrap();
    lut.array_mut().values_mut().fill(0.0);
    lut.set_array_from_red_fastest_order(&red_fast).unwrap();
    assert_eq!(
        lut.array().values(),
        Lut3DOpData::new(n).unwrap().array().values()
    );
    assert_err_contains(
        lut.set_array_from_red_fastest_order(&red_fast[1..]),
        "does not match the vector size",
    );

    assert_eq!(
        get_lut3d_index_blue_fast(1, 2, 0, 3, 3, 3),
        3 * ((3 + 2) * 3)
    );
    assert_eq!(get_lut3d_index_red_fast(1, 2, 0, 3, 3, 3), 3 * (1 + 3 * 2));
}

#[test]
fn opdata_validate_errors() {
    let mut l = Lut3DOpData::new(3).unwrap();
    l.array_mut().values_mut().pop();
    assert_err_contains(
        l.validate(),
        "Lut3D content array issue: Array contains: 80 values, but 81 are expected.",
    );
    assert!(Lut3DOp::new(l).is_err());

    let l = Lut3DOpData::new(0).unwrap();
    assert_err_contains(l.validate(), "Array content is empty.");

    let l = Lut3DOpData::new(1).unwrap();
    assert_err_contains(l.validate(), "at least 2");
}

// ---------------------------------------------------------------------------
// Lut3DOp

#[test]
fn op_inverse_comparison_check() {
    let lut_a = Lut3DOpData::new(32).unwrap();
    let lut_b = Lut3DOpData::new(16).unwrap();

    let mut ops = OpVec::new();
    create_lut3d_op_from_data(&mut ops, &lut_a, TransformDirection::Forward).unwrap();
    create_lut3d_op_from_data(&mut ops, &lut_a, TransformDirection::Inverse).unwrap();
    create_lut3d_op_from_data(&mut ops, &lut_b, TransformDirection::Forward).unwrap();
    create_lut3d_op_from_data(&mut ops, &lut_b, TransformDirection::Inverse).unwrap();
    assert_eq!(ops.len(), 4);

    let cloned: OpRc = Arc::from(ops[3].clone_box());
    let d = |o: &OpRc| lut_op(o).data().clone();
    assert!(d(&ops[0]).is_inverse(&d(&ops[1])));
    assert!(!d(&ops[0]).is_inverse(&d(&ops[2])));
    assert!(!d(&ops[0]).is_inverse(&d(&ops[3])));
    assert!(d(&ops[2]).is_inverse(&d(&ops[3])));
    assert!(d(&ops[2]).is_inverse(&d(&cloned)));
}

#[test]
fn op_generate_identity_throw() {
    let n = 3;
    let mut lut = vec![0.0f32; n * n * n * 3];
    assert_err_contains(
        generate_identity_lut3d(&mut lut, n, 2, Lut3DOrder::FastRed),
        "less than 3 channels",
    );
    assert_err_contains(
        get_3d_lut_edge_len_from_num_pixels(10),
        "Cannot infer 3D LUT size",
    );
    assert!(generate_identity_lut3d(&mut lut[1..], n, 3, Lut3DOrder::FastRed).is_err());
}

#[test]
fn op_create_op() {
    let lut = Lut3DOpData::new(3).unwrap();
    let mut ops = OpVec::new();
    create_lut3d_op_from_data(&mut ops, &lut, TransformDirection::Inverse).unwrap();
    assert_eq!(ops.len(), 1);
    // Inverse is fine.
    let mut px = [[0.2, 0.4, 0.6, 1.0]];
    ops[0].apply(&mut px);
    for (a, e) in px[0].iter().zip([0.2, 0.4, 0.6, 1.0]) {
        assert_close(*a, e, 1e-6);
    }
}

#[test]
fn op_cache_id() {
    let mut ops = OpVec::new();
    for _ in 0..2 {
        let lut = Lut3DOpData::new(3).unwrap();
        create_lut3d_op_from_data(&mut ops, &lut, TransformDirection::Forward).unwrap();
    }
    assert_eq!(ops.len(), 2);
    let id0 = ops[0].cache_id();
    let id1 = ops[1].cache_id();
    assert!(!id0.is_empty());
    // Identical LUTs have the same cache id.
    assert_eq!(id0, id1);
    assert!(id0.starts_with("<Lut3D "));
    assert!(id0.ends_with("default forward >"));
}

#[test]
fn op_edge_len_from_num_pixels() {
    assert_err_contains(
        get_3d_lut_edge_len_from_num_pixels(10),
        "Cannot infer 3D LUT size",
    );
    assert_eq!(
        get_3d_lut_edge_len_from_num_pixels(33 * 33 * 33).unwrap(),
        33
    );
    assert_eq!(
        get_3d_lut_edge_len_from_num_pixels(1290 * 1290 * 1290).unwrap(),
        1290
    );
}

#[test]
fn op_lut3d_order() {
    let n = 3;
    let mut lut = vec![0.0f32; n * n * n * 3];

    generate_identity_lut3d(&mut lut, n, 3, Lut3DOrder::FastRed).unwrap();
    // First 3 values have red changing.
    assert_eq!([lut[0], lut[3], lut[6]], [0.0, 0.5, 1.0]);
    // Blue is all 0.
    assert_eq!([lut[2], lut[5], lut[8]], [0.0, 0.0, 0.0]);
    // Last 3 values have red changing.
    assert_eq!([lut[72], lut[75], lut[78]], [0.0, 0.5, 1.0]);
    // Blue is all 1.
    assert_eq!([lut[74], lut[77], lut[80]], [1.0, 1.0, 1.0]);

    generate_identity_lut3d(&mut lut, n, 3, Lut3DOrder::FastBlue).unwrap();
    // First 3 values have blue changing.
    assert_eq!([lut[2], lut[5], lut[8]], [0.0, 0.5, 1.0]);
    // Red is all 0.
    assert_eq!([lut[0], lut[3], lut[6]], [0.0, 0.0, 0.0]);
    // Last 3 values have blue changing.
    assert_eq!([lut[74], lut[77], lut[80]], [0.0, 0.5, 1.0]);
    // Red is all 1.
    assert_eq!([lut[72], lut[75], lut[78]], [1.0, 1.0, 1.0]);

    // 4 channels.
    let mut lut4 = vec![-1.0f32; n * n * n * 4];
    generate_identity_lut3d(&mut lut4, n, 4, Lut3DOrder::FastBlue).unwrap();
    assert_eq!(&lut4[4..8], &[0.0, 0.0, 0.5, -1.0]);
}

#[test]
fn opdata_lut_order() {
    let lb = Lut3DOpData::new(3).unwrap();
    let v = lb.array().values();
    // First 3 values have blue changing.
    assert_eq!([v[2], v[5], v[8]], [0.0, 0.5, 1.0]);
    // Red is all 0.
    assert_eq!([v[0], v[3], v[6]], [0.0, 0.0, 0.0]);
    // Last 3 values have blue changing.
    assert_eq!([v[74], v[77], v[80]], [0.0, 0.5, 1.0]);
    // Red is all 1.
    assert_eq!([v[72], v[75], v[78]], [1.0, 1.0, 1.0]);
}

#[test]
fn opdata_lut_combine() {
    let lut1 = Lut3DOpData::new(3).unwrap();
    let lut2 = Lut3DOpData::new(5).unwrap();

    let mut ops = OpVec::new();
    create_lut3d_op_from_data(&mut ops, &lut1, TransformDirection::Forward).unwrap();
    create_lut3d_op_from_data(&mut ops, &lut2, TransformDirection::Forward).unwrap();
    create_lut3d_op_from_data(&mut ops, &lut1, TransformDirection::Inverse).unwrap();
    create_lut3d_op_from_data(&mut ops, &lut2, TransformDirection::Inverse).unwrap();
    // Another op type (a 1D LUT here).
    create_lut1d_op_from_data(
        &mut ops,
        &Lut1DOpData::new(4).unwrap(),
        TransformDirection::Forward,
    )
    .unwrap();
    assert_eq!(ops.len(), 5);

    let flags = OptimizationFlags::COMP_LUT3D;
    // LUT 3D can combine with other LUT 3D.
    assert!(ops[0].combine_with(ops[1].as_ref(), flags).is_some());
    assert!(ops[0].combine_with(ops[2].as_ref(), flags).is_some());
    assert!(ops[2].combine_with(ops[3].as_ref(), flags).is_some());
    assert!(ops[2].combine_with(ops[0].as_ref(), flags).is_some());

    // LUT 3D can't combine with other ops.
    assert!(ops[0].combine_with(ops[4].as_ref(), flags).is_none());
    assert!(ops[2].combine_with(ops[4].as_ref(), flags).is_none());
    assert!(ops[4]
        .combine_with(ops[0].as_ref(), OptimizationFlags::ALL)
        .is_none());

    // Not without the flag.
    assert!(ops[0]
        .combine_with(ops[1].as_ref(), OptimizationFlags::DEFAULT)
        .is_none());
}

#[test]
fn op_optimize_compose() {
    let lut1 = synthetic_lut(9);
    let lut2 = synthetic_lut(5);
    let mut ops = OpVec::new();
    create_lut3d_op_from_data(&mut ops, &lut1, TransformDirection::Forward).unwrap();
    create_lut3d_op_from_data(&mut ops, &lut2, TransformDirection::Forward).unwrap();

    // COMP_LUT3D is not part of the default optimization.
    assert_eq!(optimize_ops(&ops, OptimizationFlags::DEFAULT).len(), 2);
    let opt = optimize_ops(&ops, OptimizationFlags::GOOD);
    assert_eq!(opt.len(), 1);
    assert_eq!(lut_op(&opt[0]).data().array().length(), 9);
}

#[test]
fn op_inverse_pair_optimized_to_range() {
    let lut = synthetic_lut(5);
    let mut ops = OpVec::new();
    create_lut3d_op_from_data(&mut ops, &lut, TransformDirection::Forward).unwrap();
    create_lut3d_op_from_data(&mut ops, &lut, TransformDirection::Inverse).unwrap();
    // A forward + inverse pair is replaced by a clamp.
    let opt = optimize_ops(&ops, OptimizationFlags::DEFAULT);
    assert_eq!(opt.len(), 1);
    assert_eq!(opt[0].name(), "Range");
}

#[test]
fn op_cpu_renderer_lut3d() {
    // An identity LUT.
    let mut lut_data = Lut3DOpData::with_interpolation(Interpolation::Linear, 33).unwrap();
    assert!(lut_data.validate().is_ok());
    assert!(!lut_data.is_identity());

    // Input values exactly at a grid point: the output is the grid value,
    // regardless of the interpolation.
    let step = 1.0f32 / (lut_data.array().length() as f32 - 1.0);
    let mut px = [[0.0, 0.0, 0.0, 0.0], [0.0, 0.0, step, 1.0]];
    render(&lut_data, &mut px);
    assert_eq!(px, [[0.0, 0.0, 0.0, 0.0], [0.0, 0.0, step, 1.0]]);

    // No more an identity LUT.
    const ARBITRARY: f32 = 0.123456;
    lut_data.array_mut()[5] = ARBITRARY;
    let op = Lut3DOp::new(lut_data.clone()).unwrap();
    assert!(!op.is_no_op());
    let mut px = [[0.0, 0.0, 0.0, 0.0], [0.0, 0.0, step, 1.0]];
    op.apply(&mut px);
    assert_eq!(px, [[0.0, 0.0, 0.0, 0.0], [0.0, 0.0, ARBITRARY, 1.0]]);

    // Change interpolation.
    lut_data.set_interpolation(Interpolation::Tetrahedral);
    assert!(lut_data.validate().is_ok());
    let mut px = [[0.0, 0.0, 0.0, 0.0], [0.0, 0.0, step, 1.0]];
    render(&lut_data, &mut px);
    assert_eq!(px, [[0.0, 0.0, 0.0, 0.0], [0.0, 0.0, ARBITRARY, 1.0]]);
}

#[test]
fn op_cpu_renderer_cloned() {
    // Cloned ops produce the exact same results.
    let mut lut = synthetic_lut(17);
    lut.set_file_output_bit_depth(BitDepth::UInt12);
    let op = Lut3DOp::new(lut.clone()).unwrap();
    assert_eq!(op.data().file_output_bit_depth(), BitDepth::UInt12);
    let cloned = op.clone_box();
    let cloned_cloned = cloned.clone_box();
    let op_from_data = Lut3DOp::new(op.data().clone()).unwrap();

    let input = [
        [0.1, 0.25, 0.7, 0.0],
        [0.66, 0.25, 0.81, 0.5],
        [0.18, 0.99, 0.45, 1.0],
    ];
    let mut a = input;
    let mut b = input;
    let mut c = input;
    let mut d = input;
    op.apply(&mut a);
    cloned.apply(&mut b);
    cloned_cloned.apply(&mut c);
    op_from_data.apply(&mut d);
    assert_eq!(a, b);
    assert_eq!(a, c);
    assert_eq!(a, d);
}

#[test]
fn op_cpu_renderer_inverse() {
    // Synthetic version of the inversed ops test: the inversion is based on
    // tetrahedral interpolation, so the forward evals are also tetrahedral.
    let mut fwd = synthetic_lut(17);
    fwd.set_interpolation(Interpolation::Tetrahedral);
    let fwd_op = Lut3DOp::new(fwd.clone()).unwrap();

    let input = [
        [0.1, 0.25, 0.7, 0.0],
        [0.66, 0.25, 0.81, 0.5],
        [0.18, 0.99, 0.45, 1.0],
    ];
    let mut buffer = input;
    fwd_op.apply(&mut buffer);
    let out1 = buffer;

    // Step 1: forward and inverse ops produce the right results (EXACT).
    let inv = fwd.inverse();
    let inv_op = Lut3DOp::new(inv.clone()).unwrap();
    inv_op.apply(&mut buffer);
    for i in 0..3 {
        for c in 0..3 {
            assert_close(buffer[i][c], input[i][c], 1e-4);
        }
        assert_eq!(buffer[i][3], input[i][3]);
    }
    // Another forward apply lands in the same place.
    fwd_op.apply(&mut buffer);
    for i in 0..3 {
        for c in 0..4 {
            assert_close(buffer[i][c], out1[i][c], 1e-6);
        }
    }

    // Step 2: repeat with the FAST inverse.
    let mut buffer = out1;
    let fast = make_fast_lut3d_from_inverse(&inv).unwrap();
    let fast_op = Lut3DOp::new(fast).unwrap();
    fast_op.apply(&mut buffer);
    fwd_op.apply(&mut buffer);
    // The FAST inverse is not exact: use a loose tolerance.
    for i in 0..3 {
        for c in 0..4 {
            assert_close(buffer[i][c], out1[i][c], 0.015);
        }
    }

    // Step 3: clamping of large values in EXACT mode.
    let mut buffer = out1;
    buffer[0][0] = 100.0;
    inv_op.apply(&mut buffer[..1]);
    // Extreme large values get inverted (no inverse would return zeros).
    assert!(buffer[0][0] > 0.5);
}

#[test]
fn op_cpu_renderer_inverse_grid_sizes() {
    // The exact inverse works for various grid sizes (range tree depths).
    for n in [2, 3, 4, 5, 9, 33] {
        let mut fwd = Lut3DOpData::with_interpolation(Interpolation::Tetrahedral, n).unwrap();
        for v in fwd.array_mut().values_mut().iter_mut() {
            *v = v.powf(1.5);
        }
        let input = [
            [0.1, 0.25, 0.7, 0.0],
            [0.66, 0.25, 0.81, 0.5],
            [0.0, 1.0, 0.45, 1.0],
        ];
        let mut buffer = input;
        render(&fwd, &mut buffer);
        let out1 = buffer;
        render(&fwd.inverse(), &mut buffer);
        render(&fwd, &mut buffer);
        for i in 0..3 {
            for c in 0..3 {
                assert_close(buffer[i][c], out1[i][c], 1e-5);
            }
        }
    }
}

#[test]
fn op_cpu_renderer_lut3d_with_nan() {
    let lut = Lut3DOpData::new(17).unwrap();
    let qnan = f32::NAN;
    let mut px = [
        [qnan, 0.25, 0.25, 0.0],
        [0.25, qnan, 0.25, 0.0],
        [0.25, 0.25, qnan, 0.0],
        [0.25, 0.25, 0.0, qnan],
        [0.5, 0.5, 0.5, 0.0],
    ];
    render(&lut, &mut px);
    assert_eq!(px[0], [0.0, 0.25, 0.25, 0.0]);
    assert_eq!(px[1][1], 0.0);
    assert_eq!(px[2][2], 0.0);
    assert!(px[3][3].is_nan());
    assert_eq!(px[4], [0.5, 0.5, 0.5, 0.0]);

    // Inverse with NaN: NaNs are clamped to 0.
    let mut px = [[qnan, 0.25, 0.25, 0.0]];
    render(&lut.inverse(), &mut px);
    assert_close(px[0][0], 0.0, 1e-6);
    assert_close(px[0][1], 0.25, 1e-6);
}

#[test]
fn op_replace_inverse_luts() {
    let lut = synthetic_lut(5);
    let mut ops = OpVec::new();
    create_lut3d_op_from_data(&mut ops, &lut, TransformDirection::Inverse).unwrap();
    assert_eq!(
        replace_inverse_luts(&mut ops, OptimizationFlags::DEFAULT).unwrap(),
        1
    );
    let fast = lut_op(&ops[0]).data();
    assert_eq!(fast.direction(), TransformDirection::Forward);
    assert_eq!(fast.array().length(), FAST_INVERSE_GRID_SIZE);
}

#[test]
fn op_create_transform() {
    let mut lut = Lut3DOpData::new(3).unwrap();
    lut.set_file_output_bit_depth(BitDepth::UInt10);
    lut.array_mut()[39] = 0.61;
    lut.array_mut()[40] = 0.52;
    lut.array_mut()[41] = 0.74;
    lut.format_metadata_mut()
        .add_attribute(METADATA_NAME, "test");

    let mut ops = OpVec::new();
    create_lut3d_op_from_data(&mut ops, &lut, TransformDirection::Forward).unwrap();
    assert_eq!(ops.len(), 1);

    let t = match ops[0].to_transform() {
        Some(Transform::Lut3D(t)) => t,
        t => panic!("unexpected transform {t:?}"),
    };
    assert_eq!(t.metadata.attributes.len(), 1);
    assert_eq!(t.metadata.attributes[0].0, METADATA_NAME);
    assert_eq!(t.metadata.attributes[0].1, "test");
    assert_eq!(t.direction, TransformDirection::Forward);
    assert_eq!(t.grid_size, 3);
    assert_eq!(t.file_output_bit_depth, BitDepth::UInt10);
    assert_eq!(t.value(1, 1, 1), [0.61, 0.52, 0.74]);
}

#[test]
fn transform_build_op() {
    let mut lut = Lut3DTransform::default();
    let gs = 4;
    lut.set_grid_size(gs);
    let (ri, gi, bi) = (1, 2, 3);
    lut.set_value(ri, gi, bi, [0.51, 0.52, 0.53]);

    let config = Config::create_raw();
    let mut ops = OpVec::new();
    crate::transforms::build::build_ops(
        &mut ops,
        &config,
        config.current_context(),
        &Transform::Lut3D(lut),
        TransformDirection::Forward,
    )
    .unwrap();
    assert_eq!(ops.len(), 1);
    let data = lut_op(&ops[0]).data();

    // Blue fast.
    let i = 3 * ((ri * gs + gi) * gs + bi);
    assert_eq!(data.array().length(), gs);
    assert_eq!(data.array()[i], 0.51);
    assert_eq!(data.array()[i + 1], 0.52);
    assert_eq!(data.array()[i + 2], 0.53);

    // Direction handling.
    let mut ops = OpVec::new();
    let mut lut = Lut3DTransform::new(2);
    lut.direction = TransformDirection::Inverse;
    create_lut3d_op(&mut ops, &lut, TransformDirection::Inverse).unwrap();
    assert_eq!(
        lut_op(&ops[0]).data().direction(),
        TransformDirection::Forward
    );
}

// ---------------------------------------------------------------------------
// Lut3DTransform

#[test]
fn transform_basic() {
    let mut lut = Lut3DTransform::default();
    assert_eq!(lut.grid_size, 2);
    assert_eq!(lut.direction, TransformDirection::Forward);
    assert_eq!(lut.value(0, 0, 0), [0.0, 0.0, 0.0]);
    assert_eq!(lut.value(0, 1, 1), [0.0, 1.0, 1.0]);
    assert_eq!(lut.value(1, 0, 0), [1.0, 0.0, 0.0]);

    lut.direction = TransformDirection::Inverse;
    lut.set_grid_size(3);
    assert_eq!(lut.grid_size, 3);
    assert_eq!(lut.value(0, 0, 0), [0.0, 0.0, 0.0]);
    assert_eq!(lut.value(0, 1, 1), [0.0, 0.5, 0.5]);
    assert_eq!(lut.value(2, 0, 2), [1.0, 0.0, 1.0]);
    assert_eq!(lut.value(0, 1, 2), [0.0, 0.5, 1.0]);

    lut.set_value(0, 1, 2, [0.1, 0.52, 0.93]);
    assert_eq!(lut.value(0, 1, 2), [0.1, 0.52, 0.93]);

    assert_eq!(lut.file_output_bit_depth, BitDepth::Unknown);
    lut.file_output_bit_depth = BitDepth::UInt8;
    // File out bit-depth does not affect values.
    assert_eq!(lut.value(0, 1, 2), [0.1, 0.52, 0.93]);

    assert!(lut.validate().is_ok());

    lut.set_value(0, 0, 0, [-0.2, -0.1, -0.3]);
    lut.set_value(2, 2, 2, [1.2, 1.3, 1.8]);

    assert_eq!(
        lut.to_string(),
        "<Lut3DTransform direction=inverse, fileoutdepth=8ui, interpolation=default, gridSize=3, \
         minrgb=[-0.2, -0.1, -0.3], maxrgb=[1.2, 1.3, 1.8]>"
    );

    let mut too_big = lut.clone();
    too_big.grid_size = 200;
    assert_err_contains(too_big.validate(), "must not be greater than '129'");

    let mut bad = lut.clone();
    bad.values.pop();
    assert_err_contains(
        bad.validate(),
        "Lut3DTransform validation failed: Lut3D content array issue",
    );

    let mut bad = lut;
    bad.interpolation = Interpolation::Cubic;
    assert_err_contains(
        bad.validate(),
        "does not support interpolation algorithm: cubic",
    );
}

#[test]
fn transform_create_with_parameters() {
    let lut = Lut3DTransform::new(8);
    assert_eq!(lut.grid_size, 8);
    assert_eq!(lut.direction, TransformDirection::Forward);
    assert_eq!(lut.interpolation, Interpolation::Default);
    assert_eq!(lut.value(7, 7, 7), [1.0, 1.0, 1.0]);
}
