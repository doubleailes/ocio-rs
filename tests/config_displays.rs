//! Port of `Config_tests.cpp` (part 4: displays, views and active lists).

mod config_common;

use config_common::*;
use ocio::*;

const DISPLAY_HEADER: &str = r#"ocio_profile_version: 2

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
  sRGB_2:
    - !<View> {name: Raw, colorspace: raw}
  sRGB_F:
    - !<View> {name: Raw, colorspace: raw}
  sRGB_1:
    - !<View> {name: Raw, colorspace: raw}
  sRGB_3:
    - !<View> {name: Raw, colorspace: raw}
  sRGB_B:
    - !<View> {name: Raw, colorspace: raw}
  sRGB_A:
    - !<View> {name: Raw, colorspace: raw}

"#;

const DISPLAY_FOOTER: &str = r#"
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
"#;

fn display_profile(active: &str) -> String {
    format!("{DISPLAY_HEADER}active_displays: {active}\nactive_views: []\n{DISPLAY_FOOTER}")
}

fn displays(config: &Config) -> Vec<String> {
    (0..config.num_displays()).map(|i| config.display(i)).collect()
}

#[test]
fn config_display() {
    let _lock = env_lock();
    let _g = EnvGuard::set(OCIO_ACTIVE_DISPLAYS_ENVVAR, None);
    {
        let p = display_profile("[]");
        let config = check_roundtrip(&p);
        assert_eq!(displays(&config), ["sRGB_2", "sRGB_F", "sRGB_1", "sRGB_3", "sRGB_B", "sRGB_A"]);
        assert_eq!(config.default_display(), "sRGB_2");
    }
    {
        let p = display_profile("[sRGB_1]");
        let config = check_roundtrip(&p);
        assert_eq!(displays(&config), ["sRGB_1"]);
        assert_eq!(config.default_display(), "sRGB_1");
        assert_eq!(config.num_displays_all(), 6);
    }
    {
        let config = Config::create_from_str(&display_profile("[sRGB_2, sRGB_1]")).unwrap();
        assert_eq!(displays(&config), ["sRGB_2", "sRGB_1"]);
        assert_eq!(config.default_display(), "sRGB_2");
    }
    for active in ["[]", "[sRGB_2, sRGB_1]"] {
        let _g = EnvGuard::set(OCIO_ACTIVE_DISPLAYS_ENVVAR, Some(" sRGB_3, sRGB_2"));
        let config = Config::create_from_str(&display_profile(active)).unwrap();
        config.validate().unwrap();
        assert_eq!(displays(&config), ["sRGB_3", "sRGB_2"]);
        assert_eq!(config.default_display(), "sRGB_3");
    }
    for env in ["", " "] {
        let _g = EnvGuard::set(OCIO_ACTIVE_DISPLAYS_ENVVAR, Some(env));
        let config = Config::create_from_str(&display_profile("[sRGB_2, sRGB_1]")).unwrap();
        config.validate().unwrap();
        assert_eq!(displays(&config), ["sRGB_2", "sRGB_1"]);
        assert_eq!(config.default_display(), "sRGB_2");
    }
    {
        let _g = EnvGuard::set(OCIO_ACTIVE_DISPLAYS_ENVVAR, Some("ABCDEF"));
        let config = Config::create_from_str(&display_profile("[sRGB_2, sRGB_1]")).unwrap();
        assert_err!(
            config.validate(),
            "The content of the env. variable for the list of active displays [ABCDEF] is invalid."
        );
    }
    {
        let _g = EnvGuard::set(OCIO_ACTIVE_DISPLAYS_ENVVAR, Some("sRGB_2, sRGB_1, ABCDEF"));
        let config = Config::create_from_str(&display_profile("[sRGB_2, sRGB_1]")).unwrap();
        assert_err!(
            config.validate(),
            "The content of the env. variable for the list of active displays [sRGB_2, sRGB_1, ABCDEF] contains invalid display name(s)."
        );
    }
    {
        let config = Config::create_from_str(&display_profile("[ABCDEF]")).unwrap();
        // The active displays list is ignored if it would remove all displays.
        assert_eq!(config.num_displays(), 6);
        assert_eq!(config.display(0), "sRGB_2");
        assert_eq!(config.display(1), "sRGB_F");
        assert_eq!(config.default_display(), "sRGB_2");
        assert_err!(
            config.validate(),
            "The list of active displays [ABCDEF] from the config file is invalid."
        );
    }
    {
        let config = Config::create_from_str(&display_profile("[sRGB_2, sRGB_1, ABCDEF]")).unwrap();
        assert_err!(
            config.validate(),
            "The list of active displays [sRGB_2, sRGB_1, ABCDEF] from the config file contains invalid display name(s)"
        );
    }
}

const VIEW_HEADER: &str = r#"ocio_profile_version: 1

search_path: luts
strictparsing: true
luma: [0.2126, 0.7152, 0.0722]

roles:
  default: raw
  scene_linear: lnh

displays:
  sRGB_1:
    - !<View> {name: View_1, colorspace: raw}
    - !<View> {name: View_2, colorspace: raw}
  sRGB_2:
    - !<View> {name: View_2, colorspace: raw}
    - !<View> {name: View_3, colorspace: raw}
  sRGB_3:
    - !<View> {name: View_3, colorspace: raw}
    - !<View> {name: View_1, colorspace: raw}

"#;

fn view_profile(active: &str) -> String {
    format!("{VIEW_HEADER}active_displays: []\nactive_views: {active}\n{DISPLAY_FOOTER}")
}

fn views(config: &Config, display: &str) -> Vec<String> {
    (0..config.num_views(display)).map(|i| config.view(display, i)).collect()
}

#[track_caller]
fn check_views(config: &Config, expected: [(&str, &str, &[&str]); 3]) {
    for (display, default, list) in expected {
        assert_eq!(config.default_view(display), default, "display {display}");
        assert_eq!(views(config, display), list, "display {display}");
    }
}

#[test]
fn config_view() {
    let _lock = env_lock();
    let _g = EnvGuard::set(OCIO_ACTIVE_VIEWS_ENVVAR, None);
    let all: [(&str, &str, &[&str]); 3] = [
        ("sRGB_1", "View_1", &["View_1", "View_2"]),
        ("sRGB_2", "View_2", &["View_2", "View_3"]),
        ("sRGB_3", "View_3", &["View_3", "View_1"]),
    ];
    {
        let p = view_profile("[]");
        let config = Config::create_from_str(&p).unwrap();
        check_views(&config, all);
        assert_eq!(config.view("sRGB_1", 42), "");
        assert_eq!(config.serialize().unwrap(), p);
    }
    {
        let p = view_profile("[View_3]");
        let config = Config::create_from_str(&p).unwrap();
        // The active views list is ignored, for a display, if it would remove all views.
        check_views(
            &config,
            [
                ("sRGB_1", "View_1", &["View_1", "View_2"]),
                ("sRGB_2", "View_3", &["View_3"]),
                ("sRGB_3", "View_3", &["View_3"]),
            ],
        );
        for d in ["sRGB_1", "sRGB_2", "sRGB_3"] {
            assert_eq!(config.num_views_by_type(ViewType::DisplayDefined, d), 2);
        }
        assert_eq!(config.serialize().unwrap(), p);
    }
    {
        let config = Config::create_from_str(&view_profile("[View_3, View_2, View_1]")).unwrap();
        check_views(
            &config,
            [
                ("sRGB_1", "View_2", &["View_2", "View_1"]),
                ("sRGB_2", "View_3", &["View_3", "View_2"]),
                ("sRGB_3", "View_3", &["View_3", "View_1"]),
            ],
        );
    }
    {
        let _g = EnvGuard::set(OCIO_ACTIVE_VIEWS_ENVVAR, Some(" View_3, View_2"));
        let config = Config::create_from_str(&view_profile("[]")).unwrap();
        check_views(
            &config,
            [
                ("sRGB_1", "View_2", &["View_2"]),
                ("sRGB_2", "View_3", &["View_3", "View_2"]),
                ("sRGB_3", "View_3", &["View_3"]),
            ],
        );
    }
    for env in ["", " "] {
        let _g = EnvGuard::set(OCIO_ACTIVE_VIEWS_ENVVAR, Some(env));
        let config = Config::create_from_str(&view_profile("[]")).unwrap();
        check_views(&config, all);
    }
}

#[test]
fn config_display_view_order() {
    const CONFIG: &str = r#"
        ocio_profile_version: 2

        environment:
          {}

        displays:
          sRGB_B:
            - !<View> {name: View_2, colorspace: raw}
            - !<View> {name: View_1, colorspace: raw}
          sRGB_D:
            - !<View> {name: View_2, colorspace: raw}
            - !<View> {name: View_3, colorspace: raw}
          sRGB_A:
            - !<View> {name: View_3, colorspace: raw}
            - !<View> {name: View_1, colorspace: raw}
          sRGB_C:
            - !<View> {name: View_4, colorspace: raw}
            - !<View> {name: View_1, colorspace: raw}

        colorspaces:
          - !<ColorSpace>
            name: raw
            allocation: uniform

          - !<ColorSpace>
            name: lnh
            allocation: uniform

        file_rules:
          - !<Rule> {name: Default, colorspace: raw}
        "#;
    let config = Config::create_from_str(CONFIG).unwrap();
    config.validate().unwrap();
    assert_eq!(config.num_displays(), 4);
    assert_eq!(config.default_display(), "sRGB_B");
    assert_eq!(displays(&config), ["sRGB_B", "sRGB_D", "sRGB_A", "sRGB_C"]);
    assert_eq!(config.default_view("sRGB_B"), "View_2");
    assert_eq!(views(&config, "sRGB_B"), ["View_2", "View_1"]);
}

#[test]
fn config_active_displayview_lists() {
    let mut config = Config::create_raw().create_editable_copy();

    assert_eq!(config.num_active_displays(), 0);
    assert_eq!(config.num_active_views(), 0);
    config.add_active_display("sRGB").unwrap();
    config.add_active_display("Display P3").unwrap();
    config.add_active_view("v1").unwrap();
    config.add_active_view("v2").unwrap();

    assert_eq!(config.num_active_displays(), 2);
    assert_eq!(config.active_display(0), Some("sRGB"));
    assert_eq!(config.active_display(1), Some("Display P3"));
    assert_eq!(config.num_active_views(), 2);
    assert_eq!(config.active_view(0), Some("v1"));
    assert_eq!(config.active_view(1), Some("v2"));

    config.add_active_display("sRGB").unwrap();
    config.add_active_view("v1").unwrap();
    assert_eq!(config.num_active_displays(), 2);
    assert_eq!(config.num_active_views(), 2);

    config.set_active_displays("sRGB:01, \"Name, with comma\", \"Quoted name\"").unwrap();
    assert_eq!(config.num_active_displays(), 3);
    assert_eq!(config.active_display(0), Some("sRGB:01"));
    assert_eq!(config.active_display(1), Some("Name, with comma"));
    config.set_active_views("v:01, \"View, with comma\", \"Quoted view\"").unwrap();
    assert_eq!(config.num_active_views(), 3);
    assert_eq!(config.active_view(0), Some("v:01"));
    assert_eq!(config.active_view(1), Some("View, with comma"));

    config.remove_active_display("Name, with comma").unwrap();
    assert_eq!(config.num_active_displays(), 2);
    assert_eq!(config.active_display(1), Some("Quoted name"));
    config.remove_active_view("View, with comma").unwrap();
    assert_eq!(config.num_active_views(), 2);
    assert_eq!(config.active_view(1), Some("Quoted view"));

    config.clear_active_displays();
    assert_eq!(config.num_active_displays(), 0);
    config.clear_active_views();
    assert_eq!(config.num_active_views(), 0);

    assert_err!(
        config.remove_active_display("not found"),
        "Active display could not be removed from config"
    );
    assert_err!(config.remove_active_view("not found"), "Active view could not be removed from config");

    config.set_active_displays("").unwrap();
    assert_eq!(config.num_active_displays(), 0);
    config.add_active_display("sRGB").unwrap();
    assert_eq!(config.num_active_displays(), 1);
    config.set_active_views("").unwrap();
    assert_eq!(config.num_active_views(), 0);
    config.add_active_view("v1").unwrap();
    assert_eq!(config.num_active_views(), 1);

    {
        config.set_active_displays("sRGB:01, \"Name, with comma\", \"Quoted name\"").unwrap();
        config.set_active_views("v:01, \"View, with comma\", \"Quoted view\"").unwrap();
        let config2 = Config::create_from_str(&config.serialize().unwrap()).unwrap();
        assert_eq!(config2.num_active_displays(), 3);
        assert_eq!(config2.active_display(0), Some("sRGB:01"));
        assert_eq!(config2.active_display(1), Some("Name, with comma"));
        assert_eq!(config2.active_display(2), Some("Quoted name"));
        assert_eq!(config2.num_active_views(), 3);
        assert_eq!(config2.active_view(0), Some("v:01"));
        assert_eq!(config2.active_view(1), Some("View, with comma"));
        assert_eq!(config2.active_view(2), Some("Quoted view"));
    }
    {
        config.set_active_displays("sRGB01 : Name : \"Quoted name\"").unwrap();
        config.set_active_views("v01:View: \"Quoted view\"").unwrap();
        let config2 = Config::create_from_str(&config.serialize().unwrap()).unwrap();
        assert_eq!(config2.num_active_displays(), 3);
        assert_eq!(config2.active_displays(), "sRGB01, Name, Quoted name");
        assert_eq!(config2.num_active_views(), 3);
        assert_eq!(config2.active_views(), "v01, View, Quoted view");
    }
}
