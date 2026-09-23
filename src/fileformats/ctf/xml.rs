//! XML helpers shared by the CLF/CTF and CDL (CC / CCC / CDL) formats.
//!
//! Port of `fileformats/xmlutils/XMLReaderUtils.*`, `XMLWriterUtils.*` and of
//! the parts of the expat parser behavior OCIO relies upon:
//!
//! * [`parse_xml`] is a small SAX parser that emits the same callbacks as
//!   expat does for OCIO: start / end of elements and character data where
//!   every line break and every entity reference is delivered as a separate
//!   chunk. Line numbers are those of the line fed to the parser when the
//!   callback happens (OCIO feeds expat line by line).
//! * [`get_numbers`], [`parse_number`], ... reproduce OCIO's number parsing
//!   (`std::from_chars` based) and error messages.
//! * [`XmlFormatter`] writes indented XML exactly like OCIO does.
//! * [`fmt_g`] reproduces the C++ `std::ostream` default floating point
//!   formatting (`%g` style with a given precision).

use crate::error::{Error, Result};

// Strings used by CDL and CLF parsers or writers.

/// `id` attribute.
pub(crate) const ATTR_ID: &str = "id";
/// `name` attribute.
pub(crate) const ATTR_NAME: &str = "name";
/// `xmlns` attribute.
pub(crate) const ATTR_XMLNS: &str = "xmlns";

/// `ColorCorrection` element.
pub(crate) const CDL_TAG_COLOR_CORRECTION: &str = "ColorCorrection";

/// `Description` element.
pub(crate) const TAG_DESCRIPTION: &str = "Description";
/// `Offset` element.
pub(crate) const TAG_OFFSET: &str = "Offset";
/// `Power` element.
pub(crate) const TAG_POWER: &str = "Power";
/// `SatNode` element.
pub(crate) const TAG_SATNODE: &str = "SatNode";
/// Alternate spelling of the `SatNode` element.
pub(crate) const TAG_SATNODEALT: &str = "SATNode";
/// `Saturation` element.
pub(crate) const TAG_SATURATION: &str = "Saturation";
/// `Slope` element.
pub(crate) const TAG_SLOPE: &str = "Slope";
/// `SOPNode` element.
pub(crate) const TAG_SOPNODE: &str = "SOPNode";

// ---------------------------------------------------------------------------
// String helpers (XMLReaderUtils)

/// Case insensitive (ASCII) string comparison, like `Platform::Strcasecmp == 0`.
pub(crate) fn eq_ic(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// Truncate a string for display purposes (default limit of 17 bytes).
pub(crate) fn truncate_string(s: &str) -> String {
    truncate_string_to(s, 17)
}

/// Truncate a string to at most `limit` bytes (for display purposes).
pub(crate) fn truncate_string_to(s: &str, limit: usize) -> String {
    let b = s.as_bytes();
    let n = limit.min(b.len());
    String::from_utf8_lossy(&b[..n]).into_owned()
}

/// Is `c` a white space character (`' '`, `'\n'`, `'\t'`, `'\r'`, `'\v'`, `'\f'`)?
pub(crate) fn is_space(c: u8) -> bool {
    c == b' ' || c == b'\n' || c == b'\t' || c == b'\r' || c == 0x0b || c == 0x0c
}

/// Is the character a valid number delimiter (space or comma)?
pub(crate) fn is_number_delimiter(c: u8) -> bool {
    is_space(c) || c == b','
}

/// Trim white spaces from the start.
pub(crate) fn ltrim(s: &str) -> &str {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() && is_space(b[i]) {
        i += 1;
    }
    &s[i..]
}

/// Trim white spaces from the end.
pub(crate) fn rtrim(s: &str) -> &str {
    let b = s.as_bytes();
    let mut i = b.len();
    while i > 0 && is_space(b[i - 1]) {
        i -= 1;
    }
    &s[..i]
}

/// Trim white spaces from both ends.
pub(crate) fn trim(s: &str) -> &str {
    rtrim(ltrim(s))
}

/// Get start (first non space character) and end (just after the last non
/// space character) of the sub string of `s` without surrounding spaces.
/// Returns `(0, 0)` if the string is empty or only has white spaces.
pub(crate) fn find_sub_string(s: &[u8]) -> (usize, usize) {
    if s.is_empty() || s[0] == 0 {
        return (0, 0);
    }
    let start = match s.iter().position(|&c| !is_space(c)) {
        Some(p) => p,
        None => return (0, 0),
    };
    // There is at least one non space character.
    let last = s.iter().rposition(|&c| !is_space(c)).unwrap_or(start);
    (start, last + 1)
}

/// Find the position of the next character to start scanning at (skipping
/// delimiters: spaces and commas).
pub(crate) fn find_next_token_start(s: &[u8], pos: usize) -> usize {
    let mut pos = pos;
    while pos < s.len() && is_number_delimiter(s[pos]) {
        pos += 1;
    }
    pos.min(s.len())
}

/// Find the position of the next delimiter (space or comma) in the string.
pub(crate) fn find_delim(s: &[u8], pos: usize) -> usize {
    let mut pos = pos;
    while pos < s.len() && !is_number_delimiter(s[pos]) {
        pos += 1;
    }
    pos.min(s.len())
}

// ---------------------------------------------------------------------------
// Number parsing

/// Error codes of `from_chars`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CharsError {
    InvalidArgument,
    OutOfRange,
}

/// Emulation of OCIO's `NumberUtils::from_chars` for doubles: skips leading
/// white spaces and a `+` sign, handles the `0x` hexadecimal prefix, and
/// parses the longest valid prefix. Returns the parsed value (if any), the
/// number of bytes consumed from the start of `s` and the error code.
fn from_chars(s: &[u8]) -> (Option<f64>, usize, Option<CharsError>) {
    if s.is_empty() {
        return (None, 0, Some(CharsError::InvalidArgument));
    }
    let mut i = 0;
    while i < s.len() && is_space(s[i]) {
        i += 1;
    }
    if i < s.len() && s[i] == b'+' {
        i += 1;
    }
    let first = i;

    if i + 2 < s.len() && s[i] == b'0' && (s[i + 1] == b'x' || s[i + 1] == b'X') {
        i += 2;
        return from_chars_hex(s, i);
    }

    // General format.
    let mut j = i;
    let neg = j < s.len() && s[j] == b'-';
    if neg {
        j += 1;
    }
    // Infinity / NaN.
    let rest = &s[j..];
    let starts_ic =
        |pat: &[u8]| rest.len() >= pat.len() && rest[..pat.len()].eq_ignore_ascii_case(pat);
    if starts_ic(b"infinity") {
        let v = if neg {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
        return (Some(v), j + 8, None);
    }
    if starts_ic(b"inf") {
        let v = if neg {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
        return (Some(v), j + 3, None);
    }
    if starts_ic(b"nan") {
        let mut end = j + 3;
        // Optional n-char-sequence.
        if end < s.len() && s[end] == b'(' {
            let mut k = end + 1;
            while k < s.len() && (s[k].is_ascii_alphanumeric() || s[k] == b'_') {
                k += 1;
            }
            if k < s.len() && s[k] == b')' {
                end = k + 1;
            }
        }
        let v = if neg { -f64::NAN } else { f64::NAN };
        return (Some(v), end, None);
    }

    // Decimal digits.
    let mut k = j;
    let mut ndigits = 0;
    while k < s.len() && s[k].is_ascii_digit() {
        k += 1;
        ndigits += 1;
    }
    if k < s.len() && s[k] == b'.' {
        k += 1;
        while k < s.len() && s[k].is_ascii_digit() {
            k += 1;
            ndigits += 1;
        }
    }
    if ndigits == 0 {
        return (None, first, Some(CharsError::InvalidArgument));
    }
    // Exponent.
    if k < s.len() && (s[k] == b'e' || s[k] == b'E') {
        let mut e = k + 1;
        if e < s.len() && (s[e] == b'+' || s[e] == b'-') {
            e += 1;
        }
        let exp_start = e;
        while e < s.len() && s[e].is_ascii_digit() {
            e += 1;
        }
        if e > exp_start {
            k = e;
        }
    }
    let text = std::str::from_utf8(&s[j..k]).unwrap_or("0");
    let mut v: f64 = text.parse().unwrap_or(0.0);
    if v.is_infinite() {
        // Overflow: the value is not assigned.
        return (None, k, Some(CharsError::OutOfRange));
    }
    if neg {
        v = -v;
    }
    (Some(v), k, None)
}

/// Hexadecimal floating point parsing (`std::chars_format::hex`), starting at
/// `i` (just after the `0x` prefix).
fn from_chars_hex(s: &[u8], i: usize) -> (Option<f64>, usize, Option<CharsError>) {
    let mut k = i;
    let neg = k < s.len() && s[k] == b'-';
    if neg {
        k += 1;
    }
    let mut mant: f64 = 0.0;
    let mut ndigits = 0;
    let mut scale_exp: i32 = 0;
    while k < s.len() && s[k].is_ascii_hexdigit() {
        mant = mant * 16.0 + f64::from((s[k] as char).to_digit(16).unwrap_or(0));
        k += 1;
        ndigits += 1;
    }
    if k < s.len() && s[k] == b'.' {
        k += 1;
        while k < s.len() && s[k].is_ascii_hexdigit() {
            mant = mant * 16.0 + f64::from((s[k] as char).to_digit(16).unwrap_or(0));
            scale_exp -= 4;
            k += 1;
            ndigits += 1;
        }
    }
    if ndigits == 0 {
        return (None, i, Some(CharsError::InvalidArgument));
    }
    let mut exp: i32 = 0;
    if k < s.len() && (s[k] == b'p' || s[k] == b'P') {
        let mut e = k + 1;
        let eneg = e < s.len() && s[e] == b'-';
        if e < s.len() && (s[e] == b'+' || s[e] == b'-') {
            e += 1;
        }
        let start = e;
        let mut val: i32 = 0;
        while e < s.len() && s[e].is_ascii_digit() {
            val = val
                .saturating_mul(10)
                .saturating_add(i32::from(s[e] - b'0'));
            e += 1;
        }
        if e > start {
            exp = if eneg { -val } else { val };
            k = e;
        }
    }
    let mut v = mant * 2f64.powi(exp.saturating_add(scale_exp));
    if v.is_infinite() {
        return (None, k, Some(CharsError::OutOfRange));
    }
    if neg {
        v = -v;
    }
    (Some(v), k, None)
}

/// Numeric types that can be parsed by [`parse_number`].
pub(crate) trait XmlNumber: Copy + Default {
    /// Convert the parsed double (like a C cast).
    fn from_f64(v: f64) -> Self;
    /// Is the conversion lossless (always true for floating point types)?
    fn is_valid(self, v: f64) -> bool;
}

impl XmlNumber for f64 {
    fn from_f64(v: f64) -> Self {
        v
    }
    fn is_valid(self, _v: f64) -> bool {
        true
    }
}

impl XmlNumber for f32 {
    fn from_f64(v: f64) -> Self {
        v as f32
    }
    fn is_valid(self, _v: f64) -> bool {
        true
    }
}

impl XmlNumber for u32 {
    fn from_f64(v: f64) -> Self {
        v as u32
    }
    fn is_valid(self, v: f64) -> bool {
        f64::from(self) == v
    }
}

/// Parse the number found between `start` and `end` in `s` (port of
/// `ParseNumber`). Leading spaces are allowed but the number has to end at
/// `end`.
pub(crate) fn parse_number<T: XmlNumber>(s: &[u8], start: usize, end: usize) -> Result<T> {
    if end == start {
        return Err(Error::msg("ParseNumber: nothing to parse."));
    }
    let sub = &s[start..end];
    let (adj_start, adj_end) = find_sub_string(sub);
    let (val, consumed, err) = from_chars(&sub[adj_start..adj_end]);
    let parsed_str = String::from_utf8_lossy(sub).into_owned();
    let full_str = || truncate_string_to(&String::from_utf8_lossy(&s[..end]), 100);

    if err == Some(CharsError::InvalidArgument) || (adj_start == adj_end) {
        return Err(Error::msg(format!(
            "ParserNumber: Characters '{}' can not be parsed to numbers in '{}'.",
            parsed_str,
            full_str()
        )));
    }
    let dval = val.unwrap_or(0.0);
    let value = T::from_f64(dval);
    if !value.is_valid(dval) {
        return Err(Error::msg(format!(
            "ParserNumber: Characters '{}' are illegal in '{}'.",
            parsed_str,
            full_str()
        )));
    }
    if start + adj_start + consumed != end {
        return Err(Error::msg(format!(
            "ParserNumber: '{}' number is followed by unexpected characters in '{}'.",
            parsed_str,
            full_str()
        )));
    }
    Ok(value)
}

/// Extract the next number of `s` starting at `*pos` (port of
/// `GetNextNumber`). `*pos` is updated to the start of the next token (or
/// the end of the string).
pub(crate) fn get_next_number<T: XmlNumber>(s: &[u8], pos: &mut usize) -> Result<Option<T>> {
    *pos = find_next_token_start(s, *pos);
    if *pos != s.len() {
        let next = find_delim(s, *pos);
        let num = parse_number(s, *pos, next)?;
        *pos = next;
        if *pos != s.len() {
            *pos = find_next_token_start(s, next);
        }
        return Ok(Some(num));
    }
    Ok(None)
}

/// Tokenize a string like `"0 1 2"` of numbers (port of `GetNumbers`).
pub(crate) fn get_numbers<T: XmlNumber>(s: &str) -> Result<Vec<T>> {
    let b = s.as_bytes();
    let mut numbers = Vec::new();
    let mut pos = find_next_token_start(b, 0);
    while pos != b.len() {
        match get_next_number(b, &mut pos)? {
            Some(n) => numbers.push(n),
            None => break,
        }
    }
    Ok(numbers)
}

// ---------------------------------------------------------------------------
// XML escaping

/// Replace the XML special characters by their entity (port of
/// `ConvertSpecialCharToXmlToken`).
pub(crate) fn escape_xml(s: &str) -> String {
    let mut res = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => res.push_str("&quot;"),
            '\'' => res.push_str("&apos;"),
            '<' => res.push_str("&lt;"),
            '>' => res.push_str("&gt;"),
            '&' => res.push_str("&amp;"),
            _ => res.push(c),
        }
    }
    res
}

// ---------------------------------------------------------------------------
// SAX parser

/// Callbacks of [`parse_xml`] (the expat handlers used by OCIO).
pub(crate) trait SaxHandler {
    /// Start of an element. `line` is the current line of the parser.
    fn start_element(&mut self, name: &str, atts: &[(String, String)], line: u32) -> Result<()>;
    /// End of an element.
    fn end_element(&mut self, name: &str, line: u32) -> Result<()>;
    /// A chunk of character data (never empty).
    fn character_data(&mut self, s: &str, line: u32) -> Result<()>;
}

/// Errors of [`parse_xml`].
#[derive(Debug)]
pub(crate) enum SaxError {
    /// An end tag does not match the open element (`XML_ERROR_TAG_MISMATCH`).
    TagMismatch(u32),
    /// Any other XML error: the expat error string and the line.
    Syntax(&'static str, u32),
    /// An error raised by a handler callback.
    Handler(Error),
}

impl From<Error> for SaxError {
    fn from(e: Error) -> Self {
        SaxError::Handler(e)
    }
}

/// Number of lines OCIO feeds to the parser for `data` (every line read with
/// `std::getline`, the last one included even if empty).
pub(crate) fn fed_line_count(data: &[u8]) -> u32 {
    let n = data.iter().filter(|&&c| c == b'\n').count();
    u32::try_from(n + 1).unwrap_or(u32::MAX)
}

/// Decode the file content to a string (UTF-8, or ISO-8859-1 / US-ASCII /
/// UTF-16 when declared or detected).
fn decode(data: &[u8]) -> std::result::Result<String, SaxError> {
    // UTF-16 with a byte order mark.
    if data.len() >= 2 && (data[0] == 0xFF && data[1] == 0xFE || data[0] == 0xFE && data[1] == 0xFF)
    {
        let le = data[0] == 0xFF;
        let units: Vec<u16> = data[2..]
            .chunks(2)
            .map(|c| {
                let (a, b) = (c[0], *c.get(1).unwrap_or(&0));
                if le {
                    u16::from_le_bytes([a, b])
                } else {
                    u16::from_be_bytes([a, b])
                }
            })
            .collect();
        return String::from_utf16(&units)
            .map_err(|_| SaxError::Syntax("not well-formed (invalid token)", 1));
    }
    let data = if data.starts_with(&[0xEF, 0xBB, 0xBF]) {
        &data[3..]
    } else {
        data
    };

    // Look for an encoding declaration.
    let head = &data[..data.len().min(200)];
    let head_str = String::from_utf8_lossy(head);
    let mut latin1 = false;
    if head_str.starts_with("<?xml") {
        if let Some(end) = head_str.find("?>") {
            let decl = &head_str[..end];
            if let Some(p) = decl.find("encoding") {
                let rest = decl[p + 8..].trim_start();
                if let Some(rest) = rest.strip_prefix('=') {
                    let rest = rest.trim_start();
                    let q = rest.chars().next().unwrap_or('"');
                    if q == '"' || q == '\'' {
                        let v = &rest[1..];
                        let enc = v.split(q).next().unwrap_or("").to_ascii_lowercase();
                        if enc == "iso-8859-1" || enc == "latin1" || enc == "latin-1" {
                            latin1 = true;
                        }
                    }
                }
            }
        }
    }
    if latin1 {
        return Ok(data.iter().map(|&c| c as char).collect());
    }
    match std::str::from_utf8(data) {
        Ok(s) => Ok(s.to_string()),
        Err(e) => {
            let line = fed_line_count(&data[..e.valid_up_to()]);
            Err(SaxError::Syntax("not well-formed (invalid token)", line))
        }
    }
}

struct SaxParser<'a> {
    s: &'a str,
    b: &'a [u8],
    pos: usize,
    /// Feeding schedule (see [`FeedSchedule`]).
    sched: FeedSchedule,
    open: Vec<String>,
}

/// Emulation of the way OCIO feeds expat: the file is parsed line by line
/// (each line followed by a newline) and the reported line is the number of
/// lines fed so far. Expat processes the complete tokens available at each
/// call and (since expat 2.6) defers the re-parsing of an incomplete token
/// until enough new data is available ("reparse deferral"). So the callbacks
/// of a token happen when the line containing its end is fed, or later.
#[derive(Debug, Default)]
struct FeedSchedule {
    /// `(position processed up to, fed line)` for each processing call.
    processed: Vec<(usize, u32)>,
    /// `(end of the available data, fed line)` for each processing call.
    available: Vec<(usize, u32)>,
    /// Line of the last (final) call.
    last_line: u32,
}

/// Find `pat` in `b` from `from`.
fn find_bytes(b: &[u8], from: usize, pat: &[u8]) -> Option<usize> {
    if from > b.len() {
        return None;
    }
    b[from..]
        .windows(pat.len())
        .position(|w| w == pat)
        .map(|p| from + p)
}

/// End (exclusive) of a construct ending with `>` outside of quotes (and
/// outside of brackets when `brackets` is set), `n + 1` if unterminated.
fn markup_end(b: &[u8], from: usize, brackets: bool) -> usize {
    let mut j = from;
    let mut depth = 0i32;
    let mut quote: Option<u8> = None;
    while j < b.len() {
        let c = b[j];
        j += 1;
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            b'"' | b'\'' => quote = Some(c),
            b'[' if brackets => depth += 1,
            b']' if brackets => depth -= 1,
            b'>' if depth <= 0 => return j,
            _ => {}
        }
    }
    b.len() + 1
}

/// Positions where the parsing of a call can not stop: `(start, end)` of
/// each markup construct (tags, comments, processing instructions,
/// declarations, CDATA delimiters and references). Unterminated constructs
/// end after the data.
fn markup_intervals(b: &[u8]) -> Vec<(usize, usize)> {
    let n = b.len();
    let mut v = Vec::new();
    let mut i = 0;
    let mut in_cdata = false;
    while i < n {
        if in_cdata {
            match find_bytes(b, i, b"]]>") {
                Some(p) => {
                    v.push((p, p + 3));
                    i = p + 3;
                    in_cdata = false;
                    continue;
                }
                None => break,
            }
        }
        match b[i] {
            b'<' => {
                let rest = &b[i..];
                let end = if rest.starts_with(b"<!--") {
                    find_bytes(b, i + 4, b"-->").map_or(n + 1, |p| p + 3)
                } else if rest.starts_with(b"<![CDATA[") {
                    v.push((i, i + 9));
                    i += 9;
                    in_cdata = true;
                    continue;
                } else if rest.starts_with(b"<?") {
                    find_bytes(b, i + 2, b"?>").map_or(n + 1, |p| p + 2)
                } else if rest.starts_with(b"<!") {
                    markup_end(b, i + 2, true)
                } else {
                    markup_end(b, i + 1, false)
                };
                v.push((i, end));
                i = end;
            }
            b'&' => {
                let end = find_bytes(b, i, b";").map_or(n + 1, |p| p + 1);
                v.push((i, end));
                i = end;
            }
            _ => i += 1,
        }
    }
    v
}

impl FeedSchedule {
    /// Simulate the calls to `XML_Parse` (expat 2.6 buffer management and
    /// reparse deferral heuristic, `XML_CONTEXT_BYTES` = 1024).
    fn new(b: &[u8]) -> Self {
        const INIT_BUFFER_SIZE: usize = 1024;
        const CONTEXT_BYTES: usize = 1024;
        let intervals = markup_intervals(b);
        let advance = |p: usize, e: usize| -> usize {
            // Construct containing `e` strictly.
            let idx = intervals.partition_point(|&(s, _)| s < e);
            if idx > 0 {
                let (s, end) = intervals[idx - 1];
                if s < e && e < end {
                    return s.max(p);
                }
            }
            e
        };
        let mut ends: Vec<usize> = b
            .iter()
            .enumerate()
            .filter(|(_, &c)| c == b'\n')
            .map(|(i, _)| i + 1)
            .collect();
        // The last line is fed with an added newline.
        ends.push(b.len() + 1);

        let mut sched = FeedSchedule::default();
        let (mut lim, mut ptr, mut end) = (0usize, 0usize, 0usize);
        let mut has_buf = false;
        let mut had_before = 0usize;
        let mut p_abs = 0usize;
        let mut prev_end = 0usize;
        let count = ends.len();
        for (k, &e_abs) in ends.iter().enumerate() {
            let line = u32::try_from(k + 1).unwrap_or(u32::MAX);
            let len = e_abs - prev_end;
            prev_end = e_abs;
            let is_final = k + 1 == count;
            // XML_GetBuffer.
            if !has_buf || len > lim - end {
                let keep = ptr.min(CONTEXT_BYTES);
                let needed = len + (end - ptr) + keep;
                if has_buf && needed <= lim {
                    if keep < ptr {
                        let offset = ptr - keep;
                        end -= offset;
                        ptr -= offset;
                    }
                } else {
                    let mut size = if lim == 0 { INIT_BUFFER_SIZE } else { lim };
                    loop {
                        size *= 2;
                        if size >= needed {
                            break;
                        }
                    }
                    if has_buf {
                        end = keep + (end - ptr);
                        ptr = keep;
                    } else {
                        end = 0;
                        ptr = 0;
                    }
                    lim = size;
                    has_buf = true;
                }
            }
            // XML_ParseBuffer.
            end += len;
            let have_now = end - ptr;
            let attempt = is_final || {
                let available = ptr - ptr.min(CONTEXT_BYTES) + (lim - end);
                have_now >= 2 * had_before || len > available
            };
            if attempt {
                sched.available.push((e_abs, line));
                let new_p = if is_final {
                    b.len() + 1
                } else {
                    advance(p_abs, e_abs)
                };
                let consumed = new_p - p_abs;
                had_before = if consumed == 0 { have_now } else { 0 };
                ptr += consumed;
                p_abs = new_p;
                sched.processed.push((new_p, line));
            }
            sched.last_line = line;
        }
        sched
    }

    /// Fed line at which the byte at `pos` is processed.
    fn processed_line(&self, pos: usize) -> u32 {
        let idx = self.processed.partition_point(|&(p, _)| p <= pos);
        self.processed.get(idx).map_or(self.last_line, |&(_, l)| l)
    }

    /// Fed line at which the byte at `pos` is first examined.
    fn available_line(&self, pos: usize) -> u32 {
        let idx = self.available.partition_point(|&(p, _)| p <= pos);
        self.available.get(idx).map_or(self.last_line, |&(_, l)| l)
    }
}

fn is_xml_space(c: u8) -> bool {
    c == b' ' || c == b'\t' || c == b'\r' || c == b'\n'
}

fn is_name_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_' || c == b':' || c >= 0x80
}

fn is_name_char(c: u8) -> bool {
    is_name_start(c) || c.is_ascii_digit() || c == b'-' || c == b'.'
}

const ERR_INVALID_TOKEN: &str = "not well-formed (invalid token)";
const ERR_UNCLOSED_TOKEN: &str = "unclosed token";
const ERR_NO_ELEMENTS: &str = "no element found";
const ERR_SYNTAX: &str = "syntax error";
const ERR_JUNK: &str = "junk after document element";
const ERR_UNDEFINED_ENTITY: &str = "undefined entity";
const ERR_BAD_CHAR_REF: &str = "reference to invalid character number";
const ERR_DUPLICATE_ATTRIBUTE: &str = "duplicate attribute";
const ERR_MISPLACED_XML_PI: &str = "XML or text declaration not at start of entity";
const ERR_UNCLOSED_CDATA: &str = "unclosed CDATA section";

impl<'a> SaxParser<'a> {
    /// Line reported for the callbacks of a token ending at `pos`.
    fn line_at(&mut self, pos: usize) -> u32 {
        self.sched.processed_line(pos)
    }

    fn eof_line(&self) -> u32 {
        self.sched.last_line
    }

    fn err_at(&mut self, msg: &'static str, pos: usize) -> SaxError {
        let l = self.sched.available_line(pos);
        SaxError::Syntax(msg, l)
    }

    fn starts_with(&self, pat: &str) -> bool {
        self.b[self.pos..].starts_with(pat.as_bytes())
    }

    fn skip_space(&mut self) {
        while self.pos < self.b.len() && is_xml_space(self.b[self.pos]) {
            self.pos += 1;
        }
    }

    /// Skip until `pat` (included). Returns false if not found.
    fn skip_past(&mut self, pat: &str) -> bool {
        match self.s[self.pos..].find(pat) {
            Some(p) => {
                self.pos += p + pat.len();
                true
            }
            None => false,
        }
    }

    fn parse_name(&mut self) -> std::result::Result<String, SaxError> {
        let start = self.pos;
        if self.pos >= self.b.len() {
            return Err(SaxError::Syntax(ERR_UNCLOSED_TOKEN, self.eof_line()));
        }
        if !is_name_start(self.b[self.pos]) {
            return Err(self.err_at(ERR_INVALID_TOKEN, start));
        }
        while self.pos < self.b.len() && is_name_char(self.b[self.pos]) {
            self.pos += 1;
        }
        Ok(self.s[start..self.pos].to_string())
    }

    /// Parse an entity or character reference starting at `&`. Returns the
    /// replacement text.
    fn parse_reference(&mut self) -> std::result::Result<String, SaxError> {
        let start = self.pos;
        self.pos += 1; // '&'
        let semi = match self.s[self.pos..].find(';') {
            Some(p) => self.pos + p,
            None => return Err(SaxError::Syntax(ERR_UNCLOSED_TOKEN, self.eof_line())),
        };
        let body = &self.s[self.pos..semi];
        self.pos = semi + 1;
        if let Some(num) = body.strip_prefix('#') {
            let code = if let Some(hex) = num.strip_prefix('x') {
                if hex.is_empty() || !hex.bytes().all(|c| c.is_ascii_hexdigit()) {
                    return Err(self.err_at(ERR_INVALID_TOKEN, start));
                }
                u32::from_str_radix(hex, 16).ok()
            } else {
                if num.is_empty() || !num.bytes().all(|c| c.is_ascii_digit()) {
                    return Err(self.err_at(ERR_INVALID_TOKEN, start));
                }
                num.parse::<u32>().ok()
            };
            let ch = code.and_then(char::from_u32).filter(|&c| {
                let c = c as u32;
                c == 0x9
                    || c == 0xA
                    || c == 0xD
                    || (0x20..=0xD7FF).contains(&c)
                    || (0xE000..=0xFFFD).contains(&c)
                    || c >= 0x10000
            });
            return match ch {
                Some(c) => Ok(c.to_string()),
                None => Err(self.err_at(ERR_BAD_CHAR_REF, start)),
            };
        }
        if body.is_empty() || !body.bytes().all(is_name_char) || !is_name_start(body.as_bytes()[0])
        {
            return Err(self.err_at(ERR_INVALID_TOKEN, start));
        }
        match body {
            "lt" => Ok("<".to_string()),
            "gt" => Ok(">".to_string()),
            "amp" => Ok("&".to_string()),
            "quot" => Ok("\"".to_string()),
            "apos" => Ok("'".to_string()),
            _ => Err(self.err_at(ERR_UNDEFINED_ENTITY, start)),
        }
    }

    /// Parse the attributes of a start tag and the end of the tag. Returns
    /// the attributes and true for an empty element tag (`/>`).
    fn parse_attributes(&mut self) -> std::result::Result<(Vec<(String, String)>, bool), SaxError> {
        let mut atts: Vec<(String, String)> = Vec::new();
        loop {
            let had_space = self.pos < self.b.len() && is_xml_space(self.b[self.pos]);
            self.skip_space();
            if self.pos >= self.b.len() {
                return Err(SaxError::Syntax(ERR_UNCLOSED_TOKEN, self.eof_line()));
            }
            match self.b[self.pos] {
                b'>' => {
                    self.pos += 1;
                    return Ok((atts, false));
                }
                b'/' => {
                    if self.pos + 1 >= self.b.len() {
                        return Err(SaxError::Syntax(ERR_UNCLOSED_TOKEN, self.eof_line()));
                    }
                    if self.b[self.pos + 1] != b'>' {
                        return Err(self.err_at(ERR_INVALID_TOKEN, self.pos));
                    }
                    self.pos += 2;
                    return Ok((atts, true));
                }
                _ => {}
            }
            if !had_space {
                return Err(self.err_at(ERR_INVALID_TOKEN, self.pos));
            }
            let name_pos = self.pos;
            let name = self.parse_name()?;
            self.skip_space();
            if self.pos >= self.b.len() {
                return Err(SaxError::Syntax(ERR_UNCLOSED_TOKEN, self.eof_line()));
            }
            if self.b[self.pos] != b'=' {
                return Err(self.err_at(ERR_INVALID_TOKEN, self.pos));
            }
            self.pos += 1;
            self.skip_space();
            if self.pos >= self.b.len() {
                return Err(SaxError::Syntax(ERR_UNCLOSED_TOKEN, self.eof_line()));
            }
            let quote = self.b[self.pos];
            if quote != b'"' && quote != b'\'' {
                return Err(self.err_at(ERR_INVALID_TOKEN, self.pos));
            }
            self.pos += 1;
            let mut value = String::new();
            loop {
                if self.pos >= self.b.len() {
                    return Err(SaxError::Syntax(ERR_UNCLOSED_TOKEN, self.eof_line()));
                }
                let c = self.b[self.pos];
                if c == quote {
                    self.pos += 1;
                    break;
                }
                match c {
                    b'<' => return Err(self.err_at(ERR_INVALID_TOKEN, self.pos)),
                    b'&' => {
                        let r = self.parse_reference()?;
                        value.push_str(&r);
                    }
                    b'\r' => {
                        value.push(' ');
                        self.pos += 1;
                        if self.pos < self.b.len() && self.b[self.pos] == b'\n' {
                            self.pos += 1;
                        }
                    }
                    b'\n' | b'\t' => {
                        value.push(' ');
                        self.pos += 1;
                    }
                    _ => {
                        // Copy a run of plain characters.
                        let start = self.pos;
                        while self.pos < self.b.len() {
                            let d = self.b[self.pos];
                            if d == quote
                                || d == b'<'
                                || d == b'&'
                                || d == b'\r'
                                || d == b'\n'
                                || d == b'\t'
                            {
                                break;
                            }
                            self.pos += 1;
                        }
                        value.push_str(&self.s[start..self.pos]);
                    }
                }
            }
            if atts.iter().any(|(n, _)| *n == name) {
                return Err(self.err_at(ERR_DUPLICATE_ATTRIBUTE, name_pos));
            }
            atts.push((name, value));
        }
    }

    fn parse_start_tag<H: SaxHandler>(&mut self, h: &mut H) -> std::result::Result<(), SaxError> {
        self.pos += 1; // '<'
        let name = self.parse_name()?;
        let (atts, empty) = self.parse_attributes()?;
        let line = self.line_at(self.pos - 1);
        h.start_element(&name, &atts, line)?;
        if empty {
            h.end_element(&name, line)?;
        } else {
            self.open.push(name);
        }
        Ok(())
    }

    fn parse_end_tag<H: SaxHandler>(&mut self, h: &mut H) -> std::result::Result<(), SaxError> {
        self.pos += 2; // '</'
        let name = self.parse_name()?;
        self.skip_space();
        if self.pos >= self.b.len() {
            return Err(SaxError::Syntax(ERR_UNCLOSED_TOKEN, self.eof_line()));
        }
        if self.b[self.pos] != b'>' {
            return Err(self.err_at(ERR_INVALID_TOKEN, self.pos));
        }
        self.pos += 1;
        let line = self.line_at(self.pos - 1);
        match self.open.last() {
            Some(top) if *top == name => {}
            _ => return Err(SaxError::TagMismatch(line)),
        }
        self.open.pop();
        h.end_element(&name, line)?;
        Ok(())
    }

    /// Emit character data from `start` to `end`, split at line breaks.
    fn emit_text<H: SaxHandler>(
        &mut self,
        h: &mut H,
        start: usize,
        end: usize,
    ) -> std::result::Result<(), SaxError> {
        let mut i = start;
        while i < end {
            let c = self.b[i];
            if c == b'\n' {
                let l = self.line_at(i);
                h.character_data("\n", l)?;
                i += 1;
            } else if c == b'\r' {
                let mut j = i + 1;
                if j < end && self.b[j] == b'\n' {
                    j += 1;
                }
                let l = self.line_at(j - 1);
                h.character_data("\n", l)?;
                i = j;
            } else {
                let run_start = i;
                while i < end && self.b[i] != b'\n' && self.b[i] != b'\r' {
                    i += 1;
                }
                let l = self.line_at(i - 1);
                h.character_data(&self.s[run_start..i], l)?;
            }
        }
        Ok(())
    }

    /// Skip a comment, a processing instruction or a DOCTYPE declaration at
    /// the current position. Returns false if nothing was skipped.
    fn skip_misc(&mut self) -> std::result::Result<bool, SaxError> {
        if self.starts_with("<!--") {
            self.pos += 4;
            if !self.skip_past("-->") {
                return Err(SaxError::Syntax(ERR_UNCLOSED_TOKEN, self.eof_line()));
            }
            return Ok(true);
        }
        if self.starts_with("<?") {
            let start = self.pos;
            self.pos += 2;
            let target_start = self.pos;
            while self.pos < self.b.len() && is_name_char(self.b[self.pos]) {
                self.pos += 1;
            }
            let target = &self.s[target_start..self.pos];
            if target.eq_ignore_ascii_case("xml") && start != 0 {
                return Err(self.err_at(ERR_MISPLACED_XML_PI, start));
            }
            if !self.skip_past("?>") {
                return Err(SaxError::Syntax(ERR_UNCLOSED_TOKEN, self.eof_line()));
            }
            return Ok(true);
        }
        Ok(false)
    }

    fn skip_doctype(&mut self) -> std::result::Result<(), SaxError> {
        // Skip "<!DOCTYPE ... [ ... ] >".
        self.pos += 9;
        let mut depth = 0i32;
        let mut quote: Option<u8> = None;
        while self.pos < self.b.len() {
            let c = self.b[self.pos];
            self.pos += 1;
            if let Some(q) = quote {
                if c == q {
                    quote = None;
                }
                continue;
            }
            match c {
                b'"' | b'\'' => quote = Some(c),
                b'[' => depth += 1,
                b']' => depth -= 1,
                b'>' if depth <= 0 => return Ok(()),
                _ => {}
            }
        }
        Err(SaxError::Syntax(ERR_UNCLOSED_TOKEN, self.eof_line()))
    }

    fn run<H: SaxHandler>(&mut self, h: &mut H) -> std::result::Result<(), SaxError> {
        // Prolog.
        loop {
            self.skip_space();
            if self.pos >= self.b.len() {
                return Err(SaxError::Syntax(ERR_NO_ELEMENTS, self.eof_line()));
            }
            if self.skip_misc()? {
                continue;
            }
            if self.starts_with("<!DOCTYPE") {
                self.skip_doctype()?;
                continue;
            }
            if self.b[self.pos] == b'<' {
                if self.pos + 1 < self.b.len() && is_name_start(self.b[self.pos + 1]) {
                    self.parse_start_tag(h)?;
                    break;
                }
                if self.pos + 1 >= self.b.len() {
                    return Err(SaxError::Syntax(ERR_UNCLOSED_TOKEN, self.eof_line()));
                }
                return Err(self.err_at(ERR_INVALID_TOKEN, self.pos));
            }
            return Err(self.err_at(ERR_SYNTAX, self.pos));
        }

        // Content.
        while !self.open.is_empty() {
            if self.pos >= self.b.len() {
                return Err(SaxError::Syntax(ERR_NO_ELEMENTS, self.eof_line()));
            }
            let c = self.b[self.pos];
            if c == b'<' {
                if self.starts_with("</") {
                    self.parse_end_tag(h)?;
                } else if self.starts_with("<![CDATA[") {
                    self.pos += 9;
                    let start = self.pos;
                    match self.s[self.pos..].find("]]>") {
                        Some(p) => {
                            let end = start + p;
                            self.pos = end + 3;
                            self.emit_text(h, start, end)?;
                        }
                        None => return Err(SaxError::Syntax(ERR_UNCLOSED_CDATA, self.eof_line())),
                    }
                } else if self.skip_misc()? {
                    // Comment or processing instruction.
                } else if self.pos + 1 < self.b.len() && is_name_start(self.b[self.pos + 1]) {
                    self.parse_start_tag(h)?;
                } else if self.pos + 1 >= self.b.len() {
                    return Err(SaxError::Syntax(ERR_UNCLOSED_TOKEN, self.eof_line()));
                } else {
                    return Err(self.err_at(ERR_INVALID_TOKEN, self.pos));
                }
            } else if c == b'&' {
                let start = self.pos;
                let r = self.parse_reference()?;
                let l = self.line_at(self.pos - 1);
                let _ = start;
                h.character_data(&r, l)?;
            } else {
                let start = self.pos;
                while self.pos < self.b.len() {
                    let d = self.b[self.pos];
                    if d == b'<' || d == b'&' {
                        break;
                    }
                    if d == b']' && self.starts_with("]]>") {
                        return Err(self.err_at(ERR_INVALID_TOKEN, self.pos));
                    }
                    if d < 0x20 && d != b'\t' && d != b'\n' && d != b'\r' {
                        return Err(self.err_at(ERR_INVALID_TOKEN, self.pos));
                    }
                    self.pos += 1;
                }
                let end = self.pos;
                self.emit_text(h, start, end)?;
            }
        }

        // Epilog.
        loop {
            self.skip_space();
            if self.pos >= self.b.len() {
                return Ok(());
            }
            if self.skip_misc()? {
                continue;
            }
            return Err(self.err_at(ERR_JUNK, self.pos));
        }
    }
}

/// Parse an XML document and call the handler callbacks.
pub(crate) fn parse_xml<H: SaxHandler>(
    data: &[u8],
    h: &mut H,
) -> std::result::Result<(), SaxError> {
    let text = decode(data)?;
    let sched = FeedSchedule::new(text.as_bytes());
    let mut p = SaxParser {
        s: &text,
        b: text.as_bytes(),
        pos: 0,
        sched,
        open: Vec::new(),
    };
    p.run(h)
}

// ---------------------------------------------------------------------------
// Number formatting

/// Format like C `printf("%.*g", precision, v)`, which is also what a C++
/// `std::ostream` does by default (the precision of 0 is treated as 1).
pub(crate) fn fmt_g(v: f64, precision: usize) -> String {
    if v.is_nan() {
        return if v.is_sign_negative() {
            "-nan".to_string()
        } else {
            "nan".to_string()
        };
    }
    if v.is_infinite() {
        return if v < 0.0 {
            "-inf".to_string()
        } else {
            "inf".to_string()
        };
    }
    let p = precision.max(1);
    if v == 0.0 {
        return if v.is_sign_negative() {
            "-0".to_string()
        } else {
            "0".to_string()
        };
    }
    let sci = format!("{:.*e}", p - 1, v);
    let (mant, exp) = match sci.split_once('e') {
        Some((m, e)) => (m.to_string(), e.parse::<i32>().unwrap_or(0)),
        None => (sci.clone(), 0),
    };
    let p_i = i32::try_from(p).unwrap_or(i32::MAX);
    if exp < -4 || exp >= p_i {
        let mant = strip_trailing_zeros(&mant);
        let sign = if exp < 0 { '-' } else { '+' };
        let a = exp.unsigned_abs();
        if a < 10 {
            format!("{mant}e{sign}0{a}")
        } else {
            format!("{mant}e{sign}{a}")
        }
    } else {
        let decimals = usize::try_from(p_i - 1 - exp).unwrap_or(0);
        let fixed = format!("{:.*}", decimals, v);
        strip_trailing_zeros(&fixed)
    }
}

fn strip_trailing_zeros(s: &str) -> String {
    if s.contains('.') {
        let t = s.trim_end_matches('0');
        let t = t.trim_end_matches('.');
        t.to_string()
    } else {
        s.to_string()
    }
}

/// Default precision of C++ streams.
pub(crate) const DEFAULT_PRECISION: usize = 6;

/// Format a value like OCIO's `WriteValue` (`nan`, `inf` and `-inf` are
/// written as such, other values with `fmt_g`).
pub(crate) fn write_value(v: f64, precision: usize) -> String {
    if v.is_nan() {
        "nan".to_string()
    } else if v == f64::INFINITY {
        "inf".to_string()
    } else if v == f64::NEG_INFINITY {
        "-inf".to_string()
    } else {
        fmt_g(v, precision)
    }
}

/// Left pad a string with spaces to `width` characters (C++ `std::setw`).
pub(crate) fn pad_left(s: &str, width: usize) -> String {
    let n = s.chars().count();
    if n >= width {
        s.to_string()
    } else {
        let mut r = " ".repeat(width - n);
        r.push_str(s);
        r
    }
}

// ---------------------------------------------------------------------------
// Writer

/// Attributes of an element to write.
pub(crate) type Attributes = Vec<(String, String)>;

/// Writes indented XML (port of `XmlFormatter`).
#[derive(Debug, Default)]
pub(crate) struct XmlFormatter {
    out: String,
    indent: usize,
}

impl XmlFormatter {
    /// New formatter writing into an empty string.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// The written text.
    pub(crate) fn into_string(self) -> String {
        self.out
    }

    /// Direct access to the output (like `getStream()`).
    pub(crate) fn stream(&mut self) -> &mut String {
        &mut self.out
    }

    pub(crate) fn increment_indent(&mut self) {
        self.indent += 1;
    }

    pub(crate) fn decrement_indent(&mut self) {
        self.indent = self.indent.saturating_sub(1);
    }

    fn write_indent(&mut self) {
        for _ in 0..self.indent {
            self.out.push_str("    ");
        }
    }

    fn write_attributes(&mut self, attributes: &[(String, String)]) {
        for (n, v) in attributes {
            self.out.push(' ');
            self.out.push_str(n);
            self.out.push_str("=\"");
            self.out.push_str(&escape_xml(v));
            self.out.push('"');
        }
    }

    /// Write a start element on a standalone line.
    pub(crate) fn write_start_tag(&mut self, tag: &str, attributes: &[(String, String)]) {
        self.write_indent();
        self.out.push('<');
        self.out.push_str(tag);
        self.write_attributes(attributes);
        self.out.push_str(">\n");
    }

    /// Write an end element on a standalone line.
    pub(crate) fn write_end_tag(&mut self, tag: &str) {
        self.write_indent();
        self.out.push_str("</");
        self.out.push_str(tag);
        self.out.push_str(">\n");
    }

    /// Write `<tag attributes>content</tag>` on a standalone line.
    pub(crate) fn write_content_tag(
        &mut self,
        tag: &str,
        attributes: &[(String, String)],
        content: &str,
    ) {
        self.write_indent();
        self.out.push('<');
        self.out.push_str(tag);
        self.write_attributes(attributes);
        self.out.push('>');
        self.out.push_str(&escape_xml(content));
        self.out.push_str("</");
        self.out.push_str(tag);
        self.out.push_str(">\n");
    }

    /// Write the content (escaped) on a standalone line.
    pub(crate) fn write_content(&mut self, content: &str) {
        self.write_indent();
        self.out.push_str(&escape_xml(content));
        self.out.push('\n');
    }

    /// Write an empty element (`<tag attributes />`) on a standalone line.
    pub(crate) fn write_empty_tag(&mut self, tag: &str, attributes: &[(String, String)]) {
        self.write_indent();
        self.out.push('<');
        self.out.push_str(tag);
        self.write_attributes(attributes);
        self.out.push_str(" />\n");
    }
}

/// Build an attribute pair.
pub(crate) fn attr(name: &str, value: &str) -> (String, String) {
    (name.to_string(), value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pn(s: &str, start: usize, end: usize) -> Result<f32> {
        parse_number::<f32>(s.as_bytes(), start, end)
    }

    #[test]
    fn string_to_float() {
        let s = "12345";
        assert_eq!(pn(s, 0, s.len()).unwrap(), 12345.0);
    }

    #[test]
    fn string_to_float_failure() {
        let s = "ABDSCSGFDS";
        let e = pn(s, 0, s.len()).unwrap_err();
        assert!(e.message().contains("can not be parsed"));

        let s1 = "10 ";
        let e = pn(s1, 0, s1.len()).unwrap_err();
        assert!(e.message().contains("followed by unexpected characters"));
        assert!(pn(s1, 0, 2).is_ok());

        let s2 = "12345";
        assert_eq!(pn(s2, 0, s2.len() - 2).unwrap(), 123.0);

        let s3 = "123XX";
        assert!(pn(s3, 0, s3.len() - 2).is_ok());
    }

    #[test]
    fn get_numbers_test() {
        let s = "  1.0 , 2.0     3.0,4";
        let v: Vec<f32> = get_numbers(s).unwrap();
        assert_eq!(v, vec![1.0, 2.0, 3.0, 4.0]);

        let s1 = "inf, -infinity 1.0, -2.0 0x42 nan  , -nan 5.0";
        let v: Vec<f32> = get_numbers(s1).unwrap();
        assert_eq!(v.len(), 8);
        assert!(v[0].is_infinite());
        assert!(v[1].is_infinite());
        assert_eq!(v[2], 1.0);
        assert_eq!(v[3], -2.0);
        assert_eq!(v[4], 66.0);
        assert!(v[5].is_nan());
        assert!(v[6].is_nan());
        assert_eq!(v[7], 5.0);

        let s2 = ",  ,, , 0 2.0 \n \t 3.0 0.1e+1";
        let v: Vec<f32> = get_numbers(s2).unwrap();
        assert_eq!(v, vec![0.0, 2.0, 3.0, 1.0]);

        let s3 = "  0   error 2.0 3.0";
        let e = get_numbers::<f32>(s3).unwrap_err();
        assert!(e.message().contains("can not be parsed"));

        let s4 = "0   1.0error 2.0 3.0";
        let e = get_numbers::<f32>(s4).unwrap_err();
        assert!(e.message().contains("followed by unexpected characters"));
    }

    #[test]
    fn trim_test() {
        let o1 = "    some text    ";
        let o2 = " \n \r some text  \t \u{b} \u{c} ";
        assert_eq!(trim(o1), "some text");
        assert_eq!(trim(o2), "some text");
        assert_eq!(rtrim(o1), "    some text");
        assert_eq!(rtrim(o2), " \n \r some text");
        assert_eq!(ltrim(o1), "some text    ");
        assert_eq!(ltrim(o2), "some text  \t \u{b} \u{c} ");
    }

    #[test]
    fn parse_number_test() {
        let check = |s: &str, start: usize, end: usize, expected: f32| {
            assert_eq!(pn(s, start, end).unwrap(), expected, "{s}");
        };
        check("1 0", 0, 1, 1.0);
        check(" 1 0", 0, 2, 1.0);
        check("1.0 0", 0, 3, 1.0);
        check("1.0000 0", 0, 6, 1.0);
        check("1.0", 0, 3, 1.0);
        check("1", 0, 1, 1.0);
        check("10.0e-1", 0, 7, 1.0);
        check("0.1e+1", 0, 6, 1.0);
        check("-1 0", 0, 2, -1.0);
        check("-1.0 0", 0, 4, -1.0);
        let b = "-1.0000 0";
        let end = find_delim(b.as_bytes(), 0);
        check(b, 0, end, -1.0);
        check("  -1.0", 0, 6, -1.0);
        check("   -1", 0, 5, -1.0);
        check(" -10.0e-1", 0, 9, -1.0);
        check("-0.1e+1", 0, 7, -1.0);
        check("INF", 0, 3, f32::INFINITY);

        let b = "INF 1.0 2.0";
        let bb = b.as_bytes();
        let mut pos = 0;
        let mut next = find_delim(bb, pos);
        assert_eq!(next, 3);
        check(b, pos, next, f32::INFINITY);
        pos = find_next_token_start(bb, next);
        assert_eq!(pos, 4);
        next = find_delim(bb, pos);
        assert_eq!(next, 7);
        check(b, pos, next, 1.0);
        pos = find_next_token_start(bb, next);
        assert_eq!(pos, 8);
        next = find_delim(bb, pos);
        assert_eq!(next, 11);
        check(b, pos, next, 2.0);

        check("INFINITY", 0, 8, f32::INFINITY);
        check("-INF", 0, 4, f32::NEG_INFINITY);
        check("-INFINITY", 0, 9, f32::NEG_INFINITY);
        assert!(pn("NAN", 0, 3).unwrap().is_nan());
        assert!(pn("-NAN", 0, 4).unwrap().is_nan());
        check("0.001", 0, 5, 0.001);
        check("-0.001", 0, 6, -0.001);
        check(".001", 0, 4, 0.001);
        check("-.001", 0, 5, -0.001);
        check(".01e-1", 0, 6, 0.001);
        check("-.01e-1", 0, 7, -0.001);
        check("-.01e-1,", 0, 7, -0.001);
        check("-.01e-1\n", 0, 7, -0.001);
        check("-.01e-1\t", 0, 7, -0.001);
        check("10E-1", 0, 5, 1.0);
        check("0.10E01", 0, 7, 1.0);

        let e = pn("XY", 0, 2).unwrap_err();
        assert!(e.message().contains("can not be parsed"));
    }

    #[test]
    fn find_sub_string_test() {
        assert_eq!(find_sub_string(b"   new order   "), (3, 12));
        assert_eq!(find_sub_string(b"new order   "), (0, 9));
        assert_eq!(find_sub_string(b"   new order"), (3, 12));
        assert_eq!(find_sub_string(b"new order"), (0, 9));
        assert_eq!(find_sub_string(b""), (0, 0));
        assert_eq!(find_sub_string(b"      "), (0, 0));
        assert_eq!(find_sub_string(b"   \t123    "), (4, 7));
        assert_eq!(find_sub_string(b"1   \t \n \r"), (0, 1));
        assert_eq!(find_sub_string(b"\t"), (0, 0));
    }

    #[test]
    fn integer_parsing() {
        let v: Vec<u32> = get_numbers("3 3 3").unwrap();
        assert_eq!(v, vec![3, 3, 3]);
        let e = get_numbers::<u32>("3.5 3").unwrap_err();
        assert!(e.message().contains("are illegal in"));
    }

    #[test]
    fn format_g() {
        assert_eq!(fmt_g(0.0, 6), "0");
        assert_eq!(fmt_g(1.0, 6), "1");
        assert_eq!(fmt_g(0.1, 15), "0.1");
        assert_eq!(fmt_g(1e-5, 6), "1e-05");
        assert_eq!(fmt_g(123456789.0, 6), "1.23457e+08");
        assert_eq!(fmt_g(1023.5, 6), "1023.5");
        assert_eq!(fmt_g(0.4123908, 8), "0.4123908");
        assert_eq!(fmt_g(2.3456789123456, 15), "2.3456789123456");
        assert_eq!(fmt_g(f64::from(0.1f32), 8), "0.1");
        assert_eq!(fmt_g(100000.0, 6), "100000");
        assert_eq!(fmt_g(1000000.0, 6), "1e+06");
        assert_eq!(fmt_g(-0.03, 16), "-0.03");
        assert_eq!(write_value(f64::NAN, 8), "nan");
        assert_eq!(write_value(f64::NEG_INFINITY, 8), "-inf");
    }

    #[derive(Default)]
    struct Recorder {
        events: Vec<String>,
    }

    impl SaxHandler for Recorder {
        fn start_element(
            &mut self,
            name: &str,
            atts: &[(String, String)],
            line: u32,
        ) -> Result<()> {
            let a: Vec<String> = atts.iter().map(|(n, v)| format!("{n}={v}")).collect();
            self.events.push(format!("S{line}:{name}[{}]", a.join(",")));
            Ok(())
        }
        fn end_element(&mut self, name: &str, line: u32) -> Result<()> {
            self.events.push(format!("E{line}:{name}"));
            Ok(())
        }
        fn character_data(&mut self, s: &str, line: u32) -> Result<()> {
            self.events.push(format!("C{line}:{s:?}"));
            Ok(())
        }
    }

    #[test]
    fn sax_events() {
        let xml = "<?xml version=\"1.0\"?>\n<!-- c -->\n<a x='1' y=\"a&amp;b\nc\">text &lt; more\r\n<b/></a>\n";
        let mut r = Recorder::default();
        parse_xml(xml.as_bytes(), &mut r).unwrap();
        assert_eq!(
            r.events,
            vec![
                "S5:a[x=1,y=a&b c]",
                "C5:\"text \"",
                "C5:\"<\"",
                "C5:\" more\"",
                "C5:\"\\n\"",
                "S5:b[]",
                "E5:b",
                "E5:a",
            ]
        );
    }

    #[test]
    fn sax_errors() {
        let mut r = Recorder::default();
        match parse_xml(b"<a><b></a>", &mut r) {
            Err(SaxError::TagMismatch(1)) => {}
            other => panic!("{other:?}"),
        }
        let mut r = Recorder::default();
        match parse_xml(b"<a>\n<b></b>\n", &mut r) {
            Err(SaxError::Syntax(m, 3)) => assert_eq!(m, "no element found"),
            other => panic!("{other:?}"),
        }
        let mut r = Recorder::default();
        match parse_xml(b"<a></a><b/>", &mut r) {
            Err(SaxError::Syntax(m, _)) => assert_eq!(m, "junk after document element"),
            other => panic!("{other:?}"),
        }
        let mut r = Recorder::default();
        match parse_xml(b"<a x='1' x='2'/>", &mut r) {
            Err(SaxError::Syntax(m, _)) => assert_eq!(m, "duplicate attribute"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn formatter() {
        let mut f = XmlFormatter::new();
        f.write_start_tag("A", &[attr("x", "a\"b")]);
        f.increment_indent();
        f.write_content_tag("B", &[], "<>");
        f.write_empty_tag("C", &[attr("y", "1")]);
        f.decrement_indent();
        f.write_end_tag("A");
        assert_eq!(
            f.into_string(),
            "<A x=\"a&quot;b\">\n    <B>&lt;&gt;</B>\n    <C y=\"1\" />\n</A>\n"
        );
    }
}
