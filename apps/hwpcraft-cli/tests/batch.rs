use std::{fs, process::Command};

#[test]
fn failed_batch_preserves_existing_output_and_source() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.hwpx");
    let output = dir.path().join("output.hwpx");
    let plan = dir.path().join("plan.json");
    let exe = env!("CARGO_BIN_EXE_hwpcraft-cli");
    assert!(
        Command::new(exe)
            .args(["new", source.to_str().unwrap(), "원본 문서"])
            .status()
            .unwrap()
            .success()
    );
    let original = fs::read(&source).unwrap();
    fs::write(&output, b"existing output").unwrap();
    fs::write(&plan, r#"[{"command":"text.insert","params":{"text":"추가"}},{"command":"page.setup","params":{"width_mm":1}}]"#).unwrap();
    let result = Command::new(exe)
        .args([
            "edit",
            source.to_str().unwrap(),
            plan.to_str().unwrap(),
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert_eq!(fs::read(&source).unwrap(), original);
    assert_eq!(fs::read(&output).unwrap(), b"existing output");
    assert!(String::from_utf8_lossy(&result.stderr).contains("출력 파일은 저장하지 않았습니다"));
}

#[test]
fn batch_page_break_and_replacement_survive_saved_document() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("output.hwpx");
    let plan = dir.path().join("plan.json");
    fs::write(&plan, r#"[{"command":"text.insert","params":{"text":"첫 페이지"}},{"command":"insert.page_break"},{"command":"text.insert","params":{"text":"두 번째 페이지"}},{"command":"edit.replace_all","params":{"query":"페이지","replacement":"쪽"}},{"command":"page.setup","params":{"landscape":true}}]"#).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_hwpcraft-cli"))
        .args([
            "edit",
            "--new",
            plan.to_str().unwrap(),
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let mut session = hwpcraft_engine::Session::new().unwrap();
    session.open(&output).unwrap();
    assert_eq!(session.core.page_count(), 2);
    assert!(session.page_setup().unwrap().landscape);
    let text = session
        .entries()
        .iter()
        .map(|(cursor, _)| session.paragraph(*cursor).unwrap().text.clone())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("첫 쪽"));
    assert!(text.contains("두 번째 쪽"));
}
