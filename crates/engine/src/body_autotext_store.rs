//! Independent, versioned storage for formatted body snippets.
//!
//! `autotext.json` remains the plain-text snippet store. This module never parses
//! documents or renders payloads; the fragment boundary validates HWPX content.
use anyhow::{Context, Result, anyhow, bail, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    fs::{File, OpenOptions, TryLockError},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

const FILE_NAME: &str = "autotext-body-v1.json";
const LOCK_NAME: &str = "autotext-body-v1.lock";
const MAX_ENTRIES: usize = 1000;
const MAX_PAYLOAD: usize = 8 * 1024 * 1024;
const MAX_STORE: usize = 32 * 1024 * 1024;
const MAX_ENCODED: usize = MAX_PAYLOAD.div_ceil(3) * 4;
const MAX_STATS_NODES: usize = 100_000;
static NEXT_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Entry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub summary: String,
    pub paragraphs: usize,
    pub stats: Value,
    pub payload: String,
    pub payload_format: String,
    pub created_at: u64,
    pub modified_at: u64,
}

pub(crate) struct NewEntry {
    pub name: String,
    pub description: String,
    pub summary: String,
    pub paragraphs: usize,
    pub stats: Value,
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug)]
pub(crate) struct Snapshot {
    pub path: PathBuf,
    pub revision: u64,
    pub entries: Vec<Entry>,
}

impl Entry {
    pub fn payload_bytes(&self) -> Result<Vec<u8>> {
        ensure!(
            self.payload_format == "hwpx",
            "본문 상용구 payloadFormat은 hwpx여야 합니다."
        );
        ensure!(
            !self.payload.is_empty() && self.payload.len() <= MAX_ENCODED,
            "본문 상용구 payload는 1바이트–8 MiB여야 합니다."
        );
        let bytes = STANDARD
            .decode(self.payload.as_bytes())
            .context("본문 상용구 payload의 base64 형식이 올바르지 않습니다.")?;
        ensure!(
            !bytes.is_empty() && bytes.len() <= MAX_PAYLOAD,
            "본문 상용구 payload는 1바이트–8 MiB여야 합니다."
        );
        // STANDARD requires canonical padding and rejects unused non-zero bits.
        // Keep that wire contract explicit if its decoder settings ever change.
        ensure!(
            STANDARD.encode(&bytes) == self.payload,
            "본문 상용구 payload가 표준 base64 형식이 아닙니다."
        );
        Ok(bytes)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Store {
    schema_version: u32,
    revision: u64,
    entries: Vec<Entry>,
}

fn path() -> Result<PathBuf> {
    crate::autotext::config_dir()
        .map(|dir| dir.join(FILE_NAME))
        .ok_or_else(|| anyhow!("설정 폴더를 찾지 못해 본문 상용구를 저장할 수 없습니다."))
}
fn check_read(path: &Path) -> Result<()> {
    hwpcraft_formats::file_access::check_read(&path.to_string_lossy()).map_err(anyhow::Error::msg)
}
fn check_write(path: &Path) -> Result<()> {
    hwpcraft_formats::file_access::check_write(&path.to_string_lossy()).map_err(anyhow::Error::msg)
}
fn mutation_gates(path: &Path) -> Result<PathBuf> {
    let lock = path.with_file_name(LOCK_NAME);
    // An unauthorized mutator must not even read protected snippet bytes or
    // create the persistent lock file. Check both authorities for both paths.
    check_read(path)?;
    check_write(path)?;
    check_read(&lock)?;
    check_write(&lock)?;
    Ok(lock)
}

struct StoreLock {
    file: File,
}
impl StoreLock {
    fn acquire(path: &Path) -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .with_context(|| {
                format!(
                    "본문 상용구 잠금 파일을 열지 못했습니다: {}",
                    path.display()
                )
            })?;
        match file.try_lock() {
            Ok(()) => Ok(Self { file }),
            Err(TryLockError::WouldBlock) => bail!(
                "다른 창이나 프로세스가 본문 상용구를 저장하고 있습니다. 잠시 후 다시 시도하세요."
            ),
            Err(TryLockError::Error(error)) => {
                Err(error).context("본문 상용구 저장소를 잠그지 못했습니다.")
            }
        }
    }
}
impl Drop for StoreLock {
    fn drop(&mut self) {
        // The handle is closed immediately afterwards even if unlock fails.
        // Never unlink this persistent file: replacing a locked inode would
        // allow two processes to hold unrelated locks for the same store.
        let _ = self.file.unlock();
    }
}

fn valid_text(text: &str, max: usize, label: &str, multiline: bool) -> Result<()> {
    ensure!(
        text.chars().count() <= max,
        "{label}은 {max}자 이하여야 합니다."
    );
    ensure!(
        !text
            .chars()
            .any(|c| c.is_control() && !(multiline && matches!(c, '\n' | '\r' | '\t'))),
        "{label}에 지원하지 않는 제어 문자가 있습니다."
    );
    Ok(())
}
fn valid_name(name: &str) -> Result<()> {
    ensure!(
        !name.is_empty() && name == name.trim(),
        "본문 상용구 이름은 앞뒤 공백 없이 1–64자여야 합니다."
    );
    valid_text(name, 64, "본문 상용구 이름", false)
}
fn valid_id(id: &str) -> Result<()> {
    ensure!(
        !id.is_empty()
            && id.len() <= 128
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_')),
        "본문 상용구 식별자 형식이 올바르지 않습니다."
    );
    Ok(())
}
fn valid_stats(stats: &Value) -> Result<()> {
    let mut pending = vec![(stats, 0usize)];
    let mut count = 0usize;
    while let Some((node, depth)) = pending.pop() {
        count += 1;
        ensure!(
            depth <= 64 && count <= MAX_STATS_NODES,
            "본문 상용구 통계의 중첩/항목 한도를 초과했습니다."
        );
        match node {
            Value::Array(items) => pending.extend(items.iter().map(|item| (item, depth + 1))),
            Value::Object(items) => pending.extend(items.values().map(|item| (item, depth + 1))),
            _ => {}
        }
    }
    Ok(())
}
fn validate_entry(entry: &Entry) -> Result<()> {
    valid_id(&entry.id)?;
    valid_name(&entry.name)?;
    valid_text(&entry.description, 1024, "본문 상용구 설명", true)?;
    valid_text(&entry.summary, 1000, "본문 상용구 요약", true)?;
    ensure!(
        entry.paragraphs > 0,
        "본문 상용구 문단 수는 1 이상이어야 합니다."
    );
    ensure!(
        entry.created_at <= entry.modified_at,
        "본문 상용구의 생성/수정 시각 순서가 올바르지 않습니다."
    );
    valid_stats(&entry.stats)?;
    entry.payload_bytes()?;
    Ok(())
}
fn validate(store: &Store) -> Result<()> {
    ensure!(
        store.schema_version == 1,
        "지원하지 않는 본문 상용구 schemaVersion입니다."
    );
    ensure!(
        store.entries.len() <= MAX_ENTRIES,
        "본문 상용구는 1000개까지 저장할 수 있습니다."
    );
    let mut ids = BTreeSet::new();
    let mut names = BTreeSet::new();
    for entry in &store.entries {
        validate_entry(entry)?;
        ensure!(
            ids.insert(&entry.id) && names.insert(&entry.name),
            "본문 상용구 식별자 또는 이름이 중복됩니다."
        );
    }
    Ok(())
}
fn read_path(path: &Path) -> Result<Snapshot> {
    check_read(path)?;
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Snapshot {
                path: path.to_owned(),
                revision: 0,
                entries: Vec::new(),
            });
        }
        Err(error) => {
            return Err(error).with_context(|| {
                format!("본문 상용구 저장소를 열지 못했습니다: {}", path.display())
            });
        }
    };
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file(),
        "본문 상용구 저장소는 일반 파일이어야 합니다."
    );
    ensure!(
        metadata.len() <= MAX_STORE as u64,
        "본문 상용구 저장소가 32 MiB 한도를 초과했습니다."
    );
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(MAX_STORE as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= MAX_STORE,
        "본문 상용구 저장소가 32 MiB 한도를 초과했습니다."
    );
    let store: Store = serde_json::from_slice(&bytes)
        .context("본문 상용구 저장소 형식이 올바르지 않습니다. 기존 파일은 보존했습니다.")?;
    validate(&store)?;
    Ok(Snapshot {
        path: path.to_owned(),
        revision: store.revision,
        entries: store.entries,
    })
}

struct BoundedJson {
    bytes: Vec<u8>,
}
impl Write for BoundedJson {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let size = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .ok_or_else(|| std::io::Error::other("본문 상용구 크기 한도 초과"))?;
        if size > MAX_STORE {
            return Err(std::io::Error::other(
                "본문 상용구 저장소가 32 MiB 한도를 초과했습니다.",
            ));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn write_snapshot(snapshot: &Snapshot) -> Result<()> {
    let store = Store {
        schema_version: 1,
        revision: snapshot.revision,
        entries: snapshot.entries.clone(),
    };
    validate(&store)?;
    let mut encoded = BoundedJson { bytes: Vec::new() };
    serde_json::to_writer_pretty(&mut encoded, &store)
        .context("본문 상용구 저장소를 직렬화하지 못했습니다.")?;
    // Atomic replacement and unique temporary files are shared with document
    // exports. No fixed .tmp name can collide across writers or redirect IO.
    hwpcraft_formats::atomic_write(&snapshot.path, &encoded.bytes)
}
fn revision(snapshot: &Snapshot, expected: Option<u64>) -> Result<u64> {
    if let Some(expected) = expected {
        ensure!(
            expected == snapshot.revision,
            "본문 상용구 저장소가 변경되었습니다. 목록을 새로 읽으세요 (예상 {expected}, 현재 {}).",
            snapshot.revision
        );
    }
    snapshot
        .revision
        .checked_add(1)
        .ok_or_else(|| anyhow!("본문 상용구 revision 한도를 초과했습니다."))
}
fn now() -> Result<u64> {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("시스템 시각이 올바르지 않습니다.")?
            .as_millis(),
    )
    .context("본문 상용구 시각의 한도를 초과했습니다.")
}
fn new_id(entries: &[Entry], time: u64) -> Result<String> {
    for _ in 0..16 {
        let sequence = NEXT_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| anyhow!("본문 상용구 식별자 한도를 초과했습니다."))?;
        let id = format!(
            "body-{time:016x}-{:08x}-{sequence:016x}",
            std::process::id()
        );
        if !entries.iter().any(|entry| entry.id == id) {
            return Ok(id);
        }
    }
    bail!("충돌하지 않는 본문 상용구 식별자를 만들지 못했습니다.")
}
fn lock_for(path: &Path) -> Result<StoreLock> {
    let lock = mutation_gates(path)?;
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).context("본문 상용구 설정 폴더를 만들지 못했습니다.")?;
    }
    // Repeat gates after creating missing parent components.
    mutation_gates(path)?;
    StoreLock::acquire(&lock)
}
fn upsert_path(
    path: &Path,
    mut entry: NewEntry,
    overwrite: bool,
    expected_revision: Option<u64>,
) -> Result<(Snapshot, Entry)> {
    entry.name = entry.name.trim().to_owned();
    valid_name(&entry.name)?;
    valid_text(&entry.description, 1024, "본문 상용구 설명", true)?;
    valid_text(&entry.summary, 1000, "본문 상용구 요약", true)?;
    ensure!(
        entry.paragraphs > 0,
        "본문 상용구 문단 수는 1 이상이어야 합니다."
    );
    valid_stats(&entry.stats)?;
    ensure!(
        !entry.payload.is_empty() && entry.payload.len() <= MAX_PAYLOAD,
        "본문 상용구 payload는 1바이트–8 MiB여야 합니다."
    );
    let _lock = lock_for(path)?;
    // A list shown by another process is not authoritative. Read the latest
    // revision only after acquiring the common persistent OS lock.
    let mut snapshot = read_path(path)?;
    let next = revision(&snapshot, expected_revision)?;
    let old = snapshot
        .entries
        .iter()
        .position(|existing| existing.name == entry.name);
    ensure!(
        old.is_none() || overwrite,
        "같은 이름의 본문 상용구가 있습니다. 덮어쓰기를 선택하세요."
    );
    ensure!(
        old.is_some() || snapshot.entries.len() < MAX_ENTRIES,
        "본문 상용구는 1000개까지 저장할 수 있습니다."
    );
    let time = now()?;
    let (id, created_at, modified_at) = if let Some(index) = old {
        let existing = &snapshot.entries[index];
        (
            existing.id.clone(),
            existing.created_at,
            time.max(
                existing
                    .modified_at
                    .checked_add(1)
                    .ok_or_else(|| anyhow!("본문 상용구 수정 시각의 한도를 초과했습니다."))?,
            ),
        )
    } else {
        (new_id(&snapshot.entries, time)?, time, time)
    };
    let stored = Entry {
        id,
        name: entry.name,
        description: entry.description,
        summary: entry.summary,
        paragraphs: entry.paragraphs,
        stats: entry.stats,
        payload: STANDARD.encode(entry.payload),
        payload_format: "hwpx".into(),
        created_at,
        modified_at,
    };
    if let Some(index) = old {
        snapshot.entries[index] = stored.clone();
    } else {
        snapshot.entries.push(stored.clone());
    }
    snapshot.revision = next;
    write_snapshot(&snapshot)?;
    Ok((snapshot, stored))
}
fn delete_path(path: &Path, id: &str, expected_revision: Option<u64>) -> Result<(Snapshot, Entry)> {
    valid_id(id)?;
    let _lock = lock_for(path)?;
    let mut snapshot = read_path(path)?;
    let next = revision(&snapshot, expected_revision)?;
    let index = snapshot
        .entries
        .iter()
        .position(|entry| entry.id == id)
        .ok_or_else(|| anyhow!("본문 상용구를 찾지 못했습니다."))?;
    let deleted = snapshot.entries.remove(index);
    snapshot.revision = next;
    write_snapshot(&snapshot)?;
    Ok((snapshot, deleted))
}

pub(crate) fn read() -> Result<Snapshot> {
    read_path(&path()?)
}
pub(crate) fn upsert(
    entry: NewEntry,
    overwrite: bool,
    expected_revision: Option<u64>,
) -> Result<(Snapshot, Entry)> {
    upsert_path(&path()?, entry, overwrite, expected_revision)
}
pub(crate) fn delete(id: &str, expected_revision: Option<u64>) -> Result<(Snapshot, Entry)> {
    delete_path(&path()?, id, expected_revision)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::{Arc, Barrier};

    fn entry(name: &str) -> NewEntry {
        NewEntry {
            name: name.into(),
            description: "자체 서식 상용구".into(),
            summary: "자체 본문 😀".into(),
            paragraphs: 1,
            stats: json!({"tables":0,"objects":1}),
            payload: b"PK\x03\x04own opaque payload\xff".to_vec(),
        }
    }
    fn target(dir: &tempfile::TempDir) -> PathBuf {
        dir.path().join(FILE_NAME)
    }
    fn sample() -> Entry {
        Entry {
            id: "body-own-test".into(),
            name: "샘플".into(),
            description: String::new(),
            summary: "자체 샘플".into(),
            paragraphs: 1,
            stats: json!({}),
            payload: STANDARD.encode(b"own binary payload\xff"),
            payload_format: "hwpx".into(),
            created_at: 1,
            modified_at: 1,
        }
    }
    fn seed(path: &Path) -> Result<Entry> {
        let e = sample();
        write_snapshot(&Snapshot {
            path: path.into(),
            revision: 1,
            entries: vec![e.clone()],
        })?;
        Ok(e)
    }

    #[test]
    fn roundtrip_overwrite_and_delete_preserve_identity_and_plain_text_store() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = target(&dir);
        let legacy = dir.path().join("autotext.json");
        std::fs::write(&legacy, b"existing plain-text store must remain untouched")?;
        let lock = path.with_file_name(LOCK_NAME);
        std::fs::write(&lock, b"persistent inode marker")?;
        assert_eq!(read_path(&path)?.revision, 0);
        let (first, added) = upsert_path(&path, entry("  인사😀  "), false, Some(0))?;
        assert_eq!(first.path, path);
        assert_eq!(first.revision, 1);
        assert_eq!(added.name, "인사😀");
        assert_eq!(added.payload_bytes()?, entry("unused").payload);
        assert_eq!(read_path(&path)?.entries, first.entries);
        let value: Value = serde_json::from_slice(&std::fs::read(&path)?)?;
        assert_eq!(value["schemaVersion"], 1);
        assert_eq!(value["entries"][0]["payloadFormat"], "hwpx");
        assert!(value["entries"][0].get("createdAt").is_some());
        assert!(value.get("path").is_none());
        let mut changed = entry("인사😀");
        changed.payload = b"updated opaque bytes".to_vec();
        let (second, replaced) = upsert_path(&path, changed, true, Some(1))?;
        assert_eq!(second.revision, 2);
        assert_eq!(second.entries.len(), 1);
        assert_eq!(replaced.id, added.id);
        assert_eq!(replaced.created_at, added.created_at);
        assert!(replaced.modified_at > added.modified_at);
        let (third, removed) = delete_path(&path, &added.id, Some(2))?;
        assert_eq!(third.revision, 3);
        assert!(third.entries.is_empty());
        assert_eq!(removed, replaced);
        assert!(read_path(&path)?.entries.is_empty());
        assert_eq!(std::fs::read(lock)?, b"persistent inode marker");
        assert_eq!(
            std::fs::read(legacy)?,
            b"existing plain-text store must remain untouched"
        );
        Ok(())
    }

    #[test]
    fn conflicts_unknown_ids_and_revision_overflow_keep_existing_bytes() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = target(&dir);
        let (_, e) = upsert_path(&path, entry("기존"), false, None)?;
        let before = std::fs::read(&path)?;
        assert!(upsert_path(&path, entry("기존"), false, Some(1)).is_err());
        assert!(upsert_path(&path, entry("기존"), true, Some(0)).is_err());
        assert!(delete_path(&path, &e.id, Some(0)).is_err());
        assert!(delete_path(&path, "missing-id", Some(1)).is_err());
        assert_eq!(std::fs::read(&path)?, before);
        let mut snapshot = read_path(&path)?;
        snapshot.revision = u64::MAX;
        write_snapshot(&snapshot)?;
        let overflow = std::fs::read(&path)?;
        assert!(upsert_path(&path, entry("다른 이름"), false, Some(u64::MAX)).is_err());
        assert!(delete_path(&path, &e.id, Some(u64::MAX)).is_err());
        assert_eq!(std::fs::read(path)?, overflow);
        Ok(())
    }

    #[test]
    fn malformed_schema_metadata_duplicates_and_base64_are_never_replaced() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = target(&dir);
        let good = serde_json::to_value(Store {
            schema_version: 1,
            revision: 7,
            entries: vec![sample()],
        })?;
        let mut bad = Vec::new();
        bad.push(json!({}));
        for (key, value) in [("schemaVersion", json!(2)), ("extra", json!(true))] {
            let mut value_copy = good.clone();
            value_copy[key] = value;
            bad.push(value_copy);
        }
        for (key, value) in [
            ("payload", json!("Zh==")),
            ("payload", json!("YQ")),
            ("payload", json!("invalid!")),
            ("payloadFormat", json!("hwp")),
            ("paragraphs", json!(0)),
            ("name", json!(" padded ")),
            ("modifiedAt", json!(0)),
            ("unexpected", json!(true)),
        ] {
            let mut value_copy = good.clone();
            value_copy["entries"][0][key] = value;
            bad.push(value_copy);
        }
        let mut duplicate = good.clone();
        let first = duplicate["entries"][0].clone();
        duplicate["entries"].as_array_mut().unwrap().push(first);
        bad.push(duplicate);
        let mut duplicate_id = good.clone();
        let mut e = duplicate_id["entries"][0].clone();
        e["name"] = json!("다른 이름");
        duplicate_id["entries"].as_array_mut().unwrap().push(e);
        bad.push(duplicate_id);
        let mut duplicate_name = good.clone();
        let mut e = duplicate_name["entries"][0].clone();
        e["id"] = json!("another-id");
        duplicate_name["entries"].as_array_mut().unwrap().push(e);
        bad.push(duplicate_name);
        for malformed in bad {
            let bytes = serde_json::to_vec(&malformed)?;
            std::fs::write(&path, &bytes)?;
            assert!(read_path(&path).is_err(), "{malformed}");
            assert!(upsert_path(&path, entry("새 이름"), false, None).is_err());
            assert!(delete_path(&path, "body-own-test", None).is_err());
            assert_eq!(std::fs::read(&path)?, bytes);
        }
        std::fs::write(&path, b"{truncated")?;
        assert!(upsert_path(&path, entry("새 이름"), false, None).is_err());
        assert_eq!(std::fs::read(path)?, b"{truncated");
        Ok(())
    }

    #[test]
    fn name_payload_entry_and_serialized_size_limits_preserve_previous_store() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = target(&dir);
        seed(&path)?;
        let before = std::fs::read(&path)?;
        let mut bad = Vec::new();
        for name in [" ".to_owned(), "가".repeat(65), "bad\0name".into()] {
            bad.push(entry(&name));
        }
        let mut e = entry("description");
        e.description = "설".repeat(1025);
        bad.push(e);
        let mut e = entry("summary");
        e.summary = "요".repeat(1001);
        bad.push(e);
        let mut e = entry("empty");
        e.payload.clear();
        bad.push(e);
        let mut e = entry("large");
        e.payload = vec![0; MAX_PAYLOAD + 1];
        bad.push(e);
        let mut e = entry("no paragraphs");
        e.paragraphs = 0;
        bad.push(e);
        for e in bad {
            assert!(upsert_path(&path, e, false, None).is_err());
            assert_eq!(std::fs::read(&path)?, before);
        }
        let mut decoded_too_large = sample();
        decoded_too_large.payload = "A".repeat(MAX_ENCODED);
        assert!(decoded_too_large.payload_bytes().is_err());
        let mut e = entry("serialized-overflow");
        e.stats = Value::String("s".repeat(MAX_STORE));
        assert!(upsert_path(&path, e, false, None).is_err());
        assert_eq!(std::fs::read(&path)?, before);
        let mut entries = Vec::new();
        for index in 0..MAX_ENTRIES {
            let mut e = sample();
            e.id = format!("entry-{index}");
            e.name = format!("name-{index}");
            entries.push(e);
        }
        write_snapshot(&Snapshot {
            path: path.clone(),
            revision: 9,
            entries,
        })?;
        let full = std::fs::read(&path)?;
        assert!(upsert_path(&path, entry("over-capacity"), false, Some(9)).is_err());
        assert_eq!(std::fs::read(&path)?, full);
        let (_, replaced) = upsert_path(&path, entry("name-0"), true, Some(9))?;
        assert_eq!(replaced.id, "entry-0");
        let mut file = OpenOptions::new().write(true).open(&path)?;
        file.set_len(MAX_STORE as u64 + 1)?;
        file.flush()?;
        assert!(read_path(&path).is_err());
        assert!(upsert_path(&path, entry("no truncate"), false, None).is_err());
        assert_eq!(std::fs::metadata(path)?.len(), MAX_STORE as u64 + 1);
        Ok(())
    }

    #[test]
    fn authorization_checks_both_roots_before_store_read_or_lock_creation() -> Result<()> {
        use hwpcraft_formats::file_access::{AutomationRoots, confine};
        let dir = tempfile::tempdir()?;
        let allowed = dir.path().join("allowed");
        let other = dir.path().join("other");
        std::fs::create_dir_all(&allowed)?;
        std::fs::create_dir_all(&other)?;
        let path = allowed.join(FILE_NAME);
        seed(&path)?;
        let before = std::fs::read(&path)?;
        let lock = path.with_file_name(LOCK_NAME);
        assert!(!lock.exists());
        for (read, write) in [
            (Some(allowed.as_path()), None),
            (None, Some(allowed.as_path())),
            (Some(allowed.as_path()), Some(other.as_path())),
            (Some(other.as_path()), Some(allowed.as_path())),
        ] {
            let roots = AutomationRoots::new(read, write).map_err(anyhow::Error::msg)?;
            assert!(
                confine(roots.as_ref(), || upsert_path(
                    &path,
                    entry("unauthorized"),
                    false,
                    None
                ))
                .is_err()
            );
            assert!(confine(roots.as_ref(), || delete_path(&path, "body-own-test", None)).is_err());
            assert!(!lock.exists());
            assert_eq!(std::fs::read(&path)?, before);
        }
        let roots =
            AutomationRoots::new(Some(&allowed), Some(&allowed)).map_err(anyhow::Error::msg)?;
        let (saved, _) = confine(roots.as_ref(), || {
            upsert_path(&path, entry("authorized"), false, Some(1))
        })?;
        assert_eq!(saved.revision, 2);
        assert!(lock.exists());
        Ok(())
    }

    #[test]
    fn concurrent_expected_revisions_have_only_one_winner() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = target(&dir);
        let barrier = Arc::new(Barrier::new(3));
        let mut threads = Vec::new();
        for name in ["writer-a", "writer-b"] {
            let path = path.clone();
            let barrier = barrier.clone();
            threads.push(std::thread::spawn(move || {
                barrier.wait();
                upsert_path(&path, entry(name), false, Some(0))
            }));
        }
        barrier.wait();
        let results = threads
            .into_iter()
            .map(|t| t.join().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
        let saved = read_path(&path)?;
        assert_eq!(saved.revision, 1);
        assert_eq!(saved.entries.len(), 1);
        upsert_path(&path, entry("writer-c"), false, Some(1))?;
        let saved = read_path(&path)?;
        assert_eq!(saved.revision, 2);
        assert_eq!(saved.entries.len(), 2);
        Ok(())
    }

    #[test]
    fn persistent_os_lock_blocks_another_process_without_lost_updates() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = target(&dir);
        let (first, _) = upsert_path(&path, entry("parent"), false, Some(0))?;
        let lock = lock_for(&path)?;
        let before = std::fs::read(&path)?;
        let result = std::process::Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "body_autotext_store::tests::lock_child",
                "--ignored",
                "--nocapture",
            ])
            .env("HWPCRAFT_BODY_STORE_TEST_PATH", &path)
            .output()?;
        assert!(
            result.status.success(),
            "{} {}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(std::fs::read(&path)?, before);
        drop(lock);
        let (second, _) = upsert_path(&path, entry("after child"), false, Some(first.revision))?;
        assert_eq!(second.entries.len(), 2);
        assert_eq!(second.revision, 2);
        let child = std::process::Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "body_autotext_store::tests::lock_child",
                "--ignored",
                "--nocapture",
            ])
            .env("HWPCRAFT_BODY_STORE_TEST_PATH", &path)
            .env("HWPCRAFT_BODY_STORE_TEST_WRITE", "1")
            .output()?;
        assert!(
            child.status.success(),
            "{} {}",
            String::from_utf8_lossy(&child.stdout),
            String::from_utf8_lossy(&child.stderr)
        );
        let latest = read_path(&path)?;
        assert_eq!(latest.revision, 3);
        assert_eq!(latest.entries.len(), 3);
        for name in ["parent", "after child", "child successful write"] {
            assert!(latest.entries.iter().any(|entry| entry.name == name));
        }
        Ok(())
    }
    #[test]
    #[ignore = "subprocess helper; invoked by persistent_os_lock test"]
    fn lock_child() -> Result<()> {
        let path = PathBuf::from(
            std::env::var_os("HWPCRAFT_BODY_STORE_TEST_PATH")
                .ok_or_else(|| anyhow!("child test path missing"))?,
        );
        if std::env::var_os("HWPCRAFT_BODY_STORE_TEST_WRITE").is_some() {
            upsert_path(&path, entry("child successful write"), false, None)?;
            return Ok(());
        }
        let error = upsert_path(&path, entry("child cannot write"), false, None)
            .err()
            .ok_or_else(|| anyhow!("child acquired parent's lock"))?;
        ensure!(
            error.to_string().contains("다른 창이나 프로세스"),
            "{error:#}"
        );
        Ok(())
    }

    #[test]
    fn io_failure_and_nonregular_store_leave_previous_bytes_intact() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = target(&dir);
        seed(&path)?;
        let before = std::fs::read(&path)?;
        std::fs::create_dir(path.with_file_name(LOCK_NAME))?;
        assert!(upsert_path(&path, entry("failed lock open"), false, None).is_err());
        assert_eq!(std::fs::read(&path)?, before);
        let directory = dir.path().join("store-is-directory");
        std::fs::create_dir(&directory)?;
        assert!(read_path(&directory).is_err());
        assert!(directory.is_dir());
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn failed_atomic_replace_of_readonly_target_keeps_original_bytes() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = target(&dir);
        seed(&path)?;
        let before = std::fs::read(&path)?;
        struct Restore {
            path: PathBuf,
            permissions: std::fs::Permissions,
        }
        impl Drop for Restore {
            fn drop(&mut self) {
                let _ = std::fs::set_permissions(&self.path, self.permissions.clone());
            }
        }
        let original = std::fs::metadata(&path)?.permissions();
        let _restore = Restore {
            path: path.clone(),
            permissions: original.clone(),
        };
        let mut readonly = original;
        readonly.set_readonly(true);
        std::fs::set_permissions(&path, readonly)?;
        assert!(upsert_path(&path, entry("cannot replace"), false, None).is_err());
        assert_eq!(std::fs::read(path)?, before);
        Ok(())
    }
}
