//! Iridas `itx` 3D LUT format (port of `FileFormatIridasItx.cpp`).
//!
//! ```text
//! LUT_3D_SIZE M
//! # The data is RGB, ordered in such a way that the red coordinate
//! # changes fastest, then the green coordinate, and finally, the blue
//! # coordinate changes slowest.
//! 0.0 0.0 0.0
//! 1.0 0.0 0.0
//! ...
//! ```

use super::utils::{bake_identity_lut3d, format_fixed6_rgb};
use super::utils::{
    new_lut3d, set_lut3d_from_red_fastest, split_by_white_spaces, string_to_int,
    string_vec_to_float_vec, trim, IStream, MAX_3D_LUT_LENGTH,
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
    let mut os = format!("Error parsing Iridas .itx file ({file_name}).  ");
    if line != -1 {
        os.push_str(&format!("At line ({line}): '{line_content}'.  "));
    }
    os.push_str(error);
    Error::msg(os)
}

impl FileFormat for LocalFileFormat {
    fn format_info(&self) -> Vec<FormatInfo> {
        vec![FormatInfo {
            name: "iridas_itx",
            extension: "itx",
            capabilities: capability::READ | capability::BAKE,
            bake_capabilities: bake_capability::LUT3D,
        }]
    }

    fn read(&self, data: &[u8], file_name: &str, interp: Interpolation) -> Result<CachedFile> {
        let mut istream = IStream::new(data);

        // Parse the file.
        let mut raw: Vec<f32> = Vec::new();
        let mut size3d: i32 = 0;
        let mut in3d = false;
        let mut line_number = 0;

        while let Some(line) = istream.nextline() {
            line_number += 1;

            // All lines starting with '#' are comments.
            if line.starts_with('#') {
                continue;
            }

            // Strip, lowercase, and split the line.
            let parts = split_by_white_spaces(&trim(&line).to_ascii_lowercase());
            if parts.is_empty() {
                continue;
            }

            if parts[0] == "lut_3d_size" {
                let size = match (
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
                if size < 2 || size as i64 > MAX_3D_LUT_LENGTH as i64 {
                    return Err(error_message(
                        &format!("LUT_3D_SIZE must be between 2 and {MAX_3D_LUT_LENGTH}."),
                        file_name,
                        line_number,
                        &line,
                    ));
                }
                size3d = size;
                raw.reserve(3 * (size3d * size3d * size3d) as usize);
                in3d = true;
            } else if in3d {
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
                if raw.len() > MAX_3D_LUT_LENGTH * MAX_3D_LUT_LENGTH * MAX_3D_LUT_LENGTH * 3 {
                    return Err(error_message(
                        "Too many 3D LUT entries.",
                        file_name,
                        line_number,
                        &line,
                    ));
                }
                raw.extend_from_slice(&values);
            }
        }

        // Interpret the parsed data, validate LUT sizes.
        if !in3d {
            return Err(error_message("No 3D LUT found.", file_name, -1, ""));
        }

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

        let mut group = GroupTransform::new();
        group.append(lut);
        Ok(CachedFile::new(group))
    }

    fn bake(&self, baker: &Baker, format_name: &str) -> Result<Vec<u8>> {
        const DEFAULT_CUBE_SIZE: usize = 64;

        if format_name != "iridas_itx" {
            // Note: OCIO's message says 3dl.
            crate::bail!("Unknown 3dl format name, '{format_name}'.");
        }

        // Smallest cube is 2x2x2.
        let cube_size = baker.cube_size().unwrap_or(DEFAULT_CUBE_SIZE).max(2);

        let mut cube_data = bake_identity_lut3d(cube_size, Lut3DOrder::FastRed)?;

        // Apply the conversion from the input space to the output space.
        input_to_target_processor(baker)?.apply_rgb_slice(&mut cube_data);

        // Write out the file. For maximum compatibility with other apps,
        // the shaper is not utilized and no metadata is written.
        let mut out = String::new();
        out.push_str(&format!("LUT_3D_SIZE {cube_size}\n"));

        // Fixed 6 decimal precision.
        for rgb in cube_data.as_chunks::<3>().0 {
            out.push_str(&format_fixed6_rgb(rgb));
            out.push('\n');
        }
        out.push('\n');

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

    const SAMPLE_NO_ERROR: &str = "LUT_3D_SIZE 2\n0.0 0.0 0.0\n1.0 0.0 0.0\n0.0 1.0 0.0\n1.0 1.0 0.0\n0.0 0.0 1.0\n1.0 0.0 1.0\n0.0 1.0 1.0\n1.0 1.0 1.0\n";

    #[test]
    fn format_info() {
        let info = LocalFileFormat.format_info();
        assert_eq!(info.len(), 1);
        assert_eq!(info[0].name, "iridas_itx");
        assert_eq!(info[0].extension, "itx");
        assert_eq!(info[0].capabilities, capability::READ | capability::BAKE);
    }

    #[test]
    fn read_failure() {
        assert!(read(SAMPLE_NO_ERROR).is_ok());
        // Wrong LUT_3D_SIZE tag.
        check_error(
            &SAMPLE_NO_ERROR.replace("LUT_3D_SIZE 2", "LUT_3D_SIZE 2 2"),
            "Malformed LUT_3D_SIZE tag",
        );
        // Unexpected tag.
        check_error(
            &SAMPLE_NO_ERROR.replace("LUT_3D_SIZE 2\n", "LUT_3D_SIZE 2\nWRONG_TAG\n"),
            "Malformed color triples specified",
        );
        // Wrong number of entries.
        check_error(
            &SAMPLE_NO_ERROR.replace("0.0 1.0 1.0\n", "0.0 1.0 1.0\n0.0 1.0 1.0\n0.0 1.0 1.0\n"),
            "Incorrect number of 3D LUT entries",
        );
        check_error("0.0 0.0 0.0\n", "No 3D LUT found.");
        check_error(
            &SAMPLE_NO_ERROR.replace("LUT_3D_SIZE 2", "LUT_3D_SIZE 200"),
            "LUT_3D_SIZE must be between 2 and 129.",
        );
    }

    #[test]
    fn load_3d() {
        let path = format!(
            "{}/tests/data/files/iridas_3d.itx",
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
        assert_eq!(lut.file_output_bit_depth, BitDepth::F32);
        let expected: Vec<f32> = vec![
            0.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 2.0, 0.0, 0.0, 2.0, 2.0, //
            2.0, 0.0, 0.0, 2.0, 0.0, 2.0, 2.0, 2.0, 0.0, 2.0, 2.0, 2.0,
        ];
        assert_eq!(lut.values, expected);
    }

    // Baker tests (OCIO has no bake test for this format).

    use crate::fileformats::utils::bake_test_utils::{bake, baker, check_round_trip, config_yaml};

    #[test]
    fn bake_3d() {
        let config = config_yaml(&[
            ("input", ""),
            ("target", "from_scene_reference: !<CDLTransform> {sat: 0.5}"),
        ]);
        let mut b = baker(&config, "iridas_itx");
        // The metadata is not written.
        b.format_metadata_mut()
            .add_child_element(crate::types::METADATA_DESCRIPTION, "not written");
        b.set_input_space("input");
        b.set_target_space("target");
        b.set_cube_size(Some(2));

        let expected = "LUT_3D_SIZE 2\n\
            0.000000 0.000000 0.000000\n\
            0.606300 0.106300 0.106300\n\
            0.357600 0.857600 0.357600\n\
            0.963900 0.963900 0.463900\n\
            0.036100 0.036100 0.536100\n\
            0.642400 0.142400 0.642400\n\
            0.393700 0.893700 0.893700\n\
            1.000000 1.000000 1.000000\n\
            \n";
        assert_eq!(bake(&b), expected);
    }

    #[test]
    fn bake_defaults_and_errors() {
        let config = config_yaml(&[("input", ""), ("target", "")]);
        let mut b = baker(&config, "iridas_itx");
        b.set_input_space("input");
        b.set_target_space("target");
        let out = bake(&b);
        // Default cube size of 64.
        assert!(out.starts_with("LUT_3D_SIZE 64\n"));
        assert_eq!(out.lines().count(), 1 + 64 * 64 * 64 + 1);

        let e = LocalFileFormat.bake(&b, "itx").unwrap_err();
        assert_eq!(e.message(), "Unknown 3dl format name, 'itx'.");
    }

    #[test]
    fn bake_round_trip() {
        let config = config_yaml(&[
            ("input", ""),
            (
                "target",
                "from_scene_reference: !<CDLTransform> {slope: [0.5, 0.6, 0.7], sat: 0.8}",
            ),
        ]);
        let mut b = baker(&config, "iridas_itx");
        b.set_input_space("input");
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
