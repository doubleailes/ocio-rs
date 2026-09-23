//! Color matrix helpers: RGB primaries to CIE XYZ matrices and von Kries
//! chromatic adaptation (port of `ColorMatrixHelpers.cpp`).
//!
//! All matrices are 4x4, row major, with an identity alpha row/column (as
//! OCIO's `MatrixOpData::MatrixArray`), so they can be used directly in a
//! [`MatrixTransform`](crate::transforms::MatrixTransform).

use crate::error::{Error, Result};

/// A 4x4 row-major matrix.
pub type Matrix44 = [f64; 16];

/// A 4-component vector (OCIO's `MatrixOpData::Offsets`).
pub type Vec4 = [f64; 4];

/// CIE xy chromaticity coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Chromaticities {
    pub xy: [f64; 2],
}

impl Chromaticities {
    /// Chromaticity from its x and y coordinates.
    pub const fn new(x: f64, y: f64) -> Self {
        Self { xy: [x, y] }
    }
}

/// RGB primaries and white point chromaticities of a color space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Primaries {
    /// Red primary.
    pub red: Chromaticities,
    /// Green primary.
    pub grn: Chromaticities,
    /// Blue primary.
    pub blu: Chromaticities,
    /// White (or gray) point.
    pub wht: Chromaticities,
}

impl Primaries {
    /// Primaries from `(x, y)` pairs for red, green, blue and white.
    pub const fn new(red: (f64, f64), grn: (f64, f64), blu: (f64, f64), wht: (f64, f64)) -> Self {
        Self {
            red: Chromaticities::new(red.0, red.1),
            grn: Chromaticities::new(grn.0, grn.1),
            blu: Chromaticities::new(blu.0, blu.1),
            wht: Chromaticities::new(wht.0, wht.1),
        }
    }
}

/// CIE XYZ with illuminant E (equal energy) white.
pub const CIE_XYZ_ILLUM_E: Primaries =
    Primaries::new((1.0, 0.0), (0.0, 1.0), (0.0, 0.0), (1.0 / 3.0, 1.0 / 3.0));

/// ACES AP0 primaries (SMPTE ST2065-1).
pub const ACES_AP0: Primaries = Primaries::new(
    (0.7347, 0.2653),
    (0.0000, 1.0000),
    (0.0001, -0.0770),
    (0.32168, 0.33767),
);

/// ACES AP1 primaries.
pub const ACES_AP1: Primaries = Primaries::new(
    (0.713, 0.293),
    (0.165, 0.830),
    (0.128, 0.044),
    (0.32168, 0.33767),
);

/// Rec.709 primaries, D65 white.
pub const REC709: Primaries =
    Primaries::new((0.64, 0.33), (0.30, 0.60), (0.15, 0.06), (0.3127, 0.3290));

/// Rec.709 primaries, D60 (ACES) white.
pub const REC709_D60: Primaries =
    Primaries::new((0.64, 0.33), (0.30, 0.60), (0.15, 0.06), (0.32168, 0.33767));

/// Rec.2020 primaries, D65 white.
pub const REC2020: Primaries = Primaries::new(
    (0.708, 0.292),
    (0.170, 0.797),
    (0.131, 0.046),
    (0.3127, 0.3290),
);

/// Rec.2020 primaries, D60 (ACES) white.
pub const REC2020_D60: Primaries = Primaries::new(
    (0.708, 0.292),
    (0.170, 0.797),
    (0.131, 0.046),
    (0.32168, 0.33767),
);

/// P3 primaries, DCI white.
pub const P3_DCI: Primaries = Primaries::new(
    (0.680, 0.320),
    (0.265, 0.690),
    (0.150, 0.060),
    (0.314, 0.351),
);

/// P3 primaries, D65 white.
pub const P3_D65: Primaries = Primaries::new(
    (0.680, 0.320),
    (0.265, 0.690),
    (0.150, 0.060),
    (0.3127, 0.3290),
);

/// P3 primaries, D60 (ACES) white.
pub const P3_D60: Primaries = Primaries::new(
    (0.680, 0.320),
    (0.265, 0.690),
    (0.150, 0.060),
    (0.32168, 0.33767),
);

/// White point XYZ values (Y = 1).
pub mod whitepoint {
    use super::Vec4;

    /// ACES D60 white.
    pub const D60_XYZ: Vec4 = [0.95264607456985, 1.0, 1.00882518435159, 0.0];
    /// D65 white.
    pub const D65_XYZ: Vec4 = [0.95045592705167, 1.0, 1.08905775075988, 0.0];
    /// DCI white.
    pub const DCI_XYZ: Vec4 = [0.89458689458689, 1.0, 0.95441595441595, 0.0];
}

/// Chromatic adaptation method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdaptationMethod {
    /// No adaptation.
    None,
    /// Bradford cone response.
    Bradford,
    /// CAT02 cone response.
    Cat02,
}

/// The 4x4 identity.
pub const IDENTITY44: Matrix44 = [
    1.0, 0.0, 0.0, 0.0, //
    0.0, 1.0, 0.0, 0.0, //
    0.0, 0.0, 1.0, 0.0, //
    0.0, 0.0, 0.0, 1.0,
];

/// Matrix product `a * b` (port of `MatrixArray::inner(MatrixArray)`).
pub fn m44_inner(a: &Matrix44, b: &Matrix44) -> Matrix44 {
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

/// Matrix-vector product `m * v` (port of `MatrixArray::inner(Offsets)`).
pub fn m44_inner_vec(m: &Matrix44, v: &Vec4) -> Vec4 {
    let mut out = [0.0; 4];
    for (i, o) in out.iter_mut().enumerate() {
        let mut accum = 0.0;
        for j in 0..4 {
            accum += m[i * 4 + j] * v[j];
        }
        *o = accum;
    }
    out
}

/// Inverse of a 4x4 matrix using the Gauss-Jordan elimination of
/// `MatrixArray::inverse` (itself copied from Imath's `gjInverse`).
pub fn m44_inverse(m: &Matrix44) -> Result<Matrix44> {
    let singular = || Error::msg("Singular Matrix can't be inverted.");

    let mut t = *m;
    let mut s = IDENTITY44;

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

/// True if any component is non-zero (port of `Offsets::isNotNull`).
fn is_not_null(v: &Vec4) -> bool {
    v.iter().any(|&x| x != 0.0)
}

/// Matrix converting RGB tristimulus values with the given primaries to CIE
/// XYZ, scaled so that RGB `[1, 1, 1]` maps to the white point with `Y = 1`.
///
/// Apply as `X = m[0] * R + m[1] * G + m[2] * B`, etc.
pub fn rgb2xyz_from_xy(primaries: &Primaries) -> Result<Matrix44> {
    let mut matrix = IDENTITY44;

    matrix[0] = primaries.red.xy[0];
    matrix[4] = primaries.red.xy[1];
    matrix[8] = 1.0 - primaries.red.xy[0] - primaries.red.xy[1];

    matrix[1] = primaries.grn.xy[0];
    matrix[5] = primaries.grn.xy[1];
    matrix[9] = 1.0 - primaries.grn.xy[0] - primaries.grn.xy[1];

    matrix[2] = primaries.blu.xy[0];
    matrix[6] = primaries.blu.xy[1];
    matrix[10] = 1.0 - primaries.blu.xy[0] - primaries.blu.xy[1];

    // 'matrix' is always well-conditioned, forming inverse is okay.
    let inv_matrix = m44_inverse(&matrix)?;

    let wht_xyz = [
        primaries.wht.xy[0] / primaries.wht.xy[1],
        1.0, // Set scaling of XYZ values to [0, 1].
        (1.0 - primaries.wht.xy[0] - primaries.wht.xy[1]) / primaries.wht.xy[1],
    ];

    let mut rgb2xyz = IDENTITY44;
    for i in 0..3 {
        let gain = wht_xyz[0] * inv_matrix[i * 4]
            + wht_xyz[1] * inv_matrix[i * 4 + 1]
            + wht_xyz[2] * inv_matrix[i * 4 + 2];

        for j in 0..3 {
            rgb2xyz[j * 4 + i] = gain * matrix[j * 4 + i];
        }
    }

    Ok(rgb2xyz)
}

const CONE_RESP_MAT_BRADFORD: Matrix44 = [
    0.8951, 0.2664, -0.1614, 0.0, //
    -0.7502, 1.7135, 0.0367, 0.0, //
    0.0389, -0.0685, 1.0296, 0.0, //
    0.0, 0.0, 0.0, 1.0,
];

const CONE_RESP_MAT_CAT02: Matrix44 = [
    0.7328, 0.4296, -0.1624, 0.0, //
    -0.7036, 1.6975, 0.0061, 0.0, //
    0.0030, 0.0136, 0.9834, 0.0, //
    0.0, 0.0, 0.0, 1.0,
];

/// Von Kries type chromatic adaptation matrix from the source white point
/// `src_xyz` to the destination white point `dst_xyz`, using the cone
/// primary matrix of `method` (Bradford unless `Cat02`).
pub fn build_vonkries_adapt(
    src_xyz: &Vec4,
    dst_xyz: &Vec4,
    method: AdaptationMethod,
) -> Result<Matrix44> {
    let xyz2rgb = if method == AdaptationMethod::Cat02 {
        CONE_RESP_MAT_CAT02
    } else {
        CONE_RESP_MAT_BRADFORD
    };

    let rgb2xyz = m44_inverse(&xyz2rgb)?;

    // Convert white point XYZ values to cone primary RGBs.
    let src_rgb = m44_inner_vec(&xyz2rgb, src_xyz);
    let dst_rgb = m44_inner_vec(&xyz2rgb, dst_xyz);

    // Make a diagonal matrix with the scale factors.
    let mut scale_mat = IDENTITY44;
    scale_mat[0] = dst_rgb[0] / src_rgb[0];
    scale_mat[5] = dst_rgb[1] / src_rgb[1];
    scale_mat[10] = dst_rgb[2] / src_rgb[2];
    scale_mat[15] = 1.0;

    // Compose into the adaptation matrix.
    Ok(m44_inner(&rgb2xyz, &m44_inner(&scale_mat, &xyz2rgb)))
}

/// Conversion matrix from source primaries to destination primaries, with
/// optional explicit adaptation white points (pass zeros to take the white
/// point from the primaries).
pub fn build_conversion_matrix_with_white(
    src_prims: &Primaries,
    dst_prims: &Primaries,
    src_wht_xyz: &Vec4,
    dst_wht_xyz: &Vec4,
    method: AdaptationMethod,
) -> Result<Matrix44> {
    const ONES: Vec4 = [1.0, 1.0, 1.0, 0.0];

    // Calculate the primary conversion matrices.
    let src_rgb2xyz = rgb2xyz_from_xy(src_prims)?;
    let dst_rgb2xyz = rgb2xyz_from_xy(dst_prims)?;
    let dst_xyz2rgb = m44_inverse(&dst_rgb2xyz)?;

    // Return the composed matrix if no white point adaptation is needed.
    if !is_not_null(src_wht_xyz)
        && !is_not_null(dst_wht_xyz)
        && src_prims.wht.xy[0] == dst_prims.wht.xy[0]
        && src_prims.wht.xy[1] == dst_prims.wht.xy[1]
    {
        // If the white points are equal, don't need to adapt.
        return Ok(m44_inner(&dst_xyz2rgb, &src_rgb2xyz));
    }
    if method == AdaptationMethod::None {
        return Ok(m44_inner(&dst_xyz2rgb, &src_rgb2xyz));
    }

    // Calculate src and dst white XYZ.
    let dst_wht = if is_not_null(dst_wht_xyz) {
        *dst_wht_xyz
    } else {
        m44_inner_vec(&dst_rgb2xyz, &ONES)
    };
    let src_wht = if is_not_null(src_wht_xyz) {
        *src_wht_xyz
    } else {
        m44_inner_vec(&src_rgb2xyz, &ONES)
    };

    // Build the adaptation matrix (may be an identity).
    let vkmat = build_vonkries_adapt(&src_wht, &dst_wht, method)?;

    // Compose the adaptation into the conversion matrix.
    Ok(m44_inner(&dst_xyz2rgb, &m44_inner(&vkmat, &src_rgb2xyz)))
}

/// Conversion matrix from source primaries to destination primaries. The
/// resulting matrix maps `[1, 1, 1]` input RGB to `[1, 1, 1]` output RGB.
pub fn build_conversion_matrix(
    src_prims: &Primaries,
    dst_prims: &Primaries,
    method: AdaptationMethod,
) -> Result<Matrix44> {
    const ZERO: Vec4 = [0.0; 4];
    build_conversion_matrix_with_white(src_prims, dst_prims, &ZERO, &ZERO, method)
}

/// Conversion matrix from the source primaries to CIE XYZ with a D65 white.
pub fn build_conversion_matrix_to_xyz_d65(
    src_prims: &Primaries,
    method: AdaptationMethod,
) -> Result<Matrix44> {
    const ZERO: Vec4 = [0.0; 4];
    build_conversion_matrix_with_white(
        src_prims,
        &CIE_XYZ_ILLUM_E,
        &ZERO,
        &whitepoint::D65_XYZ,
        method,
    )
}

/// Conversion matrix from CIE XYZ with a D65 white to the destination
/// primaries.
pub fn build_conversion_matrix_from_xyz_d65(
    dst_prims: &Primaries,
    method: AdaptationMethod,
) -> Result<Matrix44> {
    const ZERO: Vec4 = [0.0; 4];
    build_conversion_matrix_with_white(
        &CIE_XYZ_ILLUM_E,
        dst_prims,
        &whitepoint::D65_XYZ,
        &ZERO,
        method,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math_utils::equal_with_safe_rel_error;

    fn check(m: &Matrix44, expected: &[(usize, f64, f64)]) {
        for &(idx, aim, tol) in expected {
            assert!(
                equal_with_safe_rel_error(m[idx], aim, tol, 1.0),
                "index {idx}: {} expected {aim}",
                m[idx]
            );
        }
        for idx in [3, 7, 11, 12, 13, 14] {
            assert_eq!(m[idx], 0.0);
        }
        assert_eq!(m[15], 1.0);
    }

    #[test]
    fn color_matrix_helpers() {
        let m = rgb2xyz_from_xy(&ACES_AP1).unwrap();
        check(
            &m,
            &[
                (0, 0.66245418, 1e-7),
                (1, 0.13400421, 1e-7),
                (2, 0.15618769, 1e-7),
                (4, 0.27222872, 1e-7),
                (5, 0.67408177, 1e-7),
                (6, 0.05368952, 1e-7),
                (8, -0.00557465, 1e-7),
                (9, 0.00406073, 1e-7),
                (10, 1.0103391, 1e-6),
            ],
        );

        // D65 to D60.
        let src_xyz = [0.9504559270516716, 1.0, 1.0890577507598784, 0.0];
        let dst_xyz = [0.9526460745698463, 1.0, 1.0088251843515859, 0.0];
        let m = build_vonkries_adapt(&src_xyz, &dst_xyz, AdaptationMethod::Bradford).unwrap();
        check(
            &m,
            &[
                (0, 1.01303491, 1e-7),
                (1, 0.00610526, 1e-7),
                (2, -0.01497094, 1e-7),
                (4, 0.00769823, 1e-7),
                (5, 0.99816335, 1e-7),
                (6, -0.00503204, 1e-7),
                (8, -0.00284132, 1e-7),
                (9, 0.00468516, 1e-7),
                (10, 0.92450614, 1e-7),
            ],
        );

        // Source and dest white points are equal.
        let m = build_conversion_matrix(&P3_D65, &REC709, AdaptationMethod::Bradford).unwrap();
        check(
            &m,
            &[
                (0, 1.22494018, 1e-7),
                (1, -0.22494018, 1e-7),
                (2, 0.0, 1e-7),
                (4, -0.04205695, 1e-7),
                (5, 1.04205695, 1e-7),
                (6, 0.0, 1e-7),
                (8, -0.01963755, 1e-7),
                (9, -0.07863605, 1e-7),
                (10, 1.09827360, 1e-7),
            ],
        );

        // Source and dest white points differ.
        let m = build_conversion_matrix(&ACES_AP1, &REC709, AdaptationMethod::Bradford).unwrap();
        check(
            &m,
            &[
                (0, 1.70505099, 1e-7),
                (1, -0.62179212, 1e-7),
                (2, -0.08325887, 1e-7),
                (4, -0.13025642, 1e-7),
                (5, 1.14080474, 1e-7),
                (6, -0.01054832, 1e-7),
                (8, -0.02400336, 1e-7),
                (9, -0.12896898, 1e-7),
                (10, 1.15297233, 1e-7),
            ],
        );

        // Source and dest white points differ, manual override specified.
        let null = [0.0; 4];
        let d65_wht_xyz = [0.95045592705167, 1.0, 1.08905775075988, 0.0];
        let m = build_conversion_matrix_with_white(
            &ACES_AP0,
            &CIE_XYZ_ILLUM_E,
            &null,
            &d65_wht_xyz,
            AdaptationMethod::Bradford,
        )
        .unwrap();
        check(
            &m,
            &[
                (0, 0.93827985, 1e-7),
                (1, -0.00445145, 1e-7),
                (2, 0.01662752, 1e-7),
                (4, 0.33736889, 1e-7),
                (5, 0.72952157, 1e-7),
                (6, -0.06689046, 1e-7),
                (8, 0.00117395, 1e-7),
                (9, -0.00371071, 1e-7),
                (10, 1.09159451, 1e-7),
            ],
        );
    }

    #[test]
    fn inverse() {
        let m = rgb2xyz_from_xy(&REC709).unwrap();
        let inv = m44_inverse(&m).unwrap();
        let id = m44_inner(&m, &inv);
        for i in 0..16 {
            assert!((id[i] - IDENTITY44[i]).abs() < 1e-12);
        }
        assert_eq!(
            m44_inverse(&[0.0; 16]).unwrap_err().message(),
            "Singular Matrix can't be inverted."
        );
    }
}
