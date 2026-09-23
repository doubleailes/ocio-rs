//! Port of `Display_tests.cpp`.

mod config_common;

use ocio::config::{ColorSpace, Look, ViewTransform, ViewingRules};
use ocio::*;

#[test]
fn shared_views_basic() {
    // Shared views can not be used with v1 config.
    let mut config = Config::create();
    config.set_major_version(1).unwrap();
    config
        .add_shared_view("shared1", "", "colorspace", "", "", "")
        .unwrap();
    assert_err!(
        config.serialize(),
        "Only version 2 (or higher) can have shared views"
    );

    // Using a v2 config.
    let mut config = Config::create_raw().create_editable_copy();
    config.validate().unwrap();

    // Shared views need to refer to existing colorspaces.
    config
        .add_shared_view("shared1", "", "colorspace1", "", "", "")
        .unwrap();
    assert_err!(
        config.validate(),
        "color space or a named transform, 'colorspace1', which is not defined"
    );

    let mut cs = ColorSpace::default();
    cs.set_name("colorspace1");
    config.add_color_space(&cs).unwrap();
    config.validate().unwrap();

    // Shared views need to refer to existing looks.
    cs.set_name("colorspace2");
    config.add_color_space(&cs).unwrap();
    config
        .add_shared_view("shared2", "", "colorspace2", "look1", "", "")
        .unwrap();
    assert_err!(
        config.validate(),
        "refers to a look, 'look1', which is not defined."
    );

    let mut lk = Look::new();
    lk.set_name("look1");
    lk.set_process_space("look1_process");
    cs.set_name("look1_process");
    config.add_color_space(&cs).unwrap();
    config.add_look(&lk).unwrap();
    config.validate().unwrap();

    // Shared views need to refer to existing view transforms.
    let mut cs = ColorSpace::new(ReferenceSpaceType::Display);
    cs.set_name("colorspace3");
    config.add_color_space(&cs).unwrap();
    config
        .add_shared_view(
            "shared3",
            "viewTransform1",
            "colorspace3",
            "",
            "",
            "shared view description",
        )
        .unwrap();
    assert_err!(
        config.validate(),
        "refers to a view transform, 'viewTransform1', which is neither a view transform nor a named transform"
    );

    let mut vt = ViewTransform::new(ReferenceSpaceType::Scene);
    vt.set_name("viewTransform1");
    vt.set_transform(
        Some(MatrixTransform::default().into()),
        ViewTransformDirection::FromReference,
    );
    config.add_view_transform(&vt).unwrap();
    config.validate().unwrap();

    // Shared views need to refer to existing rules.
    config
        .add_shared_view("shared4", "", "colorspace1", "", "rule1", "")
        .unwrap();
    assert_err!(
        config.validate(),
        "viewing rule, 'rule1', which is not defined"
    );

    let mut vrules = ViewingRules::new();
    vrules.insert_rule(0, "rule1").unwrap();
    vrules.add_color_space(0, "colorspace3").unwrap();
    config.set_viewing_rules(&vrules);
    config.validate().unwrap();

    // Add shared view with description.
    config
        .add_shared_view("shared5", "", "colorspace2", "", "", "Sample description")
        .unwrap();
    config.validate().unwrap();

    config
        .add_display_view("sRGB", "view1", "colorspace1", "")
        .unwrap();
    config.validate().unwrap();

    config.add_display_shared_view("sRGB", "shared2").unwrap();
    config.add_display_shared_view("sRGB", "shared3").unwrap();
    config.add_display_shared_view("sRGB", "shared4").unwrap();
    config.validate().unwrap();

    assert_eq!(config.num_views("sRGB"), 5);
    assert_eq!(config.view("sRGB", 0), "Raw");
    assert_eq!(config.view("sRGB", 1), "view1");
    assert_eq!(config.view("sRGB", 2), "shared2");
    assert_eq!(config.view("sRGB", 3), "shared3");
    assert_eq!(config.view("sRGB", 4), "shared4");
    assert_eq!(
        config.num_views_by_type(ViewType::DisplayDefined, "sRGB"),
        2
    );
    assert_eq!(
        config.view_by_type(ViewType::DisplayDefined, "sRGB", 0),
        "Raw"
    );
    assert_eq!(
        config.view_by_type(ViewType::DisplayDefined, "sRGB", 1),
        "view1"
    );
    assert_eq!(config.num_views_by_type(ViewType::Shared, "sRGB"), 3);
    assert_eq!(config.view_by_type(ViewType::Shared, "sRGB", 0), "shared2");
    assert_eq!(config.view_by_type(ViewType::Shared, "sRGB", 1), "shared3");
    assert_eq!(config.view_by_type(ViewType::Shared, "sRGB", 2), "shared4");

    assert_eq!(
        config.display_view_color_space_name("sRGB", "view1"),
        "colorspace1"
    );
    assert_eq!(
        config.display_view_color_space_name("sRGB", "shared2"),
        "colorspace2"
    );
    assert_eq!(
        config.display_view_transform_name("sRGB", "shared3"),
        "viewTransform1"
    );
    assert_eq!(config.display_view_looks("sRGB", "shared2"), "look1");
    assert_eq!(config.display_view_rule("sRGB", "shared4"), "rule1");
    assert_eq!(
        config.display_view_description("sRGB", "shared3"),
        "shared view description"
    );

    // An empty display name may be used to access shared views.
    assert_eq!(
        config.display_view_color_space_name("", "shared1"),
        "colorspace1"
    );
    assert_eq!(
        config.display_view_color_space_name("", "shared2"),
        "colorspace2"
    );
    assert_eq!(config.display_view_looks("", "shared2"), "look1");
    assert_eq!(
        config.display_view_transform_name("", "shared3"),
        "viewTransform1"
    );
    assert_eq!(
        config.display_view_color_space_name("", "shared3"),
        "colorspace3"
    );
    assert_eq!(config.display_view_rule("", "shared4"), "rule1");
    assert_eq!(
        config.display_view_description("", "shared5"),
        "Sample description"
    );

    // Use active views.
    config.set_active_views("view1, shared3").unwrap();
    assert_eq!(config.num_views("sRGB"), 2);
    assert_eq!(config.view("sRGB", 0), "view1");
    assert_eq!(config.view("sRGB", 1), "shared3");

    assert_eq!(config.display_view_looks("sRGB", "shared2"), "look1");

    assert_eq!(
        config.num_views_by_type(ViewType::DisplayDefined, "sRGB"),
        2
    );
    assert_eq!(config.num_views_by_type(ViewType::Shared, "sRGB"), 3);

    // Save and reload.
    let s = config.serialize().unwrap();
    let back = Config::create_from_str(&s).unwrap();
    assert_eq!(
        config.num_views_by_type(ViewType::Shared, ""),
        back.num_views_by_type(ViewType::Shared, "")
    );
    assert_eq!(
        back.display_view_transform_name("", "shared3"),
        "viewTransform1"
    );
    assert_eq!(
        back.display_view_color_space_name("", "shared3"),
        "colorspace3"
    );
    assert_eq!(back.display_view_rule("", "shared4"), "rule1");
    assert_eq!(
        back.display_view_description("", "shared5"),
        "Sample description"
    );

    assert_err!(
        config.add_display_view("sRGB", "shared2", "colorspace1", ""),
        "There is already a shared view named 'shared2' in the display 'sRGB'"
    );

    config
        .add_display_view("sRGB", "shared1", "colorspace1", "")
        .unwrap();
    config.validate().unwrap();
    assert_err!(
        config.add_display_shared_view("sRGB", "shared1"),
        "There is already a view named 'shared1' in the display 'sRGB'"
    );

    assert_eq!(config.num_views_by_type(ViewType::Shared, "sRGB"), 3);
    config.validate().unwrap();

    // Add undefined shared view.
    config.add_display_shared_view("sRGB", "shared42").unwrap();
    assert_err!(
        config.validate(),
        "contains a shared view 'shared42' that is not defined"
    );

    config.remove_display_view("sRGB", "shared42").unwrap();
    config.validate().unwrap();

    config.remove_shared_view("shared1").unwrap();
    config.validate().unwrap();

    config
        .add_shared_view(
            "shared3",
            "viewTransform1",
            OCIO_VIEW_USE_DISPLAY_NAME,
            "",
            "",
            "shared view description",
        )
        .unwrap();
    assert_err!(
        config.validate(),
        "The display 'sRGB' contains a shared view 'shared3' which does not define a color space and there is no color space that matches the display name"
    );

    cs.set_name("sRGB");
    config.add_color_space(&cs).unwrap();
    config.validate().unwrap();

    let s = config.serialize().unwrap();
    assert!(s.contains(OCIO_VIEW_USE_DISPLAY_NAME));
    let back = Config::create_from_str(&s).unwrap();
    assert_eq!(
        back.display_view_color_space_name("", "shared3"),
        OCIO_VIEW_USE_DISPLAY_NAME
    );

    assert_eq!(config.num_views_by_type(ViewType::Shared, ""), 4);
    config.clear_shared_views();
    assert_eq!(config.num_views_by_type(ViewType::Shared, ""), 0);
}

const COMPARE_CONFIG1: &str = r#"ocio_profile_version: 2

roles:
  default: raw

file_rules:
  - !<Rule> {name: Default, colorspace: default}

shared_views:
  - !<View> {name: sview1, colorspace: raw}

displays:
  Raw:
    - !<View> {name: Raw, colorspace: raw}
  sRGB:
    - !<View> {name: Raw, colorspace: raw}
    - !<View> {name: view, view_transform: display_vt, display_colorspace: display_cs}
    - !<Views> [sview1]

active_displays: [sRGB]
active_views: [view, sview1]

view_transforms:
  - !<ViewTransform>
    name: default_vt
    to_scene_reference: !<CDLTransform> {sat: 1.5}

  - !<ViewTransform>
    name: display_vt
    to_display_reference: !<CDLTransform> {sat: 1.5}

display_colorspaces:
  - !<ColorSpace>
    name: display_cs
    to_display_reference: !<CDLTransform> {sat: 1.5}

colorspaces:
  - !<ColorSpace>
    name: raw
"#;

const COMPARE_CONFIG2: &str = r#"ocio_profile_version: 2

roles:
  default: raw

file_rules:
  - !<Rule> {name: Default, colorspace: default}

shared_views:
  - !<View> {name: view, view_transform: display_vt, display_colorspace: display_cs}
  - !<View> {name: sview1, colorspace: raw}

displays:
  Raw:
    - !<View> {name: Raw, colorspace: raw}
  sRGB:
    - !<View> {name: Raw, colorspace: raw}
    - !<Views> [view, sview1]

active_displays: [Raw]
active_views: [Raw]

view_transforms:
  - !<ViewTransform>
    name: default_vt
    to_scene_reference: !<CDLTransform> {sat: 1.5}

  - !<ViewTransform>
    name: display_vt
    to_display_reference: !<CDLTransform> {sat: 1.5}

display_colorspaces:
  - !<ColorSpace>
    name: display_cs
    to_display_reference: !<CDLTransform> {sat: 1.5}

colorspaces:
  - !<ColorSpace>
    name: raw
"#;

#[test]
fn config_compare_displays() {
    let config1 = Config::create_from_str(COMPARE_CONFIG1).unwrap();
    let config2 = Config::create_from_str(COMPARE_CONFIG2).unwrap();
    config1.validate().unwrap();
    config2.validate().unwrap();

    {
        assert_eq!(config1.num_displays(), 1);
        assert_eq!(config1.default_display(), "sRGB");
        assert_eq!(
            config1.num_views_by_type(ViewType::DisplayDefined, "sRGB"),
            2
        );
        assert_eq!(
            config1.view_by_type(ViewType::DisplayDefined, "sRGB", 0),
            "Raw"
        );
        assert_eq!(
            config1.view_by_type(ViewType::DisplayDefined, "sRGB", 1),
            "view"
        );
        assert!(!config1.is_view_shared("sRGB", "view"));
        assert_eq!(config1.num_views("sRGB"), 2);
        assert_eq!(config1.view("sRGB", 0), "view");

        assert_eq!(config2.num_displays(), 1);
        assert_eq!(config2.default_display(), "Raw");
        assert_eq!(config2.num_views("Raw"), 1);
        assert_eq!(config2.default_view("Raw"), "Raw");
        assert_eq!(config2.num_displays_all(), 2);
        assert_eq!(config2.display_all(1), "sRGB");
        assert_eq!(config2.num_views_by_type(ViewType::Shared, "sRGB"), 2);
        assert_eq!(config2.view_by_type(ViewType::Shared, "sRGB", 0), "view");
        assert_eq!(config2.view_by_type(ViewType::Shared, "sRGB", 1), "sview1");
        assert!(config2.is_view_shared("sRGB", "view"));

        assert!(Config::are_views_equal(&config1, &config2, "sRGB", "view"));
    }
    {
        assert_eq!(config1.num_displays(), 1);
        assert_eq!(config1.default_display(), "sRGB");
        assert_eq!(config1.display_all(0), "Raw");
        assert_eq!(
            config1.num_views_by_type(ViewType::DisplayDefined, "Raw"),
            1
        );
        assert_eq!(
            config1.view_by_type(ViewType::DisplayDefined, "Raw", 0),
            "Raw"
        );
        assert!(!config1.is_view_shared("Raw", "Raw"));

        assert_eq!(
            config2.num_views_by_type(ViewType::DisplayDefined, "Raw"),
            1
        );
        assert_eq!(
            config2.view_by_type(ViewType::DisplayDefined, "Raw", 0),
            "Raw"
        );
        assert!(!config2.is_view_shared("Raw", "Raw"));

        assert!(Config::are_views_equal(&config1, &config2, "Raw", "Raw"));
    }
    {
        assert_eq!(config1.num_views_by_type(ViewType::Shared, "sRGB"), 1);
        assert_eq!(config1.view_by_type(ViewType::Shared, "sRGB", 0), "sview1");
        assert!(config1.is_view_shared("sRGB", "sview1"));
        assert_eq!(config2.num_views_by_type(ViewType::Shared, "sRGB"), 2);
        assert!(config2.is_view_shared("sRGB", "sview1"));
        assert!(Config::are_views_equal(
            &config1, &config2, "sRGB", "sview1"
        ));
    }
    {
        let mut cfg1 = config1.create_editable_copy();
        assert!(cfg1.has_view("sRGB", "Raw"));
        assert!(cfg1.has_view("sRGB", "view"));
        assert!(cfg1.has_view("sRGB", "sview1"));
        assert!(cfg1.has_view("Raw", "Raw"));

        cfg1.set_active_displays("Raw").unwrap();
        assert_eq!(cfg1.num_displays(), 1);
        assert_eq!(cfg1.default_display(), "Raw");
        assert!(cfg1.has_view("sRGB", "sview1"));

        cfg1.set_active_views("Raw").unwrap();
        assert_eq!(cfg1.num_views("sRGB"), 1);
        assert_eq!(cfg1.view("sRGB", 0), "Raw");
        assert!(cfg1.has_view("sRGB", "Raw"));
        assert!(cfg1.has_view("sRGB", "sview1"));

        cfg1.set_active_displays("sRGB").unwrap();
        assert_eq!(cfg1.num_displays(), 1);
        assert_eq!(cfg1.default_display(), "sRGB");
        assert!(cfg1.has_view("sRGB", "sview1"));
    }
    {
        let mut cfg1 = config1.create_editable_copy();
        assert_eq!(cfg1.default_display(), "sRGB");
        assert_eq!(cfg1.num_views_by_type(ViewType::DisplayDefined, "sRGB"), 2);
        assert!(cfg1.has_view("sRGB", "Raw"));
        assert!(Config::are_views_equal(&config1, &cfg1, "sRGB", "Raw"));

        cfg1.remove_display_view("sRGB", "Raw").unwrap();
        assert_eq!(cfg1.num_views_by_type(ViewType::DisplayDefined, "sRGB"), 1);
        assert_eq!(
            cfg1.view_by_type(ViewType::DisplayDefined, "sRGB", 0),
            "view"
        );
        assert!(!cfg1.has_view("sRGB", "Raw"));
        assert!(!Config::are_views_equal(&config1, &cfg1, "sRGB", "Raw"));
    }
    {
        let mut cfg2 = config2.create_editable_copy();
        assert_eq!(cfg2.default_display(), "Raw");
        assert_eq!(cfg2.num_views("Raw"), 1);
        assert_eq!(cfg2.view("Raw", 0), "Raw");
        assert_eq!(cfg2.active_views(), "Raw");
        assert!(cfg2.has_view("Raw", "Raw"));
        assert!(Config::are_views_equal(&config2, &cfg2, "Raw", "Raw"));

        assert_eq!(cfg2.num_displays_all(), 2);
        cfg2.remove_display_view("Raw", "Raw").unwrap();
        assert_eq!(cfg2.num_displays_all(), 1);
        assert_eq!(cfg2.active_views(), "Raw");
        assert!(!cfg2.has_view("Raw", "Raw"));
        assert!(!Config::are_views_equal(&config2, &cfg2, "Raw", "Raw"));
    }
    {
        let mut cfg1 = config1.create_editable_copy();
        assert_eq!(cfg1.num_views_by_type(ViewType::Shared, "sRGB"), 1);
        assert!(cfg1.is_view_shared("sRGB", "sview1"));
        assert!(cfg1.has_view("sRGB", "sview1"));

        cfg1.remove_display_view("sRGB", "sview1").unwrap();
        assert_eq!(config1.default_display(), "sRGB");
        assert_eq!(cfg1.num_views_by_type(ViewType::Shared, "sRGB"), 0);

        assert_eq!(cfg1.num_views_by_type(ViewType::Shared, ""), 1);
        assert_eq!(cfg1.view_by_type(ViewType::Shared, "", 0), "sview1");
        assert!(cfg1.is_view_shared("", "sview1"));
        assert!(!cfg1.has_view("sRGB", "sview1"));
        assert!(cfg1.has_view("", "sview1"));
    }
}

const VIRTUAL_CONFIG1: &str = r#"ocio_profile_version: 2

roles:
  default: raw

file_rules:
  - !<Rule> {name: Default, colorspace: default}

viewing_rules:
  - !<Rule> {name: Linear, colorspaces: default}

shared_views:
  - !<View> {name: Film, view_transform: display_vt, display_colorspace: <USE_DISPLAY_NAME>, looks: look1, rule: Linear, description: Test view}
  - !<View> {name: view, view_transform: display_vt, display_colorspace: display_cs}

displays:
  Raw:
    - !<View> {name: Raw, colorspace: raw}
  sRGB:
    - !<View> {name: Raw, colorspace: raw}

virtual_display:
  - !<View> {name: Raw, colorspace: raw}
  - !<Views> [Film, view]

looks:
  - !<Look>
    name: look1
    process_space: default

view_transforms:
  - !<ViewTransform>
    name: default_vt
    to_scene_reference: !<CDLTransform> {sat: 1.5}

  - !<ViewTransform>
    name: display_vt
    to_display_reference: !<CDLTransform> {sat: 1.5}

display_colorspaces:
  - !<ColorSpace>
    name: display_cs
    to_display_reference: !<CDLTransform> {sat: 1.5}

colorspaces:
  - !<ColorSpace>
    name: raw
"#;

const VIRTUAL_CONFIG2: &str = r#"ocio_profile_version: 2

roles:
  default: raw

file_rules:
  - !<Rule> {name: Default, colorspace: default}

viewing_rules:
  - !<Rule> {name: Linear, colorspaces: default}

shared_views:
  - !<View> {name: view, view_transform: display_vt, display_colorspace: display_cs}

displays:
  Raw:
    - !<View> {name: Raw, colorspace: raw}
  sRGB:
    - !<View> {name: Raw, colorspace: raw}
    - !<Views> [view]

virtual_display:
  - !<View> {name: Raw, colorspace: raw}
  - !<View> {name: Film, view_transform: display_vt, display_colorspace: <USE_DISPLAY_NAME>, looks: look1, rule: Linear, description: Test view}
  - !<Views> [view]

looks:
  - !<Look>
    name: look1
    process_space: default

view_transforms:
  - !<ViewTransform>
    name: default_vt
    to_scene_reference: !<CDLTransform> {sat: 1.5}

  - !<ViewTransform>
    name: display_vt
    to_display_reference: !<CDLTransform> {sat: 1.5}

display_colorspaces:
  - !<ColorSpace>
    name: display_cs
    to_display_reference: !<CDLTransform> {sat: 1.5}

colorspaces:
  - !<ColorSpace>
    name: raw
"#;

fn check_virtual_view(
    cfg: &Config,
    name: &str,
    vt: &str,
    cs: &str,
    looks: &str,
    rule: &str,
    desc: &str,
) {
    assert_eq!(cfg.virtual_display_view_transform_name(name), vt);
    assert_eq!(cfg.virtual_display_view_color_space_name(name), cs);
    assert_eq!(cfg.virtual_display_view_looks(name), looks);
    assert_eq!(cfg.virtual_display_view_rule(name), rule);
    assert_eq!(cfg.virtual_display_view_description(name), desc);
}

#[test]
fn config_compare_virtual_displays() {
    let config1 = Config::create_from_str(VIRTUAL_CONFIG1).unwrap();
    let config2 = Config::create_from_str(VIRTUAL_CONFIG2).unwrap();
    config1.validate().unwrap();
    config2.validate().unwrap();

    {
        assert_eq!(config1.virtual_display_num_views(ViewType::Shared), 2);
        let v1 = config1
            .virtual_display_view(ViewType::Shared, 0)
            .to_string();
        assert_eq!(v1, "Film");
        check_virtual_view(
            &config1,
            &v1,
            "display_vt",
            "<USE_DISPLAY_NAME>",
            "look1",
            "Linear",
            "Test view",
        );

        assert_eq!(
            config2.virtual_display_num_views(ViewType::DisplayDefined),
            2
        );
        let v2 = config2
            .virtual_display_view(ViewType::DisplayDefined, 1)
            .to_string();
        assert_eq!(v2, "Film");
        check_virtual_view(
            &config2,
            &v2,
            "display_vt",
            "<USE_DISPLAY_NAME>",
            "look1",
            "Linear",
            "Test view",
        );

        assert!(Config::are_virtual_views_equal(&config1, &config2, &v1));
    }
    {
        assert_eq!(
            config1.virtual_display_num_views(ViewType::DisplayDefined),
            1
        );
        let v1 = config1
            .virtual_display_view(ViewType::DisplayDefined, 0)
            .to_string();
        assert_eq!(v1, "Raw");
        check_virtual_view(&config1, &v1, "", "raw", "", "", "");
        let v2 = config2
            .virtual_display_view(ViewType::DisplayDefined, 0)
            .to_string();
        assert_eq!(v2, "Raw");
        check_virtual_view(&config2, &v2, "", "raw", "", "", "");
        assert!(Config::are_virtual_views_equal(&config1, &config2, &v1));
    }
    {
        let v1 = config1
            .virtual_display_view(ViewType::Shared, 1)
            .to_string();
        assert_eq!(v1, "view");
        check_virtual_view(&config1, &v1, "display_vt", "display_cs", "", "", "");
        assert_eq!(config2.virtual_display_num_views(ViewType::Shared), 1);
        let v2 = config2
            .virtual_display_view(ViewType::Shared, 0)
            .to_string();
        assert_eq!(v2, "view");
        check_virtual_view(&config2, &v2, "display_vt", "display_cs", "", "", "");
        assert!(Config::are_virtual_views_equal(&config1, &config2, &v1));
    }
    {
        let mut cfg = config1.create_editable_copy();
        assert!(config1.has_virtual_view("Film"));
        assert!(config1.is_virtual_view_shared("Film"));
        assert_eq!(cfg.virtual_display_num_views(ViewType::Shared), 2);
        assert!(cfg.has_virtual_view("Film"));
        assert!(cfg.is_virtual_view_shared("Film"));
        assert!(Config::are_virtual_views_equal(&config1, &cfg, "Film"));
        assert!(Config::are_virtual_views_equal(&config2, &cfg, "Film"));

        cfg.remove_virtual_display_view("Film");
        assert_eq!(cfg.virtual_display_num_views(ViewType::Shared), 1);
        assert!(!cfg.has_virtual_view("Film"));
        assert!(!cfg.is_virtual_view_shared("Film"));
        assert!(!Config::are_virtual_views_equal(&config1, &cfg, "Film"));
        assert!(!Config::are_virtual_views_equal(&config2, &cfg, "Film"));
    }
    {
        let mut cfg = config2.create_editable_copy();
        assert!(config2.has_virtual_view("Film"));
        assert!(!config2.is_virtual_view_shared("Film"));
        assert_eq!(cfg.virtual_display_num_views(ViewType::DisplayDefined), 2);
        assert!(cfg.has_virtual_view("Film"));
        assert!(!cfg.is_virtual_view_shared("Film"));
        assert!(Config::are_virtual_views_equal(&config2, &cfg, "Film"));
        assert!(Config::are_virtual_views_equal(&config1, &cfg, "Film"));

        cfg.remove_virtual_display_view("Film");
        assert_eq!(cfg.virtual_display_num_views(ViewType::DisplayDefined), 1);
        assert!(!cfg.has_virtual_view("Film"));
        assert!(!Config::are_virtual_views_equal(&config2, &cfg, "Film"));
        assert!(!Config::are_virtual_views_equal(&config1, &cfg, "Film"));
    }
}
