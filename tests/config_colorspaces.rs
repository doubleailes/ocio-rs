//! Port of `Config_tests.cpp` (part 7: adding/removing color spaces and
//! inactive color spaces).

mod config_common;

use config_common::profiles::*;
use config_common::*;
use ocio::config::logging::LogGuard;
use ocio::config::ColorSpace;
use ocio::*;

#[test]
fn config_add_color_space() {
    let _lock = env_lock();
    let s = format!(
        "{}    from_scene_reference: !<MatrixTransform> {{offset: [-1, -2, -3, -4]}}\n",
        profile_v2_start()
    );
    let mut config = Config::create_from_str(&s).unwrap().create_editable_copy();
    config.validate().unwrap();
    assert_eq!(config.num_color_spaces(), 3);

    let mut cs = ColorSpace::default();
    cs.set_name("astéroïde");
    cs.set_description("é À Â Ç É È ç -- $ € 円 £ 元");
    cs.set_transform(
        Some(FixedFunctionTransform::new(FixedFunctionStyle::AcesRedMod03, &[]).into()),
        ColorSpaceDirection::ToReference,
    );

    let name = "astéroïde";
    assert_eq!(config.index_for_color_space(name), None);
    config.add_color_space(&cs).unwrap();
    assert_eq!(config.index_for_color_space(name), Some(3));

    let res = format!(
        "{s}\n  - !<ColorSpace>\n    name: {name}\n    family: \"\"\n    equalitygroup: \"\"\n    bitdepth: unknown\n    description: é À Â Ç É È ç -- $ € 円 £ 元\n    isdata: false\n    allocation: uniform\n    to_scene_reference: !<FixedFunctionTransform> {{style: ACES_RedMod03}}\n"
    );
    assert_eq!(config.serialize().unwrap(), res);

    config.remove_color_space(name);
    assert_eq!(config.num_color_spaces(), 3);
    assert_eq!(config.index_for_color_space(name), None);

    config.clear_color_spaces();
    assert_eq!(config.num_color_spaces(), 0);
}

#[test]
fn config_faulty_config_file() {
    let _lock = env_lock();
    assert_err!(
        Config::create_from_str("/usr/tmp/not_existing.ocio"),
        "Error: Loading the OCIO profile failed."
    );
}

#[test]
fn config_remove_color_space() {
    let _lock = env_lock();
    let s = format!(
        "{}    from_scene_reference: !<MatrixTransform> {{offset: [-1, -2, -3, -4]}}\n\n  - !<ColorSpace>\n    name: cs5\n    allocation: uniform\n    to_scene_reference: !<FixedFunctionTransform> {{style: ACES_RedMod03}}\n",
        profile_v2_start()
    );
    let mut config = Config::create_from_str(&s).unwrap().create_editable_copy();
    config.validate().unwrap();
    assert_eq!(config.num_color_spaces(), 4);

    assert_eq!(config.index_for_color_space("cs5"), Some(3));
    config.remove_color_space("cs5");
    assert_eq!(config.num_color_spaces(), 3);
    assert_eq!(config.index_for_color_space("cs5"), None);

    config.remove_color_space("cs5");
    config.validate().unwrap();

    config.remove_color_space("scene_linear");
    config.validate().unwrap();

    config.remove_color_space("raw");
    assert_err!(
        config.validate(),
        "Config failed role validation. The role 'default' refers to a color space, 'raw', which is not defined."
    );
}

const INACTIVE_START: &str = r#"ocio_profile_version: 2

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

const INACTIVE_END: &str = r#"
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

  - !<ColorSpace>
    name: cs1
    aliases: [alias1]
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    categories: [file-io]
    allocation: uniform
    from_scene_reference: !<CDLTransform> {offset: [0.1, 0.1, 0.1]}

  - !<ColorSpace>
    name: cs2
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    categories: [working-space]
    allocation: uniform
    from_scene_reference: !<CDLTransform> {offset: [0.2, 0.2, 0.2]}

  - !<ColorSpace>
    name: cs3
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    categories: [cat3]
    allocation: uniform
    from_scene_reference: !<CDLTransform> {offset: [0.3, 0.3, 0.3]}
"#;

use ColorSpaceVisibility as Vis;
use SearchReferenceSpaceType as Ref;

fn n(c: &Config, r: Ref, v: Vis) -> usize {
    c.num_color_spaces_filtered(r, v)
}

fn name(c: &Config, r: Ref, v: Vis, i: usize) -> String {
    c.color_space_name_by_index_filtered(r, v, i).to_string()
}

fn active_names(c: &Config) -> Vec<String> {
    (0..c.num_color_spaces())
        .map(|i| c.color_space_name_by_index(i).to_string())
        .collect()
}

#[track_caller]
fn check_counts(c: &Config, all: [usize; 3], scene: [usize; 3], display: [usize; 3]) {
    for (r, e) in [
        (Ref::All, all),
        (Ref::Scene, scene),
        (Ref::Display, display),
    ] {
        assert_eq!(n(c, r, Vis::Inactive), e[0], "{r:?} inactive");
        assert_eq!(n(c, r, Vis::Active), e[1], "{r:?} active");
        assert_eq!(n(c, r, Vis::All), e[2], "{r:?} all");
    }
}

fn inactive_config(extra: &str) -> Config {
    Config::create_from_str(&format!("{INACTIVE_START}{extra}{INACTIVE_END}"))
        .unwrap()
        .create_editable_copy()
}

#[test]
fn config_inactive_color_space() {
    let _lock = env_lock();
    let _g = EnvGuard::set(OCIO_INACTIVE_COLORSPACES_ENVVAR, None);
    let mut config = inactive_config("");
    config.validate().unwrap();

    // Step 1 - No inactive color spaces.
    check_counts(&config, [0, 5, 5], [0, 5, 5], [0, 0, 0]);
    let all: Vec<String> = (0..6)
        .map(|i| name(&config, Ref::All, Vis::All, i))
        .collect();
    assert_eq!(all, ["raw", "lnh", "cs1", "cs2", "cs3", ""]);
    assert_eq!(active_names(&config), ["raw", "lnh", "cs1", "cs2", "cs3"]);
    assert_eq!(config.color_space_name_by_index(5), "");
    assert_eq!(config.color_spaces("").num_color_spaces(), 5);
    assert_eq!(
        config.get_color_space("scene_linear").unwrap().name(),
        "lnh"
    );
    assert_eq!(config.index_for_color_space("scene_linear"), Some(1));
    assert_eq!(config.index_for_color_space("lnh"), Some(1));

    // Step 2 - Some inactive color spaces.
    config.set_inactive_color_spaces("lnh, cs1");
    assert_eq!(config.inactive_color_spaces(), "lnh, cs1");
    check_counts(&config, [2, 3, 5], [2, 3, 5], [0, 0, 0]);
    let all: Vec<String> = (0..5)
        .map(|i| name(&config, Ref::All, Vis::All, i))
        .collect();
    assert_eq!(all, ["raw", "lnh", "cs1", "cs2", "cs3"]);
    assert_eq!(active_names(&config), ["raw", "cs2", "cs3"]);
    assert_eq!(config.color_spaces("").num_color_spaces(), 3);
    assert_eq!(config.color_spaces("file-io").num_color_spaces(), 0);
    assert_eq!(config.color_spaces("working-space").num_color_spaces(), 1);
    assert_eq!(config.get_color_space("cs2").unwrap().name(), "cs2");
    assert_eq!(config.get_color_space("cs1").unwrap().name(), "cs1");
    assert_eq!(config.get_color_space("default").unwrap().name(), "raw");
    assert_eq!(
        config.get_color_space("scene_linear").unwrap().name(),
        "lnh"
    );
    assert_eq!(config.index_for_color_space("scene_linear"), None);
    assert_eq!(config.index_for_color_space("lnh"), None);
    assert_eq!(config.color_space_name_by_index(3), "");
    assert!(config.get_color_space("cs1").is_some());

    // Step 3 - Same as 2, but using role name.
    config.set_inactive_color_spaces("scene_linear, cs1");
    assert_eq!(config.inactive_color_spaces(), "scene_linear, cs1");
    check_counts(&config, [2, 3, 5], [2, 3, 5], [0, 0, 0]);
    assert_eq!(active_names(&config), ["raw", "cs2", "cs3"]);
    assert!(config.has_role("scene_linear"));

    // Step 4 - Same as 2, but using an alias.
    config.set_inactive_color_spaces("lnh, alias1");
    assert_eq!(config.inactive_color_spaces(), "lnh, alias1");
    check_counts(&config, [2, 3, 5], [2, 3, 5], [0, 0, 0]);
    assert_eq!(active_names(&config), ["raw", "cs2", "cs3"]);

    // Step 5 & 6 - No inactive color spaces.
    config.set_inactive_color_spaces("");
    assert_eq!(config.inactive_color_spaces(), "");
    assert_eq!(n(&config, Ref::All, Vis::All), 5);
    assert_eq!(config.num_color_spaces(), 5);
    assert_eq!(n(&config, Ref::Scene, Vis::All), 5);
    assert_eq!(n(&config, Ref::Display, Vis::All), 0);

    // Step 7 - Add display color spaces.
    for d in ["display0", "display1", "display2"] {
        let mut dcs = ColorSpace::new(ReferenceSpaceType::Display);
        dcs.set_name(d);
        config.add_color_space(&dcs).unwrap();
    }
    assert_eq!(n(&config, Ref::All, Vis::All), 8);
    assert_eq!(n(&config, Ref::Scene, Vis::All), 5);
    assert_eq!(n(&config, Ref::Display, Vis::All), 3);

    // Step 8 - Some inactive color spaces.
    config.set_inactive_color_spaces("cs1, display1");
    assert_eq!(config.inactive_color_spaces(), "cs1, display1");
    check_counts(&config, [2, 6, 8], [1, 4, 5], [1, 2, 3]);
    assert_eq!(name(&config, Ref::Scene, Vis::Inactive, 0), "cs1");
    assert_eq!(name(&config, Ref::Display, Vis::Inactive, 0), "display1");
    assert_eq!(name(&config, Ref::Scene, Vis::Inactive, 1), "");
    assert_eq!(name(&config, Ref::Display, Vis::Inactive, 1), "");
    assert_eq!(name(&config, Ref::Scene, Vis::Active, 2), "cs2");
    assert_eq!(name(&config, Ref::Display, Vis::Active, 1), "display2");
    assert_eq!(name(&config, Ref::Scene, Vis::All, 0), "raw");
    assert_eq!(name(&config, Ref::Scene, Vis::All, 3), "cs2");
    assert_eq!(name(&config, Ref::Scene, Vis::All, 10), "");
    assert_eq!(name(&config, Ref::Display, Vis::All, 1), "display1");
}

#[test]
fn config_inactive_color_space_processors() {
    let _lock = env_lock();
    let _g = EnvGuard::set(OCIO_INACTIVE_COLORSPACES_ENVVAR, None);
    let mut config = inactive_config("");
    config.set_inactive_color_spaces("lnh, cs1");
    let dst = config
        .display_view_color_space_name("sRGB", "Lnh")
        .to_string();
    let lt: Transform = LookTransform::new("raw", &dst, "beauty").into();
    config
        .get_processor_for_transform(&lt, TransformDirection::Forward)
        .unwrap();
    config.get_processor("lnh", "cs1").unwrap();
    config.get_processor("raw", "cs1").unwrap();
    config.get_processor("lnh", "cs2").unwrap();
    config.get_processor("cs2", "scene_linear").unwrap();
}

#[test]
fn config_is_inactive() {
    let _lock = env_lock();
    let config =
        Config::create_from_builtin_config("studio-config-v1.0.0_aces-v1.3_ocio-v2.1").unwrap();
    config.validate().unwrap();
    assert!(!config.is_inactive_color_space(""));
    assert!(!config.is_inactive_color_space("fake-colorspace-name"));
    assert!(!config.is_inactive_color_space("Linear P3-D65"));
    assert!(config.is_inactive_color_space("Rec.1886 Rec.2020 - Display"));
}

#[test]
fn config_is_inactive_local() {
    let _lock = env_lock();
    // Same checks as `config_is_inactive` using a local config.
    let _g = EnvGuard::set(OCIO_INACTIVE_COLORSPACES_ENVVAR, None);
    let config = inactive_config("inactive_colorspaces: [cs2]\n");
    assert!(!config.is_inactive_color_space(""));
    assert!(!config.is_inactive_color_space("fake-colorspace-name"));
    assert!(!config.is_inactive_color_space("cs1"));
    assert!(config.is_inactive_color_space("cs2"));
}

#[test]
fn config_inactive_color_space_precedence() {
    let _lock = env_lock();
    let _g = EnvGuard::set(OCIO_INACTIVE_COLORSPACES_ENVVAR, None);
    let config = inactive_config("inactive_colorspaces: [cs2]\n");
    config.validate().unwrap();
    assert_eq!(n(&config, Ref::All, Vis::Inactive), 1);
    assert_eq!(n(&config, Ref::All, Vis::Active), 4);
    assert_eq!(n(&config, Ref::All, Vis::All), 5);
    assert_eq!(active_names(&config), ["raw", "lnh", "cs1", "cs3"]);

    // Env. variable supersedes the config content.
    let _g2 = EnvGuard::set(OCIO_INACTIVE_COLORSPACES_ENVVAR, Some("cs3, cs1, lnh"));
    let mut config = inactive_config("inactive_colorspaces: [cs2]\n");
    config.validate().unwrap();
    assert_eq!(n(&config, Ref::All, Vis::Inactive), 3);
    assert_eq!(n(&config, Ref::All, Vis::Active), 2);
    assert_eq!(n(&config, Ref::All, Vis::All), 5);
    assert_eq!(active_names(&config), ["raw", "cs2"]);

    // An API request supersedes the lists from the env. variable and the config file.
    config.set_inactive_color_spaces("cs1, lnh");
    assert_eq!(n(&config, Ref::All, Vis::Inactive), 2);
    assert_eq!(n(&config, Ref::All, Vis::Active), 3);
    assert_eq!(n(&config, Ref::All, Vis::All), 5);
    assert_eq!(active_names(&config), ["raw", "cs2", "cs3"]);
}

#[test]
fn config_inactive_color_space_read_write() {
    let _lock = env_lock();
    let _g = EnvGuard::set(OCIO_INACTIVE_COLORSPACES_ENVVAR, None);
    {
        let s = format!("{INACTIVE_START}inactive_colorspaces: [cs2]\n{INACTIVE_END}");
        let config = Config::create_from_str(&s).unwrap();
        config.validate().unwrap();
        assert_eq!(n(&config, Ref::All, Vis::All), 5);
        assert_eq!(config.num_color_spaces(), 4);
        assert_eq!(config.serialize().unwrap(), s);
    }
    {
        let _g2 = EnvGuard::set(OCIO_INACTIVE_COLORSPACES_ENVVAR, Some("cs3, cs1, lnh"));
        let s = format!("{INACTIVE_START}inactive_colorspaces: [cs2]\n{INACTIVE_END}");
        let config = Config::create_from_str(&s).unwrap();
        {
            let _log = LogGuard::new();
            config.validate().unwrap();
        }
        assert_eq!(n(&config, Ref::All, Vis::All), 5);
        assert_eq!(config.num_color_spaces(), 2);
        assert_eq!(config.serialize().unwrap(), s);
    }
    {
        let s = format!(
            "{INACTIVE_START}inactive_colorspaces: [cs1\t\n   \n,   \ncs2]\n{INACTIVE_END}"
        );
        let config = Config::create_from_str(&s).unwrap();
        config.validate().unwrap();
        assert_eq!(n(&config, Ref::All, Vis::All), 5);
        assert_eq!(config.num_color_spaces(), 3);
        assert_eq!(
            config.serialize().unwrap(),
            format!("{INACTIVE_START}inactive_colorspaces: [cs1, cs2]\n{INACTIVE_END}")
        );
    }
    {
        let s = format!("{INACTIVE_START}inactive_colorspaces: []\n{INACTIVE_END}");
        let config = Config::create_from_str(&s).unwrap();
        config.validate().unwrap();
        assert_eq!(config.num_color_spaces(), 5);
        assert_eq!(
            config.serialize().unwrap(),
            format!("{INACTIVE_START}{INACTIVE_END}")
        );
    }
    {
        let s = format!("{INACTIVE_START}inactive_colorspaces: [unknown]\n{INACTIVE_END}");
        let config = Config::create_from_str(&s).unwrap();
        {
            let log = LogGuard::new();
            config.validate().unwrap();
            assert_eq!(
                log.output(),
                "[OpenColorIO Info]: Inactive 'unknown' is neither a color space nor a named transform.\n"
            );
        }
        assert_eq!(n(&config, Ref::All, Vis::All), 5);
        assert_eq!(config.num_color_spaces(), 5);
        assert_eq!(config.serialize().unwrap(), s);
    }
}
