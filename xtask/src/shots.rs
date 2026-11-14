//! `cargo xtask shots`: redraw the pictures of the editor the docs show.
//!
//! The editor runs for real: this builds `dharmatiles`, starts `dharmatiles
//! preview` on a free port with `DT_HOME`, `DT_CACHE` and `HOME` pointing into
//! `out/shots/` (so nothing of yours is read or written: no tiles, no cache, no
//! settings), starts a headless Chrome on a free debugging port, and hands both
//! to `xtask/shots/shots.mjs`, which walks the editor through each documented
//! state and writes `docs/assets/shots/<name>-<theme>.webp` and
//! `shots.json` (where each callout's part of the page is, in pixels, read
//! from the live page). The docs draw their numbered callouts over the
//! pictures from `shots.json`, so a redraw moves them with the editor.
//!
//! Both programs are killed when this returns, however it returns.

use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Where the pictures live under `docs/`.
pub const DIR: &str = "docs/assets/shots";

/// A child process that is killed (and reaped) when it goes out of scope.
struct Guard(Child, &'static str);

impl Drop for Guard {
    fn drop(&mut self) {
        // best-effort cleanup of our own child at exit: it may already be gone
        let _ = self.0.kill();
        let _ = self.0.wait();
        eprintln!("shots: stopped {}", self.1);
    }
}

/// `cargo xtask shots [--dev] [--only name …]`.
pub fn run(root: &Path, args: &[String]) -> Result<(), String> {
    let started = Instant::now();
    let dev = args.iter().any(|a| a == "--dev");
    let only: Vec<&String> = match args.iter().position(|a| a == "--only") {
        Some(i) => args[i + 1..]
            .iter()
            .take_while(|a| !a.starts_with("--"))
            .collect(),
        None => Vec::new(),
    };
    let chrome = find_chrome()?;
    let node = find_node()?;
    let exe = crate::program(root, !dev)?;

    let scratch = root.join("out").join("shots");
    match std::fs::remove_dir_all(&scratch) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("cannot clear {}: {e}", scratch.display())),
    }
    let home = scratch.join("home");
    let cache = scratch.join("cache");
    let user = scratch.join("user");
    let profile = scratch.join("chrome-profile");
    for d in [&home, &cache, &user, &profile] {
        std::fs::create_dir_all(d).map_err(|e| format!("cannot make {}: {e}", d.display()))?;
    }
    let env = Env {
        home: &home,
        cache: &cache,
        user: &user,
    };
    seed(root, &exe, &env)?;

    let out = root.join(DIR);
    std::fs::create_dir_all(&out).map_err(|e| format!("cannot make {}: {e}", out.display()))?;

    let port = free_port()?;
    let log = scratch.join("preview.log");
    let logfile =
        std::fs::File::create(&log).map_err(|e| format!("cannot write {}: {e}", log.display()))?;
    let mut server = env.apply(Command::new(&exe), root);
    server
        .args(["preview", "--port", &port.to_string()])
        .stdout(
            logfile
                .try_clone()
                .map_err(|e| format!("cannot share the log: {e}"))?,
        )
        .stderr(logfile);
    let server = Guard(
        server
            .spawn()
            .map_err(|e| format!("cannot start {}: {e}", exe.display()))?,
        "the editor",
    );
    wait_for_port(port, Duration::from_secs(60), &log)?;
    eprintln!(
        "shots: the editor is on 127.0.0.1:{port} (home {})",
        home.display()
    );

    let cdp = free_port()?;
    let mut browser = Command::new(&chrome);
    browser
        .args([
            "--headless=new",
            &format!("--remote-debugging-port={cdp}"),
            &format!("--user-data-dir={}", profile.display()),
            "--window-size=1512,860",
            "--hide-scrollbars",
            "--no-first-run",
            "--no-default-browser-check",
            "--use-gl=angle",
            "--use-angle=swiftshader",
            "--enable-unsafe-swiftshader",
            "about:blank",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let browser = Guard(
        browser
            .spawn()
            .map_err(|e| format!("cannot start {}: {e}", chrome.display()))?,
        "Chrome",
    );
    wait_for_port(
        cdp,
        Duration::from_secs(30),
        Path::new("(Chrome prints nothing)"),
    )?;

    let script = root.join("xtask/shots/shots.mjs");
    let mut node = Command::new(&node);
    node.arg(&script)
        .args(["--editor", &format!("http://127.0.0.1:{port}/")])
        .args(["--chrome", &cdp.to_string()])
        .arg("--out")
        .arg(&out);
    if !only.is_empty() {
        node.arg("--only").args(&only);
    }
    let result = crate::run(&mut node);
    drop(browser);
    drop(server);
    result.map_err(|e| format!("{e}\nthe editor's log: {}", log.display()))?;
    println!(
        "shots: {} written in {:.0} s",
        out.display(),
        started.elapsed().as_secs_f64()
    );
    Ok(())
}

/// The environment a child sees: nothing of the user's own.
struct Env<'a> {
    home: &'a Path,
    cache: &'a Path,
    user: &'a Path,
}

impl Env<'_> {
    fn apply(&self, mut cmd: Command, cwd: &Path) -> Command {
        cmd.current_dir(cwd)
            .env("DT_HOME", self.home)
            .env("DT_CACHE", self.cache)
            .env("HOME", self.user)
            .env("XDG_CACHE_HOME", self.cache)
            .env("XDG_CONFIG_HOME", self.user.join(".config"))
            .env("XDG_DATA_HOME", self.user.join(".local"));
        cmd
    }
}

/// The scratch "My tiles": copies of library tiles in two folders, a map, a
/// print list, and an inventory made by the program's own command.
fn seed(root: &Path, exe: &Path, env: &Env) -> Result<(), String> {
    let lib = root.join("library/tiles");
    let copies = [
        ("crossing", "ground/2x2-glade", "glade"),
        ("crossing", "ground/1x1-grass", "grass"),
        ("crossing", "water/1x1-water-corridor+grass", "stream"),
        ("crossing", "water/1x1-water+grass", "pool"),
        ("crossing", "walls/1x1-wall-fieldstone", "wall"),
        ("meadows", "ground/1x1-grass-flowers", "flowers"),
        ("meadows", "ground/2x1-tree-row", "tree-row"),
    ];
    for (folder, from, name) in copies {
        let dir = env.home.join(folder);
        std::fs::create_dir_all(&dir).map_err(|e| format!("cannot make {}: {e}", dir.display()))?;
        let src = lib.join(format!("{from}.dtile"));
        std::fs::copy(&src, dir.join(format!("{name}.dtile")))
            .map_err(|e| format!("cannot copy {}: {e}", src.display()))?;
    }
    let map = r#"dharmatiles version="1.0.0"

map {
    about "A stream crossing."
    grid cols=4 rows=3
    piece p1 "home:crossing/glade"  at="(0sq, 0sq)" rot=0
    piece p2 "home:crossing/stream" at="(2sq, 0sq)" rot=0
    piece p3 "home:crossing/stream" at="(2sq, 1sq)" rot=0 seed=3
    piece p4 "home:crossing/grass"  at="(3sq, 0sq)" rot=0
    piece p5 "home:crossing/wall"   at="(3sq, 1sq)" rot=90
    piece p6 "home:crossing/grass"  at="(0sq, 2sq)" rot=0
    piece p7 "home:crossing/pool"   at="(1sq, 2sq)" rot=0
    piece p8 "home:crossing/grass"  at="(2sq, 2sq)" rot=0 seed=5
}
"#;
    let plates = r#"dharmatiles version="1.0.0"

map {
    about "What to print for the crossing."
    piece "home:crossing/grass"  count=4
    piece "home:crossing/stream" count=2
    piece "home:crossing/wall"   count=2
    piece "home:crossing/pool"
    piece "home:meadows/flowers" count=2
}

plates printer="bambu/x1c-ams"
"#;
    for (name, text) in [("crossing.dmap", map), ("print-list.dmap", plates)] {
        let p = env.home.join(name);
        std::fs::write(&p, text).map_err(|e| format!("cannot write {}: {e}", p.display()))?;
    }
    // the inventory is the program's own log: prints of several seeds, a failure, a loss
    let have: [&[&str]; 7] = [
        &["home:crossing/grass", "--count", "2"],
        &["home:crossing/grass", "--seed", "12"],
        &["home:crossing/stream", "--seed", "3"],
        &[
            "home:crossing/wall",
            "--failed",
            "--note",
            "top layer stringing",
        ],
        &["home:crossing/wall"],
        &["home:crossing/pool", "--seed", "7"],
        &["home:meadows/flowers", "--seed", "4"],
    ];
    for a in have {
        let mut cmd = env.apply(Command::new(exe), root);
        cmd.args(["inventory", "have"])
            .args(a)
            .args(["--colour", "multi"])
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        crate::run(&mut cmd)?;
    }
    let mut cmd = env.apply(Command::new(exe), root);
    cmd.args([
        "inventory",
        "gone",
        "home:crossing/grass",
        "--colour",
        "multi",
        "--note",
        "dropped on the floor",
    ])
    .stdout(Stdio::null())
    .stderr(Stdio::null());
    crate::run(&mut cmd)
}

/// A free port on the loopback address: bound and released, which is as
/// good as it can be made without keeping it.
fn free_port() -> Result<u16, String> {
    let l = TcpListener::bind(("127.0.0.1", 0)).map_err(|e| format!("no free port: {e}"))?;
    l.local_addr()
        .map(|a| a.port())
        .map_err(|e| format!("no free port: {e}"))
}

fn wait_for_port(port: u16, within: Duration, log: &Path) -> Result<(), String> {
    let t = Instant::now();
    while t.elapsed() < within {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    Err(format!(
        "nothing listens on 127.0.0.1:{port} after {} s (see {})",
        within.as_secs(),
        log.display()
    ))
}

/// Chrome (or a Chromium that speaks the same protocol): `$DT_CHROME`, else
/// the usual places. Says which it looked for when there is none.
fn find_chrome() -> Result<PathBuf, String> {
    if let Some(p) = std::env::var_os("DT_CHROME") {
        let p = PathBuf::from(p);
        return if p.is_file() {
            Ok(p)
        } else {
            Err(format!(
                "DT_CHROME is {} and that is not a file",
                p.display()
            ))
        };
    }
    let fixed = [
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
        "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
        "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
        "/usr/bin/google-chrome",
        "/usr/bin/google-chrome-stable",
        "/usr/bin/chromium",
        "/usr/bin/chromium-browser",
        "/snap/bin/chromium",
    ];
    for p in fixed {
        if Path::new(p).is_file() {
            return Ok(PathBuf::from(p));
        }
    }
    Err(format!(
        "cargo xtask shots needs Google Chrome (or Chromium, Edge, Brave) and found none.\n\
         Looked in:\n  {}\n\
         Install Chrome, or set DT_CHROME to the browser's executable.",
        fixed.join("\n  ")
    ))
}

/// Node 22 or newer (the script uses its built-in `WebSocket` and `fetch`).
fn find_node() -> Result<PathBuf, String> {
    let out = Command::new("node")
        .arg("--version")
        .output()
        .map_err(|e| format!("cargo xtask shots needs Node 22 or newer on the PATH (`node --version` failed: {e})"))?;
    let v = String::from_utf8_lossy(&out.stdout);
    let major: u32 = v
        .trim()
        .trim_start_matches('v')
        .split('.')
        .next()
        .and_then(|m| m.parse().ok())
        .unwrap_or(0);
    if major < 22 {
        return Err(format!(
            "cargo xtask shots needs Node 22 or newer; `node --version` says {}",
            v.trim()
        ));
    }
    Ok(PathBuf::from("node"))
}

/// Every picture the docs show exists, and every callout they name is in
/// `shots.json`: each `<figure class="shot" data-shot="x">` needs
/// `x-light.webp`, `x-dark.webp`, an `x` entry in `shots.json` and, for each
/// `data-at="k"` in its key, an anchor `k`. Run by `cargo xtask docs` (and
/// `--check`) and by this crate's test.
pub fn check(root: &Path) -> Result<(), String> {
    let dir = root.join(DIR);
    let book_path = dir.join("shots.json");
    let book: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&book_path)
            .map_err(|e| format!("{}: {e} (run `cargo xtask shots`)", book_path.display()))?,
    )
    .map_err(|e| format!("{}: {e}", book_path.display()))?;
    let mut pages = Vec::new();
    html_files(&root.join("docs"), &mut pages)?;
    let mut problems = Vec::new();
    for page in pages {
        let text =
            std::fs::read_to_string(&page).map_err(|e| format!("{}: {e}", page.display()))?;
        let name = page
            .strip_prefix(root)
            .unwrap_or(&page)
            .display()
            .to_string();
        let mut rest = text.as_str();
        while let Some(i) = rest.find("<figure class=\"shot\" data-shot=\"") {
            rest = &rest[i + 32..];
            let (shot, after) = rest
                .split_once('"')
                .ok_or(format!("{name}: an unclosed data-shot"))?;
            let body = after.split("</figure>").next().unwrap_or(after);
            for theme in ["light", "dark"] {
                if !dir.join(format!("{shot}-{theme}.webp")).is_file() {
                    problems.push(format!("{name}: {DIR}/{shot}-{theme}.webp is missing"));
                }
            }
            let Some(entry) = book["shots"].get(shot) else {
                problems.push(format!("{name}: shots.json has no picture \"{shot}\""));
                continue;
            };
            let mut keys = body;
            while let Some(j) = keys.find("data-at=\"") {
                keys = &keys[j + 9..];
                let at = keys.split('"').next().unwrap_or("");
                if entry["anchors"].get(at).is_none() {
                    problems.push(format!(
                        "{name}: picture \"{shot}\" has no callout \"{at}\" in shots.json"
                    ));
                }
            }
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{}\nredraw them with `cargo xtask shots`",
            problems.join("\n")
        ))
    }
}

fn html_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    for e in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let p = e.map_err(|e| e.to_string())?.path();
        if p.is_dir() {
            html_files(&p, out)?;
        } else if p.extension().is_some_and(|x| x == "html") {
            out.push(p);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_docs_pictures_exist() {
        let root = crate::root();
        super::check(&root).unwrap_or_else(|e| panic!("{e}"));
    }
}
