//! docx2hwp <in.docx> <out> [hwpx]: Word → HWP (out .hwp/.hwpx/.pdf); `hwpx` also writes
//! the raw converted package next to `out` for inspection.
use anyhow::{Context, Result};
use std::path::Path;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let input = Path::new(args.get(1).context("in")?);
    let output = Path::new(args.get(2).context("out")?);
    let bytes = std::fs::read(input)?;
    let t = std::time::Instant::now();
    if args.get(3).is_some_and(|a| a == "hwpx") {
        let (hwpx, _) = hwpcraft_docx::docx_to_hwpx(&bytes, None)?;
        std::fs::write(output.with_extension("raw.hwpx"), hwpx)?;
    }
    let imported = hwpcraft_docx::import(&bytes, None)?;
    let took = t.elapsed();
    hwpcraft_formats::save(&imported.core, output)?;
    println!(
        "{} pages={} import={:?} omitted={:?}",
        input.display(),
        imported.core.page_count(),
        took,
        imported.omitted
    );
    Ok(())
}
