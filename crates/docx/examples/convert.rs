//! convert <in> <out> [old]: HWP/HWPX → DOCX with the WordCraft exporter (or the old one).
use anyhow::{Context, Result};
use std::path::Path;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let input = Path::new(args.get(1).context("in")?);
    let output = Path::new(args.get(2).context("out")?);
    let old = args.get(3).is_some_and(|a| a == "old");
    if args.get(3).is_some_and(|a| a == "pdf") {
        let core = hwpcraft_formats::open(input)?;
        hwpcraft_formats::save(&core, output)?;
        println!("{} pages={}", input.display(), core.page_count());
        return Ok(());
    }
    let t = std::time::Instant::now();
    let core = hwpcraft_formats::open(input)?;
    let opened = t.elapsed();
    let t = std::time::Instant::now();
    let (bytes, omitted) = if old {
        let e = hwpcraft_formats::docx_export::export(&core)?;
        (e.bytes, e.omitted)
    } else {
        let e = hwpcraft_docx::export(&core)?;
        (e.bytes, e.omitted)
    };
    std::fs::write(output, &bytes)?;
    println!(
        "{} pages={} open={:?} export={:?} bytes={} omitted={:?}",
        input.display(),
        core.page_count(),
        opened,
        t.elapsed(),
        bytes.len(),
        omitted
    );
    Ok(())
}
