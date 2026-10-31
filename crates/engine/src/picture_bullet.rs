//! Owner: format area. 그림 글머리표 (Hangul 서식 > 글머리표 > 그림 글머리표).
//!
//! The image is embedded once as document BinData and referenced by a new
//! bullet definition: `image_bullet` (HWP) and `<hc:img binaryItemIDRef>`
//! inside the HWPX bullet (written through the bullet's para-head splice, which
//! RHWP otherwise emits without the image). The definition also carries a ●
//! for readers that cannot draw the picture.
use super::*;
use hwpcraft_doc::model::{
    bin_data::{
        BinData, BinDataBytes, BinDataCompression, BinDataContent, BinDataStatus, BinDataType,
    },
    style::Bullet,
};
use serde::Deserialize;
use std::path::PathBuf;

/// (command id, Korean label, shortcut) shown in help and command search.
pub const COMMANDS: &[(&str, &str, &str)] = &[(
    "format.bullet.picture",
    "그림 글머리표 (그림 파일로 글머리표 만들기)",
    "",
)];

/// Parameter help and JSON example per command, for `command_help`.
pub const PARAMS: &[(&str, &str, &str)] = &[(
    "format.bullet.picture",
    "path(필수): PNG·JPG·BMP·GIF 그림(1 MB 이하, 512픽셀 이하). 캐럿 문단 또는 선택한 문단들에 그 그림을 글머리표로 붙입니다. level(선택, 0–6): 수준. 같은 그림의 글머리표는 다시 씁니다. 저장한 HWP·HWPX는 한글과 HwpCraft 모두 그림 글머리표로 보입니다(그림을 그릴 수 없는 프로그램은 ●). 결과: id(글머리표 모양 번호).",
    r#"{"path":"bullet.png"}"#,
)];

const MAX_BYTES: usize = 1_000_000;
const MAX_SIDE: u32 = 512;
/// Shown by readers that cannot draw the image.
const FALLBACK: char = '●';

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PictureBullet {
    path: PathBuf,
    #[serde(default)]
    level: Option<u8>,
}

impl Session {
    /// Return `None` for ids this module does not own.
    pub(crate) fn run_picture_bullet_ext(
        &mut self,
        id: &str,
        params: &Value,
    ) -> Option<Result<Value>> {
        (id == "format.bullet.picture").then(|| {
            serde_json::from_value::<PictureBullet>(params.clone())
                .map_err(|e| anyhow!("{id}: 매개 변수 오류 ({e})"))
                .and_then(|p| self.picture_bullet(p))
        })
    }

    fn picture_bullet(&mut self, p: PictureBullet) -> Result<Value> {
        if p.level.is_some_and(|l| l > 6) {
            bail!("수준은 0–6입니다.");
        }
        let (bytes, extension) = read_bullet_image(&p.path)?;
        self.compound(|s| {
            let id = s.picture_bullet_definition(&bytes, extension);
            let mut apply = json!({"kind": "bullet", "id": id});
            if let Some(level) = p.level {
                apply["level"] = json!(level);
            }
            s.run("format.numbering.apply", apply)?;
            Ok(json!({"id": id}))
        })
    }

    /// The bullet definition for this image: an existing one with the same
    /// embedded bytes, or a new embedded image and bullet.
    fn picture_bullet_definition(&mut self, bytes: &[u8], extension: &str) -> u16 {
        let doc = self.core.document_mut();
        let existing = doc.doc_info.bullets.iter().position(|b| {
            b.image_bullet > 0
                && doc
                    .bin_data_content
                    .iter()
                    .find(|c| i32::from(c.id) == b.image_bullet)
                    .is_some_and(|c| c.data.load_shared().as_ref() == bytes)
        });
        if let Some(index) = existing {
            return (index + 1) as u16;
        }
        // A fresh storage id names the embedded stream; reusing one would swap images.
        let storage_id = doc
            .bin_data_content
            .iter()
            .map(|c| c.id)
            .chain(doc.doc_info.bin_data_list.iter().map(|b| b.storage_id))
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        doc.bin_data_content.push(BinDataContent {
            id: storage_id,
            data: BinDataBytes::from_shared(bytes.to_vec()),
            extension: extension.to_owned(),
        });
        doc.doc_info.bin_data_list.push(BinData {
            raw_data: None,
            // Embedded, default compression, load succeeded.
            attr: 0x0101,
            data_type: BinDataType::Embedding,
            compression: BinDataCompression::Default,
            status: BinDataStatus::Success,
            abs_path: None,
            rel_path: None,
            storage_id,
            extension: Some(extension.to_owned()),
        });
        let id = i32::from(storage_id);
        doc.doc_info.bullets.push(Bullet {
            bullet_char: FALLBACK,
            char_shape_id: u32::MAX,
            // Automatic hanging indent, as Hangul's own bullets.
            attr: 1 << 3,
            text_distance: 50,
            image_bullet: id,
            // contrast, brightness, effect (real picture), image id.
            image_data: [0, 0, 0, (id & 0xff) as u8],
            raw_para_head: Some(format!(
                concat!(
                    r#"<hc:img binaryItemIDRef="image{id}" bright="0" contrast="0" effect="REAL_PIC" alpha="0"/>"#,
                    r#"<hh:paraHead level="0" align="LEFT" useInstWidth="0" autoIndent="1" widthAdjust="0" "#,
                    r#"textOffsetType="PERCENT" textOffset="50" numFormat="DIGIT" charPrIDRef="4294967295" checkable="0"/>"#
                ),
                id = id
            )),
            ..Default::default()
        });
        doc.doc_info.raw_stream = None;
        doc.doc_info.raw_stream_dirty = true;
        doc.doc_info.bullets.len() as u16
    }
}

/// A small bullet image within the automation read roots.
fn read_bullet_image(path: &std::path::Path) -> Result<(Vec<u8>, &'static str)> {
    use std::io::Read;
    hwpcraft_formats::file_access::check_read(&path.to_string_lossy())
        .map_err(anyhow::Error::msg)?;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_BYTES {
        bail!("글머리표 그림은 1 MB 이하입니다.");
    }
    let format = image::guess_format(&bytes)
        .map_err(|_| anyhow!("그림 형식을 알 수 없습니다. PNG, JPG, BMP, GIF를 지원합니다."))?;
    let extension = match format {
        image::ImageFormat::Png => "png",
        image::ImageFormat::Jpeg => "jpg",
        image::ImageFormat::Bmp => "bmp",
        image::ImageFormat::Gif => "gif",
        other => bail!("{other:?} 그림은 글머리표로 쓸 수 없습니다. PNG, JPG, BMP, GIF를 쓰세요."),
    };
    let (w, h) =
        image::ImageReader::with_format(std::io::Cursor::new(&bytes), format).into_dimensions()?;
    if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE {
        bail!("글머리표 그림은 가로·세로 {MAX_SIDE}픽셀 이하입니다.");
    }
    Ok((bytes, extension))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(dir: &std::path::Path, name: &str, rgb: [u8; 3]) -> Result<PathBuf> {
        let path = dir.join(name);
        image::RgbImage::from_pixel(8, 8, image::Rgb(rgb)).save(&path)?;
        Ok(path)
    }

    #[test]
    fn embeds_once_applies_and_survives_both_formats() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let red = png(dir.path(), "red.png", [255, 0, 0])?;
        let mut s = Session::new()?;
        s.run("text.insert", json!({"text":"첫 항목\n둘째 항목"}))?;
        s.run("select.all", json!({}))?;
        let out = s.run("format.bullet.picture", json!({"path": red}))?;
        let id = out["id"].as_u64().ok_or_else(|| anyhow!("id"))? as usize;
        let images = s.core.document().doc_info.bin_data_list.len();
        // The same picture reuses its definition and image.
        let again = s.run("format.bullet.picture", json!({"path": red}))?;
        assert_eq!(again["id"], out["id"]);
        assert_eq!(s.core.document().doc_info.bin_data_list.len(), images);
        let bullet = s.core.document().doc_info.bullets[id - 1].clone();
        assert!(bullet.image_bullet > 0);
        for paragraph in &s.core.document().sections[0].paragraphs {
            let shape =
                &s.core.document().doc_info.para_shapes[usize::from(paragraph.para_shape_id)];
            assert_eq!(usize::from(shape.numbering_id), id);
        }
        let dir2 = tempfile::tempdir()?;
        for name in ["b.hwpx", "b.hwp"] {
            let path = dir2.path().join(name);
            s.save(&path)?;
            let mut reopened = Session::new()?;
            reopened.open(&path)?;
            let doc = reopened.core.document();
            let b = &doc.doc_info.bullets[id - 1];
            assert!(b.image_bullet > 0, "{name}");
            let image = doc
                .bin_data_content
                .iter()
                .find(|c| i32::from(c.id) == b.image_bullet)
                .ok_or_else(|| anyhow!("{name}: bullet image lost"))?;
            assert_eq!(
                image.data.load_shared().as_ref(),
                std::fs::read(&red)?.as_slice(),
                "{name}"
            );
        }
        // One undo removes the bullet from the paragraphs.
        s.run("edit.undo", json!({}))?;
        s.run("edit.undo", json!({}))?;
        let shape = &s.core.document().doc_info.para_shapes
            [usize::from(s.core.document().sections[0].paragraphs[0].para_shape_id)];
        assert_ne!(usize::from(shape.numbering_id), id);
        Ok(())
    }

    #[test]
    fn refuses_large_unknown_and_bad_level() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let big = dir.path().join("big.png");
        image::RgbImage::new(600, 10).save(&big)?;
        let text = dir.path().join("x.txt");
        std::fs::write(&text, "not an image")?;
        let ok = png(dir.path(), "ok.png", [0, 0, 255])?;
        let mut s = Session::new()?;
        s.run("text.insert", json!({"text":"항목"}))?;
        assert!(
            s.run("format.bullet.picture", json!({"path": big}))
                .is_err()
        );
        assert!(
            s.run("format.bullet.picture", json!({"path": text}))
                .is_err()
        );
        assert!(
            s.run("format.bullet.picture", json!({"path": ok, "level": 7}))
                .is_err()
        );
        assert!(
            s.core
                .document()
                .doc_info
                .bullets
                .iter()
                .all(|b| b.image_bullet == 0)
        );
        Ok(())
    }
}
