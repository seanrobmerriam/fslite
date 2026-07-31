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

#[cfg(test)]
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
}
