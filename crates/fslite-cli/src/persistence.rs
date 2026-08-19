//! Crash-safe persistence for the CLI's small JSON state files.

use std::io::Write;
use std::path::Path;

use atomic_write_file::AtomicWriteFile;
use serde::Serialize;

pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(value)?;
    let mut file = AtomicWriteFile::open(path)?;
    file.write_all(json.as_bytes())?;
    file.sync_all()?;
    file.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;

    #[derive(Serialize)]
    struct Value {
        answer: u8,
    }

    #[test]
    fn replaces_existing_json_without_exposing_an_intermediate_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        std::fs::write(&path, "old").unwrap();
        write_json(&path, &Value { answer: 42 }).unwrap();
        assert_eq!(
            std::fs::read_to_string(path).unwrap(),
            "{\n  \"answer\": 42\n}"
        );
    }
}
