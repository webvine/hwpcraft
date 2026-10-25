//! HwpCraft MCP: stdio JSON-RPC over the shared RHWP editing session.
//! Architecture follows WordCraft; no DOCX model or desktop dependency is used.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod catalog;
mod server;
mod tools;

use anyhow::{Context, Result, bail};
use hwpcraft_engine::Session;
use hwpcraft_formats::file_access::{self, AutomationRoots};
use serde::Deserialize;
use serde_json::{Value, json};
pub use server::Server;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
pub use tools::tool_definitions;

pub struct Headless {
    session: Session,
    roots: Option<Arc<AutomationRoots>>,
    renderer: Option<hwpcraft_render::Renderer>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Open {
    path: PathBuf,
    #[serde(default)]
    force: bool,
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct New {
    #[serde(default)]
    force: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Save {
    path: Option<PathBuf>,
}

impl Headless {
    pub fn new(roots: Option<Arc<AutomationRoots>>) -> Result<Self> {
        Ok(Self {
            session: Session::new()?,
            roots,
            renderer: None,
        })
    }

    fn require_discard(&self, force: bool) -> Result<()> {
        if self.session.dirty() && !force {
            bail!(
                "저장하지 않은 변경이 있습니다. 먼저 save_document를 실행하거나 force: true로 명시적으로 버리세요."
            );
        }
        Ok(())
    }

    fn execute(&mut self, command: &str, params: Value) -> Result<Value> {
        if !params.is_object() {
            bail!("params는 JSON 객체여야 합니다.");
        }
        let roots = self.roots.clone();
        file_access::confine(roots.as_ref(), || {
            match command {
                "file.new" => {
                    let p: New = serde_json::from_value(params)?;
                    self.require_discard(p.force)?;
                    self.session.reset()?;
                }
                "file.open" => {
                    let p: Open = serde_json::from_value(params)?;
                    self.require_discard(p.force)?;
                    self.session.open(&p.path)?;
                }
                "file.save" | "file.save_as" | "file.pdf" => {
                    let p: Save = serde_json::from_value(params)?;
                    let path = p
                        .path
                        .or_else(|| {
                            (command == "file.save")
                                .then(|| self.session.path.clone())
                                .flatten()
                        })
                        .context("저장할 path가 필요합니다.")?;
                    if command == "file.pdf"
                        && !path
                            .extension()
                            .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
                    {
                        bail!("file.pdf의 path는 .pdf여야 합니다.");
                    }
                    self.session.save(&path)?;
                }
                _ => return self.session.run(command, params),
            }
            Ok(self.session.info())
        })
    }

    fn render(&mut self, page: u32, scale: f32) -> Result<Value> {
        use base64::Engine;
        use image::ImageEncoder;
        if page == 0 || page > self.session.core.page_count() {
            bail!("쪽 범위를 벗어났습니다.");
        }
        if !scale.is_finite() || !(0.25..=2.0).contains(&scale) {
            bail!("scale은 0.25–2.0이어야 합니다.");
        }
        let renderer = self.renderer.get_or_insert_with(Default::default);
        let image = renderer.page(&self.session.core, page - 1, scale)?;
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png).write_image(
            &image.rgba,
            image.width,
            image.height,
            image::ExtendedColorType::Rgba8,
        )?;
        Ok(json!({"content":[
            {"type":"image","mimeType":"image/png","data":base64::engine::general_purpose::STANDARD.encode(png)},
            {"type":"text","text":json!({"page":page,"pages":self.session.core.page_count(),"width":image.width,"height":image.height}).to_string()}
        ],"isError":false}))
    }
}

/// Parse only MCP options. Unknown/repeated flags fail before stdin is consumed.
pub fn run_stdio(args: &[String]) -> Result<()> {
    if args == ["--help"] || args == ["-h"] {
        println!(
            "hwpcraft-cli mcp [--automation-read-root DIR] [--automation-write-root DIR]\nHeadless MCP over stdio. HWP/HWPX editing; no --connect or desktop UI tools."
        );
        return Ok(());
    }
    let mut read = None;
    let mut write = None;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let (flag, inline) = arg
            .split_once('=')
            .map_or((arg.as_str(), None), |(k, v)| (k, Some(v)));
        let slot = match flag {
            "--automation-read-root" => &mut read,
            "--automation-write-root" => &mut write,
            _ => bail!("지원하지 않는 MCP 옵션: {flag}. hwpcraft-cli mcp --help"),
        };
        if slot.is_some() {
            bail!("중복 옵션: {flag}");
        }
        let value = inline
            .or_else(|| args.next().map(String::as_str))
            .context("옵션 뒤에 폴더가 필요합니다.")?;
        if value.is_empty() || value.starts_with("--") {
            bail!("{flag} 뒤에 폴더가 필요합니다.");
        }
        *slot = Some(PathBuf::from(value));
    }
    let roots = AutomationRoots::new(
        read.as_deref().map(Path::as_ref),
        write.as_deref().map(Path::as_ref),
    )
    .map_err(anyhow::Error::msg)?;
    // Covers any worker threads added by the engine in the future as well.
    if let Some(roots) = &roots {
        file_access::confine_process(roots.clone());
    }
    let mut server = Server::new(Headless::new(roots)?);
    match server.serve(std::io::stdin().lock(), std::io::stdout().lock()) {
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        result => result.map_err(Into::into),
    }
}
