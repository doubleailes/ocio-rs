//! Port of `ViewTransform_tests.cpp`.

use ocio::config::ViewTransform;
use ocio::ReferenceSpaceType;

#[test]
fn view_transform_basic() {
    let mut vt = ViewTransform::new(ReferenceSpaceType::Scene);
    assert_eq!(vt.reference_space_type(), ReferenceSpaceType::Scene);
    assert_eq!(vt.name(), "");
    assert_eq!(vt.family(), "");
    assert_eq!(vt.description(), "");
    assert_eq!(vt.num_categories(), 0);
    assert_eq!(vt.interchange_attribute("amf_transform_ids").unwrap(), "");

    vt.set_name("name");
    assert_eq!(vt.name(), "name");
    vt.set_family("family");
    assert_eq!(vt.family(), "family");
    vt.set_description("description");
    assert_eq!(vt.description(), "description");
    vt.set_interchange_attribute("amf_transform_ids", "amf_text")
        .unwrap();
    assert_eq!(
        vt.interchange_attribute("amf_transform_ids").unwrap(),
        "amf_text"
    );
    assert_eq!(vt.interchange_attributes().len(), 1);

    assert!(!vt.has_category("linear"));
    assert!(!vt.has_category("rendering"));
    assert!(!vt.has_category("log"));

    vt.add_category("linear");
    vt.add_category("rendering");
    assert_eq!(vt.num_categories(), 2);
    assert!(vt.has_category("linear"));
    assert!(vt.has_category("rendering"));
    assert!(!vt.has_category("log"));
    assert_eq!(vt.category(0), Some("linear"));
    assert_eq!(vt.category(1), Some("rendering"));
    assert_eq!(vt.category(2), None);

    vt.remove_category("linear");
    assert_eq!(vt.num_categories(), 1);
    assert!(!vt.has_category("linear"));
    assert!(vt.has_category("rendering"));

    vt.remove_category("log");
    assert_eq!(vt.num_categories(), 1);
    assert!(vt.has_category("rendering"));

    vt.clear_categories();
    assert_eq!(vt.num_categories(), 0);

    let vtd = ViewTransform::new(ReferenceSpaceType::Display);
    assert_eq!(vtd.reference_space_type(), ReferenceSpaceType::Display);
}
