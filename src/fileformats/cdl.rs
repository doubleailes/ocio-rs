//! CC / CCC / CDL (ASC CDL XML) file formats (TODO: port from OCIO).

use super::{
    bake_capability, capability, FileFormat, FormatInfo, StubFormat, FILEFORMAT_COLOR_CORRECTION,
    FILEFORMAT_COLOR_CORRECTION_COLLECTION, FILEFORMAT_COLOR_DECISION_LIST,
};

pub(crate) fn create_cc() -> Box<dyn FileFormat> {
    Box::new(StubFormat(vec![FormatInfo { name: FILEFORMAT_COLOR_CORRECTION, extension: "cc", capabilities: capability::READ | capability::WRITE, bake_capabilities: bake_capability::NONE }]))
}

pub(crate) fn create_ccc() -> Box<dyn FileFormat> {
    Box::new(StubFormat(vec![FormatInfo { name: FILEFORMAT_COLOR_CORRECTION_COLLECTION, extension: "ccc", capabilities: capability::READ | capability::WRITE, bake_capabilities: bake_capability::NONE }]))
}

pub(crate) fn create_cdl() -> Box<dyn FileFormat> {
    Box::new(StubFormat(vec![FormatInfo { name: FILEFORMAT_COLOR_DECISION_LIST, extension: "cdl", capabilities: capability::READ | capability::WRITE, bake_capabilities: bake_capability::NONE }]))
}
