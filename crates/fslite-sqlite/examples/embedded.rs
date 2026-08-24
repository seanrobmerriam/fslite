//! Direct embedded usage of `fslite-sqlite`: open a database, create a
//! workspace, write a file from a byte stream, read it back, list the root,
//! and print workspace usage.
//!
//! Run with `cargo run --example embedded`.

use fslite_core::{RequestContext, VirtualPath, WriteSource};
use fslite_sqlite::SqliteFileSystem;
use futures::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let fs = SqliteFileSystem::open_in_memory(Default::default()).await?;
    let workspace = fs.create_workspace(Default::default()).await?;
    let ctx = RequestContext::trusted(workspace.id);

    let path = VirtualPath::parse("/hello.txt")?;
    fs.write(
        &ctx,
        &path,
        WriteSource::from_bytes(b"hello".to_vec()),
        Default::default(),
    )
    .await?;

    let read = fs.read(&ctx, &path, Default::default()).await?;
    let mut stream = read.into_stream();
    let mut contents = Vec::new();
    while let Some(chunk) = stream.next().await {
        contents.extend_from_slice(&chunk?);
    }
    println!("{}", String::from_utf8(contents)?);

    let listing = fs
        .read_dir(&ctx, &VirtualPath::root(), Default::default())
        .await?;
    println!("root contains {} entry(ies):", listing.items.len());
    for node in &listing.items {
        println!("  {}", node.name);
    }

    let usage = fs.workspace_usage(&ctx).await?;
    println!(
        "usage: {} active bytes across {} nodes",
        usage.active_logical_bytes, usage.active_nodes
    );

    Ok(())
}
