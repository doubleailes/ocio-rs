//! RED camera builtins (port of `RedCameras.cpp`).

use crate::builtins::color_matrix_helpers::{
    build_conversion_matrix, AdaptationMethod, Primaries, ACES_AP0,
};
use crate::builtins::op_helpers::{add_log_camera, add_matrix, LogCameraParams};
use crate::builtins::registry::BuiltinTransformRegistry;
use crate::types::TransformDirection::{Forward as FWD, Inverse as INV};

/// RED Wide Gamut RGB primaries.
pub const RED_WIDE_GAMUT_RGB: Primaries = Primaries::new(
    (0.780308, 0.304253),
    (0.121595, 1.493994),
    (0.095612, -0.084589),
    (0.3127, 0.3290),
);

/// RED LogFilm curve (a log affine curve, no linear segment; log-to-lin is
/// the inverse direction).
pub(crate) fn redlogfilm() -> LogCameraParams {
    let ref_white = 685.0 / 1023.0;
    let ref_black = 95.0 / 1023.0;
    let range = 0.002 * 1023.0;
    let gamma = 0.6;
    let highlight = 1.0;
    let shadow = 0.0;
    let multi_factor = range / gamma;
    let gain = (highlight - shadow) / (1.0 - 10.0f64.powf(multi_factor * (ref_black - ref_white)));
    let offset = gain - (highlight - shadow);

    LogCameraParams {
        base: 10.0,
        log_side_slope: 1.0 / multi_factor,
        log_side_offset: ref_white,
        lin_side_slope: 1.0 / gain,
        lin_side_offset: (offset - shadow) / gain,
        lin_side_break: None,
        linear_slope: None,
    }
}

const LOG3G10_LIN_SIDE_SLOPE: f64 = 155.975327;

/// RED Log3G10 curve (log-to-lin is the inverse direction).
pub(crate) const RED_LOG3G10: LogCameraParams = LogCameraParams {
    base: 10.0,
    log_side_slope: 0.224282,
    log_side_offset: 0.0,
    lin_side_slope: LOG3G10_LIN_SIDE_SLOPE,
    lin_side_offset: 0.01 * LOG3G10_LIN_SIDE_SLOPE + 1.0,
    lin_side_break: Some(-0.01),
    linear_slope: None,
};

/// Register the RED camera builtins.
pub(crate) fn register_all(registry: &mut BuiltinTransformRegistry) {
    registry.add_builtin(
        "RED_REDLOGFILM-RWG_to_ACES2065-1",
        "Convert RED LogFilm RED Wide Gamut to ACES2065-1",
        |ops| {
            add_log_camera(ops, &redlogfilm(), INV);
            let m = build_conversion_matrix(
                &RED_WIDE_GAMUT_RGB,
                &ACES_AP0,
                AdaptationMethod::Bradford,
            )?;
            add_matrix(ops, &m, FWD);
            Ok(())
        },
    );

    registry.add_builtin(
        "RED_LOG3G10-RWG_to_ACES2065-1",
        "Convert RED Log3G10 RED Wide Gamut to ACES2065-1",
        |ops| {
            add_log_camera(ops, &RED_LOG3G10, INV);
            let m = build_conversion_matrix(
                &RED_WIDE_GAMUT_RGB,
                &ACES_AP0,
                AdaptationMethod::Bradford,
            )?;
            add_matrix(ops, &m, FWD);
            Ok(())
        },
    );
}
