//! The command line itself: help and exit codes, single commands with their result,
//! command and action catalogs, edit plans from stdin, Hangul action steps,
//! keep-going, page images and passwords.
use serde_json::Value;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn cli(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_hwpcraft-cli"))
        .args(args)
        .current_dir(dir)
        .env("HWPCRAFT_CONFIG_DIR", dir.join("config"))
        .output()
        .unwrap()
}

fn cli_with_stdin(dir: &Path, args: &[&str], stdin: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_hwpcraft-cli"))
        .args(args)
        .current_dir(dir)
        .env("HWPCRAFT_CONFIG_DIR", dir.join("config"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn ok(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap_or(Value::Null)
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn text(dir: &Path, file: &str) -> String {
    let out = cli(dir, &["text", file]);
    assert!(out.status.success(), "{}", stderr(&out));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn help_version_and_usage_errors() {
    let dir = tempfile::tempdir().unwrap();
    let help = cli(dir.path(), &["--help"]);
    assert!(help.status.success());
    let help = String::from_utf8_lossy(&help.stdout).into_owned();
    // The HTTP server options and every subcommand are listed.
    for needle in [
        "--transport stdio|http",
        "run <입력|--new>",
        "actions",
        "종료 코드",
    ] {
        assert!(help.contains(needle), "{needle}");
    }
    for alias in ["-h", "help"] {
        let out = cli(dir.path(), &[alias]);
        assert_eq!(String::from_utf8_lossy(&out.stdout), help, "{alias}");
    }
    assert!(cli(dir.path(), &["-V"]).status.success());
    // A mistake in the command line exits with 2, a failed operation with 1.
    for args in [
        &["frobnicate"][..],
        &["new"],
        &["info", "a.hwpx", "--frob"],
        &["info", "a.hwpx", "--password"],
    ] {
        assert_eq!(cli(dir.path(), args).status.code(), Some(2), "{args:?}");
    }
    assert_eq!(
        cli(dir.path(), &["info", "nope.hwpx"]).status.code(),
        Some(1)
    );
}

#[test]
fn run_prints_a_result_and_saves_only_with_an_output() {
    let dir = tempfile::tempdir().unwrap();
    ok(&cli(dir.path(), &["new", "a.hwpx", "첫 문단"]));
    let before = std::fs::read(dir.path().join("a.hwpx")).unwrap();
    // A query: the result on stdout and no file written.
    let read = ok(&cli(
        dir.path(),
        &["run", "a.hwpx", "document.text", r#"{"limit":10}"#],
    ));
    assert_eq!(read["result"]["text"], "첫 문단", "{read}");
    assert!(read["saved"].is_null());
    assert_eq!(std::fs::read(dir.path().join("a.hwpx")).unwrap(), before);
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    // An edit is saved only to the -o file.
    let edit = ok(&cli(
        dir.path(),
        &[
            "run",
            "a.hwpx",
            "text.insert",
            r#"{"text":"앞"}"#,
            "-o",
            "b.hwpx",
        ],
    ));
    assert_eq!(edit["saved"], "b.hwpx");
    assert_eq!(text(dir.path(), "b.hwpx").trim(), "앞첫 문단");
    assert_eq!(std::fs::read(dir.path().join("a.hwpx")).unwrap(), before);
    // A failed command saves nothing.
    let failed = cli(
        dir.path(),
        &["run", "a.hwpx", "no.such", "{}", "-o", "c.hwpx"],
    );
    assert_eq!(failed.status.code(), Some(1));
    assert!(!dir.path().join("c.hwpx").exists());
    // A Hangul action name runs through its mapping, as MCP hwp_action does.
    let action = ok(&cli(dir.path(), &["run", "a.hwpx", "MoveDocumentEnd"]));
    assert_eq!(action["result"]["command"], "caret.move", "{action}");
    assert_eq!(
        cli(dir.path(), &["run", "a.hwpx", "document.text", "{bad"])
            .status
            .code(),
        Some(2)
    );
}

#[test]
fn command_and_action_catalogs() {
    let dir = tempfile::tempdir().unwrap();
    let all = ok(&cli(dir.path(), &["commands"]));
    let all = all.as_array().unwrap();
    assert!(all.len() > 400);
    assert!(
        all.iter()
            .all(|c| c.as_array().is_some_and(|c| c.len() == 3))
    );
    let bold = ok(&cli(dir.path(), &["commands", "format.bold", "--json"]));
    let entry = bold
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "format.bold")
        .unwrap();
    assert!(entry["paramsExample"].is_object(), "{entry}");
    assert!(!entry["paramsHelp"].as_str().unwrap().is_empty());
    assert!(!entry["hwpActions"].as_array().unwrap().is_empty());
    let actions = ok(&cli(dir.path(), &["actions", "MoveDocumentEnd"]));
    assert_eq!(actions[0]["command"], "caret.move", "{actions}");
}

#[test]
fn edit_reads_stdin_runs_hangul_actions_and_can_keep_going() {
    let dir = tempfile::tempdir().unwrap();
    let plan = r#"[{"command":"text.insert","params":{"text":"끝"}},
        {"hwpAction":"MoveDocBegin"},
        {"command":"text.insert","params":{"text":"앞"}}]"#;
    ok(&cli_with_stdin(
        dir.path(),
        &["edit", "--new", "-", "a.hwpx"],
        plan,
    ));
    assert_eq!(text(dir.path(), "a.hwpx").trim(), "앞끝");
    // By default a failure saves nothing.
    let plan = r#"[{"command":"no.such"},{"command":"text.insert","params":{"text":"k"}}]"#;
    let stopped = cli_with_stdin(dir.path(), &["edit", "--new", "-", "b.hwpx"], plan);
    assert_eq!(stopped.status.code(), Some(1));
    assert!(!dir.path().join("b.hwpx").exists());
    // --keep-going runs the rest, saves, and still reports the failure.
    let kept = cli_with_stdin(
        dir.path(),
        &["edit", "--new", "-", "c.hwpx", "--keep-going"],
        plan,
    );
    assert_eq!(kept.status.code(), Some(1), "{}", stderr(&kept));
    let report: Value = serde_json::from_slice(&kept.stdout).unwrap();
    assert_eq!(report["failed"], 1);
    assert_eq!(report["results"][0]["ok"], false);
    assert_eq!(text(dir.path(), "c.hwpx").trim(), "k");
    let both = cli_with_stdin(
        dir.path(),
        &["edit", "--new", "-", "d.hwpx"],
        r#"[{"command":"a","hwpAction":"b"}]"#,
    );
    assert!(!both.status.success());
    assert!(stderr(&both).contains("하나만"), "{}", stderr(&both));
}

#[test]
fn render_every_page_at_a_scale() {
    let dir = tempfile::tempdir().unwrap();
    let plan = r#"[{"command":"insert.page_break"},{"command":"insert.page_break"}]"#;
    ok(&cli_with_stdin(
        dir.path(),
        &["edit", "--new", "-", "a.hwpx"],
        plan,
    ));
    ok(&cli(
        dir.path(),
        &["render", "a.hwpx", "p.png", "all", "--scale", "0.5"],
    ));
    let small = image::open(dir.path().join("p-1.png")).unwrap();
    for page in 1..=3 {
        assert!(dir.path().join(format!("p-{page}.png")).exists());
    }
    ok(&cli(
        dir.path(),
        &["render", "a.hwpx", "big.png", "2", "--scale", "1"],
    ));
    let big = image::open(dir.path().join("big.png")).unwrap();
    assert!(big.width() > small.width() * 3 / 2);
    for args in [
        &["render", "a.hwpx", "p.jpg"][..],
        &["render", "a.hwpx", "p.png", "abc"],
        &["render", "a.hwpx", "p.png", "1", "--scale", "9"],
    ] {
        assert_eq!(cli(dir.path(), args).status.code(), Some(2), "{args:?}");
    }
    assert_eq!(
        cli(dir.path(), &["render", "a.hwpx", "p.png", "4"])
            .status
            .code(),
        Some(1)
    );
}

#[test]
fn passwords_can_be_set_changed_and_removed_on_save() {
    let dir = tempfile::tempdir().unwrap();
    ok(&cli(dir.path(), &["new", "a.hwpx", "글"]));
    ok(&cli(
        dir.path(),
        &["convert", "a.hwpx", "p.hwpx", "--new-password", "one"],
    ));
    let locked = cli(dir.path(), &["info", "p.hwpx"]);
    assert_eq!(locked.status.code(), Some(1));
    assert!(
        stderr(&locked).contains("--password"),
        "{}",
        stderr(&locked)
    );
    ok(&cli(
        dir.path(),
        &[
            "convert",
            "p.hwpx",
            "q.hwpx",
            "--password",
            "one",
            "--new-password",
            "two",
        ],
    ));
    assert!(
        !cli(dir.path(), &["info", "q.hwpx", "--password", "one"])
            .status
            .success()
    );
    ok(&cli(
        dir.path(),
        &[
            "convert",
            "q.hwpx",
            "r.hwpx",
            "--password",
            "two",
            "--remove-password",
        ],
    ));
    let info = ok(&cli(dir.path(), &["info", "r.hwpx"]));
    assert_eq!(info["passwordProtected"], false);
    assert_eq!(text(dir.path(), "r.hwpx").trim(), "글");
}

#[test]
fn file_commands_save_through_the_cli_and_point_opening_at_its_arguments() {
    let dir = tempfile::tempdir().unwrap();
    ok(&cli(dir.path(), &["new", "a.hwpx", "글"]));
    // The saves MCP runs itself are saves here too, by command or Hangul action name.
    ok(&cli(
        dir.path(),
        &["run", "a.hwpx", "file.save_as", r#"{"path":"b.hwp"}"#],
    ));
    assert_eq!(text(dir.path(), "b.hwp").trim(), "글");
    let action = ok(&cli(
        dir.path(),
        &["run", "a.hwpx", "FileSaveAs", r#"{"path":"c.hwpx"}"#],
    ));
    assert_eq!(action["result"]["command"], "file.save_as", "{action}");
    assert!(dir.path().join("c.hwpx").exists());
    ok(&cli(
        dir.path(),
        &["run", "a.hwpx", "file.pdf", r#"{"path":"d.pdf"}"#],
    ));
    assert!(dir.path().join("d.pdf").exists());
    let not_pdf = cli(
        dir.path(),
        &["run", "a.hwpx", "file.pdf", r#"{"path":"e.hwpx"}"#],
    );
    assert!(!not_pdf.status.success());
    assert!(!dir.path().join("e.hwpx").exists());
    // A step of an edit plan saves where it stands.
    let plan = r#"[{"command":"text.insert","params":{"text":"앞"}},
        {"command":"file.save_as","params":{"path":"mid.hwpx"}},
        {"command":"text.insert","params":{"text":"뒤"}}]"#;
    ok(&cli_with_stdin(
        dir.path(),
        &["edit", "--new", "-", "end.hwpx"],
        plan,
    ));
    assert_eq!(text(dir.path(), "mid.hwpx").trim(), "앞");
    assert_eq!(text(dir.path(), "end.hwpx").trim(), "앞뒤");
    // Opening and new documents are the command line's input argument.
    for args in [
        &["run", "a.hwpx", "file.open", r#"{"path":"b.hwp"}"#][..],
        &["run", "a.hwpx", "FileNew"],
    ] {
        let out = cli(dir.path(), args);
        assert!(!out.status.success(), "{args:?}");
        assert!(stderr(&out).contains("--new"), "{}", stderr(&out));
    }
}
