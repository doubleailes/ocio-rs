//! Port of the OpenColorIO GPU unit test harness (`tests/gpu/GPUUnitTest.cpp`)
//! running the WGSL shaders.
//!
//! As upstream, each test builds a processor, renders an image of
//! 256 x 256 RGBA input values with the GPU shader and compares it with the
//! default CPU processor. The input image is either a ramp over the wide
//! range [-1, 2] (or [0, 1]) preceded by NaN, +Inf and -Inf test values, or
//! the custom values of the test. The comparison is absolute or relative,
//! and NaN / infinite results must match. Retests update dynamic properties
//! and render again with the same shader.
//!
//! Debug builds compare a sample of the image (all the NaN / Inf test
//! values and every 16th ramp value); optimized builds (the CI runs
//! `cargo test --release`) or `OCIO_TEST_WGSL_FULL` compare all of it.
//!
//! Upstream renders with OpenGL, Metal, DirectX or Vulkan; here the WGSL
//! shader is validated by naga and run by the naga IR interpreter of
//! [`super::interp`].

use super::interp;
use ocio::gpu::{GpuShaderCreator, GpuShaderDesc};
use ocio::{
    Config, GpuLanguage, OptimizationFlags, Processor, ProcessorCacheFlags, Transform,
    TransformDirection,
};

const WIN_WIDTH: usize = 256;
const WIN_HEIGHT: usize = 256;
const COMPONENTS: usize = 4;

/// The pixel sampling stride of the debug builds (see
/// [`super::full_corpus`]).
const DEBUG_STRIDE: usize = 16;

/// Default error threshold.
const DEFAULT_ERROR_THRESHOLD: f32 = 1e-7;

/// Some GPU renderers "round" `f32::MAX` to infinity: all the values over
/// this threshold are considered equal.
const LARGE_THRESHOLD: f32 = f32::MAX;

/// A retest: updates the dynamic properties of the processor (for the CPU)
/// and of the shader description (for the GPU).
pub type Retest = Box<dyn FnMut(&Processor, &GpuShaderDesc)>;

/// A customization of the shader description before the extraction.
pub type ShaderDescSetup = Box<dyn FnOnce(&mut GpuShaderDesc)>;

/// Port of `OCIOGPUTest`.
pub struct GpuTest {
    pub name: String,
    processor: Option<Processor>,
    pub error_threshold: f32,
    pub expected_minimal_value: f32,
    pub relative_comparison: bool,
    pub test_wide_range: bool,
    pub range_min: f32,
    pub range_max: f32,
    pub test_nan: bool,
    pub test_infinity: bool,
    /// Custom RGBA input values (generated when empty).
    pub custom_values: Vec<f32>,
    pub legacy_shader: bool,
    pub legacy_shader_lut_edge: u32,
    pub retests: Vec<Retest>,
    enabled: bool,
    /// Called on the shader description before the extraction.
    pub shader_desc_setup: Option<ShaderDescSetup>,
}

impl GpuTest {
    pub fn new(name: &str) -> Self {
        GpuTest {
            name: name.to_string(),
            processor: None,
            error_threshold: DEFAULT_ERROR_THRESHOLD,
            expected_minimal_value: 1e-6,
            relative_comparison: false,
            test_wide_range: true,
            range_min: -1.0,
            range_max: 2.0,
            test_nan: true,
            test_infinity: true,
            custom_values: Vec::new(),
            legacy_shader: false,
            legacy_shader_lut_edge: 32,
            retests: Vec::new(),
            enabled: true,
            shader_desc_setup: None,
        }
    }

    /// Use the processor of `transform` in a raw config without cache.
    pub fn set_processor_transform(&mut self, transform: Transform) {
        let config = Config::create_raw();
        config.set_processor_cache_flags(ProcessorCacheFlags::OFF);
        self.set_processor_config(&config, transform);
    }

    /// Use the processor of `transform` in `config`.
    pub fn set_processor_config(&mut self, config: &Config, transform: Transform) {
        let p = config
            .get_processor_for_transform(&transform, TransformDirection::Forward)
            .unwrap_or_else(|e| panic!("{}: {e}", self.name));
        self.set_processor(p);
    }

    pub fn set_processor(&mut self, processor: Processor) {
        assert!(self.processor.is_none(), "GPU Unit test already exists");
        self.processor = Some(processor);
    }

    pub fn processor(&self) -> &Processor {
        self.processor.as_ref().expect("no processor")
    }

    pub fn set_error_threshold(&mut self, e: f32) {
        self.error_threshold = e;
    }

    pub fn set_expected_minimal_value(&mut self, v: f32) {
        self.expected_minimal_value = v;
    }

    pub fn set_relative_comparison(&mut self, rel: bool) {
        self.relative_comparison = rel;
    }

    pub fn set_test_wide_range(&mut self, wide: bool) {
        self.test_wide_range = wide;
    }

    pub fn set_wide_range_interval(&mut self, min: f32, max: f32) {
        self.range_min = min;
        self.range_max = max;
    }

    pub fn set_test_nan(&mut self, nan: bool) {
        self.test_nan = nan;
    }

    pub fn set_test_infinity(&mut self, inf: bool) {
        self.test_infinity = inf;
    }

    pub fn set_custom_values(&mut self, values: Vec<f32>) {
        self.custom_values = values;
    }

    pub fn set_legacy_shader(&mut self, legacy: bool) {
        self.legacy_shader = legacy;
    }

    pub fn set_legacy_shader_lut_edge(&mut self, edge: u32) {
        self.legacy_shader_lut_edge = edge;
    }

    pub fn add_retest(&mut self, r: impl FnMut(&Processor, &GpuShaderDesc) + 'static) {
        self.retests.push(Box::new(r));
    }

    /// Port of `OCIO_DISABLE_GPU_TEST`.
    pub fn disable(&mut self) {
        self.enabled = false;
    }

    /// The input values (port of `PrepareInputValues`); returns the values
    /// and the number of meaningful values.
    fn input_values(&self) -> (Vec<f32>, usize) {
        let predefined = WIN_WIDTH * WIN_HEIGHT * COMPONENTS;
        if self.custom_values.is_empty() {
            let (min, max) = if self.test_wide_range {
                (self.range_min, self.range_max)
            } else {
                (0.0, 1.0)
            };
            let range = max - min;
            let mut values = vec![min; predefined];
            let mut idx = 0;
            let mut num_entries = predefined;
            let num_tests = COMPONENTS * COMPONENTS;
            let set_test_value = |values: &mut [f32], idx: usize, val: f32| {
                for component in 0..COMPONENTS {
                    for i in 0..COMPONENTS {
                        values[idx + i + component * COMPONENTS] =
                            if i == component { val } else { 0.0 };
                    }
                }
            };
            if self.test_nan {
                set_test_value(&mut values, idx, f32::NAN);
                idx += num_tests;
                num_entries -= num_tests;
            }
            if self.test_infinity {
                set_test_value(&mut values, idx, f32::INFINITY);
                idx += num_tests;
                num_entries -= num_tests;
                set_test_value(&mut values, idx, f32::NEG_INFINITY);
                idx += num_tests;
                num_entries -= num_tests;
            }
            // The value step based on the remaining number of values.
            let step = range / num_entries as f32;
            for i in 0..num_entries {
                values[idx] = min + step * i as f32;
                idx += 1;
            }
            (values, predefined)
        } else {
            assert_eq!(
                self.custom_values.len() % COMPONENTS,
                0,
                "Only the RGBA input values are supported"
            );
            assert!(
                self.custom_values.len() <= predefined,
                "Exceed the predefined texture maximum size"
            );
            let n = self.custom_values.len();
            let mut values = self.custom_values.clone();
            values.resize(predefined, 0.0);
            (values, n)
        }
    }

    /// Build and validate the WGSL shader, render the image and compare it
    /// with the CPU (port of the main loop of `GPUUnitTest.cpp`). Panics on
    /// failure.
    pub fn run(mut self) {
        if !self.enabled {
            return;
        }
        let name = self.name.clone();
        let processor = self.processor.take().expect("Invalid test");

        let (input, num_values) = self.input_values();

        let mut desc = GpuShaderDesc::new();
        desc.set_language(GpuLanguage::Wgsl);
        desc.set_pixel_name("myPixel");
        if let Some(setup) = self.shader_desc_setup.take() {
            setup(&mut desc);
        }
        let gpu = if self.legacy_shader {
            processor
                .optimized_legacy_gpu_processor(
                    OptimizationFlags::DEFAULT,
                    self.legacy_shader_lut_edge,
                )
                .unwrap_or_else(|e| panic!("{name}: {e}"))
        } else {
            processor
                .default_gpu_processor()
                .unwrap_or_else(|e| panic!("{name}: {e}"))
        };
        gpu.extract_gpu_shader_info(&mut desc)
            .unwrap_or_else(|e| panic!("{name}: WGSL generation failed: {e}"));

        let src = desc.shader_text().to_string();
        let module = naga::front::wgsl::parse_str(&src)
            .unwrap_or_else(|e| panic!("{name}: invalid WGSL: {}\n{src}", e.emit_to_string(&src)));
        let mut validator = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        );
        validator
            .validate(&module)
            .unwrap_or_else(|e| panic!("{name}: invalid WGSL: {}\n{src}", e.emit_to_string(&src)));

        let textures = super::textures(&desc);
        let bindings = desc.wgsl_bindings().expect("WGSL bindings").clone();
        let text = desc.shader_text().to_string();

        let num_pixels = num_values / COMPONENTS;
        let full = super::full_corpus();
        let pixels: Vec<[f32; 4]> = input[..num_pixels * COMPONENTS]
            .as_chunks::<4>()
            .0
            .iter()
            .copied()
            .enumerate()
            // Debug builds test a sample of the image (the NaN / Inf test
            // values, every 16th pixel and the last one); optimized builds
            // the whole image, as upstream.
            .filter(|(i, _)| full || *i < 12 || i % DEBUG_STRIDE == 0 || *i == num_pixels - 1)
            .map(|(_, p)| p)
            .collect();

        let num_retests = self.retests.len();
        for idx in 0..=num_retests {
            if idx != 0 {
                (self.retests[idx - 1])(&processor, &desc);
            }
            // Only the uniform buffer changes: never the shader.
            assert_eq!(desc.shader_text(), text);
            let uniforms = bindings.uniform_buffer(&desc).unwrap();
            let gpu_image =
                interp::run_image(&module, &uniforms, &textures, desc.function_name(), &pixels);

            // Each retest rebuilds a CPU processor.
            let cpu = processor.default_cpu_processor();
            let mut cpu_image = pixels.clone();
            for p in cpu_image.iter_mut() {
                cpu.apply_rgba(p);
            }
            if let Err(e) = self.validate(&pixels, &cpu_image, &gpu_image) {
                panic!(
                    "{name}{}: FAILED - {e}",
                    if idx > 0 {
                        format!(" (retest {idx})")
                    } else {
                        String::new()
                    }
                );
            }
        }
    }

    /// Port of `ValidateResults`.
    fn validate(
        &self,
        input: &[[f32; 4]],
        cpu: &[[f32; 4]],
        gpu: &[[f32; 4]],
    ) -> Result<(), String> {
        let epsilon = self.error_threshold;
        let mut diff = 0.0f32;
        let mut idx_diff = None;
        let mut idx_nan = None;
        let mut idx_inf = None;
        for (i, (c, g)) in cpu.iter().zip(gpu).enumerate() {
            for chan in 0..4 {
                let (x1, x2) = (c[chan], g[chan]);
                match validate_inf(x1, x2) {
                    Limits::NotApplicable => match validate_nan(x1, x2) {
                        Limits::NotApplicable => {
                            let d = if self.relative_comparison {
                                (x1 - x2).abs() / x1.abs().max(self.expected_minimal_value)
                            } else {
                                (x2 - x1).abs()
                            };
                            if d > diff {
                                diff = d;
                                idx_diff = Some((i, chan));
                            }
                        }
                        Limits::Inconsistent => idx_nan = Some((i, chan)),
                        Limits::Consistent => {}
                    },
                    Limits::Inconsistent => idx_inf = Some((i, chan)),
                    Limits::Consistent => {}
                }
            }
        }
        let describe = |(i, chan): (usize, usize)| {
            format!(
                "pixel {i} component {chan}: src = {:?}, cpu = {:?}, gpu = {:?}",
                input[i], cpu[i], gpu[i]
            )
        };
        let mut errors = Vec::new();
        if diff > epsilon {
            errors.push(format!(
                "maximum error {diff} larger than the {} tolerance {epsilon} at {}",
                if self.relative_comparison {
                    "relative"
                } else {
                    "absolute"
                },
                describe(idx_diff.unwrap())
            ));
        }
        if let Some(i) = idx_inf {
            errors.push(format!("large number error at {}", describe(i)));
        }
        if let Some(i) = idx_nan {
            errors.push(format!("NaN error at {}", describe(i)));
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("\n"))
        }
    }
}

enum Limits {
    NotApplicable,
    Inconsistent,
    Consistent,
}

fn validate_inf(x1: f32, x2: f32) -> Limits {
    if (x1.abs() < LARGE_THRESHOLD && x2.abs() < LARGE_THRESHOLD)
        // If either value is NaN, consider this a NaN error rather than an Inf error.
        || x1.is_nan()
        || x2.is_nan()
    {
        return Limits::NotApplicable;
    }
    if (x1 >= LARGE_THRESHOLD && x2 < LARGE_THRESHOLD)
        || (x1 < LARGE_THRESHOLD && x2 >= LARGE_THRESHOLD)
        || (x1 <= -LARGE_THRESHOLD && x2 > -LARGE_THRESHOLD)
        || (x1 > -LARGE_THRESHOLD && x2 <= -LARGE_THRESHOLD)
    {
        return Limits::Inconsistent;
    }
    Limits::Consistent
}

fn validate_nan(x1: f32, x2: f32) -> Limits {
    match (x1.is_nan(), x2.is_nan()) {
        (false, false) => Limits::NotApplicable,
        (true, true) => Limits::Consistent,
        _ => Limits::Inconsistent,
    }
}

/// Run a GPU unit test: `setup` configures it (port of `OCIO_ADD_GPU_TEST`).
pub fn gpu_test(name: &str, setup: impl FnOnce(&mut GpuTest)) {
    let mut t = GpuTest::new(name);
    setup(&mut t);
    t.run();
}
