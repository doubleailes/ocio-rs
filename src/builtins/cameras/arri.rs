//! ARRI camera builtins (port of `ArriCameras.cpp`).

use crate::builtins::color_matrix_helpers::{
    build_conversion_matrix, AdaptationMethod, Primaries, ACES_AP0,
};
use crate::builtins::op_helpers::{add_log_camera, add_matrix, LogCameraParams};
use crate::builtins::registry::BuiltinTransformRegistry;
use crate::types::TransformDirection::{Forward as FWD, Inverse as INV};

/// ARRI ALEXA Wide Gamut primaries.
pub const ARRI_ALEXA_WIDE_GAMUT: Primaries = Primaries::new(
    (0.68400, 0.31300),
    (0.22100, 0.84800),
    (0.08610, -0.10200),
    (0.31270, 0.32900),
);

/// ARRI Wide Gamut 4 primaries.
pub const ARRI_WIDE_GAMUT_4: Primaries = Primaries::new(
    (0.73470, 0.26530),
    (0.14240, 0.85760),
    (0.09910, -0.03080),
    (0.31270, 0.32900),
);

const LOGC_EI800_LIN_SIDE_SLOPE: f64 = 1.0 / (0.18 * 0.005 * (800.0 / 400.0) / 0.01);
const LOGC_EI800_LIN_SIDE_OFFSET: f64 = 0.0522722750;

/// ARRI ALEXA LogC (EI800) curve (log-to-lin is the inverse direction).
pub(crate) const ARRI_ALEXA_LOGC_EI800: LogCameraParams = LogCameraParams {
    base: 10.0,
    log_side_slope: 0.2471896383,
    log_side_offset: 0.3855369987,
    lin_side_slope: LOGC_EI800_LIN_SIDE_SLOPE,
    lin_side_offset: LOGC_EI800_LIN_SIDE_OFFSET,
    lin_side_break: Some(((1.0 / 9.0) - LOGC_EI800_LIN_SIDE_OFFSET) / LOGC_EI800_LIN_SIDE_SLOPE),
    linear_slope: None,
};

/// ARRI LogC4 curve (log-to-lin is the inverse direction).
pub(crate) const ARRI_LOGC4: LogCameraParams = LogCameraParams {
    base: 2.0,
    log_side_slope: 0.0647954196341293,
    log_side_offset: -0.295908392682586,
    lin_side_slope: 2231.82630906769,
    lin_side_offset: 64.0,
    lin_side_break: Some(-0.0180569961199113),
    linear_slope: None,
};

/// Register the ARRI camera builtins.
pub(crate) fn register_all(registry: &mut BuiltinTransformRegistry) {
    registry.add_builtin(
        "ARRI_ALEXA-LOGC-EI800-AWG_to_ACES2065-1",
        "Convert ARRI ALEXA LogC (EI800) ALEXA Wide Gamut to ACES2065-1",
        |ops| {
            add_log_camera(ops, &ARRI_ALEXA_LOGC_EI800, INV);
            let m = build_conversion_matrix(
                &ARRI_ALEXA_WIDE_GAMUT,
                &ACES_AP0,
                AdaptationMethod::Cat02,
            )?;
            add_matrix(ops, &m, FWD);
            Ok(())
        },
    );

    registry.add_builtin(
        "ARRI_LOGC4_to_ACES2065-1",
        "Convert ARRI LogC4 to ACES2065-1",
        |ops| {
            add_log_camera(ops, &ARRI_LOGC4, INV);
            let m =
                build_conversion_matrix(&ARRI_WIDE_GAMUT_4, &ACES_AP0, AdaptationMethod::Cat02)?;
            add_matrix(ops, &m, FWD);
            Ok(())
        },
    );
}
