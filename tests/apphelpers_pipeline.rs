//! Port of `LegacyViewingPipeline_tests.cpp` and `MixingHelpers_tests.cpp`.

use ocio::apphelpers::{LegacyViewingPipeline, MixingColorSpaceManager};
use ocio::config::{ColorSpace, NamedTransform, ViewTransform};
use ocio::*;
use std::sync::Arc;

const CATEGORY_TEST_CONFIG: &str = include_str!("data/files/apphelpers/category_test_config.ocio");

fn category_config() -> Config {
    let config = Config::create_from_str(CATEGORY_TEST_CONFIG).unwrap();
    config.validate().unwrap();
    config
}

#[track_caller]
fn assert_err<T: std::fmt::Debug>(r: ocio::Result<T>, what: &str) {
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

/// An exposure / contrast transform that is not an identity.
///
/// The C++ tests use a default (identity) `ExposureContrastTransform`; the Rust processor drops
/// identity ops when it is created, so a small exposure is used to keep the op in the pipeline.
fn non_identity_ec() -> ExposureContrastTransform {
    ExposureContrastTransform {
        exposure: 0.1,
        ..Default::default()
    }
}

fn group(p: &Processor) -> GroupTransform {
    let g = p.create_group_transform();
    Transform::Group(g.clone()).validate().unwrap();
    g
}

// ---------------------------------------------------------------------------------------------
// LegacyViewingPipeline

#[test]
fn legacy_viewing_pipeline_basic() {
    // Validate default values.
    let mut vp = LegacyViewingPipeline::new();
    assert!(vp.display_view_transform().is_none());
    assert!(vp.channel_view().is_none());
    assert!(vp.color_timing_cc().is_none());
    assert!(vp.display_cc().is_none());
    assert!(vp.linear_cc().is_none());
    assert!(!vp.looks_override_enabled());
    assert_eq!(vp.looks_override(), "");

    // An empty viewing pipeline transform is not valid.
    let mut config = Config::create_raw().create_editable_copy();
    assert_err(
        vp.get_processor_with_context(&config, config.current_context()),
        "can't create a processor without a display transform",
    );

    // Validate setters.
    let mut dte = DisplayViewTransform::default();
    vp.set_display_view_transform(Some(&dte));
    assert!(vp.display_view_transform().is_some());

    // Display transform member has to be valid.
    assert_err(
        vp.get_processor(&config),
        "LegacyViewingPipeline is not valid: DisplayViewTransform: empty source color space name",
    );

    dte.src = "colorspace1".to_string();
    vp.set_display_view_transform(Some(&dte));

    // Display transform still invalid: missing display/view.
    assert_err(
        vp.get_processor(&config),
        "LegacyViewingPipeline is not valid: DisplayViewTransform: empty display name",
    );

    dte.display = "sRGB".to_string();
    dte.view = "view1".to_string();
    vp.set_display_view_transform(Some(&dte));

    // Validation is fine but missing elements in config.
    assert_err(
        vp.get_processor(&config),
        "LegacyViewingPipeline error: Cannot find inputColorSpace, named 'colorspace1'",
    );

    let mut cs = ColorSpace::default();
    cs.set_name("colorspace1");
    cs.set_transform(
        Some(Transform::FixedFunction(FixedFunctionTransform::new(
            FixedFunctionStyle::AcesRedMod03,
            &[],
        ))),
        ColorSpaceDirection::FromReference,
    );
    config.add_color_space(&cs).unwrap();
    config
        .add_display_view("sRGB", "view1", "colorspace1", "")
        .unwrap();

    vp.get_processor(&config).unwrap();

    let ff = Transform::FixedFunction(FixedFunctionTransform::new(
        FixedFunctionStyle::AcesRedMod03,
        &[],
    ));
    vp.set_channel_view(Some(&ff));
    assert!(vp.channel_view().is_some());
    vp.get_processor(&config).unwrap();
    vp.set_channel_view(None);
    assert!(vp.channel_view().is_none());

    vp.set_color_timing_cc(Some(&ff));
    assert!(vp.color_timing_cc().is_some());
    // Missing element: color_timing role.
    assert_err(
        vp.get_processor(&config),
        "ColorTimingCC requires 'color_timing' role to be defined",
    );
    vp.set_color_timing_cc(None);
    assert!(vp.color_timing_cc().is_none());

    vp.set_linear_cc(Some(&ff));
    assert!(vp.linear_cc().is_some());
    // Missing element: scene_linear role.
    assert_err(
        vp.get_processor(&config),
        "LinearCC requires 'scene_linear' role to be defined",
    );
    vp.set_linear_cc(None);
    assert!(vp.linear_cc().is_none());

    vp.set_display_cc(Some(&ff));
    assert!(vp.display_cc().is_some());
    vp.get_processor(&config).unwrap();
    vp.set_display_cc(None);
    assert!(vp.display_cc().is_none());

    vp.set_looks_override("missingLook");
    assert_eq!(vp.looks_override(), "missingLook");

    // Look is missing but looks override is not enabled.
    vp.get_processor(&config).unwrap();

    vp.set_looks_override_enabled(true);
    assert!(vp.looks_override_enabled());

    // Missing look error.
    assert_err(
        vp.get_processor(&config),
        "The specified look, 'missingLook', cannot be found",
    );
}

#[track_caller]
fn check_exponent(t: &Transform, dir: TransformDirection, v: f64) {
    match t {
        Transform::Exponent(e) => {
            assert_eq!(e.direction, dir);
            assert_eq!(e.value, [v, v, v, 1.0]);
        }
        other => panic!("expected an exponent transform, got {other:?}"),
    }
}

#[track_caller]
fn check_log(t: &Transform, dir: TransformDirection, base: f64) {
    match t {
        Transform::Log(l) => {
            assert_eq!(l.direction, dir);
            assert_eq!(l.base, base);
        }
        other => panic!("expected a log transform, got {other:?}"),
    }
}

#[track_caller]
fn check_cdl(t: &Transform, dir: TransformDirection, slope: [f64; 3]) {
    match t {
        Transform::Cdl(c) => {
            assert_eq!(c.direction, dir);
            assert_eq!(c.slope, slope);
        }
        other => panic!("expected a CDL transform, got {other:?}"),
    }
}

#[track_caller]
fn check_ff(t: &Transform, dir: TransformDirection) {
    match t {
        Transform::FixedFunction(f) => assert_eq!(f.direction, dir),
        other => panic!("expected a fixed function transform, got {other:?}"),
    }
}

#[track_caller]
fn check_ff_style(t: &Transform, style: FixedFunctionStyle) {
    match t {
        Transform::FixedFunction(f) => {
            assert_eq!(f.direction, TransformDirection::Forward);
            assert_eq!(f.style, style);
        }
        other => panic!("expected a fixed function transform, got {other:?}"),
    }
}

#[track_caller]
fn matrix_of(t: &Transform) -> &MatrixTransform {
    match t {
        Transform::Matrix(m) => m,
        other => panic!("expected a matrix transform, got {other:?}"),
    }
}

#[test]
fn legacy_viewing_pipeline_processor_with_looks() {
    let cfg = category_config();

    let mut dt = DisplayViewTransform::new("in_1", "DISP_2", "VIEW_2");
    let mut vp = LegacyViewingPipeline::new();
    vp.set_display_view_transform(Some(&dt));

    #[rustfmt::skip]
    let mut m = [
        1.1, 0.0, 0.0, 0.0,
        0.0, 1.2, 0.0, 0.0,
        0.0, 0.0, 1.1, 0.0,
        0.0, 0.0, 0.0, 1.0,
    ];
    let mat = MatrixTransform::new(m, [0.0; 4]);
    vp.set_channel_view(Some(&Transform::Matrix(mat)));

    let ff = Transform::FixedFunction(FixedFunctionTransform::new(
        FixedFunctionStyle::AcesRedMod03,
        &[],
    ));
    vp.set_linear_cc(Some(&ff));

    // Processor in forward direction.
    let g = group(
        &vp.get_processor_with_context(&cfg, cfg.current_context())
            .unwrap(),
    );
    assert_eq!(g.num_transforms(), 8);
    // LinearCC creates a color space conversion and a transform.
    // Color space conversion from in_1 to scene_linear role (lin_1 color space).
    check_exponent(&g.transforms[0], TransformDirection::Forward, 2.6);
    // LinearCC transform.
    check_ff(&g.transforms[1], TransformDirection::Forward);
    // Lin_1 to look3 process space (log_1).
    check_log(&g.transforms[2], TransformDirection::Forward, 2.0);
    // Look_3 transform.
    check_cdl(
        &g.transforms[3],
        TransformDirection::Forward,
        [1.0, 2.0, 1.0],
    );
    // Look_3 & look_4 have the same process space, no color space conversion.
    // Look_4 transform.
    check_cdl(
        &g.transforms[4],
        TransformDirection::Inverse,
        [1.2, 2.2, 1.2],
    );
    // Channel View transform (no color space conversion).
    {
        let mt = matrix_of(&g.transforms[5]);
        assert_eq!(mt.direction, TransformDirection::Forward);
        assert_eq!(mt.matrix[0], 1.1);
        assert_eq!(mt.matrix[1], 0.0);
        assert_eq!(mt.matrix[2], 0.0);
        assert_eq!(mt.matrix[3], 0.0);
        assert_eq!(mt.matrix[5], 1.2);
        assert_eq!(mt.matrix[10], 1.1);
    }
    // Look_4 process color space (log_1) to reference.
    check_log(&g.transforms[6], TransformDirection::Inverse, 2.0);
    // Reference to view_2 color space.
    check_exponent(&g.transforms[7], TransformDirection::Inverse, 2.4);

    // Repeat in inverse direction.
    dt.direction = TransformDirection::Inverse;
    vp.set_display_view_transform(Some(&dt));
    let g = group(
        &vp.get_processor_with_context(&cfg, cfg.current_context())
            .unwrap(),
    );
    assert_eq!(g.num_transforms(), 8);

    // Apply the inverse view transform, channel view, and looks.
    // View_2 to reference.
    check_exponent(&g.transforms[0], TransformDirection::Forward, 2.4);
    // Reference to look_4 process color space (log_1).
    check_log(&g.transforms[1], TransformDirection::Forward, 2.0);
    // Channel View transform.
    {
        let mt = matrix_of(&g.transforms[2]);
        assert_eq!(mt.direction, TransformDirection::Forward);
        assert_eq!(mt.matrix[0], 1.0 / 1.1);
        assert_eq!(mt.matrix[5], 1.0 / 1.2);
        assert_eq!(mt.matrix[10], 1.0 / 1.1);
    }
    // Look_4 transform.
    check_cdl(
        &g.transforms[3],
        TransformDirection::Forward,
        [1.2, 2.2, 1.2],
    );
    // Look_3 transform.
    check_cdl(
        &g.transforms[4],
        TransformDirection::Inverse,
        [1.0, 2.0, 1.0],
    );
    // Look_3 process color space (log_1) to lin_1.
    check_log(&g.transforms[5], TransformDirection::Inverse, 2.0);
    // LinearCC transform.
    check_ff(&g.transforms[6], TransformDirection::Inverse);
    // LinearCC color space conversion.
    check_exponent(&g.transforms[7], TransformDirection::Inverse, 2.6);

    // Channel view with alpha will cause color space conversions to be skipped if data bypass
    // is enabled (looks are also bypassed).
    m[3] = 0.1;
    let mat = MatrixTransform::new(m, [0.0; 4]);
    vp.set_channel_view(Some(&Transform::Matrix(mat)));
    let g = group(
        &vp.get_processor_with_context(&cfg, cfg.current_context())
            .unwrap(),
    );
    assert_eq!(g.num_transforms(), 2);
    // Channel view.
    matrix_of(&g.transforms[0]);
    // LinearCC transform.
    check_ff(&g.transforms[1], TransformDirection::Inverse);

    // Looks are still applied if looks override is used.
    vp.set_looks_override_enabled(true);
    vp.set_looks_override(cfg.display_view_looks("DISP_2", "VIEW_2"));
    let g = group(
        &vp.get_processor_with_context(&cfg, cfg.current_context())
            .unwrap(),
    );
    assert_eq!(g.num_transforms(), 4);
    // Channel view.
    matrix_of(&g.transforms[0]);
    // Look_4 transform.
    assert!(matches!(g.transforms[1], Transform::Cdl(_)));
    // Look_3 transform.
    assert!(matches!(g.transforms[2], Transform::Cdl(_)));
    // LinearCC transform.
    assert!(matches!(g.transforms[3], Transform::FixedFunction(_)));

    dt.data_bypass = false;
    vp.set_display_view_transform(Some(&dt));
    let g = group(
        &vp.get_processor_with_context(&cfg, cfg.current_context())
            .unwrap(),
    );
    assert_eq!(g.num_transforms(), 8);
}

#[test]
fn legacy_viewing_pipeline_full_pipeline_no_look() {
    // Validate the pipeline where the display/view is a simple color space (i.e., no view
    // transform).

    let src = "source";
    let dst = "destination";
    let linear_cs = "linear_cs";
    let timing_cs = "color_timing_cs";

    let mut cfg = Config::create_raw().create_editable_copy();
    let mut cs_source = ColorSpace::default();
    cs_source.set_name(src);
    let offset_src = [0.0, 0.1, 0.2, 0.0];
    cs_source.set_transform(
        Some(Transform::Matrix(MatrixTransform {
            offset: offset_src,
            ..Default::default()
        })),
        ColorSpaceDirection::ToReference,
    );
    cfg.add_color_space(&cs_source).unwrap();

    let ff = |style: FixedFunctionStyle| {
        Some(Transform::FixedFunction(FixedFunctionTransform::new(
            style,
            &[],
        )))
    };

    let mut cs = ColorSpace::default();
    cs.set_name(dst);
    cs.set_transform(
        ff(FixedFunctionStyle::AcesGlow03),
        ColorSpaceDirection::FromReference,
    );
    cfg.add_color_space(&cs).unwrap();

    let mut cs = ColorSpace::default();
    cs.set_name(linear_cs);
    cs.set_transform(
        ff(FixedFunctionStyle::AcesGlow10),
        ColorSpaceDirection::FromReference,
    );
    cs.set_transform(
        ff(FixedFunctionStyle::AcesRedMod10),
        ColorSpaceDirection::ToReference,
    );
    cfg.add_color_space(&cs).unwrap();
    cfg.set_role(ROLE_SCENE_LINEAR, Some(linear_cs)).unwrap();

    let mut cs = ColorSpace::default();
    cs.set_name(timing_cs);
    cs.set_transform(
        ff(FixedFunctionStyle::RgbToHsv),
        ColorSpaceDirection::FromReference,
    );
    cs.set_transform(
        ff(FixedFunctionStyle::AcesDarkToDim10),
        ColorSpaceDirection::ToReference,
    );
    cfg.add_color_space(&cs).unwrap();
    cfg.set_role(ROLE_COLOR_TIMING, Some(timing_cs)).unwrap();

    let display = "display";
    let view = "view";
    cfg.add_display_view(display, view, dst, "").unwrap();
    cfg.validate().unwrap();

    let mut dt = DisplayViewTransform::new(src, display, view);

    let mut vp = LegacyViewingPipeline::new();
    vp.set_display_view_transform(Some(&dt));

    let offset_linear_cc = [0.2, 0.3, 0.4, 0.0];
    vp.set_linear_cc(Some(&Transform::Matrix(MatrixTransform {
        offset: offset_linear_cc,
        ..Default::default()
    })));
    let value_timing_cc = [2.2, 2.3, 2.4, 1.0];
    vp.set_color_timing_cc(Some(&Transform::Exponent(ExponentTransform::new(
        value_timing_cc,
    ))));
    let offset_cv = [0.2, 0.1, 0.1, 0.0];
    vp.set_channel_view(Some(&Transform::Matrix(MatrixTransform {
        offset: offset_cv,
        ..Default::default()
    })));
    vp.set_display_cc(Some(&Transform::ExposureContrast(non_identity_ec())));

    {
        let g = group(&vp.get_processor(&cfg).unwrap());
        assert_eq!(g.num_transforms(), 10);

        // 0. Input to reference.
        let m = matrix_of(&g.transforms[0]);
        assert_eq!(m.direction, TransformDirection::Forward);
        assert_eq!(m.offset, offset_src);
        // 1. Scene linear role from reference.
        check_ff_style(&g.transforms[1], FixedFunctionStyle::AcesGlow10);
        // 2. LinearCC.
        let m = matrix_of(&g.transforms[2]);
        assert_eq!(m.direction, TransformDirection::Forward);
        assert_eq!(m.offset, offset_linear_cc);
        // 3. Scene linear role to reference.
        check_ff_style(&g.transforms[3], FixedFunctionStyle::AcesRedMod10);
        // 4. ColorTiming from reference.
        check_ff_style(&g.transforms[4], FixedFunctionStyle::RgbToHsv);
        // 5. ColorTimingCC.
        match &g.transforms[5] {
            Transform::Exponent(e) => {
                assert_eq!(e.direction, TransformDirection::Forward);
                assert_eq!(e.value, value_timing_cc);
            }
            other => panic!("unexpected {other:?}"),
        }
        // 6. ChannelView.
        let m = matrix_of(&g.transforms[6]);
        assert_eq!(m.direction, TransformDirection::Forward);
        assert_eq!(m.offset, offset_cv);
        // 7. ColorTiming to reference.
        check_ff_style(&g.transforms[7], FixedFunctionStyle::AcesDarkToDim10);
        // 8. DisplayCS from reference.
        check_ff_style(&g.transforms[8], FixedFunctionStyle::AcesGlow03);
        // 9. DisplayCC.
        assert!(matches!(g.transforms[9], Transform::ExposureContrast(_)));
    }

    //
    // Using a scene-referred view transform.
    //

    let dsp = "display";
    let mut cs = ColorSpace::new(ReferenceSpaceType::Display);
    cs.set_name(dsp);
    cs.set_transform(
        Some(Transform::ExposureContrast(non_identity_ec())),
        ColorSpaceDirection::FromReference,
    );
    cfg.add_color_space(&cs).unwrap();

    let scene_vt = "scene_vt";
    let mut vt = ViewTransform::new(ReferenceSpaceType::Scene);
    vt.set_name(scene_vt);
    vt.set_transform(
        Some(Transform::Log(LogTransform::new(4.2))),
        ViewTransformDirection::FromReference,
    );
    cfg.add_view_transform(&vt).unwrap();

    let viewt = "viewt";
    cfg.add_display_view_full(display, viewt, scene_vt, dsp, "", "", "")
        .unwrap();
    cfg.validate().unwrap();

    dt.view = viewt.to_string();
    vp.set_display_view_transform(Some(&dt));

    {
        let g = group(&vp.get_processor(&cfg).unwrap());
        // Getting an additional op for the reference space change.
        assert_eq!(g.num_transforms(), 11);

        // 0 to 7: same as previous up to colorTiming to reference.
        // 8. Changing from scene-referred space to display-referred space done with the
        //    specified view transform.
        check_log(&g.transforms[8], TransformDirection::Forward, 4.2);
        // 9. DisplayCS from reference.
        assert!(matches!(g.transforms[9], Transform::ExposureContrast(_)));
        // 10. DisplayCC.
        assert!(matches!(g.transforms[10], Transform::ExposureContrast(_)));
    }

    //
    // Adding a display-referred view transform.
    //

    let display_vt = "display_vt";
    let mut vt = ViewTransform::new(ReferenceSpaceType::Display);
    vt.set_name(display_vt);
    vt.set_transform(
        Some(Transform::Log(LogTransform::new(2.1))),
        ViewTransformDirection::FromReference,
    );
    cfg.add_view_transform(&vt).unwrap();

    // Replace view display.
    cfg.add_display_view_full(display, viewt, display_vt, dsp, "", "", "")
        .unwrap();
    cfg.validate().unwrap();

    {
        let g = group(&vp.get_processor(&cfg).unwrap());
        // Getting an additional op for the display to display view transform.
        assert_eq!(g.num_transforms(), 12);

        // 0 to 8: same as previous up to scene-referred to display referred using the default
        // view transform.
        // 9. Display-referred reference to display-referred reference using the specified view
        //    transform.
        check_log(&g.transforms[9], TransformDirection::Forward, 2.1);
        // 10. DisplayCS from reference.
        assert!(matches!(g.transforms[10], Transform::ExposureContrast(_)));
        // 11. DisplayCC.
        assert!(matches!(g.transforms[11], Transform::ExposureContrast(_)));
    }

    // Using a named transform.
    let mut nt = NamedTransform::new();
    nt.set_name("nt1");
    let offset_nt = [0.01, 0.05, 0.1, 0.0];
    nt.set_transform(
        Some(Transform::Matrix(MatrixTransform {
            offset: offset_nt,
            ..Default::default()
        })),
        TransformDirection::Forward,
    );
    cfg.add_named_transform(&nt).unwrap();

    {
        let viewnt = "viewnt";
        cfg.add_display_view(display, viewnt, "nt1", "").unwrap();
        cfg.validate().unwrap();

        dt.view = viewnt.to_string();
        vp.set_display_view_transform(Some(&dt));

        let g = group(&vp.get_processor(&cfg).unwrap());
        assert_eq!(g.num_transforms(), 5);

        // 0. LinearCC.
        matrix_of(&g.transforms[0]);
        // 1. ColorTimingCC.
        assert!(matches!(g.transforms[1], Transform::Exponent(_)));
        // 2. ChannelView.
        matrix_of(&g.transforms[2]);
        // 3. Named transform.
        let m = matrix_of(&g.transforms[3]);
        assert_eq!(m.direction, TransformDirection::Forward);
        assert_eq!(m.offset, offset_nt);
        // 4. DisplayCC.
        assert!(matches!(g.transforms[4], Transform::ExposureContrast(_)));
    }

    dt.view = viewt.to_string();
    vp.set_display_view_transform(Some(&dt));
    cs_source.set_is_data(true);
    cfg.add_color_space(&cs_source).unwrap();
    cfg.validate().unwrap();

    {
        let g = group(&vp.get_processor(&cfg).unwrap());
        // Color space conversion is skipped.
        assert_eq!(g.num_transforms(), 4);

        // With isData true, the view/display transform is not applied. The CC and channel view
        // are applied, but without converting to their usual process spaces.
        // 0. LinearCC.
        matrix_of(&g.transforms[0]);
        // 1. ColorTimingCC.
        assert!(matches!(g.transforms[1], Transform::Exponent(_)));
        // 2. ChannelView.
        matrix_of(&g.transforms[2]);
        // 3. DisplayCC.
        assert!(matches!(g.transforms[3], Transform::ExposureContrast(_)));
    }
}

#[test]
fn legacy_viewing_pipeline_processor_with_no_op_look() {
    // Validate the pipeline when a noop look override is specified.
    let cfg = category_config();

    let mut dt = DisplayViewTransform::new("in_1", "DISP_2", "VIEW_2");

    let mut vp = LegacyViewingPipeline::new();
    vp.set_display_view_transform(Some(&dt));
    vp.set_looks_override_enabled(true);
    vp.set_looks_override("look_noop");

    // Processor in forward direction.
    group(
        &vp.get_processor_with_context(&cfg, cfg.current_context())
            .unwrap(),
    );

    // Repeat in inverse direction.
    dt.direction = TransformDirection::Inverse;
    vp.set_display_view_transform(Some(&dt));
    vp.set_looks_override_enabled(true);
    vp.set_looks_override("look_noop");

    // Processor in inverse direction.
    group(
        &vp.get_processor_with_context(&cfg, cfg.current_context())
            .unwrap(),
    );
}

#[test]
fn legacy_viewing_pipeline_serialization() {
    let mut vp = LegacyViewingPipeline::new();
    assert_eq!(vp.to_string(), "");
    vp.set_looks_override_enabled(true);
    vp.set_looks_override("look_1");
    assert_eq!(
        vp.to_string(),
        "LooksOverrideEnabled, LooksOverride: look_1"
    );
    let dt = DisplayViewTransform::new("in", "disp", "view");
    vp.set_display_view_transform(Some(&dt));
    assert!(vp
        .to_string()
        .starts_with("DisplayViewTransform: <DisplayViewTransform "));
    // The looks of the display / view transform are bypassed (applied by the pipeline).
    assert!(vp.display_view_transform().unwrap().looks_bypass);
}

// ---------------------------------------------------------------------------------------------
// MixingColorSpaceManager / MixingSlider

#[test]
fn mixing_color_space_manager_basic() {
    let config = Arc::new(category_config());
    let mut mixing = MixingColorSpaceManager::new(config);

    {
        let p = mixing
            .get_processor("lin_1", "DISP_1", "VIEW_1", TransformDirection::Forward)
            .unwrap();
        let g = group(&p);
        // Mixing in the rendering space uses an identity matrix. The C++ processor keeps it (one
        // matrix transform) but the Rust processor removes identity ops when it is created.
        assert_eq!(g.num_transforms(), 0);
    }

    assert_eq!(mixing.selected_mixing_encoding_idx(), 0);
    assert_eq!(mixing.num_mixing_encodings(), 2);

    mixing.set_selected_mixing_encoding("HSV").unwrap();
    assert_eq!(mixing.selected_mixing_encoding_idx(), 1);
    mixing.set_selected_mixing_encoding_idx(0).unwrap();
    assert_eq!(mixing.selected_mixing_encoding_idx(), 0);

    assert!(mixing.set_selected_mixing_encoding("HS").is_err());

    mixing.set_selected_mixing_encoding_idx(1).unwrap(); // i.e. HSV

    {
        let p = mixing
            .get_processor("lin_1", "DISP_1", "VIEW_1", TransformDirection::Forward)
            .unwrap();
        let g = group(&p);
        // The identity matrix is removed (see above), only the HSV conversion remains.
        assert_eq!(g.num_transforms(), 1);
        check_ff_style(&g.transforms[0], FixedFunctionStyle::RgbToHsv);
    }

    assert_eq!(mixing.selected_mixing_space_idx(), 0);
    assert_eq!(mixing.num_mixing_spaces(), 2);

    mixing.set_selected_mixing_space("Display Space").unwrap();
    assert_eq!(mixing.selected_mixing_space_idx(), 1);
    mixing.set_selected_mixing_space_idx(0).unwrap();
    assert_eq!(mixing.selected_mixing_space_idx(), 0);

    assert!(mixing.set_selected_mixing_space("DisplaySpace").is_err());

    mixing.set_selected_mixing_space_idx(1).unwrap(); // i.e. 'Display Space'

    {
        let p = mixing
            .get_processor("lin_1", "DISP_1", "VIEW_1", TransformDirection::Forward)
            .unwrap();
        let g = group(&p);
        assert_eq!(g.num_transforms(), 2);
        check_exponent(&g.transforms[0], TransformDirection::Inverse, 2.6);
        check_ff_style(&g.transforms[1], FixedFunctionStyle::RgbToHsv);
    }

    // Some other accessors.
    assert_eq!(mixing.mixing_space_ui_name(0).unwrap(), "Rendering Space");
    assert_err(
        mixing.mixing_space_ui_name(2),
        "Invalid mixing space index 2 where size is 2.",
    );
    assert_eq!(mixing.mixing_encoding_name(1).unwrap(), "HSV");
    assert_err(
        mixing.mixing_encoding_name(2),
        "Invalid mixing encoding index 2 where size is 2.",
    );
    assert_err(
        mixing.set_selected_mixing_encoding_idx(2),
        "Invalid idx for the mixing encoding index 2 where size is 2.",
    );
}

#[test]
fn mixing_color_space_manager_color_picker_role() {
    let config = category_config();
    let mut mixing = MixingColorSpaceManager::new(Arc::new(config.clone()));
    assert_eq!(mixing.num_mixing_spaces(), 2);

    // Add a color_picking role.
    let mut cfg = config.create_editable_copy();
    assert!(!cfg.has_role(ROLE_COLOR_PICKING));
    cfg.set_role(ROLE_COLOR_PICKING, Some("log_1")).unwrap();

    // The config changes so refresh the templates.
    mixing.refresh(Arc::new(cfg));
    assert_eq!(mixing.num_mixing_spaces(), 1);
    assert_eq!(
        mixing.mixing_space_ui_name(0).unwrap(),
        "color_picking (log_1)"
    );

    {
        let p = mixing
            .get_processor("lin_1", "DISP_1", "VIEW_1", TransformDirection::Forward)
            .unwrap();
        let g = group(&p);
        assert_eq!(g.num_transforms(), 1);
        check_log(&g.transforms[0], TransformDirection::Forward, 2.0);
    }

    mixing.set_selected_mixing_encoding_idx(1).unwrap(); // i.e. HSV

    {
        let p = mixing
            .get_processor("lin_1", "DISP_1", "VIEW_1", TransformDirection::Forward)
            .unwrap();
        let g = group(&p);
        assert_eq!(g.num_transforms(), 2);
        check_log(&g.transforms[0], TransformDirection::Forward, 2.0);
        check_ff_style(&g.transforms[1], FixedFunctionStyle::RgbToHsv);
    }

    assert_err(
        mixing.set_selected_mixing_space_idx(1), // i.e. Display
        "Invalid idx for the mixing space index 1 where size is 1.",
    );
}

/// Port of the `FLOAT_CHECK_EQUAL(a, b)` helper: `int(a) == int(b * 100000.)`.
#[track_caller]
fn float_check_equal(a: i32, b: f32) {
    assert_eq!(a, (b as f64 * 100000.0) as i32, "value {b}");
}

#[test]
fn mixing_slider_basic() {
    let config = Arc::new(category_config());
    let mut mixing = MixingColorSpaceManager::new(config);

    let slider = mixing.slider_with_edges(0.0, 1.0);

    for encoding in [1, 0] {
        // i.e. HSV then RGB.
        mixing.set_selected_mixing_encoding_idx(encoding).unwrap();

        // Needs linear to perceptually linear adjustment.

        mixing.set_selected_mixing_space_idx(0).unwrap(); // i.e. Rendering Space
        assert_eq!(mixing.selected_mixing_space_idx(), 0);

        slider.set_slider_min_edge(0.0);
        slider.set_slider_max_edge(1.0);

        float_check_equal(0, slider.slider_min_edge());
        float_check_equal(83386, slider.slider_max_edge());

        float_check_equal(37923, slider.mixing_to_slider(0.1));
        float_check_equal(80144, slider.mixing_to_slider(0.5));

        float_check_equal(10000, slider.slider_to_mixing(0.379232));
        float_check_equal(50000, slider.slider_to_mixing(0.801448));

        slider.set_slider_min_edge(-0.2);
        slider.set_slider_max_edge(5.0);

        float_check_equal(3792, slider.mixing_to_slider(-0.1));
        float_check_equal(31573, slider.mixing_to_slider(0.1));
        float_check_equal(58279, slider.mixing_to_slider(0.5));
        float_check_equal(90744, slider.mixing_to_slider(3.0));

        float_check_equal(-10000, slider.slider_to_mixing(0.037927));
        float_check_equal(10000, slider.slider_to_mixing(0.315733));
        float_check_equal(50000, slider.slider_to_mixing(0.582797));
        float_check_equal(300000, slider.slider_to_mixing(0.907444));

        // Does not need any linear to perceptually linear adjustment.

        mixing.set_selected_mixing_space_idx(1).unwrap(); // i.e. Display Space
        assert_eq!(mixing.selected_mixing_space_idx(), 1);

        slider.set_slider_min_edge(0.0);
        slider.set_slider_max_edge(1.0);

        float_check_equal(0, slider.slider_min_edge());
        float_check_equal(100000, slider.slider_max_edge());

        float_check_equal(10000, slider.mixing_to_slider(0.1));
        float_check_equal(50000, slider.mixing_to_slider(0.5));

        float_check_equal(37923, slider.slider_to_mixing(0.379232));
        float_check_equal(80144, slider.slider_to_mixing(0.801448));

        slider.set_slider_min_edge(-0.2);
        slider.set_slider_max_edge(5.0);

        float_check_equal(0, slider.mixing_to_slider(slider.slider_min_edge()));
        float_check_equal(100000, slider.mixing_to_slider(slider.slider_max_edge()));

        float_check_equal(1923, slider.mixing_to_slider(-0.1));
        float_check_equal(5769, slider.mixing_to_slider(0.1));
        float_check_equal(13461, slider.mixing_to_slider(0.5));
        float_check_equal(61538, slider.mixing_to_slider(3.0));

        float_check_equal(-277, slider.slider_to_mixing(0.037927));
        float_check_equal(144181, slider.slider_to_mixing(0.315733));
        float_check_equal(283054, slider.slider_to_mixing(0.582797));
        float_check_equal(451870, slider.slider_to_mixing(0.907444));
    }
}

#[test]
fn mixing_slider_color_picker_role() {
    let config = category_config();
    let mut mixing = MixingColorSpaceManager::new(Arc::new(config.clone()));

    // Add the color_picking role.
    let mut cfg = config.create_editable_copy();
    assert!(!cfg.has_role(ROLE_COLOR_PICKING));
    cfg.set_role(ROLE_COLOR_PICKING, Some("lin_1")).unwrap();

    // Refresh the templates as the config changed.
    mixing.refresh(Arc::new(cfg));

    assert_eq!(mixing.num_mixing_spaces(), 1);
    assert_eq!(
        mixing.mixing_space_ui_name(0).unwrap(),
        "color_picking (lin_1)"
    );

    assert_err(
        mixing.set_selected_mixing_space_idx(1),
        "Invalid idx for the mixing space index 1 where size is 1.",
    );

    mixing.set_selected_mixing_encoding_idx(1).unwrap(); // i.e. HSV
    mixing.set_selected_mixing_space_idx(0).unwrap(); // i.e. Color Picker role

    let slider = mixing.slider_with_edges(0.0, 1.0);
    float_check_equal(50501, slider.mixing_to_slider(0.50501));
    float_check_equal(50501, slider.slider_to_mixing(0.50501));

    mixing.set_selected_mixing_encoding_idx(0).unwrap(); // i.e. RGB
    mixing.set_selected_mixing_space_idx(0).unwrap(); // i.e. Color Picker role

    float_check_equal(50501, slider.mixing_to_slider(0.50501));
    float_check_equal(50501, slider.slider_to_mixing(0.50501));

    assert!(mixing
        .to_string()
        .ends_with("selectedMixingSpaceIdx: 0, selectedMixingEncodingIdx: 0, colorPicking"));
}
