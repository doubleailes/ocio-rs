//! B-spline fitting and evaluation (port of the computation parts of
//! `GradingBSplineCurve.cpp`).
//!
//! A [`GradingBSplineCurve`] is converted into knots and quadratic polynomial
//! coefficients. All the curves of an RGB curve (or hue curve) are packed into
//! one [`KnotsCoefs`] structure, in the order of the curve type enum.

use crate::error::Result;
use crate::transforms::grading::{
    GradingBSplineCurve, GradingControlPoint, GradingHueCurve, GradingRgbCurve,
};
use crate::types::{BSplineType, HueCurveType, RgbCurveType};

/// Knots and coefficients of all the curves of an RGB or hue curve (port of
/// `GradingBSplineCurveImpl::KnotsCoefs`).
#[derive(Debug, Clone, PartialEq)]
pub struct KnotsCoefs {
    /// Do not apply the op if all curves are identity.
    pub local_bypass: bool,
    /// Offset and count of the knots of each curve (offset is -1 and count is
    /// 0 for identity curves).
    pub knots_offsets: Vec<i32>,
    /// Offset and count of the coefficients of each curve.
    pub coefs_offsets: Vec<i32>,
    /// Packed coefficients of all the curves (`MAX_NUM_COEFS` long).
    pub coefs: Vec<f32>,
    /// Packed knots of all the curves (`MAX_NUM_KNOTS` long).
    pub knots: Vec<f32>,
    pub num_coefs: i32,
    pub num_knots: i32,
}

impl KnotsCoefs {
    /// Maximum size of the knots array (for all curves).
    pub const MAX_NUM_KNOTS: i32 = 120;
    /// Maximum size of the coefs array (for all curves).
    pub const MAX_NUM_COEFS: i32 = 360;

    /// Empty knots and coefs for `num_curves` curves.
    pub fn new(num_curves: usize) -> Self {
        Self {
            local_bypass: false,
            knots_offsets: vec![0; 2 * num_curves],
            coefs_offsets: vec![0; 2 * num_curves],
            coefs: vec![0.0; Self::MAX_NUM_COEFS as usize],
            knots: vec![0.0; Self::MAX_NUM_KNOTS as usize],
            num_coefs: 0,
            num_knots: 0,
        }
    }

    /// Knots and coefs of the four curves of an RGB curve (port of
    /// `DynamicPropertyGradingRGBCurveImpl::precompute`).
    pub fn from_rgb_curve(value: &GradingRgbCurve) -> Result<Self> {
        let mut kc = Self::new(4);
        for c in RgbCurveType::ALL {
            compute_knots_and_coefs(value.curve(c), &mut kc, c as usize, false)?;
        }
        if kc.num_knots <= 0 {
            kc.local_bypass = true;
        }
        Ok(kc)
    }

    /// Knots and coefs of the eight curves of a hue curve (port of
    /// `DynamicPropertyGradingHueCurveImpl::precompute`).
    pub fn from_hue_curve(value: &GradingHueCurve) -> Result<Self> {
        let mut kc = Self::new(8);
        for c in HueCurveType::ALL {
            compute_knots_and_coefs(value.curve(c), &mut kc, c as usize, value.draw_curve_only)?;
        }
        if kc.num_knots <= 0 {
            kc.local_bypass = true;
        }
        Ok(kc)
    }

    fn set_identity(&mut self, curve_idx: usize) {
        self.knots_offsets[curve_idx * 2] = -1;
        self.knots_offsets[curve_idx * 2 + 1] = 0;
        self.coefs_offsets[curve_idx * 2] = -1;
        self.coefs_offsets[curve_idx * 2 + 1] = 0;
    }

    /// Offsets of a curve: (coefs sets, coefs offset, knots count, knots offset).
    #[inline]
    fn layout(&self, c: usize) -> (usize, usize, usize, usize) {
        let coefs_sets = (self.coefs_offsets[2 * c + 1] / 3).max(0) as usize;
        let coefs_offs = self.coefs_offsets[2 * c].max(0) as usize;
        let knots_cnt = self.knots_offsets[2 * c + 1].max(0) as usize;
        let knots_offs = self.knots_offsets[2 * c].max(0) as usize;
        (coefs_sets, coefs_offs, knots_cnt, knots_offs)
    }

    /// Forward evaluation of any spline type. `identity_x` is returned for
    /// identity curves.
    ///
    /// NB: When evaluating hue curves, x should be wrapped to [0,1) by the
    /// caller so there is no extrapolation.
    pub fn eval_curve(&self, c: usize, x: f32, identity_x: f32) -> f32 {
        let (coefs_sets, coefs_offs, knots_cnt, knots_offs) = self.layout(c);
        if coefs_sets == 0 || knots_cnt < 2 {
            return identity_x;
        }
        let kn = &self.knots;
        let co = &self.coefs;

        let kn_start = kn[knots_offs];
        let kn_end = kn[knots_offs + knots_cnt - 1];

        if x <= kn_start {
            let b = co[coefs_offs + coefs_sets];
            let cc = co[coefs_offs + coefs_sets * 2];
            (x - kn_start) * b + cc
        } else if x >= kn_end {
            let a = co[coefs_offs + coefs_sets - 1];
            let b = co[coefs_offs + coefs_sets * 2 - 1];
            let cc = co[coefs_offs + coefs_sets * 3 - 1];
            let k = kn[knots_offs + knots_cnt - 2];
            let t = kn_end - k;
            let slope = 2.0 * a * t + b;
            let offs = (a * t + b) * t + cc;
            (x - kn_end) * slope + offs
        } else {
            let mut i = 0;
            while i < knots_cnt - 2 {
                if x < kn[knots_offs + i + 1] {
                    break;
                }
                i += 1;
            }
            let a = co[coefs_offs + i];
            let b = co[coefs_offs + coefs_sets + i];
            let cc = co[coefs_offs + coefs_sets * 2 + i];
            let k = kn[knots_offs + i];
            let t = x - k;
            (a * t + b) * t + cc
        }
    }

    /// Reverse evaluation of B_SPLINE or DIAGONAL_B_SPLINE curves.
    ///
    /// This is only intended to invert the monotonic curve types. The
    /// horizontal curve types only need to be evaluated in the forward
    /// direction, even when inverting the hue curve transform (the exception
    /// is the HueFX curve, see [`eval_curve_rev_hue`](Self::eval_curve_rev_hue)).
    pub fn eval_curve_rev(&self, c: usize, y: f32) -> f32 {
        let (coefs_sets, coefs_offs, knots_cnt, knots_offs) = self.layout(c);
        if coefs_sets == 0 || knots_cnt < 2 {
            return y;
        }
        let kn = &self.knots;
        let co = &self.coefs;

        let kn_start = kn[knots_offs];
        let kn_end = kn[knots_offs + knots_cnt - 1];
        let kn_start_y = co[coefs_offs + coefs_sets * 2];
        let kn_end_y = {
            let a = co[coefs_offs + coefs_sets - 1];
            let b = co[coefs_offs + coefs_sets * 2 - 1];
            let cc = co[coefs_offs + coefs_sets * 3 - 1];
            let k = kn[knots_offs + knots_cnt - 2];
            let t = kn_end - k;
            (a * t + b) * t + cc
        };

        if y <= kn_start_y {
            // Extrapolate low side.
            let b = co[coefs_offs + coefs_sets];
            let cc = co[coefs_offs + coefs_sets * 2];
            if b.abs() < 1e-5 {
                kn_start
            } else {
                (y - cc) / b + kn_start
            }
        } else if y >= kn_end_y {
            // Extrapolate high side.
            let a = co[coefs_offs + coefs_sets - 1];
            let b = co[coefs_offs + coefs_sets * 2 - 1];
            let cc = co[coefs_offs + coefs_sets * 3 - 1];
            let k = kn[knots_offs + knots_cnt - 2];
            let t = kn_end - k;
            let slope = 2.0 * a * t + b;
            let offs = (a * t + b) * t + cc;
            if slope.abs() < 1e-5 {
                kn_end
            } else {
                (y - offs) / slope + kn_end
            }
        } else {
            let mut i = 0;
            while i < knots_cnt - 2 {
                if y < co[coefs_offs + coefs_sets * 2 + i + 1] {
                    break;
                }
                i += 1;
            }
            let a = co[coefs_offs + i];
            let b = co[coefs_offs + coefs_sets + i];
            let cc = co[coefs_offs + coefs_sets * 2 + i];
            let k = kn[knots_offs + i];
            solve_quadratic(a, b, cc - y, k)
        }
    }

    /// Reverse evaluation of HUE_HUE_B_SPLINE or HUE_FX curves (using
    /// PERIODIC_0_B_SPLINE).
    ///
    /// The output of the HueFX curve is a "delta hue" signal that is added on
    /// to the incoming hue: HueOut = HueIn + HueFX(HueIn). The input to this
    /// function should be HueOut; it returns HueIn. The caller should wrap
    /// the output to ensure it is a hue on [0,1).
    pub fn eval_curve_rev_hue(&self, c: usize, y: f32) -> f32 {
        let (coefs_sets, coefs_offs, knots_cnt, knots_offs) = self.layout(c);
        if coefs_sets == 0 || knots_cnt < 2 {
            return y;
        }
        let kn = &self.knots;
        let co = &self.coefs;

        let kn_start = kn[knots_offs];
        let kn_end = kn[knots_offs + knots_cnt - 1];
        let is_hfx = c == HueCurveType::HueFx as usize;
        let mut kn_start_y = co[coefs_offs + coefs_sets * 2];
        if is_hfx {
            kn_start_y += kn_start;
        }
        let kn_end_y = {
            let a = co[coefs_offs + coefs_sets - 1];
            let b = co[coefs_offs + coefs_sets * 2 - 1];
            let cc = co[coefs_offs + coefs_sets * 3 - 1];
            let k = kn[knots_offs + knots_cnt - 2];
            let t = kn_end - k;
            let v = (a * t + b) * t + cc;
            if is_hfx {
                v + kn_end
            } else {
                v
            }
        };

        let mut y = y;
        if y < kn_start_y {
            // Wrap up into the valid hue range.
            y += (kn_start_y - y).ceil();
        } else if y > kn_end_y {
            // Wrap down into the valid hue range.
            y -= (y - kn_end_y).ceil();
        }

        let mut i = 0;
        while i < knots_cnt - 2 {
            let mut curve_y = co[coefs_offs + coefs_sets * 2 + i + 1];
            if is_hfx {
                curve_y += kn[knots_offs + i + 1];
            }
            if y < curve_y {
                break;
            }
            i += 1;
        }
        let a = co[coefs_offs + i];
        let mut b = co[coefs_offs + coefs_sets + i];
        let mut cc = co[coefs_offs + coefs_sets * 2 + i];
        let k = kn[knots_offs + i];
        if is_hfx {
            cc += k; // Shift curve up so left edge is on the main diagonal.
            b += 1.0; // Add diagonal line.
        }
        solve_quadratic(a, b, cc - y, k)
    }
}

/// Solve `a t^2 + b t + c0 = 0` for the segment starting at knot `k`.
#[inline]
fn solve_quadratic(a: f32, b: f32, c0: f32, k: f32) -> f32 {
    let discrim = (b * b - 4.0 * a * c0).sqrt();
    let denom = discrim + b;
    if denom.abs() < 1e-5 {
        // A~=0, B<0: linear segment with negative slope; use linear inverse.
        return if b.abs() < 1e-5 { k } else { k + (-c0 / b) };
    }
    k + (-2.0 * c0) / denom
}

// ---------------------------------------------------------------------------
// Fitting

/// Wrap, sort and space the control points of a hue curve (port of
/// `PrepHueCurveData`).
fn prep_hue_curve_data(
    ctrl_pnts: &[GradingControlPoint],
    is_periodic: bool,
    is_horizontal: bool,
) -> Vec<GradingControlPoint> {
    let num = ctrl_pnts.len();
    let mut out: Vec<GradingControlPoint> = ctrl_pnts
        .iter()
        .map(|cp| {
            let (x, y) = (cp.x, cp.y);
            // Wrap periodic x values into [0,1).
            if is_periodic && x < 0.0 {
                GradingControlPoint::new(x + 1.0, if is_horizontal { y } else { y + 1.0 })
            } else if is_periodic && x >= 1.0 {
                GradingControlPoint::new(x - 1.0, if is_horizontal { y } else { y - 1.0 })
            } else {
                GradingControlPoint::new(x, y)
            }
        })
        .collect();

    // Sort x and y based on x order (selection sort, as in OCIO).
    for i in 0..num {
        let mut min_index = i;
        let mut min_val = out[i].x;
        for (j, cp) in out.iter().enumerate().skip(i + 1) {
            if cp.x < min_val {
                min_val = cp.x;
                min_index = j;
            }
        }
        out.swap(i, min_index);
    }

    // Ensure that there is a minimum space between the x values.
    const TOL: f32 = 2e-3;
    let x_span = out[num - 1].x - out[0].x;
    for i in 1..out.len() {
        if (out[i].x - out[i - 1].x) < x_span * TOL {
            out[i].x = out[i - 1].x + x_span * TOL;
        }
    }
    if !is_horizontal {
        // Ensure that there is a minimum space between the y values.
        let y_span = out[num - 1].y - out[0].y;
        for i in 1..out.len() {
            if (out[i].y - out[i - 1].y) < y_span * TOL {
                out[i].y = out[i - 1].y + y_span * TOL;
            }
        }
    }

    if is_periodic {
        // Copy a value from each side and wrap it around to the other side.
        let mut first = out[num - 1];
        first.x -= 1.0;
        if !is_horizontal {
            first.y -= 1.0;
        }
        out.insert(0, first);

        let mut last = out[1];
        last.x += 1.0;
        if !is_horizontal {
            last.y += 1.0;
        }
        out.push(last);
    }
    out
}

/// Middle knot of a segment (port of `CalcKsi`).
fn calc_ksi(i: usize, pts: &[GradingControlPoint], slopes: &[f32]) -> f32 {
    let p0 = pts[i];
    let p1 = pts[i + 1];

    let k = 0.2f32;
    let dx = p1.x - p0.x;
    let secant_slope = (p1.y - p0.y) / dx;
    let mut secant = secant_slope;
    let mut m0 = slopes[i];
    let mut m1 = slopes[i + 1];
    if secant < 0.0 {
        m0 = -slopes[i];
        m1 = -slopes[i + 1];
        secant = -secant;
    }

    let x_mid = p0.x + 0.5 * dx;
    let left_bnd = p0.x + dx * k;
    let right_bnd = p1.x - dx * k;
    let mut top_bnd = left_bnd;
    let mut bottom_bnd = right_bnd;
    let mut m_min = m0;
    let mut m_max = m1;
    if m0 > m1 {
        m_max = m0;
        m_min = m1;
        top_bnd = right_bnd;
        bottom_bnd = left_bnd;
    }

    let dm = m_max - m_min;
    let b = 1.0 - 0.5 * k;
    let b_high = m_min + b * dm;
    let b_low = m_min + (1.0 - b) * dm;
    let bbb = m_max * 4.0;
    let bb = m_max * 1.1;
    let m_rel_diff = dm / std_max(0.01, m_max);
    let alpha = std_max(0.0, std_min((m_rel_diff - 0.05) / (0.75 - 0.05), 1.0));
    top_bnd = x_mid + alpha * (top_bnd - x_mid);
    bottom_bnd = x_mid + alpha * (bottom_bnd - x_mid);

    // Calculate the middle knot.
    if secant >= bbb {
        x_mid
    } else if secant > bb {
        let blend = (secant - bb) / (bbb - bb);
        top_bnd + blend * (x_mid - top_bnd)
    } else if secant >= b_high {
        top_bnd
    } else if (secant > b_low) && (b_high != b_low) {
        let blend = (secant - b_low) / (b_high - b_low);
        bottom_bnd + blend * (top_bnd - bottom_bnd)
    } else {
        bottom_bnd
    }
}

use super::super::grading_primary::{std_max, std_min};

/// Coefficients of a fitted spline.
#[derive(Default)]
struct Fit {
    knots: Vec<f32>,
    a: Vec<f32>,
    b: Vec<f32>,
    c: Vec<f32>,
}

/// Port of `FitHueSpline`.
fn fit_hue_spline(pts: &[GradingControlPoint], slopes: &[f32]) -> Fit {
    let mut f = Fit::default();
    f.knots.push(pts[0].x);
    for i in 0..pts.len() - 1 {
        let p0 = pts[i];
        let p1 = pts[i + 1];

        let dx = p1.x - p0.x;
        let secant_slope = (p1.y - p0.y) / dx;

        if ((slopes[i] + slopes[i + 1]) - 2.0 * secant_slope).abs() <= 1e-5 {
            f.c.push(p0.y);
            f.b.push(slopes[i]);
            f.a.push(0.5 * (slopes[i + 1] - slopes[i]) / dx);
        } else {
            // Calculate the middle knot.
            let ksi = calc_ksi(i, pts, slopes);

            // Calculate the coefficients.
            let m_bar = (2.0 * secant_slope - slopes[i + 1])
                + (slopes[i + 1] - slopes[i]) * (ksi - p0.x) / (p1.x - p0.x);
            let eta = (m_bar - slopes[i]) / (ksi - p0.x);
            f.c.push(p0.y);
            f.b.push(slopes[i]);
            f.a.push(0.5 * eta);
            f.c.push(p0.y + slopes[i] * (ksi - p0.x) + 0.5 * eta * (ksi - p0.x) * (ksi - p0.x));
            f.b.push(m_bar);
            f.a.push(0.5 * (slopes[i + 1] - m_bar) / (p1.x - ksi));
            f.knots.push(ksi);
        }
        f.knots.push(p1.x);
    }
    f
}

/// Port of `EstimateHueSlopes`.
fn estimate_hue_slopes(
    pts: &[GradingControlPoint],
    is_periodic: bool,
    is_horizontal: bool,
) -> Vec<f32> {
    let n = pts.len();
    let mut secant_slope = Vec::with_capacity(n);
    let mut secant_len = Vec::with_capacity(n);
    for w in pts.windows(2) {
        // PrepHueCurveData ensures del_x is > 0.
        let del_x = w[1].x - w[0].x;
        let del_y = w[1].y - w[0].y;
        secant_slope.push(del_y / del_x);
        secant_len.push((del_x * del_x + del_y * del_y).sqrt());
    }

    let mut slopes = Vec::with_capacity(n);
    if n == 2 {
        slopes.push(secant_slope[0]);
        slopes.push(secant_slope[0]);
        return slopes;
    }

    slopes.push(0.0);

    if is_horizontal {
        // All horizontal curves and diagonal hue-hue.
        for i in 1..n - 1 {
            let denom = secant_slope[i] + secant_slope[i - 1];
            let mut s = if denom.abs() < 1e-3 {
                let minval = if denom < 0.0 { -1e-3 } else { 1e-3 };
                2.0 * secant_slope[i] * secant_slope[i - 1] / minval
            } else {
                2.0 * secant_slope[i] * secant_slope[i - 1] / denom
            };
            // Set slope to zero at flat areas or extrema.
            if secant_slope[i] * secant_slope[i - 1] <= 0.0 {
                s = 0.0;
            }
            slopes.push(s);
        }
        slopes.push(0.5 * (3.0 * secant_slope[n - 2] - slopes[n - 2]));
        slopes[0] = 0.5 * (3.0 * secant_slope[0] - slopes[1]);
    } else {
        // Diagonal curves except hue-hue (LvL and SvS).
        merge_collinear_secants(&secant_slope, &mut secant_len, n);
        for k in 1..n - 1 {
            let s = (secant_len[k] * secant_slope[k] + secant_len[k - 1] * secant_slope[k - 1])
                / (secant_len[k] + secant_len[k - 1]);
            slopes.push(s);
        }
        const MIN_SLOPE: f32 = 0.01;
        slopes.push(std_max(
            MIN_SLOPE,
            0.5 * (3.0 * secant_slope[n - 2] - slopes[n - 2]),
        ));
        slopes[0] = std_max(MIN_SLOPE, 0.5 * (3.0 * secant_slope[0] - slopes[1]));
    }

    // Adjust slopes that are not shape-preserving.
    for i in 0..n - 1 {
        let mut k = 0.2f32;
        if slopes[i].abs() > slopes[i + 1].abs() {
            k = 1.0 - k;
        }
        let m_near_min = slopes[i] + k * (slopes[i + 1] - slopes[i]);
        let mut scale = 1.0f32;
        if m_near_min != 0.0 {
            scale = 0.75 * 2.0 * secant_slope[i] / m_near_min;
        }
        if scale < 1.0 {
            slopes[i] *= scale;
            slopes[i + 1] *= scale;
        }
    }

    // Copy end slopes from the opposite side.
    if is_periodic {
        slopes[0] = slopes[n - 2];
        slopes[n - 1] = slopes[1];
    }
    slopes
}

/// Use the total length of runs of collinear secants (shared by the slope
/// estimations).
fn merge_collinear_secants(secant_slope: &[f32], secant_len: &mut [f32], n: usize) {
    let mut i = 0usize;
    loop {
        let mut j = i;
        let mut dl = secant_len[i];
        while j < n - 2 && (secant_slope[j + 1] - secant_slope[j]).abs() < 1e-6 {
            dl += secant_len[j + 1];
            j += 1;
        }
        for len in secant_len.iter_mut().take(j + 1).skip(i) {
            *len = dl;
        }
        if j >= n - 3 {
            break;
        }
        i = j + 1;
    }
}

/// Port of `EstimateRGBSlopes`.
fn estimate_rgb_slopes(pts: &[GradingControlPoint]) -> Vec<f32> {
    let n = pts.len();
    let mut secant_slope = Vec::with_capacity(n);
    let mut secant_len = Vec::with_capacity(n);
    for w in pts.windows(2) {
        let del_x = w[1].x - w[0].x;
        let del_y = w[1].y - w[0].y;
        secant_slope.push(del_y / del_x);
        secant_len.push((del_x * del_x + del_y * del_y).sqrt());
    }
    let mut slopes = Vec::with_capacity(n);
    if n == 2 {
        slopes.push(secant_slope[0]);
        slopes.push(secant_slope[0]);
        return slopes;
    }
    merge_collinear_secants(&secant_slope, &mut secant_len, n);
    slopes.push(0.0);
    for k in 1..n - 1 {
        let s = (secant_len[k] * secant_slope[k] + secant_len[k - 1] * secant_slope[k - 1])
            / (secant_len[k] + secant_len[k - 1]);
        slopes.push(s);
    }
    slopes.push(std_max(
        0.01,
        0.5 * (3.0 * secant_slope[n - 2] - slopes[n - 2]),
    ));
    slopes[0] = std_max(0.01, 0.5 * (3.0 * secant_slope[0] - slopes[1]));
    slopes
}

/// Port of `FitRGBSpline`.
fn fit_rgb_spline(pts: &[GradingControlPoint], slopes: &[f32]) -> Fit {
    let mut f = Fit::default();
    f.knots.push(pts[0].x);
    for i in 0..pts.len() - 1 {
        let xi = pts[i].x;
        let xi_pl1 = pts[i + 1].x;
        let yi = pts[i].y;
        let yi_pl1 = pts[i + 1].y;
        let del_x = xi_pl1 - xi;
        let del_y = yi_pl1 - yi;
        let secant_slope = del_y / del_x;
        if ((slopes[i] + slopes[i + 1]) - 2.0 * secant_slope).abs() < 1e-6 {
            f.c.push(yi);
            f.b.push(slopes[i]);
            f.a.push(0.5 * (slopes[i + 1] - slopes[i]) / del_x);
        } else {
            let aa = slopes[i] - secant_slope;
            let bb = slopes[i + 1] - secant_slope;
            let ksi = if aa * bb >= 0.0 {
                (xi + xi_pl1) * 0.5
            } else if aa.abs() > bb.abs() {
                xi_pl1 + aa * del_x / (slopes[i + 1] - slopes[i])
            } else {
                xi + bb * del_x / (slopes[i + 1] - slopes[i])
            };
            let s_bar = (2.0 * secant_slope - slopes[i + 1])
                + (slopes[i + 1] - slopes[i]) * (ksi - xi) / del_x;
            let eta = (s_bar - slopes[i]) / (ksi - xi);
            f.c.push(yi);
            f.b.push(slopes[i]);
            f.a.push(0.5 * eta);
            f.c.push(yi + slopes[i] * (ksi - xi) + 0.5 * eta * (ksi - xi) * (ksi - xi));
            f.b.push(s_bar);
            f.a.push(0.5 * (slopes[i + 1] - s_bar) / (xi_pl1 - ksi));
            f.knots.push(ksi);
        }
        f.knots.push(xi_pl1);
    }
    f
}

/// Port of `AdjustRGBSlopes`: returns true if slopes were adjusted.
fn adjust_rgb_slopes(pts: &[GradingControlPoint], slopes: &mut [f32], knots: &[f32]) -> bool {
    let mut adjustment_done = false;
    let mut i = 0usize;
    for &ksi in knots {
        if i + 1 >= pts.len() {
            break;
        }
        if pts[i].x != ksi {
            let xi = pts[i].x;
            let xi_pl1 = pts[i + 1].x;
            let yi = pts[i].y;
            let yi_pl1 = pts[i + 1].y;
            let s_bar =
                (2.0 * (yi_pl1 - yi) - (ksi - xi) * slopes[i] - (xi_pl1 - ksi) * slopes[i + 1])
                    / (xi_pl1 - xi);
            if s_bar < 0.0 {
                adjustment_done = true;
                let secant = (yi_pl1 - yi) / (xi_pl1 - xi);
                let blend_slope =
                    ((ksi - xi) * slopes[i] + (xi_pl1 - ksi) * slopes[i + 1]) / (xi_pl1 - xi);
                let mut aim_slope = 0.01 * 0.5 * (slopes[i] + slopes[i + 1]);
                if aim_slope > secant {
                    aim_slope = secant;
                }
                let adjust = (2.0 * secant - aim_slope) / blend_slope;
                slopes[i] *= adjust;
                slopes[i + 1] *= adjust;
            }
            i += 1;
        }
    }
    adjustment_done
}

/// Store a fitted curve into `kc`.
fn store(kc: &mut KnotsCoefs, curve_idx: usize, f: &Fit, what: &str) -> Result<()> {
    let num_knots = kc.num_knots;
    let new_knots = f.knots.len() as i32;
    let num_coefs = kc.num_coefs;
    let new_coefs = (f.a.len() * 3) as i32;

    if num_knots + new_knots > KnotsCoefs::MAX_NUM_KNOTS
        || num_coefs + new_coefs > KnotsCoefs::MAX_NUM_COEFS
    {
        crate::bail!("{what} curve: maximum number of control points reached.");
    }

    kc.knots_offsets[curve_idx * 2] = num_knots;
    kc.knots_offsets[curve_idx * 2 + 1] = new_knots;
    kc.coefs_offsets[curve_idx * 2] = num_coefs;
    kc.coefs_offsets[curve_idx * 2 + 1] = new_coefs;

    let nk = num_knots as usize;
    let nc = num_coefs as usize;
    let sz = f.a.len();
    kc.knots[nk..nk + f.knots.len()].copy_from_slice(&f.knots);
    kc.coefs[nc..nc + sz].copy_from_slice(&f.a);
    kc.coefs[nc + sz..nc + 2 * sz].copy_from_slice(&f.b);
    kc.coefs[nc + 2 * sz..nc + 3 * sz].copy_from_slice(&f.c);

    kc.num_knots += new_knots;
    kc.num_coefs += new_coefs;
    Ok(())
}

fn user_slopes(curve: &GradingBSplineCurve) -> Option<Vec<f32>> {
    if !curve.slopes_are_default() && curve.slopes.len() == curve.control_points.len() {
        Some(curve.slopes.clone())
    } else {
        None
    }
}

fn compute_for_rgb_curve(
    curve: &GradingBSplineCurve,
    kc: &mut KnotsCoefs,
    curve_idx: usize,
) -> Result<()> {
    // Skip invalid data and identity.
    if curve.control_points.len() < 2 || curve.is_identity() {
        kc.set_identity(curve_idx);
        return Ok(());
    }
    let pts = &curve.control_points;
    // If the user-supplied slopes are non-zero, use those. Otherwise, estimate
    // slopes based on the control points.
    let mut slopes = user_slopes(curve).unwrap_or_else(|| estimate_rgb_slopes(pts));

    let mut f = fit_rgb_spline(pts, &slopes);
    if adjust_rgb_slopes(pts, &mut slopes, &f.knots) {
        f = fit_rgb_spline(pts, &slopes);
    }
    store(kc, curve_idx, &f, "RGB")
}

fn compute_for_hue_curve(
    curve: &GradingBSplineCurve,
    kc: &mut KnotsCoefs,
    curve_idx: usize,
    draw_curve_only: bool,
) -> Result<()> {
    // Return 0 knots and coefficients when the curve is identity.
    if curve.control_points.len() < 2 || curve.is_identity() {
        if !draw_curve_only {
            // Do not add any knots or coefs. This allows localBypass to be true if all
            // the curves are identities.
            kc.set_identity(curve_idx);
            return Ok(());
        }
        // DrawCurveOnly is set when drawing the splines for a UI. In this mode, the
        // spline is always set on the HueSat curve and the HueCurve eval only computes
        // that one curve. But the curve/spline type are not known, so the polynomial
        // must be set so that it returns the correct values, even if it is an identity.
        // Note that the value returned for an identity varies among the spline types.
        const N_IDENTITY_KNOTS: i32 = 2;
        const N_IDENTITY_COEFS: i32 = 3;
        // Identity curves are linear or constant, so the quadratic coefficient is zero;
        // the linear coefficient matches the slope of the identity curve.
        let linear = if matches!(
            curve.spline_type,
            BSplineType::DiagonalBSpline | BSplineType::HueHueBSpline
        ) {
            1.0
        } else {
            0.0
        };
        let constant = if matches!(
            curve.spline_type,
            BSplineType::Periodic1BSpline | BSplineType::Horizontal1BSpline
        ) {
            1.0
        } else {
            0.0
        };
        let f = Fit {
            knots: vec![0.0, 1.0],
            a: vec![0.0],
            b: vec![linear],
            c: vec![constant],
        };
        debug_assert_eq!(f.knots.len() as i32, N_IDENTITY_KNOTS);
        debug_assert_eq!(f.a.len() as i32 * 3, N_IDENTITY_COEFS);
        return store(kc, curve_idx, &f, "Hue");
    }

    let is_periodic = matches!(
        curve.spline_type,
        BSplineType::Periodic1BSpline | BSplineType::Periodic0BSpline | BSplineType::HueHueBSpline
    );
    let mut is_horizontal = !matches!(
        curve.spline_type,
        BSplineType::DiagonalBSpline | BSplineType::HueHueBSpline
    );

    let pts = prep_hue_curve_data(&curve.control_points, is_periodic, is_horizontal);

    // For the purposes of slope estimation, consider the hue-hue spline to be horizontal.
    if curve.spline_type == BSplineType::HueHueBSpline {
        is_horizontal = true;
    }

    let slopes = match user_slopes(curve) {
        Some(mut slopes) => {
            // Ensure an equal number of slopes and control points for the spline fit.
            if is_periodic {
                let first_slope = slopes[slopes.len() - 1];
                slopes.insert(0, first_slope);
                let last_slope = slopes[0];
                slopes.push(last_slope);
            }
            slopes
        }
        None => estimate_hue_slopes(&pts, is_periodic, is_horizontal),
    };
    if slopes.len() != pts.len() {
        crate::bail!(
            "Hue curve: the number of slopes does not match the number of control points."
        );
    }

    let f = fit_hue_spline(&pts, &slopes);
    store(kc, curve_idx, &f, "Hue")
}

/// Compute the knots and coefficients of a curve and add them to `kc` (port
/// of `GradingBSplineCurveImpl::computeKnotsAndCoefs`). It has to be called
/// for each curve, in the curve type order.
pub fn compute_knots_and_coefs(
    curve: &GradingBSplineCurve,
    kc: &mut KnotsCoefs,
    curve_idx: usize,
    draw_curve_only: bool,
) -> Result<()> {
    if curve.spline_type == BSplineType::BSpline {
        compute_for_rgb_curve(curve, kc, curve_idx)
    } else {
        compute_for_hue_curve(curve, kc, curve_idx, draw_curve_only)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::GradingStyle;

    #[test]
    fn identity_curves_have_no_knots() {
        let kc = KnotsCoefs::from_rgb_curve(&GradingRgbCurve::new(GradingStyle::Log)).unwrap();
        assert!(kc.local_bypass);
        assert_eq!(kc.num_knots, 0);
        assert_eq!(kc.knots_offsets, vec![-1, 0, -1, 0, -1, 0, -1, 0]);
        assert_eq!(kc.eval_curve(0, 0.3, 0.3), 0.3);
        assert_eq!(kc.eval_curve_rev(0, 0.3), 0.3);

        let kc = KnotsCoefs::from_hue_curve(&GradingHueCurve::new(GradingStyle::Log)).unwrap();
        assert!(kc.local_bypass);

        // Draw curve only mode always stores knots.
        let mut hc = GradingHueCurve::new(GradingStyle::Log);
        hc.draw_curve_only = true;
        let kc = KnotsCoefs::from_hue_curve(&hc).unwrap();
        assert!(!kc.local_bypass);
        assert_eq!(kc.num_knots, 16);
        assert_eq!(kc.num_coefs, 24);
        // Hue-sat identity is constant 1, hue-hue identity is diagonal.
        assert_eq!(kc.eval_curve(HueCurveType::HueSat as usize, 0.3, 0.0), 1.0);
        assert_eq!(kc.eval_curve(HueCurveType::HueHue as usize, 0.3, 0.0), 0.3);
        assert_eq!(kc.eval_curve(HueCurveType::HueFx as usize, 0.3, 1.0), 0.0);
    }

    #[test]
    fn linear_curve() {
        // Two points: a line.
        let curve = GradingBSplineCurve::new(&[(0.0, 0.0), (1.0, 2.0)], BSplineType::BSpline);
        let mut kc = KnotsCoefs::new(4);
        compute_knots_and_coefs(&curve, &mut kc, 0, false).unwrap();
        assert_eq!(kc.num_knots, 2);
        assert_eq!(kc.num_coefs, 3);
        for x in [-1.0f32, 0.0, 0.25, 0.5, 1.0, 3.0] {
            let y = kc.eval_curve(0, x, x);
            assert!((y - 2.0 * x).abs() < 1e-6);
            assert!((kc.eval_curve_rev(0, y) - x).abs() < 1e-6);
        }
    }

    #[test]
    fn max_ctrl_pnts_rgb() {
        // Port of GradingRGBCurve_tests.cpp max_ctrl_pnts.
        let pts: Vec<(f32, f32)> = vec![
            (0., 10.),
            (2., 10.),
            (3., 10.),
            (5., 10.),
            (6., 10.),
            (8., 10.),
            (9., 10.5),
            (11., 15.),
            (12., 50.),
            (14., 60.),
            (15., 85.),
            (16., 86.),
            (17., 87.),
            (18., 88.),
            (19., 89.),
            (20., 90.),
            (21., 91.),
            (22., 92.),
            (23., 93.),
            (24., 94.),
            (25., 95.),
            (26., 96.),
            (27., 97.),
            (28., 98.),
            (29., 99.),
            (30., 100.),
        ];
        let c = GradingBSplineCurve::new(&pts, BSplineType::BSpline);
        let rgb = GradingRgbCurve::from_curves(c.clone(), c.clone(), c.clone(), c);
        assert_eq!(
            KnotsCoefs::from_rgb_curve(&rgb).unwrap_err().message(),
            "RGB curve: maximum number of control points reached."
        );
    }

    #[test]
    fn max_ctrl_pnts_hue() {
        // Port of GradingHueCurve_tests.cpp max_ctrl_pnts.
        let mut hc = GradingHueCurve::new(GradingStyle::Video);
        for c in HueCurveType::ALL {
            hc.curve_mut(c).set_num_control_points(28);
        }
        assert_eq!(
            KnotsCoefs::from_hue_curve(&hc).unwrap_err().message(),
            "Hue curve: maximum number of control points reached."
        );
    }

    #[test]
    fn degenerate_curves_do_not_panic() {
        // Duplicated points and weird slopes must not panic.
        let mut curve = GradingBSplineCurve::new(
            &[(0.5, 0.5), (0.5, 0.5), (0.5, 0.7), (1.0, 1.0)],
            BSplineType::BSpline,
        );
        let mut kc = KnotsCoefs::new(4);
        let _ = compute_knots_and_coefs(&curve, &mut kc, 0, false);
        let _ = kc.eval_curve(0, 0.6, 0.6);
        curve.slopes = vec![1.0, 0.0, -3.0, 2.0];
        let mut kc = KnotsCoefs::new(4);
        let _ = compute_knots_and_coefs(&curve, &mut kc, 0, false);
        let _ = kc.eval_curve_rev(0, 0.6);

        for t in [
            BSplineType::DiagonalBSpline,
            BSplineType::HueHueBSpline,
            BSplineType::Periodic1BSpline,
            BSplineType::Periodic0BSpline,
            BSplineType::Horizontal1BSpline,
        ] {
            let mut curve = GradingBSplineCurve::new(&[(0.0, 0.3), (0.0, 0.3), (0.0, 0.2)], t);
            let mut kc = KnotsCoefs::new(8);
            let _ = compute_knots_and_coefs(&curve, &mut kc, 0, false);
            let _ = kc.eval_curve_rev_hue(0, 0.6);
            curve.slopes = vec![1.0, 0.0, 2.0];
            let mut kc = KnotsCoefs::new(8);
            let _ = compute_knots_and_coefs(&curve, &mut kc, 7, true);
            let _ = kc.eval_curve_rev_hue(7, 0.6);
        }
    }
}
