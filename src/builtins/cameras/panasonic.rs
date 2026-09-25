//! Panasonic camera builtins (port of `PanasonicCameras.cpp`).

use crate::builtins::color_matrix_helpers::{
    build_conversion_matrix, AdaptationMethod, Primaries, ACES_AP0,
};
use crate::builtins::op_helpers::{add_log_camera, add_matrix, LogCameraParams};
use crate::builtins::registry::BuiltinTransformRegistry;
use crate::types::TransformDirection::{Forward as FWD, Inverse as INV};

/// Panasonic V-Gamut primaries.
pub const PANASONIC_VGAMUT: Primaries = Primaries::new(
    (0.730, 0.280),
    (0.165, 0.840),
    (0.100, -0.030),
    (0.3127, 0.3290),
);

/// Panasonic V-Log curve (log-to-lin is the inverse direction).
pub(crate) const PANASONIC_VLOG: LogCameraParams = LogCameraParams {
    base: 10.0,
    log_side_slope: 0.241514,  // c
    log_side_offset: 0.598206, // d
    lin_side_slope: 1.0,
    lin_side_offset: 0.00873,   // b
    lin_side_break: Some(0.01), // cut1
    linear_slope: None,
};

/// Register the Panasonic camera builtins.
pub(crate) fn register_all(registry: &mut BuiltinTransformRegistry) {
    registry.add_builtin(
        "PANASONIC_VLOG-VGAMUT_to_ACES2065-1",
        "Convert Panasonic Varicam V-Log V-Gamut to ACES2065-1",
        |ops| {
            add_log_camera(ops, &PANASONIC_VLOG, INV);
            let m =
                build_conversion_matrix(&PANASONIC_VGAMUT, &ACES_AP0, AdaptationMethod::Bradford)?;
            add_matrix(ops, &m, FWD);
            Ok(())
        },
    );
}
