//! Port of `OCIOZArchive_tests.cpp`.

mod config_common;

use config_common::*;
use ocio::config::archive::extract_ocioz_archive;
use ocio::config::ColorSpace;
use ocio::*;
use std::path::PathBuf;

/// A temporary directory removed when dropped.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let unique = format!(
            "ocio-rs-test-{}-{}-{:?}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        );
        let p = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }
    fn path(&self) -> String {
        self.0.to_string_lossy().into_owned()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const IS_ARCHIVABLE_CONFIG: &str = "ocio_profile_version: 2\n\
\n\
search_path:\n\
\x20 - abc\n\
\x20 - def\n\
environment:\n\
\x20 MYLUT: exposure_contrast_linear.ctf\n\
\n\
roles:\n\
\x20 default: cs1\n\
\n\
displays:\n\
\x20 disp1:\n\
\x20   - !<View> {name: view1, colorspace: cs2}\n\
\n\
colorspaces:\n\
\x20 - !<ColorSpace>\n\
\x20   name: cs1\n\
\n\
\x20 - !<ColorSpace>\n\
\x20   name: cs2\n\
\x20   from_scene_reference: !<FileTransform> {src: ./$MYLUT}\n";

#[test]
fn ocioz_is_config_archivable() {
    let mut cfg = Config::create_from_str(IS_ARCHIVABLE_CONFIG)
        .unwrap()
        .create_editable_copy();
    cfg.set_working_dir("/fake_working_dir");
    cfg.validate().unwrap();

    // Legal search paths.
    for sp in [
        "luts",
        "luts/myluts1",
        r"luts\myluts1",
        "./myLuts",
        r".\myLuts",
        "./$SHOT/myluts",
        r".\$SHOT\myluts",
        "luts/$SHOT",
        "luts/$SHOT/luts1",
        r"luts\$SHOT",
        r"luts\$SHOT\luts1",
    ] {
        cfg.set_search_path(sp);
        assert!(cfg.is_archivable(), "search path {sp:?}");
    }
    // Illegal search paths.
    for sp in [
        "luts:../luts",
        r"luts:..\myLuts",
        "luts:$SHOT",
        "luts:/luts",
        "luts:/$SHOT",
    ] {
        cfg.set_search_path(sp);
        assert!(!cfg.is_archivable(), "search path {sp:?}");
    }

    cfg.clear_search_paths();

    let mut check_ft = |path: &str, archivable: bool| {
        let full = ocio::path_utils::join(path, "fake_lut.clf");
        let ft: Transform = FileTransform::new(&full).into();
        let mut cs = ColorSpace::default();
        cs.set_name("csTest");
        cs.set_transform(Some(ft), ColorSpaceDirection::ToReference);
        cfg.add_color_space(&cs).unwrap();
        assert_eq!(
            cfg.is_archivable(),
            archivable,
            "file transform path {full:?}"
        );
        cfg.remove_color_space("csTest");
    };

    for p in [
        "luts",
        "luts/myluts1",
        r"luts\myluts1",
        "./myLuts",
        r".\myLuts",
        "./$SHOT/myluts",
        r".\$SHOT\myluts",
        "luts/$SHOT",
        "luts/$SHOT/luts1",
        r"luts\$SHOT",
        r"luts\$SHOT\luts1",
    ] {
        check_ft(p, true);
    }
    for p in ["../luts", r"..\myLuts", "$SHOT", "/luts", "/$SHOT"] {
        check_ft(p, false);
    }
}

fn archive_path(name: &str) -> String {
    data_file(&format!("configs/context_test1/{name}"))
}

fn first_matrix_value(proc: &Processor) -> f64 {
    let group = proc.create_group_transform();
    match group.transforms.first() {
        Some(Transform::Matrix(m)) => m.matrix[0],
        t => panic!("expected a matrix, got {t:?}"),
    }
}

#[test]
fn ocioz_load_archives() {
    for name in ["context_test1_windows.ocioz", "context_test1_linux.ocioz"] {
        let cfg = Config::create_from_file(&archive_path(name)).unwrap();
        cfg.validate().unwrap();
        assert!(cfg.ocioz_archive().is_some());
    }
}

#[test]
#[ignore = "needs-merge"]
fn ocioz_context_test_for_search_paths_and_filetransform_source_path() {
    for name in ["context_test1_windows.ocioz", "context_test1_linux.ocioz"] {
        let cfg = Config::create_from_file(&archive_path(name))
            .unwrap()
            .create_editable_copy();
        cfg.validate().unwrap();
        let mut ctx = cfg.current_context().clone();
        for v in ["SHOT", "LUT_PATH", "CAMERA", "CCCID"] {
            ctx.set_string_var(v, Some("none"));
        }

        let value = |ctx: &Context, src: &str| {
            let p = cfg
                .get_processor_with_context_names(ctx, src, "reference")
                .unwrap();
            first_matrix_value(&p)
        };

        assert_eq!(value(&ctx, "shot1_lut1_cs"), 10.0);
        assert_eq!(value(&ctx, "shot2_lut1_cs"), 20.0);
        assert_eq!(value(&ctx, "shot2_lut2_cs"), 2.0);
        assert_eq!(value(&ctx, "lut3_cs"), 3.0);

        ctx.set_string_var("LUT_PATH", Some("shot3/lut1.clf"));
        assert_eq!(value(&ctx, "lut_path_cs"), 30.0);

        ctx.set_string_var("SHOT", Some("."));
        assert_eq!(value(&ctx, "plain_lut1_cs"), 5.0);
        ctx.set_string_var("SHOT", Some("shot2"));
        assert_eq!(value(&ctx, "plain_lut1_cs"), 20.0);
        ctx.set_string_var("SHOT", Some("no_shot"));
        assert_eq!(value(&ctx, "plain_lut1_cs"), 10.0);

        ctx.set_string_var("SHOT", Some("no_shot"));
        match cfg.get_processor_with_context_names(&ctx, "lut4_cs", "reference") {
            Err(e) => assert!(matches!(e, Error::MissingFile(_)), "{e}"),
            Ok(_) => panic!("expected a missing file error"),
        }

        ctx.set_string_var("SHOT", Some("shot4"));
        assert_eq!(value(&ctx, "lut4_cs"), 4.0);

        // File transform source is an absolute path, not in the archive.
        let t: Transform = FileTransform::new(&data_file("matrix_example4x4.ctf")).into();
        let p = cfg
            .get_processor_for_transform(&t, TransformDirection::Forward)
            .unwrap();
        assert_eq!(first_matrix_value(&p), 3.24);

        // File transform source is an abs path but doesn't exist anywhere.
        let t: Transform = FileTransform::new(&data_file("missing.ctf")).into();
        assert!(cfg
            .get_processor_for_transform(&t, TransformDirection::Forward)
            .is_err());
    }
}

#[test]
fn ocioz_archive_config_and_compare_to_original_no_processor() {
    let config_path = archive_path("config.ocio");
    let _lock = env_lock();
    let _guard = EnvGuard::set("OCIO", Some(&config_path));

    let from_file = Config::create_from_env().unwrap();
    from_file.validate().unwrap();

    let data = from_file.archive().unwrap();
    assert_eq!(data[0], b'P');
    assert_eq!(data[1], b'K');

    let dir = TempDir::new("archive");
    let archive_file = format!("{}/archive.ocioz", dir.path());
    std::fs::write(&archive_file, &data).unwrap();

    let from_archive = Config::create_from_file(&archive_file).unwrap();
    from_archive.validate().unwrap();

    assert_eq!(
        from_file.cache_id_with_context(None),
        from_archive.cache_id_with_context(None)
    );
    assert_eq!(
        from_file.serialize().unwrap(),
        from_archive.serialize().unwrap()
    );
}

#[test]
#[ignore = "needs-merge"]
fn ocioz_archive_config_and_compare_to_original() {
    let config_path = archive_path("config.ocio");
    let _lock = env_lock();
    let _guard = EnvGuard::set("OCIO", Some(&config_path));

    let from_file = Config::create_from_env().unwrap();
    let data = from_file.archive().unwrap();
    let dir = TempDir::new("archive_proc");
    let archive_file = format!("{}/archive.ocioz", dir.path());
    std::fs::write(&archive_file, &data).unwrap();
    let from_archive = Config::create_from_file(&archive_file).unwrap();

    let p1 = from_file
        .get_processor("plain_lut1_cs", "shot1_lut1_cs")
        .unwrap();
    let p2 = from_archive
        .get_processor("plain_lut1_cs", "shot1_lut1_cs")
        .unwrap();
    assert_eq!(p1.cache_id(), p2.cache_id());
}

#[test]
fn ocioz_extract_config_and_compare_to_original_no_processor() {
    let archive = archive_path("context_test1_windows.ocioz");
    let from_archive = Config::create_from_file(&archive).unwrap();
    from_archive.validate().unwrap();

    let dir = TempDir::new("context_test1");
    extract_ocioz_archive(&archive, &dir.path()).unwrap();

    let extracted = Config::create_from_file(&format!("{}/config.ocio", dir.path())).unwrap();
    extracted.validate().unwrap();

    assert_eq!(
        from_archive.cache_id_with_context(None),
        extracted.cache_id_with_context(None)
    );
    assert_eq!(
        from_archive.serialize().unwrap(),
        extracted.serialize().unwrap()
    );
}

#[test]
#[ignore = "needs-merge"]
fn ocioz_extract_config_and_compare_to_original() {
    let archive = archive_path("context_test1_windows.ocioz");
    let from_archive = Config::create_from_file(&archive).unwrap();
    let dir = TempDir::new("context_test1_proc");
    extract_ocioz_archive(&archive, &dir.path()).unwrap();
    let extracted = Config::create_from_file(&format!("{}/config.ocio", dir.path())).unwrap();

    let p1 = from_archive
        .get_processor("plain_lut1_cs", "shot1_lut1_cs")
        .unwrap();
    let p2 = extracted
        .get_processor("plain_lut1_cs", "shot1_lut1_cs")
        .unwrap();
    assert_eq!(p1.cache_id(), p2.cache_id());
}
