// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Copyright (c) 2026 Philippe Llerena.

//! Port of the OpenColorIO GPU unit tests `tests/gpu/LogOp_test.cpp`,
//! running the WGSL shaders (see `wgsl_common::gpu_unit`).

mod wgsl_common;

use ocio::{LogAffineTransform, LogCameraTransform, LogTransform, Transform, TransformDirection};
use wgsl_common::gpu_unit::{gpu_test, GpuTest};

// The OCIO_USE_SSE2 thresholds: the ocio-rs CPU renderers reproduce OCIO's
// SSE2 builds (see PORTING.md), the log ops using the fast `sseLog2` /
// `sseExp2` approximations (upstream uses 1e-5 / 1e-4 without SSE2).
const G_EPSILON: f32 = 1e-4;
const G_EPSILON_INVERSE: f32 = 1e-3;

// Upstream constants are floats passed to `setBase(double)`.
const BASE10: f64 = 10.0f32 as f64;
fn euler_constant() -> f64 {
    1.0f32.exp() as f64
}

// Helper method to build unit tests
fn add_log_test(test: &mut GpuTest, direction: TransformDirection, base: f64, epsilon: f32) {
    let log = LogTransform {
        direction,
        base,
        ..Default::default()
    };

    test.set_processor_transform(Transform::Log(log));

    test.set_error_threshold(epsilon);

    // TODO: Would like to be able to remove the setTestNaN(false) and
    // setTestInfinity(false) from all of these tests.
    test.set_test_infinity(false);
}

#[test]
fn log_base_10_legacy() {
    gpu_test("LogTransform/LogBase_10_legacy", |t| {
        add_log_test(t, TransformDirection::Forward, BASE10, G_EPSILON);

        t.set_legacy_shader(true);
        // TODO: Would like to be able to remove the setTestNaN(false) from all of these tests.
        t.set_test_nan(false);
    });
}

#[test]
fn log_base_10_legacy_inverse() {
    gpu_test("LogTransform/LogBase_10_legacy_inverse", |t| {
        add_log_test(t, TransformDirection::Inverse, BASE10, G_EPSILON_INVERSE);

        t.set_legacy_shader(true);
        t.set_test_nan(false);
    });
}

#[test]
fn log_base_10_generic_shader() {
    gpu_test("LogTransform/LogBase_10_generic_shader", |t| {
        add_log_test(t, TransformDirection::Forward, BASE10, G_EPSILON);

        t.set_test_nan(false);
    });
}

#[test]
fn log_base_10_inverse_generic_shader() {
    gpu_test("LogTransform/LogBase_10_inverse_generic_shader", |t| {
        add_log_test(t, TransformDirection::Inverse, BASE10, G_EPSILON_INVERSE);

        t.set_test_nan(false);
    });
}

#[test]
fn log_base_euler_legacy() {
    gpu_test("LogTransform/LogBase_euler_legacy", |t| {
        add_log_test(t, TransformDirection::Forward, euler_constant(), G_EPSILON);

        t.set_legacy_shader(true);
        t.set_test_nan(false);
    });
}

#[test]
fn log_base_euler_legacy_inverse() {
    gpu_test("LogTransform/LogBase_euler_legacy_inverse", |t| {
        add_log_test(
            t,
            TransformDirection::Inverse,
            euler_constant(),
            G_EPSILON_INVERSE,
        );

        t.set_legacy_shader(true);
        t.set_test_nan(false);
    });
}

#[test]
fn log_base_euler_generic_shader() {
    gpu_test("LogTransform/LogBase_euler_generic_shader", |t| {
        add_log_test(t, TransformDirection::Forward, euler_constant(), G_EPSILON);

        t.set_test_nan(false);
    });
}

#[test]
fn log_base_euler_inverse_generic_shader() {
    gpu_test("LogTransform/LogBase_euler_inverse_generic_shader", |t| {
        add_log_test(
            t,
            TransformDirection::Inverse,
            euler_constant(),
            G_EPSILON_INVERSE,
        );

        t.set_test_nan(false);
    });
}

#[test]
fn log_affine_base() {
    gpu_test("LogAffineTransform/base", |t| {
        let log = LogAffineTransform {
            direction: TransformDirection::Forward,
            base: BASE10,
            ..Default::default()
        };

        t.set_processor_transform(Transform::LogAffine(log));

        t.set_error_threshold(G_EPSILON);

        t.set_test_infinity(false);
    });
}

#[test]
fn log_affine_base_inverse() {
    gpu_test("LogAffineTransform/base_inverse", |t| {
        let log = LogAffineTransform {
            direction: TransformDirection::Inverse,
            base: BASE10,
            ..Default::default()
        };

        t.set_processor_transform(Transform::LogAffine(log));

        t.set_error_threshold(G_EPSILON); // Use the tighter tolerance than g_epsilon_inverse

        t.set_relative_comparison(true);

        t.set_test_nan(false);
        t.set_test_infinity(false);
    });
}

#[test]
fn log_affine_lin_side_slope() {
    gpu_test("LogAffineTransform/linSideSlope", |t| {
        let log = LogAffineTransform {
            direction: TransformDirection::Forward,
            lin_side_slope: [2.0, 0.5, 3.0],
            ..Default::default()
        };

        t.set_processor_transform(Transform::LogAffine(log));

        t.set_error_threshold(G_EPSILON);

        t.set_test_infinity(false);
    });
}

#[test]
fn log_affine_lin_side_slope_inverse() {
    gpu_test("LogAffineTransform/linSideSlope_inverse", |t| {
        let log = LogAffineTransform {
            direction: TransformDirection::Inverse,
            lin_side_slope: [2.0, 0.5, 3.0],
            ..Default::default()
        };

        t.set_processor_transform(Transform::LogAffine(log));

        t.set_error_threshold(G_EPSILON); // Use the tighter tolerance than g_epsilon_inverse

        t.set_test_nan(false);
        t.set_test_infinity(false);
    });
}

#[test]
fn log_affine_lin_side_offset() {
    gpu_test("LogAffineTransform/linSideOffset", |t| {
        let log = LogAffineTransform {
            direction: TransformDirection::Forward,
            lin_side_offset: [0.1, 0.2, 0.3],
            ..Default::default()
        };

        t.set_processor_transform(Transform::LogAffine(log));

        t.set_error_threshold(G_EPSILON);

        t.set_test_infinity(false);
    });
}

#[test]
fn log_affine_lin_side_offset_inverse() {
    gpu_test("LogAffineTransform/linSideOffset_inverse", |t| {
        let log = LogAffineTransform {
            direction: TransformDirection::Inverse,
            lin_side_offset: [0.1, 0.2, 0.3],
            ..Default::default()
        };

        t.set_processor_transform(Transform::LogAffine(log));

        t.set_error_threshold(G_EPSILON); // Use the tighter tolerance than g_epsilon_inverse

        t.set_test_nan(false);
        t.set_test_infinity(false);
    });
}

#[test]
fn log_affine_log_side_slope() {
    gpu_test("LogAffineTransform/logSideSlope", |t| {
        let log = LogAffineTransform {
            direction: TransformDirection::Forward,
            log_side_slope: [2.0, 0.5, 3.0],
            ..Default::default()
        };

        t.set_processor_transform(Transform::LogAffine(log));

        t.set_error_threshold(G_EPSILON * 5.0);

        t.set_test_infinity(false);
    });
}

#[test]
fn log_affine_log_side_slope_inverse() {
    gpu_test("LogAffineTransform/logSideSlope_inverse", |t| {
        let log = LogAffineTransform {
            direction: TransformDirection::Inverse,
            log_side_slope: [2.0, 0.5, 3.0],
            ..Default::default()
        };

        t.set_processor_transform(Transform::LogAffine(log));

        t.set_error_threshold(G_EPSILON); // Use the tighter tolerance than g_epsilon_inverse

        t.set_test_nan(false);
        t.set_test_infinity(false);
    });
}

#[test]
fn log_affine_log_side_offset() {
    gpu_test("LogAffineTransform/logSideOffset", |t| {
        let log = LogAffineTransform {
            direction: TransformDirection::Forward,
            log_side_offset: [0.1, 0.2, 0.3],
            ..Default::default()
        };

        t.set_processor_transform(Transform::LogAffine(log));

        t.set_error_threshold(G_EPSILON);

        t.set_test_infinity(false);
    });
}

#[test]
fn log_affine_log_side_offset_inverse() {
    gpu_test("LogAffineTransform/logSideOffset_inverse", |t| {
        let log = LogAffineTransform {
            direction: TransformDirection::Inverse,
            log_side_offset: [0.1, 0.2, 0.3],
            ..Default::default()
        };

        t.set_processor_transform(Transform::LogAffine(log));

        t.set_error_threshold(G_EPSILON); // Use the tighter tolerance than g_epsilon_inverse

        t.set_test_nan(false);
        t.set_test_infinity(false);
    });
}

#[test]
fn log_affine_lin2log() {
    gpu_test("LogAffineTransform/lin2log", |t| {
        let log = LogAffineTransform {
            direction: TransformDirection::Forward,
            log_side_slope: [0.2, 0.4, 0.25],
            log_side_offset: [0.14, 0.13, 0.12],
            lin_side_slope: [1.5, 1.8, 1.2],
            lin_side_offset: [0.05, 0.1, 0.15],
            ..Default::default()
        };

        t.set_processor_transform(Transform::LogAffine(log));

        t.set_error_threshold(G_EPSILON * 5.0);

        t.set_test_infinity(false);
    });
}

#[test]
fn log_affine_log2lin() {
    gpu_test("LogAffineTransform/log2lin", |t| {
        let log = LogAffineTransform {
            direction: TransformDirection::Inverse,
            log_side_slope: [0.21, 0.2, 0.19],
            log_side_offset: [0.61, 0.6, 0.59],
            lin_side_slope: [1.11, 1.1, 1.12],
            lin_side_offset: [0.051, 0.05, 0.052],
            ..Default::default()
        };

        t.set_processor_transform(Transform::LogAffine(log));

        t.set_error_threshold(G_EPSILON_INVERSE);

        t.set_test_infinity(false);
    });
}

#[test]
fn log_camera_camera_lin2log() {
    gpu_test("LogCameraTransform/camera_lin2log", |t| {
        let log = LogCameraTransform {
            direction: TransformDirection::Forward,
            log_side_slope: [0.2, 0.3, 0.4],
            log_side_offset: [0.7, 0.6, 0.5],
            lin_side_slope: [1.4, 1.1, 1.2],
            lin_side_offset: [0.15, 0.16, 0.25],
            linear_slope: Some([1.22, 1.33, 1.44]),
            ..LogCameraTransform::new([0.12, 0.13, 0.15])
        };

        t.set_processor_transform(Transform::LogCamera(log));

        t.set_error_threshold(G_EPSILON);

        t.set_test_infinity(false);
        // Upstream also disables the NaN test on Apple platforms.
    });
}

#[test]
fn log_camera_camera_log2lin() {
    gpu_test("LogCameraTransform/camera_log2lin", |t| {
        let log = LogCameraTransform {
            direction: TransformDirection::Inverse,
            log_side_slope: [0.21, 0.22, 0.23],
            log_side_offset: [0.6, 0.7, 0.8],
            lin_side_slope: [1.1, 1.2, 1.3],
            lin_side_offset: [0.051, 0.052, 0.053],
            linear_slope: Some([1.25, 1.23, 1.22]),
            ..LogCameraTransform::new([0.12, 0.13, 0.14])
        };

        t.set_processor_transform(Transform::LogCamera(log));

        t.set_error_threshold(G_EPSILON_INVERSE);

        t.set_test_infinity(false);
        // Upstream also disables the NaN test on Apple platforms.
    });
}
