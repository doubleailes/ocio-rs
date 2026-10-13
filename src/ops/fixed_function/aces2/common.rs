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
    /// RGB to chromatically adapted CAM16 cone responses (includes the
    /// reference luminance scale and the white-point adaptation gains).
    pub matrix_rgb_to_cam16_c: M33f,
    /// Inverse of `matrix_rgb_to_cam16_c`.
    pub matrix_cam16_c_to_rgb: M33f,
    /// Compressed cone responses to the opponent space Aab (A normalised by
    /// the white's achromatic response).
    pub matrix_cone_response_to_aab: M33f,
    /// Inverse of `matrix_cone_response_to_aab`.
    pub matrix_aab_to_cone_response: M33f,
    /// F_L normalised.
    pub f_l_n: f32,
    /// Exponent `c * z` mapping the achromatic response A to lightness J.
    pub cz: f32,
    /// 1 / cz.
    pub inv_cz: f32,
    /// Compressed achromatic response of the adapting luminance, used to
    /// convert between A and Y for achromatic values.
    pub a_w_j: f32,
    /// 1 / A_w_J.
    pub inv_a_w_j: f32,
}

/// Tonescale parameters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ToneScaleParams {
    /// Peak display luminance, in nits.
    pub n: f32,
    /// Normalised white luminance, in nits (what 1.0 maps to).
    pub n_r: f32,
    /// Surround / contrast exponent.
    pub g: f32,
    /// Shadow toe (flare/glare compensation).
    pub t_1: f32,
    /// Output value of 18% grey, relative to `n_r`.
    pub c_t: f32,
    /// Scale of the curve's input (Michaelis-Menten semi-saturation).
    pub s_2: f32,
    /// Curve normalisation term used to derive `m_2`.
    pub u_2: f32,
    /// Output scale of the curve.
    pub m_2: f32,
    /// Largest scene-referred input considered by the forward transform
    /// (bounds the reach corner search).
    pub forward_limit: f32,
    /// Clamp applied to normalised luminance before the inverse curve.
    pub inverse_limit: f32,
    /// `log10(n / n_r)`, drives peak-dependent compression parameters.
    pub log_peak: f32,
}

/// Parameters shared by chroma and gamut compression.
#[derive(Debug, Clone, PartialEq)]
pub struct SharedCompressionParameters {
    /// Lightness J of the peak luminance.
    pub limit_j_max: f32,
    /// 1 / model gamma (`c * z`), the exponent of the reach boundary.
    pub model_gamma_inv: f32,
    /// Per-hue maximum colourfulness M of the reach gamut at `limit_j_max`.
    pub reach_m_table: Box<Table1D>,
}

/// Shared compression parameters resolved for a given hue.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedSharedCompressionParameters {
    /// Lightness J of the peak luminance.
    pub limit_j_max: f32,
    /// 1 / model gamma (`c * z`), the exponent of the reach boundary.
    pub model_gamma_inv: f32,
    /// Maximum reach colourfulness M at this hue (interpolated from the table).
    pub reach_max_m: f32,
}

/// Chroma compression parameters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChromaCompressParams {
    /// Saturation expansion strength (toe applied near the limit).
    pub sat: f32,
    /// Saturation expansion threshold.
    pub sat_thr: f32,
    /// Chroma compression strength.
    pub compr: f32,
    /// Scale of the hue-dependent chroma normalisation.
    pub chroma_compress_scale: f32,
}

/// Gamut compression parameters depending on the hue.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HueDependantGamutParams {
    /// 1 / gamma of the lower gamut hull.
    pub gamma_bottom_inv: f32,
    /// J and M of the limiting gamut cusp at this hue.
    pub jm_cusp: F2,
    /// 1 / gamma of the upper gamut hull at this hue.
    pub gamma_top_inv: f32,
    /// Lightness of the focus point the compression vectors point at.
    pub focus_j: f32,
    /// J above which the focus gain uses its approximate adjustment.
    pub analytical_threshold: f32,
}

/// Gamut compression parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct GamutCompressParams {
    /// Lightness J of 18% grey after tonescaling.
    pub mid_j: f32,
    /// Focus distance, scaling the compression vector slope gain.
    pub focus_dist: f32,
    /// 1 / gamma of the lower gamut hull.
    pub lower_hull_gamma_inv: f32,
    /// Offsets bounding the hue table search around a uniform-table guess.
    pub hue_linearity_search_range: [i32; 2],
    /// Sampled hues (non-uniform, includes the gamut corners).
    pub hue_table: Box<Table1D>,
    /// Per-hue limiting gamut cusp J and M, and upper hull 1 / gamma.
    pub gamut_cusp_table: Box<Table3D>,
}

// CAM
/// Reference luminance (in nits) corresponding to 1.0.
pub const REFERENCE_LUMINANCE: f32 = 100.0;
/// Adapting field luminance L_A.
pub const L_A: f32 = 100.0;
/// Relative background luminance Y_b.
pub const Y_B: f32 = 20.0;
/// Dim surround.
pub const SURROUND: F3 = [0.9, 0.59, 0.9];

/// Scale of lightness J (J of the white).
pub const J_SCALE: f32 = 100.0;
/// Reference luminance of the cone response non-linearity.
pub const CAM_NL_Y_REFERENCE: f32 = 100.0;
/// Offset (semi-saturation) of the cone response compression.
pub const CAM_NL_OFFSET: f32 = 0.2713 * CAM_NL_Y_REFERENCE;
/// Scale applied to compressed cone responses before conversion to Aab.
pub const CAM_NL_SCALE: f32 = 4.0 * CAM_NL_Y_REFERENCE;

// Chroma compression
/// Base chroma compression strength.
pub const CHROMA_COMPRESS: f32 = 2.4;
/// Growth of the chroma compression with log peak luminance.
pub const CHROMA_COMPRESS_FACT: f32 = 3.3;
/// Base saturation expansion strength.
pub const CHROMA_EXPAND: f32 = 1.3;
/// Reduction of the saturation expansion with log peak luminance.
pub const CHROMA_EXPAND_FACT: f32 = 0.69;
/// Saturation expansion threshold (divided by the peak luminance).
pub const CHROMA_EXPAND_THR: f32 = 0.5;

// Gamut compression
/// Smoothing of the minimum between the lower and upper gamut hulls.
pub const SMOOTH_CUSPS: f32 = 0.12;
/// Extra cusp M expansion (scaled by `SMOOTH_CUSPS`) to offset the smoothing.
pub const SMOOTH_M: f32 = 0.27;
/// Blend of the focus J from the cusp J towards the mid J.
pub const CUSP_MID_BLEND: f32 = 1.3;
/// Position of the analytical threshold between the cusp J and the max J.
pub const FOCUS_GAIN_BLEND: f32 = 0.3;
/// Inverse of the focus adjustment gain (unused by the current algorithm).
pub const FOCUS_ADJUST_GAIN_INV: f32 = 1.0 / 0.55;
/// Base focus distance.
pub const FOCUS_DISTANCE: f32 = 1.35;
/// Growth of the focus distance with log peak luminance.
pub const FOCUS_DISTANCE_SCALING: f32 = 1.75;
/// Minimum gamut/reach boundary ratio below which M is left uncompressed.
pub const COMPRESSION_THRESHOLD: f32 = 0.75;

/// CAM16 primaries.
pub const CAM16_PRIMARIES: Primaries = Primaries::new(
    Chromaticities::new(0.8336, 0.1735),
    Chromaticities::new(2.3854, -1.4659),
    Chromaticities::new(0.087, -0.125),
    Chromaticities::new(0.333, 0.333),
);

// Table generation
/// Lower bound of the upper hull gamma search.
pub const GAMMA_MINIMUM: f32 = 0.0;
/// Upper bound of the upper hull gamma search.
pub const GAMMA_MAXIMUM: f32 = 5.0;
/// Step of the coarse upper hull gamma search.
pub const GAMMA_SEARCH_STEP: f32 = 0.4;
/// Precision of the upper hull gamma bisection.
pub const GAMMA_ACCURACY: f32 = 1e-5;

/// Number of RGB cube cusp corners (R, Y, G, C, B, M).
pub const CUSP_CORNER_COUNT: usize = 6;
/// Cusp corners plus one wrapping entry at each end.
pub const TOTAL_CORNER_COUNT: usize = CUSP_CORNER_COUNT + 2;
/// Capacity of the merged reach and display corner hue list.
pub const MAX_SORTED_CORNERS: usize = 2 * CUSP_CORNER_COUNT;
/// Bisection precision when searching the reach corners at max J.
pub const REACH_CUSP_TOLERANCE: f32 = 1e-3;
/// Bisection precision when searching the display cusp for a hue.
pub const DISPLAY_CUSP_TOLERANCE: f32 = 1e-7;
