//! The Korean font HwpCraft picks must come from the OS candidate list and, when a
//! file-backed face exists, point at a readable font file (used for the egui UI font).
#[test]
fn default_korean_font_is_an_os_candidate_or_an_installed_fallback() {
    let family = hwpcraft_render::default_korean_font();
    let mut db = resvg::usvg::fontdb::Database::new();
    db.load_system_fonts();
    let installed = |name: &str| {
        db.faces()
            .any(|f| f.families.iter().any(|(n, _)| n == name))
    };
    let candidate = hwpcraft_doc::fonts::KOREAN_CANDIDATES
        .iter()
        .find(|name| installed(name));
    match candidate {
        // A Korean font is installed: it is the one used.
        Some(name) => {
            assert_eq!(family, *name);
            assert!(hwpcraft_render::korean_font_installed());
        }
        // None installed (minimal Linux): an installed font, so text still renders.
        None => {
            assert!(
                installed(family) || db.is_empty(),
                "{family} is not installed"
            );
            assert!(!hwpcraft_render::korean_font_installed());
        }
    }
    // Probing is cached: a second call returns the same name.
    assert_eq!(hwpcraft_render::default_korean_font(), family);
    // The PDF fallback name is also an OS candidate (English alias on Windows).
    let fallback = hwpcraft_doc::fonts::default_family();
    assert!(hwpcraft_doc::fonts::KOREAN_CANDIDATES.contains(&fallback));
}

#[test]
fn korean_font_face_points_at_a_readable_file_when_present() {
    if let Some((path, index)) = hwpcraft_render::korean_font_face() {
        let bytes = std::fs::read(&path).expect("font file must be readable");
        assert!(
            bytes.len() > 1024,
            "{} is too small to be a font",
            path.display()
        );
        // Collections (.ttc) carry an index; single fonts use 0.
        let collection = bytes.starts_with(b"ttcf");
        assert!(
            collection || index == 0,
            "{}: index {index}",
            path.display()
        );
    } else if cfg!(windows) {
        panic!("Windows always ships 맑은 고딕; the face should resolve");
    }
}
