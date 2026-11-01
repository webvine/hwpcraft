use anyhow::{Context, Result, anyhow, bail};
use hwpcraft_engine::{Command, Cursor, Session};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const HELP: &str = "HwpCraft CLI — 한글 문서(HWP·HWPX·HWP3·HWPML)를 데스크톱 앱과 같은 엔진으로 다룹니다.

사용법:
  hwpcraft-cli info <입력>                         문서 정보(JSON)
  hwpcraft-cli text <입력>                         본문·표 셀 글(문단마다 한 줄)
  hwpcraft-cli convert <입력> <출력>               형식 변환(출력 확장자: hwp hwpx pdf txt docx)
  hwpcraft-cli render <입력> <출력.png> [쪽|all] [--scale S]
                                                   쪽 그림(PNG, 쪽은 1부터, 기본 1, 배율 0.25–2 기본 1.25;
                                                   all이면 <출력>-1.png, <출력>-2.png …)
  hwpcraft-cli new <출력> [글]                     새 문서(출력 확장자: hwp hwpx pdf txt docx)
  hwpcraft-cli run <입력|--new> <명령> [params JSON] [-o 출력]
                                                   편집 명령 하나 또는 한글 동작 이름 하나를 실행하고 결과(JSON)를
                                                   출력합니다. -o를 주면 성공한 뒤 저장합니다(주지 않으면 저장 안 함).
                                                   file.save·file.save_as·file.pdf(한글 동작 FileSave 등)는 run·edit
                                                   단계에서도 그 path로 저장합니다. 열기·새 문서는 입력 인수(경로, --new)로.
  hwpcraft-cli edit <입력|--new> <명령 파일|-> <출력.hwp|hwpx> [--keep-going]
                                                   명령 파일(JSON 배열, -는 표준 입력)을 차례로 실행하고 저장합니다.
                                                   단계: {\"command\":\"text.insert\",\"params\":{…}} 또는
                                                   {\"hwpAction\":\"MoveDocumentEnd\",\"params\":{…}}. 기본은 하나라도
                                                   실패하면 저장하지 않고, --keep-going이면 나머지를 실행해 저장한 뒤
                                                   실패가 있으면 종료 코드 1입니다.
  hwpcraft-cli commands [검색어] [--json]          편집 명령 목록([id, 이름, 단축키]); --json은 명령마다
                                                   paramsHelp·paramsExample·hwpActions(MCP list_commands와 같음)
  hwpcraft-cli actions [검색어]                    한글 동작 이름과 대응 명령·기본 params(MCP hwp_action과 같음)
  hwpcraft-cli demo <출력 폴더>                    견본 문서 만들기
  hwpcraft-cli mcp [--transport stdio|http] [--listen 127.0.0.1:7082] [--auth-token-file FILE]
                   [--automation-read-root DIR] [--automation-write-root DIR]
                                                   MCP 서버(기본 stdio). 자세히: hwpcraft-cli mcp --help
  hwpcraft-cli help | -h | --help                  이 도움말
  hwpcraft-cli --version | -V                      판 정보

옵션(어느 자리에나):
  --password <암호>        암호 문서를 열고, 저장할 때 같은 암호를 겁니다.
  --new-password <암호>    저장할 때 이 암호를 겁니다(열 때는 --password).
  --remove-password        저장할 때 암호를 풉니다.
  --accept-track-changes   변경 추적 문서를 모든 변경을 적용한 결과본으로 저장합니다(DOCX 등에 필요).
  --drop-combo-items       .hwp에 담을 수 없는 콤보 상자 목록을 빼고 저장합니다.
  --drop-picture-effects   이전 판 호환(이제 .hwp도 그림 효과를 저장하므로 빼지 않음).

종료 코드: 0 성공, 1 실행 실패(문서·명령·저장), 2 사용법 오류. 결과는 표준 출력, 경고와 오류는 표준 오류에 씁니다.
위치(offset)는 유니코드 스칼라 값 단위입니다. 실패한 열기·명령·저장은 기존 출력 파일을 바꾸지 않습니다.";

/// A mistake in the command line itself (exit code 2), not a failed document operation.
#[derive(Debug)]
struct Usage(String);

impl std::fmt::Display for Usage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (hwpcraft-cli --help)", self.0)
    }
}

impl std::error::Error for Usage {}

fn usage(message: impl Into<String>) -> anyhow::Error {
    Usage(message.into()).into()
}

fn argument(args: &[String], n: usize) -> Result<&str> {
    args.get(n)
        .map(String::as_str)
        .ok_or_else(|| usage("인수가 부족합니다."))
}

/// Remove `flag` and the value after it, wherever it is.
fn take_value(args: &mut Vec<String>, flag: &str) -> Result<Option<String>> {
    let Some(i) = args.iter().position(|a| a == flag) else {
        return Ok(None);
    };
    if i + 1 >= args.len() {
        return Err(usage(format!("{flag} 뒤에 값을 입력하세요.")));
    }
    let value = args.remove(i + 1);
    args.remove(i);
    Ok(Some(value))
}

/// Remove a switch wherever it is; true when it was there.
fn take_switch(args: &mut Vec<String>, flag: &str) -> bool {
    args.iter()
        .position(|a| a == flag)
        .map(|i| args.remove(i))
        .is_some()
}

fn main() -> ExitCode {
    match run_cli() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("오류: {e:#}");
            if e.downcast_ref::<Usage>().is_some() {
                ExitCode::from(2)
            } else {
                ExitCode::FAILURE
            }
        }
    }
}

/// Every engine command with its help, example and Hangul action names, as MCP
/// `list_commands` lists them, filtered by `query` (id or label, any case).
fn command_catalog(query: &str) -> Vec<Value> {
    let mut help = std::collections::HashMap::new();
    for (id, text, example) in hwpcraft_engine::command_help_entries() {
        help.entry(id).or_insert((text, example));
    }
    let query = query.to_lowercase();
    let mut seen = std::collections::BTreeSet::new();
    hwpcraft_engine::COMMANDS
        .iter()
        .chain(hwpcraft_engine::extension_commands())
        .filter(|(id, _, _)| seen.insert(*id))
        .filter(|(id, label, _)| {
            query.is_empty()
                || id.to_lowercase().contains(&query)
                || label.to_lowercase().contains(&query)
        })
        .map(|(id, label, shortcut)| {
            let (text, example) = help.remove(id).unwrap_or(("", Value::Null));
            let actions: Vec<&str> = hwpcraft_engine::HWP_ACTIONS
                .iter()
                .filter(|(_, command, _, _)| command == id)
                .map(|(action, _, _, _)| *action)
                .collect();
            json!({"id":id,"label":label,"shortcut":shortcut,"paramsHelp":text,
                "paramsExample":example,"hwpActions":actions})
        })
        .collect()
}

/// Hangul action names with their command and default parameters, as MCP
/// `hwp_action {query}` lists them.
fn action_catalog(query: &str) -> Vec<Value> {
    let query = query.to_lowercase();
    hwpcraft_engine::HWP_ACTIONS
        .iter()
        .filter(|(id, command, _, _)| {
            query.is_empty() || id.to_lowercase().contains(&query) || command.contains(&query)
        })
        .map(|(id, command, params, mapping)| {
            json!({"id":id,"command":command,
                "defaultParams":serde_json::from_str::<Value>(params).unwrap_or(json!({})),
                "mapping":mapping,"parameterSet":hwpcraft_engine::hwp_parameter_schema(id)})
        })
        .collect()
}

fn is_command(id: &str) -> bool {
    hwpcraft_engine::COMMANDS
        .iter()
        .chain(hwpcraft_engine::extension_commands())
        .any(|(c, _, _)| *c == id)
}

/// What MCP's own file commands mean in the CLI: the three saves run through the
/// CLI's save (atomic, the same format and option rules as `convert`); opening and
/// new documents are the command line's input argument instead.
fn cli_file_command(
    session: &mut Session,
    command: &str,
    params: &Value,
    save: &dyn Fn(&mut Session, &Path) -> Result<()>,
) -> Option<Result<Value>> {
    match command {
        "file.save" | "file.save_as" | "file.pdf" => Some((|| {
            let path = match params.get("path").and_then(Value::as_str) {
                Some(p) => PathBuf::from(p),
                None if command == "file.save" => session
                    .path
                    .clone()
                    .context("새 문서라 저장할 path가 필요합니다.")?,
                None => bail!("{command}에는 path가 필요합니다."),
            };
            if command == "file.pdf"
                && !path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
            {
                bail!("file.pdf의 path는 .pdf여야 합니다.");
            }
            save(session, &path)?;
            Ok(json!({"saved": path, "document": session.info()}))
        })()),
        "file.new" | "file.open" | "file.open_protected" => Some(Err(anyhow!(
            "{command}는 CLI에서 명령 인수로 합니다: 새 문서는 --new, 열기는 문서 경로(암호 문서는 --password)."
        ))),
        _ => None,
    }
}

/// Run an engine command, or a Hangul action name through its mapping (as MCP
/// `hwp_action` does, with the same result shape).
fn run_step(
    session: &mut Session,
    name: &str,
    params: Value,
    parameter_set: Option<Value>,
    action: bool,
    save: &dyn Fn(&mut Session, &Path) -> Result<()>,
) -> Result<Value> {
    if !action && (is_command(name) || hwpcraft_engine::hwp_action(name).is_none()) {
        if parameter_set.is_some() {
            bail!("parameterSet은 한글 동작 이름(hwpAction)에만 씁니다.");
        }
        if let Some(done) = cli_file_command(session, name, &params, save) {
            return done;
        }
        return session.run(name, params);
    }
    let prepared = session.prepare_hwp_action(name, params, parameter_set)?;
    let result = match cli_file_command(session, prepared.command, &prepared.params, save) {
        Some(done) => done?,
        None => session
            .run(prepared.command, prepared.params)
            .map_err(|e| hwpcraft_engine::explain_hwp_action_error(name, prepared.command, e))?,
    };
    Ok(json!({"action":name,"command":prepared.command,"mapping":prepared.mapping,"result":result}))
}

fn read_plan(plan: &str) -> Result<Vec<u8>> {
    use std::io::Read;
    let mut bytes = Vec::new();
    if plan == "-" {
        std::io::stdin()
            .take(1_000_001)
            .read_to_end(&mut bytes)
            .context("표준 입력에서 명령을 읽지 못했습니다")?;
    } else {
        std::fs::File::open(plan)
            .with_context(|| format!("명령 파일을 열지 못했습니다: {plan}"))?
            .take(1_000_001)
            .read_to_end(&mut bytes)?;
    }
    if bytes.len() > 1_000_000 {
        bail!("명령 파일은 1 MB 이하입니다.");
    }
    Ok(bytes)
}

fn parse_page(value: &str, count: u32) -> Result<u32> {
    let page: u32 = value
        .parse()
        .map_err(|_| usage(format!("쪽은 1부터 세는 정수 또는 all입니다: {value}")))?;
    if page == 0 || page > count {
        bail!("쪽 범위를 벗어났습니다: {page} (문서는 {count}쪽)");
    }
    Ok(page)
}

/// `out.png` → `out-<page>.png` for `render … all`.
fn numbered(path: &Path, page: u32) -> PathBuf {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("page");
    path.with_file_name(format!("{stem}-{page}.png"))
}

/// The RHWP open error for a password-protected file names its own API; say what to do.
fn readable_open_error(e: anyhow::Error) -> anyhow::Error {
    if hwpcraft_formats::needs_password(&e) {
        anyhow!("비밀번호가 필요한 암호 문서입니다. --password <암호>로 여세요.")
    } else {
        e
    }
}

/// Encode an RGBA page and replace `path` only once the whole PNG is written.
fn write_png(path: &Path, rgba: &[u8], width: u32, height: u32) -> Result<()> {
    use image::ImageEncoder;
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png).write_image(
        rgba,
        width,
        height,
        image::ExtendedColorType::Rgba8,
    )?;
    hwpcraft_formats::atomic_write(path, &png)
        .with_context(|| format!("PNG를 쓰지 못했습니다: {}", path.display()))
}

fn run_cli() -> Result<ExitCode> {
    let mut args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("mcp") {
        return hwpcraft_mcp::run_stdio(&args[1..]).map(|()| ExitCode::SUCCESS);
    }
    // Options may stand anywhere.
    let password = take_value(&mut args, "--password")?;
    let new_password = take_value(&mut args, "--new-password")?;
    let remove_password = take_switch(&mut args, "--remove-password");
    if new_password.is_some() && remove_password {
        return Err(usage(
            "--new-password와 --remove-password는 함께 쓸 수 없습니다.",
        ));
    }
    // Save a change-tracked document as HWP/HWPX, merging the changes into the shown
    // text (refused otherwise).
    let accept_track_changes = take_switch(&mut args, "--accept-track-changes");
    // Accepted from older scripts; .hwp now keeps picture effects, so nothing is dropped.
    let drop_picture_effects = take_switch(&mut args, "--drop-picture-effects");
    // Save a .hwp without the combo box items HWP cannot hold (refused otherwise).
    let drop_combo_items = take_switch(&mut args, "--drop-combo-items");
    let keep_going = take_switch(&mut args, "--keep-going");
    let catalog_json = take_switch(&mut args, "--json");
    let scale = take_value(&mut args, "--scale")?;
    let output = match take_value(&mut args, "-o")? {
        Some(o) => Some(o),
        None => take_value(&mut args, "--output")?,
    };
    if let Some(unknown) = args
        .iter()
        .find(|a| a.starts_with("--") && *a != "--new" && *a != "--help" && *a != "--version")
    {
        return Err(usage(format!("알 수 없는 옵션: {unknown}")));
    }
    let command = args.first().map(String::as_str).unwrap_or("--help");
    match command {
        "--help" | "-h" | "help" => {
            println!("{HELP}");
            return Ok(ExitCode::SUCCESS);
        }
        "--version" | "-V" => {
            println!("HwpCraft {} / RHWP 0.8.7", hwpcraft_engine::VERSION.trim());
            return Ok(ExitCode::SUCCESS);
        }
        "commands" => {
            let query = args.get(1).map(String::as_str).unwrap_or("");
            let list: Vec<Value> = if catalog_json {
                command_catalog(query)
            } else {
                command_catalog(query)
                    .into_iter()
                    .map(|c| json!([c["id"], c["label"], c["shortcut"]]))
                    .collect()
            };
            println!("{}", serde_json::to_string_pretty(&list)?);
            return Ok(ExitCode::SUCCESS);
        }
        "actions" => {
            let query = args.get(1).map(String::as_str).unwrap_or("");
            println!("{}", serde_json::to_string_pretty(&action_catalog(query))?);
            return Ok(ExitCode::SUCCESS);
        }
        "info" | "text" | "convert" | "render" | "new" | "edit" | "run" | "demo" => {}
        "mcp" => return Err(usage("mcp는 첫 인수로 쓰고, 그 뒤에 mcp 옵션을 적으세요.")),
        _ => return Err(usage(format!("알 수 없는 명령: {command}"))),
    }
    let mut session = Session::new()?;
    let open = |session: &mut Session, path: &Path| -> Result<()> {
        match &password {
            Some(p) => session.open_with_password(path, p),
            None => session.open(path),
        }
        .map_err(readable_open_error)
        .with_context(|| format!("문서를 열지 못했습니다: {}", path.display()))
    };
    let output_password = if remove_password {
        None
    } else {
        new_password.clone().or_else(|| password.clone())
    };
    let set_output_password = |session: &mut Session| -> Result<()> {
        if output_password.is_some() || remove_password {
            session.set_password(output_password.clone())?;
        }
        Ok(())
    };
    let save = |session: &mut Session, path: &Path| -> Result<()> {
        if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("docx"))
        {
            let omitted = session.export_docx(path, accept_track_changes)?;
            if !omitted.is_empty() {
                eprintln!("DOCX에 담지 못한 것: {}", omitted.join(", "));
            }
            return Ok(());
        }
        if drop_combo_items {
            let dropped = session.save_dropping_combo_items(path, accept_track_changes)?;
            if dropped > 0 {
                eprintln!("콤보 상자 {dropped}개의 목록을 빼고 저장했습니다.");
            }
            Ok(())
        } else if drop_picture_effects {
            let dropped = session.save_dropping_picture_effects(path, accept_track_changes)?;
            if dropped > 0 {
                eprintln!("그림 효과 {dropped}개를 빼고 저장했습니다.");
            }
            Ok(())
        } else if accept_track_changes {
            session.save_accepting_track_changes(path)
        } else {
            session.save(path)
        }
    };
    let input = |session: &mut Session, name: &str| -> Result<()> {
        if name != "--new" {
            open(session, Path::new(name))?;
        }
        set_output_password(session)
    };
    match command {
        "run" => {
            let name = argument(&args, 2)?.to_string();
            let params: Value = match args.get(3) {
                Some(text) => serde_json::from_str(text)
                    .map_err(|e| usage(format!("params JSON을 읽지 못했습니다: {e}")))?,
                None => json!({}),
            };
            if !params.is_object() {
                return Err(usage("params는 JSON 객체여야 합니다."));
            }
            if args.len() > 4 {
                return Err(usage(format!("남는 인수: {}", args[4..].join(" "))));
            }
            input(&mut session, argument(&args, 1)?)?;
            let result = run_step(&mut session, &name, params, None, false, &save)?;
            if let Some(path) = &output {
                save(&mut session, Path::new(path))?;
            }
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &json!({"result":result,"saved":output,"document":session.info()})
                )?
            );
        }
        "edit" => {
            #[derive(serde::Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Edit {
                #[serde(default)]
                command: Option<String>,
                #[serde(default, rename = "hwpAction")]
                hwp_action: Option<String>,
                #[serde(default = "empty_params")]
                params: Value,
                #[serde(default, rename = "parameterSet")]
                parameter_set: Option<Value>,
            }
            fn empty_params() -> Value {
                json!({})
            }
            let plan = argument(&args, 2)?;
            let output = Path::new(argument(&args, 3)?);
            let extension = output
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if !matches!(extension.as_str(), "hwp" | "hwpx") {
                return Err(usage(
                    "편집 결과는 HWP / HWPX로 저장하세요(PDF 등은 convert로).",
                ));
            }
            let bytes = read_plan(plan)?;
            let edits: Vec<Edit> = serde_json::from_slice(&bytes).with_context(|| {
                format!("명령 파일은 [{{\"command\":…,\"params\":{{…}}}}, …] 형태의 JSON 배열이어야 합니다: {plan}")
            })?;
            if edits.len() > 1000 {
                bail!("명령 파일은 1,000개 명령 이하입니다.");
            }
            input(&mut session, argument(&args, 1)?)?;
            let mut results = Vec::new();
            let mut failed = 0;
            for (i, edit) in edits.into_iter().enumerate() {
                let (name, action) = match (edit.command, edit.hwp_action) {
                    (Some(c), None) => (c, false),
                    (None, Some(a)) => (a, true),
                    _ => bail!("명령 {}: command와 hwpAction 중 하나만 쓰세요.", i + 1),
                };
                match run_step(
                    &mut session,
                    &name,
                    edit.params,
                    edit.parameter_set,
                    action,
                    &save,
                ) {
                    Ok(value) => results.push(value),
                    Err(e) if keep_going => {
                        failed += 1;
                        eprintln!("명령 {} ({name}) 실패: {e:#}", i + 1);
                        results.push(json!({"command":name,"ok":false,"error":format!("{e:#}")}));
                    }
                    Err(e) => {
                        return Err(e.context(format!(
                            "명령 {} ({name})이 실패해 출력 파일은 저장하지 않았습니다",
                            i + 1
                        )));
                    }
                }
            }
            save(&mut session, output)?;
            println!(
                "{}",
                json!({"results":results,"failed":failed,"document":session.info()})
            );
            if failed > 0 {
                return Ok(ExitCode::FAILURE);
            }
        }
        "new" => {
            let path = Path::new(argument(&args, 1)?);
            set_output_password(&mut session)?;
            if let Some(t) = args.get(2) {
                session.execute(Command::Insert(t.clone()))?;
            }
            save(&mut session, path)?;
        }
        "demo" => demo(Path::new(argument(&args, 1)?))?,
        "info" | "text" | "convert" | "render" => {
            input(&mut session, argument(&args, 1)?)?;
            match command {
                "info" => {
                    let mut info = session.info();
                    let path = Path::new(&args[1]);
                    info["format"] = json!(
                        path.extension()
                            .and_then(|e| e.to_str())
                            .map(str::to_ascii_lowercase)
                    );
                    info["passwordProtected"] = json!(password.is_some());
                    info["distribution"] = json!(session.is_distribution());
                    if let Ok(track) = session.run("document.track_changes", json!({})) {
                        info["trackChanges"] = track["tracked"].clone();
                    }
                    println!("{}", serde_json::to_string_pretty(&info)?);
                }
                "text" => {
                    if session.is_distribution() {
                        bail!("{}", hwpcraft_engine::DISTRIBUTION_READ_ONLY);
                    }
                    for (c, _) in session.entries() {
                        println!("{}", session.paragraph(c)?.text);
                    }
                }
                "convert" => save(&mut session, Path::new(argument(&args, 2)?))?,
                "render" => {
                    let out = Path::new(argument(&args, 2)?);
                    if !out
                        .extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("png"))
                    {
                        return Err(usage("render는 PNG(.png)로 저장합니다."));
                    }
                    let scale = match &scale {
                        Some(s) => s
                            .parse::<f32>()
                            .ok()
                            .filter(|s| (0.25..=2.0).contains(s))
                            .ok_or_else(|| usage(format!("--scale은 0.25–2 사이 수입니다: {s}")))?,
                        None => 1.25,
                    };
                    let count = session.page_count()?;
                    let pages: Vec<(u32, PathBuf)> = match args.get(3).map(String::as_str) {
                        Some("all") => (1..=count).map(|p| (p, numbered(out, p))).collect(),
                        Some(v) => vec![(parse_page(v, count)?, out.to_path_buf())],
                        None => vec![(parse_page("1", count)?, out.to_path_buf())],
                    };
                    let renderer = hwpcraft_render::Renderer::default();
                    for (page, path) in pages {
                        let p = renderer.svg(&session.render_page_svg(page - 1)?, scale)?;
                        write_png(&path, &p.rgba, p.width, p.height)?;
                    }
                }
                _ => {}
            }
        }
        _ => {}
    }
    Ok(ExitCode::SUCCESS)
}
fn demo(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    let mut s = Session::new()?;
    s.execute(Command::Character(
        json!({"fontFamily":"Noto Sans KR","bold":true,"fontSize":2400,"textColor":"#234c7d"}),
    ))?;
    s.execute(Command::Insert("HwpCraft · 한글 문서 작업실".into()))?;
    s.execute(Command::Paragraph(
        json!({"alignment":"center","spacingAfter":1600}),
    ))?;
    s.execute(Command::Split)?;
    s.execute(Command::Character(
        json!({"fontFamily":"Noto Sans KR","bold":false,"fontSize":1200,"textColor":"#263445"}),
    ))?;
    s.execute(Command::Insert(
        "문서를 내 컴퓨터에서 만들고, 편집하고, 보관합니다.".into(),
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
        s.execute(Command::Character(
            json!({"fontFamily":"Noto Sans KR","fontSize":1000,"bold":false}),
        ))?;
        s.execute(Command::Insert(text.into()))?;
    }
    s.select(Cursor::default(), false)?;
    for extension in ["hwpx", "hwp", "pdf"] {
        s.save(&dir.join(format!("HwpCraft-시작하기.{extension}")))?;
    }
    let p = hwpcraft_render::Renderer::default().svg(&s.render_page_svg(0)?, 1.25)?;
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
