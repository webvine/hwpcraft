//! PDF export stores a picture repeated on several pages once
//! (`hwpcraft_formats::pdf_compact`).
use hwpcraft_engine::{Command, Session};
use hwpcraft_formats::pdf_compact::compact;
use serde_json::json;
use std::collections::HashMap;

fn count(pdf: &[u8], needle: &[u8]) -> usize {
    pdf.windows(needle.len()).filter(|w| *w == needle).count()
}

/// In-use objects from the cross-reference table, each checked to start where the
/// table says, and every reference in object dictionaries resolving to one of them.
fn check_structure(pdf: &[u8]) -> anyhow::Result<usize> {
    let tail = String::from_utf8_lossy(&pdf[pdf.len().saturating_sub(64)..]).into_owned();
    let start: usize = tail
        .split("startxref")
        .nth(1)
        .and_then(|s| s.split_whitespace().next())
        .ok_or_else(|| anyhow::anyhow!("startxref"))?
        .parse()?;
    let table = String::from_utf8_lossy(&pdf[start..]).into_owned();
    let mut lines = table.lines();
    anyhow::ensure!(lines.next() == Some("xref"));
    let size: usize = lines
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .ok_or_else(|| anyhow::anyhow!("xref size"))?
        .parse()?;
    let mut objects = HashMap::new();
    for id in 0..size {
        let entry = lines.next().ok_or_else(|| anyhow::anyhow!("entry {id}"))?;
        let fields: Vec<&str> = entry.split_whitespace().collect();
        if fields.get(2) == Some(&"n") {
            let at: usize = fields[0].parse()?;
            let opener = format!("{id} 0 obj");
            anyhow::ensure!(pdf[at..].starts_with(opener.as_bytes()), "object {id}");
            objects.insert(id, at);
        }
    }
    for (&id, &at) in &objects {
        let body = &pdf[at..];
        let end = [&b"\nstream\n"[..], b"endobj"]
            .iter()
            .filter_map(|k| body.windows(k.len()).position(|w| w == *k))
            .min()
            .ok_or_else(|| anyhow::anyhow!("end of {id}"))?;
        let text = String::from_utf8_lossy(&body[..end]).into_owned();
        let words: Vec<&str> = text.split_whitespace().collect();
        for w in words.windows(3) {
            if w[1] == "0" && (w[2] == "R" || w[2].starts_with("R]") || w[2].starts_with("R>")) {
                let target: usize = w[0].trim_start_matches(['[', '<']).parse()?;
                anyhow::ensure!(objects.contains_key(&target), "{id} -> {target}: {text}");
            }
        }
    }
    Ok(objects.len())
}

#[test]
fn a_picture_on_every_page_is_stored_once() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let png = dir.path().join("logo.png");
    image::RgbImage::from_fn(300, 200, |x, y| {
        image::Rgb([
            (x * 7 % 251) as u8,
            (y * 13 % 241) as u8,
            ((x ^ y) % 256) as u8,
        ])
    })
    .save(&png)?;
    let mut s = Session::new()?;
    for page in 0..3 {
        s.execute(Command::Insert(format!("{}쪽 본문", page + 1)))?;
        s.run("picture.insert", json!({"path":png}))?;
        if page < 2 {
            s.execute(Command::PageBreak)?;
        }
    }
    let pages: Vec<u32> = (0..s.page_count()?).collect();
    assert_eq!(pages.len(), 3);
    let raw = s
        .core
        .render_pages_pdf_native_with_options(&pages, &hwpcraft_formats::pdf_options())
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let small = compact(raw.clone());
    assert_eq!(count(&raw, b"/Subtype /Image"), 3);
    assert_eq!(count(&small, b"/Subtype /Image"), 1);
    assert!(small.len() < raw.len(), "{} -> {}", raw.len(), small.len());
    assert_eq!(
        count(&small, b"/Type /Page\n"),
        count(&raw, b"/Type /Page\n")
    );
    assert!(check_structure(&small)? < check_structure(&raw)?);
    // Nothing is left to merge, and the export paths compact too.
    assert_eq!(compact(small.clone()), small);
    assert_eq!(s.render_pdf(&pages)?, small);
    let path = dir.path().join("out.pdf");
    s.save(&path)?;
    let saved = std::fs::read(&path)?;
    assert_eq!(count(&saved, b"/Subtype /Image"), 1);
    check_structure(&saved)?;
    Ok(())
}

#[test]
fn unexpected_input_is_returned_unchanged() -> anyhow::Result<()> {
    for bytes in [
        b"not a pdf".to_vec(),
        b"%PDF-1.7\nstartxref\n999999\n%%EOF".to_vec(),
        Vec::new(),
    ] {
        assert_eq!(compact(bytes.clone()), bytes);
    }
    // A cut-off export keeps its bytes (no merge is attempted on a broken table).
    let mut s = Session::new()?;
    s.execute(Command::Insert("본문".into()))?;
    let raw = s.render_pdf(&[0])?;
    let cut = raw[..raw.len() - 40].to_vec();
    assert_eq!(compact(cut.clone()), cut);
    Ok(())
}

/// `HWPCRAFT_PDF_IN=<a.pdf> HWPCRAFT_PDF_OUT=<b.pdf> cargo test --release -p
/// hwpcraft-engine --test pdf_compact -- --ignored`: compact an existing export.
#[test]
#[ignore]
fn compact_a_file() -> anyhow::Result<()> {
    let (Some(input), Some(output)) = (
        std::env::var_os("HWPCRAFT_PDF_IN"),
        std::env::var_os("HWPCRAFT_PDF_OUT"),
    ) else {
        return Ok(());
    };
    let raw = std::fs::read(input)?;
    let t = std::time::Instant::now();
    let small = compact(raw.clone());
    eprintln!(
        "{} -> {} bytes in {:?}, {} objects",
        raw.len(),
        small.len(),
        t.elapsed(),
        check_structure(&small)?
    );
    std::fs::write(output, small)?;
    Ok(())
}
