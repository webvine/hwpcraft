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

#[test]
fn extended_options_find_and_replace() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("대 한 민국 韓國 제12조\n제1조 비고".into()))?;
    s.select(at(0, 0), false)?;
    // 띄어쓰기 무시 selects the spaced run.
    let spaces = FindOptions {
        ignore_spaces: true,
        ..Default::default()
    };
    assert!(s.find("대한민국", spaces)?);
    assert_eq!((s.anchor, s.cursor.offset), (Some(0), 6));
    // 한글로 한자 찾기 continues to 韓國.
    let hanja = FindOptions {
        hanja_by_hangul: true,
        ..Default::default()
    };
    assert!(s.find("한국", hanja)?);
    assert_eq!((s.anchor, s.cursor.offset), (Some(7), 9));
    // 아무개 문자 `?` is one character, so 제12조 is skipped for 제1조.
    let wild = FindOptions {
        wildcards: true,
        ..Default::default()
    };
    assert!(s.find("제?조", wild)?);
    assert_eq!((s.cursor.paragraph, s.anchor), (1, Some(0)));
    // 조건식 with an invalid pattern is an error, not a crash.
    let regex = FindOptions {
        regex: true,
        ..Default::default()
    };
    assert!(s.find("[", regex).is_err());
    assert!(s.find(r"제\d+조", regex)?);
    // Through the command protocol, unknown options are still rejected.
    let v = s.run(
        "edit.find",
        json!({"query":"비 고","options":{"ignore_spaces":true,"direction":"all"}}),
    )?;
    assert_eq!(v["found"], true);
    assert!(
        s.run("edit.find", json!({"query":"a","options":{"regexp":true}}))
            .is_err()
    );
    // 모두 바꾸기 with a wildcard replaces variable-length matches in one undo step.
    let count = s.replace_all_with("제*조", "조항", wild)?;
    assert_eq!(count, 2);
    assert_eq!(s.entries()[0].1, "대 한 민국 韓國 조항");
    assert_eq!(s.entries()[1].1, "조항 비고");
    s.execute(Command::Undo)?;
    assert_eq!(s.entries()[1].1, "제1조 비고");
    Ok(())
}

#[test]
fn format_conditions_restrict_matches() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("bold plain bold".into()))?;
    s.select(at(0, 0), false)?;
    s.select(at(0, 4), true)?;
    s.execute(Command::Character(json!({"bold": true})))?;
    s.select(at(0, 0), false)?;
    let bold = FindOptions {
        bold: Some(true),
        ..Default::default()
    };
    assert!(s.find("bold", bold)?);
    assert_eq!(s.anchor, Some(0));
    // Only the first "bold" is bold, so the search wraps back to it.
    assert!(s.find("bold", bold)?);
    assert_eq!(s.anchor, Some(0));
    let plain = FindOptions {
        bold: Some(false),
        ..Default::default()
    };
    assert!(s.find("bold", plain)?);
    assert_eq!(s.anchor, Some(11));
    assert_eq!(s.replace_all_with("bold", "B", bold)?, 1);
    assert_eq!(s.text(), "B plain bold");
    Ok(())
}
