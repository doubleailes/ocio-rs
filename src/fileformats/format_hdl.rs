//! `lut` file format (TODO: port from OCIO).

use super::{bake_capability, capability, FileFormat, FormatInfo, StubFormat};

pub(crate) fn create() -> Box<dyn FileFormat> {
    Box::new(StubFormat(vec![
        FormatInfo { name: "houdini", extension: "lut", capabilities: capability::READ | capability::BAKE, bake_capabilities: bake_capability::LUT3D | bake_capability::LUT1D | bake_capability::LUT1D_3D },
    ]))
}
