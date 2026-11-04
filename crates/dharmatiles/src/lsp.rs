//! `dharmatiles lsp`: a language server for tile documents over stdio.
//!
//! Diagnostics are the evaluator's own (`check`), so an editor shows what
//! `dharmatiles check` prints, at the text it points to. Hover explains a
//! look or a control from the schema; completion offers the looks, and a
//! look's controls after its name. Full-text sync; one document at a time
//! is held open per URI, and a document that `use`s files reads them from
//! disk.

use std::collections::HashMap;
use std::io::{BufRead, Read, Write};

use dt_doc::looks::LOOKS;
use dt_doc::schema;
use dt_eval::{Files, Options, Source};
use serde_json::{Value, json};

/// The open buffers over the disk.
struct Overlay<'a>(&'a HashMap<String, String>);

impl Source for Overlay<'_> {
    fn read(&self, path: &str) -> Result<String, String> {
        match self.0.get(path) {
            Some(t) => Ok(t.clone()),
            None => Files.read(path),
        }
    }
}

fn uri_to_path(uri: &str) -> String {
    let p = uri.strip_prefix("file://").unwrap_or(uri);
    let mut out = Vec::new();
    let b = p.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let Ok(v) = u8::from_str_radix(&p[i + 1..i + 3], 16)
        {
            out.push(v);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// (line, character) of a byte offset, 0-based.
fn position(text: &str, off: usize) -> (usize, usize) {
    let off = off.min(text.len());
    let before = &text[..off];
    let line = before.matches('\n').count();
    let start = before.rfind('\n').map_or(0, |i| i + 1);
    (line, text[start..off].chars().count())
}

/// The byte offset of (line, character).
fn offset(text: &str, line: usize, ch: usize) -> usize {
    let mut at = 0;
    for (n, l) in text.split_inclusive('\n').enumerate() {
        if n == line {
            return at + l.char_indices().nth(ch).map_or(l.len(), |(i, _)| i);
        }
        at += l.len();
    }
    text.len()
}

fn diagnostics(path: &str, docs: &HashMap<String, String>) -> Vec<Value> {
    let text = docs.get(path).cloned().unwrap_or_default();
    let errs = match dt_eval::check(&Overlay(docs), path, &Options::default()) {
        Ok(c) => c.pending().to_vec(),
        Err(e) => e,
    };
    errs.iter()
        .filter(|d| d.location().is_some_and(|(f, _, _)| f == path))
        .map(|d| {
            let (off, len) = d
                .at
                .as_ref()
                .map_or((0, 0), |(_, s)| (s.offset(), s.len().max(1)));
            let (l0, c0) = position(&text, off);
            let (l1, c1) = position(&text, off + len);
            let mut message = d.message.clone();
            if let Some(h) = &d.help {
                message = format!("{message}\n{h}");
            }
            json!({
                "range": {"start": {"line": l0, "character": c0}, "end": {"line": l1, "character": c1}},
                "severity": if d.code == "dt::not-yet" { 2 } else { 1 },
                "code": d.code, "source": "dharmatiles", "message": message,
            })
        })
        .collect()
}

fn word_at(text: &str, line: usize, ch: usize) -> Option<String> {
    let off = offset(text, line, ch);
    let is = |c: char| c.is_alphanumeric() || c == '-' || c == '_';
    let s = text[..off].rfind(|c: char| !is(c)).map_or(0, |i| i + 1);
    let e = text[off..]
        .find(|c: char| !is(c))
        .map_or(text.len(), |i| off + i);
    (e > s).then(|| text[s..e].to_owned())
}

fn hover(text: &str, line: usize, ch: usize) -> Value {
    let Some(w) = word_at(text, line, ch) else {
        return Value::Null;
    };
    let contents = if let Some(l) = schema::look(&w) {
        let controls: Vec<String> = l.controls.iter().map(|c| format!("`{}`", c.name)).collect();
        format!(
            "**{}**: {}\n\nControls: {}",
            l.name,
            l.about,
            controls.join(", ")
        )
    } else if let Some(c) = LOOKS
        .iter()
        .find_map(|l| l.controls.iter().find(|c| c.name == w))
    {
        let d = if c.default.is_empty() {
            String::new()
        } else {
            format!(" (default `{}`)", c.default)
        };
        format!("**{}**: {}{d}", c.name, c.about)
    } else {
        return Value::Null;
    };
    json!({"contents": {"kind": "markdown", "value": contents}})
}

fn completion(text: &str, line: usize, ch: usize) -> Value {
    let off = offset(text, line, ch);
    let ls = text[..off].rfind('\n').map_or(0, |i| i + 1);
    let before = text[ls..off].trim_start();
    let first = before.split_whitespace().next();
    let items: Vec<Value> = match first.and_then(schema::look) {
        Some(l) if before.contains(' ') => l
            .controls
            .iter()
            .map(|c| json!({"label": c.name, "kind": 10, "detail": c.about, "insertText": format!("{}=", c.name)}))
            .collect(),
        _ => LOOKS
            .iter()
            .map(|l| json!({"label": l.name, "kind": 3, "detail": l.about}))
            .collect(),
    };
    json!(items)
}

fn send(out: &mut impl Write, v: &Value) {
    let body = v.to_string();
    let _ = write!(out, "Content-Length: {}\r\n\r\n{body}", body.len());
    let _ = out.flush();
}

/// Serve until `exit`.
pub fn run() -> u8 {
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let mut out = std::io::stdout().lock();
    let mut docs: HashMap<String, String> = HashMap::new();
    loop {
        let mut len = 0usize;
        loop {
            let mut h = String::new();
            if input.read_line(&mut h).unwrap_or(0) == 0 {
                return 0;
            }
            let h = h.trim();
            if h.is_empty() {
                break;
            }
            if let Some(v) = h.strip_prefix("Content-Length:") {
                len = v.trim().parse().unwrap_or(0);
            }
        }
        let mut buf = vec![0u8; len];
        if input.read_exact(&mut buf).is_err() {
            return 0;
        }
        let Ok(msg) = serde_json::from_slice::<Value>(&buf) else {
            continue;
        };
        let method = msg["method"].as_str().unwrap_or("");
        let id = msg.get("id").cloned();
        let p = &msg["params"];
        let uri = p["textDocument"]["uri"].as_str().unwrap_or("").to_owned();
        let result = match method {
            "initialize" => Some(json!({
                "capabilities": {
                    "textDocumentSync": 1,
                    "hoverProvider": true,
                    "completionProvider": {"triggerCharacters": [" "]},
                },
                "serverInfo": {"name": "dharmatiles"},
            })),
            "shutdown" => Some(Value::Null),
            "exit" => return 0,
            "textDocument/didOpen" | "textDocument/didChange" => {
                let text = p["textDocument"]["text"]
                    .as_str()
                    .or_else(|| p["contentChanges"][0]["text"].as_str())
                    .unwrap_or("")
                    .to_owned();
                let path = uri_to_path(&uri);
                docs.insert(path.clone(), text);
                let d = diagnostics(&path, &docs);
                send(
                    &mut out,
                    &json!({"jsonrpc": "2.0", "method": "textDocument/publishDiagnostics",
                        "params": {"uri": uri, "diagnostics": d}}),
                );
                None
            }
            "textDocument/didClose" => {
                docs.remove(&uri_to_path(&uri));
                None
            }
            "textDocument/hover" | "textDocument/completion" => {
                let path = uri_to_path(&uri);
                let text = docs.get(&path).cloned().unwrap_or_default();
                let (l, c) = (
                    usize::try_from(p["position"]["line"].as_u64().unwrap_or(0)).unwrap_or(0),
                    usize::try_from(p["position"]["character"].as_u64().unwrap_or(0)).unwrap_or(0),
                );
                Some(if method.ends_with("hover") {
                    hover(&text, l, c)
                } else {
                    completion(&text, l, c)
                })
            }
            _ => id.as_ref().map(|_| Value::Null),
        };
        if let (Some(id), Some(r)) = (id, result) {
            send(&mut out, &json!({"jsonrpc": "2.0", "id": id, "result": r}));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_round_trip() {
        let t = "tile a size=\"1x1\" {\n  stack { grass }\n}\n";
        let off = t.find("grass").unwrap();
        let (l, c) = position(t, off);
        assert_eq!((l, c), (1, 10));
        assert_eq!(offset(t, l, c), off);
        assert_eq!(word_at(t, l, c + 2).as_deref(), Some("grass"));
    }

    #[test]
    fn a_bad_document_gets_a_diagnostic_at_its_mistake() {
        let text = "tile a size=\"1x1\" {\n  stack {\n    grasss\n  }\n}\n".to_owned();
        let mut docs = HashMap::new();
        docs.insert("/tmp/a.kdl".to_owned(), text);
        let d = diagnostics("/tmp/a.kdl", &docs);
        assert!(!d.is_empty());
        assert_eq!(d[0]["range"]["start"]["line"], 2);
    }

    #[test]
    fn hover_and_completion_come_from_the_schema() {
        let t = "grass ";
        assert!(
            hover(t, 0, 2)["contents"]["value"]
                .as_str()
                .unwrap()
                .contains("grass")
        );
        let c = completion(t, 0, 6);
        assert!(c.as_array().unwrap().iter().any(|i| i["label"] == "bare"));
    }
}
