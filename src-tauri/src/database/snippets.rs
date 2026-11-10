//! Command snippet persistence + validation.

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use super::now;
use crate::error::{AppError, AppResult};

pub const CATEGORIES: &[&str] = &["linux", "networking", "docker", "git", "diagnostics", "powershell", "security", "custom"];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnippetInput {
    pub title: String,
    pub description: String,
    pub command: String,
    pub category: String,
    pub tags: Vec<String>,
    pub favorite: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snippet {
    pub id: String,
    #[serde(flatten)]
    pub input: SnippetInput,
    pub use_count: i64,
    pub last_used_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

pub fn validate(input: &SnippetInput) -> AppResult<SnippetInput> {
    let bad = |m: &str| Err(AppError::Validation(m.into()));
    let mut out = input.clone();
    out.title = input.title.trim().to_string();
    out.description = input.description.trim().to_string();
    // Keep the command exactly as written except trailing newlines, which
    // would make "insert" execute it.
    out.command = input.command.trim_end_matches(['\r', '\n']).to_string();
    out.category = input.category.trim().to_lowercase();

    if out.title.is_empty() || out.title.chars().count() > 100 {
        return bad("Title is required (max 100 characters)");
    }
    if out.description.chars().count() > 500 {
        return bad("Description is too long (max 500 characters)");
    }
    if out.command.trim().is_empty() {
        return bad("Command is required");
    }
    if out.command.len() > 10_000 {
        return bad("Command is too long (max 10 000 characters)");
    }
    if out.command.contains('\0') {
        return bad("Command contains a NUL character");
    }
    if !CATEGORIES.contains(&out.category.as_str()) {
        return bad("Unknown category");
    }
    let mut tags: Vec<String> = Vec::new();
    for t in &input.tags {
        let t = t.trim().to_lowercase();
        if t.is_empty() || tags.contains(&t) {
            continue;
        }
        if t.chars().count() > 32 {
            return bad("Tags are limited to 32 characters");
        }
        tags.push(t);
    }
    if tags.len() > 12 {
        return bad("At most 12 tags");
    }
    out.tags = tags;
    Ok(out)
}

const COLS: &str = "id, title, description, command, category, tags, favorite, use_count, last_used_at, created_at, updated_at";

fn from_row(r: &Row) -> rusqlite::Result<Snippet> {
    let tags: String = r.get(5)?;
    Ok(Snippet {
        id: r.get(0)?,
        input: SnippetInput {
            title: r.get(1)?,
            description: r.get(2)?,
            command: r.get(3)?,
            category: r.get(4)?,
            tags: serde_json::from_str(&tags).unwrap_or_default(),
            favorite: r.get(6)?,
        },
        use_count: r.get(7)?,
        last_used_at: r.get(8)?,
        created_at: r.get(9)?,
        updated_at: r.get(10)?,
    })
}

pub fn list(conn: &Connection) -> AppResult<Vec<Snippet>> {
    let mut s = conn.prepare(&format!("SELECT {COLS} FROM snippets ORDER BY favorite DESC, title COLLATE NOCASE"))?;
    let rows = s.query_map([], from_row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn get(conn: &Connection, id: &str) -> AppResult<Snippet> {
    conn.query_row(&format!("SELECT {COLS} FROM snippets WHERE id = ?1"), [id], from_row)
        .optional()?
        .ok_or_else(|| AppError::NotFound("snippet".into()))
}

pub fn insert(conn: &Connection, input: &SnippetInput) -> AppResult<Snippet> {
    let i = validate(input)?;
    let id = uuid::Uuid::new_v4().to_string();
    let ts = now();
    conn.execute(
        "INSERT INTO snippets (id, title, description, command, category, tags, favorite, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
        params![id, i.title, i.description, i.command, i.category, serde_json::to_string(&i.tags)?, i.favorite, ts],
    )?;
    get(conn, &id)
}

pub fn update(conn: &Connection, id: &str, input: &SnippetInput) -> AppResult<Snippet> {
    let i = validate(input)?;
    let n = conn.execute(
        "UPDATE snippets SET title=?2, description=?3, command=?4, category=?5, tags=?6, favorite=?7, updated_at=?8 WHERE id=?1",
        params![id, i.title, i.description, i.command, i.category, serde_json::to_string(&i.tags)?, i.favorite, now()],
    )?;
    if n == 0 {
        return Err(AppError::NotFound("snippet".into()));
    }
    get(conn, id)
}

pub fn set_favorite(conn: &Connection, id: &str, favorite: bool) -> AppResult<()> {
    conn.execute("UPDATE snippets SET favorite=?2 WHERE id=?1", params![id, favorite])?;
    Ok(())
}

/// Usage stats only — the command text and target are not recorded anywhere.
pub fn mark_used(conn: &Connection, id: &str) -> AppResult<()> {
    conn.execute("UPDATE snippets SET use_count = use_count + 1, last_used_at = ?2 WHERE id = ?1", params![id, now()])?;
    Ok(())
}

pub fn delete(conn: &Connection, id: &str) -> AppResult<()> {
    if conn.execute("DELETE FROM snippets WHERE id=?1", [id])? == 0 {
        return Err(AppError::NotFound("snippet".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Database;

    fn input() -> SnippetInput {
        SnippetInput { title: " Disk ".into(), description: "".into(), command: "df -h\n\n".into(), category: "Linux".into(), tags: vec!["Disk".into(), "disk".into()], favorite: false }
    }

    #[test]
    fn seeded_and_crud() {
        let db = Database::open_in_memory().unwrap();
        db.with(|c| {
            let seeded = list(c)?;
            assert!(seeded.len() >= 10, "starter snippets present");
            assert!(seeded.iter().all(|s| CATEGORIES.contains(&s.input.category.as_str())));

            let s = insert(c, &input())?;
            assert_eq!(s.input.title, "Disk");
            assert_eq!(s.input.command, "df -h", "trailing newlines stripped so insert never executes");
            assert_eq!(s.input.category, "linux");
            assert_eq!(s.input.tags, vec!["disk"]);

            mark_used(c, &s.id)?;
            mark_used(c, &s.id)?;
            assert_eq!(get(c, &s.id)?.use_count, 2);

            let mut e = input();
            e.command = "free -h".into();
            e.favorite = true;
            assert_eq!(update(c, &s.id, &e)?.input.command, "free -h");
            assert_eq!(list(c)?[0].id, s.id, "favorites first");

            delete(c, &s.id)?;
            assert!(get(c, &s.id).is_err());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn validation() {
        let mut i = input();
        i.command = "   \n".into();
        assert!(validate(&i).is_err());
        let mut i = input();
        i.category = "hacking".into();
        assert!(validate(&i).is_err());
        let mut i = input();
        i.command = "echo a\0b".into();
        assert!(validate(&i).is_err());
        let mut i = input();
        i.command = "x".repeat(10_001);
        assert!(validate(&i).is_err());
        // Multi-line commands are allowed; only trailing newlines are removed.
        let mut i = input();
        i.command = "cd /tmp\nls\n".into();
        assert_eq!(validate(&i).unwrap().command, "cd /tmp\nls");
    }
}
