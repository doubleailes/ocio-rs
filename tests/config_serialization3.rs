//! Port of `Config_tests.cpp` (part 6: fixed function, exposure contrast,
//! matrix, CDL and file transforms serialization).

mod config_common;

use config_common::profiles::*;
use config_common::*;
use ocio::config::logging::LogGuard;
use ocio::config::ColorSpace;
use ocio::*;

fn v2(end: &str) -> String {
    format!("{}{end}", profile_v2_start())
}

fn group(children: &str) -> String {
    format!("    from_scene_reference: !<GroupTransform>\n      children:\n{children}")
}

#[test]
fn config_fixed_function_serialization() {
    check_roundtrip(&v2(&group(
        r#"        - !<FixedFunctionTransform> {style: ACES_RedMod03}
        - !<FixedFunctionTransform> {style: ACES_RedMod03, direction: inverse}
        - !<FixedFunctionTransform> {style: ACES_RedMod10}
        - !<FixedFunctionTransform> {style: ACES_RedMod10, direction: inverse}
        - !<FixedFunctionTransform> {style: ACES_Glow03}
        - !<FixedFunctionTransform> {style: ACES_Glow03, direction: inverse}
        - !<FixedFunctionTransform> {style: ACES_Glow10}
        - !<FixedFunctionTransform> {style: ACES_Glow10, direction: inverse}
        - !<FixedFunctionTransform> {style: ACES_DarkToDim10}
        - !<FixedFunctionTransform> {style: ACES_DarkToDim10, direction: inverse}
        - !<FixedFunctionTransform> {style: REC2100_Surround, params: [0.75]}
        - !<FixedFunctionTransform> {style: REC2100_Surround, params: [0.75], direction: inverse}
        - !<FixedFunctionTransform> {style: RGB_TO_HSV}
        - !<FixedFunctionTransform> {style: RGB_TO_HSV, direction: inverse}
        - !<FixedFunctionTransform> {style: XYZ_TO_xyY}
        - !<FixedFunctionTransform> {style: XYZ_TO_xyY, direction: inverse}
        - !<FixedFunctionTransform> {style: XYZ_TO_uvY}
        - !<FixedFunctionTransform> {style: XYZ_TO_uvY, direction: inverse}
        - !<FixedFunctionTransform> {style: XYZ_TO_LUV}
        - !<FixedFunctionTransform> {style: XYZ_TO_LUV, direction: inverse}
"#,
    )));

    let gamut13 = group(
        r#"        - !<FixedFunctionTransform> {style: ACES_GamutComp13, params: [1.147, 1.264, 1.312, 0.815, 0.803, 0.88, 1.2]}
        - !<FixedFunctionTransform> {style: ACES_GamutComp13, params: [1.147, 1.264, 1.312, 0.815, 0.803, 0.88, 1.2], direction: inverse}
"#,
    );
    check_roundtrip(&format!("{}{gamut13}", profile_v21_start()));
    assert_err!(
        Config::create_from_str(&v2(&gamut13)),
        "Only config version 2.1 (or higher) can have FixedFunctionTransform style 'ACES_GAMUT_COMP_13'."
    );
    assert_err!(
        Config::create_from_str(&v2(&group("        - !<FixedFunctionTransform> {style: ACES_GamutComp13}\n"))),
        "Only config version 2.1 (or higher) can have FixedFunctionTransform style 'ACES_GAMUT_COMP_13'."
    );
    assert_err!(
        Config::create_from_str(&v2(&group("        - !<FixedFunctionTransform> {direction: inverse}\n"))),
        "'FixedFunctionTransform' parsing failed: style value is missing."
    );

    for style in [
        "{style: Lin_TO_PQ}",
        "{style: Lin_TO_GammaLog, params: [0.0, 0.25, 0.5, 1.0, 0.0, 2.718, 0.17883277, 0.807825590164, 1.0, -0.07116723]}",
        "{style: Lin_TO_DoubleLog, params: [10.0, 0.25, 0.5, -1.0, 0.0, -1.0, 1.25, 1.0, 1.0, 1.0, 0.5, 1.0, 0.0]}",
    ] {
        let end = group(&format!("        - !<FixedFunctionTransform> {style}\n"));
        let name = style.split([',', '}']).next().unwrap().trim_start_matches("{style: ");
        assert_err!(
            Config::create_from_str(&format!("{}{end}", profile_start_v(2, 3))),
            &format!("Only config version 2.4 (or higher) can have FixedFunctionTransform style '{name}'.")
        );
        Config::create_from_str(&format!("{}{end}", profile_start_v(2, 4))).unwrap();
    }

    let aces2 = group(
        r#"        - !<FixedFunctionTransform> {style: ACES2_OutputTransform, params: [100, 0.64, 0.33, 0.3, 0.6, 0.15, 0.06, 0.3127, 0.329]}
        - !<FixedFunctionTransform> {style: ACES2_OutputTransform, params: [100, 0.64, 0.33, 0.3, 0.6, 0.15, 0.06, 0.3127, 0.329], direction: inverse}
        - !<FixedFunctionTransform> {style: ACES2_RGB_TO_JMh, params: [0.64, 0.33, 0.3, 0.6, 0.15, 0.06, 0.3127, 0.329]}
        - !<FixedFunctionTransform> {style: ACES2_RGB_TO_JMh, params: [0.64, 0.33, 0.3, 0.6, 0.15, 0.06, 0.3127, 0.329], direction: inverse}
        - !<FixedFunctionTransform> {style: ACES2_TonescaleCompress, params: [100]}
        - !<FixedFunctionTransform> {style: ACES2_TonescaleCompress, params: [100], direction: inverse}
        - !<FixedFunctionTransform> {style: ACES2_GamutCompress, params: [100, 0.64, 0.33, 0.3, 0.6, 0.15, 0.06, 0.3127, 0.329]}
        - !<FixedFunctionTransform> {style: ACES2_GamutCompress, params: [100, 0.64, 0.33, 0.3, 0.6, 0.15, 0.06, 0.3127, 0.329], direction: inverse}
"#,
    );
    let s = format!("{}{aces2}", profile_start_v(2, 4));
    let config = {
        let log = LogGuard::new();
        let config = Config::create_from_str(&s).unwrap();
        config.validate().unwrap();
        let mut expected = String::new();
        for style in ["ACES2_OutputTransform", "ACES2_RGB_TO_JMh", "ACES2_TonescaleCompress", "ACES2_GamutCompress"] {
            for _ in 0..2 {
                expected.push_str(&format!(
                    "[OpenColorIO Warning]: FixedFunction style is experimental and may be removed in a future release: '{style}'.\n"
                ));
            }
        }
        assert_eq!(log.output(), expected);
        config
    };
    {
        let _log = LogGuard::new();
        assert_eq!(config.serialize().unwrap(), s);
    }

    let hmj = group(
        r#"        - !<FixedFunctionTransform> {style: ACES2_RGB_TO_HMJ, params: [0.64, 0.33, 0.3, 0.6, 0.15, 0.06, 0.3127, 0.329]}
        - !<FixedFunctionTransform> {style: ACES2_RGB_TO_HMJ, params: [0.64, 0.33, 0.3, 0.6, 0.15, 0.06, 0.3127, 0.329], direction: inverse}
"#,
    );
    {
        let _log = LogGuard::new();
        assert_err!(
            Config::create_from_str(&format!("{}{hmj}", profile_start_v(2, 5))),
            "Only config version 2.6 (or higher) can have FixedFunctionTransform style 'ACES2_RGB_TO_HMJ'."
        );
        Config::create_from_str(&format!("{}{hmj}", profile_start_v(2, 6))).unwrap();
    }

    for style in ["ACES2_OutputTransform", "ACES2_RGB_TO_JMh", "ACES2_TonescaleCompress", "ACES2_GamutCompress"] {
        let end = group(&format!(
            "        - !<FixedFunctionTransform> {{style: {style}, params: [100, 0.64, 0.33, 0.3, 0.6, 0.15, 0.06, 0.3127, 0.329]}}\n"
        ));
        let _log = LogGuard::new();
        assert_err!(
            Config::create_from_str(&format!("{}{end}", profile_start_v(2, 3))),
            &format!("Only config version 2.4 (or higher) can have FixedFunctionTransform style '{style}'.")
        );
    }

    let hsy = group(
        r#"        - !<FixedFunctionTransform> {style: RGB_TO_HSY_LOG}
        - !<FixedFunctionTransform> {style: RGB_TO_HSY_LOG, direction: inverse}
        - !<FixedFunctionTransform> {style: RGB_TO_HSY_LIN}
        - !<FixedFunctionTransform> {style: RGB_TO_HSY_LIN, direction: inverse}
        - !<FixedFunctionTransform> {style: RGB_TO_HSY_VID}
        - !<FixedFunctionTransform> {style: RGB_TO_HSY_VID, direction: inverse}
"#,
    );
    assert_err!(
        Config::create_from_str(&format!("{}{hsy}", profile_start_v(2, 4))),
        "Only config version 2.5 (or higher) can have FixedFunctionTransform style 'RGB_TO_HSY_LOG'."
    );
    check_roundtrip(&format!("{}{hsy}", profile_start_v(2, 5)));
}

#[test]
#[ignore = "needs-merge"]
fn config_fixed_function_validation() {
    let _log = LogGuard::new();
    let cases = [
        (
            v2(&group("        - !<FixedFunctionTransform> {style: ACES_DarkToDim10, params: [0.75]}\n")),
            "The style 'ACES_DarkToDim10 (Forward)' must have zero parameters but 1 found.",
        ),
        (
            format!("{}{}", profile_v21_start(), group("        - !<FixedFunctionTransform> {style: ACES_GamutComp13}\n")),
            "The style 'ACES_GamutComp13 (Forward)' must have seven parameters but 0 found.",
        ),
        (
            v2(&group("        - !<FixedFunctionTransform> {style: REC2100_Surround, direction: inverse}\n")),
            "The style 'REC2100_Surround (Inverse)' must have one parameter but 0 found.",
        ),
        (
            format!(
                "{}{}",
                profile_start_v(2, 4),
                group("        - !<FixedFunctionTransform> {style: Lin_TO_GammaLog, params: [0.0, 0.25, 0.5, 1.0, 0.0, 2.718, 0.17, 0.80, 1.0]}\n")
            ),
            "The style 'Lin_TO_GammaLog' must have 10 parameters but 9 found.",
        ),
        (
            format!(
                "{}{}",
                profile_start_v(2, 4),
                group("        - !<FixedFunctionTransform> {style: Lin_TO_DoubleLog, params: [10.0, 0.25, 0.5, -1.0, 0.0, -1.0, 1.25, 1.0, 1.0, 1.0, 0.5, 1.0]}\n")
            ),
            "The style 'Lin_TO_DoubleLog' must have 13 parameters but 12 found.",
        ),
        (
            format!(
                "{}{}",
                profile_start_v(2, 6),
                group("        - !<FixedFunctionTransform> {style: ACES2_RGB_TO_HMJ, params: [0.64, 0.33, 0.3, 0.6, 0.15, 0.06, 0.3127, 0.329, 0.], direction: inverse}\n")
            ),
            "The style 'HMJ_TO_RGB_20' must have 8 parameters but 9 found.",
        ),
        (
            format!(
                "{}{}",
                profile_start_v(2, 4),
                group("        - !<FixedFunctionTransform> {style: ACES2_OutputTransform, params: []}\n")
            ),
            "The style 'ACES_OutputTransform20 (Forward)' must have 9 parameters but 0 found.",
        ),
        (
            format!(
                "{}{}",
                profile_start_v(2, 4),
                group("        - !<FixedFunctionTransform> {style: ACES2_OutputTransform, params: [-1, 0.64, 0.33, 0.3, 0.6, 0.15, 0.06, 0.3127, 0.329]}\n")
            ),
            "FixedFunctionTransform validation failed: Parameter -1 (peak_luminance) is outside valid range [1,10000]",
        ),
        (
            format!(
                "{}{}",
                profile_start_v(2, 4),
                group("        - !<FixedFunctionTransform> {style: ACES2_OutputTransform, params: [100.5, 0.64, 0.33, 0.3, 0.6, 0.15, 0.06, 0.3127, 0.329]}\n")
            ),
            "FixedFunctionTransform validation failed: Parameter 100.5 (peak_luminance) cannot include any fractional component",
        ),
    ];
    for (s, what) in cases {
        let config = Config::create_from_str(&s).unwrap();
        assert_err!(config.validate(), what);
    }
}

#[test]
fn config_exposure_contrast_serialization() {
    let s = v2(&group(
        r#"        - !<ExposureContrastTransform> {style: video, contrast: 0.5, gamma: 1.1, pivot: 0.18}
        - !<ExposureContrastTransform> {style: video, exposure: 1.5, gamma: 1.1, pivot: 0.18}
        - !<ExposureContrastTransform> {style: video, exposure: 1.5, contrast: 0.5, pivot: 0.18}
        - !<ExposureContrastTransform> {style: video, exposure: 1.5, contrast: 0.5, gamma: 1.1, pivot: 0.18}
        - !<ExposureContrastTransform> {style: video, exposure: 1.5, contrast: 0.5, gamma: 1.1, pivot: 0.18}
        - !<ExposureContrastTransform> {style: video, exposure: -1.4, contrast: 0.6, gamma: 1.2, pivot: 0.2, direction: inverse}
        - !<ExposureContrastTransform> {style: log, exposure: 1.5, contrast: 0.6, gamma: 1.2, pivot: 0.18}
        - !<ExposureContrastTransform> {style: log, exposure: 1.5, contrast: 0.5, gamma: 1.1, pivot: 0.18, direction: inverse}
        - !<ExposureContrastTransform> {style: log, exposure: 1.5, contrast: 0.6, gamma: 1.2, pivot: 0.18}
        - !<ExposureContrastTransform> {style: linear, exposure: 1.5, contrast: 0.5, gamma: 1.1, pivot: 0.18}
        - !<ExposureContrastTransform> {style: linear, exposure: 1.5, contrast: 0.5, gamma: 1.1, pivot: 0.18, direction: inverse}
        - !<ExposureContrastTransform> {style: linear, exposure: 1.5, contrast: 0.5, gamma: 1.1, pivot: 0.18}
"#,
    ));
    let config = check_roundtrip(&s);

    // No value for exposure, contrast or gamma means dynamic.
    let cs = config.get_color_space("lnh").unwrap();
    let grp = match cs.transform(ColorSpaceDirection::FromReference) {
        Some(Transform::Group(g)) => g,
        t => panic!("unexpected {t:?}"),
    };
    assert_eq!(grp.transforms.len(), 12);
    let ec = |i: usize| match &grp.transforms[i] {
        Transform::ExposureContrast(e) => (e.exposure_dynamic, e.contrast_dynamic, e.gamma_dynamic),
        t => panic!("unexpected {t:?}"),
    };
    assert_eq!(ec(0), (true, false, false));
    assert_eq!(ec(1), (false, true, false));
    assert_eq!(ec(2), (false, false, true));

    assert_err!(
        Config::create_from_str(&v2(&group("        - !<ExposureContrastTransform> {style: wrong}\n"))),
        "Unknown exposure contrast style"
    );
}

#[test]
fn config_matrix_serialization() {
    let end = r#"    from_reference: !<GroupTransform>
      children:
        - !<MatrixTransform> {matrix: [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15], offset: [-1, -2, -3, -4]}
        - !<MatrixTransform> {offset: [0.123456789876, 1.23456789876, 12.3456789876, 123.456789876]}
        - !<MatrixTransform> {matrix: [0.123456789876, 1.23456789876, 12.3456789876, 123.456789876, 1234.56789876, 12345.6789876, 123456.789876, 1234567.89876, 0, 0, 1, 0, 0, 0, 0, 1]}
"#;
    check_roundtrip(&format!("{}{end}", simple_profile_v1()));
}

#[test]
fn config_cdl_serialization() {
    check_roundtrip(&v2(&group(
        r#"        - !<CDLTransform> {slope: [1, 2, 1]}
        - !<CDLTransform> {offset: [0.1, 0.2, 0.1]}
        - !<CDLTransform> {power: [1.1, 1.2, 1.1]}
        - !<CDLTransform> {sat: 0.1, direction: inverse}
        - !<CDLTransform> {slope: [2, 2, 3], offset: [0.2, 0.3, 0.1], power: [1.2, 1.1, 1], sat: 0.2, style: asc}
"#,
    )));
    let end = r#"    from_reference: !<GroupTransform>
      children:
        - !<CDLTransform> {slope: [1, 2, 1]}
        - !<CDLTransform> {offset: [0.1, 0.2, 0.1]}
        - !<CDLTransform> {power: [1.1, 1.2, 1.1]}
        - !<CDLTransform> {sat: 0.1}
"#;
    check_roundtrip(&format!("{}{end}", simple_profile_v1()));
}

#[test]
fn config_file_transform_serialization() {
    check_roundtrip(&v2(&group(
        r#"        - !<FileTransform> {src: a.clf}
        - !<FileTransform> {src: b.ccc, cccid: cdl1, interpolation: best}
        - !<FileTransform> {src: b.ccc, cccid: cdl2, cdl_style: asc, interpolation: linear}
        - !<FileTransform> {src: a.clf, direction: inverse}
"#,
    )));
}

#[test]
fn config_file_transform_serialization_v1() {
    let mut cfg = Config::create();
    cfg.set_major_version(1).unwrap();
    let mut ft = FileTransform::new("file");
    let mut cs = ColorSpace::default();
    cs.set_transform(Some(ft.clone().into()), ColorSpaceDirection::ToReference);
    ft.src = "other".into();
    ft.interpolation = Interpolation::Tetrahedral;
    cs.set_transform(Some(ft.into()), ColorSpaceDirection::FromReference);
    cs.set_name("cs");
    cfg.add_color_space(&cs).unwrap();
    assert_eq!(
        cfg.serialize().unwrap(),
        r#"ocio_profile_version: 1

search_path: ""
strictparsing: true
luma: [0.2126, 0.7152, 0.0722]

roles:
  {}

displays:
  {}

active_displays: []
active_views: []

colorspaces:
  - !<ColorSpace>
    name: cs
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    allocation: uniform
    to_reference: !<FileTransform> {src: file, interpolation: linear}
    from_reference: !<FileTransform> {src: other, interpolation: tetrahedral}
"#
    );
}
