use hwpcraft_engine::{Command, Cursor, FindDirection, FindOptions, Session};
use serde_json::json;

fn at(paragraph: usize, offset: usize) -> Cursor {
    Cursor {
        paragraph,
        offset,
        ..Default::default()
    }
}

#[test]
fn find_options_case_word_and_direction() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("Rust rusty RUST\n한글 rust".into()))?;
    s.select(at(0, 0), false)?;
    let o = FindOptions::default();
    assert!(s.find("rust", o)?);
    assert_eq!((s.cursor, s.anchor), (at(0, 4), Some(0)));
    assert!(s.find("rust", o)?);
    assert_eq!(s.anchor, Some(5));
    let word = FindOptions {
        whole_word: true,
        ..o
    };
    assert!(s.find("rust", word)?);
    assert_eq!((s.cursor.paragraph, s.anchor), (0, Some(11)));
    assert!(s.find("rust", word)?);
    assert_eq!((s.cursor.paragraph, s.anchor), (1, Some(3)));
    // Down wraps to the document start.
    assert!(s.find("rust", word)?);
    assert_eq!((s.cursor.paragraph, s.anchor), (0, Some(0)));
    let exact = FindOptions {
        case_sensitive: true,
        ..o
    };
    assert!(s.find("RUST", exact)?);
    assert_eq!(s.anchor, Some(11));
    let up = FindOptions {
        direction: FindDirection::Up,
        ..o
    };
    assert!(s.find("rust", up)?);
    assert_eq!(s.anchor, Some(5));
    assert!(!s.find("없음", o)?);
    Ok(())
}

#[test]
fn replace_next_and_replace_all_with_options() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("Cat cat category CAT".into()))?;
    s.select(at(0, 0), false)?;
    let o = FindOptions {
        whole_word: true,
        ..Default::default()
    };
    assert!(s.find("cat", o)?);
    assert!(s.replace_next("cat", "개", o)?);
    assert_eq!(s.text(), "개 cat category CAT");
    assert_eq!(s.selected_text(), "cat");
    s.execute(Command::Undo)?;
    assert_eq!(s.text(), "Cat cat category CAT");

    let count = s.replace_all_with("cat", "dog", o)?;
    assert_eq!(count, 3);
    assert_eq!(s.text(), "dog dog category dog");
    s.execute(Command::Undo)?;
    assert_eq!(s.text(), "Cat cat category CAT");

    let v = s.run(
        "edit.replace_all",
        json!({"query":"cat","replacement":"x","options":{"case_sensitive":true}}),
    )?;
    assert_eq!(v["replaced"], 2);
    assert_eq!(s.text(), "Cat x xegory CAT");
    let v = s.run(
        "edit.find",
        json!({"query":"CAT","options":{"direction":"all"}}),
    )?;
    assert_eq!(v["found"], true);
    assert_eq!(v["anchor"], 0);
    assert!(
        s.run("edit.find", json!({"query":"a","options":{"bogus":1}}))
            .is_err()
    );
    Ok(())
}
