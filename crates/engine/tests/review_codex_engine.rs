//! Reproductions from the read-only review of new engine modules (table-object area).
//! Keep former owner-review reproductions enabled after their fixes are verified.
use hwpcraft_engine::{Command, Session};
use serde_json::json;

/// chart.create accepts any finite value within ±1e12, but the OOXML writer
/// formats values with `{}`; a subnormal such as 5e-324 becomes a ~330-character
/// decimal string, so a chart well inside the documented limits (16 series ×
/// 1000 values) is rejected as "XML over 2 MiB". Expected: created.
#[test]
fn chart_with_tiny_values_inside_documented_limits_is_created() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("차트".into()))?;
    let categories: Vec<String> = (0..1000).map(|i| format!("c{i}")).collect();
    let series: Vec<_> = (0..16)
        .map(|i| json!({"name":format!("t{i}"),"values":vec![5e-324; 1000]}))
        .collect();
    let v = s.run(
        "chart.create",
        json!({"chart":{"kind":"line","categories":categories,"series":series}}),
    )?;
    assert!(v["index"].is_u64(), "{v}");
    Ok(())
}
