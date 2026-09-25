//! A YAML document tree mimicking the parts of the `yaml-cpp` node API used
//! by OCIO: tags, key order, duplicate keys, marks (line / column) and the
//! `as<T>()` conversions with their error messages.

use crate::error::{Error, Result};
use std::collections::HashMap;
use yaml_rust2::parser::{Event, MarkedEventReceiver, Parser, Tag};
use yaml_rust2::scanner::{Marker, TScalarStyle};

/// Content of a node.
#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    /// Missing or null value (`~`, `null`, empty).
    Null,
    /// A scalar.
    Scalar(String),
    /// A sequence.
    Seq(Vec<Node>),
    /// A map, keys in document order (duplicates are kept).
    Map(Vec<(Node, Node)>),
}

/// A YAML node.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub kind: Kind,
    /// The tag as returned by `yaml-cpp`'s `Node::Tag()`: the verbatim tag
    /// content (e.g. `ColorSpace` for `!<ColorSpace>`), `?` for untagged
    /// plain nodes, `!` for untagged quoted scalars, `""` for undefined nodes.
    pub tag: String,
    /// 0-based line (-1 for undefined nodes).
    pub line: i64,
    /// 0-based column (-1 for undefined nodes).
    pub col: i64,
}

impl Node {
    /// An undefined / null node without mark (e.g. an empty document).
    pub fn undefined() -> Self {
        Node {
            kind: Kind::Null,
            tag: String::new(),
            line: -1,
            col: -1,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self.kind, Kind::Null)
    }
    pub fn is_scalar(&self) -> bool {
        matches!(self.kind, Kind::Scalar(_))
    }
    pub fn is_seq(&self) -> bool {
        matches!(self.kind, Kind::Seq(_))
    }
    pub fn is_map(&self) -> bool {
        matches!(self.kind, Kind::Map(_))
    }

    /// Number of children (0 for scalars and null), as `yaml-cpp`'s `size()`.
    pub fn size(&self) -> usize {
        match &self.kind {
            Kind::Seq(v) => v.len(),
            Kind::Map(v) => v.len(),
            _ => 0,
        }
    }

    /// The node type name used in error messages (`NodeType::value`).
    pub fn type_id(&self) -> i32 {
        match &self.kind {
            Kind::Null => 1,
            Kind::Scalar(_) => 2,
            Kind::Seq(_) => 3,
            Kind::Map(_) => 4,
        }
    }

    /// Map entries (empty for other nodes).
    pub fn map_entries(&self) -> &[(Node, Node)] {
        match &self.kind {
            Kind::Map(v) => v,
            _ => &[],
        }
    }

    /// Sequence items (empty for other nodes).
    pub fn seq_items(&self) -> &[Node] {
        match &self.kind {
            Kind::Seq(v) => v,
            _ => &[],
        }
    }

    /// First value of the map for `key` (`node[key]`).
    pub fn get(&self, key: &str) -> Option<&Node> {
        self.map_entries()
            .iter()
            .find(|(k, _)| matches!(&k.kind, Kind::Scalar(s) if s == key))
            .map(|(_, v)| v)
    }

    fn bad_conversion(&self) -> Error {
        Error::msg(format!(
            "yaml-cpp: error at line {}, column {}: bad conversion",
            self.line + 1,
            self.col + 1
        ))
    }

    /// `as<std::string>()`: scalars give their value, null gives `"null"`.
    pub fn as_string(&self) -> Result<String> {
        match &self.kind {
            Kind::Scalar(s) => Ok(s.clone()),
            Kind::Null => Ok("null".to_string()),
            _ => Err(self.bad_conversion()),
        }
    }

    /// `as<bool>()`.
    pub fn as_bool(&self) -> Result<bool> {
        let s = match &self.kind {
            Kind::Scalar(s) => s,
            _ => return Err(self.bad_conversion()),
        };
        // yaml-cpp only accepts "flexible case" strings: all lower, all upper,
        // or first upper and the rest lower.
        let flexible = {
            let all_lower = s.chars().all(|c| !c.is_ascii_uppercase());
            let all_upper = s.chars().all(|c| !c.is_ascii_lowercase());
            let first_upper = s
                .chars()
                .next()
                .map(|c| {
                    !c.is_ascii_lowercase() && s.chars().skip(1).all(|c| !c.is_ascii_uppercase())
                })
                .unwrap_or(true);
            all_lower || all_upper || first_upper
        };
        if flexible {
            let l = s.to_ascii_lowercase();
            for (t, f) in [("y", "n"), ("yes", "no"), ("true", "false"), ("on", "off")] {
                if l == t {
                    return Ok(true);
                }
                if l == f {
                    return Ok(false);
                }
            }
        }
        Err(self.bad_conversion())
    }

    /// `as<double>()`.
    pub fn as_f64(&self) -> Result<f64> {
        match &self.kind {
            Kind::Scalar(s) => parse_double(s).ok_or_else(|| self.bad_conversion()),
            _ => Err(self.bad_conversion()),
        }
    }

    /// `as<float>()`.
    pub fn as_f32(&self) -> Result<f32> {
        match &self.kind {
            Kind::Scalar(s) => parse_float(s).ok_or_else(|| self.bad_conversion()),
            _ => Err(self.bad_conversion()),
        }
    }

    /// `as<std::vector<std::string>>()`.
    pub fn as_string_vec(&self) -> Result<Vec<String>> {
        match &self.kind {
            Kind::Seq(v) => v.iter().map(|n| n.as_string()).collect(),
            _ => Err(self.bad_conversion()),
        }
    }

    /// `as<std::vector<double>>()`.
    pub fn as_f64_vec(&self) -> Result<Vec<f64>> {
        match &self.kind {
            Kind::Seq(v) => v.iter().map(|n| n.as_f64()).collect(),
            _ => Err(self.bad_conversion()),
        }
    }

    /// `as<std::vector<float>>()`.
    pub fn as_f32_vec(&self) -> Result<Vec<f32>> {
        match &self.kind {
            Kind::Seq(v) => v.iter().map(|n| n.as_f32()).collect(),
            _ => Err(self.bad_conversion()),
        }
    }
}

/// Length of the numeric prefix accepted by `std::istream >> double`.
fn numeric_prefix_len(s: &str) -> usize {
    let b = s.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    let digits_start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    let mut ndigits = i - digits_start;
    if i < b.len() && b[i] == b'.' {
        i += 1;
        let f = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        ndigits += i - f;
    }
    if ndigits == 0 {
        return 0;
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        let mut j = i + 1;
        if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
            j += 1;
        }
        let e = j;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        if j > e {
            i = j;
        } else {
            // "1e" is not a valid number for the C++ stream.
            return 0;
        }
    }
    i
}

fn special_float(s: &str) -> Option<f64> {
    match s {
        ".inf" | ".Inf" | ".INF" | "+.inf" | "+.Inf" | "+.INF" => Some(f64::INFINITY),
        "-.inf" | "-.Inf" | "-.INF" => Some(f64::NEG_INFINITY),
        ".nan" | ".NaN" | ".NAN" => Some(f64::NAN),
        _ => None,
    }
}

/// Parse a double like `yaml-cpp` (C++ stream parsing, trailing white spaces
/// allowed, plus the YAML `.inf` / `.nan` special values).
pub fn parse_double(s: &str) -> Option<f64> {
    let n = numeric_prefix_len(s);
    if n > 0 && s[n..].chars().all(|c| c.is_whitespace()) {
        return s[..n].parse::<f64>().ok();
    }
    special_float(s)
}

/// Parse a float like `yaml-cpp`.
pub fn parse_float(s: &str) -> Option<f32> {
    let n = numeric_prefix_len(s);
    if n > 0 && s[n..].chars().all(|c| c.is_whitespace()) {
        return s[..n].parse::<f32>().ok();
    }
    special_float(s).map(|v| v as f32)
}

enum Frame {
    Seq(Node, usize),
    Map(Node, Vec<(Node, Node)>, Option<Node>, usize),
}

struct Builder<'a> {
    chars: &'a [char],
    line_starts: Vec<usize>,
    stack: Vec<Frame>,
    root: Option<Node>,
    anchors: HashMap<usize, Node>,
    done: bool,
}

fn tag_to_string(tag: &Option<Tag>) -> Option<String> {
    tag.as_ref().map(|t| {
        if t.handle.is_empty() {
            t.suffix.clone()
        } else if t.handle == "!!" {
            format!("tag:yaml.org,2002:{}", t.suffix)
        } else {
            format!("{}{}", t.handle, t.suffix)
        }
    })
}

impl Builder<'_> {
    fn line_col(&self, idx: usize) -> (i64, i64) {
        let line = match self.line_starts.binary_search(&idx) {
            Ok(l) => l,
            Err(l) => l.saturating_sub(1),
        };
        (line as i64, (idx - self.line_starts[line]) as i64)
    }

    /// Mark of a node: the position of its tag if any (as `yaml-cpp`), else
    /// the marker given by the parser.
    fn mark(&self, tag: &Option<Tag>, marker: &Marker) -> (i64, i64) {
        if let Some(t) = tag {
            let pat: Vec<char> = if t.handle.is_empty() {
                format!("!<{}>", t.suffix).chars().collect()
            } else {
                format!("{}{}", t.handle, t.suffix).chars().collect()
            };
            let end = marker.index().min(self.chars.len());
            if pat.len() <= end {
                let mut i = end - pat.len() + 1;
                while i > 0 {
                    i -= 1;
                    if self.chars[i..i + pat.len()] == pat[..] {
                        return self.line_col(i);
                    }
                }
            }
        }
        (marker.line() as i64 - 1, marker.col() as i64)
    }

    fn push_value(&mut self, node: Node) {
        match self.stack.last_mut() {
            None => {
                if self.root.is_none() {
                    self.root = Some(node);
                }
            }
            Some(Frame::Seq(n, _)) => {
                if let Kind::Seq(v) = &mut n.kind {
                    v.push(node);
                }
            }
            Some(Frame::Map(_, entries, key, _)) => match key.take() {
                None => *key = Some(node),
                Some(k) => entries.push((k, node)),
            },
        }
    }
}

impl MarkedEventReceiver for Builder<'_> {
    fn on_event(&mut self, ev: Event, marker: Marker) {
        if self.done {
            return;
        }
        match ev {
            Event::Scalar(value, style, anchor, tag) => {
                let (line, col) = self.mark(&tag, &marker);
                let tag_str = tag_to_string(&tag);
                let plain = style == TScalarStyle::Plain;
                let kind = if plain
                    && tag_str.is_none()
                    && (value.is_empty()
                        || value == "~"
                        || value == "null"
                        || value == "Null"
                        || value == "NULL")
                {
                    Kind::Null
                } else {
                    Kind::Scalar(value)
                };
                let t = tag_str.unwrap_or_else(|| {
                    if plain {
                        "?".to_string()
                    } else {
                        "!".to_string()
                    }
                });
                let node = Node {
                    kind,
                    tag: t,
                    line,
                    col,
                };
                if anchor > 0 {
                    self.anchors.insert(anchor, node.clone());
                }
                self.push_value(node);
            }
            Event::SequenceStart(anchor, tag) => {
                let (line, col) = self.mark(&tag, &marker);
                let t = tag_to_string(&tag).unwrap_or_else(|| "?".to_string());
                self.stack.push(Frame::Seq(
                    Node {
                        kind: Kind::Seq(Vec::new()),
                        tag: t,
                        line,
                        col,
                    },
                    anchor,
                ));
            }
            Event::SequenceEnd => {
                if let Some(Frame::Seq(n, anchor)) = self.stack.pop() {
                    if anchor > 0 {
                        self.anchors.insert(anchor, n.clone());
                    }
                    self.push_value(n);
                }
            }
            Event::MappingStart(anchor, tag) => {
                let (line, col) = self.mark(&tag, &marker);
                let t = tag_to_string(&tag).unwrap_or_else(|| "?".to_string());
                self.stack.push(Frame::Map(
                    Node {
                        kind: Kind::Map(Vec::new()),
                        tag: t,
                        line,
                        col,
                    },
                    Vec::new(),
                    None,
                    anchor,
                ));
            }
            Event::MappingEnd => {
                if let Some(Frame::Map(mut n, entries, _, anchor)) = self.stack.pop() {
                    n.kind = Kind::Map(entries);
                    if anchor > 0 {
                        self.anchors.insert(anchor, n.clone());
                    }
                    self.push_value(n);
                }
            }
            Event::Alias(id) => {
                let node = self
                    .anchors
                    .get(&id)
                    .cloned()
                    .unwrap_or_else(Node::undefined);
                self.push_value(node);
            }
            Event::DocumentEnd if self.stack.is_empty() => {
                self.done = true;
            }
            _ => {}
        }
    }
}

/// Parse the first document of a YAML stream.
pub fn load(text: &str) -> Result<Node> {
    match load_impl(text) {
        Err(e)
            if e.to_string()
                .contains("invalid indentation in flow construct") =>
        {
            // yaml-cpp accepts flow collections whose continuation lines are
            // not indented (e.g. "key: [a,\nb]"); yaml-rust2 does not. As the
            // indentation has no meaning inside a flow collection, indent the
            // continuation lines and try again.
            load_impl(&indent_flow_continuations(text)).map_err(|_| e)
        }
        r => r,
    }
}

/// Indent the lines that continue a flow collection so that they are more
/// indented than the line where the (outermost) collection starts.
fn indent_flow_continuations(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    let mut depth = 0usize;
    let mut base_indent = 0usize;
    let mut in_single = false;
    let mut in_double = false;
    for line in text.split_inclusive('\n') {
        let leading = line.chars().take_while(|c| *c == ' ').count();
        if depth > 0 && leading < base_indent + 1 {
            out.extend(std::iter::repeat_n(' ', base_indent + 1 - leading));
        }
        let mut prev_ws = true;
        let mut chars = line.chars().peekable();
        while let Some(c) = chars.next() {
            out.push(c);
            if in_single {
                if c == '\'' {
                    if chars.peek() == Some(&'\'') {
                        out.push('\'');
                        chars.next();
                    } else {
                        in_single = false;
                    }
                }
            } else if in_double {
                if c == '\\' {
                    if let Some(n) = chars.next() {
                        out.push(n);
                    }
                } else if c == '"' {
                    in_double = false;
                }
            } else {
                match c {
                    '#' if prev_ws => {
                        // Comment: copy the rest of the line as is.
                        out.extend(chars.by_ref());
                        break;
                    }
                    '\'' => in_single = true,
                    '"' => in_double = true,
                    '[' | '{' => {
                        if depth == 0 {
                            base_indent = leading;
                        }
                        depth += 1;
                    }
                    ']' | '}' => depth = depth.saturating_sub(1),
                    _ => {}
                }
            }
            prev_ws = c.is_whitespace();
        }
    }
    out
}

fn load_impl(text: &str) -> Result<Node> {
    let chars: Vec<char> = text.chars().collect();
    let mut line_starts = vec![0usize];
    for (i, c) in chars.iter().enumerate() {
        if *c == '\n' {
            line_starts.push(i + 1);
        }
    }
    let mut b = Builder {
        chars: &chars,
        line_starts,
        stack: Vec::new(),
        root: None,
        anchors: HashMap::new(),
        done: false,
    };
    let mut parser = Parser::new_from_str(text);
    parser.load(&mut b, false).map_err(|e| {
        let m = e.marker();
        Error::msg(format!(
            "yaml-cpp: error at line {}, column {}: {}",
            m.line(),
            m.col() + 1,
            e.info()
        ))
    })?;
    Ok(b.root.unwrap_or_else(Node::undefined))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_unindented_flow_continuation() {
        let n = load("a: [cs1\t\n   \n,   \ncs2]\nb: 1\n").unwrap();
        assert_eq!(n.get("a").unwrap().seq_items().len(), 2);
        assert_eq!(n.get("b").unwrap().as_f64().unwrap(), 1.0);
    }

    #[test]
    fn parse_basic() {
        let n = load("a: 1\nb: [1, 2]\nc: !<View> {name: x}\nd:\ne: ''\n").unwrap();
        assert!(n.is_map());
        assert_eq!(n.tag, "?");
        assert_eq!(n.get("a").unwrap().as_f64().unwrap(), 1.0);
        assert_eq!(n.get("b").unwrap().as_f64_vec().unwrap(), vec![1.0, 2.0]);
        let c = n.get("c").unwrap();
        assert_eq!(c.tag, "View");
        assert_eq!(c.line, 2);
        assert!(n.get("d").unwrap().is_null());
        assert_eq!(n.get("e").unwrap().as_string().unwrap(), "");
        assert_eq!(n.get("e").unwrap().tag, "!");
        let keys: Vec<_> = n
            .map_entries()
            .iter()
            .map(|(k, _)| k.as_string().unwrap())
            .collect();
        assert_eq!(keys, vec!["a", "b", "c", "d", "e"]);
        assert_eq!(n.map_entries()[1].0.line, 1);
    }

    #[test]
    fn block_tag_mark() {
        let n = load("cs:\n  - !<ColorSpace>\n      name: raw\n").unwrap();
        let cs = &n.get("cs").unwrap().seq_items()[0];
        assert_eq!(cs.tag, "ColorSpace");
        assert_eq!(cs.line, 1);
    }

    #[test]
    fn conversions() {
        let n = load("a: yes\nb: False\nc: tRue\nd: 1e\ne: .5\nf: -2.5e-3\n").unwrap();
        assert!(n.get("a").unwrap().as_bool().unwrap());
        assert!(!n.get("b").unwrap().as_bool().unwrap());
        assert!(n.get("c").unwrap().as_bool().is_err());
        assert!(n.get("d").unwrap().as_f64().is_err());
        assert_eq!(n.get("e").unwrap().as_f64().unwrap(), 0.5);
        assert_eq!(n.get("f").unwrap().as_f64().unwrap(), -2.5e-3);
        assert_eq!(
            n.get("a").unwrap().as_f64().unwrap_err().to_string(),
            "yaml-cpp: error at line 1, column 4: bad conversion"
        );
        assert!(load("").unwrap().is_null());
    }
}
