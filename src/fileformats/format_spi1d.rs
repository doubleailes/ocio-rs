//! Sony Pictures Imageworks `spi1d` 1D LUT format (port of
//! `FileFormatSpi1D.cpp`).
//!
//! ```text
//! Version 1
//! From -7.5 3.7555555555555555
//! Components 1
//! Length 4096
//! {
//!         0.031525943963232252
//!         0.045645604561056156
//!         ...
//! }
//! ```
//!
//! The file content becomes a range remapping matrix (`From` values, omitted
//! when `[0, 1]`) followed by a 1D LUT.

use super::utils::{
    bake_identity_lut1d, bake_linear_scale_lut1d, format_fixed6, format_fixed6_rgb,
};
use super::utils::{
    from_chars_f32, min_max_matrix_f32, new_lut1d, scanf, trim, IStream, MAX_1D_LUT_LENGTH,
};
use super::{bake_capability, capability, CachedFile, FileFormat, FormatInfo};
use crate::baker::{input_to_target_processor, shaper_range, Baker};
use crate::error::{Error, Result};
use crate::transforms::GroupTransform;
use crate::types::{BitDepth, Interpolation};

const MAX_LINE_SIZE: usize = 4096;

struct LocalFileFormat;

pub(crate) fn create() -> Box<dyn FileFormat> {
    Box::new(LocalFileFormat)
}

/// Build the error message used by the reader.
fn error_message(error: &str, line: i32, line_content: &str) -> Error {
    let mut os = String::new();
    if line != -1 {
        os.push_str(&format!("At line {line}: "));
    }
    os.push_str(error);
    if line != -1 && !line_content.is_empty() {
        os.push_str(&format!(" ({line_content})"));
    }
    Error::msg(os)
}

impl FileFormat for LocalFileFormat {
    fn format_info(&self) -> Vec<FormatInfo> {
        vec![FormatInfo {
            name: "spi1d",
            extension: "spi1d",
            capabilities: capability::READ | capability::BAKE,
            bake_capabilities: bake_capability::LUT1D,
        }]
    }

    fn read(
        &self,
        data: &[u8],
        _original_file_name: &str,
        interp: Interpolation,
    ) -> Result<CachedFile> {
        let mut istream = IStream::new(data);

        // Parse header info.
        let mut lut_size: i32 = -1;
        let mut from_min: f32 = 0.0;
        let mut from_max: f32 = 1.0;
        let mut version: i32 = -1;
        let mut components: i32 = -1;
        let mut current_line: i32 = 0;

        loop {
            let header_line = istream.getline_limited(MAX_LINE_SIZE);
            current_line += 1;

            if header_line.starts_with("Version") {
                // " " in the format means any number of white spaces
                // (including 0 of them): "Version1" is valid.
                let (n, v) = scanf(&header_line, "Version %d");
                if n != 1 {
                    return Err(error_message(
                        "Invalid 'Version' Tag",
                        current_line,
                        &header_line,
                    ));
                }
                version = v[0].as_int();
                if version != 1 {
                    return Err(error_message(
                        "Only format version 1 supported",
                        current_line,
                        &header_line,
                    ));
                }
            } else if header_line.starts_with("From") {
                let (n, v) = scanf(&header_line, "From %63s %63s");
                if n != 2 {
                    return Err(error_message(
                        "Invalid 'From' Tag",
                        current_line,
                        &header_line,
                    ));
                }
                match (from_chars_f32(v[0].as_str()), from_chars_f32(v[1].as_str())) {
                    (Some(mn), Some(mx)) => {
                        from_min = mn;
                        from_max = mx;
                    }
                    _ => {
                        return Err(error_message(
                            "Invalid 'From' Tag",
                            current_line,
                            &header_line,
                        ))
                    }
                }
            } else if header_line.starts_with("Components") {
                let (n, v) = scanf(&header_line, "Components %d");
                if n != 1 {
                    return Err(error_message(
                        "Invalid 'Components' Tag",
                        current_line,
                        &header_line,
                    ));
                }
                components = v[0].as_int();
            } else if header_line.starts_with("Length") {
                let (n, v) = scanf(&header_line, "Length %d");
                if n != 1 {
                    return Err(error_message(
                        "Invalid 'Length' Tag",
                        current_line,
                        &header_line,
                    ));
                }
                lut_size = v[0].as_int();
            }

            if !(istream.good() && !header_line.starts_with('{')) {
                break;
            }
        }

        if version == -1 {
            return Err(error_message("Could not find 'Version' Tag", -1, ""));
        }
        if lut_size == -1 {
            return Err(error_message("Could not find 'Length' Tag", -1, ""));
        }
        if lut_size < 2 || lut_size as i64 > MAX_1D_LUT_LENGTH as i64 {
            return Err(error_message(
                &format!("'Length' must be between 2 and {MAX_1D_LUT_LENGTH}"),
                -1,
                "",
            ));
        }
        if components == -1 {
            return Err(error_message("Could not find 'Components' Tag", -1, ""));
        }
        if !(0..=3).contains(&components) {
            return Err(error_message("Components must be [1,2,3]", -1, ""));
        }

        let lut_len = lut_size as usize;
        let mut lut = new_lut1d(lut_len, false, interp, BitDepth::F32);
        let ncomp = components as usize;

        let mut i = 0usize;
        let mut line_buffer = istream.getline_limited(MAX_LINE_SIZE);
        current_line += 1;
        let mut line_count = 0usize;

        while istream.good() {
            let line = trim(&line_buffer).to_string();
            if line.eq_ignore_ascii_case("}") {
                break;
            }

            if !line.is_empty() {
                let (n, parts) = scanf(&line_buffer, "%63s %63s %63s %63s");
                if n != components {
                    return Err(error_message("Malformed LUT line", current_line, &line));
                }

                if line_count >= lut_len {
                    return Err(error_message("Too many entries found", current_line, ""));
                }

                let mut values = [0.0f32; 3];
                for (c, value) in values.iter_mut().enumerate().take(ncomp) {
                    match from_chars_f32(parts[c].as_str()) {
                        Some(v) => *value = v,
                        None => {
                            return Err(error_message("Malformed LUT line", current_line, &line))
                        }
                    }
                }

                match ncomp {
                    // If 1 component is specified, use x1 x1 x1.
                    1 => {
                        lut.values[i] = values[0];
                        lut.values[i + 1] = values[0];
                        lut.values[i + 2] = values[0];
                    }
                    // If 2 components are specified, use x1 x2 0.0.
                    2 => {
                        lut.values[i] = values[0];
                        lut.values[i + 1] = values[1];
                        lut.values[i + 2] = 0.0;
                    }
                    // If 3 components are specified, use x1 x2 x3.
                    _ => {
                        lut.values[i] = values[0];
                        lut.values[i + 1] = values[1];
                        lut.values[i + 2] = values[2];
                    }
                }
                i += 3;
                line_count += 1;
            }

            line_buffer = istream.getline_limited(MAX_LINE_SIZE);
            current_line += 1;
        }

        if line_count != lut_len {
            return Err(error_message("Not enough entries found", current_line, ""));
        }

        let mut group = GroupTransform::new();
        if let Some(m) = min_max_matrix_f32(from_min, from_max)? {
            group.append(m);
        }
        group.append(lut);
        Ok(CachedFile::new(group))
    }

    fn bake(&self, baker: &Baker, format_name: &str) -> Result<Vec<u8>> {
        const DEFAULT_1D_SIZE: usize = 4096;

        if format_name != "spi1d" {
            crate::bail!("Unknown spi format name, '{format_name}'.");
        }

        // The cube size is used as the 1D LUT size.
        let oned_size = baker.cube_size().unwrap_or(DEFAULT_1D_SIZE);

        let mut from_in_start = 0.0f32;
        let mut from_in_end = 1.0f32;

        // Generate the 1D LUT (the shaper space, if any, gives the input
        // range of the LUT).
        let mut oned_data = if !baker.shaper_space().is_empty() {
            (from_in_start, from_in_end) = shaper_range(baker)?;
            bake_linear_scale_lut1d(oned_size, from_in_start, from_in_end)?
        } else {
            bake_identity_lut1d(oned_size)?
        };

        input_to_target_processor(baker)?.apply_rgb_slice(&mut oned_data);

        // Write the LUT (fixed 6 decimal precision).
        let mut out = String::new();

        // Header.
        out.push_str("Version 1\n");
        out.push_str(&format!(
            "From {} {}\n",
            format_fixed6(from_in_start),
            format_fixed6(from_in_end)
        ));
        out.push_str(&format!("Length {oned_size}\n"));
        out.push_str("Components 3\n");
        out.push_str("{\n");

        // Write the 1D data.
        for rgb in oned_data.as_chunks::<3>().0 {
            out.push_str("    ");
            out.push_str(&format_fixed6_rgb(rgb));
            out.push('\n');
        }

        // Footer.
        out.push_str("}\n");

        Ok(out.into_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transforms::Transform;

    fn test_file(name: &str) -> Vec<u8> {
        let path = format!("{}/tests/data/files/{}", env!("CARGO_MANIFEST_DIR"), name);
        std::fs::read(path).expect("test file")
    }

    fn read_spi1d(content: &str) -> Result<CachedFile> {
        LocalFileFormat.read(content.as_bytes(), "Memory File", Interpolation::Default)
    }

    fn check_error(content: &str, what: &str) {
        match read_spi1d(content) {
            Ok(_) => panic!("expected an error containing '{what}'"),
            Err(e) => assert!(
                e.message().contains(what),
                "'{}' does not contain '{}'",
                e.message(),
                what
            ),
        }
    }

    #[test]
    fn format_info() {
        let info = LocalFileFormat.format_info();
        assert_eq!(info.len(), 1);
        assert_eq!(info[0].name, "spi1d");
        assert_eq!(info[0].extension, "spi1d");
        assert_eq!(info[0].capabilities, capability::READ | capability::BAKE);
    }

    #[test]
    fn test() {
        let file = LocalFileFormat
            .read(&test_file("cpf.spi1d"), "cpf.spi1d", Interpolation::Default)
            .unwrap();
        // from_min = 0 & from_max = 1: no range matrix.
        assert_eq!(file.group.num_transforms(), 1);
        let Transform::Lut1D(lut) = &file.group.transforms[0] else {
            panic!("expected a Lut1D")
        };
        assert_eq!(lut.file_output_bit_depth, BitDepth::F32);
        assert_eq!(lut.length(), 2048);
        assert_eq!(lut.values[0], 0.0);
        assert_eq!(lut.values[1], 0.0);
        assert_eq!(lut.values[2], 0.0);
        assert_eq!(lut.values[1970 * 3], 4.511920005404118f32);
        assert_eq!(lut.values[1970 * 3 + 1], 4.511920005404118f32);
        assert_eq!(lut.values[1970 * 3 + 2], 4.511920005404118f32);
    }

    #[test]
    fn from_range() {
        let file = read_spi1d(
            "Version 1\nFrom -1.0 3.0\nLength 2\nComponents 2\n{\n0.0 0.5\n1.0 2.0\n}\n",
        )
        .unwrap();
        assert_eq!(file.group.num_transforms(), 2);
        let Transform::Matrix(m) = &file.group.transforms[0] else {
            panic!("expected a matrix")
        };
        assert_eq!(m.matrix[0], 0.25);
        assert_eq!(m.offset[0], 0.25);
        let Transform::Lut1D(lut) = &file.group.transforms[1] else {
            panic!("expected a Lut1D")
        };
        assert_eq!(lut.values, vec![0.0, 0.5, 0.0, 1.0, 2.0, 0.0]);
    }

    #[test]
    fn interpolation() {
        let sample = "Version 1\nFrom 0.0 1.0\nLength 2\nComponents 1\n{\n0.0\n1.0\n}\n";
        let file = LocalFileFormat
            .read(sample.as_bytes(), "", Interpolation::Nearest)
            .unwrap();
        let Transform::Lut1D(lut) = &file.group.transforms[0] else {
            panic!()
        };
        assert_eq!(lut.interpolation, Interpolation::Nearest);
        let file = LocalFileFormat
            .read(sample.as_bytes(), "", Interpolation::Tetrahedral)
            .unwrap();
        let Transform::Lut1D(lut) = &file.group.transforms[0] else {
            panic!()
        };
        assert_eq!(lut.interpolation, Interpolation::Default);
    }

    #[test]
    fn read_failure() {
        // Validate stream can be read with no error.
        assert!(
            read_spi1d("Version 1\nFrom 0.0 1.0\nLength 2\nComponents 1\n{\n0.0\n\n1.0\n}\n")
                .is_ok()
        );
        // Version missing.
        check_error(
            "From 0.0 1.0\nLength 2\nComponents 1\n{\n0.0\n1.0\n}\n",
            "Could not find 'Version' Tag",
        );
        // Version is not 1.
        check_error(
            "Version 2\nFrom 0.0 1.0\nLength 2\nComponents 1\n{\n0.0\n1.0\n}\n",
            "Only format version 1 supported",
        );
        // Version can't be scanned.
        check_error(
            "Version A\nFrom 0.0 1.0\nLength 2\nComponents 1\n{\n0.0\n1.0\n}\n",
            "Invalid 'Version' Tag",
        );
        // Version case is wrong.
        check_error(
            "VERSION 1\nFrom 0.0 1.0\nLength 2\nComponents 1\n{\n0.0\n1.0\n}\n",
            "Could not find 'Version' Tag",
        );
        // From does not specify 2 floats.
        check_error(
            "Version 1\nFrom 0.0\nLength 2\nComponents 1\n{\n0.0\n1.0\n}\n",
            "Invalid 'From' Tag",
        );
        // Length is missing.
        check_error(
            "Version 1\nFrom 0.0 1.0\nComponents 1\n{\n0.0\n1.0\n}\n",
            "Could not find 'Length' Tag",
        );
        // Length can't be read.
        check_error(
            "Version 1\nFrom 0.0 1.0\nLength A\nComponents 1\n{\n0.0\n1.0\n}\n",
            "Invalid 'Length' Tag",
        );
        // Component is missing.
        check_error(
            "Version 1\nFrom 0.0 1.0\nLength 2\n{\n0.0\n1.0\n}\n",
            "Could not find 'Components' Tag",
        );
        // Component can't be read.
        check_error(
            "Version 1\nFrom 0.0 1.0\nLength 2\nComponents A\n{\n0.0\n1.0\n}\n",
            "Invalid 'Components' Tag",
        );
        // Component not 1 or 2 or 3.
        check_error(
            "Version 1\nFrom 0.0 1.0\nLength 2\nComponents 4\n{\n0.0\n1.0\n}\n",
            "Components must be [1,2,3]",
        );
        // LUT too short.
        check_error(
            "Version 1\nFrom 0.0 1.0\nLength 2\nComponents 1\n{\n0.0\n}\n",
            "Not enough entries found",
        );
        // LUT too long.
        check_error(
            "Version 1\nFrom 0.0 1.0\nLength 2\nComponents 1\n{\n0.0\n0.0\n0.0\n}\n",
            "Too many entries found",
        );
        // Components==1 but two components specified in LUT.
        check_error(
            "Version 1\nFrom 0.0 1.0\nLength 2\nComponents 1\n{\n0.0\n1.0 1.0\n}\n",
            "Malformed LUT line",
        );
        // Length out of bounds.
        check_error(
            "Version 1\nFrom 0.0 1.0\nLength 1\nComponents 1\n{\n0.0\n}\n",
            "'Length' must be between 2 and 300000",
        );
        // Error messages include the line.
        match read_spi1d("Version 1\nFrom 0.0 1.0\nLength 2\nComponents 1\n{\n0.0\n1.0 1.0\n}\n") {
            Err(e) => assert_eq!(e.message(), "At line 7: Malformed LUT line (1.0 1.0)"),
            Ok(_) => panic!(),
        }
    }

    #[test]
    fn identity_values() {
        let file =
            read_spi1d("Version 1\nFrom 0.0 1.0\nLength 2\nComponents 1\n{\n0.0\n1.000007\n}\n")
                .unwrap();
        let Transform::Lut1D(lut) = &file.group.transforms[0] else {
            panic!()
        };
        assert_eq!(lut.values[3], 1.000007f32);
    }

    #[test]
    fn identity() {
        use crate::ops::lut1d::Lut1DOp;
        // Port of OCIO's check on `Lut1DOpData::isIdentity` (which ignores
        // the clamping a standard domain LUT performs).
        let lut_is_identity = |text: &str| -> bool {
            let file = read_spi1d(text).unwrap();
            let mut ops = crate::ops::OpVec::new();
            crate::transforms::build::build_ops(
                &mut ops,
                &crate::Config::create_raw(),
                &crate::Context::new(),
                &Transform::Group(file.group),
                crate::TransformDirection::Forward,
            )
            .unwrap();
            let lut = ops
                .iter()
                .find_map(|o| o.downcast_ref::<Lut1DOp>())
                .unwrap();
            lut.data().is_identity()
        };
        assert!(lut_is_identity(
            "Version 1\nFrom 0.0 1.0\nLength 2\nComponents 1\n{\n0.0\n1.000007\n}\n"
        ));
        assert!(!lut_is_identity(
            "Version 1\nFrom 0.0 1.0\nLength 2\nComponents 1\n{\n0.0\n1.00001\n}\n"
        ));
    }

    // Baker tests (port of the baker parts of `FileFormatSpi1D_tests.cpp`).

    use crate::fileformats::utils::bake_test_utils::{
        bake, baker, check_round_trip, compare_lines, config_yaml, SHAPER_LOG2_CONFIG,
    };

    #[test]
    fn bake_1d() {
        let config = config_yaml(&[("input", ""), ("target", "")]);
        let mut b = baker(&config, "spi1d");
        b.set_input_space("input");
        b.set_target_space("target");
        b.set_cube_size(Some(2));

        let expected = "Version 1\n\
            From 0.000000 1.000000\n\
            Length 2\n\
            Components 3\n\
            {\n    0.000000 0.000000 0.000000\n    1.000000 1.000000 1.000000\n}\n";
        assert_eq!(bake(&b), expected);
    }

    #[test]
    fn bake_1d_shaper() {
        {
            // Lin to Log.
            let mut b = baker(SHAPER_LOG2_CONFIG, "spi1d");
            b.set_input_space("Raw");
            b.set_target_space("Log2");
            // The shaper space is used here to derive the range of the LUT.
            // This is needed because the range [0, 1] will not cover the
            // full extent of the log space.
            b.set_shaper_space("Log2");
            b.set_cube_size(Some(10));

            let expected = "Version 1\n\
                From 0.001989 16.291878\n\
                Length 10\n\
                Components 3\n\
                {\n    0.000000 0.000000 0.000000\n    0.756268 0.756268 0.756268\n    0.833130 0.833130 0.833130\n    0.878107 0.878107 0.878107\n    0.910023 0.910023 0.910023\n    0.934780 0.934780 0.934780\n    0.955010 0.955010 0.955010\n    0.972114 0.972114 0.972114\n    0.986931 0.986931 0.986931\n    1.000000 1.000000 1.000000\n}\n";
            assert_eq!(bake(&b), expected);
        }
        {
            // Log to Lin.
            let mut b = baker(SHAPER_LOG2_CONFIG, "spi1d");
            b.set_input_space("Log2");
            b.set_target_space("Raw");
            b.set_cube_size(Some(10));

            let expected = "Version 1\n\
                From 0.000000 1.000000\n\
                Length 10\n\
                Components 3\n\
                {\n    0.001989 0.001989 0.001989\n    0.005413 0.005413 0.005413\n    0.014731 0.014731 0.014731\n    0.040091 0.040091 0.040091\n    0.109110 0.109110 0.109110\n    0.296951 0.296951 0.296951\n    0.808177 0.808177 0.808177\n    2.199522 2.199522 2.199522\n    5.986179 5.986179 5.986179\n    16.291878 16.291878 16.291878\n}\n";
            compare_lines(&bake(&b), expected, 1e-5, |i| (6..15).contains(&i));
        }
    }

    #[test]
    fn bake_defaults_and_errors() {
        let config = config_yaml(&[("input", ""), ("target", "")]);
        let mut b = baker(&config, "spi1d");
        b.set_input_space("input");
        b.set_target_space("target");
        let out = bake(&b);
        assert!(out.contains("Length 4096\n"));
        assert_eq!(out.lines().count(), 5 + 4096 + 1);

        let e = LocalFileFormat.bake(&b, "spi").unwrap_err();
        assert_eq!(e.message(), "Unknown spi format name, 'spi'.");
    }

    #[test]
    fn bake_round_trip() {
        let mut b = baker(SHAPER_LOG2_CONFIG, "spi1d");
        b.set_input_space("Log2");
        b.set_target_space("Raw");
        check_round_trip(
            &b,
            &[[0.0, 0.0, 0.0], [0.25, 0.5, 0.75], [1.0, 1.0, 1.0]],
            1e-3,
        );

        let mut b = baker(SHAPER_LOG2_CONFIG, "spi1d");
        b.set_input_space("Raw");
        b.set_target_space("Log2");
        b.set_shaper_space("Log2");
        check_round_trip(&b, &[[0.18, 1.0, 10.0], [0.01, 0.5, 16.0]], 1e-3);
    }
}
