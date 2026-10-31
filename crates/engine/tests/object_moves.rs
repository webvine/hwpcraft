//! Owner: shell/page area. `object.move_by`: several objects move together in one undo step.
use hwpcraft_doc::model::control::Control;
use hwpcraft_engine::{Command, ObjectAddress, Session};
use serde_json::json;

fn shapes(s: &Session) -> Vec<ObjectAddress> {
    s.core.document().sections[0]
        .paragraphs
        .iter()
        .enumerate()
        .flat_map(|(p, para)| {
            para.controls
                .iter()
                .enumerate()
                .filter(|(_, c)| matches!(c, Control::Shape(_)))
                .map(move |(c, _)| ObjectAddress {
                    section: 0,
                    paragraph: p,
                    control: c,
                })
        })
        .collect()
}

fn offsets(s: &Session, a: ObjectAddress) -> anyhow::Result<(i64, i64, bool)> {
    let v = s.object_properties(a)?;
    let p = &v["props"];
    Ok((
        p["horzOffset"].as_i64().unwrap_or(0),
        p["vertOffset"].as_i64().unwrap_or(0),
        p["treatAsChar"] == true,
    ))
}

#[test]
fn selected_objects_move_together_in_one_undo_step() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    for _ in 0..2 {
        s.run(
            "insert.shape",
            json!({"kind":"rectangle","width_mm":20,"height_mm":10,"text":""}),
        )?;
    }
    let found = shapes(&s);
    anyhow::ensure!(found.len() == 2, "{found:?}");
    for a in &found {
        if offsets(&s, *a)?.2 {
            s.run(
                "object.properties",
                json!({"address": a, "props": {"treatAsChar": false}}),
            )?;
        }
    }
    let before: Vec<_> = found
        .iter()
        .map(|a| offsets(&s, *a))
        .collect::<anyhow::Result<_>>()?;
    s.run(
        "object.move_by",
        json!({"addresses": found, "dx_mm": 10.0, "dy_mm": 5.0}),
    )?;
    let (dx, dy) = (2835, 1417);
    for (a, (x, y, _)) in found.iter().zip(&before) {
        let (nx, ny, _) = offsets(&s, *a)?;
        assert!(
            (nx - (x + dx)).abs() <= 1 && (ny - (y + dy)).abs() <= 1,
            "{a:?} {x},{y} -> {nx},{ny}"
        );
    }
    // One undo puts both back; the next one undoes the previous command, not half a move.
    s.execute(Command::Undo)?;
    for (a, b) in found.iter().zip(&before) {
        assert_eq!(&offsets(&s, *a)?, b);
    }
    s.execute(Command::Undo)?;
    // A leftover inner step would show the first object moved and the second not.
    let half = (offsets(&s, found[0])?, offsets(&s, found[1]).ok());
    let half_moved = (before[0].0 + dx, before[0].1 + dy, before[0].2);
    assert!(
        !(half.0 == half_moved && half.1.as_ref() == Some(&before[1])),
        "no half move left in the undo history"
    );
    // Refusals happen before anything moves.
    s.execute(Command::Redo)?;
    let now: Vec<_> = found
        .iter()
        .map(|a| offsets(&s, *a))
        .collect::<anyhow::Result<_>>()?;
    let twice = vec![found[0], found[0]];
    assert!(
        s.run(
            "object.move_by",
            json!({"addresses": twice, "dx_mm": 1.0, "dy_mm": 0.0})
        )
        .is_err()
    );
    assert!(
        s.run(
            "object.move_by",
            json!({"addresses": found, "dx_mm": 900.0, "dy_mm": 0.0})
        )
        .is_err()
    );
    let unchanged: Vec<_> = found
        .iter()
        .map(|a| offsets(&s, *a))
        .collect::<anyhow::Result<_>>()?;
    assert_eq!(unchanged, now);
    Ok(())
}
