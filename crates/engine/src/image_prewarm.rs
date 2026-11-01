//! Converting a document's pictures ahead (RHWP patch 0059). The first time a page is
//! drawn, RHWP converts its BMP / TIFF / PCX pictures to PNG and fully decodes each
//! JPEG to see whether it is grey, on the thread that draws: about 90 ms a page with a
//! BMP in the 2025 handbook, which showed as a stutter while scrolling. The results go
//! into a process-wide memo keyed by the bytes, so converting them on another thread
//! right after opening makes the first draw a memo hit.
use crate::Session;

impl Session {
    /// Convert the open document's pictures on a background thread. Returns at once;
    /// nothing happens for a document without pictures.
    pub fn prewarm_pictures(&self) {
        let pictures: Vec<_> = self
            .core
            .document()
            .bin_data_content
            .iter()
            .map(|content| content.data.clone())
            .collect();
        if pictures.is_empty() {
            return;
        }
        let _ = std::thread::Builder::new()
            .name("hwpcraft-picture-prewarm".into())
            .spawn(move || {
                for picture in pictures {
                    hwpcraft_doc::prewarm_image_conversion(&picture.load_shared());
                }
            });
    }
}
