use hwpcraft_formats::file_access::{Access, AutomationRoots, check_read, check_write, confine};

#[test]
fn confinement_checks_components_traversal_and_restores_scope() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("work");
    let sibling = dir.path().join("work2");
    std::fs::create_dir(&root).unwrap();
    std::fs::create_dir(&sibling).unwrap();
    let roots = AutomationRoots::new(Some(&root), Some(&root))
        .unwrap()
        .unwrap();
    let path = |p: std::path::PathBuf| p.to_string_lossy().into_owned();
    assert!(
        roots
            .check(&path(root.join("new/deep/doc.hwpx")), Access::Write)
            .is_ok()
    );
    for bad in [
        sibling.join("doc.hwpx"),
        root.join("../work2/doc.hwpx"),
        root.join("missing/../../work2/doc.hwpx"),
    ] {
        assert!(roots.check(&path(bad), Access::Write).is_err());
    }
    confine(Some(&roots), || {
        assert!(check_read(&path(sibling.join("x"))).is_err());
        assert!(check_write(&path(root.join("ok"))).is_ok());
    });
    assert!(check_read(&path(sibling.join("x"))).is_ok());
    let write_only = AutomationRoots::new(None, Some(&root)).unwrap().unwrap();
    assert!(
        write_only
            .check(&path(root.join("x")), Access::Read)
            .is_err()
    );
    assert!(AutomationRoots::new(Some(&dir.path().join("missing")), None).is_err());
    #[cfg(windows)]
    for device in ["NUL", "CON.txt", "COM1"] {
        assert!(
            roots
                .check(&path(root.join(device)), Access::Write)
                .is_err()
        );
    }
}

#[test]
fn links_outside_roots_are_rejected_when_platform_allows_links() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("root");
    let outside = dir.path().join("outside");
    std::fs::create_dir(&root).unwrap();
    std::fs::create_dir(&outside).unwrap();
    let link = root.join("link");
    #[cfg(windows)]
    let made = std::os::windows::fs::symlink_dir(&outside, &link);
    #[cfg(unix)]
    let made = std::os::unix::fs::symlink(&outside, &link);
    #[cfg(not(any(windows, unix)))]
    let made: std::io::Result<()> = Err(std::io::ErrorKind::Unsupported.into());
    if let Err(e) = made {
        eprintln!("symlink test skipped: {e}");
        return;
    }
    let roots = AutomationRoots::new(Some(&root), Some(&root))
        .unwrap()
        .unwrap();
    assert!(
        roots
            .check(&link.join("doc.hwpx").to_string_lossy(), Access::Write)
            .is_err()
    );
    assert!(
        roots
            .check(&link.join("doc.hwpx").to_string_lossy(), Access::Read)
            .is_err()
    );
}

#[test]
fn forbidden_writes_override_any_scope_and_end_with_the_guard() {
    use hwpcraft_formats::file_access::{forbid_writes, unconfined};
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("out.txt");
    let path = target.to_string_lossy().into_owned();
    {
        let _outer = forbid_writes();
        {
            let _inner = forbid_writes();
            assert!(unconfined(|| check_write(&path)).is_err());
        }
        assert!(check_write(&path).is_err());
        assert!(check_read(&path).is_ok());
        assert!(hwpcraft_formats::atomic_write(&target, b"x").is_err());
        assert!(!target.exists());
    }
    assert!(check_write(&path).is_ok());
    hwpcraft_formats::atomic_write(&target, b"x").unwrap();
}

#[test]
fn a_cancelled_request_keeps_the_existing_file() {
    use hwpcraft_formats::file_access::{cancel_scope, cancelled};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("report.pdf");
    std::fs::write(&target, b"original").unwrap();
    let flag = Arc::new(AtomicBool::new(false));
    {
        let check = flag.clone();
        let _scope = cancel_scope(move || check.load(Ordering::SeqCst));
        assert!(!cancelled());
        hwpcraft_formats::atomic_write(&target, b"first").unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"first");
        flag.store(true, Ordering::SeqCst);
        assert!(cancelled());
        assert!(hwpcraft_formats::atomic_write(&target, b"second").is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"first");
    }
    assert!(!cancelled());
    let leftovers = std::fs::read_dir(dir.path()).unwrap().count();
    assert_eq!(leftovers, 1, "temporary file must not remain");
    hwpcraft_formats::atomic_write(&target, b"third").unwrap();
    assert_eq!(std::fs::read(&target).unwrap(), b"third");
}
