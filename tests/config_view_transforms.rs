//! Port of `Config_tests.cpp` (part 8: processors between two configs,
//! display color spaces, view transforms and display/views).

mod config_common;

use config_common::profiles::*;
use config_common::*;
use ocio::config::{ColorSpace, ViewTransform};
use ocio::*;

const TWO_CONFIGS_1: &str = r#"
ocio_profile_version: 2

environment:
  {}

roles:
  default: raw1
  aces_interchange: aces1
  cie_xyz_d65_interchange: display1

displays:
  displayname:
    - !<View> {name: view1, colorspace: displaytest1}
    - !<View> {name: view2, view_transform: vt1, display_colorspace: display2}
    - !<View> {name: view3, colorspace: data_space}

view_transforms:
  - !<ViewTransform>
    name: vt1
    from_scene_reference: !<RangeTransform> {min_in_value: 0., min_out_value: 0.}

colorspaces:
  - !<ColorSpace>
    name: raw1
    allocation: uniform

  - !<ColorSpace>
    name: test1
    allocation: uniform
    to_scene_reference: !<MatrixTransform> {offset: [0.01, 0.02, 0.03, 0]}

  - !<ColorSpace>
    name: displaytest1
    allocation: uniform
    to_scene_reference: !<LogTransform> {base: 2}

  - !<ColorSpace>
    name: aces1
    allocation: uniform
    from_scene_reference: !<ExponentTransform> {value: [1.101, 1.202, 1.303, 1.404]}

  - !<ColorSpace>
    name: data_space
    isdata: true

display_colorspaces:
  - !<ColorSpace>
    name: display1
    allocation: uniform
    from_display_reference: !<CDLTransform> {slope: [1, 2, 1]}

  - !<ColorSpace>
    name: display2
    allocation: uniform
    from_display_reference: !<FixedFunctionTransform> {style: ACES_RedMod03}

"#;

const TWO_CONFIGS_2: &str = r#"
ocio_profile_version: 2

environment:
  {}

roles:
  default: raw2
  aces_interchange: aces2
  cie_xyz_d65_interchange: display3
  test_role: test2

colorspaces:
  - !<ColorSpace>
    name: raw2
    allocation: uniform

  - !<ColorSpace>
    name: test2
    allocation: uniform
    from_scene_reference: !<MatrixTransform> {offset: [0.11, 0.12, 0.13, 0]}

  - !<ColorSpace>
    name: aces2
    allocation: uniform
    to_scene_reference: !<RangeTransform> {min_in_value: -0.0109, max_in_value: 1.0505, min_out_value: 0.0009, max_out_value: 2.5001}

display_colorspaces:
  - !<ColorSpace>
    name: display3
    allocation: uniform
    from_display_reference: !<ExponentTransform> {value: 2.4}

  - !<ColorSpace>
    name: display4
    allocation: uniform
    from_display_reference: !<LogTransform> {base: 5}
"#;

const TWO_CONFIGS_3: &str = r#"
ocio_profile_version: 2

environment:
  {}

roles:
  default: raw

colorspaces:
  - !<ColorSpace>
    name: raw
    allocation: uniform

  - !<ColorSpace>
    name: test
    allocation: uniform
    from_scene_reference: !<MatrixTransform> {offset: [0.11, 0.12, 0.13, 0]}

display_colorspaces:
  - !<ColorSpace>
    name: display5
    allocation: uniform
    from_display_reference: !<ExponentTransform> {value: 2.4}
"#;

fn kinds(p: &Processor) -> Vec<&'static str> {
    p.create_group_transform()
        .transforms
        .iter()
        .map(|t| match t {
            Transform::Matrix(_) => "matrix",
            Transform::Exponent(_) => "exponent",
            Transform::Range(_) => "range",
            Transform::FixedFunction(_) => "ff",
            Transform::Cdl(_) => "cdl",
            Transform::Log(_) => "log",
            _ => "other",
        })
        .collect()
}

#[test]
#[ignore = "needs-merge"]
fn config_get_processor_from_two_configs() {
    use TransformDirection::{Forward, Inverse};
    let config1 = Config::create_from_str(TWO_CONFIGS_1).unwrap();
    let config2 = Config::create_from_str(TWO_CONFIGS_2).unwrap();

    let p = Config::get_processor_from_configs(&config1, "test1", &config2, "test2").unwrap();
    assert_eq!(kinds(&p), ["matrix", "exponent", "range", "matrix"]);

    let p = Config::get_processor_from_configs_interchange(
        &config1, "test1", "aces1", &config2, "test2", "aces2",
    )
    .unwrap();
    assert_eq!(kinds(&p).len(), 4);
    let p = Config::get_processor_from_configs_interchange(
        &config1,
        "test1",
        ROLE_INTERCHANGE_SCENE,
        &config2,
        "test2",
        "aces2",
    )
    .unwrap();
    assert_eq!(kinds(&p).len(), 4);
    let p = Config::get_processor_from_configs_interchange(
        &config1,
        "test1",
        ROLE_INTERCHANGE_SCENE,
        &config2,
        "test_role",
        "aces2",
    )
    .unwrap();
    assert_eq!(kinds(&p).len(), 4);

    let p = Config::get_processor_from_configs(&config1, "display2", &config2, "display4").unwrap();
    assert_eq!(kinds(&p), ["ff", "cdl", "exponent", "log"]);

    let p = Config::get_processor_from_configs(&config1, "data_space", &config2, "test2").unwrap();
    assert_eq!(kinds(&p).len(), 0);

    let p = Config::get_processor_from_configs(&config1, "display2", &config2, "test2").unwrap();
    assert_eq!(kinds(&p), ["ff", "range", "exponent", "range", "matrix"]);

    assert_err!(
        Config::get_processor_from_configs(&config1, "test1", &config2, "display3"),
        "There is no view transform between the main scene-referred space and the display-referred space"
    );

    let p = Config::get_processor_from_configs_display_view_interchange(
        &config2,
        "test2",
        "aces2",
        &config1,
        "displayname",
        "view1",
        "aces1",
        Forward,
    )
    .unwrap();
    assert_eq!(kinds(&p), ["matrix", "range", "exponent", "log"]);

    let p = Config::get_processor_from_configs_display_view_interchange(
        &config2,
        "test2",
        "aces2",
        &config1,
        "displayname",
        "view1",
        "aces1",
        Inverse,
    )
    .unwrap();
    assert_eq!(kinds(&p), ["log", "exponent", "range", "matrix"]);

    let p = Config::get_processor_from_configs_display_view(
        &config2,
        "test2",
        &config1,
        "displayname",
        "view2",
        Forward,
    )
    .unwrap();
    assert_eq!(kinds(&p), ["matrix", "range", "exponent", "range", "ff"]);

    let p = Config::get_processor_from_configs_display_view(
        &config2,
        "test2",
        &config1,
        "displayname",
        "view3",
        Forward,
    )
    .unwrap();
    assert_eq!(kinds(&p).len(), 0);
}

#[test]
fn config_get_processor_from_two_configs_errors() {
    let config1 = Config::create_from_str(TWO_CONFIGS_1).unwrap();
    let config3 = Config::create_from_str(TWO_CONFIGS_3).unwrap();
    assert_err!(
        Config::get_processor_from_configs(&config1, "test1", &config3, "test"),
        "The required role 'aces_interchange' is missing from the source and/or destination config."
    );
    assert_err!(
        Config::get_processor_from_configs(&config1, "display1", &config3, "test"),
        "The required role 'aces_interchange' is missing from the source and/or destination config."
    );
    assert_err!(
        Config::get_processor_from_configs(&config1, "display1", &config3, "display5"),
        "The required role 'cie_xyz_d65_interchange' is missing from the source and/or destination config."
    );
}

fn profile_v2_dcs_start() -> String {
    format!("{PROFILE_V2}{SIMPLE_PROFILE_A}{DEFAULT_RULES}{SIMPLE_PROFILE_DISPLAYS_LOOKS}")
}

#[test]
fn config_display_color_spaces_serialization() {
    let dcs = r#"
view_transforms:
  - !<ViewTransform>
    name: display
    from_display_reference: !<MatrixTransform> {}

  - !<ViewTransform>
    name: scene
    from_scene_reference: !<MatrixTransform> {}

display_colorspaces:
  - !<ColorSpace>
    name: dcs1
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    allocation: uniform
    from_display_reference: !<ExponentTransform> {value: 2.4, direction: inverse}

  - !<ColorSpace>
    name: dcs2
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    allocation: uniform
    to_display_reference: !<ExponentTransform> {value: 2.4}
"#;
    check_roundtrip(&format!(
        "{}{dcs}{SIMPLE_PROFILE_CS_V2}",
        profile_v2_dcs_start()
    ));
}

#[test]
fn config_display_color_spaces_errors() {
    let make = |first: &str, second: &str| {
        format!(
            "{}\ndisplay_colorspaces:\n  - !<ColorSpace>\n    name: dcs1\n    family: \"\"\n    equalitygroup: \"\"\n    bitdepth: unknown\n    isdata: false\n    allocation: uniform\n    {first}: !<ExponentTransform> {{value: [2.4, 2.4, 2.4, 1], direction: inverse}}\n\n  - !<ColorSpace>\n    name: dcs2\n    family: \"\"\n    equalitygroup: \"\"\n    bitdepth: unknown\n    isdata: false\n    allocation: uniform\n    {second}: !<ExponentTransform> {{value: [2.4, 2.4, 2.4, 1]}}\n{SIMPLE_PROFILE_CS_V2}",
            profile_v2_dcs_start()
        )
    };
    assert_err!(
        Config::create_from_str(&make("from_scene_reference", "to_display_reference")),
        "'from_scene_reference' cannot be used for a display color space"
    );
    assert_err!(
        Config::create_from_str(&make("from_display_reference", "to_scene_reference")),
        "'to_scene_reference' cannot be used for a display color space"
    );
}

#[test]
fn config_config_v1() {
    const CONFIG: &str = "ocio_profile_version: 1\nstrictparsing: false\nroles:\n  default: raw\ndisplays:\n  sRGB:\n  - !<View> {name: Raw, colorspace: raw}\ncolorspaces:\n  - !<ColorSpace>\n      name: raw\n";
    let config = Config::create_from_str(CONFIG).unwrap();
    config.validate().unwrap();
    assert_eq!(config.num_view_transforms(), 0);
    assert_eq!(
        config.num_color_spaces_filtered(
            SearchReferenceSpaceType::Display,
            ColorSpaceVisibility::All
        ),
        0
    );
}

#[test]
fn config_view_transforms() {
    let s = format!("{}{SIMPLE_PROFILE_CS_V2}", profile_v2_dcs_start());
    let config = Config::create_from_str(&s).unwrap();
    config.validate().unwrap();

    let mut edit = config.create_editable_copy();
    let mut vt = ViewTransform::new(ReferenceSpaceType::Display);
    assert_err!(
        edit.add_view_transform(&vt),
        "Cannot add view transform with an empty name"
    );
    vt.set_name("display");
    assert_err!(
        edit.add_view_transform(&vt),
        "Cannot add view transform 'display' with no transform"
    );
    vt.set_transform(
        Some(MatrixTransform::default().into()),
        ViewTransformDirection::FromReference,
    );
    edit.add_view_transform(&vt).unwrap();
    assert_eq!(edit.num_view_transforms(), 1);
    assert_err!(
        edit.validate(),
        "at least one must use the scene reference space"
    );
    assert!(edit.default_scene_to_display_view_transform().is_none());

    let mut vt = ViewTransform::new(ReferenceSpaceType::Scene);
    vt.set_name("scene");
    vt.set_transform(
        Some(MatrixTransform::default().into()),
        ViewTransformDirection::FromReference,
    );
    edit.add_view_transform(&vt).unwrap();
    assert_eq!(edit.num_view_transforms(), 2);
    edit.validate().unwrap();

    let scene_vt = edit
        .default_scene_to_display_view_transform()
        .unwrap()
        .clone();
    assert_eq!(edit.view_transform_name_by_index(0), "display");
    assert_eq!(edit.view_transform_name_by_index(1), "scene");
    assert_eq!(edit.view_transform_name_by_index(42), "");
    assert!(edit.view_transform("scene").is_some());
    assert!(edit.view_transform("not a view transform").is_none());

    assert_eq!(edit.default_view_transform_name(), "");
    edit.set_default_view_transform_name("not valid");
    assert_eq!(edit.default_view_transform_name(), "not valid");
    assert_err!(
        edit.validate(),
        "Default view transform is defined as: 'not valid' but this does not correspond to an existing scene-referred view transform"
    );
    edit.set_default_view_transform_name("display");
    assert_err!(
        edit.validate(),
        "Default view transform is defined as: 'display' but this does not correspond to an existing scene-referred view transform"
    );

    let mut new_scene_vt = scene_vt.clone();
    new_scene_vt.set_name("NotFirst");
    edit.add_view_transform(&new_scene_vt).unwrap();
    edit.set_default_view_transform_name("NotFirst");
    edit.validate().unwrap();

    let reloaded = Config::create_from_str(&edit.serialize().unwrap()).unwrap();
    reloaded.validate().unwrap();

    // Setting a view transform with the same name replaces the earlier one.
    vt.set_transform(
        Some(LogTransform::default().into()),
        ViewTransformDirection::FromReference,
    );
    edit.add_view_transform(&vt).unwrap();
    assert_eq!(edit.num_view_transforms(), 3);
    let t = edit
        .view_transform("scene")
        .unwrap()
        .transform(ViewTransformDirection::FromReference);
    assert!(matches!(t, Some(Transform::Log(_))));

    assert_eq!(reloaded.num_view_transforms(), 3);
    assert_eq!(reloaded.default_view_transform_name(), "NotFirst");

    edit.clear_view_transforms();
    assert_eq!(edit.num_view_transforms(), 0);
    assert_eq!(edit.default_view_transform_name(), "NotFirst");
}

#[test]
fn config_display_view() {
    let mut config = Config::create();
    let mut cs = ColorSpace::default();
    cs.set_name("default");
    cs.set_is_data(true);
    config.add_color_space(&cs).unwrap();
    config.set_version(2, 1).unwrap();

    let mut cs = ColorSpace::new(ReferenceSpaceType::Scene);
    cs.set_name("scs");
    config.add_color_space(&cs).unwrap();
    let mut cs = ColorSpace::new(ReferenceSpaceType::Display);
    cs.set_name("dcs");
    config.add_color_space(&cs).unwrap();

    let mut vt = ViewTransform::new(ReferenceSpaceType::Display);
    vt.set_name("display");
    vt.set_transform(
        Some(MatrixTransform::default().into()),
        ViewTransformDirection::FromReference,
    );
    config.add_view_transform(&vt).unwrap();
    let mut vt = ViewTransform::new(ReferenceSpaceType::Scene);
    vt.set_name("view_transform");
    vt.set_transform(
        Some(MatrixTransform::default().into()),
        ViewTransformDirection::FromReference,
    );
    config.add_view_transform(&vt).unwrap();
    config.set_default_view_transform_name("view_transform");

    assert!(!config.has_view("display", "view1"));
    config
        .add_display_view("display", "view1", "scs", "")
        .unwrap();
    assert!(config.has_view("display", "view1"));
    config.validate().unwrap();

    assert!(!config.has_view("display", "view2"));
    config
        .add_display_view_full("display", "view2", "view_transform", "scs", "", "", "")
        .unwrap();
    assert_err!(
        config.validate(),
        "color space, 'scs', that is not a display-referred"
    );
    assert!(config.has_view("display", "view2"));
    config
        .add_display_view_full("display", "view2", "view_transform", "dcs", "", "", "")
        .unwrap();
    assert!(config.has_view("display", "view2"));
    config.validate().unwrap();

    let expected = r#"ocio_profile_version: 2.1

environment:
  {}
search_path: ""
strictparsing: true
luma: [0.2126, 0.7152, 0.0722]

roles:
  {}

file_rules:
  - !<Rule> {name: Default, colorspace: default}

displays:
  display:
    - !<View> {name: view1, colorspace: scs}
    - !<View> {name: view2, view_transform: view_transform, display_colorspace: dcs}

active_displays: []
active_views: []

default_view_transform: view_transform

view_transforms:
  - !<ViewTransform>
    name: display
    from_display_reference: !<MatrixTransform> {}

  - !<ViewTransform>
    name: view_transform
    from_scene_reference: !<MatrixTransform> {}

display_colorspaces:
  - !<ColorSpace>
    name: dcs
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    allocation: uniform

colorspaces:
  - !<ColorSpace>
    name: default
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    isdata: true
    allocation: uniform

  - !<ColorSpace>
    name: scs
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    allocation: uniform
"#;
    let s = config.serialize().unwrap();
    assert_eq!(s, expected);

    let read = Config::create_from_str(&s).unwrap();
    assert_eq!(read.num_views("display"), 2);
    let v1 = read.view("display", 0);
    assert_eq!(v1, "view1");
    assert_eq!(read.display_view_color_space_name("display", &v1), "scs");
    assert_eq!(read.display_view_transform_name("display", &v1), "");
    let v2 = read.view("display", 1);
    assert_eq!(v2, "view2");
    assert_eq!(read.display_view_color_space_name("display", &v2), "dcs");
    assert_eq!(
        read.display_view_transform_name("display", &v2),
        "view_transform"
    );
    assert_eq!(read.default_view_transform_name(), "view_transform");

    assert_err!(
        config.add_display_view("", "view1", "scs", ""),
        "a non-empty display name is needed"
    );
    assert_err!(
        config.add_display_view("display", "", "scs", ""),
        "a non-empty view name is needed"
    );
    assert_err!(
        config.add_display_view("display", "view3", "", ""),
        "a non-empty color space name is needed"
    );
    assert_err!(
        config.add_display_view_full("display", "view4", "view_transform", "", "", "", ""),
        "a non-empty color space name is needed"
    );
}

#[test]
fn config_not_case_sensitive() {
    let config = Config::create_from_str(&profile_v2_start()).unwrap();
    config.validate().unwrap();
    assert!(config.get_color_space("lnh").is_some());
    assert!(config.get_color_space("LNH").is_some());
    assert!(config.get_color_space("RaW").is_some());
    assert!(config.has_role("default"));
    assert!(config.has_role("Default"));
    assert!(config.has_role("DEFAULT"));
    assert!(config.has_role("scene_linear"));
    assert!(config.has_role("Scene_Linear"));
    assert!(!config.has_role("reference"));
    assert!(!config.has_role("REFERENCE"));
}

const TRANSFORM_WITH_ROLES: &str = r#"
ocio_profile_version: 1

roles:
  DEFAULT: raw
  scene_linear: cs1

displays:
  Disp1:
  - !<View> {name: View1, colorspace: RaW, looks: beauty}

looks:
  - !<Look>
    name: beauty
    process_space: SCENE_LINEAR
    transform: !<ColorSpaceTransform> {src: SCENE_LINEAR, dst: raw}

colorspaces:
  - !<ColorSpace>
    name: RAW
    allocation: uniform

  - !<ColorSpace>
    name: CS1
    allocation: uniform
    from_reference: !<MatrixTransform> {offset: [0.11, 0.12, 0.13, 0]}

  - !<ColorSpace>
    name: cs2
    allocation: uniform
    to_reference: !<ColorSpaceTransform> {src: SCENE_LINEAR, dst: raw}

  - !<ColorSpace>
    name: cs3
    allocation: uniform
    to_reference: !<ColorSpaceTransform> {src: SCENE_LINEAR, dst: raw, data_bypass: false}
"#;

#[test]
fn config_transform_with_roles() {
    let config = Config::create_from_str(TRANSFORM_WITH_ROLES).unwrap();
    config.validate().unwrap();
    let bypass = |name: &str| match config
        .get_color_space(name)
        .unwrap()
        .transform(ColorSpaceDirection::ToReference)
    {
        Some(Transform::ColorSpace(t)) => t.data_bypass,
        t => panic!("unexpected {t:?}"),
    };
    assert!(bypass("cs2"));
    assert!(!bypass("cs3"));
}

#[test]
#[ignore = "needs-merge"]
fn config_transform_with_roles_processors() {
    let config = Config::create_from_str(TRANSFORM_WITH_ROLES).unwrap();
    config.get_processor("raw", "cs1").unwrap();
    config.get_processor("raw", "cs2").unwrap();
    config.get_processor("cs1", "cs2").unwrap();
    for src in ["raw", "cs1", "cs2"] {
        let dt: Transform = DisplayViewTransform::new(src, "Disp1", "View1").into();
        config
            .get_processor_for_transform(&dt, TransformDirection::Forward)
            .unwrap();
    }
}

#[test]
fn config_look_transform() {
    const CONFIG: &str = r#"
ocio_profile_version: 2

environment:
  {}

roles:
  default: raw

file_rules:
  - !<Rule> {name: Default, colorspace: default}

displays:
  Disp1:
  - !<View> {name: View1, colorspace: raw, looks: look1}

looks:
  - !<Look>
    name: look1
    process_space: default
    transform: !<ColorSpaceTransform> {src: default, dst: raw}
  - !<Look>
    name: look2
    process_space: default
    transform: !<LookTransform> {src: default, dst: raw, looks:+look1}

colorspaces:
  - !<ColorSpace>
    name: raw
    allocation: uniform
"#;
    let config = Config::create_from_str(CONFIG).unwrap();
    config.validate().unwrap();
}
