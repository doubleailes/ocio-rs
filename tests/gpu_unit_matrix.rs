// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of the OpenColorIO GPU unit tests `tests/gpu/MatrixOp_test.cpp`,
//! running the WGSL shaders (see `wgsl_common::gpu_unit`).

mod wgsl_common;

use ocio::{MatrixTransform, Transform, TransformDirection};
use wgsl_common::gpu_unit::{gpu_test, GpuTest};

// 1e-6 accommodates 1-2 ULP FMA rounding with DirectX tests (matrix output
// values ~7).
const EPSILON: f32 = 1e-6;

const M: [f64; 16] = [
    1.1, 0.2, 0.3, 0.4, //
    0.5, 1.6, 0.7, 0.8, //
    0.2, 0.1, 1.1, 0.2, //
    0.3, 0.4, 0.5, 1.6,
];

const SCALE: [f64; 16] = [
    1.0, 0.0, 0.0, 0.0, //
    0.0, -0.3, 0.0, 0.0, //
    0.0, 0.0, 0.6, 0.0, //
    0.0, 0.0, 0.0, 1.0,
];

fn add_matrix_test(
    test: &mut GpuTest,
    direction: TransformDirection,
    m: Option<&[f64; 16]>,
    o: Option<&[f64; 4]>,
    generic_shader_desc: bool,
) {
    let mut matrix = MatrixTransform {
        direction,
        ..Default::default()
    };
    if let Some(m) = m {
        matrix.matrix = *m;
    }
    if let Some(o) = o {
        matrix.offset = *o;
    }
    test.set_error_threshold(EPSILON);
    test.set_processor_transform(Transform::Matrix(matrix));
    test.set_legacy_shader(!generic_shader_desc);
}

#[test]
fn matrix() {
    gpu_test("MatrixOps/matrix", |t| {
        add_matrix_test(t, TransformDirection::Forward, Some(&M), None, false)
    });
}

#[test]
fn scale() {
    gpu_test("MatrixOps/scale", |t| {
        add_matrix_test(t, TransformDirection::Forward, Some(&SCALE), None, false)
    });
}

#[test]
fn offset() {
    gpu_test("MatrixOps/offset", |t| {
        add_matrix_test(
            t,
            TransformDirection::Forward,
            None,
            Some(&[-0.5, 0.25, -0.25, 0.0]),
            false,
        )
    });
}

#[test]
fn matrix_offset() {
    gpu_test("MatrixOps/matrix_offset", |t| {
        add_matrix_test(
            t,
            TransformDirection::Forward,
            Some(&M),
            Some(&[-0.5, -0.25, 0.25, 0.0]),
            false,
        )
    });
}

#[test]
fn matrix_inverse() {
    gpu_test("MatrixOps/matrix_inverse", |t| {
        add_matrix_test(t, TransformDirection::Inverse, Some(&M), None, false)
    });
}

#[test]
fn scale_inverse() {
    gpu_test("MatrixOps/scale_inverse", |t| {
        add_matrix_test(t, TransformDirection::Inverse, Some(&SCALE), None, false)
    });
}

#[test]
fn offset_inverse() {
    gpu_test("MatrixOps/offset_inverse", |t| {
        add_matrix_test(
            t,
            TransformDirection::Inverse,
            None,
            Some(&[-0.5, 0.25, -0.25, 0.0]),
            false,
        )
    });
}

#[test]
fn matrix_offset_inverse() {
    gpu_test("MatrixOps/matrix_offset_inverse", |t| {
        add_matrix_test(
            t,
            TransformDirection::Inverse,
            Some(&M),
            Some(&[-0.5, -0.25, 0.25, 0.0]),
            false,
        )
    });
}

#[test]
fn matrix_offset_generic_shader() {
    gpu_test("MatrixOps/matrix_offset_generic_shader", |t| {
        add_matrix_test(
            t,
            TransformDirection::Forward,
            Some(&M),
            Some(&[-0.0, -0.25, 0.25, 0.0]),
            true,
        )
    });
}

#[test]
fn matrix_offset_inverse_generic_shader() {
    gpu_test("MatrixOps/matrix_offset_inverse_generic_shader", |t| {
        add_matrix_test(
            t,
            TransformDirection::Inverse,
            Some(&M),
            Some(&[-0.5, -0.25, 0.25, 0.0]),
            true,
        )
    });
}
