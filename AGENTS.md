# pdfsqueeze for AI agents

pdfsqueeze is an offline PDF toolkit. Nothing is uploaded; every operation runs
locally. This guide is for agents (and the people wiring them up) that call it
as a tool.

There are two ways to use it:

1. **MCP server** (recommended): `pdfsqueeze mcp --allow-dir <folder>`
2. **CLI with `--json`**: `pdfsqueeze --json <command> ...`

Both run the same code and return the same JSON.

## Install

macOS and Linux: `curl -fsSL https://raw.githubusercontent.com/shivamhire025/pdfsqueeze/main/install.sh | sh`
Windows (PowerShell): `irm https://raw.githubusercontent.com/shivamhire025/pdfsqueeze/main/install.ps1 | iex`

Everything it needs is bundled; no other software is required. In containers and CI, install
into a known folder with `PDFSQUEEZE_HOME=/opt/pdfsqueeze PDFSQUEEZE_BIN=/usr/local/bin`.
Linux builds need glibc (not Alpine/musl). Paths in tool arguments use the host's native form
(`C:\\Users\\you\\file.pdf` on Windows).

## Connect it over MCP

The server speaks MCP over stdio. It only touches folders you allow.

Claude Code:

```bash
claude mcp add pdfsqueeze -- pdfsqueeze mcp --allow-dir ~/Documents --allow-dir ~/Downloads
```

Claude Desktop (`claude_desktop_config.json`) and most other MCP clients:

```json
{
  "mcpServers": {
    "pdfsqueeze": {
      "command": "/full/path/to/pdfsqueeze",
      "args": ["mcp", "--allow-dir", "/Users/you/Documents"]
    }
  }
}
```

- `--allow-dir` is repeatable. Without it the server refuses to start.
- `--allow-any-path` removes the restriction. Use it only when the agent is fully trusted.
- Paths are resolved (symlinks and `..` included) before the check, so a link inside an
  allowed folder cannot point outside it.

## Discover what is available

```bash
pdfsqueeze tools --json
```

Prints every command with a JSON Schema for its arguments, the exit codes, and the
error codes. Over MCP the same schemas arrive through `tools/list`. They are generated
from the CLI definitions, so they cannot drift from the real behaviour.

## The result contract

Every command, over CLI `--json` or MCP, returns one JSON object:

```json
{
  "ok": true,
  "command": "compress",
  "result": { "...": "command specific" },
  "warnings": [],
  "exit_code": 0
}
```

On failure:

```json
{
  "ok": false,
  "command": "extract",
  "error": { "code": "invalid_argument", "message": "page 99 is out of range (document has 4 pages)" },
  "exit_code": 1
}
```

`ok` is true only when `exit_code` is 0. Branch on `error.code`, not on message text.

### Exit codes

| Code | Meaning |
|---|---|
| 0 | Success |
| 1 | Failure: nothing useful was produced |
| 2 | Invalid command line or arguments |
| 3 | Completed with problems: damaged input, or some items of a batch failed. Read `result`. |

Over MCP, `isError` is true for 1 and 2. A code of 3 is a normal result that you should
read, not a transport failure (for example `check` reporting problems is a successful check).

### Error codes

| Code | Meaning |
|---|---|
| `usage` | Invalid arguments or unknown tool |
| `input_not_found` | A file or folder does not exist or cannot be read |
| `no_input` | A folder contained no PDFs |
| `output_conflict` | The output would overwrite an input or another output |
| `invalid_argument` | A value is out of range or malformed (pages, sizes, angles) |
| `engine_missing` | A required engine is not installed |
| `engine_failed` | An engine ran and reported an error |
| `damaged_input` | The PDF is too damaged to process |
| `password_required` | A password is needed and none was supplied |
| `invalid_password` | The supplied password is wrong |
| `language_unavailable` | The requested OCR language is not installed |
| `path_not_allowed` | The path is outside the folders the MCP server may access |
| `error` | Anything else |

## Behaviour you can rely on

- **Inputs are never modified.** Each command writes a new file and reports its path in
  `result.output` (or `result.outputs`). If you do not pass `output`, a sibling name such
  as `report-compressed.pdf` is used.
- **Existing files are not overwritten by accident.** Output equal to an input is refused
  (`output_conflict`).
- **It never prompts.** Passwords come from `password_file`, or the `PDFSQUEEZE_PASSWORD`
  environment variable. If neither is set you get `password_required` immediately.
- **No network access.** Ever.
- **Batch is partial-success.** `compress` over many files reports each file's status; the
  exit code is 3 if some failed and 1 if all failed.

## Passwords

Pass a file path, not the password itself, so it does not appear in logs or the model's
context:

```json
{ "input": "secret.pdf", "password_file": "/Users/you/.pw" }
```

or set `PDFSQUEEZE_PASSWORD` in the server's environment.

## Recipes

**Make a PDF fit an email limit**

```json
{ "tool": "compress", "arguments": { "inputs": ["report.pdf"], "target_size": "2MB" } }
```

`result.files[0]` has `after_bytes` and `target_met`. If `target_met` is false the file
could not get that small; this is the smallest result.

**Make a scan searchable, then read it**

1. `ocr` with `{ "input": "scan.pdf" }`
2. `text` with `{ "input": "scan-ocr.pdf" }`, then read `result.output`

`ocr` skips pages that already have text. English is bundled; other languages need
`tessdata`.

**Triage an unknown or suspect file**

1. `info` for page count, encryption, and metadata
2. `check` for structural problems (status `ok`, `problems` or `damaged`)
3. `repair` if it is not `ok`. If `result.data_lost` is true the file was truncated and some
   content is gone; re-obtain the original if you can.

**Remove personal metadata before sharing**

`strip_metadata`, then `info` to confirm `info` is empty and `xmp_present` is false.

**Combine and clean up**

`merge` (two or more inputs, in order), then `organize` to reorder, rotate and delete pages in
one pass (`order` lists the pages to keep in their new order; `rotate` is `page:angle` pairs such
as `1:90,3:180` using original page numbers). `delete`, `reorder` and `rotate` do one job each.
Page specs are 1-based, such as `1-3,7,z` where `z` is the last page.

## Limits worth knowing

- OCR needs reasonably upright scans; there is no deskew.
- `watermark` and `number_pages` accept printable ASCII text only.
- `pdfa` declares PDF/A but does not validate it.
- `info` on an encrypted file reports `locked: true` and no page count until it is unlocked.
