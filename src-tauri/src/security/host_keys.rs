//! Host key trust decisions. Pure logic, no I/O, so it is easy to test.
//!
//! Policy (mirrors OpenSSH StrictHostKeyChecking=ask):
//! * presented key matches a trusted key for host:port   -> Trusted
//! * host:port has no trusted keys                         -> Unknown (ask user)
//! * host:port has trusted keys but none match             -> Changed (block; warn;
//!   only an explicit acknowledgement can replace the stored key)

use serde::Serialize;

use crate::database::known_hosts::KnownHost;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresentedKey {
    pub algorithm: String,
    /// "<algorithm> <base64>"
    pub public_key: String,
    /// "SHA256:..."
    pub fingerprint: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum HostKeyStatus {
    Trusted,
    Unknown,
    Changed,
}

/// Strip any trailing comment so "alg base64 comment" compares equal to "alg base64".
pub fn canonical_key(line: &str) -> String {
    let mut parts = line.split_whitespace();
    match (parts.next(), parts.next()) {
        (Some(a), Some(b)) => format!("{a} {b}"),
        _ => line.trim().to_string(),
    }
}

pub fn evaluate(stored: &[KnownHost], presented: &PresentedKey) -> HostKeyStatus {
    let p = canonical_key(&presented.public_key);
    if stored.iter().any(|k| canonical_key(&k.public_key) == p) {
        HostKeyStatus::Trusted
    } else if stored.is_empty() {
        HostKeyStatus::Unknown
    } else {
        HostKeyStatus::Changed
    }
}

/// What the user answered in the host key dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostKeyAnswer {
    pub accept: bool,
    /// Must be true to accept a CHANGED key. The UI only sets it after the
    /// user ticks an explicit "I understand / I verified" confirmation.
    pub acknowledge_changed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustAction {
    /// Continue the connection, no DB change.
    Proceed,
    /// Store as an additional trusted key, then continue.
    AddAndProceed,
    /// Replace all stored keys for host:port, then continue.
    ReplaceAndProceed,
    /// Abort the connection.
    Reject,
}

pub fn decide(status: HostKeyStatus, answer: Option<HostKeyAnswer>) -> TrustAction {
    match (status, answer) {
        (HostKeyStatus::Trusted, _) => TrustAction::Proceed,
        (_, None) => TrustAction::Reject, // no answer / timeout / cancelled
        (HostKeyStatus::Unknown, Some(a)) if a.accept => TrustAction::AddAndProceed,
        (HostKeyStatus::Changed, Some(a)) if a.accept && a.acknowledge_changed => TrustAction::ReplaceAndProceed,
        _ => TrustAction::Reject,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn known(key: &str) -> KnownHost {
        KnownHost {
            host: "h".into(),
            port: 22,
            algorithm: "ssh-ed25519".into(),
            public_key: key.into(),
            fingerprint: "SHA256:x".into(),
            added_at: "t".into(),
        }
    }

    fn presented(key: &str) -> PresentedKey {
        PresentedKey { algorithm: "ssh-ed25519".into(), public_key: key.into(), fingerprint: "SHA256:y".into() }
    }

    #[test]
    fn evaluates_status() {
        assert_eq!(evaluate(&[], &presented("ssh-ed25519 AAA")), HostKeyStatus::Unknown);
        assert_eq!(evaluate(&[known("ssh-ed25519 AAA")], &presented("ssh-ed25519 AAA")), HostKeyStatus::Trusted);
        assert_eq!(evaluate(&[known("ssh-ed25519 AAA comment")], &presented("ssh-ed25519 AAA")), HostKeyStatus::Trusted);
        assert_eq!(evaluate(&[known("ssh-ed25519 AAA")], &presented("ssh-ed25519 BBB")), HostKeyStatus::Changed);
        // Different algorithm for a known host is still a change, not "unknown".
        assert_eq!(evaluate(&[known("ssh-ed25519 AAA")], &presented("ecdsa-sha2-nistp256 CCC")), HostKeyStatus::Changed);
    }

    #[test]
    fn decisions() {
        let yes = Some(HostKeyAnswer { accept: true, acknowledge_changed: false });
        let yes_ack = Some(HostKeyAnswer { accept: true, acknowledge_changed: true });
        let no = Some(HostKeyAnswer { accept: false, acknowledge_changed: true });

        assert_eq!(decide(HostKeyStatus::Trusted, None), TrustAction::Proceed);
        assert_eq!(decide(HostKeyStatus::Unknown, None), TrustAction::Reject);
        assert_eq!(decide(HostKeyStatus::Unknown, yes), TrustAction::AddAndProceed);
        assert_eq!(decide(HostKeyStatus::Unknown, no), TrustAction::Reject);
        // Changed keys are blocked unless explicitly acknowledged.
        assert_eq!(decide(HostKeyStatus::Changed, None), TrustAction::Reject);
        assert_eq!(decide(HostKeyStatus::Changed, yes), TrustAction::Reject);
        assert_eq!(decide(HostKeyStatus::Changed, no), TrustAction::Reject);
        assert_eq!(decide(HostKeyStatus::Changed, yes_ack), TrustAction::ReplaceAndProceed);
    }
}
