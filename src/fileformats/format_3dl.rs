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
    alloc_bake_buffer, bake_identity_lut3d, bit_depth_max_value,
    get_3d_lut_edge_len_from_num_pixels, new_lut1d, new_lut3d, split_by_white_spaces,
    string_vec_to_int_vec, trim, IStream, MAX_3D_LUT_LENGTH,
};
use super::{bake_capability, capability, CachedFile, FileFormat, FormatInfo};
use crate::baker::{input_to_target_processor, Baker};
use crate::error::Result;
use crate::ops::lut1d::generate_identity_lut1d;
use crate::ops::lut3d::Lut3DOrder;
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

    fn bake(&self, baker: &Baker, format_name: &str) -> Result<Vec<u8>> {
        const SHAPER_BIT_DEPTH: i32 = 10;
        const CUBE_BIT_DEPTH: i32 = 12;

        // NOTE: This code is very old, Lustre and Flame have long been able
        //       to support much larger cube sizes. Furthermore there is no
        //       need to use the legacy 3dl format since CLF/CTF is supported.
        let default_cube_size = match format_name {
            "lustre" => 33,
            "flame" => 17,
            _ => crate::bail!("Unknown 3dl format name, '{format_name}'."),
        };
        let lustre = format_name == "lustre";

        // Smallest cube is 2x2x2.
        let cube_size = baker.cube_size().unwrap_or(default_cube_size).max(2);
        let shaper_size = baker.shaper_size().unwrap_or(cube_size);

        let mut cube_data = bake_identity_lut3d(cube_size, Lut3DOrder::FastBlue)?;
        input_to_target_processor(baker)?.apply_rgb_slice(&mut cube_data);

        // Write out the file. For maximum compatibility with other apps,
        // the shaper is not utilized and no metadata is written.
        let mut out = String::new();

        if lustre {
            let mesh_input_bit_depth = cube_dimension_len_to_lustre_bit_depth(cube_size);
            out.push_str("3DMESH\n");
            out.push_str(&format!("Mesh {mesh_input_bit_depth} {CUBE_BIT_DEPTH}\n"));
        }

        let mut shaper_data = alloc_bake_buffer(Some(shaper_size))?;
        generate_identity_lut1d(&mut shaper_data, shaper_size, 1);

        let shaper_scale = max_value_from_integer_bit_depth(SHAPER_BIT_DEPTH) as f32;
        let shaper_line: Vec<String> = shaper_data
            .iter()
            .map(|&v| clamped_int_from_norm_float(v, shaper_scale).to_string())
            .collect();
        out.push_str(&shaper_line.join(" "));
        out.push('\n');

        // Write out the 3D cube.
        let cube_scale = max_value_from_integer_bit_depth(CUBE_BIT_DEPTH) as f32;
        for rgb in cube_data.chunks_exact(3) {
            let r = clamped_int_from_norm_float(rgb[0], cube_scale);
            let g = clamped_int_from_norm_float(rgb[1], cube_scale);
            let b = clamped_int_from_norm_float(rgb[2], cube_scale);
            out.push_str(&format!("{r} {g} {b}\n"));
        }
        out.push('\n');

        if lustre {
            out.push_str("LUT8\n");
            out.push_str("gamma 1.0\n");
        }

        Ok(out.into_bytes())
    }
}

/// Port of `GetMaxValueFromIntegerBitDepth`.
fn max_value_from_integer_bit_depth(bit_depth: i32) -> i32 {
    2f64.powi(bit_depth) as i32 - 1
}

/// Port of `GetClampedIntFromNormFloat`: clamp to `[0, 1]` (NaN giving 0,
/// as `std::min(std::max(0.0f, val), 1.0f)` does), scale and round.
fn clamped_int_from_norm_float(val: f32, scale: f32) -> i32 {
    let val = if 0.0 < val { val } else { 0.0 };
    let val = if 1.0 < val { 1.0 } else { val };
    (val * scale).round() as i32
}

/// Port of `CubeDimensionLenToLustreBitDepth` (65 -> 6, 33 -> 5, 17 -> 4).
fn cube_dimension_len_to_lustre_bit_depth(size: usize) -> i32 {
    // Single precision, as `logf` is used.
    let logval = (size.saturating_sub(1) as f32).ln() / 2f32.ln();
    logval as i32
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

    const BAKE_CONFIG: &str = r#"ocio_profile_version: 2

roles:
  reference: lnf
  default: lnf

colorspaces:
  - !<ColorSpace>
    name: lnf
    family: lnf

  - !<ColorSpace>
    name: target
    family: target
    from_scene_reference: !<CDLTransform> {offset: [0, 0.1, 0.2]}
"#;

    #[test]
    fn bake() {
        use crate::fileformats::utils::bake_test_utils::{bake, baker};

        let mut b = baker(BAKE_CONFIG, "flame");
        // The metadata is not written.
        b.format_metadata_mut()
            .add_child_element(crate::types::METADATA_DESCRIPTION, "MetaData not written");
        b.set_input_space("lnf");
        b.set_target_space("target");
        b.set_shaper_size(Some(10));
        b.set_cube_size(Some(2));
        let flame = bake(&b);

        b.set_format("lustre").unwrap();
        let lustre = bake(&b);

        let expected_body = "0 114 227 341 455 568 682 796 909 1023\n\
                             0 410 819\n\
                             0 410 4095\n\
                             0 4095 819\n\
                             0 4095 4095\n\
                             4095 410 819\n\
                             4095 410 4095\n\
                             4095 4095 819\n\
                             4095 4095 4095\n\
                             \n";
        assert_eq!(flame, expected_body);
        assert_eq!(
            lustre,
            format!("3DMESH\nMesh 0 12\n{expected_body}LUT8\ngamma 1.0\n")
        );
    }

    #[test]
    fn bake_defaults_and_errors() {
        use crate::fileformats::utils::bake_test_utils::{bake, baker};

        let mut b = baker(BAKE_CONFIG, "lustre");
        b.set_input_space("lnf");
        b.set_target_space("lnf");
        let lustre = bake(&b);
        let lines: Vec<&str> = lustre.lines().collect();
        // Default cube size of 33 for lustre (5 bits).
        assert_eq!(lines[1], "Mesh 5 12");
        assert_eq!(lines[2].split(' ').count(), 33);
        assert_eq!(lines.len(), 2 + 1 + 33 * 33 * 33 + 1 + 2);
        assert_eq!(lines[3], "0 0 0");
        assert_eq!(lines[4], "0 0 128");
        assert_eq!(lines[3 + 33 * 33 * 33 - 1], "4095 4095 4095");

        b.set_format("flame").unwrap();
        let flame = bake(&b);
        let lines: Vec<&str> = flame.lines().collect();
        // Default cube size of 17 for flame.
        assert_eq!(lines[0].split(' ').count(), 17);
        assert_eq!(lines[0].split(' ').next_back(), Some("1023"));
        assert_eq!(lines.len(), 1 + 17 * 17 * 17 + 1);

        // Other sizes (65 -> 6 bits, 17 -> 4 bits).
        assert_eq!(cube_dimension_len_to_lustre_bit_depth(65), 6);
        assert_eq!(cube_dimension_len_to_lustre_bit_depth(33), 5);
        assert_eq!(cube_dimension_len_to_lustre_bit_depth(17), 4);
        assert_eq!(cube_dimension_len_to_lustre_bit_depth(2), 0);

        // Clamping.
        assert_eq!(clamped_int_from_norm_float(-1.0, 4095.0), 0);
        assert_eq!(clamped_int_from_norm_float(2.0, 4095.0), 4095);
        assert_eq!(clamped_int_from_norm_float(f32::NAN, 4095.0), 0);
        assert_eq!(clamped_int_from_norm_float(0.5, 1023.0), 512);

        // Unknown format name.
        let e = LocalFileFormat.bake(&b, "unknown").unwrap_err();
        assert_eq!(e.message(), "Unknown 3dl format name, 'unknown'.");
    }

    #[test]
    fn bake_round_trip() {
        use crate::fileformats::utils::bake_test_utils::{baker, check_round_trip};

        let config = r#"ocio_profile_version: 2

roles:
  reference: lnf
  default: lnf

colorspaces:
  - !<ColorSpace>
    name: lnf

  - !<ColorSpace>
    name: target
    from_scene_reference: !<CDLTransform> {slope: [0.5, 0.6, 0.7], sat: 0.8}
"#;
        for format in ["flame", "lustre"] {
            let mut b = baker(config, format);
            b.set_input_space("lnf");
            b.set_target_space("target");
            let samples = [[0.0, 0.0, 0.0], [0.25, 0.5, 0.75], [1.0, 1.0, 1.0]];
            // 12 bits quantization.
            check_round_trip(&b, &samples, 1e-3);
        }
    }
}
