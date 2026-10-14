//! `pdfsqueeze ui`: a point-and-click interface served from this machine only.
//!
//! A small HTTP server bound to 127.0.0.1 hands a single page to your browser.
//! Files you drop on it are copied into a private temporary folder, the same
//! commands the CLI and the MCP server use run on them there, and results are
//! offered back as downloads. Nothing leaves the computer.
//!
//! Safety: loopback only, a random per-run token on every request, Host header
//! checked (DNS-rebinding), uploads capped, and tool paths confined to the
//! session folder by the same allow-list the MCP server uses.

use crate::agent::{self, PathPolicy, ToolSpec};
use crate::engine::{host_os, Os};
use anyhow::{Context, Result};
use serde_json::{json, Map, Value};
use std::collections::hash_map::RandomState;
use std::collections::HashSet;
use std::hash::{BuildHasher, Hasher};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tiny_http::{Header, Method, Request, Response, Server};

const PAGE: &str = include_str!("ui.html");
/// Background image for the window (original artwork, see scripts/make-hero.py).
const HERO: &[u8] = include_bytes!("../assets/hero.jpg");

/// Largest single upload accepted (1 GiB).
const MAX_UPLOAD: u64 = 1 << 30;
/// Most pages the reorder grid shows previews for; longer files use a typed order.
const PAGE_GRID_MAX: u32 = 200;

/// Biggest JSON body for /api/run.
const MAX_JSON: u64 = 1 << 20;

struct Upload {
    id: String,
    name: String,
    path: PathBuf,
    size: u64,
    /// "pdf", "image" or "other"
    kind: &'static str,
    pages: Option<u32>,
}

/// Look at a file's extension and, for PDFs, count its pages.
fn describe_file(path: &Path) -> (&'static str, Option<u32>) {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "pdf" => (
            "pdf",
            crate::pages::page_count(path).ok(),
        ),
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "tif" | "tiff" => ("image", None),
        _ => ("other", None),
    }
}

struct State {
    token: String,
    port: u16,
    session: PathBuf,
    specs: Vec<ToolSpec>,
    policy: PathPolicy,
    uploads: Mutex<Vec<Upload>>,
    counter: Mutex<u64>,
    /// Files this window has saved, so "show in folder" cannot be pointed elsewhere.
    saved: Mutex<HashSet<PathBuf>>,
}

impl State {
    /// Add a file the user dropped from disk. It is used where it is, not copied.
    fn register_path(&self, path: &Path) -> Option<Value> {
        let meta = std::fs::metadata(path).ok().filter(|m| m.is_file())?;
        let (kind, pages) = describe_file(path);
        let id = self.next_id().to_string();
        let name = path.file_name()?.to_string_lossy().into_owned();
        self.uploads.lock().unwrap().push(Upload {
            id: id.clone(),
            name: name.clone(),
            path: path.to_path_buf(),
            size: meta.len(),
            kind,
            pages,
        });
        Some(json!({ "id": id, "name": name, "size": meta.len(), "kind": kind, "pages": pages }))
    }

    fn next_id(&self) -> u64 {
        let mut c = self.counter.lock().unwrap();
        *c += 1;
        *c
    }
}

fn random_hex() -> String {
    // Straight from the operating system's random source where there is one.
    #[cfg(unix)]
    {
        use std::io::Read;
        let mut buf = [0u8; 16];
        if std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut buf)).is_ok() {
            return buf.iter().map(|b| format!("{b:02x}")).collect();
        }
    }
    // Otherwise RandomState, which is seeded from the operating system.
    let mut out = String::new();
    for i in 0..2u64 {
        let mut h = RandomState::new().build_hasher();
        h.write_u64(i);
        h.write_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0),
        );
        out.push_str(&format!("{:016x}", h.finish()));
    }
    out
}

/// Keep a file name safe to use as a path component.
pub fn sanitize_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let cleaned: String = base
        .chars()
        .map(|c| if c.is_alphanumeric() || matches!(c, '.' | '-' | '_' | ' ' | '(' | ')') { c } else { '_' })
        .collect();
    let cleaned = cleaned.trim_matches(|c: char| c == '.' || c == ' ').to_string();
    if cleaned.is_empty() {
        "file".into()
    } else {
        cleaned.chars().take(120).collect()
    }
}

pub fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len() => {
                let hex = std::str::from_utf8(&b[i + 1..i + 3]).ok();
                if let Some(v) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    out.push(v);
                    i += 3;
                } else {
                    out.push(b'%');
                    i += 1;
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn query_param(query: &str, key: &str) -> Option<String> {
    query.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        (k == key).then(|| percent_decode(v))
    })
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("valid header")
}

fn respond_json(req: Request, status: u16, v: &Value) {
    let body = serde_json::to_vec(v).unwrap_or_default();
    let resp = Response::from_data(body)
        .with_status_code(status)
        .with_header(header("Content-Type", "application/json"))
        .with_header(header("Cache-Control", "no-store"));
    let _ = req.respond(resp);
}

fn respond_error(req: Request, status: u16, code: &str, message: &str) {
    respond_json(
        req,
        status,
        &json!({ "ok": false, "error": { "code": code, "message": message } }),
    );
}

/// Every string anywhere in `v` that names an existing file under `dir`.
fn collect_files(v: &Value, dir: &Path, out: &mut Vec<PathBuf>) {
    match v {
        Value::String(s) => {
            let p = Path::new(s);
            if p.starts_with(dir) && p.is_file() && !out.contains(&p.to_path_buf()) {
                out.push(p.to_path_buf());
            }
        }
        Value::Array(a) => a.iter().for_each(|x| collect_files(x, dir, out)),
        Value::Object(o) => o.values().for_each(|x| collect_files(x, dir, out)),
        _ => {}
    }
}

fn content_type(name: &str) -> &'static str {
    match name.rsplit('.').next().unwrap_or("").to_ascii_lowercase().as_str() {
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn handle(state: &State, mut req: Request) {
    let url = req.url().to_string();
    let (path, query) = url.split_once('?').unwrap_or((url.as_str(), ""));
    let path = path.to_string();
    let query = query.to_string();

    // DNS-rebinding guard: only our own address is a valid Host.
    let host_ok = req
        .headers()
        .iter()
        .find(|h| h.field.equiv("Host"))
        .map(|h| {
            let v = h.value.as_str();
            v == format!("127.0.0.1:{}", state.port) || v == format!("localhost:{}", state.port)
        })
        .unwrap_or(false);
    if !host_ok {
        return respond_error(req, 403, "forbidden", "unexpected Host header");
    }

    // The token arrives as a header (API calls) or ?t= (page load, downloads).
    let given = req
        .headers()
        .iter()
        .find(|h| h.field.equiv("X-Token"))
        .map(|h| h.value.as_str().to_string())
        .or_else(|| query_param(&query, "t"));
    if given.as_deref() != Some(state.token.as_str()) {
        return respond_error(req, 403, "forbidden", "missing or wrong token");
    }

    let method = req.method().clone();
    match (&method, path.as_str()) {
        (Method::Get, "/") => {
            let resp = Response::from_string(PAGE)
                .with_header(header("Content-Type", "text/html; charset=utf-8"))
                .with_header(header("Cache-Control", "no-store"))
                .with_header(header("X-Content-Type-Options", "nosniff"))
                .with_header(header("Referrer-Policy", "no-referrer"))
                .with_header(header(
                    "Content-Security-Policy",
                    "default-src 'self'; img-src 'self' data: blob:; style-src 'self' 'unsafe-inline'; \
                     script-src 'self' 'unsafe-inline'; connect-src 'self'; object-src 'none'; \
                     base-uri 'none'; form-action 'none'; frame-ancestors 'none'",
                ));
            let _ = req.respond(resp);
        }

        (Method::Get, "/hero.jpg") => {
            let resp = Response::from_data(HERO)
                .with_header(header("Content-Type", "image/jpeg"))
                .with_header(header("Cache-Control", "max-age=86400"));
            let _ = req.respond(resp);
        }

        (Method::Get, "/api/tools") => {
            let d = agent::describe();
            respond_json(req, 200, &json!({ "ok": true, "result": d.data }));
        }

        (Method::Get, "/api/files") => {
            let list: Vec<Value> = state
                .uploads
                .lock()
                .unwrap()
                .iter()
                .map(|u| json!({ "id": u.id, "name": u.name, "size": u.size, "kind": u.kind, "pages": u.pages }))
                .collect();
            respond_json(req, 200, &json!({ "ok": true, "files": list }));
        }

        (Method::Post, "/api/upload") => {
            let name = sanitize_name(&query_param(&query, "name").unwrap_or_else(|| "file".into()));
            let id = state.next_id().to_string();
            let dir = state.session.join("in").join(&id);
            if std::fs::create_dir_all(&dir).is_err() {
                return respond_error(req, 500, "error", "could not create the upload folder");
            }
            let dest = dir.join(&name);
            let size = match std::fs::File::create(&dest) {
                Ok(mut f) => {
                    let mut reader = req.as_reader().take(MAX_UPLOAD + 1);
                    match std::io::copy(&mut reader, &mut f) {
                        Ok(n) => n,
                        Err(e) => {
                            let _ = std::fs::remove_dir_all(&dir);
                            return respond_error(req, 500, "error", &format!("upload failed: {e}"));
                        }
                    }
                }
                Err(e) => return respond_error(req, 500, "error", &format!("cannot write: {e}")),
            };
            if size > MAX_UPLOAD {
                let _ = std::fs::remove_dir_all(&dir);
                return respond_error(req, 413, "too_large", "file is larger than 1 GB");
            }
            let (kind, pages) = describe_file(&dest);
            state.uploads.lock().unwrap().push(Upload {
                id: id.clone(),
                name: name.clone(),
                path: dest,
                size,
                kind,
                pages,
            });
            respond_json(
                req,
                200,
                &json!({ "ok": true, "file": { "id": id, "name": name, "size": size, "kind": kind, "pages": pages } }),
            );
        }

        (Method::Post, "/api/run") => {
            let mut body = String::new();
            if req
                .as_reader()
                .take(MAX_JSON)
                .read_to_string(&mut body)
                .is_err()
            {
                return respond_error(req, 400, "usage", "could not read the request");
            }
            let parsed: Value = match serde_json::from_str(&body) {
                Ok(v) => v,
                Err(e) => return respond_error(req, 400, "usage", &format!("invalid JSON: {e}")),
            };
            let out = run_tool(state, &parsed);
            respond_json(req, 200, &out);
        }

        (Method::Get, "/api/download") => {
            let rel = query_param(&query, "rel").unwrap_or_default();
            let full = state.session.join(&rel);
            // Resolve links and `..` before checking it is inside the results folder.
            let ok = inside_results(&state.session, &full);
            if !ok {
                return respond_error(req, 404, "input_not_found", "no such file");
            }
            let name = full.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            match std::fs::File::open(&full) {
                Ok(f) => {
                    let resp = Response::from_file(f)
                        .with_header(header("Content-Type", content_type(&name)))
                        .with_header(header(
                            "Content-Disposition",
                            &format!("attachment; filename=\"{}\"", name.replace('"', "_")),
                        ));
                    let _ = req.respond(resp);
                }
                Err(_) => respond_error(req, 404, "input_not_found", "no such file"),
            }
        }

        (Method::Get, "/api/thumb") => {
            let id = query_param(&query, "id").unwrap_or_default();
            let found = state
                .uploads
                .lock()
                .unwrap()
                .iter()
                .find(|u| u.id == id)
                .map(|u| (u.path.clone(), u.kind));
            let Some((path, kind)) = found else {
                return respond_error(req, 404, "input_not_found", "unknown file");
            };
            let png = match kind {
                "image" => path.clone(),
                "pdf" => {
                    let dir = state.session.join("thumbs");
                    let _ = std::fs::create_dir_all(&dir);
                    let out = dir.join(format!("{id}.png"));
                    if !out.is_file() && crate::convert::render_thumbnail(&path, &out).is_err() {
                        return respond_error(req, 404, "error", "no preview");
                    }
                    out
                }
                _ => return respond_error(req, 404, "error", "no preview"),
            };
            match std::fs::File::open(&png) {
                Ok(f) => {
                    let name = png.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    let resp = Response::from_file(f)
                        .with_header(header("Content-Type", content_type(&name)))
                        .with_header(header("Cache-Control", "max-age=3600"));
                    let _ = req.respond(resp);
                }
                Err(_) => respond_error(req, 404, "error", "no preview"),
            }
        }

        // Page count of an uploaded PDF, rendering a preview of every page
        // once (the page grid for reordering reads them via /api/pagethumb).
        (Method::Get, "/api/pages") => {
            let id = query_param(&query, "id").unwrap_or_default();
            let path = state
                .uploads
                .lock()
                .unwrap()
                .iter()
                .find(|u| u.id == id && u.kind == "pdf")
                .map(|u| u.path.clone());
            let Some(path) = path else {
                return respond_error(req, 404, "input_not_found", "unknown PDF");
            };
            let count = match crate::pages::page_count(&path) {
                Ok(n) => n,
                Err(e) => return respond_error(req, 422, "error", &format!("{e:#}")),
            };
            let shown = count.min(PAGE_GRID_MAX);
            let dir = state.session.join("pagethumbs");
            if count <= PAGE_GRID_MAX && !dir.join(format!("{id}-1.png")).is_file() {
                if let Err(e) = crate::convert::render_page_thumbnails(&path, &dir, &id, shown) {
                    return respond_error(req, 422, "error", &format!("{e:#}"));
                }
            }
            respond_json(req, 200, &json!({ "ok": true, "count": count, "max": PAGE_GRID_MAX }));
        }

        // One page rendered large enough to place a signature on.
        (Method::Get, "/api/pagepreview") => {
            let id = query_param(&query, "id").unwrap_or_default();
            let page = query_param(&query, "page").and_then(|p| p.parse::<u32>().ok()).unwrap_or(0);
            let path = state
                .uploads
                .lock()
                .unwrap()
                .iter()
                .find(|u| u.id == id && u.kind == "pdf")
                .map(|u| u.path.clone());
            let Some(path) = path else {
                return respond_error(req, 404, "input_not_found", "unknown PDF");
            };
            if page == 0 {
                return respond_error(req, 404, "error", "no such page");
            }
            let dir = state.session.join("previews");
            let _ = std::fs::create_dir_all(&dir);
            let out = dir.join(format!("{id}-{page}.png"));
            if !out.is_file() && crate::convert::render_page_color(&path, page, 100, &out).is_err() {
                return respond_error(req, 404, "error", "no preview");
            }
            match std::fs::File::open(&out) {
                Ok(f) => {
                    let resp = Response::from_file(f)
                        .with_header(header("Content-Type", "image/png"))
                        .with_header(header("Cache-Control", "max-age=3600"));
                    let _ = req.respond(resp);
                }
                Err(_) => respond_error(req, 404, "error", "no preview"),
            }
        }

        (Method::Get, "/api/pagethumb") => {
            let id = query_param(&query, "id").unwrap_or_default();
            let page = query_param(&query, "page").and_then(|p| p.parse::<u32>().ok()).unwrap_or(0);
            let known = state.uploads.lock().unwrap().iter().any(|u| u.id == id);
            if !known || page == 0 {
                return respond_error(req, 404, "error", "no preview");
            }
            let png = state.session.join("pagethumbs").join(format!("{id}-{page}.png"));
            match std::fs::File::open(&png) {
                Ok(f) => {
                    let resp = Response::from_file(f)
                        .with_header(header("Content-Type", "image/png"))
                        .with_header(header("Cache-Control", "max-age=3600"));
                    let _ = req.respond(resp);
                }
                Err(_) => respond_error(req, 404, "error", "no preview"),
            }
        }

        // What the running job reports (page counts, files done); the window polls this.
        (Method::Get, "/api/progress") => {
            let p = crate::progress::snapshot();
            respond_json(req, 200, &json!({ "ok": true, "fraction": p.fraction, "label": p.label }));
        }

        (Method::Post, "/api/remove") => {
            let mut body = String::new();
            let _ = req.as_reader().take(MAX_JSON).read_to_string(&mut body);
            let id = serde_json::from_str::<Value>(&body)
                .ok()
                .and_then(|v| v["id"].as_str().map(String::from))
                .unwrap_or_default();
            state.uploads.lock().unwrap().retain(|u| u.id != id);
            respond_json(req, 200, &json!({ "ok": true }));
        }

        // Copy a result into the Downloads folder (never overwriting) and say where.
        (Method::Post, "/api/save") => {
            let mut body = String::new();
            let _ = req.as_reader().take(MAX_JSON).read_to_string(&mut body);
            let rel = serde_json::from_str::<Value>(&body)
                .ok()
                .and_then(|v| v["rel"].as_str().map(String::from))
                .unwrap_or_default();
            let src = state.session.join(&rel);
            let inside = inside_results(&state.session, &src);
            if !inside {
                return respond_error(req, 404, "input_not_found", "no such result");
            }
            match save_to_downloads(&src) {
                Ok(dest) => {
                    state.saved.lock().unwrap().insert(dest.clone());
                    respond_json(
                        req,
                        200,
                        &json!({ "ok": true, "path": dest.to_string_lossy(),
                                 "folder": dest.parent().map(|p| p.to_string_lossy().into_owned()) }),
                    );
                }
                Err(e) => respond_error(req, 500, "error", &format!("could not save: {e}")),
            }
        }

        // Several results at once: one new folder in Downloads, never overwriting anything.
        (Method::Post, "/api/save_all") => {
            let mut body = String::new();
            let _ = req.as_reader().take(MAX_JSON).read_to_string(&mut body);
            let v: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
            let rels: Vec<String> = v["rels"]
                .as_array()
                .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                .unwrap_or_default();
            if rels.is_empty() || rels.len() > 5000 {
                return respond_error(req, 400, "usage", "nothing to save");
            }
            let Ok(session) = state.session.join("out").canonicalize() else {
                return respond_error(req, 500, "error", "session folder is missing");
            };
            let mut sources: Vec<PathBuf> = Vec::new();
            for rel in &rels {
                match state.session.join(rel).canonicalize() {
                    Ok(f) if f.starts_with(&session) && f.is_file() => sources.push(f),
                    _ => return respond_error(req, 404, "input_not_found", "no such result"),
                }
            }
            // A folder name from the page is only a suggestion: keep it to one safe path segment.
            let wanted: String = v["folder"]
                .as_str()
                .unwrap_or("pdfsqueeze results")
                .chars()
                .map(|c| if c.is_alphanumeric() || " ._-()".contains(c) { c } else { '_' })
                .collect::<String>()
                .trim()
                .trim_matches('.')
                .chars()
                .take(80)
                .collect();
            let wanted = if wanted.is_empty() { "pdfsqueeze results".to_string() } else { wanted };
            let dest = unique_path(&downloads_dir(), &wanted);
            if let Err(e) = std::fs::create_dir_all(&dest) {
                return respond_error(req, 500, "error", &format!("could not create the folder: {e}"));
            }
            for src in &sources {
                let name = src.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "file".into());
                let target = unique_path(&dest, &name);
                if let Err(e) = std::fs::copy(src, &target) {
                    return respond_error(req, 500, "error", &format!("could not save {name}: {e}"));
                }
                state.saved.lock().unwrap().insert(target);
            }
            state.saved.lock().unwrap().insert(dest.clone());
            respond_json(
                req,
                200,
                &json!({ "ok": true, "folder": dest.to_string_lossy(), "count": sources.len() }),
            );
        }

        (Method::Post, "/api/reveal") | (Method::Post, "/api/open") => {
            let mut body = String::new();
            let _ = req.as_reader().take(MAX_JSON).read_to_string(&mut body);
            let p = serde_json::from_str::<Value>(&body)
                .ok()
                .and_then(|v| v["path"].as_str().map(PathBuf::from))
                .unwrap_or_default();
            if !state.saved.lock().unwrap().contains(&p) {
                return respond_error(req, 403, "forbidden", "that file was not saved by this window");
            }
            reveal_or_open(&p, path == "/api/reveal");
            respond_json(req, 200, &json!({ "ok": true }));
        }

        _ => respond_error(req, 404, "not_found", "unknown address"),
    }
}

/// True when `file` is a real file inside this session's results folder (`out/`).
/// Uploads, previews and the temporary password file are never offered back.
fn inside_results(session: &Path, file: &Path) -> bool {
    file.canonicalize()
        .ok()
        .zip(session.join("out").canonicalize().ok())
        .map(|(f, out)| f.starts_with(out) && f.is_file())
        .unwrap_or(false)
}

fn downloads_dir() -> PathBuf {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let d = home.join("Downloads");
    if d.is_dir() {
        d
    } else {
        home
    }
}

/// `report.pdf` -> `report.pdf`, then `report (1).pdf`, `report (2).pdf`...
pub fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let p = Path::new(name);
    let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "file".into());
    let ext = p.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    (1..10_000)
        .map(|n| dir.join(format!("{stem} ({n}){ext}")))
        .find(|c| !c.exists())
        .unwrap_or(first)
}

fn save_to_downloads(src: &Path) -> std::io::Result<PathBuf> {
    let dir = downloads_dir();
    let name = src.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "result.pdf".into());
    let dest = unique_path(&dir, &name);
    std::fs::copy(src, &dest)?;
    Ok(dest)
}

fn reveal_or_open(path: &Path, reveal: bool) {
    let _ = match host_os() {
        Os::Mac if reveal => std::process::Command::new("open").arg("-R").arg(path).spawn(),
        Os::Mac => std::process::Command::new("open").arg(path).spawn(),
        Os::Windows if reveal => std::process::Command::new("explorer")
            .arg(format!("/select,{}", path.display()))
            .spawn(),
        // explorer opens a file with its default program and never goes through a
        // command interpreter, so characters like & in a file name are inert.
        Os::Windows => std::process::Command::new("explorer").arg(path).spawn(),
        Os::Linux => std::process::Command::new("xdg-open")
            .arg(if reveal { path.parent().unwrap_or(path) } else { path })
            .spawn(),
    };
}

/// Turn `{tool, files, args, password}` from the page into a tool call.
fn run_tool(state: &State, req: &Value) -> Value {
    let tool = req["tool"].as_str().unwrap_or("").to_string();
    let Some(spec) = state.specs.iter().find(|s| s.name == tool) else {
        return json!({ "ok": false, "error": { "code": "usage", "message": format!("unknown tool `{tool}`") } });
    };
    let schema = spec.input_schema();
    let props = schema["properties"].as_object().cloned().unwrap_or_default();

    let mut args: Map<String, Value> = req["args"].as_object().cloned().unwrap_or_default();
    // Paths are decided by this server (uploaded ids, results folder), never by the page.
    for k in ["input", "inputs", "output", "out_dir", "tessdata", "password_file", "image"] {
        args.remove(k);
    }

    // Uploaded files arrive as ids; map them to their private paths.
    let uploads = state.uploads.lock().unwrap();
    let find = |id: &str| uploads.iter().find(|u| u.id == id);
    let mut first_name: Option<String> = None;
    if let Some(files) = req["files"].as_object() {
        for (arg, ids) in files {
            if !props.contains_key(arg) {
                continue;
            }
            let ids: Vec<String> = match ids {
                Value::Array(a) => a.iter().filter_map(|v| v.as_str().map(String::from)).collect(),
                Value::String(s) => vec![s.clone()],
                _ => vec![],
            };
            let mut paths: Vec<String> = Vec::new();
            for id in &ids {
                match find(id) {
                    Some(u) => {
                        // Name the result after the document, not after a helper picture.
                        if first_name.is_none() && (arg == "input" || arg == "inputs") {
                            first_name = Some(u.name.clone());
                        }
                        paths.push(u.path.to_string_lossy().into_owned());
                    }
                    None => {
                        return json!({ "ok": false, "error": { "code": "input_not_found",
                            "message": format!("file {id} is no longer available; add it again") } })
                    }
                }
            }
            let is_array = props[arg]["type"] == "array";
            args.insert(
                arg.clone(),
                if is_array { json!(paths) } else { json!(paths.first().cloned().unwrap_or_default()) },
            );
        }
    }
    drop(uploads);

    // Results go into a fresh folder inside the session.
    let run_id = state.next_id();
    let out_dir = state.session.join("out").join(format!("run-{run_id}"));
    let _ = std::fs::create_dir_all(&out_dir);
    if props.contains_key("out_dir") && !args.contains_key("out_dir") {
        args.insert("out_dir".into(), json!(out_dir.to_string_lossy()));
    } else if props.contains_key("output") && !args.contains_key("output") {
        let stem = first_name
            .as_deref()
            .map(|n| Path::new(n).file_stem().map(|s| sanitize_name(&s.to_string_lossy())).unwrap_or_default())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "result".into());
        let ext = if tool == "text" { "txt" } else { "pdf" };
        args.insert(
            "output".into(),
            json!(out_dir.join(format!("{stem}-{tool}.{ext}")).to_string_lossy()),
        );
    }

    // A typed password becomes a private file so it never appears in an argument list.
    let mut pw_file: Option<PathBuf> = None;
    if let Some(pw) = req["password"].as_str().filter(|p| !p.is_empty()) {
        if props.contains_key("password_file") {
            let f = state.session.join(format!("pw-{run_id}.txt"));
            if std::fs::write(&f, pw).is_ok() {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o600));
                }
                args.insert("password_file".into(), json!(f.to_string_lossy()));
                pw_file = Some(f);
            }
        }
    }

    // Drop empty strings so optional fields left blank behave as "not given".
    args.retain(|_, v| !matches!(v, Value::String(s) if s.trim().is_empty()) && !v.is_null());

    crate::progress::reset();
    let (envelope, _is_err) = agent::call_tool(&state.specs, &state.policy, &tool, &args);
    if let Some(f) = pw_file {
        let _ = std::fs::remove_file(f);
    }

    // Offer every file the command produced as a download.
    let mut produced: Vec<PathBuf> = Vec::new();
    collect_files(&envelope, &out_dir, &mut produced);
    let downloads: Vec<Value> = produced
        .iter()
        .filter_map(|p| {
            let rel = p.strip_prefix(&state.session).ok()?;
            Some(json!({
                "name": p.file_name()?.to_string_lossy(),
                "rel": rel.to_string_lossy().replace('\\', "/"),
                "size": std::fs::metadata(p).map(|m| m.len()).unwrap_or(0),
            }))
        })
        .collect();

    json!({ "envelope": envelope, "downloads": downloads })
}

fn open_browser(url: &str) {
    let spawned = match host_os() {
        Os::Mac => std::process::Command::new("open").arg(url).spawn(),
        Os::Linux => std::process::Command::new("xdg-open").arg(url).spawn(),
        Os::Windows => std::process::Command::new("explorer").arg(url).spawn(),
    };
    if spawned.is_err() {
        eprintln!("Could not open a browser automatically. Open the address above yourself.");
    }
}

/// Remove sessions from earlier runs that were not cleaned up (older than a day).
fn sweep_old_sessions() {
    let Ok(rd) = std::fs::read_dir(std::env::temp_dir()) else { return };
    let day = std::time::Duration::from_secs(24 * 3600);
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if !name.starts_with("pdfsqueeze-ui-") {
            continue;
        }
        let old = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .map(|a| a > day)
            .unwrap_or(false);
        if old {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

/// How `pdfsqueeze ui` shows itself.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// Its own application window (macOS and Windows).
    Window,
    /// The default web browser.
    Browser,
    /// Only serve; print the address.
    ServerOnly,
}

pub fn window_available() -> bool {
    cfg!(any(target_os = "macos", target_os = "windows"))
}

pub fn serve(port: Option<u16>, mode: Mode) -> Result<()> {
    sweep_old_sessions();

    let server = Server::http(("127.0.0.1", port.unwrap_or(0)))
        .map_err(|e| anyhow::anyhow!("could not start the local server: {e}"))?;
    let port = server
        .server_addr()
        .to_ip()
        .map(|a| a.port())
        .context("could not read the server port")?;

    let session = std::env::temp_dir().join(format!("pdfsqueeze-ui-{}", random_hex()));
    std::fs::create_dir_all(session.join("in"))?;
    std::fs::create_dir_all(session.join("out"))?;
    #[cfg(unix)]
    {
        // Other users of a shared machine must not be able to read the documents here.
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&session, std::fs::Permissions::from_mode(0o700));
    }
    let session = session.canonicalize().map(crate::engine::plain_path).unwrap_or(session);

    let state = Arc::new(State {
        token: random_hex(),
        port,
        // Tool paths are built by this server from registered files, never taken
        // from the page, so the allow-list does not need to confine them.
        policy: PathPolicy::new(&[], true)?,
        specs: agent::tool_specs(),
        session: session.clone(),
        uploads: Mutex::new(Vec::new()),
        counter: Mutex::new(0),
        saved: Mutex::new(HashSet::new()),
    });

    let url = format!("http://127.0.0.1:{port}/?t={}", state.token);

    let serving = Arc::clone(&state);
    let worker = std::thread::spawn(move || {
        for request in server.incoming_requests() {
            let state = Arc::clone(&serving);
            std::thread::spawn(move || handle(&state, request));
        }
    });

    match mode {
        Mode::Window => {
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            {
                window::run(&url, Arc::clone(&state))?;
                // closing the window ends the program
                let _ = std::fs::remove_dir_all(&session);
                return Ok(());
            }
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            {
                // no embedded web view on this platform: fall through to the browser
                println!("This platform has no built-in window; opening your browser instead.");
                open_browser(&url);
            }
        }
        Mode::Browser => {
            // The address holds the token, and a program's arguments are visible to other
            // users of the machine. Open a private local page that forwards to it instead.
            let hop = session.join("open.html");
            let page = format!(
                "<!doctype html><meta charset=utf-8><meta http-equiv=refresh content=\"0;url={url}\">\
                 <a href=\"{url}\">Continue to pdfsqueeze</a>"
            );
            if std::fs::write(&hop, page).is_ok() {
                open_browser(&hop.to_string_lossy());
            } else {
                open_browser(&url);
            }
        }
        Mode::ServerOnly => {}
    }

    println!("pdfsqueeze is running at:\n  {url}\n");
    println!("Files stay on this computer. Press Ctrl+C to stop.");
    println!("Working folder: {}", session.display());
    let _ = worker.join();
    Ok(())
}

/// The application window: the system's own web view in a native frame, so there
/// is no browser, tab strip or address bar. Dropped files arrive as real paths.
#[cfg(any(target_os = "macos", target_os = "windows"))]
mod window {
    use super::*;
    use tao::dpi::LogicalSize;
    use tao::event::{Event, WindowEvent};
    use tao::event_loop::{ControlFlow, EventLoopBuilder};
    use tao::window::WindowBuilder;
    use wry::{DragDropEvent, WebViewBuilder};

    enum UserEvent {
        FilesChanged,
        Dragging(bool),
    }

    /// The window toolkit creates no menu, and on macOS that means no Cmd+Q, no
    /// Cmd+C / Cmd+V in text fields and no Hide or Minimise. Provide the standard ones.
    #[cfg(target_os = "macos")]
    fn install_menu() {
        use muda::{Menu, PredefinedMenuItem, Submenu};
        let menu = Menu::new();
        let app = Submenu::new("pdfsqueeze", true);
        let _ = app.append_items(&[
            &PredefinedMenuItem::about(None, None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::hide(None),
            &PredefinedMenuItem::hide_others(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::quit(None),
        ]);
        let edit = Submenu::new("Edit", true);
        let _ = edit.append_items(&[
            &PredefinedMenuItem::undo(None),
            &PredefinedMenuItem::redo(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::cut(None),
            &PredefinedMenuItem::copy(None),
            &PredefinedMenuItem::paste(None),
            &PredefinedMenuItem::select_all(None),
        ]);
        let window = Submenu::new("Window", true);
        let _ = window.append_items(&[
            &PredefinedMenuItem::minimize(None),
            &PredefinedMenuItem::close_window(None),
        ]);
        let _ = menu.append_items(&[&app, &edit, &window]);
        menu.init_for_nsapp();
        std::mem::forget(menu); // it must outlive the event loop
    }

    /// The Dock shows a generic icon for a bare binary; give it ours. (Inside the .app bundle
    /// the icon comes from the bundle, and this simply sets the same picture.)
    #[cfg(target_os = "macos")]
    fn set_dock_icon() {
        use objc2::{AnyThread, MainThreadMarker};
        use objc2_app_kit::{NSApplication, NSImage};
        use objc2_foundation::NSData;
        let Some(mtm) = MainThreadMarker::new() else { return };
        let data = NSData::with_bytes(include_bytes!("../assets/icon-mac.png"));
        if let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) {
            // SAFETY: called on the main thread with a valid image.
            unsafe { NSApplication::sharedApplication(mtm).setApplicationIconImage(Some(&image)) };
        }
    }

    pub fn run(url: &str, state: Arc<State>) -> Result<()> {
        let session = state.session.clone();
        let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
        let proxy = event_loop.create_proxy();
        #[cfg(target_os = "macos")]
        install_menu();

        let icon = image::load_from_memory(include_bytes!("../assets/icon-256.png"))
            .ok()
            .and_then(|i| {
                let rgba = i.to_rgba8();
                let (w, h) = rgba.dimensions();
                tao::window::Icon::from_rgba(rgba.into_raw(), w, h).ok()
            });
        #[cfg(target_os = "macos")]
        set_dock_icon();
        let builder = WindowBuilder::new().with_window_icon(icon.clone());
        // Windows keeps a separate icon for the taskbar button.
        #[cfg(target_os = "windows")]
        let builder = {
            use tao::platform::windows::WindowBuilderExtWindows;
            builder.with_taskbar_icon(icon)
        };
        let window = builder
            .with_title("pdfsqueeze")
            .with_inner_size(LogicalSize::new(1180.0, 800.0))
            .with_min_inner_size(LogicalSize::new(820.0, 560.0))
            .build(&event_loop)
            .map_err(|e| anyhow::anyhow!("could not open a window: {e}"))?;

        let drop_proxy = proxy.clone();
        let drop_state = Arc::clone(&state);
        let webview = WebViewBuilder::new()
            .with_url(url)
            .with_drag_drop_handler(move |event| {
                match event {
                    DragDropEvent::Enter { .. } => {
                        let _ = drop_proxy.send_event(UserEvent::Dragging(true));
                    }
                    DragDropEvent::Leave => {
                        let _ = drop_proxy.send_event(UserEvent::Dragging(false));
                    }
                    DragDropEvent::Drop { paths, .. } => {
                        let mut added = false;
                        for p in paths {
                            if drop_state.register_path(&p).is_some() {
                                added = true;
                            }
                        }
                        let _ = drop_proxy.send_event(UserEvent::Dragging(false));
                        if added {
                            let _ = drop_proxy.send_event(UserEvent::FilesChanged);
                        }
                    }
                    _ => {}
                }
                true // we handled it; the page never sees the raw drop
            })
            .build(&window)
            .map_err(|e| anyhow::anyhow!("could not start the web view: {e}"))?;

        event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Wait;
            match event {
                Event::UserEvent(UserEvent::FilesChanged) => {
                    let _ = webview.evaluate_script("window.native && window.native.filesChanged()");
                }
                Event::UserEvent(UserEvent::Dragging(on)) => {
                    let _ = webview.evaluate_script(&format!(
                        "window.native && window.native.dragging({on})"
                    ));
                }
                Event::WindowEvent {
                    event: WindowEvent::CloseRequested,
                    ..
                } => *control_flow = ControlFlow::Exit,
                // Reached by closing the window and by Cmd+Q / the Quit menu item:
                // remove the private working folder (uploads, results, previews).
                Event::LoopDestroyed => {
                    let _ = std::fs::remove_dir_all(&session);
                }
                _ => {}
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_are_made_safe() {
        assert_eq!(sanitize_name("report.pdf"), "report.pdf");
        assert_eq!(sanitize_name("../../etc/passwd"), "passwd");
        assert_eq!(sanitize_name("C:\\Users\\me\\a b.pdf"), "a b.pdf");
        assert_eq!(sanitize_name("we<ird>:name?.pdf"), "we_ird__name_.pdf");
        assert_eq!(sanitize_name("..."), "file");
        assert_eq!(sanitize_name(""), "file");
    }

    #[test]
    fn decodes_percent_escapes() {
        assert_eq!(percent_decode("a%20b"), "a b");
        assert_eq!(percent_decode("my+file.pdf"), "my file.pdf");
        assert_eq!(percent_decode("caf%C3%A9"), "café");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("bad%zzx"), "bad%zzx");
    }

    #[test]
    fn query_params_are_found() {
        assert_eq!(query_param("name=a%20b.pdf&t=abc", "name").as_deref(), Some("a b.pdf"));
        assert_eq!(query_param("name=x&t=abc", "t").as_deref(), Some("abc"));
        assert_eq!(query_param("name=x", "t"), None);
    }

    #[test]
    fn saved_files_never_overwrite() {
        let dir = std::env::temp_dir().join(format!("pdfsqueeze-uniq-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(unique_path(&dir, "a.pdf"), dir.join("a.pdf"));
        std::fs::write(dir.join("a.pdf"), b"x").unwrap();
        assert_eq!(unique_path(&dir, "a.pdf"), dir.join("a (1).pdf"));
        std::fs::write(dir.join("a (1).pdf"), b"x").unwrap();
        assert_eq!(unique_path(&dir, "a.pdf"), dir.join("a (2).pdf"));
        assert_eq!(unique_path(&dir, "noext"), dir.join("noext"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn tokens_are_long_and_differ() {
        let a = random_hex();
        let b = random_hex();
        assert_eq!(a.len(), 32);
        assert_ne!(a, b);
    }

    #[test]
    fn only_files_inside_the_run_folder_are_offered() {
        let root = std::env::temp_dir().join(format!("pdfsqueeze-uitest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("out")).unwrap();
        std::fs::write(root.join("out/a.pdf"), b"x").unwrap();
        std::fs::write(root.join("secret.txt"), b"x").unwrap();
        let v = json!({ "output": root.join("out/a.pdf").to_string_lossy(),
                        "other": root.join("secret.txt").to_string_lossy(), "n": 3 });
        let mut found = Vec::new();
        collect_files(&v, &root.join("out"), &mut found);
        assert_eq!(found, vec![root.join("out/a.pdf")]);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn only_the_results_folder_is_downloadable() {
        let root = std::env::temp_dir().join(format!("pdfsqueeze-res-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("out/run-1")).unwrap();
        std::fs::create_dir_all(root.join("in/1")).unwrap();
        std::fs::write(root.join("out/run-1/a.pdf"), b"x").unwrap();
        std::fs::write(root.join("in/1/secret.pdf"), b"x").unwrap();
        std::fs::write(root.join("pw-3.txt"), b"hunter2").unwrap();
        assert!(inside_results(&root, &root.join("out/run-1/a.pdf")));
        assert!(!inside_results(&root, &root.join("in/1/secret.pdf")));
        assert!(!inside_results(&root, &root.join("pw-3.txt")));
        assert!(!inside_results(&root, &root.join("out/../pw-3.txt")));
        let _ = std::fs::remove_dir_all(root);
    }
}
