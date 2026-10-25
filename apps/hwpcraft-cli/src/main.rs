use anyhow::{Context, Result, bail};
use hwpcraft_engine::{Command, Cursor, Session};
use serde_json::json;
use std::path::Path;

fn argument(args: &[String], n: usize) -> Result<&str> {
    args.get(n)
        .map(String::as_str)
        .context("인수가 부족합니다. hwpcraft-cli --help")
}
fn main() -> Result<()> {
    let mut args: Vec<_> = std::env::args().skip(1).collect();
    // `--password <암호>` anywhere: opens an encrypted input and protects the output.
    let mut password = None;
    if let Some(i) = args.iter().position(|a| a == "--password") {
        if i + 1 >= args.len() {
            bail!("--password 뒤에 암호를 입력하세요.");
        }
        password = Some(args.remove(i + 1));
        args.remove(i);
    }
    let command = args.first().map(String::as_str).unwrap_or("--help");
    if command == "mcp" {
        return hwpcraft_mcp::run_stdio(&args[1..]);
    }
    if command == "--version" {
        println!("HwpCraft {} / RHWP 0.8.7", hwpcraft_engine::VERSION);
        return Ok(());
    }
    if command == "--help" {
        println!("  mcp [--automation-read-root DIR] [--automation-write-root DIR] (stdio server)");
        println!(
            "HwpCraft CLI (옵션: --password <암호>  암호 문서 열기·암호 저장)\n  info <hwp|hwpx>\n  text <hwp|hwpx>\n  convert <input> <output.hwp|hwpx|pdf|txt>\n  render <input> <output.png> [page: 1-based]\n  new <output.hwp|hwpx> [text]\n  edit <input.hwp|hwpx|--new> <commands.json> <output.hwp|hwpx>\n  demo <output-directory>\n  commands\n  --version"
        );
        return Ok(());
    }
    if command == "commands" {
        println!(
            "{}",
            serde_json::to_string_pretty(
                &hwpcraft_engine::COMMANDS
                    .iter()
                    .chain(hwpcraft_engine::extension_commands())
                    .collect::<Vec<_>>()
            )?
        );
        return Ok(());
    }
    let mut session = Session::new()?;
    let open = |session: &mut Session, path: &Path| -> Result<()> {
        match &password {
            Some(p) => session.open_with_password(path, p),
            None => session.open(path),
        }
    };
    if let Some(p) = &password {
        session.set_password(Some(p.clone()))?;
    }
    match command {
        "edit" => {
            use std::io::Read;
            #[derive(serde::Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Edit {
                command: String,
                #[serde(default = "empty_params")]
                params: serde_json::Value,
            }
            fn empty_params() -> serde_json::Value {
                json!({})
            }
            let input = argument(&args, 1)?;
            let plan = argument(&args, 2)?;
            let output = Path::new(argument(&args, 3)?);
            let extension = output
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if !matches!(extension.as_str(), "hwp" | "hwpx") {
                bail!("편집 결과는 HWP / HWPX로 저장하세요.");
            }
            let mut bytes = Vec::new();
            std::fs::File::open(plan)?
                .take(1_000_001)
                .read_to_end(&mut bytes)?;
            if bytes.len() > 1_000_000 {
                bail!("명령 파일은 1 MB 이하입니다.");
            }
            let edits: Vec<Edit> = serde_json::from_slice(&bytes)?;
            if edits.len() > 1000 {
                bail!("명령 파일은 1,000개 명령 이하입니다.");
            }
            if input != "--new" {
                open(&mut session, Path::new(input))?;
            }
            let mut results = Vec::new();
            for (i, edit) in edits.into_iter().enumerate() {
                results.push(session.run(&edit.command, edit.params).with_context(|| {
                    format!(
                        "명령 {} ({}) 실패; 출력 파일은 저장하지 않았습니다.",
                        i + 1,
                        edit.command
                    )
                })?);
            }
            session.save(output)?;
            println!("{}", json!({"results":results,"document":session.info()}));
        }
        "new" => {
            let path = Path::new(argument(&args, 1)?);
            if let Some(t) = args.get(2) {
                session.execute(Command::Insert(t.clone()))?;
            }
            session.save(path)?;
        }
        "demo" => demo(Path::new(argument(&args, 1)?))?,
        "info" | "text" | "convert" | "render" => {
            open(&mut session, Path::new(argument(&args, 1)?))?;
            match command {
                "info" => println!("{}", serde_json::to_string_pretty(&session.info())?),
                "text" => {
                    for (c, _) in session.entries() {
                        println!("{}", session.paragraph(c)?.text);
                    }
                }
                "convert" => session.save(Path::new(argument(&args, 2)?))?,
                "render" => {
                    let page = args
                        .get(3)
                        .map(|v| v.parse::<u32>())
                        .transpose()?
                        .unwrap_or(1);
                    if page == 0 || page > session.core.page_count() {
                        bail!("쪽 범위를 벗어났습니다.");
                    }
                    let p =
                        hwpcraft_render::Renderer::default().page(&session.core, page - 1, 1.25)?;
                    image::save_buffer(
                        argument(&args, 2)?,
                        &p.rgba,
                        p.width,
                        p.height,
                        image::ColorType::Rgba8,
                    )?;
                }
                _ => {}
            }
        }
        _ => bail!("알 수 없는 명령: {command}"),
    }
    Ok(())
}
fn demo(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    let mut s = Session::new()?;
    s.execute(Command::Insert("HwpCraft · 한글 문서 작업실".into()))?;
    s.execute(Command::Character(
        json!({"fontFamily":"Noto Sans KR","bold":true,"fontSize":2400,"textColor":"#234c7d"}),
    ))?;
    s.execute(Command::Paragraph(
        json!({"alignment":"center","spacingAfter":1600}),
    ))?;
    s.execute(Command::Split)?;
    s.execute(Command::Insert(
        "문서를 내 컴퓨터에서 만들고, 편집하고, 보관합니다.".into(),
    ))?;
    s.execute(Command::Character(
        json!({"fontFamily":"Noto Sans KR","bold":false,"fontSize":1200,"textColor":"#263445"}),
    ))?;
    s.execute(Command::Paragraph(
        json!({"alignment":"left","spacingAfter":1000}),
    ))?;
    s.execute(Command::Split)?;
    s.execute(Command::Insert("HWP와 HWPX 열기 · 한글 입력 · 글자 및 문단 서식 · 표 편집\n실행 취소 / 다시 실행 · 찾기 / 바꾸기 · PDF 내보내기\n보기에서 문서 탐색을 켜거나 표 셀을 클릭해 편집하세요.\n".into()))?;
    s.execute(Command::Table {
        rows: 3,
        columns: 3,
    })?;
    let cells: Vec<_> = s
        .entries()
        .into_iter()
        .filter(|(c, _)| c.cell.is_some())
        .map(|(c, _)| c)
        .collect();
    for (c, text) in cells.into_iter().zip([
        "항목",
        "내용",
        "상태",
        "문서 엔진",
        "RHWP / Rust",
        "로컬",
        "파일 형식",
        "HWP · HWPX",
        "지원",
    ]) {
        s.select(c, false)?;
        s.execute(Command::Insert(text.into()))?;
        s.execute(Command::Character(json!({"fontFamily":"Noto Sans KR","fontSize":1000,"bold":false})))?;
    }
    s.select(Cursor::default(), false)?;
    for extension in ["hwpx", "hwp", "pdf"] {
        s.save(&dir.join(format!("HwpCraft-시작하기.{extension}")))?;
    }
    let p = hwpcraft_render::Renderer::default().page(&s.core, 0, 1.25)?;
    image::save_buffer(
        dir.join("sample-page.png"),
        &p.rgba,
        p.width,
        p.height,
        image::ColorType::Rgba8,
    )?;
    std::fs::write(dir.join("sample-page.svg"), p.svg)?;
    println!("{}", s.info());
    Ok(())
}
