use fslite_core::LinkTarget;

#[test]
fn normalizes_absolute_link_targets() {
    let target = LinkTarget::parse("//docs/./drafts/../notes.txt").unwrap();

    assert_eq!(
        (target.as_str(), target.is_absolute()),
        ("/docs/notes.txt", true)
    );
}

#[test]
fn normalizes_relative_link_targets_without_discarding_leading_parents() {
    let target = LinkTarget::parse("../../docs/./drafts/../notes.txt").unwrap();

    assert_eq!(
        (target.as_str(), target.is_absolute()),
        ("../../docs/notes.txt", false)
    );
}

#[test]
fn rejects_empty_and_nul_link_targets() {
    for input in ["", "notes\0.txt"] {
        assert!(LinkTarget::parse(input).is_err(), "{input:?}");
    }
}
