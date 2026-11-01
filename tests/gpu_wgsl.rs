//! WGSL shader generation (`GpuLanguage::Wgsl`): every generated module is
//! parsed and validated by naga, then executed on the CPU by a naga IR
//! interpreter and compared with the CPU processor.

mod wgsl_common;

use ocio::gpu::wgsl::{WgslTextureFormat, WgslUniformType};
use ocio::gpu::{GpuShaderCreator, GpuShaderDesc, UniformDataType};
use ocio::{
    Config, DynamicPropertyType, ExposureContrastTransform, GpuLanguage, GradingPrimaryTransform,
    GradingStyle, MatrixTransform, OptimizationFlags, Transform, TransformDirection,
};
use wgsl_common::{interp, Case, PIXELS};

/// Largest accepted difference between the WGSL and the CPU results,
/// relative to the CPU value (absolute below 1).
///
/// The CPU processors use the same LUT approximations as the GPU (fast LUT
/// inverses) but exact log / exp / pow; the remaining differences come from
/// the SSE2 style approximations of some CPU renderers and from single
/// precision evaluation order (the largest seen is 7.3e-4, on a 1D LUT
/// extrapolated at a value of 100).
const TOLERANCE: f32 = 1e-3;

/// Non-finite GPU results that the shaders of every shading language give.
///
/// The ACES 0.3 / 1.0 glow shaders compute `0 * inf` (in
/// `GlowGain * (GlowMid / YC - 0.5)`) for an exactly black pixel, which is
/// NaN on GPUs while the CPU renderer gives 0.
fn known_non_finite(case: &str, pixel: &[f32; 4]) -> bool {
    case.contains("AcesGlow") && pixel[..3] == [0.0, 0.0, 0.0]
}

fn cpu_flags() -> OptimizationFlags {
    OptimizationFlags(OptimizationFlags::DEFAULT.0 & !OptimizationFlags::FAST_LOG_EXP_POW.0)
}

/// Check the resources described by `wgsl_bindings` against the shader
/// description.
fn check_bindings(name: &str, desc: &GpuShaderDesc) -> Result<(), String> {
    let b = desc.wgsl_bindings().ok_or("no WGSL bindings")?;
    if b.uniform_buffer_size % 16 != 0 {
        return Err(format!(
            "{name}: uniform buffer size {}",
            b.uniform_buffer_size
        ));
    }
    if b.uniforms.len() != desc.num_uniforms() {
        return Err(format!(
            "{name}: {} WGSL uniforms, {} in the description",
            b.uniforms.len(),
            desc.num_uniforms()
        ));
    }
    for i in 0..desc.num_uniforms() {
        let (uname, data) = desc.uniform(i).unwrap();
        let u = b
            .uniform(uname)
            .ok_or(format!("{name}: no uniform {uname}"))?;
        // The WGSL layout is the std140 layout of the Vulkan uniform block.
        if u.offset as usize != data.buffer_offset {
            return Err(format!(
                "{name}: uniform {uname} at {} instead of {}",
                u.offset, data.buffer_offset
            ));
        }
        let ok = matches!(
            (data.data_type(), u.ty),
            (UniformDataType::Double, WgslUniformType::Float)
                | (UniformDataType::Bool, WgslUniformType::Bool)
                | (UniformDataType::Float3, WgslUniformType::Float3)
                | (UniformDataType::VectorFloat, WgslUniformType::FloatArray(_))
                | (UniformDataType::VectorInt, WgslUniformType::IntArray(_))
        );
        if !ok {
            return Err(format!("{name}: uniform {uname} has the type {:?}", u.ty));
        }
    }
    if b.uniform_buffer_size as usize != desc.uniform_buffer_size().div_ceil(16) * 16 {
        return Err(format!("{name}: uniform buffer size mismatch"));
    }
    let n = desc.num_textures() + desc.num_3d_textures();
    if b.textures.len() != n {
        return Err(format!(
            "{name}: {} textures instead of {n}",
            b.textures.len()
        ));
    }
    let mut seen = std::collections::HashSet::new();
    for t in &b.textures {
        if Some(t.binding) == b.uniform_binding || !seen.insert(t.binding) {
            return Err(format!("{name}: binding {} used twice", t.binding));
        }
        let expected = if t.dimensions == 3 {
            (0..desc.num_3d_textures())
                .map(|i| desc.texture_3d(i).unwrap())
                .find(|x| x.texture_name == t.texture_name)
        } else {
            (0..desc.num_textures())
                .map(|i| desc.texture(i).unwrap())
                .find(|x| x.texture_name == t.texture_name)
        }
        .ok_or(format!("{name}: unknown texture {}", t.texture_name))?;
        if (t.width, t.height, t.depth) != (expected.width, expected.height, expected.depth)
            || t.interpolation != expected.interpolation
        {
            return Err(format!("{name}: texture {} mismatch", t.texture_name));
        }
        let rgb = expected.channel == ocio::gpu::TextureType::RgbChannel;
        if rgb != (t.format == WgslTextureFormat::Rgba32Float) {
            return Err(format!("{name}: texture {} format", t.texture_name));
        }
    }
    Ok(())
}

/// Validate the WGSL shader of each case and compare its results with the
/// CPU processor.
fn check_cases(cases: Vec<Case>) {
    assert!(!cases.is_empty());
    let mut failures = Vec::new();
    for case in &cases {
        let shader = match wgsl_common::wgsl_shader(&case.processor) {
            Ok(s) => s,
            Err(e) => {
                failures.push(format!("{}: {e}", case.name));
                continue;
            }
        };
        if let Err(e) = check_bindings(&case.name, &shader.desc) {
            failures.push(e);
            continue;
        }
        let cpu = case.processor.optimized_cpu_processor(cpu_flags());
        let mut machine = wgsl_common::machine(&shader);
        for pixel in PIXELS {
            let gpu = interp::run_pixel(&mut machine, "OCIOMain", *pixel);
            let mut expected = *pixel;
            cpu.apply_rgba(&mut expected);
            for k in 0..4 {
                if !expected[k].is_finite() {
                    continue;
                }
                let ok = if gpu[k].is_finite() {
                    (gpu[k] - expected[k]).abs() <= TOLERANCE * expected[k].abs().max(1.0)
                } else {
                    known_non_finite(&case.name, pixel)
                };
                if !ok {
                    failures.push(format!(
                        "{}: {pixel:?} gives {gpu:?} instead of {expected:?}",
                        case.name
                    ));
                    break;
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failure(s) out of {} cases:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
}

#[test]
fn wgsl_builtin_transforms() {
    check_cases(wgsl_common::builtin_cases());
}

#[test]
fn wgsl_lut_and_ctf_files() {
    check_cases(wgsl_common::file_cases());
}

#[test]
fn wgsl_grading_ops() {
    check_cases(wgsl_common::grading_cases());
}

#[test]
fn wgsl_fixed_functions() {
    check_cases(wgsl_common::fixed_function_cases());
}

#[test]
fn wgsl_builtin_config_display_views() {
    // The views of the `sRGB - Display` (ACES 2.0, ACES 1.x and
    // un-tone-mapped) and of all the other displays.
    let cases = wgsl_common::display_view_cases(&["cg-config-latest", "studio-config-latest"]);
    assert!(cases
        .iter()
        .any(|c| c.name.contains("sRGB - Display / Un-tone-mapped")));
    assert!(cases.iter().any(|c| c
        .name
        .contains("sRGB - Display / ACES 2.0 - SDR 100 nits (Rec.709)")));
    check_cases(cases);
}

/// Parse and validate a generated WGSL module with naga.
fn validated_module(text: &str) -> naga::Module {
    let module = naga::front::wgsl::parse_str(text)
        .unwrap_or_else(|e| panic!("invalid WGSL: {}\n{text}", e.emit_to_string(text)));
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .unwrap_or_else(|e| panic!("invalid WGSL: {}\n{text}", e.emit_to_string(text)));
    module
}

/// The processor of `t` and its validated WGSL shader description.
fn wgsl_desc(t: Transform) -> (ocio::Processor, GpuShaderDesc) {
    let config = Config::create_raw();
    let processor = config
        .get_processor_for_transform(&t, TransformDirection::Forward)
        .unwrap();
    let gpu = processor.default_gpu_processor().unwrap();
    let mut desc = GpuShaderDesc::new();
    desc.set_language(GpuLanguage::Wgsl);
    gpu.extract_gpu_shader_info(&mut desc).unwrap();
    validated_module(desc.shader_text());
    (processor, desc)
}

/// Changing a dynamic property only changes the uniform buffer: the shader
/// (and its cache id) stays the same and gives the new result.
#[test]
fn wgsl_dynamic_properties_update_the_uniform_buffer_only() {
    let ec = ExposureContrastTransform {
        exposure_dynamic: true,
        contrast_dynamic: true,
        ..Default::default()
    };
    let mut gp = GradingPrimaryTransform::new(GradingStyle::Log);
    gp.dynamic = true;
    let group = ocio::GroupTransform {
        direction: TransformDirection::Forward,
        transforms: vec![
            Transform::ExposureContrast(ec),
            Transform::GradingPrimary(gp),
        ],
        metadata: Default::default(),
    };
    let (processor, desc) = wgsl_desc(Transform::Group(group));
    let text = desc.shader_text().to_string();
    let cache_id = desc.cache_id();
    let bindings = desc.wgsl_bindings().unwrap().clone();
    let module = validated_module(&text);

    let cpu = processor.optimized_cpu_processor(cpu_flags());
    let gpu_exposure = desc
        .dynamic_property(DynamicPropertyType::Exposure)
        .unwrap();
    let cpu_exposure = cpu.dynamic_property(DynamicPropertyType::Exposure).unwrap();
    let gpu_primary = desc
        .dynamic_property(DynamicPropertyType::GradingPrimary)
        .unwrap();
    let cpu_primary = cpu
        .dynamic_property(DynamicPropertyType::GradingPrimary)
        .unwrap();

    for (exposure, saturation) in [(0.0, 1.0), (-2.0, 0.5), (1.5, 1.3), (4.0, 0.8)] {
        for p in [&gpu_exposure, &cpu_exposure] {
            p.as_double().unwrap().set(exposure);
        }
        for p in [&gpu_primary, &cpu_primary] {
            let v = p.as_grading_primary().unwrap();
            let mut g = v.get();
            g.saturation = saturation;
            v.set(g);
        }
        let uniforms = bindings.uniform_buffer(&desc).unwrap();
        let mut machine = interp::Machine::new(&module, &uniforms, &Default::default());
        for pixel in PIXELS {
            let gpu = interp::run_pixel(&mut machine, "OCIOMain", *pixel);
            let mut expected = *pixel;
            cpu.apply_rgba(&mut expected);
            for k in 0..4 {
                assert!(
                    (gpu[k] - expected[k]).abs() <= TOLERANCE * expected[k].abs().max(1.0),
                    "exposure {exposure}, saturation {saturation}: {pixel:?} gives {gpu:?} instead of {expected:?}"
                );
            }
        }
        assert_eq!(desc.shader_text(), text);
        assert_eq!(desc.cache_id(), cache_id);
    }
}

#[test]
fn wgsl_module_and_bindings() {
    let m = MatrixTransform {
        offset: [0.1, 0.2, 0.3, 0.0],
        ..Default::default()
    };
    let (_, desc) = wgsl_desc(Transform::Matrix(m));
    let text = desc.shader_text();
    assert!(
        text.contains("fn OCIOMain(inPixel: vec4<f32>) -> vec4<f32> {"),
        "{text}"
    );
    assert!(
        text.contains("var outColor: vec4<f32> = inPixel;"),
        "{text}"
    );
    let b = desc.wgsl_bindings().unwrap();
    assert_eq!(b.uniform_binding, None);
    assert_eq!(b.uniform_buffer_size, 0);
    assert!(b.textures.is_empty());
    assert!(b.uniform_buffer(&desc).unwrap().is_empty());
    // The language is part of the shader cache id.
    assert!(desc.cache_id().starts_with("wgsl "));

    // The other languages do not describe WGSL bindings.
    let mut glsl = GpuShaderDesc::new();
    glsl.set_language(GpuLanguage::Glsl4_0);
    let config = Config::create_raw();
    let processor = config
        .get_processor_for_transform(
            &Transform::Matrix(MatrixTransform::default()),
            TransformDirection::Forward,
        )
        .unwrap();
    processor
        .default_gpu_processor()
        .unwrap()
        .extract_gpu_shader_info(&mut glsl)
        .unwrap();
    assert!(glsl.wgsl_bindings().is_none());
}

fn lut3d_processor() -> ocio::Processor {
    let config = Config::create_raw();
    let t = Transform::File(ocio::FileTransform {
        src: wgsl_common::data_dir()
            .join("clf/lut3d_17x17x17_10i_12i.clf")
            .to_string_lossy()
            .to_string(),
        ..Default::default()
    });
    config
        .get_processor_for_transform(&t, TransformDirection::Forward)
        .unwrap()
}

/// The resources follow the descriptor set index and the texture binding
/// start of the shader creator.
#[test]
fn wgsl_descriptor_set_and_bindings() {
    let processor = lut3d_processor();
    let gpu = processor.default_gpu_processor().unwrap();
    let mut desc = GpuShaderDesc::new();
    desc.set_language(GpuLanguage::Wgsl);
    desc.set_descriptor_set_index(2, 5).unwrap();
    gpu.extract_gpu_shader_info(&mut desc).unwrap();
    validated_module(desc.shader_text());
    let b = desc.wgsl_bindings().unwrap();
    assert_eq!(b.group, 2);
    assert!(!b.textures.is_empty());
    for t in &b.textures {
        assert_eq!(t.group, 2);
        assert!(t.binding >= 5);
        assert!(desc.shader_text().contains(&format!(
            "@group(2) @binding({}) var {}: texture_{}d<f32>;",
            t.binding, t.texture_name, t.dimensions
        )));
    }
}

#[test]
fn wgsl_texel_data() {
    let processor = lut3d_processor();
    let gpu = processor.default_gpu_processor().unwrap();
    let mut desc = GpuShaderDesc::new();
    desc.set_language(GpuLanguage::Wgsl);
    gpu.extract_gpu_shader_info(&mut desc).unwrap();
    validated_module(desc.shader_text());
    let b = desc.wgsl_bindings().unwrap();
    let t = b
        .textures
        .iter()
        .find(|t| t.dimensions == 3)
        .expect("3D texture");
    assert_eq!(t.format, WgslTextureFormat::Rgba32Float);
    let values = desc.texture_3d_values(0).unwrap();
    let data = t.texel_data(values).unwrap();
    assert_eq!(data.len(), 17 * 17 * 17 * 16);
    assert_eq!(t.bytes_per_row(), 17 * 16);
    let f = |i: usize| f32::from_le_bytes(data[4 * i..4 * i + 4].try_into().unwrap());
    assert_eq!(
        [f(4), f(5), f(6), f(7)],
        [values[3], values[4], values[5], 1.0]
    );
    assert!(t.texel_data(&values[..10]).is_err());
}
