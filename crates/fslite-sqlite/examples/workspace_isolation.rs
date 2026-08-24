//! Workspace isolation: one SQLite database can hold many independent
//! workspaces. The same absolute path can exist in two workspaces at once
//! with unrelated content, and neither can see the other's nodes.
//!
//! Run with `cargo run --example workspace_isolation`.

use fslite_core::{RequestContext, VirtualPath, WriteSource};
use fslite_sqlite::SqliteFileSystem;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let fs = SqliteFileSystem::open_in_memory(Default::default()).await?;

    let workspace_a = fs.create_workspace(Default::default()).await?;
    let workspace_b = fs.create_workspace(Default::default()).await?;
    println!("workspace A: {}", workspace_a.id);
    println!("workspace B: {}", workspace_b.id);

    let ctx_a = RequestContext::trusted(workspace_a.id);
    let ctx_b = RequestContext::trusted(workspace_b.id);

    // The identical path exists independently in both workspaces.
    let path = VirtualPath::parse("/config.json")?;
    fs.write(
        &ctx_a,
        &path,
        WriteSource::from_bytes(b"{\"owner\":\"a\"}".to_vec()),
        Default::default(),
    )
    .await?;
    fs.write(
        &ctx_b,
        &path,
        WriteSource::from_bytes(b"{\"owner\":\"b\"}".to_vec()),
        Default::default(),
    )
    .await?;

    let node_a = fs.stat(&ctx_a, &path, Default::default()).await?;
    let node_b = fs.stat(&ctx_b, &path, Default::default()).await?;
    println!(
        "/config.json in A: node id {} ({} bytes)",
        node_a.id, node_a.logical_size
    );
    println!(
        "/config.json in B: node id {} ({} bytes)",
        node_b.id, node_b.logical_size
    );
    assert_ne!(node_a.id, node_b.id, "each workspace's node is independent");

    // A cursor from one workspace is rejected in another, not silently
    // reinterpreted.
    let page_a = fs
        .read_dir(&ctx_a, &VirtualPath::root(), Default::default())
        .await?;
    if let Some(cursor) = page_a.next_cursor {
        let page = fslite_core::PageRequest::default().cursor(Some(cursor));
        match fs.read_dir(&ctx_b, &VirtualPath::root(), page).await {
            Ok(_) => println!("unexpected: workspace A's cursor was accepted by workspace B"),
            Err(err) => println!("workspace A's cursor rejected in workspace B: {err}"),
        }
    } else {
        println!("workspace A's single-entry root has no continuation cursor to test");
    }

    Ok(())
}
