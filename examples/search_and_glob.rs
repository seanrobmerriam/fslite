//! Finding things: `glob` matches paths, `find` matches bounded metadata
//! predicates, and `search_content` finds literal byte matches inside files.
//!
//! Run with `cargo run --example search_and_glob`.

use fslite_core::{ContentQuery, FindQuery, NodeKind, RequestContext, VirtualPath, WriteSource};
use fslite_sqlite::SqliteFileSystem;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let fs = SqliteFileSystem::open_in_memory(Default::default()).await?;
    let workspace = fs.create_workspace(Default::default()).await?;
    let ctx = RequestContext::trusted(workspace.id);

    fs.mkdir(&ctx, &VirtualPath::parse("/logs")?, Default::default())
        .await?;
    for (name, contents) in [
        ("2026-01.txt", "warn: disk 80% full"),
        ("2026-02.txt", "info: rotation complete"),
        ("2026-03.txt", "warn: disk 92% full"),
    ] {
        let path = VirtualPath::parse(&format!("/logs/{name}"))?;
        fs.write(
            &ctx,
            &path,
            WriteSource::from_bytes(contents.as_bytes().to_vec()),
            Default::default(),
        )
        .await?;
    }

    // glob: match paths by shape.
    let page = fs.glob(&ctx, "/logs/*.txt", Default::default()).await?;
    println!("glob '/logs/*.txt' matched {} file(s):", page.items.len());
    for node in &page.items {
        println!("  {}", node.name);
    }

    // find: match bounded metadata — every regular file under /logs.
    let query = FindQuery::default()
        .root(VirtualPath::parse("/logs")?)
        .kind(Some(NodeKind::File));
    let page = fs.find(&ctx, query, Default::default()).await?;
    println!("find matched {} file(s) under /logs", page.items.len());

    // search_content: literal byte matches inside file bodies.
    let query = ContentQuery::default()
        .root(VirtualPath::parse("/logs")?)
        .needle(b"warn:".to_vec());
    let page = fs.search_content(&ctx, query, Default::default()).await?;
    println!(
        "search_content found 'warn:' in {} file(s):",
        page.items.len()
    );
    for search_match in &page.items {
        println!("  {}", search_match.path);
    }

    Ok(())
}
