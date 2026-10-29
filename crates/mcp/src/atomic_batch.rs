//! Whole-history batch baselines. A failed restore invalidates this backend;
//! never claim rolledBack=true or reuse a partially restored document.
use crate::Headless;
use anyhow::{Result, bail};
use hwpcraft_engine::FullCheckpoint;

pub(crate) fn rollback(backend: &mut Headless, checkpoint: FullCheckpoint) -> Result<()> {
    if backend.session.rollback_full(checkpoint).is_err() {
        backend.usable = false;
        bail!(
            "문서 상태 복원에 실패하여 해당 문서를 닫았습니다. 다른 문서는 계속 사용할 수 있습니다."
        );
    }
    Ok(())
}

pub(crate) fn release(backend: &mut Headless, checkpoint: FullCheckpoint) -> Result<()> {
    if backend.session.release_full(checkpoint).is_err() {
        backend.usable = false;
        bail!("문서 작업 범위를 종료하지 못하여 해당 문서를 닫았습니다.");
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::tools;
    use serde_json::{Value, json};

    fn data(value: Value) -> Value {
        assert_eq!(value["isError"], false, "{value}");
        serde_json::from_str(value["content"][0]["text"].as_str().unwrap()).unwrap()
    }

    #[test]
    fn failed_batch_restores_redo_and_consumed_undo_entries() -> Result<()> {
        let mut b = Headless::new(None)?;
        for text in ["A", " B", " C", " D"] {
            b.execute("text.insert", json!({"text":text}))?;
        }
        b.execute("edit.undo", json!({}))?;
        b.execute("edit.undo", json!({}))?;
        let response = tools::call(
            &mut b,
            "batch",
            json!({"atomic":true,"commands":[
                {"command":"edit.redo"}, {"command":"edit.undo"},
                {"command":"edit.undo"}, {"command":"text.insert","params":{"text":"X"}},
                {"command":"not.a.command"}
            ]}),
        );
        assert_eq!(response["isError"], true);
        let body: Value = serde_json::from_str(response["content"][0]["text"].as_str().unwrap())?;
        assert_eq!(body["rolledBack"], true);
        assert_eq!(b.session.text(), "A B");
        for expected in ["A B C", "A B C D"] {
            b.execute("edit.redo", json!({}))?;
            assert_eq!(b.session.text(), expected);
        }
        for expected in ["A B C", "A B", "A", ""] {
            b.execute("edit.undo", json!({}))?;
            assert_eq!(b.session.text(), expected);
        }
        assert!(b.usable);
        Ok(())
    }

    #[test]
    fn success_releases_scope_and_new_work_keeps_normal_history() -> Result<()> {
        let mut b = Headless::new(None)?;
        b.execute("text.insert", json!({"text":"A"}))?;
        let result = data(tools::call(
            &mut b,
            "batch",
            json!({"atomic":true,"commands":[
                {"command":"text.insert","params":{"text":" B"}},
                {"command":"text.insert","params":{"text":" C"}}
            ]}),
        ));
        assert_eq!(result["rolledBack"], false);
        assert_eq!(b.session.text(), "A B C");
        b.execute("edit.undo", json!({}))?;
        assert_eq!(b.session.text(), "A B");
        b.execute("edit.redo", json!({}))?;
        assert_eq!(b.session.text(), "A B C");
        let token = b.session.full_checkpoint()?;
        b.session.release_full(token)?;
        Ok(())
    }

    #[test]
    fn password_changes_in_failed_batches_preserve_actual_save_credentials() -> Result<()> {
        let dir = tempfile::tempdir()?;
        for ext in ["hwp", "hwpx"] {
            for (case, original, changed) in [
                ("set", None, Some("new-test-key")),
                ("change", Some("old-test-key"), Some("new-test-key")),
                ("clear", Some("old-test-key"), None),
            ] {
                let mut b = Headless::new(None)?;
                b.execute("text.insert", json!({"text":"암호 배치 자체 문서😀"}))?;
                b.execute("file.password", json!({"password":original}))?;
                let before = dir.path().join(format!("{case}-before.{ext}"));
                b.execute("file.save", json!({"path":before}))?;
                assert!(!b.session.dirty());
                let response = tools::call(
                    &mut b,
                    "batch",
                    json!({"atomic":true,"commands":[
                        {"command":"file.password","params":{"password":changed}},
                        {"command":"not.a.command"}
                    ]}),
                );
                assert_eq!(response["isError"], true);
                assert!(!b.session.dirty());
                assert_eq!(b.session.has_password(), original.is_some());
                let after = dir.path().join(format!("{case}-after.{ext}"));
                b.execute("file.save", json!({"path":after}))?;
                let mut reopened = hwpcraft_engine::Session::new()?;
                match original {
                    Some(key) => {
                        assert!(reopened.open(&after).is_err());
                        assert!(
                            reopened
                                .open_with_password(&after, "wrong-test-key")
                                .is_err()
                        );
                        reopened.open_with_password(&after, key)?;
                    }
                    None => reopened.open(&after)?,
                }
                assert_eq!(reopened.text(), "암호 배치 자체 문서😀");
            }
        }
        Ok(())
    }

    #[test]
    fn restore_failure_is_error_and_invalid_backend_refuses_further_tools() -> Result<()> {
        let mut b = Headless::new(None)?;
        b.execute("text.insert", json!({"text":"isolated fixture"}))?;
        let checkpoint = b.session.full_checkpoint()?;
        // Deliberately invalidate the native baseline. This is fault-routing
        // evidence, not a claim that arbitrary native panics are recoverable.
        b.session
            .core
            .create_blank_document_native()
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        assert!(rollback(&mut b, checkpoint).is_err());
        assert!(!b.usable);
        for name in ["get_text", "execute", "new_document", "list_commands"] {
            let result = tools::call(&mut b, name, json!({}));
            assert_eq!(result["isError"], true);
        }
        let mut other = Headless::new(None)?;
        data(tools::call(
            &mut other,
            "type_text",
            json!({"text":"other document"}),
        ));
        assert_eq!(other.session.text(), "other document");
        Ok(())
    }
}
