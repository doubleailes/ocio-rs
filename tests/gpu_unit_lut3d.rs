//! Port of the OpenColorIO GPU unit tests `tests/gpu/Lut3DOp_test.cpp`,
//! running the WGSL shaders (see `wgsl_common::gpu_unit`).

mod wgsl_common;

use ocio::ops::lut3d::MAX_3D_LUT_LENGTH;
use ocio::{FileTransform, Interpolation, Lut3DTransform, Transform, TransformDirection};
use wgsl_common::gpu_unit::gpu_test;

fn get_file_transform(filename: &str) -> FileTransform {
    FileTransform {
        src: wgsl_common::data_dir()
            .join(filename)
            .to_string_lossy()
            .to_string(),
        ..Default::default()
    }
}

#[test]
fn red_only_using_csp_file_legacy_shader() {
    gpu_test("Lut3DOp/red_only_using_CSP_file_legacy_shader", |t| {
        // Any other 3D LUT file format would have been good also.

        // Create the transform & set the unit test

        let mut file = get_file_transform("lut3d_red-only.csp");
        file.interpolation = Interpolation::Linear;

        t.set_processor_transform(Transform::File(file));

        t.set_legacy_shader(true);
        t.set_error_threshold(2e-4);
    });
}

#[test]
fn green_only_using_csp_file_legacy_shader() {
    gpu_test("Lut3DOp/green_only_using_CSP_file_legacy_shader", |t| {
        // Any other 3D LUT file format would have been good also.

        // Create the transform & set the unit test

        let mut file = get_file_transform("lut3d_grn-only.csp");
        file.interpolation = Interpolation::Linear;

        t.set_processor_transform(Transform::File(file));

        t.set_legacy_shader(true);
        t.set_error_threshold(2e-4);
    });
}

#[test]
fn blue_only_using_csp_file_legacy_shader() {
    gpu_test("Lut3DOp/blue_only_using_CSP_file_legacy_shader", |t| {
        // Any other 3D LUT file format would have been good also.

        // Create the transform & set the unit test

        let mut file = get_file_transform("lut3d_blu-only.csp");
        file.interpolation = Interpolation::Linear;

        t.set_processor_transform(Transform::File(file));

        t.set_legacy_shader(true);
        t.set_error_threshold(2e-4);
    });
}

#[test]
fn arbitrary_using_csp_file_legacy_shader() {
    gpu_test("Lut3DOp/arbitrary_using_CSP_file_legacy_shader", |t| {
        // Any other 3D LUT file format would have been good also.

        // Create the transform & set the unit test

        let mut file = get_file_transform("lut3d_arbitrary.csp");
        file.interpolation = Interpolation::Linear;

        t.set_processor_transform(Transform::File(file));

        t.set_legacy_shader(true);
        t.set_error_threshold(2e-4);
    });
}

#[test]
fn arbitrary_using_csp_file() {
    gpu_test("Lut3DOp/arbitrary_using_CSP_file", |t| {
        // Create the transform & set the unit test

        let mut file = get_file_transform("lut3d_arbitrary.csp");
        file.interpolation = Interpolation::Linear;

        t.set_processor_transform(Transform::File(file));

        // TODO: Small LUTs not being resampled for now, such error threshold is expected
        //       The legacy shader has a better error threshold because
        //       it converts all LUTs in one 3D LUT of dimension LUT3D_EDGE_SIZE
        //       which performs a resampling of small LUTs.
        t.set_error_threshold(1e-2);
    });
}

#[test]
fn lut3d_file_legacy_shader() {
    gpu_test("Lut3DOp/3dlut_file_legacy_shader", |t| {
        let file = get_file_transform("lut3d_1.spi3d");

        t.set_processor_transform(Transform::File(file));

        t.set_legacy_shader(true);
        t.set_error_threshold(5e-4);
    });
}

#[test]
fn lut3d_file_spi3d_linear() {
    gpu_test("Lut3DOp/3dlut_file_spi3d_linear", |t| {
        // Linear interpolation
        let file = get_file_transform("lut3d_1.spi3d");

        t.set_processor_transform(Transform::File(file));

        t.set_error_threshold(5e-4);
    });
}

#[test]
fn lut3d_file_spi3d_tetra() {
    gpu_test("Lut3DOp/3dlut_file_spi3d_tetra", |t| {
        let mut file = get_file_transform("lut3d_1.spi3d");
        file.interpolation = Interpolation::Tetrahedral;

        t.set_processor_transform(Transform::File(file));

        t.set_error_threshold(1e-6);

        // TODO: Would like to be able to remove the setTestNaN(false) and
        // setTestInfinity(false) from all of these tests.
        t.set_test_nan(false);
        t.set_test_infinity(false);
    });
}

#[test]
fn inv3dlut_file_spi3d_linear() {
    gpu_test("Lut3DOp/inv3dlut_file_spi3d_linear", |t| {
        if cfg!(all(debug_assertions, windows)) {
            // TODO: 3D LUT inversion might be very slow in debug on windows.
            t.disable();
        }
        // The test uses the FAST style of inverse on both CPU and GPU.
        // The FAST style uses EXACT inversion to build an approximate inverse
        // that may be applied as a forward Lut3D.
        let mut file = get_file_transform("lut3d_1.spi3d");
        file.direction = TransformDirection::Inverse;

        t.set_processor_transform(Transform::File(file));

        t.set_error_threshold(1.2e-3);
    });
}

#[test]
fn inv3dlut_file_spi3d_tetra() {
    gpu_test("Lut3DOp/inv3dlut_file_spi3d_tetra", |t| {
        if cfg!(all(debug_assertions, windows)) {
            t.disable();
        }
        let mut file = get_file_transform("lut3d_1.spi3d");
        file.direction = TransformDirection::Inverse;
        // Note: Currently the interpolation style is ignored when applying the
        // inverse LUT, so this test produces the same result as the previous one.
        file.interpolation = Interpolation::Tetrahedral;

        t.set_processor_transform(Transform::File(file));

        t.set_error_threshold(1.2e-3);
    });
}

#[test]
fn lut3d_file_spi3d_bizarre_linear() {
    gpu_test("Lut3DOp/3dlut_file_spi3d_bizarre_linear", |t| {
        // Linear interpolation
        let file = get_file_transform("lut3d_bizarre.spi3d");

        t.set_processor_transform(Transform::File(file));

        // This is due to the fact that the LUT is small and to
        // the GPU 8-bit index quantization.
        t.set_error_threshold(1e-2);
    });
}

#[test]
fn lut3d_file_spi3d_bizarre_tetra() {
    gpu_test("Lut3DOp/3dlut_file_spi3d_bizarre_tetra", |t| {
        let mut file = get_file_transform("lut3d_bizarre.spi3d");
        file.interpolation = Interpolation::Tetrahedral;

        t.set_processor_transform(Transform::File(file));

        t.set_error_threshold(1e-6);

        t.set_test_nan(false);
        t.set_test_infinity(false);
    });
}

#[test]
fn inv3dlut_file_spi3d_bizarre_linear() {
    gpu_test("Lut3DOp/inv3dlut_file_spi3d_bizarre_linear", |t| {
        if cfg!(all(debug_assertions, windows)) {
            t.disable();
        }
        let mut file = get_file_transform("lut3d_bizarre.spi3d");
        file.direction = TransformDirection::Inverse;

        t.set_processor_transform(Transform::File(file));

        t.set_error_threshold(3e-4);
    });
}

#[test]
fn inv3dlut_file_spi3d_bizarre_tetra() {
    gpu_test("Lut3DOp/inv3dlut_file_spi3d_bizarre_tetra", |t| {
        if cfg!(all(debug_assertions, windows)) {
            t.disable();
        }
        let mut file = get_file_transform("lut3d_bizarre.spi3d");
        file.direction = TransformDirection::Inverse;
        file.interpolation = Interpolation::Tetrahedral;

        t.set_processor_transform(Transform::File(file));

        t.set_error_threshold(3e-4);
    });
}

#[test]
fn lut3d_biggest_supported() {
    gpu_test("Lut3DOp/3dlut_biggest_supported", |t| {
        // Linear interpolation
        let lut = Lut3DTransform::new(MAX_3D_LUT_LENGTH);

        t.set_processor_transform(Transform::Lut3D(lut));

        t.set_error_threshold(1e-4);
    });
}
