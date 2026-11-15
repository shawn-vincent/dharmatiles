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
/// notification). Local use: it binds where it is told and has no token
/// (a configured client could not send one), so give it a loopback address;
/// it refuses a `Host` or `Origin` that is not the loopback, a request that
/// is not `application/json`, and a body over 4 MiB.
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

/// Is a `Content-Type` value `application/json` (parameters such as the charset allowed)?
fn is_json(ctype: &str) -> bool {
    ctype
        .split(';')
        .next()
        .is_some_and(|t| t.trim().eq_ignore_ascii_case("application/json"))
}

/// After an answer given before the request was read to its end (a refusal):
/// say no more, then read and drop what the client is still sending, so the
/// close does not reset the connection and lose the answer on its way.
fn finish_early(conn: &mut std::net::TcpStream) {
    use std::io::Read;
    // best effort: the answer is already written; a client that has gone needs nothing more
    if conn.shutdown(std::net::Shutdown::Write).is_err() {
        return;
    }
    if conn
        .set_read_timeout(Some(std::time::Duration::from_secs(2)))
        .is_err()
    {
        return;
    }
    let max = u64::try_from(crate::preview::http::MAX_BODY).unwrap_or(u64::MAX);
    // what is read is thrown away; a read error only ends the draining
    if std::io::copy(&mut conn.take(max), &mut std::io::sink()).is_err() {}
}

fn http_one(mut conn: std::net::TcpStream) {
    use std::io::Read;
    let reply = |conn: &mut std::net::TcpStream, status: &str, body: &str| {
        let sent = write!(
            conn,
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        if let Err(e) = sent {
            eprintln!("error: dharmatiles mcp: cannot reply ({status}): {e}");
        }
    };
    let bad = |conn: &mut std::net::TcpStream, why: String| {
        eprintln!("error: dharmatiles mcp: {why}");
        reply(
            conn,
            "400 Bad Request",
            &error(&Value::Null, -32600, &why).to_string(),
        );
        finish_early(conn);
    };
    let clone = match conn.try_clone() {
        Ok(c) => c,
        Err(e) => return eprintln!("error: dharmatiles mcp: cannot read a connection: {e}"),
    };
    let mut reader = std::io::BufReader::new(clone);
    let mut first = String::new();
    match reader.read_line(&mut first) {
        // the client connected and left without a word: nothing to answer
        Ok(0) => return,
        Ok(_) => {}
        Err(e) => return bad(&mut conn, format!("cannot read the request: {e}")),
    }
    let mut len = 0usize;
    let (mut host, mut origin, mut ctype) = (String::new(), None::<String>, String::new());
    loop {
        let mut h = String::new();
        match reader.read_line(&mut h) {
            Ok(0) => break,
            Ok(_) if h.trim().is_empty() => break,
            Ok(_) => {}
            Err(e) => return bad(&mut conn, format!("cannot read the request headers: {e}")),
        }
        if let Some((k, v)) = h.split_once(':') {
            let v = v.trim();
            if k.eq_ignore_ascii_case("content-length") {
                match v.parse() {
                    Ok(n) => len = n,
                    Err(e) => return bad(&mut conn, format!("bad Content-Length `{v}`: {e}")),
                }
            } else if k.eq_ignore_ascii_case("host") {
                v.clone_into(&mut host);
            } else if k.eq_ignore_ascii_case("origin") {
                origin = Some(v.to_owned());
            } else if k.eq_ignore_ascii_case("content-type") {
                v.clone_into(&mut ctype);
            }
        }
    }
    // A web page must not reach this server: not by DNS rebinding (a `Host`
    // that is not the loopback), not by a cross-site POST (browsers always
    // send an `Origin`; an MCP client sends none), not by a "simple" request
    // (`text/plain`, which needs no preflight).  No token: a configured
    // client could not send one.
    let refuse_as = |conn: &mut std::net::TcpStream, status: &str, why: &str| {
        eprintln!("error: dharmatiles mcp: refused: {why}");
        reply(conn, status, &error(&Value::Null, -32600, why).to_string());
        finish_early(conn);
    };
    let refuse = |conn: &mut std::net::TcpStream, why: &str| refuse_as(conn, "403 Forbidden", why);
    if !crate::preview::http::loopback(&host) {
        return refuse(
            &mut conn,
            "loopback only: the Host is not 127.0.0.1, localhost or ::1",
        );
    }
    if let Some(o) = &origin
        && !crate::preview::http::loopback_origin(o)
    {
        return refuse(
            &mut conn,
            "a web page of another site cannot call this server (its Origin is not the loopback)",
        );
    }
    if !is_json(&ctype) {
        return refuse(
            &mut conn,
            "the request must be Content-Type: application/json",
        );
    }
    if len > crate::preview::http::MAX_BODY {
        return refuse_as(
            &mut conn,
            "413 Payload Too Large",
            &format!(
                "the body is {len} bytes; the most read is {}",
                crate::preview::http::MAX_BODY
            ),
        );
    }
    if !first.starts_with("POST /mcp") {
        reply(&mut conn, "404 Not Found", "{}");
        finish_early(&mut conn);
        return;
    }
    let mut body = vec![0u8; len];
    if let Err(e) = reader.read_exact(&mut body) {
        return bad(
            &mut conn,
            format!("the body is shorter than its Content-Length: {e}"),
        );
    }
    match serde_json::from_slice::<Value>(&body) {
        Ok(msg) => {
            // only to address an error reply if the call panics: a message that is no request gets its own
            // error from `handle`
            let id = serde_json::from_value::<Request>(msg.clone())
                .ok()
                .and_then(|r| r.id)
                .unwrap_or(Value::Null);
            match std::panic::catch_unwind(|| handle(&msg)) {
                Ok(Some(r)) => reply(&mut conn, "200 OK", &r.to_string()),
                Ok(None) => reply(&mut conn, "202 Accepted", ""),
                Err(panic) => {
                    let why = format!(
                        "the call panicked: {}",
                        crate::preview::panic_message(panic.as_ref())
                    );
                    eprintln!("error: dharmatiles mcp: {why}");
                    reply(
                        &mut conn,
                        "500 Internal Server Error",
                        &error(&id, -32603, &why).to_string(),
                    );
                }
            }
        }
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

/// A JSON-RPC message: the envelope, read as a type.
#[derive(serde::Deserialize)]
struct Request {
    /// None for a notification (no `id` at all, which is not the same as `"id": null`).
    #[serde(default, deserialize_with = "present")]
    id: Option<Value>,
    #[serde(default)]
    method: String,
    #[serde(default)]
    params: Value,
}

/// The params of `tools/call`.
#[derive(serde::Deserialize)]
struct CallParams {
    name: String,
    #[serde(default)]
    arguments: Value,
}

/// A field that is there, `null` or not, is `Some`.
fn present<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Value>, D::Error> {
    <Value as serde::Deserialize>::deserialize(d).map(Some)
}

fn handle(msg: &Value) -> Option<Value> {
    let req: Request = match serde_json::from_value(msg.clone()) {
        Ok(r) => r,
        Err(e) => {
            return Some(error(
                &Value::Null,
                -32600,
                &format!("not a JSON-RPC request: {e}"),
            ));
        }
    };
    let id = req.id?; // a notification has no id: no reply
    let method = req.method.as_str();
    let result = match method {
        "initialize" => json!({
            "protocolVersion": PROTOCOL,
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "dharmatiles", "version": env!("CARGO_PKG_VERSION")},
        }),
        "ping" => json!({}),
        "tools/list" => json!({"tools": tools()}),
        "tools/call" => match serde_json::from_value::<CallParams>(req.params) {
            Ok(p) => call(&p.name, &p.arguments),
            Err(e) => {
                return Some(error(
                    &id,
                    -32602,
                    &format!("tools/call needs params {{name, arguments}}: {e}"),
                ));
            }
        },
        _ => return Some(error(&id, -32601, &format!("no method `{method}`"))),
    };
    Some(json!({"jsonrpc": "2.0", "id": id, "result": result}))
}

/// What a Rust type is as a JSON-schema property, and whether leaving it out is allowed: the one place a tool's
/// argument type becomes its `inputSchema`, so the schema is read off the struct the call is deserialized into.
trait Param {
    /// A value of this type is required (an `Option` is not).
    const REQUIRED: bool = true;
    fn schema() -> Value;
}

impl Param for String {
    fn schema() -> Value {
        json!({"type": "string"})
    }
}

impl Param for u64 {
    fn schema() -> Value {
        json!({"type": "integer", "minimum": 0})
    }
}

impl Param for f64 {
    fn schema() -> Value {
        json!({"type": "number"})
    }
}

impl Param for bool {
    fn schema() -> Value {
        json!({"type": "boolean"})
    }
}

impl<T: Param> Param for Option<T> {
    const REQUIRED: bool = false;
    fn schema() -> Value {
        T::schema()
    }
}

/// A tool's arguments: the struct a call is deserialized into (an argument the tool does not take is an error, never
/// ignored) and, from the same field list, its `inputSchema`.  Each field is `name: Type = "what it is"`.
macro_rules! args {
    ($(#[$m:meta])* $name:ident { $($field:ident : $ty:ty = $about:expr),* $(,)? }) => {
        $(#[$m])*
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct $name {
            $($field: $ty),*
        }

        impl $name {
            /// The `inputSchema` of the tool that takes these.
            fn schema() -> Value {
                #[allow(unused_mut)]
                let mut properties = serde_json::Map::new();
                #[allow(unused_mut)]
                let mut required: Vec<&str> = Vec::new();
                $(
                    let mut p = <$ty as Param>::schema();
                    p["description"] = json!($about);
                    properties.insert(stringify!($field).to_owned(), p);
                    if <$ty as Param>::REQUIRED {
                        required.push(stringify!($field));
                    }
                )*
                let mut schema = json!({"type": "object", "properties": properties});
                if !required.is_empty() {
                    schema["required"] = json!(required);
                }
                schema
            }
        }
    };
}

/// A string argument that is one of a few words: the words are its schema's `enum`.
macro_rules! words {
    ($(#[$m:meta])* $name:ident { $($variant:ident = $word:literal),* $(,)? }) => {
        $(#[$m])*
        #[derive(serde::Deserialize, Clone, Copy)]
        enum $name {
            $(#[serde(rename = $word)] $variant),*
        }

        impl $name {
            fn word(self) -> &'static str {
                match self { $(Self::$variant => $word),* }
            }
        }

        impl Param for $name {
            fn schema() -> Value {
                json!({"type": "string", "enum": [$($word),*]})
            }
        }
    };
}

words! {
    /// The build target.
    System { Db = "db", Ol = "ol" }
}

words! {
    /// One of the four views of a preview.
    View { Painted = "painted", Top = "top", Low = "low", Close = "close" }
}

const PATH: &str = "A tile document: a file, or `lib:tiles/…` in the bundled library";
const SYSTEM: &str = "Base system: db (DungeonBlocks) or ol (OpenLOCK)";
const PRINTER: &str =
    "Printer profile name (library/printers/ or your own printers/ folder; default: default)";
const SEED: &str = "Replace the tile's seed";

args! {
    /// `check_document`.
    CheckArgs { path: String = PATH }
}
args! {
    /// `list_elements` and `list_tiles`: nothing.
    NoArgs {}
}
args! {
    /// `describe_element`.
    DescribeArgs { name: String = "An element's name" }
}
args! {
    /// `build_tile`.
    BuildArgs {
        path: String = PATH,
        system: Option<System> = SYSTEM,
        printer: Option<String> = PRINTER,
        seed: Option<u64> = SEED,
        out: Option<String> = "Output folder (default out/)",
        threemf: Option<bool> = "Also write a 3MF with materials",
        palette: Option<String> = "A palette document (material \"#rrggbb\" lines) to paint the 3MF with",
    }
}
args! {
    /// `render_preview`.
    PreviewArgs {
        path: String = PATH,
        system: Option<System> = SYSTEM,
        printer: Option<String> = PRINTER,
        seed: Option<u64> = SEED,
        view: Option<View> = "Which of the four views (default painted)",
    }
}
args! {
    /// `audit_mesh`.
    AuditArgs {
        path: String = "An STL file",
        width_mm: Option<f64> = "The tile's width, mm",
        height_mm: Option<f64> = "The tile's height, mm",
    }
}

/// One tool: its name and words, the schema of its arguments and what it runs.  The tools are this table and nothing
/// else, so `tools/list` and `tools/call` cannot disagree about a tool.
struct Tool {
    name: &'static str,
    about: &'static str,
    schema: fn() -> Value,
    run: fn(&Value) -> Value,
}

/// Read a call's arguments as `A` and run `f` on them; arguments that are not `A` are a tool error that says why.
fn with<A: serde::de::DeserializeOwned>(
    tool: &str,
    a: &Value,
    f: impl FnOnce(A) -> Value,
) -> Value {
    // a client may leave `arguments` out of a tool that takes none
    let given = if a.is_null() { json!({}) } else { a.clone() };
    match serde_json::from_value::<A>(given) {
        Ok(args) => f(args),
        Err(e) => text(&format!("`{tool}`: the arguments are not valid: {e}"), true),
    }
}

/// The options every build-like tool takes, as command-line flags.
fn flags(system: Option<System>, printer: Option<&String>, seed: Option<u64>) -> Vec<String> {
    let mut opts = Vec::new();
    if let Some(sys) = system {
        opts.extend(["--system".to_owned(), sys.word().to_owned()]);
    }
    if let Some(p) = printer {
        opts.extend(["--printer".to_owned(), p.clone()]);
    }
    if let Some(seed) = seed {
        opts.extend(["--seed".to_owned(), seed.to_string()]);
    }
    opts
}

const TOOLS: [Tool; 7] = [
    Tool {
        name: "check_document",
        about: "Read and plan a tile document without building geometry; returns its errors (with file, line and help) or its stack.",
        schema: CheckArgs::schema,
        run: |a| {
            with("check_document", a, |a: CheckArgs| {
                run_self(&["check".to_owned(), a.path, "--json".to_owned()])
            })
        },
    },
    Tool {
        name: "list_elements",
        about: "The elements a stack can hold: ground, dirt, rock, scatter, water, grass, reeds, wall, floor, tree, cliff.",
        schema: NoArgs::schema,
        run: |a| {
            with("list_elements", a, |_: NoArgs| {
                run_self(&["elements".to_owned(), "--json".to_owned()])
            })
        },
    },
    Tool {
        name: "describe_element",
        about: "One element's controls with units, ranges and defaults, and which this build takes.",
        schema: DescribeArgs::schema,
        run: |a| {
            with("describe_element", a, |a: DescribeArgs| {
                run_self(&["elements".to_owned(), a.name, "--json".to_owned()])
            })
        },
    },
    Tool {
        name: "list_tiles",
        about: "The library tiles that ship with dharmatiles (`lib:tiles/…` paths).",
        schema: NoArgs::schema,
        run: |a| {
            with("list_tiles", a, |_: NoArgs| match crate::library_tiles() {
                Ok(tiles) => text(&json!({"tiles": tiles}).to_string(), false),
                Err(e) => text(&format!("cannot list the library: {e}"), true),
            })
        },
    },
    Tool {
        name: "build_tile",
        about: "Build a tile to STL through the printability gate; returns the audit (watertight, overhang mm², dropped parts), timings and the files written.",
        schema: BuildArgs::schema,
        run: |a| {
            with("build_tile", a, |a: BuildArgs| {
                let mut args = vec!["build".to_owned(), a.path, "--json".to_owned()];
                args.extend(flags(a.system, a.printer.as_ref(), a.seed));
                if let Some(o) = a.out {
                    args.extend(["--out".to_owned(), o]);
                }
                if a.threemf == Some(true) {
                    args.push("--3mf".to_owned());
                }
                if let Some(p) = a.palette {
                    args.extend(["--palette".to_owned(), p]);
                }
                run_self(&args)
            })
        },
    },
    Tool {
        name: "render_preview",
        about: "Build a tile and return its painted preview (overhang faces red).",
        schema: PreviewArgs::schema,
        run: |a| {
            with("render_preview", a, |a: PreviewArgs| {
                let opts = flags(a.system, a.printer.as_ref(), a.seed);
                preview(&a.path, &opts, a.view.unwrap_or(View::Painted))
            })
        },
    },
    Tool {
        name: "audit_mesh",
        about: "Run the printability gate on a binary STL file: watertight, bodies, overhang over 65 degrees (the 55-65 band is reported apart), see-through holes.",
        schema: AuditArgs::schema,
        run: |a| {
            with("audit_mesh", a, |a: AuditArgs| {
                let mut args = vec!["audit".to_owned(), a.path, "--json".to_owned()];
                for (v, f) in [(a.width_mm, "--width"), (a.height_mm, "--height")] {
                    if let Some(v) = v {
                        args.extend([f.to_owned(), v.to_string()]);
                    }
                }
                run_self(&args)
            })
        },
    },
];

fn tools() -> Value {
    Value::Array(
        TOOLS
            .iter()
            .map(|t| json!({"name": t.name, "description": t.about, "inputSchema": (t.schema)()}))
            .collect(),
    )
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

/// Run the tool `name` on its arguments.
fn call(name: &str, a: &Value) -> Value {
    match TOOLS.iter().find(|t| t.name == name) {
        Some(t) => (t.run)(a),
        None => text(&format!("`{name}` is not a tool (see tools/list)"), true),
    }
}

fn preview(path: &str, opts: &[String], view: View) -> Value {
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
    // the build writes `<tile>-<view>.png` for each of the four views, named by the word the schema offers
    let want = format!("-{}.png", view.word());
    let found = match walk(&dir) {
        Ok(files) => files
            .into_iter()
            .find(|p| p.to_string_lossy().ends_with(&want)),
        // nothing was written: the build failed, and `report` says why
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return report,
        Err(e) => {
            return text(
                &format!("built, but the preview images could not be listed: {e}"),
                true,
            );
        }
    };
    let png = match found.map(std::fs::read) {
        Some(Ok(png)) => png,
        Some(Err(e)) => {
            return text(
                &format!("built, but the preview image could not be read: {e}"),
                true,
            );
        }
        // no image: the build failed, and `report` says why
        None => {
            // best-effort cleanup of the scratch folder
            let _ = std::fs::remove_dir_all(&dir);
            return report;
        }
    };
    // best-effort cleanup of the scratch folder
    let _ = std::fs::remove_dir_all(&dir);
    json!({"content": [
        {"type": "image", "data": base64(&png), "mimeType": "image/png"},
        {"type": "text", "text": report["content"][0]["text"]},
    ], "isError": false})
}

fn walk(dir: &std::path::Path) -> std::io::Result<Vec<std::path::PathBuf>> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir)? {
        let p = e?.path();
        if p.is_dir() {
            out.extend(walk(&p)?);
        } else {
            out.push(p);
        }
    }
    Ok(out)
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

    #[test]
    fn a_tools_schema_is_its_argument_structs() {
        let build = BuildArgs::schema();
        assert_eq!(build["required"], json!(["path"]));
        assert_eq!(build["properties"]["system"]["enum"], json!(["db", "ol"]));
        assert_eq!(build["properties"]["seed"]["type"], "integer");
        assert_eq!(build["properties"]["threemf"]["type"], "boolean");
        assert_eq!(
            NoArgs::schema(),
            json!({"type": "object", "properties": {}})
        );
        // every tool in the table is listed, with the schema its struct makes
        let listed = tools();
        assert_eq!(listed.as_array().unwrap().len(), TOOLS.len());
    }

    #[test]
    fn an_argument_a_tool_does_not_take_is_refused() {
        let r = super::call("check_document", &json!({"path": "x", "colour": "red"}));
        assert_eq!(r["isError"], true);
        assert!(r["content"][0]["text"].as_str().unwrap().contains("colour"));
        let r = super::call("build_tile", &json!({"path": "x", "system": "xx"}));
        assert_eq!(r["isError"], true);
        let r = super::call("no_such_tool", &json!({}));
        assert_eq!(r["isError"], true);
    }

    /// One request to a real `http_one`; the status line and the body come back.
    fn call(head: &str, body: &str) -> String {
        use std::io::Read;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || http_one(listener.accept().unwrap().0));
        let mut c = std::net::TcpStream::connect(addr).unwrap();
        write!(c, "{head}\r\n{body}").unwrap();
        let mut all = String::new();
        c.read_to_string(&mut all).unwrap();
        // a client that has its answer goes, as a real one does
        drop(c);
        server.join().unwrap();
        all
    }

    const PING: &str = r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;

    fn post(extra: &str, body: &str) -> String {
        call(
            &format!(
                "POST /mcp HTTP/1.1\r\nContent-Length: {}\r\n{extra}",
                body.len()
            ),
            body,
        )
    }

    #[test]
    fn http_answers_a_loopback_json_client() {
        let r = post(
            "Host: 127.0.0.1:8765\r\nContent-Type: application/json; charset=utf-8\r\n",
            PING,
        );
        assert!(r.starts_with("HTTP/1.1 200"), "{r}");
        let r = post(
            "Host: localhost:1\r\nContent-Type: application/json\r\nOrigin: http://localhost:3000\r\n",
            PING,
        );
        assert!(r.starts_with("HTTP/1.1 200"), "{r}");
    }

    #[test]
    fn http_refuses_a_host_that_is_not_the_loopback() {
        let r = post(
            "Host: evil.example:8765\r\nContent-Type: application/json\r\n",
            PING,
        );
        assert!(r.starts_with("HTTP/1.1 403"), "{r}");
        let r = post("Content-Type: application/json\r\n", PING);
        assert!(r.starts_with("HTTP/1.1 403"), "{r}");
    }

    #[test]
    fn http_refuses_a_web_pages_origin() {
        let r = post(
            "Host: 127.0.0.1:1\r\nOrigin: https://evil.example\r\nContent-Type: application/json\r\n",
            PING,
        );
        assert!(r.starts_with("HTTP/1.1 403"), "{r}");
        let r = post(
            "Host: 127.0.0.1:1\r\nOrigin: null\r\nContent-Type: application/json\r\n",
            PING,
        );
        assert!(r.starts_with("HTTP/1.1 403"), "{r}");
    }

    #[test]
    fn http_refuses_a_simple_request_that_is_not_json() {
        let r = post("Host: 127.0.0.1:1\r\nContent-Type: text/plain\r\n", PING);
        assert!(r.starts_with("HTTP/1.1 403"), "{r}");
        let r = post("Host: 127.0.0.1:1\r\n", PING);
        assert!(r.starts_with("HTTP/1.1 403"), "{r}");
    }

    #[test]
    fn http_refuses_a_body_over_the_cap_before_reading_it() {
        // the head names a body of 5 MiB and sends none: refused on the number alone
        let r = call(
            "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:1\r\nContent-Type: application/json\r\nContent-Length: 5242880\r\n",
            "",
        );
        assert!(r.starts_with("HTTP/1.1 413"), "{r}");
    }
}
