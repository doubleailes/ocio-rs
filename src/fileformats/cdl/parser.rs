//! Parser of the ASC CDL XML files: ColorDecisionList (`.cdl`),
//! ColorCorrectionCollection (`.ccc`) and ColorCorrection (`.cc`) (port of
//! `fileformats/cdl/CDLParser.cpp`, `CDLReaderHelper.*` and the CDL related
//! element readers of `XMLReaderHelper.*`).

use crate::error::{Error, Result};
use crate::fileformats::ctf::opdata::validate_cdl_params;
use crate::fileformats::ctf::xml::*;
use crate::format_metadata::FormatMetadata;
use crate::transforms::{
    CdlTransform, METADATA_INPUT_DESCRIPTION, METADATA_SAT_DESCRIPTION, METADATA_SOP_DESCRIPTION,
    METADATA_VIEWING_DESCRIPTION,
};

/// `ColorDecisionList` element.
pub(crate) const CDL_TAG_COLOR_DECISION_LIST: &str = "ColorDecisionList";
/// `ColorDecision` element.
pub(crate) const CDL_TAG_COLOR_DECISION: &str = "ColorDecision";
/// `ColorCorrectionCollection` element.
pub(crate) const CDL_TAG_COLOR_CORRECTION_COLLECTION: &str = "ColorCorrectionCollection";

/// The CDLs of a file and the metadata of the root element (port of
/// `CDLParsingInfo`).
#[derive(Debug, Clone, Default)]
pub(crate) struct CdlParsingInfo {
    /// The color corrections, in file order.
    pub transforms: Vec<CdlTransform>,
    /// Descriptive elements of the ColorDecisionList or
    /// ColorCorrectionCollection.
    pub metadata: FormatMetadata,
}

/// The schema of the file (from the root element).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Schema {
    Cdl,
    Ccc,
    Cc,
}

/// Values of a ColorCorrection being read (the `CDLOpData` of OCIO).
#[derive(Debug, Clone)]
struct CcData {
    slope: [f64; 3],
    offset: [f64; 3],
    power: [f64; 3],
    sat: f64,
    metadata: FormatMetadata,
}

impl Default for CcData {
    fn default() -> Self {
        Self {
            slope: [1.0; 3],
            offset: [0.0; 3],
            power: [1.0; 3],
            sat: 1.0,
            metadata: FormatMetadata::default(),
        }
    }
}

#[derive(Debug)]
enum Kind {
    /// ColorDecisionList or ColorCorrectionCollection (root containers).
    Root,
    /// ColorDecision (its metadata is not preserved).
    ColorDecision,
    ColorCorrection(Box<CcData>),
    SopNode {
        slope: bool,
        offset: bool,
        power: bool,
    },
    SatNode,
    /// Slope, Offset or Power.
    SopValue(String),
    Saturation(String),
    Description {
        text: String,
        changed: bool,
    },
    Dummy,
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
            Kind::Root
                | Kind::ColorDecision
                | Kind::ColorCorrection(_)
                | Kind::SopNode { .. }
                | Kind::SatNode
        )
    }

    fn is_dummy(&self) -> bool {
        matches!(self.kind, Kind::Dummy)
    }

    /// Port of `XmlReaderElement::throwMessage`.
    fn error(&self, msg: &str) -> Error {
        Error::msg(format!("At line {}: {}", self.line, msg))
    }
}

/// Port of `CDLParser::Impl`.
struct Parser {
    file_name: String,
    schema: Schema,
    line: u32,
    elms: Vec<Elt>,
    info: Option<CdlParsingInfo>,
    warnings: Vec<String>,
}

/// Port of `IsValidDescriptionTag`.
fn is_valid_description_tag(current: &str, parent: &str) -> bool {
    let is_desc = current == TAG_DESCRIPTION;
    let is_input_viewing =
        current == METADATA_INPUT_DESCRIPTION || current == METADATA_VIEWING_DESCRIPTION;
    let is_sop_sat = parent == TAG_SOPNODE || parent == TAG_SATNODE || parent == TAG_SATNODEALT;
    is_desc || (is_input_viewing && !is_sop_sat)
}

impl Parser {
    fn xml_file(&self) -> &str {
        if self.file_name.is_empty() {
            "File name not specified"
        } else {
            &self.file_name
        }
    }

    /// Port of `CDLParser::Impl::throwMessage`.
    fn throw_message(&self, error: &str) -> Error {
        let root = match self.schema {
            Schema::Cc => CDL_TAG_COLOR_CORRECTION,
            Schema::Ccc => CDL_TAG_COLOR_CORRECTION_COLLECTION,
            Schema::Cdl => CDL_TAG_COLOR_DECISION_LIST,
        };
        Error::msg(format!(
            "Error parsing {} ({}). Error is: {}. At line ({})",
            root, self.file_name, error, self.line
        ))
    }

    fn has_transforms(&self) -> bool {
        self.info
            .as_ref()
            .map_or(false, |i| !i.transforms.is_empty())
    }

    fn back_is(&self, f: impl Fn(&Kind) -> bool) -> bool {
        self.elms.last().map_or(false, |e| f(&e.kind))
    }

    /// Port of `createDummyElement` (the warning of `XmlReaderDummyElt`).
    fn push_dummy(&mut self, name: &str, msg: &str) {
        let (pname, pline) = match self.elms.last() {
            Some(e) => (e.name.clone(), e.line),
            None => (String::new(), 0),
        };
        let w = format!(
            "{}({}): Unrecognized element '{}' where its parent is '{}' ({}): {}.",
            self.xml_file(),
            self.line,
            name,
            pname,
            pline,
            msg
        );
        self.warnings.push(w);
        self.push(name, Kind::Dummy);
    }

    fn push(&mut self, name: &str, kind: Kind) {
        let line = self.line;
        self.elms.push(Elt {
            name: name.to_string(),
            line,
            kind,
        });
    }

    fn handle_root(&mut self, name: &str, tag: &str, msg: &str) -> bool {
        if name != tag {
            return false;
        }
        if !self.has_transforms() {
            // Note: the parsing info is bound to the root element.
            self.info = Some(CdlParsingInfo::default());
            self.push(name, Kind::Root);
        } else {
            self.push_dummy(name, msg);
        }
        true
    }

    fn handle_color_decision(&mut self, name: &str) -> bool {
        if name != CDL_TAG_COLOR_DECISION {
            return false;
        }
        if self.back_is(|k| matches!(k, Kind::Root)) {
            self.push(name, Kind::ColorDecision);
        } else {
            self.push_dummy(name, ": ColorDecision must be under a ColorDecisionList");
        }
        true
    }

    fn handle_color_correction(&mut self, name: &str) -> bool {
        const MSG: &str = ": ColorCorrection must be under a ColorDecision (CDL), ColorCorrectionCollection (CCC), or must be the root element (CC)";
        if name != CDL_TAG_COLOR_CORRECTION {
            return false;
        }
        let ok = match self.schema {
            Schema::Cdl => self.back_is(|k| matches!(k, Kind::ColorDecision)),
            Schema::Ccc => self.back_is(|k| matches!(k, Kind::Root)),
            Schema::Cc => !self.has_transforms(),
        };
        if ok {
            self.push(name, Kind::ColorCorrection(Box::default()));
        } else {
            self.push_dummy(name, MSG);
        }
        true
    }

    fn handle_sop_node(&mut self, name: &str) -> bool {
        if name != TAG_SOPNODE {
            return false;
        }
        if self.back_is(|k| matches!(k, Kind::ColorCorrection(_))) {
            self.push(
                name,
                Kind::SopNode {
                    slope: false,
                    offset: false,
                    power: false,
                },
            );
        } else {
            self.push_dummy(name, ": SOPNode must be under a ColorCorrection");
        }
        true
    }

    fn handle_sat_node(&mut self, name: &str) -> bool {
        if name != TAG_SATNODE && name != TAG_SATNODEALT {
            return false;
        }
        if self.back_is(|k| matches!(k, Kind::ColorCorrection(_))) {
            self.push(name, Kind::SatNode);
        } else {
            self.push_dummy(name, ": SatNode must be under a ColorCorrection");
        }
        true
    }

    /// Port of `HandleTerminalStartElement`.
    fn handle_terminal(&mut self, name: &str) -> bool {
        let container_id = match self.elms.last() {
            Some(e) if e.is_container() => e.name.clone(),
            _ => {
                self.push_dummy(name, "Internal error");
                return true;
            }
        };
        if is_valid_description_tag(name, &container_id) {
            self.push(
                name,
                Kind::Description {
                    text: String::new(),
                    changed: false,
                },
            );
            return true;
        }
        if name == TAG_SLOPE || name == TAG_OFFSET || name == TAG_POWER {
            if self.back_is(|k| matches!(k, Kind::SopNode { .. })) {
                self.push(name, Kind::SopValue(String::new()));
            } else {
                self.push_dummy(name, ": Slope, Offset or Power tags must be under SOPNode");
            }
            return true;
        }
        if name == TAG_SATURATION {
            if self.back_is(|k| matches!(k, Kind::SatNode)) {
                self.push(name, Kind::Saturation(String::new()));
            } else {
                self.push_dummy(name, ": Saturation tags must be under SatNode");
            }
            return true;
        }
        false
    }

    fn start(&mut self, name: &str, atts: &[(String, String)]) -> Result<()> {
        if name.is_empty() {
            return Err(self.throw_message("Internal parsing error"));
        }
        let handled = match self.schema {
            Schema::Cdl => {
                self.handle_root(
                    name,
                    CDL_TAG_COLOR_DECISION_LIST,
                    ": The ColorDecisionList already exists",
                ) || self.handle_color_decision(name)
                    || self.handle_color_correction(name)
            }
            Schema::Ccc => {
                self.handle_root(
                    name,
                    CDL_TAG_COLOR_CORRECTION_COLLECTION,
                    ": The ColorCorrectionCollection already exists",
                ) || self.handle_color_correction(name)
            }
            Schema::Cc => self.handle_color_correction(name),
        } || self.handle_sop_node(name)
            || self.handle_sat_node(name)
            || self.handle_terminal(name);
        if !handled {
            self.push_dummy(name, ": Unknown element");
        }

        // Element start.
        let top = match self.elms.last_mut() {
            Some(t) => t,
            None => return Ok(()),
        };
        if let Kind::ColorCorrection(data) = &mut top.kind {
            for (n, v) in atts {
                if n == ATTR_ID {
                    // Note: escaped characters are already replaced.
                    data.metadata.set_id(v);
                }
            }
        }
        Ok(())
    }

    /// Append a description to the container at `idx` (port of the
    /// `appendMetadata` methods).
    fn append_metadata(&mut self, idx: usize, mut md: FormatMetadata) {
        let (before, rest) = self.elms.split_at_mut(idx);
        let elt = &mut rest[0];
        match &mut elt.kind {
            Kind::Root => {
                if let Some(info) = self.info.as_mut() {
                    info.metadata.children.push(md);
                }
            }
            Kind::ColorCorrection(data) => data.metadata.children.push(md),
            Kind::SopNode { .. } | Kind::SatNode => {
                let is_sop = matches!(elt.kind, Kind::SopNode { .. });
                md.set_element_name(if is_sop {
                    METADATA_SOP_DESCRIPTION
                } else {
                    METADATA_SAT_DESCRIPTION
                });
                if let Some(Elt {
                    kind: Kind::ColorCorrection(data),
                    ..
                }) = before.last_mut()
                {
                    data.metadata.children.push(md);
                }
            }
            // The metadata of a ColorDecision is not preserved.
            _ => {}
        }
    }

    /// The ColorCorrection of the SOPNode / SatNode at `idx`.
    fn cc_data_of_node(&mut self, idx: usize) -> Option<&mut CcData> {
        if idx == 0 {
            return None;
        }
        match &mut self.elms[idx - 1].kind {
            Kind::ColorCorrection(d) => Some(d),
            _ => None,
        }
    }

    fn end(&mut self, name: &str) -> Result<()> {
        if name.is_empty() {
            return Err(self.throw_message("Internal parsing error"));
        }
        let elt = match self.elms.last() {
            Some(e) => e,
            None => return Err(self.throw_message("Missing element")),
        };
        if elt.name != name {
            let msg = format!("Unexpected element ({}). Expecting ({}). ", name, elt.name);
            return Err(self.throw_message(&msg));
        }
        let elt = match self.elms.pop() {
            Some(e) => e,
            None => return Err(self.throw_message("Missing element")),
        };

        if !elt.is_container() && !elt.is_dummy() {
            // Is it at the right location in the stack?
            let parent_ok = self.elms.last().map_or(false, |p| p.is_container());
            if !parent_ok {
                return Err(self.throw_message(&format!("Parsing error ({name})")));
            }
        }

        let parent_idx = self.elms.len().wrapping_sub(1);
        // The element without its content (for the error messages).
        let head = Elt {
            name: elt.name.clone(),
            line: elt.line,
            kind: Kind::Dummy,
        };
        match elt.kind {
            Kind::Description { text, changed } => {
                if changed {
                    self.append_metadata(parent_idx, FormatMetadata::new(&head.name, &text));
                }
            }
            Kind::SopValue(content) => {
                let data = parse_values(&head, &content)?;
                if data.len() != 3 {
                    return Err(head.error("SOPNode: 3 values required."));
                }
                let v = [data[0], data[1], data[2]];
                if let Some(Kind::SopNode {
                    slope,
                    offset,
                    power,
                }) = self.elms.last_mut().map(|e| &mut e.kind)
                {
                    if head.name == TAG_SLOPE {
                        *slope = true;
                    } else if head.name == TAG_OFFSET {
                        *offset = true;
                    } else if head.name == TAG_POWER {
                        *power = true;
                    }
                }
                if let Some(cc) = self.cc_data_of_node(parent_idx) {
                    if head.name == TAG_SLOPE {
                        cc.slope = v;
                    } else if head.name == TAG_OFFSET {
                        cc.offset = v;
                    } else if head.name == TAG_POWER {
                        cc.power = v;
                    }
                }
            }
            Kind::Saturation(content) => {
                let data = parse_values(&head, &content)?;
                if data.len() != 1 {
                    return Err(head.error("SatNode: non-single value. "));
                }
                if head.name == TAG_SATURATION {
                    if let Some(cc) = self.cc_data_of_node(parent_idx) {
                        cc.sat = data[0];
                    }
                }
            }
            Kind::SopNode {
                slope,
                offset,
                power,
            } => {
                if !slope {
                    return Err(head.error("Required node 'Slope' is missing. "));
                }
                if !offset {
                    return Err(head.error("Required node 'Offset' is missing. "));
                }
                if !power {
                    return Err(head.error("Required node 'Power' is missing. "));
                }
            }
            Kind::ColorCorrection(data) => {
                let mut t = CdlTransform::default();
                t.slope = data.slope;
                t.offset = data.offset;
                t.power = data.power;
                t.sat = data.sat;
                t.metadata = data.metadata;
                validate_cdl_params(&t.slope, &t.power, t.sat).map_err(|e| {
                    Error::msg(format!("CDLTransform validation failed: {}", e.message()))
                })?;
                if let Some(info) = self.info.as_mut() {
                    info.transforms.push(t);
                }
            }
            Kind::Root | Kind::ColorDecision | Kind::SatNode | Kind::Dummy => {}
        }
        Ok(())
    }

    fn chars(&mut self, s: &str) -> Result<()> {
        if s.is_empty() {
            return Ok(());
        }
        // Parsing a single new line. This is valid.
        if s == "\n" {
            return Ok(());
        }
        if self.elms.is_empty() {
            return Err(self.throw_message("Unexpected character data before root element"));
        }
        let illegal = format!("Illegal attribute ({s})");
        let top = match self.elms.last_mut() {
            Some(t) => t,
            None => return Ok(()),
        };
        if let Kind::Description { text, changed } = &mut top.kind {
            // For description all the text is kept.
            text.push_str(s);
            *changed = true;
            return Ok(());
        }
        // Ignore white-spaces.
        let (start, end) = find_sub_string(s.as_bytes());
        if end > 0 {
            if top.is_container() {
                return Err(self.throw_message(&illegal));
            }
            let sub = &s[start..end];
            match &mut top.kind {
                Kind::SopValue(c) | Kind::Saturation(c) => {
                    c.push_str(sub);
                    c.push(' ');
                }
                _ => {}
            }
        }
        Ok(())
    }
}

/// Parse the numbers of a Slope / Offset / Power / Saturation element.
fn parse_values(elt: &Elt, content: &str) -> Result<Vec<f64>> {
    let content = trim(content);
    get_numbers::<f64>(content).map_err(|_| {
        let s = truncate_string(content);
        elt.error(&format!("Illegal values '{}' in {}", s, elt.name))
    })
}

impl SaxHandler for Parser {
    fn start_element(&mut self, name: &str, atts: &[(String, String)], line: u32) -> Result<()> {
        self.line = line;
        self.start(name, atts)
    }

    fn end_element(&mut self, name: &str, line: u32) -> Result<()> {
        self.line = line;
        self.end(name)
    }

    fn character_data(&mut self, s: &str, line: u32) -> Result<()> {
        self.line = line;
        self.chars(s)
    }
}

/// Port of `CDLParser::Impl::loadHeader`: the first kilobytes of the file.
fn load_header(data: &[u8]) -> String {
    const LIMIT: usize = 5 * 1024;
    let mut header = String::new();
    let mut processed = 0usize;
    for line in data.split(|&c| c == b'\n') {
        if processed >= LIMIT {
            break;
        }
        let line = &line[..line.len().min(LIMIT - 1)];
        // Strings stop at the first null character.
        let line = match line.iter().position(|&c| c == 0) {
            Some(p) => &line[..p],
            None => line,
        };
        header.push_str(&String::from_utf8_lossy(line));
        header.push(' ');
        processed += line.len();
    }
    header
}

/// The result of the parsing of a CDL file.
#[derive(Debug)]
pub(crate) struct CdlParseResult {
    /// The CDLs and root metadata.
    pub info: CdlParsingInfo,
    /// True if the root element is a ColorCorrection.
    pub is_cc: bool,
    /// True if the root element is a ColorCorrectionCollection.
    #[cfg_attr(not(test), allow(dead_code))]
    pub is_ccc: bool,
    /// The warnings OCIO would log while reading (there is no logging
    /// facility, they are kept for inspection).
    #[cfg_attr(not(test), allow(dead_code))]
    pub warnings: Vec<String>,
}

/// Parse a CDL, CCC or CC file (port of `CDLParser::parse`).
pub(crate) fn parse_cdl(data: &[u8], file_name: &str) -> Result<CdlParseResult> {
    let header = load_header(data);
    let mut p = Parser {
        file_name: file_name.to_string(),
        schema: Schema::Cdl,
        line: 0,
        elms: Vec::new(),
        info: None,
        warnings: Vec::new(),
    };
    if header.contains(&format!("<{CDL_TAG_COLOR_DECISION_LIST}")) {
        p.schema = Schema::Cdl;
    } else if header.contains(&format!("<{CDL_TAG_COLOR_CORRECTION_COLLECTION}")) {
        p.schema = Schema::Ccc;
    } else if header.contains(&format!("<{CDL_TAG_COLOR_CORRECTION}")) {
        p.schema = Schema::Cc;
        // If parsing a CC, initialize the transform list explicitly.
        p.info = Some(CdlParsingInfo::default());
    } else {
        return Err(p.throw_message("Missing CDL tag"));
    }

    match parse_xml(data, &mut p) {
        Ok(()) => {}
        Err(SaxError::Handler(e)) => return Err(e),
        Err(SaxError::TagMismatch(line)) => {
            p.line = line;
            let msg = match p.elms.last() {
                Some(e) => format!("XML parsing error (no closing tag for '{}'). ", e.name),
                None => "XML parsing error (unbalanced element tags). ".to_string(),
            };
            return Err(p.throw_message(&msg));
        }
        Err(SaxError::Syntax(msg, line)) => {
            p.line = line;
            return Err(p.throw_message(&format!("XML parsing error: {msg}")));
        }
    }
    p.line = fed_line_count(data);

    // Port of `validateParsing`.
    if let Some(e) = p.elms.last() {
        let msg = format!("CDL parsing error (no closing tag for '{})", e.name);
        return Err(p.throw_message(&msg));
    }

    let is_cc = p.schema == Schema::Cc;
    let is_ccc = p.schema == Schema::Ccc;
    Ok(CdlParseResult {
        info: p.info.take().unwrap_or_default(),
        is_cc,
        is_ccc,
        warnings: p.warnings,
    })
}

/// Port of `CDLParser::getCDLTransforms`: check that the ids are unique.
pub(crate) fn check_unique_ids(transforms: &[CdlTransform]) -> Result<()> {
    let mut ids: Vec<&str> = Vec::new();
    for t in transforms {
        let id = t.metadata.id();
        if !id.is_empty() {
            if ids.contains(&id) {
                crate::bail!(
                    "Error loading ccc xml. Duplicate elements with '{}' found. If id is specified, it must be unique.",
                    id
                );
            }
            ids.push(id);
        }
    }
    Ok(())
}
