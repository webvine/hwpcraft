use serde_json::json;
use std::{fs, process::Command};

#[test]
fn demo_preserves_title_body_and_cell_styles_after_reopening() {
    let dir = tempfile::tempdir().unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_hwpcraft-cli"))
        .arg("demo")
        .arg(dir.path())
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    for extension in ["hwp", "hwpx"] {
        let mut session = hwpcraft_engine::Session::new().unwrap();
        session
            .open(&dir.path().join(format!("HwpCraft-시작하기.{extension}")))
            .unwrap();
        for (needle, size, bold, color) in [
            ("HwpCraft · 한글 문서 작업실", 2400, true, Some("#234c7d")),
            ("문서를 내 컴퓨터에서", 1200, false, Some("#263445")),
            ("RHWP / Rust", 1000, false, None),
        ] {
            let cursor = session
                .cursors()
                .into_iter()
                .find(|c| session.paragraph(*c).unwrap().text.starts_with(needle))
                .unwrap_or_else(|| panic!("{extension}: sample is missing {needle}"));
            let length = session.paragraph(cursor).unwrap().text.chars().count();
            session.select(cursor, false).unwrap();
            session
                .select(
                    hwpcraft_engine::Cursor {
                        offset: length,
                        ..cursor
                    },
                    true,
                )
                .unwrap();
            let shape = session.character_properties().unwrap();
            assert_eq!(shape["fontSize"], size, "{extension}: {needle}");
            assert_eq!(shape["bold"], bold, "{extension}: {needle}");
            if let Some(color) = color {
                assert_eq!(shape["textColor"], color, "{extension}: {needle}");
            }
            assert_eq!(shape["fontFamily"], "Noto Sans KR", "{extension}: {needle}");
        }
    }
}

#[test]
fn cli_edit_accepts_public_parameter_sets_without_exposing_file_aliases() {
    let dir = tempfile::tempdir().unwrap();
    let plan = dir.path().join("parameters.json");
    let output = dir.path().join("parameters.hwpx");
    fs::write(&plan,serde_json::to_vec(&json!([
        {"command":"text.insert","params":{"text":"공개 인자 CLI"}},
        {"command":"select.all"},
        {"command":"hwp.parameters.apply","params":{"id":"CharShape","parameterSet":{"Height":1600,"Bold":1,"TextColor":0x563412}}}
    ])).unwrap()).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_hwpcraft-cli"))
        .args(["edit", "--new"])
        .arg(&plan)
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let mut session = hwpcraft_engine::Session::new().unwrap();
    session.open(&output).unwrap();
    let shape = session.character_properties().unwrap();
    assert_eq!(shape["fontSize"], 1600);
    assert_eq!(shape["bold"], true);
    assert_eq!(shape["textColor"], "#123456");
    let before = fs::read(&output).unwrap();
    fs::write(&plan,serde_json::to_vec(&json!([
        {"command":"hwp.parameters.apply","params":{"id":"FileOpen","parameterSet":{"filename":"unused"}}}
    ])).unwrap()).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_hwpcraft-cli"))
        .args(["edit", "--new"])
        .arg(&plan)
        .arg(&output)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert_eq!(fs::read(output).unwrap(), before);
}

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
