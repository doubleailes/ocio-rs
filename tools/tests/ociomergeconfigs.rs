//! Integration tests of `ociomergeconfigs`.

mod common;
use common::*;

const EXE: &str = env!("CARGO_BIN_EXE_ociomergeconfigs");

fn merge_file(name: &str) -> String {
    data_file(&format!("configs/mergeconfigs/{name}"))
}

#[test]
fn usage() {
    // The number of arguments is checked before the help flag (as in OCIO).
    let r = run(EXE, &["--help"]);
    assert_eq!(r.code, 1);
    assert_eq!(r.stderr, "ERROR: Expecting 1 arguments, found 0.\n");
    assert!(r.stdout.starts_with(
        "ociomergeconfigs -- Merge configs using an OCIOM file with merge parameters\n\n\
         Usage:\n    ociomergeconfigs [options] mergeFile.ociom --out mergedConfig.ocio\n\n\
         Options:\n    --out %s       Filepath to save the merged config\n"
    ));

    let r = run(EXE, &["-h", "file.ociom"]);
    assert_eq!(r.code, 0);
    assert!(r
        .stdout
        .contains("    --show-params  Display merger options from OCIOM file\n"));

    let r = run(EXE, &["--bogus", "file.ociom"]);
    assert_eq!(r.code, 1);
    assert!(r.stderr.starts_with("Invalid option \"--bogus\"\n"));
}

#[test]
fn merge_and_save() {
    let dir = temp_dir("ociomergeconfigs");
    let out = dir.join("merged.ocio").to_string_lossy().into_owned();

    let r = run(
        EXE,
        &[
            &merge_file("merged1/merged1.ociom"),
            "--validate",
            "--out",
            &out,
        ],
    );
    assert_eq!(r.code, 0, "{}{}", r.stdout, r.stderr);
    assert!(r.stdout.is_empty());
    // The merge warnings are logged.
    assert!(r.stderr.contains(
        "Merged color space 'ACES2065-1' has a conflict with alias 'aces' in color space 'ACEScg'"
    ));

    let merged = ocio::Config::create_from_file(&out).unwrap();
    assert_eq!(merged.name(), "Merged1");
    assert_eq!(merged.description(), "Basic merge with default strategy");
    assert_eq!(
        merged.num_color_spaces_filtered(
            ocio::SearchReferenceSpaceType::All,
            ocio::ColorSpaceVisibility::All
        ),
        7
    );
}

#[test]
fn show_options() {
    let ociom = merge_file("merged2/merged.ociom");

    let r = run(EXE, &[&ociom, "--show-params"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(r.stdout.starts_with(
        "********************\nMerger options\n********************\nociom_version: "
    ));
    assert!(r.stdout.ends_with("\n\n"));

    let r = run(EXE, &[&ociom, "--show-all", "--show-last"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(r
        .stdout
        .starts_with("*********************\nMerged Config 0\n*********************\n"));
    assert!(r
        .stdout
        .contains("*********************\nMerged Config 1\n*********************\n"));
    assert!(r.stdout.contains("\nname: Merged1\n"));
    assert!(r.stdout.contains("\nname: Merged2\n"));
    // --show-all takes priority.
    assert!(!r.stdout.contains("Last Merged Config"));

    let r = run(EXE, &[&ociom, "--show-last"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(r
        .stdout
        .starts_with("********************\nLast Merged Config\n********************\n"));
    assert!(r.stdout.contains("\nname: Merged2\n"));
    assert!(!r.stdout.contains("\nname: Merged1\n"));
}

#[test]
fn errors() {
    let r = run(EXE, &["/does/not/exist.ociom"]);
    assert_eq!(r.code, 1);
    assert_eq!(
        r.stdout,
        "Error could not read '/does/not/exist.ociom' merge options.\n"
    );

    // The configs referenced by the OCIOM file do not exist.
    let r = run(EXE, &[&merge_file("parser_test.ociom")]);
    assert_eq!(r.code, 1);
    assert_eq!(r.stderr, "Could not load the base or the input config");

    // A conflict is an error when requested.
    let dir = temp_dir("ociomergeconfigs_errors");
    for f in ["base1.ocio", "input1.ocio"] {
        std::fs::copy(merge_file(&format!("merged1/{f}")), dir.join(f)).unwrap();
    }
    let text = std::fs::read_to_string(merge_file("merged1/merged1.ociom"))
        .unwrap()
        .replace("error_on_conflict: false", "error_on_conflict: true");
    let ociom = dir.join("conflict.ociom");
    std::fs::write(&ociom, text).unwrap();
    let r = run(EXE, &[ociom.to_str().unwrap()]);
    assert_eq!(r.code, 1);
    assert!(!r.stderr.is_empty());
}
