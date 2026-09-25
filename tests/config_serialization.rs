//! Port of `Config_tests.cpp` (part 3: serialization of the transforms).

mod config_common;

use config_common::profiles::*;
use config_common::*;
use ocio::*;

#[test]
fn config_serialize_colorspace_displayview_transforms() {
    let end = r#"    from_scene_reference: !<GroupTransform>
      children:
        - !<ColorSpaceTransform> {src: raw, dst: log}
        - !<ColorSpaceTransform> {src: raw, dst: log, direction: inverse}
        - !<ColorSpaceTransform> {src: default, dst: log, data_bypass: false}
        - !<DisplayViewTransform> {src: raw, display: sRGB, view: RawView}
        - !<DisplayViewTransform> {src: default, display: sRGB, view: RawView, direction: inverse}
        - !<DisplayViewTransform> {src: log, display: sRGB, view: RawView, looks_bypass: true, data_bypass: false}
"#;
    check_roundtrip(&format!("{}{end}", profile_v2_start()));
}

fn v2(end: &str) -> String {
    format!("{}{end}", profile_v2_start())
}

fn v1(end: &str) -> String {
    format!("{}{end}", simple_profile_v1())
}

/// The part of a v2 profile before the file rules (i.e. without them).
fn v2_no_rules(end: &str) -> String {
    format!(
        "{PROFILE_V2}{SIMPLE_PROFILE_A}{}{end}",
        simple_profile_b_v2()
    )
}

#[test]
fn config_range_serialization() {
    for end in [
        "    from_scene_reference: !<RangeTransform> {min_in_value: 0, min_out_value: 0}\n",
        "    from_scene_reference: !<RangeTransform> {min_in_value: 0, min_out_value: 0, direction: inverse}\n",
        "    from_scene_reference: !<RangeTransform> {min_in_value: 0, max_in_value: 1, min_out_value: 0, max_out_value: 1, style: noClamp, direction: inverse}\n",
        "    from_scene_reference: !<RangeTransform> {min_in_value: -0.0109, max_in_value: 1.0505, min_out_value: 0.0009, max_out_value: 2.5001, direction: inverse}\n",
        "    from_scene_reference: !<RangeTransform> {min_in_value: -0.01, max_in_value: 1.05, min_out_value: 0.0009, max_out_value: 2.5}\n",
    ] {
        check_roundtrip(&v2(end));
    }

    // Clamp style is not saved.
    check_roundtrip_to(
        &v2("    from_scene_reference: !<RangeTransform> {min_in_value: -0.0109, max_in_value: 1.0505, min_out_value: 0.0009, max_out_value: 2.5001, style: Clamp, direction: inverse}\n"),
        &v2("    from_scene_reference: !<RangeTransform> {min_in_value: -0.0109, max_in_value: 1.0505, min_out_value: 0.0009, max_out_value: 2.5001, direction: inverse}\n"),
    );

    // Invalid ranges can still be read and written.
    for end in [
        "    from_scene_reference: !<RangeTransform> {min_in_value: 0, max_out_value: 1}\n",
        "    from_scene_reference: !<RangeTransform> {min_in_value: 0.12345678901234, max_out_value: 1.23456789012345}\n",
        "    from_scene_reference: !<RangeTransform> {min_out_value: 0.0009, max_out_value: 2.5}\n",
        "    from_scene_reference: !<GroupTransform>\n      children:\n        - !<RangeTransform> {min_in_value: -0.01, max_in_value: 1.05, min_out_value: 0.0009, max_out_value: 2.5}\n        - !<RangeTransform> {min_out_value: 0.0009, max_out_value: 2.1}\n        - !<RangeTransform> {min_out_value: 0.1, max_out_value: 0.9}\n",
    ] {
        check_roundtrip_no_validation(&v2(end));
    }

    // max_in_value has an illegal second number.
    assert_err!(
        Config::create_from_str(&v2_no_rules(
            "    from_scene_reference: !<RangeTransform> {min_in_value: -0.01, max_in_value: 1.05  10, min_out_value: 0.0009, max_out_value: 2.5}\n"
        )),
        "parsing double failed"
    );

    // max_in_value & max_out_value have no value, they will not be defined.
    check_roundtrip_to(
        &v2_no_rules("    from_scene_reference: !<RangeTransform> {min_in_value: -0.01, max_in_value: , min_out_value: -0.01, max_out_value: }\n"),
        &v2("    from_scene_reference: !<RangeTransform> {min_in_value: -0.01, min_out_value: -0.01}\n"),
    );

    // Some faulty cases.
    for end in [
        "    from_scene_reference: !<GroupTransform>\n      children:\n        - !<RangeTransform> mInValue: -0.01, max_in_value: 1.05, min_out_value: 0.0009, max_out_value: 2.5}\n",
        "    from_scene_reference: !<RangeTransform> {min_in_value: -0.01 max_in_value: 1.05, min_out_value: 0.0009, max_out_value: 2.5}\n",
        "    from_scene_reference: !<RangeTransform> {min_in_value: -0.01, max_in_value: 1.05, min_out_value: 0.0009maxOutValue: 2.5}\n",
    ] {
        assert_err!(Config::create_from_str(&v2(end)), "Loading the OCIO profile failed");
    }
}

#[test]
fn config_range_serialization_validation() {
    let config = Config::create_from_str(&v2(
        "    from_scene_reference: !<RangeTransform> {min_in_value: 0, min_out_value: 0, style: noClamp}\n",
    ))
    .unwrap();
    assert_err!(
        config.validate(),
        "non clamping range must have min and max values defined"
    );

    for end in [
        "    from_scene_reference: !<RangeTransform> {min_in_value: 0, max_out_value: 1}\n",
        "    from_scene_reference: !<RangeTransform> {min_in_value: 0.12345678901234, max_out_value: 1.23456789012345}\n",
        "    from_scene_reference: !<RangeTransform> {min_out_value: 0.0009, max_out_value: 2.5}\n",
        "    from_scene_reference: !<GroupTransform>\n      children:\n        - !<RangeTransform> {min_in_value: -0.01, max_in_value: 1.05, min_out_value: 0.0009, max_out_value: 2.5}\n        - !<RangeTransform> {min_out_value: 0.0009, max_out_value: 2.1}\n        - !<RangeTransform> {min_out_value: 0.1, max_out_value: 0.9}\n",
    ] {
        let config = Config::create_from_str(&v2(end)).unwrap();
        assert_err!(config.validate(), "must be both set or both missing");
    }
}

#[test]
fn config_exponent_serialization() {
    check_roundtrip(&v1(
        "    from_reference: !<ExponentTransform> {value: [1.101, 1.202, 1.303, 1.404]}\n",
    ));
    // If R==G==B and A==1, and the version is > 1, the compact syntax is used.
    check_roundtrip(&v2(
        "    from_scene_reference: !<ExponentTransform> {value: 1.101}\n",
    ));
    // If version==1, then write all values for compatibility with the v1 library.
    check_roundtrip(&v1(
        "    from_reference: !<ExponentTransform> {value: [1.101, 1.101, 1.101, 1]}\n",
    ));
    check_roundtrip(&v1(
        "    from_reference: !<ExponentTransform> {value: [1.101, 1.202, 1.303, 1.404], direction: inverse}\n",
    ));
    check_roundtrip(&v2(
        "    from_scene_reference: !<ExponentTransform> {value: [1.101, 1.202, 1.303, 1.404], style: mirror, direction: inverse}\n",
    ));
    check_roundtrip(&v2(
        "    from_scene_reference: !<ExponentTransform> {value: [1.101, 1.202, 1.303, 1.404], style: pass_thru, direction: inverse}\n",
    ));

    // Errors.
    assert_err!(
        Config::create_from_str(&v1(
            "    from_reference: !<ExponentTransform> {value: [1.1, 1.2, 1.3]}\n"
        )),
        "'value' values must be 4 floats. Found '3'"
    );
    assert_err!(
        Config::create_from_str(&v1(
            "    from_reference: !<ExponentTransform> {value: [1.101, 1.202, 1.303, 1.404], style: wrong,}\n"
        )),
        "Unknown exponent style"
    );
}

#[test]
fn config_exponent_with_linear_serialization() {
    for end in [
        "    from_scene_reference: !<ExponentWithLinearTransform> {gamma: [1.1, 1.2, 1.3, 1.4], offset: [0.101, 0.102, 0.103, 0.1]}\n",
        "    from_scene_reference: !<ExponentWithLinearTransform> {gamma: [1.1, 1.2, 1.3, 1.4], offset: [0.101, 0.102, 0.103, 0.1], style: mirror}\n",
        "    from_scene_reference: !<ExponentWithLinearTransform> {gamma: [1.1, 1.2, 1.3, 1.4], offset: [0.101, 0.102, 0.103, 0.1], direction: inverse}\n",
        "    from_scene_reference: !<ExponentWithLinearTransform> {gamma: [1.1, 1.2, 1.3, 1.4], offset: [0.101, 0.102, 0.103, 0.1], style: mirror, direction: inverse}\n",
        "    from_scene_reference: !<ExponentWithLinearTransform> {gamma: 1.1, offset: 0.101, direction: inverse}\n",
    ] {
        check_roundtrip(&v2(end));
    }

    let cases = [
        (
            "    from_scene_reference: !<ExponentWithLinearTransform> {}\n",
            "ExponentWithLinear parse error, gamma and offset fields are missing",
        ),
        (
            "    from_scene_reference: !<ExponentWithLinearTransform> {gamma: [1.1, 1.2, 1.3, 1.4]}\n",
            "ExponentWithLinear parse error, offset field is missing",
        ),
        (
            "    from_scene_reference: !<ExponentWithLinearTransform> {offset: [1.1, 1.2, 1.3, 1.4]}\n",
            "ExponentWithLinear parse error, gamma field is missing",
        ),
        (
            "    from_scene_reference: !<ExponentWithLinearTransform> {gamma: [1.1, 1.2, 1.3]}\n",
            "ExponentWithLinear parse error, gamma field must be 4 floats",
        ),
        (
            "    from_scene_reference: !<ExponentWithLinearTransform> {gamma: [1.1, 1.2, 1.3, 1.4], offset: [0.101, 0.102]}\n",
            "ExponentWithLinear parse error, offset field must be 4 floats",
        ),
        (
            "    from_scene_reference: !<ExponentWithLinearTransform> {gamma: [1.1, 1.2, 1.3, 1.4], offset: [0.101, 0.102, 0.103, 0.1], direction: inverse, style: pass_thru}\n",
            "Pass thru negative extrapolation is not valid for MonCurve",
        ),
    ];
    for (end, what) in cases {
        assert_err!(Config::create_from_str(&v2(end)), what);
    }
}

#[test]
fn config_exponent_vs_config_version() {
    let apply = |s: &str| {
        let config = Config::create_from_str(s).unwrap();
        config.validate().unwrap();
        let cpu = config
            .get_processor("raw", "lnh")
            .unwrap()
            .default_cpu_processor();
        let mut img = [-0.5f32, 0.0, 1.0, 1.0];
        cpu.apply_rgba(&mut img);
        img
    };
    let end1 = "    from_reference: !<ExponentTransform> {value: [1, 1, 1, 1]}\n";
    let end2 = "    from_reference: !<ExponentTransform> {value: [2, 2, 2, 1]}\n";

    assert_eq!(apply(&v1(end1)), [-0.5, 0.0, 1.0, 1.0]);
    assert_eq!(apply(&v1(end2)), [0.0, 0.0, 1.0, 1.0]);

    let img = apply(&v2(end1));
    assert_eq!(img[0], 0.0);
    assert_eq!(img[1], 0.0);
    assert!((img[2] - 1.0).abs() < 2e-5);
    assert!((img[3] - 1.0).abs() < 2e-5);

    let img = apply(&v2(end2));
    assert_eq!(img[0], 0.0);
    assert_eq!(img[1], 0.0);
    assert!((img[2] - 1.0).abs() < 3e-5);
    assert!((img[3] - 1.0).abs() < 2e-5);
}

#[test]
fn config_categories() {
    const CONFIG: &str = r#"ocio_profile_version: 2

environment:
  {}
search_path: luts
strictparsing: true
luma: [0.2126, 0.7152, 0.0722]

roles:
  default: raw1
  scene_linear: raw1

file_rules:
  - !<Rule> {name: Default, colorspace: default}

displays:
  sRGB:
    - !<View> {name: Raw, colorspace: raw1}

active_displays: []
active_views: []

colorspaces:
  - !<ColorSpace>
    name: raw1
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    categories: [rendering, linear]
    encoding: scene-linear
    allocation: uniform
    allocationvars: [-0.125, 1.125]

  - !<ColorSpace>
    name: raw2
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    categories: [rendering]
    encoding: data
    allocation: uniform
    allocationvars: [-0.125, 1.125]
"#;
    let config = check_roundtrip(CONFIG);

    let css = config.color_spaces("");
    assert_eq!(css.num_color_spaces(), 2);
    let cs = css.color_space_by_index(0).unwrap();
    assert_eq!(cs.num_categories(), 2);
    assert_eq!(cs.category(0), Some("rendering"));
    assert_eq!(cs.category(1), Some("linear"));

    let css = config.color_spaces("linear");
    assert_eq!(css.num_color_spaces(), 1);
    let cs = css.color_space_by_index(0).unwrap();
    assert_eq!(cs.num_categories(), 2);
    assert_eq!(cs.category(0), Some("rendering"));
    assert_eq!(cs.category(1), Some("linear"));

    assert_eq!(config.color_spaces("rendering").num_color_spaces(), 2);

    assert_eq!(config.num_color_spaces(), 2);
    assert_eq!(config.color_space_name_by_index(0), "raw1");
    assert_eq!(config.color_space_name_by_index(1), "raw2");
    assert_eq!(config.index_for_color_space("raw1"), Some(0));
    assert_eq!(config.index_for_color_space("raw2"), Some(1));
    let cs = config.get_color_space("raw1").unwrap();
    assert_eq!(cs.name(), "raw1");
    assert_eq!(cs.encoding(), "scene-linear");
    let cs = config.get_color_space("raw2").unwrap();
    assert_eq!(cs.name(), "raw2");
    assert_eq!(cs.encoding(), "data");
}
