use std::collections::{BTreeSet, HashSet};

use fslite_core::VirtualPath;
use proptest::prelude::*;

#[test]
fn normalizes_absolute_paths() {
    for (input, expected) in [
        ("/", "/"),
        ("//a///b", "/a/b"),
        ("/a/./b", "/a/b"),
        ("/a/c/../b", "/a/b"),
        ("/../../a", "/a"),
    ] {
        assert_eq!(VirtualPath::parse(input).unwrap().as_str(), expected);
    }
}

#[test]
fn rejects_non_absolute_and_invalid_paths() {
    for input in ["relative", "/a\0b"] {
        assert!(VirtualPath::parse(input).is_err(), "{input:?}");
    }
}

#[test]
fn joins_without_workspace_escape() {
    let base = VirtualPath::parse("/a/b").unwrap();
    assert_eq!(base.join("../../c").unwrap().as_str(), "/c");
}

#[test]
fn rejects_absolute_and_invalid_joins() {
    let base = VirtualPath::root();

    for input in ["/absolute", "a\0b"] {
        assert!(base.join(input).is_err(), "{input:?}");
    }
}

#[test]
fn root_has_no_name_parent_or_segments() {
    let root = VirtualPath::root();

    assert_eq!(root.as_str(), "/");
    assert_eq!(root.name(), None);
    assert_eq!(root.parent(), None);
    assert_eq!(root.segments().count(), 0);
}

#[test]
fn exposes_canonical_name_parent_and_segments() {
    let path = VirtualPath::parse("/a/b/c").unwrap();

    assert_eq!(path.name(), Some("c"));
    assert_eq!(path.parent().unwrap().as_str(), "/a/b");
    assert_eq!(path.segments().collect::<Vec<_>>(), ["a", "b", "c"]);
}

#[test]
fn display_and_as_ref_expose_the_canonical_form() {
    let path = VirtualPath::parse("//a/./b").unwrap();

    assert_eq!(path.to_string(), "/a/b");
    assert_eq!(path.as_ref(), "/a/b");
}

#[test]
fn serialization_uses_the_canonical_form_and_deserialization_normalizes() {
    let path = VirtualPath::parse("//a/./b").unwrap();
    let serialized = serde_json::to_string(&path).unwrap();
    let deserialized: VirtualPath = serde_json::from_str("\"//a/c/../b\"").unwrap();

    assert_eq!(serialized, "\"/a/b\"");
    assert_eq!(deserialized, path);
}

#[test]
fn equality_ordering_and_hashing_use_the_canonical_form() {
    let normalized = VirtualPath::parse("/a/b").unwrap();
    let equivalent = VirtualPath::parse("//a/./b").unwrap();
    let mut ordered = BTreeSet::new();
    let mut hashed = HashSet::new();

    ordered.insert(normalized.clone());
    ordered.insert(equivalent.clone());
    hashed.insert(normalized);
    hashed.insert(equivalent);

    assert_eq!(ordered.len(), 1);
    assert_eq!(hashed.len(), 1);
}

proptest! {
    #[test]
    fn normalization_is_idempotent_for_valid_segment_lists(
        segments in prop::collection::vec("[A-Za-z0-9_-]{1,16}", 0..32),
    ) {
        let input = format!("/{}", segments.join("//./"));
        let normalized = VirtualPath::parse(&input).unwrap();

        prop_assert_eq!(
            VirtualPath::parse(normalized.as_str()).unwrap(),
            normalized,
        );
    }
}
