//! Explicit nested-cell editor. Canvas hit testing remains a separate route.
use super::*;
use hwpcraft_engine::DocumentAddress;
use serde_json::Value;

#[derive(Clone)]
struct Row {
    address: DocumentAddress,
    text: String,
    protected: bool,
}

pub(crate) struct State {
    open: bool,
    refresh: bool,
    rows: Vec<Row>,
    start: usize,
    next: Option<usize>,
    total: usize,
    truncated: bool,
    show_body: bool,
    selected: Option<DocumentAddress>,
    generation: u64,
    original: String,
    buffer: String,
    protected: bool,
    text_truncated: bool,
    range_start: usize,
    range_end: usize,
    split_at: usize,
    font_size: f64,
    alignment: String,
    line_spacing: f64,
    line_spacing_type: String,
    table_rows: u16,
    table_columns: u16,
    delete_table_content: bool,
    status: String,
}

impl Default for State {
    fn default() -> Self {
        Self {
            open: false,
            refresh: false,
            rows: vec![],
            start: 0,
            next: None,
            total: 0,
            truncated: false,
            show_body: false,
            selected: None,
            generation: 0,
            original: String::new(),
            buffer: String::new(),
            protected: false,
            text_truncated: false,
            range_start: 0,
            range_end: 0,
            split_at: 0,
            font_size: 10.0,
            alignment: "justify".into(),
            line_spacing: 160.0,
            line_spacing_type: "Percent".into(),
            table_rows: 2,
            table_columns: 2,
            delete_table_content: false,
            status: String::new(),
        }
    }
}

impl HwpApp {
    pub(crate) fn contexts_ext_enabled(&self, id: &str) -> Option<bool> {
        (id == "edit.contexts").then_some(true)
    }
    pub(crate) fn contexts_ext_dispatch(&mut self, id: &str) -> bool {
        if id != "edit.contexts" {
            return false;
        }
        self.ext.contexts.open = true;
        self.ext.contexts.refresh = true;
        true
    }
    pub(crate) fn contexts_ext_dialogs(&mut self, ctx: &egui::Context) {
        let mut state = std::mem::take(&mut self.ext.contexts);
        state.show(ctx, &mut self.session, &mut self.message);
        self.ext.contexts = state;
    }
    pub(crate) fn contexts_ext_view(&mut self, name: &str) -> bool {
        if name != "contexts" {
            return false;
        }
        self.contexts_ext_dispatch("edit.contexts")
    }
    pub(crate) fn contexts_ext_modal_open(&self) -> bool {
        self.ext.contexts.open
    }
    pub(crate) fn contexts_canvas(
        &mut self,
        page: u32,
        response: &egui::Response,
        rect: Rect,
    ) -> bool {
        if !response.double_clicked() {
            return false;
        }
        let Some(point) = response.interact_pointer_pos() else {
            return false;
        };
        let x = f64::from((point.x - rect.left()) / self.zoom);
        let y = f64::from((point.y - rect.top()) / self.zoom);
        let Ok(value) = self.session.context_hit_test(page, x, y) else {
            return false;
        };
        let Ok(address) = serde_json::from_value::<DocumentAddress>(value["address"].clone())
        else {
            return false;
        };
        if address.path.len() < 2 {
            return false;
        }
        self.ext.contexts.open = true;
        if self.ext.contexts.dirty() {
            self.message = "입력 중인 문맥 편집을 먼저 적용하거나 취소하세요.".into();
            return true;
        }
        self.ext.contexts.refresh = true;
        if let Err(error) = self.ext.contexts.load(&mut self.session, address) {
            self.ext.contexts.status = error.to_string();
        }
        true
    }
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

fn title(session: &Session, address: &DocumentAddress) -> String {
    let mut text = format!(
        "구역 {} / 본문 문단 {}",
        address.section + 1,
        address.paragraph + 1
    );
    let mut parent = address.clone();
    parent.path.clear();
    for step in &address.path {
        let table = session
            .context_paragraph(&parent)
            .ok()
            .map(|p| {
                p.controls
                    .iter()
                    .take(step.control + 1)
                    .filter(|c| matches!(c, hwpcraft_doc::model::control::Control::Table(_)))
                    .count()
            })
            .unwrap_or(0);
        text.push_str(&format!(
            " / 표 {} 셀 {} 문단 {}",
            table,
            step.cell + 1,
            step.paragraph + 1
        ));
        parent.path.push(step.clone());
    }
    text
}

impl State {
    fn dirty(&self) -> bool {
        self.original != self.buffer
    }
    fn load_rows(&mut self, session: &mut Session) -> anyhow::Result<()> {
        let value = session.run("context.list", json!({"start":self.start,"limit":250}))?;
        self.rows = value["items"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|row| {
                Ok(Row {
                    address: serde_json::from_value(row["address"].clone())?,
                    text: row["text"].as_str().unwrap_or("").to_owned(),
                    protected: row["protected"].as_bool().unwrap_or(true),
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        self.total = value["total"].as_u64().unwrap_or(0) as usize;
        self.next = value["nextStart"].as_u64().map(|n| n as usize);
        self.truncated = value["truncated"].as_bool().unwrap_or(false);
        Ok(())
    }
    fn load(&mut self, session: &mut Session, address: DocumentAddress) -> anyhow::Result<()> {
        let value = session.run("context.get", json!({"address":address}))?;
        self.selected = Some(address);
        self.generation = value["generation"].as_u64().unwrap_or(session.generation);
        self.original = value["text"].as_str().unwrap_or("").to_owned();
        self.buffer = self.original.clone();
        self.protected = value["protected"].as_bool().unwrap_or(true);
        self.text_truncated = value["textTruncated"].as_bool().unwrap_or(false);
        self.range_start = 0;
        self.range_end = self.original.chars().count();
        self.split_at = self.range_end;
        let character = &value["character"];
        self.font_size = character["fontSize"].as_f64().unwrap_or(1000.0) / 100.0;
        let paragraph = &value["paragraph"];
        self.alignment = paragraph["alignment"]
            .as_str()
            .unwrap_or("justify")
            .to_owned();
        self.line_spacing_type = paragraph["lineSpacingType"]
            .as_str()
            .unwrap_or("Percent")
            .to_owned();
        self.line_spacing = paragraph["lineSpacing"].as_f64().unwrap_or(160.0)
            / if self.line_spacing_type == "Percent" {
                1.0
            } else {
                100.0
            };
        self.status.clear();
        Ok(())
    }
    fn apply(&mut self, session: &mut Session, command: &str, params: Value, message: &mut String) {
        let keep_range = matches!(command, "context.character" | "context.paragraph");
        let previous_range = (self.range_start, self.range_end, self.split_at);
        match session.run(command, params) {
            Ok(result) => {
                let address = serde_json::from_value::<DocumentAddress>(result["address"].clone())
                    .ok()
                    .or_else(|| self.selected.clone());
                if let Some(address) = address
                    && let Err(error) = self.load(session, address)
                {
                    self.status = error.to_string();
                    self.selected = None;
                }
                if let Err(error) = self.load_rows(session) {
                    self.status = error.to_string();
                }
                if keep_range {
                    let length = self.original.chars().count();
                    self.range_start = previous_range.0.min(length);
                    self.range_end = previous_range.1.min(length);
                    self.split_at = previous_range.2.min(length);
                }
                *message = "문맥 편집을 적용했습니다.".into();
                if command == "context.table.delete" {
                    self.delete_table_content = false;
                }
            }
            Err(error) => {
                self.status = if command == "context.table.delete"
                    && error.to_string().contains("allow_nonempty")
                {
                    "내용이 있는 표입니다. '표 내용도 함께 삭제'를 선택하면 지울 수 있습니다."
                        .into()
                } else {
                    error.to_string()
                };
            }
        }
    }
    fn show(&mut self, ctx: &egui::Context, session: &mut Session, message: &mut String) {
        if !self.open {
            return;
        }
        if self.refresh {
            self.refresh = false;
            if let Err(error) = self.load_rows(session) {
                self.status = error.to_string();
            }
            if self.selected.is_none()
                && !self.dirty()
                && let Some(row) = self
                    .rows
                    .iter()
                    .find(|row| !row.address.path.is_empty())
                    .or_else(|| self.rows.first())
                    .cloned()
                && let Err(error) = self.load(session, row.address)
            {
                self.status = error.to_string();
            }
        }
        let mut open = self.open;
        egui::Window::new("중첩 표와 본문 문단 편집").id(egui::Id::new("context-editor"))
            .open(&mut open).default_size([980.0,620.0]).min_width(750.0).resizable(true).show(ctx,|ui|{
            ui.label("목록에서 셀 문단을 고른 뒤 내용을 편집합니다. 문단 나누기·합치기는 아래 단추를 사용하세요.");
            ui.horizontal(|ui|{
                if ui.add_enabled(!self.dirty(),egui::Button::new("목록 새로 고침")).clicked() {self.start=0;self.refresh=true;}
                ui.checkbox(&mut self.show_body,"본문도 표시");
                ui.label(format!("{}–{} / {} 문단",self.start.saturating_add(1).min(self.total),self.start+self.rows.len(),self.total));
                if ui.add_enabled(!self.dirty()&&self.start>0,egui::Button::new("이전 목록")).clicked() {self.start=self.start.saturating_sub(250);self.refresh=true;}
                if ui.add_enabled(!self.dirty()&&self.next.is_some(),egui::Button::new("다음 목록")).clicked() {self.start=self.next.unwrap_or(self.start);self.refresh=true;}
            });
            if self.truncated {ui.colored_label(egui::Color32::DARK_RED,"목록의 깊이·개수 제한에 도달했습니다. 표시되지 않은 문단이 있습니다.");}
            ui.separator();
            ui.columns(2,|columns|{
                let left=&mut columns[0];left.heading("문단 목록");
                let rows=self.rows.clone();let mut load=None;
                egui::ScrollArea::vertical().id_salt("context-list").max_height(450.0).show(left,|ui|{
                    for row in rows.iter().filter(|row|self.show_body||!row.address.path.is_empty()) {
                        let selected=self.selected.as_ref()==Some(&row.address);
                        let caption=format!("{}{}\n{}",if row.protected {"보호됨 · "} else {""},title(session,&row.address),if row.text.is_empty(){"(빈 문단)"}else{&row.text});
                        if ui.add_enabled(!self.dirty(),egui::Button::selectable(selected,caption).wrap()).clicked() {load=Some(row.address.clone());}
                    }
                });
                if let Some(address)=load&& let Err(error)=self.load(session,address) {self.status=error.to_string();}
                let ui=&mut columns[1];
                egui::ScrollArea::vertical().id_salt("context-edit").max_height(500.0).show(ui,|ui|{
                let Some(address)=self.selected.clone() else {ui.label("편집할 문단을 선택하세요.");return;};
                ui.heading("문단 편집");ui.label(title(session,&address));
                let stale=self.generation!=session.generation;
                if stale {
                    ui.colored_label(egui::Color32::DARK_RED,"다른 편집으로 문서가 바뀌었습니다. 입력한 내용은 유지되며, 다시 불러와야 적용할 수 있습니다.");
                    if ui.button("다시 불러오기 (입력한 변경 버림)").clicked()&& let Err(error)=self.load(session,address.clone()) {self.status=error.to_string();}
                }
                if self.protected {ui.label("보호된 표/셀입니다. 내용을 읽을 수 있지만 변경할 수 없습니다.");}
                if self.text_truncated {ui.label("문단이 너무 길어 일부만 표시했습니다. 이 창에서는 변경할 수 없습니다.");}
                let editable=!stale&&!self.protected&&!self.text_truncated;
                let edit=ui.add_enabled_ui(editable,|ui|egui::TextEdit::multiline(&mut self.buffer).desired_rows(7).desired_width(f32::INFINITY).id_salt("context-body").show(ui)).inner;
                if edit.response.has_focus() && let Some(range)=edit.cursor_range {
                    let primary:usize=range.primary.index.into();let secondary:usize=range.secondary.index.into();
                    self.range_start=primary.min(secondary);
                    self.range_end=primary.max(secondary);
                    self.split_at=primary;
                }
                let dirty=self.dirty();
                ui.horizontal(|ui|{
                    if ui.add_enabled(editable&&dirty,egui::Button::new("텍스트 적용")).clicked()
                        && let Some((start,end,text))=difference(&self.original,&self.buffer) {
                            self.apply(session,"context.replace",json!({"address":address,"start":start,"end":end,"text":text,"expected_generation":self.generation}),message);
                        }
                    if ui.add_enabled(dirty,egui::Button::new("입력한 변경 취소")).clicked() {self.buffer=self.original.clone();self.status.clear();}
                    ui.label(format!("{}자",self.buffer.chars().count()));
                });
                if dirty {ui.label("문단을 바꾸기 전에 텍스트를 적용하거나 변경을 취소하세요.");}
                let ready=editable&&!self.dirty();let length=self.original.chars().count();
                ui.separator();ui.label("글자를 선택하거나 범위를 지정하세요. 선택이 없으면 문단 전체에 적용합니다.");
                ui.horizontal(|ui|{
                    ui.add(egui::DragValue::new(&mut self.range_start).range(0..=length).prefix("시작 "));
                    ui.add(egui::DragValue::new(&mut self.range_end).range(0..=length).prefix("끝 "));
                });
                ui.add_enabled_ui(ready,|ui|{
                    let (format_start,format_end)=if self.range_start==self.range_end {(0,length)}else{(self.range_start,self.range_end)};
                    ui.horizontal_wrapped(|ui|{
                        for (name,key) in [("진하게 전환","bold"),("기울임 전환","italic"),("밑줄 전환","underline")] {
                            if ui.button(name).clicked() {
                                match session.run("context.get",json!({"address":address,"offset":format_start})) {
                                    Ok(value)=>{let have=value["character"][key].as_bool().unwrap_or(false);self.apply(session,"context.character",json!({"address":address,"start":format_start,"end":format_end,"format":{(key):!have},"expected_generation":self.generation}),message);}
                                    Err(error)=>self.status=error.to_string(),
                                }
                            }
                        }
                        ui.add(egui::DragValue::new(&mut self.font_size).range(4.0..=100.0).speed(0.5).suffix(" pt"));
                        if ui.button("크기 적용").clicked() {self.apply(session,"context.character",json!({"address":address,"start":format_start,"end":format_end,"format":{"fontSize":(self.font_size*100.0).round() as i64},"expected_generation":self.generation}),message);}
                    });
                    ui.horizontal_wrapped(|ui|{
                        for (name,value) in [("왼쪽","left"),("가운데","center"),("오른쪽","right"),("양쪽","justify")] {
                            if ui.selectable_label(self.alignment==value,name).clicked() {self.apply(session,"context.paragraph",json!({"address":address,"format":{"alignment":value},"expected_generation":self.generation}),message);}
                        }
                    });
                    ui.horizontal(|ui|{
                        let was_percent=self.line_spacing_type=="Percent";
                        egui::ComboBox::from_id_salt("context-spacing-type").selected_text(match self.line_spacing_type.as_str(){"Fixed"=>"고정","SpaceOnly"=>"여백만","Minimum"=>"최소",_=>"비율"}).show_ui(ui,|ui|{
                            for (value,label) in [("Percent","비율 (%)"),("Fixed","고정"),("SpaceOnly","여백만"),("Minimum","최소")] {ui.selectable_value(&mut self.line_spacing_type,value.into(),label);}
                        });
                        let percent=self.line_spacing_type=="Percent";
                        if was_percent!=percent {self.line_spacing=if percent {160.0} else {12.0};}
                        ui.add(egui::DragValue::new(&mut self.line_spacing).range(0.01..=if percent {1000.0}else{100.0}).speed(0.5).suffix(if percent {" %"}else{" pt"}));
                        if ui.button("줄 간격 적용").clicked() {self.apply(session,"context.paragraph",json!({"address":address,"format":{"lineSpacing":(self.line_spacing*if percent {1.0}else{100.0}).round() as i64,"lineSpacingType":self.line_spacing_type},"expected_generation":self.generation}),message);}
                    });
                    ui.separator();ui.horizontal_wrapped(|ui|{
                        ui.add(egui::DragValue::new(&mut self.split_at).range(0..=length).prefix("나눌 위치 "));
                        if ui.button("문단 나누기").clicked() {self.apply(session,"context.split",json!({"address":address,"offset":self.split_at,"expected_generation":self.generation}),message);}
                        if ui.button("앞 문단과 합치기").clicked() {self.apply(session,"context.merge",json!({"address":address,"expected_generation":self.generation}),message);}
                    });
                    ui.separator();ui.label("이 문단의 문맥 안에 표를 만들거나 현재 표를 지웁니다.");
                    ui.horizontal_wrapped(|ui|{
                        ui.add(egui::DragValue::new(&mut self.table_rows).range(1..=200).suffix(" 줄"));
                        ui.add(egui::DragValue::new(&mut self.table_columns).range(1..=64).suffix(" 칸"));
                        let cells=usize::from(self.table_rows)*usize::from(self.table_columns);
                        if cells>4096 {ui.colored_label(egui::Color32::DARK_RED,"표는 4,096셀 이하로 만들 수 있습니다.");}
                        if ui.add_enabled(cells<=4096,egui::Button::new("이 문단 뒤에 표 만들기")).clicked() {self.apply(session,"context.table.create",json!({"address":address,"rows":self.table_rows,"columns":self.table_columns,"expected_generation":self.generation}),message);}
                    });
                    ui.horizontal_wrapped(|ui|{
                        ui.checkbox(&mut self.delete_table_content,"표 내용도 함께 삭제");
                        if ui.add_enabled(!address.path.is_empty(),egui::Button::new("현재 표 삭제")).clicked() {
                            let mut parent=address.clone();
                            if let Some(step)=parent.path.pop() {self.apply(session,"context.table.delete",json!({"address":parent,"control":step.control,"allow_nonempty":self.delete_table_content,"expected_generation":self.generation}),message);}
                        }
                    });
                });
                ui.horizontal(|ui|{
                    if ui.add_enabled(!dirty&&session.can_undo(),egui::Button::new("실행 취소")).clicked() {self.apply(session,"edit.undo",json!({}),message);}
                    if ui.add_enabled(!dirty&&session.can_redo(),egui::Button::new("다시 실행")).clicked() {self.apply(session,"edit.redo",json!({}),message);}
                });
                });
            });
            if !self.status.is_empty() {ui.separator();ui.colored_label(egui::Color32::DARK_RED,&self.status);}
        });
        self.open = open;
    }
}

#[cfg(test)]
mod tests {
    use super::{State, difference};
    use hwpcraft_engine::{Command, DocumentAddress, Session};
    use serde_json::json;
    #[test]
    fn minimal_text_diff_counts_unicode_scalars_and_preserves_unchanged_anchors() {
        assert_eq!(
            difference("앞😀 뒤", "앞한자 뒤"),
            Some((1, 2, "한자".into()))
        );
        assert_eq!(
            difference("개체  뒤", "개체  뒤!"),
            Some((5, 5, "!".into()))
        );
        assert_eq!(difference("같음", "같음"), None);
    }

    #[test]
    fn format_actions_keep_selected_range_and_stale_writes_keep_draft() -> anyhow::Result<()> {
        let mut session = Session::new()?;
        session.execute(Command::Insert("앞😀뒤".into()))?;
        let address = DocumentAddress::default();
        let mut state = State::default();
        state.load(&mut session, address.clone())?;
        state.range_start = 1;
        state.range_end = 2;
        let mut message = String::new();
        state.apply(&mut session,"context.character",json!({"address":address,"start":1,"end":2,"format":{"bold":true},"expected_generation":state.generation}),&mut message);
        assert!(state.status.is_empty());
        assert_eq!((state.range_start, state.range_end), (1, 2));
        assert_eq!(
            session.run("context.get", json!({"address":address,"offset":1}))?["character"]["bold"],
            true
        );
        assert_eq!(
            session.run("context.get", json!({"address":address,"offset":0}))?["character"]["bold"],
            false
        );
        state.buffer = "앞 새 내용 뒤".into();
        session.execute(Command::Insert("외부 편집".into()))?;
        let actual = session.text().to_owned();
        state.apply(&mut session,"context.replace",json!({"address":address,"start":0,"end":3,"text":"덮어쓰기","expected_generation":state.generation}),&mut message);
        assert!(!state.status.is_empty());
        assert_eq!(state.buffer, "앞 새 내용 뒤");
        assert!(state.dirty());
        assert_eq!(session.text(), actual);
        Ok(())
    }
}
