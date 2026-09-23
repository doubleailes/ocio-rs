//! `itx` file format (TODO: port from OCIO).

use super::{bake_capability, capability, FileFormat, FormatInfo, StubFormat};

pub(crate) fn create() -> Box<dyn FileFormat> {
    Box::new(StubFormat(vec![
        FormatInfo { name: "iridas_itx", extension: "itx", capabilities: capability::READ | capability::BAKE, bake_capabilities: bake_capability::LUT3D },
    ]))
}
