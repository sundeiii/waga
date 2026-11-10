//! Input validation for server profiles. All checks run in Rust; the
//! frontend mirrors them only for fast feedback.

use std::net::{Ipv4Addr, Ipv6Addr};
use std::path::Path;

use crate::database::servers::{AuthMethod, ServerInput};
use crate::error::{AppError, AppResult};

fn has_control(s: &str) -> bool {
    s.chars().any(|c| c.is_control())
}

pub fn validate_host(host: &str) -> Result<(), String> {
    let h = host.trim();
    if h.is_empty() {
        return Err("Hostname is required".into());
    }
    if h.len() > 253 {
        return Err("Hostname is too long".into());
    }
    if h.starts_with('-') {
        return Err("Hostname cannot start with '-'".into());
    }
    if has_control(h) || h.chars().any(char::is_whitespace) {
        return Err("Hostname cannot contain spaces or control characters".into());
    }
    let unbracketed = h.strip_prefix('[').and_then(|x| x.strip_suffix(']')).unwrap_or(h);
    if unbracketed.parse::<Ipv6Addr>().is_ok() {
        return Ok(());
    }
    if h.parse::<Ipv4Addr>().is_ok() {
        return Ok(());
    }
    // Looks like a dotted-quad but isn't a valid IPv4 address.
    if h.split('.').all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit())) {
        return Err("Invalid IPv4 address".into());
    }
    let name = h.strip_suffix('.').unwrap_or(h);
    for label in name.split('.') {
        if label.is_empty() || label.len() > 63 {
            return Err("Invalid hostname".into());
        }
        if label.starts_with('-') || label.ends_with('-') {
            return Err("Hostname labels cannot start or end with '-'".into());
        }
        if !label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            return Err("Hostname may contain only letters, digits, '-', '_' and '.'".into());
        }
    }
    Ok(())
}

pub fn validate_username(user: &str) -> Result<(), String> {
    let u = user.trim();
    if u.is_empty() {
        return Err("Username is required".into());
    }
    if u.len() > 256 {
        return Err("Username is too long".into());
    }
    if u.starts_with('-') {
        return Err("Username cannot start with '-'".into());
    }
    if has_control(u) {
        return Err("Username cannot contain control characters".into());
    }
    Ok(())
}

pub fn validate_server_input(input: &ServerInput) -> AppResult<ServerInput> {
    let err = |m: String| AppError::Validation(m);
    let mut out = input.clone();
    out.name = input.name.trim().to_string();
    out.host = input.host.trim().to_string();
    out.username = input.username.trim().to_string();
    out.notes = input.notes.trim_end().to_string();
    out.group_name = input
        .group_name
        .as_ref()
        .map(|g| g.trim().to_string())
        .filter(|g| !g.is_empty());

    if out.name.is_empty() {
        return Err(err("Display name is required".into()));
    }
    if out.name.chars().count() > 100 || has_control(&out.name) {
        return Err(err("Display name must be at most 100 printable characters".into()));
    }
    validate_host(&out.host).map_err(err)?;
    if out.port == 0 {
        return Err(err("Port must be between 1 and 65535".into()));
    }
    validate_username(&out.username).map_err(err)?;
    if let Some(g) = &out.group_name {
        if g.chars().count() > 60 || has_control(g) {
            return Err(err("Group name must be at most 60 characters".into()));
        }
    }
    if out.notes.len() > 10_000 {
        return Err(err("Notes are too long".into()));
    }

    let mut tags: Vec<String> = Vec::new();
    for t in &input.tags {
        let t = t.trim().to_lowercase();
        if t.is_empty() {
            continue;
        }
        if t.chars().count() > 32 || has_control(&t) {
            return Err(err(format!("Tag '{t}' is too long")));
        }
        if !tags.contains(&t) {
            tags.push(t);
        }
    }
    if tags.len() > 20 {
        return Err(err("At most 20 tags".into()));
    }
    out.tags = tags;

    match out.auth_method {
        AuthMethod::Key => {
            let p = out
                .key_path
                .as_ref()
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty())
                .ok_or_else(|| err("Choose a private key file".into()))?;
            if !Path::new(&p).is_file() {
                return Err(err("Private key file not found".into()));
            }
            out.key_path = Some(p);
        }
        _ => out.key_path = None,
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::servers::sample_input;

    #[test]
    fn hosts() {
        for ok in ["10.0.0.1", "example.com", "web-01.prod.local", "::1", "[fe80::1]", "2001:db8::42", "my_host", "localhost", "fqdn.example."] {
            assert!(validate_host(ok).is_ok(), "{ok}");
        }
        for bad in ["", " ", "-oProxyCommand=x", "a b", "300.1.1.1", "bad..host", "-host", "host-.com", "h\u{0}st", "foo;rm", "a/b", &"a".repeat(64)] {
            assert!(validate_host(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn usernames() {
        assert!(validate_username("root").is_ok());
        assert!(validate_username("DOMAIN\\user").is_ok());
        assert!(validate_username("user@corp.local").is_ok());
        assert!(validate_username("").is_err());
        assert!(validate_username("-l").is_err());
        assert!(validate_username("a\nb").is_err());
    }

    #[test]
    fn normalizes_input() {
        let mut i = sample_input();
        i.name = "  web  ".into();
        i.tags = vec![" Web ".into(), "web".into(), "".into(), "DB".into()];
        i.group_name = Some("   ".into());
        let out = validate_server_input(&i).unwrap();
        assert_eq!(out.name, "web");
        assert_eq!(out.tags, vec!["web", "db"]);
        assert_eq!(out.group_name, None);
    }

    #[test]
    fn rejects_bad_profiles() {
        let mut i = sample_input();
        i.port = 0;
        assert!(validate_server_input(&i).is_err());

        let mut i = sample_input();
        i.name = "".into();
        assert!(validate_server_input(&i).is_err());

        let mut i = sample_input();
        i.auth_method = AuthMethod::Key;
        i.key_path = Some("Z:\\definitely\\missing\\id_ed25519".into());
        assert!(validate_server_input(&i).is_err());

        let mut i = sample_input();
        i.auth_method = AuthMethod::Key;
        i.key_path = None;
        assert!(validate_server_input(&i).is_err());
    }

    #[test]
    fn key_path_cleared_for_non_key_auth() {
        let mut i = sample_input();
        i.key_path = Some("whatever".into());
        assert_eq!(validate_server_input(&i).unwrap().key_path, None);
    }
}
