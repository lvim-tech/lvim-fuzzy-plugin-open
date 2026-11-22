//! lvim-fuzzy-plugin-open: a PROCESS plugin (JSON lines on stdin / stdout) that decides WHICH PROGRAM OPENS A FILE. It
//! never runs the program: it answers with one `open` instruction and the tool that asked carries it out
//! (`lvim_fuzzy_ui::open`): `{"op":"open","action":"editor"}` (the tool's own editor),
//! `{"op":"open","action":"exec","cmd":"imv '/p/a.png'"}` (a command, run detached) or `{"op":"open","missing":[…]}`
//! (the program that would open it is not installed: say so, open nothing). `{"op":"pass"}`: not this plugin's file.
//!
//! The request's context: `{ "call": "open", "path": "…", "config": { … } }`; `config` is the plugin's settings merged
//! from its defaults, the shared override file and the asking tool's own override. Which engine opens a file is
//! chosen by its `[config.types.*]` group (see `plugin.toml`).

use std::io::{BufRead, Write};
use std::path::Path;

use serde_json::{Value, json};

/// The plugin's loop: JSON lines on stdin, answers on stdout. Returns when stdin closes or a `quit` event comes.
pub fn run() {
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let Ok(req) = serde_json::from_str::<Value>(&line) else { continue };
        match req["method"].as_str() {
            Some("call") => {
                let id = req["id"].clone();
                let reply = match answer(&req["params"]) {
                    Ok(ins) => json!({"id": id, "result": [ins]}),
                    Err(e) => json!({"id": id, "error": e}),
                };
                let _ = writeln!(out, "{reply}");
                let _ = out.flush();
            }
            Some("event") if req["params"]["event"] == "quit" => break,
            _ => {}
        }
    }
}

/// The answer to one call: how to open the file, per the engine its type group gives; or `pass`.
pub fn answer(ctx: &Value) -> Result<Value, String> {
    if ctx["call"] != "open" {
        return Err(format!("no such function: {}", ctx["call"].as_str().unwrap_or("?")));
    }
    let path = Path::new(ctx["path"].as_str().ok_or("no `path`")?);
    let cfg = &ctx["config"];
    let Some((group, engine)) = lvim_fuzzy_plugin::types::engine_for(path, cfg) else { return Ok(json!({"op": "pass"})) };
    Ok(match engine.as_str() {
        "editor" => json!({"op": "open", "action": "editor"}),
        "system" => run_command("system", "xdg-open {}", path),
        "host" | "pass" => json!({"op": "pass"}),
        name => match cfg["engines"][name].as_object() {
            Some(e) if e.get("kind").and_then(Value::as_str).unwrap_or("command") == "command" => {
                run_command(name, e.get("command").and_then(Value::as_str).unwrap_or(""), path)
            }
            Some(e) => json!({"op": "open", "missing": [format!("engines.{name}: kind `{}` — only \"command\" can be defined", e.get("kind").and_then(Value::as_str).unwrap_or(""))]}),
            None => json!({
                "op": "open",
                "missing": [format!("types.{group}: engine \"{name}\" is neither built in (editor, system, host) nor defined in [engines.{name}]")],
            }),
        },
    })
}

/// A command engine: the command line with the file in it, or the name of the program that is missing.
fn run_command(engine: &str, template: &str, path: &Path) -> Value {
    let prog = template.split_whitespace().next().unwrap_or("");
    if prog.is_empty() {
        return json!({"op": "open", "missing": [format!("engines.{engine}: no `command`")]});
    }
    if !lvim_fuzzy_plugin::have(prog) {
        return json!({"op": "open", "missing": [format!("`{prog}` is not installed (opens this type: engine `{engine}`)")]});
    }
    json!({"op": "open", "action": "exec", "cmd": lvim_fuzzy_plugin::types::expand(template, path)})
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = include_str!("../plugin.toml");

    fn tmp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("lvim-fuzzy-plugin-open-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// The plugin's own default configuration (plugin.toml's [config]) with `over` laid over it.
    fn with(over: Value) -> Value {
        let m: toml::Table = toml::from_str(MANIFEST).unwrap();
        let mut c = serde_json::to_value(&m["config"]).unwrap();
        fn merge(a: &mut Value, b: &Value) {
            match (a, b) {
                (Value::Object(x), Value::Object(y)) => y.iter().for_each(|(k, v)| merge(x.entry(k.clone()).or_insert(Value::Null), v)),
                (a, b) => *a = b.clone(),
            }
        }
        merge(&mut c, &over);
        c
    }

    fn ask(path: &Path, over: Value) -> Value {
        answer(&json!({"call": "open", "path": path, "config": with(over)})).unwrap()
    }

    #[test]
    fn text_and_directories_go_to_the_editor_and_the_rest_to_the_system() {
        let d = tmp("defaults");
        std::fs::write(d.join("a.txt"), "hello").unwrap();
        std::fs::write(d.join("b.dat"), [1u8, 0, 2]).unwrap();
        assert_eq!(ask(&d.join("a.txt"), json!({}))["action"], "editor");
        assert_eq!(ask(&d, json!({}))["action"], "editor");
        let v = ask(&d.join("b.dat"), json!({}));
        if lvim_fuzzy_plugin::have("xdg-open") {
            assert_eq!(v["cmd"], format!("xdg-open '{}'", d.join("b.dat").display()));
        } else {
            assert!(v["missing"][0].as_str().unwrap().contains("xdg-open"));
        }
        assert_eq!(ask(&d.join("none"), json!({}))["op"], "pass");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_type_is_opened_by_the_engine_the_config_names() {
        let d = tmp("engines");
        let (png, pdf) = (d.join("c.png"), d.join("d.pdf"));
        std::fs::write(&png, [0x89, b'P', 0, 1]).unwrap();
        std::fs::write(&pdf, "%PDF-1.4 x").unwrap();
        // `sh` is on every PATH: a stand-in for a viewer
        let cfg = json!({
            "types": {"pictures": {"mime": ["image/*"], "engine": "view"}, "pdf": {"ext": ["pdf"], "engine": "reader"}},
            "engines": {"view": {"kind": "command", "command": "sh -c 'echo {}'"}, "reader": {"kind": "command", "command": "sh"}},
        });
        assert_eq!(ask(&png, cfg.clone())["cmd"], format!("sh -c 'echo {}'", quote(&png)));
        assert_eq!(ask(&pdf, cfg)["cmd"], format!("sh {}", quote(&pdf)), "no {{}}: the file is appended");
        let _ = std::fs::remove_dir_all(&d);
    }

    fn quote(p: &Path) -> String {
        lvim_fuzzy_plugin::types::quote(&p.to_string_lossy())
    }

    #[test]
    fn a_missing_program_and_a_mistake_in_the_config_are_said() {
        let d = tmp("missing");
        let f = d.join("a.rs");
        std::fs::write(&f, "x").unwrap();
        let by = |engine: &str, defs: Value| json!({"types": {"text": {"engine": engine}}, "engines": defs});
        let v = ask(&f, by("g", json!({"g": {"kind": "command", "command": "no-such-viewer-here {}"}})));
        assert!(v["missing"][0].as_str().unwrap().contains("no-such-viewer-here"));
        assert!(v.get("action").is_none(), "nothing is opened when the program is missing");
        assert!(ask(&f, by("nosuch", json!({})))["missing"][0].as_str().unwrap().contains("nosuch"));
        assert!(ask(&f, by("g", json!({"g": {"kind": "wasm"}})))["missing"][0].as_str().unwrap().contains("only \"command\""));
        assert!(ask(&f, by("g", json!({"g": {"kind": "command"}})))["missing"][0].as_str().unwrap().contains("no `command`"));
        assert_eq!(ask(&f, by("host", json!({})))["op"], "pass");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn unknown_call_and_missing_path() {
        assert!(answer(&json!({"call": "preview", "path": "/"})).is_err());
        assert!(answer(&json!({"call": "open"})).is_err());
    }
}
