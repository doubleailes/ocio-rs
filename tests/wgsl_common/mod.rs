//! Processors covering every GPU op renderer, shared by the WGSL tests.

#![allow(dead_code)]

use ocio::builtins::registry::builtin_transform_styles;
use ocio::{
    BuiltinTransform, Config, FileTransform, FixedFunctionStyle, FixedFunctionTransform,
    GradingHueCurveTransform, GradingPrimaryTransform, GradingRgbCurveTransform, GradingStyle,
    GradingToneTransform, Interpolation, Processor, Transform, TransformDirection,
};
use std::path::PathBuf;

/// Directory of the test fixtures.
pub fn data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/files")
}

/// True to test the whole corpus: always in optimized builds (the CI runs
/// `cargo test --release --test gpu_wgsl`), or when `OCIO_TEST_WGSL_FULL`
/// is set. Debug builds otherwise test a subset covering every op type.
pub fn full_corpus() -> bool {
    !cfg!(debug_assertions) || std::env::var_os("OCIO_TEST_WGSL_FULL").is_some()
}

/// The fixtures tested by debug builds (every op type and LUT texture
/// kind).
const FILE_SUBSET: &[&str] = &[
    "lut1d_1.spi1d",
    "lut1d_half_domain_raw_half_set.clf",
    "lut1d_1024_hue_adjust_test.ctf",
    "lut1d_32_10i_10i.ctf",
    "Test_12to16fp.lut",
    "iridas_3d.cube",
    "lut3d_17x17x17_10i_12i.clf",
    "lut1d_lut3d_lut1d.clf",
    "cdl_all_styles.clf",
    "log_all_styles.clf",
    "exponent_all_styles.clf",
    "range.clf",
    "matrix_example_utf8.clf",
    "gamma_test1.ctf",
    "gamma_alpha_test1.ctf",
    "exposure_contrast_log.ctf",
    "exposure_contrast_video.ctf",
    "fixed_function.ctf",
    "deprecated_ops.ctf",
    "indexMap_test.ctf",
    "multiple_ops.clf",
];

/// A named processor of the corpus.
pub struct Case {
    pub name: String,
    pub processor: Processor,
}

/// Add a case; a processor that cannot be built fails the test rather
/// than silently shrinking the corpus.
fn push(cases: &mut Vec<Case>, name: String, p: ocio::Result<Processor>) {
    match p {
        Ok(processor) => cases.push(Case { name, processor }),
        Err(e) => panic!("{name}: cannot build the processor: {e}"),
    }
}

fn both_dirs() -> [TransformDirection; 2] {
    [TransformDirection::Forward, TransformDirection::Inverse]
}

/// Every builtin transform, in both directions.
pub fn builtin_cases() -> Vec<Case> {
    let config = Config::create_raw();
    let mut cases = Vec::new();
    for (i, style) in builtin_transform_styles().into_iter().enumerate() {
        if !full_corpus() && i % 3 != 0 {
            continue;
        }
        for dir in both_dirs() {
            let t = Transform::Builtin(BuiltinTransform {
                direction: dir,
                style: style.to_string(),
            });
            push(
                &mut cases,
                format!("builtin {style} {dir:?}"),
                config.get_processor_for_transform(&t, TransformDirection::Forward),
            );
        }
    }
    cases
}

/// Every LUT / CTF / CLF fixture that loads, in both directions and with
/// the interpolations the GPU renderers implement.
pub fn file_cases() -> Vec<Case> {
    let config = Config::create_raw();
    let mut cases = Vec::new();
    let mut files: Vec<PathBuf> = Vec::new();
    for dir in [data_dir(), data_dir().join("clf")] {
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_file() {
                    files.push(p);
                }
            }
        }
    }
    files.sort();
    for path in files {
        let src = path.to_string_lossy().to_string();
        let file = path.file_name().unwrap().to_string_lossy().to_string();
        if !full_corpus() && !FILE_SUBSET.contains(&file.as_str()) {
            continue;
        }
        // The interpolations only change the forward LUTs (the GPU inverse
        // LUTs are baked).
        let variants = [
            (TransformDirection::Forward, Interpolation::Default),
            (TransformDirection::Forward, Interpolation::Linear),
            (TransformDirection::Forward, Interpolation::Tetrahedral),
            (TransformDirection::Forward, Interpolation::Nearest),
            (TransformDirection::Inverse, Interpolation::Default),
        ];
        let file_transform = |dir, interp| {
            Transform::File(FileTransform {
                direction: dir,
                src: src.clone(),
                interpolation: interp,
                ..Default::default()
            })
        };
        // The fixtures directory also holds files that do not load in a raw
        // config on purpose (invalid files, references needing a search
        // path, images): only the files loading forward are tested, and then
        // every variant must build.
        if config
            .get_processor_for_transform(
                &file_transform(TransformDirection::Forward, Interpolation::Default),
                TransformDirection::Forward,
            )
            .is_err()
        {
            continue;
        }
        for (dir, interp) in variants {
            push(
                &mut cases,
                format!("file {file} {dir:?} {interp:?}"),
                config.get_processor_for_transform(
                    &file_transform(dir, interp),
                    TransformDirection::Forward,
                ),
            );
        }
    }
    cases
}

/// The grading transforms, static and dynamic, in every style and direction.
pub fn grading_cases() -> Vec<Case> {
    let config = Config::create_raw();
    let mut cases = Vec::new();
    for style in [GradingStyle::Log, GradingStyle::Lin, GradingStyle::Video] {
        for dir in both_dirs() {
            for dynamic in [false, true] {
                let mut p = GradingPrimaryTransform::new(style);
                p.direction = dir;
                p.dynamic = dynamic;
                p.value.contrast.master = 1.2;
                p.value.gamma.red = 1.1;
                p.value.offset.blue = 0.01;
                p.value.saturation = 0.9;
                p.value.clamp_black = -0.1;
                p.value.clamp_white = 5.0;
                push(
                    &mut cases,
                    format!("grading primary {style:?} {dir:?} dyn={dynamic}"),
                    config.get_processor_for_transform(
                        &Transform::GradingPrimary(p),
                        TransformDirection::Forward,
                    ),
                );

                let mut t = GradingToneTransform::new(style);
                t.direction = dir;
                t.dynamic = dynamic;
                t.value.midtones.master = 1.2;
                t.value.shadows.red = 0.9;
                t.value.highlights.green = 1.1;
                t.value.whites.master = 1.1;
                t.value.blacks.blue = 0.95;
                t.value.s_contrast = 1.1;
                push(
                    &mut cases,
                    format!("grading tone {style:?} {dir:?} dyn={dynamic}"),
                    config.get_processor_for_transform(
                        &Transform::GradingTone(t),
                        TransformDirection::Forward,
                    ),
                );

                let mut c = GradingRgbCurveTransform::new(style);
                c.direction = dir;
                c.dynamic = dynamic;
                if !dynamic {
                    // A non-identity curve so the op is kept.
                    c.value = rgb_curve_value(style);
                }
                push(
                    &mut cases,
                    format!("grading rgb curve {style:?} {dir:?} dyn={dynamic}"),
                    config.get_processor_for_transform(
                        &Transform::GradingRgbCurve(c),
                        TransformDirection::Forward,
                    ),
                );

                let mut h = GradingHueCurveTransform::new(style);
                h.direction = dir;
                h.dynamic = dynamic;
                if !dynamic {
                    h.value = hue_curve_value(style);
                }
                push(
                    &mut cases,
                    format!("grading hue curve {style:?} {dir:?} dyn={dynamic}"),
                    config.get_processor_for_transform(
                        &Transform::GradingHueCurve(h),
                        TransformDirection::Forward,
                    ),
                );
            }
        }
    }
    cases
}

fn rgb_curve_value(style: GradingStyle) -> ocio::GradingRgbCurve {
    let mut v = ocio::GradingRgbCurve::new(style);
    let pts = &mut v.curves[3].control_points;
    if pts.len() >= 2 {
        let mid = pts.len() / 2;
        pts[mid].y += 0.05;
    }
    v
}

fn hue_curve_value(style: GradingStyle) -> ocio::GradingHueCurve {
    let mut v = ocio::GradingHueCurve::new(style);
    let pts = &mut v.curves[1].control_points;
    if !pts.is_empty() {
        pts[0].y += 0.1;
    }
    v
}

/// The fixed functions, with typical parameters, in both directions.
pub fn fixed_function_cases() -> Vec<Case> {
    use FixedFunctionStyle as S;
    let config = Config::create_raw();
    let mut cases = Vec::new();
    let list: Vec<(S, Vec<f64>)> = vec![
        (S::AcesRedMod03, vec![]),
        (S::AcesRedMod10, vec![]),
        (S::AcesGlow03, vec![]),
        (S::AcesGlow10, vec![]),
        (S::AcesDarkToDim10, vec![]),
        (S::Rec2100Surround, vec![0.78]),
        (S::RgbToHsv, vec![]),
        (S::XyzToXyy, vec![]),
        (S::XyzToUvy, vec![]),
        (S::XyzToLuv, vec![]),
        // ACES_GAMUTMAP_02 and ACES_GAMUTMAP_07 are not implemented by
        // OpenColorIO (their transforms fail to validate).
        (
            S::AcesGamutComp13,
            vec![1.147, 1.264, 1.312, 0.815, 0.803, 0.880, 1.2],
        ),
        (S::LinToPq, vec![]),
        // The HLG curve.
        (
            S::LinToGammaLog,
            vec![
                0.0,
                0.25,
                0.5,
                1.0,
                0.0,
                std::f64::consts::E,
                0.17883277,
                0.807825590164,
                1.0,
                -0.07116723,
            ],
        ),
        (
            S::LinToDoubleLog,
            vec![
                10.0, 0.25, 0.5, -1.0, 0.0, -1.0, 1.25, 1.0, 1.0, 1.0, 0.5, 1.0, 0.0,
            ],
        ),
        (S::RgbToHsyLin, vec![]),
        (S::RgbToHsyLog, vec![]),
        (S::RgbToHsyVid, vec![]),
    ];
    for (style, params) in list {
        for dir in both_dirs() {
            let mut t = FixedFunctionTransform::new(style, &params);
            t.direction = dir;
            push(
                &mut cases,
                format!("fixed function {style:?} {dir:?}"),
                config.get_processor_for_transform(
                    &Transform::FixedFunction(t),
                    TransformDirection::Forward,
                ),
            );
        }
    }
    cases
}

/// Display / view processors of the builtin configs (from the scene linear
/// role).
pub fn display_view_cases(config_names: &[&str]) -> Vec<Case> {
    let mut cases = Vec::new();
    for name in config_names {
        let config = Config::create_from_builtin_config(name).unwrap();
        let src = "scene_linear";
        for d in 0..config.num_displays() {
            let display = config.display(d);
            for v in 0..config.num_views(&display) {
                let view = config.view(&display, v);
                push(
                    &mut cases,
                    format!("{name} {display} / {view}"),
                    config.get_display_view_processor(src, &display, &view),
                );
            }
        }
    }
    cases
}

/// The whole corpus.
pub fn all_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    cases.extend(builtin_cases());
    cases.extend(file_cases());
    cases.extend(grading_cases());
    cases.extend(fixed_function_cases());
    cases.extend(display_view_cases(&[
        "cg-config-latest",
        "studio-config-latest",
    ]));
    cases
}

pub mod gpu_unit;
pub mod interp;

use ocio::gpu::{GpuShaderCreator, GpuShaderDesc};
use ocio::GpuLanguage;
use std::collections::HashMap;
use std::sync::Arc;

/// The WGSL shader of a processor, validated by naga.
pub struct WgslShader {
    pub desc: GpuShaderDesc,
    pub module: naga::Module,
}

/// Generate and validate the WGSL shader of `processor`.
pub fn wgsl_shader(processor: &Processor) -> Result<WgslShader, String> {
    let gpu = processor
        .default_gpu_processor()
        .map_err(|e| e.to_string())?;
    let mut desc = GpuShaderDesc::new();
    desc.set_language(GpuLanguage::Wgsl);
    gpu.extract_gpu_shader_info(&mut desc)
        .map_err(|e| format!("generation: {e}"))?;
    let src = desc.shader_text().to_string();
    let module = naga::front::wgsl::parse_str(&src)
        .map_err(|e| format!("parse: {}\n{src}", e.emit_to_string(&src)))?;
    let mut validator = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    );
    validator
        .validate(&module)
        .map_err(|e| format!("validation: {}\n{src}", e.emit_to_string(&src)))?;
    Ok(WgslShader { desc, module })
}

/// The LUT textures of the shader, by (group, binding).
pub fn textures(desc: &GpuShaderDesc) -> HashMap<(u32, u32), Arc<interp::Texture>> {
    let bindings = desc.wgsl_bindings().expect("WGSL bindings");
    let mut values: HashMap<String, Vec<f32>> = HashMap::new();
    for i in 0..desc.num_textures() {
        let t = desc.texture(i).unwrap();
        values.insert(t.texture_name.clone(), t.values.clone());
    }
    for i in 0..desc.num_3d_textures() {
        let t = desc.texture_3d(i).unwrap();
        values.insert(t.texture_name.clone(), t.values.clone());
    }
    let mut textures = HashMap::new();
    for t in &bindings.textures {
        let bytes = t.texel_data(&values[&t.texture_name]).unwrap();
        let floats: Vec<f32> = bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_le_bytes(*b))
            .collect();
        let texels = match t.format {
            ocio::gpu::wgsl::WgslTextureFormat::R32Float => {
                floats.iter().map(|v| [*v, 0.0, 0.0, 1.0]).collect()
            }
            ocio::gpu::wgsl::WgslTextureFormat::Rgba32Float => floats.as_chunks::<4>().0.to_vec(),
        };
        textures.insert(
            (t.group, t.binding),
            Arc::new(interp::Texture {
                width: t.width,
                height: t.height,
                depth: t.depth,
                texels,
            }),
        );
    }
    textures
}

/// A machine running the shader with the current uniform values.
pub fn machine(shader: &WgslShader) -> interp::Machine<'_> {
    let desc = &shader.desc;
    let bindings = desc.wgsl_bindings().expect("WGSL bindings");
    let uniforms = bindings.uniform_buffer(desc).expect("uniform buffer");
    interp::Machine::new(&shader.module, &uniforms, &textures(desc))
}

/// Test pixels: negative values, black, near zero, diffuse range,
/// saturated colors and scene-linear HDR values.
pub const PIXELS: &[[f32; 4]] = &[
    [-0.1, -0.05, -0.2, 1.0],
    [-1.5, 0.3, 0.6, 1.0],
    [0.0, 0.0, 0.0, 1.0],
    [1e-4, 2e-4, 5e-5, 1.0],
    [0.002, 0.004, 0.001, 1.0],
    [0.18, 0.18, 0.18, 1.0],
    [0.5, 0.4, 0.3, 0.5],
    [0.9, 0.1, 0.5, 1.0],
    [1.0, 1.0, 1.0, 1.0],
    [1.0, 0.0, 0.0, 1.0],
    [0.0, 1.0, 0.0, 1.0],
    [0.0, 0.0, 1.0, 0.0],
    [4.0, 2.0, 1.0, 1.0],
    [16.0, 8.0, 30.0, 1.0],
    [100.0, 50.0, 10.0, 1.0],
];

/// The path of the fixture `tests/data/files/<name>`, or `None` when it is
/// one of the large fixtures left out of the published crate (the tests
/// using it then skip it, as `test_fixture_present` does in the unit
/// tests).
pub fn fixture(name: &str) -> Option<String> {
    let path = data_dir().join(name);
    if path.exists() {
        Some(path.to_string_lossy().to_string())
    } else {
        eprintln!("skipped: tests/data/files/{name} is not in the published crate");
        None
    }
}
