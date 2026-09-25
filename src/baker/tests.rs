//! Port of the non-format parts of `tests/cpu/Baker_tests.cpp`.

use super::*;

#[test]
fn accessors() {
    // Port of the accessor part of the `bake_3dlut` test.
    let config = Config::create_raw();
    let mut bake = Baker::new();
    assert!(bake.config().is_none());
    bake.set_config(&config);
    assert!(bake.config().is_some());

    let test_string = "this is some metadata!";
    bake.format_metadata_mut()
        .add_child_element("Desc", test_string);
    let data = bake.format_metadata();
    assert_eq!(data.children.len(), 1);
    assert_eq!(data.children[0].element_value(), test_string);

    bake.set_format("cinespace").unwrap();
    assert_eq!(bake.format(), "cinespace");
    bake.set_input_space("lnh");
    assert_eq!(bake.input_space(), "lnh");
    bake.set_looks("foo, +bar");
    assert_eq!(bake.looks(), "foo, +bar");
    bake.set_looks("");
    assert_eq!(bake.looks(), "");
    bake.set_target_space("gamma22");
    assert_eq!(bake.target_space(), "gamma22");
    bake.set_shaper_space("logcnt");
    assert_eq!(bake.shaper_space(), "logcnt");
    bake.set_shaper_size(Some(4));
    assert_eq!(bake.shaper_size(), Some(4));
    bake.set_cube_size(Some(2));
    assert_eq!(bake.cube_size(), Some(2));
    bake.set_display_view("display1", "view1");
    assert_eq!(bake.display(), "display1");
    assert_eq!(bake.view(), "view1");

    // The formats supporting baking.
    assert_eq!(Baker::num_formats(), 12);
    assert_eq!(Baker::format_name_by_index(4), Some("cinespace"));
    assert_eq!(Baker::format_extension_by_index(1), Some("3dl"));
    assert_eq!(Baker::format_name_by_index(12), None);
    assert_eq!(Baker::format_extension_by_index(12), None);
}

#[test]
fn set_format() {
    let mut bake = Baker::new();
    // Case insensitive lookup.
    bake.set_format("Resolve_Cube").unwrap();
    assert_eq!(bake.format(), "Resolve_Cube");

    // Unknown format.
    assert_eq!(
        bake.set_format("unknown").unwrap_err().message(),
        "File format unknown does not support baking."
    );
    // A format that exists but can't bake.
    assert_eq!(
        bake.set_format("ColorCorrection").unwrap_err().message(),
        "File format ColorCorrection does not support baking."
    );
    // The format is unchanged.
    assert_eq!(bake.format(), "Resolve_Cube");
}

fn bake_error(bake: &Baker) -> String {
    bake.bake().unwrap_err().message().to_string()
}

#[test]
fn baking_validation_basic() {
    // The validations that do not need a config with color spaces.

    // Unknown format.
    let mut bake = Baker::new();
    bake.format = "unknown".to_string();
    assert_eq!(
        bake_error(&bake),
        "The format named 'unknown' could not be found. "
    );

    // Missing configuration.
    let mut bake = Baker::new();
    bake.set_format("cinespace").unwrap();
    assert_eq!(bake_error(&bake), "No OCIO config has been set.");

    let config = Config::create_raw();

    // Missing input space.
    let mut bake = Baker::new();
    bake.set_config(&config);
    bake.set_target_space("Gamma22");
    bake.set_format("cinespace").unwrap();
    assert_eq!(bake_error(&bake), "No input space has been set.");

    // Missing target space and display / view.
    let mut bake = Baker::new();
    bake.set_config(&config);
    bake.set_input_space("Raw");
    bake.set_format("cinespace").unwrap();
    assert_eq!(
        bake_error(&bake),
        "No display / view or target colorspace has been set."
    );

    // A display without a view is not a display / view.
    bake.display = "sRGB".to_string();
    assert_eq!(
        bake_error(&bake),
        "No display / view or target colorspace has been set."
    );

    // Setting both target space and display / view.
    let mut bake = Baker::new();
    bake.set_config(&config);
    bake.set_input_space("Raw");
    bake.set_target_space("Gamma22");
    bake.set_display_view("sRGB", "Film");
    bake.set_format("cinespace").unwrap();
    assert_eq!(
        bake_error(&bake),
        "Cannot use both display / view and target colorspace."
    );

    // Invalid input space.
    let mut bake = Baker::new();
    bake.set_config(&config);
    bake.set_input_space("Invalid");
    bake.set_display_view("sRGB", "Film");
    bake.set_format("cinespace").unwrap();
    assert_eq!(
        bake_error(&bake),
        "Could not find input colorspace 'Invalid'."
    );
}

#[test]
fn baking_utils_errors() {
    let bake = Baker::new();
    assert_eq!(
        input_to_target_processor(&bake).unwrap_err().message(),
        "Input space is empty."
    );
    assert_eq!(
        shaper_to_target_processor(&bake).unwrap_err().message(),
        "Shaper space is empty."
    );
    assert_eq!(
        input_to_shaper_processor(&bake).unwrap_err().message(),
        "No OCIO config has been set."
    );
    assert_eq!(
        shaper_to_input_processor(&bake).unwrap_err().message(),
        "No OCIO config has been set."
    );
    assert_eq!(
        shaper_range(&bake).unwrap_err().message(),
        "No OCIO config has been set."
    );
    assert_eq!(
        target_range(&bake).unwrap_err().message(),
        "No OCIO config has been set."
    );

    let mut bake = Baker::new();
    bake.set_input_space("lnh");
    assert_eq!(
        input_to_target_processor(&bake).unwrap_err().message(),
        "No OCIO config has been set."
    );
}

#[test]
fn input_to_target_transforms() {
    // Target space mode: a single look transform (possibly without looks).
    let mut bake = Baker::new();
    bake.set_input_space("lnh");
    bake.set_target_space("gamma22");
    let g = input_to_target_transform(&bake);
    assert_eq!(
        g.transforms,
        vec![Transform::Look(LookTransform::new("lnh", "gamma22", ""))]
    );

    bake.set_looks("foo");
    let g = input_to_target_transform(&bake);
    assert_eq!(
        g.transforms,
        vec![Transform::Look(LookTransform::new("lnh", "gamma22", "foo"))]
    );

    // Display / view mode: the looks are applied in the input space and
    // bypassed in the display / view transform.
    let mut bake = Baker::new();
    bake.set_input_space("lnh");
    bake.set_display_view("display1", "view1");
    let g = input_to_target_transform(&bake);
    assert_eq!(
        g.transforms,
        vec![Transform::DisplayView(DisplayViewTransform::new(
            "lnh", "display1", "view1"
        ))]
    );

    bake.set_looks("contrastlook");
    let g = input_to_target_transform(&bake);
    let mut dv = DisplayViewTransform::new("lnh", "display1", "view1");
    dv.looks_bypass = true;
    assert_eq!(
        g.transforms,
        vec![
            Transform::Look(LookTransform::new("lnh", "lnh", "contrastlook")),
            Transform::DisplayView(dv)
        ]
    );
}

const BAKE_3DLUT_PROFILE: &str = "ocio_profile_version: 2\n\
\n\
file_rules:\n\
\x20 - !<Rule> {name: Default, colorspace: lnh}\n\
\n\
displays:\n\
\x20 display1:\n\
\x20   - !<View> {name: view1, colorspace: gamma22}\n\
\x20   - !<View> {name: view2, looks: satlook, colorspace: gamma22}\n\
\n\
looks:\n\
\x20 - !<Look>\n\
\x20   name : contrastlook\n\
\x20   process_space : lnh\n\
\x20   transform : !<ExponentTransform> {value: [2.2, 2.2, 2.2, 1]}\n\
\x20 - !<Look>\n\
\x20   name : satlook\n\
\x20   process_space : lnh\n\
\x20   transform : !<CDLTransform> {sat: 2}\n\
\n\
colorspaces:\n\
\x20 - !<ColorSpace>\n\
\x20   name : lnh\n\
\x20   bitdepth : 16f\n\
\x20   isdata : false\n\
\x20   allocation : lg2\n\
\n\
\x20 - !<ColorSpace>\n\
\x20   name : gamma22\n\
\x20   bitdepth : 8ui\n\
\x20   isdata : false\n\
\x20   allocation : uniform\n\
\x20   to_reference : !<ExponentTransform> {value: [2.2, 2.2, 2.2, 1]}\n\
\n\
named_transforms:\n\
- !<NamedTransform>\n\
\x20 name: logcnt\n\
\x20 transform: !<LogCameraTransform>\n\
\x20   log_side_slope:  0.247189638318671\n\
\x20   log_side_offset: 0.385536998692443\n\
\x20   lin_side_slope:  5.55555555555556\n\
\x20   lin_side_offset: 0.0522722750251688\n\
\x20   lin_side_break:  0.0105909904954696\n\
\x20   base: 10\n\
\x20   direction: inverse\n\
\n";

#[test]
fn bake_3dlut_processors() {
    // The processors and ranges the formats bake from (the format outputs of
    // the `bake_3dlut` test are checked by the format tests).
    let config = Config::create_from_str(BAKE_3DLUT_PROFILE).unwrap();
    config.validate().unwrap();

    let mut bake = Baker::new();
    bake.set_config(&config);
    bake.set_input_space("lnh");
    bake.set_target_space("gamma22");

    // gamma22 [0, 1] in lnh is [0, 1] (the exponent keeps 0 and 1).
    let (start, end) = target_range(&bake).unwrap();
    assert!((start - 0.0).abs() < 1e-6);
    assert!((end - 1.0).abs() < 1e-6);

    let cpu = input_to_target_processor(&bake).unwrap();
    let mut rgb = [0.5f32, 0.5, 0.5];
    cpu.apply_rgb(&mut rgb);
    assert!((rgb[0] - 0.5f32.powf(1.0 / 2.2)).abs() < 1e-5);

    // Named transform as a shaper space.
    bake.set_shaper_space("logcnt");
    let (start, end) = shaper_range(&bake).unwrap();
    assert!((start - -0.017290).abs() < 1e-5, "{start}");
    assert!((end - 55.080036).abs() < 1e-3, "{end}");

    let to_shaper = input_to_shaper_processor(&bake).unwrap();
    let from_shaper = shaper_to_input_processor(&bake).unwrap();
    let mut rgb = [0.18f32, 0.18, 0.18];
    to_shaper.apply_rgb(&mut rgb);
    from_shaper.apply_rgb(&mut rgb);
    assert!((rgb[0] - 0.18).abs() < 1e-5);

    let shaper_to_target = shaper_to_target_processor(&bake).unwrap();
    let mut rgb = [0.0f32; 3];
    shaper_to_target.apply_rgb(&mut rgb);
    assert!(rgb[0].is_finite());

    // Display / view with a look.
    let mut bake = Baker::new();
    bake.set_config(&config);
    bake.set_input_space("lnh");
    bake.set_looks("contrastlook");
    bake.set_display_view("display1", "view1");
    let cpu = input_to_target_processor(&bake).unwrap();
    // The look and the view cancel out.
    let mut rgb = [0.25f32, 0.5, 0.75];
    cpu.apply_rgb(&mut rgb);
    assert!((rgb[0] - 0.25).abs() < 1e-5);
    assert!((rgb[1] - 0.5).abs() < 1e-5);
    assert!((rgb[2] - 0.75).abs() < 1e-5);
}

#[test]
fn bake_3dlut() {
    // Port of the baked outputs of the `bake_3dlut` test.
    let config = Config::create_from_str(BAKE_3DLUT_PROFILE).unwrap();
    config.validate().unwrap();

    {
        // Cinespace, with the shaper derived from the lg2 allocation of
        // the input space.
        let mut bake = Baker::new();
        bake.set_config(&config);
        bake.format_metadata_mut()
            .add_child_element("Desc", "this is some metadata!");
        bake.set_format("cinespace").unwrap();
        bake.set_input_space("lnh");
        bake.set_looks("foo, +bar");
        bake.set_looks("");
        bake.set_target_space("gamma22");
        bake.set_shaper_size(Some(4));
        bake.set_cube_size(Some(2));
        let out = String::from_utf8(bake.bake().unwrap()).unwrap();

        let expected = "CSPLUTV100\n\
            3D\n\
            \n\
            BEGIN METADATA\n\
            this is some metadata!\n\
            END METADATA\n\
            \n\
            4\n\
            0.000977 0.039373 1.587401 64.000000\n\
            0.000000 0.333333 0.666667 1.000000\n\
            4\n\
            0.000977 0.039373 1.587401 64.000000\n\
            0.000000 0.333333 0.666667 1.000000\n\
            4\n\
            0.000977 0.039373 1.587401 64.000000\n\
            0.000000 0.333333 0.666667 1.000000\n\
            \n\
            2 2 2\n\
            0.042823 0.042823 0.042823\n\
            6.622026 0.042823 0.042823\n\
            0.042823 6.622026 0.042823\n\
            6.622026 6.622026 0.042823\n\
            0.042823 0.042823 6.622026\n\
            6.622026 0.042823 6.622026\n\
            0.042823 6.622026 6.622026\n\
            6.622026 6.622026 6.622026\n\
            \n";
        let res: Vec<&str> = out.lines().collect();
        let exp: Vec<&str> = expected.lines().collect();
        assert_eq!(res.len(), exp.len());
        for (i, (r, e)) in res.iter().zip(exp.iter()).enumerate() {
            if i > 6 {
                // Number comparison.
                let rv: Vec<f32> = r.split_whitespace().map(|v| v.parse().unwrap()).collect();
                let ev: Vec<f32> = e.split_whitespace().map(|v| v.parse().unwrap()).collect();
                assert_eq!(rv.len(), ev.len());
                for (a, b) in rv.iter().zip(ev.iter()) {
                    assert!((a - b).abs() < 1e-5, "line {i}: '{r}' vs '{e}'");
                }
            } else {
                // Text comparison.
                assert_eq!(r, e);
            }
        }
    }

    {
        // Resolve cube with a look cancelling out the view.
        let mut bake = Baker::new();
        bake.set_config(&config);
        bake.set_format("resolve_cube").unwrap();
        bake.set_input_space("lnh");
        bake.set_looks("contrastlook");
        bake.set_display_view("display1", "view1");
        bake.set_cube_size(Some(10));
        let out = String::from_utf8(bake.bake().unwrap()).unwrap();

        let expected = "LUT_1D_SIZE 10\n\
            0.000000 0.000000 0.000000\n\
            0.111111 0.111111 0.111111\n\
            0.222222 0.222222 0.222222\n\
            0.333333 0.333333 0.333333\n\
            0.444444 0.444444 0.444444\n\
            0.555556 0.555556 0.555556\n\
            0.666667 0.666667 0.666667\n\
            0.777778 0.777778 0.777778\n\
            0.888889 0.888889 0.888889\n\
            1.000000 1.000000 1.000000\n";
        assert_eq!(out, expected);
    }

    {
        // Resolve cube with a named transform as shaper space and a view
        // with a look.
        let mut bake = Baker::new();
        bake.set_config(&config);
        bake.set_format("resolve_cube").unwrap();
        bake.set_input_space("lnh");
        bake.set_shaper_space("logcnt");
        bake.set_display_view("display1", "view2");
        bake.set_shaper_size(Some(10));
        bake.set_cube_size(Some(2));
        let out = String::from_utf8(bake.bake().unwrap()).unwrap();

        let expected = "LUT_1D_SIZE 10\n\
            LUT_1D_INPUT_RANGE -0.017290 55.080036\n\
            LUT_3D_SIZE 2\n\
            0.000000 0.000000 0.000000\n\
            0.763998 0.763998 0.763998\n\
            0.838479 0.838479 0.838479\n\
            0.882030 0.882030 0.882030\n\
            0.912925 0.912925 0.912925\n\
            0.936887 0.936887 0.936887\n\
            0.956464 0.956464 0.956464\n\
            0.973016 0.973016 0.973016\n\
            0.987354 0.987354 0.987354\n\
            1.000000 1.000000 1.000000\n\
            0.000000 0.000000 0.000000\n\
            8.054426 0.000000 0.000000\n\
            0.000000 6.931791 0.000000\n\
            6.384501 6.384501 0.000000\n\
            0.000000 0.000000 8.336130\n\
            7.904850 0.000000 7.904850\n\
            0.000000 6.751890 6.751890\n\
            6.185304 6.185304 6.185304\n";
        assert_eq!(out, expected);
    }
}

#[test]
fn bake_all_formats() {
    // Every format supporting baking bakes, and the error of an unknown
    // format name given to a format is prefixed by the baker.
    let config = Config::create_from_str(BAKE_3DLUT_PROFILE).unwrap();
    for i in 0..Baker::num_formats() {
        let name = Baker::format_name_by_index(i).unwrap();
        let mut bake = Baker::new();
        bake.set_config(&config);
        bake.set_format(name).unwrap();
        bake.set_input_space("lnh");
        bake.set_target_space("gamma22");
        bake.set_cube_size(Some(3));
        let out = bake.bake().unwrap();
        assert!(!out.is_empty(), "{name}");
    }
}

const BAKING_VALIDATION_PROFILE: &str = r#"
        ocio_profile_version: 2

        strictparsing: false

        roles:
          scene_linear: Raw

        file_rules:
          - !<Rule> {name: Default, colorspace: Raw}

        shared_views:
          - !<View> {name: Raw, colorspace: Raw}
          - !<View> {name: RawInactive, colorspace: Raw}

        displays:
          sRGB:
            - !<Views> [Raw, RawInactive]
            - !<View> {name: Film, colorspace: sRGB}
            - !<View> {name: FilmInactive, colorspace: sRGB}
          sRGBInactive:
            - !<Views> [Raw, RawInactive]
            - !<View> {name: Film, colorspace: sRGB}
            - !<View> {name: FilmInactive, colorspace: sRGB}

        active_displays: [sRGB]
        active_views: [Film, Raw]

        looks:
        - !<Look>
          name : foo
          process_space : Raw
          transform : !<CDLTransform> {sat: 2}

        colorspaces:
        - !<ColorSpace>
          name : Raw
          isdata : false

        - !<ColorSpace>
          name : RawInactive
          isdata : false

        - !<ColorSpace>
          name : Log
          isdata : false
          to_reference: !<LogTransform> {}

        - !<ColorSpace>
          name : Saturation
          isdata : false
          to_reference: !<CDLTransform> {sat: 0.5}

        - !<ColorSpace>
          name : Log2sRGB
          isdata : false
          to_reference: !<GroupTransform>
            children:
              - !<LogTransform> {base: 2, direction: inverse}
              - !<MatrixTransform> {matrix: [3.2409, -1.5373, -0.4986, 0, -0.9692, 1.8759, 0.0415, 0, 0.0556, -0.2039, 1.0569, 0, 0, 0, 0, 1 ], direction: inverse}

        - !<ColorSpace>
          name : sRGB
          isdata : false
          from_reference: !<GroupTransform>
            children:
              - !<MatrixTransform> {matrix: [3.2409, -1.5373, -0.4986, 0, -0.9692, 1.8759, 0.0415, 0, 0.0556, -0.2039, 1.0569, 0, 0, 0, 0, 1 ]}
              - !<ExponentWithLinearTransform> {gamma: 2.4, offset: 0.055, direction: inverse}

        - !<ColorSpace>
          name : Gamma22
          isdata : false
          from_reference : !<ExponentTransform> {value: [2.2, 2.2, 2.2, 1], direction: inverse}

        named_transforms:

        - !<NamedTransform>
          name: Log2NT
          transform: !<LogTransform> {base: 2}

        inactive_colorspaces: [RawInactive]
    "#;

fn validation_baker(config: &Config, format: &str) -> Baker {
    let mut bake = Baker::new();
    bake.set_config(config);
    bake.set_format(format).unwrap();
    bake
}

#[test]
fn baking_validation() {
    let config = Config::create_from_str(BAKING_VALIDATION_PROFILE).unwrap();
    config.validate().unwrap();

    // Missing configuration.
    let mut bake = Baker::new();
    bake.set_format("cinespace").unwrap();
    assert_eq!(bake_error(&bake), "No OCIO config has been set.");

    // Missing input space.
    let mut bake = validation_baker(&config, "cinespace");
    bake.set_target_space("Gamma22");
    assert_eq!(bake_error(&bake), "No input space has been set.");

    // Missing target space and display / view.
    let mut bake = validation_baker(&config, "cinespace");
    bake.set_input_space("Raw");
    assert_eq!(
        bake_error(&bake),
        "No display / view or target colorspace has been set."
    );

    // Setting both target space and display / view.
    let mut bake = validation_baker(&config, "cinespace");
    bake.set_input_space("Raw");
    bake.set_target_space("Gamma22");
    bake.set_display_view("sRGB", "Film");
    assert_eq!(
        bake_error(&bake),
        "Cannot use both display / view and target colorspace."
    );

    // Setting looks with display / view.
    let mut bake = validation_baker(&config, "cinespace");
    bake.set_input_space("Raw");
    bake.set_display_view("sRGB", "Film");
    bake.set_looks("foo");
    bake.bake().unwrap();

    // Invalid input space.
    let mut bake = validation_baker(&config, "cinespace");
    bake.set_input_space("Invalid");
    bake.set_display_view("sRGB", "Film");
    assert_eq!(
        bake_error(&bake),
        "Could not find input colorspace 'Invalid'."
    );

    // Inactive input space.
    let mut bake = validation_baker(&config, "cinespace");
    bake.set_input_space("RawInactive");
    bake.set_display_view("sRGB", "Film");
    bake.bake().unwrap();

    // Invalid target space.
    let mut bake = validation_baker(&config, "cinespace");
    bake.set_input_space("Raw");
    bake.set_target_space("Invalid");
    assert_eq!(
        bake_error(&bake),
        "Could not find target colorspace 'Invalid'."
    );

    // Invalid display.
    let mut bake = validation_baker(&config, "cinespace");
    bake.set_input_space("Raw");
    bake.set_display_view("Invalid", "Film");
    assert_eq!(bake_error(&bake), "Could not find display 'Invalid'.");

    // Invalid view.
    let mut bake = validation_baker(&config, "cinespace");
    bake.set_input_space("Raw");
    bake.set_display_view("sRGB", "Invalid");
    assert_eq!(bake_error(&bake), "Could not find view 'Invalid'.");

    // Inactive display.
    let mut bake = validation_baker(&config, "cinespace");
    bake.set_input_space("Raw");
    bake.set_display_view("sRGBInactive", "Film");
    bake.bake().unwrap();

    // Shared view.
    let mut bake = validation_baker(&config, "cinespace");
    bake.set_input_space("Raw");
    bake.set_display_view("sRGB", "Raw");
    bake.bake().unwrap();

    // Inactive view.
    let mut bake = validation_baker(&config, "cinespace");
    bake.set_input_space("Raw");
    bake.set_display_view("sRGB", "FilmInactive");
    bake.bake().unwrap();

    // Inactive shared view.
    let mut bake = validation_baker(&config, "cinespace");
    bake.set_input_space("Raw");
    bake.set_display_view("sRGBInactive", "RawInactive");
    bake.bake().unwrap();

    // Baking 1D LUT with Crosstalk.
    let mut bake = validation_baker(&config, "spi1d");
    bake.set_input_space("Raw");
    bake.set_display_view("sRGB", "Film");
    assert_eq!(
        bake_error(&bake),
        "The format 'spi1d' does not support transformations with channel crosstalk."
    );

    // Cube Size < 2.
    let mut bake = validation_baker(&config, "cinespace");
    bake.set_input_space("Raw");
    bake.set_target_space("sRGB");
    bake.set_cube_size(Some(1));
    assert_eq!(bake_error(&bake), "Cube size must be at least 2 if set.");

    // Shaper Size < 2.
    let mut bake = validation_baker(&config, "resolve_cube");
    bake.set_input_space("Raw");
    bake.set_target_space("sRGB");
    bake.set_shaper_space("Log");
    bake.set_shaper_size(Some(1));
    assert_eq!(
        bake_error(&bake),
        "A shaper space 'Log' has been specified, so the shaper size must be 2 or larger."
    );

    // Using shaper with unsupported format.
    let mut bake = validation_baker(&config, "iridas_itx");
    bake.set_input_space("Raw");
    bake.set_target_space("sRGB");
    bake.set_shaper_space("Log");
    assert_eq!(
        bake_error(&bake),
        "The format 'iridas_itx' does not support shaper space."
    );

    // Using shaper space with Crosstalk.
    let mut bake = validation_baker(&config, "cinespace");
    bake.set_input_space("Raw");
    bake.set_target_space("sRGB");
    bake.set_shaper_space("Saturation");
    assert_eq!(
        bake_error(&bake),
        "The specified shaper space, 'Saturation' has channel crosstalk, which is not appropriate for shapers. \
         Please select an alternate shaper space or omit this option."
    );

    // Using shaper space without Crosstalk (after optimization).
    let mut bake = validation_baker(&config, "cinespace");
    bake.set_input_space("sRGB");
    bake.set_target_space("Raw");
    bake.set_shaper_space("Log2sRGB");
    bake.bake().unwrap();

    // Using NamedTransform as shaper space.
    let mut bake = validation_baker(&config, "cinespace");
    bake.set_input_space("sRGB");
    bake.set_target_space("Raw");
    bake.set_shaper_space("Log2NT");
    bake.bake().unwrap();

    // Using NamedTransform as input space is not supported.
    let mut bake = validation_baker(&config, "cinespace");
    bake.set_input_space("Log2NT");
    bake.set_target_space("sRGB");
    assert_eq!(
        bake_error(&bake),
        "Could not find input colorspace 'Log2NT'."
    );

    // Using NamedTransform as target space is not supported.
    let mut bake = validation_baker(&config, "cinespace");
    bake.set_input_space("sRGB");
    bake.set_target_space("Log2NT");
    assert_eq!(
        bake_error(&bake),
        "Could not find target colorspace 'Log2NT'."
    );
}
