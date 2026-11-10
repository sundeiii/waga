//! russh client handler: performs host key verification during the handshake.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::client;
use russh::keys::{HashAlg, PublicKey, PublicKeyOrCertificate};

use super::events::{PreviousKey, SessionEvent, SessionSink, SessionState};
use super::prompts::PromptRegistry;
use crate::database::activity::{self, ActivityKind};
use crate::database::{host_cas, known_hosts, Database};
use crate::security::host_certs::{self, CertVerdict};
use crate::security::host_keys::{canonical_key, decide, evaluate, HostKeyStatus, PresentedKey, TrustAction};

/// Why the handshake was aborted by us (as opposed to a network error).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostKeyVerdict {
    Rejected { changed: bool },
    Error(String),
}

pub struct ClientHandler {
    pub sid: String,
    pub host: String,
    pub port: u16,
    pub server_id: Option<String>,
    pub server_name: String,
    pub db: Arc<Database>,
    pub prompts: Arc<PromptRegistry>,
    pub sink: Arc<dyn SessionSink>,
    pub verdict: Arc<Mutex<Option<HostKeyVerdict>>>,
    pub prompt_timeout: Duration,
}

pub fn presented_key(pk: &PublicKey) -> Result<PresentedKey, String> {
    let line = pk.to_openssh().map_err(|e| format!("Cannot encode host key: {e}"))?;
    Ok(PresentedKey {
        algorithm: pk.algorithm().as_str().to_string(),
        public_key: canonical_key(&line),
        fingerprint: pk.fingerprint(HashAlg::Sha256).to_string(),
    })
}

impl ClientHandler {
    fn set_verdict(&self, v: HostKeyVerdict) {
        *self.verdict.lock().unwrap() = Some(v);
    }

    fn log(&self, kind: ActivityKind, msg: &str) {
        let _ = self.db.with(|c| activity::record(c, self.server_id.as_deref(), Some(&self.server_name), kind, msg));
    }

    async fn verify(&self, pk: PublicKey) -> bool {
        let presented = match presented_key(&pk) {
            Ok(p) => p,
            Err(e) => {
                self.set_verdict(HostKeyVerdict::Error(e));
                return false;
            }
        };
        let stored = match self.db.with(|c| known_hosts::list_for(c, &self.host, self.port)) {
            Ok(s) => s,
            Err(e) => {
                self.set_verdict(HostKeyVerdict::Error(format!("Cannot read trusted host keys: {e}")));
                return false;
            }
        };
        let status = evaluate(&stored, &presented);
        if status == HostKeyStatus::Trusted {
            return true;
        }
        let endpoint = format!("{}:{}", self.host, self.port);
        if status == HostKeyStatus::Changed {
            let old: Vec<_> = stored.iter().map(|k| k.fingerprint.as_str()).collect();
            self.log(
                ActivityKind::HostKeyChanged,
                &format!("Host key for {endpoint} changed: presented {} ({}), trusted {}", presented.fingerprint, presented.algorithm, old.join(", ")),
            );
        }

        let rx = self.prompts.ask_host_key(&self.sid);
        self.sink.event(SessionEvent::status(SessionState::AwaitingHostKey, None));
        self.sink.event(SessionEvent::HostKeyPrompt {
            host: self.host.clone(),
            port: self.port,
            algorithm: presented.algorithm.clone(),
            fingerprint: presented.fingerprint.clone(),
            status,
            previous: stored
                .iter()
                .map(|k| PreviousKey { algorithm: k.algorithm.clone(), fingerprint: k.fingerprint.clone(), added_at: k.added_at.clone() })
                .collect(),
        });
        let answer = match tokio::time::timeout(self.prompt_timeout, rx).await {
            Ok(Ok(a)) => Some(a),
            _ => None,
        };
        self.prompts.cancel(&self.sid);

        let result = match decide(status, answer) {
            TrustAction::Proceed => Ok(()),
            TrustAction::AddAndProceed => self
                .db
                .with(|c| known_hosts::add(c, &self.host, self.port, &presented.algorithm, &presented.public_key, &presented.fingerprint))
                .map(|_| self.log(ActivityKind::HostKeyTrusted, &format!("Trusted new host key {} for {endpoint}", presented.fingerprint))),
            TrustAction::ReplaceAndProceed => self
                .db
                .with(|c| known_hosts::replace(c, &self.host, self.port, &presented.algorithm, &presented.public_key, &presented.fingerprint))
                .map(|_| self.log(ActivityKind::HostKeyTrusted, &format!("User replaced CHANGED host key for {endpoint} with {}", presented.fingerprint))),
            TrustAction::Reject => {
                let changed = status == HostKeyStatus::Changed;
                self.log(ActivityKind::HostKeyRejected, &format!("Host key {} for {endpoint} was not trusted", presented.fingerprint));
                self.set_verdict(HostKeyVerdict::Rejected { changed });
                return false;
            }
        };
        match result {
            Ok(()) => true,
            Err(e) => {
                self.set_verdict(HostKeyVerdict::Error(format!("Cannot store trusted host key: {e}")));
                false
            }
        }
    }
}

impl ClientHandler {
    /// Host certificate check against trusted CAs. Some(result) when decided,
    /// None to continue with the plain key flow.
    fn verify_certificate(&self, cert: &russh::keys::Certificate) -> Option<bool> {
        let cas = self.db.with(host_cas::list).unwrap_or_default();
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        match host_certs::check(cert, &self.host, &cas, now) {
            CertVerdict::Trusted { ca_fingerprint } => {
                self.log(ActivityKind::HostKeyTrusted, &format!("Host certificate for {}:{} accepted (signed by CA {ca_fingerprint})", self.host, self.port));
                Some(true)
            }
            CertVerdict::Invalid(msg) => {
                self.log(ActivityKind::HostKeyRejected, &msg);
                self.set_verdict(HostKeyVerdict::Error(format!("Connection blocked: {msg}")));
                Some(false)
            }
            CertVerdict::UnknownCa => None,
        }
    }
}

impl client::Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(&mut self, server_public_key: &PublicKeyOrCertificate) -> Result<bool, Self::Error> {
        if let Some(cert) = server_public_key.certificate() {
            if let Some(decided) = self.verify_certificate(cert) {
                return Ok(decided);
            }
        }
        // Plain key (or certificate from an untrusted CA): fingerprint check.
        Ok(self.verify(server_public_key.public_key()).await)
    }
}
