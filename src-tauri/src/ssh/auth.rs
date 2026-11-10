//! Client authentication: password, private key (incl. encrypted keys) and ssh-agent.
//! Secrets are read from the OS credential store or requested from the user
//! via a one-shot prompt; they are never logged or emitted as events.

use std::sync::Arc;

use russh::client::{self, AuthResult, KeyboardInteractiveAuthResponse};
use russh::keys::agent::client::AgentClient;
use russh::MethodKind;
use russh::keys::{self, Algorithm, PrivateKeyWithHashAlg};
use tokio::io::{AsyncRead, AsyncWrite};
use zeroize::Zeroizing;

use super::events::{InteractiveQuestion, SecretKind, SessionEvent, SessionState};
use super::handler::ClientHandler;
use super::prompts::SecretAnswer;
use super::session::{Failure, SessionContext};
use crate::credentials::{passphrase_account, password_account};
use crate::database::servers::AuthMethod;

const MAX_ATTEMPTS: u32 = 3;

type Handle = client::Handle<ClientHandler>;

fn auth_failed(msg: impl Into<String>) -> Failure {
    Failure::new(SessionState::AuthFailed, msg)
}

fn ssh_err(e: russh::Error) -> Failure {
    Failure::error(format!("SSH error during authentication: {e}"))
}

async fn ask_secret(ctx: &SessionContext, kind: SecretKind, prompt: String, attempt: u32) -> Option<SecretAnswer> {
    let rx = ctx.prompts.ask_secret(&ctx.sid);
    ctx.emit(SessionState::AwaitingSecret, None);
    ctx.sink.event(SessionEvent::SecretPrompt { kind, prompt, attempt });
    let answer = tokio::time::timeout(ctx.prompt_timeout, rx).await.ok().and_then(Result::ok);
    ctx.prompts.cancel(&ctx.sid);
    if answer.is_some() {
        ctx.emit(SessionState::Authenticating, None);
    }
    answer
}

pub async fn authenticate(handle: &mut Handle, ctx: &SessionContext) -> Result<(), Failure> {
    match ctx.profile.input.auth_method {
        AuthMethod::Password => password(handle, ctx).await,
        AuthMethod::Key => private_key(handle, ctx).await,
        AuthMethod::Agent => agent(handle, ctx).await,
    }
}

// ---------------------------------------------------------------- keyboard-interactive (2FA)

const MAX_KBD_ROUNDS: u32 = 10;

/// Server text is untrusted: strip control characters and cap length.
fn clean(s: &str, max: usize) -> String {
    s.chars().filter(|c| !c.is_control() || *c == '\n').take(max).collect::<String>().trim().to_string()
}

fn looks_like_password_prompt(p: &str, echo: bool) -> bool {
    let p = p.to_lowercase();
    !echo && p.contains("password") && !p.contains("new") && !p.contains("code") && !p.contains("otp")
}

/// Does this failed result mean "now do keyboard-interactive"?
fn wants_kbd(result: &AuthResult) -> bool {
    matches!(result, AuthResult::Failure { remaining_methods, .. } if remaining_methods.contains(&MethodKind::KeyboardInteractive))
}

/// After a public-key attempt: is a second factor being requested? Either the
/// server says "partial success", or keyboard-interactive is the only way left
/// (some servers don't set the partial flag reliably).
fn needs_second_factor(result: &AuthResult) -> bool {
    match result {
        AuthResult::Success => false,
        AuthResult::Failure { remaining_methods, partial_success } => {
            remaining_methods.contains(&MethodKind::KeyboardInteractive) && (*partial_success || !remaining_methods.contains(&MethodKind::PublicKey))
        }
    }
}

/// Run keyboard-interactive rounds, asking the user for each set of prompts.
/// `prefill_password` answers a single hidden "Password:" prompt once (e.g. the
/// saved password), so only the 2FA code has to be typed. Answers are never stored.
async fn keyboard_interactive(handle: &mut Handle, ctx: &SessionContext, mut prefill_password: Option<Zeroizing<String>>) -> Result<AuthResult, Failure> {
    let user = ctx.profile.input.username.clone();
    let mut resp = handle.authenticate_keyboard_interactive_start(user, None::<String>).await.map_err(ssh_err)?;
    for round in 1..=MAX_KBD_ROUNDS {
        match resp {
            KeyboardInteractiveAuthResponse::Success => return Ok(AuthResult::Success),
            KeyboardInteractiveAuthResponse::Failure { remaining_methods, partial_success } => {
                return Ok(AuthResult::Failure { remaining_methods, partial_success });
            }
            KeyboardInteractiveAuthResponse::InfoRequest { name, instructions, prompts } => {
                let answers: Vec<Zeroizing<String>> = if prompts.is_empty() {
                    Vec::new() // informational round: acknowledge
                } else if prompts.len() == 1 && looks_like_password_prompt(&prompts[0].prompt, prompts[0].echo) && prefill_password.is_some() {
                    vec![prefill_password.take().unwrap()]
                } else {
                    let rx = ctx.prompts.ask_interactive(&ctx.sid);
                    ctx.emit(SessionState::AwaitingSecret, None);
                    ctx.sink.event(SessionEvent::InteractivePrompt {
                        name: clean(&name, 200),
                        instructions: clean(&instructions, 2000),
                        prompts: prompts.iter().map(|p| InteractiveQuestion { prompt: clean(&p.prompt, 300), echo: p.echo }).collect(),
                        round,
                    });
                    let got = tokio::time::timeout(ctx.prompt_timeout, rx).await.ok().and_then(Result::ok);
                    ctx.prompts.cancel(&ctx.sid);
                    match got {
                        Some(a) if a.len() == prompts.len() => {
                            ctx.emit(SessionState::Authenticating, None);
                            a
                        }
                        Some(_) => return Err(Failure::error("Wrong number of answers")),
                        None => return Err(auth_failed("Authentication cancelled")),
                    }
                };
                let plain: Vec<String> = answers.iter().map(|a| a.to_string()).collect();
                resp = handle.authenticate_keyboard_interactive_respond(plain).await.map_err(ssh_err)?;
            }
        }
    }
    Err(auth_failed("Too many verification rounds"))
}

/// After a primary method: continue with keyboard-interactive if the server asks for it.
async fn continue_if_needed(handle: &mut Handle, ctx: &SessionContext, result: AuthResult, prefill: Option<Zeroizing<String>>) -> Result<AuthResult, Failure> {
    if !result.success() && wants_kbd(&result) {
        return keyboard_interactive(handle, ctx, prefill).await;
    }
    Ok(result)
}

async fn password(handle: &mut Handle, ctx: &SessionContext) -> Result<(), Failure> {
    let user = ctx.profile.input.username.clone();
    let account = password_account(&ctx.profile.id);
    let mut stored = ctx.secrets.get(&account).map_err(|e| Failure::error(e.to_string()))?;
    let mut note = String::new();

    for attempt in 1..=MAX_ATTEMPTS {
        let (secret, remember, from_store) = match stored.take() {
            Some(s) => (s, false, true),
            None => {
                let prompt = format!("{note}Password for {}@{}", user, ctx.profile.input.host);
                match ask_secret(ctx, SecretKind::Password, prompt, attempt).await {
                    Some(a) => (a.secret, a.remember, false),
                    None => return Err(auth_failed("Authentication cancelled")),
                }
            }
        };
        let first = handle.authenticate_password(user.clone(), secret.as_str()).await.map_err(ssh_err)?;
        // Servers with "PasswordAuthentication no" often take the password via
        // keyboard-interactive instead, and/or then ask for a 2FA code.
        let password_refused_as_method = matches!(&first, AuthResult::Failure { remaining_methods, partial_success: false } if !remaining_methods.contains(&MethodKind::Password));
        let partial = matches!(&first, AuthResult::Failure { partial_success: true, .. });
        let result = if partial || password_refused_as_method {
            continue_if_needed(handle, ctx, first, Some(secret.clone())).await?
        } else {
            first
        };
        if result.success() {
            if remember && !from_store {
                if let Err(e) = ctx.secrets.set(&account, &secret) {
                    log::warn!("could not save password to credential store: {}", e.kind());
                }
            }
            return Ok(());
        }
        note = if from_store { "The saved password was rejected. ".into() } else { "Incorrect password. ".into() };
    }
    Err(auth_failed("The server rejected the password"))
}

async fn private_key(handle: &mut Handle, ctx: &SessionContext) -> Result<(), Failure> {
    let path = ctx
        .profile
        .input
        .key_path
        .clone()
        .ok_or_else(|| auth_failed("No private key file configured for this server"))?;
    let account = passphrase_account(&ctx.profile.id);
    let mut passphrase: Option<Zeroizing<String>> = ctx.secrets.get(&account).map_err(|e| Failure::error(e.to_string()))?;
    let mut remember = false;
    let mut from_prompt = false;
    let mut attempt = 0;

    let key = loop {
        let pp = passphrase.clone();
        let p = path.clone();
        let loaded = tokio::task::spawn_blocking(move || keys::load_secret_key(&p, pp.as_ref().map(|s| s.as_str())))
            .await
            .map_err(|e| Failure::error(format!("Key loading task failed: {e}")))?;
        match loaded {
            Ok(k) => break k,
            Err(keys::Error::IO(e)) => return Err(auth_failed(format!("Cannot read private key file: {e}"))),
            Err(e) => {
                let needs_passphrase = matches!(e, keys::Error::KeyIsEncrypted);
                if !needs_passphrase && passphrase.is_none() {
                    return Err(auth_failed(format!("Unsupported or corrupt private key: {e}")));
                }
                attempt += 1;
                if attempt > MAX_ATTEMPTS {
                    return Err(auth_failed("Could not decrypt the private key (wrong passphrase?)"));
                }
                let prompt = if passphrase.is_some() {
                    "Incorrect passphrase. Passphrase for private key".to_string()
                } else {
                    "Passphrase for private key".to_string()
                };
                match ask_secret(ctx, SecretKind::Passphrase, prompt, attempt).await {
                    Some(a) => {
                        passphrase = Some(a.secret);
                        remember = a.remember;
                        from_prompt = true;
                    }
                    None => return Err(auth_failed("Authentication cancelled")),
                }
            }
        }
    };

    if from_prompt && remember {
        if let Some(pp) = &passphrase {
            if let Err(e) = ctx.secrets.set(&account, pp) {
                log::warn!("could not save passphrase to credential store: {}", e.kind());
            }
        }
    }
    drop(passphrase);

    let hash = handle.best_supported_rsa_hash().await.map_err(ssh_err)?.flatten();
    let result = handle
        .authenticate_publickey(ctx.profile.input.username.clone(), PrivateKeyWithHashAlg::new(Arc::new(key), hash))
        .await
        .map_err(ssh_err)?;
    // Key accepted but a second factor required (AuthenticationMethods publickey,keyboard-interactive).
    let result = if needs_second_factor(&result) {
        continue_if_needed(handle, ctx, result, None).await?
    } else {
        result
    };
    if result.success() {
        Ok(())
    } else {
        Err(auth_failed("The server rejected the private key. Is the public key in ~/.ssh/authorized_keys?"))
    }
}

async fn agent(handle: &mut Handle, ctx: &SessionContext) -> Result<(), Failure> {
    #[cfg(windows)]
    {
        if let Ok(mut a) = AgentClient::connect_named_pipe(r"\\.\pipe\openssh-ssh-agent").await {
            return agent_auth(handle, ctx, &mut a).await;
        }
        if let Ok(mut a) = AgentClient::connect_pageant().await {
            return agent_auth(handle, ctx, &mut a).await;
        }
        Err(auth_failed(
            "No SSH agent found. Start the 'OpenSSH Authentication Agent' service (ssh-agent) or Pageant and add a key.",
        ))
    }
    #[cfg(not(windows))]
    {
        let mut a = AgentClient::connect_env().await.map_err(|e| auth_failed(format!("No SSH agent available: {e}")))?;
        agent_auth(handle, ctx, &mut a).await
    }
}

async fn agent_auth<S>(handle: &mut Handle, ctx: &SessionContext, agent: &mut AgentClient<S>) -> Result<(), Failure>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let ids = agent
        .request_identities()
        .await
        .map_err(|e| Failure::error(format!("Could not list ssh-agent keys: {e}")))?;
    if ids.is_empty() {
        return Err(auth_failed("The SSH agent has no keys loaded (try `ssh-add`)"));
    }
    let rsa_hash = handle.best_supported_rsa_hash().await.map_err(ssh_err)?.flatten();
    for id in ids {
        let pk = id.public_key().into_owned();
        let hash = if matches!(pk.algorithm(), Algorithm::Rsa { .. }) { rsa_hash } else { None };
        let result = handle
            .authenticate_publickey_with(ctx.profile.input.username.clone(), pk, hash, agent)
            .await
            .map_err(|e| Failure::error(format!("SSH agent signing failed: {e:?}")))?;
        if result.success() {
            return Ok(());
        }
        if needs_second_factor(&result) {
            return match continue_if_needed(handle, ctx, result, None).await? {
                r if r.success() => Ok(()),
                _ => Err(auth_failed("Second authentication factor was rejected")),
            };
        }
    }
    Err(auth_failed("The server rejected all keys offered by the SSH agent"))
}
