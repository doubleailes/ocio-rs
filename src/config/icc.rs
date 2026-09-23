//! Minimal ICC profile reader extracting the profile description (port of
//! `GetProfileDescriptionFromICCProfile` and of the SampleICC description
//! readers), used to instantiate displays from ICC profiles.

use crate::error::{Error, Result};

fn be32(d: &[u8], off: usize) -> Option<u32> {
    d.get(off..off + 4)
        .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

fn be16(d: &[u8], off: usize) -> Option<u16> {
    d.get(off..off + 2)
        .map(|b| u16::from_be_bytes([b[0], b[1]]))
}

fn icc_error(msg: &str, file: &str) -> Error {
    Error::msg(format!("Error parsing .icc file ({file}).  {msg}"))
}

const SIG_DESC: u32 = 0x6465_7363; // 'desc'
const SIG_DSCM: u32 = 0x6473_636d; // 'dscm'
const TYPE_MLUC: u32 = 0x6d6c_7563; // 'mluc'
const MAGIC: u32 = 0x6163_7370; // 'acsp'

fn cstr(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

fn read_text_description(d: &[u8], off: usize, size: usize) -> Option<String> {
    if 12 > size {
        return None;
    }
    let len = be32(d, off + 8)? as usize;
    if len == 0 {
        return Some(String::new());
    }
    let bytes = d.get(off + 12..off + 12 + len)?;
    Some(cstr(bytes))
}

fn read_mluc(d: &[u8], off: usize, size: usize) -> Option<String> {
    if 16 > size {
        return None;
    }
    let num = be32(d, off + 8)? as usize;
    let rec_size = be32(d, off + 12)?;
    if rec_size != 12 {
        return None;
    }
    let (mut us, mut uk, mut en, mut first) =
        (String::new(), String::new(), String::new(), String::new());
    // As in SampleICC, the strings are read sequentially after each record.
    let mut pos = off + 16;
    for i in 0..num {
        if 16 + (i + 1) * 12 > size {
            return None;
        }
        let lang = be16(d, pos)?;
        let region = be16(d, pos + 2)?;
        let length = be32(d, pos + 4)? as usize;
        let offset = be32(d, pos + 8)? as usize;
        pos += 12;
        if offset + length > size {
            return None;
        }
        let n = length / 2;
        let mut s = Vec::with_capacity(n);
        for c in 0..n {
            s.push(be16(d, pos + 2 * c)? as u8);
        }
        pos += 2 * n;
        let s = cstr(&s);
        if region == 0x5553 {
            us = s;
            break;
        }
        if region == 0x554b && uk.is_empty() {
            uk = s.clone();
        }
        if lang == 0x656e && en.is_empty() {
            en = s.clone();
        }
        if i == 0 {
            first = s;
        }
    }
    Some(if !us.is_empty() {
        us
    } else if !uk.is_empty() {
        uk
    } else if !en.is_empty() {
        en
    } else {
        first
    })
}

/// The description of an ICC profile (the file name when missing).
pub(crate) fn profile_description(path: &str) -> Result<String> {
    let d = std::fs::read(path).map_err(|_| {
        Error::msg(format!(
            "The specified file '{path}' could not be opened. Please confirm the file exists with appropriate read permissions."
        ))
    })?;
    if d.len() < 132 {
        return Err(icc_error("Error loading header.", path));
    }
    if be32(&d, 36) != Some(MAGIC) {
        return Err(icc_error("Wrong magic number.", path));
    }
    let count =
        be32(&d, 128).ok_or_else(|| icc_error("Error loading number of tags.", path))? as usize;
    if count > 100 {
        return Err(icc_error("Too many tags in ICC profile.", path));
    }
    let mut tags = Vec::with_capacity(count);
    for i in 0..count {
        let base = 132 + 12 * i;
        match (be32(&d, base), be32(&d, base + 4), be32(&d, base + 8)) {
            (Some(s), Some(o), Some(z)) => tags.push((s, o as usize, z as usize)),
            _ => {
                return Err(icc_error(
                    "Error loading tag offset table from header.",
                    path,
                ))
            }
        }
    }
    let find = |sig: u32| tags.iter().find(|t| t.0 == sig).copied();
    let tag = find(SIG_DSCM).or_else(|| find(SIG_DESC));
    let desc = match tag {
        None => String::new(),
        Some((_, off, size)) => {
            let ty = be32(&d, off)
                .ok_or_else(|| icc_error("The 'desc' (or 'dcsm') reader is missing.", path))?;
            let r = match ty {
                SIG_DESC => read_text_description(&d, off, size),
                TYPE_MLUC => read_mluc(&d, off, size),
                _ => return Err(icc_error("The 'desc' (or 'dcsm') reader is missing.", path)),
            };
            r.unwrap_or_default()
        }
    };
    if desc.is_empty() {
        return Ok(crate::path_utils::basename(path));
    }
    Ok(desc)
}
