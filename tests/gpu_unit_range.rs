// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Copyright (c) 2026 Philippe Llerena.

//! Port of the OpenColorIO GPU unit tests `tests/gpu/RangeOp_test.cpp`,
//! running the WGSL shaders (see `wgsl_common::gpu_unit`).

mod wgsl_common;

use ocio::{RangeStyle, RangeTransform, Transform};
use wgsl_common::gpu_unit::gpu_test;

const EPSILON: f32 = 1e-6;

/// Port of `RangeTransform::Create()` followed by the `set*Value` calls
/// (upstream passes `float` values, hence the `f32` parameters).
fn range(
    min_in: Option<f32>,
    max_in: Option<f32>,
    min_out: Option<f32>,
    max_out: Option<f32>,
) -> RangeTransform {
    RangeTransform {
        min_in: min_in.map(f64::from),
        max_in: max_in.map(f64::from),
        min_out: min_out.map(f64::from),
        max_out: max_out.map(f64::from),
        ..Default::default()
    }
}

#[test]
fn scale_with_low_and_high_clippings() {
    gpu_test("RangeOp/scale_with_low_and_high_clippings", |t| {
        let range = range(Some(0.1), Some(1.1), Some(0.5), Some(1.5));
        t.set_processor_transform(Transform::Range(range));
        t.set_test_wide_range(true);
        t.set_error_threshold(EPSILON);
    });
}

#[test]
fn scale_with_low_clipping() {
    gpu_test("RangeOp/scale_with_low_clipping", |t| {
        let range = range(Some(0.2), None, Some(0.2), None);
        t.set_processor_transform(Transform::Range(range));
        t.set_error_threshold(EPSILON);
    });
}

#[test]
fn scale_with_high_clipping() {
    gpu_test("RangeOp/scale_with_high_clipping", |t| {
        let range = range(None, Some(0.9), None, Some(0.9));
        t.set_processor_transform(Transform::Range(range));
        t.set_error_threshold(EPSILON);
    });
}

#[test]
fn scale_with_low_and_high_clippings_2() {
    gpu_test("RangeOp/scale_with_low_and_high_clippings_2", |t| {
        // The similar test above is only an offset, this test is a scale & offset.
        let range = range(Some(0.1), Some(1.1), Some(-0.5), Some(1.5));
        t.set_processor_transform(Transform::Range(range));
        t.set_test_wide_range(true);
        t.set_error_threshold(EPSILON);
    });
}

#[test]
fn arbitrary_1() {
    gpu_test("RangeOp/arbitrary_1", |t| {
        let range = range(
            Some(0.4000202),
            Some(0.6000502),
            Some(0.4000601),
            Some(0.6000801),
        );
        t.set_processor_transform(Transform::Range(range));
        t.set_error_threshold(EPSILON);
    });
}

#[test]
fn arbitrary_1_no_clamp() {
    gpu_test("RangeOp/arbitrary_1_no_clamp", |t| {
        let mut range = range(
            Some(0.4000202),
            Some(0.6000502),
            Some(0.4000601),
            Some(0.6000801),
        );
        range.style = RangeStyle::NoClamp;
        t.set_processor_transform(Transform::Range(range));
        t.set_error_threshold(EPSILON);
    });
}

#[test]
fn arbitrary_2() {
    gpu_test("RangeOp/arbitrary_2", |t| {
        let range = range(
            Some(-0.010201),
            Some(0.601102),
            Some(0.209803),
            Some(1.600208),
        );
        t.set_processor_transform(Transform::Range(range));
        t.set_test_wide_range(true);
        t.set_error_threshold(EPSILON);
    });
}

#[test]
fn arbitrary_2_no_clamp() {
    gpu_test("RangeOp/arbitrary_2_no_clamp", |t| {
        let mut range = range(
            Some(-0.010201),
            Some(0.601102),
            Some(0.209803),
            Some(1.600208),
        );
        range.style = RangeStyle::NoClamp;
        t.set_processor_transform(Transform::Range(range));
        t.set_test_wide_range(true);
        t.set_error_threshold(EPSILON);
    });
}
