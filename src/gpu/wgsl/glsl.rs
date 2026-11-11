// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Lexer, syntax tree and parser of the GLSL 4.0 subset written by the GPU
//! renderers (the source of the WGSL lowering).

use crate::error::{Error, Result};

/// A token of the source program.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Tok {
    Ident(String),
    /// A number, as written (`is_float` is true when it has a dot or an
    /// exponent).
    Num(String, bool),
    Punct(&'static str),
}

const PUNCTS: &[&str] = &[
    "<<=", ">>=", "++", "--", "+=", "-=", "*=", "/=", "%=", "<=", ">=", "==", "!=", "&&", "||",
    "^^", "(", ")", "{", "}", "[", "]", ";", ",", ".", "+", "-", "*", "/", "%", "<", ">", "=", "!",
    "?", ":", "&", "|", "^", "~",
];

fn err(msg: impl Into<String>) -> Error {
    Error::msg(format!("WGSL translation: {}", msg.into()))
}

/// Split the program into tokens (comments and preprocessor lines are
/// dropped).
pub(crate) fn tokenize(src: &str) -> Result<Vec<Tok>> {
    let b = src.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    let mut line_start = true;
    while i < b.len() {
        let c = b[i];
        if c == b'\n' {
            line_start = true;
            i += 1;
            continue;
        }
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if line_start && c == b'#' {
            // Preprocessor directive (e.g. `#version`): not part of the
            // program.
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        line_start = false;
        if c == b'/' && b.get(i + 1) == Some(&b'/') {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if c == b'/' && b.get(i + 1) == Some(&b'*') {
            i += 2;
            while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                i += 1;
            }
            i += 2;
            continue;
        }
        if c.is_ascii_alphabetic() || c == b'_' {
            let start = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            out.push(Tok::Ident(src[start..i].to_string()));
            continue;
        }
        if c.is_ascii_digit() || (c == b'.' && b.get(i + 1).is_some_and(|d| d.is_ascii_digit())) {
            let start = i;
            let mut is_float = false;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            if i < b.len() && b[i] == b'.' {
                is_float = true;
                i += 1;
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
            }
            if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
                let mut j = i + 1;
                if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
                    j += 1;
                }
                if j < b.len() && b[j].is_ascii_digit() {
                    is_float = true;
                    i = j;
                    while i < b.len() && b[i].is_ascii_digit() {
                        i += 1;
                    }
                    // OCIO adds a dot to integral values, even after an
                    // exponent (e.g. `1e+20.`).
                    if i < b.len() && b[i] == b'.' {
                        i += 1;
                    }
                }
            }
            // Float suffixes.
            if i < b.len() && (b[i] == b'f' || b[i] == b'F') {
                is_float = true;
                i += 1;
            }
            out.push(Tok::Num(src[start..i].to_string(), is_float));
            continue;
        }
        let rest = &src[i..];
        match PUNCTS.iter().find(|p| rest.starts_with(**p)) {
            Some(p) => {
                out.push(Tok::Punct(p));
                i += p.len();
            }
            None => {
                return Err(err(format!(
                    "unexpected character '{}'",
                    rest.chars().next().unwrap_or(' ')
                )))
            }
        }
    }
    Ok(out)
}

/// Scalar kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Scalar {
    Bool,
    Int,
    Float,
}

/// Types of the source program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Ty {
    Void,
    Scalar(Scalar),
    Vec(u8, Scalar),
    /// Square float matrix.
    Mat(u8),
    Array(Box<Ty>, usize),
    /// A combined texture sampler of 1, 2 or 3 dimensions.
    Sampler(u8),
}

impl Ty {
    pub(crate) fn float() -> Ty {
        Ty::Scalar(Scalar::Float)
    }

    pub(crate) fn int() -> Ty {
        Ty::Scalar(Scalar::Int)
    }

    pub(crate) fn bool() -> Ty {
        Ty::Scalar(Scalar::Bool)
    }

    /// The scalar kind of a scalar or a vector.
    pub(crate) fn scalar(&self) -> Option<Scalar> {
        match self {
            Ty::Scalar(s) | Ty::Vec(_, s) => Some(*s),
            Ty::Mat(_) => Some(Scalar::Float),
            _ => None,
        }
    }

    /// Number of components of a vector (1 for a scalar).
    pub(crate) fn width(&self) -> Option<u8> {
        match self {
            Ty::Scalar(_) => Some(1),
            Ty::Vec(n, _) => Some(*n),
            _ => None,
        }
    }

    /// A scalar (`n == 1`) or a vector of `n` components.
    pub(crate) fn vec_or_scalar(n: u8, s: Scalar) -> Ty {
        if n == 1 {
            Ty::Scalar(s)
        } else {
            Ty::Vec(n, s)
        }
    }

    /// Parse a GLSL type keyword.
    pub(crate) fn from_keyword(kw: &str) -> Option<Ty> {
        Some(match kw {
            "void" => Ty::Void,
            "float" | "half" | "double" => Ty::float(),
            "int" | "uint" => Ty::int(),
            "bool" => Ty::bool(),
            "vec2" => Ty::Vec(2, Scalar::Float),
            "vec3" => Ty::Vec(3, Scalar::Float),
            "vec4" => Ty::Vec(4, Scalar::Float),
            "ivec2" => Ty::Vec(2, Scalar::Int),
            "ivec3" => Ty::Vec(3, Scalar::Int),
            "ivec4" => Ty::Vec(4, Scalar::Int),
            "bvec2" => Ty::Vec(2, Scalar::Bool),
            "bvec3" => Ty::Vec(3, Scalar::Bool),
            "bvec4" => Ty::Vec(4, Scalar::Bool),
            "mat2" => Ty::Mat(2),
            "mat3" => Ty::Mat(3),
            "mat4" => Ty::Mat(4),
            "sampler1D" => Ty::Sampler(1),
            "sampler2D" => Ty::Sampler(2),
            "sampler3D" => Ty::Sampler(3),
            _ => return None,
        })
    }
}

/// Expressions.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Expr {
    Num(String, bool),
    Bool(bool),
    Ident(String),
    Unary(&'static str, Box<Expr>),
    Binary(&'static str, Box<Expr>, Box<Expr>),
    Ternary(Box<Expr>, Box<Expr>, Box<Expr>),
    /// Function call or constructor.
    Call(String, Vec<Expr>),
    /// Array constructor, e.g. `float[3](...)`.
    ArrayCtor(Ty, Vec<Expr>),
    Member(Box<Expr>, String),
    Index(Box<Expr>, Box<Expr>),
    /// Assignment (`=` or a compound operator such as `+=`).
    Assign(&'static str, Box<Expr>, Box<Expr>),
    /// `++` / `--`, prefix or postfix (same meaning as statements).
    Step(&'static str, Box<Expr>),
}

/// A declared variable.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Declarator {
    pub name: String,
    pub ty: Ty,
    pub init: Option<Expr>,
}

/// Statements.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Stmt {
    Decl {
        is_const: bool,
        vars: Vec<Declarator>,
    },
    Expr(Expr),
    Block(Vec<Stmt>),
    If(Expr, Box<Stmt>, Option<Box<Stmt>>),
    For(Option<Box<Stmt>>, Option<Expr>, Option<Expr>, Box<Stmt>),
    While(Expr, Box<Stmt>),
    DoWhile(Box<Stmt>, Expr),
    Return(Option<Expr>),
    Break,
    Continue,
    Empty,
}

/// Parameter qualifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParamQual {
    In,
    Out,
    InOut,
}

/// A function parameter.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Param {
    pub qual: ParamQual,
    pub ty: Ty,
    pub name: String,
}

/// A function definition.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Function {
    pub ret: Ty,
    pub name: String,
    pub params: Vec<Param>,
    pub body: Vec<Stmt>,
}

/// Module scope items.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Item {
    Uniform(Declarator),
    Global { is_const: bool, var: Declarator },
    Function(Function),
}

/// Parse the program.
pub(crate) fn parse(src: &str) -> Result<Vec<Item>> {
    let toks = tokenize(src)?;
    let mut p = Parser { toks, pos: 0 };
    let mut items = Vec::new();
    while !p.at_end() {
        p.item(&mut items)?;
    }
    Ok(items)
}

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
}

const QUALIFIERS: &[&str] = &[
    "highp",
    "mediump",
    "lowp",
    "precise",
    "flat",
    "smooth",
    "noperspective",
];

impl Parser {
    fn at_end(&self) -> bool {
        self.pos >= self.toks.len()
    }

    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }

    fn peek_at(&self, n: usize) -> Option<&Tok> {
        self.toks.get(self.pos + n)
    }

    fn is_punct(&self, p: &str) -> bool {
        matches!(self.peek(), Some(Tok::Punct(q)) if *q == p)
    }

    fn is_ident(&self, s: &str) -> bool {
        matches!(self.peek(), Some(Tok::Ident(q)) if q == s)
    }

    fn eat_punct(&mut self, p: &str) -> bool {
        if self.is_punct(p) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn eat_ident(&mut self, s: &str) -> bool {
        if self.is_ident(s) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, p: &str) -> Result<()> {
        if self.eat_punct(p) {
            Ok(())
        } else {
            Err(err(format!(
                "expected '{p}', found {:?} (token {})",
                self.peek(),
                self.pos
            )))
        }
    }

    fn ident(&mut self) -> Result<String> {
        match self.peek() {
            Some(Tok::Ident(s)) => {
                let s = s.clone();
                self.pos += 1;
                Ok(s)
            }
            t => Err(err(format!("expected an identifier, found {t:?}"))),
        }
    }

    fn skip_qualifiers(&mut self) {
        while let Some(Tok::Ident(s)) = self.peek() {
            if QUALIFIERS.contains(&s.as_str()) {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    /// Skip a `layout(...)` qualifier.
    fn skip_layout(&mut self) -> Result<()> {
        if self.eat_ident("layout") {
            self.expect("(")?;
            let mut depth = 1;
            while depth > 0 {
                match self.peek() {
                    None => return Err(err("unterminated layout qualifier")),
                    Some(Tok::Punct("(")) => depth += 1,
                    Some(Tok::Punct(")")) => depth -= 1,
                    _ => {}
                }
                self.pos += 1;
            }
        }
        Ok(())
    }

    fn type_at(&self, n: usize) -> Option<Ty> {
        match self.peek_at(n) {
            Some(Tok::Ident(s)) => Ty::from_keyword(s),
            _ => None,
        }
    }

    fn parse_type(&mut self) -> Result<Ty> {
        self.skip_qualifiers();
        let kw = self.ident()?;
        Ty::from_keyword(&kw).ok_or_else(|| err(format!("unknown type '{kw}'")))
    }

    /// Optional array size after a name or a type.
    fn array_suffix(&mut self, ty: Ty) -> Result<Ty> {
        if self.eat_punct("[") {
            let n = match self.peek() {
                Some(Tok::Num(s, false)) => s
                    .parse::<usize>()
                    .map_err(|_| err(format!("invalid array size '{s}'")))?,
                t => return Err(err(format!("unsupported array size {t:?}"))),
            };
            self.pos += 1;
            self.expect("]")?;
            Ok(Ty::Array(Box::new(ty), n))
        } else {
            Ok(ty)
        }
    }

    fn item(&mut self, items: &mut Vec<Item>) -> Result<()> {
        if self.eat_punct(";") {
            return Ok(());
        }
        self.skip_layout()?;
        if self.eat_ident("uniform") {
            if self.is_punct("{") || matches!(self.peek_at(1), Some(Tok::Punct("{"))) {
                return Err(err("uniform blocks are not supported"));
            }
            let ty = self.parse_type()?;
            loop {
                let name = self.ident()?;
                let ty = self.array_suffix(ty.clone())?;
                items.push(Item::Uniform(Declarator {
                    name,
                    ty,
                    init: None,
                }));
                if !self.eat_punct(",") {
                    break;
                }
            }
            self.expect(";")?;
            return Ok(());
        }
        let is_const = self.eat_ident("const");
        let base = self.parse_type()?;
        let base = self.array_suffix(base)?;
        let name = self.ident()?;
        if self.is_punct("(") {
            // Function prototype or definition.
            self.pos += 1;
            let mut params = Vec::new();
            if !self.is_punct(")") {
                if self.is_ident("void") && matches!(self.peek_at(1), Some(Tok::Punct(")"))) {
                    self.pos += 1;
                } else {
                    loop {
                        let mut qual = ParamQual::In;
                        loop {
                            if self.eat_ident("in") || self.eat_ident("const") {
                                continue;
                            }
                            if self.eat_ident("out") {
                                qual = ParamQual::Out;
                                continue;
                            }
                            if self.eat_ident("inout") {
                                qual = ParamQual::InOut;
                                continue;
                            }
                            break;
                        }
                        let ty = self.parse_type()?;
                        let ty = self.array_suffix(ty)?;
                        let pname = self.ident()?;
                        let ty = self.array_suffix(ty)?;
                        params.push(Param {
                            qual,
                            ty,
                            name: pname,
                        });
                        if !self.eat_punct(",") {
                            break;
                        }
                    }
                }
            }
            self.expect(")")?;
            if self.eat_punct(";") {
                // A prototype: the definition follows.
                return Ok(());
            }
            self.expect("{")?;
            let body = self.block_body()?;
            items.push(Item::Function(Function {
                ret: base,
                name,
                params,
                body,
            }));
            return Ok(());
        }
        // Global variables.
        let vars = self.declarators(base, name)?;
        for var in vars {
            items.push(Item::Global { is_const, var });
        }
        Ok(())
    }

    /// Declarators after the type and the first name, up to the `;`.
    fn declarators(&mut self, base: Ty, first: String) -> Result<Vec<Declarator>> {
        let mut vars = Vec::new();
        let mut name = first;
        loop {
            let ty = self.array_suffix(base.clone())?;
            let init = if self.eat_punct("=") {
                Some(self.assignment()?)
            } else {
                None
            };
            vars.push(Declarator { name, ty, init });
            if !self.eat_punct(",") {
                break;
            }
            name = self.ident()?;
        }
        self.expect(";")?;
        Ok(vars)
    }

    fn block_body(&mut self) -> Result<Vec<Stmt>> {
        let mut stmts = Vec::new();
        while !self.eat_punct("}") {
            if self.at_end() {
                return Err(err("unterminated block"));
            }
            stmts.push(self.statement()?);
        }
        Ok(stmts)
    }

    /// True if a declaration starts here.
    fn at_declaration(&self) -> bool {
        let mut n = 0;
        while let Some(Tok::Ident(s)) = self.peek_at(n) {
            if s == "const" || QUALIFIERS.contains(&s.as_str()) {
                n += 1;
            } else {
                break;
            }
        }
        if self.type_at(n).is_none() {
            return false;
        }
        // `vec3 x` or `float[3] x` (but not `vec3(...)` or `float[3](...)`).
        match self.peek_at(n + 1) {
            Some(Tok::Ident(_)) => true,
            Some(Tok::Punct("[")) => {
                // float[3] name
                matches!(self.peek_at(n + 4), Some(Tok::Ident(_)))
            }
            _ => false,
        }
    }

    fn statement(&mut self) -> Result<Stmt> {
        if self.eat_punct(";") {
            return Ok(Stmt::Empty);
        }
        if self.eat_punct("{") {
            return Ok(Stmt::Block(self.block_body()?));
        }
        if self.eat_ident("if") {
            self.expect("(")?;
            let cond = self.expression()?;
            self.expect(")")?;
            let then = Box::new(self.statement()?);
            let els = if self.eat_ident("else") {
                Some(Box::new(self.statement()?))
            } else {
                None
            };
            return Ok(Stmt::If(cond, then, els));
        }
        if self.eat_ident("for") {
            self.expect("(")?;
            let init = if self.eat_punct(";") {
                None
            } else {
                Some(Box::new(self.simple_statement()?))
            };
            let cond = if self.is_punct(";") {
                None
            } else {
                Some(self.expression()?)
            };
            self.expect(";")?;
            let update = if self.is_punct(")") {
                None
            } else {
                Some(self.expression()?)
            };
            self.expect(")")?;
            let body = Box::new(self.statement()?);
            return Ok(Stmt::For(init, cond, update, body));
        }
        if self.eat_ident("while") {
            self.expect("(")?;
            let cond = self.expression()?;
            self.expect(")")?;
            let body = Box::new(self.statement()?);
            return Ok(Stmt::While(cond, body));
        }
        if self.eat_ident("do") {
            let body = Box::new(self.statement()?);
            if !self.eat_ident("while") {
                return Err(err("expected 'while' after a 'do' body"));
            }
            self.expect("(")?;
            let cond = self.expression()?;
            self.expect(")")?;
            self.expect(";")?;
            return Ok(Stmt::DoWhile(body, cond));
        }
        if self.eat_ident("return") {
            if self.eat_punct(";") {
                return Ok(Stmt::Return(None));
            }
            let e = self.expression()?;
            self.expect(";")?;
            return Ok(Stmt::Return(Some(e)));
        }
        if self.eat_ident("break") {
            self.expect(";")?;
            return Ok(Stmt::Break);
        }
        if self.eat_ident("continue") {
            self.expect(";")?;
            return Ok(Stmt::Continue);
        }
        self.simple_statement()
    }

    /// A declaration or an expression statement (with its `;`).
    fn simple_statement(&mut self) -> Result<Stmt> {
        if self.at_declaration() {
            let is_const = self.eat_ident("const");
            let base = self.parse_type()?;
            let base = self.array_suffix(base)?;
            let name = self.ident()?;
            let vars = self.declarators(base, name)?;
            return Ok(Stmt::Decl { is_const, vars });
        }
        let e = self.expression()?;
        self.expect(";")?;
        Ok(Stmt::Expr(e))
    }

    fn expression(&mut self) -> Result<Expr> {
        let e = self.assignment()?;
        if self.is_punct(",") {
            return Err(err("the comma operator is not supported"));
        }
        Ok(e)
    }

    fn assignment(&mut self) -> Result<Expr> {
        let lhs = self.conditional()?;
        for op in ["=", "+=", "-=", "*=", "/=", "%="] {
            if self.is_punct(op) {
                let op: &'static str = match self.peek() {
                    Some(Tok::Punct(p)) => p,
                    _ => unreachable!(),
                };
                self.pos += 1;
                let rhs = self.assignment()?;
                return Ok(Expr::Assign(op, Box::new(lhs), Box::new(rhs)));
            }
        }
        Ok(lhs)
    }

    fn conditional(&mut self) -> Result<Expr> {
        let cond = self.binary(0)?;
        if self.eat_punct("?") {
            let a = self.assignment()?;
            self.expect(":")?;
            let b = self.assignment()?;
            return Ok(Expr::Ternary(Box::new(cond), Box::new(a), Box::new(b)));
        }
        Ok(cond)
    }

    fn binary(&mut self, min_prec: u8) -> Result<Expr> {
        let mut lhs = self.unary()?;
        while let Some((op, prec)) = match self.peek() {
            Some(Tok::Punct(p)) => binary_precedence(p)
                .filter(|prec| *prec >= min_prec)
                .map(|prec| (*p, prec)),
            _ => None,
        } {
            self.pos += 1;
            let rhs = self.binary(prec + 1)?;
            lhs = Expr::Binary(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn unary(&mut self) -> Result<Expr> {
        for op in ["-", "+", "!", "~"] {
            if self.eat_punct(op) {
                let e = self.unary()?;
                let op: &'static str = match op {
                    "-" => "-",
                    "+" => "+",
                    "!" => "!",
                    _ => "~",
                };
                return Ok(Expr::Unary(op, Box::new(e)));
            }
        }
        for op in ["++", "--"] {
            if self.eat_punct(op) {
                let e = self.unary()?;
                return Ok(Expr::Step(
                    if op == "++" { "++" } else { "--" },
                    Box::new(e),
                ));
            }
        }
        self.postfix()
    }

    fn postfix(&mut self) -> Result<Expr> {
        let mut e = self.primary()?;
        loop {
            if self.eat_punct(".") {
                let m = self.ident()?;
                e = Expr::Member(Box::new(e), m);
            } else if self.eat_punct("[") {
                let i = self.expression()?;
                self.expect("]")?;
                e = Expr::Index(Box::new(e), Box::new(i));
            } else if self.eat_punct("++") {
                e = Expr::Step("++", Box::new(e));
            } else if self.eat_punct("--") {
                e = Expr::Step("--", Box::new(e));
            } else {
                break;
            }
        }
        Ok(e)
    }

    fn call_args(&mut self) -> Result<Vec<Expr>> {
        self.expect("(")?;
        let mut args = Vec::new();
        if self.eat_punct(")") {
            return Ok(args);
        }
        if self.is_ident("void") && matches!(self.peek_at(1), Some(Tok::Punct(")"))) {
            self.pos += 2;
            return Ok(args);
        }
        loop {
            args.push(self.assignment()?);
            if !self.eat_punct(",") {
                break;
            }
        }
        self.expect(")")?;
        Ok(args)
    }

    fn primary(&mut self) -> Result<Expr> {
        match self.peek().cloned() {
            Some(Tok::Num(s, f)) => {
                self.pos += 1;
                Ok(Expr::Num(s, f))
            }
            Some(Tok::Punct("(")) => {
                self.pos += 1;
                let e = self.expression()?;
                self.expect(")")?;
                Ok(e)
            }
            Some(Tok::Ident(s)) => {
                self.pos += 1;
                match s.as_str() {
                    "true" => return Ok(Expr::Bool(true)),
                    "false" => return Ok(Expr::Bool(false)),
                    _ => {}
                }
                // Array constructor: `float[3](...)` or `float[](...)`.
                if self.is_punct("[") {
                    if let Some(elem) = Ty::from_keyword(&s) {
                        self.pos += 1;
                        let size = match self.peek() {
                            Some(Tok::Num(n, false)) => {
                                let n = n.parse::<usize>().ok();
                                self.pos += 1;
                                n
                            }
                            _ => None,
                        };
                        self.expect("]")?;
                        let args = self.call_args()?;
                        let n = size.unwrap_or(args.len());
                        return Ok(Expr::ArrayCtor(Ty::Array(Box::new(elem), n), args));
                    }
                }
                if self.is_punct("(") {
                    let args = self.call_args()?;
                    return Ok(Expr::Call(s, args));
                }
                Ok(Expr::Ident(s))
            }
            t => Err(err(format!("unexpected token {t:?}"))),
        }
    }
}

fn binary_precedence(op: &str) -> Option<u8> {
    Some(match op {
        "||" => 1,
        "^^" => 2,
        "&&" => 3,
        "|" => 4,
        "^" => 5,
        "&" => 6,
        "==" | "!=" => 7,
        "<" | ">" | "<=" | ">=" => 8,
        "<<" | ">>" => 9,
        "+" | "-" => 10,
        "*" | "/" | "%" => 11,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers() {
        let t = tokenize("1. 0.5 1e+20. 2 .5 3e-7").unwrap();
        assert_eq!(
            t,
            vec![
                Tok::Num("1.".into(), true),
                Tok::Num("0.5".into(), true),
                Tok::Num("1e+20.".into(), true),
                Tok::Num("2".into(), false),
                Tok::Num(".5".into(), true),
                Tok::Num("3e-7".into(), true),
            ]
        );
    }

    #[test]
    fn parse_function() {
        let items = parse(
            "uniform float u;\nconst float k[2] = float[2](1., 2.);\nvec4 f(vec4 inPixel)\n{\n  vec4 outColor = inPixel;\n  outColor.rgb = (u > 0.) ? outColor.rgb : vec3(k[1]);\n  return outColor;\n}\n",
        )
        .unwrap();
        assert_eq!(items.len(), 3);
        match &items[2] {
            Item::Function(f) => {
                assert_eq!(f.name, "f");
                assert_eq!(f.body.len(), 3);
            }
            _ => panic!("not a function"),
        }
    }
}
