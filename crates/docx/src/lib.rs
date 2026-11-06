//! Owner: format area. DOCX (Word) import and export through WordCraft.
//!
//! Export maps the RHWP model to WordCraft's document model and writes it with
//! `wordcraft-docx`; import reads a package with `wordcraft-docx` and builds an HWPX
//! package from it, which RHWP then opens like any HWPX file.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use anyhow::{Context, Result};
use hwpcraft_doc::DocumentCore;

mod export;
mod fonts;
mod import;
mod units;

/// A finished package and what it could not carry, as Korean "what · count" lines.
pub struct DocxExport {
    pub bytes: Vec<u8>,
    pub omitted: Vec<String>,
}

/// The document as a `.docx` package.
pub fn export(core: &DocumentCore) -> Result<DocxExport> {
    let exported = export::to_wordcraft(core)?;
    let bytes = wordcraft_docx::write(&exported.doc)
        .map_err(|e| anyhow::anyhow!("{e}"))
        .context("DOCX 묶음을 만들지 못했습니다.")?;
    Ok(DocxExport {
        bytes,
        omitted: exported.omitted,
    })
}

/// A document read from `.docx` and what HWP could not carry.
pub struct DocxImport {
    pub core: DocumentCore,
    pub omitted: Vec<String>,
}

/// The `.docx` (or `.docm`/`.dotx`) package converted to an HWP document. An encrypted
/// package needs `password`.
pub fn import(bytes: &[u8], password: Option<&str>) -> Result<DocxImport> {
    let (hwpx, omitted) = docx_to_hwpx(bytes, password)?;
    let core = DocumentCore::from_bytes(&hwpx)
        .map_err(|e| anyhow::anyhow!("{e}"))
        .context("Word 문서를 한글 문서로 바꾸지 못했습니다.")?;
    Ok(DocxImport { core, omitted })
}

/// The HWPX package a `.docx` converts to, with what it could not carry.
pub fn docx_to_hwpx(bytes: &[u8], password: Option<&str>) -> Result<(Vec<u8>, Vec<String>)> {
    // The messages HwpCraft's open path recognises for passwords (RHWP's wording).
    let doc = wordcraft_docx::read_with_password(bytes, password).map_err(|e| match e {
        wordcraft_docx::DocxError::PasswordRequired => {
            anyhow::anyhow!("비밀번호가 필요한 암호 문서입니다 (Word 문서).")
        }
        wordcraft_docx::DocxError::WrongPassword => {
            anyhow::anyhow!("비밀번호가 일치하지 않거나 문서가 손상되었습니다 (Word 문서).")
        }
        e => anyhow::anyhow!("Word 문서를 읽지 못했습니다: {e}"),
    })?;
    let converted = import::to_hwpx(&doc)?;
    Ok((converted.hwpx, converted.omitted))
}
