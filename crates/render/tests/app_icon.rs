//! The app icon rasterizes without system fonts at every packaging size, with a
//! transparent rounded corner and an opaque body (straight alpha).
const ICON: &str = include_str!("../../../assets/icon.svg");

#[test]
fn app_icon_rasterizes_at_package_sizes_with_transparent_corners() -> anyhow::Result<()> {
    for size in [16, 32, 64, 128, 256, 512, 1024] {
        let rgba = hwpcraft_render::rasterize_icon(ICON, size)?;
        assert_eq!(rgba.len(), (size * size * 4) as usize, "{size}");
        let at = |x: u32, y: u32| {
            let i = ((y * size + x) * 4) as usize;
            [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
        };
        assert!(
            at(0, 0)[3] < 64,
            "{size}: rounded corner is (nearly) transparent"
        );
        let centre_edge = at(2, size / 2);
        assert_eq!(centre_edge[3], 255, "{size}: body is opaque");
        // #234c7d background, not darkened by premultiplication.
        assert!(
            (i32::from(centre_edge[0]) - 0x23).abs() <= 2
                && (i32::from(centre_edge[2]) - 0x7d).abs() <= 2,
            "{size}: {centre_edge:?}"
        );
    }
    assert!(hwpcraft_render::rasterize_icon(ICON, 8).is_err());
    assert!(hwpcraft_render::rasterize_icon(ICON, 4096).is_err());
    assert!(hwpcraft_render::rasterize_icon("<svg", 64).is_err());
    Ok(())
}
