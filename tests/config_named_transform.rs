//! Port of `NamedTransform_tests.cpp`.

mod config_common;

use config_common::*;
use ocio::config::{ColorSpace, FileRules, Look, NamedTransform, ViewTransform};
use ocio::*;

fn matrix_with_offset(offset: [f64; 4]) -> Transform {
    MatrixTransform {
        offset,
        ..Default::default()
    }
    .into()
}

#[test]
fn named_transform_basic() {
    let _lock = env_lock();
    let mut nt = NamedTransform::new();
    assert!(nt.name().is_empty());
    assert!(nt.transform(TransformDirection::Forward).is_none());
    assert!(nt.transform(TransformDirection::Inverse).is_none());
    nt.set_name("NewName");
    assert_eq!(nt.name(), "NewName");

    nt.set_transform(
        Some(MatrixTransform::default().into()),
        TransformDirection::Forward,
    );
    assert!(matches!(
        nt.transform(TransformDirection::Forward),
        Some(Transform::Matrix(_))
    ));
    assert!(nt.transform(TransformDirection::Inverse).is_none());

    let fwd = NamedTransform::get_transform(&nt, TransformDirection::Forward).unwrap();
    assert!(matches!(fwd, Transform::Matrix(_)));
    let inv = NamedTransform::get_transform(&nt, TransformDirection::Inverse).unwrap();
    assert!(matches!(inv, Transform::Matrix(_)));

    assert_eq!(
        nt.to_string(),
        "<NamedTransform name=NewName,\n    forward=\n        \
<MatrixTransform direction=forward, fileindepth=unknown, fileoutdepth=unknown, \
matrix=[1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1], offset=[0, 0, 0, 0]>>"
    );

    // Faulty cases.
    let mut config = Config::create_raw().create_editable_copy();
    let mut nt_inv = NamedTransform::new();
    assert_err!(
        config.add_named_transform(&nt_inv),
        "Named transform must have a non-empty name"
    );
    nt_inv.set_name("name");
    assert_err!(
        config.add_named_transform(&nt_inv),
        "Named transform must define at least one transform"
    );
}

#[test]
fn named_transform_alias() {
    let _lock = env_lock();
    let mut nt = NamedTransform::new();
    assert_eq!(nt.num_aliases(), 0);
    const ALIAS_A: &str = "aliasA";
    const ALIAS_A_ALT: &str = "aLiaSa";
    const ALIAS_B: &str = "aliasB";
    nt.add_alias(ALIAS_A);
    assert_eq!(nt.num_aliases(), 1);
    assert!(nt.has_alias(ALIAS_A));
    assert!(nt.has_alias(ALIAS_A_ALT));
    assert!(!nt.has_alias(ALIAS_B));
    nt.add_alias(ALIAS_B);
    assert_eq!(nt.num_aliases(), 2);
    assert_eq!(nt.alias(0), ALIAS_A);
    assert_eq!(nt.alias(1), ALIAS_B);
    assert!(nt.has_alias(ALIAS_B));

    nt.add_alias(ALIAS_A_ALT);
    assert_eq!(nt.num_aliases(), 2);
    assert_eq!(nt.alias(0), ALIAS_A);
    assert_eq!(nt.alias(1), ALIAS_B);

    nt.remove_alias(ALIAS_A_ALT);
    assert_eq!(nt.num_aliases(), 1);
    assert_eq!(nt.alias(0), ALIAS_B);
    assert!(!nt.has_alias(ALIAS_A));
    assert!(!nt.has_alias(ALIAS_A_ALT));

    nt.add_alias(ALIAS_A_ALT);
    assert_eq!(nt.num_aliases(), 2);
    assert_eq!(nt.alias(0), ALIAS_B);
    assert_eq!(nt.alias(1), ALIAS_A_ALT);
    assert!(nt.has_alias(ALIAS_A));
    assert!(nt.has_alias(ALIAS_A_ALT));

    nt.set_name(ALIAS_A);
    assert_eq!(nt.name(), ALIAS_A);
    assert_eq!(nt.num_aliases(), 1);
    assert_eq!(nt.alias(0), ALIAS_B);
    assert!(!nt.has_alias(ALIAS_A));
    assert!(!nt.has_alias(ALIAS_A_ALT));

    nt.add_alias(ALIAS_A_ALT);
    assert_eq!(nt.name(), ALIAS_A);
    assert_eq!(nt.num_aliases(), 1);
    assert_eq!(nt.alias(0), ALIAS_B);
    assert!(!nt.has_alias(ALIAS_A_ALT));

    nt.add_alias("other");
    assert_eq!(nt.num_aliases(), 2);
    assert!(nt.has_alias("other"));
    nt.clear_aliases();
    assert_eq!(nt.num_aliases(), 0);
    assert!(!nt.has_alias(ALIAS_B));
    assert!(!nt.has_alias("other"));

    // Add and access named transforms in a config.
    let mut config = Config::create_raw().create_editable_copy();
    nt.set_name("name");
    nt.set_transform(
        Some(MatrixTransform::default().into()),
        TransformDirection::Forward,
    );
    {
        config.add_named_transform(&nt).unwrap();
        nt.set_name("other");
        nt.add_alias(ALIAS_B);
        config.add_named_transform(&nt).unwrap();
        assert_eq!(config.num_named_transforms(), 2);
        let ntcfg = config.get_named_transform("name").unwrap();
        assert_eq!(ntcfg.num_aliases(), 0);
    }
    {
        let ntcfg = config.get_named_transform(ALIAS_B).unwrap();
        assert_eq!(ntcfg.name(), "other");
        assert_eq!(ntcfg.num_aliases(), 1);
        assert_eq!(config.canonical_name(ALIAS_B), "other");
        assert_eq!(config.canonical_name("other"), "other");
        assert_eq!(config.canonical_name("not found"), "");
        assert_eq!(config.canonical_name(""), "");
    }
    {
        nt.set_name("name");
        nt.clear_aliases();
        nt.add_alias(ALIAS_A);
        config.add_named_transform(&nt).unwrap();
        assert_eq!(config.num_named_transforms(), 2);
        assert_eq!(config.get_named_transform("name").unwrap().num_aliases(), 1);
    }
    {
        nt.set_name(ALIAS_A);
        nt.clear_aliases();
        assert_err!(
            config.add_named_transform(&nt),
            "Cannot add 'aliasA' named transform, existing named transform, 'name' is using this name as an alias"
        );
    }
    {
        nt.set_name("newName");
        nt.add_alias(ALIAS_B);
        assert_err!(
            config.add_named_transform(&nt),
            "Cannot add 'newName' named transform, it has 'aliasB' alias and existing named transform, 'other' is using the same alias"
        );
    }
    {
        nt.add_alias("other");
        assert_err!(
            config.add_named_transform(&nt),
            "Cannot add 'newName' named transform, it has 'aliasB' alias and existing named transform, 'other' is using the same alias"
        );
    }
}

/// Check that the processor holds a single matrix with the given metadata
/// name and offset.
fn check_single_matrix(proc: &Processor, name: &str, offset: &[f64]) {
    let group = proc.create_group_transform();
    assert_eq!(group.transforms.len(), 1);
    check_matrix(&group.transforms[0], name, offset);
}

fn check_matrix(t: &Transform, name: &str, offset: &[f64]) {
    match t {
        Transform::Matrix(m) => {
            if !name.is_empty() {
                assert_eq!(m.metadata.attribute_value("name"), name);
            }
            for (i, o) in offset.iter().enumerate() {
                assert_eq!(m.offset[i], *o, "offset {i}");
            }
        }
        other => panic!("expected a matrix, got {other:?}"),
    }
}

#[test]
#[ignore = "needs-merge"]
fn named_transform_static_get_transform() {
    let _lock = env_lock();
    let config = Config::create_raw();
    let offset_f = [0.1, 0.2, 0.3, 0.4];
    let offset_i = [-0.1, -0.2, -0.3, -0.4];

    let mut nt1 = NamedTransform::new();
    nt1.set_transform(
        Some(matrix_with_offset(offset_f)),
        TransformDirection::Forward,
    );
    let mut nt2 = NamedTransform::new();
    nt2.set_transform(
        Some(matrix_with_offset(offset_i)),
        TransformDirection::Inverse,
    );

    let cases = [
        (&nt1, TransformDirection::Forward, offset_f),
        (&nt1, TransformDirection::Inverse, offset_i),
        (&nt2, TransformDirection::Forward, offset_f),
        (&nt2, TransformDirection::Inverse, offset_i),
    ];
    for (nt, dir, expected) in cases {
        let t = NamedTransform::get_transform(nt, dir).unwrap();
        let proc = config
            .get_processor_for_transform(&t, TransformDirection::Forward)
            .unwrap();
        check_single_matrix(&proc, "", &expected);
    }
}

const NT_PROCESSOR_CONFIG: &str = r#"ocio_profile_version: 2

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
    - !<View> {name: ntview, colorspace: ntf}

active_displays: []
active_views: []

display_colorspaces:
  - !<ColorSpace>
    name: dcs
    aliases: [display color space]
    isdata: false
    allocation: uniform
    from_display_reference: !<RangeTransform> {min_in_value: 0, min_out_value: 0}

colorspaces:
  - !<ColorSpace>
    name: raw
    family: raw
    bitdepth: 32f
    description: |
      A raw color space. Conversions to and from this space are no-ops.
    isdata: true
    allocation: uniform

  - !<ColorSpace>
    name: cs
    aliases: [colorspace]
    isdata: false
    allocation: uniform
    to_scene_reference: !<RangeTransform> {max_in_value: 1, max_out_value: 1}

named_transforms:
  - !<NamedTransform>
    name: forward
    aliases: [nt1, ntf]
    encoding: scene-linear
    transform: !<MatrixTransform> {name: forward, offset: [0.1, 0.2, 0.3, 0.4]}

  - !<NamedTransform>
    name: inverse
    aliases: [nt2, nti]
    inverse_transform: !<MatrixTransform> {name: inverse, offset: [-0.2, -0.1, -0.1, 0]}

  - !<NamedTransform>
    name: both
    aliases: [nt3, ntb]
    transform: !<MatrixTransform> {name: forward, offset: [0.1, 0.2, 0.3, 0.4]}
    inverse_transform: !<MatrixTransform> {name: inverse, offset: [-0.2, -0.1, -0.1, 0]}
"#;

#[test]
fn config_named_transform_processor_access() {
    let _lock = env_lock();
    // The part of the test that does not need to build ops.
    let config = Config::create_from_str(NT_PROCESSOR_CONFIG).unwrap();
    let nt = config.get_named_transform("forward").unwrap();
    assert_eq!(nt.encoding(), "scene-linear");
    assert!(nt.transform(TransformDirection::Forward).is_some());
    let nt = config.get_named_transform("nt1").unwrap();
    assert_eq!(nt.name(), "forward");
}

#[test]
#[ignore = "needs-merge"]
fn config_named_transform_processor() {
    let _lock = env_lock();
    let config = Config::create_from_str(NT_PROCESSOR_CONFIG).unwrap();
    let context = config.current_context().clone();

    const FWD: &str = "forward";
    const INV: &str = "inverse";
    let offset_f = [0.1, 0.2, 0.3];
    let offset_i = [-0.2, -0.1, -0.1];
    let neg = |o: [f64; 3]| [-o[0], -o[1], -o[2]];

    for name in ["forward", "nt1"] {
        let nt = config.get_named_transform(name).unwrap();
        let tf = nt.transform(TransformDirection::Forward).unwrap();
        let proc = config
            .get_processor_for_transform(tf, TransformDirection::Forward)
            .unwrap();
        check_single_matrix(&proc, FWD, &offset_f);
    }

    let nt = config.get_named_transform("forward").unwrap().clone();
    let proc = config
        .get_processor_for_named_transform(&nt, TransformDirection::Forward)
        .unwrap();
    check_single_matrix(&proc, FWD, &offset_f);
    let proc = config
        .get_processor_for_named_transform(&nt, TransformDirection::Inverse)
        .unwrap();
    check_single_matrix(&proc, FWD, &neg(offset_f));

    let nt = config.get_named_transform("inverse").unwrap().clone();
    let proc = config
        .get_processor_for_named_transform_with_context(&context, &nt, TransformDirection::Forward)
        .unwrap();
    check_single_matrix(&proc, INV, &neg(offset_i));
    let proc = config
        .get_processor_for_named_transform_with_context(&context, &nt, TransformDirection::Inverse)
        .unwrap();
    check_single_matrix(&proc, INV, &offset_i);

    let proc = config
        .get_processor_named_transform("inverse", TransformDirection::Forward)
        .unwrap();
    check_single_matrix(&proc, INV, &neg(offset_i));
    let proc = config
        .get_processor_named_transform("inverse", TransformDirection::Inverse)
        .unwrap();
    check_single_matrix(&proc, INV, &offset_i);

    let proc = config
        .get_processor_named_transform_with_context(
            &context,
            "forward",
            TransformDirection::Forward,
        )
        .unwrap();
    check_single_matrix(&proc, FWD, &offset_f);
    let proc = config
        .get_processor_named_transform_with_context(
            &context,
            "forward",
            TransformDirection::Inverse,
        )
        .unwrap();
    check_single_matrix(&proc, FWD, &neg(offset_f));

    let proc = config
        .get_processor_named_transform("ntb", TransformDirection::Forward)
        .unwrap();
    check_single_matrix(&proc, FWD, &offset_f);
    let proc = config
        .get_processor_named_transform("nt3", TransformDirection::Inverse)
        .unwrap();
    check_single_matrix(&proc, INV, &offset_i);

    // Display color space to named transform.
    check_single_matrix(
        &config.get_processor("dcs", FWD).unwrap(),
        FWD,
        &neg(offset_f),
    );
    check_single_matrix(
        &config.get_processor("display color space", "ntf").unwrap(),
        FWD,
        &neg(offset_f),
    );

    // Color space to named transform.
    check_single_matrix(&config.get_processor("cs", INV).unwrap(), INV, &offset_i);
    check_single_matrix(
        &config.get_processor("colorspace", "nt2").unwrap(),
        INV,
        &offset_i,
    );

    // Display color space to named transform (using ColorSpaceTransform).
    let cst: Transform = ColorSpaceTransform::new("dcs", "both").into();
    let proc = config
        .get_processor_for_transform(&cst, TransformDirection::Forward)
        .unwrap();
    check_single_matrix(&proc, INV, &offset_i);

    // Named transform to color space.
    check_single_matrix(&config.get_processor(FWD, "cs").unwrap(), FWD, &offset_f);
    check_single_matrix(
        &config.get_processor("ntf", "colorspace").unwrap(),
        FWD,
        &offset_f,
    );

    // Named transform to display color space.
    check_single_matrix(
        &config.get_processor(INV, "dcs").unwrap(),
        INV,
        &neg(offset_i),
    );
    check_single_matrix(&config.get_processor("both", "cs").unwrap(), FWD, &offset_f);

    // Named transform to named transform.
    for (a, b) in [("both", "both"), ("nt3", "ntb")] {
        let proc = config.get_processor(a, b).unwrap();
        let group = proc.create_group_transform();
        assert_eq!(group.transforms.len(), 2);
        check_matrix(&group.transforms[0], FWD, &offset_f);
        check_matrix(&group.transforms[1], INV, &offset_i);
    }

    let proc = config
        .get_display_view_processor_dir("colorspace", "sRGB", "ntview", TransformDirection::Forward)
        .unwrap();
    check_single_matrix(&proc, FWD, &offset_f);
}

fn validation_config() -> Config {
    let mut config = Config::create_raw().create_editable_copy();
    let mut nt = NamedTransform::new();
    nt.set_name("name");
    let mat = matrix_with_offset([0.1, 0.2, 0.3, 0.4]);
    nt.set_transform(Some(mat.clone()), TransformDirection::Forward);
    config.add_named_transform(&nt).unwrap();
    assert_eq!(config.num_named_transforms(), 1);

    nt.set_name("other_name");
    nt.set_transform(Some(mat), TransformDirection::Inverse);
    config.add_named_transform(&nt).unwrap();
    assert_eq!(config.num_named_transforms(), 2);
    config
}

#[test]
fn config_named_transform_validation() {
    let _lock = env_lock();
    let mut config = validation_config();
    config.validate().unwrap();

    assert_eq!(config.named_transform_name_by_index(0), "name");
    assert_eq!(config.named_transform_name_by_index(1), "other_name");
    assert_eq!(config.named_transform_name_by_index(2), "");

    let nt = config.get_named_transform("name").unwrap();
    assert!(nt.transform(TransformDirection::Forward).is_some());
    assert!(nt.transform(TransformDirection::Inverse).is_none());
    let nt = config.get_named_transform("other_name").unwrap();
    assert!(nt.transform(TransformDirection::Forward).is_some());
    assert!(nt.transform(TransformDirection::Inverse).is_some());

    assert_err!(
        config.get_processor("raw", "missing"),
        "Color space 'missing' could not be found"
    );

    // NamedTransform can't use a role name.
    assert_err!(
        config.set_role("name", Some("raw")),
        "Cannot add 'name' role, there is already a named transform using this as a name or an alias"
    );
    config.set_role("name", None).unwrap();

    // NamedTransform can't use a color space name.
    let mut cs = ColorSpace::default();
    cs.set_name("name");
    assert_err!(
        config.add_color_space(&cs),
        "Cannot add 'name' color space, there is already a named transform using this name as a name or as an alias: 'name'"
    );
    config.remove_color_space("name");

    // NamedTransform can't use a look name.
    let mut look = Look::new();
    look.set_name("name");
    look.set_process_space("raw");
    config.add_look(&look).unwrap();
    assert_err!(config.validate(), "This name is already used for a look");
    config.clear_looks();

    // NamedTransform can't use a view transform name.
    let mut vt = ViewTransform::new(ReferenceSpaceType::Scene);
    vt.set_name("name");
    vt.set_transform(
        Some(MatrixTransform::default().into()),
        ViewTransformDirection::ToReference,
    );
    config.add_view_transform(&vt).unwrap();
    assert_err!(
        config.validate(),
        "This name is already used for a view transform"
    );
    config.clear_view_transforms();

    config.set_major_version(1).unwrap();
    config.set_file_rules(&FileRules::new());
    assert_err!(
        config.validate(),
        "Only version 2 (or higher) can have NamedTransforms"
    );
}

#[test]
#[ignore = "needs-merge"]
fn config_named_transform_validation_processors() {
    let _lock = env_lock();
    let config = validation_config();
    config.get_processor("raw", "name").unwrap();
    config.get_processor("name", "name").unwrap();
}

const NT_IO_CONFIG_START: &str = r#"ocio_profile_version: 2

environment:
  {}
search_path: ""
strictparsing: true
luma: [0.2126, 0.7152, 0.0722]

roles:
  default: raw

file_rules:
  - !<Rule> {name: Default, colorspace: default}

displays:
  Disp1:
    - !<View> {name: View1, colorspace: raw}

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

"#;

#[test]
fn config_named_transform_io() {
    let _lock = env_lock();
    {
        const NT: &str = r#"named_transforms:
  - !<NamedTransform>
    name: namedTransform1
    aliases: [named1, named2]
    family: family
    categories: [input, basic]
    encoding: data
    transform: !<ColorSpaceTransform> {src: default, dst: raw}

  - !<NamedTransform>
    name: namedTransform2
    inverse_transform: !<ColorSpaceTransform> {src: default, dst: raw}
"#;
        let config_str = format!("{NT_IO_CONFIG_START}{NT}");
        let config = Config::create_from_str(&config_str).unwrap();
        config.validate().unwrap();

        assert_eq!(config.num_named_transforms(), 2);
        assert_eq!(config.named_transform_name_by_index(0), "namedTransform1");
        assert_eq!(config.named_transform_name_by_index(1), "namedTransform2");
        let nt = config.get_named_transform("namedTransform1").unwrap();
        assert_eq!(nt.num_aliases(), 2);
        assert_eq!(nt.alias(0), "named1");
        assert_eq!(nt.alias(1), "named2");
        assert_eq!(nt.family(), "family");
        assert_eq!(nt.num_categories(), 2);
        assert_eq!(nt.category(0), Some("input"));
        assert_eq!(nt.category(1), Some("basic"));
        assert_eq!(nt.encoding(), "data");
        assert_eq!(config.serialize().unwrap(), config_str);

        // Look can't use named transform.
        let mut look = Look::new();
        look.set_name("look");
        look.set_process_space("namedTransform1");
        let mut config_edit = config.create_editable_copy();
        config_edit.add_look(&look).unwrap();
        assert_err!(
            config_edit.validate(),
            "process color space, 'namedTransform1', which is not defined"
        );
        config_edit.clear_looks();

        // Role can't use named transform.
        config_edit
            .set_role("newrole", Some("namedTransform1"))
            .unwrap();
        assert_err!(
            config_edit.validate(),
            "refers to a color space, 'namedTransform1', which is not defined"
        );
        config_edit.set_role("newrole", None).unwrap();

        // File rule can use named transform.
        let mut rules = config_edit.file_rules().clone();
        rules
            .insert_rule(0, "newrule", "namedTransform1", "*", "*")
            .unwrap();
        config_edit.set_file_rules(&rules);
        config_edit.validate().unwrap();
    }
    {
        const NT: &str = "named_transforms:\n  - !<NamedTransform>\n    name: namedTransform1";
        let config_str = format!("{NT_IO_CONFIG_START}{NT}");
        assert_err!(
            Config::create_from_str(&config_str),
            "Named transform must define at least one transform."
        );
    }
    {
        const NT: &str = r#"named_transforms:
  - !<NamedTransform>
    name: namedTransform1
    transform: !<ColorSpaceTransform> {src: default}
"#;
        let config_str = format!("{NT_IO_CONFIG_START}{NT}");
        let config = Config::create_from_str(&config_str).unwrap();
        assert_err!(
            config.validate(),
            "ColorSpaceTransform: empty destination color space name"
        );
    }
}

#[test]
fn config_colorspace_transform_named_transform() {
    let _lock = env_lock();
    const CONFIG: &str = r#"
ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: raw}

displays:
  sRGB:
    - !<View> {name: Raw, colorspace: raw}
  Rec.2100-PQ - Display:
    - !<View> {name: test_view, view_transform: vt, display_colorspace: Rec.2100-PQ - Display}

view_transforms:
  - !<ViewTransform>
    name: vt
    from_scene_reference: !<ColorSpaceTransform> {src: nt, dst: cs2}

display_colorspaces:
  - !<ColorSpace>
    name: Rec.2100-PQ - Display
    isdata: false
    from_display_reference: !<BuiltinTransform> {style: DISPLAY - CIE-XYZ-D65_to_REC.2100-PQ}

colorspaces:
  - !<ColorSpace>
    name: raw
    isdata: true

  - !<ColorSpace>
    name: cs2
    isdata: false
    from_scene_reference: !<MatrixTransform> {matrix: [ 2.041587903811, -0.565006974279, -0.344731350778, 0, -0.969243636281, 1.875967501508, 0.041555057407, 0, 0.013444280632, -0.118362392231, 1.015174994391, 0, 0, 0, 0, 1 ]}

  - !<ColorSpace>
    name: cs3
    isdata: false
    from_scene_reference: !<ColorSpaceTransform> {src: nt_alias, dst: cs2}

  - !<ColorSpace>
    name: cs4
    isdata: false
    from_scene_reference: !<DisplayViewTransform> {src: nt_alias, display: Rec.2100-PQ - Display, view: test_view}

named_transforms:
  - !<NamedTransform>
    name: nt
    aliases: [nt_alias]
    transform: !<GroupTransform>
      children:
        - !<MatrixTransform> {matrix: [1.49086870465701, -0.268712979082956, -0.222155725704626, 0, -0.0792372106028327, 1.1793685831111, -0.100131372460806, 0, 0.00277810076707935, -0.0304336146315336, 1.02765551391237, 0, 0, 0, 0, 1]}
"#;
    let config = Config::create_from_str(CONFIG).unwrap();
    config.validate().unwrap();
}

const INACTIVE_NT_CONFIG_START: &str = r#"ocio_profile_version: 2

environment:
  {}
search_path: luts
strictparsing: true
luma: [0.2126, 0.7152, 0.0722]

roles:
  default: raw
  scene_linear: lnh

file_rules:
  - !<Rule> {name: Default, colorspace: default}

displays:
  sRGB:
    - !<View> {name: Raw, colorspace: raw}
    - !<View> {name: Lnh, colorspace: lnh, looks: beauty}

active_displays: []
active_views: []
"#;

const INACTIVE_NT_CONFIG_END: &str = r#"
looks:
  - !<Look>
    name: beauty
    process_space: lnh
    transform: !<CDLTransform> {slope: [1, 2, 1]}


colorspaces:
  - !<ColorSpace>
    name: raw
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    allocation: uniform

  - !<ColorSpace>
    name: lnh
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    allocation: uniform

named_transforms:
  - !<NamedTransform>
    name: nt1
    aliases: [alias1]
    categories: [cat1]
    transform: !<CDLTransform> {offset: [0.1, 0.1, 0.1]}

  - !<NamedTransform>
    name: nt2
    categories: [cat2]
    transform: !<CDLTransform> {offset: [0.2, 0.2, 0.2]}

  - !<NamedTransform>
    name: nt3
    categories: [cat3]
    transform: !<CDLTransform> {offset: [0.3, 0.3, 0.3]}
"#;

fn inactive_config() -> Config {
    let s = format!("{INACTIVE_NT_CONFIG_START}{INACTIVE_NT_CONFIG_END}");
    let config = Config::create_from_str(&s).unwrap().create_editable_copy();
    config.validate().unwrap();
    config
}

#[test]
fn config_inactive_named_transforms() {
    let _lock = env_lock();
    use NamedTransformVisibility as V;
    let mut config = inactive_config();

    // Step 1 - No inactive named transforms.
    assert_eq!(config.num_named_transforms_filtered(V::Inactive), 0);
    assert_eq!(config.num_named_transforms_filtered(V::Active), 3);
    assert_eq!(config.num_named_transforms_filtered(V::All), 3);
    assert_eq!(
        config.named_transform_name_by_index_filtered(V::All, 0),
        "nt1"
    );
    assert_eq!(
        config.named_transform_name_by_index_filtered(V::All, 1),
        "nt2"
    );
    assert_eq!(
        config.named_transform_name_by_index_filtered(V::All, 2),
        "nt3"
    );
    assert_eq!(config.named_transform_name_by_index_filtered(V::All, 3), "");
    assert_eq!(config.num_named_transforms(), 3);
    assert_eq!(config.named_transform_name_by_index(0), "nt1");
    assert_eq!(config.named_transform_name_by_index(1), "nt2");
    assert_eq!(config.named_transform_name_by_index(2), "nt3");
    assert_eq!(config.named_transform_name_by_index(3), "");

    // Step 2 - Some inactive color space and named transforms (aliases can be used).
    config.set_inactive_color_spaces("lnh, alias1");
    assert_eq!(config.inactive_color_spaces(), "lnh, alias1");

    let n = |c: &Config, v| c.num_color_spaces_filtered(SearchReferenceSpaceType::All, v);
    assert_eq!(n(&config, ColorSpaceVisibility::Inactive), 1);
    assert_eq!(n(&config, ColorSpaceVisibility::Active), 1);
    assert_eq!(n(&config, ColorSpaceVisibility::All), 2);

    assert_eq!(config.num_named_transforms_filtered(V::Inactive), 1);
    assert_eq!(config.num_named_transforms_filtered(V::Active), 2);
    assert_eq!(config.num_named_transforms_filtered(V::All), 3);

    assert_eq!(
        config.named_transform_name_by_index_filtered(V::All, 0),
        "nt1"
    );
    assert_eq!(
        config.named_transform_name_by_index_filtered(V::All, 1),
        "nt2"
    );
    assert_eq!(
        config.named_transform_name_by_index_filtered(V::All, 2),
        "nt3"
    );
    assert_eq!(
        config.named_transform_name_by_index_filtered(V::Active, 0),
        "nt2"
    );
    assert_eq!(
        config.named_transform_name_by_index_filtered(V::Active, 1),
        "nt3"
    );
    assert_eq!(
        config.named_transform_name_by_index_filtered(V::Inactive, 0),
        "nt1"
    );

    assert_eq!(config.num_named_transforms(), 2);
    assert_eq!(config.named_transform_name_by_index(0), "nt2");
    assert_eq!(config.named_transform_name_by_index(1), "nt3");

    assert_eq!(config.get_named_transform("nt2").unwrap().name(), "nt2");
    let nt = config.get_named_transform("nt1").unwrap();
    assert_eq!(nt.name(), "nt1");
    assert_eq!(config.index_for_named_transform("nt1"), None);
    let nt = config.get_named_transform("alias1").unwrap();
    assert_eq!(nt.name(), "nt1");
    assert_eq!(config.index_for_named_transform("nt1"), None);

    // Step 3 - No inactive color spaces or named transforms.
    config.set_inactive_color_spaces("");
    assert_eq!(config.inactive_color_spaces(), "");
    assert_eq!(n(&config, ColorSpaceVisibility::All), 2);
    assert_eq!(config.num_color_spaces(), 2);
    assert_eq!(config.num_named_transforms_filtered(V::All), 3);
    assert_eq!(config.num_named_transforms(), 3);

    // Step 4.
    config.set_inactive_color_spaces("lnh, nt1");
    assert_eq!(config.inactive_color_spaces(), "lnh, nt1");
    config.set_inactive_color_spaces("");
    assert_eq!(config.inactive_color_spaces(), "");
    assert_eq!(n(&config, ColorSpaceVisibility::All), 2);
    assert_eq!(config.num_color_spaces(), 2);
    assert_eq!(config.num_named_transforms_filtered(V::All), 3);
    assert_eq!(config.num_named_transforms(), 3);
}

#[test]
#[ignore = "needs-merge"]
fn config_inactive_named_transforms_processors() {
    let _lock = env_lock();
    let mut config = inactive_config();
    config.set_inactive_color_spaces("lnh, alias1");
    config.get_processor("lnh", "nt1").unwrap();
    config.get_processor("raw", "nt1").unwrap();
    config.get_processor("lnh", "nt2").unwrap();
    config.get_processor("nt2", "scene_linear").unwrap();
}

#[test]
fn config_inactive_named_transform_precedence() {
    let _lock = env_lock();
    use NamedTransformVisibility as V;
    let config_str =
        format!("{INACTIVE_NT_CONFIG_START}inactive_colorspaces: [nt2]\n{INACTIVE_NT_CONFIG_END}");

    let _unset = EnvGuard::set(OCIO_INACTIVE_COLORSPACES_ENVVAR, None);
    let config = Config::create_from_str(&config_str)
        .unwrap()
        .create_editable_copy();
    config.validate().unwrap();

    let n = |c: &Config, v| c.num_color_spaces_filtered(SearchReferenceSpaceType::All, v);
    assert_eq!(config.num_named_transforms_filtered(V::Inactive), 1);
    assert_eq!(config.num_named_transforms_filtered(V::Active), 2);
    assert_eq!(config.num_named_transforms_filtered(V::All), 3);
    assert_eq!(n(&config, ColorSpaceVisibility::All), 2);
    assert_eq!(config.num_color_spaces(), 2);
    assert_eq!(config.named_transform_name_by_index(0), "nt1");
    assert_eq!(config.named_transform_name_by_index(1), "nt3");

    // Env. variable supersedes the config content.
    let _guard = EnvGuard::set(OCIO_INACTIVE_COLORSPACES_ENVVAR, Some("nt3, nt1, lnh"));
    let mut config = Config::create_from_str(&config_str)
        .unwrap()
        .create_editable_copy();
    config.validate().unwrap();

    assert_eq!(config.num_named_transforms_filtered(V::Inactive), 2);
    assert_eq!(config.num_named_transforms_filtered(V::Active), 1);
    assert_eq!(config.num_named_transforms_filtered(V::All), 3);
    assert_eq!(n(&config, ColorSpaceVisibility::All), 2);
    assert_eq!(config.num_color_spaces(), 1);
    assert_eq!(config.named_transform_name_by_index(0), "nt2");

    // An API request supersedes the lists from the env. variable and the config file.
    config.set_inactive_color_spaces("nt1, lnh");
    assert_eq!(config.num_named_transforms_filtered(V::Inactive), 1);
    assert_eq!(config.num_named_transforms_filtered(V::Active), 2);
    assert_eq!(config.num_named_transforms_filtered(V::All), 3);
    assert_eq!(n(&config, ColorSpaceVisibility::All), 2);
    assert_eq!(config.num_color_spaces(), 1);
    assert_eq!(config.named_transform_name_by_index(0), "nt2");
    assert_eq!(config.named_transform_name_by_index(1), "nt3");
}
