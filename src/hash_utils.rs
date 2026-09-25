//! Cache identifier hashes (port of `HashUtils.cpp`): XXH3 128-bit hashes
//! formatted as OCIO does.

use xxhash_rust::xxh3::xxh3_128;

/// 32 hex characters: the low 64 bits then the high 64 bits (port of
/// `CacheIDHash`).
pub fn cache_id_hash(data: &[u8]) -> String {
    let h = xxh3_128(data);
    format!("{:016x}{:016x}", h as u64, (h >> 64) as u64)
}

/// UUID formatted hash `8-4-4-4-12` of the high then low 64 bits (port of
/// `CacheIDHashUUID`).
pub fn cache_id_hash_uuid(data: &[u8]) -> String {
    let h = xxh3_128(data);
    let hex = format!("{:016x}{:016x}", (h >> 64) as u64, h as u64);
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        let id = cache_id_hash(b"abc");
        assert_eq!(id.len(), 32);
        let uuid = cache_id_hash_uuid(b"abc");
        assert_eq!(uuid.len(), 36);
        assert_eq!(&uuid[8..9], "-");
        // The UUID holds the same 64-bit halves in the opposite order.
        assert_eq!(uuid.replace('-', ""), format!("{}{}", &id[16..], &id[..16]));
    }
}
