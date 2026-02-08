use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use fslite_core::{ErrorCode, MutationOptions, RequestContext, Revision, VirtualPath};
use fslite_sqlite::SqliteFileSystem;
use serde_json::Value;

fn path(input: &str) -> VirtualPath {
    VirtualPath::parse(input).unwrap()
}

fn decoded(node: &fslite_core::Node, key: &str) -> Vec<u8> {
    let Value::String(encoded) = &node.attributes[key] else {
        panic!("attribute {key} is not encoded as a string");
    };
    URL_SAFE_NO_PAD.decode(encoded).unwrap()
}

#[tokio::test]
async fn set_attribute_round_trips_a_value() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.mkdir(&ctx, &path("/f"), Default::default())
        .await
        .unwrap();
    let node = fs
        .set_attribute(
            &ctx,
            &path("/f"),
            "color",
            b"blue",
            MutationOptions::default(),
        )
        .await
        .unwrap();

    assert_eq!(decoded(&node, "color"), b"blue");
}

#[tokio::test]
async fn setting_a_second_attribute_preserves_the_first() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.mkdir(&ctx, &path("/f"), Default::default())
        .await
        .unwrap();
    fs.set_attribute(
        &ctx,
        &path("/f"),
        "color",
        b"blue",
        MutationOptions::default(),
    )
    .await
    .unwrap();
    let node = fs
        .set_attribute(
            &ctx,
            &path("/f"),
            "size",
            b"large",
            MutationOptions::default(),
        )
        .await
        .unwrap();

    assert_eq!(decoded(&node, "color"), b"blue");
    assert_eq!(decoded(&node, "size"), b"large");
}

#[tokio::test]
async fn set_attribute_upsert_replaces_the_value() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.mkdir(&ctx, &path("/f"), Default::default())
        .await
        .unwrap();
    fs.set_attribute(
        &ctx,
        &path("/f"),
        "color",
        b"blue",
        MutationOptions::default(),
    )
    .await
    .unwrap();
    let node = fs
        .set_attribute(
            &ctx,
            &path("/f"),
            "color",
            b"red",
            MutationOptions::default(),
        )
        .await
        .unwrap();

    assert_eq!(decoded(&node, "color"), b"red");
    assert_eq!(node.attributes.len(), 1);
}

#[tokio::test]
async fn attribute_mutations_bump_the_node_revision() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    let created = fs
        .mkdir(&ctx, &path("/f"), Default::default())
        .await
        .unwrap();
    let after_set = fs
        .set_attribute(
            &ctx,
            &path("/f"),
            "color",
            b"blue",
            MutationOptions::default(),
        )
        .await
        .unwrap();
    assert!(after_set.revision.get() > created.revision.get());
}

#[tokio::test]
async fn remove_attribute_removes_the_key() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.mkdir(&ctx, &path("/f"), Default::default())
        .await
        .unwrap();
    fs.set_attribute(
        &ctx,
        &path("/f"),
        "color",
        b"blue",
        MutationOptions::default(),
    )
    .await
    .unwrap();
    let node = fs
        .remove_attribute(&ctx, &path("/f"), "color", MutationOptions::default())
        .await
        .unwrap();

    assert!(!node.attributes.contains_key("color"));
}

#[tokio::test]
async fn remove_attribute_on_a_missing_key_is_a_no_op() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    let created = fs
        .mkdir(&ctx, &path("/f"), Default::default())
        .await
        .unwrap();
    let node = fs
        .remove_attribute(&ctx, &path("/f"), "missing", MutationOptions::default())
        .await
        .unwrap();

    assert_eq!(node.revision, created.revision);
}

#[tokio::test]
async fn set_attribute_rejects_an_empty_key() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.mkdir(&ctx, &path("/f"), Default::default())
        .await
        .unwrap();
    let error = fs
        .set_attribute(&ctx, &path("/f"), "", b"x", MutationOptions::default())
        .await
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::InvalidPathOrName);
}

#[tokio::test]
async fn set_attribute_rejects_an_oversized_value() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.mkdir(&ctx, &path("/f"), Default::default())
        .await
        .unwrap();
    let oversized = vec![0u8; 4097];
    let error = fs
        .set_attribute(
            &ctx,
            &path("/f"),
            "big",
            &oversized,
            MutationOptions::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::QuotaExceeded);
}

#[tokio::test]
async fn set_attribute_rejects_exceeding_the_per_node_count() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.mkdir(&ctx, &path("/f"), Default::default())
        .await
        .unwrap();
    for i in 0..64 {
        fs.set_attribute(
            &ctx,
            &path("/f"),
            &format!("key-{i}"),
            b"v",
            MutationOptions::default(),
        )
        .await
        .unwrap();
    }

    let error = fs
        .set_attribute(
            &ctx,
            &path("/f"),
            "one-too-many",
            b"v",
            MutationOptions::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::QuotaExceeded);
}

#[tokio::test]
async fn set_attribute_expected_revision_conflict_is_rejected() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.mkdir(&ctx, &path("/f"), Default::default())
        .await
        .unwrap();
    let bogus = Revision::new(999).unwrap();
    let error = fs
        .set_attribute(
            &ctx,
            &path("/f"),
            "color",
            b"blue",
            MutationOptions::default().expected_revision(Some(bogus)),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::RevisionConflict);
}

#[tokio::test]
async fn set_attribute_on_a_missing_path_is_not_found() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    let error = fs
        .set_attribute(
            &ctx,
            &path("/missing"),
            "k",
            b"v",
            MutationOptions::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::NotFound);
}

