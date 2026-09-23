//! Discreet (Autodesk Flame / Smoke) 1D LUT `lut` format (port of
//! `FileFormatDiscreet1DL.cpp`).
//!
//! This format is now deprecated (but still supported) in those products.
//! It has been supplanted by the Academy CLF/CTF format.
//!
//! Two flavors exist: the old format (a single table of 256 values) and
//! the new format starting with a `LUT: <numtables> <length> [dstDepth]`
//! header followed by 1, 3 or 4 tables of integer values.

use super::utils::{bit_depth_max_value, new_lut1d, scanf, IStream};
use super::{bake_capability, capability, CachedFile, FileFormat, FormatInfo};
use crate::error::Result;
use crate::path_utils;
use crate::transforms::GroupTransform;
use crate::types::{BitDepth, Interpolation};

/// Size of the line buffer of the original reader.
const LINE_BUFFER_SIZE: usize = 200;

/// Defined values of supported LUT formats (mapped onto `IM_BitsPerChannel`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LutBitsPerChannel {
    Unknown,
    Bits8,
    Bits10,
    Bits12,
    Bits16,
    Half,
    Float,
}

impl LutBitsPerChannel {
    fn bit_depth(self) -> BitDepth {
        match self {
            LutBitsPerChannel::Unknown => BitDepth::Unknown,
            LutBitsPerChannel::Bits8 => BitDepth::UInt8,
            LutBitsPerChannel::Bits10 => BitDepth::UInt10,
            LutBitsPerChannel::Bits12 => BitDepth::UInt12,
            LutBitsPerChannel::Bits16 => BitDepth::UInt16,
            LutBitsPerChannel::Half => BitDepth::F16,
            LutBitsPerChannel::Float => BitDepth::F32,
        }
    }
}

/// Convert between table size and bit depth.
fn table_size_to_bit_depth(table_size: i32, is_float: bool) -> LutBitsPerChannel {
    match table_size {
        256 => LutBitsPerChannel::Bits8,
        1024 => LutBitsPerChannel::Bits10,
        4096 => LutBitsPerChannel::Bits12,
        65536 => {
            if is_float {
                LutBitsPerChannel::Half
            } else {
                LutBitsPerChannel::Bits16
            }
        }
        _ => LutBitsPerChannel::Unknown,
    }
}

/// Image LUT library errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LutError {
    UnexpectedEof,
    Syntax,
}

impl LutError {
    fn as_str(self) -> &'static str {
        match self {
            LutError::UnexpectedEof => "Premature EOF reading LUT file",
            LutError::Syntax => "Syntax error reading LUT file",
        }
    }
}

/// A look-up table descriptor.
struct LutStruct {
    length: i32,
    target_bit_depth: LutBitsPerChannel,
    src_bit_depth: LutBitsPerChannel,
    tables: Vec<Vec<u16>>,
}

impl LutStruct {
    fn new(num: usize, length: i32) -> Self {
        // On import, we never supported LUTs with 16bit integer input
        // (16bit integer input was interpreted as 12bit). On export, 16bit
        // input is necessarily float.
        Self {
            length,
            src_bit_depth: table_size_to_bit_depth(length, true),
            // targetBitDepth will be set appropriately for conversion LUTs.
            target_bit_depth: table_size_to_bit_depth(length, false),
            tables: vec![vec![0u16; length.max(0) as usize]; num],
        }
    }
}

/// Replace tabs by spaces and strip leading & trailing spaces (port of
/// `ReplaceTabsAndStripSpaces`).
pub(crate) fn replace_tabs_and_strip_spaces(s: &str) -> String {
    s.replace('\t', " ").trim_matches(' ').to_string()
}

/// Remove one trailing line feed or carriage return (port of
/// `StripEndNewLine`).
pub(crate) fn strip_end_new_line(s: &str) -> String {
    match s.as_bytes().last() {
        Some(b'\n') | Some(b'\r') => s[..s.len() - 1].to_string(),
        _ => s.to_string(),
    }
}

/// Port of `std::stoi` (leading white spaces, sign, digits; the value must
/// fit in an `int`).
fn stoi(s: &str) -> Option<i32> {
    super::utils::string_to_int(s, false)
}

fn starts_with_digit(s: &str) -> bool {
    s.as_bytes().first().is_some_and(|c| c.is_ascii_digit())
}

/// Load values from the stream into `table`, starting at `start`.
fn table_load(
    istream: &mut IStream,
    table: &mut [u16],
    start: usize,
    line: &mut i32,
    error_line: &mut String,
) -> std::result::Result<(), LutError> {
    let length = table.len();
    let mut count = start;
    while istream.good() {
        *line += 1;
        let in_string = istream.getline_limited(LINE_BUFFER_SIZE);
        if !istream.good() {
            return Err(LutError::UnexpectedEof);
        }
        let in_string = strip_end_new_line(&replace_tabs_and_strip_spaces(&in_string));
        if starts_with_digit(&in_string) {
            match stoi(&in_string) {
                Some(v) => {
                    if count < length {
                        table[count] = v as u16;
                    }
                    count += 1;
                }
                None => {
                    *error_line = in_string;
                    return Err(LutError::Syntax);
                }
            }
            if count >= length {
                break;
            }
        } else if !in_string.is_empty() {
            *error_line = in_string;
            return Err(LutError::Syntax);
        }
    }
    Ok(())
}

/// Find the first line that is not blank or a comment. Returns the line
/// and whether the stream is still good.
fn find_non_comment(istream: &mut IStream, line: &mut i32) -> (String, bool) {
    let mut in_string = String::new();
    let mut read_on = true;
    while istream.good() && read_on {
        read_on = false;
        in_string = istream.getline_limited(LINE_BUFFER_SIZE);
        *line += 1;
        in_string = strip_end_new_line(&replace_tabs_and_strip_spaces(&in_string));
        if in_string.is_empty() || in_string.starts_with('#') {
            read_on = true;
        }
    }
    (in_string, istream.good())
}

/// Determine the bit depth of a LUT from its file name: the first `to`
/// sequence of characters is searched for and the following numeric
/// characters are parsed (e.g. `12to10log`).
fn bit_depth_from_file_name(file_name: &str) -> LutBitsPerChannel {
    if file_name.is_empty() {
        return LutBitsPerChannel::Unknown;
    }
    let lower = file_name.to_ascii_lowercase();
    let Some(pos) = lower.find("to") else {
        return LutBitsPerChannel::Unknown;
    };
    let t = &lower.as_bytes()[pos + 2..];
    let at = |i: usize| t.get(i).copied().unwrap_or(0);
    match at(0) {
        b'8' => LutBitsPerChannel::Bits8,
        b'1' => match at(1) {
            b'0' => LutBitsPerChannel::Bits10,
            b'2' => LutBitsPerChannel::Bits12,
            b'6' => {
                if at(2) == b'f' {
                    LutBitsPerChannel::Half
                } else {
                    LutBitsPerChannel::Bits16
                }
            }
            _ => LutBitsPerChannel::Unknown,
        },
        b'3' => {
            if at(1) == b'2' && at(2) == b'f' {
                LutBitsPerChannel::Float
            } else {
                LutBitsPerChannel::Unknown
            }
        }
        _ => LutBitsPerChannel::Unknown,
    }
}

/// Read a file as an image look-up table (port of `IMLutGet`). On error,
/// returns the error, the line of the error and its content.
fn lut_get(
    istream: &mut IStream,
    file_name: &str,
) -> std::result::Result<LutStruct, (LutError, i32, String)> {
    let mut line = 0i32;
    let mut error_line = String::new();

    // Find first line that is not blank or a comment.
    let (in_string, good) = find_non_comment(istream, &mut line);
    if !good {
        return Err((LutError::UnexpectedEof, line, error_line));
    }

    let mut lut;
    let tablestart;
    let mut depth_scaled = LutBitsPerChannel::Unknown;

    if starts_with_digit(&in_string) {
        // Old format LUT file: 1 table of 256 entries.
        lut = LutStruct::new(1, 256);
        // Load first table value.
        match stoi(&in_string) {
            Some(v) => lut.tables[0][0] = v as u16,
            None => return Err((LutError::Syntax, line, in_string)),
        }
        tablestart = 1;
    } else {
        let (nummatched, values) = scanf(&in_string, "%*s %d %d %15s");
        let numtables = values.first().map(|v| v.as_int()).unwrap_or(0);
        let length = values.get(1).map(|v| v.as_int()).unwrap_or(0);
        let sub_str = in_string
            .get(..5)
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_default();
        if nummatched < 2
            || sub_str != "lut: "
            || (numtables != 1 && numtables != 3 && numtables != 4)
            || length <= 0
            || length > 65536
        {
            return Err((LutError::Syntax, line, in_string));
        }

        // New format LUT file: "numtables" tables, each of "length"
        // entries. Optional dstDepth.
        if nummatched > 2 {
            // Optional dstDepth was specified. Validate it.
            let dst_depth_s = values[2].as_str();
            let (n, v) = scanf(dst_depth_s, "%d%c");
            let dst_depth = if n >= 1 { v[0].as_int() } else { 0 };
            let float_c = if n >= 2 { v[1].as_char() } else { ' ' };

            // Currently when Smoke exports a 16f output depth it uses
            // "65536f" as the third token. However it is likely that earlier
            // versions either wrote only two tokens or wrote the third token
            // without the "f". In that case we may wrongly interpret a 16f
            // outDepth as 16i.
            depth_scaled = table_size_to_bit_depth(dst_depth, float_c == 'f' || float_c == 'F');
            if depth_scaled == LutBitsPerChannel::Unknown {
                return Err((LutError::Syntax, line, in_string));
            }
        }

        lut = LutStruct::new(numtables as usize, length);
        tablestart = 0;
    }

    for i in 0..lut.tables.len() {
        table_load(
            istream,
            &mut lut.tables[i],
            tablestart,
            &mut line,
            &mut error_line,
        )
        .map_err(|e| (e, line, error_line.clone()))?;
    }

    if lut.tables.len() == 1 {
        let t = lut.tables[0].clone();
        lut.tables.push(t.clone());
        lut.tables.push(t);
    }

    if depth_scaled == LutBitsPerChannel::Unknown {
        depth_scaled = bit_depth_from_file_name(file_name);
    }

    if depth_scaled != LutBitsPerChannel::Unknown {
        lut.target_bit_depth = depth_scaled;
    }

    // If there are any more lines in the file that are not blank or
    // comments, it's a syntax error.
    let (in_string, good) = find_non_comment(istream, &mut line);
    if good {
        return Err((LutError::Syntax, line, in_string));
    }

    Ok(lut)
}

struct LocalFileFormat;

pub(crate) fn create() -> Box<dyn FileFormat> {
    Box::new(LocalFileFormat)
}

impl FileFormat for LocalFileFormat {
    fn format_info(&self) -> Vec<FormatInfo> {
        vec![FormatInfo {
            name: "Discreet 1D LUT",
            extension: "lut",
            capabilities: capability::READ,
            bake_capabilities: bake_capability::NONE,
        }]
    }

    fn read(&self, data: &[u8], file_path: &str, interp: Interpolation) -> Result<CachedFile> {
        // The file name without directory nor extension.
        let base = path_utils::basename(file_path);
        let file_name = match base.rfind('.') {
            Some(i) if i > 0 => base[..i].to_string(),
            _ => base,
        };

        let mut istream = IStream::new(data);
        let discreet_lut = match lut_get(&mut istream, &file_name) {
            Ok(l) => l,
            Err((status, errline, error_line)) => {
                let mut os = format!(
                    "Error parsing .lut file ({}) using Discreet 1D LUT reader. Error is: {}",
                    file_path,
                    status.as_str()
                );
                if status == LutError::Syntax {
                    os.push_str(&format!(" At line ({errline}): '{error_line}'."));
                }
                crate::bail!("{os}");
            }
        };

        let input_bd = discreet_lut.src_bit_depth.bit_depth();
        let output_bd = discreet_lut.target_bit_depth.bit_depth();
        let lut_size = discreet_lut.length.max(0) as usize;

        let half_domain = input_bd == BitDepth::F16;
        let mut lut = new_lut1d(lut_size, half_domain, interp, output_bd);

        let scale = bit_depth_max_value(output_bd)? as f32;
        let src_table_limit = discreet_lut.tables.len() - 1;
        let is_half = discreet_lut.target_bit_depth == LutBitsPerChannel::Half;
        let mut p = 0;
        for i in 0..lut_size {
            for j in 0..3 {
                let src_table = j.min(src_table_limit);
                let raw = discreet_lut.tables[src_table][i];
                lut.values[p] = if is_half {
                    // Convert raw half values to floats.
                    half::f16::from_bits(raw).to_f32()
                } else {
                    raw as f32 / scale
                };
                p += 1;
            }
        }

        let mut group = GroupTransform::new();
        group.append(lut);
        Ok(CachedFile::new(group))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transforms::{Lut1DTransform, Transform};

    fn load_lut_file(name: &str) -> Result<Lut1DTransform> {
        let path = format!("{}/tests/data/files/{}", env!("CARGO_MANIFEST_DIR"), name);
        let data = std::fs::read(&path).expect("test file");
        let file = LocalFileFormat.read(&data, &path, Interpolation::Default)?;
        assert_eq!(file.group.num_transforms(), 1);
        match &file.group.transforms[0] {
            Transform::Lut1D(l) => Ok(l.clone()),
            _ => panic!("expected a Lut1D"),
        }
    }

    #[test]
    fn format_info() {
        let info = LocalFileFormat.format_info();
        assert_eq!(info.len(), 1);
        assert_eq!(info[0].name, "Discreet 1D LUT");
        assert_eq!(info[0].extension, "lut");
        assert_eq!(info[0].capabilities, capability::READ);
    }

    #[test]
    fn test_string_util() {
        let strip = |s: &str| replace_tabs_and_strip_spaces(s);
        assert_eq!(strip("this is a test"), "this is a test");
        assert_eq!(strip("   this is a test      "), "this is a test");
        assert_eq!(strip(" \t  this\tis a test    \t  "), "this is a test");
        assert_eq!(strip("\t \t  this is a  test    \t  \t"), "this is a  test");
        assert_eq!(
            strip("\t \t  this\nis a\t\ttest    \t  \t"),
            "this\nis a  test"
        );
        assert_eq!(strip(""), "");

        assert_eq!(strip_end_new_line(""), "");
        assert_eq!(strip_end_new_line("\n"), "");
        assert_eq!(strip_end_new_line("\r"), "");
        assert_eq!(strip_end_new_line("a\n"), "a");
        assert_eq!(strip_end_new_line("b\r"), "b");
        assert_eq!(strip_end_new_line("\na"), "\na");
        assert_eq!(strip_end_new_line("\rb"), "\rb");
    }

    #[test]
    fn test_lut1d_8i_8i() {
        let lut = load_lut_file("logtolin_8to8.lut").unwrap();
        assert_eq!(lut.metadata.id(), "");
        assert_eq!(lut.metadata.name(), "");
        assert_eq!(lut.interpolation, Interpolation::Default);
        assert_eq!(lut.file_output_bit_depth, BitDepth::UInt8);
        assert!(!lut.input_half_domain);
        assert!(!lut.output_raw_halfs);
        assert_eq!(lut.length(), 256);
        assert_eq!(lut.values.len(), 256 * 3);

        // Select some samples to verify the LUT was fully read.
        let expected: [f32; 60] = [
            0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 3.0, 6.0, 9.0, //
            12.0, 15.0, 18.0, 22.0, 25.0, 30.0, 33.0, 37.0, 43.0, 48.0, //
            52.0, 59.0, 64.0, 70.0, 78.0, 85.0, 92.0, 101.0, 109.0, 117.0, //
            129.0, 138.0, 148.0, 161.0, 173.0, 185.0, 201.0, 214.0, 229.0, 248.0, //
            255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, //
            255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0,
        ];
        for (ei, li) in (0..lut.values.len()).step_by(13).enumerate() {
            assert_eq!(lut.values[li] * 255.0, expected[ei], "index {li}");
        }
    }

    #[test]
    fn test_lut1d_12i_16f() {
        let lut = load_lut_file("Test_12to16fp.lut").unwrap();
        assert_eq!(lut.interpolation, Interpolation::Default);
        assert_eq!(lut.file_output_bit_depth, BitDepth::F16);
        assert!(!lut.input_half_domain);
        assert!(!lut.output_raw_halfs);
        assert_eq!(lut.length(), 4096);

        let expected: [u16; 60] = [
            0, 12546, 13171, 13491, 13705, 13898, 14074, 14238, 14365, 14438, //
            14507, 14574, 14638, 14700, 14760, 14818, 14875, 14930, 14983, 15037, //
            15094, 15156, 15222, 15294, 15366, 15408, 15453, 15501, 15553, 15609, //
            15669, 15733, 15802, 15876, 15954, 16038, 16128, 16224, 16327, 16410, //
            16468, 16530, 16596, 16667, 16741, 16821, 16905, 16995, 17090, 17191, //
            17298, 17410, 17470, 17534, 17602, 17673, 17749, 17829, 17914, 18003,
        ];
        for (ei, li) in (0..lut.values.len()).step_by(207).enumerate() {
            assert_eq!(
                half::f16::from_f32(lut.values[li]).to_bits(),
                expected[ei],
                "index {li}"
            );
        }
    }

    #[test]
    fn test_lut1d_16f_16f() {
        let lut = load_lut_file("photo_default_16fpto16fp.lut").unwrap();
        assert_eq!(lut.interpolation, Interpolation::Default);
        assert_eq!(lut.file_output_bit_depth, BitDepth::F16);
        assert!(lut.input_half_domain);
        assert!(!lut.output_raw_halfs);
        assert_eq!(lut.length(), 65536);

        let expected: [u16; 60] = [
            0, 242, 554, 1265, 2463, 3679, 4918, 6234, 7815, 9945, //
            11918, 13222, 14063, 14616, 14958, 15176, 15266, 15349, 15398, 15442, //
            15488, 15536, 15586, 15637, 15690, 15745, 15802, 15862, 15923, 15987, //
            32770, 33862, 34954, 36047, 37139, 38231, 39324, 40416, 41508, 42601, //
            43693, 44785, 45878, 46970, 48062, 49155, 50247, 51339, 52432, 53524, //
            54616, 55709, 56801, 57893, 58986, 60078, 61170, 62263, 63355, 64447,
        ];
        for (ei, li) in (0..lut.values.len()).step_by(3277).enumerate() {
            assert_eq!(
                half::f16::from_f32(lut.values[li]).to_bits(),
                expected[ei],
                "index {li}"
            );
        }
    }

    #[test]
    fn test_lut1d_16f_12i() {
        let lut = load_lut_file("Test_16fpto12.lut").unwrap();
        assert_eq!(lut.interpolation, Interpolation::Default);
        assert_eq!(lut.file_output_bit_depth, BitDepth::UInt12);
        assert!(lut.input_half_domain);
        assert!(!lut.output_raw_halfs);
        assert_eq!(lut.length(), 65536);

        let expected: [f32; 60] = [
            0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 3.0, //
            10.0, 36.0, 130.0, 466.0, 1585.0, 2660.0, 3595.0, 4095.0, 4095.0, 4095.0, //
            4095.0, 4095.0, 4095.0, 4095.0, 4095.0, 4095.0, 4095.0, 4095.0, 4095.0, 4095.0, //
            0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, //
            0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, //
            0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
        ];
        for (ei, li) in (0..lut.values.len()).step_by(3277).enumerate() {
            assert_eq!(lut.values[li] * 4095.0, expected[ei], "index {li}");
        }
    }

    #[test]
    fn test_bad_file() {
        assert!(load_lut_file("error_truncated_file.lut").is_err());
    }

    #[test]
    fn test_errors() {
        let read =
            |s: &str| LocalFileFormat.read(s.as_bytes(), "/a/b/file.lut", Interpolation::Default);
        let e = read("").unwrap_err();
        assert_eq!(
            e.message(),
            "Error parsing .lut file (/a/b/file.lut) using Discreet 1D LUT reader. Error is: Premature EOF reading LUT file"
        );
        let e = read("LUT: 2 256\n0\n").unwrap_err();
        assert_eq!(
            e.message(),
            "Error parsing .lut file (/a/b/file.lut) using Discreet 1D LUT reader. Error is: Syntax error reading LUT file At line (1): 'LUT: 2 256'."
        );
        let e = read("LUT: 1 3\n0\nx\n").unwrap_err();
        assert!(
            e.message().ends_with("At line (3): 'x'."),
            "{}",
            e.message()
        );
        // Unknown bit depth (length 3 does not map to a bit depth).
        let e = read("LUT: 1 3\n0\n1\n2\n").unwrap_err();
        assert_eq!(e.message(), "Bit depth is not supported: unknown.");
        // 3 tables with a 10 bits output depth.
        let mut content = String::from("# comment\nLUT: 3 256 1024\n");
        for t in 0..3 {
            for i in 0..256 {
                content.push_str(&format!("{}\n", i * (t + 1)));
            }
        }
        let file = read(&content).unwrap();
        let Transform::Lut1D(lut) = &file.group.transforms[0] else {
            panic!()
        };
        assert_eq!(lut.file_output_bit_depth, BitDepth::UInt10);
        assert_eq!(lut.value(10), [10.0 / 1023.0, 20.0 / 1023.0, 30.0 / 1023.0]);
        // Extra content is a syntax error.
        content.push_str("5\n\n");
        assert!(read(&content)
            .unwrap_err()
            .message()
            .contains("Syntax error"));
    }

    #[test]
    fn test_bit_depth_from_file_name() {
        // The first "to" is used.
        assert_eq!(
            bit_depth_from_file_name("logtolin_8to8"),
            LutBitsPerChannel::Unknown
        );
        assert_eq!(
            bit_depth_from_file_name("lin_8to8"),
            LutBitsPerChannel::Bits8
        );
        assert_eq!(
            bit_depth_from_file_name("12to10log"),
            LutBitsPerChannel::Bits10
        );
        assert_eq!(
            bit_depth_from_file_name("a_to12"),
            LutBitsPerChannel::Bits12
        );
        assert_eq!(
            bit_depth_from_file_name("Test_12to16fp"),
            LutBitsPerChannel::Half
        );
        assert_eq!(
            bit_depth_from_file_name("x_to16"),
            LutBitsPerChannel::Bits16
        );
        assert_eq!(
            bit_depth_from_file_name("x_to32f"),
            LutBitsPerChannel::Float
        );
        assert_eq!(
            bit_depth_from_file_name("x_to3"),
            LutBitsPerChannel::Unknown
        );
        assert_eq!(bit_depth_from_file_name("abc"), LutBitsPerChannel::Unknown);
        assert_eq!(bit_depth_from_file_name(""), LutBitsPerChannel::Unknown);
    }
}
