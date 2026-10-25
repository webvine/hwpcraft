//! CLI entry point for the current official MCP SDK, without legacy negotiation.
use crate::{document_workers::DocumentWorkers, sdk_http, sdk_service::HwpService, strict_stdio};
use anyhow::{Context, Result, bail};
use hwpcraft_formats::file_access::{self, AutomationRoots};
use rmcp::ServiceExt;
use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio_util::sync::CancellationToken;

/// Discovery-only clients and empty streams can end before the SDK switches
/// from its bootstrap loop to the request loop. EOF is normal in both phases.
pub(crate) async fn run_stream<R, W>(service: HwpService, read: R, write: W) -> Result<()>
where
    R: tokio::io::AsyncRead + Send + Unpin + 'static,
    W: tokio::io::AsyncWrite + Send + Unpin + 'static,
{
    let subscription_shutdown = CancellationToken::new();
    let service = service.with_subscription_shutdown(subscription_shutdown.clone());
    match service
        .serve(strict_stdio::new_with_subscription_shutdown(
            read,
            write,
            subscription_shutdown,
        ))
        .await
    {
        Ok(running) => {
            running.waiting().await?;
            Ok(())
        }
        Err(rmcp::service::ServerInitializeError::ConnectionClosed(_)) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Transport {
    Stdio,
    Http,
}
struct Options {
    transport: Transport,
    listen: SocketAddr,
    read: Option<PathBuf>,
    write: Option<PathBuf>,
    token_file: Option<PathBuf>,
}

/// Where the HTTP bearer token lives unless `--auth-token-file` says otherwise.
fn default_token_file() -> Result<PathBuf> {
    hwpcraft_engine::config_dir()
        .map(|dir| dir.join("mcp-http-token"))
        .context("no settings folder for the MCP HTTP token; pass --auth-token-file")
}

fn options(args: &[String]) -> Result<Options> {
    let mut read = None;
    let mut write = None;
    let mut transport = None;
    let mut listen = None;
    let mut token_file = None;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let (flag, inline) = arg
            .split_once('=')
            .map_or((arg.as_str(), None), |(key, value)| (key, Some(value)));
        let value = inline
            .or_else(|| args.next().map(String::as_str))
            .context("MCP option needs a value")?;
        if value.is_empty() || value.starts_with("--") {
            bail!("{flag} needs a value");
        }
        match flag {
            "--automation-read-root" => {
                if read.is_some() {
                    bail!("duplicate option: {flag}");
                }
                read = Some(PathBuf::from(value));
            }
            "--automation-write-root" => {
                if write.is_some() {
                    bail!("duplicate option: {flag}");
                }
                write = Some(PathBuf::from(value));
            }
            "--transport" => {
                if transport.is_some() {
                    bail!("duplicate option: {flag}");
                }
                transport = Some(match value {
                    "stdio" => Transport::Stdio,
                    "http" => Transport::Http,
                    _ => bail!("transport must be stdio or http"),
                });
            }
            "--listen" => {
                if listen.is_some() {
                    bail!("duplicate option: {flag}");
                }
                listen = Some(
                    value
                        .parse::<SocketAddr>()
                        .context("invalid listen address")?,
                );
            }
            "--auth-token-file" => {
                if token_file.is_some() {
                    bail!("duplicate option: {flag}");
                }
                token_file = Some(PathBuf::from(value));
            }
            _ => bail!("unsupported MCP option: {flag}"),
        }
    }
    let transport = transport.unwrap_or(Transport::Stdio);
    if transport == Transport::Stdio && listen.is_some() {
        bail!("--listen requires --transport http");
    }
    if transport == Transport::Stdio && token_file.is_some() {
        bail!("--auth-token-file requires --transport http");
    }
    let listen = listen.unwrap_or_else(|| SocketAddr::from(([127, 0, 0, 1], 7082)));
    if transport == Transport::Http && !listen.ip().is_loopback() {
        bail!("HTTP currently requires a loopback listen address; remote access is not supported");
    }
    Ok(Options {
        transport,
        listen,
        read,
        write,
        token_file,
    })
}

pub(crate) fn run(args: &[String]) -> Result<()> {
    if args == ["--help"] || args == ["-h"] {
        println!(
            "hwpcraft-cli mcp [--transport stdio|http] [--listen 127.0.0.1:7082] [--auth-token-file FILE] [--automation-read-root DIR] [--automation-write-root DIR]\nMCP 2026-07-28 only. Requests carry protocol metadata; document operations carry document_id. HTTP is loopback-only and every request needs `Authorization: Bearer <token>`; the token is read from FILE (default: mcp-http-token in the HwpCraft settings folder) and created there, readable only by you, if it does not exist."
        );
        return Ok(());
    }
    let options = options(args)?;
    // The token file is the operator's own setting, not automation input, so it
    // is read before the automation folders confine the process.
    let token = match options.transport {
        Transport::Http => {
            let path = match &options.token_file {
                Some(path) => path.clone(),
                None => default_token_file()?,
            };
            let token = sdk_http::load_or_create_token(&path)?;
            eprintln!(
                "MCP HTTP: http://{}/mcp needs `Authorization: Bearer <token>`; the token is in {}",
                options.listen,
                path.display()
            );
            Some(token)
        }
        Transport::Stdio => None,
    };
    let roots = AutomationRoots::new(
        options.read.as_deref().map(Path::as_ref),
        options.write.as_deref().map(Path::as_ref),
    )
    .map_err(anyhow::Error::msg)?;
    if let Some(roots) = &roots {
        file_access::confine_process(roots.clone());
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let service = HwpService::new(Arc::new(DocumentWorkers::new(roots)))?;
        match options.transport {
            Transport::Stdio => {
                let (read, write) = rmcp::transport::stdio();
                run_stream(service, read, write).await?;
            }
            Transport::Http => {
                let shutdown = CancellationToken::new();
                let signal = shutdown.clone();
                tokio::spawn(async move {
                    let _ = tokio::signal::ctrl_c().await;
                    signal.cancel();
                });
                let token = token.context("MCP HTTP token missing")?;
                sdk_http::serve(service, options.listen, token, shutdown).await?;
            }
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }
    #[test]
    fn current_transport_options_fail_before_any_stdin_is_consumed() -> Result<()> {
        assert_eq!(options(&[])?.transport, Transport::Stdio);
        assert_eq!(
            options(&args(&["--transport", "http", "--auth-token-file", "t"]))?.token_file,
            Some(PathBuf::from("t"))
        );
        assert_eq!(
            options(&args(&["--transport=http", "--listen", "127.0.0.1:0"]))?
                .listen
                .port(),
            0
        );
        for invalid in [
            args(&["--connect", "desktop"]),
            args(&["--transport", "legacy"]),
            args(&["--listen", "127.0.0.1:10"]),
            args(&["--transport", "http", "--listen", "0.0.0.0:7082"]),
            args(&["--transport", "http", "--transport", "stdio"]),
            args(&["--automation-read-root"]),
            args(&["--auth-token-file", "token"]),
            args(&[
                "--transport",
                "http",
                "--auth-token-file",
                "a",
                "--auth-token-file",
                "b",
            ]),
        ] {
            assert!(options(&invalid).is_err(), "{invalid:?}");
        }
        Ok(())
    }
}
