//! Passphrase encryption for backups and sync files.
//!
//! Layout (all integers little-endian):
//!   "YORU" | format u16 | m_cost u32 | t_cost u32 | p_cost u32 | salt[16] | nonce[24] | ciphertext
//! Key = Argon2id(passphrase, salt, m/t/p) -> 32 bytes; cipher = XChaCha20-Poly1305
//! with the whole header as associated data, so any tampering (or a wrong
//! passphrase) fails authentication instead of producing garbage.

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, Payload};
use chacha20poly1305::{KeyInit, XChaCha20Poly1305, XNonce};
use zeroize::Zeroizing;

const MAGIC: &[u8; 4] = b"YORU";
const FORMAT: u16 = 1;
const HEADER_LEN: usize = 4 + 2 + 4 * 3 + 16 + 24;
pub const MIN_PASSPHRASE: usize = 10;

#[derive(Debug, Clone, Copy)]
pub struct KdfCost {
    pub m_cost_kib: u32,
    pub t_cost: u32,
    pub p_cost: u32,
}

/// ~64 MiB, 3 passes: a fraction of a second here, very expensive to brute-force.
pub const DEFAULT_COST: KdfCost = KdfCost { m_cost_kib: 64 * 1024, t_cost: 3, p_cost: 1 };

fn derive(pass: &str, salt: &[u8], c: KdfCost) -> Result<Zeroizing<[u8; 32]>, String> {
    let params = Params::new(c.m_cost_kib, c.t_cost, c.p_cost, Some(32)).map_err(|e| format!("Bad KDF parameters: {e}"))?;
    let mut key = Zeroizing::new([0u8; 32]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(pass.as_bytes(), salt, key.as_mut())
        .map_err(|e| format!("Key derivation failed: {e}"))?;
    Ok(key)
}

pub fn check_passphrase(pass: &str) -> Result<(), String> {
    if pass.chars().count() < MIN_PASSPHRASE {
        return Err(format!("Use a passphrase of at least {MIN_PASSPHRASE} characters"));
    }
    Ok(())
}

pub fn seal_with(pass: &str, plaintext: &[u8], cost: KdfCost) -> Result<Vec<u8>, String> {
    let mut salt = [0u8; 16];
    let mut nonce = [0u8; 24];
    getrandom::fill(&mut salt).map_err(|e| e.to_string())?;
    getrandom::fill(&mut nonce).map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(HEADER_LEN + plaintext.len() + 16);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&FORMAT.to_le_bytes());
    out.extend_from_slice(&cost.m_cost_kib.to_le_bytes());
    out.extend_from_slice(&cost.t_cost.to_le_bytes());
    out.extend_from_slice(&cost.p_cost.to_le_bytes());
    out.extend_from_slice(&salt);
    out.extend_from_slice(&nonce);
    let key = derive(pass, &salt, cost)?;
    let cipher = XChaCha20Poly1305::new_from_slice(key.as_ref()).map_err(|e| e.to_string())?;
    let ct = cipher
        .encrypt(&XNonce::from(nonce), Payload { msg: plaintext, aad: &out[..HEADER_LEN] })
        .map_err(|_| "Encryption failed".to_string())?;
    out.extend_from_slice(&ct);
    Ok(out)
}

pub fn seal(pass: &str, plaintext: &[u8]) -> Result<Vec<u8>, String> {
    seal_with(pass, plaintext, DEFAULT_COST)
}

pub fn open(pass: &str, data: &[u8]) -> Result<Zeroizing<Vec<u8>>, String> {
    if data.len() < HEADER_LEN + 16 || &data[..4] != MAGIC {
        return Err("Not a yoru backup/sync file".into());
    }
    let u32_at = |i: usize| u32::from_le_bytes(data[i..i + 4].try_into().unwrap());
    let format = u16::from_le_bytes([data[4], data[5]]);
    if format != FORMAT {
        return Err(format!("Unsupported file format version {format}; update yoru"));
    }
    let cost = KdfCost { m_cost_kib: u32_at(6), t_cost: u32_at(10), p_cost: u32_at(14) };
    // Refuse absurd parameters from a crafted file (memory/time DoS).
    if cost.m_cost_kib > 1024 * 1024 || cost.t_cost > 20 || cost.p_cost > 16 {
        return Err("File uses unsupported key-derivation settings".into());
    }
    let salt = &data[18..34];
    let nonce: [u8; 24] = data[34..58].try_into().unwrap();
    let key = derive(pass, salt, cost)?;
    let cipher = XChaCha20Poly1305::new_from_slice(key.as_ref()).map_err(|e| e.to_string())?;
    cipher
        .decrypt(&XNonce::from(nonce), Payload { msg: &data[HEADER_LEN..], aad: &data[..HEADER_LEN] })
        .map(Zeroizing::new)
        .map_err(|_| "Wrong passphrase, or the file was modified".to_string())
}

#[cfg(test)]
pub(crate) const TEST_COST: KdfCost = KdfCost { m_cost_kib: 256, t_cost: 1, p_cost: 1 };

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_wrong_pass_and_tamper() {
        let data = seal_with("correct horse battery", b"secret servers", TEST_COST).unwrap();
        assert_eq!(&open("correct horse battery", &data).unwrap()[..], b"secret servers");
        assert!(open("wrong passphrase!!", &data).is_err());
        for i in [3usize, 8, 40, HEADER_LEN + 2, data.len() - 1] {
            let mut t = data.clone();
            t[i] ^= 1;
            assert!(open("correct horse battery", &t).is_err(), "byte {i} tamper detected");
        }
        assert!(open("x", b"not a file").is_err());
    }

    #[test]
    fn fresh_salt_and_nonce_each_time() {
        let a = seal_with("passphrase-123", b"x", TEST_COST).unwrap();
        let b = seal_with("passphrase-123", b"x", TEST_COST).unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn rejects_dos_parameters() {
        let mut d = seal_with("passphrase-123", b"x", TEST_COST).unwrap();
        d[6..10].copy_from_slice(&(u32::MAX).to_le_bytes());
        assert!(open("passphrase-123", &d).unwrap_err().contains("unsupported"));
    }

    #[test]
    fn passphrase_policy() {
        assert!(check_passphrase("short").is_err());
        assert!(check_passphrase("long enough pass").is_ok());
    }
}
