/// Diagnostic: `cargo test -p hwpcraft-render --test strokes -- --ignored --nocapture`
/// with HWPCRAFT_PROBE=<file> lists the stroke widths RHWP emits for page 1.
#[test]
#[ignore]
fn probe_stroke_widths() -> anyhow::Result<()> {
    let Ok(path) = std::env::var("HWPCRAFT_PROBE") else {
        return Ok(());
    };
    let core = hwpcraft_formats::open(std::path::Path::new(&path))?;
    let svg = core
        .render_page_svg_native(0)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut widths = std::collections::BTreeMap::<String, usize>::new();
    for part in svg.split("stroke-width=\"").skip(1) {
        if let Some(end) = part.find('"') {
            *widths.entry(part[..end].to_owned()).or_default() += 1;
        }
    }
    println!("{widths:?}");
    Ok(())
}
