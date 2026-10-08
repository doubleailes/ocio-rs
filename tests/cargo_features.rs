//! What the crate does when an optional cargo feature is off. With the
//! default features this file has no tests.

#[allow(dead_code)]
fn data_dir() -> String {
    concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/files").to_string()
}

#[cfg(not(feature = "file-formats"))]
mod without_file_formats {
    use super::data_dir;
    use ocio::*;

    /// A config whose `lut` colour space reads `src` from the test data.
    fn lut_config(src: &str) -> Config {
        Config::create_from_str(&format!(
            r#"ocio_profile_version: 2

search_path: {}

roles:
  default: raw

displays:
  sRGB:
    - !<View> {{name: Raw, colorspace: raw}}

colorspaces:
  - !<ColorSpace>
    name: raw

  - !<ColorSpace>
    name: lut
    to_scene_reference: !<FileTransform> {{src: {src}}}
"#,
            data_dir()
        ))
        .unwrap()
    }

    #[test]
    fn config_with_file_transform_loads() {
        let config = lut_config("lut1d_green.ctf");
        config.validate().unwrap();
        assert!(config.has_color_space("lut"));
        // Processors not needing the file are built.
        config.get_processor("raw", "raw").unwrap();
    }

    #[test]
    fn file_transform_processor_names_the_feature() {
        let config = lut_config("lut1d_green.ctf");
        let err = config.get_processor("lut", "raw").unwrap_err().to_string();
        assert!(err.contains("lut1d_green.ctf"), "{err}");
        assert!(err.contains("`file-formats` feature"), "{err}");
    }

    #[test]
    fn missing_file_is_reported_as_missing() {
        let config = lut_config("does_not_exist.ctf");
        let err = config.get_processor("lut", "raw").unwrap_err().to_string();
        assert!(err.contains("could not be located"), "{err}");
        assert!(!err.contains("`file-formats` feature"), "{err}");
    }

    #[test]
    fn file_transform_reports_no_formats() {
        assert_eq!(FileTransform::num_formats(), 0);
        assert_eq!(FileTransform::format_name_by_index(0), "");
        assert_eq!(FileTransform::format_extension_by_index(0), "");
        assert!(!FileTransform::is_format_extension_supported("ctf"));
    }

    #[test]
    fn file_transform_is_validated() {
        Transform::from(FileTransform::default())
            .validate()
            .unwrap_err();
        let ft = FileTransform {
            src: "lut1d_green.ctf".into(),
            ..Default::default()
        };
        Transform::from(ft).validate().unwrap();
    }

    #[test]
    fn cdl_from_file_names_the_feature() {
        let path = format!("{}/cdl_test1.cc", data_dir());
        let err = CdlTransform::create_from_file(&path, "")
            .unwrap_err()
            .to_string();
        assert!(err.contains("`file-formats` feature"), "{err}");
        let err = CdlTransform::create_group_from_file(&path)
            .unwrap_err()
            .to_string();
        assert!(err.contains("`file-formats` feature"), "{err}");
    }
}

#[cfg(not(feature = "ocioz"))]
#[test]
fn opening_an_archive_names_the_feature() {
    let path = format!(
        "{}/configs/context_test1/context_test1_linux.ocioz",
        data_dir()
    );
    let err = ocio::Config::create_from_file(&path)
        .unwrap_err()
        .to_string();
    assert!(err.contains("`ocioz` feature"), "{err}");
}

#[cfg(all(feature = "ocioz", not(feature = "file-formats")))]
#[test]
fn archive_leaves_out_the_luts() {
    let path = format!("{}/configs/context_test1/config.ocio", data_dir());
    let config = ocio::Config::create_from_file(&path).unwrap();
    let data = config.archive().unwrap();

    let file =
        std::env::temp_dir().join(format!("ocio-cargo-features-{}.ocioz", std::process::id()));
    std::fs::write(&file, &data).unwrap();
    let archive = ocio::config::archive::OciozArchive::open(&file.to_string_lossy());
    std::fs::remove_file(&file).unwrap();

    let files: Vec<String> = archive
        .unwrap()
        .entry_names()
        .filter(|n| !n.ends_with('/'))
        .map(String::from)
        .collect();
    assert_eq!(files, ["config.ocio"]);
}
