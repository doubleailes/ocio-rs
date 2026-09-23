//! Constants, lookup tables and parameter structures of the ACES 2 output
//! transform (port of `ACES2/Common.h`).

use super::matrix::{Chromaticities, M33f, Primaries, F2, F3};

/// Pi, as used by the ACES 2 code.
pub const PI: f32 = 3.14159265358979;

/// Hues are expressed in degrees.
pub const HUE_LIMIT: f32 = 360.0;

/// Wrap a hue already in `(-hue_limit, hue_limit)` into `[0, hue_limit]`.
#[inline]
pub fn wrap_to_hue_limit_unchecked(y: f32) -> f32 {
    if y < 0.0 {
        y + HUE_LIMIT
    } else {
        y
    }
}

/// Wrap any hue into `[0, hue_limit]`.
#[inline]
pub fn wrap_to_hue_limit(hue: f32) -> f32 {
    wrap_to_hue_limit_unchecked(hue % HUE_LIMIT)
}

/// Internal hue unit to degrees (identity: hues are in degrees).
#[inline]
pub fn to_degrees(v: f32) -> f32 {
    v
}

/// Degrees to the internal (wrapped) hue unit.
#[inline]
pub fn from_degrees(v: f32) -> f32 {
    wrap_to_hue_limit(v)
}

/// Internal hue unit to radians.
#[inline]
pub fn to_radians(v: f32) -> f32 {
    PI * v / 180.0
}

/// Radians (already within `(-pi, pi]`) to the internal hue unit.
#[inline]
pub fn from_radians_unchecked(v: f32) -> f32 {
    wrap_to_hue_limit_unchecked(180.0 * v / PI)
}

/// Radians to the internal hue unit.
#[inline]
pub fn from_radians(v: f32) -> f32 {
    wrap_to_hue_limit(180.0 * v / PI)
}

/// Layout of the hue-indexed tables (`TableBase`).
pub mod table {
    /// Extra entries below the nominal range.
    pub const LOWER_ENTRIES: usize = 1;
    /// Extra entries above the nominal range.
    pub const UPPER_ENTRIES: usize = 2;
    /// Index of the first nominal entry.
    pub const BASE_INDEX: usize = LOWER_ENTRIES;
    /// Number of nominal entries (one per degree).
    pub const NOMINAL_SIZE: usize = 360;
    /// Total number of entries.
    pub const TOTAL_SIZE: usize = NOMINAL_SIZE + LOWER_ENTRIES + UPPER_ENTRIES;
    /// Index of the lower wrapping entry.
    pub const LOWER_WRAP_INDEX: usize = 0;
    /// Index of the (first) upper wrapping entry.
    pub const UPPER_WRAP_INDEX: usize = BASE_INDEX + NOMINAL_SIZE;
    /// Index of the first nominal entry.
    pub const FIRST_NOMINAL_INDEX: usize = BASE_INDEX;
    /// Index of the last nominal entry.
    pub const LAST_NOMINAL_INDEX: usize = UPPER_WRAP_INDEX - 1;

    /// Hue corresponding to a table position.
    #[inline]
    pub fn base_hue_for_position(i_lo: usize) -> f32 {
        // hue_limit == nominal_size.
        i_lo as f32
    }

    /// Position of a wrapped hue in a uniform table. The result is bounded
    /// to the nominal size (only reached for invalid, e.g. NaN, hues).
    #[inline]
    pub fn hue_position_in_uniform_table(wrapped_hue: f32) -> usize {
        // hue_limit == nominal_size. Note: `as` saturates (and maps NaN to 0).
        (wrapped_hue as usize).min(NOMINAL_SIZE)
    }

    /// Nominal position of a wrapped hue in a uniform table.
    #[inline]
    pub fn nominal_hue_position_in_uniform_table(wrapped_hue: f32) -> usize {
        FIRST_NOMINAL_INDEX + hue_position_in_uniform_table(wrapped_hue)
    }
}

/// A hue-indexed table of floats.
pub type Table1D = [f32; table::TOTAL_SIZE];
/// A hue-indexed table of float triplets.
pub type Table3D = [[f32; 3]; table::TOTAL_SIZE];

/// Parameters of the CAM (JMh) model for a set of primaries.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JMhParams {
    pub matrix_rgb_to_cam16_c: M33f,
    pub matrix_cam16_c_to_rgb: M33f,
    pub matrix_cone_response_to_aab: M33f,
    pub matrix_aab_to_cone_response: M33f,
    /// F_L normalised.
    pub f_l_n: f32,
    pub cz: f32,
    /// 1 / cz.
    pub inv_cz: f32,
    pub a_w_j: f32,
    /// 1 / A_w_J.
    pub inv_a_w_j: f32,
}

/// Tonescale parameters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ToneScaleParams {
    pub n: f32,
    pub n_r: f32,
    pub g: f32,
    pub t_1: f32,
    pub c_t: f32,
    pub s_2: f32,
    pub u_2: f32,
    pub m_2: f32,
    pub forward_limit: f32,
    pub inverse_limit: f32,
    pub log_peak: f32,
}

/// Parameters shared by chroma and gamut compression.
#[derive(Debug, Clone, PartialEq)]
pub struct SharedCompressionParameters {
    pub limit_j_max: f32,
    pub model_gamma_inv: f32,
    pub reach_m_table: Box<Table1D>,
}

/// Shared compression parameters resolved for a given hue.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedSharedCompressionParameters {
    pub limit_j_max: f32,
    pub model_gamma_inv: f32,
    pub reach_max_m: f32,
}

/// Chroma compression parameters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChromaCompressParams {
    pub sat: f32,
    pub sat_thr: f32,
    pub compr: f32,
    pub chroma_compress_scale: f32,
}

/// Gamut compression parameters depending on the hue.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HueDependantGamutParams {
    pub gamma_bottom_inv: f32,
    pub jm_cusp: F2,
    pub gamma_top_inv: f32,
    pub focus_j: f32,
    pub analytical_threshold: f32,
}

/// Gamut compression parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct GamutCompressParams {
    pub mid_j: f32,
    pub focus_dist: f32,
    pub lower_hull_gamma_inv: f32,
    pub hue_linearity_search_range: [i32; 2],
    pub hue_table: Box<Table1D>,
    pub gamut_cusp_table: Box<Table3D>,
}

// CAM
pub const REFERENCE_LUMINANCE: f32 = 100.0;
pub const L_A: f32 = 100.0;
pub const Y_B: f32 = 20.0;
/// Dim surround.
pub const SURROUND: F3 = [0.9, 0.59, 0.9];

pub const J_SCALE: f32 = 100.0;
pub const CAM_NL_Y_REFERENCE: f32 = 100.0;
pub const CAM_NL_OFFSET: f32 = 0.2713 * CAM_NL_Y_REFERENCE;
pub const CAM_NL_SCALE: f32 = 4.0 * CAM_NL_Y_REFERENCE;

// Chroma compression
pub const CHROMA_COMPRESS: f32 = 2.4;
pub const CHROMA_COMPRESS_FACT: f32 = 3.3;
pub const CHROMA_EXPAND: f32 = 1.3;
pub const CHROMA_EXPAND_FACT: f32 = 0.69;
pub const CHROMA_EXPAND_THR: f32 = 0.5;

// Gamut compression
pub const SMOOTH_CUSPS: f32 = 0.12;
pub const SMOOTH_M: f32 = 0.27;
pub const CUSP_MID_BLEND: f32 = 1.3;
pub const FOCUS_GAIN_BLEND: f32 = 0.3;
pub const FOCUS_ADJUST_GAIN_INV: f32 = 1.0 / 0.55;
pub const FOCUS_DISTANCE: f32 = 1.35;
pub const FOCUS_DISTANCE_SCALING: f32 = 1.75;
pub const COMPRESSION_THRESHOLD: f32 = 0.75;

/// CAM16 primaries.
pub const CAM16_PRIMARIES: Primaries = Primaries::new(
    Chromaticities::new(0.8336, 0.1735),
    Chromaticities::new(2.3854, -1.4659),
    Chromaticities::new(0.087, -0.125),
    Chromaticities::new(0.333, 0.333),
);

// Table generation
pub const GAMMA_MINIMUM: f32 = 0.0;
pub const GAMMA_MAXIMUM: f32 = 5.0;
pub const GAMMA_SEARCH_STEP: f32 = 0.4;
pub const GAMMA_ACCURACY: f32 = 1e-5;

pub const CUSP_CORNER_COUNT: usize = 6;
pub const TOTAL_CORNER_COUNT: usize = CUSP_CORNER_COUNT + 2;
pub const MAX_SORTED_CORNERS: usize = 2 * CUSP_CORNER_COUNT;
pub const REACH_CUSP_TOLERANCE: f32 = 1e-3;
pub const DISPLAY_CUSP_TOLERANCE: f32 = 1e-7;
