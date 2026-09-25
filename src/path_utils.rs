//! Path helpers mirroring the `pystring::os::path` functions used by OCIO.

use std::path::Path;

/// Separator of the search path strings. As in OCIO, it is `:` on every
/// platform (`Context::setSearchPath` / `addSearchPath`); on Windows, use
/// separate search paths (e.g. the YAML list form) for drive letter paths.
pub const SEARCH_PATH_SEPARATOR: &str = ":";

/// Split a search path string on `:` (see [`SEARCH_PATH_SEPARATOR`]).
pub fn split_search_path(path: &str) -> Vec<&str> {
    path.split(':').collect()
}

/// True if the path is absolute (`pystring::os::path::isabs`).
pub fn is_absolute(p: &str) -> bool {
    #[cfg(windows)]
    {
        isabs_nt(p)
    }
    #[cfg(not(windows))]
    {
        p.starts_with('/') || Path::new(p).is_absolute()
    }
}

/// Join two paths (`b` wins if absolute), as `pystring::os::path::join`.
pub fn join(a: &str, b: &str) -> String {
    #[cfg(windows)]
    {
        join_nt(a, b)
    }
    #[cfg(not(windows))]
    {
        join_posix(a, b)
    }
}

/// Normalize a path: collapse `//`, `.` and `a/..`
/// (`pystring::os::path::normpath`; on Windows the separators become `\`).
pub fn normpath(p: &str) -> String {
    #[cfg(windows)]
    {
        normpath_nt(p)
    }
    #[cfg(not(windows))]
    {
        normpath_posix(p)
    }
}

#[cfg_attr(windows, allow(dead_code))]
fn join_posix(a: &str, b: &str) -> String {
    if b.is_empty() {
        return a.to_string();
    }
    if a.is_empty() || b.starts_with('/') || Path::new(b).is_absolute() {
        return b.to_string();
    }
    if a.ends_with('/') || a.ends_with('\\') {
        format!("{a}{b}")
    } else {
        format!("{a}/{b}")
    }
}

/// Normalize a path with `/` separators on every platform (also converting
/// `\`), keeping a Windows drive letter.
pub fn normpath_posix(p: &str) -> String {
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

/// Split a Windows path into the drive (`"C:"` or `""`) and the rest
/// (`pystring::os::path::splitdrive_nt`).
fn splitdrive_nt(p: &str) -> (&str, &str) {
    if p.len() >= 2 && p.as_bytes()[1] == b':' && p.is_char_boundary(2) {
        (&p[..2], &p[2..])
    } else {
        ("", p)
    }
}

/// `pystring::os::path::isabs_nt`: a separator must follow the drive, so
/// `C:lut.cube` is relative.
#[cfg_attr(not(windows), allow(dead_code))]
fn isabs_nt(p: &str) -> bool {
    let (_, path) = splitdrive_nt(p);
    path.starts_with('/') || path.starts_with('\\')
}

/// `pystring::os::path::join_nt` for two paths.
#[cfg_attr(not(windows), allow(dead_code))]
fn join_nt(a: &str, b: &str) -> String {
    let mut path = a.to_string();
    let mut b_nts = false;
    if path.is_empty() {
        b_nts = true;
    } else if isabs_nt(b) {
        let pb = path.as_bytes();
        let bb = b.as_bytes();
        if (pb.len() >= 2 && pb[1] != b':') || (bb.len() >= 2 && bb[1] == b':') {
            // The path does not start with a drive letter.
            b_nts = true;
        } else if path.len() > 3
            || (path.len() == 3 && !path.ends_with('/') && !path.ends_with('\\'))
        {
            // The path has a drive letter and b does not, but is absolute.
            b_nts = true;
        }
    }
    if b_nts {
        return b.to_string();
    }
    let b_sep = b.starts_with('/') || b.starts_with('\\');
    if path.ends_with('/') || path.ends_with('\\') {
        path.push_str(if b_sep { &b[1..] } else { b });
    } else if path.ends_with(':') {
        path.push_str(b);
    } else if !b.is_empty() {
        if !b_sep {
            path.push('\\');
        }
        path.push_str(b);
    } else {
        path.push('\\');
    }
    path
}

/// `pystring::os::path::normpath_nt`: `A//B`, `A/./B` and `A/foo/../B` all
/// become `A\B`. The leading separators of a path without a drive letter are
/// kept (UNC paths such as `\\server\share`).
#[cfg_attr(not(windows), allow(dead_code))]
fn normpath_nt(p: &str) -> String {
    let replaced = p.replace('/', "\\");
    let (drive, mut path) = splitdrive_nt(&replaced);
    let mut prefix = drive.to_string();
    if prefix.is_empty() {
        // No drive letter: preserve the initial backslashes.
        while let Some(rest) = path.strip_prefix('\\') {
            prefix.push('\\');
            path = rest;
        }
    } else if path.starts_with('\\') {
        // A drive letter: collapse the initial backslashes.
        prefix.push('\\');
        path = path.trim_start_matches('\\');
    }
    let mut comps: Vec<&str> = path.split('\\').collect();
    let mut i = 0;
    while i < comps.len() {
        if comps[i].is_empty() || comps[i] == "." {
            comps.remove(i);
        } else if comps[i] == ".." {
            if i > 0 && comps[i - 1] != ".." {
                comps.drain(i - 1..=i);
                i -= 1;
            } else if i == 0 && prefix.ends_with('\\') {
                comps.remove(i);
            } else {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    if prefix.is_empty() && comps.is_empty() {
        comps.push(".");
    }
    prefix + &comps.join("\\")
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
        assert_eq!(normpath_posix("/a//b/./c/../d"), "/a/b/d");
        assert_eq!(normpath_posix("a/../../b"), "../b");
        assert_eq!(normpath_posix("./"), ".");
        assert_eq!(join_posix("/a/b", "c.lut"), "/a/b/c.lut");
        assert_eq!(join_posix("/a/b", "/c.lut"), "/c.lut");
        assert_eq!(extension("/x/y/lut.SPI1D"), "spi1d");
        assert_eq!(dirname("/x/y/lut.spi1d"), "/x/y");
        assert_eq!(split_search_path(".:luts"), vec![".", "luts"]);
    }

    // The Windows variants are tested on every platform.
    #[test]
    fn isabs_windows() {
        assert!(isabs_nt("C:/luts/a.cube"));
        assert!(isabs_nt("C:\\luts\\a.cube"));
        assert!(isabs_nt("\\\\server\\share\\a.cube"));
        assert!(isabs_nt("/luts/a.cube"));
        // Drive relative paths are relative.
        assert!(!isabs_nt("C:lut.cube"));
        assert!(!isabs_nt("C:"));
        assert!(!isabs_nt("luts/a.cube"));
    }

    #[test]
    fn join_windows() {
        assert_eq!(join_nt("C:\\a", "b.cube"), "C:\\a\\b.cube");
        assert_eq!(join_nt("C:\\a\\", "b.cube"), "C:\\a\\b.cube");
        assert_eq!(join_nt("C:/a/", "/b"), "/b");
        assert_eq!(join_nt("c:", "/a"), "c:/a");
        assert_eq!(join_nt("c:/", "/a"), "c:/a");
        assert_eq!(join_nt("c:/a", "/b"), "/b");
        assert_eq!(join_nt("c:", "d:/"), "d:/");
        assert_eq!(join_nt("c:/", "d:/"), "d:/");
        assert_eq!(join_nt("C:", "lut.cube"), "C:lut.cube");
        assert_eq!(join_nt("", "lut.cube"), "lut.cube");
        assert_eq!(join_nt("a", ""), "a\\");
    }

    #[test]
    fn normpath_windows() {
        assert_eq!(
            normpath_nt("D:\\a\\ocio-rs/tests/./data//files/../x.spimtx"),
            "D:\\a\\ocio-rs\\tests\\data\\x.spimtx"
        );
        // UNC paths keep their two leading separators.
        assert_eq!(
            normpath_nt("\\\\server\\share/luts/../a.cube"),
            "\\\\server\\share\\a.cube"
        );
        assert_eq!(
            normpath_nt("//server/share/a.cube"),
            "\\\\server\\share\\a.cube"
        );
        assert_eq!(normpath_nt("C://a"), "C:\\a");
        assert_eq!(normpath_nt("C:\\..\\a"), "C:\\a");
        assert_eq!(normpath_nt("a/../../b"), "..\\b");
        assert_eq!(normpath_nt("./"), ".");
        assert_eq!(normpath_nt("C:"), "C:");
    }
}
