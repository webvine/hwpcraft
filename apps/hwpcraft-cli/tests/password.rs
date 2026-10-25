//! `--password` on the CLI: protected output, protected input, wrong password refused.
use std::{fs, process::Command};

#[test]
fn password_option_protects_output_and_opens_protected_input() {
    let dir = tempfile::tempdir().unwrap();
    let exe = env!("CARGO_BIN_EXE_hwpcraft-cli");
    for ext in ["hwp", "hwpx"] {
        let secret = dir.path().join(format!("secret.{ext}"));
        let status = Command::new(exe)
            .args([
                "new",
                secret.to_str().unwrap(),
                "비밀 본문",
                "--password",
                "암호123",
            ])
            .status()
            .unwrap();
        assert!(status.success(), "{ext}: new --password");
        assert!(fs::metadata(&secret).unwrap().len() > 0);

        // Without the password the file does not open.
        let plain = Command::new(exe)
            .args(["text", secret.to_str().unwrap()])
            .output()
            .unwrap();
        assert!(!plain.status.success(), "{ext}: plain open must fail");
        assert!(
            String::from_utf8_lossy(&plain.stderr).contains("비밀번호가 필요한"),
            "{ext}: {}",
            String::from_utf8_lossy(&plain.stderr)
        );

        // A wrong password is refused.
        let wrong = Command::new(exe)
            .args(["--password", "틀림", "text", secret.to_str().unwrap()])
            .output()
            .unwrap();
        assert!(!wrong.status.success(), "{ext}: wrong password must fail");

        // The right password reads the text and converts to a protected copy.
        let right = Command::new(exe)
            .args(["--password", "암호123", "text", secret.to_str().unwrap()])
            .output()
            .unwrap();
        assert!(right.status.success(), "{ext}");
        assert!(String::from_utf8_lossy(&right.stdout).contains("비밀 본문"));

        let copy = dir.path().join(format!("copy-{ext}.hwpx"));
        let convert = Command::new(exe)
            .args([
                "convert",
                secret.to_str().unwrap(),
                copy.to_str().unwrap(),
                "--password",
                "암호123",
            ])
            .status()
            .unwrap();
        assert!(convert.success(), "{ext}: convert --password");
        let mut session = hwpcraft_engine::Session::new().unwrap();
        assert!(
            session.open(&copy).is_err(),
            "{ext}: the copy stays protected"
        );
        session.open_with_password(&copy, "암호123").unwrap();
        assert_eq!(session.text(), "비밀 본문");
    }
    // `--password` without a value is an error before anything is touched.
    let missing = Command::new(exe)
        .args(["--version", "--password"])
        .output()
        .unwrap();
    assert!(!missing.status.success());
}
