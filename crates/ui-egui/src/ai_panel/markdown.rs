//! The Markdown the CLIs write in their answers, drawn as plain panel text:
//! headings, bullet and numbered lists, `**bold**`, `` `code` `` and fenced
//! code blocks. Anything else stays as written. Every line wraps to the panel.
use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId, Ui};

/// One line (or code block) of an answer.
#[derive(Debug, PartialEq)]
pub(crate) enum Block {
    Heading(String),
    /// Marker ("•", "1.") and text.
    Item(String, String),
    Paragraph(String),
    Code(String),
    Blank,
}

/// Split an answer into blocks.
pub(crate) fn blocks(text: &str) -> Vec<Block> {
    let mut out = Vec::new();
    let mut code: Option<Vec<&str>> = None;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            match code.take() {
                Some(lines) => out.push(Block::Code(lines.join("\n"))),
                None => code = Some(Vec::new()),
            }
            continue;
        }
        if let Some(lines) = code.as_mut() {
            lines.push(line);
            continue;
        }
        let t = trimmed.trim_end();
        out.push(if t.is_empty() {
            Block::Blank
        } else if let Some(h) = t.strip_prefix('#') {
            Block::Heading(h.trim_start_matches('#').trim().to_owned())
        } else if let Some(rest) = ["- ", "* ", "• "].iter().find_map(|m| t.strip_prefix(m)) {
            Block::Item("•".to_owned(), rest.to_owned())
        } else if let Some((n, rest)) = numbered(t) {
            Block::Item(n, rest.to_owned())
        } else {
            Block::Paragraph(t.to_owned())
        });
    }
    // An unclosed fence still shows its lines.
    if let Some(lines) = code {
        out.push(Block::Code(lines.join("\n")));
    }
    out
}

/// "12. text" → ("12.", "text").
fn numbered(t: &str) -> Option<(String, &str)> {
    let digits = t.chars().take_while(char::is_ascii_digit).count();
    let rest = t.get(digits..)?;
    (digits > 0 && digits < 4)
        .then(|| rest.strip_prefix(". "))
        .flatten()
        .map(|r| (format!("{}.", &t[..digits]), r))
}

/// Inline spans: (text, bold, code).
pub(crate) fn spans(text: &str) -> Vec<(String, bool, bool)> {
    let mut out: Vec<(String, bool, bool)> = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let bold = rest.find("**");
        let code = rest.find('`');
        let (at, marker) = match (bold, code) {
            (Some(b), Some(c)) if c < b => (c, "`"),
            (Some(b), _) => (b, "**"),
            (None, Some(c)) => (c, "`"),
            (None, None) => break,
        };
        let after = &rest[at + marker.len()..];
        let Some(end) = after.find(marker) else {
            break;
        };
        if at > 0 {
            out.push((rest[..at].to_owned(), false, false));
        }
        out.push((after[..end].to_owned(), marker == "**", marker == "`"));
        rest = &after[end + marker.len()..];
    }
    if !rest.is_empty() {
        out.push((rest.to_owned(), false, false));
    }
    out
}

fn line_job(ui: &Ui, text: &str, size: f32, strong: bool, width: f32) -> LayoutJob {
    let body = FontId::proportional(size);
    let mono = FontId::monospace(size - 1.0);
    let normal = if strong {
        ui.visuals().strong_text_color()
    } else {
        ui.visuals().text_color()
    };
    let mut job = LayoutJob::default();
    for (s, bold, code) in spans(text) {
        let format = if code {
            TextFormat {
                font_id: mono.clone(),
                color: normal,
                background: Color32::from_rgb(238, 240, 244),
                ..Default::default()
            }
        } else {
            TextFormat {
                font_id: body.clone(),
                color: if bold {
                    ui.visuals().strong_text_color()
                } else {
                    normal
                },
                ..Default::default()
            }
        };
        job.append(&s, 0.0, format);
    }
    job.wrap.max_width = width;
    job
}

/// Draw an answer.
pub(crate) fn show(ui: &mut Ui, text: &str) {
    let size = egui::TextStyle::Body.resolve(ui.style()).size;
    for block in blocks(text) {
        let width = ui.available_width();
        match block {
            Block::Heading(h) => {
                ui.label(line_job(ui, &h, size + 1.0, true, width));
            }
            Block::Paragraph(p) => {
                ui.label(line_job(ui, &p, size, false, width));
            }
            Block::Item(marker, t) => {
                ui.horizontal_top(|ui| {
                    ui.add_space(4.0);
                    ui.label(marker);
                    let width = ui.available_width();
                    ui.label(line_job(ui, &t, size, false, width));
                });
            }
            Block::Code(c) => {
                egui::Frame::new()
                    .fill(Color32::from_rgb(246, 247, 249))
                    .corner_radius(6)
                    .inner_margin(egui::Margin::symmetric(8, 6))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.add(egui::Label::new(egui::RichText::new(c).monospace().small()).wrap());
                    });
            }
            Block::Blank => ui.add_space(2.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_split_into_headings_lists_paragraphs_and_code() {
        let text = "## 결과\n표를 넣었습니다.\n\n- 머리글: 단계\n* 칸 3개\n2. 둘째\n```\nlet x = 1;\n```\n2024. 연도는 문단";
        assert_eq!(
            blocks(text),
            [
                Block::Heading("결과".into()),
                Block::Paragraph("표를 넣었습니다.".into()),
                Block::Blank,
                Block::Item("•".into(), "머리글: 단계".into()),
                Block::Item("•".into(), "칸 3개".into()),
                Block::Item("2.".into(), "둘째".into()),
                Block::Code("let x = 1;".into()),
                Block::Paragraph("2024. 연도는 문단".into()),
            ]
        );
        // An unclosed fence keeps its lines.
        assert_eq!(blocks("```\na"), [Block::Code("a".into())]);
    }

    #[test]
    fn bold_and_code_spans_unmarked_text_stays() {
        assert_eq!(
            spans("제목을 **굵게** 하고 `table.create` 실행"),
            [
                ("제목을 ".to_owned(), false, false),
                ("굵게".to_owned(), true, false),
                (" 하고 ".to_owned(), false, false),
                ("table.create".to_owned(), false, true),
                (" 실행".to_owned(), false, false),
            ]
        );
        // An unpaired marker is shown as written.
        assert_eq!(spans("2 * 3 **x"), [("2 * 3 **x".to_owned(), false, false)]);
        assert_eq!(spans(""), []);
    }
}
