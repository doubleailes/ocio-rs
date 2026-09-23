//! Class wrappers adapting the generated shader program to the shading
//! languages needing it (port of `GpuShaderClassWrapper.cpp`).

use crate::error::{Error, Result};
use crate::nl;
use crate::types::GpuLanguage;

use super::shader_text::GpuShaderText;

/// Name of the variable holding the length of an array parameter.
fn array_length_variable_name(variable_name: &str) -> String {
    format!("{variable_name}_count")
}

/// A parameter of the Metal class wrapper function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FunctionParam {
    ty: String,
    name: String,
    is_array: bool,
}

impl FunctionParam {
    fn new(ty: &str, name: &str) -> Self {
        Self {
            ty: ty.to_string(),
            name: name.to_string(),
            is_array: name.contains('['),
        }
    }

    /// The name without the array size.
    fn base_name(&self) -> &str {
        match self.name.find('[') {
            Some(pos) => &self.name[..pos],
            None => &self.name,
        }
    }
}

/// The class wrapper of a shading language (port of `GpuShaderClassWrapper`
/// and its `Null`, `OSL` and `Metal` implementations).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ClassWrapper {
    /// Most languages do not need any wrapper.
    Null,
    /// Open Shading Language wrapper.
    Osl { function_name: String },
    /// Metal wrapper (the shader is embedded in a struct).
    Metal {
        class_name: String,
        function_name: String,
        params: Vec<FunctionParam>,
    },
}

impl ClassWrapper {
    /// The class wrapper of `language` (port of `CreateClassWrapper`).
    pub(crate) fn create(language: GpuLanguage) -> Self {
        match language {
            GpuLanguage::Msl2_0 => ClassWrapper::Metal {
                class_name: String::new(),
                function_name: String::new(),
                params: Vec::new(),
            },
            GpuLanguage::Osl1 => ClassWrapper::Osl {
                function_name: String::new(),
            },
            _ => ClassWrapper::Null,
        }
    }

    /// Prepare the wrapper (collects the Metal function parameters).
    pub(crate) fn prepare(&mut self, resource_prefix: &str, fn_name: &str, original_header: &str) {
        match self {
            ClassWrapper::Null => {}
            ClassWrapper::Osl { function_name } => *function_name = fn_name.to_string(),
            ClassWrapper::Metal {
                class_name,
                function_name,
                params,
            } => {
                *function_name = fn_name.to_string();
                *class_name = metal_class_wrapper_name(resource_prefix, fn_name);
                *params = extract_function_parameters(original_header);
            }
        }
    }

    /// True if the wrapper replaces the declarations header.
    pub(crate) fn has_header(&self) -> bool {
        !matches!(self, ClassWrapper::Null)
    }

    /// The header of the shader program.
    pub(crate) fn header(&self, original_header: &str) -> Result<String> {
        match self {
            ClassWrapper::Null => Ok(original_header.to_string()),
            ClassWrapper::Osl { function_name } => Ok(osl_header(function_name) + original_header),
            ClassWrapper::Metal {
                class_name, params, ..
            } => {
                let mut st = GpuShaderText::new(GpuLanguage::Msl2_0);
                metal_header(&mut st, class_name, params)?;
                nl!(st);
                let mut s = "\n// Declaration of class wrapper\n\n".to_string();
                s.push_str(st.as_str());
                s.push_str(&rewrite_array_declarations(params, original_header));
                Ok(s)
            }
        }
    }

    /// The footer of the shader program.
    pub(crate) fn footer(&self, original_footer: &str) -> Result<String> {
        match self {
            ClassWrapper::Null => Ok(original_footer.to_string()),
            ClassWrapper::Osl { function_name } => {
                let mut st = GpuShaderText::new(GpuLanguage::Osl1);
                nl!(st, "");
                nl!(st, "outColor = ", function_name, "(inColor);");
                nl!(st, "}");
                Ok(original_footer.to_string() + st.as_str())
            }
            ClassWrapper::Metal {
                class_name,
                function_name,
                params,
            } => {
                let mut st = GpuShaderText::new(GpuLanguage::Msl2_0);
                nl!(st);
                metal_footer(&mut st, class_name, function_name, params)?;
                let mut s = original_footer.to_string();
                s.push_str("\n// Close class wrapper\n\n");
                s.push_str(st.as_str());
                Ok(s)
            }
        }
    }
}

fn osl_header(function_name: &str) -> String {
    let mut st = GpuShaderText::new(GpuLanguage::Osl1);

    nl!(st, "");
    nl!(st, "/* All the includes */");
    nl!(st, "");
    nl!(st, "#include \"vector4.h\"");
    nl!(st, "#include \"color4.h\"");

    nl!(st, "");
    nl!(st, "/* All the generic helper methods */");

    let helpers: [(&str, &str); 8] = [
        (
            "vector4 __operator__mul__(matrix m, vector4 v)",
            "return transform(m, v);",
        ),
        (
            "vector4 __operator__mul__(color4 c, vector4 v)",
            "return vector4(c.rgb.r, c.rgb.g, c.rgb.b, c.a) * v;",
        ),
        (
            "vector4 __operator__mul__(vector4 v, color4 c)",
            "return v * vector4(c.rgb.r, c.rgb.g, c.rgb.b, c.a);",
        ),
        (
            "vector4 __operator__sub__(color4 c, vector4 v)",
            "return vector4(c.rgb.r, c.rgb.g, c.rgb.b, c.a) - v;",
        ),
        (
            "vector4 __operator__add__(vector4 v, color4 c)",
            "return v + vector4(c.rgb.r, c.rgb.g, c.rgb.b, c.a);",
        ),
        (
            "vector4 __operator__add__(color4 c, vector4 v)",
            "return vector4(c.rgb.r, c.rgb.g, c.rgb.b, c.a) + v;",
        ),
        (
            "vector4 pow(color4 c, vector4 v)",
            "return pow(vector4(c.rgb.r, c.rgb.g, c.rgb.b, c.a), v);",
        ),
        (
            "vector4 max(vector4 v, color4 c)",
            "return max(v, vector4(c.rgb.r, c.rgb.g, c.rgb.b, c.a));",
        ),
    ];

    for (decl, body) in helpers {
        nl!(st, "");
        nl!(st, decl);
        nl!(st, "{");
        st.indent();
        nl!(st, body);
        st.dedent();
        nl!(st, "}");
    }

    nl!(st, "");
    nl!(st, "/* The shader implementation */");
    nl!(st, "");
    nl!(
        st,
        "shader ",
        "OSL_",
        function_name,
        "(color4 inColor = {color(0), 1}, output color4 outColor = {color(0), 1})"
    );
    nl!(st, "{");

    st.string()
}

fn metal_class_wrapper_name(resource_prefix: &str, function_name: &str) -> String {
    let prefix = if resource_prefix.is_empty() {
        "OCIO_"
    } else {
        resource_prefix
    };
    format!("{prefix}{function_name}")
}

fn check_class_name(class_name: &str) -> Result<()> {
    if class_name.is_empty() {
        return Err(Error::msg("Struct name must include at least 1 character"));
    }
    if class_name.starts_with(|c: char| c.is_ascii_digit()) {
        return Err(Error::msg(format!(
            "Struct name must not start with a digit. Invalid className passed in: {class_name}"
        )));
    }
    Ok(())
}

fn push_params(st: &mut GpuShaderText, params: &[FunctionParam]) {
    let mut separator = "";
    for param in params {
        nl!(
            st,
            separator,
            if param.is_array { "constant " } else { "" },
            param.ty,
            " ",
            param.name
        );
        if param.is_array {
            nl!(st, ", int ", array_length_variable_name(param.base_name()));
        }
        separator = ", ";
    }
}

fn metal_header(st: &mut GpuShaderText, class_name: &str, params: &[FunctionParam]) -> Result<()> {
    check_class_name(class_name)?;

    nl!(st, "struct ", class_name);
    nl!(st, "{");
    nl!(st, class_name, "(");
    st.indent();
    push_params(st, params);
    st.dedent();
    nl!(st, ")");
    nl!(st, "{");

    st.indent();
    for param in params {
        // Array members are pointers into constant memory (see
        // rewrite_array_declarations) so they are bound rather than copied,
        // using the parameter name without its array size.
        let variable_name = param.base_name();
        nl!(st, "this->", variable_name, " = ", variable_name, ";");
    }
    st.dedent();
    nl!(st, "}");
    Ok(())
}

fn metal_footer(
    st: &mut GpuShaderText,
    class_name: &str,
    ocio_function_name: &str,
    params: &[FunctionParam],
) -> Result<()> {
    check_class_name(class_name)?;

    nl!(st, "};");

    nl!(st, st.float4_keyword(), " ", ocio_function_name, "(");

    st.indent();
    push_params(st, params);
    let separator = if params.is_empty() { "" } else { ", " };
    nl!(st, separator, st.float4_keyword(), " inPixel)");
    st.dedent();
    nl!(st, "{");
    st.indent();
    nl!(st, "return ", class_name, "(");

    st.indent();
    let mut separator = "";
    for param in params {
        if !param.is_array {
            nl!(st, separator, param.name);
        } else {
            let base = param.base_name();
            nl!(st, separator, base);
            nl!(st, ", ", array_length_variable_name(base));
        }
        separator = ", ";
    }
    st.dedent();

    nl!(st, ").", ocio_function_name, "(inPixel);");
    st.dedent();
    nl!(st, "}");
    Ok(())
}

fn is_space(c: u8) -> bool {
    // std::isspace in the "C" locale.
    matches!(c, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
}

fn skip_spaces(line: &[u8], mut i: usize) -> usize {
    while i < line.len() && is_space(line[i]) {
        i += 1;
    }
    i
}

/// Find the first of `chars` at or after `from` (`len` if none).
fn find_first_of(line: &[u8], from: usize, chars: &[u8]) -> usize {
    line.iter()
        .enumerate()
        .skip(from)
        .find(|(_, c)| chars.contains(c))
        .map(|(i, _)| i)
        .unwrap_or(line.len())
}

fn substr(line: &[u8], start: usize, end: usize) -> String {
    let start = start.min(line.len());
    let end = end.clamp(start, line.len());
    String::from_utf8_lossy(&line[start..end]).into_owned()
}

/// Extract the parameters of the Metal wrapper function from the
/// declarations (port of `extractFunctionParameters`). The 3D textures are
/// always passed first, then the other textures and then the uniforms.
fn extract_function_parameters(declaration: &str) -> Vec<FunctionParam> {
    let mut lut3d_textures: Vec<(String, String, String)> = Vec::new();
    let mut lut_textures: Vec<(String, String, String)> = Vec::new();
    let mut uniforms: Vec<(String, String)> = Vec::new();

    let lines: Vec<&str> = declaration.split('\n').collect();
    let mut idx = 0;
    while idx < lines.len() {
        let line = lines[idx].as_bytes();
        idx += 1;

        if line.is_empty() {
            continue;
        }

        // Skip spaces.
        let mut i = skip_spaces(line, 0);

        // If the line was all skippable characters.
        if i >= line.len() {
            continue;
        }

        // If the line is a comment.
        if line[i] == b'/' && line.get(i + 1) == Some(&b'/') {
            continue;
        }

        if line[i..].starts_with(b"texture") {
            let texture_dim = line.get(i + 7).map(|c| i32::from(*c) - i32::from(b'0'));

            let end_texture_type = line
                .iter()
                .position(|c| *c == b'>')
                .unwrap_or(line.len().saturating_sub(1));
            let texture_type = substr(line, i, end_texture_type + 1);

            i = skip_spaces(line, end_texture_type + 1);

            let end_texture_name = find_first_of(line, i, b" \t;");
            let texture_name = substr(line, i, end_texture_name);

            // The sampler is declared on the next line.
            let sampler_line = if idx < lines.len() {
                idx += 1;
                lines[idx - 1].as_bytes()
            } else {
                &[]
            };

            let sampler_pos = sampler_line
                .windows(7)
                .position(|w| w == b"sampler")
                .map(|p| p + 7)
                .unwrap_or(sampler_line.len());
            let j = skip_spaces(sampler_line, sampler_pos);
            let end_sampler_name = find_first_of(sampler_line, j, b" \t;");
            let sampler_name = substr(sampler_line, j, end_sampler_name);

            if texture_dim == Some(3) {
                lut3d_textures.push((texture_type, texture_name, sampler_name));
            } else {
                lut_textures.push((texture_type, texture_name, sampler_name));
            }
        } else {
            let end_type_name = find_first_of(line, i, b" \t");
            let variable_type = substr(line, i, end_type_name);

            i = skip_spaces(line, end_type_name + 1);

            let end_variable_name = find_first_of(line, i, b" \t;");
            let variable_name = substr(line, i, end_variable_name);
            uniforms.push((variable_type, variable_name));
        }
    }

    let mut params = Vec::new();
    for (ty, name, sampler) in lut3d_textures.iter().chain(lut_textures.iter()) {
        params.push(FunctionParam::new(ty, name));
        params.push(FunctionParam::new("sampler", sampler));
    }
    for (ty, name) in &uniforms {
        params.push(FunctionParam::new(ty, name));
    }
    params
}

/// Rewrite the array declarations as pointers into constant memory (port of
/// `rewriteArrayDeclarations`).
///
/// The uniform declarations are shared with the other GPU languages so
/// arrays arrive here as fixed-size members i.e. `float name[120];`. Owning
/// them would make the constructor copy the whole array for each pixel, so a
/// pointer to the constant memory is held instead.
fn rewrite_array_declarations(params: &[FunctionParam], declarations: &str) -> String {
    let mut rewritten = declarations.to_string();
    for param in params.iter().filter(|p| p.is_array) {
        let declaration = format!("{} {};", param.ty, param.name);
        if let Some(pos) = rewritten.find(&declaration) {
            let replacement = format!("constant {}* {};", param.ty, param.base_name());
            rewritten.replace_range(pos..pos + declaration.len(), &replacement);
        }
    }
    rewritten
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metal_parameters() {
        let decl = "\n// Declaration of all variables\n\nfloat a;\nint b[8];\n\n\
                    // Declaration of all textures\n\ntexture1d<float> t1;\nsampler t1Sampler;\n\
                    texture3d<float> t3;\nsampler t3Sampler;\n";
        let params = extract_function_parameters(decl);
        let names: Vec<_> = params.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["t3", "t3Sampler", "t1", "t1Sampler", "a", "b[8]"]);
        assert!(params[5].is_array);
        assert_eq!(params[4].ty, "float");
    }

    #[test]
    fn metal_class_name_errors() {
        let mut w = ClassWrapper::create(GpuLanguage::Msl2_0);
        w.prepare("1", "abc", "");
        assert_eq!(
            w.header("").unwrap_err().message(),
            "Struct name must not start with a digit. Invalid className passed in: 1abc"
        );
        assert!(w.footer("").is_err());
        w.prepare("", "abc", "");
        assert!(w.header("").unwrap().contains("struct OCIO_abc"));

        let decl = "\nint b[8];\n";
        let params = extract_function_parameters(decl);
        assert_eq!(
            rewrite_array_declarations(&params, decl),
            "\nconstant int* b;\n"
        );
    }
}
