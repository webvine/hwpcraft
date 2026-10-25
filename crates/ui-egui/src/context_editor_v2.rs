//! Explicit editing route for document regions that the legacy canvas caret
//! cannot yet represent. Product labels intentionally use ordinary region names.
use hwpcraft_engine::{ContextAddress, ContextStep, Session};
use serde_json::{Value, json};

#[derive(Clone)]
struct Row {
    address: ContextAddress,
    text: String,
    kind: String,
    readonly: bool,
}

pub(crate) struct State {
    pub open: bool,
    refresh: bool,
    filter: String,
    rows: Vec<Row>,
    start: usize,
    total: usize,
    next: Option<usize>,
    truncated: bool,
    selected: Option<ContextAddress>,
    generation: u64,
    original: String,
    buffer: String,
    editable: bool,
    can_merge: bool,
    readonly_message: String,
    range_start: usize,
    range_end: usize,
    split_at: usize,
    font_size: f64,
    font_family: String,
    alignment: String,
    line_spacing: f64,
    line_spacing_type: String,
    status: String,
}
impl Default for State {
    fn default() -> Self {
        Self {
            open: false,
            refresh: true,
            filter: String::new(),
            rows: vec![],
            start: 0,
            total: 0,
            next: None,
            truncated: false,
            selected: None,
            generation: 0,
            original: String::new(),
            buffer: String::new(),
            editable: false,
            can_merge: false,
            readonly_message: String::new(),
            range_start: 0,
            range_end: 0,
            split_at: 0,
            font_size: 10.0,
            font_family: String::new(),
            alignment: "justify".into(),
            line_spacing: 160.0,
            line_spacing_type: "Percent".into(),
            status: String::new(),
        }
    }
}
fn kind_label(kind: &str) -> &'static str {
    match kind {
        "body" => "본문",
        "table_cell" => "표 셀",
        "header" => "머리말",
        "footer" => "꼬리말",
        "textbox" => "글상자",
        "footnote" => "각주",
        "endnote" => "미주",
        _ => "전체",
    }
}
fn title(address: &ContextAddress) -> String {
    let mut result = format!(
        "구역 {} · 본문 문단 {}",
        address.section + 1,
        address.paragraph + 1
    );
    for step in &address.path {
        let (name, paragraph) = match step {
            ContextStep::TableCell {
                cell, paragraph, ..
            } => (format!("표 셀 {}", cell + 1), paragraph),
            ContextStep::Header { paragraph, .. } => ("머리말".into(), paragraph),
            ContextStep::Footer { paragraph, .. } => ("꼬리말".into(), paragraph),
            ContextStep::Footnote { paragraph, .. } => ("각주".into(), paragraph),
            ContextStep::Endnote { paragraph, .. } => ("미주".into(), paragraph),
            ContextStep::TextBox {
                paragraph,
                shape_path,
                ..
            } => (
                if shape_path.is_empty() {
                    "글상자".into()
                } else {
                    "묶음 안 글상자".into()
                },
                paragraph,
            ),
        };
        result.push_str(&format!(" / {name} 문단 {}", paragraph + 1));
    }
    result
}
fn difference(before: &str, after: &str) -> Option<(usize, usize, String)> {
    if before == after {
        return None;
    }
    let a: Vec<char> = before.chars().collect();
    let b: Vec<char> = after.chars().collect();
    let prefix = a.iter().zip(&b).take_while(|(a, b)| a == b).count();
    let suffix = a[prefix..]
        .iter()
        .rev()
        .zip(b[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    Some((
        prefix,
        a.len() - suffix,
        b[prefix..b.len() - suffix].iter().collect(),
    ))
}
fn readonly_message(value: &Value) -> String {
    if value["protected"] == true {
        return "보호된 표·셀·개체 안의 문단입니다. 내용을 읽을 수 있지만 고칠 수 없습니다.".into();
    }
    if value["textTruncated"] == true {
        return "문단이 너무 길어 일부만 표시했습니다. 이 창에서는 고칠 수 없습니다.".into();
    }
    if value["readonlyReasons"]
        .as_array()
        .is_some_and(|reasons| !reasons.is_empty())
    {
        return "이 문단의 도형 또는 세로쓰기 배치는 아직 이 편집 창에서 지원하지 않습니다. 내용을 읽을 수 있습니다.".into();
    }
    String::new()
}

impl State {
    pub fn open_editor(&mut self) {
        self.open = true;
        self.refresh = true;
    }
    fn dirty(&self) -> bool {
        self.buffer != self.original
    }
    fn load_rows(&mut self, session: &mut Session) -> anyhow::Result<()> {
        let include = if self.filter.is_empty() {
            vec![]
        } else {
            vec![self.filter.clone()]
        };
        let value = session.run(
            "context.v2.list",
            json!({"start":self.start,"limit":250,"include":include}),
        )?;
        let rows = value["items"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("문단 목록을 읽지 못했습니다."))?;
        self.rows = rows
            .iter()
            .map(|row| {
                Ok(Row {
                    address: serde_json::from_value(row["address"].clone())?,
                    text: row["text"].as_str().unwrap_or("").into(),
                    kind: row["kind"].as_str().unwrap_or("body").into(),
                    readonly: row["readonly"].as_bool().unwrap_or(true),
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        self.total = value["total"].as_u64().unwrap_or(0) as usize;
        self.next = value["nextStart"].as_u64().map(|n| n as usize);
        self.truncated = value["truncated"].as_bool().unwrap_or(false);
        Ok(())
    }
    fn load(&mut self, session: &mut Session, address: ContextAddress) -> anyhow::Result<()> {
        let value = session.run("context.v2.get", json!({"address":address}))?;
        let original = value["text"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("문단 내용을 읽지 못했습니다."))?
            .to_owned();
        self.selected = Some(address);
        self.generation = value["generation"].as_u64().unwrap_or(session.generation);
        self.original = original;
        self.buffer = self.original.clone();
        self.readonly_message = readonly_message(&value);
        self.editable = value["capabilities"]["replace"] == true && value["textTruncated"] != true;
        self.can_merge = value["capabilities"]["merge"] == true;
        self.range_start = 0;
        self.range_end = self.original.chars().count();
        self.split_at = self.range_end;
        self.font_size = value["character"]["fontSize"].as_f64().unwrap_or(1000.0) / 100.0;
        self.font_family = value["character"]["fontFamily"]
            .as_str()
            .unwrap_or("")
            .into();
        self.alignment = value["paragraph"]["alignment"]
            .as_str()
            .unwrap_or("justify")
            .into();
        self.line_spacing_type = value["paragraph"]["lineSpacingType"]
            .as_str()
            .unwrap_or("Percent")
            .into();
        self.line_spacing = value["paragraph"]["lineSpacing"].as_f64().unwrap_or(160.0)
            / if self.line_spacing_type == "Percent" {
                1.0
            } else {
                100.0
            };
        self.status.clear();
        Ok(())
    }
    fn apply(
        &mut self,
        session: &mut Session,
        command: &str,
        mut params: Value,
        message: &mut String,
    ) {
        if command.starts_with("context.v2.") {
            params["expected_generation"] = json!(self.generation);
        }
        let keep_range = matches!(command, "context.v2.character" | "context.v2.paragraph");
        let range = (self.range_start, self.range_end, self.split_at);
        match session.run(command, params) {
            Ok(value) => {
                let address = serde_json::from_value::<ContextAddress>(value["address"].clone())
                    .ok()
                    .or_else(|| self.selected.clone());
                if let Some(address) = address
                    && self.load(session, address).is_err()
                {
                    self.selected = None;
                    self.buffer.clear();
                    self.original.clear();
                    self.status =
                        "문서 구조가 바뀌었습니다. 목록에서 편집할 문단을 다시 선택하세요.".into();
                }
                if let Err(error) = self.load_rows(session) {
                    self.status = error.to_string();
                }
                if keep_range {
                    let length = self.original.chars().count();
                    self.range_start = range.0.min(length);
                    self.range_end = range.1.min(length);
                    self.split_at = range.2.min(length);
                }
                *message = "문단 편집을 적용했습니다.".into();
            }
            Err(error) => self.status = error.to_string(),
        }
    }
    pub fn show(&mut self, ctx: &egui::Context, session: &mut Session, message: &mut String) {
        if !self.open {
            return;
        }
        if self.refresh {
            self.refresh = false;
            if let Err(error) = self.load_rows(session) {
                self.status = error.to_string();
            }
        }
        let mut open = self.open;
        egui::Window::new("문서 영역 편집").id(egui::Id::new("document-region-editor")).open(&mut open).default_size(egui::vec2(1000.0,660.0)).show(ctx,|ui|{
            ui.label("본문, 표, 머리말, 꼬리말, 글상자와 주석의 문단을 편집합니다.");
            let dirty=self.dirty();
            ui.horizontal_wrapped(|ui|{
                let previous=self.filter.clone();
                ui.add_enabled_ui(!dirty,|ui|egui::ComboBox::from_id_salt("region-kind").selected_text(kind_label(&self.filter)).show_ui(ui,|ui|{
                    for kind in ["","body","table_cell","header","footer","textbox","footnote","endnote"]{ui.selectable_value(&mut self.filter,kind.into(),kind_label(kind));}
                }));
                if previous!=self.filter{self.start=0;self.refresh=true;}
                if ui.add_enabled(!dirty,egui::Button::new("목록 새로 고침")).clicked(){self.refresh=true;}
                ui.label(format!("{}–{} / {} 문단",self.start.saturating_add(1).min(self.total),self.start+self.rows.len(),self.total));
                if ui.add_enabled(!dirty&&self.start>0,egui::Button::new("이전 목록")).clicked(){self.start=self.start.saturating_sub(250);self.refresh=true;}
                if ui.add_enabled(!dirty&&self.next.is_some(),egui::Button::new("다음 목록")).clicked(){self.start=self.next.unwrap_or(self.start);self.refresh=true;}
            });
            if self.truncated{ui.label("문단의 깊이 또는 개수 제한에 도달해 일부 문단을 표시하지 못했습니다.");}
            ui.separator();
            ui.columns(2,|columns|{
                let left=&mut columns[0];left.heading("문단 목록");
                let mut selected=None;
                egui::ScrollArea::vertical().id_salt("region-list").max_height(500.0).show(left,|ui|{
                    for row in &self.rows{
                        let caption=format!("{}{} · {}\n{}",if row.readonly{"읽기 전용 · "}else{""},kind_label(&row.kind),title(&row.address),if row.text.is_empty(){"(빈 문단)"}else{&row.text});
                        if ui.add_enabled(!dirty,egui::Button::selectable(self.selected.as_ref()==Some(&row.address),caption).wrap()).clicked(){selected=Some(row.address.clone());}
                    }
                });
                if let Some(address)=selected&&let Err(error)=self.load(session,address){self.status=error.to_string();}
                let ui=&mut columns[1];
                egui::ScrollArea::vertical().id_salt("region-fields").max_height(500.0).show(ui,|ui|{
                    let Some(address)=self.selected.clone()else{ui.label("목록에서 편집할 문단을 선택하세요.");return;};
                    ui.heading("문단 편집");ui.label(title(&address));
                    let stale=self.generation!=session.generation;
                    if stale{
                        ui.colored_label(egui::Color32::DARK_RED,"다른 편집으로 문서가 바뀌었습니다. 입력한 내용은 유지되며 다시 불러와야 적용할 수 있습니다.");
                        if ui.button("다시 불러오기 (입력한 변경 버림)").clicked()&&let Err(error)=self.load(session,address.clone()){self.status=error.to_string();}
                    }
                    if !self.readonly_message.is_empty(){ui.label(&self.readonly_message);}
                    let editable=self.editable&&!stale;
                    let edit=ui.add_enabled_ui(editable,|ui|egui::TextEdit::multiline(&mut self.buffer).desired_rows(7).desired_width(f32::INFINITY).id_salt("region-text").show(ui)).inner;
                    if edit.response.has_focus()&&let Some(range)=edit.cursor_range{let a:usize=range.primary.index.into();let b:usize=range.secondary.index.into();self.range_start=a.min(b);self.range_end=a.max(b);self.split_at=a;}
                    let dirty=self.dirty();
                    ui.horizontal(|ui|{
                        if ui.add_enabled(editable&&dirty,egui::Button::new("텍스트 적용")).clicked()&&let Some((start,end,text))=difference(&self.original,&self.buffer){self.apply(session,"context.v2.replace",json!({"address":address,"start":start,"end":end,"text":text}),message);}
                        if ui.add_enabled(dirty,egui::Button::new("입력한 변경 취소")).clicked(){self.buffer=self.original.clone();self.status.clear();}
                        ui.label(format!("{}자",self.buffer.chars().count()));
                    });
                    ui.label("새 문단은 아래의 문단 나누기를 사용하세요.");
                    if self.dirty(){ui.label("다른 문단을 선택하거나 서식을 바꾸기 전에 입력한 텍스트를 적용하거나 취소하세요.");}
                    let ready=editable&&!self.dirty();let length=self.original.chars().count();
                    ui.add_enabled_ui(ready,|ui|{
                        ui.separator();ui.label("글자 선택 또는 범위 지정 · 선택이 없으면 문단 전체");
                        ui.horizontal(|ui|{ui.add(egui::DragValue::new(&mut self.range_start).range(0..=length).prefix("시작 "));ui.add(egui::DragValue::new(&mut self.range_end).range(0..=length).prefix("끝 "));});
                        let(start,end)=if self.range_start==self.range_end{(0,length)}else{(self.range_start.min(self.range_end),self.range_start.max(self.range_end))};
                        ui.horizontal_wrapped(|ui|{
                            for(label,key)in[("진하게 전환","bold"),("기울임 전환","italic"),("밑줄 전환","underline")]{if ui.button(label).clicked(){match session.run("context.v2.get",json!({"address":address,"offset":start})){Ok(value)=>self.apply(session,"context.v2.character",json!({"address":address,"start":start,"end":end,"format":{(key):!value["character"][key].as_bool().unwrap_or(false)}}),message),Err(error)=>self.status=error.to_string()}}}
                        });
                        ui.horizontal_wrapped(|ui|{ui.add(egui::DragValue::new(&mut self.font_size).range(4.0..=100.0).speed(0.5).suffix(" pt"));if ui.button("크기 적용").clicked(){self.apply(session,"context.v2.character",json!({"address":address,"start":start,"end":end,"format":{"fontSize":(self.font_size*100.0).round() as i64}}),message);}});
                        ui.horizontal(|ui|{ui.add(egui::TextEdit::singleline(&mut self.font_family).desired_width(180.0).hint_text("글꼴 이름"));if ui.button("글꼴 적용").clicked(){self.apply(session,"context.v2.character",json!({"address":address,"start":start,"end":end,"format":{"fontFamily":self.font_family}}),message);}});
                        ui.horizontal_wrapped(|ui|{for(label,value)in[("왼쪽","left"),("가운데","center"),("오른쪽","right"),("양쪽","justify")]{if ui.selectable_label(self.alignment==value,label).clicked(){self.apply(session,"context.v2.paragraph",json!({"address":address,"format":{"alignment":value}}),message);}}});
                        ui.horizontal_wrapped(|ui|{
                            let was_percent=self.line_spacing_type=="Percent";
                            egui::ComboBox::from_id_salt("region-spacing").selected_text(match self.line_spacing_type.as_str(){"Fixed"=>"고정","Minimum"=>"최소","SpaceOnly"=>"여백만",_=>"비율"}).show_ui(ui,|ui|{for(value,label)in[("Percent","비율 (%)"),("Fixed","고정 (pt)"),("Minimum","최소 (pt)"),("SpaceOnly","여백만 (pt)")]{ui.selectable_value(&mut self.line_spacing_type,value.into(),label);}});
                            let percent=self.line_spacing_type=="Percent";if percent!=was_percent{self.line_spacing=if percent{160.0}else{12.0};}
                            ui.add(egui::DragValue::new(&mut self.line_spacing).range(0.01..=if percent{1000.0}else{100.0}).suffix(if percent{" %"}else{" pt"}));
                            if ui.button("줄 간격 적용").clicked(){self.apply(session,"context.v2.paragraph",json!({"address":address,"format":{"lineSpacing":(self.line_spacing*if percent{1.0}else{100.0}).round() as i64,"lineSpacingType":self.line_spacing_type}}),message);}
                        });
                        ui.separator();ui.horizontal_wrapped(|ui|{
                            ui.add(egui::DragValue::new(&mut self.split_at).range(0..=length).prefix("나눌 위치 "));
                            if ui.button("문단 나누기").clicked(){self.apply(session,"context.v2.split",json!({"address":address,"offset":self.split_at}),message);}
                            if ui.add_enabled(self.can_merge,egui::Button::new("앞 문단과 합치기")).clicked(){self.apply(session,"context.v2.merge",json!({"address":address}),message);}
                        });
                    });
                    ui.horizontal(|ui|{
                        if ui.add_enabled(!self.dirty()&&session.can_undo(),egui::Button::new("실행 취소")).clicked(){self.apply(session,"edit.undo",json!({}),message);}
                        if ui.add_enabled(!self.dirty()&&session.can_redo(),egui::Button::new("다시 실행")).clicked(){self.apply(session,"edit.redo",json!({}),message);}
                    });
                });
            });
            if !self.status.is_empty(){ui.separator();ui.colored_label(egui::Color32::DARK_RED,&self.status);}
        });
        self.open = open;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn minimal_scalar_diff_preserves_unchanged_control_anchor_and_text() {
        assert_eq!(
            difference("앞😀 개체 뒤", "앞한자 개체 뒤"),
            Some((1, 2, "한자".into()))
        );
        assert_eq!(difference("변경 없음", "변경 없음"), None);
        assert_eq!(kind_label("header"), "머리말");
        assert_eq!(kind_label("footnote"), "각주");
    }

    #[test]
    fn formatting_keeps_range_and_stale_writes_preserve_the_input_draft() -> anyhow::Result<()> {
        use hwpcraft_engine::Command;
        let mut session = Session::new()?;
        session.execute(Command::Insert("앞😀뒤".into()))?;
        let address = ContextAddress::default();
        let mut state = State::default();
        state.load(&mut session, address.clone())?;
        state.range_start = 1;
        state.range_end = 2;
        let mut message = String::new();
        state.apply(
            &mut session,
            "context.v2.character",
            json!({"address":address,"start":1,"end":2,"format":{"bold":true}}),
            &mut message,
        );
        assert!(state.status.is_empty());
        assert_eq!((state.range_start, state.range_end), (1, 2));
        state.buffer = "입력한 변경😀".into();
        session.execute(Command::Insert("외부 변경".into()))?;
        let actual = session.text().to_owned();
        state.apply(
            &mut session,
            "context.v2.replace",
            json!({"address":address,"start":0,"end":3,"text":"덮어쓰기"}),
            &mut message,
        );
        assert!(!state.status.is_empty());
        assert_eq!(state.buffer, "입력한 변경😀");
        assert_eq!(session.text(), actual);
        state.open = false;
        state.show(&egui::Context::default(), &mut session, &mut message);
        assert_eq!(state.buffer, "입력한 변경😀");
        Ok(())
    }
}
