//! Pending user prompts (host key confirmation, password/passphrase entry),
//! keyed by session id. The SSH task awaits the receiver; a Tauri command
//! resolves it when the user answers.

use std::collections::HashMap;
use std::sync::Mutex;

use tokio::sync::oneshot;
use zeroize::Zeroizing;

use crate::error::{AppError, AppResult};
use crate::security::host_keys::HostKeyAnswer;

pub struct SecretAnswer {
    pub secret: Zeroizing<String>,
    pub remember: bool,
}

enum Pending {
    HostKey(oneshot::Sender<HostKeyAnswer>),
    Secret(oneshot::Sender<SecretAnswer>),
    Interactive(oneshot::Sender<Vec<Zeroizing<String>>>),
}

#[derive(Default)]
pub struct PromptRegistry {
    pending: Mutex<HashMap<String, Pending>>,
}

impl PromptRegistry {
    pub fn ask_host_key(&self, sid: &str) -> oneshot::Receiver<HostKeyAnswer> {
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(sid.to_string(), Pending::HostKey(tx));
        rx
    }

    pub fn ask_secret(&self, sid: &str) -> oneshot::Receiver<SecretAnswer> {
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(sid.to_string(), Pending::Secret(tx));
        rx
    }

    pub fn answer_host_key(&self, sid: &str, answer: HostKeyAnswer) -> AppResult<()> {
        match self.pending.lock().unwrap().remove(sid) {
            Some(Pending::HostKey(tx)) => {
                let _ = tx.send(answer);
                Ok(())
            }
            Some(other) => {
                // Wrong prompt type: put it back and refuse.
                self.pending.lock().unwrap().insert(sid.to_string(), other);
                Err(AppError::Validation("No host key confirmation is pending for this session".into()))
            }
            None => Err(AppError::Validation("No host key confirmation is pending for this session".into())),
        }
    }

    /// `None` cancels the prompt.
    pub fn answer_secret(&self, sid: &str, answer: Option<SecretAnswer>) -> AppResult<()> {
        let mut map = self.pending.lock().unwrap();
        match map.remove(sid) {
            Some(Pending::Secret(tx)) => {
                if let Some(a) = answer {
                    let _ = tx.send(a);
                }
                Ok(())
            }
            Some(other) => {
                map.insert(sid.to_string(), other);
                Err(AppError::Validation("No credential prompt is pending for this session".into()))
            }
            None => Err(AppError::Validation("No credential prompt is pending for this session".into())),
        }
    }

    pub fn ask_interactive(&self, sid: &str) -> oneshot::Receiver<Vec<Zeroizing<String>>> {
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(sid.to_string(), Pending::Interactive(tx));
        rx
    }

    /// `None` cancels the prompt.
    pub fn answer_interactive(&self, sid: &str, answers: Option<Vec<Zeroizing<String>>>) -> AppResult<()> {
        let mut map = self.pending.lock().unwrap();
        match map.remove(sid) {
            Some(Pending::Interactive(tx)) => {
                if let Some(a) = answers {
                    let _ = tx.send(a);
                }
                Ok(())
            }
            Some(other) => {
                map.insert(sid.to_string(), other);
                Err(AppError::Validation("No verification prompt is pending for this session".into()))
            }
            None => Err(AppError::Validation("No verification prompt is pending for this session".into())),
        }
    }

    pub fn cancel(&self, sid: &str) {
        self.pending.lock().unwrap().remove(sid);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn host_key_answer_roundtrip_and_type_safety() {
        let r = PromptRegistry::default();
        let rx = r.ask_host_key("s1");
        assert!(r.answer_secret("s1", None).is_err(), "wrong prompt type must be refused");
        r.answer_host_key("s1", HostKeyAnswer { accept: true, acknowledge_changed: false }).unwrap();
        assert!(rx.await.unwrap().accept);
        assert!(r.answer_host_key("s1", HostKeyAnswer { accept: true, acknowledge_changed: false }).is_err());
    }

    #[tokio::test]
    async fn cancel_drops_sender() {
        let r = PromptRegistry::default();
        let rx = r.ask_secret("s2");
        r.answer_secret("s2", None).unwrap();
        assert!(rx.await.is_err());
    }
}
