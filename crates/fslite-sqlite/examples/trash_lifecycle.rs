//! Recoverable trash vs. permanent removal: `trash` hides a subtree without
//! touching its data, `restore` brings it back, and `purge` is the only way
//! a trashed node's content is actually reclaimed.
//!
//! Run with `cargo run --example trash_lifecycle`.

use fslite_core::{MutationOptions, RequestContext, VirtualPath, WriteSource};
use fslite_sqlite::SqliteFileSystem;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let fs = SqliteFileSystem::open_in_memory(Default::default()).await?;
    let workspace = fs.create_workspace(Default::default()).await?;
    let ctx = RequestContext::trusted(workspace.id);

    let keep_path = VirtualPath::parse("/keep-me.txt")?;
    fs.write(
        &ctx,
        &keep_path,
        WriteSource::from_bytes(b"important".to_vec()),
        Default::default(),
    )
    .await?;

    let discard_path = VirtualPath::parse("/discard-me.txt")?;
    fs.write(
        &ctx,
        &discard_path,
        WriteSource::from_bytes(b"scratch".to_vec()),
        Default::default(),
    )
    .await?;

    // Trash /discard-me.txt: it disappears from listings immediately, but
    // its content is untouched.
    let trashed = fs
        .trash(&ctx, &discard_path, MutationOptions::default())
        .await?;
    println!(
        "trashed {} as trash id {}",
        trashed.original_path, trashed.id
    );

    let listing = fs
        .read_dir(&ctx, &VirtualPath::root(), Default::default())
        .await?;
    println!("root now contains {} entry(ies):", listing.items.len());
    for node in &listing.items {
        println!("  {}", node.name);
    }

    // list_trash enumerates trashed subtrees.
    let trash_page = fs.list_trash(&ctx, Default::default()).await?;
    println!("trash contains {} entry(ies)", trash_page.items.len());

    // Restore it to a different name, since nothing may already occupy the
    // destination.
    let restored_path = VirtualPath::parse("/discard-me-restored.txt")?;
    let restored = fs
        .restore(
            &ctx,
            trashed.id,
            Some(&restored_path),
            MutationOptions::default(),
        )
        .await?;
    println!("restored as {}", restored.name);

    // Trash it again and this time purge it: content is now unrecoverable.
    let trashed_again = fs
        .trash(&ctx, &restored_path, MutationOptions::default())
        .await?;
    fs.purge(&ctx, trashed_again.id).await?;
    println!(
        "purged trash id {} — no restore is possible now",
        trashed_again.id
    );

    let trash_page = fs.list_trash(&ctx, Default::default()).await?;
    println!(
        "trash contains {} entry(ies) after purge",
        trash_page.items.len()
    );

    Ok(())
}
