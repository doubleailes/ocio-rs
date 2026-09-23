//! `mga` file format (TODO: port from OCIO).

use super::{bake_capability, capability, FileFormat, FormatInfo, StubFormat};

pub(crate) fn create() -> Box<dyn FileFormat> {
    Box::new(StubFormat(vec![
        FormatInfo { name: "pandora_mga", extension: "mga", capabilities: capability::READ, bake_capabilities: bake_capability::NONE },
        FormatInfo { name: "pandora_m3d", extension: "m3d", capabilities: capability::READ, bake_capabilities: bake_capability::NONE },
    ]))
}
