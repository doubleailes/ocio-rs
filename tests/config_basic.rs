//! Port of `Config_tests.cpp` (part 1: creation, roles, serialization of the
//! main sections, validation and context variables).

mod config_common;

use config_common::*;
use ocio::config::logging::LogGuard;
use ocio::config::{ColorSpace, ViewTransform, INTERNAL_RAW_PROFILE};
use ocio::*;

#[test]
fn config_internal_raw_profile() {
    Config::create_from_str(INTERNAL_RAW_PROFILE).unwrap();
}

#[test]
fn config_create_raw_config() {
    let config = Config::create_raw();
    config.validate().unwrap();
    assert_eq!(config.num_color_spaces(), 1);
    assert_eq!(config.color_space_name_by_index(0), "raw");

    let proc = config.get_processor("raw", "raw").unwrap();
    proc.default_cpu_processor();

    assert_err!(config.get_processor("not_found", "raw"), "Color space 'not_found' could not be found");
    assert_err!(config.get_processor("raw", "not_found"), "Color space 'not_found' could not be found");
}

#[test]
fn config_simple_config() {
    const SIMPLE_PROFILE: &str = r#"ocio_profile_version: 1
resource_path: luts
strictparsing: false
luma: [0.2126, 0.7152, 0.0722]
roles:
  default: raw
  scene_linear: lnh
displays:
  sRGB:
  - !<View> {name: Film1D, colorspace: loads_of_transforms}
  - !<View> {name: Ln, colorspace: lnh}
  - !<View> {name: Raw, colorspace: raw}
colorspaces:
  - !<ColorSpace>
      name: raw
      family: raw
      equalitygroup:
      bitdepth: 32f
      description: |
        A raw color space. Conversions to and from this space are no-ops.
      isdata: true
      allocation: uniform
  - !<ColorSpace>
      name: lnh
      family: ln
      equalitygroup:
      bitdepth: 16f
      description: |
        The show reference space. This is a sensor referred linear
        representation of the scene with primaries that correspond to
        scanned film. 0.18 in this space corresponds to a properly
        exposed 18% grey card.
      isdata: false
      allocation: lg2
  - !<ColorSpace>
      name: loads_of_transforms
      family: vd8
      equalitygroup:
      bitdepth: 8ui
      description: 'how many transforms can we use?'
      isdata: false
      allocation: uniform
      to_reference: !<GroupTransform>
        direction: forward
        children:
          - !<FileTransform>
            src: diffusemult.spimtx
            interpolation: unknown
          - !<ColorSpaceTransform>
            src: raw
            dst: lnh
          - !<ExponentTransform>
            value: [2.2, 2.2, 2.2, 1]
          - !<MatrixTransform>
            matrix: [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]
            offset: [0, 0, 0, 0]
          - !<CDLTransform>
            slope: [1, 1, 1]
            offset: [0, 0, 0]
            power: [1, 1, 1]
            saturation: 1

"#;
    let config = Config::create_from_str(SIMPLE_PROFILE).unwrap();
    config.validate().unwrap();
}

const DUP_PREFIX: &str = r#"ocio_profile_version: 2
search_path: luts
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
"#;

#[test]
fn config_colorspace_duplicate() {
    let cfg = format!("{DUP_PREFIX}    name: raw_duplicated\n    name: raw\n\n");
    assert_err!(
        Config::create_from_str(&cfg),
        "Key-value pair with key 'name' specified more than once. "
    );
}

#[test]
fn config_cdltransform_duplicate() {
    let cfg = format!(
        "{DUP_PREFIX}    name: raw\n    to_scene_reference: !<CDLTransform> {{slope: [1, 2, 1], slope: [1, 2, 1]}}\n\n"
    );
    assert_err!(
        Config::create_from_str(&cfg),
        "Key-value pair with key 'slope' specified more than once. "
    );
}

#[test]
fn config_searchpath_duplicate() {
    const CFG: &str = r#"ocio_profile_version: 2
search_path: luts
search_path: luts-dir
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

"#;
    assert_err!(
        Config::create_from_str(CFG),
        "Key-value pair with key 'search_path' specified more than once. "
    );
}

#[test]
fn config_roles() {
    const CFG: &str = r#"ocio_profile_version: 1
strictparsing: false
roles:
  compositing_log: lgh
  default: raw
  scene_linear: lnh
colorspaces:
  - !<ColorSpace>
      name: raw
  - !<ColorSpace>
      name: lnh
  - !<ColorSpace>
      name: lgh

"#;
    let config = Config::create_from_str(CFG).unwrap();
    assert_eq!(config.num_roles(), 3);
    assert!(config.has_role("compositing_log"));
    assert!(!config.has_role("cheese"));
    assert!(!config.has_role(""));

    assert_eq!(config.role_name(2), "scene_linear");
    assert_eq!(config.role_color_space_by_index(2), "lnh");
    assert_eq!(config.role_name(0), "compositing_log");
    assert_eq!(config.role_color_space_by_index(0), "lgh");
    assert_eq!(config.role_name(1), "default");
    assert_eq!(config.role_name(10), "");
    assert_eq!(config.role_color_space_by_index(10), "");

    assert_eq!(config.role_color_space("scene_linear"), "lnh");
    assert_eq!(config.role_color_space("compositing_log"), "lgh");
    assert_eq!(config.role_color_space("wrong_role"), "");
    assert_eq!(config.role_color_space(""), "");
}

#[test]
fn config_required_roles_for_version_2_2() {
    let mut config = Config::create();

    let mut cs = ColorSpace::new(ReferenceSpaceType::Scene);
    cs.set_name("default");
    config.add_color_space(&cs).unwrap();
    config.add_display_view("display", "view1", "default", "").unwrap();

    let mut scs = ColorSpace::new(ReferenceSpaceType::Scene);
    scs.set_name("scs");
    config.add_color_space(&scs).unwrap();
    let mut dcs = ColorSpace::new(ReferenceSpaceType::Display);
    dcs.set_name("dcs");
    config.add_color_space(&dcs).unwrap();

    let mut vt = ViewTransform::new(ReferenceSpaceType::Scene);
    vt.set_name("view_transform");
    vt.set_transform(Some(MatrixTransform::default().into()), ViewTransformDirection::FromReference);
    config.add_view_transform(&vt).unwrap();

    assert!(config.major_version() >= 2);
    assert!(config.minor_version() >= 2);

    {
        let g = LogGuard::new();
        config.validate().unwrap();
        assert!(mute_scene_linear_role_error(&g));
        assert!(mute_compositing_log_role_error(&g));
        assert!(mute_color_timing_role_error(&g));
        assert!(mute_aces_interchange_role_error(&g));
        assert!(mute_display_interchange_role_error(&g));
        assert!(g.is_empty(), "{}", g.output());
    }

    config.set_role(ROLE_SCENE_LINEAR, Some("scs")).unwrap();
    config.set_role(ROLE_COMPOSITING_LOG, Some("dcs")).unwrap();
    config.set_role(ROLE_COLOR_TIMING, Some("dcs")).unwrap();
    config.set_role(ROLE_INTERCHANGE_SCENE, Some("scs")).unwrap();
    config.set_role(ROLE_INTERCHANGE_DISPLAY, Some("dcs")).unwrap();
    {
        let g = LogGuard::new();
        config.validate().unwrap();
        assert!(g.is_empty(), "{}", g.output());
    }
    {
        config.set_role(ROLE_SCENE_LINEAR, None).unwrap();
        let g = LogGuard::new();
        config.validate().unwrap();
        assert!(g
            .output()
            .lines()
            .any(|l| l == "[OpenColorIO Error]: The scene_linear role is required for a config version 2.2 or higher."));
        config.set_role(ROLE_SCENE_LINEAR, Some("dcs")).unwrap();
    }
    {
        config.set_role(ROLE_COMPOSITING_LOG, None).unwrap();
        let g = LogGuard::new();
        config.validate().unwrap();
        assert!(mute_compositing_log_role_error(&g));
        config.set_role(ROLE_COMPOSITING_LOG, Some("dcs")).unwrap();
    }
    {
        config.set_role(ROLE_COLOR_TIMING, None).unwrap();
        let g = LogGuard::new();
        config.validate().unwrap();
        assert!(mute_color_timing_role_error(&g));
        config.set_role(ROLE_COLOR_TIMING, Some("dcs")).unwrap();
    }
    {
        config.set_role(ROLE_INTERCHANGE_SCENE, None).unwrap();
        let g = LogGuard::new();
        config.validate().unwrap();
        assert!(mute_aces_interchange_role_error(&g));
        config.set_role(ROLE_INTERCHANGE_SCENE, Some("scs")).unwrap();
    }
    {
        config.set_role(ROLE_INTERCHANGE_DISPLAY, None).unwrap();
        let g = LogGuard::new();
        config.validate().unwrap();
        assert!(mute_display_interchange_role_error(&g));
        config.set_role(ROLE_INTERCHANGE_DISPLAY, Some("dcs")).unwrap();
    }
    {
        config.set_role(ROLE_INTERCHANGE_SCENE, Some("dcs")).unwrap();
        config.set_role(ROLE_INTERCHANGE_DISPLAY, Some("dcs")).unwrap();
        let g = LogGuard::new();
        config.validate().unwrap();
        assert!(g
            .output()
            .starts_with("[OpenColorIO Error]: The aces_interchange role must be a scene-referred color space."));
    }
    {
        config.set_role(ROLE_INTERCHANGE_SCENE, Some("scs")).unwrap();
        config.set_role(ROLE_INTERCHANGE_DISPLAY, Some("scs")).unwrap();
        let g = LogGuard::new();
        config.validate().unwrap();
        assert!(g
            .output()
            .starts_with("[OpenColorIO Error]: The cie_xyz_d65_interchange role must be a display-referred color space."));
    }
    {
        config.set_major_version(2).unwrap();
        config.set_minor_version(1).unwrap();
        for r in [
            ROLE_SCENE_LINEAR,
            ROLE_COMPOSITING_LOG,
            ROLE_COLOR_TIMING,
            ROLE_INTERCHANGE_SCENE,
            ROLE_INTERCHANGE_DISPLAY,
        ] {
            config.set_role(r, None).unwrap();
        }
        let g = LogGuard::new();
        config.validate().unwrap();
        assert!(g.is_empty(), "{}", g.output());
    }
}

#[test]
fn config_serialize_group_transform() {
    let mut config = Config::create();
    {
        let mut cs = ColorSpace::default();
        cs.set_name("testing");
        cs.set_family("test");
        let mut group = GroupTransform::new();
        for interp in [
            None,
            Some(Interpolation::Unknown),
            Some(Interpolation::Best),
            Some(Interpolation::Nearest),
            Some(Interpolation::Cubic),
        ] {
            let mut ft = FileTransform::default();
            if let Some(i) = interp {
                ft.interpolation = i;
            }
            group.transforms.push(ft.into());
        }
        cs.set_transform(Some(group.into()), ColorSpaceDirection::FromReference);
        config.add_color_space(&cs).unwrap();
        config.set_role(ROLE_DEFAULT, Some("testing")).unwrap();
        config.set_role(ROLE_COMPOSITING_LOG, Some("testing")).unwrap();
    }
    {
        let mut cs = ColorSpace::default();
        cs.set_name("testing2");
        cs.set_family("test");
        let mut group = GroupTransform::new();
        group.transforms.push(ExponentTransform::default().into());
        cs.set_transform(Some(group.into()), ColorSpaceDirection::ToReference);
        config.add_color_space(&cs).unwrap();
        config.set_role(ROLE_COMPOSITING_LOG, Some("testing2")).unwrap();
    }
    config.set_version(2, 2).unwrap();

    let expected = r#"ocio_profile_version: 2.2

environment:
  {}
search_path: ""
strictparsing: true
luma: [0.2126, 0.7152, 0.0722]

roles:
  compositing_log: testing2
  default: testing

file_rules:
  - !<Rule> {name: Default, colorspace: default}

displays:
  {}

active_displays: []
active_views: []

colorspaces:
  - !<ColorSpace>
    name: testing
    family: test
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    allocation: uniform
    from_scene_reference: !<GroupTransform>
      children:
        - !<FileTransform> {src: ""}
        - !<FileTransform> {src: "", interpolation: unknown}
        - !<FileTransform> {src: "", interpolation: best}
        - !<FileTransform> {src: "", interpolation: nearest}
        - !<FileTransform> {src: "", interpolation: cubic}

  - !<ColorSpace>
    name: testing2
    family: test
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    allocation: uniform
    to_scene_reference: !<GroupTransform>
      children:
        - !<ExponentTransform> {value: 1}
"#;
    check_lines(&config.serialize().unwrap(), expected);
}

#[test]
fn config_serialize_searchpath() {
    {
        let mut config = Config::create();
        let mut cs = ColorSpace::default();
        cs.set_name("default");
        cs.set_is_data(true);
        config.add_color_space(&cs).unwrap();
        config.set_version(2, 2).unwrap();

        let expected = r#"ocio_profile_version: 2.2

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
  {}

active_displays: []
active_views: []

colorspaces:
  - !<ColorSpace>
    name: default
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    isdata: true
    allocation: uniform
"#;
        check_lines(&config.serialize().unwrap(), expected);
    }
    {
        let mut config = Config::create();
        config.set_major_version(1).unwrap();
        config.set_minor_version(0).unwrap();
        config.set_search_path("a:b:c");

        let s = config.serialize().unwrap();
        let lines: Vec<&str> = s.lines().collect();
        // V1 saves search_path as a single string.
        assert_eq!(lines[2], "search_path: a:b:c");

        // V2 saves search_path as separate strings.
        config.set_major_version(2).unwrap();
        let s = config.serialize().unwrap();
        let lines: Vec<&str> = s.lines().collect();
        assert_eq!(&lines[4..8], ["search_path:", "  - a", "  - b", "  - c"]);

        let read = Config::create_from_str(&s).unwrap();
        assert_eq!(read.num_search_paths(), 3);
        assert_eq!(read.search_path(), "a:b:c");
        assert_eq!(read.search_path_by_index(0), "a");
        assert_eq!(read.search_path_by_index(1), "b");
        assert_eq!(read.search_path_by_index(2), "c");

        config.clear_search_paths();
        let sp = [
            "a path with a - in it/",
            "/absolute/linux/path",
            "C:\\absolute\\windows\\path",
            "!<path> using /yaml/symbols",
        ];
        for p in sp {
            config.add_search_path(p);
        }
        let s = config.serialize().unwrap();
        let lines: Vec<&str> = s.lines().collect();
        assert_eq!(
            &lines[4..9],
            [
                "search_path:",
                "  - a path with a - in it/",
                "  - /absolute/linux/path",
                "  - C:\\absolute\\windows\\path",
                "  - \"!<path> using /yaml/symbols\""
            ]
        );
        let read = Config::create_from_str(&s).unwrap();
        assert_eq!(read.num_search_paths(), 4);
        for (i, p) in sp.iter().enumerate() {
            assert_eq!(read.search_path_by_index(i), *p);
        }
    }
}

#[test]
fn config_serialize_environment() {
    {
        let mut config = Config::create();
        config.set_major_version(1).unwrap();
        config.set_minor_version(0).unwrap();
        let s = config.serialize().unwrap();
        let lines: Vec<&str> = s.lines().collect();
        assert_eq!(lines[2], "search_path: \"\"");
    }
    {
        let mut config = Config::create();
        config.set_major_version(2).unwrap();
        config.set_minor_version(0).unwrap();
        let s = config.serialize().unwrap();
        let lines: Vec<&str> = s.lines().collect();
        assert_eq!(lines[2], "environment:");
        assert_eq!(lines[3], "  {}");
    }
    {
        let mut config = Config::create();
        config.set_major_version(1).unwrap();
        config.set_minor_version(0).unwrap();
        config.add_environment_var("SHOT", Some("0001"));
        let s = config.serialize().unwrap();
        let lines: Vec<&str> = s.lines().collect();
        assert_eq!(lines[2], "environment:");
        assert_eq!(lines[3], "  SHOT: 0001");
    }
}

#[test]
fn config_validation() {
    const DUP: &str = r#"ocio_profile_version: 1
colorspaces:
  - !<ColorSpace>
      name: raw
  - !<ColorSpace>
      name: raw
strictparsing: false
roles:
  default: raw
displays:
  sRGB:
  - !<View> {name: Raw, colorspace: raw}

"#;
    assert_err!(Config::create_from_str(DUP), "Colorspace with name 'raw' already defined");

    const OK: &str = r#"ocio_profile_version: 1
colorspaces:
  - !<ColorSpace>
      name: raw
strictparsing: false
roles:
  default: raw
displays:
  sRGB:
  - !<View> {name: Raw, colorspace: raw}

"#;
    let config = Config::create_from_str(OK).unwrap();
    config.validate().unwrap();
}

#[test]
fn config_context_variable_v1() {
    const PROFILE: &str = r#"ocio_profile_version: 1
environment:
  SHOW: super
  SHOT: test
  SEQ: foo
  test: bar${cheese}
  cheese: chedder
colorspaces:
  - !<ColorSpace>
      name: raw
strictparsing: false
roles:
  default: raw
displays:
  sRGB:
  - !<View> {name: Raw, colorspace: raw}

"#;
    const PROFILE2: &str = r#"ocio_profile_version: 1
colorspaces:
  - !<ColorSpace>
      name: raw
strictparsing: false
roles:
  default: raw
displays:
  sRGB:
  - !<View> {name: Raw, colorspace: raw}

"#;
    let _lock = env_lock();
    let _g1 = EnvGuard::set("SHOW", Some("bar"));
    let _g2 = EnvGuard::set("TASK", Some("lighting"));

    let config = Config::create_from_str(PROFILE).unwrap();
    config.validate().unwrap();
    assert_eq!(config.num_environment_vars(), 5);

    let mut used = Context::new();
    assert_eq!(config.current_context().resolve_string_var_with_used("test${test}", &mut used), "testbarchedder");
    assert_eq!(used.num_string_vars(), 2);
    assert_eq!(used.string_var_name_by_index(0), Some("cheese"));
    assert_eq!(used.string_var_by_index(0), Some("chedder"));
    assert_eq!(used.string_var_name_by_index(1), Some("test"));
    assert_eq!(used.string_var_by_index(1), Some("bar${cheese}"));

    used.clear_string_vars();
    assert_eq!(config.current_context().resolve_string_var_with_used("${SHOW}", &mut used), "bar");
    assert_eq!(used.num_string_vars(), 1);
    assert_eq!(used.string_var_name_by_index(0), Some("SHOW"));
    assert_eq!(used.string_var_by_index(0), Some("bar"));
    assert_eq!(config.environment_var_default("SHOW"), "super");

    let mut edit = config.create_editable_copy();
    assert_eq!(edit.num_environment_vars(), 5);
    edit.clear_environment_vars();
    assert_eq!(edit.num_environment_vars(), 0);
    edit.add_environment_var("testing", Some("dupvar"));
    assert_eq!(edit.num_environment_vars(), 1);
    edit.add_environment_var("testing", Some("dupvar"));
    assert_eq!(edit.num_environment_vars(), 1);
    edit.add_environment_var("foobar", Some("testing"));
    assert_eq!(edit.num_environment_vars(), 2);
    edit.add_environment_var("blank", Some(""));
    assert_eq!(edit.num_environment_vars(), 3);
    edit.add_environment_var("dontadd", None);
    assert_eq!(edit.num_environment_vars(), 3);
    edit.add_environment_var("foobar", None);
    assert_eq!(edit.num_environment_vars(), 2);
    edit.clear_environment_vars();
    assert_eq!(edit.num_environment_vars(), 0);

    assert_eq!(edit.environment_mode(), EnvironmentMode::LoadPredefined);
    edit.set_environment_mode(EnvironmentMode::LoadAll);
    assert_eq!(edit.environment_mode(), EnvironmentMode::LoadAll);

    let log = LogGuard::new();
    let noenv = Config::create_from_str(PROFILE2).unwrap();
    noenv.validate().unwrap();
    assert_eq!(noenv.environment_mode(), EnvironmentMode::LoadAll);
    assert_eq!(noenv.current_context().resolve_string_var("${TASK}"), "lighting");
    assert_eq!(
        log.output(),
        "[OpenColorIO Debug]: This .ocio config has no environment section defined. The default behaviour is to load all environment variables (0), which reduces the efficiency of OCIO's caching. Consider predefining the environment variables used.\n"
    );
}

const FAULTY_CONTEXT_CONFIG: &str = r#"ocio_profile_version: 2

search_path: luts

environment:
  DST1: cs2
  DST2: cs2
  DST3: cs2

roles:
  default: cs1

view_transforms:
  - !<ViewTransform>
    name: vt1
    from_scene_reference: !<ColorSpaceTransform> {src: cs1, dst: $DST3}

displays:
  disp1:
    - !<View> {name: view1, view_transform: vt1, display_colorspace: dcs1}
    - !<View> {name: view2, colorspace: cs3, looks: look1}

looks:
  - !<Look>
    name: look1
    process_space: cs2
    transform: !<ColorSpaceTransform> {src: cs1, dst: $DST1}

colorspaces:
  - !<ColorSpace>
    name: cs1

  - !<ColorSpace>
    name: cs2
    from_scene_reference: !<MatrixTransform> {offset: [0.11, 0.12, 0.13, 0]}

  - !<ColorSpace>
    name: cs3
    from_scene_reference: !<ColorSpaceTransform> {src: cs1, dst: $DST2}

display_colorspaces:
  - !<ColorSpace>
    name: dcs1
    allocation: uniform
    from_display_reference: !<CDLTransform> {slope: [1, 2, 1]}
"#;

#[test]
fn config_context_variable_faulty_cases() {
    faulty_cases(false);
}

#[test]
#[ignore = "needs-merge"]
fn config_context_variable_faulty_cases_processor() {
    let cfg = Config::create_from_str(FAULTY_CONTEXT_CONFIG).unwrap();
    cfg.validate().unwrap();
    cfg.get_display_view_processor_dir("cs1", "disp1", "view1", TransformDirection::Forward)
        .unwrap();
    faulty_cases(true);
}

/// `with_ops`: also check the errors that happen after some ops are built.
fn faulty_cases(with_ops: bool) {
    let _lock = env_lock();
    let mut cfg = Config::create_from_str(FAULTY_CONTEXT_CONFIG).unwrap().create_editable_copy();
    cfg.validate().unwrap();
    let dv = |cfg: &Config, view: &str| {
        cfg.get_display_view_processor_dir("cs1", "disp1", view, TransformDirection::Forward)
    };
    {
        cfg.add_environment_var("DST3", None);
        assert_eq!(cfg.num_environment_vars(), 2);
        assert_err!(
            cfg.validate(),
            "references a color space '$DST3' using an unknown context variable"
        );
        assert_err!(dv(&cfg, "view1"), "Color space '$DST3' could not be found");
    }
    {
        cfg.add_environment_var("DST2", None);
        assert_eq!(cfg.num_environment_vars(), 1);
        assert_err!(
            cfg.validate(),
            "references a color space '$DST2' using an unknown context variable"
        );
        if with_ops {
            assert_err!(dv(&cfg, "view2"), "Color space '$DST2' could not be found");
        }
    }
    {
        cfg.add_environment_var("DST2", Some("cs1"));
        cfg.add_environment_var("DST1", None);
        assert_eq!(cfg.num_environment_vars(), 1);
        assert_err!(
            cfg.validate(),
            "references a color space '$DST1' using an unknown context variable"
        );
        if with_ops {
            assert_err!(dv(&cfg, "view2"), "Color space '$DST1' could not be found");
        }
    }
}

#[test]
fn config_context_variable() {
    const CONFIG: &str = r#"ocio_profile_version: 2

environment:
  VAR1: $VAR1
  VAR2: var2
  VAR3: env3
search_path: luts
strictparsing: true
luma: [0.2126, 0.7152, 0.0722]

roles:
  default: cs1

file_rules:
  - !<Rule> {name: Default, colorspace: default}

displays:
  disp1:
    - !<View> {name: view1, colorspace: cs1}

active_displays: []
active_views: []

colorspaces:
  - !<ColorSpace>
    name: cs1
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    allocation: uniform
"#;
    let _lock = env_lock();
    let _g1 = EnvGuard::set("VAR1", Some("env1"));
    let g2 = EnvGuard::set("VAR2", Some("env2"));
    let _g3 = EnvGuard::set("VAR3", None);

    let config = Config::create_from_str(CONFIG).unwrap();
    config.validate().unwrap();
    assert_eq!(config.environment_mode(), EnvironmentMode::LoadPredefined);
    let ctx = config.current_context();
    assert_eq!(ctx.resolve_string_var("$VAR1"), "env1");
    assert_eq!(ctx.resolve_string_var("$VAR2"), "env2");
    assert_eq!(ctx.resolve_string_var("$VAR3"), "env3");
    assert_eq!(config.serialize().unwrap(), CONFIG);

    // VAR2 reverts to its default value.
    drop(g2);
    let _g2 = EnvGuard::set("VAR2", None);
    let config = Config::create_from_str(CONFIG).unwrap();
    config.validate().unwrap();
    let ctx = config.current_context();
    assert_eq!(ctx.resolve_string_var("$VAR1"), "env1");
    assert_eq!(ctx.resolve_string_var("$VAR2"), "var2");
    assert_eq!(ctx.resolve_string_var("$VAR3"), "env3");

    // System env. variable VAR1 is now missing.
    let _g1b = EnvGuard::set("VAR1", None);
    let config = Config::create_from_str(CONFIG).unwrap();
    config.validate().unwrap();
    let ctx = config.current_context();
    assert_eq!(ctx.resolve_string_var("$VAR1"), "$VAR1");
    assert_eq!(ctx.resolve_string_var("$VAR2"), "var2");
    assert_eq!(ctx.resolve_string_var("$VAR3"), "env3");
}

#[test]
fn config_context_variable_unresolved() {
    const BODY: &str = r#"
search_path: luts

roles:
  default: cs1
  reference: cs1

displays:
  disp1:
    - !<View> {name: view1, colorspace: cs2}

colorspaces:
  - !<ColorSpace>
    name: cs1

  - !<ColorSpace>
    name: cs2
    from_reference: !<MatrixTransform> {offset: [0.1, 0.2, 0.3, 0.0]}
"#;
    let _lock = env_lock();
    let _g1 = EnvGuard::set("ENV1", None);
    let _g2 = EnvGuard::set("ENV2", None);
    let make = |env: &str| format!("ocio_profile_version: 2\n{env}{BODY}");

    for env in ["environment: {ENV1: $ENV1}\n", "environment:\n  ENV1: ${ENV1}\n"] {
        let config = Config::create_from_str(&make(env)).unwrap();
        config.validate().unwrap();
    }
    let cases = [
        ("environment: {ENV1: $ENV2}\n", "'ENV1 = $ENV2'"),
        ("environment: {ENV1: env, ENV2: $ENV1}\n", "'ENV2 = $ENV1'"),
        ("environment: {ENV1: env$ENV1}\n", "'ENV1 = env$ENV1'"),
        ("environment:\n ENV1: env${ENV2}\n", "'ENV1 = env${ENV2}'"),
        ("environment: {ENV1: $ENV1$ENV2}\n", "'ENV1 = $ENV1$ENV2'"),
    ];
    for (env, what) in cases {
        let config = Config::create_from_str(&make(env)).unwrap();
        assert_err!(
            config.validate(),
            &format!("Unresolved context variable in environment declaration {what}.")
        );
    }
}
