//! Port of the OpenColorIO GPU unit tests `tests/gpu/Config_test.cpp`,
//! running the WGSL shaders (see `wgsl_common::gpu_unit`).

mod wgsl_common;

use ocio::gpu::GpuShaderCreator;
use ocio::Config;
use wgsl_common::gpu_unit::gpu_test;

// Based on testings, the interpolation precision for GPU textures is 8-bits
// so it is the default error threshold for all GPU unit tests.
const DEFAULT_ERROR_THRESHOLD: f32 = 1.0 / 256.0;

fn create_config() -> String {
    let search_path = format!("{}/", wgsl_common::data_dir().to_string_lossy());

    let mut ocio_config_str = String::from(
        "ocio_profile_version: 1\n\
         \n\
         search_path: ",
    );

    ocio_config_str += &search_path;

    ocio_config_str += concat!(
        "\n",
        "strictparsing: true\n",
        "luma: [0.2126, 0.7152, 0.0722]\n",
        "\n",
        "roles:\n",
        "  default: raw\n",
        "  reference: raw\n",
        "  scene_linear: raw\n",
        "\n",
        "displays:\n",
        "  sRGB:\n",
        "    - !<View> {name: Lgh, colorspace: lgh}\n",
        "\n",
        "colorspaces:\n",
        "  - !<ColorSpace>\n",
        "    name: raw\n",
        "    family: \"\"\n",
        "    equalitygroup: \"\"\n",
        "    bitdepth: unknown\n",
        "    isdata: false\n",
        "    allocation: uniform\n",
        "\n",
        "  - !<ColorSpace>\n",
        "    name: lgh\n",
        "    family: \"\"\n",
        "    equalitygroup: \"\"\n",
        "    bitdepth: unknown\n",
        "    isdata: false\n",
        "    allocation: uniform\n",
        "    allocationvars: [0, 1]\n",
        "    from_reference: !<GroupTransform>\n",
        "      children:\n",
    );

    ocio_config_str
}

fn load_config(config_str: &str) -> Config {
    let config = Config::create_from_str(config_str).expect("valid config");
    config.validate().expect("validated config");
    config
}

#[test]
fn several_1d_luts_legacy_shader() {
    gpu_test("Config/several_1D_luts_legacy_shader", |t| {
        let mut config_str = create_config();
        config_str += concat!(
            "        - !<FileTransform> {src: lut1d_1.spi1d, interpolation: linear}\n",
            "        - !<FileTransform> {src: lut1d_2.spi1d, interpolation: linear}\n",
        );

        let config = load_config(&config_str);

        let processor = config.get_processor("raw", "lgh").expect("processor");
        t.set_processor(processor);
        t.set_legacy_shader(true);
        t.set_error_threshold(DEFAULT_ERROR_THRESHOLD);
    });
}

#[test]
fn several_1d_luts_generic_shader() {
    gpu_test("Config/several_1D_luts_generic_shader", |t| {
        let mut config_str = create_config();
        config_str += concat!(
            "        - !<FileTransform> {src: lut1d_1.spi1d, interpolation: linear}\n",
            "        - !<FileTransform> {src: lut1d_2.spi1d, interpolation: linear}\n",
        );

        let config = load_config(&config_str);

        let processor = config.get_processor("raw", "lgh").expect("processor");
        t.set_processor(processor);
        t.set_error_threshold(DEFAULT_ERROR_THRESHOLD);

        // TODO: Would like to be able to remove the setTestNaN(false) and
        // setTestInfinity(false) from all of these tests.
        t.set_test_nan(false);
    });
}

#[test]
fn arbitrary_generic_shader() {
    gpu_test("Config/arbitrary_generic_shader", |t| {
        let mut config_str = create_config();
        config_str += concat!(
            "        - !<FileTransform> {src: lut1d_1.spi1d, interpolation: linear}\n",
            "        - !<FileTransform> {src: lut1d_2.spi1d, interpolation: linear}\n",
            "        - !<LogTransform> {base: 10}\n",
            "        - !<MatrixTransform> {matrix: [0.075573, 0.022197,  0.00223,  0, ",
            "0.005901, 0.096928, -0.002829, 0, ",
            "0.016134, 0.007406,  0.07646,  0, ",
            "0,        0,         0,        1]}\n",
        );

        let config = load_config(&config_str);

        let processor = config.get_processor("raw", "lgh").expect("processor");

        t.set_processor(processor);

        t.shader_desc_setup = Some(Box::new(|shader_desc| {
            shader_desc.set_pixel_name("another_pixel_name");
            shader_desc.set_function_name("another_func_name");
        }));

        // TODO: To be investigated when the new LUT 1D OpData will be in
        t.set_error_threshold(5e-3);

        t.set_test_nan(false);
        t.set_test_infinity(false);
    });
}

#[test]
fn several_luts_generic_shader() {
    gpu_test("Config/several_luts_generic_shader", |t| {
        let mut config_str = create_config();
        config_str += concat!(
            "        - !<FileTransform> {src: lut1d_1.spi1d, interpolation: linear}\n",
            "        - !<FileTransform> {src: lut1d_2.spi1d, interpolation: linear}\n",
            "        - !<FileTransform> {src: lut1d_1.spi1d, interpolation: linear}\n",
            "        - !<FileTransform> {src: lut1d_2.spi1d, interpolation: linear}\n",
            "        - !<FileTransform> {src: lut1d_1.spi1d, interpolation: linear}\n",
            "        - !<FileTransform> {src: lut1d_2.spi1d, interpolation: linear}\n",
            "        - !<FileTransform> {src: lut1d_3.spi1d, interpolation: linear}\n",
        );

        let config = load_config(&config_str);

        let processor = config.get_processor("raw", "lgh").expect("processor");

        t.set_processor(processor);
        t.set_error_threshold(DEFAULT_ERROR_THRESHOLD);

        t.set_test_nan(false);
    });
}

#[test]
fn with_underscores() {
    gpu_test("Config/with_underscores", |t| {
        // The unit tests validates that there will be no double underscores for
        // GPU resource names as it's forbidden by GLSL.

        let mut config_str = create_config();
        config_str += concat!(
            "        - !<LogTransform> {base: 10}\n",
            "\n",
            "  - !<ColorSpace>\n",
            "    name: __lgh__\n",
            "    family: \"\"\n",
            "    equalitygroup: \"\"\n",
            "    bitdepth: unknown\n",
            "    isdata: false\n",
            "    allocation: uniform\n",
            "    allocationvars: [0, 1]\n",
            "    from_reference: !<GroupTransform>\n",
            "      children:\n",
            "        - !<MatrixTransform> {matrix: [0.075573, 0.022197,  0.00223,  0, ",
            "0.005901, 0.096928, -0.002829, 0, ",
            "0.016134, 0.007406,  0.07646,  0, ",
            "0,        0,         0,        1]}\n",
            "        - !<FileTransform> {src: lut1d_3.spi1d, interpolation: linear}\n",
        );

        let config = load_config(&config_str);

        let processor = config.get_processor("raw", "__lgh__").expect("processor");
        t.set_processor(processor);

        t.shader_desc_setup = Some(Box::new(|shader_desc| {
            shader_desc.set_resource_prefix("ocio___");
            shader_desc.set_pixel_name("another_pixel_name__");
            shader_desc.set_function_name("__another_func_name____");
        }));

        t.set_error_threshold(DEFAULT_ERROR_THRESHOLD);

        t.set_test_nan(false);
        t.set_test_infinity(false);
    });
}
