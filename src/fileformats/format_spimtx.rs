//! Sony Pictures Imageworks `spimtx` matrix format (port of
//! `FileFormatSpiMtx.cpp`).
//!
//! The file holds 12 floats: a 3x4 matrix whose last column is an offset
//! expressed in 16-bit code values.

use super::utils::{split_by_white_spaces, string_vec_to_float_vec, trim};
use super::{bake_capability, capability, CachedFile, FileFormat, FormatInfo};
use crate::error::Result;
use crate::transforms::{GroupTransform, MatrixTransform};
use crate::types::Interpolation;

/// A valid spimtx file contains exactly 12 floats.
const MAX_FILE_SIZE: usize = 1024;

struct LocalFileFormat;

pub(crate) fn create() -> Box<dyn FileFormat> {
    Box::new(LocalFileFormat)
}

impl FileFormat for LocalFileFormat {
    fn format_info(&self) -> Vec<FormatInfo> {
        vec![FormatInfo {
            name: "spimtx",
            extension: "spimtx",
            capabilities: capability::READ,
            bake_capabilities: bake_capability::NONE,
        }]
    }

    fn read(&self, data: &[u8], file_name: &str, _interp: Interpolation) -> Result<CachedFile> {
        // Read the entire file (capped).
        if data.len() >= MAX_FILE_SIZE {
            crate::bail!("Error parsing .spimtx file ({file_name}). File is too large to be a valid .spimtx file.");
        }
        let text = String::from_utf8_lossy(data);

        // Turn it into parts.
        let parts = split_by_white_spaces(trim(&text));
        if parts.len() != 12 {
            crate::bail!(
                "Error parsing .spimtx file ({}). File must contain 12 float entries. {} found.",
                file_name,
                parts.len()
            );
        }

        // Turn the parts into floats.
        let Some(f) = string_vec_to_float_vec(&parts) else {
            crate::bail!(
                "Error parsing .spimtx file ({file_name}). File must contain all float entries. "
            );
        };

        // Put the bits in the right place.
        let m44 = [
            f[0] as f64,
            f[1] as f64,
            f[2] as f64,
            0.0, //
            f[4] as f64,
            f[5] as f64,
            f[6] as f64,
            0.0, //
            f[8] as f64,
            f[9] as f64,
            f[10] as f64,
            0.0, //
            0.0,
            0.0,
            0.0,
            1.0,
        ];
        let offset4 = [
            f[3] as f64 / 65535.0,
            f[7] as f64 / 65535.0,
            f[11] as f64 / 65535.0,
            0.0,
        ];

        let mut group = GroupTransform::new();
        group.append(MatrixTransform::new(m44, offset4));
        Ok(CachedFile::new(group))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transforms::Transform;

    fn read_mtx(data: &[u8]) -> Result<MatrixTransform> {
        let file = LocalFileFormat.read(data, "Memory File", Interpolation::Default)?;
        assert_eq!(file.group.num_transforms(), 1);
        match &file.group.transforms[0] {
            Transform::Matrix(m) => Ok(m.clone()),
            _ => panic!("expected a matrix"),
        }
    }

    #[test]
    fn format_info() {
        let info = LocalFileFormat.format_info();
        assert_eq!(info.len(), 1);
        assert_eq!(info[0].name, "spimtx");
        assert_eq!(info[0].extension, "spimtx");
        assert_eq!(info[0].capabilities, capability::READ);
    }

    #[test]
    fn test() {
        let path = format!(
            "{}/tests/data/files/camera_to_aces.spimtx",
            env!("CARGO_MANIFEST_DIR")
        );
        let m = read_mtx(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(m.offset, [0.0; 4]);
        let expected: [f32; 16] = [
            0.754338638,
            0.133697046,
            0.111968437,
            0.0, //
            0.021198141,
            1.005410934,
            -0.026610548,
            0.0, //
            -0.009756991,
            0.004508563,
            1.005253201,
            0.0, //
            0.0,
            0.0,
            0.0,
            1.0,
        ];
        for i in 0..16 {
            assert_eq!(m.matrix[i] as f32, expected[i], "index {i}");
        }
    }

    #[test]
    fn read_offset() {
        let m = read_mtx(b"1 0 0 6553.5\n0 1 0 32767.5\n0 0 1 65535.0\n").unwrap();
        assert_eq!(m.offset, [0.1, 0.5, 1.0, 0.0]);
    }

    #[test]
    fn read_failure() {
        assert!(read_mtx(b"1.0 0.0 0.0 0.0\n0.0 1.0 0.0 0.0\n0.0 0.0 1.0 0.0\n").is_ok());
        let e = read_mtx(b"1.0 0.0 0.0\n0.0 1.0 0.0\n0.0 0.0 1.0\n").unwrap_err();
        assert!(e.message().contains("File must contain 12 float entries"));
        let e = read_mtx(b"1.0 0.0 0.0 0.0\n0.0 error 0.0 0.0\n0.0 0.0 1.0 0.0\n").unwrap_err();
        assert!(e.message().contains("File must contain all float entries"));
        let e = read_mtx(&[b' '; 2048]).unwrap_err();
        assert!(e.message().contains("File is too large"));
    }
}
