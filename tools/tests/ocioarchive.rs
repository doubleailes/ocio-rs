//! Integration tests of `ocioarchive`.

mod common;
use common::*;
use std::process::Command;

const EXE: &str = env!("CARGO_BIN_EXE_ocioarchive");

#[test]
fn usage() {
    let r = run(EXE, &[]);
    assert_eq!(r.code, 0);
    assert!(r.stdout.starts_with(
        "ocioarchive -- Archive a config and its LUT files or extract a config archive. \n"
    ));
    assert!(r
        .stdout
        .contains("    --extract     Extract an OCIOZ config archive\n"));

    let r = run(EXE, &["-h", "x"]);
    assert_eq!(r.code, 0);

    let r = run(EXE, &["--bogus"]);
    assert_eq!(r.code, 1);
    assert_eq!(r.stderr, "Invalid option \"--bogus\"\n");
}

#[test]
fn archive_list_extract() {
    let dir = temp_dir("ocioarchive");
    let config = write_test_config(&dir, false);
    let work = temp_dir("ocioarchive_work");

    // Archive (the extension is added).
    let r: Run = Command::new(EXE)
        .current_dir(&work)
        .args(["myarchive", "--iconfig", &config])
        .env_remove("OCIO")
        .output()
        .unwrap()
        .into();
    assert_eq!(r.code, 0, "{}", r.stderr);
    let archive = work.join("myarchive.ocioz");
    assert!(archive.exists());

    // The archive can be loaded.
    let c = ocio::Config::create_from_file(archive.to_str().unwrap()).unwrap();
    assert_eq!(c.num_color_spaces(), 4);
    let p = c.get_processor("lin", "film").unwrap();
    let mut px = [0.5f32, 0.5, 0.5];
    p.default_cpu_processor().apply_rgb(&mut px);
    assert!(px[0].abs() < 1e-6 && (px[1] - 0.5).abs() < 1e-3);

    // List.
    let r = run(EXE, &["--list", archive.to_str().unwrap()]);
    assert_eq!(r.code, 0);
    assert!(r.stdout.starts_with(
        "\nThe archive contains the following files:\n\n      Date     Time  CRC-32     Name\n      ----     ----  ------     ----\n"
    ));
    assert!(r.stdout.contains("   config.ocio\n"));
    assert!(r.stdout.contains("   luts/lut1d_green.ctf\n"));

    // Extract into the default directory (the archive name without extension).
    let r: Run = Command::new(EXE)
        .current_dir(&work)
        .args(["--extract", "myarchive.ocioz"])
        .output()
        .unwrap()
        .into();
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(r.stdout, "myarchive.ocioz has been extracted.\n");
    assert!(work.join("myarchive/config.ocio").exists());
    assert!(work.join("myarchive/luts/lut1d_green.ctf").exists());

    // Extract into a given directory.
    let dest = work.join("sub/dir");
    let r = run(
        EXE,
        &[
            "--extract",
            archive.to_str().unwrap(),
            "--dir",
            dest.to_str().unwrap(),
        ],
    );
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(dest.join("luts/lut1d_green.ctf").exists());
    let c = ocio::Config::create_from_file(dest.join("config.ocio").to_str().unwrap()).unwrap();
    assert_eq!(c.num_color_spaces(), 4);

    // Archive from $OCIO, with the extension already present.
    let r: Run = Command::new(EXE)
        .current_dir(&work)
        .args(["second.ocioz"])
        .env("OCIO", &config)
        .output()
        .unwrap()
        .into();
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(r.stdout, format!("Archiving $OCIO={config}\n"));
    assert!(work.join("second.ocioz").exists());
}

#[test]
fn errors() {
    let r = run(EXE, &["myarchive"]);
    assert_eq!(r.code, 1);
    assert_eq!(
        r.stderr,
        "ERROR: You must specify an input OCIO configuration.\n"
    );

    let r = run(EXE, &["a", "b"]);
    assert_eq!(r.code, 1);
    assert_eq!(
        r.stderr,
        "ERROR: Missing the name of the archive to create.\n"
    );

    let r = run(EXE, &["myarchive", "--iconfig", "/does/not/exist.ocio"]);
    assert_eq!(r.code, 1);
    assert_eq!(
        r.stderr,
        "ERROR: Could not load config: /does/not/exist.ocio\n"
    );

    let r = run(EXE, &["--extract", "a", "b"]);
    assert_eq!(r.code, 1);
    assert_eq!(
        r.stderr,
        "ERROR: Missing the name of the archive to extract.\n"
    );

    let r = run(EXE, &["--extract", "--list", "a"]);
    assert_eq!(r.code, 1);
    assert_eq!(
        r.stderr,
        "Archive, extract, and/or list functions may not be used at the same time.\n"
    );

    let r = run(EXE, &["--list", "/does/not/exist.ocioz"]);
    assert_eq!(r.code, 1);
    assert_eq!(r.stderr, "ERROR: File not found: /does/not/exist.ocioz\n");

    let empty = data_file("configs/ocioz_archive_configs/empty.ocioz");
    let r = run(EXE, &["--list", &empty]);
    assert_eq!(r.code, 1);

    // A config with an absolute LUT path is not archivable.
    let dir = temp_dir("ocioarchive_errors");
    let text = format!(
        "ocio_profile_version: 2\n\nroles:\n  default: raw\n\ndisplays:\n  sRGB:\n    - !<View> {{name: Raw, colorspace: raw}}\n\ncolorspaces:\n  - !<ColorSpace>\n    name: raw\n    from_scene_reference: !<FileTransform> {{src: {}}}\n",
        data_file("lut1d_green.ctf")
    );
    let config = dir.join("config.ocio");
    std::fs::write(&config, text).unwrap();
    let out = dir.join("out");
    let r = run(
        EXE,
        &[out.to_str().unwrap(), "--iconfig", config.to_str().unwrap()],
    );
    assert_eq!(r.code, 1);
    assert_eq!(r.stderr, "Config is not archivable.\n");
}
