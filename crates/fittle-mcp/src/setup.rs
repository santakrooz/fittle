//! Connecting AI tools: where the `fittle` binary is, copy-ready setup for
//! popular MCP clients, and a self-test that starts the server and lists its
//! tools. Fittle's server runs locally over stdio, so clients need a command,
//! not a URL. Nothing here edits another app's configuration.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{Value, json};

/// Setup for one client.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Snippet {
    /// Stable id (`claude-code`, `claude-desktop`, …).
    pub id: &'static str,
    pub client: &'static str,
    /// What to do with `text`.
    pub how: String,
    /// Config file to edit, when the client uses one (for this OS).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// `shell`, `json` or `toml`.
    pub format: &'static str,
    pub text: String,
}

fn exe_name() -> &'static str {
    if cfg!(windows) {
        "fittle.exe"
    } else {
        "fittle"
    }
}

fn on_path() -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|p| {
        std::env::split_paths(&p)
            .map(|d| d.join(exe_name()))
            .find(|c| c.is_file())
    })
}

/// The `fittle` CLI to launch: `FITTLE_BIN`, next to `exe_dir` (bundled
/// with the app), on `PATH`, in `~/.cargo/bin`, or a dev build beside it.
pub fn find_binary(exe_dir: Option<&Path>) -> Option<PathBuf> {
    if let Some(b) = std::env::var_os("FITTLE_BIN")
        .map(PathBuf::from)
        .filter(|p| p.is_file())
    {
        return Some(b);
    }
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(d) = exe_dir {
        candidates.push(d.join(exe_name()));
        // Dev: target/debug/fittle-app beside target/{release,debug}/fittle.
        if let Some(target) = d.parent() {
            candidates.push(target.join("release").join(exe_name()));
            candidates.push(target.join("debug").join(exe_name()));
        }
    }
    if let Some(p) = on_path() {
        candidates.push(p);
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from);
    if let Some(h) = home {
        candidates.push(h.join(".cargo").join("bin").join(exe_name()));
    }
    candidates.into_iter().find(|c| c.is_file())
}

fn args(roots: &[String]) -> Vec<String> {
    let mut a = vec!["mcp".to_string()];
    for r in roots {
        a.push("--root".into());
        a.push(r.clone());
    }
    a
}

fn shell_quote(s: &str) -> String {
    if !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "/._-~:\\".contains(c))
    {
        s.to_string()
    } else if cfg!(windows) {
        format!("\"{s}\"")
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

fn config_home(parts: &[&str]) -> String {
    let base = if cfg!(target_os = "macos") {
        "~/Library/Application Support".to_string()
    } else if cfg!(windows) {
        "%APPDATA%".to_string()
    } else {
        "~/.config".to_string()
    };
    let sep = if cfg!(windows) { "\\" } else { "/" };
    std::iter::once(base)
        .chain(parts.iter().map(|s| s.to_string()))
        .collect::<Vec<_>>()
        .join(sep)
}

fn user_file(parts: &[&str]) -> String {
    let base = if cfg!(windows) { "%USERPROFILE%" } else { "~" };
    let sep = if cfg!(windows) { "\\" } else { "/" };
    std::iter::once(base.to_string())
        .chain(parts.iter().map(|s| s.to_string()))
        .collect::<Vec<_>>()
        .join(sep)
}

/// Copy-ready setup for each client, for `bin` and the allowed `roots`.
pub fn snippets(bin: &Path, roots: &[String]) -> Vec<Snippet> {
    let cmd = bin.to_string_lossy().into_owned();
    let a = args(roots);
    let server = json!({ "command": cmd, "args": a });
    let pretty = |v: Value| serde_json::to_string_pretty(&v).unwrap_or_default();
    let toml_args = a
        .iter()
        .map(|x| format!("{x:?}"))
        .collect::<Vec<_>>()
        .join(", ");
    let cli_args = a
        .iter()
        .map(|x| shell_quote(x))
        .collect::<Vec<_>>()
        .join(" ");
    vec![
        Snippet {
            id: "claude-code",
            client: "Claude Code",
            how: "Run once in a terminal.".into(),
            file: None,
            format: "shell",
            text: format!("claude mcp add fittle -- {} {cli_args}", shell_quote(&cmd)),
        },
        Snippet {
            id: "claude-desktop",
            client: "Claude Desktop",
            how: "Add to the mcpServers section (Settings → Developer → Edit Config), then restart Claude Desktop.".into(),
            file: Some(config_home(&["Claude", "claude_desktop_config.json"])),
            format: "json",
            text: pretty(json!({ "mcpServers": { "fittle": server } })),
        },
        Snippet {
            id: "codex",
            client: "Codex",
            how: "Add to Codex's config (CLI and IDE extension share it).".into(),
            file: Some(user_file(&[".codex", "config.toml"])),
            format: "toml",
            text: format!("[mcp_servers.fittle]\ncommand = {cmd:?}\nargs = [{toml_args}]\n"),
        },
        Snippet {
            id: "cursor",
            client: "Cursor",
            how: "Add to the mcpServers section (Settings → MCP → Add new global MCP server).".into(),
            file: Some(user_file(&[".cursor", "mcp.json"])),
            format: "json",
            text: pretty(json!({ "mcpServers": { "fittle": server } })),
        },
        Snippet {
            id: "vscode",
            client: "VS Code (Copilot)",
            how: "Add to .vscode/mcp.json in a workspace, or run “MCP: Open User Configuration”.".into(),
            file: Some(".vscode/mcp.json".into()),
            format: "json",
            text: pretty(json!({ "servers": { "fittle": { "type": "stdio", "command": cmd, "args": a } } })),
        },
        Snippet {
            id: "windsurf",
            client: "Windsurf",
            how: "Add to the mcpServers section, then refresh MCP servers.".into(),
            file: Some(user_file(&[".codeium", "windsurf", "mcp_config.json"])),
            format: "json",
            text: pretty(json!({ "mcpServers": { "fittle": server } })),
        },
        Snippet {
            id: "other",
            client: "Any MCP client (stdio)",
            how: "Fittle runs locally over stdio: give the client this command and arguments. There is no URL; claude.ai on the web needs a remote server, which Fittle doesn't run.".into(),
            file: None,
            format: "shell",
            text: format!("{} {cli_args}", shell_quote(&cmd)),
        },
    ]
}

/// What a self-test found.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TestResult {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
    pub tools: Vec<String>,
    pub elapsed_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Start `bin mcp --root …`, initialize, list tools, and stop it.
pub fn self_test(bin: &Path, roots: &[String]) -> TestResult {
    let t = Instant::now();
    let fail = |e: String| TestResult {
        ok: false,
        server: None,
        tools: vec![],
        elapsed_ms: t.elapsed().as_millis() as u64,
        error: Some(e),
    };
    let mut child = match Command::new(bin)
        .args(args(roots))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return fail(format!("could not start {}: {e}", bin.display())),
    };
    let (Some(mut stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
        let _ = child.kill();
        return fail("no stdio".into());
    };
    // Read replies on a thread so a stuck server can't hang the caller.
    let (tx, rx) = std::sync::mpsc::channel::<Value>();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Ok(v) = serde_json::from_str::<Value>(&line) {
                if tx.send(v).is_err() {
                    break;
                }
            }
        }
    });
    let mut send = |v: Value| writeln!(stdin, "{v}").and_then(|_| stdin.flush());
    let wait = |id: u64| -> Option<Value> {
        let deadline = Instant::now() + Duration::from_secs(5);
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            match rx.recv_timeout(left) {
                Ok(v) if v["id"] == id => return Some(v),
                Ok(_) => continue,
                Err(_) => return None,
            }
        }
        None
    };
    let result = (|| -> Result<TestResult, String> {
        send(json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "fittle-self-test", "version": "0"}}}))
            .map_err(|e| e.to_string())?;
        let init = wait(1).ok_or("no reply to initialize (is this a Fittle build with MCP?)")?;
        if let Some(e) = init.get("error") {
            return Err(e.to_string());
        }
        let info = &init["result"]["serverInfo"];
        send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))
            .map_err(|e| e.to_string())?;
        send(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}))
            .map_err(|e| e.to_string())?;
        let list = wait(2).ok_or("no reply to tools/list")?;
        let tools: Vec<String> = list["result"]["tools"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|t| t["name"].as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        Ok(TestResult {
            ok: !tools.is_empty(),
            server: Some(format!(
                "{} {}",
                info["name"].as_str().unwrap_or("?"),
                info["version"].as_str().unwrap_or("")
            )),
            tools,
            elapsed_ms: t.elapsed().as_millis() as u64,
            error: None,
        })
    })();
    let _ = child.kill();
    let mut stderr = String::new();
    if let Some(mut e) = child.stderr.take() {
        use std::io::Read;
        let _ = child.wait();
        let _ = e.read_to_string(&mut stderr);
    }
    result.unwrap_or_else(|e| {
        let last = stderr.lines().rfind(|l| !l.trim().is_empty()).unwrap_or("");
        fail(if last.is_empty() {
            e
        } else {
            format!("{e}: {last}")
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippets_cover_clients() {
        let s = snippets(
            Path::new("/Applications/Fittle.app/Contents/MacOS/fittle"),
            &["/Users/me/Astro".into(), "/Volumes/Data Drive".into()],
        );
        let ids: Vec<&str> = s.iter().map(|x| x.id).collect();
        assert_eq!(
            ids,
            [
                "claude-code",
                "claude-desktop",
                "codex",
                "cursor",
                "vscode",
                "windsurf",
                "other"
            ]
        );
        let cc = &s[0].text;
        assert!(cc.starts_with("claude mcp add fittle -- "), "{cc}");
        assert!(
            cc.contains("--root /Users/me/Astro") && cc.contains("'/Volumes/Data Drive'"),
            "{cc}"
        );
        let desktop: Value = serde_json::from_str(&s[1].text).unwrap();
        assert_eq!(desktop["mcpServers"]["fittle"]["args"][0], "mcp");
        assert_eq!(
            desktop["mcpServers"]["fittle"]["args"][4],
            "/Volumes/Data Drive"
        );
        assert!(
            s[2].text.contains("[mcp_servers.fittle]")
                && s[2].text.contains("args = [\"mcp\", \"--root\"")
        );
        let vs: Value = serde_json::from_str(&s[4].text).unwrap();
        assert_eq!(vs["servers"]["fittle"]["type"], "stdio");
    }

    #[test]
    fn self_test_reports_a_bad_binary() {
        let r = self_test(Path::new("/nonexistent/fittle"), &[]);
        assert!(!r.ok && r.error.unwrap().contains("could not start"));
    }
}
