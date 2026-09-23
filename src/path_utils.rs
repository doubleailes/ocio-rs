//! Path helpers mirroring the `pystring::os::path` functions used by OCIO.

use std::path::Path;

/// Separator used when joining search paths.
#[cfg(windows)]
pub const SEARCH_PATH_SEPARATOR: &str = ";";
#[cfg(not(windows))]
pub const SEARCH_PATH_SEPARATOR: &str = ":";

/// Split a search path string on `:` (or `;` on Windows).
pub fn split_search_path(path: &str) -> Vec<&str> {
    #[cfg(windows)]
    {
        path.split(';').collect()
    }
    #[cfg(not(windows))]
    {
        path.split(':').collect()
    }
}

/// True if the path is absolute.
pub fn is_absolute(p: &str) -> bool {
    if p.starts_with('/') {
        return true;
    }
    #[cfg(windows)]
    {
        if p.starts_with('\\') {
            return true;
        }
        let b = p.as_bytes();
        if b.len() >= 2 && b[1] == b':' {
            return true;
        }
    }
    Path::new(p).is_absolute()
}

/// Join two paths (`b` wins if absolute).
pub fn join(a: &str, b: &str) -> String {
    if b.is_empty() {
        return a.to_string();
    }
    if a.is_empty() || is_absolute(b) {
        return b.to_string();
    }
    if a.ends_with('/') || a.ends_with('\\') {
        format!("{a}{b}")
    } else {
        format!("{a}/{b}")
    }
}

/// Normalize a path: collapse `//`, `.` and `a/..`.
pub fn normpath(p: &str) -> String {
    if p.is_empty() {
        return ".".to_string();
    }
    let p = p.replace('\\', "/");
    let absolute = p.starts_with('/');
    let mut prefix = String::new();
    let mut rest = p.as_str();
    // Keep Windows drive letters.
    if rest.len() >= 2 && rest.as_bytes()[1] == b':' {
        prefix = rest[..2].to_string();
        rest = &rest[2..];
    }
    let mut parts: Vec<&str> = Vec::new();
    for comp in rest.split('/') {
        match comp {
            "" | "." => {}
            ".." => {
                if let Some(last) = parts.last() {
                    if *last != ".." {
                        parts.pop();
                        continue;
                    }
                }
                if !absolute {
                    parts.push("..");
                }
            }
            c => parts.push(c),
        }
    }
    let body = parts.join("/");
    let mut out = prefix;
    if absolute || rest.starts_with('/') {
        out.push('/');
    }
    out.push_str(&body);
    if out.is_empty() {
        ".".to_string()
    } else {
        out
    }
}

/// True if a regular file exists at `p`.
pub fn file_exists(p: &str) -> bool {
    Path::new(p).is_file()
}

/// Directory part of the path (`""` if none).
pub fn dirname(p: &str) -> String {
    match p.rfind(['/', '\\']) {
        Some(0) => "/".to_string(),
        Some(i) => p[..i].to_string(),
        None => String::new(),
    }
}

/// File name part of the path.
pub fn basename(p: &str) -> String {
    match p.rfind(['/', '\\']) {
        Some(i) => p[i + 1..].to_string(),
        None => p.to_string(),
    }
}

/// Extension of the file (without the dot, lower-cased).
pub fn extension(p: &str) -> String {
    let b = basename(p);
    match b.rfind('.') {
        Some(i) if i > 0 || b.len() > 1 => b[i + 1..].to_ascii_lowercase(),
        _ => String::new(),
    }
}

/// Absolute path relative to the current working directory.
pub fn absolute(p: &str) -> String {
    if is_absolute(p) {
        return normpath(p);
    }
    match std::env::current_dir() {
        Ok(cwd) => normpath(&join(&cwd.to_string_lossy(), p)),
        Err(_) => normpath(p),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn norm() {
        assert_eq!(normpath("/a//b/./c/../d"), "/a/b/d");
        assert_eq!(normpath("a/../../b"), "../b");
        assert_eq!(normpath("./"), ".");
        assert_eq!(join("/a/b", "c.lut"), "/a/b/c.lut");
        assert_eq!(join("/a/b", "/c.lut"), "/c.lut");
        assert_eq!(extension("/x/y/lut.SPI1D"), "spi1d");
        assert_eq!(dirname("/x/y/lut.spi1d"), "/x/y");
    }
}
