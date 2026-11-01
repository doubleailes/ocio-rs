//! Port of the OpenColorIO GPU unit tests `tests/gpu/GammaOp_test.cpp`,
//! running the WGSL shaders (see `wgsl_common::gpu_unit`).
//!
//! Where upstream picks a threshold with `#if OCIO_USE_SSE2`, the SSE2 value
//! is used: the ocio-rs CPU renderers reproduce OCIO's SSE2 builds (see
//! `PORTING.md`).

mod wgsl_common;

use ocio::{
    Config, ExponentTransform, ExponentWithLinearTransform, NegativeStyle, Transform,
    TransformDirection,
};
use wgsl_common::gpu_unit::{gpu_test, GpuTest};

const EPSILON: f32 = 1e-6;

/// Config major version.
const OCIO_VERSION_1: u32 = 1;
/// Version 2 or higher.
const OCIO_VERSION_2: u32 = 2;

/// Helper method to build unit tests.
fn add_exponent(
    test: &mut GpuTest,
    direction: TransformDirection,
    gamma: &[f64; 4],
    style: NegativeStyle,
    epsilon: f32,
    ver: u32,
) {
    let exp = ExponentTransform {
        negative_style: style,
        direction,
        value: *gamma,
        ..Default::default()
    };

    let mut config = Config::create();
    config.set_major_version(ver).unwrap();
    test.set_error_threshold(epsilon);
    test.set_processor_config(&config, Transform::Exponent(exp));
}

/// Helper method to build unit tests.
fn add_exponent_with_linear(
    test: &mut GpuTest,
    direction: TransformDirection,
    gamma: &[f64; 4],
    offset: &[f64; 4],
    style: NegativeStyle,
    epsilon: f32,
) {
    let exp = ExponentWithLinearTransform {
        direction,
        gamma: *gamma,
        offset: *offset,
        negative_style: style,
        ..Default::default()
    };

    let config = Config::create();
    test.set_error_threshold(epsilon);
    test.set_processor_config(&config, Transform::ExponentWithLinear(exp));
}

const EXP: [f64; 4] = [2.6, 1.0, 1.8, 1.1];

#[test]
fn legacy_shader_v1() {
    gpu_test("ExponentOp/legacy_shader_v1", |t| {
        add_exponent(
            t,
            TransformDirection::Forward,
            &EXP,
            NegativeStyle::Clamp,
            1e-5,
            OCIO_VERSION_1,
        );
        t.set_legacy_shader(true);
        // TODO: Would like to be able to remove the setTestNaN(false) and
        // setTestInfinity(false) from all of these tests.
        t.set_test_nan(false);
    });
}

#[test]
fn forward_v1() {
    gpu_test("ExponentOp/forward_v1", |t| {
        add_exponent(
            t,
            TransformDirection::Forward,
            &EXP,
            NegativeStyle::Clamp,
            1e-5,
            OCIO_VERSION_1,
        );
        t.set_test_nan(false);
    });
}

#[test]
fn forward() {
    gpu_test("ExponentOp/forward", |t| {
        add_exponent(
            t,
            TransformDirection::Forward,
            &EXP,
            NegativeStyle::Clamp,
            5e-4, // 1e-5 without OCIO_USE_SSE2.
            OCIO_VERSION_2,
        );
    });
}

#[test]
fn forward_mirror() {
    gpu_test("ExponentOp/forward_mirror", |t| {
        add_exponent(
            t,
            TransformDirection::Forward,
            &EXP,
            NegativeStyle::Mirror,
            // TODO: Only related to the ssePower optimization ?
            5e-4, // 1e-5 without OCIO_USE_SSE2.
            OCIO_VERSION_2,
        );
        t.set_test_nan(false);
    });
}

#[test]
fn forward_pass_thru() {
    gpu_test("ExponentOp/forward_pass_thru", |t| {
        add_exponent(
            t,
            TransformDirection::Forward,
            &EXP,
            NegativeStyle::PassThru,
            // TODO: Only related to the ssePower optimization ?
            5e-4, // 1e-5 without OCIO_USE_SSE2.
            OCIO_VERSION_2,
        );
        t.set_test_infinity(false);
    });
}

#[test]
fn inverse_legacy_shader_v1() {
    gpu_test("ExponentOp/inverse_legacy_shader_v1", |t| {
        add_exponent(
            t,
            TransformDirection::Inverse,
            &EXP,
            NegativeStyle::Clamp,
            EPSILON,
            OCIO_VERSION_1,
        );
        t.set_legacy_shader(true);
        t.set_test_nan(false);
    });
}

#[test]
fn inverse_v1() {
    gpu_test("ExponentOp/inverse_v1", |t| {
        add_exponent(
            t,
            TransformDirection::Inverse,
            &EXP,
            NegativeStyle::Clamp,
            EPSILON,
            OCIO_VERSION_1,
        );
        t.set_test_nan(false);
    });
}

#[test]
fn inverse() {
    gpu_test("ExponentOp/inverse", |t| {
        add_exponent(
            t,
            TransformDirection::Inverse,
            &EXP,
            NegativeStyle::Clamp,
            // TODO: Only related to the ssePower optimization ?
            5e-4, // EPSILON without OCIO_USE_SSE2.
            OCIO_VERSION_2,
        );
        t.set_test_infinity(false);
    });
}

#[test]
fn inverse_mirror() {
    gpu_test("ExponentOp/inverse_mirror", |t| {
        add_exponent(
            t,
            TransformDirection::Inverse,
            &EXP,
            NegativeStyle::Mirror,
            // TODO: Only related to the ssePower optimization ?
            5e-4, // EPSILON without OCIO_USE_SSE2.
            OCIO_VERSION_2,
        );
        t.set_test_infinity(false);
        t.set_test_nan(false);
    });
}

#[test]
fn inverse_pass_thru() {
    gpu_test("ExponentOp/inverse_pass_thru", |t| {
        add_exponent(
            t,
            TransformDirection::Inverse,
            &EXP,
            NegativeStyle::PassThru,
            // TODO: Only related to the ssePower optimization ?
            5e-4, // EPSILON without OCIO_USE_SSE2.
            OCIO_VERSION_2,
        );
        t.set_test_infinity(false);
    });
}

const GAMMA_VALS: [f64; 4] = [2.1, 1.0, 2.3, 1.5];
const OFFSET_VALS: [f64; 4] = [0.01, 0.0, 0.03, 0.05];

#[test]
fn exponent_with_linear_forward() {
    gpu_test("ExponentWithLinearOp/forward", |t| {
        add_exponent_with_linear(
            t,
            TransformDirection::Forward,
            &GAMMA_VALS,
            &OFFSET_VALS,
            NegativeStyle::Linear,
            // Note: Related to the ssePower optimization !
            1e-4, // 5e-6 without OCIO_USE_SSE2.
        );
        t.set_test_infinity(false);
    });
}

#[test]
fn exponent_with_linear_mirror_forward() {
    gpu_test("ExponentWithLinearOp/mirror_forward", |t| {
        add_exponent_with_linear(
            t,
            TransformDirection::Forward,
            &GAMMA_VALS,
            &OFFSET_VALS,
            NegativeStyle::Mirror,
            // Note: Related to the ssePower optimization !
            1e-4, // 5e-6 without OCIO_USE_SSE2.
        );
        t.set_test_infinity(false);
    });
}

#[test]
fn exponent_with_linear_inverse() {
    gpu_test("ExponentWithLinearOp/inverse", |t| {
        add_exponent_with_linear(
            t,
            TransformDirection::Inverse,
            &GAMMA_VALS,
            &OFFSET_VALS,
            NegativeStyle::Linear,
            // Note: Related to the ssePower optimization !
            5e-5, // 5e-7 without OCIO_USE_SSE2.
        );
        t.set_test_infinity(false);
    });
}

#[test]
fn exponent_with_linear_mirror_inverse() {
    gpu_test("ExponentWithLinearOp/mirror_inverse", |t| {
        add_exponent_with_linear(
            t,
            TransformDirection::Inverse,
            &GAMMA_VALS,
            &OFFSET_VALS,
            NegativeStyle::Mirror,
            // Note: Related to the ssePower optimization !
            5e-5, // 5e-7 without OCIO_USE_SSE2.
        );
        t.set_test_infinity(false);
    });
}
