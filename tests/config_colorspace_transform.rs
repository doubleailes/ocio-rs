//! Port of `transforms/ColorSpaceTransform_tests.cpp`.

mod config_common;

use config_common::*;
use ocio::config::{
    collect_context_variables, BuildColorSpaceOps, ColorSpace, NamedTransform, ViewTransform,
};
use ocio::ops::{OpRc, OpVec};
use ocio::transforms::BuildOps;
use ocio::*;

#[test]
fn colorspace_transform_basic() {
    let mut cst = ColorSpaceTransform::default();
    assert_eq!(cst.direction, TransformDirection::Forward);
    cst.direction = TransformDirection::Inverse;
    assert_eq!(cst.direction, TransformDirection::Inverse);

    assert_eq!(cst.src, "");
    cst.src = "source".into();
    assert_eq!(cst.src, "source");
    assert_eq!(cst.dst, "");
    cst.dst = "destination".into();
    assert_eq!(cst.dst, "destination");

    assert!(cst.data_bypass);
    cst.data_bypass = false;
    assert!(!cst.data_bypass);
    cst.data_bypass = true;

    let t: Transform = cst.clone().into();
    t.validate().unwrap();

    let mut c = cst.clone();
    c.src.clear();
    assert_err!(
        Transform::from(c).validate(),
        "ColorSpaceTransform: empty source color space name"
    );
    let mut c = cst.clone();
    c.dst.clear();
    assert_err!(
        Transform::from(c).validate(),
        "ColorSpaceTransform: empty destination color space name"
    );
}

/// The transform an op converts back to.
fn op_transform(op: &OpRc) -> Transform {
    op.to_transform()
        .unwrap_or_else(|| panic!("op {} has no transform", op.name()))
}

fn check_matrix_op(op: &OpRc, offset: &[f64; 4], dir: TransformDirection) {
    match op_transform(op) {
        Transform::Matrix(m) => {
            assert_eq!(m.direction, dir);
            assert_eq!(&m.offset, offset);
        }
        t => panic!("expected a matrix, got {t:?}"),
    }
}

fn check_ff_op(op: &OpRc, style: FixedFunctionStyle, dir: TransformDirection) {
    match op_transform(op) {
        Transform::FixedFunction(f) => {
            assert_eq!(f.style, style);
            assert_eq!(f.direction, dir);
        }
        t => panic!("expected a fixed function, got {t:?}"),
    }
}

fn check_log_op(op: &OpRc, base: f64, dir: TransformDirection) {
    match op_transform(op) {
        Transform::Log(l) => {
            assert_eq!(l.base, base);
            assert_eq!(l.direction, dir);
        }
        t => panic!("expected a log, got {t:?}"),
    }
}

fn build(config: &Config, cst: &ColorSpaceTransform, dir: TransformDirection) -> Result<OpVec> {
    let mut ops = OpVec::new();
    cst.build_ops(&mut ops, config, config.current_context(), dir)?;
    Ok(ops)
}

fn matrix_offset(offset: [f64; 4]) -> Transform {
    MatrixTransform {
        offset,
        ..Default::default()
    }
    .into()
}

fn ff(style: FixedFunctionStyle) -> Transform {
    FixedFunctionTransform::new(style, &[]).into()
}

struct BuildSetup {
    config: Config,
    cs_scene_to_ref: ColorSpace,
    cs_scene_from_ref: ColorSpace,
    cst: ColorSpaceTransform,
}

const OFFSET: [f64; 4] = [0.0, 0.1, 0.2, 0.0];

fn build_setup() -> BuildSetup {
    let cst = ColorSpaceTransform::new("source", "destination");
    let mut config = Config::create_raw().create_editable_copy();
    let mut cs_scene_to_ref = ColorSpace::new(ReferenceSpaceType::Scene);
    cs_scene_to_ref.set_name("source");
    cs_scene_to_ref.set_transform(
        Some(matrix_offset(OFFSET)),
        ColorSpaceDirection::ToReference,
    );
    config.add_color_space(&cs_scene_to_ref).unwrap();

    let mut cs_scene_from_ref = ColorSpace::new(ReferenceSpaceType::Scene);
    cs_scene_from_ref.set_name("destination");
    cs_scene_from_ref.set_transform(
        Some(ff(FixedFunctionStyle::AcesGlow03)),
        ColorSpaceDirection::FromReference,
    );
    config.add_color_space(&cs_scene_from_ref).unwrap();

    config
        .add_display_view("display", "view", "destination", "")
        .unwrap();
    config.validate().unwrap();
    BuildSetup {
        config,
        cs_scene_to_ref,
        cs_scene_from_ref,
        cst,
    }
}

#[test]
fn colorspace_transform_build_colorspace_ops_errors() {
    let BuildSetup { config, .. } = build_setup();
    let cst = ColorSpaceTransform::new("source_missing", "destination");
    assert_err!(
        build(&config, &cst, TransformDirection::Forward),
        "Color space 'source_missing' could not be found"
    );
    let cst = ColorSpaceTransform::new("source", "destination_missing");
    assert_err!(
        build(&config, &cst, TransformDirection::Forward),
        "Color space 'destination_missing' could not be found"
    );

    // From a color space to the same one, identified by its name and an alias.
    let mut config = config;
    let mut cs = config.get_color_space("source").unwrap().clone();
    cs.add_alias("aliasToRef");
    config.add_color_space(&cs).unwrap();
    let ops = build(
        &config,
        &ColorSpaceTransform::new("source", "aliasToRef"),
        TransformDirection::Forward,
    )
    .unwrap();
    assert_eq!(ops.len(), 0);
}

#[test]
fn colorspace_transform_build_colorspace_ops() {
    use TransformDirection::{Forward, Inverse};
    let BuildSetup {
        mut config,
        mut cs_scene_to_ref,
        mut cs_scene_from_ref,
        mut cst,
    } = build_setup();

    {
        let ops = build(&config, &cst, Forward).unwrap();
        assert_eq!(ops.len(), 4);
        assert!(ops[0].is_no_op());
        check_matrix_op(&ops[1], &OFFSET, Forward);
        check_ff_op(&ops[2], FixedFunctionStyle::AcesGlow03, Forward);
        assert!(ops[3].is_no_op());
    }
    {
        cs_scene_to_ref.add_alias("aliasToRef");
        config.add_color_space(&cs_scene_to_ref).unwrap();
        cs_scene_from_ref.add_alias("aliasFromRef");
        config.add_color_space(&cs_scene_from_ref).unwrap();

        let cst_alias = ColorSpaceTransform::new("aliasToRef", "aliasFromRef");
        let ops = build(&config, &cst_alias, Forward).unwrap();
        assert_eq!(ops.len(), 4);
        assert!(ops[0].is_no_op());
        assert!(matches!(op_transform(&ops[1]), Transform::Matrix(_)));
        assert!(matches!(op_transform(&ops[2]), Transform::FixedFunction(_)));
        assert!(ops[3].is_no_op());
    }
    {
        let ops = build(
            &config,
            &ColorSpaceTransform::new("source", "aliasToRef"),
            Forward,
        )
        .unwrap();
        assert_eq!(ops.len(), 0);
    }
    {
        // Data color spaces.
        cs_scene_to_ref.set_is_data(true);
        config.add_color_space(&cs_scene_to_ref).unwrap();

        assert_eq!(build(&config, &cst, Forward).unwrap().len(), 0);
        config.set_processor_cache_flags(ProcessorCacheFlags::OFF);
        let t: Transform = cst.clone().into();
        let proc = config.get_processor_for_transform(&t, Forward).unwrap();
        assert_eq!(proc.ops().len(), 0);

        cst.data_bypass = false;
        assert_eq!(build(&config, &cst, Forward).unwrap().len(), 4);
        let t: Transform = cst.clone().into();
        let proc = config.get_processor_for_transform(&t, Forward).unwrap();
        let proc = proc.optimized(OptimizationFlags::NONE);
        assert_eq!(proc.ops().len(), 2);

        let proc = config.get_processor("source", "destination").unwrap();
        assert_eq!(proc.ops().len(), 0);

        cs_scene_to_ref.set_is_data(false);
        config.add_color_space(&cs_scene_to_ref).unwrap();
        cst.data_bypass = true;

        cs_scene_from_ref.set_is_data(true);
        config.add_color_space(&cs_scene_from_ref).unwrap();

        assert_eq!(build(&config, &cst, Forward).unwrap().len(), 0);
        let t: Transform = cst.clone().into();
        assert_eq!(
            config
                .get_processor_for_transform(&t, Forward)
                .unwrap()
                .ops()
                .len(),
            0
        );

        cst.data_bypass = false;
        assert_eq!(build(&config, &cst, Forward).unwrap().len(), 4);
        let t: Transform = cst.clone().into();
        let proc = config.get_processor_for_transform(&t, Forward).unwrap();
        assert_eq!(proc.optimized(OptimizationFlags::NONE).ops().len(), 2);

        let proc = config.get_processor("source", "destination").unwrap();
        assert_eq!(proc.ops().len(), 0);

        cs_scene_from_ref.set_is_data(false);
        config.add_color_space(&cs_scene_from_ref).unwrap();
        cst.data_bypass = true;
    }
    {
        // Other direction.
        let ops = build(&config, &cst, Inverse).unwrap();
        assert_eq!(ops.len(), 4);
        assert!(ops[0].is_no_op());
        check_ff_op(&ops[1], FixedFunctionStyle::AcesGlow03, Inverse);
        check_matrix_op(&ops[2], &OFFSET, Inverse);
        assert!(ops[3].is_no_op());

        let ops = ocio::processor::optimize_ops(&ops, OptimizationFlags::NONE);
        assert_eq!(ops.len(), 2);
        match op_transform(&ops[1]) {
            Transform::Matrix(m) => {
                if m.direction == Forward {
                    assert_eq!(m.offset, [-OFFSET[0], -OFFSET[1], -OFFSET[2], -OFFSET[3]]);
                } else {
                    assert_eq!(m.offset, OFFSET);
                }
            }
            t => panic!("expected a matrix, got {t:?}"),
        }
    }
    {
        let ctx = config.current_context().clone();
        let mut ops = OpVec::new();
        BuildColorSpaceOps::to_reference(&mut ops, &config, &ctx, &cs_scene_from_ref, false)
            .unwrap();
        assert_eq!(ops.len(), 2);
        check_ff_op(&ops[1], FixedFunctionStyle::AcesGlow03, Inverse);

        let mut ops = OpVec::new();
        BuildColorSpaceOps::from_reference(&mut ops, &config, &ctx, &cs_scene_from_ref, true)
            .unwrap();
        assert_eq!(ops.len(), 2);
        check_ff_op(&ops[0], FixedFunctionStyle::AcesGlow03, Forward);
    }
    {
        let ctx = config.current_context().clone();
        let mut cs_scene_both = cs_scene_from_ref.clone();
        cs_scene_both.set_transform(
            Some(ff(FixedFunctionStyle::AcesGlow10)),
            ColorSpaceDirection::ToReference,
        );
        cs_scene_both.set_is_data(true);

        let mut ops = OpVec::new();
        BuildColorSpaceOps::from_reference(&mut ops, &config, &ctx, &cs_scene_both, true).unwrap();
        assert_eq!(ops.len(), 0);
        BuildColorSpaceOps::from_reference(&mut ops, &config, &ctx, &cs_scene_both, false).unwrap();
        assert_eq!(ops.len(), 2);
        check_ff_op(&ops[0], FixedFunctionStyle::AcesGlow03, Forward);

        cs_scene_both.set_is_data(false);
        let mut ops = OpVec::new();
        BuildColorSpaceOps::to_reference(&mut ops, &config, &ctx, &cs_scene_both, true).unwrap();
        assert_eq!(ops.len(), 2);
        check_ff_op(&ops[1], FixedFunctionStyle::AcesGlow10, Forward);
    }

    // Replace the 2 color spaces by display-referred color spaces.
    let mut cs_display_to_ref = ColorSpace::new(ReferenceSpaceType::Display);
    cs_display_to_ref.set_name("source");
    cs_display_to_ref.set_transform(
        Some(matrix_offset(OFFSET)),
        ColorSpaceDirection::ToReference,
    );
    config.add_color_space(&cs_display_to_ref).unwrap();
    let mut cs_display_from_ref = ColorSpace::new(ReferenceSpaceType::Display);
    cs_display_from_ref.set_name("destination");
    cs_display_from_ref.set_transform(
        Some(ff(FixedFunctionStyle::AcesGlow10)),
        ColorSpaceDirection::FromReference,
    );
    config.add_color_space(&cs_display_from_ref).unwrap();

    let mut vt = ViewTransform::new(ReferenceSpaceType::Scene);
    vt.set_name("view_transform");
    vt.set_transform(
        Some(matrix_offset(OFFSET)),
        ViewTransformDirection::FromReference,
    );
    config.add_view_transform(&vt).unwrap();

    assert_eq!(config.num_color_spaces(), 3);
    config.validate().unwrap();
    {
        let ops = build(&config, &cst, Forward).unwrap();
        assert_eq!(ops.len(), 4);
        assert!(ops[0].is_no_op());
        assert!(matches!(op_transform(&ops[1]), Transform::Matrix(_)));
        assert!(matches!(op_transform(&ops[2]), Transform::FixedFunction(_)));
        assert!(ops[3].is_no_op());
    }
}

fn reference_setup() -> Config {
    let mut config = Config::create_raw().create_editable_copy();
    let mut cs = ColorSpace::new(ReferenceSpaceType::Scene);
    cs.set_name("scene");
    cs.set_transform(
        Some(ff(FixedFunctionStyle::AcesGlow03)),
        ColorSpaceDirection::FromReference,
    );
    config.add_color_space(&cs).unwrap();
    config
        .add_display_view("display", "view", "scene", "")
        .unwrap();
    config.validate().unwrap();
    config
}

fn add_scene_view_transform(config: &mut Config) -> ViewTransform {
    let mut vt = ViewTransform::new(ReferenceSpaceType::Scene);
    vt.set_name("view_transform");
    vt.set_transform(
        Some(matrix_offset(OFFSET)),
        ViewTransformDirection::FromReference,
    );
    config.add_view_transform(&vt).unwrap();
    config.validate().unwrap();
    vt
}

#[test]
fn colorspace_transform_build_reference_conversion_ops_no_view_transform() {
    use ReferenceSpaceType::{Display, Scene};
    let config = reference_setup();
    let ctx = config.current_context().clone();
    let mut ops = OpVec::new();
    BuildColorSpaceOps::reference_conversion(&mut ops, &config, &ctx, Scene, Scene).unwrap();
    assert_eq!(ops.len(), 0);
    BuildColorSpaceOps::reference_conversion(&mut ops, &config, &ctx, Display, Display).unwrap();
    assert_eq!(ops.len(), 0);
    assert_err!(
        BuildColorSpaceOps::reference_conversion(&mut ops, &config, &ctx, Scene, Display),
        "no view transform between the main scene-referred space and the display-referred space"
    );
    assert_err!(
        BuildColorSpaceOps::reference_conversion(&mut ops, &config, &ctx, Display, Scene),
        "no view transform between the main scene-referred space and the display-referred space"
    );
}

#[test]
fn colorspace_transform_build_reference_conversion_ops() {
    use ReferenceSpaceType::{Display, Scene};
    let mut config = reference_setup();
    add_scene_view_transform(&mut config);
    let ctx = config.current_context().clone();

    let mut ops = OpVec::new();
    BuildColorSpaceOps::reference_conversion(&mut ops, &config, &ctx, Scene, Display).unwrap();
    assert_eq!(ops.len(), 1);
    check_matrix_op(&ops[0], &OFFSET, TransformDirection::Forward);

    let mut ops = OpVec::new();
    BuildColorSpaceOps::reference_conversion(&mut ops, &config, &ctx, Display, Scene).unwrap();
    assert_eq!(ops.len(), 1);
    check_matrix_op(&ops[0], &OFFSET, TransformDirection::Inverse);
}

#[test]
fn colorspace_transform_build_colorspace_ops_with_reference_conversion() {
    use TransformDirection::{Forward, Inverse};
    let mut config = reference_setup();
    let mut vt = add_scene_view_transform(&mut config);

    let mut cs = ColorSpace::new(ReferenceSpaceType::Display);
    cs.set_name("display");
    cs.set_transform(
        Some(LogTransform::default().into()),
        ColorSpaceDirection::FromReference,
    );
    config.add_color_space(&cs).unwrap();
    config.validate().unwrap();

    let cst = ColorSpaceTransform::new("scene", "display");
    {
        let ops = build(&config, &cst, Forward).unwrap();
        assert_eq!(ops.len(), 5);
        assert!(ops[0].is_no_op());
        check_ff_op(&ops[1], FixedFunctionStyle::AcesGlow03, Inverse);
        check_matrix_op(&ops[2], &OFFSET, Forward);
        check_log_op(&ops[3], 2.0, Forward);
        assert!(ops[4].is_no_op());
    }
    {
        let ops = build(&config, &cst, Inverse).unwrap();
        assert_eq!(ops.len(), 5);
        assert!(ops[0].is_no_op());
        check_log_op(&ops[1], 2.0, Inverse);
        check_matrix_op(&ops[2], &OFFSET, Inverse);
        check_ff_op(&ops[3], FixedFunctionStyle::AcesGlow03, Forward);
        assert!(ops[4].is_no_op());
    }

    vt.set_transform(
        Some(ExponentTransform::default().into()),
        ViewTransformDirection::ToReference,
    );
    config.add_view_transform(&vt).unwrap();
    {
        let ops = build(&config, &cst, Inverse).unwrap();
        assert_eq!(ops.len(), 5);
        assert!(ops[0].is_no_op());
        check_log_op(&ops[1], 2.0, Inverse);
        // ExponentTransform is implemented using a gamma op (identity
        // parameters, but it still clamps negative values).
        assert!(ops[2]
            .downcast_ref::<ocio::ops::gamma::GammaOp>()
            .unwrap()
            .data()
            .is_identity());
        check_ff_op(&ops[3], FixedFunctionStyle::AcesGlow03, Forward);
        assert!(ops[4].is_no_op());
    }
}

#[test]
fn colorspace_transform_context_variables() {
    let mut cfg = Config::create_raw().create_editable_copy();
    cfg.set_search_path(&data_file(""));
    let mut ctx = cfg.current_context().clone();

    let matrix = matrix_offset([0.1, 0.2, 0.3, 0.0]);
    let mut cs1 = ColorSpace::default();
    cs1.set_name("cs1");
    cs1.set_transform(Some(matrix.clone()), ColorSpaceDirection::ToReference);
    cfg.add_color_space(&cs1).unwrap();
    let mut cs2 = ColorSpace::default();
    cs2.set_name("cs2");
    cs2.set_transform(Some(matrix), ColorSpaceDirection::ToReference);
    cfg.add_color_space(&cs2).unwrap();

    let mut cst = ColorSpaceTransform::new("cs1", "cs2");
    let mut cs3 = ColorSpace::default();
    cs3.set_name("cs3");
    cs3.set_transform(Some(cst.clone().into()), ColorSpaceDirection::ToReference);
    cfg.add_color_space(&cs3).unwrap();
    cfg.validate().unwrap();

    let collect = |cfg: &Config, ctx: &Context, cst: &ColorSpaceTransform| {
        let mut used = Context::new();
        let found = collect_context_variables(cfg, ctx, &cst.clone().into(), &mut used).unwrap();
        (found, used)
    };

    // Case 1 - No context variables.
    let (found, used) = collect(&cfg, &ctx, &cst);
    assert!(!found);
    assert_eq!(used.num_string_vars(), 0);

    // Case 2 - The source color space name is now a context variable.
    ctx.set_string_var("ENV1", Some("cs1"));
    cst.src = "$ENV1".into();
    let (found, used) = collect(&cfg, &ctx, &cst);
    assert!(found);
    assert_eq!(used.num_string_vars(), 1);
    assert_eq!(used.string_var_name_by_index(0), Some("ENV1"));
    assert_eq!(used.string_var_by_index(0), Some("cs1"));

    // Case 3 - A context variable exists but is not used.
    cst.src = "cs1".into();
    let (found, used) = collect(&cfg, &ctx, &cst);
    assert!(!found);
    assert_eq!(used.num_string_vars(), 0);

    // Case 4 - Context variable indirectly used.
    ctx.set_string_var("ENV1", Some("exposure_contrast_linear.ctf"));
    let file: Transform = FileTransform::new("$ENV1").into();
    let mut cs4 = ColorSpace::default();
    cs4.set_name("cs4");
    cs4.set_transform(Some(file.clone()), ColorSpaceDirection::ToReference);
    cfg.add_color_space(&cs4).unwrap();
    cst.src = "cs4".into();
    let (found, used) = collect(&cfg, &ctx, &cst);
    assert!(found);
    assert_eq!(used.num_string_vars(), 1);
    assert_eq!(used.string_var_name_by_index(0), Some("ENV1"));
    assert_eq!(
        used.string_var_by_index(0),
        Some("exposure_contrast_linear.ctf")
    );

    // Case 5 - Context variable indirectly used via a NamedTransform.
    let mut nt = NamedTransform::new();
    nt.set_name("nt");
    nt.set_transform(Some(file), TransformDirection::Forward);
    cfg.add_named_transform(&nt).unwrap();
    cst.src = "nt".into();
    let (found, used) = collect(&cfg, &ctx, &cst);
    assert!(found);
    assert_eq!(used.num_string_vars(), 1);
    assert_eq!(used.string_var_name_by_index(0), Some("ENV1"));
    assert_eq!(
        used.string_var_by_index(0),
        Some("exposure_contrast_linear.ctf")
    );
}
