//! Port of `Config_tests.cpp` (part 10: virtual display, description, alias
//! validation, looks, archives, interchange attributes and cycles).

mod config_common;

use config_common::profiles::*;
use config_common::*;
use ocio::config::{ColorSpace, FileRules, NamedTransform};
use ocio::*;

const VIRTUAL_CONFIG: &str = r#"ocio_profile_version: 2

environment:
  {}
search_path: ""
strictparsing: true
luma: [0.2126, 0.7152, 0.0722]

roles:
  default: raw

file_rules:
  - !<Rule> {name: Default, colorspace: default}

shared_views:
  - !<View> {name: sview1, colorspace: raw}
  - !<View> {name: sview2, colorspace: raw}

displays:
  sRGB:
    - !<View> {name: Raw, colorspace: raw}
    - !<View> {name: view, view_transform: display_vt, display_colorspace: display_cs}
    - !<Views> [sview1]

virtual_display:
  - !<View> {name: Raw, colorspace: raw}
  - !<View> {name: Film, view_transform: display_vt, display_colorspace: <USE_DISPLAY_NAME>}
  - !<Views> [sview2]

active_displays: []
active_views: []

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
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    allocation: uniform
    to_display_reference: !<CDLTransform> {sat: 1.5}

colorspaces:
  - !<ColorSpace>
    name: raw
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    isdata: true
    allocation: uniform
"#;

#[test]
fn config_virtual_display() {
    let _lock = env_lock();
    // Step 1 & 2 - Validate, load and save a config containing a virtual display.
    let config = check_roundtrip(VIRTUAL_CONFIG);

    assert_eq!(config.num_views("sRGB"), 3);
    assert_eq!(config.num_views_by_type(ViewType::DisplayDefined, "sRGB"), 2);
    assert_eq!(config.num_views_by_type(ViewType::Shared, "sRGB"), 1);
    assert_eq!(config.view_by_type(ViewType::Shared, "sRGB", 0), "sview1");
    assert!(config.has_view("sRGB", "sview1"));
    assert!(config.is_view_shared("sRGB", "sview1"));
    assert!(!config.is_view_shared("sRGB", ""));
    assert_eq!(config.display_view_color_space_name("sRGB", "sview1"), "raw");
    assert_eq!(config.virtual_display_view_color_space_name("sview2"), "raw");

    // Step 3 - Validate the virtual display information.
    let mut cfg = config.create_editable_copy();
    assert!(Config::are_views_equal(&config, &cfg, "sRGB", "sview1"));
    assert!(Config::are_views_equal(&config, &cfg, "sRGB", "Raw"));
    assert!(Config::are_views_equal(&config, &cfg, "sRGB", "view"));

    assert_eq!(cfg.virtual_display_num_views(ViewType::DisplayDefined), 2);
    let name = cfg.virtual_display_view(ViewType::DisplayDefined, 0).to_string();
    assert!(cfg.has_virtual_view(&name));
    assert_eq!(name, "Raw");
    assert_eq!(cfg.virtual_display_view_transform_name(&name), "");
    assert_eq!(cfg.virtual_display_view_color_space_name(&name), "raw");
    assert_eq!(cfg.virtual_display_view_looks(&name), "");
    assert_eq!(cfg.virtual_display_view_rule(&name), "");
    assert_eq!(cfg.virtual_display_view_description(&name), "");
    assert!(Config::are_virtual_views_equal(&config, &cfg, &name));

    let name = cfg.virtual_display_view(ViewType::DisplayDefined, 1).to_string();
    assert_eq!(name, "Film");
    assert_eq!(cfg.virtual_display_view_transform_name(&name), "display_vt");
    assert_eq!(cfg.virtual_display_view_color_space_name(&name), "<USE_DISPLAY_NAME>");
    assert_eq!(cfg.virtual_display_view_looks(&name), "");
    assert_eq!(cfg.virtual_display_view_rule(&name), "");
    assert_eq!(cfg.virtual_display_view_description(&name), "");

    assert_eq!(cfg.virtual_display_num_views(ViewType::Shared), 1);
    assert_eq!(cfg.virtual_display_view(ViewType::Shared, 0), "sview2");
    assert_eq!(cfg.virtual_display_view_color_space_name("sview2"), "raw");
    assert!(cfg.has_virtual_view("sview2"));
    assert!(cfg.is_virtual_view_shared("sview2"));
    assert!(Config::are_virtual_views_equal(&config, &cfg, "sview2"));
    assert!(!cfg.is_virtual_view_shared(""));

    // Remove a view from the Virtual Display.
    cfg.remove_virtual_display_view("Raw");
    assert!(!Config::are_virtual_views_equal(&config, &cfg, "Raw"));
    assert!(!cfg.has_virtual_view("Raw"));
    assert_eq!(cfg.virtual_display_num_views(ViewType::DisplayDefined), 1);
    assert_eq!(cfg.virtual_display_view(ViewType::DisplayDefined, 0), "Film");
    assert_eq!(cfg.virtual_display_num_views(ViewType::Shared), 1);
    assert_eq!(cfg.virtual_display_view(ViewType::Shared, 0), "sview2");

    // Remove a shared view from the Virtual Display.
    cfg.remove_virtual_display_view("sview2");
    assert_eq!(cfg.virtual_display_num_views(ViewType::DisplayDefined), 1);
    assert_eq!(cfg.virtual_display_num_views(ViewType::Shared), 0);
    assert!(!cfg.has_virtual_view("sview2"));
    assert!(!cfg.is_virtual_view_shared("sview2"));
    assert!(!Config::are_virtual_views_equal(&config, &cfg, "sview2"));
    {
        let config2 = Config::create_from_str(&cfg.serialize().unwrap()).unwrap();
        assert_eq!(config2.virtual_display_num_views(ViewType::DisplayDefined), 1);
        assert_eq!(config2.virtual_display_num_views(ViewType::Shared), 0);
    }

    cfg.add_virtual_display_shared_view("sview2").unwrap();
    assert_eq!(cfg.virtual_display_num_views(ViewType::DisplayDefined), 1);
    assert_eq!(cfg.virtual_display_num_views(ViewType::Shared), 1);
    assert!(cfg.is_virtual_view_shared("sview2"));

    // Remove the Virtual Display.
    cfg.clear_virtual_display();
    assert_eq!(cfg.virtual_display_num_views(ViewType::DisplayDefined), 0);
    assert_eq!(cfg.virtual_display_num_views(ViewType::Shared), 0);
    assert!(!cfg.is_virtual_view_shared("sview2"));
    {
        let config2 = Config::create_from_str(&cfg.serialize().unwrap()).unwrap();
        assert_eq!(config2.virtual_display_num_views(ViewType::DisplayDefined), 0);
        assert_eq!(config2.virtual_display_num_views(ViewType::Shared), 0);
    }

    // Step 4 - Instantiate a (display, view) using a custom ICC profile (the
    // Linux behavior of the C++ test: there are no system monitors).
    let mut cfg = config.create_editable_copy();
    cfg.instantiate_display_from_icc_profile(&data_file("icc-test-1.icc")).unwrap();
    assert_eq!(cfg.num_displays(), 1 + config.num_displays());
    let num_cs = config.num_color_spaces_filtered(SearchReferenceSpaceType::Display, ColorSpaceVisibility::Active);
    assert_eq!(
        cfg.num_color_spaces_filtered(SearchReferenceSpaceType::Display, ColorSpaceVisibility::Active),
        1 + num_cs
    );
    let custom = cfg.display(config.num_displays());
    assert_eq!(cfg.num_views(&custom), 3);
    assert_eq!(cfg.num_views_by_type(ViewType::DisplayDefined, &custom), 2);
    assert_eq!(cfg.num_views_by_type(ViewType::Shared, &custom), 1);

    // There is no uniform way to retrieve the monitor information.
    assert!(cfg.instantiate_display_from_monitor_name("monitor").is_err());
}

#[test]
fn config_virtual_display_with_active_displays() {
    const CONFIG: &str = r#"ocio_profile_version: 2

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

virtual_display:
  - !<View> {name: Raw, colorspace: raw}
  - !<Views> [sview1]

active_displays: [sRGB]
active_views: [view]

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
    let _lock = env_lock();
    let config = Config::create_from_str(CONFIG).unwrap();
    config.validate().unwrap();
    assert_eq!(config.num_displays(), 1);
    assert_eq!(config.num_views("sRGB"), 1);
}

#[test]
fn config_virtual_display_v2_only() {
    const CONFIG: &str = r#"ocio_profile_version: 1

roles:
  default: raw

displays:
  sRGB:
    - !<View> {name: Raw, colorspace: raw}

virtual_display:
  - !<View> {name: Raw, colorspace: raw}

colorspaces:
  - !<ColorSpace>
    name: raw
"#;
    let _lock = env_lock();
    assert_err!(Config::create_from_str(CONFIG), "Only version 2 (or higher) can have a virtual display.");

    let mut cfg = Config::create_raw().create_editable_copy();
    cfg.add_virtual_display_shared_view("sview").unwrap();
    cfg.set_major_version(1).unwrap();
    cfg.set_file_rules(&FileRules::new());
    assert_err!(cfg.validate(), "Only version 2 (or higher) can have a virtual display.");
    assert_err!(cfg.serialize(), "Only version 2 (or higher) can have a virtual display.");
}

#[test]
fn config_virtual_display_exceptions() {
    const CONFIG: &str = r#"ocio_profile_version: 2

roles:
  default: raw

file_rules:
  - !<Rule> {name: Default, colorspace: default}

shared_views:
  - !<View> {name: sview1, colorspace: raw}

displays:
  Raw:
    - !<View> {name: Raw, colorspace: raw}

virtual_display:
  - !<View> {name: Raw, colorspace: raw}
  - !<Views> [sview1]

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
    let _lock = env_lock();
    let mut cfg = Config::create_from_str(CONFIG).unwrap().create_editable_copy();
    cfg.validate().unwrap();

    assert_err!(
        cfg.add_virtual_display_shared_view("sview1"),
        "Shared view could not be added to virtual_display: There is already a shared view named 'sview1'."
    );
    cfg.add_virtual_display_shared_view("sview2").unwrap();
    assert_err!(
        cfg.validate(),
        "The display 'virtual_display' contains a shared view 'sview2' that is not defined."
    );
    cfg.remove_virtual_display_view("sview2");
    cfg.validate().unwrap();

    assert_err!(
        cfg.add_virtual_display_view("Raw", "", "raw", "", "", ""),
        "View could not be added to virtual_display in config: View 'Raw' already exists."
    );
    cfg.add_virtual_display_view("Raw1", "", "raw1", "", "", "").unwrap();
    assert_err!(
        cfg.validate(),
        "Display 'virtual_display' has a view 'Raw1' that refers to a color space or a named transform, 'raw1', which is not defined."
    );
    cfg.remove_virtual_display_view("Raw1");
    cfg.validate().unwrap();

    cfg.add_virtual_display_view("Raw1", "", "raw", "look", "", "").unwrap();
    assert_err!(
        cfg.validate(),
        "Display 'virtual_display' has a view 'Raw1' refers to a look, 'look', which is not defined."
    );
}

#[test]
fn config_description_and_name() {
    let _lock = env_lock();
    let mut cfg = Config::create_raw().create_editable_copy();
    let header = "ocio_profile_version: 2\n\nenvironment:\n  {}\nsearch_path: \"\"\nstrictparsing: false\nluma: [0.2126, 0.7152, 0.0722]\n";
    let body = r#"
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
    assert_eq!(cfg.serialize().unwrap(), format!("{header}{body}"));

    cfg.set_description("single line description");
    cfg.set_name("Test config name");
    assert_eq!(cfg.create_editable_copy().name(), "Test config name");
    assert_eq!(
        cfg.serialize().unwrap(),
        format!("{header}name: Test config name\ndescription: single line description\n{body}")
    );

    cfg.set_description("multi line description\n\nother line");
    cfg.set_name("");
    assert_eq!(
        cfg.serialize().unwrap(),
        format!("{header}description: |\n  multi line description\n\n  other line\n{body}")
    );
}

#[test]
fn config_alias_validation() {
    let _lock = env_lock();
    let mut cfg = Config::create_raw().create_editable_copy();
    let mut cs = ColorSpace::default();
    cs.set_name("colorspace1");
    cfg.add_color_space(&cs).unwrap();
    cs.set_name("colorspace2");
    cfg.add_color_space(&cs).unwrap();
    cfg.validate().unwrap();
    cs.set_name("colorspace3");
    cs.add_alias("colorspace1");
    assert_err!(
        cfg.add_color_space(&cs),
        "Cannot add 'colorspace3' color space, it has 'colorspace1' alias and existing color space, 'colorspace1' is using the same alias"
    );
    cs.remove_alias("colorspace1");

    cfg.set_role("alias", Some("colorspace2")).unwrap();
    cs.add_alias("alias");
    assert_err!(
        cfg.add_color_space(&cs),
        "Cannot add 'colorspace3' color space, it has an alias 'alias' and there is already a role with this name"
    );
    cs.remove_alias("alias");
    cs.add_alias("test%test");
    assert_err!(
        cfg.add_color_space(&cs),
        "Cannot add 'colorspace3' color space, it has an alias 'test%test' that cannot contain a context variable reserved token i.e. % or $"
    );

    cs.remove_alias("test%test");
    cs.add_alias("namedtransform");
    cfg.add_color_space(&cs).unwrap();
    let mut nt = NamedTransform::new();
    nt.set_transform(Some(MatrixTransform::default().into()), TransformDirection::Forward);
    nt.set_name("namedtransform");
    assert_err!(
        cfg.add_named_transform(&nt),
        "Cannot add 'namedtransform' named transform, there is already a color space using this name as a name or as an alias: 'colorspace3"
    );
    nt.set_name("nt");
    cfg.add_named_transform(&nt).unwrap();
    cfg.validate().unwrap();

    nt.add_alias("namedtransform");
    assert_err!(
        cfg.add_named_transform(&nt),
        "Cannot add 'nt' named transform, it has an alias 'namedtransform' and there is already a color space using this name as a name or as an alias: 'colorspace3'"
    );
    nt.remove_alias("namedtransform");
    nt.add_alias("colorspace3");
    assert_err!(
        cfg.add_named_transform(&nt),
        "Cannot add 'nt' named transform, it has an alias 'colorspace3' and there is already a color space using this name as a name or as an alias: 'colorspace3'"
    );
    nt.remove_alias("colorspace3");
    nt.add_alias("alias");
    assert_err!(
        cfg.add_named_transform(&nt),
        "Cannot add 'nt' named transform, it has an alias 'alias' and there is already a role with this name"
    );
    nt.remove_alias("alias");
    nt.add_alias("test%test");
    assert_err!(
        cfg.add_named_transform(&nt),
        "Cannot add 'nt' named transform, it has an alias 'test%test' that cannot contain a context variable reserved token i.e. % or $"
    );
}

fn kinds(p: &Processor) -> Vec<&'static str> {
    p.create_group_transform()
        .transforms
        .iter()
        .map(|t| match t {
            Transform::Matrix(_) => "matrix",
            Transform::FixedFunction(_) => "ff",
            Transform::Exponent(_) => "exponent",
            _ => "other",
        })
        .collect()
}

#[test]
#[ignore = "needs-merge"]
fn config_get_processor_alias() {
    let _lock = env_lock();
    let mut config = Config::create_raw().create_editable_copy();
    let mut src = ColorSpace::new(ReferenceSpaceType::Scene);
    src.set_name("source");
    src.set_transform(
        Some(MatrixTransform { offset: [0.0, 0.1, 0.2, 0.0], ..Default::default() }.into()),
        ColorSpaceDirection::ToReference,
    );
    src.add_alias("alias source");
    src.add_alias("src");
    config.add_color_space(&src).unwrap();
    let mut dst = ColorSpace::new(ReferenceSpaceType::Scene);
    dst.set_name("destination");
    dst.set_transform(
        Some(FixedFunctionTransform::new(FixedFunctionStyle::AcesGlow03, &[]).into()),
        ColorSpaceDirection::FromReference,
    );
    dst.add_alias("alias destination");
    dst.add_alias("dst");
    config.add_color_space(&dst).unwrap();
    config.validate().unwrap();

    let ref_proc = config.get_processor("source", "destination").unwrap();
    assert_eq!(kinds(&ref_proc), ["matrix", "ff"]);
    let with_alias = config.get_processor("alias source", "destination").unwrap();
    assert_eq!(with_alias.cache_id(), ref_proc.cache_id());

    config.set_processor_cache_flags(ProcessorCacheFlags::OFF);
    assert_eq!(kinds(&config.get_processor("alias source", "destination").unwrap()), ["matrix", "ff"]);
    assert_eq!(kinds(&config.get_processor("alias source", "dst").unwrap()), ["matrix", "ff"]);

    let mut nt = NamedTransform::new();
    nt.set_name("named_transform");
    nt.add_alias("nt");
    nt.set_transform(Some(ExponentTransform::default().into()), TransformDirection::Forward);
    config.add_named_transform(&nt).unwrap();
    assert_eq!(kinds(&config.get_processor("nt", "dst").unwrap()), ["exponent"]);

    config.add_display_view("display", "view", "alias destination", "").unwrap();
    let p = config
        .get_display_view_processor_dir("alias source", "display", "view", TransformDirection::Forward)
        .unwrap();
    assert_eq!(kinds(&p), ["matrix", "ff"]);
}

#[test]
#[ignore = "needs-merge"]
fn config_look_is_noop() {
    let _lock = env_lock();
    use TransformDirection::{Forward, Inverse};
    const CONFIG1: &str = r#"ocio_profile_version: 1
roles:
  scene_linear: cs

displays:
  disp1:
    - !<View>
      name: view1
      colorspace: cs
      looks: cdl

looks:
  - !<Look>
    name: cdl
    process_space: cs
    transform: !<CDLTransform> {}

colorspaces:
  - !<ColorSpace>
    name: cs
"#;
    let config = Config::create_from_str(CONFIG1).unwrap();
    config.validate().unwrap();
    for dir in [Forward, Inverse] {
        let p = config.get_display_view_processor_dir("cs", "disp1", "view1", dir).unwrap();
        assert!(p.is_no_op());
    }

    let config2 = CONFIG1.replace("process_space: cs\n", "process_space: cs1\n")
        + "  - !<ColorSpace>\n    name: cs1\n    from_reference: !<CDLTransform> {offset: [0.3, 0.3, 0.3]}\n";
    let config = Config::create_from_str(&config2).unwrap();
    config.validate().unwrap();
    for dir in [Forward, Inverse] {
        let p = config.get_display_view_processor_dir("cs", "disp1", "view1", dir).unwrap();
        assert!(!p.is_no_op());
        assert!(p.optimized(OptimizationFlags::DEFAULT).is_no_op());
    }
}

fn look_fallback_config() -> String {
    format!(
        "ocio_profile_version: 1\n\nsearch_path: {}\n\nroles:\n  scene_linear: cs\n\ndisplays:\n  disp1:\n    - !<View>\n      name: view1\n      colorspace: cs\n      looks: missing_file_look | \n\nlooks:\n  - !<Look>\n    name: missing_file_look\n    process_space: cs\n    transform: !<FileTransform> {{src: \"${{LOOK_CDL}}.cc\"}}\n\ncolorspaces:\n  - !<ColorSpace>\n    name: cs\n\n",
        data_file("")
    )
}

#[test]
#[ignore = "needs-merge"]
fn config_look_fallback() {
    let _lock = env_lock();
    let _g = EnvGuard::set("LOOK_CDL", Some("cdl_test1"));
    let config = Config::create_from_str(&look_fallback_config()).unwrap();
    config.validate().unwrap();
    let p = config
        .get_display_view_processor_dir("cs", "disp1", "view1", TransformDirection::Forward)
        .unwrap();
    assert!(!p.is_no_op());

    let _g2 = EnvGuard::set("LOOK_CDL", None);
    let config = Config::create_from_str(&look_fallback_config()).unwrap();
    let p = config
        .get_display_view_processor_dir("cs", "disp1", "view1", TransformDirection::Forward)
        .unwrap();
    assert!(p.is_no_op());
}

#[test]
fn config_create_from_archive() {
    let _lock = env_lock();
    for name in ["context_test1_windows.ocioz", "context_test1_linux.ocioz"] {
        let config = Config::create_from_file(&data_file(&format!("configs/context_test1/{name}"))).unwrap();
        config.validate().unwrap();
        assert_eq!(config.num_color_spaces(), 13);
    }
    for name in ["empty.ocioz", "missing_config.ocioz"] {
        assert_err!(
            Config::create_from_file(&data_file(&format!("configs/ocioz_archive_configs/{name}"))),
            "Loading the OCIO profile failed. At line 0, '' parsing failed: The specified OCIO configuration file from Archive/ConfigIOProxy does not appear to have a valid version <null>"
        );
    }
    let config =
        Config::create_from_file(&data_file("configs/ocioz_archive_configs/config_missing_luts.ocioz")).unwrap();
    // validate() does not try to fetch the LUT files.
    config.validate().unwrap();
}

#[test]
#[ignore = "needs-merge"]
fn config_create_from_archive_processors() {
    let _lock = env_lock();
    for name in ["context_test1_windows.ocioz", "context_test1_linux.ocioz"] {
        let config = Config::create_from_file(&data_file(&format!("configs/context_test1/{name}"))).unwrap();
        let p = config.get_processor("plain_lut1_cs", "shot1_lut1_cs").unwrap();
        p.default_cpu_processor();
    }
    let config =
        Config::create_from_file(&data_file("configs/ocioz_archive_configs/config_missing_luts.ocioz")).unwrap();
    // Deviation: the archive LUTs are extracted to a temporary directory, so
    // the attempted paths are absolute.
    assert_err!(
        config.get_processor("plain_lut11_cs", "shot1_lut11_cs"),
        "The specified file reference 'lut11.clf' could not be located. The following attempts were made:"
    );
}

#[test]
fn config_interchange_attributes() {
    let _lock = env_lock();
    let end = "\nview_transforms:\n  - !<ViewTransform>\n    name: vt1\n    from_scene_reference: !<RangeTransform> {min_in_value: 0., min_out_value: 0.}";
    let s = format!("{}{end}", profile_start_v(2, 5));
    let mut config = Config::create_from_str(&s).unwrap().create_editable_copy();
    config.validate().unwrap();

    // Color space.
    {
        let mut cs = config.get_color_space("log").unwrap().clone();
        cs.set_interchange_attribute("amf_transform_ids", "sample amf id").unwrap();
        config.add_color_space(&cs).unwrap();
        config.validate().unwrap();
        let out = config.serialize().unwrap();
        assert!(out.contains("amf_transform_ids: sample amf id"));
        let cfg2 = Config::create_from_str(&out).unwrap();
        assert_eq!(
            cfg2.get_color_space("log").unwrap().interchange_attribute("amf_transform_ids").unwrap(),
            "sample amf id"
        );
        config.set_version(2, 4).unwrap();
        assert_err!(
            config.validate(),
            "Config failed validation. The color space 'log' has non-empty interchange attributes and config version is less than 2.5."
        );
        cs.set_interchange_attribute("amf_transform_ids", "").unwrap();
        config.add_color_space(&cs).unwrap();
        config.validate().unwrap();
        config.set_version(2, 5).unwrap();
    }
    // View transform.
    {
        let mut vt = config.view_transform("vt1").unwrap().clone();
        vt.set_interchange_attribute("amf_transform_ids", "sample amf id").unwrap();
        config.add_view_transform(&vt).unwrap();
        config.validate().unwrap();
        assert_err!(
            vt.set_interchange_attribute("icc_profile_name", "some icc profile"),
            "Unknown attribute name 'icc_profile_name'."
        );
        let out = config.serialize().unwrap();
        assert!(out.contains("amf_transform_ids: sample amf id"));
        let cfg2 = Config::create_from_str(&out).unwrap();
        assert_eq!(
            cfg2.view_transform("vt1").unwrap().interchange_attribute("amf_transform_ids").unwrap(),
            "sample amf id"
        );
        config.set_version(2, 4).unwrap();
        assert_err!(
            config.validate(),
            "Config failed validation. The view transform 'vt1' has non-empty interchange attributes and config version is less than 2.5."
        );
        vt.set_interchange_attribute("amf_transform_ids", "").unwrap();
        config.add_view_transform(&vt).unwrap();
        config.validate().unwrap();
        config.set_version(2, 5).unwrap();
    }
    // Look.
    {
        let mut lk = config.look("beauty").unwrap().clone();
        lk.set_interchange_attribute("amf_transform_ids", "sample amf id").unwrap();
        config.add_look(&lk).unwrap();
        config.validate().unwrap();
        let out = config.serialize().unwrap();
        assert!(out.contains("amf_transform_ids: sample amf id"));
        let cfg2 = Config::create_from_str(&out).unwrap();
        assert_eq!(
            cfg2.look("beauty").unwrap().interchange_attribute("amf_transform_ids").unwrap(),
            "sample amf id"
        );
        config.set_version(2, 4).unwrap();
        assert_err!(
            config.validate(),
            "Config failed validation. The look 'beauty' has non-empty interchange attributes and config version is less than 2.5."
        );
        lk.set_interchange_attribute("amf_transform_ids", "").unwrap();
        config.add_look(&lk).unwrap();
        config.validate().unwrap();
        config.set_version(2, 5).unwrap();
    }
}

#[test]
fn config_cyclic_color_space_transform() {
    let _lock = env_lock();
    const CONFIG: &str = "ocio_profile_version: 2\nroles:\n  default: cs0\ncolorspaces:\n  - !<ColorSpace>\n    name: cs0\n    isdata: true\n  - !<ColorSpace>\n    name: cs1\n    from_scene_reference: !<ColorSpaceTransform> {src: cs0, dst: cs1}\n";
    let config = Config::create_from_str(CONFIG).unwrap();
    assert_err!(config.get_processor("cs0", "cs1"), "Cycle detected");
}

#[test]
fn config_cyclic_color_space_two_step() {
    let _lock = env_lock();
    const CONFIG: &str = "ocio_profile_version: 2\nroles:\n  default: cs0\ncolorspaces:\n  - !<ColorSpace>\n    name: cs0\n    isdata: true\n  - !<ColorSpace>\n    name: cs1\n    from_scene_reference: !<ColorSpaceTransform> {src: cs0, dst: cs2}\n  - !<ColorSpace>\n    name: cs2\n    from_scene_reference: !<ColorSpaceTransform> {src: cs0, dst: cs1}\n";
    let config = Config::create_from_str(CONFIG).unwrap();
    assert_err!(config.get_processor("cs0", "cs1"), "Cycle detected");
}

#[test]
fn config_cyclic_look_transform() {
    let _lock = env_lock();
    const CONFIG: &str = "ocio_profile_version: 2\nroles:\n  default: cs0\nlooks:\n  - !<Look>\n    name: lookA\n    process_space: cs0\n    transform: !<LookTransform> {src: cs0, dst: cs0, looks: lookA}\ncolorspaces:\n  - !<ColorSpace>\n    name: cs0\n  - !<ColorSpace>\n    name: cs1\n    from_scene_reference: !<LookTransform> {src: cs0, dst: cs0, looks: lookA}\n";
    let config = Config::create_from_str(CONFIG).unwrap();
    assert_err!(config.get_processor("cs0", "cs1"), "Cycle detected");
}

#[test]
fn config_cyclic_color_space_linearity_check() {
    let _lock = env_lock();
    const CONFIG: &str = "ocio_profile_version: 2\nroles:\n  default: cs0\ncolorspaces:\n  - !<ColorSpace>\n    name: cs0\n  - !<ColorSpace>\n    name: cs1\n    from_scene_reference: !<ColorSpaceTransform> {src: cs0, dst: cs1}\n";
    let config = Config::create_from_str(CONFIG).unwrap();
    assert_err!(config.is_color_space_linear("cs1", ReferenceSpaceType::Scene), "Cycle detected");
}

#[test]
fn config_cyclic_display_view_transform() {
    let _lock = env_lock();
    const CONFIG: &str = "ocio_profile_version: 2\nroles:\n  default: cs0\ndisplays:\n  D1:\n    - !<View> {name: V1, colorspace: cs_loop}\ncolorspaces:\n  - !<ColorSpace>\n    name: cs0\n  - !<ColorSpace>\n    name: cs_loop\n    from_scene_reference: !<DisplayViewTransform> {src: cs0, display: D1, view: V1}\n";
    let config = Config::create_from_str(CONFIG).unwrap();
    assert_err!(config.get_processor("cs0", "cs_loop"), "Cycle detected");
}
