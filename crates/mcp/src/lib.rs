//! Current MCP over isolated shared-engine document workers, using stdio or HTTP.
//! Each document operation carries an explicit handle; no desktop is required.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod atomic_batch;
mod catalog;
mod document_subscriptions;
mod document_workers;
mod output_schemas;
mod page_png;
mod progress;
mod protected_open;
mod sdk_catalog;
mod sdk_http;
mod sdk_runtime;
mod sdk_service;
mod staged_document;
mod strict_stdio;
mod tasks;
mod tools;

use anyhow::{Context, Result, bail};
use hwpcraft_engine::Session;
use hwpcraft_formats::file_access::{self, AutomationRoots};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc};
pub use tools::tool_definitions;

pub struct Headless {
    // False after a failed full-history restore; this backend must never be reused.
    pub(crate) usable: bool,
    pub(crate) session: Session,
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
            usable: true,
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
                    let prepared = staged_document::prepare_new(staged_document::next_generation(
                        self.session.generation,
                    ))?;
                    return Ok(prepared.commit(&mut self.session));
                }
                "file.open" => {
                    let p: Open = serde_json::from_value(params)?;
                    self.require_discard(p.force)?;
                    let prepared = staged_document::prepare_open(
                        p.path,
                        None,
                        staged_document::next_generation(self.session.generation),
                    )?;
                    return Ok(prepared.commit(&mut self.session));
                }
                "file.open_protected" => return protected_open::open(self, params),
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
        let count = self.session.page_count()?;
        if page == 0 || page > count {
            bail!("쪽 범위를 벗어났습니다.");
        }
        if !scale.is_finite() || !(0.25..=2.0).contains(&scale) {
            bail!("scale은 0.25–2.0이어야 합니다.");
        }
        let renderer = self.renderer.get_or_insert_with(Default::default);
        let image = renderer.svg(&self.session.render_page_svg(page - 1)?, scale)?;
        let png = page_png::encode(&image.rgba, image.width, image.height)?;
        Ok(json!({"content":[
            {"type":"image","mimeType":"image/png","data":base64::engine::general_purpose::STANDARD.encode(png)},
            {"type":"text","text":json!({"page":page,"pages":count,"width":image.width,"height":image.height}).to_string()}
        ],"isError":false}))
    }
}

/// Parse only MCP options. Unknown/repeated flags fail before stdin is consumed.
pub fn run_stdio(args: &[String]) -> Result<()> {
    sdk_runtime::run(args)
}

/// Serve current MCP over an owned JSONL stream. Every document request carries
/// its handle; roots are applied on each engine worker, never process-globally.
pub async fn serve_stream<R, W>(
    roots: Option<Arc<AutomationRoots>>,
    read: R,
    write: W,
) -> Result<()>
where
    R: tokio::io::AsyncRead + Send + Unpin + 'static,
    W: tokio::io::AsyncWrite + Send + Unpin + 'static,
{
    let service =
        sdk_service::HwpService::new(Arc::new(document_workers::DocumentWorkers::new(roots)))?;
    sdk_runtime::run_stream(service, read, write).await?;
    Ok(())
}
