//! DaVinci Resolve `cube` LUT format (port of `FileFormatResolveCube.cpp`).
//!
//! A Resolve cube file may contain a 3D LUT, a 1D LUT or both (the 1D LUT
//! then being a "shaper" applied before the 3D LUT):
//!
//! ```text
//! # Comments are only allowed before the header.
//! LUT_1D_SIZE 6
//! LUT_1D_INPUT_RANGE 0.0 1.0   (optional)
//! LUT_3D_SIZE 3
//! LUT_3D_INPUT_RANGE 0.0 1.0   (optional)
//! <LUT_1D_SIZE lines of 1D data>
//! <LUT_3D_SIZE^3 lines of 3D data, red fastest>
//! ```
//!
//! Each input range becomes a range matrix placed before its LUT.

use super::utils::{
    min_max_matrix_f32, new_lut1d, new_lut3d, set_lut3d_from_red_fastest, split_by_white_spaces,
    string_to_float, string_to_int, string_vec_to_float_vec, trim, IStream, MAX_1D_LUT_LENGTH,
    MAX_3D_LUT_LENGTH,
};
use super::{bake_capability, capability, CachedFile, FileFormat, FormatInfo};
use crate::error::{Error, Result};
use crate::transforms::GroupTransform;
use crate::types::{BitDepth, Interpolation};

struct LocalFileFormat;

pub(crate) fn create() -> Box<dyn FileFormat> {
    Box::new(LocalFileFormat)
}

fn error_message(error: &str, file_name: &str, line: i32, line_content: &str) -> Error {
    let mut os = format!("Error parsing Resolve .cube file ({file_name}).  ");
    if line != -1 {
        os.push_str(&format!("At line ({line}): '{line_content}'.  "));
    }
    os.push_str(error);
    Error::msg(os)
}

impl FileFormat for LocalFileFormat {
    fn format_info(&self) -> Vec<FormatInfo> {
        vec![FormatInfo {
            name: "resolve_cube",
            extension: "cube",
            capabilities: capability::READ | capability::BAKE,
            bake_capabilities: bake_capability::LUT3D
                | bake_capability::LUT1D
                | bake_capability::LUT1D_3D,
        }]
    }

    fn read(&self, data: &[u8], file_name: &str, interp: Interpolation) -> Result<CachedFile> {
        let mut istream = IStream::new(data);

        // Parse the file.
        let mut raw1d: Vec<f32> = Vec::new();
        let mut raw3d: Vec<f32> = Vec::new();
        let mut size3d: i32 = 0;
        let mut size1d: i32 = 0;
        let mut has1d = false;
        let mut has3d = false;
        let mut range1d_min = 0.0f32;
        let mut range1d_max = 1.0f32;
        let mut range3d_min = 0.0f32;
        let mut range3d_max = 1.0f32;

        let mut line_number = 0;
        let mut header_complete = false;
        let mut triplet_number: i32 = 0;

        let malformed_range = |parts: &[String]| -> Option<(f32, f32)> {
            if parts.len() != 3 {
                return None;
            }
            Some((string_to_float(&parts[1])?, string_to_float(&parts[2])?))
        };

        while let Some(line) = istream.nextline() {
            line_number += 1;

            // All lines starting with '#' are comments.
            if line.starts_with('#') {
                if header_complete {
                    return Err(error_message(
                        "Comments not allowed after header.",
                        file_name,
                        line_number,
                        &line,
                    ));
                }
                continue;
            }

            // Strip, lowercase, and split the line.
            let parts = split_by_white_spaces(&trim(&line).to_ascii_lowercase());
            if parts.is_empty() {
                continue;
            }

            match parts[0].as_str() {
                "title" => {
                    return Err(error_message(
                        "Unsupported tag: 'TITLE'.",
                        file_name,
                        line_number,
                        &line,
                    ));
                }
                "lut_1d_size" => {
                    size1d = match (
                        parts.len(),
                        parts.get(1).and_then(|p| string_to_int(p, false)),
                    ) {
                        (2, Some(s)) => s,
                        _ => {
                            return Err(error_message(
                                "Malformed LUT_1D_SIZE tag.",
                                file_name,
                                line_number,
                                &line,
                            ))
                        }
                    };
                    if size1d < 2 || size1d as i64 > MAX_1D_LUT_LENGTH as i64 {
                        return Err(error_message(
                            &format!("LUT_1D_SIZE must be between 2 and {MAX_1D_LUT_LENGTH}."),
                            file_name,
                            line_number,
                            &line,
                        ));
                    }
                    raw1d.reserve(3 * size1d as usize);
                    has1d = true;
                }
                "lut_2d_size" => {
                    return Err(error_message(
                        "Unsupported tag: 'LUT_2D_SIZE'.",
                        file_name,
                        line_number,
                        &line,
                    ));
                }
                "lut_3d_size" => {
                    size3d = match (
                        parts.len(),
                        parts.get(1).and_then(|p| string_to_int(p, false)),
                    ) {
                        (2, Some(s)) => s,
                        _ => {
                            return Err(error_message(
                                "Malformed LUT_3D_SIZE tag.",
                                file_name,
                                line_number,
                                &line,
                            ))
                        }
                    };
                    if size3d < 2 || size3d as i64 > MAX_3D_LUT_LENGTH as i64 {
                        return Err(error_message(
                            &format!("LUT_3D_SIZE must be between 2 and {MAX_3D_LUT_LENGTH}."),
                            file_name,
                            line_number,
                            &line,
                        ));
                    }
                    raw3d.reserve(3 * (size3d * size3d * size3d) as usize);
                    has3d = true;
                }
                "lut_1d_input_range" => match malformed_range(&parts) {
                    Some((mn, mx)) => {
                        range1d_min = mn;
                        range1d_max = mx;
                    }
                    None => {
                        return Err(error_message(
                            "Malformed LUT_1D_INPUT_RANGE tag.",
                            file_name,
                            line_number,
                            &line,
                        ))
                    }
                },
                "lut_3d_input_range" => match malformed_range(&parts) {
                    Some((mn, mx)) => {
                        range3d_min = mn;
                        range3d_max = mx;
                    }
                    None => {
                        return Err(error_message(
                            "Malformed LUT_3D_INPUT_RANGE tag.",
                            file_name,
                            line_number,
                            &line,
                        ))
                    }
                },
                _ => {
                    header_complete = true;

                    // It must be a float triple!
                    let values = match string_vec_to_float_vec(&parts) {
                        Some(v) if v.len() == 3 => v,
                        _ => {
                            return Err(error_message(
                                "Malformed color triples specified.",
                                file_name,
                                line_number,
                                &line,
                            ))
                        }
                    };

                    for &v in &values {
                        if has1d && triplet_number < size1d {
                            raw1d.push(v);
                        } else {
                            if raw3d.len()
                                > MAX_3D_LUT_LENGTH * MAX_3D_LUT_LENGTH * MAX_3D_LUT_LENGTH * 3
                            {
                                return Err(error_message(
                                    "Too many 3D LUT entries.",
                                    file_name,
                                    line_number,
                                    &line,
                                ));
                            }
                            raw3d.push(v);
                        }
                    }
                    triplet_number += 1;
                }
            }
        }

        // Interpret the parsed data, validate LUT sizes.
        let mut group = GroupTransform::new();

        if has1d {
            if size1d as i64 != (raw1d.len() / 3) as i64 {
                return Err(error_message(
                    &format!(
                        "Incorrect number of lut1d entries. Found {}, expected {}.",
                        raw1d.len() / 3,
                        size1d
                    ),
                    file_name,
                    -1,
                    "",
                ));
            }

            // Reformat 1D data.
            let mut lut = new_lut1d(size1d as usize, false, interp, BitDepth::F32);
            lut.values.copy_from_slice(&raw1d);
            if let Some(m) = min_max_matrix_f32(range1d_min, range1d_max)? {
                group.append(m);
            }
            group.append(lut);
        }

        if has3d {
            if (size3d * size3d * size3d) as i64 != (raw3d.len() / 3) as i64 {
                return Err(error_message(
                    &format!(
                        "Incorrect number of lut3d entries. Found {}, expected {}.",
                        raw3d.len() / 3,
                        size3d * size3d * size3d
                    ),
                    file_name,
                    -1,
                    "",
                ));
            }

            // Reformat 3D data.
            let mut lut = new_lut3d(size3d as usize, interp, BitDepth::F32);
            set_lut3d_from_red_fastest(&mut lut, &raw3d)?;
            if let Some(m) = min_max_matrix_f32(range3d_min, range3d_max)? {
                group.append(m);
            }
            group.append(lut);
        }

        if !has1d && !has3d {
            return Err(error_message(
                "Lut type (1D/3D) unspecified.",
                file_name,
                -1,
                "",
            ));
        }

        Ok(CachedFile::new(group))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transforms::Transform;

    fn read(content: &str) -> Result<CachedFile> {
        LocalFileFormat.read(content.as_bytes(), "Memory File", Interpolation::Default)
    }

    const CUBE_2: &str =
        "0.0 0.0 0.0\n1.0 0.0 0.0\n0.0 1.0 0.0\n1.0 1.0 0.0\n0.0 0.0 1.0\n1.0 0.0 1.0\n0.0 1.0 1.0\n1.0 1.0 1.0\n";

    #[test]
    fn format_info() {
        let info = LocalFileFormat.format_info();
        assert_eq!(info.len(), 1);
        assert_eq!(info[0].name, "resolve_cube");
        assert_eq!(info[0].extension, "cube");
        assert_eq!(info[0].capabilities, capability::READ | capability::BAKE);
    }

    #[test]
    fn read_1d() {
        let file =
            read("LUT_1D_SIZE 2\nLUT_1D_INPUT_RANGE 0.0 1.0\n0.0 0.0 0.0\n1.0 0.0 0.0\n").unwrap();
        assert_eq!(file.group.num_transforms(), 1);
        assert!(matches!(file.group.transforms[0], Transform::Lut1D(_)));
    }

    #[test]
    fn read_3d() {
        let file = read(&format!(
            "LUT_3D_SIZE 2\nLUT_3D_INPUT_RANGE 0.0 1.0\n{CUBE_2}"
        ))
        .unwrap();
        assert_eq!(file.group.num_transforms(), 1);
        assert!(matches!(file.group.transforms[0], Transform::Lut3D(_)));
    }

    #[test]
    fn read_1d_3d() {
        let content = "LUT_1D_SIZE 6\nLUT_1D_INPUT_RANGE 0.0 1.0\nLUT_3D_SIZE 3\nLUT_3D_INPUT_RANGE 0.0 1.0\n1.0 1.0 1.0\n0.8 0.8 0.8\n0.6 0.6 0.6\n0.4 0.4 0.4\n0.2 0.2 0.2\n0.0 0.0 0.0\n1.0 1.0 1.0\n0.5 1.0 1.0\n0.0 1.0 1.0\n1.0 0.5 1.0\n0.5 0.5 1.0\n0.0 0.5 1.0\n1.0 0.0 1.0\n0.5 0.0 1.0\n0.0 0.0 1.0\n1.0 1.0 0.5\n0.5 1.0 0.5\n0.0 1.0 0.5\n1.0 0.5 0.5\n0.5 0.5 0.5\n0.0 0.5 0.5\n1.0 0.0 0.5\n0.5 0.0 0.5\n0.0 0.0 0.5\n1.0 1.0 0.0\n0.5 1.0 0.0\n0.0 1.0 0.0\n1.0 0.5 0.0\n0.5 0.5 0.0\n0.0 0.5 0.0\n1.0 0.0 0.0\n0.5 0.0 0.0\n0.0 0.0 0.0\n";
        let file = read(content).unwrap();
        assert_eq!(file.group.num_transforms(), 2);
        let Transform::Lut1D(lut1) = &file.group.transforms[0] else {
            panic!()
        };
        assert_eq!(lut1.length(), 6);
        assert_eq!(lut1.value(1), [0.8, 0.8, 0.8]);
        let Transform::Lut3D(lut3) = &file.group.transforms[1] else {
            panic!()
        };
        assert_eq!(lut3.grid_size, 3);
        assert_eq!(lut3.value(0, 0, 0), [1.0, 1.0, 1.0]);
        assert_eq!(lut3.value(1, 0, 0), [0.5, 1.0, 1.0]);
    }

    #[test]
    fn read_default_range() {
        assert!(read("LUT_1D_SIZE 2\n0.0 0.0 0.0\n1.0 0.0 0.0\n").is_ok());
        assert!(read(&format!("LUT_3D_SIZE 2\n{CUBE_2}")).is_ok());
        let file = read(&format!(
            "LUT_1D_SIZE 2\nLUT_3D_SIZE 2\n0.0 0.0 0.0\n1.0 1.0 1.0\n{CUBE_2}"
        ))
        .unwrap();
        assert_eq!(file.group.num_transforms(), 2);
    }

    #[test]
    fn read_failure() {
        // Wrong LUT_3D_SIZE tag.
        assert!(read(&format!(
            "LUT_3D_SIZE 2 2\nLUT_3D_INPUT_RANGE 0.0 1.0\n{CUBE_2}"
        ))
        .is_err());
        // Wrong LUT_3D_INPUT_RANGE tag.
        let e = read(&format!(
            "LUT_3D_SIZE 2\nLUT_3D_INPUT_RANGE 0.0 1.0 2.0\n{CUBE_2}"
        ))
        .unwrap_err();
        assert!(e.message().contains("Malformed LUT_3D_INPUT_RANGE tag."));
        // Comment after header.
        let e = read(&format!(
            "LUT_3D_SIZE 2\n0.0 0.0 0.0\n# Malformed comment\n{CUBE_2}"
        ))
        .unwrap_err();
        assert!(e.message().contains("Comments not allowed after header."));
        // Unexpected tag.
        let e = read(&format!("LUT_3D_SIZE 2\nWRONG_TAG\n{CUBE_2}")).unwrap_err();
        assert!(e.message().contains("Malformed color triples specified."));
        // Wrong number of entries.
        let e = read(&format!(
            "LUT_3D_SIZE 2\n0.0 1.0 1.0\n0.0 1.0 1.0\n{CUBE_2}"
        ))
        .unwrap_err();
        assert!(e
            .message()
            .contains("Incorrect number of lut3d entries. Found 10, expected 8."));
        let e = read("TITLE \"x\"\n").unwrap_err();
        assert!(e.message().contains("Unsupported tag: 'TITLE'."));
        let e = read("0 0 0\n").unwrap_err();
        assert!(e.message().contains("Lut type (1D/3D) unspecified."));
    }

    #[test]
    fn load_ops() {
        let path = format!(
            "{}/tests/data/files/resolve_1d3d.cube",
            env!("CARGO_MANIFEST_DIR")
        );
        let data = std::fs::read(&path).unwrap();
        let file = LocalFileFormat
            .read(&data, &path, Interpolation::Default)
            .unwrap();
        assert_eq!(file.group.num_transforms(), 4);

        let mut expected_m = [0.0f64; 16];
        expected_m[0] = 0.25;
        expected_m[5] = 0.25;
        expected_m[10] = 0.25;
        expected_m[15] = 1.0;

        let Transform::Matrix(m) = &file.group.transforms[0] else {
            panic!("expected a matrix")
        };
        assert_eq!(m.matrix, expected_m);
        assert_eq!(m.offset, [0.25, 0.25, 0.25, 0.0]);

        let Transform::Lut1D(lut) = &file.group.transforms[1] else {
            panic!("expected a Lut1D")
        };
        assert_eq!(lut.file_output_bit_depth, BitDepth::F32);
        assert_eq!(
            lut.values,
            vec![
                3.3, 3.4, 3.5, 3.0, 3.1, 3.2, 2.2, 2.3, 2.4, 2.1, 2.0, 2.0, 1.0, 1.0, 1.0, 0.0,
                0.0, 0.0
            ]
        );

        let Transform::Matrix(m) = &file.group.transforms[2] else {
            panic!("expected a matrix")
        };
        assert_eq!(m.matrix, expected_m);
        assert_eq!(m.offset, [0.0, 0.0, 0.0, 0.0]);

        let Transform::Lut3D(lut) = &file.group.transforms[3] else {
            panic!("expected a Lut3D")
        };
        assert_eq!(lut.file_output_bit_depth, BitDepth::F32);
        assert_eq!(lut.values.len(), 81);
        // File line 11 - R:0 - G:0 - B:0.
        assert_eq!(lut.values[0..3], [1.1, 1.1, 1.1]);
        // File line 23 - R:0 - G:1 - B:1.
        assert_eq!(lut.values[12..15], [1.0, 0.5, 0.5]);
        // File line 31 - R:2 - G:0 - B:2.
        assert_eq!(lut.values[60..63], [0.0, 1.0, 0.0]);
    }
}
