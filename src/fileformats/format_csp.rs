//! Rising Sun Research cineSpace `csp` LUT format (port of
//! `FileFormatCSP.cpp`).
//!
//! A csp file holds a per-channel spline "prelut" followed by either a 1D
//! LUT or a 3D LUT (red fastest). The prelut splines are resampled into a
//! 65536 entries 1D LUT, preceded by a range matrix mapping the spline
//! input domain to `[0, 1]`.

use super::utils::{
    bake_identity_lut1d, bake_identity_lut3d, format_fixed6, lerpf, min_max_matrix, new_lut1d,
    new_lut3d, split_by_white_spaces, string_to_int, string_vec_to_float_vec,
    string_vec_to_int_vec, trim, vecs_equal_with_rel_error, write_metadata_lines, IStream,
    MAX_1D_LUT_LENGTH, MAX_3D_LUT_LENGTH,
};
use super::{bake_capability, capability, CachedFile, FileFormat, FormatInfo};
use crate::baker::{
    input_to_target_processor, shaper_to_input_processor, shaper_to_target_processor, Baker,
};
use crate::error::{Error, Result};
use crate::ops::lut3d::Lut3DOrder;
use crate::transforms::{AllocationTransform, GroupTransform, Transform};
use crate::types::{
    Allocation, BitDepth, Interpolation, OptimizationFlags, TransformDirection,
    METADATA_DESCRIPTION,
};

/// 2^16 samples.
const NUM_PRELUT_SAMPLES: usize = 65536;

/// Always use linear interpolation for preluts to get the best precision.
const PRELUT_INTERPOLATION: Interpolation = Interpolation::Linear;

// ---------------------------------------------------------------------------
// Port of the cineSpace 1D interpolator (Interpolators.c).

/// Cubic spline interpolator through a set of sample points.
struct Interpolator1D {
    stims: Vec<f32>,
    /// `5 * (n - 1)` values holding a sequence of `1.0/delta, a, b, c, d`
    /// such that the curve in interval `i` is given by
    /// `z = (x - stims[i]) * (1.0/delta)` and `y = a + b*z + c*z^2 + d*z^3`.
    parameters: Vec<f32>,
    /// `f(stims[0])`.
    min_value: f32,
    /// `f(stims[n - 1])`.
    max_value: f32,
}

impl Interpolator1D {
    /// Port of `rsr_Interpolator1D_createFromRaw`. `stims` and `values`
    /// must hold at least 2 entries.
    fn new(stims: &[f32], values: &[f32]) -> Self {
        let length = stims.len().min(values.len());
        let mut parameters = vec![0.0f32; 5 * length.saturating_sub(1)];

        if length == 2 {
            parameters[0] = 1.0 / (stims[1] - stims[0]);
            parameters[1] = values[0];
            parameters[2] = values[1] - values[0];
            parameters[3] = 0.0;
            parameters[4] = 0.0;
        } else {
            for i in 0..length.saturating_sub(1) {
                let params = &mut parameters[5 * i..5 * i + 5];
                let f0 = values[i];
                let f1 = values[i + 1];

                params[0] = 1.0 / (stims[i + 1] - stims[i]);

                if i == 0 {
                    let delta = stims[i + 1] - stims[i];
                    let delta2 = (stims[i + 2] - stims[i + 1]) / delta;
                    let f2 = values[i + 2];

                    let dfdx1 = (f2 - f0) / (1.0 + delta2);
                    params[1] = 1.0 * f0 + 0.0 * f1 + 0.0 * dfdx1;
                    params[2] = -2.0 * f0 + 2.0 * f1 - 1.0 * dfdx1;
                    params[3] = 1.0 * f0 - 1.0 * f1 + 1.0 * dfdx1;
                    params[4] = 0.0;
                } else if i == length - 2 {
                    let delta = stims[i + 1] - stims[i];
                    let delta1 = (stims[i] - stims[i - 1]) / delta;
                    let fn1 = values[i - 1];
                    let dfdx0 = (f1 - fn1) / (1.0 + delta1);
                    params[1] = 1.0 * f0 + 0.0 * f1 + 0.0 * dfdx0;
                    params[2] = 0.0 * f0 + 0.0 * f1 + 1.0 * dfdx0;
                    params[3] = -1.0 * f0 + 1.0 * f1 - 1.0 * dfdx0;
                    params[4] = 0.0;
                } else {
                    let delta = stims[i + 1] - stims[i];
                    let fn1 = values[i - 1];
                    let delta1 = (stims[i] - stims[i - 1]) / delta;
                    let f2 = values[i + 2];
                    let delta2 = (stims[i + 2] - stims[i + 1]) / delta;
                    let dfdx0 = (f1 - fn1) / (1.0 + delta1);
                    let dfdx1 = (f2 - f0) / (1.0 + delta2);
                    params[1] = 1.0 * f0 + 0.0 * dfdx0 + 0.0 * f1 + 0.0 * dfdx1;
                    params[2] = 0.0 * f0 + 1.0 * dfdx0 + 0.0 * f1 + 0.0 * dfdx1;
                    params[3] = -3.0 * f0 - 2.0 * dfdx0 + 3.0 * f1 - 1.0 * dfdx1;
                    params[4] = 2.0 * f0 + 1.0 * dfdx0 - 2.0 * f1 + 1.0 * dfdx1;
                }
            }
        }

        Self {
            stims: stims[..length].to_vec(),
            parameters,
            min_value: values[0],
            max_value: values[length - 1],
        }
    }

    /// Port of `rsr_internal_I1D_findSegmentContaining`.
    fn find_segment_containing(&self, x: f32) -> usize {
        let mut low = 0usize;
        let mut high = self.stims.len() - 1;
        while high - low > 1 {
            let mid = (low + high) / 2;
            if x < self.stims[mid] {
                high = mid;
            } else {
                low = mid;
            }
        }
        low
    }

    /// Port of `rsr_Interpolator1D_interpolate`.
    fn interpolate(&self, x: f32) -> f32 {
        // Is x in range?
        if x.is_nan() {
            return x;
        }
        if x < self.stims[0] {
            return self.min_value;
        }
        if x > self.stims[self.stims.len() - 1] {
            return self.max_value;
        }

        // Ok so it's between the beginning and end... let's find out where.
        let seg_id = self.find_segment_containing(x);
        let segdata = &self.parameters[5 * seg_id..5 * seg_id + 5];

        let inv_delta = segdata[0];
        let a = segdata[1];
        let b = segdata[2];
        let c = segdata[3];
        let d = segdata[4];

        let z = (x - self.stims[seg_id]) * inv_delta;
        a + z * (b + z * (c + d * z))
    }
}

// ---------------------------------------------------------------------------

/// `startswithU`: case-insensitive prefix test of the trimmed string.
fn startswith_u(s: &str, prefix: &str) -> bool {
    trim(s).to_ascii_uppercase().starts_with(prefix)
}

struct LocalFileFormat;

pub(crate) fn create() -> Box<dyn FileFormat> {
    Box::new(LocalFileFormat)
}

impl FileFormat for LocalFileFormat {
    fn format_info(&self) -> Vec<FormatInfo> {
        vec![FormatInfo {
            name: "cinespace",
            extension: "csp",
            capabilities: capability::READ | capability::BAKE,
            bake_capabilities: bake_capability::LUT3D | bake_capability::LUT1D_3D,
        }]
    }

    fn read(&self, data: &[u8], file_name: &str, interp: Interpolation) -> Result<CachedFile> {
        let mut istream = IStream::new(data);

        // Try and read the LUT header.
        let Some(mut line) = istream.nextline() else {
            crate::bail!("File {file_name}: file stream empty when trying to read csp LUT.");
        };

        if !startswith_u(&line, "CSPLUTV100") {
            crate::bail!("File {file_name} doesn't seem to be a csp LUT, expected 'CSPLUTV100'. First line: '{line}'.");
        }

        // Next line tells us if we are reading a 1D or 3D LUT.
        line = istream.nextline().unwrap_or_default();
        if !startswith_u(&line, "1D") && !startswith_u(&line, "3D") {
            crate::bail!(
                "Unsupported CSP LUT type. Require 1D or 3D. Found, '{line}' in {file_name}."
            );
        }
        let csptype = line.clone();

        // Read meta data block.
        let mut metadata = String::new();
        let mut line_update_needed = false;
        line = istream.nextline().unwrap_or_default();
        if startswith_u(&line, "BEGIN METADATA") {
            while !startswith_u(&line, "END METADATA") {
                match istream.nextline() {
                    Some(l) => line = l,
                    // The stream is exhausted (malformed metadata block).
                    None => {
                        line.clear();
                        break;
                    }
                }
                if !startswith_u(&line, "END METADATA") {
                    metadata.push_str(&line);
                    metadata.push('\n');
                }
            }
            line_update_needed = true;
        } // Else line update not needed.

        // Make 3 vectors of prelut inputs + output values.
        let mut prelut_in: [Vec<f32>; 3] = Default::default();
        let mut prelut_out: [Vec<f32>; 3] = Default::default();
        let mut useprelut = [false; 3];

        // Parse the prelut block.
        for c in 0..3 {
            // How many points do we have for this channel.
            if line_update_needed {
                line = istream.nextline().unwrap_or_default();
            }

            let cpoints = match string_to_int(&line, false) {
                Some(v) if v >= 0 => v,
                _ => crate::bail!(
                    "Prelut does not specify valid dimension size on channel '{c}: '{line}' in {file_name}."
                ),
            };

            if cpoints >= 2 {
                line = istream.nextline().unwrap_or_default();
                let inputparts = split_by_white_spaces(trim(&line));

                line = istream.nextline().unwrap_or_default();
                let outputparts = split_by_white_spaces(trim(&line));

                if inputparts.len() as i64 != cpoints as i64
                    || outputparts.len() as i64 != cpoints as i64
                {
                    crate::bail!(
                        "Prelut does not specify the expected number of data points. Expected: {}.Found: {}, {}. In {}.",
                        cpoints,
                        inputparts.len(),
                        outputparts.len(),
                        file_name
                    );
                }

                match (
                    string_vec_to_float_vec(&inputparts),
                    string_vec_to_float_vec(&outputparts),
                ) {
                    (Some(i), Some(o)) => {
                        prelut_in[c] = i;
                        prelut_out[c] = o;
                    }
                    _ => crate::bail!(
                        "Prelut data is malformed, cannot convert to float array. In {file_name}."
                    ),
                }

                useprelut[c] = !vecs_equal_with_rel_error(&prelut_in[c], &prelut_out[c], 1e-6);
            } else {
                // Even though it's probably not part of the spec, why not
                // allow for a size 0 in a channel to be specified? It should
                // be synonymous with identity, and allows the code lower
                // down to assume all 3 channels exist.
                prelut_in[c] = vec![0.0, 1.0];
                prelut_out[c] = vec![0.0, 1.0];
                useprelut[c] = false;
            }
            line_update_needed = true;
        }

        let mut lut1d = None;
        let mut lut3d = None;

        if csptype == "1D" {
            // How many 1D LUT points do we have.
            line = istream.nextline().unwrap_or_default();
            let Some(points1d) = string_to_int(&line, false) else {
                crate::bail!(
                    "A csp 1D LUT with invalid number of entries ({line}) in {file_name}."
                );
            };
            if points1d <= 0 || points1d as i64 > MAX_1D_LUT_LENGTH as i64 {
                crate::bail!("A csp 1D LUT with invalid number of entries ({points1d}): {line} . In {file_name}.");
            }

            let points = points1d as usize;
            let mut lut = new_lut1d(points, false, interp, BitDepth::F32);
            for i in 0..points {
                // Scan for the three floats.
                line = istream.nextline().unwrap_or_default();
                let parts = split_by_white_spaces(&line);
                let values = match string_vec_to_float_vec(&parts) {
                    Some(v) if v.len() == 3 => v,
                    _ => crate::bail!(
                        "Malformed 1D csp LUT. Each line of LUT values must contain three numbers. Line: '{line}'. File: {file_name}."
                    ),
                };
                // Store each channel.
                lut.values[i * 3..i * 3 + 3].copy_from_slice(&values);
            }
            lut1d = Some(lut);
        } else if csptype == "3D" {
            // Read the cube size.
            line = istream.nextline().unwrap_or_default();
            let parts = split_by_white_spaces(&line);
            let cube_size = match string_vec_to_int_vec(&parts) {
                Some(v) if v.len() == 3 => v,
                _ => crate::bail!(
                    "Malformed 3D csp in LUT file, couldn't read cube size. '{line}'. In file: {file_name}."
                ),
            };

            // TODO: Support nonuniform cube sizes.
            let lut_size = cube_size[0];
            if lut_size != cube_size[1] || lut_size != cube_size[2] {
                crate::bail!(
                    "A csp 3D LUT with nonuniform cube sizes is not supported ({}, {}, {}): {} .",
                    cube_size[0],
                    cube_size[1],
                    cube_size[2],
                    line
                );
            }

            if lut_size <= 0 || lut_size as i64 > MAX_3D_LUT_LENGTH as i64 {
                crate::bail!(
                    "A csp 3D LUT with invalid cube size ({lut_size}): {line}' in {file_name}."
                );
            }

            let n = lut_size as usize;
            let mut lut = new_lut3d(n, interp, BitDepth::F32);
            let (mut r, mut g, mut b) = (0usize, 0usize, 0usize);
            for i in 0..n * n * n {
                // Load the cube.
                line = istream.nextline().unwrap_or_default();

                // Lut3DTransform index: b changes fastest.
                let idx = 3 * (b + n * (g + n * r));

                let parts = split_by_white_spaces(&line);
                let values = match string_vec_to_float_vec(&parts) {
                    Some(v) if v.len() == 3 => v,
                    _ => crate::bail!("Malformed 3D csp LUT, couldn't read cube row ({i}): {line}' in {file_name}."),
                };
                lut.values[idx..idx + 3].copy_from_slice(&values);

                // CSP stores the LUT in red-fastest order.
                r += 1;
                if r == n {
                    r = 0;
                    g += 1;
                    if g == n {
                        g = 0;
                        b += 1;
                    }
                }
            }
            lut3d = Some(lut);
        }

        let mut group = GroupTransform::new();
        if !metadata.is_empty() {
            group
                .metadata
                .add_child_element(METADATA_DESCRIPTION, &metadata);
        }

        if useprelut.iter().any(|&u| u) {
            let mut prelut = new_lut1d(
                NUM_PRELUT_SAMPLES,
                false,
                Interpolation::Default,
                BitDepth::F32,
            );
            let mut prelut_from_min = [0.0f64; 3];
            let mut prelut_from_max = [1.0f64; 3];

            for c in 0..3 {
                let numpts = prelut_in[c].len();
                let from_min = prelut_in[c][0];
                let from_max = prelut_in[c][numpts - 1];

                // Create the interpolator, to resample to simple 1D LUT.
                let interpolator = Interpolator1D::new(&prelut_in[c], &prelut_out[c]);

                // Resample into 1D LUT.
                // TODO: Fancy spline analysis to determine required number of samples.
                prelut_from_min[c] = from_min as f64;
                prelut_from_max[c] = from_max as f64;

                for i in 0..NUM_PRELUT_SAMPLES {
                    let interpo = i as f32 / (NUM_PRELUT_SAMPLES - 1) as f32;
                    let srcval = lerpf(from_min, from_max, interpo);
                    prelut.values[i * 3 + c] = interpolator.interpolate(srcval);
                }
            }

            prelut.interpolation = PRELUT_INTERPOLATION;

            if let Some(m) = min_max_matrix(prelut_from_min, prelut_from_max)? {
                group.append(m);
            }
            group.append(prelut);
        }

        // If the file contains neither, the group only holds the prelut.
        if let Some(lut) = lut1d {
            group.append(lut);
        } else if let Some(lut) = lut3d {
            group.append(lut);
        }

        Ok(CachedFile::new(group))
    }

    fn bake(&self, baker: &Baker, _format_name: &str) -> Result<Vec<u8>> {
        const DEFAULT_CUBE_SIZE: usize = 32;
        const DEFAULT_SHAPER_SIZE: usize = 1024;

        let config = baker.required_config()?;

        // Smallest cube is 2x2x2.
        let cube_size = baker.cube_size().unwrap_or(DEFAULT_CUBE_SIZE).max(2);

        let mut cube_data = bake_identity_lut3d(cube_size, Lut3DOrder::FastRed)?;

        let shaper_in_data: Vec<f32>;
        let shaper_out_data: Vec<f32>;

        // Use an explicit shaper space.
        // (OCIO note: the optional allocation of the shaper space could be
        // used instead of the implied 0-1 uniform allocation.)
        if !baker.shaper_space().is_empty() {
            let shaper_size = baker.shaper_size().unwrap_or(DEFAULT_SHAPER_SIZE);

            shaper_out_data = bake_identity_lut1d(shaper_size)?;
            let mut shaper_in = bake_identity_lut1d(shaper_size)?;

            shaper_to_input_processor(baker)?.apply_rgb_slice(&mut shaper_in);
            shaper_in_data = shaper_in;

            shaper_to_target_processor(baker)?.apply_rgb_slice(&mut cube_data);
        } else {
            // A shaper is not specified, let's fake one, using the input
            // space allocation as our guide.
            let input_color_space =
                config.get_color_space(baker.input_space()).ok_or_else(|| {
                    Error::msg(format!(
                        "Could not find input colorspace '{}'.",
                        baker.input_space()
                    ))
                })?;

            // Let's make an allocation transform for this colorspace
            // (the number of variables may be 0).
            let allocation = AllocationTransform {
                allocation: input_color_space.allocation(),
                vars: input_color_space
                    .allocation_vars()
                    .iter()
                    .map(|&v| f64::from(v))
                    .collect(),
                ..Default::default()
            };

            // What size shaper should we make?
            let mut shaper_size = baker.shaper_size().unwrap_or(DEFAULT_SHAPER_SIZE).max(2);
            if input_color_space.allocation() == Allocation::Uniform {
                // If we know it's a uniform scaling, only 2 points will
                // suffice.
                shaper_size = 2;
            }

            shaper_out_data = bake_identity_lut1d(shaper_size)?;
            let mut shaper_in = bake_identity_lut1d(shaper_size)?;

            // Apply the inverse of the allocation to the shaper input
            // (x axis) and to the cube.
            let shaper_to_input = config
                .get_processor_for_transform(
                    &Transform::Allocation(allocation),
                    TransformDirection::Inverse,
                )?
                .optimized_cpu_processor(OptimizationFlags::LOSSLESS);

            shaper_to_input.apply_rgb_slice(&mut shaper_in);
            shaper_to_input.apply_rgb_slice(&mut cube_data);
            shaper_in_data = shaper_in;

            // Apply the 3D LUT to the remainder (from the input to the
            // output).
            input_to_target_processor(baker)?.apply_rgb_slice(&mut cube_data);
        }

        // Write out the file.
        let mut out = String::new();
        out.push_str("CSPLUTV100\n");
        out.push_str("3D\n");
        out.push('\n');
        out.push_str("BEGIN METADATA\n");
        write_metadata_lines(&mut out, baker.format_metadata(), "");
        out.push_str("END METADATA\n");
        out.push('\n');

        // Write out the 1D prelut.
        if shaper_out_data.len() != shaper_in_data.len() {
            crate::bail!("Internal shaper size exception.");
        }

        if !shaper_in_data.is_empty() {
            let num = shaper_in_data.len() / 3;
            for c in 0..3 {
                out.push_str(&format!("{num}\n"));
                let ins: Vec<String> = (0..num)
                    .map(|i| format_fixed6(shaper_in_data[3 * i + c]))
                    .collect();
                out.push_str(&ins.join(" "));
                out.push('\n');

                let outs: Vec<String> = (0..num)
                    .map(|i| format_fixed6(shaper_out_data[3 * i + c]))
                    .collect();
                out.push_str(&outs.join(" "));
                out.push('\n');
            }
        }
        out.push('\n');

        // Write out the 3D cube.
        out.push_str(&format!("{cube_size} {cube_size} {cube_size}\n"));
        for rgb in cube_data.chunks_exact(3) {
            out.push_str(&format!(
                "{} {} {}\n",
                format_fixed6(rgb[0]),
                format_fixed6(rgb[1]),
                format_fixed6(rgb[2])
            ));
        }
        out.push('\n');

        Ok(out.into_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transforms::Transform;

    fn read(content: &str, interp: Interpolation) -> Result<CachedFile> {
        LocalFileFormat.read(content.as_bytes(), "file.name", interp)
    }

    fn check_error(content: &str, what: &str) {
        match read(content, Interpolation::Default) {
            Ok(_) => panic!("expected an error containing '{what}'"),
            Err(e) => assert!(
                e.message().contains(what),
                "'{}' does not contain '{}'",
                e.message(),
                what
            ),
        }
    }

    fn metadata(file: &CachedFile) -> String {
        file.group
            .metadata
            .children_named(METADATA_DESCRIPTION)
            .next()
            .map(|c| c.element_value.clone())
            .unwrap_or_default()
    }

    const SIMPLE_1D: &str = "CSPLUTV100\n1D\n\nBEGIN METADATA\nfoobar\nEND METADATA\n\n2\n0.0 1.0\n0.0 2.0\n6\n0.0 0.2 0.4 0.6 0.8 1.0\n0.0 0.4 0.8 1.2 1.6 2.0\n3\n0.0 0.1 1.0\n0.0 0.2 2.0\n\n6\n0.0 0.0 0.0\n0.2 0.3 0.1\n0.4 0.5 0.2\n0.5 0.6 0.3\n0.6 0.8 0.4\n1.0 0.9 0.5\n";

    const PRELUT_3D: &str = "11\n0.0 0.1 0.2 0.3 0.4 0.5 0.6 0.7 0.8 0.9 1.0\n0.0 0.1 0.2 0.3 0.4 0.5 0.6 0.7 0.8 0.9 1.0\n6\n0.0 0.2       0.4 0.6 0.8 1.0\n0.0 0.2000000 0.4 0.6 0.8 1.0\n5\n0.0 0.25       0.5 0.6 0.7\n0.0 0.25000001 0.5 0.6 0.7\n\n";

    const CUBE_333: &str = "0.0 0.0 0.0\n0.5 0.0 0.0\n1.0 0.0 0.0\n0.0 0.5 0.0\n0.5 0.5 0.0\n1.0 0.5 0.0\n0.0 1.0 0.0\n0.5 1.0 0.0\n1.0 1.0 0.0\n0.0 0.0 0.5\n0.5 0.0 0.5\n1.0 0.0 0.5\n0.0 0.5 0.5\n0.5 0.5 0.5\n1.0 0.5 0.5\n0.0 1.0 0.5\n0.5 1.0 0.5\n1.0 1.0 0.5\n0.0 0.0 1.0\n0.5 0.0 1.0\n1.0 0.0 1.0\n0.0 0.5 1.0\n0.5 0.5 1.0\n1.0 0.5 1.0\n0.0 1.0 1.0\n0.5 1.0 1.0\n1.0 1.0 1.0\n";

    fn header_3d() -> String {
        format!("CSPLUTV100\n3D\n\nBEGIN METADATA\nfoobar\nEND METADATA\n\n{PRELUT_3D}")
    }

    #[test]
    fn format_info() {
        let info = LocalFileFormat.format_info();
        assert_eq!(info.len(), 1);
        assert_eq!(info[0].name, "cinespace");
        assert_eq!(info[0].extension, "csp");
        assert_eq!(info[0].capabilities, capability::READ | capability::BAKE);
    }

    #[test]
    fn simple_1d() {
        let red = [0.0f32, 0.2, 0.4, 0.5, 0.6, 1.0];
        let green = [0.0f32, 0.3, 0.5, 0.6, 0.8, 0.9];
        let blue = [0.0f32, 0.1, 0.2, 0.3, 0.4, 0.5];

        let file = read(SIMPLE_1D, Interpolation::Default).unwrap();
        assert_eq!(metadata(&file), "foobar\n");

        // Range [0, 1] for all channels: no matrix, prelut and 1D LUT.
        assert_eq!(file.group.num_transforms(), 2);
        let Transform::Lut1D(prelut) = &file.group.transforms[0] else {
            panic!("expected a prelut")
        };
        assert_eq!(prelut.file_output_bit_depth, BitDepth::F32);
        assert_eq!(prelut.interpolation, Interpolation::Linear);

        // Check prelut data (note: the spline is resampled into a 1D LUT).
        let length = prelut.length();
        assert_eq!(length, NUM_PRELUT_SAMPLES);
        for i in (0..length).step_by(128) {
            let input = i as f32 / (length - 1) as f32;
            let output = prelut.values[i * 3];
            assert!(
                (input * 2.0 - output).abs() <= 1e-4,
                "{i}: {input} {output}"
            );
        }

        let Transform::Lut1D(lut) = &file.group.transforms[1] else {
            panic!("expected a Lut1D")
        };
        assert_eq!(lut.file_output_bit_depth, BitDepth::F32);
        assert_eq!(lut.length(), 6);
        for i in 0..6 {
            assert_eq!(red[i], lut.values[i * 3]);
            assert_eq!(green[i], lut.values[i * 3 + 1]);
            assert_eq!(blue[i], lut.values[i * 3 + 2]);
        }
    }

    #[test]
    fn simple_3d() {
        let content = format!("{}3 3 3\n{}", header_3d(), CUBE_333);
        let file = read(&content, Interpolation::Tetrahedral).unwrap();
        assert_eq!(metadata(&file), "foobar\n");

        // As in & out preLut values are the same there is nothing to do.
        assert_eq!(file.group.num_transforms(), 1);
        let Transform::Lut3D(lut) = &file.group.transforms[0] else {
            panic!("expected a Lut3D")
        };
        assert_eq!(lut.interpolation, Interpolation::Tetrahedral);

        let v = [0.0f32, 0.5, 1.0];
        let mut expected = Vec::new();
        for r in v {
            for g in v {
                for b in v {
                    expected.extend_from_slice(&[r, g, b]);
                }
            }
        }
        assert_eq!(lut.values, expected);
    }

    #[test]
    fn prelut_range() {
        let content = "CSPLUTV100\n3D\n\n2\n-1.0 3.0\n0.0 1.0\n2\n0.0 1.0\n0.0 1.0\n2\n0.0 1.0\n0.0 1.0\n\n2 2 2\n0 0 0\n1 0 0\n0 1 0\n1 1 0\n0 0 1\n1 0 1\n0 1 1\n1 1 1\n";
        let file = read(content, Interpolation::Default).unwrap();
        assert_eq!(file.group.num_transforms(), 3);
        let Transform::Matrix(m) = &file.group.transforms[0] else {
            panic!("expected a matrix")
        };
        assert_eq!(m.matrix[0], 0.25);
        assert_eq!(m.offset[0], 0.25);
        assert_eq!(m.matrix[5], 1.0);
        let Transform::Lut1D(prelut) = &file.group.transforms[1] else {
            panic!("expected a prelut")
        };
        // Red spline goes from 0 to 1 over the resampled domain.
        assert_eq!(prelut.values[0], 0.0);
        assert!((prelut.values[3 * 65535] - 1.0).abs() < 1e-6);
        assert!((prelut.values[3 * 32768] - 0.5).abs() < 1e-4);
        assert!(matches!(file.group.transforms[2], Transform::Lut3D(_)));
    }

    #[test]
    fn less_strict_parse() {
        let content = format!(
            " CspluTV100 malformed\n3D\n\n BegIN MEtadATA malformed malformed malfo\nfoobar\n   end metadata malformed malformed m a l\n\n{}2 2 2\n0.100000 0.100000 0.100000\n1.100000 0.100000 0.100000\n0.100000 1.100000 0.100000\n1.100000 1.100000 0.100000\n0.100000 0.100000 1.100000\n1.100000 0.100000 1.100000\n0.100000 1.100000 1.100000\n1.100000 1.100000 1.100000\n",
            PRELUT_3D
        );
        let file = read(&content, Interpolation::Default).unwrap();
        assert_eq!(metadata(&file), "foobar\n");
        // As in & out from the preLut are the same, there is nothing to do.
        assert_eq!(file.group.num_transforms(), 1);
    }

    #[test]
    fn failures_1d() {
        // Empty.
        check_error("", "file stream empty");
        // Wrong first line.
        check_error("CSPLUTV2000\n1D\n\n", "expected 'CSPLUTV100'");
        // Missing LUT.
        check_error(
            "CSPLUTV100\n\nBEGIN METADATA\nfoobar\nEND METADATA\n",
            "Require 1D or 3D",
        );
        // Can't read prelut size.
        check_error(
            &SIMPLE_1D.replacen("\n2\n0.0 1.0\n", "\nA\n0.0 1.0\n", 1),
            "Prelut does not specify valid dimension size",
        );
        // Prelut has too many points.
        check_error(
            &SIMPLE_1D.replacen("\n2\n0.0 1.0\n", "\n2\n0.0 1.0 1.0\n", 1),
            "expected number of data points",
        );
        // Can't read a float in prelut.
        check_error(
            &SIMPLE_1D.replacen("\n2\n0.0 1.0\n", "\n2\n0.0 notFloat\n", 1),
            "Prelut data is malformed",
        );
        // Bad number of LUT entries.
        check_error(
            &SIMPLE_1D.replacen("\n6\n0.0 0.0 0.0\n", "\n-6\n0.0 0.0 0.0\n", 1),
            "1D LUT with invalid number of entries",
        );
        // Too many components on LUT entry.
        check_error(
            &SIMPLE_1D.replacen("\n6\n0.0 0.0 0.0\n", "\n6\n0.0 0.0 0.0 0.0\n", 1),
            "must contain three numbers",
        );
    }

    #[test]
    fn failures_3d() {
        let cube = &CUBE_333[12..]; // Drop the first entry.
                                    // Cube size has only 2 entries.
        check_error(
            &format!("{}3 3\n1.0 0.0 0.0\n{}", header_3d(), cube),
            "couldn't read cube size",
        );
        // Cube sizes are not equal.
        check_error(
            &format!("{}3 3 4\n1.0 0.0 0.0\n{}", header_3d(), cube),
            "nonuniform cube sizes",
        );
        // Cube size is not > 0.
        check_error(
            &format!("{}-3 -3 -3\n1.0 0.0 0.0\n{}", header_3d(), cube),
            "invalid cube size",
        );
        // One LUT entry has 4 components.
        check_error(
            &format!("{}3 3 3\n0.5 0.5 0.0 1.0\n{}", header_3d(), cube),
            "couldn't read cube row",
        );
        // One LUT entry has 2 components.
        check_error(
            &format!("{}3 3 3\n1.0 1.0\n{}", header_3d(), cube),
            "couldn't read cube row",
        );
        // One LUT entry can't be converted to 3 floats.
        check_error(
            &format!("{}3 3 3\n1.0 0.5 One\n{}", header_3d(), cube),
            "couldn't read cube row",
        );
    }

    #[test]
    fn unterminated_metadata() {
        // Must not loop forever.
        check_error(
            "CSPLUTV100\n3D\nBEGIN METADATA\nfoobar\n",
            "Prelut does not specify valid dimension size",
        );
    }

    #[test]
    fn interpolator() {
        // Linear data is reproduced exactly by the spline.
        let interp = Interpolator1D::new(&[0.0, 0.5, 1.0], &[0.0, 1.0, 2.0]);
        assert_eq!(interp.interpolate(-1.0), 0.0);
        assert_eq!(interp.interpolate(2.0), 2.0);
        assert!((interp.interpolate(0.25) - 0.5).abs() < 1e-6);
        assert!(interp.interpolate(f32::NAN).is_nan());
        let interp = Interpolator1D::new(&[0.0, 1.0], &[1.0, 3.0]);
        assert_eq!(interp.interpolate(0.5), 2.0);
    }

    #[test]
    fn file() {
        let path = format!(
            "{}/tests/data/files/lut3d_arbitrary.csp",
            env!("CARGO_MANIFEST_DIR")
        );
        let data = std::fs::read(&path).unwrap();
        let file = LocalFileFormat
            .read(&data, &path, Interpolation::Default)
            .unwrap();
        assert!(matches!(
            file.group.transforms.last().unwrap(),
            Transform::Lut3D(_)
        ));
    }

    // Baker tests (port of the baker parts of `FileFormatCSP_tests.cpp`).

    use crate::fileformats::utils::bake_test_utils::{
        bake, baker, check_round_trip, compare_lines, config_yaml,
    };

    fn target_offset_config(extra: &[(&str, &str)]) -> String {
        let mut spaces = vec![("lnf", "")];
        spaces.extend_from_slice(extra);
        spaces.push((
            "target",
            "from_scene_reference: !<CDLTransform> {offset: [0.1, 0.1, 0.1]}",
        ));
        config_yaml(&spaces)
    }

    const CUBE_OFFSET: &str = "2 2 2\n\
        0.100000 0.100000 0.100000\n\
        1.100000 0.100000 0.100000\n\
        0.100000 1.100000 0.100000\n\
        1.100000 1.100000 0.100000\n\
        0.100000 0.100000 1.100000\n\
        1.100000 0.100000 1.100000\n\
        0.100000 1.100000 1.100000\n\
        1.100000 1.100000 1.100000\n\
        \n";

    #[test]
    fn complete_3d() {
        let config = target_offset_config(&[(
            "shaper",
            "to_scene_reference: !<ExponentTransform> {value: [2.6, 2.6, 2.6, 1]}",
        )]);
        let mut b = baker(&config, "cinespace");
        b.format_metadata_mut()
            .add_child_element(METADATA_DESCRIPTION, "date: 2011:02:21 15:22:55");
        b.format_metadata_mut()
            .add_child_element(METADATA_DESCRIPTION, "Baked by OCIO");
        b.set_input_space("lnf");
        b.set_shaper_space("shaper");
        b.set_target_space("target");
        b.set_shaper_size(Some(10));
        b.set_cube_size(Some(2));

        let shaper_in = "0.000000 0.003303 0.020028 0.057476 0.121430 0.216916 0.348468 0.520265 0.736213 1.000000";
        let shaper_out = "0.000000 0.111111 0.222222 0.333333 0.444444 0.555556 0.666667 0.777778 0.888889 1.000000";
        let expected = format!(
            "CSPLUTV100\n3D\n\nBEGIN METADATA\ndate: 2011:02:21 15:22:55\nBaked by OCIO\nEND METADATA\n\n\
             10\n{shaper_in}\n{shaper_out}\n10\n{shaper_in}\n{shaper_out}\n10\n{shaper_in}\n{shaper_out}\n\n{CUBE_OFFSET}"
        );
        let out = bake(&b);
        compare_lines(&out, &expected, 1e-5, |i| i > 6);
        assert_eq!(out, expected);
    }

    #[test]
    fn shaper_hdr() {
        let config = config_yaml(&[
            ("lnf", ""),
            (
                "lnf_tweak",
                "from_scene_reference: !<CDLTransform> {offset: [2, -2, 0.9]}",
            ),
            (
                "target",
                "from_scene_reference: !<CDLTransform> {offset: [0.1, 0.1, 0.1]}",
            ),
        ]);
        let mut b = baker(&config, "cinespace");
        b.format_metadata_mut()
            .add_child_element(METADATA_DESCRIPTION, "date: 2011:02:21 15:22:55");
        b.set_input_space("lnf_tweak");
        b.set_shaper_space("lnf");
        b.set_target_space("target");
        b.set_shaper_size(Some(10));
        b.set_cube_size(Some(2));

        let shaper_out = "0.000000 0.111111 0.222222 0.333333 0.444444 0.555556 0.666667 0.777778 0.888889 1.000000";
        let expected = format!(
            "CSPLUTV100\n3D\n\nBEGIN METADATA\ndate: 2011:02:21 15:22:55\nEND METADATA\n\n\
             10\n2.000000 2.111111 2.222222 2.333333 2.444444 2.555556 2.666667 2.777778 2.888889 3.000000\n{shaper_out}\n\
             10\n-2.000000 -1.888889 -1.777778 -1.666667 -1.555556 -1.444444 -1.333333 -1.222222 -1.111111 -1.000000\n{shaper_out}\n\
             10\n0.900000 1.011111 1.122222 1.233333 1.344444 1.455556 1.566667 1.677778 1.788889 1.900000\n{shaper_out}\n\n{CUBE_OFFSET}"
        );
        let out = bake(&b);
        compare_lines(&out, &expected, 1e-5, |i| i > 6);
    }

    #[test]
    fn no_shaper() {
        let config = target_offset_config(&[]);
        let mut b = baker(&config, "cinespace");
        b.format_metadata_mut()
            .add_child_element(METADATA_DESCRIPTION, "date: 2011:02:21 15:22:55");
        b.set_input_space("lnf");
        b.set_target_space("target");
        b.set_shaper_size(Some(10));
        b.set_cube_size(Some(2));

        // The input space has a uniform allocation, so a 2 entries shaper.
        let shaper = "2\n0.000000 1.000000\n0.000000 1.000000\n";
        let expected = format!(
            "CSPLUTV100\n3D\n\nBEGIN METADATA\ndate: 2011:02:21 15:22:55\nEND METADATA\n\n\
             {shaper}{shaper}{shaper}\n{CUBE_OFFSET}"
        );
        assert_eq!(bake(&b), expected);
    }

    #[test]
    fn bake_lg2_allocation() {
        // Port of the cinespace part of the `Baker, bake_3dlut` test: the
        // shaper is derived from the lg2 allocation of the input space.
        let config = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: lnh}

colorspaces:
  - !<ColorSpace>
    name : lnh
    bitdepth : 16f
    isdata : false
    allocation : lg2

  - !<ColorSpace>
    name : gamma22
    bitdepth : 8ui
    isdata : false
    allocation : uniform
    to_reference : !<ExponentTransform> {value: [2.2, 2.2, 2.2, 1]}
"#;
        let mut b = baker(config, "cinespace");
        b.format_metadata_mut()
            .add_child_element("Desc", "this is some metadata!");
        b.set_input_space("lnh");
        b.set_target_space("gamma22");
        b.set_shaper_size(Some(4));
        b.set_cube_size(Some(2));

        let expected = "CSPLUTV100\n\
            3D\n\
            \n\
            BEGIN METADATA\n\
            this is some metadata!\n\
            END METADATA\n\
            \n\
            4\n\
            0.000977 0.039373 1.587401 64.000000\n\
            0.000000 0.333333 0.666667 1.000000\n\
            4\n\
            0.000977 0.039373 1.587401 64.000000\n\
            0.000000 0.333333 0.666667 1.000000\n\
            4\n\
            0.000977 0.039373 1.587401 64.000000\n\
            0.000000 0.333333 0.666667 1.000000\n\
            \n\
            2 2 2\n\
            0.042823 0.042823 0.042823\n\
            6.622026 0.042823 0.042823\n\
            0.042823 6.622026 0.042823\n\
            6.622026 6.622026 0.042823\n\
            0.042823 0.042823 6.622026\n\
            6.622026 0.042823 6.622026\n\
            0.042823 6.622026 6.622026\n\
            6.622026 6.622026 6.622026\n\
            \n";
        compare_lines(&bake(&b), expected, 1e-5, |i| i > 6);
    }

    #[test]
    fn bake_defaults() {
        let config = target_offset_config(&[(
            "shaper",
            "to_scene_reference: !<ExponentTransform> {value: [2.6, 2.6, 2.6, 1]}",
        )]);
        let mut b = baker(&config, "cinespace");
        b.set_input_space("lnf");
        b.set_target_space("target");
        let out = bake(&b);
        let lines: Vec<&str> = out.lines().collect();
        // No metadata, 2 entries shaper (uniform allocation), 32^3 cube.
        assert_eq!(lines[3], "BEGIN METADATA");
        assert_eq!(lines[4], "END METADATA");
        assert_eq!(lines[6], "2");
        assert_eq!(lines[15], "");
        assert_eq!(lines[16], "32 32 32");
        assert_eq!(lines.len(), 17 + 32 * 32 * 32 + 1);

        // Default shaper size with a shaper space.
        b.set_shaper_space("shaper");
        let out = bake(&b);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[6], "1024");
        assert_eq!(lines[7].split(' ').count(), 1024);
    }

    #[test]
    fn bake_round_trip() {
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
        let samples = [
            [0.0, 0.0, 0.0],
            [0.25, 0.5, 0.75],
            [0.9, 0.1, 0.4],
            [1.0, 1.0, 1.0],
        ];

        let mut b = baker(&config, "cinespace");
        b.set_input_space("lnf");
        b.set_target_space("target");
        b.set_cube_size(Some(9));
        check_round_trip(&b, &samples, 1e-5);

        b.set_shaper_space("shaper");
        b.set_shaper_size(Some(64));
        b.set_cube_size(Some(33));
        // The cube is sampled in the (non linear) shaper space.
        check_round_trip(&b, &samples, 5e-4);
    }
}
