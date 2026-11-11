// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of the OpenColorIO GPU unit tests `tests/gpu/Lut1DOp_test.cpp`,
//! running the WGSL shaders (see `wgsl_common::gpu_unit`).

mod wgsl_common;

use ocio::gpu::GpuShaderCreator;
use ocio::{FileTransform, Interpolation, Lut1DTransform, Transform, TransformDirection};
use wgsl_common::fixture;
use wgsl_common::gpu_unit::gpu_test;

const LUT3D_EDGE_SIZE: u32 = 32;

/// The file transform of the test file `filename`, or `None` when the file
/// is not in the published crate.
fn get_file_transform(filename: &str) -> Option<FileTransform> {
    Some(FileTransform {
        src: fixture(filename)?,
        ..Default::default()
    })
}

/// Shortcut for `get_file_transform` with a direction.
fn file_transform(filename: &str, direction: TransformDirection) -> Option<Transform> {
    let mut file = get_file_transform(filename)?;
    file.direction = direction;
    Some(Transform::File(file))
}

// The LUTs below are identities unless otherwise noted.
// Various sizes are used to test different 1D LUT texture packings on the GPU.
// lut1d_1.spi1d has    512 entries
// lut1d_2.spi1d has   8192 entries
// lut1d_3.spi1d has 131072 entries

#[test]
fn lut1d_1_small_legacy_shader() {
    let Some(file) = file_transform("lut1d_1.spi1d", TransformDirection::Forward) else {
        return;
    };
    gpu_test("Lut1DOp/lut1d_1_small_legacy_shader", |t| {
        t.set_processor_transform(file);

        t.set_legacy_shader(true);
        t.set_error_threshold(2e-4);
    });
}

#[test]
fn lut1d_1_small_inverse_legacy_shader() {
    let Some(file) = file_transform("lut1d_1.spi1d", TransformDirection::Inverse) else {
        return;
    };
    gpu_test("Lut1DOp/lut1d_1_small_inverse_legacy_shader", |t| {
        t.set_processor_transform(file);

        t.set_legacy_shader(true);
        t.set_error_threshold(2e-4);
    });
}

#[test]
fn lut1d_1_small_generic_shader() {
    let Some(file) = file_transform("lut1d_1.spi1d", TransformDirection::Forward) else {
        return;
    };
    gpu_test("Lut1DOp/lut1d_1_small_generic_shader", |t| {
        t.set_processor_transform(file);

        t.set_error_threshold(5e-6);
    });
}

#[test]
fn lut1d_1_small_inverse_generic_shader() {
    let Some(file) = file_transform("lut1d_1.spi1d", TransformDirection::Inverse) else {
        return;
    };
    gpu_test("Lut1DOp/lut1d_1_small_inverse_generic_shader", |t| {
        t.set_processor_transform(file);

        t.set_error_threshold(1e-6);
        t.set_test_nan(false);
    });
}

#[test]
fn lut1d_2_legacy_shader() {
    let Some(file) = file_transform("lut1d_2.spi1d", TransformDirection::Forward) else {
        return;
    };
    gpu_test("Lut1DOp/lut1d_2_legacy_shader", |t| {
        t.set_processor_transform(file);

        t.set_legacy_shader(true);
        t.set_legacy_shader_lut_edge(2 * LUT3D_EDGE_SIZE);
        t.set_error_threshold(2e-4);
    });
}

#[test]
fn lut1d_2_inverse_legacy_shader() {
    let Some(file) = file_transform("lut1d_2.spi1d", TransformDirection::Inverse) else {
        return;
    };
    gpu_test("Lut1DOp/lut1d_2_inverse_legacy_shader", |t| {
        t.set_processor_transform(file);

        t.set_legacy_shader(true);
        t.set_legacy_shader_lut_edge(2 * LUT3D_EDGE_SIZE);
        t.set_error_threshold(2e-4);
    });
}

#[test]
fn lut1d_2_generic_shader() {
    let Some(file) = file_transform("lut1d_2.spi1d", TransformDirection::Forward) else {
        return;
    };
    gpu_test("Lut1DOp/lut1d_2_generic_shader", |t| {
        t.set_processor_transform(file);

        t.set_error_threshold(1e-6);

        // TODO: Would like to be able to remove the setTestNaN(false) and
        // setTestInfinity(false) from all of these tests.
        t.set_test_nan(false);
    });
}

#[test]
fn lut1d_2_inverse_generic_shader() {
    let Some(file) = file_transform("lut1d_2.spi1d", TransformDirection::Inverse) else {
        return;
    };
    gpu_test("Lut1DOp/lut1d_2_inverse_generic_shader", |t| {
        t.set_processor_transform(file);

        t.set_error_threshold(1e-4);
        t.set_test_nan(false);
    });
}

#[test]
fn lut1d_3_big_legacy_shader() {
    let Some(file) = file_transform("lut1d_3.spi1d", TransformDirection::Forward) else {
        return;
    };
    gpu_test("Lut1DOp/lut1d_3_big_legacy_shader", |t| {
        t.set_processor_transform(file);

        t.set_legacy_shader(true);
        t.set_error_threshold(2e-4);
    });
}

#[test]
fn lut1d_3_big_inverse_legacy_shader() {
    let Some(file) = file_transform("lut1d_3.spi1d", TransformDirection::Inverse) else {
        return;
    };
    gpu_test("Lut1DOp/lut1d_3_big_inverse_legacy_shader", |t| {
        t.set_processor_transform(file);

        t.set_legacy_shader(true);
        t.set_error_threshold(2e-4);
    });
}

#[test]
fn lut1d_3_big_generic_shader() {
    let Some(file) = file_transform("lut1d_3.spi1d", TransformDirection::Forward) else {
        return;
    };
    gpu_test("Lut1DOp/lut1d_3_big_generic_shader", |t| {
        t.set_processor_transform(file);

        t.set_error_threshold(1e-6);
        t.set_test_nan(false);
    });
}

#[test]
fn lut1d_3_big_inverse_generic_shader() {
    let Some(file) = file_transform("lut1d_3.spi1d", TransformDirection::Inverse) else {
        return;
    };
    gpu_test("Lut1DOp/lut1d_3_big_inverse_generic_shader", |t| {
        t.set_processor_transform(file);

        t.set_error_threshold(1e-6);
        t.set_test_nan(false);
    });
}

#[test]
fn lut1d_3_big_nearest_generic_shader() {
    let Some(mut file) = get_file_transform("lut1d_3.spi1d") else {
        return;
    };
    file.interpolation = Interpolation::Nearest;
    gpu_test("Lut1DOp/lut1d_3_big_nearest_generic_shader", |t| {
        t.set_processor_transform(Transform::File(file));

        t.set_error_threshold(1e-6);
        t.set_test_nan(false);
    });
}

#[test]
fn scale_lut1d_4_legacy_shader() {
    let Some(file) = file_transform("lut1d_4.spi1d", TransformDirection::Forward) else {
        return;
    };
    gpu_test("Lut1DOp/scale_lut1d_4_legacy_shader", |t| {
        t.set_processor_transform(file);

        t.set_legacy_shader(true);
        t.set_legacy_shader_lut_edge(2 * LUT3D_EDGE_SIZE);
        t.set_error_threshold(2e-4);

        // lut1d_4.spi1d has values outside [0, 1]. Legacy shader is baking ops
        // into a 3D LUT and would clamp outside of [0, 1].
        t.set_test_wide_range(false);

        t.set_test_nan(false);
        t.set_test_infinity(false);
    });
}

#[test]
fn scale_lut1d_4_inverse_legacy_shader() {
    let Some(file) = file_transform("lut1d_4.spi1d", TransformDirection::Inverse) else {
        return;
    };
    gpu_test("Lut1DOp/scale_lut1d_4_inverse_legacy_shader", |t| {
        t.set_processor_transform(file);

        t.set_legacy_shader(true);
        t.set_legacy_shader_lut_edge(2 * LUT3D_EDGE_SIZE);
        t.set_error_threshold(5e-5);

        t.set_test_wide_range(false);

        t.set_test_nan(false);
        t.set_test_infinity(false);
    });
}

#[test]
fn scale_lut1d_4_generic_shader() {
    let Some(file) = file_transform("lut1d_4.spi1d", TransformDirection::Forward) else {
        return;
    };
    gpu_test("Lut1DOp/scale_lut1d_4_generic_shader", |t| {
        t.set_processor_transform(file);

        // TODO: Should be smaller.
        t.set_error_threshold(1e-4);

        t.set_test_nan(false);
    });
}

#[test]
fn scale_lut1d_4_inverse_generic_shader() {
    let Some(file) = file_transform("lut1d_4.spi1d", TransformDirection::Inverse) else {
        return;
    };
    gpu_test("Lut1DOp/scale_lut1d_4_inverse_generic_shader", |t| {
        t.set_processor_transform(file);

        t.set_error_threshold(1e-6);

        t.set_test_nan(false);
        t.set_test_infinity(false);
    });
}

#[test]
fn not_linear_lut1d_5_generic_shader() {
    let Some(file) = file_transform("lut1d_5.spi1d", TransformDirection::Forward) else {
        return;
    };
    gpu_test("Lut1DOp/not_linear_lut1d_5_generic_shader", |t| {
        t.set_processor_transform(file);

        t.set_error_threshold(5e-4); // Good value for a relative error threshold.

        t.set_relative_comparison(true); // LUT contains values from 0.0f to 64.0f
                                         // explaining why an absolute error could not be used.
    });
}

#[test]
fn not_linear_lut1d_5_inverse_generic_shader() {
    let Some(file) = file_transform("lut1d_5.spi1d", TransformDirection::Inverse) else {
        return;
    };
    gpu_test("Lut1DOp/not_linear_lut1d_5_inverse_generic_shader", |t| {
        t.set_processor_transform(file);

        t.set_error_threshold(1e-6);
    });
}

#[test]
fn lut1d_half_domain_unequal_channels() {
    let Some(file) = file_transform("lut1d_halfdom.ctf", TransformDirection::Forward) else {
        return;
    };
    gpu_test("Lut1DOp/lut1d_half_domain_unequal_channels", |t| {
        t.set_processor_transform(file);

        t.set_error_threshold(1e-6);

        t.set_test_nan(false);
        t.set_test_infinity(false);
    });
}

#[test]
fn lut1d_half_domain_negative_zero() {
    gpu_test("Lut1DOp/lut1d_half_domain_negative_zero", |t| {
        // This is an edge case, but this test documents that the behavior of CPU & GPU
        // are different with respect to where in the LUT negative zero looks up at.
        // This is only visible with half-domain LUTs that set different values for
        // positive and negative zero, which really should be considered a bug in the LUT.
        // Given that IEEE arithmetic specifies that -0 == +0 in comparisons, this does
        // not seem to be worth fixing in OCIO at the cost of reduced performance.

        // Create a half-domain LUT1D.
        let mut lut = Lut1DTransform::new(65536, true);

        // Set the positive and negative denorms to large values to make it easy
        // to check that the processing is correct.
        for i in 0..1024 {
            let x = i as f32;
            // Positive denorms.
            lut.set_value(i, x, x, x);
            // Negative denorms.  Create a jump between +0 and -0.
            lut.set_value(32768 + i, x + 10.0, x + 10.0, x + 10.0);
        }

        t.set_processor_transform(Transform::Lut1D(lut));

        // TODO: Would like this to be lower.
        t.set_error_threshold(2e-3);

        #[rustfmt::skip]
        let values = vec![
            // Negative zero uses the positive 0 LUT value on the GPU, and negative 0 LUT on CPU.
            // -0.00, -0.00, -0.000,   0.0,
             0.00,  0.00,  0.000,   1.0,
            // Use values that fall in the middle of the first, second, and third LUT segments
            // to test accuracy in the denormals.
             3e-8,  9e-8,  15e-8,   0.0,
            -3e-8, -9e-8, -15e-8,   0.0,
            // Throw in a more typical value.
             0.50,  0.05,  0.005,   0.5,
        ];
        t.set_custom_values(values);

        t.set_test_nan(false);
        t.set_test_infinity(false);
    });
}

#[test]
fn lut1d_file2_test() {
    let Some(file) = file_transform("lut1d_green.ctf", TransformDirection::Forward) else {
        return;
    };
    gpu_test("Lut1DOp/lut1d_file2_test", |t| {
        t.set_processor_transform(file);

        // LUT has just 32 entries and thus requires a larger tolerance due to
        // index quantization on GPUs.
        t.set_error_threshold(1e-4);
    });
}

#[test]
fn lut1d_file2_disallow_tex1d_test() {
    let Some(file) = file_transform("lut1d_green.ctf", TransformDirection::Forward) else {
        return;
    };
    gpu_test("Lut1DOp/lut1d_file2_disallow_tex1D_test", |t| {
        // Disallow 1D texture resource/sampler types.
        t.shader_desc_setup = Some(Box::new(|desc| desc.set_allow_texture_1d(false)));

        t.set_processor_transform(file);

        // LUT has just 32 entries and thus requires a larger tolerance due to
        // index quantization on GPUs.
        t.set_error_threshold(1e-4);
    });
}

#[test]
fn lut1d_hue_adjust_test() {
    // Note: This LUT has 1024 entries so it tests the "small LUT" path.
    let Some(file) = file_transform(
        "lut1d_1024_hue_adjust_test.ctf",
        TransformDirection::Forward,
    ) else {
        return;
    };
    gpu_test("Lut1DOp/lut1d_hue_adjust_test", |t| {
        t.set_processor_transform(file);

        // NB: This test has required a tolerance of 0.1 on older graphics cards.
        t.set_error_threshold(1e-5);

        t.set_test_nan(false);
        t.set_test_infinity(false);
    });
}

#[test]
fn lut1d_half_domain_hue_adjust_test() {
    // Note: This LUT is half domain and also a "big LUT" so it tests that path.
    let Some(file) = file_transform("lut1d_hd_hue_adjust.ctf", TransformDirection::Forward) else {
        return;
    };
    gpu_test("Lut1DOp/lut1d_half_domain_hue_adjust_test", |t| {
        t.set_processor_transform(file);

        // NB: This test has required a tolerance of 0.1 on older graphics cards.
        t.set_error_threshold(1e-6);

        // LUT range is 0.0001 -> 10000.0.
        t.set_relative_comparison(true);

        t.set_test_nan(false);
        t.set_test_infinity(false);
    });
}

#[test]
fn lut1d_inverse_file1_test() {
    let Some(file) = file_transform("lut1d_inv.ctf", TransformDirection::Forward) else {
        return;
    };
    gpu_test("Lut1DOp/lut1d_inverse_file1_test", |t| {
        t.set_processor_transform(file);

        // Inverse LUT leads bigger errors.
        t.set_error_threshold(1e-4);

        t.set_test_nan(false);
        t.set_test_infinity(false);
    });
}

#[test]
fn lut1d_inverse_file2_test() {
    // This LUT has an extended domain (entries outside [0,1]) and hence the fast LUT
    // that gets built from it must have a halfDomain for both CPU and GPU.
    let Some(file) = file_transform("lut1d_inverse_gpu.ctf", TransformDirection::Forward) else {
        return;
    };
    gpu_test("Lut1DOp/lut1d_inverse_file2_test", |t| {
        t.set_processor_transform(file);

        t.set_error_threshold(1e-6);

        t.set_test_nan(false);
    });
}

#[test]
fn lut1d_inverse_half_file1_test() {
    let Some(file) = file_transform(
        "lut1d_inverse_halfdom_slog_fclut.ctf",
        TransformDirection::Forward,
    ) else {
        return;
    };
    gpu_test("Lut1DOp/lut1d_inverse_half_file1_test", |t| {
        t.set_processor_transform(file);

        t.set_error_threshold(1e-4);

        t.set_relative_comparison(true);
        t.set_expected_minimal_value(1e-3);

        t.set_test_nan(false);
    });
}

#[test]
fn lut1d_inverse_half_hue_adjust_file1_test() {
    let Some(file) = file_transform(
        "lut1d_inverse_hd_hueAdjust.ctf",
        TransformDirection::Forward,
    ) else {
        return;
    };
    gpu_test("Lut1DOp/lut1d_inverse_half_hue_adjust_file1_test", |t| {
        t.set_processor_transform(file);

        t.set_error_threshold(1e-6);

        t.set_test_nan(false);
        t.set_test_infinity(false);
    });
}

// TODO: Add tests with LUTs with Inf and NaN values.
// TODO: Port syncolor test: renderer\test\GPURenderer_cases.cpp_inc - GPURendererLut1D_8i_test
// TODO: Port syncolor test: renderer\test\GPURenderer_cases.cpp_inc - GPURendererLut1D_16i_test
// TODO: Port syncolor test: renderer\test\GPURenderer_cases.cpp_inc - GPURendererLut1D_16f_test
// TODO: Port syncolor test: renderer\test\GPURenderer_cases.cpp_inc - GPURendererLut1D_16f_half_domain_test
// TODO: Port syncolor test: renderer\test\GPURenderer_cases.cpp_inc - GPURendererLut1D_32f_test
// TODO: Port syncolor test: renderer\test\GPURenderer_cases.cpp_inc - GPURendererLut1DTooManyTextures_test
// TODO: Port syncolor test: renderer\test\GPURenderer_cases.cpp_inc - GPURendererLut1D_File1_test
// TODO: Port syncolor test: renderer\test\GPURenderer_cases.cpp_inc - GPURendererLut1D_too_small
// TODO: Port syncolor test: renderer\test\GPURenderer_cases.cpp_inc - GPURendererLut1D_half_domain_test
