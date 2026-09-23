//! ACES builtin transforms (port of `transforms/builtins/ACES.cpp`).

// The constants are copied verbatim from OCIO.
#![allow(clippy::excessive_precision)]

use super::color_matrix_helpers::{
    build_conversion_matrix, build_conversion_matrix_to_xyz_d65, build_vonkries_adapt,
    m44_inner_vec, rgb2xyz_from_xy, whitepoint, AdaptationMethod, Primaries, ACES_AP0, ACES_AP1,
    CIE_XYZ_ILLUM_E, P3_D60, P3_D65, REC2020, REC2020_D60, REC709, REC709_D60,
};
use super::op_helpers::{
    add_fixed_function, add_log, add_log_camera, add_master_bspline_curve, add_matrix, add_range,
    add_scale, add_scale_offset, create_half_lut, create_lut, interpolate_1d, LogCameraParams,
    TransformVec,
};
use super::registry::BuiltinTransformRegistry;
use crate::error::Result;
use crate::types::FixedFunctionStyle;
use crate::types::TransformDirection::{Forward as FWD, Inverse as INV};

// ---------------------------------------------------------------------------
// Component functions reused in multiple builtins.

/// ACES AP1 to CIE XYZ D65 (Bradford adaptation).
pub(crate) fn ap1_to_cie_xyz_d65(ops: &mut TransformVec) -> Result<()> {
    let m = build_conversion_matrix_to_xyz_d65(&ACES_AP1, AdaptationMethod::Bradford)?;
    add_matrix(ops, &m, FWD);
    Ok(())
}

/// The ACEScct curve (the log-to-lin direction is the inverse).
pub(crate) const ACESCCT_LOG: LogCameraParams = LogCameraParams {
    base: 2.0,
    log_side_slope: 1.0 / 17.52,
    log_side_offset: 9.72 / 17.52,
    lin_side_slope: 1.0,
    lin_side_offset: 0.0,
    lin_side_break: Some(0.0078125),
    linear_slope: None,
};

mod adx {
    use super::*;

    const LUT_SIZE: usize = 11;
    const NONUNIFORM_LUT: [f64; LUT_SIZE * 2] = [
        -0.190000000000000,
        -6.000000000000000,
        0.010000000000000,
        -2.721718645000000,
        0.028000000000000,
        -2.521718645000000,
        0.054000000000000,
        -2.321718645000000,
        0.095000000000000,
        -2.121718645000000,
        0.145000000000000,
        -1.921718645000000,
        0.220000000000000,
        -1.721718645000000,
        0.300000000000000,
        -1.521718645000000,
        0.400000000000000,
        -1.321718645000000,
        0.500000000000000,
        -1.121718645000000,
        0.600000000000000,
        -0.926545676714876,
    ];

    fn lut_value(input: f64) -> f32 {
        let out = if input < NONUNIFORM_LUT[0] {
            // Lower bound: extrapolate to ease conversion to LUT1D.
            let slope =
                (NONUNIFORM_LUT[3] - NONUNIFORM_LUT[1]) / (NONUNIFORM_LUT[2] - NONUNIFORM_LUT[0]);
            let out = NONUNIFORM_LUT[1] - slope * (NONUNIFORM_LUT[0] - input);
            if out < -10.0 {
                -10.0
            } else {
                out
            }
        } else if input <= NONUNIFORM_LUT[(LUT_SIZE - 1) * 2] {
            // The input is inside the LUT domain, so the interpolation can't fail.
            interpolate_1d(LUT_SIZE, &NONUNIFORM_LUT, input).unwrap_or(0.0)
        } else {
            // Upper bound.
            let ref_pt = (7120.0 - 1520.0) / 8000.0 * (100.0 / 55.0) - 0.18f64.log10();
            let out = (100.0 / 55.0) * input - ref_pt;
            if out > 4.8162678 {
                4.8162678 // log10(HALF_MAX)
            } else {
                out
            }
        };
        out as f32
    }

    /// ADX (channel dependent density) to ACES2065-1.
    pub(super) fn generate_ops(ops: &mut TransformVec) {
        // Note that in CTL, the matrices are stored transposed.
        const CDD_TO_CID: [f64; 16] = [
            0.75573, 0.22197, 0.02230, 0.0, //
            0.05901, 0.96928, -0.02829, 0.0, //
            0.16134, 0.07406, 0.76460, 0.0, //
            0.0, 0.0, 0.0, 1.0,
        ];

        // Convert Channel Dependent Density values into Channel Independent Density values.
        add_matrix(ops, &CDD_TO_CID, FWD);

        // Convert Channel Independent Density values to Relative Log Exposure values.
        create_half_lut(ops, lut_value);

        // Convert Relative Log Exposure values to Relative Exposure values.
        add_log(ops, 10.0, INV);

        const EXP_TO_ACES: [f64; 16] = [
            0.72286, 0.12630, 0.15084, 0.0, //
            0.11923, 0.76418, 0.11659, 0.0, //
            0.01427, 0.08213, 0.90359, 0.0, //
            0.0, 0.0, 0.0, 1.0,
        ];

        // Convert Relative Exposure values to ACES values.
        add_matrix(ops, &EXP_TO_ACES, FWD);
    }
}

mod aces_output {
    use super::*;

    pub(super) fn rrt_preamble_ops(ops: &mut TransformVec) -> Result<()> {
        add_fixed_function(ops, FixedFunctionStyle::AcesGlow10, &[], FWD);
        add_fixed_function(ops, FixedFunctionStyle::AcesRedMod10, &[], FWD);

        // Don't clamp high end.
        add_range(ops, Some(0.0), None, Some(0.0), None, FWD);

        let m = build_conversion_matrix(&ACES_AP0, &ACES_AP1, AdaptationMethod::None)?;
        add_matrix(ops, &m, FWD);

        // Don't clamp high end.
        add_range(ops, Some(0.0), None, Some(0.0), None, FWD);

        const RRT_SAT_MAT: [f64; 16] = [
            0.970889148671,
            0.026963270632,
            0.002147580696,
            0.0,
            0.010889148671,
            0.986963270632,
            0.002147580696,
            0.0,
            0.010889148671,
            0.026963270632,
            0.962147580696,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
        ];
        add_matrix(ops, &RRT_SAT_MAT, FWD);
        Ok(())
    }

    pub(super) fn tonecurve_ops(ops: &mut TransformVec) {
        // Convert to Log space.
        add_log(ops, 10.0, FWD);

        // Apply RRT shaper using the same quadratic B-spline as the CTL.
        add_master_bspline_curve(
            ops,
            &[
                (-5.26017743, -4.0),
                (-3.75502745, -3.57868829),
                (-2.24987747, -1.82131329),
                (-0.74472749, 0.68124124),
                (1.06145248, 2.87457742),
                (2.86763245, 3.83406206),
                (4.67381243, 4.0),
            ],
            &[
                0.0, 0.55982688, 1.77532247, 1.55, 0.8787017, 0.18374463, 0.0,
            ],
        );

        // Apply SDR ODT shaper using the same quadratic B-spline as the CTL.
        add_master_bspline_curve(
            ops,
            &[
                (-2.54062362, -1.69897000),
                (-2.08035721, -1.58843500),
                (-1.62009080, -1.35350000),
                (-1.15982439, -1.04695000),
                (-0.69955799, -0.65640000),
                (-0.23929158, -0.22141000),
                (0.22097483, 0.22814402),
                (0.68124124, 0.68124124),
                (1.01284632, 0.99142189),
                (1.34445140, 1.25800000),
                (1.67605648, 1.44995000),
                (2.00766156, 1.55910000),
                (2.33926665, 1.62260000),
                (2.67087173, 1.66065457),
                (3.00247681, 1.68124124),
            ],
            &[
                0.0, 0.4803088, 0.5405565, 0.79149813, 0.9055625, 0.98460368, 0.96884766, 1.0,
                0.87078346, 0.73702127, 0.42068113, 0.23763206, 0.14535362, 0.08416378, 0.04,
            ],
        );

        // Undo the logarithm.
        add_log(ops, 10.0, INV);

        // Apply Cinema White/Black correction.
        const CINEMA_WHITE: f64 = 48.0;
        const CINEMA_BLACK: f64 = 0.02;
        const SCALE: f64 = 1.0 / (CINEMA_WHITE - CINEMA_BLACK);
        const OFFSET: f64 = -CINEMA_BLACK * SCALE;
        add_scale_offset(
            ops,
            &[SCALE, SCALE, SCALE, 1.0],
            &[OFFSET, OFFSET, OFFSET, 0.0],
            FWD,
        );
    }

    pub(super) fn video_adjustment_ops(ops: &mut TransformVec) {
        // Surround correction for cinema to video.
        add_fixed_function(ops, FixedFunctionStyle::AcesDarkToDim10, &[], FWD);

        // Desat to compensate 48 nit to 100 nit brightness.
        const DESAT_100_NITS: [f64; 16] = [
            0.949056010175,
            0.047185723607,
            0.003758266219,
            0.0,
            0.019056010175,
            0.977185723607,
            0.003758266219,
            0.0,
            0.019056010175,
            0.047185723607,
            0.933758266219,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
        ];
        add_matrix(ops, &DESAT_100_NITS, FWD);
    }

    pub(super) fn hdr_tonecurve_ops(ops: &mut TransformVec, y_max: f64) {
        // Convert to Log space.
        add_log(ops, 10.0, FWD);

        // Apply RRT shaper using the same quadratic B-spline as the CTL.
        // Control points and slopes of the B-spline.
        type Curve = ([(f32, f32); 7], [f32; 7]);
        let curve: Option<Curve> = if y_max == 1000.0 {
            Some((
                [
                    (-5.60050155, -4.00000000),
                    (-4.09535157, -3.57868829),
                    (-2.59020159, -1.82131329),
                    (-1.08505161, 0.68124124),
                    (0.22347059, 2.22673503),
                    (1.53199279, 2.87906206),
                    (2.84051500, 3.00000000),
                ],
                [
                    0.0, 0.55982688, 1.77532247, 1.55, 0.81219728, 0.1848466, 0.0,
                ],
            ))
        } else if y_max == 2000.0 {
            Some((
                [
                    (-5.59738488, -4.00000000),
                    (-4.09223490, -3.57868829),
                    (-2.58708492, -1.82131329),
                    (-1.08193494, 0.68124124),
                    (0.37639718, 2.42130131),
                    (1.83472930, 3.16609199),
                    (3.29306142, 3.30103000),
                ],
                [
                    0.0, 0.55982688, 1.77532247, 1.55, 0.83637009, 0.18505799, 0.0,
                ],
            ))
        } else if y_max == 4000.0 {
            Some((
                [
                    (-5.59503319, -4.00000000),
                    (-4.08988322, -3.57868829),
                    (-2.58473324, -1.82131329),
                    (-1.07958326, 0.68124124),
                    (0.52855878, 2.61625839),
                    (2.13670081, 3.45351273),
                    (3.74484285, 3.60205999),
                ],
                [
                    0.0, 0.55982688, 1.77532247, 1.55, 0.85652519, 0.18474395, 0.0,
                ],
            ))
        } else if y_max == 108.0 {
            Some((
                [
                    (-5.37852506, -4.00000000),
                    (-3.87337508, -3.57868829),
                    (-2.36822510, -1.82131329),
                    (-0.86307513, 0.68124124),
                    (-0.03557710, 1.60464482),
                    (0.79192092, 1.96008059),
                    (1.61941895, 2.03342376),
                ],
                [
                    0.0, 0.55982688, 1.77532247, 1.55, 0.68179646, 0.17726487, 0.0,
                ],
            ))
        } else {
            None
        };
        if let Some((points, slopes)) = curve {
            add_master_bspline_curve(ops, &points, &slopes);
        }

        // Undo the logarithm.
        add_log(ops, 10.0, INV);

        // Apply Cinema White/Black correction.
        let y_min = 0.0001;
        let scale = 1.0 / (y_max - y_min);
        let offset = -y_min * scale;
        add_scale_offset(
            ops,
            &[scale, scale, scale, 1.0],
            &[offset, offset, offset, 0.0],
            FWD,
        );
    }

    pub(super) fn sdr_primary_clamp_ops(
        ops: &mut TransformVec,
        limit_primaries: &Primaries,
    ) -> Result<()> {
        let m1 = build_conversion_matrix(&ACES_AP1, limit_primaries, AdaptationMethod::Bradford)?;
        add_matrix(ops, &m1, FWD);

        add_range(ops, Some(0.0), Some(1.0), Some(0.0), Some(1.0), FWD);

        let m2 = rgb2xyz_from_xy(limit_primaries)?;
        add_matrix(ops, &m2, FWD);
        Ok(())
    }

    pub(super) fn hdr_primary_clamp_ops(
        ops: &mut TransformVec,
        limit_primaries: &Primaries,
    ) -> Result<()> {
        let m1 = build_conversion_matrix(&ACES_AP1, limit_primaries, AdaptationMethod::None)?;
        add_matrix(ops, &m1, FWD);

        add_range(ops, Some(0.0), Some(1.0), Some(0.0), Some(1.0), FWD);

        let m2 = rgb2xyz_from_xy(limit_primaries)?;
        add_matrix(ops, &m2, FWD);

        let m3 = build_vonkries_adapt(
            &whitepoint::D60_XYZ,
            &whitepoint::D65_XYZ,
            AdaptationMethod::Bradford,
        )?;
        add_matrix(ops, &m3, FWD);
        Ok(())
    }

    pub(super) fn nit_normalization_ops(ops: &mut TransformVec, nit_level: f64) {
        // The PQ curve expects nits / 100 as input. Unnormalize 1.0 to the nit
        // level for the transform and then renormalize to put 100 nits at 1.0.
        let scale = nit_level * 0.01;
        add_scale(ops, &[scale, scale, scale, 1.0], FWD);
    }

    fn roll_white(input: f64, new_wht: f64) -> f32 {
        let width = 0.5;
        let x0 = -1.0;
        let x1 = x0 + width;
        let y0 = -new_wht;
        let y1 = x1;
        let m1 = x1 - x0;
        let a = y0 - y1 + m1;
        let b = 2.0 * (y1 - y0) - m1;
        let c = y0;
        let t = (-input - x0) / (x1 - x0);
        let out = if t < 0.0 {
            -(t * b + c)
        } else if t > 1.0 {
            input
        } else {
            -((t * a + b) * t + c)
        };
        out as f32
    }

    pub(super) fn roll_white_d60_ops(ops: &mut TransformVec) {
        create_half_lut(ops, |x| roll_white(x, 0.918));
    }

    pub(super) fn roll_white_d65_ops(ops: &mut TransformVec) {
        create_half_lut(ops, |x| roll_white(x, 0.908));
    }
}

mod aces2_output {
    use super::*;

    /// The ACES 2 output transform (without the display encoding).
    pub(super) fn output_transform(
        ops: &mut TransformVec,
        peak_luminance: f32,
        limiting_pri: &Primaries,
        encoding_pri: &Primaries,
        linear_scale: f32,
        scale_white: bool,
    ) -> Result<()> {
        // Clamp to AP1.
        let matrix_to_ap1 = build_conversion_matrix(&ACES_AP0, &ACES_AP1, AdaptationMethod::None)?;
        add_matrix(ops, &matrix_to_ap1, FWD);

        // The numerator is computed in float and the denominator in double, as in OCIO.
        let upper_bound = (8.0f64
            * (128.0f64
                + 768.0f64
                    * ((peak_luminance / 100.0f32).ln() as f64 / (10000.0f64 / 100.0f64).ln())))
            as f32;
        add_range(
            ops,
            Some(0.0),
            Some(upper_bound as f64),
            Some(0.0),
            Some(upper_bound as f64),
            FWD,
        );

        add_matrix(ops, &matrix_to_ap1, INV);

        // Display rendering.
        add_fixed_function(
            ops,
            FixedFunctionStyle::AcesOutputTransform20,
            &[
                peak_luminance as f64,
                limiting_pri.red.xy[0],
                limiting_pri.red.xy[1],
                limiting_pri.grn.xy[0],
                limiting_pri.grn.xy[1],
                limiting_pri.blu.xy[0],
                limiting_pri.blu.xy[1],
                limiting_pri.wht.xy[0],
                limiting_pri.wht.xy[1],
            ],
            FWD,
        );

        // Post transform clamp.
        let norm_peak_luminance = (peak_luminance / 100.0f32) as f64;
        add_range(
            ops,
            Some(0.0),
            Some(norm_peak_luminance),
            Some(0.0),
            Some(norm_peak_luminance),
            FWD,
        );

        // White point simulation.
        if scale_white {
            let matrix_lim_to_out =
                build_conversion_matrix(limiting_pri, encoding_pri, AdaptationMethod::None)?;
            let white = m44_inner_vec(&matrix_lim_to_out, &[1.0, 1.0, 1.0, 0.0]);
            let scale = 1.0 / white[0].max(white[1]).max(white[2]);
            add_scale(ops, &[scale, scale, scale, 1.0], FWD);
        }

        // Linear scale factor.
        if linear_scale != 1.0 {
            let scale = linear_scale as f64;
            add_scale(ops, &[scale, scale, scale, 1.0], FWD);
        }

        let matrix_to_xyz =
            build_conversion_matrix_to_xyz_d65(limiting_pri, AdaptationMethod::None)?;
        add_matrix(ops, &matrix_to_xyz, FWD);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Registration.

fn acescc_lut_value(input: f64) -> f32 {
    // The functor input will be [0,1]. Remap this to a wider domain to better
    // capture the full extent of ACEScc.
    const IN_MIN: f64 = -0.36;
    const IN_MAX: f64 = 1.50;
    let x = input * (IN_MAX - IN_MIN) + IN_MIN;

    let out = if x < ((9.72 - 15.0) / 17.52) {
        (2.0f64.powf(x * 17.52 - 9.72) - 2.0f64.powf(-16.0)) * 2.0
    } else {
        2.0f64.powf(x * 17.52 - 9.72)
    };
    // The CTL clamps at HALF_MAX, but it's better to avoid a slope
    // discontinuity in a LUT.
    out as f32
}

/// Register all the ACES related builtin transforms.
pub(crate) fn register_all(registry: &mut BuiltinTransformRegistry) {
    registry.add_builtin(
        "UTILITY - ACES-AP0_to_CIE-XYZ-D65_BFD",
        "Convert ACES AP0 primaries to CIE XYZ with a D65 white point with Bradford adaptation",
        |ops| {
            // The CIE XYZ space has its conventional normalization (i.e., to
            // illuminant E). A neutral value of [1.,1.,1] in AP0 maps to the XYZ
            // value of D65 ([0.9504..., 1., 1.089...]).
            let m = build_conversion_matrix_to_xyz_d65(&ACES_AP0, AdaptationMethod::Bradford)?;
            add_matrix(ops, &m, FWD);
            Ok(())
        },
    );

    registry.add_builtin(
        "UTILITY - ACES-AP1_to_CIE-XYZ-D65_BFD",
        "Convert ACES AP1 primaries to CIE XYZ with a D65 white point with Bradford adaptation",
        ap1_to_cie_xyz_d65,
    );

    registry.add_builtin(
        "UTILITY - ACES-AP1_to_LINEAR-REC709_BFD",
        "Convert ACES AP1 primaries to linear Rec.709 primaries with Bradford adaptation",
        |ops| {
            let m = build_conversion_matrix(&ACES_AP1, &REC709, AdaptationMethod::Bradford)?;
            add_matrix(ops, &m, FWD);
            Ok(())
        },
    );

    registry.add_builtin(
        "CURVE - ACEScct-LOG_to_LINEAR",
        "Apply the log-to-lin curve used in ACEScct",
        |ops| {
            add_log_camera(ops, &ACESCCT_LOG, INV);
            Ok(())
        },
    );

    registry.add_builtin(
        "ACEScct_to_ACES2065-1",
        "Convert ACEScct to ACES2065-1",
        |ops| {
            add_log_camera(ops, &ACESCCT_LOG, INV);

            let m = build_conversion_matrix(&ACES_AP1, &ACES_AP0, AdaptationMethod::None)?;
            add_matrix(ops, &m, FWD);
            Ok(())
        },
    );

    registry.add_builtin(
        "ACEScc_to_ACES2065-1",
        "Convert ACEScc to ACES2065-1",
        |ops| {
            // Allow the LUT to work over a wider input range to better capture the ACEScc extent.
            add_range(ops, Some(-0.36), Some(1.5), Some(0.0), Some(1.0), FWD);

            create_lut(ops, 4096, acescc_lut_value);

            let m = build_conversion_matrix(&ACES_AP1, &ACES_AP0, AdaptationMethod::None)?;
            add_matrix(ops, &m, FWD);

            // This helps when the transform is inverted to match the CTL, which
            // clamps incoming ACES2065-1 values (don't clamp high end).
            add_range(ops, Some(0.0), None, Some(0.0), None, FWD);
            Ok(())
        },
    );

    registry.add_builtin(
        "ACEScg_to_ACES2065-1",
        "Convert ACEScg to ACES2065-1",
        |ops| {
            let m = build_conversion_matrix(&ACES_AP1, &ACES_AP0, AdaptationMethod::None)?;
            add_matrix(ops, &m, FWD);
            Ok(())
        },
    );

    registry.add_builtin(
        "ACESproxy10i_to_ACES2065-1",
        "Convert ACESproxy 10i to ACES2065-1",
        |ops| {
            add_range(
                ops,
                Some(64.0 / 1023.0),
                Some(940.0 / 1023.0),
                Some(((64.0 - 425.0) / 50.0) - 2.5),
                Some(((940.0 - 425.0) / 50.0) - 2.5),
                FWD,
            );

            add_log(ops, 2.0, INV);

            let m = build_conversion_matrix(&ACES_AP1, &ACES_AP0, AdaptationMethod::None)?;
            add_matrix(ops, &m, FWD);
            Ok(())
        },
    );

    registry.add_builtin(
        "ADX10_to_ACES2065-1",
        "Convert ADX10 to ACES2065-1",
        |ops| {
            const SCALE: f64 = 1023.0 / 500.0;
            const OFFSET: f64 = -95.0 / 500.0;

            // Convert ADX10 values to Channel Dependent Density values.
            add_scale_offset(
                ops,
                &[SCALE, SCALE, SCALE, 1.0],
                &[OFFSET, OFFSET, OFFSET, 0.0],
                FWD,
            );

            // Convert to ACES2065-1.
            adx::generate_ops(ops);
            Ok(())
        },
    );

    registry.add_builtin(
        "ADX16_to_ACES2065-1",
        "Convert ADX16 to ACES2065-1",
        |ops| {
            const SCALE: f64 = 65535.0 / 8000.0;
            const OFFSET: f64 = -1520.0 / 8000.0;

            // Convert ADX16 values to Channel Dependent Density values.
            add_scale_offset(
                ops,
                &[SCALE, SCALE, SCALE, 1.0],
                &[OFFSET, OFFSET, OFFSET, 0.0],
                FWD,
            );

            // Convert to ACES2065-1.
            adx::generate_ops(ops);
            Ok(())
        },
    );

    registry.add_builtin(
        "ACES-LMT - BLUE_LIGHT_ARTIFACT_FIX",
        "LMT for desaturating blue hues to reduce clipping artifacts",
        |ops| {
            // Note that in CTL, the matrices are stored transposed.
            const BLUE_LIGHT_FIX: [f64; 16] = [
                0.9404372683,
                -0.0183068787,
                0.0778696104,
                0.0,
                0.0083786969,
                0.8286599939,
                0.1629613092,
                0.0,
                0.0005471261,
                -0.0008833746,
                1.0003362486,
                0.0,
                0.0,
                0.0,
                0.0,
                1.0,
            ];
            add_matrix(ops, &BLUE_LIGHT_FIX, FWD);
            Ok(())
        },
    );

    registry.add_builtin(
        "ACES-LMT - ACES 1.3 Reference Gamut Compression",
        "LMT (applied in ACES2065-1) to compress scene-referred values from common cameras into the AP1 gamut",
        |ops| {
            let m = build_conversion_matrix(&ACES_AP0, &ACES_AP1, AdaptationMethod::None)?;
            add_matrix(ops, &m, FWD);

            add_fixed_function(
                ops,
                FixedFunctionStyle::AcesGamutComp13,
                &[1.147, 1.264, 1.312, 0.815, 0.803, 0.880, 1.2],
                FWD,
            );

            add_matrix(ops, &m, INV);
            Ok(())
        },
    );

    //
    // ACES OUTPUT TRANSFORMS
    //

    registry.add_builtin(
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-CINEMA_1.0",
        "Component of ACES Output Transforms for SDR cinema",
        |ops| {
            aces_output::rrt_preamble_ops(ops)?;
            aces_output::tonecurve_ops(ops);
            ap1_to_cie_xyz_d65(ops)
        },
    );

    registry.add_builtin(
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-VIDEO_1.0",
        "Component of ACES Output Transforms for SDR D65 video",
        |ops| {
            aces_output::rrt_preamble_ops(ops)?;
            aces_output::tonecurve_ops(ops);
            aces_output::video_adjustment_ops(ops);
            ap1_to_cie_xyz_d65(ops)
        },
    );

    registry.add_builtin(
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-CINEMA-REC709lim_1.1",
        "Component of ACES Output Transforms for SDR cinema",
        |ops| {
            aces_output::rrt_preamble_ops(ops)?;
            aces_output::tonecurve_ops(ops);
            aces_output::sdr_primary_clamp_ops(ops, &REC709)
        },
    );

    registry.add_builtin(
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-VIDEO-REC709lim_1.1",
        "Component of ACES Output Transforms for SDR D65 video",
        |ops| {
            aces_output::rrt_preamble_ops(ops)?;
            aces_output::tonecurve_ops(ops);
            aces_output::video_adjustment_ops(ops);
            aces_output::sdr_primary_clamp_ops(ops, &REC709)
        },
    );

    registry.add_builtin(
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-VIDEO-P3lim_1.1",
        "Component of ACES Output Transforms for SDR D65 video",
        |ops| {
            aces_output::rrt_preamble_ops(ops)?;
            aces_output::tonecurve_ops(ops);
            aces_output::video_adjustment_ops(ops);
            aces_output::sdr_primary_clamp_ops(ops, &P3_D65)
        },
    );

    registry.add_builtin(
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-CINEMA-D60sim-D65_1.1",
        "Component of ACES Output Transforms for SDR D65 cinema simulating D60 white",
        |ops| {
            aces_output::rrt_preamble_ops(ops)?;
            aces_output::tonecurve_ops(ops);

            // Don't clamp low end.
            add_range(ops, None, Some(1.0), None, Some(1.0), FWD);

            const SCALE: f64 = 0.964;
            add_scale(ops, &[SCALE, SCALE, SCALE, 1.0], FWD);

            let m = rgb2xyz_from_xy(&ACES_AP1)?;
            add_matrix(ops, &m, FWD);
            Ok(())
        },
    );

    registry.add_builtin(
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-VIDEO-D60sim-D65_1.0",
        "Component of ACES Output Transforms for SDR D65 video simulating D60 white",
        |ops| {
            aces_output::rrt_preamble_ops(ops)?;
            aces_output::tonecurve_ops(ops);

            // Don't clamp low end.
            add_range(ops, None, Some(1.0), None, Some(1.0), FWD);

            const SCALE: f64 = 0.955;
            add_scale(ops, &[SCALE, SCALE, SCALE, 1.0], FWD);

            aces_output::video_adjustment_ops(ops);

            let m = rgb2xyz_from_xy(&ACES_AP1)?;
            add_matrix(ops, &m, FWD);
            Ok(())
        },
    );

    registry.add_builtin(
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-CINEMA-D60sim-DCI_1.0",
        "Component of ACES Output Transforms for SDR DCI cinema simulating D60 white",
        |ops| {
            aces_output::rrt_preamble_ops(ops)?;
            aces_output::tonecurve_ops(ops);
            aces_output::roll_white_d60_ops(ops);

            // Don't clamp low end.
            add_range(ops, None, Some(0.918), None, Some(0.918), FWD);

            const SCALE: f64 = 0.96;
            add_scale(ops, &[SCALE, SCALE, SCALE, 1.0], FWD);

            let m = rgb2xyz_from_xy(&ACES_AP1)?;
            add_matrix(ops, &m, FWD);

            let m2 = build_vonkries_adapt(
                &whitepoint::DCI_XYZ,
                &whitepoint::D65_XYZ,
                AdaptationMethod::Bradford,
            )?;
            add_matrix(ops, &m2, FWD);
            Ok(())
        },
    );

    registry.add_builtin(
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-CINEMA-D65sim-DCI_1.1",
        "Component of ACES Output Transforms for SDR DCI cinema simulating D65 white",
        |ops| {
            aces_output::rrt_preamble_ops(ops)?;
            aces_output::tonecurve_ops(ops);
            aces_output::roll_white_d65_ops(ops);

            // Don't clamp low end.
            add_range(ops, None, Some(0.908), None, Some(0.908), FWD);

            const SCALE: f64 = 0.9575;
            add_scale(ops, &[SCALE, SCALE, SCALE, 1.0], FWD);

            ap1_to_cie_xyz_d65(ops)?;

            let m2 = build_vonkries_adapt(
                &whitepoint::DCI_XYZ,
                &whitepoint::D65_XYZ,
                AdaptationMethod::Bradford,
            )?;
            add_matrix(ops, &m2, FWD);
            Ok(())
        },
    );

    let hdr: [(&str, &str, f64, Primaries); 7] = [
        (
            "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-VIDEO-1000nit-15nit-REC2020lim_1.1",
            "Component of ACES Output Transforms for 1000 nit HDR D65 video",
            1000.0,
            REC2020,
        ),
        (
            "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-VIDEO-1000nit-15nit-P3lim_1.1",
            "Component of ACES Output Transforms for 1000 nit HDR D65 video",
            1000.0,
            P3_D65,
        ),
        (
            "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-VIDEO-2000nit-15nit-REC2020lim_1.1",
            "Component of ACES Output Transforms for 2000 nit HDR D65 video",
            2000.0,
            REC2020,
        ),
        (
            "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-VIDEO-2000nit-15nit-P3lim_1.1",
            "Component of ACES Output Transforms for 2000 nit HDR D65 video",
            2000.0,
            P3_D65,
        ),
        (
            "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-VIDEO-4000nit-15nit-REC2020lim_1.1",
            "Component of ACES Output Transforms for 4000 nit HDR D65 video",
            4000.0,
            REC2020,
        ),
        (
            "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-VIDEO-4000nit-15nit-P3lim_1.1",
            "Component of ACES Output Transforms for 4000 nit HDR D65 video",
            4000.0,
            P3_D65,
        ),
        (
            "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-CINEMA-108nit-7.2nit-P3lim_1.1",
            "Component of ACES Output Transforms for 108 nit HDR D65 cinema",
            108.0,
            P3_D65,
        ),
    ];
    for (style, desc, nits, limit) in hdr {
        registry.add_builtin(style, desc, move |ops| {
            aces_output::rrt_preamble_ops(ops)?;
            aces_output::hdr_tonecurve_ops(ops, nits);
            aces_output::hdr_primary_clamp_ops(ops, &limit)?;
            aces_output::nit_normalization_ops(ops, nits);
            Ok(())
        });
    }

    //
    // ACES 2 OUTPUT TRANSFORMS
    //

    struct Aces2OutputTransform {
        name: &'static str,
        desc: &'static str,
        peak_luminance: f32,
        limiting_primaries: Primaries,
        encoding_primaries: Primaries,
        linear_scale: f32,
        scale_white: bool,
    }

    const fn ot(
        name: &'static str,
        desc: &'static str,
        peak_luminance: f32,
        limiting_primaries: Primaries,
        encoding_primaries: Primaries,
        linear_scale: f32,
        scale_white: bool,
    ) -> Aces2OutputTransform {
        Aces2OutputTransform {
            name,
            desc,
            peak_luminance,
            limiting_primaries,
            encoding_primaries,
            linear_scale,
            scale_white,
        }
    }

    let aces2_output_transforms = [
        //
        // D65
        //
        ot(
            "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-REC709_2.0",
            "Component of ACES 2 Output Transforms for 100 nit SDR Rec709",
            100.0,
            REC709,
            REC709,
            1.0,
            false,
        ),
        ot(
            "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-P3-D65_2.0",
            "Component of ACES 2 Output Transforms for 100 nit SDR P3-D65",
            100.0,
            P3_D65,
            P3_D65,
            1.0,
            false,
        ),
        ot(
            "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-108nit-P3-D65_2.0",
            "Component of ACES 2 Output Transforms for 108 nit HDR P3-D65",
            225.0, // = 108 * (100/48)
            P3_D65,
            P3_D65,
            0.48,
            false,
        ),
        ot(
            "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-300nit-P3-D65_2.0",
            "Component of ACES 2 Output Transforms for 300 nit HDR P3-D65",
            625.0, // = 300 * (100/48)
            P3_D65,
            P3_D65,
            0.48,
            false,
        ),
        ot(
            "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-500nit-P3-D65_2.0",
            "Component of ACES 2 Output Transforms for 500 nit HDR P3-D65",
            500.0,
            P3_D65,
            P3_D65,
            1.0,
            false,
        ),
        ot(
            "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-1000nit-P3-D65_2.0",
            "Component of ACES 2 Output Transforms for 1000 nit HDR P3-D65",
            1000.0,
            P3_D65,
            P3_D65,
            1.0,
            false,
        ),
        ot(
            "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-2000nit-P3-D65_2.0",
            "Component of ACES 2 Output Transforms for 2000 nit HDR P3-D65",
            2000.0,
            P3_D65,
            P3_D65,
            1.0,
            false,
        ),
        ot(
            "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-4000nit-P3-D65_2.0",
            "Component of ACES 2 Output Transforms for 4000 nit HDR P3-D65",
            4000.0,
            P3_D65,
            P3_D65,
            1.0,
            false,
        ),
        ot(
            "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-500nit-REC2020_2.0",
            "Component of ACES 2 Output Transforms for 500 nit HDR Rec2020",
            500.0,
            REC2020,
            REC2020,
            1.0,
            false,
        ),
        ot(
            "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-1000nit-REC2020_2.0",
            "Component of ACES 2 Output Transforms for 1000 nit HDR Rec2020",
            1000.0,
            REC2020,
            REC2020,
            1.0,
            false,
        ),
        ot(
            "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-2000nit-REC2020_2.0",
            "Component of ACES 2 Output Transforms for 2000 nit HDR Rec2020",
            2000.0,
            REC2020,
            REC2020,
            1.0,
            false,
        ),
        ot(
            "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-4000nit-REC2020_2.0",
            "Component of ACES 2 Output Transforms for 4000 nit HDR Rec2020",
            4000.0,
            REC2020,
            REC2020,
            1.0,
            false,
        ),
        //
        // D60
        //
        ot(
            "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-REC709-D60-in-REC709-D65_2.0",
            "Component of ACES 2 Output Transforms for 100 nit SDR Rec709 simulating D60 white in Rec709",
            100.0,
            REC709_D60,
            REC709,
            1.0,
            true,
        ),
        ot(
            "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-REC709-D60-in-P3-D65_2.0",
            "Component of ACES 2 Output Transforms for 100 nit SDR Rec709 simulating D60 white in P3-D65",
            100.0,
            REC709_D60,
            P3_D65,
            1.0,
            true,
        ),
        ot(
            "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-REC709-D60-in-REC2020-D65_2.0",
            "Component of ACES 2 Output Transforms for 100 nit SDR Rec709 simulating D60 white in Rec2020",
            100.0,
            REC709_D60,
            REC2020,
            1.0,
            true,
        ),
        ot(
            "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-P3-D60-in-P3-D65_2.0",
            "Component of ACES 2 Output Transforms for 100 nit SDR P3-D60 simulating D60 white in P3-D65",
            100.0,
            P3_D60,
            P3_D65,
            1.0,
            true,
        ),
        ot(
            "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-P3-D60-in-XYZ-E_2.0",
            "Component of ACES 2 Output Transforms for 100 nit SDR P3-D60 simulating D60 white in XYZ-E",
            100.0,
            P3_D60,
            CIE_XYZ_ILLUM_E,
            1.0,
            false,
        ),
        ot(
            "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-108nit-P3-D60-in-P3-D65_2.0",
            "Component of ACES 2 Output Transforms for 108 nit HDR P3-D60 simulating D60 white in P3-D65",
            225.0, // = 108 * (100/48)
            P3_D60,
            P3_D65,
            0.48,
            true,
        ),
        ot(
            "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-300nit-P3-D60-in-XYZ-E_2.0",
            "Component of ACES 2 Output Transforms for 300 nit HDR P3-D60 simulating D60 white in XYZ-E",
            625.0, // = 300 * (100/48)
            P3_D60,
            CIE_XYZ_ILLUM_E,
            0.48,
            true,
        ),
        ot(
            "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-500nit-P3-D60-in-P3-D65_2.0",
            "Component of ACES 2 Output Transforms for 500 nit HDR P3-D60 simulating D60 white in P3-D65",
            500.0,
            P3_D60,
            P3_D65,
            1.0,
            true,
        ),
        ot(
            "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-1000nit-P3-D60-in-P3-D65_2.0",
            "Component of ACES 2 Output Transforms for 1000 nit HDR P3-D60 simulating D60 white in P3-D65",
            1000.0,
            P3_D60,
            P3_D65,
            1.0,
            true,
        ),
        ot(
            "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-2000nit-P3-D60-in-P3-D65_2.0",
            "Component of ACES 2 Output Transforms for 2000 nit HDR P3-D60 simulating D60 white in P3-D65",
            2000.0,
            P3_D60,
            P3_D65,
            1.0,
            true,
        ),
        ot(
            "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-4000nit-P3-D60-in-P3-D65_2.0",
            "Component of ACES 2 Output Transforms for 4000 nit HDR P3-D60 simulating D60 white in P3-D65",
            4000.0,
            P3_D60,
            P3_D65,
            1.0,
            true,
        ),
        ot(
            "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-500nit-P3-D60-in-REC2020-D65_2.0",
            "Component of ACES 2 Output Transforms for 500 nit HDR P3-D60 simulating D60 white in Rec2020",
            500.0,
            P3_D60,
            REC2020,
            1.0,
            true,
        ),
        ot(
            "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-1000nit-P3-D60-in-REC2020-D65_2.0",
            "Component of ACES 2 Output Transforms for 1000 nit HDR P3-D60 simulating D60 white in Rec2020",
            1000.0,
            P3_D60,
            REC2020,
            1.0,
            true,
        ),
        ot(
            "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-2000nit-P3-D60-in-REC2020-D65_2.0",
            "Component of ACES 2 Output Transforms for 2000 nit HDR P3-D60 simulating D60 white in Rec2020",
            2000.0,
            P3_D60,
            REC2020,
            1.0,
            true,
        ),
        ot(
            "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-4000nit-P3-D60-in-REC2020-D65_2.0",
            "Component of ACES 2 Output Transforms for 4000 nit HDR P3-D60 simulating D60 white in Rec2020",
            4000.0,
            P3_D60,
            REC2020,
            1.0,
            true,
        ),
        ot(
            "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-500nit-REC2020-D60-in-REC2020-D65_2.0",
            "Component of ACES 2 Output Transforms for 500 nit HDR Rec2020 simulating D60 white in Rec2020",
            500.0,
            REC2020_D60,
            REC2020,
            1.0,
            true,
        ),
        ot(
            "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-1000nit-REC2020-D60-in-REC2020-D65_2.0",
            "Component of ACES 2 Output Transforms for 1000 nit HDR Rec2020 simulating D60 white in Rec2020",
            1000.0,
            REC2020_D60,
            REC2020,
            1.0,
            true,
        ),
        ot(
            "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-2000nit-REC2020-D60-in-REC2020-D65_2.0",
            "Component of ACES 2 Output Transforms for 2000 nit HDR Rec2020 simulating D60 white in Rec2020",
            2000.0,
            REC2020_D60,
            REC2020,
            1.0,
            true,
        ),
        ot(
            "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-4000nit-REC2020-D60-in-REC2020-D65_2.0",
            "Component of ACES 2 Output Transforms for 4000 nit HDR Rec2020 simulating D60 white in Rec2020",
            4000.0,
            REC2020_D60,
            REC2020,
            1.0,
            true,
        ),
    ];

    for tr in aces2_output_transforms {
        let Aces2OutputTransform {
            name,
            desc,
            peak_luminance,
            limiting_primaries,
            encoding_primaries,
            linear_scale,
            scale_white,
        } = tr;
        registry.add_builtin(name, desc, move |ops| {
            aces2_output::output_transform(
                ops,
                peak_luminance,
                &limiting_primaries,
                &encoding_primaries,
                linear_scale,
                scale_white,
            )
        });
    }
}
