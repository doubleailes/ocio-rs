//! Port of `ViewingRules_tests.cpp`.

mod config_common;

use ocio::config::{ColorSpace, ViewingRules};
use ocio::Config;

#[test]
fn viewing_rules_basic() {
    let mut vrules = ViewingRules::new();
    assert_eq!(vrules.num_entries(), 0);

    assert_err!(vrules.name(0), "Viewing rules: rule index '0' invalid.");
    assert_err!(vrules.insert_rule(1, "test"), "Viewing rules: rule index '1' invalid.");
    assert_err!(vrules.insert_rule(0, ""), "Viewing rules: rule must have a non-empty name.");

    vrules.insert_rule(0, "Rule0").unwrap();
    vrules.insert_rule(1, "Rule2").unwrap();
    vrules.insert_rule(1, "Rule1").unwrap();

    assert_eq!(vrules.num_entries(), 3);
    assert_eq!(vrules.name(0).unwrap(), "Rule0");
    assert_eq!(vrules.name(1).unwrap(), "Rule1");
    assert_eq!(vrules.name(2).unwrap(), "Rule2");

    assert_err!(vrules.name(3), "Viewing rules: rule index '3' invalid.");
    assert_err!(vrules.insert_rule(1, "Rule1"), "A rule named 'Rule1' already exists");

    for r in 0..3 {
        assert_eq!(vrules.num_color_spaces(r).unwrap(), 0);
        assert_eq!(vrules.num_encodings(r).unwrap(), 0);
        assert_eq!(vrules.num_custom_keys(r).unwrap(), 0);
    }

    vrules.add_color_space(0, "colorspace0").unwrap();
    vrules.add_color_space(0, "colorspace1").unwrap();
    assert_eq!(vrules.num_color_spaces(0).unwrap(), 2);
    assert_eq!(vrules.color_space(0, 0).unwrap(), "colorspace0");
    assert_eq!(vrules.color_space(0, 1).unwrap(), "colorspace1");
    assert_err!(vrules.color_space(0, 2), "rule 'Rule0' at index '0': colorspace index '2' is invalid.");
    assert_err!(vrules.remove_color_space(3, 0), "Viewing rules: rule index '3' invalid.");
    assert_err!(
        vrules.remove_color_space(0, 2),
        "rule 'Rule0' at index '0': colorspace index '2' is invalid."
    );
    vrules.remove_color_space(0, 0).unwrap();
    assert_eq!(vrules.num_color_spaces(0).unwrap(), 1);
    assert_eq!(vrules.color_space(0, 0).unwrap(), "colorspace1");
    vrules.add_color_space(0, "colorspace0").unwrap();

    assert_err!(
        vrules.add_encoding(0, "encoding0"),
        "encoding can't be added if there are colorspaces."
    );
    vrules.add_encoding(1, "encoding0").unwrap();
    vrules.add_encoding(1, "encoding1").unwrap();
    assert_err!(
        vrules.add_color_space(1, "colorspace0"),
        "colorspace can't be added if there are encodings."
    );
    assert_eq!(vrules.num_encodings(1).unwrap(), 2);
    assert_eq!(vrules.encoding(1, 0).unwrap(), "encoding0");
    assert_eq!(vrules.encoding(1, 1).unwrap(), "encoding1");
    assert_err!(vrules.encoding(1, 2), "rule 'Rule1' at index '1': encoding index '2' is invalid.");
    assert_err!(vrules.remove_encoding(3, 0), "Viewing rules: rule index '3' invalid.");
    assert_err!(
        vrules.remove_encoding(1, 2),
        "rule 'Rule1' at index '1': encoding index '2' is invalid."
    );
    vrules.remove_encoding(1, 0).unwrap();
    assert_eq!(vrules.num_encodings(1).unwrap(), 1);
    assert_eq!(vrules.encoding(1, 0).unwrap(), "encoding1");
    vrules.add_encoding(1, "encoding0").unwrap();

    vrules.set_custom_key(0, "key0", "value0").unwrap();
    vrules.set_custom_key(0, "key1", "value1").unwrap();
    assert_eq!(vrules.num_custom_keys(0).unwrap(), 2);
    assert_eq!(vrules.custom_key_name(0, 0).unwrap(), "key0");
    assert_eq!(vrules.custom_key_value(0, 0).unwrap(), "value0");
    assert_eq!(vrules.custom_key_name(0, 1).unwrap(), "key1");
    assert_eq!(vrules.custom_key_value(0, 1).unwrap(), "value1");
    assert_err!(vrules.custom_key_name(0, 2), "rule named 'Rule0' error: Key index '2' is invalid");
    assert_err!(vrules.custom_key_value(0, 2), "rule named 'Rule0' error: Key index '2' is invalid");

    vrules.set_custom_key(0, "key0", "newvalue0").unwrap();
    assert_eq!(vrules.num_custom_keys(0).unwrap(), 2);
    assert_eq!(vrules.custom_key_value(0, 0).unwrap(), "newvalue0");

    let expected = "<ViewingRule name=Rule0, colorspaces=[colorspace1, colorspace0], customKeys=[(key0, newvalue0), (key1, value1)]>\n\
<ViewingRule name=Rule1, encodings=[encoding1, encoding0]>\n\
<ViewingRule name=Rule2>";
    assert_eq!(vrules.to_string(), expected);

    let num_rules = vrules.num_entries();
    assert_err!(vrules.remove_rule(num_rules), "rule index '3' invalid. There are only '3' rules");
    assert_eq!(vrules.num_entries(), num_rules);
    assert_eq!(vrules.num_color_spaces(0).unwrap(), 2);
    assert_eq!(vrules.num_encodings(1).unwrap(), 2);

    vrules.remove_rule(1).unwrap();
    assert_eq!(vrules.num_entries(), 2);
    assert_eq!(vrules.name(0).unwrap(), "Rule0");
    assert_eq!(vrules.name(1).unwrap(), "Rule2");

    assert_eq!(vrules.index_for_rule("Rule0").unwrap(), 0);
    assert_eq!(vrules.index_for_rule("Rule2").unwrap(), 1);
    assert_err!(vrules.index_for_rule("I am not there"), "rule name 'I am not there' not found");
}

#[test]
fn viewing_rules_config_io() {
    let mut config = Config::create_raw().create_editable_copy();

    let mut vrules = ViewingRules::new();
    vrules.insert_rule(0, "Rule0").unwrap();
    vrules.insert_rule(1, "Rule1").unwrap();
    vrules.set_custom_key(0, "key0", "value0").unwrap();
    vrules.set_custom_key(0, "key1", "value1").unwrap();
    vrules.add_encoding(1, "encoding0").unwrap();
    vrules.add_encoding(1, "encoding1").unwrap();

    config.set_viewing_rules(&vrules);
    assert_err!(config.validate(), "must have either a color space or an encoding");

    vrules.add_color_space(0, "colorspace0").unwrap();
    let mut cs = ColorSpace::default();
    cs.set_name("colorspace0");
    config.add_color_space(&cs).unwrap();
    cs.set_name("cs_enc0");
    cs.set_encoding("encoding0");
    config.add_color_space(&cs).unwrap();
    cs.set_name("cs_enc1");
    cs.set_encoding("encoding1");
    config.add_color_space(&cs).unwrap();

    config.set_viewing_rules(&vrules);
    config.validate().unwrap();

    let s = config.serialize().unwrap();
    let back = Config::create_from_str(&s).unwrap();
    let vr = back.viewing_rules();
    assert_eq!(vr.num_entries(), 2);
    assert_eq!(vr.name(0).unwrap(), "Rule0");
    assert_eq!(vr.name(1).unwrap(), "Rule1");

    assert_eq!(vr.num_color_spaces(0).unwrap(), 1);
    assert_eq!(vr.num_encodings(0).unwrap(), 0);
    assert_eq!(vr.num_custom_keys(0).unwrap(), 2);
    assert_eq!(vr.color_space(0, 0).unwrap(), "colorspace0");
    assert_eq!(vr.custom_key_name(0, 0).unwrap(), "key0");
    assert_eq!(vr.custom_key_value(0, 0).unwrap(), "value0");
    assert_eq!(vr.custom_key_name(0, 1).unwrap(), "key1");
    assert_eq!(vr.custom_key_value(0, 1).unwrap(), "value1");

    assert_eq!(vr.num_color_spaces(1).unwrap(), 0);
    assert_eq!(vr.num_encodings(1).unwrap(), 2);
    assert_eq!(vr.num_custom_keys(1).unwrap(), 0);
    assert_eq!(vr.encoding(1, 0).unwrap(), "encoding0");
    assert_eq!(vr.encoding(1, 1).unwrap(), "encoding1");
}

const SIMPLE_CONFIG: &str = r#"ocio_profile_version: 2

environment:
  {}
search_path: ""
strictparsing: true
luma: [0.2126, 0.7152, 0.0722]

roles:
  default: raw
  scene_linear: c3

file_rules:
  - !<Rule> {name: ColorSpaceNamePathSearch}
  - !<Rule> {name: Default, colorspace: raw}

viewing_rules:
  - !<Rule> {name: Rule_1, colorspaces: c1}
  - !<Rule> {name: Rule_2, colorspaces: [c2, c3]}
  - !<Rule> {name: Rule_3, colorspaces: scene_linear}
  - !<Rule> {name: Rule_4, colorspaces: [c3, c4]}
  - !<Rule> {name: Rule_5, encodings: log}
  - !<Rule> {name: Rule_6, encodings: [log, video]}

shared_views:
  - !<View> {name: SView_a, colorspace: raw, rule: Rule_2}
  - !<View> {name: SView_b, colorspace: raw, rule: Rule_3}
  - !<View> {name: SView_c, colorspace: raw}
  - !<View> {name: SView_d, colorspace: raw, rule: Rule_5}
  - !<View> {name: SView_e, colorspace: raw}

displays:
  sRGB:
    - !<View> {name: View_a, colorspace: raw, rule: Rule_1}
    - !<View> {name: View_b, colorspace: raw, rule: Rule_2}
    - !<View> {name: View_c, colorspace: raw, rule: Rule_2}
    - !<View> {name: View_d, colorspace: raw, rule: Rule_3}
    - !<View> {name: View_e, colorspace: raw, rule: Rule_4}
    - !<View> {name: View_f, colorspace: raw, rule: Rule_5}
    - !<View> {name: View_g, colorspace: raw, rule: Rule_6}
    - !<View> {name: View_h, colorspace: raw}
    - !<Views> [SView_a, SView_b, SView_d, SView_e]

active_displays: []
active_views: []

colorspaces:
  - !<ColorSpace>
    name: raw
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    allocation: uniform

  - !<ColorSpace>
    name: c1
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    encoding: video
    allocation: uniform

  - !<ColorSpace>
    name: c2
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    allocation: uniform

  - !<ColorSpace>
    name: c3
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    allocation: uniform

  - !<ColorSpace>
    name: c4
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    encoding: log
    allocation: uniform

  - !<ColorSpace>
    name: c5
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    encoding: data
    allocation: uniform

  - !<ColorSpace>
    name: c6
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    encoding: video
    allocation: uniform
"#;

#[test]
fn viewing_rules_filtered_views() {
    let config = Config::create_from_str(SIMPLE_CONFIG).unwrap();
    config.validate().unwrap();

    assert_eq!(config.display_view_rule("no", "unknown"), "");
    assert_eq!(config.display_view_rule("sRGB", "unknown"), "");
    assert_eq!(config.display_view_rule("sRGB", "View_b"), "Rule_2");

    assert_eq!(config.num_views_for_color_space("no", "unknown").unwrap(), 0);
    assert_eq!(config.view_for_color_space("no", "unknown", 0).unwrap(), "");

    assert_err!(
        config.num_views_for_color_space("sRGB", "unknown"),
        "Could not find source color space 'unknown'."
    );
    assert_err!(
        config.view_for_color_space("sRGB", "unknown", 0),
        "Could not find source color space 'unknown'."
    );

    let views = |cs: &str, cfg: &Config| -> Vec<String> {
        let n = cfg.num_views_for_color_space("sRGB", cs).unwrap();
        (0..n).map(|i| cfg.view_for_color_space("sRGB", cs, i).unwrap()).collect()
    };

    assert_eq!(views("c6", &config), ["View_g", "View_h", "SView_e"]);
    assert_eq!(config.view_for_color_space("sRGB", "c6", 3).unwrap(), "");

    assert_eq!(
        views("c3", &config),
        ["View_b", "View_c", "View_d", "View_e", "View_h", "SView_a", "SView_b", "SView_e"]
    );
    assert_eq!(
        views("c4", &config),
        ["View_e", "View_f", "View_g", "View_h", "SView_d", "SView_e"]
    );

    assert_eq!(config.serialize().unwrap(), SIMPLE_CONFIG);

    let mut configav = config.create_editable_copy();
    configav
        .set_active_views("SView_e, View_h, SView_d, View_d, SView_a, View_b")
        .unwrap();
    configav.validate().unwrap();

    assert_eq!(views("c3", &configav), ["SView_e", "View_h", "View_d", "SView_a", "View_b"]);

    assert_eq!(configav.default_display(), "sRGB");
    assert_eq!(
        configav.view_for_color_space("sRGB", "c3", 0).unwrap(),
        configav.default_view_for_color_space("sRGB", "c3").unwrap()
    );
}
