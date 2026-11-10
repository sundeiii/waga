//! Agentless monitoring over an existing authenticated SSH connection.
//! Each provider runs ONE constant command per sample and parses its output.
//! Rates (CPU %, network) are derived from deltas between consecutive real
//! samples; the first sample reports them as unavailable rather than guessing.

pub mod linux;
pub mod manager;
pub mod windows;

use std::time::Instant;

use serde::Serialize;

use crate::database::servers::ProfileKind;
use crate::ssh::connections::{exec, SshHandle};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskUsage {
    pub mount: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuCounters {
    pub total: u64,
    pub idle: u64,
}

/// What a provider could read in one go. Every field is optional: missing
/// data stays missing.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RawSnapshot {
    pub cpu_counters: Option<CpuCounters>,
    /// Providers that report utilisation directly (Windows).
    pub cpu_percent: Option<f64>,
    pub cpu_cores: Option<u32>,
    pub mem_total_bytes: Option<u64>,
    pub mem_used_bytes: Option<u64>,
    pub disks: Vec<DiskUsage>,
    /// Cumulative bytes (rx, tx) across non-loopback interfaces.
    pub net_totals: Option<(u64, u64)>,
    pub load: Option<[f64; 3]>,
    pub uptime_secs: Option<u64>,
    pub hostname: Option<String>,
    pub os: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sample {
    pub server_id: String,
    pub ts: String,
    pub cpu_percent: Option<f64>,
    pub cpu_cores: Option<u32>,
    pub mem_total_bytes: Option<u64>,
    pub mem_used_bytes: Option<u64>,
    pub disks: Vec<DiskUsage>,
    pub net_rx_bps: Option<f64>,
    pub net_tx_bps: Option<f64>,
    pub load: Option<[f64; 3]>,
    pub uptime_secs: Option<u64>,
    pub hostname: Option<String>,
    pub os: Option<String>,
}

/// Turns consecutive snapshots into a sample with rates.
#[derive(Default)]
pub struct Sampler {
    prev_cpu: Option<CpuCounters>,
    prev_net: Option<((u64, u64), Instant)>,
}

impl Sampler {
    pub fn sample(&mut self, server_id: &str, raw: RawSnapshot, now: Instant) -> Sample {
        let cpu_percent = raw.cpu_percent.or_else(|| {
            let cur = raw.cpu_counters?;
            let prev = self.prev_cpu.replace(cur)?;
            let total = cur.total.checked_sub(prev.total)?;
            let idle = cur.idle.checked_sub(prev.idle)?;
            if total == 0 || idle > total {
                return None;
            }
            Some(((total - idle) as f64 / total as f64 * 100.0).clamp(0.0, 100.0))
        });
        if raw.cpu_percent.is_some() {
            self.prev_cpu = raw.cpu_counters;
        }

        let (net_rx_bps, net_tx_bps) = match (raw.net_totals, self.prev_net) {
            (Some(cur), Some((prev, at))) => {
                let secs = now.duration_since(at).as_secs_f64();
                let rate = |c: u64, p: u64| (secs > 0.0 && c >= p).then(|| (c - p) as f64 / secs);
                (rate(cur.0, prev.0), rate(cur.1, prev.1))
            }
            _ => (None, None),
        };
        self.prev_net = raw.net_totals.map(|n| (n, now));

        Sample {
            server_id: server_id.to_string(),
            ts: crate::database::now(),
            cpu_percent,
            cpu_cores: raw.cpu_cores,
            mem_total_bytes: raw.mem_total_bytes,
            mem_used_bytes: raw.mem_used_bytes,
            disks: raw.disks,
            net_rx_bps,
            net_tx_bps,
            load: raw.load,
            uptime_secs: raw.uptime_secs,
            hostname: raw.hostname,
            os: raw.os,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Linux,
    Windows,
}

impl Provider {
    pub fn for_kind(kind: ProfileKind) -> Self {
        match kind {
            ProfileKind::Windows => Provider::Windows,
            _ => Provider::Linux,
        }
    }

    pub async fn collect(&self, handle: &SshHandle, timeout: std::time::Duration) -> Result<RawSnapshot, String> {
        match self {
            Provider::Linux => {
                let out = exec(handle, &linux::command(), timeout).await?;
                linux::parse(&out.stdout)
            }
            Provider::Windows => {
                let out = exec(handle, &windows::command(), timeout).await?;
                windows::parse(&out.stdout)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn first_sample_has_no_rates_second_does() {
        let mut s = Sampler::default();
        let t0 = Instant::now();
        let raw = |total, idle, rx, tx| RawSnapshot {
            cpu_counters: Some(CpuCounters { total, idle }),
            net_totals: Some((rx, tx)),
            ..Default::default()
        };
        let a = s.sample("x", raw(1000, 800, 10_000, 5_000), t0);
        assert_eq!(a.cpu_percent, None);
        assert_eq!(a.net_rx_bps, None);
        let b = s.sample("x", raw(1100, 850, 12_000, 5_500), t0 + Duration::from_secs(2));
        assert_eq!(b.cpu_percent, Some(50.0));
        assert_eq!(b.net_rx_bps, Some(1000.0));
        assert_eq!(b.net_tx_bps, Some(250.0));
    }

    #[test]
    fn counter_reset_yields_none_not_garbage() {
        let mut s = Sampler::default();
        let t0 = Instant::now();
        let mk = |total, rx| RawSnapshot { cpu_counters: Some(CpuCounters { total, idle: 0 }), net_totals: Some((rx, 0)), ..Default::default() };
        s.sample("x", mk(5000, 9000), t0);
        let b = s.sample("x", mk(100, 10), t0 + Duration::from_secs(1)); // reboot
        assert_eq!(b.cpu_percent, None);
        assert_eq!(b.net_rx_bps, None);
    }

    #[test]
    fn direct_cpu_percent_is_used_as_is() {
        let mut s = Sampler::default();
        let r = RawSnapshot { cpu_percent: Some(37.0), ..Default::default() };
        assert_eq!(s.sample("x", r, Instant::now()).cpu_percent, Some(37.0));
    }
}
