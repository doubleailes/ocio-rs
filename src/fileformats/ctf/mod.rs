//! CLF / CTF file formats (port of OCIO `fileformats/FileFormatCTF.cpp` and
//! the `fileformats/ctf` directory).
//!
//! Reading produces a [`CachedFile`] whose group holds plain transforms
//! (matrices, ranges, LUTs, ...), with the file bit-depths and all the
//! metadata recorded on the transforms. Writing converts a group transform
//! to CLF or CTF XML, generating the same text as OCIO.

pub(crate) mod opdata;
mod reader;
mod transform;
mod writer;
pub(crate) mod xml;

#[cfg(test)]
mod tests_read;
#[cfg(test)]
mod tests_write;

use super::{
    bake_capability, capability, CachedFile, FileFormat, FormatInfo, FormatRegistry,
    FILEFORMAT_CLF, FILEFORMAT_CTF,
};
use crate::baker::{self, Baker};
use crate::config::Config;
use crate::context::Context;
use crate::error::{Error, Result};
use crate::format_metadata::{FormatMetadata, METADATA_ROOT};
use crate::transforms::{
    GroupTransform, Lut1DTransform, Lut3DTransform, RangeTransform, Transform,
};
use crate::types::Interpolation;
use opdata::{Lut1DData, Lut3DData, OpData};

/// The CLF / CTF reader and writer.
struct LocalFileFormat;

pub(crate) fn create() -> Box<dyn FileFormat> {
    Box::new(LocalFileFormat)
}

/// Port of `Lut1DOpData::GetConcreteInterpolation` (only linear is
/// implemented for 1D LUTs).
fn lut1d_concrete_interpolation(_i: Interpolation) -> Interpolation {
    Interpolation::Linear
}

/// Port of `Lut3DOpData::GetConcreteInterpolation`.
fn lut3d_concrete_interpolation(i: Interpolation) -> Interpolation {
    match i {
        Interpolation::Best | Interpolation::Tetrahedral => Interpolation::Tetrahedral,
        _ => Interpolation::Linear,
    }
}

/// Port of `HandleLUT`: a LUT that does not specify an interpolation in
/// the file uses the `FileTransform` interpolation (if valid for the LUT).
fn handle_lut_interpolation(op: &mut OpData, file_interp: Interpolation) {
    match op {
        OpData::Lut1D(l) => {
            if Lut1DData::is_valid_interpolation(file_interp)
                && l.interpolation == Interpolation::Default
                && lut1d_concrete_interpolation(l.interpolation)
                    != lut1d_concrete_interpolation(file_interp)
            {
                l.interpolation = file_interp;
            }
        }
        OpData::Lut3D(l) => {
            if Lut3DData::is_valid_interpolation(file_interp)
                && l.interpolation == Interpolation::Default
                && lut3d_concrete_interpolation(l.interpolation)
                    != lut3d_concrete_interpolation(file_interp)
            {
                l.interpolation = file_interp;
            }
        }
        _ => {}
    }
}

/// Parse a CLF/CTF file into a group transform (the `ProcessList`
/// metadata goes into the group metadata).
fn read_group(data: &[u8], file_name: &str, interp: Interpolation) -> Result<GroupTransform> {
    let parsed = reader::parse_ctf(data, file_name)?;
    let t = parsed.transform;
    let mut group = GroupTransform::new();
    let mut metadata = FormatMetadata::new(METADATA_ROOT, "");
    t.to_metadata(&mut metadata);
    group.metadata = metadata;
    for op in &t.ops {
        let mut op = op.clone();
        handle_lut_interpolation(&mut op, interp);
        if let Some(tr) = op.to_transform() {
            group.transforms.push(tr);
        }
    }
    Ok(group)
}

/// Map a format name to the writer sub-format (case insensitive).
fn sub_format(format_name: &str) -> Result<writer::SubFormat> {
    if format_name.eq_ignore_ascii_case(FILEFORMAT_CLF) {
        Ok(writer::SubFormat::Clf)
    } else if format_name.eq_ignore_ascii_case(FILEFORMAT_CTF) {
        Ok(writer::SubFormat::Ctf)
    } else {
        crate::bail!(
            "Error: CLF/CTF writer does not also write format {}.",
            format_name
        )
    }
}

impl FileFormat for LocalFileFormat {
    fn format_info(&self) -> Vec<FormatInfo> {
        let caps = capability::READ | capability::BAKE | capability::WRITE;
        let bake = bake_capability::LUT3D | bake_capability::LUT1D | bake_capability::LUT1D_3D;
        vec![
            FormatInfo {
                name: FILEFORMAT_CLF,
                extension: "clf",
                capabilities: caps,
                bake_capabilities: bake,
            },
            FormatInfo {
                name: FILEFORMAT_CTF,
                extension: "ctf",
                capabilities: caps,
                bake_capabilities: bake,
            },
        ]
    }

    fn read(
        &self,
        data: &[u8],
        original_file_name: &str,
        interp: Interpolation,
    ) -> Result<CachedFile> {
        Ok(CachedFile::new(read_group(
            data,
            original_file_name,
            interp,
        )?))
    }

    fn bake(&self, baker: &Baker, format_name: &str) -> Result<Vec<u8>> {
        bake_clf(baker, format_name).map(String::into_bytes)
    }

    fn write(
        &self,
        config: &Config,
        context: &Context,
        group: &GroupTransform,
        format_name: &str,
    ) -> Result<String> {
        let sub = sub_format(format_name)?;
        writer::write_group(config, context, group, sub)
    }
}

/// Port of `LocalFileFormat::bake`: a (half-domain) 1D shaper LUT and/or a
/// 3D LUT, depending on the crosstalk of the baked transform.
fn bake_clf(baker: &Baker, format_name: &str) -> Result<String> {
    const DEFAULT_1D_SIZE: usize = 4096;
    const DEFAULT_3D_SIZE: usize = 64;

    if format_name != FILEFORMAT_CTF && format_name != FILEFORMAT_CLF {
        crate::bail!("Unknown CLF/CTF file format name, '{}'.", format_name);
    }
    let config = baker
        .config
        .as_ref()
        .ok_or_else(|| Error::msg("No OCIO config has been set"))?;

    // NB: As in OCIO, the 1D size is taken from the cube size.
    let oned_size = baker.cube_size.unwrap_or(DEFAULT_1D_SIZE);
    let cube_size = baker.cube_size.unwrap_or(DEFAULT_3D_SIZE).max(2);
    let shaper_space = baker.shaper_space.as_str();

    #[derive(PartialEq)]
    enum Required {
        Lut1D,
        Lut3D,
        Lut1D3D,
    }

    let input_to_target = baker::input_to_target_processor(baker)?;
    let required = if input_to_target.has_channel_crosstalk() {
        if shaper_space.is_empty() {
            Required::Lut3D
        } else {
            Required::Lut1D3D
        }
    } else {
        Required::Lut1D
    };

    // Shaper.
    let mut shaper: Option<Lut1DTransform> = None;
    let mut from_in_start = 0.0f32;
    let mut from_in_end = 1.0f32;
    if required == Required::Lut1D3D {
        let mut lut = match baker.shaper_size {
            None => {
                // Half-domain identity shaper (NaNs are filtered).
                let mut lut = Lut1DTransform::new(65536, true);
                for idx in 0..65536usize {
                    let bits = u16::try_from(idx).unwrap_or(u16::MAX);
                    let mut f = half::f16::from_bits(bits).to_f32();
                    if f.is_nan() {
                        f = 0.0;
                    }
                    lut.values[idx * 3] = f;
                    lut.values[idx * 3 + 1] = f;
                    lut.values[idx * 3 + 2] = f;
                }
                lut
            }
            Some(size) => {
                let (s, e) = baker::shaper_range(baker)?;
                from_in_start = s;
                from_in_end = e;
                let mut lut = Lut1DTransform::new(size, false);
                if from_in_start != 0.0 || from_in_end != 1.0 {
                    generate_linear_scale_lut1d(&mut lut.values, size, from_in_start, from_in_end);
                }
                lut
            }
        };
        let input_to_shaper = baker::input_to_shaper_processor(baker)?;
        input_to_shaper.apply_rgb_slice(&mut lut.values);
        shaper = Some(lut);
    }

    // 3D LUT.
    let mut cube_data = Vec::new();
    if required == Required::Lut3D || required == Required::Lut1D3D {
        cube_data = vec![0.0f32; cube_size * cube_size * cube_size * 3];
        let c = 1.0f32 / (cube_size as f32 - 1.0);
        for i in 0..cube_size * cube_size * cube_size {
            cube_data[3 * i] = ((i / cube_size / cube_size) % cube_size) as f32 * c;
            cube_data[3 * i + 1] = ((i / cube_size) % cube_size) as f32 * c;
            cube_data[3 * i + 2] = (i % cube_size) as f32 * c;
        }
        if required == Required::Lut1D3D {
            baker::shaper_to_target_processor(baker)?.apply_rgb_slice(&mut cube_data);
        } else {
            input_to_target.apply_rgb_slice(&mut cube_data);
        }
    }

    // 1D LUT.
    let mut oned_data = Vec::new();
    if required == Required::Lut1D {
        oned_data = vec![0.0f32; oned_size * 3];
        if !shaper_space.is_empty() {
            let (s, e) = baker::shaper_range(baker)?;
            from_in_start = s;
            from_in_end = e;
            generate_linear_scale_lut1d(&mut oned_data, oned_size, from_in_start, from_in_end);
        } else {
            let scale = 1.0f32 / (oned_size as f32 - 1.0);
            for i in 0..oned_size {
                let v = scale * i as f32;
                oned_data[3 * i] = v;
                oned_data[3 * i + 1] = v;
                oned_data[3 * i + 2] = v;
            }
        }
        input_to_target.apply_rgb_slice(&mut oned_data);
    }

    // Write.
    let mut group = GroupTransform::new();
    let range = || {
        Transform::Range(RangeTransform {
            min_in: Some(f64::from(from_in_start)),
            max_in: Some(f64::from(from_in_end)),
            min_out: Some(0.0),
            max_out: Some(1.0),
            ..Default::default()
        })
    };
    match required {
        Required::Lut1D => {
            if from_in_start != 0.0 || from_in_end != 1.0 {
                group.transforms.push(range());
            }
            let mut lut = Lut1DTransform::new(oned_size, false);
            lut.values = oned_data;
            group.transforms.push(Transform::Lut1D(lut));
        }
        Required::Lut1D3D => {
            if from_in_start != 0.0 || from_in_end != 1.0 {
                group.transforms.push(range());
            }
            if let Some(s) = shaper {
                group.transforms.push(Transform::Lut1D(s));
            }
        }
        Required::Lut3D => {}
    }
    if required == Required::Lut3D || required == Required::Lut1D3D {
        let mut lut = Lut3DTransform::new(cube_size);
        lut.values = cube_data;
        group.transforms.push(Transform::Lut3D(lut));
    }
    group.metadata = baker.metadata.clone();
    let sub = sub_format(format_name)?;
    writer::write_group(config, config.current_context(), &group, sub)
}

/// Port of `GenerateLinearScaleLut1D` (3 channels).
fn generate_linear_scale_lut1d(values: &mut [f32], num: usize, start: f32, end: f32) {
    for i in 0..num {
        let x = (i as f64 / (num as f64 - 1.0)) as f32;
        let v = (end - start) * x + start;
        if let Some(px) = values.get_mut(3 * i..3 * i + 3) {
            px.fill(v);
        }
    }
}

impl GroupTransform {
    /// Write the group transform into the format named `format_name`
    /// (e.g. "Academy/ASC Common LUT Format", "Color Transform Format", or
    /// the ASC CDL formats), using `config` to convert the transforms that
    /// can not be written directly (port of `GroupTransform::write`).
    pub fn write(&self, config: &Config, format_name: &str) -> Result<String> {
        let fmt = FormatRegistry::instance()
            .format_by_name(format_name)
            .ok_or_else(|| {
                Error::msg(format!(
                    "The format named '{format_name}' could not be found. "
                ))
            })?;
        fmt.write(config, config.current_context(), self, format_name)
            .map_err(|e| {
                Error::msg(format!(
                    "Error writing format '{}': {}",
                    format_name,
                    e.message()
                ))
            })
    }

    /// Names of the formats that can be written.
    pub fn write_format_names() -> Vec<&'static str> {
        FormatRegistry::instance()
            .format_infos(capability::WRITE)
            .into_iter()
            .map(|i| i.name)
            .collect()
    }

    /// Number of formats that can be written (port of `GetNumWriteFormats`).
    pub fn num_write_formats() -> usize {
        FormatRegistry::instance().num_formats(capability::WRITE)
    }

    /// Name of the `index`-th write format, `""` if out of range (port of
    /// `GetFormatNameByIndex`).
    pub fn format_name_by_index(index: usize) -> &'static str {
        FormatRegistry::instance()
            .format_infos(capability::WRITE)
            .get(index)
            .map(|i| i.name)
            .unwrap_or("")
    }

    /// Extension of the `index`-th write format, `""` if out of range (port
    /// of `GetFormatExtensionByIndex`).
    pub fn format_extension_by_index(index: usize) -> &'static str {
        FormatRegistry::instance()
            .format_infos(capability::WRITE)
            .get(index)
            .map(|i| i.extension)
            .unwrap_or("")
    }
}
