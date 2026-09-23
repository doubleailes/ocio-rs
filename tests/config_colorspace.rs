//! Port of `ColorSpace_tests.cpp` and `ColorSpaceSet_tests.cpp`.

mod config_common;

use ocio::config::logging::LogGuard;
use ocio::config::{ColorSpace, ColorSpaceSet};
use ocio::*;

#[test]
fn colorspace_basic() {
    let cs = ColorSpace::default();
    assert_eq!(cs.reference_space_type(), ReferenceSpaceType::Scene);
    let cs = ColorSpace::new(ReferenceSpaceType::Display);
    assert_eq!(cs.reference_space_type(), ReferenceSpaceType::Display);
    let mut cs = ColorSpace::new(ReferenceSpaceType::Scene);
    assert_eq!(cs.reference_space_type(), ReferenceSpaceType::Scene);

    assert_eq!(cs.name(), "");
    assert_eq!(cs.num_aliases(), 0);
    assert_eq!(cs.alias(0), "");
    assert_eq!(cs.family(), "");
    assert_eq!(cs.description(), "");
    assert_eq!(cs.equality_group(), "");
    assert_eq!(cs.encoding(), "");
    assert_eq!(cs.bit_depth(), BitDepth::Unknown);
    assert!(!cs.is_data());
    assert_eq!(cs.allocation(), Allocation::Uniform);
    assert_eq!(cs.allocation_num_vars(), 0);

    cs.set_name("NAME");
    cs.set_description("DESC");
    cs.set_family("FAMILY");
    cs.set_equality_group("EQGRP");
    cs.set_encoding("ENC");
    cs.set_interop_id("interop").unwrap();
    cs.set_interchange_attribute("amf_transform_ids", "AMF")
        .unwrap();
    cs.set_interchange_attribute("icc_profile_name", "ICC")
        .unwrap();

    cs.set_name("");
    cs.set_description("");
    cs.set_family("");
    cs.set_equality_group("");
    cs.set_encoding("");
    cs.set_interop_id("").unwrap();
    cs.set_interchange_attribute("amf_transform_ids", "")
        .unwrap();
    cs.set_interchange_attribute("icc_profile_name", "")
        .unwrap();
    assert!(cs.name().is_empty());
    assert!(cs.description().is_empty());
    assert!(cs.family().is_empty());
    assert!(cs.equality_group().is_empty());
    assert!(cs.encoding().is_empty());
    assert!(cs.interop_id().is_empty());
    assert!(cs
        .interchange_attribute("amf_transform_ids")
        .unwrap()
        .is_empty());
    assert!(cs
        .interchange_attribute("icc_profile_name")
        .unwrap()
        .is_empty());

    cs.set_name("name");
    assert_eq!(cs.name(), "name");
    cs.set_family("family");
    assert_eq!(cs.family(), "family");
    cs.set_description("description");
    assert_eq!(cs.description(), "description");
    cs.set_equality_group("equalitygroup");
    assert_eq!(cs.equality_group(), "equalitygroup");
    cs.set_encoding("encoding");
    assert_eq!(cs.encoding(), "encoding");
    cs.set_bit_depth(BitDepth::F16);
    assert_eq!(cs.bit_depth(), BitDepth::F16);
    cs.set_is_data(true);
    assert!(cs.is_data());
    cs.set_allocation(Allocation::Unknown);
    assert_eq!(cs.allocation(), Allocation::Unknown);
    cs.set_allocation_vars(&[1.0, 2.0]);
    assert_eq!(cs.allocation_num_vars(), 2);
    assert_eq!(cs.allocation_vars(), &[1.0, 2.0]);
    cs.set_interop_id("interop_id").unwrap();
    assert_eq!(cs.interop_id(), "interop_id");
    cs.set_interchange_attribute("amf_transform_ids", "amf_transform_id1\namf_transform_id2")
        .unwrap();
    assert_eq!(
        cs.interchange_attribute("amf_transform_ids").unwrap(),
        "amf_transform_id1\namf_transform_id2"
    );
    cs.set_interchange_attribute("icc_profile_name", "icc_profile_name")
        .unwrap();
    assert_eq!(
        cs.interchange_attribute("icc_profile_name").unwrap(),
        "icc_profile_name"
    );

    assert_eq!(cs.to_string().len(), 306);
}

#[test]
fn colorspace_alias() {
    let mut cs = ColorSpace::default();
    assert_eq!(cs.num_aliases(), 0);
    let (a, a_alt, b) = ("aliasA", "aLiaSa", "aliasB");
    cs.add_alias(a);
    assert_eq!(cs.num_aliases(), 1);
    assert!(cs.has_alias(a));
    assert!(cs.has_alias(a_alt));
    assert!(!cs.has_alias(b));
    cs.add_alias(b);
    assert_eq!(cs.num_aliases(), 2);
    assert_eq!(cs.alias(0), a);
    assert_eq!(cs.alias(1), b);
    assert!(cs.has_alias(b));

    cs.add_alias(a_alt);
    assert_eq!(cs.num_aliases(), 2);
    assert_eq!(cs.alias(0), a);
    assert_eq!(cs.alias(1), b);

    cs.remove_alias(a_alt);
    assert_eq!(cs.num_aliases(), 1);
    assert_eq!(cs.alias(0), b);
    assert!(!cs.has_alias(a));
    assert!(!cs.has_alias(a_alt));

    cs.add_alias(a_alt);
    assert_eq!(cs.num_aliases(), 2);
    assert_eq!(cs.alias(0), b);
    assert_eq!(cs.alias(1), a_alt);
    assert!(cs.has_alias(a));
    assert!(cs.has_alias(a_alt));

    cs.set_name(a);
    assert_eq!(cs.name(), a);
    assert_eq!(cs.num_aliases(), 1);
    assert_eq!(cs.alias(0), b);
    assert!(!cs.has_alias(a));
    assert!(!cs.has_alias(a_alt));

    cs.add_alias(a_alt);
    assert_eq!(cs.name(), a);
    assert_eq!(cs.num_aliases(), 1);
    assert_eq!(cs.alias(0), b);
    assert!(!cs.has_alias(a_alt));

    cs.add_alias("other");
    assert_eq!(cs.num_aliases(), 2);
    assert!(cs.has_alias("other"));
    cs.clear_aliases();
    assert_eq!(cs.num_aliases(), 0);
    assert!(!cs.has_alias(b));
    assert!(!cs.has_alias("other"));
}

#[test]
fn colorspace_category() {
    let mut cs = ColorSpace::default();
    assert_eq!(cs.num_categories(), 0);
    assert!(!cs.has_category("linear"));
    assert!(!cs.has_category("rendering"));
    assert!(!cs.has_category("log"));
    cs.add_category("linear");
    cs.add_category("rendering");
    assert_eq!(cs.num_categories(), 2);
    assert!(cs.has_category("linear"));
    assert!(cs.has_category("rendering"));
    assert!(!cs.has_category("log"));
    assert_eq!(cs.category(0), Some("linear"));
    assert_eq!(cs.category(1), Some("rendering"));
    assert_eq!(cs.category(2), None);
    cs.remove_category("linear");
    assert_eq!(cs.num_categories(), 1);
    assert!(!cs.has_category("linear"));
    assert!(cs.has_category("rendering"));
    cs.remove_category("log");
    assert_eq!(cs.num_categories(), 1);
    assert!(cs.has_category("rendering"));
    cs.clear_categories();
    assert_eq!(cs.num_categories(), 0);
}

const START: &str = r#"ocio_profile_version: 2

environment:
  {}
search_path: ""
strictparsing: false
luma: [0.2126, 0.7152, 0.0722]

roles:
  default: raw

file_rules:
  - !<Rule> {name: ColorSpaceNamePathSearch}
  - !<Rule> {name: Default, colorspace: default}

displays:
  sRGB:
    - !<View> {name: Raw, colorspace: raw}

active_displays: []
active_views: []

"#;

#[test]
fn config_color_space_serialize_raw() {
    let end = r#"colorspaces:
  - !<ColorSpace>
    name: raw
    family: raw
    equalitygroup: ""
    bitdepth: 32f
    description: A raw color space. Conversions to and from this space are no-ops.
    isdata: true
    allocation: uniform
"#;
    let cfg_string = format!("{START}{end}");
    let config = Config::create_from_str(&cfg_string).unwrap();
    config.validate().unwrap();
    assert_eq!(config.num_color_spaces(), 1);
    let cs = config
        .get_color_space(config.color_space_name_by_index(0))
        .unwrap();
    assert_eq!(cs.allocation(), Allocation::Uniform);
    assert_eq!(cs.allocation_num_vars(), 0);
    assert_eq!(cs.bit_depth(), BitDepth::F32);
    assert_eq!(
        cs.description(),
        "A raw color space. Conversions to and from this space are no-ops."
    );
    assert_eq!(cs.encoding(), "");
    assert_eq!(cs.equality_group(), "");
    assert_eq!(cs.family(), "raw");
    assert_eq!(cs.name(), "raw");
    assert_eq!(cs.num_categories(), 0);
    assert_eq!(cs.reference_space_type(), ReferenceSpaceType::Scene);
    assert!(cs.transform(ColorSpaceDirection::ToReference).is_none());
    assert!(cs.transform(ColorSpaceDirection::FromReference).is_none());
    assert!(cs.is_data());
    assert_eq!(config.serialize().unwrap(), cfg_string);
}

#[test]
fn config_color_space_serialize_all_params() {
    let end = r#"colorspaces:
  - !<ColorSpace>
    name: raw
    family: raw
    equalitygroup: ""
    bitdepth: 32f
    description: A raw color space. Conversions to and from this space are no-ops.
    isdata: true
    allocation: uniform

  - !<ColorSpace>
    name: colorspace
    aliases: [alias1, alias2]
    family: family
    equalitygroup: group
    bitdepth: 16f
    description: |
      A raw color space.
      Second line.
    isdata: false
    categories: [one, two]
    encoding: scene-linear
    allocation: lg2
    allocationvars: [0.1, 0.9, 0.15]
    to_scene_reference: !<LogTransform> {}
    from_scene_reference: !<LogTransform> {}
"#;
    let cfg_string = format!("{START}{end}");
    let config = Config::create_from_str(&cfg_string).unwrap();
    config.validate().unwrap();
    assert_eq!(config.num_color_spaces(), 2);
    let cs = config
        .get_color_space(config.color_space_name_by_index(1))
        .unwrap();
    assert_eq!(cs.allocation(), Allocation::Lg2);
    assert_eq!(cs.allocation_vars(), &[0.1f32, 0.9, 0.15]);
    assert_eq!(cs.bit_depth(), BitDepth::F16);
    assert_eq!(cs.description(), "A raw color space.\nSecond line.");
    assert_eq!(cs.encoding(), "scene-linear");
    assert_eq!(cs.equality_group(), "group");
    assert_eq!(cs.family(), "family");
    assert_eq!(cs.name(), "colorspace");
    assert_eq!(cs.num_aliases(), 2);
    assert_eq!(cs.alias(0), "alias1");
    assert_eq!(cs.alias(1), "alias2");
    assert_eq!(cs.num_categories(), 2);
    assert_eq!(cs.category(0), Some("one"));
    assert_eq!(cs.category(1), Some("two"));
    assert!(cs.transform(ColorSpaceDirection::ToReference).is_some());
    assert!(cs.transform(ColorSpaceDirection::FromReference).is_some());
    assert!(!cs.is_data());
    assert_eq!(config.serialize().unwrap(), cfg_string);
}

#[test]
fn config_color_space_serialize_description_newlines() {
    let end = r#"colorspaces:
  - !<ColorSpace>
    name: raw
    family: raw
    equalitygroup: ""
    bitdepth: 32f
    description: Some text.
    isdata: true
    allocation: uniform

  - !<ColorSpace>
    name: raw2
    family: raw
    equalitygroup: ""
    bitdepth: 32f
    description: |
      One line.

      Other line.
    isdata: true
    allocation: uniform
"#;
    let cfg_string = format!("{START}{end}");
    let config = Config::create_from_str(&cfg_string).unwrap();
    config.validate().unwrap();
    assert_eq!(config.num_color_spaces(), 2);
    let cs = config
        .get_color_space(config.color_space_name_by_index(0))
        .unwrap();
    assert_eq!(cs.description(), "Some text.");
    let cs = config
        .get_color_space(config.color_space_name_by_index(1))
        .unwrap();
    assert_eq!(cs.description(), "One line.\n\nOther line.");
    assert_eq!(config.serialize().unwrap(), cfg_string);

    let mut cs_edit = cs.clone();
    cs_edit.set_description("One line.\n\nOther line.\n");
    let mut config_edit = config.clone();
    config_edit.add_color_space(&cs_edit).unwrap();
    assert_eq!(config_edit.serialize().unwrap(), cfg_string);

    cs_edit.set_description("One line.\n\nOther line.\n\n\n\n");
    config_edit.add_color_space(&cs_edit).unwrap();
    assert_eq!(config_edit.serialize().unwrap(), cfg_string);

    let cs = config
        .get_color_space(config.color_space_name_by_index(0))
        .unwrap();
    assert_eq!(cs.description(), "Some text.");
    let mut cs_edit = cs.clone();
    cs_edit.set_description("Some text.\n\n\n");
    config_edit.add_color_space(&cs_edit).unwrap();
    assert_eq!(config_edit.serialize().unwrap(), cfg_string);
}

#[test]
fn config_color_space_serialize_description_styles() {
    let end = r#"colorspaces:
  - !<ColorSpace>
    name: raw
    description: |
      "Some text."

  - !<ColorSpace>
    name: raw2
    description: "Multiple lines\n\nOther line.\n\n\n"

  - !<ColorSpace>
    name: raw3
    description: |
      Test \n backslash+n.

  - !<ColorSpace>
    name: raw4
    description: "One"

  - !<ColorSpace>
    name: raw5
    description: More "than" one

  - !<ColorSpace>
    name: raw6
    description: Other \n test.

  - !<ColorSpace>
    name: raw7
    description: Double backslash+n \\n test.

  - !<ColorSpace>
    name: raw8
    description: "Double backslash+n \\n in quotes."
"#;
    let cfg_string = format!("{START}{end}");
    let config = Config::create_from_str(&cfg_string).unwrap();
    config.validate().unwrap();
    assert_eq!(config.num_color_spaces(), 8);
    let desc = |i: usize| {
        config
            .get_color_space(config.color_space_name_by_index(i))
            .unwrap()
            .description()
            .to_string()
    };
    assert_eq!(desc(0), "\"Some text.\"");
    assert_eq!(desc(1), "Multiple lines\n\nOther line.");
    assert_eq!(desc(2), "Test \\n backslash+n.");
    assert_eq!(desc(3), "One");
    assert_eq!(desc(4), "More \"than\" one");
    assert_eq!(desc(5), "Other \\n test.");
    assert_eq!(desc(6), "Double backslash+n \\\\n test.");
    assert_eq!(desc(7), "Double backslash+n \\n in quotes.");

    let end_res = r#"colorspaces:
  - !<ColorSpace>
    name: raw
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    description: "\"Some text.\""
    isdata: false
    allocation: uniform

  - !<ColorSpace>
    name: raw2
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    description: |
      Multiple lines

      Other line.
    isdata: false
    allocation: uniform

  - !<ColorSpace>
    name: raw3
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    description: Test \n backslash+n.
    isdata: false
    allocation: uniform

  - !<ColorSpace>
    name: raw4
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    description: One
    isdata: false
    allocation: uniform

  - !<ColorSpace>
    name: raw5
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    description: More "than" one
    isdata: false
    allocation: uniform

  - !<ColorSpace>
    name: raw6
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    description: Other \n test.
    isdata: false
    allocation: uniform

  - !<ColorSpace>
    name: raw7
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    description: Double backslash+n \\n test.
    isdata: false
    allocation: uniform

  - !<ColorSpace>
    name: raw8
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    description: Double backslash+n \n in quotes.
    isdata: false
    allocation: uniform
"#;
    assert_eq!(config.serialize().unwrap(), format!("{START}{end_res}"));
}

#[test]
fn config_color_space_interop_id_and_interchange() {
    // interop_id is valid in a v2.0 config.
    let end = r#"colorspaces:
  - !<ColorSpace>
    name: raw
    aliases: [ data ]
    interop_id: data
    family: raw
    equalitygroup: ""
    bitdepth: 32f
    description: Some text.
    isdata: true
    allocation: uniform
"#;
    let config = Config::create_from_str(&format!("{START}{end}")).unwrap();
    assert_eq!(config.get_color_space("raw").unwrap().interop_id(), "data");
    config.validate().unwrap();

    // Undefined interop_id does not pass validation.
    let end = r#"colorspaces:
  - !<ColorSpace>
    name: raw
    interop_id: data
    family: raw
    equalitygroup: ""
    bitdepth: 32f
    description: Some text.
    isdata: true
    allocation: uniform
"#;
    let config = Config::create_from_str(&format!("{START}{end}")).unwrap();
    assert_eq!(config.get_color_space("raw").unwrap().interop_id(), "data");
    assert_err!(
        config.validate(),
        "Config failed color space validation. The color space 'raw' refers to an interop ID, 'data', which is not a color space name or alias."
    );

    // The interop id can be found in another color space.
    let end = r#"colorspaces:
  - !<ColorSpace>
    name: raw
    interop_id: data
    family: raw
    equalitygroup: ""
    bitdepth: 32f
    description: one data color space.
    isdata: true
    allocation: uniform
  - !<ColorSpace>
    name: data
    interop_id: data
    family: raw
    equalitygroup: ""
    bitdepth: 32f
    description: another data color space.
    isdata: true
    allocation: uniform
"#;
    let config = Config::create_from_str(&format!("{START}{end}")).unwrap();
    config.validate().unwrap();

    // The interchange is not valid in a v2.0 config.
    let end = r#"colorspaces:
  - !<ColorSpace>
    name: raw
    interchange:
        amf_transform_ids: should NOT be valid in 2.0 config
    family: raw
    equalitygroup: ""
    bitdepth: 32f
    description: Some text.
    isdata: true
    allocation: uniform
"#;
    assert_err!(
        Config::create_from_str(&format!("{START}{end}")),
        "Config failed validation. The color space 'raw' has non-empty interchange attributes and config version is less than 2.5."
    );

    let start25 = START.replacen(
        "ocio_profile_version: 2\n",
        "ocio_profile_version: 2.5\n",
        1,
    );
    let end_amf = r#"
colorspaces:
  - !<ColorSpace>
    name: raw
    interchange:
        amf_transform_ids: This is valid in 2.5 config
    family: raw
    equalitygroup: ""
    bitdepth: 32f
    description: Some text.
    isdata: true
    allocation: uniform
"#;
    let config = Config::create_from_str(&format!("{start25}{end_amf}")).unwrap();
    assert_eq!(
        config
            .get_color_space("raw")
            .unwrap()
            .interchange_attributes()
            .len(),
        1
    );

    let end_unknown = r#"
colorspaces:
  - !<ColorSpace>
    name: raw
    interchange:
        my-attrib: will be ignored
    family: raw
    equalitygroup: ""
    bitdepth: 32f
    description: Some text.
    isdata: true
    allocation: uniform
"#;
    let guard = LogGuard::new();
    let config = Config::create_from_str(&format!("{start25}{end_unknown}")).unwrap();
    assert_eq!(
        guard.output(),
        "[OpenColorIO Warning]: Unknown key in interchange: 'my-attrib'.\n"
    );
    assert_eq!(
        config
            .get_color_space("raw")
            .unwrap()
            .interchange_attributes()
            .len(),
        0
    );
}

#[test]
fn config_use_alias() {
    let text = r#"ocio_profile_version: 2

environment:
  {}
search_path: ""
strictparsing: false
luma: [0.2126, 0.7152, 0.0722]

roles:
  testAlias: aces
  default: raw

file_rules:
  - !<Rule> {name: ColorSpaceNamePathSearch}
  - !<Rule> {name: Default, colorspace: default}

displays:
  sRGB:
    - !<View> {name: Raw, colorspace: aces}

active_displays: []
active_views: []

colorspaces:
  - !<ColorSpace>
    name: raw
    aliases: [ colorspaceAlias ]
    family: raw
    equalitygroup: ""
    bitdepth: 32f
    description: A raw color space. Conversions to and from this space are no-ops.
    isdata: true
    allocation: uniform

  - !<ColorSpace>
    name: colorspace
    aliases: [ aces, aces2065-1, ACES - ACES2065-1, "ACES AP0, scene-linear" ]
    family: family
    equalitygroup: group
    bitdepth: 16f
    description: |
      A raw color space.
      Second line.
    isdata: false
    categories: [one, two]
    encoding: scene-linear
    allocation: lg2
    allocationvars: [0.1, 0.9, 0.15]
    to_reference: !<LogTransform> {}
    from_reference: !<LogTransform> {}
"#;
    let config = Config::create_from_str(text).unwrap();
    config.validate().unwrap();
    assert_eq!(
        config.get_color_space("aces2065-1").unwrap().name(),
        "colorspace"
    );
    assert_eq!(
        config.get_color_space("ACES - ACES2065-1").unwrap().name(),
        "colorspace"
    );
    assert!(config.get_color_space("alias no valid").is_none());

    assert_eq!(config.canonical_name("aces"), "colorspace");
    assert_eq!(
        config.canonical_name("ACES AP0, scene-linear"),
        "colorspace"
    );
    assert_eq!(config.canonical_name("colorspace"), "colorspace");
    assert_eq!(config.canonical_name("default"), "raw");
    assert_eq!(config.canonical_name("DEFault"), "raw");
    assert_eq!(config.canonical_name("not an alias"), "");
    assert_eq!(config.canonical_name(""), "");

    assert_eq!(config.index_for_color_space("AceS"), Some(1));
    assert_eq!(config.index_for_color_space("aces2065-1"), Some(1));
    assert_eq!(config.index_for_color_space("not an alias"), None);

    assert_eq!(
        config.get_color_space("testAlias").unwrap().name(),
        "colorspace"
    );

    assert_eq!(
        config.color_space_from_filepath("test_aces_test"),
        "colorspace"
    );
    assert_eq!(
        config.color_space_from_filepath("skdj_ColorspaceAlias_dfjdk"),
        "raw"
    );

    let mut cfg = config.clone();
    cfg.set_inactive_color_spaces("colorspace");
    assert_eq!(
        cfg.color_space_from_filepath("test_aces_test"),
        "colorspace"
    );
}

#[test]
fn colorspace_interop_id() {
    let mut cs = ColorSpace::default();
    assert_eq!(cs.interop_id(), "");
    cs.set_interop_id("srgb_p3d65_scene").unwrap();
    assert_eq!(cs.interop_id(), "srgb_p3d65_scene");
    cs.set_interop_id("").unwrap();
    assert_eq!(cs.interop_id(), "");
    cs.set_interop_id("lin_rec2020_scene").unwrap();
    assert_eq!(cs.interop_id(), "lin_rec2020_scene");
    cs.set_interop_id("srgb_p3d65_scene").unwrap();
    let copy = cs.clone();
    assert_eq!(copy.interop_id(), "srgb_p3d65_scene");
    cs.set_interop_id("namespace:colorspace_name").unwrap();
    assert_eq!(cs.interop_id(), "namespace:colorspace_name");
    assert_err!(
        cs.set_interop_id("name:space:cs_name"),
        "Only one ':' is allowed to separate the namespace and the color space."
    );
    assert_err!(
        cs.set_interop_id("namespace:"),
        " If ':' is used, both the namespace and the color space parts must be non-empty."
    );
    assert_err!(
        cs.set_interop_id(":cs_name"),
        "If ':' is used, both the namespace and the color space parts must be non-empty."
    );
    for bad in [
        "café_scene",
        "UPPERCASE",
        "{curly_bracket}",
        "\\backslash",
        " space ",
    ] {
        assert_err!(
            cs.set_interop_id(bad),
            "Only lowercase a-z, 0-9 and . - _ ~ / * # % ^ + ( ) [ ] | are allowed."
        );
    }
}

#[test]
fn colorspace_interop_id_serialization() {
    let mut cfg = Config::create();
    let mut cs = ColorSpace::default();
    cs.set_name("test_colorspace");
    cs.set_interop_id("lin_rec709_scene").unwrap();
    cfg.add_color_space(&cs).unwrap();
    let yaml = cfg.serialize().unwrap();
    assert!(yaml.contains("interop_id"));
    assert!(yaml.contains("lin_rec709_scene"));
    let de = Config::create_from_str(&yaml).unwrap();
    assert_eq!(
        de.get_color_space("test_colorspace").unwrap().interop_id(),
        "lin_rec709_scene"
    );

    let mut copy = cfg.clone();
    copy.set_version(2, 0).unwrap();
    copy.serialize().unwrap();
    copy.set_version(1, 0).unwrap();
    assert_err!(
        copy.serialize(),
        "Config failed validation. The color space 'test_colorspace' has non-empty InteropID and config version is less than 2.0."
    );

    cs.set_interop_id("").unwrap();
    cfg.add_color_space(&cs).unwrap();
    assert!(!cfg.serialize().unwrap().contains("interop_id"));
}

#[test]
fn colorspace_amf_transform_ids() {
    let mut cs = ColorSpace::default();
    assert_eq!(cs.interchange_attribute("amf_transform_ids").unwrap(), "");
    let single = "urn:ampas:aces:transformId:v1.5:ACEScsc.Academy.ACEScc_to_ACES.a1.0.3";
    cs.set_interchange_attribute("amf_transform_ids", single)
        .unwrap();
    assert_eq!(
        cs.interchange_attribute("amf_transform_ids").unwrap(),
        single
    );
    cs.set_interchange_attribute("amf_transform_ids", "")
        .unwrap();
    assert_eq!(cs.interchange_attribute("amf_transform_ids").unwrap(), "");
    let multiple = "urn:ampas:aces:transformId:v1.5:ACEScsc.Academy.ACEScc_to_ACES.a1.0.3\n\
                    urn:ampas:aces:transformId:v1.5:ACEScsc.Academy.ACES_to_ACEScc.a1.0.3";
    cs.set_interchange_attribute("amf_transform_ids", multiple)
        .unwrap();
    assert_eq!(
        cs.interchange_attribute("amf_transform_ids").unwrap(),
        multiple
    );
    cs.set_interchange_attribute("amf_transform_ids", single)
        .unwrap();
    let copy = cs.clone();
    assert_eq!(
        copy.interchange_attribute("amf_transform_ids").unwrap(),
        single
    );
}

#[test]
fn colorspace_icc_profile_name() {
    let mut cs = ColorSpace::default();
    assert_eq!(cs.interchange_attribute("icc_profile_name").unwrap(), "");
    cs.set_interchange_attribute("icc_profile_name", "sRGB IEC61966-2.1")
        .unwrap();
    assert_eq!(
        cs.interchange_attribute("icc_profile_name").unwrap(),
        "sRGB IEC61966-2.1"
    );
    cs.set_interchange_attribute("icc_profile_name", "Adobe RGB (1998)")
        .unwrap();
    assert_eq!(
        cs.interchange_attribute("icc_profile_name").unwrap(),
        "Adobe RGB (1998)"
    );
    cs.set_interchange_attribute("icc_profile_name", "")
        .unwrap();
    assert_eq!(cs.interchange_attribute("icc_profile_name").unwrap(), "");
}

#[test]
fn colorspace_icc_profile_name_serialization() {
    let mut cfg = Config::create();
    let mut cs = ColorSpace::default();
    cs.set_name("test_colorspace");
    cs.set_interchange_attribute("icc_profile_name", "sRGB IEC61966-2.1")
        .unwrap();
    cfg.add_color_space(&cs).unwrap();
    let yaml = cfg.serialize().unwrap();
    assert!(yaml.contains("icc_profile_name"));
    assert!(yaml.contains("sRGB IEC61966-2.1"));
    let de = Config::create_from_str(&yaml).unwrap();
    assert_eq!(
        de.get_color_space("test_colorspace")
            .unwrap()
            .interchange_attribute("icc_profile_name")
            .unwrap(),
        "sRGB IEC61966-2.1"
    );
    let mut copy = cfg.clone();
    copy.set_version(2, 4).unwrap();
    assert_err!(
        copy.serialize(),
        "has non-empty interchange attributes and config version is less than 2.5."
    );
    cs.set_interchange_attribute("icc_profile_name", "")
        .unwrap();
    cfg.add_color_space(&cs).unwrap();
    assert!(!cfg.serialize().unwrap().contains("icc_profile_name"));
}

#[test]
fn colorspace_unknown_interchange_attrib() {
    let mut cs = ColorSpace::default();
    assert_err!(
        cs.interchange_attribute("unknown_attrib"),
        "Unknown attribute name"
    );
    assert_err!(cs.interchange_attribute(""), "Unknown attribute name");
    assert_err!(
        cs.set_interchange_attribute("unknown_attribute1", "unknown"),
        "Unknown attribute name"
    );
    assert_err!(
        cs.set_interchange_attribute("unknown_attribute2", ""),
        "Unknown attribute name"
    );
    assert_eq!(cs.interchange_attributes().len(), 0);
}

// ---------------------------------------------------------------------------
// ColorSpaceSet

fn named(name: &str) -> ColorSpace {
    ColorSpace::with_name(name)
}

#[test]
fn color_space_set_basic() {
    let mut css1 = ColorSpaceSet::new();
    assert_eq!(css1.num_color_spaces(), 0);
    let mut css2 = css1.clone();
    assert_eq!(css2.num_color_spaces(), 0);
    assert_eq!(css1, css2);

    let mut cs1 = named("cs1");
    cs1.add_alias("alias1");
    css1.add_color_space(&cs1).unwrap();
    assert_eq!(css1.num_color_spaces(), 1);
    assert_ne!(css1, css2);
    let mut css3 = css1.clone();
    assert_eq!(css3.num_color_spaces(), 1);
    assert_eq!(css1, css3);

    // Adding an existing color space replaces it.
    css1.add_color_space(&cs1).unwrap();
    assert_eq!(css1.num_color_spaces(), 1);
    assert_eq!(css1, css3);

    // Alias conflicts.
    let mut cs = named("cs2");
    cs.add_alias("alias1");
    assert_err!(
        css1.add_color_space(&cs),
        "Cannot add 'cs2' color space, it has 'alias1' alias and existing color space, 'cs1' is using the same alias."
    );
    let cs = named("alias1");
    assert_err!(
        css1.add_color_space(&cs),
        "Cannot add 'alias1' color space, existing color space, 'cs1' is using this name as an alias."
    );

    css2.add_color_space(&named("cs1")).unwrap();
    assert_eq!(css1, css2);
    css2.add_color_space(&named("cs2")).unwrap();
    assert_eq!(css2.num_color_spaces(), 2);
    assert_ne!(css1, css2);

    assert_eq!(css2.color_space_name_by_index(0), Some("cs1"));
    assert_eq!(css2.color_space_name_by_index(1), Some("cs2"));
    assert_eq!(css2.color_space_name_by_index(2), None);
    assert_eq!(css2.color_space_index("cs2"), Some(1));
    assert_eq!(css2.color_space_index("CS2"), Some(1));
    assert_eq!(css2.color_space_index("cs3"), None);
    assert!(css2.has_color_space("cs1"));
    assert!(!css2.has_color_space("cs3"));

    css3.clear_color_spaces();
    assert_eq!(css3.num_color_spaces(), 0);
    css1.remove_color_space("alias1");
    assert_eq!(css1.num_color_spaces(), 1);
    css1.remove_color_space("CS1");
    assert_eq!(css1.num_color_spaces(), 0);
}

#[test]
fn color_space_set_decoupled_and_order() {
    let mut css = ColorSpaceSet::new();
    let mut cs = named("name");
    css.add_color_space(&cs).unwrap();
    cs.set_name("other");
    assert_eq!(css.color_space_by_index(0).unwrap().name(), "name");

    let mut set = ColorSpaceSet::new();
    for n in ["cs1", "cs2", "cs3", "cs4"] {
        set.add_color_space(&named(n)).unwrap();
    }
    let names: Vec<&str> = set.iter().map(|c| c.name()).collect();
    assert_eq!(names, vec!["cs1", "cs2", "cs3", "cs4"]);
    set.remove_color_space("cs2");
    let names: Vec<&str> = set.iter().map(|c| c.name()).collect();
    assert_eq!(names, vec!["cs1", "cs3", "cs4"]);
}

#[test]
fn color_space_set_operations() {
    let mut css1 = ColorSpaceSet::new();
    css1.add_color_space(&named("cs1")).unwrap();
    css1.add_color_space(&named("cs2")).unwrap();
    css1.add_color_space(&named("cs3")).unwrap();
    let mut css2 = ColorSpaceSet::new();
    css2.add_color_space(&named("cs3")).unwrap();
    css2.add_color_space(&named("cs1")).unwrap();
    css2.add_color_space(&named("cs4")).unwrap();

    let u = css1.union(&css2).unwrap();
    let names: Vec<&str> = u.iter().map(|c| c.name()).collect();
    assert_eq!(names, vec!["cs1", "cs2", "cs3", "cs4"]);

    let i = css1.intersection(&css2).unwrap();
    let names: Vec<&str> = i.iter().map(|c| c.name()).collect();
    assert_eq!(names, vec!["cs3", "cs1"]);

    let d = css1.difference(&css2).unwrap();
    let names: Vec<&str> = d.iter().map(|c| c.name()).collect();
    assert_eq!(names, vec!["cs2"]);

    let d = css2.difference(&css1).unwrap();
    let names: Vec<&str> = d.iter().map(|c| c.name()).collect();
    assert_eq!(names, vec!["cs4"]);
}
