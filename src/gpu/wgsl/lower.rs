// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Lowering of the GLSL 4.0 subset written by the GPU renderers to WGSL.
//!
//! The source program is parsed (see [`super::glsl`]), typed and printed as
//! WGSL. The GLSL constructs without a WGSL equivalent are rewritten:
//!
//! * the ternary operator becomes `select(else, then, condition)`;
//! * assignments to multi-component swizzles (`v.rgb = e`) rebuild the
//!   whole vector, and swizzles of swizzles (`v.rgb.r`) are folded;
//! * implicit int to float conversions, scalar arguments of component-wise
//!   built-ins (`max(v, 0.)`) and vector comparisons (`==`) are made
//!   explicit;
//! * uniforms are read from a uniform buffer (see [`super::WgslBindings`])
//!   and textures are read with `textureLoad`, the interpolation being done
//!   by a generated helper function.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

use super::glsl::{self, Declarator, Expr, Function, Item, ParamQual, Scalar, Stmt, Ty};
use super::{WgslBindings, WgslTextureBinding, WgslTextureFormat, WgslUniform, WgslUniformType};
use crate::error::{Error, Result};
use crate::gpu::TextureType;
use crate::types::Interpolation;

fn err(msg: impl Into<String>) -> Error {
    Error::msg(format!("WGSL translation: {}", msg.into()))
}

/// A texture used by the program (port of the texture description of the
/// shader creator).
#[derive(Debug, Clone)]
pub(crate) struct TextureSource {
    pub texture_name: String,
    pub sampler_name: String,
    pub dimensions: u32,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub channel: TextureType,
    pub interpolation: Interpolation,
    pub binding: u32,
}

/// What the lowering needs to know besides the program.
pub(crate) struct LowerInput<'a> {
    pub source: &'a str,
    pub function_name: &'a str,
    pub group: u32,
    pub textures: &'a [TextureSource],
}

/// Words that cannot be used as WGSL identifiers (keywords and reserved
/// words), plus the predeclared names the lowering emits.
const RESERVED: &[&str] = &[
    "NULL",
    "Self",
    "abstract",
    "active",
    "alias",
    "alignas",
    "alignof",
    "array",
    "as",
    "asm",
    "asm_fragment",
    "async",
    "atomic",
    "attribute",
    "auto",
    "await",
    "become",
    "binding_array",
    "bitcast",
    "break",
    "case",
    "cast",
    "catch",
    "class",
    "co_await",
    "co_return",
    "co_yield",
    "coherent",
    "column_major",
    "common",
    "compile",
    "compile_fragment",
    "concept",
    "const",
    "const_assert",
    "const_cast",
    "consteval",
    "constexpr",
    "constinit",
    "continue",
    "continuing",
    "crate",
    "debugger",
    "decltype",
    "default",
    "delete",
    "demote",
    "demote_to_helper",
    "diagnostic",
    "discard",
    "do",
    "dynamic_cast",
    "else",
    "enable",
    "enum",
    "explicit",
    "export",
    "extends",
    "extern",
    "external",
    "f16",
    "f32",
    "fallthrough",
    "false",
    "filter",
    "final",
    "finally",
    "fn",
    "for",
    "friend",
    "from",
    "fxgroup",
    "get",
    "goto",
    "groupshared",
    "highp",
    "i32",
    "if",
    "impl",
    "implements",
    "import",
    "inline",
    "instanceof",
    "interface",
    "layout",
    "let",
    "loop",
    "lowp",
    "macro",
    "macro_rules",
    "mat2x2",
    "mat3x3",
    "mat4x4",
    "match",
    "mediump",
    "meta",
    "mod",
    "module",
    "move",
    "mut",
    "mutable",
    "namespace",
    "new",
    "nil",
    "noexcept",
    "noinline",
    "nointerpolation",
    "noperspective",
    "null",
    "nullptr",
    "of",
    "operator",
    "override",
    "package",
    "packoffset",
    "partition",
    "pass",
    "patch",
    "pixelfragment",
    "precise",
    "precision",
    "premerge",
    "priv",
    "protected",
    "ptr",
    "pub",
    "public",
    "readonly",
    "ref",
    "regardless",
    "register",
    "reinterpret_cast",
    "require",
    "requires",
    "resource",
    "restrict",
    "return",
    "sampler",
    "select",
    "self",
    "set",
    "shared",
    "sizeof",
    "smooth",
    "snorm",
    "static",
    "static_assert",
    "static_cast",
    "std",
    "struct",
    "subroutine",
    "super",
    "switch",
    "target",
    "template",
    "textureLoad",
    "this",
    "thread_local",
    "throw",
    "trait",
    "true",
    "try",
    "type",
    "typedef",
    "typeid",
    "typename",
    "typeof",
    "u32",
    "union",
    "unless",
    "unorm",
    "unsafe",
    "unsized",
    "use",
    "using",
    "var",
    "varying",
    "vec2",
    "vec3",
    "vec4",
    "virtual",
    "volatile",
    "wgsl",
    "where",
    "while",
    "with",
    "writeonly",
    "yield",
    "atan2",
    "inverseSqrt",
    "all",
    "any",
];

/// The WGSL spelling of an identifier of the source program.
fn sanitize(name: &str) -> String {
    let mut s = if RESERVED.contains(&name) || name.starts_with("__") || name == "_" {
        format!("{name}_")
    } else {
        name.to_string()
    };
    if s.starts_with("__") {
        s = format!("v{s}");
    }
    s
}

fn scalar_name(s: Scalar) -> &'static str {
    match s {
        Scalar::Bool => "bool",
        Scalar::Int => "i32",
        Scalar::Float => "f32",
    }
}

/// The WGSL spelling of a type.
fn ty_name(ty: &Ty) -> Result<String> {
    Ok(match ty {
        Ty::Scalar(s) => scalar_name(*s).to_string(),
        Ty::Vec(n, s) => format!("vec{n}<{}>", scalar_name(*s)),
        Ty::Mat(n) => format!("mat{n}x{n}<f32>"),
        Ty::Array(e, n) => format!("array<{}, {n}>", ty_name(e)?),
        Ty::Void => return Err(err("void is not a value type")),
        Ty::Sampler(_) => return Err(err("samplers are not value types")),
    })
}

/// A WGSL `f32` literal holding exactly the single precision value of the
/// GLSL literal.
fn float_literal(v: f32) -> Result<String> {
    if !v.is_finite() {
        return Err(err(format!("the value {v} has no WGSL f32 literal")));
    }
    let mut s = format!("{v:?}");
    if !s.contains('.') && !s.contains('e') {
        s.push_str(".0");
    }
    s.push('f');
    if v.is_sign_negative() {
        s = format!("({s})");
    }
    Ok(s)
}

fn parse_float_text(text: &str) -> Result<f32> {
    let t = text.trim_end_matches(['f', 'F']).trim_end_matches('.');
    let t = if t.is_empty() { "0" } else { t };
    t.parse::<f32>()
        .map_err(|_| err(format!("invalid float literal '{text}'")))
}

/// A lowered expression.
#[derive(Debug, Clone)]
struct Te {
    s: String,
    ty: Ty,
    /// Integer literal (convertible to a float literal).
    int_lit: Option<i64>,
    /// Can be followed by a postfix operator without parentheses.
    atomic: bool,
    /// Precedence of the top level operator (for parenthesizing).
    prec: u8,
    /// The top level binary operator, if any.
    op: Option<&'static str>,
}

impl Te {
    fn atom(s: String, ty: Ty) -> Te {
        Te {
            s,
            ty,
            int_lit: None,
            atomic: true,
            prec: 100,
            op: None,
        }
    }

    fn postfix(&self) -> String {
        if self.atomic {
            self.s.clone()
        } else {
            format!("({})", self.s)
        }
    }
}

/// How a name is bound.
#[derive(Debug, Clone)]
enum Binding {
    /// A local variable, a value parameter or a module constant.
    Value { name: String, ty: Ty },
    /// An `out` / `inout` parameter (a pointer).
    Pointer { name: String, ty: Ty },
    /// A member of the uniform buffer.
    Uniform { member: String, ty: Ty },
    /// A texture sampler.
    Sampler(usize),
}

/// A function signature.
#[derive(Debug, Clone)]
struct Signature {
    wgsl_name: String,
    params: Vec<(ParamQual, Ty)>,
    ret: Ty,
}

struct Lowering<'a> {
    input: &'a LowerInput<'a>,
    params_var: String,
    globals: HashMap<String, Binding>,
    functions: HashMap<String, Vec<Signature>>,
    scopes: Vec<HashMap<String, Binding>>,
    ret: Ty,
    out: String,
    indent: usize,
    tmp: usize,
    used_textures: Vec<usize>,
}

/// Lower the GLSL program to WGSL. Returns the WGSL module and the
/// description of its resources.
pub(crate) fn lower(input: &LowerInput<'_>) -> Result<(String, WgslBindings)> {
    let items = glsl::parse(input.source)?;
    let mut l = Lowering {
        input,
        params_var: format!("{}_params", sanitize(input.function_name)),
        globals: HashMap::new(),
        functions: HashMap::new(),
        scopes: Vec::new(),
        ret: Ty::Void,
        out: String::new(),
        indent: 0,
        tmp: 0,
        used_textures: Vec::new(),
    };
    l.module(&items)
}

fn align(offset: u32, alignment: u32) -> u32 {
    offset.div_ceil(alignment) * alignment
}

impl<'a> Lowering<'a> {
    fn line(&mut self, s: &str) {
        for _ in 0..self.indent {
            self.out.push_str("    ");
        }
        self.out.push_str(s);
        self.out.push('\n');
    }

    fn module(&mut self, items: &[Item]) -> Result<(String, WgslBindings)> {
        // Uniforms and textures.
        let mut uniforms = Vec::new();
        let mut members = Vec::new();
        let mut offset = 0u32;
        for item in items {
            let Item::Uniform(d) = item else { continue };
            if let Ty::Sampler(dim) = d.ty {
                let idx = self
                    .input
                    .textures
                    .iter()
                    .position(|t| t.sampler_name == d.name)
                    .ok_or_else(|| err(format!("unknown texture sampler '{}'", d.name)))?;
                if self.input.textures[idx].dimensions != u32::from(dim) {
                    return Err(err(format!(
                        "the texture '{}' has {} dimensions, not {dim}",
                        self.input.textures[idx].texture_name, self.input.textures[idx].dimensions
                    )));
                }
                self.used_textures.push(idx);
                self.globals.insert(d.name.clone(), Binding::Sampler(idx));
                continue;
            }
            let (uty, wgsl_ty, align_, size) = match &d.ty {
                Ty::Scalar(Scalar::Float) => (WgslUniformType::Float, "f32".to_string(), 4, 4),
                Ty::Scalar(Scalar::Int) => (WgslUniformType::Int, "i32".to_string(), 4, 4),
                Ty::Scalar(Scalar::Bool) => (WgslUniformType::Bool, "i32".to_string(), 4, 4),
                Ty::Vec(3, Scalar::Float) => {
                    (WgslUniformType::Float3, "vec3<f32>".to_string(), 16, 12)
                }
                Ty::Array(e, n) if **e == Ty::float() => (
                    WgslUniformType::FloatArray(*n as u32),
                    format!("array<vec4<f32>, {n}>"),
                    16,
                    16 * *n as u32,
                ),
                Ty::Array(e, n) if **e == Ty::int() => (
                    WgslUniformType::IntArray(*n as u32),
                    format!("array<vec4<i32>, {n}>"),
                    16,
                    16 * *n as u32,
                ),
                t => {
                    return Err(err(format!(
                        "unsupported uniform type {t:?} for '{}'",
                        d.name
                    )))
                }
            };
            offset = align(offset, align_);
            let member = sanitize(&d.name);
            uniforms.push(WgslUniform {
                name: d.name.clone(),
                ty: uty,
                offset,
                size,
            });
            members.push(format!("{member}: {wgsl_ty},"));
            self.globals.insert(
                d.name.clone(),
                Binding::Uniform {
                    member,
                    ty: d.ty.clone(),
                },
            );
            offset += size;
        }
        let uniform_buffer_size = align(offset, 16);

        // Module constants and function signatures.
        for item in items {
            match item {
                Item::Global { var, .. } => {
                    self.globals.insert(
                        var.name.clone(),
                        Binding::Value {
                            name: sanitize(&var.name),
                            ty: var.ty.clone(),
                        },
                    );
                }
                Item::Function(f) => {
                    let overloads = self.functions.entry(f.name.clone()).or_default();
                    let wgsl_name = if overloads.is_empty() {
                        sanitize(&f.name)
                    } else {
                        format!("{}_{}", sanitize(&f.name), overloads.len())
                    };
                    overloads.push(Signature {
                        wgsl_name,
                        params: f.params.iter().map(|p| (p.qual, p.ty.clone())).collect(),
                        ret: f.ret.clone(),
                    });
                }
                Item::Uniform(_) => {}
            }
        }

        let group = self.input.group;
        if !members.is_empty() {
            let struct_name = format!("{}_Parameters", sanitize(self.input.function_name));
            self.line("// Uniform buffer holding the OCIO parameters.");
            self.line(&format!("struct {struct_name} {{"));
            self.indent += 1;
            for m in &members {
                self.line(m);
            }
            self.indent -= 1;
            self.line("}");
            let decl = format!(
                "@group({group}) @binding(0) var<uniform> {}: {struct_name};",
                self.params_var
            );
            self.line(&decl);
            self.line("");
        }

        let mut textures = Vec::new();
        let used = self.used_textures.clone();
        for idx in used {
            let t = self.input.textures[idx].clone();
            if !members.is_empty() && t.binding == 0 {
                return Err(err(format!(
                    "the texture '{}' uses the binding 0 of the uniform buffer",
                    t.texture_name
                )));
            }
            self.texture(&t)?;
            textures.push(WgslTextureBinding {
                texture_name: t.texture_name.clone(),
                sampler_name: t.sampler_name.clone(),
                group,
                binding: t.binding,
                dimensions: t.dimensions,
                width: t.width,
                height: t.height,
                depth: t.depth,
                format: if t.channel == TextureType::RedChannel {
                    WgslTextureFormat::R32Float
                } else {
                    WgslTextureFormat::Rgba32Float
                },
                interpolation: t.interpolation,
            });
        }

        for item in items {
            if let Item::Global { is_const, var } = item {
                self.global(*is_const, var)?;
            }
        }

        let mut seen: HashMap<String, usize> = HashMap::new();
        for item in items {
            if let Item::Function(f) = item {
                let n = seen.entry(f.name.clone()).or_insert(0);
                let sig = self.functions[&f.name][*n].clone();
                *n += 1;
                self.function(f, &sig)?;
            }
        }

        let bindings = WgslBindings {
            group,
            uniform_binding: if members.is_empty() { None } else { Some(0) },
            uniform_buffer_size,
            uniforms,
            textures,
        };
        Ok((std::mem::take(&mut self.out), bindings))
    }

    /// Texture declaration and its lookup helper.
    fn texture(&mut self, t: &TextureSource) -> Result<()> {
        let name = sanitize(&t.texture_name);
        let n = t.dimensions;
        let group = self.input.group;
        let interp = match t.interpolation {
            Interpolation::Nearest => "nearest",
            _ => "linear",
        };
        self.line(&format!(
            "// {n}D LUT texture ({}, {} filtering done in the shader).",
            if t.channel == TextureType::RedChannel {
                "R32Float"
            } else {
                "Rgba32Float"
            },
            interp
        ));
        self.line(&format!(
            "@group({group}) @binding({}) var {name}: texture_{n}d<f32>;",
            t.binding
        ));
        let dims = [t.width, t.height, t.depth];
        let (fty, ity, fsize, imax) = if n == 1 {
            (
                "f32".to_string(),
                "i32".to_string(),
                float_literal(dims[0] as f32)?,
                format!("{}", dims[0].saturating_sub(1)),
            )
        } else {
            let fs: Vec<String> = dims[..n as usize]
                .iter()
                .map(|d| float_literal(*d as f32))
                .collect::<Result<_>>()?;
            let is: Vec<String> = dims[..n as usize]
                .iter()
                .map(|d| d.saturating_sub(1).to_string())
                .collect();
            (
                format!("vec{n}<f32>"),
                format!("vec{n}<i32>"),
                format!("vec{n}<f32>({})", fs.join(", ")),
                format!("vec{n}<i32>({})", is.join(", ")),
            )
        };
        let zero_i = if n == 1 {
            "0".to_string()
        } else {
            format!("vec{n}<i32>(0)")
        };
        let one_i = if n == 1 {
            "1".to_string()
        } else {
            format!("vec{n}<i32>(1)")
        };
        let lo = if n == 1 {
            "-1.0f".to_string()
        } else {
            format!("{fty}(-1.0f)")
        };
        self.line(&format!("fn {name}_lookup(coords: {fty}) -> vec4<f32> {{"));
        self.indent += 1;
        self.line(&format!("let size = {fsize};"));
        if t.interpolation == Interpolation::Nearest {
            self.line(&format!(
                "let i = clamp({ity}(floor(clamp(coords * size, {lo}, size))), {zero_i}, {imax});"
            ));
            self.line(&format!("return textureLoad({name}, i, 0);"));
        } else {
            self.line(&format!(
                "let p = clamp(coords * size - {}, {lo}, size);",
                if n == 1 {
                    "0.5f".to_string()
                } else {
                    format!("{fty}(0.5f)")
                }
            ));
            self.line("let f = floor(p);");
            self.line("let t = p - f;");
            self.line(&format!("let i0 = clamp({ity}(f), {zero_i}, {imax});"));
            self.line(&format!(
                "let i1 = clamp({ity}(f) + {one_i}, {zero_i}, {imax});"
            ));
            match n {
                1 => {
                    self.line(&format!(
                        "return mix(textureLoad({name}, i0, 0), textureLoad({name}, i1, 0), t);"
                    ));
                }
                2 => {
                    for (v, (x, y)) in [
                        ("c00", ("i0", "i0")),
                        ("c10", ("i1", "i0")),
                        ("c01", ("i0", "i1")),
                        ("c11", ("i1", "i1")),
                    ] {
                        self.line(&format!(
                            "let {v} = textureLoad({name}, vec2<i32>({x}.x, {y}.y), 0);"
                        ));
                    }
                    self.line("return mix(mix(c00, c10, t.x), mix(c01, c11, t.x), t.y);");
                }
                _ => {
                    for (v, (x, y, z)) in [
                        ("c000", ("i0", "i0", "i0")),
                        ("c100", ("i1", "i0", "i0")),
                        ("c010", ("i0", "i1", "i0")),
                        ("c110", ("i1", "i1", "i0")),
                        ("c001", ("i0", "i0", "i1")),
                        ("c101", ("i1", "i0", "i1")),
                        ("c011", ("i0", "i1", "i1")),
                        ("c111", ("i1", "i1", "i1")),
                    ] {
                        self.line(&format!(
                            "let {v} = textureLoad({name}, vec3<i32>({x}.x, {y}.y, {z}.z), 0);"
                        ));
                    }
                    self.line("let c00 = mix(c000, c100, t.x);");
                    self.line("let c10 = mix(c010, c110, t.x);");
                    self.line("let c01 = mix(c001, c101, t.x);");
                    self.line("let c11 = mix(c011, c111, t.x);");
                    self.line("return mix(mix(c00, c10, t.y), mix(c01, c11, t.y), t.z);");
                }
            }
        }
        self.indent -= 1;
        self.line("}");
        self.line("");
        Ok(())
    }

    fn global(&mut self, is_const: bool, var: &Declarator) -> Result<()> {
        let ty = ty_name(&var.ty)?;
        let name = sanitize(&var.name);
        self.scopes.push(HashMap::new());
        let init = match &var.init {
            Some(e) => Some(self.expr_to(e, &var.ty)?),
            None => None,
        };
        self.scopes.pop();
        // Arrays are indexed with run-time values: a module constant cannot
        // be, so they are private variables.
        let is_array = matches!(var.ty, Ty::Array(..));
        let line = match (init, is_const && !is_array) {
            (Some(init), true) => format!("const {name}: {ty} = {init};"),
            (Some(init), false) => format!("var<private> {name}: {ty} = {init};"),
            (None, _) => format!("var<private> {name}: {ty};"),
        };
        self.line(&line);
        Ok(())
    }

    fn lookup(&self, name: &str) -> Option<&Binding> {
        for s in self.scopes.iter().rev() {
            if let Some(b) = s.get(name) {
                return Some(b);
            }
        }
        self.globals.get(name)
    }

    fn declare(&mut self, name: &str, b: Binding) {
        if let Some(s) = self.scopes.last_mut() {
            s.insert(name.to_string(), b);
        }
    }

    fn function(&mut self, f: &Function, sig: &Signature) -> Result<()> {
        // Value parameters are immutable in WGSL: the assigned ones are
        // copied to a local variable.
        let mut assigned = HashSet::new();
        for s in &f.body {
            collect_assigned_stmt(s, &mut assigned);
        }
        self.scopes.push(HashMap::new());
        let mut params = Vec::new();
        let mut copies = Vec::new();
        for p in &f.params {
            let name = sanitize(&p.name);
            let ty = ty_name(&p.ty)?;
            match p.qual {
                ParamQual::In => {
                    if assigned.contains(&p.name) {
                        params.push(format!("{name}_in: {ty}"));
                        copies.push(format!("var {name}: {ty} = {name}_in;"));
                    } else {
                        params.push(format!("{name}: {ty}"));
                    }
                    self.declare(
                        &p.name,
                        Binding::Value {
                            name,
                            ty: p.ty.clone(),
                        },
                    );
                }
                ParamQual::Out | ParamQual::InOut => {
                    params.push(format!("{name}: ptr<function, {ty}>"));
                    self.declare(
                        &p.name,
                        Binding::Pointer {
                            name,
                            ty: p.ty.clone(),
                        },
                    );
                }
            }
        }
        let ret = if f.ret == Ty::Void {
            String::new()
        } else {
            format!(" -> {}", ty_name(&f.ret)?)
        };
        self.line(&format!(
            "fn {}({}){ret} {{",
            sig.wgsl_name,
            params.join(", ")
        ));
        self.indent += 1;
        for c in copies {
            self.line(&c);
        }
        self.ret = f.ret.clone();
        for s in &f.body {
            self.stmt(s)?;
        }
        self.indent -= 1;
        self.line("}");
        self.line("");
        self.scopes.pop();
        Ok(())
    }

    // Statements.

    fn block(&mut self, stmts: &[Stmt]) -> Result<()> {
        self.scopes.push(HashMap::new());
        for s in stmts {
            self.stmt(s)?;
        }
        self.scopes.pop();
        Ok(())
    }

    /// The statements of a branch / loop body (a block or one statement).
    fn body(&mut self, s: &Stmt) -> Result<()> {
        self.indent += 1;
        match s {
            Stmt::Block(b) => self.block(b)?,
            s => self.block(std::slice::from_ref(s))?,
        }
        self.indent -= 1;
        Ok(())
    }

    fn stmt(&mut self, s: &Stmt) -> Result<()> {
        match s {
            Stmt::Empty => {}
            Stmt::Decl { is_const, vars } => {
                for v in vars {
                    let line = self.decl(*is_const, v)?;
                    self.line(&format!("{line};"));
                }
            }
            Stmt::Expr(e) => {
                for line in self.expr_stmt(e)? {
                    self.line(&line);
                }
            }
            Stmt::Block(b) => {
                self.line("{");
                self.indent += 1;
                self.block(b)?;
                self.indent -= 1;
                self.line("}");
            }
            Stmt::If(c, t, e) => {
                let c = self.expr_to(c, &Ty::bool())?;
                self.line(&format!("if ({c}) {{"));
                self.body(t)?;
                let mut els = e.as_deref();
                while let Some(e) = els {
                    if let Stmt::If(c2, t2, e2) = e {
                        let c2 = self.expr_to(c2, &Ty::bool())?;
                        self.line(&format!("}} else if ({c2}) {{"));
                        self.body(t2)?;
                        els = e2.as_deref();
                    } else {
                        self.line("} else {");
                        self.body(e)?;
                        els = None;
                    }
                }
                self.line("}");
            }
            Stmt::For(init, cond, update, body) => {
                self.scopes.push(HashMap::new());
                let init = match init.as_deref() {
                    None | Some(Stmt::Empty) => String::new(),
                    Some(Stmt::Decl { is_const, vars }) if vars.len() == 1 => {
                        self.decl(*is_const, &vars[0])?
                    }
                    Some(Stmt::Expr(e)) => self.single_expr_stmt(e)?,
                    Some(_) => return Err(err("unsupported for loop initializer")),
                };
                let cond = match cond {
                    Some(c) => self.expr_to(c, &Ty::bool())?,
                    None => String::new(),
                };
                let update = match update {
                    Some(u) => self.single_expr_stmt(u)?,
                    None => String::new(),
                };
                self.line(&format!("for ({init}; {cond}; {update}) {{"));
                self.body(body)?;
                self.line("}");
                self.scopes.pop();
            }
            Stmt::While(c, body) => {
                let c = self.expr_to(c, &Ty::bool())?;
                self.line(&format!("while ({c}) {{"));
                self.body(body)?;
                self.line("}");
            }
            Stmt::DoWhile(body, c) => {
                self.line("loop {");
                self.body(body)?;
                self.indent += 1;
                let c = self.expr_to(c, &Ty::bool())?;
                self.line("continuing {");
                self.indent += 1;
                self.line(&format!("break if !({c});"));
                self.indent -= 1;
                self.line("}");
                self.indent -= 1;
                self.line("}");
            }
            Stmt::Return(e) => match e {
                Some(e) => {
                    let ret = self.ret.clone();
                    let v = self.expr_to(e, &ret)?;
                    self.line(&format!("return {v};"));
                }
                None => self.line("return;"),
            },
            Stmt::Break => self.line("break;"),
            Stmt::Continue => self.line("continue;"),
        }
        Ok(())
    }

    /// A local declaration (without the `;`).
    fn decl(&mut self, is_const: bool, v: &Declarator) -> Result<String> {
        let ty = ty_name(&v.ty)?;
        let init = match &v.init {
            Some(e) => Some(self.expr_to(e, &v.ty)?),
            None => None,
        };
        let name = sanitize(&v.name);
        self.declare(
            &v.name,
            Binding::Value {
                name: name.clone(),
                ty: v.ty.clone(),
            },
        );
        Ok(match init {
            Some(init) if is_const => format!("let {name}: {ty} = {init}"),
            Some(init) => format!("var {name}: {ty} = {init}"),
            None => format!("var {name}: {ty}"),
        })
    }

    /// An expression statement that must be a single WGSL statement (for
    /// loop clauses), without the `;`.
    fn single_expr_stmt(&mut self, e: &Expr) -> Result<String> {
        let lines = self.expr_stmt(e)?;
        match lines.as_slice() {
            [l] => Ok(l.trim_end_matches(';').to_string()),
            _ => Err(err("unsupported for loop clause")),
        }
    }

    /// The WGSL statements of an expression statement.
    fn expr_stmt(&mut self, e: &Expr) -> Result<Vec<String>> {
        match e {
            Expr::Assign(op, lhs, rhs) => self.assign(op, lhs, rhs),
            Expr::Step(op, target) => {
                let t = self.lvalue(target)?;
                let simple =
                    t.swizzle.is_none() || t.swizzle.as_ref().is_some_and(|s| s.len() == 1);
                let ty = self.lvalue_type(&t)?;
                if !simple {
                    return Err(err("increment of a vector swizzle"));
                }
                let target = t.text();
                if ty == Ty::int() {
                    Ok(vec![format!("{target}{op};")])
                } else {
                    let one = float_literal(1.0)?;
                    let o = if *op == "++" { "+=" } else { "-=" };
                    Ok(vec![format!("{target} {o} {one};")])
                }
            }
            Expr::Call(..) => {
                let te = self.expr(e)?;
                if te.ty == Ty::Void {
                    Ok(vec![format!("{};", te.s)])
                } else {
                    Ok(vec![format!("_ = {};", te.s)])
                }
            }
            _ => Err(err(format!("unsupported expression statement {e:?}"))),
        }
    }

    /// Decompose an assignment target into a base reference and an
    /// optional swizzle (component indices).
    fn lvalue(&mut self, e: &Expr) -> Result<LValue> {
        match e {
            Expr::Member(base, m) => {
                let mut inner = self.lvalue(base)?;
                let base_ty = self.lvalue_type(&inner)?;
                let idx = swizzle_indices(m, base_ty.width().unwrap_or(0))?;
                inner.swizzle = Some(match inner.swizzle {
                    Some(prev) => idx.iter().map(|i| prev[*i]).collect(),
                    None => idx,
                });
                Ok(inner)
            }
            Expr::Index(base, i) => {
                let inner = self.lvalue(base)?;
                if inner.swizzle.is_some() {
                    return Err(err("indexing of a swizzle in an assignment"));
                }
                let ty = self.lvalue_type(&inner)?;
                let elem = match ty {
                    Ty::Array(e, _) => *e,
                    Ty::Vec(_, s) => Ty::Scalar(s),
                    Ty::Mat(n) => Ty::Vec(n, Scalar::Float),
                    t => return Err(err(format!("indexing of a {t:?}"))),
                };
                let idx = self.expr_to(i, &Ty::int())?;
                Ok(LValue {
                    base: format!("{}[{idx}]", inner.base),
                    ty: elem,
                    swizzle: None,
                })
            }
            Expr::Ident(n) => match self.lookup(n) {
                Some(Binding::Value { name, ty }) => Ok(LValue {
                    base: name.clone(),
                    ty: ty.clone(),
                    swizzle: None,
                }),
                Some(Binding::Pointer { name, ty }) => Ok(LValue {
                    base: format!("(*{name})"),
                    ty: ty.clone(),
                    swizzle: None,
                }),
                _ => Err(err(format!("'{n}' cannot be assigned"))),
            },
            _ => Err(err(format!("unsupported assignment target {e:?}"))),
        }
    }

    fn lvalue_type(&self, l: &LValue) -> Result<Ty> {
        Ok(match &l.swizzle {
            None => l.ty.clone(),
            Some(idx) => {
                let s =
                    l.ty.scalar()
                        .ok_or_else(|| err("swizzle of a non-vector"))?;
                Ty::vec_or_scalar(idx.len() as u8, s)
            }
        })
    }

    fn assign(&mut self, op: &str, lhs: &Expr, rhs: &Expr) -> Result<Vec<String>> {
        let l = self.lvalue(lhs)?;
        let lty = self.lvalue_type(&l)?;
        let r = self.expr(rhs)?;
        let r = if op == "=" {
            self.coerce(&r, &lty)?
        } else {
            // e1 op= e2 is e1 = e1 op e2.
            let bop: &'static str = match op {
                "+=" => "+",
                "-=" => "-",
                "*=" => "*",
                "/=" => "/",
                _ => "%",
            };
            let cur = Te::atom(l.text(), lty.clone());
            let v = self.binary(bop, cur, r)?;
            self.coerce(&v, &lty)?
        };
        match &l.swizzle {
            None => Ok(vec![format!("{} = {r};", l.base)]),
            Some(idx) if idx.len() == 1 => {
                Ok(vec![format!("{}.{} = {r};", l.base, COMPONENTS[idx[0]])])
            }
            Some(idx) => {
                let n = l.ty.width().unwrap_or(0) as usize;
                let s = l.ty.scalar().unwrap_or(Scalar::Float);
                let vty = format!("vec{n}<{}>", scalar_name(s));
                // The whole vector, in order: a plain assignment.
                if idx.len() == n && idx.iter().enumerate().all(|(i, c)| i == *c) {
                    return Ok(vec![format!("{} = {r};", l.base)]);
                }
                // A prefix (e.g. .rgb of a vec4): the value then the rest.
                if idx.iter().enumerate().all(|(i, c)| i == *c) {
                    let rest: Vec<String> = (idx.len()..n)
                        .map(|k| format!("{}.{}", l.base, COMPONENTS[k]))
                        .collect();
                    return Ok(vec![format!(
                        "{} = {vty}({r}, {});",
                        l.base,
                        rest.join(", ")
                    )]);
                }
                // General case.
                self.tmp += 1;
                let t = format!("swz_{}", self.tmp);
                let comps: Vec<String> = (0..n)
                    .map(|k| match idx.iter().position(|c| *c == k) {
                        Some(j) => format!("{t}.{}", COMPONENTS[j]),
                        None => format!("{}.{}", l.base, COMPONENTS[k]),
                    })
                    .collect();
                Ok(vec![
                    "{".to_string(),
                    format!("    let {t} = {r};"),
                    format!("    {} = {vty}({});", l.base, comps.join(", ")),
                    "}".to_string(),
                ])
            }
        }
    }

    // Expressions.

    fn expr_to(&mut self, e: &Expr, ty: &Ty) -> Result<String> {
        let te = self.expr(e)?;
        self.coerce(&te, ty)
    }

    /// Convert `te` to `to` (GLSL implicit conversions and the splats of
    /// scalar arguments).
    fn coerce(&self, te: &Te, to: &Ty) -> Result<String> {
        if &te.ty == to {
            return Ok(te.s.clone());
        }
        match (&te.ty, to) {
            (Ty::Scalar(Scalar::Int), Ty::Scalar(Scalar::Float)) => match te.int_lit {
                Some(v) => float_literal(v as f32),
                None => Ok(format!("f32({})", te.s)),
            },
            (Ty::Scalar(Scalar::Float), Ty::Scalar(Scalar::Int)) => Ok(format!("i32({})", te.s)),
            (Ty::Scalar(Scalar::Int), Ty::Scalar(Scalar::Bool)) => Ok(format!("({} != 0)", te.s)),
            (Ty::Scalar(_), Ty::Vec(n, s)) => {
                let inner = self.coerce(te, &Ty::Scalar(*s))?;
                Ok(format!("vec{n}<{}>({inner})", scalar_name(*s)))
            }
            (Ty::Vec(m, _), Ty::Vec(n, s)) if m == n => {
                Ok(format!("vec{n}<{}>({})", scalar_name(*s), te.s))
            }
            (Ty::Array(a, n), Ty::Array(b, m)) if n == m && a == b => Ok(te.s.clone()),
            (from, to) => Err(err(format!(
                "cannot convert {from:?} to {to:?} in '{}'",
                te.s
            ))),
        }
    }

    fn expr(&mut self, e: &Expr) -> Result<Te> {
        match e {
            Expr::Num(text, is_float) => {
                if *is_float {
                    let v = parse_float_text(text)?;
                    let mut te = Te::atom(float_literal(v)?, Ty::float());
                    te.atomic = !v.is_sign_negative();
                    Ok(te)
                } else {
                    let v: i64 = text
                        .parse()
                        .map_err(|_| err(format!("invalid integer literal '{text}'")))?;
                    let mut te = Te::atom(text.clone(), Ty::int());
                    te.int_lit = Some(v);
                    Ok(te)
                }
            }
            Expr::Bool(b) => Ok(Te::atom(b.to_string(), Ty::bool())),
            Expr::Ident(n) => match self.lookup(n).cloned() {
                Some(Binding::Value { name, ty }) => Ok(Te::atom(name, ty)),
                Some(Binding::Pointer { name, ty }) => Ok(Te::atom(format!("(*{name})"), ty)),
                Some(Binding::Uniform { member, ty }) => {
                    let s = format!("{}.{member}", self.params_var);
                    match ty {
                        Ty::Scalar(Scalar::Bool) => Ok(Te::atom(format!("({s} != 0)"), Ty::bool())),
                        Ty::Array(..) => {
                            Err(err(format!("the uniform array '{n}' can only be indexed")))
                        }
                        ty => Ok(Te::atom(s, ty)),
                    }
                }
                Some(Binding::Sampler(_)) => Err(err(format!(
                    "the sampler '{n}' can only be used by texture()"
                ))),
                None => Err(err(format!("unknown identifier '{n}'"))),
            },
            Expr::Unary(op, inner) => {
                let v = self.expr(inner)?;
                match *op {
                    "+" => Ok(v),
                    "-" => {
                        if let Some(i) = v.int_lit {
                            let mut te = Te::atom(format!("-{i}"), v.ty.clone());
                            te.int_lit = Some(-i);
                            te.atomic = false;
                            te.prec = 50;
                            return Ok(te);
                        }
                        let inner = if v.prec < 100 || v.s.starts_with('-') {
                            format!("({})", v.s)
                        } else {
                            v.s.clone()
                        };
                        let mut te = Te::atom(format!("-{inner}"), v.ty.clone());
                        te.atomic = false;
                        te.prec = 50;
                        Ok(te)
                    }
                    "!" => {
                        let inner = self.coerce(&v, &Ty::bool())?;
                        let mut te = Te::atom(format!("!({inner})"), Ty::bool());
                        te.atomic = false;
                        te.prec = 50;
                        Ok(te)
                    }
                    _ => {
                        let mut te = Te::atom(format!("~({})", v.s), v.ty.clone());
                        te.atomic = false;
                        te.prec = 50;
                        Ok(te)
                    }
                }
            }
            Expr::Binary(op, a, b) => {
                let a = self.expr(a)?;
                let b = self.expr(b)?;
                self.binary(op, a, b)
            }
            Expr::Ternary(c, a, b) => {
                let c = self.expr_to(c, &Ty::bool())?;
                let a = self.expr(a)?;
                let b = self.expr(b)?;
                let ty = common_type(&a.ty, &b.ty)
                    .ok_or_else(|| err(format!("ternary of {:?} and {:?}", a.ty, b.ty)))?;
                let a = self.coerce(&a, &ty)?;
                let b = self.coerce(&b, &ty)?;
                Ok(Te::atom(format!("select({b}, {a}, {c})"), ty))
            }
            Expr::Member(base, m) => {
                let b = self.expr(base)?;
                let (n, s) = match b.ty {
                    Ty::Vec(n, s) => (n, s),
                    Ty::Scalar(s) => (1, s),
                    ref t => return Err(err(format!("member '{m}' of a {t:?}"))),
                };
                let idx = swizzle_indices(m, n)?;
                let sw: String = idx.iter().map(|i| COMPONENTS[*i]).collect();
                if n == 1 {
                    // A scalar swizzle (`x.x` / `x.xxx`).
                    let ty = Ty::vec_or_scalar(idx.len() as u8, s);
                    return Ok(if idx.len() == 1 {
                        b
                    } else {
                        Te::atom(format!("vec{}<{}>({})", idx.len(), scalar_name(s), b.s), ty)
                    });
                }
                Ok(Te::atom(
                    format!("{}.{sw}", b.postfix()),
                    Ty::vec_or_scalar(idx.len() as u8, s),
                ))
            }
            Expr::Index(base, i) => {
                if let Expr::Ident(n) = base.as_ref() {
                    if let Some(Binding::Uniform { member, ty }) = self.lookup(n).cloned() {
                        let elem = match ty {
                            Ty::Array(e, _) => *e,
                            t => return Err(err(format!("indexing of the uniform {t:?}"))),
                        };
                        let idx = self.expr_to(i, &Ty::int())?;
                        return Ok(Te::atom(
                            format!("{}.{member}[{idx}].x", self.params_var),
                            elem,
                        ));
                    }
                }
                let b = self.expr(base)?;
                let elem = match &b.ty {
                    Ty::Array(e, _) => (**e).clone(),
                    Ty::Vec(_, s) => Ty::Scalar(*s),
                    Ty::Mat(n) => Ty::Vec(*n, Scalar::Float),
                    t => return Err(err(format!("indexing of a {t:?}"))),
                };
                let idx = self.expr_to(i, &Ty::int())?;
                Ok(Te::atom(format!("{}[{idx}]", b.postfix()), elem))
            }
            Expr::ArrayCtor(ty, args) => {
                let Ty::Array(elem, n) = ty else {
                    return Err(err("invalid array constructor"));
                };
                if args.len() != *n {
                    return Err(err("array constructor size mismatch"));
                }
                let vals = args
                    .iter()
                    .map(|a| self.expr_to(a, elem))
                    .collect::<Result<Vec<_>>>()?;
                Ok(Te::atom(
                    format!("{}({})", ty_name(ty)?, vals.join(", ")),
                    ty.clone(),
                ))
            }
            Expr::Call(name, args) => self.call(name, args),
            Expr::Assign(..) | Expr::Step(..) => {
                Err(err("assignments inside expressions are not supported"))
            }
        }
    }

    fn binary(&mut self, op: &'static str, a: Te, b: Te) -> Result<Te> {
        let prec = binary_prec(op);
        let wrap = |t: &Te, right: bool| -> String {
            let need = match t.op {
                None => t.prec < 100 && t.prec < prec,
                Some(cop) => {
                    let cp = binary_prec(cop);
                    if is_logical(op) || is_logical(cop) {
                        cop != op || right
                    } else if is_relational(op) {
                        true
                    } else {
                        cp < prec || (cp == prec && right)
                    }
                }
            };
            if need {
                format!("({})", t.s)
            } else {
                t.s.clone()
            }
        };
        let make = |s: String, ty: Ty| Te {
            s,
            ty,
            int_lit: None,
            atomic: false,
            prec,
            op: Some(op),
        };
        match op {
            "&&" | "||" | "^^" => {
                let a2 = Te {
                    s: self.coerce(&a, &Ty::bool())?,
                    ..a.clone()
                };
                let b2 = Te {
                    s: self.coerce(&b, &Ty::bool())?,
                    ..b.clone()
                };
                let wop = if op == "^^" { "!=" } else { op };
                Ok(make(
                    format!("{} {wop} {}", wrap(&a2, false), wrap(&b2, true)),
                    Ty::bool(),
                ))
            }
            "==" | "!=" | "<" | ">" | "<=" | ">=" => {
                let ty = common_type(&a.ty, &b.ty)
                    .ok_or_else(|| err(format!("comparison of {:?} and {:?}", a.ty, b.ty)))?;
                let a2 = Te {
                    s: self.coerce(&a, &ty)?,
                    ..a.clone()
                };
                let b2 = Te {
                    s: self.coerce(&b, &ty)?,
                    ..b.clone()
                };
                let s = format!("{} {op} {}", wrap(&a2, false), wrap(&b2, true));
                if let Ty::Vec(..) = ty {
                    // GLSL compares the whole vectors.
                    let f = if op == "!=" { "any" } else { "all" };
                    return Ok(Te::atom(format!("{f}({s})"), Ty::bool()));
                }
                Ok(make(s, Ty::bool()))
            }
            _ => {
                // Arithmetic.
                let (ta, tb, ty) = arith_types(op, &a.ty, &b.ty)
                    .ok_or_else(|| err(format!("operator {op} on {:?} and {:?}", a.ty, b.ty)))?;
                let a2 = Te {
                    s: self.coerce(&a, &ta)?,
                    ..a.clone()
                };
                let b2 = Te {
                    s: self.coerce(&b, &tb)?,
                    ..b.clone()
                };
                let mut te = make(format!("{} {op} {}", wrap(&a2, false), wrap(&b2, true)), ty);
                if let (Some(x), Some(y)) = (a.int_lit, b.int_lit) {
                    te.int_lit = match op {
                        "+" => x.checked_add(y),
                        "-" => x.checked_sub(y),
                        "*" => x.checked_mul(y),
                        _ => None,
                    };
                    if let Some(v) = te.int_lit {
                        te.s = v.to_string();
                        if v < 0 {
                            te.s = format!("({v})");
                        }
                        te.atomic = true;
                        te.prec = 100;
                        te.op = None;
                    }
                }
                Ok(te)
            }
        }
    }

    fn call(&mut self, name: &str, args: &[Expr]) -> Result<Te> {
        // User functions first (they may hide a built-in).
        if let Some(overloads) = self.functions.get(name).cloned() {
            let vals = args
                .iter()
                .map(|a| self.expr(a))
                .collect::<Result<Vec<_>>>()?;
            let sig = overloads
                .iter()
                .find(|s| {
                    s.params.len() == vals.len()
                        && s.params.iter().zip(&vals).all(|((_, t), v)| &v.ty == t)
                })
                .or_else(|| overloads.iter().find(|s| s.params.len() == vals.len()))
                .ok_or_else(|| err(format!("no overload of '{name}' matches")))?
                .clone();
            let mut out = Vec::new();
            for ((qual, ty), (v, a)) in sig.params.iter().zip(vals.iter().zip(args)) {
                match qual {
                    ParamQual::In => out.push(self.coerce(v, ty)?),
                    _ => {
                        let s = match a {
                            Expr::Ident(n) => match self.lookup(n) {
                                Some(Binding::Value { name, .. }) => format!("&{name}"),
                                Some(Binding::Pointer { name, .. }) => name.clone(),
                                _ => return Err(err(format!("'{n}' cannot be an out parameter"))),
                            },
                            _ => return Err(err("out parameters must be plain variables")),
                        };
                        out.push(s);
                    }
                }
            }
            return Ok(Te::atom(
                format!("{}({})", sig.wgsl_name, out.join(", ")),
                sig.ret.clone(),
            ));
        }

        // Constructors.
        if let Some(ty) = Ty::from_keyword(name) {
            return self.constructor(&ty, args);
        }

        if name == "texture" || name == "texture1D" || name == "texture2D" || name == "texture3D" {
            if args.len() != 2 {
                return Err(err("texture() takes two arguments"));
            }
            let Expr::Ident(sampler) = &args[0] else {
                return Err(err("texture() needs a sampler name"));
            };
            let Some(Binding::Sampler(idx)) = self.lookup(sampler).cloned() else {
                return Err(err(format!("'{sampler}' is not a sampler")));
            };
            let t = &self.input.textures[idx];
            let cty = Ty::vec_or_scalar(t.dimensions as u8, Scalar::Float);
            let tex = sanitize(&t.texture_name);
            let coords = self.expr_to(&args[1], &cty)?;
            return Ok(Te::atom(
                format!("{tex}_lookup({coords})"),
                Ty::Vec(4, Scalar::Float),
            ));
        }

        let vals = args
            .iter()
            .map(|a| self.expr(a))
            .collect::<Result<Vec<_>>>()?;

        match name {
            "lessThan" | "lessThanEqual" | "greaterThan" | "greaterThanEqual" | "equal"
            | "notEqual" => {
                if vals.len() != 2 {
                    return Err(err(format!("{name}() takes two arguments")));
                }
                let op = match name {
                    "lessThan" => "<",
                    "lessThanEqual" => "<=",
                    "greaterThan" => ">",
                    "greaterThanEqual" => ">=",
                    "equal" => "==",
                    _ => "!=",
                };
                let ty = common_type(&vals[0].ty, &vals[1].ty)
                    .ok_or_else(|| err(format!("{name}() of different types")))?;
                let n = ty.width().unwrap_or(1);
                let a = self.coerce(&vals[0], &ty)?;
                let b = self.coerce(&vals[1], &ty)?;
                return Ok(Te::atom(
                    format!("({a} {op} {b})"),
                    Ty::vec_or_scalar(n, Scalar::Bool),
                ));
            }
            "any" | "all" | "not" => {
                if vals.len() != 1 {
                    return Err(err(format!("{name}() takes one argument")));
                }
                if name == "not" {
                    return Ok(Te::atom(format!("!({})", vals[0].s), vals[0].ty.clone()));
                }
                return Ok(Te::atom(format!("{name}({})", vals[0].s), Ty::bool()));
            }
            "isnan" => {
                if vals.len() != 1 {
                    return Err(err("isnan() takes one argument"));
                }
                let n = vals[0].ty.width().unwrap_or(1);
                return Ok(Te::atom(
                    format!("({0} != {0})", vals[0].postfix()),
                    Ty::vec_or_scalar(n, Scalar::Bool),
                ));
            }
            "mod" => {
                if vals.len() != 2 {
                    return Err(err("mod() takes two arguments"));
                }
                let ty = float_of(&wider(&vals[0].ty, &vals[1].ty));
                let a = self.coerce(&vals[0], &ty)?;
                let b = self.coerce(&vals[1], &ty)?;
                return Ok(Te::atom(format!("({a} - {b} * floor({a} / {b}))"), ty));
            }
            "dot" | "length" | "distance" | "cross" | "normalize" | "reflect" => {
                let ty = float_of(&vals.iter().fold(Ty::float(), |acc, v| wider(&acc, &v.ty)));
                let a = vals
                    .iter()
                    .map(|v| self.coerce(v, &ty))
                    .collect::<Result<Vec<_>>>()?;
                let ret = match name {
                    "dot" | "length" | "distance" => Ty::float(),
                    _ => ty,
                };
                return Ok(Te::atom(format!("{name}({})", a.join(", ")), ret));
            }
            _ => {}
        }

        let (wname, int_ok) = match name {
            "abs" | "sign" | "min" | "max" | "clamp" => (name, true),
            "atan" if vals.len() == 2 => ("atan2", false),
            "inversesqrt" => ("inverseSqrt", false),
            "floor" | "ceil" | "fract" | "trunc" | "round" | "roundEven" | "exp" | "exp2"
            | "log" | "log2" | "sqrt" | "sin" | "cos" | "tan" | "asin" | "acos" | "atan"
            | "sinh" | "cosh" | "tanh" | "pow" | "mix" | "step" | "smoothstep" | "degrees"
            | "radians" | "fma" => (name, false),
            _ => return Err(err(format!("unsupported function '{name}'"))),
        };
        if vals.is_empty() {
            return Err(err(format!("{name}() without argument")));
        }
        let mut ty = vals
            .iter()
            .skip(1)
            .fold(vals[0].ty.clone(), |acc, v| wider(&acc, &v.ty));
        let any_float = vals.iter().any(|v| v.ty.scalar() == Some(Scalar::Float));
        if !int_ok || any_float {
            ty = float_of(&ty);
        }
        let a = vals
            .iter()
            .map(|v| self.coerce(v, &ty))
            .collect::<Result<Vec<_>>>()?;
        Ok(Te::atom(format!("{wname}({})", a.join(", ")), ty))
    }

    fn constructor(&mut self, ty: &Ty, args: &[Expr]) -> Result<Te> {
        let vals = args
            .iter()
            .map(|a| self.expr(a))
            .collect::<Result<Vec<_>>>()?;
        match ty {
            Ty::Scalar(s) => {
                let [v] = vals.as_slice() else {
                    return Err(err("scalar constructor with several arguments"));
                };
                if v.ty == *ty {
                    return Ok(v.clone());
                }
                if *s == Scalar::Float {
                    if let Some(i) = v.int_lit {
                        return Ok(Te::atom(float_literal(i as f32)?, ty.clone()));
                    }
                }
                let v = match v.ty {
                    Ty::Vec(..) => format!("{}.x", v.postfix()),
                    _ => v.s.clone(),
                };
                Ok(Te::atom(format!("{}({v})", scalar_name(*s)), ty.clone()))
            }
            Ty::Vec(n, s) => {
                let tn = ty_name(ty)?;
                if let [v] = vals.as_slice() {
                    match &v.ty {
                        Ty::Scalar(_) => {
                            let inner = self.coerce(v, &Ty::Scalar(*s))?;
                            return Ok(Te::atom(format!("{tn}({inner})"), ty.clone()));
                        }
                        Ty::Vec(m, vs) => {
                            let src = if m > n {
                                let sw: String = COMPONENTS[..*n as usize].concat();
                                format!("{}.{sw}", v.postfix())
                            } else {
                                v.s.clone()
                            };
                            if vs == s && m >= n {
                                return Ok(Te::atom(src, ty.clone()));
                            }
                            return Ok(Te::atom(format!("{tn}({src})"), ty.clone()));
                        }
                        t => return Err(err(format!("vector constructor from {t:?}"))),
                    }
                }
                let parts = vals
                    .iter()
                    .map(|v| {
                        let w = v.ty.width().unwrap_or(1);
                        self.coerce(v, &Ty::vec_or_scalar(w, *s))
                    })
                    .collect::<Result<Vec<_>>>()?;
                Ok(Te::atom(format!("{tn}({})", parts.join(", ")), ty.clone()))
            }
            Ty::Mat(n) => {
                let tn = ty_name(ty)?;
                let parts = vals
                    .iter()
                    .map(|v| match v.ty {
                        Ty::Vec(..) => self.coerce(v, &Ty::Vec(*n, Scalar::Float)),
                        _ => self.coerce(v, &Ty::float()),
                    })
                    .collect::<Result<Vec<_>>>()?;
                if parts.len() == 1 && vals[0].ty.width() == Some(1) {
                    // Diagonal matrix.
                    let d = &parts[0];
                    let mut cols = Vec::new();
                    for c in 0..*n {
                        let comps: Vec<String> = (0..*n)
                            .map(|r| {
                                if r == c {
                                    d.clone()
                                } else {
                                    "0.0f".to_string()
                                }
                            })
                            .collect();
                        cols.push(format!("vec{n}<f32>({})", comps.join(", ")));
                    }
                    return Ok(Te::atom(format!("{tn}({})", cols.join(", ")), ty.clone()));
                }
                Ok(Te::atom(format!("{tn}({})", parts.join(", ")), ty.clone()))
            }
            t => Err(err(format!("constructor of {t:?}"))),
        }
    }
}

/// An assignment target.
#[derive(Debug, Clone)]
struct LValue {
    /// The reference expression (a variable, an element, a dereference).
    base: String,
    /// The type of the reference.
    ty: Ty,
    /// The components assigned, if any.
    swizzle: Option<Vec<usize>>,
}

impl LValue {
    fn text(&self) -> String {
        match &self.swizzle {
            None => self.base.clone(),
            Some(idx) => {
                let s: String = idx.iter().map(|i| COMPONENTS[*i]).collect();
                format!("{}.{s}", self.base)
            }
        }
    }
}

const COMPONENTS: [&str; 4] = ["x", "y", "z", "w"];

fn swizzle_indices(m: &str, width: u8) -> Result<Vec<usize>> {
    if m.is_empty() || m.len() > 4 {
        return Err(err(format!("invalid swizzle '{m}'")));
    }
    m.chars()
        .map(|c| {
            let i = match c {
                'x' | 'r' | 's' => 0,
                'y' | 'g' | 't' => 1,
                'z' | 'b' | 'p' => 2,
                'w' | 'a' | 'q' => 3,
                _ => return Err(err(format!("invalid swizzle '{m}'"))),
            };
            if i >= width as usize {
                return Err(err(format!("swizzle '{m}' out of a vector of {width}")));
            }
            Ok(i)
        })
        .collect()
}

fn binary_prec(op: &str) -> u8 {
    match op {
        "||" => 1,
        "^^" => 2,
        "&&" => 3,
        "==" | "!=" => 7,
        "<" | ">" | "<=" | ">=" => 8,
        "+" | "-" => 10,
        "*" | "/" | "%" => 11,
        _ => 12,
    }
}

fn is_logical(op: &str) -> bool {
    matches!(op, "&&" | "||" | "^^")
}

fn is_relational(op: &str) -> bool {
    matches!(op, "==" | "!=" | "<" | ">" | "<=" | ">=")
}

/// The float version of a scalar / vector type.
fn float_of(ty: &Ty) -> Ty {
    match ty {
        Ty::Scalar(_) => Ty::float(),
        Ty::Vec(n, _) => Ty::Vec(*n, Scalar::Float),
        t => t.clone(),
    }
}

/// The type with the most components, float winning over int.
fn wider(a: &Ty, b: &Ty) -> Ty {
    let wa = a.width().unwrap_or(1);
    let wb = b.width().unwrap_or(1);
    let s = match (a.scalar(), b.scalar()) {
        (Some(Scalar::Float), _) | (_, Some(Scalar::Float)) => Scalar::Float,
        (Some(x), _) => x,
        (None, Some(y)) => y,
        _ => Scalar::Float,
    };
    Ty::vec_or_scalar(wa.max(wb), s)
}

/// The common type of the two branches of a ternary / sides of a
/// comparison.
fn common_type(a: &Ty, b: &Ty) -> Option<Ty> {
    if a == b {
        return Some(a.clone());
    }
    match (a, b) {
        (Ty::Scalar(_) | Ty::Vec(..), Ty::Scalar(_) | Ty::Vec(..)) => {
            let wa = a.width()?;
            let wb = b.width()?;
            if wa != wb && wa != 1 && wb != 1 {
                return None;
            }
            Some(wider(a, b))
        }
        _ => None,
    }
}

/// Operand and result types of an arithmetic operator.
fn arith_types(op: &str, a: &Ty, b: &Ty) -> Option<(Ty, Ty, Ty)> {
    match (a, b) {
        (Ty::Mat(n), Ty::Mat(m)) if n == m => Some((a.clone(), b.clone(), a.clone())),
        (Ty::Mat(n), Ty::Vec(m, _)) if n == m && op == "*" => {
            let v = Ty::Vec(*m, Scalar::Float);
            Some((a.clone(), v.clone(), v))
        }
        (Ty::Vec(m, _), Ty::Mat(n)) if n == m && op == "*" => {
            let v = Ty::Vec(*m, Scalar::Float);
            Some((v.clone(), b.clone(), v))
        }
        (Ty::Mat(_), Ty::Scalar(_)) => Some((a.clone(), Ty::float(), a.clone())),
        (Ty::Scalar(_), Ty::Mat(_)) => Some((Ty::float(), b.clone(), b.clone())),
        (Ty::Scalar(_) | Ty::Vec(..), Ty::Scalar(_) | Ty::Vec(..)) => {
            let wa = a.width()?;
            let wb = b.width()?;
            if wa != wb && wa != 1 && wb != 1 {
                return None;
            }
            let s = match (a.scalar()?, b.scalar()?) {
                (Scalar::Float, _) | (_, Scalar::Float) => Scalar::Float,
                (Scalar::Int, Scalar::Int) => Scalar::Int,
                _ => return None,
            };
            let ta = Ty::vec_or_scalar(wa, s);
            let tb = Ty::vec_or_scalar(wb, s);
            Some((ta, tb, Ty::vec_or_scalar(wa.max(wb), s)))
        }
        _ => None,
    }
}

fn root_ident(e: &Expr) -> Option<&str> {
    match e {
        Expr::Ident(n) => Some(n),
        Expr::Member(b, _) | Expr::Index(b, _) => root_ident(b),
        _ => None,
    }
}

fn collect_assigned_expr(e: &Expr, out: &mut HashSet<String>) {
    match e {
        Expr::Assign(_, l, r) => {
            if let Some(n) = root_ident(l) {
                out.insert(n.to_string());
            }
            collect_assigned_expr(r, out);
        }
        Expr::Step(_, t) => {
            if let Some(n) = root_ident(t) {
                out.insert(n.to_string());
            }
        }
        _ => {}
    }
}

fn collect_assigned_stmt(s: &Stmt, out: &mut HashSet<String>) {
    match s {
        Stmt::Expr(e) => collect_assigned_expr(e, out),
        Stmt::Block(b) => b.iter().for_each(|s| collect_assigned_stmt(s, out)),
        Stmt::If(_, t, e) => {
            collect_assigned_stmt(t, out);
            if let Some(e) = e {
                collect_assigned_stmt(e, out);
            }
        }
        Stmt::For(i, _, u, b) => {
            if let Some(i) = i {
                collect_assigned_stmt(i, out);
            }
            if let Some(u) = u {
                collect_assigned_expr(u, out);
            }
            collect_assigned_stmt(b, out);
        }
        Stmt::While(_, b) | Stmt::DoWhile(b, _) => collect_assigned_stmt(b, out),
        _ => {}
    }
}

/// Write a short description of the resources as a comment header.
pub(crate) fn bindings_comment(b: &WgslBindings) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "// WGSL module generated by ocio-rs.");
    if let Some(u) = b.uniform_binding {
        let _ = writeln!(
            s,
            "// @group({}) @binding({u}): uniform buffer, {} bytes.",
            b.group, b.uniform_buffer_size
        );
    }
    for t in &b.textures {
        let _ = writeln!(
            s,
            "// @group({}) @binding({}): {}D texture '{}' ({:?}, {}x{}x{}), not filtered.",
            t.group, t.binding, t.dimensions, t.texture_name, t.format, t.width, t.height, t.depth
        );
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lower_src(src: &str) -> String {
        let input = LowerInput {
            source: src,
            function_name: "F",
            group: 0,
            textures: &[],
        };
        lower(&input).unwrap().0
    }

    #[test]
    fn ternary_and_swizzles() {
        let w = lower_src(
            "vec4 F(vec4 inPixel)\n{\n  vec4 outColor = inPixel;\n  outColor.rgb = max(outColor.rgb, 0.);\n  outColor.rgb.r = (outColor.r < 0.5) ? 1 : outColor.g;\n  outColor.gbr = outColor.rgb;\n  return outColor;\n}\n",
        );
        assert!(w.contains("fn F(inPixel: vec4<f32>) -> vec4<f32> {"), "{w}");
        assert!(
            w.contains("outColor = vec4<f32>(max(outColor.xyz, vec3<f32>(0.0f)), outColor.w);"),
            "{w}"
        );
        assert!(
            w.contains("outColor.x = select(outColor.y, 1.0f, outColor.x < 0.5f);"),
            "{w}"
        );
        assert!(w.contains("let swz_1 = outColor.xyz;"), "{w}");
        assert!(
            w.contains("outColor = vec4<f32>(swz_1.z, swz_1.x, swz_1.y, outColor.w);"),
            "{w}"
        );
    }

    #[test]
    fn literals() {
        assert_eq!(float_literal(1.0).unwrap(), "1.0f");
        assert_eq!(float_literal(-0.5).unwrap(), "(-0.5f)");
        assert_eq!(float_literal(1e20).unwrap(), "1e20f");
        assert_eq!(parse_float_text("1e+20.").unwrap(), 1e20);
        assert!(float_literal(f32::INFINITY).is_err());
    }

    fn lower_err(src: &str) -> String {
        let input = LowerInput {
            source: src,
            function_name: "F",
            group: 0,
            textures: &[],
        };
        lower(&input).unwrap_err().to_string()
    }

    #[test]
    fn unsupported_input_is_an_error() {
        // Unknown built-in.
        let e = lower_err("float F(float a)\n{\n  return frobnicate(a);\n}\n");
        assert!(e.contains("unsupported function 'frobnicate'"), "{e}");
        // A texture the shader creator does not know.
        let e = lower_err("uniform sampler3D lutSampler;\nvec4 F(vec4 p)\n{\n  return texture(lutSampler, p.rgb);\n}\n");
        assert!(e.contains("unknown texture sampler 'lutSampler'"), "{e}");
        // Uniform blocks.
        let e = lower_err("uniform Block { float a; };\n");
        assert!(e.contains("uniform blocks"), "{e}");
        // Values without a WGSL literal.
        let e = lower_err("float F(float a)\n{\n  return a * 1e39;\n}\n");
        assert!(e.contains("no WGSL f32 literal"), "{e}");
    }

    #[test]
    fn textures_and_uniforms() {
        let textures = [TextureSource {
            texture_name: "lut".to_string(),
            sampler_name: "lutSampler".to_string(),
            dimensions: 1,
            width: 4,
            height: 1,
            depth: 1,
            channel: TextureType::RedChannel,
            interpolation: Interpolation::Linear,
            binding: 1,
        }];
        let input = LowerInput {
            source: "uniform float gain;\nuniform bool flag;\nuniform vec3 tint;\nuniform float arr[4];\nuniform sampler1D lutSampler;\nvec4 F(vec4 p)\n{\n  vec4 outColor = p;\n  if (flag) outColor.r = texture(lutSampler, outColor.r).r * gain * arr[1];\n  outColor.rgb *= tint;\n  return outColor;\n}\n",
            function_name: "F",
            group: 3,
            textures: &textures,
        };
        let (w, b) = lower(&input).unwrap();
        assert!(
            w.contains("@group(3) @binding(0) var<uniform> F_params: F_Parameters;"),
            "{w}"
        );
        assert!(
            w.contains("@group(3) @binding(1) var lut: texture_1d<f32>;"),
            "{w}"
        );
        assert!(w.contains("if ((F_params.flag != 0)) {"), "{w}");
        assert!(
            w.contains(
                "outColor.x = lut_lookup(outColor.x).x * F_params.gain * F_params.arr[1].x;"
            ),
            "{w}"
        );
        assert!(
            w.contains("outColor = vec4<f32>(outColor.xyz * F_params.tint, outColor.w);"),
            "{w}"
        );
        let offsets: Vec<(u32, u32)> = b.uniforms.iter().map(|u| (u.offset, u.size)).collect();
        assert_eq!(offsets, vec![(0, 4), (4, 4), (16, 12), (32, 64)]);
        assert_eq!(b.uniform_buffer_size, 96);
        assert_eq!(b.textures.len(), 1);
        assert_eq!(b.textures[0].format, WgslTextureFormat::R32Float);
    }

    /// `vec3(greaterThan(a, b))` (the 0 / 1 masks of the log and fixed
    /// function renderers) is a WGSL conversion of a `vec3<bool>` to
    /// `vec3<f32>`, valid for naga.
    #[test]
    fn bool_vector_masks_validate() {
        let w = lower_src(
            "vec4 F(vec4 inPixel)\n{\n  vec4 outColor = inPixel;\n  vec3 isAboveBreak = vec3(greaterThan(outColor.rgb, vec3(0.18)));\n  outColor.rgb = isAboveBreak;\n  return outColor;\n}\n",
        );
        assert!(
            w.contains(
                "var isAboveBreak: vec3<f32> = vec3<f32>((outColor.xyz > vec3<f32>(0.18f)));"
            ),
            "{w}"
        );
        let module = naga::front::wgsl::parse_str(&w).unwrap();
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
    }

    #[test]
    fn precedence() {
        let w = lower_src(
            "float F(float a, float b)\n{\n  return (a - (b - 1.)) * (a + b) / -(a * b);\n}\n",
        );
        assert!(
            w.contains("return (a - (b - 1.0f)) * (a + b) / -(a * b);"),
            "{w}"
        );
    }
}
