//! Math helpers (port of the parts of `MathUtils.cpp` shared across modules).

/// Smallest positive normal `f32`, used as tolerance in comparisons.
pub const FLTMIN: f32 = f32::MIN_POSITIVE;

/// `|v| < FLTMIN`.
pub fn is_scalar_equal_to_zero_f32(v: f32) -> bool {
    v.abs() < FLTMIN
}

/// `|v| < FLTMIN` (double version, same tolerance as OCIO).
pub fn is_scalar_equal_to_zero(v: f64) -> bool {
    v.abs() < FLTMIN as f64
}

/// `|v - 1| < FLTMIN`.
pub fn is_scalar_equal_to_one(v: f64) -> bool {
    (v - 1.0).abs() < FLTMIN as f64
}

pub fn is_vec_equal_to_zero(v: &[f64]) -> bool {
    v.iter().all(|&x| is_scalar_equal_to_zero(x))
}

pub fn is_vec_equal_to_one(v: &[f64]) -> bool {
    v.iter().all(|&x| is_scalar_equal_to_one(x))
}

/// Absolute error comparison.
pub fn equal_with_abs_error(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

/// Relative error comparison: `|a-b| / max(|b|, min_expected) <= tol`.
pub fn equal_with_safe_rel_error(a: f64, b: f64, tol: f64, min_expected: f64) -> bool {
    let div = if b.abs() > min_expected { b.abs() } else { min_expected };
    ((a - b).abs() / div) <= tol
}

/// Clamp `v` into `[lo, hi]`; NaN maps to `lo` (like OCIO's `Clamp`).
pub fn clamp_f32(v: f32, lo: f32, hi: f32) -> f32 {
    if v > lo {
        if v < hi {
            v
        } else {
            hi
        }
    } else {
        lo
    }
}

/// Clamp `v` into `[lo, hi]`; NaN maps to `lo`.
pub fn clamp(v: f64, lo: f64, hi: f64) -> f64 {
    if v > lo {
        if v < hi {
            v
        } else {
            hi
        }
    } else {
        lo
    }
}

pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Replace NaN by 0.
pub fn sanitize_float(f: f32) -> f32 {
    if f.is_nan() {
        0.0
    } else {
        f
    }
}

// ---------------------------------------------------------------------------
// Matrices (row major).

/// Identity 4x4.
pub const M44_IDENTITY: [f64; 16] = [
    1.0, 0.0, 0.0, 0.0, //
    0.0, 1.0, 0.0, 0.0, //
    0.0, 0.0, 1.0, 0.0, //
    0.0, 0.0, 0.0, 1.0,
];

/// Identity 3x3.
pub const M33_IDENTITY: [f64; 9] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];

/// `a * b` (4x4).
pub fn m44_mult(a: &[f64; 16], b: &[f64; 16]) -> [f64; 16] {
    let mut r = [0.0; 16];
    for i in 0..4 {
        for j in 0..4 {
            r[i * 4 + j] = (0..4).map(|k| a[i * 4 + k] * b[k * 4 + j]).sum();
        }
    }
    r
}

/// `m * v` (4x4 by 4-vector).
pub fn m44_mult_vec(m: &[f64; 16], v: &[f64; 4]) -> [f64; 4] {
    let mut r = [0.0; 4];
    for i in 0..4 {
        r[i] = (0..4).map(|k| m[i * 4 + k] * v[k]).sum();
    }
    r
}

/// Inverse of a 4x4 matrix (Gauss-Jordan with partial pivoting). `None`
/// if singular.
pub fn m44_inverse(m: &[f64; 16]) -> Option<[f64; 16]> {
    let mut a = *m;
    let mut inv = M44_IDENTITY;
    for col in 0..4 {
        // Pivot.
        let mut piv = col;
        let mut best = a[col * 4 + col].abs();
        for r in (col + 1)..4 {
            let v = a[r * 4 + col].abs();
            if v > best {
                best = v;
                piv = r;
            }
        }
        if best == 0.0 || !best.is_finite() {
            return None;
        }
        if piv != col {
            for c in 0..4 {
                a.swap(col * 4 + c, piv * 4 + c);
                inv.swap(col * 4 + c, piv * 4 + c);
            }
        }
        let d = a[col * 4 + col];
        for c in 0..4 {
            a[col * 4 + c] /= d;
            inv[col * 4 + c] /= d;
        }
        for r in 0..4 {
            if r != col {
                let f = a[r * 4 + col];
                if f != 0.0 {
                    for c in 0..4 {
                        a[r * 4 + c] -= f * a[col * 4 + c];
                        inv[r * 4 + c] -= f * inv[col * 4 + c];
                    }
                }
            }
        }
    }
    Some(inv)
}

/// `a * b` (3x3).
pub fn m33_mult(a: &[f64; 9], b: &[f64; 9]) -> [f64; 9] {
    let mut r = [0.0; 9];
    for i in 0..3 {
        for j in 0..3 {
            r[i * 3 + j] = (0..3).map(|k| a[i * 3 + k] * b[k * 3 + j]).sum();
        }
    }
    r
}

/// `m * v` (3x3 by 3-vector).
pub fn m33_mult_vec(m: &[f64; 9], v: &[f64; 3]) -> [f64; 3] {
    [
        m[0] * v[0] + m[1] * v[1] + m[2] * v[2],
        m[3] * v[0] + m[4] * v[1] + m[5] * v[2],
        m[6] * v[0] + m[7] * v[1] + m[8] * v[2],
    ]
}

/// Inverse of a 3x3 matrix. `None` if singular.
pub fn m33_inverse(m: &[f64; 9]) -> Option<[f64; 9]> {
    let det = m[0] * (m[4] * m[8] - m[5] * m[7]) - m[1] * (m[3] * m[8] - m[5] * m[6])
        + m[2] * (m[3] * m[7] - m[4] * m[6]);
    if det == 0.0 || !det.is_finite() {
        return None;
    }
    let inv_det = 1.0 / det;
    Some([
        (m[4] * m[8] - m[5] * m[7]) * inv_det,
        (m[2] * m[7] - m[1] * m[8]) * inv_det,
        (m[1] * m[5] - m[2] * m[4]) * inv_det,
        (m[5] * m[6] - m[3] * m[8]) * inv_det,
        (m[0] * m[8] - m[2] * m[6]) * inv_det,
        (m[2] * m[3] - m[0] * m[5]) * inv_det,
        (m[3] * m[7] - m[4] * m[6]) * inv_det,
        (m[1] * m[6] - m[0] * m[7]) * inv_det,
        (m[0] * m[4] - m[1] * m[3]) * inv_det,
    ])
}

/// Embed a 3x3 matrix into a 4x4 (alpha untouched).
pub fn m33_to_m44(m: &[f64; 9]) -> [f64; 16] {
    let mut r = M44_IDENTITY;
    for i in 0..3 {
        for j in 0..3 {
            r[i * 4 + j] = m[i * 3 + j];
        }
    }
    r
}

/// Extract the upper-left 3x3 of a 4x4.
pub fn m44_to_m33(m: &[f64; 16]) -> [f64; 9] {
    let mut r = [0.0; 9];
    for i in 0..3 {
        for j in 0..3 {
            r[i * 3 + j] = m[i * 4 + j];
        }
    }
    r
}

/// Half float helpers.
pub fn half_to_f32(bits: u16) -> f32 {
    half::f16::from_bits(bits).to_f32()
}

pub fn f32_to_half_bits(v: f32) -> u16 {
    half::f16::from_f32(v).to_bits()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inverse44() {
        let m = [2.0, 0.0, 0.0, 1.0, 0.0, 4.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
        let inv = m44_inverse(&m).unwrap();
        let id = m44_mult(&m, &inv);
        for i in 0..16 {
            assert!((id[i] - M44_IDENTITY[i]).abs() < 1e-12);
        }
        assert!(m44_inverse(&[0.0; 16]).is_none());
    }

    #[test]
    fn inverse33() {
        let m = [0.4124, 0.3576, 0.1805, 0.2126, 0.7152, 0.0722, 0.0193, 0.1192, 0.9505];
        let inv = m33_inverse(&m).unwrap();
        let id = m33_mult(&m, &inv);
        for i in 0..9 {
            assert!((id[i] - M33_IDENTITY[i]).abs() < 1e-12);
        }
    }
}
