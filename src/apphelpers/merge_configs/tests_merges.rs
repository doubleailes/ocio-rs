//! Port of `MergeConfigsHelpers_tests.cpp` (part 5: merges driven by OCIOM
//! files, OCIOZ archives, in memory configs and single color spaces).

use super::tests::*;
use super::*;
use crate::transforms::Transform;
use crate::types::*;

/// First value of the matrix of the first transform of the processor.
#[track_caller]
fn first_matrix_value(config: &Config, ctx: &crate::Context, src: &str, dst: &str) -> f64 {
    let processor = config
        .get_processor_with_context_names(ctx, src, dst)
        .unwrap();
    match processor.create_group_transform().transforms.first() {
        Some(Transform::Matrix(m)) => m.matrix[0],
        other => panic!("expected a matrix transform, got {other:?}"),
    }
}

#[test]
fn merge_configs_merges_with_ociom_file() {
    {
        let ociom_path = merge_file("merged1/merged1.ociom");

        // PreferInput, Input first.
        let merger = ConfigMerger::create_from_file(&ociom_path).unwrap();
        let mut new_merger = None;
        check_for_log_or_exception(
            LogType::Warning,
            || {
                new_merger = Some(merger.merge_configs()?);
                Ok(())
            },
            &[
                "The Input config contains a value that would override the Base config: file_rules: Default",
                "Merged color space 'ACES2065-1' has a conflict with alias 'aces' in color space 'ACEScg'",
                "Equivalent input color space 'sRGB - Display' replaces 'sRGB - Display' in the base config, preserving aliases.",
                "Equivalent input color space 'CIE-XYZ-D65' replaces 'CIE-XYZ-D65' in the base config, preserving aliases.",
                "Equivalent input color space 'ACES2065-1' replaces 'ap0' in the base config, preserving aliases.",
                "Equivalent input color space 'sRGB' replaces 'sRGB - Texture' in the base config, preserving aliases.",
            ],
        );
        let new_merger = new_merger.unwrap();
        let merged = new_merger.merged_config().unwrap();

        // This test is essentially the same as the first sub-test of
        // colorspaces_section_common_reference_and_duplicates, so just do a quick sanity test.
        assert_eq!(merged.name(), "Merged1");
        assert_eq!(merged.description(), "Basic merge with default strategy");
        assert_eq!(
            merged.num_color_spaces_filtered(
                SearchReferenceSpaceType::All,
                ColorSpaceVisibility::All
            ),
            7
        );
    }

    // Test is similar to the previous one but it has two merges in the OCIOM file and it is
    // using the output of the first merged config as the input for the second merge.
    {
        let ociom_path = merge_file("merged2/merged.ociom");
        let merger = ConfigMerger::create_from_file(&ociom_path).unwrap();
        let mut new_merger = None;
        check_for_log_or_exception(
            LogType::Warning,
            || {
                new_merger = Some(merger.merge_configs()?);
                Ok(())
            },
            &[
                "The Input config contains a value that would override the Base config: file_rules: Default",
                "The Input config contains a role that would override Base config role 'cie_xyz_d65_interchange'",
                "Color space 'sRGB - Display' was not merged as it's already present in the base config",
                "Color space 'CIE-XYZ-D65' was not merged as it's already present in the base config",
                "Merged color space 'ACES2065-1' has a conflict with alias 'aces' in color space 'ACEScg'",
                "Color space 'sRGB' was not merged as it conflicts with an alias in color space 'sRGB - Texture'",
                "Equivalent base color space 'ap0' overrides 'rec709' in the input config, preserving aliases",
            ],
        );
        let new_merger = new_merger.unwrap();
        assert_eq!(new_merger.num_merged_configs(), 2);

        let merged = new_merger.merged_config().unwrap();
        merged.validate().unwrap();

        assert_eq!(merged.name(), "Merged2");
        assert_eq!(merged.description(), "Description override");
        assert_eq!(
            merged.num_color_spaces_filtered(
                SearchReferenceSpaceType::All,
                ColorSpaceVisibility::All
            ),
            7
        );

        // The second merge replaces the file rules of the first merge with the input config.
        let fr = merged.file_rules();
        assert_eq!(fr.num_entries(), 1);
        assert_eq!(fr.name(0).unwrap(), "Default");
        assert_eq!(fr.color_space(0).unwrap(), "sRGB");

        // The second merge replaces the roles of the first merge with the input config.
        assert_eq!(merged.num_roles(), 1);
        assert_eq!(
            merged.role_color_space("cie_xyz_d65_interchange"),
            "CIE-XYZ-D65"
        );

        // The rest of the config should be the result of the first merge.
        assert_eq!(merged.num_displays_all(), 1);
        assert_eq!(merged.display(0), "sRGB - Display");
        assert_eq!(
            merged.num_views_by_type(ViewType::DisplayDefined, "sRGB - Display"),
            2
        );

        let cs = check_color_space(
            merged,
            "sRGB - Display",
            0,
            SearchReferenceSpaceType::Display,
        );
        assert_eq!(cs.num_aliases(), 1);
        assert_eq!(cs.alias(0), "srgb_display");
        assert_eq!(cs.family(), "Display-Basic");
        assert_eq!(cs.description(), "from base");

        let cs = check_color_space(merged, "ACES2065-1", 0, SearchReferenceSpaceType::Scene);
        assert_eq!(cs.num_aliases(), 0);
        assert_eq!(cs.family(), "ACES~Linear");
        assert_eq!(cs.description(), "from input");

        assert_eq!(merged.inactive_color_spaces(), "ACES2065-1");
    }

    // Test with external LUT files.
    {
        let ociom_path = merge_file("merged3/merged.ociom");

        // PreferInput, Input first.
        let merger = ConfigMerger::create_from_file(&ociom_path).unwrap();
        let mut new_merger = None;
        check_for_log_or_exception(
            LogType::Warning,
            || {
                new_merger = Some(merger.merge_configs()?);
                Ok(())
            },
            &["Color space 'raw' will replace a color space in the base config."],
        );
        let new_merger = new_merger.unwrap();
        let merged = new_merger.merged_config().unwrap();
        merged.validate().unwrap();

        assert_eq!(merged.search_path(), "./$SHOT:./shot1:shot2:.");
        let cs = merged.get_color_space("shot1_lut1_cs").unwrap();
        let tf = cs.transform(ColorSpaceDirection::ToReference).unwrap();
        match tf {
            Transform::File(f) => assert_eq!(f.src, "shot1/lut1.clf"),
            other => panic!("expected a file transform, got {other:?}"),
        }
        merged
            .get_processor_for_transform(tf, TransformDirection::Forward)
            .unwrap();

        let look = merged.look("shot_look").unwrap();
        let ltf = look.transform().unwrap();
        merged
            .get_processor_for_transform(ltf, TransformDirection::Forward)
            .unwrap();
    }

    // Test that a merge could go wrong if the search_paths are merged with a different strategy
    // than the other sections.
    {
        let ociom_path = merge_file("merged3/merged.ociom");
        let mut merger = ConfigMerger::create_from_file(&ociom_path).unwrap();
        // Changing the strategy for colorspace merger to INPUT_ONLY. This will break the look
        // "shot_look" (from base) as it needs the search paths from the base config
        // (search_paths are managed by the colorspace merger).
        merger
            .params_mut(0)
            .unwrap()
            .set_colorspaces(MergeStrategy::InputOnly);
        // The rest of the merges uses PreferInput strategy.

        let new_merger = merger.merge_configs().unwrap();
        let merged = new_merger.merged_config().unwrap();

        let look = merged.look("shot_look").unwrap();
        let ltf = look.transform().unwrap();

        // Expected to fail as the search_paths were merged following the InputOnly strategy and
        // the looks were merged following the PreferInput (see OCIOM file default strategy).
        // Therefore, the look's FileTransform can not find "look.cdl".
        assert!(merged
            .get_processor_for_transform(ltf, TransformDirection::Forward)
            .is_err());

        // It can happen with any section that uses the search_paths such as looks, named
        // transforms, and colorspaces.
    }

    // Test with a built-in config.
    {
        let ociom_path = merge_file("merged4/merged.ociom");

        // InputOnly.
        let merger = ConfigMerger::create_from_file(&ociom_path).unwrap();
        let new_merger = merger.merge_configs().unwrap();
        let merged = new_merger.merged_config().unwrap();

        merged.validate().unwrap();

        assert_eq!(merged.name(), "cg-config-v1.0.0_aces-v1.3_ocio-v2.1");
        assert_eq!(
            merged.num_color_spaces_filtered(
                SearchReferenceSpaceType::All,
                ColorSpaceVisibility::All
            ),
            20
        );
    }
}

#[test]
fn merge_configs_merges_with_ocioz_file() {
    let archive = format!(
        "{}/configs/context_test1/context_test1_linux.ocioz",
        test_files_dir()
    );
    let base = Config::create_from_file(&archive).unwrap();
    let input = get_config("merged3/input.ocio");

    let mut params = ConfigMergingParameters::new();
    params.set_input_first(false);
    let strategy = MergeStrategy::PreferInput;
    params.set_roles(strategy);
    params.set_colorspaces(strategy);
    params.set_named_transforms(strategy);
    params.set_default_strategy(strategy);
    params.set_input_family_prefix("Input/");
    params.set_base_family_prefix("Base/");
    params.set_adjust_input_reference_space(false);
    params.set_avoid_duplicates(false);

    let mut merged = None;
    check_for_log_or_exception(
        LogType::Warning,
        || {
            merged = Some(merge_configs(&params, &base, &input)?);
            Ok(())
        },
        &[
            "Color space 'reference' will replace a color space in the base config.",
            "Color space 'raw' will replace a color space in the base config.",
            "Color space 'plain_lut1_cs' will replace a color space in the base config.",
            "Color space 'shot1_lut1_cs' will replace a color space in the base config.",
        ],
    );
    let merged = merged.unwrap();

    // The working dir is the one of the base (OCIO: empty for an archive; this port extracts
    // the archive in a directory used as the working dir).
    assert_eq!(merged.working_dir(), base.working_dir());
    // Note: OCIO also checks that the config IO proxy (i.e. the archive) is kept by the merge,
    // the resolution of the files below from the archive checks it.

    let mut ctx = merged.current_context().clone();

    // This is resolved in the OCIOZ base.
    // Note: the $SHOT = shot4 search path takes precedence for this color space.
    // It is 40 in the base config, 20 in input.
    assert_eq!(
        first_matrix_value(&merged, &ctx, "plain_lut1_cs", "reference"),
        40.0
    );
    // It is 10 in the base config, 100 in input.
    assert_eq!(
        first_matrix_value(&merged, &ctx, "shot1_lut1_cs", "reference"),
        10.0
    );
    // Will try to resolve it using relative paths and won't find it.
    assert!(merged
        .get_processor_with_context_names(&ctx, "shot1_lut2_cs", "reference")
        .is_err());

    // Add an absolute search path for the input config.
    let search_path_input = crate::path_utils::normpath(&format!(
        "{}/configs/mergeconfigs/merged3",
        test_files_dir()
    ));
    ctx.clear_search_paths();
    ctx.add_search_path(&search_path_input);
    for i in 0..merged.num_search_paths() {
        ctx.add_search_path(merged.search_path_by_index(i));
    }

    // It doesn't exist in the base config.
    assert_eq!(
        first_matrix_value(&merged, &ctx, "shot1_lut2_cs", "reference"),
        42.0
    );

    merged.clear_processor_cache();

    // It is 10 in the base config, 100 in input.
    assert_eq!(
        first_matrix_value(&merged, &ctx, "shot1_lut1_cs", "reference"),
        100.0
    );
}

#[test]
fn merge_configs_merge_in_memory_configs() {
    const BASE: &str = r#"ocio_profile_version: 2.1

file_rules:
  - !<Rule> {name: Default, colorspace: A}

roles:
    a: colorspace_a

colorspaces:
- !<ColorSpace>
    name: colorspace_a
    family: utility
"#;

    const INPUT: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: B}

colorspaces:
- !<ColorSpace>
    name: B
    family: aces
"#;

    const RESULT: &str = r#"ocio_profile_version: 2.1

roles:
  a: colorspace_a

file_rules:
  - !<Rule> {name: Default, colorspace: B}

colorspaces:
  - !<ColorSpace>
    name: colorspace_a
    family: Base/utility
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    allocation: uniform

  - !<ColorSpace>
    name: B
    family: Input/aces
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    allocation: uniform"#;

    let base = from_str(BASE);
    let input = from_str(INPUT);

    let mut params = ConfigMergingParameters::new();
    params.set_input_first(false);
    let strategy = MergeStrategy::PreferInput;
    params.set_roles(strategy);
    params.set_colorspaces(strategy);
    params.set_named_transforms(strategy);
    params.set_default_strategy(strategy);
    params.set_input_family_prefix("Input/");
    params.set_base_family_prefix("Base/");
    params.set_adjust_input_reference_space(false);
    params.set_avoid_duplicates(false);

    let mut merged = None;
    check_for_log_or_exception(
        LogType::Warning,
        || {
            merged = Some(merge_configs(&params, &base, &input)?);
            Ok(())
        },
        &["The Input config contains a value that would override the Base config: file_rules: Default"],
    );
    let merged = merged.unwrap();

    let result = from_str(RESULT);
    assert_eq!(merged.serialize().unwrap(), result.serialize().unwrap());
}

#[test]
fn merge_configs_merge_single_colorspace() {
    const BASE: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: A}

roles:
    a: colorspace_a

colorspaces:
- !<ColorSpace>
    name: colorspace_a
    family: utility
"#;

    const INPUT: &str = r#"ocio_profile_version: 2.1

file_rules:
  - !<Rule> {name: Default, colorspace: B}

colorspaces:
- !<ColorSpace>
    name: B
    family: aces
"#;

    const RESULT: &str = r#"ocio_profile_version: 2

environment:
  {}
search_path: ""
strictparsing: true
luma: [0.2126, 0.7152, 0.0722]

roles:
  a: colorspace_a

file_rules:
  - !<Rule> {name: Default, colorspace: A}

displays:
  {}

active_displays: []
active_views: []

colorspaces:
  - !<ColorSpace>
    name: colorspace_a
    family: Base/utility
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    allocation: uniform

  - !<ColorSpace>
    name: B
    family: Input/aces
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    allocation: uniform"#;

    let base = from_str(BASE);
    let input = from_str(INPUT);
    let colorspace = input.get_color_space("B").unwrap();

    let mut params = ConfigMergingParameters::new();
    params.set_input_first(false);
    let strategy = MergeStrategy::PreferInput;
    params.set_roles(strategy);
    params.set_colorspaces(strategy);
    params.set_named_transforms(strategy);
    params.set_default_strategy(strategy);
    params.set_input_family_prefix("Input/");
    params.set_base_family_prefix("Base/");
    params.set_adjust_input_reference_space(false);
    params.set_avoid_duplicates(false);

    let merged = merge_color_space(&params, &base, colorspace).unwrap();

    let result = from_str(RESULT);
    assert_eq!(merged.serialize().unwrap(), result.serialize().unwrap());
}
