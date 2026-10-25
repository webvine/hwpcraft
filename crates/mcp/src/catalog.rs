//! Discovery uses the engine registry, not a second list of executable IDs.
use serde_json::{Value, json};

pub(crate) fn commands(query: &str) -> Value {
    let query = query.to_lowercase();
    let mut seen = std::collections::BTreeSet::new();
    let commands: Vec<_> = hwpcraft_engine::COMMANDS.iter().chain(hwpcraft_engine::extension_commands())
        .filter(|(id,label,_)| seen.insert(*id) && (query.is_empty() || id.to_lowercase().contains(&query) || label.to_lowercase().contains(&query)))
        .map(|(id,label,shortcut)| {
            let (help,example) = parameters(id);
            json!({"id":id,"label":label,"shortcut":shortcut,"paramsHelp":help,"paramsExample":example})
        }).collect();
    json!(commands)
}

fn parameters(id: &str) -> (&'static str, Value) {
    match id {
        "table.move" => (
            "Move a top-level table on its current page. page is zero-based; dx_mm/dy_mm are millimetre deltas. Inline tables become floating; split tables are rejected.",
            json!({"table":{"section":0,"paragraph":0,"control":0},"page":0,"dx_mm":10,"dy_mm":5}),
        ),
        "table.draw_create" => (
            "Draw a floating single-cell table. Dimensions and paper-relative origin are millimetres; no active selection or nested table.",
            json!({"width_mm":100,"height_mm":50,"x_mm":30,"y_mm":40}),
        ),
        "table.pen" => (
            "Start/end are [x,y] millimetres from the table origin. erase merges a shared rectangular edge or hides an outer edge. line uses HWP type/width indices.",
            json!({"table":{"section":0,"paragraph":0,"control":0},"start":[30,0],"end":[30,50],"erase":false,"line":{"type":1,"width":0,"color":"#000000"}}),
        ),
        "file.new" => (
            "Blank document; force:true explicitly discards unsaved changes.",
            json!({"force":false}),
        ),
        "file.open" => (
            "HWP/HWPX input path; force:true explicitly discards unsaved changes.",
            json!({"path":"input.hwpx","force":false}),
        ),
        "file.save" | "file.save_as" => (
            "HWP/HWPX save or PDF/TXT export. Only file.save permits omitting path.",
            json!({"path":"output.hwpx"}),
        ),
        "file.pdf" => ("Export to a .pdf path.", json!({"path":"output.pdf"})),
        "file.print_pdf" => (
            "PDF export: pages all/current/to_current/from_current or 1,3,5-7; current is zero-based.",
            json!({"path":"print.pdf","pages":"all","current":0}),
        ),
        "text.insert" | "insert.footnote" | "insert.endnote" => (
            "text string; text.insert splits newlines into paragraphs.",
            json!({"text":"안녕하세요"}),
        ),
        "select.text" => (
            "Case-sensitive search within a paragraph. occurrence starts at 1.",
            json!({"text":"한글","occurrence":1}),
        ),
        "document.text" | "document.inspect" => (
            "Paginated body and first-level cells; follow nextStart. start >= 0, limit 1–1000.",
            json!({"start":0,"limit":100}),
        ),
        "selection.set" => (
            "Zero-based Unicode scalar offset. Optional cell:{control,index,paragraph}; anchor is a scalar offset in the same paragraph.",
            json!({"cursor":{"section":0,"paragraph":0,"offset":2,"cell":null},"anchor":0}),
        ),
        "format.character" => (
            "Partial character properties. fontSize is 1/100 pt, colors #RRGGBB. Use format.character.get to inspect.",
            json!({"bold":true,"fontSize":1200}),
        ),
        "format.paragraph" => (
            "Partial paragraph properties. Use format.paragraph.get to inspect.",
            json!({"alignment":"center","lineSpacing":160}),
        ),
        "insert.table" | "table.create" => (
            "rows/columns. table.create also accepts width_mm,height_mm,treat_as_char,column_widths_mm.",
            json!({"rows":2,"columns":3}),
        ),
        "insert.picture" => (
            "Image path, subject to read root.",
            json!({"path":"picture.png"}),
        ),
        "picture.insert" => (
            "Image path, optional width_mm (1–600), treat_as_char boolean.",
            json!({"path":"picture.png","width_mm":50,"treat_as_char":true}),
        ),
        "edit.find" | "edit.replace" | "edit.replace_all" => (
            "query, optional replacement and options:{case_sensitive,whole_word,direction:down/up/all}.",
            json!({"query":"기존","replacement":"변경","options":{"case_sensitive":true,"direction":"all"}}),
        ),
        "bookmark.add" | "bookmark.goto" | "bookmark.delete" => {
            ("Bookmark name.", json!({"name":"요약"}))
        }
        "insert.hyperlink" => (
            "HTTP/HTTPS uri and optional displayed text.",
            json!({"uri":"https://example.com","text":"참고"}),
        ),
        "style.apply" => (
            "Existing numeric style ID from style.list.",
            json!({"id":0}),
        ),
        "page.setup" => (
            "Partial page setup; dimensions/margins in mm, landscape bool.",
            json!({"landscape":false,"width_mm":210,"height_mm":297}),
        ),
        "page.columns" => (
            "Column count and spacing in mm.",
            json!({"count":2,"spacing_mm":8}),
        ),
        "page.header_footer" => (
            "Header or footer plain text.",
            json!({"is_header":true,"text":"보고서"}),
        ),
        "object.delete" => (
            "ObjectAddress from object.list, passed directly (no address wrapper).",
            json!({"section":0,"paragraph":0,"control":0}),
        ),
        "object.properties.get" | "textbox.paragraphs.get" | "note.paragraphs.get" => (
            "ObjectAddress from object.list.",
            json!({"address":{"section":0,"paragraph":0,"control":0}}),
        ),
        "object.properties" => (
            "ObjectAddress and partial props; use object.properties.get first. Sizes HWPUNIT, colors BGR integers.",
            json!({"address":{"section":0,"paragraph":0,"control":0},"props":{"width":14400}}),
        ),
        "note.set" | "textbox.set" => (
            "ObjectAddress and replacement text.",
            json!({"address":{"section":0,"paragraph":0,"control":0},"text":"수정한 내용"}),
        ),
        "shape.resize" => (
            "ObjectAddress and dimensions in mm.",
            json!({"address":{"section":0,"paragraph":0,"control":0},"width_mm":40,"height_mm":20}),
        ),
        "table.info"
        | "table.properties.get"
        | "table.cell_properties.get"
        | "table.merge"
        | "table.equal_height"
        | "table.equal_width"
        | "table.delete"
        | "table.transpose" => (
            "Optional table:ObjectAddress (otherwise current table), range:{start_row,start_col,end_row,end_col} where applicable; zero-based indices.",
            json!({}),
        ),
        "edit.undo"
        | "edit.redo"
        | "text.backspace"
        | "text.delete"
        | "paragraph.split"
        | "insert.page_break"
        | "insert.column_break"
        | "insert.page_number"
        | "format.character.get"
        | "format.paragraph.get"
        | "object.list"
        | "style.list"
        | "bookmark.list"
        | "chart.list"
        | "memo.list"
        | "file.doc_info" => ("No parameters.", json!({})),
        "insert.equation" => (
            "Hancom equation script, size_pt 4–100.",
            json!({"script":"a over b","size_pt":12}),
        ),
        "equation.set" => (
            "ObjectAddress and options:{script,size_pt}.",
            json!({"address":{"section":0,"paragraph":0,"control":0},"options":{"script":"x^2","size_pt":12}}),
        ),
        "equation.insert" => (
            "script,size_pt; optional color (BGR integer), baseline, font_name.",
            json!({"script":"a over b","size_pt":12}),
        ),
        "insert.shape" => (
            "kind rectangle/ellipse/line/textbox/arc/polygon/curve, dimensions mm, text required.",
            json!({"kind":"rectangle","width_mm":40,"height_mm":20,"text":""}),
        ),
        "shape.insert" => (
            "kind, width_mm,height_mm; optional treat_as_char,text_wrap,points_mm:[[x,y]],paragraphs:[text],x_mm,y_mm.",
            json!({"kind":"textbox","width_mm":40,"height_mm":20,"paragraphs":["글상자"]}),
        ),
        "textbox.paragraphs" | "note.paragraphs" => (
            "ObjectAddress and array of paragraph strings.",
            json!({"address":{"section":0,"paragraph":0,"control":0},"paragraphs":["첫 문단","둘째 문단"]}),
        ),
        "note.insert" => (
            "endnote boolean, paragraphs array.",
            json!({"endnote":false,"paragraphs":["각주 내용"]}),
        ),
        "chart.data.get" => (
            "Zero-based chart index from chart.list.",
            json!({"index":0}),
        ),
        "chart.data" => (
            "Existing chart index and series:[{name?:string,values:[string]}]; no new chart creation.",
            json!({"index":0,"series":[{"name":"매출","values":["10","20"]}]}),
        ),
        "edit.sort" => (
            "key text/number, descending and case_sensitive booleans. Selected body paragraphs or whole section.",
            json!({"key":"text","descending":false,"case_sensitive":false}),
        ),
        "edit.delete_ctrl_marks" => (
            "kinds array: table,picture,shape,equation,footnote,endnote,bookmark,hyperlink,hidden_comment,field.",
            json!({"kinds":["bookmark"]}),
        ),
        "table.edit" => (
            "action: row_above,row_below,column_left,column_right,delete_row,delete_column,fit_page,merge_right,split_merged. Select a cell first.",
            json!({"action":"row_below"}),
        ),
        "table.cell_properties" => (
            "Optional table:ObjectAddress, range:CellRange, props object, borders object. Read matching .get command first; use only fields to change.",
            json!({"props":{"width":14400}}),
        ),
        "table.properties" => (
            "Optional table:ObjectAddress; props uses HWPUNIT lengths and native enum names. Use table.properties.get first.",
            json!({"props":{"repeatHeader":true}}),
        ),
        "table.split" => (
            "rows,columns; optional table,range,equal_height,merge_first.",
            json!({"rows":2,"columns":2}),
        ),
        "table.split_table"
        | "table.merge_table"
        | "table.auto_fill"
        | "table.caption_text.get" => (
            "Optional table:ObjectAddress,range:CellRange; normally operates on current cell/block.",
            json!({}),
        ),
        "table.thousands" => (
            "Optional table,range,remove boolean.",
            json!({"remove":false}),
        ),
        "table.from_text" | "table.to_text" => (
            "delimiter string; table.to_text accepts optional table:ObjectAddress.",
            json!({"delimiter":"\t"}),
        ),
        "table.sort" => (
            "column (zero-based), optional descending,table,range.",
            json!({"column":0,"descending":false}),
        ),
        "table.caption_text" => (
            "text string, optional table:ObjectAddress. Requires an existing caption: table.properties {props:{hasCaption:true}} first.",
            json!({"text":"표 설명"}),
        ),
        "table.style" => (
            "preset string, optional table:ObjectAddress. Presets: basic,blue_header,gray_grid,striped.",
            json!({"preset":"basic"}),
        ),
        "table.move_border" => (
            "vertical bool,boundary zero-based,delta_mm; optional table.",
            json!({"vertical":true,"boundary":1,"delta_mm":2}),
        ),
        "table.resize" => (
            "vertical bool,delta_mm; optional table,range.",
            json!({"vertical":true,"delta_mm":2}),
        ),
        "table.formula" => (
            "formula string, optional table,row,col; otherwise current cell.",
            json!({"formula":"=SUM(A1:A2)"}),
        ),
        "table.block_calc" => (
            "op sum/average/product; optional table,range.",
            json!({"op":"sum"}),
        ),
        "page.header" | "page.footer" => (
            "apply_to both/odd/even, template 0–10.",
            json!({"apply_to":"both","template":2}),
        ),
        "page.header_footer.delete" => (
            "header bool, apply_to both/odd/even.",
            json!({"header":true,"apply_to":"both"}),
        ),
        "page.number" => (
            "position none/top_left/top_center/top_right/bottom_left/bottom_center/bottom_right/outside_top/outside_bottom/inside_top/inside_bottom; format 0–5 or 8/10/12/13, dash bool, start optional.",
            json!({"position":"bottom_center","format":0,"dash":false}),
        ),
        "page.new_number" => ("start 1–65535.", json!({"start":1})),
        "page.hide" => (
            "All six booleans required (snake_case).",
            json!({"header":false,"footer":false,"master_page":false,"border":false,"fill":false,"page_number":true}),
        ),
        "page.border_fill" => (
            "camelCase keys; borders in left/right/top/bottom order, type 0–17,width 0–15,color #RRGGBB; optional fill,fillArea,wholeDocument.",
            json!({"basis":"paper","spacingMm":[0,0,0,0],"headerInside":false,"footerInside":false,"applyPage":"all","borders":[{"type":0,"width":0,"color":"#000000"},{"type":0,"width":0,"color":"#000000"},{"type":0,"width":0,"color":"#000000"},{"type":0,"width":0,"color":"#000000"}]}),
        ),
        "page.border_fill.get"
        | "page.header_footer.list"
        | "page.number.get"
        | "page.hide.get"
        | "page.master.list" => ("No parameters; current section/body paragraph.", json!({})),
        "format.bold"
        | "format.italic"
        | "format.underline"
        | "format.strikethrough"
        | "format.superscript"
        | "format.subscript" => (
            "Optional value boolean; omit to toggle.",
            json!({"value":true}),
        ),
        "format.highlight" => (
            "color #RRGGBB or null to clear; implemented as shading.",
            json!({"color":"#ffff00"}),
        ),
        "format.style.detail" => ("Numeric id from format.style.list.", json!({"id":0})),
        "format.style.create" => (
            "name required; optional englishName,type,nextStyleId,character,paragraph.",
            json!({"name":"보고서 제목","character":{"bold":true,"fontSize":1800}}),
        ),
        "format.style.update" => (
            "id required; optional name,englishName,nextStyleId,character,paragraph.",
            json!({"id":0,"character":{"fontSize":1200}}),
        ),
        "format.style.delete" => (
            "id; optional replacement style ID.",
            json!({"id":1,"replacement":0}),
        ),
        "format.style.shortcut" => ("number 0–9, matching Ctrl+0…Ctrl+9.", json!({"number":1})),
        "format.numbering.create" | "format.outline.shape" => (
            "levelFormats and numberFormats arrays (7 levels), optional startNumber. Use format.numbering.list for examples.",
            json!({"levelFormats":["^1.","^2.","^3.","^4.","^5.","^6.","^7."],"numberFormats":[0,0,0,0,0,0,0],"startNumber":1}),
        ),
        "format.numbering.apply" => (
            "kind none/number/bullet/outline; optional id,char,level,levelFormats,numberFormats,startNumber,restart:{mode,start}.",
            json!({"kind":"number","level":0}),
        ),
        "format.numbering.restart" => (
            "mode continue/previous/new, start required for new.",
            json!({"mode":"new","start":1}),
        ),
        "format.paragraph.apply" => (
            "format: partial paragraph properties, optional head:{kind,level}.",
            json!({"format":{"alignment":"left"}}),
        ),
        // Remaining registered format commands are no-argument steps/list/toggle actions.
        id if id.starts_with("format.") => ("No parameters.", json!({})),
        _ => (
            "Native engine parameters; see docs/mcp-commands.md. null means no generic example, not an unimplemented command.",
            Value::Null,
        ),
    }
}
