//! Local shell profiles, read from Windows Terminal's settings.json when present.
//! The frontend only ever sends a profile id; the command line is resolved here.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use super::text::{expand_env, split_cmdline, strip_jsonc};

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LocalProfile {
    pub id: String,
    pub name: String,
    /// "windowsTerminal" or "builtin"
    pub source: String,
    #[serde(skip)]
    pub argv: Vec<String>,
    /// Shown in the UI (resolved program path).
    pub program: String,
    #[serde(skip)]
    pub cwd: Option<PathBuf>,
    pub is_default: bool,
    /// xterm.js theme (keys: background, foreground, black … brightWhite, cursor, selectionBackground).
    pub theme: Option<HashMap<String, String>>,
    pub color_scheme: Option<String>,
    pub font_face: Option<String>,
    /// Font size converted from points to CSS px.
    pub font_size_px: Option<u32>,
    /// Background opacity 0.3–1.0 from the WT profile's `opacity` (0–100).
    pub opacity: Option<f32>,
}

pub fn wt_settings_paths() -> Vec<PathBuf> {
    let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) else { return vec![] };
    vec![
        local.join(r"Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe\LocalState\settings.json"),
        local.join(r"Packages\Microsoft.WindowsTerminalPreview_8wekyb3d8bbwe\LocalState\settings.json"),
        local.join(r"Microsoft\Windows Terminal\settings.json"),
    ]
}

fn env_get(k: &str) -> Option<String> {
    std::env::var(k).ok()
}

/// Find an executable on PATH (plus WindowsApps aliases, which are on PATH).
pub fn which(exe: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|d| d.join(exe)).find(|p| p.is_file() || p.exists())
}

fn resolve_program(argv0: &str) -> String {
    let p = Path::new(argv0);
    if p.is_absolute() {
        return argv0.to_string();
    }
    which(argv0).or_else(|| which(&format!("{argv0}.exe"))).map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| argv0.to_string())
}

/// Map a WT color scheme object to an xterm.js theme.
fn scheme_to_theme(s: &Value) -> HashMap<String, String> {
    let mut t = HashMap::new();
    for (wt, xt) in [
        ("background", "background"),
        ("foreground", "foreground"),
        ("cursorColor", "cursor"),
        ("selectionBackground", "selectionBackground"),
        ("black", "black"),
        ("red", "red"),
        ("green", "green"),
        ("yellow", "yellow"),
        ("blue", "blue"),
        ("purple", "magenta"),
        ("cyan", "cyan"),
        ("white", "white"),
        ("brightBlack", "brightBlack"),
        ("brightRed", "brightRed"),
        ("brightGreen", "brightGreen"),
        ("brightYellow", "brightYellow"),
        ("brightBlue", "brightBlue"),
        ("brightPurple", "brightMagenta"),
        ("brightCyan", "brightCyan"),
        ("brightWhite", "brightWhite"),
    ] {
        if let Some(v) = s.get(wt).and_then(Value::as_str) {
            if v.len() == 7 && v.starts_with('#') && v[1..].chars().all(|c| c.is_ascii_hexdigit()) {
                t.insert(xt.to_string(), v.to_string());
            }
        }
    }
    if let Some(sel) = t.get("selectionBackground").cloned() {
        // WT draws selection translucent; mimic that.
        t.insert("selectionBackground".into(), format!("{sel}55"));
    }
    t
}

/// Built-in Windows Terminal schemes (not stored in settings.json).
fn builtin_scheme(name: &str) -> Option<Value> {
    let (bg, fg, c): (&str, &str, [&str; 16]) = match name {
        "Campbell" => ("#0C0C0C", "#CCCCCC", ["#0C0C0C", "#C50F1F", "#13A10E", "#C19C00", "#0037DA", "#881798", "#3A96DD", "#CCCCCC", "#767676", "#E74856", "#16C60C", "#F9F1A5", "#3B78FF", "#B4009E", "#61D6D6", "#F2F2F2"]),
        "Campbell Powershell" => ("#012456", "#CCCCCC", ["#0C0C0C", "#C50F1F", "#13A10E", "#C19C00", "#0037DA", "#881798", "#3A96DD", "#CCCCCC", "#767676", "#E74856", "#16C60C", "#F9F1A5", "#3B78FF", "#B4009E", "#61D6D6", "#F2F2F2"]),
        "One Half Dark" => ("#282C34", "#DCDFE4", ["#282C34", "#E06C75", "#98C379", "#E5C07B", "#61AFEF", "#C678DD", "#56B6C2", "#DCDFE4", "#5A6374", "#E06C75", "#98C379", "#E5C07B", "#61AFEF", "#C678DD", "#56B6C2", "#DCDFE4"]),
        "One Half Light" => ("#FAFAFA", "#383A42", ["#383A42", "#E45649", "#50A14F", "#C18301", "#0184BC", "#A626A4", "#0997B3", "#FAFAFA", "#4F525D", "#DF6C75", "#98C379", "#E4C07A", "#61AFEF", "#C577DD", "#56B5C1", "#FFFFFF"]),
        "Tango Dark" => ("#000000", "#D3D7CF", ["#000000", "#CC0000", "#4E9A06", "#C4A000", "#3465A4", "#75507B", "#06989A", "#D3D7CF", "#555753", "#EF2929", "#8AE234", "#FCE94F", "#729FCF", "#AD7FA8", "#34E2E2", "#EEEEEC"]),
        "Solarized Dark" => ("#002B36", "#839496", ["#002B36", "#DC322F", "#859900", "#B58900", "#268BD2", "#D33682", "#2AA198", "#EEE8D5", "#073642", "#CB4B16", "#586E75", "#657B83", "#839496", "#6C71C4", "#93A1A1", "#FDF6E3"]),
        "Vintage" => ("#000000", "#C0C0C0", ["#000000", "#800000", "#008000", "#808000", "#000080", "#800080", "#008080", "#C0C0C0", "#808080", "#FF0000", "#00FF00", "#FFFF00", "#0000FF", "#FF00FF", "#00FFFF", "#FFFFFF"]),
        _ => return None,
    };
    let keys = ["black", "red", "green", "yellow", "blue", "purple", "cyan", "white", "brightBlack", "brightRed", "brightGreen", "brightYellow", "brightBlue", "brightPurple", "brightCyan", "brightWhite"];
    let mut m = serde_json::Map::new();
    m.insert("background".into(), bg.into());
    m.insert("foreground".into(), fg.into());
    m.insert("cursorColor".into(), "#FFFFFF".into());
    m.insert("selectionBackground".into(), "#FFFFFF".into());
    for (k, v) in keys.iter().zip(c) {
        m.insert((*k).into(), v.into());
    }
    Some(Value::Object(m))
}

fn builtin_profiles() -> Vec<LocalProfile> {
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
    let mut v = vec![
        mk("builtin-powershell", "Windows PowerShell", "builtin", format!(r"{root}\System32\WindowsPowerShell\v1.0\powershell.exe")),
        mk("builtin-cmd", "Command Prompt", "builtin", format!(r"{root}\System32\cmd.exe")),
    ];
    if let Some(p) = which("pwsh.exe") {
        v.insert(0, mk("builtin-pwsh", "PowerShell 7", "builtin", p.to_string_lossy().into_owned()));
    }
    if let Some(first) = v.first_mut() {
        first.is_default = true;
    }
    v
}

fn mk(id: &str, name: &str, source: &str, program: String) -> LocalProfile {
    LocalProfile {
        id: id.into(),
        name: name.into(),
        source: source.into(),
        argv: vec![program.clone()],
        program,
        cwd: None,
        is_default: false,
        theme: None,
        color_scheme: None,
        font_face: None,
        font_size_px: None,
        opacity: None,
    }
}

/// Parse a Windows Terminal settings.json document.
pub fn parse_wt_settings(json: &str, get_env: &dyn Fn(&str) -> Option<String>, find_exe: &dyn Fn(&str) -> Option<String>) -> Result<Vec<LocalProfile>, String> {
    let v: Value = serde_json::from_str(&strip_jsonc(json)).map_err(|e| format!("Invalid Windows Terminal settings: {e}"))?;
    let default_guid = v.get("defaultProfile").and_then(Value::as_str).unwrap_or("").to_ascii_lowercase();
    let defaults = v.pointer("/profiles/defaults").cloned().unwrap_or(Value::Null);
    let list = v
        .pointer("/profiles/list")
        .or_else(|| v.get("profiles").filter(|p| p.is_array()))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let schemes: Vec<Value> = v.get("schemes").and_then(Value::as_array).cloned().unwrap_or_default();
    let find_scheme = |name: &str| schemes.iter().find(|s| s.get("name").and_then(Value::as_str) == Some(name)).cloned().or_else(|| builtin_scheme(name));

    let mut out = Vec::new();
    for (idx, p) in list.iter().enumerate() {
        if p.get("hidden").and_then(Value::as_bool).unwrap_or(false) {
            continue;
        }
        let get = |k: &str| p.get(k).or_else(|| defaults.get(k));
        let name = p.get("name").and_then(Value::as_str).unwrap_or("Profile").to_string();
        let guid = p.get("guid").and_then(Value::as_str).map(|g| g.to_ascii_lowercase());
        let source = p.get("source").and_then(Value::as_str).unwrap_or("");

        let commandline = match (p.get("commandline").and_then(Value::as_str), source) {
            (Some(c), _) => expand_env(c, get_env),
            (None, "Windows.Terminal.PowershellCore") => match find_exe("pwsh.exe") {
                Some(p) => format!("\"{p}\""),
                None => continue,
            },
            (None, "Windows.Terminal.Wsl") => format!("wsl.exe -d \"{name}\""),
            (None, "") => expand_env(r"%SystemRoot%\System32\cmd.exe", get_env),
            // Azure Cloud Shell, Visual Studio dev shells, etc. need WT itself.
            (None, _) => continue,
        };
        let argv = split_cmdline(&commandline);
        if argv.is_empty() {
            continue;
        }
        let cwd = get("startingDirectory")
            .and_then(Value::as_str)
            .map(|d| PathBuf::from(expand_env(d, get_env)))
            .filter(|d| !d.as_os_str().is_empty());
        let color_scheme = match get("colorScheme") {
            Some(Value::String(s)) => Some(s.clone()),
            Some(Value::Object(o)) => o.get("dark").and_then(Value::as_str).map(String::from),
            _ => None,
        };
        let theme = color_scheme.as_deref().and_then(find_scheme).map(|s| scheme_to_theme(&s)).filter(|t| !t.is_empty());
        let font = get("font");
        let font_face = font
            .and_then(|f| f.get("face"))
            .or_else(|| get("fontFace"))
            .and_then(Value::as_str)
            .map(|s| s.split(',').next().unwrap_or(s).trim().to_string());
        let font_size_px = font
            .and_then(|f| f.get("size"))
            .or_else(|| get("fontSize"))
            .and_then(Value::as_f64)
            .filter(|s| (4.0..=72.0).contains(s))
            .map(|pt| (pt * 96.0 / 72.0).round() as u32);

        let opacity = get("opacity").and_then(Value::as_f64).map(|o| (o / 100.0).clamp(0.3, 1.0) as f32);
        let id = guid.clone().unwrap_or_else(|| format!("wt-{idx}"));
        out.push(LocalProfile {
            is_default: guid.as_deref() == Some(default_guid.as_str()),
            program: argv[0].clone(),
            id,
            name,
            source: "windowsTerminal".into(),
            argv,
            cwd,
            theme,
            color_scheme,
            font_face,
            font_size_px,
            opacity,
        });
    }
    if !out.iter().any(|p| p.is_default) {
        if let Some(f) = out.first_mut() {
            f.is_default = true;
        }
    }
    Ok(out)
}

/// Windows Terminal profiles if available, otherwise sensible built-ins.
pub fn load_profiles() -> Vec<LocalProfile> {
    for path in wt_settings_paths() {
        if let Ok(text) = std::fs::read_to_string(&path) {
            match parse_wt_settings(&text, &env_get, &|e| which(e).map(|p| p.to_string_lossy().into_owned())) {
                Ok(mut list) if !list.is_empty() => {
                    for p in &mut list {
                        p.program = resolve_program(&p.argv[0]);
                    }
                    return list;
                }
                Ok(_) => {}
                Err(e) => log::warn!("{e}"),
            }
        }
    }
    builtin_profiles()
}

#[cfg(test)]
mod tests {
    use super::*;

    const WT: &str = r##"{
        "$schema": "https://aka.ms/terminal-profiles-schema",
        "defaultProfile": "{61C54BBD-C2C6-5271-96E7-009A87FF44BF}",
        "profiles": {
            "defaults": { "colorScheme": "One Half Dark", "font": { "face": "CaskaydiaCove Nerd Font", "size": 11 }, "opacity": 85 },
            "list": [
                { "guid": "{61c54bbd-c2c6-5271-96e7-009a87ff44bf}", "name": "PowerShell", "commandline": "%SystemRoot%\\System32\\WindowsPowerShell\\v1.0\\powershell.exe", "hidden": false },
                { "guid": "{0caa0dad-35be-5f56-a8ff-afceeeaa6101}", "name": "Command Prompt", "commandline": "%SystemRoot%\\System32\\cmd.exe", "colorScheme": "My Scheme", "startingDirectory": "%USERPROFILE%\\code" },
                { "guid": "{b453ae62-4e3d-5e58-b989-0a998ec441b8}", "name": "Azure Cloud Shell", "source": "Windows.Terminal.Azure" },
                { "guid": "{574e775e-4f2a-5b96-ac1e-a2962a402336}", "name": "PowerShell", "source": "Windows.Terminal.PowershellCore" },
                { "guid": "{aaaa}", "name": "Ubuntu-24.04", "source": "Windows.Terminal.Wsl" },
                { "guid": "{bbbb}", "name": "Hidden one", "commandline": "x.exe", "hidden": true }, // trailing comment
            ]
        },
        "schemes": [ { "name": "My Scheme", "background": "#101010", "foreground": "#EEEEEE", "purple": "#AA00AA", "selectionBackground": "#334455", "red": "bad" } ],
    }"##;

    fn env(k: &str) -> Option<String> {
        match k.to_ascii_uppercase().as_str() {
            "SYSTEMROOT" => Some(r"C:\Windows".into()),
            "USERPROFILE" => Some(r"C:\Users\me".into()),
            _ => None,
        }
    }

    #[test]
    fn parses_wt_profiles() {
        let list = parse_wt_settings(WT, &env, &|e| (e == "pwsh.exe").then(|| r"C:\Program Files\PowerShell\7\pwsh.exe".to_string())).unwrap();
        let names: Vec<_> = list.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["PowerShell", "Command Prompt", "PowerShell", "Ubuntu-24.04"], "Azure and hidden skipped");

        let ps = &list[0];
        assert!(ps.is_default, "defaultProfile matched case-insensitively");
        assert_eq!(ps.argv, vec![r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe"]);
        assert_eq!(ps.color_scheme.as_deref(), Some("One Half Dark"));
        assert_eq!(ps.theme.as_ref().unwrap()["background"], "#282C34");
        assert_eq!(ps.theme.as_ref().unwrap()["magenta"], "#C678DD", "WT purple maps to xterm magenta");
        assert_eq!(ps.font_face.as_deref(), Some("CaskaydiaCove Nerd Font"));
        assert_eq!(ps.font_size_px, Some(15), "11pt -> 15px");
        assert_eq!(ps.opacity, Some(0.85), "WT opacity 85 -> 0.85");

        let cmd = &list[1];
        assert_eq!(cmd.cwd.as_deref(), Some(Path::new(r"C:\Users\me\code")));
        let t = cmd.theme.as_ref().unwrap();
        assert_eq!(t["background"], "#101010");
        assert_eq!(t["selectionBackground"], "#33445555");
        assert!(!t.contains_key("red"), "invalid colors dropped");

        assert_eq!(list[2].argv, vec![r"C:\Program Files\PowerShell\7\pwsh.exe"]);
        assert_eq!(list[3].argv, vec!["wsl.exe", "-d", "Ubuntu-24.04"]);
        assert!(list.iter().all(|p| !p.id.is_empty()));
    }

    #[test]
    fn pwsh_profile_skipped_when_not_installed() {
        let list = parse_wt_settings(WT, &env, &|_| None).unwrap();
        assert_eq!(list.len(), 3);
    }

    #[test]
    fn invalid_json_is_an_error() {
        assert!(parse_wt_settings("{ nope", &env, &|_| None).is_err());
    }

    #[test]
    fn builtins_exist() {
        let b = builtin_profiles();
        assert!(b.iter().any(|p| p.name == "Command Prompt"));
        assert_eq!(b.iter().filter(|p| p.is_default).count(), 1);
    }
}
