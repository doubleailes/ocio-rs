//! Port of `ColorSpaceHelpers_tests.cpp` (menu helper part) and
//! `DisplayViewHelpers_tests.cpp`.
//!
//! These tests change environment variables (`OCIO_USER_CATEGORIES`,
//! `OCIO_ACTIVE_DISPLAYS`, `OCIO_ACTIVE_VIEWS`) read by the config and the menu
//! helper, so they are serialized.

use ocio::apphelpers::display_view_helpers;
use ocio::apphelpers::{ColorSpaceMenuHelper, ColorSpaceMenuParameters};
use ocio::config::logging::LogGuard;
use ocio::*;
use std::sync::{Arc, Mutex, MutexGuard};

const CATEGORY_TEST_CONFIG: &str = include_str!("data/files/apphelpers/category_test_config.ocio");

fn lock() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Set (or unset) an environment variable while the guard is alive.
struct EnvGuard {
    name: String,
    old: Option<String>,
}

impl EnvGuard {
    fn set(name: &str, value: &str) -> Self {
        let old = std::env::var(name).ok();
        std::env::set_var(name, value);
        Self {
            name: name.to_string(),
            old,
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.old {
            Some(v) => std::env::set_var(&self.name, v),
            None => std::env::remove_var(&self.name),
        }
    }
}

fn data_file(rel: &str) -> String {
    format!("{}/tests/data/files/{}", env!("CARGO_MANIFEST_DIR"), rel)
}

fn category_config() -> Arc<Config> {
    let config = Config::create_from_str(CATEGORY_TEST_CONFIG).unwrap();
    config.validate().unwrap();
    Arc::new(config)
}

fn menu(params: &ColorSpaceMenuParameters) -> Arc<ColorSpaceMenuHelper> {
    ColorSpaceMenuHelper::create(params).unwrap()
}

fn names(m: &ColorSpaceMenuHelper) -> Vec<String> {
    (0..m.num_color_spaces())
        .map(|i| m.name(i).to_string())
        .collect()
}

#[test]
fn color_space_menu_helper_no_color_spaces() {
    let _l = lock();
    let config = Config::create_from_str(
        r#"ocio_profile_version: 2

environment:
  {}

search_path: luts
strictparsing: true
family_separator: /
luma: [0.2126, 0.7152, 0.0722]

roles:
  rendering: test_1
  default: raw

view_transforms:
  - !<ViewTransform>
    name: view_transform
    from_scene_reference: !<MatrixTransform> {}

displays:
  DISP_1:
    - !<View> {name: VIEW_1, colorspace: test_1}
    - !<View> {name: VIEW_2, colorspace: test_2}

active_displays: []
active_views: []

colorspaces:
  - !<ColorSpace>
    name: raw
    family: Raw
    description: A raw color space. Conversions to and from this space are no-ops.
    isdata: true
    allocation: uniform

  - !<ColorSpace>
    name: test_1
    categories: [ working-space, basic-2d ]
    encoding: scene-linear

  - !<ColorSpace>
    name: test_2
    categories: [ working-space ]
    encoding: scene-linear
 "#,
    )
    .unwrap();
    config.validate().unwrap();

    // Use app-oriented categories with exact case.
    let mut params = ColorSpaceMenuParameters::new(Arc::new(config));
    assert_eq!(menu(&params).num_color_spaces(), 3);

    params.set_include_color_spaces(false);
    assert_eq!(menu(&params).num_color_spaces(), 0);

    params.set_include_named_transforms(true);
    assert_eq!(menu(&params).num_color_spaces(), 0);

    params.set_include_color_spaces(true);
    params.set_search_reference_space_type(SearchReferenceSpaceType::Display);
    assert_eq!(menu(&params).num_color_spaces(), 0);

    params.set_include_color_spaces(true);
    params.set_app_categories("basic-2d");
    params.set_search_reference_space_type(SearchReferenceSpaceType::Scene);
    params.set_treat_no_category_as_any(true);
    assert_eq!(menu(&params).num_color_spaces(), 2);

    params.set_treat_no_category_as_any(false);
    assert_eq!(menu(&params).num_color_spaces(), 1);

    params.set_include_color_spaces(false);
    assert_eq!(menu(&params).num_color_spaces(), 0);

    params.set_include_color_spaces(true);
    params.set_search_reference_space_type(SearchReferenceSpaceType::Display);
    assert_eq!(menu(&params).num_color_spaces(), 0);
}

#[test]
fn color_space_menu_helper_categories() {
    let _l = lock();
    let config = category_config();

    // Use app-oriented categories with exact case.
    let mut params = ColorSpaceMenuParameters::new(config.clone());
    params.set_app_categories("file-io");
    // Note: The test config has two active color spaces and a NT without categories.
    params.set_treat_no_category_as_any(true);
    assert_eq!(menu(&params).num_color_spaces(), 6);

    // Use app-oriented categories with other case.
    params.set_app_categories("FILE-IO");
    assert_eq!(menu(&params).num_color_spaces(), 6);

    // Use app-oriented categories, including named transforms.
    params.set_app_categories("file-io");
    params.set_include_named_transforms(true);
    let m = menu(&params);
    assert_eq!(m.num_color_spaces(), 8);
    let expected = [
        "raw",
        "in_1",
        "in_2",
        "in_3",
        "view_1",
        "lut_input_3",
        "nt2",
        "nt3",
    ];
    assert_eq!(names(&m), expected);
    assert_eq!(m.name(8), "");
    for (i, n) in expected.iter().enumerate() {
        assert_eq!(m.ui_name(i), *n);
    }
    assert_eq!(m.ui_name(8), "");

    let levels = [1, 3, 0, 0, 0, 0, 0, 1, 0];
    for (i, n) in levels.iter().enumerate() {
        assert_eq!(m.num_hierarchy_levels(i), *n);
    }
    assert_eq!(m.hierarchy_level(1, 0), "Input");
    assert_eq!(m.hierarchy_level(1, 1), "Camera");
    assert_eq!(m.hierarchy_level(1, 2), "Acme");
    assert_eq!(m.hierarchy_level(7, 0), "NamedTransforms");

    // Use null categories.
    params.set_include_named_transforms(false);
    params.set_app_categories("");
    // All active color spaces (scene and display).
    assert_eq!(menu(&params).num_color_spaces(), 14);

    // Active non-display color spaces only.
    params.set_search_reference_space_type(SearchReferenceSpaceType::Scene);
    assert_eq!(menu(&params).num_color_spaces(), 11);

    // Active display color spaces only.
    params.set_search_reference_space_type(SearchReferenceSpaceType::Display);
    assert_eq!(menu(&params).num_color_spaces(), 3);

    // Use null categories, including named transforms.
    params.set_search_reference_space_type(SearchReferenceSpaceType::All);
    params.set_include_named_transforms(true);
    // All active color spaces and named transforms.
    assert_eq!(menu(&params).num_color_spaces(), 17);

    // Use app-oriented category, include roles.
    params.set_include_named_transforms(false);
    params.set_app_categories("look-process-space");
    params.set_include_roles(true);
    params.set_treat_no_category_as_any(false);
    let m = menu(&params);
    assert_eq!(m.num_color_spaces(), 7);
    let expected = [
        "lut_input_1",
        "lut_input_2",
        "lut_input_3",
        "default",
        "reference",
        "rendering",
        "scene_linear",
    ];
    assert_eq!(names(&m), expected);
    assert_eq!(m.name(7), "");
    for (i, n) in expected.iter().enumerate() {
        assert_eq!(m.index_from_name(n), Some(i));
    }
    assert_eq!(m.index_from_name("default (lin_1)"), None);

    let ui = [
        "lut_input_1",
        "lut_input_2",
        "lut_input_3",
        "default (raw)",
        "reference (lin_1)",
        "rendering (lin_1)",
        "scene_linear (lin_1)",
    ];
    for (i, n) in ui.iter().enumerate() {
        assert_eq!(m.ui_name(i), *n);
        assert_eq!(m.index_from_ui_name(n), Some(i));
    }
    assert_eq!(m.ui_name(7), "");
    assert_eq!(m.index_from_ui_name("default"), None);

    let levels = [0, 0, 0, 1, 1, 1, 1, 0];
    for (i, n) in levels.iter().enumerate() {
        assert_eq!(m.num_hierarchy_levels(i), *n);
    }
    for i in 3..7 {
        assert_eq!(m.hierarchy_level(i, 0), "Roles");
    }
    assert_eq!(m.hierarchy_level(7, 1), "");
    assert_eq!(m.hierarchy_level(6, 1), "");

    // Use an arbitrary (but existing) category only used by a named transform.
    {
        params.set_include_roles(false);
        params.set_include_named_transforms(true);
        params.set_app_categories("");
        params.set_user_categories("basic-3d");
        let m = menu(&params);
        assert_eq!(m.num_color_spaces(), 1);
        assert_eq!(m.ui_name(0), "nt1");

        // No color space is found, using all active color spaces and log a warning.
        let guard = LogGuard::new();
        params.set_include_named_transforms(false);
        let m = menu(&params);
        assert_eq!(
            guard.output(),
            "[OpenColorIO Info]: All parameters could not be used to create the menu: Found no \
             color space using user categories. Categories have been ignored since they matched \
             no color spaces.\n"
        );
        guard.clear();
        assert_eq!(m.num_color_spaces(), 14);
    }

    // Use a role.
    params.set_role(ROLE_RENDERING);
    params.set_app_categories("");
    params.set_include_roles(false);
    let m = menu(&params);
    assert_eq!(m.num_color_spaces(), 1);
    assert_eq!(m.name(0), "lin_1");
    assert_eq!(m.ui_name(0), "rendering (lin_1)");
    assert_eq!(m.family(0), "");

    // Use an existing role and app-oriented categories: categories are ignored.
    params.set_app_categories("file-io");
    let m = menu(&params);
    assert_eq!(m.num_color_spaces(), 1);
    assert_eq!(m.name(0), "lin_1");
    assert_eq!(m.ui_name(0), "rendering (lin_1)");
    assert_eq!(m.family(0), "");

    // Use an existing role and include roles: include roles is ignored.
    params.set_app_categories("");
    params.set_include_roles(true);
    let m = menu(&params);
    assert_eq!(m.num_color_spaces(), 1);
    assert_eq!(m.name(0), "lin_1");
    assert_eq!(m.ui_name(0), "rendering (lin_1)");
    assert_eq!(m.family(0), "");

    // Using an unknown category or role returns all the color spaces.
    {
        let guard = LogGuard::new();

        params.set_include_roles(false);
        params.set_role("");
        params.set_app_categories("unknown_category");

        // Return all the active color spaces.
        let m = menu(&params);
        assert_eq!(
            guard.output(),
            "[OpenColorIO Info]: All parameters could not be used to create the menu: Found no \
             color space using app categories. Found no color space using user categories. \
             Categories have been ignored since they matched no color spaces.\n"
        );
        guard.clear();
        assert_eq!(m.num_color_spaces(), 14);

        params.set_app_categories("");
        params.set_role("unknown_role");

        // Return all the active color spaces.
        let m = menu(&params);
        assert_eq!(
            guard.output(),
            "[OpenColorIO Info]: All parameters could not be used to create the menu: Found no \
             color space using user categories. Categories have been ignored since they matched \
             no color spaces.\n"
        );
        guard.clear();
        assert_eq!(m.num_color_spaces(), 14);
    }
}

#[test]
fn color_space_menu_helper_user_categories() {
    let _l = lock();
    let config = category_config();

    let mut params = ColorSpaceMenuParameters::new(config);
    // Note: The test config has two active color spaces and a NT without categories.
    params.set_treat_no_category_as_any(true);

    // User categories can be used instead of app-oriented categories.
    params.set_user_categories("basic-2d");
    assert_eq!(menu(&params).num_color_spaces(), 5);

    params.set_user_categories("advanced-2d");
    assert_eq!(menu(&params).num_color_spaces(), 6);

    params.set_user_categories("basic-2d, advanced-2d");
    params.set_include_named_transforms(true);
    assert_eq!(menu(&params).num_color_spaces(), 12);

    // Env. variable overrides parameter.
    {
        let _g = EnvGuard::set(OCIO_USER_CATEGORIES_ENVVAR, "basic-3d");
        assert_eq!(menu(&params).num_color_spaces(), 4);
    }

    //
    // Using both app-oriented categories and user categories.
    //

    // Intersection is used if non-empty.
    params.set_include_named_transforms(false);
    params.set_app_categories("file-io, working-space");
    params.set_user_categories("advanced-2d");
    let m = menu(&params);
    assert_eq!(m.num_color_spaces(), 5);
    assert_eq!(m.name(1), "in_2");

    // Intersection is used if non-empty, named transforms can be included.
    params.set_include_named_transforms(true);
    params.set_app_categories("working-space");
    params.set_user_categories("basic-3d");
    let m = menu(&params);
    assert_eq!(m.num_color_spaces(), 4);
    assert_eq!(m.name(2), "nt1");
    params.set_include_named_transforms(false);

    // Intersection is empty. App-oriented categories are used as the fall-back.
    let guard = LogGuard::new();
    params.set_app_categories("look-process-space");
    params.set_user_categories("advanced-3d");
    params.set_treat_no_category_as_any(false);
    let m = menu(&params);
    assert_eq!(
        guard.output(),
        "[OpenColorIO Info]: All parameters could not be used to create the menu: Intersection \
         of color spaces with app categories and color spaces with user categories is empty. User \
         categories have been ignored.\n"
    );
    guard.clear();
    assert_eq!(m.num_color_spaces(), 3);

    // Intersection leads to no results and there are no app-oriented category results. Fall
    // back to user categories.
    params.set_app_categories("not a category, not used");
    params.set_user_categories("basic-2d, not used");
    params.set_treat_no_category_as_any(false);
    let m = menu(&params);
    assert_eq!(
        guard.output(),
        "[OpenColorIO Info]: All parameters could not be used to create the menu: Found no \
         color space using app categories.\n"
    );
    guard.clear();
    assert_eq!(m.num_color_spaces(), 3);
}

#[test]
fn color_space_menu_helper_encodings() {
    let _l = lock();
    let config = category_config();

    let mut params = ColorSpaceMenuParameters::new(config);
    params.set_app_categories("file-io");
    params.set_encodings("sdr-video");
    // Note: The test config has two active color spaces without categories but they don't have
    // encodings and so are not included. However, there is a named transform with no category
    // but an encoding.
    params.set_treat_no_category_as_any(true);
    let m = menu(&params);
    assert_eq!(names(&m), ["in_1", "in_2", "view_1"]);

    params.set_include_named_transforms(true);
    let m = menu(&params);
    assert_eq!(m.num_color_spaces(), 4);
    assert_eq!(m.name(3), "nt3");
    params.set_include_named_transforms(false);

    let guard = LogGuard::new();
    params.set_encodings("not found encoding");
    let m = menu(&params);
    assert_eq!(
        guard.output(),
        "[OpenColorIO Info]: All parameters could not be used to create the menu: Encodings \
         have been ignored since they matched no color spaces.\n"
    );
    guard.clear();
    // The encoding doesn't match, so it's only the 4 with "file-io" and the two without
    // categories.
    assert_eq!(m.num_color_spaces(), 6);

    // If intersection is empty, encodings are ignored.
    params.set_include_named_transforms(true);
    params.set_treat_no_category_as_any(false);
    params.set_app_categories("working-space");
    params.set_user_categories("basic-3d");
    params.set_encodings("log");
    let m = menu(&params);
    assert_eq!(
        guard.output(),
        "[OpenColorIO Info]: All parameters could not be used to create the menu: Encodings have \
         been ignored since they matched no color spaces.\n"
    );
    guard.clear();
    assert_eq!(names(&m), ["nt1"]);

    // If intersection is empty (with and without encoding), user categories are ignored and
    // encodings are used.
    params.set_include_named_transforms(true);
    params.set_treat_no_category_as_any(false);
    params.set_app_categories("file-io");
    params.set_user_categories("basic-3d");
    params.set_encodings("sdr-video");
    let m = menu(&params);
    assert_eq!(
        guard.output(),
        "[OpenColorIO Info]: All parameters could not be used to create the menu: Intersection \
         of color spaces with app categories and color spaces with user categories is empty. \
         User categories have been ignored.\n"
    );
    guard.clear();
    assert_eq!(names(&m), ["in_1", "in_2", "nt3"]);

    // Categories give no color space, all categories are ignored but encodings are used.
    params.set_include_named_transforms(true);
    params.set_treat_no_category_as_any(false);
    params.set_app_categories("no");
    params.set_user_categories("no");
    params.set_encodings("sdr-video");
    let m = menu(&params);
    assert_eq!(
        guard.output(),
        "[OpenColorIO Info]: All parameters could not be used to create the menu: Found no color \
         space using app categories. Found no color space using user categories. Categories have \
         been ignored since they matched no color spaces.\n"
    );
    guard.clear();
    assert_eq!(
        names(&m),
        ["in_1", "in_2", "view_1", "display_lin_2", "nt1", "nt3"]
    );

    // App-oriented categories is empty, but encodings are used. Intersection with user
    // categories.
    params.set_treat_no_category_as_any(false);
    params.set_app_categories("");
    params.set_encodings("sdr-video");
    params.set_user_categories("advanced-2d");
    let m = menu(&params);
    assert_eq!(guard.output(), "");
    guard.clear();
    assert_eq!(m.num_color_spaces(), 2);
}

#[test]
fn color_space_menu_helper_usability_issues() {
    let _l = lock();

    // Adding a color space without categories to a config (and app) that uses them results in
    // that color space not showing up. This is fixed by treating color spaces without
    // categories as having all categories.
    {
        let config = Config::create_from_str(
            r#"ocio_profile_version: 2
roles:
  default: raw

displays:
  DISP_1:
    - !<View> {name: VIEW_1, colorspace: test_1}

colorspaces:
  - !<ColorSpace>
    name: raw
    categories: [ file-io ]

  - !<ColorSpace>
    name: test_1
    categories: [ file-io, working-space ]

  - !<ColorSpace>
    name: test_2
    categories: [ file-io ]

    # A color space is then added with no categories present.

  - !<ColorSpace>
    name: test_3
 "#,
        )
        .unwrap();
        config.validate().unwrap();

        let mut params = ColorSpaceMenuParameters::new(Arc::new(config));
        params.set_app_categories("file-io");
        params.set_treat_no_category_as_any(false);

        // The last color space doesn't show up, which may cause end-user confusion.
        let m = menu(&params);
        assert_eq!(names(&m), ["raw", "test_1", "test_2"]);

        // Treating color spaces with no categories as having all categories solves the issue.
        params.set_treat_no_category_as_any(true);
        let m = menu(&params);
        assert_eq!(m.num_color_spaces(), 4);
        assert_eq!(m.name(3), "test_3");
    }

    // Adding a color space with categories to a config that does not use them results in the
    // original color spaces disappearing (for apps that use categories). This is fixed by
    // treating color spaces without categories as having all categories.
    {
        let mut config = Config::create_from_str(
            r#"ocio_profile_version: 2
roles:
  default: raw

displays:
  DISP_1:
    - !<View> {name: VIEW_1, colorspace: test_1}

colorspaces:
  - !<ColorSpace>
    name: raw

  - !<ColorSpace>
    name: test_1

  - !<ColorSpace>
    name: test_2

    # A color space is then added that uses categories.

  - !<ColorSpace>
    name: test_3
    categories: [ file-io ]
 "#,
        )
        .unwrap()
        .create_editable_copy();
        config.validate().unwrap();

        // Make test_3 inactive to simulate it not being present.
        config.set_inactive_color_spaces("test_3");

        let mut params = ColorSpaceMenuParameters::new(Arc::new(config.clone()));
        params.set_app_categories("file-io");
        params.set_treat_no_category_as_any(false);

        {
            let guard = LogGuard::new();
            let m = menu(&params);
            assert_eq!(
                guard.output(),
                "[OpenColorIO Info]: All parameters could not be used to create the menu: Found \
                 no color space using app categories. Categories have been ignored since they \
                 matched no color spaces.\n"
            );
            // All three expected color spaces show up, as desired.
            assert_eq!(names(&m), ["raw", "test_1", "test_2"]);
        }

        // Add test_3, which contains a category.
        config.set_inactive_color_spaces("");
        params.set_config(Arc::new(config));
        // The original three color spaces disappear, which may cause end-user confusion.
        assert_eq!(menu(&params).num_color_spaces(), 1);

        // Treating color spaces with no categories as having all categories solves the issue.
        params.set_treat_no_category_as_any(true);
        let m = menu(&params);
        assert_eq!(m.num_color_spaces(), 4);
        assert_eq!(m.name(3), "test_3");
    }
}

#[test]
fn color_space_menu_helper_no_category() {
    let _l = lock();
    let config = Config::create_from_str(
        r#"ocio_profile_version: 1

environment:
  {}

search_path: luts
strictparsing: true

roles:
  rendering: test_1
  default: raw

displays:
  DISP_1:
    - !<View> {name: VIEW_1, colorspace: test_1}
    - !<View> {name: VIEW_2, colorspace: test_2}

active_displays: []
active_views: []

colorspaces:
  - !<ColorSpace>
    name: raw
    family: Raw
    description: A raw color space. Conversions to and from this space are no-ops.
    isdata: true
    allocation: uniform

  - !<ColorSpace>
    name: test_1

  - !<ColorSpace>
    name: test_2
 "#,
    )
    .unwrap();
    config.validate().unwrap();

    let mut params = ColorSpaceMenuParameters::new(Arc::new(config));

    // Categories are ignored when config is version 1 and no message is logged.
    let guard = LogGuard::new();
    params.set_app_categories("file-io");
    // Return all the color spaces.
    let m = menu(&params);
    assert_eq!(guard.output(), "");
    assert_eq!(m.num_color_spaces(), 3);
}

#[test]
fn color_space_menu_helper_input_color_transformation() {
    let _l = lock();
    let config = category_config();

    // Step 1 - Validate the selected input color spaces.
    let mut params = ColorSpaceMenuParameters::new(config.clone());
    params.set_app_categories("file-io");
    params.set_treat_no_category_as_any(false);
    let input_menu = menu(&params);
    assert_eq!(names(&input_menu), ["in_1", "in_2", "in_3", "lut_input_3"]);

    // Some extra validation.
    assert_eq!(input_menu.num_hierarchy_levels(0), 3);
    assert_eq!(input_menu.hierarchy_level(0, 0), "Input");
    assert_eq!(input_menu.hierarchy_level(0, 1), "Camera");
    assert_eq!(input_menu.hierarchy_level(0, 2), "Acme");
    assert_eq!(
        input_menu.description(0),
        "An input color space.\nFor the Acme camera."
    );
    assert_eq!(input_menu.num_hierarchy_levels(1), 0);
    assert_eq!(input_menu.description(1), "");

    // Step 2 - Validate the selected working color spaces.
    params.set_app_categories("working-space");
    let working_menu = menu(&params);
    assert_eq!(
        names(&working_menu),
        [
            "lin_1",
            "lin_2",
            "log_1",
            "in_3",
            "display_lin_1",
            "display_lin_2",
            "display_log_1"
        ]
    );

    // Step 3 - Validate the color transformation from in_1 to lin_2.
    let processor = config
        .get_processor(input_menu.name(0), working_menu.name(1))
        .unwrap();
    let group = processor.create_group_transform();
    Transform::Group(group.clone()).validate().unwrap();
    assert_eq!(group.num_transforms(), 1);
    match &group.transforms[0] {
        Transform::Exponent(exp) => {
            assert_eq!(exp.direction, TransformDirection::Forward);
            assert_eq!(exp.value, [2.6, 2.6, 2.6, 1.0]);
        }
        other => panic!("expected an exponent transform, got {other:?}"),
    }
}

#[test]
fn color_space_menu_helper_additional_color_space() {
    let _l = lock();
    // The unit test validates that a custom color transformation (i.e. an inactive one or a
    // newly created one not in the config instance) are correctly handled.
    let config = category_config();

    // Use an arbitrary menu helper.
    let mut params = ColorSpaceMenuParameters::new(config.clone());
    params.set_app_categories("file-io");
    params.set_treat_no_category_as_any(false);
    assert_eq!(
        names(&menu(&params)),
        ["in_1", "in_2", "in_3", "lut_input_3"]
    );

    // Add an additional color space to the menu. Note that it could be an inactive color space
    // or an active color space not having one of the selected categories.
    params.add_color_space("lin_1");
    assert_eq!(
        names(&menu(&params)),
        ["in_1", "in_2", "in_3", "lut_input_3", "lin_1"]
    );

    // Add an additional color space that is already there: nothing gets added.
    params.add_color_space("in_2");
    assert_eq!(menu(&params).num_color_spaces(), 5);

    // Delete a color space and recreate the menu helper.
    let mut cfg = config.create_editable_copy();
    cfg.remove_color_space("in_1");
    params.set_config(Arc::new(cfg.clone()));
    // And the additional color space is still present.
    assert_eq!(
        names(&menu(&params)),
        ["in_2", "in_3", "lut_input_3", "lin_1"]
    );

    // Additional color space are case insensitive.
    params.clear_added_color_spaces();
    assert_eq!(params.num_added_color_spaces(), 0);
    params.add_color_space("LIN_1");
    // Still get 4 items.
    assert_eq!(menu(&params).num_color_spaces(), 4);

    // Same color space can't be added twice.
    params.add_color_space("lin_1");
    assert_eq!(params.num_added_color_spaces(), 1);
    params.add_color_space("LIN_1");
    assert_eq!(params.num_added_color_spaces(), 1);
    params.clear_added_color_spaces();

    // Add a named transform.
    params.add_color_space("lin_1");
    params.add_color_space("nt1");
    assert_eq!(params.num_added_color_spaces(), 2);
    let m = menu(&params);
    assert_eq!(m.num_color_spaces(), 5);
    assert_eq!(m.name(4), "nt1");

    // Add a role (first that refers to color space already there or not).
    params.add_color_space(ROLE_RENDERING);
    assert_eq!(params.num_added_color_spaces(), 3);
    // Color space is already there: nothing is added.
    assert_eq!(menu(&params).num_color_spaces(), 5);

    params.add_color_space("default");
    let m = menu(&params);
    assert_eq!(m.num_color_spaces(), 6);
    assert_eq!(m.name(5), "raw");

    // Add inactive color space.
    params.clear_added_color_spaces();
    params.set_app_categories("file-io");
    assert_eq!(names(&menu(&params)), ["in_2", "in_3", "lut_input_3"]);
    cfg.set_inactive_color_spaces("in_3");
    params.set_config(Arc::new(cfg.clone()));
    assert_eq!(names(&menu(&params)), ["in_2", "lut_input_3"]);
    params.add_color_space("in_3");
    assert_eq!(names(&menu(&params)), ["in_2", "lut_input_3", "in_3"]);

    // Add a color space that does not exist.
    params.add_color_space("doesNotExist");
    let err = ColorSpaceMenuHelper::create(&params).unwrap_err();
    assert!(err
        .message()
        .contains("Element 'doesNotExist' is neither a color space not a named transform"));
}

#[test]
fn color_space_menu_parameters_and_helper_serialization() {
    let _l = lock();
    let config = category_config();
    let mut params = ColorSpaceMenuParameters::new(config.clone());
    params.set_app_categories("file-io");
    params.set_role("unknown");
    params.set_search_reference_space_type(SearchReferenceSpaceType::Scene);
    params.add_color_space("lin_1");
    params.add_color_space("nt1");
    params.set_treat_no_category_as_any(false);
    let expected = format!(
        "config: {}, role: unknown, appCategories: file-io, includeColorSpaces: true, \
         includeRoles: false, includeNamedTransforms: false, treatNoCategoryAsAny: false, \
         colorSpaceType: scene, addedSpaces: [lin_1, nt1]",
        config.cache_id()
    );
    assert_eq!(params.to_string(), expected);

    let m = menu(&params);
    assert_eq!(
        m.to_string(),
        format!("{expected}, color spaces = [in_1, in_2, in_3, lut_input_3, lin_1, nt1]")
    );
}

#[test]
fn color_space_helpers_add_color_space() {
    let _l = lock();
    let mut config = category_config().create_editable_copy();
    let file_path = data_file("lut1d_green.ctf");

    ocio::apphelpers::add_color_space(
        &mut config,
        "new_cs",
        &file_path,
        "file-io, cat_unknown",
        "lut_input_1",
    )
    .unwrap();

    let cs = config.get_color_space("new_cs").unwrap();
    // The categories are used by the config, so they are added.
    assert!(cs.has_category("file-io"));
    assert!(cs.has_category("cat_unknown"));
    match cs.transform(ColorSpaceDirection::ToReference) {
        Some(Transform::Group(g)) => {
            assert_eq!(g.num_transforms(), 2);
            assert!(matches!(&g.transforms[0], Transform::File(f) if f.src == file_path));
            // The inverse of the from_reference transform of the connection color space.
            match &g.transforms[1] {
                Transform::Exponent(e) => {
                    assert_eq!(e.direction, TransformDirection::Forward);
                    assert_eq!(e.value, [2.6, 2.6, 2.6, 1.0]);
                }
                other => panic!("expected an exponent transform, got {other:?}"),
            }
        }
        other => panic!("expected a group transform, got {other:?}"),
    }

    // The color space already exists.
    assert!(ocio::apphelpers::add_color_space(
        &mut config,
        "new_cs",
        &file_path,
        "",
        "lut_input_1"
    )
    .unwrap_err()
    .message()
    .contains("Color space name 'new_cs' already exists."));

    // Invalid connection color space.
    assert!(
        ocio::apphelpers::add_color_space(&mut config, "new_cs2", &file_path, "", "")
            .unwrap_err()
            .message()
            .contains("Invalid connection color space name.")
    );
    assert!(
        ocio::apphelpers::add_color_space(&mut config, "new_cs2", &file_path, "", "unknown")
            .unwrap_err()
            .message()
            .contains("Connection color space name 'unknown' does not exist.")
    );
}

// ---------------------------------------------------------------------------------------------
// DisplayViewHelpers

#[test]
fn display_view_helpers_basic() {
    let _l = lock();
    let cfg = category_config();

    // Step 1 - Validate the selected working color spaces.
    let mut params = ColorSpaceMenuParameters::new(cfg.clone());
    params.set_treat_no_category_as_any(false);
    params.set_app_categories("working-space");
    let working_menu = menu(&params);
    assert_eq!(
        names(&working_menu),
        [
            "lin_1",
            "lin_2",
            "log_1",
            "in_3",
            "display_lin_1",
            "display_lin_2",
            "display_log_1"
        ]
    );

    // Step 2 - Validate the selected connection color spaces.
    params.set_app_categories("LUT-connection-space");
    let connection_menu = menu(&params);
    assert_eq!(names(&connection_menu), ["lut_input_1"]);

    // Step 3 - Create a (display, view) pair.
    let mut config = cfg.create_editable_copy();
    let file_path = data_file("lut1d_green.ctf");
    display_view_helpers::add_display_view(
        &mut config,
        "DISP_1",
        "VIEW_5",
        "look_3",
        "view_5",
        "",
        "",
        "cat1, cat2",
        &file_path,
        "lut_input_1",
    )
    .unwrap();

    // Step 4 - Validate the new (display, view) pair.
    assert_eq!(config.view("DISP_1", 3), "VIEW_5");
    assert_eq!(
        config.display_view_color_space_name("DISP_1", "VIEW_5"),
        "view_5"
    );
    assert_eq!(config.display_view_looks("DISP_1", "VIEW_5"), "look_3");

    // Step 5 - Check the newly created color space.
    {
        let cs = config.get_color_space("view_5").unwrap();
        // These categories were not already used in the config, so they are ignored.
        assert!(!cs.has_category("cat1"));
        assert!(!cs.has_category("cat2"));
        assert_eq!(cs.family(), "");
        assert_eq!(cs.description(), "");
    }

    // Step 6 - Create a processor for the new (display, view) pair.
    let processor = display_view_helpers::get_processor(
        &config,
        "lin_1",
        "DISP_1",
        "VIEW_5",
        None,
        TransformDirection::Forward,
    )
    .unwrap();
    let group = processor.create_group_transform();
    Transform::Group(group.clone()).validate().unwrap();
    assert_eq!(group.num_transforms(), 7);

    // The E/C op.
    match &group.transforms[0] {
        Transform::ExposureContrast(ex) => {
            assert_eq!(ex.direction, TransformDirection::Forward);
            assert_eq!(ex.style, ExposureContrastStyle::Linear);
            assert_eq!(ex.pivot, 0.18);
            assert_eq!(ex.exposure, 0.0);
            assert!(ex.exposure_dynamic);
            assert_eq!(ex.contrast, 1.0);
            assert!(ex.contrast_dynamic);
            assert_eq!(ex.gamma, 1.0);
            assert!(!ex.gamma_dynamic);
        }
        other => panic!("unexpected {other:?}"),
    }

    // Working color space (i.e. lin_1) to the 'look' process color space (i.e. log_1).
    match &group.transforms[1] {
        Transform::Log(log) => {
            assert_eq!(log.direction, TransformDirection::Forward);
            assert_eq!(log.base, 2.0);
        }
        other => panic!("unexpected {other:?}"),
    }

    // 'look' color processing i.e. look_3.
    match &group.transforms[2] {
        Transform::Cdl(cdl) => {
            assert_eq!(cdl.direction, TransformDirection::Forward);
            assert_eq!(cdl.slope, [1.0, 2.0, 1.0]);
            assert_eq!(cdl.power, [1.0, 1.0, 1.0]);
            assert_eq!(cdl.sat, 1.0);
        }
        other => panic!("unexpected {other:?}"),
    }

    // 'look' process color space (i.e. log_1) to 'reference'.
    match &group.transforms[3] {
        Transform::Log(log) => {
            assert_eq!(log.direction, TransformDirection::Inverse);
            assert_eq!(log.base, 2.0);
        }
        other => panic!("unexpected {other:?}"),
    }

    // 'reference' to the display color space: the 'reference' to the connection color space
    // (lut_input_1) then the user 1D LUT.
    match &group.transforms[4] {
        Transform::Exponent(exp) => {
            assert_eq!(exp.direction, TransformDirection::Inverse);
            assert_eq!(exp.value, [2.6, 2.6, 2.6, 1.0]);
        }
        other => panic!("unexpected {other:?}"),
    }
    match &group.transforms[5] {
        Transform::Lut1D(lut) => {
            assert_eq!(lut.direction, TransformDirection::Forward);
            assert_eq!(lut.value(0), [0.0, 0.0, 0.0]);
            let v1 = lut.value(1);
            assert_eq!(v1[0], 0.0);
            assert!((v1[1] - 33.0 / 1023.0).abs() <= 1e-8);
            assert_eq!(v1[2], 0.0);
            let v2 = lut.value(2);
            assert_eq!(v2[0], 0.0);
            assert!((v2[1] - 66.0 / 1023.0).abs() <= 1e-8);
            assert_eq!(v2[2], 0.0);
        }
        other => panic!("unexpected {other:?}"),
    }

    // The E/C op.
    match &group.transforms[6] {
        Transform::ExposureContrast(ex) => {
            assert_eq!(ex.direction, TransformDirection::Forward);
            assert_eq!(ex.style, ExposureContrastStyle::Video);
            assert_eq!(ex.pivot, 1.0);
            assert_eq!(ex.exposure, 0.0);
            assert!(!ex.exposure_dynamic);
            assert_eq!(ex.contrast, 1.0);
            assert!(!ex.contrast_dynamic);
            assert_eq!(ex.gamma, 1.0);
            assert!(ex.gamma_dynamic);
        }
        other => panic!("unexpected {other:?}"),
    }

    // Step 7 - Some faulty scenarios.
    let err = |r: Result<()>| r.unwrap_err().message().to_string();

    // Color space already exists.
    assert!(err(display_view_helpers::add_display_view(
        &mut config,
        "",
        "VIEW_4",
        "look_3",
        "view_5",
        "",
        "",
        "cat1, cat2",
        &file_path,
        "lut_input_1"
    ))
    .contains("Color space name 'view_5' already exists."));

    // Display is empty.
    assert!(err(display_view_helpers::add_display_view(
        &mut config,
        "",
        "VIEW_4",
        "look_3",
        "view_51",
        "",
        "",
        "cat1, cat2",
        &file_path,
        "lut_input_1"
    ))
    .contains("Invalid display name."));

    // View is empty.
    assert!(err(display_view_helpers::add_display_view(
        &mut config,
        "DISP_1",
        "",
        "look_3",
        "view_51",
        "",
        "",
        "cat1, cat2",
        &file_path,
        "lut_input_1"
    ))
    .contains("Invalid view name."));

    // Connection CS does not exist.
    assert!(err(display_view_helpers::add_display_view(
        &mut config,
        "DISP_1",
        "VIEW_4",
        "look_3",
        "view_51",
        "",
        "",
        "cat1, cat2",
        &file_path,
        "lut_unknown"
    ))
    .contains("Connection color space name 'lut_unknown' does not exist."));

    // Step 8 - Remove the display/view.
    assert_eq!(config.view("DISP_1", 3), "VIEW_5");
    display_view_helpers::remove_display_view(&mut config, "DISP_1", "VIEW_5").unwrap();
    assert!(config.get_color_space("view_5").is_none());
    assert_eq!(config.view("DISP_1", 3), "");
    assert_eq!(config.view("DISP_1", 2), "VIEW_3");
}

#[test]
fn display_view_helpers_display_view_without_look() {
    let _l = lock();
    let cfg = category_config();

    let check = |channel_view: Option<&MatrixTransform>,
                 dir: TransformDirection,
                 n: usize|
     -> GroupTransform {
        let processor = display_view_helpers::get_processor(
            &cfg,
            "lin_1",
            "DISP_1",
            "VIEW_1",
            channel_view,
            dir,
        )
        .unwrap();
        let group = processor.create_group_transform();
        Transform::Group(group.clone()).validate().unwrap();
        assert_eq!(group.num_transforms(), n);
        group
    };

    // Forward direction.
    let group = check(None, TransformDirection::Forward, 3);
    match &group.transforms[1] {
        Transform::Exponent(exp) => assert_eq!(exp.direction, TransformDirection::Inverse),
        other => panic!("unexpected {other:?}"),
    }

    // Inverse direction.
    let group = check(None, TransformDirection::Inverse, 3);
    match &group.transforms[1] {
        Transform::Exponent(exp) => assert_eq!(exp.direction, TransformDirection::Forward),
        other => panic!("unexpected {other:?}"),
    }

    // Forward with a channel view matrix.
    #[rustfmt::skip]
    let mat = [
        1.0, 0.0, 0.0, 0.0,
        0.0, 0.0, 0.0, 0.0,
        0.0, 0.0, 0.0, 0.0,
        0.0, 0.0, 0.0, 0.0,
    ];
    let cv = MatrixTransform::new(mat, [0.0; 4]);
    let group = check(Some(&cv), TransformDirection::Forward, 4);
    match &group.transforms[1] {
        Transform::Matrix(mt) => {
            assert_eq!(mt.direction, TransformDirection::Forward);
            assert_eq!(mt.matrix[0], 1.0);
            assert_eq!(mt.matrix[5], 0.0);
        }
        other => panic!("unexpected {other:?}"),
    }

    // Inverse test with a channel view matrix can't be done because channel view matrix is
    // singular and inversion will fail.
}

#[test]
fn display_view_helpers_active_display_view() {
    let _l = lock();
    let mut cfg = Config::create_from_str(CATEGORY_TEST_CONFIG)
        .unwrap()
        .create_editable_copy();
    cfg.validate().unwrap();

    // Step 1 - Check the current status.
    assert_eq!(cfg.num_displays(), 2);
    assert_eq!(cfg.num_views("DISP_1"), 3);
    assert_eq!(cfg.num_views("DISP_2"), 4);

    // Step 2 - Add some active displays & views.
    cfg.set_active_displays("DISP_1").unwrap();
    cfg.set_active_views("VIEW_3, VIEW_2").unwrap();

    assert_eq!(cfg.num_displays(), 1);
    assert_eq!(cfg.display(0), "DISP_1");
    assert_eq!(cfg.num_views("DISP_1"), 2);
    assert_eq!(cfg.view("DISP_1", 0), "VIEW_3");
    assert_eq!(cfg.view("DISP_1", 1), "VIEW_2");

    // Step 3 - Create a (display, view) pair.
    let file_path = data_file("lut1d_green.ctf");
    display_view_helpers::add_display_view(
        &mut cfg,
        "DISP_1",
        "VIEW_5",
        "",
        "VIEW_5",
        "",
        "",
        "cat1, cat2",
        &file_path,
        "lut_input_1",
    )
    .unwrap();

    // The active displays & views were correctly updated.
    assert_eq!(cfg.active_displays(), "DISP_1");
    assert_eq!(cfg.active_views(), "VIEW_3, VIEW_2, VIEW_5");

    assert_eq!(cfg.num_displays(), 1);
    assert_eq!(cfg.display(0), "DISP_1");
    assert_eq!(cfg.num_views("DISP_1"), 3);
    assert_eq!(cfg.view("DISP_1", 0), "VIEW_3");
    assert_eq!(cfg.view("DISP_1", 1), "VIEW_2");
    assert_eq!(cfg.view("DISP_1", 2), "VIEW_5");

    // Step 4 - Remove a (display, view) pair.
    display_view_helpers::remove_display_view(&mut cfg, "DISP_1", "VIEW_5").unwrap();

    assert_eq!(cfg.active_displays(), "DISP_1");
    assert_eq!(cfg.active_views(), "VIEW_3, VIEW_2");
    assert_eq!(cfg.num_displays(), 1);
    assert_eq!(cfg.display(0), "DISP_1");
    assert_eq!(cfg.num_views("DISP_1"), 2);
    assert_eq!(cfg.view("DISP_1", 0), "VIEW_3");
    assert_eq!(cfg.view("DISP_1", 1), "VIEW_2");

    // Step 5 - Reset active displays & views.
    cfg.set_active_displays("").unwrap();
    cfg.set_active_views("").unwrap();

    assert_eq!(cfg.num_displays(), 2);
    assert_eq!(cfg.display(0), "DISP_1");
    assert_eq!(cfg.display(1), "DISP_2");
    assert_eq!(cfg.num_views("DISP_1"), 3);
    assert_eq!(cfg.view("DISP_1", 0), "VIEW_1");
    assert_eq!(cfg.view("DISP_1", 1), "VIEW_2");
    assert_eq!(cfg.view("DISP_1", 2), "VIEW_3");

    // Step 6 - Add some active displays.
    {
        let _g = EnvGuard::set(OCIO_ACTIVE_DISPLAYS_ENVVAR, "DISP_1");

        // Grab the envvar value.
        let mut cfg = Config::create_from_str(CATEGORY_TEST_CONFIG)
            .unwrap()
            .create_editable_copy();

        assert_eq!(cfg.num_displays(), 1);
        assert_eq!(cfg.display(0), "DISP_1");
        assert_eq!(cfg.num_views("DISP_1"), 3);
        assert_eq!(cfg.view("DISP_1", 0), "VIEW_1");
        assert_eq!(cfg.view("DISP_1", 1), "VIEW_2");
        assert_eq!(cfg.view("DISP_1", 2), "VIEW_3");

        let err = display_view_helpers::add_display_view(
            &mut cfg,
            "DISP_5",
            "VIEW_5",
            "",
            "VIEW_5",
            "",
            "",
            "cat1, cat2",
            &file_path,
            "lut_input_1",
        )
        .unwrap_err();
        assert!(err.message().contains(
            "Forbidden to add an active display as 'OCIO_ACTIVE_DISPLAYS' controls the active list."
        ));
    }

    // Step 7 - Add some active views.
    {
        let _g = EnvGuard::set(OCIO_ACTIVE_VIEWS_ENVVAR, "VIEW_3, VIEW_2");

        // Grab the envvar value.
        let mut cfg = Config::create_from_str(CATEGORY_TEST_CONFIG)
            .unwrap()
            .create_editable_copy();

        assert_eq!(cfg.num_displays(), 2);
        assert_eq!(cfg.display(0), "DISP_1");
        assert_eq!(cfg.display(1), "DISP_2");
        assert_eq!(cfg.num_views("DISP_1"), 2);
        assert_eq!(cfg.view("DISP_1", 0), "VIEW_3");
        assert_eq!(cfg.view("DISP_1", 1), "VIEW_2");

        let err = display_view_helpers::add_display_view(
            &mut cfg,
            "DISP_1",
            "VIEW_5",
            "",
            "VIEW_5",
            "",
            "",
            "cat1, cat2",
            &file_path,
            "lut_input_1",
        )
        .unwrap_err();
        assert!(err.message().contains(
            "Forbidden to add an active view as 'OCIO_ACTIVE_VIEWS' controls the active list."
        ));
    }
}

#[test]
fn display_view_helpers_remove_display_view() {
    let _l = lock();
    // Validate that a color space is removed or not depending of its usage i.e. color spaces
    // used by a ColorSpaceTransform for example. When removing a (display, view) pair the
    // associated color space is then removed only if not used.
    const CONFIG: &str = "ocio_profile_version: 2\n\
        \n\
        environment:\n\
        \x20 {}\n\
        \n\
        search_path: luts\n\
        strictparsing: true\n\
        luma: [0.2126, 0.7152, 0.0722]\n\
        \n\
        roles:\n\
        \x20 default: cs1\n\
        \n\
        displays:\n\
        \x20 disp1:\n\
        \x20   - !<View> {name: view1, colorspace: cs1}\n\
        \x20   - !<View> {name: view2, colorspace: cs2}\n\
        \x20   - !<View> {name: view3, colorspace: cs3}\n\
        \x20   - !<View> {name: view4, colorspace: cs2}\n\
        \n\
        colorspaces:\n\
        \x20 - !<ColorSpace>\n\
        \x20   name: cs1\n\
        \n\
        \x20 - !<ColorSpace>\n\
        \x20   name: cs2\n\
        \n\
        \x20 - !<ColorSpace>\n\
        \x20   name: cs3\n\
        \x20   from_reference: !<ColorSpaceTransform> {src: cs2, dst: cs2}\n";

    let mut config = Config::create_from_str(CONFIG)
        .unwrap()
        .create_editable_copy();
    config.validate().unwrap();
    assert_eq!(config.num_views("disp1"), 4);

    // Remove a (display, view) pair.
    display_view_helpers::remove_display_view(&mut config, "disp1", "view2").unwrap();
    assert_eq!(config.num_views("disp1"), 3);
    // 'cs2' still exists because it's used by 'cs3' and the (disp1, view4) pair.
    assert!(config.get_color_space("cs2").is_some());

    display_view_helpers::remove_display_view(&mut config, "disp1", "view3").unwrap();
    assert_eq!(config.num_views("disp1"), 2);
    // 'cs3' is removed because it was not used.
    assert!(config.get_color_space("cs3").is_none());

    display_view_helpers::remove_display_view(&mut config, "disp1", "view4").unwrap();
    assert_eq!(config.num_views("disp1"), 1);
    // 'cs2' is removed because it was not anymore used (i.e. 'cs3' is now removed).
    assert!(config.get_color_space("cs2").is_none());
}

#[test]
fn display_view_helpers_identity_processor() {
    let _l = lock();
    let config = Config::new();
    let identity = display_view_helpers::get_identity_processor(&config).unwrap();
    let grp = identity.create_group_transform();
    assert_eq!(grp.num_transforms(), 2);
    match (&grp.transforms[0], &grp.transforms[1]) {
        (Transform::ExposureContrast(ec0), Transform::ExposureContrast(ec1)) => {
            assert!(ec0.contrast_dynamic);
            assert!(ec0.exposure_dynamic);
            assert!(!ec0.gamma_dynamic);
            assert!(!ec1.contrast_dynamic);
            assert!(!ec1.exposure_dynamic);
            assert!(ec1.gamma_dynamic);
        }
        other => panic!("unexpected {other:?}"),
    }
}
