//! Merge byte-identical objects in an exported PDF.
//!
//! RHWP converts every page on its own (svg2pdf `to_chunk`) and then concatenates the
//! chunks, so a picture drawn on every page is stored once per page, and so is every
//! font subset two pages happen to share. On the 381-page handbook that was 21 MB of
//! a 46 MB file. Two objects with the same dictionary (after earlier merges) and the
//! same stream bytes mean the same thing, so references to the later one can point at
//! the first; repeating that until nothing changes also merges the font dictionaries
//! and descriptors that refer to merged font files.
//!
//! Only resources are merged: streams and font, descriptor, graphics state, pattern
//! and shading dictionaries. Pages, the page tree, the catalog, annotations, outline
//! items and anything that points back at a parent keep their identity. Stream bytes
//! are never touched; references are rewritten only in dictionary text, outside
//! strings. Kept objects keep their numbers and merged ones become free entries.
//!
//! The input is the classic single-section cross-reference table pdf-writer writes.
//! Anything else, or any object that does not parse as expected, returns the PDF
//! unchanged.
use std::collections::HashMap;

/// Merge rounds; each round can only merge objects that refer to merged ones.
const MAX_ROUNDS: usize = 8;

struct Object {
    id: u32,
    /// Dictionary (or other non-stream) text between `obj` and `stream`/`endobj`.
    head: Vec<u8>,
    /// Stream bytes and the stream's place in the input, if any.
    stream: Option<std::ops::Range<usize>>,
    mergeable: bool,
}

/// The PDF with duplicate resources merged, or `pdf` itself when nothing was merged
/// or its layout is not the expected one.
pub fn compact(pdf: Vec<u8>) -> Vec<u8> {
    match plan(&pdf) {
        Some((objects, header_end, trailer)) => {
            rewrite(&pdf, &objects, header_end, &trailer).unwrap_or(pdf)
        }
        None => pdf,
    }
}

struct Trailer {
    size: u32,
    /// Trailer dictionary text (between `trailer` and `startxref`).
    dict: Vec<u8>,
}

fn plan(pdf: &[u8]) -> Option<(Vec<Object>, usize, Trailer)> {
    let xref_at = start_xref(pdf)?;
    let (offsets, trailer) = xref_table(pdf, xref_at)?;
    let mut order: Vec<(usize, u32)> = offsets.iter().map(|(&id, &at)| (at, id)).collect();
    order.sort_unstable();
    let header_end = order.first()?.0;
    let mut objects = Vec::with_capacity(order.len());
    for (i, &(at, id)) in order.iter().enumerate() {
        let end = order.get(i + 1).map_or(xref_at, |next| next.0);
        objects.push(object(pdf, id, at, end)?);
    }
    Some((objects, header_end, trailer))
}

/// The byte offset after `startxref` at the end of the file.
fn start_xref(pdf: &[u8]) -> Option<usize> {
    let tail_start = pdf.len().saturating_sub(64);
    let tail = &pdf[tail_start..];
    let at = find(tail, b"startxref")? + b"startxref".len();
    let digits: String = tail[at..]
        .iter()
        .map(|&b| b as char)
        .skip_while(|c| c.is_ascii_whitespace())
        .take_while(char::is_ascii_digit)
        .collect();
    let offset: usize = digits.parse().ok()?;
    pdf.get(offset..)?.starts_with(b"xref").then_some(offset)
}

/// In-use object offsets and the trailer of a one-section xref table.
fn xref_table(pdf: &[u8], at: usize) -> Option<(HashMap<u32, usize>, Trailer)> {
    let text = &pdf[at..];
    let mut lines = text.split(|&b| b == b'\n');
    if lines.next()?.trim_ascii() != b"xref" {
        return None;
    }
    let section = std::str::from_utf8(lines.next()?).ok()?;
    let mut parts = section.split_ascii_whitespace();
    let (first, count): (u32, u32) = (parts.next()?.parse().ok()?, parts.next()?.parse().ok()?);
    if first != 0 {
        return None;
    }
    let mut offsets = HashMap::new();
    for id in 0..count {
        let entry = std::str::from_utf8(lines.next()?).ok()?;
        let mut fields = entry.split_ascii_whitespace();
        let (offset, generation, kind) = (fields.next()?, fields.next()?, fields.next()?);
        if kind == "n" {
            if generation != "00000" {
                return None;
            }
            offsets.insert(id, offset.parse().ok()?);
        }
    }
    let rest = &text[text.len().min(find(text, b"trailer")?)..];
    let dict_start = find(rest, b"<<")?;
    let dict_end = find(rest, b"startxref")?;
    let dict = rest.get(dict_start..dict_end)?.trim_ascii_end().to_vec();
    let size = number_after(&dict, b"/Size")?;
    if size != count {
        return None;
    }
    Some((offsets, Trailer { size, dict }))
}

fn object(pdf: &[u8], id: u32, at: usize, end: usize) -> Option<Object> {
    let raw = pdf.get(at..end)?;
    let opener = format!("{id} 0 obj");
    let body_start = raw.starts_with(opener.as_bytes()).then_some(opener.len())?;
    let body = raw.get(body_start..)?.trim_ascii_end();
    let body = body.strip_suffix(b"endobj")?;
    let (head, stream) = match find(body, b"\nstream\n") {
        Some(s) if body[..s].trim_ascii_start().starts_with(b"<<") => {
            let head = &body[..s];
            let length = number_after(head, b"/Length")? as usize;
            let data_start = at + body_start + s + b"\nstream\n".len();
            let data_end = data_start.checked_add(length)?;
            if !pdf.get(data_end..end)?.starts_with(b"\nendstream") {
                return None;
            }
            (head, Some(data_start..data_end))
        }
        _ => (body, None),
    };
    let mergeable = stream.is_some() || resource_dictionary(head);
    let mergeable = mergeable && !keeps_identity(head);
    Some(Object {
        id,
        head: head.to_vec(),
        stream,
        mergeable,
    })
}

fn resource_dictionary(head: &[u8]) -> bool {
    [
        &b"/Type /Font"[..],
        b"/Type /FontDescriptor",
        b"/Type /ExtGState",
        b"/Type /Pattern",
        b"/PatternType",
        b"/ShadingType",
    ]
    .iter()
    .any(|key| find(head, key).is_some())
}

/// Objects whose identity matters: tree nodes, annotations, back references.
fn keeps_identity(head: &[u8]) -> bool {
    [
        &b"/Type /Page"[..], // also /Pages
        b"/Type /Catalog",
        b"/Type /Annot",
        b"/Subtype /Link",
        b"/Parent",
        b"/Kids",
        b"/Next",
        b"/Prev",
        b"/First",
        b"/Last",
        b"/P ",
    ]
    .iter()
    .any(|key| find(head, key).is_some())
}

/// Merge until stable; returns the map from merged id to the id kept for it.
fn merges(pdf: &[u8], objects: &[Object]) -> HashMap<u32, u32> {
    let mut map: HashMap<u32, u32> = HashMap::new();
    for _ in 0..MAX_ROUNDS {
        let mut seen: HashMap<(Vec<u8>, u64, usize), Vec<&Object>> = HashMap::new();
        let mut added = false;
        for o in objects.iter().filter(|o| o.mergeable) {
            if map.contains_key(&o.id) {
                continue;
            }
            let data = o.stream.clone().map_or(&[][..], |r| &pdf[r]);
            let key = (renumber(&o.head, &map), hash(data), data.len());
            let same = seen.entry(key).or_default();
            // Equal hashes are confirmed byte for byte.
            if let Some(kept) = same
                .iter()
                .find(|k| k.stream.clone().map_or(&[][..], |r| &pdf[r]) == data)
            {
                map.insert(o.id, kept.id);
                added = true;
            } else {
                same.push(o);
            }
        }
        if !added {
            break;
        }
    }
    map
}

fn rewrite(
    pdf: &[u8],
    objects: &[Object],
    header_end: usize,
    trailer: &Trailer,
) -> Option<Vec<u8>> {
    let map = merges(pdf, objects);
    if map.is_empty() {
        return None;
    }
    let mut out = Vec::with_capacity(pdf.len());
    out.extend_from_slice(&pdf[..header_end]);
    let mut offsets: HashMap<u32, usize> = HashMap::new();
    for o in objects.iter().filter(|o| !map.contains_key(&o.id)) {
        offsets.insert(o.id, out.len());
        out.extend_from_slice(format!("{} 0 obj", o.id).as_bytes());
        out.extend_from_slice(&renumber(&o.head, &map));
        if let Some(range) = o.stream.clone() {
            out.extend_from_slice(b"\nstream\n");
            out.extend_from_slice(&pdf[range]);
            out.extend_from_slice(b"\nendstream");
        }
        out.extend_from_slice(b"\nendobj\n\n");
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n", trailer.size).as_bytes());
    for id in 0..trailer.size {
        match offsets.get(&id) {
            Some(at) => out.extend_from_slice(format!("{at:010} 00000 n\r\n").as_bytes()),
            None if id == 0 => out.extend_from_slice(b"0000000000 65535 f\r\n"),
            None => out.extend_from_slice(b"0000000000 00000 f\r\n"),
        }
    }
    out.extend_from_slice(b"trailer\n");
    out.extend_from_slice(&renumber(&trailer.dict, &map));
    out.extend_from_slice(format!("\nstartxref\n{xref}\n%%EOF").as_bytes());
    Some(out)
}

/// `text` with every `N 0 R` outside strings pointed through `map`.
fn renumber(text: &[u8], map: &HashMap<u32, u32>) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        match text[i] {
            b'(' => {
                // Literal string: balanced parentheses, backslash escapes.
                let start = i;
                let mut depth = 0usize;
                while i < text.len() {
                    match text[i] {
                        b'\\' => i += 1,
                        b'(' => depth += 1,
                        b')' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    i += 1;
                }
                i = (i + 1).min(text.len());
                out.extend_from_slice(&text[start..i]);
            }
            b'<' if text.get(i + 1) == Some(&b'<') => {
                out.extend_from_slice(b"<<");
                i += 2;
            }
            b'<' => {
                let end = text[i..]
                    .iter()
                    .position(|&b| b == b'>')
                    .map_or(text.len(), |p| i + p + 1);
                out.extend_from_slice(&text[i..end]);
                i = end;
            }
            b'0'..=b'9' if i == 0 || !is_regular(text[i - 1]) => {
                let digits = text[i..].iter().take_while(|b| b.is_ascii_digit()).count();
                let after = &text[i + digits..];
                let reference =
                    after.starts_with(b" 0 R") && after.get(4).is_none_or(|&b| !is_regular(b));
                let id = std::str::from_utf8(&text[i..i + digits])
                    .ok()
                    .and_then(|s| s.parse::<u32>().ok());
                match (reference, id.and_then(|id| kept_for(map, id))) {
                    (true, Some(kept)) => out.extend_from_slice(kept.to_string().as_bytes()),
                    _ => out.extend_from_slice(&text[i..i + digits]),
                }
                i += digits;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    out
}

/// The object finally kept for a merged `id`. A kept object can itself be merged in a
/// later round (once its own references were merged), so follow the chain.
fn kept_for(map: &HashMap<u32, u32>, id: u32) -> Option<u32> {
    let mut kept = *map.get(&id)?;
    for _ in 0..=MAX_ROUNDS {
        match map.get(&kept) {
            Some(&next) => kept = next,
            None => break,
        }
    }
    Some(kept)
}

/// A PDF regular character (not white space or a delimiter).
fn is_regular(b: u8) -> bool {
    !b.is_ascii_whitespace() && !b"()<>[]{}/%".contains(&b)
}

fn number_after(text: &[u8], key: &[u8]) -> Option<u32> {
    let mut at = 0;
    // `/Length` must not match `/Length1`: the name has to end at the key.
    while let Some(found) = find(&text[at..], key) {
        let end = at + found + key.len();
        if text.get(end).is_some_and(|&b| b.is_ascii_whitespace()) {
            let digits: String = text[end..]
                .iter()
                .map(|&b| b as char)
                .skip_while(|c| c.is_ascii_whitespace())
                .take_while(char::is_ascii_digit)
                .collect();
            return digits.parse().ok();
        }
        at = end;
    }
    None
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn hash(data: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    data.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_are_renumbered_only_outside_strings() {
        let map = HashMap::from([(25, 11), (11, 4), (7, 3)]);
        let text = b"<<\n  /XObject <<\n    /xo0 25 0 R\n  >>\n  /A [7 0 R 70 0 R 17 0 R]\n  /T (see 25 0 R \\) 7 0 R)\n  /H <25 0 R>\n  /N /x25 0 R\n>>";
        let out = renumber(text, &map);
        assert_eq!(
            String::from_utf8_lossy(&out),
            "<<\n  /XObject <<\n    /xo0 4 0 R\n  >>\n  /A [3 0 R 70 0 R 17 0 R]\n  /T (see 25 0 R \\) 7 0 R)\n  /H <25 0 R>\n  /N /x25 0 R\n>>"
        );
    }

    #[test]
    fn length_does_not_match_length1() {
        assert_eq!(
            number_after(b"/Length1 900 /Length 12", b"/Length"),
            Some(12)
        );
        assert_eq!(number_after(b"/Length1 900", b"/Length"), None);
    }
}
