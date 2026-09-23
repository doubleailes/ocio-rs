//! Helpers creating the transforms the builtins are made of (port of
//! `OpHelpers.cpp` and of the `Create*Op` calls used by the builtins).
//!
//! OCIO builds the builtins directly as ops; here every helper appends the
//! equivalent plain transform to a list, which is later converted into ops
//! with [`crate::transforms::build::build_ops`].

use crate::error::{Error, Result};
use crate::transforms::grading::GradingBSplineCurve;
use crate::transforms::{
    ExponentTransform, ExponentWithLinearTransform, FixedFunctionTransform,
    GradingRgbCurveTransform, LogCameraTransform, LogTransform, Lut1DTransform, MatrixTransform,
    RangeTransform, Transform,
};
use crate::types::{
    BSplineType, FixedFunctionStyle, GradingStyle, Interpolation, NegativeStyle, RgbCurveType,
    TransformDirection,
};

/// Largest finite half float value.
pub const HALF_MAX: f64 = 65504.0;

/// The transforms of a builtin, in order.
pub type TransformVec = Vec<Transform>;

/// Linearly interpolate a single input value through a non-uniformly spaced
/// LUT. `lut_values` is ordered as `[in0, out0, in1, out1, ...]` and holds
/// `lut_size` pairs. Values outside the domain are clamped.
pub fn interpolate_1d(lut_size: usize, lut_values: &[f64], input: f64) -> Result<f64> {
    if lut_size == 0 || lut_values.len() < 2 * lut_size {
        return Err(Error::msg("Invalid interpolation value."));
    }

    // Clamp if values are outside the domain of the LUT.
    if input < lut_values[0] {
        return Ok(lut_values[1]);
    } else if input >= lut_values[2 * (lut_size - 1)] {
        return Ok(lut_values[2 * (lut_size - 1) + 1]);
    }

    for idx in 1..lut_size {
        if input < lut_values[2 * idx] {
            let min_idx = 2 * (idx - 1);
            let max_idx = 2 * idx;

            let in_coeff =
                (input - lut_values[min_idx]) / (lut_values[max_idx] - lut_values[min_idx]);

            return Ok(
                lut_values[min_idx + 1] * (1.0 - in_coeff) + lut_values[max_idx + 1] * in_coeff
            );
        }
    }

    Err(Error::msg("Invalid interpolation value."))
}

/// A 1D LUT (linear interpolation) whose three channels hold
/// `generator(x)` for `lut_dimension` inputs `x` linearly spaced on `[0, 1]`.
pub fn create_lut(ops: &mut TransformVec, lut_dimension: usize, generator: impl Fn(f64) -> f32) {
    let mut lut = Lut1DTransform::new(lut_dimension, false);
    lut.interpolation = Interpolation::Linear;
    let denom = lut_dimension as f64 - 1.0;
    for idx in 0..lut_dimension {
        let v = generator(idx as f64 / denom);
        lut.set_value(idx, v, v, v);
    }
    ops.push(Transform::Lut1D(lut));
}

/// A 1D LUT (linear interpolation) whose channels may differ: the generator
/// receives the RGB input (linearly spaced on `[0, 1]`) and returns the RGB
/// output.
pub fn create_lut_rgb(
    ops: &mut TransformVec,
    lut_dimension: usize,
    generator: impl Fn(&[f64; 3]) -> [f64; 3],
) {
    let mut lut = Lut1DTransform::new(lut_dimension, false);
    lut.interpolation = Interpolation::Linear;
    let denom = lut_dimension as f64 - 1.0;
    for idx in 0..lut_dimension {
        let x = idx as f64 / denom;
        let out = generator(&[x, x, x]);
        lut.set_value(idx, out[0] as f32, out[1] as f32, out[2] as f32);
    }
    ops.push(Transform::Lut1D(lut));
}

/// A half-domain 1D LUT (65536 entries indexed by half float bit patterns).
/// NaNs are mapped to 0 and +/-Inf to +/-`HALF_MAX` before calling the
/// generator.
pub fn create_half_lut(ops: &mut TransformVec, generator: impl Fn(f64) -> f32) {
    let mut lut = Lut1DTransform::new(65536, true);
    lut.interpolation = Interpolation::Linear;
    for idx in 0..65536usize {
        let h = half::f16::from_bits(idx as u16);
        let value = if h.is_nan() {
            0.0
        } else if h.is_infinite() {
            if h.is_sign_negative() {
                -HALF_MAX
            } else {
                HALF_MAX
            }
        } else {
            h.to_f32() as f64
        };
        let v = generator(value);
        lut.set_value(idx, v, v, v);
    }
    ops.push(Transform::Lut1D(lut));
}

/// Append a 4x4 matrix (no offset).
pub fn add_matrix(ops: &mut TransformVec, m44: &[f64; 16], dir: TransformDirection) {
    let mut t = MatrixTransform::new(*m44, [0.0; 4]);
    t.direction = dir;
    ops.push(Transform::Matrix(t));
}

/// Append an identity matrix (port of `CreateIdentityMatrixOp`).
pub fn add_identity_matrix(ops: &mut TransformVec) {
    ops.push(Transform::Matrix(MatrixTransform::default()));
}

/// Append a diagonal scale matrix (port of `CreateScaleOp`).
pub fn add_scale(ops: &mut TransformVec, scale4: &[f64; 4], dir: TransformDirection) {
    add_scale_offset(ops, scale4, &[0.0; 4], dir);
}

/// Append a diagonal scale matrix with an offset (port of
/// `CreateScaleOffsetOp`).
pub fn add_scale_offset(
    ops: &mut TransformVec,
    scale4: &[f64; 4],
    offset4: &[f64; 4],
    dir: TransformDirection,
) {
    let (m, _) = MatrixTransform::scale(scale4);
    let mut t = MatrixTransform::new(m, *offset4);
    t.direction = dir;
    ops.push(Transform::Matrix(t));
}

/// Append a clamping range. `None` bounds are not clamped (OCIO's
/// `RangeOpData::EmptyValue()`).
pub fn add_range(
    ops: &mut TransformVec,
    min_in: Option<f64>,
    max_in: Option<f64>,
    min_out: Option<f64>,
    max_out: Option<f64>,
    dir: TransformDirection,
) {
    let mut t = RangeTransform::new(min_in, max_in, min_out, max_out);
    t.direction = dir;
    ops.push(Transform::Range(t));
}

/// Append a pure log (`dir` forward) or anti-log (`dir` inverse) of the
/// given base (port of `CreateLogOp(ops, base, dir)`).
pub fn add_log(ops: &mut TransformVec, base: f64, dir: TransformDirection) {
    let mut t = LogTransform::new(base);
    t.direction = dir;
    ops.push(Transform::Log(t));
}

/// Parameters of a camera log curve (OCIO's `LogOpData::Params`, applied to
/// all three channels).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LogCameraParams {
    pub base: f64,
    pub log_side_slope: f64,
    pub log_side_offset: f64,
    pub lin_side_slope: f64,
    pub lin_side_offset: f64,
    /// `None` for a pure log affine curve (no linear segment).
    pub lin_side_break: Option<f64>,
    /// Optional explicit linear segment slope.
    pub linear_slope: Option<f64>,
}

/// Append a camera log curve; `dir` inverse converts log to linear.
pub fn add_log_camera(ops: &mut TransformVec, p: &LogCameraParams, dir: TransformDirection) {
    match p.lin_side_break {
        Some(brk) => {
            let mut t = LogCameraTransform::new([brk; 3]);
            t.base = p.base;
            t.log_side_slope = [p.log_side_slope; 3];
            t.log_side_offset = [p.log_side_offset; 3];
            t.lin_side_slope = [p.lin_side_slope; 3];
            t.lin_side_offset = [p.lin_side_offset; 3];
            t.linear_slope = p.linear_slope.map(|s| [s; 3]);
            t.direction = dir;
            ops.push(Transform::LogCamera(t));
        }
        None => {
            let t = crate::transforms::LogAffineTransform {
                direction: dir,
                base: p.base,
                log_side_slope: [p.log_side_slope; 3],
                log_side_offset: [p.log_side_offset; 3],
                lin_side_slope: [p.lin_side_slope; 3],
                lin_side_offset: [p.lin_side_offset; 3],
                metadata: Default::default(),
            };
            ops.push(Transform::LogAffine(t));
        }
    }
}

/// Append a fixed function (port of `CreateFixedFunctionOp`).
pub fn add_fixed_function(
    ops: &mut TransformVec,
    style: FixedFunctionStyle,
    params: &[f64],
    dir: TransformDirection,
) {
    let mut t = FixedFunctionTransform::new(style, params);
    t.direction = dir;
    ops.push(Transform::FixedFunction(t));
}

/// Gamma styles used by the display builtins (OCIO's `GammaOpData::Style`,
/// restricted to the reverse styles the builtins use).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GammaStyle {
    /// `BASIC_REV`: inverse power, negative values clamped.
    BasicRev,
    /// `BASIC_MIRROR_REV`: inverse power, mirrored for negative values.
    BasicMirrorRev,
    /// `MONCURVE_REV`: inverse moncurve (with linear segment).
    MoncurveRev,
    /// `MONCURVE_MIRROR_REV`: inverse moncurve, mirrored for negative values.
    MoncurveMirrorRev,
}

/// Append a gamma curve applying `gamma` (and `offset` for the moncurve
/// styles) to RGB, alpha untouched.
pub fn add_gamma(ops: &mut TransformVec, style: GammaStyle, gamma: f64, offset: f64) {
    match style {
        GammaStyle::BasicRev | GammaStyle::BasicMirrorRev => {
            let mut t = ExponentTransform::new([gamma, gamma, gamma, 1.0]);
            t.negative_style = if style == GammaStyle::BasicRev {
                NegativeStyle::Clamp
            } else {
                NegativeStyle::Mirror
            };
            t.direction = TransformDirection::Inverse;
            ops.push(Transform::Exponent(t));
        }
        GammaStyle::MoncurveRev | GammaStyle::MoncurveMirrorRev => {
            let t = ExponentWithLinearTransform {
                direction: TransformDirection::Inverse,
                gamma: [gamma, gamma, gamma, 1.0],
                offset: [offset, offset, offset, 0.0],
                negative_style: if style == GammaStyle::MoncurveRev {
                    NegativeStyle::Linear
                } else {
                    NegativeStyle::Mirror
                },
                metadata: Default::default(),
            };
            ops.push(Transform::ExponentWithLinear(t));
        }
    }
}

/// Append a log-style RGB curve grading whose master curve is the given
/// B-spline (control points and slopes), the R, G, B curves being identity
/// (as the ACES 1 tone curves built with `GradingRGBCurveOpData`).
pub fn add_master_bspline_curve(ops: &mut TransformVec, points: &[(f32, f32)], slopes: &[f32]) {
    let mut curve = GradingBSplineCurve::new(points, BSplineType::BSpline);
    for (i, &s) in slopes.iter().enumerate() {
        curve.set_slope(i, s);
    }
    let identity = GradingBSplineCurve::new(&[(0.0, 0.0), (1.0, 1.0)], BSplineType::BSpline);

    let mut t = GradingRgbCurveTransform::new(GradingStyle::Log);
    *t.value.curve_mut(RgbCurveType::Red) = identity.clone();
    *t.value.curve_mut(RgbCurveType::Green) = identity.clone();
    *t.value.curve_mut(RgbCurveType::Blue) = identity;
    *t.value.curve_mut(RgbCurveType::Master) = curve;
    ops.push(Transform::GradingRgbCurve(t));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math_utils::equal_with_safe_rel_error;

    #[test]
    fn interpolate() {
        // Test the non-uniform 1D linear interpolation helper function.
        const LUT_SIZE: usize = 4;
        let lut_values = [0.0, 1.0, 0.50, 2.0, 0.75, 2.5, 1.0, 3.0];

        let check = |input: f64, aim: f64| {
            let v = interpolate_1d(LUT_SIZE, &lut_values, input).unwrap();
            assert!(
                equal_with_safe_rel_error(v, aim, 1e-7, 1.0),
                "{input}: {v} expected {aim}"
            );
        };
        check(-1.0, 1.0);
        check(0.0, 1.0);
        check(0.1, 1.2);
        check(0.5, 2.0);
        check(0.99, 2.98);
        check(2.0, 3.0);

        assert!(interpolate_1d(0, &lut_values, 0.5).is_err());
        assert!(interpolate_1d(1, &lut_values, f64::NAN).is_err());
    }

    #[test]
    fn luts() {
        let mut ops = TransformVec::new();
        create_lut(&mut ops, 3, |x| (2.0 * x) as f32);
        create_half_lut(&mut ops, |x| x as f32);
        create_lut_rgb(&mut ops, 2, |x| [x[0], 2.0 * x[1], 3.0 * x[2]]);
        assert_eq!(ops.len(), 3);

        let Transform::Lut1D(l) = &ops[0] else {
            panic!()
        };
        assert_eq!(l.length(), 3);
        assert_eq!(l.interpolation, Interpolation::Linear);
        assert_eq!(l.value(1), [1.0, 1.0, 1.0]);
        assert_eq!(l.value(2), [2.0, 2.0, 2.0]);

        let Transform::Lut1D(l) = &ops[1] else {
            panic!()
        };
        assert!(l.input_half_domain);
        assert_eq!(l.length(), 65536);
        // 1.0 in half.
        assert_eq!(l.value(0x3c00), [1.0, 1.0, 1.0]);
        // +Inf and -Inf.
        assert_eq!(l.value(0x7c00), [65504.0; 3]);
        assert_eq!(l.value(0xfc00), [-65504.0; 3]);
        // NaN.
        assert_eq!(l.value(0x7e00), [0.0; 3]);

        let Transform::Lut1D(l) = &ops[2] else {
            panic!()
        };
        assert_eq!(l.value(1), [1.0, 2.0, 3.0]);
    }
}
