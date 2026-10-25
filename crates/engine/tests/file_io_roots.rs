//! Direct engine file commands obey the same confinement used by MCP.
use hwpcraft_engine::{Command, Session};
use hwpcraft_formats::file_access::{self, AutomationRoots};
use serde_json::json;

#[test]
fn style_files_respect_separate_read_write_roots_and_preserve_denied_targets() -> anyhow::Result<()>
{
    let temp = tempfile::tempdir()?;
    let allowed = temp.path().join("allowed");
    std::fs::create_dir(&allowed)?;
    let outside = temp.path().join("outside.json");
    let inside = allowed.join("inside.json");
    let mut s = Session::new()?;
    s.execute(Command::Insert("보존할 문서".into()))?;
    s.run("format.style.export", json!({"path":outside}))?;
    let original = std::fs::read(&outside)?;
    let state = json!({"document":format!("{:?}",s.core.document()),"info":s.info(),"cursor":s.cursor,"generation":s.generation});

    let roots = AutomationRoots::new(Some(&allowed), Some(&allowed)).map_err(anyhow::Error::msg)?;
    file_access::confine(roots.as_ref(), || -> anyhow::Result<()> {
        assert!(
            s.run("format.style.export", json!({"path":outside}))
                .is_err()
        );
        assert!(
            s.run("format.style.import", json!({"path":outside}))
                .is_err()
        );
        s.run("format.style.export", json!({"path":inside}))?;
        // Query-only export performs no file access.
        assert!(
            s.run("format.style.export", json!({}))?
                .get("styles")
                .is_some()
        );
        Ok(())
    })?;
    assert_eq!(std::fs::read(&outside)?, original);
    assert_eq!(
        json!({"document":format!("{:?}",s.core.document()),"info":s.info(),"cursor":s.cursor,"generation":s.generation}),
        state
    );
    assert_eq!(std::fs::read(&inside)?, original);

    let roots = AutomationRoots::new(Some(&allowed), None).map_err(anyhow::Error::msg)?;
    file_access::confine(roots.as_ref(), || {
        assert!(
            s.run("format.style.export", json!({"path":inside}))
                .is_err()
        );
    });
    let roots = AutomationRoots::new(None, Some(&allowed)).map_err(anyhow::Error::msg)?;
    file_access::confine(roots.as_ref(), || {
        assert!(
            s.run("format.style.import", json!({"path":inside}))
                .is_err()
        );
    });
    assert_eq!(std::fs::read(&inside)?, original);
    Ok(())
}

#[test]
fn picture_exports_check_the_final_extension_and_all_destination_paths() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let allowed = temp.path().join("allowed");
    std::fs::create_dir(&allowed)?;
    let input = temp.path().join("own-image.png");
    image::RgbaImage::from_pixel(3, 2, image::Rgba([20, 80, 160, 255])).save(&input)?;
    let bytes = std::fs::read(&input)?;
    let mut s = Session::new()?;
    let address = s.insert_picture(&input, Some(10.0), false)?.unwrap();
    let outside = temp.path().join("outside.png");
    std::fs::write(&outside, b"existing target")?;
    let roots = AutomationRoots::new(None, Some(&allowed)).map_err(anyhow::Error::msg)?;
    file_access::confine(roots.as_ref(), || -> anyhow::Result<()> {
        assert!(
            s.save_picture(address, &outside.with_extension("jpg"), true)
                .is_err()
        );
        assert!(s.save_all_pictures(temp.path(), "denied", true).is_err());
        let actual = s.save_picture(address, &allowed.join("own-image.jpg"), false)?;
        assert_eq!(actual, allowed.join("own-image.png"));
        assert_eq!(
            s.save_all_pictures(&allowed, "all", false)?,
            vec![allowed.join("all1.png")]
        );
        Ok(())
    })?;
    assert_eq!(std::fs::read(&outside)?, b"existing target");
    assert!(!temp.path().join("denied1.png").exists());
    assert_eq!(std::fs::read(allowed.join("own-image.png"))?, bytes);
    assert_eq!(std::fs::read(allowed.join("all1.png"))?, bytes);
    Ok(())
}

#[test]
fn save_all_pictures_rejects_a_directory_target_before_replacing_any_picture() -> anyhow::Result<()>
{
    let temp = tempfile::tempdir()?;
    let input = temp.path().join("own-source.png");
    image::RgbaImage::from_pixel(2, 2, image::Rgba([50, 100, 150, 255])).save(&input)?;
    let mut s = Session::new()?;
    s.insert_picture(&input, Some(10.0), false)?;
    s.insert_picture(&input, Some(10.0), false)?;
    let first = temp.path().join("all1.png");
    let second = temp.path().join("all2.png");
    std::fs::write(&first, b"existing first picture")?;
    std::fs::create_dir(&second)?;
    let source = format!("{:?}", s.core.document());
    assert!(s.save_all_pictures(temp.path(), "all", true).is_err());
    assert_eq!(std::fs::read(&first)?, b"existing first picture");
    assert!(second.is_dir());
    assert_eq!(format!("{:?}", s.core.document()), source);
    Ok(())
}
