//! Offline licence keys.
//!
//! A licence is a small JSON document signed with the publisher's Ed25519 key. The program holds
//! only the *public* key, so it can verify a licence but not make one. Verification is entirely
//! local: nothing is sent anywhere, and there is no account or login.
//!
//! Key format (one line, safe to paste or email):
//!
//! ```text
//! PDFSQ1.<payload, base64url>.<signature, base64url>
//! ```
//!
//! The payload is JSON: `{"v":1,"id":"order id","email":"..","name":"..","edition":"personal",
//! "issued":"YYYY-MM-DD","expires":null}`; the signature covers the payload bytes exactly.
//!
//! What a licence does: it identifies the supporter and edition. In this version it gates nothing:
//! every command, the command line, agents and the window work without one. It must stay
//! self-contained (std plus `ed25519-compact` and `serde_json`) because the publisher's key tool
//! includes this file directly.

use ed25519_compact::{PublicKey, Signature};
use serde_json::Value;
use std::path::PathBuf;

/// The publisher's public key (Ed25519, 32 bytes, hex). The matching private key is held only by
/// the publisher and is never in this repository.
pub const PUBLIC_KEY_HEX: &str = "36c11c5b6cd9f2b1b74975ff6e0a40f5f356de0e55260f7a85787e2199dbaf1f";

pub const PREFIX: &str = "PDFSQ1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Licence {
    pub id: String,
    pub email: String,
    pub name: Option<String>,
    pub edition: String,
    pub issued: String,
    pub expires: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// No licence is installed.
    None,
    /// A licence is installed, genuine and current.
    Valid(Licence),
    /// A genuine licence whose end date has passed.
    Expired(Licence),
    /// Something is installed but it is not a genuine licence (the reason is for people).
    Invalid(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Env,
    EnvFile(PathBuf),
    File(PathBuf),
}

// ---- base64url (no padding), enough for this format ---------------------------------------

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

#[allow(dead_code)] // used by the publisher key tool, which includes this file
pub fn b64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len() * 4 / 3 + 3);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[(n >> 6) as usize & 63] as char);
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[n as usize & 63] as char);
        }
    }
    out
}

pub fn b64_decode(s: &str) -> Option<Vec<u8>> {
    let mut vals = Vec::with_capacity(s.len());
    for c in s.bytes() {
        vals.push(ALPHABET.iter().position(|&a| a == c)? as u32);
    }
    if vals.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(vals.len() * 3 / 4);
    for chunk in vals.chunks(4) {
        let mut n = 0u32;
        for (i, v) in chunk.iter().enumerate() {
            n |= v << (18 - 6 * i as u32);
        }
        out.push((n >> 16) as u8);
        if chunk.len() > 2 {
            out.push((n >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(n as u8);
        }
    }
    Some(out)
}

fn from_hex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect()
}

// ---- dates ---------------------------------------------------------------------------------

/// Today's date as `YYYY-MM-DD` (UTC), from the system clock.
pub fn today() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    ymd_from_days((secs / 86_400) as i64)
}

/// Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
pub fn ymd_from_days(z: i64) -> String {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

fn looks_like_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter().enumerate().all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
}

// ---- verify --------------------------------------------------------------------------------

/// Put a payload and its signature into the one-line key format.
#[allow(dead_code)] // used by the publisher key tool
pub fn encode(payload: &[u8], signature: &[u8]) -> String {
    format!("{PREFIX}.{}.{}", b64_encode(payload), b64_encode(signature))
}

/// Verify `key` against `public_key_hex` as of `today` (`YYYY-MM-DD`).
pub fn verify_with(key: &str, public_key_hex: &str, today: &str) -> Status {
    let compact: String = key.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.is_empty() {
        return Status::None;
    }
    let parts: Vec<&str> = compact.split('.').collect();
    if parts.len() != 3 || parts[0] != PREFIX {
        return Status::Invalid("this is not a pdfsqueeze licence key".into());
    }
    let (Some(payload), Some(sig)) = (b64_decode(parts[1]), b64_decode(parts[2])) else {
        return Status::Invalid("the licence key is damaged (it may have been cut short)".into());
    };
    let pk = from_hex(public_key_hex)
        .and_then(|b| PublicKey::from_slice(&b).ok());
    let Some(pk) = pk else {
        return Status::Invalid("this build has no valid publisher key".into());
    };
    let Ok(sig) = Signature::from_slice(&sig) else {
        return Status::Invalid("the licence key is damaged".into());
    };
    if pk.verify(&payload, &sig).is_err() {
        return Status::Invalid("the licence key's signature does not match".into());
    }
    let Ok(v) = serde_json::from_slice::<Value>(&payload) else {
        return Status::Invalid("the licence data is unreadable".into());
    };
    if v["v"].as_i64() != Some(1) {
        return Status::Invalid("this licence is for a newer version of pdfsqueeze".into());
    }
    let s = |k: &str| v[k].as_str().map(str::to_string);
    let (Some(id), Some(email), Some(edition), Some(issued)) =
        (s("id"), s("email"), s("edition"), s("issued"))
    else {
        return Status::Invalid("the licence is missing required fields".into());
    };
    let expires = s("expires");
    if !looks_like_date(&issued) || expires.as_deref().is_some_and(|e| !looks_like_date(e)) {
        return Status::Invalid("the licence has a malformed date".into());
    }
    let lic = Licence { id, email, name: s("name"), edition, issued, expires };
    match &lic.expires {
        Some(e) if e.as_str() < today => Status::Expired(lic),
        _ => Status::Valid(lic),
    }
}

/// Verify `key` against this build's public key, as of today.
pub fn verify(key: &str) -> Status {
    verify_with(key, PUBLIC_KEY_HEX, &today())
}

// ---- where a licence lives -----------------------------------------------------------------

/// The folder holding the installed licence for this user.
pub fn config_dir() -> Option<PathBuf> {
    let home = || std::env::var_os("HOME").map(PathBuf::from);
    if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("pdfsqueeze"))
    } else if cfg!(target_os = "macos") {
        home().map(|h| h.join("Library").join("Application Support").join("pdfsqueeze"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| home().map(|h| h.join(".config")))
            .map(|c| c.join("pdfsqueeze"))
    }
}

pub fn licence_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("licence.txt"))
}

/// Find an installed licence key: `PDFSQUEEZE_LICENCE` (the key itself), then
/// `PDFSQUEEZE_LICENCE_FILE`, then the per-user file. The environment variables let scripts and
/// agents supply a licence without touching the user's files.
pub fn find() -> Option<(String, Source)> {
    if let Ok(k) = std::env::var("PDFSQUEEZE_LICENCE") {
        if !k.trim().is_empty() {
            return Some((k, Source::Env));
        }
    }
    if let Some(f) = std::env::var_os("PDFSQUEEZE_LICENCE_FILE").map(PathBuf::from) {
        if let Ok(k) = std::fs::read_to_string(&f) {
            return Some((k, Source::EnvFile(f)));
        }
    }
    let f = licence_path()?;
    std::fs::read_to_string(&f).ok().map(|k| (k, Source::File(f)))
}

pub fn status() -> (Status, Option<Source>) {
    match find() {
        None => (Status::None, None),
        Some((key, src)) => (verify(&key), Some(src)),
    }
}

/// Check a key and, if it is genuine and current, store it for this user.
pub fn install(key: &str) -> Result<(Licence, PathBuf), String> {
    match verify(key) {
        Status::Valid(l) => {
            let path = licence_path().ok_or("could not find a place to store the licence")?;
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
            }
            let compact: String = key.chars().filter(|c| !c.is_whitespace()).collect();
            std::fs::write(&path, format!("{compact}\n"))
                .map_err(|e| format!("could not write {}: {e}", path.display()))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
            }
            Ok((l, path))
        }
        Status::Expired(l) => Err(format!(
            "this licence expired on {}",
            l.expires.unwrap_or_default()
        )),
        Status::Invalid(why) => Err(why),
        Status::None => Err("no licence key was given".into()),
    }
}

/// Delete the per-user licence file. Returns whether there was one.
pub fn remove() -> Result<bool, String> {
    let Some(path) = licence_path() else { return Ok(false) };
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(format!("could not remove {}: {e}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_compact::{KeyPair, Seed};

    fn pair() -> (KeyPair, String) {
        let kp = KeyPair::from_seed(Seed::new([7u8; 32]));
        let hex: String = kp.pk.iter().map(|b| format!("{b:02x}")).collect();
        (kp, hex)
    }

    fn make(kp: &KeyPair, json: &str) -> String {
        let sig = kp.sk.sign(json.as_bytes(), None);
        encode(json.as_bytes(), sig.as_ref())
    }

    const OK: &str = r#"{"v":1,"id":"LS-1001","email":"a@example.com","name":"Ada","edition":"personal","issued":"2026-10-15","expires":null}"#;

    #[test]
    fn base64_round_trips() {
        for n in 0..40usize {
            let data: Vec<u8> = (0..n).map(|i| (i * 37 + 11) as u8).collect();
            assert_eq!(b64_decode(&b64_encode(&data)).unwrap(), data, "length {n}");
        }
        assert!(b64_decode("ab$d").is_none());
        assert!(b64_decode("a").is_none());
    }

    #[test]
    fn dates_are_computed_correctly() {
        assert_eq!(ymd_from_days(0), "1970-01-01");
        assert_eq!(ymd_from_days(19_723), "2024-01-01");
        assert_eq!(ymd_from_days(20_582), "2026-05-09");
        assert_eq!(ymd_from_days(11_016), "2000-02-29");
    }

    #[test]
    fn a_genuine_licence_verifies() {
        let (kp, hex) = pair();
        let key = make(&kp, OK);
        match verify_with(&key, &hex, "2026-10-16") {
            Status::Valid(l) => {
                assert_eq!(l.id, "LS-1001");
                assert_eq!(l.email, "a@example.com");
                assert_eq!(l.name.as_deref(), Some("Ada"));
                assert_eq!(l.edition, "personal");
                assert_eq!(l.expires, None);
            }
            other => panic!("expected valid, got {other:?}"),
        }
    }

    #[test]
    fn pasted_with_line_breaks_still_verifies() {
        let (kp, hex) = pair();
        let key = make(&kp, OK);
        let wrapped = format!("  {}\n{}\n", &key[..40], &key[40..]);
        assert!(matches!(verify_with(&wrapped, &hex, "2026-10-16"), Status::Valid(_)));
    }

    #[test]
    fn a_changed_payload_is_rejected() {
        let (kp, hex) = pair();
        let key = make(&kp, OK);
        let forged = make(&kp, &OK.replace("personal", "company"));
        // keep the genuine signature, swap in the forged payload
        let sig = key.rsplit('.').next().unwrap();
        let forged_payload = forged.split('.').nth(1).unwrap();
        let tampered = format!("{PREFIX}.{forged_payload}.{sig}");
        assert!(matches!(verify_with(&tampered, &hex, "2026-10-16"), Status::Invalid(_)));
    }

    #[test]
    fn another_publishers_key_is_rejected() {
        let (kp, _) = pair();
        let other = KeyPair::from_seed(Seed::new([9u8; 32]));
        let other_hex: String = other.pk.iter().map(|b| format!("{b:02x}")).collect();
        let key = make(&kp, OK);
        assert!(matches!(verify_with(&key, &other_hex, "2026-10-16"), Status::Invalid(_)));
    }

    #[test]
    fn expiry_is_honoured() {
        let (kp, hex) = pair();
        let key = make(&kp, &OK.replace("\"expires\":null", "\"expires\":\"2026-12-31\""));
        assert!(matches!(verify_with(&key, &hex, "2026-12-31"), Status::Valid(_)));
        assert!(matches!(verify_with(&key, &hex, "2027-01-01"), Status::Expired(_)));
    }

    #[test]
    fn junk_is_reported_not_trusted() {
        let (_, hex) = pair();
        assert_eq!(verify_with("", &hex, "2026-10-16"), Status::None);
        assert_eq!(verify_with("   \n", &hex, "2026-10-16"), Status::None);
        for junk in ["hello", "PDFSQ1.abc", "PDFSQ2.a.b", "PDFSQ1.%%%.@@@", "PDFSQ1..", "a.b.c"] {
            assert!(matches!(verify_with(junk, &hex, "2026-10-16"), Status::Invalid(_)), "{junk}");
        }
    }

    #[test]
    fn missing_fields_and_bad_dates_are_rejected() {
        let (kp, hex) = pair();
        let no_email = make(&kp, r#"{"v":1,"id":"x","edition":"personal","issued":"2026-10-15"}"#);
        assert!(matches!(verify_with(&no_email, &hex, "2026-10-16"), Status::Invalid(_)));
        let bad_date = make(&kp, &OK.replace("2026-10-15", "yesterday"));
        assert!(matches!(verify_with(&bad_date, &hex, "2026-10-16"), Status::Invalid(_)));
        let v2 = make(&kp, &OK.replace("\"v\":1", "\"v\":2"));
        assert!(matches!(verify_with(&v2, &hex, "2026-10-16"), Status::Invalid(_)));
    }

    #[test]
    fn a_build_without_a_real_publisher_key_accepts_nothing() {
        let (kp, _) = pair();
        let key = make(&kp, OK);
        assert!(matches!(verify_with(&key, "PUBLIC_KEY_PLACEHOLDER", "2026-10-16"), Status::Invalid(_)));
    }
}
