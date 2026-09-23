//! Port of `MergeConfigsHelpers_tests.cpp` (part 3: color spaces section).

use super::tests::*;
use super::*;
use crate::types::*;

#[test]
fn merge_configs_colorspaces_section() {
    let base = get_config("base_colorspaces_config.yaml");
    let input = get_config("input_colorspaces_config.yaml");

    let setup = |strategy: MergeStrategy| -> ConfigMergingParameters {
        let mut p = ConfigMergingParameters::new();
        p.set_colorspaces(strategy);
        p.set_default_strategy(strategy);
        p.set_input_family_prefix("Input/");
        p.set_base_family_prefix("Base/");
        p.set_adjust_input_reference_space(false);
        p.set_avoid_duplicates(false);
        p
    };

    // Test that the default strategy is used as a fallback if the section strategy was not defined.
    {
        // Using STRATEGY_UNSPECIFIED as this simulates that the section is missing from the OCIOM file.
        let mut params = setup(MergeStrategy::Unspecified);
        // Simulate settings from OCIOM file.
        params.set_default_strategy(MergeStrategy::InputOnly);
        let merged = merge_section(&base, &input, &params, colorspaces);

        assert_eq!(merged.family_separator(), '~');

        let expected_names = ["test", "test3"];
        let expected_values = ["differentValue", "value3"];
        compare_environment_var(&merged, &expected_names, &expected_values);

        assert_eq!(merged.search_path(), ".:def");
        assert_eq!(merged.inactive_color_spaces(), "ACES2065-1, sRGB - Display");
    }

    // Test Colorspaces with strategy = PreferInput, options InputFirst = true.
    {
        let params = setup(MergeStrategy::PreferInput);
        let merged = check_merge(LogType::Warning, &base, &input, &params, colorspaces, &["Color space 'sRGB - Display' will replace a color space in the base config",
            "Color space 'look' will replace a color space in the base config",
            "Merged color space 'look' has a different reference space type than the color space it's replacing",
            "Merged color space 'ACES2065-1' has a conflict with alias 'aces' in color space 'ACEScg'",
            "The name of merged color space 'sRGB' has a conflict with an alias in color space 'sRGB - Texture'"]);

        assert_eq!(merged.family_separator(), '~');

        // Note that the environment vars are always written in alphabetical order,
        // so the InputFirst directive doesn't apply to this specific element.
        let expected_names = ["test", "test1", "test3"];
        let expected_values = ["differentValue", "value1", "value3"];
        compare_environment_var(&merged, &expected_names, &expected_values);

        assert_eq!(merged.search_path(), ".:def:abc");
        assert_eq!(
            merged.inactive_color_spaces(),
            "ACES2065-1, sRGB - Display, sRGB - Texture, ACEScg"
        );

        assert_eq!(
            merged.num_color_spaces_filtered(
                SearchReferenceSpaceType::All,
                ColorSpaceVisibility::All
            ),
            6
        );

        // Display colorspaces.

        let cs = check_color_space(
            &merged,
            "sRGB - Display",
            0,
            SearchReferenceSpaceType::Display,
        );
        assert_eq!(cs.num_aliases(), 1);
        assert_eq!(cs.alias(0), "srgb_display");
        assert_eq!(cs.family(), "Input~Display~Standard");
        assert_eq!(cs.description(), "from input");

        let cs = check_color_space(&merged, "look", 1, SearchReferenceSpaceType::Display);
        assert_eq!(cs.num_aliases(), 1);
        assert_eq!(cs.alias(0), "look1");
        assert_eq!(cs.description(), "from input");

        // Scene colorspaces.

        let cs = check_color_space(&merged, "ACES2065-1", 0, SearchReferenceSpaceType::Scene);
        assert_eq!(cs.num_aliases(), 1);
        assert_eq!(cs.alias(0), "aces");
        assert_eq!(cs.family(), "Input~ACES~Linear");

        let cs = check_color_space(&merged, "sRGB", 1, SearchReferenceSpaceType::Scene);
        assert_eq!(cs.num_aliases(), 1);
        assert_eq!(cs.alias(0), "my_srgb");
        assert_eq!(cs.family(), "Input~Texture~");

        let cs = check_color_space(&merged, "ACEScg", 2, SearchReferenceSpaceType::Scene);
        assert_eq!(cs.num_aliases(), 0);
        assert_eq!(cs.family(), "Base~ACES~Linear");

        let cs = check_color_space(
            &merged,
            "sRGB - Texture",
            3,
            SearchReferenceSpaceType::Scene,
        );
        // Note "srgb" is removed as an alias since it is a color space name in the input config.
        assert_eq!(cs.num_aliases(), 1);
        assert_eq!(cs.alias(0), "srgb_tx");
        assert_eq!(cs.family(), "Base~Texture");
        assert_eq!(cs.description(), "from base");

        // Note that the "look" scene color space is not merged since there is already a display color space
        // with that name.
    }

    // Test Colorspaces with strategy=PreferInput, options InputFirst = false.
    {
        let mut params = setup(MergeStrategy::PreferInput);
        params.set_input_first(false);
        let merged = check_merge(LogType::Warning, &base, &input, &params, colorspaces, &["Color space 'sRGB - Display' will replace a color space in the base config",
            "Color space 'look' will replace a color space in the base config",
            "Merged color space 'look' has a different reference space type than the color space it's replacing",
            "Merged color space 'ACES2065-1' has a conflict with alias 'aces' in color space 'ACEScg'",
            "The name of merged color space 'sRGB' has a conflict with an alias in color space 'sRGB - Texture'"]);

        assert_eq!(merged.family_separator(), '~');

        let expected_names = ["test", "test1", "test3"];
        let expected_values = ["differentValue", "value1", "value3"];
        compare_environment_var(&merged, &expected_names, &expected_values);

        // Note that the search path ignores InputFirst, it works based on the strategy only.
        assert_eq!(merged.search_path(), ".:def:abc");
        assert_eq!(
            merged.inactive_color_spaces(),
            "sRGB - Texture, sRGB - Display, ACEScg, ACES2065-1"
        );

        assert_eq!(
            merged.num_color_spaces_filtered(
                SearchReferenceSpaceType::All,
                ColorSpaceVisibility::All
            ),
            6
        );

        // Display colorspaces.

        let cs = check_color_space(
            &merged,
            "sRGB - Display",
            0,
            SearchReferenceSpaceType::Display,
        );
        assert_eq!(cs.alias(0), "srgb_display");
        assert_eq!(cs.family(), "Input~Display~Standard");
        assert_eq!(cs.description(), "from input");

        let cs = check_color_space(&merged, "look", 1, SearchReferenceSpaceType::Display);
        assert_eq!(cs.alias(0), "look1");
        assert_eq!(cs.description(), "from input");

        // Scene colorspaces.

        let cs = check_color_space(&merged, "ACEScg", 0, SearchReferenceSpaceType::Scene);
        assert_eq!(cs.num_aliases(), 0);
        assert_eq!(cs.family(), "Base~ACES~Linear");

        let cs = check_color_space(
            &merged,
            "sRGB - Texture",
            1,
            SearchReferenceSpaceType::Scene,
        );
        assert_eq!(cs.num_aliases(), 1);
        assert_eq!(cs.alias(0), "srgb_tx");
        assert_eq!(cs.family(), "Base~Texture");
        assert_eq!(cs.description(), "from base");

        let cs = check_color_space(&merged, "ACES2065-1", 2, SearchReferenceSpaceType::Scene);
        assert_eq!(cs.alias(0), "aces");
        assert_eq!(cs.family(), "Input~ACES~Linear");

        let cs = check_color_space(&merged, "sRGB", 3, SearchReferenceSpaceType::Scene);
        assert_eq!(cs.alias(0), "my_srgb");
        assert_eq!(cs.family(), "Input~Texture~");
    }

    // Test Colorspaces with strategy = PreferBase, options InputFirst = true.
    {
        let params = setup(MergeStrategy::PreferBase);
        let merged = check_merge(LogType::Warning, &base, &input, &params, colorspaces, &["Color space 'sRGB - Display' was not merged as it's already present in the base config",
            "Color space 'look' was not merged as it's already present in the base config",
            "Merged color space 'ACES2065-1' has a conflict with alias 'aces' in color space 'ACEScg'",
            "Color space 'sRGB' was not merged as it conflicts with an alias in color space 'sRGB - Texture'"]);

        assert_eq!(merged.family_separator(), '#');

        let expected_names = ["test", "test1", "test3"];
        let expected_values = ["value", "value1", "value3"];
        compare_environment_var(&merged, &expected_names, &expected_values);

        // Note that the search path ignores InputFirst, it works based on the strategy only.
        assert_eq!(merged.search_path(), ".:abc:def");
        assert_eq!(
            merged.inactive_color_spaces(),
            "ACES2065-1, sRGB - Display, sRGB - Texture, ACEScg"
        );

        assert_eq!(
            merged.num_color_spaces_filtered(
                SearchReferenceSpaceType::All,
                ColorSpaceVisibility::All
            ),
            5
        );

        // Display colorspaces.

        let cs = check_color_space(
            &merged,
            "sRGB - Display",
            0,
            SearchReferenceSpaceType::Display,
        );
        assert_eq!(cs.alias(0), "srgb_display");
        assert_eq!(cs.family(), "Base#Display#Basic");
        assert_eq!(cs.description(), "from base");

        // Scene colorspaces.

        let cs = check_color_space(&merged, "ACES2065-1", 0, SearchReferenceSpaceType::Scene);
        assert_eq!(cs.num_aliases(), 0);
        assert_eq!(cs.family(), "Input#ACES#Linear");
        assert_eq!(cs.description(), "from input");

        let cs = check_color_space(&merged, "ACEScg", 1, SearchReferenceSpaceType::Scene);
        assert_eq!(cs.num_aliases(), 1);
        assert_eq!(cs.alias(0), "aces");
        assert_eq!(cs.family(), "Base#ACES#Linear");

        let cs = check_color_space(
            &merged,
            "sRGB - Texture",
            2,
            SearchReferenceSpaceType::Scene,
        );
        assert_eq!(cs.num_aliases(), 2);
        assert_eq!(cs.alias(0), "srgb");
        assert_eq!(cs.alias(1), "srgb_tx");
        assert_eq!(cs.family(), "Base#Texture");
        assert_eq!(cs.description(), "from base");

        let cs = check_color_space(&merged, "look", 3, SearchReferenceSpaceType::Scene);
        assert_eq!(cs.num_aliases(), 0);
        assert_eq!(cs.description(), "from base");
    }

    // Test Colorspaces with strategy = PreferBase, options InputFirst = false.
    {
        let mut params = setup(MergeStrategy::PreferBase);
        params.set_input_first(false);
        let merged = check_merge(LogType::Warning, &base, &input, &params, colorspaces, &["Color space 'sRGB - Display' was not merged as it's already present in the base config",
            "Color space 'look' was not merged as it's already present in the base config",
            "Merged color space 'ACES2065-1' has a conflict with alias 'aces' in color space 'ACEScg'",
            "Color space 'sRGB' was not merged as it conflicts with an alias in color space 'sRGB - Texture'"]);

        assert_eq!(merged.family_separator(), '#');

        let expected_names = ["test", "test1", "test3"];
        let expected_values = ["value", "value1", "value3"];
        compare_environment_var(&merged, &expected_names, &expected_values);

        assert_eq!(merged.search_path(), ".:abc:def");
        assert_eq!(
            merged.inactive_color_spaces(),
            "sRGB - Texture, sRGB - Display, ACEScg, ACES2065-1"
        );

        assert_eq!(
            merged.num_color_spaces_filtered(
                SearchReferenceSpaceType::All,
                ColorSpaceVisibility::All
            ),
            5
        );

        // Display colorspaces.

        let cs = check_color_space(
            &merged,
            "sRGB - Display",
            0,
            SearchReferenceSpaceType::Display,
        );
        assert_eq!(cs.alias(0), "srgb_display");
        assert_eq!(cs.family(), "Base#Display#Basic");
        assert_eq!(cs.description(), "from base");

        // Scene colorspaces.

        let cs = check_color_space(&merged, "ACEScg", 0, SearchReferenceSpaceType::Scene);
        assert_eq!(cs.alias(0), "aces");

        let cs = check_color_space(
            &merged,
            "sRGB - Texture",
            1,
            SearchReferenceSpaceType::Scene,
        );
        assert_eq!(cs.num_aliases(), 2);
        assert_eq!(cs.alias(0), "srgb");
        assert_eq!(cs.alias(1), "srgb_tx");
        assert_eq!(cs.family(), "Base#Texture");

        let cs = check_color_space(&merged, "look", 2, SearchReferenceSpaceType::Scene);
        assert_eq!(cs.num_aliases(), 0);
        assert_eq!(cs.description(), "from base");

        let cs = check_color_space(&merged, "ACES2065-1", 3, SearchReferenceSpaceType::Scene);
        assert_eq!(cs.num_aliases(), 0);
    }

    // Test Colorspaces with strategy = BaseOnly.
    {
        let params = setup(MergeStrategy::BaseOnly);
        let merged = merge_section(&base, &input, &params, colorspaces);

        assert_eq!(merged.family_separator(), '#');

        let expected_names = ["test", "test1"];
        let expected_values = ["value", "value1"];
        compare_environment_var(&merged, &expected_names, &expected_values);

        assert_eq!(merged.search_path(), ".:abc");
        assert_eq!(
            merged.inactive_color_spaces(),
            "sRGB - Texture, sRGB - Display, ACEScg"
        );

        assert_eq!(
            merged.num_color_spaces_filtered(
                SearchReferenceSpaceType::All,
                ColorSpaceVisibility::All
            ),
            4
        );

        let cs = check_color_space(
            &merged,
            "sRGB - Display",
            0,
            SearchReferenceSpaceType::Display,
        );
        assert_eq!(cs.family(), "Display#Basic");

        let cs = check_color_space(&merged, "ACEScg", 0, SearchReferenceSpaceType::Scene);
        assert_eq!(cs.family(), "ACES#Linear");

        let cs = check_color_space(
            &merged,
            "sRGB - Texture",
            1,
            SearchReferenceSpaceType::Scene,
        );
        assert_eq!(cs.family(), "Texture");

        let cs = check_color_space(&merged, "look", 2, SearchReferenceSpaceType::Scene);
        assert_eq!(cs.family(), "");
    }

    // Test Colorspaces with strategy = InputOnly.
    {
        let params = setup(MergeStrategy::InputOnly);
        let merged = merge_section(&base, &input, &params, colorspaces);

        assert_eq!(merged.family_separator(), '~');

        let expected_names = ["test", "test3"];
        let expected_values = ["differentValue", "value3"];
        compare_environment_var(&merged, &expected_names, &expected_values);

        assert_eq!(merged.search_path(), ".:def");
        assert_eq!(merged.inactive_color_spaces(), "ACES2065-1, sRGB - Display");

        assert_eq!(
            merged.num_color_spaces_filtered(
                SearchReferenceSpaceType::All,
                ColorSpaceVisibility::All
            ),
            4
        );

        let cs = check_color_space(
            &merged,
            "sRGB - Display",
            0,
            SearchReferenceSpaceType::Display,
        );
        assert_eq!(cs.family(), "Display~Standard");

        let cs = check_color_space(&merged, "look", 1, SearchReferenceSpaceType::Display);
        assert_eq!(cs.family(), "Display~Standard");

        let cs = check_color_space(&merged, "ACES2065-1", 0, SearchReferenceSpaceType::Scene);
        assert_eq!(cs.family(), "ACES~Linear");

        let cs = check_color_space(&merged, "sRGB", 1, SearchReferenceSpaceType::Scene);
        assert_eq!(cs.family(), "Texture~");
    }

    // Test Colorspaces with strategy = Remove
    {
        let params = setup(MergeStrategy::Remove);
        let merged = merge_section(&base, &input, &params, colorspaces);

        assert_eq!(merged.family_separator(), '#');

        let expected_names = ["test1"];
        let expected_values = ["value1"];
        compare_environment_var(&merged, &expected_names, &expected_values);

        assert_eq!(merged.search_path(), "abc");
        assert_eq!(merged.inactive_color_spaces(), "sRGB - Texture, ACEScg");

        assert_eq!(
            merged.num_color_spaces_filtered(
                SearchReferenceSpaceType::All,
                ColorSpaceVisibility::All
            ),
            2
        );

        let cs = check_color_space(&merged, "ACEScg", 0, SearchReferenceSpaceType::Scene);
        assert_eq!(cs.family(), "ACES#Linear");

        let cs = check_color_space(
            &merged,
            "sRGB - Texture",
            1,
            SearchReferenceSpaceType::Scene,
        );
        assert_eq!(cs.family(), "Texture");
    }
}

#[test]
fn merge_configs_colorspaces_section_common_reference_and_duplicates() {
    // Base config display ref space: CIE-XYZ-D65, scene ref space: ACES2065-1.
    // Input config display ref space: linear Rec.709, scene ref space: linear Rec.709
    //
    // Both configs have the role: cie_xyz_d65_interchange: CIE-XYZ-D65 but not the
    // aces_interchange role, so heuristics will be used for that.
    //
    // The merged configs will contain color spaces from the input config where the reference
    // space has been converted to that of the base config. The base reference spaces are always
    // used, regardless of strategy.
    //
    // Duplicates are removed, even though they use different reference spaces.

    let base = get_config("merged1/base1.ocio");
    let input = get_config("merged1/input1.ocio");

    let setup = |strategy: MergeStrategy| -> ConfigMergingParameters {
        let mut p = ConfigMergingParameters::new();
        p.set_colorspaces(strategy);
        p.set_default_strategy(strategy);
        p.set_input_family_prefix("Input/");
        p.set_base_family_prefix("Base/");
        p.set_adjust_input_reference_space(true);
        p.set_avoid_duplicates(true);
        p
    };

    // PreferInput, Input first.
    {
        let mut params = setup(MergeStrategy::PreferInput);
        params.set_input_first(true);
        let merged = check_merge(LogType::Warning, &base, &input, &params, |o: &mut MergeHandlerOptions| -> Result<()> { roles(o)?; display_views(o)?; view_transforms(o)?; colorspaces(o)?; Ok(()) }, &["Equivalent input color space 'sRGB - Display' replaces 'sRGB - Display' in the base config, preserving aliases.",
            "Equivalent input color space 'CIE-XYZ-D65' replaces 'CIE-XYZ-D65' in the base config, preserving aliases.",
            "Equivalent input color space 'ACES2065-1' replaces 'ap0' in the base config, preserving aliases.",
            "Equivalent input color space 'sRGB' replaces 'sRGB - Texture' in the base config, preserving aliases.",
            "Merged color space 'ACES2065-1' has a conflict with alias 'aces' in color space 'ACEScg'"]);

        assert_eq!(merged.num_roles(), 1);
        assert_eq!(
            merged.role_color_space("cie_xyz_d65_interchange"),
            "CIE-XYZ-D65"
        );

        assert_eq!(
            merged.num_color_spaces_filtered(
                SearchReferenceSpaceType::All,
                ColorSpaceVisibility::All
            ),
            7
        );

        // Display-referred spaces.
        {
            // This is a duplicate.
            let cs = check_color_space(
                &merged,
                "sRGB - Display",
                0,
                SearchReferenceSpaceType::Display,
            );
            assert_eq!(cs.num_aliases(), 1);
            // Check for alias srgb_display (added from base config).
            assert_eq!(cs.alias(0), "srgb_display");
            assert_eq!(cs.description(), "from input");
            assert_eq!(cs.encoding(), "sdr-video");
            // Check categories were copied.
            assert_eq!(cs.num_categories(), 3);
            assert!(cs.has_category("file-io"));
            assert!(cs.has_category("texture"));
            assert!(cs.has_category("display"));

            // Check that the input config reference space was converted to the base reference space.
            // See ConfigUtils_tests.cpp for more detailed testing of the reference space conversion.
            {
                assert!(cs.transform(ColorSpaceDirection::ToReference).is_none());
                check_group(
                    cs.transform(ColorSpaceDirection::FromReference),
                    &[TransformType::Matrix, TransformType::ExponentWithLinear],
                );
            }

            let cs1 =
                check_color_space(&merged, "CIE-XYZ-D65", 1, SearchReferenceSpaceType::Display);
            assert_eq!(cs1.num_aliases(), 1);
            assert_eq!(cs1.alias(0), "cie_xyz_d65");
            {
                assert!(cs1.transform(ColorSpaceDirection::ToReference).is_none());
                check_group(
                    cs1.transform(ColorSpaceDirection::FromReference),
                    &[TransformType::Matrix, TransformType::Matrix],
                );
            }
        }

        // Scene-referred spaces.
        {
            // This is recognized as a duplicate, even though the name is different in the two configs.
            let cs = check_color_space(&merged, "ACES2065-1", 0, SearchReferenceSpaceType::Scene);
            assert_eq!(cs.num_aliases(), 2);
            assert_eq!(cs.alias(0), "aces");
            // Check for alias ap0 (added from base config).
            assert_eq!(cs.alias(1), "ap0");
            // Check categories were copied.
            assert_eq!(cs.num_categories(), 2);
            assert!(cs.has_category("file-io"));
            assert!(cs.has_category("texture"));
            assert_eq!(cs.encoding(), "scene-linear");
            {
                assert!(cs.transform(ColorSpaceDirection::FromReference).is_none());
                check_group(
                    cs.transform(ColorSpaceDirection::ToReference),
                    &[TransformType::Matrix, TransformType::Matrix],
                );
            }

            // This is recognized as a duplicate, even though the name is different in the two configs.
            let cs1 = check_color_space(&merged, "sRGB", 1, SearchReferenceSpaceType::Scene);
            assert_eq!(cs1.num_aliases(), 2);
            // Check for alias sRGB - Texture (added from base config colorspace name).
            assert_eq!(cs1.alias(0), "sRGB - Texture");
            // Check for alias srgb_tx (added from base config).
            assert_eq!(cs1.alias(1), "srgb_tx");
            // Check categories were copied.
            assert_eq!(cs1.num_categories(), 2);
            assert!(cs1.has_category("file-io"));
            assert!(cs1.has_category("texture"));
            {
                assert!(cs1.transform(ColorSpaceDirection::FromReference).is_none());
                check_group(
                    cs1.transform(ColorSpaceDirection::ToReference),
                    &[TransformType::ExponentWithLinear, TransformType::Matrix],
                );
            }

            let cs2 = check_color_space(&merged, "rec709", 2, SearchReferenceSpaceType::Scene);
            assert_eq!(cs2.num_categories(), 1);
            assert!(cs2.has_category("texture"));
            {
                assert!(cs2.transform(ColorSpaceDirection::FromReference).is_none());
                check_group(
                    cs2.transform(ColorSpaceDirection::ToReference),
                    &[TransformType::Matrix],
                );
            }

            let cs3 = check_color_space(&merged, "Raw", 3, SearchReferenceSpaceType::Scene);
            assert_eq!(cs3.num_aliases(), 1);
            assert_eq!(cs3.alias(0), "Utility - Raw");
            assert!(cs3.is_data());
            {
                assert!(cs3.transform(ColorSpaceDirection::ToReference).is_none());
                assert!(cs3.transform(ColorSpaceDirection::FromReference).is_none());
            }

            let cs4 = check_color_space(&merged, "ACEScg", 4, SearchReferenceSpaceType::Scene);
            assert_eq!(cs4.num_aliases(), 0);
            assert_eq!(cs4.num_categories(), 0);
            {
                assert!(cs4.transform(ColorSpaceDirection::FromReference).is_none());
                assert_eq!(
                    cs4.transform(ColorSpaceDirection::ToReference)
                        .unwrap()
                        .transform_type(),
                    TransformType::Builtin
                );
            }
        }

        // View transforms.
        {
            assert_eq!(merged.num_view_transforms(), 2);
            assert_eq!(merged.view_transform_name_by_index(0), "SDR Video");
            assert_eq!(
                merged.view_transform("SDR Video").unwrap().description(),
                "from input"
            );
            let tf = merged
                .view_transform("SDR Video")
                .unwrap()
                .transform(ViewTransformDirection::FromReference)
                .unwrap();

            // Validate the reference space conversion was added to the transform from the input config.
            check_group(
                Some(tf),
                &[
                    TransformType::Matrix,
                    TransformType::Builtin,
                    TransformType::Matrix,
                ],
            );

            assert_eq!(merged.view_transform_name_by_index(1), "vt2");
            let tf = merged
                .view_transform("vt2")
                .unwrap()
                .transform(ViewTransformDirection::ToReference)
                .unwrap();

            // Validate the reference space conversion was not added to the transform from the base config.
            assert_eq!(tf.transform_type(), TransformType::ExponentWithLinear);
        }
    }

    // PreferBase, Input first.
    {
        let mut params = setup(MergeStrategy::PreferBase);
        params.set_input_first(true);
        let merged = check_merge(LogType::Warning, &base, &input, &params, |o: &mut MergeHandlerOptions| -> Result<()> { roles(o)?; display_views(o)?; view_transforms(o)?; colorspaces(o)?; Ok(()) }, &["Equivalent base color space 'sRGB - Display' overrides 'sRGB - Display' in the input config, preserving aliases.",
            "Equivalent base color space 'CIE-XYZ-D65' overrides 'CIE-XYZ-D65' in the input config, preserving aliases.",
            "Equivalent base color space 'ap0' overrides 'ACES2065-1' in the input config, preserving aliases.",
            "Equivalent base color space 'sRGB - Texture' overrides 'sRGB' in the input config, preserving aliases.",
            "Input color space 'ACES2065-1' is a duplicate of base color space 'ap0' but was unable to add alias 'aces' since it conflicts with base color space 'ACEScg'."]);

        assert_eq!(merged.num_roles(), 1);
        assert_eq!(
            merged.role_color_space("cie_xyz_d65_interchange"),
            "CIE-XYZ-D65"
        );

        assert_eq!(
            merged.num_color_spaces_filtered(
                SearchReferenceSpaceType::All,
                ColorSpaceVisibility::All
            ),
            7
        );

        // Display-referred spaces.
        {
            let cs = check_color_space(
                &merged,
                "sRGB - Display",
                0,
                SearchReferenceSpaceType::Display,
            );
            assert_eq!(cs.num_aliases(), 1);
            assert_eq!(cs.alias(0), "srgb_display");
            assert_eq!(cs.description(), "from base");
            assert_eq!(cs.encoding(), "");
            // Check categories were copied.
            assert_eq!(cs.num_categories(), 3);
            assert!(cs.has_category("file-io"));
            assert!(cs.has_category("texture"));
            assert!(cs.has_category("display"));
            {
                assert!(cs.transform(ColorSpaceDirection::ToReference).is_none());
                assert_eq!(
                    cs.transform(ColorSpaceDirection::FromReference)
                        .unwrap()
                        .transform_type(),
                    TransformType::Builtin
                );
            }

            let cs1 =
                check_color_space(&merged, "CIE-XYZ-D65", 1, SearchReferenceSpaceType::Display);
            assert_eq!(cs1.num_aliases(), 1);
            assert_eq!(cs1.alias(0), "cie_xyz_d65");
            {
                assert!(cs1.transform(ColorSpaceDirection::ToReference).is_none());
                assert!(cs1.transform(ColorSpaceDirection::FromReference).is_none());
            }
        }

        // Scene-referred spaces.
        {
            let cs = check_color_space(&merged, "rec709", 0, SearchReferenceSpaceType::Scene);
            assert_eq!(cs.num_categories(), 1);
            assert!(cs.has_category("texture"));
            {
                assert!(cs.transform(ColorSpaceDirection::FromReference).is_none());
                check_group(
                    cs.transform(ColorSpaceDirection::ToReference),
                    &[TransformType::Matrix],
                );
            }

            let cs1 = check_color_space(&merged, "Raw", 1, SearchReferenceSpaceType::Scene);
            assert_eq!(cs1.num_aliases(), 1);
            assert_eq!(cs1.alias(0), "Utility - Raw");
            assert!(cs1.is_data());
            {
                assert!(cs1.transform(ColorSpaceDirection::ToReference).is_none());
                assert!(cs1.transform(ColorSpaceDirection::FromReference).is_none());
            }

            let cs2 = check_color_space(&merged, "ACEScg", 2, SearchReferenceSpaceType::Scene);
            assert_eq!(cs2.num_aliases(), 1);
            assert_eq!(cs2.alias(0), "aces");
            {
                assert!(cs2.transform(ColorSpaceDirection::FromReference).is_none());
                assert_eq!(
                    cs2.transform(ColorSpaceDirection::ToReference)
                        .unwrap()
                        .transform_type(),
                    TransformType::Builtin
                );
            }

            let cs3 = check_color_space(&merged, "ap0", 3, SearchReferenceSpaceType::Scene);
            assert_eq!(cs3.num_aliases(), 1);
            assert_eq!(cs3.alias(0), "ACES2065-1");
            assert!(!cs3.is_data());
            // Check categories were copied.
            assert_eq!(cs3.num_categories(), 2);
            assert!(cs3.has_category("file-io"));
            assert!(cs3.has_category("texture"));
            assert_eq!(cs3.encoding(), "");
            {
                assert!(cs3.transform(ColorSpaceDirection::ToReference).is_none());
                assert!(cs3.transform(ColorSpaceDirection::FromReference).is_none());
            }

            let cs4 = check_color_space(
                &merged,
                "sRGB - Texture",
                4,
                SearchReferenceSpaceType::Scene,
            );
            assert_eq!(cs4.num_aliases(), 2);
            assert_eq!(cs4.alias(0), "srgb");
            assert_eq!(cs4.alias(1), "srgb_tx");
            assert_eq!(cs4.num_categories(), 2);
            {
                assert!(cs4.transform(ColorSpaceDirection::ToReference).is_none());
                check_group(
                    cs4.transform(ColorSpaceDirection::FromReference),
                    &[TransformType::Matrix, TransformType::ExponentWithLinear],
                );
            }
        }

        // View transforms.
        {
            assert_eq!(merged.num_view_transforms(), 2);
            assert_eq!(merged.view_transform_name_by_index(0), "SDR Video");
            assert_eq!(
                merged.view_transform("SDR Video").unwrap().description(),
                "from base"
            );
            let tf = merged
                .view_transform("SDR Video")
                .unwrap()
                .transform(ViewTransformDirection::FromReference)
                .unwrap();

            // Validate that no reference space conversion was added, since the base transform was used.
            assert_eq!(tf.transform_type(), TransformType::Builtin);

            assert_eq!(merged.view_transform_name_by_index(1), "vt2");
            let tf = merged
                .view_transform("vt2")
                .unwrap()
                .transform(ViewTransformDirection::ToReference)
                .unwrap();

            // Validate the reference space conversion was not added to the transform from the base config.
            assert_eq!(tf.transform_type(), TransformType::ExponentWithLinear);
        }
    }

    // Nothing special to test for Input only and Base only.
}

#[test]
fn merge_configs_colorspaces_section_errors() {
    let setup = |strategy: MergeStrategy| -> ConfigMergingParameters {
        let mut p = ConfigMergingParameters::new();
        // Note that these tests run several of the mergers.
        p.set_roles(strategy);
        p.set_colorspaces(strategy);
        p.set_named_transforms(strategy);
        p.set_default_strategy(strategy);
        p.set_input_family_prefix("Input/");
        p.set_base_family_prefix("Base/");
        p.set_adjust_input_reference_space(false);
        p.set_avoid_duplicates(false);
        p
    };

    // Test ADD_CS_ERROR_NAME_IDENTICAL_TO_A_ROLE_NAME
    {
        {
            const BASE: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: A}

roles:
    b: colorspace_a

colorspaces:
- !<ColorSpace>
    name: colorspace_a
"#;

            const INPUT: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: B}

colorspaces:
- !<ColorSpace>
    name: B
"#;
            let base = from_str(BASE);
            let input = from_str(INPUT);

            // The role takes priority over the inbound colorspace.
            // The conflicting color space should not be added to the merged config (skipped).
            {
                let mut params = setup(MergeStrategy::PreferInput);
                params.set_input_first(false);

                let mut merged = base.create_editable_copy();
                run_on(&base, &input, &params, &mut merged, roles).unwrap();
                check_on(
                    LogType::Warning,
                    &base,
                    &input,
                    &params,
                    &mut merged,
                    colorspaces,
                    &["Color space 'B' was not merged as it's identical to a role name"],
                );

                assert_eq!(merged.num_roles(), 1);
                assert_eq!(merged.role_name(0), "b");

                // Colorspace A should not be added to the merged config (skipped)
                assert_eq!(
                    merged.num_color_spaces_filtered(
                        SearchReferenceSpaceType::All,
                        ColorSpaceVisibility::All
                    ),
                    1
                );
                assert_eq!(merged.color_space_name_by_index(0), "colorspace_a");
            }
            {
                let mut params = setup(MergeStrategy::PreferBase);
                params.set_input_first(true);

                let mut merged = base.create_editable_copy();
                run_on(&base, &input, &params, &mut merged, roles).unwrap();
                check_on(
                    LogType::Warning,
                    &base,
                    &input,
                    &params,
                    &mut merged,
                    colorspaces,
                    &["Color space 'B' was not merged as it's identical to a role name"],
                );

                assert_eq!(merged.num_roles(), 1);
                assert_eq!(merged.role_name(0), "b");

                // Colorspace A should not be added to the merged config (skipped)
                assert_eq!(
                    merged.num_color_spaces_filtered(
                        SearchReferenceSpaceType::All,
                        ColorSpaceVisibility::All
                    ),
                    1
                );
                assert_eq!(merged.color_space_name_by_index(0), "colorspace_a");
            }
            // Testing the error message when Error on conflict is enabled.
            {
                let mut params = setup(MergeStrategy::PreferInput);
                params.set_input_first(false);
                params.set_error_on_conflict(true);

                let mut merged = base.create_editable_copy();
                run_on(&base, &input, &params, &mut merged, roles).unwrap();
                assert_err_msg(
                    run_on(&base, &input, &params, &mut merged, colorspaces),
                    "Color space 'B' was not merged as it's identical to a role name",
                );
            }
        }

        {
            const BASE: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: A}

colorspaces:
- !<ColorSpace>
    name: A
"#;

            const INPUT: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: B}

roles:
    A: colorspace_b

colorspaces:
- !<ColorSpace>
    name: colorspace_b
"#;
            let base = from_str(BASE);
            let input = from_str(INPUT);

            {
                let params = setup(MergeStrategy::PreferInput);

                let mut merged = base.create_editable_copy();
                check_on(LogType::Warning, &base, &input, &params, &mut merged, roles, &["The Input config contains a role 'a' that would override Base config color space 'A'"]);
                run_on(&base, &input, &params, &mut merged, colorspaces).unwrap();

                assert_eq!(merged.num_roles(), 0);

                assert_eq!(
                    merged.num_color_spaces_filtered(
                        SearchReferenceSpaceType::All,
                        ColorSpaceVisibility::All
                    ),
                    2
                );
                assert_eq!(merged.color_space_name_by_index(0), "colorspace_b");
                assert_eq!(merged.color_space_name_by_index(1), "A");
            }
            {
                let params = setup(MergeStrategy::PreferBase);

                let mut merged = base.create_editable_copy();
                check_on(LogType::Warning, &base, &input, &params, &mut merged, roles, &["The Input config contains a role 'a' that would override Base config color space 'A'"]);
                run_on(&base, &input, &params, &mut merged, colorspaces).unwrap();

                assert_eq!(merged.num_roles(), 0);

                assert_eq!(
                    merged.num_color_spaces_filtered(
                        SearchReferenceSpaceType::All,
                        ColorSpaceVisibility::All
                    ),
                    2
                );
                assert_eq!(merged.color_space_name_by_index(0), "colorspace_b");
                assert_eq!(merged.color_space_name_by_index(1), "A");
            }
        }
    }

    {
        {
            const BASE: &str = r#"ocio_profile_version: 2

file_rules:
- !<Rule> {name: Default, colorspace: cs_base}

named_transforms:
- !<NamedTransform>
    name: nt_base
    encoding: log
    inverse_transform: !<MatrixTransform> {name: inverse, offset: [-0.2, -0.1, -0.1, 0]}
- !<NamedTransform>
    name: nt_base_extra
    aliases: [nt_base2]
    encoding: log
    inverse_transform: !<MatrixTransform> {name: inverse, offset: [-0.2, -0.1, -0.1, 0]}

colorspaces:
- !<ColorSpace>
    name: cs_base
"#;

            const INPUT: &str = r#"ocio_profile_version: 2

file_rules:
- !<Rule> {name: Default, colorspace: nt_base}

colorspaces:
- !<ColorSpace>
    name: nt_base
- !<ColorSpace>
    name: nt_base2
"#;
            let base = from_str(BASE);
            let input = from_str(INPUT);
            {
                let mut merged = base.create_editable_copy();
                let mut params = setup(MergeStrategy::PreferInput);
                params.set_input_first(false);
                run_on(&base, &input, &params, &mut merged, colorspaces).unwrap();
                check_on(LogType::Warning, &base, &input, &params, &mut merged, named_transforms, &["Named transform 'nt_base' was not merged as there's a color space with that name",
                    "Merged Base named transform 'nt_base_extra' has an alias 'nt_base2' that conflicts with color space 'nt_base2'"]);

                assert_eq!(
                    merged.num_named_transforms_filtered(NamedTransformVisibility::All),
                    1
                );
                let nt = check_named_transform(&merged, "nt_base_extra", 0);
                assert_eq!(nt.num_aliases(), 0);

                assert_eq!(
                    merged.num_color_spaces_filtered(
                        SearchReferenceSpaceType::All,
                        ColorSpaceVisibility::All
                    ),
                    3
                );
                assert_eq!(merged.color_space_name_by_index(0), "cs_base");
                assert_eq!(merged.color_space_name_by_index(1), "nt_base");
                assert_eq!(merged.color_space_name_by_index(2), "nt_base2");
            }
            {
                let mut merged = base.create_editable_copy();
                let mut params = setup(MergeStrategy::PreferBase);
                params.set_input_first(false);
                run_on(&base, &input, &params, &mut merged, colorspaces).unwrap();
                check_on(LogType::Warning, &base, &input, &params, &mut merged, named_transforms, &["Named transform 'nt_base' was not merged as there's a color space with that name",
                    "Merged Base named transform 'nt_base_extra' has an alias 'nt_base2' that conflicts with color space 'nt_base2'"]);

                assert_eq!(
                    merged.num_named_transforms_filtered(NamedTransformVisibility::All),
                    1
                );
                let nt = check_named_transform(&merged, "nt_base_extra", 0);
                assert_eq!(nt.num_aliases(), 0);

                assert_eq!(
                    merged.num_color_spaces_filtered(
                        SearchReferenceSpaceType::All,
                        ColorSpaceVisibility::All
                    ),
                    3
                );
                assert_eq!(merged.color_space_name_by_index(0), "cs_base");
                assert_eq!(merged.color_space_name_by_index(1), "nt_base");
                assert_eq!(merged.color_space_name_by_index(2), "nt_base2");
            }
            // Testing the error message when Error on conflict is enabled.
            {
                let mut merged = base.create_editable_copy();

                let mut params = setup(MergeStrategy::PreferInput);
                params.set_input_first(false);
                params.set_error_on_conflict(true);
                run_on(&base, &input, &params, &mut merged, colorspaces).unwrap();
                assert_err_msg(run_on(&base, &input, &params, &mut merged, named_transforms), "Named transform 'nt_base' was not merged as there's a color space with that name");
            }
        }

        {
            const BASE: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: nt_input}

colorspaces:
- !<ColorSpace>
    name: nt_input
- !<ColorSpace>
    name: nt_input2
"#;

            const INPUT: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: cs_input}

named_transforms:
  - !<NamedTransform>
    name: nt_input
    categories: [ working-space, basic-3d, advanced-2d ]
    encoding: sdr-video
    transform: !<MatrixTransform> {name: forwardBase, offset: [0.1, 0.2, 0.3, 0.4]}

  - !<NamedTransform>
    name: nt_input_extra
    aliases: [nt_input2]
    categories: [ working-space, basic-3d, advanced-2d ]
    encoding: sdr-video
    transform: !<MatrixTransform> {name: forwardBase, offset: [0.1, 0.2, 0.3, 0.4]}

colorspaces:
- !<ColorSpace>
    name: cs_input
"#;
            let base = from_str(BASE);
            let input = from_str(INPUT);
            {
                let mut merged = base.create_editable_copy();
                let mut params = setup(MergeStrategy::PreferInput);
                params.set_input_first(true);
                run_on(&base, &input, &params, &mut merged, colorspaces).unwrap();
                check_on(LogType::Warning, &base, &input, &params, &mut merged, named_transforms, &["Named transform 'nt_input' was not merged as there's a color space with that name",
                    "Merged Input named transform 'nt_input_extra' has an alias 'nt_input2' that conflicts with color space 'nt_input2'"]);

                assert_eq!(
                    merged.num_named_transforms_filtered(NamedTransformVisibility::All),
                    1
                );
                let nt = check_named_transform(&merged, "nt_input_extra", 0);
                assert_eq!(nt.num_aliases(), 0);

                assert_eq!(
                    merged.num_color_spaces_filtered(
                        SearchReferenceSpaceType::All,
                        ColorSpaceVisibility::All
                    ),
                    3
                );
                assert_eq!(merged.color_space_name_by_index(0), "cs_input");
                assert_eq!(merged.color_space_name_by_index(1), "nt_input");
                assert_eq!(merged.color_space_name_by_index(2), "nt_input2");
            }

            {
                let mut merged = base.create_editable_copy();
                let mut params = setup(MergeStrategy::PreferBase);
                params.set_input_first(false);
                run_on(&base, &input, &params, &mut merged, colorspaces).unwrap();
                check_on(LogType::Warning, &base, &input, &params, &mut merged, named_transforms, &["Named transform 'nt_input' was not merged as there's a color space with that name",
                    "Merged Input named transform 'nt_input_extra' has an alias 'nt_input2' that conflicts with color space 'nt_input2'"]);

                assert_eq!(
                    merged.num_named_transforms_filtered(NamedTransformVisibility::All),
                    1
                );
                let nt = check_named_transform(&merged, "nt_input_extra", 0);
                assert_eq!(nt.num_aliases(), 0);

                assert_eq!(
                    merged.num_color_spaces_filtered(
                        SearchReferenceSpaceType::All,
                        ColorSpaceVisibility::All
                    ),
                    3
                );
                assert_eq!(merged.color_space_name_by_index(0), "nt_input");
                assert_eq!(merged.color_space_name_by_index(1), "nt_input2");
                assert_eq!(merged.color_space_name_by_index(2), "cs_input");
            }
        }
    }

    {
        {
            const BASE: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: csBase}

roles:
    role_base: csBase

colorspaces:
- !<ColorSpace>
    name: csBase

"#;

            const INPUT: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: csInput}

colorspaces:
- !<ColorSpace>
    name: csInput
    aliases: [role_base]
"#;
            let base = from_str(BASE);
            let input = from_str(INPUT);
            {
                let mut params = setup(MergeStrategy::PreferInput);
                params.set_input_first(false);

                let mut merged = base.create_editable_copy();
                run_on(&base, &input, &params, &mut merged, roles).unwrap();
                check_on(LogType::Warning, &base, &input, &params, &mut merged, colorspaces, &["Merged color space 'csInput' has an alias 'role_base' that conflicts with a role"]);

                assert_eq!(merged.num_roles(), 1);
                assert_eq!(merged.role_name(0), "role_base");

                assert_eq!(
                    merged.num_color_spaces_filtered(
                        SearchReferenceSpaceType::All,
                        ColorSpaceVisibility::All
                    ),
                    2
                );
                assert_eq!(merged.color_space_name_by_index(0), "csBase");
                let name = merged.color_space_name_by_index(1);
                assert_eq!(name, "csInput");
                assert_eq!(merged.get_color_space(name).unwrap().num_aliases(), 0);
            }
            {
                let mut params = setup(MergeStrategy::PreferBase);
                params.set_input_first(false);

                let mut merged = base.create_editable_copy();
                run_on(&base, &input, &params, &mut merged, roles).unwrap();
                check_on(LogType::Warning, &base, &input, &params, &mut merged, colorspaces, &["Merged color space 'csInput' has an alias 'role_base' that conflicts with a role"]);

                assert_eq!(merged.num_roles(), 1);
                assert_eq!(merged.role_name(0), "role_base");

                assert_eq!(
                    merged.num_color_spaces_filtered(
                        SearchReferenceSpaceType::All,
                        ColorSpaceVisibility::All
                    ),
                    2
                );
                assert_eq!(merged.color_space_name_by_index(0), "csBase");
                let name = merged.color_space_name_by_index(1);
                assert_eq!(name, "csInput");
                assert_eq!(merged.get_color_space(name).unwrap().num_aliases(), 0);
            }
            // Testing the error message when Error on conflict is enabled.
            {
                let mut params = setup(MergeStrategy::PreferInput);
                params.set_error_on_conflict(true);

                let mut merged = base.create_editable_copy();
                run_on(&base, &input, &params, &mut merged, roles).unwrap();
                assert_err_msg(run_on(&base, &input, &params, &mut merged, colorspaces), "Merged color space 'csInput' has an alias 'role_base' that conflicts with a role");
            }
        }

        {
            const BASE: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: csBase}

colorspaces:
- !<ColorSpace>
    name: csBase
    aliases: [role_input]
"#;

            const INPUT: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: csInput}

roles:
    role_input: csInput

colorspaces:
- !<ColorSpace>
    name: csInput
"#;
            let base = from_str(BASE);
            let input = from_str(INPUT);
            {
                let params = setup(MergeStrategy::PreferInput);

                let mut merged = base.create_editable_copy();
                check_on(LogType::Warning, &base, &input, &params, &mut merged, roles, &["The Input config contains a role 'role_input' that would override an alias of Base config color space 'csBase'"]);
                run_on(&base, &input, &params, &mut merged, colorspaces).unwrap();

                assert_eq!(merged.num_roles(), 0);

                assert_eq!(
                    merged.num_color_spaces_filtered(
                        SearchReferenceSpaceType::All,
                        ColorSpaceVisibility::All
                    ),
                    2
                );
                assert_eq!(merged.color_space_name_by_index(0), "csInput");
                let name = merged.color_space_name_by_index(1);
                assert_eq!(name, "csBase");
                assert_eq!(merged.get_color_space(name).unwrap().num_aliases(), 1);
                assert_eq!(merged.get_color_space(name).unwrap().alias(0), "role_input");
            }
            {
                let params = setup(MergeStrategy::PreferBase);

                let mut merged = base.create_editable_copy();
                check_on(LogType::Warning, &base, &input, &params, &mut merged, roles, &["The Input config contains a role 'role_input' that would override an alias of Base config color space 'csBase'"]);
                run_on(&base, &input, &params, &mut merged, colorspaces).unwrap();

                assert_eq!(merged.num_roles(), 0);

                assert_eq!(
                    merged.num_color_spaces_filtered(
                        SearchReferenceSpaceType::All,
                        ColorSpaceVisibility::All
                    ),
                    2
                );
                assert_eq!(merged.color_space_name_by_index(0), "csInput");
                let name = merged.color_space_name_by_index(1);
                assert_eq!(name, "csBase");
                assert_eq!(merged.get_color_space(name).unwrap().num_aliases(), 1);
                assert_eq!(merged.get_color_space(name).unwrap().alias(0), "role_input");
            }
        }
    }

    {
        {
            const BASE: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: A}

named_transforms:
  - !<NamedTransform>
    name: nt_base
    encoding: log
    inverse_transform: !<MatrixTransform> {name: inverse, offset: [-0.2, -0.1, -0.1, 0]}

  - !<NamedTransform>
    name: nt_base_extra
    aliases: [nt_base2]
    encoding: log
    inverse_transform: !<MatrixTransform> {name: inverse, offset: [-0.2, -0.1, -0.1, 0]}
"#;

            const INPUT: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: cs_input}

colorspaces:
- !<ColorSpace>
    name: cs_input
    aliases: [nt_base]
- !<ColorSpace>
    name: cs_input2
    aliases: [nt_base2]
"#;
            let base = from_str(BASE);
            let input = from_str(INPUT);
            {
                let mut params = setup(MergeStrategy::PreferInput);
                params.set_input_first(false);

                let mut merged = base.create_editable_copy();
                run_on(&base, &input, &params, &mut merged, colorspaces).unwrap();
                check_on(LogType::Warning, &base, &input, &params, &mut merged, named_transforms, &["Named transform 'nt_base' was not merged as there's a color space alias with that name",
                    "Merged Base named transform 'nt_base_extra' has a conflict with alias 'nt_base2' in color space 'cs_input2'"]);

                assert_eq!(
                    merged.num_named_transforms_filtered(NamedTransformVisibility::All),
                    1
                );
                let nt = check_named_transform(&merged, "nt_base_extra", 0);
                assert_eq!(nt.num_aliases(), 0);

                assert_eq!(
                    merged.num_color_spaces_filtered(
                        SearchReferenceSpaceType::All,
                        ColorSpaceVisibility::All
                    ),
                    2
                );
                let cs = check_color_space(&merged, "cs_input", 0, SearchReferenceSpaceType::Scene);
                assert_eq!(cs.num_aliases(), 1);
                assert_eq!(cs.alias(0), "nt_base");

                let cs =
                    check_color_space(&merged, "cs_input2", 1, SearchReferenceSpaceType::Scene);
                assert_eq!(cs.num_aliases(), 1);
                assert_eq!(cs.alias(0), "nt_base2");
            }
            {
                let mut params = setup(MergeStrategy::PreferBase);
                params.set_input_first(false);

                let mut merged = base.create_editable_copy();
                run_on(&base, &input, &params, &mut merged, colorspaces).unwrap();
                check_on(LogType::Warning, &base, &input, &params, &mut merged, named_transforms, &["Named transform 'nt_base' was not merged as there's a color space alias with that name",
                    "Merged Base named transform 'nt_base_extra' has a conflict with alias 'nt_base2' in color space 'cs_input2'"]);

                assert_eq!(
                    merged.num_named_transforms_filtered(NamedTransformVisibility::All),
                    1
                );
                let nt = check_named_transform(&merged, "nt_base_extra", 0);
                assert_eq!(nt.num_aliases(), 0);

                assert_eq!(
                    merged.num_color_spaces_filtered(
                        SearchReferenceSpaceType::All,
                        ColorSpaceVisibility::All
                    ),
                    2
                );
                let cs = check_color_space(&merged, "cs_input", 0, SearchReferenceSpaceType::Scene);
                assert_eq!(cs.num_aliases(), 1);
                assert_eq!(cs.alias(0), "nt_base");

                let cs =
                    check_color_space(&merged, "cs_input2", 1, SearchReferenceSpaceType::Scene);
                assert_eq!(cs.num_aliases(), 1);
                assert_eq!(cs.alias(0), "nt_base2");
            }
            // Testing the error message when Error on conflict is enabled.
            {
                let mut params = setup(MergeStrategy::PreferInput);
                params.set_error_on_conflict(true);

                let mut merged = base.create_editable_copy();
                run_on(&base, &input, &params, &mut merged, colorspaces).unwrap();
                assert_err_msg(run_on(&base, &input, &params, &mut merged, named_transforms), "Named transform 'nt_base' was not merged as there's a color space alias with that name");
            }
        }

        {
            const BASE: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: cs_base}

colorspaces:
- !<ColorSpace>
    name: cs_base
    aliases: [nt_input]
- !<ColorSpace>
    name: cs_base2
    aliases: [nt_input2]
"#;

            const INPUT: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: B}

named_transforms:
  - !<NamedTransform>
    name: nt_input
    encoding: log
    inverse_transform: !<MatrixTransform> {name: inverse, offset: [-0.2, -0.1, -0.1, 0]}
  - !<NamedTransform>
    name: nt_input_extra
    aliases: [nt_input2]
    encoding: log
    inverse_transform: !<MatrixTransform> {name: inverse, offset: [-0.2, -0.1, -0.1, 0]}
"#;
            let base = from_str(BASE);
            let input = from_str(INPUT);
            {
                let mut params = setup(MergeStrategy::PreferInput);
                params.set_input_first(false);

                let mut merged = base.create_editable_copy();
                run_on(&base, &input, &params, &mut merged, colorspaces).unwrap();
                check_on(LogType::Warning, &base, &input, &params, &mut merged, named_transforms, &["Named transform 'nt_input' was not merged as there's a color space alias with that name",
                    "Merged Input named transform 'nt_input_extra' has a conflict with alias 'nt_input2' in color space 'cs_base2'"]);

                assert_eq!(
                    merged.num_named_transforms_filtered(NamedTransformVisibility::All),
                    1
                );
                let nt = check_named_transform(&merged, "nt_input_extra", 0);
                assert_eq!(nt.num_aliases(), 0);

                assert_eq!(
                    merged.num_color_spaces_filtered(
                        SearchReferenceSpaceType::All,
                        ColorSpaceVisibility::All
                    ),
                    2
                );
                let cs = check_color_space(&merged, "cs_base", 0, SearchReferenceSpaceType::Scene);
                assert_eq!(cs.num_aliases(), 1);
                assert_eq!(cs.alias(0), "nt_input");

                let cs = check_color_space(&merged, "cs_base2", 1, SearchReferenceSpaceType::Scene);
                assert_eq!(cs.num_aliases(), 1);
                assert_eq!(cs.alias(0), "nt_input2");
            }
            {
                let mut params = setup(MergeStrategy::PreferBase);
                params.set_input_first(false);

                let mut merged = base.create_editable_copy();
                run_on(&base, &input, &params, &mut merged, colorspaces).unwrap();
                check_on(LogType::Warning, &base, &input, &params, &mut merged, named_transforms, &["Named transform 'nt_input' was not merged as there's a color space alias with that name",
                    "Merged Input named transform 'nt_input_extra' has a conflict with alias 'nt_input2' in color space 'cs_base2'"]);

                assert_eq!(
                    merged.num_named_transforms_filtered(NamedTransformVisibility::All),
                    1
                );
                let nt = check_named_transform(&merged, "nt_input_extra", 0);
                assert_eq!(nt.num_aliases(), 0);

                assert_eq!(
                    merged.num_color_spaces_filtered(
                        SearchReferenceSpaceType::All,
                        ColorSpaceVisibility::All
                    ),
                    2
                );
                let cs = check_color_space(&merged, "cs_base", 0, SearchReferenceSpaceType::Scene);
                assert_eq!(cs.num_aliases(), 1);
                assert_eq!(cs.alias(0), "nt_input");

                let cs = check_color_space(&merged, "cs_base2", 1, SearchReferenceSpaceType::Scene);
                assert_eq!(cs.num_aliases(), 1);
                assert_eq!(cs.alias(0), "nt_input2");
            }
        }
    }

    {
        {
            const BASE: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: cs_base}

colorspaces:
- !<ColorSpace>
    name: cs_base
    aliases: [my_colorspace]
"#;

            const INPUT: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: cs_input}

colorspaces:
- !<ColorSpace>
    name: cs_input
    aliases: [my_colorspace]
"#;
            let base = from_str(BASE);
            let input = from_str(INPUT);

            {
                let mut params = setup(MergeStrategy::PreferInput);
                params.set_input_first(false);

                let mut merged = base.create_editable_copy();
                check_on(LogType::Warning, &base, &input, &params, &mut merged, colorspaces, &["Merged color space 'cs_input' has a conflict with alias 'my_colorspace' in color space 'cs_base'"]);

                assert_eq!(
                    merged.num_color_spaces_filtered(
                        SearchReferenceSpaceType::All,
                        ColorSpaceVisibility::All
                    ),
                    2
                );
                let cs = check_color_space(&merged, "cs_base", 0, SearchReferenceSpaceType::Scene);
                assert_eq!(cs.num_aliases(), 0);

                let cs = check_color_space(&merged, "cs_input", 1, SearchReferenceSpaceType::Scene);
                assert_eq!(cs.num_aliases(), 1);
                assert_eq!(cs.alias(0), "my_colorspace");
            }
            {
                let mut params = setup(MergeStrategy::PreferBase);
                params.set_input_first(false);

                let mut merged = base.create_editable_copy();
                check_on(LogType::Warning, &base, &input, &params, &mut merged, colorspaces, &["Merged color space 'cs_input' has a conflict with alias 'my_colorspace' in color space 'cs_base'"]);

                assert_eq!(
                    merged.num_color_spaces_filtered(
                        SearchReferenceSpaceType::All,
                        ColorSpaceVisibility::All
                    ),
                    2
                );
                let cs = check_color_space(&merged, "cs_base", 0, SearchReferenceSpaceType::Scene);
                assert_eq!(cs.num_aliases(), 1);
                assert_eq!(cs.alias(0), "my_colorspace");

                let cs = check_color_space(&merged, "cs_input", 1, SearchReferenceSpaceType::Scene);
                assert_eq!(cs.num_aliases(), 0);
            }
            {
                let params = setup(MergeStrategy::PreferInput);

                let mut merged = base.create_editable_copy();
                check_on(LogType::Warning, &base, &input, &params, &mut merged, colorspaces, &["Merged color space 'cs_input' has a conflict with alias 'my_colorspace' in color space 'cs_base'"]);

                assert_eq!(
                    merged.num_color_spaces_filtered(
                        SearchReferenceSpaceType::All,
                        ColorSpaceVisibility::All
                    ),
                    2
                );
                let cs = check_color_space(&merged, "cs_input", 0, SearchReferenceSpaceType::Scene);
                assert_eq!(cs.num_aliases(), 1);
                assert_eq!(cs.alias(0), "my_colorspace");

                let cs = check_color_space(&merged, "cs_base", 1, SearchReferenceSpaceType::Scene);
                assert_eq!(cs.num_aliases(), 0);
            }
            {
                let params = setup(MergeStrategy::PreferBase);

                let mut merged = base.create_editable_copy();
                check_on(LogType::Warning, &base, &input, &params, &mut merged, colorspaces, &["Merged color space 'cs_input' has a conflict with alias 'my_colorspace' in color space 'cs_base'"]);

                assert_eq!(
                    merged.num_color_spaces_filtered(
                        SearchReferenceSpaceType::All,
                        ColorSpaceVisibility::All
                    ),
                    2
                );
                let cs = check_color_space(&merged, "cs_input", 0, SearchReferenceSpaceType::Scene);
                assert_eq!(cs.num_aliases(), 0);

                let cs = check_color_space(&merged, "cs_base", 1, SearchReferenceSpaceType::Scene);
                assert_eq!(cs.num_aliases(), 1);
                assert_eq!(cs.alias(0), "my_colorspace");
            }
            // Testing the error message when Error on conflict is enabled.
            {
                let mut params = setup(MergeStrategy::PreferInput);
                params.set_error_on_conflict(true);

                let mut merged = base.create_editable_copy();
                assert_err_msg(run_on(&base, &input, &params, &mut merged, colorspaces), "Merged color space 'cs_input' has a conflict with alias 'my_colorspace' in color space 'cs_base'");
            }
        }
    }

    {
        {
            const BASE: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: A}

colorspaces:
- !<ColorSpace>
    name: A
    aliases: [B]
"#;

            const INPUT: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: B}

colorspaces:
- !<ColorSpace>
    name: B
"#;
            let base = from_str(BASE);
            let input = from_str(INPUT);
            {
                let mut params = setup(MergeStrategy::PreferInput);
                params.set_input_first(false);

                let mut merged = base.create_editable_copy();
                check_on(LogType::Warning, &base, &input, &params, &mut merged, colorspaces, &["The name of merged color space 'B' has a conflict with an alias in color space 'A'"]);

                assert_eq!(
                    merged.num_color_spaces_filtered(
                        SearchReferenceSpaceType::All,
                        ColorSpaceVisibility::All
                    ),
                    2
                );
                let cs = check_color_space(&merged, "A", 0, SearchReferenceSpaceType::Scene);
                assert_eq!(cs.num_aliases(), 0);

                let cs = check_color_space(&merged, "B", 1, SearchReferenceSpaceType::Scene);
                assert_eq!(cs.num_aliases(), 0);
            }
            {
                let mut params = setup(MergeStrategy::PreferBase);
                params.set_input_first(false);

                let mut merged = base.create_editable_copy();
                check_on(LogType::Warning, &base, &input, &params, &mut merged, colorspaces, &["Color space 'B' was not merged as it conflicts with an alias in color space 'A'"]);

                assert_eq!(
                    merged.num_color_spaces_filtered(
                        SearchReferenceSpaceType::All,
                        ColorSpaceVisibility::All
                    ),
                    1
                );
                let cs = check_color_space(&merged, "A", 0, SearchReferenceSpaceType::Scene);
                assert_eq!(cs.num_aliases(), 1);
                assert_eq!(cs.alias(0), "B");
            }
            // Testing the error message when Error on conflict is enabled.
            {
                let mut params = setup(MergeStrategy::PreferInput);
                params.set_error_on_conflict(true);

                let mut merged = base.create_editable_copy();
                assert_err_msg(run_on(&base, &input, &params, &mut merged, colorspaces), "The name of merged color space 'B' has a conflict with an alias in color space 'A'");
            }
        }

        {
            const BASE: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: A}

colorspaces:
- !<ColorSpace>
    name: A
"#;

            const INPUT: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: B}

colorspaces:
- !<ColorSpace>
    name: B
    aliases: [A]
"#;
            let base = from_str(BASE);
            let input = from_str(INPUT);
            {
                let params = setup(MergeStrategy::PreferInput);

                let mut merged = base.create_editable_copy();
                check_on(LogType::Warning, &base, &input, &params, &mut merged, colorspaces, &["Merged color space 'B' has an alias 'A' that conflicts with color space 'A'"]);

                assert_eq!(
                    merged.num_color_spaces_filtered(
                        SearchReferenceSpaceType::All,
                        ColorSpaceVisibility::All
                    ),
                    1
                );

                let cs = check_color_space(&merged, "B", 0, SearchReferenceSpaceType::Scene);
                assert_eq!(cs.num_aliases(), 1);
                assert_eq!(cs.alias(0), "A");
            }
            {
                let mut params = setup(MergeStrategy::PreferInput);
                params.set_input_first(false);

                let mut merged = base.create_editable_copy();
                check_on(LogType::Warning, &base, &input, &params, &mut merged, colorspaces, &["Merged color space 'B' has an alias 'A' that conflicts with color space 'A'"]);

                assert_eq!(
                    merged.num_color_spaces_filtered(
                        SearchReferenceSpaceType::All,
                        ColorSpaceVisibility::All
                    ),
                    1
                );

                let cs = check_color_space(&merged, "B", 0, SearchReferenceSpaceType::Scene);
                assert_eq!(cs.num_aliases(), 1);
                assert_eq!(cs.alias(0), "A");
            }
            {
                let params = setup(MergeStrategy::PreferBase);

                let mut merged = base.create_editable_copy();
                check_on(LogType::Warning, &base, &input, &params, &mut merged, colorspaces, &["Merged color space 'B' has an alias 'A' that conflicts with color space 'A'"]);

                assert_eq!(
                    merged.num_color_spaces_filtered(
                        SearchReferenceSpaceType::All,
                        ColorSpaceVisibility::All
                    ),
                    2
                );
                let cs = check_color_space(&merged, "B", 0, SearchReferenceSpaceType::Scene);
                assert_eq!(cs.num_aliases(), 0);

                let cs = check_color_space(&merged, "A", 1, SearchReferenceSpaceType::Scene);
                assert_eq!(cs.num_aliases(), 0);
            }
            {
                let mut params = setup(MergeStrategy::PreferBase);
                params.set_input_first(false);

                let mut merged = base.create_editable_copy();
                check_on(LogType::Warning, &base, &input, &params, &mut merged, colorspaces, &["Merged color space 'B' has an alias 'A' that conflicts with color space 'A'"]);

                assert_eq!(
                    merged.num_color_spaces_filtered(
                        SearchReferenceSpaceType::All,
                        ColorSpaceVisibility::All
                    ),
                    2
                );
                let cs = check_color_space(&merged, "A", 0, SearchReferenceSpaceType::Scene);
                assert_eq!(cs.num_aliases(), 0);

                let cs = check_color_space(&merged, "B", 1, SearchReferenceSpaceType::Scene);
                assert_eq!(cs.num_aliases(), 0);
            }
            // Testing the error message when Error on conflict is enabled.
            {
                let mut params = setup(MergeStrategy::PreferInput);
                params.set_error_on_conflict(true);

                let mut merged = base.create_editable_copy();
                assert_err_msg(
                    run_on(&base, &input, &params, &mut merged, colorspaces),
                    "Merged color space 'B' has an alias 'A' that conflicts with color space 'A'",
                );
            }
        }
    }
}
