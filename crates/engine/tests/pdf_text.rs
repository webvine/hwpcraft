//! Text copied or searched in an exported PDF keeps its word spaces (RHWP 0114): the
//! print SVG that becomes the PDF writes each space as unpainted text, and the screen
//! SVG stays as it was.
use hwpcraft_doc::RenderProfile;
use hwpcraft_engine::Session;
use serde_json::json;

#[test]
fn print_output_keeps_word_spaces_and_the_screen_does_not_change() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run("text.insert", json!({"text":"국립국어원 업무 계획"}))?;
    let print = s
        .core
        .render_page_svg_layer_with_profile_native(0, RenderProfile::Print)
        .map_err(anyhow::Error::msg)?;
    let spaces = print.matches("xml:space=\"preserve\"> </text>").count();
    assert_eq!(spaces, 2, "one unpainted space per word gap");
    assert!(!print.contains("fill-opacity=\"0\" text-decoration"));
    let screen = s.render_page_svg(0)?;
    assert!(!screen.contains("xml:space=\"preserve\"> </text>"));
    Ok(())
}
