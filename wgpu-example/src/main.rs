//! Run the WGSL shader of an OCIO display / view processor with wgpu and
//! compare it with the CPU processor.
//!
//! ```text
//! cargo run --release --manifest-path wgpu-example/Cargo.toml
//! ```
//!
//! The processor is the `sRGB - Display` / `ACES 2.0 - SDR 100 nits (Rec.709)`
//! view of the default ACES 2.0 builtin config, with a dynamic exposure in
//! front of it. The pixels are run through a compute shader calling the OCIO
//! function, for several exposures: only the uniform buffer is updated
//! between the runs (the shader and the LUT textures are not regenerated).
//!
//! The bind group layout is built from `WgslBindings` only: the LUT textures
//! are declared unfilterable (`Float { filterable: false }`), so no optional
//! device feature is needed.
//!
//! Exits with 0 when every finite result is within 2 code values of the CPU
//! after 8 bits quantization, 1 otherwise and 2 when no GPU adapter is
//! available.

use ocio::gpu::wgsl::{WgslBindings, WgslTextureFormat};
use ocio::gpu::{GpuShaderCreator, GpuShaderDesc};
use ocio::{
    Config, DisplayViewTransform, DynamicPropertyType, ExposureContrastTransform, GpuLanguage,
    GroupTransform, Transform, TransformDirection,
};
use std::borrow::Cow;
use std::collections::HashMap;

const CONFIG: &str = "cg-config-latest";
const SRC: &str = "ACEScg";
const DISPLAY: &str = "sRGB - Display";
const VIEW: &str = "ACES 2.0 - SDR 100 nits (Rec.709)";

/// Group of the input / output storage buffers of the compute shader (the
/// OCIO resources use the group 0).
const IO_GROUP: u32 = 1;

fn test_pixels() -> Vec<[f32; 4]> {
    let mut p = vec![
        [-0.1, -0.05, -0.2, 1.0],
        [0.0, 0.0, 0.0, 1.0],
        [1e-4, 2e-4, 5e-5, 1.0],
        [0.18, 0.18, 0.18, 1.0],
        [0.5, 0.4, 0.3, 1.0],
        [1.0, 0.0, 0.0, 1.0],
        [0.0, 1.0, 0.0, 1.0],
        [0.0, 0.0, 1.0, 1.0],
        [4.0, 2.0, 1.0, 1.0],
        [16.0, 8.0, 30.0, 1.0],
        [100.0, 50.0, 10.0, 1.0],
    ];
    // A ramp over the scene-linear range.
    for i in 0..256 {
        let v = 2f32.powf(-12.0 + 20.0 * i as f32 / 255.0);
        p.push([v, v * 0.7, v * 0.3, 1.0]);
    }
    p
}

fn main() {
    let config = Config::create_from_builtin_config(CONFIG).expect("builtin config");
    let ec = ExposureContrastTransform {
        exposure_dynamic: true,
        ..Default::default()
    };
    let group = GroupTransform {
        direction: TransformDirection::Forward,
        transforms: vec![
            Transform::ExposureContrast(ec),
            Transform::DisplayView(DisplayViewTransform::new(SRC, DISPLAY, VIEW)),
        ],
        metadata: Default::default(),
    };
    let processor = config
        .get_processor_for_transform(&Transform::Group(group), TransformDirection::Forward)
        .expect("processor");
    let cpu = processor.default_cpu_processor();
    let gpu = processor.default_gpu_processor().expect("GPU processor");

    let mut desc = GpuShaderDesc::new();
    desc.set_language(GpuLanguage::Wgsl);
    gpu.extract_gpu_shader_info(&mut desc)
        .expect("WGSL shader generation");
    let bindings = desc.wgsl_bindings().expect("WGSL bindings").clone();

    let Some((device, queue)) = pollster::block_on(open_device()) else {
        eprintln!("No GPU adapter available.");
        std::process::exit(2);
    };

    let runner = Runner::new(&device, &queue, &desc, &bindings);
    let pixels = test_pixels();

    let gpu_exposure = desc
        .dynamic_property(DynamicPropertyType::Exposure)
        .expect("exposure property");
    let cpu_exposure = cpu
        .dynamic_property(DynamicPropertyType::Exposure)
        .expect("exposure property");

    let mut worst = 0u32;
    for exposure in [-2.0, 0.0, 1.5, 4.0] {
        // Update the dynamic property: only the uniform buffer changes.
        gpu_exposure.as_double().unwrap().set(exposure);
        cpu_exposure.as_double().unwrap().set(exposure);
        let uniforms = bindings.uniform_buffer(&desc).expect("uniform buffer");
        let out = runner.run(&device, &queue, &uniforms, &pixels);

        let mut max_codes = 0u32;
        let mut non_finite = 0;
        for (p, g) in pixels.iter().zip(&out) {
            let mut c = *p;
            cpu.apply_rgba(&mut c);
            for k in 0..3 {
                if !c[k].is_finite() {
                    continue;
                }
                if !g[k].is_finite() {
                    non_finite += 1;
                    continue;
                }
                let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as i32;
                max_codes = max_codes.max(q(c[k]).abs_diff(q(g[k])));
            }
        }
        println!(
            "exposure {exposure:+}: max difference {max_codes} code value(s), {non_finite} non-finite GPU value(s)"
        );
        worst = worst.max(max_codes);
        if non_finite > 0 {
            worst = u32::MAX;
        }
    }
    if worst > 2 {
        eprintln!("The GPU results differ from the CPU results.");
        std::process::exit(1);
    }
}

async fn open_device() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::default();
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions::default())
        .await
        .ok()?;
    adapter
        .request_device(&wgpu::DeviceDescriptor::default())
        .await
        .ok()
}

/// The bind group layout entries of the OCIO resources.
pub fn layout_entries(bindings: &WgslBindings) -> Vec<wgpu::BindGroupLayoutEntry> {
    let mut entries = Vec::new();
    if let Some(binding) = bindings.uniform_binding {
        entries.push(wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new(u64::from(bindings.uniform_buffer_size)),
            },
            count: None,
        });
    }
    for t in &bindings.textures {
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: t.binding,
            visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Texture {
                // The shader only uses textureLoad: no float32 filtering.
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: view_dimension(t.dimensions),
                multisampled: false,
            },
            count: None,
        });
    }
    entries
}

fn view_dimension(dimensions: u32) -> wgpu::TextureViewDimension {
    match dimensions {
        1 => wgpu::TextureViewDimension::D1,
        2 => wgpu::TextureViewDimension::D2,
        _ => wgpu::TextureViewDimension::D3,
    }
}

struct Runner {
    pipeline: wgpu::ComputePipeline,
    ocio_layout: wgpu::BindGroupLayout,
    io_layout: wgpu::BindGroupLayout,
    uniform_buffer: Option<wgpu::Buffer>,
    texture_views: Vec<(u32, wgpu::TextureView)>,
}

impl Runner {
    fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        desc: &GpuShaderDesc,
        bindings: &WgslBindings,
    ) -> Self {
        let source = format!(
            "{}\n\
             @group({IO_GROUP}) @binding(0) var<storage, read> ocio_input: array<vec4<f32>>;\n\
             @group({IO_GROUP}) @binding(1) var<storage, read_write> ocio_output: array<vec4<f32>>;\n\
             @compute @workgroup_size(64)\n\
             fn main(@builtin(global_invocation_id) id: vec3<u32>) {{\n\
             \x20   let i = id.x;\n\
             \x20   if (i < arrayLength(&ocio_input)) {{\n\
             \x20       ocio_output[i] = {}(ocio_input[i]);\n\
             \x20   }}\n\
             }}\n",
            desc.shader_text(),
            desc.function_name()
        );
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ocio"),
            source: wgpu::ShaderSource::Wgsl(Cow::Owned(source)),
        });

        let ocio_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ocio resources"),
            entries: &layout_entries(bindings),
        });
        let storage = |binding, read_only| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let io_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("pixels"),
            entries: &[storage(0, true), storage(1, false)],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&ocio_layout), Some(&io_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("ocio"),
            layout: Some(&pipeline_layout),
            module: &module,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        let uniform_buffer = bindings.uniform_binding.map(|_| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("ocio uniforms"),
                size: u64::from(bindings.uniform_buffer_size),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });

        // Upload the LUT textures once.
        let mut values: HashMap<&str, &[f32]> = HashMap::new();
        for i in 0..desc.num_textures() {
            let t = desc.texture(i).unwrap();
            values.insert(&t.texture_name, &t.values);
        }
        for i in 0..desc.num_3d_textures() {
            let t = desc.texture_3d(i).unwrap();
            values.insert(&t.texture_name, &t.values);
        }
        let mut texture_views = Vec::new();
        for t in &bindings.textures {
            let size = wgpu::Extent3d {
                width: t.width,
                height: t.height,
                depth_or_array_layers: t.depth,
            };
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some(&t.texture_name),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: match t.dimensions {
                    1 => wgpu::TextureDimension::D1,
                    2 => wgpu::TextureDimension::D2,
                    _ => wgpu::TextureDimension::D3,
                },
                format: match t.format {
                    WgslTextureFormat::R32Float => wgpu::TextureFormat::R32Float,
                    WgslTextureFormat::Rgba32Float => wgpu::TextureFormat::Rgba32Float,
                },
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let data = t
                .texel_data(values[t.texture_name.as_str()])
                .expect("texel data");
            queue.write_texture(
                texture.as_image_copy(),
                &data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(t.bytes_per_row()),
                    rows_per_image: Some(t.height),
                },
                size,
            );
            let view = texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(view_dimension(t.dimensions)),
                ..Default::default()
            });
            texture_views.push((t.binding, view));
        }

        Runner {
            pipeline,
            ocio_layout,
            io_layout,
            uniform_buffer,
            texture_views,
        }
    }

    fn run(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        uniforms: &[u8],
        pixels: &[[f32; 4]],
    ) -> Vec<[f32; 4]> {
        if let Some(b) = &self.uniform_buffer {
            queue.write_buffer(b, 0, uniforms);
        }
        let mut entries = Vec::new();
        if let Some(b) = &self.uniform_buffer {
            entries.push(wgpu::BindGroupEntry {
                binding: 0,
                resource: b.as_entire_binding(),
            });
        }
        for (binding, view) in &self.texture_views {
            entries.push(wgpu::BindGroupEntry {
                binding: *binding,
                resource: wgpu::BindingResource::TextureView(view),
            });
        }
        let ocio_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.ocio_layout,
            entries: &entries,
        });

        let bytes: Vec<u8> = pixels
            .iter()
            .flatten()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let size = bytes.len() as u64;
        let input = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&input, 0, &bytes);
        let output = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let io_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.io_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: input.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: output.as_entire_binding(),
                },
            ],
        });

        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &ocio_group, &[]);
            pass.set_bind_group(IO_GROUP, &io_group, &[]);
            pass.dispatch_workgroups((pixels.len() as u32).div_ceil(64), 1, 1);
        }
        encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, size);
        queue.submit([encoder.finish()]);

        let slice = readback.slice(..);
        slice.map_async(wgpu::MapMode::Read, |r| r.expect("map"));
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("poll");
        let data = slice.get_mapped_range().expect("mapped range");
        let out = data
            .as_chunks::<16>()
            .0
            .iter()
            .map(|c| {
                let f = |i: usize| f32::from_le_bytes(c[4 * i..4 * i + 4].try_into().unwrap());
                [f(0), f(1), f(2), f(3)]
            })
            .collect();
        drop(data);
        readback.unmap();
        out
    }
}
