//! Port of `MergeConfigsHelpers_tests.cpp` (part 2: file rules, displays /
//! views, view transforms and looks sections).

use super::tests::*;
use super::*;
use crate::config::FileRules;
use crate::types::*;

/// Check the name and color space of the file rule at `idx`.
#[track_caller]
fn check_rule(fr: &FileRules, idx: usize, name: &str, cs: Option<&str>) {
    assert_eq!(fr.name(idx).unwrap(), name);
    if let Some(cs) = cs {
        assert_eq!(fr.color_space(idx).unwrap(), cs);
    }
}

#[test]
fn merge_configs_file_rules_section() {
    let base = get_base_config();
    let input = get_input_config();

    let setup = |strategy: MergeStrategy| -> ConfigMergingParameters {
        let mut p = ConfigMergingParameters::new();
        p.set_file_rules(strategy);
        p.set_adjust_input_reference_space(false);
        p.set_avoid_duplicates(false);
        p
    };

    // Allowed strategies: All. Allowed merge options: All.

    // Test that the default strategy is used as a fallback if the section strategy was not
    // defined.
    {
        let mut params = setup(MergeStrategy::Unspecified);
        params.set_default_strategy(MergeStrategy::InputOnly);
        let merged = merge_section(&base, &input, &params, file_rules);
        assert!(!merged.is_strict_parsing_enabled());
        let fr = merged.file_rules();
        assert_eq!(fr.num_entries(), 5);
        check_rule(fr, 0, "LogC", Some("ACES2065-1"));
        check_rule(fr, 1, "TIFF", Some("sRGB - Texture"));
        check_rule(fr, 2, "JPEG", Some("Linear Rec.2020"));
        assert_eq!(fr.regex(2).unwrap(), ".*\\.jpeg$");
        check_rule(fr, 3, "ColorSpaceNamePathSearch", None);
        check_rule(fr, 4, "Default", Some("ACEScct - SomeOtherName"));
    }

    let conflicts = [
        "The Input config contains a value that would override the Base config: file_rules: TIFF",
        "The Input config contains a value that would override the Base config: file_rules: Default",
    ];

    // Test FileRules section with strategy = PreferInput.
    {
        let mut params = setup(MergeStrategy::PreferInput);
        params.set_input_first(true);
        let merged = check_merge(
            LogType::Warning,
            &base,
            &input,
            &params,
            file_rules,
            &conflicts,
        );
        let fr = merged.file_rules();
        assert_eq!(fr.num_entries(), 6);
        check_rule(fr, 0, "LogC", Some("ACES2065-1"));
        check_rule(fr, 1, "TIFF", Some("sRGB - Texture"));
        assert_eq!(fr.regex(1).unwrap(), ".*\\.TIF?F$");
        // Verify that the custom keys are merged.
        assert_eq!(fr.custom_key_name(1, 0).unwrap(), "key1");
        assert_eq!(fr.custom_key_value(1, 0).unwrap(), "value1");
        assert_eq!(fr.custom_key_name(1, 1).unwrap(), "key2");
        assert_eq!(fr.custom_key_value(1, 1).unwrap(), "value2");
        check_rule(fr, 2, "JPEG", Some("Linear Rec.2020"));
        assert_eq!(fr.regex(2).unwrap(), ".*\\.jpeg$");
        check_rule(fr, 3, "ColorSpaceNamePathSearch", None);
        check_rule(fr, 4, "OpenEXR", Some("ACEScct"));
        assert_eq!(fr.pattern(4).unwrap(), "*");
        assert_eq!(fr.extension(4).unwrap(), "exr");
        check_rule(fr, 5, "Default", Some("ACEScct - SomeOtherName"));
    }

    // Test FileRules section with strategy = PreferInput, options InputFirst = false.
    {
        let mut params = setup(MergeStrategy::PreferInput);
        params.set_input_first(false);
        let merged = check_merge(
            LogType::Warning,
            &base,
            &input,
            &params,
            file_rules,
            &conflicts,
        );
        assert!(!merged.is_strict_parsing_enabled());
        let fr = merged.file_rules();
        assert_eq!(fr.num_entries(), 6);
        check_rule(fr, 0, "LogC", Some("ACES2065-1"));
        check_rule(fr, 1, "TIFF", Some("sRGB - Texture"));
        assert_eq!(fr.custom_key_name(1, 0).unwrap(), "key1");
        assert_eq!(fr.custom_key_value(1, 0).unwrap(), "value1");
        assert_eq!(fr.custom_key_name(1, 1).unwrap(), "key2");
        assert_eq!(fr.custom_key_value(1, 1).unwrap(), "value2");
        check_rule(fr, 2, "OpenEXR", Some("ACEScct"));
        assert_eq!(fr.pattern(2).unwrap(), "*");
        assert_eq!(fr.extension(2).unwrap(), "exr");
        check_rule(fr, 3, "ColorSpaceNamePathSearch", None);
        check_rule(fr, 4, "JPEG", Some("Linear Rec.2020"));
        assert_eq!(fr.regex(4).unwrap(), ".*\\.jpeg$");
        check_rule(fr, 5, "Default", Some("ACEScct - SomeOtherName"));
    }

    // Test FileRules section with strategy = PreferBase.
    {
        let mut params = setup(MergeStrategy::PreferBase);
        params.set_input_first(true);
        let merged = check_merge(
            LogType::Warning,
            &base,
            &input,
            &params,
            file_rules,
            &conflicts,
        );
        assert!(merged.is_strict_parsing_enabled());
        let fr = merged.file_rules();
        assert_eq!(fr.num_entries(), 6);
        check_rule(fr, 0, "LogC", Some("ACES2065-1"));
        check_rule(fr, 1, "TIFF", Some("Gamma 2.2 AP1 - Texture"));
        assert_eq!(fr.num_custom_keys(1).unwrap(), 0);
        check_rule(fr, 2, "JPEG", Some("Linear Rec.2020"));
        assert_eq!(fr.regex(2).unwrap(), ".*\\.jpeg$");
        check_rule(fr, 3, "ColorSpaceNamePathSearch", None);
        check_rule(fr, 4, "OpenEXR", Some("ACEScct"));
        assert_eq!(fr.pattern(4).unwrap(), "*");
        assert_eq!(fr.extension(4).unwrap(), "exr");
        check_rule(fr, 5, "Default", Some("Raw"));
    }

    // Test FileRules section with strategy = PreferBase, options InputFirst = false.
    {
        let mut params = setup(MergeStrategy::PreferBase);
        params.set_input_first(false);
        let merged = check_merge(
            LogType::Warning,
            &base,
            &input,
            &params,
            file_rules,
            &conflicts,
        );
        assert!(merged.is_strict_parsing_enabled());
        let fr = merged.file_rules();
        assert_eq!(fr.num_entries(), 6);
        check_rule(fr, 0, "LogC", Some("ACES2065-1"));
        check_rule(fr, 1, "TIFF", Some("Gamma 2.2 AP1 - Texture"));
        check_rule(fr, 2, "OpenEXR", Some("ACEScct"));
        assert_eq!(fr.pattern(2).unwrap(), "*");
        assert_eq!(fr.extension(2).unwrap(), "exr");
        check_rule(fr, 3, "ColorSpaceNamePathSearch", None);
        check_rule(fr, 4, "JPEG", Some("Linear Rec.2020"));
        assert_eq!(fr.regex(4).unwrap(), ".*\\.jpeg$");
        check_rule(fr, 5, "Default", Some("Raw"));
    }

    // Test FileRules section with strategy = InputOnly.
    {
        let params = setup(MergeStrategy::InputOnly);
        let merged = merge_section(&base, &input, &params, file_rules);
        assert!(!merged.is_strict_parsing_enabled());
        let fr = merged.file_rules();
        assert_eq!(fr.num_entries(), 5);
        check_rule(fr, 0, "LogC", Some("ACES2065-1"));
        check_rule(fr, 1, "TIFF", Some("sRGB - Texture"));
        check_rule(fr, 2, "JPEG", Some("Linear Rec.2020"));
        assert_eq!(fr.regex(2).unwrap(), ".*\\.jpeg$");
        check_rule(fr, 3, "ColorSpaceNamePathSearch", None);
        check_rule(fr, 4, "Default", Some("ACEScct - SomeOtherName"));
    }

    // Test FileRules section with strategy = BaseOnly.
    {
        let params = setup(MergeStrategy::BaseOnly);
        let merged = merge_section(&base, &input, &params, file_rules);
        assert!(merged.is_strict_parsing_enabled());
        let fr = merged.file_rules();
        check_rule(fr, 0, "LogC", Some("ACES2065-1"));
        check_rule(fr, 1, "TIFF", Some("Gamma 2.2 AP1 - Texture"));
        check_rule(fr, 2, "OpenEXR", Some("ACEScct"));
        assert_eq!(fr.pattern(2).unwrap(), "*");
        assert_eq!(fr.extension(2).unwrap(), "exr");
        check_rule(fr, 3, "ColorSpaceNamePathSearch", None);
        check_rule(fr, 4, "Default", Some("Raw"));
    }

    // Test FileRules section with strategy = Remove.
    {
        let params = setup(MergeStrategy::Remove);
        let merged = merge_section(&base, &input, &params, file_rules);
        assert!(merged.is_strict_parsing_enabled());
        let fr = merged.file_rules();
        assert_eq!(fr.num_entries(), 2);
        check_rule(fr, 0, "OpenEXR", Some("ACEScct"));
        assert_eq!(fr.pattern(0).unwrap(), "*");
        assert_eq!(fr.extension(0).unwrap(), "exr");
        check_rule(fr, 1, "Default", Some("Raw"));
    }

    // Test FileRules section with strategy = PreferInput and copying ColorSpaceNamePathSearch.
    {
        let mut params = ConfigMergingParameters::new();
        params.set_file_rules(MergeStrategy::PreferInput);

        let mut e_input = input.create_editable_copy();
        let mut input_fr = e_input.file_rules().clone();
        // Delete ColorSpaceNamePathSearch, so it is only in the base and must be copied over.
        input_fr.remove_rule(3).unwrap();
        e_input.set_file_rules(&input_fr);

        let merged = check_merge(
            LogType::Warning,
            &base,
            &e_input,
            &params,
            file_rules,
            &conflicts,
        );
        let fr = merged.file_rules();
        assert_eq!(fr.num_entries(), 6);
        check_rule(fr, 0, "LogC", Some("ACES2065-1"));
        check_rule(fr, 1, "TIFF", Some("sRGB - Texture"));
        assert_eq!(fr.regex(1).unwrap(), ".*\\.TIF?F$");
        assert_eq!(fr.custom_key_name(1, 0).unwrap(), "key1");
        assert_eq!(fr.custom_key_value(1, 0).unwrap(), "value1");
        assert_eq!(fr.custom_key_name(1, 1).unwrap(), "key2");
        assert_eq!(fr.custom_key_value(1, 1).unwrap(), "value2");
        check_rule(fr, 2, "JPEG", Some("Linear Rec.2020"));
        assert_eq!(fr.regex(2).unwrap(), ".*\\.jpeg$");
        check_rule(fr, 3, "OpenEXR", Some("ACEScct"));
        assert_eq!(fr.pattern(3).unwrap(), "*");
        assert_eq!(fr.extension(3).unwrap(), "exr");
        check_rule(fr, 4, "ColorSpaceNamePathSearch", None);
        check_rule(fr, 5, "Default", Some("ACEScct - SomeOtherName"));
    }

    const PREFIX: &str = "The Input config contains a value that would override the Base config: ";

    // Test that error_on_conflicts is processed correctly.
    // strategy = PreferInput, InputFirst = true
    {
        let mut params = ConfigMergingParameters::new();
        params.set_file_rules(MergeStrategy::PreferInput);
        params.set_error_on_conflict(true);
        params.set_default_strategy(MergeStrategy::PreferInput);

        let with_rules = |cfg: &Config, rules: FileRules| -> Config {
            let mut c = cfg.create_editable_copy();
            c.set_file_rules(&rules);
            c
        };

        // Test that an error is thrown when the input config's COLORSPACE is different.
        {
            let mut base_fr = FileRules::new();
            base_fr
                .insert_rule(0, "ruleTestColorspace", "colorspace1", "*abc*", "*")
                .unwrap();
            let mut input_fr = FileRules::new();
            input_fr
                .insert_rule(0, "ruleTestColorspace", "colorspace2", "*abc*", "*")
                .unwrap();
            let b = with_rules(&base, base_fr);
            let i = with_rules(&input, input_fr);
            let msg = format!("{PREFIX}file_rules: ruleTestColorspace");
            check_merge(LogType::Error, &b, &i, &params, file_rules, &[&msg]);
        }

        // Test that an error is thrown when the input config's REGEX is different.
        {
            let mut base_fr = FileRules::new();
            base_fr
                .insert_rule_regex(0, "ruleTestColorspace", "colorspace1", ".*\\.TIF?F$")
                .unwrap();
            let mut input_fr = FileRules::new();
            input_fr
                .insert_rule_regex(0, "ruleTestColorspace", "colorspace1", ".*\\.TIF?F")
                .unwrap();
            let b = with_rules(&base, base_fr);
            let i = with_rules(&input, input_fr);
            let msg = format!("{PREFIX}file_rules: ruleTestColorspace");
            check_merge(LogType::Error, &b, &i, &params, file_rules, &[&msg]);
        }

        // Test that an error is thrown when the input config's PATTERN is different.
        {
            let mut base_fr = FileRules::new();
            base_fr
                .insert_rule(0, "ruleTestPattern", "colorspace1", "*abc*", "*")
                .unwrap();
            let mut input_fr = FileRules::new();
            input_fr
                .insert_rule(0, "ruleTestPattern", "colorspace1", "*abcd*", "*")
                .unwrap();
            let b = with_rules(&base, base_fr);
            let i = with_rules(&input, input_fr);
            let msg = format!("{PREFIX}file_rules: ruleTestPattern");
            check_merge(LogType::Error, &b, &i, &params, file_rules, &[&msg]);
        }

        // Test that an error is thrown when the input config's EXTENSION is different.
        {
            let mut base_fr = FileRules::new();
            base_fr
                .insert_rule(0, "ruleTestExtension", "colorspace1", "*abc*", "*")
                .unwrap();
            let mut input_fr = FileRules::new();
            input_fr
                .insert_rule(0, "ruleTestExtension", "colorspace1", "*abc*", "*a")
                .unwrap();
            let b = with_rules(&base, base_fr);
            let i = with_rules(&input, input_fr);
            let msg = format!("{PREFIX}file_rules: ruleTestExtension");
            check_merge(LogType::Error, &b, &i, &params, file_rules, &[&msg]);
        }

        // Test that an error is thrown when the input config's CUSTOM KEYS are different.
        {
            let mut base_fr = FileRules::new();
            base_fr
                .insert_rule(0, "ruleTestCustomKeys", "colorspace1", "*abc*", "*")
                .unwrap();
            base_fr.set_custom_key(0, "key1", "value1").unwrap();
            base_fr.set_custom_key(0, "key2", "value2").unwrap();
            let mut input_fr = FileRules::new();
            input_fr
                .insert_rule(0, "ruleTestCustomKeys", "colorspace1", "*abc*", "*")
                .unwrap();
            input_fr.set_custom_key(0, "key1", "value1").unwrap();
            input_fr.set_custom_key(0, "key2", "value22").unwrap();
            let b = with_rules(&base, base_fr);
            let i = with_rules(&input, input_fr);
            let msg = format!("{PREFIX}file_rules: ruleTestCustomKeys");
            check_merge(LogType::Error, &b, &i, &params, file_rules, &[&msg]);
        }

        // Test that no error is thrown when the input config's CUSTOM KEYS are the same.
        {
            let mut base_fr = FileRules::new();
            base_fr
                .insert_rule(0, "ruleTestCustomKeys", "colorspace1", "*abc*", "*")
                .unwrap();
            base_fr.set_custom_key(0, "key1", "value1").unwrap();
            base_fr.set_custom_key(0, "key2", "value2").unwrap();
            let mut input_fr = FileRules::new();
            input_fr
                .insert_rule(0, "ruleTestCustomKeys", "colorspace1", "*abc*", "*")
                .unwrap();
            input_fr.set_custom_key(0, "key2", "value2").unwrap();
            // Must be equal even in a different order.
            input_fr.set_custom_key(0, "key1", "value1").unwrap();
            let b = with_rules(&base, base_fr);
            let i = with_rules(&input, input_fr);
            merge_section(&b, &i, &params, file_rules);
        }
    }
}

#[test]
fn merge_configs_displays_views_section() {
    let base = get_base_config();
    let input = get_input_config();

    let setup = |strategy: MergeStrategy| -> ConfigMergingParameters {
        let mut p = ConfigMergingParameters::new();
        p.set_display_views(strategy);
        p
    };

    // Allowed strategies: All
    // Allowed merge options: All

    // Test that the default strategy is used as a fallback if the section strategy was not defined.
    {
        // Using STRATEGY_UNSPECIFIED as this simulates that the section
        // is missing from the OCIOM file.
        let mut params = setup(MergeStrategy::Unspecified);
        // Simulate settings from OCIOM file.
        params.set_default_strategy(MergeStrategy::InputOnly);
        params.set_input_first(true);
        let merged = merge_section(&base, &input, &params, display_views);

        assert_eq!(merged.active_displays(), "DISP_1, DISP_3");
        assert_eq!(merged.active_views(), "SHARED_1, SHARED_3, VIEW_1, VIEW_3");

        // Validate shared_views
        assert_eq!(merged.num_views_by_type(ViewType::Shared, ""), 2);
        assert_eq!(merged.view_by_type(ViewType::Shared, "", 0), "SHARED_1");
        assert_eq!(merged.view_by_type(ViewType::Shared, "", 1), "SHARED_3");

        // Validate displays
        assert_eq!(merged.num_displays_all(), 2);
        assert_eq!(merged.display(0), "DISP_1");
        assert_eq!(merged.display(1), "DISP_3");

        // Validate display/views

        assert_eq!(
            merged.num_views_by_type(ViewType::DisplayDefined, "DISP_1"),
            1
        );
        assert_eq!(merged.num_views_by_type(ViewType::Shared, "DISP_1"), 2);
        assert_eq!(
            merged.view_by_type(ViewType::DisplayDefined, "DISP_1", 0),
            "VIEW_1"
        );
        // Make sure this is the right VIEW_1 by checking the colorspace.
        assert_eq!(
            merged.display_view_color_space_name("DISP_1", "VIEW_1"),
            "view_1B"
        );

        assert_eq!(
            merged.view_by_type(ViewType::Shared, "DISP_1", 0),
            "SHARED_3"
        );
        assert_eq!(
            merged.display_view_color_space_name("DISP_1", "SHARED_3"),
            "log_3"
        );

        assert_eq!(
            merged.view_by_type(ViewType::Shared, "DISP_1", 1),
            "SHARED_1"
        );
        assert_eq!(
            merged.display_view_color_space_name("DISP_1", "SHARED_1"),
            "lin_3"
        );

        assert_eq!(
            merged.num_views_by_type(ViewType::DisplayDefined, "DISP_3"),
            2
        );
        assert_eq!(
            merged.view_by_type(ViewType::DisplayDefined, "DISP_3", 0),
            "VIEW_1"
        );
        assert_eq!(
            merged.view_by_type(ViewType::DisplayDefined, "DISP_3", 1),
            "VIEW_3"
        );

        // Validate viewing_rules
        let rules = merged.viewing_rules();

        assert_eq!(rules.num_entries(), 2);

        assert_eq!(rules.name(0).unwrap(), "RULE_1");
        assert_eq!(rules.num_color_spaces(0).unwrap(), 1);
        assert_eq!(rules.color_space(0, 0).unwrap(), "sRGB - Texture");

        assert_eq!(rules.name(1).unwrap(), "RULE_3");
        assert_eq!(rules.num_color_spaces(1).unwrap(), 2);
        assert_eq!(rules.color_space(1, 0).unwrap(), "Linear Rec.2020");
        assert_eq!(rules.color_space(1, 1).unwrap(), "ACEScct - SomeOtherName");

        // Validate virtual_display
        assert_eq!(
            merged.virtual_display_num_views(ViewType::DisplayDefined),
            2
        );
        assert_eq!(merged.virtual_display_num_views(ViewType::Shared), 1);

        assert_eq!(
            merged.virtual_display_view(ViewType::DisplayDefined, 0),
            "ACES"
        );
        assert_eq!(
            merged.virtual_display_view(ViewType::DisplayDefined, 1),
            "Lin"
        );

        assert_eq!(merged.virtual_display_view(ViewType::Shared, 0), "SHARED_3");
    }

    // Test display/views with strategy = PreferInput, options InputFirst = true.
    {
        let mut params = setup(MergeStrategy::PreferInput);
        params.set_input_first(true);
        let merged = check_merge(LogType::Warning, &base, &input, &params, display_views, &["The Input config contains a value that would override the Base config: shared_views: SHARED_1",
            "The Input config contains a value that would override the Base config: display: DISP_1, view: VIEW_1",
            "The Input config contains a value that would override the Base config: viewing_rules: RULE_1"]);

        assert_eq!(merged.active_displays(), "DISP_1, DISP_3, DISP_2");
        assert_eq!(
            merged.active_views(),
            "SHARED_1, SHARED_3, VIEW_1, VIEW_3, SHARED_2, VIEW_2"
        );

        // Validate shared_views
        assert_eq!(merged.num_views_by_type(ViewType::Shared, ""), 3);
        assert_eq!(merged.view_by_type(ViewType::Shared, "", 0), "SHARED_1");
        assert_eq!(merged.view_by_type(ViewType::Shared, "", 1), "SHARED_3");
        assert_eq!(merged.view_by_type(ViewType::Shared, "", 2), "SHARED_2");

        // Validate displays
        assert_eq!(merged.num_displays_all(), 3);
        assert_eq!(merged.display(0), "DISP_1");
        assert_eq!(merged.display(1), "DISP_3");
        assert_eq!(merged.display(2), "DISP_2");

        // Validate display/views
        assert_eq!(
            merged.num_views_by_type(ViewType::DisplayDefined, "DISP_1"),
            1
        );
        assert_eq!(merged.num_views_by_type(ViewType::Shared, "DISP_1"), 3);
        assert_eq!(
            merged.view_by_type(ViewType::DisplayDefined, "DISP_1", 0),
            "VIEW_1"
        );
        // Make sure this is the right VIEW_1 by checking the colorspace.
        assert_eq!(
            merged.display_view_color_space_name("DISP_1", "VIEW_1"),
            "view_1B"
        );
        assert_eq!(merged.display_view_rule("DISP_1", "VIEW_1"), "RULE_3");

        assert_eq!(
            merged.view_by_type(ViewType::Shared, "DISP_1", 0),
            "SHARED_3"
        );
        assert_eq!(
            merged.display_view_color_space_name("DISP_1", "SHARED_3"),
            "log_3"
        );

        assert_eq!(
            merged.view_by_type(ViewType::Shared, "DISP_1", 1),
            "SHARED_1"
        );
        assert_eq!(
            merged.display_view_color_space_name("DISP_1", "SHARED_1"),
            "lin_3"
        );

        assert_eq!(
            merged.view_by_type(ViewType::Shared, "DISP_1", 2),
            "SHARED_2"
        );
        assert_eq!(
            merged.display_view_color_space_name("DISP_1", "SHARED_2"),
            "<USE_DISPLAY_NAME>"
        );
        assert_eq!(
            merged.display_view_transform_name("DISP_1", "SHARED_2"),
            "SDR Video"
        );

        assert_eq!(
            merged.num_views_by_type(ViewType::DisplayDefined, "DISP_3"),
            2
        );
        assert_eq!(
            merged.view_by_type(ViewType::DisplayDefined, "DISP_3", 0),
            "VIEW_1"
        );
        assert_eq!(
            merged.view_by_type(ViewType::DisplayDefined, "DISP_3", 1),
            "VIEW_3"
        );
        assert_eq!(merged.display_view_looks("DISP_3", "VIEW_3"), "look_input");

        assert_eq!(
            merged.num_views_by_type(ViewType::DisplayDefined, "DISP_2"),
            2
        );
        assert_eq!(
            merged.view_by_type(ViewType::DisplayDefined, "DISP_2", 0),
            "VIEW_1"
        );
        assert_eq!(
            merged.view_by_type(ViewType::DisplayDefined, "DISP_2", 1),
            "VIEW_2"
        );

        // Validate viewing_rules
        let rules = merged.viewing_rules();

        assert_eq!(rules.num_entries(), 3);

        assert_eq!(rules.name(0).unwrap(), "RULE_1");
        assert_eq!(rules.num_color_spaces(0).unwrap(), 1);
        assert_eq!(rules.color_space(0, 0).unwrap(), "sRGB - Texture");

        assert_eq!(rules.name(1).unwrap(), "RULE_3");
        assert_eq!(rules.num_color_spaces(1).unwrap(), 2);
        assert_eq!(rules.color_space(1, 0).unwrap(), "Linear Rec.2020");
        assert_eq!(rules.color_space(1, 1).unwrap(), "ACEScct - SomeOtherName");

        assert_eq!(rules.name(2).unwrap(), "RULE_2");
        assert_eq!(rules.num_encodings(2).unwrap(), 1);
        assert_eq!(rules.encoding(2, 0).unwrap(), "scene-linear");

        // Validate virtual_display
        assert_eq!(
            merged.virtual_display_num_views(ViewType::DisplayDefined),
            3
        );
        assert_eq!(merged.virtual_display_num_views(ViewType::Shared), 2);

        assert_eq!(
            merged.virtual_display_view(ViewType::DisplayDefined, 0),
            "ACES"
        );
        assert_eq!(
            merged.virtual_display_view(ViewType::DisplayDefined, 1),
            "Lin"
        );
        assert_eq!(
            merged.virtual_display_view(ViewType::DisplayDefined, 2),
            "Log"
        );

        assert_eq!(merged.virtual_display_view(ViewType::Shared, 0), "SHARED_3");
        assert_eq!(merged.virtual_display_view(ViewType::Shared, 1), "SHARED_1");
    }

    // Test display/views with strategy=PreferInput, options InputFirst = false.
    {
        let mut params = setup(MergeStrategy::PreferInput);
        params.set_input_first(false);
        let merged = check_merge(LogType::Warning, &base, &input, &params, display_views, &["The Input config contains a value that would override the Base config: shared_views: SHARED_1",
            "The Input config contains a value that would override the Base config: display: DISP_1, view: VIEW_1",
            "The Input config contains a value that would override the Base config: viewing_rules: RULE_1"]);

        assert_eq!(merged.active_displays(), "DISP_1, DISP_2, DISP_3");
        assert_eq!(
            merged.active_views(),
            "SHARED_1, SHARED_2, VIEW_1, VIEW_2, SHARED_3, VIEW_3"
        );

        // Validate shared_views
        assert_eq!(merged.num_views_by_type(ViewType::Shared, ""), 3);
        assert_eq!(merged.view_by_type(ViewType::Shared, "", 0), "SHARED_1");
        assert_eq!(merged.view_by_type(ViewType::Shared, "", 1), "SHARED_2");
        assert_eq!(merged.view_by_type(ViewType::Shared, "", 2), "SHARED_3");

        // Validate displays
        assert_eq!(merged.num_displays_all(), 3);
        assert_eq!(merged.display(0), "DISP_1");
        assert_eq!(merged.display(1), "DISP_2");
        assert_eq!(merged.display(2), "DISP_3");

        // Validate display/views

        assert_eq!(
            merged.num_views_by_type(ViewType::DisplayDefined, "DISP_1"),
            1
        );
        assert_eq!(merged.num_views_by_type(ViewType::Shared, "DISP_1"), 3);
        assert_eq!(
            merged.view_by_type(ViewType::DisplayDefined, "DISP_1", 0),
            "VIEW_1"
        );
        // Make sure this is the right VIEW_1 by checking the colorspace.
        assert_eq!(
            merged.display_view_color_space_name("DISP_1", "VIEW_1"),
            "view_1B"
        );
        assert_eq!(merged.display_view_rule("DISP_1", "VIEW_1"), "RULE_3");

        assert_eq!(
            merged.view_by_type(ViewType::Shared, "DISP_1", 0),
            "SHARED_1"
        );
        assert_eq!(
            merged.display_view_color_space_name("DISP_1", "SHARED_1"),
            "lin_3"
        );

        assert_eq!(
            merged.view_by_type(ViewType::Shared, "DISP_1", 1),
            "SHARED_2"
        );
        assert_eq!(
            merged.display_view_color_space_name("DISP_1", "SHARED_2"),
            "<USE_DISPLAY_NAME>"
        );
        assert_eq!(
            merged.display_view_transform_name("DISP_1", "SHARED_2"),
            "SDR Video"
        );

        assert_eq!(
            merged.view_by_type(ViewType::Shared, "DISP_1", 2),
            "SHARED_3"
        );
        assert_eq!(
            merged.display_view_color_space_name("DISP_1", "SHARED_3"),
            "log_3"
        );

        assert_eq!(
            merged.num_views_by_type(ViewType::DisplayDefined, "DISP_2"),
            2
        );
        assert_eq!(
            merged.view_by_type(ViewType::DisplayDefined, "DISP_2", 0),
            "VIEW_1"
        );
        assert_eq!(
            merged.view_by_type(ViewType::DisplayDefined, "DISP_2", 1),
            "VIEW_2"
        );

        assert_eq!(
            merged.num_views_by_type(ViewType::DisplayDefined, "DISP_3"),
            2
        );
        assert_eq!(
            merged.view_by_type(ViewType::DisplayDefined, "DISP_3", 0),
            "VIEW_1"
        );
        assert_eq!(
            merged.view_by_type(ViewType::DisplayDefined, "DISP_3", 1),
            "VIEW_3"
        );
        assert_eq!(merged.display_view_looks("DISP_3", "VIEW_3"), "look_input");

        // Validate viewing_rules
        let rules = merged.viewing_rules();

        assert_eq!(rules.num_entries(), 3);

        assert_eq!(rules.name(0).unwrap(), "RULE_1");
        assert_eq!(rules.num_color_spaces(0).unwrap(), 1);
        assert_eq!(rules.color_space(0, 0).unwrap(), "sRGB - Texture");

        assert_eq!(rules.name(1).unwrap(), "RULE_2");
        assert_eq!(rules.num_encodings(1).unwrap(), 1);
        assert_eq!(rules.encoding(1, 0).unwrap(), "scene-linear");

        assert_eq!(rules.name(2).unwrap(), "RULE_3");
        assert_eq!(rules.num_color_spaces(2).unwrap(), 2);
        assert_eq!(rules.color_space(2, 0).unwrap(), "Linear Rec.2020");
        assert_eq!(rules.color_space(2, 1).unwrap(), "ACEScct - SomeOtherName");

        // Validate virtual_display
        assert_eq!(
            merged.virtual_display_num_views(ViewType::DisplayDefined),
            3
        );
        assert_eq!(merged.virtual_display_num_views(ViewType::Shared), 2);

        assert_eq!(
            merged.virtual_display_view(ViewType::DisplayDefined, 0),
            "ACES"
        );
        assert_eq!(
            merged.virtual_display_view(ViewType::DisplayDefined, 1),
            "Log"
        );
        assert_eq!(
            merged.virtual_display_view(ViewType::DisplayDefined, 2),
            "Lin"
        );

        assert_eq!(merged.virtual_display_view(ViewType::Shared, 0), "SHARED_1");
        assert_eq!(merged.virtual_display_view(ViewType::Shared, 1), "SHARED_3");
    }

    // Test display/views with strategy = PreferBase, options InputFirst = true.
    {
        let mut params = setup(MergeStrategy::PreferBase);
        params.set_input_first(true);
        let merged = check_merge(LogType::Warning, &base, &input, &params, display_views, &["The Input config contains a value that would override the Base config: shared_views: SHARED_1",
            "The Input config contains a value that would override the Base config: display: DISP_1, view: VIEW_1",
            "The Input config contains a value that would override the Base config: viewing_rules: RULE_1"]);

        assert_eq!(merged.active_displays(), "DISP_1, DISP_3, DISP_2");
        assert_eq!(
            merged.active_views(),
            "SHARED_1, SHARED_3, VIEW_1, VIEW_3, SHARED_2, VIEW_2"
        );

        // Validate shared_views
        assert_eq!(merged.num_views_by_type(ViewType::Shared, ""), 3);
        assert_eq!(merged.view_by_type(ViewType::Shared, "", 0), "SHARED_1");
        assert_eq!(merged.view_by_type(ViewType::Shared, "", 1), "SHARED_3");
        assert_eq!(merged.view_by_type(ViewType::Shared, "", 2), "SHARED_2");

        // Validate displays
        assert_eq!(merged.num_displays_all(), 3);
        assert_eq!(merged.display(0), "DISP_1");
        assert_eq!(merged.display(1), "DISP_3");
        assert_eq!(merged.display(2), "DISP_2");

        // Validate display/views

        assert_eq!(
            merged.num_views_by_type(ViewType::DisplayDefined, "DISP_1"),
            1
        );
        assert_eq!(merged.num_views_by_type(ViewType::Shared, "DISP_1"), 3);
        assert_eq!(
            merged.view_by_type(ViewType::DisplayDefined, "DISP_1", 0),
            "VIEW_1"
        );
        // Make sure this is the right VIEW_1 by checking the colorspace.
        assert_eq!(
            merged.display_view_color_space_name("DISP_1", "VIEW_1"),
            "view_1"
        );
        assert_eq!(merged.display_view_rule("DISP_1", "VIEW_1"), "RULE_1");

        assert_eq!(
            merged.view_by_type(ViewType::Shared, "DISP_1", 0),
            "SHARED_3"
        );
        assert_eq!(
            merged.display_view_color_space_name("DISP_1", "SHARED_3"),
            "log_3"
        );

        assert_eq!(
            merged.view_by_type(ViewType::Shared, "DISP_1", 1),
            "SHARED_1"
        );
        assert_eq!(
            merged.display_view_color_space_name("DISP_1", "SHARED_1"),
            "lin_1"
        );

        assert_eq!(
            merged.view_by_type(ViewType::Shared, "DISP_1", 2),
            "SHARED_2"
        );
        assert_eq!(
            merged.display_view_color_space_name("DISP_1", "SHARED_2"),
            "<USE_DISPLAY_NAME>"
        );
        assert_eq!(
            merged.display_view_transform_name("DISP_1", "SHARED_2"),
            "SDR Video"
        );

        assert_eq!(
            merged.num_views_by_type(ViewType::DisplayDefined, "DISP_3"),
            2
        );
        assert_eq!(
            merged.view_by_type(ViewType::DisplayDefined, "DISP_3", 0),
            "VIEW_1"
        );
        assert_eq!(
            merged.view_by_type(ViewType::DisplayDefined, "DISP_3", 1),
            "VIEW_3"
        );
        assert_eq!(merged.display_view_looks("DISP_3", "VIEW_3"), "look_input");

        assert_eq!(
            merged.num_views_by_type(ViewType::DisplayDefined, "DISP_2"),
            2
        );
        assert_eq!(
            merged.view_by_type(ViewType::DisplayDefined, "DISP_2", 0),
            "VIEW_1"
        );
        assert_eq!(merged.display_view_rule("DISP_2", "VIEW_1"), "RULE_2");
        assert_eq!(
            merged.view_by_type(ViewType::DisplayDefined, "DISP_2", 1),
            "VIEW_2"
        );
        assert_eq!(merged.display_view_looks("DISP_2", "VIEW_2"), "look_base");

        // Validate viewing_rules
        let rules = merged.viewing_rules();

        assert_eq!(rules.num_entries(), 3);

        assert_eq!(rules.name(0).unwrap(), "RULE_1");
        assert_eq!(rules.num_color_spaces(0).unwrap(), 1);
        assert_eq!(rules.color_space(0, 0).unwrap(), "Gamma 2.2 AP1 - Texture");

        assert_eq!(rules.name(1).unwrap(), "RULE_3");
        assert_eq!(rules.num_color_spaces(1).unwrap(), 2);
        assert_eq!(rules.color_space(1, 0).unwrap(), "Linear Rec.2020");
        assert_eq!(rules.color_space(1, 1).unwrap(), "ACEScct - SomeOtherName");

        assert_eq!(rules.name(2).unwrap(), "RULE_2");
        assert_eq!(rules.num_encodings(2).unwrap(), 1);
        assert_eq!(rules.encoding(2, 0).unwrap(), "scene-linear");

        // Validate virtual_display
        assert_eq!(
            merged.virtual_display_num_views(ViewType::DisplayDefined),
            3
        );
        assert_eq!(merged.virtual_display_num_views(ViewType::Shared), 2);

        assert_eq!(
            merged.virtual_display_view(ViewType::DisplayDefined, 0),
            "ACES"
        );
        assert_eq!(
            merged.virtual_display_view(ViewType::DisplayDefined, 1),
            "Lin"
        );
        assert_eq!(
            merged.virtual_display_view(ViewType::DisplayDefined, 2),
            "Log"
        );

        assert_eq!(merged.virtual_display_view(ViewType::Shared, 0), "SHARED_3");
        assert_eq!(merged.virtual_display_view(ViewType::Shared, 1), "SHARED_1");
    }

    // Test display/views with strategy = PreferBase, options InputFirst = false.
    {
        let mut params = setup(MergeStrategy::PreferBase);
        params.set_input_first(false);
        let merged = check_merge(LogType::Warning, &base, &input, &params, display_views, &["The Input config contains a value that would override the Base config: shared_views: SHARED_1",
            "The Input config contains a value that would override the Base config: display: DISP_1, view: VIEW_1",
            "The Input config contains a value that would override the Base config: viewing_rules: RULE_1"]);

        assert_eq!(merged.active_displays(), "DISP_1, DISP_2, DISP_3");
        assert_eq!(
            merged.active_views(),
            "SHARED_1, SHARED_2, VIEW_1, VIEW_2, SHARED_3, VIEW_3"
        );

        // Validate shared_views
        assert_eq!(merged.num_views_by_type(ViewType::Shared, ""), 3);
        assert_eq!(merged.view_by_type(ViewType::Shared, "", 0), "SHARED_1");
        assert_eq!(merged.view_by_type(ViewType::Shared, "", 1), "SHARED_2");
        assert_eq!(merged.view_by_type(ViewType::Shared, "", 2), "SHARED_3");

        // Validate displays
        assert_eq!(merged.num_displays_all(), 3);
        assert_eq!(merged.display(0), "DISP_1");
        assert_eq!(merged.display(1), "DISP_2");
        assert_eq!(merged.display(2), "DISP_3");

        // Validate display/views

        assert_eq!(
            merged.num_views_by_type(ViewType::DisplayDefined, "DISP_1"),
            1
        );
        assert_eq!(merged.num_views_by_type(ViewType::Shared, "DISP_1"), 3);
        assert_eq!(
            merged.view_by_type(ViewType::DisplayDefined, "DISP_1", 0),
            "VIEW_1"
        );
        // Make sure this is the right VIEW_1 by checking the colorspace.
        assert_eq!(
            merged.display_view_color_space_name("DISP_1", "VIEW_1"),
            "view_1"
        );
        assert_eq!(merged.display_view_rule("DISP_1", "VIEW_1"), "RULE_1");

        assert_eq!(
            merged.view_by_type(ViewType::Shared, "DISP_1", 0),
            "SHARED_1"
        );
        assert_eq!(
            merged.display_view_color_space_name("DISP_1", "SHARED_1"),
            "lin_1"
        );

        assert_eq!(
            merged.view_by_type(ViewType::Shared, "DISP_1", 1),
            "SHARED_2"
        );
        assert_eq!(
            merged.display_view_color_space_name("DISP_1", "SHARED_2"),
            "<USE_DISPLAY_NAME>"
        );
        assert_eq!(
            merged.display_view_transform_name("DISP_1", "SHARED_2"),
            "SDR Video"
        );

        assert_eq!(
            merged.view_by_type(ViewType::Shared, "DISP_1", 2),
            "SHARED_3"
        );
        assert_eq!(
            merged.display_view_color_space_name("DISP_1", "SHARED_3"),
            "log_3"
        );

        assert_eq!(
            merged.num_views_by_type(ViewType::DisplayDefined, "DISP_2"),
            2
        );
        assert_eq!(
            merged.view_by_type(ViewType::DisplayDefined, "DISP_2", 0),
            "VIEW_1"
        );
        assert_eq!(merged.display_view_rule("DISP_2", "VIEW_1"), "RULE_2");
        assert_eq!(
            merged.view_by_type(ViewType::DisplayDefined, "DISP_2", 1),
            "VIEW_2"
        );
        assert_eq!(merged.display_view_looks("DISP_2", "VIEW_2"), "look_base");

        assert_eq!(
            merged.num_views_by_type(ViewType::DisplayDefined, "DISP_3"),
            2
        );
        assert_eq!(
            merged.view_by_type(ViewType::DisplayDefined, "DISP_3", 0),
            "VIEW_1"
        );
        assert_eq!(
            merged.view_by_type(ViewType::DisplayDefined, "DISP_3", 1),
            "VIEW_3"
        );
        assert_eq!(merged.display_view_looks("DISP_3", "VIEW_3"), "look_input");

        // Validate viewing_rules
        let rules = merged.viewing_rules();

        assert_eq!(rules.num_entries(), 3);

        assert_eq!(rules.name(0).unwrap(), "RULE_1");
        assert_eq!(rules.num_color_spaces(0).unwrap(), 1);
        assert_eq!(rules.color_space(0, 0).unwrap(), "Gamma 2.2 AP1 - Texture");

        assert_eq!(rules.name(1).unwrap(), "RULE_2");
        assert_eq!(rules.num_encodings(1).unwrap(), 1);
        assert_eq!(rules.encoding(1, 0).unwrap(), "scene-linear");

        assert_eq!(rules.name(2).unwrap(), "RULE_3");
        assert_eq!(rules.num_color_spaces(2).unwrap(), 2);
        assert_eq!(rules.color_space(2, 0).unwrap(), "Linear Rec.2020");
        assert_eq!(rules.color_space(2, 1).unwrap(), "ACEScct - SomeOtherName");

        // Validate virtual_display
        assert_eq!(
            merged.virtual_display_num_views(ViewType::DisplayDefined),
            3
        );
        assert_eq!(merged.virtual_display_num_views(ViewType::Shared), 2);

        assert_eq!(
            merged.virtual_display_view(ViewType::DisplayDefined, 0),
            "ACES"
        );
        assert_eq!(
            merged.virtual_display_view(ViewType::DisplayDefined, 1),
            "Log"
        );
        assert_eq!(
            merged.virtual_display_view(ViewType::DisplayDefined, 2),
            "Lin"
        );

        assert_eq!(merged.virtual_display_view(ViewType::Shared, 0), "SHARED_1");
        assert_eq!(merged.virtual_display_view(ViewType::Shared, 1), "SHARED_3");
    }

    // Test display/views with strategy = BaseOnly.
    {
        let params = setup(MergeStrategy::BaseOnly);
        let merged = merge_section(&base, &input, &params, display_views);

        assert_eq!(merged.active_displays(), "DISP_1, DISP_2");
        assert_eq!(merged.active_views(), "SHARED_1, SHARED_2, VIEW_1, VIEW_2");

        // Validate shared_views
        assert_eq!(merged.num_views_by_type(ViewType::Shared, ""), 2);
        assert_eq!(merged.view_by_type(ViewType::Shared, "", 0), "SHARED_1");
        assert_eq!(merged.view_by_type(ViewType::Shared, "", 1), "SHARED_2");

        // Validate displays
        assert_eq!(merged.num_displays_all(), 2);
        assert_eq!(merged.display(0), "DISP_1");
        assert_eq!(merged.display(1), "DISP_2");

        // Validate display/views

        assert_eq!(
            merged.num_views_by_type(ViewType::DisplayDefined, "DISP_1"),
            1
        );
        assert_eq!(merged.num_views_by_type(ViewType::Shared, "DISP_1"), 2);
        assert_eq!(
            merged.view_by_type(ViewType::DisplayDefined, "DISP_1", 0),
            "VIEW_1"
        );
        // Make sure this is the right VIEW_1 by checking the colorspace.
        assert_eq!(
            merged.display_view_color_space_name("DISP_1", "VIEW_1"),
            "view_1"
        );
        assert_eq!(merged.display_view_rule("DISP_1", "VIEW_1"), "RULE_1");

        assert_eq!(
            merged.view_by_type(ViewType::Shared, "DISP_1", 0),
            "SHARED_1"
        );
        assert_eq!(
            merged.display_view_color_space_name("DISP_1", "SHARED_1"),
            "lin_1"
        );

        assert_eq!(
            merged.view_by_type(ViewType::Shared, "DISP_1", 1),
            "SHARED_2"
        );
        assert_eq!(
            merged.display_view_color_space_name("DISP_1", "SHARED_2"),
            "<USE_DISPLAY_NAME>"
        );
        assert_eq!(
            merged.display_view_transform_name("DISP_1", "SHARED_2"),
            "SDR Video"
        );

        assert_eq!(
            merged.num_views_by_type(ViewType::DisplayDefined, "DISP_2"),
            2
        );
        assert_eq!(
            merged.view_by_type(ViewType::DisplayDefined, "DISP_2", 0),
            "VIEW_1"
        );
        assert_eq!(merged.display_view_rule("DISP_2", "VIEW_1"), "RULE_2");
        assert_eq!(
            merged.view_by_type(ViewType::DisplayDefined, "DISP_2", 1),
            "VIEW_2"
        );
        assert_eq!(merged.display_view_looks("DISP_2", "VIEW_2"), "look_base");

        // Validate viewing_rules
        let rules = merged.viewing_rules();

        assert_eq!(rules.num_entries(), 2);

        assert_eq!(rules.name(0).unwrap(), "RULE_1");
        assert_eq!(rules.num_color_spaces(0).unwrap(), 1);
        assert_eq!(rules.color_space(0, 0).unwrap(), "Gamma 2.2 AP1 - Texture");

        assert_eq!(rules.name(1).unwrap(), "RULE_2");
        assert_eq!(rules.num_encodings(1).unwrap(), 1);
        assert_eq!(rules.encoding(1, 0).unwrap(), "scene-linear");

        // Validate virtual_display
        assert_eq!(
            merged.virtual_display_num_views(ViewType::DisplayDefined),
            2
        );
        assert_eq!(merged.virtual_display_num_views(ViewType::Shared), 1);

        assert_eq!(
            merged.virtual_display_view(ViewType::DisplayDefined, 0),
            "ACES"
        );
        assert_eq!(
            merged.virtual_display_view(ViewType::DisplayDefined, 1),
            "Log"
        );

        assert_eq!(merged.virtual_display_view(ViewType::Shared, 0), "SHARED_1");
    }

    // Test display/views with strategy = InputOnly.
    {
        let params = setup(MergeStrategy::InputOnly);
        let merged = merge_section(&base, &input, &params, display_views);

        assert_eq!(merged.active_displays(), "DISP_1, DISP_3");
        assert_eq!(merged.active_views(), "SHARED_1, SHARED_3, VIEW_1, VIEW_3");

        // Validate shared_views
        assert_eq!(merged.num_views_by_type(ViewType::Shared, ""), 2);
        assert_eq!(merged.view_by_type(ViewType::Shared, "", 0), "SHARED_1");
        assert_eq!(merged.view_by_type(ViewType::Shared, "", 1), "SHARED_3");

        // Validate displays
        assert_eq!(merged.num_displays_all(), 2);
        assert_eq!(merged.display(0), "DISP_1");
        assert_eq!(merged.display(1), "DISP_3");

        // Validate display/views

        assert_eq!(
            merged.num_views_by_type(ViewType::DisplayDefined, "DISP_1"),
            1
        );
        assert_eq!(merged.num_views_by_type(ViewType::Shared, "DISP_1"), 2);
        assert_eq!(
            merged.view_by_type(ViewType::DisplayDefined, "DISP_1", 0),
            "VIEW_1"
        );
        // Make sure this is the right VIEW_1 by checking the colorspace.
        assert_eq!(
            merged.display_view_color_space_name("DISP_1", "VIEW_1"),
            "view_1B"
        );
        assert_eq!(merged.display_view_rule("DISP_1", "VIEW_1"), "RULE_3");

        assert_eq!(
            merged.view_by_type(ViewType::Shared, "DISP_1", 0),
            "SHARED_3"
        );
        assert_eq!(
            merged.display_view_color_space_name("DISP_1", "SHARED_3"),
            "log_3"
        );

        assert_eq!(
            merged.view_by_type(ViewType::Shared, "DISP_1", 1),
            "SHARED_1"
        );
        assert_eq!(
            merged.display_view_color_space_name("DISP_1", "SHARED_1"),
            "lin_3"
        );

        assert_eq!(
            merged.num_views_by_type(ViewType::DisplayDefined, "DISP_3"),
            2
        );
        assert_eq!(
            merged.view_by_type(ViewType::DisplayDefined, "DISP_3", 0),
            "VIEW_1"
        );
        assert_eq!(
            merged.view_by_type(ViewType::DisplayDefined, "DISP_3", 1),
            "VIEW_3"
        );
        assert_eq!(merged.display_view_looks("DISP_3", "VIEW_3"), "look_input");

        // Validate viewing_rules
        let rules = merged.viewing_rules();

        assert_eq!(rules.num_entries(), 2);

        assert_eq!(rules.name(0).unwrap(), "RULE_1");
        assert_eq!(rules.num_color_spaces(0).unwrap(), 1);
        assert_eq!(rules.color_space(0, 0).unwrap(), "sRGB - Texture");

        assert_eq!(rules.name(1).unwrap(), "RULE_3");
        assert_eq!(rules.num_color_spaces(1).unwrap(), 2);
        assert_eq!(rules.color_space(1, 0).unwrap(), "Linear Rec.2020");
        assert_eq!(rules.color_space(1, 1).unwrap(), "ACEScct - SomeOtherName");

        // Validate virtual_display
        assert_eq!(
            merged.virtual_display_num_views(ViewType::DisplayDefined),
            2
        );
        assert_eq!(merged.virtual_display_num_views(ViewType::Shared), 1);

        assert_eq!(
            merged.virtual_display_view(ViewType::DisplayDefined, 0),
            "ACES"
        );
        assert_eq!(
            merged.virtual_display_view(ViewType::DisplayDefined, 1),
            "Lin"
        );

        assert_eq!(merged.virtual_display_view(ViewType::Shared, 0), "SHARED_3");
    }

    // Test display/views with strategy = Remove
    {
        let params = setup(MergeStrategy::Remove);
        let merged = merge_section(&base, &input, &params, display_views);

        assert_eq!(merged.active_displays(), "DISP_2");
        assert_eq!(merged.active_views(), "SHARED_2, VIEW_2");

        // Validate shared_views
        assert_eq!(merged.num_views_by_type(ViewType::Shared, ""), 1);
        assert_eq!(merged.view_by_type(ViewType::Shared, "", 0), "SHARED_2");

        // Validate displays
        assert_eq!(merged.num_displays_all(), 2);
        assert_eq!(merged.display_all(0), "DISP_1");
        assert_eq!(merged.display_all(1), "DISP_2");

        // Validate display/views
        assert_eq!(
            merged.num_views_by_type(ViewType::DisplayDefined, "DISP_1"),
            0
        );
        assert_eq!(merged.num_views_by_type(ViewType::Shared, "DISP_1"), 1);
        assert_eq!(
            merged.view_by_type(ViewType::Shared, "DISP_1", 0),
            "SHARED_2"
        );
        assert_eq!(
            merged.display_view_color_space_name("DISP_1", "SHARED_2"),
            "<USE_DISPLAY_NAME>"
        );
        assert_eq!(
            merged.display_view_transform_name("DISP_1", "SHARED_2"),
            "SDR Video"
        );

        assert_eq!(
            merged.num_views_by_type(ViewType::DisplayDefined, "DISP_2"),
            2
        );
        assert_eq!(
            merged.view_by_type(ViewType::DisplayDefined, "DISP_2", 0),
            "VIEW_1"
        );
        assert_eq!(merged.display_view_rule("DISP_2", "VIEW_1"), "RULE_2");
        assert_eq!(
            merged.view_by_type(ViewType::DisplayDefined, "DISP_2", 1),
            "VIEW_2"
        );
        assert_eq!(merged.display_view_looks("DISP_2", "VIEW_2"), "look_base");

        // Validate viewing_rules
        let rules = merged.viewing_rules();
        assert_eq!(rules.num_entries(), 1);
        assert_eq!(rules.name(0).unwrap(), "RULE_2");
        assert_eq!(rules.num_encodings(0).unwrap(), 1);
        assert_eq!(rules.encoding(0, 0).unwrap(), "scene-linear");

        // Validate virtual_display
        assert_eq!(
            merged.virtual_display_num_views(ViewType::DisplayDefined),
            1
        );
        assert_eq!(merged.virtual_display_num_views(ViewType::Shared), 1);

        assert_eq!(
            merged.virtual_display_view(ViewType::DisplayDefined, 0),
            "Log"
        );
        assert_eq!(merged.virtual_display_view(ViewType::Shared, 0), "SHARED_1");
    }

    // Test that error_on_conflicts is processed correctly.
    // strategy = PreferInput, InputFirst = false
    {
        let mut params = ConfigMergingParameters::new();
        params.set_display_views(MergeStrategy::PreferInput);
        params.set_input_first(false);
        params.set_error_on_conflict(true);

        const PREFIX: &str =
            "The Input config contains a value that would override the Base config: ";

        // Test that an error is thrown when the input config values are different.
        let msgs = [
            format!("{PREFIX}shared_views: SHARED_1"),
            format!("{PREFIX}display: DISP_1, views: VIEW_1"),
            format!("{PREFIX}viewing_rules: RULE_1"),
            format!("{PREFIX}virtual_display: ACES"),
        ];
        let msgs: Vec<&str> = msgs.iter().map(|s| s.as_str()).collect();
        check_merge(LogType::Error, &base, &input, &params, display_views, &msgs);
    }
}

/// Style of the builtin `from_reference` transform of a view transform.
#[track_caller]
fn builtin_style(config: &Config, vt: &str) -> String {
    match config
        .view_transform(vt)
        .unwrap()
        .transform(ViewTransformDirection::FromReference)
    {
        Some(crate::Transform::Builtin(b)) => b.style.clone(),
        other => panic!("expected a builtin transform, got {other:?}"),
    }
}

#[test]
fn merge_configs_view_transforms_section() {
    let base = get_base_config();
    let input = get_input_config();

    let setup = |strategy: MergeStrategy| -> ConfigMergingParameters {
        let mut p = ConfigMergingParameters::new();
        p.set_view_transforms(strategy);
        p
    };

    // Allowed strategies: All
    // Allowed merge options: All

    // Test that the default strategy is used as a fallback if the section strategy was not defined.
    {
        // Using STRATEGY_UNSPECIFIED as this simulates that the section
        // is missing from the OCIOM file.
        let mut params = setup(MergeStrategy::Unspecified);
        // Simulate settings from OCIOM file.
        params.set_default_strategy(MergeStrategy::InputOnly);
        params.set_input_first(true);
        let merged = merge_section(&base, &input, &params, view_transforms);

        assert_eq!(merged.default_view_transform_name(), "Un-tone-mapped-2");

        // Validate view_transforms

        assert_eq!(merged.num_view_transforms(), 3);
        assert_eq!(merged.view_transform_name_by_index(0), "SDR Video");
        assert_eq!(merged.view_transform_name_by_index(1), "Un-tone-mapped-2");
        assert_eq!(merged.view_transform_name_by_index(2), "Equal");
    }

    // Test display/views with strategy = PreferInput, options InputFirst = true.
    {
        let mut params = setup(MergeStrategy::PreferInput);
        params.set_input_first(true);
        let merged = check_merge(LogType::Warning, &base, &input, &params, view_transforms, &["The Input config contains a value that would override the Base config: view_transforms: SDR Video",
            "The Input config contains a value that would override the Base config: default_view_transform: Un-tone-mapped-2"]);

        // Validate default_view_transform
        assert_eq!(merged.default_view_transform_name(), "Un-tone-mapped-2");

        // Validate view_transforms

        assert_eq!(merged.num_view_transforms(), 4);
        assert_eq!(merged.view_transform_name_by_index(0), "SDR Video");
        assert_eq!(
            builtin_style(&merged, "SDR Video"),
            "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-VIDEO-P3lim_1.1"
        );

        assert_eq!(merged.view_transform_name_by_index(1), "Un-tone-mapped-2");
        assert_eq!(merged.view_transform_name_by_index(2), "Equal");
        assert_eq!(merged.view_transform_name_by_index(3), "Un-tone-mapped");
        assert_eq!(
            merged.view_transform("Equal").unwrap().description(),
            "from input"
        );
    }

    // Test display/views with strategy=PreferInput, options InputFirst = false.
    {
        let mut params = setup(MergeStrategy::PreferInput);
        params.set_input_first(false);
        let merged = check_merge(LogType::Warning, &base, &input, &params, view_transforms, &["The Input config contains a value that would override the Base config: view_transforms: SDR Video",
            "The Input config contains a value that would override the Base config: default_view_transform: Un-tone-mapped-2"]);

        assert_eq!(merged.default_view_transform_name(), "Un-tone-mapped-2");

        // Validate view_transforms

        assert_eq!(merged.num_view_transforms(), 4);
        assert_eq!(merged.view_transform_name_by_index(0), "SDR Video");
        assert_eq!(
            builtin_style(&merged, "SDR Video"),
            "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-VIDEO-P3lim_1.1"
        );

        assert_eq!(merged.view_transform_name_by_index(1), "Un-tone-mapped");
        assert_eq!(merged.view_transform_name_by_index(2), "Equal");
        assert_eq!(merged.view_transform_name_by_index(3), "Un-tone-mapped-2");
    }

    // Test display/views with strategy = PreferBase, options InputFirst = true.
    {
        let mut params = setup(MergeStrategy::PreferBase);
        params.set_input_first(true);
        let merged = check_merge(LogType::Warning, &base, &input, &params, view_transforms, &["The Input config contains a value that would override the Base config: view_transforms: SDR Video",
            "The Input config contains a value that would override the Base config: default_view_transform: Un-tone-mapped-2"]);

        assert_eq!(merged.default_view_transform_name(), "SDR Video");

        // Validate view_transforms

        assert_eq!(merged.num_view_transforms(), 4);
        assert_eq!(merged.view_transform_name_by_index(0), "SDR Video");
        assert_eq!(
            builtin_style(&merged, "SDR Video"),
            "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-VIDEO_1.0"
        );

        assert_eq!(merged.view_transform_name_by_index(1), "Un-tone-mapped-2");
        assert_eq!(merged.view_transform_name_by_index(2), "Equal");
        assert_eq!(merged.view_transform_name_by_index(3), "Un-tone-mapped");
        assert_eq!(
            merged.view_transform("Equal").unwrap().description(),
            "from base"
        );
    }

    // Test display/views with strategy = PreferBase, options InputFirst = false.
    {
        let mut params = setup(MergeStrategy::PreferBase);
        params.set_input_first(false);
        let merged = check_merge(LogType::Warning, &base, &input, &params, view_transforms, &["The Input config contains a value that would override the Base config: view_transforms: SDR Video",
            "The Input config contains a value that would override the Base config: default_view_transform: Un-tone-mapped-2"]);

        assert_eq!(merged.default_view_transform_name(), "SDR Video");

        // Validate view_transforms

        assert_eq!(merged.num_view_transforms(), 4);
        assert_eq!(merged.view_transform_name_by_index(0), "SDR Video");
        assert_eq!(
            builtin_style(&merged, "SDR Video"),
            "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-VIDEO_1.0"
        );

        assert_eq!(merged.view_transform_name_by_index(1), "Un-tone-mapped");
        assert_eq!(merged.view_transform_name_by_index(2), "Equal");
        assert_eq!(merged.view_transform_name_by_index(3), "Un-tone-mapped-2");
    }

    // Test display/views with strategy = BaseOnly.
    {
        let params = setup(MergeStrategy::BaseOnly);
        let merged = merge_section(&base, &input, &params, view_transforms);

        assert_eq!(merged.default_view_transform_name(), "SDR Video");

        // Validate view_transforms

        assert_eq!(merged.num_view_transforms(), 3);
        assert_eq!(merged.view_transform_name_by_index(0), "SDR Video");
        assert_eq!(merged.view_transform_name_by_index(1), "Un-tone-mapped");
        assert_eq!(merged.view_transform_name_by_index(2), "Equal");
    }

    // Test display/views with strategy = InputOnly.
    {
        let params = setup(MergeStrategy::InputOnly);
        let merged = merge_section(&base, &input, &params, view_transforms);

        assert_eq!(merged.default_view_transform_name(), "Un-tone-mapped-2");

        // Validate view_transforms

        assert_eq!(merged.num_view_transforms(), 3);
        assert_eq!(merged.view_transform_name_by_index(0), "SDR Video");
        assert_eq!(
            builtin_style(&merged, "SDR Video"),
            "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-VIDEO-P3lim_1.1"
        );

        assert_eq!(merged.view_transform_name_by_index(1), "Un-tone-mapped-2");
        assert_eq!(merged.view_transform_name_by_index(2), "Equal");
    }

    // Test display/views with strategy = Remove
    {
        let params = setup(MergeStrategy::Remove);
        let merged = merge_section(&base, &input, &params, view_transforms);

        // Note that the "SDR Video" view transform was removed, so the "SDR Video" value
        // of the default view transform was reset to empty (will use the first one by default).
        assert_eq!(merged.default_view_transform_name(), "");

        // Validate view_transforms

        assert_eq!(merged.num_view_transforms(), 1);
        assert_eq!(merged.view_transform_name_by_index(0), "Un-tone-mapped");
    }

    // Test that error_on_conflicts is processed correctly.
    // strategy = PreferInput, InputFirst = false
    {
        let mut params = ConfigMergingParameters::new();
        params.set_view_transforms(MergeStrategy::PreferInput);
        params.set_input_first(false);
        params.set_error_on_conflict(true);

        const PREFIX: &str =
            "The Input config contains a value that would override the Base config: ";

        // Test that an error is thrown when the input values are different.
        let msgs = [
            format!("{PREFIX}view_transforms: SDR Video"),
            format!("{PREFIX}default_view_transform: Un-tone-mapped-2"),
        ];
        let msgs: Vec<&str> = msgs.iter().map(|s| s.as_str()).collect();
        check_merge(
            LogType::Error,
            &base,
            &input,
            &params,
            view_transforms,
            &msgs,
        );
    }
}

#[test]
fn merge_configs_looks_section() {
    let base = get_base_config();
    let input = get_input_config();

    let setup_looks = |strategy: MergeStrategy, cb: &dyn Fn(&mut ConfigMergingParameters)| {
        let mut params = ConfigMergingParameters::new();
        params.set_looks(strategy);
        params.set_input_family_prefix("Input/");
        params.set_base_family_prefix("Base/");
        cb(&mut params);
        merge_section(&base, &input, &params, looks)
    };

    let check = |merged: &Config, expected: &[(&str, &str)]| {
        assert_eq!(merged.num_looks(), expected.len());
        for (i, (name, ps)) in expected.iter().enumerate() {
            assert_eq!(merged.look_name_by_index(i), *name);
            assert_eq!(
                merged
                    .look(merged.look_name_by_index(i))
                    .unwrap()
                    .process_space(),
                *ps
            );
        }
    };

    // Test that the default strategy is used as a fallback if the section strategy was not
    // defined.
    {
        let merged = setup_looks(MergeStrategy::Unspecified, &|p| {
            // Simulate settings from OCIOM file.
            p.set_default_strategy(MergeStrategy::InputOnly);
        });
        check(
            &merged,
            &[
                ("look_both", "ACEScct - SomeOtherName"),
                ("look_input", "log_3"),
            ],
        );
    }

    // Test Looks with strategy = PreferInput, options InputFirst = true.
    {
        let merged = setup_looks(MergeStrategy::PreferInput, &|p| {
            p.set_adjust_input_reference_space(false);
        });
        check(
            &merged,
            &[
                ("look_both", "ACEScct - SomeOtherName"),
                ("look_input", "log_3"),
                ("look_base", "log_1"),
            ],
        );
    }

    // Test Looks with strategy=PreferInput, options InputFirst = false.
    {
        let merged = setup_looks(MergeStrategy::PreferInput, &|p| {
            p.set_input_first(false);
            p.set_adjust_input_reference_space(false);
        });
        check(
            &merged,
            &[
                ("look_both", "ACEScct - SomeOtherName"),
                ("look_base", "log_1"),
                ("look_input", "log_3"),
            ],
        );
    }

    // Test Looks with strategy = PreferBase, options InputFirst = true.
    {
        let merged = setup_looks(MergeStrategy::PreferBase, &|p| {
            p.set_input_first(true);
            p.set_adjust_input_reference_space(false);
        });
        check(
            &merged,
            &[
                ("look_both", "ACES2065-1"),
                ("look_input", "log_3"),
                ("look_base", "log_1"),
            ],
        );
    }

    // Test Looks with strategy = PreferBase, options InputFirst = false.
    {
        let merged = setup_looks(MergeStrategy::PreferBase, &|p| {
            p.set_input_first(false);
            p.set_adjust_input_reference_space(false);
        });
        check(
            &merged,
            &[
                ("look_both", "ACES2065-1"),
                ("look_base", "log_1"),
                ("look_input", "log_3"),
            ],
        );
    }

    // Test Looks with strategy = BaseOnly.
    {
        let merged = setup_looks(MergeStrategy::BaseOnly, &|_| {});
        check(
            &merged,
            &[("look_both", "ACES2065-1"), ("look_base", "log_1")],
        );
    }

    // Test Looks with strategy = InputOnly.
    {
        let merged = setup_looks(MergeStrategy::InputOnly, &|_| {});
        check(
            &merged,
            &[
                ("look_both", "ACEScct - SomeOtherName"),
                ("look_input", "log_3"),
            ],
        );
    }

    // Test Looks with strategy = Remove.
    {
        let merged = setup_looks(MergeStrategy::Remove, &|_| {});
        check(&merged, &[("look_base", "log_1")]);
    }
}
