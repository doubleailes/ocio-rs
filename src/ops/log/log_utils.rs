//! Log helpers (port of `LogUtils.cpp`): CTF log styles, conversion of the
//! legacy (Cineon-like) CTF parameters to the OCIO log parameters, and the
//! computation of the linear segment of camera logs.

use super::{
    LINEAR_SLOPE, LIN_SIDE_BREAK, LIN_SIDE_OFFSET, LIN_SIDE_SLOPE, LOG_SIDE_OFFSET, LOG_SIDE_SLOPE,
};
use crate::error::Result;
use crate::ops::matrix::float_format::format_g;
use crate::types::TransformDirection;

/// Log styles of the CTF format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum LogStyle {
    /// Base-10 logarithm.
    #[default]
    Log10,
    /// Base-2 logarithm.
    Log2,
    /// Base-10 anti-logarithm (power).
    AntiLog10,
    /// Base-2 anti-logarithm (power).
    AntiLog2,
    /// Convert Cineon (or similar) log media to scene-linear or video.
    LogToLin,
    /// Convert scene-linear or video to Cineon (or similar) log media.
    LinToLog,
    /// Log-to-lin with a linear section near black.
    CameraLogToLin,
    /// Lin-to-log with a linear section near black.
    CameraLinToLog,
}

/// CTF name of the `log2` style.
pub const LOG2_STR: &str = "log2";
/// CTF name of the `log10` style.
pub const LOG10_STR: &str = "log10";
/// CTF name of the `antiLog2` style.
pub const ANTI_LOG2_STR: &str = "antiLog2";
/// CTF name of the `antiLog10` style.
pub const ANTI_LOG10_STR: &str = "antiLog10";
/// CTF name of the `linToLog` style.
pub const LIN_TO_LOG_STR: &str = "linToLog";
/// CTF name of the `logToLin` style.
pub const LOG_TO_LIN_STR: &str = "logToLin";
/// CTF name of the `cameraLinToLog` style.
pub const CAMERA_LIN_TO_LOG_STR: &str = "cameraLinToLog";
/// CTF name of the `cameraLogToLin` style.
pub const CAMERA_LOG_TO_LIN_STR: &str = "cameraLogToLin";

/// Parse a CTF log style (case insensitive).
pub fn convert_string_to_style(s: &str) -> Result<LogStyle> {
    if s.is_empty() {
        crate::bail!("Missing Log style.");
    }
    let l = s.to_ascii_lowercase();
    let style = if l == LOG10_STR.to_ascii_lowercase() {
        LogStyle::Log10
    } else if l == LOG2_STR.to_ascii_lowercase() {
        LogStyle::Log2
    } else if l == ANTI_LOG10_STR.to_ascii_lowercase() {
        LogStyle::AntiLog10
    } else if l == ANTI_LOG2_STR.to_ascii_lowercase() {
        LogStyle::AntiLog2
    } else if l == LOG_TO_LIN_STR.to_ascii_lowercase() {
        LogStyle::LogToLin
    } else if l == LIN_TO_LOG_STR.to_ascii_lowercase() {
        LogStyle::LinToLog
    } else if l == CAMERA_LOG_TO_LIN_STR.to_ascii_lowercase() {
        LogStyle::CameraLogToLin
    } else if l == CAMERA_LIN_TO_LOG_STR.to_ascii_lowercase() {
        LogStyle::CameraLinToLog
    } else {
        crate::bail!("Unknown Log style: '{}'.", s);
    };
    Ok(style)
}

/// CTF name of a log style.
pub fn convert_style_to_string(style: LogStyle) -> &'static str {
    match style {
        LogStyle::Log10 => LOG10_STR,
        LogStyle::Log2 => LOG2_STR,
        LogStyle::AntiLog10 => ANTI_LOG10_STR,
        LogStyle::AntiLog2 => ANTI_LOG2_STR,
        LogStyle::LogToLin => LOG_TO_LIN_STR,
        LogStyle::LinToLog => LIN_TO_LOG_STR,
        LogStyle::CameraLogToLin => CAMERA_LOG_TO_LIN_STR,
        LogStyle::CameraLinToLog => CAMERA_LIN_TO_LOG_STR,
    }
}

/// Kind of CTF log parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CtfParamsType {
    /// Not yet known.
    #[default]
    Unknown,
    /// Legacy Cineon parameters (gamma, refWhite, refBlack, highlight, shadow).
    Cineon,
    /// CLF parameters.
    Clf,
}

/// Channel index of [`CtfParams::params`].
pub const CTF_RED: usize = 0;
/// Channel index of [`CtfParams::params`].
pub const CTF_GREEN: usize = 1;
/// Channel index of [`CtfParams::params`].
pub const CTF_BLUE: usize = 2;

/// Index of the gamma value of a legacy CTF log parameter set.
pub const CTF_GAMMA: usize = 0;
/// Index of the reference white value.
pub const CTF_REF_WHITE: usize = 1;
/// Index of the reference black value.
pub const CTF_REF_BLACK: usize = 2;
/// Index of the highlight value.
pub const CTF_HIGHLIGHT: usize = 3;
/// Index of the shadow value.
pub const CTF_SHADOW: usize = 4;

/// Log parameters read from a CTF file (port of `LogUtil::CTFParams`).
#[derive(Debug, Clone, PartialEq)]
pub struct CtfParams {
    /// Style of the log.
    pub style: LogStyle,
    /// Red, green and blue parameters: gamma, refWhite, refBlack, highlight,
    /// shadow.
    pub params: [Vec<f64>; 3],
    /// Kind of parameters (see [`CtfParams::set_type`]).
    pub param_type: CtfParamsType,
}

impl Default for CtfParams {
    fn default() -> Self {
        Self {
            style: LogStyle::Log10,
            params: [vec![0.0; 5], vec![0.0; 5], vec![0.0; 5]],
            param_type: CtfParamsType::Unknown,
        }
    }
}

impl CtfParams {
    /// Parameters of a channel ([`CTF_RED`], [`CTF_GREEN`], [`CTF_BLUE`]).
    pub fn get(&self, channel: usize) -> &Vec<f64> {
        &self.params[channel]
    }

    /// Mutable parameters of a channel.
    pub fn get_mut(&mut self, channel: usize) -> &mut Vec<f64> {
        &mut self.params[channel]
    }

    /// Set the kind of parameters; returns false if a different kind was
    /// already set.
    pub fn set_type(&mut self, ty: CtfParamsType) -> bool {
        if self.param_type == CtfParamsType::Unknown {
            self.param_type = ty;
        } else if ty != self.param_type {
            return false;
        }
        true
    }

    /// The kind of parameters.
    pub fn get_type(&self) -> CtfParamsType {
        self.param_type
    }
}

fn convert_from_ctf_to_ocio(ctf: &[f64], ocio: &mut [f64]) {
    // Base is 10.0.
    const RANGE: f64 = 0.002 * 1023.0;

    let gamma = ctf[CTF_GAMMA];
    let ref_white = ctf[CTF_REF_WHITE] / 1023.0;
    let ref_black = ctf[CTF_REF_BLACK] / 1023.0;
    let highlight = ctf[CTF_HIGHLIGHT];
    let shadow = ctf[CTF_SHADOW];

    let mult_factor = RANGE / gamma;
    let mut tmp_value = (ref_black - ref_white) * mult_factor;
    // Avoid a division by zero in the gain calculation.
    tmp_value = if -0.0001 < tmp_value {
        -0.0001
    } else {
        tmp_value
    };

    let gain = (highlight - shadow) / (1.0 - 10.0f64.powf(tmp_value));
    let offset = gain - (highlight - shadow);

    ocio[LOG_SIDE_SLOPE] = 1.0 / mult_factor;
    ocio[LIN_SIDE_SLOPE] = 1.0 / gain;
    ocio[LIN_SIDE_OFFSET] = (offset - shadow) / gain;
    ocio[LOG_SIDE_OFFSET] = ref_white;
}

fn validate_legacy_params(ctf: &[f64]) -> Result<()> {
    if ctf.len() != 5 {
        crate::bail!("Log: Expecting 5 parameters.");
    }
    let gamma = ctf[CTF_GAMMA];
    let ref_white = ctf[CTF_REF_WHITE];
    let ref_black = ctf[CTF_REF_BLACK];
    let highlight = ctf[CTF_HIGHLIGHT];
    let shadow = ctf[CTF_SHADOW];

    if !(gamma > 0.01f32 as f64) {
        crate::bail!(
            "Log: Invalid gamma value '{}', gamma should be greater than 0.01.",
            format_g(gamma, 6)
        );
    }
    if !(ref_white > ref_black) {
        crate::bail!(
            "Log: Invalid refWhite '{}' and refBlack '{}', refWhite should be greater than refBlack.",
            format_g(ref_white, 6),
            format_g(ref_black, 6)
        );
    }
    if !(highlight > shadow) {
        crate::bail!(
            "Log: Invalid highlight '{}' and shadow '{}', highlight should be greater than shadow.",
            format_g(highlight, 6),
            format_g(shadow, 6)
        );
    }
    Ok(())
}

/// Convert CTF log parameters into a base and OCIO log parameters (log side
/// slope, log side offset, lin side slope, lin side offset) per channel.
/// `base` is left unchanged for the camera styles (which do not use the
/// legacy parameters).
pub fn convert_log_parameters(ctf: &CtfParams, base: &mut f64) -> Result<[Vec<f64>; 3]> {
    let mut p = [
        vec![1.0, 0.0, 1.0, 0.0],
        vec![1.0, 0.0, 1.0, 0.0],
        vec![1.0, 0.0, 1.0, 0.0],
    ];
    match ctf.style {
        LogStyle::Log10 | LogStyle::AntiLog10 => *base = 10.0,
        LogStyle::Log2 | LogStyle::AntiLog2 => *base = 2.0,
        LogStyle::LinToLog | LogStyle::LogToLin => {
            *base = 10.0;
            for c in 0..3 {
                validate_legacy_params(&ctf.params[c])?;
            }
            for c in 0..3 {
                convert_from_ctf_to_ocio(&ctf.params[c], &mut p[c]);
            }
        }
        // Should not be used for the new style.
        LogStyle::CameraLinToLog | LogStyle::CameraLogToLin => {}
    }
    Ok(p)
}

/// Direction of a CTF log style.
pub fn get_log_direction(style: LogStyle) -> TransformDirection {
    match style {
        LogStyle::Log10 | LogStyle::Log2 | LogStyle::LinToLog | LogStyle::CameraLinToLog => {
            TransformDirection::Forward
        }
        LogStyle::AntiLog10
        | LogStyle::AntiLog2
        | LogStyle::LogToLin
        | LogStyle::CameraLogToLin => TransformDirection::Inverse,
    }
}

/// Slope of the linear segment of a camera log (given or computed so that
/// the linear segment matches the log slope at the break point).
pub fn get_linear_slope(params: &[f64], base: f64) -> f32 {
    if params.len() > LINEAR_SLOPE {
        params[LINEAR_SLOPE] as f32
    } else {
        (params[LOG_SIDE_SLOPE] * params[LIN_SIDE_SLOPE]
            / ((params[LIN_SIDE_SLOPE] * params[LIN_SIDE_BREAK] + params[LIN_SIDE_OFFSET])
                * base.ln())) as f32
    }
}

/// Log side value of the break point of a camera log.
pub fn get_log_side_break(params: &[f64], base: f64) -> f32 {
    // Note: OCIO calls the C `log2(double)` here, so the logarithms are
    // computed in double precision on float arguments.
    let mut log_side_break = (((params[LIN_SIDE_SLOPE] * params[LIN_SIDE_BREAK]
        + params[LIN_SIDE_OFFSET]) as f32) as f64)
        .log2() as f32;
    log_side_break = (log_side_break as f64
        * (params[LOG_SIDE_SLOPE] as f32 as f64 / (base as f32 as f64).log2()))
        as f32;
    log_side_break += params[LOG_SIDE_OFFSET] as f32;
    log_side_break
}

/// Offset of the linear segment of a camera log.
pub fn get_linear_offset(params: &[f64], linear_slope: f32, log_side_break: f32) -> f32 {
    log_side_break - linear_slope * params[LIN_SIDE_BREAK] as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::log::LogOpData;
    use crate::ops::matrix::test_utils::assert_close;

    #[test]
    fn styles() {
        for s in [
            LogStyle::Log10,
            LogStyle::Log2,
            LogStyle::AntiLog10,
            LogStyle::AntiLog2,
            LogStyle::LogToLin,
            LogStyle::LinToLog,
            LogStyle::CameraLogToLin,
            LogStyle::CameraLinToLog,
        ] {
            assert_eq!(
                convert_string_to_style(convert_style_to_string(s)).unwrap(),
                s
            );
        }
        assert_eq!(convert_string_to_style("LOG10").unwrap(), LogStyle::Log10);
        assert_eq!(
            convert_string_to_style("").unwrap_err().message(),
            "Missing Log style."
        );
        assert_eq!(
            convert_string_to_style("foo").unwrap_err().message(),
            "Unknown Log style: 'foo'."
        );
    }

    #[test]
    fn ctf_to_ocio_fail() {
        let mut ctf = CtfParams {
            style: LogStyle::LogToLin,
            ..Default::default()
        };
        ctf.params[CTF_RED] = vec![0.005, 375., 140., 0.8, 0.5];
        ctf.params[CTF_GREEN] = ctf.params[CTF_RED].clone();
        ctf.params[CTF_BLUE] = ctf.params[CTF_RED].clone();
        let e = convert_log_parameters(&ctf, &mut 1.0).unwrap_err();
        assert!(e.message().contains("gamma should be greater than 0.01"));
        assert_eq!(
            e.message(),
            "Log: Invalid gamma value '0.005', gamma should be greater than 0.01."
        );

        ctf.params[CTF_RED] = vec![0.9, 375., 375., 0.8, 0.5];
        let e = convert_log_parameters(&ctf, &mut 1.0).unwrap_err();
        assert!(e
            .message()
            .contains("refWhite should be greater than refBlack"));

        ctf.params[CTF_RED] = vec![0.9, 375., 140., 0.5, 0.5];
        let e = convert_log_parameters(&ctf, &mut 1.0).unwrap_err();
        assert!(e
            .message()
            .contains("highlight should be greater than shadow"));
    }

    #[test]
    fn ctf_to_ocio_ok() {
        let mut ctf = CtfParams {
            style: LogStyle::Log10,
            ..Default::default()
        };
        let mut base = 1.0;
        let p = convert_log_parameters(&ctf, &mut base).unwrap();
        let dir = get_log_direction(ctf.style);
        assert_eq!(base, 10.0);
        assert_eq!(p[0], vec![1.0, 0.0, 1.0, 0.0]);
        assert_eq!(dir, TransformDirection::Forward);
        let log = LogOpData::new(base, p[0].clone(), p[1].clone(), p[2].clone(), dir).unwrap();
        assert!(!log.is_identity());
        assert!(!log.has_channel_crosstalk());
        assert!(log.validate().is_ok());

        for (style, exp_base, exp_dir) in [
            (LogStyle::Log2, 2.0, TransformDirection::Forward),
            (LogStyle::AntiLog10, 10.0, TransformDirection::Inverse),
            (LogStyle::AntiLog2, 2.0, TransformDirection::Inverse),
        ] {
            ctf.style = style;
            let mut base = 1.0;
            let p = convert_log_parameters(&ctf, &mut base).unwrap();
            assert_eq!(base, exp_base);
            assert_eq!(p[0], vec![1.0, 0.0, 1.0, 0.0]);
            assert_eq!(get_log_direction(style), exp_dir);
            let log =
                LogOpData::new(base, p[0].clone(), p[1].clone(), p[2].clone(), exp_dir).unwrap();
            assert!(log.validate().is_ok());
        }

        ctf.params[CTF_RED] = vec![4.6, 758., 30., 0.7, 0.4];
        ctf.params[CTF_GREEN] = vec![2.6, 300., 42., 0.8, 0.1];
        ctf.params[CTF_BLUE] = ctf.params[CTF_RED].clone();
        ctf.style = LogStyle::LinToLog;
        let mut base = 1.0;
        let p = convert_log_parameters(&ctf, &mut base).unwrap();
        let tol = 1e-6;
        assert_eq!(base, 10.0);
        assert_close(p[0][LOG_SIDE_SLOPE], 2.2482893, tol);
        assert_close(p[0][LIN_SIDE_SLOPE], 1.7250706, tol);
        assert_close(p[0][LIN_SIDE_OFFSET], -0.2075494, tol);
        assert_close(p[0][LOG_SIDE_OFFSET], 0.7409580, tol);
        assert_close(p[1][LOG_SIDE_SLOPE], 1.2707722, tol);
        assert_close(p[1][LIN_SIDE_SLOPE], 0.5240051, tol);
        assert_close(p[1][LIN_SIDE_OFFSET], 0.5807959, tol);
        assert_close(p[1][LOG_SIDE_OFFSET], 0.2932551, tol);
        assert_eq!(get_log_direction(ctf.style), TransformDirection::Forward);
        let log = LogOpData::new(
            base,
            p[0].clone(),
            p[1].clone(),
            p[2].clone(),
            TransformDirection::Forward,
        );
        assert!(log.unwrap().validate().is_ok());

        ctf.style = LogStyle::LogToLin;
        let mut base = 1.0;
        let p = convert_log_parameters(&ctf, &mut base).unwrap();
        assert_eq!(get_log_direction(ctf.style), TransformDirection::Inverse);
        let log = LogOpData::new(
            base,
            p[0].clone(),
            p[1].clone(),
            p[2].clone(),
            TransformDirection::Inverse,
        );
        assert!(log.unwrap().validate().is_ok());
    }

    #[test]
    fn ctf_params_type() {
        let mut p = CtfParams::default();
        assert_eq!(p.get_type(), CtfParamsType::Unknown);
        assert!(p.set_type(CtfParamsType::Clf));
        assert!(p.set_type(CtfParamsType::Clf));
        assert!(!p.set_type(CtfParamsType::Cineon));
        assert_eq!(p.get_type(), CtfParamsType::Clf);
    }
}
