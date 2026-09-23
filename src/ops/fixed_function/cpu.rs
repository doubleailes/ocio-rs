//! CPU renderers of the fixed function op (port of `FixedFunctionOpCPU.cpp`).
//!
//! All renderers work in single precision on interleaved RGBA pixels, as the
//! OCIO scalar CPU renderers do. (The SSE "fast power" variants of the PQ
//! curves are not ported; the accurate scalar path is always used.)

use super::aces2::common::{
    ChromaCompressParams, GamutCompressParams, JMhParams, SharedCompressionParameters,
    ToneScaleParams,
};
use super::aces2::matrix::{Primaries, ACES_AP0, ACES_AP1, F3};
use super::aces2::{self, common::from_degrees, common::to_degrees, common::to_radians};
use super::{FixedFunctionOpData, FixedFunctionOpStyle as S};
use crate::error::Result;
use crate::ops::Pixel;
use std::fmt::Debug;
use std::sync::Arc;

/// A CPU renderer (OCIO's `OpCPU`).
pub(crate) trait Renderer: Debug + Send + Sync {
    /// Name of the OCIO renderer class (used by tests).
    fn name(&self) -> &'static str;
    /// Process pixels in place.
    fn apply(&self, pixels: &mut [Pixel]);
}

/// Shared renderer pointer.
pub(crate) type RendererRc = Arc<dyn Renderer>;

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

/// OCIO's `Clamp` template: `std::min(std::max(min, a), max)`.
#[inline]
fn clamp_t(a: f32, lo: f32, hi: f32) -> f32 {
    min_f(max_f(lo, a), hi)
}

/// OCIO's `CLAMP` macro.
#[inline]
fn clamp_m(a: f32, lo: f32, hi: f32) -> f32 {
    if a > hi {
        hi
    } else if lo > a {
        lo
    } else {
        a
    }
}

// ---------------------------------------------------------------------------
// ACES red modifier

/// Calculate a saturation measure in a safe manner.
#[inline]
fn calc_sat_weight(red: f32, grn: f32, blu: f32, noise_limit: f32) -> f32 {
    let min_val = min_f(red, min_f(grn, blu));
    let max_val = max_f(red, max_f(grn, blu));

    // The numerator is clamped to prevent problems from negative values, the
    // denominator is clamped higher to prevent dark noise from being
    // classified as having high saturation.
    (max_f(1e-10, max_val) - max_f(1e-10, min_val)) / max_f(noise_limit, max_val)
}

#[inline]
fn calc_hue_weight(red: f32, grn: f32, blu: f32, inv_width: f32) -> f32 {
    // Convert RGB to Yab (luma/chroma).
    let a = 2.0 * red - (grn + blu);
    const SQRT3: f32 = 1.7320508075688772;
    let b = SQRT3 * (grn - blu);

    let hue = b.atan2(a);

    // Determine normalized input coords to B-spline.
    let knot_coord = hue * inv_width + 2.0;
    let j = knot_coord as i32; // index

    // These are the coefficients for a quadratic B-spline basis function.
    // (All coefs taken from the ACES ctl code on github.)
    const M: [[f32; 4]; 4] = [
        [0.25, 0.00, 0.00, 0.00],
        [-0.75, 0.75, 0.75, 0.25],
        [0.75, -1.50, 0.00, 1.00],
        [-0.25, 0.75, -0.75, 0.25],
    ];

    // Hue is in range of the window, calculate weight.
    if (0..4).contains(&j) {
        let t = knot_coord - j as f32; // fractional component
        let coefs = &M[j as usize];
        coefs[3] + t * (coefs[2] + t * (coefs[1] + t * coefs[0]))
    } else {
        0.0
    }
}

const RED_MOD_NOISE_LIMIT: f32 = 1e-2;

#[derive(Debug)]
struct AcesRedMod03 {
    inverse: bool,
    one_minus_scale: f32,
    pivot: f32,
    inv_width: f32,
}

impl AcesRedMod03 {
    fn new(inverse: bool) -> Self {
        Self {
            inverse,
            // (1. - scale) from the original ctl code.
            one_minus_scale: 1.0 - 0.85,
            // Offset will be applied to unnormalized input values.
            pivot: 0.03,
            // Note: inv_width = 4 / (width * pi/180) with width = 120.
            inv_width: 1.9098593171027443,
        }
    }
}

impl Renderer for AcesRedMod03 {
    fn name(&self) -> &'static str {
        if self.inverse {
            "Renderer_ACES_RedMod03_Inv"
        } else {
            "Renderer_ACES_RedMod03_Fwd"
        }
    }

    fn apply(&self, pixels: &mut [Pixel]) {
        for px in pixels.iter_mut() {
            let mut red = px[0];
            let mut grn = px[1];
            let mut blu = px[2];

            let f_h = calc_hue_weight(red, grn, blu, self.inv_width);

            // Hue is in range of the window, apply mod.
            if f_h > 0.0 {
                let new_red = if self.inverse {
                    let min_chan = if grn < blu { grn } else { blu };
                    let a = f_h * self.one_minus_scale - 1.0;
                    let b = red - f_h * (self.pivot + min_chan) * self.one_minus_scale;
                    let c = f_h * self.pivot * min_chan * self.one_minus_scale;
                    (-b - (b * b - 4.0 * a * c).sqrt()) / (2.0 * a)
                } else {
                    let f_s = calc_sat_weight(red, grn, blu, RED_MOD_NOISE_LIMIT);
                    red + f_h * f_s * (self.pivot - red) * self.one_minus_scale
                };

                // Restore hue.
                if grn >= blu {
                    // red >= grn >= blu
                    let hue_fac = (grn - blu) / max_f(1e-10, red - blu);
                    grn = hue_fac * (new_red - blu) + blu;
                } else {
                    // red >= blu >= grn
                    let hue_fac = (blu - grn) / max_f(1e-10, red - grn);
                    blu = hue_fac * (new_red - grn) + grn;
                }

                red = new_red;
            }

            px[0] = red;
            px[1] = grn;
            px[2] = blu;
        }
    }
}

#[derive(Debug)]
struct AcesRedMod10 {
    inverse: bool,
    one_minus_scale: f32,
    pivot: f32,
    inv_width: f32,
}

impl AcesRedMod10 {
    fn new(inverse: bool) -> Self {
        Self {
            inverse,
            one_minus_scale: 1.0 - 0.82,
            pivot: 0.03,
            // Note: inv_width = 4 / (width * pi/180) with width = 135.
            inv_width: 1.6976527263135504,
        }
    }
}

impl Renderer for AcesRedMod10 {
    fn name(&self) -> &'static str {
        if self.inverse {
            "Renderer_ACES_RedMod10_Inv"
        } else {
            "Renderer_ACES_RedMod10_Fwd"
        }
    }

    fn apply(&self, pixels: &mut [Pixel]) {
        for px in pixels.iter_mut() {
            let red = px[0];
            let grn = px[1];
            let blu = px[2];

            let f_h = calc_hue_weight(red, grn, blu, self.inv_width);
            if f_h > 0.0 {
                px[0] = if self.inverse {
                    let min_chan = if grn < blu { grn } else { blu };
                    let a = f_h * self.one_minus_scale - 1.0;
                    let b = red - f_h * (self.pivot + min_chan) * self.one_minus_scale;
                    let c = f_h * self.pivot * min_chan * self.one_minus_scale;
                    (-b - (b * b - 4.0 * a * c).sqrt()) / (2.0 * a)
                } else {
                    let f_s = calc_sat_weight(red, grn, blu, RED_MOD_NOISE_LIMIT);
                    red + f_h * f_s * (self.pivot - red) * self.one_minus_scale
                };
            }
        }
    }
}

// ---------------------------------------------------------------------------
// ACES glow

#[inline]
fn rgb_to_yc(red: f32, grn: f32, blu: f32) -> f32 {
    // Convert RGB to YC (luma + chroma factor).
    const YC_RADIUS_WEIGHT: f32 = 1.75;
    let chroma = (blu * (blu - grn) + grn * (grn - red) + red * (red - blu)).sqrt();
    (blu + grn + red + YC_RADIUS_WEIGHT * chroma) / 3.0
}

#[inline]
fn sigmoid_shaper(sat: f32) -> f32 {
    let x = (sat - 0.4) * 5.0;
    let sign = 1.0f32.copysign(x);
    let t = max_f(0.0, 1.0 - 0.5 * sign * x);
    (1.0 + sign * (1.0 - t * t)) * 0.5
}

#[derive(Debug)]
struct AcesGlow {
    inverse: bool,
    glow_gain: f32,
    glow_mid: f32,
}

impl Renderer for AcesGlow {
    fn name(&self) -> &'static str {
        // OCIO uses the same renderer class for the 0.3 and 1.0 versions.
        if self.inverse {
            "Renderer_ACES_Glow03_Inv"
        } else {
            "Renderer_ACES_Glow03_Fwd"
        }
    }

    fn apply(&self, pixels: &mut [Pixel]) {
        for px in pixels.iter_mut() {
            let red = px[0];
            let grn = px[1];
            let blu = px[2];

            // NB: YC is at inScale.
            let yc = rgb_to_yc(red, grn, blu);
            let sat = calc_sat_weight(red, grn, blu, RED_MOD_NOISE_LIMIT);
            let s = sigmoid_shaper(sat);

            let glow_gain = self.glow_gain * s;
            let glow_mid = self.glow_mid;

            let factor = if self.inverse {
                // Apply InvGlow.
                let glow_gain_out = if yc >= glow_mid * 2.0 {
                    0.0
                } else if yc <= (1.0 + glow_gain) * glow_mid * 2.0 / 3.0 {
                    -glow_gain / (1.0 + glow_gain)
                } else {
                    glow_gain * (glow_mid / yc - 0.5) / (glow_gain * 0.5 - 1.0)
                };
                1.0 + glow_gain_out
            } else {
                // Apply FwdGlow.
                let glow_gain_out = if yc >= glow_mid * 2.0 {
                    0.0
                } else if yc <= glow_mid * 2.0 / 3.0 {
                    glow_gain
                } else {
                    glow_gain * (glow_mid / yc - 0.5)
                };
                1.0 + glow_gain_out
            };

            px[0] = red * factor;
            px[1] = grn * factor;
            px[2] = blu * factor;
        }
    }
}

// ---------------------------------------------------------------------------
// ACES dark to dim

#[derive(Debug)]
struct AcesDarkToDim10 {
    gamma: f32,
}

impl AcesDarkToDim10 {
    fn new(gamma: f32) -> Self {
        // Compute Y^gamma / Y.
        Self { gamma: gamma - 1.0 }
    }
}

impl Renderer for AcesDarkToDim10 {
    fn name(&self) -> &'static str {
        "Renderer_ACES_DarkToDim10_Fwd"
    }

    fn apply(&self, pixels: &mut [Pixel]) {
        // With the modest 2% ACES surround, this minLum allows the min/max gain
        // applied to dark colors to be about 0.6 to 1.6.
        const MIN_LUM: f32 = 1e-10;
        for px in pixels.iter_mut() {
            let red = px[0];
            let grn = px[1];
            let blu = px[2];

            // Calculate luminance assuming input is AP1 RGB.
            let y = max_f(
                MIN_LUM,
                0.27222871678091454 * red + 0.67408176581114831 * grn + 0.053689517407937051 * blu,
            );

            let ypow_over_y = y.powf(self.gamma);

            px[0] = red * ypow_over_y;
            px[1] = grn * ypow_over_y;
            px[2] = blu * ypow_over_y;
        }
    }
}

// ---------------------------------------------------------------------------
// ACES gamut compression 1.3

fn compress(dist: f32, thr: f32, scale: f32, power: f32) -> f32 {
    // Normalize distance outside threshold by scale factor.
    let nd = (dist - thr) / scale;
    let p = nd.powf(power);
    thr + scale * nd / (1.0 + p).powf(1.0 / power)
}

fn uncompress(dist: f32, thr: f32, scale: f32, power: f32) -> f32 {
    // Avoid singularity.
    if dist >= (thr + scale) {
        dist
    } else {
        // Normalize distance outside threshold by scale factor.
        let nd = (dist - thr) / scale;
        let p = nd.powf(power);
        thr + scale * (-(p / (p - 1.0))).powf(1.0 / power)
    }
}

#[inline]
fn gamut_comp(
    val: f32,
    ach: f32,
    thr: f32,
    scale: f32,
    power: f32,
    f: fn(f32, f32, f32, f32) -> f32,
) -> f32 {
    // Note: Strict equality is fine here (see OCIO comments).
    if ach == 0.0 {
        return 0.0;
    }

    // Distance from the achromatic axis, aka inverse RGB ratios.
    let dist = (ach - val) / ach.abs();

    // No compression below threshold.
    if dist < thr {
        return val;
    }

    // Compress / Uncompress distance with parameterized shaper function.
    let compr_dist = f(dist, thr, scale, power);

    // Recalculate RGB from compressed distance and achromatic.
    ach - compr_dist * ach.abs()
}

#[derive(Debug)]
struct AcesGamutComp13 {
    inverse: bool,
    thr_cyan: f32,
    thr_magenta: f32,
    thr_yellow: f32,
    power: f32,
    scale_cyan: f32,
    scale_magenta: f32,
    scale_yellow: f32,
}

impl AcesGamutComp13 {
    fn new(data: &FixedFunctionOpData, inverse: bool) -> Self {
        let p = &data.params;
        let lim_cyan = p[0] as f32;
        let lim_magenta = p[1] as f32;
        let lim_yellow = p[2] as f32;
        let thr_cyan = p[3] as f32;
        let thr_magenta = p[4] as f32;
        let thr_yellow = p[5] as f32;
        let power = p[6] as f32;

        // Precompute scale factor for y = 1 intersect.
        let f_scale = |lim: f32, thr: f32| {
            (lim - thr) / (((1.0 - thr) / (lim - thr)).powf(-power) - 1.0).powf(1.0 / power)
        };

        Self {
            inverse,
            thr_cyan,
            thr_magenta,
            thr_yellow,
            power,
            scale_cyan: f_scale(lim_cyan, thr_cyan),
            scale_magenta: f_scale(lim_magenta, thr_magenta),
            scale_yellow: f_scale(lim_yellow, thr_yellow),
        }
    }
}

impl Renderer for AcesGamutComp13 {
    fn name(&self) -> &'static str {
        if self.inverse {
            "Renderer_ACES_GamutComp13_Inv"
        } else {
            "Renderer_ACES_GamutComp13_Fwd"
        }
    }

    fn apply(&self, pixels: &mut [Pixel]) {
        let f: fn(f32, f32, f32, f32) -> f32 = if self.inverse { uncompress } else { compress };
        for px in pixels.iter_mut() {
            let red = px[0];
            let grn = px[1];
            let blu = px[2];

            // Achromatic axis.
            let ach = max_f(red, max_f(grn, blu));

            px[0] = gamut_comp(red, ach, self.thr_cyan, self.scale_cyan, self.power, f);
            px[1] = gamut_comp(
                grn,
                ach,
                self.thr_magenta,
                self.scale_magenta,
                self.power,
                f,
            );
            px[2] = gamut_comp(blu, ach, self.thr_yellow, self.scale_yellow, self.power, f);
        }
    }
}

// ---------------------------------------------------------------------------
// ACES 2

/// Read 8 float chromaticity coordinates starting at `offset`.
fn primaries_from_params(params: &[f64], offset: usize) -> Primaries {
    let mut v = [0.0f32; 8];
    for (i, x) in v.iter_mut().enumerate() {
        *x = params[offset + i] as f32;
    }
    Primaries::from_f32(&v)
}

#[derive(Debug)]
struct AcesOutputTransform20 {
    fwd: bool,
    p_in: JMhParams,
    p_out: JMhParams,
    t: ToneScaleParams,
    s: SharedCompressionParameters,
    c: ChromaCompressParams,
    g: GamutCompressParams,
}

impl AcesOutputTransform20 {
    fn new(data: &FixedFunctionOpData) -> Result<Self> {
        let fwd = data.style == S::AcesOutputTransform20Fwd;
        let peak_luminance = data.params[0] as f32;
        let lim_primaries = primaries_from_params(&data.params, 1);

        let p_in = aces2::init_jmh_params(&ACES_AP0)?;
        let p_out = aces2::init_jmh_params(&lim_primaries)?;
        let t = aces2::init_tone_scale_params(peak_luminance);
        let reach_gamut = aces2::init_jmh_params(&ACES_AP1)?;
        let s = aces2::init_shared_compression_params(peak_luminance, &p_in, &reach_gamut);
        let c = aces2::init_chroma_compress_params(peak_luminance, &t);
        let g =
            aces2::init_gamut_compress_params(peak_luminance, &p_in, &p_out, &t, &s, &reach_gamut);
        Ok(Self {
            fwd,
            p_in,
            p_out,
            t,
            s,
            c,
            g,
        })
    }

    fn fwd(&self, pixels: &mut [Pixel]) {
        for px in pixels.iter_mut() {
            let rgb_in: F3 = [px[0], px[1], px[2]];
            let aab = aces2::rgb_to_aab(&rgb_in, &self.p_in);
            let jmh = aces2::aab_to_jmh(&aab, &self.p_in);

            let rp = aces2::resolve_compression_params(jmh[2], &self.s);
            let h_rad = to_radians(jmh[2]);
            let cos_hr1 = h_rad.cos();
            let sin_hr1 = h_rad.sin();
            let m_norm =
                aces2::chroma_compress_norm(cos_hr1, sin_hr1, self.c.chroma_compress_scale);

            let j_ts = aces2::tonescale_a_to_j_fwd(aab[0], &self.p_in, &self.t);
            let tonemapped_jmh = aces2::chroma_compress_fwd(&jmh, j_ts, m_norm, &rp, &self.c);
            let compressed_jmh = aces2::gamut_compress_fwd(&tonemapped_jmh, &rp, &self.g);

            let aab_out = aces2::jmh_to_aab_trig(&compressed_jmh, cos_hr1, sin_hr1, &self.p_out);
            let rgb_out = aces2::aab_to_rgb(&aab_out, &self.p_out);

            px[0] = rgb_out[0];
            px[1] = rgb_out[1];
            px[2] = rgb_out[2];
        }
    }

    fn inv(&self, pixels: &mut [Pixel]) {
        for px in pixels.iter_mut() {
            let rgb_out: F3 = [px[0], px[1], px[2]];
            let compressed_jmh = aces2::rgb_to_jmh(&rgb_out, &self.p_out);

            let rp = aces2::resolve_compression_params(compressed_jmh[2], &self.s);
            let h_rad = to_radians(compressed_jmh[2]);
            let cos_hr1 = h_rad.cos();
            let sin_hr1 = h_rad.sin();
            let m_norm =
                aces2::chroma_compress_norm(cos_hr1, sin_hr1, self.c.chroma_compress_scale);

            let tonemapped_jmh = aces2::gamut_compress_inv(&compressed_jmh, &rp, &self.g);
            let j = aces2::tonescale_inv(tonemapped_jmh[0], &self.p_in, &self.t);
            let jmh = aces2::chroma_compress_inv(&tonemapped_jmh, j, m_norm, &rp, &self.c);

            let aab = aces2::jmh_to_aab_trig(&jmh, cos_hr1, sin_hr1, &self.p_in);
            let rgb_in = aces2::aab_to_rgb(&aab, &self.p_in);

            px[0] = rgb_in[0];
            px[1] = rgb_in[1];
            px[2] = rgb_in[2];
        }
    }
}

impl Renderer for AcesOutputTransform20 {
    fn name(&self) -> &'static str {
        "Renderer_ACES_OutputTransform20"
    }

    fn apply(&self, pixels: &mut [Pixel]) {
        if self.fwd {
            self.fwd(pixels)
        } else {
            self.inv(pixels)
        }
    }
}

#[derive(Debug)]
struct AcesRgbToJmh20 {
    fwd: bool,
    p: JMhParams,
}

impl AcesRgbToJmh20 {
    fn new(data: &FixedFunctionOpData) -> Result<Self> {
        Ok(Self {
            fwd: data.style == S::AcesRgbToJmh20,
            p: aces2::init_jmh_params(&primaries_from_params(&data.params, 0))?,
        })
    }
}

impl Renderer for AcesRgbToJmh20 {
    fn name(&self) -> &'static str {
        "Renderer_ACES_RGB_TO_JMh_20"
    }

    fn apply(&self, pixels: &mut [Pixel]) {
        for px in pixels.iter_mut() {
            if self.fwd {
                let jmh = aces2::rgb_to_jmh(&[px[0], px[1], px[2]], &self.p);
                px[0] = jmh[0];
                px[1] = jmh[1];
                px[2] = to_degrees(jmh[2]);
            } else {
                let normalised_hue = from_degrees(px[2]);
                let rgb = aces2::jmh_to_rgb(&[px[0], px[1], normalised_hue], &self.p);
                px[0] = rgb[0];
                px[1] = rgb[1];
                px[2] = rgb[2];
            }
        }
    }
}

#[derive(Debug)]
struct AcesRgbToHmj20 {
    fwd: bool,
    p: JMhParams,
}

impl AcesRgbToHmj20 {
    fn new(data: &FixedFunctionOpData) -> Result<Self> {
        Ok(Self {
            fwd: data.style == S::AcesRgbToHmj20,
            p: aces2::init_jmh_params(&primaries_from_params(&data.params, 0))?,
        })
    }
}

impl Renderer for AcesRgbToHmj20 {
    fn name(&self) -> &'static str {
        "Renderer_ACES_RGB_TO_HMJ_20"
    }

    fn apply(&self, pixels: &mut [Pixel]) {
        for px in pixels.iter_mut() {
            if self.fwd {
                let jmh = aces2::rgb_to_jmh(&[px[0], px[1], px[2]], &self.p);
                px[0] = to_degrees(jmh[2]) / 360.0;
                px[1] = jmh[1] / 200.0;
                px[2] = jmh[0] / 100.0;
            } else {
                let normalised_hue = from_degrees(px[0] * 360.0);
                let m = px[1] * 200.0;
                let j = px[2] * 100.0;
                let rgb = aces2::jmh_to_rgb(&[j, m, normalised_hue], &self.p);
                px[0] = rgb[0];
                px[1] = rgb[1];
                px[2] = rgb[2];
            }
        }
    }
}

#[derive(Debug)]
struct AcesTonescaleCompress20 {
    fwd: bool,
    p: JMhParams,
    t: ToneScaleParams,
    s: SharedCompressionParameters,
    c: ChromaCompressParams,
}

impl AcesTonescaleCompress20 {
    fn new(data: &FixedFunctionOpData) -> Result<Self> {
        let fwd = data.style == S::AcesTonescaleCompress20Fwd;
        let peak_luminance = data.params[0] as f32;

        let p = aces2::init_jmh_params(&ACES_AP0)?;
        let t = aces2::init_tone_scale_params(peak_luminance);
        let reach_gamut = aces2::init_jmh_params(&ACES_AP1)?;
        let s = aces2::init_shared_compression_params(peak_luminance, &p, &reach_gamut);
        let c = aces2::init_chroma_compress_params(peak_luminance, &t);
        Ok(Self { fwd, p, t, s, c })
    }
}

impl Renderer for AcesTonescaleCompress20 {
    fn name(&self) -> &'static str {
        "Renderer_ACES_TONESCALE_COMPRESS_20"
    }

    fn apply(&self, pixels: &mut [Pixel]) {
        for px in pixels.iter_mut() {
            let normalised_hue = from_degrees(px[2]);
            let h_rad = to_radians(normalised_hue);
            let cos_hr1 = h_rad.cos();
            let sin_hr1 = h_rad.sin();
            let m_norm =
                aces2::chroma_compress_norm(cos_hr1, sin_hr1, self.c.chroma_compress_scale);
            let rp = aces2::resolve_compression_params(normalised_hue, &self.s);
            let jmh = if self.fwd {
                let j_ts = aces2::tonescale_fwd(px[0], &self.p, &self.t);
                aces2::chroma_compress_fwd(
                    &[px[0], px[1], normalised_hue],
                    j_ts,
                    m_norm,
                    &rp,
                    &self.c,
                )
            } else {
                let j = aces2::tonescale_inv(px[0], &self.p, &self.t);
                aces2::chroma_compress_inv(&[px[0], px[1], normalised_hue], j, m_norm, &rp, &self.c)
            };
            px[0] = jmh[0];
            px[1] = jmh[1];
            px[2] = to_degrees(jmh[2]);
        }
    }
}

#[derive(Debug)]
struct AcesGamutCompress20 {
    fwd: bool,
    s: SharedCompressionParameters,
    g: GamutCompressParams,
}

impl AcesGamutCompress20 {
    fn new(data: &FixedFunctionOpData) -> Result<Self> {
        let fwd = data.style == S::AcesGamutCompress20Fwd;
        let peak_luminance = data.params[0] as f32;
        let limiting_primaries = primaries_from_params(&data.params, 1);

        let p_in = aces2::init_jmh_params(&ACES_AP0)?;
        let p_lim = aces2::init_jmh_params(&limiting_primaries)?;
        let t = aces2::init_tone_scale_params(peak_luminance);
        let reach_gamut = aces2::init_jmh_params(&ACES_AP1)?;
        let s = aces2::init_shared_compression_params(peak_luminance, &p_in, &reach_gamut);
        let g =
            aces2::init_gamut_compress_params(peak_luminance, &p_in, &p_lim, &t, &s, &reach_gamut);
        Ok(Self { fwd, s, g })
    }
}

impl Renderer for AcesGamutCompress20 {
    fn name(&self) -> &'static str {
        "Renderer_ACES_GAMUT_COMPRESS_20"
    }

    fn apply(&self, pixels: &mut [Pixel]) {
        for px in pixels.iter_mut() {
            let normalised_hue = from_degrees(px[2]);
            let rp = aces2::resolve_compression_params(normalised_hue, &self.s);
            let jmh = if self.fwd {
                aces2::gamut_compress_fwd(&[px[0], px[1], normalised_hue], &rp, &self.g)
            } else {
                aces2::gamut_compress_inv(&[px[0], px[1], normalised_hue], &rp, &self.g)
            };
            px[0] = jmh[0];
            px[1] = jmh[1];
            px[2] = to_degrees(jmh[2]);
        }
    }
}

// ---------------------------------------------------------------------------
// Rec.2100 surround

#[derive(Debug)]
struct Rec2100Surround {
    gamma: f32,
    min_lum: f32,
}

impl Rec2100Surround {
    fn new(data: &FixedFunctionOpData) -> Self {
        let fwd = data.style == S::Rec2100SurroundFwd;
        let mut gamma = data.params[0] as f32;
        let min_lum = if fwd { 1e-4 } else { 1e-4f32.powf(gamma) };
        gamma = if fwd { gamma } else { 1.0 / gamma };
        // Compute Y^gamma / Y.
        Self {
            gamma: gamma - 1.0,
            min_lum,
        }
    }
}

impl Renderer for Rec2100Surround {
    fn name(&self) -> &'static str {
        "Renderer_REC2100_Surround"
    }

    fn apply(&self, pixels: &mut [Pixel]) {
        for px in pixels.iter_mut() {
            let red = px[0];
            let grn = px[1];
            let blu = px[2];

            // Calculate luminance assuming input is Rec.2100 RGB.
            let mut y = 0.2627 * red + 0.6780 * grn + 0.0593 * blu;

            // Mirror the function around the origin.
            y = y.abs();

            // Since the slope may approach infinity as Y approaches 0, limit the
            // min value to avoid gaining up the RGB values (which may not be as
            // close to 0).
            y = max_f(self.min_lum, y);

            let ypow_over_y = y.powf(self.gamma);

            px[0] = red * ypow_over_y;
            px[1] = grn * ypow_over_y;
            px[2] = blu * ypow_over_y;
        }
    }
}

// ---------------------------------------------------------------------------
// HSV

// These HSV conversion routines are designed to handle extended range values.
// If RGB are non-negative or all negative, S is on [0,1]. If RGB are a mix of
// pos and neg, S is on [1,2]. The H is [0,1] for all inputs, with 1 meaning 360
// degrees. For RGB on [0,1], the algorithm is the classic HSV formula.

#[derive(Debug)]
struct RgbToHsv;

impl Renderer for RgbToHsv {
    fn name(&self) -> &'static str {
        "Renderer_RGB_TO_HSV"
    }

    fn apply(&self, pixels: &mut [Pixel]) {
        for px in pixels.iter_mut() {
            let red = px[0];
            let grn = px[1];
            let blu = px[2];

            let rgb_min = min_f(min_f(red, grn), blu);
            let rgb_max = max_f(max_f(red, grn), blu);

            let mut val = rgb_max;
            let mut sat = 0.0f32;
            let mut hue = 0.0f32;

            if rgb_min != rgb_max {
                // Sat
                let delta = rgb_max - rgb_min;
                if rgb_max != 0.0 {
                    sat = delta / rgb_max;
                }

                // Hue
                hue = if red == rgb_max {
                    (grn - blu) / delta
                } else if grn == rgb_max {
                    2.0 + (blu - red) / delta
                } else {
                    4.0 + (red - grn) / delta
                };
                if hue < 0.0 {
                    hue += 6.0;
                }
                hue *= 0.16666666666666666;
            }

            // Handle extended range inputs.
            if rgb_min < 0.0 {
                val += rgb_min;
            }
            if -rgb_min > rgb_max {
                sat = (rgb_max - rgb_min) / -rgb_min;
            }

            px[0] = hue;
            px[1] = sat;
            px[2] = val;
        }
    }
}

// This algorithm is designed to handle extended range values. H is nominally
// on [0,1], but values outside this are accepted and wrapped back into range.
// S is nominally on [0,1] for non-negative RGB but may extend up to 2. S values
// outside [0,MAX_SAT] are clamped.

#[derive(Debug)]
struct HsvToRgb;

impl Renderer for HsvToRgb {
    fn name(&self) -> &'static str {
        "Renderer_HSV_TO_RGB"
    }

    fn apply(&self, pixels: &mut [Pixel]) {
        const MAX_SAT: f32 = 1.999;
        for px in pixels.iter_mut() {
            let hue = (px[0] - px[0].floor()) * 6.0;
            let sat = clamp_t(px[1], 0.0, MAX_SAT);
            let val = px[2];

            let red = clamp_t((hue - 3.0).abs() - 1.0, 0.0, 1.0);
            let grn = clamp_t(2.0 - (hue - 2.0).abs(), 0.0, 1.0);
            let blu = clamp_t(2.0 - (hue - 4.0).abs(), 0.0, 1.0);

            let mut rgb_max = val;
            let mut rgb_min = val * (1.0 - sat);

            // Handle extended range inputs.
            if sat > 1.0 {
                rgb_min = val * (1.0 - sat) / (2.0 - sat);
                rgb_max = val - rgb_min;
            }
            if val < 0.0 {
                rgb_min = val / (2.0 - sat);
                rgb_max = val - rgb_min;
            }

            let delta = rgb_max - rgb_min;
            px[0] = red * delta + rgb_min;
            px[1] = grn * delta + rgb_min;
            px[2] = blu * delta + rgb_min;
        }
    }
}

// ---------------------------------------------------------------------------
// HSY

/// The flavor of the HSY conversions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HsyStyle {
    Lin,
    Log,
    Vid,
}

#[derive(Debug)]
struct HsyToRgb(HsyStyle);

impl Renderer for HsyToRgb {
    fn name(&self) -> &'static str {
        match self.0 {
            HsyStyle::Lin => "Renderer_HSY_LIN_TO_RGB",
            HsyStyle::Log => "Renderer_HSY_LOG_TO_RGB",
            HsyStyle::Vid => "Renderer_HSY_VID_TO_RGB",
        }
    }

    fn apply(&self, pixels: &mut [Pixel]) {
        for px in pixels.iter_mut() {
            // Make magenta 0 hue, rather than red.
            let mut hue = px[0] - 1.0 / 6.0;
            let mut sat = px[1];
            let luma = px[2];

            // Rotate hue 180 deg. for negative luma values.
            hue = if luma < 0.0 { hue + 0.5 } else { hue };
            hue = (hue - hue.floor()) * 6.0;

            let mut red = clamp_m((hue - 3.0).abs() - 1.0, 0.0, 1.0);
            let mut grn = clamp_m(2.0 - (hue - 2.0).abs(), 0.0, 1.0);
            let mut blu = clamp_m(2.0 - (hue - 4.0).abs(), 0.0, 1.0);

            let curr_y = 0.2126 * red + 0.7152 * grn + 0.0722 * blu;
            red *= luma / curr_y;
            grn *= luma / curr_y;
            blu *= luma / curr_y;

            let dist_rgb = (red - luma).abs() + (grn - luma).abs() + (blu - luma).abs();

            let gain_s = match self.0 {
                HsyStyle::Lin => {
                    let sum_rgb = red + grn + blu;

                    let k = 0.15f32;
                    let lo_gain = 5.0f32;

                    sat /= 1.4;
                    let mut tmp = -sat * sum_rgb + sat * 3.0 * luma + dist_rgb;
                    // Don't allow tmp to go negative, which would cause a negative gainS.
                    tmp = max_f(1e-6, tmp);

                    let mut s1 = sat * (k + 3.0 * luma) / tmp;
                    // Prevent gainS from becoming too extreme.
                    s1 = min_f(s1, 50.0);

                    let s0 = sat / max_f(1e-10, dist_rgb * lo_gain);

                    let max_lum = 0.01f32;
                    let min_lum = max_lum * 0.1;
                    let alpha = clamp_m((luma - min_lum) / (max_lum - min_lum), 0.0, 1.0);

                    if alpha == 1.0 {
                        s1
                    } else if alpha == 0.0 {
                        s0
                    } else {
                        let a = dist_rgb * lo_gain * (1.0 - alpha) * (sum_rgb - 3.0 * luma);
                        let b = dist_rgb * lo_gain * (1.0 - alpha) * (k + 3.0 * luma)
                            + dist_rgb * alpha
                            - sat * (sum_rgb - 3.0 * luma);
                        let c = -sat * (k + 3.0 * luma);
                        let discrim = (b * b - 4.0 * a * c).sqrt();
                        let denom = -discrim - b;
                        let gain_s = (2.0 * c) / denom;
                        if gain_s >= 0.0 {
                            gain_s
                        } else {
                            (2.0 * c) / (denom + discrim * 2.0)
                        }
                    }
                }
                HsyStyle::Log => {
                    let sat_gain = 4.0f32;
                    let curr_sat = dist_rgb * sat_gain;
                    sat / max_f(1e-10, curr_sat)
                }
                HsyStyle::Vid => {
                    let sat_gain = 1.25f32;
                    let curr_sat = dist_rgb * sat_gain;
                    sat / max_f(1e-10, curr_sat)
                }
            };

            px[0] = luma + gain_s * (red - luma);
            px[1] = luma + gain_s * (grn - luma);
            px[2] = luma + gain_s * (blu - luma);
        }
    }
}

#[derive(Debug)]
struct RgbToHsy(HsyStyle);

impl Renderer for RgbToHsy {
    fn name(&self) -> &'static str {
        match self.0 {
            HsyStyle::Lin => "Renderer_RGB_TO_HSY_LIN",
            HsyStyle::Log => "Renderer_RGB_TO_HSY_LOG",
            HsyStyle::Vid => "Renderer_RGB_TO_HSY_VID",
        }
    }

    fn apply(&self, pixels: &mut [Pixel]) {
        for px in pixels.iter_mut() {
            let red = px[0];
            let grn = px[1];
            let blu = px[2];

            let rgb_min = min_f(min_f(red, grn), blu);
            let rgb_max = max_f(max_f(red, grn), blu);

            let luma = 0.2126 * red + 0.7152 * grn + 0.0722 * blu;

            let rm = red - luma;
            let gm = grn - luma;
            let bm = blu - luma;

            let dist_rgb = rm.abs() + gm.abs() + bm.abs();

            let sat = match self.0 {
                HsyStyle::Lin => {
                    let sum_rgb = red + grn + blu;
                    let k = 0.15f32;
                    let sat_hi = dist_rgb / max_f(0.07 * dist_rgb + 1e-6, k + sum_rgb);
                    let lo_gain = 5.0f32;
                    let sat_lo = dist_rgb * lo_gain;
                    let max_lum = 0.01f32;
                    let min_lum = max_lum * 0.1;
                    let alpha = clamp_m((luma - min_lum) / (max_lum - min_lum), 0.0, 1.0);
                    let sat = sat_lo + alpha * (sat_hi - sat_lo);
                    sat * 1.4
                }
                HsyStyle::Log => dist_rgb * 4.0,
                HsyStyle::Vid => dist_rgb * 1.25,
            };

            // NB: Unlike typical HSV, HSY maps magenta rather than red to a hue
            // of zero. (This allows for better placement of red when
            // manipulating curves in a UI.)
            let mut hue = 0.0f32;
            if rgb_min != rgb_max {
                let delta = rgb_max - rgb_min;
                hue = if red == rgb_max {
                    1.0 + (grn - blu) / delta
                } else if grn == rgb_max {
                    3.0 + (blu - red) / delta
                } else {
                    5.0 + (red - grn) / delta
                };
                hue *= 0.16666666666666666;
            }

            px[0] = hue;
            px[1] = sat;
            px[2] = luma;
        }
    }
}

// ---------------------------------------------------------------------------
// CIE conversions

#[derive(Debug)]
struct XyzToXyy;

impl Renderer for XyzToXyy {
    fn name(&self) -> &'static str {
        "Renderer_XYZ_TO_xyY"
    }

    fn apply(&self, pixels: &mut [Pixel]) {
        for px in pixels.iter_mut() {
            let (x_, y_, z_) = (px[0], px[1], px[2]);
            let mut d = x_ + y_ + z_;
            d = if d == 0.0 { 0.0 } else { 1.0 / d };
            px[0] = x_ * d;
            px[1] = y_ * d;
            px[2] = y_;
        }
    }
}

#[derive(Debug)]
struct XyyToXyz;

impl Renderer for XyyToXyz {
    fn name(&self) -> &'static str {
        "Renderer_xyY_TO_XYZ"
    }

    fn apply(&self, pixels: &mut [Pixel]) {
        for px in pixels.iter_mut() {
            let (x, y, y_) = (px[0], px[1], px[2]);
            let d = if y == 0.0 { 0.0 } else { 1.0 / y };
            px[0] = y_ * x * d;
            px[1] = y_;
            px[2] = y_ * (1.0 - x - y) * d;
        }
    }
}

#[derive(Debug)]
struct XyzToUvy;

impl Renderer for XyzToUvy {
    fn name(&self) -> &'static str {
        "Renderer_XYZ_TO_uvY"
    }

    fn apply(&self, pixels: &mut [Pixel]) {
        for px in pixels.iter_mut() {
            let (x_, y_, z_) = (px[0], px[1], px[2]);
            let mut d = x_ + 15.0 * y_ + 3.0 * z_;
            d = if d == 0.0 { 0.0 } else { 1.0 / d };
            px[0] = 4.0 * x_ * d;
            px[1] = 9.0 * y_ * d;
            px[2] = y_;
        }
    }
}

#[derive(Debug)]
struct UvyToXyz;

impl Renderer for UvyToXyz {
    fn name(&self) -> &'static str {
        "Renderer_uvY_TO_XYZ"
    }

    fn apply(&self, pixels: &mut [Pixel]) {
        for px in pixels.iter_mut() {
            let (u, v, y_) = (px[0], px[1], px[2]);
            let d = if v == 0.0 { 0.0 } else { 1.0 / v };
            px[0] = (9.0 / 4.0) * y_ * u * d;
            px[1] = y_;
            px[2] = (3.0 / 4.0) * y_ * (4.0 - u - 6.666666666666667 * v) * d;
        }
    }
}

#[derive(Debug)]
struct XyzToLuv;

impl Renderer for XyzToLuv {
    fn name(&self) -> &'static str {
        "Renderer_XYZ_TO_LUV"
    }

    fn apply(&self, pixels: &mut [Pixel]) {
        for px in pixels.iter_mut() {
            let (x_, y_, z_) = (px[0], px[1], px[2]);
            let mut d = x_ + 15.0 * y_ + 3.0 * z_;
            d = if d == 0.0 { 0.0 } else { 1.0 / d };
            let u = 4.0 * x_ * d;
            let v = 9.0 * y_ * d;

            let lstar = if y_ <= 0.008856451679 {
                9.0329629629629608 * y_
            } else {
                1.16 * y_.powf(0.333333333) - 0.16
            };
            let ustar = 13.0 * lstar * (u - 0.19783001); // D65 white
            let vstar = 13.0 * lstar * (v - 0.46831999); // D65 white

            px[0] = lstar;
            px[1] = ustar;
            px[2] = vstar;
        }
    }
}

#[derive(Debug)]
struct LuvToXyz;

impl Renderer for LuvToXyz {
    fn name(&self) -> &'static str {
        "Renderer_LUV_TO_XYZ"
    }

    fn apply(&self, pixels: &mut [Pixel]) {
        for px in pixels.iter_mut() {
            let (lstar, ustar, vstar) = (px[0], px[1], px[2]);

            let d = if lstar == 0.0 {
                0.0
            } else {
                0.076923076923076927 / lstar
            };
            let u = ustar * d + 0.19783001; // D65 white
            let v = vstar * d + 0.46831999; // D65 white

            let tmp = (lstar + 0.16) * 0.86206896551724144;
            let y_ = if lstar <= 0.08 {
                0.11070564598794539 * lstar
            } else {
                tmp * tmp * tmp
            };

            let dd = if v == 0.0 { 0.0 } else { 0.25 / v };
            px[0] = 9.0 * y_ * u * dd;
            px[1] = y_;
            px[2] = y_ * (12.0 - 3.0 * u - 20.0 * v) * dd;
        }
    }
}

// ---------------------------------------------------------------------------
// ST-2084 (PQ)

mod st_2084 {
    pub const M1: f64 = 0.25 * 2610. / 4096.;
    pub const M2: f64 = 128. * 2523. / 4096.;
    pub const C2: f64 = 32. * 2413. / 4096.;
    pub const C3: f64 = 32. * 2392. / 4096.;
    pub const C1: f64 = C3 - C2 + 1.;
}

#[derive(Debug)]
struct PqToLin;

impl Renderer for PqToLin {
    fn name(&self) -> &'static str {
        "Renderer_PQ_TO_LIN"
    }

    fn apply(&self, pixels: &mut [Pixel]) {
        use st_2084::*;
        let (m1, m2, c1, c2, c3) = (M1 as f32, M2 as f32, C1 as f32, C2 as f32, C3 as f32);
        for px in pixels.iter_mut() {
            for v in px.iter_mut().take(3) {
                let vabs = v.abs();
                let x = vabs.powf(1.0 / m2);
                let nits = (max_f(0.0, x - c1) / (c2 - c3 * x)).powf(1.0 / m1);
                // Output scale is 1.0 = 10000 nits, we map it to make 1.0 = 100 nits.
                *v = (100.0 * nits).copysign(*v);
            }
        }
    }
}

#[derive(Debug)]
struct LinToPq;

impl Renderer for LinToPq {
    fn name(&self) -> &'static str {
        "Renderer_LIN_TO_PQ"
    }

    fn apply(&self, pixels: &mut [Pixel]) {
        use st_2084::*;
        let (m1, m2, c1, c2, c3) = (M1 as f32, M2 as f32, C1 as f32, C2 as f32, C3 as f32);
        for px in pixels.iter_mut() {
            for v in px.iter_mut().take(3) {
                // Input is in nits/100, convert to [0,1], where 1 is 10000 nits.
                let l = (*v * 0.01).abs();
                let y = l.powf(m1);
                let ratpoly = (c1 + c2 * y) / (1.0 + c3 * y);
                let n = ratpoly.powf(m2);
                *v = n.copysign(*v);
                // Note: the PQ value for zero is 0.836^78.84 = 7.36e-07 so there
                // is a very small jump in the mirroring at zero.
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Gamma / log curve

#[derive(Debug, Clone, Copy)]
struct GammaSegment {
    // Ygamma = slope * (Xlin + off)^power;
    power: f32,
    slope: f32,
    off: f32,
}

#[derive(Debug, Clone, Copy)]
struct GammaLogSegment {
    // Ylog = logSlope * log( linSlope * Xlin + linOff, base) + logOff;
    log_slope: f32,
    log_off: f32,
    lin_slope: f32,
    lin_off: f32,
}

#[derive(Debug)]
struct LinToGammaLog {
    inverse: bool,
    mirror: f32,
    brk: f32,
    gamma_seg: GammaSegment,
    log_seg: GammaLogSegment,
    prime_break: f32,
    prime_mirror: f32,
}

impl LinToGammaLog {
    fn new(data: &FixedFunctionOpData, inverse: bool) -> Self {
        let p = &data.params;
        // Store the parameters, baking the log base conversion into 'logSlope'.
        let mirror = p[0] as f32;
        let brk = p[1] as f32;
        let gamma_seg = GammaSegment {
            power: p[2] as f32,
            slope: p[3] as f32,
            off: p[4] as f32,
        };
        let log_seg = GammaLogSegment {
            log_slope: (p[6] / p[5].ln()) as f32,
            log_off: p[7] as f32,
            lin_slope: p[8] as f32,
            lin_off: p[9] as f32,
        };
        // Assuming that the function is continuous, use the gamma segment to
        // compute the break point in the non-linear domain.
        let prime_break = gamma_seg.slope * (brk + gamma_seg.off).powf(gamma_seg.power);
        let prime_mirror = gamma_seg.slope * (mirror + gamma_seg.off).powf(gamma_seg.power);
        Self {
            inverse,
            mirror,
            brk,
            gamma_seg,
            log_seg,
            prime_break,
            prime_mirror,
        }
    }
}

impl Renderer for LinToGammaLog {
    fn name(&self) -> &'static str {
        if self.inverse {
            "Renderer_GAMMA_LOG_TO_LIN"
        } else {
            "Renderer_LIN_TO_GAMMA_LOG"
        }
    }

    fn apply(&self, pixels: &mut [Pixel]) {
        let g = &self.gamma_seg;
        let l = &self.log_seg;
        for px in pixels.iter_mut() {
            for v in px.iter_mut().take(3) {
                if self.inverse {
                    let eprime_in = *v;
                    let mirror_in = eprime_in - self.prime_mirror;
                    let eprime = mirror_in.abs() + self.prime_mirror;
                    let e = if eprime < self.prime_break {
                        (eprime / g.slope).powf(1.0 / g.power) - g.off
                    } else {
                        (((eprime - l.log_off) / l.log_slope).exp() - l.lin_off) / l.lin_slope
                    };
                    // Flip the sign below the mirror point.
                    *v = e * 1.0f32.copysign(mirror_in);
                } else {
                    let e_in = *v;
                    let mirror_in = e_in - self.mirror;
                    let e = mirror_in.abs() + self.mirror;
                    let eprime = if e < self.brk {
                        g.slope * (e + g.off).powf(g.power)
                    } else {
                        l.log_slope * (l.lin_slope * e + l.lin_off).ln() + l.log_off
                    };
                    *v = eprime * 1.0f32.copysign(mirror_in);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Double log curve

#[derive(Debug, Clone, Copy)]
struct DoubleLogSegment {
    // Ylog = logSlope * log( linSlope * Xlin + linOff, base) + logOff;
    log_slope: f32,
    log_off: f32,
    lin_slope: f32,
    lin_off: f32,
}

#[derive(Debug, Clone, Copy)]
struct LinSegment {
    // Ylin = slope * Xlin + off;
    slope: f32,
    off: f32,
}

#[derive(Debug)]
struct LinToDoubleLog {
    inverse: bool,
    break1: f32,
    break2: f32,
    log_seg1: DoubleLogSegment,
    log_seg2: DoubleLogSegment,
    lin_seg: LinSegment,
    break1_log: f32,
    break2_log: f32,
}

impl LinToDoubleLog {
    fn new(data: &FixedFunctionOpData, inverse: bool) -> Self {
        let p = &data.params;
        // Store the parameters, baking the log base conversion into 'logSlope'.
        let base = p[0] as f32;
        let break1 = p[1] as f32;
        let break2 = p[2] as f32;
        let log_seg1 = DoubleLogSegment {
            log_slope: p[3] as f32 / base.ln(),
            log_off: p[4] as f32,
            lin_slope: p[5] as f32,
            lin_off: p[6] as f32,
        };
        let log_seg2 = DoubleLogSegment {
            log_slope: p[7] as f32 / base.ln(),
            log_off: p[8] as f32,
            lin_slope: p[9] as f32,
            lin_off: p[10] as f32,
        };
        let lin_seg = LinSegment {
            slope: p[11] as f32,
            off: p[12] as f32,
        };
        // Calculate the break locations in log space (note that the break
        // points belong to the log segments, not the linear segment which may
        // be missing).
        let break1_log = log_seg1.log_slope * (log_seg1.lin_slope * break1 + log_seg1.lin_off).ln()
            + log_seg1.log_off;
        let break2_log = log_seg2.log_slope * (log_seg2.lin_slope * break2 + log_seg2.lin_off).ln()
            + log_seg2.log_off;
        Self {
            inverse,
            break1,
            break2,
            log_seg1,
            log_seg2,
            lin_seg,
            break1_log,
            break2_log,
        }
    }
}

impl Renderer for LinToDoubleLog {
    fn name(&self) -> &'static str {
        if self.inverse {
            "Renderer_DOUBLE_LOG_TO_LIN"
        } else {
            "Renderer_LIN_TO_DOUBLE_LOG"
        }
    }

    fn apply(&self, pixels: &mut [Pixel]) {
        let s1 = &self.log_seg1;
        let s2 = &self.log_seg2;
        let ls = &self.lin_seg;
        for px in pixels.iter_mut() {
            for v in px.iter_mut().take(3) {
                let x = *v;
                *v = if self.inverse {
                    if x <= self.break1_log {
                        (((x - s1.log_off) / s1.log_slope).exp() - s1.lin_off) / s1.lin_slope
                    } else if x < self.break2_log {
                        (x - ls.off) / ls.slope
                    } else {
                        (((x - s2.log_off) / s2.log_slope).exp() - s2.lin_off) / s2.lin_slope
                    }
                } else if x <= self.break1 {
                    // Linear segment may not exist or be valid. Thus we include
                    // the break points in the log segments.
                    s1.log_slope * (s1.lin_slope * x + s1.lin_off).ln() + s1.log_off
                } else if x < self.break2 {
                    ls.slope * x + ls.off
                } else {
                    s2.log_slope * (s2.lin_slope * x + s2.lin_off).ln() + s2.log_off
                };
            }
        }
    }
}

// ---------------------------------------------------------------------------

/// Build the CPU renderer of validated fixed function data (port of
/// `GetFixedFunctionCPURenderer`).
pub(crate) fn get_fixed_function_cpu_renderer(data: &FixedFunctionOpData) -> Result<RendererRc> {
    Ok(match data.style {
        S::AcesRedMod03Fwd => Arc::new(AcesRedMod03::new(false)),
        S::AcesRedMod03Inv => Arc::new(AcesRedMod03::new(true)),
        S::AcesRedMod10Fwd => Arc::new(AcesRedMod10::new(false)),
        S::AcesRedMod10Inv => Arc::new(AcesRedMod10::new(true)),
        S::AcesGlow03Fwd => Arc::new(AcesGlow {
            inverse: false,
            glow_gain: 0.075,
            glow_mid: 0.1,
        }),
        S::AcesGlow03Inv => Arc::new(AcesGlow {
            inverse: true,
            glow_gain: 0.075,
            glow_mid: 0.1,
        }),
        S::AcesGlow10Fwd => Arc::new(AcesGlow {
            inverse: false,
            glow_gain: 0.05,
            glow_mid: 0.08,
        }),
        S::AcesGlow10Inv => Arc::new(AcesGlow {
            inverse: true,
            glow_gain: 0.05,
            glow_mid: 0.08,
        }),
        S::AcesDarkToDim10Fwd => Arc::new(AcesDarkToDim10::new(0.9811)),
        S::AcesDarkToDim10Inv => Arc::new(AcesDarkToDim10::new(1.0192640913260627)),
        S::AcesGamutComp13Fwd => Arc::new(AcesGamutComp13::new(data, false)),
        S::AcesGamutComp13Inv => Arc::new(AcesGamutComp13::new(data, true)),
        S::AcesOutputTransform20Fwd | S::AcesOutputTransform20Inv => {
            Arc::new(AcesOutputTransform20::new(data)?)
        }
        S::AcesRgbToJmh20 | S::AcesJmhToRgb20 => Arc::new(AcesRgbToJmh20::new(data)?),
        S::AcesRgbToHmj20 | S::AcesHmjToRgb20 => Arc::new(AcesRgbToHmj20::new(data)?),
        S::AcesTonescaleCompress20Fwd | S::AcesTonescaleCompress20Inv => {
            Arc::new(AcesTonescaleCompress20::new(data)?)
        }
        S::AcesGamutCompress20Fwd | S::AcesGamutCompress20Inv => {
            Arc::new(AcesGamutCompress20::new(data)?)
        }
        S::Rec2100SurroundFwd | S::Rec2100SurroundInv => Arc::new(Rec2100Surround::new(data)),
        S::RgbToHsv => Arc::new(RgbToHsv),
        S::HsvToRgb => Arc::new(HsvToRgb),
        S::XyzToXyy => Arc::new(XyzToXyy),
        S::XyyToXyz => Arc::new(XyyToXyz),
        S::XyzToUvy => Arc::new(XyzToUvy),
        S::UvyToXyz => Arc::new(UvyToXyz),
        S::XyzToLuv => Arc::new(XyzToLuv),
        S::LuvToXyz => Arc::new(LuvToXyz),
        S::LinToPq => Arc::new(LinToPq),
        S::PqToLin => Arc::new(PqToLin),
        S::LinToGammaLog => Arc::new(LinToGammaLog::new(data, false)),
        S::GammaLogToLin => Arc::new(LinToGammaLog::new(data, true)),
        S::LinToDoubleLog => Arc::new(LinToDoubleLog::new(data, false)),
        S::DoubleLogToLin => Arc::new(LinToDoubleLog::new(data, true)),
        S::RgbToHsyLog => Arc::new(RgbToHsy(HsyStyle::Log)),
        S::HsyLogToRgb => Arc::new(HsyToRgb(HsyStyle::Log)),
        S::RgbToHsyLin => Arc::new(RgbToHsy(HsyStyle::Lin)),
        S::HsyLinToRgb => Arc::new(HsyToRgb(HsyStyle::Lin)),
        S::RgbToHsyVid => Arc::new(RgbToHsy(HsyStyle::Vid)),
        S::HsyVidToRgb => Arc::new(HsyToRgb(HsyStyle::Vid)),
    })
}
