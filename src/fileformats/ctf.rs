//! CLF / CTF file formats (TODO: port from OCIO `fileformats/ctf`).

use super::{bake_capability, capability, FileFormat, FormatInfo, StubFormat, FILEFORMAT_CLF, FILEFORMAT_CTF};

pub(crate) fn create() -> Box<dyn FileFormat> {
    Box::new(StubFormat(vec![
        FormatInfo { name: FILEFORMAT_CLF, extension: "clf", capabilities: capability::READ | capability::BAKE | capability::WRITE, bake_capabilities: bake_capability::LUT3D | bake_capability::LUT1D | bake_capability::LUT1D_3D },
        FormatInfo { name: FILEFORMAT_CTF, extension: "ctf", capabilities: capability::READ | capability::BAKE | capability::WRITE, bake_capabilities: bake_capability::LUT3D | bake_capability::LUT1D | bake_capability::LUT1D_3D },
    ]))
}
