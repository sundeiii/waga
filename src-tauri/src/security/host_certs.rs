//! OpenSSH host certificates signed by a trusted CA (@cert-authority).
//!
//! A certificate is trusted without prompting only if ALL hold:
//! * it is signed by a CA the user added, whose host pattern matches this host;
//! * it is a *host* certificate (not a user certificate);
//! * this host is explicitly listed in its principals (no "any host" certs);
//! * it is within its validity window and the CA signature verifies.
//! A certificate from an unknown CA falls back to the normal key check.

use russh::keys::ssh_key::certificate::CertType;
use russh::keys::ssh_key::{Certificate, Fingerprint, PublicKey};
use russh::keys::HashAlg;

use crate::database::host_cas::{pattern_matches, HostCa};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CertVerdict {
    /// Signed by this trusted CA (fingerprint) and valid for the host.
    Trusted { ca_fingerprint: String },
    /// No trusted CA signed it (or none covers this host): use the plain key flow.
    UnknownCa,
    /// A trusted CA signed it but it is not valid here. Block the connection.
    Invalid(String),
}

pub fn ca_fingerprint(ca_line: &str) -> Result<String, String> {
    let pk = PublicKey::from_openssh(ca_line.trim()).map_err(|e| format!("Not a valid OpenSSH public key: {e}"))?;
    Ok(pk.fingerprint(HashAlg::Sha256).to_string())
}

pub fn check(cert: &Certificate, host: &str, cas: &[HostCa], now_unix: u64) -> CertVerdict {
    let signer = PublicKey::from(cert.signature_key().clone()).fingerprint(HashAlg::Sha256).to_string();
    let candidates: Vec<&HostCa> = cas.iter().filter(|c| c.fingerprint == signer).collect();
    if candidates.is_empty() {
        return CertVerdict::UnknownCa;
    }
    if !candidates.iter().any(|c| pattern_matches(&c.pattern, host)) {
        // Trusted CA, but not for this host: don't let it vouch here.
        return CertVerdict::UnknownCa;
    }
    if cert.cert_type() != CertType::Host {
        return CertVerdict::Invalid("The server presented a user certificate as its host key".into());
    }
    if !cert.valid_principals().iter().any(|p| p.eq_ignore_ascii_case(host)) {
        return CertVerdict::Invalid(format!("The host certificate is not issued for '{host}' (principals: {})", cert.valid_principals().join(", ")));
    }
    let fp: Fingerprint = match signer.parse() {
        Ok(f) => f,
        Err(_) => return CertVerdict::Invalid("Unreadable CA fingerprint".into()),
    };
    match cert.validate_at(now_unix, [&fp]) {
        Ok(()) => CertVerdict::Trusted { ca_fingerprint: signer },
        Err(e) => {
            let when = if now_unix < cert.valid_after() {
                " (not yet valid)"
            } else if now_unix >= cert.valid_before() {
                " (expired)"
            } else {
                ""
            };
            CertVerdict::Invalid(format!("The host certificate failed validation{when}: {e}"))
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use russh::keys::ssh_key::certificate::Builder;
    use russh::keys::ssh_key::private::Ed25519Keypair;
    use russh::keys::PrivateKey;

    pub fn key(seed: u8) -> PrivateKey {
        PrivateKey::from(Ed25519Keypair::from_seed(&[seed; 32]))
    }

    pub fn host_cert(host_key: &PrivateKey, ca: &PrivateKey, principals: &[&str], ty: CertType, after: u64, before: u64) -> Certificate {
        let mut b = Builder::new([7u8; 16], host_key.public_key().key_data().clone(), after, before).unwrap();
        b.cert_type(ty).unwrap().key_id("test").unwrap();
        if principals.is_empty() {
            b.all_principals_valid().unwrap();
        }
        for p in principals {
            b.valid_principal(*p).unwrap();
        }
        b.sign(ca).unwrap()
    }

    fn ca_entry(ca: &PrivateKey, pattern: &str) -> HostCa {
        let line = ca.public_key().to_openssh().unwrap();
        HostCa { id: "x".into(), pattern: pattern.into(), fingerprint: ca_fingerprint(&line).unwrap(), public_key: line, comment: "".into(), added_at: "".into() }
    }

    const NOW: u64 = 1_800_000_000;

    #[test]
    fn verdicts() {
        let (host, ca, other) = (key(1), key(2), key(3));
        let cas = vec![ca_entry(&ca, "*.corp.example")];
        let good = host_cert(&host, &ca, &["web.corp.example"], CertType::Host, NOW - 10, NOW + 10);
        assert!(matches!(check(&good, "web.corp.example", &cas, NOW), CertVerdict::Trusted { .. }));
        assert!(matches!(check(&good, "WEB.corp.example", &cas, NOW), CertVerdict::Trusted { .. }), "case-insensitive");

        // Wrong principal for this host.
        assert!(matches!(check(&good, "db.corp.example", &cas, NOW), CertVerdict::Invalid(_)));
        // CA pattern doesn't cover the host -> normal prompt flow.
        assert_eq!(check(&good, "web.other", &cas, NOW), CertVerdict::UnknownCa);
        // Unknown CA.
        let foreign = host_cert(&host, &other, &["web.corp.example"], CertType::Host, NOW - 10, NOW + 10);
        assert_eq!(check(&foreign, "web.corp.example", &cas, NOW), CertVerdict::UnknownCa);
        // Expired / not yet valid.
        let old = host_cert(&host, &ca, &["web.corp.example"], CertType::Host, NOW - 100, NOW - 50);
        assert!(matches!(check(&old, "web.corp.example", &cas, NOW), CertVerdict::Invalid(m) if m.contains("expired")));
        let future = host_cert(&host, &ca, &["web.corp.example"], CertType::Host, NOW + 50, NOW + 100);
        assert!(matches!(check(&future, "web.corp.example", &cas, NOW), CertVerdict::Invalid(m) if m.contains("not yet valid")));
        // User certificate used as host key.
        let user = host_cert(&host, &ca, &["web.corp.example"], CertType::User, NOW - 10, NOW + 10);
        assert!(matches!(check(&user, "web.corp.example", &cas, NOW), CertVerdict::Invalid(_)));
        // "Any principal" certificates are not accepted for hosts.
        let golden = host_cert(&host, &ca, &[], CertType::Host, NOW - 10, NOW + 10);
        assert!(matches!(check(&golden, "web.corp.example", &cas, NOW), CertVerdict::Invalid(_)));
    }

    #[test]
    fn ca_fingerprint_parsing() {
        assert!(ca_fingerprint(&key(9).public_key().to_openssh().unwrap()).unwrap().starts_with("SHA256:"));
        assert!(ca_fingerprint("not a key").is_err());
    }
}
