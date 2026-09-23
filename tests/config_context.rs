//! Port of `Context_tests.cpp` (the file-resolution part used by `Config`).

use ocio::path_utils::normpath;
use ocio::Context;

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

#[test]
fn context_search_paths() {
    let mut con = Context::new();
    assert_eq!(con.num_search_paths(), 0);
    assert_eq!(con.search_path(), "");
    assert_eq!(con.search_path_by_index(42), None);

    con.add_search_path("");
    assert_eq!(con.num_search_paths(), 0);

    con.add_search_path("First");
    assert_eq!(con.num_search_paths(), 1);
    assert_eq!(con.search_path(), "First");
    assert_eq!(con.search_path_by_index(0), Some("First"));
    con.clear_search_paths();
    assert_eq!(con.num_search_paths(), 0);
    assert_eq!(con.search_path(), "");

    con.add_search_path("First");
    con.add_search_path("Second");
    assert_eq!(con.num_search_paths(), 2);
    assert_eq!(con.search_path(), "First:Second");
    assert_eq!(con.search_path_by_index(0), Some("First"));
    assert_eq!(con.search_path_by_index(1), Some("Second"));
    con.add_search_path("");
    assert_eq!(con.num_search_paths(), 2);

    con.set_search_path("First");
    assert_eq!(con.num_search_paths(), 1);
    assert_eq!(con.search_path(), "First");

    con.set_search_path("First:Second");
    assert_eq!(con.num_search_paths(), 2);
    assert_eq!(con.search_path(), "First:Second");
    assert_eq!(con.search_path_by_index(0), Some("First"));
    assert_eq!(con.search_path_by_index(1), Some("Second"));
}

#[test]
fn context_abs_path() {
    let contextpath = format!("{ROOT}/src/config/mod.rs");
    let mut con = Context::new();
    con.add_search_path(ROOT);
    con.set_string_var("non_abs", Some("src/config/mod.rs"));
    con.set_string_var("is_abs", Some(&contextpath));

    let r = con.resolve_file_location("${non_abs}").unwrap();
    assert_eq!(normpath(&r), normpath(&contextpath));
    let r = con.resolve_file_location("${is_abs}").unwrap();
    assert_eq!(r, normpath(&contextpath));
}

#[test]
fn context_var_search_path() {
    let mut con = Context::new();
    let contextpath = format!("{ROOT}/src/config/mod.rs");
    con.set_string_var("SOURCE_DIR", Some(ROOT));
    con.add_search_path("${SOURCE_DIR}/src/config");
    let r = con.resolve_file_location("mod.rs").unwrap();
    assert_eq!(normpath(&r), normpath(&contextpath));
}

#[test]
fn context_use_searchpaths() {
    let mut con = Context::new();
    let sp1 = format!("{ROOT}/src/config");
    let sp2 = format!("{ROOT}/tests/config_common");
    con.add_search_path(&sp1);
    con.add_search_path(&sp2);
    let r = con.resolve_file_location("look_parse.rs").unwrap();
    assert_eq!(normpath(&r), normpath(&format!("{sp1}/look_parse.rs")));
    // mod.rs is in both, the first search path wins.
    let r = con.resolve_file_location("mod.rs").unwrap();
    assert_eq!(normpath(&r), normpath(&format!("{sp1}/mod.rs")));
}

#[test]
fn context_use_searchpaths_workingdir() {
    let mut con = Context::new();
    con.set_working_dir(ROOT);
    con.add_search_path("src/config/yaml");
    con.add_search_path("tests/config_common");
    let r = con.resolve_file_location("emitter.rs").unwrap();
    assert_eq!(
        normpath(&r),
        normpath(&format!("{ROOT}/src/config/yaml/emitter.rs"))
    );
    let r = con.resolve_file_location("mod.rs").unwrap();
    assert_eq!(
        normpath(&r),
        normpath(&format!("{ROOT}/src/config/yaml/mod.rs"))
    );
}

#[test]
fn context_string_vars() {
    let mut ctx1 = Context::new();
    ctx1.set_string_var("var1", Some("val1"));
    ctx1.set_string_var("var2", Some("val2"));
    let mut ctx2 = Context::new();
    ctx2.set_string_var("var1", Some("val11"));
    ctx2.set_string_var("var3", Some("val3"));
    ctx1.add_string_vars(&ctx2);
    assert_eq!(ctx1.num_string_vars(), 3);
    assert_eq!(ctx1.string_var_name_by_index(0), Some("var1"));
    assert_eq!(ctx1.string_var_by_index(0), Some("val11"));
    assert_eq!(ctx1.string_var_name_by_index(1), Some("var2"));
    assert_eq!(ctx1.string_var_by_index(1), Some("val2"));
    assert_eq!(ctx1.string_var_name_by_index(2), Some("var3"));
    assert_eq!(ctx1.string_var_by_index(2), Some("val3"));
}
