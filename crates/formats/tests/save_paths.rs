//! A save to a missing folder makes it, but a failed save leaves no new folder behind;
//! a folder or empty path, and a folder given to open, get a plain explanation
//! (MCP 2nd review D3, D7).
use hwpcraft_doc::DocumentCore;

fn blank() -> DocumentCore {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().expect("blank document");
    core
}

#[test]
fn a_failed_save_leaves_no_new_folder() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let core = blank();
    // A missing folder is made for a save that succeeds.
    let good = dir.path().join("새 폴더/깊은 곳/보고서.hwpx");
    hwpcraft_formats::save(&core, &good)?;
    assert!(good.is_file());
    // One that fails takes the folders it made away again.
    let made = dir.path().join("없던 폴더");
    // Invalid on Windows; a NUL is invalid everywhere else.
    let name = if cfg!(windows) {
        "bad<name>.hwpx"
    } else {
        "bad\0name.hwpx"
    };
    let bad = made.join("안쪽").join(name);
    let error = hwpcraft_formats::save(&core, &bad).expect_err("invalid name");
    assert!(!made.exists(), "left behind: {error:#}");
    if cfg!(windows) {
        assert!(
            format!("{error:#}").contains("쓸 수 없는 문자"),
            "{error:#}"
        );
    }
    Ok(())
}

#[test]
fn folders_and_empty_paths_are_named_as_such() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let core = blank();
    let folder = dir.path().join("폴더.hwpx");
    std::fs::create_dir(&folder)?;
    let saved = hwpcraft_formats::save(&core, &folder).expect_err("a folder");
    assert!(saved.to_string().contains("폴더"), "{saved}");
    let empty = hwpcraft_formats::save(&core, std::path::Path::new("")).expect_err("empty");
    assert!(empty.to_string().contains("비어"), "{empty}");
    let opened = hwpcraft_formats::open(&folder)
        .err()
        .expect("a folder does not open");
    assert!(opened.to_string().contains("폴더"), "{opened}");
    Ok(())
}
