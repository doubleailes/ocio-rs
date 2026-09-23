//! Houdini `lut` format (port of `FileFormatHDL.cpp`).
//!
//! See <http://www.sidefx.com/docs/hdk11.0/hdk_io_lut.html>. Supported
//! types:
//!
//! * `C`: 1D LUT (partial support),
//! * `3D`: 3D LUT (red fastest),
//! * `3D+1D`: 3D LUT with a 1D prelut.
//!
//! TODO (as in OCIO): add support for the other 1D types (R, G, B, A, RGB,
//! RGBA, All) and for the `Sampling` tag.

use super::utils::{
    from_chars_f32, min_max_matrix_f32, new_lut1d, new_lut3d, set_lut3d_from_red_fastest,
    split_by_white_spaces, string_to_float, string_to_int, trim, IStream, MAX_1D_LUT_LENGTH,
    MAX_3D_LUT_LENGTH,
};
use super::{bake_capability, capability, CachedFile, FileFormat, FormatInfo};
use crate::error::{Error, Result};
use crate::transforms::{GroupTransform, Lut1DTransform};
use crate::types::{BitDepth, Interpolation};
use std::collections::BTreeMap;

type StringToStringVecMap = BTreeMap<String, Vec<String>>;
type StringToFloatVecMap = BTreeMap<String, Vec<f32>>;

/// Read the headers into key / values pairs, stopping after the `LUT:` line.
fn read_headers(istream: &mut IStream) -> StringToStringVecMap {
    let mut headers = StringToStringVecMap::new();
    while let Some(line) = istream.nextline() {
        // Remove trailing/leading white spaces, lower-case and split into words.
        let mut chunks = split_by_white_spaces(&trim(&line).to_ascii_lowercase());

        // Skip empty lines.
        if chunks.is_empty() {
            continue;
        }

        // Stop looking for headers at the "LUT:" line.
        if chunks[0] == "lut:" {
            break;
        }

        // Use first index as key, and remove it from the value.
        let key = chunks.remove(0);
        headers.insert(key, chunks);
    }
    headers
}

/// Grab `key` from the headers. Fails if not found, or if the number of
/// values is not between `min_vals` and `max_vals`.
fn find_header_item<'a>(
    headers: &'a StringToStringVecMap,
    key: &str,
    min_vals: usize,
    max_vals: usize,
) -> Result<&'a [String]> {
    let Some(values) = headers.get(key) else {
        crate::bail!("'{key}' line not found");
    };

    if values.len() < min_vals || values.len() > max_vals {
        let mut os = format!(
            "Incorrect number of chunks ({}) after '{}' line, expected ",
            values.len(),
            key
        );
        if min_vals == max_vals {
            os.push_str(&min_vals.to_string());
        } else {
            os.push_str(&format!("between {min_vals} and {max_vals}"));
        }
        return Err(Error::msg(os));
    }
    Ok(values)
}

/// Crudely parse the LUTs: just grab a series of floats for `Pre{...}`,
/// `3d{...}` etc.
fn read_luts(istream: &mut IStream) -> Result<StringToFloatVecMap> {
    let mut lut_values = StringToFloatVecMap::new();
    let mut inlut = false;
    let mut lutname = String::new();

    while let Some(word) = istream.next_word() {
        if !inlut {
            if word == "{" {
                // Lone "{" is for a 3D.
                inlut = true;
                lutname = "3d".to_string();
            } else {
                // Named LUT, e.g. "Pre {".
                inlut = true;
                lutname = word.to_ascii_lowercase();

                // Ensure next word is "{".
                let nextword = istream.next_word().unwrap_or_default();
                if nextword != "{" {
                    crate::bail!(
                        "Malformed LUT - Unknown word '{word}' after LUT name '{nextword}'"
                    );
                }
            }
        } else if word == "}" {
            // End of LUT.
            inlut = false;
            lutname.clear();
        } else {
            match from_chars_f32(&word) {
                Some(v) => {
                    let values = lut_values.entry(lutname.clone()).or_default();
                    // Cap per-LUT entry count: max is Max3DLUTLength^3 * 3.
                    if values.len() > MAX_3D_LUT_LENGTH * MAX_3D_LUT_LENGTH * MAX_3D_LUT_LENGTH * 3
                    {
                        crate::bail!("Too many values in {lutname} LUT block");
                    }
                    values.push(v);
                }
                None => crate::bail!("Invalid float value in {lutname} LUT, '{word}'"),
            }
        }
    }
    Ok(lut_values)
}

/// A 1D LUT using the same values for all channels.
fn make_lut1d(values: &[f32], interp: Interpolation) -> Lut1DTransform {
    let mut lut = new_lut1d(values.len(), false, interp, BitDepth::F32);
    for (i, &v) in values.iter().enumerate() {
        lut.values[3 * i] = v;
        lut.values[3 * i + 1] = v;
        lut.values[3 * i + 2] = v;
    }
    lut
}

struct LocalFileFormat;

pub(crate) fn create() -> Box<dyn FileFormat> {
    Box::new(LocalFileFormat)
}

impl FileFormat for LocalFileFormat {
    fn format_info(&self) -> Vec<FormatInfo> {
        vec![FormatInfo {
            name: "houdini",
            extension: "lut",
            capabilities: capability::READ | capability::BAKE,
            bake_capabilities: bake_capability::LUT3D
                | bake_capability::LUT1D
                | bake_capability::LUT1D_3D,
        }]
    }

    fn read(
        &self,
        data: &[u8],
        _original_file_name: &str,
        interp: Interpolation,
    ) -> Result<CachedFile> {
        let mut istream = IStream::new(data);

        // Read headers, ending after the "LUT:" line.
        let headers = read_headers(&mut istream);

        // "Version 3" - format version (currently one version number per
        // LUT type).
        find_header_item(&headers, "version", 1, 1)?;

        // "Format any" - bit depth of image the LUT should be applied to
        // (this is basically ignored).
        find_header_item(&headers, "format", 1, 1)?;

        // "Type 3d" - type of LUT.
        let hdltype = find_header_item(&headers, "type", 1, 1)?[0].clone();

        // "From 0.0 1.0" - range of input values.
        let value = find_header_item(&headers, "from", 2, 2)?;
        let (from_min, from_max) = match (string_to_float(&value[0]), string_to_float(&value[1])) {
            (Some(a), Some(b)) => (a, b),
            _ => crate::bail!(
                "Invalid float value(s) on 'From' line, '{}' and '{}'",
                value[0],
                value[1]
            ),
        };

        // "To 0.0 1.0" - range of values in LUT (e.g "0 255" to specify
        // values as 8-bit numbers, usually "0 1").
        let value = find_header_item(&headers, "to", 2, 2)?;
        if string_to_float(&value[0]).is_none() || string_to_float(&value[1]).is_none() {
            crate::bail!(
                "Invalid float value(s) on 'To' line, '{}' and '{}'",
                value[0],
                value[1]
            );
        }

        // "Black 0" and "White 1" - obsolete options, should be 0 and 1.
        let value = find_header_item(&headers, "black", 1, 1)?;
        if string_to_float(&value[0]).is_none() {
            crate::bail!("Invalid float value on 'Black' line, '{}'", value[0]);
        }
        let value = find_header_item(&headers, "white", 1, 1)?;
        if string_to_float(&value[0]).is_none() {
            crate::bail!("Invalid float value on 'White' line, '{}'", value[0]);
        }

        // Verify type is valid and supported - used to handle length
        // sensibly, and checking the LUT later.
        if hdltype != "3d" && hdltype != "3d+1d" && hdltype != "c" {
            crate::bail!("Unsupported Houdini LUT type: '{hdltype}'");
        }

        // "Length 2" or "Length 2 5" - either "[cube size]", or "[cube size]
        // [prelut size]".
        let mut size_3d: i32 = -1;
        let mut size_prelut: i32 = -1;
        let mut size_1d: i32 = -1;
        {
            let value = find_header_item(&headers, "length", 1, 2)?;
            let mut lut_sizes = Vec::new();
            for v in value {
                match string_to_int(v, false) {
                    Some(s) => lut_sizes.push(s),
                    None => crate::bail!("Invalid integer on 'Length' line: '{}'", value[0]),
                }
            }

            if hdltype == "3d" || hdltype == "3d+1d" {
                // Set cube size.
                size_3d = lut_sizes[0];
                if size_3d < 2 || size_3d as i64 > MAX_3D_LUT_LENGTH as i64 {
                    crate::bail!("3D LUT cube size must be between 2 and {MAX_3D_LUT_LENGTH}, found: {size_3d}");
                }
            }

            if hdltype == "c" {
                size_1d = lut_sizes[0];
                if size_1d < 2 || size_1d as i64 > MAX_1D_LUT_LENGTH as i64 {
                    crate::bail!(
                        "1D LUT size must be between 2 and {MAX_1D_LUT_LENGTH}, found: {size_1d}"
                    );
                }
            }

            if hdltype == "3d+1d" {
                size_prelut = lut_sizes.get(1).copied().unwrap_or(-1);
                if size_prelut < 2 || size_prelut as i64 > MAX_1D_LUT_LENGTH as i64 {
                    crate::bail!("Prelut size must be between 2 and {MAX_1D_LUT_LENGTH}, found: {size_prelut}");
                }
            }
        }

        // Read stuff after "LUT:".
        let lut_data = read_luts(&mut istream)?;

        let mut lut1d = None;
        let mut lut3d = None;

        if hdltype == "3d+1d" {
            // Read prelut.
            let Some(values) = lut_data.get("pre") else {
                crate::bail!("3D+1D LUT should contain Pre{{}} LUT section");
            };
            if size_prelut as i64 != values.len() as i64 {
                crate::bail!(
                    "Pre{{}} LUT was {} values long, expected {} values",
                    values.len(),
                    size_prelut
                );
            }
            lut1d = Some(make_lut1d(values, interp));
        }

        if hdltype == "3d" || hdltype == "3d+1d" {
            // Bind 3D LUT, along with some slightly-elaborate error messages.
            let Some(values) = lut_data.get("3d") else {
                crate::bail!("3D LUT section not found");
            };

            let size_3d_cubed = (size_3d * size_3d * size_3d) as i64;
            if size_3d_cubed * 3 != values.len() as i64 {
                let foundsize = values.len();
                let foundlines = foundsize / 3;
                crate::bail!(
                    "3D LUT contains incorrect number of values. Contained {} values ({} lines), expected {} values ({} lines)",
                    foundsize,
                    foundlines,
                    size_3d_cubed * 3,
                    size_3d_cubed
                );
            }

            let mut lut = new_lut3d(size_3d as usize, interp, BitDepth::F32);
            set_lut3d_from_red_fastest(&mut lut, values)?;
            lut3d = Some(lut);
        }

        if hdltype == "c" {
            // Bind simple 1D RGB LUT.
            let Some(values) = lut_data.get("rgb") else {
                crate::bail!("3D+1D LUT should contain Pre{{}} LUT section");
            };
            if size_1d as i64 != values.len() as i64 {
                crate::bail!(
                    "RGB{{}} LUT was {} values long, expected {} values",
                    values.len(),
                    size_1d
                );
            }
            lut1d = Some(make_lut1d(values, interp));
        }

        // The ops: [range matrix, 1D LUT] for 'c', [3D LUT] for '3d' and
        // [range matrix, prelut, 3D LUT] for '3d+1d'.
        let mut group = GroupTransform::new();
        if let Some(lut) = lut1d {
            if let Some(m) = min_max_matrix_f32(from_min, from_max)? {
                group.append(m);
            }
            group.append(lut);
        }
        if let Some(lut) = lut3d {
            group.append(lut);
        }
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
        assert_eq!(info[0].name, "houdini");
        assert_eq!(info[0].extension, "lut");
        assert_eq!(info[0].capabilities, capability::READ | capability::BAKE);
    }

    #[test]
    fn read_1d() {
        let content = "Version\t\t1\nFormat\t\tany\nType\t\tC\nFrom\t\t0.1 3.2\nTo\t\t0 1\nBlack\t\t0\nWhite\t\t0.99\nLength\t\t9\nLUT:\nRGB {\n\t0\n\t0.000977517\n\t0.00195503\n\t0.00293255\n\t0.00391007\n\t0.00488759\n\t0.0058651\n\t0.999022\n\t1.67 }\n";
        let lut1d = [
            0.0f32,
            0.000977517,
            0.00195503,
            0.00293255,
            0.00391007,
            0.00488759,
            0.0058651,
            0.999022,
            1.67,
        ];

        let file = read(content).unwrap();
        assert_eq!(file.group.num_transforms(), 2);

        // Check the range.
        let Transform::Matrix(m) = &file.group.transforms[0] else {
            panic!("expected a matrix")
        };
        let (from_min, from_max) = (0.1f32 as f64, 3.2f32 as f64);
        assert_eq!(m.matrix[0], 1.0 / (from_max - from_min));
        assert_eq!(m.offset[0], -from_min / (from_max - from_min));

        // Check 1D data.
        let Transform::Lut1D(lut) = &file.group.transforms[1] else {
            panic!("expected a Lut1D")
        };
        assert_eq!(lut.file_output_bit_depth, BitDepth::F32);
        assert_eq!(lut.length(), 9);
        for (i, &v) in lut1d.iter().enumerate() {
            assert_eq!(lut.value(i), [v, v, v]);
        }
    }

    #[test]
    fn read_3d() {
        let content = "Version         2\nFormat      any\nType        3D\nFrom        0.2 0.9\nTo      0.001 0.999\nBlack       0.002\nWhite       0.98\nLength      2\nLUT:\n {\n 0 0 0\n 0 0 0\n 0 0.390735 2.68116e-28\n 0 0.390735 0\n 0 0 0\n 0 0 0.599397\n 0 0.601016 0\n 0 0.601016 0.917034\n }\n";
        let cube: [f32; 24] = [
            0.0,
            0.0,
            0.0, //
            0.0,
            0.0,
            0.0, //
            0.0,
            0.390735,
            2.68116e-28, //
            0.0,
            0.390735,
            0.0, //
            0.0,
            0.0,
            0.0, //
            0.0,
            0.0,
            0.599397, //
            0.0,
            0.601016,
            0.0, //
            0.0,
            0.601016,
            0.917034,
        ];

        let file = read(content).unwrap();
        // from_min & from_max are only used when there is a 1D LUT.
        assert_eq!(file.group.num_transforms(), 1);
        let Transform::Lut3D(lut) = &file.group.transforms[0] else {
            panic!("expected a Lut3D")
        };
        let n = lut.grid_size;
        assert_eq!(n, 2);
        for b in 0..n {
            for g in 0..n {
                for r in 0..n {
                    // Lut3DTransform index: blue changes fastest.
                    let array_idx = 3 * ((r * n + g) * n + b);
                    // Houdini order, red changes fastest.
                    let ocio_idx = 3 * ((b * n + g) * n + r);
                    assert_eq!(
                        lut.values[array_idx..array_idx + 3],
                        cube[ocio_idx..ocio_idx + 3]
                    );
                }
            }
        }
    }

    #[test]
    fn read_3d_1d() {
        let content = "Version         3\nFormat      any\nType        3D+1D\nFrom        0.005478 14.080103\nTo      0 1\nBlack       0\nWhite       1\nLength      2 10\nLUT:\nPre {\n    0.994922\n    0.995052\n    0.995181\n    0.995310\n    0.995439\n    0.995568\n    0.995697\n    0.995826\n    0.995954\n    0.996082\n}\n3D {\n    0.093776 0.093776 0.093776\n    0.105219 0.093776 0.093776\n    0.118058 0.093776 0.093776\n    0.132463 0.093776 0.093776\n    0.148626 0.093776 0.093776\n    0.166761 0.093776 0.093776\n    0.187109 0.093776 0.093776\n    0.209939 0.093776 0.093776\n}\n";
        let prelut = [
            0.994922f32,
            0.995052,
            0.995181,
            0.995310,
            0.995439,
            0.995568,
            0.995697,
            0.995826,
            0.995954,
            0.996082,
        ];
        let cube: [f32; 24] = [
            0.093776, 0.093776, 0.093776, //
            0.105219, 0.093776, 0.093776, //
            0.118058, 0.093776, 0.093776, //
            0.132463, 0.093776, 0.093776, //
            0.148626, 0.093776, 0.093776, //
            0.166761, 0.093776, 0.093776, //
            0.187109, 0.093776, 0.093776, //
            0.209939, 0.093776, 0.093776,
        ];

        let file = read(content).unwrap();
        assert_eq!(file.group.num_transforms(), 3);

        let Transform::Matrix(m) = &file.group.transforms[0] else {
            panic!("expected a matrix")
        };
        let (from_min, from_max) = (0.005478f32 as f64, 14.080103f32 as f64);
        assert_eq!(m.matrix[0], 1.0 / (from_max - from_min));

        let Transform::Lut1D(lut) = &file.group.transforms[1] else {
            panic!("expected a Lut1D")
        };
        assert_eq!(lut.file_output_bit_depth, BitDepth::F32);
        assert_eq!(lut.length(), 10);
        for (i, &v) in prelut.iter().enumerate() {
            assert_eq!(lut.value(i), [v, v, v]);
        }

        let Transform::Lut3D(lut) = &file.group.transforms[2] else {
            panic!("expected a Lut3D")
        };
        let n = lut.grid_size;
        assert_eq!(n, 2);
        for b in 0..n {
            for g in 0..n {
                for r in 0..n {
                    let array_idx = 3 * ((r * n + g) * n + b);
                    let ocio_idx = 3 * ((b * n + g) * n + r);
                    assert_eq!(
                        lut.values[array_idx..array_idx + 3],
                        cube[ocio_idx..ocio_idx + 3]
                    );
                }
            }
        }
    }

    #[test]
    fn read_file() {
        let path = format!(
            "{}/tests/data/files/houdini.lut",
            env!("CARGO_MANIFEST_DIR")
        );
        let data = std::fs::read(&path).unwrap();
        let file = LocalFileFormat
            .read(&data, &path, Interpolation::Default)
            .unwrap();
        assert!(file.group.num_transforms() >= 1);
    }

    #[test]
    fn read_failures() {
        let header = "Version 1\nFormat any\nType C\nFrom 0 1\nTo 0 1\nBlack 0\nWhite 1\n";
        let e = read("Version 1\n").unwrap_err();
        assert_eq!(e.message(), "'format' line not found");
        let e = read("Version 1 2\n").unwrap_err();
        assert_eq!(
            e.message(),
            "Incorrect number of chunks (2) after 'version' line, expected 1"
        );
        let e = read(&format!("{header}Length 1 2 3\nLUT:\n")).unwrap_err();
        assert_eq!(
            e.message(),
            "Incorrect number of chunks (3) after 'length' line, expected between 1 and 2"
        );
        let e = read(&header.replace("Type C", "Type RGB")).unwrap_err();
        assert_eq!(e.message(), "Unsupported Houdini LUT type: 'rgb'");
        let e = read(&header.replace("From 0 1", "From a 1")).unwrap_err();
        assert!(e
            .message()
            .starts_with("Invalid float value(s) on 'From' line"));
        let e = read(&format!("{header}Length 2\nLUT:\nRGB {{ 0 x }}\n")).unwrap_err();
        assert_eq!(e.message(), "Invalid float value in rgb LUT, 'x'");
        let e = read(&format!("{header}Length 2\nLUT:\nRGB x 0 1 }}\n")).unwrap_err();
        assert_eq!(
            e.message(),
            "Malformed LUT - Unknown word 'RGB' after LUT name 'x'"
        );
        let e = read(&format!("{header}Length 3\nLUT:\nRGB {{ 0 1 }}\n")).unwrap_err();
        assert_eq!(
            e.message(),
            "RGB{} LUT was 2 values long, expected 3 values"
        );
        let e = read(&format!("{header}Length 1\nLUT:\nRGB {{ 0 }}\n")).unwrap_err();
        assert_eq!(
            e.message(),
            "1D LUT size must be between 2 and 300000, found: 1"
        );
        let e = read(&format!(
            "{}Length 2\nLUT:\n{{ 0 1 }}\n",
            header.replace("Type C", "Type 3D")
        ))
        .unwrap_err();
        assert!(e
            .message()
            .starts_with("3D LUT contains incorrect number of values. Contained 2 values"));
        let e = read(&format!(
            "{}Length 2\nLUT:\n{{ 0 1 }}\n",
            header.replace("Type C", "Type 3D+1D")
        ))
        .unwrap_err();
        assert_eq!(
            e.message(),
            "Prelut size must be between 2 and 300000, found: -1"
        );
    }
}
