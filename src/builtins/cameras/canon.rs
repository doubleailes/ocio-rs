//! Canon camera builtins (port of `CanonCameras.cpp`).

use crate::builtins::color_matrix_helpers::{
    build_conversion_matrix, AdaptationMethod, Primaries, ACES_AP0,
};
use crate::builtins::op_helpers::{add_matrix, create_lut, TransformVec};
use crate::builtins::registry::BuiltinTransformRegistry;
use crate::types::TransformDirection::Forward as FWD;

/// Canon Cinema Gamut primaries.
pub const CANON_CGAMUT: Primaries = Primaries::new(
    (0.7400, 0.2700),
    (0.1700, 1.1400),
    (0.0800, -0.1000),
    (0.3127, 0.3290),
);

/// Canon Log 2 to linear (a 4096 entries LUT, as OCIO builds it with
/// `OCIO_LUT_SUPPORT`).
pub(crate) fn clog2_to_linear(ops: &mut TransformVec) {
    create_lut(ops, 4096, |input| {
        let out = if input < 0.092864125 {
            -(10.0f64.powf((0.092864125 - input) / 0.24136077) - 1.0) / 87.099375
        } else {
            (10.0f64.powf((input - 0.092864125) / 0.24136077) - 1.0) / 87.099375
        };
        (out * 0.9) as f32
    });
}

/// Canon Log 3 to linear (a 4096 entries LUT, as OCIO builds it with
/// `OCIO_LUT_SUPPORT`).
pub(crate) fn clog3_to_linear(ops: &mut TransformVec) {
    create_lut(ops, 4096, |input| {
        let out = if input < 0.097465473 {
            -(10.0f64.powf((0.12783901 - input) / 0.36726845) - 1.0) / 14.98325
        } else if input <= 0.15277891 {
            (input - 0.12512219) / 1.9754798
        } else {
            (10.0f64.powf((input - 0.12240537) / 0.36726845) - 1.0) / 14.98325
        };
        (out * 0.9) as f32
    });
}

/// Register the Canon camera builtins.
pub(crate) fn register_all(registry: &mut BuiltinTransformRegistry) {
    registry.add_builtin(
        "CANON_CLOG2-CGAMUT_to_ACES2065-1",
        "Convert Canon Log 2 Cinema Gamut to ACES2065-1",
        |ops| {
            clog2_to_linear(ops);
            let m = build_conversion_matrix(&CANON_CGAMUT, &ACES_AP0, AdaptationMethod::Cat02)?;
            add_matrix(ops, &m, FWD);
            Ok(())
        },
    );

    registry.add_builtin(
        "CURVE - CANON_CLOG2_to_LINEAR",
        "Convert Canon Log 2 to linear",
        |ops| {
            clog2_to_linear(ops);
            Ok(())
        },
    );

    registry.add_builtin(
        "CANON_CLOG3-CGAMUT_to_ACES2065-1",
        "Convert Canon Log 3 Cinema Gamut to ACES2065-1",
        |ops| {
            clog3_to_linear(ops);
            let m = build_conversion_matrix(&CANON_CGAMUT, &ACES_AP0, AdaptationMethod::Cat02)?;
            add_matrix(ops, &m, FWD);
            Ok(())
        },
    );

    registry.add_builtin(
        "CURVE - CANON_CLOG3_to_LINEAR",
        "Convert Canon Log 3 to linear",
        |ops| {
            clog3_to_linear(ops);
            Ok(())
        },
    );
}
