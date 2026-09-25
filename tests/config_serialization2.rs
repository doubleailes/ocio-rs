//! Port of `Config_tests.cpp` (part 5: log and grading transforms
//! serialization, parser errors).

mod config_common;

use config_common::profiles::*;
use config_common::*;
use ocio::config::logging::LogGuard;
use ocio::*;

fn v2(end: &str) -> String {
    format!("{}{end}", profile_v2_start())
}

fn v1(end: &str) -> String {
    format!("{}{end}", simple_profile_v1())
}

#[test]
fn config_log_serialization() {
    // Log with default base value (saved in V1) and default direction.
    check_roundtrip(&v1("    from_reference: !<LogTransform> {base: 2}\n"));
    // Log with default base value (not saved in V2) and default direction.
    check_roundtrip(&v2("    from_scene_reference: !<LogTransform> {}\n"));
    check_roundtrip(&v1(
        "    from_reference: !<LogTransform> {base: 2, direction: inverse}\n",
    ));
    check_roundtrip(&v2(
        "    from_scene_reference: !<LogTransform> {direction: inverse}\n",
    ));
    check_roundtrip(&v1("    from_reference: !<LogTransform> {base: 5}\n"));
    check_roundtrip(&v1(
        "    from_reference: !<LogTransform> {base: 7, direction: inverse}\n",
    ));

    for end in [
        "    from_scene_reference: !<LogAffineTransform> {base: 10, log_side_slope: [1.3, 1.4, 1.5], log_side_offset: [0, 0, 0.1], lin_side_slope: [1, 1, 1.1], lin_side_offset: [0.1234567890123, 0.5, 0.1]}\n",
        "    from_scene_reference: !<LogAffineTransform> {log_side_slope: [1, 1, 1.1], log_side_offset: [0.1234567890123, 0.5, 0.1], lin_side_slope: [1.3, 1.4, 1.5], lin_side_offset: [0, 0, 0.1]}\n",
        "    from_scene_reference: !<LogAffineTransform> {base: 10, log_side_slope: [1, 1, 1.1], log_side_offset: [0.1234567890123, 0.5, 0.1], lin_side_slope: [1.3, 1.4, 1.5], lin_side_offset: 0.5}\n",
        "    from_scene_reference: !<LogAffineTransform> {log_side_slope: [1, 1, 1.1], lin_side_slope: 1.3, lin_side_offset: [0, 0, 0.1]}\n",
        "    from_scene_reference: !<LogAffineTransform> {log_side_slope: [1, 1, 1.1], log_side_offset: 0.5, lin_side_slope: [1.3, 1, 1], lin_side_offset: [0, 0, 0.1]}\n",
        "    from_scene_reference: !<LogAffineTransform> {log_side_slope: 1.1, log_side_offset: [0.5, 0, 0], lin_side_slope: [1.3, 1, 1], lin_side_offset: [0, 0, 0.1]}\n",
        "    from_scene_reference: !<LogAffineTransform> {log_side_offset: [0.1234567890123, 0.5, 0.1], lin_side_slope: [1.3, 1.4, 1.5], lin_side_offset: [0.1, 0, 0]}\n",
        "    from_scene_reference: !<LogAffineTransform> {base: 10}\n",
        "    from_scene_reference: !<LogCameraTransform> {log_side_slope: [1, 1, 1.1], log_side_offset: [0.1234567890123, 0.5, 0.1], lin_side_slope: [1.3, 1.4, 1.5], lin_side_offset: [0, 0, 0.1], lin_side_break: [0.1, 0.2, 0.3]}\n",
        "    from_scene_reference: !<LogCameraTransform> {lin_side_break: 0.2}\n",
        "    from_scene_reference: !<LogCameraTransform> {lin_side_break: 0.2, linear_slope: [1.1, 0.9, 1.2]}\n",
    ] {
        check_roundtrip(&v2(end));
    }

    let cases = [
        (
            "    from_scene_reference: !<LogAffineTransform> {log_side_slope: [1, 1], log_side_offset: [0.1234567890123, 0.5, 0.1]}\n",
            "log_side_slope value field must have 3 components",
        ),
        (
            "    from_scene_reference: !<LogAffineTransform> {base: [2, 2, 2], log_side_offset: [0.1234567890123, 0.5, 0.1]}\n",
            "base must be a single double",
        ),
        (
            "    from_scene_reference: !<LogCameraTransform> {base: 5}\n",
            "lin_side_break values are missing",
        ),
    ];
    for (end, what) in cases {
        assert_err!(Config::create_from_str(&v2(end)), what);
    }
}

#[test]
fn config_key_value_error() {
    const PROFILE: &str = r#"ocio_profile_version: 2
strictparsing: false
roles:
  default: raw
displays:
  sRGB:
  - !<View> {name: Raw, colorspace: raw}

colorspaces:
  - !<ColorSpace>
    name: raw
    to_scene_reference: !<MatrixTransform>
                      {
                           matrix: [1, 0, 0, 0, 0, 1]
                      }
    allocation: uniform

"#;
    assert_err!(
        Config::create_from_str(PROFILE),
        "Error: Loading the OCIO profile failed. At line 14, the value parsing of the key 'matrix' from 'MatrixTransform' failed: 'matrix' values must be 16 numbers. Found '6'."
    );
}

#[test]
fn config_unknown_key_error() {
    let s = format!("{}    dummyKey: dummyValue\n", profile_v2_start());
    let g = LogGuard::new();
    Config::create_from_str(&s).unwrap();
    assert!(
        g.output().starts_with(
            "[OpenColorIO Warning]: At line 56, unknown key 'dummyKey' in 'ColorSpace'."
        ),
        "{}",
        g.output()
    );
}

#[test]
fn config_grading_primary_serialization() {
    check_roundtrip_to(
        &v2(r#"    from_scene_reference: !<GroupTransform>
      children:
        - !<GradingPrimaryTransform> {style: log}
        - !<GradingPrimaryTransform> {style: log, contrast: {rgb: [1.1, 1, 1], master: 1.1}}
        - !<GradingPrimaryTransform> {style: log, direction: inverse}
        - !<GradingPrimaryTransform> {style: linear, saturation: 0.9}
        - !<GradingPrimaryTransform> {style: linear, saturation: 1.1, direction: inverse}
        - !<GradingPrimaryTransform> {name: test, style: video}
        - !<GradingPrimaryTransform> {style: video, direction: inverse}
"#),
        &v2(r#"    from_scene_reference: !<GroupTransform>
      children:
        - !<GradingPrimaryTransform> {style: log}
        - !<GradingPrimaryTransform>
          style: log
          contrast: {rgb: [1.1, 1, 1], master: 1.1}
          pivot: {contrast: -0.2}
        - !<GradingPrimaryTransform> {style: log, direction: inverse}
        - !<GradingPrimaryTransform>
          style: linear
          saturation: 0.9
        - !<GradingPrimaryTransform>
          style: linear
          saturation: 1.1
          direction: inverse
        - !<GradingPrimaryTransform> {name: test, style: video}
        - !<GradingPrimaryTransform> {style: video, direction: inverse}
"#),
    );

    check_roundtrip(&v2(r#"    from_scene_reference: !<GroupTransform>
      children:
        - !<GradingPrimaryTransform>
          style: log
          brightness: {rgb: [0.1, 0.12345678, 0], master: 0.1}
        - !<GradingPrimaryTransform>
          style: log
          contrast: {rgb: [1.1, 1, 1], master: 1.1}
          pivot: {contrast: -0.2}
        - !<GradingPrimaryTransform>
          style: log
          gamma: {rgb: [1.1, 1.1, 1], master: 1.1}
        - !<GradingPrimaryTransform>
          style: log
          saturation: 0.9
        - !<GradingPrimaryTransform>
          style: log
          pivot: {contrast: -0.1, black: 0.1, white: 1.1}
        - !<GradingPrimaryTransform>
          style: log
          pivot: {black: 0.1, white: 1.1}
        - !<GradingPrimaryTransform>
          style: log
          pivot: {black: 0.1}
        - !<GradingPrimaryTransform>
          style: log
          clamp: {black: 0.1, white: 1.1}
        - !<GradingPrimaryTransform>
          style: log
          clamp: {black: 0.1}
        - !<GradingPrimaryTransform>
          style: linear
          offset: {rgb: [0.1, 0.12345678, 0], master: 0.1}
        - !<GradingPrimaryTransform>
          style: linear
          contrast: {rgb: [1.1, 1, 1], master: 1.1}
          pivot: {contrast: 0.18}
        - !<GradingPrimaryTransform>
          style: linear
          exposure: {rgb: [-1.1, 0.9, -0.01], master: 1.1}
        - !<GradingPrimaryTransform>
          style: linear
          saturation: 0.9
        - !<GradingPrimaryTransform>
          style: linear
          pivot: {contrast: -0.1}
        - !<GradingPrimaryTransform>
          style: linear
          clamp: {black: 0.1, white: 1.1}
        - !<GradingPrimaryTransform>
          style: linear
          clamp: {white: 1.1}
        - !<GradingPrimaryTransform>
          style: video
          offset: {rgb: [0.1, 0.12345678, 0], master: 0.1}
        - !<GradingPrimaryTransform>
          style: video
          gain: {rgb: [1.1, 1, 1], master: 1.1}
        - !<GradingPrimaryTransform>
          style: video
          gamma: {rgb: [1.1, 1, 1], master: 1.1}
        - !<GradingPrimaryTransform>
          style: video
          lift: {rgb: [0.1, 0.12345678, 0], master: 0.1}
        - !<GradingPrimaryTransform>
          style: video
          pivot: {black: 0.1, white: 1.1}
        - !<GradingPrimaryTransform>
          style: video
          pivot: {white: 1.1}
        - !<GradingPrimaryTransform>
          style: video
          clamp: {black: 0.1, white: 1.1}
        - !<GradingPrimaryTransform>
          style: video
          clamp: {black: 0.1}
"#));

    // Primary can be on one line or multiple lines (but is written on multiple lines).
    check_roundtrip_to(
        &v2(r#"    from_scene_reference: !<GroupTransform>
      children:
        - !<GradingPrimaryTransform> {style: log, brightness: {rgb: [0.1, 0.12345678, 0], master: 0.1}, pivot: {contrast: -0.2}}
        - !<GradingPrimaryTransform>
          style: linear
          offset:
            rgb: [0.1, 0.12345678, 0]
            master: 0.1
          pivot: {contrast: 0.18}
"#),
        &v2(r#"    from_scene_reference: !<GroupTransform>
      children:
        - !<GradingPrimaryTransform>
          style: log
          brightness: {rgb: [0.1, 0.12345678, 0], master: 0.1}
        - !<GradingPrimaryTransform>
          style: linear
          offset: {rgb: [0.1, 0.12345678, 0], master: 0.1}
"#),
    );

    let cases = [
        (
            "{style: log, brightness: {rgb: [0.1, 0], master: 0.1}}",
            "The RGB value needs to be a 3 doubles",
        ),
        (
            "{style: log, brightness: {rgb: [0.1, 0.12345678, 0, 0], master: 0.1}}",
            "The RGB value needs to be a 3 doubles",
        ),
        (
            "{style: log, brightness: [0.1, 0.12345678, 0, 0]}",
            "'brightness' failed: The value needs to be a map",
        ),
        (
            "{style: log, brightness: {rgb: [0.1, 0.12345678, 0]}}",
            "'brightness' failed: Both rgb and master values are required",
        ),
        (
            "{style: log, brightness: {rgb: [0.1, 0.12345678, 0], master: [0.1, 0.2, 0.3]}}",
            "parsing double failed",
        ),
        (
            "{style: log, brightness: {master: 0.1}}",
            "'brightness' failed: Both rgb and master values are required",
        ),
        (
            "{style: log, pivot: 0.1}",
            "'pivot' failed: The value needs to be a map",
        ),
        (
            "{style: log, pivot: {}}",
            "'pivot' failed: At least one of the pivot values must be provided",
        ),
        (
            "{style: log, clamp: 0.1}",
            "'clamp' failed: The value needs to be a map",
        ),
        (
            "{style: log, clamp: {}}",
            "'clamp' failed: At least one of the clamp values must be provided",
        ),
    ];
    for (t, what) in cases {
        let s = v2(&format!(
            "    from_scene_reference: !<GradingPrimaryTransform> {t}\n"
        ));
        assert_err!(Config::create_from_str(&s), what);
    }
}

#[test]
fn config_grading_rgbcurve_serialization() {
    check_roundtrip(&v2(r#"    from_scene_reference: !<GroupTransform>
      children:
        - !<GradingRGBCurveTransform> {style: log}
        - !<GradingRGBCurveTransform> {style: log, direction: inverse}
        - !<GradingRGBCurveTransform> {style: linear, lintolog_bypass: true}
        - !<GradingRGBCurveTransform> {style: linear, direction: inverse}
        - !<GradingRGBCurveTransform> {name: test, style: video}
        - !<GradingRGBCurveTransform> {style: video, direction: inverse}
"#));
    check_roundtrip(&v2(r#"    from_scene_reference: !<GroupTransform>
      children:
        - !<GradingRGBCurveTransform>
          style: log
          red: {control_points: [0, 0, 0.5, 0.5, 1, 1.123456]}
        - !<GradingRGBCurveTransform>
          style: log
          red: {control_points: [0, 0, 0.5, 0.5, 1, 1.5]}
          green: {control_points: [-1, -1, 0, 0.1, 0.5, 0.6, 1, 1.1]}
          direction: inverse
        - !<GradingRGBCurveTransform>
          style: linear
          lintolog_bypass: true
          red: {control_points: [0, 0, 0.1, 0.2, 0.5, 0.5, 0.7, 0.6, 1, 1.5]}
          master: {control_points: [-1, -1, 0, 0.1, 0.5, 0.6, 1, 1.1]}
        - !<GradingRGBCurveTransform>
          style: video
          red: {control_points: [-0.2, 0, 0.5, 0.5, 1.2, 1.5]}
          green: {control_points: [0, 0, 0.2, 0.5, 1, 1.5]}
          blue: {control_points: [0, 0, 0.1, 0.5, 1, 1.5], slopes: [0, 1, 1.1]}
          master: {control_points: [-1, -1, 0, 0.1, 0.5, 0.6, 1, 1.1]}
          direction: inverse
"#));
    assert_err!(
        Config::create_from_str(&v2(r#"    from_reference: !<GroupTransform>
      children:
        - !<GradingRGBCurveTransform>
          style: log
          blue: {control_points: [0, 0, 0.1, 0.5, 1, 1.5], slopes: [0, 1, 1.1, 1]}
"#)),
        "Number of slopes must match number of control points"
    );
}

#[test]
fn config_grading_huecurve_serialization() {
    let v25 = |end: &str| format!("{}{end}", profile_start_v(2, 5));
    check_roundtrip(&v25(r#"    from_scene_reference: !<GroupTransform>
      children:
        - !<GradingHueCurveTransform> {style: log}
        - !<GradingHueCurveTransform> {style: log, direction: inverse}
        - !<GradingHueCurveTransform> {style: linear}
        - !<GradingHueCurveTransform> {style: linear, direction: inverse}
        - !<GradingHueCurveTransform> {name: test, style: video}
        - !<GradingHueCurveTransform> {style: video, direction: inverse}
"#));
    check_roundtrip(&v25(r#"    from_scene_reference: !<GroupTransform>
      children:
        - !<GradingHueCurveTransform>
          style: log
          hue_hue: {control_points: [0, 0.15, 0.5, 0.5, 1, 1.123456]}
        - !<GradingHueCurveTransform>
          style: log
          hue_sat: {control_points: [0, 0, 0.5, 0.5, 1, 1.5]}
          lum_lum: {control_points: [-1, -1, 0, 0.1, 0.5, 0.6, 1, 1.1]}
          direction: inverse
        - !<GradingHueCurveTransform>
          style: linear
          hsy_transform: none
          sat_sat: {control_points: [0, 0, 0.1, 0.2, 0.5, 0.5, 0.7, 0.6, 1, 1.5]}
          lum_lum: {control_points: [-1, -1, 0, 0.1, 0.5, 0.6, 1, 1.1]}
        - !<GradingHueCurveTransform>
          style: video
          hue_hue: {control_points: [0.02, -0.1, 0.5, 0.5, 0.9, 0.8]}
          hue_lum: {control_points: [0, 0, 0.2, 0.5, 1, 1.5]}
          lum_sat: {control_points: [0, 0, 0.1, 0.5, 1, 1.5], slopes: [0, 1, 1.1]}
          sat_lum: {control_points: [-1, -1, 0, 0.1, 0.5, 0.6, 1, 1.1]}
          direction: inverse
"#));
    assert_err!(
        Config::create_from_str(&format!(
            "{}    from_reference: !<GroupTransform>\n      children:\n        - !<GradingHueCurveTransform> {{style: log}}\n",
            profile_start_v(2, 4)
        )),
        "Only config version 2.5 (or higher) can have GradingHueCurveTransform"
    );
    assert_err!(
        Config::create_from_str(&v25(r#"    from_reference: !<GroupTransform>
      children:
        - !<GradingHueCurveTransform>
          style: log
          sat_sat: {control_points: [0, 0, 0.1, 0.5, 1, 1.5], slopes: [0, 1, 1.1, 1]}
"#)),
        "Number of slopes must match number of control points"
    );
    assert_err!(
        Config::create_from_str(&v25(
            "    from_reference: !<GroupTransform>\n      children:\n        - !<GradingHueCurveTransform> {style: linear, hsy_transform: hsy1}\n"
        )),
        "Unknown hsy_transform value"
    );
}

#[test]
fn config_grading_tone_serialization() {
    check_roundtrip_to(
        &v2(r#"    from_scene_reference: !<GroupTransform>
      children:
        - !<GradingToneTransform> {style: log}
        - !<GradingToneTransform> {style: log, s_contrast: 1.1}
        - !<GradingToneTransform> {style: log, direction: inverse}
        - !<GradingToneTransform> {style: linear}
        - !<GradingToneTransform> {style: linear, direction: inverse}
        - !<GradingToneTransform> {name: test, style: video}
        - !<GradingToneTransform> {style: video, direction: inverse}
"#),
        &v2(r#"    from_scene_reference: !<GroupTransform>
      children:
        - !<GradingToneTransform> {style: log}
        - !<GradingToneTransform>
          style: log
          s_contrast: 1.1
        - !<GradingToneTransform> {style: log, direction: inverse}
        - !<GradingToneTransform> {style: linear}
        - !<GradingToneTransform> {style: linear, direction: inverse}
        - !<GradingToneTransform> {name: test, style: video}
        - !<GradingToneTransform> {style: video, direction: inverse}
"#),
    );
    check_roundtrip(&v2(r#"    from_scene_reference: !<GroupTransform>
      children:
        - !<GradingToneTransform>
          style: log
          blacks: {rgb: [0.1, 0.12345678, 0.9], master: 1, start: 0.1, width: 0.9}
          shadows: {rgb: [1, 1.1, 1.1111], master: 1.1, start: 0.9, pivot: 0.1}
          midtones: {rgb: [0.85, 0.98, 1], master: 1.11, center: 0.1, width: 0.9}
          highlights: {rgb: [1.1, 1.1111, 1], master: 1.2, start: 0.15, pivot: 1.1}
          whites: {rgb: [0.95, 0.96, 0.95], master: 1.1, start: 0.1, width: 0.9}
          s_contrast: 1.1
        - !<GradingToneTransform>
          style: log
          midtones: {rgb: [0.85, 0.98, 1], master: 1.11, center: 0.1, width: 0.9}
          highlights: {rgb: [1.1, 1.1111, 1], master: 1.2, start: 0.15, pivot: 1.1}
          whites: {rgb: [0.95, 0.96, 0.95], master: 1.1, start: 0.1, width: 0.9}
          s_contrast: 1.1
        - !<GradingToneTransform>
          style: linear
          blacks: {rgb: [0.1, 0.12345678, 0.9], master: 1, start: 0.1, width: 0.9}
          shadows: {rgb: [1, 1.1, 1.1111], master: 1.1, start: 0.9, pivot: 0.1}
          whites: {rgb: [0.95, 0.96, 0.95], master: 1.1, start: 0.1, width: 0.9}
          s_contrast: 1.1
        - !<GradingToneTransform>
          style: video
          shadows: {rgb: [1, 1.1, 1.1111], master: 1.1, start: 0.9, pivot: 0.1}
          midtones: {rgb: [0.85, 0.98, 1], master: 1.11, center: 0.1, width: 0.9}
          highlights: {rgb: [1.1, 1.1111, 1], master: 1.2, start: 0.15, pivot: 1.1}
          direction: inverse
"#));

    let cases = [
        (
            "{style: log, whites: {rgb: [0.1, 1], master: 1, start: 1, width: 1}}",
            "The RGB value needs to be a 3 doubles",
        ),
        (
            "{style: log, whites: {rgb: [0.1, 0.12345678, 1, 1], master: 0.1, start: 1, width: 1}}",
            "The RGB value needs to be a 3 doubles",
        ),
        (
            "{style: log, whites: [0.1, 0.12345678, 0, 0]}",
            "'whites' failed: The value needs to be a map",
        ),
        (
            "{style: log, whites: {rgb: [0.1, 1, 1], master: 0.1, width: 1}}",
            "'whites' failed: Rgb, master, start, and width values are required",
        ),
        (
            "{style: log, midtones: {rgb: [0.1, 1, 1], master: 0.1, width: 1}}",
            "'midtones' failed: Rgb, master, center, and width values are required",
        ),
        (
            "{style: log, whites: {rgb: [0.1, 1, 1], master: 0.1, start: [1, 1.1], width: 1}}",
            "parsing double failed",
        ),
    ];
    for (t, what) in cases {
        let s = v2(&format!(
            "    from_scene_reference: !<GradingToneTransform> {t}\n"
        ));
        assert_err!(Config::create_from_str(&s), what);
    }
}
