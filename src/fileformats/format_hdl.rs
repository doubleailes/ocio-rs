//! Houdini `lut` format (port of `FileFormatHDL.cpp`).
//!
//! See <http://www.sidefx.com/docs/hdk11.0/hdk_io_lut.html>. Supported
//! types:
//!
//! * `C`: 1D LUT (partial support),
//! * `3D`: 3D LUT (red fastest),
//! * `3D+1D`: 3D LUT with a 1D prelut.
//!
//! TODO (as in OCIO): add support for the other 1D types (R, G, B, A, RGB,
//! RGBA, All) and for the `Sampling` tag.

use super::utils::{
    bake_identity_lut1d, bake_identity_lut3d, bake_linear_scale_lut1d, format_fixed6,
    from_chars_f32, min_max_matrix_f32, new_lut1d, new_lut3d, set_lut3d_from_red_fastest,
    split_by_white_spaces, string_to_float, string_to_int, trim, IStream, MAX_1D_LUT_LENGTH,
    MAX_3D_LUT_LENGTH,
};
use super::{bake_capability, capability, CachedFile, FileFormat, FormatInfo};
use crate::baker::{
    input_to_shaper_processor, input_to_target_processor, shaper_range, shaper_to_target_processor,
    Baker,
};
use crate::error::{Error, Result};
use crate::ops::lut3d::Lut3DOrder;
use crate::transforms::{GroupTransform, Lut1DTransform};
use crate::types::{BitDepth, Interpolation};
use std::collections::BTreeMap;

type StringToStringVecMap = BTreeMap<String, Vec<String>>;
type StringToFloatVecMap = BTreeMap<String, Vec<f32>>;

/// Read the headers into key / values pairs, stopping after the `LUT:` line.
fn read_headers(istream: &mut IStream) -> StringToStringVecMap {
    let mut headers = StringToStringVecMap::new();
    while let Some(line) = istream.nextline() {
        // Remove trailing/leading white spaces, lower-case and split into words.
        let mut chunks = split_by_white_spaces(&trim(&line).to_ascii_lowercase());

        // Skip empty lines.
        if chunks.is_empty() {
            continue;
        }

        // Stop looking for headers at the "LUT:" line.
        if chunks[0] == "lut:" {
            break;
        }

        // Use first index as key, and remove it from the value.
        let key = chunks.remove(0);
        headers.insert(key, chunks);
    }
    headers
}

/// Grab `key` from the headers. Fails if not found, or if the number of
/// values is not between `min_vals` and `max_vals`.
fn find_header_item<'a>(
    headers: &'a StringToStringVecMap,
    key: &str,
    min_vals: usize,
    max_vals: usize,
) -> Result<&'a [String]> {
    let Some(values) = headers.get(key) else {
        crate::bail!("'{key}' line not found");
    };

    if values.len() < min_vals || values.len() > max_vals {
        let mut os = format!(
            "Incorrect number of chunks ({}) after '{}' line, expected ",
            values.len(),
            key
        );
        if min_vals == max_vals {
            os.push_str(&min_vals.to_string());
        } else {
            os.push_str(&format!("between {min_vals} and {max_vals}"));
        }
        return Err(Error::msg(os));
    }
    Ok(values)
}

/// Crudely parse the LUTs: just grab a series of floats for `Pre{...}`,
/// `3d{...}` etc.
fn read_luts(istream: &mut IStream) -> Result<StringToFloatVecMap> {
    let mut lut_values = StringToFloatVecMap::new();
    let mut inlut = false;
    let mut lutname = String::new();

    while let Some(word) = istream.next_word() {
        if !inlut {
            if word == "{" {
                // Lone "{" is for a 3D.
                inlut = true;
                lutname = "3d".to_string();
            } else {
                // Named LUT, e.g. "Pre {".
                inlut = true;
                lutname = word.to_ascii_lowercase();

                // Ensure next word is "{".
                let nextword = istream.next_word().unwrap_or_default();
                if nextword != "{" {
                    crate::bail!(
                        "Malformed LUT - Unknown word '{word}' after LUT name '{nextword}'"
                    );
                }
            }
        } else if word == "}" {
            // End of LUT.
            inlut = false;
            lutname.clear();
        } else {
            match from_chars_f32(&word) {
                Some(v) => {
                    let values = lut_values.entry(lutname.clone()).or_default();
                    // Cap per-LUT entry count: max is Max3DLUTLength^3 * 3.
                    if values.len() > MAX_3D_LUT_LENGTH * MAX_3D_LUT_LENGTH * MAX_3D_LUT_LENGTH * 3
                    {
                        crate::bail!("Too many values in {lutname} LUT block");
                    }
                    values.push(v);
                }
                None => crate::bail!("Invalid float value in {lutname} LUT, '{word}'"),
            }
        }
    }
    Ok(lut_values)
}

/// A 1D LUT using the same values for all channels.
fn make_lut1d(values: &[f32], interp: Interpolation) -> Lut1DTransform {
    let mut lut = new_lut1d(values.len(), false, interp, BitDepth::F32);
    for (i, &v) in values.iter().enumerate() {
        lut.values[3 * i] = v;
        lut.values[3 * i + 1] = v;
        lut.values[3 * i + 2] = v;
    }
    lut
}

struct LocalFileFormat;

pub(crate) fn create() -> Box<dyn FileFormat> {
    Box::new(LocalFileFormat)
}

impl FileFormat for LocalFileFormat {
    fn format_info(&self) -> Vec<FormatInfo> {
        vec![FormatInfo {
            name: "houdini",
            extension: "lut",
            capabilities: capability::READ | capability::BAKE,
            bake_capabilities: bake_capability::LUT3D
                | bake_capability::LUT1D
                | bake_capability::LUT1D_3D,
        }]
    }

    fn read(
        &self,
        data: &[u8],
        _original_file_name: &str,
        interp: Interpolation,
    ) -> Result<CachedFile> {
        let mut istream = IStream::new(data);

        // Read headers, ending after the "LUT:" line.
        let headers = read_headers(&mut istream);

        // "Version 3" - format version (currently one version number per
        // LUT type).
        find_header_item(&headers, "version", 1, 1)?;

        // "Format any" - bit depth of image the LUT should be applied to
        // (this is basically ignored).
        find_header_item(&headers, "format", 1, 1)?;

        // "Type 3d" - type of LUT.
        let hdltype = find_header_item(&headers, "type", 1, 1)?[0].clone();

        // "From 0.0 1.0" - range of input values.
        let value = find_header_item(&headers, "from", 2, 2)?;
        let (from_min, from_max) = match (string_to_float(&value[0]), string_to_float(&value[1])) {
            (Some(a), Some(b)) => (a, b),
            _ => crate::bail!(
                "Invalid float value(s) on 'From' line, '{}' and '{}'",
                value[0],
                value[1]
            ),
        };

        // "To 0.0 1.0" - range of values in LUT (e.g "0 255" to specify
        // values as 8-bit numbers, usually "0 1").
        let value = find_header_item(&headers, "to", 2, 2)?;
        if string_to_float(&value[0]).is_none() || string_to_float(&value[1]).is_none() {
            crate::bail!(
                "Invalid float value(s) on 'To' line, '{}' and '{}'",
                value[0],
                value[1]
            );
        }

        // "Black 0" and "White 1" - obsolete options, should be 0 and 1.
        let value = find_header_item(&headers, "black", 1, 1)?;
        if string_to_float(&value[0]).is_none() {
            crate::bail!("Invalid float value on 'Black' line, '{}'", value[0]);
        }
        let value = find_header_item(&headers, "white", 1, 1)?;
        if string_to_float(&value[0]).is_none() {
            crate::bail!("Invalid float value on 'White' line, '{}'", value[0]);
        }

        // Verify type is valid and supported - used to handle length
        // sensibly, and checking the LUT later.
        if hdltype != "3d" && hdltype != "3d+1d" && hdltype != "c" {
            crate::bail!("Unsupported Houdini LUT type: '{hdltype}'");
        }

        // "Length 2" or "Length 2 5" - either "[cube size]", or "[cube size]
        // [prelut size]".
        let mut size_3d: i32 = -1;
        let mut size_prelut: i32 = -1;
        let mut size_1d: i32 = -1;
        {
            let value = find_header_item(&headers, "length", 1, 2)?;
            let mut lut_sizes = Vec::new();
            for v in value {
                match string_to_int(v, false) {
                    Some(s) => lut_sizes.push(s),
                    None => crate::bail!("Invalid integer on 'Length' line: '{}'", value[0]),
                }
            }

            if hdltype == "3d" || hdltype == "3d+1d" {
                // Set cube size.
                size_3d = lut_sizes[0];
                if size_3d < 2 || size_3d as i64 > MAX_3D_LUT_LENGTH as i64 {
                    crate::bail!("3D LUT cube size must be between 2 and {MAX_3D_LUT_LENGTH}, found: {size_3d}");
                }
            }

            if hdltype == "c" {
                size_1d = lut_sizes[0];
                if size_1d < 2 || size_1d as i64 > MAX_1D_LUT_LENGTH as i64 {
                    crate::bail!(
                        "1D LUT size must be between 2 and {MAX_1D_LUT_LENGTH}, found: {size_1d}"
                    );
                }
            }

            if hdltype == "3d+1d" {
                size_prelut = lut_sizes.get(1).copied().unwrap_or(-1);
                if size_prelut < 2 || size_prelut as i64 > MAX_1D_LUT_LENGTH as i64 {
                    crate::bail!("Prelut size must be between 2 and {MAX_1D_LUT_LENGTH}, found: {size_prelut}");
                }
            }
        }

        // Read stuff after "LUT:".
        let lut_data = read_luts(&mut istream)?;

        let mut lut1d = None;
        let mut lut3d = None;

        if hdltype == "3d+1d" {
            // Read prelut.
            let Some(values) = lut_data.get("pre") else {
                crate::bail!("3D+1D LUT should contain Pre{{}} LUT section");
            };
            if size_prelut as i64 != values.len() as i64 {
                crate::bail!(
                    "Pre{{}} LUT was {} values long, expected {} values",
                    values.len(),
                    size_prelut
                );
            }
            lut1d = Some(make_lut1d(values, interp));
        }

        if hdltype == "3d" || hdltype == "3d+1d" {
            // Bind 3D LUT, along with some slightly-elaborate error messages.
            let Some(values) = lut_data.get("3d") else {
                crate::bail!("3D LUT section not found");
            };

            let size_3d_cubed = (size_3d * size_3d * size_3d) as i64;
            if size_3d_cubed * 3 != values.len() as i64 {
                let foundsize = values.len();
                let foundlines = foundsize / 3;
                crate::bail!(
                    "3D LUT contains incorrect number of values. Contained {} values ({} lines), expected {} values ({} lines)",
                    foundsize,
                    foundlines,
                    size_3d_cubed * 3,
                    size_3d_cubed
                );
            }

            let mut lut = new_lut3d(size_3d as usize, interp, BitDepth::F32);
            set_lut3d_from_red_fastest(&mut lut, values)?;
            lut3d = Some(lut);
        }

        if hdltype == "c" {
            // Bind simple 1D RGB LUT.
            let Some(values) = lut_data.get("rgb") else {
                crate::bail!("3D+1D LUT should contain Pre{{}} LUT section");
            };
            if size_1d as i64 != values.len() as i64 {
                crate::bail!(
                    "RGB{{}} LUT was {} values long, expected {} values",
                    values.len(),
                    size_1d
                );
            }
            lut1d = Some(make_lut1d(values, interp));
        }

        // The ops: [range matrix, 1D LUT] for 'c', [3D LUT] for '3d' and
        // [range matrix, prelut, 3D LUT] for '3d+1d'.
        let mut group = GroupTransform::new();
        if let Some(lut) = lut1d {
            if let Some(m) = min_max_matrix_f32(from_min, from_max)? {
                group.append(m);
            }
            group.append(lut);
        }
        if let Some(lut) = lut3d {
            group.append(lut);
        }
        Ok(CachedFile::new(group))
    }

    fn bake(&self, baker: &Baker, format_name: &str) -> Result<Vec<u8>> {
        if format_name != "houdini" {
            crate::bail!("Unknown hdl format name, '{format_name}'.");
        }

        // Default sizes.
        const DEFAULT_SHAPER_SIZE: usize = 1024;
        // MPlay produces bad results with 32^3 cube (in a way that looks
        // more quantised than even "nearest" interpolation in
        // OCIOFileTransform).
        const DEFAULT_CUBE_SIZE: usize = 64;
        const DEFAULT_1D_SIZE: usize = 1024;

        // Get configured sizes (the cube size is also the 1D LUT size).
        let cube_size = baker.cube_size().unwrap_or(DEFAULT_CUBE_SIZE);
        let shaper_size = baker.shaper_size().unwrap_or(DEFAULT_SHAPER_SIZE);
        let oned_size = baker.cube_size().unwrap_or(DEFAULT_1D_SIZE);

        // Version numbers.
        const HDL_1D: i32 = 1; // 1D LUT version number.
        const HDL_3D: i32 = 2; // 3D LUT version number.
        const HDL_3D1D: i32 = 3; // 3D LUT with 1D prelut.

        let shaper_space = baker.shaper_space();

        // Determine the required LUT type.
        let input_to_target = input_to_target_processor(baker)?;
        let required_lut = if input_to_target.has_channel_crosstalk() {
            if shaper_space.is_empty() {
                // Has crosstalk, but no prelut, so need 3D LUT.
                HDL_3D
            } else {
                // Crosstalk with shaper-space.
                HDL_3D1D
            }
        } else {
            // No crosstalk.
            HDL_1D
        };

        // Make the prelut.
        let mut prelut_data: Vec<f32> = Vec::new();

        let mut from_in_start = 0.0f32;
        let mut from_in_end = 1.0f32;

        if required_lut == HDL_3D1D {
            (from_in_start, from_in_end) = shaper_range(baker)?;

            // Generate the identity prelut values, then apply the
            // transform. The prelut is linearly sampled from fromInStart
            // to fromInEnd.
            prelut_data = bake_linear_scale_lut1d(shaper_size, from_in_start, from_in_end)?;
            input_to_shaper_processor(baker)?.apply_rgb_slice(&mut prelut_data);
        }

        // (OCIO note: the "auto prelut" input-space allocation of the csp
        // baker could be done here too.)

        // Make the 3D LUT.
        let mut cube_data: Vec<f32> = Vec::new();
        if required_lut == HDL_3D || required_lut == HDL_3D1D {
            cube_data = bake_identity_lut3d(cube_size, Lut3DOrder::FastRed)?;
            if required_lut == HDL_3D1D {
                shaper_to_target_processor(baker)?.apply_rgb_slice(&mut cube_data);
            } else {
                // No prelut, so the cube goes from input to target.
                input_to_target.apply_rgb_slice(&mut cube_data);
            }
        }

        // Make the 1D LUT.
        let mut oned_data: Vec<f32> = Vec::new();
        if required_lut == HDL_1D {
            oned_data = if !shaper_space.is_empty() {
                (from_in_start, from_in_end) = shaper_range(baker)?;
                bake_linear_scale_lut1d(oned_size, from_in_start, from_in_end)?
            } else {
                bake_identity_lut1d(oned_size)?
            };
            input_to_target.apply_rgb_slice(&mut oned_data);
        }

        // Write the file contents.
        let mut out = String::new();
        out.push_str(&format!("Version\t\t{required_lut}\n"));
        out.push_str("Format\t\tany\n");

        out.push_str("Type\t\t");
        out.push_str(match required_lut {
            HDL_1D => "RGB",
            HDL_3D => "3D",
            _ => "3D+1D",
        });
        out.push('\n');

        out.push_str(&format!(
            "From\t\t{} {}\n",
            format_fixed6(from_in_start),
            format_fixed6(from_in_end)
        ));
        out.push_str(&format!(
            "To\t\t{} {}\n",
            format_fixed6(0.0),
            format_fixed6(1.0)
        ));
        out.push_str(&format!("Black\t\t{}\n", format_fixed6(0.0)));
        out.push_str(&format!("White\t\t{}\n", format_fixed6(1.0)));

        match required_lut {
            HDL_3D1D => out.push_str(&format!("Length\t\t{cube_size} {shaper_size}\n")),
            HDL_3D => out.push_str(&format!("Length\t\t{cube_size}\n")),
            _ => out.push_str(&format!("Length\t\t{oned_size}\n")),
        }

        out.push_str("LUT:\n");

        // Write the prelut.
        if required_lut == HDL_3D1D {
            out.push_str("Pre {\n");
            // Grab the green channel from the RGB prelut.
            for rgb in prelut_data.as_chunks::<3>().0 {
                out.push_str(&format!("\t{}\n", format_fixed6(rgb[1])));
            }
            out.push_str("}\n");

            // Write the "3D {" part of the output of the 3D+1D LUT.
            out.push_str("3D {\n");
        }

        // Write the slightly-different "{" without line for the 3D-only LUT.
        if required_lut == HDL_3D {
            out.push_str(" {\n");
        }

        // Write the cube data after the "{".
        if required_lut == HDL_3D || required_lut == HDL_3D1D {
            // (OCIO note: the original baker code clamped values to 1.0.)
            for rgb in cube_data.as_chunks::<3>().0 {
                out.push_str(&format!(
                    "\t{} {} {}\n",
                    format_fixed6(rgb[0]),
                    format_fixed6(rgb[1]),
                    format_fixed6(rgb[2])
                ));
            }

            // Write the closing "}".
            out.push_str(" }\n");
        }

        // Write out the channels of the 1D LUT.
        if required_lut == HDL_1D {
            for (name, c) in [("R", 0), ("G", 1), ("B", 2)] {
                out.push_str(name);
                out.push_str(" {\n");
                for rgb in oned_data.as_chunks::<3>().0 {
                    out.push_str(&format!("\t{}\n", format_fixed6(rgb[c])));
                }
                out.push_str("}\n");
            }
        }

        Ok(out.into_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transforms::Transform;

    fn read(content: &str) -> Result<CachedFile> {
        LocalFileFormat.read(content.as_bytes(), "", Interpolation::Default)
    }

    #[test]
    fn format_info() {
        let info = LocalFileFormat.format_info();
        assert_eq!(info.len(), 1);
        assert_eq!(info[0].name, "houdini");
        assert_eq!(info[0].extension, "lut");
        assert_eq!(info[0].capabilities, capability::READ | capability::BAKE);
    }

    #[test]
    fn read_1d() {
        let content = "Version\t\t1\nFormat\t\tany\nType\t\tC\nFrom\t\t0.1 3.2\nTo\t\t0 1\nBlack\t\t0\nWhite\t\t0.99\nLength\t\t9\nLUT:\nRGB {\n\t0\n\t0.000977517\n\t0.00195503\n\t0.00293255\n\t0.00391007\n\t0.00488759\n\t0.0058651\n\t0.999022\n\t1.67 }\n";
        let lut1d = [
            0.0f32,
            0.000977517,
            0.00195503,
            0.00293255,
            0.00391007,
            0.00488759,
            0.0058651,
            0.999022,
            1.67,
        ];

        let file = read(content).unwrap();
        assert_eq!(file.group.num_transforms(), 2);

        // Check the range.
        let Transform::Matrix(m) = &file.group.transforms[0] else {
            panic!("expected a matrix")
        };
        let (from_min, from_max) = (0.1f32 as f64, 3.2f32 as f64);
        assert_eq!(m.matrix[0], 1.0 / (from_max - from_min));
        assert_eq!(m.offset[0], -from_min / (from_max - from_min));

        // Check 1D data.
        let Transform::Lut1D(lut) = &file.group.transforms[1] else {
            panic!("expected a Lut1D")
        };
        assert_eq!(lut.file_output_bit_depth, BitDepth::F32);
        assert_eq!(lut.length(), 9);
        for (i, &v) in lut1d.iter().enumerate() {
            assert_eq!(lut.value(i), [v, v, v]);
        }
    }

    #[test]
    fn read_3d() {
        let content = "Version         2\nFormat      any\nType        3D\nFrom        0.2 0.9\nTo      0.001 0.999\nBlack       0.002\nWhite       0.98\nLength      2\nLUT:\n {\n 0 0 0\n 0 0 0\n 0 0.390735 2.68116e-28\n 0 0.390735 0\n 0 0 0\n 0 0 0.599397\n 0 0.601016 0\n 0 0.601016 0.917034\n }\n";
        let cube: [f32; 24] = [
            0.0,
            0.0,
            0.0, //
            0.0,
            0.0,
            0.0, //
            0.0,
            0.390735,
            2.68116e-28, //
            0.0,
            0.390735,
            0.0, //
            0.0,
            0.0,
            0.0, //
            0.0,
            0.0,
            0.599397, //
            0.0,
            0.601016,
            0.0, //
            0.0,
            0.601016,
            0.917034,
        ];

        let file = read(content).unwrap();
        // from_min & from_max are only used when there is a 1D LUT.
        assert_eq!(file.group.num_transforms(), 1);
        let Transform::Lut3D(lut) = &file.group.transforms[0] else {
            panic!("expected a Lut3D")
        };
        let n = lut.grid_size;
        assert_eq!(n, 2);
        for b in 0..n {
            for g in 0..n {
                for r in 0..n {
                    // Lut3DTransform index: blue changes fastest.
                    let array_idx = 3 * ((r * n + g) * n + b);
                    // Houdini order, red changes fastest.
                    let ocio_idx = 3 * ((b * n + g) * n + r);
                    assert_eq!(
                        lut.values[array_idx..array_idx + 3],
                        cube[ocio_idx..ocio_idx + 3]
                    );
                }
            }
        }
    }

    #[test]
    fn read_3d_1d() {
        let content = "Version         3\nFormat      any\nType        3D+1D\nFrom        0.005478 14.080103\nTo      0 1\nBlack       0\nWhite       1\nLength      2 10\nLUT:\nPre {\n    0.994922\n    0.995052\n    0.995181\n    0.995310\n    0.995439\n    0.995568\n    0.995697\n    0.995826\n    0.995954\n    0.996082\n}\n3D {\n    0.093776 0.093776 0.093776\n    0.105219 0.093776 0.093776\n    0.118058 0.093776 0.093776\n    0.132463 0.093776 0.093776\n    0.148626 0.093776 0.093776\n    0.166761 0.093776 0.093776\n    0.187109 0.093776 0.093776\n    0.209939 0.093776 0.093776\n}\n";
        let prelut = [
            0.994922f32,
            0.995052,
            0.995181,
            0.995310,
            0.995439,
            0.995568,
            0.995697,
            0.995826,
            0.995954,
            0.996082,
        ];
        let cube: [f32; 24] = [
            0.093776, 0.093776, 0.093776, //
            0.105219, 0.093776, 0.093776, //
            0.118058, 0.093776, 0.093776, //
            0.132463, 0.093776, 0.093776, //
            0.148626, 0.093776, 0.093776, //
            0.166761, 0.093776, 0.093776, //
            0.187109, 0.093776, 0.093776, //
            0.209939, 0.093776, 0.093776,
        ];

        let file = read(content).unwrap();
        assert_eq!(file.group.num_transforms(), 3);

        let Transform::Matrix(m) = &file.group.transforms[0] else {
            panic!("expected a matrix")
        };
        let (from_min, from_max) = (0.005478f32 as f64, 14.080103f32 as f64);
        assert_eq!(m.matrix[0], 1.0 / (from_max - from_min));

        let Transform::Lut1D(lut) = &file.group.transforms[1] else {
            panic!("expected a Lut1D")
        };
        assert_eq!(lut.file_output_bit_depth, BitDepth::F32);
        assert_eq!(lut.length(), 10);
        for (i, &v) in prelut.iter().enumerate() {
            assert_eq!(lut.value(i), [v, v, v]);
        }

        let Transform::Lut3D(lut) = &file.group.transforms[2] else {
            panic!("expected a Lut3D")
        };
        let n = lut.grid_size;
        assert_eq!(n, 2);
        for b in 0..n {
            for g in 0..n {
                for r in 0..n {
                    let array_idx = 3 * ((r * n + g) * n + b);
                    let ocio_idx = 3 * ((b * n + g) * n + r);
                    assert_eq!(
                        lut.values[array_idx..array_idx + 3],
                        cube[ocio_idx..ocio_idx + 3]
                    );
                }
            }
        }
    }

    #[test]
    fn read_file() {
        let path = format!(
            "{}/tests/data/files/houdini.lut",
            env!("CARGO_MANIFEST_DIR")
        );
        let data = std::fs::read(&path).unwrap();
        let file = LocalFileFormat
            .read(&data, &path, Interpolation::Default)
            .unwrap();
        assert!(file.group.num_transforms() >= 1);
    }

    #[test]
    fn read_failures() {
        let header = "Version 1\nFormat any\nType C\nFrom 0 1\nTo 0 1\nBlack 0\nWhite 1\n";
        let e = read("Version 1\n").unwrap_err();
        assert_eq!(e.message(), "'format' line not found");
        let e = read("Version 1 2\n").unwrap_err();
        assert_eq!(
            e.message(),
            "Incorrect number of chunks (2) after 'version' line, expected 1"
        );
        let e = read(&format!("{header}Length 1 2 3\nLUT:\n")).unwrap_err();
        assert_eq!(
            e.message(),
            "Incorrect number of chunks (3) after 'length' line, expected between 1 and 2"
        );
        let e = read(&header.replace("Type C", "Type RGB")).unwrap_err();
        assert_eq!(e.message(), "Unsupported Houdini LUT type: 'rgb'");
        let e = read(&header.replace("From 0 1", "From a 1")).unwrap_err();
        assert!(e
            .message()
            .starts_with("Invalid float value(s) on 'From' line"));
        let e = read(&format!("{header}Length 2\nLUT:\nRGB {{ 0 x }}\n")).unwrap_err();
        assert_eq!(e.message(), "Invalid float value in rgb LUT, 'x'");
        let e = read(&format!("{header}Length 2\nLUT:\nRGB x 0 1 }}\n")).unwrap_err();
        assert_eq!(
            e.message(),
            "Malformed LUT - Unknown word 'RGB' after LUT name 'x'"
        );
        let e = read(&format!("{header}Length 3\nLUT:\nRGB {{ 0 1 }}\n")).unwrap_err();
        assert_eq!(
            e.message(),
            "RGB{} LUT was 2 values long, expected 3 values"
        );
        let e = read(&format!("{header}Length 1\nLUT:\nRGB {{ 0 }}\n")).unwrap_err();
        assert_eq!(
            e.message(),
            "1D LUT size must be between 2 and 300000, found: 1"
        );
        let e = read(&format!(
            "{}Length 2\nLUT:\n{{ 0 1 }}\n",
            header.replace("Type C", "Type 3D")
        ))
        .unwrap_err();
        assert!(e
            .message()
            .starts_with("3D LUT contains incorrect number of values. Contained 2 values"));
        let e = read(&format!(
            "{}Length 2\nLUT:\n{{ 0 1 }}\n",
            header.replace("Type C", "Type 3D+1D")
        ))
        .unwrap_err();
        assert_eq!(
            e.message(),
            "Prelut size must be between 2 and 300000, found: -1"
        );
    }

    // Baker tests (port of the baker parts of `FileFormatHDL_tests.cpp`).

    use crate::fileformats::utils::bake_test_utils::{
        bake, baker, check_round_trip, compare_lines, config_yaml, SHAPER_LOG2_CONFIG,
    };

    /// The target space desaturates, causing channel crosstalk.
    const TARGET_SAT: (&str, &str) = ("target", "from_scene_reference: !<CDLTransform> {sat: 0.5}");

    fn hdl_1d_channel(name: &str, values: &str) -> String {
        let mut s = format!("{name} {{\n");
        for v in values.split(' ') {
            s.push_str(&format!("\t{v}\n"));
        }
        s.push_str("}\n");
        s
    }

    fn hdl_1d(from: &str, values: &str) -> String {
        format!(
            "Version\t\t1\nFormat\t\tany\nType\t\tRGB\nFrom\t\t{from}\nTo\t\t0.000000 1.000000\n\
             Black\t\t0.000000\nWhite\t\t1.000000\nLength\t\t10\nLUT:\n{}{}{}",
            hdl_1d_channel("R", values),
            hdl_1d_channel("G", values),
            hdl_1d_channel("B", values)
        )
    }

    #[test]
    fn bake_1d() {
        let config = config_yaml(&[
            ("lnf", ""),
            (
                "target",
                "from_scene_reference: !<CDLTransform> {offset: [0.1, 0.1, 0.1]}",
            ),
        ]);
        let mut b = baker(&config, "houdini");
        b.set_input_space("lnf");
        b.set_target_space("target");
        // FIXME (as in OCIO): misusing the cube size to set the 1D LUT size.
        b.set_cube_size(Some(10));

        let expected = hdl_1d(
            "0.000000 1.000000",
            "0.100000 0.211111 0.322222 0.433333 0.544444 0.655556 0.766667 0.877778 0.988889 1.100000",
        );
        assert_eq!(bake(&b), expected);
    }

    #[test]
    fn bake_1d_shaper() {
        {
            // Lin to Log.
            let mut b = baker(SHAPER_LOG2_CONFIG, "houdini");
            b.set_input_space("Raw");
            b.set_target_space("Log2");
            b.set_shaper_space("Log2");
            b.set_cube_size(Some(10));

            let expected = hdl_1d(
                "0.001989 16.291878",
                "0.000000 0.756268 0.833130 0.878107 0.910023 0.934780 0.955010 0.972114 0.986931 1.000000",
            );
            assert_eq!(bake(&b), expected);
        }
        {
            // Log to Lin.
            let mut b = baker(SHAPER_LOG2_CONFIG, "houdini");
            b.set_input_space("Log2");
            b.set_target_space("Raw");
            b.set_cube_size(Some(10));

            let expected = hdl_1d(
                "0.000000 1.000000",
                "0.001989 0.005413 0.014731 0.040091 0.109110 0.296951 0.808177 2.199522 5.986179 16.291878",
            );
            compare_lines(&bake(&b), &expected, 1e-5, |i| {
                (10..=19).contains(&i) || (22..=31).contains(&i) || (34..=43).contains(&i)
            });
        }
    }

    const CUBE_SAT_2: &str = "\t0.000000 0.000000 0.000000\n\
        \t0.606300 0.106300 0.106300\n\
        \t0.357600 0.857600 0.357600\n\
        \t0.963900 0.963900 0.463900\n\
        \t0.036100 0.036100 0.536100\n\
        \t0.642400 0.142400 0.642400\n\
        \t0.393700 0.893700 0.893700\n\
        \t1.000000 1.000000 1.000000\n";

    #[test]
    fn bake_3d() {
        let config = config_yaml(&[("lnf", ""), TARGET_SAT]);
        let mut b = baker(&config, "houdini");
        b.set_input_space("lnf");
        b.set_target_space("target");
        b.set_cube_size(Some(2));

        let expected = format!(
            "Version\t\t2\nFormat\t\tany\nType\t\t3D\nFrom\t\t0.000000 1.000000\n\
             To\t\t0.000000 1.000000\nBlack\t\t0.000000\nWhite\t\t1.000000\nLength\t\t2\n\
             LUT:\n {{\n{CUBE_SAT_2} }}\n"
        );
        assert_eq!(bake(&b), expected);
    }

    #[test]
    fn bake_3d_1d() {
        let config = config_yaml(&[
            ("lnf", ""),
            (
                "shaper",
                "to_scene_reference: !<ExponentTransform> {value: [2.6, 2.6, 2.6, 1]}",
            ),
            TARGET_SAT,
        ]);
        let mut b = baker(&config, "houdini");
        b.set_input_space("lnf");
        b.set_shaper_space("shaper");
        b.set_target_space("target");
        b.set_shaper_size(Some(10));
        b.set_cube_size(Some(2));

        let expected = format!(
            "Version\t\t3\nFormat\t\tany\nType\t\t3D+1D\nFrom\t\t0.000000 1.000000\n\
             To\t\t0.000000 1.000000\nBlack\t\t0.000000\nWhite\t\t1.000000\nLength\t\t2 10\n\
             LUT:\nPre {{\n\t0.000000\n\t0.429520\n\t0.560744\n\t0.655378\n\t0.732057\n\
             \t0.797661\n\t0.855604\n\t0.907865\n\t0.955710\n\t1.000000\n}}\n\
             3D {{\n{CUBE_SAT_2} }}\n"
        );
        let out = bake(&b);
        // The lines are compared as numbers (OCIO does not check the values
        // of this test because of platform differences).
        compare_lines(&out, &expected, 1e-5, |i| {
            (10..=19).contains(&i) || (22..=29).contains(&i)
        });
    }

    #[test]
    fn look_test() {
        // Sets up a Look with the same parameters as the bake_3d_1d test,
        // but with a different shaper space, to ensure that case is caught.
        // Also ensure the effects of the desaturation are detected by using
        // a 3 cubed LUT, which will thus test colour values other than the
        // corner points of the cube.
        let config = r#"ocio_profile_version: 2

roles:
  reference: lnf
  default: lnf

looks:
  - !<Look>
    name: look
    process_space: look_process
    transform: !<CDLTransform> {sat: 0.5}

colorspaces:
  - !<ColorSpace>
    name: lnf
    family: lnf

  - !<ColorSpace>
    name: shaper
    family: shaper
    to_scene_reference: !<ExponentTransform> {value: [2.2, 2.2, 2.2, 1]}

  - !<ColorSpace>
    name: look_process
    family: look_process
    to_scene_reference: !<ExponentTransform> {value: [2.6, 2.6, 2.6, 1]}
"#;
        let mut b = baker(config, "houdini");
        b.set_input_space("lnf");
        b.set_shaper_space("shaper");
        b.set_target_space("shaper");
        b.set_looks("look");
        b.set_shaper_size(Some(10));
        b.set_cube_size(Some(3));

        let expected = "Version\t\t3\n\
            Format\t\tany\n\
            Type\t\t3D+1D\n\
            From\t\t0.000000 1.000000\n\
            To\t\t0.000000 1.000000\n\
            Black\t\t0.000000\n\
            White\t\t1.000000\n\
            Length\t\t3 10\n\
            LUT:\n\
            Pre {\n\
            \t0.000000\n\
            \t0.368344\n\
            \t0.504760\n\
            \t0.606913\n\
            \t0.691699\n\
            \t0.765539\n\
            \t0.831684\n\
            \t0.892049\n\
            \t0.947870\n\
            \t1.000000\n\
            }\n\
            3D {\n\
            \t0.000000 0.000000 0.000000\n\
            \t0.276787 0.035360 0.035360\n\
            \t0.553575 0.070720 0.070720\n\
            \t0.148309 0.416989 0.148309\n\
            \t0.478739 0.478739 0.201718\n\
            \t0.774120 0.528900 0.245984\n\
            \t0.296618 0.833978 0.296618\n\
            \t0.650361 0.902354 0.355417\n\
            \t0.957478 0.957478 0.403436\n\
            \t0.009867 0.009867 0.239325\n\
            \t0.296368 0.049954 0.296368\n\
            \t0.575308 0.086766 0.343137\n\
            \t0.166161 0.437812 0.437812\n\
            \t0.500000 0.500000 0.500000\n\
            \t0.796987 0.550484 0.550484\n\
            \t0.316402 0.857106 0.607391\n\
            \t0.672631 0.925760 0.672631\n\
            \t0.981096 0.981096 0.725386\n\
            \t0.019735 0.019735 0.478650\n\
            \t0.312132 0.062101 0.541651\n\
            \t0.592736 0.099909 0.592736\n\
            \t0.180618 0.454533 0.695009\n\
            \t0.517061 0.517061 0.761560\n\
            \t0.815301 0.567796 0.815301\n\
            \t0.332322 0.875624 0.875624\n\
            \t0.690478 0.944497 0.944497\n\
            \t1.000000 1.000000 1.000000\n\
            }\n";
        let out = bake(&b);
        let out_lines: Vec<&str> = out.lines().map(str::trim).collect();
        let exp_lines: Vec<&str> = expected.lines().map(str::trim).collect();
        assert_eq!(out_lines, exp_lines);
    }

    #[test]
    fn bake_defaults_and_errors() {
        let config = config_yaml(&[
            ("lnf", ""),
            (
                "shaper",
                "to_scene_reference: !<ExponentTransform> {value: [2.2, 2.2, 2.2, 1]}",
            ),
            TARGET_SAT,
        ]);
        let mut b = baker(&config, "houdini");
        b.set_input_space("lnf");
        b.set_target_space("shaper");
        let out = bake(&b);
        assert!(out.contains("Length\t\t1024\n"));

        b.set_target_space("target");
        let out = bake(&b);
        assert!(out.contains("Length\t\t64\n"));
        assert_eq!(out.lines().count(), 11 + 64 * 64 * 64);

        b.set_shaper_space("shaper");
        b.set_cube_size(Some(3));
        let out = bake(&b);
        assert!(out.contains("Length\t\t3 1024\n"));

        let e = LocalFileFormat.bake(&b, "hdl").unwrap_err();
        assert_eq!(e.message(), "Unknown hdl format name, 'hdl'.");
    }

    #[test]
    fn bake_round_trip() {
        let samples = [
            [0.0, 0.0, 0.0],
            [0.25, 0.5, 0.75],
            [0.9, 0.1, 0.4],
            [1.0, 1.0, 1.0],
        ];

        // Note: the 1D LUTs ("RGB" type) can't be read back, the Houdini
        // reader only supports the 'C', '3D' and '3D+1D' types (as in OCIO).

        // 3D and 3D + 1D.
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
        let mut b = baker(&config, "houdini");
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
