//! Small text helpers for Windows Terminal settings and command lines.

/// Strip `//` and `/* */` comments and trailing commas from JSONC.
/// String contents (including escaped quotes) are preserved.
pub fn strip_jsonc(src: &str) -> String {
    strip_trailing_commas(&strip_comments(src))
}

fn strip_comments(src: &str) -> String {
    let c: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let (mut i, mut in_str) = (0, false);
    while i < c.len() {
        if in_str {
            out.push(c[i]);
            if c[i] == '\\' && i + 1 < c.len() {
                out.push(c[i + 1]);
                i += 2;
                continue;
            }
            in_str = c[i] != '"';
            i += 1;
            continue;
        }
        match (c[i], c.get(i + 1)) {
            ('"', _) => {
                in_str = true;
                out.push('"');
                i += 1;
            }
            ('/', Some('/')) => {
                while i < c.len() && c[i] != '\n' {
                    i += 1;
                }
            }
            ('/', Some('*')) => {
                i += 2;
                while i < c.len() && !(c[i] == '*' && c.get(i + 1) == Some(&'/')) {
                    i += 1;
                }
                i += 2;
            }
            (ch, _) => {
                out.push(ch);
                i += 1;
            }
        }
    }
    out
}

fn strip_trailing_commas(src: &str) -> String {
    let c: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let (mut i, mut in_str) = (0, false);
    while i < c.len() {
        let ch = c[i];
        if in_str {
            out.push(ch);
            if ch == '\\' && i + 1 < c.len() {
                out.push(c[i + 1]);
                i += 2;
                continue;
            }
            in_str = ch != '"';
        } else if ch == '"' {
            in_str = true;
            out.push(ch);
        } else if ch == ',' {
            let next = c[i + 1..].iter().find(|x| !x.is_whitespace());
            if !matches!(next, Some('}') | Some(']')) {
                out.push(ch);
            }
        } else {
            out.push(ch);
        }
        i += 1;
    }
    out
}

/// Expand `%VAR%` like Windows does; unknown variables are left as-is.
pub fn expand_env(s: &str, get: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) if end > 0 && !after[..end].contains(char::is_whitespace) => {
                let name = &after[..end];
                match get(name) {
                    Some(v) => out.push_str(&v),
                    None => {
                        out.push('%');
                        out.push_str(name);
                        out.push('%');
                    }
                }
                rest = &after[end + 1..];
            }
            _ => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Split a Windows command line into argv (CommandLineToArgvW rules,
/// simplified: quotes group, `\"` is a literal quote, `""` inside quotes is a quote).
pub fn split_cmdline(s: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut cur = String::new();
    let mut in_q = false;
    let mut has = false;
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\\' => {
                let mut n = 0;
                while i < chars.len() && chars[i] == '\\' {
                    n += 1;
                    i += 1;
                }
                if i < chars.len() && chars[i] == '"' {
                    cur.extend(std::iter::repeat_n('\\', n / 2));
                    if n % 2 == 1 {
                        cur.push('"');
                        i += 1;
                    }
                } else {
                    cur.extend(std::iter::repeat_n('\\', n));
                }
                has = true;
                continue;
            }
            '"' => {
                if in_q && i + 1 < chars.len() && chars[i + 1] == '"' {
                    cur.push('"');
                    i += 2;
                    continue;
                }
                in_q = !in_q;
                has = true;
            }
            c if c.is_whitespace() && !in_q => {
                if has {
                    args.push(std::mem::take(&mut cur));
                    has = false;
                }
            }
            c => {
                cur.push(c);
                has = true;
            }
        }
        i += 1;
    }
    if has {
        args.push(cur);
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jsonc() {
        let src = r#"{
            // comment with "quote"
            "a": "http://x//y", /* block */
            "b": [1, 2, ],
            "c": "esc \" // not comment",
            "d": "üñï",
        }"#;
        let v: serde_json::Value = serde_json::from_str(&strip_jsonc(src)).unwrap();
        assert_eq!(v["a"], "http://x//y");
        assert_eq!(v["b"], serde_json::json!([1, 2]));
        assert_eq!(v["c"], "esc \" // not comment");
        assert_eq!(v["d"], "üñï");
    }

    #[test]
    fn env_expansion() {
        let get = |k: &str| match k.to_ascii_uppercase().as_str() {
            "SYSTEMROOT" => Some("C:\\Windows".to_string()),
            "USERPROFILE" => Some("C:\\Users\\me".to_string()),
            _ => None,
        };
        assert_eq!(expand_env("%SystemRoot%\\System32\\cmd.exe", get), "C:\\Windows\\System32\\cmd.exe");
        assert_eq!(expand_env("%NOPE%\\x", get), "%NOPE%\\x");
        assert_eq!(expand_env("100% sure", get), "100% sure");
        assert_eq!(expand_env("%USERPROFILE%", get), "C:\\Users\\me");
    }

    #[test]
    fn cmdline_split() {
        assert_eq!(split_cmdline("cmd.exe"), vec!["cmd.exe"]);
        assert_eq!(split_cmdline(r#""C:\Program Files\PowerShell\7\pwsh.exe" -NoLogo"#), vec![r"C:\Program Files\PowerShell\7\pwsh.exe", "-NoLogo"]);
        assert_eq!(split_cmdline("wsl.exe -d Ubuntu-24.04"), vec!["wsl.exe", "-d", "Ubuntu-24.04"]);
        assert_eq!(split_cmdline(r#"a "b c" d\"e"#), vec!["a", "b c", "d\"e"]);
        assert_eq!(split_cmdline(r#"x """#), vec!["x", ""]);
        assert!(split_cmdline("   ").is_empty());
    }
}
