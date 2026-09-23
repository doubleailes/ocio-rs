//! Apple camera builtins (port of `AppleCameras.cpp`).

use crate::builtins::color_matrix_helpers::{
    build_conversion_matrix, AdaptationMethod, Primaries, ACES_AP0, REC2020,
};
use crate::builtins::op_helpers::{add_fixed_function, add_matrix, add_range, TransformVec};
use crate::builtins::registry::BuiltinTransformRegistry;
use crate::types::FixedFunctionStyle;
use crate::types::TransformDirection::{Forward as FWD, Inverse as INV};

/// Apple Wide Gamut primaries (from the Apple Log 2 white paper).
pub const APPLE_WIDE_GAMUT: Primaries = Primaries::new(
    (0.725, 0.301),
    (0.221, 0.814),
    (0.068, -0.076),
    (0.3127, 0.3290),
);

/// Apple Log to linear: a clamp at 0 followed by the inverse gamma-log curve.
pub(crate) fn apple_log_to_linear(ops: &mut TransformVec) {
    const R_0: f64 = -0.05641088;
    const R_T: f64 = 0.01;
    const C: f64 = 47.28711236;
    const BETA: f64 = 0.00964052;
    const GAMMA: f64 = 0.08550479;
    const DELTA: f64 = 0.69336945;

    let gamma_log_params = [
        R_0, // mirror point
        R_T, // break point
        // Gamma segment.
        2.0,  // gamma power
        C,    // post-power scale
        -R_0, // pre-power offset
        // Log segment.
        2.0,   // log base
        GAMMA, // log-side slope
        DELTA, // log-side offset
        1.0,   // lin-side slope
        BETA,  // lin-side offset
    ];

    // Don't clamp high end.
    add_range(ops, Some(0.0), None, Some(0.0), None, FWD);
    // GAMMA_LOG_TO_LIN is the inverse of LIN_TO_GAMMA_LOG.
    add_fixed_function(
        ops,
        FixedFunctionStyle::LinToGammaLog,
        &gamma_log_params,
        INV,
    );
}

/// Register the Apple camera builtins.
pub(crate) fn register_all(registry: &mut BuiltinTransformRegistry) {
    registry.add_builtin(
        "APPLE_LOG_to_ACES2065-1",
        "Convert Apple Log to ACES2065-1",
        |ops| {
            apple_log_to_linear(ops);
            let m = build_conversion_matrix(&REC2020, &ACES_AP0, AdaptationMethod::Bradford)?;
            add_matrix(ops, &m, FWD);
            Ok(())
        },
    );

    // Apple Log 2 uses the same transfer function as Apple Log, but with the
    // wider Apple Wide Gamut primaries rather than Rec.2020.
    registry.add_builtin(
        "APPLE_LOG-APPLEWG_to_ACES2065-1",
        "Convert Apple Log 2 Apple Wide Gamut to ACES2065-1",
        |ops| {
            apple_log_to_linear(ops);
            let m =
                build_conversion_matrix(&APPLE_WIDE_GAMUT, &ACES_AP0, AdaptationMethod::Bradford)?;
            add_matrix(ops, &m, FWD);
            Ok(())
        },
    );

    registry.add_builtin(
        "CURVE - APPLE_LOG_to_LINEAR",
        "Convert Apple Log to linear",
        |ops| {
            apple_log_to_linear(ops);
            Ok(())
        },
    );
}
