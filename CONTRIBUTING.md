# Contributing to pdfsqueeze

Thanks for wanting to help. pdfsqueeze is a small project with a few firm rules: it stays offline,
it stays free software (AGPL-3.0-or-later), and it never sends your files anywhere.

## Before you start

- Open an issue first for anything bigger than a small fix, so we can agree on the approach.
- Security problems: do not open a public issue; see `SECURITY.md`.
- Be kind. Assume good faith.

## Build and test

```bash
cargo test                                       # unit tests, no engines needed
cargo build --release
./scripts/smoke.sh target/release/pdfsqueeze     # needs gs, qpdf 11+, tesseract on PATH
scripts/linux-docker.sh                          # Linux package + smoke test in Docker
```

Please add a test with your change, and say in the pull request what you ran and what you did
**not** run (for example, "not tested on Windows").

## What we will not accept

- Anything that makes an outgoing network connection from the app, adds analytics or telemetry,
  or uploads user files.
- New dependencies with licences that cannot be combined with the AGPL, or that we cannot list in
  `THIRD-PARTY-NOTICES.md` (run `python3 scripts/gen-notices.py` after changing dependencies).

## Legal: your contribution and the DCO

By contributing you confirm that you have the right to submit your work and that it may be
distributed under the project's licence. We use the Developer Certificate of Origin: add a
`Signed-off-by` line to every commit with `git commit -s`.

> By making a contribution to this project, I certify that:
> (a) the contribution was created in whole or in part by me and I have the right to submit it
> under the open source licence indicated in the file; or
> (b) the contribution is based upon previous work that, to the best of my knowledge, is covered
> under an appropriate open source licence and I have the right under that licence to submit that
> work with modifications, whether created in whole or in part by me, under the same open source
> licence (unless I am permitted to submit under a different licence), as indicated in the file; or
> (c) the contribution was provided directly to me by some other person who certified (a), (b) or
> (c) and I have not modified it; and
> (d) I understand and agree that this project and the contribution are public and that a record of
> the contribution (including all personal information I submit with it, including my sign-off) is
> maintained indefinitely and may be redistributed consistent with this project or the open source
> licence(s) involved.
>
> (Developer Certificate of Origin, version 1.1. https://developercertificate.org)

### Relicensing permission

The publisher also offers a commercial licence to companies that cannot use the AGPL. To keep that
possible, the pull-request checklist asks you to confirm:

- [ ] I agree that [PUBLISHER LEGAL NAME] may also distribute my contribution under other
  licences, including a commercial licence, in addition to the AGPL. (Optional: if you do not tick
  this, we may still accept small fixes, but not larger features.)

## Names and logos

The name "pdfsqueeze" and its logo are covered by `TRADEMARK.md`, not by the AGPL.
