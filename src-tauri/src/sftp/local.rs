//! Local side of the file manager (Rust only; the webview has no fs access).

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use super::paths::{local_join, validate_local_name};
use super::{sort_and_cap, EntryKind, FileEntry, Listing};
use crate::error::{AppError, AppResult};

pub fn home() -> String {
    std::env::var("USERPROFILE").unwrap_or_else(|_| "C:\\".into())
}

fn drives() -> Listing {
    let entries: Vec<FileEntry> = (b'A'..=b'Z')
        .map(|c| format!("{}:\\", c as char))
        .filter(|d| Path::new(d).exists())
        .map(|d| FileEntry { name: d.clone(), path: d, kind: EntryKind::Dir, is_dir: true, size: None, modified: None, permissions: None, hidden: false })
        .collect();
    let total = entries.len();
    Listing { path: String::new(), parent: None, entries, truncated: false, total }
}

#[cfg(windows)]
fn is_hidden(meta: &std::fs::Metadata, name: &str) -> bool {
    use std::os::windows::fs::MetadataExt;
    const HIDDEN: u32 = 0x2;
    const SYSTEM: u32 = 0x4;
    meta.file_attributes() & (HIDDEN | SYSTEM) != 0 || name.starts_with('.')
}

#[cfg(not(windows))]
fn is_hidden(_meta: &std::fs::Metadata, name: &str) -> bool {
    name.starts_with('.')
}

/// List a local directory; "" lists drives.
pub fn list(path: &str) -> AppResult<Listing> {
    if path.is_empty() {
        return Ok(drives());
    }
    let dir = PathBuf::from(path);
    let rd = std::fs::read_dir(&dir).map_err(|e| AppError::Validation(format!("Cannot open {path}: {e}")))?;
    let mut entries = Vec::new();
    for item in rd.flatten() {
        let name = item.file_name().to_string_lossy().into_owned();
        let Ok(ft) = item.file_type() else { continue };
        let meta = std::fs::metadata(item.path()).or_else(|_| item.metadata());
        let Ok(meta) = meta else { continue };
        let kind = if ft.is_symlink() { EntryKind::Symlink } else if meta.is_dir() { EntryKind::Dir } else if meta.is_file() { EntryKind::File } else { EntryKind::Other };
        entries.push(FileEntry {
            hidden: is_hidden(&meta, &name),
            is_dir: meta.is_dir(),
            size: meta.is_file().then(|| meta.len()),
            modified: meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64),
            permissions: meta.permissions().readonly().then(|| "read-only".to_string()),
            path: item.path().to_string_lossy().into_owned(),
            name,
            kind,
        });
    }
    let (entries, truncated, total) = sort_and_cap(entries);
    let parent = Some(dir.parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default());
    Ok(Listing { path: dir.to_string_lossy().into_owned(), parent, entries, truncated, total })
}

pub fn mkdir(dir: &str, name: &str) -> AppResult<String> {
    let p = local_join(Path::new(dir), name).map_err(AppError::Validation)?;
    std::fs::create_dir(&p).map_err(|e| AppError::Validation(format!("Cannot create folder: {e}")))?;
    Ok(p.to_string_lossy().into_owned())
}

pub fn rename(dir: &str, from: &str, to: &str) -> AppResult<()> {
    validate_local_name(to).map_err(AppError::Validation)?;
    let a = Path::new(dir).join(from);
    if a.parent() != Some(Path::new(dir)) {
        return Err(AppError::Validation("Invalid name".into()));
    }
    let b = local_join(Path::new(dir), to).map_err(AppError::Validation)?;
    if b.exists() {
        return Err(AppError::Exists(format!("'{to}' already exists")));
    }
    std::fs::rename(a, b).map_err(|e| AppError::Validation(format!("Cannot rename: {e}")))
}

/// Moves to the Recycle Bin (recoverable), never a permanent delete.
pub fn delete(dir: &str, name: &str) -> AppResult<()> {
    let p = Path::new(dir).join(name);
    if p.parent() != Some(Path::new(dir)) || name.is_empty() || name == "." || name == ".." {
        return Err(AppError::Validation("Invalid name".into()));
    }
    trash::delete(&p).map_err(|e| AppError::Validation(format!("Cannot move to Recycle Bin: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_mkdir_rename() {
        let d = tempfile::tempdir().unwrap();
        let dir = d.path().to_string_lossy().into_owned();
        std::fs::write(d.path().join("b.txt"), b"hello").unwrap();
        mkdir(&dir, "zfolder").unwrap();
        assert!(mkdir(&dir, "..").is_err());
        assert!(mkdir(&dir, "a:b").is_err());

        let l = list(&dir).unwrap();
        let names: Vec<_> = l.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["zfolder", "b.txt"], "folders first");
        assert_eq!(l.entries[1].size, Some(5));
        assert!(l.parent.is_some());

        rename(&dir, "b.txt", "c.txt").unwrap();
        std::fs::write(d.path().join("d.txt"), b"x").unwrap();
        assert!(matches!(rename(&dir, "c.txt", "d.txt"), Err(AppError::Exists(_))), "never overwrites on rename");
        assert!(rename(&dir, r"..\c.txt", "e.txt").is_err());
    }

    #[test]
    fn drives_listed_at_root() {
        let l = list("").unwrap();
        assert!(l.entries.iter().any(|e| e.name == "C:\\"));
        assert!(l.parent.is_none());
    }
}
