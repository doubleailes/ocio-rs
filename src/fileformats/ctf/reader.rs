//! CLF / CTF reader (port of `FileFormatCTF.cpp` `XMLParserHelper` and of
//! `fileformats/ctf/CTFReaderHelper.cpp`).
//!
//! The reader is driven by the SAX callbacks of [`super::xml::parse_xml`]
//! and maintains a stack of elements, like OCIO's element classes. Each
//! element kind implements the `start` / `end` / `setRawData` behavior of
//! the corresponding OCIO reader class.

use super::opdata::*;
use super::transform::*;
use super::xml::*;
use crate::error::{Error, Result};
use crate::format_metadata::FormatMetadata;
use crate::transforms::grading::{
    GradingHueCurve, GradingPrimary, GradingRgbCurve, GradingRgbm, GradingRgbmsw, GradingTone,
};
use crate::transforms::METADATA_SAT_DESCRIPTION;
use crate::transforms::METADATA_SOP_DESCRIPTION;
use crate::transforms::{METADATA_INPUT_DESCRIPTION, METADATA_VIEWING_DESCRIPTION};
use crate::types::*;

// Element and attribute names.
pub(crate) const TAG_ACES: &str = "ACES";
pub(crate) const TAG_ACES_PARAMS: &str = "ACESParams";
pub(crate) const TAG_ARRAY: &str = "Array";
pub(crate) const TAG_CDL: &str = "ASC_CDL";
pub(crate) const TAG_CURVE_CTRL_PNTS: &str = "ControlPoints";
pub(crate) const TAG_CURVE_SLOPES: &str = "Slopes";
pub(crate) const TAG_DYN_PROP_CONTRAST: &str = "CONTRAST";
pub(crate) const TAG_DYN_PROP_EXPOSURE: &str = "EXPOSURE";
pub(crate) const TAG_DYN_PROP_GAMMA: &str = "GAMMA";
pub(crate) const TAG_DYN_PROP_PRIMARY: &str = "PRIMARY";
pub(crate) const TAG_DYN_PROP_HUECURVE: &str = "HUE_CURVE";
pub(crate) const TAG_DYN_PROP_RGBCURVE: &str = "RGB_CURVE";
pub(crate) const TAG_DYN_PROP_TONE: &str = "TONE";
pub(crate) const TAG_DYN_PROP_LOOK: &str = "LOOK_SWITCH";
pub(crate) const TAG_DYNAMIC_PARAMETER: &str = "DynamicParameter";
pub(crate) const TAG_EXPONENT: &str = "Exponent";
pub(crate) const TAG_EXPONENT_PARAMS: &str = "ExponentParams";
pub(crate) const TAG_EXPOSURE_CONTRAST: &str = "ExposureContrast";
pub(crate) const TAG_EC_PARAMS: &str = "ECParams";
pub(crate) const TAG_FIXED_FUNCTION: &str = "FixedFunction";
pub(crate) const TAG_FUNCTION: &str = "Function";
pub(crate) const TAG_GAMMA: &str = "Gamma";
pub(crate) const TAG_GAMMA_PARAMS: &str = "GammaParams";
pub(crate) const TAG_ID: &str = "Id";
pub(crate) const TAG_INDEX_MAP: &str = "IndexMap";
pub(crate) const TAG_INFO: &str = "Info";
pub(crate) const TAG_INVLUT1D: &str = "InverseLUT1D";
pub(crate) const TAG_INVLUT3D: &str = "InverseLUT3D";
pub(crate) const TAG_LOG: &str = "Log";
pub(crate) const TAG_LOG_PARAMS: &str = "LogParams";
pub(crate) const TAG_LUT1D: &str = "LUT1D";
pub(crate) const TAG_LUT3D: &str = "LUT3D";
pub(crate) const TAG_MATRIX: &str = "Matrix";
pub(crate) const TAG_MAX_IN_VALUE: &str = "maxInValue";
pub(crate) const TAG_MAX_OUT_VALUE: &str = "maxOutValue";
pub(crate) const TAG_MIN_IN_VALUE: &str = "minInValue";
pub(crate) const TAG_MIN_OUT_VALUE: &str = "minOutValue";
pub(crate) const TAG_PRIMARY: &str = "GradingPrimary";
pub(crate) const TAG_PRIMARY_BRIGHTNESS: &str = "Brightness";
pub(crate) const TAG_PRIMARY_CLAMP: &str = "Clamp";
pub(crate) const TAG_PRIMARY_CONTRAST: &str = "Contrast";
pub(crate) const TAG_PRIMARY_EXPOSURE: &str = "Exposure";
pub(crate) const TAG_PRIMARY_GAIN: &str = "Gain";
pub(crate) const TAG_PRIMARY_GAMMA: &str = "Gamma";
pub(crate) const TAG_PRIMARY_LIFT: &str = "Lift";
pub(crate) const TAG_PRIMARY_OFFSET: &str = "Offset";
pub(crate) const TAG_PRIMARY_PIVOT: &str = "Pivot";
pub(crate) const TAG_PRIMARY_SATURATION: &str = "Saturation";
pub(crate) const TAG_PROCESS_LIST: &str = "ProcessList";
pub(crate) const TAG_RANGE: &str = "Range";
pub(crate) const TAG_REFERENCE: &str = "Reference";
pub(crate) const TAG_HUE_CURVE: &str = "GradingHueCurve";
pub(crate) const TAG_HUE_CURVE_NAMES: [&str; 8] = [
    "HueHue", "HueSat", "HueLum", "LumSat", "SatSat", "LumLum", "SatLum", "HueFx",
];
pub(crate) const TAG_RGB_CURVE: &str = "GradingRGBCurve";
pub(crate) const TAG_RGB_CURVE_NAMES: [&str; 4] = ["Red", "Green", "Blue", "Master"];
pub(crate) const TAG_TONE: &str = "GradingTone";
pub(crate) const TAG_TONE_BLACKS: &str = "Blacks";
pub(crate) const TAG_TONE_HIGHLIGHTS: &str = "Highlights";
pub(crate) const TAG_TONE_MIDTONES: &str = "Midtones";
pub(crate) const TAG_TONE_SCONTRAST: &str = "SContrast";
pub(crate) const TAG_TONE_SHADOWS: &str = "Shadows";
pub(crate) const TAG_TONE_WHITES: &str = "Whites";

pub(crate) const ATTR_ALIAS: &str = "alias";
pub(crate) const ATTR_BASE: &str = "base";
pub(crate) const ATTR_BASE_PATH: &str = "basePath";
pub(crate) const ATTR_BITDEPTH_IN: &str = "inBitDepth";
pub(crate) const ATTR_BITDEPTH_OUT: &str = "outBitDepth";
pub(crate) const ATTR_BYPASS: &str = "bypass";
pub(crate) const ATTR_BYPASS_LIN_TO_LOG: &str = "bypassLinToLog";
pub(crate) const ATTR_RGB_TO_HSY: &str = "hsyTransform";
pub(crate) const ATTR_CENTER: &str = "center";
pub(crate) const ATTR_CHAN: &str = "channel";
pub(crate) const ATTR_COMP_CLF_VERSION: &str = "compCLFversion";
pub(crate) const ATTR_CONTRAST: &str = "contrast";
pub(crate) const ATTR_DIMENSION: &str = "dim";
pub(crate) const ATTR_EXPONENT: &str = "exponent";
pub(crate) const ATTR_EXPOSURE: &str = "exposure";
pub(crate) const ATTR_GAMMA: &str = "gamma";
pub(crate) const ATTR_HALF_DOMAIN: &str = "halfDomain";
pub(crate) const ATTR_HIGHLIGHT: &str = "highlight";
pub(crate) const ATTR_HUE_ADJUST: &str = "hueAdjust";
pub(crate) const ATTR_INTERPOLATION: &str = "interpolation";
pub(crate) const ATTR_IS_INVERTED: &str = "inverted";
pub(crate) const ATTR_LANGUAGE: &str = "language";
pub(crate) const ATTR_LINEARSLOPE: &str = "linearSlope";
pub(crate) const ATTR_LINSIDEBREAK: &str = "linSideBreak";
pub(crate) const ATTR_LINSIDESLOPE: &str = "linSideSlope";
pub(crate) const ATTR_LINSIDEOFFSET: &str = "linSideOffset";
pub(crate) const ATTR_LOGEXPOSURESTEP: &str = "logExposureStep";
pub(crate) const ATTR_LOGMIDGRAY: &str = "logMidGray";
pub(crate) const ATTR_LOGSIDESLOPE: &str = "logSideSlope";
pub(crate) const ATTR_LOGSIDEOFFSET: &str = "logSideOffset";
pub(crate) const ATTR_MASTER: &str = "master";
pub(crate) const ATTR_OFFSET: &str = "offset";
pub(crate) const ATTR_PARAM: &str = "param";
pub(crate) const ATTR_PARAMS: &str = "params";
pub(crate) const ATTR_PATH: &str = "path";
pub(crate) const ATTR_PIVOT: &str = "pivot";
pub(crate) const ATTR_PRIMARY_BLACK: &str = "black";
pub(crate) const ATTR_PRIMARY_CONTRAST: &str = "contrast";
pub(crate) const ATTR_PRIMARY_WHITE: &str = "white";
pub(crate) const ATTR_RAW_HALFS: &str = "rawHalfs";
pub(crate) const ATTR_REFBLACK: &str = "refBlack";
pub(crate) const ATTR_REFWHITE: &str = "refWhite";
pub(crate) const ATTR_RGB: &str = "rgb";
pub(crate) const ATTR_SHADOW: &str = "shadow";
pub(crate) const ATTR_START: &str = "start";
pub(crate) const ATTR_STYLE: &str = "style";
pub(crate) const ATTR_VERSION: &str = "version";
pub(crate) const ATTR_WIDTH: &str = "width";

/// Parse a 1D LUT interpolation attribute (port of `GetInterpolation1D`).
pub(crate) fn interpolation_1d_from_name(s: &str) -> Result<Interpolation> {
    if s.is_empty() {
        return Err(Error::msg("1D LUT missing interpolation value."));
    }
    if eq_ic(s, "linear") {
        return Ok(Interpolation::Linear);
    }
    crate::bail!("1D LUT interpolation not recongnized: '{}'.", s)
}

/// Name of a 1D LUT interpolation (`None` means "do not write").
pub(crate) fn interpolation_1d_name(i: Interpolation) -> Option<&'static str> {
    match i {
        Interpolation::Linear | Interpolation::Best => Some("linear"),
        _ => None,
    }
}

/// Parse a 3D LUT interpolation attribute (port of `GetInterpolation3D`).
pub(crate) fn interpolation_3d_from_name(s: &str) -> Result<Interpolation> {
    if s.is_empty() {
        return Err(Error::msg("3D LUT missing interpolation value."));
    }
    if eq_ic(s, "trilinear") {
        return Ok(Interpolation::Linear);
    }
    if eq_ic(s, "tetrahedral") {
        return Ok(Interpolation::Tetrahedral);
    }
    crate::bail!("3D LUT interpolation not recongnized: '{}'.", s)
}

/// Name of a 3D LUT interpolation (`None` means "do not write").
pub(crate) fn interpolation_3d_name(i: Interpolation) -> Option<&'static str> {
    match i {
        Interpolation::Linear => Some("trilinear"),
        Interpolation::Tetrahedral | Interpolation::Best => Some("tetrahedral"),
        _ => None,
    }
}

/// Validate a SMPTE ST 2136-1 id (`urn:uuid:` followed by a UUID).
pub(crate) fn validate_smpte_id(id: &str) -> bool {
    let rest = match id.strip_prefix("urn:uuid:") {
        Some(r) => r,
        None => return false,
    };
    let groups: Vec<&str> = rest.split('-').collect();
    let sizes = [8, 4, 4, 4, 12];
    groups.len() == 5
        && groups
            .iter()
            .zip(sizes.iter())
            .all(|(g, &n)| g.len() == n && g.bytes().all(|c| c.is_ascii_hexdigit()))
}

/// Parse a CLF bit-depth attribute value.
fn bit_depth_from_name(s: &str) -> BitDepth {
    match s.to_ascii_lowercase().as_str() {
        "8i" => BitDepth::UInt8,
        "10i" => BitDepth::UInt10,
        "12i" => BitDepth::UInt12,
        "16i" => BitDepth::UInt16,
        "16f" => BitDepth::F16,
        "32f" => BitDepth::F32,
        _ => BitDepth::Unknown,
    }
}

// ---------------------------------------------------------------------------
// Elements

/// Version specific variants of the gamma reader.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GammaVariant {
    /// CTF up to 1.4 (no alpha, basic and moncurve styles).
    Ctf12,
    /// CTF 1.5 to 1.8 (alpha).
    Ctf15,
    /// CTF 2.0 and later (all styles).
    Ctf20,
    /// CLF 3 (all styles, no alpha).
    Clf30,
}

/// Version specific variants of the 1D LUT reader.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lut1DVariant {
    V13,
    V14,
    V17,
}

#[derive(Debug)]
enum OpKind {
    Aces(FfData),
    Cdl(CdlData),
    FixedFunction(FfData),
    Function(FfData),
    Gamma {
        gamma: GammaData,
        variant: GammaVariant,
    },
    GradingPrimary(GradingPrimaryData),
    GradingRgbCurve(GradingRgbCurveData),
    GradingHueCurve(GradingHueCurveData),
    GradingTone(GradingToneData),
    InvLut1D {
        lut: Lut1DData,
        completed: bool,
    },
    InvLut3D {
        lut: Lut3DData,
        completed: bool,
    },
    Log {
        log: LogData,
        ctf: CtfLogParams,
        base_set: bool,
        v2: bool,
    },
    Lut1D {
        lut: Lut1DData,
        completed: bool,
        im: IndexMapping,
        completed_im: bool,
        variant: Lut1DVariant,
    },
    Lut3D {
        lut: Lut3DData,
        completed: bool,
        im: IndexMapping,
        completed_im: bool,
        v17: bool,
    },
    Matrix {
        m: MatrixData,
        array: Vec<f64>,
        length: usize,
        comps: usize,
        completed: bool,
        v13: bool,
    },
    Range {
        range: RangeData,
        v17: bool,
        no_clamp: bool,
    },
    Reference(ReferenceData),
    ExposureContrast(EcData),
}

#[derive(Debug)]
struct OpElt {
    in_bd: BitDepth,
    out_bd: BitDepth,
    kind: OpKind,
}

impl OpElt {
    fn metadata_mut(&mut self) -> &mut FormatMetadata {
        match &mut self.kind {
            OpKind::Aces(d) | OpKind::FixedFunction(d) | OpKind::Function(d) => &mut d.metadata,
            OpKind::Cdl(d) => &mut d.metadata,
            OpKind::Gamma { gamma, .. } => &mut gamma.metadata,
            OpKind::GradingPrimary(d) => &mut d.metadata,
            OpKind::GradingRgbCurve(d) => &mut d.metadata,
            OpKind::GradingHueCurve(d) => &mut d.metadata,
            OpKind::GradingTone(d) => &mut d.metadata,
            OpKind::InvLut1D { lut, .. } | OpKind::Lut1D { lut, .. } => &mut lut.metadata,
            OpKind::InvLut3D { lut, .. } | OpKind::Lut3D { lut, .. } => &mut lut.metadata,
            OpKind::Log { log, .. } => &mut log.metadata,
            OpKind::Matrix { m, .. } => &mut m.metadata,
            OpKind::Range { range, .. } => &mut range.metadata,
            OpKind::Reference(d) => &mut d.metadata,
            OpKind::ExposureContrast(d) => &mut d.metadata,
        }
    }

    /// Does the op support an `Array` element?
    fn is_array_mgt(&self) -> bool {
        matches!(
            self.kind,
            OpKind::InvLut1D { .. }
                | OpKind::InvLut3D { .. }
                | OpKind::Lut1D { .. }
                | OpKind::Lut3D { .. }
                | OpKind::Matrix { .. }
        )
    }

    fn array_completed(&self) -> bool {
        match &self.kind {
            OpKind::InvLut1D { completed, .. }
            | OpKind::InvLut3D { completed, .. }
            | OpKind::Lut1D { completed, .. }
            | OpKind::Lut3D { completed, .. }
            | OpKind::Matrix { completed, .. } => *completed,
            _ => false,
        }
    }

    /// Does the op support an `IndexMap` element?
    fn is_index_map_mgt(&self) -> bool {
        matches!(self.kind, OpKind::Lut1D { .. } | OpKind::Lut3D { .. })
    }

    fn index_map_completed(&self) -> bool {
        match &self.kind {
            OpKind::Lut1D { completed_im, .. } | OpKind::Lut3D { completed_im, .. } => {
                *completed_im
            }
            _ => false,
        }
    }

    /// Is the attribute a valid parameter of the op (port of
    /// `isOpParameterValid`)?
    fn is_op_parameter_valid(&self, att: &str, is_clf: bool) -> bool {
        let base = eq_ic(att, METADATA_ID)
            || eq_ic(att, METADATA_NAME)
            || eq_ic(att, ATTR_BITDEPTH_IN)
            || eq_ic(att, ATTR_BITDEPTH_OUT)
            || (eq_ic(att, ATTR_BYPASS) && !is_clf);
        if base {
            return true;
        }
        let is = |names: &[&str]| names.iter().any(|n| eq_ic(att, n));
        match &self.kind {
            OpKind::Aces(_)
            | OpKind::Cdl(_)
            | OpKind::Function(_)
            | OpKind::Gamma { .. }
            | OpKind::GradingPrimary(_)
            | OpKind::GradingTone(_)
            | OpKind::Log { .. }
            | OpKind::ExposureContrast(_) => is(&[ATTR_STYLE]),
            OpKind::FixedFunction(_) => is(&[ATTR_STYLE, ATTR_PARAMS]),
            OpKind::GradingRgbCurve(_) => is(&[ATTR_STYLE, ATTR_BYPASS_LIN_TO_LOG]),
            OpKind::GradingHueCurve(_) => is(&[ATTR_STYLE, ATTR_RGB_TO_HSY]),
            OpKind::InvLut1D { .. } => is(&[
                ATTR_INTERPOLATION,
                ATTR_HALF_DOMAIN,
                ATTR_RAW_HALFS,
                ATTR_HUE_ADJUST,
            ]),
            OpKind::InvLut3D { .. } | OpKind::Lut3D { .. } => is(&[ATTR_INTERPOLATION]),
            OpKind::Lut1D { variant, .. } => {
                if *variant == Lut1DVariant::V13 {
                    is(&[ATTR_INTERPOLATION, ATTR_HALF_DOMAIN, ATTR_RAW_HALFS])
                } else {
                    is(&[
                        ATTR_INTERPOLATION,
                        ATTR_HALF_DOMAIN,
                        ATTR_RAW_HALFS,
                        ATTR_HUE_ADJUST,
                    ])
                }
            }
            OpKind::Matrix { .. } => false,
            OpKind::Range { v17, .. } => *v17 && is(&[ATTR_STYLE]),
            OpKind::Reference(_) => is(&[ATTR_PATH, ATTR_BASE_PATH, ATTR_ALIAS, ATTR_IS_INVERTED]),
        }
    }
}

#[derive(Debug)]
enum Kind {
    Transform {
        is_clf: bool,
    },
    Dummy,
    Metadata(FormatMetadata),
    Info(FormatMetadata),
    Desc {
        desc: String,
        language: String,
    },
    Id(String),
    Op(Box<OpElt>),
    Array {
        position: usize,
    },
    IndexMap {
        position: usize,
    },
    AcesParams,
    EcParams,
    DynamicParam,
    GammaParams {
        allow_alpha: bool,
    },
    LogParams {
        v2: bool,
    },
    RangeValue,
    SatNode,
    SopNode {
        slope: bool,
        offset: bool,
        power: bool,
    },
    SopValue(String),
    Saturation(String),
    GradingPrimaryParam,
    GradingCurve {
        rgb: bool,
        index: usize,
    },
    CurvePoints(Vec<f32>),
    CurveSlopes(Vec<f32>),
    GradingToneParam,
}

#[derive(Debug)]
struct Elt {
    name: String,
    line: u32,
    kind: Kind,
}

impl Elt {
    fn is_container(&self) -> bool {
        matches!(
            self.kind,
            Kind::Transform { .. }
                | Kind::Metadata(_)
                | Kind::Info(_)
                | Kind::Op(_)
                | Kind::SatNode
                | Kind::SopNode { .. }
                | Kind::GradingCurve { .. }
        )
    }

    fn is_metadata(&self) -> bool {
        matches!(self.kind, Kind::Metadata(_) | Kind::Info(_))
    }

    fn op(&self) -> Option<&OpElt> {
        match &self.kind {
            Kind::Op(o) => Some(o),
            _ => None,
        }
    }

    /// `XmlReaderElement::throwMessage`.
    fn err(&self, msg: &str) -> Error {
        Error::msg(format!("At line {}: {}", self.line, msg))
    }
}

/// Parse the (single) number of an attribute (port of
/// `XmlReaderElement::parseScalarAttribute`).
fn parse_scalar_attribute(elt_line: u32, name: &str, value: &str) -> Result<f64> {
    let data: Vec<f64> = get_numbers(value).map_err(|e| {
        Error::msg(format!(
            "At line {}: For parameter: '{}'. {}",
            elt_line, name, e
        ))
    })?;
    if data.len() != 1 {
        crate::bail!(
            "At line {}: For parameter: '{}'. Expecting 1 value, found {} values.",
            elt_line,
            name,
            data.len()
        );
    }
    Ok(data[0])
}

// Index map parsing helpers.

fn find_index_delim(s: &[u8], pos: usize) -> usize {
    let mut pos = pos;
    while pos < s.len() && !is_space(s[pos]) && s[pos] != b'@' {
        pos += 1;
    }
    pos.min(s.len())
}

fn find_next_token_start_index_map(s: &[u8], pos: usize) -> usize {
    let mut pos = pos;
    while pos < s.len() && (is_number_delimiter(s[pos]) || s[pos] == b'@') {
        pos += 1;
    }
    pos.min(s.len())
}

/// Extract the next pair of index map numbers (port of `GetNextIndexPair`).
fn get_next_index_pair(s: &[u8], pos: &mut usize) -> Result<Option<(f32, f32)>> {
    *pos = find_next_token_start(s, *pos);
    if *pos == s.len() {
        return Ok(None);
    }
    let end = find_index_delim(s, *pos);
    if end == s.len() {
        crate::bail!(
            "GetNextIndexPair: First number of a pair is the end of the string '{}'.",
            truncate_string(&String::from_utf8_lossy(s))
        );
    }
    let n1: f32 = parse_number(s, *pos, end)?;
    *pos = find_next_token_start_index_map(s, end);
    let end = find_delim(s, *pos);
    let n2: f32 = parse_number(s, *pos, end)?;
    *pos = end;
    if *pos != s.len() {
        *pos = find_next_token_start(s, *pos);
    }
    Ok(Some((n1, n2)))
}

// ---------------------------------------------------------------------------
// Reader

const V1_2: CtfVersion = CTF_PROCESS_LIST_VERSION_1_2;
const V1_3: CtfVersion = CTF_PROCESS_LIST_VERSION_1_3;
const V1_4: CtfVersion = CTF_PROCESS_LIST_VERSION_1_4;
const V1_5: CtfVersion = CTF_PROCESS_LIST_VERSION_1_5;
const V1_6: CtfVersion = CTF_PROCESS_LIST_VERSION_1_6;
const V1_8: CtfVersion = CTF_PROCESS_LIST_VERSION_1_8;
const V2_0: CtfVersion = CTF_PROCESS_LIST_VERSION_2_0;
const V2_5: CtfVersion = CTF_PROCESS_LIST_VERSION_2_5;

/// Op element types (port of `CTFReaderOpElt::Type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OpType {
    Aces,
    Cdl,
    ExposureContrast,
    FixedFunction,
    Function,
    Gamma,
    GradingPrimary,
    GradingRgbCurve,
    GradingHueCurve,
    GradingTone,
    InvLut1D,
    InvLut3D,
    Log,
    Lut1D,
    Lut3D,
    Matrix,
    Range,
    Reference,
}

/// Create the reader of an op for the version (port of
/// `CTFReaderOpElt::GetReader`).
fn get_reader(t: OpType, version: &CtfVersion, is_clf: bool) -> Option<OpKind> {
    let v = version;
    let cur = &CTF_PROCESS_LIST_VERSION;
    let ff = FfData::default;
    match t {
        OpType::Aces => (v >= &V1_5 && v <= cur).then(|| OpKind::Aces(ff())),
        OpType::Cdl => (v >= &V1_3 && v <= cur).then(|| {
            OpKind::Cdl(CdlData {
                style: CdlOpStyle::NoClampFwd,
                ..Default::default()
            })
        }),
        OpType::ExposureContrast => {
            (!is_clf && v <= cur).then(|| OpKind::ExposureContrast(EcData::default()))
        }
        OpType::FixedFunction => {
            (!is_clf && v >= &V2_0 && v <= cur).then(|| OpKind::FixedFunction(ff()))
        }
        OpType::Function => (!is_clf && v >= &V1_6 && v <= cur).then(|| OpKind::Function(ff())),
        OpType::Gamma => {
            let variant = if !is_clf {
                if v <= &V1_4 {
                    Some(GammaVariant::Ctf12)
                } else if v <= &V1_8 {
                    Some(GammaVariant::Ctf15)
                } else if v >= &V2_0 && v <= cur {
                    Some(GammaVariant::Ctf20)
                } else {
                    None
                }
            } else if v >= &V2_0 && v <= cur {
                Some(GammaVariant::Clf30)
            } else {
                None
            };
            variant.map(|variant| OpKind::Gamma {
                gamma: GammaData::default(),
                variant,
            })
        }
        OpType::GradingHueCurve => (!is_clf && v >= &V2_5 && v <= cur).then(|| {
            OpKind::GradingHueCurve(GradingHueCurveData {
                style: GradingStyle::Log,
                dir: TransformDirection::Forward,
                value: GradingHueCurve::new(GradingStyle::Log),
                rgb_to_hsy: HsyTransformStyle::Hsy1,
                dynamic: false,
                metadata: FormatMetadata::default(),
            })
        }),
        OpType::GradingPrimary => (!is_clf && v >= &V2_0 && v <= cur).then(|| {
            OpKind::GradingPrimary(GradingPrimaryData {
                style: GradingStyle::Log,
                dir: TransformDirection::Forward,
                value: GradingPrimary::new(GradingStyle::Log),
                dynamic: false,
                metadata: FormatMetadata::default(),
            })
        }),
        OpType::GradingRgbCurve => (!is_clf && v >= &V2_0 && v <= cur).then(|| {
            OpKind::GradingRgbCurve(GradingRgbCurveData {
                style: GradingStyle::Log,
                dir: TransformDirection::Forward,
                value: GradingRgbCurve::new(GradingStyle::Log),
                bypass_lin_to_log: false,
                dynamic: false,
                metadata: FormatMetadata::default(),
            })
        }),
        OpType::GradingTone => (!is_clf && v >= &V2_0 && v <= cur).then(|| {
            OpKind::GradingTone(GradingToneData {
                style: GradingStyle::Log,
                dir: TransformDirection::Forward,
                value: GradingTone::new(GradingStyle::Log),
                dynamic: false,
                metadata: FormatMetadata::default(),
            })
        }),
        OpType::InvLut1D => (!is_clf && v >= &V1_3 && v <= cur).then(|| OpKind::InvLut1D {
            lut: Lut1DData::new(2, TransformDirection::Inverse),
            completed: false,
        }),
        OpType::InvLut3D => (!is_clf && v >= &V1_6 && v <= cur).then(|| OpKind::InvLut3D {
            lut: Lut3DData::new(2, TransformDirection::Inverse),
            completed: false,
        }),
        OpType::Log => {
            let log = || LogData::new(2.0, TransformDirection::Forward);
            if !is_clf && v >= &V1_3 && v <= &V1_8 {
                Some(OpKind::Log {
                    log: log(),
                    ctf: CtfLogParams::default(),
                    base_set: false,
                    v2: false,
                })
            } else if v >= &V2_0 && v <= cur {
                Some(OpKind::Log {
                    log: log(),
                    ctf: CtfLogParams::default(),
                    base_set: false,
                    v2: true,
                })
            } else {
                None
            }
        }
        OpType::Lut1D => {
            let variant = if v <= &V1_3 {
                Lut1DVariant::V13
            } else if v <= &V1_4 {
                Lut1DVariant::V14
            } else if v <= cur {
                Lut1DVariant::V17
            } else {
                return None;
            };
            Some(OpKind::Lut1D {
                lut: Lut1DData::new(2, TransformDirection::Forward),
                completed: false,
                im: IndexMapping::new(0),
                completed_im: false,
                variant,
            })
        }
        OpType::Lut3D => (v <= cur).then(|| OpKind::Lut3D {
            lut: Lut3DData::new(2, TransformDirection::Forward),
            completed: false,
            im: IndexMapping::new(0),
            completed_im: false,
            v17: v > &V1_6,
        }),
        OpType::Matrix => (v <= cur).then(|| OpKind::Matrix {
            m: MatrixData::default(),
            array: IDENTITY_MATRIX44_VEC.to_vec(),
            length: 4,
            comps: 4,
            completed: false,
            v13: v > &V1_2,
        }),
        OpType::Range => (v <= cur).then(|| OpKind::Range {
            range: RangeData::default(),
            v17: v > &V1_6,
            no_clamp: false,
        }),
        OpType::Reference => (v <= cur).then(|| OpKind::Reference(ReferenceData::default())),
    }
}

const IDENTITY_MATRIX44_VEC: [f64; 16] = crate::transforms::IDENTITY_MATRIX44;

/// Result of the parsing of a CLF/CTF file.
#[derive(Debug, Clone)]
pub(crate) struct CtfParseResult {
    /// The file content.
    pub transform: CtfReaderTransform,
    /// The warnings OCIO would log while reading (there is no logging
    /// facility, they are kept for inspection).
    #[cfg_attr(not(test), allow(dead_code))]
    pub warnings: Vec<String>,
}

struct Reader {
    file_name: String,
    is_clf_ext: bool,
    line: u32,
    keep_namespaces: i32,
    transform: Option<CtfReaderTransform>,
    warnings: Vec<String>,
    elms: Vec<Elt>,
}

impl Reader {
    fn xml_file(&self) -> &str {
        if self.file_name.is_empty() {
            "File name not specified"
        } else {
            &self.file_name
        }
    }

    /// `XMLParserHelper::throwMessage`.
    fn throw_message(&self, error: &str) -> Error {
        Error::msg(format!(
            "Error parsing CTF/CLF file ({}). Error is: {}. At line ({})",
            self.file_name, error, self.line
        ))
    }

    fn warn(&mut self, msg: String) {
        self.warnings.push(msg);
    }

    /// `XmlReaderElement::logParameterWarning`.
    fn log_parameter_warning(&mut self, elt_name: &str, elt_line: u32, param: &str) {
        let msg = format!(
            "{}({}): Unrecognized attribute '{}' of '{}'.",
            self.xml_file(),
            elt_line,
            param,
            elt_name
        );
        self.warn(msg);
    }

    /// Create a dummy element (and log the matching warning).
    fn dummy(&mut self, name: &str, parent: Option<(String, u32)>, msg: Option<&str>) -> Elt {
        let (pname, pline) = parent.unwrap_or((String::new(), 0));
        let mut m = format!(
            "{}({}): Unrecognized element '{}' where its parent is '{}' ({})",
            self.xml_file(),
            self.line,
            name,
            pname,
            pline
        );
        if let Some(msg) = msg {
            m.push_str(": ");
            m.push_str(msg);
        }
        m.push('.');
        self.warn(m);
        Elt {
            name: name.to_string(),
            line: self.line,
            kind: Kind::Dummy,
        }
    }

    fn transform_mut(&mut self) -> Result<&mut CtfReaderTransform> {
        match self.transform.as_mut() {
            Some(t) => Ok(t),
            None => Err(Error::msg("ProcessList tag missing.")),
        }
    }

    fn transform_ref(&self) -> Result<&CtfReaderTransform> {
        match self.transform.as_ref() {
            Some(t) => Ok(t),
            None => Err(Error::msg("ProcessList tag missing.")),
        }
    }

    // -----------------------------------------------------------------------
    // Start

    fn handle_start(
        &mut self,
        elms: &mut Vec<Elt>,
        name_full: &str,
        atts: &[(String, String)],
    ) -> Result<()> {
        if name_full.is_empty() {
            return Err(self.throw_message("Internal CTF/CLF parser error. "));
        }
        let name: &str = if self.keep_namespaces <= 0 {
            match name_full.rfind(':') {
                Some(p) => &name_full[p + 1..],
                None => name_full,
            }
        } else {
            name_full
        };

        // Check if we are still processing a metadata structure.
        if let Some(back) = elms.last() {
            if back.is_metadata() {
                elms.push(Elt {
                    name: name.to_string(),
                    line: self.line,
                    kind: Kind::Metadata(FormatMetadata::new(name, "")),
                });
                return self.start_top(elms, atts);
            }
        }

        if eq_ic(name, TAG_PROCESS_LIST) {
            if self.transform.is_some() {
                let parent = elms.first().map(|e| (e.name.clone(), e.line));
                let d = self.dummy(name, parent, Some("The Transform already exists"));
                elms.push(d);
            } else {
                elms.push(Elt {
                    name: name.to_string(),
                    line: self.line,
                    kind: Kind::Transform {
                        is_clf: self.is_clf_ext,
                    },
                });
                self.transform = Some(CtfReaderTransform::default());
            }
            return self.start_top(elms, atts);
        }

        let parent: Option<&Elt> = elms.last();
        let mut recognized = false;

        let supported = |tag: &str, parent_name: &str, recognized: &mut bool| -> bool {
            if !name.is_empty() && eq_ic(name, tag) {
                *recognized = true;
                if parent_name.is_empty() || parent.map_or(false, |p| eq_ic(&p.name, parent_name)) {
                    return true;
                }
            }
            false
        };

        const OP_TAGS: [(&str, OpType); 19] = [
            (TAG_ACES, OpType::Aces),
            (TAG_CDL, OpType::Cdl),
            (TAG_EXPOSURE_CONTRAST, OpType::ExposureContrast),
            (TAG_FIXED_FUNCTION, OpType::FixedFunction),
            (TAG_FUNCTION, OpType::Function),
            (TAG_GAMMA, OpType::Gamma),
            (TAG_EXPONENT, OpType::Gamma),
            (TAG_PRIMARY, OpType::GradingPrimary),
            (TAG_RGB_CURVE, OpType::GradingRgbCurve),
            (TAG_HUE_CURVE, OpType::GradingHueCurve),
            (TAG_TONE, OpType::GradingTone),
            (TAG_INVLUT1D, OpType::InvLut1D),
            (TAG_INVLUT3D, OpType::InvLut3D),
            (TAG_LOG, OpType::Log),
            (TAG_LUT1D, OpType::Lut1D),
            (TAG_LUT3D, OpType::Lut3D),
            (TAG_MATRIX, OpType::Matrix),
            (TAG_RANGE, OpType::Range),
            (TAG_REFERENCE, OpType::Reference),
        ];
        let mut op_type = None;
        for (tag, t) in OP_TAGS {
            if supported(tag, TAG_PROCESS_LIST, &mut recognized) {
                op_type = Some(t);
                break;
            }
        }
        if let Some(t) = op_type {
            self.add_op_reader(elms, t, name)?;
            return self.start_top(elms, atts);
        }

        // Other elements (transform-level metadata or parts of ops).
        let parent_is_container = parent.map_or(false, |p| p.is_container());
        let back_info = parent.map(|p| (p.name.clone(), p.line));

        let new_kind: std::result::Result<Kind, Option<String>>;
        if !parent_is_container {
            new_kind = Err(None);
        } else if supported(TAG_ACES_PARAMS, TAG_ACES, &mut recognized) {
            new_kind = Ok(Kind::AcesParams);
        } else if supported(TAG_ARRAY, TAG_LUT1D, &mut recognized)
            || supported(TAG_ARRAY, TAG_INVLUT1D, &mut recognized)
            || supported(TAG_ARRAY, TAG_LUT3D, &mut recognized)
            || supported(TAG_ARRAY, TAG_INVLUT3D, &mut recognized)
            || supported(TAG_ARRAY, TAG_MATRIX, &mut recognized)
        {
            let op = parent.and_then(|p| p.op()).filter(|o| o.is_array_mgt());
            new_kind = match op {
                None => Err(Some("Array not allowed in this element".to_string())),
                Some(o) if o.array_completed() => {
                    Err(Some("Only one Array allowed per op".to_string()))
                }
                Some(_) => Ok(Kind::Array { position: 0 }),
            };
        } else if supported(TAG_DESCRIPTION, "", &mut recognized)
            || supported(METADATA_INPUT_DESCRIPTION, TAG_CDL, &mut recognized)
            || supported(METADATA_VIEWING_DESCRIPTION, TAG_CDL, &mut recognized)
        {
            new_kind = Ok(Kind::Desc {
                desc: String::new(),
                language: String::new(),
            });
        } else if supported(TAG_ID, "", &mut recognized) {
            new_kind = Ok(Kind::Id(String::new()));
        } else if supported(TAG_DYNAMIC_PARAMETER, "", &mut recognized)
            && parent.and_then(|p| p.op()).is_some()
        {
            new_kind = Ok(Kind::DynamicParam);
        } else if supported(TAG_EC_PARAMS, TAG_EXPOSURE_CONTRAST, &mut recognized) {
            new_kind = Ok(Kind::EcParams);
        } else if supported(TAG_GAMMA_PARAMS, TAG_GAMMA, &mut recognized)
            || supported(TAG_EXPONENT_PARAMS, TAG_EXPONENT, &mut recognized)
        {
            let allow_alpha = match parent.and_then(|p| p.op()).map(|o| &o.kind) {
                Some(OpKind::Gamma { variant, .. }) => {
                    matches!(variant, GammaVariant::Ctf15 | GammaVariant::Ctf20)
                }
                _ => false,
            };
            new_kind = Ok(Kind::GammaParams { allow_alpha });
        } else if supported(TAG_INDEX_MAP, TAG_LUT1D, &mut recognized)
            || supported(TAG_INDEX_MAP, TAG_LUT3D, &mut recognized)
        {
            let op = parent.and_then(|p| p.op()).filter(|o| o.is_index_map_mgt());
            match op {
                None => new_kind = Err(Some("IndexMap not allowed in this element".to_string())),
                Some(o) if o.index_map_completed() => {
                    return Err(self.throw_message("Only one IndexMap allowed per LUT. "));
                }
                Some(_) => new_kind = Ok(Kind::IndexMap { position: 0 }),
            }
        } else if supported(TAG_INFO, TAG_PROCESS_LIST, &mut recognized) {
            self.keep_namespaces += 1;
            new_kind = Ok(Kind::Info(FormatMetadata::new(name, "")));
        } else if supported(METADATA_INPUT_DESCRIPTOR, TAG_PROCESS_LIST, &mut recognized) {
            new_kind = Ok(Kind::Desc {
                desc: String::new(),
                language: String::new(),
            });
        } else if supported(TAG_LOG_PARAMS, TAG_LOG, &mut recognized) {
            match parent.and_then(|p| p.op()).map(|o| &o.kind) {
                Some(OpKind::Log { ctf, v2, .. }) => {
                    let s = ctf.style;
                    if !matches!(
                        s,
                        LogStyle::LogToLin
                            | LogStyle::LinToLog
                            | LogStyle::CameraLogToLin
                            | LogStyle::CameraLinToLog
                    ) {
                        new_kind = Err(Some("Log Params not allowed in this element".to_string()));
                    } else {
                        new_kind = Ok(Kind::LogParams { v2: *v2 });
                    }
                }
                _ => new_kind = Err(Some("Log Params not allowed in this element".to_string())),
            }
        } else if supported(
            METADATA_OUTPUT_DESCRIPTOR,
            TAG_PROCESS_LIST,
            &mut recognized,
        ) {
            new_kind = Ok(Kind::Desc {
                desc: String::new(),
                language: String::new(),
            });
        } else if [
            TAG_MIN_IN_VALUE,
            TAG_MAX_IN_VALUE,
            TAG_MIN_OUT_VALUE,
            TAG_MAX_OUT_VALUE,
        ]
        .iter()
        .any(|t| supported(t, TAG_RANGE, &mut recognized))
        {
            new_kind = Ok(Kind::RangeValue);
        } else if supported(TAG_SATNODE, TAG_CDL, &mut recognized)
            || supported(TAG_SATNODEALT, TAG_CDL, &mut recognized)
        {
            new_kind = Ok(Kind::SatNode);
        } else if supported(TAG_SATURATION, TAG_SATNODE, &mut recognized) {
            new_kind = Ok(Kind::Saturation(String::new()));
        } else if supported(TAG_SOPNODE, TAG_CDL, &mut recognized) {
            new_kind = Ok(Kind::SopNode {
                slope: false,
                offset: false,
                power: false,
            });
        } else if [TAG_SLOPE, TAG_OFFSET, TAG_POWER]
            .iter()
            .any(|t| supported(t, TAG_SOPNODE, &mut recognized))
        {
            new_kind = Ok(Kind::SopValue(String::new()));
        } else if [
            TAG_PRIMARY_BRIGHTNESS,
            TAG_PRIMARY_CLAMP,
            TAG_PRIMARY_CONTRAST,
            TAG_PRIMARY_EXPOSURE,
            TAG_PRIMARY_GAIN,
            TAG_PRIMARY_GAMMA,
            TAG_PRIMARY_LIFT,
            TAG_PRIMARY_OFFSET,
            TAG_PRIMARY_PIVOT,
            TAG_PRIMARY_SATURATION,
        ]
        .iter()
        .any(|t| supported(t, TAG_PRIMARY, &mut recognized))
        {
            new_kind = Ok(Kind::GradingPrimaryParam);
        } else if TAG_RGB_CURVE_NAMES
            .iter()
            .any(|t| supported(t, TAG_RGB_CURVE, &mut recognized))
            || TAG_HUE_CURVE_NAMES
                .iter()
                .any(|t| supported(t, TAG_HUE_CURVE, &mut recognized))
        {
            new_kind = Ok(Kind::GradingCurve {
                rgb: true,
                index: 0,
            });
        } else if {
            let under_curve = |recognized: &mut bool| {
                if eq_ic(name, TAG_CURVE_CTRL_PNTS) {
                    *recognized = true;
                    parent.map_or(false, |p| {
                        TAG_RGB_CURVE_NAMES
                            .iter()
                            .chain(TAG_HUE_CURVE_NAMES.iter())
                            .any(|n| eq_ic(&p.name, n))
                    })
                } else {
                    false
                }
            };
            under_curve(&mut recognized)
        } {
            new_kind = Ok(Kind::CurvePoints(Vec::new()));
        } else if {
            let under_curve = |recognized: &mut bool| {
                if eq_ic(name, TAG_CURVE_SLOPES) {
                    *recognized = true;
                    parent.map_or(false, |p| {
                        TAG_RGB_CURVE_NAMES
                            .iter()
                            .chain(TAG_HUE_CURVE_NAMES.iter())
                            .any(|n| eq_ic(&p.name, n))
                    })
                } else {
                    false
                }
            };
            under_curve(&mut recognized)
        } {
            new_kind = Ok(Kind::CurveSlopes(Vec::new()));
        } else if [
            TAG_TONE_BLACKS,
            TAG_TONE_SHADOWS,
            TAG_TONE_MIDTONES,
            TAG_TONE_HIGHLIGHTS,
            TAG_TONE_WHITES,
            TAG_TONE_SCONTRAST,
        ]
        .iter()
        .any(|t| supported(t, TAG_TONE, &mut recognized))
        {
            new_kind = Ok(Kind::GradingToneParam);
        } else if recognized {
            new_kind = Err(Some(format!("'{name}' not allowed in this element")));
        } else {
            new_kind = Err(Some("Unknown element".to_string()));
        }

        let elt = match new_kind {
            Ok(kind) => Elt {
                name: name.to_string(),
                line: self.line,
                kind,
            },
            Err(msg) => self.dummy(name, back_info, msg.as_deref()),
        };
        elms.push(elt);
        self.start_top(elms, atts)
    }

    /// Port of `XMLParserHelper::AddOpReader`.
    fn add_op_reader(&mut self, elms: &mut Vec<Elt>, t: OpType, tag: &str) -> Result<()> {
        if elms.len() != 1 {
            let msg = format!("The {tag}'s parent can only be a Transform");
            let parent = elms.last().map(|e| (e.name.clone(), e.line));
            let d = self.dummy(tag, parent, Some(&msg));
            elms.push(d);
            return Ok(());
        }
        let tr = self.transform_ref()?;
        let is_clf = tr.is_clf();
        match get_reader(t, &tr.version, is_clf) {
            None => {
                let msg = if is_clf {
                    format!(
                        "CLF file version '{}' does not support operator '{}'",
                        tr.clf_version, tag
                    )
                } else {
                    format!(
                        "CTF file version '{}' does not support operator '{}'",
                        tr.version, tag
                    )
                };
                Err(self.throw_message(&msg))
            }
            Some(kind) => {
                elms.push(Elt {
                    name: tag.to_string(),
                    line: self.line,
                    kind: Kind::Op(Box::new(OpElt {
                        in_bd: BitDepth::Unknown,
                        out_bd: BitDepth::Unknown,
                        kind,
                    })),
                });
                Ok(())
            }
        }
    }

    /// Call the `start` method of the element on top of the stack.
    fn start_top(&mut self, elms: &mut [Elt], atts: &[(String, String)]) -> Result<()> {
        let n = elms.len();
        let (before, top) = elms.split_at_mut(n - 1);
        let top = &mut top[0];
        let parent = before.last_mut();
        let (name, line) = (top.name.clone(), top.line);
        match &mut top.kind {
            Kind::Transform { is_clf } => {
                let mut clf = *is_clf;
                self.transform_start(line, atts, &mut clf)?;
                *is_clf = clf;
            }
            Kind::Dummy => {}
            Kind::Metadata(md) => {
                for (n, v) in atts {
                    if !v.is_empty() {
                        md.add_attribute(n, v);
                    }
                }
            }
            Kind::Info(md) => {
                if let Some((n, v)) = atts.first() {
                    validate_info_element_version(n, v)?;
                }
                for (n, v) in atts {
                    if !v.is_empty() {
                        md.add_attribute(n, v);
                    }
                }
            }
            Kind::Desc { desc, language } => {
                desc.clear();
                language.clear();
                // Note: OCIO compares the first attribute name for each
                // attribute, so the value of the last attribute is kept.
                if let Some((first, _)) = atts.first() {
                    if eq_ic(ATTR_LANGUAGE, first) {
                        for (_, v) in atts {
                            if v.is_empty() {
                                return Err(top.err("Attribute 'language' does not have a value."));
                            }
                            *language = v.clone();
                        }
                    }
                }
            }
            Kind::Id(id) => id.clear(),
            Kind::Op(op) => {
                self.op_start(&name, line, op, atts)?;
            }
            Kind::Array { position } => {
                *position = 0;
                let parent = parent.ok_or_else(|| Error::msg("Array without parent"))?;
                self.array_start(&name, line, parent, atts)?;
            }
            Kind::IndexMap { position } => {
                *position = 0;
                let parent = parent.ok_or_else(|| Error::msg("IndexMap without parent"))?;
                self.index_map_start(&name, line, parent, atts)?;
            }
            Kind::AcesParams => {
                let parent = parent.ok_or_else(|| Error::msg("ACESParams without parent"))?;
                self.aces_params_start(&name, line, parent, atts)?;
            }
            Kind::EcParams => {
                let parent = parent.ok_or_else(|| Error::msg("ECParams without parent"))?;
                self.ec_params_start(&name, line, parent, atts)?;
            }
            Kind::DynamicParam => {
                let parent = parent.ok_or_else(|| Error::msg("DynamicParameter without parent"))?;
                self.dynamic_param_start(line, parent, atts)?;
            }
            Kind::GammaParams { allow_alpha } => {
                let allow_alpha = *allow_alpha;
                let parent = parent.ok_or_else(|| Error::msg("GammaParams without parent"))?;
                self.gamma_params_start(&name, line, parent, atts, allow_alpha)?;
            }
            Kind::LogParams { v2 } => {
                let v2 = *v2;
                let parent = parent.ok_or_else(|| Error::msg("LogParams without parent"))?;
                if v2 {
                    self.log_params_v2_start(&name, line, parent, atts)?;
                } else {
                    self.log_params_start(&name, line, parent, atts)?;
                }
            }
            Kind::RangeValue => {
                for (n, _) in atts {
                    self.log_parameter_warning(&name, line, n);
                }
            }
            Kind::SatNode => {}
            Kind::SopNode {
                slope,
                offset,
                power,
            } => {
                *slope = false;
                *offset = false;
                *power = false;
            }
            Kind::SopValue(s) | Kind::Saturation(s) => s.clear(),
            Kind::GradingPrimaryParam => {
                let parent = parent.ok_or_else(|| Error::msg("Grading element without parent"))?;
                grading_primary_param_start(&name, line, parent, atts)?;
            }
            Kind::GradingCurve { rgb, index } => {
                let r = curve_type_from_name(&name)
                    .map_err(|e| Error::msg(format!("At line {line}: {e}")))?;
                *rgb = r.0;
                *index = r.1;
            }
            Kind::CurvePoints(_) | Kind::CurveSlopes(_) => {}
            Kind::GradingToneParam => {
                let parent = parent.ok_or_else(|| Error::msg("Grading element without parent"))?;
                grading_tone_param_start(&name, line, parent, atts)?;
            }
        }
        Ok(())
    }

    /// Port of `CTFReaderTransformElt::start`.
    fn transform_start(
        &mut self,
        line: u32,
        atts: &[(String, String)],
        is_clf: &mut bool,
    ) -> Result<()> {
        let err = |m: &str| Error::msg(format!("At line {line}: {m}"));
        let mut is_id_found = false;
        let mut is_version_found = false;
        let mut is_clf_version_found = false;
        let mut is_smpte_version_found = false;
        let mut requested_version = CtfVersion::new(0, 0, 0);
        let mut requested_clf_version = CtfVersion::new(0, 0, 0);

        for (n, v) in atts {
            if eq_ic(ATTR_ID, n) {
                if v.is_empty() {
                    return Err(err("Attribute 'id' does not have a value."));
                }
                self.transform_mut()?.metadata.add_attribute(ATTR_ID, v);
                is_id_found = true;
            } else if eq_ic(ATTR_XMLNS, n) {
                if v.is_empty() {
                    return Err(err("Attribute 'xmlns' does not have a value."));
                }
                if let Ok(version) = CtfVersion::parse(v, version_format::SMPTE_XMLNS) {
                    requested_version = CTF_PROCESS_LIST_VERSION_2_0;
                    requested_clf_version = version;
                    is_smpte_version_found = true;
                    *is_clf = true;
                }
                if is_version_found && is_smpte_version_found {
                    return Err(err(
                        "SMPTE 'xmlns' version and 'Version' attribute cannot both be present.",
                    ));
                }
            } else if eq_ic(ATTR_NAME, n) {
                if v.is_empty() {
                    return Err(err(
                        "If the attribute 'name' is present, it must have a value.",
                    ));
                }
                self.transform_mut()?.metadata.add_attribute(ATTR_NAME, v);
            } else if eq_ic(ATTR_INVERSE_OF, n) {
                if v.is_empty() {
                    return Err(err(
                        "If the attribute 'inverseOf' is present, it must have a value.",
                    ));
                }
                self.transform_mut()?
                    .metadata
                    .add_attribute(ATTR_INVERSE_OF, v);
            } else if eq_ic(ATTR_VERSION, n) {
                if is_clf_version_found {
                    return Err(err(
                        "'compCLFversion' and 'Version' cannot both be present.",
                    ));
                }
                if is_smpte_version_found {
                    return Err(err(
                        "SMPTE 'xmlns' version and 'Version' attribute cannot both be present.",
                    ));
                }
                if is_version_found {
                    return Err(err("'Version' can only be there once."));
                }
                if v.is_empty() {
                    return Err(err(
                        "If the attribute 'version' is present, it must have a value.",
                    ));
                }
                requested_version = CtfVersion::parse_numeric(v).map_err(|e| err(e.message()))?;
                is_version_found = true;
            } else if eq_ic(ATTR_COMP_CLF_VERSION, n) {
                if is_clf_version_found {
                    return Err(err("'compCLFversion' can only be there once."));
                }
                if is_version_found {
                    return Err(err(
                        "'compCLFversion' and 'Version' cannot be both present.",
                    ));
                }
                if v.is_empty() {
                    return Err(err(
                        "Required attribute 'compCLFversion' does not have a value.",
                    ));
                }
                requested_clf_version = CtfVersion::parse(v, version_format::SMPTE_CLF)
                    .map_err(|e| err(e.message()))?;
                let max_clf = CtfVersion::new(3, 0, 0);
                if max_clf < requested_clf_version {
                    return Err(err(&format!(
                        "Unsupported transform file version '{v}' supplied."
                    )));
                }
                requested_version = if requested_clf_version <= CtfVersion::new(2, 0, 0) {
                    CTF_PROCESS_LIST_VERSION_1_7
                } else {
                    CTF_PROCESS_LIST_VERSION_2_0
                };
                is_clf_version_found = true;
                *is_clf = true;
            } else if n.starts_with("xmlns:") {
                if v.is_empty() {
                    return Err(err(
                        "If the attribute 'xmlns:*' is present, it must have a value.",
                    ));
                }
                self.transform_mut()?.metadata.add_attribute(n, v);
            } else {
                self.log_parameter_warning(TAG_PROCESS_LIST, line, n);
            }
        }

        if !is_id_found && !is_smpte_version_found {
            return Err(err("Required attribute 'id' is missing."));
        }

        let (version, clf_version) = if !(is_version_found
            || is_clf_version_found
            || is_smpte_version_found)
        {
            if *is_clf {
                return Err(err(
                    "No valid 'version', 'compCLFversion', or 'xmlns' attributes were found; at least one of them is required.",
                ));
            }
            (CTF_PROCESS_LIST_VERSION_1_2, None)
        } else {
            (requested_version, Some(requested_clf_version))
        };
        if CTF_PROCESS_LIST_VERSION < version {
            return Err(err(&format!(
                "Unsupported transform file version '{version}' supplied."
            )));
        }
        let t = self.transform_mut()?;
        t.version = version;
        if let Some(c) = clf_version {
            t.clf_version = c;
        }
        Ok(())
    }

    /// Port of `CTFReaderOpElt::start` and of the op specific `start`.
    fn op_start(
        &mut self,
        name: &str,
        line: u32,
        op: &mut OpElt,
        atts: &[(String, String)],
    ) -> Result<()> {
        let err = |m: &str| Error::msg(format!("At line {line}: {m}"));

        // Common attributes.
        let mut in_found = false;
        let mut out_found = false;
        for (n, v) in atts {
            if eq_ic(ATTR_ID, n) {
                op.metadata_mut().add_attribute(METADATA_ID, v);
            } else if eq_ic(ATTR_NAME, n) {
                op.metadata_mut().add_attribute(METADATA_NAME, v);
            } else if eq_ic(ATTR_BITDEPTH_IN, n) {
                let bd = bit_depth_from_name(v);
                if bd == BitDepth::Unknown {
                    return Err(err(&format!("inBitDepth unknown value ({v}).")));
                }
                op.in_bd = bd;
                in_found = true;
            } else if eq_ic(ATTR_BITDEPTH_OUT, n) {
                let bd = bit_depth_from_name(v);
                if bd == BitDepth::Unknown {
                    return Err(err(&format!("outBitDepth unknown value ({v}).")));
                }
                op.out_bd = bd;
                out_found = true;
            }
        }
        if !in_found {
            return Err(err("inBitDepth is missing."));
        } else if !out_found {
            return Err(err("outBitDepth is missing."));
        }

        let t = self.transform_mut()?;
        let prev = t.prev_out_bd;
        t.prev_out_bd = op.out_bd;
        let is_clf = t.is_clf();
        let (version, clf_version) = (t.version.clone(), t.clf_version.clone());
        if prev != BitDepth::Unknown && prev != op.in_bd {
            return Err(err(&format!(
                "Bit-depth mismatch between ops. Previous op output bit-depth is: '{}' and this op input bit-depth is '{}'. ",
                prev.as_str(),
                op.in_bd.as_str()
            )));
        }

        for (n, _) in atts {
            if !op.is_op_parameter_valid(n, is_clf) {
                self.log_parameter_warning(name, line, n);
            }
        }

        let find = |a: &'static str| {
            atts.iter()
                .filter(move |(n, _)| eq_ic(n, a))
                .map(|(_, v)| v.as_str())
        };

        match &mut op.kind {
            OpKind::Aces(ff) => {
                let mut found = false;
                for v in find(ATTR_STYLE) {
                    let (s, d) = ff_style_from_name(v).map_err(|e| err(e.message()))?;
                    ff.style = s;
                    ff.dir = d;
                    found = true;
                }
                if !found {
                    return Err(err("style parameter for FixedFunction is missing."));
                }
            }
            OpKind::Cdl(cdl) => {
                let mut found = false;
                for v in find(ATTR_STYLE) {
                    cdl.style = CdlOpStyle::from_name(v)?;
                    found = true;
                }
                if !found {
                    cdl.style = CdlOpStyle::V12Fwd;
                }
            }
            OpKind::FixedFunction(ff) => {
                let mut found = false;
                for (n, v) in atts {
                    if eq_ic(ATTR_STYLE, n) {
                        let (s, d) = ff_style_from_name(v).map_err(|e| err(e.message()))?;
                        ff.style = s;
                        ff.dir = d;
                        found = true;
                    } else if eq_ic(ATTR_PARAMS, n) {
                        let data: Vec<f64> = get_numbers(v).map_err(|_| {
                            err(&format!(
                                "Illegal '{}' params {}.",
                                name,
                                truncate_string(v)
                            ))
                        })?;
                        ff.params = data;
                    }
                }
                if !found {
                    return Err(err("Style parameter for FixedFunction is missing."));
                }
            }
            OpKind::Function(ff) => {
                let mut found = false;
                for v in find(ATTR_STYLE) {
                    let (s, d) = ff_style_from_name(v).map_err(|e| err(e.message()))?;
                    ff.style = s;
                    ff.dir = d;
                    found = true;
                }
                if !found {
                    return Err(err("Style parameter for FixedFunction is missing."));
                }
            }
            OpKind::Gamma { gamma, variant } => {
                let mut found = false;
                for v in find(ATTR_STYLE) {
                    let style = GammaStyle::from_name(v)?;
                    let valid = match variant {
                        GammaVariant::Ctf12 | GammaVariant::Ctf15 => matches!(
                            style,
                            GammaStyle::BasicFwd
                                | GammaStyle::BasicRev
                                | GammaStyle::MonCurveFwd
                                | GammaStyle::MonCurveRev
                        ),
                        GammaVariant::Ctf20 | GammaVariant::Clf30 => true,
                    };
                    if !valid {
                        let msg = if is_clf {
                            format!(
                                "Style not handled: '{v}' for CLF file version '{clf_version}'."
                            )
                        } else {
                            format!("Style not handled: '{v}' for CTF file version '{version}'.")
                        };
                        return Err(err(&msg));
                    }
                    gamma.style = style;
                    found = true;
                    gamma.set_params(style.identity_params());
                }
                if !found {
                    return Err(err("Missing parameter 'style'."));
                }
            }
            OpKind::GradingPrimary(gp) => {
                let mut found = false;
                for v in find(ATTR_STYLE) {
                    let (s, d) = grading_style_from_name(v).map_err(|_| {
                        err(&format!("Required attribute 'style' '{v}' is invalid."))
                    })?;
                    gp.style = s;
                    gp.dir = d;
                    gp.value = GradingPrimary::new(s);
                    found = true;
                }
                if !found {
                    return Err(err("Required attribute 'style' is missing."));
                }
            }
            OpKind::GradingRgbCurve(c) => {
                let mut found = false;
                for (n, v) in atts {
                    if eq_ic(ATTR_STYLE, n) {
                        let (s, d) = grading_style_from_name(v).map_err(|_| {
                            err(&format!("Required attribute 'style' '{v}' is invalid."))
                        })?;
                        c.style = s;
                        c.dir = d;
                        c.value = GradingRgbCurve::new(s);
                        found = true;
                    } else if eq_ic(ATTR_BYPASS_LIN_TO_LOG, n) {
                        if !eq_ic("true", v) {
                            return Err(err(&format!(
                                "Unknown bypassLinToLog value: '{v}' while parsing RGBCurve."
                            )));
                        }
                        c.bypass_lin_to_log = true;
                    }
                }
                if !found {
                    return Err(err("Required attribute 'style' is missing."));
                }
            }
            OpKind::GradingHueCurve(c) => {
                let mut found = false;
                for (n, v) in atts {
                    if eq_ic(ATTR_STYLE, n) {
                        let (s, d) = grading_style_from_name(v).map_err(|_| {
                            err(&format!("Required attribute 'style' '{v}' is invalid."))
                        })?;
                        c.style = s;
                        c.dir = d;
                        c.value = GradingHueCurve::new(s);
                        found = true;
                    } else if eq_ic(ATTR_RGB_TO_HSY, n) {
                        if !eq_ic("none", v) {
                            return Err(err(&format!(
                                "Unknown hsyTransform value: '{v}' while parsing HueCurve."
                            )));
                        }
                        c.rgb_to_hsy = HsyTransformStyle::None;
                    }
                }
                if !found {
                    return Err(err("Required attribute 'style' is missing."));
                }
            }
            OpKind::GradingTone(t) => {
                let mut found = false;
                for v in find(ATTR_STYLE) {
                    let (s, d) = grading_style_from_name(v).map_err(|_| {
                        err(&format!("Required attribute 'style' '{v}' is invalid."))
                    })?;
                    t.style = s;
                    t.dir = d;
                    t.value = GradingTone::new(s);
                    found = true;
                }
                if !found {
                    return Err(err("Required attribute 'style' is missing."));
                }
            }
            OpKind::InvLut1D { lut, .. } => {
                lut.interpolation = Interpolation::Default;
                for (n, v) in atts {
                    if eq_ic(ATTR_INTERPOLATION, n) {
                        lut.interpolation =
                            interpolation_1d_from_name(v).map_err(|e| err(e.message()))?;
                    }
                    if eq_ic(ATTR_HALF_DOMAIN, n) {
                        if !eq_ic("true", v) {
                            return Err(err(&format!(
                                "Unknown halfDomain value: '{v}' while parsing InvLut1D."
                            )));
                        }
                        lut.half_domain = true;
                    }
                    if eq_ic(ATTR_RAW_HALFS, n) {
                        if !eq_ic("true", v) {
                            return Err(err(&format!(
                                "Unknown rawHalfs value: '{v}' while parsing InvLut1D."
                            )));
                        }
                        lut.raw_halfs = true;
                    }
                    if eq_ic(ATTR_HUE_ADJUST, n) {
                        if !eq_ic("dw3", v) {
                            return Err(err(&format!(
                                "Unknown hueAdjust value: '{v}' while parsing InvLut1D."
                            )));
                        }
                        lut.hue_adjust = Lut1DHueAdjust::Dw3;
                    }
                }
            }
            OpKind::InvLut3D { lut, .. } | OpKind::Lut3D { lut, .. } => {
                lut.interpolation = Interpolation::Default;
                for v in find(ATTR_INTERPOLATION) {
                    lut.interpolation =
                        interpolation_3d_from_name(v).map_err(|e| err(e.message()))?;
                }
            }
            OpKind::Log { ctf, .. } => {
                let mut found = false;
                for v in find(ATTR_STYLE) {
                    ctf.style = LogStyle::from_name(v).map_err(|_| {
                        err(&format!("Required attribute 'style' '{v}' is invalid."))
                    })?;
                    found = true;
                }
                if !found {
                    return Err(err(
                        "CTF/CLF Log parsing. Required attribute 'style' is missing.",
                    ));
                }
            }
            OpKind::Lut1D { lut, variant, .. } => {
                lut.interpolation = Interpolation::Default;
                for (n, v) in atts {
                    if eq_ic(ATTR_INTERPOLATION, n) {
                        lut.interpolation =
                            interpolation_1d_from_name(v).map_err(|e| err(e.message()))?;
                    }
                    if eq_ic(ATTR_HALF_DOMAIN, n) {
                        if !eq_ic("true", v) {
                            return Err(err(&format!(
                                "Illegal 'halfDomain' attribute '{v}' while parsing Lut1D."
                            )));
                        }
                        lut.half_domain = true;
                    }
                    if eq_ic(ATTR_RAW_HALFS, n) {
                        if !eq_ic("true", v) {
                            return Err(err(&format!(
                                "Illegal 'rawHalfs' attribute '{v}' while parsing Lut1D."
                            )));
                        }
                        lut.raw_halfs = true;
                    }
                    if *variant != Lut1DVariant::V13 && eq_ic(ATTR_HUE_ADJUST, n) {
                        if !eq_ic("dw3", v) {
                            return Err(err(&format!(
                                "Illegal 'hueAdjust' attribute '{v}' while parsing Lut1D."
                            )));
                        }
                        lut.hue_adjust = Lut1DHueAdjust::Dw3;
                    }
                }
            }
            OpKind::Matrix { .. } => {}
            OpKind::Range { v17, no_clamp, .. } => {
                if *v17 {
                    *no_clamp = false;
                    for v in find(ATTR_STYLE) {
                        *no_clamp = eq_ic("noClamp", v);
                    }
                }
            }
            OpKind::Reference(r) => {
                let mut alias = String::new();
                let mut path = String::new();
                let mut base_path_found = false;
                for (n, v) in atts {
                    if eq_ic(ATTR_PATH, n) {
                        path = v.clone();
                    } else if eq_ic(ATTR_BASE_PATH, n) {
                        base_path_found = true;
                    } else if eq_ic(ATTR_ALIAS, n) {
                        alias = v.clone();
                        if eq_ic(&alias, "currentMonitor") {
                            return Err(err("The 'currentMonitor' alias is not supported."));
                        }
                    } else if eq_ic(ATTR_IS_INVERTED, n) && eq_ic("true", v) {
                        r.dir = TransformDirection::Inverse;
                    }
                }
                if !alias.is_empty() {
                    if !path.is_empty() {
                        return Err(err(
                            "alias & path attributes for Reference should not be both defined.",
                        ));
                    }
                    if base_path_found {
                        return Err(err(
                            "alias & basepath attributes for Reference should not be both defined.",
                        ));
                    }
                    r.alias = alias;
                } else {
                    if path.is_empty() {
                        return Err(err("path attribute for Reference is missing."));
                    }
                    r.path = path;
                }
            }
            OpKind::ExposureContrast(ec) => {
                let mut found = false;
                for v in find(ATTR_STYLE) {
                    ec.style = EcOpStyle::from_name(v)
                        .map_err(|e| err(&format!("ExposureContrast element: {}", e)))?;
                    found = true;
                }
                if !found {
                    return Err(err("ExposureContrast element: style missing."));
                }
            }
        }
        Ok(())
    }

    /// Port of `CTFReaderArrayElt::start`.
    fn array_start(
        &mut self,
        name: &str,
        line: u32,
        parent: &mut Elt,
        atts: &[(String, String)],
    ) -> Result<()> {
        let type_name = parent.name.clone();
        let err = |m: &str| Error::msg(format!("At line {line}: {m}"));
        let mut dim_found = false;
        for (n, v) in atts {
            if n.is_empty() {
                break;
            }
            if eq_ic(ATTR_DIMENSION, n) {
                dim_found = true;
                let dims: Vec<u32> = get_numbers(v).map_err(|_| {
                    err(&format!(
                        "Illegal '{}' array dimensions {}.",
                        type_name,
                        truncate_string(v)
                    ))
                })?;
                let op = match &mut parent.kind {
                    Kind::Op(op) => op,
                    _ => {
                        return Err(err(&format!(
                            "Parsing issue while parsing array dimensions of '{}' ({}).",
                            type_name,
                            truncate_string(v)
                        )))
                    }
                };
                if dims.len() <= 1 {
                    return Err(err(&format!(
                        "Illegal '{}' array dimensions {}.",
                        type_name,
                        truncate_string(v)
                    )));
                }
                if !update_dimension(op, &dims) {
                    return Err(err(&format!(
                        "'{}' Illegal array dimensions {}.",
                        type_name,
                        truncate_string(v)
                    )));
                }
            } else {
                self.log_parameter_warning(name, line, n);
            }
        }
        if !dim_found {
            return Err(err("Missing 'dim' attribute."));
        }
        Ok(())
    }

    /// Port of `CTFReaderIndexMapElt::start`.
    fn index_map_start(
        &mut self,
        name: &str,
        line: u32,
        parent: &mut Elt,
        atts: &[(String, String)],
    ) -> Result<()> {
        let type_name = parent.name.clone();
        let err = |m: &str| Error::msg(format!("At line {line}: {m}"));
        let mut dim_found = false;
        for (n, v) in atts {
            if n.is_empty() {
                break;
            }
            if eq_ic(ATTR_DIMENSION, n) {
                dim_found = true;
                let bad = || {
                    err(&format!(
                        "Illegal '{}' IndexMap dimensions {}.",
                        type_name,
                        truncate_string(v)
                    ))
                };
                let dims: Vec<u32> = get_numbers(v).map_err(|_| bad())?;
                if dims.len() != 1 {
                    return Err(bad());
                }
                let d = dims[0] as usize;
                match &mut parent.kind {
                    Kind::Op(op) => match &mut op.kind {
                        OpKind::Lut1D { im, .. } => {
                            if !(2..=MAX_1D_LUT_LENGTH).contains(&d) {
                                return Err(bad());
                            }
                            im.resize(d);
                        }
                        OpKind::Lut3D { im, .. } => {
                            if !(2..=MAX_3D_LUT_LENGTH).contains(&d) {
                                return Err(bad());
                            }
                            im.resize(d);
                        }
                        _ => return Err(bad()),
                    },
                    _ => return Err(bad()),
                }
            } else {
                self.log_parameter_warning(name, line, n);
            }
        }
        if !dim_found {
            return Err(err("Required attribute 'dim' is missing."));
        }
        Ok(())
    }

    /// Port of `CTFReaderACESParamsElt::start`.
    fn aces_params_start(
        &mut self,
        name: &str,
        line: u32,
        parent: &mut Elt,
        atts: &[(String, String)],
    ) -> Result<()> {
        let err = |m: &str| Error::msg(format!("At line {line}: {m}"));
        let mut gamma = f64::NAN;
        for (n, v) in atts {
            if eq_ic(ATTR_GAMMA, n) {
                gamma = parse_scalar_attribute(line, n, v)?;
            } else {
                self.log_parameter_warning(name, line, n);
            }
        }
        let ff = match &mut parent.kind {
            Kind::Op(op) => match &mut op.kind {
                OpKind::Aces(ff) => ff,
                _ => return Err(err("Invalid ACES parameters.")),
            },
            _ => return Err(err("Invalid ACES parameters.")),
        };
        let style_name = ff_style_to_name(ff.style, ff.dir, false)?;
        if ff.style == FixedFunctionStyle::Rec2100Surround {
            if !ff.params.is_empty() {
                return Err(err(&format!(
                    "ACES FixedFunction element with style {style_name} expects only 1 gamma parameter."
                )));
            }
            if gamma.is_nan() {
                return Err(err(&format!(
                    "Missing required parameter {ATTR_GAMMA}for ACES FixedFunction element with style {style_name}."
                )));
            }
            ff.params = vec![gamma];
        } else {
            return Err(err(&format!(
                "ACES FixedFunction element with style {style_name} does not take any parameter."
            )));
        }
        Ok(())
    }

    /// Port of `CTFReaderECParamsElt::start`.
    fn ec_params_start(
        &mut self,
        name: &str,
        line: u32,
        parent: &mut Elt,
        atts: &[(String, String)],
    ) -> Result<()> {
        let err = |m: &str| Error::msg(format!("At line {line}: {m}"));
        let mut exposure = f64::NAN;
        let mut contrast = f64::NAN;
        let mut gamma = f64::NAN;
        let mut pivot = f64::NAN;
        let mut log_exposure_step = f64::NAN;
        let mut log_mid_gray = f64::NAN;
        for (n, v) in atts {
            if n.is_empty() {
                break;
            }
            if eq_ic(ATTR_EXPOSURE, n) {
                exposure = parse_scalar_attribute(line, n, v)?;
            } else if eq_ic(ATTR_CONTRAST, n) {
                contrast = parse_scalar_attribute(line, n, v)?;
            } else if eq_ic(ATTR_GAMMA, n) {
                gamma = parse_scalar_attribute(line, n, v)?;
            } else if eq_ic(ATTR_PIVOT, n) {
                pivot = parse_scalar_attribute(line, n, v)?;
            } else if eq_ic(ATTR_LOGEXPOSURESTEP, n) {
                log_exposure_step = parse_scalar_attribute(line, n, v)?;
            } else if eq_ic(ATTR_LOGMIDGRAY, n) {
                log_mid_gray = parse_scalar_attribute(line, n, v)?;
            } else {
                self.log_parameter_warning(name, line, n);
            }
        }
        if exposure.is_nan() {
            return Err(err("ExposureContrast element: exposure missing."));
        }
        if contrast.is_nan() {
            return Err(err("ExposureContrast element: contrast missing."));
        }
        if pivot.is_nan() {
            return Err(err("ExposureContrast element: pivot missing."));
        }
        let ec = match &mut parent.kind {
            Kind::Op(op) => match &mut op.kind {
                OpKind::ExposureContrast(ec) => ec,
                _ => return Err(err("Invalid ExposureContrast parameters.")),
            },
            _ => return Err(err("Invalid ExposureContrast parameters.")),
        };
        ec.exposure = exposure;
        ec.contrast = contrast;
        if !gamma.is_nan() {
            ec.gamma = gamma;
        }
        ec.pivot = pivot;
        if !log_exposure_step.is_nan() {
            ec.log_exposure_step = log_exposure_step;
        }
        if !log_mid_gray.is_nan() {
            ec.log_mid_gray = log_mid_gray;
        }
        Ok(())
    }

    /// Port of `CTFReaderDynamicParamElt::start`.
    fn dynamic_param_start(
        &mut self,
        line: u32,
        parent: &mut Elt,
        atts: &[(String, String)],
    ) -> Result<()> {
        let err = |m: &str| Error::msg(format!("At line {line}: {m}"));
        let container = parent.name.clone();
        for (n, v) in atts {
            if !eq_ic(ATTR_PARAM, n) {
                continue;
            }
            let not_supported = || {
                err(&format!(
                    "Dynamic parameter '{v}' is not supported in '{container}'."
                ))
            };
            let op_kind = match &mut parent.kind {
                Kind::Op(op) => Some(&mut op.kind),
                _ => None,
            };
            if eq_ic(TAG_DYN_PROP_EXPOSURE, v)
                || eq_ic(TAG_DYN_PROP_CONTRAST, v)
                || eq_ic(TAG_DYN_PROP_GAMMA, v)
            {
                match op_kind {
                    Some(OpKind::ExposureContrast(ec)) => {
                        if eq_ic(TAG_DYN_PROP_EXPOSURE, v) {
                            ec.exposure_dynamic = true;
                        } else if eq_ic(TAG_DYN_PROP_CONTRAST, v) {
                            ec.contrast_dynamic = true;
                        } else {
                            ec.gamma_dynamic = true;
                        }
                    }
                    _ => return Err(not_supported()),
                }
            } else if eq_ic(TAG_DYN_PROP_PRIMARY, v) {
                match op_kind {
                    Some(OpKind::GradingPrimary(gp)) => gp.dynamic = true,
                    _ => return Err(not_supported()),
                }
            } else if eq_ic(TAG_DYN_PROP_RGBCURVE, v) {
                match op_kind {
                    Some(OpKind::GradingRgbCurve(c)) => c.dynamic = true,
                    _ => return Err(not_supported()),
                }
            } else if eq_ic(TAG_DYN_PROP_HUECURVE, v) {
                match op_kind {
                    Some(OpKind::GradingHueCurve(c)) => c.dynamic = true,
                    _ => return Err(not_supported()),
                }
            } else if eq_ic(TAG_DYN_PROP_TONE, v) {
                match op_kind {
                    Some(OpKind::GradingTone(t)) => t.dynamic = true,
                    _ => return Err(not_supported()),
                }
            } else if eq_ic(TAG_DYN_PROP_LOOK, v) {
                let msg = format!(
                    "{}({}): Dynamic parameter '{}' on '{}' is ignored.",
                    self.xml_file(),
                    line,
                    v,
                    container
                );
                self.warn(msg);
            } else {
                return Err(err(&format!(
                    "Dynamic parameter '{v}' is not valid in '{container}'."
                )));
            }
        }
        Ok(())
    }

    /// Port of `CTFReaderGammaParamsElt::start` (and of the 1.5 variant).
    fn gamma_params_start(
        &mut self,
        name: &str,
        line: u32,
        parent: &mut Elt,
        atts: &[(String, String)],
        allow_alpha: bool,
    ) -> Result<()> {
        let err = |m: &str| Error::msg(format!("At line {line}: {m}"));
        let mut chan: i32 = -1;
        let mut gamma = f64::NAN;
        let mut offset = f64::NAN;
        for (n, v) in atts {
            if eq_ic(ATTR_CHAN, n) {
                chan = if allow_alpha && eq_ic("A", v) {
                    3
                } else if eq_ic("R", v) {
                    0
                } else if eq_ic("G", v) {
                    1
                } else if eq_ic("B", v) {
                    2
                } else {
                    -1
                };
                if chan == -1 {
                    return Err(err(&format!("Invalid channel: {v}.")));
                }
            } else if eq_ic(ATTR_GAMMA, n) || eq_ic(ATTR_EXPONENT, n) {
                gamma = parse_scalar_attribute(line, n, v)?;
            } else if eq_ic(ATTR_OFFSET, n) {
                offset = parse_scalar_attribute(line, n, v)?;
            } else {
                self.log_parameter_warning(name, line, n);
            }
        }
        let g = match &mut parent.kind {
            Kind::Op(op) => match &mut op.kind {
                OpKind::Gamma { gamma, .. } => gamma,
                _ => return Err(err("Invalid gamma parameters.")),
            },
            _ => return Err(err("Invalid gamma parameters.")),
        };
        let style = g.style;
        let mut params = Vec::new();
        if !style.is_moncurve() {
            if gamma.is_nan() {
                return Err(err(&format!(
                    "Missing required gamma parameter for style: {}.",
                    style.name()
                )));
            }
            params.push(gamma);
            if !offset.is_nan() {
                return Err(err(&format!(
                    "Illegal offset parameter for style: {}.",
                    style.name()
                )));
            }
        } else {
            if gamma.is_nan() {
                return Err(err(&format!(
                    "Missing required gamma parameter for style: {}.",
                    style.name()
                )));
            }
            params.push(gamma);
            if offset.is_nan() {
                return Err(err(&format!(
                    "Missing required offset parameter for style: {}.",
                    style.name()
                )));
            }
            params.push(offset);
        }
        match chan {
            -1 => g.set_params(params),
            c => g.params[c as usize] = params,
        }
        Ok(())
    }

    fn parse_cineon(line: u32, n: &str, v: &str, vals: &mut [f64; 5]) -> Result<bool> {
        let idx = if eq_ic(ATTR_GAMMA, n) {
            0
        } else if eq_ic(ATTR_REFWHITE, n) {
            1
        } else if eq_ic(ATTR_REFBLACK, n) {
            2
        } else if eq_ic(ATTR_HIGHLIGHT, n) {
            3
        } else if eq_ic(ATTR_SHADOW, n) {
            4
        } else {
            return Ok(false);
        };
        vals[idx] = parse_scalar_attribute(line, n, v)?;
        Ok(true)
    }

    fn set_cineon(line: u32, ctf: &mut CtfLogParams, chan: i32, vals: &[f64; 5]) -> Result<()> {
        let names = [
            ATTR_GAMMA,
            ATTR_REFWHITE,
            ATTR_REFBLACK,
            ATTR_HIGHLIGHT,
            ATTR_SHADOW,
        ];
        for (i, n) in names.iter().enumerate() {
            if vals[i].is_nan() {
                crate::bail!("At line {}: Required attribute '{}' is missing.", line, n);
            }
        }
        match chan {
            -1 => {
                ctf.params = [*vals, *vals, *vals];
            }
            c => ctf.params[c as usize] = *vals,
        }
        Ok(())
    }

    fn log_channel(line: u32, v: &str) -> Result<i32> {
        if eq_ic("R", v) {
            Ok(0)
        } else if eq_ic("G", v) {
            Ok(1)
        } else if eq_ic("B", v) {
            Ok(2)
        } else {
            crate::bail!("At line {}: Illegal channel attribute value '{}'.", line, v)
        }
    }

    /// Port of `CTFReaderLogParamsElt::start`.
    fn log_params_start(
        &mut self,
        name: &str,
        line: u32,
        parent: &mut Elt,
        atts: &[(String, String)],
    ) -> Result<()> {
        let mut chan = -1;
        let mut vals = [f64::NAN; 5];
        for (n, v) in atts {
            if eq_ic(ATTR_CHAN, n) {
                chan = Self::log_channel(line, v)?;
            } else if !Self::parse_cineon(line, n, v, &mut vals)? {
                self.log_parameter_warning(name, line, n);
            }
        }
        match &mut parent.kind {
            Kind::Op(op) => match &mut op.kind {
                OpKind::Log { ctf, .. } => Self::set_cineon(line, ctf, chan, &vals),
                _ => Err(Error::msg("Invalid log parameters.")),
            },
            _ => Err(Error::msg("Invalid log parameters.")),
        }
    }

    /// Port of `CTFReaderLogParamsElt_2_0::start`.
    fn log_params_v2_start(
        &mut self,
        name: &str,
        line: u32,
        parent: &mut Elt,
        atts: &[(String, String)],
    ) -> Result<()> {
        let err = |m: &str| Error::msg(format!("At line {line}: {m}"));
        let is_clf = self.transform_ref()?.is_clf();
        let (log, ctf, base_set) = match &mut parent.kind {
            Kind::Op(op) => match &mut op.kind {
                OpKind::Log {
                    log, ctf, base_set, ..
                } => (log, ctf, base_set),
                _ => return Err(err("Invalid log parameters.")),
            },
            _ => return Err(err("Invalid log parameters.")),
        };
        let style = ctf.style;
        let camera_style = matches!(style, LogStyle::CameraLinToLog | LogStyle::CameraLogToLin);
        let allow_cineon = !camera_style && !is_clf;

        let mut chan = -1;
        let mut lin_side_slope = f64::NAN;
        let mut lin_side_offset = f64::NAN;
        let mut log_side_slope = f64::NAN;
        let mut log_side_offset = f64::NAN;
        let mut base = f64::NAN;
        let mut lin_side_break = f64::NAN;
        let mut linear_slope = f64::NAN;
        let mut cineon = [f64::NAN; 5];
        let mut warnings = Vec::new();

        for (n, v) in atts {
            let mut valid_type = true;
            if eq_ic(ATTR_CHAN, n) {
                chan = Self::log_channel(line, v)?;
            } else if eq_ic(ATTR_LINSIDESLOPE, n) {
                lin_side_slope = parse_scalar_attribute(line, n, v)?;
                valid_type = ctf.set_type(CtfLogParamsType::Clf);
            } else if eq_ic(ATTR_LINSIDEOFFSET, n) {
                lin_side_offset = parse_scalar_attribute(line, n, v)?;
                valid_type = ctf.set_type(CtfLogParamsType::Clf);
            } else if eq_ic(ATTR_LOGSIDESLOPE, n) {
                log_side_slope = parse_scalar_attribute(line, n, v)?;
                valid_type = ctf.set_type(CtfLogParamsType::Clf);
            } else if eq_ic(ATTR_LOGSIDEOFFSET, n) {
                log_side_offset = parse_scalar_attribute(line, n, v)?;
                valid_type = ctf.set_type(CtfLogParamsType::Clf);
            } else if eq_ic(ATTR_BASE, n) {
                base = parse_scalar_attribute(line, n, v)?;
                valid_type = ctf.set_type(CtfLogParamsType::Clf);
            } else if eq_ic(ATTR_LINEARSLOPE, n) {
                linear_slope = parse_scalar_attribute(line, n, v)?;
                valid_type = ctf.set_type(CtfLogParamsType::Clf);
            } else if eq_ic(ATTR_LINSIDEBREAK, n) {
                lin_side_break = parse_scalar_attribute(line, n, v)?;
                valid_type = ctf.set_type(CtfLogParamsType::Clf);
            } else if allow_cineon && Self::parse_cineon(line, n, v, &mut cineon)? {
                valid_type = ctf.set_type(CtfLogParamsType::Cineon);
            } else {
                warnings.push(n.clone());
            }
            if !valid_type {
                return Err(err(
                    "CLF type and Cineon types parameters can not be mixed.",
                ));
            }
        }
        for w in warnings {
            self.log_parameter_warning(name, line, &w);
        }

        if ctf.get_type() == CtfLogParamsType::Cineon {
            return Self::set_cineon(line, ctf, chan, &cineon);
        }

        let mut new_params = vec![0.0; 4];
        new_params[LIN_SIDE_SLOPE] = if lin_side_slope.is_nan() {
            1.0
        } else {
            lin_side_slope
        };
        new_params[LIN_SIDE_OFFSET] = if lin_side_offset.is_nan() {
            0.0
        } else {
            lin_side_offset
        };
        new_params[LOG_SIDE_SLOPE] = if log_side_slope.is_nan() {
            1.0
        } else {
            log_side_slope
        };
        new_params[LOG_SIDE_OFFSET] = if log_side_offset.is_nan() {
            0.0
        } else {
            log_side_offset
        };

        if !base.is_nan() {
            set_log_base(line, log, base_set, base)?;
        }

        let cam_styles = format!(
            "'{}' or '{}'",
            LogStyle::CameraLogToLin.name(),
            LogStyle::CameraLinToLog.name()
        );
        if !lin_side_break.is_nan() {
            if !camera_style {
                return Err(err(&format!(
                    "Parameter '{ATTR_LINSIDEBREAK}' is only allowed for style {cam_styles}."
                )));
            }
            new_params.push(lin_side_break);
        } else if camera_style {
            return Err(err(&format!(
                "Parameter '{ATTR_LINSIDEBREAK}' should be defined for style {cam_styles}. "
            )));
        }

        if !linear_slope.is_nan() {
            if !camera_style {
                return Err(err(&format!(
                    "Parameter '{ATTR_LINEARSLOPE}' is only allowed for style {cam_styles}. "
                )));
            }
            new_params.push(linear_slope);
        }

        match chan {
            -1 => log.params = [new_params.clone(), new_params.clone(), new_params],
            c => log.params[c as usize] = new_params,
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Character data

    fn handle_chars(&mut self, elms: &mut [Elt], s: &str) -> Result<()> {
        if s.is_empty() || s == "\n" {
            return Ok(());
        }
        let n = elms.len();
        if n == 0 {
            return Err(
                self.throw_message(&format!("CTF/CLF parsing error: missing end tag '{s}'."))
            );
        }
        let (before, top) = elms.split_at_mut(n - 1);
        let top = &mut top[0];
        if let Kind::Desc { desc, .. } = &mut top.kind {
            desc.push_str(s);
            return Ok(());
        }
        let (start, end) = find_sub_string(s.as_bytes());
        if end == 0 {
            return Ok(());
        }
        let trimmed = &s[start..end];
        match &mut top.kind {
            Kind::Metadata(md) | Kind::Info(md) => {
                md.element_value.push_str(trimmed);
                return Ok(());
            }
            _ => {}
        }
        if top.is_container() {
            return Err(
                self.throw_message(&format!("CTF/CLF parsing error: attribute illegal '{s}'."))
            );
        }
        let depth = before.len();
        let parent = before.last_mut();
        self.set_raw_data(top, parent, depth, trimmed)
    }

    fn set_raw_data(
        &mut self,
        top: &mut Elt,
        parent: Option<&mut Elt>,
        _depth: usize,
        s: &str,
    ) -> Result<()> {
        let line = top.line;
        let name = top.name.clone();
        let err = |m: &str| Error::msg(format!("At line {line}: {m}"));
        match &mut top.kind {
            Kind::Dummy => {}
            Kind::Id(id) => id.push_str(s),
            Kind::Array { position } => {
                let parent = parent.ok_or_else(|| err("Array without parent"))?;
                let type_name = parent.name.clone();
                let op = match &mut parent.kind {
                    Kind::Op(op) => op,
                    _ => return Err(err("Array without op")),
                };
                let b = s.as_bytes();
                let mut pos = find_next_token_start(b, 0);
                while pos != b.len() {
                    let v: f64 = match get_next_number(b, &mut pos) {
                        Ok(Some(v)) => v,
                        Ok(None) => break,
                        Err(_) => {
                            return Err(err(&format!(
                                "Illegal values '{}' in array of {}.",
                                truncate_string(s),
                                type_name
                            )))
                        }
                    };
                    let (max_values, arg) = array_info(op);
                    if *position < max_values {
                        set_array_value(op, *position, v);
                        *position += 1;
                    } else {
                        return Err(err(&format!(
                            "Expected {arg} Array, found too many values in array of '{type_name}'."
                        )));
                    }
                }
            }
            Kind::IndexMap { position } => {
                let parent = parent.ok_or_else(|| err("IndexMap without parent"))?;
                let type_name = parent.name.clone();
                let im = match &mut parent.kind {
                    Kind::Op(op) => match &mut op.kind {
                        OpKind::Lut1D { im, .. } | OpKind::Lut3D { im, .. } => im,
                        _ => return Err(err("IndexMap without LUT")),
                    },
                    _ => return Err(err("IndexMap without LUT")),
                };
                let b = s.as_bytes();
                let mut pos = find_next_token_start(b, 0);
                while pos != b.len() {
                    let (d1, d2) = match get_next_index_pair(b, &mut pos) {
                        Ok(Some(p)) => p,
                        Ok(None) => break,
                        Err(_) => {
                            return Err(err(&format!(
                                "Illegal values '{}' in '{}' IndexMap.",
                                truncate_string(s),
                                type_name
                            )))
                        }
                    };
                    if *position < im.dimension() {
                        im.set_pair(*position, d1, d2)?;
                        *position += 1;
                    } else {
                        return Err(err(&format!(
                            "Expected {} entries, found too many values in '{}' IndexMap.",
                            im.dimension(),
                            type_name
                        )));
                    }
                }
            }
            Kind::RangeValue => {
                let parent = parent.ok_or_else(|| err("Range value without parent"))?;
                let data: Vec<f64> = get_numbers(s).map_err(|e| {
                    err(&format!(
                        "Illegal '{}' values {} [{}]",
                        name,
                        truncate_string(s),
                        e
                    ))
                })?;
                if data.len() != 1 {
                    return Err(err("Range element: non-single value."));
                }
                if let Kind::Op(op) = &mut parent.kind {
                    if let OpKind::Range { range, .. } = &mut op.kind {
                        if eq_ic(&name, TAG_MIN_IN_VALUE) {
                            range.min_in = data[0];
                        } else if eq_ic(&name, TAG_MAX_IN_VALUE) {
                            range.max_in = data[0];
                        } else if eq_ic(&name, TAG_MIN_OUT_VALUE) {
                            range.min_out = data[0];
                        } else if eq_ic(&name, TAG_MAX_OUT_VALUE) {
                            range.max_out = data[0];
                        }
                    }
                }
            }
            Kind::SopValue(c) | Kind::Saturation(c) => {
                c.push_str(s);
                c.push(' ');
            }
            Kind::CurvePoints(data) | Kind::CurveSlopes(data) => {
                let v: Vec<f32> = get_numbers(s).map_err(|e| {
                    err(&format!(
                        "Illegal '{}' values {} [{}]",
                        name,
                        truncate_string(s),
                        e
                    ))
                })?;
                data.extend(v);
            }
            // The other plain elements ignore their content.
            _ => {}
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // End

    fn handle_end(&mut self, elms: &mut Vec<Elt>, name_full: &str) -> Result<()> {
        if name_full.is_empty() {
            return Err(Error::msg("CTF/CLF internal parsing error."));
        }
        let name: &str = if self.keep_namespaces <= 0 {
            match name_full.rfind(':') {
                Some(p) => &name_full[p + 1..],
                None => name_full,
            }
        } else {
            name_full
        };
        let elt = match elms.pop() {
            Some(e) => e,
            None => return Err(self.throw_message("CTF/CLF parsing error: Tag is missing.")),
        };
        if elt.name != name {
            return Err(
                self.throw_message(&format!("CTF/CLF parsing error: Tag '{name}' is missing."))
            );
        }
        if matches!(elt.kind, Kind::Info(_)) {
            self.keep_namespaces -= 1;
        }
        self.end_element(elms, elt)
    }

    /// Call `appendMetadata` on the element at `idx`.
    fn append_metadata(
        &mut self,
        elms: &mut [Elt],
        idx: usize,
        mut md: FormatMetadata,
    ) -> Result<()> {
        let (before, rest) = elms.split_at_mut(idx);
        match &mut rest[0].kind {
            Kind::Transform { .. } => self.transform_mut()?.metadata.children.push(md),
            Kind::Op(op) => op.metadata_mut().children.push(md),
            Kind::SopNode { .. } | Kind::SatNode => {
                let sop = matches!(rest[0].kind, Kind::SopNode { .. });
                md.element_name = if sop {
                    METADATA_SOP_DESCRIPTION
                } else {
                    METADATA_SAT_DESCRIPTION
                }
                .to_string();
                if let Some(Kind::Op(op)) = before.last_mut().map(|e| &mut e.kind) {
                    op.metadata_mut().children.push(md);
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn end_element(&mut self, elms: &mut Vec<Elt>, elt: Elt) -> Result<()> {
        let line = elt.line;
        let err = |m: &str| Error::msg(format!("At line {line}: {m}"));
        let n = elms.len();
        match elt.kind {
            Kind::Transform { .. } | Kind::Dummy => {}
            Kind::Metadata(md) => {
                if let Some(p) = elms.last_mut() {
                    if let Kind::Metadata(pm) | Kind::Info(pm) = &mut p.kind {
                        pm.children.push(md);
                    }
                }
            }
            Kind::Info(md) => {
                if let Some(Kind::Transform { .. }) = elms.last().map(|e| &e.kind) {
                    self.transform_mut()?.info_metadata = md;
                }
            }
            Kind::Desc { desc, language } => {
                let mut md = FormatMetadata::new(&elt.name, &desc);
                if !language.is_empty() {
                    md.add_attribute(ATTR_LANGUAGE, &language);
                }
                if n > 0 {
                    self.append_metadata(elms, n - 1, md)?;
                }
            }
            Kind::Id(id) => {
                if !validate_smpte_id(&id) {
                    let msg = format!(
                        "{}({}): '{}' is not a SMPTE ST 2136-1 compliant Id value.",
                        self.xml_file(),
                        line,
                        id
                    );
                    self.warn(msg);
                }
                if !id.is_empty() && n > 0 {
                    let md = FormatMetadata::new(&elt.name, &id);
                    self.append_metadata(elms, n - 1, md)?;
                }
            }
            Kind::Op(op) => {
                let ops = self.op_end(line, *op)?;
                let t = self.transform_mut()?;
                t.ops.extend(ops);
            }
            Kind::Array { position } => {
                if let Some(p) = elms.last_mut() {
                    let pline = p.line;
                    if let Kind::Op(op) = &mut p.kind {
                        end_array(pline, op, position)?;
                    }
                }
            }
            Kind::IndexMap { position } => {
                let version = self.transform_ref()?.version.clone();
                let file = self.xml_file().to_string();
                if let Some(p) = elms.last_mut() {
                    let pline = p.line;
                    if let Kind::Op(op) = &mut p.kind {
                        if version < CTF_PROCESS_LIST_VERSION_2_0 {
                            end_index_map(pline, op, position)?;
                        } else {
                            let msg = format!(
                                "{}({}): Element '{}' is not valid since CLF 3 (or CTF 2).",
                                file, line, elt.name
                            );
                            self.warn(msg);
                        }
                    }
                }
            }
            Kind::SopNode {
                slope,
                offset,
                power,
            } => {
                if !slope {
                    return Err(err("Required node 'Slope' is missing. "));
                }
                if !offset {
                    return Err(err("Required node 'Offset' is missing. "));
                }
                if !power {
                    return Err(err("Required node 'Power' is missing. "));
                }
            }
            Kind::SopValue(content) => {
                let content = trim(&content).to_string();
                let data: Vec<f64> = get_numbers(&content).map_err(|_| {
                    err(&format!(
                        "Illegal values '{}' in {}",
                        truncate_string(&content),
                        elt.name
                    ))
                })?;
                if data.len() != 3 {
                    return Err(err("SOPNode: 3 values required."));
                }
                let v = [data[0], data[1], data[2]];
                if n >= 2 {
                    let (before, rest) = elms.split_at_mut(n - 1);
                    if let Kind::SopNode {
                        slope,
                        offset,
                        power,
                    } = &mut rest[0].kind
                    {
                        if let Some(Kind::Op(op)) = before.last_mut().map(|e| &mut e.kind) {
                            if let OpKind::Cdl(cdl) = &mut op.kind {
                                if elt.name == TAG_SLOPE {
                                    cdl.slope = v;
                                    *slope = true;
                                } else if elt.name == TAG_OFFSET {
                                    cdl.offset = v;
                                    *offset = true;
                                } else if elt.name == TAG_POWER {
                                    cdl.power = v;
                                    *power = true;
                                }
                            }
                        }
                    }
                }
            }
            Kind::Saturation(content) => {
                let content = trim(&content).to_string();
                let data: Vec<f64> = get_numbers(&content).map_err(|_| {
                    err(&format!(
                        "Illegal values '{}' in {}",
                        truncate_string(&content),
                        elt.name
                    ))
                })?;
                if data.len() != 1 {
                    return Err(err("SatNode: non-single value. "));
                }
                if n >= 2 && elt.name == TAG_SATURATION {
                    if let Some(Kind::Op(op)) = elms.get_mut(n - 2).map(|e| &mut e.kind) {
                        if let OpKind::Cdl(cdl) = &mut op.kind {
                            cdl.sat = data[0];
                        }
                    }
                }
            }
            Kind::CurvePoints(data) => {
                if data.len() % 2 != 0 {
                    return Err(err("Control points element: odd number of values."));
                }
                if let Some(curve) = curve_of(elms)? {
                    let num = data.len() / 2;
                    curve.set_num_control_points(num);
                    for p in 0..num {
                        curve.control_points[p].x = data[2 * p];
                        curve.control_points[p].y = data[2 * p + 1];
                    }
                }
            }
            Kind::CurveSlopes(data) => {
                if let Some(curve) = curve_of(elms)? {
                    if data.len() != curve.num_control_points() {
                        return Err(err("Number of slopes must match number of control points."));
                    }
                    for (i, s) in data.iter().enumerate() {
                        curve.set_slope(i, *s);
                    }
                }
            }
            Kind::AcesParams
            | Kind::EcParams
            | Kind::DynamicParam
            | Kind::GammaParams { .. }
            | Kind::LogParams { .. }
            | Kind::RangeValue
            | Kind::SatNode
            | Kind::GradingPrimaryParam
            | Kind::GradingCurve { .. }
            | Kind::GradingToneParam => {}
        }
        Ok(())
    }

    /// Port of the `end` method of the op readers. Returns the process
    /// nodes to append to the transform.
    fn op_end(&mut self, line: u32, op: OpElt) -> Result<Vec<OpData>> {
        let err = |m: &str| Error::msg(format!("At line {line}: {m}"));
        let (in_bd, out_bd) = (op.in_bd, op.out_bd);
        Ok(match op.kind {
            OpKind::Aces(ff) | OpKind::FixedFunction(ff) | OpKind::Function(ff) => {
                ff.validate()?;
                vec![OpData::FixedFunction(ff)]
            }
            OpKind::Cdl(cdl) => {
                cdl.validate()?;
                vec![OpData::Cdl(cdl)]
            }
            OpKind::Gamma { gamma, .. } => {
                gamma
                    .validate()
                    .map_err(|e| err(&format!("Invalid parameters: {e}.")))?;
                vec![OpData::Gamma(gamma)]
            }
            OpKind::GradingPrimary(gp) => {
                validate_grading_primary(&gp.value, gp.style)?;
                vec![OpData::GradingPrimary(gp)]
            }
            OpKind::GradingRgbCurve(c) => {
                validate_rgb_curves(&c.value)?;
                vec![OpData::GradingRgbCurve(c)]
            }
            OpKind::GradingHueCurve(c) => {
                validate_hue_curves(&c.value)?;
                vec![OpData::GradingHueCurve(c)]
            }
            OpKind::GradingTone(t) => {
                validate_grading_tone(&t.value)?;
                vec![OpData::GradingTone(t)]
            }
            OpKind::InvLut1D { mut lut, .. } => {
                let scale = 1.0f32 / in_bd.max_value() as f32;
                lut.scale(scale);
                lut.file_output_bd = in_bd;
                lut.validate()?;
                vec![OpData::Lut1D(lut)]
            }
            OpKind::InvLut3D { mut lut, .. } => {
                let scale = 1.0f32 / in_bd.max_value() as f32;
                lut.scale(scale);
                lut.file_output_bd = in_bd;
                lut.validate()?;
                vec![OpData::Lut3D(lut)]
            }
            OpKind::Log {
                mut log,
                ctf,
                mut base_set,
                v2,
            } => {
                if !v2 {
                    let dir = ctf.style.direction();
                    let (base, params) = convert_log_parameters(&ctf)
                        .map_err(|e| err(&format!("Parameters are not valid: '{e}'.")))?;
                    log.base = base;
                    log.dir = dir;
                    log.params = params;
                } else {
                    log.dir = ctf.style.direction();
                    if ctf.get_type() == CtfLogParamsType::Cineon {
                        let (base, params) = convert_log_parameters(&ctf)
                            .map_err(|e| err(&format!("Parameters are not valid: '{e}'.")))?;
                        log.base = base;
                        log.params = params;
                    } else if !base_set {
                        match ctf.style {
                            LogStyle::Log2 | LogStyle::AntiLog2 => {
                                set_log_base(line, &mut log, &mut base_set, 2.0)?
                            }
                            LogStyle::Log10 | LogStyle::AntiLog10 => {
                                set_log_base(line, &mut log, &mut base_set, 10.0)?
                            }
                            _ => {}
                        }
                    }
                }
                log.validate()
                    .map_err(|e| err(&format!("Log is not valid: '{e}'.")))?;
                vec![OpData::Log(log)]
            }
            OpKind::Lut1D {
                mut lut,
                im,
                completed_im,
                variant,
                ..
            } => {
                let scale = 1.0f32 / out_bd.max_value() as f32;
                lut.scale(scale);
                lut.file_output_bd = out_bd;
                lut.validate()?;
                let mut res = Vec::new();
                if variant == Lut1DVariant::V17 && completed_im {
                    res.push(OpData::Range(RangeData::from_index_mapping(
                        &im, lut.length, in_bd,
                    )?));
                }
                res.push(OpData::Lut1D(lut));
                res
            }
            OpKind::Lut3D {
                mut lut,
                im,
                completed_im,
                v17,
                ..
            } => {
                let scale = 1.0f32 / out_bd.max_value() as f32;
                lut.scale(scale);
                lut.file_output_bd = out_bd;
                lut.validate()?;
                let mut res = Vec::new();
                if v17 && completed_im {
                    res.push(OpData::Range(RangeData::from_index_mapping(
                        &im,
                        lut.grid_size,
                        in_bd,
                    )?));
                }
                res.push(OpData::Lut3D(lut));
                res
            }
            OpKind::Matrix {
                mut m,
                mut array,
                mut length,
                mut comps,
                ..
            } => {
                // Scale the array (and offsets) to normalized values.
                let in_scale = in_bd.max_value();
                let out_scale = 1.0 / out_bd.max_value();
                let combined = in_scale * out_scale;
                if combined != 1.0 {
                    for v in &mut array {
                        *v *= combined;
                    }
                }
                for o in &mut m.offsets {
                    *o *= out_scale;
                }
                m.file_in_bd = in_bd;
                m.file_out_bd = out_bd;
                // Validate the array (expand 3x3 to 4x4).
                let wrap = |e: &str| Error::msg(format!("Matrix array content issue: {e}"));
                if length == 0 {
                    return Err(wrap("Array content is empty."));
                }
                if array.len() != length * length {
                    return Err(wrap(&format!(
                        "Array contains: {} values, but {} are expected.",
                        array.len(),
                        length * length
                    )));
                }
                if length == 3 {
                    let old = array.clone();
                    array = vec![
                        old[0], old[1], old[2], 0.0, old[3], old[4], old[5], 0.0, old[6], old[7],
                        old[8], 0.0, 0.0, 0.0, 0.0, 1.0,
                    ];
                    length = 4;
                    comps = 4;
                } else if length != 4 {
                    return Err(wrap("Matrix: array content issue."));
                }
                if comps != 4 {
                    return Err(wrap("Matrix: dimensions must be 4x4."));
                }
                let _ = length;
                m.matrix.copy_from_slice(&array[..16]);
                m.validate()?;
                vec![OpData::Matrix(m)]
            }
            OpKind::Range {
                mut range,
                v17,
                no_clamp,
            } => {
                range.file_in_bd = in_bd;
                range.file_out_bd = out_bd;
                range.normalize();
                range.validate()?;
                if v17 && no_clamp {
                    vec![OpData::Matrix(range.convert_to_matrix()?)]
                } else {
                    vec![OpData::Range(range)]
                }
            }
            OpKind::Reference(r) => vec![OpData::Reference(r)],
            OpKind::ExposureContrast(ec) => vec![OpData::ExposureContrast(ec)],
        })
    }
}

/// Port of `CTFReaderLogElt::setBase`.
fn set_log_base(line: u32, log: &mut LogData, base_set: &mut bool, base: f64) -> Result<()> {
    if *base_set {
        if log.base != base {
            crate::bail!(
                "At line {}: Log base has to be the same on all components: Current base: {}, new base: {}.",
                line,
                fmt_g(log.base, DEFAULT_PRECISION),
                fmt_g(base, DEFAULT_PRECISION)
            );
        }
    } else {
        *base_set = true;
        log.base = base;
    }
    Ok(())
}

/// Port of `validateInfoElementVersion`.
fn validate_info_element_version(attr: &str, value: &str) -> Result<()> {
    if !attr.is_empty() && eq_ic(ATTR_VERSION, attr) {
        if value.is_empty() {
            return Err(Error::msg(
                "CTF reader. Invalid Info element version attribute.",
            ));
        }
        // Equivalent of sscanf("%d").
        let t = value.trim_start();
        let digits: String = {
            let mut s = String::new();
            let mut chars = t.chars().peekable();
            if let Some(&c) = chars.peek() {
                if c == '-' || c == '+' {
                    s.push(c);
                    chars.next();
                }
            }
            for c in chars {
                if c.is_ascii_digit() {
                    s.push(c);
                } else {
                    break;
                }
            }
            s
        };
        let fver: i64 = match digits.parse() {
            Ok(v) => v,
            Err(_) => crate::bail!(
                "CTF reader. Invalid Info element version attribute: {} .",
                value
            ),
        };
        if fver > i64::from(CTF_INFO_ELEMENT_VERSION) {
            crate::bail!(
                "CTF reader. Unsupported Info element version attribute: {} .",
                value
            );
        }
    }
    Ok(())
}

/// Parse the name of a grading curve element: (is RGB curve, index).
fn curve_type_from_name(name: &str) -> Result<(bool, usize)> {
    if let Some(i) = TAG_RGB_CURVE_NAMES.iter().position(|n| eq_ic(n, name)) {
        return Ok((true, i));
    }
    if let Some(i) = TAG_HUE_CURVE_NAMES.iter().position(|n| eq_ic(n, name)) {
        return Ok((false, i));
    }
    crate::bail!("Illegal grading curve name '{}'.", name)
}

/// The curve being loaded for the curve element below the top of `elms`
/// (the parent of the points / slopes element that was just popped).
fn curve_of(
    elms: &mut [Elt],
) -> Result<Option<&mut crate::transforms::grading::GradingBSplineCurve>> {
    let n = elms.len();
    if n < 2 {
        return Ok(None);
    }
    let (before, rest) = elms.split_at_mut(n - 1);
    let (rgb, index) = match rest[0].kind {
        Kind::GradingCurve { rgb, index } => (rgb, index),
        _ => return Ok(None),
    };
    let op = match before.last_mut().map(|e| &mut e.kind) {
        Some(Kind::Op(op)) => op,
        _ => return Ok(None),
    };
    Ok(match &mut op.kind {
        OpKind::GradingRgbCurve(c) if rgb => c.value.curves.get_mut(index),
        OpKind::GradingHueCurve(c) if !rgb => c.value.curves.get_mut(index),
        _ => None,
    })
}

/// Port of the `updateDimension` methods. Returns false for illegal
/// dimensions.
fn update_dimension(op: &mut OpElt, dims: &[u32]) -> bool {
    let d = |i: usize| dims[i] as usize;
    match &mut op.kind {
        OpKind::Lut1D { lut, .. } | OpKind::InvLut1D { lut, .. } => {
            if dims.len() != 2 {
                return false;
            }
            if d(1) != 3 && d(1) != 1 {
                return false;
            }
            if d(0) < 2 || d(0) > MAX_1D_LUT_LENGTH {
                return false;
            }
            lut.resize(d(0), d(1));
            true
        }
        OpKind::Lut3D { lut, .. } | OpKind::InvLut3D { lut, .. } => {
            if dims.len() != 4 {
                return false;
            }
            if d(3) != 3 || d(1) != d(0) || d(2) != d(0) {
                return false;
            }
            if d(0) < 2 || d(0) > MAX_3D_LUT_LENGTH {
                return false;
            }
            lut.resize(d(0), d(3));
            true
        }
        OpKind::Matrix {
            array,
            length,
            comps,
            v13,
            ..
        } => {
            if !*v13 {
                if dims.len() != 3 {
                    return false;
                }
                let num_comps = d(2);
                let size = d(0);
                if size != d(1) || num_comps != 3 {
                    return false;
                }
                if !(3..=4).contains(&size) {
                    return false;
                }
                *length = size;
                *comps = num_comps;
                array.resize(size * size, 0.0);
                true
            } else {
                let size = dims.len();
                if size != 3 && size != 2 {
                    return false;
                }
                let ok = matches!((d(0), d(1)), (3, 3) | (3, 4) | (4, 4) | (4, 5));
                if !ok {
                    return false;
                }
                if size == 3 && d(0) != d(2) {
                    return false;
                }
                *length = d(1);
                *comps = d(0);
                array.resize(d(1) * d(1), 0.0);
                true
            }
        }
        _ => false,
    }
}

/// Maximum number of array values and the dimension string used in error
/// messages.
fn array_info(op: &OpElt) -> (usize, String) {
    match &op.kind {
        OpKind::Lut1D { lut, .. } | OpKind::InvLut1D { lut, .. } => (
            lut.length * 3,
            format!("{}x{}", lut.length, lut.num_components),
        ),
        OpKind::Lut3D { lut, .. } | OpKind::InvLut3D { lut, .. } => {
            let l = lut.grid_size;
            (l * l * l * 3, format!("{l}x{l}x{l}x{}", lut.num_components))
        }
        OpKind::Matrix { length, .. } => (length * length, format!("{length}x{length}")),
        _ => (0, String::new()),
    }
}

fn set_array_value(op: &mut OpElt, pos: usize, v: f64) {
    match &mut op.kind {
        OpKind::Lut1D { lut, .. } | OpKind::InvLut1D { lut, .. } => lut.values[pos] = v as f32,
        OpKind::Lut3D { lut, .. } | OpKind::InvLut3D { lut, .. } => lut.values[pos] = v as f32,
        OpKind::Matrix { array, .. } => array[pos] = v,
        _ => {}
    }
}

/// Port of the `endArray` methods.
fn end_array(line: u32, op: &mut OpElt, position: usize) -> Result<()> {
    let err = |m: &str| Error::msg(format!("At line {line}: {m}"));
    match &mut op.kind {
        OpKind::Lut1D { lut, completed, .. } | OpKind::InvLut1D { lut, completed } => {
            if lut.raw_halfs {
                for v in &mut lut.values {
                    *v = half::f16::from_bits(*v as u16).to_f32();
                }
            }
            let num_values = lut.length * 3;
            if num_values != position {
                let dims = lut.length;
                if lut.num_components != 1 || position != dims {
                    return Err(err(&format!(
                        "Expected {}x{} Array values, found {}.",
                        dims, lut.num_components, position
                    )));
                }
                for i in (0..dims).rev() {
                    let v = lut.values[i];
                    for j in 0..3 {
                        lut.values[i * 3 + j] = v;
                    }
                }
            }
            if lut.length == 0 {
                return Err(Error::msg("Array content is empty."));
            }
            *completed = true;
        }
        OpKind::Lut3D { lut, completed, .. } | OpKind::InvLut3D { lut, completed } => {
            let l = lut.grid_size;
            if l * l * l * 3 != position {
                return Err(err(&format!(
                    "Expected {l}x{l}x{l}x{} Array values, found {position}.",
                    lut.num_components
                )));
            }
            if l == 0 {
                return Err(Error::msg("Array content is empty."));
            }
            *completed = true;
        }
        OpKind::Matrix {
            m,
            array,
            length,
            comps,
            completed,
            v13,
        } => {
            if !*v13 {
                if *length * *length != position {
                    crate::bail!(
                        "Expected {}x{} Array values, found {}",
                        length,
                        length,
                        position
                    );
                }
                *completed = true;
                // Convert matrix data from 1.2 to the latest.
                if *length == 3 {
                    m.offsets = [0.0; 4];
                } else if *length == 4 {
                    let old = array.clone();
                    m.offsets = [old[3], old[7], old[11], 0.0];
                    *array = vec![
                        old[0], old[1], old[2], old[4], old[5], old[6], old[8], old[9], old[10],
                    ];
                    *length = 3;
                    *comps = 3;
                } else {
                    crate::bail!(
                        "MatrixElt: Expecting array dimension to be 3 or 4. Got: {}.",
                        length
                    );
                }
            } else {
                if *length == 3 && *comps == 3 {
                    if position != 9 {
                        return Err(err(&format!(
                            "Expected 3x3 Array values, found {position}."
                        )));
                    }
                } else if *length == 4 {
                    if *comps == 3 {
                        if position != 12 {
                            return Err(err(&format!(
                                "Expected 3x4 Array values, found {position}."
                            )));
                        }
                        let old = array.clone();
                        m.offsets = [old[3], old[7], old[11], 0.0];
                        *array = vec![
                            old[0], old[1], old[2], old[4], old[5], old[6], old[8], old[9], old[10],
                        ];
                        *length = 3;
                    } else {
                        if position != 16 {
                            return Err(err(&format!(
                                "Expected 4x4 Array values, found {position}."
                            )));
                        }
                        m.offsets = [0.0; 4];
                    }
                } else {
                    if position != 20 {
                        return Err(err(&format!(
                            "Expected 4x5 Array values, found {position}."
                        )));
                    }
                    let old = array.clone();
                    m.offsets = [old[4], old[9], old[14], old[19]];
                    *array = vec![
                        old[0], old[1], old[2], old[3], old[5], old[6], old[7], old[8], old[10],
                        old[11], old[12], old[13], old[15], old[16], old[17], old[18],
                    ];
                    *length = 4;
                    *comps = 4;
                }
                *completed = true;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Port of the `endIndexMap` methods.
fn end_index_map(line: u32, op: &mut OpElt, position: usize) -> Result<()> {
    match &mut op.kind {
        OpKind::Lut1D {
            im, completed_im, ..
        }
        | OpKind::Lut3D {
            im, completed_im, ..
        } => {
            if im.dimension() != position {
                crate::bail!(
                    "At line {}: Expected {} IndexMap values, found {}.",
                    line,
                    im.dimension(),
                    position
                );
            }
            im.validate()?;
            *completed_im = true;
        }
        _ => {}
    }
    Ok(())
}

/// Parse the attributes of a grading RGBM element.
fn parse_rgbm(
    name: &str,
    line: u32,
    atts: &[(String, String)],
    rgbm: &mut GradingRgbm,
) -> Result<()> {
    let err = |m: &str| Error::msg(format!("At line {line}: {m}"));
    let mut rgb_found = false;
    let mut master_found = false;
    for (n, v) in atts {
        if n.is_empty() {
            break;
        }
        let data: Vec<f64> = get_numbers(v).map_err(|e| {
            err(&format!(
                "Illegal '{}' values {} [{}].",
                name,
                truncate_string(v),
                e
            ))
        })?;
        if eq_ic(ATTR_RGB, n) {
            if data.len() != 3 {
                return Err(err(&format!(
                    "Illegal number of 'rgb' values for '{}': '{}'.",
                    name,
                    truncate_string(v)
                )));
            }
            rgbm.red = data[0];
            rgbm.green = data[1];
            rgbm.blue = data[2];
            rgb_found = true;
        } else if eq_ic(ATTR_MASTER, n) {
            if data.len() != 1 {
                return Err(err(&format!(
                    "'Master' for '{}' must be a single value: '{}'",
                    name,
                    truncate_string(v)
                )));
            }
            rgbm.master = data[0];
            master_found = true;
        } else {
            return Err(err(&format!("Illegal attribute for '{name}': '{n}'.")));
        }
    }
    if !rgb_found {
        return Err(err(&format!("Missing 'rgb' attribute for '{name}'.")));
    }
    if !master_found {
        return Err(err(&format!("Missing 'master' attribute for '{name}'.")));
    }
    Ok(())
}

/// Parse black / white attributes (`Clamp` element).
fn parse_bw(
    name: &str,
    line: u32,
    atts: &[(String, String)],
    black: &mut f64,
    white: &mut f64,
) -> Result<()> {
    let err = |m: &str| Error::msg(format!("At line {line}: {m}"));
    let mut black_found = false;
    let mut white_found = false;
    for (n, v) in atts {
        if n.is_empty() {
            break;
        }
        let data: Vec<f64> = get_numbers(v).map_err(|e| {
            err(&format!(
                "Illegal '{}' values {} [{}].",
                name,
                truncate_string(v),
                e
            ))
        })?;
        if eq_ic(ATTR_PRIMARY_BLACK, n) {
            if data.len() != 1 {
                return Err(err(&format!(
                    "'Black' for '{}' must be a single value: '{}'.",
                    name,
                    truncate_string(v)
                )));
            }
            *black = data[0];
            black_found = true;
        } else if eq_ic(ATTR_PRIMARY_WHITE, n) {
            if data.len() != 1 {
                return Err(err(&format!(
                    "'White' for '{}' must be a single value: '{}'.",
                    name,
                    truncate_string(v)
                )));
            }
            *white = data[0];
            white_found = true;
        } else {
            return Err(err(&format!("Illegal attribute for '{name}': '{n}'.")));
        }
    }
    if !black_found && !white_found {
        return Err(err(&format!(
            "Missing 'black' or 'white' attribute for '{name}'."
        )));
    }
    Ok(())
}

/// Parse contrast / black / white attributes (`Pivot` element).
fn parse_pivot(
    name: &str,
    line: u32,
    atts: &[(String, String)],
    contrast: &mut f64,
    black: &mut f64,
    white: &mut f64,
) -> Result<()> {
    let err = |m: &str| Error::msg(format!("At line {line}: {m}"));
    let mut contrast_found = false;
    let mut black_found = false;
    let mut white_found = false;
    for (n, v) in atts {
        if n.is_empty() {
            break;
        }
        let data: Vec<f64> = get_numbers(v).map_err(|e| {
            err(&format!(
                "Illegal '{}' values {} [{}].",
                name,
                truncate_string(v),
                e
            ))
        })?;
        if eq_ic(ATTR_PRIMARY_BLACK, n) {
            if data.len() != 1 {
                return Err(err(&format!(
                    "'Black' for '{}' must be a single value: '{}'.",
                    name,
                    truncate_string(v)
                )));
            }
            *black = data[0];
            black_found = true;
        } else if eq_ic(ATTR_PRIMARY_WHITE, n) {
            if data.len() != 1 {
                return Err(err(&format!(
                    "'White' for '{}' must be a single value: '{}'.",
                    name,
                    truncate_string(v)
                )));
            }
            *white = data[0];
            white_found = true;
        } else if eq_ic(ATTR_PRIMARY_CONTRAST, n) {
            if data.len() != 1 {
                return Err(err(&format!(
                    "'Contrast' for '{}' must be a single value: '{}'.",
                    name,
                    truncate_string(v)
                )));
            }
            *contrast = data[0];
            contrast_found = true;
        } else {
            return Err(err(&format!("Illegal attribute for '{name}': '{n}'.")));
        }
    }
    if !contrast_found && !white_found && !black_found {
        return Err(err(&format!(
            "Missing 'contrast', 'black' or 'white' attribute for '{name}'."
        )));
    }
    Ok(())
}

/// Parse a single named attribute (`Saturation`, `SContrast`).
fn parse_scalar_attr_value(
    name: &str,
    line: u32,
    atts: &[(String, String)],
    tag: &str,
    value: &mut f64,
) -> Result<()> {
    let err = |m: &str| Error::msg(format!("At line {line}: {m}"));
    let mut found = false;
    for (n, v) in atts {
        if n.is_empty() {
            break;
        }
        let data: Vec<f64> = get_numbers(v).map_err(|e| {
            err(&format!(
                "Illegal '{}' values {} [{}].",
                name,
                truncate_string(v),
                e
            ))
        })?;
        if eq_ic(tag, n) {
            if data.len() != 1 {
                return Err(err(&format!(
                    "'{}' for '{}' must be a single value: '{}'.",
                    tag,
                    name,
                    truncate_string(v)
                )));
            }
            *value = data[0];
            found = true;
        } else {
            return Err(err(&format!("Illegal attribute for '{name}': '{n}'.")));
        }
    }
    if !found {
        return Err(err(&format!("Missing attribute for '{name}'.")));
    }
    Ok(())
}

/// Port of `CTFReaderGradingPrimaryParamElt::start`.
fn grading_primary_param_start(
    name: &str,
    line: u32,
    parent: &mut Elt,
    atts: &[(String, String)],
) -> Result<()> {
    let v = match &mut parent.kind {
        Kind::Op(op) => match &mut op.kind {
            OpKind::GradingPrimary(gp) => &mut gp.value,
            _ => return Ok(()),
        },
        _ => return Ok(()),
    };
    if eq_ic(TAG_PRIMARY_BRIGHTNESS, name) {
        parse_rgbm(name, line, atts, &mut v.brightness)?;
    } else if eq_ic(TAG_PRIMARY_CONTRAST, name) {
        parse_rgbm(name, line, atts, &mut v.contrast)?;
    } else if eq_ic(TAG_PRIMARY_GAMMA, name) {
        parse_rgbm(name, line, atts, &mut v.gamma)?;
    } else if eq_ic(TAG_PRIMARY_PIVOT, name) {
        parse_pivot(
            name,
            line,
            atts,
            &mut v.pivot,
            &mut v.pivot_black,
            &mut v.pivot_white,
        )?;
    } else if eq_ic(TAG_PRIMARY_SATURATION, name) {
        parse_scalar_attr_value(name, line, atts, ATTR_MASTER, &mut v.saturation)?;
    } else if eq_ic(TAG_PRIMARY_OFFSET, name) {
        parse_rgbm(name, line, atts, &mut v.offset)?;
    } else if eq_ic(TAG_PRIMARY_EXPOSURE, name) {
        parse_rgbm(name, line, atts, &mut v.exposure)?;
    } else if eq_ic(TAG_PRIMARY_LIFT, name) {
        parse_rgbm(name, line, atts, &mut v.lift)?;
    } else if eq_ic(TAG_PRIMARY_GAIN, name) {
        parse_rgbm(name, line, atts, &mut v.gain)?;
    } else if eq_ic(TAG_PRIMARY_CLAMP, name) {
        parse_bw(name, line, atts, &mut v.clamp_black, &mut v.clamp_white)?;
    }
    Ok(())
}

/// Parse a grading tone RGBMSW element.
fn parse_rgbmsw(
    name: &str,
    line: u32,
    atts: &[(String, String)],
    v: &mut GradingRgbmsw,
    center: bool,
    pivot: bool,
) -> Result<()> {
    let err = |m: &str| Error::msg(format!("At line {line}: {m}"));
    let start_name = if center { ATTR_CENTER } else { ATTR_START };
    let width_name = if pivot { ATTR_PIVOT } else { ATTR_WIDTH };
    let (mut rgb_found, mut master_found, mut start_found, mut width_found) =
        (false, false, false, false);
    for (n, val) in atts {
        if n.is_empty() {
            break;
        }
        let data: Vec<f64> = get_numbers(val).map_err(|e| {
            err(&format!(
                "Illegal '{}' values {} [{}].",
                name,
                truncate_string(val),
                e
            ))
        })?;
        if eq_ic(ATTR_RGB, n) {
            if data.len() != 3 {
                return Err(err(&format!(
                    "Illegal number of 'rgb' values for '{}': '{}'.",
                    name,
                    truncate_string(val)
                )));
            }
            v.red = data[0];
            v.green = data[1];
            v.blue = data[2];
            rgb_found = true;
        } else if eq_ic(ATTR_MASTER, n) {
            if data.len() != 1 {
                return Err(err(&format!(
                    "'Master' for '{}' must be a single value: '{}'",
                    name,
                    truncate_string(val)
                )));
            }
            v.master = data[0];
            master_found = true;
        } else if eq_ic(start_name, n) {
            if data.len() != 1 {
                return Err(err(&format!(
                    "'{}' for '{}' must be a single value: '{}'",
                    start_name,
                    name,
                    truncate_string(val)
                )));
            }
            v.start = data[0];
            start_found = true;
        } else if eq_ic(width_name, n) {
            if data.len() != 1 {
                return Err(err(&format!(
                    "'{}' for '{}' must be a single value: '{}'",
                    width_name,
                    name,
                    truncate_string(val)
                )));
            }
            v.width = data[0];
            width_found = true;
        } else {
            return Err(err(&format!("Illegal attribute for '{name}': '{n}'.")));
        }
    }
    if !rgb_found {
        return Err(err(&format!("Missing 'rgb' attribute for '{name}'.")));
    }
    if !master_found {
        return Err(err(&format!("Missing 'master' attribute for '{name}'.")));
    }
    if !start_found {
        return Err(err(&format!(
            "Missing '{start_name}' attribute for '{name}'."
        )));
    }
    if !width_found {
        return Err(err(&format!(
            "Missing '{width_name}' attribute for '{name}'."
        )));
    }
    Ok(())
}

/// Port of `CTFReaderGradingToneParamElt::start`.
fn grading_tone_param_start(
    name: &str,
    line: u32,
    parent: &mut Elt,
    atts: &[(String, String)],
) -> Result<()> {
    let v = match &mut parent.kind {
        Kind::Op(op) => match &mut op.kind {
            OpKind::GradingTone(t) => &mut t.value,
            _ => return Ok(()),
        },
        _ => return Ok(()),
    };
    if eq_ic(TAG_TONE_BLACKS, name) {
        parse_rgbmsw(name, line, atts, &mut v.blacks, false, false)
    } else if eq_ic(TAG_TONE_SHADOWS, name) {
        parse_rgbmsw(name, line, atts, &mut v.shadows, false, true)
    } else if eq_ic(TAG_TONE_MIDTONES, name) {
        parse_rgbmsw(name, line, atts, &mut v.midtones, true, false)
    } else if eq_ic(TAG_TONE_HIGHLIGHTS, name) {
        parse_rgbmsw(name, line, atts, &mut v.highlights, false, true)
    } else if eq_ic(TAG_TONE_WHITES, name) {
        parse_rgbmsw(name, line, atts, &mut v.whites, false, false)
    } else if eq_ic(TAG_TONE_SCONTRAST, name) {
        parse_scalar_attr_value(name, line, atts, ATTR_MASTER, &mut v.s_contrast)
    } else {
        crate::bail!("At line {}: Invalid element '{}'.", line, name)
    }
}

impl SaxHandler for Reader {
    fn start_element(&mut self, name: &str, atts: &[(String, String)], line: u32) -> Result<()> {
        self.line = line;
        let mut elms = std::mem::take(&mut self.elms);
        let r = self.handle_start(&mut elms, name, atts);
        self.elms = elms;
        r
    }

    fn end_element(&mut self, name: &str, line: u32) -> Result<()> {
        self.line = line;
        let mut elms = std::mem::take(&mut self.elms);
        let r = self.handle_end(&mut elms, name);
        self.elms = elms;
        r
    }

    fn character_data(&mut self, s: &str, line: u32) -> Result<()> {
        self.line = line;
        let mut elms = std::mem::take(&mut self.elms);
        let r = self.handle_chars(&mut elms, s);
        self.elms = elms;
        r
    }
}

/// Port of `isLoadableCTF`: look for the `ProcessList` tag in the first
/// kilobytes of the file.
pub(crate) fn is_loadable_ctf(data: &[u8]) -> bool {
    const LIMIT: usize = 5 * 1024;
    let mut processed = 0usize;
    let mut rest = data;
    while !rest.is_empty() && processed < LIMIT {
        let (line, next, too_long) = match rest.iter().position(|&c| c == b'\n') {
            Some(p) if p < LIMIT => (&rest[..p], &rest[p + 1..], false),
            Some(_) | None if rest.len() >= LIMIT => (&rest[..LIMIT - 1], &rest[LIMIT - 1..], true),
            Some(p) => (&rest[..p], &rest[p + 1..], false),
            None => (rest, &rest[rest.len()..], false),
        };
        // Strings stop at the first null character.
        let line = match line.iter().position(|&c| c == 0) {
            Some(p) => &line[..p],
            None => line,
        };
        let s = String::from_utf8_lossy(line);
        if s.contains("<ProcessList") || s.contains(":ProcessList") {
            return true;
        }
        processed += line.len();
        if too_long {
            break;
        }
        rest = next;
    }
    false
}

/// Parse the content of a CLF/CTF file (port of `LocalFileFormat::read` up
/// to the creation of the cached file).
pub(crate) fn parse_ctf(data: &[u8], file_path: &str) -> Result<CtfParseResult> {
    if !is_loadable_ctf(data) {
        crate::bail!("Parsing error: '{}' is not a CTF/CLF file.", file_path);
    }
    let is_clf_ext = std::path::Path::new(file_path)
        .extension()
        .map_or(false, |e| e.to_string_lossy().eq_ignore_ascii_case("clf"));
    let mut reader = Reader {
        file_name: file_path.to_string(),
        is_clf_ext,
        line: 0,
        keep_namespaces: 0,
        transform: None,
        warnings: Vec::new(),
        elms: Vec::new(),
    };
    match parse_xml(data, &mut reader) {
        Ok(()) => {}
        Err(SaxError::Handler(e)) => return Err(e),
        Err(SaxError::TagMismatch(line)) => {
            reader.line = line;
            let msg = match reader.elms.last() {
                Some(e) => format!("CTF/CLF parsing error (no closing tag for '{}').", e.name),
                None => "CTF/CLF parsing error (unbalanced element tags).".to_string(),
            };
            return Err(reader.throw_message(&msg));
        }
        Err(SaxError::Syntax(msg, line)) => {
            reader.line = line;
            return Err(reader.throw_message(&format!("CTF/CLF parsing error: {msg}")));
        }
    }
    reader.line = fed_line_count(data);
    if let Some(e) = reader.elms.last() {
        return Err(reader.throw_message(&format!(
            "CTF/CLF parsing error (no closing tag for '{}) ",
            e.name
        )));
    }
    let transform = match reader.transform.take() {
        Some(t) => t,
        None => return Err(reader.throw_message("CTF/CLF parsing error: Invalid transform.")),
    };
    if transform.ops.is_empty() {
        return Err(reader.throw_message("CTF/CLF parsing error: No color operator in file."));
    }
    Ok(CtfParseResult {
        transform,
        warnings: reader.warnings,
    })
}
