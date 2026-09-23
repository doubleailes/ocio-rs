//! Port of `Config_tests.cpp` (part 9: family separator, displays, color
//! space usage, versions, dynamic properties, builtin transforms, cache ids).

mod config_common;

use config_common::*;
use ocio::config::ColorSpace;
use ocio::*;

#[test]
fn config_family_separator() {
    let mut cfg = Config::create_raw().create_editable_copy();
    cfg.validate().unwrap();
    assert_eq!(cfg.family_separator(), '/');
    cfg.set_family_separator(' ').unwrap();
    assert_eq!(cfg.family_separator(), ' ');
    cfg.set_family_separator('\0').unwrap();
    assert_eq!(cfg.family_separator(), '\0');

    assert_eq!(Config::default_family_separator(), '/');
    cfg.set_family_separator(Config::default_family_separator()).unwrap();
    assert_eq!(cfg.family_separator(), '/');

    assert!(cfg.set_family_separator(127 as char).is_err());
    assert!(cfg.set_family_separator(31 as char).is_err());

    const CONFIG: &str = r#"ocio_profile_version: 2

environment:
  {}
search_path: ""
strictparsing: false
family_separator: " "
luma: [0.2126, 0.7152, 0.0722]

roles:
  default: raw

file_rules:
  - !<Rule> {name: Default, colorspace: default}

displays:
  sRGB:
    - !<View> {name: Raw, colorspace: raw}

active_displays: []
active_views: []

colorspaces:
  - !<ColorSpace>
    name: raw
    family: raw
    equalitygroup: ""
    bitdepth: 32f
    description: A raw color space. Conversions to and from this space are no-ops.
    isdata: true
    allocation: uniform
"#;
    cfg.set_family_separator(' ').unwrap();
    assert_eq!(cfg.serialize().unwrap(), CONFIG);

    const CONFIG_V1: &str = r#"ocio_profile_version: 1

search_path: ""

roles:
  reference: raw

displays:
  sRGB:
    - !<View> {name: Raw, colorspace: raw}

colorspaces:
  - !<ColorSpace>
    name: raw
    allocation: uniform
"#;
    let mut cfg = Config::create_from_str(CONFIG_V1).unwrap().create_editable_copy();
    assert_eq!(cfg.family_separator(), '/');
    cfg.set_family_separator('&').unwrap();
    assert_err!(cfg.validate(), "Only version 2 (or higher) can have a family separator.");
    assert_err!(cfg.serialize(), "Only version 2 (or higher) can have a family separator.");

    let v1bis = CONFIG_V1.replace("search_path: \"\"\n", "search_path: \"\"\nfamily_separator: \"/\"\n");
    assert_err!(Config::create_from_str(&v1bis), "Config v1 can't have 'family_separator'.");
}

#[test]
fn config_add_remove_display() {
    let mut config = Config::create_raw().create_editable_copy();
    config.validate().unwrap();
    assert_eq!(config.num_displays(), 1);
    assert_eq!(config.display(0), "sRGB");
    assert_eq!(config.num_views("sRGB"), 1);
    assert_eq!(config.view("sRGB", 0), "Raw");

    config.add_display_view("disp1", "view1", "raw", "").unwrap();
    assert!(config.has_view("disp1", "view1"));
    assert_eq!(config.num_displays(), 2);
    assert_eq!(config.display(0), "sRGB");
    assert_eq!(config.display(1), "disp1");
    assert_eq!(config.num_views("disp1"), 1);

    config.remove_display_view("disp1", "view1").unwrap();
    assert!(!config.has_view("disp1", "view1"));
    assert_eq!(config.num_displays(), 1);
    assert_eq!(config.display(0), "sRGB");
}

#[test]
fn config_is_colorspace_used() {
    const CONFIG: &str = r#"ocio_profile_version: 2

environment:
  {}

search_path: luts
strictparsing: true
luma: [0.2126, 0.7152, 0.0722]

roles:
  default: cs1

view_transforms:
  - !<ViewTransform>
    name: vt1
    from_scene_reference: !<ColorSpaceTransform> {src: cs11, dst: cs11}

displays:
  disp1:
    - !<View> {name: view1, colorspace: cs2}
    - !<View> {name: view2, colorspace: cs9}

active_displays: [disp1]
active_views: [view1]

file_rules:
  - !<Rule> {name: rule1, colorspace: cs10, pattern: "*", extension: "*"}
  - !<Rule> {name: Default, colorspace: default}

looks:
  - !<Look>
    name: beauty
    process_space: cs5
    transform: !<ColorSpaceTransform> {src: cs6, dst: cs6}


colorspaces:
  - !<ColorSpace>
    name: cs1

  - !<ColorSpace>
    name: cs2

  - !<ColorSpace>
    name: cs3

  - !<ColorSpace>
    name: cs4
    from_scene_reference: !<ColorSpaceTransform> {src: cs3, dst: cs3}

  - !<ColorSpace>
    name: cs5

  - !<ColorSpace>
    name: cs6

  - !<ColorSpace>
    name: cs7

  - !<ColorSpace>
    name: cs8
    from_scene_reference: !<GroupTransform>
      children:
        - !<ColorSpaceTransform> {src: cs7, dst: cs7}

  - !<ColorSpace>
    name: cs9
    from_scene_reference: !<GroupTransform>
      children:
        - !<GroupTransform>
             children:
               - !<LookTransform> {src: cs8, dst: cs8}

  - !<ColorSpace>
    name: cs10

  - !<ColorSpace>
    name: cs11
"#;
    let _lock = env_lock();
    let config = Config::create_from_str(CONFIG).unwrap();
    config.validate().unwrap();
    for cs in ["cs1", "cs2", "cs3", "cs5", "cs6", "cs7", "cs8", "cs9", "cs10", "cs11"] {
        assert!(config.is_color_space_used(cs), "{cs}");
    }
    assert!(!config.is_color_space_used("cs4"));
    assert!(!config.is_color_space_used(""));
    assert!(!config.is_color_space_used("cs65"));
}

#[test]
fn config_transform_versions() {
    let mut config = Config::create();
    assert_eq!(config.major_version(), 2);
    config.set_major_version(1).unwrap();
    config.set_minor_version(0).unwrap();
    assert_eq!(config.major_version(), 1);

    let mut cs = ColorSpace::default();
    cs.set_name("range");
    cs.set_transform(Some(RangeTransform::default().into()), ColorSpaceDirection::ToReference);
    config.add_color_space(&cs).unwrap();
    assert_err!(
        config.serialize(),
        "Error building YAML: Only config version 2 (or higher) can have RangeTransform."
    );

    const CONFIG: &str = r#"
ocio_profile_version: 1

roles:
  default: raw

colorspaces:
  - !<ColorSpace>
    name: raw
    allocation: uniform
    from_reference: !<GroupTransform>
       children:
         - !<RangeTransform> {min_in_value: 0, min_out_value: 0}
"#;
    assert_err!(
        Config::create_from_str(CONFIG),
        "Only config version 2 (or higher) can have RangeTransform."
    );
}

#[test]
fn config_dynamic_properties() {
    let mut config = Config::create_raw().create_editable_copy();
    let mut cs = ColorSpace::default();
    cs.set_name("test");
    let ec = ExposureContrastTransform { exposure_dynamic: true, ..Default::default() };
    cs.set_transform(Some(ec.into()), ColorSpaceDirection::ToReference);
    config.add_color_space(&cs).unwrap();
    config.validate().unwrap();

    let mut gp = GradingPrimaryTransform::new(GradingStyle::Log);
    gp.dynamic = true;
    cs.set_transform(Some(gp.into()), ColorSpaceDirection::FromReference);
    config.add_color_space(&cs).unwrap();
    config.validate().unwrap();

    let back = Config::create_from_str(&config.serialize().unwrap()).unwrap();
    let cs_back = back.get_color_space("test").unwrap();
    match cs_back.transform(ColorSpaceDirection::ToReference) {
        // Exposure contrast is dynamic when loaded back.
        Some(Transform::ExposureContrast(e)) => assert!(e.exposure_dynamic),
        t => panic!("unexpected {t:?}"),
    }
    match cs_back.transform(ColorSpaceDirection::FromReference) {
        // Grading primary is not dynamic when loaded back.
        Some(Transform::GradingPrimary(g)) => assert!(!g.dynamic),
        t => panic!("unexpected {t:?}"),
    }
}

const BUILTIN_TRANSFORMS_CONFIG: &str = r#"ocio_profile_version: 2

environment:
  {}
search_path: ""
strictparsing: true
luma: [0.2126, 0.7152, 0.0722]

roles:
  default: ref

file_rules:
  - !<Rule> {name: Default, colorspace: default}

displays:
  Disp1:
    - !<View> {name: View1, colorspace: test}

active_displays: []
active_views: []

colorspaces:
  - !<ColorSpace>
    name: ref
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    allocation: uniform

  - !<ColorSpace>
    name: test
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    allocation: uniform
    from_scene_reference: !<GroupTransform>
      children:
        - !<BuiltinTransform> {style: ACEScct_to_ACES2065-1}
        - !<BuiltinTransform> {style: ACEScct_to_ACES2065-1, direction: inverse}
"#;

#[test]
fn config_builtin_transforms_serialization() {
    let config = Config::create_from_str(BUILTIN_TRANSFORMS_CONFIG).unwrap();
    assert_eq!(config.num_color_spaces(), 2);
    assert_eq!(config.serialize().unwrap(), BUILTIN_TRANSFORMS_CONFIG);
}

#[test]
#[ignore = "needs-merge"]
fn config_builtin_transforms() {
    let config = Config::create_from_str(BUILTIN_TRANSFORMS_CONFIG).unwrap();
    config.validate().unwrap();
    config.get_processor("ref", "test").unwrap();
}

const CACHEID_CONFIG: &str = r#"ocio_profile_version: 2

search_path: luts

environment: {CS3: lut1d_green.ctf}

roles:
  default: cs1

displays:
  disp1:
    - !<View> {name: view1, colorspace: cs3}
    - !<View> {name: view2, colorspace: cs3, looks: look1}

looks:
  - !<Look>
    name: look1
    process_space: cs2
    transform: !<FileTransform> {src: $LOOK1}


colorspaces:
  - !<ColorSpace>
    name: cs1

  - !<ColorSpace>
    name: cs2
    from_scene_reference: !<MatrixTransform> {offset: [0.11, 0.12, 0.13, 0]}

  - !<ColorSpace>
    name: cs3
    from_scene_reference: !<FileTransform> {src: $CS3}
"#;

#[test]
fn config_config_context_cacheids() {
    cacheids(false);
}

#[test]
#[ignore = "needs-merge"]
fn config_config_context_cacheids_processors() {
    cacheids(true);
}

fn cacheids(with_ops: bool) {
    let _lock = env_lock();
    let _g1 = EnvGuard::set("CS3", None);
    let _g2 = EnvGuard::set("LOOK1", None);
    let config = Config::create_from_str(CACHEID_CONFIG).unwrap();
    let mut cfg = config.create_editable_copy();
    cfg.clear_search_paths();
    cfg.add_search_path(&data_file(""));

    let context_id = cfg.current_context().cache_id();
    let config_id = cfg.cache_id();

    let dv = |cfg: &Config, ctx: Option<&Context>, view: &str| {
        if with_ops {
            let ctx = ctx.cloned().unwrap_or_else(|| cfg.current_context().clone());
            cfg.get_display_view_processor_with_context(&ctx, "cs2", "disp1", view, TransformDirection::Forward)
                .unwrap();
        }
    };

    dv(&cfg, None, "view1");
    {
        let mut ctx = cfg.current_context().clone();
        ctx.set_string_var("CS3", Some("lut1d_green.ctf"));
        dv(&cfg, Some(&ctx), "view1");
        assert_eq!(context_id, ctx.cache_id());
        assert_eq!(config_id, cfg.cache_id_with_context(Some(&ctx)));
    }
    {
        cfg.add_environment_var("CS3", Some("lut1d_green.ctf"));
        dv(&cfg, None, "view1");
        assert_eq!(context_id, cfg.current_context().cache_id());
        assert_eq!(config_id, cfg.cache_id());
    }
    {
        let mut ctx = cfg.current_context().clone();
        ctx.set_string_var("CS3", Some("exposure_contrast_log.ctf"));
        dv(&cfg, Some(&ctx), "view1");
        assert_ne!(context_id, ctx.cache_id());
        assert_ne!(config_id, cfg.cache_id_with_context(Some(&ctx)));
        assert_eq!(config_id, cfg.cache_id());
    }
    {
        cfg.add_environment_var("CS3", Some("exposure_contrast_log.ctf"));
        dv(&cfg, None, "view1");
        assert_ne!(context_id, cfg.current_context().cache_id());
        assert_ne!(config_id, cfg.cache_id());
    }
    {
        cfg.add_environment_var("LOOK1", Some("lut1d_green.ctf"));
        dv(&cfg, None, "view2");
        assert_ne!(context_id, cfg.current_context().cache_id());
        assert_ne!(config_id, cfg.cache_id());
    }
    {
        cfg.add_environment_var("CS3", Some("lut1d_green.ctf"));
        dv(&cfg, None, "view2");
        assert_ne!(context_id, cfg.current_context().cache_id());
        assert_ne!(config_id, cfg.cache_id());
    }
    {
        cfg.add_environment_var("CS3", Some("lut1d_green.ctf"));
        cfg.add_environment_var("LOOK1", None);
        assert_eq!(context_id, cfg.current_context().cache_id());
        assert_eq!(config_id, cfg.cache_id());
    }
}

#[test]
#[ignore = "needs-merge"]
fn config_processor_cache_with_context_variables() {
    // Deviation: processors are values in Rust so the C++ pointer comparisons
    // are replaced by processor cache id comparisons.
    const CONFIG: &str = r#"ocio_profile_version: 2

environment: { VAR: cs1 }

search_path: ""
strictparsing: true
luma: [0.2126, 0.7152, 0.0722]

roles:
  default: ref

file_rules:
  - !<Rule> {name: Default, colorspace: default}

displays:
  Disp1:
    - !<View> {name: View1, colorspace: cs1}

colorspaces:
  - !<ColorSpace>
    name: ref

  - !<ColorSpace>
    name: cs1
    from_scene_reference: !<BuiltinTransform> {style: ACEScct_to_ACES2065-1}

  - !<ColorSpace>
    name: cs2
    from_scene_reference: !<ColorSpaceTransform> {src: ref, dst: cs1}

  - !<ColorSpace>
    name: cs3
    from_scene_reference: !<ColorSpaceTransform> {src: ref, dst: $VAR}
"#;
    let _lock = env_lock();
    let _g = EnvGuard::set("VAR", None);
    let config = Config::create_from_str(CONFIG).unwrap();
    config.validate().unwrap();
    let id = |c: &Config, a: &str, b: &str| c.get_processor(a, b).unwrap().cache_id();
    assert_eq!(id(&config, "ref", "cs1"), id(&config, "ref", "cs1"));
    assert_eq!(id(&config, "ref", "cs1"), id(&config, "ref", "cs2"));
    assert_eq!(id(&config, "ref", "cs2"), id(&config, "ref", "cs3"));

    let mut cfg = config.create_editable_copy();
    cfg.add_environment_var("VAR", Some("ref"));
    assert_eq!(id(&cfg, "ref", "cs1"), id(&cfg, "ref", "cs2"));
    assert_ne!(id(&cfg, "ref", "cs2"), id(&cfg, "ref", "cs3"));
}
