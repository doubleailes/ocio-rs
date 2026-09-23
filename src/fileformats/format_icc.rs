//! `icc` file format (TODO: port from OCIO).

use super::{bake_capability, capability, FileFormat, FormatInfo, StubFormat};

pub(crate) fn create() -> Box<dyn FileFormat> {
    Box::new(StubFormat(vec![
        FormatInfo { name: "International Color Consortium profile", extension: "icc", capabilities: capability::READ, bake_capabilities: bake_capability::NONE },
        FormatInfo { name: "Image Color Matching profile", extension: "icm", capabilities: capability::READ, bake_capabilities: bake_capability::NONE },
        FormatInfo { name: "ICC profile", extension: "pf", capabilities: capability::READ, bake_capabilities: bake_capability::NONE },
    ]))
}
