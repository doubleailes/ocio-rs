//! Discreet / Autodesk Flame and Lustre `3dl` 3D LUT format (port of
//! `FileFormat3DL.cpp`).
//!
//! A loose interpretation of the format is used to allow other 3D LUTs that
//! look similar, but don't strictly adhere to the real definition:
//!
//! * lines starting with text or `#` are skipped,
//! * a line of more than 3 integers is the 1D shaper LUT,
//! * all remaining lines of 3 integers are the 3D LUT data (blue fastest),
//!   the cube size being determined from the number of entries.
//!
//! The bit depths of the shaper LUT and of the 3D LUT are inferred from
//! their maximum values and need not be the same.
//!
//! ```text
//! #Tokens required by applications - do not edit
//! 3DMESH
//! Mesh 4 12
//! 0 64 128 192 256 320 384 448 512 576 640 704 768 832 896 960 1023
//!
//! 0 17 17
//! 0 0 88
//! ...
//! ```

use super::utils::{
    bit_depth_max_value, get_3d_lut_edge_len_from_num_pixels, new_lut1d, new_lut3d,
    split_by_white_spaces, string_vec_to_int_vec, trim, IStream, MAX_3D_LUT_LENGTH,
};
use super::{bake_capability, capability, CachedFile, FileFormat, FormatInfo};
use crate::error::Result;
use crate::transforms::GroupTransform;
use crate::types::{BitDepth, Interpolation};

const MAX_LINE_SIZE: usize = 4096;

/// If the maximum value of a LUT is lower than this, it's likely not an
/// integer format, and thus not a 3dl file.
const FORMAT3DL_CODEVALUE_LOWEST_PLAUSIBLE_MAXINT: i32 = 128;

struct LocalFileFormat;

pub(crate) fn create() -> Box<dyn FileFormat> {
    Box::new(LocalFileFormat)
}

/// Infer the bit depth of a LUT from its maximum value (port of
/// `GetLikelyLutBitDepth`).
///
/// A 2x overshoot is allowed so sizes from 1/2 max to 2x max are valid:
///
/// | file   | expected max | decoded if max in |
/// |--------|--------------|-------------------|
/// | 8-bit  | 255          | [0, 511]          |
/// | 10-bit | 1023         | [512, 2047]       |
/// | 12-bit | 4095         | [2048, 8191]      |
/// | 14-bit | 16383        | [8192, 32767]     |
/// | 16-bit | 65535        | [32768, 131071+]  |
pub(crate) fn get_likely_lut_bit_depth(testval: i32) -> i32 {
    const MIN_BIT_DEPTH: i32 = 8;
    const MAX_BIT_DEPTH: i32 = 16;

    if testval < 0 {
        return -1;
    }

    // Only test even bit depths.
    let mut bit_depth = MIN_BIT_DEPTH;
    while bit_depth <= MAX_BIT_DEPTH {
        let maxcode = 2i64.pow(bit_depth as u32);
        let adjusted_max = maxcode * 2 - 1;
        if testval as i64 <= adjusted_max {
            // Since 14-bit scaling is not used in practice, if the max is
            // more than 8192, they are likely 16-bit values.
            if bit_depth == 14 {
                return 16;
            }
            return bit_depth;
        }
        bit_depth += 2;
    }
    MAX_BIT_DEPTH
}

/// Port of `GetOCIOBitdepth`.
fn get_ocio_bitdepth(bitdepth: i32) -> BitDepth {
    match bitdepth {
        8 => BitDepth::UInt8,
        10 => BitDepth::UInt10,
        12 => BitDepth::UInt12,
        16 => BitDepth::UInt16,
        _ => BitDepth::Unknown,
    }
}

/// The shaper LUT part of the format was never properly documented and its
/// usage is quite inconsistent, so a loose tolerance is used for what
/// constitutes an identity.
fn is_identity(rawshaper: &[i32], out_bit_depth: BitDepth) -> Result<bool> {
    let dim = rawshaper.len();
    let step_value = bit_depth_max_value(out_bit_depth)? as f32 / (dim as f32 - 1.0);
    Ok(rawshaper
        .iter()
        .enumerate()
        .all(|(i, &v)| (i as f32 * step_value - v as f32).abs() < 2.0))
}

impl FileFormat for LocalFileFormat {
    fn format_info(&self) -> Vec<FormatInfo> {
        vec![
            FormatInfo {
                name: "flame",
                extension: "3dl",
                capabilities: capability::READ | capability::BAKE,
                bake_capabilities: bake_capability::LUT3D,
            },
            FormatInfo {
                name: "lustre",
                extension: "3dl",
                capabilities: capability::READ | capability::BAKE,
                bake_capabilities: bake_capability::LUT3D,
            },
        ]
    }

    fn read(
        &self,
        data: &[u8],
        _original_file_name: &str,
        interp: Interpolation,
    ) -> Result<CachedFile> {
        let mut rawshaper: Vec<i32> = Vec::new();
        let mut raw3d: Vec<i32> = Vec::new();
        let mut lut3dmax = 0i32;

        // Parse the file 3D LUT data to an int array.
        let mut istream = IStream::new(data);
        let mut line_number = 0;
        while istream.good() {
            let line_buffer = istream.getline_limited(MAX_LINE_SIZE);
            line_number += 1;

            // Strip and split the line.
            let parts = split_by_white_spaces(trim(&line_buffer));
            if parts.is_empty() {
                continue;
            }
            if parts[0].starts_with('#') {
                continue;
            }
            if parts[0].starts_with('<') {
                // Format error: reject files that could be formatted as xml.
                crate::bail!(
                    "Error parsing .3dl file. Not expecting a line starting with \"<\".Line ({line_number}): '{line_buffer}'."
                );
            }

            // If we haven't found a list of ints, continue. Some keywords are
            // valid (3DMESH, mesh, gamma, LUT*) but others could be format
            // errors. To preserve v1 behavior, don't reject them.
            let Some(tmp_data) = string_vec_to_int_vec(&parts) else {
                continue;
            };

            if tmp_data.len() > 3 {
                // If we've found more than 3 ints, and don't have a shaper
                // LUT yet, we've got it!
                if rawshaper.is_empty() {
                    rawshaper.extend_from_slice(&tmp_data);
                } else {
                    // Format error, more than 1 shaper LUT.
                    crate::bail!(
                        "Error parsing .3dl file. Appears to contain more than 1 shaper LUT.Line ({line_number}): '{line_buffer}'."
                    );
                }
            } else if tmp_data.len() == 3 {
                // If we've found 3 ints, add it to our 3D LUT.
                if raw3d.len() > MAX_3D_LUT_LENGTH * MAX_3D_LUT_LENGTH * MAX_3D_LUT_LENGTH * 3 {
                    crate::bail!("Error parsing .3dl file. Too many 3D LUT entries found.");
                }
                raw3d.extend_from_slice(&tmp_data);
                // Find the maximum 3D LUT value to infer bit-depth.
                lut3dmax = lut3dmax.max(tmp_data[0]).max(tmp_data[1]).max(tmp_data[2]);
            } else {
                // Format error, line with 1 or 2 int.
                crate::bail!(
                    "Error parsing .3dl file. Invalid line with less than 3 values.Line ({line_number}): '{line_buffer}'."
                );
            }
        }

        if raw3d.is_empty() && rawshaper.is_empty() {
            crate::bail!("Error parsing .3dl file. Does not appear to contain a valid shaper LUT or a 3D LUT.");
        }

        let mut group = GroupTransform::new();

        // Interpret the shaper LUT.
        if !rawshaper.is_empty() {
            // Find the maximum shaper LUT value to infer bit-depth.
            let shapermax = rawshaper.iter().copied().fold(0, i32::max);

            if shapermax < FORMAT3DL_CODEVALUE_LOWEST_PLAUSIBLE_MAXINT {
                crate::bail!(
                    "Error parsing .3dl file. The maximum shaper LUT value, {shapermax}, is unreasonably low. This LUT is probably not a .3dl file, but instead a related format that shares a similar structure."
                );
            }

            let shaperbitdepth = get_likely_lut_bit_depth(shapermax);
            if shaperbitdepth < 0 {
                crate::bail!(
                    "Error parsing .3dl file. The maximum shaper LUT value, {shapermax}, does not correspond to any likely bit depth. Please confirm source file is valid."
                );
            }

            let out1d_bd = get_ocio_bitdepth(shaperbitdepth);
            if out1d_bd == BitDepth::Unknown {
                crate::bail!(
                    "Error parsing .3dl file. The shaper LUT bit depth is not known. Please confirm source file is valid."
                );
            }

            if !is_identity(&rawshaper, out1d_bd)? {
                let mut lut = new_lut1d(rawshaper.len(), false, interp, out1d_bd);
                let scale = bit_depth_max_value(out1d_bd)? as f32;
                for (i, &v) in rawshaper.iter().enumerate() {
                    let value = v as f32 / scale;
                    lut.values[3 * i] = value;
                    lut.values[3 * i + 1] = value;
                    lut.values[3 * i + 2] = value;
                }
                group.append(lut);
            }
        }

        // Interpret the parsed data.
        if !raw3d.is_empty() {
            // lut3dmax has been stored while reading values.
            if lut3dmax < FORMAT3DL_CODEVALUE_LOWEST_PLAUSIBLE_MAXINT {
                crate::bail!(
                    "Error parsing .3dl file.The maximum 3D LUT value, {lut3dmax}, is unreasonably low. This LUT is probably not a .3dl file, but instead a related format that shares a similar structure."
                );
            }

            let lut3dbitdepth = get_likely_lut_bit_depth(lut3dmax);
            if lut3dbitdepth < 0 {
                crate::bail!(
                    "Error parsing .3dl file.The maximum 3D LUT value, {lut3dmax}, does not correspond to any likely bit depth. Please confirm source file is valid."
                );
            }

            // Interpret the int array as a 3D LUT.
            let lut_edge_len = get_3d_lut_edge_len_from_num_pixels((raw3d.len() / 3) as i64)?;

            // The 3dl format stores the LUT entries in blue-fastest order,
            // which is the same order used by Lut3DTransform, so no
            // transposition of LUT entries is needed in this case.
            let out3d_bd = get_ocio_bitdepth(lut3dbitdepth);
            let mut lut = new_lut3d(lut_edge_len, interp, out3d_bd);
            let scale = bit_depth_max_value(out3d_bd)? as f32;
            for (dst, &v) in lut.values.iter_mut().zip(raw3d.iter()) {
                *dst = v as f32 / scale;
            }
            group.append(lut);
        }

        Ok(CachedFile::new(group))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transforms::Transform;

    fn load_lut_file(name: &str) -> Result<CachedFile> {
        let path = format!("{}/tests/data/files/{}", env!("CARGO_MANIFEST_DIR"), name);
        let data = std::fs::read(&path).expect("test file");
        LocalFileFormat.read(&data, &path, Interpolation::Default)
    }

    fn read_3dl(content: &str) -> Result<CachedFile> {
        LocalFileFormat.read(content.as_bytes(), "Memory File", Interpolation::Default)
    }

    #[test]
    fn format_info() {
        let info = LocalFileFormat.format_info();
        assert_eq!(info.len(), 2);
        assert_eq!(info[0].name, "flame");
        assert_eq!(info[1].name, "lustre");
        assert_eq!(info[0].extension, "3dl");
        assert_eq!(info[1].extension, "3dl");
        assert_eq!(info[0].capabilities, capability::READ | capability::BAKE);
        assert_eq!(info[1].capabilities, capability::READ | capability::BAKE);
    }

    #[test]
    fn get_likely_lut_bitdepth() {
        assert_eq!(get_likely_lut_bit_depth(-1), -1);

        assert_eq!(get_likely_lut_bit_depth(0), 8);
        assert_eq!(get_likely_lut_bit_depth(1), 8);
        assert_eq!(get_likely_lut_bit_depth(255), 8);
        assert_eq!(get_likely_lut_bit_depth(256), 8);
        assert_eq!(get_likely_lut_bit_depth(511), 8);

        assert_eq!(get_likely_lut_bit_depth(512), 10);
        assert_eq!(get_likely_lut_bit_depth(1023), 10);
        assert_eq!(get_likely_lut_bit_depth(1024), 10);
        assert_eq!(get_likely_lut_bit_depth(2047), 10);

        assert_eq!(get_likely_lut_bit_depth(2048), 12);
        assert_eq!(get_likely_lut_bit_depth(4095), 12);
        assert_eq!(get_likely_lut_bit_depth(4096), 12);
        assert_eq!(get_likely_lut_bit_depth(8191), 12);

        assert_eq!(get_likely_lut_bit_depth(16383), 16);

        assert_eq!(get_likely_lut_bit_depth(65535), 16);
        assert_eq!(get_likely_lut_bit_depth(65536), 16);
        assert_eq!(get_likely_lut_bit_depth(131071), 16);

        assert_eq!(get_likely_lut_bit_depth(131072), 16);
    }

    #[test]
    fn load() {
        let file = load_lut_file("discreet-3d-lut.3dl").unwrap();
        assert_eq!(file.group.num_transforms(), 1);
        let Transform::Lut3D(lut) = &file.group.transforms[0] else {
            panic!("expected a Lut3D")
        };
        assert_eq!(lut.file_output_bit_depth, BitDepth::UInt12);
        assert_eq!(lut.grid_size, 17);

        let scale = 4095.0f32;
        // File and LUT are using the same order.
        // 41: 54 323 597
        assert_eq!(scale * lut.values[41 * 3], 54.0);
        assert_eq!(scale * lut.values[41 * 3 + 1], 323.0);
        assert_eq!(scale * lut.values[41 * 3 + 2], 597.0);

        // 4591: 4025 3426 0
        assert_eq!(scale * lut.values[4591 * 3], 4025.0);
        assert_eq!(scale * lut.values[4591 * 3 + 1], 3426.0);
        assert_eq!(scale * lut.values[4591 * 3 + 2], 0.0);

        let e = load_lut_file("error_truncated_file.3dl").unwrap_err();
        assert!(
            e.message().contains("Cannot infer 3D LUT size"),
            "{}",
            e.message()
        );
    }

    #[test]
    fn load_others() {
        let file = load_lut_file("lustre_33x33x33.3dl").unwrap();
        let Transform::Lut3D(lut) = file.group.transforms.last().unwrap() else {
            panic!()
        };
        assert_eq!(lut.grid_size, 33);
        let file = load_lut_file("crosstalk.3dl").unwrap();
        assert!(matches!(
            file.group.transforms.last().unwrap(),
            Transform::Lut3D(_)
        ));
    }

    #[test]
    fn parse_1d() {
        // Rounding down test.
        let file = read_3dl(
            "#Tokens required by applications - do not edit\n\n3DMESH\nMesh 4 10\n0 63 127 191 255 319 383 447 511 575 639 703 767 831 895 959 1023\n",
        )
        .unwrap();
        assert_eq!(file.group.num_transforms(), 0);

        // Rounding up test.
        let file = read_3dl(
            "#Tokens required by applications - do not edit\n\n3DMESH\nMesh 4 10\n0 64 128 192 256 320 384 448 512 576 640 704 768 832 896 960 1023\n",
        )
        .unwrap();
        assert_eq!(file.group.num_transforms(), 0);

        // Not an identity test.
        let file = read_3dl(
            "#Tokens required by applications - do not edit\n\n3DMESH\nMesh 4 10\n0 64 128 192 256 320 384 448 512 576 640 704 768 832 896 960 1020\n",
        )
        .unwrap();
        assert_eq!(file.group.num_transforms(), 1);
        let Transform::Lut1D(lut) = &file.group.transforms[0] else {
            panic!("expected a Lut1D")
        };
        assert_eq!(lut.file_output_bit_depth, BitDepth::UInt10);
        assert_eq!(lut.length(), 17);
        assert_eq!(lut.values[3 * 16], 1020.0 / 1023.0);
    }

    #[test]
    fn read_failure() {
        let e = read_3dl("<xml>\n").unwrap_err();
        assert!(e.message().contains("Not expecting a line starting with"));
        let e = read_3dl("0 1\n").unwrap_err();
        assert!(e.message().contains("Invalid line with less than 3 values"));
        let e = read_3dl("0 1 2 3\n0 1 2 3\n").unwrap_err();
        assert!(e.message().contains("more than 1 shaper LUT"));
        let e = read_3dl("# only comments\n").unwrap_err();
        assert!(e
            .message()
            .contains("Does not appear to contain a valid shaper LUT or a 3D LUT"));
        let e = read_3dl("0 0 0\n0 0 1\n").unwrap_err();
        assert!(e.message().contains("is unreasonably low"));
    }
}
