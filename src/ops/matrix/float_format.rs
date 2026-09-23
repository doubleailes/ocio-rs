//! Formatting of floating point numbers the way a C++ `std::ostream` does
//! with a given `precision()` and the default float field (i.e. `printf`'s
//! `%g`). OCIO uses it to build cache ids and to serialize parameters.

/// Format `v` like `std::ostream << v` with `precision` significant digits
/// (the default float field, equivalent to `printf("%.*g", precision, v)`).
///
/// ```
/// use ocio::ops::matrix::float_format::format_g;
/// assert_eq!(format_g(0.18, 7), "0.18");
/// assert_eq!(format_g(1.0, 7), "1");
/// assert_eq!(format_g(1e-10, 7), "1e-10");
/// assert_eq!(format_g(123456789.0, 7), "1.234568e+08");
/// ```
pub fn format_g(v: f64, precision: usize) -> String {
    if v.is_nan() {
        return if v.is_sign_negative() {
            "-nan".to_string()
        } else {
            "nan".to_string()
        };
    }
    if v.is_infinite() {
        return if v < 0.0 {
            "-inf".to_string()
        } else {
            "inf".to_string()
        };
    }
    let p = precision.max(1);
    if v == 0.0 {
        return if v.is_sign_negative() {
            "-0".to_string()
        } else {
            "0".to_string()
        };
    }

    // Round to `p` significant digits using the scientific representation,
    // then decide between the fixed and the scientific notation as %g does.
    let sci = format!("{:.*e}", p - 1, v);
    let (mantissa, exp) = match sci.split_once('e') {
        Some((m, e)) => (m, e.parse::<i32>().unwrap_or(0)),
        None => (sci.as_str(), 0),
    };

    if exp < -4 || exp >= p as i32 {
        let m = strip_trailing_zeros(mantissa);
        let sign = if exp < 0 { '-' } else { '+' };
        format!("{}e{}{:02}", m, sign, exp.abs())
    } else {
        let decimals = (p as i32 - 1 - exp).max(0) as usize;
        let fixed = format!("{:.*}", decimals, v);
        strip_trailing_zeros(&fixed).to_string()
    }
}

/// Format `v` (single precision) like `std::ostream << v` with `precision`
/// significant digits.
pub fn format_g_f32(v: f32, precision: usize) -> String {
    format_g(v as f64, precision)
}

fn strip_trailing_zeros(s: &str) -> &str {
    if s.contains('.') {
        let s = s.trim_end_matches('0');
        s.trim_end_matches('.')
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_g_values() {
        assert_eq!(format_g(0.0, 7), "0");
        assert_eq!(format_g(-0.0, 7), "-0");
        assert_eq!(format_g(1.0, 7), "1");
        assert_eq!(format_g(-2.5, 7), "-2.5");
        assert_eq!(format_g(0.18, 7), "0.18");
        assert_eq!(format_g(0.1 + 0.2, 7), "0.3");
        assert_eq!(format_g(0.0001, 7), "0.0001");
        assert_eq!(format_g(0.00001, 7), "1e-05");
        assert_eq!(format_g(1234567.0, 7), "1234567");
        assert_eq!(format_g(12345678.0, 7), "1.234568e+07");
        assert_eq!(format_g(1e100, 7), "1e+100");
        assert_eq!(format_g(0.435, 7), "0.435");
        assert_eq!(format_g(1.0 / 3.0, 7), "0.3333333");
        assert_eq!(format_g(2.0 / 3.0, 16), "0.6666666666666666");
        assert_eq!(format_g(f64::NAN, 7), "nan");
        assert_eq!(format_g(f64::INFINITY, 7), "inf");
        assert_eq!(format_g(f64::NEG_INFINITY, 7), "-inf");
        assert_eq!(format_g(9.9999999, 7), "10");
        assert_eq!(format_g(0.1f32 as f64, 7), "0.1");
    }
}
