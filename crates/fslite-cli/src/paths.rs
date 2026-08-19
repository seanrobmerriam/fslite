//! Resolves the directory `fslite` stores its local registry and context
//! state in. `FSLITE_CONFIG_DIR` overrides everything else — tests set it
//! to a fresh temp directory so they never touch a real `$HOME`. Absent
//! that override, this follows the same `$XDG_CONFIG_HOME`-or-`$HOME/.config`
//! convention most CLI tools on Linux use.

use std::path::PathBuf;

pub fn config_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Ok(dir) = std::env::var("FSLITE_CONFIG_DIR") {
        return Ok(PathBuf::from(dir));
    }
    if let Ok(dir) = std::env::var("XDG_CONFIG_HOME") {
        return Ok(PathBuf::from(dir).join("fslite"));
    }
    let home = std::env::var("HOME").map_err(|_| {
        "cannot resolve a config directory: none of FSLITE_CONFIG_DIR, XDG_CONFIG_HOME, or HOME is set"
    })?;
    Ok(PathBuf::from(home).join(".config").join("fslite"))
}

/// Resolves the directory containing persistent databases created by the CLI.
/// Unlike registry/context metadata, database files belong in the platform's
/// local application-data directory.
pub fn data_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Ok(dir) = std::env::var("FSLITE_DATA_DIR") {
        return Ok(PathBuf::from(dir));
    }
    let project = directories::ProjectDirs::from("", "", "fslite")
        .ok_or("cannot resolve a user data directory; set FSLITE_DATA_DIR")?;
    Ok(project.data_local_dir().to_path_buf())
}

#[cfg(test)]
#[allow(unsafe_code)] // SAFETY: env-var mutation is serialized by `test_support::lock`.
mod tests {
    use super::*;

    #[test]
    fn fslite_config_dir_env_var_wins_over_everything() {
        // Held for the whole test body — see `crate::test_support` for why
        // env-var-mutating tests must serialize against each other.
        let _guard = crate::test_support::lock();
        // SAFETY: serialized by the lock above.
        unsafe {
            std::env::set_var("FSLITE_CONFIG_DIR", "/tmp/fslite-test-config");
            std::env::set_var("XDG_CONFIG_HOME", "/should/be/ignored");
        }
        let dir = config_dir().unwrap();
        unsafe {
            std::env::remove_var("FSLITE_CONFIG_DIR");
            std::env::remove_var("XDG_CONFIG_HOME");
        }
        assert_eq!(dir, PathBuf::from("/tmp/fslite-test-config"));
    }

    #[test]
    fn xdg_config_home_is_used_when_set() {
        let _guard = crate::test_support::lock();
        // SAFETY: serialized by the lock above.
        unsafe {
            std::env::remove_var("FSLITE_CONFIG_DIR");
            std::env::set_var("XDG_CONFIG_HOME", "/tmp/fslite-xdg-test");
        }
        let dir = config_dir().unwrap();
        unsafe {
            std::env::remove_var("XDG_CONFIG_HOME");
        }
        assert_eq!(dir, PathBuf::from("/tmp/fslite-xdg-test/fslite"));
    }

    #[test]
    fn fslite_data_dir_env_var_wins() {
        let _guard = crate::test_support::lock();
        unsafe {
            std::env::set_var("FSLITE_DATA_DIR", "/tmp/fslite-test-data");
        }
        let dir = data_dir().unwrap();
        unsafe {
            std::env::remove_var("FSLITE_DATA_DIR");
        }
        assert_eq!(dir, PathBuf::from("/tmp/fslite-test-data"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn xdg_data_home_is_used_when_set() {
        let _guard = crate::test_support::lock();
        unsafe {
            std::env::remove_var("FSLITE_DATA_DIR");
            std::env::set_var("XDG_DATA_HOME", "/tmp/fslite-xdg-data-test");
        }
        let dir = data_dir().unwrap();
        unsafe {
            std::env::remove_var("XDG_DATA_HOME");
        }
        assert_eq!(dir, PathBuf::from("/tmp/fslite-xdg-data-test/fslite"));
    }
}
