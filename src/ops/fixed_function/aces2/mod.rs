//! ACES 2 output transform building blocks (port of `ACES2/Transform.cpp`).
//!
//! All the computations are done in single precision, as in OCIO.

pub mod common;
pub mod matrix;

use crate::error::Result;
use common::*;
use matrix::*;

/// `(b - a) * z + a` (OCIO's `lerpf`).
#[inline]
fn lerpf(a: f32, b: f32, z: f32) -> f32 {
    (b - a) * z + a
}

// ---------------------------------------------------------------------------
// Table lookups

#[inline]
fn lerp3(lower: &[f32; 3], upper: &[f32; 3], t: f32) -> F3 {
    [
        lerpf(lower[0], upper[0], t),
        lerpf(lower[1], upper[1], t),
        lerpf(lower[2], upper[2], t),
    ]
}

#[inline]
fn midpoint_u(a: usize, b: usize) -> usize {
    (a + b) / 2
}

#[inline]
fn midpoint(a: f32, b: f32) -> f32 {
    (a + b) / 2.0
}

/// Search the hue table for the interval containing `h`; returns the upper
/// index of the interval.
fn lookup_hue_interval(h: f32, hues: &Table1D, hue_linearity_search_range: &[i32; 2]) -> usize {
    // We can narrow the search range based on the hues being almost uniform.
    let mut i = table::nominal_hue_position_in_uniform_table(h);
    let mut i_lo = (table::LOWER_WRAP_INDEX as i64)
        .max(i as i64 + i64::from(hue_linearity_search_range[0])) as usize;
    let mut i_hi = (table::UPPER_WRAP_INDEX as i64)
        .min(i as i64 + i64::from(hue_linearity_search_range[1])) as usize;

    while i_lo + 1 < i_hi {
        if h > hues[i] {
            i_lo = i;
        } else {
            i_hi = i;
        }
        i = midpoint_u(i_lo, i_hi);
    }

    i_hi.max(1)
}

#[inline]
fn interpolation_weight(h: f32, h_lo: f32, h_hi: f32) -> f32 {
    (h - h_lo) / (h_hi - h_lo)
}

#[inline]
fn cusp_from_table(i_hi: usize, t: f32, gt: &Table3D) -> F3 {
    lerp3(&gt[i_hi - 1], &gt[i_hi], t)
}

fn reach_m_from_table(h: f32, rt: &Table1D) -> f32 {
    let base = table::hue_position_in_uniform_table(h);
    let t = h - base as f32; // NOTE assumes uniform 1 degree 360 spacing
    let i_lo = base + table::FIRST_NOMINAL_INDEX;
    let i_hi = i_lo + 1;
    lerpf(rt[i_lo], rt[i_hi], t)
}

// ---------------------------------------------------------------------------
// CAM

#[inline]
fn post_adaptation_cone_response_compression_fwd_abs(rc: f32) -> f32 {
    let f_l_y = rc.powf(0.42);
    f_l_y / (CAM_NL_OFFSET + f_l_y)
}

#[inline]
fn post_adaptation_cone_response_compression_inv_abs(ra: f32) -> f32 {
    let ra_lim = min_f(ra, 0.99);
    let f_l_y = (CAM_NL_OFFSET * ra_lim) / (1.0 - ra_lim);
    f_l_y.powf(1.0 / 0.42)
}

/// Sign-preserving forward cone response compression.
pub fn post_adaptation_cone_response_compression_fwd(v: f32) -> f32 {
    let ra = post_adaptation_cone_response_compression_fwd_abs(v.abs());
    // Note that copysign(1, 0) returns 1 but the CTL copysign(1., 0.) returns 0.
    ra.copysign(v)
}

/// Sign-preserving inverse cone response compression.
pub fn post_adaptation_cone_response_compression_inv(v: f32) -> f32 {
    let rc = post_adaptation_cone_response_compression_inv_abs(v.abs());
    rc.copysign(v)
}

#[inline]
fn achromatic_n_to_j(a: f32, cz: f32) -> f32 {
    J_SCALE * a.powf(cz)
}

#[inline]
fn j_to_achromatic_n(j: f32, inv_cz: f32) -> f32 {
    (j * (1.0 / J_SCALE)).powf(inv_cz)
}

// Optimization for achromatic values.

#[inline]
fn a_to_y(a: f32, p: &JMhParams) -> f32 {
    let ra = p.a_w_j * a;
    post_adaptation_cone_response_compression_inv_abs(ra) / p.f_l_n
}

#[inline]
fn j_to_y(abs_j: f32, p: &JMhParams) -> f32 {
    a_to_y(j_to_achromatic_n(abs_j, p.inv_cz), p)
}

#[inline]
fn y_to_j_abs(abs_y: f32, p: &JMhParams) -> f32 {
    let ra = post_adaptation_cone_response_compression_fwd_abs(abs_y * p.f_l_n);
    achromatic_n_to_j(ra * p.inv_a_w_j, p.cz)
}

/// Luminance to lightness J.
pub fn y_to_j(y: f32, p: &JMhParams) -> f32 {
    y_to_j_abs(y.abs(), p).copysign(y)
}

/// RGB to the opponent space Aab.
pub fn rgb_to_aab(rgb: &F3, p: &JMhParams) -> F3 {
    let rgb_m = mult_f3_f33(rgb, &p.matrix_rgb_to_cam16_c);
    let rgb_a = [
        post_adaptation_cone_response_compression_fwd(rgb_m[0]),
        post_adaptation_cone_response_compression_fwd(rgb_m[1]),
        post_adaptation_cone_response_compression_fwd(rgb_m[2]),
    ];
    mult_f3_f33(&rgb_a, &p.matrix_cone_response_to_aab)
}

/// Aab to JMh.
pub fn aab_to_jmh(aab: &F3, p: &JMhParams) -> F3 {
    if aab[0] <= 0.0 {
        return [0.0, 0.0, 0.0];
    }
    let j = achromatic_n_to_j(aab[0], p.cz);
    let m = (aab[1] * aab[1] + aab[2] * aab[2]).sqrt();
    let h_rad = aab[2].atan2(aab[1]);
    // Call to unwrapped hue version due to atan2 limits.
    let h = from_radians_unchecked(h_rad);
    [j, m, h]
}

/// RGB to JMh.
pub fn rgb_to_jmh(rgb: &F3, p: &JMhParams) -> F3 {
    aab_to_jmh(&rgb_to_aab(rgb, p), p)
}

/// JMh to Aab with precomputed hue cosine and sine.
pub fn jmh_to_aab_trig(jmh: &F3, cos_hr: f32, sin_hr: f32, p: &JMhParams) -> F3 {
    let j = jmh[0];
    let m = jmh[1];
    let a = j_to_achromatic_n(j, p.inv_cz);
    [a, m * cos_hr, m * sin_hr]
}

/// JMh to Aab.
pub fn jmh_to_aab(jmh: &F3, p: &JMhParams) -> F3 {
    let h_rad = to_radians(jmh[2]);
    jmh_to_aab_trig(jmh, h_rad.cos(), h_rad.sin(), p)
}

/// Aab to RGB.
pub fn aab_to_rgb(aab: &F3, p: &JMhParams) -> F3 {
    let rgb_a = mult_f3_f33(aab, &p.matrix_aab_to_cone_response);
    let rgb_m = [
        post_adaptation_cone_response_compression_inv(rgb_a[0]),
        post_adaptation_cone_response_compression_inv(rgb_a[1]),
        post_adaptation_cone_response_compression_inv(rgb_a[2]),
    ];
    mult_f3_f33(&rgb_m, &p.matrix_cam16_c_to_rgb)
}

/// JMh to RGB.
pub fn jmh_to_rgb(jmh: &F3, p: &JMhParams) -> F3 {
    aab_to_rgb(&jmh_to_aab(jmh, p), p)
}

// ---------------------------------------------------------------------------
// Tonescale / Chroma compress

/// Hue-dependent chroma normalization factor.
pub fn chroma_compress_norm(cos_hr1: f32, sin_hr1: f32, chroma_compress_scale: f32) -> f32 {
    let cos_hr2 = 2.0 * cos_hr1 * cos_hr1 - 1.0;
    let sin_hr2 = 2.0 * cos_hr1 * sin_hr1;
    let cos_hr3 = 4.0 * cos_hr1 * cos_hr1 * cos_hr1 - 3.0 * cos_hr1;
    let sin_hr3 = 3.0 * sin_hr1 - 4.0 * sin_hr1 * sin_hr1 * sin_hr1;

    let trig_angles_hr: [f32; 8] = [
        cos_hr1, cos_hr2, cos_hr3, 0.0, sin_hr1, sin_hr2, sin_hr3, 1.0,
    ];
    const WEIGHTS: [f32; 8] = [
        11.34072, 16.46899, 7.88380, 0.0, 14.66441, -6.37224, 9.19364, 77.12896,
    ];

    let m = WEIGHTS[0] * trig_angles_hr[0]
        + WEIGHTS[1] * trig_angles_hr[1]
        + WEIGHTS[2] * trig_angles_hr[2]
        + WEIGHTS[4] * trig_angles_hr[4]
        + WEIGHTS[5] * trig_angles_hr[5]
        + WEIGHTS[6] * trig_angles_hr[6]
        + WEIGHTS[7];

    m * chroma_compress_scale
}

#[inline]
fn toe_fwd(x: f32, limit: f32, k1_in: f32, k2_in: f32) -> f32 {
    if x > limit {
        return x;
    }
    let k2 = max_f(k2_in, 0.001);
    let k1 = (k1_in * k1_in + k2 * k2).sqrt();
    let k3 = (limit + k1) / (limit + k2);
    let minus_b = k3 * x - k1;
    let minus_ac = k2 * k3 * x; // a is 1.0
    0.5 * (minus_b + (minus_b * minus_b + 4.0 * minus_ac).sqrt())
}

#[inline]
fn toe_inv(x: f32, limit: f32, k1_in: f32, k2_in: f32) -> f32 {
    if x > limit {
        return x;
    }
    let k2 = max_f(k2_in, 0.001);
    let k1 = (k1_in * k1_in + k2 * k2).sqrt();
    let k3 = (limit + k1) / (limit + k2);
    (x * x + k1 * x) / (k3 * (x + k2))
}

/// `std::max(a, b)` semantics (returns `a` unless `a < b`).
#[inline]
fn max_f(a: f32, b: f32) -> f32 {
    if a < b {
        b
    } else {
        a
    }
}

/// `std::min(a, b)` semantics (returns `a` unless `b < a`).
#[inline]
fn min_f(a: f32, b: f32) -> f32 {
    if b < a {
        b
    } else {
        a
    }
}

#[inline]
fn aces_tonescale(y_in: f32, pt: &ToneScaleParams, inverse: bool) -> f32 {
    if inverse {
        let y_ts_norm = y_in / REFERENCE_LUMINANCE;
        let z = max_f(0.0, min_f(pt.inverse_limit, y_ts_norm));
        let f = (z + (z * (4.0 * pt.t_1 + z)).sqrt()) / 2.0;
        return pt.s_2 / ((pt.m_2 / f).powf(1.0 / pt.g) - 1.0);
    }

    let f = pt.m_2 * (y_in / (y_in + pt.s_2)).powf(pt.g);
    // max prevents -ve values being output also handles division by zero possibility.
    max_f(0.0, f * f / (f + pt.t_1)) * pt.n_r
}

#[inline]
fn tonescale(j: f32, p: &JMhParams, pt: &ToneScaleParams, inverse: bool) -> f32 {
    // Tonescale applied in Y (convert to and from J).
    let j_abs = j.abs();
    let y_in = j_to_y(j_abs, p);
    let y_out = aces_tonescale(y_in, pt, inverse);
    let j_out = y_to_j_abs(y_out, p);
    j_out.copysign(j)
}

/// Forward tonescale in J.
pub fn tonescale_fwd(j: f32, p: &JMhParams, pt: &ToneScaleParams) -> f32 {
    tonescale(j, p, pt, false)
}

/// Inverse tonescale in J.
pub fn tonescale_inv(j: f32, p: &JMhParams, pt: &ToneScaleParams) -> f32 {
    tonescale(j, p, pt, true)
}

/// Forward tonescale from the achromatic response A to J.
pub fn tonescale_a_to_j_fwd(a: f32, p: &JMhParams, pt: &ToneScaleParams) -> f32 {
    let y_in = a_to_y(a, p);
    let y_out = aces_tonescale(y_in, pt, false);
    let j_out = y_to_j_abs(y_out, p);
    j_out.copysign(a)
}

/// Forward chroma compression.
pub fn chroma_compress_fwd(
    jmh: &F3,
    j_ts: f32,
    m_norm: f32,
    pr: &ResolvedSharedCompressionParameters,
    pc: &ChromaCompressParams,
) -> F3 {
    let j = jmh[0];
    let m = jmh[1];
    let h = jmh[2];

    let mut m_cp = m;

    if m != 0.0 {
        let n_j = j_ts / pr.limit_j_max;
        let sn_j = max_f(0.0, 1.0 - n_j);
        let limit = n_j.powf(pr.model_gamma_inv) * pr.reach_max_m / m_norm;

        m_cp = m * (j_ts / j).powf(pr.model_gamma_inv);
        m_cp /= m_norm;
        m_cp = limit
            - toe_fwd(
                limit - m_cp,
                limit - 0.001,
                sn_j * pc.sat,
                (n_j * n_j + pc.sat_thr).sqrt(),
            );
        m_cp = toe_fwd(m_cp, limit, n_j * pc.compr, sn_j);
        m_cp *= m_norm;
    }

    [j_ts, m_cp, h]
}

/// Inverse chroma compression.
pub fn chroma_compress_inv(
    jmh: &F3,
    j: f32,
    m_norm: f32,
    pr: &ResolvedSharedCompressionParameters,
    pc: &ChromaCompressParams,
) -> F3 {
    let j_ts = jmh[0];
    let m_cp = jmh[1];
    let h = jmh[2];
    let mut m = m_cp;

    if m_cp != 0.0 {
        let n_j = j_ts / pr.limit_j_max;
        let sn_j = max_f(0.0, 1.0 - n_j);
        let limit = n_j.powf(pr.model_gamma_inv) * pr.reach_max_m / m_norm;

        m = m_cp / m_norm;
        m = toe_inv(m, limit, n_j * pc.compr, sn_j);
        m = limit
            - toe_inv(
                limit - m,
                limit - 0.001,
                sn_j * pc.sat,
                (n_j * n_j + pc.sat_thr).sqrt(),
            );
        m *= m_norm;
        m *= (j_ts / j).powf(-pr.model_gamma_inv);
    }

    [j, m, h]
}

/// c * z nonlinearity.
#[inline]
fn model_gamma() -> f32 {
    SURROUND[1] * (1.48 + (Y_B / REFERENCE_LUMINANCE).sqrt())
}

/// Precompute the CAM parameters for a set of primaries.
pub fn init_jmh_params(prims: &Primaries) -> Result<JMhParams> {
    let base_cone_response_to_aab: M33f = [
        2.0,
        1.0,
        1.0 / 20.0,
        1.0,
        -12.0 / 11.0,
        1.0 / 11.0,
        1.0 / 9.0,
        1.0 / 9.0,
        -2.0 / 9.0,
    ];

    let matrix_16 = xyz_to_rgb_f33(&CAM16_PRIMARIES)?;
    let rgb_to_xyz = rgb_to_xyz_f33(prims)?;
    let xyz_w = mult_f3_f33(&f3_from_f(REFERENCE_LUMINANCE), &rgb_to_xyz);

    let y_w = xyz_w[1];

    let rgb_w = mult_f3_f33(&xyz_w, &matrix_16);

    // Viewing condition dependent parameters.
    const K: f32 = 1.0 / (5.0 * L_A + 1.0);
    const K4: f32 = K * K * K * K;
    let f_l = 0.2 * K4 * (5.0 * L_A) + 0.1 * (1.0 - K4).powf(2.0) * (5.0 * L_A).powf(1.0 / 3.0);

    let f_l_n = f_l / REFERENCE_LUMINANCE;
    let cz = model_gamma();
    let inv_cz = 1.0 / cz;

    let d_rgb = [
        f_l_n * y_w / rgb_w[0],
        f_l_n * y_w / rgb_w[1],
        f_l_n * y_w / rgb_w[2],
    ];

    let rgb_wc = [
        d_rgb[0] * rgb_w[0],
        d_rgb[1] * rgb_w[1],
        d_rgb[2] * rgb_w[2],
    ];

    let rgb_aw = [
        post_adaptation_cone_response_compression_fwd(rgb_wc[0]),
        post_adaptation_cone_response_compression_fwd(rgb_wc[1]),
        post_adaptation_cone_response_compression_fwd(rgb_wc[2]),
    ];

    let cone_response_to_aab = mult_f33_f33(
        &scale_f33(&IDENTITY_M33, &f3_from_f(CAM_NL_SCALE)),
        &base_cone_response_to_aab,
    );
    let a_w = cone_response_to_aab[0] * rgb_aw[0]
        + cone_response_to_aab[1] * rgb_aw[1]
        + cone_response_to_aab[2] * rgb_aw[2];
    let a_w_j = post_adaptation_cone_response_compression_fwd_abs(f_l);
    let inv_a_w_j = 1.0 / a_w_j;

    // Note we are prescaling the CAM16 LMS responses to directly provide for
    // chromatic adaptation.
    let matrix_rgb_to_cam16 = mult_f33_f33(
        &rgb_to_rgb_f33(prims, &CAM16_PRIMARIES)?,
        &scale_f33(&IDENTITY_M33, &f3_from_f(REFERENCE_LUMINANCE)),
    );
    let matrix_rgb_to_cam16_c =
        mult_f33_f33(&scale_f33(&IDENTITY_M33, &d_rgb), &matrix_rgb_to_cam16);
    let matrix_cam16_c_to_rgb = invert_f33(&matrix_rgb_to_cam16_c)?;

    let c = &cone_response_to_aab;
    let matrix_cone_response_to_aab: M33f = [
        c[0] / a_w,
        c[1] / a_w,
        c[2] / a_w,
        c[3] * 43.0 * SURROUND[2],
        c[4] * 43.0 * SURROUND[2],
        c[5] * 43.0 * SURROUND[2],
        c[6] * 43.0 * SURROUND[2],
        c[7] * 43.0 * SURROUND[2],
        c[8] * 43.0 * SURROUND[2],
    ];
    let matrix_aab_to_cone_response = invert_f33(&matrix_cone_response_to_aab)?;

    Ok(JMhParams {
        matrix_rgb_to_cam16_c,
        matrix_cam16_c_to_rgb,
        matrix_cone_response_to_aab,
        matrix_aab_to_cone_response,
        f_l_n,
        cz,
        inv_cz,
        a_w_j,
        inv_a_w_j,
    })
}

#[inline]
fn generate_unit_cube_cusp_corners(corner: usize) -> F3 {
    // Generation order R, Y, G, C, B, M to ensure hues rotate in correct order.
    let b = |v: bool| if v { 1.0 } else { 0.0 };
    [
        b(((corner + 1) % CUSP_CORNER_COUNT) < 3),
        b(((corner + 5) % CUSP_CORNER_COUNT) < 3),
        b(((corner + 3) % CUSP_CORNER_COUNT) < 3),
    ]
}

type Corners = [F3; TOTAL_CORNER_COUNT];

fn build_limiting_cusp_corners_tables(
    rgb_corners: &mut Corners,
    jmh_corners: &mut Corners,
    params: &JMhParams,
    peak_luminance: f32,
) {
    // We calculate the RGB and JMh values for the limiting gamut cusp corners.
    // They are then arranged into a cycle with the lowest JMh value at [1] to
    // allow for hue wrapping.
    let mut temp_rgb_corners = [[0.0f32; 3]; CUSP_CORNER_COUNT];
    let mut temp_jmh_corners = [[0.0f32; 3]; CUSP_CORNER_COUNT];
    let mut min_index = 0;
    for i in 0..CUSP_CORNER_COUNT {
        temp_rgb_corners[i] = mult_f_f3(
            peak_luminance / REFERENCE_LUMINANCE,
            &generate_unit_cube_cusp_corners(i),
        );
        temp_jmh_corners[i] = rgb_to_jmh(&temp_rgb_corners[i], params);
        if temp_jmh_corners[i][2] < temp_jmh_corners[min_index][2] {
            min_index = i;
        }
    }

    // Rotate entries placing lowest at [1] (not [0]).
    for i in 0..CUSP_CORNER_COUNT {
        rgb_corners[i + 1] = temp_rgb_corners[(i + min_index) % CUSP_CORNER_COUNT];
        jmh_corners[i + 1] = temp_jmh_corners[(i + min_index) % CUSP_CORNER_COUNT];
    }

    // Copy end elements to create a cycle.
    rgb_corners[0] = rgb_corners[CUSP_CORNER_COUNT];
    rgb_corners[CUSP_CORNER_COUNT + 1] = rgb_corners[1];
    jmh_corners[0] = jmh_corners[CUSP_CORNER_COUNT];
    jmh_corners[CUSP_CORNER_COUNT + 1] = jmh_corners[1];

    // Wrap the hues, to maintain monotonicity these entries will fall outside
    // [0.0, hue_limit).
    jmh_corners[0][2] -= HUE_LIMIT;
    jmh_corners[CUSP_CORNER_COUNT + 1][2] += HUE_LIMIT;
}

fn find_reach_corners_table(
    jmh_corners: &mut Corners,
    params: &JMhParams,
    limit_j: f32,
    maximum_source: f32,
) {
    // We need to find the value of JMh that corresponds to limitJ for each
    // corner. This is done by scaling the unit corners converting to JMh until
    // the J value is near the limitJ. As an optimisation we use the equivalent
    // Achromatic value to search for the J value and avoid the non-linear
    // transform during the search.
    let mut temp_jmh_corners = [[0.0f32; 3]; CUSP_CORNER_COUNT];
    let limit_a = j_to_achromatic_n(limit_j, params.inv_cz);

    let mut min_index = 0;
    for i in 0..CUSP_CORNER_COUNT {
        let rgb_vector = generate_unit_cube_cusp_corners(i);

        let mut lower = 0.0f32;
        let mut upper = maximum_source;
        while (upper - lower) > REACH_CUSP_TOLERANCE {
            let test = midpoint(lower, upper);
            let test_corner = mult_f_f3(test, &rgb_vector);
            let a = rgb_to_aab(&test_corner, params)[0];
            if a < limit_a {
                lower = test;
            } else {
                upper = test;
            }
            if a == limit_a {
                break;
            }
        }
        temp_jmh_corners[i] = rgb_to_jmh(&mult_f_f3(upper, &rgb_vector), params);

        if temp_jmh_corners[i][2] < temp_jmh_corners[min_index][2] {
            min_index = i;
        }
    }

    // Rotate entries placing lowest at [1] (not [0]).
    for i in 0..CUSP_CORNER_COUNT {
        jmh_corners[i + 1] = temp_jmh_corners[(i + min_index) % CUSP_CORNER_COUNT];
    }

    // Copy end elements to create a cycle.
    jmh_corners[0] = jmh_corners[CUSP_CORNER_COUNT];
    jmh_corners[CUSP_CORNER_COUNT + 1] = jmh_corners[1];

    // Wrap the hues, to maintain monotonicity these entries will fall outside
    // [0.0, hue_limit).
    jmh_corners[0][2] -= HUE_LIMIT;
    jmh_corners[CUSP_CORNER_COUNT + 1][2] += HUE_LIMIT;
}

fn extract_sorted_cube_hues(
    sorted_hues: &mut [f32; MAX_SORTED_CORNERS],
    reach_jmh: &Corners,
    display_jmh: &Corners,
) -> usize {
    // Basic merge of 2 sorted arrays, extracting the unique hues.
    // Return the count of the unique hues.
    let mut idx = 0;
    let mut reach_idx = 1;
    let mut display_idx = 1;
    while (reach_idx < (CUSP_CORNER_COUNT + 1)) || (display_idx < (CUSP_CORNER_COUNT + 1)) {
        let reach_hue = reach_jmh[reach_idx][2];
        let display_hue = display_jmh[display_idx][2];
        let value = if reach_hue == display_hue {
            reach_idx += 1;
            display_idx += 1; // When equal consume both.
            reach_hue
        } else if reach_hue < display_hue {
            reach_idx += 1;
            reach_hue
        } else {
            display_idx += 1;
            display_hue
        };
        if idx < MAX_SORTED_CORNERS {
            sorted_hues[idx] = value;
        }
        idx += 1;
        if reach_idx >= TOTAL_CORNER_COUNT || display_idx >= TOTAL_CORNER_COUNT {
            // Only reachable with invalid (e.g. NaN) hues.
            break;
        }
    }
    idx.min(MAX_SORTED_CORNERS)
}

fn build_hue_sample_interval(
    samples: usize,
    lower: f32,
    upper: f32,
    hue_table: &mut Table1D,
    base: usize,
) {
    let delta = (upper - lower) / samples as f32;
    for i in 0..samples {
        if let Some(slot) = hue_table.get_mut(base + i) {
            *slot = lower + i as f32 * delta;
        }
    }
}

fn build_hue_table(
    hue_table: &mut Table1D,
    sorted_hues: &[f32; MAX_SORTED_CORNERS],
    unique_hues: usize,
) {
    let ideal_spacing = table::NOMINAL_SIZE as f32 / HUE_LIMIT;
    let mut samples_count = [0u32; 2 * CUSP_CORNER_COUNT + 2];
    let nominal_size = table::NOMINAL_SIZE as u32;
    let mut last_idx = u32::MAX;
    let mut min_index: u32 = if sorted_hues[0] == 0.0 { 0 } else { 1 }; // Ensure we can always sample at 0.0 hue.
    for hue_idx in 0..unique_hues {
        // BUG: "hue_table.size - 1" will fail if we have multiple hues mapping
        // near the top of the table.
        let mut nominal_idx = ((sorted_hues[hue_idx] * ideal_spacing).round() as u32)
            .max(min_index)
            .min(nominal_size - 1);
        if last_idx == nominal_idx {
            // Last two hues should sample at same index, need to adjust them.
            // Adjust previous sample down if we can.
            if hue_idx > 1
                && samples_count[hue_idx - 2] != samples_count[hue_idx - 1].wrapping_sub(1)
            {
                samples_count[hue_idx - 1] = samples_count[hue_idx - 1].wrapping_sub(1);
            } else {
                nominal_idx += 1;
            }
        }
        samples_count[hue_idx] = nominal_idx.min(nominal_size - 1);
        last_idx = nominal_idx;
        min_index = nominal_idx;
    }

    let mut total_samples: usize = 0;
    // Special cases for ends.
    let mut i = 0;
    build_hue_sample_interval(
        samples_count[i] as usize,
        0.0,
        sorted_hues[i],
        hue_table,
        total_samples + 1,
    );
    total_samples += samples_count[i] as usize;
    i += 1;
    while i < unique_hues {
        let samples = samples_count[i].wrapping_sub(samples_count[i - 1]) as usize;
        let samples = samples.min(table::TOTAL_SIZE);
        build_hue_sample_interval(
            samples,
            sorted_hues[i - 1],
            sorted_hues[i],
            hue_table,
            total_samples + 1,
        );
        total_samples += samples;
        i += 1;
    }
    // BUG: could break if we are unlucky with samples all being used up by
    // this point.
    let last = sorted_hues[i.max(1) - 1];
    build_hue_sample_interval(
        table::NOMINAL_SIZE.saturating_sub(total_samples),
        last,
        HUE_LIMIT,
        hue_table,
        total_samples + 1,
    );

    hue_table[table::LOWER_WRAP_INDEX] = hue_table[table::LAST_NOMINAL_INDEX] - HUE_LIMIT;
    hue_table[table::UPPER_WRAP_INDEX] = hue_table[table::FIRST_NOMINAL_INDEX] + HUE_LIMIT;
    hue_table[table::UPPER_WRAP_INDEX + 1] = hue_table[table::FIRST_NOMINAL_INDEX + 1] + HUE_LIMIT;
}

fn find_display_cusp_for_hue(
    hue: f32,
    rgb_corners: &Corners,
    jmh_corners: &Corners,
    params: &JMhParams,
    previous: &mut [f32; 2],
) -> F2 {
    // This works by finding the required line segment between two of the XYZ
    // cusp corners, then binary searching along the line calculating the JMh
    // of points along the line till we find the required value. All values on
    // the line segments are valid cusp locations.
    let mut upper_corner = 1;
    for (i, corner) in jmh_corners.iter().enumerate().skip(upper_corner) {
        if corner[2] > hue {
            upper_corner = i;
            break;
        }
    }
    let lower_corner = upper_corner - 1;

    // Hue should now be within [lower_corner, upper_corner), handle exact match.
    if jmh_corners[lower_corner][2] == hue {
        return [jmh_corners[lower_corner][0], jmh_corners[lower_corner][1]];
    }

    // Search by lerping between RGB corners for the hue.
    let cusp_lower = rgb_corners[lower_corner];
    let cusp_upper = rgb_corners[upper_corner];

    // If we are still on the same segment start from where we left off.
    let mut lower_t = if upper_corner as f32 == previous[0] {
        previous[1]
    } else {
        0.0
    };
    let mut upper_t = 1.0f32;

    // There is an edge case where we need to search towards the range when
    // across the [0.0f, hue_limit) boundary each edge needs the directions
    // swapped. This is handled by comparing against the appropriate corner to
    // make sure we are still in the expected range between the lower and upper
    // corner hue limits.
    while (upper_t - lower_t) > DISPLAY_CUSP_TOLERANCE {
        let sample_t = midpoint(lower_t, upper_t);
        let sample = lerp3(&cusp_lower, &cusp_upper, sample_t);
        let jmh = rgb_to_jmh(&sample, params);
        if jmh[2] < jmh_corners[lower_corner][2] {
            upper_t = sample_t;
        } else if jmh[2] >= jmh_corners[upper_corner][2] {
            lower_t = sample_t;
        } else if jmh[2] > hue {
            upper_t = sample_t;
        } else {
            lower_t = sample_t;
        }
    }

    // Use the midpoint of the final interval for the actual samples.
    let sample_t = midpoint(lower_t, upper_t);
    let sample = lerp3(&cusp_lower, &cusp_upper, sample_t);
    let jmh = rgb_to_jmh(&sample, params);

    previous[0] = upper_corner as f32;
    previous[1] = sample_t;

    [jmh[0], jmh[1]]
}

fn build_cusp_table(
    hue_table: &Table1D,
    rgb_corners: &Corners,
    jmh_corners: &Corners,
    params: &JMhParams,
) -> Box<Table3D> {
    let mut previous = [0.0f32; 2];
    let mut output_table: Box<Table3D> = Box::new([[0.0; 3]; table::TOTAL_SIZE]);
    for i in table::FIRST_NOMINAL_INDEX..table::UPPER_WRAP_INDEX {
        let hue = hue_table[i];
        let jm = find_display_cusp_for_hue(hue, rgb_corners, jmh_corners, params, &mut previous);
        output_table[i][0] = jm[0];
        output_table[i][1] = jm[1] * (1.0 + SMOOTH_M * SMOOTH_CUSPS);
        output_table[i][2] = hue;
    }

    // Copy extra entries to ease the code to handle hues wrapping around.
    let lw = table::LOWER_WRAP_INDEX;
    let uw = table::UPPER_WRAP_INDEX;
    let first = table::FIRST_NOMINAL_INDEX;
    let last = table::LAST_NOMINAL_INDEX;
    output_table[lw] = [output_table[last][0], output_table[last][1], hue_table[lw]];
    output_table[uw] = [
        output_table[first][0],
        output_table[first][1],
        hue_table[uw],
    ];
    output_table[uw + 1] = [
        output_table[first + 1][0],
        output_table[first + 1][1],
        hue_table[uw + 1],
    ];
    output_table
}

fn make_uniform_hue_gamut_table(
    reach_params: &JMhParams,
    params: &JMhParams,
    peak_luminance: f32,
    forward_limit: f32,
    sp: &SharedCompressionParameters,
    hue_table: &mut Table1D,
) -> Box<Table3D> {
    // The principal here is to sample the hues as uniformly as possible, whilst
    // ensuring we sample the corners of the limiting gamut and the reach
    // primaries at limit J Max.
    //
    // The corners are calculated then the hues are extracted and merged to
    // form a unique sorted hue list. We then build the hue table from the
    // list, those hues are then used to compute the JMh of the limiting gamut
    // cusp.
    let mut reach_jmh_corners: Corners = [[0.0; 3]; TOTAL_CORNER_COUNT];
    let mut limiting_rgb_corners: Corners = [[0.0; 3]; TOTAL_CORNER_COUNT];
    let mut limiting_jmh_corners: Corners = [[0.0; 3]; TOTAL_CORNER_COUNT];
    let mut sorted_hues = [0.0f32; MAX_SORTED_CORNERS];

    find_reach_corners_table(
        &mut reach_jmh_corners,
        reach_params,
        sp.limit_j_max,
        forward_limit,
    );
    build_limiting_cusp_corners_tables(
        &mut limiting_rgb_corners,
        &mut limiting_jmh_corners,
        params,
        peak_luminance,
    );
    let unique_hues =
        extract_sorted_cube_hues(&mut sorted_hues, &reach_jmh_corners, &limiting_jmh_corners);
    build_hue_table(hue_table, &sorted_hues, unique_hues);
    build_cusp_table(
        hue_table,
        &limiting_rgb_corners,
        &limiting_jmh_corners,
        params,
    )
}

#[inline]
fn any_below_zero(rgb: &F3) -> bool {
    rgb[0] < 0.0 || rgb[1] < 0.0 || rgb[2] < 0.0
}

fn make_reach_m_table(params: &JMhParams, limit_j_max: f32) -> Box<Table1D> {
    let mut gamut_reach_table: Box<Table1D> = Box::new([0.0; table::TOTAL_SIZE]);

    for i in 0..table::NOMINAL_SIZE {
        let hue = table::base_hue_for_position(i);

        const SEARCH_RANGE: f32 = 50.0;
        const SEARCH_MAXIMUM: f32 = 1300.0;
        let mut low = 0.0f32;
        let mut high = low + SEARCH_RANGE;
        let mut outside = false;

        while !outside && (high < SEARCH_MAXIMUM) {
            let search_jmh = [limit_j_max, high, hue];
            let new_limit_rgb = jmh_to_rgb(&search_jmh, params);
            outside = any_below_zero(&new_limit_rgb);
            if !outside {
                low = high;
                high += SEARCH_RANGE;
            }
        }

        while high - low > 1e-2 {
            let sample_m = (high + low) / 2.0;
            let search_jmh = [limit_j_max, sample_m, hue];
            let new_limit_rgb = jmh_to_rgb(&search_jmh, params);
            outside = any_below_zero(&new_limit_rgb);
            if outside {
                high = sample_m;
            } else {
                low = sample_m;
            }
        }

        gamut_reach_table[i + table::BASE_INDEX] = high;
    }
    gamut_reach_table[table::LOWER_WRAP_INDEX] = gamut_reach_table[table::LAST_NOMINAL_INDEX];
    gamut_reach_table[table::UPPER_WRAP_INDEX] = gamut_reach_table[table::FIRST_NOMINAL_INDEX];
    gamut_reach_table[table::UPPER_WRAP_INDEX + 1] =
        gamut_reach_table[table::FIRST_NOMINAL_INDEX + 1];

    gamut_reach_table
}

#[inline]
fn outside_hull(rgb: &F3, max_rgb_test_val: f32) -> bool {
    // Limit value, once we cross this value, we are outside of the top gamut
    // shell.
    rgb[0] > max_rgb_test_val || rgb[1] > max_rgb_test_val || rgb[2] > max_rgb_test_val
}

#[inline]
fn get_focus_gain(j: f32, analytical_threshold: f32, limit_j_max: f32, focus_dist: f32) -> f32 {
    let mut gain = limit_j_max * focus_dist;
    if j > analytical_threshold {
        // Approximate inverse required above threshold due to the introduction
        // of J in the calculation.
        let mut gain_adjustment =
            ((limit_j_max - analytical_threshold) / max_f(0.0001, limit_j_max - j)).log10();
        gain_adjustment = gain_adjustment * gain_adjustment + 1.0;
        gain *= gain_adjustment;
    }
    gain
}

fn solve_j_intersect(j: f32, m: f32, focus_j: f32, max_j: f32, slope_gain: f32) -> f32 {
    let m_scaled = m / slope_gain;
    let a = m_scaled / focus_j;

    if j < focus_j {
        let b = 1.0 - m_scaled;
        let c = -j;
        let det = b * b - 4.0 * a * c;
        let root = det.sqrt();
        -2.0 * c / (b + root)
    } else {
        let b = -(1.0 + m_scaled + max_j * a);
        let c = max_j * m_scaled + j;
        let det = b * b - 4.0 * a * c;
        let root = det.sqrt();
        -2.0 * c / (b - root)
    }
}

/// Smooth minimum about the scaled reference, based upon a cubic polynomial.
#[inline]
fn smin_scaled(a: f32, b: f32, scale_reference: f32) -> f32 {
    let s_scaled = SMOOTH_CUSPS * scale_reference;
    let h = max_f(s_scaled - (a - b).abs(), 0.0) / s_scaled;
    min_f(a, b) - h * h * h * s_scaled * (1.0 / 6.0)
}

#[inline]
fn compute_compression_vector_slope(
    intersect_j: f32,
    focus_j: f32,
    limit_j_max: f32,
    slope_gain: f32,
) -> f32 {
    let direction_scaler = if intersect_j < focus_j {
        intersect_j
    } else {
        limit_j_max - intersect_j
    };
    direction_scaler * (intersect_j - focus_j) / (focus_j * slope_gain)
}

#[inline]
fn estimate_line_and_boundary_intersection_m(
    j_axis_intersect: f32,
    slope: f32,
    inv_gamma: f32,
    j_max: f32,
    m_max: f32,
    j_intersection_reference: f32,
) -> f32 {
    // Line defined by     J = slope * x + J_axis_intersect
    // Boundary defined by J = J_max * (x / M_max) ^ (1/inv_gamma)
    // Approximate as we do not want to iteratively solve intersection of a
    // straight line and an exponential.

    // We calculate a shifted intersection from the original intersection
    // using the inverse of the exponential and the provided reference.
    let normalised_j = j_axis_intersect / j_intersection_reference;
    let shifted_intersection = j_intersection_reference * normalised_j.powf(inv_gamma);

    // Now we find the M intersection of two lines:
    // line from origin to J,M Max       l1(x) = J/M * x
    // line from J Intersect' with slope l2(x) = slope * x + Intersect'
    shifted_intersection * m_max / (j_max - slope * m_max)
}

fn find_gamut_boundary_intersection(
    jm_cusp: &F2,
    j_max: f32,
    gamma_top_inv: f32,
    gamma_bottom_inv: f32,
    j_intersect_source: f32,
    slope: f32,
    j_intersect_cusp: f32,
) -> f32 {
    let m_boundary_lower = estimate_line_and_boundary_intersection_m(
        j_intersect_source,
        slope,
        gamma_bottom_inv,
        jm_cusp[0],
        jm_cusp[1],
        j_intersect_cusp,
    );

    // The upper hull is flipped and thus 'zeroed' at J_max.
    // Also note we negate the slope.
    let f_j_intersect_cusp = j_max - j_intersect_cusp;
    let f_j_intersect_source = j_max - j_intersect_source;
    let f_jm_cusp_j = j_max - jm_cusp[0];
    let m_boundary_upper = estimate_line_and_boundary_intersection_m(
        f_j_intersect_source,
        -slope,
        gamma_top_inv,
        f_jm_cusp_j,
        jm_cusp[1],
        f_j_intersect_cusp,
    );

    // Smooth minimum between the two calculated values for the M component.
    smin_scaled(m_boundary_lower, m_boundary_upper, jm_cusp[1])
}

#[inline]
fn reinhard_remap(scale: f32, nd: f32, invert: bool) -> f32 {
    if invert {
        if nd >= 1.0 {
            scale
        } else {
            scale * -(nd / (nd - 1.0))
        }
    } else {
        scale * nd / (1.0 + nd)
    }
}

#[inline]
fn remap_m(m: f32, gamut_boundary_m: f32, reach_boundary_m: f32, invert: bool) -> f32 {
    let boundary_ratio = gamut_boundary_m / reach_boundary_m;
    let proportion = max_f(boundary_ratio, COMPRESSION_THRESHOLD);
    let threshold = proportion * gamut_boundary_m;

    if m <= threshold || proportion >= 1.0 {
        return m;
    }

    // Translate to place threshold at zero.
    let m_offset = m - threshold;
    let gamut_offset = gamut_boundary_m - threshold;
    let reach_offset = reach_boundary_m - threshold;

    let scale = reach_offset / ((reach_offset / gamut_offset) - 1.0);
    let nd = m_offset / scale;

    // Shift back to absolute.
    threshold + reinhard_remap(scale, nd, invert)
}

fn compress_gamut(
    jmh: &F3,
    jx: f32,
    sr: &ResolvedSharedCompressionParameters,
    p: &GamutCompressParams,
    hdp: &HueDependantGamutParams,
    invert: bool,
) -> F3 {
    let j = jmh[0];
    let m = jmh[1];
    let h = jmh[2];

    let slope_gain = get_focus_gain(jx, hdp.analytical_threshold, sr.limit_j_max, p.focus_dist);
    let j_intersect_source = solve_j_intersect(j, m, hdp.focus_j, sr.limit_j_max, slope_gain);
    let gamut_slope = compute_compression_vector_slope(
        j_intersect_source,
        hdp.focus_j,
        sr.limit_j_max,
        slope_gain,
    );

    let j_intersect_cusp = solve_j_intersect(
        hdp.jm_cusp[0],
        hdp.jm_cusp[1],
        hdp.focus_j,
        sr.limit_j_max,
        slope_gain,
    );
    let gamut_boundary_m = find_gamut_boundary_intersection(
        &hdp.jm_cusp,
        sr.limit_j_max,
        hdp.gamma_top_inv,
        hdp.gamma_bottom_inv,
        j_intersect_source,
        gamut_slope,
        j_intersect_cusp,
    );

    if gamut_boundary_m <= 0.0 {
        return [j, 0.0, h];
    }

    let reach_boundary_m = estimate_line_and_boundary_intersection_m(
        j_intersect_source,
        gamut_slope,
        sr.model_gamma_inv,
        sr.limit_j_max,
        sr.reach_max_m,
        sr.limit_j_max,
    );

    let remapped_m = remap_m(m, gamut_boundary_m, reach_boundary_m, invert);

    [j_intersect_source + remapped_m * gamut_slope, remapped_m, h]
}

#[inline]
fn compute_focus_j(cusp_j: f32, mid_j: f32, limit_j_max: f32) -> f32 {
    lerpf(
        cusp_j,
        mid_j,
        min_f(1.0, CUSP_MID_BLEND - (cusp_j / limit_j_max)),
    )
}

fn init_hue_dependant_gamut_params(
    hue: f32,
    sr: &ResolvedSharedCompressionParameters,
    p: &GamutCompressParams,
) -> HueDependantGamutParams {
    let i_hi = lookup_hue_interval(hue, &p.hue_table, &p.hue_linearity_search_range);
    let t = interpolation_weight(hue, p.hue_table[i_hi - 1], p.hue_table[i_hi]);
    let cusp = cusp_from_table(i_hi, t, &p.gamut_cusp_table);

    let jm_cusp = [cusp[0], cusp[1]];
    HueDependantGamutParams {
        gamma_bottom_inv: p.lower_hull_gamma_inv,
        jm_cusp,
        gamma_top_inv: cusp[2],
        focus_j: compute_focus_j(jm_cusp[0], p.mid_j, sr.limit_j_max),
        analytical_threshold: lerpf(jm_cusp[0], sr.limit_j_max, FOCUS_GAIN_BLEND),
    }
}

/// Forward gamut compression.
pub fn gamut_compress_fwd(
    jmh: &F3,
    sr: &ResolvedSharedCompressionParameters,
    p: &GamutCompressParams,
) -> F3 {
    let j = jmh[0];
    let m = jmh[1];
    let h = jmh[2];

    if j <= 0.0 {
        // Limit to +ve J values.
        return [0.0, 0.0, h];
    }
    if m <= 0.0 || j > sr.limit_j_max {
        // We compress M only so avoid mapping zero. Above the expected maximum
        // we explicitly map to 0 M.
        return [j, 0.0, h];
    }
    let hdp = init_hue_dependant_gamut_params(h, sr, p);
    compress_gamut(jmh, jmh[0], sr, p, &hdp, false)
}

/// Inverse gamut compression.
pub fn gamut_compress_inv(
    jmh: &F3,
    sr: &ResolvedSharedCompressionParameters,
    p: &GamutCompressParams,
) -> F3 {
    let j = jmh[0];
    let m = jmh[1];
    let h = jmh[2];

    if j <= 0.0 {
        return [0.0, 0.0, h];
    }
    if m <= 0.0 || j > sr.limit_j_max {
        return [j, 0.0, h];
    }
    let hdp = init_hue_dependant_gamut_params(h, sr, p);

    let mut jx = j;
    if jx > hdp.analytical_threshold {
        // Approximation above threshold.
        jx = compress_gamut(jmh, jx, sr, p, &hdp, true)[0];
    }
    compress_gamut(jmh, jx, sr, p, &hdp, true)
}

const GAMMA_TEST_COUNT: usize = 5;

#[derive(Debug, Clone, Copy)]
struct TestData {
    test_jmh: F3,
    j_intersect_source: f32,
    slope: f32,
    j_intersect_cusp: f32,
}

fn generate_gamma_test_data(
    jm_cusp: &F2,
    hue: f32,
    limit_j_max: f32,
    mid_j: f32,
    focus_dist: f32,
) -> [TestData; GAMMA_TEST_COUNT] {
    const TEST_POSITIONS: [f32; GAMMA_TEST_COUNT] = [0.01, 0.1, 0.5, 0.8, 0.99];
    let analytical_threshold = lerpf(jm_cusp[0], limit_j_max, FOCUS_GAIN_BLEND);
    let focus_j = compute_focus_j(jm_cusp[0], mid_j, limit_j_max);

    TEST_POSITIONS.map(|pos| {
        let test_j = lerpf(jm_cusp[0], limit_j_max, pos);
        let slope_gain = get_focus_gain(test_j, analytical_threshold, limit_j_max, focus_dist);
        let j_intersect_source =
            solve_j_intersect(test_j, jm_cusp[1], focus_j, limit_j_max, slope_gain);
        TestData {
            test_jmh: [test_j, jm_cusp[1], hue],
            j_intersect_source,
            slope: compute_compression_vector_slope(
                j_intersect_source,
                focus_j,
                limit_j_max,
                slope_gain,
            ),
            j_intersect_cusp: solve_j_intersect(
                jm_cusp[0],
                jm_cusp[1],
                focus_j,
                limit_j_max,
                slope_gain,
            ),
        }
    })
}

#[allow(clippy::too_many_arguments)]
fn evaluate_gamma_fit(
    jm_cusp: &F2,
    data: &[TestData; GAMMA_TEST_COUNT],
    top_gamma_inv: f32,
    peak_luminance: f32,
    limit_j_max: f32,
    lower_hull_gamma_inv: f32,
    limit_jmh_params: &JMhParams,
) -> bool {
    let luminance_limit = peak_luminance / REFERENCE_LUMINANCE;
    for test_data in data {
        let approx_limit_m = find_gamut_boundary_intersection(
            jm_cusp,
            limit_j_max,
            top_gamma_inv,
            lower_hull_gamma_inv,
            test_data.j_intersect_source,
            test_data.slope,
            test_data.j_intersect_cusp,
        );
        let approx_limit_j = test_data.j_intersect_source + test_data.slope * approx_limit_m;

        let approximate_jmh = [approx_limit_j, approx_limit_m, test_data.test_jmh[2]];
        let new_limit_rgb = jmh_to_rgb(&approximate_jmh, limit_jmh_params);

        if !outside_hull(&new_limit_rgb, luminance_limit) {
            return false;
        }
    }
    true
}

#[allow(clippy::too_many_arguments)]
fn make_upper_hull_gamma(
    hue_table: &Table1D,
    gamut_cusp_table: &mut Table3D,
    peak_luminance: f32,
    limit_j_max: f32,
    mid_j: f32,
    focus_dist: f32,
    lower_hull_gamma_inv: f32,
    limit_jmh_params: &JMhParams,
) {
    for i in table::FIRST_NOMINAL_INDEX..table::UPPER_WRAP_INDEX {
        let hue = hue_table[i];
        let jm_cusp = [gamut_cusp_table[i][0], gamut_cusp_table[i][1]];

        let data = generate_gamma_test_data(&jm_cusp, hue, limit_j_max, mid_j, focus_dist);

        let search_range = GAMMA_SEARCH_STEP;
        let mut low = GAMMA_MINIMUM;
        let mut high = low + search_range;
        let mut outside = false;

        let gamma_fit_predicate = |gamma: f32| {
            evaluate_gamma_fit(
                &jm_cusp,
                &data,
                1.0 / gamma,
                peak_luminance,
                limit_j_max,
                lower_hull_gamma_inv,
                limit_jmh_params,
            )
        };
        while !outside && (high < GAMMA_MAXIMUM) {
            let gamma_found = gamma_fit_predicate(high);
            if !gamma_found {
                low = high;
                high += search_range;
            } else {
                outside = true;
            }
        }

        while (high - low) > GAMMA_ACCURACY {
            let test_gamma = midpoint(high, low);
            let gamma_found = gamma_fit_predicate(test_gamma);
            if gamma_found {
                high = test_gamma;
            } else {
                low = test_gamma;
            }
        }
        gamut_cusp_table[i][2] = 1.0 / high;
    }

    // Copy last populated entries to empty spot 'wrapping' entries.
    gamut_cusp_table[table::LOWER_WRAP_INDEX][2] = gamut_cusp_table[table::LAST_NOMINAL_INDEX][2];
    gamut_cusp_table[table::UPPER_WRAP_INDEX][2] = gamut_cusp_table[table::FIRST_NOMINAL_INDEX][2];
    gamut_cusp_table[table::UPPER_WRAP_INDEX + 1][2] =
        gamut_cusp_table[table::FIRST_NOMINAL_INDEX + 1][2];
}

/// Tonescale pre-calculations.
pub fn init_tone_scale_params(peak_luminance: f32) -> ToneScaleParams {
    // Preset constants that set the desired behavior for the curve.
    let n = peak_luminance;

    let n_r = 100.0f32; // normalized white in nits (what 1.0 should be)
    let g = 1.15f32; // surround / contrast
    let c = 0.18f32; // anchor for 18% grey
    let c_d = 10.013f32; // output luminance of 18% grey (in nits)
    let w_g = 0.14f32; // change in grey between different peak luminance
    let t_1 = 0.04f32; // shadow toe or flare/glare compensation
    let r_hit_min = 128.0f32; // scene-referred value "hitting the roof"
    let r_hit_max = 896.0f32; // scene-referred value "hitting the roof"

    // Calculate output constants.
    let r_hit = r_hit_min + (r_hit_max - r_hit_min) * ((n / n_r).ln() / (10000.0f32 / 100.0).ln());
    let m_0 = n / n_r;
    let m_1 = 0.5 * (m_0 + (m_0 * (m_0 + 4.0 * t_1)).sqrt());
    let u = ((r_hit / m_1) / ((r_hit / m_1) + 1.0)).powf(g);
    let m = m_1 / u;
    let w_i = (n / 100.0).ln() / 2.0f32.ln();
    let c_t = c_d / n_r * (1.0 + w_i * w_g);
    let g_ip = 0.5 * (c_t + (c_t * (c_t + 4.0 * t_1)).sqrt());
    let g_ipp2 = -(m_1 * (g_ip / m).powf(1.0 / g)) / ((g_ip / m).powf(1.0 / g) - 1.0);
    let w_2 = c / g_ipp2;
    let s_2 = w_2 * m_1 * REFERENCE_LUMINANCE;
    let u_2 = ((r_hit / m_1) / ((r_hit / m_1) + w_2)).powf(g);
    let m_2 = m_1 / u_2;
    let inverse_limit = n / (u_2 * n_r);
    let forward_limit = 8.0 * r_hit;
    let log_peak = (n / n_r).log10();

    ToneScaleParams {
        n,
        n_r,
        g,
        t_1,
        c_t,
        s_2,
        u_2,
        m_2,
        forward_limit,
        inverse_limit,
        log_peak,
    }
}

/// Parameters shared by the chroma and gamut compression.
pub fn init_shared_compression_params(
    peak_luminance: f32,
    input_jmh_params: &JMhParams,
    reach_params: &JMhParams,
) -> SharedCompressionParameters {
    let limit_j_max = y_to_j(peak_luminance, input_jmh_params);
    let model_gamma_inv = 1.0 / model_gamma();
    SharedCompressionParameters {
        limit_j_max,
        model_gamma_inv,
        reach_m_table: make_reach_m_table(reach_params, limit_j_max),
    }
}

/// Resolve the shared compression parameters for a hue.
pub fn resolve_compression_params(
    hue: f32,
    p: &SharedCompressionParameters,
) -> ResolvedSharedCompressionParameters {
    ResolvedSharedCompressionParameters {
        limit_j_max: p.limit_j_max,
        model_gamma_inv: p.model_gamma_inv,
        reach_max_m: reach_m_from_table(hue, &p.reach_m_table),
    }
}

/// Chroma compression pre-calculations.
pub fn init_chroma_compress_params(
    peak_luminance: f32,
    ts_params: &ToneScaleParams,
) -> ChromaCompressParams {
    let compr = CHROMA_COMPRESS + (CHROMA_COMPRESS * CHROMA_COMPRESS_FACT) * ts_params.log_peak;
    let sat = max_f(
        0.2,
        CHROMA_EXPAND - (CHROMA_EXPAND * CHROMA_EXPAND_FACT) * ts_params.log_peak,
    );
    let sat_thr = CHROMA_EXPAND_THR / ts_params.n;
    let chroma_compress_scale = (0.03379 * peak_luminance).powf(0.30596) - 0.45135;
    ChromaCompressParams {
        sat,
        sat_thr,
        compr,
        chroma_compress_scale,
    }
}

fn determine_hue_linearity_search_range(gamut_cusp_table: &Table3D) -> [i32; 2] {
    // This function searches through the hues looking for the largest
    // deviations from a linear distribution. We can then use this to initialise
    // the binary search range to something smaller than the full one to
    // reduce the number of lookups per hue lookup.
    const LOWER_PADDING: i32 = 0;
    const UPPER_PADDING: i32 = 1;
    let mut range = [LOWER_PADDING, UPPER_PADDING];
    for (i, entry) in gamut_cusp_table
        .iter()
        .enumerate()
        .take(table::UPPER_WRAP_INDEX)
        .skip(table::FIRST_NOMINAL_INDEX)
    {
        let pos = table::nominal_hue_position_in_uniform_table(entry[2]);
        let delta = i as i32 - pos as i32;
        range[0] = range[0].min(delta + LOWER_PADDING);
        range[1] = range[1].max(delta + UPPER_PADDING);
    }
    range
}

/// Gamut compression pre-calculations (builds the cusp and hue tables).
pub fn init_gamut_compress_params(
    peak_luminance: f32,
    input_jmh_params: &JMhParams,
    limit_jmh_params: &JMhParams,
    ts_params: &ToneScaleParams,
    sh_params: &SharedCompressionParameters,
    reach_params: &JMhParams,
) -> GamutCompressParams {
    let mid_j = y_to_j(ts_params.c_t * REFERENCE_LUMINANCE, input_jmh_params);

    let focus_dist = FOCUS_DISTANCE + FOCUS_DISTANCE * FOCUS_DISTANCE_SCALING * ts_params.log_peak;
    let lower_hull_gamma_inv = 1.0 / (1.14 + 0.07 * ts_params.log_peak);

    let mut hue_table: Box<Table1D> = Box::new([0.0; table::TOTAL_SIZE]);
    let mut gamut_cusp_table = make_uniform_hue_gamut_table(
        reach_params,
        limit_jmh_params,
        peak_luminance,
        ts_params.forward_limit,
        sh_params,
        &mut hue_table,
    );
    let hue_linearity_search_range = determine_hue_linearity_search_range(&gamut_cusp_table);
    make_upper_hull_gamma(
        &hue_table,
        &mut gamut_cusp_table,
        peak_luminance,
        sh_params.limit_j_max,
        mid_j,
        focus_dist,
        lower_hull_gamma_inv,
        limit_jmh_params,
    );
    GamutCompressParams {
        mid_j,
        focus_dist,
        lower_hull_gamma_inv,
        hue_linearity_search_range,
        hue_table,
        gamut_cusp_table,
    }
}
