//! `look` file format (TODO: port from OCIO).

use super::{bake_capability, capability, FileFormat, FormatInfo, StubFormat};

pub(crate) fn create() -> Box<dyn FileFormat> {
    Box::new(StubFormat(vec![
        FormatInfo { name: "iridas_look", extension: "look", capabilities: capability::READ, bake_capabilities: bake_capability::NONE },
    ]))
}
