//! Small vector / matrix helpers and color primaries used by the ACES 2
//! output transform (port of `ACES2/MatrixLib.h`, `ACES2/ColorLib.h` and the
//! parts of `transforms/builtins/ColorMatrixHelpers.cpp` they rely on).
//!
//! The 3x3 float matrices are stored in row-major order. The primaries
//! conversion matrices are computed in double precision on 4x4 matrices,
//! exactly as OCIO's `MatrixOpData::MatrixArray` does, and then rounded to
//! float.

use crate::error::{Error, Result};

/// Two floats.
pub type F2 = [f32; 2];
/// Three floats.
pub type F3 = [f32; 3];
/// A row-major 3x3 float matrix.
pub type M33f = [f32; 9];

/// The 3x3 identity matrix.
pub const IDENTITY_M33: M33f = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];

/// `[v, v, v]`.
#[inline]
pub fn f3_from_f(v: f32) -> F3 {
    [v, v, v]
}

/// `v * f3`.
#[inline]
pub fn mult_f_f3(v: f32, f3: &F3) -> F3 {
    [v * f3[0], v * f3[1], v * f3[2]]
}

/// Matrix times column vector (`mat33 * f3`).
#[inline]
pub fn mult_f3_f33(f3: &F3, mat33: &M33f) -> F3 {
    [
        f3[0] * mat33[0] + f3[1] * mat33[1] + f3[2] * mat33[2],
        f3[0] * mat33[3] + f3[1] * mat33[4] + f3[2] * mat33[5],
        f3[0] * mat33[6] + f3[1] * mat33[7] + f3[2] * mat33[8],
    ]
}

/// Matrix product `a * b`.
pub fn mult_f33_f33(a: &M33f, b: &M33f) -> M33f {
    [
        a[0] * b[0] + a[1] * b[3] + a[2] * b[6],
        a[0] * b[1] + a[1] * b[4] + a[2] * b[7],
        a[0] * b[2] + a[1] * b[5] + a[2] * b[8],
        a[3] * b[0] + a[4] * b[3] + a[5] * b[6],
        a[3] * b[1] + a[4] * b[4] + a[5] * b[7],
        a[3] * b[2] + a[4] * b[5] + a[5] * b[8],
        a[6] * b[0] + a[7] * b[3] + a[8] * b[6],
        a[6] * b[1] + a[7] * b[4] + a[8] * b[7],
        a[6] * b[2] + a[7] * b[5] + a[8] * b[8],
    ]
}

/// Scale the diagonal of a matrix (literal port of OCIO's `scale_f33`,
/// which also transposes the off-diagonal entries).
pub fn scale_f33(mat33: &M33f, scale: &F3) -> M33f {
    [
        mat33[0] * scale[0],
        mat33[3],
        mat33[6],
        mat33[1],
        mat33[4] * scale[1],
        mat33[7],
        mat33[2],
        mat33[5],
        mat33[8] * scale[2],
    ]
}

/// A row-major 4x4 double matrix (OCIO's `MatrixOpData::MatrixArray`).
pub type M44d = [f64; 16];

const M44_IDENTITY: M44d = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

/// `a * b` (port of `MatrixArray::inner`).
fn m44_inner(a: &M44d, b: &M44d) -> M44d {
    let mut out = [0.0; 16];
    for row in 0..4 {
        for col in 0..4 {
            let mut accum = 0.0;
            for i in 0..4 {
                accum += a[row * 4 + i] * b[i * 4 + col];
            }
            out[row * 4 + col] = accum;
        }
    }
    out
}

/// Gauss-Jordan inverse (port of `MatrixArray::inverse`, itself taken from
/// Imath's `Matrix44::gjInverse`).
fn m44_inverse(m: &M44d) -> Result<M44d> {
    let mut t = *m;
    let mut s = M44_IDENTITY;
    let singular = || Error::msg("Singular Matrix can't be inverted.");

    // Forward elimination.
    for i in 0..3 {
        let mut pivot = i;
        let mut pivotsize = t[i * 4 + i].abs();
        for j in (i + 1)..4 {
            let tmp = t[j * 4 + i].abs();
            if tmp > pivotsize {
                pivot = j;
                pivotsize = tmp;
            }
        }
        if pivotsize == 0.0 {
            return Err(singular());
        }
        if pivot != i {
            for j in 0..4 {
                t.swap(i * 4 + j, pivot * 4 + j);
                s.swap(i * 4 + j, pivot * 4 + j);
            }
        }
        for j in (i + 1)..4 {
            let f = t[j * 4 + i] / t[i * 4 + i];
            for k in 0..4 {
                t[j * 4 + k] -= f * t[i * 4 + k];
                s[j * 4 + k] -= f * s[i * 4 + k];
            }
        }
    }

    // Backward substitution.
    for i in (0..4).rev() {
        let f = t[i * 4 + i];
        if f == 0.0 {
            return Err(singular());
        }
        for j in 0..4 {
            t[i * 4 + j] /= f;
            s[i * 4 + j] /= f;
        }
        for j in 0..i {
            let f = t[j * 4 + i];
            for k in 0..4 {
                t[j * 4 + k] -= f * t[i * 4 + k];
                s[j * 4 + k] -= f * s[i * 4 + k];
            }
        }
    }
    Ok(s)
}

/// Round the upper-left 3x3 part of a 4x4 double matrix to float.
fn m33_from_m44(v: &M44d) -> M33f {
    [
        v[0] as f32,
        v[1] as f32,
        v[2] as f32,
        v[4] as f32,
        v[5] as f32,
        v[6] as f32,
        v[8] as f32,
        v[9] as f32,
        v[10] as f32,
    ]
}

/// Invert a 3x3 float matrix (computed in double precision, as in OCIO).
pub fn invert_f33(mat33: &M33f) -> Result<M33f> {
    let mut v = M44_IDENTITY;
    v[0] = f64::from(mat33[0]);
    v[1] = f64::from(mat33[1]);
    v[2] = f64::from(mat33[2]);
    v[4] = f64::from(mat33[3]);
    v[5] = f64::from(mat33[4]);
    v[6] = f64::from(mat33[5]);
    v[8] = f64::from(mat33[6]);
    v[9] = f64::from(mat33[7]);
    v[10] = f64::from(mat33[8]);
    Ok(m33_from_m44(&m44_inverse(&v)?))
}

/// CIE xy chromaticity coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Chromaticities {
    pub xy: [f64; 2],
}

impl Chromaticities {
    /// Build from `x` and `y`.
    pub const fn new(x: f64, y: f64) -> Self {
        Self { xy: [x, y] }
    }
}

/// Chromaticities of the red, green and blue primaries and of the white.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Primaries {
    pub red: Chromaticities,
    pub grn: Chromaticities,
    pub blu: Chromaticities,
    pub wht: Chromaticities,
}

impl Primaries {
    /// Build from the four chromaticities.
    pub const fn new(
        red: Chromaticities,
        grn: Chromaticities,
        blu: Chromaticities,
        wht: Chromaticities,
    ) -> Self {
        Self { red, grn, blu, wht }
    }

    /// Build from float coordinates (the ACES 2 renderers convert their
    /// double parameters to float before building the primaries).
    pub fn from_f32(v: &[f32; 8]) -> Self {
        let c = |x: f32, y: f32| Chromaticities::new(f64::from(x), f64::from(y));
        Self::new(c(v[0], v[1]), c(v[2], v[3]), c(v[4], v[5]), c(v[6], v[7]))
    }
}

/// CIE XYZ with illuminant E.
pub const CIE_XYZ_ILLUM_E: Primaries = Primaries::new(
    Chromaticities::new(1.0, 0.0),
    Chromaticities::new(0.0, 1.0),
    Chromaticities::new(0.0, 0.0),
    Chromaticities::new(1.0 / 3.0, 1.0 / 3.0),
);

/// ACES AP0 primaries (SMPTE ST2065-1).
pub const ACES_AP0: Primaries = Primaries::new(
    Chromaticities::new(0.7347, 0.2653),
    Chromaticities::new(0.0000, 1.0000),
    Chromaticities::new(0.0001, -0.0770),
    Chromaticities::new(0.32168, 0.33767),
);

/// ACES AP1 primaries.
pub const ACES_AP1: Primaries = Primaries::new(
    Chromaticities::new(0.713, 0.293),
    Chromaticities::new(0.165, 0.830),
    Chromaticities::new(0.128, 0.044),
    Chromaticities::new(0.32168, 0.33767),
);

/// Matrix converting RGB with the given primaries to XYZ (port of
/// `rgb2xyz_from_xy`).
fn rgb2xyz_from_xy(p: &Primaries) -> Result<M44d> {
    let mut matrix = M44_IDENTITY;
    matrix[0] = p.red.xy[0];
    matrix[4] = p.red.xy[1];
    matrix[8] = 1.0 - p.red.xy[0] - p.red.xy[1];

    matrix[1] = p.grn.xy[0];
    matrix[5] = p.grn.xy[1];
    matrix[9] = 1.0 - p.grn.xy[0] - p.grn.xy[1];

    matrix[2] = p.blu.xy[0];
    matrix[6] = p.blu.xy[1];
    matrix[10] = 1.0 - p.blu.xy[0] - p.blu.xy[1];

    let inv = m44_inverse(&matrix)?;

    let wht_xyz = [
        p.wht.xy[0] / p.wht.xy[1],
        1.0,
        (1.0 - p.wht.xy[0] - p.wht.xy[1]) / p.wht.xy[1],
    ];

    let mut rgb2xyz = M44_IDENTITY;
    for i in 0..3 {
        let gain =
            wht_xyz[0] * inv[i * 4] + wht_xyz[1] * inv[i * 4 + 1] + wht_xyz[2] * inv[i * 4 + 2];
        for j in 0..3 {
            rgb2xyz[j * 4 + i] = gain * matrix[j * 4 + i];
        }
    }
    Ok(rgb2xyz)
}

/// Conversion matrix between two sets of primaries without chromatic
/// adaptation (port of `build_conversion_matrix(src, dst, ADAPTATION_NONE)`).
fn build_conversion_matrix_no_adaptation(src: &Primaries, dst: &Primaries) -> Result<M44d> {
    let src_rgb2xyz = rgb2xyz_from_xy(src)?;
    let dst_rgb2xyz = rgb2xyz_from_xy(dst)?;
    let dst_xyz2rgb = m44_inverse(&dst_rgb2xyz)?;
    Ok(m44_inner(&dst_xyz2rgb, &src_rgb2xyz))
}

/// RGB (given primaries) to CIE XYZ (illuminant E), in float.
pub fn rgb_to_xyz_f33(c: &Primaries) -> Result<M33f> {
    Ok(m33_from_m44(&build_conversion_matrix_no_adaptation(
        c,
        &CIE_XYZ_ILLUM_E,
    )?))
}

/// CIE XYZ (illuminant E) to RGB (given primaries), in float.
pub fn xyz_to_rgb_f33(c: &Primaries) -> Result<M33f> {
    let m = build_conversion_matrix_no_adaptation(c, &CIE_XYZ_ILLUM_E)?;
    Ok(m33_from_m44(&m44_inverse(&m)?))
}

/// RGB to RGB conversion between two sets of primaries, in float.
pub fn rgb_to_rgb_f33(src: &Primaries, dst: &Primaries) -> Result<M33f> {
    Ok(mult_f33_f33(&xyz_to_rgb_f33(dst)?, &rgb_to_xyz_f33(src)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inverse_roundtrip() {
        let m: M33f = [2.0, 1.0, 0.5, 0.0, 3.0, 1.0, 1.0, 0.0, 4.0];
        let inv = invert_f33(&m).unwrap();
        let id = mult_f33_f33(&m, &inv);
        for (a, b) in id.iter().zip(IDENTITY_M33.iter()) {
            assert!((a - b).abs() < 1e-6);
        }
        assert!(invert_f33(&[0.0; 9]).is_err());
    }

    #[test]
    fn ap0_to_xyz() {
        // Known AP0 to XYZ (no adaptation, illuminant E white on the XYZ side
        // does not change the RGB -> XYZ part of the matrix).
        let m = rgb_to_xyz_f33(&ACES_AP0).unwrap();
        // White (1,1,1) maps to the AP0 white point XYZ normalized to Y = 1,
        // then through the XYZ(E) conversion which is an identity.
        let w = mult_f3_f33(&[1.0, 1.0, 1.0], &m);
        assert!((w[1] - 1.0).abs() < 1e-6);
        assert!((w[0] - (0.32168 / 0.33767) as f32).abs() < 1e-6);
    }
}
