//! Owner: core area. 문자표 catalogue: sets, KS table, search, font-tagged insertion.
use hwpcraft_engine::{Command, Session, SymbolGroup, SymbolInsert};
use serde_json::json;

#[test]
fn catalogue_covers_every_group_without_duplicates_or_control_characters() {
    let sets = hwpcraft_engine::symbol_sets();
    for group in SymbolGroup::ALL {
        assert!(
            sets.iter().any(|s| s.group == group),
            "{group:?} has no set"
        );
    }
    let mut ids = std::collections::BTreeSet::new();
    for set in &sets {
        assert!(ids.insert(set.id), "duplicate set id {}", set.id);
        assert!(set.count > 0, "{} is empty", set.id);
        let chars = hwpcraft_engine::symbol_chars(set.id).unwrap();
        assert_eq!(chars.len(), set.count, "{}", set.id);
        assert!(chars.iter().all(|c| !c.is_control()), "{}", set.id);
        if set.group != SymbolGroup::Unicode {
            let unique: std::collections::BTreeSet<_> = chars.iter().collect();
            assert_eq!(unique.len(), chars.len(), "{} repeats a character", set.id);
        }
    }
    // More areas than Hangul 2024's HNC table (which lists under 30).
    assert!(sets.iter().filter(|s| s.group == SymbolGroup::Hnc).count() >= 30);
    assert!(
        sets.iter()
            .filter(|s| s.group == SymbolGroup::Unicode)
            .count()
            >= 150
    );
    assert!(hwpcraft_engine::symbol_chars("no.such").is_none());
}

#[test]
fn ks_x_1001_rows_match_the_standard_counts() {
    let hangul = hwpcraft_engine::symbol_chars("ks.hangul").unwrap();
    assert_eq!(hangul.len(), 2350);
    assert_eq!(hangul[0], '가');
    assert_eq!(*hangul.last().unwrap(), '힝');
    let hanja = hwpcraft_engine::symbol_chars("ks.hanja").unwrap();
    assert_eq!(hanja.len(), 4888);
    let symbols = hwpcraft_engine::symbol_chars("ks.symbols").unwrap();
    assert!(symbols.contains(&'※') && symbols.contains(&'①') && symbols.contains(&'㈜'));
    assert_eq!(hwpcraft_engine::ks_code('가'), Some([0xB0, 0xA1]));
    assert_eq!(hwpcraft_engine::ks_code('😀'), None);
}

#[test]
fn search_finds_code_points_names_and_keywords() {
    let s = hwpcraft_engine::symbol_search("U+2605");
    assert_eq!(s.chars.first(), Some(&'★'));
    assert_eq!(
        hwpcraft_engine::symbol_search("2605").chars.first(),
        Some(&'★')
    );
    assert_eq!(
        hwpcraft_engine::symbol_search("#9733").chars.first(),
        Some(&'★')
    );
    assert_eq!(hwpcraft_engine::symbol_search("★").chars, vec!['★']);
    let dash = hwpcraft_engine::symbol_search("줄표");
    assert!(dash.chars.contains(&'\u{2014}') && dash.chars.contains(&'\u{2013}'));
    let arrows = hwpcraft_engine::symbol_search("화살표");
    assert!(arrows.sets.iter().any(|s| s.id == "hnc.arrows"));
    assert!(
        arrows.chars.is_empty(),
        "Korean words are keywords, not characters"
    );
    let emoji = hwpcraft_engine::symbol_search("emoji flag");
    assert!(emoji.sets.iter().any(|s| s.id == "emoji.flags"));
    assert!(hwpcraft_engine::symbol_search("   ").chars.is_empty());
    assert!(
        hwpcraft_engine::symbol_search("zzzz-no-such")
            .sets
            .is_empty()
    );
    assert_eq!(hwpcraft_engine::block_of('가'), Some("한글 음절"));
    assert!(
        hwpcraft_engine::special_chars()
            .iter()
            .any(|c| c.code == "U+00A0")
    );
}

#[test]
fn insert_symbol_applies_font_in_one_undo_step_and_rejects_bad_input() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("앞".into()))?;
    let r = s.insert_symbol(&SymbolInsert {
        text: "★☆".into(),
        font: Some("Segoe UI Symbol".into()),
    })?;
    assert_eq!(r, 1..3);
    assert_eq!(s.text(), "앞★☆");
    s.cursor.offset = 2;
    assert_eq!(s.character_properties()?["fontFamily"], "Segoe UI Symbol");
    s.cursor.offset = 0;
    assert_ne!(s.character_properties()?["fontFamily"], "Segoe UI Symbol");
    s.execute(Command::Undo)?;
    assert_eq!(s.text(), "앞");

    // Selection is replaced; no font keeps the surrounding one.
    s.anchor = Some(0);
    s.cursor.offset = 1;
    s.insert_symbol(&SymbolInsert {
        text: "※".into(),
        font: None,
    })?;
    assert_eq!(s.text(), "※");

    let before = (s.generation, s.text().to_owned());
    for bad in [
        SymbolInsert {
            text: String::new(),
            font: None,
        },
        SymbolInsert {
            text: "a\nb".into(),
            font: None,
        },
        SymbolInsert {
            text: "★".into(),
            font: Some("  ".into()),
        },
        SymbolInsert {
            text: "x".repeat(1001),
            font: None,
        },
    ] {
        assert!(s.insert_symbol(&bad).is_err());
    }
    assert_eq!((s.generation, s.text().to_owned()), before);
    Ok(())
}

#[test]
fn protocol_lists_pages_searches_and_inserts() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    let sets = s.run("symbol.sets", json!({}))?;
    assert!(sets.as_array().unwrap().len() > 180);
    let page = s.run(
        "symbol.list",
        json!({"set": "hnc.circled", "start": 0, "limit": 3}),
    )?;
    assert_eq!(page["chars"].as_array().unwrap().len(), 3);
    assert_eq!(page["chars"][0]["ch"], "①");
    assert!(page["total"].as_u64().unwrap() > 100);
    assert!(
        s.run("symbol.list", json!({"set": "hnc.circled", "limit": 0}))
            .is_err()
    );
    assert!(s.run("symbol.list", json!({"set": "nope"})).is_err());
    let found = s.run("symbol.search", json!({"query": "U+00A9"}))?;
    assert_eq!(found["chars"][0], "©");
    let special = s.run("symbol.special", json!({}))?;
    assert!(special.as_array().unwrap().len() >= 40);
    let r = s.run("insert.symbol", json!({"text": "①", "font": "Arial"}))?;
    assert_eq!(r["end"], 1);
    assert_eq!(s.text(), "①");
    assert!(
        s.run("insert.symbol", json!({"text": "①", "bogus": 1}))
            .is_err()
    );
    let temp = tempfile::tempdir()?;
    for ext in ["hwp", "hwpx"] {
        let path = temp.path().join(format!("symbol.{ext}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        assert_eq!(r.text(), "①", "{ext}");
        assert_eq!(r.character_properties()?["fontFamily"], "Arial", "{ext}");
    }
    Ok(())
}
