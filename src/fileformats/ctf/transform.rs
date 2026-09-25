//! CTF versions and the in-memory content of a CLF/CTF file (port of
//! `CTFVersion` and `CTFReaderTransform` from `fileformats/ctf/CTFTransform.*`).

use super::opdata::OpData;
use crate::error::{Error, Result};
use crate::format_metadata::FormatMetadata;
use crate::types::*;
use std::cmp::Ordering;
use std::fmt;

/// SMPTE ST 2136-1 namespace, also used as a version.
pub(crate) const SMPTE_XMLNS_URL: &str = "http://www.smpte-ra.org/ns/2136-1/2024";
/// Short SMPTE version string accepted by `compCLFversion`.
pub(crate) const SMPTE_CLF_VERSION: &str = "ST2136-1:2024";

/// `inverseOf` attribute.
pub(crate) const ATTR_INVERSE_OF: &str = "inverseOf";

/// Accepted version string formats (bit flags).
pub(crate) mod version_format {
    /// Numeric versions are always accepted.
    pub const NUMERIC: u32 = 0;
    /// The SMPTE namespace URL is accepted.
    pub const SMPTE_XMLNS: u32 = 1 << 1;
    /// The short SMPTE version is accepted.
    pub const SMPTE_CLF: u32 = 1 << 2;
}

/// A CTF / CLF version (`MAJOR[.MINOR[.REVISION]]` or a SMPTE version
/// string, regarded as version 3).
#[derive(Debug, Clone, Default)]
pub(crate) struct CtfVersion {
    major: u32,
    minor: u32,
    revision: u32,
    version_string: String,
}

impl CtfVersion {
    /// Numeric version.
    pub(crate) const fn new(major: u32, minor: u32, revision: u32) -> Self {
        Self {
            major,
            minor,
            revision,
            version_string: String::new(),
        }
    }

    /// Parse a version string (port of the `CTFVersion` string constructor).
    pub(crate) fn parse(s: &str, accepted: u32) -> Result<Self> {
        if accepted & (version_format::SMPTE_XMLNS | version_format::SMPTE_CLF) != 0 {
            let mut res = false;
            if accepted & version_format::SMPTE_XMLNS != 0 {
                res = s.eq_ignore_ascii_case(SMPTE_XMLNS_URL);
            }
            if !res && accepted & version_format::SMPTE_CLF != 0 {
                res = s.eq_ignore_ascii_case(SMPTE_CLF_VERSION);
            }
            if res {
                return Ok(Self {
                    major: 3,
                    minor: 0,
                    revision: 0,
                    version_string: s.to_string(),
                });
            }
        }

        let b = s.as_bytes();
        let mut num_dot = 0u32;
        let mut num_int = 0u32;
        let mut can_be_dot = false;
        let mut i = 0;
        while i < b.len() {
            if b[i].is_ascii_digit() {
                num_int = num_dot + 1;
                can_be_dot = true;
                i += 1;
            } else if b[i] == b'.' && can_be_dot {
                can_be_dot = false;
                num_dot += 1;
                i += 1;
            } else {
                break;
            }
        }
        if s.is_empty() || i != b.len() || num_int == 0 || num_int > 3 || num_int == num_dot {
            let mut msg = format!("'{s}' is not a valid version. Expecting ");
            if accepted & version_format::SMPTE_CLF != 0 {
                msg.push_str(&format!("'{SMPTE_CLF_VERSION}' or "));
            }
            if accepted & version_format::SMPTE_XMLNS != 0 {
                msg.push_str(&format!("'{SMPTE_XMLNS_URL}' or "));
            }
            msg.push_str("MAJOR[.MINOR[.REVISION]] ");
            return Err(Error::msg(msg));
        }
        // Equivalent of sscanf("%d.%d.%d").
        let mut parts = s.split('.').map(|p| p.parse::<u32>().unwrap_or(0));
        let major = parts.next().unwrap_or(0);
        let minor = parts.next().unwrap_or(0);
        let revision = parts.next().unwrap_or(0);
        Ok(Self {
            major,
            minor,
            revision,
            version_string: String::new(),
        })
    }

    /// Parse a numeric version.
    pub(crate) fn parse_numeric(s: &str) -> Result<Self> {
        Self::parse(s, version_format::NUMERIC)
    }
}

impl PartialEq for CtfVersion {
    fn eq(&self, other: &Self) -> bool {
        self.major == other.major && self.minor == other.minor && self.revision == other.revision
    }
}

impl Eq for CtfVersion {}

impl PartialOrd for CtfVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CtfVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.revision).cmp(&(other.major, other.minor, other.revision))
    }
}

impl fmt::Display for CtfVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.version_string.is_empty() {
            return f.write_str(&self.version_string);
        }
        write!(f, "{}", self.major)?;
        if self.minor != 0 || self.revision != 0 {
            write!(f, ".{}", self.minor)?;
            if self.revision != 0 {
                write!(f, ".{}", self.revision)?;
            }
        }
        Ok(())
    }
}

/// Version 1.2 (2012, initial Autodesk version).
pub(crate) const CTF_PROCESS_LIST_VERSION_1_2: CtfVersion = CtfVersion::new(1, 2, 0);
/// Version 1.3 (revised matrix).
pub(crate) const CTF_PROCESS_LIST_VERSION_1_3: CtfVersion = CtfVersion::new(1, 3, 0);
/// Version 1.4 (ACES v0.2).
pub(crate) const CTF_PROCESS_LIST_VERSION_1_4: CtfVersion = CtfVersion::new(1, 4, 0);
/// Version 1.5 (ACES v0.7).
pub(crate) const CTF_PROCESS_LIST_VERSION_1_5: CtfVersion = CtfVersion::new(1, 5, 0);
/// Version 1.6 (invLut3D).
pub(crate) const CTF_PROCESS_LIST_VERSION_1_6: CtfVersion = CtfVersion::new(1, 6, 0);
/// Version 1.7 (inverted flag, ACES 1.0 styles, CLF v2 support).
pub(crate) const CTF_PROCESS_LIST_VERSION_1_7: CtfVersion = CtfVersion::new(1, 7, 0);
/// Version 1.8 (Function op).
pub(crate) const CTF_PROCESS_LIST_VERSION_1_8: CtfVersion = CtfVersion::new(1, 8, 0);
/// Version 2.0 (sync with OCIO and CLF v3).
pub(crate) const CTF_PROCESS_LIST_VERSION_2_0: CtfVersion = CtfVersion::new(2, 0, 0);
/// Version 2.5 (GradingHueCurve).
pub(crate) const CTF_PROCESS_LIST_VERSION_2_5: CtfVersion = CtfVersion::new(2, 5, 0);
/// Version 2.6 (ACES RGB to HMJ fixed function).
pub(crate) const CTF_PROCESS_LIST_VERSION_2_6: CtfVersion = CtfVersion::new(2, 6, 0);
/// Latest supported version.
pub(crate) const CTF_PROCESS_LIST_VERSION: CtfVersion = CTF_PROCESS_LIST_VERSION_2_6;

/// Latest supported major version of the `Info` element.
pub(crate) const CTF_INFO_ELEMENT_VERSION: i32 = 2;

/// Port of `FormatMetadataImpl::combine` (values and same-named attributes
/// are concatenated with `" + "`, children are appended).
pub(crate) fn combine_metadata(dst: &mut FormatMetadata, rhs: &FormatMetadata) -> Result<()> {
    if dst.element_name != rhs.element_name {
        return Err(Error::msg(
            "Only FormatMetadata with the same name can be combined.",
        ));
    }
    fn combine(first: &mut String, second: &str) {
        if !second.is_empty() {
            if !first.is_empty() {
                first.push_str(" + ");
            }
            first.push_str(second);
        }
    }
    combine(&mut dst.element_value, &rhs.element_value);
    for (n, v) in &rhs.attributes {
        if !v.is_empty() {
            if let Some(a) = dst.attributes.iter_mut().find(|(an, _)| an == n) {
                combine(&mut a.1, v);
            } else {
                dst.attributes.push((n.clone(), v.clone()));
            }
        }
    }
    dst.children.extend(rhs.children.iter().cloned());
    Ok(())
}

/// The content of a CLF/CTF file: the process list metadata and the
/// process nodes (port of `CTFReaderTransform`).
#[derive(Debug, Clone)]
pub(crate) struct CtfReaderTransform {
    /// The merged `Info` elements.
    pub info_metadata: FormatMetadata,
    /// Attributes and children (descriptions, descriptors, `Id`) of the
    /// `ProcessList`.
    pub metadata: FormatMetadata,
    /// The process nodes.
    pub ops: Vec<OpData>,
    /// CTF version (CLF versions are translated).
    pub version: CtfVersion,
    /// Original CLF version, (0, 0) for CTF files.
    pub clf_version: CtfVersion,
    /// Output bit-depth of the previous op (while reading).
    pub prev_out_bd: BitDepth,
}

impl Default for CtfReaderTransform {
    fn default() -> Self {
        Self {
            info_metadata: FormatMetadata::new(METADATA_INFO, ""),
            metadata: FormatMetadata::default(),
            ops: Vec::new(),
            version: CTF_PROCESS_LIST_VERSION,
            clf_version: CtfVersion::new(0, 0, 0),
            prev_out_bd: BitDepth::Unknown,
        }
    }
}

fn copy_non_empty_attribute(dst: &mut FormatMetadata, src: &FormatMetadata, name: &str) {
    let v = src.attribute_value(name);
    if !v.is_empty() {
        let v = v.to_string();
        dst.add_attribute(name, &v);
    }
}

fn last_element_value<'a>(elements: &'a [FormatMetadata], name: &str) -> &'a str {
    elements
        .iter()
        .rev()
        .find(|e| e.element_name.eq_ignore_ascii_case(name))
        .map(|e| e.element_value.as_str())
        .unwrap_or("")
}

fn copy_descs(dst: &mut FormatMetadata, src: &FormatMetadata, name: &str) {
    for desc in src
        .children
        .iter()
        .filter(|c| c.element_name.eq_ignore_ascii_case(name))
    {
        let mut e = FormatMetadata::new(&desc.element_name, &desc.element_value);
        let lang = desc.attribute_value("language");
        if !lang.is_empty() {
            let lang = lang.to_string();
            e.add_attribute("language", &lang);
        }
        dst.children.push(e);
    }
}

fn copy_process_list_metadata(dst: &mut FormatMetadata, src: &FormatMetadata) {
    copy_non_empty_attribute(dst, src, METADATA_ID);
    copy_non_empty_attribute(dst, src, METADATA_NAME);
    copy_non_empty_attribute(dst, src, ATTR_INVERSE_OF);
    let xmlns: Vec<String> = src
        .attributes
        .iter()
        .filter(|(n, _)| n.starts_with("xmlns:"))
        .map(|(n, _)| n.clone())
        .collect();
    for n in xmlns {
        copy_non_empty_attribute(dst, src, &n);
    }
    let id = last_element_value(&src.children, METADATA_ID_ELEMENT).to_string();
    if !id.is_empty() {
        dst.add_child_element(METADATA_ID_ELEMENT, &id);
    }
    copy_descs(dst, src, METADATA_DESCRIPTION);
    copy_descs(dst, src, METADATA_INPUT_DESCRIPTOR);
    copy_descs(dst, src, METADATA_OUTPUT_DESCRIPTOR);
}

impl CtfReaderTransform {
    /// Transform for writing: `ops` and the metadata of a group transform.
    pub(crate) fn from_ops(ops: Vec<OpData>, metadata: &FormatMetadata) -> Result<Self> {
        let mut t = Self::default();
        t.from_metadata(metadata)?;
        t.ops = ops;
        Ok(t)
    }

    /// The `id` attribute.
    pub(crate) fn id(&self) -> &str {
        self.metadata.attribute_value(METADATA_ID)
    }

    /// Set the `id` attribute.
    pub(crate) fn set_id(&mut self, id: &str) {
        self.metadata.add_attribute(METADATA_ID, id);
    }

    /// The `name` attribute.
    pub(crate) fn name(&self) -> &str {
        self.metadata.attribute_value(METADATA_NAME)
    }

    /// Set the `name` attribute.
    #[cfg(test)]
    pub(crate) fn set_name(&mut self, name: &str) {
        self.metadata.add_attribute(METADATA_NAME, name);
    }

    /// True for CLF files.
    pub(crate) fn is_clf(&self) -> bool {
        self.clf_version != CtfVersion::new(0, 0, 0)
    }

    /// Copy the metadata that CLF/CTF preserves (port of
    /// `CTFReaderTransform::fromMetadata`).
    pub(crate) fn from_metadata(&mut self, metadata: &FormatMetadata) -> Result<()> {
        copy_process_list_metadata(&mut self.metadata, metadata);
        for elt in &metadata.children {
            if elt.element_name.eq_ignore_ascii_case(METADATA_INFO) {
                combine_metadata(&mut self.info_metadata, elt)?;
            }
        }
        Ok(())
    }

    /// Put the process list information into `metadata` (port of
    /// `CTFReaderTransform::toMetadata`).
    pub(crate) fn to_metadata(&self, metadata: &mut FormatMetadata) {
        copy_process_list_metadata(metadata, &self.metadata);
        let info = &self.info_metadata;
        if !info.attributes.is_empty()
            || !info.children.is_empty()
            || !info.element_value.is_empty()
        {
            metadata.children.push(info.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fileformats::ctf::opdata::MatrixData;

    #[test]
    fn read_version() {
        let v1 = CtfVersion::new(1, 2, 3);
        let v2 = CtfVersion::new(1, 2, 3);
        assert_eq!(v1, v2);
        for v3 in [
            CtfVersion::new(0, 0, 1),
            CtfVersion::new(0, 1, 0),
            CtfVersion::new(1, 0, 0),
            CtfVersion::new(1, 2, 0),
            CtfVersion::new(1, 2, 2),
        ] {
            assert!(v1 != v3);
            assert!(v3 < v1);
        }

        assert_eq!(
            CtfVersion::parse_numeric("1.2.3").unwrap(),
            CtfVersion::new(1, 2, 3)
        );
        assert_eq!(
            CtfVersion::parse_numeric("1.2").unwrap(),
            CtfVersion::new(1, 2, 0)
        );
        assert_eq!(
            CtfVersion::parse_numeric("1").unwrap(),
            CtfVersion::new(1, 0, 0)
        );
        assert_eq!(
            CtfVersion::parse_numeric("1.10").unwrap(),
            CtfVersion::new(1, 10, 0)
        );
        assert_eq!(
            CtfVersion::parse_numeric("1.1.0").unwrap(),
            CtfVersion::new(1, 1, 0)
        );
        assert_eq!(
            CtfVersion::parse_numeric("1.01").unwrap(),
            CtfVersion::new(1, 1, 0)
        );
        assert_eq!(
            CtfVersion::parse("2.0.0", version_format::SMPTE_CLF).unwrap(),
            CtfVersion::new(2, 0, 0)
        );

        let e = CtfVersion::parse_numeric("ST2136-1:2024").unwrap_err();
        assert!(e
            .message()
            .contains("is not a valid version. Expecting MAJOR[.MINOR[.REVISION]]"));
        let e = CtfVersion::parse_numeric(SMPTE_XMLNS_URL).unwrap_err();
        assert!(e
            .message()
            .contains("is not a valid version. Expecting MAJOR[.MINOR[.REVISION]]"));
        assert_eq!(
            CtfVersion::parse("ST2136-1:2024", version_format::SMPTE_CLF).unwrap(),
            CtfVersion::new(3, 0, 0)
        );
        let e = CtfVersion::parse("ST2136-1:2024", version_format::SMPTE_XMLNS).unwrap_err();
        assert!(e.message().contains(
            "is not a valid version. Expecting 'http://www.smpte-ra.org/ns/2136-1/2024' or MAJOR[.MINOR[.REVISION]]"
        ));
        assert_eq!(
            CtfVersion::parse(SMPTE_XMLNS_URL, version_format::SMPTE_XMLNS).unwrap(),
            CtfVersion::new(3, 0, 0)
        );
        let e = CtfVersion::parse(SMPTE_XMLNS_URL, version_format::SMPTE_CLF).unwrap_err();
        assert!(e.message().contains(
            "is not a valid version. Expecting 'ST2136-1:2024' or MAJOR[.MINOR[.REVISION]]"
        ));

        for bad in ["", "1 2", "1-2", "a", "1.", ".2", "1.0 2", "-1"] {
            let e = CtfVersion::parse_numeric(bad).unwrap_err();
            assert!(e.message().contains("is not a valid version"), "{bad}");
        }
    }

    #[test]
    fn version_write() {
        assert_eq!(CtfVersion::new(1, 2, 3).to_string(), "1.2.3");
        assert_eq!(CtfVersion::new(1, 0, 3).to_string(), "1.0.3");
        assert_eq!(CtfVersion::new(1, 2, 0).to_string(), "1.2");
        assert_eq!(CtfVersion::new(1, 20, 0).to_string(), "1.20");
        assert_eq!(CtfVersion::new(1, 0, 0).to_string(), "1");
        assert_eq!(CtfVersion::new(0, 0, 0).to_string(), "0");
        assert_eq!(
            CtfVersion::parse("ST2136-1:2024", version_format::SMPTE_CLF)
                .unwrap()
                .to_string(),
            "ST2136-1:2024"
        );
        assert_eq!(
            CtfVersion::parse(SMPTE_XMLNS_URL, version_format::SMPTE_XMLNS)
                .unwrap()
                .to_string(),
            SMPTE_XMLNS_URL
        );
    }

    #[test]
    fn accessors() {
        let mut t = CtfReaderTransform::default();
        assert_eq!(t.info_metadata.element_name(), METADATA_INFO);
        assert_eq!(t.id(), "");
        assert_eq!(t.name(), "");
        assert_eq!(t.info_metadata.attribute_value("inverseOf"), "");
        assert!(t.ops.is_empty());
        assert!(t.info_metadata.children.is_empty());

        t.set_name("Name");
        t.set_id("123");
        let meta = &mut t.info_metadata;
        meta.add_attribute(ATTR_INVERSE_OF, "654");
        meta.add_child_element(
            METADATA_ID_ELEMENT,
            "urn:uuid:123e4567-e89b-12d3-a456-426655440000",
        );
        t.ops.push(OpData::Matrix(MatrixData::default()));
        let meta = &mut t.info_metadata;
        meta.add_child_element(METADATA_DESCRIPTION, "One");
        meta.add_child_element(METADATA_DESCRIPTION, "Two");
        meta.add_child_element(METADATA_INPUT_DESCRIPTOR, "input 1");
        meta.add_child_element(METADATA_INPUT_DESCRIPTOR, "input 2");
        meta.add_child_element(METADATA_OUTPUT_DESCRIPTOR, "output 1");
        meta.add_child_element(METADATA_OUTPUT_DESCRIPTOR, "output 2")
            .add_attribute("language", "tr");

        assert_eq!(t.id(), "123");
        assert_eq!(t.name(), "Name");
        assert_eq!(t.info_metadata.attribute_value("inverseOf"), "654");
        assert_eq!(t.ops.len(), 1);
        let meta = &t.info_metadata;
        assert_eq!(meta.children.len(), 7);
        assert_eq!(meta.children[0].element_name(), "Id");
        assert_eq!(
            meta.children[0].element_value(),
            "urn:uuid:123e4567-e89b-12d3-a456-426655440000"
        );
        assert_eq!(meta.children[1].element_value(), "One");
        assert_eq!(meta.children[2].element_value(), "Two");
        assert_eq!(meta.children[3].element_name(), "InputDescriptor");
        assert_eq!(meta.children[6].element_name(), "OutputDescriptor");
        assert_eq!(meta.children[6].attribute_value("language"), "tr");
    }
}
