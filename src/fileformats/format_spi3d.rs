//! `spi3d` file format (TODO: port from OCIO).

use super::{bake_capability, capability, FileFormat, FormatInfo, StubFormat};

pub(crate) fn create() -> Box<dyn FileFormat> {
    Box::new(StubFormat(vec![
        FormatInfo { name: "spi3d", extension: "spi3d", capabilities: capability::READ | capability::BAKE, bake_capabilities: bake_capability::LUT3D },
    ]))
}
