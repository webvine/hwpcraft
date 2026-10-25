#[test]
fn font_families_are_unique_with_korean_first() {
    let names = hwpcraft_render::Renderer::default().font_families();
    let mut sorted = names.clone();
    sorted.dedup();
    assert_eq!(sorted.len(), names.len());
    assert!(names.iter().all(|n| !n.is_empty() && !n.starts_with('@')));
    let korean = |s: &str| s.chars().any(|c| ('\u{AC00}'..='\u{D7A3}').contains(&c));
    if let Some(first_other) = names.iter().position(|n| !korean(n)) {
        assert!(names[first_other..].iter().all(|n| !korean(n)));
    }
    // Windows ships Malgun Gothic; its Korean name is listed on Korean systems.
    if cfg!(windows) {
        assert!(
            names
                .iter()
                .any(|n| n == "Malgun Gothic" || n == "맑은 고딕")
        );
    }
}
