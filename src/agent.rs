//! Everything for AI agents: machine-readable tool descriptions and an MCP
//! (Model Context Protocol) server over stdio.
//!
//! Tool schemas are generated from the clap definitions in `cli.rs`, so a new
//! command or option shows up for agents automatically and cannot drift from
//! the real CLI. A tool call is turned back into argv and parsed by the same
//! clap definitions, then executed by the same code as the command line.

use crate::cli::Cli;
use crate::commands::{
    command_name, envelope, execute, fail, Ctx, Done, ERROR_CODES, EXIT_ERROR, EXIT_OK,
    EXIT_PARTIAL, EXIT_USAGE,
};
use anyhow::Result;
use clap::{ArgAction, CommandFactory, Parser};
use serde_json::{json, Map, Value};
use std::io::{BufRead, Write};
use std::path::{Component, Path, PathBuf};

/// Commands that are not exposed as tools.
const HIDDEN: &[&str] = &["help", "tools", "mcp"];

/// Arguments that hold file-system paths (checked against the allow-list).
const PATH_ARGS: &[&str] = &[
    "input",
    "inputs",
    "output",
    "out_dir",
    "tessdata",
    "password_file",
];
const INTEGER_ARGS: &[&str] = &[
    "every", "first", "last", "jobs", "start", "part", "quality",
];
/// Numeric but may be fractional in at least one command.
const NUMBER_ARGS: &[&str] = &["dpi", "opacity", "size", "angle"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Bool,
    Integer,
    Number,
    Text,
    Path,
}

#[derive(Debug, Clone)]
pub struct ArgSpec {
    id: String,
    long: Option<String>,
    positional: bool,
    kind: Kind,
    multiple: bool,
    required: bool,
    help: String,
    choices: Vec<String>,
    default: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ToolSpec {
    /// MCP tool name: the command name with `-` replaced by `_`.
    pub name: String,
    pub cli_name: String,
    pub description: String,
    args: Vec<ArgSpec>,
}

fn kind_of(id: &str, is_flag: bool) -> Kind {
    if is_flag {
        Kind::Bool
    } else if PATH_ARGS.contains(&id) {
        Kind::Path
    } else if INTEGER_ARGS.contains(&id) {
        Kind::Integer
    } else if NUMBER_ARGS.contains(&id) {
        Kind::Number
    } else {
        Kind::Text
    }
}

pub fn tool_specs() -> Vec<ToolSpec> {
    let mut root = Cli::command();
    root.build();
    let mut out = Vec::new();
    for sub in root.get_subcommands() {
        let cli_name = sub.get_name().to_string();
        if HIDDEN.contains(&cli_name.as_str()) {
            continue;
        }
        let mut args = Vec::new();
        let mut push = |arg: &clap::Arg, positional: bool| {
            let id = arg.get_id().as_str().to_string();
            if matches!(id.as_str(), "help" | "version" | "json") {
                return;
            }
            let is_flag = matches!(arg.get_action(), ArgAction::SetTrue | ArgAction::SetFalse);
            let multiple = matches!(arg.get_action(), ArgAction::Append)
                || arg
                    .get_num_args()
                    .map(|r| r.max_values() > 1)
                    .unwrap_or(false);
            args.push(ArgSpec {
                long: arg.get_long().map(String::from),
                positional,
                kind: kind_of(&id, is_flag),
                multiple,
                required: arg.is_required_set(),
                help: arg.get_help().map(|h| h.to_string()).unwrap_or_default(),
                choices: arg
                    .get_possible_values()
                    .iter()
                    .filter(|v| !v.is_hide_set())
                    .map(|v| v.get_name().to_string())
                    .collect(),
                default: arg
                    .get_default_values()
                    .first()
                    .map(|v| v.to_string_lossy().into_owned()),
                id,
            });
        };
        for a in sub.get_positionals() {
            push(a, true);
        }
        for a in sub.get_arguments().filter(|a| !a.is_positional()) {
            push(a, false);
        }
        out.push(ToolSpec {
            name: cli_name.replace('-', "_"),
            description: sub.get_about().map(|s| s.to_string()).unwrap_or_default(),
            cli_name,
            args,
        });
    }
    out
}

impl ToolSpec {
    pub fn input_schema(&self) -> Value {
        let mut props = Map::new();
        let mut required: Vec<Value> = Vec::new();
        for a in &self.args {
            let base = match a.kind {
                Kind::Bool => json!({ "type": "boolean" }),
                Kind::Integer => json!({ "type": "integer" }),
                Kind::Number => json!({ "type": "number" }),
                Kind::Text | Kind::Path => json!({ "type": "string" }),
            };
            let mut base = base;
            if !a.choices.is_empty() {
                base["enum"] = json!(a.choices);
            }
            let mut prop = if a.multiple {
                json!({ "type": "array", "items": base, "minItems": 1 })
            } else {
                base
            };
            let mut desc = a.help.clone();
            if a.kind == Kind::Path {
                if !desc.is_empty() {
                    desc.push(' ');
                }
                desc.push_str("(file-system path)");
            }
            if let Some(d) = &a.default {
                desc.push_str(&format!(" Default: {d}."));
            }
            if !desc.trim().is_empty() {
                prop["description"] = json!(desc.trim());
            }
            props.insert(a.id.clone(), prop);
            if a.required {
                required.push(json!(a.id));
            }
        }
        json!({
            "type": "object",
            "properties": props,
            "required": required,
            "additionalProperties": false,
        })
    }

    fn to_json(&self) -> Value {
        json!({
            "name": self.name,
            "cli": self.cli_name,
            "description": self.description,
            "input_schema": self.input_schema(),
        })
    }
}

/// `pdfsqueeze tools`: the full machine-readable description.
pub fn describe() -> Done {
    let specs = tool_specs();
    let mut human = String::from("Commands (use --json for full schemas, or `pdfsqueeze mcp` to serve them as MCP tools):\n");
    for s in &specs {
        human.push_str(&format!("  {:<16} {}\n", s.cli_name, s.description));
    }
    let data = json!({
        "name": "pdfsqueeze",
        "version": env!("CARGO_PKG_VERSION"),
        "exit_codes": {
            EXIT_OK.to_string(): "success",
            EXIT_ERROR.to_string(): "failure: nothing useful was produced",
            EXIT_USAGE.to_string(): "invalid command line or arguments",
            EXIT_PARTIAL.to_string(): "completed with problems (damaged input, some batch items failed); see the result",
        },
        "error_codes": ERROR_CODES.iter().map(|(c, d)| json!({"code": c, "meaning": d})).collect::<Vec<_>>(),
        "tools": specs.iter().map(ToolSpec::to_json).collect::<Vec<_>>(),
    });
    Done {
        human: human.trim_end().to_string(),
        data,
        warnings: vec![],
        exit_code: EXIT_OK,
    }
}

fn scalar_to_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Number(n) => {
            // 90.0 must reach clap as `90` for integer options.
            if let Some(f) = n.as_f64() {
                if f.fract() == 0.0 && f.abs() < 1e15 {
                    return Some(format!("{}", f as i64));
                }
            }
            Some(n.to_string())
        }
        _ => None,
    }
}

fn values_of(v: &Value) -> Result<Vec<String>> {
    match v {
        Value::Array(items) => items
            .iter()
            .map(|i| {
                scalar_to_string(i).ok_or_else(|| fail("usage", "array items must be strings or numbers"))
            })
            .collect(),
        other => scalar_to_string(other)
            .map(|s| vec![s])
            .ok_or_else(|| fail("usage", "arguments must be strings, numbers, booleans or arrays")),
    }
}

/// Turn a tool call's JSON arguments into the argv the CLI would have received.
pub fn argv_for(spec: &ToolSpec, args: &Map<String, Value>) -> Result<Vec<String>> {
    for k in args.keys() {
        if !spec.args.iter().any(|a| &a.id == k) {
            return Err(fail("usage", format!("unknown argument `{k}` for {}", spec.name)));
        }
    }
    let mut argv: Vec<String> = vec!["pdfsqueeze".into(), "--json".into(), spec.cli_name.clone()];

    for a in spec.args.iter().filter(|a| !a.positional) {
        let Some(v) = args.get(&a.id).filter(|v| !v.is_null()) else {
            continue;
        };
        let long = format!("--{}", a.long.as_deref().unwrap_or(&a.id));
        if a.kind == Kind::Bool {
            match v.as_bool() {
                Some(true) => argv.push(long),
                Some(false) => {}
                None => return Err(fail("usage", format!("`{}` must be true or false", a.id))),
            }
            continue;
        }
        let vals = values_of(v)?;
        if !a.multiple && vals.len() != 1 {
            return Err(fail("usage", format!("`{}` takes a single value", a.id)));
        }
        for val in vals {
            argv.push(long.clone());
            argv.push(val);
        }
    }

    let mut tail: Vec<String> = Vec::new();
    for a in spec.args.iter().filter(|a| a.positional) {
        match args.get(&a.id).filter(|v| !v.is_null()) {
            Some(v) => tail.extend(values_of(v)?),
            None if a.required => {
                return Err(fail("usage", format!("missing required argument `{}`", a.id)))
            }
            None => {}
        }
    }
    if !tail.is_empty() {
        argv.push("--".into());
        argv.extend(tail);
    }
    Ok(argv)
}

/// Folders an MCP session may touch.
pub struct PathPolicy {
    roots: Vec<PathBuf>,
    allow_any: bool,
}

impl PathPolicy {
    pub fn new(dirs: &[PathBuf], allow_any: bool) -> Result<Self> {
        let mut roots = Vec::new();
        for d in dirs {
            roots.push(
                d.canonicalize()
                    .map_err(|e| anyhow::anyhow!("--allow-dir {}: {e}", d.display()))?,
            );
        }
        Ok(PathPolicy { roots, allow_any })
    }

    pub fn check(&self, raw: &str) -> Result<()> {
        if self.allow_any {
            return Ok(());
        }
        let abs = if Path::new(raw).is_absolute() {
            PathBuf::from(raw)
        } else {
            std::env::current_dir()?.join(raw)
        };
        // Canonicalize the deepest part that exists, then re-attach the rest.
        // This resolves symlinks, so a link inside an allowed folder cannot
        // point outside it.
        let mut existing = abs.as_path();
        let mut tail: Vec<std::ffi::OsString> = Vec::new();
        let base = loop {
            if let Ok(c) = existing.canonicalize() {
                break c;
            }
            match (existing.file_name(), existing.parent()) {
                (Some(n), Some(parent)) => {
                    tail.push(n.to_os_string());
                    existing = parent;
                }
                // reached the top without finding anything that exists
                _ => break existing.ancestors().last().map(PathBuf::from).unwrap_or_default(),
            }
        };
        let mut full = base;
        for part in tail.iter().rev() {
            full.push(part);
        }
        if full.components().any(|c| c == Component::ParentDir) {
            return Err(fail("path_not_allowed", format!("`{raw}` is not allowed")));
        }
        if self.roots.iter().any(|r| full.starts_with(r)) {
            Ok(())
        } else {
            Err(fail(
                "path_not_allowed",
                format!(
                    "`{raw}` is outside the folders this server may access ({})",
                    self.roots
                        .iter()
                        .map(|r| r.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            ))
        }
    }

    fn check_args(&self, spec: &ToolSpec, args: &Map<String, Value>) -> Result<()> {
        for a in spec.args.iter().filter(|a| a.kind == Kind::Path) {
            if let Some(v) = args.get(&a.id).filter(|v| !v.is_null()) {
                for s in values_of(v)? {
                    self.check(&s)?;
                }
            }
        }
        Ok(())
    }
}

/// Run one tool call and return its JSON envelope plus whether it is an error.
pub fn call_tool(
    specs: &[ToolSpec],
    policy: &PathPolicy,
    name: &str,
    args: &Map<String, Value>,
) -> (Value, bool) {
    let result: Result<(String, Done)> = (|| {
        let spec = specs
            .iter()
            .find(|s| s.name == name)
            .ok_or_else(|| fail("usage", format!("unknown tool `{name}`")))?;
        policy.check_args(spec, args)?;
        let argv = argv_for(spec, args)?;
        let cli = Cli::try_parse_from(&argv)
            .map_err(|e| fail("usage", e.to_string().trim().to_string()))?;
        let cmd_name = command_name(&cli.cmd).to_string();
        let ctx = Ctx {
            json: true,
            interactive: false,
        };
        let done = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| execute(cli.cmd, &ctx)))
            .map_err(|_| anyhow::anyhow!("internal error: the command crashed"))??;
        Ok((cmd_name, done))
    })();

    match result {
        Ok((cmd, done)) => {
            let (env, code) = envelope(&cmd, Ok(done));
            (env, code == EXIT_ERROR || code == EXIT_USAGE)
        }
        Err(e) => {
            let (env, _) = envelope(name, Err(e));
            (env, true)
        }
    }
}

fn reply(id: &Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn reply_err(id: &Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

const INSTRUCTIONS: &str = "Offline PDF toolkit: compress, merge, split, convert, OCR, repair, \
protect and more. Files never leave this machine. Every tool returns one JSON object \
({ok, result|error, warnings, exit_code}). Inputs are never modified; each tool writes a new \
file and reports its path. Run `info` or `check` first when a file's state is unknown.";

/// Handle one JSON-RPC message; returns the response, if one is due.
pub fn handle_message(specs: &[ToolSpec], policy: &PathPolicy, msg: &Value) -> Option<Value> {
    let id = msg.get("id").cloned();
    let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
    // Notifications carry no id and get no reply.
    let id = match id {
        Some(i) => i,
        None => return None,
    };
    let params = msg.get("params").cloned().unwrap_or(Value::Null);

    Some(match method {
        "initialize" => {
            let ver = params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or("2025-06-18");
            reply(
                &id,
                json!({
                    "protocolVersion": ver,
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": { "name": "pdfsqueeze", "version": env!("CARGO_PKG_VERSION") },
                    "instructions": INSTRUCTIONS,
                }),
            )
        }
        "ping" => reply(&id, json!({})),
        "tools/list" => reply(
            &id,
            json!({
                "tools": specs.iter().map(|s| json!({
                    "name": s.name,
                    "description": s.description,
                    "inputSchema": s.input_schema(),
                })).collect::<Vec<_>>()
            }),
        ),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let empty = Map::new();
            let args = params
                .get("arguments")
                .and_then(Value::as_object)
                .unwrap_or(&empty);
            let (env, is_err) = call_tool(specs, policy, name, args);
            reply(
                &id,
                json!({
                    "content": [{ "type": "text", "text": serde_json::to_string_pretty(&env).unwrap_or_default() }],
                    "isError": is_err,
                }),
            )
        }
        other => reply_err(&id, -32601, &format!("method not found: {other}")),
    })
}

/// `pdfsqueeze mcp`: newline-delimited JSON-RPC over stdin/stdout.
/// stdout carries protocol messages only; diagnostics go to stderr.
pub fn serve(policy: PathPolicy) -> Result<()> {
    let specs = tool_specs();
    let stdin = std::io::stdin();
    let mut out = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let resp = match serde_json::from_str::<Value>(&line) {
            Ok(msg) => handle_message(&specs, &policy, &msg),
            Err(e) => Some(reply_err(&Value::Null, -32700, &format!("parse error: {e}"))),
        };
        if let Some(r) = resp {
            writeln!(out, "{}", serde_json::to_string(&r)?)?;
            out.flush()?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(name: &str) -> ToolSpec {
        tool_specs().into_iter().find(|s| s.name == name).unwrap()
    }

    #[test]
    fn every_command_is_a_tool_except_hidden() {
        let names: Vec<String> = tool_specs().into_iter().map(|s| s.name).collect();
        for n in [
            "compress", "merge", "split", "extract", "delete", "reorder", "rotate", "to_images",
            "images_to_pdf", "text", "protect", "unlock", "strip_metadata", "info", "watermark",
            "number_pages", "ocr", "repair", "check", "linearize", "pdfa", "doctor",
        ] {
            assert!(names.contains(&n.to_string()), "missing tool {n}");
        }
        assert!(!names.contains(&"mcp".to_string()));
        assert!(!names.contains(&"tools".to_string()));
    }

    #[test]
    fn schema_reflects_cli_definitions() {
        let s = spec("compress").input_schema();
        assert_eq!(s["properties"]["inputs"]["type"], "array");
        assert_eq!(s["properties"]["recursive"]["type"], "boolean");
        assert_eq!(s["properties"]["quality"]["type"], "integer");
        assert_eq!(s["properties"]["level"]["enum"], json!(["low", "recommended", "extreme"]));
        assert_eq!(s["required"], json!(["inputs"]));
        assert!(s["properties"].get("json").is_none());
        assert!(s["properties"].get("help").is_none());

        let s = spec("number_pages").input_schema();
        assert_eq!(
            s["properties"]["position"]["enum"][0],
            "bottom-center"
        );
        let m = spec("merge").input_schema();
        assert_eq!(m["required"], json!(["inputs", "output"]));
    }

    #[test]
    fn builds_argv_for_options_and_positionals() {
        let args = json!({ "inputs": ["a.pdf", "b.pdf"], "output": "out.pdf" });
        let argv = argv_for(&spec("merge"), args.as_object().unwrap()).unwrap();
        assert_eq!(
            argv,
            vec!["pdfsqueeze", "--json", "merge", "--output", "out.pdf", "--", "a.pdf", "b.pdf"]
        );

        let args = json!({ "input": "s.pdf", "angle": 90.0, "pages": "1-2" });
        let argv = argv_for(&spec("rotate"), args.as_object().unwrap()).unwrap();
        assert_eq!(
            argv,
            vec!["pdfsqueeze", "--json", "rotate", "--pages", "1-2", "--", "s.pdf", "90"]
        );

        let args = json!({ "inputs": ["dir"], "recursive": true, "quality": 60 });
        let argv = argv_for(&spec("compress"), args.as_object().unwrap()).unwrap();
        assert!(argv.contains(&"--recursive".to_string()));
        assert!(argv.windows(2).any(|w| w == ["--quality", "60"]));
    }

    #[test]
    fn rejects_bad_calls() {
        let bad = json!({ "inputs": ["a.pdf"], "output": "o.pdf", "nope": 1 });
        assert!(argv_for(&spec("merge"), bad.as_object().unwrap()).is_err());
        let missing = json!({ "output": "o.pdf" });
        assert!(argv_for(&spec("merge"), missing.as_object().unwrap()).is_err());
        // values that look like flags must stay values
        let tricky = json!({ "inputs": ["--output", "x.pdf"], "output": "o.pdf" });
        let argv = argv_for(&spec("merge"), tricky.as_object().unwrap()).unwrap();
        assert_eq!(&argv[argv.len() - 3..], ["--", "--output", "x.pdf"]);
    }

    #[test]
    fn path_policy_confines_to_roots() {
        let root = std::env::temp_dir().join(format!("pdfsqueeze-policy-{}", std::process::id()));
        std::fs::create_dir_all(root.join("sub")).unwrap();
        let outside = std::env::temp_dir().join(format!("pdfsqueeze-outside-{}", std::process::id()));
        std::fs::create_dir_all(&outside).unwrap();
        let pol = PathPolicy::new(&[root.clone()], false).unwrap();

        assert!(pol.check(root.join("sub/a.pdf").to_str().unwrap()).is_ok()); // not yet existing
        assert!(pol.check(root.to_str().unwrap()).is_ok());
        assert!(pol.check(outside.join("x.pdf").to_str().unwrap()).is_err());
        if cfg!(unix) {
            assert!(pol.check("/etc/passwd").is_err());
        }
        // traversal through a not-yet-existing directory
        let sneaky = format!("{}/nope/../../x.pdf", root.display());
        assert!(pol.check(&sneaky).is_err());

        #[cfg(unix)]
        {
            let link = root.join("escape");
            let _ = std::os::unix::fs::symlink(&outside, &link);
            assert!(pol.check(link.join("x.pdf").to_str().unwrap()).is_err());
        }
        assert!(PathPolicy::new(&[], true).unwrap().check("anything/at/all.pdf").is_ok());
        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(outside);
    }

    #[test]
    fn protocol_basics() {
        let specs = tool_specs();
        let pol = PathPolicy::new(&[], true).unwrap();
        let init = handle_message(
            &specs,
            &pol,
            &json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}}),
        )
        .unwrap();
        assert_eq!(init["result"]["protocolVersion"], "2024-11-05");
        assert_eq!(init["result"]["serverInfo"]["name"], "pdfsqueeze");

        // notifications get no reply
        assert!(handle_message(&specs, &pol, &json!({"jsonrpc":"2.0","method":"notifications/initialized"})).is_none());

        let list = handle_message(&specs, &pol, &json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})).unwrap();
        let tools = list["result"]["tools"].as_array().unwrap();
        assert!(tools.iter().any(|t| t["name"] == "compress"));
        assert!(tools.iter().all(|t| t["inputSchema"]["type"] == "object"));

        let bad = handle_message(&specs, &pol, &json!({"jsonrpc":"2.0","id":3,"method":"nope"})).unwrap();
        assert_eq!(bad["error"]["code"], -32601);
    }
}
