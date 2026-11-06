//! Owner: object area. Pictures a client hands over with a question, and replacing a
//! picture's image.
//!
//! An AI run may not read files (`RESTRICTED_COMMANDS`: `picture.insert` takes a path), so
//! the AI edit panel gives the pictures attached to the question to the window's session
//! for that run, numbered as the question names them. `picture.insert_attachment` puts one
//! in at the caret and `picture.replace` swaps an existing picture's image (body or, through
//! `hf_objects`, header and footer; object addresses do not reach pictures in table
//! cells) with one of them or, outside an AI
//! run, with a file. The frame keeps its place; by default its height follows the new
//! picture's proportions.

use super::*;
use std::path::PathBuf;

impl Session {
    /// The pictures attached to the question being answered: (number, name, bytes).
    pub fn set_run_pictures(&mut self, pictures: Vec<(usize, String, Vec<u8>)>) {
        self.run_pictures = pictures;
    }

    /// The question ended: its pictures go.
    pub fn clear_run_pictures(&mut self) {
        self.run_pictures.clear();
    }

    /// A session over `core` for running a command on a projection (header and footer
    /// objects, master pages): it keeps this run's restriction, so a command that checks
    /// it inside (`picture.replace`'s path) refuses there too. `pictures` hands it the
    /// question's pictures, for the one command that reads them.
    pub(crate) fn scratch_session(&self, core: DocumentCore, pictures: bool) -> Session {
        let mut scratch = Session::with_core(core, None);
        scratch.commands_restricted = self.commands_restricted;
        if pictures {
            scratch.run_pictures = self.run_pictures.clone();
        }
        scratch
    }

    /// Picture bytes from a file or from an attachment of the question, exactly one of them.
    pub(crate) fn picture_source(
        &self,
        path: Option<PathBuf>,
        attachment: Option<usize>,
    ) -> Result<(String, Vec<u8>)> {
        match (path, attachment) {
            (Some(_), Some(_)) => bail!("path와 attachment 중 하나만 주세요."),
            (None, None) => bail!("바꿀 그림을 path나 attachment로 주세요."),
            (None, Some(n)) => {
                let numbers: Vec<String> = self
                    .run_pictures
                    .iter()
                    .map(|(k, _, _)| k.to_string())
                    .collect();
                self.run_pictures
                    .iter()
                    .find(|(k, _, _)| *k == n)
                    .map(|(_, name, bytes)| (name.clone(), bytes.clone()))
                    .ok_or_else(|| {
                        if numbers.is_empty() {
                            anyhow!("이 질문에 첨부된 그림이 없습니다.")
                        } else {
                            anyhow!(
                                "첨부 그림 {n}이(가) 없습니다. 이 질문의 그림 첨부: {}",
                                numbers.join(", ")
                            )
                        }
                    })
            }
            (Some(path), None) => {
                if self.commands_restricted {
                    bail!(
                        "AI 편집 중에는 파일에서 그림을 읽을 수 없습니다. 질문에 첨부한 그림은 attachment 번호로 주세요."
                    );
                }
                hwpcraft_formats::file_access::check_read(&path.to_string_lossy())
                    .map_err(anyhow::Error::msg)?;
                use std::io::Read;
                let mut bytes = Vec::new();
                std::fs::File::open(&path)?
                    .take(20_000_001)
                    .read_to_end(&mut bytes)?;
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().chars().take(200).collect::<String>())
                    .unwrap_or_default();
                Ok((name, bytes))
            }
        }
    }

    /// Put attachment `n` of the question in at the caret, like `picture.insert`.
    pub fn insert_run_picture(
        &mut self,
        n: usize,
        width_mm: Option<f64>,
        treat_as_char: bool,
    ) -> Result<Option<ObjectAddress>> {
        let (name, bytes) = self.picture_source(None, Some(n))?;
        self.insert_picture_data(&bytes, &name, width_mm, treat_as_char)
    }

    /// Give picture `at` the image `bytes` named `name`. The frame stays where it is; with
    /// `keep_aspect` its height follows the new image at the same width. A description
    /// that only named the old file (what 그림 넣기 leaves) names the new one.
    pub fn replace_picture(
        &mut self,
        at: ObjectAddress,
        name: &str,
        bytes: &[u8],
        keep_aspect: bool,
    ) -> Result<()> {
        if !matches!(self.object(at)?, Control::Picture(_)) {
            bail!("그림이 아닌 개체는 그림 바꾸기를 할 수 없습니다.");
        }
        self.guard_object_lock(at)?;
        let (extension, w, h) = features::checked_picture(bytes)?;
        let props = self.object_properties(at)?["props"].clone();
        let width_hu = props["width"].as_f64().unwrap_or(0.0);
        let old_name = props["description"].as_str().unwrap_or_default().to_owned();
        let file_name = |n: &str| {
            let lower = n.to_ascii_lowercase();
            n.is_empty()
                || [".png", ".jpg", ".jpeg", ".bmp", ".gif"]
                    .iter()
                    .any(|e| lower.ends_with(e))
        };
        let rename = !name.is_empty() && file_name(&old_name) && !name.contains(['\n', '\r']);
        self.transaction(|s| {
            if rename {
                s.set_object_properties(at, &json!({"description": name}))?;
            }
            // The frame first: resizing scales the crop, and the new image's crop must be
            // the whole picture, which assigning it sets last.
            if keep_aspect && width_hu > 0.0 {
                let width_mm = width_hu * 25.4 / 7200.0;
                let height_mm = width_mm * f64::from(h) / f64::from(w);
                s.size_object(at, Some(width_mm), Some(height_mm.clamp(0.1, 600.0)), false)?;
            }
            s.core
                .assign_picture_image_native(
                    at.section,
                    at.paragraph,
                    &[],
                    at.control,
                    bytes,
                    w,
                    h,
                    extension,
                )
                .map_err(err)?;
            // The crop is the whole new image; the range it is measured in (`img_dim`, as
            // 그림 넣기 records it) must be the new image's too, or the old picture's size
            // crops the new one.
            if let Some(Control::Picture(pic)) = s
                .core
                .document_mut()
                .sections
                .get_mut(at.section)
                .and_then(|sec| sec.paragraphs.get_mut(at.paragraph))
                .and_then(|para| para.controls.get_mut(at.control))
            {
                pic.img_dim = (w * 75, h * 75);
            }
            Ok(())
        })
    }
}
