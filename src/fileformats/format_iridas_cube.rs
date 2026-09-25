//! Iridas `cube` LUT format (port of `FileFormatIridasCube.cpp`).
//!
//! ```text
//! TITLE "LUT name from title"
//! LUT_3D_SIZE M          (or LUT_1D_SIZE N)
//! DOMAIN_MIN 0.0 0.0 0.0 (optional)
//! DOMAIN_MAX 1.0 1.0 1.0 (optional)
//! # Data: RGB triples, red changes fastest for 3D LUTs.
//! 0.0 0.0 0.0
//! 1.0 0.0 0.0
//! ...
//! ```
//!
//! A LUT may contain a 1D or a 3D LUT but not both. The domain becomes a
//! range matrix placed before the LUT.

use super::utils::{bake_identity_lut3d, format_fixed6_rgb, write_metadata_lines};
use super::utils::{
    from_chars_f32, left_trim, min_max_matrix, new_lut1d, new_lut3d, scanf,
    set_lut3d_from_red_fastest, trim, IStream, MAX_1D_LUT_LENGTH, MAX_3D_LUT_LENGTH,
};
use super::{bake_capability, capability, CachedFile, FileFormat, FormatInfo};
use crate::baker::{input_to_target_processor, Baker};
use crate::error::{Error, Result};
use crate::ops::lut3d::Lut3DOrder;
use crate::transforms::GroupTransform;
use crate::types::{BitDepth, Interpolation};

struct LocalFileFormat;

pub(crate) fn create() -> Box<dyn FileFormat> {
    Box::new(LocalFileFormat)
}

fn error_message(error: &str, file_name: &str, line: i32, line_content: &str) -> Error {
    let mut os = format!("Error parsing Iridas .cube file ({file_name}).  ");
    if line != -1 {
        os.push_str(&format!("At line ({line}): '{line_content}'.  "));
    }
    os.push_str(error);
    Error::msg(os)
}

/// Parse the 3 float values of a `DOMAIN_MIN` / `DOMAIN_MAX` tag.
fn parse_domain(line: &str, tag: &str, file_name: &str, line_number: i32) -> Result<[f32; 3]> {
    let upper = tag.to_ascii_uppercase();
    let (n, v) = scanf(line, &format!("{tag} %63s %63s %63s %c"));
    if n != 3 {
        return Err(error_message(
            &format!("Malformed '{upper}' tag."),
            file_name,
            line_number,
            line,
        ));
    }
    match (
        from_chars_f32(v[0].as_str()),
        from_chars_f32(v[1].as_str()),
        from_chars_f32(v[2].as_str()),
    ) {
        (Some(r), Some(g), Some(b)) => Ok([r, g, b]),
        _ => Err(error_message(
            &format!("Invalid '{upper}' Tag"),
            file_name,
            line_number,
            line,
        )),
    }
}

impl FileFormat for LocalFileFormat {
    fn format_info(&self) -> Vec<FormatInfo> {
        vec![FormatInfo {
            name: "iridas_cube",
            extension: "cube",
            capabilities: capability::READ | capability::BAKE,
            bake_capabilities: bake_capability::LUT3D,
        }]
    }

    fn read(&self, data: &[u8], file_name: &str, interp: Interpolation) -> Result<CachedFile> {
        let mut istream = IStream::new(data);

        // Parse the file.
        let mut raw: Vec<f32> = Vec::new();
        let mut size3d: i32 = 0;
        let mut size1d: i32 = 0;
        let mut in1d = false;
        let mut in3d = false;
        let mut domain_min = [0.0f32; 3];
        let mut domain_max = [1.0f32; 3];

        let mut line = String::new();
        let mut line_number = 0;
        let mut entries_started = false;

        while !entries_started {
            let Some(l) = istream.nextline() else {
                line.clear();
                break;
            };
            line = l;
            line_number += 1;

            // All lines starting with '#' are comments.
            if line.starts_with('#') {
                continue;
            }

            line = trim(&line).to_ascii_lowercase();
            if line.is_empty() {
                continue;
            }

            if line.starts_with("title") {
                // Optional, and currently unhandled.
            } else if line.starts_with("lut_1d_size") {
                let (n, v) = scanf(&line, "lut_1d_size %d %c");
                if n != 1 {
                    return Err(error_message(
                        "Malformed 'LUT_1D_SIZE' tag.",
                        file_name,
                        line_number,
                        &line,
                    ));
                }
                size1d = v[0].as_int();
                if size1d < 2 || size1d as i64 > MAX_1D_LUT_LENGTH as i64 {
                    return Err(error_message(
                        &format!("'LUT_1D_SIZE' must be between 2 and {MAX_1D_LUT_LENGTH}."),
                        file_name,
                        line_number,
                        &line,
                    ));
                }
                raw.reserve(3 * size1d as usize);
                in1d = true;
            } else if line.starts_with("lut_2d_size") {
                return Err(error_message(
                    "Unsupported tag: 'LUT_2D_SIZE'.",
                    file_name,
                    line_number,
                    &line,
                ));
            } else if line.starts_with("lut_3d_size") {
                let (n, v) = scanf(&line, "lut_3d_size %d %c");
                if n != 1 {
                    return Err(error_message(
                        "Malformed 'LUT_3D_SIZE' tag.",
                        file_name,
                        line_number,
                        &line,
                    ));
                }
                size3d = v[0].as_int();
                if size3d < 2 || size3d as i64 > MAX_3D_LUT_LENGTH as i64 {
                    return Err(error_message(
                        &format!("'LUT_3D_SIZE' must be between 2 and {MAX_3D_LUT_LENGTH}."),
                        file_name,
                        line_number,
                        &line,
                    ));
                }
                raw.reserve(3 * (size3d * size3d * size3d) as usize);
                in3d = true;
            } else if line.starts_with("domain_min") {
                domain_min = parse_domain(&line, "domain_min", file_name, line_number)?;
            } else if line.starts_with("domain_max") {
                domain_max = parse_domain(&line, "domain_max", file_name, line_number)?;
            } else {
                entries_started = true;
            }
        }

        loop {
            let l = left_trim(&line).to_string();
            // All lines starting with '#' are comments.
            if !l.starts_with('#') && !l.is_empty() {
                let (n, v) = scanf(&l, "%63s %63s %63s %c");
                if n != 3 {
                    // It must be a float triple!
                    return Err(error_message(
                        "Malformed color triples specified.",
                        file_name,
                        line_number,
                        &l,
                    ));
                }
                match (
                    from_chars_f32(v[0].as_str()),
                    from_chars_f32(v[1].as_str()),
                    from_chars_f32(v[2].as_str()),
                ) {
                    (Some(r), Some(g), Some(b)) => raw.extend_from_slice(&[r, g, b]),
                    _ => {
                        return Err(error_message(
                            "Invalid color triples",
                            file_name,
                            line_number,
                            &l,
                        ))
                    }
                }
                line_number += 1;
            }
            match istream.nextline() {
                Some(next) => line = next,
                None => break,
            }
        }

        // Interpret the parsed data, validate LUT sizes.
        let mut group = GroupTransform::new();
        let dmin = domain_min.map(|v| v as f64);
        let dmax = domain_max.map(|v| v as f64);

        if in1d {
            if size1d as i64 != (raw.len() / 3) as i64 {
                return Err(error_message(
                    &format!(
                        "Incorrect number of lut1d entries. Found {}, expected {}.",
                        raw.len() / 3,
                        size1d
                    ),
                    file_name,
                    -1,
                    "",
                ));
            }

            // Reformat 1D data.
            let mut lut = new_lut1d(size1d as usize, false, interp, BitDepth::F32);
            lut.values.copy_from_slice(&raw);
            if let Some(m) = min_max_matrix(dmin, dmax)? {
                group.append(m);
            }
            group.append(lut);
        } else if in3d {
            if (size3d * size3d * size3d) as i64 != (raw.len() / 3) as i64 {
                return Err(error_message(
                    &format!(
                        "Incorrect number of 3D LUT entries. Found {}, expected {}.",
                        raw.len() / 3,
                        size3d * size3d * size3d
                    ),
                    file_name,
                    -1,
                    "",
                ));
            }

            // Reformat 3D data.
            let mut lut = new_lut3d(size3d as usize, interp, BitDepth::F32);
            set_lut3d_from_red_fastest(&mut lut, &raw)?;
            if let Some(m) = min_max_matrix(dmin, dmax)? {
                group.append(m);
            }
            group.append(lut);
        } else {
            return Err(error_message(
                "LUT type (1D/3D) unspecified.",
                file_name,
                -1,
                "",
            ));
        }

        Ok(CachedFile::new(group))
    }

    fn bake(&self, baker: &Baker, format_name: &str) -> Result<Vec<u8>> {
        const DEFAULT_CUBE_SIZE: usize = 32;

        if format_name != "iridas_cube" {
            crate::bail!("Unknown cube format name, '{format_name}'.");
        }

        // Smallest cube is 2x2x2.
        let cube_size = baker.cube_size().unwrap_or(DEFAULT_CUBE_SIZE).max(2);

        let mut cube_data = bake_identity_lut3d(cube_size, Lut3DOrder::FastRed)?;
        input_to_target_processor(baker)?.apply_rgb_slice(&mut cube_data);

        let mut out = String::new();
        let metadata = baker.format_metadata();
        write_metadata_lines(&mut out, metadata, "# ");
        if !metadata.children.is_empty() {
            out.push('\n');
        }

        out.push_str(&format!("LUT_3D_SIZE {cube_size}\n"));

        // Fixed 6 decimal precision.
        for rgb in cube_data.as_chunks::<3>().0 {
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

    fn load(name: &str) -> CachedFile {
        let path = format!("{}/tests/data/files/{}", env!("CARGO_MANIFEST_DIR"), name);
        let data = std::fs::read(&path).unwrap();
        LocalFileFormat
            .read(&data, &path, Interpolation::Default)
            .unwrap()
    }

    const SAMPLE_NO_ERROR: &str = "LUT_3D_SIZE 2\nDOMAIN_MIN 0.0 0.0 0.0\nDOMAIN_MAX 1.0 1.0 1.0\n0.0 0.0 0.0\n1.0 0.0 0.0\n0.0 1.0 0.0\n1.0 1.0 0.0\n0.0 0.0 1.0\n1.0 0.0 1.0\n0.0 1.0 1.0\n1.0 1.0 1.0\n";

    #[test]
    fn format_info() {
        let info = LocalFileFormat.format_info();
        assert_eq!(info.len(), 1);
        assert_eq!(info[0].name, "iridas_cube");
        assert_eq!(info[0].extension, "cube");
        assert_eq!(info[0].capabilities, capability::READ | capability::BAKE);
    }

    #[test]
    fn read_failure() {
        assert!(read(SAMPLE_NO_ERROR).is_ok());
        // Wrong LUT_3D_SIZE tag.
        check_error(
            &SAMPLE_NO_ERROR.replace("LUT_3D_SIZE 2", "LUT_3D_SIZE 2 2"),
            "Malformed 'LUT_3D_SIZE' tag",
        );
        // Wrong DOMAIN_MIN tag.
        check_error(
            &SAMPLE_NO_ERROR.replace("DOMAIN_MIN 0.0 0.0 0.0", "DOMAIN_MIN 0.0 0.0"),
            "Malformed 'DOMAIN_MIN' tag",
        );
        // Wrong DOMAIN_MAX tag.
        check_error(
            &SAMPLE_NO_ERROR.replace("DOMAIN_MAX 1.0 1.0 1.0", "DOMAIN_MAX 1.0 1.0 1.0 1.0"),
            "Malformed 'DOMAIN_MAX' tag",
        );
        // Unexpected tag.
        check_error(
            &SAMPLE_NO_ERROR.replace(
                "DOMAIN_MAX 1.0 1.0 1.0\n",
                "DOMAIN_MAX 1.0 1.0 1.0\nWRONG_TAG\n",
            ),
            "Malformed color triples specified",
        );
        // Wrong number of entries.
        check_error(
            &SAMPLE_NO_ERROR.replace("0.0 1.0 1.0\n", "0.0 1.0 1.0\n0.0 1.0 1.0\n0.0 1.0 1.0\n"),
            "Incorrect number of 3D LUT entries",
        );
        // Other errors.
        check_error(
            &SAMPLE_NO_ERROR.replace("LUT_3D_SIZE 2", "LUT_3D_SIZE 1"),
            "'LUT_3D_SIZE' must be between 2 and 129.",
        );
        check_error(
            &SAMPLE_NO_ERROR.replace("LUT_3D_SIZE 2", "LUT_2D_SIZE 2"),
            "Unsupported tag: 'LUT_2D_SIZE'.",
        );
        check_error("0.0 0.0 0.0\n", "LUT type (1D/3D) unspecified.");
        check_error(
            &SAMPLE_NO_ERROR.replace("DOMAIN_MIN 0.0", "DOMAIN_MIN x"),
            "Invalid 'DOMAIN_MIN' Tag",
        );
        check_error(
            &SAMPLE_NO_ERROR.replace("0.0 1.0 1.0\n1.0 1.0 1.0\n", "0.0 1.0 1.0\n1.0 1.0 x\n"),
            "Invalid color triples",
        );
        let e = read(&SAMPLE_NO_ERROR.replace("LUT_3D_SIZE 2", "LUT_3D_SIZE 2 2")).unwrap_err();
        assert_eq!(
            e.message(),
            "Error parsing Iridas .cube file (Memory File).  At line (1): 'lut_3d_size 2 2'.  Malformed 'LUT_3D_SIZE' tag."
        );
    }

    #[test]
    fn whitespace_handling() {
        let sample = "# comment\n# comment with trailing space  \n# next up various forms of empty lines\n\n   \n   \t  \n# whitespace before keywords or after data should be supported\n  LUT_3D_SIZE \t 2  \t\n\t \tDOMAIN_MIN    0.25    0.5    0.75\n\nDOMAIN_MAX\t1.5\t2.5\t3.5\n0.0 0.0 0.0\n# comments in between data should be ignored\n   1.0    0.0 \t 0.0\n0.0 1.0 0.0    \n     1.0 1.0 0.0\n    \n0.0 0.0 1.0\n1.0 0.0 1.0\n0.0 1.0 1.0\n1.0 1.0 1.0\n   \n\n";
        let file = read(sample).unwrap();
        assert_eq!(file.group.num_transforms(), 2);
        let Transform::Matrix(m) = &file.group.transforms[0] else {
            panic!("expected a matrix")
        };
        // domain [0.25, 1.5], [0.5, 2.5], [0.75, 3.5].
        assert_eq!(m.matrix[0], 1.0 / 1.25);
        assert_eq!(m.matrix[5], 1.0 / 2.0);
        assert_eq!(m.matrix[10], 1.0 / 2.75);
        assert_eq!(m.offset[0], -0.25 / 1.25);
        let Transform::Lut3D(lut) = &file.group.transforms[1] else {
            panic!("expected a Lut3D")
        };
        assert_eq!(lut.grid_size, 2);
    }

    #[test]
    fn load_1d() {
        let file = load("iridas_1d.cube");
        assert_eq!(file.group.num_transforms(), 2);
        let Transform::Matrix(m) = &file.group.transforms[0] else {
            panic!("expected a matrix")
        };
        let mut expected = [0.0f64; 16];
        expected[0] = 0.25;
        expected[5] = 1.0;
        expected[10] = 1.0;
        expected[15] = 1.0;
        assert_eq!(m.matrix, expected);
        assert_eq!(m.offset, [0.5, -1.0, 0.0, 0.0]);

        let Transform::Lut1D(lut) = &file.group.transforms[1] else {
            panic!("expected a Lut1D")
        };
        assert_eq!(lut.file_output_bit_depth, BitDepth::F32);
        assert_eq!(
            lut.values,
            vec![-1.0, -2.0, -3.0, 0.0, 0.1, 0.2, 0.4, 0.5, 0.6, 0.8, 0.9, 1.0, 1.0, 2.1, 3.2]
        );
    }

    #[test]
    fn load_3d() {
        let file = load("iridas_3d.cube");
        assert_eq!(file.group.num_transforms(), 2);
        let Transform::Matrix(m) = &file.group.transforms[0] else {
            panic!("expected a matrix")
        };
        let mut expected = [0.0f64; 16];
        expected[0] = 0.5;
        expected[5] = 1.0;
        expected[10] = 1.0;
        expected[15] = 1.0;
        assert_eq!(m.matrix, expected);
        assert_eq!(m.offset, [0.0, -1.0, 0.0, 0.0]);

        let Transform::Lut3D(lut) = &file.group.transforms[1] else {
            panic!("expected a Lut3D")
        };
        assert_eq!(lut.file_output_bit_depth, BitDepth::F32);
        let expected: Vec<f32> = vec![
            0.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 2.0, 0.0, 0.0, 2.0, 2.0, //
            2.0, 0.0, 0.0, 2.0, 0.0, 2.0, 2.0, 2.0, 0.0, 2.0, 2.0, 2.0,
        ];
        assert_eq!(lut.values, expected);
    }

    // Baker tests (port of the baker parts of
    // `FileFormatIridasCube_tests.cpp`).

    use crate::fileformats::utils::bake_test_utils::{bake, baker, check_round_trip, config_yaml};

    #[test]
    fn no_shaper() {
        let config = config_yaml(&[("lnf", ""), ("target", "")]);
        let mut b = baker(&config, "iridas_cube");
        b.format_metadata_mut().add_child_element(
            crate::types::METADATA_DESCRIPTION,
            "Alexa conversion LUT, logc2video. Full in/full out.",
        );
        b.format_metadata_mut().add_child_element(
            crate::types::METADATA_DESCRIPTION,
            "created by alexalutconv (2.11)",
        );
        b.set_input_space("lnf");
        b.set_target_space("target");
        b.set_cube_size(Some(2));

        let expected = "# Alexa conversion LUT, logc2video. Full in/full out.\n\
            # created by alexalutconv (2.11)\n\
            \n\
            LUT_3D_SIZE 2\n\
            0.000000 0.000000 0.000000\n\
            1.000000 0.000000 0.000000\n\
            0.000000 1.000000 0.000000\n\
            1.000000 1.000000 0.000000\n\
            0.000000 0.000000 1.000000\n\
            1.000000 0.000000 1.000000\n\
            0.000000 1.000000 1.000000\n\
            1.000000 1.000000 1.000000\n";
        assert_eq!(bake(&b), expected);
    }

    #[test]
    fn bake_defaults_and_errors() {
        let config = config_yaml(&[("lnf", ""), ("target", "")]);
        let mut b = baker(&config, "iridas_cube");
        b.set_input_space("lnf");
        b.set_target_space("target");
        let out = bake(&b);
        // No metadata, default cube size of 32.
        assert!(out.starts_with("LUT_3D_SIZE 32\n0.000000 0.000000 0.000000\n"));
        assert_eq!(out.lines().count(), 1 + 32 * 32 * 32);

        let e = LocalFileFormat.bake(&b, "cube").unwrap_err();
        assert_eq!(e.message(), "Unknown cube format name, 'cube'.");
    }

    #[test]
    fn bake_round_trip() {
        let config = config_yaml(&[
            ("lnf", ""),
            (
                "target",
                "from_scene_reference: !<CDLTransform> {slope: [0.5, 0.6, 0.7], sat: 0.8}",
            ),
        ]);
        let mut b = baker(&config, "iridas_cube");
        b.format_metadata_mut()
            .add_child_element(crate::types::METADATA_DESCRIPTION, "Round trip");
        b.set_input_space("lnf");
        b.set_target_space("target");
        b.set_cube_size(Some(5));
        let samples = [
            [0.0, 0.0, 0.0],
            [0.25, 0.5, 0.75],
            [0.9, 0.1, 0.4],
            [1.0, 1.0, 1.0],
        ];
        check_round_trip(&b, &samples, 1e-5);
    }
}
