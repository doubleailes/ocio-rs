// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of the OpenColorIO GPU unit tests `tests/gpu/ECOp_test.cpp`,
//! running the WGSL shaders (see `wgsl_common::gpu_unit`).

mod wgsl_common;

use ocio::gpu::GpuShaderCreator;
use ocio::{
    DynamicPropertyType, ExposureContrastStyle, ExposureContrastTransform, GroupTransform,
    Transform, TransformDirection,
};
use wgsl_common::gpu_unit::{gpu_test, GpuTest};

fn ec(
    style: ExposureContrastStyle,
    direction: TransformDirection,
    contrast: f64,
) -> ExposureContrastTransform {
    ExposureContrastTransform {
        style,
        direction,
        exposure: 1.1,
        contrast,
        gamma: 0.9,
        pivot: 0.22,
        ..Default::default()
    }
}

#[test]
fn style_linear_fwd() {
    gpu_test("ExposureContrast/style_linear_fwd", |t| {
        let ec = ec(
            ExposureContrastStyle::Linear,
            TransformDirection::Forward,
            0.8,
        );

        t.set_processor_transform(Transform::ExposureContrast(ec));

        t.set_error_threshold(2e-5);
        t.set_expected_minimal_value(1.0);
        t.set_relative_comparison(true);
        t.set_test_wide_range(true);

        t.set_test_infinity(false);
    });
}

#[test]
fn style_linear_rev() {
    gpu_test("ExposureContrast/style_linear_rev", |t| {
        let ec = ec(
            ExposureContrastStyle::Linear,
            TransformDirection::Inverse,
            0.7,
        );

        t.set_processor_transform(Transform::ExposureContrast(ec));

        t.set_error_threshold(2e-5);
        t.set_expected_minimal_value(1.0);
        t.set_relative_comparison(true);
        t.set_test_wide_range(true);
    });
}

#[test]
fn style_video_fwd() {
    gpu_test("ExposureContrast/style_video_fwd", |t| {
        let ec = ec(
            ExposureContrastStyle::Video,
            TransformDirection::Forward,
            0.8,
        );

        t.set_processor_transform(Transform::ExposureContrast(ec));

        t.set_error_threshold(2e-5);
        t.set_expected_minimal_value(1.0);
        t.set_relative_comparison(true);
        t.set_test_wide_range(true);

        t.set_test_infinity(false);
    });
}

#[test]
fn style_video_rev() {
    gpu_test("ExposureContrast/style_video_rev", |t| {
        let ec = ec(
            ExposureContrastStyle::Video,
            TransformDirection::Inverse,
            0.7,
        );

        t.set_processor_transform(Transform::ExposureContrast(ec));

        t.set_error_threshold(2e-5);
        t.set_expected_minimal_value(1.0);
        t.set_relative_comparison(true);
        t.set_test_wide_range(true);
    });
}

#[test]
fn style_log_fwd() {
    gpu_test("ExposureContrast/style_log_fwd", |t| {
        let ec = ec(
            ExposureContrastStyle::Logarithmic,
            TransformDirection::Forward,
            0.8,
        );

        t.set_processor_transform(Transform::ExposureContrast(ec));

        t.set_error_threshold(1e-6);
        t.set_expected_minimal_value(1.0);
        t.set_relative_comparison(true);
        t.set_test_wide_range(true);
    });
}

#[test]
fn style_log_rev() {
    gpu_test("ExposureContrast/style_log_rev", |t| {
        let ec = ec(
            ExposureContrastStyle::Logarithmic,
            TransformDirection::Inverse,
            0.7,
        );

        t.set_processor_transform(Transform::ExposureContrast(ec));

        t.set_error_threshold(1e-6);
        t.set_expected_minimal_value(1.0);
        t.set_relative_comparison(true);
        t.set_test_wide_range(true);
    });
}

// Port of the `ECRetest` helper: upstream keeps the dynamic properties of
// the processor (CPU; the testing infrastructure creates a new cpu processor
// for each retest, so changing the dynamic property on the processor is
// reflected in each cpu processor) and of the shader description (GPU; the
// shader is created once and its dynamic properties are decoupled from the
// processor ones). The new CPU value is copied to the GPU property.

/// Set the double dynamic property `ty` of the processor (CPU) to
/// `f(current value)` and copy the value to the shader (GPU). Does nothing
/// if the processor has no such property (upstream `if (m_xxxCPU)`).
fn update_ec_property(
    processor: &ocio::Processor,
    desc: &ocio::gpu::GpuShaderDesc,
    ty: DynamicPropertyType,
    f: impl FnOnce(f64) -> f64,
) {
    let Some(cpu) = processor.dynamic_property(ty) else {
        return;
    };
    let cpu = cpu.as_double().unwrap();
    cpu.set(f(cpu.get()));
    let gpu = desc
        .dynamic_property(ty)
        .unwrap_or_else(|e| panic!("{ty:?}: {e}"));
    gpu.as_double().unwrap().set(cpu.get());
}

#[test]
fn style_linear_dynamic_parameter() {
    gpu_test("ExposureContrast/style_linear_dynamic_parameter", |t| {
        let ec = ExposureContrastTransform {
            exposure_dynamic: true,
            contrast_dynamic: true,
            gamma_dynamic: true,
            ..ec(
                ExposureContrastStyle::Linear,
                TransformDirection::Forward,
                0.8,
            )
        };

        t.set_processor_transform(Transform::ExposureContrast(ec));

        t.add_retest(|p, d| update_ec_property(p, d, DynamicPropertyType::Exposure, |v| v + 0.1));
        t.add_retest(|p, d| update_ec_property(p, d, DynamicPropertyType::Contrast, |v| v + 0.1));
        t.add_retest(|p, d| update_ec_property(p, d, DynamicPropertyType::Gamma, |v| v + 0.1));

        t.set_error_threshold(5e-5);
        t.set_expected_minimal_value(1.0);
        t.set_relative_comparison(true);
        t.set_test_wide_range(true);

        t.set_test_infinity(false);
    });
}

fn prepare_2_ec_dynamic(test: &mut GpuTest, first_dyn: bool, second_dyn: bool) {
    // See also OCIO_ADD_TEST(ExposureContrastTransform, processor_several_ec).
    let mut ec1 = ExposureContrastTransform {
        style: ExposureContrastStyle::Logarithmic,
        exposure: 0.8,
        contrast: 0.5,
        gamma: 1.5,
        ..Default::default()
    };

    let mut ec2 = ec1.clone();

    if first_dyn {
        ec1.contrast_dynamic = true;
    }
    if second_dyn {
        ec2.exposure_dynamic = true;
    }

    let grp = GroupTransform::from_transforms(vec![
        Transform::ExposureContrast(ec1),
        Transform::ExposureContrast(ec2),
    ]);

    test.set_processor_transform(Transform::Group(grp));

    test.add_retest(|p, d| {
        update_ec_property(p, d, DynamicPropertyType::Contrast, |_| 0.5);
        update_ec_property(p, d, DynamicPropertyType::Exposure, |_| 1.1);
    });
    test.add_retest(|p, d| {
        update_ec_property(p, d, DynamicPropertyType::Contrast, |_| 0.7);
        update_ec_property(p, d, DynamicPropertyType::Exposure, |_| 2.1);
    });

    test.set_error_threshold(5e-5);
    test.set_expected_minimal_value(1.0);
    test.set_relative_comparison(true);
    test.set_test_wide_range(true);

    test.set_test_infinity(false);
}

#[test]
fn dp_several_one_dynamic() {
    gpu_test("ExposureContrast/dp_several_one_dynamic", |t| {
        // 2 EC, first not dynamic, second dynamic.
        prepare_2_ec_dynamic(t, false, true);
    });
}

#[test]
fn dp_several_both_dynamic() {
    gpu_test("ExposureContrast/dp_several_both_dynamic", |t| {
        // 2 EC, both dynamic.
        prepare_2_ec_dynamic(t, true, true);
    });
}
