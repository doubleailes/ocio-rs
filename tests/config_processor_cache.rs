//! Port of `Config_tests.cpp` test `Config/context_variables_typical_use_cases`.
//!
//! Deviation: processors are returned by value in Rust, so the C++ pointer
//! comparisons (same processor instance returned by the cache) are replaced
//! by comparing the addresses of the shared ops: a processor returned from
//! the cache shares its `Arc` ops with the cached one, while a newly built
//! processor has new ops.

mod config_common;

use config_common::*;
use ocio::*;
use std::sync::Arc;

fn same_instance(a: &Processor, b: &Processor) -> bool {
    a.ops().len() == b.ops().len()
        && !a.ops().is_empty()
        && a.ops()
            .iter()
            .zip(b.ops())
            .all(|(x, y)| std::ptr::addr_eq(Arc::as_ptr(x), Arc::as_ptr(y)))
}

fn cs(cfg: &Config, ctx: Option<&Context>, src: &str, dst: &str) -> Processor {
    match ctx {
        Some(c) => cfg.get_processor_with_context_names(c, src, dst).unwrap(),
        None => cfg.get_processor(src, dst).unwrap(),
    }
}

fn dv(cfg: &Config, ctx: Option<&Context>, view: &str) -> Processor {
    let c = ctx
        .cloned()
        .unwrap_or_else(|| cfg.current_context().clone());
    cfg.get_display_view_processor_with_context(
        &c,
        "cs1",
        "disp1",
        view,
        TransformDirection::Forward,
    )
    .unwrap()
}

fn load(s: &str) -> Config {
    let cfg = Config::create_from_str(s).unwrap().create_editable_copy();
    cfg.validate().unwrap();
    cfg
}

#[test]
#[ignore = "needs-merge"]
fn config_context_variables_typical_use_cases() {
    let _lock = env_lock();
    let dir = data_file("");
    for v in [
        "FILE",
        "ENV",
        "SHOW",
        "SHOT",
        "TRANSFORM_DIR",
        "PATH_1",
        "PATH_2",
        "CCPREFIX",
        "CCNUM",
    ] {
        std::env::remove_var(v);
    }
    let _g = EnvGuard::set(OCIO_DISABLE_CACHE_FALLBACK, None);

    // Case 1 - No context variables used in the config.
    {
        let config = format!(
            "ocio_profile_version: 2\n\nsearch_path: {dir}\n\nroles:\n  default: cs1\n\ndisplays:\n  disp1:\n    - !<View> {{name: view1, colorspace: cs2}}\n    - !<View> {{name: view2, colorspace: cs3}}\n\ncolorspaces:\n  - !<ColorSpace>\n    name: cs1\n\n  - !<ColorSpace>\n    name: cs2\n    from_scene_reference: !<FileTransform> {{src: exposure_contrast_linear.ctf}}\n\n  - !<ColorSpace>\n    name: cs3\n    from_scene_reference: !<MatrixTransform> {{offset: [0.11, 0.12, 0.13, 0]}}\n"
        );
        let cfg = load(&config);
        assert!(same_instance(
            &cs(&cfg, None, "cs1", "cs2"),
            &cs(&cfg, None, "cs1", "cs2")
        ));
        assert!(same_instance(
            &cs(&cfg, None, "cs1", "cs3"),
            &cs(&cfg, None, "cs1", "cs3")
        ));
        assert!(same_instance(
            &dv(&cfg, None, "view1"),
            &dv(&cfg, None, "view1")
        ));
        assert!(same_instance(
            &dv(&cfg, None, "view2"),
            &dv(&cfg, None, "view2")
        ));

        let mut ctx = cfg.current_context().clone();
        for _ in 0..2 {
            let c = Some(&ctx);
            assert!(same_instance(
                &cs(&cfg, c, "cs1", "cs2"),
                &cs(&cfg, c, "cs1", "cs2")
            ));
            assert!(same_instance(
                &cs(&cfg, c, "cs1", "cs3"),
                &cs(&cfg, c, "cs1", "cs3")
            ));
            assert!(same_instance(
                &cs(&cfg, c, "cs1", "cs2"),
                &cs(&cfg, None, "cs1", "cs2")
            ));
            assert!(same_instance(&dv(&cfg, c, "view1"), &dv(&cfg, c, "view1")));
            assert!(same_instance(&dv(&cfg, c, "view2"), &dv(&cfg, c, "view2")));
            assert!(same_instance(
                &dv(&cfg, c, "view1"),
                &dv(&cfg, None, "view1")
            ));
            // Add an unused context variable in the context. The cache is still used.
            ctx.set_string_var("ENV", Some("xxx"));
        }
    }

    // Case 2 - Context variables used anywhere but in the search_path.
    {
        let config = format!(
            "ocio_profile_version: 2\n\nenvironment: {{FILE: exposure_contrast_linear.ctf }}\n\nsearch_path: {dir}\n\nroles:\n  default: cs1\n\ndisplays:\n  disp1:\n    - !<View> {{name: view1, colorspace: cs2}}\n\ncolorspaces:\n  - !<ColorSpace>\n    name: cs1\n\n  - !<ColorSpace>\n    name: cs2\n    from_scene_reference: !<FileTransform> {{src: $FILE}}\n"
        );
        let cfg = load(&config);
        assert!(same_instance(
            &cs(&cfg, None, "cs1", "cs2"),
            &cs(&cfg, None, "cs1", "cs2")
        ));
        assert!(same_instance(
            &dv(&cfg, None, "view1"),
            &dv(&cfg, None, "view1")
        ));

        let mut ctx = cfg.current_context().clone();
        ctx.set_string_var("ENV", Some("xxx"));
        let c = Some(&ctx);
        assert!(same_instance(
            &cs(&cfg, c, "cs1", "cs2"),
            &cs(&cfg, c, "cs1", "cs2")
        ));
        assert!(same_instance(
            &cs(&cfg, c, "cs1", "cs2"),
            &cs(&cfg, None, "cs1", "cs2")
        ));
        assert!(same_instance(&dv(&cfg, c, "view1"), &dv(&cfg, c, "view1")));
        assert!(same_instance(
            &dv(&cfg, c, "view1"),
            &dv(&cfg, None, "view1")
        ));

        // Change the value of the used context variable.
        ctx.set_string_var("FILE", Some("exposure_contrast_log.ctf"));
        let c = Some(&ctx);
        assert!(same_instance(
            &cs(&cfg, c, "cs1", "cs2"),
            &cs(&cfg, c, "cs1", "cs2")
        ));
        assert!(same_instance(
            &cs(&cfg, None, "cs1", "cs2"),
            &cs(&cfg, None, "cs1", "cs2")
        ));
        assert!(!same_instance(
            &cs(&cfg, c, "cs1", "cs2"),
            &cs(&cfg, None, "cs1", "cs2")
        ));
        assert!(same_instance(&dv(&cfg, c, "view1"), &dv(&cfg, c, "view1")));
        assert!(same_instance(
            &dv(&cfg, None, "view1"),
            &dv(&cfg, None, "view1")
        ));
        assert!(!same_instance(
            &dv(&cfg, c, "view1"),
            &dv(&cfg, None, "view1")
        ));
    }

    // Case 3 - Context variables used on the search_path, but that variable is unchanged.
    let case3 = format!(
        "ocio_profile_version: 2\n\nenvironment:\n  SHOW: {dir}\n  SHOT: exposure_contrast_linear.ctf\n\nsearch_path: $SHOW\n\nroles:\n  default: cs1\n\ndisplays:\n  disp1:\n    - !<View> {{name: view1, colorspace: cs2}}\n\ncolorspaces:\n  - !<ColorSpace>\n    name: cs1\n\n  - !<ColorSpace>\n    name: cs2\n    from_scene_reference: !<FileTransform> {{src: exposure_contrast_linear.ctf}}\n\n  - !<ColorSpace>\n    name: cs3\n    from_scene_reference: !<FileTransform> {{src: $SHOT}}\n"
    );
    {
        let cfg = load(&case3);
        let mut ctx = cfg.current_context().clone();
        ctx.set_string_var("SHOT", Some("lut1d_green.ctf"));
        assert!(same_instance(
            &cs(&cfg, None, "cs1", "cs2"),
            &cs(&cfg, Some(&ctx), "cs1", "cs2")
        ));
        assert!(same_instance(
            &cs(&cfg, None, "cs1", "cs2"),
            &cs(&cfg, None, "cs1", "cs3")
        ));
    }
    {
        let _fb = EnvGuard::set(OCIO_DISABLE_CACHE_FALLBACK, Some("1"));
        let cfg = load(&case3);
        assert!(!same_instance(
            &cs(&cfg, None, "cs1", "cs2"),
            &cs(&cfg, None, "cs1", "cs3")
        ));
        let mut ctx = cfg.current_context().clone();
        ctx.set_string_var("SHOT", Some("lut1d_green.ctf"));
        assert!(same_instance(
            &cs(&cfg, None, "cs1", "cs2"),
            &cs(&cfg, Some(&ctx), "cs1", "cs2")
        ));
    }

    // Case 4 - Context vars used in the search_path and they are changing per shot, but no
    // FileTransforms are used.
    let case4 = format!(
        "ocio_profile_version: 2\n\nenvironment:\n  SHOW: {dir}\n\nsearch_path: $SHOW\n\nroles:\n  default: cs1\n\ndisplays:\n  disp1:\n    - !<View> {{name: view1, colorspace: cs2}}\n\ncolorspaces:\n  - !<ColorSpace>\n    name: cs1\n\n  - !<ColorSpace>\n    name: cs2\n    from_scene_reference: !<MatrixTransform> {{offset: [0.11, 0.12, 0.13, 0]}}\n"
    );
    for disable in [false, true] {
        let _fb = EnvGuard::set(
            OCIO_DISABLE_CACHE_FALLBACK,
            if disable { Some("1") } else { None },
        );
        let cfg = load(&case4);
        let mut ctx = cfg.current_context().clone();
        ctx.set_string_var("SHOW", Some("/some/arbitrary/path"));
        assert_ne!(cfg.current_context().cache_id(), ctx.cache_id());
        assert!(same_instance(
            &cs(&cfg, None, "cs1", "cs2"),
            &cs(&cfg, Some(&ctx), "cs1", "cs2")
        ));
    }

    // Case 5 - Context vars in the search_path and they are changing but the changed vars are
    // not used to resolve the file transform.
    let case5 = format!(
        "ocio_profile_version: 2\n\nenvironment:\n  TRANSFORM_DIR: {dir}\n\nsearch_path:\n  - /bogus/unknown/path\n  - $TRANSFORM_DIR\n  - $SHOT\n\nroles:\n  default: cs1\n\ndisplays:\n  disp1:\n    - !<View> {{name: view1, colorspace: cs2}}\n\ncolorspaces:\n  - !<ColorSpace>\n    name: cs1\n\n  - !<ColorSpace>\n    name: cs2\n    from_scene_reference: !<FileTransform> {{src: exposure_contrast_linear.ctf}}\n"
    );
    {
        let cfg = load(&case5);
        let mut ctx1 = cfg.current_context().clone();
        ctx1.set_string_var("SHOT", Some("/unknow/path/for_path_1"));
        let mut ctx2 = cfg.current_context().clone();
        ctx2.set_string_var("SHOT", Some("/unknow/path/for_path_2"));
        assert!(same_instance(
            &cs(&cfg, Some(&ctx1), "cs1", "cs2"),
            &cs(&cfg, Some(&ctx2), "cs1", "cs2")
        ));
        {
            let _fb = EnvGuard::set(OCIO_DISABLE_CACHE_FALLBACK, Some("1"));
            let cfg = load(&case5);
            assert!(!same_instance(
                &cs(&cfg, Some(&ctx1), "cs1", "cs2"),
                &cs(&cfg, Some(&ctx2), "cs1", "cs2")
            ));
        }
    }

    // Case 6 - Context vars in the search_path, the vars on the path to the file do change, but
    // the resulting file is the same.
    let case6 = format!(
        "ocio_profile_version: 2\n\nenvironment:\n  PATH_1: {dir}\n  PATH_2: {dir}\n\nsearch_path:\n  - $PATH_1\n  - $PATH_2\n\nroles:\n  default: cs1\n\ndisplays:\n  disp1:\n    - !<View> {{name: view1, colorspace: cs2}}\n\ncolorspaces:\n  - !<ColorSpace>\n    name: cs1\n\n  - !<ColorSpace>\n    name: cs2\n    from_scene_reference: !<FileTransform> {{src: exposure_contrast_linear.ctf}}\n"
    );
    {
        let cfg = load(&case6);
        let mut ctx1 = cfg.current_context().clone();
        ctx1.set_string_var("PATH_1", Some("/unknow/path/for_path_1"));
        let mut ctx2 = cfg.current_context().clone();
        ctx2.set_string_var("PATH_2", Some("/unknow/path/for_path_2"));
        assert_ne!(ctx1.cache_id(), ctx2.cache_id());
        assert!(same_instance(
            &cs(&cfg, Some(&ctx1), "cs1", "cs2"),
            &cs(&cfg, Some(&ctx2), "cs1", "cs2")
        ));
        {
            let _fb = EnvGuard::set(OCIO_DISABLE_CACHE_FALLBACK, Some("1"));
            let cfg = load(&case6);
            assert!(!same_instance(
                &cs(&cfg, Some(&ctx1), "cs1", "cs2"),
                &cs(&cfg, Some(&ctx2), "cs1", "cs2")
            ));
        }
    }

    // Case 7 - Context variables in the FileTransform's CCCID.
    {
        let config = format!(
            "ocio_profile_version: 2\n\nenvironment:\n  CCPREFIX: cc\n\nsearch_path: {dir}\n\nroles:\n  default: cs1\n\ndisplays:\n  disp1:\n    - !<View> {{name: view1, colorspace: cs2}}\n\ncolorspaces:\n  - !<ColorSpace>\n    name: cs1\n\n  - !<ColorSpace>\n    name: cs2\n    from_scene_reference: !<FileTransform> {{src: cdl_test1.ccc, cccid: $CCPREFIX00$CCNUM}}\n"
        );
        let cfg = load(&config);
        let t = cfg
            .get_color_space("cs2")
            .unwrap()
            .transform(ColorSpaceDirection::FromReference)
            .unwrap()
            .clone();
        let mut ctx = cfg.current_context().clone();
        let mut procs = Vec::new();
        for n in ["01", "02", "03"] {
            ctx.set_string_var("CCNUM", Some(n));
            procs.push(
                cfg.get_processor_with_context(&ctx, &t, TransformDirection::Forward)
                    .unwrap(),
            );
        }
        assert!(!same_instance(&procs[0], &procs[1]));
        assert!(!same_instance(&procs[0], &procs[2]));
        assert!(!same_instance(&procs[1], &procs[2]));
    }
}
