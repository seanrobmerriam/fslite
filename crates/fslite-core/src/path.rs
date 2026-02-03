use std::fmt;
use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{FsError, FsResult};

/// An absolute, normalized path within a filesystem workspace.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct VirtualPath {
    canonical: Arc<str>,
}

impl VirtualPath {
    /// Parses and normalizes an absolute virtual path.
    pub fn parse(input: &str) -> FsResult<Self> {
        if !input.starts_with('/') || input.contains('\0') {
            return Err(FsError::invalid_path_or_name(input));
        }

        Ok(Self {
            canonical: normalize(input.split('/')),
        })
    }

    /// Returns the workspace root path.
    pub fn root() -> Self {
        Self {
            canonical: Arc::from("/"),
        }
    }

    /// Returns the canonical absolute path.
    pub fn as_str(&self) -> &str {
        &self.canonical
    }

    /// Returns the final path segment, or `None` for the root.
    pub fn name(&self) -> Option<&str> {
        if self.canonical.len() == 1 {
            None
        } else {
            self.canonical.rsplit('/').next()
        }
    }

    /// Returns the normalized parent, or `None` for the root.
    pub fn parent(&self) -> Option<Self> {
        let separator = self.canonical.rfind('/')?;
        if separator == 0 {
            if self.canonical.len() == 1 {
                None
            } else {
                Some(Self::root())
            }
        } else {
            Some(Self {
                canonical: Arc::from(&self.canonical[..separator]),
            })
        }
    }

    /// Resolves a relative path against this path without escaping the root.
    pub fn join(&self, relative: &str) -> FsResult<Self> {
        if relative.starts_with('/') || relative.contains('\0') {
            return Err(FsError::invalid_path_or_name(relative));
        }

        Ok(Self {
            canonical: normalize(self.segments().chain(relative.split('/'))),
        })
    }

    /// Iterates over canonical path segments from root to leaf.
    pub fn segments(&self) -> impl Iterator<Item = &str> {
        self.canonical[1..]
            .split('/')
            .filter(|segment| !segment.is_empty())
    }
}

impl AsRef<str> for VirtualPath {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for VirtualPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl Serialize for VirtualPath {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for VirtualPath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let input = String::deserialize(deserializer)?;
        Self::parse(&input).map_err(serde::de::Error::custom)
    }
}

/// A normalized absolute or relative target stored by a symbolic link.
///
/// Relative targets retain leading `..` segments so a filesystem backend can
/// resolve them against the link's containing directory without allowing the
/// resolved [`VirtualPath`] to escape its workspace root.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LinkTarget {
    canonical: Arc<str>,
}

impl LinkTarget {
    /// Parses and normalizes a symbolic-link target.
    ///
    /// # Errors
    ///
    /// Returns an error when the target is empty or contains a NUL byte.
    pub fn parse(input: &str) -> FsResult<Self> {
        if input.is_empty() || input.contains('\0') {
            return Err(FsError::invalid_path_or_name(input));
        }

        let canonical = if input.starts_with('/') {
            normalize(input.split('/'))
        } else {
            normalize_relative(input.split('/'))
        };

        Ok(Self { canonical })
    }

    /// Returns the normalized target string.
    pub fn as_str(&self) -> &str {
        &self.canonical
    }

    /// Returns whether this target is absolute.
    pub fn is_absolute(&self) -> bool {
        self.canonical.starts_with('/')
    }
}

impl AsRef<str> for LinkTarget {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for LinkTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl Serialize for LinkTarget {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for LinkTarget {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let input = String::deserialize(deserializer)?;
        Self::parse(&input).map_err(serde::de::Error::custom)
    }
}

fn normalize<'a>(segments: impl IntoIterator<Item = &'a str>) -> Arc<str> {
    let mut normalized = Vec::new();

    for segment in segments {
        match segment {
            "" | "." => {}
            ".." => {
                normalized.pop();
            }
            _ => normalized.push(segment),
        }
    }

    if normalized.is_empty() {
        return Arc::from("/");
    }

    let capacity = normalized.iter().map(|segment| segment.len() + 1).sum();
    let mut canonical = String::with_capacity(capacity);
    for segment in normalized {
        canonical.push('/');
        canonical.push_str(segment);
    }
    Arc::from(canonical)
}

fn normalize_relative<'a>(segments: impl IntoIterator<Item = &'a str>) -> Arc<str> {
    let mut normalized = Vec::new();

    for segment in segments {
        match segment {
            "" | "." => {}
            ".." if normalized.last().is_some_and(|last| *last != "..") => {
                normalized.pop();
            }
            ".." => normalized.push(segment),
            _ => normalized.push(segment),
        }
    }

    if normalized.is_empty() {
        return Arc::from(".");
    }

    Arc::from(normalized.join("/"))
}

