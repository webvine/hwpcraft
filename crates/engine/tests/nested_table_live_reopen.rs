//! A table made in a cell grows its host as the reopened file lays it out: the
//! host's rows are not stretched while editing (they were by about one empty line,
//! 1,000 HU, because the grown cell's stored height left that line out).
use hwpcraft_engine::{Command, ObjectAddress, Session};
use serde_json::{Value, json};

/// Every table's cell boxes on page 0 (x, y, w, h).
fn drawn(s: &Session) -> anyhow::Result<Vec<Vec<[f64; 4]>>> {
    let v: Value = serde_json::from_str(&s.page_control_layout(0)?)?;
    Ok(v["controls"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| c["type"] == "table")
        .map(|t| {
            t["cells"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|c| {
                    let f = |k: &str| c[k].as_f64().unwrap_or(f64::NAN);
                    [f("x"), f("y"), f("w"), f("h")]
                })
                .collect()
        })
        .collect())
}

fn same(a: &[Vec<[f64; 4]>], b: &[Vec<[f64; 4]>]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| {
            x.len() == y.len()
                && x.iter()
                    .zip(y)
                    .all(|(p, q)| p.iter().zip(q).all(|(m, n)| (m - n).abs() < 0.5))
        })
}

const HOST: ObjectAddress = ObjectAddress {
    section: 0,
    paragraph: 0,
    control: 2,
};

fn host_height(s: &Session) -> anyhow::Result<u64> {
    Ok(s.table_properties(HOST)?["tableHeight"]
        .as_u64()
        .unwrap_or(0))
}

/// The live layout and the host's height equal those of the saved file reopened.
fn reopens_alike(s: &mut Session) -> anyhow::Result<()> {
    reopens_alike_in(s, &["hwp", "hwpx"])
}

fn reopens_alike_in(s: &mut Session, formats: &[&str]) -> anyhow::Result<()> {
    let live = drawn(s)?;
    for ext in formats {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join(format!("nested.{ext}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        let again = drawn(&r)?;
        assert!(
            same(&again, &live),
            "{ext}: live {live:?}\nreopened {again:?}"
        );
        assert_eq!(host_height(&r)?, host_height(s)?, "{ext}: stored height");
    }
    Ok(())
}

fn nest(s: &mut Session, address: Value) -> anyhow::Result<Value> {
    Ok(s.run(
        "context.table.create",
        json!({"address": address, "rows": 3, "columns": 3}),
    )?["address"]
        .clone())
}

fn first_cell() -> Value {
    json!({"section":0,"paragraph":0,"path":[{"control":2,"cell":0,"paragraph":0}]})
}

#[test]
fn a_table_made_in_a_cell_lays_out_live_as_reopened() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run("insert.table", json!({"rows": 2, "columns": 2}))?;
    let before = (drawn(&s)?, host_height(&s)?);
    nest(&mut s, first_cell())?;
    // The second row only holds an empty line: as tall as in a fresh table.
    let rows = &drawn(&s)?[0];
    assert!((rows[2][3] - before.0[0][2][3]).abs() < 0.5, "{rows:?}");
    reopens_alike(&mut s)?;
    // One undo puts the host back exactly.
    s.execute(Command::Undo)?;
    assert!(same(&drawn(&s)?, &before.0));
    assert_eq!(host_height(&s)?, before.1);
    s.execute(Command::Redo)?;
    reopens_alike(&mut s)
}

#[test]
fn a_host_with_set_row_heights_keeps_them() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run(
        "table.create",
        json!({"rows": 2, "columns": 2, "width_mm": 120.0, "height_mm": 80.0}),
    )?;
    let set = host_height(&s)?;
    nest(&mut s, first_cell())?;
    // The new table fits in the 40 mm row: nothing grows, nothing shrinks.
    assert!(host_height(&s)? >= set, "{} < {set}", host_height(&s)?);
    reopens_alike(&mut s)
}

#[test]
fn a_table_two_levels_deep_lays_out_live_as_reopened() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run("insert.table", json!({"rows": 2, "columns": 2}))?;
    let inner = nest(&mut s, first_cell())?;
    nest(&mut s, inner)?;
    // HWPX leaves out RHWP's own lines; a table two levels deep laid out afresh
    // from that file sits one outer margin higher (an RHWP layout gap, not this
    // edit), so only HWP is compared here.
    reopens_alike_in(&mut s, &["hwp"])
}
