//! Candidate document loading without borrowing the current live Session.
//! The MCP boundary uses the existing engine APIs with owned inputs.
//!
//! Only preparation owns the new Session; it never receives the live Session.
//! The commit and old-session destruction are OUTSIDE catch_unwind. Generic
//! editing/history/commit/drop failures keep the worker's existing fail-closed
//! policy. Tests with deliberately injected panic exercise routing only, not
//! every native parser/layout implementation's unwind safety or process health.
use anyhow::{Result, anyhow};
use hwpcraft_engine::Session;
use serde_json::Value;
use std::{
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    path::PathBuf,
};

pub(crate) struct PreparedDocument {
    session: Session,
    info: Value,
}

impl PreparedDocument {
    /// Caller must run the existing dirty/force guard before preparation.
    /// This consumes a completely prepared candidate. An old-session Drop or
    /// later tool-envelope panic must propagate to the worker's outer catcher.
    pub(crate) fn commit(self, target: &mut Session) -> Value {
        let Self { session, info } = self;
        let old = std::mem::replace(target, session);
        drop(old);
        info
    }
}

/// Match the existing release rollover policy explicitly at the MCP boundary,
/// so preparing a replacement is deterministic in debug and release builds.
pub(crate) fn next_generation(current: u64) -> u64 {
    current.wrapping_add(1)
}

/// Owned inputs prevent borrowing or replacing the live Session during unwind.
/// Call within the EXISTING file_access::confine scope; never unconfine loading.
pub(crate) fn prepare_open(
    path: PathBuf,
    password: Option<String>,
    next_generation: u64,
) -> Result<PreparedDocument> {
    prepare_owned(move || {
        // The existing public API has no empty Session factory accepting a core.
        // One extra blank core is the intentional cost of avoiding new engine API.
        let mut candidate = Session::new()?;
        match password.as_deref() {
            Some(password) => candidate.open_with_password(&path, password)?,
            None => candidate.open(&path)?,
        }
        finish_candidate(candidate, next_generation)
    })
}

pub(crate) fn prepare_new(next_generation: u64) -> Result<PreparedDocument> {
    prepare_owned(move || finish_candidate(Session::new()?, next_generation))
}

fn finish_candidate(mut session: Session, next_generation: u64) -> Result<PreparedDocument> {
    session.generation = next_generation;
    // Open already validates page_count, but changing the generation can require
    // candidate-only cache rebuilding. Preflight again BEFORE the live commit.
    // For new this is a deliberate strict preflight rather than returning an
    // info object whose pages=null/layoutError is silently treated as success.
    session.page_count()?;
    let info = session.info();
    Ok(PreparedDocument { session, info })
}

fn prepare_owned(make: impl FnOnce() -> Result<PreparedDocument>) -> Result<PreparedDocument> {
    match catch_unwind(AssertUnwindSafe(make)) {
        Ok(result) => result,
        Err(payload) => {
            // Native Rust panic!/expect commonly use these standard, inert Drop
            // payloads. Do not format, log, serialize or downcast their contents.
            // Arbitrary panic_any payloads may have a panicking destructor; keep
            // those in the outer fail-closed policy instead of claiming recovery.
            if !payload.is::<String>() && !payload.is::<&'static str>() {
                resume_unwind(payload);
            }
            drop(payload);
            Err(anyhow!(
                "문서 준비 중 엔진 오류가 발생하여 문서를 교체하지 않았습니다."
            ))
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use hwpcraft_engine::Command;
    use serde_json::json;
    use std::path::Path;

    const OLD_PASSWORD: &str = "self-created-old-secret";

    fn fingerprint(session: &Session) -> Value {
        json!({"document":format!("{:?}",session.core.document()),
            "info":session.info(),"generation":session.generation,
            "cursor":session.cursor,"anchor":session.anchor,"span":session.span_anchor,
            "overwrite":session.overwrite,"undo":session.can_undo(),"redo":session.can_redo(),
            "hasPassword":session.has_password()})
    }

    fn old_session(path: &Path) -> Result<Session> {
        let mut session = Session::new()?;
        session.execute(Command::Insert("첫 문단\n둘째 문단".into()))?;
        session.set_password(Some(OLD_PASSWORD.into()))?;
        session.save(path)?;
        session.execute(Command::Insert(" 미저장".into()))?;
        session.execute(Command::Split)?;
        session.run("edit.undo", json!({}))?;
        let generation = session.generation;
        session.run(
            "selection.range",
            json!({
            "start":{"section":0,"paragraph":0,"offset":1},
            "end":{"section":0,"paragraph":1,"offset":1},
            "expected_generation":generation}),
        )?;
        session.overwrite = true;
        assert!(session.dirty() && session.can_undo() && session.can_redo());
        Ok(session)
    }

    fn own_source(path: &Path, password: Option<&str>) -> Result<()> {
        let mut source = Session::new()?;
        source.execute(Command::Insert("교체할 자체 원문".into()))?;
        source.set_password(password.map(str::to_owned))?;
        source.save(path)
    }

    #[test]
    fn actual_wrong_password_permission_and_invalid_load_do_not_touch_live_session() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let old_path = dir.path().join("old.hwpx");
        let mut live = old_session(&old_path)?;
        let before = fingerprint(&live);
        let old_bytes = std::fs::read(&old_path)?;
        let source = dir.path().join("source.hwp");
        own_source(&source, Some("self-created-new-secret"))?;
        let wrong = prepare_open(
            source.clone(),
            Some("wrong-self-created-secret".into()),
            next_generation(live.generation),
        )
        .err()
        .expect("wrong password accepted");
        assert!(hwpcraft_formats::wrong_password(&wrong));
        assert_eq!(fingerprint(&live), before);
        let allowed = dir.path().join("allowed");
        std::fs::create_dir(&allowed)?;
        let roots =
            hwpcraft_formats::file_access::AutomationRoots::new(Some(&allowed), Some(&allowed))
                .map_err(anyhow::Error::msg)?;
        let rejected = hwpcraft_formats::file_access::confine(roots.as_ref(), || {
            prepare_open(
                source.clone(),
                Some("self-created-new-secret".into()),
                next_generation(live.generation),
            )
        });
        assert!(rejected.is_err());
        assert_eq!(fingerprint(&live), before);
        let invalid = dir.path().join("unsupported.pdf");
        std::fs::write(&invalid, b"%PDF-1.4\nself-created invalid HWP input")?;
        assert!(prepare_open(invalid, None, next_generation(live.generation)).is_err());
        assert_eq!(fingerprint(&live), before);
        assert_eq!(std::fs::read(&old_path)?, old_bytes);
        // Behavioral proof of the OLD private password, not just has_password.
        let preserved = dir.path().join("preserved.hwpx");
        live.save(&preserved)?;
        let mut reopened = Session::new()?;
        reopened.open_with_password(&preserved, OLD_PASSWORD)?;
        assert_eq!(
            reopened.core.document().sections[0].paragraphs[0].text,
            "첫 문단"
        );
        Ok(())
    }

    #[test]
    fn prepared_load_is_not_a_commit_and_success_uses_requested_generation() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let old_path = dir.path().join("old.hwpx");
        let mut live = old_session(&old_path)?;
        let source = dir.path().join("source.hwpx");
        own_source(&source, None)?;
        let before = fingerprint(&live);
        let next = next_generation(live.generation);
        let prepared = prepare_open(source.clone(), None, next)?;
        assert_eq!(fingerprint(&live), before);
        let info = prepared.commit(&mut live);
        assert_eq!(live.generation, next);
        assert_eq!(live.path.as_ref(), Some(&source));
        assert!(!live.has_password() && !live.can_undo() && !live.can_redo());
        assert_eq!(info, live.info());
        let next = next_generation(live.generation);
        let before = fingerprint(&live);
        let prepared = prepare_new(next)?;
        assert_eq!(fingerprint(&live), before);
        prepared.commit(&mut live);
        assert_eq!(live.generation, next);
        assert!(live.path.is_none() && live.is_empty() && !live.dirty());
        Ok(())
    }

    #[test]
    fn injected_prepare_panic_proves_routing_only_not_vendor_unwind_safety() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let live = old_session(&dir.path().join("old.hwpx"))?;
        let before = fingerprint(&live);
        let error = prepare_owned(|| {
            let _candidate = Session::new()?;
            panic!("private-deliberate-payload-not-native-proof")
        })
        .err()
        .expect("preparation panic was not mapped");
        assert!(!error.to_string().contains("private-deliberate"));
        assert_eq!(fingerprint(&live), before);
        // No native parser/layout panic fixture has been reproduced by this test.
        Ok(())
    }

    #[test]
    fn unknown_panic_payload_is_not_masked_as_safe_preparation_failure() {
        let outer = catch_unwind(AssertUnwindSafe(|| {
            let _ = prepare_owned(|| std::panic::panic_any(123_u64));
        }));
        assert!(outer.is_err());
        assert_eq!(next_generation(u64::MAX), 0);
    }

    #[test]
    fn full_existing_undo_and_redo_sequences_survive_a_failed_preparation() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("same-own-original.hwpx");
        // Saving stamps the document's times; both copies must be written at the same instant.
        hwpcraft_engine::fixed_clock(Some(1_767_225_600));
        let mut baseline = old_session(&path)?;
        let mut actual = old_session(&path)?;
        assert_eq!(fingerprint(&actual), fingerprint(&baseline));
        assert!(
            prepare_open(
                dir.path().join("missing-own-file.hwp"),
                None,
                next_generation(actual.generation)
            )
            .is_err()
        );
        for command in [
            "edit.undo",
            "edit.redo",
            "edit.redo",
            "edit.undo",
            "edit.undo",
            "edit.redo",
        ] {
            baseline.run(command, json!({}))?;
            actual.run(command, json!({}))?;
            assert_eq!(fingerprint(&actual), fingerprint(&baseline), "{command}");
        }
        Ok(())
    }

    #[test]
    fn actual_headless_routes_preserve_live_history_and_password_until_commit() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let old_path = dir.path().join("own-current.hwpx");
        let source = dir.path().join("own-protected.hwp");
        own_source(&source, Some("own-replacement-password"))?;
        let mut backend = crate::Headless::new(None)?;
        backend.session = old_session(&old_path)?;
        let before = fingerprint(&backend.session);
        let rejected = crate::tools::call(&mut backend, "open_document", json!({"path":source}));
        assert_eq!(rejected["isError"], true);
        assert_eq!(fingerprint(&backend.session), before);
        let rejected = crate::tools::call(&mut backend, "new_document", json!({}));
        assert_eq!(rejected["isError"], true);
        assert_eq!(fingerprint(&backend.session), before);
        for params in [
            json!({"command":"file.open","params":{"path":dir.path().join("missing.hwp"),"force":true}}),
            json!({"command":"file.open_protected","params":{"path":source,"password":"own-wrong-password","force":true}}),
        ] {
            let result = crate::tools::call(&mut backend, "execute", params);
            assert_eq!(result["isError"], true);
            assert_eq!(fingerprint(&backend.session), before);
        }
        let result = crate::tools::call(
            &mut backend,
            "open_protected_document",
            json!({"path":source,"password":"own-replacement-password","force":true}),
        );
        assert_eq!(result["isError"], false);
        assert_eq!(backend.session.path.as_ref(), Some(&source));
        assert!(!backend.session.can_undo() && !backend.session.can_redo());
        let saved = dir.path().join("own-replacement-saved.hwpx");
        backend.session.save(&saved)?;
        let mut reopened = Session::new()?;
        reopened.open_with_password(&saved, "own-replacement-password")?;
        assert_eq!(reopened.text(), "교체할 자체 원문");
        assert!(reopened.open_with_password(&saved, OLD_PASSWORD).is_err());
        let result = crate::tools::call(&mut backend, "new_document", json!({"force":true}));
        assert_eq!(result["isError"], false);
        assert!(backend.session.path.is_none() && !backend.session.has_password());
        Ok(())
    }

    #[test]
    fn ordinary_batch_keeps_prior_edit_when_later_preparation_fails() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let mut backend = crate::Headless::new(None)?;
        backend.execute("text.insert", json!({"text":"기존"}))?;
        let original_generation = backend.session.generation;
        let result = crate::tools::call(
            &mut backend,
            "batch",
            json!({"commands":[
                {"command":"text.insert","params":{"text":" 앞선 편집"}},
                {"command":"file.open","params":{"path":dir.path().join("missing.hwp"),"force":true}}
            ]}),
        );
        assert_eq!(result["isError"], true);
        assert_eq!(backend.session.text(), "기존 앞선 편집");
        assert!(backend.session.generation > original_generation);
        assert!(backend.session.dirty() && backend.session.can_undo());
        let before = fingerprint(&backend.session);
        let rejected = crate::tools::call(
            &mut backend,
            "batch",
            json!({"atomic":true,"commands":[
                {"command":"text.insert","params":{"text":" 실행 금지"}},
                {"command":"file.new","params":{"force":true}}
            ]}),
        );
        assert_eq!(rejected["isError"], true);
        assert_eq!(fingerprint(&backend.session), before);
        Ok(())
    }

    // Root's integration regressions, after actual Headless wiring:
    // - dirty + force:false with valid/invalid own target refuses BEFORE prepare;
    // - tool aliases and execute/file.* share the same preparation path;
    // - protected_open uses the same path and keeps the OLD private password on
    //   error, while successful replacement takes the NEW password;
    // - non-atomic batch preserves the state at the failing load command, with
    //   prior edits retained; atomic preflight still refuses these commands;
    // - failed preparation leaves the handle/queue/subscriptions alive and
    //   produces no invalidation when its DocumentStamp is unchanged;
    // - a minimized REAL native parser/layout panic fixture is a separate oracle.
}
