# Licence keys

pdfsqueeze is free software (AGPL-3.0-or-later). A **licence key** is how someone supports the
project and receives the official signed builds, updates and support. This page explains how keys
work and how the publisher issues them. It contains no secrets.

## For users

```bash
pdfsqueeze licence status               # is a licence installed, and for whom
pdfsqueeze licence activate PDFSQ1....  # check a key and keep it for this user
pdfsqueeze licence activate --file key.txt
pdfsqueeze licence remove
```

- Checking a key happens **on your computer**. Nothing is sent anywhere, and there is no account or
  login.
- **Nothing is blocked without a licence.** Every command, the window, the command line and the MCP
  server work the same. A licence identifies you as a supporter.
- Where it is stored: macOS `~/Library/Application Support/pdfsqueeze/licence.txt`, Windows
  `%APPDATA%\pdfsqueeze\licence.txt`, Linux `~/.config/pdfsqueeze/licence.txt`.
- Scripts and CI can supply a key without touching your files: set `PDFSQUEEZE_LICENCE` (the key
  itself) or `PDFSQUEEZE_LICENCE_FILE` (a path).
- `licence` is not exposed to AI agents as a tool: it changes the person's own settings.

## How it works

A key is one line:

```
PDFSQ1.<payload>.<signature>
```

`payload` is base64url JSON: `{"v":1,"id":"order id","email":"..","name":"..","edition":"personal",
"issued":"YYYY-MM-DD","expires":null}`. `signature` is an Ed25519 signature over the payload bytes.
The program contains only the public key (`PUBLIC_KEY_HEX` in `src/licence.rs`), so it can verify a
key but cannot make one. Editing a key, or signing with another key, fails verification. Code:
`src/licence.rs` (about 250 lines, with tests).

Because the code is open, someone can rebuild the program with the check removed. That is allowed
by the AGPL and is not a goal to prevent; see `TRADEMARK.md` for how the official build is
distinguished.

## For the publisher

The private key lives **outside this repository** (currently
`~/.config/pdfsqueeze-publisher/licence-private.key`, mode 600). Back it up somewhere safe and never
commit it. Whoever holds it can issue licences.

```bash
cargo build --release                       # also builds target/release/pdfsqueeze-keytool
KT=target/release/pdfsqueeze-keytool
$KT issue --key ~/.config/pdfsqueeze-publisher/licence-private.key \
          --id LS-12345 --email buyer@example.com --name "Buyer Name" --edition personal
# prints the key; send it to the buyer. Add --expires YYYY-MM-DD for a time-limited licence.
$KT verify PDFSQ1....                       # check a key against this build's public key
```

`pdfsqueeze-keytool` is a development tool and is not part of any download.

### With Lemon Squeezy

1. A sale happens at the store; Lemon Squeezy emails the buyer a receipt.
2. **Now (manual):** issue a key with the command above using the order number as `--id` and the
   buyer's email, and send it. Fine for the first sales.
3. **Later (automatic):** a small webhook function receives the `order_created` event, signs a key
   with the same code and emails it. The private key then lives in that function's secret store.
   Not built yet.

### Rotating the key

Generate a new pair with `keygen`, put the new public key in `PUBLIC_KEY_HEX`, and ship a new
version. Old keys stop verifying in new versions, so re-issue them first or keep accepting both
keys in one release.

### What to remember

- Keep `id` and `email` in your own records: they are the only link between a key and a sale.
- The keys are not tied to a machine; sharing is possible. That is accepted by design (offline, no
  accounts).
- If the private key leaks, anyone can issue licences. Rotate and re-issue.
