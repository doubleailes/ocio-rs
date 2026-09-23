//! GPU shader generation (port of `GpuShaderDesc.cpp`, `GpuShader.cpp`,
//! `GPUProcessor.cpp`, `GpuShaderUtils.cpp`, `GpuShaderClassWrapper.cpp` and
//! the `*OpGPU.cpp` renderers).
//!
//! A [`GpuProcessor`] (obtained from
//! [`Processor::default_gpu_processor`](crate::Processor::default_gpu_processor))
//! fills a [`GpuShaderCreator`] with the shader program implementing the
//! color transformation, plus the textures (LUTs) and uniforms (dynamic
//! properties) it needs. [`GpuShaderDesc`] is the generic implementation of
//! the creator, collecting everything so that a client can build and feed
//! the shader program:
//!
//! ```
//! use ocio::gpu::{GpuShaderCreator, GpuShaderDesc};
//! use ocio::{Config, GpuLanguage, MatrixTransform, Transform, TransformDirection};
//!
//! let config = Config::create_raw();
//! let mut m = MatrixTransform::default();
//! m.offset = [0.1, 0.2, 0.3, 0.0];
//! let processor = config
//!     .get_processor_for_transform(&Transform::Matrix(m), TransformDirection::Forward)
//!     .unwrap();
//! let gpu = processor.default_gpu_processor().unwrap();
//!
//! let mut desc = GpuShaderDesc::new();
//! desc.set_language(GpuLanguage::Glsl4_0);
//! desc.set_function_name("OCIODisplay");
//! gpu.extract_gpu_shader_info(&mut desc).unwrap();
//! assert!(desc.shader_text().contains("vec4 OCIODisplay(vec4 inPixel)"));
//! ```

mod class_wrapper;
mod op_gpu;
pub mod processor;
pub mod shader_text;

pub use op_gpu::{
    default_extract_gpu_shader_info, extract_op_gpu_shader_info, supported_by_legacy_shader,
};
pub use processor::GpuProcessor;
pub use shader_text::GpuShaderText;

use crate::config::logging::{log_debug, logging_level};
use crate::dynamic_property::DynamicProperty;
use crate::error::{Error, Result};
use crate::types::{DynamicPropertyType, GpuLanguage, Interpolation, LoggingLevel};
use class_wrapper::ClassWrapper;
use std::fmt;
use std::sync::Arc;

/// Maximum edge length of a 3D LUT texture (port of `Max3DLUTLength`).
pub const MAX_3D_LUT_LENGTH: u32 = 129;

/// Hash of a string as OCIO's `CacheIDHash` (128 bits XXH3, low then high
/// 64 bits in hexadecimal).
pub(crate) fn cache_id_hash(s: &str) -> String {
    let h = xxhash_rust::xxh3::xxh3_128(s.as_bytes());
    format!("{:016x}{:016x}", h as u64, (h >> 64) as u64)
}

/// Function returning a double, used by uniforms. GPU converts double to
/// float.
pub type DoubleGetter = Arc<dyn Fn() -> f64 + Send + Sync>;
/// Function returning a bool, used by uniforms.
pub type BoolGetter = Arc<dyn Fn() -> bool + Send + Sync>;
/// Function returning three floats, used by uniforms.
pub type Float3Getter = Arc<dyn Fn() -> [f32; 3] + Send + Sync>;
/// Function returning a size, used by array uniforms.
pub type SizeGetter = Arc<dyn Fn() -> i32 + Send + Sync>;
/// Function returning floats, used by array uniforms.
pub type VectorFloatGetter = Arc<dyn Fn() -> Vec<f32> + Send + Sync>;
/// Function returning ints, used by array uniforms.
pub type VectorIntGetter = Arc<dyn Fn() -> Vec<i32> + Send + Sync>;

/// Types of uniforms (port of `UniformDataType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UniformDataType {
    /// A double (converted to float by the GPU).
    Double,
    /// A bool.
    Bool,
    /// Array of 3 floats.
    Float3,
    /// Vector of floats (size is set by uniform).
    VectorFloat,
    /// Vector of int pairs (size is set by uniform).
    VectorInt,
    /// Unknown.
    Unknown,
}

/// The value getter of a uniform.
#[derive(Clone)]
pub enum UniformValue {
    /// A double value.
    Double(DoubleGetter),
    /// A bool value.
    Bool(BoolGetter),
    /// Three floats.
    Float3(Float3Getter),
    /// A vector of floats (the size getter gives the used length).
    VectorFloat(SizeGetter, VectorFloatGetter),
    /// A vector of ints (the size getter gives the used length).
    VectorInt(SizeGetter, VectorIntGetter),
}

impl fmt::Debug for UniformValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UniformValue::Double(g) => write!(f, "Double({})", g()),
            UniformValue::Bool(g) => write!(f, "Bool({})", g()),
            UniformValue::Float3(g) => write!(f, "Float3({:?})", g()),
            UniformValue::VectorFloat(s, _) => write!(f, "VectorFloat(size {})", s()),
            UniformValue::VectorInt(s, _) => write!(f, "VectorInt(size {})", s()),
        }
    }
}

/// Uniform information (port of `GpuShaderDesc::UniformData`).
#[derive(Debug, Clone)]
pub struct UniformData {
    /// The value getter (its variant is the uniform type).
    pub value: UniformValue,
    /// Offset in bytes from the start of the uniform buffer (for the shading
    /// languages using uniform buffers).
    pub buffer_offset: usize,
}

impl UniformData {
    /// The type of the uniform.
    pub fn data_type(&self) -> UniformDataType {
        match self.value {
            UniformValue::Double(_) => UniformDataType::Double,
            UniformValue::Bool(_) => UniformDataType::Bool,
            UniformValue::Float3(_) => UniformDataType::Float3,
            UniformValue::VectorFloat(..) => UniformDataType::VectorFloat,
            UniformValue::VectorInt(..) => UniformDataType::VectorInt,
        }
    }
}

/// Channels of a texture (port of `GpuShaderCreator::TextureType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextureType {
    /// Only needs a red channel texture.
    RedChannel,
    /// Needs a RGB texture.
    RgbChannel,
}

/// Dimensions of a texture built from a 1D LUT (port of
/// `GpuShaderCreator::TextureDimensions`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextureDimensions {
    /// A 1D texture.
    Texture1D = 1,
    /// A 2D texture.
    Texture2D = 2,
}

/// Common state of every [`GpuShaderCreator`] (port of
/// `GpuShaderCreator::Impl`).
#[derive(Debug, Clone)]
pub struct GpuShaderCreatorBase {
    uid: String,
    language: GpuLanguage,
    function_name: String,
    resource_prefix: String,
    pixel_name: String,
    num_resources: u32,

    parameter_declarations: String,
    texture_declarations: String,
    helper_methods: String,
    function_header: String,
    function_body: String,
    function_footer: String,

    shader_code: String,
    shader_code_id: String,

    dynamic_properties: Vec<DynamicProperty>,

    class_wrapper: ClassWrapper,

    descriptor_set_index: u32,
    texture_binding_start: u32,
}

impl Default for GpuShaderCreatorBase {
    fn default() -> Self {
        let language = GpuLanguage::Glsl1_2;
        Self {
            uid: String::new(),
            language,
            function_name: "OCIOMain".to_string(),
            resource_prefix: "ocio".to_string(),
            pixel_name: "outColor".to_string(),
            num_resources: 0,
            parameter_declarations: String::new(),
            texture_declarations: String::new(),
            helper_methods: String::new(),
            function_header: String::new(),
            function_body: String::new(),
            function_footer: String::new(),
            shader_code: String::new(),
            shader_code_id: String::new(),
            dynamic_properties: Vec::new(),
            class_wrapper: ClassWrapper::create(language),
            descriptor_set_index: 0,
            texture_binding_start: 1,
        }
    }
}

impl GpuShaderCreatorBase {
    /// A new state with the OCIO default values (GLSL 1.2, `OCIOMain`,
    /// `ocio` and `outColor`).
    pub fn new() -> Self {
        Self::default()
    }

    /// Copy of the state as done by `GpuShaderDesc::clone` (the dynamic
    /// properties and the generated shader code are not copied).
    pub fn clone_for_creator(&self) -> Self {
        let mut c = self.clone();
        c.dynamic_properties.clear();
        c.shader_code.clear();
        c.shader_code_id.clear();
        c
    }

    /// The complete shader program (empty until finalized).
    pub fn shader_text(&self) -> &str {
        &self.shader_code
    }
}

/// Remove the potentially problematic double underscores from GLSL resource
/// names.
fn remove_double_underscores(s: &str) -> String {
    s.replace("__", "_")
}

/// Interface of the objects collecting the GPU shader information of a
/// processor (port of `GpuShaderCreator`).
///
/// The shared state lives in a [`GpuShaderCreatorBase`]; implementations
/// only provide the resource management (textures and uniforms) and may
/// override the provided methods (as the virtual methods of the C++ class).
pub trait GpuShaderCreator {
    /// The common state.
    fn base(&self) -> &GpuShaderCreatorBase;
    /// The common state (mutable).
    fn base_mut(&mut self) -> &mut GpuShaderCreatorBase;

    /// A copy of the creator (the resources and the dynamic properties are
    /// not copied).
    fn clone_creator(&self) -> Box<dyn GpuShaderCreator>;

    /// The unique id, if any.
    fn unique_id(&self) -> &str {
        &self.base().uid
    }

    /// Set a unique id prepended to the resource key given to `begin`.
    fn set_unique_id(&mut self, uid: &str) {
        self.base_mut().uid = uid.to_string();
    }

    /// The shading language.
    fn language(&self) -> GpuLanguage {
        self.base().language
    }

    /// Set the shading language.
    fn set_language(&mut self, lang: GpuLanguage) {
        let b = self.base_mut();
        b.language = lang;
        b.class_wrapper = ClassWrapper::create(lang);
    }

    /// The name of the shader function.
    fn function_name(&self) -> &str {
        &self.base().function_name
    }

    /// Set the name of the shader function.
    fn set_function_name(&mut self, name: &str) {
        self.base_mut().function_name = remove_double_underscores(name);
    }

    /// The name of the variable holding the color values.
    fn pixel_name(&self) -> &str {
        &self.base().pixel_name
    }

    /// Set the name of the variable holding the color values.
    fn set_pixel_name(&mut self, name: &str) {
        self.base_mut().pixel_name = remove_double_underscores(name);
    }

    /// The prefix of the resource names (textures, uniforms and helper
    /// methods), as several processors could coexist in one program.
    fn resource_prefix(&self) -> &str {
        &self.base().resource_prefix
    }

    /// Set the prefix of the resource names.
    fn set_resource_prefix(&mut self, prefix: &str) {
        self.base_mut().resource_prefix = remove_double_underscores(prefix);
    }

    /// Set the descriptor set index and the texture binding start index
    /// (only used by the shading languages using descriptor sets, such as
    /// Vulkan). The binding 0 is reserved for the uniform buffer.
    fn set_descriptor_set_index(&mut self, index: u32, texture_binding_start: u32) -> Result<()> {
        if texture_binding_start == 0 {
            return Err(Error::msg(
                "Texture binding start index must be greater than 0.",
            ));
        }
        let b = self.base_mut();
        b.descriptor_set_index = index;
        b.texture_binding_start = texture_binding_start;
        Ok(())
    }

    /// The descriptor set index.
    fn descriptor_set_index(&self) -> u32 {
        self.base().descriptor_set_index
    }

    /// The texture binding start index.
    fn texture_binding_start(&self) -> u32 {
        self.base().texture_binding_start
    }

    /// Identifier of the shader program.
    fn cache_id(&self) -> String {
        let b = self.base();
        format!(
            "{} {} {} {} {} {} {} {}",
            b.language.as_str(),
            b.function_name,
            b.resource_prefix,
            b.pixel_name,
            b.num_resources,
            b.descriptor_set_index,
            b.texture_binding_start,
            b.shader_code_id
        )
    }

    /// Start to collect the shader data.
    fn begin(&mut self, _uid: &str) {}

    /// End to collect the shader data.
    fn end(&mut self) {}

    /// Set the maximum width of the 1D & 2D textures.
    fn set_texture_max_width(&mut self, max_width: u32);
    /// The maximum width of the 1D & 2D textures.
    fn texture_max_width(&self) -> u32;

    /// Allow 1D GPU resources, otherwise 2D resources are always used for 1D
    /// LUTs.
    fn set_allow_texture_1d(&mut self, allowed: bool);
    /// True if 1D GPU resources are allowed.
    fn allow_texture_1d(&self) -> bool;

    /// An increasing index appended to the resource names to avoid clashes.
    fn next_resource_index(&mut self) -> u32 {
        let b = self.base_mut();
        let i = b.num_resources;
        b.num_resources += 1;
        i
    }

    /// Add a double uniform. Returns false if the name is already used.
    fn add_uniform_double(&mut self, name: &str, getter: DoubleGetter) -> Result<bool>;
    /// Add a bool uniform. Returns false if the name is already used.
    fn add_uniform_bool(&mut self, name: &str, getter: BoolGetter) -> Result<bool>;
    /// Add a three floats uniform. Returns false if the name is already used.
    fn add_uniform_float3(&mut self, name: &str, getter: Float3Getter) -> Result<bool>;
    /// Add a float array uniform; `max_size` is the size of the array
    /// declared in the shader. Returns false if the name is already used.
    fn add_uniform_vector_float(
        &mut self,
        name: &str,
        get_size: SizeGetter,
        getter: VectorFloatGetter,
        max_size: u32,
    ) -> Result<bool>;
    /// Add an int array uniform; `max_size` is the size of the array
    /// declared in the shader. Returns false if the name is already used.
    fn add_uniform_vector_int(
        &mut self,
        name: &str,
        get_size: SizeGetter,
        getter: VectorIntGetter,
        max_size: u32,
    ) -> Result<bool>;

    /// Add a dynamic property (used internally): the client changes its
    /// value to update the uniforms.
    fn add_dynamic_property(&mut self, prop: DynamicProperty) -> Result<()> {
        let ty = prop.property_type();
        if self.has_dynamic_property(ty) {
            return Err(Error::msg(format!(
                "Dynamic property already here: {}.",
                ty as i32
            )));
        }
        self.base_mut().dynamic_properties.push(prop);
        Ok(())
    }

    /// Number of dynamic properties.
    fn num_dynamic_properties(&self) -> usize {
        self.base().dynamic_properties.len()
    }

    /// The dynamic property at `index`.
    fn dynamic_property_at(&self, index: usize) -> Result<DynamicProperty> {
        let props = &self.base().dynamic_properties;
        props.get(index).cloned().ok_or_else(|| {
            Error::msg(format!(
                "Dynamic properties access error: index = {} where size = {}",
                index,
                props.len()
            ))
        })
    }

    /// True if a dynamic property of the type is held.
    fn has_dynamic_property(&self, ty: DynamicPropertyType) -> bool {
        self.base()
            .dynamic_properties
            .iter()
            .any(|p| p.property_type() == ty)
    }

    /// The dynamic property of the type. Changing its value changes the
    /// uniforms of the shader program.
    fn dynamic_property(&self, ty: DynamicPropertyType) -> Result<DynamicProperty> {
        self.base()
            .dynamic_properties
            .iter()
            .find(|p| p.property_type() == ty)
            .cloned()
            .ok_or_else(|| Error::msg("Dynamic property not found."))
    }

    /// Add a 1D or 2D texture. `values` contains the LUT data which must be
    /// used as-is. Returns the shader binding index of the texture.
    #[allow(clippy::too_many_arguments)]
    fn add_texture(
        &mut self,
        texture_name: &str,
        sampler_name: &str,
        width: u32,
        height: u32,
        channel: TextureType,
        dimensions: TextureDimensions,
        interpolation: Interpolation,
        values: &[f32],
    ) -> Result<u32>;

    /// Add a 3D texture with RGB channels. Returns the shader binding index
    /// of the texture.
    fn add_3d_texture(
        &mut self,
        texture_name: &str,
        sampler_name: &str,
        edgelen: u32,
        interpolation: Interpolation,
        values: &[f32],
    ) -> Result<u32>;

    /// Add code to the declarations of the uniforms.
    fn add_to_parameter_declare_shader_code(&mut self, shader_code: &str) {
        let b = self.base_mut();
        if b.parameter_declarations.is_empty() {
            b.parameter_declarations
                .push_str("\n// Declaration of all variables\n\n");
        }
        b.parameter_declarations.push_str(shader_code);
    }

    /// Add code to the declarations of the textures.
    fn add_to_texture_declare_shader_code(&mut self, shader_code: &str) {
        let b = self.base_mut();
        if b.texture_declarations.is_empty() {
            b.texture_declarations
                .push_str("\n// Declaration of all textures\n\n");
        }
        b.texture_declarations.push_str(shader_code);
    }

    /// Add code to the helper methods.
    fn add_to_helper_shader_code(&mut self, shader_code: &str) {
        let b = self.base_mut();
        if b.helper_methods.is_empty() {
            b.helper_methods
                .push_str("\n// Declaration of all helper methods\n\n");
        }
        b.helper_methods.push_str(shader_code);
    }

    /// Add code to the function header.
    fn add_to_function_header_shader_code(&mut self, shader_code: &str) {
        self.base_mut().function_header.push_str(shader_code);
    }

    /// Add code to the function body.
    fn add_to_function_shader_code(&mut self, shader_code: &str) {
        self.base_mut().function_body.push_str(shader_code);
    }

    /// Add code to the function footer.
    fn add_to_function_footer_shader_code(&mut self, shader_code: &str) {
        self.base_mut().function_footer.push_str(shader_code);
    }

    /// Create the complete shader program from its parts (implementations
    /// may reorganize them to fit a client program).
    fn create_shader_text(
        &mut self,
        parameter_declarations: &str,
        texture_declarations: &str,
        helper_methods: &str,
        function_header: &str,
        function_body: &str,
        function_footer: &str,
    ) {
        let b = self.base_mut();
        let mut code = String::new();

        let vulkan_block =
            b.language == GpuLanguage::GlslVk4_6 && !parameter_declarations.is_empty();
        if vulkan_block {
            code.push_str(&format!(
                "layout (set = {}, binding = 0) uniform {}_Parameters\n{{\n",
                b.descriptor_set_index, b.function_name
            ));
        }
        code.push_str(parameter_declarations);
        if vulkan_block {
            code.push_str("\n};\n");
        }

        code.push_str(texture_declarations);
        code.push_str(helper_methods);
        code.push_str(function_header);
        code.push_str(function_body);
        code.push_str(function_footer);

        b.shader_code_id = cache_id_hash(&code);
        b.shader_code = code;
    }

    /// Build the complete shader program (the class wrapper of the language
    /// is applied first).
    fn finalize(&mut self) -> Result<()> {
        // For some GPU languages, the default header and footer do not fit
        // well so, the class wrapper encapsulates differences when needed.
        let (params, textures, helpers, header, body, footer) = {
            let b = self.base_mut();
            let original_header =
                format!("{}\n{}", b.parameter_declarations, b.texture_declarations);
            let prefix = b.resource_prefix.clone();
            let fn_name = b.function_name.clone();
            b.class_wrapper.prepare(&prefix, &fn_name, &original_header);

            if b.class_wrapper.has_header() {
                b.parameter_declarations = b.class_wrapper.header(&original_header)?;
                // The texture declarations are already included in the header.
                b.texture_declarations.clear();
            }
            b.function_footer = b.class_wrapper.footer(&b.function_footer)?;

            (
                b.parameter_declarations.clone(),
                b.texture_declarations.clone(),
                b.helper_methods.clone(),
                b.function_header.clone(),
                b.function_body.clone(),
                b.function_footer.clone(),
            )
        };

        // Build the complete shader program.
        self.create_shader_text(&params, &textures, &helpers, &header, &body, &footer);

        if logging_level() >= LoggingLevel::Debug && logging_level() != LoggingLevel::Unknown {
            log_debug(&format!(
                "\n**\nGPU Fragment Shader program\n{}\n",
                self.base().shader_code
            ));
        }
        Ok(())
    }
}

/// A texture of a [`GpuShaderDesc`].
#[derive(Debug, Clone, PartialEq)]
pub struct GpuTexture {
    /// The texture name.
    pub texture_name: String,
    /// The sampler name.
    pub sampler_name: String,
    /// The width.
    pub width: u32,
    /// The height (1 for a 1D texture).
    pub height: u32,
    /// The depth (1 for 1D or 2D textures).
    pub depth: u32,
    /// The channels.
    pub channel: TextureType,
    /// Number of dimensions (1, 2 or 3).
    pub dimensions: u32,
    /// The interpolation.
    pub interpolation: Interpolation,
    /// Index of the texture (before adding the texture binding start).
    pub shader_binding_index: u32,
    /// The values.
    pub values: Vec<f32>,
}

impl GpuTexture {
    #[allow(clippy::too_many_arguments)]
    fn new(
        texture_name: &str,
        sampler_name: &str,
        w: u32,
        h: u32,
        d: u32,
        channel: TextureType,
        dimensions: u32,
        interpolation: Interpolation,
        shader_binding_index: u32,
        values: &[f32],
    ) -> Result<Self> {
        if texture_name.is_empty() {
            return Err(Error::msg("The texture name is invalid."));
        }
        if sampler_name.is_empty() {
            return Err(Error::msg("The texture sampler name is invalid."));
        }
        if w == 0 || h == 0 || d == 0 {
            return Err(Error::msg(format!(
                "The texture buffer size is invalid: [{w} x {h} x {d}]."
            )));
        }
        let size = w as usize
            * h as usize
            * d as usize
            * if channel == TextureType::RgbChannel {
                3
            } else {
                1
            };
        if values.len() < size {
            return Err(Error::msg("The buffer is invalid"));
        }
        Ok(Self {
            texture_name: texture_name.to_string(),
            sampler_name: sampler_name.to_string(),
            width: w,
            height: h,
            depth: d,
            channel,
            dimensions,
            interpolation,
            shader_binding_index,
            values: values[..size].to_vec(),
        })
    }
}

/// A uniform of a [`GpuShaderDesc`].
#[derive(Debug, Clone)]
struct Uniform {
    name: String,
    data: UniformData,
}

const GPU_FLOAT_SIZE: usize = 4;
const GPU_FLOAT_ALIGNMENT: usize = 4;
const GPU_INT_SIZE: usize = 4;
const GPU_INT_ALIGNMENT: usize = 4;
const GPU_VEC3_SIZE: usize = 12;
const GPU_VEC3_ALIGNMENT: usize = 16;
const GPU_ARRAY_ALIGNMENT: usize = 16;
const GPU_ARRAY_STRIDE: usize = 16;

fn align_offset(offset: usize, alignment: usize) -> usize {
    offset.div_ceil(alignment) * alignment
}

/// The generic shader description (port of `GenericGpuShaderDesc`): holds all
/// the information to build a shader program without baking the color
/// transform, i.e. the processor could contain several 1D or 3D LUTs.
#[derive(Debug, Clone)]
pub struct GpuShaderDesc {
    base: GpuShaderCreatorBase,
    max_1d_lut_width: u32,
    allow_texture_1d: bool,
    textures: Vec<GpuTexture>,
    textures_3d: Vec<GpuTexture>,
    uniforms: Vec<Uniform>,
    uniform_buffer_size: usize,
}

impl Default for GpuShaderDesc {
    fn default() -> Self {
        Self::new()
    }
}

impl GpuShaderDesc {
    /// Create the default shader description (port of `CreateShaderDesc`).
    pub fn new() -> Self {
        Self {
            base: GpuShaderCreatorBase::new(),
            max_1d_lut_width: 4 * 1024,
            allow_texture_1d: true,
            textures: Vec::new(),
            textures_3d: Vec::new(),
            uniforms: Vec::new(),
            uniform_buffer_size: 0,
        }
    }

    /// The complete shader program (after the processor filled it).
    pub fn shader_text(&self) -> &str {
        self.base.shader_text()
    }

    fn uniform_name_used(&self, name: &str) -> bool {
        self.uniforms.iter().any(|u| u.name == name)
    }

    fn push_uniform(
        &mut self,
        name: &str,
        value: UniformValue,
        alignment: usize,
        size: usize,
    ) -> Result<bool> {
        if name.is_empty() {
            return Err(Error::msg("The dynamic property name is invalid."));
        }
        if self.uniform_name_used(name) {
            // Uniform is already there.
            return Ok(false);
        }
        self.uniform_buffer_size = align_offset(self.uniform_buffer_size, alignment);
        self.uniforms.push(Uniform {
            name: name.to_string(),
            data: UniformData {
                value,
                buffer_offset: self.uniform_buffer_size,
            },
        });
        self.uniform_buffer_size += size;
        Ok(true)
    }

    // Accessors to the uniforms.

    /// Number of uniforms.
    pub fn num_uniforms(&self) -> usize {
        self.uniforms.len()
    }

    /// The name and the data of the uniform at `index`.
    pub fn uniform(&self, index: usize) -> Result<(&str, &UniformData)> {
        self.uniforms
            .get(index)
            .map(|u| (u.name.as_str(), &u.data))
            .ok_or_else(|| {
                Error::msg(format!(
                    "Uniforms access error: index = {} where size = {}",
                    index,
                    self.uniforms.len()
                ))
            })
    }

    /// Size in bytes of the uniform buffer holding all the uniforms (for the
    /// shading languages using uniform buffers).
    pub fn uniform_buffer_size(&self) -> usize {
        self.uniform_buffer_size
    }

    // Accessors to the 1D & 2D textures built from 1D LUTs.

    /// Number of 1D & 2D textures.
    pub fn num_textures(&self) -> usize {
        self.textures.len()
    }

    fn lut1d_access_error(&self, index: usize) -> Error {
        Error::msg(format!(
            "1D LUT access error: index = {} where size = {}",
            index,
            self.textures.len()
        ))
    }

    fn lut3d_access_error(&self, index: usize) -> Error {
        Error::msg(format!(
            "3D LUT access error: index = {} where size = {}",
            index,
            self.textures_3d.len()
        ))
    }

    /// The 1D or 2D texture at `index` (its values are also available with
    /// [`texture_values`](Self::texture_values)).
    pub fn texture(&self, index: usize) -> Result<&GpuTexture> {
        let t = self
            .textures
            .get(index)
            .ok_or_else(|| self.lut1d_access_error(index))?;
        if t.dimensions > 2 {
            return Err(Error::msg(format!(
                "1D LUT cannot have more than two dimensions: {} > 2",
                t.dimensions
            )));
        }
        Ok(t)
    }

    /// The dimensions of the 1D or 2D texture at `index`.
    pub fn texture_dimensions(&self, index: usize) -> Result<TextureDimensions> {
        let t = self.texture(index)?;
        Ok(if t.dimensions == 1 {
            TextureDimensions::Texture1D
        } else {
            TextureDimensions::Texture2D
        })
    }

    /// The values of the 1D or 2D texture at `index`.
    pub fn texture_values(&self, index: usize) -> Result<&[f32]> {
        self.textures
            .get(index)
            .map(|t| t.values.as_slice())
            .ok_or_else(|| self.lut1d_access_error(index))
    }

    /// The index used to declare the texture in the shader (for languages
    /// such as Vulkan).
    pub fn texture_shader_binding_index(&self, index: usize) -> Result<u32> {
        self.textures
            .get(index)
            .map(|t| t.shader_binding_index + self.base.texture_binding_start)
            .ok_or_else(|| self.lut1d_access_error(index))
    }

    // Accessors to the 3D textures built from 3D LUTs.

    /// Number of 3D textures.
    pub fn num_3d_textures(&self) -> usize {
        self.textures_3d.len()
    }

    /// The 3D texture at `index` (the edge length is its width).
    pub fn texture_3d(&self, index: usize) -> Result<&GpuTexture> {
        self.textures_3d
            .get(index)
            .ok_or_else(|| self.lut3d_access_error(index))
    }

    /// The values of the 3D texture at `index`.
    pub fn texture_3d_values(&self, index: usize) -> Result<&[f32]> {
        self.texture_3d(index).map(|t| t.values.as_slice())
    }

    /// The index used to declare the 3D texture in the shader.
    pub fn texture_3d_shader_binding_index(&self, index: usize) -> Result<u32> {
        self.texture_3d(index)
            .map(|t| t.shader_binding_index + self.base.texture_binding_start)
    }

    fn next_binding_index(&self) -> u32 {
        (self.textures.len() + self.textures_3d.len()) as u32
    }
}

impl GpuShaderCreator for GpuShaderDesc {
    fn base(&self) -> &GpuShaderCreatorBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut GpuShaderCreatorBase {
        &mut self.base
    }

    fn clone_creator(&self) -> Box<dyn GpuShaderCreator> {
        let mut desc = GpuShaderDesc::new();
        desc.base = self.base.clone_for_creator();
        Box::new(desc)
    }

    fn set_texture_max_width(&mut self, max_width: u32) {
        self.max_1d_lut_width = max_width;
    }

    fn texture_max_width(&self) -> u32 {
        self.max_1d_lut_width
    }

    fn set_allow_texture_1d(&mut self, allowed: bool) {
        self.allow_texture_1d = allowed;
    }

    fn allow_texture_1d(&self) -> bool {
        self.allow_texture_1d
    }

    fn add_uniform_double(&mut self, name: &str, getter: DoubleGetter) -> Result<bool> {
        self.push_uniform(
            name,
            UniformValue::Double(getter),
            GPU_FLOAT_ALIGNMENT,
            GPU_FLOAT_SIZE,
        )
    }

    fn add_uniform_bool(&mut self, name: &str, getter: BoolGetter) -> Result<bool> {
        // bool is not supported for buffered uniforms, an int is used instead.
        self.push_uniform(
            name,
            UniformValue::Bool(getter),
            GPU_INT_ALIGNMENT,
            GPU_INT_SIZE,
        )
    }

    fn add_uniform_float3(&mut self, name: &str, getter: Float3Getter) -> Result<bool> {
        self.push_uniform(
            name,
            UniformValue::Float3(getter),
            GPU_VEC3_ALIGNMENT,
            GPU_VEC3_SIZE,
        )
    }

    fn add_uniform_vector_float(
        &mut self,
        name: &str,
        get_size: SizeGetter,
        getter: VectorFloatGetter,
        max_size: u32,
    ) -> Result<bool> {
        self.push_uniform(
            name,
            UniformValue::VectorFloat(get_size, getter),
            GPU_ARRAY_ALIGNMENT,
            GPU_ARRAY_STRIDE * max_size as usize,
        )
    }

    fn add_uniform_vector_int(
        &mut self,
        name: &str,
        get_size: SizeGetter,
        getter: VectorIntGetter,
        max_size: u32,
    ) -> Result<bool> {
        self.push_uniform(
            name,
            UniformValue::VectorInt(get_size, getter),
            GPU_ARRAY_ALIGNMENT,
            GPU_ARRAY_STRIDE * max_size as usize,
        )
    }

    fn add_texture(
        &mut self,
        texture_name: &str,
        sampler_name: &str,
        width: u32,
        height: u32,
        channel: TextureType,
        dimensions: TextureDimensions,
        interpolation: Interpolation,
        values: &[f32],
    ) -> Result<u32> {
        if width > self.max_1d_lut_width {
            return Err(Error::msg(format!(
                "1D LUT size exceeds the maximum: {} > {}",
                width, self.max_1d_lut_width
            )));
        }
        let index = self.next_binding_index();
        let t = GpuTexture::new(
            texture_name,
            sampler_name,
            width,
            height,
            1,
            channel,
            dimensions as u32,
            interpolation,
            index,
            values,
        )?;
        self.textures.push(t);
        Ok(index + self.base.texture_binding_start)
    }

    fn add_3d_texture(
        &mut self,
        texture_name: &str,
        sampler_name: &str,
        edgelen: u32,
        interpolation: Interpolation,
        values: &[f32],
    ) -> Result<u32> {
        if edgelen > MAX_3D_LUT_LENGTH {
            return Err(Error::msg(format!(
                "3D LUT edge length exceeds the maximum: {} > {}",
                edgelen, MAX_3D_LUT_LENGTH
            )));
        }
        let index = self.next_binding_index();
        let t = GpuTexture::new(
            texture_name,
            sampler_name,
            edgelen,
            edgelen,
            edgelen,
            TextureType::RgbChannel,
            3,
            interpolation,
            index,
            values,
        )?;
        self.textures_3d.push(t);
        Ok(index + self.base.texture_binding_start)
    }
}

#[cfg(test)]
mod tests;
