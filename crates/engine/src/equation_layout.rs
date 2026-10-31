//! Equation layout for visual editing: the boxes RHWP's equation renderer draws,
//! each tied back to the script text that produced it, and the empty `{}` slots
//! a template leaves to fill.
//!
//! RHWP's layout tree carries no source positions, so drawn pieces are matched to
//! script tokens in order: a box takes the next token (within a short look-ahead)
//! that spells it. Pieces without a match (a lone `over`, implicit spacing, a
//! legacy hwpeq script rewritten before parsing) keep `start`/`end` as `None`.
//! Coordinates are in the units of `equation_preview`'s SVG view box.

use hwpcraft_doc::equation::layout::{EqLayout, LayoutBox, LayoutKind};
use hwpcraft_doc::equation::parser::EqParser;
use hwpcraft_doc::equation::symbols::{DECORATIONS, lookup_function, lookup_symbol};
use hwpcraft_doc::equation::tokenizer::{Token, TokenType, Tokenizer, tokenize};
use serde::Serialize;

use crate::{EquationOptions, Result, Session};

/// How many tokens ahead a drawn piece may find its source.
const LOOK_AHEAD: usize = 12;

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct EquationLayoutItem {
    /// text, number, symbol, math_symbol, function, big_op, limit, sqrt,
    /// paren_left, paren_right, decoration or slot
    pub kind: &'static str,
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// Script range in Unicode scalar values.
    pub start: Option<usize>,
    pub end: Option<usize>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct EquationLayout {
    pub width: f64,
    pub height: f64,
    pub baseline: f64,
    pub items: Vec<EquationLayoutItem>,
    /// Script offsets just inside each empty `{}`, in script order.
    pub slots: Vec<usize>,
}

struct Matcher<'a> {
    tokens: &'a [Token],
    lengths: Vec<usize>,
    next: usize,
    font_size: f64,
}

impl Matcher<'_> {
    /// Claim the next `{}` pair within the look-ahead; the offset inside it.
    fn claim_slot(&mut self) -> Option<usize> {
        let end = (self.next + LOOK_AHEAD).min(self.tokens.len().saturating_sub(1));
        let i = (self.next..end).find(|&i| {
            self.tokens[i].ty == TokenType::LBrace && self.tokens[i + 1].ty == TokenType::RBrace
        })?;
        self.next = i + 2;
        Some(self.tokens[i].pos + 1)
    }

    /// Claim the next token within the look-ahead that `accept` takes.
    fn claim(&mut self, accept: impl Fn(&Token) -> bool) -> Span {
        let end = (self.next + LOOK_AHEAD).min(self.tokens.len());
        for i in self.next..end {
            if accept(&self.tokens[i]) {
                self.next = i + 1;
                let start = self.tokens[i].pos;
                return (Some(start), Some(start + self.lengths[i]));
            }
        }
        (None, None)
    }
}

fn command_is(token: &Token, names: &[&str]) -> bool {
    token.ty == TokenType::Command && names.iter().any(|n| token.value.eq_ignore_ascii_case(n))
}

fn spells_symbol(token: &Token, symbol: &str) -> bool {
    token.value == symbol || lookup_symbol(&token.value) == Some(symbol)
}

fn spells_text(token: &Token, text: &str) -> bool {
    matches!(
        token.ty,
        TokenType::Text | TokenType::Number | TokenType::Quoted | TokenType::Command
    ) && (token.value == text || (!text.is_empty() && token.value.contains(text)))
}

type Span = (Option<usize>, Option<usize>);

fn push(
    out: &mut Vec<EquationLayoutItem>,
    b: &LayoutBox,
    (x, y): (f64, f64),
    kind: &'static str,
    text: &str,
    span: Span,
) {
    out.push(EquationLayoutItem {
        kind,
        text: text.to_owned(),
        x,
        y,
        width: b.width,
        height: b.height,
        start: span.0,
        end: span.1,
    });
}

/// A vertical strip of `b` (a bracket or radical sign beside its body).
fn sliver(
    b: &LayoutBox,
    (x, y): (f64, f64),
    from: f64,
    width: f64,
    kind: &'static str,
    text: &str,
    span: Span,
) -> EquationLayoutItem {
    EquationLayoutItem {
        kind,
        text: text.to_owned(),
        x: x + from,
        y,
        width: width.max(0.0),
        height: b.height,
        start: span.0,
        end: span.1,
    }
}

/// A slot box centred on `centre`, starting at `top`, for the next `{}` pair.
fn slot_at(m: &mut Matcher, out: &mut Vec<EquationLayoutItem>, centre: f64, top: f64) {
    if let Some(slot) = m.claim_slot() {
        let (w, h) = (m.font_size * 0.5, m.font_size * 0.8);
        out.push(EquationLayoutItem {
            kind: "slot",
            text: String::new(),
            x: (centre - w / 2.0).max(0.0),
            y: top.max(0.0),
            width: w,
            height: h,
            start: Some(slot),
            end: Some(slot),
        });
    }
}

fn walk(b: &LayoutBox, ox: f64, oy: f64, m: &mut Matcher, out: &mut Vec<EquationLayoutItem>) {
    let (x, y) = (ox + b.x, oy + b.y);
    match &b.kind {
        LayoutKind::Row(children) => {
            for c in children {
                walk(c, x, y, m, out);
            }
        }
        LayoutKind::Text(t) => {
            let span = m.claim(|k| spells_text(k, t));
            push(out, b, (x, y), "text", t, span);
        }
        LayoutKind::Number(t) => {
            let span = m.claim(|k| spells_text(k, t));
            push(out, b, (x, y), "number", t, span);
        }
        LayoutKind::Symbol(s) => {
            let span = m.claim(|k| spells_symbol(k, s));
            push(out, b, (x, y), "symbol", s, span);
        }
        LayoutKind::MathSymbol(s) => {
            let span = m.claim(|k| spells_symbol(k, s));
            push(out, b, (x, y), "math_symbol", s, span);
        }
        LayoutKind::Function(f) => {
            let span = m.claim(|k| {
                k.ty == TokenType::Command
                    && (k.value.eq_ignore_ascii_case(f) || lookup_function(&k.value) == Some(f))
            });
            push(out, b, (x, y), "function", f, span);
        }
        LayoutKind::Fraction { numer, denom } => {
            // An empty numerator or denominator has no height: put its slot
            // above or below the bar instead of on it.
            let bar = y + (numer.y + numer.height + denom.y) / 2.0;
            let centre = x + b.width / 2.0;
            let h = m.font_size * 0.8;
            match numer.kind {
                LayoutKind::Empty => slot_at(m, out, centre, bar - h),
                _ => walk(numer, x, y, m, out),
            }
            m.claim(|k| command_is(k, &["over"]));
            match denom.kind {
                LayoutKind::Empty => slot_at(m, out, centre, bar),
                _ => walk(denom, x, y, m, out),
            }
        }
        LayoutKind::Atop { top, bottom } => {
            walk(top, x, y, m, out);
            m.claim(|k| command_is(k, &["atop"]));
            walk(bottom, x, y, m, out);
        }
        LayoutKind::Sqrt { index, body } => {
            let span = m.claim(|k| command_is(k, &["sqrt", "root"]));
            out.push(sliver(b, (x, y), 0.0, body.x, "sqrt", "√", span));
            if let Some(i) = index {
                walk(i, x, y, m, out);
                m.claim(|k| command_is(k, &["of"]));
            }
            walk(body, x, y, m, out);
        }
        LayoutKind::Superscript { base, sup } => {
            walk(base, x, y, m, out);
            walk(sup, x, y, m, out);
        }
        LayoutKind::Subscript { base, sub } => {
            walk(base, x, y, m, out);
            walk(sub, x, y, m, out);
        }
        LayoutKind::SubSup { base, sub, sup } => {
            walk(base, x, y, m, out);
            walk(sub, x, y, m, out);
            walk(sup, x, y, m, out);
        }
        LayoutKind::BigOp { symbol, sub, sup } => {
            let span = m.claim(|k| k.ty == TokenType::Command && spells_symbol(k, symbol));
            push(out, b, (x, y), "big_op", symbol, span);
            for part in [sub, sup].into_iter().flatten() {
                walk(part, x, y, m, out);
            }
        }
        LayoutKind::Limit { is_upper, sub } => {
            let span = m.claim(|k| command_is(k, &["lim"]));
            push(
                out,
                b,
                (x, y),
                "limit",
                if *is_upper { "Lim" } else { "lim" },
                span,
            );
            if let Some(s) = sub {
                walk(s, x, y, m, out);
            }
        }
        LayoutKind::Matrix { cells, .. } => {
            for c in cells.iter().flatten() {
                walk(c, x, y, m, out);
            }
        }
        LayoutKind::Rel { arrow, over, under } => {
            m.claim(|k| command_is(k, &["rel", "buildrel"]));
            walk(arrow, x, y, m, out);
            walk(over, x, y, m, out);
            if let Some(u) = under {
                walk(u, x, y, m, out);
            }
        }
        LayoutKind::EqAlign { rows } => {
            for (l, r) in rows {
                walk(l, x, y, m, out);
                walk(r, x, y, m, out);
            }
        }
        LayoutKind::Paren { left, right, body } => {
            // `LEFT ( … RIGHT )`, a `cases` brace, or a plain `( … )` sized to its body.
            let span = m.claim(|k| command_is(k, &["left", "cases"]) || k.value == *left);
            let mut item = sliver(b, (x, y), 0.0, body.x, "paren_left", left, span);
            out.push(item.clone());
            walk(body, x, y, m, out);
            if !right.is_empty() {
                let span = m.claim(|k| command_is(k, &["right"]) || k.value == *right);
                let from = body.x + body.width;
                item = sliver(b, (x, y), from, b.width - from, "paren_right", right, span);
                out.push(item);
            }
        }
        LayoutKind::Decoration { body, .. } => {
            let span = m.claim(|k| {
                k.ty == TokenType::Command
                    && (DECORATIONS.contains_key(k.value.as_str())
                        || DECORATIONS.contains_key(k.value.to_ascii_lowercase().as_str()))
            });
            push(out, b, (x, y), "decoration", "", span);
            walk(body, x, y, m, out);
        }
        LayoutKind::FontStyle { body, .. } => walk(body, x, y, m, out),
        // An empty group draws nothing; its zero-size box marks where it sits.
        LayoutKind::Empty => slot_at(m, out, x, y - m.font_size * 0.4),
        LayoutKind::Space(_) | LayoutKind::Newline => {}
        #[allow(unreachable_patterns)]
        _ => {}
    }
}

/// Source length of each token: quotes count, a command's text is its value.
fn token_lengths(script: &[char], tokens: &[Token]) -> Vec<usize> {
    tokens
        .iter()
        .map(|t| match t.ty {
            TokenType::Quoted => {
                let inner = t.value.chars().count();
                let closed = script.get(t.pos + 1 + inner) == Some(&'"');
                inner + 1 + usize::from(closed)
            }
            TokenType::Eof => 0,
            _ => t.value.chars().count().max(1),
        })
        .collect()
}

/// Offsets just inside every `{` whose next token is its `}`.
fn empty_slots(tokens: &[Token]) -> Vec<usize> {
    tokens
        .windows(2)
        .filter(|w| w[0].ty == TokenType::LBrace && w[1].ty == TokenType::RBrace)
        .map(|w| w[0].pos + 1)
        .collect()
}

pub(crate) fn layout_script(script: &str, font_size: f64) -> EquationLayout {
    let parsed = EqParser::new(tokenize(script)).parse();
    let root = EqLayout::new(font_size).layout(&parsed);
    // Positions come from the raw script; a legacy hwpeq script is rewritten
    // before parsing, so its positions would not point into the text.
    let raw = Tokenizer::new(script).tokenize();
    let chars: Vec<char> = script.chars().collect();
    let legacy = tokenize(script) != raw;
    let tokens: &[Token] = if legacy { &[] } else { &raw };
    let mut matcher = Matcher {
        tokens,
        lengths: token_lengths(&chars, tokens),
        next: 0,
        font_size,
    };
    let mut items = Vec::new();
    walk(&root, 0.0, 0.0, &mut matcher, &mut items);
    let slots = empty_slots(tokens);
    // A slot the layout gave no box (its group was dropped): place it after the
    // item drawn just before it, so a click or Tab can still reach it.
    for &slot in &slots {
        if items
            .iter()
            .any(|i| i.kind == "slot" && i.start == Some(slot))
        {
            continue;
        }
        let before = items
            .iter()
            .filter(|i| i.end.is_some_and(|e| e <= slot))
            .max_by_key(|i| i.end);
        let (x, y, h) = before.map_or((0.0, 0.0, root.height), |i| (i.x + i.width, i.y, i.height));
        items.push(EquationLayoutItem {
            kind: "slot",
            text: String::new(),
            x,
            y,
            width: font_size * 0.5,
            height: h,
            start: Some(slot),
            end: Some(slot),
        });
    }
    EquationLayout {
        width: root.width,
        height: root.height,
        baseline: root.baseline,
        items,
        slots,
    }
}

impl Session {
    /// Layout of an equation script as the preview draws it, for visual editing.
    pub fn equation_layout(&self, options: &EquationOptions) -> Result<EquationLayout> {
        let size = crate::objects::equation_size(options)?;
        let px =
            hwpcraft_doc::equation::hwpunit_to_px(size as i32, hwpcraft_doc::equation::DEFAULT_DPI);
        Ok(layout_script(&options.script, px))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spans(script: &str) -> Vec<(&'static str, String, Option<String>)> {
        let chars: Vec<char> = script.chars().collect();
        layout_script(script, 20.0)
            .items
            .into_iter()
            .map(|i| {
                let src = i
                    .start
                    .zip(i.end)
                    .map(|(s, e)| chars[s..e].iter().collect());
                (i.kind, i.text, src)
            })
            .collect()
    }

    #[test]
    fn drawn_pieces_point_back_into_the_script() {
        let got = spans("{a+b} over {c+d}");
        let texts: Vec<_> = got
            .iter()
            .map(|(_, t, s)| (t.as_str(), s.as_deref()))
            .collect();
        assert_eq!(
            texts,
            [
                ("a", Some("a")),
                ("+", Some("+")),
                ("b", Some("b")),
                ("c", Some("c")),
                ("+", Some("+")),
                ("d", Some("d"))
            ]
        );
        let got = spans("sqrt {alpha} times 2");
        assert!(
            got.contains(&("sqrt", "√".into(), Some("sqrt".into()))),
            "{got:?}"
        );
        assert!(
            got.iter()
                .any(|(_, t, s)| t == "α" && s.as_deref() == Some("alpha")),
            "{got:?}"
        );
        assert!(
            got.iter()
                .any(|(_, t, s)| t == "×" && s.as_deref() == Some("times")),
            "{got:?}"
        );
    }

    #[test]
    fn empty_braces_become_slots_in_order() {
        let script = "{} over {}";
        let layout = layout_script(script, 20.0);
        assert_eq!(layout.slots, [1, 9]);
        let slot_items: Vec<_> = layout.items.iter().filter(|i| i.kind == "slot").collect();
        assert_eq!(slot_items.len(), 2);
        let sum = layout_script("sum_{} ^{} x", 20.0);
        assert_eq!(sum.slots.len(), 2);
    }
}
