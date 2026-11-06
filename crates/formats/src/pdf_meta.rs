//! Document information and bookmarks added to an exported PDF.
//!
//! RHWP writes the pages and names itself as producer, nothing more: no title or author,
//! no bookmarks. Hangul's PDF carries the document's 문서 정보 and turns its outline
//! (개요) into PDF bookmarks. This appends, after the existing bytes (which stay as they
//! are), a new document information dictionary, an outline tree whose items go to their
//! heading's page and line, a copy of the catalog that points at the outline and opens
//! the bookmark panel, and one complete cross-reference table for the whole file, so
//! readers that expect a single table (and `pdf_compact`) still read it.
//!
//! Like [`crate::pdf_compact`], only the classic single-section cross-reference table
//! pdf-writer writes is read; anything else returns the PDF unchanged.
use crate::pdf_compact::{find, start_xref, xref_table};
use std::collections::HashMap;

/// What [`apply`] writes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PdfMeta {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub keywords: Option<String>,
    /// The application that made the document (Info `/Creator`).
    pub creator: String,
    /// The program that wrote the PDF (Info `/Producer`).
    pub producer: String,
    /// `/CreationDate` and `/ModDate`, already in PDF form (`D:20261105093000Z`).
    pub date: Option<String>,
    /// Bookmarks in document order.
    pub outline: Vec<OutlineItem>,
}

/// One bookmark: a heading at `level` (1 is the top), going to `page` (0-based, in the
/// PDF) at `x`, `y` points from the page's top left.
#[derive(Clone, Debug, PartialEq)]
pub struct OutlineItem {
    pub level: u8,
    pub title: String,
    pub page: usize,
    pub x: f32,
    pub y: f32,
}

/// `pdf` with `meta` appended, or `pdf` itself when its layout is not the expected one.
pub fn apply(pdf: Vec<u8>, meta: &PdfMeta) -> Vec<u8> {
    match update(&pdf, meta) {
        Some(tail) => {
            let mut out = pdf;
            out.extend_from_slice(&tail);
            out
        }
        None => pdf,
    }
}

fn update(pdf: &[u8], meta: &PdfMeta) -> Option<Vec<u8>> {
    let xref_at = start_xref(pdf)?;
    let (offsets, trailer) = xref_table(pdf, xref_at)?;
    let root = reference_after(&trailer.dict, b"/Root")?;
    let catalog = dictionary(pdf, &offsets, root)?;
    let mut pages = Vec::new();
    collect_pages(
        pdf,
        &offsets,
        reference_after(&catalog, b"/Pages")?,
        None,
        &mut pages,
        0,
    )?;

    let mut next = trailer.size;
    let mut objects: Vec<(u32, Vec<u8>)> = Vec::new();
    let info = next;
    next += 1;
    objects.push((info, info_dictionary(meta)));
    let items: Vec<&OutlineItem> = meta
        .outline
        .iter()
        .filter(|item| item.page < pages.len() && !item.title.trim().is_empty())
        .collect();
    let mut new_catalog = None;
    if !items.is_empty() {
        // Without the old keys the catalog must still read as a dictionary; when it does
        // not, the PDF gets its information but no bookmarks.
        if let Some(mut text) =
            without_key(&catalog, b"/Outlines").and_then(|text| without_key(&text, b"/PageMode"))
        {
            let outlines = next;
            let (tree, used) = outline_objects(&items, &pages, outlines);
            next += used;
            objects.extend(tree);
            let end = text.iter().rposition(|&b| b == b'>')?.checked_sub(1)?;
            if text.get(end) != Some(&b'>') {
                return None;
            }
            text.splice(
                end..end,
                format!("/Outlines {outlines} 0 R\n/PageMode /UseOutlines\n").into_bytes(),
            );
            new_catalog = Some(text);
        }
    }

    let mut out = Vec::new();
    if !pdf.ends_with(b"\n") {
        out.push(b'\n');
    }
    let mut written: Vec<(u32, usize)> = Vec::new();
    let mut write = |out: &mut Vec<u8>, id: u32, body: &[u8]| {
        written.push((id, pdf.len() + out.len()));
        out.extend_from_slice(format!("{id} 0 obj\n").as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    };
    if let Some(text) = &new_catalog {
        write(&mut out, root, text);
    }
    for (id, body) in &objects {
        write(&mut out, *id, body);
    }
    // One whole table, as pdf-writer and `pdf_compact` write it: the old objects keep
    // their places and the new ones (and the catalog copy) point after them.
    let written: HashMap<u32, usize> = written.into_iter().collect();
    let xref = pdf.len() + out.len();
    out.extend_from_slice(format!("xref\n0 {next}\n").as_bytes());
    for id in 0..next {
        match written.get(&id).or_else(|| offsets.get(&id)) {
            Some(at) => out.extend_from_slice(format!("{at:010} 00000 n\r\n").as_bytes()),
            None if id == 0 => out.extend_from_slice(b"0000000000 65535 f\r\n"),
            None => out.extend_from_slice(b"0000000000 00000 f\r\n"),
        }
    }
    let id = array_after(&trailer.dict, b"/ID")
        .map(|array| format!(" /ID {array}"))
        .unwrap_or_default();
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {next} /Root {root} 0 R /Info {info} 0 R{id} >>\nstartxref\n{xref}\n%%EOF\n"
        )
        .as_bytes(),
    );
    Some(out)
}

/// The dictionary text (`<< … >>`) of object `id`.
fn dictionary(pdf: &[u8], offsets: &HashMap<u32, usize>, id: u32) -> Option<Vec<u8>> {
    let at = *offsets.get(&id)?;
    let raw = pdf.get(at..)?;
    let opener = format!("{id} 0 obj");
    let body = raw.strip_prefix(opener.as_bytes())?;
    let start = find(body, b"<<")?;
    let end = matching_dictionary_end(body, start)?;
    Some(body[start..end].to_vec())
}

/// The end (after `>>`) of the dictionary opening at `start`, skipping strings.
fn matching_dictionary_end(text: &[u8], start: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut i = start;
    while i < text.len() {
        match text[i] {
            b'(' => i = string_end(text, i)?,
            b'<' if text.get(i + 1) == Some(&b'<') => {
                depth += 1;
                i += 2;
                continue;
            }
            b'>' if text.get(i + 1) == Some(&b'>') => {
                depth = depth.checked_sub(1)?;
                i += 2;
                if depth == 0 {
                    return Some(i);
                }
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// The index of the `)` closing the literal string opening at `start`.
fn string_end(text: &[u8], start: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut i = start;
    while i < text.len() {
        match text[i] {
            b'\\' => i += 1,
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Where `key` is used as a name (not the start of a longer name), and the end of it.
fn key_at(text: &[u8], key: &[u8]) -> Option<(usize, usize)> {
    let mut from = 0;
    while let Some(found) = find(&text[from..], key) {
        let start = from + found;
        let end = start + key.len();
        if text.get(end).is_none_or(|&b| !is_regular(b)) {
            return Some((start, end));
        }
        from = end;
    }
    None
}

fn is_regular(b: u8) -> bool {
    !b.is_ascii_whitespace() && !b"()<>[]{}/%".contains(&b)
}

/// The object number of the `n 0 R` reference after `key`.
fn reference_after(text: &[u8], key: &[u8]) -> Option<u32> {
    let (_, end) = key_at(text, key)?;
    let rest = std::str::from_utf8(&text[end..]).ok()?;
    let mut parts = rest.split_ascii_whitespace();
    let id = parts.next()?.parse().ok()?;
    (parts.next()? == "0" && parts.next()?.starts_with('R')).then_some(id)
}

/// The `[ … ]` array text after `key`.
fn array_after(text: &[u8], key: &[u8]) -> Option<String> {
    let (_, end) = key_at(text, key)?;
    let open = end + text[end..].iter().position(|&b| b == b'[')?;
    let close = open + text[open..].iter().position(|&b| b == b']')?;
    String::from_utf8(text[open..=close].to_vec()).ok()
}

/// The numbers of the array after `key`.
fn numbers_after(text: &[u8], key: &[u8]) -> Option<Vec<f32>> {
    let array = array_after(text, key)?;
    array
        .trim_matches(|c| c == '[' || c == ']')
        .split_ascii_whitespace()
        .map(|n| n.parse().ok())
        .collect()
}

/// `text` without `key` and its value (a reference or a name); `text` itself when it
/// has no such key, `None` when the value is something else.
fn without_key(text: &[u8], key: &[u8]) -> Option<Vec<u8>> {
    let Some((start, end)) = key_at(text, key) else {
        return Some(text.to_vec());
    };
    let rest = &text[end..];
    let skip = rest.iter().take_while(|b| b.is_ascii_whitespace()).count();
    let value = &rest[skip..];
    let len = if value.first() == Some(&b'/') {
        let name = 1 + value[1..].iter().take_while(|&&b| is_regular(b)).count();
        name + value[name..]
            .iter()
            .take_while(|b| b.is_ascii_whitespace())
            .count()
    } else {
        // `n g R`
        let mut parts = 0;
        let mut i = 0;
        while parts < 3 {
            let token = value[i..].iter().take_while(|&&b| is_regular(b)).count();
            if token == 0 {
                return None;
            }
            let word = &value[i..i + token];
            let ok = if parts < 2 {
                word.iter().all(u8::is_ascii_digit)
            } else {
                word == b"R"
            };
            if !ok {
                return None;
            }
            i += token;
            parts += 1;
            i += value[i..]
                .iter()
                .take_while(|b| b.is_ascii_whitespace())
                .count();
        }
        i
    };
    let mut out = text[..start].to_vec();
    out.extend_from_slice(&value[len..]);
    Some(out)
}

/// The page objects under `id` in order, with their height in points (`/MediaBox`,
/// inherited from the page tree when the page has none).
fn collect_pages(
    pdf: &[u8],
    offsets: &HashMap<u32, usize>,
    id: u32,
    inherited: Option<f32>,
    pages: &mut Vec<(u32, f32)>,
    depth: usize,
) -> Option<()> {
    if depth > 32 {
        return None;
    }
    let dict = dictionary(pdf, offsets, id)?;
    let height = numbers_after(&dict, b"/MediaBox")
        .filter(|b| b.len() == 4)
        .map(|b| b[3] - b[1])
        .or(inherited);
    if key_at(&dict, b"/Kids").is_some() {
        let kids = array_after(&dict, b"/Kids")?;
        let numbers: Vec<&str> = kids
            .trim_matches(|c| c == '[' || c == ']')
            .split_ascii_whitespace()
            .collect();
        for reference in numbers.chunks(3) {
            let [kid, "0", "R"] = reference else {
                return None;
            };
            collect_pages(pdf, offsets, kid.parse().ok()?, height, pages, depth + 1)?;
        }
    } else {
        pages.push((id, height?));
    }
    Some(())
}

/// A PDF text string: UTF-16BE with its byte order mark, in hex.
fn text_string(text: &str) -> String {
    let mut out = String::from("<FEFF");
    for unit in text.encode_utf16() {
        out.push_str(&format!("{unit:04X}"));
    }
    out.push('>');
    out
}

fn info_dictionary(meta: &PdfMeta) -> Vec<u8> {
    let mut out = String::from("<<\n");
    for (key, value) in [
        ("Title", meta.title.as_deref()),
        ("Author", meta.author.as_deref()),
        ("Subject", meta.subject.as_deref()),
        ("Keywords", meta.keywords.as_deref()),
        ("Creator", Some(meta.creator.as_str())),
        ("Producer", Some(meta.producer.as_str())),
    ] {
        if let Some(value) = value.map(str::trim).filter(|v| !v.is_empty()) {
            out.push_str(&format!("/{key} {}\n", text_string(value)));
        }
    }
    if let Some(date) = &meta.date
        && date.starts_with("D:")
        && date[2..].bytes().all(|b| b.is_ascii_alphanumeric())
    {
        out.push_str(&format!("/CreationDate ({date})\n/ModDate ({date})\n"));
    }
    out.push_str(">>");
    out.into_bytes()
}

/// The outline dictionary (object `first`) and its items (the following numbers), and
/// how many object numbers they use. Top-level items open to show their children;
/// deeper ones start closed.
fn outline_objects(
    items: &[&OutlineItem],
    pages: &[(u32, f32)],
    first: u32,
) -> (Vec<(u32, Vec<u8>)>, u32) {
    let n = items.len();
    let id = |i: usize| first + 1 + i as u32;
    // Parent of each item: the last earlier item with a lower level.
    let mut parent: Vec<Option<usize>> = Vec::with_capacity(n);
    let mut stack: Vec<usize> = Vec::new();
    for (i, item) in items.iter().enumerate() {
        while stack.last().is_some_and(|&s| items[s].level >= item.level) {
            stack.pop();
        }
        parent.push(stack.last().copied());
        stack.push(i);
    }
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut top = Vec::new();
    for (i, p) in parent.iter().enumerate() {
        match p {
            Some(p) => children[*p].push(i),
            None => top.push(i),
        }
    }
    let siblings = |i: usize| -> &[usize] {
        match parent[i] {
            Some(p) => &children[p],
            None => &top,
        }
    };
    let mut objects = Vec::with_capacity(n + 1);
    let shown: usize = top.len() + top.iter().map(|&t| children[t].len()).sum::<usize>();
    let mut root = format!("<<\n/Type /Outlines\n/Count {shown}\n");
    if let (Some(&a), Some(&b)) = (top.first(), top.last()) {
        root.push_str(&format!("/First {} 0 R\n/Last {} 0 R\n", id(a), id(b)));
    }
    root.push_str(">>");
    objects.push((first, root.into_bytes()));
    for (i, item) in items.iter().enumerate() {
        let (page, height) = pages[item.page];
        let mut dict = format!(
            "<<\n/Title {}\n/Parent {} 0 R\n",
            text_string(item.title.trim()),
            parent[i].map_or(first, id)
        );
        let row = siblings(i);
        let at = row.iter().position(|&s| s == i).unwrap_or(0);
        if at > 0 {
            dict.push_str(&format!("/Prev {} 0 R\n", id(row[at - 1])));
        }
        if let Some(&next) = row.get(at + 1) {
            dict.push_str(&format!("/Next {} 0 R\n", id(next)));
        }
        if let (Some(&a), Some(&b)) = (children[i].first(), children[i].last()) {
            let count = children[i].len() as i64;
            let count = if parent[i].is_none() { count } else { -count };
            dict.push_str(&format!(
                "/First {} 0 R\n/Last {} 0 R\n/Count {count}\n",
                id(a),
                id(b)
            ));
        }
        let top = (height - item.y).clamp(0.0, height);
        dict.push_str(&format!(
            "/Dest [{page} 0 R /XYZ {:.2} {top:.2} null]\n>>",
            item.x.max(0.0)
        ));
        objects.push((id(i), dict.into_bytes()));
    }
    (objects, n as u32 + 1)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::pdf_compact::number_after;

    fn pdf(pages: usize) -> Vec<u8> {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="794" height="1123"><rect x="10" y="10" width="100" height="50" fill="#000"/></svg>"##;
        let svgs = vec![svg.to_owned(); pages];
        let links = vec![Vec::new(); pages];
        crate::pdf_compact::compact(
            hwpcraft_doc::svgs_to_pdf_with_links(&svgs, &links, &crate::pdf_options()).unwrap(),
        )
    }

    /// The objects the last cross-reference section points at, by number.
    fn updated(pdf: &[u8]) -> HashMap<u32, String> {
        let xref = start_xref(pdf).unwrap();
        let text = std::str::from_utf8(&pdf[xref..]).unwrap();
        let mut out = HashMap::new();
        let mut lines = text.lines().skip(1);
        while let Some(line) = lines.next() {
            let mut parts = line.split_ascii_whitespace();
            let (Some(first), Some(count)) = (parts.next(), parts.next()) else {
                break;
            };
            let (Ok(first), Ok(count)) = (first.parse::<u32>(), count.parse::<u32>()) else {
                break;
            };
            for id in first..first + count {
                let entry = lines.next().unwrap();
                if entry.trim_end().ends_with('f') {
                    continue;
                }
                let offset: usize = entry[..10].parse().unwrap();
                let end = offset + find(&pdf[offset..], b"endobj").unwrap();
                out.insert(id, String::from_utf8_lossy(&pdf[offset..end]).into_owned());
            }
        }
        out
    }

    fn meta(outline: Vec<OutlineItem>) -> PdfMeta {
        PdfMeta {
            title: Some("2026년 업무 계획".into()),
            author: Some("기획팀".into()),
            subject: None,
            keywords: Some(" ".into()),
            creator: "HwpCraft 0.22".into(),
            producer: "HwpCraft 0.22 (RHWP)".into(),
            date: Some("D:20261105093000Z".into()),
            outline,
        }
    }

    fn item(level: u8, title: &str, page: usize) -> OutlineItem {
        OutlineItem {
            level,
            title: title.into(),
            page,
            x: 75.0,
            y: 100.0,
        }
    }

    #[test]
    fn information_is_appended_after_the_original_bytes() {
        let original = pdf(1);
        let out = apply(original.clone(), &meta(Vec::new()));
        assert!(
            out.starts_with(&original),
            "the original bytes stay as they are"
        );
        let tail = String::from_utf8_lossy(&out[original.len()..]).into_owned();
        // One complete table: every old object keeps its offset (the catalog has a copy).
        let (offsets, _) = xref_table(&out, start_xref(&out).unwrap()).unwrap();
        let (old, _) = xref_table(&original, start_xref(&original).unwrap()).unwrap();
        assert!(old.iter().all(|(id, at)| offsets.get(id) == Some(at)));
        assert!(tail.contains(&format!("/Title {}", text_string("2026년 업무 계획"))));
        assert!(tail.contains("/Author <FEFFAE30D68DD300>"));
        assert!(tail.contains("/CreationDate (D:20261105093000Z)"));
        // Empty values are left out, and nothing points at an outline.
        assert!(!tail.contains("/Keywords") && !tail.contains("/Subject"));
        assert!(!tail.contains("/Outlines"));
        let size = number_after(tail.as_bytes(), b"/Size").unwrap();
        assert_eq!(updated(&out).keys().max().copied(), Some(size - 1));
    }

    #[test]
    fn headings_become_nested_bookmarks_to_their_page_and_line() {
        let original = pdf(3);
        let outline = vec![
            item(1, "Ⅰ. 개요", 0),
            item(2, "1. 배경", 0),
            item(2, "2. 목표", 1),
            item(1, "Ⅱ. 추진 계획", 2),
            item(3, "가. 일정", 2),
            item(1, "다른 문서의 쪽", 7),
        ];
        let out = apply(original, &meta(outline));
        let objects = updated(&out);
        let catalog = objects
            .values()
            .find(|o| o.contains("/Type /Catalog"))
            .expect("a new catalog");
        assert!(catalog.contains("/PageMode /UseOutlines"));
        let root = reference_after(catalog.as_bytes(), b"/Outlines").unwrap();
        let tree = &objects[&root];
        // Two top-level items (the one on a missing page is dropped), each open with
        // its children shown: 2 + 2 + 1.
        assert!(tree.contains("/Count 5"), "{tree}");
        let titled = |title: &str| {
            objects
                .values()
                .find(|o| o.contains(&format!("/Title {}", text_string(title))))
                .unwrap_or_else(|| panic!("{title}"))
                .clone()
        };
        let first = titled("Ⅰ. 개요");
        assert!(first.contains(&format!("/Parent {root} 0 R")) && first.contains("/Count 2"));
        let second = titled("2. 목표");
        assert!(second.contains("/Prev ") && !second.contains("/Next "));
        // A level skipped (1 → 3) still nests under the last higher heading.
        let skipped = titled("가. 일정");
        let parent = reference_after(skipped.as_bytes(), b"/Parent").unwrap();
        assert!(objects[&parent].contains(&text_string("Ⅱ. 추진 계획")));
        // Points from the top left become PDF space (the page is 842.25 pt tall).
        assert!(first.contains("/XYZ 75.00 742.25 null"), "{first}");
        assert!(
            !objects
                .values()
                .any(|o| o.contains(&text_string("다른 문서의 쪽")))
        );
    }

    #[test]
    fn an_unexpected_layout_is_left_alone() {
        let junk = b"%PDF-1.7\nnot really a pdf".to_vec();
        assert_eq!(apply(junk.clone(), &meta(vec![item(1, "a", 0)])), junk);
    }

    #[test]
    fn catalog_keys_are_replaced_not_repeated() {
        let text = b"<<\n/Type /Catalog\n/Pages 2 0 R\n/PageMode /UseNone\n/Outlines 9 0 R\n>>";
        let text = without_key(text, b"/Outlines").unwrap();
        let text = without_key(&text, b"/PageMode").unwrap();
        assert_eq!(
            String::from_utf8_lossy(&text),
            "<<\n/Type /Catalog\n/Pages 2 0 R\n>>"
        );
        assert_eq!(without_key(b"<< /Outlines [1] >>", b"/Outlines"), None);
        assert_eq!(
            key_at(b"/PageModeX 1 /PageMode 2", b"/PageMode"),
            Some((13, 22))
        );
    }
}
