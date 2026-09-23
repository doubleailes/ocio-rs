//! Port of `FileRules_tests.cpp`.

mod config_common;

use config_common::*;
use ocio::config::logging::LogGuard;
use ocio::config::{ColorSpace, FileRules, DEFAULT_RULE_NAME, FILE_PATH_SEARCH_RULE_NAME};
use ocio::{Config, ROLE_DEFAULT};

fn rules_of(config: &Config) -> &FileRules {
    config.file_rules()
}

#[test]
fn file_rules_config_v1() {
    {
        const CONFIG: &str = "ocio_profile_version: 1\n\
\n\
search_path: \"\"\n\
strictparsing: false\n\
luma: [0.2126, 0.7152, 0.0722]\n\
\n\
roles:\n\
\x20 default: raw\n\
\n\
displays:\n\
\x20 sRGB:\n\
\x20   - !<View> {name: Raw, colorspace: raw}\n\
\n\
active_displays: []\n\
active_views: []\n\
\n\
colorspaces:\n\
\x20 - !<ColorSpace>\n\
\x20   name: raw\n\
\x20   family: \"\"\n\
\x20   equalitygroup: \"\"\n\
\x20   bitdepth: unknown\n\
\x20   isdata: false\n\
\x20   allocation: uniform\n";

        let config = Config::create_from_str(CONFIG).unwrap();
        config.validate().unwrap();
        let fr = rules_of(&config);
        assert_eq!(fr.num_entries(), 2);
        assert_eq!(fr.name(0).unwrap(), FILE_PATH_SEARCH_RULE_NAME);
        assert_eq!(fr.name(1).unwrap(), DEFAULT_RULE_NAME);
        assert_eq!(fr.color_space(1).unwrap(), "default");

        // Check that the file rules are not saved in a v1 config.
        assert_eq!(config.serialize().unwrap(), CONFIG);
    }

    // Fallback 1: no default role, there is a data color space named 'raw'.
    {
        const CONFIG: &str = r#"ocio_profile_version: 1
displays:
  sRGB:
    - !<View> {name: Raw, colorspace: raw}
colorspaces:
  - !<ColorSpace>
    name: cs2
  - !<ColorSpace>
    name: raw
    isdata: true
"#;
        let config = Config::create_from_str(CONFIG).unwrap();
        config.validate().unwrap();
        let fr = rules_of(&config);
        assert_eq!(fr.num_entries(), 2);
        assert_eq!(fr.name(0).unwrap(), FILE_PATH_SEARCH_RULE_NAME);
        assert_eq!(fr.name(1).unwrap(), DEFAULT_RULE_NAME);
        assert_eq!(fr.color_space(1).unwrap(), "raw");
    }

    // Fallback 2: no default role, 'raw' is not data but another one is.
    {
        const CONFIG: &str = r#"ocio_profile_version: 1
displays:
  sRGB:
    - !<View> {name: Raw, colorspace: raw}
colorspaces:
  - !<ColorSpace>
    name: cs2
  - !<ColorSpace>
    name: raw
  - !<ColorSpace>
    name: cs3
    isdata: true
"#;
        let config = Config::create_from_str(CONFIG).unwrap();
        config.validate().unwrap();
        assert_eq!(rules_of(&config).color_space(1).unwrap(), "cs3");
    }

    // Fallback 3: no default role and no data color space.
    {
        const CONFIG: &str = r#"ocio_profile_version: 1
displays:
  sRGB:
    - !<View> {name: Raw, colorspace: raw}
colorspaces:
  - !<ColorSpace>
    name: cs2
  - !<ColorSpace>
    name: raw
"#;
        let config = Config::create_from_str(CONFIG).unwrap();
        config.validate().unwrap();
        assert_eq!(rules_of(&config).num_entries(), 2);
        assert_eq!(rules_of(&config).color_space(1).unwrap(), "cs2");
    }

    // getColorSpaceFromFilepath works with a v1 config.
    {
        const CONFIG: &str = r#"ocio_profile_version: 1
roles:
  default: raw
displays:
  sRGB:
    - !<View> {name: Raw, colorspace: raw}
colorspaces:
  - !<ColorSpace>
    name: cs2
  - !<ColorSpace>
    name: raw
  - !<ColorSpace>
    name: cs3
    isdata: true
"#;
        let config = Config::create_from_str(CONFIG).unwrap();
        config.validate().unwrap();
        assert_eq!(rules_of(&config).color_space(1).unwrap(), "default");

        assert_eq!(config.color_space_from_filepath_with_index("/usr/cs2_file.exr"), ("cs2".into(), 0));
        assert!(!config.filepath_only_matches_default_rule("/usr/cs2_file.exr"));
        assert_eq!(config.color_space_from_filepath_with_index("/usr/cs3/file.exr"), ("cs3".into(), 0));
        assert!(!config.filepath_only_matches_default_rule("/usr/cs3/file.exr"));
        assert_eq!(
            config.color_space_from_filepath_with_index("/usr/cs3/cs2_file.exr"),
            ("cs2".into(), 0)
        );
        assert!(!config.filepath_only_matches_default_rule("/usr/cs3/cs2_file.exr"));
        assert_eq!(config.color_space_from_filepath_with_index("/usr/file.exr"), ("default".into(), 1));
        assert!(config.filepath_only_matches_default_rule("/usr/file.exr"));
    }
}

#[test]
fn file_rules_config_read_only() {
    let config = Config::create_raw();
    let fr = config.file_rules();
    assert_eq!(fr.num_entries(), 1);
    assert_eq!(fr.name(0).unwrap(), DEFAULT_RULE_NAME);
    assert_eq!(fr.index_for_rule(DEFAULT_RULE_NAME).unwrap(), 0);
    assert_eq!(fr.pattern(0).unwrap(), "");
    assert_eq!(fr.extension(0).unwrap(), "");
    assert_eq!(fr.regex(0).unwrap(), "");
    assert_eq!(fr.color_space(0).unwrap(), ROLE_DEFAULT);
    assert_err!(fr.name(1), "rule index '1' invalid. There are only '1' rules.");
    assert_err!(fr.index_for_rule("toto"), "rule name 'toto' not found");
}

#[test]
fn file_rules_config_insert_rule() {
    let config = Config::create_raw().create_editable_copy();
    let mut fr = config.file_rules().clone();
    assert_eq!(fr.num_entries(), 1);
    fr.insert_rule(0, "rule", "raw", "*", "a").unwrap();
    assert_eq!(fr.num_entries(), 2);
    fr.insert_rule_regex(0, "TIFF rule", "raw", r".*\.TIF?F$").unwrap();
    assert_eq!(fr.num_entries(), 3);
    assert_err!(fr.insert_rule(0, "rule", "raw", "*", "b"), "A rule named 'rule' already exists");
    assert_err!(fr.insert_rule(4, "rule2", "raw", "*", "a"), "rule index '4' invalid");
    assert_err!(fr.remove_rule(3), "invalid");
    assert_err!(fr.remove_rule(2), "is the default rule");
    fr.remove_rule(1).unwrap();
    fr.remove_rule(0).unwrap();
    assert_eq!(fr.num_entries(), 1);

    assert_err!(
        fr.insert_rule(0, FILE_PATH_SEARCH_RULE_NAME, "colorspace", "", ""),
        "does not accept any color space"
    );
    assert_err!(
        fr.insert_rule(0, FILE_PATH_SEARCH_RULE_NAME, "", "pattern", ""),
        "do not accept any pattern"
    );
    assert_err!(
        fr.insert_rule(0, FILE_PATH_SEARCH_RULE_NAME, "", "", "extension"),
        "do not accept any extension"
    );
    fr.insert_rule(0, FILE_PATH_SEARCH_RULE_NAME, "", "", "").unwrap();
    assert_err!(
        fr.insert_rule(0, FILE_PATH_SEARCH_RULE_NAME, "", "", "extension"),
        "File rules: A rule named 'ColorSpaceNamePathSearch' already exists."
    );
    assert_err!(
        fr.insert_rule(0, "default", "", "", "extension"),
        "File rules: A rule named 'default' already exists."
    );
    assert_err!(
        fr.insert_rule(0, "defauLT", "", "", "extension"),
        "File rules: A rule named 'defauLT' already exists."
    );
    assert_err!(
        fr.insert_rule(0, "   Default   ", "", "", "extension"),
        "File rules: A rule named 'Default' already exists."
    );

    fr.remove_rule(0).unwrap();
    fr.insert_path_search_rule(0).unwrap();

    assert_err!(fr.insert_rule(0, "", "raw", "*", "a"), "rule should have a non-empty name");
    assert_err!(fr.insert_rule(0, "rule", "raw", "", "a"), "file name pattern is empty");
    assert_err!(fr.insert_rule(0, "rule", "raw", "*", ""), "file extension pattern is empty");
    assert_err!(fr.insert_rule(0, "rule", "raw", "[", "a"), "invalid regular expression");
    assert_err!(
        fr.insert_rule_regex(0, "rule", "raw", "(.*)(\u{8}what"),
        "invalid regular expression"
    );
}

#[test]
fn file_rules_config_rule_customkeys() {
    let config_raw = Config::create_raw();
    let mut fr = config_raw.file_rules().clone();
    assert_eq!(fr.num_entries(), 1);
    fr.insert_rule(0, "rule", "raw", "*", "a").unwrap();
    assert_eq!(fr.num_entries(), 2);
    assert_eq!(fr.num_custom_keys(0).unwrap(), 0);
    assert_eq!(fr.num_custom_keys(1).unwrap(), 0);
    assert_err!(fr.num_custom_keys(2), "rule index '2' invalid");
    assert_err!(fr.custom_key_name(0, 0), "Key index '0' is invalid");
    assert_err!(fr.custom_key_name(1, 0), "Key index '0' is invalid");
    assert_err!(fr.custom_key_value(0, 0), "Key index '0' is invalid");
    assert_err!(fr.custom_key_value(1, 0), "Key index '0' is invalid");
    fr.set_custom_key(0, "key", "val").unwrap();
    fr.set_custom_key(1, "keyDef", "valDef").unwrap();
    assert_err!(fr.set_custom_key(0, "", "val"), "Key has to be a non-empty string");
    assert_eq!(fr.num_custom_keys(0).unwrap(), 1);
    assert_eq!(fr.num_custom_keys(1).unwrap(), 1);
    assert_eq!(fr.custom_key_name(0, 0).unwrap(), "key");
    assert_eq!(fr.custom_key_value(0, 0).unwrap(), "val");
    fr.set_custom_key(0, "key", "").unwrap();
    assert_eq!(fr.num_custom_keys(0).unwrap(), 0);
    fr.set_custom_key(0, "key1", "val").unwrap();
    assert_eq!(fr.num_custom_keys(0).unwrap(), 1);
    fr.set_custom_key(0, "key1", "new val").unwrap();
    assert_eq!(fr.num_custom_keys(0).unwrap(), 1);
    assert_eq!(fr.custom_key_value(0, 0).unwrap(), "new val");
    fr.set_custom_key(0, "key2", "val2").unwrap();
    fr.set_custom_key(0, "key3", "3").unwrap();
    fr.set_custom_key(0, "4", "val4").unwrap();
    assert_eq!(fr.num_custom_keys(0).unwrap(), 4);
    assert_eq!(fr.custom_key_name(0, 1).unwrap(), "key1");
    assert_eq!(fr.custom_key_value(0, 1).unwrap(), "new val");
    assert_eq!(fr.custom_key_name(0, 2).unwrap(), "key2");
    assert_eq!(fr.custom_key_value(0, 2).unwrap(), "val2");
    assert_eq!(fr.custom_key_name(0, 3).unwrap(), "key3");
    assert_eq!(fr.custom_key_value(0, 3).unwrap(), "3");
    assert_eq!(fr.custom_key_name(0, 0).unwrap(), "4");
    assert_eq!(fr.custom_key_value(0, 0).unwrap(), "val4");

    let mut config = config_raw.create_editable_copy();
    config.set_file_rules(&fr);

    let expected = r#"ocio_profile_version: 2

environment:
  {}
search_path: ""
strictparsing: false
luma: [0.2126, 0.7152, 0.0722]

roles:
  default: raw

file_rules:
  - !<Rule> {name: rule, colorspace: raw, pattern: "*", extension: a, custom: {4: val4, key1: new val, key2: val2, key3: 3}}
  - !<Rule> {name: Default, colorspace: default, custom: {keyDef: valDef}}

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
    assert_eq!(config.serialize().unwrap(), expected);

    let reloaded = Config::create_from_str(expected).unwrap();
    let rr = reloaded.file_rules();
    assert_eq!(rr.num_entries(), 2);
    assert_eq!(rr.num_custom_keys(0).unwrap(), 4);
    assert_eq!(rr.custom_key_name(0, 1).unwrap(), "key1");
    assert_eq!(rr.custom_key_value(0, 1).unwrap(), "new val");
    assert_eq!(rr.custom_key_name(0, 2).unwrap(), "key2");
    assert_eq!(rr.custom_key_value(0, 2).unwrap(), "val2");
    assert_eq!(rr.custom_key_name(0, 3).unwrap(), "key3");
    assert_eq!(rr.custom_key_value(0, 3).unwrap(), "3");
    assert_eq!(rr.custom_key_name(0, 0).unwrap(), "4");
    assert_eq!(rr.custom_key_value(0, 0).unwrap(), "val4");
    assert_eq!(rr.num_custom_keys(1).unwrap(), 1);
    assert_eq!(rr.custom_key_name(1, 0).unwrap(), "keyDef");
    assert_eq!(rr.custom_key_value(1, 0).unwrap(), "valDef");
}

#[test]
fn file_rules_config_rule_u8() {
    let config_raw = Config::create_raw();
    let mut fr = config_raw.file_rules().clone();
    fr.insert_rule(0, "éÀÂÇÉÈç$€", "raw", "*", "a").unwrap();
    assert_eq!(fr.num_entries(), 2);
    fr.set_custom_key(0, "key£", "val€").unwrap();
    assert_eq!(fr.custom_key_name(0, 0).unwrap(), "key£");
    assert_eq!(fr.custom_key_value(0, 0).unwrap(), "val€");

    let mut config = config_raw.create_editable_copy();
    config.set_file_rules(&fr);
    let s = config.serialize().unwrap();
    let reloaded = Config::create_from_str(&s).unwrap();
    let rr = reloaded.file_rules();
    assert_eq!(rr.num_entries(), 2);
    assert_eq!(rr.name(0).unwrap(), "éÀÂÇÉÈç$€");
    assert_eq!(rr.num_custom_keys(0).unwrap(), 1);
    assert_eq!(rr.custom_key_name(0, 0).unwrap(), "key£");
    assert_eq!(rr.custom_key_value(0, 0).unwrap(), "val€");
}

const G_CONFIG: &str = r#"ocio_profile_version: 2
environment:
  {}
strictparsing: true
roles:
  default: raw
  role1: cs1
  role2: cs2
displays:
  sRGB:
  - !<View> {name: Raw, colorspace: raw}
colorspaces:
  - !<ColorSpace>
      name: raw
  - !<ColorSpace>
      name: cs1
  - !<ColorSpace>
      name: cs2
  - !<ColorSpace>
      name: other_cs1
"#;

const G_NAME: &str = "rule1";
const G_FILE_EXT: &str = "exr";
const G_FILE_PATTERN: &str = "*";

fn g_config() -> Config {
    Config::create_from_str(G_CONFIG).unwrap().create_editable_copy()
}

#[test]
fn file_rules_rule_invalid() {
    let mut config = g_config();
    config.validate().unwrap();
    let mut rules = config.file_rules().clone();
    assert_eq!(rules.num_entries(), 1);

    rules.insert_rule(0, G_NAME, "cs1", G_FILE_PATTERN, G_FILE_EXT).unwrap();
    config.set_file_rules(&rules);
    config.validate().unwrap();

    rules.set_color_space(0, "role1").unwrap();
    config.set_file_rules(&rules);
    config.validate().unwrap();

    rules.set_color_space(0, "invalid_color_space").unwrap();
    config.set_file_rules(&rules);
    assert_err!(
        config.validate(),
        "rule named 'rule1' is referencing 'invalid_color_space' that is neither a color space nor a named transform"
    );
}

#[test]
fn file_rules_pattern_error() {
    let mut rules = Config::create_raw().file_rules().clone();
    rules.insert_rule(0, FILE_PATH_SEARCH_RULE_NAME, "", "", "").unwrap();
    rules.insert_rule(0, "new rule", "raw", "*", "a").unwrap();
    assert_eq!(rules.num_entries(), 3);

    assert_err!(rules.set_pattern(0, ""), "file name pattern is empty");
    for p in ["[]", "[!]", "[a-b", "[a-b]]", "[[a-b]]", "[*]"] {
        assert_err!(rules.set_pattern(0, p), "invalid regular expression");
    }
}

#[test]
fn file_rules_with_defaults() {
    let config = Config::create_raw().create_editable_copy();
    let mut rules = config.file_rules().clone();
    rules.insert_rule(0, FILE_PATH_SEARCH_RULE_NAME, "", "", "").unwrap();
    assert_eq!(rules.num_entries(), 2);

    assert!(rules.insert_rule(0, "new rule2", "raw", "", "a").is_err());
    assert!(rules.insert_rule(0, "new rule3", "raw", "a", "").is_err());
    assert!(rules.insert_rule_regex(0, "new rule2", "raw", "").is_err());
}

#[test]
fn file_rules_extension_error() {
    let mut rules = Config::create_raw().file_rules().clone();
    rules.insert_rule(0, FILE_PATH_SEARCH_RULE_NAME, "", "", "").unwrap();
    rules.insert_rule(0, "new rule", "raw", "*", "a").unwrap();
    assert_eq!(rules.num_entries(), 3);
    assert_err!(rules.set_extension(0, ""), "file extension pattern is empty");
}

#[test]
fn file_rules_multiple_rules() {
    let mut config = g_config();
    config.validate().unwrap();
    let mut rules = config.file_rules().clone();

    let nb_default = rules.num_entries();
    for i in 0..42 {
        rules
            .insert_rule(0, &format!("rule{i}"), "cs1", G_FILE_PATTERN, G_FILE_EXT)
            .unwrap();
        assert_eq!(rules.num_entries(), i + 1 + nb_default);
    }
    config.set_file_rules(&rules);

    let s = config.serialize().unwrap();
    let reloaded = Config::create_from_str(&s).unwrap();
    assert_eq!(reloaded.file_rules().num_entries(), 42 + nb_default);
}

fn pos(config: &Config, path: &str) -> usize {
    config.color_space_from_filepath_with_index(path).1
}

#[test]
fn file_rules_rules_filepattern() {
    let mut config = g_config();
    let mut rules = config.file_rules().clone();

    rules.insert_rule(0, G_NAME, "cs1", "*", "[eE][xX][r]").unwrap();
    config.set_file_rules(&rules);

    assert_eq!(pos(&config, "/An/Arbitrary/Path/MyFile.exr"), 0);
    assert_eq!(pos(&config, "/An/Arbitrary/Path/MyFile.eXr"), 0);
    assert_eq!(pos(&config, "/An/Arbitrary/Path/MyFile.EXR"), 1);
    assert_eq!(pos(&config, "/An/Arbitrary/Path/MyFileexr"), 1);
    assert_eq!(pos(&config, "/An/Arbitrary/Path/MyFile.jpeg"), 1);
    assert_eq!(pos(&config, "/An/Arbitrary.exr/Path/MyFileexr"), 1);
    assert_eq!(pos(&config, ""), 1);

    let mut check = |pattern: &str, cases: &[(&str, usize)]| {
        rules.set_pattern(0, pattern).unwrap();
        config.set_file_rules(&rules);
        for (path, expected) in cases {
            assert_eq!(pos(&config, path), *expected, "pattern {pattern:?} path {path:?}");
        }
    };

    check("gamma", &[("/An/gamma/Arbitrary/Path/MyFile.exr", 1)]);
    check("*gamma", &[("/An/gamma/Arbitrary/Path/MyFile.exr", 1)]);
    check("gamma*", &[("/An/gamma/Arbitrary/Path/MyFile.exr", 1)]);
    check(
        "*gamma*",
        &[
            ("/An/Arbitrary/Path/MyFile.exr", 1),
            ("/An/GaMma/Arbitrary/Path/MyFile.exr", 1),
            ("/An/gamma/Arbitrary/Path/MyFile.exr", 0),
            ("/An/gammaArbitrary/Path/MyFile.exr", 0),
        ],
    );
    check(
        "*ga?ma*",
        &[
            ("/An/Arbitrary/Path/MyFile.exr", 1),
            ("/An/GaMma/Arbitrary/Path/MyFile.exr", 1),
            ("/An/gamma/Arbitrary/Path/MyFile.exr", 0),
            ("/An/gammaArbitrary/Path/MyFile.exr", 0),
            ("/An/gatmaArbitrary/Path/MyFile.exr", 0),
            ("/An/gatttttttmaArbitrary/Path/MyFile.exr", 1),
            ("/An/gamaArbitrary/Path/MyFile.exr", 1),
        ],
    );
    check(
        "*ga*ma*",
        &[
            ("/An/Arbitrary/Path/MyFile.exr", 1),
            ("/An/GaMma/Arbitrary/Path/MyFile.exr", 1),
            ("/An/gamma/Arbitrary/Path/MyFile.exr", 0),
            ("/An/gammaArbitrary/Path/MyFile.exr", 0),
            ("/An/gatmaArbitrary/Path/MyFile.exr", 0),
            ("/An/gatttttttmaArbitrary/Path/MyFile.exr", 0),
            ("/An/gamaArbitrary/Path/MyFile.exr", 0),
        ],
    );
    check(
        "*g?mm*",
        &[
            ("/An/gamma/Arbitrary/Path/MyFile.exr", 0),
            ("/An/gImma/Arbitrary/Path/MyFile.exr", 0),
            ("/An/gImmaaa/Arbitrary/Path/MyFile.exr", 0),
        ],
    );
    check(
        "*g*mm*",
        &[
            ("/An/gamma/Arbitrary/Path/MyFile.exr", 0),
            ("/An/gImma/Arbitrary/Path/MyFile.exr", 0),
            ("/An/gIIImmaaa/Arbitrary/Path/MyFile.exr", 0),
            ("/An/gmm/Arbitrary/Path/MyFile.exr", 0),
        ],
    );
    check(
        "*g?m?a*",
        &[
            ("/An/gamma/Arbitrary/Path/MyFile.exr", 0),
            ("/An/gImma/Arbitrary/Path/MyFile.exr", 0),
            ("/An/gImIa/Arbitrary/Path/MyFile.exr", 0),
            ("/An/gma/Arbitrary/Path/MyFile.exr", 1),
            ("/An/gIIImmaaa/Arbitrary/Path/MyFile.exr", 1),
        ],
    );
    check(
        "*g[a]mma*",
        &[
            ("/An/gmma/Arbitrary/Path/MyFile.exr", 1),
            ("/An/gamma/Arbitrary/Path/MyFile.exr", 0),
            ("/An/gbmma/Arbitrary/Path/MyFile.exr", 1),
        ],
    );
    check(
        "*g[!a]mma*",
        &[
            ("/An/gmma/Arbitrary/Path/MyFile.exr", 1),
            ("/An/gamma/Arbitrary/Path/MyFile.exr", 1),
            ("/An/gbmma/Arbitrary/Path/MyFile.exr", 0),
        ],
    );
    check(
        "*g[abcd]mma*",
        &[
            ("/An/gamma/Arbitrary/Path/MyFile.exr", 0),
            ("/An/gbmma/Arbitrary/Path/MyFile.exr", 0),
            ("/An/gcmma/Arbitrary/Path/MyFile.exr", 0),
            ("/An/gdmma/Arbitrary/Path/MyFile.exr", 0),
            ("/An/gmma/Arbitrary/Path/MyFile.exr", 1),
            ("/An/gemma/Arbitrary/Path/MyFile.exr", 1),
            ("/An/gabmma/Arbitrary/Path/MyFile.exr", 1),
        ],
    );
    check(
        "*g[!abcd]mma*",
        &[
            ("/An/gamma/Arbitrary/Path/MyFile.exr", 1),
            ("/An/gbmma/Arbitrary/Path/MyFile.exr", 1),
            ("/An/gcmma/Arbitrary/Path/MyFile.exr", 1),
            ("/An/gdmma/Arbitrary/Path/MyFile.exr", 1),
            ("/An/gmma/Arbitrary/Path/MyFile.exr", 1),
            ("/An/gemma/Arbitrary/Path/MyFile.exr", 0),
            ("/An/gabmma/Arbitrary/Path/MyFile.exr", 1),
            ("/An/gefmma/Arbitrary/Path/MyFile.exr", 1),
        ],
    );
    check(
        "*g[a-d]mma*",
        &[
            ("/An/gamma/Arbitrary/Path/MyFile.exr", 0),
            ("/An/gbmma/Arbitrary/Path/MyFile.exr", 0),
            ("/An/gcmma/Arbitrary/Path/MyFile.exr", 0),
            ("/An/gdmma/Arbitrary/Path/MyFile.exr", 0),
            ("/An/gmma/Arbitrary/Path/MyFile.exr", 1),
            ("/An/gemma/Arbitrary/Path/MyFile.exr", 1),
            ("/An/gabmma/Arbitrary/Path/MyFile.exr", 1),
            ("/An/gefmma/Arbitrary/Path/MyFile.exr", 1),
        ],
    );
    check(
        "g[!a-d]mma*",
        &[
            ("gamma/Arbitrary/Path/MyFile.exr", 1),
            ("gbmma/Arbitrary/Path/MyFile.exr", 1),
            ("gcmma/Arbitrary/Path/MyFile.exr", 1),
            ("gdmma/Arbitrary/Path/MyFile.exr", 1),
            ("gmma/Arbitrary/Path/MyFile.exr", 1),
            ("gemma/Arbitrary/Path/MyFile.exr", 0),
            ("gabmma/Arbitrary/Path/MyFile.exr", 1),
            ("gefmma/Arbitrary/Path/MyFile.exr", 1),
        ],
    );
    check("g[!a-d][\\*][e-g]mma", &[("ge*fmma.exr", 0)]);

    // Add pattern + extension rule.
    rules.insert_rule(0, "rule0", "cs1", "*", "jpg").unwrap();
    config.set_file_rules(&rules);
    assert_eq!(pos(&config, "test.jpg"), 0);
    assert_eq!(pos(&config, "test.Jpg"), 0);
    assert_eq!(pos(&config, "test.jpG"), 0);
    assert_eq!(pos(&config, "test.Jpeg"), 2);

    let mut check_ext = |ext: &str, cases: &[(&str, usize)]| {
        rules.set_extension(0, ext).unwrap();
        config.set_file_rules(&rules);
        for (path, expected) in cases {
            assert_eq!(pos(&config, path), *expected, "extension {ext:?} path {path:?}");
        }
    };
    check_ext("jp[gG]", &[("test.jpg", 0), ("test.jpG", 0), ("test.Jpg", 2)]);
    check_ext("[Jj]pg", &[("/mnt/media/image.Jpg", 0)]);
    check_ext("?pg", &[("/mnt/media/image.Jpg", 0)]);
    check_ext("jpg", &[("/mnt/media/image.Jpg", 0)]);
    check_ext("JPG", &[("/mnt/media/image.Jpg", 0)]);
    check_ext("jp[gG]", &[("/mnt/media/image.Jpg", 2)]);
    check_ext("?PG", &[("/mnt/media/image.Jpg", 2)]);
    check_ext("jP*", &[("/mnt/media/image.Jpg", 2)]);
    check_ext("*g", &[("/mnt/media/image.Jpg", 0)]);

    rules.set_pattern(0, "*[^]*").unwrap();
    config.set_file_rules(&rules);
    assert_eq!(pos(&config, "/mnt/me^ia/image.Jpg"), 0);
    assert_eq!(pos(&config, "/mnt/media/image.Jpg"), 2);

    rules.set_pattern(0, "*(name)*").unwrap();
    config.set_file_rules(&rules);
    assert_eq!(pos(&config, "/mnt/(name)/image.Jpg"), 0);
}

#[test]
fn file_rules_rules_regex() {
    let mut config = g_config();
    let mut rules = config.file_rules().clone();
    rules
        .insert_rule_regex(0, G_NAME, "cs1", r"(.*)(\bmine\b|\byours\b)(.*)")
        .unwrap();
    config.set_file_rules(&rules);

    assert_eq!(pos(&config, "mnt/mine/media/image.jpg"), 0);
    assert_eq!(pos(&config, "mnt/miner/media/image.jpg"), 1);
    assert_eq!(pos(&config, "yours/mnt/media/image.jpg"), 0);
    assert_eq!(pos(&config, r"mnt\media\yours\image.jpg"), 0);
    assert_eq!(pos(&config, "mine/media/image.jpg"), 0);

    assert_err!(
        rules.insert_rule_regex(1, "invalid", "cs1", r"(.*)(\bmine\b|\byours\b(.*)"),
        "invalid regular expression"
    );
}

#[test]
fn file_rules_rules_long_filepattern() {
    let mut config = g_config();
    let mut rules = config.file_rules().clone();
    rules.insert_rule(0, G_NAME, "cs1", "*", "exr").unwrap();
    config.set_file_rules(&rules);

    const PATH: &str =
        "/Users/hodoulp/Documents/work/Color Management/ocio-images.1.0v4/spi-vfx/marci_512_srgb.exr";
    assert_eq!(pos(&config, PATH), 0);

    for p in [
        "*Col?r*",
        "************************************************************",
        "*?",
        "?*",
        "*?*",
        "*.1.0v4*",
        "*.1.*",
    ] {
        rules.set_pattern(0, p).unwrap();
        config.set_file_rules(&rules);
        assert_eq!(pos(&config, PATH), 0, "pattern {p:?}");
    }
}

#[test]
fn file_rules_rules_test() {
    let mut config = g_config();
    let mut rules = config.file_rules().clone();
    rules.insert_path_search_rule(0).unwrap();
    rules.insert_rule(1, "dpx file", "raw", "*", "dpx").unwrap();
    config.set_file_rules(&rules);

    let f = |p: &str| config.color_space_from_filepath_with_index(p);
    assert_eq!(f("/mnt/user/show/img_cs1.dpx"), ("cs1".into(), 0));
    // The first color space name from the right.
    assert_eq!(f("show/cs2/img_cs1.exr"), ("cs1".into(), 0));
    // If there are 2 cs names ending the same position, the longest is used.
    assert_eq!(f("show/cs1/img_other_cs1.exr"), ("other_cs1".into(), 0));
    assert_eq!(f("show/other_cs1/img_cs1.exr"), ("cs1".into(), 0));
    assert_eq!(f("/mnt/user/unknown.dpx"), ("raw".into(), 1));
    assert_eq!(f("/mnt/user/unknown.jpg"), (ROLE_DEFAULT.into(), 2));
}

#[test]
fn file_rules_rules_priority() {
    let mut config = g_config();
    let mut rules = config.file_rules().clone();
    rules.insert_rule(0, "pattern dpx file", "raw", "*cs2*", "dpx").unwrap();
    rules.insert_path_search_rule(1).unwrap();
    rules.insert_rule_regex(2, "regex rule", "cs5", ".*cs5.dpx").unwrap();
    config.set_file_rules(&rules);

    let f = |p: &str| config.color_space_from_filepath_with_index(p);
    assert_eq!(f("/mnt/media/cs2.dpx"), ("raw".into(), 0));
    assert_eq!(f("/mnt/media/cs2.exr"), ("cs2".into(), 1));
    assert_eq!(f("/mnt/media/cs5.dpx"), ("cs5".into(), 2));
    assert_eq!(f("/mnt/media/cs5.DPX"), (ROLE_DEFAULT.into(), 3));
}

#[test]
fn file_rules_config_no_default() {
    const CONFIG: &str = r#"ocio_profile_version: 2
strictparsing: true
roles:
  role1: cs1
  role2: cs2
displays:
  sRGB:
  - !<View> {name: Raw, colorspace: raw}
colorspaces:
  - !<ColorSpace>
      name: raw
  - !<ColorSpace>
      name: cs1
  - !<ColorSpace>
      name: cs2
"#;
    assert_err!(
        Config::create_from_str(CONFIG),
        "must contain either a Default file rule or the 'default' role"
    );
}

#[test]
fn file_rules_config_default_missmatch() {
    const CONFIG: &str = r#"ocio_profile_version: 2
environment:
  {}
strictparsing: true
roles:
  default: raw
  role1: cs1
  role2: cs2
displays:
  sRGB:
  - !<View> {name: Raw, colorspace: raw}
colorspaces:
  - !<ColorSpace>
      name: raw
  - !<ColorSpace>
      name: cs1
  - !<ColorSpace>
      name: cs2
file_rules:
  - !<Rule> {name: Default, colorspace: cs1}
"#;
    let guard = LogGuard::new();
    let config = Config::create_from_str(CONFIG).unwrap().create_editable_copy();
    assert!(guard.output().contains("that does not match the default role"));

    let rules = config.file_rules();
    assert_eq!(rules.num_entries(), 1);
    assert_eq!(rules.name(0).unwrap(), DEFAULT_RULE_NAME);
    assert_eq!(rules.color_space(0).unwrap(), "cs1");
    assert_eq!(config.color_space_from_filepath("anything"), "cs1");
    assert_eq!(config.get_color_space(ROLE_DEFAULT).unwrap().name(), "raw");
}

#[test]
fn file_rules_config_no_default_role() {
    const CONFIG: &str = r#"ocio_profile_version: 2
environment:
  {}
strictparsing: true
roles:
  role1: cs1
  role2: cs2
displays:
  sRGB:
  - !<View> {name: Raw, colorspace: raw}
colorspaces:
  - !<ColorSpace>
      name: raw
  - !<ColorSpace>
      name: cs1
  - !<ColorSpace>
      name: cs2
file_rules:
  - !<Rule> {name: Default, colorspace: cs1}
"#;
    let guard = LogGuard::new();
    let config = Config::create_from_str(CONFIG).unwrap();
    assert!(guard.output().is_empty(), "{}", guard.output());
    config.validate().unwrap();
}

const RULES_BASE: &str = r#"ocio_profile_version: 2
strictparsing: true
roles:
  default: raw
  role1: cs1
  role2: cs2
displays:
  sRGB:
  - !<View> {name: Raw, colorspace: raw}
colorspaces:
  - !<ColorSpace>
      name: raw
  - !<ColorSpace>
      name: cs1
  - !<ColorSpace>
      name: cs2
"#;

#[test]
fn file_rules_config_default_no_colorspace() {
    let cfg = format!("{RULES_BASE}file_rules:\n  - !<Rule> {{name: Default}}\n");
    assert_err!(
        Config::create_from_str(&cfg),
        "'Default' rule cannot have an empty color space name"
    );
}

#[test]
fn file_rules_config_no_default_rule() {
    let cfg = format!(
        "{RULES_BASE}file_rules:\n  - !<Rule> {{name: Custom, pattern: \"*\", extension: jpg, colorspace: cs1}}\n"
    );
    assert_err!(Config::create_from_str(&cfg), "'file_rules' does not contain a Default <Rule>");
}

#[test]
fn file_rules_config_filerule_no_colorspace() {
    let cfg = format!(
        "{RULES_BASE}file_rules:\n  - !<Rule> {{name: Custom, pattern: \"*\", extension: jpg}}\n  - !<Rule> {{name: Default, colorspace: default}}\n"
    );
    assert_err!(
        Config::create_from_str(&cfg),
        "File rule 'Custom' cannot have an empty color space name"
    );
}

#[test]
fn file_rules_config_v1_faulty() {
    let cfg = RULES_BASE.replace("ocio_profile_version: 2", "ocio_profile_version: 1")
        + "file_rules:\n  - !<Rule> {name: Default, colorspace: default}\n";
    assert_err!(Config::create_from_str(&cfg), "Config v1 can't use 'file_rules'");
}

fn validate_muting_roles(config: &Config) {
    let guard = LogGuard::new();
    config.validate().unwrap();
    mute_missing_role_errors(&guard);
    assert!(guard.output().is_empty(), "unexpected log: {}", guard.output());
}

#[test]
fn file_rules_config_v1_to_v2_from_file() {
    {
        let cfg = RULES_BASE.replace("ocio_profile_version: 2", "ocio_profile_version: 1");
        let mut config = Config::create_from_str(&cfg).unwrap().create_editable_copy();
        config.validate().unwrap();
        assert_eq!(config.major_version(), 1);
        let rules = config.file_rules();
        assert_eq!(rules.num_entries(), 2);
        assert_eq!(rules.name(0).unwrap(), FILE_PATH_SEARCH_RULE_NAME);
        assert_eq!(rules.name(1).unwrap(), DEFAULT_RULE_NAME);
        assert_eq!(rules.color_space(1).unwrap(), "default");
        assert_eq!(config.color_space_from_filepath("/usr/cs2_file.exr"), "cs2");
        assert_eq!(config.color_space_from_filepath("/usr/file.exr"), "default");

        config.upgrade_to_latest_version();
        validate_muting_roles(&config);
        assert_eq!(config.major_version(), 2);
        let rules = config.file_rules();
        assert_eq!(rules.num_entries(), 2);
        assert_eq!(rules.name(0).unwrap(), FILE_PATH_SEARCH_RULE_NAME);
        assert_eq!(rules.name(1).unwrap(), DEFAULT_RULE_NAME);
        assert_eq!(rules.color_space(1).unwrap(), "default");
        assert_eq!(config.color_space_from_filepath("/usr/cs2_file.exr"), "cs2");
        assert_eq!(config.color_space_from_filepath("/usr/file.exr"), "default");
    }
    {
        const CONFIG: &str = r#"ocio_profile_version: 1
strictparsing: true
roles:
  role1: cs1
  role2: cs2
displays:
  sRGB:
  - !<View> {name: Raw, colorspace: rAw}
colorspaces:
  - !<ColorSpace>
      name: rAw
      isdata: true
  - !<ColorSpace>
      name: cs1
  - !<ColorSpace>
      name: cs2
"#;
        let mut config = Config::create_from_str(CONFIG).unwrap().create_editable_copy();
        config.validate().unwrap();
        assert_eq!(config.major_version(), 1);
        assert_eq!(config.file_rules().num_entries(), 2);
        assert_eq!(config.file_rules().color_space(1).unwrap(), "rAw");
        assert_eq!(config.color_space_from_filepath("/usr/cs2_file.exr"), "cs2");
        assert_eq!(config.color_space_from_filepath("/usr/file.exr"), "rAw");

        config.upgrade_to_latest_version();
        validate_muting_roles(&config);
        assert_eq!(config.major_version(), 2);
        assert_eq!(config.file_rules().num_entries(), 2);
        assert_eq!(config.file_rules().name(0).unwrap(), FILE_PATH_SEARCH_RULE_NAME);
        assert_eq!(config.file_rules().color_space(1).unwrap(), "rAw");
        assert_eq!(config.color_space_from_filepath("/usr/cs2_file.exr"), "cs2");
        assert_eq!(config.color_space_from_filepath("/usr/file.exr"), "rAw");
    }
    {
        const CONFIG: &str = r#"ocio_profile_version: 1
strictparsing: true
roles:
  role1: cs1
  role2: cs2
displays:
  sRGB:
  - !<View> {name: Raw, colorspace: rAw}
colorspaces:
  - !<ColorSpace>
      name: cs1
  - !<ColorSpace>
      name: cs2
  - !<ColorSpace>
      name: rAw
"#;
        let config = Config::create_from_str(CONFIG).unwrap();
        config.validate().unwrap();
        assert_eq!(config.major_version(), 1);
        assert_eq!(config.file_rules().num_entries(), 2);
        assert_eq!(config.file_rules().color_space(1).unwrap(), "cs1");
        assert_eq!(config.color_space_from_filepath("/usr/cs2_file.exr"), "cs2");
        assert_eq!(config.color_space_from_filepath("/usr/file.exr"), "cs1");

        {
            let mut cfg = config.create_editable_copy();
            cfg.set_inactive_color_spaces("cs1");
            cfg.upgrade_to_latest_version();
            validate_muting_roles(&cfg);
            assert_eq!(cfg.major_version(), 2);
            let rules = cfg.file_rules();
            assert_eq!(rules.num_entries(), 2);
            assert_eq!(rules.name(0).unwrap(), FILE_PATH_SEARCH_RULE_NAME);
            assert_eq!(rules.name(1).unwrap(), DEFAULT_RULE_NAME);
            assert_eq!(rules.color_space(1).unwrap(), "cs2");
            assert_eq!(cfg.color_space_from_filepath("/usr/cs1_file.exr"), "cs1");
            assert_eq!(cfg.color_space_from_filepath("/usr/file.exr"), "cs2");
        }
        {
            let mut cfg = config.create_editable_copy();
            cfg.set_inactive_color_spaces("cs1, cs2, raw");
            {
                let l = LogGuard::new();
                cfg.upgrade_to_latest_version();
                assert_eq!(
                    l.output(),
                    "[OpenColorIO Warning]: The default rule creation falls back to the first color space because no suitable color space exists.\n"
                );
            }
            validate_muting_roles(&cfg);
            assert_eq!(cfg.major_version(), 2);
            let rules = cfg.file_rules();
            assert_eq!(rules.num_entries(), 2);
            assert_eq!(rules.color_space(1).unwrap(), "cs1");
            assert_eq!(cfg.color_space_from_filepath("/usr/raw_file.exr"), "rAw");
            assert_eq!(cfg.color_space_from_filepath("/usr/file.exr"), "cs1");
        }
    }
}

fn validate_error_muting_roles(config: &Config, what: &str) {
    let guard = LogGuard::new();
    assert_err!(config.validate(), what);
    mute_missing_role_errors(&guard);
    assert!(guard.output().is_empty(), "unexpected log: {}", guard.output());
}

#[test]
fn file_rules_config_v1_to_v2_from_memory() {
    const ERR: &str =
        "rule named 'Default' is referencing 'default' that is neither a color space nor a named transform";
    {
        let mut config = Config::create();
        config.set_major_version(1).unwrap();
        config.add_display_view("disp1", "view1", "cs1", "").unwrap();
        let mut cs1 = ColorSpace::default();
        cs1.set_name("cs1");
        cs1.set_is_data(true);
        config.add_color_space(&cs1).unwrap();
        let mut raw = ColorSpace::default();
        raw.set_name("rAw");
        config.add_color_space(&raw).unwrap();
        config.validate().unwrap();

        config.set_major_version(2).unwrap();
        validate_error_muting_roles(&config, ERR);

        config.set_major_version(1).unwrap();
        config.upgrade_to_latest_version();
        validate_muting_roles(&config);
        assert_eq!(config.major_version(), 2);
        let rules = config.file_rules();
        assert_eq!(rules.num_entries(), 2);
        assert_eq!(rules.name(0).unwrap(), FILE_PATH_SEARCH_RULE_NAME);
        assert_eq!(rules.name(1).unwrap(), DEFAULT_RULE_NAME);
        assert_eq!(rules.color_space(1).unwrap(), "cs1");
    }
    {
        let mut config = Config::create();
        config.set_major_version(1).unwrap();
        config.add_display_view("disp1", "view1", "cs1", "").unwrap();
        let mut cs1 = ColorSpace::default();
        cs1.set_name("cs1");
        config.add_color_space(&cs1).unwrap();
        let mut raw = ColorSpace::default();
        raw.set_name("rAw");
        config.add_color_space(&raw).unwrap();
        config.validate().unwrap();

        config.set_major_version(2).unwrap();
        validate_error_muting_roles(&config, ERR);

        config.set_major_version(1).unwrap();
        config.upgrade_to_latest_version();
        validate_muting_roles(&config);
        assert_eq!(config.major_version(), 2);
        assert_eq!(config.file_rules().color_space(1).unwrap(), "cs1");
    }
    {
        let mut config = Config::create();
        config.set_major_version(1).unwrap();
        config.add_display_view("disp1", "view1", "cs1", "").unwrap();
        let mut cs1 = ColorSpace::default();
        cs1.set_name("cs1");
        config.add_color_space(&cs1).unwrap();
        config.validate().unwrap();

        config.set_inactive_color_spaces("cs1");
        config.set_major_version(2).unwrap();
        validate_error_muting_roles(&config, ERR);

        config.set_major_version(1).unwrap();
        {
            let l = LogGuard::new();
            config.upgrade_to_latest_version();
            assert!(l.output().lines().any(|line| line
                == "[OpenColorIO Warning]: The default rule creation falls back to the first color space because no suitable color space exists."));
        }
        validate_muting_roles(&config);
        assert_eq!(config.major_version(), 2);
        let rules = config.file_rules();
        assert_eq!(rules.num_entries(), 2);
        assert_eq!(rules.color_space(1).unwrap(), "cs1");
    }
}

#[test]
fn file_rules_read_write_incomplete_configs() {
    {
        const CONFIG: &str = r#"ocio_profile_version: 2
roles:
  default: cs2

colorspaces:
  - !<ColorSpace>
      name: raw
"#;
        let cfg = Config::create_from_str(CONFIG).unwrap();
        cfg.serialize().unwrap();
        assert_err!(
            cfg.validate(),
            "Config failed role validation. The role 'default' refers to a color space, 'cs2', which is not defined."
        );
    }
    {
        const CONFIG: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: cs2}

displays:
  sRGB:
  - !<View> {name: Raw, colorspace: raw}
colorspaces:
  - !<ColorSpace>
      name: raw
"#;
        let cfg = Config::create_from_str(CONFIG).unwrap();
        cfg.serialize().unwrap();
        assert_err!(
            cfg.validate(),
            "File rules: rule named 'Default' is referencing 'cs2' that is neither a color space nor a named transform."
        );
    }
}

#[test]
fn file_rules_config_v2_wrong_rule() {
    let cases = [
        (
            "file_rules:\n  - !<Rule> {name: Default, colorspace: default}\n  - !<Rule> {name: Default, colorspace: cs1}\n",
            "Default rule has to be the last rule",
        ),
        (
            "file_rules:\n  - !<Rule> {name: Default, colorspace: cs2, regex: \".*\\\\.TIF?F$\"}\n",
            "'Default' rule can't use pattern, extension or regex.",
        ),
        (
            "file_rules:\n  - !<Rule> {name: Default, colorspace: raw}\n  - !<Rule> {name: Custom, colorspace: cs1, pattern: \"*\", extension: jpg}\n",
            "Default rule has to be the last rule",
        ),
        (
            "file_rules:\n  - !<Rule> {name: ColorSpaceNamePathSearch}\n  - !<Rule> {name: ColorSpaceNamePathSearch}\n  - !<Rule> {name: Default, colorspace: cs1}\n",
            "A rule named 'ColorSpaceNamePathSearch' already exists",
        ),
        (
            "file_rules:\n  - !<Rule> {name: Custom, colorspace: cs1, pattern: \"*\", extension: jpg, regex: \".*\\\\.TIF?F$\"}\n  - !<Rule> {name: Default, colorspace: cs1}\n",
            r"can't use regex '.*\.TIF?F$' and pattern & extension",
        ),
    ];
    for (rules, what) in cases {
        let cfg = format!("{G_CONFIG}{rules}");
        assert_err!(Config::create_from_str(&cfg), what);
    }
}

#[test]
fn file_rules_rule_move() {
    let config = g_config();
    config.validate().unwrap();
    let mut rules = config.file_rules().clone();
    for i in 0..5 {
        rules
            .insert_rule(i, &format!("rule{i}"), "cs1", G_FILE_PATTERN, G_FILE_EXT)
            .unwrap();
    }
    assert_eq!(rules.num_entries(), 6);

    assert_err!(rules.increase_rule_priority(0), "may not be moved to index '-1'");
    assert_err!(rules.decrease_rule_priority(4), "may not be moved to index '5'");
    assert_err!(rules.increase_rule_priority(5), "is the default rule");
    assert_err!(rules.decrease_rule_priority(5), "is the default rule");

    rules.decrease_rule_priority(2).unwrap();
    assert_eq!(rules.name(2).unwrap(), "rule3");
    assert_eq!(rules.name(3).unwrap(), "rule2");

    rules.increase_rule_priority(3).unwrap();
    assert_eq!(rules.name(2).unwrap(), "rule2");
    assert_eq!(rules.name(3).unwrap(), "rule3");

    rules.decrease_rule_priority(2).unwrap();
    rules.decrease_rule_priority(3).unwrap();
    assert_eq!(rules.name(2).unwrap(), "rule3");
    assert_eq!(rules.name(3).unwrap(), "rule4");
    assert_eq!(rules.name(4).unwrap(), "rule2");

    rules.increase_rule_priority(4).unwrap();
    rules.increase_rule_priority(3).unwrap();
    assert_eq!(rules.name(2).unwrap(), "rule2");
    assert_eq!(rules.name(3).unwrap(), "rule3");
    assert_eq!(rules.name(4).unwrap(), "rule4");
}

#[test]
fn file_rules_clone() {
    let config = Config::create_raw().create_editable_copy();
    let mut file_rules = config.file_rules().clone();
    file_rules.insert_rule(0, FILE_PATH_SEARCH_RULE_NAME, "", "", "").unwrap();
    file_rules.insert_rule(0, "rule", "raw", "*", "a").unwrap();
    assert_eq!(file_rules.num_entries(), 3);

    let mut new_rules = file_rules.clone();
    assert_eq!(new_rules.num_entries(), 3);
    assert_eq!(new_rules.pattern(0).unwrap(), file_rules.pattern(0).unwrap());

    new_rules.set_pattern(0, "*A").unwrap();
    assert_ne!(new_rules.pattern(0).unwrap(), file_rules.pattern(0).unwrap());
    file_rules.set_pattern(0, "*B").unwrap();
    assert_ne!(new_rules.pattern(0).unwrap(), file_rules.pattern(0).unwrap());
    new_rules.set_pattern(0, "*").unwrap();
    file_rules.set_pattern(0, "*").unwrap();
    assert_eq!(new_rules.pattern(0).unwrap(), file_rules.pattern(0).unwrap());
}

#[test]
fn file_rules_is_default() {
    let mut fr = FileRules::new();
    assert!(fr.is_default());
    fr.set_color_space(0, "DEFault").unwrap();
    assert!(fr.is_default());
    fr.set_color_space(0, "raw").unwrap();
    assert!(!fr.is_default());
    fr.set_color_space(0, "default").unwrap();
    assert!(fr.is_default());

    fr.set_custom_key(0, "key", "val").unwrap();
    assert!(!fr.is_default());
    fr.set_custom_key(0, "key", "").unwrap();
    assert!(fr.is_default());

    fr.insert_rule(0, "rule", "raw", "*", "a").unwrap();
    assert!(!fr.is_default());

    assert!(Config::create_raw().file_rules().is_default());
}
