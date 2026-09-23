//! `lut` file format (TODO: port from OCIO).

use super::{bake_capability, capability, FileFormat, FormatInfo, StubFormat};

pub(crate) fn create() -> Box<dyn FileFormat> {
    Box::new(StubFormat(vec![
        FormatInfo { name: "Discreet 1D LUT", extension: "lut", capabilities: capability::READ, bake_capabilities: bake_capability::NONE },
    ]))
}
