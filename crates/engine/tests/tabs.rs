use hwpcraft_engine::{Command, Session};

fn tab_records(s: &Session) -> Vec<[u16; 7]> {
    s.core.document().sections[0].paragraphs[0]
        .tab_extended
        .clone()
}

#[test]
fn saved_tabs_carry_their_laid_out_width() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("가\t나다\t끝".into()))?;
    let live = tab_records(&s);
    let dir = tempfile::tempdir()?;
    for name in ["tabs.hwp", "tabs.hwpx"] {
        let path = dir.path().join(name);
        s.save(&path)?;
        // The live model is unchanged, so later edits still re-flow tabs.
        assert_eq!(tab_records(&s), live);
        let mut reopened = Session::new()?;
        reopened.open(&path)?;
        let saved = tab_records(&reopened);
        assert_eq!(saved.len(), 2, "{name}");
        for record in &saved {
            let width = u32::from(record[0]) | (u32::from(record[1]) << 16);
            // A default 40 pt tab grid gives tabs of a few millimetres to 40 pt.
            assert!(width > 100 && width <= 4000, "{name}: width {width}");
            assert_eq!(record[2] >> 8, 1, "{name}: left tab");
        }
    }
    Ok(())
}

#[test]
fn right_centre_and_decimal_tabs_record_their_stop_kind() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("a\t오른쪽\t가운데\t12.5".into()))?;
    s.execute(Command::Paragraph(serde_json::json!({"tabStops": [
        {"position": 40000, "type": 1, "fill": 0},
        {"position": 60000, "type": 2, "fill": 0},
        {"position": 80000, "type": 3, "fill": 3}
    ]})))?;
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("kinds.hwp");
    s.save(&path)?;
    let mut reopened = Session::new()?;
    reopened.open(&path)?;
    let saved = tab_records(&reopened);
    let kinds: Vec<u16> = saved.iter().map(|r| r[2] >> 8).collect();
    assert_eq!(kinds, [2, 3, 4]);
    assert_eq!(saved[2][2] & 0xFF, 3, "decimal tab keeps its dotted fill");
    for record in &saved {
        assert!(u32::from(record[0]) | (u32::from(record[1]) << 16) > 0);
    }
    Ok(())
}
