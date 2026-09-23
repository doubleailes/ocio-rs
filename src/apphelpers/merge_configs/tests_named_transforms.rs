//! Port of `MergeConfigsHelpers_tests.cpp` (part 4: named transforms section).

use super::tests::*;
use super::*;
use crate::types::*;

#[test]
fn merge_configs_named_transform_section() {
    let base = get_base_config();
    let input = get_input_config();

    let setup = |strategy: MergeStrategy| -> ConfigMergingParameters {
        let mut p = ConfigMergingParameters::new();
        // Note that these tests run several of the mergers. Need to run the color space merger
        // too, since that affects how the named transform merger will work (in terms of
        // avoiding conflicts with color space names).
        p.set_roles(strategy);
        p.set_colorspaces(strategy);
        p.set_named_transforms(strategy);
        p.set_default_strategy(strategy);
        p.set_input_family_prefix("Input/");
        p.set_base_family_prefix("Base/");
        p.set_adjust_input_reference_space(false);
        p.set_avoid_duplicates(true);
        p.set_input_first(true);
        p
    };

    // Test that the default strategy is used as a fallback if the section strategy was not defined.
    {
        // Using STRATEGY_UNSPECIFIED as this simulate that the section
        // is missing from the OCIOM file.
        let mut params = setup(MergeStrategy::Unspecified);
        // Simulate settings from OCIOM file.
        params.set_default_strategy(MergeStrategy::InputOnly);

        let mut merged = base.create_editable_copy();
        run_on(&base, &input, &params, &mut merged, colorspaces).unwrap();
        run_on(&base, &input, &params, &mut merged, named_transforms).unwrap();

        assert_eq!(
            merged.num_named_transforms_filtered(NamedTransformVisibility::All),
            3
        );
        let nt = check_named_transform(&merged, "nt_both", 0);
        assert_eq!(nt.num_aliases(), 2);
        assert_eq!(nt.alias(0), "Utility - Raw");
        assert_eq!(nt.alias(1), "nametr");
        assert_eq!(nt.family(), "");
        assert_eq!(nt.description(), "from input");

        let nt = check_named_transform(&merged, "nt_input", 1);
        assert_eq!(nt.num_aliases(), 2);
        assert_eq!(nt.alias(0), "Raw");
        assert_eq!(nt.alias(1), "in nt");
        assert_eq!(nt.family(), "Raw");
        assert_eq!(nt.description(), "from input");

        let nt = check_named_transform(&merged, "view_2", 2);
        assert_eq!(nt.num_aliases(), 1);
        assert_eq!(nt.alias(0), "g22_ap1");
        assert_eq!(nt.family(), "Raw");
        assert_eq!(nt.description(), "from input");
    }

    // Test NamedTransform with strategy = PreferInput, options InputFirst = true.
    {
        let params = setup(MergeStrategy::PreferInput);

        let mut merged = base.create_editable_copy();
        check_on(LogType::Warning, &base, &input, &params, &mut merged, colorspaces, &["Equivalent input color space 'ACES2065-1' replaces 'ACES2065-1' in the base config, preserving aliases.",
            "Equivalent input color space 'ACEScct - SomeOtherName' replaces 'ACEScct' in the base config, preserving aliases.",
            "Equivalent input color space 'view_1' replaces 'view_1' in the base config, preserving aliases.",
            "Equivalent input color space 'view_1B' replaces 'view_1' in the base config, preserving aliases.",
            "Equivalent input color space 'view_3' replaces 'view_2' in the base config, preserving aliases.",
            "Equivalent input color space 'log_3' replaces 'log_1' in the base config, preserving aliases.",
            "Equivalent input color space 'lin_3' replaces 'ACES2065-1' in the base config, preserving aliases."]);
        check_on(LogType::Warning, &base, &input, &params, &mut merged, named_transforms, &["Named transform 'nt_both' will replace a named transform in the base config",
            "Merged Base named transform 'nt_both' has a conflict with alias 'srgb_tx' in color space 'sRGB - Texture'",
            "Merged Base named transform 'nt_base' has an alias 'view_3' that conflicts with color space 'view_3'",
            "Merged Input named transform 'nt_both' has a conflict with alias 'Utility - Raw' in color space 'Raw'",
            "The name of merged named transform 'nt_input' has a conflict with an alias in named transform 'nt_base'",
            "Merged Input named transform 'nt_input' has an alias 'Raw' that conflicts with color space 'Raw'",
            "Named transform 'view_2' was not merged as there's a color space alias with that name."]);

        assert_eq!(
            merged.num_named_transforms_filtered(NamedTransformVisibility::All),
            3
        );
        let nt = check_named_transform(&merged, "nt_both", 0);
        assert_eq!(nt.num_aliases(), 1);
        assert_eq!(nt.alias(0), "nametr");
        assert_eq!(nt.family(), "Input@");
        assert_eq!(nt.description(), "from input");

        let nt = check_named_transform(&merged, "nt_input", 1);
        assert_eq!(nt.num_aliases(), 1);
        assert_eq!(nt.alias(0), "in nt");
        assert_eq!(nt.family(), "Input@Raw");
        assert_eq!(nt.description(), "from input");

        let nt = check_named_transform(&merged, "nt_base", 2);
        assert_eq!(nt.num_aliases(), 0);
        assert_eq!(nt.family(), "Base@nt");
        assert_eq!(nt.description(), "from base");
    }

    // Test NamedTransform with strategy=PreferInput, options InputFirst = false.
    {
        let mut params = setup(MergeStrategy::PreferInput);
        params.set_input_first(false);

        let mut merged = base.create_editable_copy();
        check_on(LogType::Warning, &base, &input, &params, &mut merged, colorspaces, &["Equivalent input color space 'ACES2065-1' replaces 'ACES2065-1' in the base config, preserving aliases.",
            "Equivalent input color space 'ACEScct - SomeOtherName' replaces 'ACEScct' in the base config, preserving aliases.",
            "Equivalent input color space 'view_1' replaces 'view_1' in the base config, preserving aliases.",
            "Equivalent input color space 'view_1B' replaces 'view_1' in the base config, preserving aliases.",
            "Equivalent input color space 'view_3' replaces 'view_2' in the base config, preserving aliases.",
            "Equivalent input color space 'log_3' replaces 'log_1' in the base config, preserving aliases.",
            "Equivalent input color space 'lin_3' replaces 'ACES2065-1' in the base config, preserving aliases."]);
        check_on(LogType::Warning, &base, &input, &params, &mut merged, named_transforms, &["Named transform 'nt_both' will replace a named transform in the base config",
            "Merged Base named transform 'nt_both' has a conflict with alias 'srgb_tx' in color space 'sRGB - Texture'",
            "Merged Base named transform 'nt_base' has an alias 'view_3' that conflicts with color space 'view_3'",
            "Merged Input named transform 'nt_both' has a conflict with alias 'Utility - Raw' in color space 'Raw'",
            "The name of merged named transform 'nt_input' has a conflict with an alias in named transform 'nt_base'",
            "Merged Input named transform 'nt_input' has an alias 'Raw' that conflicts with color space 'Raw'",
            "Named transform 'view_2' was not merged as there's a color space alias with that name."]);

        assert_eq!(
            merged.num_named_transforms_filtered(NamedTransformVisibility::All),
            3
        );
        let nt = check_named_transform(&merged, "nt_base", 0);
        assert_eq!(nt.num_aliases(), 0);
        assert_eq!(nt.family(), "Base@nt");
        assert_eq!(nt.description(), "from base");

        let nt = check_named_transform(&merged, "nt_both", 1);
        assert_eq!(nt.num_aliases(), 1);
        assert_eq!(nt.alias(0), "nametr");
        assert_eq!(nt.family(), "Input@");
        assert_eq!(nt.description(), "from input");

        let nt = check_named_transform(&merged, "nt_input", 2);
        assert_eq!(nt.num_aliases(), 1);
        assert_eq!(nt.alias(0), "in nt");
        assert_eq!(nt.family(), "Input@Raw");
        assert_eq!(nt.description(), "from input");

        assert_eq!(
            merged.inactive_color_spaces(),
            "Gamma 2.2 AP1 - Texture, Linear Rec.2020, nt_both, nt_input"
        );
    }

    // Test NamedTransform with strategy = PreferBase, options InputFirst = true.
    {
        let params = setup(MergeStrategy::PreferBase);

        let mut merged = base.create_editable_copy();
        check_on(LogType::Warning, &base, &input, &params, &mut merged, colorspaces, &["Equivalent base color space 'ACES2065-1' overrides 'ACES2065-1' in the input config, preserving aliases.",
            "Equivalent base color space 'ACEScct' overrides 'ACEScct - SomeOtherName' in the input config, preserving aliases.",
            "Equivalent base color space 'view_1' overrides 'view_1' in the input config, preserving aliases.",
            "Equivalent base color space 'view_1' overrides 'view_1B' in the input config, preserving aliases.",
            "Equivalent base color space 'view_2' overrides 'view_3' in the input config, preserving aliases.",
            "Equivalent base color space 'log_1' overrides 'log_3' in the input config, preserving aliases.",
            "Equivalent base color space 'ACES2065-1' overrides 'lin_3' in the input config, preserving aliases."]);
        check_on(LogType::Warning, &base, &input, &params, &mut merged, named_transforms, &["Merged Base named transform 'nt_both' has a conflict with alias 'srgb_tx' in color space 'sRGB - Texture'",
            "Merged Base named transform 'nt_base' has a conflict with alias 'view_3' in color space 'view_2'.",
            "Named transform 'nt_both' was not merged as it's already present in the base config",
            "Named transform 'nt_input' was not merged as it conflicts with an alias in named transform 'nt_base'",
            "Named transform 'view_2' was not merged as there's a color space with that name"]);

        assert_eq!(
            merged.num_named_transforms_filtered(NamedTransformVisibility::All),
            2
        );
        let nt = check_named_transform(&merged, "nt_both", 0);
        assert_eq!(nt.num_aliases(), 1);
        assert_eq!(nt.alias(0), "namet2");
        assert_eq!(nt.family(), "Base#");
        assert_eq!(nt.description(), "from base");

        let nt = check_named_transform(&merged, "nt_base", 1);
        assert_eq!(nt.num_aliases(), 1);
        assert_eq!(nt.alias(0), "nt_input");
        assert_eq!(nt.family(), "Base#nt");
        assert_eq!(nt.description(), "from base");

        // NB: The nt_input is included referring to the alias in the base config, not the input config.
        assert_eq!(
            merged.inactive_color_spaces(),
            "Linear Rec.2020, nt_both, view_2, Gamma 2.2 AP1 - Texture"
        );
    }

    // Test NamedTransform with strategy = PreferBase, options InputFirst = false.
    {
        let mut params = setup(MergeStrategy::PreferBase);
        params.set_input_first(false);

        let mut merged = base.create_editable_copy();
        check_on(LogType::Warning, &base, &input, &params, &mut merged, colorspaces, &["Equivalent base color space 'ACES2065-1' overrides 'ACES2065-1' in the input config, preserving aliases.",
            "Equivalent base color space 'ACEScct' overrides 'ACEScct - SomeOtherName' in the input config, preserving aliases.",
            "Equivalent base color space 'view_1' overrides 'view_1' in the input config, preserving aliases.",
            "Equivalent base color space 'view_1' overrides 'view_1B' in the input config, preserving aliases.",
            "Equivalent base color space 'view_2' overrides 'view_3' in the input config, preserving aliases.",
            "Equivalent base color space 'log_1' overrides 'log_3' in the input config, preserving aliases.",
            "Equivalent base color space 'ACES2065-1' overrides 'lin_3' in the input config, preserving aliases."]);
        check_on(LogType::Warning, &base, &input, &params, &mut merged, named_transforms, &["Merged Base named transform 'nt_both' has a conflict with alias 'srgb_tx' in color space 'sRGB - Texture'",
            "Merged Base named transform 'nt_base' has a conflict with alias 'view_3' in color space 'view_2'.",
            "Named transform 'nt_both' was not merged as it's already present in the base config",
            "Named transform 'nt_input' was not merged as it conflicts with an alias in named transform 'nt_base'",
            "Named transform 'view_2' was not merged as there's a color space with that name"]);

        assert_eq!(
            merged.num_named_transforms_filtered(NamedTransformVisibility::All),
            2
        );
        let nt = check_named_transform(&merged, "nt_both", 0);
        assert_eq!(nt.num_aliases(), 1);
        assert_eq!(nt.alias(0), "namet2");
        assert_eq!(nt.family(), "Base#");
        assert_eq!(nt.description(), "from base");

        let nt = check_named_transform(&merged, "nt_base", 1);
        assert_eq!(nt.num_aliases(), 1);
        assert_eq!(nt.alias(0), "nt_input");
        assert_eq!(nt.family(), "Base#nt");
        assert_eq!(nt.description(), "from base");
    }

    // Test NamedTransform with strategy = BaseOnly.
    {
        let params = setup(MergeStrategy::BaseOnly);

        let mut merged = base.create_editable_copy();
        run_on(&base, &input, &params, &mut merged, colorspaces).unwrap();
        run_on(&base, &input, &params, &mut merged, named_transforms).unwrap();

        assert_eq!(
            merged.num_named_transforms_filtered(NamedTransformVisibility::All),
            2
        );
        let nt = check_named_transform(&merged, "nt_both", 0);
        assert_eq!(nt.num_aliases(), 2);
        assert_eq!(nt.alias(0), "srgb_tx");
        assert_eq!(nt.alias(1), "namet2");
        assert_eq!(nt.family(), "");
        assert_eq!(nt.description(), "from base");

        let nt = check_named_transform(&merged, "nt_base", 1);
        assert_eq!(nt.num_aliases(), 2);
        assert_eq!(nt.alias(0), "view_3");
        assert_eq!(nt.alias(1), "nt_input");
        assert_eq!(nt.family(), "nt");
        assert_eq!(nt.description(), "from base");
    }

    // Test NamedTransform with strategy = InputOnly.
    {
        let params = setup(MergeStrategy::InputOnly);

        let mut merged = base.create_editable_copy();
        run_on(&base, &input, &params, &mut merged, colorspaces).unwrap();
        run_on(&base, &input, &params, &mut merged, named_transforms).unwrap();

        assert_eq!(
            merged.num_named_transforms_filtered(NamedTransformVisibility::All),
            3
        );
        let nt = check_named_transform(&merged, "nt_both", 0);
        assert_eq!(nt.num_aliases(), 2);
        assert_eq!(nt.alias(0), "Utility - Raw");
        assert_eq!(nt.alias(1), "nametr");
        assert_eq!(nt.family(), "");
        assert_eq!(nt.description(), "from input");

        let nt = check_named_transform(&merged, "nt_input", 1);
        assert_eq!(nt.num_aliases(), 2);
        assert_eq!(nt.alias(0), "Raw");
        assert_eq!(nt.alias(1), "in nt");
        assert_eq!(nt.family(), "Raw");
        assert_eq!(nt.description(), "from input");

        let nt = check_named_transform(&merged, "view_2", 2);
        assert_eq!(nt.num_aliases(), 1);
        assert_eq!(nt.alias(0), "g22_ap1");
        assert_eq!(nt.family(), "Raw");
        assert_eq!(nt.description(), "from input");
    }

    // Test NamedTransform with strategy = Remove
    {
        let params = setup(MergeStrategy::Remove);

        let mut merged = base.create_editable_copy();
        run_on(&base, &input, &params, &mut merged, colorspaces).unwrap();
        run_on(&base, &input, &params, &mut merged, named_transforms).unwrap();

        assert_eq!(
            merged.num_named_transforms_filtered(NamedTransformVisibility::All),
            1
        );
        let nt = check_named_transform(&merged, "nt_base", 0);
        assert_eq!(nt.num_aliases(), 2);
        assert_eq!(nt.alias(0), "view_3");
        assert_eq!(nt.alias(1), "nt_input");
        assert_eq!(nt.family(), "nt");
        assert_eq!(nt.description(), "from base");
    }
}

#[test]
fn merge_configs_named_transform_section_errors() {
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

    // NT name matches a role name.
    {
        {
            const BASE: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: A}

roles:
    b: Raw

colorspaces:
- !<ColorSpace>
    name: Raw

named_transforms:
  - !<NamedTransform>
    name: A
    categories: [ working-space, basic-3d, advanced-2d ]
    encoding: sdr-video
    transform: !<MatrixTransform> {name: forwardBase, offset: [0.1, 0.2, 0.3, 0.4]}
"#;

            const INPUT: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: B}

named_transforms:
  - !<NamedTransform>
    name: B
    categories: [ working-space, basic-3d, advanced-2d ]
    encoding: sdr-video
    transform: !<MatrixTransform> {name: forwardBase, offset: [0.1, 0.2, 0.3, 0.4]}
"#;
            let base = from_str(BASE);
            let input = from_str(INPUT);

            // The role takes priority.
            {
                let mut params = setup(MergeStrategy::PreferInput);
                params.set_input_first(false);

                let mut merged = base.create_editable_copy();
                run_on(&base, &input, &params, &mut merged, roles).unwrap();
                run_on(&base, &input, &params, &mut merged, colorspaces).unwrap();
                check_on(
                    LogType::Warning,
                    &base,
                    &input,
                    &params,
                    &mut merged,
                    named_transforms,
                    &["Named transform 'B' was not merged as it's identical to a role name"],
                );

                assert_eq!(merged.num_roles(), 1);
                assert_eq!(merged.role_name(0), "b");

                // NamedTransform B should not be added to the merged config (skipped).
                assert_eq!(
                    merged.num_named_transforms_filtered(NamedTransformVisibility::All),
                    1
                );
                assert_eq!(merged.named_transform_name_by_index(0), "A");
            }
            {
                let mut params = setup(MergeStrategy::PreferBase);
                params.set_input_first(false);

                let mut merged = base.create_editable_copy();
                run_on(&base, &input, &params, &mut merged, roles).unwrap();
                run_on(&base, &input, &params, &mut merged, colorspaces).unwrap();
                check_on(
                    LogType::Warning,
                    &base,
                    &input,
                    &params,
                    &mut merged,
                    named_transforms,
                    &["Named transform 'B' was not merged as it's identical to a role name"],
                );

                assert_eq!(merged.num_roles(), 1);
                assert_eq!(merged.role_name(0), "b");

                // NamedTransform B should not be added to the merged config (skipped)
                assert_eq!(
                    merged.num_named_transforms_filtered(NamedTransformVisibility::All),
                    1
                );
                assert_eq!(merged.named_transform_name_by_index(0), "A");
            }
            // Testing the error message when Error on conflict is enabled.
            {
                let mut params = setup(MergeStrategy::PreferInput);
                params.set_error_on_conflict(true);

                let mut merged = base.create_editable_copy();
                assert_err_msg(
                    run_on(&base, &input, &params, &mut merged, named_transforms),
                    "Named transform 'B' was not merged as it's identical to a role name",
                );
            }
        }

        {
            const BASE: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: A}

named_transforms:
  - !<NamedTransform>
    name: A
    transform: !<MatrixTransform> {name: forwardBase, offset: [0.1, 0.2, 0.3, 0.4]}
"#;

            const INPUT: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: B}

roles:
    a: B

colorspaces:
- !<ColorSpace>
    name: B
"#;
            let base = from_str(BASE);
            let input = from_str(INPUT);

            {
                let params = setup(MergeStrategy::PreferInput);

                let mut merged = base.create_editable_copy();
                check_on(LogType::Warning, &base, &input, &params, &mut merged, roles, &["The Input config contains a role 'a' that would override Base config named transform: 'A'"]);
                run_on(&base, &input, &params, &mut merged, colorspaces).unwrap();
                run_on(&base, &input, &params, &mut merged, named_transforms).unwrap();

                assert_eq!(merged.num_roles(), 0);

                assert_eq!(
                    merged.num_named_transforms_filtered(NamedTransformVisibility::All),
                    1
                );
                check_named_transform(&merged, "A", 0);
            }
            {
                let params = setup(MergeStrategy::PreferBase);

                let mut merged = base.create_editable_copy();
                check_on(LogType::Warning, &base, &input, &params, &mut merged, roles, &["The Input config contains a role 'a' that would override Base config named transform: 'A'"]);
                run_on(&base, &input, &params, &mut merged, colorspaces).unwrap();
                run_on(&base, &input, &params, &mut merged, named_transforms).unwrap();

                assert_eq!(merged.num_roles(), 0);

                assert_eq!(
                    merged.num_named_transforms_filtered(NamedTransformVisibility::All),
                    1
                );
                check_named_transform(&merged, "A", 0);
            }
        }
    }

    // NT name matches a color space name.
    {
        {
            const BASE: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: B}

colorspaces:
- !<ColorSpace>
    name: B
- !<ColorSpace>
    name: myB
    aliases: [B1]
"#;

            const INPUT: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: csB}

colorspaces:
- !<ColorSpace>
    name: csB

named_transforms:
  - !<NamedTransform>
    name: B
    categories: [ working-space, basic-3d, advanced-2d ]
    encoding: sdr-video
    transform: !<MatrixTransform> {name: forwardBase, offset: [0.1, 0.2, 0.3, 0.4]}

  - !<NamedTransform>
    name: B1
    categories: [ working-space, basic-3d, advanced-2d ]
    encoding: sdr-video
    transform: !<MatrixTransform> {name: forwardBase, offset: [0.1, 0.2, 0.3, 0.4]}
"#;
            let base = from_str(BASE);
            let input = from_str(INPUT);

            // The role take priority over the inbound colorspace.
            // The conflicting colorspace should not be added to the merged config (skipped).
            {
                let mut params = setup(MergeStrategy::PreferInput);
                params.set_input_first(false);

                let mut merged = base.create_editable_copy();
                run_on(&base, &input, &params, &mut merged, colorspaces).unwrap();
                check_on(LogType::Warning, &base, &input, &params, &mut merged, named_transforms, &["Named transform 'B' was not merged as there's a color space with that name",
                    "Named transform 'B1' was not merged as there's a color space alias with that name"]);

                assert_eq!(
                    merged.num_named_transforms_filtered(NamedTransformVisibility::All),
                    0
                );

                assert_eq!(
                    merged.num_color_spaces_filtered(
                        SearchReferenceSpaceType::All,
                        ColorSpaceVisibility::All
                    ),
                    3
                );
                check_color_space(&merged, "B", 0, SearchReferenceSpaceType::Scene);
                let cs = check_color_space(&merged, "myB", 1, SearchReferenceSpaceType::Scene);
                assert_eq!(cs.num_aliases(), 1);
                assert_eq!(cs.alias(0), "B1");
                check_color_space(&merged, "csB", 2, SearchReferenceSpaceType::Scene);
            }
            {
                let mut params = setup(MergeStrategy::PreferBase);
                params.set_input_first(false);

                let mut merged = base.create_editable_copy();
                run_on(&base, &input, &params, &mut merged, colorspaces).unwrap();
                check_on(LogType::Warning, &base, &input, &params, &mut merged, named_transforms, &["Named transform 'B' was not merged as there's a color space with that name",
                    "Named transform 'B1' was not merged as there's a color space alias with that name"]);

                assert_eq!(
                    merged.num_named_transforms_filtered(NamedTransformVisibility::All),
                    0
                );

                assert_eq!(
                    merged.num_color_spaces_filtered(
                        SearchReferenceSpaceType::All,
                        ColorSpaceVisibility::All
                    ),
                    3
                );
                check_color_space(&merged, "B", 0, SearchReferenceSpaceType::Scene);

                let cs = check_color_space(&merged, "myB", 1, SearchReferenceSpaceType::Scene);
                assert_eq!(cs.num_aliases(), 1);
                assert_eq!(cs.alias(0), "B1");

                check_color_space(&merged, "csB", 2, SearchReferenceSpaceType::Scene);
            }
            // Testing the error message when Error on conflict is enabled.
            {
                let mut params = setup(MergeStrategy::PreferInput);
                params.set_error_on_conflict(true);

                let mut merged = base.create_editable_copy();
                run_on(&base, &input, &params, &mut merged, colorspaces).unwrap();
                assert_err_msg(
                    run_on(&base, &input, &params, &mut merged, named_transforms),
                    "Named transform 'B' was not merged as there's a color space with that name",
                );
            }
        }

        {
            const BASE: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: csA}

colorspaces:
- !<ColorSpace>
    name: csA

named_transforms:
  - !<NamedTransform>
    name: A
    categories: [ working-space, basic-3d, advanced-2d ]
    encoding: sdr-video
    transform: !<MatrixTransform> {name: forwardBase, offset: [0.1, 0.2, 0.3, 0.4]}

  - !<NamedTransform>
    name: A1
    categories: [ working-space, basic-3d, advanced-2d ]
    encoding: sdr-video
    transform: !<MatrixTransform> {name: forwardBase, offset: [0.1, 0.2, 0.3, 0.4]}
"#;

            const INPUT: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: A}

colorspaces:
- !<ColorSpace>
    name: A
- !<ColorSpace>
    name: myA
    aliases: [A1]
"#;
            let base = from_str(BASE);
            let input = from_str(INPUT);
            {
                let params = setup(MergeStrategy::PreferInput);

                let mut merged = base.create_editable_copy();
                run_on(&base, &input, &params, &mut merged, colorspaces).unwrap();
                check_on(LogType::Warning, &base, &input, &params, &mut merged, named_transforms, &["Named transform 'A' was not merged as there's a color space with that name",
                    "Named transform 'A1' was not merged as there's a color space alias with that name"]);

                assert_eq!(
                    merged.num_named_transforms_filtered(NamedTransformVisibility::All),
                    0
                );

                assert_eq!(
                    merged.num_color_spaces_filtered(
                        SearchReferenceSpaceType::All,
                        ColorSpaceVisibility::All
                    ),
                    3
                );
                check_color_space(&merged, "A", 0, SearchReferenceSpaceType::Scene);
                let cs = check_color_space(&merged, "myA", 1, SearchReferenceSpaceType::Scene);
                assert_eq!(cs.num_aliases(), 1);
                assert_eq!(cs.alias(0), "A1");
                check_color_space(&merged, "csA", 2, SearchReferenceSpaceType::Scene);
            }
            {
                let params = setup(MergeStrategy::PreferBase);

                let mut merged = base.create_editable_copy();
                run_on(&base, &input, &params, &mut merged, colorspaces).unwrap();
                check_on(LogType::Warning, &base, &input, &params, &mut merged, named_transforms, &["Named transform 'A' was not merged as there's a color space with that name",
                    "Named transform 'A1' was not merged as there's a color space alias with that name"]);

                assert_eq!(
                    merged.num_named_transforms_filtered(NamedTransformVisibility::All),
                    0
                );

                assert_eq!(
                    merged.num_color_spaces_filtered(
                        SearchReferenceSpaceType::All,
                        ColorSpaceVisibility::All
                    ),
                    3
                );
                check_color_space(&merged, "A", 0, SearchReferenceSpaceType::Scene);
                let cs = check_color_space(&merged, "myA", 1, SearchReferenceSpaceType::Scene);
                assert_eq!(cs.num_aliases(), 1);
                assert_eq!(cs.alias(0), "A1");
                check_color_space(&merged, "csA", 2, SearchReferenceSpaceType::Scene);
            }
        }
    }

    // NT name matches an NT alias.
    {
        {
            const BASE: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: csA}

colorspaces:
- !<ColorSpace>
    name: csA

named_transforms:
  - !<NamedTransform>
    name: A
    aliases: [B]
    categories: [ working-space, basic-3d, advanced-2d ]
    encoding: sdr-video
    transform: !<MatrixTransform> {name: forwardBase, offset: [0.1, 0.2, 0.3, 0.4]}
"#;

            const INPUT: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: csB}

colorspaces:
- !<ColorSpace>
    name: csB

named_transforms:
  - !<NamedTransform>
    name: B
    categories: [ working-space, basic-3d, advanced-2d ]
    encoding: sdr-video
    transform: !<MatrixTransform> {name: forwardBase, offset: [0.1, 0.2, 0.3, 0.4]}
"#;
            let base = from_str(BASE);
            let input = from_str(INPUT);
            {
                let mut params = setup(MergeStrategy::PreferInput);
                params.set_input_first(false);

                let mut merged = base.create_editable_copy();
                run_on(&base, &input, &params, &mut merged, colorspaces).unwrap();
                check_on(LogType::Warning, &base, &input, &params, &mut merged, named_transforms, &["The name of merged named transform 'B' has a conflict with an alias in named transform 'A'"]);

                assert_eq!(
                    merged.num_color_spaces_filtered(
                        SearchReferenceSpaceType::All,
                        ColorSpaceVisibility::All
                    ),
                    2
                );

                assert_eq!(
                    merged.num_named_transforms_filtered(NamedTransformVisibility::All),
                    2
                );
                let nt = check_named_transform(&merged, "A", 0);
                assert_eq!(nt.num_aliases(), 0);
                check_named_transform(&merged, "B", 1);
            }
            {
                let mut params = setup(MergeStrategy::PreferInput);
                params.set_input_first(false);

                let mut merged = base.create_editable_copy();
                run_on(&base, &input, &params, &mut merged, colorspaces).unwrap();
                check_on(LogType::Warning, &base, &input, &params, &mut merged, named_transforms, &["The name of merged named transform 'B' has a conflict with an alias in named transform 'A'"]);

                assert_eq!(
                    merged.num_color_spaces_filtered(
                        SearchReferenceSpaceType::All,
                        ColorSpaceVisibility::All
                    ),
                    2
                );

                assert_eq!(
                    merged.num_named_transforms_filtered(NamedTransformVisibility::All),
                    2
                );
                let nt = check_named_transform(&merged, "A", 0);
                assert_eq!(nt.num_aliases(), 0);
                check_named_transform(&merged, "B", 1);
            }
            {
                let mut params = setup(MergeStrategy::PreferInput);
                params.set_error_on_conflict(true);

                let mut merged = base.create_editable_copy();
                run_on(&base, &input, &params, &mut merged, colorspaces).unwrap();
                assert_err_msg(run_on(&base, &input, &params, &mut merged, named_transforms), "The name of merged named transform 'B' has a conflict with an alias in named transform 'A'");
            }
        }

        {
            const BASE: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: csA}

colorspaces:
- !<ColorSpace>
    name: csA

named_transforms:
  - !<NamedTransform>
    name: A
    categories: [ working-space, basic-3d, advanced-2d ]
    encoding: sdr-video
    transform: !<MatrixTransform> {name: forwardBase, offset: [0.1, 0.2, 0.3, 0.4]}
"#;

            const INPUT: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: B}

colorspaces:
- !<ColorSpace>
    name: csB

named_transforms:
  - !<NamedTransform>
    name: B
    aliases: [A]
    categories: [ working-space, basic-3d, advanced-2d ]
    encoding: sdr-video
    transform: !<MatrixTransform> {name: forwardBase, offset: [0.1, 0.2, 0.3, 0.4]}
"#;
            let base = from_str(BASE);
            let input = from_str(INPUT);
            {
                let params = setup(MergeStrategy::PreferInput);

                let mut merged = base.create_editable_copy();
                run_on(&base, &input, &params, &mut merged, colorspaces).unwrap();
                check_on(LogType::Warning, &base, &input, &params, &mut merged, named_transforms, &["Merged Input named transform 'B' has an alias 'A' that conflicts with named transform 'A'"]);

                assert_eq!(
                    merged.num_color_spaces_filtered(
                        SearchReferenceSpaceType::All,
                        ColorSpaceVisibility::All
                    ),
                    2
                );

                assert_eq!(
                    merged.num_named_transforms_filtered(NamedTransformVisibility::All),
                    1
                );

                let nt = check_named_transform(&merged, "B", 0);
                assert_eq!(nt.num_aliases(), 1);
                assert_eq!(nt.alias(0), "A");
            }
            {
                let mut merged = base.create_editable_copy();

                let params = setup(MergeStrategy::PreferBase);
                run_on(&base, &input, &params, &mut merged, colorspaces).unwrap();
                check_on(LogType::Warning, &base, &input, &params, &mut merged, named_transforms, &["Merged Input named transform 'B' has an alias 'A' that conflicts with named transform 'A'"]);

                assert_eq!(
                    merged.num_color_spaces_filtered(
                        SearchReferenceSpaceType::All,
                        ColorSpaceVisibility::All
                    ),
                    2
                );

                assert_eq!(
                    merged.num_named_transforms_filtered(NamedTransformVisibility::All),
                    2
                );
                let nt = check_named_transform(&merged, "B", 0);
                assert_eq!(nt.num_aliases(), 0);
                let nt = check_named_transform(&merged, "A", 1);
                assert_eq!(nt.num_aliases(), 0);
            }
        }
    }

    // NT alias matches a role name.
    {
        {
            const BASE: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: csA}

roles:
    b1: csA

colorspaces:
- !<ColorSpace>
    name: csA
"#;

            const INPUT: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: csB}

colorspaces:
- !<ColorSpace>
    name: csB

named_transforms:
  - !<NamedTransform>
    name: B
    aliases: [B1]
    categories: [ working-space, basic-3d, advanced-2d ]
    encoding: sdr-video
    transform: !<MatrixTransform> {name: forwardBase, offset: [0.1, 0.2, 0.3, 0.4]}
"#;
            let base = from_str(BASE);
            let input = from_str(INPUT);
            {
                let mut params = setup(MergeStrategy::PreferInput);
                params.set_input_first(false);

                let mut merged = base.create_editable_copy();
                run_on(&base, &input, &params, &mut merged, roles).unwrap();
                run_on(&base, &input, &params, &mut merged, colorspaces).unwrap();
                check_on(LogType::Warning, &base, &input, &params, &mut merged, named_transforms, &["Merged Input named transform 'B' has an alias 'B1' that conflicts with a role"]);

                assert_eq!(merged.num_roles(), 1);
                assert_eq!(merged.role_name(0), "b1");

                assert_eq!(
                    merged.num_color_spaces_filtered(
                        SearchReferenceSpaceType::All,
                        ColorSpaceVisibility::All
                    ),
                    2
                );

                assert_eq!(
                    merged.num_named_transforms_filtered(NamedTransformVisibility::All),
                    1
                );

                let nt = check_named_transform(&merged, "B", 0);
                assert_eq!(nt.num_aliases(), 0);
            }
            {
                let mut params = setup(MergeStrategy::PreferBase);
                params.set_input_first(false);

                let mut merged = base.create_editable_copy();
                run_on(&base, &input, &params, &mut merged, roles).unwrap();
                run_on(&base, &input, &params, &mut merged, colorspaces).unwrap();
                check_on(LogType::Warning, &base, &input, &params, &mut merged, named_transforms, &["Merged Input named transform 'B' has an alias 'B1' that conflicts with a role"]);

                assert_eq!(merged.num_roles(), 1);
                assert_eq!(merged.role_name(0), "b1");

                assert_eq!(
                    merged.num_color_spaces_filtered(
                        SearchReferenceSpaceType::All,
                        ColorSpaceVisibility::All
                    ),
                    2
                );

                assert_eq!(
                    merged.num_named_transforms_filtered(NamedTransformVisibility::All),
                    1
                );

                let nt = check_named_transform(&merged, "B", 0);
                assert_eq!(nt.num_aliases(), 0);
            }
            // Testing the error message when Error on conflict is enabled.
            {
                let mut params = setup(MergeStrategy::PreferInput);
                params.set_error_on_conflict(true);

                let mut merged = base.create_editable_copy();
                run_on(&base, &input, &params, &mut merged, roles).unwrap();
                run_on(&base, &input, &params, &mut merged, colorspaces).unwrap();
                assert_err_msg(
                    run_on(&base, &input, &params, &mut merged, named_transforms),
                    "Merged Input named transform 'B' has an alias 'B1' that conflicts with a role",
                );
            }
        }

        {
            const BASE: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: csA}

colorspaces:
- !<ColorSpace>
    name: csA

named_transforms:
  - !<NamedTransform>
    name: B
    aliases: [A1]
    categories: [ working-space, basic-3d, advanced-2d ]
    encoding: sdr-video
    transform: !<MatrixTransform> {name: forwardBase, offset: [0.1, 0.2, 0.3, 0.4]}
"#;

            const INPUT: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: csB}

roles:
    a1: csB

colorspaces:
- !<ColorSpace>
    name: csB
"#;
            let base = from_str(BASE);
            let input = from_str(INPUT);
            {
                let mut params = setup(MergeStrategy::PreferInput);
                params.set_input_first(false);

                let mut merged = base.create_editable_copy();
                check_on(LogType::Warning, &base, &input, &params, &mut merged, roles, &["The Input config contains a role 'a1' that would override an alias of Base config named transform: 'B'"]);
                run_on(&base, &input, &params, &mut merged, colorspaces).unwrap();
                run_on(&base, &input, &params, &mut merged, named_transforms).unwrap();

                assert_eq!(merged.num_roles(), 0);

                assert_eq!(
                    merged.num_color_spaces_filtered(
                        SearchReferenceSpaceType::All,
                        ColorSpaceVisibility::All
                    ),
                    2
                );

                assert_eq!(
                    merged.num_named_transforms_filtered(NamedTransformVisibility::All),
                    1
                );

                let nt = check_named_transform(&merged, "B", 0);
                assert_eq!(nt.num_aliases(), 1);
                assert_eq!(nt.alias(0), "A1");
            }
            {
                let mut params = setup(MergeStrategy::PreferInput);
                params.set_input_first(false);

                let mut merged = base.create_editable_copy();
                check_on(LogType::Warning, &base, &input, &params, &mut merged, roles, &["The Input config contains a role 'a1' that would override an alias of Base config named transform: 'B'"]);
                run_on(&base, &input, &params, &mut merged, colorspaces).unwrap();
                run_on(&base, &input, &params, &mut merged, named_transforms).unwrap();

                assert_eq!(merged.num_roles(), 0);

                assert_eq!(
                    merged.num_color_spaces_filtered(
                        SearchReferenceSpaceType::All,
                        ColorSpaceVisibility::All
                    ),
                    2
                );

                assert_eq!(
                    merged.num_named_transforms_filtered(NamedTransformVisibility::All),
                    1
                );

                let nt = check_named_transform(&merged, "B", 0);
                assert_eq!(nt.num_aliases(), 1);
                assert_eq!(nt.alias(0), "A1");
            }
        }
    }

    // NT name matches color space alias.
    {
        {
            const BASE: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: csA}

colorspaces:
- !<ColorSpace>
    name: csA
- !<ColorSpace>
    name: B
- !<ColorSpace>
    name: myB
    aliases: [B1]
"#;

            const INPUT: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: csB}

colorspaces:
- !<ColorSpace>
    name: csB

named_transforms:
  - !<NamedTransform>
    name: B
    categories: [ working-space, basic-3d, advanced-2d ]
    encoding: sdr-video
    transform: !<MatrixTransform> {name: forwardBase, offset: [0.1, 0.2, 0.3, 0.4]}

  - !<NamedTransform>
    name: B1
    categories: [ working-space, basic-3d, advanced-2d ]
    encoding: sdr-video
    transform: !<MatrixTransform> {name: forwardBase, offset: [0.1, 0.2, 0.3, 0.4]}
"#;
            let base = from_str(BASE);
            let input = from_str(INPUT);

            {
                let mut params = setup(MergeStrategy::PreferInput);
                params.set_input_first(false);

                let mut merged = base.create_editable_copy();
                run_on(&base, &input, &params, &mut merged, colorspaces).unwrap();
                check_on(LogType::Warning, &base, &input, &params, &mut merged, named_transforms, &["Named transform 'B' was not merged as there's a color space with that name",
                    "Named transform 'B1' was not merged as there's a color space alias with that name"]);

                assert_eq!(
                    merged.num_named_transforms_filtered(NamedTransformVisibility::All),
                    0
                );

                assert_eq!(
                    merged.num_color_spaces_filtered(
                        SearchReferenceSpaceType::All,
                        ColorSpaceVisibility::All
                    ),
                    4
                );
                check_color_space(&merged, "csA", 0, SearchReferenceSpaceType::Scene);
                check_color_space(&merged, "B", 1, SearchReferenceSpaceType::Scene);
                let cs = check_color_space(&merged, "myB", 2, SearchReferenceSpaceType::Scene);
                assert_eq!(cs.num_aliases(), 1);
                assert_eq!(cs.alias(0), "B1");
                check_color_space(&merged, "csB", 3, SearchReferenceSpaceType::Scene);
            }
            {
                let mut params = setup(MergeStrategy::PreferBase);
                params.set_input_first(false);

                let mut merged = base.create_editable_copy();
                run_on(&base, &input, &params, &mut merged, colorspaces).unwrap();
                check_on(LogType::Warning, &base, &input, &params, &mut merged, named_transforms, &["Named transform 'B' was not merged as there's a color space with that name",
                    "Named transform 'B1' was not merged as there's a color space alias with that name"]);

                assert_eq!(
                    merged.num_named_transforms_filtered(NamedTransformVisibility::All),
                    0
                );

                assert_eq!(
                    merged.num_color_spaces_filtered(
                        SearchReferenceSpaceType::All,
                        ColorSpaceVisibility::All
                    ),
                    4
                );
                check_color_space(&merged, "csA", 0, SearchReferenceSpaceType::Scene);
                check_color_space(&merged, "B", 1, SearchReferenceSpaceType::Scene);
                let cs = check_color_space(&merged, "myB", 2, SearchReferenceSpaceType::Scene);
                assert_eq!(cs.num_aliases(), 1);
                assert_eq!(cs.alias(0), "B1");
                check_color_space(&merged, "csB", 3, SearchReferenceSpaceType::Scene);
            }
            // Testing the error message when Error on conflict is enabled.
            {
                let mut params = setup(MergeStrategy::PreferInput);
                params.set_error_on_conflict(true);

                let mut merged = base.create_editable_copy();
                run_on(&base, &input, &params, &mut merged, colorspaces).unwrap();
                assert_err_msg(
                    run_on(&base, &input, &params, &mut merged, named_transforms),
                    "Named transform 'B' was not merged as there's a color space with that name",
                );
            }
        }

        {
            const BASE: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: csA}

colorspaces:
- !<ColorSpace>
    name: csA

named_transforms:
  - !<NamedTransform>
    name: A
    categories: [ working-space, basic-3d, advanced-2d ]
    encoding: sdr-video
    transform: !<MatrixTransform> {name: forwardBase, offset: [0.1, 0.2, 0.3, 0.4]}

  - !<NamedTransform>
    name: A1
    categories: [ working-space, basic-3d, advanced-2d ]
    encoding: sdr-video
    transform: !<MatrixTransform> {name: forwardBase, offset: [0.1, 0.2, 0.3, 0.4]}
"#;

            const INPUT: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: csB}

colorspaces:
- !<ColorSpace>
    name: csB
- !<ColorSpace>
    name: A
- !<ColorSpace>
    name: myA
    aliases: [A1]
"#;
            let base = from_str(BASE);
            let input = from_str(INPUT);
            {
                let mut params = setup(MergeStrategy::PreferInput);
                params.set_input_first(false);

                let mut merged = base.create_editable_copy();
                run_on(&base, &input, &params, &mut merged, colorspaces).unwrap();
                check_on(LogType::Warning, &base, &input, &params, &mut merged, named_transforms, &["Named transform 'A' was not merged as there's a color space with that name",
                    "Named transform 'A1' was not merged as there's a color space alias with that name"]);

                assert_eq!(
                    merged.num_named_transforms_filtered(NamedTransformVisibility::All),
                    0
                );

                assert_eq!(
                    merged.num_color_spaces_filtered(
                        SearchReferenceSpaceType::All,
                        ColorSpaceVisibility::All
                    ),
                    4
                );
                check_color_space(&merged, "csA", 0, SearchReferenceSpaceType::Scene);
                check_color_space(&merged, "csB", 1, SearchReferenceSpaceType::Scene);
                check_color_space(&merged, "A", 2, SearchReferenceSpaceType::Scene);
                let cs = check_color_space(&merged, "myA", 3, SearchReferenceSpaceType::Scene);
                assert_eq!(cs.num_aliases(), 1);
                assert_eq!(cs.alias(0), "A1");
            }
            {
                let mut params = setup(MergeStrategy::PreferBase);
                params.set_input_first(false);

                let mut merged = base.create_editable_copy();
                run_on(&base, &input, &params, &mut merged, colorspaces).unwrap();
                check_on(LogType::Warning, &base, &input, &params, &mut merged, named_transforms, &["Named transform 'A' was not merged as there's a color space with that name",
                    "Named transform 'A1' was not merged as there's a color space alias with that name"]);

                assert_eq!(
                    merged.num_named_transforms_filtered(NamedTransformVisibility::All),
                    0
                );

                assert_eq!(
                    merged.num_color_spaces_filtered(
                        SearchReferenceSpaceType::All,
                        ColorSpaceVisibility::All
                    ),
                    4
                );
                check_color_space(&merged, "csA", 0, SearchReferenceSpaceType::Scene);
                check_color_space(&merged, "csB", 1, SearchReferenceSpaceType::Scene);
                check_color_space(&merged, "A", 2, SearchReferenceSpaceType::Scene);
                let cs = check_color_space(&merged, "myA", 3, SearchReferenceSpaceType::Scene);
                assert_eq!(cs.num_aliases(), 1);
                assert_eq!(cs.alias(0), "A1");
            }
        }
    }

    // NT alias matches existing NT alias.
    {
        {
            const BASE: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: csA}

colorspaces:
- !<ColorSpace>
    name: csA

named_transforms:
  - !<NamedTransform>
    name: A
    aliases: [my_colorspace]
    categories: [ working-space, basic-3d, advanced-2d ]
    encoding: sdr-video
    transform: !<MatrixTransform> {name: forwardBase, offset: [0.1, 0.2, 0.3, 0.4]}
"#;

            const INPUT: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: csB}

colorspaces:
- !<ColorSpace>
    name: csB

named_transforms:
  - !<NamedTransform>
    name: B
    aliases: [my_colorspace]
    categories: [ working-space, basic-3d, advanced-2d ]
    encoding: sdr-video
    transform: !<MatrixTransform> {name: forwardBase, offset: [0.1, 0.2, 0.3, 0.4]}
"#;
            let base = from_str(BASE);
            let input = from_str(INPUT);

            {
                let mut params = setup(MergeStrategy::PreferInput);
                params.set_input_first(false);

                let mut merged = base.create_editable_copy();
                run_on(&base, &input, &params, &mut merged, colorspaces).unwrap();
                check_on(LogType::Warning, &base, &input, &params, &mut merged, named_transforms, &["Merged Input named transform 'B' has a conflict with alias 'my_colorspace' in named transform 'A'"]);

                assert_eq!(
                    merged.num_color_spaces_filtered(
                        SearchReferenceSpaceType::All,
                        ColorSpaceVisibility::All
                    ),
                    2
                );

                assert_eq!(
                    merged.num_named_transforms_filtered(NamedTransformVisibility::All),
                    2
                );
                let nt = check_named_transform(&merged, "A", 0);
                assert_eq!(nt.num_aliases(), 0);
                let nt = check_named_transform(&merged, "B", 1);
                assert_eq!(nt.num_aliases(), 1);
                assert_eq!(nt.alias(0), "my_colorspace");
            }
            {
                let params = setup(MergeStrategy::PreferInput);

                let mut merged = base.create_editable_copy();
                run_on(&base, &input, &params, &mut merged, colorspaces).unwrap();
                check_on(LogType::Warning, &base, &input, &params, &mut merged, named_transforms, &["Merged Input named transform 'B' has a conflict with alias 'my_colorspace' in named transform 'A'"]);

                assert_eq!(
                    merged.num_color_spaces_filtered(
                        SearchReferenceSpaceType::All,
                        ColorSpaceVisibility::All
                    ),
                    2
                );

                assert_eq!(
                    merged.num_named_transforms_filtered(NamedTransformVisibility::All),
                    2
                );
                let nt = check_named_transform(&merged, "B", 0);
                assert_eq!(nt.num_aliases(), 1);
                assert_eq!(nt.alias(0), "my_colorspace");
                let nt = check_named_transform(&merged, "A", 1);
                assert_eq!(nt.num_aliases(), 0);
            }
            {
                let mut params = setup(MergeStrategy::PreferBase);
                params.set_input_first(false);

                let mut merged = base.create_editable_copy();
                run_on(&base, &input, &params, &mut merged, colorspaces).unwrap();
                check_on(LogType::Warning, &base, &input, &params, &mut merged, named_transforms, &["Merged Input named transform 'B' has a conflict with alias 'my_colorspace' in named transform 'A'"]);

                assert_eq!(
                    merged.num_color_spaces_filtered(
                        SearchReferenceSpaceType::All,
                        ColorSpaceVisibility::All
                    ),
                    2
                );

                assert_eq!(
                    merged.num_named_transforms_filtered(NamedTransformVisibility::All),
                    2
                );
                let nt = check_named_transform(&merged, "A", 0);
                assert_eq!(nt.num_aliases(), 1);
                assert_eq!(nt.alias(0), "my_colorspace");
                let nt = check_named_transform(&merged, "B", 1);
                assert_eq!(nt.num_aliases(), 0);
            }
            {
                let params = setup(MergeStrategy::PreferBase);

                let mut merged = base.create_editable_copy();
                run_on(&base, &input, &params, &mut merged, colorspaces).unwrap();
                check_on(LogType::Warning, &base, &input, &params, &mut merged, named_transforms, &["Merged Input named transform 'B' has a conflict with alias 'my_colorspace' in named transform 'A'"]);

                assert_eq!(
                    merged.num_color_spaces_filtered(
                        SearchReferenceSpaceType::All,
                        ColorSpaceVisibility::All
                    ),
                    2
                );

                assert_eq!(
                    merged.num_named_transforms_filtered(NamedTransformVisibility::All),
                    2
                );
                let nt = check_named_transform(&merged, "B", 0);
                assert_eq!(nt.num_aliases(), 0);
                let nt = check_named_transform(&merged, "A", 1);
                assert_eq!(nt.num_aliases(), 1);
                assert_eq!(nt.alias(0), "my_colorspace");
            }
            // Testing the error message when Error on conflict is enabled.
            {
                let mut params = setup(MergeStrategy::PreferInput);
                params.set_error_on_conflict(true);

                let mut merged = base.create_editable_copy();
                run_on(&base, &input, &params, &mut merged, colorspaces).unwrap();
                assert_err_msg(run_on(&base, &input, &params, &mut merged, named_transforms), "Merged Input named transform 'B' has a conflict with alias 'my_colorspace' in named transform 'A'");
            }
        }
    }
}
