/// Diagnostic: HWPCRAFT_PROBE=<abs path> cargo test -p hwpcraft-engine --test tab_probe -- --ignored --nocapture
#[test]
#[ignore]
fn print_tab_records() -> anyhow::Result<()> {
    let Ok(path) = std::env::var("HWPCRAFT_PROBE") else {
        return Ok(());
    };
    let mut s = hwpcraft_engine::Session::new()?;
    s.open(std::path::Path::new(&path))?;
    for (i, p) in s.core.document().sections[0].paragraphs.iter().enumerate() {
        if p.text.contains('\t') {
            println!("para {i}: {:?} {:?}", p.text, p.tab_extended);
            for o in 0..=p.text.chars().count() {
                let c = s.caret_at(hwpcraft_engine::Cursor {
                    paragraph: i,
                    offset: o,
                    ..Default::default()
                })?;
                print!("{o}:{:.0} ", c["x"].as_f64().unwrap_or(-1.0));
            }
            println!();
        }
    }
    Ok(())
}
