//! A port of the subset of the `yaml-cpp` emitter (`Emitter`,
//! `EmitterState`, `emitterutils`) used by OCIO, reproducing its exact
//! output formatting (indentation, flow / block styles, quoting rules, blank
//! lines).

use crate::config::utils::format_g;

/// Emitter manipulators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Manip {
    BeginMap,
    EndMap,
    BeginSeq,
    EndSeq,
    /// No-op (key / value are deduced from the position).
    Key,
    /// No-op (key / value are deduced from the position).
    Value,
    /// Use the flow style for the next group.
    Flow,
    /// Use the block style for the next group.
    Block,
    /// Use the literal style for the next string.
    Literal,
    /// Emit a line break.
    Newline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FlowType {
    NoType,
    Flow,
    Block,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GroupType {
    Seq,
    Map,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NodeType {
    NoType,
    Property,
    Scalar,
    FlowSeq,
    BlockSeq,
    FlowMap,
    BlockMap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Fmt {
    Auto,
    Literal,
    Flow,
    Block,
    LongKey,
}

#[derive(Debug, Clone, Copy)]
enum Change {
    StrFmt(Fmt),
    SeqFmt(Fmt),
    MapFmt(Fmt),
    MapKeyFmt(Fmt),
}

#[derive(Debug)]
struct Group {
    ty: GroupType,
    flow: FlowType,
    indent: usize,
    child_count: usize,
    long_key: bool,
    modified: Vec<Change>,
}

impl Group {
    fn node_type(&self) -> NodeType {
        match (self.ty, self.flow) {
            (GroupType::Seq, FlowType::Flow) => NodeType::FlowSeq,
            (GroupType::Seq, _) => NodeType::BlockSeq,
            (GroupType::Map, FlowType::Flow) => NodeType::FlowMap,
            (GroupType::Map, _) => NodeType::BlockMap,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StringFormat {
    Plain,
    DoubleQuoted,
    Literal,
}

/// The YAML emitter.
#[derive(Debug)]
pub struct Emitter {
    out: String,
    col: usize,
    // Settings.
    str_fmt: Fmt,
    seq_fmt: Fmt,
    map_fmt: Fmt,
    map_key_fmt: Fmt,
    indent: usize,
    float_precision: usize,
    double_precision: usize,
    modified: Vec<Change>,
    // State.
    groups: Vec<Group>,
    cur_indent: usize,
    has_tag: bool,
    has_non_content: bool,
    doc_count: usize,
}

impl Default for Emitter {
    fn default() -> Self {
        Self::new()
    }
}

impl Emitter {
    /// A new emitter with the `yaml-cpp` default settings.
    pub fn new() -> Self {
        Self {
            out: String::new(),
            col: 0,
            str_fmt: Fmt::Auto,
            seq_fmt: Fmt::Block,
            map_fmt: Fmt::Block,
            map_key_fmt: Fmt::Auto,
            indent: 2,
            float_precision: 9,
            double_precision: 17,
            modified: Vec::new(),
            groups: Vec::new(),
            cur_indent: 0,
            has_tag: false,
            has_non_content: false,
            doc_count: 0,
        }
    }

    /// Precision used to write floats.
    pub fn set_float_precision(&mut self, p: usize) {
        self.float_precision = p;
    }

    /// Precision used to write doubles.
    pub fn set_double_precision(&mut self, p: usize) {
        self.double_precision = p;
    }

    /// The emitted text.
    pub fn as_str(&self) -> &str {
        &self.out
    }

    /// Consume the emitter and return the text.
    pub fn into_string(self) -> String {
        self.out
    }

    // --- stream helpers -------------------------------------------------

    fn write(&mut self, s: &str) {
        for c in s.chars() {
            self.out.push(c);
            if c == '\n' {
                self.col = 0;
            } else {
                self.col += 1;
            }
        }
    }

    fn indent_to(&mut self, n: usize) {
        while self.col < n {
            self.write(" ");
        }
    }

    fn space_or_indent_to(&mut self, require_space: bool, indent: usize) {
        if self.col > 0 && require_space {
            self.write(" ");
        }
        self.indent_to(indent);
    }

    // --- settings -------------------------------------------------------

    fn set_local(&mut self, change: Change) {
        let old = match change {
            Change::StrFmt(v) => Change::StrFmt(std::mem::replace(&mut self.str_fmt, v)),
            Change::SeqFmt(v) => Change::SeqFmt(std::mem::replace(&mut self.seq_fmt, v)),
            Change::MapFmt(v) => Change::MapFmt(std::mem::replace(&mut self.map_fmt, v)),
            Change::MapKeyFmt(v) => Change::MapKeyFmt(std::mem::replace(&mut self.map_key_fmt, v)),
        };
        self.modified.push(old);
    }

    fn restore(&mut self, changes: Vec<Change>) {
        for c in changes {
            match c {
                Change::StrFmt(v) => self.str_fmt = v,
                Change::SeqFmt(v) => self.seq_fmt = v,
                Change::MapFmt(v) => self.map_fmt = v,
                Change::MapKeyFmt(v) => self.map_key_fmt = v,
            }
        }
    }

    fn clear_modified(&mut self) {
        let m = std::mem::take(&mut self.modified);
        self.restore(m);
    }

    // --- state ----------------------------------------------------------

    fn has_begun_node(&self) -> bool {
        self.has_tag || self.has_non_content
    }

    fn has_begun_content(&self) -> bool {
        self.has_tag
    }

    fn cur_group_flow_type(&self) -> FlowType {
        self.groups
            .last()
            .map(|g| g.flow)
            .unwrap_or(FlowType::NoType)
    }

    fn cur_group_node_type(&self) -> NodeType {
        self.groups
            .last()
            .map(|g| g.node_type())
            .unwrap_or(NodeType::NoType)
    }

    fn cur_group_child_count(&self) -> usize {
        self.groups
            .last()
            .map(|g| g.child_count)
            .unwrap_or(self.doc_count)
    }

    fn cur_group_indent(&self) -> usize {
        self.groups.last().map(|g| g.indent).unwrap_or(0)
    }

    fn cur_group_long_key(&self) -> bool {
        self.groups.last().map(|g| g.long_key).unwrap_or(false)
    }

    fn last_indent(&self) -> usize {
        if self.groups.len() <= 1 {
            return 0;
        }
        self.cur_indent - self.groups[self.groups.len() - 2].indent
    }

    fn get_flow_type(&self, ty: GroupType) -> Fmt {
        if self.cur_group_flow_type() == FlowType::Flow {
            return Fmt::Flow;
        }
        match ty {
            GroupType::Seq => self.seq_fmt,
            GroupType::Map => self.map_fmt,
        }
    }

    fn next_group_type(&self, ty: GroupType) -> NodeType {
        let block = self.get_flow_type(ty) == Fmt::Block;
        match (ty, block) {
            (GroupType::Seq, true) => NodeType::BlockSeq,
            (GroupType::Seq, false) => NodeType::FlowSeq,
            (GroupType::Map, true) => NodeType::BlockMap,
            (GroupType::Map, false) => NodeType::FlowMap,
        }
    }

    fn started_node(&mut self) {
        match self.groups.last_mut() {
            None => self.doc_count += 1,
            Some(g) => {
                g.child_count += 1;
                if g.child_count % 2 == 0 {
                    g.long_key = false;
                }
            }
        }
        self.has_tag = false;
        self.has_non_content = false;
    }

    fn started_scalar(&mut self) {
        self.started_node();
        self.clear_modified();
    }

    fn started_group(&mut self, ty: GroupType) {
        self.started_node();
        let last = self.groups.last().map(|g| g.indent).unwrap_or(0);
        self.cur_indent += last;
        let modified = std::mem::take(&mut self.modified);
        let flow = if self.get_flow_type(ty) == Fmt::Block {
            FlowType::Block
        } else {
            FlowType::Flow
        };
        self.groups.push(Group {
            ty,
            flow,
            indent: self.indent,
            child_count: 0,
            long_key: false,
            modified,
        });
    }

    fn ended_group(&mut self) {
        if let Some(g) = self.groups.pop() {
            self.restore(g.modified);
        }
        let last = self.groups.last().map(|g| g.indent).unwrap_or(0);
        self.cur_indent = self.cur_indent.saturating_sub(last);
        self.clear_modified();
        self.has_tag = false;
        self.has_non_content = false;
    }

    fn set_long_key(&mut self) {
        if let Some(g) = self.groups.last_mut() {
            g.long_key = true;
        }
    }

    fn force_flow(&mut self) {
        if let Some(g) = self.groups.last_mut() {
            g.flow = FlowType::Flow;
        }
    }

    // --- node preparation -------------------------------------------------

    fn prepare_node(&mut self, child: NodeType) {
        match self.cur_group_node_type() {
            NodeType::NoType => self.prepare_top_node(child),
            NodeType::FlowSeq => self.flow_seq_prepare_node(child),
            NodeType::BlockSeq => self.block_seq_prepare_node(child),
            NodeType::FlowMap => self.flow_map_prepare_node(child),
            NodeType::BlockMap => self.block_map_prepare_node(child),
            NodeType::Property | NodeType::Scalar => {}
        }
    }

    fn prepare_top_node(&mut self, child: NodeType) {
        if child == NodeType::NoType {
            return;
        }
        if self.cur_group_child_count() > 0 && self.col > 0 {
            // Begin a new document.
            self.write("\n---\n");
            self.has_tag = false;
            self.has_non_content = false;
        }
        match child {
            NodeType::Property | NodeType::Scalar | NodeType::FlowSeq | NodeType::FlowMap => {
                self.space_or_indent_to(self.has_begun_content(), 0)
            }
            NodeType::BlockSeq | NodeType::BlockMap => {
                if self.has_begun_node() {
                    self.write("\n");
                }
            }
            NodeType::NoType => {}
        }
    }

    fn flow_seq_prepare_node(&mut self, child: NodeType) {
        let last_indent = self.last_indent();
        if !self.has_begun_node() {
            self.indent_to(last_indent);
            if self.cur_group_child_count() == 0 {
                self.write("[");
            } else {
                self.write(",");
            }
        }
        if matches!(
            child,
            NodeType::Property | NodeType::Scalar | NodeType::FlowSeq | NodeType::FlowMap
        ) {
            let req = self.has_begun_content() || self.cur_group_child_count() > 0;
            self.space_or_indent_to(req, last_indent);
        }
    }

    fn block_seq_prepare_node(&mut self, child: NodeType) {
        let cur_indent = self.cur_indent;
        let next_indent = cur_indent + self.cur_group_indent();
        if child == NodeType::NoType {
            return;
        }
        if !self.has_begun_content() {
            if self.cur_group_child_count() > 0 {
                self.write("\n");
            }
            self.indent_to(cur_indent);
            self.write("-");
        }
        match child {
            NodeType::Property | NodeType::Scalar | NodeType::FlowSeq | NodeType::FlowMap => {
                self.space_or_indent_to(self.has_begun_content(), next_indent)
            }
            NodeType::BlockSeq => self.write("\n"),
            NodeType::BlockMap => {
                if self.has_begun_content() {
                    self.write("\n");
                }
            }
            NodeType::NoType => {}
        }
    }

    fn flow_map_prepare_node(&mut self, child: NodeType) {
        if self.cur_group_child_count().is_multiple_of(2) {
            if self.map_key_fmt == Fmt::LongKey {
                self.set_long_key();
            }
            if self.cur_group_long_key() {
                self.flow_map_prepare_long_key(child);
            } else {
                self.flow_map_prepare_simple_key(child);
            }
        } else if self.cur_group_long_key() {
            self.flow_map_prepare_long_key_value(child);
        } else {
            self.flow_map_prepare_simple_key_value(child);
        }
    }

    fn flow_map_value_space(&mut self, child: NodeType, last_indent: usize) {
        if matches!(
            child,
            NodeType::Property | NodeType::Scalar | NodeType::FlowSeq | NodeType::FlowMap
        ) {
            let req = self.has_begun_content() || self.cur_group_child_count() > 0;
            self.space_or_indent_to(req, last_indent);
        }
    }

    fn flow_map_prepare_long_key(&mut self, child: NodeType) {
        let last_indent = self.last_indent();
        if !self.has_begun_node() {
            self.indent_to(last_indent);
            if self.cur_group_child_count() == 0 {
                self.write("{ ?");
            } else {
                self.write(", ?");
            }
        }
        self.flow_map_value_space(child, last_indent);
    }

    fn flow_map_prepare_long_key_value(&mut self, child: NodeType) {
        let last_indent = self.last_indent();
        if !self.has_begun_node() {
            self.indent_to(last_indent);
            self.write(":");
        }
        self.flow_map_value_space(child, last_indent);
    }

    fn flow_map_prepare_simple_key(&mut self, child: NodeType) {
        let last_indent = self.last_indent();
        if !self.has_begun_node() {
            self.indent_to(last_indent);
            if self.cur_group_child_count() == 0 {
                self.write("{");
            } else {
                self.write(",");
            }
        }
        self.flow_map_value_space(child, last_indent);
    }

    fn flow_map_prepare_simple_key_value(&mut self, child: NodeType) {
        let last_indent = self.last_indent();
        if !self.has_begun_node() {
            self.indent_to(last_indent);
            self.write(":");
        }
        self.flow_map_value_space(child, last_indent);
    }

    fn block_map_prepare_node(&mut self, child: NodeType) {
        if self.cur_group_child_count().is_multiple_of(2) {
            if self.map_key_fmt == Fmt::LongKey {
                self.set_long_key();
            }
            if matches!(
                child,
                NodeType::BlockSeq | NodeType::BlockMap | NodeType::Property
            ) {
                self.set_long_key();
            }
            if self.cur_group_long_key() {
                self.block_map_prepare_long_key(child);
            } else {
                self.block_map_prepare_simple_key(child);
            }
        } else if self.cur_group_long_key() {
            self.block_map_prepare_long_key_value(child);
        } else {
            self.block_map_prepare_simple_key_value(child);
        }
    }

    fn block_map_prepare_long_key(&mut self, child: NodeType) {
        let cur_indent = self.cur_indent;
        let child_count = self.cur_group_child_count();
        if child == NodeType::NoType {
            return;
        }
        if !self.has_begun_content() {
            if child_count > 0 {
                self.write("\n");
            }
            self.indent_to(cur_indent);
            self.write("?");
        }
        match child {
            NodeType::Property | NodeType::Scalar | NodeType::FlowSeq | NodeType::FlowMap => {
                self.space_or_indent_to(true, cur_indent + 1)
            }
            NodeType::BlockSeq | NodeType::BlockMap => {
                if self.has_begun_content() {
                    self.write("\n");
                }
            }
            NodeType::NoType => {}
        }
    }

    fn block_map_prepare_long_key_value(&mut self, child: NodeType) {
        let cur_indent = self.cur_indent;
        if child == NodeType::NoType {
            return;
        }
        if !self.has_begun_content() {
            self.write("\n");
            self.indent_to(cur_indent);
            self.write(":");
        }
        match child {
            NodeType::Property | NodeType::Scalar | NodeType::FlowSeq | NodeType::FlowMap => {
                self.space_or_indent_to(true, cur_indent + 1)
            }
            NodeType::BlockSeq | NodeType::BlockMap => {
                if self.has_begun_content() {
                    self.write("\n");
                }
                self.space_or_indent_to(true, cur_indent + 1);
            }
            NodeType::NoType => {}
        }
    }

    fn block_map_prepare_simple_key(&mut self, child: NodeType) {
        let cur_indent = self.cur_indent;
        let child_count = self.cur_group_child_count();
        if child == NodeType::NoType {
            return;
        }
        if !self.has_begun_node() && child_count > 0 {
            self.write("\n");
        }
        if matches!(
            child,
            NodeType::Property | NodeType::Scalar | NodeType::FlowSeq | NodeType::FlowMap
        ) {
            self.space_or_indent_to(self.has_begun_content(), cur_indent);
        }
    }

    fn block_map_prepare_simple_key_value(&mut self, child: NodeType) {
        let cur_indent = self.cur_indent;
        let next_indent = cur_indent + self.cur_group_indent();
        if !self.has_begun_node() {
            self.write(":");
        }
        match child {
            NodeType::Property | NodeType::Scalar | NodeType::FlowSeq | NodeType::FlowMap => {
                self.space_or_indent_to(true, next_indent)
            }
            NodeType::BlockSeq | NodeType::BlockMap => self.write("\n"),
            NodeType::NoType => {}
        }
    }

    // --- public API -------------------------------------------------------

    /// Apply a manipulator.
    pub fn manip(&mut self, m: Manip) -> &mut Self {
        match m {
            Manip::BeginSeq => {
                let t = self.next_group_type(GroupType::Seq);
                self.prepare_node(t);
                self.started_group(GroupType::Seq);
            }
            Manip::BeginMap => {
                let t = self.next_group_type(GroupType::Map);
                self.prepare_node(t);
                self.started_group(GroupType::Map);
            }
            Manip::EndSeq => self.end_group('[', ']'),
            Manip::EndMap => self.end_group('{', '}'),
            Manip::Key | Manip::Value => {}
            Manip::Flow => {
                self.set_local(Change::SeqFmt(Fmt::Flow));
                self.set_local(Change::MapFmt(Fmt::Flow));
            }
            Manip::Block => {
                self.set_local(Change::SeqFmt(Fmt::Block));
                self.set_local(Change::MapFmt(Fmt::Block));
            }
            Manip::Literal => self.set_local(Change::StrFmt(Fmt::Literal)),
            Manip::Newline => {
                self.prepare_node(NodeType::NoType);
                self.write("\n");
                self.has_non_content = true;
            }
        }
        self
    }

    fn end_group(&mut self, open: char, close: char) {
        let original = self.cur_group_flow_type();
        if self.cur_group_child_count() == 0 {
            self.force_flow();
        }
        if self.cur_group_flow_type() == FlowType::Flow {
            let ci = self.cur_indent;
            self.indent_to(ci);
            if original == FlowType::Block
                || (self.cur_group_child_count() == 0 && !self.has_begun_node())
            {
                self.write(&open.to_string());
            }
            self.write(&close.to_string());
        }
        self.ended_group();
    }

    /// Write a verbatim tag (`!<name>`).
    pub fn verbatim_tag(&mut self, tag: &str) -> &mut Self {
        self.prepare_node(NodeType::Property);
        self.write(&format!("!<{tag}>"));
        self.has_tag = true;
        self
    }

    /// Write a string scalar.
    pub fn string(&mut self, s: &str) -> &mut Self {
        let fmt = compute_string_format(s, self.str_fmt, self.cur_group_flow_type());
        if fmt == StringFormat::Literal || s.len() > 1024 {
            self.set_local(Change::MapKeyFmt(Fmt::LongKey));
        }
        self.prepare_node(NodeType::Scalar);
        match fmt {
            StringFormat::Plain => self.write(s),
            StringFormat::DoubleQuoted => {
                let q = double_quoted(s);
                self.write(&q);
            }
            StringFormat::Literal => {
                let indent = self.cur_indent + self.indent;
                self.write("|\n");
                for c in s.chars() {
                    if c == '\n' {
                        self.write("\n");
                    } else {
                        self.indent_to(indent);
                        self.write(&c.to_string());
                    }
                }
            }
        }
        self.started_scalar();
        self
    }

    /// Write a boolean (`true` / `false`).
    pub fn boolean(&mut self, b: bool) -> &mut Self {
        self.prepare_node(NodeType::Scalar);
        self.write(if b { "true" } else { "false" });
        self.started_scalar();
        self
    }

    /// Write a character (as `yaml-cpp`'s `Write(char)`).
    pub fn character(&mut self, ch: char) -> &mut Self {
        self.prepare_node(NodeType::Scalar);
        let s = if ch.is_ascii_alphabetic() {
            ch.to_string()
        } else {
            match ch {
                '"' => "\"\\\"\"".to_string(),
                '\t' => "\"\\t\"".to_string(),
                '\n' => "\"\\n\"".to_string(),
                '\u{8}' => "\"\\b\"".to_string(),
                '\r' => "\"\\r\"".to_string(),
                '\u{c}' => "\"\\f\"".to_string(),
                '\\' => "\"\\\\\"".to_string(),
                c if (' '..='~').contains(&c) => format!("\"{c}\""),
                c => format!("\"\\x{:02x}\"", c as u32),
            }
        };
        self.write(&s);
        self.started_scalar();
        self
    }

    fn streamable(&mut self, v: f64, precision: usize) {
        self.prepare_node(NodeType::Scalar);
        let s = if v.is_nan() {
            ".nan".to_string()
        } else if v.is_infinite() {
            if v < 0.0 {
                "-.inf".to_string()
            } else {
                ".inf".to_string()
            }
        } else {
            format_g(v, precision)
        };
        self.write(&s);
        self.started_scalar();
    }

    /// Write a double.
    pub fn double(&mut self, v: f64) -> &mut Self {
        let p = self.double_precision;
        self.streamable(v, p);
        self
    }

    /// Write a float.
    pub fn float(&mut self, v: f32) -> &mut Self {
        let p = self.float_precision;
        self.streamable(v as f64, p);
        self
    }

    /// Write a sequence of strings.
    pub fn string_seq(&mut self, v: &[String]) -> &mut Self {
        self.manip(Manip::BeginSeq);
        for s in v {
            self.string(s);
        }
        self.manip(Manip::EndSeq)
    }

    /// Write a sequence of doubles.
    pub fn double_seq(&mut self, v: &[f64]) -> &mut Self {
        self.manip(Manip::BeginSeq);
        for x in v {
            self.double(*x);
        }
        self.manip(Manip::EndSeq)
    }

    /// Write a sequence of floats.
    pub fn float_seq(&mut self, v: &[f32]) -> &mut Self {
        self.manip(Manip::BeginSeq);
        for x in v {
            self.float(*x);
        }
        self.manip(Manip::EndSeq)
    }

    /// Write a key (a string scalar).
    pub fn key(&mut self, k: &str) -> &mut Self {
        self.string(k)
    }
}

// ---------------------------------------------------------------------------
// String formatting (port of emitterutils.cpp and exp.h).

#[derive(Debug, Clone)]
enum Re {
    Empty,
    Match(u8),
    Range(u8, u8),
    Or(Vec<Re>),
    Not(Box<Re>),
    Seq(Vec<Re>),
}

fn any_of(s: &str) -> Re {
    Re::Or(s.bytes().map(Re::Match).collect())
}

fn seq_of(s: &str) -> Re {
    Re::Seq(s.bytes().map(Re::Match).collect())
}

impl Re {
    fn or(self, other: Re) -> Re {
        Re::Or(vec![self, other])
    }
    fn then(self, other: Re) -> Re {
        Re::Seq(vec![self, other])
    }
    fn not(self) -> Re {
        Re::Not(Box::new(self))
    }

    /// `RegEx::Match`: checks the source validity first.
    fn match_at(&self, s: &[u8], pos: usize) -> i32 {
        match self {
            Re::Match(_) | Re::Range(_, _) if pos >= s.len() => -1,
            _ => self.match_unchecked(s, pos),
        }
    }

    fn match_unchecked(&self, s: &[u8], pos: usize) -> i32 {
        let c = s.get(pos).copied().unwrap_or(0);
        match self {
            Re::Empty => {
                if pos >= s.len() {
                    0
                } else {
                    -1
                }
            }
            Re::Match(m) => {
                if c == *m {
                    1
                } else {
                    -1
                }
            }
            Re::Range(a, z) => {
                if c < *a || c > *z {
                    -1
                } else {
                    1
                }
            }
            Re::Or(params) => {
                for p in params {
                    let n = p.match_unchecked(s, pos);
                    if n >= 0 {
                        return n;
                    }
                }
                -1
            }
            Re::Not(p) => {
                if p.match_unchecked(s, pos) >= 0 {
                    -1
                } else {
                    1
                }
            }
            Re::Seq(params) => {
                let mut offset = 0usize;
                for p in params {
                    let n = p.match_at(s, pos + offset);
                    if n == -1 {
                        return -1;
                    }
                    offset += n as usize;
                }
                offset as i32
            }
        }
    }

    fn matches(&self, s: &[u8], pos: usize) -> bool {
        self.match_at(s, pos) >= 0
    }
}

fn blank() -> Re {
    Re::Match(b' ').or(Re::Match(b'\t'))
}

fn break_re() -> Re {
    Re::Match(b'\n').or(seq_of("\r\n")).or(Re::Match(b'\r'))
}

fn blank_or_break() -> Re {
    blank().or(break_re())
}

fn not_printable() -> Re {
    Re::Match(0)
        .or(any_of("\x01\x02\x03\x04\x05\x06\x07\x08\x0B\x0C\x7F"))
        .or(Re::Range(0x0E, 0x1F))
        .or(Re::Match(0xC2).then(Re::Range(0x80, 0x84).or(Re::Range(0x86, 0x9F))))
}

fn bom() -> Re {
    Re::Seq(vec![Re::Match(0xEF), Re::Match(0xBB), Re::Match(0xBF)])
}

fn plain_scalar() -> Re {
    blank_or_break()
        .or(any_of(",[]{}#&*!|>'\"%@`"))
        .or(any_of("-?:").then(blank_or_break().or(Re::Empty)))
        .not()
}

fn plain_scalar_in_flow() -> Re {
    blank_or_break()
        .or(any_of("?,[]{}#&*!|>'\"%@`"))
        .or(any_of("-:").then(blank().or(Re::Empty)))
        .not()
}

fn end_scalar() -> Re {
    Re::Match(b':').then(blank_or_break().or(Re::Empty))
}

fn end_scalar_in_flow() -> Re {
    Re::Match(b':')
        .then(blank_or_break().or(Re::Empty).or(any_of(",]}")))
        .or(any_of(",?[]{}"))
}

fn is_null_string(s: &str) -> bool {
    s.is_empty() || s == "~" || s == "null" || s == "Null" || s == "NULL"
}

fn is_valid_plain_scalar(s: &str, flow: FlowType) -> bool {
    if is_null_string(s) {
        return false;
    }
    let bytes = s.as_bytes();
    let start = if flow == FlowType::Flow {
        plain_scalar_in_flow()
    } else {
        plain_scalar()
    };
    if !start.matches(bytes, 0) {
        return false;
    }
    if s.ends_with(' ') {
        return false;
    }
    let end = if flow == FlowType::Flow {
        end_scalar_in_flow()
    } else {
        end_scalar()
    };
    let disallowed = end
        .or(blank_or_break().then(Re::Match(b'#')))
        .or(not_printable())
        .or(bom())
        .or(break_re())
        .or(Re::Match(b'\t'))
        .or(Re::Match(b'&'));
    (0..bytes.len()).all(|pos| !disallowed.matches(bytes, pos))
}

fn compute_string_format(s: &str, fmt: Fmt, flow: FlowType) -> StringFormat {
    match fmt {
        Fmt::Literal => {
            if flow != FlowType::Flow {
                StringFormat::Literal
            } else {
                StringFormat::DoubleQuoted
            }
        }
        _ => {
            if is_valid_plain_scalar(s, flow) {
                StringFormat::Plain
            } else {
                StringFormat::DoubleQuoted
            }
        }
    }
}

fn double_quoted(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c => {
                let cp = c as u32;
                if cp < 0x20 || (0x80..=0xA0).contains(&cp) {
                    if cp < 0xFF {
                        out.push_str(&format!("\\x{cp:02x}"));
                    } else {
                        out.push_str(&format!("\\u{cp:04x}"));
                    }
                } else if cp == 0xFEFF {
                    out.push_str("\\ufeff");
                } else {
                    out.push(c);
                }
            }
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_rules() {
        assert!(is_valid_plain_scalar("abc", FlowType::Block));
        assert!(!is_valid_plain_scalar("", FlowType::Block));
        assert!(!is_valid_plain_scalar("a: b", FlowType::Block));
        assert!(is_valid_plain_scalar("a:b", FlowType::Block));
        assert!(is_valid_plain_scalar("a:b", FlowType::Flow));
        assert!(!is_valid_plain_scalar("a: b", FlowType::Flow));
        assert!(!is_valid_plain_scalar("a:", FlowType::Flow));
        assert!(!is_valid_plain_scalar("a,b", FlowType::Flow));
        assert!(is_valid_plain_scalar("a,b", FlowType::Block));
        assert!(!is_valid_plain_scalar("-", FlowType::Block));
        assert!(is_valid_plain_scalar("-a", FlowType::Block));
        assert!(is_valid_plain_scalar("<USE_DISPLAY_NAME>", FlowType::Flow));
        assert!(!is_valid_plain_scalar("a #b", FlowType::Block));
        assert!(is_valid_plain_scalar("$", FlowType::Block));
        assert!(!is_valid_plain_scalar("%a", FlowType::Block));
    }

    #[test]
    fn simple_doc() {
        let mut e = Emitter::new();
        e.set_double_precision(15);
        e.manip(Manip::Block).manip(Manip::BeginMap);
        e.key("ocio_profile_version").string("2.2");
        e.manip(Manip::Newline).manip(Manip::Newline);
        e.key("environment")
            .manip(Manip::BeginMap)
            .manip(Manip::EndMap)
            .manip(Manip::Newline);
        e.key("search_path").string("");
        e.key("luma")
            .manip(Manip::Flow)
            .double_seq(&[0.2126, 0.7152, 0.0722]);
        e.manip(Manip::Newline).manip(Manip::Newline);
        e.key("roles").manip(Manip::BeginMap);
        e.key("default").string("raw");
        e.manip(Manip::EndMap).manip(Manip::Newline);
        e.key("colorspaces").manip(Manip::BeginSeq);
        e.verbatim_tag("ColorSpace").manip(Manip::BeginMap);
        e.key("name").string("raw");
        e.key("description")
            .manip(Manip::Literal)
            .string("line1\nline2");
        e.key("isdata").boolean(true);
        e.key("from_scene_reference")
            .verbatim_tag("GroupTransform")
            .manip(Manip::BeginMap);
        e.key("children").manip(Manip::BeginSeq);
        e.verbatim_tag("FileTransform")
            .manip(Manip::Flow)
            .manip(Manip::BeginMap);
        e.key("src").string("");
        e.key("interpolation").string("best");
        e.manip(Manip::EndMap);
        e.manip(Manip::EndSeq).manip(Manip::EndMap);
        e.manip(Manip::EndMap).manip(Manip::Newline);
        e.verbatim_tag("ColorSpace").manip(Manip::BeginMap);
        e.key("name").string("b");
        e.manip(Manip::EndMap).manip(Manip::Newline);
        e.manip(Manip::EndSeq);
        e.manip(Manip::EndMap);
        let expected = concat!(
            "ocio_profile_version: 2.2\n\nenvironment:\n  {}\nsearch_path: \"\"\n",
            "luma: [0.2126, 0.7152, 0.0722]\n\nroles:\n  default: raw\ncolorspaces:\n  - !<ColorSpace>\n    name: raw\n",
            "    description: |\n      line1\n      line2\n    isdata: true\n    from_scene_reference: !<GroupTransform>\n",
            "      children:\n        - !<FileTransform> {src: \"\", interpolation: best}\n\n  - !<ColorSpace>\n    name: b\n"
        );
        assert_eq!(e.as_str(), expected);
    }
}
