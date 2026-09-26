//! `fittle mcp` end to end: JSON-RPC over stdio, as Claude would drive it.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{Value, json};

struct Client {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    id: u64,
}

impl Client {
    fn start(root: &Path) -> Client {
        let mut child = Command::new(env!("CARGO_BIN_EXE_fittle"))
            .args(["mcp", "--root"])
            .arg(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        let mut c = Client {
            child,
            stdin,
            stdout,
            id: 0,
        };
        let init = c.request(
            "initialize",
            json!({ "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } }),
        );
        assert_eq!(init["serverInfo"]["name"], "fittle");
        assert!(init["instructions"].as_str().unwrap().contains("dry-run"));
        c.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
        c
    }

    fn send(&mut self, v: Value) {
        writeln!(self.stdin, "{v}").unwrap();
        self.stdin.flush().unwrap();
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        self.id += 1;
        let id = self.id;
        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        loop {
            let mut line = String::new();
            assert!(
                self.stdout.read_line(&mut line).unwrap() > 0,
                "server closed"
            );
            let v: Value = serde_json::from_str(&line).unwrap();
            if v["id"] == id {
                assert!(v.get("error").is_none(), "{method}: {v}");
                return v["result"].clone();
            }
        }
    }

    /// Call a tool; returns (is_error, content blocks).
    fn call(&mut self, name: &str, args: Value) -> (bool, Vec<Value>) {
        let r = self.request("tools/call", json!({ "name": name, "arguments": args }));
        (
            r["isError"] == true,
            r["content"].as_array().cloned().unwrap_or_default(),
        )
    }

    fn call_json(&mut self, name: &str, args: Value) -> Value {
        let (err, content) = self.call(name, args);
        assert!(!err, "{name}: {content:?}");
        serde_json::from_str(content[0]["text"].as_str().unwrap()).unwrap()
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

fn corpus(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/synthetic")
        .join(rel)
}

#[test]
fn mcp_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let sub = dir.path().join("sub.fit");
    std::fs::copy(
        corpus("seestar/Light_NGC 6995_20.0s_LP_20260924-213412.fit"),
        &sub,
    )
    .unwrap();
    let before = std::fs::read(&sub).unwrap();
    let s = sub.to_string_lossy().to_string();
    let mut c = Client::start(dir.path());

    // Discovery.
    let tools = c.request("tools/list", json!({}));
    let names: Vec<&str> = tools["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    for t in [
        "fits_inspect",
        "fits_header",
        "fits_preview",
        "fits_stats",
        "fits_diff",
        "fits_scan_folder",
        "fits_set_keywords",
        "fits_scrub",
        "fits_export",
        "fits_fpack",
    ] {
        assert!(names.contains(&t), "missing {t}");
    }
    let res = c.request("resources/list", json!({}));
    assert_eq!(res["resources"].as_array().unwrap().len(), 3);
    let kw = c.request("resources/read", json!({ "uri": "fits://keywords" }));
    assert!(
        kw["contents"][0]["text"]
            .as_str()
            .unwrap()
            .contains("EXPTIME")
    );

    // Reads.
    let info = c.call_json("fits_inspect", json!({ "paths": [s] }));
    assert_eq!(info["schema"], "fittle.info/1");
    assert_eq!(info["fields"]["object"]["value"], "NGC 6995");
    let (err, content) = c.call("fits_preview", json!({ "path": s, "max_edge": 64 }));
    assert!(!err);
    assert_eq!(content[0]["type"], "image");
    assert_eq!(content[0]["mimeType"], "image/jpeg");
    assert!(content[1]["text"].as_str().unwrap().contains("NGC 6995"));
    let stats = c.call_json("fits_stats", json!({ "path": s }));
    assert_eq!(stats["channels"].as_array().unwrap().len(), 3);
    assert!(stats["stars"]["stars"].as_u64().unwrap() > 0);
    let grade = c.call_json(
        "fits_grade_subs",
        json!({ "path": dir.path(), "move_rejects": true, "reject": "stars<100000" }),
    );
    assert_eq!(grade["schema"], "fittle.grade/1");
    assert_eq!(grade["rejected"], 1);
    assert_eq!(grade["dry_run"], true);
    assert!(sub.exists(), "dry run must not move");
    let scan = c.call_json("fits_scan_folder", json!({ "path": dir.path() }));
    assert_eq!(scan["files"], 1);
    assert_eq!(
        c.call_json("fits_header", json!({ "path": s, "grep": "EXPTIME" }))["schema"],
        "fittle.header/1"
    );

    // Outside the allowed folder: refused, as a tool error the agent can read.
    let outside = corpus("siril/r_pp_NGC6995_stacked.fit");
    let (err, content) = c.call("fits_inspect", json!({ "paths": [outside] }));
    assert!(
        err && content[0]["text"]
            .as_str()
            .unwrap()
            .contains("outside the allowed folders")
    );

    // Writes are dry runs by default and leave the file alone.
    let plan = c.call_json(
        "fits_set_keywords",
        json!({ "paths": [s], "set": { "FOCALLEN": 250.0 } }),
    );
    assert_eq!(plan["dry_run"], true);
    assert_eq!(plan["would_change"], 1);
    assert_eq!(std::fs::read(&sub).unwrap(), before);
    let exp = c.call_json("fits_export", json!({ "path": s, "format": "jpeg" }));
    assert_eq!(exp["dry_run"], true);
    assert!(!dir.path().join("sub.jpg").exists());

    // Applying writes only the header.
    let done = c.call_json(
        "fits_set_keywords",
        json!({ "paths": [s], "set": { "FOCALLEN": 250.0 }, "dry_run": false }),
    );
    assert_eq!(done["written"], 1);
    let info = c.call_json("fits_inspect", json!({ "paths": [s] }));
    assert_eq!(info["fields"]["focal_mm"]["value"], 250.0);
    assert!(dir.path().join("sub.fit.bak").exists());

    let out = c.call_json(
        "fits_export",
        json!({ "path": s, "format": "png", "dry_run": false }),
    );
    assert!(Path::new(out["path"].as_str().unwrap()).exists());
    let packed = c.call_json("fits_fpack", json!({ "paths": [s], "dry_run": false }));
    assert_eq!(packed["files"][0]["verified"], true);
}
