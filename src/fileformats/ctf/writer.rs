//! CLF / CTF writer (port of `TransformWriter` and the op writers of
//! `CTFTransform.cpp`, plus the op collection done by
//! `LocalFileFormat::write` in `FileFormatCTF.cpp`).

use super::opdata::*;
use super::reader::*;
use super::transform::*;
use super::xml::*;
use crate::config::Config;
use crate::context::Context;
use crate::error::{Error, Result};
use crate::format_metadata::FormatMetadata;
use crate::transforms::grading::{
    default_hue_curve, default_rgb_curve, GradingBSplineCurve, GradingPrimary, GradingRgbm,
    GradingRgbmsw, GradingTone,
};
use crate::transforms::{
    GroupTransform, Transform, METADATA_INPUT_DESCRIPTION, METADATA_SAT_DESCRIPTION,
    METADATA_SOP_DESCRIPTION, METADATA_VIEWING_DESCRIPTION,
};
use crate::types::*;

/// Precision used to write double values (see the note in OCIO: 17 would
/// restore most doubles but introduces serialization issues).
pub(crate) const DOUBLE_PRECISION: usize = 15;

/// The flavour of the written file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SubFormat {
    /// Academy/ASC Common LUT Format (SMPTE ST 2136-1).
    Clf,
    /// Autodesk Color Transform Format.
    Ctf,
}

// ---------------------------------------------------------------------------
// Case insensitive metadata helpers (OCIO's FormatMetadata lookups ignore case).

fn children_ic<'a>(
    m: &'a FormatMetadata,
    name: &'a str,
) -> impl Iterator<Item = &'a FormatMetadata> + 'a {
    m.children
        .iter()
        .filter(move |c| c.element_name.eq_ignore_ascii_case(name))
}

fn attribute_ic<'a>(m: &'a FormatMetadata, name: &str) -> &'a str {
    m.attributes
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
        .unwrap_or("")
}

// ---------------------------------------------------------------------------
// Collecting the process nodes.

/// Collect the process nodes of `t` applied in `dir` (port of the op building
/// done by `BuildGroupOps`, then `ops.optimize(OPTIMIZATION_NONE)`).
///
/// Transforms that are directly representable are converted without a
/// config; the others (color spaces, looks, files, ...) are converted to
/// plain transforms through a processor of `config`.
fn collect_ops(
    config: &Config,
    context: &Context,
    t: &Transform,
    dir: TransformDirection,
    ops: &mut Vec<OpData>,
) -> Result<()> {
    if let Transform::Group(g) = t {
        let d = g.direction.combine(dir);
        match d {
            TransformDirection::Forward => {
                for c in &g.transforms {
                    collect_ops(config, context, c, TransformDirection::Forward, ops)?;
                }
            }
            TransformDirection::Inverse => {
                for c in g.transforms.iter().rev() {
                    collect_ops(config, context, c, TransformDirection::Inverse, ops)?;
                }
            }
        }
        return Ok(());
    }
    if let Some(op) = OpData::from_transform(t, dir)? {
        ops.push(op);
        return Ok(());
    }
    // Not directly representable: build it with the config.
    let processor = config.get_processor_with_context(context, t, dir)?;
    let group = processor.create_group_transform();
    for c in &group.transforms {
        collect_ops(config, context, c, TransformDirection::Forward, ops)?;
    }
    Ok(())
}

/// Port of `Lut1DOpData::finalize` for an inverse LUT: flatten the
/// reversals (see `initializeFromForward`) so that the LUT is monotonic.
fn flatten_inverse_lut1d(lut: &mut Lut1DData) {
    const MAX_CHANNELS: usize = 3;
    let length = lut.length;
    if length == 0 || lut.values.len() < length * MAX_CHANNELS {
        return;
    }
    let half = lut.half_domain && length >= HALF_DOMAIN_REQUIRED_ENTRIES;
    let values = &mut lut.values;
    for c in 0..MAX_CHANNELS {
        let (low, high) = if half {
            (c, 15360 * MAX_CHANNELS + c)
        } else {
            (c, (length - 1) * MAX_CHANNELS + c)
        };
        let is_increasing = values[low] < values[high];
        if !half {
            let mut prev = values[c];
            let mut idx = c + MAX_CHANNELS;
            while idx < length * MAX_CHANNELS {
                if is_increasing != (values[idx] > prev) {
                    values[idx] = prev;
                } else {
                    prev = values[idx];
                }
                idx += MAX_CHANNELS;
            }
        } else {
            // Positive numbers.
            let start = c;
            let end = 31744 * MAX_CHANNELS;
            let mut prev = values[start];
            let mut idx = start + MAX_CHANNELS;
            while idx <= end {
                if is_increasing != (values[idx] > prev) {
                    values[idx] = prev;
                } else {
                    prev = values[idx];
                }
                idx += MAX_CHANNELS;
            }
            // Negative numbers.
            let is_increasing = !is_increasing;
            let start = 32768 * MAX_CHANNELS + c;
            let end = 64512 * MAX_CHANNELS;
            let mut prev = values[c];
            let mut idx = start;
            while idx <= end {
                if is_increasing != (values[idx] > prev) {
                    values[idx] = prev;
                } else {
                    prev = values[idx];
                }
                idx += MAX_CHANNELS;
            }
        }
    }
}

/// Port of `Array::adjustColorComponentNumber`: the number of components
/// to write (1 if all the channels are equal, NaN triplets are ignored).
fn lut1d_num_components(lut: &Lut1DData) -> usize {
    let v = &lut.values;
    for idx in 0..lut.length {
        let (r, g, b) = (v[idx * 3], v[idx * 3 + 1], v[idx * 3 + 2]);
        if r.is_nan() && g.is_nan() && b.is_nan() {
            continue;
        }
        if r != g || r != b {
            return 3;
        }
    }
    1
}

/// Build the process nodes of a group transform for writing.
pub(crate) fn build_write_ops(
    config: &Config,
    context: &Context,
    group: &GroupTransform,
) -> Result<Vec<OpData>> {
    let mut ops = Vec::new();
    collect_ops(
        config,
        context,
        &Transform::Group(group.clone()),
        TransformDirection::Forward,
        &mut ops,
    )?;
    // Finalize (see `OpRcPtrVec::finalize`): validate, prepare the 1D LUTs
    // for inversion and ensure Matrix & Range are forward.
    for op in &ops {
        op.validate()?;
    }
    for op in &mut ops {
        match op {
            OpData::Lut1D(l) if l.dir == TransformDirection::Inverse => flatten_inverse_lut1d(l),
            OpData::Matrix(m) if m.dir == TransformDirection::Inverse => *m = m.as_forward()?,
            OpData::Range(r) if r.dir == TransformDirection::Inverse => *r = r.as_forward()?,
            _ => {}
        }
    }
    Ok(ops)
}

/// A generated identifier for a process list without id (port of
/// `CacheIDHashUUID` applied to the ops cache id).
///
/// The hash is computed from a textual description of the process nodes.
fn generated_id(ops: &[OpData]) -> String {
    let desc = format!("{ops:?}");
    let hex = format!("{:x}", md5::compute(desc.as_bytes()));
    format!(
        "urn:uuid:{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// Write `group` as CLF or CTF (port of `LocalFileFormat::write`).
pub(crate) fn write_group(
    config: &Config,
    context: &Context,
    group: &GroupTransform,
    sub: SubFormat,
) -> Result<String> {
    let ops = build_write_ops(config, context, group)?;
    let mut transform = CtfReaderTransform::from_ops(ops, &group.metadata)?;
    if transform.id().is_empty() {
        let id = generated_id(&transform.ops);
        transform.set_id(&id);
    }
    write_transform(&transform, sub)
}

/// Write a process list (header included).
pub(crate) fn write_transform(transform: &CtfReaderTransform, sub: SubFormat) -> Result<String> {
    let mut fmt = XmlFormatter::new();
    fmt.stream()
        .push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    TransformWriter {
        fmt: &mut fmt,
        transform,
        sub,
    }
    .write()?;
    Ok(fmt.into_string())
}

// ---------------------------------------------------------------------------
// Versions.

/// Port of `GetOpMinimumVersion`.
fn op_minimum_version(op: &OpData) -> Result<CtfVersion> {
    Ok(match op {
        OpData::Cdl(_) => CTF_PROCESS_LIST_VERSION_1_7,
        OpData::ExposureContrast(ec) => {
            if ec.log_exposure_step != LOGEXPOSURESTEP_DEFAULT
                || ec.log_mid_gray != LOGMIDGRAY_DEFAULT
            {
                CTF_PROCESS_LIST_VERSION_2_0
            } else {
                CTF_PROCESS_LIST_VERSION_1_3
            }
        }
        OpData::FixedFunction(ff) => {
            let (major, minor) = ff.min_version_2x();
            CtfVersion::new(major, minor, 0)
        }
        OpData::GradingPrimary(_)
        | OpData::GradingRgbCurve(_)
        | OpData::GradingTone(_)
        | OpData::Log(_) => CTF_PROCESS_LIST_VERSION_2_0,
        OpData::GradingHueCurve(_) => CTF_PROCESS_LIST_VERSION_2_5,
        OpData::Gamma(g) => match g.style {
            GammaStyle::BasicFwd
            | GammaStyle::BasicRev
            | GammaStyle::MonCurveFwd
            | GammaStyle::MonCurveRev => {
                if g.is_alpha_identity() {
                    CTF_PROCESS_LIST_VERSION_1_3
                } else {
                    CTF_PROCESS_LIST_VERSION_1_5
                }
            }
            _ => CTF_PROCESS_LIST_VERSION_2_0,
        },
        OpData::Lut1D(l) => match l.dir {
            TransformDirection::Forward => {
                if l.hue_adjust != Lut1DHueAdjust::None {
                    CTF_PROCESS_LIST_VERSION_1_4
                } else {
                    CTF_PROCESS_LIST_VERSION_1_3
                }
            }
            TransformDirection::Inverse => {
                if l.hue_adjust != Lut1DHueAdjust::None || l.half_domain {
                    CTF_PROCESS_LIST_VERSION_1_6
                } else {
                    CTF_PROCESS_LIST_VERSION_1_3
                }
            }
        },
        OpData::Lut3D(l) => match l.dir {
            TransformDirection::Forward => CTF_PROCESS_LIST_VERSION_1_3,
            TransformDirection::Inverse => CTF_PROCESS_LIST_VERSION_1_6,
        },
        OpData::Matrix(_) | OpData::Range(_) => CTF_PROCESS_LIST_VERSION_1_3,
        OpData::Reference(_) => {
            return Err(Error::msg(
                "Reference ops should have been replaced by their content.",
            ));
        }
    })
}

/// Port of `GetMinimumVersion`.
fn minimum_version(transform: &CtfReaderTransform) -> Result<CtfVersion> {
    let mut min = CTF_PROCESS_LIST_VERSION_1_3;
    for op in &transform.ops {
        let v = op_minimum_version(op)?;
        if v > min {
            min = v;
        }
    }
    Ok(min)
}

// ---------------------------------------------------------------------------
// Values.

/// Port of `WriteValues`: write an array of values, `values_per_line` per
/// line, taking every `step` value, formatted according to `bit_depth`.
/// `is_double` selects the formatting used for double arrays (matrices).
fn write_values(
    fmt: &mut XmlFormatter,
    values: &[f64],
    values_per_line: usize,
    bit_depth: BitDepth,
    step: usize,
    is_double: bool,
) -> Result<()> {
    let (mut width, precision) = match bit_depth {
        BitDepth::UInt8 => (3, DEFAULT_PRECISION),
        BitDepth::UInt10 | BitDepth::UInt12 => (4, DEFAULT_PRECISION),
        BitDepth::UInt16 => (5, DEFAULT_PRECISION),
        BitDepth::F16 => (11, 5),
        BitDepth::F32 => {
            if is_double {
                (19, DOUBLE_PRECISION)
            } else {
                (11, 8)
            }
        }
        BitDepth::UInt14 | BitDepth::UInt32 => return Err(Error::msg("Unsupported bitdepth.")),
        BitDepth::Unknown => return Err(Error::msg("Unknown bitdepth.")),
    };
    let float_values = matches!(bit_depth, BitDepth::F16 | BitDepth::F32);
    let step = step.max(1);
    let per_line = values_per_line.max(1);
    let out = fmt.stream();
    let mut i = 0;
    while i < values.len() {
        let v = values[i];
        let s = if float_values {
            write_value(v, precision)
        } else {
            fmt_g(v, precision)
        };
        let s = pad_left(&s, width);
        // The stream width is reset after each output; OCIO then sets it to the
        // length of the value written.
        width = s.len();
        out.push_str(&s);
        if i % per_line == per_line - 1 {
            out.push('\n');
        } else {
            out.push(' ');
        }
        i += step;
    }
    Ok(())
}

/// Port of `BitDepthToCLFString`.
fn bit_depth_to_clf_string(bd: BitDepth) -> Result<&'static str> {
    Ok(match bd {
        BitDepth::UInt8 => "8i",
        BitDepth::UInt10 => "10i",
        BitDepth::UInt12 => "12i",
        BitDepth::UInt16 => "16i",
        BitDepth::F16 => "16f",
        BitDepth::F32 => "32f",
        _ => {
            return Err(Error::msg(
                "Bitdepth has not been validated before calling this.",
            ))
        }
    })
}

/// Port of `GetValidatedFileBitDepth`.
fn validated_file_bit_depth(bd: BitDepth, type_name: &str) -> Result<BitDepth> {
    match bd {
        BitDepth::Unknown => Ok(BitDepth::F32),
        BitDepth::UInt8
        | BitDepth::UInt10
        | BitDepth::UInt12
        | BitDepth::UInt16
        | BitDepth::F16
        | BitDepth::F32 => Ok(bd),
        _ => crate::bail!(
            "Op {}. Bit-depth: {} is not supported for writing to CLF/CTF.",
            type_name,
            bd as u32
        ),
    }
}

/// Port of `GetInputFileBD`.
fn input_file_bd(op: &OpData) -> Result<BitDepth> {
    match op {
        OpData::Matrix(m) => validated_file_bit_depth(m.file_in_bd, op.type_name()),
        OpData::Range(r) => validated_file_bit_depth(r.file_in_bd, op.type_name()),
        OpData::Lut1D(l) if l.dir == TransformDirection::Inverse => {
            validated_file_bit_depth(l.file_output_bd, op.type_name())
        }
        OpData::Lut3D(l) if l.dir == TransformDirection::Inverse => {
            validated_file_bit_depth(l.file_output_bd, op.type_name())
        }
        _ => Ok(BitDepth::F32),
    }
}

fn throw_write_op(type_name: &str) -> Error {
    Error::msg(format!(
        "Transform uses the '{type_name}' op which cannot be written as CLF.  Use CTF format or Bake the transform."
    ))
}

fn g15(v: f64) -> String {
    fmt_g(v, DOUBLE_PRECISION)
}

fn g15_3(v: &[f64; 3]) -> String {
    format!("{} {} {}", g15(v[0]), g15(v[1]), g15(v[2]))
}

// ---------------------------------------------------------------------------
// Writer.

struct TransformWriter<'a> {
    fmt: &'a mut XmlFormatter,
    transform: &'a CtfReaderTransform,
    sub: SubFormat,
}

impl TransformWriter<'_> {
    fn write(&mut self) -> Result<()> {
        let mut attributes = Attributes::new();
        let write_version = match self.sub {
            SubFormat::Clf => {
                attributes.push(attr(ATTR_COMP_CLF_VERSION, "3"));
                attributes.push(attr(ATTR_XMLNS, SMPTE_XMLNS_URL));
                CTF_PROCESS_LIST_VERSION_2_0
            }
            SubFormat::Ctf => {
                let v = minimum_version(self.transform)?;
                attributes.push(attr(ATTR_VERSION, &v.to_string()));
                v
            }
        };

        let meta = &self.transform.metadata;
        let id = self.transform.id();
        if id.is_empty() {
            return Err(Error::msg(
                "Internal error; at this point the transform should have an id",
            ));
        }
        attributes.push(attr(ATTR_ID, id));
        let name = self.transform.name();
        if !name.is_empty() {
            attributes.push(attr(ATTR_NAME, name));
        }
        let inverse_of = attribute_ic(meta, ATTR_INVERSE_OF);
        if !inverse_of.is_empty() {
            attributes.push(attr(ATTR_INVERSE_OF, inverse_of));
        }
        for (n, v) in &meta.attributes {
            if n.starts_with("xmlns:") && !v.is_empty() {
                attributes.push((n.clone(), v.clone()));
            }
        }

        self.fmt.write_start_tag(TAG_PROCESS_LIST, &attributes);
        self.fmt.increment_indent();
        let r = self.write_body(meta, write_version);
        self.fmt.decrement_indent();
        r?;
        self.fmt.write_end_tag(TAG_PROCESS_LIST);
        Ok(())
    }

    fn write_body(&mut self, meta: &FormatMetadata, version: CtfVersion) -> Result<()> {
        if let Some(id_elt) = children_ic(meta, METADATA_ID_ELEMENT).next() {
            let id_val = id_elt.element_value.as_str();
            if !id_val.is_empty() {
                if self.sub == SubFormat::Clf && !validate_smpte_id(id_val) {
                    crate::bail!("'{}' is not a SMPTE ST 2136-1 compliant Id value.", id_val);
                }
                self.fmt.write_content_tag(TAG_ID, &[], id_val);
            }
        }
        for name in [
            METADATA_DESCRIPTION,
            METADATA_INPUT_DESCRIPTOR,
            METADATA_OUTPUT_DESCRIPTOR,
        ] {
            for el in children_ic(meta, name) {
                self.fmt
                    .write_content_tag(&el.element_name, &el.attributes, &el.element_value);
            }
        }
        self.write_process_list_metadata(&self.transform.info_metadata);
        self.write_ops(version)
    }

    /// Port of `TransformWriter::writeProcessListMetadata`.
    fn write_process_list_metadata(&mut self, m: &FormatMetadata) {
        if m.children.is_empty() {
            if !m.attributes.is_empty() || !m.element_value.is_empty() {
                self.fmt
                    .write_content_tag(&m.element_name, &m.attributes, &m.element_value);
            }
        } else {
            self.fmt.write_start_tag(&m.element_name, &m.attributes);
            if !m.element_value.is_empty() {
                self.fmt.write_content(&m.element_value);
            }
            for c in &m.children {
                self.fmt.increment_indent();
                self.write_process_list_metadata(c);
                self.fmt.decrement_indent();
            }
            self.fmt.write_end_tag(&m.element_name);
        }
    }

    /// Port of `TransformWriter::writeOps`.
    fn write_ops(&mut self, version: CtfVersion) -> Result<()> {
        let is_clf = self.sub == SubFormat::Clf;
        let ops = &self.transform.ops;
        let mut num_saved = 0usize;
        if !ops.is_empty() {
            let mut in_bd = input_file_bd(&ops[0])?;
            for (i, op) in ops.iter().enumerate() {
                let mut out_bd = BitDepth::F32;
                if let Some(next) = ops.get(i + 1) {
                    out_bd = input_file_bd(next)?;
                }
                op.validate()?;
                num_saved += 1;

                match op {
                    OpData::Cdl(_) => {}
                    OpData::ExposureContrast(_) => {
                        if is_clf {
                            return Err(throw_write_op("ExposureContrast"));
                        }
                    }
                    OpData::FixedFunction(_) => {
                        if is_clf {
                            return Err(throw_write_op("FixedFunction"));
                        }
                    }
                    OpData::Gamma(g) => {
                        if is_clf && !g.is_alpha_identity() {
                            return Err(throw_write_op("Gamma with alpha component"));
                        }
                    }
                    OpData::GradingPrimary(_) => {
                        if is_clf {
                            return Err(throw_write_op("GradingPrimary"));
                        }
                    }
                    OpData::GradingRgbCurve(_) => {
                        if is_clf {
                            return Err(throw_write_op("GradingRGBCurve"));
                        }
                    }
                    OpData::GradingHueCurve(_) => {
                        if is_clf {
                            return Err(throw_write_op("GradingHueCurve"));
                        }
                    }
                    OpData::GradingTone(_) => {
                        if is_clf {
                            return Err(throw_write_op("GradingTone"));
                        }
                    }
                    OpData::Log(_) => {}
                    OpData::Lut1D(l) => {
                        if is_clf && l.dir != TransformDirection::Forward {
                            return Err(throw_write_op("InverseLUT1D"));
                        }
                        if l.dir == TransformDirection::Forward {
                            out_bd = validated_file_bit_depth(l.file_output_bd, op.type_name())?;
                        }
                    }
                    OpData::Lut3D(l) => {
                        if is_clf && l.dir != TransformDirection::Forward {
                            return Err(throw_write_op("InverseLUT3D"));
                        }
                        if l.dir == TransformDirection::Forward {
                            out_bd = validated_file_bit_depth(l.file_output_bd, op.type_name())?;
                        }
                    }
                    OpData::Matrix(m) => {
                        if is_clf && m.has_alpha() {
                            return Err(throw_write_op("Matrix with alpha component"));
                        }
                        out_bd = validated_file_bit_depth(m.file_out_bd, op.type_name())?;
                    }
                    OpData::Range(r) => {
                        out_bd = validated_file_bit_depth(r.file_out_bd, op.type_name())?;
                    }
                    OpData::Reference(_) => {
                        return Err(Error::msg(
                            "Reference ops should have been replaced by their content.",
                        ));
                    }
                }

                OpWriter {
                    fmt: self.fmt,
                    in_bd,
                    out_bd,
                    version: version.clone(),
                }
                .write(op)?;
                in_bd = out_bd;
            }
        }
        if num_saved == 0 {
            // When there are no ops, save an identity matrix.
            let mat = OpData::Matrix(MatrixData::default());
            OpWriter {
                fmt: self.fmt,
                in_bd: BitDepth::F32,
                out_bd: BitDepth::F32,
                version,
            }
            .write(&mat)?;
        }
        Ok(())
    }
}

struct OpWriter<'a> {
    fmt: &'a mut XmlFormatter,
    in_bd: BitDepth,
    out_bd: BitDepth,
    version: CtfVersion,
}

impl OpWriter<'_> {
    fn use_gamma_tag(&self) -> bool {
        self.version < CTF_PROCESS_LIST_VERSION_2_0
    }

    fn tag_name(&self, op: &OpData) -> &'static str {
        match op {
            OpData::Cdl(_) => TAG_CDL,
            OpData::ExposureContrast(_) => TAG_EXPOSURE_CONTRAST,
            OpData::FixedFunction(_) => TAG_FIXED_FUNCTION,
            OpData::Gamma(_) => {
                if self.use_gamma_tag() {
                    TAG_GAMMA
                } else {
                    TAG_EXPONENT
                }
            }
            OpData::GradingPrimary(_) => TAG_PRIMARY,
            OpData::GradingRgbCurve(_) => TAG_RGB_CURVE,
            OpData::GradingHueCurve(_) => TAG_HUE_CURVE,
            OpData::GradingTone(_) => TAG_TONE,
            OpData::Log(_) => TAG_LOG,
            OpData::Lut1D(l) => {
                if l.dir == TransformDirection::Forward {
                    TAG_LUT1D
                } else {
                    TAG_INVLUT1D
                }
            }
            OpData::Lut3D(l) => {
                if l.dir == TransformDirection::Forward {
                    TAG_LUT3D
                } else {
                    TAG_INVLUT3D
                }
            }
            OpData::Matrix(_) => TAG_MATRIX,
            OpData::Range(_) => TAG_RANGE,
            OpData::Reference(_) => TAG_REFERENCE,
        }
    }

    fn write(mut self, op: &OpData) -> Result<()> {
        let attributes = self.attributes(op)?;
        let tag = self.tag_name(op);
        self.fmt.write_start_tag(tag, &attributes);
        self.fmt.increment_indent();
        self.write_format_metadata(op);
        let r = self.write_content(op);
        self.fmt.decrement_indent();
        r?;
        self.fmt.write_end_tag(tag);
        Ok(())
    }

    fn write_descriptions(&mut self, meta: &FormatMetadata, name: &str, tag: &str) {
        for d in children_ic(meta, name) {
            self.fmt.write_content_tag(tag, &[], &d.element_value);
        }
    }

    fn write_format_metadata(&mut self, op: &OpData) {
        let meta = op.metadata();
        self.write_descriptions(meta, TAG_DESCRIPTION, TAG_DESCRIPTION);
        if let OpData::Cdl(_) = op {
            self.write_descriptions(meta, METADATA_INPUT_DESCRIPTION, METADATA_INPUT_DESCRIPTION);
            self.write_descriptions(
                meta,
                METADATA_VIEWING_DESCRIPTION,
                METADATA_VIEWING_DESCRIPTION,
            );
        }
    }

    fn attributes(&self, op: &OpData) -> Result<Attributes> {
        let mut attributes = Attributes::new();
        let meta = op.metadata();
        let id = attribute_ic(meta, METADATA_ID);
        if !id.is_empty() {
            attributes.push(attr(ATTR_ID, id));
        }
        let name = attribute_ic(meta, METADATA_NAME);
        if !name.is_empty() {
            attributes.push(attr(ATTR_NAME, name));
        }
        attributes.push(attr(ATTR_BITDEPTH_IN, bit_depth_to_clf_string(self.in_bd)?));
        attributes.push(attr(
            ATTR_BITDEPTH_OUT,
            bit_depth_to_clf_string(self.out_bd)?,
        ));

        match op {
            OpData::Cdl(c) => attributes.push(attr(ATTR_STYLE, c.style.name())),
            OpData::ExposureContrast(ec) => attributes.push(attr(ATTR_STYLE, ec.style.name())),
            OpData::FixedFunction(ff) => {
                attributes.push(attr(ATTR_STYLE, ff_style_to_name(ff.style, ff.dir, false)?));
                if !ff.params.is_empty() {
                    let p: Vec<String> = ff
                        .params
                        .iter()
                        .map(|&v| write_value(v, DOUBLE_PRECISION))
                        .collect();
                    attributes.push(attr(ATTR_PARAMS, &p.join(" ")));
                }
            }
            OpData::Gamma(g) => attributes.push(attr(ATTR_STYLE, g.style.name())),
            OpData::GradingPrimary(g) => {
                attributes.push(attr(ATTR_STYLE, grading_style_to_name(g.style, g.dir)))
            }
            OpData::GradingRgbCurve(g) => {
                attributes.push(attr(ATTR_STYLE, grading_style_to_name(g.style, g.dir)));
                if g.bypass_lin_to_log {
                    attributes.push(attr(ATTR_BYPASS_LIN_TO_LOG, "true"));
                }
            }
            OpData::GradingHueCurve(g) => {
                attributes.push(attr(ATTR_STYLE, grading_style_to_name(g.style, g.dir)));
                if g.rgb_to_hsy == HsyTransformStyle::None {
                    attributes.push(attr(ATTR_RGB_TO_HSY, "none"));
                }
            }
            OpData::GradingTone(g) => {
                attributes.push(attr(ATTR_STYLE, grading_style_to_name(g.style, g.dir)))
            }
            OpData::Log(l) => {
                let fwd = l.dir == TransformDirection::Forward;
                let style = if l.is_log2() {
                    if fwd {
                        "log2"
                    } else {
                        "antiLog2"
                    }
                } else if l.is_log10() {
                    if fwd {
                        "log10"
                    } else {
                        "antiLog10"
                    }
                } else if l.is_camera() {
                    if fwd {
                        "cameraLinToLog"
                    } else {
                        "cameraLogToLin"
                    }
                } else if fwd {
                    "linToLog"
                } else {
                    "logToLin"
                };
                attributes.push(attr(ATTR_STYLE, style));
            }
            OpData::Lut1D(l) => {
                if let Some(n) = interpolation_1d_name(l.interpolation) {
                    attributes.push(attr(ATTR_INTERPOLATION, n));
                }
                if l.half_domain {
                    attributes.push(attr(ATTR_HALF_DOMAIN, "true"));
                }
                if l.raw_halfs {
                    attributes.push(attr(ATTR_RAW_HALFS, "true"));
                }
                if l.hue_adjust == Lut1DHueAdjust::Dw3 {
                    attributes.push(attr(ATTR_HUE_ADJUST, "dw3"));
                }
            }
            OpData::Lut3D(l) => {
                if let Some(n) = interpolation_3d_name(l.interpolation) {
                    attributes.push(attr(ATTR_INTERPOLATION, n));
                }
            }
            OpData::Matrix(_) | OpData::Range(_) | OpData::Reference(_) => {}
        }
        Ok(attributes)
    }

    fn write_content(&mut self, op: &OpData) -> Result<()> {
        match op {
            OpData::Cdl(c) => self.write_cdl(c),
            OpData::ExposureContrast(ec) => self.write_ec(ec),
            OpData::FixedFunction(_) => {}
            OpData::Gamma(g) => self.write_gamma(g),
            OpData::GradingPrimary(g) => self.write_primary(g),
            OpData::GradingRgbCurve(g) => self.write_rgb_curve(g),
            OpData::GradingHueCurve(g) => self.write_hue_curve(g),
            OpData::GradingTone(g) => self.write_tone(g),
            OpData::Log(l) => self.write_log(l),
            OpData::Lut1D(l) => return self.write_lut1d(l),
            OpData::Lut3D(l) => return self.write_lut3d(l),
            OpData::Matrix(m) => return self.write_matrix(m),
            OpData::Range(r) => return self.write_range(r),
            OpData::Reference(_) => {}
        }
        Ok(())
    }

    fn write_cdl(&mut self, c: &CdlData) {
        let meta = &c.metadata;
        self.fmt.write_start_tag(TAG_SOPNODE, &[]);
        self.fmt.increment_indent();
        self.write_descriptions(meta, METADATA_SOP_DESCRIPTION, TAG_DESCRIPTION);
        self.fmt.write_content_tag(TAG_SLOPE, &[], &g15_3(&c.slope));
        self.fmt
            .write_content_tag(TAG_OFFSET, &[], &g15_3(&c.offset));
        self.fmt.write_content_tag(TAG_POWER, &[], &g15_3(&c.power));
        self.fmt.decrement_indent();
        self.fmt.write_end_tag(TAG_SOPNODE);

        self.fmt.write_start_tag(TAG_SATNODE, &[]);
        self.fmt.increment_indent();
        self.write_descriptions(meta, METADATA_SAT_DESCRIPTION, TAG_DESCRIPTION);
        self.fmt.write_content_tag(TAG_SATURATION, &[], &g15(c.sat));
        self.fmt.decrement_indent();
        self.fmt.write_end_tag(TAG_SATNODE);
    }

    fn write_ec(&mut self, ec: &EcData) {
        let mut attributes = vec![
            attr(ATTR_EXPOSURE, &write_value(ec.exposure, DOUBLE_PRECISION)),
            attr(ATTR_CONTRAST, &write_value(ec.contrast, DOUBLE_PRECISION)),
            attr(ATTR_GAMMA, &write_value(ec.gamma, DOUBLE_PRECISION)),
            attr(ATTR_PIVOT, &write_value(ec.pivot, DOUBLE_PRECISION)),
        ];
        if ec.log_exposure_step != LOGEXPOSURESTEP_DEFAULT {
            attributes.push(attr(
                ATTR_LOGEXPOSURESTEP,
                &write_value(ec.log_exposure_step, DOUBLE_PRECISION),
            ));
        }
        if ec.log_mid_gray != LOGMIDGRAY_DEFAULT {
            attributes.push(attr(
                ATTR_LOGMIDGRAY,
                &write_value(ec.log_mid_gray, DOUBLE_PRECISION),
            ));
        }
        self.fmt.write_empty_tag(TAG_EC_PARAMS, &attributes);
        for (dynamic, name) in [
            (ec.exposure_dynamic, TAG_DYN_PROP_EXPOSURE),
            (ec.contrast_dynamic, TAG_DYN_PROP_CONTRAST),
            (ec.gamma_dynamic, TAG_DYN_PROP_GAMMA),
        ] {
            if dynamic {
                self.write_dynamic(name);
            }
        }
    }

    fn write_dynamic(&mut self, name: &str) {
        self.fmt
            .write_empty_tag(TAG_DYNAMIC_PARAMETER, &[attr(ATTR_PARAM, name)]);
    }

    fn gamma_params(
        attributes: &mut Attributes,
        params: &[f64],
        style: GammaStyle,
        use_gamma: bool,
    ) {
        let p0 = params.first().copied().unwrap_or(1.0);
        attributes.push(attr(
            if use_gamma { ATTR_GAMMA } else { ATTR_EXPONENT },
            &g15(p0),
        ));
        if style.is_moncurve() {
            let p1 = params.get(1).copied().unwrap_or(0.0);
            attributes.push(attr(ATTR_OFFSET, &g15(p1)));
        }
    }

    fn write_gamma(&mut self, g: &GammaData) {
        let use_gamma = self.use_gamma_tag();
        let tag = if use_gamma {
            TAG_GAMMA_PARAMS
        } else {
            TAG_EXPONENT_PARAMS
        };
        if g.is_non_channel_dependent() {
            let mut attributes = Attributes::new();
            Self::gamma_params(&mut attributes, &g.params[0], g.style, use_gamma);
            self.fmt.write_empty_tag(tag, &attributes);
        } else {
            let alpha = !g.is_alpha_identity();
            for (c, chan) in ["R", "G", "B", "A"].iter().enumerate() {
                if c == 3 && !alpha {
                    break;
                }
                let mut attributes = vec![attr(ATTR_CHAN, chan)];
                Self::gamma_params(&mut attributes, &g.params[c], g.style, use_gamma);
                self.fmt.write_empty_tag(tag, &attributes);
            }
        }
    }

    fn write_rgbm(&mut self, tag: &str, default: &GradingRgbm, val: &GradingRgbm) {
        if val != default {
            let attributes = vec![
                attr(
                    ATTR_RGB,
                    &format!("{} {} {}", g15(val.red), g15(val.green), g15(val.blue)),
                ),
                attr(ATTR_MASTER, &g15(val.master)),
            ];
            self.fmt.write_empty_tag(tag, &attributes);
        }
    }

    fn write_scalar(&mut self, tag: &str, default: f64, val: f64) {
        if val != default {
            self.fmt
                .write_empty_tag(tag, &[attr(ATTR_MASTER, &g15(val))]);
        }
    }

    fn add_if_not_default(attributes: &mut Attributes, name: &str, default: f64, val: f64) {
        if val != default {
            attributes.push(attr(name, &g15(val)));
        }
    }

    fn write_primary(&mut self, g: &GradingPrimaryData) {
        let vals = &g.value;
        let def = GradingPrimary::new(g.style);
        match g.style {
            GradingStyle::Log => {
                self.write_rgbm(TAG_PRIMARY_BRIGHTNESS, &def.brightness, &vals.brightness);
                self.write_rgbm(TAG_PRIMARY_CONTRAST, &def.contrast, &vals.contrast);
                self.write_rgbm(TAG_PRIMARY_GAMMA, &def.gamma, &vals.gamma);
                self.write_scalar(TAG_PRIMARY_SATURATION, def.saturation, vals.saturation);
                let mut attributes = Attributes::new();
                if def.contrast != vals.contrast {
                    attributes.push(attr(ATTR_PRIMARY_CONTRAST, &g15(vals.pivot)));
                } else {
                    Self::add_if_not_default(
                        &mut attributes,
                        ATTR_PRIMARY_CONTRAST,
                        def.pivot,
                        vals.pivot,
                    );
                }
                Self::add_if_not_default(
                    &mut attributes,
                    ATTR_PRIMARY_BLACK,
                    def.pivot_black,
                    vals.pivot_black,
                );
                Self::add_if_not_default(
                    &mut attributes,
                    ATTR_PRIMARY_WHITE,
                    def.pivot_white,
                    vals.pivot_white,
                );
                if !attributes.is_empty() {
                    self.fmt.write_empty_tag(TAG_PRIMARY_PIVOT, &attributes);
                }
            }
            GradingStyle::Lin => {
                self.write_rgbm(TAG_PRIMARY_OFFSET, &def.offset, &vals.offset);
                self.write_rgbm(TAG_PRIMARY_EXPOSURE, &def.exposure, &vals.exposure);
                self.write_rgbm(TAG_PRIMARY_CONTRAST, &def.contrast, &vals.contrast);
                self.write_scalar(TAG_PRIMARY_SATURATION, def.saturation, vals.saturation);
                let mut attributes = Attributes::new();
                if def.contrast != vals.contrast {
                    attributes.push(attr(ATTR_PRIMARY_CONTRAST, &g15(vals.pivot)));
                } else {
                    Self::add_if_not_default(
                        &mut attributes,
                        ATTR_PRIMARY_CONTRAST,
                        def.pivot,
                        vals.pivot,
                    );
                }
                if !attributes.is_empty() {
                    self.fmt.write_empty_tag(TAG_PRIMARY_PIVOT, &attributes);
                }
            }
            GradingStyle::Video => {
                self.write_rgbm(TAG_PRIMARY_LIFT, &def.lift, &vals.lift);
                self.write_rgbm(TAG_PRIMARY_GAMMA, &def.gamma, &vals.gamma);
                self.write_rgbm(TAG_PRIMARY_GAIN, &def.gain, &vals.gain);
                self.write_rgbm(TAG_PRIMARY_OFFSET, &def.offset, &vals.offset);
                self.write_scalar(TAG_PRIMARY_SATURATION, def.saturation, vals.saturation);
                let mut attributes = Attributes::new();
                Self::add_if_not_default(
                    &mut attributes,
                    ATTR_PRIMARY_BLACK,
                    def.pivot_black,
                    vals.pivot_black,
                );
                Self::add_if_not_default(
                    &mut attributes,
                    ATTR_PRIMARY_WHITE,
                    def.pivot_white,
                    vals.pivot_white,
                );
                if !attributes.is_empty() {
                    self.fmt.write_empty_tag(TAG_PRIMARY_PIVOT, &attributes);
                }
            }
        }
        // Clamp.
        let def = GradingPrimary::new(GradingStyle::Log);
        let mut attributes = Attributes::new();
        Self::add_if_not_default(
            &mut attributes,
            ATTR_PRIMARY_BLACK,
            def.clamp_black,
            vals.clamp_black,
        );
        Self::add_if_not_default(
            &mut attributes,
            ATTR_PRIMARY_WHITE,
            def.clamp_white,
            vals.clamp_white,
        );
        if !attributes.is_empty() {
            self.fmt.write_empty_tag(TAG_PRIMARY_CLAMP, &attributes);
        }
        if g.dynamic {
            self.write_dynamic(TAG_DYN_PROP_PRIMARY);
        }
    }

    fn write_curve(&mut self, tag: &str, curve: &GradingBSplineCurve) {
        self.fmt.write_start_tag(tag, &[]);
        self.fmt.increment_indent();
        self.fmt.write_start_tag(TAG_CURVE_CTRL_PNTS, &[]);
        self.fmt.increment_indent();
        for p in &curve.control_points {
            let s = format!(
                "{} {}",
                pad_left(&fmt_g(f64::from(p.x), 8), 11),
                fmt_g(f64::from(p.y), 8)
            );
            self.fmt.write_content(&s);
        }
        self.fmt.decrement_indent();
        self.fmt.write_end_tag(TAG_CURVE_CTRL_PNTS);
        if !curve.slopes_are_default() {
            self.fmt.write_start_tag(TAG_CURVE_SLOPES, &[]);
            self.fmt.increment_indent();
            let mut s = String::new();
            for i in 0..curve.num_control_points() {
                let v = fmt_g(f64::from(curve.slope(i)), 8);
                if i == 0 {
                    s.push_str(&pad_left(&v, 11));
                } else {
                    s.push_str(&v);
                }
                s.push(' ');
            }
            self.fmt.write_content(&s);
            self.fmt.decrement_indent();
            self.fmt.write_end_tag(TAG_CURVE_SLOPES);
        }
        self.fmt.decrement_indent();
        self.fmt.write_end_tag(tag);
    }

    fn write_rgb_curve(&mut self, g: &GradingRgbCurveData) {
        let def = default_rgb_curve(g.style);
        for (c, tag) in TAG_RGB_CURVE_NAMES.iter().enumerate() {
            let curve = &g.value.curves[c];
            if *curve != def || !curve.slopes_are_default() {
                self.write_curve(tag, curve);
            }
        }
        if g.dynamic {
            self.write_dynamic(TAG_DYN_PROP_RGBCURVE);
        }
    }

    fn write_hue_curve(&mut self, g: &GradingHueCurveData) {
        for (c, tag) in TAG_HUE_CURVE_NAMES.iter().enumerate() {
            let def = default_hue_curve(HUE_CURVE_TYPES[c], g.style);
            let curve = &g.value.curves[c];
            if *curve != def || !curve.slopes_are_default() {
                self.write_curve(tag, curve);
            }
        }
        if g.dynamic {
            self.write_dynamic(TAG_DYN_PROP_HUECURVE);
        }
    }

    fn write_rgbmsw(
        &mut self,
        tag: &str,
        def: &GradingRgbmsw,
        val: &GradingRgbmsw,
        center: bool,
        pivot: bool,
    ) {
        if val != def {
            let attributes = vec![
                attr(
                    ATTR_RGB,
                    &format!("{} {} {}", g15(val.red), g15(val.green), g15(val.blue)),
                ),
                attr(ATTR_MASTER, &g15(val.master)),
                attr(
                    if center { ATTR_CENTER } else { ATTR_START },
                    &g15(val.start),
                ),
                attr(if pivot { ATTR_PIVOT } else { ATTR_WIDTH }, &g15(val.width)),
            ];
            self.fmt.write_empty_tag(tag, &attributes);
        }
    }

    fn write_tone(&mut self, g: &GradingToneData) {
        let v = &g.value;
        let def = GradingTone::new(g.style);
        self.write_rgbmsw(TAG_TONE_BLACKS, &def.blacks, &v.blacks, false, false);
        self.write_rgbmsw(TAG_TONE_SHADOWS, &def.shadows, &v.shadows, false, true);
        self.write_rgbmsw(TAG_TONE_MIDTONES, &def.midtones, &v.midtones, true, false);
        self.write_rgbmsw(
            TAG_TONE_HIGHLIGHTS,
            &def.highlights,
            &v.highlights,
            false,
            true,
        );
        self.write_rgbmsw(TAG_TONE_WHITES, &def.whites, &v.whites, false, false);
        self.write_scalar(TAG_TONE_SCONTRAST, def.s_contrast, v.s_contrast);
        if g.dynamic {
            self.write_dynamic(TAG_DYN_PROP_TONE);
        }
    }

    fn log_params(attributes: &mut Attributes, params: &[f64], base: f64) {
        let p = |i: usize| params.get(i).copied().unwrap_or(0.0);
        attributes.push(attr(ATTR_BASE, &g15(base)));
        attributes.push(attr(ATTR_LINSIDESLOPE, &g15(p(LIN_SIDE_SLOPE))));
        attributes.push(attr(ATTR_LINSIDEOFFSET, &g15(p(LIN_SIDE_OFFSET))));
        attributes.push(attr(ATTR_LOGSIDESLOPE, &g15(p(LOG_SIDE_SLOPE))));
        attributes.push(attr(ATTR_LOGSIDEOFFSET, &g15(p(LOG_SIDE_OFFSET))));
        if params.len() > 4 {
            attributes.push(attr(ATTR_LINSIDEBREAK, &g15(p(LIN_SIDE_BREAK))));
        }
        if params.len() > 5 {
            attributes.push(attr(ATTR_LINEARSLOPE, &g15(p(LINEAR_SLOPE))));
        }
    }

    fn write_log(&mut self, l: &LogData) {
        if l.is_log2() || l.is_log10() {
            return;
        }
        if l.all_components_equal() {
            let mut attributes = Attributes::new();
            Self::log_params(&mut attributes, &l.params[0], l.base);
            self.fmt.write_empty_tag(TAG_LOG_PARAMS, &attributes);
        } else {
            for (c, chan) in ["R", "G", "B"].iter().enumerate() {
                let mut attributes = vec![attr(ATTR_CHAN, chan)];
                Self::log_params(&mut attributes, &l.params[c], l.base);
                self.fmt.write_empty_tag(TAG_LOG_PARAMS, &attributes);
            }
        }
    }

    fn write_lut1d(&mut self, l: &Lut1DData) -> Result<()> {
        let num_components = lut1d_num_components(l);
        let dim = format!("{} {}", l.length, num_components);
        self.fmt
            .write_start_tag(TAG_ARRAY, &[attr(ATTR_DIMENSION, &dim)]);
        let array_bd = if l.dir == TransformDirection::Inverse {
            self.in_bd
        } else {
            self.out_bd
        };
        let scale = array_bd.max_value() as f32;
        let step = if num_components == 1 { 3 } else { 1 };
        let n = l.length * 3;
        if l.raw_halfs {
            let values: Vec<f64> = l.values[..n.min(l.values.len())]
                .iter()
                .map(|&v| f64::from(half::f16::from_f32(v * scale).to_bits()))
                .collect();
            write_values(
                self.fmt,
                &values,
                num_components,
                BitDepth::UInt16,
                step,
                false,
            )?;
        } else {
            let values: Vec<f64> = l.values[..n.min(l.values.len())]
                .iter()
                .map(|&v| f64::from(v * scale))
                .collect();
            write_values(self.fmt, &values, num_components, array_bd, step, false)?;
        }
        self.fmt.write_end_tag(TAG_ARRAY);
        Ok(())
    }

    fn write_lut3d(&mut self, l: &Lut3DData) -> Result<()> {
        let g = l.grid_size;
        let dim = format!("{g} {g} {g} 3");
        self.fmt
            .write_start_tag(TAG_ARRAY, &[attr(ATTR_DIMENSION, &dim)]);
        let array_bd = if l.dir == TransformDirection::Inverse {
            self.in_bd
        } else {
            self.out_bd
        };
        let scale = array_bd.max_value() as f32;
        let values: Vec<f64> = l.values.iter().map(|&v| f64::from(v * scale)).collect();
        write_values(self.fmt, &values, 3, array_bd, 1, false)?;
        self.fmt.write_end_tag(TAG_ARRAY);
        Ok(())
    }

    fn write_matrix(&mut self, m: &MatrixData) -> Result<()> {
        let save_dim3 = self.version < CTF_PROCESS_LIST_VERSION_2_0;
        let m = m.as_forward()?;
        let (alpha, offsets) = (m.has_alpha(), m.has_offsets());
        let dim = match (alpha, offsets) {
            (true, true) => {
                if save_dim3 {
                    "4 5 4"
                } else {
                    "4 5"
                }
            }
            (true, false) => {
                if save_dim3 {
                    "4 4 4"
                } else {
                    "4 4"
                }
            }
            (false, true) => {
                if save_dim3 {
                    "3 4 3"
                } else {
                    "3 4"
                }
            }
            (false, false) => {
                if save_dim3 {
                    "3 3 3"
                } else {
                    "3 3"
                }
            }
        };
        self.fmt
            .write_start_tag(TAG_ARRAY, &[attr(ATTR_DIMENSION, dim)]);
        let out_scale = self.out_bd.max_value();
        let in_out_scale = out_scale / self.in_bd.max_value();
        let v = &m.matrix;
        let o = &m.offsets;
        let (rows, cols) = if alpha { (4, 4) } else { (3, 3) };
        let mut values = Vec::with_capacity(20);
        for r in 0..rows {
            for c in 0..cols {
                values.push(v[r * 4 + c] * in_out_scale);
            }
            if offsets {
                values.push(o[r] * out_scale);
            }
        }
        let per_line = cols + usize::from(offsets);
        write_values(self.fmt, &values, per_line, BitDepth::F32, 1, true)?;
        self.fmt.write_end_tag(TAG_ARRAY);
        Ok(())
    }

    fn write_range(&mut self, r: &RangeData) -> Result<()> {
        let r = r.as_forward()?;
        let out_scale = self.out_bd.max_value();
        let in_scale = self.in_bd.max_value();
        let tag = |fmt: &mut XmlFormatter, t: &str, v: f64| {
            fmt.write_content_tag(t, &[], &format!(" {} ", g15(v)));
        };
        if !r.min_is_empty() {
            tag(self.fmt, TAG_MIN_IN_VALUE, r.min_in * in_scale);
        }
        if !r.max_is_empty() {
            tag(self.fmt, TAG_MAX_IN_VALUE, r.max_in * in_scale);
        }
        if !r.min_is_empty() {
            tag(self.fmt, TAG_MIN_OUT_VALUE, r.min_out * out_scale);
        }
        if !r.max_is_empty() {
            tag(self.fmt, TAG_MAX_OUT_VALUE, r.max_out * out_scale);
        }
        Ok(())
    }
}

/// Hue curve types in file order.
const HUE_CURVE_TYPES: [HueCurveType; 8] = [
    HueCurveType::HueHue,
    HueCurveType::HueSat,
    HueCurveType::HueLum,
    HueCurveType::LumSat,
    HueCurveType::SatSat,
    HueCurveType::LumLum,
    HueCurveType::SatLum,
    HueCurveType::HueFx,
];
