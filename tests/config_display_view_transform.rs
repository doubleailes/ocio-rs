//! Port of `transforms/DisplayViewTransform_tests.cpp`.

mod config_common;

use config_common::*;
use ocio::config::{collect_context_variables, ColorSpace, ViewTransform};
use ocio::ops::{OpRc, OpVec};
use ocio::transforms::BuildOps;
use ocio::*;

#[test]
fn display_view_transform_basic() {
    let mut dt = DisplayViewTransform::default();
    assert_eq!(dt.direction, TransformDirection::Forward);
    assert_eq!(dt.src, "");
    assert_eq!(dt.display, "");
    assert_eq!(dt.view, "");
    assert!(!dt.looks_bypass);
    assert!(dt.data_bypass);

    dt.src = "inputCS".into();
    dt.display = "display".into();
    dt.view = "view".into();
    Transform::from(dt.clone()).validate().unwrap();

    dt.direction = TransformDirection::Inverse;

    let mut t = dt.clone();
    t.src.clear();
    assert_err!(Transform::from(t).validate(), "DisplayViewTransform: empty source color space name");
    let mut t = dt.clone();
    t.display.clear();
    assert_err!(Transform::from(t).validate(), "DisplayViewTransform: empty display name");
    let mut t = dt.clone();
    t.view.clear();
    assert_err!(Transform::from(t).validate(), "DisplayViewTransform: empty view name");
    Transform::from(dt.clone()).validate().unwrap();

    dt.looks_bypass = true;
    dt.data_bypass = false;
    let copy = dt.clone();
    assert_eq!(copy.src, "inputCS");
    assert_eq!(copy.display, "display");
    assert_eq!(copy.view, "view");
    assert_eq!(copy.direction, TransformDirection::Inverse);
    assert!(copy.looks_bypass);
    assert!(!copy.data_bypass);
}

fn build(config: &Config, dt: &DisplayViewTransform, dir: TransformDirection) -> Result<OpVec> {
    let mut ops = OpVec::new();
    dt.build_ops(&mut ops, config, config.current_context(), dir)?;
    Ok(ops)
}

/// Short description of the kind of an op (the C++ tests check the op data
/// type).
fn kind(op: &OpRc) -> &'static str {
    if op.is_no_op() {
        return "noop";
    }
    match op.to_transform() {
        Some(Transform::Matrix(_)) => "matrix",
        Some(Transform::FixedFunction(_)) => "ff",
        Some(Transform::Log(_)) => "log",
        Some(Transform::ExposureContrast(_)) => "ec",
        Some(Transform::Cdl(_)) => "cdl",
        Some(Transform::Exponent(_)) | Some(Transform::ExponentWithLinear(_)) => "gamma",
        Some(Transform::Range(_)) => "range",
        _ => "unknown",
    }
}

fn kinds(ops: &OpVec) -> Vec<&'static str> {
    ops.iter().map(kind).collect()
}

fn offset_of(op: &OpRc) -> [f64; 4] {
    match op.to_transform() {
        Some(Transform::Matrix(m)) => m.offset,
        t => panic!("expected a matrix, got {t:?}"),
    }
}

fn log_base(op: &OpRc) -> f64 {
    match op.to_transform() {
        Some(Transform::Log(l)) => l.base,
        t => panic!("expected a log, got {t:?}"),
    }
}

const OFFSET: [f64; 4] = [0.0, 0.1, 0.2, 0.0];

#[test]
#[ignore = "needs-merge"]
fn display_view_transform_build_ops() {
    use TransformDirection::Forward;
    let mut config = Config::create_raw().create_editable_copy();
    let mut cs_source = ColorSpace::default();
    cs_source.set_name("source");
    cs_source.set_transform(
        Some(MatrixTransform { offset: OFFSET, ..Default::default() }.into()),
        ColorSpaceDirection::ToReference,
    );
    config.add_color_space(&cs_source).unwrap();

    let mut cs = ColorSpace::default();
    cs.set_name("destination");
    cs.set_transform(
        Some(FixedFunctionTransform::new(FixedFunctionStyle::AcesGlow03, &[]).into()),
        ColorSpaceDirection::FromReference,
    );
    config.add_color_space(&cs).unwrap();
    config.add_display_view("display", "view", "destination", "").unwrap();
    config.validate().unwrap();

    let mut dt = DisplayViewTransform::new("source", "display", "view");
    {
        let ops = build(&config, &dt, Forward).unwrap();
        assert_eq!(kinds(&ops), ["noop", "matrix", "ff", "noop"]);
        assert_eq!(offset_of(&ops[1]), OFFSET);
    }

    // Using a scene-referred ViewTransform.
    let mut cs = ColorSpace::new(ReferenceSpaceType::Display);
    cs.set_name("display");
    cs.set_transform(Some(ExposureContrastTransform::default().into()), ColorSpaceDirection::FromReference);
    config.add_color_space(&cs).unwrap();

    let mut vt = ViewTransform::new(ReferenceSpaceType::Scene);
    vt.set_name("default_vt");
    let cdl = CdlTransform { sat: 1.2, ..Default::default() };
    vt.set_transform(Some(cdl.into()), ViewTransformDirection::FromReference);
    config.add_view_transform(&vt).unwrap();

    let mut vt = ViewTransform::new(ReferenceSpaceType::Scene);
    vt.set_name("scene_vt");
    vt.set_transform(Some(LogTransform::new(4.2).into()), ViewTransformDirection::FromReference);
    config.add_view_transform(&vt).unwrap();

    config
        .add_display_view_full("display", "viewt", "scene_vt", "display", "", "", "")
        .unwrap();
    config.validate().unwrap();
    dt.view = "viewt".into();
    {
        let ops = build(&config, &dt, Forward).unwrap();
        assert_eq!(kinds(&ops), ["noop", "matrix", "log", "ec", "noop"]);
        assert_eq!(offset_of(&ops[1]), OFFSET);
        assert_eq!(log_base(&ops[2]), 4.2);
    }

    // Adding a display-referred ViewTransform.
    let mut vt = ViewTransform::new(ReferenceSpaceType::Display);
    vt.set_name("display_vt");
    vt.set_transform(Some(LogTransform::new(2.1).into()), ViewTransformDirection::FromReference);
    config.add_view_transform(&vt).unwrap();
    config
        .add_display_view_full("display", "viewt", "display_vt", "display", "", "", "")
        .unwrap();
    config.validate().unwrap();
    {
        let ops = build(&config, &dt, Forward).unwrap();
        assert_eq!(kinds(&ops), ["noop", "matrix", "cdl", "log", "ec", "noop"]);
        assert_eq!(log_base(&ops[3]), 2.1);
    }

    // Same test using a shared view that uses USE_DISPLAY_NAME.
    config
        .add_shared_view("shared_view", "display_vt", OCIO_VIEW_USE_DISPLAY_NAME, "", "", "")
        .unwrap();
    config.add_display_shared_view("display", "shared_view").unwrap();
    config.validate().unwrap();
    dt.view = "shared_view".into();
    {
        let ops = build(&config, &dt, Forward).unwrap();
        assert_eq!(kinds(&ops), ["noop", "matrix", "cdl", "log", "ec", "noop"]);
        assert_eq!(offset_of(&ops[1]), OFFSET);
        assert_eq!(log_base(&ops[3]), 2.1);
    }

    // Repeat with data color space.
    cs_source.set_is_data(true);
    config.add_color_space(&cs_source).unwrap();
    config.validate().unwrap();
    assert_eq!(build(&config, &dt, Forward).unwrap().len(), 0);

    dt.data_bypass = false;
    assert_eq!(build(&config, &dt, Forward).unwrap().len(), 6);
}

const LOOKS_CONFIG: &str = r#"
ocio_profile_version: 2

roles:
  default: raw

displays:
  sRGB:
    - !<View> {name: Raw, colorspace: raw}
  display:
    - !<View> {name: view, view_transform: display_vt, display_colorspace: displayCSOut, looks: look}
    - !<View> {name: viewNoVT, colorspace: displayCSOut, looks: look}
    - !<View> {name: viewVTNT, view_transform: nt_forward, display_colorspace: displayCSOut}
    - !<View> {name: viewCSNT, colorspace: nt_inverse, looks: look}
    - !<View> {name: viewCSNTNoLook, colorspace: nt_inverse}

looks:
  - !<Look>
    name: look
    process_space: displayCSProcess
    transform: !<CDLTransform> {name: look forward, sat: 1.5}
    inverse_transform: !<CDLTransform> {name: look inverse, sat: 1.5}

view_transforms:
  - !<ViewTransform>
    name: default_vt
    to_scene_reference: !<CDLTransform> {sat: 1.5}

  - !<ViewTransform>
    name: display_vt
    to_display_reference: !<CDLTransform> {name: display vt to ref, sat: 1.5}
    from_display_reference: !<CDLTransform> {name: display vt from ref, sat: 1.5}

display_colorspaces:
  - !<ColorSpace>
    name: displayCSIn
    to_display_reference: !<CDLTransform> {name: in cs to ref, sat: 1.5}
    from_display_reference: !<CDLTransform> {name: in cs from ref, sat: 1.5}

  - !<ColorSpace>
    name: displayCSOut
    to_display_reference: !<CDLTransform> {name: out cs to ref, sat: 1.5}
    from_display_reference: !<CDLTransform> {name: out cs from ref, sat: 1.5}

  - !<ColorSpace>
    name: displayCSProcess
    to_display_reference: !<CDLTransform> {name: process cs to ref, sat: 1.5}
    from_display_reference: !<CDLTransform> {name: process cs from ref, sat: 1.5}

colorspaces:
  - !<ColorSpace>
    name: raw
    family: raw
    description: A raw color space.
    isdata: true

named_transforms:
  - !<NamedTransform>
    name: nt_forward
    transform: !<CDLTransform> {name: forward transform for nt_forward, sat: 1.5}

  - !<NamedTransform>
    name: nt_inverse
    inverse_transform: !<CDLTransform> {name: inverse transform for nt_inverse, sat: 1.5}
"#;

/// Expected op: `None` for a no-op, otherwise the CDL metadata name and
/// direction (port of the local `ValidateTransform` helper).
type Expected<'a> = Option<(&'a str, TransformDirection)>;

#[track_caller]
fn check_cdl_ops(ops: &OpVec, expected: &[Expected]) {
    assert_eq!(ops.len(), expected.len(), "ops: {:?}", kinds(ops));
    for (i, (op, e)) in ops.iter().zip(expected).enumerate() {
        match e {
            None => assert!(op.is_no_op(), "op {i} should be a no-op"),
            Some((name, dir)) => match op.to_transform() {
                Some(Transform::Cdl(cdl)) => {
                    assert_eq!(cdl.metadata.attributes.len(), 1, "op {i}");
                    assert_eq!(cdl.metadata.attribute_value("name"), *name, "op {i}");
                    assert_eq!(cdl.direction, *dir, "op {i}");
                }
                t => panic!("op {i}: expected a CDL, got {t:?}"),
            },
        }
    }
}

#[test]
fn display_view_transform_build_ops_with_looks_errors() {
    let config = Config::create_from_str(LOOKS_CONFIG).unwrap();
    config.validate().unwrap();
    // Src can't be a named transform.
    let dt = DisplayViewTransform::new("nt_forward", "display", "view");
    assert_err!(
        build(&config, &dt, TransformDirection::Forward),
        "Cannot find source color space named 'nt_forward'"
    );
}

#[test]
#[ignore = "needs-merge"]
fn display_view_transform_build_ops_with_looks() {
    use TransformDirection::{Forward as F, Inverse as I};
    let config = Config::create_from_str(LOOKS_CONFIG).unwrap();
    config.validate().unwrap();

    let mut dt = DisplayViewTransform::new("displayCSIn", "display", "view");

    let ops = build(&config, &dt, F).unwrap();
    check_cdl_ops(
        &ops,
        &[
            None,
            Some(("in cs to ref", F)),
            Some(("process cs from ref", F)),
            None,
            None,
            Some(("look forward", F)),
            None,
            Some(("process cs to ref", F)),
            Some(("display vt from ref", F)),
            Some(("out cs from ref", F)),
            None,
        ],
    );

    let ops = build(&config, &dt, I).unwrap();
    check_cdl_ops(
        &ops,
        &[
            None,
            Some(("out cs to ref", F)),
            Some(("display vt to ref", F)),
            Some(("process cs from ref", F)),
            None,
            None,
            Some(("look inverse", F)),
            None,
            Some(("process cs to ref", F)),
            Some(("in cs from ref", F)),
            None,
        ],
    );

    // Looks can be bypassed.
    dt.looks_bypass = true;
    let ops = build(&config, &dt, F).unwrap();
    check_cdl_ops(
        &ops,
        &[
            None,
            Some(("in cs to ref", F)),
            Some(("display vt from ref", F)),
            Some(("out cs from ref", F)),
            None,
        ],
    );

    // Without a view transform.
    dt.looks_bypass = false;
    dt.view = "viewNoVT".into();
    let ops = build(&config, &dt, F).unwrap();
    check_cdl_ops(
        &ops,
        &[
            None,
            Some(("in cs to ref", F)),
            Some(("process cs from ref", F)),
            None,
            None,
            Some(("look forward", F)),
            None,
            Some(("process cs to ref", F)),
            Some(("out cs from ref", F)),
            None,
        ],
    );
    let ops = build(&config, &dt, I).unwrap();
    check_cdl_ops(
        &ops,
        &[
            None,
            Some(("out cs to ref", F)),
            Some(("process cs from ref", F)),
            None,
            None,
            Some(("look inverse", F)),
            None,
            Some(("process cs to ref", F)),
            Some(("in cs from ref", F)),
            None,
        ],
    );

    // Src can't be a named transform.
    dt.src = "nt_forward".into();
    dt.view = "view".into();
    assert_err!(build(&config, &dt, F), "Cannot find source color space named 'nt_forward'");

    // View color space is a named transform.
    dt.src = "displayCSIn".into();
    dt.view = "viewCSNT".into();
    let ops = build(&config, &dt, F).unwrap();
    check_cdl_ops(
        &ops,
        &[
            None,
            Some(("in cs to ref", F)),
            Some(("process cs from ref", F)),
            None,
            None,
            Some(("look forward", F)),
            Some(("inverse transform for nt_inverse", I)),
        ],
    );
    let ops = build(&config, &dt, I).unwrap();
    check_cdl_ops(
        &ops,
        &[
            Some(("inverse transform for nt_inverse", F)),
            None,
            Some(("look inverse", F)),
            None,
            Some(("process cs to ref", F)),
            Some(("in cs from ref", F)),
            None,
        ],
    );

    // View color space is a named transform and no look.
    dt.view = "viewCSNTNoLook".into();
    let ops = build(&config, &dt, F).unwrap();
    check_cdl_ops(&ops, &[Some(("inverse transform for nt_inverse", I))]);
    let ops = build(&config, &dt, I).unwrap();
    check_cdl_ops(&ops, &[Some(("inverse transform for nt_inverse", F))]);

    // View transform is a named transform.
    dt.view = "viewVTNT".into();
    let ops = build(&config, &dt, F).unwrap();
    check_cdl_ops(
        &ops,
        &[Some(("forward transform for nt_forward", F)), Some(("out cs from ref", F)), None],
    );
    let ops = build(&config, &dt, I).unwrap();
    check_cdl_ops(
        &ops,
        &[None, Some(("out cs to ref", F)), Some(("forward transform for nt_forward", I))],
    );
}

#[test]
fn display_view_transform_config_load() {
    const CONFIG: &str = r#"
ocio_profile_version: 2

roles:
  default: raw

displays:
  displayName:
    - !<View> {name: viewName, colorspace: out}

colorspaces:
  - !<ColorSpace>
    name: raw

  - !<ColorSpace>
    name: in
    to_scene_reference: !<MatrixTransform> {offset: [0.11, 0.12, 0.13, 0]}

  - !<ColorSpace>
    name: out
    from_scene_reference: !<MatrixTransform> {offset: [0.11, 0.12, 0.13, 0]}

  - !<ColorSpace>
    name: test
    from_scene_reference: !<DisplayViewTransform> {src: in, display: displayName, view: viewName}
    to_scene_reference: !<DisplayViewTransform> {src: in, display: displayName, view: viewName, looks_bypass: true, data_bypass: false}
"#;
    let config = Config::create_from_str(CONFIG).unwrap();
    let cs = config.get_color_space("test").unwrap();
    match cs.transform(ColorSpaceDirection::FromReference) {
        Some(Transform::DisplayView(d)) => {
            assert_eq!(d.direction, TransformDirection::Forward);
            assert_eq!(d.src, "in");
            assert_eq!(d.display, "displayName");
            assert_eq!(d.view, "viewName");
            assert!(!d.looks_bypass);
            assert!(d.data_bypass);
        }
        t => panic!("unexpected {t:?}"),
    }
    match cs.transform(ColorSpaceDirection::ToReference) {
        Some(Transform::DisplayView(d)) => {
            assert!(d.looks_bypass);
            assert!(!d.data_bypass);
        }
        t => panic!("unexpected {t:?}"),
    }
}

const APPLY_CONFIG: &str = r#"
ocio_profile_version: 2

roles:
  default: raw

displays:
  sRGB:
    - !<View> {name: Raw, colorspace: raw}
  display:
    - !<View> {name: view, view_transform: display_vt, display_colorspace: displayCSOut, looks: look}
    - !<View> {name: viewNoVT, colorspace: displayCSOut, looks: look}

looks:
  - !<Look>
    name: look
    process_space: displayCSProcess
    transform: !<MatrixTransform> {offset: [0.1, 0.2, 0.3, 0]}

view_transforms:
  - !<ViewTransform>
    name: default_vt
    to_scene_reference: !<MatrixTransform> {offset: [0.2, 0.2, 0.4, 0]}

  - !<ViewTransform>
    name: display_vt
    to_display_reference: !<MatrixTransform> {offset: [0.3, 0.1, 0.1, 0]}

display_colorspaces:
  - !<ColorSpace>
    name: displayCSOut
    to_display_reference: !<MatrixTransform> {offset: [0.25, 0.15, 0.35, 0]}

  - !<ColorSpace>
    name: displayCSProcess
    to_display_reference: !<MatrixTransform> {offset: [0.1, 0.1, 0.1, 0]}

colorspaces:
  - !<ColorSpace>
    name: raw
    family: raw
    description: A raw color space.
    isdata: true

  - !<ColorSpace>
    name: displayCSIn
    to_scene_reference: !<MatrixTransform> {offset: [-0.15, 0.15, 0.15, 0.05]}
"#;

fn proc_of(config: &Config, dt: &DisplayViewTransform, dir: TransformDirection) -> Result<Processor> {
    let t: Transform = dt.clone().into();
    config.get_processor_for_transform(&t, dir)
}

#[test]
#[ignore = "needs-merge"]
fn display_view_transform_apply_fwd_inv() {
    use TransformDirection::{Forward, Inverse};
    let config = Config::create_from_str(APPLY_CONFIG).unwrap();
    config.validate().unwrap();

    let mut dt = DisplayViewTransform::new("displayCSIn", "display", "view");
    let reference: [[f32; 4]; 4] = [
        [0.0, 0.1, 0.2, 0.0],
        [0.3, 0.4, 0.5, 0.5],
        [0.6, 0.7, 0.8, 0.7],
        [0.9, 1.0, 1.1, 1.0],
    ];

    for (view, n) in [("view", 7), ("viewNoVT", 6)] {
        dt.view = view.into();
        let proc = proc_of(&config, &dt, Forward).unwrap().optimized(OptimizationFlags::NONE);
        assert_eq!(proc.ops().len(), n);
        assert_eq!(proc.create_group_transform().transforms.len(), n);
        let cpu = proc.default_cpu_processor();

        let proc_inv = proc_of(&config, &dt, Inverse).unwrap().optimized(OptimizationFlags::NONE);
        assert_eq!(proc_inv.ops().len(), n);
        assert_eq!(proc_inv.create_group_transform().transforms.len(), n);
        let cpu_inv = proc_inv.default_cpu_processor();

        for r in &reference {
            let mut rgba = *r;
            cpu.apply_rgba(&mut rgba);
            cpu_inv.apply_rgba(&mut rgba);
            for c in 0..4 {
                assert!((rgba[c] - r[c]).abs() <= 1e-6, "{rgba:?} vs {r:?}");
            }
        }

        let mut group = GroupTransform::new();
        group.transforms.push(dt.clone().into());
        let mut dt_inv = dt.clone();
        dt_inv.direction = Inverse;
        group.transforms.push(dt_inv.into());
        let group_proc = config
            .get_processor_for_transform(&group.into(), Forward)
            .unwrap()
            .optimized(OptimizationFlags::DEFAULT);
        assert!(group_proc.is_no_op());
    }

    let mut e_config = config.create_editable_copy();
    let mut dt = DisplayViewTransform::new("displayCSIn", "display", "view");

    // Missing look.
    e_config
        .add_display_view_full("display", "bad_view", "display_vt", "displayCSOut", "missing look", "", "")
        .unwrap();
    dt.view = "bad_view".into();
    assert_err!(
        proc_of(&e_config, &dt, Forward),
        "RunLookTokens error. The specified look, 'missing look', cannot be found.  (looks: look)."
    );
    dt.view = "view".into();

    // Missing viewing rule does not currently throw when getting a processor.
    e_config
        .add_display_view_full("display", "bad_view", "display_vt", "displayCSOut", "", "missing rule", "desc: foo")
        .unwrap();
    proc_of(&e_config, &dt, Forward).unwrap();
    assert_err!(
        e_config.validate(),
        "Config failed display view validation. Display 'display' has a view 'bad_view' refers to a viewing rule, 'missing rule', which is not defined."
    );
}

#[test]
fn display_view_transform_errors() {
    use TransformDirection::Forward;
    let config = Config::create_from_str(APPLY_CONFIG).unwrap();
    config.validate().unwrap();
    let mut dt = DisplayViewTransform::new("displayCSIn", "display", "view");

    dt.display = "".into();
    assert_err!(proc_of(&config, &dt, Forward), "DisplayViewTransform: empty display name.");
    dt.display = "display".into();

    dt.view = "".into();
    assert_err!(proc_of(&config, &dt, Forward), "DisplayViewTransform: empty view name.");
    dt.view = "view".into();

    dt.src = "".into();
    assert_err!(
        proc_of(&config, &dt, Forward),
        "DisplayViewTransform: empty source color space name."
    );

    dt.src = "missing cs".into();
    assert_err!(
        proc_of(&config, &dt, Forward),
        "DisplayViewTransform error. Cannot find source color space named 'missing cs'."
    );
    dt.src = "displayCSIn".into();

    dt.display = "missing display".into();
    assert_err!(
        proc_of(&config, &dt, Forward),
        "DisplayViewTransform error. Display 'missing display' not found."
    );
    dt.display = "display".into();

    let mut e_config = config.create_editable_copy();
    e_config
        .add_display_view_full("display", "bad_view", "missing vt", "displayCSOut", "", "", "")
        .unwrap();
    dt.view = "bad_view".into();
    assert_err!(
        proc_of(&e_config, &dt, Forward),
        "DisplayViewTransform error. The view transform 'missing vt' is neither a view transform nor a named transform."
    );

    dt.view = "missing view".into();
    assert_err!(
        proc_of(&config, &dt, Forward),
        "DisplayViewTransform error. The display 'display' does not have view 'missing view'."
    );

    e_config
        .add_display_view_full("display", "bad_view", "display_vt", "missing cs", "", "", "")
        .unwrap();
    dt.view = "bad_view".into();
    assert_err!(
        proc_of(&e_config, &dt, Forward),
        "DisplayViewTransform error. The view 'bad_view' refers to a display color space 'missing cs' that can't be found."
    );
    assert_err!(
        e_config.validate(),
        "Config failed display view validation. Display 'display' has a view 'bad_view' that refers to a color space or a named transform, 'missing cs', which is not defined."
    );

    e_config.add_display_view("display", "bad_view", "missing cs", "").unwrap();
    assert_err!(
        proc_of(&e_config, &dt, Forward),
        "DisplayViewTransform error. Cannot find color space or named transform with name 'missing cs'."
    );
}

#[test]
fn display_view_transform_context_variables() {
    const CONFIG: &str = r#"
ocio_profile_version: 2

environment: { FILE: cdl_test1.cc }

roles:
  default: cs1

file_rules:
  - !<Rule> {name: Default, colorspace: default}

displays:
  Disp1:
    - !<View> {name: View1, colorspace: cs1}
    - !<View> {name: View2, colorspace: cs4}
    - !<View> {name: View3, view_transform: vt1, display_colorspace: dcs1}
    - !<View> {name: View4, view_transform: vt1, display_colorspace: dcs2}
    - !<View> {name: View5, view_transform: vt2, display_colorspace: dcs1}
    - !<View> {name: View6, view_transform: vt2, display_colorspace: dcs2}
    - !<View> {name: View10, colorspace: cs1, looks: look1}
    - !<View> {name: View11, colorspace: cs1, looks: look2}
    - !<View> {name: View12, colorspace: cs1, looks: look3}
    - !<View> {name: View13, view_transform: vt1, display_colorspace: dcs2, looks: +look1}
    - !<View> {name: View14, view_transform: vt1, display_colorspace: dcs2, looks: +look2}
    - !<View> {name: View15, view_transform: vt1, display_colorspace: dcs2, looks: +look3}
    - !<View> {name: View16, view_transform: vt2, display_colorspace: dcs2, looks: +look1}
    - !<View> {name: View17, view_transform: vt2, display_colorspace: dcs2, looks: +look2}
    - !<View> {name: View18, view_transform: vt2, display_colorspace: dcs2, looks: +look3}

looks:
  - !<Look>
    name: look1
    process_space: default
    transform: !<FileTransform> {src: $FILE}
  - !<Look>
    name: look2
    process_space: default
    transform: !<LookTransform> {src: default, dst: cs2, looks: +look1}
  - !<Look>
    name: look3
    process_space: default
    transform: !<CDLTransform> {offset: [0.1, 0.1, 0.1]}

view_transforms:
  - !<ViewTransform>
    name: vt1
    to_scene_reference: !<FileTransform> {src: $FILE}
  - !<ViewTransform>
    name: vt2
    to_scene_reference: !<MatrixTransform> {offset: [0.2, 0.2, 0.4, 0]}

display_colorspaces:
  - !<ColorSpace>
    name: dcs1
    to_display_reference: !<FileTransform> {src: $FILE}
  - !<ColorSpace>
    name: dcs2
    to_display_reference: !<MatrixTransform> {offset: [0.25, 0.15, 0.35, 0]}

colorspaces:
  - !<ColorSpace>
    name: cs1
    allocation: uniform
  - !<ColorSpace>
    name: cs2
    allocation: uniform
    from_scene_reference: !<MatrixTransform> {offset: [0.11, 0.12, 0.13, 0]}
  - !<ColorSpace>
    name: cs3
    allocation: uniform
    from_scene_reference: !<MatrixTransform> {offset: [0.1, 0.2, 0.3, 0]}
  - !<ColorSpace>
    name: cs4
    allocation: uniform
    from_scene_reference: !<FileTransform> {src: $FILE}
"#;
    let _lock = env_lock();
    let mut cfg = Config::create_from_str(CONFIG).unwrap().create_editable_copy();
    cfg.set_search_path(&data_file(""));
    cfg.validate().unwrap();

    let mut used = Context::new();
    let cases = [
        ("View1", false),
        ("View2", true),
        ("View3", true),
        ("View4", true),
        ("View5", true),
        ("View6", false),
        ("View10", true),
        ("View11", true),
        ("View12", false),
        ("View13", true),
        ("View14", true),
        ("View15", true),
        ("View16", true),
        ("View17", true),
        ("View18", false),
    ];
    for (view, expected) in cases {
        let dt: Transform = DisplayViewTransform::new("cs1", "Disp1", view).into();
        let found = collect_context_variables(&cfg, cfg.current_context(), &dt, &mut used).unwrap();
        assert_eq!(found, expected, "view {view}");
    }
}
