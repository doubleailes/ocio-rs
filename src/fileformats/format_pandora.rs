//! Pandora `mga` / `m3d` 3D LUT format (port of `FileFormatPandora.cpp`).
//!
//! ```text
//! channel 3d
//! in 35937
//! out 4096
//! format lut
//! values red green blue
//! 0 0 0 0
//! 1 0 0 128
//! ...
//! ```
//!
//! The entries are stored blue fastest, as integers scaled by `out - 1`.

use super::utils::{
    bitdepth_from_max_value, get_3d_lut_edge_len_from_num_pixels, new_lut3d, split_by_white_spaces,
    string_to_int, string_vec_to_int_vec, trim, IStream, MAX_3D_LUT_LENGTH,
};
use super::{bake_capability, capability, CachedFile, FileFormat, FormatInfo};
use crate::error::{Error, Result};
use crate::transforms::GroupTransform;
use crate::types::Interpolation;

struct LocalFileFormat;

pub(crate) fn create() -> Box<dyn FileFormat> {
    Box::new(LocalFileFormat)
}

fn error_message(error: &str, file_name: &str, line: i32, line_content: &str) -> Error {
    let mut os = format!("Error parsing Pandora LUT file ({file_name}).  ");
    if line != -1 {
        os.push_str(&format!("At line ({line}): '{line_content}'.  "));
    }
    os.push_str(error);
    Error::msg(os)
}

impl FileFormat for LocalFileFormat {
    fn format_info(&self) -> Vec<FormatInfo> {
        vec![
            FormatInfo {
                name: "pandora_mga",
                extension: "mga",
                capabilities: capability::READ,
                bake_capabilities: bake_capability::NONE,
            },
            FormatInfo {
                name: "pandora_m3d",
                extension: "m3d",
                capabilities: capability::READ,
                bake_capabilities: bake_capability::NONE,
            },
        ]
    }

    fn read(&self, data: &[u8], file_name: &str, interp: Interpolation) -> Result<CachedFile> {
        let mut istream = IStream::new(data);

        // Parse the file.
        let mut lut_edge_len: usize = 0;
        let mut output_bit_depth_max_value: i32 = 0;
        let mut raw3d: Vec<i32> = Vec::new();
        let mut in_lut3d = false;
        let mut line_number = 0;
        let max_entries = MAX_3D_LUT_LENGTH * MAX_3D_LUT_LENGTH * MAX_3D_LUT_LENGTH;

        while let Some(line) = istream.nextline() {
            line_number += 1;

            // Strip, lowercase, and split the line.
            let parts = split_by_white_spaces(&trim(&line).to_ascii_lowercase());
            if parts.is_empty() {
                continue;
            }

            // Skip all lines starting with '#'.
            if parts[0].starts_with('#') {
                continue;
            }

            match parts[0].as_str() {
                "channel" => {
                    if parts.len() != 2 || parts[1] != "3d" {
                        return Err(error_message(
                            "Only 3D LUTs are currently supported (channel: 3d).",
                            file_name,
                            line_number,
                            &line,
                        ));
                    }
                }
                "in" => {
                    let inval = match (
                        parts.len(),
                        parts.get(1).and_then(|p| string_to_int(p, false)),
                    ) {
                        (2, Some(v)) => v,
                        _ => {
                            return Err(error_message(
                                "Malformed 'in' tag.",
                                file_name,
                                line_number,
                                &line,
                            ))
                        }
                    };
                    if inval < 8 || inval as i64 > max_entries as i64 {
                        return Err(error_message(
                            &format!(
                                "'in' value must be between 8 and {max_entries} ({MAX_3D_LUT_LENGTH}^3)."
                            ),
                            file_name,
                            line_number,
                            &line,
                        ));
                    }
                    raw3d.reserve(inval as usize * 3);
                    lut_edge_len = get_3d_lut_edge_len_from_num_pixels(inval as i64)?;
                }
                "out" => match (
                    parts.len(),
                    parts.get(1).and_then(|p| string_to_int(p, false)),
                ) {
                    (2, Some(v)) => output_bit_depth_max_value = v,
                    _ => {
                        return Err(error_message(
                            "Malformed 'out' tag.",
                            file_name,
                            line_number,
                            &line,
                        ))
                    }
                },
                "format" => {
                    if parts.len() != 2 || parts[1] != "lut" {
                        return Err(error_message(
                            "Only LUTs are currently supported (format: lut).",
                            file_name,
                            line_number,
                            &line,
                        ));
                    }
                }
                "values" => {
                    if parts.len() != 4
                        || parts[1] != "red"
                        || parts[2] != "green"
                        || parts[3] != "blue"
                    {
                        return Err(error_message(
                            "Only rgb LUTs are currently supported (values: red green blue).",
                            file_name,
                            line_number,
                            &line,
                        ));
                    }
                    in_lut3d = true;
                }
                _ if in_lut3d => {
                    let tmpints = match string_vec_to_int_vec(&parts) {
                        Some(v) if v.len() == 4 => v,
                        _ => {
                            return Err(error_message(
                                "Expected to find 4 integers.",
                                file_name,
                                line_number,
                                &line,
                            ))
                        }
                    };
                    if raw3d.len() > max_entries * 3 {
                        return Err(error_message(
                            "Too many 3D LUT entries.",
                            file_name,
                            line_number,
                            &line,
                        ));
                    }
                    raw3d.extend_from_slice(&tmpints[1..4]);
                }
                _ => {}
            }
        }

        // Interpret the parsed data, validate LUT sizes.
        let expected = lut_edge_len * lut_edge_len * lut_edge_len;
        if expected != raw3d.len() / 3 {
            return Err(error_message(
                &format!(
                    "Incorrect number of 3D LUT entries. Found {}, expected {}.",
                    raw3d.len() / 3,
                    expected
                ),
                file_name,
                -1,
                "",
            ));
        }

        if expected == 0 {
            return Err(error_message("No 3D LUT entries found.", file_name, -1, ""));
        }

        if output_bit_depth_max_value <= 0 {
            return Err(error_message(
                "A valid 'out' tag was not found.",
                file_name,
                -1,
                "",
            ));
        }

        // Copy the raw data into the LUT.
        let file_bd = bitdepth_from_max_value(output_bit_depth_max_value as u32);
        let mut lut = new_lut3d(lut_edge_len, interp, file_bd);

        // The LUT and the file are blue fastest.
        let scale = 1.0f32 / (output_bit_depth_max_value as f32 - 1.0);
        for (dst, &v) in lut.values.iter_mut().zip(raw3d.iter()) {
            *dst = v as f32 * scale;
        }

        let mut group = GroupTransform::new();
        group.append(lut);
        Ok(CachedFile::new(group))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transforms::Transform;
    use crate::types::BitDepth;

    fn read(content: &str) -> Result<CachedFile> {
        LocalFileFormat.read(content.as_bytes(), "Memory File", Interpolation::Default)
    }

    fn check_error(content: &str, what: &str) {
        match read(content) {
            Ok(_) => panic!("expected an error containing '{what}'"),
            Err(e) => assert!(
                e.message().contains(what),
                "'{}' does not contain '{}'",
                e.message(),
                what
            ),
        }
    }

    const SAMPLE_NO_ERROR: &str = "channel 3d\nin 8\nout 256\nformat lut\nvalues red green blue\n0 0     0   0\n1 0     0 255\n2 0   255   0\n3 0   255 255\n4 255   0   0\n5 255   0 255\n6 255 255   0\n7 255 255 255\n";

    #[test]
    fn format_info() {
        let info = LocalFileFormat.format_info();
        assert_eq!(info.len(), 2);
        assert_eq!(info[0].name, "pandora_mga");
        assert_eq!(info[0].extension, "mga");
        assert_eq!(info[0].capabilities, capability::READ);
        assert_eq!(info[1].name, "pandora_m3d");
        assert_eq!(info[1].extension, "m3d");
        assert_eq!(info[1].capabilities, capability::READ);
    }

    #[test]
    fn read_failure() {
        assert!(read(SAMPLE_NO_ERROR).is_ok());
        // Wrong channel tag.
        check_error(
            &SAMPLE_NO_ERROR.replace("channel 3d", "channel 2d"),
            "Only 3D LUTs are currently supported",
        );
        // No value spec (LUT will not be read).
        check_error(
            &SAMPLE_NO_ERROR.replace("values red green blue\n", ""),
            "Incorrect number of 3D LUT entries",
        );
        // Wrong entry.
        check_error(
            &SAMPLE_NO_ERROR.replace("4 255   0   0", "4 WRONG 255   0   0"),
            "Expected to find 4 integers",
        );
        // Wrong number of entries.
        check_error(
            &SAMPLE_NO_ERROR.replace("7 255 255 255\n", "7 255 255   0\n8 255 255 255\n"),
            "Incorrect number of 3D LUT entries",
        );
        // Other errors.
        check_error(
            &SAMPLE_NO_ERROR.replace("in 8", "in 9"),
            "Cannot infer 3D LUT size",
        );
        check_error(
            &SAMPLE_NO_ERROR.replace("in 8", "in 4"),
            "'in' value must be between 8 and 2146689 (129^3).",
        );
        check_error(
            &SAMPLE_NO_ERROR.replace("out 256\n", ""),
            "A valid 'out' tag was not found.",
        );
        check_error(
            &SAMPLE_NO_ERROR.replace("format lut", "format x"),
            "Only LUTs are currently supported",
        );
    }

    #[test]
    fn load() {
        let path = format!(
            "{}/tests/data/files/pandora_3d.m3d",
            env!("CARGO_MANIFEST_DIR")
        );
        let data = std::fs::read(&path).unwrap();
        let file = LocalFileFormat
            .read(&data, &path, Interpolation::Default)
            .unwrap();
        assert_eq!(file.group.num_transforms(), 1);
        let Transform::Lut3D(lut) = &file.group.transforms[0] else {
            panic!("expected a Lut3D")
        };
        assert_eq!(lut.file_output_bit_depth, BitDepth::UInt8);
        let expected: [f32; 24] = [
            0.0, 0.0, 0.0, 0.0, 0.0, 0.8, 0.0, 0.8, 0.0, 0.0, 0.8, 0.8, //
            1.0, 0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 1.0, 0.0, 1.2, 1.0, 1.2,
        ];
        assert_eq!(lut.values.len(), 24);
        for (a, b) in lut.values.iter().zip(expected.iter()) {
            assert!((a - b).abs() <= 1e-7, "{a} {b}");
        }
    }
}
