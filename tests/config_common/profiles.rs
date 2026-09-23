//! Config fragments shared by the `Config_tests.cpp` ports (see the anonymous
//! namespace in the C++ file).
#![allow(dead_code)]

pub const PROFILE_V1: &str = "ocio_profile_version: 1\n\n";

pub const PROFILE_V2: &str = "ocio_profile_version: 2\n\nenvironment:\n  {}\n";

pub const PROFILE_V21: &str = "ocio_profile_version: 2.1\n\nenvironment:\n  {}\n";

/// Port of the `PROFILE_V<Major, Minor>()` template.
pub fn profile_v(major: u32, minor: u32) -> String {
    let mut s = format!("ocio_profile_version: {major}.{minor}\n");
    if major >= 2 {
        s.push_str("\nenvironment:\n  {}\n");
    }
    s
}

pub const SIMPLE_PROFILE_A: &str = r#"search_path: luts
strictparsing: true
luma: [0.2126, 0.7152, 0.0722]

roles:
  default: raw
  scene_linear: lnh

"#;

pub const SIMPLE_PROFILE_B: &str = r#"search_path: luts
strictparsing: true
luma: [0.2126, 0.7152, 0.0722]

roles:
  aces_interchange: lnh
  color_timing: log
  compositing_log: log
  default: raw
  scene_linear: lnh

"#;

pub const SIMPLE_PROFILE_DISPLAYS_LOOKS: &str = r#"displays:
  sRGB:
    - !<View> {name: RawView, colorspace: raw}
    - !<View> {name: LnhView, colorspace: lnh, looks: beauty}

active_displays: []
active_views: []

looks:
  - !<Look>
    name: beauty
    process_space: lnh
    transform: !<CDLTransform> {slope: [1, 2, 1]}

"#;

pub const SIMPLE_PROFILE_CS_V1: &str = r#"
colorspaces:
  - !<ColorSpace>
    name: raw
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    allocation: uniform

  - !<ColorSpace>
    name: log
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    allocation: uniform
    from_reference: !<LogTransform> {base: 10}

  - !<ColorSpace>
    name: lnh
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    allocation: uniform
"#;

pub const SIMPLE_PROFILE_CS_V2: &str = r#"
colorspaces:
  - !<ColorSpace>
    name: raw
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    allocation: uniform

  - !<ColorSpace>
    name: log
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    allocation: uniform
    from_scene_reference: !<LogTransform> {base: 10}

  - !<ColorSpace>
    name: lnh
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    allocation: uniform
"#;

pub const DEFAULT_RULES: &str = "file_rules:\n  - !<Rule> {name: Default, colorspace: default}\n\n";

pub fn simple_profile_b_v1() -> String {
    format!("{SIMPLE_PROFILE_DISPLAYS_LOOKS}{SIMPLE_PROFILE_CS_V1}")
}

pub fn simple_profile_b_v2() -> String {
    format!("{SIMPLE_PROFILE_DISPLAYS_LOOKS}{SIMPLE_PROFILE_CS_V2}")
}

pub fn profile_v2_start() -> String {
    format!("{PROFILE_V2}{SIMPLE_PROFILE_A}{DEFAULT_RULES}{}", simple_profile_b_v2())
}

pub fn profile_v21_start() -> String {
    format!("{PROFILE_V21}{SIMPLE_PROFILE_A}{DEFAULT_RULES}{}", simple_profile_b_v2())
}

/// `PROFILE_V1 + SIMPLE_PROFILE_A + SIMPLE_PROFILE_B_V1`.
pub fn simple_profile_v1() -> String {
    format!("{PROFILE_V1}{SIMPLE_PROFILE_A}{}", simple_profile_b_v1())
}

/// Port of the `PROFILE_START_V<Major, Minor>()` template.
pub fn profile_start_v(major: u32, minor: u32) -> String {
    if major <= 1 {
        return format!("{}{SIMPLE_PROFILE_A}{}", profile_v(major, minor), simple_profile_b_v1());
    }
    format!("{}{SIMPLE_PROFILE_B}{DEFAULT_RULES}{}", profile_v(major, minor), simple_profile_b_v2())
}
