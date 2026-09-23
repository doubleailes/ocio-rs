//! `spi1d` file format (TODO: port from OCIO).

use super::{bake_capability, capability, FileFormat, FormatInfo, StubFormat};

pub(crate) fn create() -> Box<dyn FileFormat> {
    Box::new(StubFormat(vec![
        FormatInfo { name: "spi1d", extension: "spi1d", capabilities: capability::READ | capability::BAKE, bake_capabilities: bake_capability::LUT1D },
    ]))
}
