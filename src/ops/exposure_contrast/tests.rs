//! Unit tests of the exposure contrast op (ports of
//! `ExposureContrastOpData_tests.cpp`, `ExposureContrastOp_tests.cpp`,
//! `ExposureContrastOpCPU_tests.cpp` and `ExposureContrastTransform_tests.cpp`).

use super::*;
use crate::ops::matrix::test_utils::*;
use crate::processor::{optimize_ops, Processor};

const QNAN: f32 = f32::NAN;
const INF: f32 = f32::INFINITY;

// ExposureContrastOpData_tests.cpp

#[test]
fn data_style() {
    assert_eq!(EcStyle::parse(EC_STYLE_LINEAR).unwrap(), EcStyle::Linear);
    assert_eq!(
        EcStyle::parse(EC_STYLE_LINEAR_REV).unwrap(),
        EcStyle::LinearRev
    );
    assert_eq!(EcStyle::parse(EC_STYLE_VIDEO).unwrap(), EcStyle::Video);
    assert_eq!(
        EcStyle::parse(EC_STYLE_VIDEO_REV).unwrap(),
        EcStyle::VideoRev
    );
    assert_eq!(
        EcStyle::parse(EC_STYLE_LOGARITHMIC).unwrap(),
        EcStyle::Logarithmic
    );
    assert_eq!(
        EcStyle::parse(EC_STYLE_LOGARITHMIC_REV).unwrap(),
        EcStyle::LogarithmicRev
    );
    assert!(EcStyle::parse("Unknown exposure contrast style")
        .unwrap_err()
        .message()
        .contains("Unknown exposure contrast style"));
    assert!(EcStyle::parse("")
        .unwrap_err()
        .message()
        .contains("Missing exposure contrast style"));
    for s in EcStyle::ALL {
        assert_eq!(EcStyle::parse(s.as_str()).unwrap(), s);
        assert_eq!(
            EcStyle::from_transform_style(s.transform_style(), s.direction()),
            s
        );
    }
}

#[test]
fn data_accessors() {
    let ec0 = ExposureContrastOpData::default();
    assert_eq!(ec0.style, EcStyle::Linear);
    assert_eq!(ec0.exposure(), 0.0);
    assert_eq!(ec0.contrast(), 1.0);
    assert_eq!(ec0.gamma(), 1.0);
    assert_eq!(ec0.pivot, 0.18);
    assert_eq!(ec0.log_exposure_step, LOGEXPOSURESTEP_DEFAULT);
    assert_eq!(ec0.log_mid_gray, LOGMIDGRAY_DEFAULT);
    assert!(ec0.is_identity());
    assert!(ec0.is_no_op());
    assert!(!ec0.has_channel_crosstalk());
    assert!(ec0.validate().is_ok());
    let cache_id = ec0.cache_id();
    assert!(cache_id.eq_ignore_ascii_case("linear E: 0 C: 1 G: 1 P: 0.18 LES: 0.088 LMG: 0.435"));

    ec0.set_exposure(0.1);
    assert!(!ec0.is_identity());
    assert!(!ec0.is_no_op());
    assert_ne!(cache_id, ec0.cache_id());

    let mut ec = ExposureContrastOpData::new(EcStyle::Video);
    assert_eq!(ec.style, EcStyle::Video);
    assert!(ec.is_no_op());
    assert!(!ec.exposure.is_dynamic());
    assert!(!ec.contrast.is_dynamic());
    assert!(!ec.gamma.is_dynamic());
    assert!(!ec.is_dynamic());

    // Never treated as no-op when dynamic.
    ec.exposure.make_dynamic();
    assert!(!ec.is_no_op());
    assert!(ec.is_dynamic());
    assert!(ec.has_dynamic_property(DynamicPropertyType::Exposure));

    ec.set_exposure(0.1);
    ec.set_contrast(0.8);
    ec.set_gamma(1.1);
    ec.pivot = 0.2;
    ec.log_exposure_step = 0.07;
    ec.log_mid_gray = 0.5;
    assert_eq!(ec.exposure(), 0.1);
    assert_eq!(ec.contrast(), 0.8);
    assert_eq!(ec.gamma(), 1.1);

    // Property must be set as dynamic to accept a dynamic value.
    assert!(!ec.has_dynamic_property(DynamicPropertyType::Contrast));
    assert!(!ec.has_dynamic_property(DynamicPropertyType::Gamma));
    assert!(ec
        .dynamic_property(DynamicPropertyType::Contrast)
        .unwrap_err()
        .message()
        .contains("not dynamic"));
    assert!(ec
        .dynamic_property(DynamicPropertyType::GradingTone)
        .unwrap_err()
        .message()
        .contains("not supported by ExposureContrast"));

    let dp_exp = ec.dynamic_property(DynamicPropertyType::Exposure).unwrap();
    let h = dp_exp.as_double().unwrap();
    assert_eq!(h.get(), 0.1);
    h.set(1.5);
    assert_eq!(ec.exposure(), 1.5);
    h.set(0.7);
    assert_eq!(ec.exposure(), 0.7);

    ec.contrast.make_dynamic();
    ec.gamma.make_dynamic();
    let dpc = ec.dynamic_property(DynamicPropertyType::Contrast).unwrap();
    let dpg = ec.dynamic_property(DynamicPropertyType::Gamma).unwrap();
    dpc.as_double().unwrap().set(1.42);
    dpg.as_double().unwrap().set(0.88);
    assert_eq!(ec.contrast(), 1.42);
    assert_eq!(ec.gamma(), 0.88);
}

#[test]
fn data_clone() {
    let mut ec = ExposureContrastOpData::default();
    ec.set_exposure(-1.4);
    ec.set_contrast(0.8);
    ec.set_gamma(1.1);
    ec.pivot = 0.2;
    ec.exposure.make_dynamic();
    let dp = ec.dynamic_property(DynamicPropertyType::Exposure).unwrap();
    let h = dp.as_double().unwrap();
    assert_eq!(h.get(), -1.4);
    h.set(1.5);
    let cloned = ec.clone();
    assert_eq!(ec.exposure(), cloned.exposure());
    assert_eq!(ec.contrast(), cloned.contrast());
    assert_eq!(ec.gamma(), cloned.gamma());
    assert_eq!(ec.pivot, cloned.pivot);
    assert_eq!(ec.exposure.is_dynamic(), cloned.exposure.is_dynamic());
    assert_eq!(ec.contrast.is_dynamic(), cloned.contrast.is_dynamic());
    // Clone makes a copy of the dynamic property rather than sharing the original.
    h.set(0.21);
    assert_eq!(ec.exposure(), 0.21);
    assert_eq!(cloned.exposure(), 1.5);
}

#[test]
fn data_inverse() {
    let mut ec = ExposureContrastOpData::new(EcStyle::Video);
    ec.set_contrast(0.8);
    ec.set_gamma(1.1);
    ec.pivot = 0.2;
    ec.exposure.make_dynamic();
    let dp = ec.dynamic_property(DynamicPropertyType::Exposure).unwrap();
    let h = dp.as_double().unwrap();
    h.set(1.5);
    let mut inv = ec.inverse();
    // Dynamic are not inverse.
    assert!(!ec.is_inverse(&inv));
    assert_eq!(inv.style, EcStyle::VideoRev);
    assert_eq!(ec.exposure(), inv.exposure());
    assert_eq!(ec.contrast(), inv.contrast());
    assert_eq!(ec.exposure.is_dynamic(), inv.exposure.is_dynamic());
    // Inverse makes a copy of the dynamic property rather than sharing the original.
    h.set(0.21);
    assert_eq!(ec.exposure(), 0.21);
    assert_eq!(inv.exposure(), 1.5);
    assert!(!ec.is_inverse(&inv));
    inv.contrast.make_dynamic();
    assert!(!ec.is_inverse(&inv));
    ec.contrast.make_dynamic();
    assert!(!ec.is_inverse(&inv));
    ec.set_gamma(1.2);
    assert!(!ec.is_inverse(&inv));

    // Static inverse.
    let ec = ExposureContrastOpData::new(EcStyle::Logarithmic);
    ec.set_exposure(0.4);
    let inv = ec.inverse();
    assert!(ec.is_inverse(&inv));
    assert!(inv.is_inverse(&ec));
    assert!(!ec.is_inverse(&ec));
}

#[test]
fn data_equality() {
    let mut ec0 = ExposureContrastOpData::default();
    let mut ec1 = ExposureContrastOpData::default();
    assert!(ec0 == ec1);
    ec0.style = EcStyle::Video;
    assert!(ec0 != ec1);
    ec1.style = EcStyle::Video;
    assert!(ec0 == ec1);
    // Change dynamic.
    ec0.exposure.make_dynamic();
    assert!(ec0 != ec1);
    ec1.exposure.make_dynamic();
    assert!(ec0 != ec1);
    ec0.set_exposure(0.5);
    ec1.set_exposure(0.5);
    assert!(ec0 != ec1);
    ec1.set_contrast(0.5);
    ec0.set_contrast(0.5);
    assert!(ec0 != ec1);
}

#[test]
fn data_replace_dynamic_property() {
    let mut ec0 = ExposureContrastOpData::default();
    let mut ec1 = ExposureContrastOpData::default();
    ec0.set_exposure(0.0);
    ec1.set_exposure(1.0);
    ec0.exposure.make_dynamic();
    ec1.exposure.make_dynamic();
    let dpe0 = ec0.dynamic_property(DynamicPropertyType::Exposure).unwrap();
    let dpe1 = ec1.dynamic_property(DynamicPropertyType::Exposure).unwrap();
    // These are 2 different values.
    assert!(!dpe0.as_double().unwrap().ptr_eq(dpe1.as_double().unwrap()));
    ec1.replace_dynamic_property(&dpe0).unwrap();
    let dpe1 = ec1.dynamic_property(DynamicPropertyType::Exposure).unwrap();
    // Now, this is the same value.
    assert!(dpe0.as_double().unwrap().ptr_eq(dpe1.as_double().unwrap()));

    ec0.contrast.make_dynamic();
    // Contrast is not enabled in ec1.
    assert!(ec1.dynamic_property(DynamicPropertyType::Contrast).is_err());
    let dpc0 = ec0.dynamic_property(DynamicPropertyType::Contrast).unwrap();
    // The property is not replaced if dynamic is not enabled.
    assert!(ec1.replace_dynamic_property(&dpc0).is_err());
    assert!(ec1.dynamic_property(DynamicPropertyType::Contrast).is_err());
}

// ExposureContrastOp_tests.cpp

#[test]
fn op_create() {
    let mut data = ExposureContrastOpData::default();
    let mut ops = OpVec::new();
    // Make it dynamic so that it is not a no-op.
    data.exposure.make_dynamic();
    create_exposure_contrast_op(&mut ops, &data, TransformDirection::Forward).unwrap();
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].name(), "ExposureContrast");
    assert!(!ops[0].is_no_op());
    let mut data = data.clone();
    data.contrast.make_dynamic();
    create_exposure_contrast_op(&mut ops, &data, TransformDirection::Forward).unwrap();
    assert_eq!(ops.len(), 2);
    assert!(ops[1].downcast_ref::<ExposureContrastOp>().is_some());
}

fn ec(ops: &OpVec, i: usize) -> &ExposureContrastOpData {
    ops[i].downcast_ref::<ExposureContrastOp>().unwrap().data()
}

#[test]
fn op_inverse() {
    let data = ExposureContrastOpData::default();
    data.set_exposure(1.2);
    let mut data = data;
    data.pivot = 0.5;
    let mut ops = OpVec::new();
    create_exposure_contrast_op(&mut ops, &data, TransformDirection::Forward).unwrap();
    create_exposure_contrast_op(&mut ops, &data, TransformDirection::Inverse).unwrap();
    assert!(ec(&ops, 0).is_inverse(ec(&ops, 1)));
    assert!(ec(&ops, 1).is_inverse(ec(&ops, 0)));

    let data2 = data.clone();
    data2.set_exposure(1.3);
    create_exposure_contrast_op(&mut ops, &data2, TransformDirection::Inverse).unwrap();
    assert!(!ec(&ops, 0).is_inverse(ec(&ops, 2)));

    // With dynamic property.
    let mut data3 = data2.clone();
    data3.exposure.make_dynamic();
    create_exposure_contrast_op(&mut ops, &data3, TransformDirection::Inverse).unwrap();
    create_exposure_contrast_op(&mut ops, &data3, TransformDirection::Forward).unwrap();
    assert!(!ec(&ops, 4).is_inverse(ec(&ops, 3)));
    assert!(!ec(&ops, 4).is_inverse(ec(&ops, 1)));
    assert!(!ec(&ops, 4).is_inverse(ec(&ops, 0)));
    let dp3 = ops[3]
        .dynamic_property(DynamicPropertyType::Exposure)
        .unwrap();
    let dp4 = ops[4]
        .dynamic_property(DynamicPropertyType::Exposure)
        .unwrap();
    dp4.as_double().unwrap().set(-1.0);
    assert_ne!(
        dp3.as_double().unwrap().get(),
        dp4.as_double().unwrap().get()
    );
    dp3.as_double().unwrap().set(-1.0);
    assert!(!ec(&ops, 4).is_inverse(ec(&ops, 3)));

    // The optimizer removes the static pair.
    let pair = vec![ops[0].clone(), ops[1].clone()];
    assert!(optimize_ops(&pair, OptimizationFlags::DEFAULT).is_empty());
    let flags = OptimizationFlags::DEFAULT & !OptimizationFlags::PAIR_IDENTITY_EXPOSURE_CONTRAST;
    assert_eq!(optimize_ops(&pair, flags).len(), 2);
    // Not the dynamic pair.
    let pair = vec![ops[3].clone(), ops[4].clone()];
    assert_eq!(optimize_ops(&pair, OptimizationFlags::DEFAULT).len(), 2);
    // Unless the dynamic properties are removed.
    let flags = OptimizationFlags::DEFAULT | OptimizationFlags::NO_DYNAMIC_PROPERTIES;
    assert!(optimize_ops(&pair, flags).is_empty());
}

#[test]
fn op_create_transform() {
    let mut data = ExposureContrastOpData::default();
    data.contrast.make_dynamic();
    data.set_exposure(1.2);
    data.pivot = 0.5;
    data.log_exposure_step = 0.09;
    data.log_mid_gray = 0.7;
    data.metadata.add_attribute("name", "test");
    let mut ops = OpVec::new();
    create_exposure_contrast_op(&mut ops, &data, TransformDirection::Forward).unwrap();
    let t = match ops[0].to_transform().unwrap() {
        Transform::ExposureContrast(t) => t,
        _ => panic!("expected an exposure contrast transform"),
    };
    assert_eq!(
        t.metadata.attributes,
        vec![("name".to_string(), "test".to_string())]
    );
    assert_eq!(t.direction, TransformDirection::Forward);
    assert_eq!(t.exposure, data.exposure());
    assert!(!t.exposure_dynamic);
    assert_eq!(t.contrast, data.contrast());
    assert!(t.contrast_dynamic);
    assert_eq!(t.gamma, data.gamma());
    assert!(!t.gamma_dynamic);
    assert_eq!(t.pivot, data.pivot);
    assert_eq!(t.log_exposure_step, data.log_exposure_step);
    assert_eq!(t.log_mid_gray, data.log_mid_gray);
}

#[test]
fn op_dynamic_properties() {
    let mut data = ExposureContrastOpData::default();
    data.exposure.make_dynamic();
    let mut ops = OpVec::new();
    create_exposure_contrast_op(&mut ops, &data, TransformDirection::Forward).unwrap();
    create_exposure_contrast_op(&mut ops, &data, TransformDirection::Forward).unwrap();
    assert!(ops[0].is_dynamic());
    assert!(ops[0]
        .dynamic_property(DynamicPropertyType::Exposure)
        .is_some());
    assert!(ops[0]
        .dynamic_property(DynamicPropertyType::Contrast)
        .is_none());
    assert_ne!(ops[0].cache_id(), "");
    assert!(!ops[0].cache_id().contains("E: "));

    // The processor shares one property between the ops.
    let proc = Processor::from_ops(ops);
    let dp = proc
        .dynamic_property(DynamicPropertyType::Exposure)
        .unwrap();
    dp.as_double().unwrap().set(1.0);
    let cpu = proc.optimized_cpu_processor(OptimizationFlags::DEFAULT);
    let mut px = [0.25f32, 0.5, 1.0, 0.5];
    cpu.apply_rgba(&mut px);
    // Two ops with an exposure of 1 stop each.
    assert_eq!(px, [1.0, 2.0, 4.0, 0.5]);

    // Non-dynamic copy.
    let nd = proc.ops()[0].make_non_dynamic().unwrap();
    assert!(!nd.is_dynamic());
    dp.as_double().unwrap().set(2.0);
    let out = apply_op(nd.as_ref(), &[0.25, 0.5, 1.0, 0.5]);
    assert_eq!(out, vec![0.5, 1.0, 2.0, 0.5]);
}

// ExposureContrastOpCPU_tests.cpp

fn video_ec_val(input: f32, ec: &ExposureContrastOpData) -> f32 {
    let exposure = 2.0f32
        .powf(ec.exposure() as f32)
        .powf(VIDEO_OETF_POWER as f32);
    let contrast = cmax_f64(MIN_CONTRAST, ec.contrast() * ec.gamma()) as f32;
    let pivot = (cmax_f64(MIN_PIVOT, ec.pivot) as f32).powf(VIDEO_OETF_POWER as f32);
    if contrast == 1.0 {
        return input * exposure / pivot * pivot;
    }
    cmax(0.0, input * exposure / pivot).powf(contrast) * pivot
}

fn log_ec_val(input: f32, ec: &ExposureContrastOpData) -> f32 {
    let exposure = (ec.log_exposure_step * ec.exposure()) as f32;
    let contrast = cmax_f64(MIN_CONTRAST, ec.contrast() * ec.gamma()) as f32;
    let pivot = cmax_f64(MIN_PIVOT, ec.pivot) as f32;
    let log_pivot = cmax_f64(
        0.0,
        (pivot as f64 / 0.18).log2() * ec.log_exposure_step + ec.log_mid_gray,
    ) as f32;
    let offset = (exposure - log_pivot) * contrast + log_pivot;
    input * contrast + offset
}

fn lin_ec_val(input: f32, ec: &ExposureContrastOpData) -> f32 {
    let exposure = 2.0f32.powf(ec.exposure() as f32);
    let contrast = cmax_f64(MIN_CONTRAST, ec.contrast() * ec.gamma()) as f32;
    let pivot = cmax_f64(MIN_PIVOT, ec.pivot) as f32;
    if contrast == 1.0 {
        return input * exposure / pivot * pivot;
    }
    cmax(0.0, input * exposure / pivot).powf(contrast) * pivot
}

fn render(r: &ExposureContrastRenderer, image: &[f32]) -> Vec<f32> {
    let mut px = to_pixels(image);
    r.apply(&mut px);
    flatten(&px)
}

fn make_all_dynamic(ec: &mut ExposureContrastOpData) {
    ec.exposure.make_dynamic();
    ec.contrast.make_dynamic();
    ec.gamma.make_dynamic();
}

fn set_values(r: &ExposureContrastOpData, e: f64, c: f64, g: f64) {
    r.set_exposure(e);
    r.set_contrast(c);
    r.set_gamma(g);
}

#[test]
fn renderer_video() {
    let image = [
        0.0367126f32,
        0.5,
        1.,
        0.,
        0.2,
        0.,
        0.99,
        128.,
        QNAN,
        QNAN,
        QNAN,
        0.,
        INF,
        INF,
        INF,
        0.,
    ];
    let mut ec = ExposureContrastOpData::new(EcStyle::Video);
    make_all_dynamic(&mut ec);
    let r = ExposureContrastRenderer::new(&ec);
    let rgba = render(&r, &image);
    for i in [0, 1, 2, 4, 5, 6, 12, 13, 14] {
        assert_eq!(rgba[i], video_ec_val(image[i], &ec), "index {i}");
    }
    assert_eq!(rgba[3], image[3]);
    assert_eq!(rgba[7], image[7]);
    assert!(rgba[8].is_nan() && rgba[9].is_nan() && rgba[10].is_nan());

    // The renderer shares the dynamic properties.
    set_values(&ec, 0.2, 1.0, 1.2);
    let rgba = render(&r, &image);
    for i in [0, 1, 2, 4, 5, 6, 8, 9, 10] {
        assert_close_f(rgba[i], video_ec_val(image[i], &ec) as f64, 1e-5);
    }
    for i in [12, 13, 14] {
        assert_eq!(rgba[i], video_ec_val(image[i], &ec));
    }
    assert_eq!(rgba[3], image[3]);
    assert_eq!(rgba[7], image[7]);
}

#[test]
fn renderer_log() {
    let image = [
        0.0367126f32,
        0.5,
        1.,
        0.,
        0.2,
        0.,
        0.99,
        128.,
        QNAN,
        QNAN,
        QNAN,
        0.,
        INF,
        INF,
        INF,
        0.,
    ];
    let mut ec = ExposureContrastOpData::new(EcStyle::Logarithmic);
    make_all_dynamic(&mut ec);
    ec.set_exposure(1.2);
    ec.pivot = 0.18;
    let r = ExposureContrastRenderer::new(&ec);
    for (e, c, g) in [(1.2, 1.0, 1.0), (0.2, 0.5, 1.6)] {
        set_values(&ec, e, c, g);
        let rgba = render(&r, &image);
        for i in [0, 1, 2, 4, 5, 6, 12, 13, 14] {
            assert_eq!(rgba[i], log_ec_val(image[i], &ec), "index {i}");
        }
        assert_eq!(rgba[3], image[3]);
        assert_eq!(rgba[7], image[7]);
        assert!(rgba[8].is_nan() && rgba[9].is_nan() && rgba[10].is_nan());
    }
}

#[test]
fn renderer_linear() {
    let image = [
        0.0f32, 0.5, 1., 0., 0.2, 0.8, 0.99, 128., QNAN, QNAN, QNAN, 0., INF, INF, INF, 0.,
    ];
    let mut ec = ExposureContrastOpData::new(EcStyle::Linear);
    make_all_dynamic(&mut ec);
    let r = ExposureContrastRenderer::new(&ec);
    let rgba = render(&r, &image);
    for i in [0, 1, 2, 4, 5, 6, 12, 13, 14] {
        assert_eq!(rgba[i], lin_ec_val(image[i], &ec), "index {i}");
    }
    assert_eq!(rgba[3], image[3]);
    assert_eq!(rgba[7], image[7]);
    assert!(rgba[8].is_nan() && rgba[9].is_nan() && rgba[10].is_nan());

    set_values(&ec, 0.2, 1.5, 1.2);
    let rgba = render(&r, &image);
    for i in [0, 1, 2, 4, 5, 6, 8, 9, 10] {
        assert_close_f(rgba[i], lin_ec_val(image[i], &ec) as f64, 5e-5);
    }
    for i in [12, 13, 14] {
        assert_eq!(rgba[i], lin_ec_val(image[i], &ec));
    }
    assert_eq!(rgba[3], image[3]);
    assert_eq!(rgba[7], image[7]);
}

fn test_ec_inverse(style: EcStyle) {
    let image = [0.0f32, 0.5, 1., 0., 0.2, 0.8, 0.99, 128.];
    let mut ec = ExposureContrastOpData::new(style);
    ec.set_exposure(1.5);
    ec.set_contrast(0.5);
    ec.set_gamma(1.1);
    ec.pivot = 0.18;
    let rgba = render(&ExposureContrastRenderer::new(&ec), &image);
    let rgba = render(&ExposureContrastRenderer::new(&ec.inverse()), &rgba);
    for i in [0, 1, 2, 4, 5, 6] {
        assert_close_f(rgba[i], image[i] as f64, 1e-5);
    }
    assert_eq!(rgba[3], image[3]);
    assert_eq!(rgba[7], image[7]);
}

#[test]
fn renderer_inverse() {
    test_ec_inverse(EcStyle::Logarithmic);
    test_ec_inverse(EcStyle::Linear);
    test_ec_inverse(EcStyle::Video);
}

fn test_log_param_for_style(style: EcStyle, has_effect: bool) {
    let image = [0.1f32, 0.2, 0.3, 0., 0.4, 0.5, 0.6, 0., 0.7, 0.8, 0.9, 0.];
    let mut ec = ExposureContrastOpData::new(style);
    ec.set_exposure(0.2);
    ec.set_contrast(1.0);
    ec.set_gamma(1.2);
    let reference = render(&ExposureContrastRenderer::new(&ec), &image);
    ec.log_exposure_step = 0.1;
    ec.log_mid_gray = 0.4;
    let rgba = render(&ExposureContrastRenderer::new(&ec), &image);
    for i in 0..12 {
        if !has_effect || i % 4 == 3 {
            assert_eq!(rgba[i], reference[i]);
        } else {
            assert_ne!(rgba[i], reference[i]);
        }
    }
}

#[test]
fn renderer_log_params() {
    test_log_param_for_style(EcStyle::Video, false);
    test_log_param_for_style(EcStyle::VideoRev, false);
    test_log_param_for_style(EcStyle::Linear, false);
    test_log_param_for_style(EcStyle::LinearRev, false);
    test_log_param_for_style(EcStyle::Logarithmic, true);
    test_log_param_for_style(EcStyle::LogarithmicRev, true);
}

// ExposureContrastTransform_tests.cpp

#[test]
fn transform_basic() {
    let mut t = ExposureContrastTransform::default();
    assert_eq!(t.direction, TransformDirection::Forward);
    assert_eq!(t.style, ExposureContrastStyle::Linear);
    assert_eq!(t.exposure, 0.0);
    assert_eq!(t.contrast, 1.0);
    assert_eq!(t.gamma, 1.0);
    assert_eq!(t.pivot, 0.18);
    assert_eq!(t.log_exposure_step, 0.088);
    assert_eq!(t.log_mid_gray, 0.435);
    assert!(!t.exposure_dynamic && !t.contrast_dynamic && !t.gamma_dynamic);
    assert!(t.validate().is_ok());

    let t2 = t.clone();
    assert!(t.equals(&t2));
    t.exposure_dynamic = true;
    assert!(!t.equals(&t2));
    let t3 = t.clone();
    // Dynamic properties are never equal.
    assert!(!t.equals(&t3));
}

#[test]
fn transform_processor_with_dynamic() {
    let config = Config::create_raw();
    let ctx = Context::new();
    let t = ExposureContrastTransform {
        style: ExposureContrastStyle::Video,
        exposure: 1.1,
        exposure_dynamic: true,
        contrast: 0.5,
        contrast_dynamic: true,
        gamma: 1.5,
        pivot: 0.18,
        ..Default::default()
    };
    let mut ops = OpVec::new();
    t.build_ops(&mut ops, &config, &ctx, TransformDirection::Forward)
        .unwrap();
    let proc = Processor::from_ops(ops);
    assert!(proc.is_dynamic());
    assert!(proc.has_dynamic_property(DynamicPropertyType::Exposure));
    assert!(proc.has_dynamic_property(DynamicPropertyType::Contrast));
    assert!(!proc.has_dynamic_property(DynamicPropertyType::Gamma));
    let cpu = proc.default_cpu_processor();

    let input = [0.3f32, 0.9, 0.5, 1.0];
    let ec_data = ExposureContrastOpData::from_transform(&t);
    let expected = |d: &ExposureContrastOpData| [0, 1, 2].map(|i| video_ec_val(input[i], d));

    let mut px = input;
    cpu.apply_rgba(&mut px);
    let e = expected(&ec_data);
    for i in 0..3 {
        assert_close_f(px[i], e[i] as f64, 1e-6);
    }

    // Change the dynamic values.
    let dpe = cpu.dynamic_property(DynamicPropertyType::Exposure).unwrap();
    let dpc = cpu.dynamic_property(DynamicPropertyType::Contrast).unwrap();
    dpe.as_double().unwrap().set(0.4);
    dpc.as_double().unwrap().set(0.8);
    ec_data.set_exposure(0.4);
    ec_data.set_contrast(0.8);
    let mut px = input;
    cpu.apply_rgba(&mut px);
    let e = expected(&ec_data);
    for i in 0..3 {
        assert_close_f(px[i], e[i] as f64, 1e-6);
    }
    assert_eq!(px[3], 1.0);

    // Inverse round trip.
    let mut ops = OpVec::new();
    let st = ExposureContrastTransform {
        exposure_dynamic: false,
        contrast_dynamic: false,
        ..t.clone()
    };
    st.build_ops(&mut ops, &config, &ctx, TransformDirection::Forward)
        .unwrap();
    st.build_ops(&mut ops, &config, &ctx, TransformDirection::Inverse)
        .unwrap();
    let out = apply_ops(&ops, &input);
    for i in 0..4 {
        assert_close_f(out[i], input[i] as f64, 1e-5);
    }
    assert!(optimize_ops(&ops, OptimizationFlags::DEFAULT).is_empty());
}

#[test]
fn transform_processor() {
    let config = Config::create_raw();
    let mut ec = ExposureContrastTransform {
        style: ExposureContrastStyle::Video,
        exposure: 1.1,
        exposure_dynamic: true,
        contrast: 0.5,
        gamma: 1.5,
        ..Default::default()
    };
    let t = Transform::ExposureContrast(ec.clone());
    let proc = config
        .get_processor_for_transform(&t, TransformDirection::Forward)
        .unwrap();
    let cpu = proc.default_cpu_processor();
    let check = |expected: [f64; 3], error: f64| {
        let mut pixel = [0.2f32, 0.3, 0.4];
        cpu.apply_rgb(&mut pixel);
        for i in 0..3 {
            assert_close_f(pixel[i], expected[i], error);
        }
    };
    check([0.32340, 0.43834, 0.54389], 1e-5);

    // Changing the original transform does not change the processor.
    ec.exposure = 2.1;
    check([0.32340, 0.43834, 0.54389], 1e-5);

    let dp = cpu.dynamic_property(DynamicPropertyType::Exposure).unwrap();
    dp.as_double().unwrap().set(2.1);
    // Gamma is not dynamic.
    assert!(cpu.dynamic_property(DynamicPropertyType::Gamma).is_none());
    check([0.42965, 0.58235, 0.72258], 1e-5);
    dp.as_double().unwrap().set(0.8);
    // OCIO doubles the tolerance here.
    check([0.29698, 0.40252, 0.49946], 2e-5);
}

#[test]
fn transform_processor_several_ec() {
    let config = Config::create_raw();
    let (a, b) = (1.1, 2.1);
    let ec1 = ExposureContrastTransform {
        style: ExposureContrastStyle::Logarithmic,
        exposure: a,
        contrast: 0.5,
        gamma: 1.5,
        ..Default::default()
    };
    let mut ec2 = ExposureContrastTransform {
        exposure: b,
        ..ec1.clone()
    };
    let src = [0.2f32, 0.3, 0.4];
    let run = |t: &ExposureContrastTransform, px: &mut [f32; 3]| {
        let t = Transform::ExposureContrast(t.clone());
        let p = config
            .get_processor_for_transform(&t, TransformDirection::Forward)
            .unwrap();
        p.default_cpu_processor().apply_rgb(px);
    };
    let mut pixel_a = src;
    run(&ec1, &mut pixel_a);
    let mut pixel_aa = pixel_a;
    run(&ec1, &mut pixel_aa);
    let mut pixel_ab = pixel_a;
    run(&ec2, &mut pixel_ab);

    // Only the second E/C is dynamic.
    ec2.exposure_dynamic = true;
    ec2.exposure = a;
    let g = crate::transforms::GroupTransform::from_transforms(vec![
        ec1.clone().into(),
        ec2.clone().into(),
    ]);
    let p = config
        .get_processor_for_transform(&g.into(), TransformDirection::Forward)
        .unwrap();
    let cpu = p.default_cpu_processor();
    let dp = cpu.dynamic_property(DynamicPropertyType::Exposure).unwrap();
    let mut px = src;
    cpu.apply_rgb(&mut px);
    for i in 0..3 {
        assert_close_f(px[i], pixel_aa[i] as f64, 1e-6);
    }
    // Change the 2nd exposure: the first E/C keeps its value.
    dp.as_double().unwrap().set(b);
    let mut px = src;
    cpu.apply_rgb(&mut px);
    for i in 0..3 {
        assert_close_f(px[i], pixel_ab[i] as f64, 1e-6);
    }
}
