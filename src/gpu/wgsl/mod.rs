//! WGSL shader generation, for `wgpu` / WebGPU clients.
//!
//! With [`GpuLanguage::Wgsl`](crate::GpuLanguage::Wgsl), the shader text of a
//! [`GpuShaderDesc`] is a WGSL module defining the function
//!
//! ```wgsl
//! fn OCIOMain(inPixel: vec4<f32>) -> vec4<f32>
//! ```
//!
//! (named after
//! [`GpuShaderCreator::function_name`](crate::gpu::GpuShaderCreator::function_name))
//! plus the resources it reads, described by
//! [`GpuShaderCreator::wgsl_bindings`](crate::gpu::GpuShaderCreator::wgsl_bindings):
//!
//! * **One uniform buffer** at `@binding(0)` holding all the uniforms
//!   (the dynamic properties), when there are any. Its layout follows the
//!   WGSL uniform address space rules (identical to std140): `f32` / `i32`
//!   members are 4 bytes aligned on 4, `vec3<f32>` are 12 bytes aligned on
//!   16 and arrays use a 16 bytes stride (`array<vec4<f32>, N>`, the value
//!   being in `.x`). Bool uniforms are `i32` (0 or 1).
//!   [`WgslBindings::write_uniform_buffer`] packs the current values of the
//!   uniforms: update the buffer after changing a dynamic property, without
//!   regenerating the shader or the LUT textures.
//! * **One texture per LUT** at the binding returned when the texture was
//!   added (starting at
//!   [`GpuShaderCreator::texture_binding_start`](crate::gpu::GpuShaderCreator::texture_binding_start),
//!   1 by default). Textures are `R32Float` (single channel LUTs) or
//!   `Rgba32Float` (use [`WgslTextureBinding::texel_data`] to expand the RGB
//!   values). They are read with `textureLoad` only, the linear or nearest
//!   interpolation being computed in the shader: no sampler is bound and
//!   the textures do not need to be filterable (no `FLOAT32_FILTERABLE`
//!   feature), so the bind group layout entries use
//!   `TextureSampleType::Float { filterable: false }`.
//!
//! All resources use the group
//! [`GpuShaderCreator::descriptor_set_index`](crate::gpu::GpuShaderCreator::descriptor_set_index)
//! (0 by default). Run several OCIO shaders in one module by giving each
//! shader creator its own function name and resource prefix.
//!
//! # Accuracy
//!
//! The renderers of the ops write their shader code in GLSL 4.0, which is
//! lowered to WGSL once the program is complete, so the WGSL shaders compute
//! what the shaders of the other languages compute, except that the texture
//! interpolation is exact (no 8 bits quantization of the hardware filtering
//! weights). Nothing is baked: the analytic ops stay analytic over an
//! unbounded HDR input. As for every shading language, the LUT inverses are
//! the fast (baked) ones of OCIO.
//!
//! The tests run every generated module (the builtin transforms, the LUT /
//! CLF / CTF test files, the grading ops and the fixed functions in both
//! directions, and the display / view processors of the latest builtin
//! configs, ACES 2.0 included) on negative, black, near zero, diffuse,
//! saturated and scene-linear HDR pixels, and compare them with the CPU
//! processor optimized with the same LUT inverses: the results are within
//! 1e-3 relative (1e-3 absolute below 1), i.e. well within one 8 bits code
//! value. On a GPU, the precision of the built-in functions (`pow`, `log`,
//! `exp`, ...) of the hardware adds its own differences.
//!
//! # Non-finite values
//!
//! NaN and infinities propagate as in IEEE arithmetic (WGSL implementations
//! may assume finite values in some built-ins, `min` / `max` of a NaN
//! usually return the other operand). LUT texture coordinates are clamped
//! to the texture edges, so an infinite input reads the first or last LUT
//! entry; a NaN coordinate reads an unspecified entry. The shaders inherit
//! the behavior of the OCIO GPU code: e.g. the ACES 0.3 / 1.0 glow fixed
//! functions give NaN for an exactly black pixel on every shading language
//! (`0 * inf`), where the CPU gives 0.
//!
//! # Errors
//!
//! A program the lowering cannot translate exactly (e.g. an unknown built-in
//! function or texture) makes
//! [`GpuProcessor::extract_gpu_shader_info`](crate::gpu::GpuProcessor::extract_gpu_shader_info)
//! return an error: no approximate shader is ever produced.
//!
//! ```
//! use ocio::gpu::{GpuShaderCreator, GpuShaderDesc};
//! use ocio::{Config, ExposureContrastTransform, GpuLanguage, Transform, TransformDirection};
//!
//! let config = Config::create_raw();
//! let mut ec = ExposureContrastTransform::default();
//! ec.exposure_dynamic = true;
//! let processor = config
//!     .get_processor_for_transform(&Transform::ExposureContrast(ec), TransformDirection::Forward)
//!     .unwrap();
//! let gpu = processor.default_gpu_processor().unwrap();
//!
//! let mut desc = GpuShaderDesc::new();
//! desc.set_language(GpuLanguage::Wgsl);
//! gpu.extract_gpu_shader_info(&mut desc).unwrap();
//! assert!(desc.shader_text().contains("fn OCIOMain(inPixel: vec4<f32>) -> vec4<f32>"));
//!
//! let bindings = desc.wgsl_bindings().unwrap();
//! assert_eq!(bindings.uniform_binding, Some(0));
//! let mut buffer = vec![0u8; bindings.uniform_buffer_size as usize];
//! bindings.write_uniform_buffer(&desc, &mut buffer).unwrap();
//! ```

pub(crate) mod glsl;
pub(crate) mod lower;

use crate::error::{Error, Result};
use crate::types::Interpolation;

use super::{GpuShaderDesc, UniformValue};

pub(crate) use lower::TextureSource;

/// Type of a member of the uniform buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WgslUniformType {
    /// `f32` (a double uniform converted to float).
    Float,
    /// `i32`.
    Int,
    /// `i32` holding a bool (0 or 1).
    Bool,
    /// `vec3<f32>`.
    Float3,
    /// `array<vec4<f32>, N>`: one float per 16 bytes element (in `.x`).
    FloatArray(u32),
    /// `array<vec4<i32>, N>`: one int per 16 bytes element (in `.x`).
    IntArray(u32),
}

/// A member of the uniform buffer.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WgslUniform {
    /// The uniform name (as given to the shader creator).
    pub name: String,
    /// Its type.
    pub ty: WgslUniformType,
    /// Offset in bytes from the start of the buffer.
    pub offset: u32,
    /// Size in bytes.
    pub size: u32,
}

/// Format of a LUT texture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WgslTextureFormat {
    /// One 32 bits float channel (`wgpu::TextureFormat::R32Float`).
    R32Float,
    /// Four 32 bits float channels (`wgpu::TextureFormat::Rgba32Float`), the
    /// alpha channel being unused.
    Rgba32Float,
}

impl WgslTextureFormat {
    /// Size in bytes of a texel.
    pub fn bytes_per_texel(&self) -> u32 {
        match self {
            WgslTextureFormat::R32Float => 4,
            WgslTextureFormat::Rgba32Float => 16,
        }
    }
}

/// A LUT texture of the WGSL module.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WgslTextureBinding {
    /// The texture name (the WGSL variable), as in [`super::GpuTexture`].
    pub texture_name: String,
    /// The sampler name of the texture in the shader creator (no WGSL
    /// sampler is declared).
    pub sampler_name: String,
    /// Bind group index.
    pub group: u32,
    /// Binding index in the group.
    pub binding: u32,
    /// Number of dimensions: 1 (`texture_1d<f32>`), 2 (`texture_2d<f32>`)
    /// or 3 (`texture_3d<f32>`).
    pub dimensions: u32,
    /// Width in texels.
    pub width: u32,
    /// Height in texels (1 for a 1D texture).
    pub height: u32,
    /// Depth in texels (1 for 1D and 2D textures).
    pub depth: u32,
    /// The texel format.
    pub format: WgslTextureFormat,
    /// The interpolation computed by the shader (nearest or linear).
    pub interpolation: Interpolation,
}

impl WgslTextureBinding {
    /// The texel data to upload, as little endian bytes in the layout of
    /// [`format`](Self::format) (RGB values are expanded to RGBA with an
    /// alpha of 1), rows of `width` texels, `height` rows per slice.
    ///
    /// `values` are the texture values of the shader description
    /// ([`GpuShaderDesc::texture_values`] or
    /// [`GpuShaderDesc::texture_3d_values`]).
    pub fn texel_data(&self, values: &[f32]) -> Result<Vec<u8>> {
        let texels = self.width as usize * self.height as usize * self.depth as usize;
        let mut out = Vec::with_capacity(texels * self.format.bytes_per_texel() as usize);
        match self.format {
            WgslTextureFormat::R32Float => {
                if values.len() < texels {
                    return Err(Error::msg(format!(
                        "The texture '{}' needs {texels} values, {} given.",
                        self.texture_name,
                        values.len()
                    )));
                }
                for v in &values[..texels] {
                    out.extend_from_slice(&v.to_le_bytes());
                }
            }
            WgslTextureFormat::Rgba32Float => {
                if values.len() < texels * 3 {
                    return Err(Error::msg(format!(
                        "The texture '{}' needs {} values, {} given.",
                        self.texture_name,
                        texels * 3,
                        values.len()
                    )));
                }
                for rgb in values[..texels * 3].as_chunks::<3>().0 {
                    for v in rgb.iter().chain(std::iter::once(&1.0f32)) {
                        out.extend_from_slice(&v.to_le_bytes());
                    }
                }
            }
        }
        Ok(out)
    }

    /// Number of bytes per row of texels (for the texture upload).
    pub fn bytes_per_row(&self) -> u32 {
        self.width * self.format.bytes_per_texel()
    }
}

/// The resources of a WGSL module (see the [module documentation](self)).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct WgslBindings {
    /// Bind group index of all the resources.
    pub group: u32,
    /// Binding of the uniform buffer, `None` without uniforms.
    pub uniform_binding: Option<u32>,
    /// Size in bytes of the uniform buffer (a multiple of 16, 0 without
    /// uniforms).
    pub uniform_buffer_size: u32,
    /// The members of the uniform buffer, in order.
    pub uniforms: Vec<WgslUniform>,
    /// The LUT textures.
    pub textures: Vec<WgslTextureBinding>,
}

impl WgslBindings {
    /// The member of the uniform buffer named `name`.
    pub fn uniform(&self, name: &str) -> Option<&WgslUniform> {
        self.uniforms.iter().find(|u| u.name == name)
    }

    /// The texture named `texture_name`.
    pub fn texture(&self, texture_name: &str) -> Option<&WgslTextureBinding> {
        self.textures
            .iter()
            .find(|t| t.texture_name == texture_name)
    }

    /// Write the current values of the uniforms of `desc` into `buffer`
    /// (little endian, at least [`uniform_buffer_size`](Self::uniform_buffer_size)
    /// bytes). Call it again after changing a dynamic property.
    pub fn write_uniform_buffer(&self, desc: &GpuShaderDesc, buffer: &mut [u8]) -> Result<()> {
        if buffer.len() < self.uniform_buffer_size as usize {
            return Err(Error::msg(format!(
                "The uniform buffer is too small: {} < {} bytes.",
                buffer.len(),
                self.uniform_buffer_size
            )));
        }
        for u in &self.uniforms {
            let data = (0..desc.num_uniforms())
                .filter_map(|i| desc.uniform(i).ok())
                .find(|(name, _)| *name == u.name)
                .map(|(_, d)| d)
                .ok_or_else(|| Error::msg(format!("Unknown uniform '{}'.", u.name)))?;
            let at = u.offset as usize;
            let put = |buffer: &mut [u8], off: usize, bytes: [u8; 4]| {
                buffer[off..off + 4].copy_from_slice(&bytes);
            };
            match (&data.value, u.ty) {
                (UniformValue::Double(g), WgslUniformType::Float) => {
                    put(buffer, at, (g() as f32).to_le_bytes());
                }
                (UniformValue::Bool(g), WgslUniformType::Bool) => {
                    put(buffer, at, i32::from(g()).to_le_bytes());
                }
                (UniformValue::Float3(g), WgslUniformType::Float3) => {
                    for (k, v) in g().iter().enumerate() {
                        put(buffer, at + 4 * k, v.to_le_bytes());
                    }
                }
                (UniformValue::VectorFloat(size, g), WgslUniformType::FloatArray(n)) => {
                    let values = g();
                    let used = (size().max(0) as usize).min(values.len()).min(n as usize);
                    buffer[at..at + u.size as usize].fill(0);
                    for (k, v) in values[..used].iter().enumerate() {
                        put(buffer, at + 16 * k, v.to_le_bytes());
                    }
                }
                (UniformValue::VectorInt(size, g), WgslUniformType::IntArray(n)) => {
                    let values = g();
                    let used = (size().max(0) as usize).min(values.len()).min(n as usize);
                    buffer[at..at + u.size as usize].fill(0);
                    for (k, v) in values[..used].iter().enumerate() {
                        put(buffer, at + 16 * k, v.to_le_bytes());
                    }
                }
                (v, t) => {
                    return Err(Error::msg(format!(
                        "The uniform '{}' ({v:?}) does not match its WGSL type {t:?}.",
                        u.name
                    )))
                }
            }
        }
        Ok(())
    }

    /// The uniform buffer holding the current values of the uniforms of
    /// `desc` (see [`write_uniform_buffer`](Self::write_uniform_buffer)).
    pub fn uniform_buffer(&self, desc: &GpuShaderDesc) -> Result<Vec<u8>> {
        let mut buffer = vec![0u8; self.uniform_buffer_size as usize];
        self.write_uniform_buffer(desc, &mut buffer)?;
        Ok(buffer)
    }
}

/// Lower the complete GLSL 4.0 program to a WGSL module.
pub(crate) fn translate(
    source: &str,
    function_name: &str,
    group: u32,
    textures: &[TextureSource],
) -> Result<(String, WgslBindings)> {
    let input = lower::LowerInput {
        source,
        function_name,
        group,
        textures,
    };
    let (body, bindings) = lower::lower(&input)?;
    let mut code = lower::bindings_comment(&bindings);
    code.push('\n');
    code.push_str(&body);
    Ok((code, bindings))
}
