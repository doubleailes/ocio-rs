//! Sony Pictures Imageworks `spi3d` 3D LUT format (port of
//! `FileFormatSpi3D.cpp`).
//!
//! ```text
//! SPILUT 1.0
//! 3 3
//! 32 32 32
//! 0 0 0 0.0132509 0.0158522 0.0156622
//! 0 0 1 0.0136178 0.0158625 0.0336781
//! ...
//! ```

use super::utils::{
    from_chars_f32, lut3d_index_blue_fast, new_lut3d, scanf, IStream, MAX_3D_LUT_LENGTH,
};
use super::{bake_capability, capability, CachedFile, FileFormat, FormatInfo};
use crate::error::Result;
use crate::transforms::GroupTransform;
use crate::types::{BitDepth, Interpolation};

const MAX_LINE_SIZE: usize = 4096;

struct LocalFileFormat;

pub(crate) fn create() -> Box<dyn FileFormat> {
    Box::new(LocalFileFormat)
}

impl FileFormat for LocalFileFormat {
    fn format_info(&self) -> Vec<FormatInfo> {
        vec![FormatInfo {
            name: "spi3d",
            extension: "spi3d",
            capabilities: capability::READ | capability::BAKE,
            bake_capabilities: bake_capability::LUT3D,
        }]
    }

    fn read(&self, data: &[u8], file_name: &str, interp: Interpolation) -> Result<CachedFile> {
        let mut istream = IStream::new(data);

        // Read header information.
        let line = istream.getline_limited(MAX_LINE_SIZE);
        if !line.to_ascii_lowercase().starts_with("spilut") {
            crate::bail!(
                "Error parsing .spi3d file ({file_name}).  LUT does not appear to be valid spilut format. Expected 'SPILUT'.  Found: '{line}'."
            );
        }

        // TODO: Assert 2nd line is 3 3.
        istream.getline_limited(MAX_LINE_SIZE);

        // Get LUT size.
        let line = istream.getline_limited(MAX_LINE_SIZE);
        let (n, v) = scanf(&line, "%d %d %d");
        if n != 3 {
            crate::bail!("Error parsing .spi3d file ({file_name}). Error while reading LUT size. Found: '{line}'.");
        }
        let (r_size, g_size, b_size) = (v[0].as_int(), v[1].as_int(), v[2].as_int());

        // TODO: Support nonuniformly sized LUTs.
        if r_size != g_size || r_size != b_size {
            crate::bail!(
                "Error parsing .spi3d file ({file_name}). LUT size should be the same for all components. Found: '{line}'."
            );
        }

        if r_size < 2 || r_size as i64 > MAX_3D_LUT_LENGTH as i64 {
            crate::bail!(
                "Error parsing .spi3d file ({file_name}). LUT size must be between 2 and {MAX_3D_LUT_LENGTH}. Found: '{line}'."
            );
        }

        let size = r_size as usize;
        let mut lut = new_lut3d(size, interp, BitDepth::F32);

        // Parse table.
        let mut entries_remaining = size * size * size;
        let num_val = lut.values.len();
        let mut index_defined = vec![false; num_val];
        while istream.good() && entries_remaining > 0 {
            let line = istream.getline_limited(MAX_LINE_SIZE);

            let (n, v) = scanf(&line, "%d %d %d %63s %63s %63s");
            if n != 6 {
                continue;
            }
            let (ri, gi, bi) = (v[0].as_int(), v[1].as_int(), v[2].as_int());
            let (rs, gs, bs) = (v[3].as_str(), v[4].as_str(), v[5].as_str());
            let (red, green, blue) = match (from_chars_f32(rs), from_chars_f32(gs), from_chars_f32(bs)) {
                (Some(r), Some(g), Some(b)) => (r, g, b),
                _ => crate::bail!(
                    "Error parsing .spi3d file ({file_name}). Data is invalid. A color value is specified ({rs} {gs} {bs}) that cannot be parsed as a floating-point triplet."
                ),
            };

            if ri < 0 || ri >= r_size || gi < 0 || gi >= g_size || bi < 0 || bi >= b_size {
                crate::bail!(
                    "Error parsing .spi3d file ({file_name}). Data is invalid. A LUT entry is specified ({ri} {gi} {bi}) that falls outside of the cube."
                );
            }
            let index = lut3d_index_blue_fast(ri as usize, gi as usize, bi as usize, size, size);

            lut.values[index] = red;
            lut.values[index + 1] = green;
            lut.values[index + 2] = blue;

            if !index_defined[index] {
                entries_remaining -= 1;
                index_defined[index] = true;
            } else {
                crate::bail!(
                    "Error parsing .spi3d file ({file_name}). Data is invalid. A LUT entry is specified multiple times ({ri} {gi} {bi})."
                );
            }
        }

        // Have we fully populated the table?
        if entries_remaining > 0 {
            crate::bail!("Error parsing .spi3d file ({file_name}). Not enough entries found.");
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

    fn test_file(name: &str) -> Vec<u8> {
        let path = format!("{}/tests/data/files/{}", env!("CARGO_MANIFEST_DIR"), name);
        std::fs::read(path).expect("test file")
    }

    fn read_spi3d(content: &str) -> Result<CachedFile> {
        LocalFileFormat.read(content.as_bytes(), "Memory File", Interpolation::Default)
    }

    fn check_error(content: &str, what: &str) {
        match read_spi3d(content) {
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
        assert_eq!(info[0].name, "spi3d");
        assert_eq!(info[0].extension, "spi3d");
        assert_eq!(info[0].capabilities, capability::READ | capability::BAKE);
    }

    #[test]
    fn test() {
        let name = "spi_ocio_srgb_test.spi3d";
        let file = LocalFileFormat
            .read(&test_file(name), name, Interpolation::Default)
            .unwrap();
        assert_eq!(file.group.num_transforms(), 1);
        let Transform::Lut3D(lut) = &file.group.transforms[0] else {
            panic!("expected a Lut3D")
        };
        assert_eq!(lut.grid_size, 32);
        assert_eq!(lut.values.len(), 32 * 32 * 32 * 3);
        assert_eq!(lut.file_output_bit_depth, BitDepth::F32);

        assert_eq!(lut.values[0], 0.040157f32);
        assert_eq!(lut.values[1], 0.038904f32);
        assert_eq!(lut.values[2], 0.028316f32);
        // 10 2 12
        assert_eq!(lut.values[30948], 0.102161f32);
        assert_eq!(lut.values[30949], 0.032187f32);
        assert_eq!(lut.values[30950], 0.175453f32);
    }

    const SAMPLE_NO_ERROR: &str = "SPILUT 1.0\n3 3\n2 2 2\n0 0 0 0.0 0.0 0.0\n0 0 1 0.0 0.0 0.9\n0 1 0 0.0 0.7 0.0\n0 1 1 0.0 0.8 0.8\n1 0 0 0.7 0.0 0.1\n1 0 1 0.7 0.6 0.1\n1 1 0 0.6 0.7 0.1\n1 1 1 0.6 0.7 0.7\n";

    #[test]
    fn read_failure() {
        assert!(read_spi3d(SAMPLE_NO_ERROR).is_ok());
        // Wrong first line.
        check_error(
            &SAMPLE_NO_ERROR.replace("SPILUT", "SPI LUT"),
            "Expected 'SPILUT'",
        );
        // 3 line is not 3 ints.
        check_error(
            &SAMPLE_NO_ERROR.replace("2 2 2\n", "42\n"),
            "Error while reading LUT size",
        );
        // Index out of range.
        check_error(
            &SAMPLE_NO_ERROR.replace("0 0 0 0.0 0.0 0.0", "0 2 0 0.0 0.0 0.0"),
            "that falls outside of the cube",
        );
        // Duplicated indices.
        check_error(
            "SPILUT 1.0\n3 3\n2 2 2\n0 0 0 0.0 0.0 0.0\n0 0 1 0.0 0.0 0.9\n0 0 1 0.0 0.0 0.9\n0 1 0 0.0 0.7 0.0\n0 1 1 0.0 0.8 0.8\n1 0 1 0.7 0.6 0.1\n1 1 0 0.6 0.7 0.1\n1 1 1 0.6 0.7 0.7\n",
            "A LUT entry is specified multiple times",
        );
        // Not enough entries.
        check_error(
            "SPILUT 1.0\n3 3\n2 2 2\n0 0 0 0.0 0.0 0.0\n0 0 1 0.0 0.0 0.9\n0 1 0 0.0 0.7 0.0\n0 1 1 0.0 0.8 0.8\n1 0 1 0.7 0.6 0.1\n1 1 0 0.6 0.7 0.1\n1 1 1 0.6 0.7 0.7\n",
            "Not enough entries found",
        );
        // Size out of range.
        check_error(
            &SAMPLE_NO_ERROR.replace("2 2 2\n", "1 1 1\n"),
            "LUT size must be between 2 and 129",
        );
        check_error(
            &SAMPLE_NO_ERROR.replace("2 2 2\n", "2 3 2\n"),
            "LUT size should be the same",
        );
    }

    #[test]
    fn values_order() {
        let file = read_spi3d(SAMPLE_NO_ERROR).unwrap();
        let Transform::Lut3D(lut) = &file.group.transforms[0] else {
            panic!()
        };
        assert_eq!(lut.value(0, 0, 1), [0.0, 0.0, 0.9]);
        assert_eq!(lut.value(1, 0, 0), [0.7, 0.0, 0.1]);
    }

    #[test]
    fn lut_interpolation_option() {
        let interp_of = |interp| {
            let file = LocalFileFormat
                .read(SAMPLE_NO_ERROR.as_bytes(), "", interp)
                .unwrap();
            let Transform::Lut3D(lut) = &file.group.transforms[0] else {
                panic!()
            };
            lut.interpolation
        };
        assert_eq!(interp_of(Interpolation::Best), Interpolation::Best);
        assert_eq!(interp_of(Interpolation::Default), Interpolation::Default);
        assert_eq!(
            interp_of(Interpolation::Tetrahedral),
            Interpolation::Tetrahedral
        );
        // Not supported by a 3D LUT: default is used.
        assert_eq!(interp_of(Interpolation::Cubic), Interpolation::Default);
    }

    #[test]
    #[ignore = "needs-merge"]
    fn lut_interpolation_option_processor() {
        use crate::transforms::FileTransform;
        let config = crate::Config::create_raw();
        let path = format!(
            "{}/tests/data/files/spi_ocio_srgb_test.spi3d",
            env!("CARGO_MANIFEST_DIR")
        );
        for (interp, expected) in [
            (Interpolation::Best, Interpolation::Best),
            (Interpolation::Default, Interpolation::Default),
            (Interpolation::Cubic, Interpolation::Default),
        ] {
            let mut ft = FileTransform::new(&path);
            ft.interpolation = interp;
            let proc = config
                .get_processor_for_transform(
                    &Transform::File(ft),
                    crate::TransformDirection::Forward,
                )
                .unwrap();
            let group = proc.create_group_transform();
            assert_eq!(group.num_transforms(), 1);
            let Transform::Lut3D(lut) = &group.transforms[0] else {
                panic!()
            };
            assert_eq!(lut.interpolation, expected);
        }
    }
}
