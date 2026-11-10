//! SFTP over the existing authenticated SSH connection, plus local file
//! browsing for the dual-pane file manager.

pub mod local;
pub mod paths;
pub mod remote;
pub mod transfer;

use serde::Serialize;

/// Directory listings are capped; the UI shows a "truncated" notice.
pub const MAX_ENTRIES: usize = 5000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EntryKind {
    Dir,
    File,
    Symlink,
    Other,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub kind: EntryKind,
    /// True for directories and symlinks pointing at directories.
    pub is_dir: bool,
    pub size: Option<u64>,
    /// Unix seconds.
    pub modified: Option<i64>,
    pub permissions: Option<String>,
    pub hidden: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Listing {
    pub path: String,
    /// None at the top (remote "/" or the local drive list).
    pub parent: Option<String>,
    pub entries: Vec<FileEntry>,
    pub truncated: bool,
    pub total: usize,
}

pub fn sort_and_cap(mut entries: Vec<FileEntry>) -> (Vec<FileEntry>, bool, usize) {
    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    let total = entries.len();
    let truncated = total > MAX_ENTRIES;
    entries.truncate(MAX_ENTRIES);
    (entries, truncated, total)
}

/// "rwxr-xr-x" from a POSIX mode.
pub fn mode_string(mode: u32) -> String {
    let bits = [(0o400, 'r'), (0o200, 'w'), (0o100, 'x'), (0o040, 'r'), (0o020, 'w'), (0o010, 'x'), (0o004, 'r'), (0o002, 'w'), (0o001, 'x')];
    bits.iter().map(|(b, c)| if mode & b != 0 { *c } else { '-' }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(name: &str, is_dir: bool) -> FileEntry {
        FileEntry { name: name.into(), path: name.into(), kind: if is_dir { EntryKind::Dir } else { EntryKind::File }, is_dir, size: None, modified: None, permissions: None, hidden: false }
    }

    #[test]
    fn dirs_first_case_insensitive() {
        let (v, t, n) = sort_and_cap(vec![e("b.txt", false), e("Zeta", true), e("a.txt", false), e("alpha", true)]);
        let names: Vec<_> = v.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, vec!["alpha", "Zeta", "a.txt", "b.txt"]);
        assert!(!t);
        assert_eq!(n, 4);
    }

    #[test]
    fn caps_huge_dirs() {
        let many: Vec<_> = (0..MAX_ENTRIES + 10).map(|i| e(&format!("f{i}"), false)).collect();
        let (v, t, n) = sort_and_cap(many);
        assert_eq!(v.len(), MAX_ENTRIES);
        assert!(t);
        assert_eq!(n, MAX_ENTRIES + 10);
    }

    #[test]
    fn modes() {
        assert_eq!(mode_string(0o755), "rwxr-xr-x");
        assert_eq!(mode_string(0o100644), "rw-r--r--");
    }
}
