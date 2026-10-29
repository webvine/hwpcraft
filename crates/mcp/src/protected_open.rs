//! Headless password entry: shares the desktop's staged load and error handling.
use crate::Headless;
use anyhow::Result;
use serde::Deserialize;
use serde_json::Value;
use std::path::PathBuf;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OpenProtected {
    path: PathBuf,
    password: String,
    #[serde(default)]
    force: bool,
}

pub(crate) fn open(backend: &mut Headless, params: Value) -> Result<Value> {
    let p: OpenProtected = serde_json::from_value(params)?;
    backend.require_discard(p.force)?;
    let prepared = crate::staged_document::prepare_open(
        p.path,
        Some(p.password),
        crate::staged_document::next_generation(backend.session.generation),
    )?;
    Ok(prepared.commit(&mut backend.session))
}

#[cfg(test)]
mod tests {
    use super::*;
    use hwpcraft_engine::{Command, Session};
    use hwpcraft_formats::file_access::AutomationRoots;
    use serde_json::json;

    #[test]
    fn protected_open_roundtrips_both_formats_and_preserves_failed_loads() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let password = "own-known-password";
        for extension in ["hwp", "hwpx"] {
            let path = dir.path().join(format!("protected.{extension}"));
            let mut source = Session::new()?;
            source.execute(Command::Insert("암호 보고서 😀".into()))?;
            source.set_password(Some(password.into()))?;
            source.save(&path)?;
            let bytes = std::fs::read(&path)?;
            let mut backend = Headless::new(None)?;
            backend.execute("text.insert", json!({"text":"현재 문서 보존"}))?;
            let before = backend.session.info();
            let generation = backend.session.generation;
            assert!(
                backend
                    .execute(
                        "file.open_protected",
                        json!({"path":path,"password":password})
                    )
                    .is_err()
            );
            let error = backend
                .execute(
                    "file.open_protected",
                    json!({"path":path,"password":"different-password","force":true}),
                )
                .err()
                .ok_or_else(|| anyhow::anyhow!("wrong password accepted"))?;
            assert!(hwpcraft_formats::wrong_password(&error));
            assert!(!format!("{error:#}").contains(password));
            assert_eq!(backend.session.info(), before);
            assert_eq!(backend.session.generation, generation);
            backend.execute(
                "file.open_protected",
                json!({"path":path,"password":password,"force":true}),
            )?;
            assert_eq!(backend.session.text(), "암호 보고서 😀");
            assert!(backend.session.has_password());
            assert!(!backend.session.dirty());
            assert_eq!(std::fs::read(&path)?, bytes);
        }
        Ok(())
    }

    #[test]
    fn protected_open_obeys_read_roots_and_atomic_batch_preflight() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let allowed = dir.path().join("allowed");
        std::fs::create_dir(&allowed)?;
        let outside = dir.path().join("outside.hwp");
        let mut source = Session::new()?;
        source.execute(Command::Insert("별도 원본".into()))?;
        source.set_password(Some("own-password".into()))?;
        source.save(&outside)?;
        let roots =
            AutomationRoots::new(Some(&allowed), Some(&allowed)).map_err(anyhow::Error::msg)?;
        let mut backend = Headless::new(roots)?;
        backend.execute("text.insert", json!({"text":"기존"}))?;
        let before = backend.session.info();
        assert!(
            backend
                .execute(
                    "file.open_protected",
                    json!({"path":outside,"password":"own-password","force":true})
                )
                .is_err()
        );
        assert_eq!(backend.session.info(), before);
        let result = crate::tools::call(
            &mut backend,
            "batch",
            json!({"atomic":true,"commands":[
                {"command":"text.insert","params":{"text":"실행되면 안 됨"}},
                {"command":"file.open_protected","params":{"path":outside,"password":"own-password","force":true}}
            ]}),
        );
        assert_eq!(result["isError"], true);
        assert_eq!(backend.session.info(), before);
        Ok(())
    }
}
