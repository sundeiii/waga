//! Windows provider: one PowerShell script via -EncodedCommand (no quoting
//! issues whether the remote default shell is cmd.exe or PowerShell).

use base64::Engine;
use serde::Deserialize;

use super::linux::clean;
use super::{DiskUsage, RawSnapshot};

const SCRIPT: &str = r#"$ErrorActionPreference = 'SilentlyContinue'
$ProgressPreference = 'SilentlyContinue'
$os = Get-CimInstance Win32_OperatingSystem
$cpu = Get-CimInstance Win32_Processor
$load = ($cpu | Measure-Object -Property LoadPercentage -Average).Average
$cores = ($cpu | Measure-Object -Property NumberOfLogicalProcessors -Sum).Sum
$disks = @(Get-CimInstance Win32_LogicalDisk -Filter 'DriveType=3' | ForEach-Object { [pscustomobject]@{ name = $_.DeviceID; size = [int64]$_.Size; free = [int64]$_.FreeSpace } })
$net = @(Get-CimInstance Win32_PerfRawData_Tcpip_NetworkInterface | ForEach-Object { [pscustomobject]@{ rx = [int64]$_.BytesReceivedPersec; tx = [int64]$_.BytesSentPersec } })
[pscustomobject]@{
  yoru = 1
  hostname = $env:COMPUTERNAME
  caption = $os.Caption
  version = $os.Version
  totalKb = [int64]$os.TotalVisibleMemorySize
  freeKb = [int64]$os.FreePhysicalMemory
  uptimeSec = if ($os.LastBootUpTime) { [int64]((Get-Date) - $os.LastBootUpTime).TotalSeconds } else { $null }
  cpu = $load
  cores = $cores
  disks = $disks
  net = $net
} | ConvertTo-Json -Compress -Depth 4
"#;

pub fn command() -> String {
    let utf16: Vec<u8> = SCRIPT.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    let b64 = base64::engine::general_purpose::STANDARD.encode(utf16);
    format!("powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand {b64}")
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Disk {
    name: Option<String>,
    size: Option<i64>,
    free: Option<i64>,
}

#[derive(Deserialize)]
struct Net {
    rx: Option<i64>,
    tx: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Raw {
    yoru: Option<u32>,
    hostname: Option<String>,
    caption: Option<String>,
    version: Option<String>,
    total_kb: Option<i64>,
    free_kb: Option<i64>,
    uptime_sec: Option<i64>,
    cpu: Option<f64>,
    cores: Option<i64>,
    #[serde(default)]
    disks: OneOrMany<Disk>,
    #[serde(default)]
    net: OneOrMany<Net>,
}

/// ConvertTo-Json emits a bare object for single-element arrays in some PS versions.
#[derive(Deserialize)]
#[serde(untagged)]
enum OneOrMany<T> {
    Many(Vec<T>),
    One(T),
}

impl<T> Default for OneOrMany<T> {
    fn default() -> Self {
        OneOrMany::Many(Vec::new())
    }
}

impl<T> OneOrMany<T> {
    fn into_vec(self) -> Vec<T> {
        match self {
            OneOrMany::Many(v) => v,
            OneOrMany::One(x) => vec![x],
        }
    }
}

pub fn parse(out: &str) -> Result<RawSnapshot, String> {
    let start = out.find('{').ok_or("Unexpected output from server. Is PowerShell available? (For Linux, change the profile type)")?;
    let end = out.rfind('}').ok_or("Truncated PowerShell output")?;
    let raw: Raw = serde_json::from_str(&out[start..=end]).map_err(|e| format!("Could not parse PowerShell output: {e}"))?;
    if raw.yoru != Some(1) {
        return Err("Unexpected PowerShell output".into());
    }
    let pos = |v: Option<i64>| v.filter(|x| *x >= 0).map(|x| x as u64);
    let total = pos(raw.total_kb).map(|k| k * 1024);
    let free = pos(raw.free_kb).map(|k| k * 1024);
    let net: Vec<Net> = raw.net.into_vec();
    let net_totals = (!net.is_empty()).then(|| {
        net.iter().fold((0u64, 0u64), |(r, t), n| (r.saturating_add(pos(n.rx).unwrap_or(0)), t.saturating_add(pos(n.tx).unwrap_or(0))))
    });
    let os = match (raw.caption, raw.version) {
        (Some(c), Some(v)) => Some(format!("{} ({v})", c.trim())),
        (Some(c), None) => Some(c),
        _ => None,
    };
    Ok(RawSnapshot {
        cpu_counters: None,
        cpu_percent: raw.cpu.filter(|c| (0.0..=100.0).contains(c)),
        cpu_cores: pos(raw.cores).map(|c| c as u32),
        mem_total_bytes: total,
        mem_used_bytes: match (total, free) {
            (Some(t), Some(f)) if f <= t => Some(t - f),
            _ => None,
        },
        disks: raw
            .disks
            .into_vec()
            .into_iter()
            .filter_map(|d| {
                let size = pos(d.size)?;
                let free = pos(d.free)?;
                (size > 0 && free <= size).then(|| DiskUsage { mount: clean(&d.name.unwrap_or_default(), 64), total_bytes: size, used_bytes: size - free })
            })
            .collect(),
        net_totals,
        load: None,
        uptime_secs: pos(raw.uptime_sec),
        hostname: raw.hostname.map(|h| clean(&h, 255)).filter(|h| !h.is_empty()),
        os: os.map(|o| clean(&o, 120)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_typical_output() {
        let out = r#"{"yoru":1,"hostname":"WIN-BUILD","caption":"Microsoft Windows Server 2022 Standard","version":"10.0.20348","totalKb":16777216,"freeKb":4194304,"uptimeSec":86400,"cpu":12.5,"cores":8,"disks":[{"name":"C:","size":107374182400,"free":53687091200},{"name":"D:","size":0,"free":0}],"net":[{"rx":1000,"tx":200},{"rx":50,"tx":5}]}"#;
        let s = parse(out).unwrap();
        assert_eq!(s.cpu_percent, Some(12.5));
        assert_eq!(s.cpu_cores, Some(8));
        assert_eq!(s.mem_total_bytes, Some(16777216 * 1024));
        assert_eq!(s.mem_used_bytes, Some((16777216 - 4194304) * 1024));
        assert_eq!(s.disks.len(), 1, "zero-size disks skipped");
        assert_eq!(s.disks[0].used_bytes, 53687091200);
        assert_eq!(s.net_totals, Some((1050, 205)));
        assert_eq!(s.uptime_secs, Some(86400));
        assert_eq!(s.os.as_deref(), Some("Microsoft Windows Server 2022 Standard (10.0.20348)"));
    }

    #[test]
    fn single_disk_object_and_null_cpu() {
        let out = "#< CLIXML\r\n{\"yoru\":1,\"cpu\":null,\"totalKb\":100,\"freeKb\":50,\"disks\":{\"name\":\"C:\",\"size\":10,\"free\":4},\"net\":[]}\r\n";
        let s = parse(out).unwrap();
        assert_eq!(s.cpu_percent, None, "unavailable stays unavailable");
        assert_eq!(s.disks.len(), 1);
        assert_eq!(s.net_totals, None);
    }

    #[test]
    fn rejects_non_powershell_output() {
        assert!(parse("bash: powershell.exe: command not found").is_err());
        assert!(parse("{\"something\":\"else\"}").is_err());
    }

    #[test]
    fn command_is_encoded() {
        let c = command();
        assert!(c.contains("-EncodedCommand "));
        assert!(!c.contains("Get-CimInstance"), "script must not be passed in clear text");
    }
}
