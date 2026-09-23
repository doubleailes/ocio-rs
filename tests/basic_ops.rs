//! Integration tests of the basic ops (matrix, range, exponent, gamma, log,
//! allocation, exposure contrast, CDL): processors built from transforms and
//! optimizer behavior (`ocio::processor::optimize_ops`).

use ocio::ops::cdl::CdlOp;
use ocio::ops::gamma::GammaOp;
use ocio::ops::log::LogOp;
use ocio::ops::matrix::MatrixOp;
use ocio::ops::range::RangeOp;
use ocio::ops::{apply_ops, OpVec, Pixel};
use ocio::processor::optimize_ops;
use ocio::*;

fn processor(t: impl Into<Transform>, dir: TransformDirection) -> Processor {
    let config = Config::create_raw();
    let ctx = Context::new();
    Processor::from_transform(&config, &ctx, &t.into(), dir).unwrap()
}

fn group(ts: Vec<Transform>) -> GroupTransform {
    GroupTransform::from_transforms(ts)
}

fn apply(ops: &[ocio::ops::OpRc], px: Pixel) -> Pixel {
    let mut p = [px];
    apply_ops(ops, &mut p);
    p[0]
}

fn assert_pixel_close(a: Pixel, b: Pixel, tol: f32) {
    for i in 0..4 {
        assert!(
            (a[i] - b[i]).abs() <= tol,
            "{a:?} != {b:?} (tolerance {tol})"
        );
    }
}

const M1: [f64; 16] = [
    1.1, 0.2, 0.3, 0.4, 0.5, 1.6, 0.7, 0.8, 0.2, 0.1, 1.1, 0.2, 0.3, 0.4, 0.5, 1.6,
];

#[test]
fn two_matrices_combine() {
    let g = group(vec![
        MatrixTransform::new(M1, [0.1, 0.2, 0.3, 0.0]).into(),
        MatrixTransform::from_m33(&[0.5, 0.1, 0.0, 0.2, 0.7, 0.1, 0.0, 0.0, 1.2]).into(),
    ]);
    let p = processor(g, TransformDirection::Forward);
    assert_eq!(p.ops().len(), 2);
    let opt = optimize_ops(p.ops(), OptimizationFlags::DEFAULT);
    assert_eq!(opt.len(), 1);
    assert!(opt[0].downcast_ref::<MatrixOp>().is_some());
    let src = [0.3, 0.6, 0.9, 1.0];
    assert_pixel_close(apply(p.ops(), src), apply(&opt, src), 1e-5);
    // Not combined without COMP_MATRIX.
    let flags = OptimizationFlags::DEFAULT & !OptimizationFlags::COMP_MATRIX;
    assert_eq!(optimize_ops(p.ops(), flags).len(), 2);
}

#[test]
fn matrix_and_inverse_are_removed() {
    let m = MatrixTransform::new(M1, [0.1, 0.2, 0.3, 0.4]);
    let g = group(vec![m.clone().into(), m.inverted_transform()]);
    let p = processor(g, TransformDirection::Forward);
    assert_eq!(p.ops().len(), 2);
    assert!(optimize_ops(p.ops(), OptimizationFlags::DEFAULT).is_empty());
    assert!(p.default_cpu_processor().is_no_op());
}

trait Inverted {
    fn inverted_transform(&self) -> Transform;
}

impl<T: Clone + Into<Transform>> Inverted for T {
    fn inverted_transform(&self) -> Transform {
        self.clone().into().inverted()
    }
}

#[test]
fn inverse_log_pair_removed() {
    let log = LogTransform::new(10.0);
    // Log followed by antilog: a clamp of the negative values remains.
    let p = processor(
        group(vec![log.clone().into(), log.inverted_transform()]),
        TransformDirection::Forward,
    );
    let opt = optimize_ops(p.ops(), OptimizationFlags::DEFAULT);
    assert_eq!(opt.len(), 1);
    assert!(opt[0].downcast_ref::<RangeOp>().is_some());
    // Antilog followed by log: removed.
    let p = processor(
        group(vec![log.inverted_transform(), log.clone().into()]),
        TransformDirection::Forward,
    );
    assert!(optimize_ops(p.ops(), OptimizationFlags::DEFAULT).is_empty());
    // Not removed without PAIR_IDENTITY_LOG.
    let flags = OptimizationFlags::DEFAULT & !OptimizationFlags::PAIR_IDENTITY_LOG;
    assert_eq!(optimize_ops(p.ops(), flags).len(), 2);
}

#[test]
fn nested_inverse_pairs_removed() {
    // Matrix -> LogAffine -> Exponent(mirror) -> inverses, in reverse order.
    let inner = group(vec![
        MatrixTransform::new(M1, [0.0; 4]).into(),
        LogAffineTransform {
            base: 10.0,
            log_side_slope: [0.3; 3],
            lin_side_slope: [2.0; 3],
            lin_side_offset: [0.1; 3],
            log_side_offset: [0.5; 3],
            ..Default::default()
        }
        .into(),
        ExponentTransform {
            value: [2.2, 2.2, 2.2, 1.0],
            negative_style: NegativeStyle::Mirror,
            ..Default::default()
        }
        .into(),
    ]);
    let mut inv = inner.clone();
    inv.direction = TransformDirection::Inverse;
    let p = processor(
        group(vec![inv.into(), inner.into()]),
        TransformDirection::Forward,
    );
    assert_eq!(p.ops().len(), 6);
    // Exponent pair removed, then log (log-to-lin then lin-to-log) removed,
    // then the matrices.
    assert!(optimize_ops(p.ops(), OptimizationFlags::DEFAULT).is_empty());
    // With no optimization, the ops round trip (within the log domain).
    let src = [0.4, 0.5, 0.6, 1.0];
    assert_pixel_close(apply(p.ops(), src), src, 1e-4);
}

#[test]
fn ranges_combine_and_no_clamp_range_is_a_matrix() {
    let r1 = RangeTransform::new(Some(0.0), Some(1.0), Some(0.5), Some(1.5));
    let r2 = RangeTransform::new(Some(0.6), Some(1.4), Some(0.0), Some(1.0));
    let p = processor(
        group(vec![r1.clone().into(), r2.into()]),
        TransformDirection::Forward,
    );
    let opt = optimize_ops(p.ops(), OptimizationFlags::DEFAULT);
    assert_eq!(opt.len(), 1);
    assert!(opt[0].downcast_ref::<RangeOp>().is_some());
    for v in [-0.5f32, 0.0, 0.1, 0.35, 0.9, 1.2] {
        let src = [v, v, v, 1.0];
        assert_pixel_close(apply(p.ops(), src), apply(&opt, src), 1e-6);
    }

    let mut nc = r1;
    nc.style = RangeStyle::NoClamp;
    let p = processor(nc, TransformDirection::Forward);
    assert_eq!(p.ops().len(), 1);
    assert!(p.ops()[0].downcast_ref::<MatrixOp>().is_some());
    assert_pixel_close(
        apply(p.ops(), [2.0, -1.0, 0.25, 0.5]),
        [2.5, -0.5, 0.75, 0.5],
        1e-6,
    );
}

#[test]
fn gamma_compose_and_identity() {
    let e1 = ExponentTransform::new([2.0, 2.0, 2.0, 1.0]);
    let e2 = ExponentTransform::new([0.5, 0.5, 0.5, 1.0]);
    let p = processor(
        group(vec![e1.into(), e2.into()]),
        TransformDirection::Forward,
    );
    let opt = optimize_ops(p.ops(), OptimizationFlags::DEFAULT);
    // Composed into an identity basic gamma, which still clamps: replaced by a range.
    assert_eq!(opt.len(), 1);
    let r = opt[0].downcast_ref::<RangeOp>().unwrap().data();
    assert_eq!(r.min_in, 0.0);
    assert!(r.max_is_empty());
    assert_pixel_close(
        apply(&opt, [-0.5, 0.5, 2.0, 1.0]),
        [0.0, 0.5, 2.0, 1.0],
        1e-6,
    );

    let e3 = ExponentTransform::new([1.8, 2.0, 2.2, 1.0]);
    let e4 = ExponentTransform::new([1.1, 1.1, 1.1, 1.0]);
    let p = processor(
        group(vec![e3.into(), e4.into()]),
        TransformDirection::Forward,
    );
    let opt = optimize_ops(p.ops(), OptimizationFlags::DEFAULT);
    assert_eq!(opt.len(), 1);
    assert!(opt[0].downcast_ref::<GammaOp>().is_some());
    assert_pixel_close(
        apply(p.ops(), [0.3, 0.6, 0.9, 1.0]),
        apply(&opt, [0.3, 0.6, 0.9, 1.0]),
        1e-6,
    );
}

#[test]
fn cdl_simplified_into_matrices() {
    let cdl = CdlTransform {
        slope: [1.2, 1.1, 0.9],
        offset: [0.01, 0.02, -0.03],
        sat: 0.8,
        ..Default::default()
    };
    let p = processor(cdl.clone(), TransformDirection::Forward);
    assert_eq!(p.ops().len(), 1);
    assert!(p.ops()[0].downcast_ref::<CdlOp>().is_some());
    // The CDL is replaced by two matrices, which are then combined.
    let opt = optimize_ops(p.ops(), OptimizationFlags::DEFAULT);
    assert_eq!(opt.len(), 1);
    assert!(opt[0].downcast_ref::<MatrixOp>().is_some());
    let src = [0.2, 0.5, 0.7, 1.0];
    assert_pixel_close(apply(p.ops(), src), apply(&opt, src), 1e-5);
    // Kept without SIMPLIFY_OPS.
    let flags = OptimizationFlags::DEFAULT & !OptimizationFlags::SIMPLIFY_OPS;
    assert!(optimize_ops(p.ops(), flags)[0]
        .downcast_ref::<CdlOp>()
        .is_some());

    // A CDL followed by its inverse is removed.
    let p = processor(
        group(vec![cdl.clone().into(), cdl.inverted_transform()]),
        TransformDirection::Forward,
    );
    assert!(optimize_ops(p.ops(), OptimizationFlags::DEFAULT).is_empty());
}

#[test]
fn allocation_round_trip() {
    let al = AllocationTransform {
        allocation: Allocation::Lg2,
        vars: vec![-10.0, 6.0],
        ..Default::default()
    };
    let p = processor(
        group(vec![al.clone().into(), al.inverted_transform()]),
        TransformDirection::Forward,
    );
    assert_eq!(p.ops().len(), 4);
    let src = [0.18, 1.0, 10.0, 1.0];
    assert_pixel_close(apply(p.ops(), src), src, 1e-4);
    let opt = optimize_ops(p.ops(), OptimizationFlags::DEFAULT);
    // Log then antilog: only the clamp of negative values remains.
    assert_eq!(opt.len(), 1);
    assert!(opt[0].downcast_ref::<RangeOp>().is_some());
}

#[test]
fn exposure_contrast_dynamic_processor() {
    let ec = ExposureContrastTransform {
        exposure: 0.0,
        exposure_dynamic: true,
        ..Default::default()
    };
    let p = processor(
        group(vec![ec.clone().into(), ec.into()]),
        TransformDirection::Forward,
    );
    assert!(p.is_dynamic());
    let cpu = p.default_cpu_processor();
    // Dynamic ops are not removed.
    assert_eq!(cpu.ops().len(), 2);
    let dp = cpu.dynamic_property(DynamicPropertyType::Exposure).unwrap();
    dp.as_double().unwrap().set(1.0);
    let mut px = [0.25f32, 0.5, 1.0, 1.0];
    cpu.apply_rgba(&mut px);
    assert_eq!(px, [1.0, 2.0, 4.0, 1.0]);
}

#[test]
fn group_transform_round_trip() {
    let g = group(vec![
        MatrixTransform::new(M1, [0.1, 0.0, 0.0, 0.0]).into(),
        LogCameraTransform::new([0.1; 3]).into(),
        ExponentWithLinearTransform {
            gamma: [2.4, 2.4, 2.4, 1.0],
            offset: [0.055, 0.055, 0.055, 0.0],
            ..Default::default()
        }
        .into(),
        CdlTransform {
            slope: [1.1; 3],
            power: [1.2; 3],
            ..Default::default()
        }
        .into(),
        RangeTransform::new(Some(0.0), Some(1.0), Some(0.0), Some(1.0)).into(),
        ExposureContrastTransform {
            exposure: 0.5,
            ..Default::default()
        }
        .into(),
    ]);
    let p = processor(g, TransformDirection::Forward);
    assert_eq!(p.ops().len(), 6);
    let back = p.create_group_transform();
    assert_eq!(back.num_transforms(), 6);
    let p2 = processor(back, TransformDirection::Forward);
    let src = [0.3, 0.45, 0.6, 1.0];
    assert_pixel_close(apply(p.ops(), src), apply(p2.ops(), src), 1e-6);
    let ids1: Vec<String> = p.ops().iter().map(|o| o.cache_id()).collect();
    let ids2: Vec<String> = p2.ops().iter().map(|o| o.cache_id()).collect();
    assert_eq!(ids1, ids2);
}

#[test]
fn inverse_processor() {
    let g = group(vec![
        MatrixTransform::new(M1, [0.1, 0.0, 0.0, 0.0]).into(),
        LogAffineTransform {
            lin_side_offset: [0.5; 3],
            ..Default::default()
        }
        .into(),
        ExponentWithLinearTransform {
            gamma: [2.4, 2.4, 2.4, 1.0],
            offset: [0.055, 0.055, 0.055, 0.0],
            negative_style: NegativeStyle::Mirror,
            ..Default::default()
        }
        .into(),
    ]);
    let fwd = processor(g.clone(), TransformDirection::Forward);
    let inv = processor(g, TransformDirection::Inverse);
    let src = [0.3, 0.45, 0.6, 1.0];
    let mut ops: OpVec = fwd.ops().to_vec();
    ops.extend(inv.ops().iter().cloned());
    assert_pixel_close(apply(&ops, src), src, 1e-5);
    assert!(optimize_ops(&ops, OptimizationFlags::DEFAULT)
        .iter()
        .all(|o| o.downcast_ref::<LogOp>().is_none()));
}

#[test]
fn validation_errors_are_reported() {
    let config = Config::create_raw();
    let t: Transform = MatrixTransform {
        direction: TransformDirection::Inverse,
        matrix: [0.0; 16],
        ..Default::default()
    }
    .into();
    let e = config
        .get_processor_for_transform(&t, TransformDirection::Forward)
        .unwrap_err();
    assert_eq!(
        e.message(),
        "MatrixTransform validation failed: Singular Matrix can't be inverted."
    );

    let t: Transform = RangeTransform::default().into();
    assert!(t.validate().is_err());
    let t: Transform = LogTransform::new(1.0).into();
    assert!(config
        .get_processor_for_transform(&t, TransformDirection::Forward)
        .is_err());
    let t: Transform = ExponentTransform {
        negative_style: NegativeStyle::Linear,
        ..Default::default()
    }
    .into();
    assert!(t.validate().is_err());
    let t: Transform = AllocationTransform {
        allocation: Allocation::Uniform,
        vars: vec![1.0],
        ..Default::default()
    }
    .into();
    assert!(t.validate().is_err());
}

// Tests needing the other packages.

#[test]
fn v1_config_uses_legacy_exponent_and_cdl() {
    // In a v1 config, an exponent does not honor the negative style and an
    // identity exponent is a no-op (the clamp is lost).
    let yaml = r#"ocio_profile_version: 1

roles:
  default: raw

displays:
  sRGB:
    - !<View> {name: Raw, colorspace: raw}

colorspaces:
  - !<ColorSpace>
    name: raw
    isdata: true

  - !<ColorSpace>
    name: exp
    to_reference: !<ExponentTransform> {value: [2.2, 2.2, 2.2, 1]}

  - !<ColorSpace>
    name: cdl
    to_reference: !<CDLTransform> {slope: [1.1, 1, 1], power: [1.2, 1, 1]}
"#;
    let config = Config::create_from_str(yaml).unwrap();
    let t: Transform = ExponentTransform::new([2.2, 2.2, 2.2, 1.0]).into();
    let p = config
        .get_processor_for_transform(&t, TransformDirection::Forward)
        .unwrap();
    assert!(p.ops()[0]
        .downcast_ref::<ocio::ops::exponent::ExponentOp>()
        .is_some());
    let t: Transform = CdlTransform {
        slope: [1.1, 1.0, 1.0],
        power: [1.2, 1.0, 1.0],
        ..Default::default()
    }
    .into();
    let p = config
        .get_processor_for_transform(&t, TransformDirection::Forward)
        .unwrap();
    // Legacy CDL: slope matrix and exponent (the identity offset / saturation
    // matrices are kept until the processor is optimized).
    assert_eq!(p.ops().iter().filter(|o| !o.is_no_op()).count(), 2);
}

#[test]
fn color_space_conversion_with_basic_ops() {
    let yaml = r#"ocio_profile_version: 2

roles:
  default: raw
  scene_linear: lin

displays:
  sRGB:
    - !<View> {name: Raw, colorspace: raw}

colorspaces:
  - !<ColorSpace>
    name: raw
    isdata: true

  - !<ColorSpace>
    name: lin

  - !<ColorSpace>
    name: log
    from_scene_reference: !<GroupTransform>
      children:
        - !<MatrixTransform> {offset: [0.01, 0.01, 0.01, 0]}
        - !<LogTransform> {base: 2}
"#;
    let config = Config::create_from_str(yaml).unwrap();
    let p = config.get_processor("lin", "log").unwrap();
    let cpu = p.default_cpu_processor();
    let mut px = [0.99f32, 0.0, 0.0];
    cpu.apply_rgb(&mut px);
    assert!((px[0] - 0.0).abs() < 1e-6);
    let back = config
        .get_processor("log", "lin")
        .unwrap()
        .default_cpu_processor();
    back.apply_rgb(&mut px);
    assert!((px[0] - 0.99).abs() < 1e-5);
}
