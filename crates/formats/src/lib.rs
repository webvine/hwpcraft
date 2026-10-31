#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
pub mod auto_number_formats;
pub mod doc_history;
pub mod docx_export;
pub mod file_access;
mod memo_formats;
mod page_counter_formats;
mod page_number_formats;
pub mod pdf_compact;
mod table_repair;
mod textbox_formats;
use anyhow::{Context, Result, bail};
use hwpcraft_doc::DocumentCore;
use std::{
    io::{Read, Write},
    path::Path,
};

pub const MAX_DOCUMENT_BYTES: u64 = 128 * 1024 * 1024;

/// Open an HWP/HWPX/HWP3 document. A password-protected file fails with an error
/// [`needs_password`] recognises; open it again with [`open_with_password`].
pub fn open(path: &Path) -> Result<DocumentCore> {
    open_inner(path, None)
}

/// Open a password-protected document (HWP5 EncryptVersion 4, HWP3, ODF-encrypted HWPX).
/// The password is passed as UTF-8, the same way the RHWP CLI does.
pub fn open_with_password(path: &Path, password: &str) -> Result<DocumentCore> {
    if password.is_empty() {
        bail!("문서 암호를 입력하세요.");
    }
    open_inner(path, Some(password.as_bytes()))
}

fn open_inner(path: &Path, password: Option<&[u8]>) -> Result<DocumentCore> {
    file_access::check_read(&path.to_string_lossy()).map_err(anyhow::Error::msg)?;
    let file = std::fs::File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        bail!("일반 문서 파일을 선택하세요.");
    }
    if metadata.len() > MAX_DOCUMENT_BYTES {
        bail!("문서가 128 MB 제한을 초과했습니다.");
    }
    let mut bytes = Vec::new();
    file.take(MAX_DOCUMENT_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_DOCUMENT_BYTES {
        bail!("문서가 128 MB 제한을 초과했습니다.");
    }
    let mut core = match page_counter_formats::open(&bytes, password)? {
        Some(core) => core,
        None => page_number_formats::open(&bytes, password)?,
    };
    // Damaged row/column counts would otherwise panic inside RHWP rendering.
    table_repair::repair(&mut core);
    // HWPX may intentionally omit generated line caches. Ask RHWP to repair
    // only layouts identified by its validation report before the first render.
    core.reflow_linesegs_on_demand();
    Ok(core)
}

/// A throwaway core holding `doc` only for serialization. RHWP's HWP/HWPX writers
/// read the model and the core's source format (and file name, here the default of
/// `new_empty`), not layout, so `set_document`'s full layout (about 0.25 s on 380
/// pages, once per number kind) is skipped. The source format decides the writers'
/// adapters: an HWPX copy of an `Hwp` source is marked HWP5-origin, and its tables
/// treated as characters then flow differently when the copy is read back. Never
/// render from this core.
pub(crate) fn export_core(
    doc: hwpcraft_doc::model::document::Document,
    source_format: hwpcraft_doc::FileFormat,
) -> DocumentCore {
    let mut core = DocumentCore::new_empty();
    *core.document_mut() = doc;
    core.set_source_format(source_format);
    core
}

/// Did `open` fail because the document is encrypted and no password was given?
/// Matches RHWP's `ParseError::EncryptedDocument` message.
pub fn needs_password(error: &anyhow::Error) -> bool {
    format!("{error:#}").contains("비밀번호가 필요한 암호 문서")
}

/// Did `open_with_password` fail because the password did not match?
/// Matches RHWP's `CryptoError::WrongPassword` message.
pub fn wrong_password(error: &anyhow::Error) -> bool {
    format!("{error:#}").contains("비밀번호가 일치하지 않거나")
}

/// A failed write never truncates the user's existing document.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    file_access::check_write(&path.to_string_lossy()).map_err(anyhow::Error::msg)?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    // Last point before the existing file is replaced; the temporary file is
    // removed when it drops.
    if file_access::cancelled() {
        bail!("요청이 취소되어 파일을 저장하지 않았습니다. 기존 파일은 그대로입니다.");
    }
    file.persist(path)
        .map_err(|e| e.error)
        .context("파일을 교체하지 못했습니다. 다른 프로그램에서 열려 있는지 확인하세요.")?;
    Ok(())
}

pub fn save(core: &DocumentCore, path: &Path) -> Result<()> {
    save_inner(core, path, None)
}

/// Save as a password-protected HWP (EncryptVersion 4) or HWPX (ODF AES-256-CBC).
/// Other formats cannot carry a password and are refused before anything is written.
pub fn save_with_password(core: &DocumentCore, path: &Path, password: &str) -> Result<()> {
    if password.is_empty() {
        bail!("문서 암호를 입력하세요.");
    }
    save_inner(core, path, Some(password.as_bytes()))
}

fn save_inner(core: &DocumentCore, path: &Path, password: Option<&[u8]>) -> Result<()> {
    file_access::check_write(&path.to_string_lossy()).map_err(anyhow::Error::msg)?;
    let extension = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if password.is_some() && !matches!(extension.as_str(), "hwp" | "hwpx") {
        bail!("문서 암호는 HWP, HWPX 저장에만 적용됩니다.");
    }
    let bytes = match (extension.as_str(), password) {
        ("hwp", password) => {
            let prepared = auto_number_formats::prepare_hwp(core)?;
            let baseline = memo_formats::export_hwp(prepared.as_ref().unwrap_or(core), password)?;
            textbox_formats::repair_hwp(baseline, password)?
        }
        ("hwpx", password) => page_number_formats::export(core, password)?,
        ("pdf", _) => {
            let options = pdf_options();
            let pdf = core.render_document_pdf_native_with_options(&options);
            // Every page's render tree stays cached otherwise (hundreds of MB).
            core.release_page_render_caches();
            pdf_compact::compact(pdf.map_err(|e| anyhow::anyhow!("{e}"))?)
        }
        // What DOCX cannot carry is reported by `docx_export::export`; callers that
        // need the list call it directly.
        ("docx", _) => docx_export::export(core)?.bytes,
        ("txt", _) => {
            let mut lines = Vec::new();
            for section in &core.document().sections {
                collect_text(&section.paragraphs, &mut lines, 0);
            }
            lines.join("\r\n").into_bytes()
        }
        _ => bail!("지원하는 저장 형식: HWP, HWPX, PDF, TXT, DOCX"),
    };
    atomic_write(path, &bytes)
}

/// PDF export options with the operating system's Korean font as fallback.
pub fn pdf_options() -> hwpcraft_doc::PdfExportOptions {
    let family = hwpcraft_doc::fonts::default_family().to_owned();
    hwpcraft_doc::PdfExportOptions {
        fallback_serif: family.clone(),
        fallback_sans: family.clone(),
        fallback_mono: family,
        ..Default::default()
    }
}

fn collect_text(
    paragraphs: &[hwpcraft_doc::model::paragraph::Paragraph],
    lines: &mut Vec<String>,
    depth: usize,
) {
    if depth > 64 {
        return;
    }
    for p in paragraphs {
        lines.push(p.text.clone());
        for control in &p.controls {
            if let hwpcraft_doc::model::control::Control::Table(table) = control {
                for cell in &table.cells {
                    collect_text(&cell.paragraphs, lines, depth + 1);
                }
            }
        }
    }
}
