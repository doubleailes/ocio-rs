// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Copyright (c) 2026 Philippe Llerena.

//! Port of the SSE math of OCIO (`SSE.h`): the polynomial `log2`, `exp2`
//! and `pow` approximations (about 15 bits of mantissa) used by the `*SSE`
//! CPU renderers.
//!
//! The functions work on one lane and reproduce the SSE2 instructions
//! exactly (same single precision operations in the same order, same
//! integer conversions, comparisons and bit masks), so the renderers built
//! on them give the results of OCIO's SSE2 builds bit for bit, on any
//! target.

// Bit masks and constants of `SSE.h`.
const EXP_MASK: u32 = 0x7F80_0000;
const EXP_BIAS: i32 = 127;
const EXP_SHIFT: u32 = 23;
const SIGN_MASK: u32 = 0x8000_0000;
const ABS_MASK: u32 = 0x7FFF_FFFF;

// Coefficients of the Chebyshev (minimax) polynomials. The C++ code rounds
// the double literals to float, which is reproduced here (parsing them as
// f32 literals could round differently).
const PNLOG5: f32 = 4.487361286440374006195e-2_f64 as f32;
const PNLOG4: f32 = -4.165637071209677112635e-1_f64 as f32;
const PNLOG3: f32 = 1.631148826119436277100_f64 as f32;
const PNLOG2: f32 = -3.550793018041176193407_f64 as f32;
const PNLOG1: f32 = 5.091710879305474367557_f64 as f32;
const PNLOG0: f32 = -2.800364054395965731506_f64 as f32;

const PNEXP4: f32 = 1.353416792833547468620e-2_f64 as f32;
const PNEXP3: f32 = 5.201146058412685018921e-2_f64 as f32;
const PNEXP2: f32 = 2.414427569091865207710e-1_f64 as f32;
const PNEXP1: f32 = 6.930038344665415134202e-1_f64 as f32;
const PNEXP0: f32 = 1.000002593370603213644_f64 as f32;

/// `_mm_max_ps(a, b)`: `a > b ? a : b`, so `b` is returned when either
/// value is a NaN.
#[inline]
pub fn max(a: f32, b: f32) -> f32 {
    if a > b {
        a
    } else {
        b
    }
}

/// `_mm_min_ps(a, b)`: `a < b ? a : b`, so `b` is returned when either
/// value is a NaN.
#[inline]
pub fn min(a: f32, b: f32) -> f32 {
    if a < b {
        a
    } else {
        b
    }
}

/// The sign bit of `x` (`_mm_and_ps(x, ESIGN_MASK)`), as raw bits.
#[inline]
pub fn sign_bits(x: f32) -> u32 {
    x.to_bits() & SIGN_MASK
}

/// `_mm_and_ps(x, EABS_MASK)`: `x` without its sign bit.
#[inline]
pub fn abs(x: f32) -> f32 {
    f32::from_bits(x.to_bits() & ABS_MASK)
}

/// `_mm_or_ps(sign, x)`: set the sign bit of `x` when `sign` has it.
#[inline]
pub fn or_sign(sign: u32, x: f32) -> f32 {
    f32::from_bits(sign | x.to_bits())
}

/// The result of an SSE arithmetic instruction on `a` and `b` whose IEEE
/// result is `r`, with the x86 NaN rules: the first NaN operand (quieted) is
/// returned, and an invalid operation (e.g. `0 * inf`) gives the default
/// NaN, whose sign bit is set. (Rust does not specify which NaN an operation
/// returns, and the compiler may swap the operands of an addition, so these
/// rules are applied explicitly where the sign of a NaN matters.)
#[inline]
fn x86_nan(a: f32, b: f32, r: f32) -> f32 {
    const QUIET: u32 = 0x0040_0000;
    if a.is_nan() {
        f32::from_bits(a.to_bits() | QUIET)
    } else if b.is_nan() {
        f32::from_bits(b.to_bits() | QUIET)
    } else if r.is_nan() {
        f32::from_bits(0xFFC0_0000)
    } else {
        r
    }
}

/// `_mm_add_ps(a, b)` with the x86 NaN rules (see [`x86_nan`]).
#[inline]
pub fn add(a: f32, b: f32) -> f32 {
    x86_nan(a, b, a + b)
}

/// `_mm_sub_ps(a, b)` with the x86 NaN rules (see [`x86_nan`]).
#[inline]
pub fn sub(a: f32, b: f32) -> f32 {
    x86_nan(a, b, a - b)
}

/// `_mm_mul_ps(a, b)` with the x86 NaN rules (see [`x86_nan`]).
#[inline]
pub fn mul(a: f32, b: f32) -> f32 {
    x86_nan(a, b, a * b)
}

/// `_mm_xor_ps(x, sign)`: flip the sign of `x` when `sign` has the sign bit.
#[inline]
pub fn xor_sign(x: f32, sign: u32) -> f32 {
    f32::from_bits(x.to_bits() ^ sign)
}

/// The luma of an RGBA pixel as the SSE renderers compute it: the four
/// lanes are weighted (alpha by 0) and summed pairwise by two shuffles, so
/// the result is `(r * wr + g * wg) + (b * wb + a * 0)`, and a non finite
/// alpha makes it NaN.
#[inline]
pub fn luma(p: &[f32; 4], weights: &[f32; 3]) -> f32 {
    (p[0] * weights[0] + p[1] * weights[1]) + (p[2] * weights[2] + p[3] * 0.0)
}

/// `_mm_cvttps_epi32`: conversion by truncation, giving the "integer
/// indefinite" value `i32::MIN` for a NaN or an out of range value.
#[inline]
fn cvttps_epi32(x: f32) -> i32 {
    if !(-2_147_483_648.0..2_147_483_648.0).contains(&x) {
        i32::MIN
    } else {
        x as i32
    }
}

/// Port of `sseLog2`.
#[inline]
pub fn log2(x: f32) -> f32 {
    let bits = x.to_bits();

    // The mantissa with the exponent of 1 (the sign bit is kept).
    let mantissa = f32::from_bits((bits & !EXP_MASK) | 1.0f32.to_bits());

    let mut log2 = PNLOG5 * mantissa + PNLOG4;
    log2 = log2 * mantissa + PNLOG3;
    log2 = log2 * mantissa + PNLOG2;
    log2 = log2 * mantissa + PNLOG1;
    log2 = log2 * mantissa + PNLOG0;

    let exponent = ((bits & EXP_MASK) >> EXP_SHIFT) as i32 - EXP_BIAS;
    log2 + exponent as f32
}

/// Port of `sseExp2`.
#[inline]
pub fn exp2(x: f32) -> f32 {
    // floor(x): truncation, minus 1 when !(0 <= x) (i.e. x < 0 or NaN). The
    // value is wrong outside the int range or for a NaN, but the result is
    // then fixed below (or is a NaN).
    let floor_x = cvttps_epi32(x).wrapping_add(if 0.0 <= x { 0 } else { -1 });

    // exp2(floor_x), built from the exponent bits.
    let zf = f32::from_bits((floor_x.wrapping_add(EXP_BIAS) as u32) << EXP_SHIFT);

    let iexp = floor_x as f32;
    let fraction = x - iexp;

    let mut mexp = PNEXP4 * fraction + PNEXP3;
    mexp = mexp * fraction + PNEXP2;
    mexp = mexp * fraction + PNEXP1;
    mexp = mexp * fraction + PNEXP0;

    let mut exp2 = zf * mexp;

    // Underflow.
    if x < -126.0 {
        exp2 = 0.0;
    }
    // Overflow.
    if x >= 128.0 {
        exp2 = f32::INFINITY;
    }
    exp2
}

/// Port of `ssePower`: `exp2(exp * log2(x))`, and 0 when `x` is not greater
/// than 0 (NaN included).
#[inline]
pub fn power(x: f32, exp: f32) -> f32 {
    let values = exp2(exp * log2(x));
    if x > 0.0 {
        values
    } else {
        0.0
    }
}

// ---------------------------------------------------------------------------
// Four lanes at once.
//
// The renderers evaluate the polynomial approximations on the four channels
// of a pixel with these functions. On x86-64, whose baseline includes SSE2,
// they use the SSE2 instructions of `SSE.h` (the same instructions as OCIO,
// so the same results as the lane by lane versions above, which the tests
// check); elsewhere they apply the lane by lane versions.

/// [`log2`] of four lanes.
#[inline]
pub fn log2_4(x: [f32; 4]) -> [f32; 4] {
    #[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
    {
        x86::log2_4(x)
    }
    #[cfg(not(all(target_arch = "x86_64", target_feature = "sse2")))]
    {
        x.map(log2)
    }
}

/// [`exp2`] of four lanes.
#[inline]
pub fn exp2_4(x: [f32; 4]) -> [f32; 4] {
    #[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
    {
        x86::exp2_4(x)
    }
    #[cfg(not(all(target_arch = "x86_64", target_feature = "sse2")))]
    {
        x.map(exp2)
    }
}

/// [`power`] of four lanes.
#[inline]
pub fn power_4(x: [f32; 4], exp: [f32; 4]) -> [f32; 4] {
    #[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
    {
        x86::power_4(x, exp)
    }
    #[cfg(not(all(target_arch = "x86_64", target_feature = "sse2")))]
    {
        [0, 1, 2, 3].map(|i| power(x[i], exp[i]))
    }
}

/// The SSE2 versions of `sseLog2`, `sseExp2` and `ssePower`.
///
/// SAFETY: the intrinsics only require the `sse` / `sse2` target features,
/// which this module is compiled for (they are part of the x86-64 baseline);
/// they do not access memory.
#[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
#[allow(unsafe_code)]
mod x86 {
    use super::*;
    use std::arch::x86_64::*;

    #[inline(always)]
    fn load(x: [f32; 4]) -> __m128 {
        // SAFETY: see the module documentation.
        unsafe { _mm_setr_ps(x[0], x[1], x[2], x[3]) }
    }

    #[inline(always)]
    fn store(v: __m128) -> [f32; 4] {
        // SAFETY: see the module documentation.
        unsafe {
            [
                _mm_cvtss_f32(v),
                _mm_cvtss_f32(_mm_shuffle_ps::<0b01_01_01_01>(v, v)),
                _mm_cvtss_f32(_mm_shuffle_ps::<0b10_10_10_10>(v, v)),
                _mm_cvtss_f32(_mm_shuffle_ps::<0b11_11_11_11>(v, v)),
            ]
        }
    }

    #[inline(always)]
    fn log2(x: __m128) -> __m128 {
        // SAFETY: see the module documentation.
        unsafe {
            let exp_mask = _mm_castsi128_ps(_mm_set1_epi32(EXP_MASK as i32));
            // The mantissa in [1, 2), with the sign of x.
            let mantissa = _mm_or_ps(_mm_andnot_ps(exp_mask, x), _mm_set1_ps(1.0));
            let mut log2 = _mm_add_ps(
                _mm_mul_ps(_mm_set1_ps(PNLOG5), mantissa),
                _mm_set1_ps(PNLOG4),
            );
            log2 = _mm_add_ps(_mm_mul_ps(log2, mantissa), _mm_set1_ps(PNLOG3));
            log2 = _mm_add_ps(_mm_mul_ps(log2, mantissa), _mm_set1_ps(PNLOG2));
            log2 = _mm_add_ps(_mm_mul_ps(log2, mantissa), _mm_set1_ps(PNLOG1));
            log2 = _mm_add_ps(_mm_mul_ps(log2, mantissa), _mm_set1_ps(PNLOG0));
            let exponent = _mm_sub_epi32(
                _mm_srli_epi32::<{ EXP_SHIFT as i32 }>(_mm_castps_si128(_mm_and_ps(x, exp_mask))),
                _mm_set1_epi32(EXP_BIAS),
            );
            _mm_add_ps(log2, _mm_cvtepi32_ps(exponent))
        }
    }

    #[inline(always)]
    fn exp2(x: __m128) -> __m128 {
        // SAFETY: see the module documentation.
        unsafe {
            // floor(x): the truncation minus 1 when x is not >= 0.
            let below_zero = _mm_castps_si128(_mm_cmpnle_ps(_mm_setzero_ps(), x));
            let floor_x = _mm_add_epi32(_mm_cvttps_epi32(x), below_zero);
            let zf = _mm_castsi128_ps(_mm_slli_epi32::<{ EXP_SHIFT as i32 }>(_mm_add_epi32(
                floor_x,
                _mm_set1_epi32(EXP_BIAS),
            )));
            let fraction = _mm_sub_ps(x, _mm_cvtepi32_ps(floor_x));
            let mut mexp = _mm_add_ps(
                _mm_mul_ps(_mm_set1_ps(PNEXP4), fraction),
                _mm_set1_ps(PNEXP3),
            );
            mexp = _mm_add_ps(_mm_mul_ps(mexp, fraction), _mm_set1_ps(PNEXP2));
            mexp = _mm_add_ps(_mm_mul_ps(mexp, fraction), _mm_set1_ps(PNEXP1));
            mexp = _mm_add_ps(_mm_mul_ps(mexp, fraction), _mm_set1_ps(PNEXP0));
            let exp2 = _mm_mul_ps(zf, mexp);
            // 0 below -126, +Inf from 128.
            let exp2 = _mm_andnot_ps(_mm_cmplt_ps(x, _mm_set1_ps(-126.0)), exp2);
            let overflow = _mm_cmpge_ps(x, _mm_set1_ps(128.0));
            _mm_or_ps(
                _mm_andnot_ps(overflow, exp2),
                _mm_and_ps(overflow, _mm_set1_ps(f32::INFINITY)),
            )
        }
    }

    #[inline]
    pub(super) fn log2_4(x: [f32; 4]) -> [f32; 4] {
        store(log2(load(x)))
    }

    #[inline]
    pub(super) fn exp2_4(x: [f32; 4]) -> [f32; 4] {
        store(exp2(load(x)))
    }

    #[inline]
    pub(super) fn power_4(x: [f32; 4], exp: [f32; 4]) -> [f32; 4] {
        let x = load(x);
        // SAFETY: see the module documentation.
        let values = unsafe { exp2(_mm_mul_ps(load(exp), log2(x))) };
        // SAFETY: see the module documentation.
        store(unsafe { _mm_and_ps(_mm_cmpgt_ps(x, _mm_setzero_ps()), values) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log2_accuracy() {
        // About 15 bits of mantissa.
        for i in 1..2000 {
            let x = i as f32 * 0.01;
            let err = (log2(x) - x.log2()).abs();
            assert!(err < 5e-5, "log2({x}): {} vs {}", log2(x), x.log2());
        }
        // The powers of 2 are exact up to the polynomial at 1 (1.3e-5).
        assert!((log2(8.0) - 3.0).abs() < 2e-5);
    }

    #[test]
    fn exp2_accuracy_and_limits() {
        for i in -2000..2000 {
            let x = i as f32 * 0.01;
            let rel = ((exp2(x) - x.exp2()) / x.exp2()).abs();
            assert!(rel < 5e-5, "exp2({x}): {} vs {}", exp2(x), x.exp2());
        }
        assert_eq!(exp2(-126.5), 0.0);
        assert_eq!(exp2(-1e30), 0.0);
        assert_eq!(exp2(f32::NEG_INFINITY), 0.0);
        assert_eq!(exp2(128.0), f32::INFINITY);
        assert_eq!(exp2(f32::INFINITY), f32::INFINITY);
        assert!(exp2(f32::NAN).is_nan());
    }

    #[test]
    fn power_values() {
        // OCIO's GammaOpCPU tests: 1^1 is not exactly 1.
        assert_eq!(power(1.0, 1.0), 1.000_011_6);
        assert_eq!(power(0.0, 2.2), 0.0);
        assert_eq!(power(-1.0, 2.2), 0.0);
        assert_eq!(power(f32::NAN, 2.2), 0.0);
        // A huge base with a small exponent stays finite.
        assert!(power(f32::INFINITY, 0.5).is_finite());
        let rel = ((power(0.5, 2.2) - 0.5f32.powf(2.2)) / 0.5f32.powf(2.2)).abs();
        assert!(rel < 1e-4);
    }

    #[test]
    fn nan_rules() {
        let pos_nan = f32::from_bits(0x7FC0_0000);
        let neg_nan = f32::from_bits(0xFFC0_0000);
        // The first NaN operand is returned.
        assert_eq!(add(neg_nan, pos_nan).to_bits(), 0xFFC0_0000);
        assert_eq!(add(pos_nan, neg_nan).to_bits(), 0x7FC0_0000);
        assert_eq!(add(1.0, pos_nan).to_bits(), 0x7FC0_0000);
        // Invalid operations give the default (negative) NaN.
        assert_eq!(mul(0.0, f32::INFINITY).to_bits(), 0xFFC0_0000);
        assert_eq!(add(f32::INFINITY, f32::NEG_INFINITY).to_bits(), 0xFFC0_0000);
        // Signaling NaNs are quieted.
        assert_eq!(mul(f32::from_bits(0x7F80_0001), 1.0).to_bits(), 0x7FC0_0001);
        assert_eq!(add(1.5, 2.0), 3.5);
    }

    #[test]
    fn min_max_nan() {
        assert_eq!(max(f32::NAN, 1.0), 1.0);
        assert!(max(1.0, f32::NAN).is_nan());
        assert_eq!(min(f32::NAN, 1.0), 1.0);
        assert!(min(1.0, f32::NAN).is_nan());
    }

    /// Values covering every exponent with varied mantissas, both signs, the
    /// special values and the ranges the polynomials are used on.
    fn samples() -> Vec<f32> {
        let mut v: Vec<f32> = (0..=u32::MAX).step_by(65_521).map(f32::from_bits).collect();
        v.extend((-3000..3000).map(|i| i as f32 * 0.05));
        v.extend([
            0.0,
            -0.0,
            1.0,
            -126.0,
            -126.5,
            127.99999,
            128.0,
            f32::MIN_POSITIVE,
            f32::from_bits(1),
            f32::MAX,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NAN,
            -f32::NAN,
            f32::from_bits(0x7F80_0001),
        ]);
        v
    }

    fn same(a: f32, b: f32) -> bool {
        a.to_bits() == b.to_bits()
    }

    #[test]
    fn four_lane_versions_match_the_lane_versions() {
        let s = samples();
        let exps = [2.2, 1.0 / 2.4, 0.45, 1.0, -0.7, 0.0, 30.0, f32::NAN];
        for (i, quad) in s.chunks_exact(4).enumerate() {
            let x = [quad[0], quad[1], quad[2], quad[3]];
            let (l, e) = (log2_4(x), exp2_4(x));
            let exp = [0, 1, 2, 3].map(|c| exps[(i + c) % exps.len()]);
            let p = power_4(x, exp);
            for c in 0..4 {
                assert!(same(l[c], log2(x[c])), "log2({:#x})", x[c].to_bits());
                assert!(same(e[c], exp2(x[c])), "exp2({:#x})", x[c].to_bits());
                assert!(
                    same(p[c], power(x[c], exp[c])),
                    "power({:#x}, {})",
                    x[c].to_bits(),
                    exp[c]
                );
            }
        }
    }
}
