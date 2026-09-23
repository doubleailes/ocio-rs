//! Nuke `vf` (Inventor volume) 3D LUT format (port of `FileFormatVF.cpp`).
//!
//! ```text
//! #Inventor V2.1 ascii
//! grid_size 2 2 2
//! global_transform 1 0 0 0  0 1 0 0  0 0 1 0  0 0 0 1
//! data
//! 0 0 0
//! ...
//! ```
//!
//! The optional global transform becomes a matrix placed before the 3D LUT
//! (blue fastest).

use super::utils::{
    new_lut3d, split_by_white_spaces, string_to_int, string_vec_to_float_vec, trim, IStream,
    MAX_3D_LUT_LENGTH,
};
use super::{bake_capability, capability, CachedFile, FileFormat, FormatInfo};
use crate::error::{Error, Result};
use crate::transforms::{GroupTransform, MatrixTransform};
use crate::types::{BitDepth, Interpolation};

struct LocalFileFormat;

pub(crate) fn create() -> Box<dyn FileFormat> {
    Box::new(LocalFileFormat)
}

fn error_message(error: &str, file_name: &str, line: i32, line_content: &str) -> Error {
    let mut os = format!("Error parsing Nuke .vf file ({file_name}).  ");
    if line != -1 {
        os.push_str(&format!("At line ({line}): '{line_content}'.  "));
    }
    os.push_str(error);
    Error::msg(os)
}

impl FileFormat for LocalFileFormat {
    fn format_info(&self) -> Vec<FormatInfo> {
        vec![FormatInfo {
            name: "nukevf",
            extension: "vf",
            capabilities: capability::READ,
            bake_capabilities: bake_capability::NONE,
        }]
    }

    fn read(&self, data: &[u8], file_name: &str, interp: Interpolation) -> Result<CachedFile> {
        let mut istream = IStream::new(data);

        // Validate the file type.
        let mut line_number = 1;
        match istream.nextline() {
            Some(line) if line.to_ascii_lowercase().starts_with("#inventor") => {}
            other => {
                return Err(error_message(
                    "Expecting '#Inventor V2.1 ascii'.",
                    file_name,
                    line_number,
                    other.as_deref().unwrap_or(""),
                ))
            }
        }

        // Parse the file.
        let mut raw3d: Vec<f32> = Vec::new();
        let mut size3d = [0i32; 3];
        let mut global_transform: Vec<f32> = Vec::new();
        let mut in3d = false;

        while let Some(line) = istream.nextline() {
            line_number += 1;

            // Strip, lowercase, and split the line.
            let mut parts = split_by_white_spaces(&trim(&line).to_ascii_lowercase());
            if parts.is_empty() {
                continue;
            }
            if parts[0].starts_with('#') {
                continue;
            }

            if !in3d {
                match parts[0].as_str() {
                    "grid_size" => {
                        let sizes: Option<Vec<i32>> = if parts.len() == 4 {
                            parts[1..4]
                                .iter()
                                .map(|p| string_to_int(p, false))
                                .collect()
                        } else {
                            None
                        };
                        let Some(sizes) = sizes else {
                            return Err(error_message(
                                "Malformed grid_size tag.",
                                file_name,
                                line_number,
                                &line,
                            ));
                        };
                        size3d.copy_from_slice(&sizes);

                        // TODO: Support nonuniformly sized LUTs.
                        if size3d[0] != size3d[1] || size3d[0] != size3d[2] {
                            return Err(error_message(
                                &format!(
                                    "Only equal grid size LUTs are supported. Found grid size: {} x {} x {}.",
                                    size3d[0], size3d[1], size3d[2]
                                ),
                                file_name,
                                line_number,
                                &line,
                            ));
                        }

                        if size3d[0] < 2 || size3d[0] as i64 > MAX_3D_LUT_LENGTH as i64 {
                            return Err(error_message(
                                &format!("Grid size must be between 2 and {MAX_3D_LUT_LENGTH}."),
                                file_name,
                                line_number,
                                &line,
                            ));
                        }
                        raw3d.reserve(3 * (size3d[0] * size3d[1] * size3d[2]) as usize);
                    }
                    "global_transform" => {
                        if parts.len() != 17 {
                            return Err(error_message(
                                "Malformed global_transform tag. 16 floats expected.",
                                file_name,
                                line_number,
                                &line,
                            ));
                        }

                        // Drop the 1st entry (the tag).
                        parts.remove(0);
                        global_transform = match string_vec_to_float_vec(&parts) {
                            Some(v) if v.len() == 16 => v,
                            _ => return Err(error_message(
                                "Malformed global_transform tag. Could not convert to float array.",
                                file_name,
                                line_number,
                                &line,
                            )),
                        };
                    }
                    // TODO: element_size (aka scale3)
                    // TODO: world_origin (aka translate3)
                    "data" => in3d = true,
                    _ => {}
                }
            } else if let Some(values) = string_vec_to_float_vec(&parts) {
                if values.len() == 3 {
                    if raw3d.len() > MAX_3D_LUT_LENGTH * MAX_3D_LUT_LENGTH * MAX_3D_LUT_LENGTH * 3 {
                        return Err(error_message(
                            "Too many 3D LUT entries.",
                            file_name,
                            line_number,
                            &line,
                        ));
                    }
                    raw3d.extend_from_slice(&values);
                }
            }
        }

        // Interpret the parsed data, validate LUT sizes.
        let num3d = (size3d[0] * size3d[1] * size3d[2]) as i64;
        if num3d != (raw3d.len() / 3) as i64 {
            return Err(error_message(
                &format!(
                    "Incorrect number of 3D LUT entries. Found {}, expected {}.",
                    raw3d.len() / 3,
                    num3d
                ),
                file_name,
                -1,
                "",
            ));
        }

        if num3d == 0 {
            return Err(error_message("No 3D LUT entries found.", file_name, -1, ""));
        }

        let mut group = GroupTransform::new();

        // Setup the global matrix (Nuke pre-scales this by the 3D LUT size,
        // so we must undo that here).
        if global_transform.len() == 16 {
            let mut m44 = [0.0f64; 16];
            for i in 0..4 {
                global_transform[4 * i] *= size3d[0] as f32;
                global_transform[4 * i + 1] *= size3d[1] as f32;
                global_transform[4 * i + 2] *= size3d[2] as f32;
                for j in 0..4 {
                    m44[4 * i + j] = global_transform[4 * i + j] as f64;
                }
            }
            group.append(MatrixTransform::new(m44, [0.0; 4]));
        }

        // The LUT in the file is blue fastest.
        let mut lut = new_lut3d(size3d[0] as usize, interp, BitDepth::F32);
        lut.values.copy_from_slice(&raw3d);
        group.append(lut);

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

    const SAMPLE_NO_ERROR: &str = "#Inventor V2.1 ascii\ngrid_size 2 2 2\nglobal_transform 1 0 0 0  0 1 0 0  0 0 1 0  0 0 0 1 \ndata\n0 0 0\n0 0 1\n0 1 0\n0 1 1\n1 0 0\n1 0 1\n1 1 0\n1 1 1\n";

    #[test]
    fn format_info() {
        let info = LocalFileFormat.format_info();
        assert_eq!(info.len(), 1);
        assert_eq!(info[0].name, "nukevf");
        assert_eq!(info[0].extension, "vf");
        assert_eq!(info[0].capabilities, capability::READ);
    }

    #[test]
    fn read_failure() {
        assert!(read(SAMPLE_NO_ERROR).is_ok());
        // Too much data.
        let e = read(&SAMPLE_NO_ERROR.replace("1 1 0\n", "1 1 0\n1 1 0\n")).unwrap_err();
        assert!(e.message().contains("Incorrect number of 3D LUT entries"));
        // Other errors.
        let e = read("#Invent\n").unwrap_err();
        assert_eq!(e.message(), "Error parsing Nuke .vf file (Memory File).  At line (1): '#Invent'.  Expecting '#Inventor V2.1 ascii'.");
        let e = read(&SAMPLE_NO_ERROR.replace("grid_size 2 2 2", "grid_size 2 2")).unwrap_err();
        assert!(e
            .message()
            .contains("At line (2): 'grid_size 2 2'.  Malformed grid_size tag."));
        let e = read(&SAMPLE_NO_ERROR.replace("0 0 0 1 \n", "0 0 0 \n")).unwrap_err();
        assert!(e
            .message()
            .contains("Malformed global_transform tag. 16 floats expected."));
        let e = read("#Inventor V2.1 ascii\ndata\n").unwrap_err();
        assert!(e.message().contains("No 3D LUT entries found."));
    }

    #[test]
    fn load_ops() {
        let path = format!("{}/tests/data/files/nuke_3d.vf", env!("CARGO_MANIFEST_DIR"));
        let data = std::fs::read(&path).unwrap();
        let file = LocalFileFormat
            .read(&data, &path, Interpolation::Default)
            .unwrap();
        assert_eq!(file.group.num_transforms(), 2);

        let Transform::Matrix(m) = &file.group.transforms[0] else {
            panic!("expected a matrix")
        };
        let mut expected = [0.0f64; 16];
        expected[0] = 2.0;
        expected[5] = 2.0;
        expected[10] = 2.0;
        expected[15] = 1.0;
        assert_eq!(m.matrix, expected);

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
}
