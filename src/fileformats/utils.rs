//! Helpers shared by the file format readers (port of `FileFormatUtils.cpp`
//! and of the parsing helpers the readers use from `ParseUtils.cpp`,
//! `NumberUtils.h`, `StringUtils.h`, `BitDepthUtils.cpp` and `LutLimits.h`).
//!
//! The text readers are ported from C++ code relying on `std::istream`
//! and `sscanf` semantics; [`IStream`] and [`scanf`] reproduce the parts of
//! these semantics the readers depend on (stream state flags, line
//! splitting, conversion counting, ...).

use crate::error::Result;
use crate::transforms::{Lut1DTransform, Lut3DTransform, MatrixTransform};
use crate::types::{BitDepth, Interpolation};

/// Maximum length of a 1D LUT read from a file (`Max1DLUTLength`).
pub const MAX_1D_LUT_LENGTH: usize = 300_000;
/// Maximum grid size of a 3D LUT read from a file (`Max3DLUTLength`).
pub const MAX_3D_LUT_LENGTH: usize = 129;

// ---------------------------------------------------------------------------
// Interpolation handling.

/// True if `interp` can be used by a 1D LUT (`Lut1DOpData::IsValidInterpolation`).
pub fn is_valid_lut1d_interpolation(interp: Interpolation) -> bool {
    matches!(
        interp,
        Interpolation::Best
            | Interpolation::Default
            | Interpolation::Linear
            | Interpolation::Nearest
    )
}

/// True if `interp` can be used by a 3D LUT (`Lut3DOpData::IsValidInterpolation`).
pub fn is_valid_lut3d_interpolation(interp: Interpolation) -> bool {
    matches!(
        interp,
        Interpolation::Best
            | Interpolation::Tetrahedral
            | Interpolation::Default
            | Interpolation::Linear
            | Interpolation::Nearest
    )
}

/// The interpolation actually used by a 1D LUT (`Lut1DOpData::GetConcreteInterpolation`).
pub fn lut1d_concrete_interpolation(_interp: Interpolation) -> Interpolation {
    Interpolation::Linear
}

/// The interpolation actually used by a 3D LUT (`Lut3DOpData::GetConcreteInterpolation`).
pub fn lut3d_concrete_interpolation(interp: Interpolation) -> Interpolation {
    match interp {
        Interpolation::Best | Interpolation::Tetrahedral => Interpolation::Tetrahedral,
        _ => Interpolation::Linear,
    }
}

/// Port of `HandleLUT1D`: apply the `FileTransform` interpolation `file_interp`
/// to a 1D LUT read from a file. Returns true if `file_interp` is valid for
/// the LUT (i.e. "used").
pub fn handle_lut1d(lut: &mut Lut1DTransform, file_interp: Interpolation) -> bool {
    let valid = is_valid_lut1d_interpolation(file_interp);
    let interp = if valid {
        file_interp
    } else {
        Interpolation::Default
    };
    if lut1d_concrete_interpolation(lut.interpolation) != lut1d_concrete_interpolation(interp) {
        lut.interpolation = interp;
    }
    valid
}

/// Port of `HandleLUT3D`: apply the `FileTransform` interpolation `file_interp`
/// to a 3D LUT read from a file. Returns true if `file_interp` is valid for
/// the LUT (i.e. "used").
pub fn handle_lut3d(lut: &mut Lut3DTransform, file_interp: Interpolation) -> bool {
    let valid = is_valid_lut3d_interpolation(file_interp);
    let interp = if valid {
        file_interp
    } else {
        Interpolation::Default
    };
    if lut3d_concrete_interpolation(lut.interpolation) != lut3d_concrete_interpolation(interp) {
        lut.interpolation = interp;
    }
    valid
}

/// Create a 1D LUT of `length` entries as the readers do: the interpolation
/// is set to `interp` when valid for a 1D LUT and the file output bit depth
/// is set to `file_bd`.
pub fn new_lut1d(
    length: usize,
    half_domain: bool,
    interp: Interpolation,
    file_bd: BitDepth,
) -> Lut1DTransform {
    let mut lut = Lut1DTransform::new(length, half_domain);
    if is_valid_lut1d_interpolation(interp) {
        lut.interpolation = interp;
    }
    lut.file_output_bit_depth = file_bd;
    lut
}

/// Create a 3D LUT of the given grid size as the readers do: the
/// interpolation is set to `interp` when valid for a 3D LUT and the file
/// output bit depth is set to `file_bd`.
pub fn new_lut3d(grid_size: usize, interp: Interpolation, file_bd: BitDepth) -> Lut3DTransform {
    let mut lut = Lut3DTransform::new(grid_size);
    if is_valid_lut3d_interpolation(interp) {
        lut.interpolation = interp;
    }
    lut.file_output_bit_depth = file_bd;
    lut
}

/// Fill a 3D LUT from values stored with the red index changing fastest
/// (port of `Lut3DOpData::setArrayFromRedFastestOrder`).
pub fn set_lut3d_from_red_fastest(lut: &mut Lut3DTransform, raw: &[f32]) -> Result<()> {
    let n = lut.grid_size;
    if raw.len() != n * n * n * 3 {
        crate::bail!(
            "Lut3DOpData length does not match the vector size: {} != {}.",
            n * n * n * 3,
            raw.len()
        );
    }
    for b in 0..n {
        for g in 0..n {
            for r in 0..n {
                // Index into the red-fastest ordered source.
                let src = 3 * ((b * n + g) * n + r);
                // Index into the blue-fastest ordered destination.
                let dst = 3 * ((r * n + g) * n + b);
                lut.values[dst..dst + 3].copy_from_slice(&raw[src..src + 3]);
            }
        }
    }
    Ok(())
}

/// Edge length of a cube holding `num_pixels` entries (port of
/// `Get3DLutEdgeLenFromNumPixels`).
pub fn get_3d_lut_edge_len_from_num_pixels(num_pixels: i64) -> Result<usize> {
    let dim = (num_pixels as f32).powf(1.0 / 3.0).round() as i64;
    if dim.checked_mul(dim).and_then(|d| d.checked_mul(dim)) != Some(num_pixels) {
        crate::bail!(
            "Cannot infer 3D LUT size. {} element(s) does not correspond to a unform cube edge length. (nearest edge length is {}).",
            num_pixels,
            dim
        );
    }
    Ok(dim.max(0) as usize)
}

/// Blue-fastest index of a 3D LUT entry (port of `GetLut3DIndex_BlueFast`).
pub fn lut3d_index_blue_fast(r: usize, g: usize, b: usize, size_g: usize, size_b: usize) -> usize {
    3 * (b + size_b * (g + size_g * r))
}

// ---------------------------------------------------------------------------
// Matrices.

/// Port of `CreateMinMaxOp`: the matrix mapping `[from_min, from_max]` to
/// `[0, 1]` per channel, or `None` if it is an identity.
pub fn min_max_matrix(from_min: [f64; 3], from_max: [f64; 3]) -> Result<Option<MatrixTransform>> {
    let mut scale4 = [1.0f64; 4];
    let mut offset4 = [0.0f64; 4];
    let mut something_to_do = false;
    for i in 0..3 {
        let range = from_max[i] - from_min[i];
        if range == 0.0 {
            crate::bail!("CreateMinMaxOp: from_min and from_max must not be equal.");
        }
        scale4[i] = 1.0 / range;
        offset4[i] = -from_min[i] * scale4[i];
        something_to_do |= scale4[i] != 1.0 || offset4[i] != 0.0;
    }
    if !something_to_do {
        return Ok(None);
    }
    let (m, _) = MatrixTransform::scale(&scale4);
    Ok(Some(MatrixTransform::new(m, offset4)))
}

/// [`min_max_matrix`] for a single min / max used for all channels.
pub fn min_max_matrix_f32(from_min: f32, from_max: f32) -> Result<Option<MatrixTransform>> {
    let mn = from_min as f64;
    let mx = from_max as f64;
    min_max_matrix([mn; 3], [mx; 3])
}

// ---------------------------------------------------------------------------
// Bit depths.

/// Port of `GetBitDepthMaxValue` (fails for unsupported depths).
pub fn bit_depth_max_value(bd: BitDepth) -> Result<f64> {
    match bd {
        BitDepth::UInt8 => Ok(255.0),
        BitDepth::UInt10 => Ok(1023.0),
        BitDepth::UInt12 => Ok(4095.0),
        BitDepth::UInt16 => Ok(65535.0),
        BitDepth::F16 | BitDepth::F32 => Ok(1.0),
        _ => crate::bail!("Bit depth is not supported: {}.", bd.as_str()),
    }
}

/// Port of `GetBitdepthFromMaxValue`.
pub fn bitdepth_from_max_value(max_value: u32) -> BitDepth {
    if max_value < 128 {
        BitDepth::F32
    } else if max_value < 639 {
        BitDepth::UInt8
    } else if max_value < 2559 {
        BitDepth::UInt10
    } else if max_value < 34815 {
        BitDepth::UInt12
    } else {
        BitDepth::UInt16
    }
}

// ---------------------------------------------------------------------------
// Number parsing.

/// `StringUtils::IsSpace`: every character up to (and including) ' '.
pub fn is_space(c: u8) -> bool {
    c <= b' '
}

/// C `isspace` in the "C" locale.
fn is_c_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
}

/// Length of the longest prefix of `s` that is a valid `strtod` number
/// (decimal, hexadecimal, `inf`, `infinity`, `nan`, `nan(...)`), sign
/// included. Returns 0 if there is none.
fn float_prefix_len(s: &[u8]) -> usize {
    let mut i = 0;
    if i < s.len() && (s[i] == b'+' || s[i] == b'-') {
        i += 1;
    }
    let rest = &s[i..];
    let lower: Vec<u8> = rest
        .iter()
        .take(8)
        .map(|c| c.to_ascii_lowercase())
        .collect();
    if lower.starts_with(b"infinity") {
        return i + 8;
    }
    if lower.starts_with(b"inf") {
        return i + 3;
    }
    if lower.starts_with(b"nan") {
        let mut j = i + 3;
        if j < s.len() && s[j] == b'(' {
            let mut k = j + 1;
            while k < s.len() && (s[k].is_ascii_alphanumeric() || s[k] == b'_') {
                k += 1;
            }
            if k < s.len() && s[k] == b')' {
                j = k + 1;
            }
        }
        return j;
    }
    // Hexadecimal.
    if rest.len() > 2 && rest[0] == b'0' && (rest[1] == b'x' || rest[1] == b'X') {
        let mut j = i + 2;
        let mut digits = 0;
        while j < s.len() && s[j].is_ascii_hexdigit() {
            j += 1;
            digits += 1;
        }
        if j < s.len() && s[j] == b'.' {
            j += 1;
            while j < s.len() && s[j].is_ascii_hexdigit() {
                j += 1;
                digits += 1;
            }
        }
        if digits == 0 {
            // Only the leading "0" is a number.
            return i + 1;
        }
        if j < s.len() && (s[j] == b'p' || s[j] == b'P') {
            let mut k = j + 1;
            if k < s.len() && (s[k] == b'+' || s[k] == b'-') {
                k += 1;
            }
            let start = k;
            while k < s.len() && s[k].is_ascii_digit() {
                k += 1;
            }
            if k > start {
                j = k;
            }
        }
        return j;
    }
    let mut j = i;
    let mut digits = 0;
    while j < s.len() && s[j].is_ascii_digit() {
        j += 1;
        digits += 1;
    }
    if j < s.len() && s[j] == b'.' {
        j += 1;
        while j < s.len() && s[j].is_ascii_digit() {
            j += 1;
            digits += 1;
        }
    }
    if digits == 0 {
        return 0;
    }
    if j < s.len() && (s[j] == b'e' || s[j] == b'E') {
        let mut k = j + 1;
        if k < s.len() && (s[k] == b'+' || s[k] == b'-') {
            k += 1;
        }
        let start = k;
        while k < s.len() && s[k].is_ascii_digit() {
            k += 1;
        }
        if k > start {
            j = k;
        }
    }
    j
}

/// Parse a hexadecimal float literal (without sign), e.g. `0x1.8p3`.
fn parse_hex_float(s: &str) -> Option<f64> {
    let body = &s[2..];
    let (mant, exp) = match body.find(['p', 'P']) {
        Some(p) => (&body[..p], body[p + 1..].parse::<i32>().ok()?),
        None => (body, 0),
    };
    let mut value = 0.0f64;
    let mut frac_digits = 0i32;
    let mut after_dot = false;
    for c in mant.chars() {
        if c == '.' {
            after_dot = true;
            continue;
        }
        let d = c.to_digit(16)? as f64;
        value = value * 16.0 + d;
        if after_dot {
            frac_digits += 1;
        }
    }
    Some(value * 2f64.powi(exp - 4 * frac_digits))
}

/// Parse the number prefix `s` (as selected by [`float_prefix_len`]).
fn parse_float_text(s: &str) -> Option<f64> {
    let (neg, body) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    let lower = body.to_ascii_lowercase();
    let v = if lower.starts_with("nan") {
        f64::NAN
    } else if lower.starts_with("inf") {
        f64::INFINITY
    } else if lower.starts_with("0x") {
        parse_hex_float(&lower)?
    } else {
        body.parse::<f64>().ok()?
    };
    Some(if neg { -v } else { v })
}

/// True if the textual number has a non-zero mantissa digit.
fn has_nonzero_digit(s: &str) -> bool {
    let mant = match s.find(['e', 'E']) {
        Some(p) if !s.to_ascii_lowercase().contains("0x") => &s[..p],
        _ => s,
    };
    mant.bytes().any(|c| c.is_ascii_digit() && c != b'0')
}

/// Port of `NumberUtils::from_chars` for floats: leading white spaces and a
/// `+` sign are skipped, the longest numeric prefix is parsed (trailing
/// characters are ignored). Returns `None` if there is no number or if it
/// is out of the `f32` range.
pub fn from_chars_f32(s: &str) -> Option<f32> {
    strtof_prefix(skip_prefix(s)?).map(|(v, _)| v)
}

/// `from_chars_skip_prefix`: skip leading white spaces and one `+` sign.
/// Returns `None` if a second sign follows the `+`.
fn skip_prefix(s: &str) -> Option<&str> {
    let t = s.trim_start_matches(|c: char| c.is_ascii() && is_c_space(c as u8));
    match t.strip_prefix('+') {
        Some(r) if r.starts_with(['+', '-']) => None,
        Some(r) => Some(r),
        None => Some(t),
    }
}

/// Parse the leading `strtof` number of `s` (no white space skipping) and
/// return the value and the number of bytes consumed. Out of range values
/// (overflow or underflow to zero) fail as in OCIO.
fn strtof_prefix(s: &str) -> Option<(f32, usize)> {
    let len = float_prefix_len(s.as_bytes());
    if len == 0 {
        return None;
    }
    let text = &s[..len];
    let v64 = parse_float_text(text)?;
    let v = v64 as f32;
    let lower = text.to_ascii_lowercase();
    if v.is_infinite() && !lower.contains("inf") {
        return None;
    }
    if v == 0.0 && has_nonzero_digit(text) {
        return None;
    }
    Some((v, len))
}

/// Port of `NumberUtils::from_chars` for doubles.
pub fn from_chars_f64(s: &str) -> Option<f64> {
    let t = skip_prefix(s)?;
    let len = float_prefix_len(t.as_bytes());
    if len == 0 {
        return None;
    }
    let text = &t[..len];
    let v = parse_float_text(text)?;
    if v.is_infinite() && !text.to_ascii_lowercase().contains("inf") {
        return None;
    }
    Some(v)
}

/// Port of `NumberUtils::from_chars` for `long int` (decimal, or
/// hexadecimal with a `0x` prefix). Trailing characters are ignored.
pub fn from_chars_i64(s: &str) -> Option<i64> {
    let t = skip_prefix(s)?;
    let b = t.as_bytes();
    if b.len() > 2 && b[0] == b'0' && (b[1] == b'x' || b[1] == b'X') {
        let digits: String = t[2..]
            .chars()
            .take_while(|c| c.is_ascii_hexdigit())
            .collect();
        return i64::from_str_radix(&digits, 16).ok();
    }
    let mut end = 0;
    if end < b.len() && b[end] == b'-' {
        end += 1;
    }
    let start = end;
    while end < b.len() && b[end].is_ascii_digit() {
        end += 1;
    }
    if end == start {
        return None;
    }
    t[..end].parse::<i64>().ok()
}

/// Port of `StringToFloat`.
pub fn string_to_float(s: &str) -> Option<f32> {
    from_chars_f32(s)
}

/// Port of `StringToInt` (`istringstream >> int`): leading white spaces are
/// skipped, then an optionally signed integer is read. With
/// `fail_if_leftover_chars`, any remaining character is an error.
pub fn string_to_int(s: &str, fail_if_leftover_chars: bool) -> Option<i32> {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() && is_c_space(b[i]) {
        i += 1;
    }
    let start = i;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    let digits_start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if i == digits_start {
        return None;
    }
    let v = s[start..i].parse::<i32>().ok()?;
    if fail_if_leftover_chars && i < b.len() {
        return None;
    }
    Some(v)
}

/// Port of `StringVecToFloatVec`.
pub fn string_vec_to_float_vec(parts: &[String]) -> Option<Vec<f32>> {
    parts.iter().map(|p| from_chars_f32(p)).collect()
}

/// Port of `StringVecToIntVec` (strict parsing of each part).
pub fn string_vec_to_int_vec(parts: &[String]) -> Option<Vec<i32>> {
    parts.iter().map(|p| string_to_int(p, true)).collect()
}

/// Port of `StringUtils::SplitByWhiteSpaces`.
pub fn split_by_white_spaces(s: &str) -> Vec<String> {
    s.split(|c: char| c.is_ascii() && is_c_space(c as u8))
        .filter(|p| !p.is_empty())
        .map(|p| p.to_string())
        .collect()
}

/// Port of `StringUtils::Trim` (trims every character `<= ' '`).
pub fn trim(s: &str) -> &str {
    s.trim_matches(|c: char| c.is_ascii() && is_space(c as u8))
}

/// Port of `StringUtils::LeftTrim`.
pub fn left_trim(s: &str) -> &str {
    s.trim_start_matches(|c: char| c.is_ascii() && is_space(c as u8))
}

/// Port of `StringUtils::IsEmptyOrWhiteSpace`.
pub fn is_empty_or_white_space(s: &str) -> bool {
    s.bytes().all(is_space)
}

/// Port of `EqualWithRelError` for floats.
pub fn equal_with_rel_error(x1: f32, x2: f32, e: f32) -> bool {
    let diff = if x1 > x2 { x1 - x2 } else { x2 - x1 };
    diff <= e * if x1 > 0.0 { x1 } else { -x1 }
}

/// Port of `VecsEqualWithRelError` for floats.
pub fn vecs_equal_with_rel_error(v1: &[f32], v2: &[f32], e: f32) -> bool {
    v1.len() == v2.len()
        && v1
            .iter()
            .zip(v2)
            .all(|(a, b)| equal_with_rel_error(*a, *b, e))
}

/// Port of `lerpf`.
pub fn lerpf(a: f32, b: f32, z: f32) -> f32 {
    (b - a) * z + a
}

// ---------------------------------------------------------------------------
// sscanf.

/// A value converted by [`scanf`].
#[derive(Debug, Clone, PartialEq)]
pub enum ScanValue {
    /// `%d`.
    Int(i32),
    /// `%f`.
    Float(f32),
    /// `%s`.
    Str(String),
    /// `%c`.
    Char(char),
}

impl ScanValue {
    /// The integer value (0 for other kinds).
    pub fn as_int(&self) -> i32 {
        match self {
            ScanValue::Int(v) => *v,
            _ => 0,
        }
    }
    /// The string value (`""` for other kinds).
    pub fn as_str(&self) -> &str {
        match self {
            ScanValue::Str(s) => s,
            _ => "",
        }
    }
    /// The float value (NaN for other kinds).
    pub fn as_float(&self) -> f32 {
        match self {
            ScanValue::Float(v) => *v,
            _ => f32::NAN,
        }
    }
    /// The character value (`'\0'` for other kinds).
    pub fn as_char(&self) -> char {
        match self {
            ScanValue::Char(c) => *c,
            _ => '\0',
        }
    }
}

/// Minimal port of C `sscanf` supporting `%d`, `%f`, `%s`, `%c` (with
/// optional width and `*` assignment suppression), literal characters and
/// white spaces. Returns the C return value (number of assigned
/// conversions, or -1 on an input failure before the first conversion) and
/// the converted values.
pub fn scanf(input: &str, fmt: &str) -> (i32, Vec<ScanValue>) {
    let inp = input.as_bytes();
    let f = fmt.as_bytes();
    let mut ip = 0usize;
    let mut fp = 0usize;
    let mut values = Vec::new();
    let mut count = 0i32;
    // Whether a conversion (even suppressed) was attempted successfully.
    let mut converted_any = false;

    let input_failure = |count: i32, converted_any: bool| -> i32 {
        if count == 0 && !converted_any {
            -1
        } else {
            count
        }
    };

    while fp < f.len() {
        let fc = f[fp];
        if is_c_space(fc) {
            while fp < f.len() && is_c_space(f[fp]) {
                fp += 1;
            }
            while ip < inp.len() && is_c_space(inp[ip]) {
                ip += 1;
            }
            continue;
        }
        if fc != b'%' {
            if ip >= inp.len() {
                return (input_failure(count, converted_any), values);
            }
            if inp[ip] != fc {
                return (count, values);
            }
            ip += 1;
            fp += 1;
            continue;
        }
        // Conversion specification.
        fp += 1;
        if fp < f.len() && f[fp] == b'%' {
            while ip < inp.len() && is_c_space(inp[ip]) {
                ip += 1;
            }
            if ip >= inp.len() {
                return (input_failure(count, converted_any), values);
            }
            if inp[ip] != b'%' {
                return (count, values);
            }
            ip += 1;
            fp += 1;
            continue;
        }
        let mut suppress = false;
        if fp < f.len() && f[fp] == b'*' {
            suppress = true;
            fp += 1;
        }
        let mut width = 0usize;
        while fp < f.len() && f[fp].is_ascii_digit() {
            width = width * 10 + (f[fp] - b'0') as usize;
            fp += 1;
        }
        if fp >= f.len() {
            return (count, values);
        }
        let conv = f[fp];
        fp += 1;
        if conv != b'c' {
            while ip < inp.len() && is_c_space(inp[ip]) {
                ip += 1;
            }
        }
        if ip >= inp.len() {
            return (input_failure(count, converted_any), values);
        }
        let limit = if width > 0 {
            (ip + width).min(inp.len())
        } else {
            inp.len()
        };
        match conv {
            b'd' | b'i' | b'u' => {
                let mut j = ip;
                if j < limit && (inp[j] == b'+' || inp[j] == b'-') {
                    j += 1;
                }
                let ds = j;
                while j < limit && inp[j].is_ascii_digit() {
                    j += 1;
                }
                if j == ds {
                    return (count, values);
                }
                let text = &input[ip..j];
                let v = text.parse::<i64>().unwrap_or(if text.starts_with('-') {
                    i64::MIN
                } else {
                    i64::MAX
                });
                ip = j;
                converted_any = true;
                if !suppress {
                    values.push(ScanValue::Int(v as i32));
                    count += 1;
                }
            }
            b'f' | b'g' | b'e' => {
                let sub = &input[ip..limit];
                let len = float_prefix_len(sub.as_bytes());
                if len == 0 {
                    return (count, values);
                }
                let v = parse_float_text(&sub[..len]).unwrap_or(f64::NAN) as f32;
                ip += len;
                converted_any = true;
                if !suppress {
                    values.push(ScanValue::Float(v));
                    count += 1;
                }
            }
            b's' => {
                let mut j = ip;
                while j < limit && !is_c_space(inp[j]) {
                    j += 1;
                }
                let text = String::from_utf8_lossy(&inp[ip..j]).into_owned();
                ip = j;
                converted_any = true;
                if !suppress {
                    values.push(ScanValue::Str(text));
                    count += 1;
                }
            }
            b'c' => {
                let c = inp[ip] as char;
                ip += 1;
                converted_any = true;
                if !suppress {
                    values.push(ScanValue::Char(c));
                    count += 1;
                }
            }
            _ => return (count, values),
        }
    }
    (count, values)
}

// ---------------------------------------------------------------------------
// Text stream.

/// A read-only text stream reproducing the `std::istream` state semantics
/// relied upon by the readers (`good()`, `eof`, `fail` flags, `getline`,
/// `operator>>` for words).
#[derive(Debug, Clone)]
pub struct IStream<'a> {
    data: &'a [u8],
    pos: usize,
    eof: bool,
    fail: bool,
}

impl<'a> IStream<'a> {
    /// Stream over `data`.
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            eof: false,
            fail: false,
        }
    }

    /// `istream::good()`.
    pub fn good(&self) -> bool {
        !self.eof && !self.fail
    }

    /// `!istream.fail()` i.e. `operator bool`.
    pub fn ok(&self) -> bool {
        !self.fail
    }

    /// `std::getline(istream, line)`.
    pub fn getline(&mut self) -> String {
        self.getline_impl(None)
    }

    /// `istream.getline(buffer, size)`: at most `size - 1` characters are
    /// stored; longer lines set the fail flag.
    pub fn getline_limited(&mut self, size: usize) -> String {
        self.getline_impl(Some(size.saturating_sub(1)))
    }

    fn getline_impl(&mut self, max_chars: Option<usize>) -> String {
        if !self.good() {
            self.fail = true;
            return String::new();
        }
        let start = self.pos;
        let mut extracted = 0usize;
        let mut end = start;
        loop {
            if self.pos >= self.data.len() {
                self.eof = true;
                break;
            }
            let c = self.data[self.pos];
            if c == b'\n' {
                self.pos += 1;
                extracted += 1;
                break;
            }
            if let Some(m) = max_chars {
                if end - start >= m {
                    self.fail = true;
                    break;
                }
            }
            self.pos += 1;
            end += 1;
            extracted += 1;
        }
        if extracted == 0 {
            self.fail = true;
        }
        String::from_utf8_lossy(&self.data[start..end]).into_owned()
    }

    /// `istream >> word`: the next white space separated word, or `None`
    /// (setting the fail flag) at the end of the stream.
    pub fn next_word(&mut self) -> Option<String> {
        if !self.good() {
            self.fail = true;
            return None;
        }
        while self.pos < self.data.len() && is_c_space(self.data[self.pos]) {
            self.pos += 1;
        }
        if self.pos >= self.data.len() {
            self.eof = true;
            self.fail = true;
            return None;
        }
        let start = self.pos;
        while self.pos < self.data.len() && !is_c_space(self.data[self.pos]) {
            self.pos += 1;
        }
        if self.pos >= self.data.len() {
            self.eof = true;
        }
        Some(String::from_utf8_lossy(&self.data[start..self.pos]).into_owned())
    }

    /// Port of `nextline`: read the next non-empty line (a trailing `\r` is
    /// removed). Returns `None` when the stream is exhausted.
    pub fn nextline(&mut self) -> Option<String> {
        while self.good() {
            let mut line = self.getline();
            if line.ends_with('\r') {
                line.pop();
            }
            if !is_empty_or_white_space(&line) {
                return Some(line);
            }
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Binary stream.

/// A read-only binary stream with `std::istream` like failure semantics
/// (once a read fails, every following read fails).
#[derive(Debug, Clone)]
pub struct BinaryStream<'a> {
    data: &'a [u8],
    pos: usize,
    fail: bool,
}

impl<'a> BinaryStream<'a> {
    /// Stream over `data`.
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            fail: false,
        }
    }

    /// True if no read failed so far.
    pub fn good(&self) -> bool {
        !self.fail
    }

    /// Move to an absolute position (no effect once failed).
    pub fn seek(&mut self, pos: usize) {
        if !self.fail {
            self.pos = pos;
        }
    }

    /// Read `n` bytes.
    pub fn read(&mut self, n: usize) -> Option<&'a [u8]> {
        if self.fail {
            return None;
        }
        match self.pos.checked_add(n) {
            Some(end) if end <= self.data.len() => {
                let s = &self.data[self.pos..end];
                self.pos = end;
                Some(s)
            }
            _ => {
                self.pos = self.data.len();
                self.fail = true;
                None
            }
        }
    }

    /// Read a big-endian `u16`.
    pub fn read_u16(&mut self) -> Option<u16> {
        self.read(2).map(|b| u16::from_be_bytes([b[0], b[1]]))
    }

    /// Read a big-endian `u32`.
    pub fn read_u32(&mut self) -> Option<u32> {
        self.read(4)
            .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// Read a big-endian `i32`.
    pub fn read_i32(&mut self) -> Option<i32> {
        self.read_u32().map(|v| v as i32)
    }

    /// Read a big-endian `u64`.
    pub fn read_u64(&mut self) -> Option<u64> {
        self.read(8)
            .map(|b| u64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]))
    }
}

// ---------------------------------------------------------------------------
// Baking helpers (shared by the `FileFormat::bake` implementations).

/// Format `v` like a C++ `std::ostream` using `std::fixed` and the given
/// precision (i.e. `printf("%.*f", precision, v)`, the float being promoted
/// to double). Non finite values are written `nan`, `-nan`, `inf`, `-inf`.
pub fn format_fixed(v: f32, precision: usize) -> String {
    if v.is_nan() {
        return if v.is_sign_negative() { "-nan" } else { "nan" }.to_string();
    }
    if v.is_infinite() {
        return if v < 0.0 { "-inf" } else { "inf" }.to_string();
    }
    format!("{:.*}", precision, f64::from(v))
}

/// Format `v` like a C++ `std::ostream` using `std::fixed` and a precision
/// of 6, which is what most LUT bakers use.
pub fn format_fixed6(v: f32) -> String {
    format_fixed(v, 6)
}

/// The first 3 values of `rgb` formatted with [`format_fixed6`] and
/// separated by a space.
pub fn format_fixed6_rgb(rgb: &[f32]) -> String {
    rgb.iter()
        .take(3)
        .map(|&v| format_fixed6(v))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Allocate a zeroed buffer of `num_values` floats (`None` meaning the size
/// computation overflowed), reporting an error instead of aborting when
/// the buffer can't be allocated (C++ throws `std::bad_alloc`).
pub fn alloc_bake_buffer(num_values: Option<usize>) -> Result<Vec<f32>> {
    let too_large = || crate::error::Error::msg("The requested LUT size is too large.");
    let n = num_values.ok_or_else(too_large)?;
    let mut v: Vec<f32> = Vec::new();
    v.try_reserve_exact(n).map_err(|_| too_large())?;
    v.resize(n, 0.0);
    Ok(v)
}

/// An RGB identity 3D LUT of `edge_len`^3 entries in the given order (the
/// `GenerateIdentityLut3D` calls of the bakers).
pub fn bake_identity_lut3d(
    edge_len: usize,
    order: crate::ops::lut3d::Lut3DOrder,
) -> Result<Vec<f32>> {
    let n = edge_len
        .checked_mul(edge_len)
        .and_then(|v| v.checked_mul(edge_len))
        .and_then(|v| v.checked_mul(3));
    let mut data = alloc_bake_buffer(n)?;
    crate::ops::lut3d::generate_identity_lut3d(&mut data, edge_len, 3, order)?;
    Ok(data)
}

/// An RGB identity ramp of `size` entries in `[0, 1]` (the
/// `GenerateIdentityLut1D` calls of the bakers, with 3 channels).
pub fn bake_identity_lut1d(size: usize) -> Result<Vec<f32>> {
    let mut data = alloc_bake_buffer(size.checked_mul(3))?;
    crate::ops::lut1d::generate_identity_lut1d(&mut data, size, 3);
    Ok(data)
}

/// An RGB linear ramp of `size` entries from `start` to `end` (the
/// `GenerateLinearScaleLut1D` calls of the bakers, with 3 channels).
pub fn bake_linear_scale_lut1d(size: usize, start: f32, end: f32) -> Result<Vec<f32>> {
    let mut data = alloc_bake_buffer(size.checked_mul(3))?;
    crate::ops::lut1d::generate_linear_scale_lut1d(&mut data, size, 3, start, end);
    Ok(data)
}

/// Append each element value of `metadata`'s children as a line starting
/// with `prefix` (the bakers writing the baker metadata as comments).
pub fn write_metadata_lines(
    out: &mut String,
    metadata: &crate::format_metadata::FormatMetadata,
    prefix: &str,
) {
    for child in &metadata.children {
        out.push_str(prefix);
        out.push_str(child.element_value());
        out.push('\n');
    }
}

/// Test helpers for the `bake` implementations of the formats.
#[cfg(test)]
pub(crate) mod bake_test_utils {
    use crate::baker::{input_to_target_processor, Baker};
    use crate::config::Config;
    use crate::fileformats::FormatRegistry;
    use crate::transforms::Transform;
    use crate::types::{Interpolation, OptimizationFlags, TransformDirection};

    /// The config of the `bake_1d_shaper` tests (a linear and a log space).
    pub const SHAPER_LOG2_CONFIG: &str = r#"
        ocio_profile_version: 1

        colorspaces:
        - !<ColorSpace>
          name : Raw
          isdata : false

        - !<ColorSpace>
          name: Log2
          isdata: false
          from_reference: !<GroupTransform>
            children:
              - !<MatrixTransform> {matrix: [5.55556, 0, 0, 0, 0, 5.55556, 0, 0, 0, 0, 5.55556, 0, 0, 0, 0, 1]}
              - !<LogTransform> {base: 2}
              - !<MatrixTransform> {offset: [6.5, 6.5, 6.5, 0]}
              - !<MatrixTransform> {matrix: [0.076923, 0, 0, 0, 0, 0.076923, 0, 0, 0, 0, 0.076923, 0, 0, 0, 0, 1]}
    "#;

    /// A v2 config whose first color space is the reference (and default)
    /// one; each space is `(name, extra yaml lines)`, the extra lines being
    /// indented by 4 spaces (e.g. `"from_scene_reference: !<...> {...}"`).
    pub fn config_yaml(spaces: &[(&str, &str)]) -> String {
        let mut s = String::from("ocio_profile_version: 2\n\n");
        s.push_str("luma: [0.333, 0.333, 0.333]\n\n");
        s.push_str(&format!(
            "roles:\n  reference: {0}\n  default: {0}\n\ncolorspaces:\n",
            spaces[0].0
        ));
        for (name, extra) in spaces {
            s.push_str(&format!(
                "  - !<ColorSpace>\n    name: {name}\n    family: {name}\n"
            ));
            for line in extra.lines().filter(|l| !l.trim().is_empty()) {
                s.push_str("    ");
                s.push_str(line.trim_start());
                s.push('\n');
            }
            s.push('\n');
        }
        s
    }

    /// A baker for `format` using the config described by `yaml`.
    pub fn baker(yaml: &str, format: &str) -> Baker {
        let config = Config::create_from_str(yaml).unwrap();
        let mut baker = Baker::new();
        baker.set_config(&config);
        baker.set_format(format).unwrap();
        baker
    }

    /// Bake with the full validation of `Baker::bake`.
    pub fn bake(baker: &Baker) -> String {
        String::from_utf8(baker.bake().unwrap()).unwrap()
    }

    /// Compare line by line, as numbers (within `tol`) for the lines
    /// satisfying `numeric(line_index)`, as text otherwise.
    pub fn compare_lines(result: &str, expected: &str, tol: f32, numeric: impl Fn(usize) -> bool) {
        let res: Vec<&str> = result.lines().collect();
        let exp: Vec<&str> = expected.lines().collect();
        assert_eq!(res.len(), exp.len(), "{result}");
        for (i, (r, e)) in res.iter().zip(exp.iter()).enumerate() {
            if numeric(i) {
                let rv: Vec<f32> = r.split_whitespace().map(|s| s.parse().unwrap()).collect();
                let ev: Vec<f32> = e.split_whitespace().map(|s| s.parse().unwrap()).collect();
                assert_eq!(rv.len(), ev.len(), "line {i}: '{r}' vs '{e}'");
                for (a, b) in rv.iter().zip(ev.iter()) {
                    assert!((a - b).abs() <= tol, "line {i}: '{r}' vs '{e}'");
                }
            } else {
                assert_eq!(r, e, "line {i}");
            }
        }
    }

    /// Bake, read the result back with the format reader and check that
    /// the LUT reproduces the baked transform on `samples` within `tol`.
    pub fn check_round_trip(baker: &Baker, samples: &[[f32; 3]], tol: f32) {
        let bytes = baker.bake().unwrap();
        let fmt = FormatRegistry::instance()
            .format_by_name(baker.format())
            .unwrap();
        let cached = fmt
            .read(&bytes, "memory file", Interpolation::Linear)
            .unwrap();
        let config = baker.config().unwrap();
        let lut = config
            .get_processor_for_transform(
                &Transform::Group(cached.group),
                TransformDirection::Forward,
            )
            .unwrap()
            .optimized_cpu_processor(OptimizationFlags::NONE);
        let reference = input_to_target_processor(baker).unwrap();
        for s in samples {
            let mut a = *s;
            let mut b = *s;
            lut.apply_rgb(&mut a);
            reference.apply_rgb(&mut b);
            for c in 0..3 {
                assert!(
                    (a[c] - b[c]).abs() <= tol,
                    "sample {s:?}: LUT {a:?} vs transform {b:?}"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baking_fixed_format() {
        assert_eq!(format_fixed6(0.0), "0.000000");
        assert_eq!(format_fixed6(-0.0), "-0.000000");
        assert_eq!(format_fixed6(1.0 / 3.0), "0.333333");
        assert_eq!(format_fixed6(16.291878), "16.291878");
        assert_eq!(format_fixed6(-2.0), "-2.000000");
        assert_eq!(format_fixed6(f32::NAN), "nan");
        assert_eq!(format_fixed6(f32::INFINITY), "inf");
        assert_eq!(format_fixed6(f32::NEG_INFINITY), "-inf");
        assert_eq!(format_fixed(0.5, 0), "0");
        assert!(alloc_bake_buffer(None).is_err());
        assert!(alloc_bake_buffer(Some(usize::MAX)).is_err());
        assert!(bake_identity_lut3d(usize::MAX, crate::ops::lut3d::Lut3DOrder::FastRed).is_err());
        let lut = bake_identity_lut3d(2, crate::ops::lut3d::Lut3DOrder::FastRed).unwrap();
        assert_eq!(&lut[..6], &[0.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
        let ramp = bake_linear_scale_lut1d(3, 1.0, 2.0).unwrap();
        assert_eq!(ramp, vec![1.0, 1.0, 1.0, 1.5, 1.5, 1.5, 2.0, 2.0, 2.0]);
        assert_eq!(
            bake_identity_lut1d(2).unwrap(),
            vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0]
        );
    }

    #[test]
    fn number_parsing() {
        assert_eq!(from_chars_f32("1.5"), Some(1.5));
        assert_eq!(from_chars_f32("  +2e2x"), Some(200.0));
        assert_eq!(from_chars_f32("-0.25"), Some(-0.25));
        assert!(from_chars_f32("nan").unwrap().is_nan());
        assert_eq!(from_chars_f32("-inf"), Some(f32::NEG_INFINITY));
        assert_eq!(from_chars_f32("0x1p1"), Some(2.0));
        assert_eq!(from_chars_f32("abc"), None);
        assert_eq!(from_chars_f32(""), None);
        assert_eq!(from_chars_f32("1e50"), None);
        assert_eq!(from_chars_f32(".5"), Some(0.5));
        assert_eq!(from_chars_f32("5."), Some(5.0));
        assert_eq!(from_chars_f32("."), None);

        assert_eq!(string_to_int(" 12", false), Some(12));
        assert_eq!(string_to_int("12abc", false), Some(12));
        assert_eq!(string_to_int("12abc", true), None);
        assert_eq!(string_to_int("-3", true), Some(-3));
        assert_eq!(string_to_int("x", false), None);
        assert_eq!(string_to_int("99999999999", false), None);

        assert_eq!(from_chars_i64("\"8\""), None);
        assert_eq!(from_chars_i64("8"), Some(8));
    }

    #[test]
    fn sscanf() {
        let (n, v) = scanf("Version 1", "Version %d");
        assert_eq!(n, 1);
        assert_eq!(v[0].as_int(), 1);
        let (n, _) = scanf("Version1", "Version %d");
        assert_eq!(n, 1);
        let (n, _) = scanf("Versionx", "Version %d");
        assert_eq!(n, 0);
        let (n, _) = scanf("", "%d");
        assert_eq!(n, -1);
        let (n, v) = scanf("lut_3d_size 2 x", "lut_3d_size %d %c");
        assert_eq!(n, 2);
        assert_eq!(v[1].as_char(), 'x');
        let (n, v) = scanf("LUT: 3 1024 65536f", "%*s %d %d %15s");
        assert_eq!(n, 3);
        assert_eq!(v[2].as_str(), "65536f");
        let (n, v) = scanf("0 1 2 0.5 0.25 1e-1", "%d %d %d %63s %63s %63s");
        assert_eq!(n, 6);
        assert_eq!(v[5].as_str(), "1e-1");
    }

    #[test]
    fn stream() {
        let mut s = IStream::new(b"a\n\nb");
        assert_eq!(s.getline(), "a");
        assert!(s.good());
        assert_eq!(s.getline(), "");
        assert!(s.good());
        assert_eq!(s.getline(), "b");
        assert!(!s.good());
        assert!(s.ok());
        assert_eq!(s.getline(), "");
        assert!(!s.ok());

        let mut s = IStream::new(b"x\n\n  \r\ny z\n");
        assert_eq!(s.nextline().as_deref(), Some("x"));
        assert_eq!(s.nextline().as_deref(), Some("y z"));
        assert_eq!(s.nextline(), None);

        let mut s = IStream::new(b" w1  w2\n");
        assert_eq!(s.next_word().as_deref(), Some("w1"));
        assert_eq!(s.next_word().as_deref(), Some("w2"));
        assert_eq!(s.next_word(), None);

        let mut s = IStream::new(b"abcdef\n");
        assert_eq!(s.getline_limited(4), "abc");
        assert!(!s.ok());
    }

    #[test]
    fn red_fastest() {
        let mut lut = Lut3DTransform::new(2);
        let raw: Vec<f32> = (0..24).map(|v| v as f32).collect();
        set_lut3d_from_red_fastest(&mut lut, &raw).unwrap();
        // Red index 1, green 0, blue 0 is the second entry of the source.
        assert_eq!(lut.value(1, 0, 0), [3.0, 4.0, 5.0]);
        assert_eq!(lut.value(0, 0, 1), [12.0, 13.0, 14.0]);
        assert!(set_lut3d_from_red_fastest(&mut lut, &raw[..3]).is_err());
    }

    #[test]
    fn edge_len() {
        assert_eq!(get_3d_lut_edge_len_from_num_pixels(8).unwrap(), 2);
        assert_eq!(get_3d_lut_edge_len_from_num_pixels(35937).unwrap(), 33);
        assert!(get_3d_lut_edge_len_from_num_pixels(9).is_err());
    }

    #[test]
    fn min_max() {
        assert!(min_max_matrix_f32(0.0, 1.0).unwrap().is_none());
        let m = min_max_matrix_f32(-1.0, 3.0).unwrap().unwrap();
        assert_eq!(m.matrix[0], 0.25);
        assert_eq!(m.offset[0], 0.25);
        assert_eq!(m.matrix[15], 1.0);
        assert!(min_max_matrix_f32(1.0, 1.0).is_err());
    }
}
