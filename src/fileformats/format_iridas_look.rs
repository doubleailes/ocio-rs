//! Iridas `look` format (port of `FileFormatIridasLook.cpp`).
//!
//! An XML format containing `<shaders>`, a series of layers describing the
//! operations and their parameters (irrelevant to us in this context). This
//! series of shaders is baked into the `<LUT>` section:
//!
//! ```text
//! <?xml version="1.0" ?>
//! <look>
//!   <shaders>
//!     # anything in here is useless to us
//!   </shaders>
//!   <LUT>
//!     <size>"8"</size> # Size of 3D LUT
//!     <data>"
//!       0000008000000080000000802CF52E3D2DF52E3D2DF52E3D2CF5AE3D2DF5AE3D
//!       # ...cut...
//!       5A216A3F5A216A3FAD10753FAD10753FAD10753F0000803F0000803F0000803F"
//!     </data>
//!   </LUT>
//! </look>
//! ```
//!
//! The LUT data contains a 3D LUT, as a hex-encoded series of 32-bit
//! floats, with little-endian bit-ordering. LUT value ordering is red
//! fastest. The data is parsed by removing all white spaces and quotes, and
//! taking 8 characters at a time.
//!
//! OCIO relies on expat for the XML parsing; a minimal XML tokenizer
//! reproducing the expat callbacks used by the reader is included here.

use super::utils::{from_chars_i64, new_lut3d, set_lut3d_from_red_fastest, MAX_3D_LUT_LENGTH};
use super::{bake_capability, capability, CachedFile, FileFormat, FormatInfo};
use crate::error::{Error, Result};
use crate::transforms::GroupTransform;
use crate::types::{BitDepth, Interpolation};

/// Convert an hex ascii character to its value.
pub(crate) fn hexasciitoint(character: u8) -> Option<u8> {
    match character {
        b'0'..=b'9' => Some(character - b'0'),
        b'A'..=b'F' => Some(10 + character - b'A'),
        b'a'..=b'f' => Some(10 + character - b'a'),
        _ => None,
    }
}

/// Convert 8 hex ascii characters holding a little-endian `f32` to the
/// float value ("AD10753F" -> 0.9572857022285461 on all architectures).
pub(crate) fn hexasciitofloat(ascii: &[u8]) -> Option<f32> {
    if ascii.len() < 8 {
        return None;
    }
    let mut nums = [0u8; 8];
    for (n, &c) in nums.iter_mut().zip(ascii.iter()) {
        *n = hexasciitoint(c)?;
    }
    let bytes = [
        nums[1] | (nums[0] << 4),
        nums[3] | (nums[2] << 4),
        nums[5] | (nums[4] << 4),
        nums[7] | (nums[6] << 4),
    ];
    Some(f32::from_le_bytes(bytes))
}

// ---------------------------------------------------------------------------
// Minimal XML event parser (the subset of expat used by the reader).

/// XML parsing events.
enum XmlEvent<'a> {
    Start(&'a str),
    End(&'a str),
    Chars(&'a str),
}

/// Reasons for a parse failure.
enum ParseFailure {
    /// Unbalanced element tags (`XML_ERROR_TAG_MISMATCH`).
    TagMismatch,
    /// Other XML errors, with the expat message.
    Xml(&'static str),
    /// Error raised by the event handler.
    Handler(Error),
}

fn is_name_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_' || c == b':' || c >= 0x80
}

fn is_name_char(c: u8) -> bool {
    is_name_start(c) || c.is_ascii_digit() || c == b'-' || c == b'.'
}

fn is_xml_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r')
}

/// Decode the character references and predefined entities of `s`.
fn decode_entities(s: &str) -> std::result::Result<String, ParseFailure> {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(pos) = rest.find('&') {
        out.push_str(&rest[..pos]);
        let after = &rest[pos + 1..];
        let end = after
            .find(';')
            .ok_or(ParseFailure::Xml("not well-formed (invalid token)"))?;
        let entity = &after[..end];
        match entity {
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "amp" => out.push('&'),
            "quot" => out.push('"'),
            "apos" => out.push('\''),
            _ => {
                let code = if let Some(hex) = entity.strip_prefix("#x") {
                    u32::from_str_radix(hex, 16).ok()
                } else if let Some(dec) = entity.strip_prefix('#') {
                    dec.parse::<u32>().ok()
                } else {
                    return Err(ParseFailure::Xml("undefined entity"));
                };
                match code.and_then(char::from_u32) {
                    Some(c) => out.push(c),
                    None => return Err(ParseFailure::Xml("reference to invalid character number")),
                }
            }
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

/// A minimal XML parser calling `handler` for each event with the (1-based)
/// line number at which expat would report it. Character data is reported
/// line by line, a new line being reported as its own `"\n"` event (as
/// expat does when fed line by line). On failure, the line of the error is
/// returned with it.
struct XmlParser<'a> {
    doc: &'a str,
    pos: usize,
    line: usize,
}

impl<'a> XmlParser<'a> {
    fn new(doc: &'a str) -> Self {
        Self {
            doc,
            pos: 0,
            line: 1,
        }
    }

    fn bytes(&self) -> &'a [u8] {
        self.doc.as_bytes()
    }

    /// Advance to `end`, counting the new lines.
    fn advance_to(&mut self, end: usize) {
        let end = end.min(self.doc.len());
        self.line += self.bytes()[self.pos..end]
            .iter()
            .filter(|&&c| c == b'\n')
            .count();
        self.pos = end;
    }

    /// Skip past the next occurrence of `pat`.
    fn skip_past(&mut self, pat: &str) -> std::result::Result<(), ParseFailure> {
        match self.doc[self.pos..].find(pat) {
            Some(p) => {
                self.advance_to(self.pos + p + pat.len());
                Ok(())
            }
            None => {
                self.advance_to(self.doc.len());
                Err(ParseFailure::Xml("unclosed token"))
            }
        }
    }

    fn skip_spaces(&mut self) {
        let b = self.bytes();
        let mut i = self.pos;
        while i < b.len() && is_xml_space(b[i]) {
            i += 1;
        }
        self.advance_to(i);
    }

    fn parse_name(&mut self) -> std::result::Result<&'a str, ParseFailure> {
        let b = self.bytes();
        let start = self.pos;
        if start >= b.len() || !is_name_start(b[start]) {
            return Err(ParseFailure::Xml("not well-formed (invalid token)"));
        }
        let mut i = start + 1;
        while i < b.len() && is_name_char(b[i]) {
            i += 1;
        }
        self.pos = i;
        Ok(&self.doc[start..i])
    }

    fn expect(&mut self, c: u8) -> std::result::Result<(), ParseFailure> {
        if self.pos < self.doc.len() && self.bytes()[self.pos] == c {
            self.advance_to(self.pos + 1);
            Ok(())
        } else if self.pos >= self.doc.len() {
            Err(ParseFailure::Xml("unclosed token"))
        } else {
            Err(ParseFailure::Xml("not well-formed (invalid token)"))
        }
    }

    /// Report character data, split at new lines.
    fn report_chars<F>(
        &mut self,
        text: &str,
        handler: &mut F,
    ) -> std::result::Result<(), ParseFailure>
    where
        F: FnMut(XmlEvent, usize) -> Result<()>,
    {
        let decoded = decode_entities(text)?;
        let mut rest = decoded.as_str();
        while !rest.is_empty() {
            match rest.find('\n') {
                Some(p) => {
                    if p > 0 {
                        handler(XmlEvent::Chars(&rest[..p]), self.line)
                            .map_err(ParseFailure::Handler)?;
                    }
                    handler(XmlEvent::Chars("\n"), self.line).map_err(ParseFailure::Handler)?;
                    self.line += 1;
                    rest = &rest[p + 1..];
                }
                None => {
                    handler(XmlEvent::Chars(rest), self.line).map_err(ParseFailure::Handler)?;
                    rest = "";
                }
            }
        }
        Ok(())
    }

    fn parse<F>(&mut self, mut handler: F) -> std::result::Result<(), (ParseFailure, usize)>
    where
        F: FnMut(XmlEvent, usize) -> Result<()>,
    {
        self.parse_impl(&mut handler).map_err(|e| (e, self.line))
    }

    fn parse_impl<F>(&mut self, handler: &mut F) -> std::result::Result<(), ParseFailure>
    where
        F: FnMut(XmlEvent, usize) -> Result<()>,
    {
        let mut stack: Vec<&'a str> = Vec::new();
        let mut root_seen = false;

        while self.pos < self.doc.len() {
            let rest = &self.doc[self.pos..];
            if rest.starts_with("<?") {
                self.skip_past("?>")?;
            } else if rest.starts_with("<!--") {
                self.skip_past("-->")?;
            } else if rest.starts_with("<![CDATA[") {
                if stack.is_empty() {
                    return Err(ParseFailure::Xml("syntax error"));
                }
                let start = self.pos + 9;
                let Some(len) = self.doc[start..].find("]]>") else {
                    self.advance_to(self.doc.len());
                    return Err(ParseFailure::Xml("unclosed CDATA section"));
                };
                let text = &self.doc[start..start + len];
                self.pos = start;
                // CDATA content is not entity decoded.
                let mut r = text;
                while !r.is_empty() {
                    match r.find('\n') {
                        Some(p) => {
                            if p > 0 {
                                handler(XmlEvent::Chars(&r[..p]), self.line)
                                    .map_err(ParseFailure::Handler)?;
                            }
                            handler(XmlEvent::Chars("\n"), self.line)
                                .map_err(ParseFailure::Handler)?;
                            self.line += 1;
                            r = &r[p + 1..];
                        }
                        None => {
                            handler(XmlEvent::Chars(r), self.line)
                                .map_err(ParseFailure::Handler)?;
                            r = "";
                        }
                    }
                }
                self.pos = start + len + 3;
            } else if rest.starts_with("<!") {
                // DOCTYPE (an internal subset is skipped as a whole).
                if root_seen {
                    return Err(ParseFailure::Xml("syntax error"));
                }
                match (rest.find('['), rest.find('>')) {
                    (Some(b), Some(g)) if b < g => self.skip_past("]>")?,
                    _ => self.skip_past(">")?,
                }
            } else if rest.starts_with("</") {
                self.pos += 2;
                let name = self.parse_name()?;
                self.skip_spaces();
                self.expect(b'>')?;
                match stack.pop() {
                    Some(open) if open == name => {}
                    _ => return Err(ParseFailure::TagMismatch),
                }
                handler(XmlEvent::End(name), self.line).map_err(ParseFailure::Handler)?;
            } else if rest.starts_with('<') {
                if root_seen && stack.is_empty() {
                    return Err(ParseFailure::Xml("junk after document element"));
                }
                self.pos += 1;
                let name = self.parse_name()?;
                // Attributes.
                let mut empty = false;
                loop {
                    let had_space =
                        self.pos < self.doc.len() && is_xml_space(self.bytes()[self.pos]);
                    self.skip_spaces();
                    if self.pos >= self.doc.len() {
                        return Err(ParseFailure::Xml("unclosed token"));
                    }
                    match self.bytes()[self.pos] {
                        b'>' => {
                            self.advance_to(self.pos + 1);
                            break;
                        }
                        b'/' => {
                            self.advance_to(self.pos + 1);
                            self.expect(b'>')?;
                            empty = true;
                            break;
                        }
                        _ => {
                            if !had_space {
                                return Err(ParseFailure::Xml("not well-formed (invalid token)"));
                            }
                            self.parse_name()?;
                            self.skip_spaces();
                            self.expect(b'=')?;
                            self.skip_spaces();
                            if self.pos >= self.doc.len() {
                                return Err(ParseFailure::Xml("unclosed token"));
                            }
                            let quote = self.bytes()[self.pos];
                            if quote != b'"' && quote != b'\'' {
                                return Err(ParseFailure::Xml("not well-formed (invalid token)"));
                            }
                            let start = self.pos + 1;
                            match self.bytes()[start..].iter().position(|&c| c == quote) {
                                Some(len) => {
                                    decode_entities(&self.doc[start..start + len])?;
                                    self.advance_to(start + len + 1);
                                }
                                None => {
                                    self.advance_to(self.doc.len());
                                    return Err(ParseFailure::Xml("unclosed token"));
                                }
                            }
                        }
                    }
                }
                root_seen = true;
                handler(XmlEvent::Start(name), self.line).map_err(ParseFailure::Handler)?;
                if empty {
                    handler(XmlEvent::End(name), self.line).map_err(ParseFailure::Handler)?;
                } else {
                    stack.push(name);
                }
            } else {
                // Character data up to the next markup.
                let end = rest
                    .find('<')
                    .map(|p| self.pos + p)
                    .unwrap_or(self.doc.len());
                let text = &self.doc[self.pos..end];
                if stack.is_empty() {
                    if !text.bytes().all(is_xml_space) {
                        return Err(ParseFailure::Xml(if root_seen {
                            "junk after document element"
                        } else {
                            "syntax error"
                        }));
                    }
                    self.advance_to(end);
                } else {
                    self.report_chars(text, handler)?;
                    self.pos = end;
                }
            }
        }

        if !stack.is_empty() || !root_seen {
            return Err(ParseFailure::Xml("no element found"));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// The reader.

/// Port of the `XMLParserHelper` state machine.
struct LookParser<'a> {
    file_name: &'a str,
    line_number: usize,
    ignoring: i32,
    in_look: bool,
    in_lut: bool,
    in_mask: bool,
    size: bool,
    data: bool,
    lut_size: i32,
    lut_string: String,
}

impl<'a> LookParser<'a> {
    fn new(file_name: &'a str) -> Self {
        Self {
            file_name,
            line_number: 0,
            ignoring: 0,
            in_look: false,
            in_lut: false,
            in_mask: false,
            size: false,
            data: false,
            lut_size: 0,
            lut_string: String::new(),
        }
    }

    fn error(&self, error: &str) -> Error {
        Error::msg(format!(
            "Error parsing Iridas Look file ({}). Error is: {}. At line ({})",
            self.file_name, error, self.line_number
        ))
    }

    fn parse(&mut self, data: &[u8]) -> Result<()> {
        let text = String::from_utf8_lossy(data).replace("\r\n", "\n");
        let mut parser = XmlParser::new(&text);
        let res = parser.parse(|event, line| {
            self.line_number = line;
            match event {
                XmlEvent::Start(name) => self.start_element(name),
                XmlEvent::End(name) => self.end_element(name),
                XmlEvent::Chars(s) => self.character_data(s),
            }
        });
        match res {
            Ok(()) => Ok(()),
            Err((failure, line)) => {
                self.line_number = line;
                Err(match failure {
                    ParseFailure::Handler(e) => e,
                    ParseFailure::TagMismatch => {
                        self.error("XML parsing error (unbalanced element tags)")
                    }
                    ParseFailure::Xml(msg) => self.error(&format!("XML parsing error: {msg}")),
                })
            }
        }
    }

    /// Start the parsing of one element.
    fn start_element(&mut self, name: &str) -> Result<()> {
        if name.is_empty() {
            return Err(self.error("Internal error"));
        }

        if self.ignoring > 0 {
            self.ignoring += 1;
            if self.in_mask {
                // Non-empty mask.
                return Err(self.error("Cannot load .look LUT containing mask"));
            }
        } else if name == "look" {
            if self.in_look {
                return Err(self.error("<look> node can not be inside a <look> node"));
            }
            self.in_look = true;
        } else if !self.in_look {
            return Err(self.error("Expecting root node to be a look node"));
        } else if !self.in_lut {
            if name == "LUT" {
                self.in_lut = true;
            } else if name == "mask" {
                self.in_mask = true;
                self.ignoring += 1;
            } else {
                self.ignoring += 1;
            }
        } else if name == "size" {
            self.size = true;
        } else if name == "data" {
            self.data = true;
        }
        Ok(())
    }

    /// End the parsing of one element.
    fn end_element(&mut self, name: &str) -> Result<()> {
        if name.is_empty() {
            return Err(Error::msg("XML internal parsing error."));
        }

        if self.ignoring > 0 {
            self.ignoring -= 1;
        } else if self.size {
            if name != "size" {
                return Err(self.error("Expecting <size> end"));
            }
            // Reading size.
            self.size = false;
        } else if self.data {
            if name != "data" {
                return Err(self.error("Expecting <data> end"));
            }
            // Reading data.
            self.data = false;
        } else if self.in_lut {
            if name != "LUT" {
                return Err(self.error("Expecting <LUT> end"));
            }
            // Reading LUT finished.
            self.in_lut = false;
        } else if self.in_look {
            if name != "look" {
                return Err(self.error("Expecting <look> end"));
            }
            // Reading look finished.
            self.in_look = false;
        } else if self.in_mask {
            if name != "mask" {
                return Err(self.error("Expecting <mask> end"));
            }
            // Reading mask finished.
            self.in_mask = false;
        }
        Ok(())
    }

    /// Handle the strings within an element.
    fn character_data(&mut self, s: &str) -> Result<()> {
        if s.is_empty() {
            return Ok(());
        }
        if s.starts_with('\0') {
            return Err(self.error("XML parsing error: attribute illegal"));
        }

        // Parsing a single new line. This is valid.
        if s == "\n" {
            return Ok(());
        }

        if self.size {
            // Strip quotes and spaces.
            let size_clean = s.trim_matches(|c| c == '\'' || c == '"' || c == ' ');
            let Some(size_3d) = from_chars_i64(size_clean) else {
                return Err(self.error(&format!(
                    "Invalid LUT size value: '{s}'. Expected quoted integer"
                )));
            };
            if size_3d < 2 || size_3d > MAX_3D_LUT_LENGTH as i64 {
                return Err(self.error(&format!(
                    "Invalid LUT size value: '{s}'. Expected value between 2 and {MAX_3D_LUT_LENGTH}"
                )));
            }
            self.lut_size = size_3d as i32;
        } else if self.data {
            // Remove spaces, quotes and newlines.
            let what: String = s
                .chars()
                .filter(|&c| c != ' ' && c != '"' && c != '\'' && c != '\n')
                .collect();
            // Append to the LUT string (capped at the max possible size).
            if self.lut_string.len() + what.len()
                > MAX_3D_LUT_LENGTH * MAX_3D_LUT_LENGTH * MAX_3D_LUT_LENGTH * 3 * 8
            {
                return Err(self.error("Too many characters in 'data' block"));
            }
            self.lut_string.push_str(&what);
        }
        Ok(())
    }

    /// Decode the LUT (port of `getLut`).
    fn lut(&self) -> Result<(usize, Vec<f32>)> {
        if self.lut_string.len() % 8 != 0 {
            crate::bail!(
                "Error parsing Iridas Look file ({}). Number of characters in 'data' must be multiple of 8. {} elements found.",
                self.file_name,
                self.lut_string.len()
            );
        }

        let lut_size = self.lut_size.max(0) as usize;
        let expected = 3 * lut_size * lut_size * lut_size;
        let mut lut = Vec::with_capacity(expected);
        for (i, chunk) in self.lut_string.as_bytes().chunks(8).enumerate() {
            match hexasciitofloat(chunk) {
                Some(v) => lut.push(v),
                None => crate::bail!(
                    "Error parsing Iridas Look file ({}). Non-hex characters found in 'data' block at index '{}'.",
                    self.file_name,
                    8 * i
                ),
            }
        }

        if expected != lut.len() {
            crate::bail!(
                "Error parsing Iridas Look file ({}). Incorrect number of lut3d entries. Found {} values, expected {}.",
                self.file_name,
                lut.len(),
                expected
            );
        }
        Ok((lut_size, lut))
    }
}

struct LocalFileFormat;

pub(crate) fn create() -> Box<dyn FileFormat> {
    Box::new(LocalFileFormat)
}

impl FileFormat for LocalFileFormat {
    fn format_info(&self) -> Vec<FormatInfo> {
        vec![FormatInfo {
            name: "iridas_look",
            extension: "look",
            capabilities: capability::READ,
            bake_capabilities: bake_capability::NONE,
        }]
    }

    fn read(&self, data: &[u8], file_name: &str, interp: Interpolation) -> Result<CachedFile> {
        let mut parser = LookParser::new(file_name);
        parser.parse(data)?;

        // TODO: There is a LUT1D section in some .look files, which we could
        // use if available.

        let (grid_size, raw) = parser.lut()?;
        let mut lut = new_lut3d(grid_size, interp, BitDepth::F32);
        set_lut3d_from_red_fastest(&mut lut, &raw)?;

        let mut group = GroupTransform::new();
        group.append(lut);
        Ok(CachedFile::new(group))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transforms::Transform;

    fn read(content: &str) -> Result<CachedFile> {
        LocalFileFormat.read(content.as_bytes(), "", Interpolation::Default)
    }

    #[test]
    fn format_info() {
        let info = LocalFileFormat.format_info();
        assert_eq!(info.len(), 1);
        assert_eq!(info[0].name, "iridas_look");
        assert_eq!(info[0].extension, "look");
        assert_eq!(info[0].capabilities, capability::READ);
    }

    #[test]
    fn test_hexasciitoint() {
        assert_eq!(hexasciitoint(b'a'), Some(10));
        assert_eq!(hexasciitoint(b'A'), Some(10));
        assert_eq!(hexasciitoint(b'f'), Some(15));
        assert_eq!(hexasciitoint(b'F'), Some(15));
        assert_eq!(hexasciitoint(b'0'), Some(0));
        assert_eq!(hexasciitoint(b'9'), Some(9));
        assert_eq!(hexasciitoint(b'\n'), None);
        assert_eq!(hexasciitoint(b'j'), None);
        assert_eq!(hexasciitoint(b'x'), None);
    }

    #[test]
    fn test_hexasciitofloat() {
        assert_eq!(hexasciitofloat(b"0000003F"), Some(0.5));
        assert_eq!(hexasciitofloat(b"0000803F"), Some(1.0));
        assert_eq!(hexasciitofloat(b"AD10753F"), Some(0.9572857022285461f32));
        assert_eq!(hexasciitofloat(b"AD10X53F"), None);
    }

    /// Build a look file with a 2x2x2 LUT of the given values (red fastest).
    fn look_2x2x2(values: &[f32]) -> String {
        let mut hex = String::new();
        for v in values {
            for b in v.to_le_bytes() {
                hex.push_str(&format!("{b:02X}"));
            }
        }
        let (a, b) = hex.split_at(hex.len() / 2);
        format!(
            "<?xml version=\"1.0\" ?>\n<look>\n  <shaders>\n    <base>\n      <visible>\"1\"</visible>\n    </base>\n  </shaders>\n  <LUT>\n    <size>\"2\"</size>\n    <data>\"\n      {a}\n      {b}\"\n    </data>\n  </LUT>\n</look>\n"
        )
    }

    #[test]
    fn simple() {
        let values: Vec<f32> = (0..24).map(|i| i as f32 / 23.0).collect();
        let file = read(&look_2x2x2(&values)).unwrap();
        assert_eq!(file.group.num_transforms(), 1);
        let Transform::Lut3D(lut) = &file.group.transforms[0] else {
            panic!("expected a Lut3D")
        };
        assert_eq!(lut.grid_size, 2);
        assert_eq!(lut.file_output_bit_depth, BitDepth::F32);
        // Red fastest in the file.
        assert_eq!(lut.value(0, 0, 0), [values[0], values[1], values[2]]);
        assert_eq!(lut.value(1, 0, 0), [values[3], values[4], values[5]]);
        assert_eq!(lut.value(0, 0, 1), [values[12], values[13], values[14]]);
    }

    #[test]
    fn simple3d() {
        // The content of the OCIO simple3d unit test.
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/files/");
        let content = std::fs::read(format!("{dir}iridas_look_simple3d.look")).unwrap();
        let expected: Vec<f32> =
            std::fs::read_to_string(format!("{dir}iridas_look_simple3d_expected.txt"))
                .unwrap()
                .lines()
                .map(|l| l.parse::<f32>().unwrap())
                .collect();

        let file = LocalFileFormat
            .read(&content, "", Interpolation::Default)
            .unwrap();
        let Transform::Lut3D(lut) = &file.group.transforms[0] else {
            panic!("expected a Lut3D")
        };
        let lut_size = 8;
        assert_eq!(lut.grid_size, lut_size);
        assert_eq!(lut.values.len(), 8 * 8 * 8 * 3);

        // The expected cube is stored in red-fastest order.
        let (mut r, mut g, mut b) = (0, 0, 0);
        for i in 0..lut_size * lut_size * lut_size {
            let idx = 3 * (b + lut_size * (g + lut_size * r));
            for c in 0..3 {
                assert!(
                    (expected[i * 3 + c] - lut.values[idx + c]).abs() <= 1e-4,
                    "entry {i}"
                );
            }
            r += 1;
            if r == lut_size {
                r = 0;
                g += 1;
                if g == lut_size {
                    g = 0;
                    b += 1;
                }
            }
        }
    }

    #[test]
    fn fail_on_mask() {
        let content = "<?xml version=\"1.0\" ?>\n<look>\n  <shaders>\n    <base>\n      <rangeversion>\"2\"</rangeversion>\n    </base>\n  </shaders>\n  <mask>\n    <name>\"Untitled00_00_00_00\"</name>\n    <activecontour>\"0\"</activecontour>\n  </mask>\n  <LUT>\n    <size>\"8\"</size>\n    <data>\"\n      000000000000000000000000878B933D000000000000000057BC563E00000000\n      ...truncated, should never be parsed due to mask section\"\n    </data>\n  </LUT>\n</look>\n";
        let e = read(content).unwrap_err();
        assert!(
            e.message()
                .contains("Cannot load .look LUT containing mask"),
            "{}",
            e.message()
        );
        assert_eq!(
            e.message(),
            "Error parsing Iridas Look file (). Error is: Cannot load .look LUT containing mask. At line (9)"
        );
    }

    #[test]
    fn read_failures() {
        let values: Vec<f32> = (0..24).map(|i| i as f32).collect();
        let good = look_2x2x2(&values);

        let e = read(&good.replace("</LUT>", "</LUTX>")).unwrap_err();
        assert!(
            e.message()
                .contains("XML parsing error (unbalanced element tags)"),
            "{}",
            e.message()
        );

        let e = read(
            &good
                .replace("<look>", "<other>")
                .replace("</look>", "</other>"),
        )
        .unwrap_err();
        assert!(e
            .message()
            .contains("Expecting root node to be a look node"));

        let e = read(&good.replace("\"2\"", "\"x\"")).unwrap_err();
        assert!(e
            .message()
            .contains("Invalid LUT size value: '\"x\"'. Expected quoted integer"));

        let e = read(&good.replace("\"2\"", "\"200\"")).unwrap_err();
        assert!(e.message().contains("Expected value between 2 and 129"));

        let e = read(&good.replace("\"2\"", "\"3\"")).unwrap_err();
        assert!(e
            .message()
            .contains("Incorrect number of lut3d entries. Found 24 values, expected 81."));

        let e = read(&good.replacen("0000", "000", 1)).unwrap_err();
        assert!(e
            .message()
            .contains("Number of characters in 'data' must be multiple of 8."));

        let e = read(&good.replacen("0000", "00G0", 1)).unwrap_err();
        assert!(e
            .message()
            .contains("Non-hex characters found in 'data' block at index '0'."));

        let e = read("<look>\n").unwrap_err();
        assert!(
            e.message().contains("XML parsing error: no element found"),
            "{}",
            e.message()
        );
    }

    #[test]
    fn xml_parser() {
        let mut events = Vec::new();
        let mut p = XmlParser::new(
            "<?xml version=\"1.0\"?>\n<!-- c -->\n<a x='1'>t&amp;u\nv<b/><![CDATA[<z>]]></a>\n",
        );
        p.parse(|e, line| {
            events.push(match e {
                XmlEvent::Start(n) => format!("S:{n}:{line}"),
                XmlEvent::End(n) => format!("E:{n}:{line}"),
                XmlEvent::Chars(c) => format!("C:{c}:{line}"),
            });
            Ok(())
        })
        .map_err(|_| ())
        .unwrap();
        assert_eq!(
            events,
            vec!["S:a:3", "C:t&u:3", "C:\n:3", "C:v:4", "S:b:4", "E:b:4", "C:<z>:4", "E:a:4"]
        );
    }
}
