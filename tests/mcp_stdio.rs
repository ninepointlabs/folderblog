//! Drive `folderblog mcp` over real stdio, the way an agent's client does.

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_folderblog");
const CANARY: &str = "canary-secret-for-mcp-test";

fn run(args: &[&str], env: &[(&str, &Path)]) {
    let mut c = Command::new(BIN);
    c.args(args);
    for (k, v) in env {
        c.env(k, v);
    }
    let out = c.output().unwrap();
    assert!(out.status.success(), "{args:?}: {}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn mcp_over_stdio_drafts_refuses_without_confirm_and_never_shows_secrets() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("Blogs");
    let conf = tmp.path().join("config");
    let blog = root.join("show.test");
    let env = [("FOLDERBLOG_CONFIG_DIR", conf.as_path()), ("FOLDERBLOG_ROOT", root.as_path())];
    run(&["new", blog.to_str().unwrap(), "--no-github"], &env);
    run(&["podcast", "on", blog.to_str().unwrap()], &env);
    // Hosting with a saved key whose secret is a canary; nothing here contacts it.
    let toml = std::fs::read_to_string(blog.join("blog.toml")).unwrap();
    std::fs::write(
        blog.join("blog.toml"),
        format!("{toml}\n[podcast.storage]\ntype = \"s3\"\nendpoint = \"http://127.0.0.1:9\"\nbucket = \"show\"\npublic_url = \"https://media.show.test\"\n"),
    )
    .unwrap();
    std::fs::create_dir_all(conf.join("storage")).unwrap();
    std::fs::write(conf.join("storage/show.json"), json!({ "access_key_id": "AKIDSTDIO9876", "secret_access_key": CANARY }).to_string()).unwrap();
    let audio = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/stress/podcast/2024-05-01-pilot/episode.mp3");

    let mut child = Command::new(BIN)
        .arg("mcp")
        .env("FOLDERBLOG_CONFIG_DIR", &conf)
        .env("FOLDERBLOG_ROOT", &root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut all = String::new();
    let mut call = |id: u64, method: &str, params: Value| -> Value {
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        writeln!(stdin, "{msg}").unwrap();
        if method == "initialize" {
            writeln!(stdin, "{}", json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).unwrap();
        }
        loop {
            let mut line = String::new();
            assert!(stdout.read_line(&mut line).unwrap() > 0, "server closed stdout");
            all.push_str(&line);
            let v: Value = serde_json::from_str(&line).unwrap();
            if v["id"] == json!(id) {
                return v["result"].clone();
            }
        }
    };
    let tool = |call: &mut dyn FnMut(u64, &str, Value) -> Value, id: u64, name: &str, args: Value| -> (bool, String) {
        let r = call(id, "tools/call", json!({ "name": name, "arguments": args }));
        (r["isError"] == json!(true), r["content"][0]["text"].as_str().unwrap_or("").to_string())
    };

    let init = call(1, "initialize", json!({ "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "test", "version": "1" } }));
    assert_eq!(init["serverInfo"]["name"], "folderblog");
    let tools = call(2, "tools/list", json!({}));
    let names: Vec<&str> = tools["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    for want in ["podcast_status", "list_episodes", "inspect_audio", "create_episode", "update_episode", "set_chapters", "validate", "publish_episode"] {
        assert!(names.contains(&want), "{want} missing from {names:?}");
    }
    let publish = tools["tools"].as_array().unwrap().iter().find(|t| t["name"] == "publish_episode").unwrap();
    assert_eq!(publish["annotations"]["destructiveHint"], true);
    assert!(publish["description"].as_str().unwrap().contains("explicit yes"));

    let (err, text) = tool(&mut call, 3, "inspect_audio", json!({ "path": audio }));
    assert!(!err && text.contains("\"accepted\": true") && text.contains("\"format\": \"mp3\""), "{text}");
    let (err, text) = tool(&mut call, 4, "create_episode", json!({ "blog": "show.test", "audio_path": audio, "title": "Hello listeners", "notes_markdown": "Notes." }));
    assert!(!err, "{text}");
    assert!(text.contains("\"draft\": true"), "{text}");
    let index = std::fs::read_to_string(blog.join("podcast").read_dir().unwrap().flatten().find(|e| e.path().is_dir()).unwrap().path().join("index.md")).unwrap();
    assert!(index.contains("draft: true"), "{index}");
    let (err, text) = tool(&mut call, 5, "publish_episode", json!({ "blog": "show.test", "slug": "hello-listeners" }));
    assert!(err && text.contains("confirm: true"), "{text}");
    let (err, text) = tool(&mut call, 6, "podcast_status", json!({ "blog": "show.test" }));
    assert!(!err && text.contains("…9876") && text.contains("\"secret\": \"set\""), "{text}");
    let (_, text) = tool(&mut call, 7, "list_episodes", json!({ "blog": "show.test" }));
    assert!(text.contains("hello-listeners"), "{text}");
    let (_, _) = tool(&mut call, 8, "validate", json!({ "blog": "show.test" }));
    let (err, text) = tool(&mut call, 9, "update_episode", json!({ "blog": "show.test", "slug": "hello-listeners", "fields": { "explicit": "no" } }));
    assert!(err && text.contains("must be true or false"), "{text}");

    drop(stdin);
    let _ = child.wait();
    assert!(!all.contains(CANARY), "the secret appeared in MCP output");
    assert!(!all.contains("AKIDSTDIO9876"), "the full key id appeared in MCP output");
}
