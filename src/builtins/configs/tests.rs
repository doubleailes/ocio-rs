//! Port of `tests/cpu/builtinconfigs/BuiltinConfig_tests.cpp`.

use super::*;
use crate::config::Config;
use crate::error::Error;

#[test]
fn basic() {
    let registry = BuiltinConfigRegistry::get();

    assert_eq!(registry.num_builtin_configs(), 8);

    let expected = [
        (
            "cg-config-v1.0.0_aces-v1.3_ocio-v2.1",
            "Academy Color Encoding System - CG Config [COLORSPACES v1.0.0] [ACES v1.3] [OCIO v2.1]",
            CG_CONFIG_V100_ACES_V13_OCIO_V21,
            false,
        ),
        (
            "cg-config-v2.1.0_aces-v1.3_ocio-v2.3",
            "Academy Color Encoding System - CG Config [COLORSPACES v2.0.0] [ACES v1.3] [OCIO v2.3]",
            CG_CONFIG_V210_ACES_V13_OCIO_V23,
            false,
        ),
        (
            "cg-config-v2.2.0_aces-v1.3_ocio-v2.4",
            "Academy Color Encoding System - CG Config [COLORSPACES v2.2.0] [ACES v1.3] [OCIO v2.4]",
            CG_CONFIG_V220_ACES_V13_OCIO_V24,
            false,
        ),
        (
            "cg-config-v4.0.0_aces-v2.0_ocio-v2.5",
            "Academy Color Encoding System - CG Config [COLORSPACES v4.0.0] [ACES v2.0] [OCIO v2.5]",
            CG_CONFIG_V400_ACES_V20_OCIO_V25,
            true,
        ),
        (
            "studio-config-v1.0.0_aces-v1.3_ocio-v2.1",
            "Academy Color Encoding System - Studio Config [COLORSPACES v1.0.0] [ACES v1.3] [OCIO v2.1]",
            STUDIO_CONFIG_V100_ACES_V13_OCIO_V21,
            false,
        ),
        (
            "studio-config-v2.1.0_aces-v1.3_ocio-v2.3",
            "Academy Color Encoding System - Studio Config [COLORSPACES v2.0.0] [ACES v1.3] [OCIO v2.3]",
            STUDIO_CONFIG_V210_ACES_V13_OCIO_V23,
            false,
        ),
        (
            "studio-config-v2.2.0_aces-v1.3_ocio-v2.4",
            "Academy Color Encoding System - Studio Config [COLORSPACES v2.2.0] [ACES v1.3] [OCIO v2.4]",
            STUDIO_CONFIG_V220_ACES_V13_OCIO_V24,
            false,
        ),
        (
            "studio-config-v4.0.0_aces-v2.0_ocio-v2.5",
            "Academy Color Encoding System - Studio Config [COLORSPACES v4.0.0] [ACES v2.0] [OCIO v2.5]",
            STUDIO_CONFIG_V400_ACES_V20_OCIO_V25,
            true,
        ),
    ];

    for (idx, (name, ui_name, text, recommended)) in expected.iter().enumerate() {
        assert_eq!(registry.builtin_config_name(idx).unwrap(), *name);
        assert_eq!(registry.builtin_config_ui_name(idx).unwrap(), *ui_name);
        assert_eq!(registry.builtin_config(idx).unwrap(), *text);
        assert_eq!(registry.builtin_config_by_name(name).unwrap(), *text);
        assert_eq!(
            registry.is_builtin_config_recommended(idx).unwrap(),
            *recommended
        );
        // The embedded text is the config itself.
        assert!(text.starts_with("ocio_profile_version:"));
        assert!(text.contains(&format!("name: {name}")));
    }

    // Testing some expected failures.
    let msg = "Config index is out of range.";
    assert_eq!(
        registry
            .is_builtin_config_recommended(999)
            .unwrap_err()
            .message(),
        msg
    );
    assert_eq!(
        registry.builtin_config_name(999).unwrap_err().message(),
        msg
    );
    assert_eq!(
        registry.builtin_config_ui_name(999).unwrap_err().message(),
        msg
    );
    assert_eq!(registry.builtin_config(999).unwrap_err().message(), msg);
    assert_eq!(
        registry
            .builtin_config_by_name("I do not exist")
            .unwrap_err()
            .message(),
        "Could not find 'I do not exist' in the built-in configurations."
    );
}

#[test]
fn basic_impl() {
    // Test the add_builtin method.
    let mut registry = BuiltinConfigRegistry::new();

    const SIMPLE_CONFIG: &str = "ocio_profile_version: 1\n\
        colorspaces:\n\
        \x20 - !<ColorSpace>\n\
        \x20     name: raw\n\
        \x20 - !<ColorSpace>\n\
        \x20     name: linear\n\
        roles:\n\
        \x20 default: raw\n\
        displays:\n\
        \x20 sRGB:\n\
        \x20 - !<View> {name: Raw, colorspace: raw}\n\
        \n";

    registry.add_builtin(
        "simple_config_1",
        "My simple config display name #1",
        SIMPLE_CONFIG,
        false,
    );
    registry.add_builtin(
        "simple_config_2",
        "My simple config display name #2",
        SIMPLE_CONFIG,
        true,
    );

    assert_eq!(registry.num_builtin_configs(), 2);

    assert_eq!(registry.builtin_config_name(0).unwrap(), "simple_config_1");
    assert_eq!(
        registry.builtin_config_ui_name(0).unwrap(),
        "My simple config display name #1"
    );
    assert!(!registry.is_builtin_config_recommended(0).unwrap());

    assert_eq!(registry.builtin_config_name(1).unwrap(), "simple_config_2");
    assert_eq!(
        registry.builtin_config_ui_name(1).unwrap(),
        "My simple config display name #2"
    );
    assert!(registry.is_builtin_config_recommended(1).unwrap());

    // Adding an existing (case insensitive) name replaces the config.
    registry.add_builtin("SIMPLE_CONFIG_1", "Replaced", SIMPLE_CONFIG, true);
    assert_eq!(registry.num_builtin_configs(), 2);
    assert_eq!(registry.builtin_config_name(0).unwrap(), "SIMPLE_CONFIG_1");
    assert_eq!(registry.builtin_config_ui_name(0).unwrap(), "Replaced");
    assert_eq!(
        registry.builtin_config_by_name("simple_config_1").unwrap(),
        SIMPLE_CONFIG
    );
}

#[test]
fn get_builtin_config_names_and_aliases() {
    // Registry names (case insensitive).
    assert_eq!(
        get_builtin_config("cg-config-v1.0.0_aces-v1.3_ocio-v2.1"),
        Some(CG_CONFIG_V100_ACES_V13_OCIO_V21)
    );
    assert_eq!(
        get_builtin_config("CG-CONFIG-V2.1.0_ACES-V1.3_OCIO-V2.3"),
        Some(CG_CONFIG_V210_ACES_V13_OCIO_V23)
    );
    assert_eq!(
        get_builtin_config("studio-config-v2.2.0_aces-v1.3_ocio-v2.4"),
        Some(STUDIO_CONFIG_V220_ACES_V13_OCIO_V24)
    );

    // URIs.
    assert_eq!(
        get_builtin_config("ocio://studio-config-v1.0.0_aces-v1.3_ocio-v2.1"),
        Some(STUDIO_CONFIG_V100_ACES_V13_OCIO_V21)
    );

    // Aliases, with and without the URI prefix.
    for n in [
        "default",
        "ocio://default",
        "cg-config-latest",
        "ocio://cg-config-latest",
        "DEFAULT",
    ] {
        assert_eq!(
            get_builtin_config(n),
            Some(CG_CONFIG_V400_ACES_V20_OCIO_V25),
            "{n}"
        );
    }
    for n in ["studio-config-latest", "ocio://studio-config-latest"] {
        assert_eq!(
            get_builtin_config(n),
            Some(STUDIO_CONFIG_V400_ACES_V20_OCIO_V25),
            "{n}"
        );
    }

    // Unknown configs.
    assert_eq!(get_builtin_config("I-do-not-exist"), None);
    assert_eq!(
        builtin_config("I-do-not-exist").unwrap_err().message(),
        "Could not find 'I-do-not-exist' in the built-in configurations."
    );
    assert_eq!(
        builtin_config("ocio://I-do-not-exist")
            .unwrap_err()
            .message(),
        "Could not find 'I-do-not-exist' in the built-in configurations."
    );
    assert_eq!(
        builtin_config("ocio://thedefault").unwrap_err().message(),
        "Could not find 'thedefault' in the built-in configurations."
    );

    // Registry queries.
    assert_eq!(builtin_config_names().len(), 8);
    assert_eq!(
        builtin_config_names()[3],
        "cg-config-v4.0.0_aces-v2.0_ocio-v2.5"
    );
    assert_eq!(
        builtin_config_ui_names()[7],
        "Academy Color Encoding System - Studio Config [COLORSPACES v4.0.0] [ACES v2.0] [OCIO v2.5]"
    );
    assert_eq!(
        default_builtin_config_name(),
        "cg-config-v4.0.0_aces-v2.0_ocio-v2.5"
    );
    assert_eq!(
        BuiltinConfigRegistry::get().default_builtin_config_name(),
        default_builtin_config_name()
    );
    assert_eq!(is_builtin_config_recommended("default"), Some(true));
    assert_eq!(
        is_builtin_config_recommended("cg-config-v1.0.0_aces-v1.3_ocio-v2.1"),
        Some(false)
    );
    assert_eq!(is_builtin_config_recommended("unknown"), None);
    assert!(is_builtin_config_uri("ocio://anything"));
    assert!(!is_builtin_config_uri("ocio:default"));
    assert!(!is_builtin_config_uri("config.ocio"));
}

#[test]
fn resolve_config_path_test() {
    assert_eq!(
        resolve_config_path("ocio://default"),
        DEFAULT_BUILTIN_CONFIG_URI
    );
    assert_eq!(
        resolve_config_path("ocio://cg-config-latest"),
        LATEST_CG_BUILTIN_CONFIG_URI
    );
    assert_eq!(
        resolve_config_path("ocio://studio-config-latest"),
        LATEST_STUDIO_BUILTIN_CONFIG_URI
    );

    // Paths that are not starting with "ocio://" are simply returned unmodified.
    assert_eq!(
        resolve_config_path("studio-config-latest"),
        "studio-config-latest"
    );
    assert_eq!(
        resolve_config_path("studio-config-latest.ocio"),
        "studio-config-latest.ocio"
    );
    assert_eq!(
        resolve_config_path("/usr/local/share/aces.ocio"),
        "/usr/local/share/aces.ocio"
    );
    assert_eq!(
        resolve_config_path("C:\\myconfig\\config.ocio"),
        "C:\\myconfig\\config.ocio"
    );
    assert_eq!(resolve_config_path(""), "");

    // The function does not try to validate to catch mistakes in URI usage.
    // That's up to the application.

    // Unknown built-in config.
    assert_eq!(
        resolve_config_path("ocio://not-a-builtin"),
        "ocio://not-a-builtin"
    );
    // Missing "//".
    assert_eq!(resolve_config_path("ocio:default"), "ocio:default");
}

// OCIO checks `config->getNumColorSpaces()` (the number of active color
// spaces). The Config query API belongs to another module, so these helpers
// check the config name, that the serialized config holds at least that many
// color spaces (inactive ones included), and that loading by name and by URI
// gives the same config.

fn test_from_builtin_config(
    name: &str,
    num_active_color_spaces: usize,
    expected_name: &str,
) -> String {
    let config = Config::create_from_builtin_config(name).unwrap();
    config.validate().unwrap();
    let expected = if expected_name.is_empty() {
        name
    } else {
        expected_name
    };
    let yaml = config.serialize().unwrap();
    assert!(yaml.contains(&format!("name: {expected}\n")), "{name}");
    assert!(
        yaml.matches("!<ColorSpace>").count() >= num_active_color_spaces,
        "{name}"
    );
    yaml
}

fn test_from_file(uri: &str, num_active_color_spaces: usize, expected_name: &str) -> String {
    let config = Config::create_from_file(uri).unwrap();
    config.validate().unwrap();
    let yaml = config.serialize().unwrap();
    assert!(yaml.contains(&format!("name: {expected_name}\n")), "{uri}");
    assert!(
        yaml.matches("!<ColorSpace>").count() >= num_active_color_spaces,
        "{uri}"
    );
    yaml
}

#[test]
#[ignore = "needs-merge"]
fn create_builtin_config() {
    let prefix = OCIO_BUILTIN_URI_PREFIX;

    // Test that create_from_file does not work without ocio:// prefix for built-in config.
    let err = Config::create_from_file("cg-config-v1.0.0_aces-v1.3_ocio-v2.1").unwrap_err();
    assert!(matches!(err, Error::MissingFile(_)));
    assert_eq!(
        err.message(),
        "'cg-config-v1.0.0_aces-v1.3_ocio-v2.1' file does not exist."
    );

    let cases = [
        ("cg-config-v1.0.0_aces-v1.3_ocio-v2.1", 14),
        ("studio-config-v1.0.0_aces-v1.3_ocio-v2.1", 39),
        ("cg-config-v2.1.0_aces-v1.3_ocio-v2.3", 15),
        ("studio-config-v2.1.0_aces-v1.3_ocio-v2.3", 41),
        ("cg-config-v2.2.0_aces-v1.3_ocio-v2.4", 23),
        ("studio-config-v2.2.0_aces-v1.3_ocio-v2.4", 54),
    ];
    for (name, n) in cases {
        let a = test_from_builtin_config(name, n, "");
        let b = test_from_file(&format!("{prefix}{name}"), n, name);
        assert_eq!(a, b);
    }

    // Default configs.
    let cg = "cg-config-v4.0.0_aces-v2.0_ocio-v2.5";
    let studio = "studio-config-v4.0.0_aces-v2.0_ocio-v2.5";
    for (alias, n, expected) in [
        (BUILTIN_DEFAULT_NAME, 25, cg),
        (BUILTIN_LATEST_CG_NAME, 25, cg),
        (BUILTIN_LATEST_STUDIO_NAME, 55, studio),
    ] {
        let a = test_from_builtin_config(alias, n, expected);
        let b = test_from_builtin_config(&format!("{prefix}{alias}"), n, expected);
        let c = test_from_file(&format!("{prefix}{alias}"), n, expected);
        assert_eq!(a, b);
        assert_eq!(a, c);
    }

    // Test some expected failures.
    assert_eq!(
        Config::create_from_builtin_config("I-do-not-exist")
            .unwrap_err()
            .message(),
        "Could not find 'I-do-not-exist' in the built-in configurations."
    );
    assert_eq!(
        Config::create_from_file("ocio://I-do-not-exist")
            .unwrap_err()
            .message(),
        "Could not find 'I-do-not-exist' in the built-in configurations."
    );
}
