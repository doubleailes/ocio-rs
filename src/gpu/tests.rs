//! Tests of the GPU shader generation (port of `GpuShader_tests.cpp`,
//! `GpuShaderUtils_tests.cpp`, the GPU parts of `Processor_tests.cpp` and the
//! `local_bypass` tests of the grading transforms).

use super::shader_text::get_float_string;
use super::*;
use crate::config::Config;
use crate::transforms::{
    ColorSpaceTransform, DisplayViewTransform, ExposureContrastTransform, FileTransform,
    GradingHueCurveTransform, GradingPrimaryTransform, GradingRgbCurveTransform,
    GradingToneTransform, MatrixTransform, Transform,
};
use crate::types::{
    GpuLanguage, GradingStyle, Interpolation, OptimizationFlags, TransformDirection,
};

const TEST_FILES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/files");

/// Compare shader texts, reporting the first different line.
fn assert_text_eq(expected: &str, actual: &str) {
    if expected != actual {
        for (i, (e, a)) in expected.lines().zip(actual.lines()).enumerate() {
            if e != a {
                panic!(
                    "line {}:\nexpected: {:?}\n  actual: {:?}\nfull text:\n{}",
                    i + 1,
                    e,
                    a,
                    actual
                );
            }
        }
        panic!(
            "texts differ in length ({} vs {} lines):\n{}",
            expected.lines().count(),
            actual.lines().count(),
            actual
        );
    }
}

fn processor(config: &Config, t: Transform) -> crate::Processor {
    config
        .get_processor_for_transform(&t, TransformDirection::Forward)
        .unwrap()
}

#[test]
fn float_to_string() {
    assert_eq!(get_float_string(1.0f32, GpuLanguage::Glsl1_3), "1.");
    assert_eq!(get_float_string(-11.0f32, GpuLanguage::Glsl1_3), "-11.");
    assert_eq!(get_float_string(-1.0f32, GpuLanguage::Glsl1_3), "-1.");
    assert_eq!(get_float_string(-1_f32, GpuLanguage::Glsl1_3), "-1.");
    assert_eq!(get_float_string(1_f32, GpuLanguage::Glsl1_3), "1.");

    // Extra checks of the C++ stream formatting.
    assert_eq!(get_float_string(0.5f32, GpuLanguage::Glsl1_3), "0.5");
    assert_eq!(
        get_float_string(0.1f32, GpuLanguage::Glsl1_3),
        "0.100000001"
    );
    assert_eq!(
        get_float_string(0.1f64, GpuLanguage::Glsl1_3),
        "0.10000000000000001"
    );
    assert_eq!(
        get_float_string(6.103_515_6e-5_f32, GpuLanguage::Glsl1_3),
        "6.10351562e-05"
    );
    assert_eq!(
        get_float_string(f32::MIN_POSITIVE, GpuLanguage::Glsl1_3),
        "1.17549435e-38"
    );
    assert_eq!(get_float_string(1e20f64, GpuLanguage::Glsl1_3), "1e+20.");
    assert_eq!(get_float_string(f32::INFINITY, GpuLanguage::Glsl1_3), "inf");
    assert_eq!(get_float_string(f32::NAN, GpuLanguage::Glsl1_3), "nan");
    // Cg clamps to the normalized half range.
    assert_eq!(get_float_string(1e-6f32, GpuLanguage::Cg), "0.");
    assert_eq!(get_float_string(1e6f64, GpuLanguage::Cg), "65504.");
}

#[test]
fn generic_shader() {
    let mut desc = GpuShaderDesc::new();

    {
        assert_ne!(desc.language(), GpuLanguage::Glsl1_3);
        desc.set_language(GpuLanguage::Glsl1_3);
        assert_eq!(desc.language(), GpuLanguage::Glsl1_3);

        assert_ne!(desc.function_name(), "1sd234_");
        desc.set_function_name("1sd234_");
        assert_eq!(desc.function_name(), "1sd234_");

        assert_ne!(desc.pixel_name(), "pxl_1sd234_");
        desc.set_pixel_name("pxl_1sd234_");
        assert_eq!(desc.pixel_name(), "pxl_1sd234_");

        assert_ne!(desc.resource_prefix(), "res_1sd234_");
        desc.set_resource_prefix("res_1sd234_");
        assert_eq!(desc.resource_prefix(), "res_1sd234_");

        desc.finalize().unwrap();
        let id = desc.cache_id();
        assert_eq!(
            id,
            "glsl_1.3 1sd234_ res_1sd234_ pxl_1sd234_ 0 0 1 6001c324468d497f99aa06d3014798d8"
        );
        desc.set_resource_prefix("res_1");
        desc.finalize().unwrap();
        assert_ne!(desc.cache_id(), id);
    }

    {
        let width = 3u32;
        let height = 2u32;
        let size = (width * height * 3) as usize;

        let values: [f32; 18] = [
            0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8,
            0.9,
        ];

        assert_eq!(desc.num_textures(), 0);
        let binding = desc
            .add_texture(
                "lut1",
                "lut1Sampler",
                width,
                height,
                TextureType::RgbChannel,
                TextureDimensions::Texture2D,
                Interpolation::Tetrahedral,
                &values,
            )
            .unwrap();
        assert_eq!(desc.num_textures(), 1);
        assert_eq!(binding, 1);

        let t = desc.texture(0).unwrap();
        assert_eq!(t.texture_name, "lut1");
        assert_eq!(t.sampler_name, "lut1Sampler");
        assert_eq!(t.width, width);
        assert_eq!(t.height, height);
        assert_eq!(t.channel, TextureType::RgbChannel);
        assert_eq!(t.interpolation, Interpolation::Tetrahedral);
        assert_eq!(
            desc.texture_dimensions(0).unwrap(),
            TextureDimensions::Texture2D
        );

        let err = desc.texture(1).unwrap_err();
        assert!(err.message().contains("1D LUT access error"));

        let vals = desc.texture_values(0).unwrap();
        assert_eq!(vals.len(), size);
        assert_eq!(vals, &values[..]);

        assert!(desc
            .texture_values(1)
            .unwrap_err()
            .message()
            .contains("1D LUT access error"));

        // Support several 1D LUTs.
        let binding = desc
            .add_texture(
                "lut2",
                "lut2Sampler",
                width,
                height,
                TextureType::RgbChannel,
                TextureDimensions::Texture2D,
                Interpolation::Tetrahedral,
                &values,
            )
            .unwrap();
        assert_eq!(desc.num_textures(), 2);
        assert_eq!(binding, 2);

        assert!(desc.texture_values(0).is_ok());
        assert!(desc.texture_values(1).is_ok());
        assert!(desc
            .texture_values(2)
            .unwrap_err()
            .message()
            .contains("1D LUT access error"));
    }

    {
        let edgelen = 2u32;
        let values: [f32; 24] = [
            0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 0.7, 0.8, 0.9, 0.1, 0.2, 0.3, 0.4, 0.5,
            0.6, 0.7, 0.8, 0.9, 0.7, 0.8, 0.9,
        ];

        assert_eq!(desc.num_3d_textures(), 0);
        let binding = desc
            .add_3d_texture(
                "lut1",
                "lut1Sampler",
                edgelen,
                Interpolation::Tetrahedral,
                &values,
            )
            .unwrap();
        assert_eq!(desc.num_3d_textures(), 1);
        assert_eq!(binding, 3);

        let t = desc.texture_3d(0).unwrap();
        assert_eq!(t.texture_name, "lut1");
        assert_eq!(t.sampler_name, "lut1Sampler");
        assert_eq!(t.width, edgelen);
        assert_eq!(t.interpolation, Interpolation::Tetrahedral);

        assert!(desc
            .texture_3d(1)
            .unwrap_err()
            .message()
            .contains("3D LUT access error"));

        assert_eq!(desc.texture_3d_values(0).unwrap(), &values[..]);
        assert!(desc
            .texture_3d_values(1)
            .unwrap_err()
            .message()
            .contains("3D LUT access error"));

        // Supports several 3D LUTs.
        let binding = desc
            .add_3d_texture(
                "lut2",
                "lut2Sampler",
                edgelen,
                Interpolation::Tetrahedral,
                &values,
            )
            .unwrap();
        assert_eq!(desc.num_3d_textures(), 2);
        assert_eq!(binding, 4);

        // Check the 3D LUT limit.
        assert!(desc
            .add_3d_texture(
                "lut1",
                "lut1Sampler",
                130,
                Interpolation::Tetrahedral,
                &values
            )
            .is_err());
    }

    {
        desc.add_to_parameter_declare_shader_code("vec2 coords;\n");
        desc.add_to_helper_shader_code("vec2 helpers() {}\n\n");
        desc.add_to_function_header_shader_code("void func() {\n");
        desc.add_to_function_shader_code("  int i;\n");
        desc.add_to_function_footer_shader_code("}\n");

        desc.finalize().unwrap();

        let frag_text = r#"
// Declaration of all variables

vec2 coords;

// Declaration of all helper methods

vec2 helpers() {}

void func() {
  int i;
}
"#;

        assert_eq!(frag_text, desc.shader_text());
    }

    {
        assert_ne!(desc.language(), GpuLanguage::GlslVk4_6);
        desc.set_language(GpuLanguage::GlslVk4_6);
        assert_eq!(desc.language(), GpuLanguage::GlslVk4_6);

        assert_eq!(
            desc.set_descriptor_set_index(123, 0).unwrap_err().message(),
            "Texture binding start index must be greater than 0."
        );
        desc.set_descriptor_set_index(123, 456).unwrap();
        assert_eq!(desc.descriptor_set_index(), 123);
        assert_eq!(desc.texture_binding_start(), 456);

        // Simulate only 2 elements in the array.
        let max_size = 3u32;
        desc.add_uniform_vector_float("array", Arc::new(|| 2), Arc::new(Vec::new), max_size)
            .unwrap();
        assert_eq!(desc.uniform_buffer_size(), 16 * max_size as usize);

        desc.add_to_texture_declare_shader_code(
            "layout(set=123, binding = 456) uniform sampler2D samplerName; \n",
        );

        desc.finalize().unwrap();

        let frag_text = r#"layout (set = 123, binding = 0) uniform 1sd234__Parameters
{

// Declaration of all variables

vec2 coords;

};

// Declaration of all textures

layout(set=123, binding = 456) uniform sampler2D samplerName; 

// Declaration of all helper methods

vec2 helpers() {}

void func() {
  int i;
}
"#;

        assert_eq!(frag_text, desc.shader_text());
    }
}

#[test]
fn uniforms() {
    let mut desc = GpuShaderDesc::new();
    assert!(desc.add_uniform_double("a", Arc::new(|| 1.5)).unwrap());
    assert!(!desc.add_uniform_double("a", Arc::new(|| 2.5)).unwrap());
    assert!(desc
        .add_uniform_float3("b", Arc::new(|| [1.0, 2.0, 3.0]))
        .unwrap());
    assert!(desc.add_uniform_bool("c", Arc::new(|| true)).unwrap());
    assert!(desc
        .add_uniform_vector_int("d", Arc::new(|| 2), Arc::new(|| vec![1, 2]), 8)
        .unwrap());
    assert!(desc.add_uniform_double("", Arc::new(|| 1.5)).is_err());

    assert_eq!(desc.num_uniforms(), 4);
    let (name, data) = desc.uniform(0).unwrap();
    assert_eq!(name, "a");
    assert_eq!(data.data_type(), UniformDataType::Double);
    assert_eq!(data.buffer_offset, 0);
    match &data.value {
        UniformValue::Double(g) => assert_eq!(g(), 1.5),
        _ => panic!("unexpected uniform type"),
    }
    let (_, data) = desc.uniform(1).unwrap();
    assert_eq!(data.data_type(), UniformDataType::Float3);
    assert_eq!(data.buffer_offset, 16);
    let (_, data) = desc.uniform(2).unwrap();
    assert_eq!(data.data_type(), UniformDataType::Bool);
    assert_eq!(data.buffer_offset, 28);
    let (_, data) = desc.uniform(3).unwrap();
    assert_eq!(data.data_type(), UniformDataType::VectorInt);
    assert_eq!(data.buffer_offset, 32);
    assert_eq!(desc.uniform_buffer_size(), 32 + 16 * 8);

    assert_eq!(
        desc.uniform(4).unwrap_err().message(),
        "Uniforms access error: index = 4 where size = 4"
    );
}

const ACESCG_CONFIG: &str = r#"ocio_profile_version: 2
environment: {}
search_path: "./"
roles:
  data: Raw
  default: Raw
  scene_linear: ACEScg

file_rules:
  - !<Rule> {name: Default, colorspace: default}

displays:
  AdobeRGB:
    - !<View> {name: Raw, colorspace: Raw}

colorspaces:
  - !<ColorSpace>
    name: ACEScg
    to_reference: !<MatrixTransform> {matrix: [ 0.695452241357, 0.140678696470, 0.163869062172, 0, 0.044794563372, 0.859671118456, 0.095534318172, 0, -0.005525882558, 0.004025210306, 1.001500672252, 0, 0, 0, 0, 1 ]}
  - !<ColorSpace>
    name: Raw
    isdata: true"#;

const METAL_EMPTY_DISPLAY: &str = concat!(
    "\n",
    "// Declaration of class wrapper\n",
    "\n",
    "struct ocioDisplay\n",
    "{\n",
    "ocioDisplay(\n",
    ")\n",
    "{\n",
    "}\n",
    "\n",
    "\n",
    "\n",
    "// Declaration of the OCIO shader function\n",
    "\n",
    "float4 Display(float4 inPixel)\n",
    "{\n",
    "  float4 outColor = inPixel;\n",
    "\n",
    "  return outColor;\n",
    "}\n",
    "\n",
    "// Close class wrapper\n",
    "\n",
    "\n",
    "};\n",
    "float4 Display(\n",
    "  float4 inPixel)\n",
    "{\n",
    "  return ocioDisplay(\n",
    "  ).Display(inPixel);\n",
    "}\n",
);

fn display_view_processor() -> crate::Processor {
    let config = Config::create_from_str(ACESCG_CONFIG).unwrap();
    config.validate().unwrap();

    let t = DisplayViewTransform {
        src: "ACEScg".into(),
        display: "AdobeRGB".into(),
        view: "raw".into(),
        ..Default::default()
    };
    processor(&config, Transform::DisplayView(t))
}

#[test]
fn metal_lut_test() {
    let proc = display_view_processor();

    let edgelen = 2;
    let gpu = proc
        .optimized_legacy_gpu_processor(OptimizationFlags::ALL, edgelen)
        .unwrap();

    let mut desc = GpuShaderDesc::new();
    desc.set_function_name("Display");
    desc.set_language(GpuLanguage::Msl2_0);

    gpu.extract_gpu_shader_info(&mut desc).unwrap();
    assert_eq!(METAL_EMPTY_DISPLAY, desc.shader_text());
}

#[test]
fn metal_lut_test2() {
    let proc = display_view_processor();
    let gpu = proc.default_gpu_processor().unwrap();

    let mut desc = GpuShaderDesc::new();
    desc.set_function_name("Display");
    desc.set_language(GpuLanguage::Msl2_0);

    gpu.extract_gpu_shader_info(&mut desc).unwrap();
    assert_eq!(METAL_EMPTY_DISPLAY, desc.shader_text());
}

fn cs1_cs2_config(cs2_transform: &str) -> Config {
    let config = format!(
        "ocio_profile_version: 2\n\
         \n\
         environment: {{ENV1: {TEST_FILES}}}\n\
         \n\
         search_path: $ENV1\n\
         \n\
         roles:\n\
         \x20 default: cs1\n\
         \x20 reference: cs1\n\
         \n\
         displays:\n\
         \x20 disp1:\n\
         \x20   - !<View> {{name: view1, colorspace: cs2}}\n\
         \n\
         colorspaces:\n\
         \x20 - !<ColorSpace>\n\
         \x20   name: cs1\n\
         \n\
         \x20 - !<ColorSpace>\n\
         \x20   name: cs2\n\
         {cs2_transform}"
    );
    let config = Config::create_from_str(&config).unwrap();
    config.validate().unwrap();
    config
}

fn cs1_to_cs2_shader(
    config: &Config,
    lang: GpuLanguage,
    customize: impl FnOnce(&mut GpuShaderDesc),
) -> String {
    let t = ColorSpaceTransform {
        src: "cs1".into(),
        dst: "cs2".into(),
        ..Default::default()
    };
    let proc = processor(config, Transform::ColorSpace(t));
    let gpu = proc
        .optimized_gpu_processor(OptimizationFlags::NONE)
        .unwrap();

    let mut desc = GpuShaderDesc::new();
    desc.set_language(lang);
    customize(&mut desc);

    gpu.extract_gpu_shader_info(&mut desc).unwrap();
    desc.shader_text().to_string()
}

#[test]
fn metal_support3() {
    // The unit test validates a single 1D LUT.
    let config =
        cs1_cs2_config("    from_scene_reference: !<FileTransform> {src: lut1d_green.ctf}\n");
    let text = cs1_to_cs2_shader(&config, GpuLanguage::Msl2_0, |d| {
        d.set_function_name("MyMethodName");
        d.set_pixel_name("myPixelName");
    });

    let expected = concat!(
        "\n",
        "// Declaration of class wrapper\n",
        "\n",
        "struct ocioMyMethodName\n",
        "{\n",
        "ocioMyMethodName(\n",
        "  texture1d<float> ocio_lut1d_0\n",
        "  , sampler ocio_lut1d_0Sampler\n",
        ")\n",
        "{\n",
        "  this->ocio_lut1d_0 = ocio_lut1d_0;\n",
        "  this->ocio_lut1d_0Sampler = ocio_lut1d_0Sampler;\n",
        "}\n",
        "\n",
        "\n",
        "\n",
        "// Declaration of all textures\n",
        "\n",
        "texture1d<float> ocio_lut1d_0;\n",
        "sampler ocio_lut1d_0Sampler;\n",
        "\n",
        "// Declaration of the OCIO shader function\n",
        "\n",
        "float4 MyMethodName(float4 inPixel)\n",
        "{\n",
        "  float4 myPixelName = inPixel;\n",
        "  \n",
        "  // Add LUT 1D processing for ocio_lut1d_0\n",
        "  \n",
        "  {\n",
        "    float3 ocio_lut1d_0_coords = (myPixelName.rgb * float3(31., 31., 31.) + float3(0.5, 0.5, 0.5) ) / float3(32., 32., 32.);\n",
        "    myPixelName.r = ocio_lut1d_0.sample(ocio_lut1d_0Sampler, ocio_lut1d_0_coords.r).r;\n",
        "    myPixelName.g = ocio_lut1d_0.sample(ocio_lut1d_0Sampler, ocio_lut1d_0_coords.g).g;\n",
        "    myPixelName.b = ocio_lut1d_0.sample(ocio_lut1d_0Sampler, ocio_lut1d_0_coords.b).b;\n",
        "  }\n",
        "\n",
        "  return myPixelName;\n",
        "}\n",
        "\n",
        "// Close class wrapper\n",
        "\n",
        "\n",
        "};\n",
        "float4 MyMethodName(\n",
        "  texture1d<float> ocio_lut1d_0\n",
        "  , sampler ocio_lut1d_0Sampler\n",
        "  , float4 inPixel)\n",
        "{\n",
        "  return ocioMyMethodName(\n",
        "    ocio_lut1d_0\n",
        "    , ocio_lut1d_0Sampler\n",
        "  ).MyMethodName(inPixel);\n",
        "}\n",
    );
    assert_text_eq(expected, &text);
}

#[test]
fn metal_support4() {
    // The unit test validates a single 3D LUT.
    let config =
        cs1_cs2_config("    from_scene_reference: !<FileTransform> {src: lut3d_example_Inv.ctf}\n");
    let text = cs1_to_cs2_shader(&config, GpuLanguage::Msl2_0, |d| {
        d.set_function_name("MyMethodName");
        d.set_pixel_name("myPixelName");
    });

    let expected = concat!(
        "\n",
        "// Declaration of class wrapper\n",
        "\n",
        "struct ocioMyMethodName\n",
        "{\n",
        "ocioMyMethodName(\n",
        "  texture3d<float> ocio_lut3d_0\n",
        "  , sampler ocio_lut3d_0Sampler\n",
        ")\n",
        "{\n",
        "  this->ocio_lut3d_0 = ocio_lut3d_0;\n",
        "  this->ocio_lut3d_0Sampler = ocio_lut3d_0Sampler;\n",
        "}\n",
        "\n",
        "\n",
        "\n",
        "// Declaration of all textures\n",
        "\n",
        "texture3d<float> ocio_lut3d_0;\n",
        "sampler ocio_lut3d_0Sampler;\n",
        "\n",
        "// Declaration of the OCIO shader function\n",
        "\n",
        "float4 MyMethodName(float4 inPixel)\n",
        "{\n",
        "  float4 myPixelName = inPixel;\n",
        "  \n",
        "  // Add LUT 3D processing for ocio_lut3d_0\n",
        "  \n",
        "  float3 ocio_lut3d_0_coords = (myPixelName.zyx * float3(47., 47., 47.) + float3(0.5, 0.5, 0.5)) / float3(48., 48., 48.);\n",
        "  myPixelName.rgb = ocio_lut3d_0.sample(ocio_lut3d_0Sampler, ocio_lut3d_0_coords).rgb;\n",
        "\n",
        "  return myPixelName;\n",
        "}\n",
        "\n",
        "// Close class wrapper\n",
        "\n",
        "\n",
        "};\n",
        "float4 MyMethodName(\n",
        "  texture3d<float> ocio_lut3d_0\n",
        "  , sampler ocio_lut3d_0Sampler\n",
        "  , float4 inPixel)\n",
        "{\n",
        "  return ocioMyMethodName(\n",
        "    ocio_lut3d_0\n",
        "    , ocio_lut3d_0Sampler\n",
        "  ).MyMethodName(inPixel);\n",
        "}\n",
    );
    assert_text_eq(expected, &text);
}

#[test]
fn metal_support5() {
    // The unit test validates a single 1D LUT needing an helper method.
    let config =
        cs1_cs2_config("    from_scene_reference: !<FileTransform> {src: clf/lut1d_long.clf}\n");
    let text = cs1_to_cs2_shader(&config, GpuLanguage::Msl2_0, |_| {});

    let expected = concat!(
        "\n",
        "// Declaration of class wrapper\n",
        "\n",
        "struct ocioOCIOMain\n",
        "{\n",
        "ocioOCIOMain(\n",
        "  texture2d<float> ocio_lut1d_0\n",
        "  , sampler ocio_lut1d_0Sampler\n",
        ")\n",
        "{\n",
        "  this->ocio_lut1d_0 = ocio_lut1d_0;\n",
        "  this->ocio_lut1d_0Sampler = ocio_lut1d_0Sampler;\n",
        "}\n",
        "\n",
        "\n",
        "\n",
        "// Declaration of all textures\n",
        "\n",
        "texture2d<float> ocio_lut1d_0;\n",
        "sampler ocio_lut1d_0Sampler;\n",
        "\n",
        "// Declaration of all helper methods\n",
        "\n",
        "float2 ocio_lut1d_0_computePos(float f)\n",
        "{\n",
        "  float dep = clamp(f, 0.0, 1.0) * 131071.;\n",
        "  float2 retVal;\n",
        "  retVal.y = floor(dep / 4095.);\n",
        "  retVal.x = dep - retVal.y * 4095.;\n",
        "  retVal.x = (retVal.x + 0.5) / 4096.;\n",
        "  retVal.y = (retVal.y + 0.5) / 33.;\n",
        "  return retVal;\n",
        "}\n",
        "\n",
        "// Declaration of the OCIO shader function\n",
        "\n",
        "float4 OCIOMain(float4 inPixel)\n",
        "{\n",
        "  float4 outColor = inPixel;\n",
        "  \n",
        "  // Add LUT 1D processing for ocio_lut1d_0\n",
        "  \n",
        "  {\n",
        "    outColor.r = ocio_lut1d_0.sample(ocio_lut1d_0Sampler, ocio_lut1d_0_computePos(outColor.r)).r;\n",
        "    outColor.g = ocio_lut1d_0.sample(ocio_lut1d_0Sampler, ocio_lut1d_0_computePos(outColor.g)).r;\n",
        "    outColor.b = ocio_lut1d_0.sample(ocio_lut1d_0Sampler, ocio_lut1d_0_computePos(outColor.b)).r;\n",
        "  }\n",
        "\n",
        "  return outColor;\n",
        "}\n",
        "\n",
        "// Close class wrapper\n",
        "\n",
        "\n",
        "};\n",
        "float4 OCIOMain(\n",
        "  texture2d<float> ocio_lut1d_0\n",
        "  , sampler ocio_lut1d_0Sampler\n",
        "  , float4 inPixel)\n",
        "{\n",
        "  return ocioOCIOMain(\n",
        "    ocio_lut1d_0\n",
        "    , ocio_lut1d_0Sampler\n",
        "  ).OCIOMain(inPixel);\n",
        "}\n",
    );
    assert_text_eq(expected, &text);
}

#[test]
fn metal_support6() {
    // The unit test validates several arbitrary luts.
    let config = cs1_cs2_config(
        "    from_scene_reference: !<GroupTransform>\n\
         \x20     children:\n\
         \x20       - !<FileTransform> {src: lut1d_green.ctf}\n\
         \x20       - !<FileTransform> {src: lut3d_example_Inv.ctf}\n\
         \x20       - !<FileTransform> {src: clf/lut1d_long.clf}\n",
    );
    let text = cs1_to_cs2_shader(&config, GpuLanguage::Msl2_0, |_| {});

    let expected = concat!(
        "\n",
        "// Declaration of class wrapper\n",
        "\n",
        "struct ocioOCIOMain\n",
        "{\n",
        "ocioOCIOMain(\n",
        "  texture3d<float> ocio_lut3d_1\n",
        "  , sampler ocio_lut3d_1Sampler\n",
        "  , texture1d<float> ocio_lut1d_0\n",
        "  , sampler ocio_lut1d_0Sampler\n",
        "  , texture2d<float> ocio_lut1d_2\n",
        "  , sampler ocio_lut1d_2Sampler\n",
        ")\n",
        "{\n",
        "  this->ocio_lut3d_1 = ocio_lut3d_1;\n",
        "  this->ocio_lut3d_1Sampler = ocio_lut3d_1Sampler;\n",
        "  this->ocio_lut1d_0 = ocio_lut1d_0;\n",
        "  this->ocio_lut1d_0Sampler = ocio_lut1d_0Sampler;\n",
        "  this->ocio_lut1d_2 = ocio_lut1d_2;\n",
        "  this->ocio_lut1d_2Sampler = ocio_lut1d_2Sampler;\n",
        "}\n",
        "\n",
        "\n",
        "\n",
        "// Declaration of all textures\n",
        "\n",
        "texture1d<float> ocio_lut1d_0;\n",
        "sampler ocio_lut1d_0Sampler;\n",
        "texture3d<float> ocio_lut3d_1;\n",
        "sampler ocio_lut3d_1Sampler;\n",
        "texture2d<float> ocio_lut1d_2;\n",
        "sampler ocio_lut1d_2Sampler;\n",
        "\n",
        "// Declaration of all helper methods\n",
        "\n",
        "float2 ocio_lut1d_2_computePos(float f)\n",
        "{\n",
        "  float dep = clamp(f, 0.0, 1.0) * 131071.;\n",
        "  float2 retVal;\n",
        "  retVal.y = floor(dep / 4095.);\n",
        "  retVal.x = dep - retVal.y * 4095.;\n",
        "  retVal.x = (retVal.x + 0.5) / 4096.;\n",
        "  retVal.y = (retVal.y + 0.5) / 33.;\n",
        "  return retVal;\n",
        "}\n",
        "\n",
        "// Declaration of the OCIO shader function\n",
        "\n",
        "float4 OCIOMain(float4 inPixel)\n",
        "{\n",
        "  float4 outColor = inPixel;\n",
        "  \n",
        "  // Add LUT 1D processing for ocio_lut1d_0\n",
        "  \n",
        "  {\n",
        "    float3 ocio_lut1d_0_coords = (outColor.rgb * float3(31., 31., 31.) + float3(0.5, 0.5, 0.5) ) / float3(32., 32., 32.);\n",
        "    outColor.r = ocio_lut1d_0.sample(ocio_lut1d_0Sampler, ocio_lut1d_0_coords.r).r;\n",
        "    outColor.g = ocio_lut1d_0.sample(ocio_lut1d_0Sampler, ocio_lut1d_0_coords.g).g;\n",
        "    outColor.b = ocio_lut1d_0.sample(ocio_lut1d_0Sampler, ocio_lut1d_0_coords.b).b;\n",
        "  }\n",
        "  \n",
        "  // Add LUT 3D processing for ocio_lut3d_1\n",
        "  \n",
        "  float3 ocio_lut3d_1_coords = (outColor.zyx * float3(47., 47., 47.) + float3(0.5, 0.5, 0.5)) / float3(48., 48., 48.);\n",
        "  outColor.rgb = ocio_lut3d_1.sample(ocio_lut3d_1Sampler, ocio_lut3d_1_coords).rgb;\n",
        "  \n",
        "  // Add LUT 1D processing for ocio_lut1d_2\n",
        "  \n",
        "  {\n",
        "    outColor.r = ocio_lut1d_2.sample(ocio_lut1d_2Sampler, ocio_lut1d_2_computePos(outColor.r)).r;\n",
        "    outColor.g = ocio_lut1d_2.sample(ocio_lut1d_2Sampler, ocio_lut1d_2_computePos(outColor.g)).r;\n",
        "    outColor.b = ocio_lut1d_2.sample(ocio_lut1d_2Sampler, ocio_lut1d_2_computePos(outColor.b)).r;\n",
        "  }\n",
        "\n",
        "  return outColor;\n",
        "}\n",
        "\n",
        "// Close class wrapper\n",
        "\n",
        "\n",
        "};\n",
        "float4 OCIOMain(\n",
        "  texture3d<float> ocio_lut3d_1\n",
        "  , sampler ocio_lut3d_1Sampler\n",
        "  , texture1d<float> ocio_lut1d_0\n",
        "  , sampler ocio_lut1d_0Sampler\n",
        "  , texture2d<float> ocio_lut1d_2\n",
        "  , sampler ocio_lut1d_2Sampler\n",
        "  , float4 inPixel)\n",
        "{\n",
        "  return ocioOCIOMain(\n",
        "    ocio_lut3d_1\n",
        "    , ocio_lut3d_1Sampler\n",
        "    , ocio_lut1d_0\n",
        "    , ocio_lut1d_0Sampler\n",
        "    , ocio_lut1d_2\n",
        "    , ocio_lut1d_2Sampler\n",
        "  ).OCIOMain(inPixel);\n",
        "}\n",
    );
    assert_text_eq(expected, &text);
}

#[test]
fn metal_support7() {
    // The unit test validates a single E/C which needs a uniform.
    let config = cs1_cs2_config(
        "    from_scene_reference:\n\
         \x20      !<ExposureContrastTransform> {style: video, contrast: 0.5}\n",
    );
    let text = cs1_to_cs2_shader(&config, GpuLanguage::Msl2_0, |_| {});

    let expected = concat!(
        "\n",
        "// Declaration of class wrapper\n",
        "\n",
        "struct ocioOCIOMain\n",
        "{\n",
        "ocioOCIOMain(\n",
        "  float ocio_exposure_contrast_exposureVal\n",
        "  , float ocio_exposure_contrast_gammaVal\n",
        ")\n",
        "{\n",
        "  this->ocio_exposure_contrast_exposureVal = ocio_exposure_contrast_exposureVal;\n",
        "  this->ocio_exposure_contrast_gammaVal = ocio_exposure_contrast_gammaVal;\n",
        "}\n",
        "\n",
        "\n",
        "// Declaration of all variables\n",
        "\n",
        "float ocio_exposure_contrast_exposureVal;\n",
        "float ocio_exposure_contrast_gammaVal;\n",
        "\n",
        "\n",
        "// Declaration of the OCIO shader function\n",
        "\n",
        "float4 OCIOMain(float4 inPixel)\n",
        "{\n",
        "  float4 outColor = inPixel;\n",
        "  \n",
        "  // Add ExposureContrast 'video' processing\n",
        "  \n",
        "  {\n",
        "    float contrastVal = 0.5;\n",
        "    float exposure = pow( pow( 2., ocio_exposure_contrast_exposureVal ), 0.54644808743169393);\n",
        "    float contrast = max( 0.001, ( contrastVal * ocio_exposure_contrast_gammaVal ) );\n",
        "    outColor.rgb = outColor.rgb * exposure;\n",
        "    if (contrast != 1.0)\n",
        "    {\n",
        "      outColor.rgb = pow( max( float3(0., 0., 0.), outColor.rgb / float3(0.39178254652545397, 0.39178254652545397, 0.39178254652545397) ), float3(contrast, contrast, contrast) ) * float3(0.39178254652545397, 0.39178254652545397, 0.39178254652545397);\n",
        "    }\n",
        "  }\n",
        "\n",
        "  return outColor;\n",
        "}\n",
        "\n",
        "// Close class wrapper\n",
        "\n",
        "\n",
        "};\n",
        "float4 OCIOMain(\n",
        "  float ocio_exposure_contrast_exposureVal\n",
        "  , float ocio_exposure_contrast_gammaVal\n",
        "  , float4 inPixel)\n",
        "{\n",
        "  return ocioOCIOMain(\n",
        "    ocio_exposure_contrast_exposureVal\n",
        "    , ocio_exposure_contrast_gammaVal\n",
        "  ).OCIOMain(inPixel);\n",
        "}\n",
    );
    assert_text_eq(expected, &text);
}

#[test]
fn metal_support8() {
    // The unit test validates a single Grading transform.
    let config = cs1_cs2_config(
        "    from_scene_reference: \n\
         \x20      !<GradingRGBCurveTransform>\n\
         \x20         style: log\n\
         \x20         red: {control_points: [0, 0, 0.5, 0.5, 1, 1.123456]}\n",
    );
    let text = cs1_to_cs2_shader(&config, GpuLanguage::Msl2_0, |_| {});

    let expected = concat!(
        "\n",
        "// Declaration of class wrapper\n",
        "\n",
        "struct ocioOCIOMain\n",
        "{\n",
        "ocioOCIOMain(\n",
        ")\n",
        "{\n",
        "}\n",
        "\n",
        "\n",
        "\n",
        "// Declaration of all helper methods\n",
        "\n",
        "\n",
        "constant constexpr static int ocio_grading_rgbcurve_knotsOffsets_0[8] = {0, 5, -1, 0, -1, 0, -1, 0};\n",
        "constant constexpr static float ocio_grading_rgbcurve_knots_0[5] = {0., 0.333333343, 0.5, 0.666666508, 1.};\n",
        "constant constexpr static int ocio_grading_rgbcurve_coefsOffsets_0[8] = {0, 12, -1, 0, -1, 0, -1, 0};\n",
        "constant constexpr static float ocio_grading_rgbcurve_coefs_0[12] = {0.0982520878, 0.393008381, 0.347727984, 0.08693178, 0.934498608, 1., 1.13100278, 1.246912, 0., 0.322416425, 0.5, 0.698159397};\n",
        "\n",
        "float ocio_grading_rgbcurve_evalBSplineCurve_0(int curveIdx, float x, float identity_x)\n",
        "{\n",
        "  int knotsOffs = ocio_grading_rgbcurve_knotsOffsets_0[curveIdx * 2];\n",
        "  int knotsCnt = ocio_grading_rgbcurve_knotsOffsets_0[curveIdx * 2 + 1];\n",
        "  int coefsOffs = ocio_grading_rgbcurve_coefsOffsets_0[curveIdx * 2];\n",
        "  int coefsCnt = ocio_grading_rgbcurve_coefsOffsets_0[curveIdx * 2 + 1];\n",
        "  int coefsSets = coefsCnt / 3;\n",
        "  if (coefsSets == 0)\n",
        "  {\n",
        "    return identity_x;\n",
        "  }\n",
        "  float knStart = ocio_grading_rgbcurve_knots_0[knotsOffs];\n",
        "  float knEnd = ocio_grading_rgbcurve_knots_0[knotsOffs + knotsCnt - 1];\n",
        "  if (x <= knStart)\n",
        "  {\n",
        "    float B = ocio_grading_rgbcurve_coefs_0[coefsOffs + coefsSets];\n",
        "    float C = ocio_grading_rgbcurve_coefs_0[coefsOffs + coefsSets * 2];\n",
        "    return (x - knStart) * B + C;\n",
        "  }\n",
        "  else if (x >= knEnd)\n",
        "  {\n",
        "    float A = ocio_grading_rgbcurve_coefs_0[coefsOffs + coefsSets - 1];\n",
        "    float B = ocio_grading_rgbcurve_coefs_0[coefsOffs + coefsSets * 2 - 1];\n",
        "    float C = ocio_grading_rgbcurve_coefs_0[coefsOffs + coefsSets * 3 - 1];\n",
        "    float kn = ocio_grading_rgbcurve_knots_0[knotsOffs + knotsCnt - 2];\n",
        "    float t = knEnd - kn;\n",
        "    float slope = 2. * A * t + B;\n",
        "    float offs = ( A * t + B ) * t + C;\n",
        "    return (x - knEnd) * slope + offs;\n",
        "  }\n",
        "  int i = 0;\n",
        "  for (i = 0; i < knotsCnt - 2; ++i)\n",
        "  {\n",
        "    if (x < ocio_grading_rgbcurve_knots_0[knotsOffs + i + 1])\n",
        "    {\n",
        "      break;\n",
        "    }\n",
        "  }\n",
        "  float A = ocio_grading_rgbcurve_coefs_0[coefsOffs + i];\n",
        "  float B = ocio_grading_rgbcurve_coefs_0[coefsOffs + coefsSets + i];\n",
        "  float C = ocio_grading_rgbcurve_coefs_0[coefsOffs + coefsSets * 2 + i];\n",
        "  float kn = ocio_grading_rgbcurve_knots_0[knotsOffs + i];\n",
        "  float t = x - kn;\n",
        "  return ( A * t + B ) * t + C;\n",
        "}\n",
        "\n",
        "// Declaration of the OCIO shader function\n",
        "\n",
        "float4 OCIOMain(float4 inPixel)\n",
        "{\n",
        "  float4 outColor = inPixel;\n",
        "  \n",
        "  // Add GradingRGBCurve 'log' forward processing\n",
        "  \n",
        "  {\n",
        "    outColor.rgb.r = ocio_grading_rgbcurve_evalBSplineCurve_0(0, outColor.rgb.r, outColor.rgb.r);\n",
        "    outColor.rgb.g = ocio_grading_rgbcurve_evalBSplineCurve_0(1, outColor.rgb.g, outColor.rgb.g);\n",
        "    outColor.rgb.b = ocio_grading_rgbcurve_evalBSplineCurve_0(2, outColor.rgb.b, outColor.rgb.b);\n",
        "    outColor.rgb.r = ocio_grading_rgbcurve_evalBSplineCurve_0(3, outColor.rgb.r, outColor.rgb.r);\n",
        "    outColor.rgb.g = ocio_grading_rgbcurve_evalBSplineCurve_0(3, outColor.rgb.g, outColor.rgb.g);\n",
        "    outColor.rgb.b = ocio_grading_rgbcurve_evalBSplineCurve_0(3, outColor.rgb.b, outColor.rgb.b);\n",
        "  }\n",
        "\n",
        "  return outColor;\n",
        "}\n",
        "\n",
        "// Close class wrapper\n",
        "\n",
        "\n",
        "};\n",
        "float4 OCIOMain(\n",
        "  float4 inPixel)\n",
        "{\n",
        "  return ocioOCIOMain(\n",
        "  ).OCIOMain(inPixel);\n",
        "}\n",
    );
    assert_text_eq(expected, &text);
}

#[test]
fn metal_support9() {
    // The unit test validates a single dynamic Grading transform.
    let config = Config::create();
    let mut t = GradingRgbCurveTransform::new(GradingStyle::Log);
    t.dynamic = true;

    let proc = processor(&config, Transform::GradingRgbCurve(t));
    let gpu = proc
        .optimized_gpu_processor(OptimizationFlags::NONE)
        .unwrap();

    let mut desc = GpuShaderDesc::new();
    desc.set_language(GpuLanguage::Msl2_0);
    gpu.extract_gpu_shader_info(&mut desc).unwrap();

    let expected = concat!(
        "\n",
        "// Declaration of class wrapper\n",
        "\n",
        "struct ocioOCIOMain\n",
        "{\n",
        "ocioOCIOMain(\n",
        "  constant int ocio_grading_rgbcurve_knotsOffsets[8]\n",
        "  , int ocio_grading_rgbcurve_knotsOffsets_count\n",
        "  , constant float ocio_grading_rgbcurve_knots[120]\n",
        "  , int ocio_grading_rgbcurve_knots_count\n",
        "  , constant int ocio_grading_rgbcurve_coefsOffsets[8]\n",
        "  , int ocio_grading_rgbcurve_coefsOffsets_count\n",
        "  , constant float ocio_grading_rgbcurve_coefs[360]\n",
        "  , int ocio_grading_rgbcurve_coefs_count\n",
        "  , bool ocio_grading_rgbcurve_localBypass\n",
        ")\n",
        "{\n",
        "  this->ocio_grading_rgbcurve_knotsOffsets = ocio_grading_rgbcurve_knotsOffsets;\n",
        "  this->ocio_grading_rgbcurve_knots = ocio_grading_rgbcurve_knots;\n",
        "  this->ocio_grading_rgbcurve_coefsOffsets = ocio_grading_rgbcurve_coefsOffsets;\n",
        "  this->ocio_grading_rgbcurve_coefs = ocio_grading_rgbcurve_coefs;\n",
        "  this->ocio_grading_rgbcurve_localBypass = ocio_grading_rgbcurve_localBypass;\n",
        "}\n",
        "\n",
        "\n",
        "// Declaration of all variables\n",
        "\n",
        "constant int* ocio_grading_rgbcurve_knotsOffsets;\n",
        "constant float* ocio_grading_rgbcurve_knots;\n",
        "constant int* ocio_grading_rgbcurve_coefsOffsets;\n",
        "constant float* ocio_grading_rgbcurve_coefs;\n",
        "bool ocio_grading_rgbcurve_localBypass;\n",
        "\n",
        "\n",
        "// Declaration of all helper methods\n",
        "\n",
        "\n",
        "float ocio_grading_rgbcurve_evalBSplineCurve(int curveIdx, float x, float identity_x)\n",
        "{\n",
        "  int knotsOffs = ocio_grading_rgbcurve_knotsOffsets[curveIdx * 2];\n",
        "  int knotsCnt = ocio_grading_rgbcurve_knotsOffsets[curveIdx * 2 + 1];\n",
        "  int coefsOffs = ocio_grading_rgbcurve_coefsOffsets[curveIdx * 2];\n",
        "  int coefsCnt = ocio_grading_rgbcurve_coefsOffsets[curveIdx * 2 + 1];\n",
        "  int coefsSets = coefsCnt / 3;\n",
        "  if (coefsSets == 0)\n",
        "  {\n",
        "    return identity_x;\n",
        "  }\n",
        "  float knStart = ocio_grading_rgbcurve_knots[knotsOffs];\n",
        "  float knEnd = ocio_grading_rgbcurve_knots[knotsOffs + knotsCnt - 1];\n",
        "  if (x <= knStart)\n",
        "  {\n",
        "    float B = ocio_grading_rgbcurve_coefs[coefsOffs + coefsSets];\n",
        "    float C = ocio_grading_rgbcurve_coefs[coefsOffs + coefsSets * 2];\n",
        "    return (x - knStart) * B + C;\n",
        "  }\n",
        "  else if (x >= knEnd)\n",
        "  {\n",
        "    float A = ocio_grading_rgbcurve_coefs[coefsOffs + coefsSets - 1];\n",
        "    float B = ocio_grading_rgbcurve_coefs[coefsOffs + coefsSets * 2 - 1];\n",
        "    float C = ocio_grading_rgbcurve_coefs[coefsOffs + coefsSets * 3 - 1];\n",
        "    float kn = ocio_grading_rgbcurve_knots[knotsOffs + knotsCnt - 2];\n",
        "    float t = knEnd - kn;\n",
        "    float slope = 2. * A * t + B;\n",
        "    float offs = ( A * t + B ) * t + C;\n",
        "    return (x - knEnd) * slope + offs;\n",
        "  }\n",
        "  int i = 0;\n",
        "  for (i = 0; i < knotsCnt - 2; ++i)\n",
        "  {\n",
        "    if (x < ocio_grading_rgbcurve_knots[knotsOffs + i + 1])\n",
        "    {\n",
        "      break;\n",
        "    }\n",
        "  }\n",
        "  float A = ocio_grading_rgbcurve_coefs[coefsOffs + i];\n",
        "  float B = ocio_grading_rgbcurve_coefs[coefsOffs + coefsSets + i];\n",
        "  float C = ocio_grading_rgbcurve_coefs[coefsOffs + coefsSets * 2 + i];\n",
        "  float kn = ocio_grading_rgbcurve_knots[knotsOffs + i];\n",
        "  float t = x - kn;\n",
        "  return ( A * t + B ) * t + C;\n",
        "}\n",
        "\n",
        "// Declaration of the OCIO shader function\n",
        "\n",
        "float4 OCIOMain(float4 inPixel)\n",
        "{\n",
        "  float4 outColor = inPixel;\n",
        "  \n",
        "  // Add GradingRGBCurve 'log' forward processing\n",
        "  \n",
        "  {\n",
        "    if (!ocio_grading_rgbcurve_localBypass)\n",
        "    {\n",
        "      outColor.rgb.r = ocio_grading_rgbcurve_evalBSplineCurve(0, outColor.rgb.r, outColor.rgb.r);\n",
        "      outColor.rgb.g = ocio_grading_rgbcurve_evalBSplineCurve(1, outColor.rgb.g, outColor.rgb.g);\n",
        "      outColor.rgb.b = ocio_grading_rgbcurve_evalBSplineCurve(2, outColor.rgb.b, outColor.rgb.b);\n",
        "      outColor.rgb.r = ocio_grading_rgbcurve_evalBSplineCurve(3, outColor.rgb.r, outColor.rgb.r);\n",
        "      outColor.rgb.g = ocio_grading_rgbcurve_evalBSplineCurve(3, outColor.rgb.g, outColor.rgb.g);\n",
        "      outColor.rgb.b = ocio_grading_rgbcurve_evalBSplineCurve(3, outColor.rgb.b, outColor.rgb.b);\n",
        "    }\n",
        "  }\n",
        "\n",
        "  return outColor;\n",
        "}\n",
        "\n",
        "// Close class wrapper\n",
        "\n",
        "\n",
        "};\n",
        "float4 OCIOMain(\n",
        "  constant int ocio_grading_rgbcurve_knotsOffsets[8]\n",
        "  , int ocio_grading_rgbcurve_knotsOffsets_count\n",
        "  , constant float ocio_grading_rgbcurve_knots[120]\n",
        "  , int ocio_grading_rgbcurve_knots_count\n",
        "  , constant int ocio_grading_rgbcurve_coefsOffsets[8]\n",
        "  , int ocio_grading_rgbcurve_coefsOffsets_count\n",
        "  , constant float ocio_grading_rgbcurve_coefs[360]\n",
        "  , int ocio_grading_rgbcurve_coefs_count\n",
        "  , bool ocio_grading_rgbcurve_localBypass\n",
        "  , float4 inPixel)\n",
        "{\n",
        "  return ocioOCIOMain(\n",
        "    ocio_grading_rgbcurve_knotsOffsets\n",
        "    , ocio_grading_rgbcurve_knotsOffsets_count\n",
        "    , ocio_grading_rgbcurve_knots\n",
        "    , ocio_grading_rgbcurve_knots_count\n",
        "    , ocio_grading_rgbcurve_coefsOffsets\n",
        "    , ocio_grading_rgbcurve_coefsOffsets_count\n",
        "    , ocio_grading_rgbcurve_coefs\n",
        "    , ocio_grading_rgbcurve_coefs_count\n",
        "    , ocio_grading_rgbcurve_localBypass\n",
        "  ).OCIOMain(inPixel);\n",
        "}\n",
    );
    assert_text_eq(expected, desc.shader_text());

    // The dynamic property of the shader drives the uniforms.
    assert!(desc.has_dynamic_property(crate::types::DynamicPropertyType::GradingRgbCurve));
    assert_eq!(desc.num_uniforms(), 5);
    let (name, data) = desc.uniform(4).unwrap();
    assert_eq!(name, "ocio_grading_rgbcurve_localBypass");
    match &data.value {
        UniformValue::Bool(g) => assert!(g()),
        _ => panic!("unexpected uniform type"),
    }
    let prop = desc
        .dynamic_property(crate::types::DynamicPropertyType::GradingRgbCurve)
        .unwrap();
    let handle = prop.as_grading_rgb_curve().unwrap().clone();
    let mut value = handle.get();
    *value.curve_mut(crate::types::RgbCurveType::Red) =
        crate::transforms::grading::GradingBSplineCurve::new(
            &[(0.0, 0.0), (0.5, 0.6), (1.0, 1.0)],
            crate::types::BSplineType::BSpline,
        );
    handle.set(value);
    match &data.value {
        UniformValue::Bool(g) => assert!(!g()),
        _ => panic!("unexpected uniform type"),
    }
    let (_, data) = desc.uniform(1).unwrap();
    match &data.value {
        UniformValue::VectorFloat(size, values) => {
            assert!(size() > 0);
            assert_eq!(values().len(), size() as usize);
        }
        _ => panic!("unexpected uniform type"),
    }
}

#[test]
fn vulkan_support() {
    let config = Config::create();
    let ft = FileTransform {
        src: format!("{TEST_FILES}/clf/lut1d_lut3d_lut1d.clf"),
        ..Default::default()
    };

    let proc = processor(&config, Transform::File(ft));
    let gpu = proc
        .optimized_gpu_processor(OptimizationFlags::NONE)
        .unwrap();

    let mut desc = GpuShaderDesc::new();
    desc.set_language(GpuLanguage::GlslVk4_6);
    desc.set_descriptor_set_index(2, 10).unwrap();

    gpu.extract_gpu_shader_info(&mut desc).unwrap();

    assert_eq!(desc.descriptor_set_index(), 2);
    assert_eq!(desc.num_textures(), 2);
    assert_eq!(desc.num_3d_textures(), 1);

    // Check Lut1D textures.
    {
        let t = desc.texture(0).unwrap();
        assert_eq!(t.texture_name, "ocio_lut1d_0");
        assert_eq!(t.sampler_name, "ocio_lut1d_0Sampler");
        assert_eq!(t.width, 65);
        assert_eq!(t.height, 1);
        assert_eq!(t.channel, TextureType::RedChannel);
        assert_eq!(
            desc.texture_dimensions(0).unwrap(),
            TextureDimensions::Texture1D
        );
        assert_eq!(t.interpolation, Interpolation::Linear);
        assert_eq!(desc.texture_shader_binding_index(0).unwrap(), 10);
    }
    {
        let t = desc.texture(1).unwrap();
        assert_eq!(t.texture_name, "ocio_lut1d_2");
        assert_eq!(t.sampler_name, "ocio_lut1d_2Sampler");
        assert_eq!(t.width, 4096);
        assert_eq!(t.height, 17);
        assert_eq!(t.channel, TextureType::RedChannel);
        assert_eq!(
            desc.texture_dimensions(1).unwrap(),
            TextureDimensions::Texture2D
        );
        assert_eq!(t.interpolation, Interpolation::Linear);
        assert_eq!(desc.texture_shader_binding_index(1).unwrap(), 12);
    }
    // Check Lut3D textures.
    {
        let t = desc.texture_3d(0).unwrap();
        assert_eq!(t.texture_name, "ocio_lut3d_1");
        assert_eq!(t.sampler_name, "ocio_lut3d_1Sampler");
        assert_eq!(t.width, 3);
        assert_eq!(t.interpolation, Interpolation::Linear);
        assert_eq!(desc.texture_3d_shader_binding_index(0).unwrap(), 11);
    }

    // Note that the binding index increments properly for the three LUTs.
    let expected = concat!(
        "\n",
        "// Declaration of all textures\n",
        "\n",
        "layout(set=2, binding = 10) uniform sampler1D ocio_lut1d_0Sampler; \n",
        "layout(set=2, binding = 11) uniform sampler3D ocio_lut3d_1Sampler; \n",
        "layout(set=2, binding = 12) uniform sampler2D ocio_lut1d_2Sampler; \n",
        "\n",
        "// Declaration of all helper methods\n",
        "\n",
        "vec2 ocio_lut1d_2_computePos(float f)\n",
        "{\n",
        "  float dep;\n",
        "  float abs_f = abs(f);\n",
        "  if (abs_f > 6.10351562e-05)\n",
        "  {\n",
        "    vec3 fComp = vec3(15., 15., 15.);\n",
        "    float absarr = min( abs_f, 65504.);\n",
        "    fComp.x = floor( log2( absarr ) );\n",
        "    float lower = pow( 2.0, fComp.x );\n",
        "    fComp.y = ( absarr - lower ) / lower;\n",
        "    vec3 scale = vec3(1024., 1024., 1024.);\n",
        "    dep = dot( fComp, scale );\n",
        "  }\n",
        "  else\n",
        "  {\n",
        "    dep = abs_f * 16777216.;\n",
        "  }\n",
        "  dep += (f < 0.) ? 32768.0 : 0.0;\n",
        "  vec2 retVal;\n",
        "  retVal.y = floor(dep / 4095.);\n",
        "  retVal.x = dep - retVal.y * 4095.;\n",
        "  retVal.x = (retVal.x + 0.5) / 4096.;\n",
        "  retVal.y = (retVal.y + 0.5) / 17.;\n",
        "  return retVal;\n",
        "}\n",
        "\n",
        "// Declaration of the OCIO shader function\n",
        "\n",
        "vec4 OCIOMain(vec4 inPixel)\n",
        "{\n",
        "  vec4 outColor = inPixel;\n",
        "  \n",
        "  // Add LUT 1D processing for ocio_lut1d_0\n",
        "  \n",
        "  {\n",
        "    vec3 ocio_lut1d_0_coords = (outColor.rgb * vec3(64., 64., 64.) + vec3(0.5, 0.5, 0.5) ) / vec3(65., 65., 65.);\n",
        "    outColor.r = texture(ocio_lut1d_0Sampler, ocio_lut1d_0_coords.r).r;\n",
        "    outColor.g = texture(ocio_lut1d_0Sampler, ocio_lut1d_0_coords.g).r;\n",
        "    outColor.b = texture(ocio_lut1d_0Sampler, ocio_lut1d_0_coords.b).r;\n",
        "  }\n",
        "  \n",
        "  // Add LUT 3D processing for ocio_lut3d_1\n",
        "  \n",
        "  vec3 ocio_lut3d_1_coords = (outColor.zyx * vec3(2., 2., 2.) + vec3(0.5, 0.5, 0.5)) / vec3(3., 3., 3.);\n",
        "  outColor.rgb = texture(ocio_lut3d_1Sampler, ocio_lut3d_1_coords).rgb;\n",
        "  \n",
        "  // Add LUT 1D processing for ocio_lut1d_2\n",
        "  \n",
        "  {\n",
        "    outColor.r = texture(ocio_lut1d_2Sampler, ocio_lut1d_2_computePos(outColor.r)).r;\n",
        "    outColor.g = texture(ocio_lut1d_2Sampler, ocio_lut1d_2_computePos(outColor.g)).r;\n",
        "    outColor.b = texture(ocio_lut1d_2Sampler, ocio_lut1d_2_computePos(outColor.b)).r;\n",
        "  }\n",
        "\n",
        "  return outColor;\n",
        "}\n",
    );
    assert_text_eq(expected, desc.shader_text());
}

const EMPTY_GLSL_SHADER: &str = r#"
// Declaration of the OCIO shader function

vec4 OCIOMain(vec4 inPixel)
{
  vec4 outColor = inPixel;

  return outColor;
}
"#;

fn default_shader_text(t: Transform) -> String {
    let config = Config::create_raw();
    let proc = processor(&config, t);
    let gpu = proc
        .optimized_gpu_processor(OptimizationFlags::NONE)
        .unwrap();
    let mut desc = GpuShaderDesc::new();
    gpu.extract_gpu_shader_info(&mut desc).unwrap();
    desc.shader_text().to_string()
}

#[test]
fn grading_local_bypass() {
    // The GPU shaders are empty for identity transforms.
    assert_eq!(
        EMPTY_GLSL_SHADER,
        default_shader_text(Transform::GradingHueCurve(GradingHueCurveTransform::new(
            GradingStyle::Log
        )))
    );
    assert_eq!(
        EMPTY_GLSL_SHADER,
        default_shader_text(Transform::GradingPrimary(GradingPrimaryTransform::new(
            GradingStyle::Log
        )))
    );
    assert_eq!(
        EMPTY_GLSL_SHADER,
        default_shader_text(Transform::GradingRgbCurve(GradingRgbCurveTransform::new(
            GradingStyle::Log
        )))
    );
    assert_eq!(
        EMPTY_GLSL_SHADER,
        default_shader_text(Transform::GradingTone(GradingToneTransform::new(
            GradingStyle::Log
        )))
    );
}

#[test]
fn processor_is_noop() {
    // Basic validation of the is_no_op() behavior.
    let config = Config::create();
    let mut matrix = MatrixTransform::default();
    let proc = processor(&config, Transform::Matrix(matrix.clone()));
    assert!(proc.is_no_op());
    assert!(proc.default_gpu_processor().unwrap().is_no_op());

    matrix.offset = [0.1, 0.2, 0.3, 0.4];
    let proc = processor(&config, Transform::Matrix(matrix));
    assert!(!proc.default_gpu_processor().unwrap().is_no_op());

    // Check with at least one dynamic property.
    let mut ec = ExposureContrastTransform::default();
    let proc = processor(&config, Transform::ExposureContrast(ec.clone()));
    assert!(proc.default_gpu_processor().unwrap().is_no_op());

    ec.exposure_dynamic = true;
    let proc = processor(&config, Transform::ExposureContrast(ec));
    assert!(!proc.default_gpu_processor().unwrap().is_no_op());
}

#[test]
fn processor_channel_crosstalk() {
    // Basic validation of the has_channel_crosstalk() behavior.
    let config = Config::create();
    let mut matrix = MatrixTransform::default();
    let mut mat = [
        1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 2., 0., 0., 0., 0., 1.,
    ];
    matrix.matrix = mat;

    let proc = processor(&config, Transform::Matrix(matrix.clone()));
    assert!(!proc
        .default_gpu_processor()
        .unwrap()
        .has_channel_crosstalk());

    // That's not anymore a diagonal matrix.
    mat[4] = 1.;
    matrix.matrix = mat;
    let proc = processor(&config, Transform::Matrix(matrix.clone()));
    assert!(proc
        .default_gpu_processor()
        .unwrap()
        .has_channel_crosstalk());

    // It's now back to a diagonal matrix.
    mat[4] = 0.;
    matrix.matrix = mat;
    let proc = processor(&config, Transform::Matrix(matrix));
    assert!(!proc
        .default_gpu_processor()
        .unwrap()
        .has_channel_crosstalk());
}

#[test]
fn processor_cache_gpu_processors() {
    // Test the cache for the GPU processors.
    let config = Config::create();

    let mut matrix = MatrixTransform::default();
    matrix.offset = [0.1, 0.2, 0.3, 0.];
    let proc1 = processor(&config, Transform::Matrix(matrix));

    let gpu1 = proc1
        .optimized_gpu_processor(OptimizationFlags::DEFAULT)
        .unwrap();
    let gpu2 = proc1
        .optimized_gpu_processor(OptimizationFlags::DEFAULT)
        .unwrap();
    assert!(Arc::ptr_eq(&gpu1, &gpu2));

    // The optimization flag is different.
    let gpu2 = proc1
        .optimized_gpu_processor(OptimizationFlags::LOSSLESS)
        .unwrap();
    assert!(!Arc::ptr_eq(&gpu1, &gpu2));

    let gpu1 = proc1
        .optimized_gpu_processor(OptimizationFlags::LOSSLESS)
        .unwrap();
    assert!(Arc::ptr_eq(&gpu1, &gpu2));

    // Even with a 'dynamic' transform (i.e. contains dynamic properties) the
    // cache is still used.
    let mut ec = ExposureContrastTransform {
        exposure: 0.65,
        ..Default::default()
    };
    let proc1 = processor(&config, Transform::ExposureContrast(ec.clone()));
    assert!(Arc::ptr_eq(
        &proc1.default_gpu_processor().unwrap(),
        &proc1.default_gpu_processor().unwrap()
    ));

    // Make exposure dynamic.
    ec.exposure_dynamic = true;
    let proc1 = processor(&config, Transform::ExposureContrast(ec));
    assert!(Arc::ptr_eq(
        &proc1.default_gpu_processor().unwrap(),
        &proc1.default_gpu_processor().unwrap()
    ));
}

#[test]
fn dynamic_property_uniforms() {
    // A dynamic exposure is a uniform connected to the shader dynamic
    // property.
    let config = Config::create();
    let ec = ExposureContrastTransform {
        exposure: 0.65,
        exposure_dynamic: true,
        ..Default::default()
    };
    let proc = processor(&config, Transform::ExposureContrast(ec));
    let gpu = proc.default_gpu_processor().unwrap();

    let mut desc = GpuShaderDesc::new();
    desc.set_language(GpuLanguage::Glsl4_0);
    gpu.extract_gpu_shader_info(&mut desc).unwrap();

    assert!(desc
        .shader_text()
        .contains("uniform float ocio_exposure_contrast_exposureVal;"));
    assert_eq!(desc.num_uniforms(), 1);
    assert_eq!(desc.num_dynamic_properties(), 1);

    let (name, data) = desc.uniform(0).unwrap();
    assert_eq!(name, "ocio_exposure_contrast_exposureVal");
    let getter = match &data.value {
        UniformValue::Double(g) => g.clone(),
        _ => panic!("unexpected uniform type"),
    };
    assert_eq!(getter(), 0.65);

    let prop = desc
        .dynamic_property(crate::types::DynamicPropertyType::Exposure)
        .unwrap();
    prop.as_double().unwrap().set(1.5);
    assert_eq!(getter(), 1.5);

    // The processor property is decoupled from the shader one.
    let proc_prop = proc
        .dynamic_property(crate::types::DynamicPropertyType::Exposure)
        .unwrap();
    assert_eq!(proc_prop.as_double().unwrap().get(), 0.65);

    assert!(desc
        .add_dynamic_property(prop)
        .unwrap_err()
        .message()
        .contains("Dynamic property already here"));
    assert_eq!(
        desc.dynamic_property_at(3).unwrap_err().message(),
        "Dynamic properties access error: index = 3 where size = 1"
    );
    assert!(desc
        .dynamic_property(crate::types::DynamicPropertyType::Contrast)
        .is_err());
}

#[test]
fn osl_unsupported_luts() {
    let config =
        cs1_cs2_config("    from_scene_reference: !<FileTransform> {src: lut1d_green.ctf}\n");
    let t = ColorSpaceTransform {
        src: "cs1".into(),
        dst: "cs2".into(),
        ..Default::default()
    };
    let proc = processor(&config, Transform::ColorSpace(t));
    let gpu = proc.default_gpu_processor().unwrap();
    let mut desc = GpuShaderDesc::new();
    desc.set_language(GpuLanguage::Osl1);
    assert_eq!(
        gpu.extract_gpu_shader_info(&mut desc)
            .unwrap_err()
            .message(),
        "The Lut1DOp is not yet supported by the 'Open Shading language (OSL)' translation"
    );
}

#[test]
fn clone_creator() {
    let mut desc = GpuShaderDesc::new();
    desc.set_language(GpuLanguage::HlslSm5_0);
    desc.set_function_name("f__n");
    assert_eq!(desc.function_name(), "f_n");
    desc.add_to_function_shader_code("abc");
    let c = desc.clone_creator();
    assert_eq!(c.language(), GpuLanguage::HlslSm5_0);
    assert_eq!(c.function_name(), "f_n");
    assert_eq!(c.num_dynamic_properties(), 0);
}

/// GLSL 4.0 shader text of the `raw` to `c` processor of a config whose `c`
/// color space is `transform` (from the reference).
fn from_reference_glsl(transform: &str) -> String {
    let config = format!(
        "ocio_profile_version: 2.6\n\nroles:\n  default: raw\n\ncolorspaces:\n  - !<ColorSpace>\n    name: raw\n\n  - !<ColorSpace>\n    name: c\n    from_scene_reference: {transform}\n"
    );
    let config = Config::create_from_str(&config).unwrap();
    let gpu = config
        .get_processor("raw", "c")
        .unwrap()
        .default_gpu_processor()
        .unwrap();
    let mut desc = GpuShaderDesc::new();
    desc.set_language(GpuLanguage::Glsl4_0);
    gpu.extract_gpu_shader_info(&mut desc).unwrap();
    desc.shader_text().to_string()
}

// The expected lines of the two tests below come from the C++ OCIO 2.6
// library.

#[test]
fn camera_log_break_precision() {
    let text = from_reference_glsl(
        "!<LogCameraTransform> {base: 10, lin_side_offset: 0.1, lin_side_slope: 1.2, \
         log_side_offset: 0.3, log_side_slope: 0.4, lin_side_break: 0.2}",
    );
    assert!(text.contains(
        "    vec3 linear_segment_offset = vec3(-0.0100327656, -0.0100327656, -0.0100327656);\n"
    ));

    let text = from_reference_glsl(
        "!<LogCameraTransform> {base: 10, lin_side_offset: 0.1, lin_side_slope: 1.2, \
         log_side_offset: 0.3, log_side_slope: 0.4, lin_side_break: 0.2, linear_slope: 1.5, \
         direction: inverse}",
    );
    assert!(text.contains("    vec3 log_break = vec3(0.112591565, 0.112591565, 0.112591565);\n"));
    assert!(text.contains(
        "    vec3 linear_segment_offset = vec3(-0.187408447, -0.187408447, -0.187408447);\n"
    ));

    let text = from_reference_glsl("!<BuiltinTransform> {style: SONY_SLOG3-SGAMUT3_to_ACES2065-1}");
    assert!(text.contains("    vec3 log_break = vec3(0.167360991, 0.167360991, 0.167360991);\n"));
    assert!(text.contains(
        "    vec3 linear_segment_offset = vec3(0.092864126, 0.092864126, 0.092864126);\n"
    ));
}

#[test]
fn aces2_clamp_upper_bound_precision() {
    let text = from_reference_glsl(
        "!<BuiltinTransform> {style: \"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-300nit-P3-D65_2.0\"}",
    );
    assert!(text.contains(
        "    outColor.rgb = min(vec3(3468.943359375, 3468.943359375, 3468.943359375), outColor.rgb);\n"
    ));
}
