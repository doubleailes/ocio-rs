//! Sony camera builtins (port of `SonyCameras.cpp`).

use crate::builtins::color_matrix_helpers::{
    build_conversion_matrix, AdaptationMethod, Primaries, ACES_AP0,
};
use crate::builtins::op_helpers::{add_log_camera, add_matrix, LogCameraParams};
use crate::builtins::registry::BuiltinTransformRegistry;
use crate::types::TransformDirection::{Forward as FWD, Inverse as INV};

/// Sony S-Gamut3 primaries.
pub const SONY_SGAMUT3: Primaries = Primaries::new(
    (0.730, 0.280),
    (0.140, 0.855),
    (0.100, -0.050),
    (0.3127, 0.3290),
);

/// Sony S-Gamut3.Cine primaries.
pub const SONY_SGAMUT3_CINE: Primaries = Primaries::new(
    (0.766, 0.275),
    (0.225, 0.800),
    (0.089, -0.087),
    (0.3127, 0.3290),
);

/// Sony S-Log3 curve (log-to-lin is the inverse direction).
pub(crate) const SONY_SLOG3: LogCameraParams = LogCameraParams {
    base: 10.0,
    log_side_slope: 261.5 / 1023.0,
    log_side_offset: 420.0 / 1023.0,
    lin_side_slope: 1.0 / (0.18 + 0.01),
    lin_side_offset: 0.01 / (0.18 + 0.01),
    lin_side_break: Some(0.01125000),
    linear_slope: Some(((171.2102946929 - 95.0) / 0.01125000) / 1023.0),
};

/// Register the Sony camera builtins.
pub(crate) fn register_all(registry: &mut BuiltinTransformRegistry) {
    registry.add_builtin(
        "SONY_SLOG3-SGAMUT3_to_ACES2065-1",
        "Convert Sony S-Log3 S-Gamut3 to ACES2065-1",
        |ops| {
            add_log_camera(ops, &SONY_SLOG3, INV);
            let m = build_conversion_matrix(&SONY_SGAMUT3, &ACES_AP0, AdaptationMethod::Cat02)?;
            add_matrix(ops, &m, FWD);
            Ok(())
        },
    );

    registry.add_builtin(
        "SONY_SLOG3-SGAMUT3.CINE_to_ACES2065-1",
        "Convert Sony S-Log3 S-Gamut3.Cine to ACES2065-1",
        |ops| {
            add_log_camera(ops, &SONY_SLOG3, INV);
            let m =
                build_conversion_matrix(&SONY_SGAMUT3_CINE, &ACES_AP0, AdaptationMethod::Cat02)?;
            add_matrix(ops, &m, FWD);
            Ok(())
        },
    );

    registry.add_builtin(
        "SONY_SLOG3-SGAMUT3-VENICE_to_ACES2065-1",
        "Convert Sony S-Log3 S-Gamut3 for the Venice camera to ACES2065-1",
        |ops| {
            add_log_camera(ops, &SONY_SLOG3, INV);

            // NB: Primaries were not provided for this case, only the matrix.
            // Note that in CTL, the matrices are stored transposed.
            const SGAMUT3_VENICE: [f64; 16] = [
                0.7933297411,
                0.0890786256,
                0.1175916333,
                0.0, //
                0.0155810585,
                1.0327123069,
                -0.0482933654,
                0.0, //
                -0.0188647478,
                0.0127694121,
                1.0060953358,
                0.0, //
                0.0,
                0.0,
                0.0,
                1.0,
            ];
            add_matrix(ops, &SGAMUT3_VENICE, FWD);
            Ok(())
        },
    );

    registry.add_builtin(
        "SONY_SLOG3-SGAMUT3.CINE-VENICE_to_ACES2065-1",
        "Convert Sony S-Log3 S-Gamut3.Cine for the Venice camera to ACES2065-1",
        |ops| {
            add_log_camera(ops, &SONY_SLOG3, INV);

            // NB: Primaries were not provided for this case, only the matrix.
            // Note that in CTL, the matrices are stored transposed.
            const SGAMUT3_CINE_VENICE: [f64; 16] = [
                0.6742570921,
                0.2205717359,
                0.1051711720,
                0.0, //
                -0.0093136061,
                1.1059588614,
                -0.0966452553,
                0.0, //
                -0.0382090673,
                -0.0179383766,
                1.0561474439,
                0.0, //
                0.0,
                0.0,
                0.0,
                1.0,
            ];
            add_matrix(ops, &SGAMUT3_CINE_VENICE, FWD);
            Ok(())
        },
    );
}
