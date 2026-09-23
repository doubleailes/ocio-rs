//! Helpers to build shader programs in the various shading languages (port of
//! `GpuShaderUtils.cpp`).
//!
//! [`GpuShaderText`] accumulates the lines of a shader program and knows how
//! to spell the basic constructs (types, constants, texture lookups, matrix
//! products, ...) of every [`GpuLanguage`]. The [`nl!`](crate::nl) macro
//! mirrors OCIO's `newLine() << a << b << ...` idiom: every argument is
//! appended to the line, numbers being formatted like OCIO does.

use crate::error::{Error, Result};
use crate::ops::matrix::float_format::format_g;
use crate::types::GpuLanguage;

use super::GpuShaderCreator;

/// Build one shader line from its parts and add it to a [`GpuShaderText`]
/// (port of OCIO's `GpuShaderText::newLine() << ...`).
///
/// Every argument must implement [`ShaderArg`]: strings are appended as-is,
/// floating point values are formatted with enough digits to round-trip
/// (adding a trailing `.` to integral values, see [`get_float_string`]) and
/// integers are appended in decimal. Without argument, an empty line is
/// added.
#[macro_export]
macro_rules! nl {
    ($st:expr) => {
        $st.push_line("")
    };
    ($st:expr, $($arg:expr),+ $(,)?) => {{
        let __lang = $st.language();
        let mut __line = ::std::string::String::new();
        $( $crate::gpu::shader_text::ShaderArg::append_to(&$arg, __lang, &mut __line); )+
        $st.push_line(__line);
    }};
}

/// Largest finite half value.
const HALF_MAX: f64 = 65504.0;
/// Smallest normalized half value.
const HALF_NRM_MIN: f64 = 6.103_515_625e-05;

/// Port of `ClampToNormHalf`: clamp to the finite half range and flush the
/// denormalized values to zero.
fn clamp_to_norm_half(val: f64) -> f64 {
    if val < -HALF_MAX {
        return -HALF_MAX;
    }
    if val > -HALF_NRM_MIN && val < HALF_NRM_MIN {
        return 0.0;
    }
    if val > HALF_MAX {
        return HALF_MAX;
    }
    val
}

/// A floating point type the shader text can print (single or double
/// precision, each with its own number of significant digits).
pub trait ShaderFloat: Copy {
    /// Format the value as OCIO's `getFloatString<T>` does.
    fn float_string(self, lang: GpuLanguage) -> String;
}

impl ShaderFloat for f32 {
    fn float_string(self, lang: GpuLanguage) -> String {
        let value = if lang == GpuLanguage::Cg {
            clamp_to_norm_half(f64::from(self)) as f32
        } else {
            self
        };
        // std::numeric_limits<float>::max_digits10 == 9.
        finish_float_string(f64::from(value), f64::from(value.fract()), 9)
    }
}

impl ShaderFloat for f64 {
    fn float_string(self, lang: GpuLanguage) -> String {
        let value = if lang == GpuLanguage::Cg {
            clamp_to_norm_half(self)
        } else {
            self
        };
        // std::numeric_limits<double>::max_digits10 == 17.
        finish_float_string(value, value.fract(), 17)
    }
}

fn finish_float_string(value: f64, frac: f64, precision: usize) -> String {
    let mut s = format_g(value, precision);
    // std::modf returns a zero fractional part for infinities, the dot is only
    // added for finite values.
    if frac == 0.0 && value.is_finite() {
        s.push('.');
    }
    s
}

/// Convert a float / double to a string, adding a dot when the value does not
/// have a fractional part so that the shader interprets it as a float and not
/// as an integer (port of `getFloatString`).
///
/// ```
/// use ocio::gpu::shader_text::get_float_string;
/// use ocio::GpuLanguage;
/// assert_eq!(get_float_string(1.0f32, GpuLanguage::Glsl1_3), "1.");
/// assert_eq!(get_float_string(0.5f64, GpuLanguage::Glsl1_3), "0.5");
/// ```
pub fn get_float_string<T: ShaderFloat>(v: T, lang: GpuLanguage) -> String {
    v.float_string(lang)
}

/// A value that can be used where OCIO accepts either a number or a string
/// expression (e.g. the components of `float3Const`).
pub trait ShaderValue {
    /// The text of the value in the shader.
    fn shader_value(&self, lang: GpuLanguage) -> String;
}

impl ShaderValue for f32 {
    fn shader_value(&self, lang: GpuLanguage) -> String {
        self.float_string(lang)
    }
}

impl ShaderValue for f64 {
    fn shader_value(&self, lang: GpuLanguage) -> String {
        self.float_string(lang)
    }
}

impl ShaderValue for str {
    fn shader_value(&self, _lang: GpuLanguage) -> String {
        self.to_string()
    }
}

impl ShaderValue for &str {
    fn shader_value(&self, _lang: GpuLanguage) -> String {
        (*self).to_string()
    }
}

impl ShaderValue for String {
    fn shader_value(&self, _lang: GpuLanguage) -> String {
        self.clone()
    }
}

impl ShaderValue for &String {
    fn shader_value(&self, _lang: GpuLanguage) -> String {
        (*self).clone()
    }
}

/// A part of a shader line (see [`nl!`](crate::nl)).
pub trait ShaderArg {
    /// Append the value to `line`.
    fn append_to(&self, lang: GpuLanguage, line: &mut String);
}

impl ShaderArg for str {
    fn append_to(&self, _lang: GpuLanguage, line: &mut String) {
        line.push_str(self);
    }
}

impl ShaderArg for &str {
    fn append_to(&self, _lang: GpuLanguage, line: &mut String) {
        line.push_str(self);
    }
}

impl ShaderArg for String {
    fn append_to(&self, _lang: GpuLanguage, line: &mut String) {
        line.push_str(self);
    }
}

impl ShaderArg for &String {
    fn append_to(&self, _lang: GpuLanguage, line: &mut String) {
        line.push_str(self);
    }
}

impl ShaderArg for f32 {
    fn append_to(&self, lang: GpuLanguage, line: &mut String) {
        line.push_str(&self.float_string(lang));
    }
}

impl ShaderArg for f64 {
    fn append_to(&self, lang: GpuLanguage, line: &mut String) {
        line.push_str(&self.float_string(lang));
    }
}

macro_rules! impl_shader_arg_int {
    ($($t:ty),*) => {
        $(
            impl ShaderArg for $t {
                fn append_to(&self, _lang: GpuLanguage, line: &mut String) {
                    line.push_str(&self.to_string());
                }
            }
        )*
    };
}

impl_shader_arg_int!(i32, u32, i64, u64, usize);

fn empty_name_error() -> Error {
    Error::msg("GPU variable name is empty.")
}

fn check_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(empty_name_error());
    }
    Ok(())
}

fn is_glsl(lang: GpuLanguage) -> bool {
    matches!(
        lang,
        GpuLanguage::Glsl1_2
            | GpuLanguage::Glsl1_3
            | GpuLanguage::Glsl4_0
            | GpuLanguage::GlslVk4_6
            | GpuLanguage::GlslEs1_0
            | GpuLanguage::GlslEs3_0
    )
}

fn vec_keyword(lang: GpuLanguage, n: usize) -> String {
    match lang {
        GpuLanguage::Glsl1_2
        | GpuLanguage::Glsl1_3
        | GpuLanguage::Glsl4_0
        | GpuLanguage::GlslVk4_6
        | GpuLanguage::GlslEs1_0
        | GpuLanguage::GlslEs3_0 => format!("vec{n}"),
        GpuLanguage::Cg => format!("half{n}"),
        GpuLanguage::Msl2_0 | GpuLanguage::HlslSm5_0 => format!("float{n}"),
        GpuLanguage::Osl1 => format!("vector{n}"),
    }
}

/// Texture and sampler declarations of an `n` dimensions texture.
fn tex_decl(
    lang: GpuLanguage,
    n: usize,
    texture_name: &str,
    sampler_name: &str,
    descriptor_set_index: u32,
    texture_index: u32,
) -> Result<(String, String)> {
    match lang {
        GpuLanguage::Glsl1_2
        | GpuLanguage::Glsl1_3
        | GpuLanguage::Cg
        | GpuLanguage::Glsl4_0
        | GpuLanguage::GlslEs1_0
        | GpuLanguage::GlslEs3_0 => Ok((String::new(), format!("uniform sampler{n}D {sampler_name};"))),
        GpuLanguage::GlslVk4_6 => Ok((
            String::new(),
            format!(
                "layout(set={descriptor_set_index}, binding = {texture_index}) uniform sampler{n}D {sampler_name}; "
            ),
        )),
        GpuLanguage::HlslSm5_0 => Ok((
            format!("Texture{n}D {texture_name};"),
            format!("SamplerState {sampler_name};"),
        )),
        GpuLanguage::Osl1 => Err(Error::msg(
            "Unsupported by the Open Shading language (OSL) translation.",
        )),
        GpuLanguage::Msl2_0 => Ok((
            format!("texture{n}d<float> {texture_name};"),
            format!("sampler {sampler_name};"),
        )),
    }
}

/// Texture lookup of an `n` dimensions texture.
fn tex_sample(
    lang: GpuLanguage,
    n: usize,
    texture_name: &str,
    sampler_name: &str,
    coords: &str,
) -> Result<String> {
    match lang {
        GpuLanguage::Glsl1_2 => Ok(format!("texture{n}D({sampler_name}, {coords})")),
        GpuLanguage::Glsl1_3 => Ok(format!("texture({sampler_name}, {coords})")),
        GpuLanguage::GlslEs1_0 => {
            if n == 1 {
                return Err(Error::msg("1D textures are unsupported by OpenGL ES."));
            }
            Ok(format!("texture{n}D({sampler_name}, {coords})"))
        }
        GpuLanguage::Cg => Ok(format!("tex{n}D({sampler_name}, {coords})")),
        GpuLanguage::HlslSm5_0 => Ok(format!("{texture_name}.Sample({sampler_name}, {coords})")),
        GpuLanguage::Glsl4_0 | GpuLanguage::GlslVk4_6 => {
            Ok(format!("texture({sampler_name}, {coords})"))
        }
        GpuLanguage::GlslEs3_0 => {
            if n == 1 {
                return Err(Error::msg("1D textures are unsupported by OpenGL ES."));
            }
            Ok(format!("texture({sampler_name}, {coords})"))
        }
        GpuLanguage::Osl1 => Err(Error::msg(
            "Unsupported by the Open Shading language (OSL) translation.",
        )),
        GpuLanguage::Msl2_0 => Ok(format!("{texture_name}.sample({sampler_name}, {coords})")),
    }
}

/// The matrix values as a comma separated list (port of `getMatrixValues`).
fn matrix_values<T: ShaderFloat>(
    mtx: &[T],
    n: usize,
    lang: GpuLanguage,
    transpose: bool,
) -> String {
    let mut vals = String::new();
    for i in 0..n * n {
        let line = i / n;
        let col = i % n;
        let idx = if i == n * n - 1 {
            // The last value is not transposed (it is on the diagonal).
            n * n - 1
        } else if transpose {
            col * n + line
        } else {
            line * n + col
        };
        if let Some(v) = mtx.get(idx) {
            vals.push_str(&v.float_string(lang));
        }
        if i != n * n - 1 {
            vals.push_str(", ");
        }
    }
    vals
}

/// Helper class to create shader programs (port of `GpuShaderText`).
#[derive(Debug, Clone)]
pub struct GpuShaderText {
    lang: GpuLanguage,
    text: String,
    indent: u32,
}

impl GpuShaderText {
    /// A new empty shader text for `lang`.
    pub fn new(lang: GpuLanguage) -> Self {
        Self {
            lang,
            text: String::new(),
            indent: 0,
        }
    }

    /// The shading language.
    pub fn language(&self) -> GpuLanguage {
        self.lang
    }

    /// Add a line: the current indentation, `line` and a trailing newline
    /// (port of `flushLine`). See also [`nl!`](crate::nl).
    pub fn push_line(&mut self, line: impl AsRef<str>) {
        const TAB_SIZE: usize = 2;
        for _ in 0..(TAB_SIZE * self.indent as usize) {
            self.text.push(' ');
        }
        self.text.push_str(line.as_ref());
        self.text.push('\n');
    }

    /// The shader text produced so far.
    pub fn string(&self) -> String {
        self.text.clone()
    }

    /// The shader text produced so far.
    pub fn as_str(&self) -> &str {
        &self.text
    }

    // Indentation helper functions.

    /// Set the indentation level of the next lines.
    pub fn set_indent(&mut self, indent: u32) {
        self.indent = indent;
    }

    /// Increase the indentation level.
    pub fn indent(&mut self) {
        self.indent += 1;
    }

    /// Decrease the indentation level.
    pub fn dedent(&mut self) {
        // The C++ code uses an unsigned counter; never go below zero here.
        self.indent = self.indent.saturating_sub(1);
    }

    // Basic types.

    /// The keyword declaring a constant (with a trailing space if any).
    pub fn const_keyword(&self) -> String {
        match self.lang {
            GpuLanguage::Glsl1_2
            | GpuLanguage::Glsl1_3
            | GpuLanguage::Glsl4_0
            | GpuLanguage::GlslVk4_6
            | GpuLanguage::GlslEs1_0
            | GpuLanguage::GlslEs3_0
            | GpuLanguage::Msl2_0 => "const ".to_string(),
            GpuLanguage::HlslSm5_0 => "static const ".to_string(),
            GpuLanguage::Osl1 | GpuLanguage::Cg => String::new(),
        }
    }

    /// The float type keyword.
    pub fn float_keyword(&self) -> String {
        if self.lang == GpuLanguage::Cg {
            "half".to_string()
        } else {
            "float".to_string()
        }
    }

    /// The constant float type keyword.
    pub fn float_keyword_const(&self) -> String {
        self.const_keyword() + &self.float_keyword()
    }

    /// Declaration of a float variable.
    pub fn float_decl(&self, name: &str) -> Result<String> {
        check_name(name)?;
        Ok(format!("{} {}", self.float_keyword(), name))
    }

    /// The int type keyword.
    pub fn int_keyword(&self) -> String {
        "int".to_string()
    }

    /// The constant int type keyword.
    pub fn int_keyword_const(&self) -> String {
        self.const_keyword() + &self.int_keyword()
    }

    /// Declaration of an int variable.
    pub fn int_decl(&self, name: &str) -> Result<String> {
        check_name(name)?;
        Ok(format!("{} {}", self.int_keyword(), name))
    }

    /// Declaration of a color variable (three elements).
    pub fn color_decl(&self, name: &str) -> Result<String> {
        check_name(name)?;
        let kw = if self.lang == GpuLanguage::Osl1 {
            "color".to_string()
        } else {
            self.float3_keyword()
        };
        Ok(format!("{kw} {name}"))
    }

    /// Comparison of two vectors usable in an `if` condition (MSL and HLSL do
    /// not allow a vector of booleans, `any()` is used).
    pub fn vector_compare_expression(&self, lhs: &str, op: &str, rhs: &str) -> String {
        let ret = format!("{lhs} {op} {rhs}");
        if self.lang == GpuLanguage::Msl2_0 || self.lang == GpuLanguage::HlslSm5_0 {
            format!("any( {ret} )")
        } else {
            ret
        }
    }

    // Scalar & arrays helper functions.

    /// Declaration and initialization of a float variable.
    pub fn declare_var_str(&self, name: &str, v: f32) -> Result<String> {
        check_name(name)?;
        // Note that OSL does not support inf and -inf so the value is adjusted
        // to the float max.
        if v.is_infinite() {
            let new_val = if v.is_sign_negative() {
                -f32::MAX
            } else {
                f32::MAX
            };
            return Ok(format!(
                "{} = {}",
                self.float_decl(name)?,
                format_g(f64::from(new_val), 9)
            ));
        }
        Ok(format!(
            "{} = {}",
            self.float_decl(name)?,
            v.float_string(self.lang)
        ))
    }

    /// Declare a float variable.
    pub fn declare_var(&mut self, name: &str, v: f32) -> Result<()> {
        let s = self.declare_var_str(name, v)?;
        nl!(self, s, ";");
        Ok(())
    }

    /// Declare a constant float variable.
    pub fn declare_var_const(&mut self, name: &str, v: f32) -> Result<()> {
        let s = self.declare_var_str(name, v)?;
        nl!(self, self.const_keyword(), s, ";");
        Ok(())
    }

    /// Declaration and initialization of a bool variable.
    pub fn declare_var_str_bool(&self, name: &str, v: bool) -> Result<String> {
        check_name(name)?;
        if self.lang == GpuLanguage::Osl1 {
            Ok(format!(
                "{} {} = {}",
                self.int_keyword(),
                name,
                if v { "1" } else { "0" }
            ))
        } else {
            Ok(format!(
                "bool {} = {}",
                name,
                if v { "true" } else { "false" }
            ))
        }
    }

    /// Declare a bool variable.
    pub fn declare_var_bool(&mut self, name: &str, v: bool) -> Result<()> {
        let s = self.declare_var_str_bool(name, v)?;
        nl!(self, s, ";");
        Ok(())
    }

    /// Declare a constant bool variable.
    pub fn declare_var_const_bool(&mut self, name: &str, v: bool) -> Result<()> {
        let s = self.declare_var_str_bool(name, v)?;
        nl!(self, self.const_keyword(), s, ";");
        Ok(())
    }

    /// Declare a constant float array.
    pub fn declare_float_array_const(&mut self, name: &str, v: &[f32]) -> Result<()> {
        if v.is_empty() {
            return Err(Error::msg("GPU array size is 0."));
        }
        check_name(name)?;
        let size = v.len();
        let values = v
            .iter()
            .map(|x| x.float_string(self.lang))
            .collect::<Vec<_>>()
            .join(", ");
        let line = match self.lang {
            GpuLanguage::Glsl1_2
            | GpuLanguage::Glsl1_3
            | GpuLanguage::Glsl4_0
            | GpuLanguage::GlslVk4_6
            | GpuLanguage::GlslEs1_0
            | GpuLanguage::GlslEs3_0 => format!(
                "{} {}[{}] = {}[{}]({});",
                self.float_keyword_const(),
                name,
                size,
                self.float_keyword(),
                size,
                values
            ),
            GpuLanguage::Osl1 | GpuLanguage::Cg | GpuLanguage::HlslSm5_0 => format!(
                "{} {}[{}] = {{{}}};",
                self.float_keyword_const(),
                name,
                size,
                values
            ),
            GpuLanguage::Msl2_0 => format!(
                "constant constexpr static float {}[{}] = {{{}}};",
                name, size, values
            ),
        };
        self.push_line(line);
        Ok(())
    }

    /// Declare a constant int array.
    pub fn declare_int_array_const(&mut self, name: &str, v: &[i32]) -> Result<()> {
        if v.is_empty() {
            return Err(Error::msg("GPU array size is 0."));
        }
        check_name(name)?;
        let size = v.len();
        let values = v
            .iter()
            .map(|x| x.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let line = match self.lang {
            GpuLanguage::Glsl1_2
            | GpuLanguage::Glsl1_3
            | GpuLanguage::Glsl4_0
            | GpuLanguage::GlslVk4_6
            | GpuLanguage::GlslEs1_0
            | GpuLanguage::GlslEs3_0 => format!(
                "{} {}[{}] = {}[{}]({});",
                self.int_keyword_const(),
                name,
                size,
                self.int_keyword(),
                size,
                values
            ),
            GpuLanguage::HlslSm5_0 => format!(
                "{} {}[{}] = {{{}}};",
                self.int_keyword_const(),
                name,
                size,
                values
            ),
            GpuLanguage::Msl2_0 => format!(
                "constant constexpr static int {}[{}] = {{{}}};",
                name, size, values
            ),
            GpuLanguage::Osl1 | GpuLanguage::Cg => format!(
                "{} {}[{}] = {{{}}};",
                self.int_keyword(),
                name,
                size,
                values
            ),
        };
        self.push_line(line);
        Ok(())
    }

    // Float2 helper functions.

    /// The keyword of a two elements vector.
    pub fn float2_keyword(&self) -> String {
        vec_keyword(self.lang, 2)
    }

    /// A constant two elements vector.
    pub fn float2_const(&self, x: &str, y: &str) -> String {
        format!("{}({}, {})", self.float2_keyword(), x, y)
    }

    /// Declaration of a two elements vector.
    pub fn float2_decl(&self, name: &str) -> Result<String> {
        check_name(name)?;
        Ok(format!("{} {}", self.float2_keyword(), name))
    }

    // Float3 helper functions.

    /// The keyword of a three elements vector.
    pub fn float3_keyword(&self) -> String {
        if self.lang == GpuLanguage::Osl1 {
            "vector".to_string()
        } else {
            vec_keyword(self.lang, 3)
        }
    }

    /// A constant three elements vector (each element being a number or an
    /// expression).
    pub fn float3_const<X, Y, Z>(&self, x: X, y: Y, z: Z) -> String
    where
        X: ShaderValue,
        Y: ShaderValue,
        Z: ShaderValue,
    {
        format!(
            "{}({}, {}, {})",
            self.float3_keyword(),
            x.shader_value(self.lang),
            y.shader_value(self.lang),
            z.shader_value(self.lang)
        )
    }

    /// A constant three elements vector with the same value for all elements.
    pub fn float3_const1<V: ShaderValue>(&self, v: V) -> String {
        let s = v.shader_value(self.lang);
        self.float3_const(s.as_str(), s.as_str(), s.as_str())
    }

    /// Declaration of a three elements vector.
    pub fn float3_decl(&self, name: &str) -> Result<String> {
        check_name(name)?;
        Ok(format!("{} {}", self.float3_keyword(), name))
    }

    /// Declare and initialize a three elements vector.
    pub fn declare_float3<X, Y, Z>(&mut self, name: &str, x: X, y: Y, z: Z) -> Result<()>
    where
        X: ShaderValue,
        Y: ShaderValue,
        Z: ShaderValue,
    {
        let decl = self.float3_decl(name)?;
        let value = self.float3_const(x, y, z);
        nl!(self, decl, " = ", value, ";");
        Ok(())
    }

    /// Declare and initialize a three elements vector from an array.
    pub fn declare_float3_arr(&mut self, name: &str, v: &[f32; 3]) -> Result<()> {
        self.declare_float3(name, v[0], v[1], v[2])
    }

    // Float4 helper functions.

    /// The keyword of a four elements vector.
    pub fn float4_keyword(&self) -> String {
        vec_keyword(self.lang, 4)
    }

    /// A constant four elements vector (each element being a number or an
    /// expression).
    pub fn float4_const<X, Y, Z, W>(&self, x: X, y: Y, z: Z, w: W) -> String
    where
        X: ShaderValue,
        Y: ShaderValue,
        Z: ShaderValue,
        W: ShaderValue,
    {
        format!(
            "{}({}, {}, {}, {})",
            self.float4_keyword(),
            x.shader_value(self.lang),
            y.shader_value(self.lang),
            z.shader_value(self.lang),
            w.shader_value(self.lang)
        )
    }

    /// A constant four elements vector with the same value for all elements.
    pub fn float4_const1<V: ShaderValue>(&self, v: V) -> String {
        let s = v.shader_value(self.lang);
        self.float4_const(s.as_str(), s.as_str(), s.as_str(), s.as_str())
    }

    /// Declaration of a four elements vector.
    pub fn float4_decl(&self, name: &str) -> Result<String> {
        check_name(name)?;
        Ok(format!("{} {}", self.float4_keyword(), name))
    }

    /// Declare and initialize a four elements vector.
    pub fn declare_float4<X, Y, Z, W>(&mut self, name: &str, x: X, y: Y, z: Z, w: W) -> Result<()>
    where
        X: ShaderValue,
        Y: ShaderValue,
        Z: ShaderValue,
        W: ShaderValue,
    {
        let decl = self.float4_decl(name)?;
        let value = self.float4_const(x, y, z, w);
        nl!(self, decl, " = ", value, ";");
        Ok(())
    }

    // Texture helpers.

    /// The name of the sampler of a texture.
    pub fn sampler_name(texture_name: &str) -> String {
        format!("{texture_name}Sampler")
    }

    fn declare_tex(&mut self, n: usize, name: &str, set: u32, index: u32) -> Result<()> {
        let (tex, sampler) = tex_decl(self.lang, n, name, &Self::sampler_name(name), set, index)?;
        if !tex.is_empty() {
            self.push_line(tex);
        }
        if !sampler.is_empty() {
            self.push_line(sampler);
        }
        Ok(())
    }

    /// Declare the global texture and sampler information for a 1D texture.
    pub fn declare_tex1d(
        &mut self,
        name: &str,
        descriptor_set_index: u32,
        index: u32,
    ) -> Result<()> {
        self.declare_tex(1, name, descriptor_set_index, index)
    }

    /// Declare the global texture and sampler information for a 2D texture.
    pub fn declare_tex2d(
        &mut self,
        name: &str,
        descriptor_set_index: u32,
        index: u32,
    ) -> Result<()> {
        self.declare_tex(2, name, descriptor_set_index, index)
    }

    /// Declare the global texture and sampler information for a 3D texture.
    pub fn declare_tex3d(
        &mut self,
        name: &str,
        descriptor_set_index: u32,
        index: u32,
    ) -> Result<()> {
        self.declare_tex(3, name, descriptor_set_index, index)
    }

    /// The texture lookup call for a 1D texture.
    pub fn sample_tex1d(&self, name: &str, coords: &str) -> Result<String> {
        tex_sample(self.lang, 1, name, &Self::sampler_name(name), coords)
    }

    /// The texture lookup call for a 2D texture.
    pub fn sample_tex2d(&self, name: &str, coords: &str) -> Result<String> {
        tex_sample(self.lang, 2, name, &Self::sampler_name(name), coords)
    }

    /// The texture lookup call for a 3D texture.
    pub fn sample_tex3d(&self, name: &str, coords: &str) -> Result<String> {
        tex_sample(self.lang, 3, name, &Self::sampler_name(name), coords)
    }

    // Uniform helpers.

    fn uniform_decl_prefix(&self) -> &'static str {
        if self.lang == GpuLanguage::Msl2_0 || self.lang == GpuLanguage::GlslVk4_6 {
            ""
        } else {
            "uniform "
        }
    }

    /// Declare a float uniform.
    pub fn declare_uniform_float(&mut self, name: &str) {
        nl!(
            self,
            self.uniform_decl_prefix(),
            self.float_keyword(),
            " ",
            name,
            ";"
        );
    }

    /// Declare a bool uniform (an int for Vulkan).
    pub fn declare_uniform_bool(&mut self, name: &str) {
        let (prefix, kw) = match self.lang {
            GpuLanguage::Msl2_0 => ("", "bool"),
            GpuLanguage::GlslVk4_6 => ("", "int"),
            _ => ("uniform ", "bool"),
        };
        nl!(self, prefix, kw, " ", name, ";");
    }

    /// Declare a three elements vector uniform.
    pub fn declare_uniform_float3(&mut self, name: &str) {
        nl!(
            self,
            self.uniform_decl_prefix(),
            self.float3_keyword(),
            " ",
            name,
            ";"
        );
    }

    /// Declare a float array uniform.
    pub fn declare_uniform_array_float(&mut self, name: &str, size: u32) {
        nl!(
            self,
            self.uniform_decl_prefix(),
            self.float_keyword(),
            " ",
            name,
            "[",
            size,
            "];"
        );
    }

    /// Declare an int array uniform.
    pub fn declare_uniform_array_int(&mut self, name: &str, size: u32) {
        nl!(
            self,
            self.uniform_decl_prefix(),
            self.int_keyword(),
            " ",
            name,
            "[",
            size,
            "];"
        );
    }

    // Matrix multiplication helpers.

    /// Multiplication of a 3x3 matrix (row major) by a three elements vector.
    pub fn mat3f_mul<T: ShaderFloat>(&self, m3x3: &[T], vec_name: &str) -> Result<String> {
        check_name(vec_name)?;
        let lang = self.lang;
        Ok(match lang {
            _ if is_glsl(lang) => {
                // OpenGL shader program requests a transposed matrix.
                format!(
                    "mat3({}) * {}",
                    matrix_values(m3x3, 3, lang, true),
                    vec_name
                )
            }
            GpuLanguage::Cg => format!(
                "mul(half3x3({}), {})",
                matrix_values(m3x3, 3, lang, false),
                vec_name
            ),
            GpuLanguage::HlslSm5_0 => format!(
                "mul({}, float3x3({}))",
                vec_name,
                matrix_values(m3x3, 3, lang, true)
            ),
            GpuLanguage::Osl1 => {
                format!(
                    "matrix({}) * {}",
                    matrix_values(m3x3, 3, lang, true),
                    vec_name
                )
            }
            _ => format!(
                "float3x3({}) * {}",
                matrix_values(m3x3, 3, lang, true),
                vec_name
            ),
        })
    }

    /// Multiplication of a 4x4 matrix (row major) by a four elements vector.
    pub fn mat4f_mul<T: ShaderFloat>(&self, m4x4: &[T], vec_name: &str) -> Result<String> {
        check_name(vec_name)?;
        let lang = self.lang;
        Ok(match lang {
            _ if is_glsl(lang) => {
                // OpenGL shader program requests a transposed matrix.
                format!(
                    "mat4({}) * {}",
                    matrix_values(m4x4, 4, lang, true),
                    vec_name
                )
            }
            GpuLanguage::Cg => format!(
                "mul(half4x4({}), {})",
                matrix_values(m4x4, 4, lang, false),
                vec_name
            ),
            GpuLanguage::HlslSm5_0 => format!(
                "mul({}, float4x4({}))",
                vec_name,
                matrix_values(m4x4, 4, lang, true)
            ),
            GpuLanguage::Osl1 => {
                format!(
                    "matrix({}) * {}",
                    matrix_values(m4x4, 4, lang, true),
                    vec_name
                )
            }
            _ => format!(
                "float4x4({}) * {}",
                matrix_values(m4x4, 4, lang, true),
                vec_name
            ),
        })
    }

    // Special function helpers.

    /// Linear interpolation of two quantities.
    pub fn lerp(&self, x: &str, y: &str, a: &str) -> String {
        match self.lang {
            GpuLanguage::Cg | GpuLanguage::HlslSm5_0 => format!("lerp({x}, {y}, {a})"),
            _ => format!("mix({x}, {y}, {a})"),
        }
    }

    /// Three elements 'greater than' comparison: each element is 1 if a > b,
    /// 0 otherwise.
    pub fn float3_greater_than(&self, a: &str, b: &str) -> String {
        self.float3_compare(a, b, ">", "greaterThan")
    }

    /// Four elements 'greater than' comparison.
    pub fn float4_greater_than(&self, a: &str, b: &str) -> String {
        self.float4_compare(a, b, ">", "greaterThan")
    }

    /// Three elements 'greater than or equal' comparison.
    pub fn float3_greater_than_equal(&self, a: &str, b: &str) -> String {
        self.float3_compare(a, b, ">=", "greaterThanEqual")
    }

    /// Four elements 'greater than or equal' comparison.
    pub fn float4_greater_than_equal(&self, a: &str, b: &str) -> String {
        self.float4_compare(a, b, ">=", "greaterThanEqual")
    }

    fn float3_compare(&self, a: &str, b: &str, op: &str, func: &str) -> String {
        match self.lang {
            GpuLanguage::Osl1 | GpuLanguage::Msl2_0 | GpuLanguage::HlslSm5_0 => format!(
                "{}(({a}[0] {op} {b}[0]) ? 1.0 : 0.0, ({a}[1] {op} {b}[1]) ? 1.0 : 0.0, ({a}[2] {op} {b}[2]) ? 1.0 : 0.0)",
                self.float3_keyword()
            ),
            _ => format!("{}({func}( {a}, {b}))", self.float3_keyword()),
        }
    }

    fn float4_compare(&self, a: &str, b: &str, op: &str, func: &str) -> String {
        match self.lang {
            GpuLanguage::Msl2_0 | GpuLanguage::HlslSm5_0 => format!(
                "{}(({a}[0] {op} {b}[0]) ? 1.0 : 0.0, ({a}[1] {op} {b}[1]) ? 1.0 : 0.0, ({a}[2] {op} {b}[2]) ? 1.0 : 0.0, ({a}[3] {op} {b}[3]) ? 1.0 : 0.0)",
                self.float4_keyword()
            ),
            GpuLanguage::Osl1 => format!(
                "{}(({a}.rgb.r {op} {b}.x) ? 1.0 : 0.0, ({a}.rgb.g {op} {b}.y) ? 1.0 : 0.0, ({a}.rgb.b {op} {b}.z) ? 1.0 : 0.0, ({a}.a {op} {b}.w) ? 1.0 : 0.0)",
                self.float4_keyword()
            ),
            _ => format!("{}({func}( {a}, {b}))", self.float4_keyword()),
        }
    }

    /// Four-quadrant arctangent.
    pub fn atan2(&self, y: &str, x: &str) -> String {
        match self.lang {
            _ if is_glsl(self.lang) || self.lang == GpuLanguage::Cg => {
                // Note: "atan" not "atan2".
                format!("atan({y}, {x})")
            }
            // Note: Various internet sources claim that the x & y arguments need
            // to be swapped for HLSL (relative to GLSL). However, recent testing
            // on Windows has revealed that the argument order needs to be the
            // same as GLSL.
            _ => format!("atan2({y}, {x})"),
        }
    }

    /// Sign of a vector (note: the returned text ends with a `;`).
    pub fn sign(&self, v: &str) -> String {
        if self.lang == GpuLanguage::Osl1 {
            format!(
                "sign({});",
                self.float4_const(
                    format!("{v}.rgb.r"),
                    format!("{v}.rgb.g"),
                    format!("{v}.rgb.b"),
                    format!("{v}.a")
                )
            )
        } else {
            format!("sign({v});")
        }
    }

    /// Cast to bool for shading languages that do not support implicit casts
    /// from int to bool (to use on bool uniforms).
    pub fn cast_to_bool(&self, v: &str) -> String {
        if self.lang == GpuLanguage::GlslVk4_6 {
            format!("bool({v})")
        } else {
            v.to_string()
        }
    }
}

/// Replace all the occurrences of `from` by `to` (port of
/// `StringUtils::ReplaceInPlace`, which does not rescan the replacement).
pub(crate) fn replace_all(s: &str, from: &str, to: &str) -> String {
    s.replace(from, to)
}

/// Create a resource name prepending the prefix of the shader creator to
/// `base` (port of `BuildResourceName`).
pub fn build_resource_name(
    shader_creator: &dyn GpuShaderCreator,
    prefix: &str,
    base: &str,
) -> String {
    let name = format!("{}_{}_{}", shader_creator.resource_prefix(), prefix, base);
    // Note: Remove potentially problematic double underscores from GLSL
    // resource names.
    replace_all(&name, "__", "_")
}

// Math functions used by multiple GPU renderers.

fn add_lin_to_log_shader_impl(pix: &str, st: &mut GpuShaderText, blue_only: bool) -> Result<()> {
    nl!(st, "{"); // establish scope so local variable names won't conflict
    st.indent();
    nl!(
        st,
        st.float_keyword_const(),
        " xbrk = 0.0041318374739483946;"
    );
    nl!(
        st,
        st.float_keyword_const(),
        " shift = -0.000157849851665374;"
    );
    nl!(st, st.float_keyword_const(), " m = 1. / (0.18 + shift);");
    nl!(st, st.float_keyword_const(), " base2 = 1.4426950408889634;"); // 1/log(2)
    nl!(st, st.float_keyword_const(), " gain = 363.034608563;");
    nl!(st, st.float_keyword_const(), " offs = -7.;");
    nl!(
        st,
        st.float3_decl("ylin")?,
        " = ",
        pix,
        ".rgb * gain + offs;"
    );
    nl!(
        st,
        st.float3_decl("ylog")?,
        " = base2 * log( ( ",
        pix,
        ".rgb + shift ) * m );"
    );
    if !blue_only {
        nl!(
            st,
            pix,
            ".rgb.r = (",
            pix,
            ".rgb.r < xbrk) ? ylin.x : ylog.x;"
        );
        nl!(
            st,
            pix,
            ".rgb.g = (",
            pix,
            ".rgb.g < xbrk) ? ylin.y : ylog.y;"
        );
    }
    nl!(
        st,
        pix,
        ".rgb.b = (",
        pix,
        ".rgb.b < xbrk) ? ylin.z : ylog.z;"
    );
    st.dedent();
    nl!(st, "}");
    Ok(())
}

fn add_log_to_lin_shader_impl(pix: &str, st: &mut GpuShaderText, blue_only: bool) -> Result<()> {
    nl!(st, "{"); // establish scope so local variable names won't conflict
    st.indent();
    nl!(st, st.float_keyword_const(), " ybrk = -5.5;");
    nl!(
        st,
        st.float_keyword_const(),
        " shift = -0.000157849851665374;"
    );
    nl!(st, st.float_keyword_const(), " gain = 363.034608563;");
    nl!(st, st.float_keyword_const(), " offs = -7.;");
    nl!(
        st,
        st.float3_decl("xlin")?,
        " = (",
        pix,
        ".rgb - offs) / gain;"
    );
    nl!(
        st,
        st.float3_decl("xlog")?,
        " = pow( ",
        st.float3_const1(2.0f32),
        ", ",
        pix,
        ".rgb ) * (0.18 + shift) - shift;"
    );
    if !blue_only {
        nl!(
            st,
            pix,
            ".rgb.r = (",
            pix,
            ".rgb.r < ybrk) ? xlin.x : xlog.x;"
        );
        nl!(
            st,
            pix,
            ".rgb.g = (",
            pix,
            ".rgb.g < ybrk) ? xlin.y : xlog.y;"
        );
    }
    nl!(
        st,
        pix,
        ".rgb.b = (",
        pix,
        ".rgb.b < ybrk) ? xlin.z : xlog.z;"
    );
    st.dedent();
    nl!(st, "}");
    Ok(())
}

/// Convert scene-linear values to "grading log" (port of `AddLinToLogShader`).
/// Grading log is in units of F-Stops with 0 being 18% grey. Above about -5,
/// it is pretty much exactly F-Stops but below that it is a pseudo-log so
/// that 0.0 is at -7 stops rather than -Inf (as in a pure log).
pub fn add_lin_to_log_shader(
    shader_creator: &dyn GpuShaderCreator,
    st: &mut GpuShaderText,
) -> Result<()> {
    add_lin_to_log_shader_impl(shader_creator.pixel_name(), st, false)
}

/// Same as [`add_lin_to_log_shader`] for the blue channel only.
pub fn add_lin_to_log_shader_channel_blue(
    shader_creator: &dyn GpuShaderCreator,
    st: &mut GpuShaderText,
) -> Result<()> {
    add_lin_to_log_shader_impl(shader_creator.pixel_name(), st, true)
}

/// Convert "grading log" values to scene-linear (port of `AddLogToLinShader`).
pub fn add_log_to_lin_shader(
    shader_creator: &dyn GpuShaderCreator,
    st: &mut GpuShaderText,
) -> Result<()> {
    add_log_to_lin_shader_impl(shader_creator.pixel_name(), st, false)
}

/// Same as [`add_log_to_lin_shader`] for the blue channel only.
pub fn add_log_to_lin_shader_channel_blue(
    shader_creator: &dyn GpuShaderCreator,
    st: &mut GpuShaderText,
) -> Result<()> {
    add_log_to_lin_shader_impl(shader_creator.pixel_name(), st, true)
}
