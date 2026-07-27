use fslite_server::range::{resolve_range, RangeError};

#[test]
fn fully_specified_range_is_inclusive_end_converted_to_exclusive() {
    let range = resolve_range("bytes=0-9", 100).unwrap();
    assert_eq!(range.start, 0);
    assert_eq!(range.end, 10);
}

#[test]
fn open_ended_range_extends_to_the_logical_size() {
    let range = resolve_range("bytes=90-", 100).unwrap();
    assert_eq!(range.start, 90);
    assert_eq!(range.end, 100);
}

#[test]
fn suffix_range_takes_the_last_n_bytes() {
    let range = resolve_range("bytes=-10", 100).unwrap();
    assert_eq!(range.start, 90);
    assert_eq!(range.end, 100);
}

#[test]
fn suffix_longer_than_the_file_clamps_to_the_whole_file() {
    let range = resolve_range("bytes=-1000", 100).unwrap();
    assert_eq!(range.start, 0);
    assert_eq!(range.end, 100);
}

#[test]
fn start_beyond_the_file_is_unsatisfiable() {
    assert!(matches!(resolve_range("bytes=200-300", 100), Err(RangeError::Unsatisfiable)));
}

#[test]
fn multiple_ranges_are_rejected() {
    assert!(matches!(resolve_range("bytes=0-9,20-29", 100), Err(RangeError::MultiRangeUnsupported)));
}

#[test]
fn malformed_unit_is_rejected() {
    assert!(matches!(resolve_range("items=0-9", 100), Err(RangeError::Malformed)));
}

#[test]
fn end_beyond_the_file_is_clamped_to_the_logical_size() {
    let range = resolve_range("bytes=50-1000", 100).unwrap();
    assert_eq!(range.start, 50);
    assert_eq!(range.end, 100);
}

#[test]
fn range_against_an_empty_file_is_unsatisfiable() {
    assert!(matches!(resolve_range("bytes=0-9", 0), Err(RangeError::Unsatisfiable)));
}

#[test]
fn zero_length_suffix_is_malformed() {
    assert!(matches!(resolve_range("bytes=-0", 100), Err(RangeError::Malformed)));
}

#[test]
fn non_numeric_start_is_malformed() {
    assert!(matches!(resolve_range("bytes=abc-9", 100), Err(RangeError::Malformed)));
}

#[test]
fn missing_dash_is_malformed() {
    assert!(matches!(resolve_range("bytes=100", 100), Err(RangeError::Malformed)));
}

#[test]
fn single_byte_range_is_satisfiable_at_the_last_offset() {
    let range = resolve_range("bytes=99-99", 100).unwrap();
    assert_eq!(range.start, 99);
    assert_eq!(range.end, 100);
}
