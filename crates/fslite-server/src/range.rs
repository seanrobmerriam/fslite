//! Pure-function resolution of the HTTP `Range: bytes=...` header against a
//! known content length. No axum/HTTP types are involved here — this module
//! is unit-tested standalone in `tests/range.rs`.

use fslite_core::ByteRange;

/// Why an HTTP `Range` header could not be resolved to a concrete `ByteRange`.
#[derive(Debug, Eq, PartialEq)]
pub enum RangeError {
    /// The header was not a well-formed single `bytes=` range.
    Malformed,
    /// The header requested more than one range; unsupported.
    MultiRangeUnsupported,
    /// The requested range starts at or beyond the content length.
    Unsatisfiable,
}

/// Resolves a single-range `Range: bytes=...` header value (without the
/// leading header name) against a known content length. Supports
/// `start-end` (inclusive end), `start-` (open-ended), and `-suffix_len`
/// (last `suffix_len` bytes, clamped to the content length).
pub fn resolve_range(header: &str, logical_size: u64) -> Result<ByteRange, RangeError> {
    let spec = header.strip_prefix("bytes=").ok_or(RangeError::Malformed)?;
    if spec.contains(',') {
        return Err(RangeError::MultiRangeUnsupported);
    }

    let (start_str, end_str) = spec.split_once('-').ok_or(RangeError::Malformed)?;

    if start_str.is_empty() {
        // Suffix range: "-N" = last N bytes.
        let suffix_len: u64 = end_str.parse().map_err(|_| RangeError::Malformed)?;
        if suffix_len == 0 {
            return Err(RangeError::Malformed);
        }
        let start = logical_size.saturating_sub(suffix_len);
        return Ok(ByteRange::new(start, logical_size));
    }

    let start: u64 = start_str.parse().map_err(|_| RangeError::Malformed)?;
    if start >= logical_size {
        return Err(RangeError::Unsatisfiable);
    }

    let end = if end_str.is_empty() {
        logical_size
    } else {
        let inclusive_end: u64 = end_str.parse().map_err(|_| RangeError::Malformed)?;
        inclusive_end.saturating_add(1).min(logical_size)
    };

    Ok(ByteRange::new(start, end))
}
