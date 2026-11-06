//! Printing on Windows through GDI, without a PDF viewer in between.
//!
//! Windows 11's default PDF app (Edge) has no shell print verb, so handing the print PDF
//! to the shell only opened it. Here each sheet (a print SVG, already laid out with
//! 모아 찍기 and watermark) is rasterized at the printer's resolution (at most 300 dpi)
//! and stretched onto the paper of the named printer, in the sheet's orientation and,
//! when the sheet is a standard size (A3, A4, A5, B4, B5, Letter, Legal), on that paper.
//! A sheet larger than the paper the printer gives is scaled down to fit and centred,
//! as Hangul's 공급 용지에 맞추어 does. Copies are printed by sending the sheets again,
//! collated or not, so every driver gives the same result.
//!
//! A printer must be named: nothing here falls back to the default printer, so a test
//! or a caller that forgot the name never reaches a paper printer. `output` sends the
//! job to a file instead (virtual printers such as Microsoft Print to PDF write it
//! without asking for a file name).
//!
//! All GDI calls are in this module, each in its own `unsafe` block with the reason it
//! is sound; the workspace forbids `unsafe` everywhere else.
#![allow(unsafe_code)]
use crate::Renderer;
use anyhow::{Result, anyhow, bail};
use std::path::Path;
use windows_sys::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateDCW, DEVMODEW, DIB_RGB_COLORS, DM_ORIENTATION,
    DM_PAPERSIZE, DeleteDC, GetDeviceCaps, HALFTONE, HDC, LOGPIXELSX, LOGPIXELSY, PHYSICALHEIGHT,
    PHYSICALOFFSETX, PHYSICALOFFSETY, PHYSICALWIDTH, ResetDCW, SRCCOPY, SetBrushOrgEx,
    SetStretchBltMode, StretchDIBits,
};
use windows_sys::Win32::Graphics::Printing::{
    ClosePrinter, DocumentPropertiesW, OpenPrinterW, PRINTER_HANDLE,
};
use windows_sys::Win32::Storage::Xps::{AbortDoc, DOCINFOW, EndDoc, EndPage, StartDocW, StartPage};

/// `DocumentPropertiesW` modes: fill the output buffer, read the input one.
const DM_OUT_BUFFER: u32 = 2;
const DM_IN_BUFFER: u32 = 8;
const DMORIENT_PORTRAIT: i16 = 1;
const DMORIENT_LANDSCAPE: i16 = 2;
/// The resolution sheets are rasterized at, at most (A3 at 300 dpi is 17 M pixels).
const MAX_DPI: i32 = 300;
/// Standard papers by their portrait size in mm and DEVMODE paper code (DMPAPER_*).
const PAPERS: [(f32, f32, i16); 7] = [
    (210.0, 297.0, 9),  // A4
    (297.0, 420.0, 8),  // A3
    (148.0, 210.0, 11), // A5
    (257.0, 364.0, 12), // B4 (JIS)
    (182.0, 257.0, 13), // B5 (JIS)
    (215.9, 279.4, 1),  // Letter
    (215.9, 355.6, 5),  // Legal
];

/// What a print job did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Printed {
    /// Sheets sent, copies included.
    pub sheets: usize,
    /// Of those, how many were scaled down to fit the paper.
    pub fitted: usize,
}

/// One print job.
pub struct PrintRequest<'a> {
    /// The printer, by the name the OS lists it under. Required.
    pub printer: &'a str,
    /// The job name shown in the print queue.
    pub title: &'a str,
    /// Print to this file instead of the device (virtual printers).
    pub output: Option<&'a Path>,
    /// 1–99.
    pub copies: u32,
    /// With copies: whole set after whole set (true) or each sheet repeated.
    pub collate: bool,
}

/// Print `sheets` (print SVGs in CSS px at 96 dpi, one sheet each) as `request` says.
/// Fails without printing anything when the printer cannot be opened, and cancels the
/// job if a sheet fails on the way.
pub fn print_sheets(
    renderer: &Renderer,
    request: &PrintRequest,
    sheets: &[String],
) -> Result<Printed> {
    if request.printer.trim().is_empty() {
        bail!("프린터 이름이 없어 인쇄하지 않았습니다.");
    }
    if sheets.is_empty() {
        bail!("인쇄할 쪽이 없습니다.");
    }
    let copies = request.copies.clamp(1, 99) as usize;
    let order: Vec<usize> = if request.collate {
        (0..copies).flat_map(|_| 0..sheets.len()).collect()
    } else {
        (0..sheets.len())
            .flat_map(|s| std::iter::repeat_n(s, copies))
            .collect()
    };
    let (handle, mut devmode) = open_printer(request.printer)?;
    let printer = wide(request.printer);
    let driver = wide("WINSPOOL");
    let first = paper(&sheets[order[0]]);
    set_paper(&mut devmode, first);
    let merged = handle.merged(&printer, &devmode);
    // SAFETY: the strings are NUL-terminated UTF-16 buffers that outlive the call, and
    // `merged` is the driver's own DEVMODE (with its private bytes) for this printer.
    let dc = unsafe {
        CreateDCW(
            driver.as_ptr(),
            printer.as_ptr(),
            std::ptr::null(),
            merged.as_ptr().cast(),
        )
    };
    if dc.is_null() {
        bail!("프린터 [{}]에 연결하지 못했습니다.", request.printer);
    }
    let dc = Dc(dc);
    let title = wide(request.title);
    let output = request.output.map(|p| wide(&p.to_string_lossy()));
    let info = DOCINFOW {
        cbSize: std::mem::size_of::<DOCINFOW>() as i32,
        lpszDocName: title.as_ptr(),
        lpszOutput: output.as_ref().map_or(std::ptr::null(), |o| o.as_ptr()),
        lpszDatatype: std::ptr::null(),
        fwType: 0,
    };
    // SAFETY: `dc` is a live printer DC and `info` points at buffers that outlive the call.
    if unsafe { StartDocW(dc.0, &info) } <= 0 {
        bail!(
            "프린터 [{}]가 인쇄 작업을 받지 않았습니다.",
            request.printer
        );
    }
    let mut current = first;
    let mut fitted = 0;
    // The sheet drawn last: without collation each sheet comes `copies` times in a row.
    let mut last: Option<Raster> = None;
    let result = order.iter().try_for_each(|&index| {
        let sheet = &sheets[index];
        let wanted = paper(sheet);
        if wanted != current {
            set_paper(&mut devmode, wanted);
            let merged = handle.merged(&printer, &devmode);
            // SAFETY: between pages (after EndPage, before StartPage), with this
            // printer's DEVMODE; ResetDCW returns the same DC or null on failure.
            if unsafe { ResetDCW(dc.0, merged.as_ptr().cast()) }.is_null() {
                bail!("용지 방향·크기를 바꾸지 못했습니다.");
            }
            current = wanted;
        }
        fitted += usize::from(print_sheet(renderer, dc.0, sheet, index, &mut last)?);
        Ok(())
    });
    match result {
        Ok(()) => {
            // SAFETY: a document was started on this live DC.
            if unsafe { EndDoc(dc.0) } <= 0 {
                bail!("인쇄 작업을 마치지 못했습니다.");
            }
            Ok(Printed {
                sheets: order.len(),
                fitted,
            })
        }
        Err(error) => {
            // SAFETY: a document was started on this live DC; abandon it.
            unsafe { AbortDoc(dc.0) };
            Err(error)
        }
    }
}

/// A sheet rasterized for the printer, opaque BGRA, kept for the copies that follow.
struct Raster {
    sheet: usize,
    /// The resolution asked for (the raster may be coarser for a very large sheet).
    dpi: i32,
    width: u32,
    height: u32,
    bgra: Vec<u8>,
}

/// One page: the sheet at its size from the paper's top left (the printable area starts
/// at the physical offset, so the image starts that far up and left of it), or scaled
/// down and centred when it is larger than the paper; true when it was scaled. `last`
/// is reused when it is this sheet at this resolution, and replaced otherwise.
fn print_sheet(
    renderer: &Renderer,
    dc: HDC,
    sheet: &str,
    index: usize,
    last: &mut Option<Raster>,
) -> Result<bool> {
    let (width_px, height_px) =
        svg_size(sheet).ok_or_else(|| anyhow!("인쇄할 쪽의 크기를 읽지 못했습니다."))?;
    // SAFETY: `dc` is a live printer DC; GetDeviceCaps only reads.
    let caps = |index| unsafe { GetDeviceCaps(dc, index) };
    let (dpi_x, dpi_y) = (caps(LOGPIXELSX as i32), caps(LOGPIXELSY as i32));
    if dpi_x <= 0 || dpi_y <= 0 {
        bail!("프린터 해상도를 읽지 못했습니다.");
    }
    let (offset_x, offset_y) = (caps(PHYSICALOFFSETX as i32), caps(PHYSICALOFFSETY as i32));
    let wanted = dpi_x.max(dpi_y).min(MAX_DPI);
    if !last
        .as_ref()
        .is_some_and(|r| r.sheet == index && r.dpi == wanted)
    {
        // A very large sheet that does not fit at the printer's resolution is drawn
        // coarser.
        let mut dpi = wanted;
        let mut page = loop {
            match renderer.print_raster(sheet, dpi as f32 / 96.0) {
                Ok(page) => break page,
                Err(_) if dpi > 75 => dpi /= 2,
                Err(error) => return Err(error),
            }
        };
        bgra_on_white(&mut page.rgba);
        *last = Some(Raster {
            sheet: index,
            dpi: wanted,
            width: page.width,
            height: page.height,
            bgra: page.rgba,
        });
    }
    let raster = last
        .as_ref()
        .ok_or_else(|| anyhow!("쪽 그림을 만들지 못했습니다."))?;
    let width = raster.width as i32;
    let height = raster.height as i32;
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            // Top-down rows.
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut dest_w = f64::from(width_px) / 96.0 * f64::from(dpi_x);
    let mut dest_h = f64::from(height_px) / 96.0 * f64::from(dpi_y);
    let (paper_w, paper_h) = (
        f64::from(caps(PHYSICALWIDTH as i32)),
        f64::from(caps(PHYSICALHEIGHT as i32)),
    );
    let (mut left, mut top) = (0.0, 0.0);
    // 1 % slack: drivers round the paper to whole device units.
    let fit =
        paper_w > 0.0 && paper_h > 0.0 && (dest_w > paper_w * 1.01 || dest_h > paper_h * 1.01);
    if fit {
        let scale = (paper_w / dest_w).min(paper_h / dest_h);
        dest_w *= scale;
        dest_h *= scale;
        left = (paper_w - dest_w) / 2.0;
        top = (paper_h - dest_h) / 2.0;
    }
    let (dest_x, dest_y) = (
        (left - f64::from(offset_x)).round() as i32,
        (top - f64::from(offset_y)).round() as i32,
    );
    let (dest_w, dest_h) = (dest_w.round() as i32, dest_h.round() as i32);
    // SAFETY: a document is open on the live DC.
    if unsafe { StartPage(dc) } <= 0 {
        bail!("인쇄할 쪽을 시작하지 못했습니다.");
    }
    // SAFETY: `raster.bgra` holds `width * height` 32-bit pixels as `info` describes and
    // lives through the call; the DC is live inside StartPage/EndPage.
    let copied = unsafe {
        SetStretchBltMode(dc, HALFTONE);
        SetBrushOrgEx(dc, 0, 0, std::ptr::null_mut());
        StretchDIBits(
            dc,
            dest_x,
            dest_y,
            dest_w,
            dest_h,
            0,
            0,
            width,
            height,
            raster.bgra.as_ptr().cast(),
            &info,
            DIB_RGB_COLORS,
            SRCCOPY,
        )
    };
    // SAFETY: the page was started on this live DC.
    let ended = unsafe { EndPage(dc) };
    if copied == 0 || copied == -1 {
        bail!("쪽 그림을 프린터로 보내지 못했습니다.");
    }
    if ended <= 0 {
        bail!("인쇄할 쪽을 마치지 못했습니다.");
    }
    Ok(fit)
}

/// An open printer, closed on drop.
struct Printer(PRINTER_HANDLE);
impl Drop for Printer {
    fn drop(&mut self) {
        // SAFETY: the handle came from OpenPrinterW and is closed once.
        unsafe { ClosePrinter(self.0) };
    }
}

impl Printer {
    /// `devmode` after the driver has merged it with its own settings (it may adjust or
    /// refuse a paper or orientation); `devmode` itself when the driver will not.
    fn merged(&self, name: &[u16], devmode: &[u8]) -> Vec<u8> {
        let mut out = vec![0u8; devmode.len()];
        // SAFETY: `name` is NUL-terminated; both buffers have the size the driver gave
        // for its DEVMODE and outlive the call.
        let got = unsafe {
            DocumentPropertiesW(
                std::ptr::null_mut(),
                self.0,
                name.as_ptr(),
                out.as_mut_ptr().cast(),
                devmode.as_ptr().cast(),
                DM_IN_BUFFER | DM_OUT_BUFFER,
            )
        };
        if got < 0 { devmode.to_vec() } else { out }
    }
}

/// The printer, opened, and its current settings (DEVMODE with the driver's private
/// bytes).
fn open_printer(printer: &str) -> Result<(Printer, Vec<u8>)> {
    let name = wide(printer);
    let mut handle = PRINTER_HANDLE::default();
    // SAFETY: `name` is NUL-terminated and outlives the call; `handle` receives the
    // printer handle, closed by `Printer` on every path.
    if unsafe { OpenPrinterW(name.as_ptr(), &mut handle, std::ptr::null()) } == 0 {
        bail!("프린터 [{printer}]를 찾지 못했습니다.");
    }
    let handle = Printer(handle);
    // SAFETY: with no buffers the call only returns the size the DEVMODE needs.
    let size = unsafe {
        DocumentPropertiesW(
            std::ptr::null_mut(),
            handle.0,
            name.as_ptr(),
            std::ptr::null_mut(),
            std::ptr::null(),
            0,
        )
    };
    if size < std::mem::size_of::<DEVMODEW>() as i32 {
        bail!("프린터 [{printer}]의 설정을 읽지 못했습니다.");
    }
    let mut buffer = vec![0u8; size as usize];
    // SAFETY: `buffer` has the size the driver asked for.
    let got = unsafe {
        DocumentPropertiesW(
            std::ptr::null_mut(),
            handle.0,
            name.as_ptr(),
            buffer.as_mut_ptr().cast(),
            std::ptr::null(),
            DM_OUT_BUFFER,
        )
    };
    if got < 0 {
        bail!("프린터 [{printer}]의 설정을 읽지 못했습니다.");
    }
    Ok((handle, buffer))
}

/// The sheet's orientation (landscape when wider than tall) and standard paper, if any.
fn paper(svg: &str) -> (bool, Option<i16>) {
    let Some((w, h)) = svg_size(svg) else {
        return (false, None);
    };
    let mm = |px: f32| px / 96.0 * 25.4;
    let (short, long) = (mm(w.min(h)), mm(w.max(h)));
    let code = PAPERS
        .iter()
        .find(|(pw, ph, _)| (short - pw).abs() <= 2.0 && (long - ph).abs() <= 2.0)
        .map(|p| p.2);
    (w > h, code)
}

fn set_paper(devmode: &mut [u8], (landscape, code): (bool, Option<i16>)) {
    if devmode.len() < std::mem::size_of::<DEVMODEW>() {
        return;
    }
    // SAFETY: the buffer is at least one DEVMODEW long; it is read and written unaligned
    // because a Vec<u8> carries no alignment guarantee.
    unsafe {
        let pointer = devmode.as_mut_ptr().cast::<DEVMODEW>();
        let mut mode = pointer.read_unaligned();
        mode.dmFields |= DM_ORIENTATION;
        mode.Anonymous1.Anonymous1.dmOrientation = if landscape {
            DMORIENT_LANDSCAPE
        } else {
            DMORIENT_PORTRAIT
        };
        if let Some(code) = code {
            mode.dmFields |= DM_PAPERSIZE;
            mode.Anonymous1.Anonymous1.dmPaperSize = code;
        }
        pointer.write_unaligned(mode);
    }
}

/// Owns a printer DC.
struct Dc(HDC);
impl Drop for Dc {
    fn drop(&mut self) {
        // SAFETY: the DC came from CreateDCW and is deleted once.
        unsafe { DeleteDC(self.0) };
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// The root `<svg>` width and height (CSS px).
pub(crate) fn svg_size(svg: &str) -> Option<(f32, f32)> {
    let start = svg.find("<svg")?;
    let tag = &svg[start..start + svg[start..].find('>')?];
    let number = |name: &str| -> Option<f32> {
        let at = tag.find(&format!(" {name}=\""))? + name.len() + 3;
        let value = &tag[at..at + tag[at..].find('"')?];
        value.trim_end_matches("px").parse().ok()
    };
    let (w, h) = (number("width")?, number("height")?);
    (w > 0.0 && h > 0.0).then_some((w, h))
}

/// Premultiplied RGBA (the renderer paints on white, so it is opaque already) to the
/// opaque BGRA order GDI reads; anything still transparent is laid over white.
fn bgra_on_white(pixels: &mut [u8]) {
    for px in pixels.as_chunks_mut::<4>().0 {
        let white = 255 - px[3];
        let over = |c: u8| c.saturating_add(white);
        let (r, g, b) = (over(px[0]), over(px[1]), over(px[2]));
        px[0] = b;
        px[1] = g;
        px[2] = r;
        px[3] = 255;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sheet_size_and_orientation_come_from_the_root_element() {
        let portrait =
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="793.7" height="1122.5"><g/></svg>"#;
        let landscape =
            r#"<svg width="1122.5px" height="793.7px" xmlns="http://www.w3.org/2000/svg"></svg>"#;
        assert_eq!(svg_size(portrait), Some((793.7, 1122.5)));
        assert_eq!(paper(portrait), (false, Some(9)), "A4");
        assert_eq!(paper(landscape), (true, Some(9)), "A4 landscape");
        let a3 = r#"<svg width="1122.5" height="1587.4"></svg>"#;
        assert_eq!(paper(a3), (false, Some(8)));
        let odd = r#"<svg width="500" height="700"></svg>"#;
        assert_eq!(paper(odd), (false, None));
        assert_eq!(svg_size("<svg></svg>"), None);
    }

    #[test]
    fn pixels_become_opaque_bgra() {
        let mut px = [10, 20, 30, 255, 0, 0, 0, 0];
        bgra_on_white(&mut px);
        assert_eq!(px, [30, 20, 10, 255, 255, 255, 255, 255]);
    }

    /// Three sheets (A4 portrait, A4 landscape, one larger than any paper), two collated
    /// copies, through Microsoft Print to PDF into a file (no dialog, no paper); the large
    /// one is scaled to fit. Skipped where that printer is missing.
    #[test]
    fn sheets_print_to_a_file_through_microsoft_print_to_pdf() -> Result<()> {
        const PRINTER: &str = "Microsoft Print to PDF";
        if open_printer(PRINTER).is_err() {
            eprintln!("{PRINTER} is not installed; skipped");
            return Ok(());
        }
        let dir = tempfile::tempdir()?;
        let out = dir.path().join("인쇄 시험.pdf");
        let sheet = |w: u32, h: u32, text: &str| {
            format!(
                r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}"><rect x="40" y="40" width="200" height="100" fill="#204080"/><text x="40" y="200" font-size="40">{text}</text></svg>"##
            )
        };
        let sheets = [
            sheet(794, 1123, "세로 쪽"),
            sheet(1123, 794, "가로 쪽"),
            sheet(1400, 1800, "큰 쪽"),
        ];
        let request = PrintRequest {
            printer: PRINTER,
            title: "HwpCraft 인쇄 시험",
            output: Some(&out),
            copies: 2,
            collate: true,
        };
        let sent = print_sheets(&Renderer::default(), &request, &sheets)?;
        assert_eq!(
            sent,
            Printed {
                sheets: 6,
                fitted: 2
            }
        );
        // The spooler finishes the file after the job leaves this process.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let mut bytes = Vec::new();
        while std::time::Instant::now() < deadline {
            bytes = std::fs::read(&out).unwrap_or_default();
            if bytes.windows(5).any(|w| w == b"%%EOF") {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        assert!(bytes.starts_with(b"%PDF"), "no PDF written");
        let text = String::from_utf8_lossy(&bytes);
        // `/Type /Page` but not `/Type /Pages`.
        let pages = text
            .match_indices("/Type")
            .filter(|(at, _)| {
                let rest = text[at + 5..].trim_start();
                rest.starts_with("/Page") && !rest.starts_with("/Pages")
            })
            .count();
        assert_eq!(pages, 6, "pages");
        Ok(())
    }

    /// Copies not collated: each sheet three times in a row (its raster reused), six
    /// pages in sheet order. Skipped where Microsoft Print to PDF is missing.
    #[test]
    fn uncollated_copies_repeat_each_sheet_in_a_row() -> Result<()> {
        const PRINTER: &str = "Microsoft Print to PDF";
        if open_printer(PRINTER).is_err() {
            eprintln!("{PRINTER} is not installed; skipped");
            return Ok(());
        }
        let dir = tempfile::tempdir()?;
        let out = dir.path().join("copies.pdf");
        let sheet = |text: &str| {
            format!(
                r##"<svg xmlns="http://www.w3.org/2000/svg" width="794" height="1123"><text x="40" y="200" font-size="40">{text}</text></svg>"##
            )
        };
        let request = PrintRequest {
            printer: PRINTER,
            title: "HwpCraft 인쇄 시험",
            output: Some(&out),
            copies: 3,
            collate: false,
        };
        let sent = print_sheets(&Renderer::default(), &request, &[sheet("첫"), sheet("둘")])?;
        assert_eq!(
            sent,
            Printed {
                sheets: 6,
                fitted: 0
            }
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let mut bytes = Vec::new();
        while std::time::Instant::now() < deadline {
            bytes = std::fs::read(&out).unwrap_or_default();
            if bytes.windows(5).any(|w| w == b"%%EOF") {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        assert!(bytes.starts_with(b"%PDF"), "no PDF written");
        Ok(())
    }

    #[test]
    fn a_print_without_a_printer_name_is_refused() {
        let renderer = Renderer::default();
        let request = PrintRequest {
            printer: " ",
            title: "시험",
            output: None,
            copies: 1,
            collate: true,
        };
        let sheet = r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100"/>"#;
        assert!(print_sheets(&renderer, &request, &[sheet.to_owned()]).is_err());
    }
}
