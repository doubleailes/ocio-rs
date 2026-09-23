//! Canon camera builtins (port of `CanonCameras.cpp`).

use crate::builtins::color_matrix_helpers::{
    build_conversion_matrix, AdaptationMethod, Primaries, ACES_AP0,
};
use crate::builtins::op_helpers::{add_fixed_function, add_matrix, TransformVec};
use crate::builtins::registry::BuiltinTransformRegistry;
use crate::types::FixedFunctionStyle;
use crate::types::TransformDirection::{Forward as FWD, Inverse as INV};

/// Canon Cinema Gamut primaries.
pub const CANON_CGAMUT: Primaries = Primaries::new(
    (0.7400, 0.2700),
    (0.1700, 1.1400),
    (0.0800, -0.1000),
    (0.3127, 0.3290),
);

/// Canon Log 2 to linear (inverse double-log curve).
pub(crate) fn clog2_to_linear(ops: &mut TransformVec) {
    let params = [
        10.0,             // log base
        0.0,              // break point 1
        0.0,              // break point 2 (no linear segment)
        -0.24136077,      // log segment 1 log-side slope
        0.092864125,      // log segment 1 log-side offset
        -87.099375 / 0.9, // log segment 1 lin-side slope
        1.0,              // log segment 1 lin-side offset
        0.24136077,       // log segment 2 log-side slope
        0.092864125,      // log segment 2 log-side offset
        87.099375 / 0.9,  // log segment 2 lin-side slope
        1.0,              // log segment 2 lin-side offset
        1.0,              // linear segment slope (not used)
        0.0,              // linear segment offset (not used)
    ];
    // DOUBLE_LOG_TO_LIN is the inverse of LIN_TO_DOUBLE_LOG.
    add_fixed_function(ops, FixedFunctionStyle::LinToDoubleLog, &params, INV);
}

/// Canon Log 3 to linear (inverse double-log curve with a linear segment).
pub(crate) fn clog3_to_linear(ops: &mut TransformVec) {
    let params = [
        10.0,            // log base
        -0.014 * 0.9,    // break point 1
        0.014 * 0.9,     // break point 2
        -0.36726845,     // log segment 1 log-side slope
        0.12783901,      // log segment 1 log-side offset
        -14.98325 / 0.9, // log segment 1 lin-side slope
        1.0,             // log segment 1 lin-side offset
        0.36726845,      // log segment 2 log-side slope
        0.12240537,      // log segment 2 log-side offset
        14.98325 / 0.9,  // log segment 2 lin-side slope
        1.0,             // log segment 2 lin-side offset
        1.9754798,       // linear segment slope
        0.12512219,      // linear segment offset
    ];
    // DOUBLE_LOG_TO_LIN is the inverse of LIN_TO_DOUBLE_LOG.
    add_fixed_function(ops, FixedFunctionStyle::LinToDoubleLog, &params, INV);
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
