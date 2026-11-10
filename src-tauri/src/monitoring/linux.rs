//! Linux provider: reads /proc and POSIX `df` in one constant `sh` script.
//! Works on Debian/Ubuntu, RHEL-likes, Alpine (busybox), Proxmox.

use super::{CpuCounters, DiskUsage, RawSnapshot};

/// Constant script; contains no single quotes so it can be wrapped in '...'.
/// Missing files simply yield empty sections.
const SCRIPT: &str = "LC_ALL=C; export LC_ALL; \
echo @@stat; head -n 1 /proc/stat 2>/dev/null; \
echo @@cpus; grep -c ^processor /proc/cpuinfo 2>/dev/null; \
echo @@meminfo; cat /proc/meminfo 2>/dev/null; \
echo @@loadavg; cat /proc/loadavg 2>/dev/null; \
echo @@uptime; cat /proc/uptime 2>/dev/null; \
echo @@hostname; cat /proc/sys/kernel/hostname 2>/dev/null || hostname 2>/dev/null; \
echo @@os; cat /etc/os-release 2>/dev/null; \
echo @@uname; uname -sr 2>/dev/null; \
echo @@df; df -P -k 2>/dev/null; \
echo @@netdev; cat /proc/net/dev 2>/dev/null; \
echo @@end";

pub fn command() -> String {
    format!("/bin/sh -c '{SCRIPT}'")
}

fn section<'a>(out: &'a str, name: &str) -> Option<&'a str> {
    let marker = format!("@@{name}\n");
    let start = out.find(&marker)? + marker.len();
    let rest = &out[start..];
    let end = rest.find("\n@@").map(|i| i + 1).or_else(|| rest.find("@@")).unwrap_or(rest.len());
    Some(&rest[..end])
}

pub fn parse_stat(s: &str) -> Option<CpuCounters> {
    let line = s.lines().find(|l| l.starts_with("cpu "))?;
    let v: Vec<u64> = line.split_whitespace().skip(1).filter_map(|x| x.parse().ok()).collect();
    if v.len() < 4 {
        return None;
    }
    // user nice system idle iowait irq softirq steal (guest is already in user)
    let total: u64 = v.iter().take(8).sum();
    let idle = v[3] + v.get(4).copied().unwrap_or(0);
    Some(CpuCounters { total, idle })
}

pub fn parse_meminfo(s: &str) -> (Option<u64>, Option<u64>) {
    let get = |key: &str| {
        s.lines()
            .find(|l| l.starts_with(key) && l[key.len()..].starts_with(':'))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| v.parse::<u64>().ok())
            .map(|kb| kb * 1024)
    };
    let total = get("MemTotal");
    let available = get("MemAvailable").or_else(|| Some(get("MemFree")? + get("Buffers").unwrap_or(0) + get("Cached").unwrap_or(0)));
    let used = match (total, available) {
        (Some(t), Some(a)) if a <= t => Some(t - a),
        _ => None,
    };
    (total, used)
}

pub fn parse_df(s: &str) -> Vec<DiskUsage> {
    let mut out: Vec<DiskUsage> = Vec::new();
    let mut seen_fs: Vec<String> = Vec::new();
    for line in s.lines().skip(1) {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() < 6 {
            continue;
        }
        let (fs, used, avail) = (f[0], f[2].parse::<u64>(), f[3].parse::<u64>());
        let mount = f[5..].join(" ");
        let real = (fs.starts_with("/dev/") && !fs.starts_with("/dev/loop")) || mount == "/" || fs.contains(":/") || fs.starts_with("//");
        if !real || seen_fs.iter().any(|x| x == fs) {
            continue;
        }
        if let (Ok(used), Ok(avail)) = (used, avail) {
            seen_fs.push(fs.to_string());
            // Same basis as df's "Use%": used / (used + available).
            out.push(DiskUsage { mount, total_bytes: (used + avail) * 1024, used_bytes: used * 1024 });
        }
    }
    out.sort_by(|a, b| (a.mount != "/").cmp(&(b.mount != "/")).then(a.mount.cmp(&b.mount)));
    out.truncate(12);
    out
}

pub fn parse_netdev(s: &str) -> Option<(u64, u64)> {
    let mut rx = 0u64;
    let mut tx = 0u64;
    let mut any = false;
    for line in s.lines() {
        let Some((iface, rest)) = line.split_once(':') else { continue };
        let iface = iface.trim();
        if iface == "lo" || iface.contains('|') {
            continue;
        }
        let v: Vec<u64> = rest.split_whitespace().filter_map(|x| x.parse().ok()).collect();
        if v.len() >= 9 {
            rx = rx.saturating_add(v[0]);
            tx = tx.saturating_add(v[8]);
            any = true;
        }
    }
    any.then_some((rx, tx))
}

fn parse_os(os_release: &str, uname: &str) -> Option<String> {
    let pretty = os_release
        .lines()
        .find_map(|l| l.strip_prefix("PRETTY_NAME="))
        .map(|v| v.trim().trim_matches('"').to_string())
        .filter(|v| !v.is_empty());
    pretty.or_else(|| Some(uname.trim().to_string()).filter(|s| !s.is_empty()))
}

/// Strip control characters: remote output is untrusted and goes to the UI.
pub fn clean(s: &str, max: usize) -> String {
    s.chars().filter(|c| !c.is_control()).take(max).collect::<String>().trim().to_string()
}

pub fn parse(out: &str) -> Result<RawSnapshot, String> {
    let out = out.replace("\r\n", "\n");
    if !out.contains("@@stat") || !out.contains("@@end") {
        return Err("Unexpected output from server. Is this a Linux host? (For Windows, set the profile to “Windows (OpenSSH)”)".into());
    }
    let sec = |n| section(&out, n).unwrap_or("");
    let (mem_total_bytes, mem_used_bytes) = parse_meminfo(sec("meminfo"));
    let load = {
        let v: Vec<f64> = sec("loadavg").split_whitespace().take(3).filter_map(|x| x.parse().ok()).collect();
        (v.len() == 3).then(|| [v[0], v[1], v[2]])
    };
    let snap = RawSnapshot {
        cpu_counters: parse_stat(sec("stat")),
        cpu_percent: None,
        cpu_cores: sec("cpus").trim().parse().ok().filter(|n: &u32| *n > 0),
        mem_total_bytes,
        mem_used_bytes,
        disks: parse_df(sec("df")),
        net_totals: parse_netdev(sec("netdev")),
        load,
        uptime_secs: sec("uptime").split_whitespace().next().and_then(|x| x.parse::<f64>().ok()).map(|f| f as u64),
        hostname: Some(clean(sec("hostname"), 255)).filter(|s| !s.is_empty()),
        os: parse_os(sec("os"), sec("uname")).map(|s| clean(&s, 120)),
    };
    if snap.cpu_counters.is_none() && snap.mem_total_bytes.is_none() {
        return Err("Could not read /proc on this host (unsupported system?)".into());
    }
    Ok(snap)
}

#[cfg(test)]
pub(crate) const SAMPLE_OUTPUT: &str = "@@stat
cpu  4705 356 584 3699176 2311 0 77 0 0 0
@@cpus
4
@@meminfo
MemTotal:        8040424 kB
MemFree:         1290724 kB
MemAvailable:    5932288 kB
Buffers:          201644 kB
Cached:          4233580 kB
@@loadavg
0.15 0.21 0.30 1/412 23140
@@uptime
356021.53 1395232.10
@@hostname
web-01
@@os
PRETTY_NAME=\"Debian GNU/Linux 12 (bookworm)\"
NAME=\"Debian GNU/Linux\"
@@uname
Linux 6.1.0-18-amd64
@@df
Filesystem     1024-blocks     Used Available Capacity Mounted on
udev               4000000        0   4000000       0% /dev
tmpfs               804044     1136    802908       1% /run
/dev/sda1         30832548 12395316  16848208      43% /
/dev/sda1         30832548 12395316  16848208      43% /var/lib/docker/bind
/dev/sdb1        103081248  5242880  92579136       6% /srv/my data
/dev/loop0           65536    65536         0     100% /snap/core/1
overlay           30832548 12395316  16848208      43% /var/lib/docker/overlay2/x/merged
@@netdev
Inter-|   Receive                                                |  Transmit
 face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed
    lo: 9999999   1000    0    0    0     0          0         0  9999999    1000    0    0    0     0       0          0
  eth0: 1000000   2000    0    0    0     0          0         0   500000    1500    0    0    0     0       0          0
 wlan0:  234567    100    0    0    0     0          0         0    65432      90    0    0    0     0       0          0
@@end
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_output() {
        let s = parse(SAMPLE_OUTPUT).unwrap();
        assert_eq!(s.cpu_counters, Some(CpuCounters { total: 4705 + 356 + 584 + 3699176 + 2311 + 77, idle: 3699176 + 2311 }));
        assert_eq!(s.cpu_cores, Some(4));
        assert_eq!(s.mem_total_bytes, Some(8040424 * 1024));
        assert_eq!(s.mem_used_bytes, Some((8040424 - 5932288) * 1024));
        assert_eq!(s.load, Some([0.15, 0.21, 0.30]));
        assert_eq!(s.uptime_secs, Some(356021));
        assert_eq!(s.hostname.as_deref(), Some("web-01"));
        assert_eq!(s.os.as_deref(), Some("Debian GNU/Linux 12 (bookworm)"));
        assert_eq!(s.net_totals, Some((1000000 + 234567, 500000 + 65432)));
        let mounts: Vec<_> = s.disks.iter().map(|d| d.mount.as_str()).collect();
        assert_eq!(mounts, vec!["/", "/srv/my data"], "tmpfs, loop, overlay and duplicate bind mounts excluded");
        assert_eq!(s.disks[0].used_bytes, 12395316 * 1024);
        assert_eq!(s.disks[0].total_bytes, (12395316 + 16848208) * 1024);
    }

    #[test]
    fn crlf_and_missing_sections_are_tolerated() {
        let out = "@@stat\r\ncpu  1 2 3 4\r\n@@meminfo\r\nMemTotal: 1000 kB\r\nMemFree: 400 kB\r\n@@end\r\n";
        let s = parse(out).unwrap();
        assert_eq!(s.cpu_counters, Some(CpuCounters { total: 10, idle: 4 }));
        assert_eq!(s.mem_used_bytes, Some(600 * 1024), "falls back to MemFree when MemAvailable is missing");
        assert!(s.disks.is_empty());
        assert_eq!(s.net_totals, None);
        assert_eq!(s.load, None);
    }

    #[test]
    fn windows_or_garbage_output_is_an_error() {
        assert!(parse("'/bin/sh' is not recognized as an internal or external command").is_err());
        assert!(parse("@@stat\n@@end\n").is_err());
    }

    #[test]
    fn untrusted_strings_are_cleaned() {
        let out = SAMPLE_OUTPUT.replace("web-01", "evil\x1b]0;pwned\x07host");
        let s = parse(&out).unwrap();
        assert_eq!(s.hostname.as_deref(), Some("evil]0;pwnedhost"));
    }

    #[test]
    fn script_is_safe_to_single_quote() {
        assert!(!SCRIPT.contains('\''));
        assert!(command().starts_with("/bin/sh -c '"));
    }
}
