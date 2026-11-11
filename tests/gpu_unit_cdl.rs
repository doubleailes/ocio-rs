// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of the OpenColorIO GPU unit tests `tests/gpu/CDLOp_test.cpp`,
//! running the WGSL shaders (see `wgsl_common::gpu_unit`).

mod wgsl_common;

use ocio::{CdlStyle, CdlTransform, Config, Transform, TransformDirection};
use wgsl_common::gpu_unit::{gpu_test, GpuTest};

// TODO: Add ctf file unit tests when the CLF reader is in.

/// The CDL values of a test.
struct CdlData {
    slope: [f64; 3],
    offset: [f64; 3],
    power: [f64; 3],
    /// `None` keeps the default saturation (`CDL_Data_1` does not set it).
    saturation: Option<f64>,
}

const CDL_DATA_1: CdlData = CdlData {
    slope: [1.35, 1.10, 0.71],
    offset: [0.05, -0.23, 0.11],
    power: [0.93, 0.81, 1.27],
    saturation: None,
};

const CDL_DATA_2: CdlData = CdlData {
    slope: [1.15, 1.10, 0.90],
    offset: [0.05, -0.02, 0.07],
    power: [1.20, 0.95, 1.13],
    saturation: Some(0.9),
};

const CDL_DATA_3: CdlData = CdlData {
    slope: [3.405, 1.0, 1.0],
    offset: [-0.178, -0.178, -0.178],
    power: [1.095, 1.095, 1.0],
    saturation: Some(1.2),
};

/// Set the processor of the CDL in a new config: `style` is `None` to keep
/// the default style of `CDLTransform::Create()`, `major_version` is `Some`
/// when the test calls `config->setMajorVersion()`.
fn set_cdl_processor(
    test: &mut GpuTest,
    style: Option<CdlStyle>,
    direction: TransformDirection,
    data: &CdlData,
    major_version: Option<u32>,
) {
    let mut cdl = CdlTransform {
        direction,
        slope: data.slope,
        offset: data.offset,
        power: data.power,
        ..Default::default()
    };
    if let Some(style) = style {
        cdl.style = style;
    }
    if let Some(sat) = data.saturation {
        cdl.sat = sat;
    }

    let mut config = Config::create();
    if let Some(version) = major_version {
        config.set_major_version(version).unwrap();
    }

    test.set_processor_config(&config, Transform::Cdl(cdl));
}

// Use the legacy shader description with the CDL from OCIO v1 implementation.
#[test]
fn clamp_fwd_v1_legacy_shader() {
    gpu_test("CDLOp/clamp_fwd_v1_legacy_shader", |t| {
        set_cdl_processor(t, None, TransformDirection::Forward, &CDL_DATA_1, Some(1));

        t.set_legacy_shader(true);
        t.set_test_wide_range(true);
        t.set_relative_comparison(false);
        t.set_error_threshold(1e-6);
        // TODO: Would like to be able to remove the setTestNaN(false)
        // from all of these tests.
        t.set_test_nan(false);
    });
}

// Use the generic shader description with the CDL from OCIO v1 implementation.
#[test]
fn clamp_fwd_v1() {
    gpu_test("CDLOp/clamp_fwd_v1", |t| {
        set_cdl_processor(t, None, TransformDirection::Forward, &CDL_DATA_1, Some(1));

        t.set_test_wide_range(true);
        t.set_relative_comparison(false);
        t.set_error_threshold(1e-6);
        t.set_test_nan(false);
    });
}

// Use the generic shader description with the CDL from OCIO v2 implementation
// (i.e. use the CDL Op with the ASC/clamping style and a forward direction).
#[test]
fn clamp_fwd_v2() {
    gpu_test("CDLOp/clamp_fwd_v2", |t| {
        set_cdl_processor(
            t,
            Some(CdlStyle::Asc),
            TransformDirection::Forward,
            &CDL_DATA_1,
            None,
        );

        t.set_test_wide_range(true);
        t.set_relative_comparison(false);
        t.set_error_threshold(1e-5);
    });
}

#[test]
fn clamp_fwd_no_clamp_v2() {
    gpu_test("CDLOp/clamp_fwd_no_clamp_v2", |t| {
        set_cdl_processor(
            t,
            Some(CdlStyle::NoClamp),
            TransformDirection::Forward,
            &CDL_DATA_1,
            None,
        );

        t.set_test_wide_range(true);
        t.set_relative_comparison(false);
        t.set_error_threshold(5e-5);
        t.set_test_nan(false);
        t.set_test_infinity(false);
    });
}

// Use the generic shader description with the CDL from OCIO v2 implementation
// (i.e. use the CDL Op with the ASC/clamping style and a inverse direction).
#[test]
fn clamp_inv_v2() {
    gpu_test("CDLOp/clamp_inv_v2", |t| {
        set_cdl_processor(
            t,
            Some(CdlStyle::Asc),
            TransformDirection::Inverse,
            &CDL_DATA_1,
            None,
        );

        t.set_test_wide_range(true);
        t.set_relative_comparison(false);
        t.set_error_threshold(1e-4);
    });
}

#[test]
fn clamp_inv_no_clamp_v2() {
    gpu_test("CDLOp/clamp_inv_no_clamp_v2", |t| {
        set_cdl_processor(
            t,
            Some(CdlStyle::NoClamp),
            TransformDirection::Inverse,
            &CDL_DATA_1,
            None,
        );

        t.set_test_wide_range(true);
        t.set_relative_comparison(false);
        t.set_error_threshold(1e-4);
        t.set_test_nan(false);
        t.set_test_infinity(false);
    });
}

// Use the legacy shader description with the CDL from OCIO v1 implementation.
#[test]
fn clamp_fwd_v1_legacy_shader_data_2() {
    gpu_test("CDLOp/clamp_fwd_v1_legacy_shader_Data_2", |t| {
        set_cdl_processor(t, None, TransformDirection::Forward, &CDL_DATA_2, Some(1));

        t.set_legacy_shader(true);
        t.set_test_wide_range(true);
        t.set_relative_comparison(false);
        t.set_error_threshold(1e-6);
        t.set_test_nan(false);
        t.set_test_infinity(false);
    });
}

// Use the generic shader description with the CDL from OCIO v1 implementation.
#[test]
fn clamp_fwd_v1_data_2() {
    gpu_test("CDLOp/clamp_fwd_v1_Data_2", |t| {
        set_cdl_processor(t, None, TransformDirection::Forward, &CDL_DATA_2, Some(1));

        t.set_test_wide_range(true);
        t.set_relative_comparison(false);
        t.set_error_threshold(1e-6);
        t.set_test_nan(false);
        t.set_test_infinity(false);
    });
}

// Use the generic shader description with the CDL from OCIO v2 implementation
// (i.e. use the CDL Op with the ASC/clamping style and a forward direction).
#[test]
fn clamp_fwd_v2_data_2() {
    gpu_test("CDLOp/clamp_fwd_v2_Data_2", |t| {
        set_cdl_processor(
            t,
            Some(CdlStyle::Asc),
            TransformDirection::Forward,
            &CDL_DATA_2,
            None,
        );

        t.set_test_wide_range(true);
        t.set_relative_comparison(false);
        t.set_error_threshold(2e-5);
    });
}

#[test]
fn clamp_inv_v2_data_2() {
    gpu_test("CDLOp/clamp_inv_v2_Data_2", |t| {
        set_cdl_processor(
            t,
            Some(CdlStyle::Asc),
            TransformDirection::Inverse,
            &CDL_DATA_2,
            None,
        );

        t.set_test_wide_range(true);
        t.set_relative_comparison(false);
        t.set_error_threshold(2e-5);
    });
}

#[test]
fn clamp_fwd_no_clamp_v2_data_2() {
    gpu_test("CDLOp/clamp_fwd_no_clamp_v2_Data_2", |t| {
        set_cdl_processor(
            t,
            Some(CdlStyle::NoClamp),
            TransformDirection::Forward,
            &CDL_DATA_2,
            None,
        );

        t.set_test_wide_range(true);
        t.set_relative_comparison(false);
        t.set_error_threshold(5e-5);
        t.set_test_nan(false);
        t.set_test_infinity(false);
    });
}

#[test]
fn clamp_inv_no_clamp_v2_data_2() {
    gpu_test("CDLOp/clamp_inv_no_clamp_v2_Data_2", |t| {
        set_cdl_processor(
            t,
            Some(CdlStyle::NoClamp),
            TransformDirection::Inverse,
            &CDL_DATA_2,
            None,
        );

        t.set_test_wide_range(true);
        t.set_relative_comparison(false);
        t.set_error_threshold(5e-5);
        t.set_test_nan(false);
        t.set_test_infinity(false);
    });
}

// Use the generic shader description with the CDL from OCIO v2 implementation
// (i.e. use the CDL Op with the ASC/clamping style and a forward direction).
#[test]
fn clamp_fwd_v2_data_3() {
    gpu_test("CDLOp/clamp_fwd_v2_Data_3", |t| {
        set_cdl_processor(
            t,
            Some(CdlStyle::Asc),
            TransformDirection::Forward,
            &CDL_DATA_3,
            None,
        );

        t.set_test_wide_range(true);
        t.set_relative_comparison(false);
        t.set_error_threshold(5e-5);
    });
}

#[test]
fn clamp_fwd_no_clamp_v2_data_3() {
    gpu_test("CDLOp/clamp_fwd_no_clamp_v2_Data_3", |t| {
        set_cdl_processor(
            t,
            Some(CdlStyle::NoClamp),
            TransformDirection::Forward,
            &CDL_DATA_3,
            None,
        );

        t.set_test_wide_range(false);
        t.set_relative_comparison(false);
        t.set_error_threshold(5e-5);
        t.set_test_nan(false);
        t.set_test_infinity(false);
    });
}

#[test]
fn clamp_inv_no_clamp_v2_data_3() {
    gpu_test("CDLOp/clamp_inv_no_clamp_v2_Data_3", |t| {
        set_cdl_processor(
            t,
            Some(CdlStyle::NoClamp),
            TransformDirection::Inverse,
            &CDL_DATA_3,
            None,
        );

        t.set_test_wide_range(false);
        t.set_relative_comparison(false);
        t.set_error_threshold(5e-5);
        t.set_test_nan(false);
        t.set_test_infinity(false);
    });
}
