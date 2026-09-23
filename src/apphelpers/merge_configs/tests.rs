//! Port of `MergeConfigsHelpers_tests.cpp` (part 1: OCIOM parsing, overrides,
//! general, roles, file rules, displays / views, view transforms and looks
//! sections, merges driven by OCIOM files).

use super::section_merger::*;
use super::*;
use crate::config::logging::LogGuard;
use crate::types::*;

pub(super) type MergeStrategy = MergeStrategies;

pub(super) fn test_files_dir() -> String {
    concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/files").to_string()
}

pub(super) fn merge_file(name: &str) -> String {
    format!("{}/configs/mergeconfigs/{}", test_files_dir(), name)
}

pub(super) fn get_config(name: &str) -> Config {
    Config::create_from_file(&merge_file(name)).unwrap()
}

pub(super) fn get_base_config() -> Config {
    get_config("base_config.yaml")
}

pub(super) fn get_input_config() -> Config {
    get_config("input_config.yaml")
}

pub(super) fn from_str(s: &str) -> Config {
    Config::create_from_str(s).unwrap()
}

#[track_caller]
pub(super) fn compare_environment_var(merged: &Config, names: &[&str], values: &[&str]) {
    for i in 0..merged.num_environment_vars() {
        let name = merged.environment_var_name_by_index(i);
        assert_eq!(name, names[i]);
        assert_eq!(merged.environment_var_default(name), values[i]);
    }
}

#[track_caller]
pub(super) fn check_color_space<'c>(
    merged: &'c Config,
    ref_name: &str,
    index: usize,
    ref_type: SearchReferenceSpaceType,
) -> &'c ColorSpace {
    let name =
        merged.color_space_name_by_index_filtered(ref_type, ColorSpaceVisibility::All, index);
    assert_eq!(name, ref_name);
    merged
        .get_color_space(ref_name)
        .unwrap_or_else(|| panic!("color space '{ref_name}' not found"))
}

#[track_caller]
pub(super) fn check_named_transform<'c>(
    merged: &'c Config,
    ref_name: &str,
    index: usize,
) -> &'c crate::config::NamedTransform {
    let name = merged.named_transform_name_by_index_filtered(NamedTransformVisibility::All, index);
    assert_eq!(name, ref_name);
    merged
        .get_named_transform(ref_name)
        .unwrap_or_else(|| panic!("named transform '{ref_name}' not found"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LogType {
    Warning,
    Error,
}

/// Run `setup` while capturing the log (`checkForLogOrException`): either
/// `setup` fails with the first message, or all the messages are logged
/// (as warnings or errors) and nothing else is logged.
#[track_caller]
pub(super) fn check_for_log_or_exception<F>(ty: LogType, setup: F, msgs: &[&str])
where
    F: FnOnce() -> Result<()>,
{
    // Use INFO rather than DEBUG for the guard to avoid a lot of optimizer output.
    let guard = LogGuard::with_level(LoggingLevel::Info);
    match setup() {
        Ok(()) => {
            for s in msgs {
                let prefix = match ty {
                    LogType::Error => r"\[OpenColorIO Error\]: ",
                    LogType::Warning => r"\[OpenColorIO Warning\]: ",
                };
                let pattern = format!(r"{}{}[\.\r\n]+", prefix, regex::escape(s));
                let found = guard.find_all_and_remove(&pattern);
                assert!(
                    found,
                    "This message was not found: {s}\nLog:\n{}",
                    guard.output()
                );
            }
            assert!(
                guard.is_empty(),
                "The following unexpected messages were encountered:\n{}",
                guard.output()
            );
        }
        Err(e) => {
            // Only checking the first string because only the first error gets out.
            assert_eq!(e.message(), msgs[0]);
        }
    }
}

/// Run a section merger on a copy of the base config, checking the log.
#[track_caller]
pub(super) fn check_merge<F>(
    ty: LogType,
    base: &Config,
    input: &Config,
    params: &ConfigMergingParameters,
    f: F,
    msgs: &[&str],
) -> Config
where
    F: FnOnce(&mut MergeHandlerOptions) -> Result<()>,
{
    let mut merged = base.create_editable_copy();
    {
        let mut options = MergeHandlerOptions {
            base_config: base,
            input_config: input,
            params,
            merged_config: &mut merged,
        };
        check_for_log_or_exception(ty, || f(&mut options), msgs);
    }
    merged
}

/// Run a section merger on a copy of the base config (no log check).
#[track_caller]
pub(super) fn merge_section<F>(
    base: &Config,
    input: &Config,
    params: &ConfigMergingParameters,
    f: F,
) -> Config
where
    F: FnOnce(&mut MergeHandlerOptions) -> Result<()>,
{
    let mut merged = base.create_editable_copy();
    {
        let mut options = MergeHandlerOptions {
            base_config: base,
            input_config: input,
            params,
            merged_config: &mut merged,
        };
        f(&mut options).unwrap();
    }
    merged
}

/// Run a section merger on `merged`.
pub(super) fn run_on<F>(
    base: &Config,
    input: &Config,
    params: &ConfigMergingParameters,
    merged: &mut Config,
    f: F,
) -> Result<()>
where
    F: FnOnce(&mut MergeHandlerOptions) -> Result<()>,
{
    let mut options = MergeHandlerOptions {
        base_config: base,
        input_config: input,
        params,
        merged_config: merged,
    };
    f(&mut options)
}

/// Run a section merger on `merged`, checking the log.
#[track_caller]
pub(super) fn check_on<F>(
    ty: LogType,
    base: &Config,
    input: &Config,
    params: &ConfigMergingParameters,
    merged: &mut Config,
    f: F,
    msgs: &[&str],
) where
    F: FnOnce(&mut MergeHandlerOptions) -> Result<()>,
{
    check_for_log_or_exception(ty, || run_on(base, input, params, merged, f), msgs);
}

/// Check that `r` is an error containing `what`.
#[track_caller]
pub(super) fn assert_err_msg<T: std::fmt::Debug>(r: Result<T>, what: &str) {
    match r {
        Ok(v) => panic!("expected an error containing {what:?}, got {v:?}"),
        Err(e) => assert!(
            e.message().contains(what),
            "error {:?} does not contain {:?}",
            e.message(),
            what
        ),
    }
}

pub(super) fn general(o: &mut MergeHandlerOptions) -> Result<()> {
    GeneralMerger::new(o).merge()
}
pub(super) fn roles(o: &mut MergeHandlerOptions) -> Result<()> {
    RolesMerger::new(o).merge()
}
pub(super) fn file_rules(o: &mut MergeHandlerOptions) -> Result<()> {
    FileRulesMerger::new(o).merge()
}
pub(super) fn display_views(o: &mut MergeHandlerOptions) -> Result<()> {
    DisplayViewMerger::new(o).merge()
}
pub(super) fn view_transforms(o: &mut MergeHandlerOptions) -> Result<()> {
    ViewTransformsMerger::new(o)?.merge()
}
pub(super) fn looks(o: &mut MergeHandlerOptions) -> Result<()> {
    LooksMerger::new(o).merge()
}
pub(super) fn colorspaces(o: &mut MergeHandlerOptions) -> Result<()> {
    ColorspacesMerger::new(o)?.merge()
}
pub(super) fn named_transforms(o: &mut MergeHandlerOptions) -> Result<()> {
    NamedTransformsMerger::new(o).merge()
}

/// Check that `t` is a group transform containing transforms of the given
/// types.
#[track_caller]
pub(super) fn check_group(t: Option<&crate::Transform>, types: &[TransformType]) {
    match t {
        Some(crate::Transform::Group(g)) => {
            let actual: Vec<TransformType> =
                g.transforms.iter().map(|t| t.transform_type()).collect();
            assert_eq!(actual, types);
        }
        other => panic!("expected a group transform, got {other:?}"),
    }
}

#[track_caller]
pub(super) fn assert_close(a: f64, b: f64, tol: f64) {
    assert!((a - b).abs() <= tol, "{a} != {b} (tolerance {tol})");
}

#[test]
fn merge_configs_ociom_parser() {
    // Ensure version is initialized correctly.
    let merger = ConfigMerger::new();
    assert_eq!(merger.major_version(), 1);
    assert_eq!(merger.minor_version(), 0);

    // Test parsing an OCIOM file.
    let merger = ConfigMerger::create_from_file(&merge_file("parser_test.ociom")).unwrap();
    assert_eq!(merger.major_version(), 1);
    assert_eq!(merger.minor_version(), 0);

    // Check the search path for finding the base and input configs.
    assert_eq!(merger.num_search_paths(), 2);
    assert_eq!(merger.search_path(0), "/usr/local/configs");
    assert_eq!(merger.search_path(1), ".");

    // The parser_test.ociom contains only one merge.
    assert_eq!(merger.num_config_merging_parameters(), 1);
    let p = merger.params(0).unwrap();

    // Test that the all the options are loaded correctly.
    assert_eq!(p.base_config_name(), "base0.ocio");
    assert_eq!(p.input_config_name(), "input0.ocio");
    assert_eq!(p.output_name(), "Merge1");

    assert_eq!(p.input_family_prefix(), "abc");
    assert_eq!(p.base_family_prefix(), "def");
    assert!(p.is_input_first());
    assert!(!p.is_error_on_conflict());

    assert_eq!(p.default_strategy(), MergeStrategy::InputOnly);
    assert!(p.is_avoid_duplicates());
    assert!(p.is_adjust_input_reference_space());

    assert_eq!(p.name(), "my merge");
    assert_eq!(p.description(), "my desc");
    assert_eq!(p.search_path(), "abc");

    // Expecting two environment variables.
    assert_eq!(p.num_environment_vars(), 2);
    assert_eq!(p.environment_var(0), "test");
    assert_eq!(p.environment_var_value(0), "valueOther");
    assert_eq!(p.environment_var(1), "test1");
    assert_eq!(p.environment_var_value(1), "value123");

    assert_eq!(p.active_displays(), "D1, D2");
    assert_eq!(p.active_views(), "V1, V2");
    assert_eq!(p.inactive_color_spaces(), "I1, I2");

    assert_eq!(p.roles(), MergeStrategy::PreferInput);
    assert_eq!(p.file_rules(), MergeStrategy::PreferBase);
    assert_eq!(p.display_views(), MergeStrategy::InputOnly);
    assert_eq!(p.looks(), MergeStrategy::BaseOnly);
    assert_eq!(p.colorspaces(), MergeStrategy::Remove);
    assert_eq!(p.named_transforms(), MergeStrategy::PreferBase);
}

#[test]
fn merge_configs_params_serialization() {
    let merger = ConfigMerger::create_from_file(&merge_file("parser_test.ociom")).unwrap();
    let p = merger.params(0).unwrap();

    const REF: &str = "<base: base0.ocio, input: input0.ocio, output_name: Merge1, input_family_prefix: abc, \
        base_family_prefix: def, input_first: true, error_on_conflict: false, default_strategy: InputOnly, \
        avoid_duplicates: true, adjust_input_reference_space: true, name: my merge, description: my desc, \
        search_path: abc, active_displays: D1, D2, active_views: V1, V2, inactive_colorspaces: I1, I2, \
        roles: PreferInput, file_rules: PreferBase, display-views: InputOnly, view_transforms: PreferBase, \
        looks: BaseOnly, colorspaces: Remove, named_transforms: PreferBase, \
        environment: [test=valueOther, test1=value123]>";

    assert_eq!(p.to_string(), REF);
}

#[test]
fn merge_configs_ociom_serialization() {
    let merger = ConfigMerger::create_from_file(&merge_file("parser_test.ociom")).unwrap();

    const REF: &str = r#"ociom_version: 1.0
search_path:
  - /usr/local/configs
  - .
merge:
  Merge1:
    base: base0.ocio
    input: input0.ocio
    options:
      input_family_prefix: abc
      base_family_prefix: def
      input_first: true
      error_on_conflict: false
      default_strategy: InputOnly
      avoid_duplicates: true
      adjust_input_reference_space: true
    overrides:
      name: my merge
      description: my desc
      search_path: abc
      environment:
        test: valueOther
        test1: value123
      active_displays: [D1, D2]
      active_views: [V1, V2]
      inactive_colorspaces: [I1, I2]
    params:
      roles:
        strategy: PreferInput
      file_rules:
        strategy: PreferBase
      display-views:
        strategy: InputOnly
      view_transforms:
        strategy: PreferBase
      looks:
        strategy: BaseOnly
      colorspaces:
        strategy: Remove
      named_transforms:
        strategy: PreferBase"#;

    assert_eq!(merger.serialize().unwrap(), REF);
    assert_eq!(merger.to_string(), REF);
}

#[test]
fn merge_configs_ociom_parser_no_overrides() {
    let merger =
        ConfigMerger::create_from_file(&merge_file("parser_test_no_overrides.ociom")).unwrap();

    // The parser_test.ociom contains only one merge.
    let p = merger.params(0).unwrap();

    assert_eq!(p.base_config_name(), "input0.ocio");
    assert_eq!(p.input_config_name(), "input2.ocio");

    assert_eq!(p.input_family_prefix(), "abc");
    assert_eq!(p.base_family_prefix(), "def");
    assert!(p.is_input_first());
    assert!(!p.is_error_on_conflict());
    assert_eq!(p.default_strategy(), MergeStrategy::InputOnly);
    assert!(p.is_avoid_duplicates());
    assert!(p.is_adjust_input_reference_space());

    assert_eq!(p.name(), "");
    assert_eq!(p.description(), "");
    assert_eq!(p.search_path(), "");

    // Expecting 0 environment variables.
    assert_eq!(p.num_environment_vars(), 0);

    assert_eq!(p.active_displays(), "");
    assert_eq!(p.active_views(), "");
    assert_eq!(p.inactive_color_spaces(), "");

    assert_eq!(p.roles(), MergeStrategy::PreferInput);
    assert_eq!(p.file_rules(), MergeStrategy::PreferBase);
    assert_eq!(p.display_views(), MergeStrategy::InputOnly);
    assert_eq!(p.view_transforms(), MergeStrategy::PreferBase);
    assert_eq!(p.looks(), MergeStrategy::BaseOnly);
    assert_eq!(p.colorspaces(), MergeStrategy::Remove);
    assert_eq!(p.named_transforms(), MergeStrategy::PreferBase);
}

#[test]
fn merge_configs_overrides() {
    let base = get_base_config();
    let input = get_input_config();

    // Test that the overrides options are taken into account in the merging process.

    let setup = |strategy: MergeStrategy| -> ConfigMergingParameters {
        let mut p = ConfigMergingParameters::new();
        p.set_display_views(strategy);
        p.set_colorspaces(strategy);
        // Not looking for duplicates as this test does not test that.
        p.set_avoid_duplicates(false);

        // Set the overrides.
        p.set_name("OVR Name");
        p.set_description("OVR Desc");
        p.set_search_path("OVR1,OVR2");
        p.add_environment_var("OVR1", "VALUE1");
        p.add_environment_var("OVR2", "VALUE2");
        p.set_active_displays("OVR DISP 1,OVR DISP 2").unwrap();
        p.set_active_views("OVR VIEW 1,OVR VIEW 2").unwrap();
        p.set_inactive_color_spaces("view_1, ACES2065-1");
        p
    };

    let do_tests = |merged: &Config| {
        assert_eq!(merged.name(), "OVR Name");
        assert_eq!(merged.description(), "OVR Desc");
        assert_eq!(merged.search_path(), "OVR1,OVR2");
        assert_eq!(merged.num_environment_vars(), 2);
        compare_environment_var(merged, &["OVR1", "OVR2"], &["VALUE1", "VALUE2"]);
        assert_eq!(merged.active_displays(), "OVR DISP 1, OVR DISP 2");
        assert_eq!(merged.active_views(), "OVR VIEW 1, OVR VIEW 2");
        assert_eq!(merged.inactive_color_spaces(), "view_1, ACES2065-1");
    };

    let all = |o: &mut MergeHandlerOptions| -> Result<()> {
        // Merge name and description.
        general(o)?;
        // Merge active_display, active_views.
        display_views(o)?;
        // Merge inactive_colorspaces, environment and search_path.
        colorspaces(o)
    };

    // Test sections with strategy = PreferInput.
    {
        let params = setup(MergeStrategy::PreferInput);
        let merged = check_merge(
            LogType::Warning,
            &base,
            &input,
            &params,
            all,
            &[
                "The Input config contains a value that would override the Base config: shared_views: SHARED_1",
                "The Input config contains a value that would override the Base config: display: DISP_1, view: VIEW_1",
                "The Input config contains a value that would override the Base config: viewing_rules: RULE_1",
                "Color space 'ACES2065-1' will replace a color space in the base config.",
                "Color space 'view_1' will replace a color space in the base config.",
            ],
        );
        do_tests(&merged);
    }

    // Test sections with strategy = PreferBase.
    {
        let params = setup(MergeStrategy::PreferBase);
        let merged = check_merge(
            LogType::Warning,
            &base,
            &input,
            &params,
            all,
            &[
                "The Input config contains a value that would override the Base config: shared_views: SHARED_1",
                "The Input config contains a value that would override the Base config: display: DISP_1, view: VIEW_1",
                "The Input config contains a value that would override the Base config: viewing_rules: RULE_1",
                "Color space 'ACES2065-1' was not merged as it's already present in the base config.",
                "Color space 'view_1' was not merged as it's already present in the base config.",
            ],
        );
        do_tests(&merged);
    }

    // Test sections with strategy = InputOnly.
    {
        let params = setup(MergeStrategy::InputOnly);
        let merged = merge_section(&base, &input, &params, all);
        do_tests(&merged);
    }

    // Test sections with strategy = BaseOnly.
    {
        let params = setup(MergeStrategy::BaseOnly);
        let merged = merge_section(&base, &input, &params, all);
        do_tests(&merged);
    }

    // Strategy Remove is not tested as the overrides do not affect that strategy.
}

#[test]
fn merge_configs_general_section() {
    let mut base = get_base_config();
    let mut input = get_input_config();

    let setup = |strategy: MergeStrategy| -> ConfigMergingParameters {
        let mut p = ConfigMergingParameters::new();
        // The general strategy is determined by the default strategy.
        p.set_default_strategy(strategy);
        p
    };

    // Test that the default strategy is used as a fallback if the section strategy was not
    // defined.
    {
        let mut params = setup(MergeStrategy::Unspecified);
        // Simulate settings from OCIOM file.
        params.set_default_strategy(MergeStrategy::InputOnly);
        let merged = merge_section(&base, &input, &params, general);
        assert_eq!(merged.name(), "input0");
        assert_eq!(merged.description(), "My description 2");
        assert_eq!(merged.major_version(), 2);
        assert_eq!(merged.minor_version(), 1);
        assert_close(merged.default_luma_coefs()[1], 0.677998, 1e-4);
    }

    let cases = [
        (
            MergeStrategy::PreferInput,
            "input0",
            "My description 2",
            0.677998,
        ),
        (
            MergeStrategy::PreferBase,
            "base0",
            "My description 1",
            0.7152,
        ),
        (
            MergeStrategy::InputOnly,
            "input0",
            "My description 2",
            0.677998,
        ),
        (MergeStrategy::BaseOnly, "base0", "My description 1", 0.7152),
    ];
    for (strategy, name, desc, luma) in cases {
        let params = setup(strategy);
        let merged = merge_section(&base, &input, &params, general);
        assert_eq!(merged.name(), name);
        assert_eq!(merged.description(), desc);
        assert_eq!(merged.major_version(), 2);
        // Config version is always highest of both configs, regardless of strategy.
        assert_eq!(merged.minor_version(), 1);
        assert_close(merged.default_luma_coefs()[1], luma, 1e-4);
    }

    {
        const BASE: &str = r#"ocio_profile_version: 1

roles:
  default: colorspace_a
colorspaces:
- !<ColorSpace>
    name: colorspace_a
"#;

        const INPUT: &str = r#"ocio_profile_version: 2.1

luma: [0.262700, 0.677998, 0.059301]
name: input0
description: |
  My description 2

roles:
  default: colorspace_b
colorspaces:
- !<ColorSpace>
    name: colorspace_b
"#;

        base = from_str(BASE);
        input = from_str(INPUT);

        let cases = [
            (
                MergeStrategy::PreferInput,
                "input0",
                "My description 2",
                0.677998,
            ),
            (MergeStrategy::PreferBase, "", "", 0.7152),
            (
                MergeStrategy::InputOnly,
                "input0",
                "My description 2",
                0.677998,
            ),
            (MergeStrategy::BaseOnly, "", "", 0.7152),
        ];
        for (strategy, name, desc, luma) in cases {
            let params = setup(strategy);
            let merged = merge_section(&base, &input, &params, general);
            assert_eq!(merged.name(), name);
            assert_eq!(merged.description(), desc);
            assert_eq!(merged.major_version(), 2);
            assert_eq!(merged.minor_version(), 1);
            assert_close(merged.default_luma_coefs()[1], luma, 1e-4);
        }
    }
}

#[test]
fn merge_configs_roles_section() {
    // Allowed strategies: PreferInput, PreferBase, InputOnly, BaseOnly, Remove.
    // Allowed merge options: ErrorOnConflict.

    let base = get_base_config();
    let input = get_input_config();

    let setup = |strategy: MergeStrategy| -> ConfigMergingParameters {
        let mut p = ConfigMergingParameters::new();
        p.set_roles(strategy);
        p.set_default_strategy(strategy);
        p
    };

    // Test that the default strategy is used as a fallback if the section strategy was not
    // defined.
    {
        let mut params = setup(MergeStrategy::Unspecified);
        params.set_default_strategy(MergeStrategy::InputOnly);
        let merged = check_merge(
            LogType::Warning,
            &base,
            &input,
            &params,
            roles,
            &[
                "The Input config contains a role 'g22_ap1_tx' that would override an alias of Base config color space 'Gamma 2.2 AP1 - Texture'",
                "The Input config contains a role 'nt_base' that would override Base config named transform: 'nt_base'",
            ],
        );
        assert_eq!(merged.num_roles(), 3);
        assert_eq!(merged.role_color_space("aces_interchange"), "ACES2065-1");
        assert_eq!(
            merged.role_color_space("texture_paint"),
            "ACEScct - SomeOtherName"
        );
        assert_eq!(merged.role_color_space("matte_paint"), "sRGB - Texture");
    }

    // Test Roles section with strategy = PreferInput.
    {
        let params = setup(MergeStrategy::PreferInput);
        let merged = check_merge(
            LogType::Warning,
            &base,
            &input,
            &params,
            roles,
            &[
                "The Input config contains a role 'g22_ap1_tx' that would override an alias of Base config color space 'Gamma 2.2 AP1 - Texture'",
                "The Input config contains a role that would override Base config role 'texture_paint'.",
                "The Input config contains a role 'nt_base' that would override Base config named transform: 'nt_base'",
            ],
        );
        assert_eq!(merged.num_roles(), 4);
        // Following three roles were overwritten by input config.
        assert_eq!(merged.role_color_space("aces_interchange"), "ACES2065-1");
        assert_eq!(
            merged.role_color_space("texture_paint"),
            "ACEScct - SomeOtherName"
        );
        assert_eq!(merged.role_color_space("matte_paint"), "sRGB - Texture");
        // Following role come from base config.
        assert_eq!(merged.role_color_space("data"), "Raw");
    }

    // Test Roles section with strategy = PreferBase.
    {
        let params = setup(MergeStrategy::PreferBase);
        let merged = check_merge(
            LogType::Warning,
            &base,
            &input,
            &params,
            roles,
            &[
                "The Input config contains a role 'g22_ap1_tx' that would override an alias of Base config color space 'Gamma 2.2 AP1 - Texture'",
                "The Input config contains a role that would override Base config role 'texture_paint'.",
                "The Input config contains a role 'nt_base' that would override Base config named transform: 'nt_base'",
            ],
        );
        assert_eq!(merged.num_roles(), 4);
        assert_eq!(merged.role_color_space("aces_interchange"), "ACES2065-1");
        assert_eq!(merged.role_color_space("texture_paint"), "ACEScct");
        assert_eq!(merged.role_color_space("data"), "Raw");
        // Following role come from input config.
        assert_eq!(merged.role_color_space("matte_paint"), "sRGB - Texture");
    }

    // Test Roles section with strategy = InputOnly.
    {
        let params = setup(MergeStrategy::InputOnly);
        let merged = check_merge(
            LogType::Warning,
            &base,
            &input,
            &params,
            roles,
            &[
                "The Input config contains a role 'g22_ap1_tx' that would override an alias of Base config color space 'Gamma 2.2 AP1 - Texture'",
                "The Input config contains a role 'nt_base' that would override Base config named transform: 'nt_base'",
            ],
        );
        assert_eq!(merged.num_roles(), 3);
        assert_eq!(merged.role_color_space("aces_interchange"), "ACES2065-1");
        assert_eq!(
            merged.role_color_space("texture_paint"),
            "ACEScct - SomeOtherName"
        );
        assert_eq!(merged.role_color_space("matte_paint"), "sRGB - Texture");
    }

    // Test Roles section with strategy = BaseOnly.
    {
        let params = setup(MergeStrategy::BaseOnly);
        let merged = merge_section(&base, &input, &params, roles);
        assert_eq!(merged.num_roles(), 3);
        assert_eq!(merged.role_color_space("aces_interchange"), "ACES2065-1");
        assert_eq!(merged.role_color_space("texture_paint"), "ACEScct");
        assert_eq!(merged.role_color_space("data"), "Raw");
    }

    // Test Roles section with strategy = Remove.
    {
        let params = setup(MergeStrategy::Remove);
        let merged = merge_section(&base, &input, &params, roles);
        assert_eq!(merged.num_roles(), 1);
        // This is the only role in base that is not in input.
        assert_eq!(merged.role_color_space("data"), "Raw");
    }

    // Test Roles section with strategy = PreferInput and option ErrorOnConflict = true.
    {
        let mut params = setup(MergeStrategy::PreferInput);
        params.set_error_on_conflict(true);
        check_merge(
            LogType::Error,
            &base,
            &input,
            &params,
            roles,
            &["The Input config contains a role 'g22_ap1_tx' that would override an alias of Base config color space 'Gamma 2.2 AP1 - Texture'."],
        );
    }
}
