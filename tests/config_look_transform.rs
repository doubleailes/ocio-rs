//! Port of `transforms/LookTransform_tests.cpp`.

mod config_common;

use config_common::*;
use ocio::config::{collect_context_variables, BuildColorSpaceOps};
use ocio::ops::{OpRc, OpVec};
use ocio::transforms::BuildOps;
use ocio::*;

#[test]
fn look_transform_basic() {
    let mut look = LookTransform::default();
    assert_eq!(look.direction, TransformDirection::Forward);
    look.direction = TransformDirection::Inverse;
    assert_eq!(look.direction, TransformDirection::Inverse);

    assert_eq!(look.src, "");
    assert_eq!(look.dst, "");
    assert_eq!(look.looks, "");

    assert_err!(Transform::from(look.clone()).validate(), "empty source");
    look.src = "src".into();
    assert_err!(
        Transform::from(look.clone()).validate(),
        "empty destination"
    );
    look.dst = "dst".into();
    Transform::from(look.clone()).validate().unwrap();

    look.looks = "look1, look2, look3".into();
    assert_eq!(look.looks, "look1, look2, look3");

    assert!(!look.skip_color_space_conversion);
    look.skip_color_space_conversion = true;

    let copy = look.clone();
    assert_eq!(copy.src, "src");
    assert_eq!(copy.dst, "dst");
    assert_eq!(copy.looks, "look1, look2, look3");
    assert!(copy.skip_color_space_conversion);
}

/// Port of the local `ValidateTransform` helper: the op must be a fixed
/// function whose metadata name is `name`.
#[track_caller]
fn validate_transform(op: &OpRc, name: &str, dir: TransformDirection) {
    match op.to_transform() {
        Some(Transform::FixedFunction(ff)) => {
            assert_eq!(ff.metadata.attributes.len(), 1);
            assert_eq!(ff.metadata.attribute_value("name"), name);
            assert_eq!(ff.direction, dir);
        }
        t => panic!("expected a fixed function transform, got {t:?}"),
    }
}

fn build(config: &Config, lt: &LookTransform, dir: TransformDirection) -> Result<OpVec> {
    let mut ops = OpVec::new();
    lt.build_ops(&mut ops, config, config.current_context(), dir)?;
    Ok(ops)
}

const BUILD_LOOK_OPS_CONFIG: &str = r#"
ocio_profile_version: 2

roles:
  default: raw

displays:
  sRGB:
    - !<View> {name: Raw, colorspace: raw}

looks:
  - !<Look>
    name: look1
    process_space: look1_cs
    transform: !<FixedFunctionTransform> {name: look1 trans, style: ACES_RedMod03}

  - !<Look>
    name: look2
    process_space: look2_3_cs
    transform: !<FixedFunctionTransform> {name: look2 trans, style: ACES_RedMod03}
    inverse_transform: !<FixedFunctionTransform> {name: look2 inverse trans, style: ACES_RedMod03}

  - !<Look>
    name: look3
    process_space: look2_3_cs
    inverse_transform: !<FixedFunctionTransform> {name: look3 inverse trans, style: ACES_RedMod03}

colorspaces:
  - !<ColorSpace>
    name: raw
    family: raw
    bitdepth: 32f
    description: |
      A raw color space. Conversions to and from this space are no-ops.
    isdata: true

  - !<ColorSpace>
    name: source
    to_scene_reference: !<FixedFunctionTransform> {name: src, style: ACES_RedMod03}

  - !<ColorSpace>
    name: destination
    from_scene_reference: !<FixedFunctionTransform> {name: dst, style: ACES_RedMod03}

  - !<ColorSpace>
    name: look1_cs
    to_scene_reference: !<FixedFunctionTransform> {name: look1_cs trans, style: ACES_RedMod03}

  - !<ColorSpace>
    name: look2_3_cs
    to_scene_reference: !<FixedFunctionTransform> {name: look2_3_cs trans, style: ACES_RedMod03}
"#;

#[test]
fn look_transform_build_look_ops_config() {
    let config = Config::create_from_str(BUILD_LOOK_OPS_CONFIG).unwrap();
    config.validate().unwrap();
}

#[test]
#[ignore = "needs-merge"]
fn look_transform_build_look_ops() {
    use TransformDirection::{Forward, Inverse};
    let config = Config::create_from_str(BUILD_LOOK_OPS_CONFIG).unwrap();
    config.validate().unwrap();

    let lt = LookTransform::new("source", "destination", "look1, +look2, -look3");

    let ops = build(&config, &lt, Forward).unwrap();
    assert_eq!(ops.len(), 18);
    let noop = [0, 3, 4, 6, 9, 10, 12, 14, 17];
    for i in noop {
        assert!(ops[i].is_no_op(), "op {i}");
    }
    validate_transform(&ops[1], "src", Forward);
    validate_transform(&ops[2], "look1_cs trans", Inverse);
    validate_transform(&ops[5], "look1 trans", Forward);
    validate_transform(&ops[7], "look1_cs trans", Forward);
    validate_transform(&ops[8], "look2_3_cs trans", Inverse);
    validate_transform(&ops[11], "look2 trans", Forward);
    validate_transform(&ops[13], "look3 inverse trans", Forward);
    validate_transform(&ops[15], "look2_3_cs trans", Forward);
    validate_transform(&ops[16], "dst", Forward);

    let ops = build(&config, &lt, Inverse).unwrap();
    assert_eq!(ops.len(), 18);
    let noop = [0, 3, 4, 6, 8, 11, 12, 14, 17];
    for i in noop {
        assert!(ops[i].is_no_op(), "op {i}");
    }
    validate_transform(&ops[1], "dst", Inverse);
    validate_transform(&ops[2], "look2_3_cs trans", Inverse);
    validate_transform(&ops[5], "look3 inverse trans", Inverse);
    validate_transform(&ops[7], "look2 inverse trans", Forward);
    validate_transform(&ops[9], "look2_3_cs trans", Forward);
    validate_transform(&ops[10], "look1_cs trans", Inverse);
    validate_transform(&ops[13], "look1 trans", Inverse);
    validate_transform(&ops[15], "look1_cs trans", Forward);
    validate_transform(&ops[16], "src", Inverse);
}

const LOOK_OPTIONS_CONFIG: &str = r#"
ocio_profile_version: 2

search_path: luts

roles:
  default: raw

displays:
  sRGB:
    - !<View> {name: Raw, colorspace: raw}

looks:
  - !<Look>
    name: look1
    process_space: raw
    transform: !<FileTransform> {src: missingfile}

  - !<Look>
    name: look2
    process_space: look2_cs
    transform: !<FixedFunctionTransform> {name: look2 trans, style: ACES_RedMod03}

  - !<Look>
    name: look3
    process_space: look3_cs
    transform: !<FixedFunctionTransform> {name: look3 trans, style: ACES_RedMod03}

  - !<Look>
    name: look4
    process_space: look4_cs
    transform: !<FixedFunctionTransform> {name: look4 trans, style: ACES_RedMod03}

  - !<Look>
    name: look5
    process_space: raw
    transform: !<FileTransform> {src: missingfile}

colorspaces:
  - !<ColorSpace>
    name: raw
    family: raw
    bitdepth: 32f
    description: |
      A raw color space. Conversions to and from this space are no-ops.
    isdata: true

  - !<ColorSpace>
    name: source
    to_scene_reference: !<FixedFunctionTransform> {name: src, style: ACES_RedMod03}

  - !<ColorSpace>
    name: destination
    from_scene_reference: !<FixedFunctionTransform> {name: dst, style: ACES_RedMod03}

  - !<ColorSpace>
    name: look2_cs
    to_scene_reference: !<FixedFunctionTransform> {name: look2_cs trans, style: ACES_RedMod03}

  - !<ColorSpace>
    name: look3_cs
    to_scene_reference: !<FixedFunctionTransform> {name: look3_cs trans, style: ACES_RedMod03}

  - !<ColorSpace>
    name: look4_cs
    to_scene_reference: !<FixedFunctionTransform> {name: look4_cs trans, style: ACES_RedMod03}
"#;

#[test]
#[ignore = "needs-merge"]
fn look_transform_build_look_options_ops() {
    use TransformDirection::{Forward, Inverse};
    let config = Config::create_from_str(LOOK_OPTIONS_CONFIG).unwrap();
    config.validate().unwrap();

    let mut lt = LookTransform::new(
        "source",
        "destination",
        "look1 | look2, look3 | look3, look4",
    );

    let ops = build(&config, &lt, Forward).unwrap();
    assert_eq!(ops.len(), 16);
    for i in [0, 3, 4, 6, 9, 10, 12, 15] {
        assert!(ops[i].is_no_op(), "op {i}");
    }
    validate_transform(&ops[1], "src", Forward);
    validate_transform(&ops[2], "look2_cs trans", Inverse);
    validate_transform(&ops[5], "look2 trans", Forward);
    validate_transform(&ops[7], "look2_cs trans", Forward);
    validate_transform(&ops[8], "look3_cs trans", Inverse);
    validate_transform(&ops[11], "look3 trans", Forward);
    validate_transform(&ops[13], "look3_cs trans", Forward);
    validate_transform(&ops[14], "dst", Forward);

    let ops = build(&config, &lt, Inverse).unwrap();
    assert_eq!(ops.len(), 16);
    for i in [0, 3, 4, 6, 9, 10, 12, 15] {
        assert!(ops[i].is_no_op(), "op {i}");
    }
    validate_transform(&ops[1], "dst", Inverse);
    validate_transform(&ops[2], "look3_cs trans", Inverse);
    validate_transform(&ops[5], "look3 trans", Inverse);
    validate_transform(&ops[7], "look3_cs trans", Forward);
    validate_transform(&ops[8], "look2_cs trans", Inverse);
    validate_transform(&ops[11], "look2 trans", Inverse);
    validate_transform(&ops[13], "look2_cs trans", Forward);
    validate_transform(&ops[14], "src", Inverse);

    lt.looks = "look1 | look2, look5 | look5, look4".into();
    assert_err!(
        build(&config, &lt, Forward),
        "The specified file reference 'missingfile' could not be located"
    );
}

#[test]
fn look_transform_context_variables() {
    const CONFIG: &str = r#"
ocio_profile_version: 2

environment: { FILE1: cdl_test1.cc, FILE2: cdl_test1.cc }

roles:
  default: cs1

file_rules:
  - !<Rule> {name: Default, colorspace: default}

displays:
  Disp1:
  - !<View> {name: View1, colorspace: cs1}

looks:
  - !<Look>
    name: look1
    process_space: default
    transform: !<FileTransform> {src: $FILE1}
  - !<Look>
    name: look2
    process_space: default
    inverse_transform: !<LookTransform> {src: default, dst: cs2, looks: +look1}
  - !<Look>
    name: look3
    process_space: default
    transform: !<CDLTransform> {offset: [0.1, 0.1, 0.1]}
  - !<Look>
    name: look4
    process_space: cs4
    transform: !<CDLTransform> {offset: [0.1, 0.1, 0.1]}

colorspaces:
  - !<ColorSpace>
    name: cs1
  - !<ColorSpace>
    name: cs2
    from_reference: !<MatrixTransform> {offset: [0.11, 0.12, 0.13, 0]}
  - !<ColorSpace>
    name: cs3
    from_reference: !<MatrixTransform> {offset: [0.1, 0.2, 0.3, 0]}
  - !<ColorSpace>
    name: cs4
    from_reference: !<FileTransform> {src: $FILE2}
"#;
    let _lock = env_lock();
    let mut cfg = Config::create_from_str(CONFIG)
        .unwrap()
        .create_editable_copy();
    cfg.set_search_path(&data_file(""));
    cfg.validate().unwrap();

    let collect = |looks: &str| {
        let mut lt = LookTransform::new("cs1", "cs3", "");
        lt.looks = looks.to_string();
        let mut used = Context::new();
        let found =
            collect_context_variables(&cfg, cfg.current_context(), &lt.into(), &mut used).unwrap();
        (found, used)
    };
    let vars = |c: &Context| -> Vec<(String, String)> {
        (0..c.num_string_vars())
            .map(|i| {
                (
                    c.string_var_name_by_index(i).unwrap().to_string(),
                    c.string_var_by_index(i).unwrap().to_string(),
                )
            })
            .collect()
    };
    let pair = |a: &str, b: &str| (a.to_string(), b.to_string());

    let (found, used) = collect("");
    assert!(!found);
    assert_eq!(used.num_string_vars(), 0);

    // Step 1 - Test each basic cases.
    let (found, used) = collect("+look1");
    assert!(found);
    assert_eq!(vars(&used), [pair("FILE1", "cdl_test1.cc")]);

    let (found, used) = collect("-look2");
    assert!(found);
    assert_eq!(vars(&used), [pair("FILE1", "cdl_test1.cc")]);

    let (found, used) = collect("look3");
    assert!(!found);
    assert_eq!(used.num_string_vars(), 0);

    let (found, used) = collect("+look4");
    assert!(found);
    assert_eq!(vars(&used), [pair("FILE2", "cdl_test1.cc")]);

    // Step 2 - Test with several looks.
    let (found, used) = collect("look3, -look1");
    assert!(found);
    assert_eq!(vars(&used), [pair("FILE1", "cdl_test1.cc")]);

    let (found, used) = collect("look3, -look2, +look4");
    assert!(found);
    assert_eq!(
        vars(&used),
        [pair("FILE1", "cdl_test1.cc"), pair("FILE2", "cdl_test1.cc")]
    );
}

const INVERSE_LOOK_CONFIG: &str = r#"
ocio_profile_version: 2

search_path: luts

roles:
  default: raw

displays:
  sRGB:
    - !<View> {name: Raw, colorspace: raw}

looks:
  - !<Look>
    name: look1
    process_space: log
    transform: !<CDLTransform> {sat: 0.8}

colorspaces:
  - !<ColorSpace>
    name: raw
    family: raw
    bitdepth: 32f
    isdata: false

  - !<ColorSpace>
    name: log
    to_scene_reference: !<LogTransform> {base: 2, direction: inverse}

  - !<ColorSpace>
    name: vd
    from_scene_reference: !<ExponentTransform> {value: [2.4, 2.4, 2.4, 1], direction: inverse}

  - !<ColorSpace>
    name: vd_graded
    from_scene_reference: !<LookTransform> {src: raw, dst: vd, looks: look1}

  - !<ColorSpace>
    name: vd_graded_inverse
    to_scene_reference: !<LookTransform> {src: raw, dst: vd, looks: look1, direction: inverse}

"#;

fn transform_dir(op: &OpRc) -> (&'static str, TransformDirection) {
    match op.to_transform() {
        Some(Transform::Log(t)) => ("log", t.direction),
        Some(Transform::Cdl(t)) => ("cdl", t.direction),
        Some(Transform::Exponent(t)) => ("gamma", t.direction),
        Some(Transform::ExponentWithLinear(t)) => ("gamma", t.direction),
        t => panic!("unexpected op transform {t:?}"),
    }
}

#[test]
#[ignore = "needs-merge"]
fn look_transform_inverse_look_transform() {
    use TransformDirection::{Forward, Inverse};
    let config = Config::create_from_str(INVERSE_LOOK_CONFIG).unwrap();
    config.validate().unwrap();
    let ctx = config.current_context().clone();

    let src = config.get_color_space("raw").unwrap();
    let dst = config.get_color_space("vd_graded").unwrap();

    let mut ops = OpVec::new();
    BuildColorSpaceOps::build(&mut ops, &config, &ctx, src, dst, true).unwrap();
    assert_eq!(ops.len(), 11);
    for i in [0, 1, 3, 4, 6, 9, 10] {
        assert!(ops[i].is_no_op(), "op {i}");
    }
    assert_eq!(transform_dir(&ops[2]), ("log", Forward));
    assert_eq!(transform_dir(&ops[5]), ("cdl", Forward));
    assert_eq!(transform_dir(&ops[7]), ("log", Inverse));
    assert_eq!(transform_dir(&ops[8]), ("gamma", Inverse));

    let mut ops = OpVec::new();
    BuildColorSpaceOps::build(&mut ops, &config, &ctx, dst, src, true).unwrap();
    assert_eq!(ops.len(), 11);
    for i in [0, 1, 4, 5, 7, 9, 10] {
        assert!(ops[i].is_no_op(), "op {i}");
    }
    assert_eq!(transform_dir(&ops[2]), ("gamma", Forward));
    assert_eq!(transform_dir(&ops[3]), ("log", Forward));
    assert_eq!(transform_dir(&ops[6]), ("cdl", Inverse));
    assert_eq!(transform_dir(&ops[8]), ("log", Inverse));

    // Generated ops for vd_graded_inverse should be identical to the above.
    let dst_inv = config.get_color_space("vd_graded_inverse").unwrap();
    let mut ops2 = OpVec::new();
    BuildColorSpaceOps::build(&mut ops2, &config, &ctx, dst_inv, src, true).unwrap();
    assert_eq!(ops2.len(), ops.len());
    for (a, b) in ops.iter().zip(ops2.iter()) {
        assert_eq!(a.cache_id(), b.cache_id());
    }
}
