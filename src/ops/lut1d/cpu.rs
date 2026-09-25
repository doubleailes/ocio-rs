//! CPU renderers of the 1D LUT (port of the 32f-in / 32f-out scalar paths of
//! `Lut1DOpCPU.cpp`).

#![allow(clippy::needless_range_loop)]

use super::{lerpf, sanitize_float, std_max, std_min, ComponentProperties, Lut1DOpData};
use crate::ops::Pixel;
use crate::types::{Lut1DHueAdjust, TransformDirection};
use half::f16;

/// Compute the indices of the smallest, middle and largest elements of
/// `rgb` (port of `GamutMapUtils::Order3`). Returns `(min, mid, max)`.
///
/// NaN comparisons are always false; the table handles `{A, NaN, B}` with
/// `A > B`.
pub fn order3(rgb: &[f32; 3]) -> (usize, usize, usize) {
    //                          0  1  2  3  4  5  6  7  8  (typical val - 3)
    const TABLE: [usize; 12] = [2, 1, 0, 2, 1, 0, 2, 1, 2, 0, 1, 2];
    let val = ((rgb[0] > rgb[1]) as i32 * 5 + (rgb[1] > rgb[2]) as i32 * 4)
        - (rgb[0] > rgb[2]) as i32 * 3
        + 3;
    // (val is always in [0, 9], the clamp only guards the indexing.)
    let val = val.clamp(0, 9) as usize;
    (TABLE[val + 2], TABLE[val + 1], TABLE[val])
}

/// Apply the DW3 hue restoration: the middle channel keeps its relative
/// position between the min and max channels.
#[inline]
pub(crate) fn hue_restore(rgb: &[f32; 3], rgb2: &mut [f32; 3]) {
    let (min, mid, max) = order3(rgb);
    let orig_chroma = rgb[max] - rgb[min];
    let hue_factor = if orig_chroma == 0.0 {
        0.0
    } else {
        (rgb[mid] - rgb[min]) / orig_chroma
    };
    let new_chroma = rgb2[max] - rgb2[min];
    rgb2[mid] = hue_factor * new_chroma + rgb2[min];
}

/// Interpolation data for a half-domain LUT (port of `IndexPair`).
struct IndexPair {
    val_a: u16,
    val_b: u16,
    fraction: f32,
}

impl IndexPair {
    /// Port of `IndexPair::GetEdgeFloatValues`.
    fn edge_float_values(f_in: f32) -> IndexPair {
        let mut f_in = f_in;
        let mut half_val = f16::from_f32(f_in);
        if half_val.is_infinite() {
            half_val = if half_val.is_sign_negative() {
                -f16::MAX
            } else {
                f16::MAX
            };
            f_in = half_val.to_f32();
        }

        // Convert back to float to compare to f_in and interpolate both values.
        let float_temp = half_val.to_f32();

        let (val_a, val_b);
        // Strict comparison required otherwise negative fractions will occur.
        if float_temp.abs() > f_in.abs() {
            val_b = half_val.to_bits();
            val_a = val_b.wrapping_sub(1);
        } else {
            val_a = half_val.to_bits();
            let mut b = val_a.wrapping_add(1);
            let hb = f16::from_bits(b);
            if hb.is_infinite() {
                let hm = if hb.is_sign_negative() {
                    -f16::MAX
                } else {
                    f16::MAX
                };
                b = hm.to_bits();
                // Necessary to reset f_in too (consider f_in = 65519, it's
                // > HALF_MAX but not Inf).
                f_in = hm.to_f32();
            }
            val_b = b;
        }

        let fa = f16::from_bits(val_a).to_f32();
        let fb = f16::from_bits(val_b).to_f32();
        let mut fraction = (f_in - fa) / (fb - fa);
        if fraction.is_nan() {
            fraction = 0.0;
        }
        IndexPair {
            val_a,
            val_b,
            fraction,
        }
    }
}

/// Forward 1D LUT renderer (standard or half domain, with or without hue
/// adjust).
#[derive(Debug)]
pub(crate) struct ForwardRenderer {
    luts: [Vec<f32>; 3],
    step: f32,
    dim_minus_one: f32,
    half_domain: bool,
    hue_adjust: bool,
}

impl ForwardRenderer {
    fn new(lut: &Lut1DOpData) -> Self {
        // Output scaling for 32f is 1.
        Self::with_out_scale(lut, 1.0)
    }

    /// Renderer of a 32f input whose LUT values are scaled by `out_max`, the
    /// maximum value of the output bit depth (the scaling done by the OCIO
    /// renderers for an integer output; the result still has to be cast).
    pub(crate) fn with_out_scale(lut: &Lut1DOpData, out_max: f32) -> Self {
        let dim = lut.array().length();
        let values = lut.array().values();
        let make = |c: usize| {
            (0..dim)
                .map(|i| sanitize_float(values[i * 3 + c] * out_max))
                .collect::<Vec<f32>>()
        };
        Self {
            luts: [make(0), make(1), make(2)],
            step: (dim as f32 - 1.0) / 1.0,
            dim_minus_one: dim as f32 - 1.0,
            half_domain: lut.is_input_half_domain(),
            hue_adjust: lut.hue_adjust() != Lut1DHueAdjust::None,
        }
    }

    #[inline]
    fn interp(&self, c: usize, v: f32) -> f32 {
        let lut = &self.luts[c];
        let mut idx = self.step * v;
        // NaNs become 0.
        idx = std_min(std_max(0.0, idx), self.dim_minus_one);
        let low = idx.floor() as usize;
        // When idx is exactly an index, high == low and delta is 0.
        let high = idx.ceil() as usize;
        // Computing delta relative to high rather than low to save computing
        // (1-delta); interpolating with 1-fraction avoids 0 * Inf.
        let delta = high as f32 - idx;
        lerpf(lut[high], lut[low], delta)
    }

    #[inline]
    fn interp_half(&self, c: usize, v: f32) -> f32 {
        let lut = &self.luts[c];
        let p = IndexPair::edge_float_values(v);
        // Since fraction is in the domain [0, 1), interpolate using
        // 1-fraction in order to avoid cases like -/+Inf * 0.
        lerpf(
            lut[p.val_b as usize],
            lut[p.val_a as usize],
            1.0 - p.fraction,
        )
    }

    pub(crate) fn apply(&self, pixels: &mut [Pixel]) {
        for p in pixels.iter_mut() {
            let rgb = [p[0], p[1], p[2]];
            let mut rgb2 = if self.half_domain {
                [
                    self.interp_half(0, rgb[0]),
                    self.interp_half(1, rgb[1]),
                    self.interp_half(2, rgb[2]),
                ]
            } else {
                [
                    self.interp(0, rgb[0]),
                    self.interp(1, rgb[1]),
                    self.interp(2, rgb[2]),
                ]
            };
            if self.hue_adjust {
                hue_restore(&rgb, &mut rgb2);
            }
            p[0] = rgb2[0];
            p[1] = rgb2[1];
            p[2] = rgb2[2];
            // Alpha scaling is 1 for 32f.
        }
    }
}

/// Parameters of a color component for the inverse renderers (port of
/// `ComponentParams`, using indices instead of pointers).
#[derive(Debug, Clone, Copy, Default)]
struct ComponentParams {
    /// Which of the temporary LUTs to use.
    lut: usize,
    /// Start of the effective LUT data.
    start: usize,
    /// Difference between real and effective start of the LUT.
    start_offset: f32,
    /// End of the effective LUT data (inclusive).
    end: usize,
    /// Start for the negative part of a half-domain LUT.
    neg_start: usize,
    /// Start offset for the negative part of a half-domain LUT.
    neg_start_offset: f32,
    /// End for the negative part of a half-domain LUT.
    neg_end: usize,
    /// Flip the sign of the value to handle decreasing LUTs.
    flip_sign: f32,
    /// Point of switching from positive to negative half domain.
    bisect_point: f32,
}

impl ComponentParams {
    fn new(properties: &ComponentProperties, lut: usize, lut_zero_entry: f32) -> Self {
        Self {
            lut,
            start: properties.start_domain,
            start_offset: properties.start_domain as f32,
            end: properties.end_domain,
            neg_start: properties.neg_start_domain,
            neg_start_offset: properties.neg_start_domain as f32,
            neg_end: properties.neg_end_domain,
            flip_sign: if properties.is_increasing { 1.0 } else { -1.0 },
            bisect_point: lut_zero_entry,
        }
    }
}

/// `std::lower_bound` (libstdc++ algorithm): first index whose value does
/// not compare less than `val`.
#[inline]
fn lower_bound(slice: &[f32], val: f32) -> usize {
    let mut first = 0;
    let mut count = slice.len();
    while count > 0 {
        let step = count / 2;
        let it = first + step;
        if slice[it] < val {
            first = it + 1;
            count -= step + 1;
        } else {
            count = step;
        }
    }
    first
}

/// Common part of `FindLutInv` / `FindLutInvHalf`: returns the total index
/// (effective start offset included) and the interpolation delta.
#[inline]
fn find_lut_inv_parts(
    lut: &[f32],
    start: usize,
    start_offset: f32,
    end: usize,
    flip_sign: f32,
    val: f32,
) -> (f32, f32) {
    // Note that the LUT data from start to end must be in increasing order,
    // regardless of whether the original LUT was increasing or decreasing.

    // Clamp the value to the range of the LUT.
    let cv = std_min(std_max(val * flip_sign, lut[start]), lut[end]);

    // First entry >= cv, decremented unless cv == lut[start].
    let mut low = start + lower_bound(&lut[start..end], cv);
    if low > start {
        low -= 1;
    }
    let mut high = low;
    if high < end {
        high += 1;
    }

    // Fractional distance of cv between the adjacent LUT entries (flat spots
    // leave delta = 0).
    let mut delta = 0.0f32;
    if lut[high] > lut[low] {
        delta = (cv - lut[low]) / (lut[high] - lut[low]);
    }

    // Index difference from the effective start to low, corrected for the
    // fact that start is not the beginning of the LUT if it starts with a
    // flat spot.
    let inds = (low - start) as f32;
    (inds + start_offset, delta)
}

/// Inverse of a value resulting from linear interpolation in a standard
/// domain LUT (port of `FindLutInv`).
#[inline]
fn find_lut_inv(
    lut: &[f32],
    start: usize,
    start_offset: f32,
    end: usize,
    flip_sign: f32,
    scale: f32,
    val: f32,
) -> f32 {
    let (total_inds, delta) = find_lut_inv_parts(lut, start, start_offset, end, flip_sign, val);
    // Scale converts from units of [0,dim] to [0,outDepth].
    (total_inds + delta) * scale
}

/// Inverse of a value resulting from linear interpolation in a half domain
/// LUT (port of `FindLutInvHalf`).
#[inline]
fn find_lut_inv_half(
    lut: &[f32],
    start: usize,
    start_offset: f32,
    end: usize,
    flip_sign: f32,
    scale: f32,
    val: f32,
) -> f32 {
    let (total_inds, delta) = find_lut_inv_parts(lut, start, start_offset, end, flip_sign, val);
    // The entries of a half domain are not a constant distance apart, so
    // convert the indices (half codes) into floats to compute the distance
    // the delta factor is working over.
    let base = f16::from_bits(total_inds as u16).to_f32();
    let base_plus1 = f16::from_bits((total_inds + 1.0) as u16).to_f32();
    let domain = base + delta * (base_plus1 - base);
    domain * scale
}

/// Exact inverse 1D LUT renderer.
#[derive(Debug)]
pub(crate) struct InverseRenderer {
    luts: Vec<Vec<f32>>,
    params: [ComponentParams; 3],
    scale: f32,
    half_domain: bool,
    hue_adjust: bool,
}

impl InverseRenderer {
    fn new(lut: &Lut1DOpData) -> Self {
        let single = lut.has_single_lut();
        let dim = lut.array().length();
        let values = lut.array().values();
        let half_domain = lut.is_input_half_domain();
        let props = [
            *lut.red_properties(),
            *lut.green_properties(),
            *lut.blue_properties(),
        ];
        let num_luts = if single { 1 } else { 3 };

        // Since the inversion requires increasing arrays, a decreasing LUT is
        // negated (input scaling is 1 for 32f).
        let lut_scale = 1.0f32;
        let luts: Vec<Vec<f32>> = (0..num_luts)
            .map(|c| {
                let inc = props[c].is_increasing;
                (0..dim)
                    .map(|i| {
                        let v = values[i * 3 + c];
                        // For a half domain, the negative half is sign reversed.
                        let positive = !half_domain || i < 32768;
                        if inc == positive {
                            v * lut_scale
                        } else {
                            -v * lut_scale
                        }
                    })
                    .collect()
            })
            .collect();

        let zero = |c: usize| if half_domain { values[c] } else { 0.0 };
        let params_r = ComponentParams::new(&props[0], 0, zero(0));
        let params = if single {
            // All the parameters refer to the red LUT.
            [params_r; 3]
        } else {
            [
                params_r,
                ComponentParams::new(&props[1], 1, zero(1)),
                ComponentParams::new(&props[2], 2, zero(2)),
            ]
        };

        let out_max = 1.0f32;
        // For a half domain, the distance between adjacent entries is not
        // constant, so it cannot be rolled into the scale.
        let scale = if half_domain {
            out_max
        } else {
            out_max / (dim - 1) as f32
        };

        Self {
            luts,
            params,
            scale,
            half_domain,
            hue_adjust: lut.hue_adjust() != Lut1DHueAdjust::None,
        }
    }

    #[inline]
    fn inv(&self, c: usize, v: f32) -> f32 {
        let p = &self.params[c];
        find_lut_inv(
            &self.luts[p.lut],
            p.start,
            p.start_offset,
            p.end,
            p.flip_sign,
            self.scale,
            v,
        )
    }

    #[inline]
    fn inv_half(&self, c: usize, v: f32) -> f32 {
        let p = &self.params[c];
        let lut = &self.luts[p.lut];
        let is_increasing = p.flip_sign > 0.0;
        // Test the value against the bisect point to determine which half of
        // the float domain to do the inverse eval in.
        if is_increasing == (v >= p.bisect_point) {
            find_lut_inv_half(
                lut,
                p.start,
                p.start_offset,
                p.end,
                p.flip_sign,
                self.scale,
                v,
            )
        } else {
            // Note: as in OCIO, the blue channel uses the red flip sign for
            // the negative half of the domain.
            let flip = if c == 2 {
                self.params[0].flip_sign
            } else {
                p.flip_sign
            };
            find_lut_inv_half(
                lut,
                p.neg_start,
                p.neg_start_offset,
                p.neg_end,
                -flip,
                self.scale,
                v,
            )
        }
    }

    pub(crate) fn apply(&self, pixels: &mut [Pixel]) {
        for p in pixels.iter_mut() {
            let rgb = [p[0], p[1], p[2]];
            let mut rgb2 = if self.half_domain {
                [
                    self.inv_half(0, rgb[0]),
                    self.inv_half(1, rgb[1]),
                    self.inv_half(2, rgb[2]),
                ]
            } else {
                [
                    self.inv(0, rgb[0]),
                    self.inv(1, rgb[1]),
                    self.inv(2, rgb[2]),
                ]
            };
            if self.hue_adjust {
                hue_restore(&rgb, &mut rgb2);
            }
            p[0] = rgb2[0];
            p[1] = rgb2[1];
            p[2] = rgb2[2];
            // Alpha scaling is 1 for 32f.
        }
    }
}

/// A 1D LUT CPU renderer (32f in, 32f out).
#[derive(Debug)]
pub(crate) enum Lut1DRenderer {
    Forward(ForwardRenderer),
    Inverse(InverseRenderer),
}

impl Lut1DRenderer {
    /// Build the renderer of a finalized LUT (see `Lut1DOpData::finalize`).
    pub(crate) fn new(lut: &Lut1DOpData) -> Self {
        match lut.direction() {
            TransformDirection::Forward => Lut1DRenderer::Forward(ForwardRenderer::new(lut)),
            TransformDirection::Inverse => Lut1DRenderer::Inverse(InverseRenderer::new(lut)),
        }
    }

    pub(crate) fn apply(&self, pixels: &mut [Pixel]) {
        match self {
            Lut1DRenderer::Forward(r) => r.apply(pixels),
            Lut1DRenderer::Inverse(r) => r.apply(pixels),
        }
    }
}
