//! `3dl` file format (TODO: port from OCIO).

use super::{bake_capability, capability, FileFormat, FormatInfo, StubFormat};

pub(crate) fn create() -> Box<dyn FileFormat> {
    Box::new(StubFormat(vec![
        FormatInfo { name: "flame", extension: "3dl", capabilities: capability::READ | capability::BAKE, bake_capabilities: bake_capability::LUT3D },
        FormatInfo { name: "lustre", extension: "3dl", capabilities: capability::READ | capability::BAKE, bake_capabilities: bake_capability::LUT3D },
    ]))
}
