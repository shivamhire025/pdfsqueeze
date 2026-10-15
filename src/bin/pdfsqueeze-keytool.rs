//! The publisher's licence tool. NOT part of any download: only the `pdfsqueeze` program is
//! packaged. It makes licence keys; the program only verifies them.
//!
//!   pdfsqueeze-keytool keygen <private-key-file>
//!   pdfsqueeze-keytool issue  --key <private-key-file> --id <order id> --email <email>
//!                             [--name <name>] [--edition personal] [--expires YYYY-MM-DD]
//!   pdfsqueeze-keytool verify <licence key> [--public-key <hex>]
//!
//! Keep the private key file safe and out of git: whoever holds it can issue licences.
#![allow(dead_code)]

#[path = "../licence.rs"]
mod licence;

use ed25519_compact::{KeyPair, Noise, SecretKey, Seed};
use serde_json::json;
use std::process::exit;

fn die(msg: &str) -> ! {
    eprintln!("error: {msg}");
    exit(1)
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap_or_else(|_| die("not valid hex")))
        .collect()
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("keygen") => {
            let path = args.get(1).unwrap_or_else(|| die("usage: keygen <private-key-file>"));
            if std::path::Path::new(path).exists() {
                die("that file already exists; refusing to overwrite a private key");
            }
            let kp = KeyPair::from_seed(Seed::generate());
            std::fs::write(path, format!("{}\n", hex(&kp.sk[..]))).unwrap_or_else(|e| die(&e.to_string()));
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
            }
            println!("private key written to {path} (mode 600). Back it up; never commit it.");
            println!("public key (put in src/licence.rs as PUBLIC_KEY_HEX):");
            println!("{}", hex(&kp.pk[..]));
        }
        Some("issue") => {
            let key_file = flag(&args, "--key").unwrap_or_else(|| die("--key <private-key-file> is required"));
            let id = flag(&args, "--id").unwrap_or_else(|| die("--id <order id> is required"));
            let email = flag(&args, "--email").unwrap_or_else(|| die("--email is required"));
            let sk_hex = std::fs::read_to_string(&key_file).unwrap_or_else(|e| die(&format!("{key_file}: {e}")));
            let sk = SecretKey::from_slice(&unhex(sk_hex.trim())).unwrap_or_else(|_| die("not a valid private key file"));
            let mut payload = json!({
                "v": 1,
                "id": id,
                "email": email,
                "edition": flag(&args, "--edition").unwrap_or_else(|| "personal".into()),
                "issued": licence::today(),
                "expires": flag(&args, "--expires"),
            });
            if let Some(n) = flag(&args, "--name") {
                payload["name"] = json!(n);
            }
            let bytes = serde_json::to_vec(&payload).unwrap();
            let sig = sk.sign(&bytes, Some(Noise::generate()));
            println!("{}", licence::encode(&bytes, sig.as_ref()));
        }
        Some("verify") => {
            let key = args.get(1).unwrap_or_else(|| die("usage: verify <licence key>"));
            let pk = flag(&args, "--public-key").unwrap_or_else(|| licence::PUBLIC_KEY_HEX.to_string());
            println!("{:#?}", licence::verify_with(key, &pk, &licence::today()));
        }
        _ => {
            eprintln!("usage: pdfsqueeze-keytool keygen|issue|verify  (see the header of src/bin/pdfsqueeze-keytool.rs)");
            exit(2)
        }
    }
}
