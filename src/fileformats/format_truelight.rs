//! FilmLight Truelight `cub` format (port of `FileFormatTruelight.cpp`).
//!
//! ```text
//! # Truelight Cube v2.0
//! # iDims 3
//! # oDims 3
//! # width 3 3 3
//! # lutLength 5
//! # InputLUT
//!  0.000000 0.000000 0.000000
//!  ...
//! # Cube
//!  0.000000 0.000000 0.000000
//!  ...
//! # end
//! ```
//!
//! The optional input LUT (1D) is applied before the cube (3D LUT, red
//! fastest). The input LUT values are scaled from `[0, width - 1]`.

use super::utils::{
    new_lut1d, new_lut3d, set_lut3d_from_red_fastest, split_by_white_spaces, string_to_int,
    string_vec_to_float_vec, trim, IStream, MAX_1D_LUT_LENGTH, MAX_3D_LUT_LENGTH,
};
use super::{bake_capability, capability, CachedFile, FileFormat, FormatInfo};
use crate::error::Result;
use crate::transforms::GroupTransform;
use crate::types::{BitDepth, Interpolation};

struct LocalFileFormat;

pub(crate) fn create() -> Box<dyn FileFormat> {
    Box::new(LocalFileFormat)
}

impl FileFormat for LocalFileFormat {
    fn format_info(&self) -> Vec<FormatInfo> {
        vec![FormatInfo {
            name: "truelight",
            extension: "cub",
            capabilities: capability::READ | capability::BAKE,
            bake_capabilities: bake_capability::LUT3D,
        }]
    }

    fn read(
        &self,
        data: &[u8],
        _original_file_name: &str,
        interp: Interpolation,
    ) -> Result<CachedFile> {
        let mut istream = IStream::new(data);

        // Validate the file type.
        match istream.nextline() {
            Some(line) if line.to_ascii_lowercase().starts_with("# truelight cube") => {}
            _ => crate::bail!("LUT doesn't seem to be a Truelight .cub LUT."),
        }

        // Parse the file.
        let mut raw1d: Vec<f32> = Vec::new();
        let mut raw3d: Vec<f32> = Vec::new();
        let mut size3d = [0i32; 3];
        let mut size1d: i32 = 0;
        let mut in1d = false;
        let mut in3d = false;

        while let Some(line) = istream.nextline() {
            // Strip, lowercase, and split the line.
            let parts = split_by_white_spaces(&trim(&line).to_ascii_lowercase());
            if parts.is_empty() {
                continue;
            }

            // Parse header metadata (which starts with #).
            if parts[0].starts_with('#') {
                if parts.len() < 2 {
                    continue;
                }

                match parts[1].as_str() {
                    "width" => {
                        let sizes: Option<Vec<i32>> = if parts.len() == 5 {
                            parts[2..5]
                                .iter()
                                .map(|p| string_to_int(p, false))
                                .collect()
                        } else {
                            None
                        };
                        let Some(sizes) = sizes else {
                            crate::bail!("Malformed width tag in Truelight .cub LUT.");
                        };
                        size3d.copy_from_slice(&sizes);

                        if size3d[0] != size3d[1] || size3d[0] != size3d[2] {
                            crate::bail!(
                                "Truelight .cub LUT. Only equal grid size LUTs are supported. Found grid size: {} x {} x {}.",
                                size3d[0],
                                size3d[1],
                                size3d[2]
                            );
                        }

                        if size3d[0] < 2 || size3d[0] as i64 > MAX_3D_LUT_LENGTH as i64 {
                            crate::bail!("Truelight .cub LUT grid size must be between 2 and {MAX_3D_LUT_LENGTH}.");
                        }
                        raw3d.reserve(3 * (size3d[0] * size3d[1] * size3d[2]) as usize);
                    }
                    "lutlength" => {
                        size1d = match (
                            parts.len(),
                            string_to_int(&parts[2.min(parts.len() - 1)], false),
                        ) {
                            (3, Some(s)) => s,
                            _ => crate::bail!("Malformed lutlength tag in Truelight .cub LUT."),
                        };
                        if size1d < 2 || size1d as i64 > MAX_1D_LUT_LENGTH as i64 {
                            crate::bail!("Truelight .cub LUT lutlength must be between 2 and {MAX_1D_LUT_LENGTH}.");
                        }
                        raw1d.reserve(3 * size1d as usize);
                    }
                    "inputlut" => {
                        in1d = true;
                        in3d = false;
                    }
                    "cube" => {
                        in3d = true;
                        in1d = false;
                    }
                    "end" => {
                        // If we hit the end tag, don't bother searching
                        // further in the file.
                        break;
                    }
                    _ => {}
                }
            }

            if in1d || in3d {
                if let Some(values) = string_vec_to_float_vec(&parts) {
                    if values.len() == 3 {
                        if in1d {
                            if raw1d.len() > MAX_1D_LUT_LENGTH * 3 {
                                crate::bail!("Too many 1D LUT entries in Truelight .cub LUT.");
                            }
                            raw1d.extend_from_slice(&values);
                        } else {
                            if raw3d.len()
                                > MAX_3D_LUT_LENGTH * MAX_3D_LUT_LENGTH * MAX_3D_LUT_LENGTH * 3
                            {
                                crate::bail!("Too many 3D LUT entries in Truelight .cub LUT.");
                            }
                            raw3d.extend_from_slice(&values);
                        }
                    }
                }
            }
        }

        // Interpret the parsed data, validate LUT sizes.
        if size1d as i64 != (raw1d.len() / 3) as i64 {
            crate::bail!(
                "Parse error in Truelight .cub LUT. Incorrect number of lut1d entries. Found {}, expected {}.",
                raw1d.len() / 3,
                size1d
            );
        }

        let num3d = (size3d[0] * size3d[1] * size3d[2]) as i64;
        if num3d != (raw3d.len() / 3) as i64 {
            crate::bail!(
                "Parse error in Truelight .cub LUT. Incorrect number of 3D LUT entries. Found {}, expected {}.",
                raw3d.len() / 3,
                num3d
            );
        }

        let has3d = num3d > 0;
        let mut group = GroupTransform::new();

        // Reformat 1D data.
        if size1d > 0 {
            let mut lut = new_lut1d(size1d as usize, false, interp, BitDepth::F32);

            // Determine the scale factor for the 1D LUT. Example: the
            // inputlut feeding a 6x6x6 3D LUT should be scaled from 0.0-5.0.
            // Beware: Nuke Truelight Writer (at least 6.3 and before) is
            // busted and does this scaling incorrectly.
            let descale = if has3d {
                1.0f32 / (size3d[0] - 1) as f32
            } else {
                1.0f32
            };
            for (dst, &v) in lut.values.iter_mut().zip(raw1d.iter()) {
                *dst = v * descale;
            }
            group.append(lut);
        }

        if has3d {
            // Reformat 3D data.
            let mut lut = new_lut3d(size3d[0] as usize, interp, BitDepth::F32);
            set_lut3d_from_red_fastest(&mut lut, &raw3d)?;
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

    const CUBE: &str = "# Cube\n 0.000000 0.000000 0.000000\n 0.250000 0.000000 0.000000\n 0.500000 0.000000 0.000000\n 0.000000 0.500000 0.000000\n 0.250000 0.500000 0.000000\n 0.500000 0.500000 0.000000\n 0.000000 1.000000 0.000000\n 0.250000 1.000000 0.000000\n 0.500000 1.000000 0.000000\n 0.000000 0.000000 0.500000\n 0.250000 0.000000 0.500000\n 0.500000 0.000000 0.500000\n 0.000000 0.500000 0.500000\n 0.250000 0.500000 0.500000\n 0.500000 0.500000 0.500000\n 0.000000 1.000000 0.500000\n 0.250000 1.000000 0.500000\n 0.500000 1.000000 0.500000\n 0.000000 0.000000 1.000000\n 0.250000 0.000000 1.000000\n 0.500000 0.000000 1.000000\n 0.000000 0.500000 1.000000\n 0.250000 0.500000 1.000000\n 0.500000 0.500000 1.000000\n 0.000000 1.000000 1.000000\n 0.250000 1.000000 1.000000\n 0.500000 1.000000 1.000000\n\n# end\n";

    fn shaper_and_lut_3d_text() -> String {
        format!(
            "# Truelight Cube v2.0\n# iDims 3\n# oDims 3\n# width 3 3 3\n# lutLength 5\n# InputLUT\n 0.000000 0.000000 0.000000\n 0.500000 0.500000 0.500000\n 1.000000 1.000000 1.000000\n 1.500000 1.500000 1.500000\n 2.000000 2.000000 2.000000\n\n{CUBE}\n# Truelight profile\ntitle{{madeup on some display}}\nprint{{someprint}}\ndisplay{{some}}\ncubeFile{{madeup.cube}}\n\n # This last line confirms 'end' tag is obeyed\n 1.23456 1.23456 1.23456\n"
        )
    }

    const SHAPER: &str = "# Truelight Cube v2.0\n# lutLength 11\n# iDims 3\n\n\n# InputLUT\n 0.000 0.000 -0.000\n 0.200 0.010 -0.100\n 0.400 0.040 -0.200\n 0.600 0.090 -0.300\n 0.800 0.160 -0.400\n 1.000 0.250 -0.500\n 1.200 0.360 -0.600\n 1.400 0.490 -0.700\n 1.600 0.640 -0.800\n 1.800 0.820 -0.900\n 2.000 1.000 -1.000\n\n\n\n# end\n";

    #[test]
    fn format_info() {
        let info = LocalFileFormat.format_info();
        assert_eq!(info.len(), 1);
        assert_eq!(info[0].name, "truelight");
        assert_eq!(info[0].extension, "cub");
        assert_eq!(info[0].capabilities, capability::READ | capability::BAKE);
    }

    #[test]
    fn shaper_and_lut_3d() {
        let file = read(&shaper_and_lut_3d_text()).unwrap();
        assert_eq!(file.group.num_transforms(), 2);
        let Transform::Lut1D(lut1) = &file.group.transforms[0] else {
            panic!("expected a Lut1D")
        };
        assert_eq!(lut1.file_output_bit_depth, BitDepth::F32);
        // The input LUT is descaled by 1 / (3 - 1).
        assert_eq!(lut1.value(1), [0.25, 0.25, 0.25]);
        assert_eq!(lut1.value(4), [1.0, 1.0, 1.0]);
        let Transform::Lut3D(lut3) = &file.group.transforms[1] else {
            panic!("expected a Lut3D")
        };
        assert_eq!(lut3.file_output_bit_depth, BitDepth::F32);
        assert_eq!(lut3.grid_size, 3);
        // Lowers the red channel by 0.5.
        assert_eq!(lut3.value(2, 1, 0), [0.5, 0.5, 0.0]);
        assert_eq!(lut3.value(2, 2, 2), [0.5, 1.0, 1.0]);
    }

    #[test]
    fn shaper() {
        let file = read(SHAPER).unwrap();
        assert_eq!(file.group.num_transforms(), 1);
        let Transform::Lut1D(lut1) = &file.group.transforms[0] else {
            panic!("expected a Lut1D")
        };
        assert_eq!(lut1.length(), 11);
        // No 3D LUT: no descale.
        assert_eq!(lut1.value(10), [2.0, 1.0, -1.0]);
    }

    #[test]
    fn lut_3d() {
        let content =
            format!("# Truelight Cube v2.0\n# iDims 3\n# oDims 3\n# width 3 3 3\n\n\n\n{CUBE}");
        let file = read(&content).unwrap();
        assert_eq!(file.group.num_transforms(), 1);
        assert!(matches!(file.group.transforms[0], Transform::Lut3D(_)));
    }

    #[test]
    fn read_failure() {
        let e = read("# Not a truelight cube\n").unwrap_err();
        assert_eq!(e.message(), "LUT doesn't seem to be a Truelight .cub LUT.");
        let e = read("# Truelight Cube v2.0\n# width 3 3\n").unwrap_err();
        assert_eq!(e.message(), "Malformed width tag in Truelight .cub LUT.");
        let e = read("# Truelight Cube v2.0\n# width 3 3 4\n").unwrap_err();
        assert!(e
            .message()
            .contains("Only equal grid size LUTs are supported. Found grid size: 3 x 3 x 4."));
        let e = read("# Truelight Cube v2.0\n# lutLength 5\n").unwrap_err();
        assert!(e
            .message()
            .contains("Incorrect number of lut1d entries. Found 0, expected 5."));
        let e = read("# Truelight Cube v2.0\n# lutLength\n").unwrap_err();
        assert_eq!(
            e.message(),
            "Malformed lutlength tag in Truelight .cub LUT."
        );
        let e = read("# Truelight Cube v2.0\n# width 2 2 2\n# Cube\n0 0 0\n").unwrap_err();
        assert!(e
            .message()
            .contains("Incorrect number of 3D LUT entries. Found 1, expected 8."));
    }

    fn apply(file: CachedFile, data: &mut [[f32; 4]]) {
        use crate::processor::Processor;
        let config = crate::Config::create_raw();
        let ctx = crate::Context::new();
        let p = Processor::from_transform(
            &config,
            &ctx,
            &Transform::Group(file.group),
            crate::TransformDirection::Forward,
        )
        .unwrap();
        p.default_cpu_processor().apply_pixels(data);
    }

    #[test]
    #[ignore = "needs-merge"]
    fn shaper_and_lut_3d_apply() {
        let mut data = [
            [0.1f32, 0.2, 0.3, 0.0],
            [1.0, 0.5, 0.123456, 0.0],
            [-1.0, 1.5, 0.5, 0.0],
        ];
        let result = [
            [0.05f32, 0.2, 0.3, 0.0],
            [0.50, 0.5, 0.123456, 0.0],
            [0.0, 1.0, 0.5, 0.0],
        ];
        apply(read(&shaper_and_lut_3d_text()).unwrap(), &mut data);
        for (d, r) in data.iter().zip(result.iter()) {
            for c in 0..4 {
                assert!((d[c] - r[c]).abs() <= 1e-6, "{d:?} {r:?}");
            }
        }
    }

    #[test]
    #[ignore = "needs-merge"]
    fn shaper_apply() {
        let mut data = [
            [0.1f32, 0.2, 0.3, 0.0],
            [1.0, 0.5, 0.123456, 0.0],
            [-1.0, 1.5, 0.5, 0.0],
        ];
        let result = [
            [0.2f32, 0.04, -0.3, 0.0],
            [2.0, 0.25, -0.123456, 0.0],
            [0.0, 1.0, -0.5, 0.0],
        ];
        apply(read(SHAPER).unwrap(), &mut data);
        for (d, r) in data.iter().zip(result.iter()) {
            for c in 0..4 {
                assert!((d[c] - r[c]).abs() <= 1e-6, "{d:?} {r:?}");
            }
        }
    }

    #[test]
    #[ignore = "needs-merge"]
    fn lut_3d_apply() {
        let content =
            format!("# Truelight Cube v2.0\n# iDims 3\n# oDims 3\n# width 3 3 3\n\n\n\n{CUBE}");
        let mut data = [
            [0.1f32, 0.2, 0.3, 0.0],
            [1.0, 0.5, 0.123456, 0.0],
            [-1.0, 1.5, 0.5, 0.0],
        ];
        let result = [
            [0.05f32, 0.2, 0.3, 0.0],
            [0.50, 0.5, 0.123456, 0.0],
            [0.0, 1.0, 0.5, 0.0],
        ];
        apply(read(&content).unwrap(), &mut data);
        for (d, r) in data.iter().zip(result.iter()) {
            for c in 0..4 {
                assert!((d[c] - r[c]).abs() <= 1e-6, "{d:?} {r:?}");
            }
        }
    }
}
