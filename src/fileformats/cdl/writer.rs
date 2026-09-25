//! Writers of the ASC CDL XML files (port of `fileformats/cdl/CDLWriter.cpp`
//! and of the `write` methods of the CC, CCC and CDL formats).

use super::parser::{
    CDL_TAG_COLOR_CORRECTION_COLLECTION, CDL_TAG_COLOR_DECISION, CDL_TAG_COLOR_DECISION_LIST,
};
use crate::fileformats::ctf::xml::*;
use crate::format_metadata::FormatMetadata;
use crate::transforms::{
    CdlTransform, METADATA_INPUT_DESCRIPTION, METADATA_SAT_DESCRIPTION, METADATA_SOP_DESCRIPTION,
    METADATA_VIEWING_DESCRIPTION,
};
use crate::types::{METADATA_DESCRIPTION, METADATA_ID, METADATA_NAME};

/// Precision of the double values (`DOUBLE_DECIMALS` of `ParseUtils`).
const DOUBLE_DECIMALS: usize = 16;

/// The descriptive elements of a metadata, per kind (port of
/// `ExtractCDLMetadata`). As in OCIO, the values are escaped here (and
/// escaped again when written).
#[derive(Debug, Default)]
struct CdlDescriptions {
    main: Vec<String>,
    input: Vec<String>,
    viewing: Vec<String>,
    sop: Vec<String>,
    sat: Vec<String>,
}

fn extract_cdl_metadata(metadata: &FormatMetadata) -> CdlDescriptions {
    let mut d = CdlDescriptions::default();
    for elt in &metadata.children {
        let name = elt.element_name.as_str();
        let value = escape_xml(&elt.element_value);
        if eq_ic(name, METADATA_DESCRIPTION) {
            d.main.push(value);
        } else if eq_ic(name, METADATA_INPUT_DESCRIPTION) {
            d.input.push(value);
        } else if eq_ic(name, METADATA_VIEWING_DESCRIPTION) {
            d.viewing.push(value);
        } else if eq_ic(name, METADATA_SOP_DESCRIPTION) {
            d.sop.push(value);
        } else if eq_ic(name, METADATA_SAT_DESCRIPTION) {
            d.sat.push(value);
        }
    }
    d
}

/// Port of `WriteStrings`.
fn write_strings(fmt: &mut XmlFormatter, tag: &str, strings: &[String]) {
    for s in strings {
        fmt.write_content_tag(tag, &[], s);
    }
}

fn double_vec_to_string(v: &[f64; 3]) -> String {
    v.iter()
        .map(|&x| fmt_g(x, DOUBLE_DECIMALS))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Port of `Write(XmlFormatter &, const ConstCDLTransformRcPtr &)`: write a
/// ColorCorrection element.
fn write_color_correction(fmt: &mut XmlFormatter, cdl: &CdlTransform) {
    let metadata = &cdl.metadata;
    let mut attributes = Attributes::new();
    let id = metadata.attribute_value(METADATA_ID);
    if !id.is_empty() {
        attributes.push(attr(ATTR_ID, id));
    }
    let name = metadata.attribute_value(METADATA_NAME);
    if !name.is_empty() {
        attributes.push(attr(ATTR_NAME, name));
    }

    fmt.write_start_tag(CDL_TAG_COLOR_CORRECTION, &attributes);
    fmt.increment_indent();
    let d = extract_cdl_metadata(metadata);
    write_strings(fmt, TAG_DESCRIPTION, &d.main);
    write_strings(fmt, METADATA_INPUT_DESCRIPTION, &d.input);
    write_strings(fmt, METADATA_VIEWING_DESCRIPTION, &d.viewing);

    fmt.write_start_tag(TAG_SOPNODE, &[]);
    fmt.increment_indent();
    write_strings(fmt, TAG_DESCRIPTION, &d.sop);
    fmt.write_content_tag(TAG_SLOPE, &[], &double_vec_to_string(&cdl.slope));
    fmt.write_content_tag(TAG_OFFSET, &[], &double_vec_to_string(&cdl.offset));
    fmt.write_content_tag(TAG_POWER, &[], &double_vec_to_string(&cdl.power));
    fmt.decrement_indent();
    fmt.write_end_tag(TAG_SOPNODE);

    fmt.write_start_tag(TAG_SATNODE, &[]);
    fmt.increment_indent();
    write_strings(fmt, TAG_DESCRIPTION, &d.sat);
    fmt.write_content_tag(TAG_SATURATION, &[], &fmt_g(cdl.sat, DOUBLE_DECIMALS));
    fmt.decrement_indent();
    fmt.write_end_tag(TAG_SATNODE);

    fmt.decrement_indent();
    fmt.write_end_tag(CDL_TAG_COLOR_CORRECTION);
}

/// Write a `.cc` file.
pub(crate) fn write_cc(cdl: &CdlTransform) -> String {
    let mut fmt = XmlFormatter::new();
    write_color_correction(&mut fmt, cdl);
    fmt.into_string()
}

fn write_collection(
    root: &str,
    metadata: &FormatMetadata,
    cdls: &[&CdlTransform],
    decision: bool,
) -> String {
    let mut fmt = XmlFormatter::new();
    fmt.write_start_tag(root, &[attr(ATTR_XMLNS, "urn:ASC:CDL:v1.01")]);
    fmt.increment_indent();
    let d = extract_cdl_metadata(metadata);
    write_strings(&mut fmt, TAG_DESCRIPTION, &d.main);
    write_strings(&mut fmt, METADATA_INPUT_DESCRIPTION, &d.input);
    write_strings(&mut fmt, METADATA_VIEWING_DESCRIPTION, &d.viewing);
    for cdl in cdls {
        if decision {
            fmt.write_start_tag(CDL_TAG_COLOR_DECISION, &[]);
            fmt.increment_indent();
            write_color_correction(&mut fmt, cdl);
            fmt.decrement_indent();
            fmt.write_end_tag(CDL_TAG_COLOR_DECISION);
        } else {
            write_color_correction(&mut fmt, cdl);
        }
    }
    fmt.decrement_indent();
    fmt.write_end_tag(root);
    fmt.into_string()
}

/// Write a `.ccc` file.
pub(crate) fn write_ccc(metadata: &FormatMetadata, cdls: &[&CdlTransform]) -> String {
    write_collection(CDL_TAG_COLOR_CORRECTION_COLLECTION, metadata, cdls, false)
}

/// Write a `.cdl` file.
pub(crate) fn write_cdl(metadata: &FormatMetadata, cdls: &[&CdlTransform]) -> String {
    write_collection(CDL_TAG_COLOR_DECISION_LIST, metadata, cdls, true)
}
