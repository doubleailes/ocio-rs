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
    bake_identity_lut1d, bake_identity_lut3d, bake_linear_scale_lut1d, format_fixed6,
    format_fixed6_rgb, write_metadata_lines,
};
use super::utils::{
    min_max_matrix_f32, new_lut1d, new_lut3d, set_lut3d_from_red_fastest, split_by_white_spaces,
    string_to_float, string_to_int, string_vec_to_float_vec, trim, IStream, MAX_1D_LUT_LENGTH,
    MAX_3D_LUT_LENGTH,
};
use super::{bake_capability, capability, CachedFile, FileFormat, FormatInfo};
use crate::baker::{
    input_to_shaper_processor, input_to_target_processor, shaper_range, shaper_to_target_processor,
    Baker,
};
use crate::error::{Error, Result};
use crate::ops::lut3d::Lut3DOrder;
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

    fn bake(&self, baker: &Baker, format_name: &str) -> Result<Vec<u8>> {
        const DEFAULT_1D_SIZE: usize = 4096;
        const DEFAULT_SHAPER_SIZE: usize = 4096;
        const DEFAULT_3D_SIZE: usize = 64;

        if format_name != "resolve_cube" {
            crate::bail!("Unknown cube format name, '{format_name}'.");
        }

        // Initialize the sizes (the cube size is also the 1D LUT size).
        let oned_size = baker.cube_size().unwrap_or(DEFAULT_1D_SIZE);
        // Smallest cube is 2x2x2.
        let cube_size = baker.cube_size().unwrap_or(DEFAULT_3D_SIZE).max(2);
        let shaper_size = baker.shaper_size().unwrap_or(DEFAULT_SHAPER_SIZE);

        let shaper_space = baker.shaper_space();

        // Determine the required LUT type.
        const CUBE_1D: i32 = 1; // 1D LUT version number.
        const CUBE_3D: i32 = 2; // 3D LUT version number.
        const CUBE_1D_3D: i32 = 3; // 3D LUT with 1D prelut.

        let input_to_target = input_to_target_processor(baker)?;
        let required_lut = if input_to_target.has_channel_crosstalk() {
            if shaper_space.is_empty() {
                // Has crosstalk, but no shaper, so need 3D LUT.
                CUBE_3D
            } else {
                // Crosstalk with shaper-space.
                CUBE_1D_3D
            }
        } else {
            CUBE_1D
        };

        // Generate the shaper.
        let mut shaper_data: Vec<f32> = Vec::new();

        let mut from_in_start = 0.0f32;
        let mut from_in_end = 1.0f32;

        if required_lut == CUBE_1D_3D {
            (from_in_start, from_in_end) = shaper_range(baker)?;

            // Generate the identity shaper values, then apply the
            // transform. The shaper is linearly sampled from fromInStart to
            // fromInEnd.
            shaper_data = bake_linear_scale_lut1d(shaper_size, from_in_start, from_in_end)?;
            input_to_shaper_processor(baker)?.apply_rgb_slice(&mut shaper_data);
        }

        // Generate the 3D LUT.
        let mut cube_data: Vec<f32> = Vec::new();
        if required_lut == CUBE_3D || required_lut == CUBE_1D_3D {
            cube_data = bake_identity_lut3d(cube_size, Lut3DOrder::FastRed)?;
            if required_lut == CUBE_1D_3D {
                shaper_to_target_processor(baker)?.apply_rgb_slice(&mut cube_data);
            } else {
                // No shaper, so the cube goes from input to target.
                input_to_target.apply_rgb_slice(&mut cube_data);
            }
        }

        // Generate the 1D LUT.
        let mut oned_data: Vec<f32> = Vec::new();
        if required_lut == CUBE_1D {
            oned_data = if !shaper_space.is_empty() {
                (from_in_start, from_in_end) = shaper_range(baker)?;
                bake_linear_scale_lut1d(oned_size, from_in_start, from_in_end)?
            } else {
                bake_identity_lut1d(oned_size)?
            };
            input_to_target.apply_rgb_slice(&mut oned_data);
        }

        // Write the LUT (fixed 6 decimal precision).
        let mut out = String::new();

        // Comments.
        let metadata = baker.format_metadata();
        write_metadata_lines(&mut out, metadata, "# ");
        if !metadata.children.is_empty() {
            out.push('\n');
        }

        // Header.
        // Note about the LUT_ND_INPUT_RANGE tags: these tags are optional
        // and default to the 0..1 range, not writing them explicitly allows
        // for wider compatibility with parsers based on other cube
        // specifications (e.g. Iridas_Itx).
        let input_range = format!(
            "{} {}",
            format_fixed6(from_in_start),
            format_fixed6(from_in_end)
        );
        if required_lut == CUBE_1D {
            out.push_str(&format!("LUT_1D_SIZE {oned_size}\n"));
            if from_in_start != 0.0 || from_in_end != 1.0 {
                out.push_str(&format!("LUT_1D_INPUT_RANGE {input_range}\n"));
            }
        } else if required_lut == CUBE_1D_3D {
            out.push_str(&format!("LUT_1D_SIZE {shaper_size}\n"));
            out.push_str(&format!("LUT_1D_INPUT_RANGE {input_range}\n"));
        }
        if required_lut == CUBE_3D || required_lut == CUBE_1D_3D {
            out.push_str(&format!("LUT_3D_SIZE {cube_size}\n"));
        }

        // Write the 1D data (the shaper for a 1D + 3D LUT), then the 3D
        // data.
        let lut1d_data = if required_lut == CUBE_1D {
            &oned_data
        } else {
            &shaper_data
        };
        for rgb in lut1d_data
            .as_chunks::<3>()
            .0
            .iter()
            .chain(cube_data.as_chunks::<3>().0.iter())
        {
            out.push_str(&format_fixed6_rgb(rgb));
            out.push('\n');
        }

        Ok(out.into_bytes())
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

    // Baker tests (port of the baker parts of
    // `FileFormatResolveCube_tests.cpp`).

    use crate::fileformats::utils::bake_test_utils::{
        bake, baker, check_round_trip, compare_lines, config_yaml, SHAPER_LOG2_CONFIG,
    };

    const TARGET_SAT: (&str, &str) = ("target", "from_scene_reference: !<CDLTransform> {sat: 0.5}");

    const CUBE_SAT_2: &str = "0.000000 0.000000 0.000000\n\
        0.606300 0.106300 0.106300\n\
        0.357600 0.857600 0.357600\n\
        0.963900 0.963900 0.463900\n\
        0.036100 0.036100 0.536100\n\
        0.642400 0.142400 0.642400\n\
        0.393700 0.893700 0.893700\n\
        1.000000 1.000000 1.000000\n";

    #[test]
    fn bake_1d() {
        let config = config_yaml(&[("input", ""), ("target", "")]);
        let mut b = baker(&config, "resolve_cube");
        b.set_input_space("input");
        b.set_target_space("target");
        b.set_cube_size(Some(2));

        let expected = "LUT_1D_SIZE 2\n\
            0.000000 0.000000 0.000000\n\
            1.000000 1.000000 1.000000\n";
        assert_eq!(bake(&b), expected);
    }

    #[test]
    fn bake_1d_shaper() {
        {
            // Lin to Log.
            let mut b = baker(SHAPER_LOG2_CONFIG, "resolve_cube");
            b.set_input_space("Raw");
            b.set_target_space("Log2");
            b.set_shaper_space("Log2");
            b.set_cube_size(Some(10));

            let expected = "LUT_1D_SIZE 10\n\
                LUT_1D_INPUT_RANGE 0.001989 16.291878\n\
                0.000000 0.000000 0.000000\n\
                0.756268 0.756268 0.756268\n\
                0.833130 0.833130 0.833130\n\
                0.878107 0.878107 0.878107\n\
                0.910023 0.910023 0.910023\n\
                0.934780 0.934780 0.934780\n\
                0.955010 0.955010 0.955010\n\
                0.972114 0.972114 0.972114\n\
                0.986931 0.986931 0.986931\n\
                1.000000 1.000000 1.000000\n";
            assert_eq!(bake(&b), expected);
        }
        {
            // Log to Lin.
            let mut b = baker(SHAPER_LOG2_CONFIG, "resolve_cube");
            b.set_input_space("Log2");
            b.set_target_space("Raw");
            b.set_cube_size(Some(10));

            let expected = "LUT_1D_SIZE 10\n\
                0.001989 0.001989 0.001989\n\
                0.005413 0.005413 0.005413\n\
                0.014731 0.014731 0.014731\n\
                0.040091 0.040091 0.040091\n\
                0.109110 0.109110 0.109110\n\
                0.296951 0.296951 0.296951\n\
                0.808177 0.808177 0.808177\n\
                2.199522 2.199522 2.199522\n\
                5.986179 5.986179 5.986179\n\
                16.291878 16.291878 16.291878\n";
            compare_lines(&bake(&b), expected, 1e-5, |i| i > 0);
        }
    }

    #[test]
    fn bake_3d() {
        let config = config_yaml(&[("input", ""), TARGET_SAT]);
        let mut b = baker(&config, "resolve_cube");
        b.format_metadata_mut().add_child_element(
            crate::types::METADATA_DESCRIPTION,
            "OpenColorIO Test Line 1",
        );
        b.format_metadata_mut().add_child_element(
            crate::types::METADATA_DESCRIPTION,
            "OpenColorIO Test Line 2",
        );
        b.set_input_space("input");
        b.set_target_space("target");
        b.set_cube_size(Some(2));

        let expected = format!(
            "# OpenColorIO Test Line 1\n# OpenColorIO Test Line 2\n\nLUT_3D_SIZE 2\n{CUBE_SAT_2}"
        );
        let out = bake(&b);
        compare_lines(&out, &expected, 1e-5, |i| i > 3);
        assert_eq!(out, expected);
    }

    #[test]
    fn bake_1d_3d() {
        let config = config_yaml(&[
            ("input", ""),
            (
                "shaper",
                "to_scene_reference: !<ExponentTransform> {value: [2.2, 2.2, 2.2, 1]}",
            ),
            TARGET_SAT,
        ]);
        let mut b = baker(&config, "resolve_cube");
        b.set_input_space("input");
        b.set_shaper_space("shaper");
        b.set_target_space("target");
        b.set_shaper_size(Some(10));
        b.set_cube_size(Some(2));

        let expected = format!(
            "LUT_1D_SIZE 10\n\
             LUT_1D_INPUT_RANGE 0.000000 1.000000\n\
             LUT_3D_SIZE 2\n\
             0.000000 0.000000 0.000000\n\
             0.368344 0.368344 0.368344\n\
             0.504760 0.504760 0.504760\n\
             0.606913 0.606913 0.606913\n\
             0.691699 0.691699 0.691699\n\
             0.765539 0.765539 0.765539\n\
             0.831684 0.831684 0.831684\n\
             0.892049 0.892049 0.892049\n\
             0.947870 0.947870 0.947870\n\
             1.000000 1.000000 1.000000\n\
             {CUBE_SAT_2}"
        );
        compare_lines(&bake(&b), &expected, 1e-5, |i| i > 2);
    }

    #[test]
    fn bake_defaults_and_errors() {
        let config = config_yaml(&[
            ("input", ""),
            (
                "shaper",
                "to_scene_reference: !<ExponentTransform> {value: [2.2, 2.2, 2.2, 1]}",
            ),
            TARGET_SAT,
        ]);
        let mut b = baker(&config, "resolve_cube");
        b.set_input_space("input");
        b.set_target_space("shaper");
        let out = bake(&b);
        assert!(out.starts_with("LUT_1D_SIZE 4096\n0.000000 0.000000 0.000000\n"));
        assert_eq!(out.lines().count(), 1 + 4096);

        b.set_target_space("target");
        let out = bake(&b);
        assert!(out.starts_with("LUT_3D_SIZE 64\n"));
        assert_eq!(out.lines().count(), 1 + 64 * 64 * 64);

        b.set_shaper_space("shaper");
        b.set_cube_size(Some(3));
        let out = bake(&b);
        assert!(out.starts_with(
            "LUT_1D_SIZE 4096\nLUT_1D_INPUT_RANGE 0.000000 1.000000\nLUT_3D_SIZE 3\n"
        ));
        assert_eq!(out.lines().count(), 3 + 4096 + 27);

        let e = LocalFileFormat.bake(&b, "cube").unwrap_err();
        assert_eq!(e.message(), "Unknown cube format name, 'cube'.");
    }

    #[test]
    fn bake_round_trip() {
        let samples = [
            [0.0, 0.0, 0.0],
            [0.25, 0.5, 0.75],
            [0.9, 0.1, 0.4],
            [1.0, 1.0, 1.0],
        ];

        // 1D.
        let mut b = baker(SHAPER_LOG2_CONFIG, "resolve_cube");
        b.set_input_space("Log2");
        b.set_target_space("Raw");
        check_round_trip(&b, &samples, 1e-3);

        // 1D with an input range.
        let mut b = baker(SHAPER_LOG2_CONFIG, "resolve_cube");
        b.set_input_space("Raw");
        b.set_target_space("Log2");
        b.set_shaper_space("Log2");
        check_round_trip(&b, &[[0.18, 1.0, 10.0], [0.01, 0.5, 16.0]], 1e-3);

        // 3D and 1D + 3D.
        let config = config_yaml(&[
            ("lnf", ""),
            (
                "shaper",
                "to_scene_reference: !<ExponentTransform> {value: [2.2, 2.2, 2.2, 1]}",
            ),
            (
                "target",
                "from_scene_reference: !<CDLTransform> {slope: [0.5, 0.6, 0.7], sat: 0.8}",
            ),
        ]);
        let mut b = baker(&config, "resolve_cube");
        b.set_input_space("lnf");
        b.set_target_space("target");
        b.set_cube_size(Some(5));
        check_round_trip(&b, &samples, 1e-5);

        b.set_shaper_space("shaper");
        b.set_shaper_size(Some(256));
        b.set_cube_size(Some(33));
        check_round_trip(&b, &samples, 5e-4);
    }
}
