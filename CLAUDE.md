# pdfsqueeze: working on this repo

Offline PDF toolkit in Rust. Read `README.md` for usage, `AGENTS.md` for the tool contract
(for agents that *use* pdfsqueeze), and `ROADMAP.md` for what is built and what is verified.
This file is about how to work on the code without causing surprises.

## CI

- `ci.yml` runs on every push to `main` and on pull requests: tests and a release build on
  Linux, macOS and Windows, 32-bit Windows tests, and an end-to-end smoke test with real
  engines on Linux.
- `release.yml` builds and smoke-tests the self-contained packages for macOS (arm64, Intel),
  Linux (x86_64, arm64) and Windows (x64, x86). It runs by hand or on a `v*` tag, and only a
  tag publishes anything.
- Every job must have `timeout-minutes`, and workflows use `concurrency` so a newer run
  cancels the one it supersedes. Never run a GUI or silent installer on a runner (unpack it
  instead): that once hung a job for an hour.
- Runner minutes are free for a public repository. A private fork is metered (macOS counts
  10x, Windows 2x), so check before running the release workflow there.

## Verify locally first, then once remotely

```bash
cargo test                                    # unit tests, no engines needed
cargo build --release
./scripts/smoke.sh target/release/pdfsqueeze  # end-to-end checks (needs gs, qpdf, tesseract)
./scripts/package-macos.sh                    # then smoke-test the package with system engines
                                              # out of reach (see ROADMAP "Phase 3.5")
```

Batch fixes and run the remote workflow once. Platform-specific logic is written to be
unit-testable from any OS (the `Os` parameter in `src/engine.rs`); keep it that way.

## Honest status

Say what was run and what was only written. The ROADMAP tables are the source of truth for
what is verified; update them when something passes or fails.
