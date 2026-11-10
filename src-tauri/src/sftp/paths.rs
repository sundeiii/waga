//! Path handling for SFTP. Remote paths are POSIX; local paths are Windows.
//! File names coming from either side are untrusted and validated before use.

use std::path::{Path, PathBuf};

pub const PARTIAL_SUFFIX: &str = ".yoru-partial";

/// A single path component that is safe to join onto a directory.
pub fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Name is empty".into());
    }
    if name == "." || name == ".." {
        return Err("Name cannot be '.' or '..'".into());
    }
    if name.contains('/') || name.contains('\0') {
        return Err("Name cannot contain '/'".into());
    }
    if name.len() > 255 {
        return Err("Name is too long".into());
    }
    Ok(())
}

/// Join a remote (POSIX) directory and a validated name.
pub fn remote_join(dir: &str, name: &str) -> Result<String, String> {
    validate_name(name)?;
    let dir = if dir.is_empty() { "." } else { dir };
    Ok(if dir.ends_with('/') { format!("{dir}{name}") } else { format!("{dir}/{name}") })
}

/// Parent of a remote absolute path ("/" stays "/").
pub fn remote_parent(path: &str) -> String {
    let p = path.trim_end_matches('/');
    match p.rfind('/') {
        Some(0) | None => "/".into(),
        Some(i) => p[..i].to_string(),
    }
}

/// Last component of a remote path.
pub fn remote_name(path: &str) -> &str {
    path.trim_end_matches('/').rsplit('/').next().unwrap_or("")
}

const WIN_RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3",
    "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Validate a name for creating/renaming on Windows.
pub fn validate_local_name(name: &str) -> Result<(), String> {
    validate_name(name)?;
    if name.chars().any(|c| matches!(c, '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control()) {
        return Err("Name cannot contain \\ / : * ? \" < > | or control characters".into());
    }
    if name.ends_with('.') || name.ends_with(' ') {
        return Err("Name cannot end with a dot or space".into());
    }
    let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    if WIN_RESERVED.contains(&stem.as_str()) {
        return Err(format!("'{name}' is a reserved name on Windows"));
    }
    Ok(())
}

/// Make a remote file name usable on Windows (for downloads): replace
/// characters Windows forbids, and refuse traversal.
pub fn sanitize_for_windows(remote_name: &str) -> Result<String, String> {
    validate_name(remote_name)?;
    let mut s: String = remote_name
        .chars()
        .map(|c| if matches!(c, '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() { '_' } else { c })
        .collect();
    while s.ends_with('.') || s.ends_with(' ') {
        s.pop();
    }
    if s.is_empty() {
        return Err("File name is not usable on Windows".into());
    }
    let stem = s.split('.').next().unwrap_or("").to_ascii_uppercase();
    if WIN_RESERVED.contains(&stem.as_str()) {
        s.insert(0, '_');
    }
    Ok(s)
}

/// Join a local directory and a validated name, ensuring the result stays inside `dir`.
pub fn local_join(dir: &Path, name: &str) -> Result<PathBuf, String> {
    validate_local_name(name)?;
    let p = dir.join(name);
    if p.parent() != Some(dir) {
        return Err("Invalid file name".into());
    }
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_paths() {
        assert_eq!(remote_join("/home/u", "a.txt").unwrap(), "/home/u/a.txt");
        assert_eq!(remote_join("/", "etc").unwrap(), "/etc");
        assert_eq!(remote_join("/srv/", "x").unwrap(), "/srv/x");
        for bad in ["", ".", "..", "a/b", "../x", "x\0y"] {
            assert!(remote_join("/home", bad).is_err(), "{bad:?}");
        }
        assert_eq!(remote_parent("/home/u/docs"), "/home/u");
        assert_eq!(remote_parent("/home"), "/");
        assert_eq!(remote_parent("/"), "/");
        assert_eq!(remote_parent("/a/b/"), "/a");
        assert_eq!(remote_name("/a/b/c.txt"), "c.txt");
        // Windows OpenSSH style paths
        assert_eq!(remote_parent("/C:/Users/me"), "/C:/Users");
    }

    #[test]
    fn local_names() {
        assert!(validate_local_name("report.pdf").is_ok());
        for bad in ["a:b", "a\\b", "a/b", "con", "CON.txt", "lpt1", "x.", "x ", "a?b", "..", ""] {
            assert!(validate_local_name(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn sanitizes_remote_names_for_windows() {
        assert_eq!(sanitize_for_windows("backup:2024-01-01.tar").unwrap(), "backup_2024-01-01.tar");
        assert_eq!(sanitize_for_windows("aux.log").unwrap(), "_aux.log");
        assert_eq!(sanitize_for_windows("trailing...").unwrap(), "trailing");
        assert_eq!(sanitize_for_windows("tab\there").unwrap(), "tab_here");
        assert!(sanitize_for_windows("..").is_err());
        assert!(sanitize_for_windows("../../evil").is_err());
        assert!(sanitize_for_windows("...").is_err());
    }

    #[test]
    fn local_join_stays_inside() {
        let d = Path::new(r"C:\Users\me\Downloads");
        assert_eq!(local_join(d, "a.txt").unwrap(), PathBuf::from(r"C:\Users\me\Downloads\a.txt"));
        assert!(local_join(d, r"..\x").is_err());
        assert!(local_join(d, r"C:\Windows").is_err());
    }
}
