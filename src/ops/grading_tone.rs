//! Tonal grading op (port of `GradingTone.cpp`, `GradingToneOpData.cpp`,
//! `GradingToneOp.cpp`, `GradingToneOpCPU.cpp` and `GradingToneTransform.cpp`).

use super::grading_primary::{std_max, std_min, GradingValue};
use crate::config::Config;
use crate::context::Context;
use crate::dynamic_property::DynamicProperty;
use crate::error::Result;
use crate::format_metadata::FormatMetadata;
use crate::math_utils::clamp_f32;
use crate::ops::{Op, OpRc, OpVec, Pixel};
use crate::transforms::grading::{GradingRgbmsw, GradingTone};
use crate::transforms::{BuildOps, GradingToneTransform, Transform, Validate};
use crate::types::{DynamicPropertyType, GradingStyle, OptimizationFlags, TransformDirection};
use std::any::Any;
use std::fmt;
use std::sync::Arc;

/// Channel of a [`GradingRgbmsw`] (port of `RGBMChannel`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RgbmChannel {
    R = 0,
    G = 1,
    B = 2,
    M = 3,
}

const CHANNELS: [RgbmChannel; 4] = [
    RgbmChannel::R,
    RgbmChannel::G,
    RgbmChannel::B,
    RgbmChannel::M,
];

/// Value of a channel, as float (port of `GetChannelValue`).
pub fn channel_value(value: &GradingRgbmsw, channel: RgbmChannel) -> f32 {
    match channel {
        RgbmChannel::R => value.red as f32,
        RgbmChannel::G => value.green as f32,
        RgbmChannel::B => value.blue as f32,
        RgbmChannel::M => value.master as f32,
    }
}

// ---------------------------------------------------------------------------
// Precomputed values

/// Values precomputed from a [`GradingTone`] for rendering (port of
/// `GradingTonePreRender`).
#[derive(Debug, Clone, PartialEq)]
pub struct GradingTonePreRender {
    // Values used by CPU & GPU.
    pub shadows_start: f64,
    pub shadows_width: f64,
    pub highlights_start: f64,
    pub highlights_width: f64,
    pub blacks_start: f64,
    pub blacks_width: f64,
    pub whites_start: f64,
    pub whites_width: f64,

    // Arrays only used by CPU.
    pub mid_x: [[f32; 6]; 4],
    pub mid_y: [[f32; 6]; 4],
    pub mid_m: [[f32; 6]; 4],

    pub hs_x: [[[f32; 3]; 4]; 2],
    pub hs_y: [[[f32; 3]; 4]; 2],
    /// m1 not used, `hs_m[][][1]` is m2.
    pub hs_m: [[[f32; 2]; 4]; 2],

    pub wb_x: [[[f32; 2]; 4]; 2],
    pub wb_y: [[[f32; 2]; 4]; 2],
    pub wb_m: [[[f32; 2]; 4]; 2],
    pub wb_gain: [[f32; 4]; 2],

    /// Top/bottom, 4 values.
    pub sc_x: [[f32; 4]; 2],
    pub sc_y: [[f32; 4]; 2],
    /// m0 & m3.
    pub sc_m: [[f32; 2]; 2],

    // Values changing with the style.
    pub top: f32,
    pub top_sc: f32,
    pub bottom: f32,
    pub pivot: f32,

    /// Do not apply the op if all params are identity.
    pub local_bypass: bool,

    style: GradingStyle,
}

impl GradingTonePreRender {
    /// Precomputation object for a style (values are not computed yet, see
    /// [`update`](Self::update)).
    pub fn new(style: GradingStyle) -> Self {
        let mut p = Self {
            shadows_start: 0.0,
            shadows_width: 0.0,
            highlights_start: 0.0,
            highlights_width: 0.0,
            blacks_start: 0.0,
            blacks_width: 0.0,
            whites_start: 0.0,
            whites_width: 0.0,
            mid_x: [[0.0; 6]; 4],
            mid_y: [[0.0; 6]; 4],
            mid_m: [[0.0; 6]; 4],
            hs_x: [[[0.0; 3]; 4]; 2],
            hs_y: [[[0.0; 3]; 4]; 2],
            hs_m: [[[0.0; 2]; 4]; 2],
            wb_x: [[[0.0; 2]; 4]; 2],
            wb_y: [[[0.0; 2]; 4]; 2],
            wb_m: [[[0.0; 2]; 4]; 2],
            wb_gain: [[0.0; 4]; 2],
            sc_x: [[0.0; 4]; 2],
            sc_y: [[0.0; 4]; 2],
            sc_m: [[0.0; 2]; 2],
            top: 1.0,
            top_sc: 1.0,
            bottom: 0.0,
            pivot: 0.4,
            local_bypass: false,
            style: GradingStyle::Log,
        };
        p.set_style(style);
        p
    }

    /// Precomputed values of `v` for `style`.
    pub fn from_value(style: GradingStyle, v: &GradingTone) -> Self {
        let mut p = Self::new(style);
        p.update(v);
        p
    }

    /// Style dependent values: (top, topSC, bottom, pivot).
    pub fn from_style(style: GradingStyle) -> (f32, f32, f32, f32) {
        match style {
            // Might like to move these for ACES, but cannot for ARRI K1S1.
            GradingStyle::Log => (1.0, 1.0, 0.0, 0.4),
            // Bottom is placed at breakpoint of lin-to-log.
            GradingStyle::Lin => (7.5, 6.5, -5.5, 0.0),
            // aces 0.18 --> 0.39.
            GradingStyle::Video => (1.0, 1.0, 0.0, 0.4),
        }
    }

    /// Change the style.
    pub fn set_style(&mut self, style: GradingStyle) {
        if self.style != style {
            self.style = style;
            let (top, top_sc, bottom, pivot) = Self::from_style(style);
            self.top = top;
            self.top_sc = top_sc;
            self.bottom = bottom;
            self.pivot = pivot;
        }
    }

    /// Recompute the values.
    pub fn update(&mut self, v: &GradingTone) {
        self.local_bypass = v.is_identity();
        if self.local_bypass {
            return;
        }

        {
            let master = v.highlights.master;
            let start = v.highlights.start;
            let pivot = v.highlights.width;
            let startw = v.whites.start;
            let widthw = v.whites.width;

            self.highlights_start = if start > pivot - 0.01 {
                pivot - 0.01
            } else {
                start
            };
            self.highlights_width = pivot;

            let new_start =
                highlight_fwd_eval(startw, self.highlights_start, self.highlights_width, master);
            let new_end = highlight_fwd_eval(
                startw + widthw,
                self.highlights_start,
                self.highlights_width,
                master,
            );
            self.whites_start = new_start;
            self.whites_width = new_end - new_start;
        }
        {
            let master = v.shadows.master;
            let start = v.shadows.start;
            let pivot = v.shadows.width;
            let startb = v.blacks.start;
            let widthb = v.blacks.width;

            self.shadows_start = if start < pivot + 0.01 {
                pivot + 0.01
            } else {
                start
            };
            self.shadows_width = pivot;

            let new_start = shadow_fwd_eval(startb, self.shadows_width, self.shadows_start, master);
            let new_end = shadow_fwd_eval(
                startb - widthb,
                self.shadows_width,
                self.shadows_start,
                master,
            );
            self.blacks_start = new_start;
            self.blacks_width = new_start - new_end;
        }

        let (top, bottom, top_sc, pivot) = (self.top, self.bottom, self.top_sc, self.pivot);
        self.mids_precompute(v, top, bottom);
        self.highlight_shadow_precompute(v);
        self.white_black_precompute(v);
        self.scontrast_precompute(v, top_sc, bottom, pivot);
    }

    fn mids_precompute(&mut self, v: &GradingTone, top: f32, bottom: f32) {
        const HALO: f32 = 0.4;

        for channel in CHANNELS {
            let c = channel as usize;
            let mut mid_adj = clamp_f32(channel_value(&v.midtones, channel), 0.01, 1.99);
            if mid_adj == 1.0 {
                continue;
            }
            let x = &mut self.mid_x[c];
            let y = &mut self.mid_y[c];
            let m = &mut self.mid_m[c];

            x[0] = bottom;
            x[5] = top;

            let max_width = (x[5] - x[0]) * 0.95;
            let width = clamp_f32(v.midtones.width as f32, 0.01, max_width);
            let min_cent = x[0] + width * 0.51;
            let max_cent = x[5] - width * 0.51;
            let center = clamp_f32(v.midtones.start as f32, min_cent, max_cent);

            x[1] = center - width * 0.5;
            x[4] = x[1] + width;

            x[2] = x[1] + (x[4] - x[1]) * 0.25;
            x[3] = x[1] + (x[4] - x[1]) * 0.75;
            y[0] = x[0];
            m[0] = 1.0;
            m[5] = 1.0;

            let min_slope = 0.1f32;

            mid_adj -= 1.0;
            mid_adj *= 1.0 - min_slope;

            m[2] = 1.0 + mid_adj;
            m[3] = 1.0 - mid_adj;
            m[1] = 1.0 + mid_adj * HALO;
            m[4] = 1.0 - mid_adj * HALO;

            let (x0, x1, x2, x3, x4, x5) = (x[0], x[1], x[2], x[3], x[4], x[5]);
            if center <= (x5 + x0) * 0.5 {
                let (m0, m1, m2, m3, m5) = (m[0], m[1], m[2], m[3], m[5]);
                let area = (x1 - x0) * (m1 - m0) * 0.5
                    + (x2 - x1) * ((m1 - m0) + (m2 - m1) * 0.5)
                    + (center - x2) * (m2 - m0) * 0.5;
                m[4] = (-0.5 * (x5 - x4) * m5
                    + (x4 - x3) * (0.5 * m3 - m5)
                    + (x3 - center) * (m3 - m5) * 0.5
                    + area)
                    / (-0.5 * (x5 - x3));
            } else {
                let (m0, m2, m3, m4, m5) = (m[0], m[2], m[3], m[4], m[5]);
                let area = (x5 - x4) * (m4 - m5) * 0.5
                    + (x4 - x3) * ((m4 - m5) + (m3 - m4) * 0.5)
                    + (x3 - center) * (m3 - m5) * 0.5;
                m[1] = (-0.5 * (x1 - x0) * m0
                    + (x2 - x1) * (0.5 * m2 - m0)
                    + (center - x2) * (m2 - m0) * 0.5
                    + area)
                    / (-0.5 * (x2 - x0));
            }

            y[1] = y[0] + (m[0] + m[1]) * (x1 - x0) * 0.5;
            y[2] = y[1] + (m[1] + m[2]) * (x2 - x1) * 0.5;
            y[3] = y[2] + (m[2] + m[3]) * (x3 - x2) * 0.5;
            y[4] = y[3] + (m[3] + m[4]) * (x4 - x3) * 0.5;
            y[5] = y[4] + (m[4] + m[5]) * (x5 - x4) * 0.5;
        }
    }

    fn highlight_shadow_precompute(&mut self, v: &GradingTone) {
        for is_shadow in [false, true] {
            let s = is_shadow as usize;
            for channel in CHANNELS {
                let c = channel as usize;
                let mut val = if is_shadow {
                    channel_value(&v.shadows, channel)
                } else {
                    channel_value(&v.highlights, channel)
                };
                if !is_shadow {
                    val = 2.0 - val;
                }
                if val == 1.0 {
                    continue;
                }
                let start = (if is_shadow {
                    self.shadows_start
                } else {
                    self.highlights_start
                }) as f32;
                let pivot = (if is_shadow {
                    self.shadows_width
                } else {
                    self.highlights_width
                }) as f32;

                let x0 = if is_shadow { pivot } else { start };
                let x2 = if is_shadow { start } else { pivot };
                let y0 = x0;
                let y2 = x2;
                let x1 = x0 + (x2 - x0) * 0.5;
                self.hs_x[s][c] = [x0, x1, x2];
                self.hs_y[s][c][0] = y0;
                self.hs_y[s][c][2] = y2;

                if val < 1.0 {
                    let m0 = if is_shadow { std_max(0.01, val) } else { 1.0 };
                    let m2 = if is_shadow { 1.0 } else { std_max(0.01, val) };
                    self.hs_m[s][c] = [m0, m2];
                    self.hs_y[s][c][1] = (0.5 / (x2 - x0))
                        * ((2.0 * y0 + m0 * (x1 - x0)) * (x2 - x1)
                            + (2.0 * y2 - m2 * (x2 - x1)) * (x1 - x0));
                } else if val > 1.0 {
                    let m0 = if is_shadow {
                        std_max(0.01, 2.0 - val)
                    } else {
                        1.0
                    };
                    let m2 = if is_shadow {
                        1.0
                    } else {
                        std_max(0.01, 2.0 - val)
                    };
                    self.hs_m[s][c] = [m0, m2];
                    self.hs_y[s][c][1] = (0.5 / ((x2 - x1) + (x1 - x0)))
                        * ((2.0 * y0 + m0 * (x1 - x0)) * (x2 - x1)
                            + (2.0 * y2 - m2 * (x2 - x1)) * (x1 - x0));
                }
            }
        }
    }

    fn white_black_precompute(&mut self, v: &GradingTone) {
        for is_black in [false, true] {
            let b = is_black as usize;
            for channel in CHANNELS {
                let c = channel as usize;
                let start = (if is_black {
                    self.blacks_start
                } else {
                    self.whites_start
                }) as f32;
                let width = (if is_black {
                    self.blacks_width
                } else {
                    self.whites_width
                }) as f32;

                let val = if is_black {
                    channel_value(&v.blacks, channel)
                } else {
                    channel_value(&v.whites, channel)
                };

                let x0 = if !is_black { start } else { start - width };
                let x1 = if !is_black { x0 + width } else { start };
                self.wb_x[b][c] = [x0, x1];

                let mtest = if !is_black { val } else { 2.0 - val };

                if mtest < 1.0 {
                    // Slope is decreasing case.
                    if !is_black {
                        let m0 = 1.0;
                        let m1 = std_max(0.01, val);
                        let y0 = x0;
                        let y1 = y0 + (m0 + m1) * (x1 - x0) * 0.5;
                        self.wb_m[b][c] = [m0, m1];
                        self.wb_y[b][c] = [y0, y1];
                    } else {
                        let m0 = std_max(0.01, 2.0 - val);
                        let m1 = 1.0;
                        let y1 = x1;
                        let y0 = y1 - (m0 + m1) * (x1 - x0) * 0.5;
                        self.wb_m[b][c] = [m0, m1];
                        self.wb_y[b][c] = [y0, y1];
                    }
                } else if mtest > 1.0 {
                    // Slope is increasing case.
                    let (m0, m1);
                    if !is_black {
                        m0 = 1.0;
                        m1 = std_max(0.01, 2.0 - val);
                        // y1 won't be used.
                        self.wb_y[b][c][0] = x0;
                    } else {
                        m0 = std_max(0.01, val);
                        m1 = 1.0;
                        let y1 = x1;
                        let y0 = y1 - (m0 + m1) * (x1 - x0) * 0.5;
                        self.wb_y[b][c] = [y0, y1];
                    }
                    self.wb_m[b][c] = [m0, m1];
                    self.wb_gain[b][c] = (m0 + m1) * 0.5;
                }
            }
        }
    }

    fn scontrast_precompute(&mut self, v: &GradingTone, top_sc: f32, bottom: f32, pivot: f32) {
        let mut contrast = v.s_contrast as f32;
        if contrast == 1.0 {
            return;
        }
        // Limit the range of values to prevent reversals.
        contrast = if contrast > 1.0 {
            1.0 / (1.8125 - 0.8125 * std_min(contrast, 1.99))
        } else {
            0.28125 + 0.71875 * std_max(contrast, 0.01)
        };

        // Top end.
        {
            let x3 = top_sc;
            let y3 = top_sc;
            let y0 = pivot + (y3 - pivot) * 0.25;
            let m0 = contrast;
            let x0 = pivot + (y0 - pivot) / m0;
            let min_width = (x3 - x0) * 0.3;
            let mut m3 = 1.0 / m0;
            // NB: Due to the if (contrast != 1.) clause above, m0 != m3.
            let center = (y3 - y0 - m3 * x3 + m0 * x0) / (m0 - m3);
            let mut x1 = x0;
            let mut x2 = 2.0 * center - x1;
            if x2 > x3 {
                x2 = x3;
                x1 = 2.0 * center - x2;
            } else if (x2 - x1) < min_width {
                x2 = x1 + min_width;
                let new_center = (x2 + x1) * 0.5;
                m3 = (y3 - y0 + m0 * x0 - new_center * m0) / (x3 - new_center);
            }
            let y1 = y0;
            let y2 = y1 + (m0 + m3) * (x2 - x1) * 0.5;
            self.sc_x[0] = [x0, x1, x2, x3];
            self.sc_y[0] = [y0, y1, y2, y3];
            self.sc_m[0] = [m0, m3];
        }

        // Bottom end.
        {
            let x0 = bottom;
            let y0 = bottom;
            let y3 = pivot - (pivot - y0) * 0.25;
            let m3 = contrast;
            let x3 = pivot - (pivot - y3) / m3;
            let min_width = (x3 - x0) * 0.3;
            let mut m0 = 1.0 / m3;
            let center = (y3 - y0 - m3 * x3 + m0 * x0) / (m0 - m3);
            let mut x2 = x3;
            let mut x1 = 2.0 * center - x2;
            if x1 < x0 {
                x1 = x0;
                x2 = 2.0 * center - x1;
            } else if (x2 - x1) < min_width {
                x1 = x2 - min_width;
                let new_center = (x2 + x1) * 0.5;
                m0 = (y3 - y0 - m3 * x3 + new_center * m3) / (new_center - x0);
            }
            let y2 = y3;
            let y1 = y2 - (m0 + m3) * (x2 - x1) * 0.5;
            self.sc_x[1] = [x0, x1, x2, x3];
            self.sc_y[1] = [y0, y1, y2, y3];
            self.sc_m[1] = [m0, m3];
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn faux_cubic_fwd_eval(
    t: f64,
    x0: f64,
    x2: f64,
    y0: f64,
    y2: f64,
    m0: f64,
    m2: f64,
    x1: f64,
) -> f64 {
    let y1 = (0.5 / ((x2 - x1) + (x1 - x0)))
        * ((2. * y0 + m0 * (x1 - x0)) * (x2 - x1) + (2. * y2 - m2 * (x2 - x1)) * (x1 - x0));

    let tl = (t - x0) / (x1 - x0);
    let tr = (t - x1) / (x2 - x1);
    let fl = y0 * (1. - tl * tl) + y1 * tl * tl + m0 * (1. - tl) * tl * (x1 - x0);
    let fr = y1 * (1. - tr) * (1. - tr) + y2 * (2. - tr) * tr + m2 * (tr - 1.) * tr * (x2 - x1);

    let mut res = if t < x1 { fl } else { fr };
    if t < x0 {
        res = y0 + (t - x0) * m0;
    }
    if t > x2 {
        res = y2 + (t - x2) * m2;
    }
    res
}

#[allow(clippy::too_many_arguments)]
fn faux_cubic_rev_eval(
    t: f64,
    x0: f64,
    x2: f64,
    y0: f64,
    y2: f64,
    m0: f64,
    m2: f64,
    x1: f64,
) -> f64 {
    let y1 = (0.5 / ((x2 - x1) + (x1 - x0)))
        * ((2. * y0 + m0 * (x1 - x0)) * (x2 - x1) + (2. * y2 - m2 * (x2 - x1)) * (x1 - x0));

    let cl = y0 - t;
    let bl = m0 * (x1 - x0);
    let al = y1 - y0 - m0 * (x1 - x0);
    let discrim_l = (bl * bl - 4. * al * cl).sqrt();
    let tmp_l = (2. * cl) / (-discrim_l - bl);
    let out_l = tmp_l * (x1 - x0) + x0;

    let cr = y1 - t;
    let br = 2. * y2 - 2. * y1 - m2 * (x2 - x1);
    let ar = y1 - y2 + m2 * (x2 - x1);
    let discrim_r = (br * br - 4. * ar * cr).sqrt();
    let tmp_r = (2. * cr) / (-discrim_r - br);
    let out_r = tmp_r * (x2 - x1) + x1;

    let mut res = if t < y1 { out_l } else { out_r };
    if t < y0 {
        res = x0 + (t - y0) / m0;
    }
    if t > y2 {
        res = x2 + (t - y2) / m2;
    }
    res
}

fn highlight_fwd_eval(t: f64, start: f64, pivot: f64, val: f64) -> f64 {
    let x0 = start;
    let x2 = pivot;
    let y0 = x0;
    let y2 = x2;
    let m0 = 1.;
    let x1 = x0 + (x2 - x0) * 0.5;
    let val = 2. - val;
    if val <= 1. {
        let m2 = if val < 0.01 { 0.01 } else { val };
        faux_cubic_fwd_eval(t, x0, x2, y0, y2, m0, m2, x1)
    } else {
        let m2 = if 2. - val < 0.01 { 0.01 } else { 2. - val };
        faux_cubic_rev_eval(t, x0, x2, y0, y2, m0, m2, x1)
    }
}

fn shadow_fwd_eval(t: f64, start: f64, pivot: f64, val: f64) -> f64 {
    let x0 = start;
    let x2 = pivot;
    let y0 = x0;
    let y2 = x2;
    let m2 = 1.;
    let x1 = x0 + (x2 - x0) * 0.5;
    if val <= 1. {
        let m0 = if val < 0.01 { 0.01 } else { val };
        faux_cubic_fwd_eval(t, x0, x2, y0, y2, m0, m2, x1)
    } else {
        let m0 = if 2. - val < 0.01 { 0.01 } else { 2. - val };
        faux_cubic_rev_eval(t, x0, x2, y0, y2, m0, m2, x1)
    }
}

// ---------------------------------------------------------------------------
// CPU rendering

/// `val < limit ? below : above`.
#[inline]
fn on_limit(val: f32, limit: f32, below: f32, above: f32) -> f32 {
    if val < limit {
        below
    } else {
        above
    }
}

/// Channels processed by a zone function: one channel, or R, G & B for the
/// master channel (the computations are component-wise).
#[inline]
fn channel_range(channel: RgbmChannel) -> std::ops::Range<usize> {
    if channel == RgbmChannel::M {
        0..3
    } else {
        let c = channel as usize;
        c..c + 1
    }
}

fn mids_fwd(v: &GradingTone, vpr: &GradingTonePreRender, channel: RgbmChannel, out: &mut Pixel) {
    let mid_adj = clamp_f32(channel_value(&v.midtones, channel), 0.01, 1.99);
    if mid_adj == 1.0 {
        return;
    }
    let c = channel as usize;
    let [x0, x1, x2, x3, x4, x5] = vpr.mid_x[c];
    let [y0, y1, y2, y3, y4, y5] = vpr.mid_y[c];
    let [m0, m1, m2, m3, m4, m5] = vpr.mid_m[c];

    let eval = |t: f32| {
        let tl = (t - x0) / (x1 - x0);
        let tm = (t - x1) / (x2 - x1);
        let tr = (t - x2) / (x3 - x2);
        let tr2 = (t - x3) / (x4 - x3);
        let tr3 = (t - x4) / (x5 - x4);

        let fl = tl * (x1 - x0) * (tl * 0.5 * (m1 - m0) + m0) + y0;
        let fm = tm * (x2 - x1) * (tm * 0.5 * (m2 - m1) + m1) + y1;
        let fr = tr * (x3 - x2) * (tr * 0.5 * (m3 - m2) + m2) + y2;
        let fr2 = tr2 * (x4 - x3) * (tr2 * 0.5 * (m4 - m3) + m3) + y3;
        let fr3 = tr3 * (x5 - x4) * (tr3 * 0.5 * (m5 - m4) + m4) + y4;
        (fl, fm, fr, fr2, fr3)
    };

    if channel != RgbmChannel::M {
        let t = out[c];
        let (fl, fm, fr, fr2, fr3) = eval(t);
        let mut res = if t < x1 { fl } else { fm };
        if t > x2 {
            res = fr;
        }
        if t > x3 {
            res = fr2;
        }
        if t > x4 {
            res = fr3;
        }
        if t < x0 {
            res = y0 + (t - x0) * m0;
        }
        if t > x5 {
            res = y5 + (t - x5) * m5;
        }
        out[c] = res;
    } else {
        for o in out.iter_mut().take(3) {
            let t = *o;
            let (fl, fm, fr, fr2, fr3) = eval(t);
            let fr4 = (t - x0) * m0 + y0;
            let fr5 = (t - x5) * m5 + y5;
            let mut res = on_limit(t, x1, fl, fm);
            res = on_limit(t, x2, res, fr);
            res = on_limit(t, x3, res, fr2);
            res = on_limit(t, x4, res, fr3);
            res = on_limit(t, x0, fr4, res);
            res = on_limit(t, x5, res, fr5);
            *o = res;
        }
    }
}

fn mids_rev(v: &GradingTone, vpr: &GradingTonePreRender, channel: RgbmChannel, out: &mut Pixel) {
    let mid_adj = clamp_f32(channel_value(&v.midtones, channel), 0.01, 1.99);
    if mid_adj == 1.0 {
        return;
    }
    let c = channel as usize;
    let [x0, x1, x2, x3, x4, x5] = vpr.mid_x[c];
    let [y0, y1, y2, y3, y4, y5] = vpr.mid_y[c];
    let [m0, m1, m2, m3, m4, m5] = vpr.mid_m[c];

    // Inverse of the quadratic segment starting at (xa, ya).
    let seg = |t: f32, xa: f32, xb: f32, ya: f32, ma: f32, mb: f32| {
        let cc = ya - t;
        let b = ma * (xb - xa);
        let a = 0.5 * (mb - ma) * (xb - xa);
        let discrim = (b * b - 4.0 * a * cc).sqrt();
        let tmp = (2.0 * cc) / (-discrim - b);
        tmp * (xb - xa) + xa
    };

    if channel != RgbmChannel::M {
        let t = out[c];
        let res = if t >= y5 {
            x0 + (t - y0) / m0
        } else if t >= y4 {
            seg(t, x4, x5, y4, m4, m5)
        } else if t >= y3 {
            seg(t, x3, x4, y3, m3, m4)
        } else if t >= y2 {
            seg(t, x2, x3, y2, m2, m3)
        } else if t >= y1 {
            seg(t, x1, x2, y1, m1, m2)
        } else if t >= y0 {
            seg(t, x0, x1, y0, m0, m1)
        } else {
            x0 + (t - y0) / m0
        };
        out[c] = res;
    } else {
        // Same computations as OCIO's float3 version ((2c) / (-b - discrim)).
        let seg3 = |t: f32, xa: f32, xb: f32, ya: f32, ma: f32, mb: f32| {
            let cc = ya - t;
            let b = ma * (xb - xa);
            let a = 0.5 * (mb - ma) * (xb - xa);
            let discrim = (b * b - 4.0 * a * cc).sqrt();
            let tmp = (2.0 * cc) / (-b - discrim);
            tmp * (xb - xa) + xa
        };
        for o in out.iter_mut().take(3) {
            let t = *o;
            let out_r4 = x5 + (t - y5) / m5;
            let out_r3 = seg3(t, x4, x5, y4, m4, m5);
            let out_r2 = seg3(t, x3, x4, y3, m3, m4);
            let out_r = seg3(t, x2, x3, y2, m2, m3);
            let out_m = seg3(t, x1, x2, y1, m1, m2);
            let out_l = seg3(t, x0, x1, y0, m0, m1);
            let out_l0 = x0 + (t - y0) / m0;

            let mut res = on_limit(t, y1, out_l, out_m);
            res = on_limit(t, y2, res, out_r);
            res = on_limit(t, y3, res, out_r2);
            res = on_limit(t, y4, res, out_r3);
            res = on_limit(t, y0, out_l0, res);
            res = on_limit(t, y5, res, out_r4);
            *o = res;
        }
    }
}

#[allow(clippy::too_many_arguments)]
#[inline]
fn compute_hs_fwd(
    x0: f32,
    x1: f32,
    x2: f32,
    y0: f32,
    y1: f32,
    y2: f32,
    m0: f32,
    m2: f32,
    t: f32,
) -> f32 {
    let tl = (t - x0) / (x1 - x0);
    let tr = (t - x1) / (x2 - x1);
    let fl = y0 * (1.0 - tl * tl) + y1 * tl * tl + m0 * (1.0 - tl) * tl * (x1 - x0);
    let fr = y1 * (1.0 - tr) * (1.0 - tr) + y2 * (2.0 - tr) * tr + m2 * (tr - 1.0) * tr * (x2 - x1);

    let mut res = on_limit(t, x1, fl, fr);
    let r0 = (t - x0) * m0 + y0;
    res = on_limit(t, x0, r0, res);
    let r2 = (t - x2) * m2 + y2;
    on_limit(t, x2, res, r2)
}

#[allow(clippy::too_many_arguments)]
#[inline]
fn compute_hs_rev(
    x0: f32,
    x1: f32,
    x2: f32,
    y0: f32,
    y1: f32,
    y2: f32,
    m0: f32,
    m2: f32,
    t: f32,
) -> f32 {
    let bl = m0 * (x1 - x0);
    let al = y1 - y0 - m0 * (x1 - x0);
    let cl = y0 - t;
    let discrim_l = (bl * bl - 4.0 * al * cl).sqrt();
    let out_l = (-2.0 * cl) / (discrim_l + bl) * (x1 - x0) + x0;
    let br = 2.0 * y2 - 2.0 * y1 - m2 * (x2 - x1);
    let ar = y1 - y2 + m2 * (x2 - x1);
    let cr = y1 - t;
    let discrim_r = (br * br - 4.0 * ar * cr).sqrt();
    let out_r = (-2.0 * cr) / (discrim_r + br) * (x2 - x1) + x1;

    let mut res = on_limit(t, y1, out_l, out_r);
    let r0 = (t - y0) / m0 + x0;
    res = on_limit(t, y0, r0, res);
    let r2 = (t - y2) / m2 + x2;
    on_limit(t, y2, res, r2)
}

fn highlight_shadow(
    v: &GradingTone,
    vpr: &GradingTonePreRender,
    channel: RgbmChannel,
    is_shadow: bool,
    reverse: bool,
    out: &mut Pixel,
) {
    // The effect of val is symmetric around 1 (<1 uses Fwd algorithm, >1 uses Rev algorithm).
    let mut val = if is_shadow {
        channel_value(&v.shadows, channel)
    } else {
        channel_value(&v.highlights, channel)
    };
    if !is_shadow {
        val = 2.0 - val;
    }
    if val == 1.0 {
        return;
    }
    let s = is_shadow as usize;
    let c = channel as usize;
    let [x0, x1, x2] = vpr.hs_x[s][c];
    let [y0, y1, y2] = vpr.hs_y[s][c];
    let [m0, m2] = vpr.hs_m[s][c];

    let use_fwd = (val < 1.0) != reverse;
    for i in channel_range(channel) {
        let t = out[i];
        out[i] = if use_fwd {
            compute_hs_fwd(x0, x1, x2, y0, y1, y2, m0, m2, t)
        } else {
            compute_hs_rev(x0, x1, x2, y0, y1, y2, m0, m2, t)
        };
    }
}

#[allow(clippy::too_many_arguments)]
fn compute_wb_fwd(
    is_black: bool,
    val: f32,
    x0: f32,
    x1: f32,
    y0: f32,
    y1: f32,
    m0: f32,
    m1: f32,
    gain: f32,
    t: f32,
) -> Option<f32> {
    let mtest = if !is_black { val } else { 2.0 - val };

    if mtest < 1.0 {
        // Slope is decreasing case.
        let tlocal = (t - x0) / (x1 - x0);
        let mut res = tlocal * (x1 - x0) * (tlocal * 0.5 * (m1 - m0) + m0) + y0;
        let res0 = y0 + (t - x0) * m0;
        res = on_limit(t, x0, res0, res);
        let res1 = y1 + (t - x1) * m1;
        Some(on_limit(t, x1, res, res1))
    } else if mtest > 1.0 {
        // Slope is increasing case.
        let mut t = if !is_black {
            (t - x0) * gain + x0
        } else {
            (t - x1) * gain + x1
        };

        let a = 0.5 * (m1 - m0) * (x1 - x0);
        let b = m0 * (x1 - x0);

        let c = y0 - t;
        let discrim = (b * b - 4.0 * a * c).sqrt();
        let tmp = (-2.0 * c) / (discrim + b);
        let mut res = tmp * (x1 - x0) + x0;
        let res0 = x0 + (t - y0) / m0;
        res = on_limit(t, y0, res0, res);

        if !is_black {
            res = (res - x0) / gain + x0;
            // Quadratic extrapolation for better HDR control.
            let new_y1 = (x1 - x0) / gain + x0;
            let xd = x0 + (x1 - x0) * 0.99;
            let mut md = m0 + (xd - x0) * (m1 - m0) / (x1 - x0);
            md = 1.0 / md;
            let aa = 0.5 * (1.0 / m1 - md) / (x1 - xd);
            let bb = 1.0 / m1 - 2.0 * aa * x1;
            let cc = new_y1 - bb * x1 - aa * x1 * x1;
            t = (t - x0) / gain + x0;

            let res1 = (aa * t + bb) * t + cc;
            res = on_limit(t, x1, res, res1);
        } else {
            let res1 = x1 + (t - y1) / m1;
            res = on_limit(t, y1, res, res1);
            res = (res - x1) / gain + x1;
        }
        Some(res)
    } else {
        None
    }
}

#[allow(clippy::too_many_arguments)]
fn compute_wb_rev(
    is_black: bool,
    val: f32,
    x0: f32,
    x1: f32,
    y0: f32,
    y1: f32,
    m0: f32,
    m1: f32,
    gain: f32,
    t: f32,
) -> Option<f32> {
    let mtest = if !is_black { val } else { 2.0 - val };

    if mtest < 1.0 {
        // Slope is decreasing case.
        let a = 0.5 * (m1 - m0) * (x1 - x0);
        let b = m0 * (x1 - x0);

        let c = y0 - t;
        let discrim = (b * b - 4.0 * a * c).sqrt();
        let tmp = (-2.0 * c) / (discrim + b);
        let mut res = tmp * (x1 - x0) + x0;
        let res0 = x0 + (t - y0) / m0;
        res = on_limit(t, y0, res0, res);

        let res1 = x1 + (t - y1) / m1;
        Some(on_limit(t, y1, res, res1))
    } else if mtest > 1.0 {
        // Slope is increasing case.
        let mut t = if !is_black {
            (t - x0) * gain + x0
        } else {
            (t - x1) * gain + x1
        };

        let tlocal = (t - x0) / (x1 - x0);
        let mut res = tlocal * (x1 - x0) * (tlocal * 0.5 * (m1 - m0) + m0) + y0;
        let res0 = y0 + (t - x0) * m0;
        res = on_limit(t, x0, res0, res);

        if !is_black {
            res = (res - x0) / gain + x0;
            // Quadratic extrapolation for better HDR control.
            let new_y1 = (x1 - x0) / gain + x0;
            let xd = x0 + (x1 - x0) * 0.99;
            let mut md = m0 + (xd - x0) * (m1 - m0) / (x1 - x0);
            md = 1.0 / md;
            let aa = 0.5 * (1.0 / m1 - md) / (x1 - xd);
            let bb = 1.0 / m1 - 2.0 * aa * x1;
            let cc = new_y1 - bb * x1 - aa * x1 * x1;
            t = (t - x0) / gain + x0;

            let c = cc - t;
            let discrim = (bb * bb - 4.0 * aa * c).sqrt();
            let res1 = (-2.0 * c) / (discrim + bb);
            let brk = (aa * x1 + bb) * x1 + cc;
            res = on_limit(t, brk, res, res1);
        } else {
            let res1 = y1 + (t - x1) * m1;
            res = on_limit(t, x1, res, res1);
            res = (res - x1) / gain + x1;
        }
        Some(res)
    } else {
        None
    }
}

fn white_black(
    v: &GradingTone,
    vpr: &GradingTonePreRender,
    channel: RgbmChannel,
    is_black: bool,
    reverse: bool,
    out: &mut Pixel,
) {
    let val = if is_black {
        channel_value(&v.blacks, channel)
    } else {
        channel_value(&v.whites, channel)
    };
    let b = is_black as usize;
    let c = channel as usize;
    let [x0, x1] = vpr.wb_x[b][c];
    let [y0, y1] = vpr.wb_y[b][c];
    let [m0, m1] = vpr.wb_m[b][c];
    let gain = vpr.wb_gain[b][c];

    for i in channel_range(channel) {
        let t = out[i];
        let res = if reverse {
            compute_wb_rev(is_black, val, x0, x1, y0, y1, m0, m1, gain, t)
        } else {
            compute_wb_fwd(is_black, val, x0, x1, y0, y1, m0, m1, gain, t)
        };
        if let Some(r) = res {
            out[i] = r;
        }
    }
}

fn scontrast_contrast(v: &GradingTone) -> Option<f32> {
    let contrast = v.s_contrast as f32;
    if contrast == 1.0 {
        return None;
    }
    // Limit the range of values to prevent reversals.
    Some(if contrast > 1.0 {
        1.0 / (1.8125 - 0.8125 * std_min(contrast, 1.99))
    } else {
        0.28125 + 0.71875 * std_max(contrast, 0.01)
    })
}

fn scontrast_fwd(v: &GradingTone, vpr: &GradingTonePreRender, out: &mut Pixel) {
    let Some(contrast) = scontrast_contrast(v) else {
        return;
    };
    for o in out.iter_mut().take(3) {
        let t = *o;
        let mut out_color = (t - vpr.pivot) * contrast + vpr.pivot;

        // Top end.
        {
            let x1 = vpr.sc_x[0][1];
            let x2 = vpr.sc_x[0][2];
            let y1 = vpr.sc_y[0][1];
            let y2 = vpr.sc_y[0][2];
            let m0 = vpr.sc_m[0][0];
            let m3 = vpr.sc_m[0][1];

            let tr = (t - x1) / (x2 - x1);
            let res = tr * (x2 - x1) * (tr * 0.5 * (m3 - m0) + m0) + y1;
            out_color = on_limit(t, x1, out_color, res);
            let res2 = y2 + (t - x2) * m3;
            out_color = on_limit(t, x2, out_color, res2);
        }

        // Bottom end.
        {
            let x1 = vpr.sc_x[1][1];
            let x2 = vpr.sc_x[1][2];
            let y1 = vpr.sc_y[1][1];
            let m0 = vpr.sc_m[1][0];
            let m3 = vpr.sc_m[1][1];

            let tr = (t - x1) / (x2 - x1);
            let res = tr * (x2 - x1) * (tr * 0.5 * (m3 - m0) + m0) + y1;
            out_color = on_limit(t, x2, res, out_color);
            let res1 = y1 + (t - x1) * m0;
            out_color = on_limit(t, x1, res1, out_color);
        }
        *o = out_color;
    }
}

fn scontrast_rev(v: &GradingTone, vpr: &GradingTonePreRender, out: &mut Pixel) {
    let Some(contrast) = scontrast_contrast(v) else {
        return;
    };
    for o in out.iter_mut().take(3) {
        let t = *o;
        let mut out_color = (t - vpr.pivot) / contrast + vpr.pivot;

        // Top end.
        {
            let x1 = vpr.sc_x[0][1];
            let x2 = vpr.sc_x[0][2];
            let y1 = vpr.sc_y[0][1];
            let y2 = vpr.sc_y[0][2];
            let m0 = vpr.sc_m[0][0];
            let m3 = vpr.sc_m[0][1];

            let b = m0 * (x2 - x1);
            let a = (m3 - m0) * 0.5 * (x2 - x1);
            let c = y1 - t;
            let discrim = (b * b - 4.0 * a * c).sqrt();
            let res = (x2 - x1) * (-2.0 * c) / (discrim + b) + x1;

            out_color = on_limit(t, y1, out_color, res);
            out_color = on_limit(t, y2, out_color, x2 + (t - y2) / m3);
        }

        // Bottom end.
        {
            let x1 = vpr.sc_x[1][1];
            let x2 = vpr.sc_x[1][2];
            let y1 = vpr.sc_y[1][1];
            let y2 = vpr.sc_y[1][2];
            let m0 = vpr.sc_m[1][0];
            let m3 = vpr.sc_m[1][1];

            let b = m0 * (x2 - x1);
            let a = (m3 - m0) * 0.5 * (x2 - x1);
            let c = y1 - t;
            let discrim = (b * b - 4.0 * a * c).sqrt();
            let res = (x2 - x1) * (-2.0 * c) / (discrim + b) + x1;

            out_color = on_limit(t, y2, res, out_color);
            out_color = on_limit(t, y1, x1 + (t - y1) / m0, out_color);
        }
        *o = out_color;
    }
}

#[inline]
fn clamp_max_rgb(out: &mut Pixel) {
    // The grading controls at high values are able to push values above the max
    // half-float at which point they overflow to infinity.
    for o in out.iter_mut().take(3) {
        *o = std_min(*o, 65504.0);
    }
}

/// Constants of the lin-to-log conversion used by the linear style (shared
/// with the RGB curve and hue curve ops).
#[allow(clippy::excessive_precision)]
pub(crate) mod log_lin {
    pub const XBRK: f32 = 0.0041318374739483946;
    pub const SHIFT: f32 = -0.000157849851665374;
    pub const M: f32 = 1.0 / (0.18 + SHIFT);
    pub const GAIN: f32 = 363.034608563;
    pub const OFFS: f32 = -7.0;
    pub const YBRK: f32 = -5.5;
    /// 1/log(2).
    pub const BASE2: f32 = std::f32::consts::LOG2_E;

    /// Lin to log of one value.
    #[inline]
    pub fn lin_log(v: f32) -> f32 {
        if v < XBRK {
            v * GAIN + OFFS
        } else {
            BASE2 * ((v + SHIFT) * M).ln()
        }
    }

    /// Log to lin of one value.
    #[inline]
    pub fn log_lin(v: f32) -> f32 {
        if v < YBRK {
            (v - OFFS) / GAIN
        } else {
            2.0f32.powf(v) * (0.18 + SHIFT) - SHIFT
        }
    }
}

use RgbmChannel::{B, G, M, R};

fn apply_fwd_pixel(v: &GradingTone, vpr: &GradingTonePreRender, out: &mut Pixel) {
    for ch in [R, G, B, M] {
        mids_fwd(v, vpr, ch, out);
    }
    for ch in [R, G, B, M] {
        highlight_shadow(v, vpr, ch, false, false, out);
    }
    for ch in [R, G, B, M] {
        white_black(v, vpr, ch, false, false, out);
    }
    for ch in [R, G, B, M] {
        highlight_shadow(v, vpr, ch, true, false, out);
    }
    for ch in [R, G, B, M] {
        white_black(v, vpr, ch, true, false, out);
    }
    scontrast_fwd(v, vpr, out);
}

fn apply_rev_pixel(v: &GradingTone, vpr: &GradingTonePreRender, out: &mut Pixel) {
    scontrast_rev(v, vpr, out);
    for ch in [M, R, G, B] {
        white_black(v, vpr, ch, true, true, out);
    }
    for ch in [M, R, G, B] {
        highlight_shadow(v, vpr, ch, true, true, out);
    }
    for ch in [M, R, G, B] {
        white_black(v, vpr, ch, false, true, out);
    }
    for ch in [M, R, G, B] {
        highlight_shadow(v, vpr, ch, false, true, out);
    }
    for ch in [M, R, G, B] {
        mids_rev(v, vpr, ch, out);
    }
}

// ---------------------------------------------------------------------------
// The op

/// Tonal grading op (port of `GradingToneOp` / `GradingToneOpData`).
#[derive(Debug, Clone)]
pub struct GradingToneOp {
    style: GradingStyle,
    direction: TransformDirection,
    value: GradingValue<GradingTone, GradingTonePreRender>,
    metadata: FormatMetadata,
}

impl GradingToneOp {
    /// Create the op. The value is validated.
    pub fn new(
        style: GradingStyle,
        value: GradingTone,
        direction: TransformDirection,
        dynamic: bool,
    ) -> Result<Self> {
        value.validate()?;
        let pre = GradingTonePreRender::from_value(style, &value);
        Ok(Self {
            style,
            direction,
            value: GradingValue::new(value, pre, dynamic),
            metadata: FormatMetadata::default(),
        })
    }

    /// Identity op of the style.
    pub fn identity(style: GradingStyle) -> Self {
        let value = GradingTone::new(style);
        let pre = GradingTonePreRender::from_value(style, &value);
        Self {
            style,
            direction: TransformDirection::Forward,
            value: GradingValue::new(value, pre, false),
            metadata: FormatMetadata::default(),
        }
    }

    pub fn style(&self) -> GradingStyle {
        self.style
    }
    pub fn direction(&self) -> TransformDirection {
        self.direction
    }
    pub fn metadata(&self) -> &FormatMetadata {
        &self.metadata
    }
    pub fn set_metadata(&mut self, metadata: FormatMetadata) {
        self.metadata = metadata;
    }

    fn state(&self) -> Arc<(GradingTone, GradingTonePreRender)> {
        let style = self.style;
        self.value.current(|v| {
            v.validate()?;
            Ok(GradingTonePreRender::from_value(style, v))
        })
    }

    /// The current value (of the dynamic property if dynamic).
    pub fn value(&self) -> GradingTone {
        self.state().0
    }

    /// The same op in the opposite direction.
    pub fn inverse(&self) -> Self {
        let mut res = self.clone();
        res.direction = self.direction.inverse();
        res
    }

    /// True if `other` is the inverse of `self` (never true for dynamic ops).
    pub fn is_inverse(&self, other: &GradingToneOp) -> bool {
        if self.is_dynamic() || other.is_dynamic() {
            return false;
        }
        self.style == other.style
            && self.value() == other.value()
            && self.direction.combine(other.direction) == TransformDirection::Inverse
    }

    fn data_cache_id(&self) -> String {
        let mut s = String::new();
        if !self.metadata.id().is_empty() {
            s.push_str(self.metadata.id());
            s.push(' ');
        }
        s.push_str(self.style.as_str());
        s.push(' ');
        s.push_str(self.direction.as_str());
        s.push(' ');
        if !self.is_dynamic() {
            s.push_str(&format!("{:.7}", self.value()));
        }
        s
    }
}

/// Create a tonal grading op (port of `CreateGradingToneOp`): the op is
/// inverted if `direction` is inverse.
pub fn create_grading_tone_op(
    ops: &mut OpVec,
    style: GradingStyle,
    value: &GradingTone,
    op_direction: TransformDirection,
    dynamic: bool,
    direction: TransformDirection,
) -> Result<()> {
    let op = GradingToneOp::new(style, *value, op_direction.combine(direction), dynamic)?;
    ops.push(Arc::new(op));
    Ok(())
}

impl Op for GradingToneOp {
    fn name(&self) -> &'static str {
        "GradingTone"
    }

    fn apply(&self, pixels: &mut [Pixel]) {
        let state = self.state();
        let (v, vpr) = (&state.0, &state.1);
        if vpr.local_bypass {
            return;
        }
        let lin = self.style == GradingStyle::Lin;
        let fwd = self.direction == TransformDirection::Forward;
        for out in pixels.iter_mut() {
            if lin {
                for o in out.iter_mut().take(3) {
                    *o = log_lin::lin_log(*o);
                }
            }
            if fwd {
                apply_fwd_pixel(v, vpr, out);
            } else {
                apply_rev_pixel(v, vpr, out);
            }
            if lin {
                for o in out.iter_mut().take(3) {
                    *o = log_lin::log_lin(*o);
                }
            }
            clamp_max_rgb(out);
        }
    }

    fn is_no_op(&self) -> bool {
        self.is_identity()
    }

    fn is_identity(&self) -> bool {
        !self.is_dynamic() && self.value().is_identity()
    }

    fn has_channel_crosstalk(&self) -> bool {
        false
    }

    fn cache_id(&self) -> String {
        format!("<GradingToneOp {}>", self.data_cache_id())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn combine_with(&self, next: &dyn Op, flags: OptimizationFlags) -> Option<OpVec> {
        if !flags.contains(OptimizationFlags::PAIR_IDENTITY_GRADING) {
            return None;
        }
        let other = next.downcast_ref::<GradingToneOp>()?;
        self.is_inverse(other).then(OpVec::new)
    }

    fn is_dynamic(&self) -> bool {
        self.value.is_dynamic()
    }

    fn dynamic_property(&self, ty: DynamicPropertyType) -> Option<DynamicProperty> {
        if ty != DynamicPropertyType::GradingTone {
            return None;
        }
        self.value
            .property()
            .map(|p| DynamicProperty::GradingTone(p.clone()))
    }

    fn replace_dynamic_property(&mut self, prop: &DynamicProperty) {
        if let Some(p) = prop.as_grading_tone() {
            self.value.replace_property(p);
        }
    }

    fn make_non_dynamic(&self) -> Option<OpRc> {
        if !self.is_dynamic() {
            return None;
        }
        let style = self.style;
        let value = self.value.to_static(|v| {
            v.validate()?;
            Ok(GradingTonePreRender::from_value(style, v))
        });
        Some(Arc::new(Self {
            value,
            ..self.clone()
        }))
    }

    fn to_transform(&self) -> Option<Transform> {
        Some(Transform::GradingTone(GradingToneTransform {
            direction: self.direction,
            style: self.style,
            value: self.value(),
            dynamic: self.is_dynamic(),
            metadata: self.metadata.clone(),
        }))
    }

    fn clone_box(&self) -> Box<dyn Op> {
        Box::new(self.clone())
    }
}

// ---------------------------------------------------------------------------
// Transform

impl GradingToneTransform {
    /// Change the style (values are reset to the defaults of the new style).
    pub fn set_style(&mut self, style: GradingStyle) {
        if style != self.style {
            self.style = style;
            self.value = GradingTone::new(style);
        }
    }

    /// Set the values (they are validated first).
    pub fn set_value(&mut self, value: GradingTone) -> Result<()> {
        value.validate()?;
        self.value = value;
        Ok(())
    }

    pub fn is_dynamic(&self) -> bool {
        self.dynamic
    }
    pub fn make_dynamic(&mut self) {
        self.dynamic = true;
    }
    pub fn make_non_dynamic(&mut self) {
        self.dynamic = false;
    }
}

impl Validate for GradingToneTransform {
    fn validate(&self) -> Result<()> {
        self.value
            .validate()
            .map_err(|e| e.prefixed("GradingToneTransform validation failed: "))
    }
}

impl BuildOps for GradingToneTransform {
    fn build_ops(
        &self,
        ops: &mut OpVec,
        _config: &Config,
        _context: &Context,
        dir: TransformDirection,
    ) -> Result<()> {
        let mut op = GradingToneOp::new(
            self.style,
            self.value,
            self.direction.combine(dir),
            self.dynamic,
        )?;
        op.set_metadata(self.metadata.clone());
        ops.push(Arc::new(op));
        Ok(())
    }
}

impl fmt::Display for GradingToneTransform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "<GradingToneTransform direction={}, style={}, values={}",
            self.direction.as_str(),
            self.style.as_str(),
            self.value
        )?;
        if self.dynamic {
            f.write_str(", dynamic")?;
        }
        f.write_str(">")
    }
}

#[cfg(test)]
mod tests;
