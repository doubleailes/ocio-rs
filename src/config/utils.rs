//! String and number helpers used by the configuration layer (port of the
//! parts of `utils/StringUtils.h` and `ParseUtils.cpp` used by the config).

use crate::error::{Error, Result};

/// ASCII lower case (locale independent, as `StringUtils::Lower`).
pub fn lower(s: &str) -> String {
    s.to_ascii_lowercase()
}

/// Case insensitive comparison (`StringUtils::Compare`).
pub fn compare(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// True for the characters trimmed by `StringUtils::Trim` (space and
/// non-printable characters).
fn is_space(c: char) -> bool {
    let v = c as u32;
    v == 0x20 || (0x09..=0x0d).contains(&v)
}

/// Trim white spaces at both ends (`StringUtils::Trim`).
pub fn trim(s: &str) -> &str {
    s.trim_matches(is_space)
}

/// Trim white spaces at the right (`StringUtils::RightTrim`).
pub fn right_trim(s: &str) -> &str {
    s.trim_end_matches(is_space)
}

/// Split on a separator (`StringUtils::Split`): an empty string gives one
/// empty element and a trailing separator gives a trailing empty element.
pub fn split(s: &str, sep: char) -> Vec<String> {
    if s.is_empty() {
        return vec![String::new()];
    }
    s.split(sep).map(|x| x.to_string()).collect()
}

/// Join with `"<sep> "` (`StringUtils::Join`).
pub fn join(v: &[String], sep: char) -> String {
    let s = format!("{sep} ");
    v.join(&s)
}

/// Split by lines (`StringUtils::SplitByLines`).
pub fn split_by_lines(s: &str) -> Vec<String> {
    if s.is_empty() {
        return vec![String::new()];
    }
    s.lines().map(|l| l.to_string()).collect()
}

/// Case insensitive membership (`StringUtils::Contain`).
pub fn contain(list: &[String], entry: &str) -> bool {
    list.iter().any(|e| compare(e, entry))
}

/// Case insensitive removal of the first matching entry
/// (`StringUtils::Remove`). Returns true if found.
pub fn remove(list: &mut Vec<String>, entry: &str) -> bool {
    if let Some(pos) = list.iter().position(|e| compare(e, entry)) {
        list.remove(pos);
        true
    } else {
        false
    }
}

/// Index of `s` in `vec` (case insensitive), as `FindInStringVecCaseIgnore`.
pub fn find_in_vec_case_ignore(vec: &[String], s: &str) -> Option<usize> {
    vec.iter().position(|e| compare(e, s))
}

/// Strings present in both vectors (case insensitive), keeping the order and
/// the capitalization of `vec1` (`IntersectStringVecsCaseIgnore`).
pub fn intersect_case_ignore(vec1: &[String], vec2: &[String]) -> Vec<String> {
    vec1.iter().filter(|v| vec2.iter().any(|w| compare(v, w))).cloned().collect()
}

/// Find the end of a (possibly quoted) name starting at `start`.
fn find_end_of_name(s: &[u8], start: usize, sep: u8) -> Result<usize> {
    let mut current = start;
    loop {
        match s[current.min(s.len())..].iter().position(|&c| c == b'"' || c == sep) {
            None => return Ok(s.len()),
            Some(off) => {
                let pos = current + off;
                if s[pos] == b'"' {
                    match s[pos + 1..].iter().position(|&c| c == b'"') {
                        None => {
                            return Err(Error::msg(format!(
                                "The string '{}' is not correctly formatted. It is missing a closing quote.",
                                String::from_utf8_lossy(s)
                            )))
                        }
                        Some(off2) => current = pos + 1 + off2 + 1,
                    }
                } else {
                    return Ok(pos);
                }
            }
        }
    }
}

/// Split a list of names separated by commas or colons, names may be quoted
/// (`SplitStringEnvStyle`). An empty string gives one empty element.
pub fn split_string_env_style(s: &str) -> Result<Vec<String>> {
    let s = trim(s);
    if s.is_empty() {
        return Ok(vec![String::new()]);
    }
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let found_comma = s.contains(',');
    let found_colon = s.contains(':');
    if found_comma || found_colon {
        let sep = if found_comma { b',' } else { b':' };
        let mut current = 0usize;
        while current <= bytes.len() {
            let end = find_end_of_name(bytes, current, sep)?;
            if end > current {
                out.push(String::from_utf8_lossy(&bytes[current..end]).into_owned());
                current = end + 1;
            } else {
                out.push(String::new());
                current += 1;
            }
        }
    } else {
        out.push(s.to_string());
    }
    for v in out.iter_mut() {
        let t = trim(v).to_string();
        if t.len() > 1 && t.starts_with('"') && t.ends_with('"') {
            *v = t[1..t.len() - 1].to_string();
        } else {
            *v = t;
        }
    }
    Ok(out)
}

/// Same as [`split_string_env_style`] but never fails (a missing closing
/// quote keeps the raw string).
pub fn split_string_env_style_lossy(s: &str) -> Vec<String> {
    split_string_env_style(s).unwrap_or_else(|_| vec![trim(s).to_string()])
}

fn needs_quotes(v: &str) -> bool {
    v.contains([',', ':']) && v.len() > 1 && !v.starts_with('"') && !v.ends_with('"')
}

/// Join names with `", "`, quoting the ones containing a separator
/// (`JoinStringEnvStyle`).
pub fn join_string_env_style(v: &[String]) -> String {
    let mut out = String::new();
    for (i, e) in v.iter().enumerate() {
        if i != 0 {
            out.push_str(", ");
        }
        if needs_quotes(e) {
            out.push('"');
            out.push_str(e);
            out.push('"');
        } else {
            out.push_str(e);
        }
    }
    out
}

/// True if the string contains `$` or `%` (`ContainsContextVariableToken`).
pub fn contains_context_variable_token(s: &str) -> bool {
    crate::context::contains_context_variable_token(s)
}

/// True if the string contains context variables (`ContainsContextVariables`).
pub fn contains_context_variables(s: &str) -> bool {
    crate::context::contains_context_variables(s)
}

/// Format a double as C++ `std::ostream << value` with the given precision
/// (i.e. `printf("%.<precision>g")`).
pub fn format_g(v: f64, precision: usize) -> String {
    if v.is_nan() {
        return if v.is_sign_negative() { "-nan".to_string() } else { "nan".to_string() };
    }
    if v.is_infinite() {
        return if v < 0.0 { "-inf".to_string() } else { "inf".to_string() };
    }
    let p = precision.max(1);
    if v == 0.0 {
        return if v.is_sign_negative() { "-0".to_string() } else { "0".to_string() };
    }
    let sci = format!("{:.*e}", p - 1, v);
    let (mant, exp) = match sci.split_once('e') {
        Some((m, e)) => (m.to_string(), e.parse::<i32>().unwrap_or(0)),
        None => (sci.clone(), 0),
    };
    if exp < -4 || exp >= p as i32 {
        let mut m = mant;
        if m.contains('.') {
            while m.ends_with('0') {
                m.pop();
            }
            if m.ends_with('.') {
                m.pop();
            }
        }
        let sign = if exp < 0 { '-' } else { '+' };
        format!("{m}e{sign}{:02}", exp.abs())
    } else {
        let decimals = (p as i32 - 1 - exp).max(0) as usize;
        let mut s = format!("{:.*}", decimals, v);
        if s.contains('.') {
            while s.ends_with('0') {
                s.pop();
            }
            if s.ends_with('.') {
                s.pop();
            }
        }
        s
    }
}

/// C++ default stream formatting of a double (precision 6).
pub fn fmt_f64(v: f64) -> String {
    format_g(v, 6)
}

/// C++ default stream formatting of a float (precision 6).
pub fn fmt_f32(v: f32) -> String {
    format_g(v as f64, 6)
}

/// `DoubleToString` (precision 16).
pub fn double_to_string(v: f64) -> String {
    format_g(v, 16)
}

/// `FloatToString` (precision 7).
pub fn float_to_string(v: f32) -> String {
    format_g(v as f64, 7)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn g_format() {
        assert_eq!(format_g(0.2126, 15), "0.2126");
        assert_eq!(format_g(1.0, 15), "1");
        assert_eq!(format_g(2.2, 15), "2.2");
        assert_eq!(format_g(1e-10, 15), "1e-10");
        assert_eq!(format_g(123456789.0, 6), "1.23457e+08");
        assert_eq!(format_g(0.1f32 as f64, 15), "0.100000001490116");
        assert_eq!(format_g(0.1f32 as f64, 7), "0.1");
        assert_eq!(format_g(-0.0001, 6), "-0.0001");
        assert_eq!(format_g(0.00001, 6), "1e-05");
        assert_eq!(format_g(100000.0, 6), "100000");
        assert_eq!(format_g(1000000.0, 6), "1e+06");
    }

    #[test]
    fn env_style() {
        assert_eq!(split_string_env_style("a, b").unwrap(), vec!["a", "b"]);
        assert_eq!(split_string_env_style("a:b").unwrap(), vec!["a", "b"]);
        assert_eq!(split_string_env_style("").unwrap(), vec![""]);
        assert_eq!(split_string_env_style("a,").unwrap(), vec!["a", ""]);
        assert_eq!(split_string_env_style("\"a, b\", c").unwrap(), vec!["a, b", "c"]);
        assert!(split_string_env_style("\"a, b").is_err());
        assert_eq!(join_string_env_style(&["a, b".to_string(), "c".to_string()]), "\"a, b\", c");
    }
}
