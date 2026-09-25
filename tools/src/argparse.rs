//! Command line parsing (port of the OCIO `apputils/argparse`, itself derived
//! from the OpenImageIO `ArgParse`).
//!
//! Options are declared with scanf-like format strings such as
//! `"--iconfig %s"` or `"--slope %f %f %f"`. Each parameter of an option is
//! stored under a *key*; several options may share the same keys (e.g. both
//! `--view` and `--displayview` set the same variables), exactly like the C++
//! version where several options point to the same variables. The values are
//! read back with the typed getters after [`ArgParse::parse`].
//!
//! Arguments that are neither an option nor an option parameter are collected
//! in [`ArgParse::args`] when a `"%*"` catch-all option is declared (the C++
//! "global" callback), otherwise they are an error.

use std::collections::HashMap;

/// A parsed parameter value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// `%d` parameter (parsed with `atoi`).
    Int(i32),
    /// `%f` / `%g` parameter (parsed with `atof`).
    Float(f32),
    /// `%F` parameter (parsed with `atof`).
    Double(f64),
    /// `%s` parameter.
    Str(String),
    /// `%L` parameter: every occurrence is appended.
    List(Vec<String>),
    /// A flag option.
    Bool(bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OptionType {
    None,
    Regular,
    Flag,
    Sublist,
}

#[derive(Debug, Clone)]
struct ArgOption {
    format: String,
    flag: String,
    code: Vec<char>,
    description: String,
    ty: OptionType,
    keys: Vec<String>,
    repetitions: usize,
}

impl ArgOption {
    fn new(format: &str, keys: &[&str], description: &str) -> Self {
        let mut opt = ArgOption {
            format: format.to_string(),
            flag: String::new(),
            code: Vec::new(),
            description: description.to_string(),
            ty: OptionType::None,
            keys: keys.iter().map(|k| k.to_string()).collect(),
            repetitions: 0,
        };
        opt.initialize();
        opt
    }

    /// Port of `ArgOption::initialize`.
    fn initialize(&mut self) {
        if self.format.is_empty() || self.format == "%*" {
            self.ty = OptionType::Sublist;
            self.code = vec!['*'];
            self.flag.clear();
        } else if self.format == "<SEPARATOR>" {
            // Nothing to do.
        } else {
            let bytes = self.format.as_bytes();
            let mut s = 1usize;
            if bytes.get(s) == Some(&b'-') {
                s += 1;
            }
            while s < bytes.len()
                && (bytes[s].is_ascii_alphanumeric() || bytes[s] == b'_' || bytes[s] == b'-')
            {
                s += 1;
            }
            if s >= bytes.len() {
                self.flag = self.format.clone();
                self.ty = OptionType::Flag;
                self.code = vec!['b'];
            } else {
                self.flag = self.format[..s].to_string();
                self.ty = OptionType::Regular;
                let rest: Vec<char> = self.format[s..].chars().collect();
                let mut i = 0;
                while i < rest.len() {
                    if rest[i] == '%' && i + 1 < rest.len() {
                        i += 1;
                        match rest[i] {
                            'd' | 'g' | 'f' | 'F' | 's' | 'L' => self.code.push(rest[i]),
                            '*' => self.ty = OptionType::Sublist,
                            _ => {}
                        }
                    }
                    i += 1;
                }
            }
        }
    }

    fn parameter_count(&self) -> usize {
        self.code.len()
    }
}

/// Command line parser (port of `ArgParse`).
#[derive(Debug, Clone, Default)]
pub struct ArgParse {
    intro: String,
    options: Vec<ArgOption>,
    has_global: bool,
    values: HashMap<String, Value>,
    args: Vec<String>,
    command_line: Vec<String>,
    error: String,
}

impl ArgParse {
    /// Create a parser with the introductory usage message.
    pub fn new(intro: &str) -> Self {
        ArgParse {
            intro: intro.to_string(),
            ..Default::default()
        }
    }

    /// Declare an option. `format` is the option name followed by its
    /// scanf-like parameters (`%d`, `%f`, `%g`, `%F`, `%s`, `%L`); a format
    /// without parameter is a flag. `keys` names where each parameter (or
    /// the flag) is stored.
    pub fn option(mut self, format: &str, keys: &[&str], description: &str) -> Self {
        let opt = ArgOption::new(format, keys, description);
        if opt.ty == OptionType::Sublist && opt.flag.is_empty() {
            self.has_global = true;
        }
        self.options.push(opt);
        self
    }

    /// Declare a flag option storing `true` under `key` when present.
    pub fn flag(self, format: &str, key: &str, description: &str) -> Self {
        self.option(format, &[key], description)
    }

    /// Declare the catch-all option (`"%*"`): arguments which are not
    /// options are collected in [`ArgParse::args`].
    pub fn positional(self, description: &str) -> Self {
        self.option("%*", &[], description)
    }

    /// Add a separator line to the usage message.
    pub fn separator(self, description: &str) -> Self {
        self.option("<SEPARATOR>", &[], description)
    }

    fn find_option(&self, name: &str) -> Option<usize> {
        let nb = name.as_bytes();
        for (idx, o) in self.options.iter().enumerate() {
            let opt = o.flag.as_str();
            let ob = opt.as_bytes();
            if name == opt {
                return Some(idx);
            }
            // Match even if the user mixes up one dash or two.
            if nb.len() > 1
                && nb[0] == b'-'
                && nb[1] == b'-'
                && ob.len() > 1
                && ob[0] == b'-'
                && ob[1] != b'-'
                && &name[1..] == opt
            {
                return Some(idx);
            }
            if nb.len() > 1
                && nb[0] == b'-'
                && nb[1] != b'-'
                && ob.len() > 1
                && ob[0] == b'-'
                && ob[1] == b'-'
                && name == &opt[1..]
            {
                return Some(idx);
            }
        }
        None
    }

    fn set_parameter(&mut self, opt_idx: usize, i: usize, arg: Option<&str>) {
        let opt = &self.options[opt_idx];
        let key = match opt.keys.get(i) {
            Some(k) => k.clone(),
            None => return,
        };
        let code = opt.code[i];
        let value = match code {
            'd' => Value::Int(arg.map(atoi).unwrap_or(0)),
            'f' | 'g' => Value::Float(arg.map(|a| atof(a) as f32).unwrap_or(0.0)),
            'F' => Value::Double(arg.map(atof).unwrap_or(0.0)),
            's' => Value::Str(arg.unwrap_or("").to_string()),
            'L' => {
                let mut list = match self.values.remove(&key) {
                    Some(Value::List(l)) => l,
                    _ => Vec::new(),
                };
                list.push(arg.unwrap_or("").to_string());
                Value::List(list)
            }
            _ => Value::Bool(true),
        };
        self.values.insert(key, value);
    }

    /// Parse the command line (`args[0]` is the program name). Returns
    /// `Err(())` on a malformed command line, see [`ArgParse::geterror`].
    #[allow(clippy::result_unit_err)]
    pub fn parse(&mut self, argv: &[String]) -> Result<(), ()> {
        self.command_line = argv.to_vec();
        let argc = argv.len();
        let mut i = 1;
        while i < argc {
            let arg = argv[i].as_str();
            let b = arg.as_bytes();
            if b.len() > 1 && b[0] == b'-' && (b[1].is_ascii_alphabetic() || b[1] == b'-') {
                let idx = match self.find_option(arg) {
                    Some(idx) => idx,
                    None => {
                        self.error = format!("Invalid option \"{arg}\"");
                        return Err(());
                    }
                };
                self.options[idx].repetitions += 1;
                match self.options[idx].ty {
                    OptionType::Flag => self.set_parameter(idx, 0, None),
                    OptionType::Regular => {
                        let count = self.options[idx].parameter_count();
                        for j in 0..count {
                            if j + i + 1 >= argc {
                                self.error = format!(
                                    "Missing parameter {} from option \"{}\"",
                                    j + 1,
                                    self.options[idx].flag
                                );
                                return Err(());
                            }
                            let param = argv[i + j + 1].clone();
                            self.set_parameter(idx, j, Some(&param));
                        }
                        i += count;
                    }
                    _ => {}
                }
            } else if self.has_global {
                self.args.push(arg.to_string());
            } else {
                self.error = format!("Argument \"{arg}\" does not have an associated option");
                return Err(());
            }
            i += 1;
        }
        Ok(())
    }

    /// The error message of the last failed parse (cleared by the call).
    pub fn geterror(&mut self) -> String {
        std::mem::take(&mut self.error)
    }

    /// The usage message (what `ArgParse::usage` prints to stdout).
    pub fn usage(&self) -> String {
        const LONGLINE: usize = 40;
        let mut out = String::new();
        out.push_str(&self.intro);
        out.push('\n');
        let maxlen = self
            .options
            .iter()
            .map(|o| o.format.len())
            .filter(|l| *l < LONGLINE)
            .max()
            .unwrap_or(0);
        for opt in &self.options {
            if opt.description.is_empty() {
                continue;
            }
            let fmtlen = opt.format.len();
            if opt.format == "<SEPARATOR>" {
                out.push_str(&opt.description);
                out.push('\n');
            } else if fmtlen < LONGLINE {
                out.push_str("    ");
                out.push_str(&opt.format);
                out.push_str(&" ".repeat(maxlen + 2 - fmtlen));
                out.push_str(&opt.description);
                out.push('\n');
            } else {
                out.push_str("    ");
                out.push_str(&opt.format);
                out.push_str("\n    ");
                out.push_str(&" ".repeat(maxlen + 2));
                out.push_str(&opt.description);
                out.push('\n');
            }
        }
        out
    }

    /// Print the usage message to stdout.
    pub fn print_usage(&self) {
        print!("{}", self.usage());
    }

    /// The whole command line as one string (arguments containing spaces
    /// are quoted).
    pub fn command_line(&self) -> String {
        self.command_line
            .iter()
            .map(|a| {
                if a.contains(' ') {
                    format!("\"{a}\"")
                } else {
                    a.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Number of times the option was found on the command line.
    pub fn found(&self, option_name: &str) -> usize {
        self.find_option(option_name)
            .map(|i| self.options[i].repetitions)
            .unwrap_or(0)
    }

    /// The arguments not associated with any option.
    pub fn args(&self) -> &[String] {
        &self.args
    }

    /// Raw value stored under `key`.
    pub fn value(&self, key: &str) -> Option<&Value> {
        self.values.get(key)
    }

    /// Flag value (false when absent).
    pub fn get_bool(&self, key: &str) -> bool {
        matches!(self.values.get(key), Some(Value::Bool(true)))
    }

    /// String value, or `default` when absent.
    pub fn get_string(&self, key: &str, default: &str) -> String {
        match self.values.get(key) {
            Some(Value::Str(s)) => s.clone(),
            _ => default.to_string(),
        }
    }

    /// Integer value, or `default` when absent.
    pub fn get_int(&self, key: &str, default: i32) -> i32 {
        match self.values.get(key) {
            Some(Value::Int(v)) => *v,
            _ => default,
        }
    }

    /// Float value, or `default` when absent.
    pub fn get_float(&self, key: &str, default: f32) -> f32 {
        match self.values.get(key) {
            Some(Value::Float(v)) => *v,
            _ => default,
        }
    }

    /// Double value, or `default` when absent.
    pub fn get_double(&self, key: &str, default: f64) -> f64 {
        match self.values.get(key) {
            Some(Value::Double(v)) => *v,
            _ => default,
        }
    }

    /// List value (empty when absent).
    pub fn get_list(&self, key: &str) -> Vec<String> {
        match self.values.get(key) {
            Some(Value::List(v)) => v.clone(),
            _ => Vec::new(),
        }
    }
}

/// Length of the longest prefix of `s` (after leading white spaces) that
/// `strtod` would parse, and the start of the number.
fn float_prefix(s: &str) -> (usize, usize) {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() && (b[i] as char).is_ascii_whitespace() {
        i += 1;
    }
    let start = i;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    let lower = s[i..].to_ascii_lowercase();
    if lower.starts_with("infinity") {
        return (start, i + 8);
    }
    if lower.starts_with("inf") || lower.starts_with("nan") {
        return (start, i + 3);
    }
    let mut digits = 0;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
        digits += 1;
    }
    if i < b.len() && b[i] == b'.' {
        i += 1;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
            digits += 1;
        }
    }
    if digits == 0 {
        return (start, start);
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        let mut j = i + 1;
        if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
            j += 1;
        }
        let exp_start = j;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        if j > exp_start {
            i = j;
        }
    }
    (start, i)
}

/// C `atof` / `strtod`: parse the longest valid prefix, 0 if none.
pub fn atof(s: &str) -> f64 {
    let (start, end) = float_prefix(s);
    if end <= start {
        return 0.0;
    }
    let txt = &s[start..end];
    let lower = txt.to_ascii_lowercase();
    let neg = lower.starts_with('-');
    let body = lower.trim_start_matches(['+', '-']);
    if body.starts_with("inf") {
        return if neg {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
    }
    if body.starts_with("nan") {
        return f64::NAN;
    }
    txt.parse::<f64>().unwrap_or(0.0)
}

/// C `strtof` (the value of [`atof`] converted to `f32`).
pub fn strtof(s: &str) -> f32 {
    atof(s) as f32
}

/// C `atoi`: parse an optional sign followed by digits, 0 if none.
pub fn atoi(s: &str) -> i32 {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() && (b[i] as char).is_ascii_whitespace() {
        i += 1;
    }
    let mut neg = false;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        neg = b[i] == b'-';
        i += 1;
    }
    let mut v: i64 = 0;
    while i < b.len() && b[i].is_ascii_digit() {
        v = (v * 10 + i64::from(b[i] - b'0')).min(i64::from(u32::MAX) + 1);
        i += 1;
    }
    let v = if neg { -v } else { v };
    v.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parse_options() {
        let mut ap = ArgParse::new("intro")
            .positional("")
            .separator("Options:")
            .flag("--help", "help", "Print help message")
            .option("--iconfig %s", &["config"], "Input config")
            .option("--view %s %s %s", &["cs", "display", "view"], "View")
            .option("--cubesize %d", &["cubesize"], "Cube size")
            .option("--slope %f %f %f", &["s0", "s1", "s2"], "slope")
            .option("--attr %L", &["attrs"], "Attributes");
        ap.parse(&argv(&[
            "app",
            "file.exr",
            "-iconfig",
            "config.ocio",
            "--view",
            "a",
            "b",
            "c",
            "--cubesize",
            "33x",
            "--slope",
            "1.5",
            "-2e1",
            "abc",
            "--attr",
            "a=1",
            "--attr",
            "b=2",
            "-0.5",
        ]))
        .unwrap();
        assert!(!ap.get_bool("help"));
        assert_eq!(ap.get_string("config", ""), "config.ocio");
        assert_eq!(ap.get_string("display", ""), "b");
        assert_eq!(ap.get_int("cubesize", -1), 33);
        assert_eq!(ap.get_float("s0", 0.0), 1.5);
        assert_eq!(ap.get_float("s1", 0.0), -20.0);
        assert_eq!(ap.get_float("s2", 1.0), 0.0);
        assert_eq!(ap.get_list("attrs"), vec!["a=1", "b=2"]);
        assert_eq!(ap.args(), &["file.exr".to_string(), "-0.5".to_string()]);
        assert_eq!(ap.found("--view"), 1);
    }

    #[test]
    fn parse_errors() {
        let mut ap = ArgParse::new("intro").flag("--help", "help", "Help");
        assert!(ap.parse(&argv(&["app", "--nope"])).is_err());
        assert_eq!(ap.geterror(), "Invalid option \"--nope\"");
        assert!(ap.geterror().is_empty());
        assert!(ap.parse(&argv(&["app", "positional"])).is_err());
        assert_eq!(
            ap.geterror(),
            "Argument \"positional\" does not have an associated option"
        );

        let mut ap = ArgParse::new("intro").option("--pair %s %s", &["a", "b"], "Pair");
        assert!(ap.parse(&argv(&["app", "--pair", "x"])).is_err());
        assert_eq!(ap.geterror(), "Missing parameter 2 from option \"--pair\"");
    }

    #[test]
    fn usage_layout() {
        let ap = ArgParse::new("tool -- test\n")
            .positional("")
            .separator("Options:")
            .flag("--help", "help", "Print help message")
            .option("--iconfig %s", &["c"], "Input .ocio configuration file")
            .option(
                "--a-very-long-option-name %s %s %s %s %s",
                &["a", "b", "c", "d", "e"],
                "Long",
            );
        assert_eq!(
            ap.usage(),
            "tool -- test\n\n\
             Options:\n\
             \x20   --help        Print help message\n\
             \x20   --iconfig %s  Input .ocio configuration file\n\
             \x20   --a-very-long-option-name %s %s %s %s %s\n\
             \x20                 Long\n"
        );
    }

    #[test]
    fn c_conversions() {
        assert_eq!(atoi("  42abc"), 42);
        assert_eq!(atoi("-7"), -7);
        assert_eq!(atoi("abc"), 0);
        assert_eq!(atof("1e3x"), 1000.0);
        assert_eq!(atof(".5"), 0.5);
        assert_eq!(atof("-"), 0.0);
        assert_eq!(atof("1e"), 1.0);
        assert!(atof("inf").is_infinite());
        assert!(atof("nan").is_nan());
    }
}
