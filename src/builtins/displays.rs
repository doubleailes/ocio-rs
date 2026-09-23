//! Display builtin transforms (port of `transforms/builtins/Displays.cpp`).

use super::color_matrix_helpers::{
    build_conversion_matrix_from_xyz_d65, AdaptationMethod, Primaries, P3_D60, P3_D65, P3_DCI,
    REC2020, REC709,
};
use super::op_helpers::{
    add_fixed_function, add_gamma, add_matrix, add_scale, GammaStyle, TransformVec,
};
use super::registry::BuiltinTransformRegistry;
use crate::error::Result;
use crate::types::FixedFunctionStyle;
use crate::types::TransformDirection::{Forward as FWD, Inverse as INV};

/// SMPTE ST-2084 (PQ) curves.
pub(crate) mod st_2084 {
    use super::*;

    /// PQ to linear nits/100.
    pub(crate) fn pq_to_linear(ops: &mut TransformVec) {
        add_fixed_function(ops, FixedFunctionStyle::LinToPq, &[], INV);
    }

    /// Linear nits/100 to PQ.
    pub(crate) fn linear_to_pq(ops: &mut TransformVec) {
        add_fixed_function(ops, FixedFunctionStyle::LinToPq, &[], FWD);
    }
}

/// ITU-R BT.2100 HLG curves.
pub(crate) mod hlg {
    use super::*;

    /// Nominal peak luminance.
    pub(crate) const LW: f64 = 1000.0;
    pub(crate) const E_MAX: f64 = 3.0;

    const A: f64 = 0.17883277;
    const B: f64 = (1.0 - 4.0 * A) * E_MAX / 12.0;
    const E_SCALE: f64 = 3.0 / E_MAX;
    const E_BREAK: f64 = E_MAX / 12.0;

    /// The parameters of the gamma-log fixed function implementing HLG.
    fn params() -> [f64; 10] {
        let c0 = 0.5 - A * (4.0 * A).ln();
        let c = (12.0 / E_MAX).ln() * A + c0;
        [
            0.0,     // mirror point
            E_BREAK, // break point
            // Gamma segment.
            0.5,            // gamma power
            E_SCALE.sqrt(), // post-power scale
            0.0,            // pre-power offset
            // Log segment.
            1.0f64.exp(), // log base
            A,            // log-side slope
            c,            // log-side offset
            1.0,          // lin-side slope
            -B,           // lin-side offset
        ]
    }

    /// HLG to linear.
    pub(crate) fn hlg_to_linear(ops: &mut TransformVec) {
        add_fixed_function(ops, FixedFunctionStyle::LinToGammaLog, &params(), INV);
    }

    /// Linear to HLG.
    pub(crate) fn linear_to_hlg(ops: &mut TransformVec) {
        add_fixed_function(ops, FixedFunctionStyle::LinToGammaLog, &params(), FWD);
    }
}

/// CIE XYZ D65 to the primaries (no adaptation) followed by a gamma.
fn xyz_to_gamma_display(
    ops: &mut TransformVec,
    primaries: &Primaries,
    method: AdaptationMethod,
    style: GammaStyle,
    gamma: f64,
    offset: f64,
) -> Result<()> {
    let m = build_conversion_matrix_from_xyz_d65(primaries, method)?;
    add_matrix(ops, &m, FWD);
    add_gamma(ops, style, gamma, offset);
    Ok(())
}

/// Register all the display builtins.
pub(crate) fn register_all(registry: &mut BuiltinTransformRegistry) {
    use AdaptationMethod::{Bradford, None as NoAdapt};
    use GammaStyle::{BasicMirrorRev, BasicRev, MoncurveMirrorRev, MoncurveRev};

    registry.add_builtin(
        "DISPLAY - CIE-XYZ-D65_to_REC.1886-REC.709",
        "Convert CIE XYZ (D65 white) to Rec.1886/Rec.709, clamp neg. values",
        |ops| xyz_to_gamma_display(ops, &REC709, NoAdapt, BasicRev, 2.4, 0.0),
    );
    registry.add_builtin(
        "DISPLAY - CIE-XYZ-D65_to_REC.1886-REC.709 - MIRROR NEGS",
        "Convert CIE XYZ (D65 white) to Rec.1886/Rec.709, mirror neg. values",
        |ops| xyz_to_gamma_display(ops, &REC709, NoAdapt, BasicMirrorRev, 2.4, 0.0),
    );

    registry.add_builtin(
        "DISPLAY - CIE-XYZ-D65_to_REC.1886-REC.2020",
        "Convert CIE XYZ (D65 white) to Rec.1886/Rec.2020, clamp neg. values",
        |ops| xyz_to_gamma_display(ops, &REC2020, NoAdapt, BasicRev, 2.4, 0.0),
    );
    registry.add_builtin(
        "DISPLAY - CIE-XYZ-D65_to_REC.1886-REC.2020 - MIRROR NEGS",
        "Convert CIE XYZ (D65 white) to Rec.1886/Rec.2020, mirror neg. values",
        |ops| xyz_to_gamma_display(ops, &REC2020, NoAdapt, BasicMirrorRev, 2.4, 0.0),
    );

    registry.add_builtin(
        "DISPLAY - CIE-XYZ-D65_to_G2.2-REC.709",
        "Convert CIE XYZ (D65 white) to Gamma2.2, Rec.709, clamp neg. values",
        |ops| xyz_to_gamma_display(ops, &REC709, NoAdapt, BasicRev, 2.2, 0.0),
    );
    registry.add_builtin(
        "DISPLAY - CIE-XYZ-D65_to_G2.2-REC.709 - MIRROR NEGS",
        "Convert CIE XYZ (D65 white) to Gamma2.2, Rec.709, mirror neg. values",
        |ops| xyz_to_gamma_display(ops, &REC709, NoAdapt, BasicMirrorRev, 2.2, 0.0),
    );

    registry.add_builtin(
        "DISPLAY - CIE-XYZ-D65_to_sRGB",
        "Convert CIE XYZ (D65 white) to sRGB (piecewise EOTF)",
        |ops| xyz_to_gamma_display(ops, &REC709, NoAdapt, MoncurveRev, 2.4, 0.055),
    );
    registry.add_builtin(
        "DISPLAY - CIE-XYZ-D65_to_sRGB - MIRROR NEGS",
        "Convert CIE XYZ (D65 white) to sRGB (piecewise EOTF), mirror neg. values",
        |ops| xyz_to_gamma_display(ops, &REC709, NoAdapt, MoncurveMirrorRev, 2.4, 0.055),
    );

    registry.add_builtin(
        "DISPLAY - CIE-XYZ-D65_to_G2.6-P3-DCI-BFD",
        "Convert CIE XYZ (D65 white) to Gamma 2.6, P3-DCI (DCI white with Bradford adaptation)",
        |ops| xyz_to_gamma_display(ops, &P3_DCI, Bradford, BasicRev, 2.6, 0.0),
    );

    registry.add_builtin(
        "DISPLAY - CIE-XYZ-D65_to_G2.6-P3-D65",
        "Convert CIE XYZ (D65 white) to Gamma 2.6, P3-D65, clamp neg. values",
        |ops| xyz_to_gamma_display(ops, &P3_D65, NoAdapt, BasicRev, 2.6, 0.0),
    );
    registry.add_builtin(
        "DISPLAY - CIE-XYZ-D65_to_G2.6-P3-D65 - MIRROR NEGS",
        "Convert CIE XYZ (D65 white) to Gamma 2.6, P3-D65, mirror neg. values",
        |ops| xyz_to_gamma_display(ops, &P3_D65, NoAdapt, BasicMirrorRev, 2.6, 0.0),
    );

    registry.add_builtin(
        "DISPLAY - CIE-XYZ-D65_to_G2.6-P3-D60-BFD",
        "Convert CIE XYZ (D65 white) to Gamma 2.6, P3-D60 (Bradford adaptation)",
        |ops| xyz_to_gamma_display(ops, &P3_D60, Bradford, BasicRev, 2.6, 0.0),
    );

    registry.add_builtin(
        "DISPLAY - CIE-XYZ-D65_to_DCDM-D65",
        "Convert CIE XYZ (D65 white) to Gamma 2.6 (D65 white in XYZ-E encoding)",
        |ops| {
            let scale = 48.0 / 52.37;
            add_scale(ops, &[scale, scale, scale, 1.0], FWD);
            add_gamma(ops, BasicRev, 2.6, 0.0);
            Ok(())
        },
    );

    // This color space is intended to be useful for macOS color spaces
    // kCGColorSpaceDisplayP3 and kCGColorSpaceExtendedDisplayP3. It uses the
    // sRGB transfer function, extended by reflecting the curve around 0 (as
    // kCGColorSpaceExtendedSRGB does), hence MONCURVE_MIRROR_REV. As with the
    // other displays here, it should be used with a RangeTransform to limit
    // the results to [0,1], if necessary.
    let display_p3 = |ops: &mut TransformVec| {
        xyz_to_gamma_display(ops, &P3_D65, NoAdapt, MoncurveMirrorRev, 2.4, 0.055)
    };
    registry.add_builtin(
        "DISPLAY - CIE-XYZ-D65_to_DisplayP3",
        "Convert CIE XYZ (D65 white) to Apple Display P3, mirror neg. values",
        display_p3,
    );
    // NOTE: This builtin is defined to be able to partition SDR and HDR view
    // transforms under two separate displays rather than a single one.
    registry.add_builtin(
        "DISPLAY - CIE-XYZ-D65_to_DisplayP3-HDR",
        "Convert CIE XYZ (D65 white) to Apple Display P3 (HDR), mirror neg. values",
        display_p3,
    );

    registry.add_builtin(
        "CURVE - ST-2084_to_LINEAR",
        "Convert SMPTE ST-2084 (PQ) full-range to linear nits/100",
        |ops| {
            st_2084::pq_to_linear(ops);
            Ok(())
        },
    );

    registry.add_builtin(
        "CURVE - LINEAR_to_ST-2084",
        "Convert linear nits/100 to SMPTE ST-2084 (PQ) full-range",
        |ops| {
            st_2084::linear_to_pq(ops);
            Ok(())
        },
    );

    registry.add_builtin(
        "DISPLAY - CIE-XYZ-D65_to_REC.2100-PQ",
        "Convert CIE XYZ (D65 white) to Rec.2100-PQ",
        |ops| {
            let m = build_conversion_matrix_from_xyz_d65(&REC2020, NoAdapt)?;
            add_matrix(ops, &m, FWD);
            st_2084::linear_to_pq(ops);
            Ok(())
        },
    );

    registry.add_builtin(
        "DISPLAY - CIE-XYZ-D65_to_ST2084-P3-D65",
        "Convert CIE XYZ (D65 white) to ST-2084 (PQ), P3-D65 primaries",
        |ops| {
            let m = build_conversion_matrix_from_xyz_d65(&P3_D65, NoAdapt)?;
            add_matrix(ops, &m, FWD);
            st_2084::linear_to_pq(ops);
            Ok(())
        },
    );

    registry.add_builtin(
        "DISPLAY - CIE-XYZ-D65_to_ST2084-DCDM-D65",
        "Convert CIE XYZ (D65 white) to ST-2084 (PQ) (D65 white in XYZ-E encoding)",
        |ops| {
            st_2084::linear_to_pq(ops);
            Ok(())
        },
    );

    registry.add_builtin(
        "CURVE - HLG-OETF-INVERSE",
        "Apply ITU-R BT.2100 (HLG) OETF inverse, scaled with HLG 0.42 at 18% grey",
        |ops| {
            hlg::hlg_to_linear(ops);
            Ok(())
        },
    );

    registry.add_builtin(
        "CURVE - HLG-OETF",
        "Apply ITU-R BT.2100 (HLG) OETF, scaled with 18% grey at HLG 0.42",
        |ops| {
            hlg::linear_to_hlg(ops);
            Ok(())
        },
    );

    registry.add_builtin(
        "DISPLAY - CIE-XYZ-D65_to_REC.2100-HLG-1000nit",
        "Convert CIE XYZ (D65 white) to Rec.2100-HLG, 1000 nit",
        |ops| {
            let m = build_conversion_matrix_from_xyz_d65(&REC2020, NoAdapt)?;
            add_matrix(ops, &m, FWD);

            let gamma = 1.2 + 0.42 * (hlg::LW / 1000.0).log10();
            {
                const SCALE: f64 = 100.0;
                add_scale(ops, &[SCALE, SCALE, SCALE, 1.0], FWD);
            }
            {
                let scale = hlg::E_MAX.powf(gamma) / hlg::LW;
                add_scale(ops, &[scale, scale, scale, 1.0], FWD);
            }

            add_fixed_function(
                ops,
                FixedFunctionStyle::Rec2100Surround,
                &[1.0 / gamma],
                FWD,
            );

            hlg::linear_to_hlg(ops);
            Ok(())
        },
    );
}
