//! Owner: shell/page area. 문서 이력 (HWP 5 `DocHistory` storage). Hangul's 문서 이력
//! 관리 keeps one `VersionLog<n>` stream per recorded version and the newest recorded
//! version as an HML document in `HistoryLastDoc`. Each stream is a run of records:
//! tag (u8), data length (u32 LE), data. Tags: 0x10 start, 0x11 end, 0x20 version
//! (u32), 0x21 date (Windows SYSTEMTIME), 0x22 writer, 0x23 description (UTF-16LE),
//! 0x30 difference to the previous version (HML diff, not applied here), 0x31 the
//! last document (UTF-16LE HML). Streams are raw-deflated when the file header says so.
//! Reading only: the storage is copied unchanged when RHWP saves the file.
use anyhow::{Context, Result, bail};
use std::io::{Cursor, Read};

/// One recorded version.
#[derive(Clone, Debug, PartialEq)]
pub struct HistoryEntry {
    /// Position in the file (`VersionLog<index>`), oldest first.
    pub index: usize,
    /// The 0x20 record. Its meaning is not documented; every version in the one sample
    /// with history holds 0x0100_0000, so it is not a running number.
    pub version: Option<u32>,
    /// `YYYY-MM-DD HH:MM:SS`, as Hangul stored it (local time).
    pub date: Option<String>,
    pub writer: String,
    pub description: String,
}

/// The versions recorded in an HWP file; empty when it has no history (or is not HWP 5).
pub fn entries(file: &[u8]) -> Result<Vec<HistoryEntry>> {
    let Some(mut compound) = open(file) else {
        return Ok(Vec::new());
    };
    let compressed = compressed(&mut compound)?;
    let mut out = Vec::new();
    for index in 0.. {
        let path = format!("/DocHistory/VersionLog{index}");
        if !compound.is_stream(&path) {
            break;
        }
        let data = stream(&mut compound, &path, compressed)?;
        let mut entry = HistoryEntry {
            index,
            version: None,
            date: None,
            writer: String::new(),
            description: String::new(),
        };
        for (tag, value) in records(&data)? {
            match tag {
                0x20 if value.len() >= 4 => {
                    entry.version = Some(u32::from_le_bytes(value[..4].try_into()?));
                }
                0x21 => entry.date = system_time(value),
                0x22 => entry.writer = utf16(value),
                0x23 => entry.description = utf16(value),
                _ => {}
            }
        }
        out.push(entry);
    }
    Ok(out)
}

/// The newest recorded version as HML text, if the file keeps one.
pub fn last_version_hml(file: &[u8]) -> Result<Option<String>> {
    let Some(mut compound) = open(file) else {
        return Ok(None);
    };
    let path = "/DocHistory/HistoryLastDoc";
    if !compound.is_stream(path) {
        return Ok(None);
    }
    let compressed = compressed(&mut compound)?;
    let data = stream(&mut compound, path, compressed)?;
    Ok(records(&data)?
        .into_iter()
        .find(|(tag, _)| *tag == 0x31)
        .map(|(_, value)| utf16(value)))
}

fn open(file: &[u8]) -> Option<cfb::CompoundFile<Cursor<&[u8]>>> {
    let compound = cfb::CompoundFile::open(Cursor::new(file)).ok()?;
    compound.is_storage("/DocHistory").then_some(compound)
}

/// File header property bit 0: streams are compressed.
fn compressed(compound: &mut cfb::CompoundFile<Cursor<&[u8]>>) -> Result<bool> {
    let mut header = Vec::new();
    compound
        .open_stream("/FileHeader")
        .context("FileHeader 없음")?
        .read_to_end(&mut header)?;
    let flags = header.get(36..40).context("FileHeader가 짧습니다")?;
    Ok(flags[0] & 1 == 1)
}

fn stream(
    compound: &mut cfb::CompoundFile<Cursor<&[u8]>>,
    path: &str,
    compressed: bool,
) -> Result<Vec<u8>> {
    let mut raw = Vec::new();
    compound.open_stream(path)?.read_to_end(&mut raw)?;
    if !compressed {
        return Ok(raw);
    }
    let mut data = Vec::new();
    flate2::read::DeflateDecoder::new(raw.as_slice())
        .read_to_end(&mut data)
        .with_context(|| format!("{path} 압축 풀기 실패"))?;
    Ok(data)
}

fn records(data: &[u8]) -> Result<Vec<(u8, &[u8])>> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < data.len() {
        let Some(head) = data.get(at..at + 5) else {
            bail!("이력 레코드 머리가 잘렸습니다 ({at})");
        };
        let len = u32::from_le_bytes(head[1..5].try_into()?) as usize;
        let start = at + 5;
        let Some(value) = start.checked_add(len).and_then(|end| data.get(start..end)) else {
            bail!("이력 레코드가 잘렸습니다 ({at})");
        };
        out.push((head[0], value));
        at = start + len;
    }
    Ok(out)
}

fn utf16(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .collect();
    String::from_utf16_lossy(&units)
        .trim_end_matches('\0')
        .to_owned()
}

/// Windows SYSTEMTIME: year, month, day of week, day, hour, minute, second, milliseconds.
fn system_time(bytes: &[u8]) -> Option<String> {
    let v: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .take(8)
        .map(|c| u16::from_le_bytes(*c))
        .collect();
    let [year, month, _, day, hour, minute, second, _] = v[..] else {
        return None;
    };
    let valid = (1..=12).contains(&month)
        && (1..=31).contains(&day)
        && hour < 24
        && minute < 60
        && second < 61;
    valid.then(|| format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn record(tag: u8, data: &[u8]) -> Vec<u8> {
        let mut out = vec![tag];
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(data);
        out
    }

    fn wide(text: &str) -> Vec<u8> {
        text.encode_utf16().flat_map(u16::to_le_bytes).collect()
    }

    fn deflate(data: &[u8]) -> Result<Vec<u8>> {
        let mut e = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        e.write_all(data)?;
        Ok(e.finish()?)
    }

    /// A compound file with a compressed `FileHeader` flag and the given history streams.
    fn hwp(logs: &[Vec<u8>], last: Option<&str>) -> Result<Vec<u8>> {
        let mut compound = cfb::CompoundFile::create(Cursor::new(Vec::new()))?;
        let mut header = vec![0u8; 256];
        header[36] = 1;
        compound.create_stream("/FileHeader")?.write_all(&header)?;
        compound.create_storage("/DocHistory")?;
        for (i, log) in logs.iter().enumerate() {
            compound
                .create_stream(format!("/DocHistory/VersionLog{i}"))?
                .write_all(&deflate(log)?)?;
        }
        if let Some(hml) = last {
            compound
                .create_stream("/DocHistory/HistoryLastDoc")?
                .write_all(&deflate(&record(0x31, &wide(hml)))?)?;
        }
        compound.flush()?;
        Ok(compound.into_inner().into_inner())
    }

    fn log(version: u32, date: [u16; 8], writer: &str, description: &str) -> Vec<u8> {
        let date: Vec<u8> = date.iter().flat_map(|v| v.to_le_bytes()).collect();
        [
            record(0x10, &[0, 0, 0, 0, 0x9f, 0]),
            record(0x20, &version.to_le_bytes()),
            record(0x21, &date),
            record(0x22, &wide(writer)),
            record(0x23, &wide(description)),
            record(0x30, &wide("<HMLDIFF/>")),
            record(0x11, &[]),
        ]
        .concat()
    }

    #[test]
    fn versions_and_the_last_document_are_read() -> Result<()> {
        let file = hwp(
            &[
                log(
                    1,
                    [2004, 9, 4, 16, 14, 25, 45, 733],
                    "(주)한글과컴퓨터",
                    "논문 개요",
                ),
                log(2, [2004, 9, 6, 18, 8, 46, 24, 419], "작성자", "본문 다듬기"),
            ],
            Some("<HWPML Style=\"embed\"/>"),
        )?;
        let got = entries(&file)?;
        assert_eq!(got.len(), 2);
        assert_eq!(
            got[0],
            HistoryEntry {
                index: 0,
                version: Some(1),
                date: Some("2004-09-16 14:25:45".into()),
                writer: "(주)한글과컴퓨터".into(),
                description: "논문 개요".into(),
            }
        );
        assert_eq!(got[1].description, "본문 다듬기");
        assert_eq!(
            last_version_hml(&file)?.as_deref(),
            Some("<HWPML Style=\"embed\"/>")
        );
        Ok(())
    }

    #[test]
    fn files_without_history_or_with_a_cut_record_are_handled() -> Result<()> {
        assert!(entries(b"not a compound file")?.is_empty());
        assert_eq!(last_version_hml(b"")?, None);
        let none = hwp(&[], None)?;
        assert!(entries(&none)?.is_empty());
        assert_eq!(last_version_hml(&none)?, None);
        let mut cut = log(1, [2004, 9, 4, 16, 14, 25, 45, 0], "a", "b");
        cut.truncate(cut.len() - 8);
        assert!(entries(&hwp(&[cut], None)?).is_err());
        Ok(())
    }
}
