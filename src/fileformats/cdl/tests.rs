//! Tests of the CC, CCC and CDL formats (port of `FileFormatCC_tests.cpp`,
//! `FileFormatCCC_tests.cpp`, `FileFormatCDL_tests.cpp` and of the CDL file
//! parts of `CDLTransform_tests.cpp`).

use super::*;
use crate::format_metadata::FormatMetadata;
use crate::types::TransformDirection;

const TEST_FILES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/files/");

fn test_path(name: &str) -> String {
    format!("{TEST_FILES}{name}")
}

fn load(format: &dyn FileFormat, name: &str) -> Result<CachedFile> {
    let p = test_path(name);
    let data = std::fs::read(&p).map_err(|_| Error::msg("Error opening test file."))?;
    format.read(&data, &p, Interpolation::Default)
}

fn cdls(cached: &CachedFile) -> Vec<&CdlTransform> {
    cached
        .group
        .transforms
        .iter()
        .map(|t| match t {
            Transform::Cdl(c) => c,
            _ => panic!("not a CDL"),
        })
        .collect()
}

#[track_caller]
fn check_err<T: std::fmt::Debug>(r: Result<T>, what: &str) {
    match r {
        Ok(v) => panic!("expected an error containing {what:?}, got {v:?}"),
        Err(e) => assert!(
            e.message().contains(what),
            "error {:?} does not contain {:?}",
            e.message(),
            what
        ),
    }
}

#[track_caller]
fn check_children(md: &FormatMetadata, expected: &[(&str, &str)]) {
    let got: Vec<(&str, &str)> = md
        .children
        .iter()
        .map(|c| (c.element_name.as_str(), c.element_value.as_str()))
        .collect();
    assert_eq!(got, expected);
}

#[track_caller]
fn check_sop(c: &CdlTransform, slope: [f64; 3], offset: [f64; 3], power: [f64; 3], sat: f64) {
    assert_eq!(c.slope, slope);
    assert_eq!(c.offset, offset);
    assert_eq!(c.power, power);
    assert_eq!(c.sat, sat);
}

fn write(group: &GroupTransform, format_name: &str) -> Result<String> {
    let config = Config::create_raw();
    group.write(&config, format_name)
}

// ---------------------------------------------------------------------------
// FileFormatCC

#[test]
fn test_cc1() {
    let cached = load(&CcFormat, "cdl_test1.cc").unwrap();
    assert!(cached.is_cdl_collection);
    let c = cdls(&cached)[0];
    assert_eq!(c.id(), "foo");
    assert_eq!(c.first_sop_description(), "this is a description");
    check_sop(c, [1.1, 1.2, 1.3], [2.1, 2.2, 2.3], [3.1, 3.2, 3.3], 0.7);
}

#[test]
fn test_cc2() {
    // CC file using windows eol.
    let cached = load(&CcFormat, "cdl_test2.cc").unwrap();
    let c = cdls(&cached)[0];
    let md = &c.metadata;
    assert_eq!(md.id(), "cc0001");
    check_children(
        md,
        &[
            ("SOPDescription", "Example look"),
            ("SATDescription", "boosting sat"),
        ],
    );
    // Only the first SOP description is available that way.
    assert_eq!(c.id(), "cc0001");
    assert_eq!(c.first_sop_description(), "Example look");
    check_sop(
        c,
        [1.0, 1.0, 0.9],
        [-0.03, -0.02, 0.0],
        [1.25, 1.0, 1.0],
        1.7,
    );
}

#[test]
fn test_cc_sat_node() {
    let cached = load(&CcFormat, "cdl_test_SATNode.cc").unwrap();
    // "SATNode" is recognized.
    assert_eq!(cdls(&cached)[0].sat, 0.42);
}

#[test]
fn test_cc_asc_sat() {
    let p = test_path("cdl_test_ASC_SAT.cc");
    let data = std::fs::read(&p).unwrap();
    let parsed = parser::parse_cdl(&data, &p).unwrap();
    // A warning is expected.
    assert!(!parsed.warnings.is_empty());
    let cached = load(&CcFormat, "cdl_test_ASC_SAT.cc").unwrap();
    // "ASC_SAT" is not recognized. Default value is returned.
    assert_eq!(cdls(&cached)[0].sat, 1.0);
}

#[test]
fn test_cc_asc_sop() {
    let cached = load(&CcFormat, "cdl_test_ASC_SOP.cc").unwrap();
    let c = cdls(&cached)[0];
    // "ASC_SOP" is not recognized. Default values are used.
    assert_eq!(c.metadata.children.len(), 0);
    assert_eq!(c.id(), "foo");
    assert_eq!(c.first_sop_description(), "");
    assert_eq!(c.slope[0], 1.0);
    assert_eq!(c.offset[0], 0.0);
    assert_eq!(c.power[0], 1.0);
}

#[test]
fn test_cc2_load_save() {
    let group = CdlTransform::create_group_from_file(&test_path("cdl_test2.cc")).unwrap();
    let out = write(&group, FILEFORMAT_COLOR_CORRECTION).unwrap();
    let expected = r#"<ColorCorrection id="cc0001">
    <SOPNode>
        <Description>Example look</Description>
        <Slope>1 1 0.9</Slope>
        <Offset>-0.03 -0.02 0</Offset>
        <Power>1.25 1 1</Power>
    </SOPNode>
    <SatNode>
        <Description>boosting sat</Description>
        <Saturation>1.7</Saturation>
    </SatNode>
</ColorCorrection>
"#;
    assert_eq!(out, expected);
}

#[test]
fn test_cc_errors() {
    // Not a CC file.
    check_err(load(&CcFormat, "cdl_test1.ccc"), "is not a .cc file.");
    // Not a CDL XML file at all.
    check_err(
        load(&CcFormat, "cdl_various.ctf"),
        "Error parsing .cc file. Does not appear to contain a valid ASC CDL XML:",
    );
    // Write errors.
    check_err(
        write(&GroupTransform::new(), FILEFORMAT_COLOR_CORRECTION),
        "CDL write: there should be a single CDL.",
    );
    let group = GroupTransform::from_transforms(vec![Transform::Range(Default::default())]);
    check_err(
        write(&group, FILEFORMAT_COLOR_CORRECTION),
        "CDL write: only CDL can be written.",
    );
}

// ---------------------------------------------------------------------------
// FileFormatCCC

#[test]
fn ccc_read() {
    let cached = load(&CccFormat, "cdl_test1.ccc").unwrap();
    assert!(cached.is_cdl_collection);

    // Descriptive element children of <ColorCorrectionCollection> are preserved.
    check_children(
        &cached.group.metadata,
        &[
            (
                "Description",
                "This is a color correction collection example.",
            ),
            ("Description", "It includes all possible description uses."),
            (
                "InputDescription",
                "These should be applied in ACESproxy color space.",
            ),
            (
                "ViewingDescription",
                "View using the ACES RRT+ODT transforms.",
            ),
        ],
    );

    let v = cdls(&cached);
    assert_eq!(v.len(), 5);
    // Two of the five CDLs in the file don't have an id attribute.
    assert_eq!(v.iter().filter(|c| !c.id().is_empty()).count(), 3);

    assert_eq!(v[0].id(), "cc0001");
    check_children(
        &v[0].metadata,
        &[
            ("Description", "CC-level description 1a"),
            ("Description", "CC-level description 1b"),
            ("InputDescription", "CC-level input description 1"),
            ("ViewingDescription", "CC-level viewing description 1"),
            ("SOPDescription", "Example look"),
            ("SOPDescription", "For scenes 1 and 2"),
            ("SATDescription", "boosting sat"),
        ],
    );
    check_sop(
        v[0],
        [1.0, 1.0, 0.9],
        [-0.03, -0.02, 0.0],
        [1.25, 1.0, 1.0],
        1.7,
    );

    assert_eq!(v[1].id(), "cc0002");
    check_children(
        &v[1].metadata,
        &[
            ("Description", "CC-level description 2a"),
            ("Description", "CC-level description 2b"),
            ("InputDescription", "CC-level input description 2"),
            ("ViewingDescription", "CC-level viewing description 2"),
            ("SOPDescription", "pastel"),
            ("SOPDescription", "another example"),
            ("SATDescription", "dropping sat"),
        ],
    );
    check_sop(v[1], [0.9, 0.7, 0.6], [0.1, 0.1, 0.1], [0.9, 0.9, 0.9], 0.7);

    assert_eq!(v[2].id(), "cc0003");
    check_children(
        &v[2].metadata,
        &[
            ("Description", "CC-level description 3"),
            ("InputDescription", "CC-level input description 3"),
            ("ViewingDescription", "CC-level viewing description 3"),
            ("SOPDescription", "golden"),
            ("SATDescription", "no sat change"),
            ("SATDescription", "sat==1"),
        ],
    );
    check_sop(v[2], [1.2, 1.1, 1.0], [0.0; 3], [0.9, 1.0, 1.2], 1.0);

    assert_eq!(v[3].id(), "");
    assert_eq!(v[3].metadata.children.len(), 0);
    // SatNode missing from XML, uses a default of 1.0.
    check_sop(v[3], [4.0, 5.0, 6.0], [0.0; 3], [0.9, 1.0, 1.2], 1.0);

    assert_eq!(v[4].id(), "");
    assert_eq!(v[4].metadata.children.len(), 0);
    // SOPNode missing from XML, uses default values.
    check_sop(v[4], [1.0; 3], [0.0; 3], [1.0; 3], 0.0);
}

#[test]
fn ccc_read_build_ops() {
    // The CDL selected by the cccid of a FileTransform keeps its metadata.
    let config = Config::create_raw();
    let ft = crate::transforms::FileTransform {
        src: test_path("cdl_test1.ccc"),
        ccc_id: "cc0002".to_string(),
        ..Default::default()
    };
    let proc = config
        .get_processor_for_transform(&Transform::File(ft.clone()), TransformDirection::Forward)
        .unwrap();
    let group = proc.create_group_transform();
    assert_eq!(group.transforms.len(), 1);
    match &group.transforms[0] {
        Transform::Cdl(c) => {
            assert_eq!(c.metadata.children.len(), 7);
            assert_eq!(c.style, crate::types::CdlStyle::NoClamp);
        }
        _ => panic!("expected a CDL"),
    }

    // Test with ASC style.
    let ft = crate::transforms::FileTransform {
        cdl_style: crate::types::CdlStyle::Asc,
        ..ft
    };
    let proc = config
        .get_processor_for_transform(&Transform::File(ft), TransformDirection::Forward)
        .unwrap();
    let group = proc.create_group_transform();
    match &group.transforms[0] {
        Transform::Cdl(c) => assert_eq!(c.style, crate::types::CdlStyle::Asc),
        _ => panic!("expected a CDL"),
    }
}

#[test]
fn ccc_write() {
    let group = CdlTransform::create_group_from_file(&test_path("cdl_test1.ccc")).unwrap();
    let out = write(&group, FILEFORMAT_COLOR_CORRECTION_COLLECTION).unwrap();
    let expected = r#"<ColorCorrectionCollection xmlns="urn:ASC:CDL:v1.01">
    <Description>This is a color correction collection example.</Description>
    <Description>It includes all possible description uses.</Description>
    <InputDescription>These should be applied in ACESproxy color space.</InputDescription>
    <ViewingDescription>View using the ACES RRT+ODT transforms.</ViewingDescription>
    <ColorCorrection id="cc0001">
        <Description>CC-level description 1a</Description>
        <Description>CC-level description 1b</Description>
        <InputDescription>CC-level input description 1</InputDescription>
        <ViewingDescription>CC-level viewing description 1</ViewingDescription>
        <SOPNode>
            <Description>Example look</Description>
            <Description>For scenes 1 and 2</Description>
            <Slope>1 1 0.9</Slope>
            <Offset>-0.03 -0.02 0</Offset>
            <Power>1.25 1 1</Power>
        </SOPNode>
        <SatNode>
            <Description>boosting sat</Description>
            <Saturation>1.7</Saturation>
        </SatNode>
    </ColorCorrection>
    <ColorCorrection id="cc0002">
        <Description>CC-level description 2a</Description>
        <Description>CC-level description 2b</Description>
        <InputDescription>CC-level input description 2</InputDescription>
        <ViewingDescription>CC-level viewing description 2</ViewingDescription>
        <SOPNode>
            <Description>pastel</Description>
            <Description>another example</Description>
            <Slope>0.9 0.7 0.6</Slope>
            <Offset>0.1 0.1 0.1</Offset>
            <Power>0.9 0.9 0.9</Power>
        </SOPNode>
        <SatNode>
            <Description>dropping sat</Description>
            <Saturation>0.7</Saturation>
        </SatNode>
    </ColorCorrection>
    <ColorCorrection id="cc0003">
        <Description>CC-level description 3</Description>
        <InputDescription>CC-level input description 3</InputDescription>
        <ViewingDescription>CC-level viewing description 3</ViewingDescription>
        <SOPNode>
            <Description>golden</Description>
            <Slope>1.2 1.1 1</Slope>
            <Offset>0 0 0</Offset>
            <Power>0.9 1 1.2</Power>
        </SOPNode>
        <SatNode>
            <Description>no sat change</Description>
            <Description>sat==1</Description>
            <Saturation>1</Saturation>
        </SatNode>
    </ColorCorrection>
    <ColorCorrection>
        <SOPNode>
            <Slope>4 5 6</Slope>
            <Offset>0 0 0</Offset>
            <Power>0.9 1 1.2</Power>
        </SOPNode>
        <SatNode>
            <Saturation>1</Saturation>
        </SatNode>
    </ColorCorrection>
    <ColorCorrection>
        <SOPNode>
            <Slope>1 1 1</Slope>
            <Offset>0 0 0</Offset>
            <Power>1 1 1</Power>
        </SOPNode>
        <SatNode>
            <Saturation>0</Saturation>
        </SatNode>
    </ColorCorrection>
</ColorCorrectionCollection>
"#;
    assert_eq!(out, expected);
}

// ---------------------------------------------------------------------------
// FileFormatCDL

#[test]
fn test_cdl() {
    let cached = load(&CdlFormat, "cdl_test1.cdl").unwrap();
    assert!(cached.is_cdl_collection);

    // Descriptive element children of <ColorDecisionList> are preserved.
    check_children(
        &cached.group.metadata,
        &[
            ("Description", "This is a color decision list example."),
            (
                "InputDescription",
                "These should be applied in ACESproxy color space.",
            ),
            (
                "ViewingDescription",
                "View using the ACES RRT+ODT transforms.",
            ),
            ("Description", "It includes all possible description uses."),
        ],
    );

    let v = cdls(&cached);
    assert_eq!(v.len(), 5);
    assert_eq!(v.iter().filter(|c| !c.id().is_empty()).count(), 3);

    // Note: Descriptive elements that are children of <ColorDecision> are
    // not preserved.
    assert_eq!(v[0].id(), "cc0001");
    check_children(
        &v[0].metadata,
        &[
            ("Description", "CC-level description 1"),
            ("InputDescription", "CC-level input description 1"),
            ("ViewingDescription", "CC-level viewing description 1"),
            ("SOPDescription", "Example look"),
            ("SOPDescription", "For scenes 1 and 2"),
            ("SATDescription", "boosting sat"),
        ],
    );
    check_sop(
        v[0],
        [1.0, 1.0, 0.9],
        [-0.03, -0.02, 0.0],
        [1.25, 1.0, 1.0],
        1.7,
    );

    assert_eq!(v[1].id(), "cc0002");
    check_children(
        &v[1].metadata,
        &[
            ("Description", "CC-level description 2"),
            ("InputDescription", "CC-level input description 2"),
            ("ViewingDescription", "CC-level viewing description 2"),
            ("SOPDescription", "pastel"),
            ("SOPDescription", "another example"),
            ("SATDescription", "dropping sat"),
        ],
    );
    check_sop(v[1], [0.9, 0.7, 0.6], [0.1, 0.1, 0.1], [0.9, 0.9, 0.9], 0.7);

    assert_eq!(v[2].id(), "cc0003");
    check_children(
        &v[2].metadata,
        &[
            ("Description", "CC-level description 3"),
            ("InputDescription", "CC-level input description 3"),
            ("ViewingDescription", "CC-level viewing description 3"),
            ("SOPDescription", "golden"),
            ("SATDescription", "no sat change"),
            ("SATDescription", "sat==1"),
        ],
    );
    check_sop(v[2], [1.2, 1.1, 1.0], [0.0; 3], [0.9, 1.0, 1.2], 1.0);

    assert_eq!(v[3].id(), "");
    assert_eq!(v[3].metadata.children.len(), 0);
    // SatNode missing from XML, uses a default of 1.0.
    check_sop(v[3], [1.2, 1.1, 1.0], [0.0; 3], [0.9, 1.0, 1.2], 1.0);

    assert_eq!(v[4].id(), "");
    assert_eq!(v[4].metadata.children.len(), 0);
    // SOPNode missing from XML, uses default values.
    check_sop(v[4], [1.0; 3], [0.0; 3], [1.0; 3], 0.0);
}

#[test]
fn cdl_write() {
    // Metadata in ColorDecisionList and in ColorCorrection are preserved,
    // but not inside ColorDecision.
    let group = CdlTransform::create_group_from_file(&test_path("cdl_test1.cdl")).unwrap();
    let out = write(&group, FILEFORMAT_COLOR_DECISION_LIST).unwrap();
    let expected = r#"<ColorDecisionList xmlns="urn:ASC:CDL:v1.01">
    <Description>This is a color decision list example.</Description>
    <Description>It includes all possible description uses.</Description>
    <InputDescription>These should be applied in ACESproxy color space.</InputDescription>
    <ViewingDescription>View using the ACES RRT+ODT transforms.</ViewingDescription>
    <ColorDecision>
        <ColorCorrection id="cc0001">
            <Description>CC-level description 1</Description>
            <InputDescription>CC-level input description 1</InputDescription>
            <ViewingDescription>CC-level viewing description 1</ViewingDescription>
            <SOPNode>
                <Description>Example look</Description>
                <Description>For scenes 1 and 2</Description>
                <Slope>1 1 0.9</Slope>
                <Offset>-0.03 -0.02 0</Offset>
                <Power>1.25 1 1</Power>
            </SOPNode>
            <SatNode>
                <Description>boosting sat</Description>
                <Saturation>1.7</Saturation>
            </SatNode>
        </ColorCorrection>
    </ColorDecision>
    <ColorDecision>
        <ColorCorrection id="cc0002">
            <Description>CC-level description 2</Description>
            <InputDescription>CC-level input description 2</InputDescription>
            <ViewingDescription>CC-level viewing description 2</ViewingDescription>
            <SOPNode>
                <Description>pastel</Description>
                <Description>another example</Description>
                <Slope>0.9 0.7 0.6</Slope>
                <Offset>0.1 0.1 0.1</Offset>
                <Power>0.9 0.9 0.9</Power>
            </SOPNode>
            <SatNode>
                <Description>dropping sat</Description>
                <Saturation>0.7</Saturation>
            </SatNode>
        </ColorCorrection>
    </ColorDecision>
    <ColorDecision>
        <ColorCorrection id="cc0003">
            <Description>CC-level description 3</Description>
            <InputDescription>CC-level input description 3</InputDescription>
            <ViewingDescription>CC-level viewing description 3</ViewingDescription>
            <SOPNode>
                <Description>golden</Description>
                <Slope>1.2 1.1 1</Slope>
                <Offset>0 0 0</Offset>
                <Power>0.9 1 1.2</Power>
            </SOPNode>
            <SatNode>
                <Description>no sat change</Description>
                <Description>sat==1</Description>
                <Saturation>1</Saturation>
            </SatNode>
        </ColorCorrection>
    </ColorDecision>
    <ColorDecision>
        <ColorCorrection>
            <SOPNode>
                <Slope>1.2 1.1 1</Slope>
                <Offset>0 0 0</Offset>
                <Power>0.9 1 1.2</Power>
            </SOPNode>
            <SatNode>
                <Saturation>1</Saturation>
            </SatNode>
        </ColorCorrection>
    </ColorDecision>
    <ColorDecision>
        <ColorCorrection>
            <SOPNode>
                <Slope>1 1 1</Slope>
                <Offset>0 0 0</Offset>
                <Power>1 1 1</Power>
            </SOPNode>
            <SatNode>
                <Saturation>0</Saturation>
            </SatNode>
        </ColorCorrection>
    </ColorDecision>
</ColorDecisionList>
"#;
    assert_eq!(out, expected);

    // Write failures.
    // Empty group.
    let mut group = GroupTransform::new();
    check_err(
        write(&group, FILEFORMAT_COLOR_DECISION_LIST),
        "there should be at least one CDL",
    );
    // Only CDL.
    group.transforms.push(Transform::Range(Default::default()));
    check_err(
        write(&group, FILEFORMAT_COLOR_DECISION_LIST),
        "only CDL can be written",
    );
}

// ---------------------------------------------------------------------------
// CDLTransform (file loading)

#[test]
fn create_from_cc_file() {
    let file_path = test_path("cdl_test1.cc");
    {
        let t = CdlTransform::create_from_file(&file_path, "").unwrap();
        assert_eq!(t.id(), "foo");
        assert_eq!(t.first_sop_description(), "this is a description");
        assert_eq!(t.style, crate::types::CdlStyle::NoClamp);
        check_sop(&t, [1.1, 1.2, 1.3], [2.1, 2.2, 2.3], [3.1, 3.2, 3.3], 0.7);
    }
    CdlTransform::create_from_file(&file_path, "foo").unwrap();
    CdlTransform::create_from_file(&file_path, "0").unwrap();
    // The cccid is case sensitive.
    check_err(
        CdlTransform::create_from_file(&file_path, "FOO"),
        "The specified CDL Id/Index 'FOO' could not be loaded from the file",
    );
    let group = CdlTransform::create_group_from_file(&file_path).unwrap();
    assert_eq!(group.transforms.len(), 1);
}

#[test]
fn create_from_ccc_file() {
    let file_path = test_path("cdl_test1.ccc");
    {
        // Using ID.
        let t = CdlTransform::create_from_file(&file_path, "cc0003").unwrap();
        assert_eq!(t.id(), "cc0003");
        assert_eq!(t.style, crate::types::CdlStyle::NoClamp);
        assert_eq!(t.first_sop_description(), "golden");
        check_sop(&t, [1.2, 1.1, 1.0], [0.0; 3], [0.9, 1.0, 1.2], 1.0);
    }
    {
        // Using 0 based index.
        let t = CdlTransform::create_from_file(&file_path, "3").unwrap();
        assert_eq!(t.id(), "");
        assert_eq!(t.style, crate::types::CdlStyle::NoClamp);
        check_sop(&t, [4.0, 5.0, 6.0], [0.0; 3], [0.9, 1.0, 1.2], 1.0);
    }
    {
        // No ID: return the first one.
        let t = CdlTransform::create_from_file(&file_path, "").unwrap();
        assert_eq!(t.id(), "cc0001");
    }
    let group = CdlTransform::create_group_from_file(&file_path).unwrap();
    assert_eq!(group.transforms.len(), 5);
    // Wrong ID.
    check_err(
        CdlTransform::create_from_file(&file_path, "NotFound"),
        "could not be loaded from the file",
    );
    // Wrong index.
    check_err(
        CdlTransform::create_from_file(&file_path, "42"),
        "is outside the valid range for this file [0,4]",
    );
}

#[test]
fn create_from_cdl_file() {
    let file_path = test_path("cdl_test1.cdl");
    let t = CdlTransform::create_from_file(&file_path, "cc0003").unwrap();
    assert_eq!(t.id(), "cc0003");
    assert_eq!(t.style, crate::types::CdlStyle::NoClamp);
    let group = CdlTransform::create_group_from_file(&file_path).unwrap();
    assert_eq!(group.transforms.len(), 5);
}

/// A temporary file removed on drop (port of `FileGuard`).
struct FileGuard {
    path: std::path::PathBuf,
}

impl FileGuard {
    fn new(tag: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("ocio_rs_cdl_test_{}_{}", std::process::id(), tag));
        Self { path }
    }

    fn write(&self, content: &str) {
        std::fs::write(&self.path, content).unwrap();
    }

    fn name(&self) -> String {
        self.path.to_string_lossy().into_owned()
    }
}

impl Drop for FileGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[test]
fn escape_xml() {
    let input = "<ColorCorrection id=\"Esc &lt; &amp; &quot; &apos; &gt;\">\n\
    <SOPNode>\n\
        <Description>These: &lt; &amp; &quot; &apos; &gt; are escape chars</Description>\n\
        <Slope>1.1 1.2 1.3</Slope>\n\
        <Offset>2.1 2.2 2.3</Offset>\n\
        <Power>3.1 3.2 3.3</Power>\n\
    </SOPNode>\n\
    <SatNode>\n\
        <Saturation>0.7</Saturation>\n\
    </SatNode>\n\
</ColorCorrection>";
    let guard = FileGuard::new("escape_xml");
    guard.write(input);
    let t = CdlTransform::create_from_file(&guard.name(), "").unwrap();
    assert_eq!(t.id(), "Esc < & \" ' >");
    assert_eq!(t.style, crate::types::CdlStyle::NoClamp);
    assert_eq!(
        t.first_sop_description(),
        "These: < & \" ' > are escape chars"
    );
}

const CONTENTS_A: &str = "<ColorCorrectionCollection>
    <ColorCorrection id=\"cc03343\">
        <SOPNode>
            <Slope>0.1 0.2 0.3 </Slope>
            <Offset>0.8 0.1 0.3 </Offset>
            <Power>0.5 0.5 0.5 </Power>
        </SOPNode>
        <SATNode>
            <Saturation>1</Saturation>
        </SATNode>
    </ColorCorrection>
    <ColorCorrection id=\"cc03344\">
        <SOPNode>
            <Slope>1.2 1.3 1.4 </Slope>
            <Offset>0.3 0 0 </Offset>
            <Power>0.75 0.75 0.75 </Power>
        </SOPNode>
        <SATNode>
            <Saturation>1</Saturation>
        </SATNode>
    </ColorCorrection>
</ColorCorrectionCollection>
";

#[test]
fn clear_caches() {
    // Files are cached: a modified file is only reloaded once the caches
    // are cleared.
    let guard = FileGuard::new("clear_caches");
    guard.write(CONTENTS_A);
    let t = CdlTransform::create_from_file(&guard.name(), "cc03343").unwrap();
    assert_eq!(t.slope, [0.1, 0.2, 0.3]);

    guard.write(&CONTENTS_A.replacen("0.1 0.2 0.3", "1.1 2.2 3.3", 1));
    let t = CdlTransform::create_from_file(&guard.name(), "cc03343").unwrap();
    assert_eq!(t.slope, [0.1, 0.2, 0.3]);

    crate::fileformats::file_transform::clear_file_transform_caches();
    let t = CdlTransform::create_from_file(&guard.name(), "cc03343").unwrap();
    assert_eq!(t.slope, [1.1, 2.2, 3.3]);
}

#[test]
fn faulty_file_content() {
    let guard = FileGuard::new("faulty_file_content");
    {
        guard.write(&format!("{CONTENTS_A}Some Extra faulty information"));
        check_err(
            CdlTransform::create_from_file(&guard.name(), "cc03343"),
            "All formats have been tried",
        );
    }
    {
        // Duplicated identifier.
        guard.write(&CONTENTS_A.replacen("cc03344", "cc03343", 1));
        check_err(
            CdlTransform::create_from_file(&guard.name(), "cc03343"),
            "All formats have been tried",
        );
        let data = std::fs::read(&guard.path).unwrap();
        check_err(
            CccFormat.read(&data, &guard.name(), Interpolation::Default),
            "Error loading ccc xml. Duplicate elements with 'cc03343' found",
        );
    }
    check_err(
        CdlTransform::create_from_file(&test_path("cdl_various.ctf"), "0"),
        "Not a CDL file format",
    );
    check_err(
        CdlTransform::create_from_file("", "0"),
        "Error loading CDL. Source file not specified.",
    );
}

// ---------------------------------------------------------------------------
// Parser details.

#[test]
fn parser_errors() {
    // Missing CDL tag.
    check_err(
        parser::parse_cdl(b"<Other/>", "file.cc"),
        "Error parsing ColorDecisionList (file.cc). Error is: Missing CDL tag. At line (0)",
    );
    // Mismatched tags.
    let e = parser::parse_cdl(
        b"<ColorCorrection>\n<SOPNode>\n</SatNode>\n</ColorCorrection>\n",
        "f.cc",
    )
    .unwrap_err();
    assert_eq!(
        e.message(),
        "Error parsing ColorCorrection (f.cc). Error is: XML parsing error (no closing tag for 'SOPNode'). . At line (3)"
    );
    // Invalid SOP values.
    check_err(
        parser::parse_cdl(
            b"<ColorCorrection>\n<SOPNode>\n<Slope>1 2</Slope>\n<Offset>0 0 0</Offset>\n<Power>1 1 1</Power>\n</SOPNode>\n</ColorCorrection>\n",
            "f.cc",
        ),
        "At line 3: SOPNode: 3 values required.",
    );
    // Missing SOP value.
    check_err(
        parser::parse_cdl(
            b"<ColorCorrection>\n<SOPNode>\n<Slope>1 2 3</Slope>\n</SOPNode>\n</ColorCorrection>\n",
            "f.cc",
        ),
        "Required node 'Offset' is missing.",
    );
    // Invalid power.
    check_err(
        parser::parse_cdl(
            b"<ColorCorrection>\n<SOPNode>\n<Slope>1 1 1</Slope>\n<Offset>0 0 0</Offset>\n<Power>1 0 1</Power>\n</SOPNode>\n</ColorCorrection>\n",
            "f.cc",
        ),
        "CDLTransform validation failed: CDLOpData: Invalid 'power' 0 should be greater than 0.",
    );
    // Text in a container.
    check_err(
        parser::parse_cdl(b"<ColorCorrection>\n text \n</ColorCorrection>\n", "f.cc"),
        "Illegal attribute ( text )",
    );
}

#[test]
fn parser_warnings() {
    let parsed = parser::parse_cdl(
        b"<ColorCorrectionCollection>\n<Unknown/>\n<ColorCorrection>\n<Slope>1 1 1</Slope>\n</ColorCorrection>\n</ColorCorrectionCollection>\n",
        "f.ccc",
    )
    .unwrap();
    assert_eq!(parsed.warnings.len(), 2);
    assert_eq!(
        parsed.warnings[0],
        "f.ccc(2): Unrecognized element 'Unknown' where its parent is 'ColorCorrectionCollection' (1): : Unknown element."
    );
    assert_eq!(
        parsed.warnings[1],
        "f.ccc(4): Unrecognized element 'Slope' where its parent is 'ColorCorrection' (3): : Slope, Offset or Power tags must be under SOPNode."
    );
    assert!(parsed.is_ccc);
    assert_eq!(parsed.info.transforms.len(), 1);
}

#[test]
fn write_format_names() {
    let names = GroupTransform::write_format_names();
    assert_eq!(names.len(), GroupTransform::num_write_formats());
    for n in [
        crate::fileformats::FILEFORMAT_CLF,
        crate::fileformats::FILEFORMAT_CTF,
        FILEFORMAT_COLOR_CORRECTION,
        FILEFORMAT_COLOR_CORRECTION_COLLECTION,
        FILEFORMAT_COLOR_DECISION_LIST,
    ] {
        assert!(names.contains(&n), "{n}");
    }
    for (i, n) in names.iter().enumerate() {
        assert_eq!(GroupTransform::format_name_by_index(i), *n);
        assert!(!GroupTransform::format_extension_by_index(i).is_empty());
    }
    assert_eq!(GroupTransform::format_name_by_index(names.len()), "");
    assert_eq!(GroupTransform::format_extension_by_index(names.len()), "");

    check_err(
        GroupTransform::new().write(&Config::create_raw(), "unknown"),
        "The format named 'unknown' could not be found. ",
    );
    check_err(
        GroupTransform::new().write(&Config::create_raw(), FILEFORMAT_COLOR_CORRECTION),
        "Error writing format 'ColorCorrection': CDL write: there should be a single CDL.",
    );
}
