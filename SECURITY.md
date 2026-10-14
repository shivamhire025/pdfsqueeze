# Security

pdfsqueeze runs on your machine and never uploads files. What it does touch:

- **Your documents are parsed by Ghostscript, qpdf and Tesseract.** Keep the bundled engines
  current; `pdfsqueeze doctor` warns about an old Ghostscript. Ghostscript runs with `-dSAFER`.
- **The app window talks to a local server** on 127.0.0.1 only, protected by a random per-run
  token and a Host check; it can only hand back result files from its own working folder.
- **The MCP server** can only read and write folders you pass with `--allow-dir`.
- **Installers** check the published SHA-256 before installing. Windows is unsigned and the Mac
  app is ad-hoc signed (no paid certificates), so the first launch needs one confirmation.

## Reporting a problem

Open a private security advisory on the GitHub repository (Security tab, "Report a vulnerability"),
or an issue if it is not sensitive. Please include the command or file that triggers it.

## Last review

2026-10-13: code review of the server, path handling, installers and workflows, local probes of the
server, and a dependency audit. Findings and fixes are listed in `CHANGELOG.md`.
Not yet done: fuzzing with malformed PDFs, a real MCP client test, and running the Windows and
Linux paths on real machines.
