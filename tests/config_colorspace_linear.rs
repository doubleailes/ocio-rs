//! Port of the `ColorSpace_tests.cpp` tests `Config/is_colorspace_linear`,
//! `ConfigUtils/processor_to_known_colorspace` and
//! `ConfigUtils/processor_to_known_colorspace_alt_config`.

mod config_common;

use config_common::*;
use ocio::*;

fn linear_config() -> Config {
    let mut config = Config::create_from_str(LINEAR_CONFIG)
        .unwrap()
        .create_editable_copy();
    config.set_search_path(&data_file(""));
    config
}

#[test]
fn config_is_colorspace_linear_validation() {
    let _lock = env_lock();
    let config = linear_config();
    config.validate().unwrap();
    for rt in [ReferenceSpaceType::Scene, ReferenceSpaceType::Display] {
        assert_err!(
            config.is_color_space_linear("colorspace_abc", rt),
            "Could not test colorspace linearity. Colorspace colorspace_abc does not exist"
        );
    }
}

#[test]
fn config_is_colorspace_linear() {
    let _lock = env_lock();
    let config = linear_config();
    config.validate().unwrap();

    let scene = [
        ("display_data", false),
        ("display_linear-enc", false),
        ("display_wrong-linear-enc", false),
        ("display_video-enc", false),
        ("display_linear-trans", false),
        ("display_video-trans", false),
        ("scene_data", false),
        ("scene_linear-enc", true),
        ("scene_wrong-linear-enc", false),
        ("scene_log-enc", false),
        ("scene_linear-trans", true),
        ("scene_nonlin-trans", false),
        ("scene_linear-trans-alias", true),
        ("scene_ref", true),
        ("linear_mtx_from_file", true),
        ("linear_lut3d_from_file", false),
    ];
    for (cs, expected) in scene {
        assert!(config.get_color_space(cs).is_some());
        assert_eq!(
            config
                .is_color_space_linear(cs, ReferenceSpaceType::Scene)
                .unwrap(),
            expected,
            "{cs}"
        );
    }
    let display = [
        ("display_data", false),
        ("display_linear-enc", true),
        ("display_wrong-linear-enc", false),
        ("display_video-enc", false),
        ("display_linear-trans", true),
        ("display_video-trans", false),
        ("scene_data", false),
        ("scene_linear-enc", false),
        ("scene_wrong-linear-enc", false),
        ("scene_log-enc", false),
        ("scene_linear-trans", false),
        ("scene_nonlin-trans", false),
        ("scene_linear-trans-alias", false),
        ("scene_ref", false),
        ("linear_mtx_from_file", false),
        ("linear_lut3d_from_file", false),
    ];
    for (cs, expected) in display {
        assert_eq!(
            config
                .is_color_space_linear(cs, ReferenceSpaceType::Display)
                .unwrap(),
            expected,
            "{cs}"
        );
    }
}

fn known_config() -> Config {
    let mut cfg = Config::create_from_str(KNOWN_CONFIG)
        .unwrap()
        .create_editable_copy();
    cfg.set_search_path(&data_file(""));
    cfg
}

#[test]
fn config_utils_processor_to_known_colorspace_errors() {
    // The error cases which do not need the built-in configs nor the ops.
    let _lock = env_lock();
    let cfg = known_config();
    let raw = Config::create_raw();
    assert_err!(
        Config::identify_interchange_space(&cfg, "Foo", &raw, "raw"),
        "Could not find source color space 'Foo'."
    );
    assert_err!(
        Config::identify_interchange_space(&cfg, "Foo", &raw, ""),
        "Could not find destination color space ''."
    );
}

#[test]
fn config_utils_processor_to_known_colorspace() {
    let _lock = env_lock();
    let mut cfg = known_config();
    let builtin = Config::create_from_file("ocio://default").unwrap();

    cfg.set_inactive_color_spaces("ACES cg, Linear ITU-R BT.709, Texture -- sRGB, OCIO v1 -- sRGB");
    let src = "not sRGB";
    let builtin_name = "Gamma 2.2 AP1 - Texture";

    assert_err!(
        Config::get_processor_to_builtin_color_space(&cfg, src, builtin_name),
        "Heuristics were not able to find a known color space in the provided config. Please set the interchange roles."
    );

    let ref_proc = Config::get_processor_from_configs_interchange(
        &cfg,
        src,
        "ref_cs",
        &builtin,
        builtin_name,
        "ACES2065-1",
    )
    .unwrap();
    for inactive in [
        "ACES cg, Linear ITU-R BT.709, OCIO v1 -- sRGB",
        "ACES cg, Texture -- sRGB, OCIO v1 -- sRGB",
        "Linear ITU-R BT.709, Texture -- sRGB, OCIO v1 -- sRGB",
    ] {
        cfg.set_inactive_color_spaces(inactive);
        let p = Config::get_processor_to_builtin_color_space(&cfg, src, builtin_name).unwrap();
        assert_eq!(ref_proc.cache_id(), p.cache_id(), "{inactive}");
    }

    let inv_ref_proc = Config::get_processor_from_configs_interchange(
        &builtin,
        builtin_name,
        "ACES2065-1",
        &cfg,
        src,
        "ref_cs",
    )
    .unwrap();
    for inactive in [
        "ACES cg, Linear ITU-R BT.709, ref_cs, OCIO v1 -- sRGB",
        "ACES cg, Texture -- sRGB, ref_cs, OCIO v1 -- sRGB",
        "Linear ITU-R BT.709, Texture -- sRGB, ref_cs, OCIO v1 -- sRGB",
    ] {
        cfg.set_inactive_color_spaces(inactive);
        let p = Config::get_processor_from_builtin_color_space(builtin_name, &cfg, src).unwrap();
        assert_eq!(inv_ref_proc.cache_id(), p.cache_id(), "{inactive}");
    }

    // Test IdentifyInterchangeSpace.
    let pair = |a: &str, b: &str| (a.to_string(), b.to_string());
    for inactive in [
        "Linear ITU-R BT.709, Texture -- sRGB, ref_cs, OCIO v1 -- sRGB",
        "ACES cg, Linear ITU-R BT.709, OCIO v1 -- sRGB",
        "ACES cg, Linear ITU-R BT.709, Texture -- sRGB",
    ] {
        cfg.set_inactive_color_spaces(inactive);
        assert_eq!(
            Config::identify_interchange_space(
                &cfg,
                "Linear ITU-R BT.709",
                &builtin,
                "lin_rec709_srgb"
            )
            .unwrap(),
            pair("ref_cs", "ACES2065-1")
        );
    }

    cfg.set_role("aces_interchange", Some("Texture -- sRGB"))
        .unwrap();
    assert_eq!(
        Config::identify_interchange_space(
            &cfg,
            "Linear ITU-R BT.709",
            &builtin,
            "lin_rec709_srgb"
        )
        .unwrap(),
        pair("Texture -- sRGB", "ACES2065-1")
    );
    cfg.set_role("aces_interchange", Some("")).unwrap();

    let raw = Config::create_raw();
    assert_err!(
        Config::identify_interchange_space(&cfg, "raw data", &raw, "raw"),
        "Could not find destination color space 'sRGB - Texture'"
    );

    // Test IdentifyBuiltinColorSpace.
    let id = |cfg: &Config, name: &str| {
        Config::identify_builtin_color_space(cfg, &builtin, name).unwrap()
    };
    cfg.set_inactive_color_spaces("OCIO v1 -- sRGB");
    assert_eq!(id(&cfg, "ACEScg"), "ACES cg");
    assert_eq!(id(&cfg, "sRGB - Texture"), "Texture -- sRGB");
    assert_eq!(id(&cfg, "ACES2065-1"), "ref_cs");

    cfg.set_inactive_color_spaces("Texture -- sRGB");
    assert_eq!(id(&cfg, "sRGB - Texture"), "OCIO v1 -- sRGB");

    cfg.set_inactive_color_spaces("Texture -- sRGB, ref_cs, OCIO v1 -- sRGB");
    assert_eq!(id(&cfg, "Linear Rec.709 (sRGB)"), "Linear ITU-R BT.709");
    assert_eq!(id(&cfg, "ACEScct"), "not sRGB");
    assert_eq!(id(&cfg, "lin_ap1"), "ACES cg");
    assert_eq!(id(&cfg, "Raw"), "raw data");

    assert_err!(
        Config::identify_builtin_color_space(&cfg, &builtin, "sRGB - Display"),
        "The heuristics currently only support scene-referred color spaces. Please set the interchange roles."
    );

    cfg.set_role("cie_xyz_d65_interchange", Some("CIE-XYZ-D65"))
        .unwrap();
    assert_eq!(id(&cfg, "sRGB - Display"), "sRGB - Display CS");
    cfg.set_inactive_color_spaces("CIE-XYZ-D65");
    assert_eq!(id(&cfg, "sRGB - Display"), "sRGB - Display CS");

    cfg.set_role("aces_interchange", Some("ref_cs")).unwrap();
    cfg.set_inactive_color_spaces("ref_cs");
    assert_eq!(id(&cfg, "ACEScg"), "ACES cg");
}

#[test]
fn config_utils_processor_to_known_colorspace_alt_config() {
    let _lock = env_lock();
    let mut cfg = Config::create_from_str(ALT_CONFIG)
        .unwrap()
        .create_editable_copy();
    let mut builtin = Config::create_from_file("ocio://default")
        .unwrap()
        .create_editable_copy();
    builtin.set_inactive_color_spaces(
        "ACES2065-1, ACEScg, Linear Rec.709 (sRGB), Linear P3-D65, Linear Rec.2020, CIE XYZ-D65 - Display-referred, sRGB - Display",
    );

    let id = |cfg: &Config, builtin: &Config, name: &str| {
        Config::identify_builtin_color_space(cfg, builtin, name).unwrap()
    };

    assert_eq!(
        id(&cfg, &builtin, "Linear Rec.2020"),
        "scene-linear Rec.2020"
    );
    assert_eq!(id(&cfg, &builtin, "Linear P3-D65"), "scene-linear P3-D65");

    cfg.set_inactive_color_spaces("ACES2065-1");
    assert_eq!(
        id(&cfg, &builtin, "Linear Rec.709 (sRGB)"),
        "scene-linear Rec.709-sRGB"
    );
    cfg.set_inactive_color_spaces("ACES2065-1, texture sRGB");
    assert_eq!(
        id(&cfg, &builtin, "Linear Rec.709 (sRGB)"),
        "scene-linear Rec.709-sRGB"
    );
    cfg.set_inactive_color_spaces("ACES2065-1");
    assert_eq!(id(&cfg, &builtin, "sRGB - Texture"), "texture sRGB");

    assert_err!(
        Config::identify_builtin_color_space(&cfg, &builtin, "ACES2065-1"),
        "Heuristics were not able to find an equivalent to the requested color space: ACES2065-1."
    );

    cfg.set_role("aces_interchange", Some("ACES2065-1"))
        .unwrap();
    assert_eq!(id(&cfg, &builtin, "sRGB - Texture"), "texture sRGB");
    assert_eq!(
        id(&cfg, &builtin, "Linear Rec.709 (sRGB)"),
        "scene-linear Rec.709-sRGB"
    );
    cfg.set_inactive_color_spaces("");
    assert_eq!(id(&cfg, &builtin, "lin_ap0"), "ACES2065-1");
    cfg.set_role("aces_interchange", Some("")).unwrap();

    assert_err!(
        Config::identify_builtin_color_space(&cfg, &builtin, "sRGB - Display"),
        "The heuristics currently only support scene-referred color spaces. Please set the interchange roles."
    );
    cfg.set_role("cie_xyz_d65_interchange", Some("CIE-XYZ D65"))
        .unwrap();
    assert_eq!(
        builtin.inactive_color_spaces().find("sRGB - Display"),
        Some(107)
    );
    assert_eq!(id(&cfg, &builtin, "sRGB - Display"), "sRGB");
    cfg.set_role("cie_xyz_d65_interchange", Some("")).unwrap();

    assert_err!(
        Config::identify_builtin_color_space(&cfg, &builtin, "does not exist"),
        "Built-in config does not contain the requested color space: does not exist."
    );

    let pair = |a: &str, b: &str| (a.to_string(), b.to_string());
    let cases = [
        (
            "scene-linear Rec.709-sRGB, ACES2065-1",
            "scene-linear Rec.709-sRGB",
            "lin_rec709_srgb",
        ),
        (
            "texture sRGB, scene-linear Rec.709-sRGB, ACES2065-1",
            "lin_p3d65",
            "lin_rec709_srgb",
        ),
        (
            "scene-linear P3-D65, texture sRGB, scene-linear Rec.709-sRGB, ACES2065-1",
            "Raw",
            "lin_rec709_srgb",
        ),
        (
            "scene-linear P3-D65, texture sRGB, scene-linear Rec.709-sRGB",
            "Raw",
            "Raw",
        ),
    ];
    for (inactive, src, dst) in cases {
        cfg.set_inactive_color_spaces(inactive);
        assert_eq!(
            Config::identify_interchange_space(&cfg, src, &builtin, dst).unwrap(),
            pair("scene-linear Rec.709-sRGB", "Linear Rec.709 (sRGB)"),
            "{inactive}"
        );
    }
    assert_err!(
        Config::identify_interchange_space(&cfg, "CIE-XYZ D65", &builtin, "CIE XYZ-D65 - Display-referred"),
        "The heuristics currently only support scene-referred color spaces. Please set the interchange roles."
    );
}

const LINEAR_CONFIG: &str = r#"ocio_profile_version: 2

description: Test config for the isColorSpaceLinear method.

environment:
  {}
search_path: "non_existing_path"
roles:
  aces_interchange: scene_linear-trans
  cie_xyz_d65_interchange: display_linear-enc
  color_timing: scene_linear-trans
  compositing_log: scene_log-enc
  default: display_data
  scene_linear: scene_linear-trans

displays:
  generic display:
    - !<View> {name: Raw, colorspace: scene_data}

# Make a few of the color spaces inactive, this should not affect the result.
inactive_colorspaces: [display_linear-trans, scene_linear-trans]

view_transforms:
  - !<ViewTransform>
    name: view_transform
    from_scene_reference: !<MatrixTransform> {}

# Display-referred color spaces.

display_colorspaces:
  - !<ColorSpace>
    name: display_data
    description: |
      Data space.
      Has a linear transform, which should never happen, but this will be ignored since 
      isdata is true.
    isdata: true
    encoding: data
    from_display_reference: !<MatrixTransform> {matrix: [ 3.240969941905, -1.537383177570, -0.498610760293, 0, -0.969243636281, 1.875967501508, 0.041555057407, 0, 0.055630079697, -0.203976958889, 1.056971514243, 0, 0, 0, 0, 1 ]}

  - !<ColorSpace>
    name: display_linear-enc
    description: |
      Encoding set to display-linear.
      Has a non-existent transform, but this should be ignored since the encoding takes precedence.
    isdata: false
    encoding: display-linear
    from_display_reference: !<FileTransform> {src: does-not-exist.lut}

  - !<ColorSpace>
    name: display_wrong-linear-enc
    description: |
      Encoding set to scene-linear.  This should never happen for a display space, but test it.
    isdata: false
    encoding: scene-linear

  - !<ColorSpace>
    name: display_video-enc
    description: |
      Encoding set to sdr-video.
      Has a linear transform, but this should be ignored since the encoding takes precedence.
    isdata: false
    encoding: sdr-video
    from_display_reference: !<MatrixTransform> {matrix: [ 3.240969941905, -1.537383177570, -0.498610760293, 0, -0.969243636281, 1.875967501508, 0.041555057407, 0, 0.055630079697, -0.203976958889, 1.056971514243, 0, 0, 0, 0, 1 ]}

  - !<ColorSpace>
    name: display_linear-trans
    description: |
      No encoding.  Transform is linear.
    isdata: false
    from_display_reference: !<GroupTransform>
      children:
        - !<MatrixTransform> {matrix: [ 3.240969941905, -1.537383177570, -0.498610760293, 0, -0.969243636281, 1.875967501508, 0.041555057407, 0, 0.055630079697, -0.203976958889, 1.056971514243, 0, 0, 0, 0, 1 ]}
        - !<CDLTransform> {slope: [0.1, 2, 3], style: noclamp}

  - !<ColorSpace>
    name: display_video-trans
    description: |
      No encoding.  Transform is non-linear.
    isdata: false
    from_display_reference: !<BuiltinTransform> {style: DISPLAY - CIE-XYZ-D65_to_sRGB}

# Scene-referred color spaces.

colorspaces:
  - !<ColorSpace>
    name: scene_data
    description: |
      Data space.
      Has a linear transform, which should never happen, but this will be ignored 
      since isdata is true.
    isdata: true
    encoding: data
    from_scene_reference: !<MatrixTransform> {matrix: [ 3.240969941905, -1.537383177570, -0.498610760293, 0, -0.969243636281, 1.875967501508, 0.041555057407, 0, 0.055630079697, -0.203976958889, 1.056971514243, 0, 0, 0, 0, 1 ]}

  - !<ColorSpace>
    name: scene_linear-enc
    description: |
      Encoding set to scene-linear.
      Has a non-linear transform, but this will be ignored since the encoding takes precedence.
    isdata: false
    encoding: scene-linear
    from_scene_reference: !<BuiltinTransform> {style: DISPLAY - CIE-XYZ-D65_to_sRGB}

  - !<ColorSpace>
    name: scene_wrong-linear-enc
    description: |
      Encoding set to display-linear.  This should never happen for a scene space, but test it.
    isdata: false
    encoding: display-linear

  - !<ColorSpace>
    name: scene_log-enc
    description: |
      Encoding set to log.
      Has a linear transform, but this will be ignored since the encoding takes precedence.
    isdata: false
    encoding: log
    from_scene_reference: !<MatrixTransform> {matrix: [ 3.240969941905, -1.537383177570, -0.498610760293, 0, -0.969243636281, 1.875967501508, 0.041555057407, 0, 0.055630079697, -0.203976958889, 1.056971514243, 0, 0, 0, 0, 1 ]}

  - !<ColorSpace>
    name: scene_linear-trans
    aliases: [scene_linear-trans-alias]
    description: |
      No encoding.  Transform is linear.
    isdata: false
    to_scene_reference: !<GroupTransform>
      children:
        - !<BuiltinTransform> {style: UTILITY - ACES-AP0_to_CIE-XYZ-D65_BFD}
        - !<MatrixTransform> {matrix: [ 3.240969941905, -1.537383177570, -0.498610760293, 0, -0.969243636281, 1.875967501508, 0.041555057407, 0, 0.055630079697, -0.203976958889, 1.056971514243, 0, 0, 0, 0, 1 ]}
        - !<MatrixTransform> {matrix: [ 3.240969941905, -1.537383177570, -0.498610760293, 0, -0.969243636281, 1.875967501508, 0.041555057407, 0, 0.055630079697, -0.203976958889, 1.056971514243, 0, 0, 0, 0, 1 ]}

  - !<ColorSpace>
    name: scene_nonlin-trans
    description: |
      No encoding.  Transform is non-linear because it clamps values outside [0,1].
    isdata: false
    to_scene_reference: !<GroupTransform>
      children:
        - !<MatrixTransform> {matrix: [ 3.240969941905, -1.537383177570, -0.498610760293, 0, -0.969243636281, 1.875967501508, 0.041555057407, 0, 0.055630079697, -0.203976958889, 1.056971514243, 0, 0, 0, 0, 1 ]}
        - !<RangeTransform> {min_in_value: 0., min_out_value: 0., max_in_value: 1., max_out_value: 1.}

  - !<ColorSpace>
    name: scene_ref
    description: |
      No encoding.  Considered linear since it is equivalent to the reference space.
    isdata: false

  - !<ColorSpace>
    name: linear_mtx_from_file
    description: This is an identity matrix and therefore linear, but is in an external file.
    isdata: false
    to_scene_reference: !<GroupTransform>
      children:
        - !<FileTransform> {src: clf/matrix_windows.clf, interpolation: linear}

  - !<ColorSpace>
    name: linear_lut3d_from_file
    description: |
      This is a Lut3D which is linear across it's unbounded range but, like all LUTs, clamps 
      outside it's [0,1] domain.  Therefore, when the algorithm inputs [4,4,4], it is no different
      than inputing [1,1,1] and so it is not determined to be linear.
    isdata: false
    to_scene_reference: !<GroupTransform>
      children:
        - !<FileTransform> {src: clf/lut3d_as_matrix.clf, interpolation: linear}
"#;

const KNOWN_CONFIG: &str = r#"
ocio_profile_version: 2

roles:
  default: raw data
  scene_linear: ref_cs

display_colorspaces:
  - !<ColorSpace>
    name: CIE-XYZ-D65
    description: The CIE XYZ (D65) display connection colorspace.
    isdata: false

  - !<ColorSpace>
    name: sRGB - Display CS
    description: Convert CIE XYZ (D65 white) to sRGB (piecewise EOTF)
    isdata: false
    from_display_reference: !<BuiltinTransform> {style: DISPLAY - CIE-XYZ-D65_to_sRGB}

colorspaces:
  # Put a couple of test color space first in the config since the heuristics stop upon success.

  - !<ColorSpace>
    name: File color space AP0 to linear rec.709
    description: Would be useable by the heuristics except for the clamping and the fact that it's a clf.
    isdata: false
    from_scene_reference: !<GroupTransform>
      children:
        - !<GroupTransform>
          children:
            - !<FileTransform> {src: clf/lut3d_as_matrix.clf}

  - !<ColorSpace>
    name: CS Transform color space
    description: Verify that that ColorSpaceTransforms load correctly when running the heuristics.
    isdata: false
    from_scene_reference: !<GroupTransform>
      children:
        - !<ColorSpaceTransform> {src: ref_cs, dst: not sRGB}

  - !<ColorSpace>
    name: raw data
    description: A data colorspace (should not be used).
    isdata: true

  - !<ColorSpace>
    name: ref_cs
    description: The reference colorspace, ACES2065-1.
    isdata: false

  - !<ColorSpace>
    name: not sRGB
    description: A color space that misleadingly has sRGB in the name, even though it's not.
    isdata: false
    to_scene_reference: !<BuiltinTransform> {style: ACEScct_to_ACES2065-1}

  - !<ColorSpace>
    name: sRGB - curve
    description: Just the sRGB gamma curve.  Note - None of the heuristics should be able to use this.
    isdata: false
    from_scene_reference: !<GroupTransform>
      children:
        - !<ExponentWithLinearTransform> {gamma: 2.4, offset: 0.055, direction: inverse}

  - !<ColorSpace>
    name: sRGB - curve 2
    description: Another sRGB curve the heuristics shouldn't use.  NB - the transform uses a matrix.
    isdata: false
    to_scene_reference: !<GroupTransform>
      children:
        - !<FileTransform> {src: sRGB_to_linear.spi1d, interpolation: linear}

  - !<ColorSpace>
    name: pseudo sRGB
    description: Ensure that a gamma 2.2 Rec.709 space is not mistaken for an sRGB space.
    isdata: false
    from_scene_reference: !<GroupTransform>
      children:
        - !<MatrixTransform> {matrix: [2.52168618674388, -1.13413098823972, -0.387555198504164, 0, -0.276479914229922, 1.37271908766826, -0.096239173438334, 0, -0.0153780649660342, -0.152975335867399, 1.16835340083343, 0, 0, 0, 0, 1]}
        - !<ExponentTransform> {value: [2.2, 2.2, 2.2, 1], direction: inverse}

  - !<ColorSpace>
    name: ACES cg
    description: An ACEScg space with an unusual spelling.
    isdata: false
    to_scene_reference: !<BuiltinTransform> {style: ACEScg_to_ACES2065-1}

  - !<ColorSpace>
    name: Linear ITU-R BT.709
    description: A linear Rec.709 space with an unusual spelling.
    isdata: false
    from_scene_reference: !<GroupTransform>
      name: AP0 to Linear Rec.709 (sRGB)
      children:
        - !<MatrixTransform> {matrix: [2.52168618674388, -1.13413098823972, -0.387555198504164, 0, -0.276479914229922, 1.37271908766826, -0.096239173438334, 0, -0.0153780649660342, -0.152975335867399, 1.16835340083343, 0, 0, 0, 0, 1]}

  - !<ColorSpace>
    name: sRGB Encoded AP1 - Texture
    description: Another space with "sRGB" in the name that is not actually an sRGB texture space.
    isdata: false
    from_scene_reference: !<GroupTransform>
      name: AP0 to sRGB Encoded AP1 - Texture
      children:
        - !<MatrixTransform> {matrix: [1.45143931614567, -0.23651074689374, -0.214928569251925, 0, -0.0765537733960206, 1.17622969983357, -0.0996759264375522, 0, 0.00831614842569772, -0.00603244979102102, 0.997716301365323, 0, 0, 0, 0, 1]}
        - !<ExponentWithLinearTransform> {gamma: 2.4, offset: 0.055, direction: inverse}

  - !<ColorSpace>
    name: OCIO v1 -- sRGB
    description: The sRGB texture space from the legacy v1 ACES config. Usable by the heuristics despite the file.
    isdata: false
    from_reference: !<GroupTransform>
      children:
        - !<MatrixTransform> {matrix: [0.952552, 0, 9.36786e-05, 0, 0.343966, 0.728166, -0.0721325, 0, 0, 0, 1.00883, 0, 0, 0, 0, 1]}
        - !<MatrixTransform> {matrix: [3.2096, -1.55743, -0.495805, 0, -0.970989, 1.88517, 0.0394894, 0, 0.0597193, -0.210104, 1.14312, 0, 0, 0, 0, 1]}
        - !<FileTransform> {src: sRGB_to_linear.spi1d, interpolation: linear, direction: inverse}

  - !<ColorSpace>
    name: Texture -- sRGB
    description: An sRGB Texture space, spelled differently than in the built-in config.
    isdata: false
    from_scene_reference: !<GroupTransform>
      name: AP0 to sRGB Rec.709
      children:
        - !<MatrixTransform> {matrix: [2.52168618674388, -1.13413098823972, -0.387555198504164, 0, -0.276479914229922, 1.37271908766826, -0.096239173438334, 0, -0.0153780649660342, -0.152975335867399, 1.16835340083343, 0, 0, 0, 0, 1]}
        - !<ExponentWithLinearTransform> {gamma: 2.4, offset: 0.055, direction: inverse}
"#;

const ALT_CONFIG: &str = r#"
ocio_profile_version: 2

environment: {}

roles:
  default: sRGB
  scene_linear: scene-linear Rec.709-sRGB
  rendering: scene-linear Rec.709-sRGB

file_rules:
  - !<Rule> {name: Default, colorspace: default}

shared_views:
  - !<View> {name: Un-tone-mapped, view_transform: Un-tone-mapped, display_colorspace: <USE_DISPLAY_NAME>}
  - !<View> {name: Raw, colorspace: Raw}

displays:
  sRGB:
    - !<Views> [Un-tone-mapped, Raw]
  Gamma 2.2 / Rec.709:
    - !<Views> [ Un-tone-mapped, Raw]

view_transforms:
  - !<ViewTransform>
    name: Un-tone-mapped
    from_scene_reference: !<MatrixTransform> {matrix: [ 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1 ]}

inactive_colorspaces: [scene-linear Rec.709-sRGB, ACES2065-1]

display_colorspaces:
  - !<ColorSpace>
    name: CIE-XYZ D65
    encoding: display-linear
    isdata: false
    to_display_reference: !<MatrixTransform> {matrix: [ 3.240969941905, -1.537383177570, -0.498610760293, 0, -0.969243636281, 1.875967501508, 0.041555057407, 0, 0.055630079697, -0.203976958889, 1.056971514243, 0, 0, 0, 0, 1 ]}

  - !<ColorSpace>
    name: display-linear Rec.709-sRGB
    description: |
      Display reference space
    isdata: false
    encoding: display-linear

  - !<ColorSpace>
    name: sRGB
    isdata: false
    categories: [ file-io ]
    encoding: sdr-video
    from_display_reference: !<GroupTransform>
      children:
        - !<ExponentWithLinearTransform> {gamma: 2.4, offset: 0.055, direction: inverse}
        - !<RangeTransform> {min_in_value: 0., min_out_value: 0., max_in_value: 1., max_out_value: 1.}

colorspaces:
  - !<ColorSpace>
    name: Raw
    isdata: true
    categories: [ file-io ]
    encoding: data

  - !<ColorSpace>
    name: ACES2065-1
    isdata: false
    encoding: scene-linear
    to_scene_reference: !<MatrixTransform> {matrix: [ 2.521686186744, -1.134130988240, -0.387555198504, 0, -0.276479914230, 1.372719087668, -0.096239173438, 0, -0.015378064966, -0.152975335867, 1.168353400833, 0, 0, 0, 0, 1 ]}

  - !<ColorSpace>
    name: scene-linear Rec.709-sRGB
    description: |
      Scene-linear Rec.709 or sRGB primaries -- ** This is the scene reference space **
    isdata: false
    categories: [ file-io, working-space ]
    encoding: scene-linear

  - !<ColorSpace>
    name: scene-linear P3-D65
    aliases: [lin_p3d65, Utility - Linear - P3-D65]
    isdata: false
    encoding: scene-linear
    to_scene_reference: !<MatrixTransform> {matrix: [ 1.224940176281e+00, -2.249401762806e-01, 0, 0, -4.205695470969e-02,  1.042056954710e+00, 0, 0, -1.963755459033e-02, -7.863604555063e-02,  1.098273600141e+00, 0, 0, 0, 0, 1 ]}

  - !<ColorSpace>
    name: scene-linear Rec.2020
    isdata: false
    encoding: scene-linear
    from_scene_reference: !<MatrixTransform> {matrix: [ 0.627403895935, 0.329283038378, 0.043313065687, 0 , 0.069097289358, 0.919540395075, 0.011362315566, 0, 0.016391438875, 0.088013307877, 0.895595253248, 0, 0, 0, 0, 1 ]}

  - !<ColorSpace>
    name: texture sRGB
    isdata: false
    categories: [ file-io ]
    encoding: sdr-video
    from_scene_reference: !<GroupTransform>
      children:
        - !<ExponentWithLinearTransform> {gamma: 2.4, offset: 0.055, direction: inverse}
        - !<RangeTransform> {min_in_value: 0., min_out_value: 0., max_in_value: 1., max_out_value: 1.}
"#;
