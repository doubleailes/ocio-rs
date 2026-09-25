//! ICC profiles (`icc`, `icm`, `pf`) (port of `FileFormatICC.cpp` and of
//! the minimal SampleICC profile reader `iccProfileReader.h`).
//!
//! ICC profiles are a widely used method of storing color information for
//! computer displays and that is the main purpose of this format reader.
//! The "matrix/TRC" model for a monitor is parsed and converted into an
//! OCIO compatible form. Other types of ICC profiles are not supported.
//!
//! The transforms describe the PCS (CIE XYZ D65) to display code value
//! direction (the forward direction of the file transform):
//! `[D50->D65 Bradford matrix (inverse), RGB->XYZ matrix (inverse), TRC
//! (inverse)]`. The TRC is a 1D LUT for sampled and parametric (types 1-4)
//! curves, and an exponent (mirrored, i.e. not clamped) for single gamma
//! curves.

use super::utils::{new_lut1d, BinaryStream};
use super::{bake_capability, capability, CachedFile, FileFormat, FormatInfo};
use crate::error::{Error, Result};
use crate::path_utils;
use crate::transforms::{ExponentTransform, GroupTransform, MatrixTransform};
use crate::types::{BitDepth, Interpolation, NegativeStyle, TransformDirection};

// ---------------------------------------------------------------------------
// ICC constants (from icProfileHeader.h).

const IC_MAGIC_NUMBER: u32 = 0x6163_7370; // 'acsp'

const IC_SIG_PROFILE_DESCRIPTION_TAG: u32 = 0x6465_7363; // 'desc'
const IC_SIG_PROFILE_DESCRIPTION_ML_TAG: u32 = 0x6473_636d; // 'dscm' (Apple)
const IC_SIG_RED_COLORANT_TAG: u32 = 0x7258_595A; // 'rXYZ'
const IC_SIG_GREEN_COLORANT_TAG: u32 = 0x6758_595A; // 'gXYZ'
const IC_SIG_BLUE_COLORANT_TAG: u32 = 0x6258_595A; // 'bXYZ'
const IC_SIG_RED_TRC_TAG: u32 = 0x7254_5243; // 'rTRC'
const IC_SIG_GREEN_TRC_TAG: u32 = 0x6754_5243; // 'gTRC'
const IC_SIG_BLUE_TRC_TAG: u32 = 0x6254_5243; // 'bTRC'

const IC_SIG_XYZ_ARRAY_TYPE: u32 = 0x5859_5A20; // 'XYZ '
const IC_SIG_PARAMETRIC_CURVE_TYPE: u32 = 0x7061_7261; // 'para'
const IC_SIG_CURVE_TYPE: u32 = 0x6375_7276; // 'curv'
const IC_SIG_TEXT_DESCRIPTION_TYPE: u32 = 0x6465_7363; // 'desc'
const IC_SIG_MULTI_LOCALIZED_UNICODE_TYPE: u32 = 0x6d6c_7563; // 'mluc'

const IC_SIG_INPUT_CLASS: u32 = 0x7363_6E72; // 'scnr'
const IC_SIG_DISPLAY_CLASS: u32 = 0x6D6E_7472; // 'mntr'
const IC_SIG_OUTPUT_CLASS: u32 = 0x7072_7472; // 'prtr'
const IC_SIG_LINK_CLASS: u32 = 0x6C69_6E6B; // 'link'
const IC_SIG_ABSTRACT_CLASS: u32 = 0x6162_7374; // 'abst'
const IC_SIG_COLOR_SPACE_CLASS: u32 = 0x7370_6163; // 'spac'
const IC_SIG_NAMED_COLOR_CLASS: u32 = 0x6e6d_636c; // 'nmcl'

const IC_LANGUAGE_CODE_ENGLISH: u16 = 0x656E; // 'en'
const IC_COUNTRY_CODE_USA: u16 = 0x5553; // 'US'
const IC_COUNTRY_CODE_UNITED_KINGDOM: u16 = 0x554B; // 'UK'

/// Maximum number of tags in a profile.
const MAX_NUM_ICC_TAGS: u32 = 100;

/// Bradford matrix converting a D50 XYZ to a D65 XYZ.
///
/// The PCS white point is D50, but for the purposes of OCIO it is much more
/// convenient for the profile to be balanced to D65 since that is the
/// native white point that most displays will be balanced to.
const D50_TO_D65_M44: [f64; 16] = [
    0.955509474537,
    -0.023074829492,
    0.063312392987,
    0.0, //
    -0.028327238868,
    1.00994465504,
    0.021055592145,
    0.0, //
    0.012329273379,
    -0.020536209966,
    1.33072998567,
    0.0, //
    0.0,
    0.0,
    0.0,
    1.0,
];

/// Port of `SampleICC::icFtoD` (s15Fixed16 to float).
fn ic_f_to_d(num: i32) -> f32 {
    (num as f64 / 65536.0) as f32
}

// ---------------------------------------------------------------------------
// Tag readers (port of the SampleICC type readers).

/// The content of a tag.
#[derive(Debug, Clone)]
enum IccTag {
    TextDescription(String),
    MultiLocalizedUnicode(String),
    XyzArray([i32; 3]),
    ParametricCurve {
        function_type: u16,
        params: Vec<i32>,
    },
    Curve(Vec<f32>),
}

impl IccTag {
    fn is_parametric_curve(&self) -> bool {
        matches!(self, IccTag::ParametricCurve { .. })
    }
}

/// `textDescriptionType` (ICC v2). `size` includes the already read
/// signature.
fn read_text_description(s: &mut BinaryStream, size: u32) -> Option<IccTag> {
    if 4 + 4 + 4 > size || !s.good() {
        return None;
    }
    let _reserved = s.read_u32()?;
    // Read the size of the string.
    let text_size = s.read_u32()?;
    let mut text = String::new();
    if text_size > 0 {
        let bytes = s.read(text_size as usize)?;
        // Removes the extra '\0' if any.
        let end = bytes.iter().position(|&c| c == 0).unwrap_or(bytes.len());
        text = String::from_utf8_lossy(&bytes[..end]).into_owned();
    }
    Some(IccTag::TextDescription(text))
}

/// `multiLocalizedUnicodeType`: only the English string is returned (USA,
/// then UK, then the first English, then the first string).
fn read_multi_localized_unicode(s: &mut BinaryStream, size: u32) -> Option<IccTag> {
    if 4 + 4 * 3 > size || !s.good() {
        return None;
    }
    let _reserved = s.read_u32()?;
    let num_rec = s.read_u32()?;
    let rec_size = s.read_u32()?;

    // Recognized version name records are 12 bytes each.
    if rec_size != 12 {
        return None;
    }

    let mut found_usa = String::new();
    let mut found_uk = String::new();
    let mut found_en = String::new();
    let mut found_first = String::new();

    for i in 0..num_rec as u64 {
        if 4 * 4 + (i + 1) * 12 > size as u64 {
            return None;
        }

        let language_code = s.read_u16()?;
        let region_code = s.read_u16()?;
        let length = s.read_u32()?;
        let offset = s.read_u32()?;

        if offset as u64 + length as u64 > size as u64 {
            return None;
        }

        let num_char = (length / 2) as usize;
        let mut chars = Vec::with_capacity(num_char);
        for _ in 0..num_char {
            chars.push(s.read_u16()?);
        }

        // As only the English language is supported the Basic Latin
        // character set is enough for all the English characters.
        let bytes: Vec<u8> = chars.iter().map(|&c| c as u8).collect();
        let end = bytes.iter().position(|&c| c == 0).unwrap_or(bytes.len());
        let text = String::from_utf8_lossy(&bytes[..end]).into_owned();

        // As the order of the (country, language) is unknown, read all the
        // strings before selecting the right one.
        if region_code == IC_COUNTRY_CODE_USA {
            // As soon as the US is found stop.
            found_usa = text;
            break;
        }
        if region_code == IC_COUNTRY_CODE_UNITED_KINGDOM && found_uk.is_empty() {
            found_uk = text.clone();
        }
        if language_code == IC_LANGUAGE_CODE_ENGLISH && found_en.is_empty() {
            found_en = text.clone();
        }
        if i == 0 {
            found_first = text;
        }
    }

    let text = if !found_usa.is_empty() {
        found_usa
    } else if !found_uk.is_empty() {
        found_uk
    } else if !found_en.is_empty() {
        found_en
    } else {
        found_first
    };
    Some(IccTag::MultiLocalizedUnicode(text))
}

/// `XYZType` holding a single XYZ value.
fn read_xyz_array(s: &mut BinaryStream, size: u32) -> Option<IccTag> {
    if 4 + 4 + 12 > size || !s.good() {
        return None;
    }
    // Only used to read a single XYZ.
    if (size - 8) / 12 != 1 {
        return None;
    }
    let _reserved = s.read_u32()?;
    let x = s.read_i32()?;
    let y = s.read_i32()?;
    let z = s.read_i32()?;
    Some(IccTag::XyzArray([x, y, z]))
}

/// `parametricCurveType`.
fn read_parametric_curve(s: &mut BinaryStream, size: u32) -> Option<IccTag> {
    let hdr_size: u32 = 4 + 4 + 2 * 2;
    if hdr_size > size || hdr_size + 4 > size || !s.good() {
        return None;
    }
    let _reserved32 = s.read_u32()?;
    let function_type = s.read_u16()?;
    let _reserved16 = s.read_u16()?;

    let num_param = ((size - hdr_size) / 4) as u16;
    let mut params = Vec::with_capacity(num_param as usize);
    for _ in 0..num_param {
        params.push(s.read_i32()?);
    }
    Some(IccTag::ParametricCurve {
        function_type,
        params,
    })
}

/// `curveType`: the entries are normalized by 65535.
fn read_curve(s: &mut BinaryStream, size: u32) -> Option<IccTag> {
    if 4 + 4 + 4 > size || !s.good() {
        return None;
    }
    let _reserved = s.read_u32()?;
    let size_data = s.read_u32()?;
    // ICC curve entries are indexed by 16-bit values; 65536 is the maximum.
    if size_data > 65536 {
        return None;
    }
    let mut curve = Vec::with_capacity(size_data as usize);
    for _ in 0..size_data {
        curve.push(s.read_u16()? as f32 / 65535.0f32);
    }
    Some(IccTag::Curve(curve))
}

/// An entry of the tag table.
#[derive(Debug, Clone)]
struct TagInfo {
    sig: u32,
    offset: u32,
    size: u32,
}

/// The profile header and tag table (port of `SampleICC::IccContent`).
struct IccContent {
    device_class: u32,
    rendering_intent: u32,
    tags: Vec<TagInfo>,
    /// Loaded tags (`None` if not loaded yet, `Some(None)` if the load
    /// failed).
    loaded: Vec<Option<Option<IccTag>>>,
}

impl IccContent {
    /// Load (once) and return the tag with the given signature.
    fn load_tag(&mut self, s: &mut BinaryStream, sig: u32) -> Option<IccTag> {
        let index = self.tags.iter().position(|t| t.sig == sig)?;
        if let Some(Some(tag)) = &self.loaded[index] {
            return Some(tag.clone());
        }
        let info = self.tags[index].clone();
        s.seek(info.offset as usize);
        let tag = if s.good() {
            s.read_u32().and_then(|sig_type| match sig_type {
                IC_SIG_XYZ_ARRAY_TYPE => read_xyz_array(s, info.size),
                IC_SIG_PARAMETRIC_CURVE_TYPE => read_parametric_curve(s, info.size),
                IC_SIG_CURVE_TYPE => read_curve(s, info.size),
                IC_SIG_TEXT_DESCRIPTION_TYPE => read_text_description(s, info.size),
                IC_SIG_MULTI_LOCALIZED_UNICODE_TYPE => read_multi_localized_unicode(s, info.size),
                _ => None,
            })
        } else {
            None
        };
        // Remember the tag if read ok (a failed read is retried as in
        // SampleICC).
        if tag.is_some() {
            self.loaded[index] = Some(tag.clone());
        }
        tag
    }

    /// Report critical issues.
    fn validate(&self) -> std::result::Result<(), String> {
        match self.device_class {
            IC_SIG_INPUT_CLASS
            | IC_SIG_DISPLAY_CLASS
            | IC_SIG_OUTPUT_CLASS
            | IC_SIG_LINK_CLASS
            | IC_SIG_COLOR_SPACE_CLASS
            | IC_SIG_ABSTRACT_CLASS
            | IC_SIG_NAMED_COLOR_CLASS => {}
            _ => return Err(format!("Unknown profile class: {}. ", self.device_class)),
        }
        if self.rendering_intent > 3 {
            return Err(format!(
                "Unknown rendering intent: {}. ",
                self.rendering_intent
            ));
        }
        if self.tags.is_empty() {
            return Err("No tags present. ".to_string());
        }
        Ok(())
    }
}

fn error_message(error: &str, file_name: &str) -> Error {
    Error::msg(format!("Error parsing .icc file ({file_name}).  {error}"))
}

/// Read the header, the tag table and the profile description (port of
/// `LocalFileFormat::ReadInfo`).
fn read_info(s: &mut BinaryStream, file_name: &str) -> Result<(IccContent, String)> {
    s.seek(0);

    // Read the 128 bytes header.
    let header = (|| {
        let _size = s.read_u32()?;
        let _cmm_id = s.read_u32()?;
        let _version = s.read_u32()?;
        let device_class = s.read_u32()?;
        let _color_space = s.read_u32()?;
        let _pcs = s.read_u32()?;
        for _ in 0..6 {
            s.read_u16()?; // Date.
        }
        let magic = s.read_u32()?;
        let _platform = s.read_u32()?;
        let _flags = s.read_u32()?;
        let _manufacturer = s.read_u32()?;
        let _model = s.read_u32()?;
        let _attributes = s.read_u64()?;
        let rendering_intent = s.read_u32()?;
        for _ in 0..3 {
            s.read_u32()?; // Illuminant.
        }
        let _creator = s.read_u32()?;
        s.read(16)?; // Profile id.
        s.read(28)?; // Reserved.
        Some((device_class, magic, rendering_intent))
    })();

    let Some((device_class, magic, rendering_intent)) = header else {
        return Err(error_message("Error loading header.", file_name));
    };

    if magic != IC_MAGIC_NUMBER {
        return Err(error_message("Wrong magic number.", file_name));
    }

    let Some(count) = s.read_u32() else {
        return Err(error_message("Error loading number of tags.", file_name));
    };

    if count > MAX_NUM_ICC_TAGS {
        return Err(error_message("Too many tags in ICC profile.", file_name));
    }

    // Read the tag offset table.
    let mut tags = Vec::with_capacity(count as usize);
    for _ in 0..count {
        match (s.read_u32(), s.read_u32(), s.read_u32()) {
            (Some(sig), Some(offset), Some(size)) => tags.push(TagInfo { sig, offset, size }),
            _ => {
                return Err(error_message(
                    "Error loading tag offset table from header.",
                    file_name,
                ))
            }
        }
    }

    let mut icc = IccContent {
        device_class,
        rendering_intent,
        loaded: vec![None; tags.len()],
        tags,
    };

    // Validate.
    if let Err(e) = icc.validate() {
        return Err(error_message(&e, file_name));
    }

    // Get the profile description: first try the Apple private 'dscm' tag,
    // which tends to have more accurate descriptions in Apple profiles. Fall
    // back to the standard 'desc' tag if 'dscm' is not present.
    let reader = icc
        .load_tag(s, IC_SIG_PROFILE_DESCRIPTION_ML_TAG)
        .or_else(|| icc.load_tag(s, IC_SIG_PROFILE_DESCRIPTION_TAG));

    let description = match reader {
        // The tags are missing.
        None => String::new(),
        Some(IccTag::TextDescription(t)) | Some(IccTag::MultiLocalizedUnicode(t)) => t,
        Some(_) => {
            return Err(error_message(
                "The 'desc' (or 'dcsm') reader is missing.",
                file_name,
            ))
        }
    };

    Ok((icc, description))
}

/// Validate a parametric curve: it must have the correct number of
/// arguments and be monotonically non-decreasing (flat segments allowed).
///
/// See <https://www.color.org/whitepapers/ICC_White_Paper35-Use_of_the_parametricCurveType.pdf>.
/// OCIO also logs warnings for curve discontinuities; logging is not
/// ported.
fn validate_parametric_curve(function_type: u16, params: &[i32], file_name: &str) -> Result<()> {
    let para_error = |msg: &str| -> Error {
        let args: Vec<String> = params.iter().map(|&p| format_float(ic_f_to_d(p))).collect();
        error_message(
            &format!(
                "Error parsing ICC Parametric Curve (with arguments {}): {}",
                args.join(" "),
                msg
            ),
            file_name,
        )
    };

    let quantize = |v: f32| -> f32 {
        let max_val = 2f32.powf(10.0) - 1.0;
        (v * max_val).round() / max_val
    };

    // Expected number of arguments.
    let expected = match function_type {
        0 => 1,
        1 => 3,
        2 => 4,
        3 => 5,
        4 => 7,
        _ => return Err(para_error("Unknown parametric curve type.")),
    };

    if params.len() != expected {
        return Err(para_error(&format!("Expecting {expected}param(s).")));
    }

    let p = |i: usize| ic_f_to_d(params[i]);

    // Monotonically non-decreasing (flat segments permitted).
    let g = p(0);
    // Forces the power law to be monotonically non-decreasing.
    if g <= 0.0 {
        return Err(para_error(
            "Expecting monotonically non-decreasing power-law.",
        ));
    }

    // Forces the argument to the power law to be an increasing function.
    if function_type != 0 && p(1) <= 0.0 {
        return Err(para_error(
            "Expecting strictly increasing argument to power-law.",
        ));
    }

    // Forces the linear segment to be flat or increasing.
    if (function_type == 3 || function_type == 4) && p(3) < 0.0 {
        return Err(para_error("Expecting flat or increasing linear segment."));
    }

    // Look for negative discontinuity at the linear segment / power law
    // boundary.
    if function_type == 3 {
        let (a, b, c, d) = (p(1), p(2), p(3), p(4));
        let lin_segment_break = quantize(c * d);
        let power_law_break = quantize((a * d + b).powf(g));
        if lin_segment_break > power_law_break {
            return Err(para_error(
                "Expecting no negative discontinuity at linear segment boundary.",
            ));
        }
    } else if function_type == 4 {
        let (a, b, c, d, e, f) = (p(1), p(2), p(3), p(4), p(5), p(6));
        let lin_segment_break = quantize(c * d + f);
        let power_law_break = quantize((a * d + b).powf(g) + e);
        if lin_segment_break > power_law_break {
            return Err(para_error(
                "Expecting no negative discontinuity at linear segment boundary.",
            ));
        }
    }

    // No complex / imaginary numbers.
    if (function_type == 3 || function_type == 4) && (p(1) * p(4) + p(2)) < 0.0 {
        return Err(para_error(
            "Expecting no negative arguments to the power law.",
        ));
    }

    Ok(())
}

/// Format a float as `std::ostream << float` does (6 significant digits).
fn format_float(v: f32) -> String {
    let v = v as f64;
    if v == 0.0 {
        return "0".to_string();
    }
    if !v.is_finite() {
        return if v.is_nan() {
            "nan".into()
        } else if v > 0.0 {
            "inf".into()
        } else {
            "-inf".into()
        };
    }
    let exp = v.abs().log10().floor() as i32;
    if (-5..6).contains(&exp) {
        let decimals = (5 - exp).max(0) as usize;
        let s = format!("{:.*}", decimals, v);
        let s = if s.contains('.') {
            s.trim_end_matches('0').trim_end_matches('.').to_string()
        } else {
            s
        };
        s
    } else {
        let s = format!("{:.5e}", v);
        // Rust prints e.g. "1.23450e7"; C++ prints "1.2345e+07".
        let (mant, e) = s.split_once('e').unwrap_or((&s, "0"));
        let mant = if mant.contains('.') {
            mant.trim_end_matches('0').trim_end_matches('.')
        } else {
            mant
        };
        let e: i32 = e.parse().unwrap_or(0);
        format!("{}e{}{:02}", mant, if e < 0 { '-' } else { '+' }, e.abs())
    }
}

/// Apply a parametric curve (types 1 to 4) to a single value. ICC specify
/// these functions shall clip any values outside [0.0, 1.0] range.
fn apply_parametric_curve(v: f32, function_type: u16, params: &[i32]) -> f32 {
    let v = v.max(0.0).min(1.0);
    let p = |i: usize| params.get(i).map(|&x| ic_f_to_d(x)).unwrap_or(0.0);
    let r = match function_type {
        // Type 1: y = (ax+b)^g (x >= -b/a), y = 0 (x < -b/a).
        1 => {
            let (g, a, b) = (p(0), p(1), p(2));
            if v >= -b / a {
                (a * v + b).powf(g)
            } else {
                0.0
            }
        }
        // Type 2: y = (ax+b)^g + c (x >= -b/a), y = c (x < -b/a).
        2 => {
            let (g, a, b, c) = (p(0), p(1), p(2), p(3));
            if v >= -b / a {
                (a * v + b).powf(g) + c
            } else {
                c
            }
        }
        // Type 3: y = (ax+b)^g (x >= d), y = cx (x < d).
        3 => {
            let (g, a, b, c, d) = (p(0), p(1), p(2), p(3), p(4));
            if v >= d {
                (a * v + b).powf(g)
            } else {
                c * v
            }
        }
        // Type 4: y = (ax+b)^g + e (x >= d), y = cx+f (x < d).
        4 => {
            let (g, a, b, c, d, e, f) = (p(0), p(1), p(2), p(3), p(4), p(5), p(6));
            if v >= d {
                (a * v + b).powf(g) + e
            } else {
                c * v + f
            }
        }
        _ => v,
    };
    r.max(0.0).min(1.0)
}

/// Return the profile description of an ICC profile, or the file name if
/// the description is missing or empty (port of
/// `GetProfileDescriptionFromICCProfile`).
pub fn get_profile_description_from_icc_profile(icc_profile_filepath: &str) -> Result<String> {
    let data = std::fs::read(icc_profile_filepath).map_err(|_| {
        Error::msg(format!(
            "The specified file '{icc_profile_filepath}' could not be opened. Please confirm the file exists with appropriate read permissions."
        ))
    })?;
    let mut s = BinaryStream::new(&data);
    let (_, desc) = read_info(&mut s, icc_profile_filepath)?;
    if desc.is_empty() {
        // Fallback to the filename if the description is missing or empty.
        return Ok(path_utils::basename(icc_profile_filepath));
    }
    Ok(desc)
}

struct LocalFileFormat;

pub(crate) fn create() -> Box<dyn FileFormat> {
    Box::new(LocalFileFormat)
}

impl FileFormat for LocalFileFormat {
    fn format_info(&self) -> Vec<FormatInfo> {
        vec![
            FormatInfo {
                name: "International Color Consortium profile",
                extension: "icc",
                capabilities: capability::READ,
                bake_capabilities: bake_capability::NONE,
            },
            // .icm and .pf file extensions are also fine.
            FormatInfo {
                name: "Image Color Matching profile",
                extension: "icm",
                capabilities: capability::READ,
                bake_capabilities: bake_capability::NONE,
            },
            FormatInfo {
                name: "ICC profile",
                extension: "pf",
                capabilities: capability::READ,
                bake_capabilities: bake_capability::NONE,
            },
        ]
    }

    fn is_binary(&self) -> bool {
        true
    }

    fn read(&self, data: &[u8], file_name: &str, _interp: Interpolation) -> Result<CachedFile> {
        let mut s = BinaryStream::new(data);
        // The profile description is only used by
        // `get_profile_description_from_icc_profile`.
        let (mut icc, _description) = read_info(&mut s, file_name)?;

        // Matrix part of the Matrix/TRC Model.
        let red = icc.load_tag(&mut s, IC_SIG_RED_COLORANT_TAG);
        let green = icc.load_tag(&mut s, IC_SIG_GREEN_COLORANT_TAG);
        let blue = icc.load_tag(&mut s, IC_SIG_BLUE_COLORANT_TAG);
        let (r, g, b) = match (red, green, blue) {
            (Some(IccTag::XyzArray(r)), Some(IccTag::XyzArray(g)), Some(IccTag::XyzArray(b))) => {
                (r, g, b)
            }
            _ => {
                return Err(error_message(
                    "Illegal matrix tag in ICC profile.",
                    file_name,
                ))
            }
        };
        let m = |v: i32| v as f64 / 65536.0;
        let matrix44 = [
            m(r[0]),
            m(g[0]),
            m(b[0]),
            0.0, //
            m(r[1]),
            m(g[1]),
            m(b[1]),
            0.0, //
            m(r[2]),
            m(g[2]),
            m(b[2]),
            0.0, //
            0.0,
            0.0,
            0.0,
            1.0,
        ];

        // Extract the "B" Curve part of the Matrix/TRC Model.
        let red_trc = icc.load_tag(&mut s, IC_SIG_RED_TRC_TAG);
        let green_trc = icc.load_tag(&mut s, IC_SIG_GREEN_TRC_TAG);
        let blue_trc = icc.load_tag(&mut s, IC_SIG_BLUE_TRC_TAG);
        let (Some(red_trc), Some(green_trc), Some(blue_trc)) = (red_trc, green_trc, blue_trc)
        else {
            return Err(error_message(
                "Illegal curve tag in ICC profile.",
                file_name,
            ));
        };

        const SAME_TYPE: &str = "All curves in the ICC profile must be of the same type.";

        let mut gamma_rgb = [1.0f32; 4];
        let mut lut = None;

        if red_trc.is_parametric_curve() {
            let (
                IccTag::ParametricCurve {
                    function_type: rt,
                    params: rp,
                },
                IccTag::ParametricCurve {
                    function_type: gt,
                    params: gp,
                },
                IccTag::ParametricCurve {
                    function_type: bt,
                    params: bp,
                },
            ) = (&red_trc, &green_trc, &blue_trc)
            else {
                return Err(error_message(SAME_TYPE, file_name));
            };

            // Red, green and blue curves must be of the same function type.
            if rt != gt || rt != bt {
                return Err(error_message(SAME_TYPE, file_name));
            }

            validate_parametric_curve(*rt, rp, file_name)?;
            validate_parametric_curve(*gt, gp, file_name)?;
            validate_parametric_curve(*bt, bp, file_name)?;

            if *rt == 0 {
                // Handle type 0 with a gamma.
                if rp.len() != 1 || gp.len() != 1 || bp.len() != 1 {
                    return Err(error_message(
                        "Expecting 1 param in parametric curve tag (type 0) of ICC profile.",
                        file_name,
                    ));
                }
                gamma_rgb = [ic_f_to_d(rp[0]), ic_f_to_d(gp[0]), ic_f_to_d(bp[0]), 1.0];
            } else {
                // Handle type 1-4 with a 1D LUT.
                const LUT_LENGTH: usize = 1024;
                let mut l = new_lut1d(LUT_LENGTH, false, Interpolation::Default, BitDepth::F32);
                for i in 0..LUT_LENGTH {
                    let v = i as f32 / (LUT_LENGTH as f32 - 1.0);
                    l.values[i * 3] = apply_parametric_curve(v, *rt, rp);
                    l.values[i * 3 + 1] = apply_parametric_curve(v, *gt, gp);
                    l.values[i * 3 + 2] = apply_parametric_curve(v, *bt, bp);
                }
                lut = Some(l);
            }
        } else {
            if green_trc.is_parametric_curve() || blue_trc.is_parametric_curve() {
                return Err(error_message(SAME_TYPE, file_name));
            }

            let (IccTag::Curve(rc), IccTag::Curve(gc), IccTag::Curve(bc)) =
                (&red_trc, &green_trc, &blue_trc)
            else {
                return Err(error_message(SAME_TYPE, file_name));
            };

            let curve_size = rc.len();
            if gc.len() != curve_size || bc.len() != curve_size {
                return Err(error_message(
                    "All curves in the ICC profile must be of the same length.",
                    file_name,
                ));
            }

            match curve_size {
                0 => {
                    return Err(error_message(
                        "Curves with no values in ICC profile.",
                        file_name,
                    ))
                }
                1 => {
                    // The curve value shall be interpreted as a gamma value,
                    // an unsigned fixed-point 8.8 number (multiply by 65535 to
                    // undo the normalization applied by the reader).
                    gamma_rgb = [
                        rc[0] * 65535.0f32 / 256.0f32,
                        gc[0] * 65535.0f32 / 256.0f32,
                        bc[0] * 65535.0f32 / 256.0f32,
                        1.0,
                    ];
                }
                _ => {
                    // The LUT stored in the profile takes gamma-corrected
                    // values and linearizes them. The entries are encoded as
                    // 16-bit ints that may be normalized by 65535 to
                    // interpret them as [0,1]. The LUT will be inverted to
                    // convert output-linear values into values that may be
                    // sent to the display. The file bit-depth is set based
                    // on what is in the ICC profile.
                    let mut l =
                        new_lut1d(curve_size, false, Interpolation::Default, BitDepth::UInt16);
                    for i in 0..curve_size {
                        l.values[i * 3] = rc[i];
                        l.values[i * 3 + 1] = gc[i];
                        l.values[i * 3 + 2] = bc[i];
                    }
                    lut = Some(l);
                }
            }
        }

        // The matrix/TRC transform in the ICC profile converts display device
        // code values to the CIE XYZ based version of the ICC profile
        // connection space (PCS). However, in OCIO the most common use of an
        // ICC monitor profile is as a display color space, and in that usage
        // it is more natural for the XYZ to display code value transform to
        // be called the forward direction.
        //
        // Single entry 'curv' tags and type 0 'para' tags are implemented
        // without clamping using an exponent which extends above 1 and
        // mirrors below 0.
        let mut group = GroupTransform::new();

        let mut d50_to_d65 = MatrixTransform::new(D50_TO_D65_M44, [0.0; 4]);
        d50_to_d65.direction = TransformDirection::Inverse;
        group.append(d50_to_d65);

        // The ICC profile tags form a matrix that converts RGB to CIE XYZ.
        // Invert since we are building a PCS -> device transform.
        let mut rgb_to_xyz = MatrixTransform::new(matrix44, [0.0; 4]);
        rgb_to_xyz.direction = TransformDirection::Inverse;
        group.append(rgb_to_xyz);

        // The LUT / gamma stored in the ICC profile works in the
        // gamma->linear direction.
        match lut {
            Some(mut l) => {
                l.direction = TransformDirection::Inverse;
                group.append(l);
            }
            None => {
                let mut exp = ExponentTransform::new([
                    gamma_rgb[0] as f64,
                    gamma_rgb[1] as f64,
                    gamma_rgb[2] as f64,
                    gamma_rgb[3] as f64,
                ]);
                exp.negative_style = NegativeStyle::Mirror;
                exp.direction = TransformDirection::Inverse;
                group.append(exp);
            }
        }

        Ok(CachedFile::new(group))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transforms::Transform;

    fn load(name: &str) -> Result<CachedFile> {
        let path = format!("{}/tests/data/files/{}", env!("CARGO_MANIFEST_DIR"), name);
        let data = std::fs::read(&path).expect("test file");
        LocalFileFormat.read(&data, &path, Interpolation::Default)
    }

    fn rgb_to_xyz(file: &CachedFile) -> [f64; 16] {
        let Transform::Matrix(m) = &file.group.transforms[1] else {
            panic!("expected a matrix")
        };
        assert_eq!(m.direction, TransformDirection::Inverse);
        m.matrix
    }

    #[test]
    fn format_info() {
        let info = LocalFileFormat.format_info();
        assert_eq!(info.len(), 3);
        assert_eq!(info[0].name, "International Color Consortium profile");
        assert_eq!(info[0].extension, "icc");
        assert_eq!(info[1].extension, "icm");
        assert_eq!(info[2].extension, "pf");
        assert!(LocalFileFormat.is_binary());
    }

    #[test]
    fn test_file_lut() {
        // This example uses a profile with a 1024-entry LUT for the TRC.
        let file = load("icc-test-3.icm").unwrap();
        assert_eq!(file.group.num_transforms(), 3);

        let Transform::Matrix(d50) = &file.group.transforms[0] else {
            panic!("expected a matrix")
        };
        assert_eq!(d50.matrix, D50_TO_D65_M44);
        assert_eq!(d50.direction, TransformDirection::Inverse);

        let Transform::Lut1D(lut) = &file.group.transforms[2] else {
            panic!("expected a Lut1D")
        };
        assert_eq!(lut.direction, TransformDirection::Inverse);
        assert_eq!(lut.file_output_bit_depth, BitDepth::UInt16);
        assert_eq!(lut.length(), 1024);
        assert_eq!(lut.value(200), [0.0317235067, 0.0317235067, 0.0317235067]);
    }

    #[test]
    fn test_file_gamma_curve() {
        // This test uses a profile where the TRC is a 1-entry curve, to be
        // interpreted as a gamma value.
        let file = load("icc-test-1.icc").unwrap();
        assert_eq!(file.group.num_transforms(), 3);

        let expected: [f32; 16] = [
            0.609741211,
            0.205276489,
            0.149185181,
            0.0, //
            0.311111450,
            0.625671387,
            0.0632171631,
            0.0, //
            0.0194702148,
            0.0608673096,
            0.744567871,
            0.0, //
            0.0,
            0.0,
            0.0,
            1.0,
        ];
        let m = rgb_to_xyz(&file);
        for i in 0..16 {
            assert_eq!(m[i] as f32, expected[i], "index {i}");
        }

        let Transform::Exponent(exp) = &file.group.transforms[2] else {
            panic!("expected an exponent")
        };
        assert_eq!(exp.value, [2.19921875, 2.19921875, 2.19921875, 1.0]);
        assert_eq!(exp.negative_style, NegativeStyle::Mirror);
        assert_eq!(exp.direction, TransformDirection::Inverse);
    }

    #[test]
    fn test_file_parametric_gamma() {
        // This test uses a profile where the TRC is a parametric curve of
        // type 0 (a single gamma value).
        let file = load("icc-test-2.pf").unwrap();
        let expected: [f32; 16] = [
            0.504470825,
            0.328125000,
            0.131607056,
            0.0, //
            0.264923096,
            0.682678223,
            0.0523834229,
            0.0, //
            0.0144805908,
            0.0871734619,
            0.723556519,
            0.0, //
            0.0,
            0.0,
            0.0,
            1.0,
        ];
        let m = rgb_to_xyz(&file);
        for i in 0..16 {
            assert_eq!(m[i] as f32, expected[i], "index {i}");
        }

        let Transform::Exponent(exp) = &file.group.transforms[2] else {
            panic!("expected an exponent")
        };
        let g = 2.17384338f32 as f64;
        assert_eq!(exp.value, [g, g, g, 1.0]);
    }

    #[test]
    fn test_file_parametric_curves() {
        // Profiles where the TRC is a parametric curve of type 1-4.
        for name in [
            "icc-test-pc1.icc",
            "icc-test-pc2.icc",
            "icc-test-pc3.icc",
            "icc-test-pc4.icc",
        ] {
            let file = load(name).unwrap();
            let Transform::Lut1D(lut) = &file.group.transforms[2] else {
                panic!("expected a Lut1D in {name}")
            };
            assert_eq!(lut.file_output_bit_depth, BitDepth::F32);
            assert_eq!(lut.length(), 1024);
        }

        // Type 1: g = 2.4, a = 1.1, b = -0.1.
        let file = load("icc-test-pc1.icc").unwrap();
        let Transform::Lut1D(lut) = &file.group.transforms[2] else {
            panic!()
        };
        assert_eq!(lut.values[0], 0.0);
        assert!((lut.values[3 * 1023] - 1.0).abs() < 1e-3);
    }

    #[test]
    fn apply_parametric() {
        let fixed = |v: f64| (v * 65536.0).round() as i32;
        // sRGB like type 3 curve.
        let params = [
            fixed(2.4),
            fixed(1.0 / 1.055),
            fixed(0.055 / 1.055),
            fixed(1.0 / 12.92),
            fixed(0.04045),
        ];
        assert!(validate_parametric_curve(3, &params, "f").is_ok());
        assert_eq!(apply_parametric_curve(-1.0, 3, &params), 0.0);
        assert!((apply_parametric_curve(1.0, 3, &params) - 1.0).abs() < 1e-4);
        assert!(
            (apply_parametric_curve(0.02, 3, &params) - 0.02 * ic_f_to_d(params[3])).abs() < 1e-7
        );

        let e = validate_parametric_curve(3, &params[..4], "f").unwrap_err();
        assert!(
            e.message().contains("Expecting 5param(s)."),
            "{}",
            e.message()
        );
        let e = validate_parametric_curve(7, &params, "f").unwrap_err();
        assert!(e.message().contains("Unknown parametric curve type."));
        let e = validate_parametric_curve(0, &[fixed(-1.0)], "f").unwrap_err();
        assert_eq!(
            e.message(),
            "Error parsing .icc file (f).  Error parsing ICC Parametric Curve (with arguments -1): Expecting monotonically non-decreasing power-law."
        );
        let e = validate_parametric_curve(1, &[fixed(2.0), fixed(-1.0), 0], "f").unwrap_err();
        assert!(e
            .message()
            .contains("Expecting strictly increasing argument to power-law."));
    }

    #[test]
    fn read_failures() {
        let e = LocalFileFormat
            .read(b"abc", "f.icc", Interpolation::Default)
            .unwrap_err();
        assert_eq!(
            e.message(),
            "Error parsing .icc file (f.icc).  Error loading header."
        );

        let mut data = std::fs::read(format!(
            "{}/tests/data/files/icc-test-1.icc",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        data[36] = b'x';
        let e = LocalFileFormat
            .read(&data, "f.icc", Interpolation::Default)
            .unwrap_err();
        assert_eq!(
            e.message(),
            "Error parsing .icc file (f.icc).  Wrong magic number."
        );
    }

    #[test]
    fn profile_description() {
        let path = format!(
            "{}/tests/data/files/icc-test-1.icc",
            env!("CARGO_MANIFEST_DIR")
        );
        let desc = get_profile_description_from_icc_profile(&path).unwrap();
        assert!(!desc.is_empty());
        assert!(get_profile_description_from_icc_profile("/does/not/exist.icc").is_err());
    }

    #[test]
    fn float_format() {
        assert_eq!(format_float(2.4), "2.4");
        assert_eq!(format_float(-0.1), "-0.1");
        assert_eq!(format_float(0.0), "0");
        assert_eq!(format_float(1.0 / 12.92), "0.0773994");
        assert_eq!(format_float(123456.0), "123456");
        assert_eq!(format_float(1234567.0), "1.23457e+06");
    }

    #[test]
    fn test_apply() {
        use crate::processor::Processor;
        let apply = |name: &str, dir: TransformDirection, pixels: &mut [[f32; 4]]| {
            let file = load(name).unwrap();
            let config = crate::Config::create_raw();
            let ctx = crate::Context::new();
            let p = Processor::from_transform(&config, &ctx, &Transform::Group(file.group), dir)
                .unwrap();
            p.optimized_cpu_processor(crate::OptimizationFlags::LOSSLESS)
                .apply_pixels(pixels);
        };

        // A profile where the TRC is a 1024 element LUT.
        let mut img = [
            [-0.1f32, 0.0, 0.3, 0.0],
            [0.4, 0.5, 0.6, 0.5],
            [0.7, 1.0, 1.9, 1.0],
        ];
        let dst = [
            [0.013221f32, 0.005287, 0.069636, 0.0],
            [0.188847, 0.204323, 0.330955, 0.5],
            [0.722887, 0.882591, 1.078655, 1.0],
        ];
        apply("icc-test-3.icm", TransformDirection::Inverse, &mut img);
        for (a, b) in img.iter().zip(dst.iter()) {
            for c in 0..4 {
                assert!((a[c] - b[c]).abs() <= 1e-5, "{a:?} {b:?}");
            }
        }
        apply("icc-test-3.icm", TransformDirection::Forward, &mut img);
        let bck = [
            [0.0f32, 0.0, 0.3, 0.0],
            [0.4, 0.5, 0.6, 0.5],
            [0.7, 1.0, 1.0, 1.0],
        ];
        for (a, b) in img.iter().zip(bck.iter()) {
            for c in 0..4 {
                assert!((a[c] - b[c]).abs() <= 1e-5, "{a:?} {b:?}");
            }
        }

        // A profile where the TRC is a basic gamma of {2.174, 2.174, 2.174, 1.0}.
        let src = [
            [-0.1f32, 0.0, 0.3, 0.0],
            [0.4, 0.5, 0.6, 0.5],
            [0.7, 1.0, 1.9, 1.0],
        ];
        let mut img = src;
        let dst = [
            [0.009241f32, 0.003003, 0.070198, 0.0],
            [0.188392, 0.206965, 0.343595, 0.5],
            [1.210462, 1.058761, 4.003706, 1.0],
        ];
        apply("icc-test-2.pf", TransformDirection::Inverse, &mut img);
        for (a, b) in img.iter().zip(dst.iter()) {
            for c in 0..4 {
                assert!((a[c] - b[c]).abs() <= 2e-5, "{a:?} {b:?}");
            }
        }
        apply("icc-test-2.pf", TransformDirection::Forward, &mut img);
        for (a, b) in img.iter().zip(src.iter()) {
            for c in 0..4 {
                assert!((a[c] - b[c]).abs() <= 2e-4, "{a:?} {b:?}");
            }
        }
    }
}
