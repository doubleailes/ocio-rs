//! Port of `Config_tests.cpp` (part 2: context variables, search paths and
//! versions).

mod config_common;

use config_common::*;
use ocio::config::ColorSpace;
use ocio::*;

const SANITY_CONFIG: &str = r#"ocio_profile_version: 2

search_path: luts

environment: {CS2: lut1d_green.ctf}

roles:
  default: cs1

displays:
  disp1:
    - !<View> {name: view1, colorspace: cs2}


colorspaces:
  - !<ColorSpace>
    name: cs1

  - !<ColorSpace>
    name: cs2
    from_scene_reference: !<FileTransform> {src: $CS2}
"#;

fn dv(cfg: &Config) -> Result<Processor> {
    cfg.get_display_view_processor_dir("cs1", "disp1", "view1", TransformDirection::Forward)
}

#[test]
fn config_context_variable_with_sanity_check() {
    sanity_check(false);
}

#[test]
#[ignore = "needs-merge"]
fn config_context_variable_with_sanity_check_processor() {
    sanity_check(true);
}

fn sanity_check(with_ops: bool) {
    let _lock = env_lock();
    let _g = EnvGuard::set("CS2", None);
    let config = Config::create_from_str(SANITY_CONFIG).unwrap();
    config.validate().unwrap();

    let mut cfg = config.create_editable_copy();
    cfg.clear_search_paths();
    cfg.add_search_path(&data_file(""));
    if with_ops {
        dv(&cfg).unwrap();
    }

    assert_eq!(cfg.num_environment_vars(), 1);
    assert_eq!(cfg.current_context().num_string_vars(), 1);
    assert_eq!(
        cfg.current_context().environment_mode(),
        EnvironmentMode::LoadPredefined
    );

    cfg.add_environment_var("CS2", Some("lut1d_green.ctf"));
    assert_eq!(cfg.num_environment_vars(), 1);
    cfg.validate().unwrap();

    cfg.add_environment_var("CS2", Some("exposure_contrast_log.ctf"));
    assert_eq!(cfg.num_environment_vars(), 1);
    cfg.validate().unwrap();

    // $TOTO is added but not used.
    cfg.add_environment_var("TOTO", Some("exposure_contrast_log.ctf"));
    assert_eq!(cfg.num_environment_vars(), 2);
    cfg.validate().unwrap();

    cfg.add_environment_var("CS2", Some("$TOTO"));
    assert_eq!(cfg.num_environment_vars(), 2);
    assert_err!(
        cfg.validate(),
        "Unresolved context variable in environment declaration 'CS2 = $TOTO'."
    );

    cfg.add_environment_var("TOTO", None);
    assert_eq!(cfg.num_environment_vars(), 1);
    assert_err!(
        cfg.validate(),
        "Unresolved context variable in environment declaration 'CS2 = $TOTO'."
    );
    if with_ops {
        assert_err!(
            dv(&cfg),
            "The specified file reference '$CS2' could not be located"
        );
    }

    cfg.add_environment_var("CS2", None);
    assert_eq!(cfg.num_environment_vars(), 0);
    assert_err!(
        cfg.validate(),
        "The file transform source cannot be resolved: '$CS2'."
    );
    if with_ops {
        assert_err!(
            dv(&cfg),
            "The specified file reference '$CS2' could not be located"
        );
    }

    cfg.add_environment_var("CS2", Some("lut1d_green.ctf"));
    cfg.clear_search_paths();
    assert_err!(
        cfg.validate(),
        "The search_path must not be empty if there are FileTransforms."
    );

    cfg.clear_search_paths();
    cfg.set_search_path("");
    // Note: the Rust context skips empty search path elements, so an empty
    // search path is the same as no search path.
    assert_err!(cfg.validate(), "The search_path must not be");

    cfg.clear_search_paths();
    cfg.set_search_path("$MYPATH");
    assert_err!(
        cfg.validate(),
        "The search_path '$MYPATH' cannot be resolved."
    );

    cfg.clear_search_paths();
    cfg.set_search_path("");
    cfg.add_display_view("disp1", "view1", "cs1", "").unwrap();
    cfg.remove_color_space("cs2");
    cfg.validate().unwrap();
}

#[test]
fn config_colorspacename_with_reserved_token() {
    let mut cfg = Config::create_raw().create_editable_copy();
    let mut cs = ColorSpace::default();
    cs.set_name("cs1$VAR");
    assert_err!(
        cfg.add_color_space(&cs),
        "A color space name 'cs1$VAR' cannot contain a context variable reserved token i.e. % or $."
    );
}

const CSNAME_CONFIG: &str = r#"ocio_profile_version: 2

environment: {ENV1: file.clf}

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
"#;

#[test]
fn config_context_variable_with_colorspacename() {
    let _lock = env_lock();
    let _g = EnvGuard::set("VAR3", None);
    {
        let s =
            format!("{CSNAME_CONFIG}    from_scene_reference: !<FileTransform> {{src: $VAR3}}\n");
        let mut cfg = Config::create_from_str(&s).unwrap().create_editable_copy();
        assert_err!(
            cfg.validate(),
            "The file transform source cannot be resolved: '$VAR3'."
        );
        cfg.add_environment_var("VAR3", Some("file.clf"));
        cfg.validate().unwrap();
    }
    {
        let s = format!(
            "{CSNAME_CONFIG}    from_scene_reference: !<ColorSpaceTransform> {{src: $VAR3, dst: cs1}}\n"
        );
        let mut cfg = Config::create_from_str(&s).unwrap().create_editable_copy();
        assert_err!(
            cfg.validate(),
            "This config references a color space '$VAR3' using an unknown context variable."
        );
        cfg.add_environment_var("VAR3", Some("cs1"));
        cfg.validate().unwrap();
        cfg.add_environment_var("VAR3", Some("reference"));
        cfg.validate().unwrap();
        cfg.add_environment_var("VAR3", Some("cs1234"));
        assert_err!(
            cfg.validate(),
            "This config references a color space, 'cs1234', which is not defined."
        );
        cfg.add_environment_var("VAR3", Some("reference1234"));
        assert_err!(
            cfg.validate(),
            "This config references a color space, 'reference1234', which is not defined."
        );
        cfg.add_environment_var("VAR3", None);
        assert_err!(
            cfg.validate(),
            "This config references a color space '$VAR3' using an unknown context variable."
        );
    }
    {
        let s = format!(
            "{CSNAME_CONFIG}    from_scene_reference: !<ColorSpaceTransform> {{src: $VAR3, dst: cs1}}\n"
        );
        let cfg = Config::create_from_str(&s).unwrap().create_editable_copy();
        assert_err!(
            cfg.get_processor("cs1", "cs2"),
            "Color space '$VAR3' could not be found."
        );

        let mut ctx = cfg.current_context().clone();
        assert_err!(
            cfg.get_processor_with_context_names(&ctx, "cs1", "cs2"),
            "Color space '$VAR3' could not be found."
        );
        ctx.set_string_var("VAR3", Some("cs1"));
        cfg.get_processor_with_context_names(&ctx, "cs1", "cs2")
            .unwrap();
        ctx.set_string_var("VAR3", Some("reference"));
        cfg.get_processor_with_context_names(&ctx, "cs1", "cs2")
            .unwrap();
        ctx.set_string_var("VAR3", Some(""));
        assert_err!(
            cfg.get_processor_with_context_names(&ctx, "cs1", "cs2"),
            "Color space '$VAR3' could not be found."
        );
    }
}

#[test]
#[ignore = "needs-merge"]
fn config_context_variable_with_colorspacename_named_transform() {
    let _lock = env_lock();
    let _g = EnvGuard::set("VAR3", None);
    let s = format!(
        "{CSNAME_CONFIG}    from_scene_reference: !<ColorSpaceTransform> {{src: $VAR3, dst: cs1}}\n\
named_transforms:\n  - !<NamedTransform>\n    name: nt1\n    transform: !<RangeTransform> {{min_in_value: 0, min_out_value: 0}}\n"
    );
    let mut cfg = Config::create_from_str(&s).unwrap().create_editable_copy();
    cfg.add_environment_var("VAR3", Some("nt1"));
    cfg.validate().unwrap();
    let ctx = cfg.current_context().clone();
    cfg.get_processor_with_context_names(&ctx, "cs1", "cs2")
        .unwrap();
}

#[test]
fn config_context_variable_with_colorspacename_named_transform_validate() {
    let _lock = env_lock();
    let _g = EnvGuard::set("VAR3", None);
    let s = format!(
        "{CSNAME_CONFIG}    from_scene_reference: !<ColorSpaceTransform> {{src: $VAR3, dst: cs1}}\n\
named_transforms:\n  - !<NamedTransform>\n    name: nt1\n    transform: !<RangeTransform> {{min_in_value: 0, min_out_value: 0}}\n"
    );
    let mut cfg = Config::create_from_str(&s).unwrap().create_editable_copy();
    cfg.add_environment_var("VAR3", Some("nt1"));
    cfg.validate().unwrap();
}

#[test]
fn config_context_variable_with_role() {
    const CONFIG: &str = r#"ocio_profile_version: 2

environment: {ENV1: cs1}

search_path: luts

roles:
  default: cs1
  reference: $ENV1

displays:
  disp1:
    - !<View> {name: view1, colorspace: cs2}

colorspaces:
  - !<ColorSpace>
    name: cs1

  - !<ColorSpace>
    name: cs2
    from_scene_reference: !<CDLTransform> {offset: [0.1, 0.1, 0.1]}

  - !<ColorSpace>
    name: cs3
    from_scene_reference: !<ColorSpaceTransform> {src: reference, dst: cs2}
"#;
    let _lock = env_lock();
    let _g = EnvGuard::set("ENV1", None);
    let cfg = Config::create_from_str(CONFIG)
        .unwrap()
        .create_editable_copy();
    cfg.set_processor_cache_flags(ProcessorCacheFlags::OFF);
    assert_err!(
        cfg.validate(),
        "The role 'reference' refers to a color space, '$ENV1', which is not defined."
    );
    assert_err!(
        cfg.get_processor("cs1", "cs3"),
        "Color space 'reference' could not be found."
    );
}

#[test]
fn config_context_variable_with_display_view() {
    const CONFIG: &str = r#"ocio_profile_version: 2

environment: {ENV1: cs2}

search_path: luts

roles:
  default: cs1
  reference: cs1

displays:
  disp1:
    - !<View> {name: view1, colorspace: $ENV1}

colorspaces:
  - !<ColorSpace>
    name: cs1

  - !<ColorSpace>
    name: cs2
    from_scene_reference: !<CDLTransform> {offset: [0.1, 0.1, 0.1]}
"#;
    let _lock = env_lock();
    let _g = EnvGuard::set("ENV1", None);
    let config = Config::create_from_str(CONFIG).unwrap();
    assert_err!(
        config.validate(),
        "Display 'disp1' has a view 'view1' that refers to a color space or a named transform, '$ENV1', which is not defined."
    );
    assert_err!(
        dv(&config),
        "DisplayViewTransform error. Cannot find color space or named transform with name '$ENV1'."
    );
}

const SEARCH_PATH_V1_CONFIG: &str = r#"ocio_profile_version: 1

search_path: $ENV1

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
    from_reference: !<FileTransform> {src: lut1d_green.ctf}
"#;

#[test]
fn config_context_variable_with_search_path_v1_validation() {
    let _lock = env_lock();
    let _g = EnvGuard::set("ENV1", None);
    let mut cfg = Config::create_from_str(SEARCH_PATH_V1_CONFIG)
        .unwrap()
        .create_editable_copy();
    assert_err!(
        cfg.validate(),
        "The search_path '$ENV1' cannot be resolved."
    );

    cfg.add_environment_var("ENV1", Some(&data_file("")));
    cfg.validate().unwrap();

    cfg.add_environment_var("ENV1", Some("faulty/path"));
    cfg.validate().unwrap();
    cfg.add_environment_var("ENV1", None);

    cfg.set_search_path(&format!("{}:$ENV1", data_file("")));
    cfg.validate().unwrap();
    cfg.set_search_path(&format!("$ENV1:{}", data_file("")));
    cfg.validate().unwrap();
}

#[test]
#[ignore = "needs-merge"]
fn config_context_variable_with_search_path_v1() {
    let _lock = env_lock();
    let _g = EnvGuard::set("ENV1", None);
    let mut cfg = Config::create_from_str(SEARCH_PATH_V1_CONFIG)
        .unwrap()
        .create_editable_copy();
    assert_err!(
        cfg.get_processor("cs1", "cs2"),
        "The specified file reference 'lut1d_green.ctf' could not be located. The following attempts were made: '$ENV1/lut1d_green.ctf'."
    );
    cfg.add_environment_var("ENV1", Some(&data_file("")));
    cfg.get_processor("cs1", "cs2").unwrap();
    cfg.add_environment_var("ENV1", Some("faulty/path"));
    assert_err!(
        cfg.get_processor("cs1", "cs2"),
        "The specified file reference 'lut1d_green.ctf' could not be located. The following attempts were made: 'faulty/path/lut1d_green.ctf'."
    );
    cfg.add_environment_var("ENV1", None);
    cfg.set_search_path(&format!("{}:$ENV1", data_file("")));
    cfg.get_processor("cs1", "cs2").unwrap();
    cfg.set_search_path(&format!("$ENV1:{}", data_file("")));
    cfg.get_processor("cs1", "cs2").unwrap();
}

fn search_path_v2_config() -> String {
    format!(
        r#"ocio_profile_version: 2

environment: {{ENV1: {}}}

search_path: $ENV1

roles:
  default: cs1
  reference: cs1

displays:
  disp1:
    - !<View> {{name: view1, colorspace: cs2}}

colorspaces:
  - !<ColorSpace>
    name: cs1

  - !<ColorSpace>
    name: cs2
    from_scene_reference: !<FileTransform> {{src: lut1d_green.ctf}}
"#,
        data_file("")
    )
}

#[test]
fn config_context_variable_with_search_path_v2_validation() {
    let _lock = env_lock();
    let _g = EnvGuard::set("ENV1", None);
    let mut cfg = Config::create_from_str(&search_path_v2_config())
        .unwrap()
        .create_editable_copy();
    cfg.validate().unwrap();

    cfg.add_environment_var("ENV1", None);
    assert_err!(
        cfg.validate(),
        "The search_path '$ENV1' cannot be resolved."
    );

    let dir = data_file("");
    for sp in [
        format!("{dir}:$ENV1"),
        format!("$ENV1:{dir}"),
        format!("{dir}:"),
        format!(":{dir}"),
    ] {
        cfg.set_search_path(&sp);
        cfg.validate().unwrap();
    }
    cfg.set_search_path("$ENV1:faulty/path");
    cfg.validate().unwrap();
}

#[test]
#[ignore = "needs-merge"]
fn config_context_variable_with_search_path_v2() {
    let _lock = env_lock();
    let _g = EnvGuard::set("ENV1", None);
    let mut cfg = Config::create_from_str(&search_path_v2_config())
        .unwrap()
        .create_editable_copy();
    cfg.get_processor("cs1", "cs2").unwrap();
    cfg.add_environment_var("ENV1", None);
    assert_err!(
        cfg.get_processor("cs1", "cs2"),
        "The specified file reference 'lut1d_green.ctf' could not be located. The following attempts were made: '$ENV1/lut1d_green.ctf'."
    );
    let dir = data_file("");
    for sp in [
        format!("{dir}:$ENV1"),
        format!("$ENV1:{dir}"),
        format!("{dir}:"),
        format!(":{dir}"),
    ] {
        cfg.set_search_path(&sp);
        cfg.get_processor("cs1", "cs2").unwrap();
    }
    cfg.set_search_path("$ENV1:faulty/path");
    assert_err!(
        cfg.get_processor("cs1", "cs2"),
        "The specified file reference 'lut1d_green.ctf' could not be located. The following attempts were made: '$ENV1/lut1d_green.ctf' : 'faulty/path/lut1d_green.ctf'."
    );
}

const ENV_CS_CONFIG: &str = r#"ocio_profile_version: 1

search_path: luts
strictparsing: true
luma: [0.2126, 0.7152, 0.0722]

roles:
  compositing_log: lgh
  default: raw
  scene_linear: lnh

displays:
  sRGB:
    - !<View> {name: Raw, colorspace: raw}

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
    name: lnh
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    allocation: uniform

  - !<ColorSpace>
    name: lgh
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    allocation: uniform
    allocationvars: [-0.125, 1.125]
"#;

#[test]
fn config_env_colorspace_name() {
    let _lock = env_lock();
    let _g = EnvGuard::set("OCIO_TEST", None);
    {
        let s = format!("{ENV_CS_CONFIG}    from_reference: !<ColorSpaceTransform> {{src: raw, dst: $MISSING_ENV}}\n");
        let config = Config::create_from_str(&s).unwrap();
        assert_err!(
            config.validate(),
            "This config references a color space '$MISSING_ENV' using an unknown context variable"
        );
        assert_err!(
            config.get_processor("raw", "lgh"),
            "Color space '$MISSING_ENV' could not be found"
        );
    }
    {
        let _g = EnvGuard::set("OCIO_TEST", Some("FaultyColorSpaceName"));
        let s = format!("{ENV_CS_CONFIG}    from_reference: !<ColorSpaceTransform> {{src: raw, dst: $OCIO_TEST}}\n");
        let config = Config::create_from_str(&s).unwrap();
        assert_err!(
            config.validate(),
            "color space, 'FaultyColorSpaceName', which is not defined"
        );
        assert_err!(
            config.get_processor("raw", "lgh"),
            "Color space '$OCIO_TEST' could not be found"
        );
    }
    {
        let _g = EnvGuard::set("OCIO_TEST", Some("lnh"));
        let s = format!("{ENV_CS_CONFIG}    from_reference: !<ColorSpaceTransform> {{src: raw, dst: $OCIO_TEST}}\n");
        let config = Config::create_from_str(&s).unwrap();
        config.validate().unwrap();
        config.get_processor("raw", "lgh").unwrap();
        assert_eq!(config.serialize().unwrap(), s);
    }
}

#[test]
fn config_version() {
    const PROFILE: &str = r#"ocio_profile_version: 2
environment:
  {}
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
    let mut config = Config::create_from_str(PROFILE)
        .unwrap()
        .create_editable_copy();
    config.validate().unwrap();

    config.set_major_version(1).unwrap();
    assert_err!(
        config.set_major_version(20000),
        "version is 20000 where supported versions start at 1 and end at 2"
    );
    assert_err!(
        config.set_minor_version(1),
        "The minor version 1 is not supported for major version 1. Maximum minor version is 0"
    );
    config.set_minor_version(0).unwrap();
    assert!(config
        .serialize()
        .unwrap()
        .to_lowercase()
        .starts_with("ocio_profile_version: 1"));

    config.set_major_version(2).unwrap();
    assert!(config
        .serialize()
        .unwrap()
        .to_lowercase()
        .starts_with("ocio_profile_version: 2"));

    assert_err!(
        config.set_version(2, 9),
        "The minor version 9 is not supported for major version 2. Maximum minor version is 6"
    );
    config.set_major_version(2).unwrap();
    assert_err!(
        config.set_minor_version(9),
        "The minor version 9 is not supported for major version 2. Maximum minor version is 6"
    );
    assert_err!(
        config.set_version(3, 4),
        "version is 3 where supported versions start at 1 and end at 2"
    );
}

#[test]
fn config_version_validation() {
    const END: &str = r#"colorspaces:
  - !<ColorSpace>
      name: raw
strictparsing: false
roles:
  default: raw
displays:
  sRGB:
  - !<View> {name: Raw, colorspace: raw}

"#;
    let with = |v: &str| format!("ocio_profile_version: {v}\n{END}");
    assert_err!(
        Config::create_from_str(&with("2.0.1")),
        "does not appear to have a valid version 2.0.1"
    );
    assert_err!(
        Config::create_from_str(&with("2.9")),
        "The minor version 9 is not supported for major version 2"
    );
    assert_err!(
        Config::create_from_str(&with("3")),
        "The version is 3 where supported versions start at 1 and end at 2"
    );
    assert_err!(
        Config::create_from_str(&with("3.0")),
        "The version is 3 where supported versions start at 1 and end at 2"
    );
    let config = Config::create_from_str(&with("1.0")).unwrap();
    assert_eq!(config.major_version(), 1);
    assert_eq!(config.minor_version(), 0);
    let config = Config::create_from_str(&with("2.0")).unwrap();
    assert_eq!(config.major_version(), 2);
    assert_eq!(config.minor_version(), 0);
}
