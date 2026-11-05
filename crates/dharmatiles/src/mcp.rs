//! `dharmatiles mcp`: the Model Context Protocol over stdio, so an agent
//! can check, build, preview and audit tiles. Newline-delimited JSON-RPC;
//! each tool runs this same program's own commands (`check`, `build`,
//! `elements`, `audit` with `--json`), so what an agent sees is what a
//! person sees.

use std::io::{BufRead, Write};
use std::process::Command;

use serde_json::{Value, json};

const PROTOCOL: &str = "2024-11-05";

/// Serve until stdin closes.
pub fn run() -> u8 {
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(msg) => handle(&msg),
            Err(e) => Some(error(&Value::Null, -32700, &format!("parse error: {e}"))),
        };
        if let Some(r) = reply
            && (writeln!(out, "{r}").is_err() || out.flush().is_err())
        {
            break;
        }
    }
    0
}

/// Serve the same tools as Streamable HTTP: `POST /mcp` with one JSON-RPC
/// message answers with its reply as `application/json` (202 for a
/// notification). Local use: it binds where it is told and does no
/// authentication, so give it a loopback address.
pub fn serve_http(addr: &str) -> u8 {
    let listener = match std::net::TcpListener::bind(addr) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("error: cannot listen on {addr}: {e}");
            return 1;
        }
    };
    eprintln!("dharmatiles mcp: http://{addr}/mcp");
    for conn in listener.incoming().flatten() {
        std::thread::spawn(move || http_one(conn));
    }
    0
}

fn http_one(mut conn: std::net::TcpStream) {
    use std::io::Read;
    let Ok(clone) = conn.try_clone() else { return };
    let mut reader = std::io::BufReader::new(clone);
    let mut first = String::new();
    if reader.read_line(&mut first).unwrap_or(0) == 0 {
        return;
    }
    let mut len = 0usize;
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h).unwrap_or(0) == 0 || h.trim().is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':')
            && k.eq_ignore_ascii_case("content-length")
        {
            len = v.trim().parse().unwrap_or(0);
        }
    }
    let reply = |conn: &mut std::net::TcpStream, status: &str, body: &str| {
        let _ = write!(
            conn,
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
    };
    if !first.starts_with("POST /mcp") {
        reply(&mut conn, "404 Not Found", "{}");
        return;
    }
    let mut body = vec![0u8; len];
    if reader.read_exact(&mut body).is_err() {
        return;
    }
    match serde_json::from_slice::<Value>(&body) {
        Ok(msg) => match handle(&msg) {
            Some(r) => reply(&mut conn, "200 OK", &r.to_string()),
            None => reply(&mut conn, "202 Accepted", ""),
        },
        Err(e) => reply(
            &mut conn,
            "400 Bad Request",
            &error(&Value::Null, -32700, &format!("parse error: {e}")).to_string(),
        ),
    }
}

fn error(id: &Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

fn handle(msg: &Value) -> Option<Value> {
    let id = msg.get("id")?.clone(); // a notification has no id: no reply
    let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
    let result = match method {
        "initialize" => json!({
            "protocolVersion": PROTOCOL,
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "dharmatiles", "version": env!("CARGO_PKG_VERSION")},
        }),
        "ping" => json!({}),
        "tools/list" => json!({"tools": tools()}),
        "tools/call" => {
            let p = msg.get("params").cloned().unwrap_or(Value::Null);
            call(
                p.get("name").and_then(Value::as_str).unwrap_or(""),
                p.get("arguments").unwrap_or(&Value::Null),
            )
        }
        _ => return Some(error(&id, -32601, &format!("no method `{method}`"))),
    };
    Some(json!({"jsonrpc": "2.0", "id": id, "result": result}))
}

fn tools() -> Value {
    let path = json!({"type": "string", "description": "A tile document: a file, or `lib:tiles/…` in the bundled library"});
    let system = json!({"type": "string", "enum": ["db", "ol"], "description": "Connector system: db (DungeonBlocks) or ol (OpenLOCK)"});
    let seed = json!({"type": "integer", "description": "Replace the tile's seed"});
    json!([
        {"name": "check_document", "description": "Read and plan a tile document without building geometry; returns its errors (with file, line and help) or its stack.",
         "inputSchema": {"type": "object", "properties": {"path": path}, "required": ["path"]}},
        {"name": "list_elements", "description": "The elements a stack can hold: ground, dirt, rock, scatter, water, grass, reeds, wall, floor, tree, cliff.",
         "inputSchema": {"type": "object", "properties": {}}},
        {"name": "describe_element", "description": "One element's controls with units, ranges and defaults, and which this build takes.",
         "inputSchema": {"type": "object", "properties": {"name": {"type": "string"}}, "required": ["name"]}},
        {"name": "list_tiles", "description": "The library tiles that ship with dharmatiles (`lib:tiles/…` paths).",
         "inputSchema": {"type": "object", "properties": {}}},
        {"name": "build_tile", "description": "Build a tile to STL through the printability gate; returns the audit (watertight, overhang mm², dropped parts), timings and the files written.",
         "inputSchema": {"type": "object", "properties": {"path": path, "system": system, "seed": seed, "out": {"type": "string", "description": "Output folder (default out/)"}, "threemf": {"type": "boolean", "description": "Also write a 3MF with materials"}, "palette": {"type": "string", "description": "A palette document (material \"#rrggbb\" lines) to paint the 3MF with"}}, "required": ["path"]}},
        {"name": "render_preview", "description": "Build a tile and return its painted preview (overhang faces red).",
         "inputSchema": {"type": "object", "properties": {"path": path, "system": system, "seed": seed, "view": {"type": "string", "enum": ["painted", "top", "low", "close"], "description": "Which of the four views (default painted)"}}, "required": ["path"]}},
        {"name": "audit_mesh", "description": "Run the printability gate on a binary STL file: watertight, bodies, overhang over 65 degrees (the 55-65 band is reported apart), see-through holes.",
         "inputSchema": {"type": "object", "properties": {"path": {"type": "string", "description": "An STL file"}, "width_mm": {"type": "number"}, "height_mm": {"type": "number"}}, "required": ["path"]}},
    ])
}

fn text(v: &str, is_error: bool) -> Value {
    json!({"content": [{"type": "text", "text": v}], "isError": is_error})
}

/// Run this program with `args`; its stdout (the `--json` report) is the answer.
fn run_self(args: &[String]) -> Value {
    let Ok(exe) = std::env::current_exe() else {
        return text("cannot find the dharmatiles program", true);
    };
    match Command::new(exe).args(args).output() {
        Ok(o) => {
            let out = String::from_utf8_lossy(&o.stdout).into_owned();
            let err = String::from_utf8_lossy(&o.stderr).into_owned();
            let body = if out.trim().is_empty() { err } else { out };
            text(&body, !o.status.success() && o.status.code() != Some(3))
        }
        Err(e) => text(&format!("cannot run dharmatiles: {e}"), true),
    }
}

fn arg<'a>(a: &'a Value, k: &str) -> Option<&'a str> {
    a.get(k).and_then(Value::as_str)
}

fn call(name: &str, a: &Value) -> Value {
    let s = |x: &str| x.to_owned();
    let mut opts = Vec::new();
    if let Some(sys) = arg(a, "system") {
        opts.extend([s("--system"), s(sys)]);
    }
    if let Some(seed) = a.get("seed").and_then(Value::as_u64) {
        opts.extend([s("--seed"), seed.to_string()]);
    }
    match (name, arg(a, "path"), arg(a, "name")) {
        ("check_document", Some(p), _) => run_self(&[s("check"), s(p), s("--json")]),
        ("list_elements", _, _) => run_self(&[s("elements"), s("--json")]),
        ("describe_element", _, Some(n)) => run_self(&[s("elements"), s(n), s("--json")]),
        ("list_tiles", _, _) => text(&json!({"tiles": crate::library_tiles()}).to_string(), false),
        ("build_tile", Some(p), _) => {
            let mut args = vec![s("build"), s(p), s("--json")];
            args.extend(opts);
            if let Some(o) = arg(a, "out") {
                args.extend([s("--out"), s(o)]);
            }
            if a.get("threemf").and_then(Value::as_bool) == Some(true) {
                args.push(s("--3mf"));
            }
            if let Some(p) = arg(a, "palette") {
                args.extend([s("--palette"), s(p)]);
            }
            run_self(&args)
        }
        ("render_preview", Some(p), _) => preview(p, &opts, arg(a, "view").unwrap_or("painted")),
        ("audit_mesh", Some(p), _) => {
            let mut args = vec![s("audit"), s(p), s("--json")];
            for (k, f) in [("width_mm", "--width"), ("height_mm", "--height")] {
                if let Some(v) = a.get(k).and_then(Value::as_f64) {
                    args.extend([s(f), v.to_string()]);
                }
            }
            run_self(&args)
        }
        (n, _, _) => text(
            &format!("`{n}` needs its arguments (see tools/list), or is not a tool"),
            true,
        ),
    }
}

fn preview(path: &str, opts: &[String], view: &str) -> Value {
    let dir = std::env::temp_dir().join(format!("dharmatiles-mcp-{}", std::process::id()));
    let mut args = vec![
        "build".to_owned(),
        path.to_owned(),
        "--png".to_owned(),
        "--out".to_owned(),
        dir.display().to_string(),
    ];
    args.extend_from_slice(opts);
    let report = run_self(&args);
    let want = match view {
        "top" => "-top.png",
        "low" => "-low.png",
        "close" => "-close.png",
        _ => "-painted.png",
    };
    let found = walk(&dir)
        .into_iter()
        .find(|p| p.to_string_lossy().ends_with(want));
    let Some(png) = found.and_then(|p| std::fs::read(p).ok()) else {
        let _ = std::fs::remove_dir_all(&dir);
        return report;
    };
    let _ = std::fs::remove_dir_all(&dir);
    json!({"content": [
        {"type": "image", "data": base64(&png), "mimeType": "image/png"},
        {"type": "text", "text": report["content"][0]["text"]},
    ], "isError": false})
}

fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(walk(&p));
            } else {
                out.push(p);
            }
        }
    }
    out
}

/// Standard base64.
fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= c.len() {
                s.push(char::from(T[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                s.push('=');
            }
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_standard_examples() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn initialize_and_list_tools() {
        let r = handle(&json!({"jsonrpc": "2.0", "id": 1, "method": "initialize"})).unwrap();
        assert_eq!(r["result"]["protocolVersion"], PROTOCOL);
        let t = handle(&json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})).unwrap();
        let names: Vec<&str> = t["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|x| x["name"].as_str())
            .collect();
        for n in [
            "check_document",
            "list_elements",
            "describe_element",
            "list_tiles",
            "build_tile",
            "render_preview",
            "audit_mesh",
        ] {
            assert!(names.contains(&n), "{n}");
        }
        // a notification gets no reply
        assert!(
            handle(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"})).is_none()
        );
    }
}
