use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

/// Public editor units are millimetres; RHWP stores 1/7200 inch units.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageSetup {
    pub width_mm: f64,
    pub height_mm: f64,
    pub landscape: bool,
    pub left_mm: f64,
    pub right_mm: f64,
    pub top_mm: f64,
    pub bottom_mm: f64,
    pub header_mm: f64,
    pub footer_mm: f64,
    pub gutter_mm: f64,
    pub binding: u8,
}

impl PageSetup {
    pub(crate) fn from_page(p: &hwpcraft_doc::model::page::PageDef) -> Self {
        use hwpcraft_doc::model::page::BindingMethod;
        let mm = |v: u32| f64::from(v) * 25.4 / 7200.0;
        Self {
            width_mm: mm(p.width),
            height_mm: mm(p.height),
            landscape: p.landscape,
            left_mm: mm(p.margin_left),
            right_mm: mm(p.margin_right),
            top_mm: mm(p.margin_top),
            bottom_mm: mm(p.margin_bottom),
            header_mm: mm(p.margin_header),
            footer_mm: mm(p.margin_footer),
            gutter_mm: mm(p.margin_gutter),
            binding: match p.binding {
                BindingMethod::SingleSided => 0,
                BindingMethod::DuplexSided => 1,
                BindingMethod::TopFlip => 2,
            },
        }
    }

    pub fn validate(&self) -> Result<()> {
        for v in [self.width_mm, self.height_mm] {
            if !v.is_finite() || !(50.0..=1000.0).contains(&v) {
                bail!("용지 크기는 50–1000 mm 범위여야 합니다.");
            }
        }
        for v in [
            self.left_mm,
            self.right_mm,
            self.top_mm,
            self.bottom_mm,
            self.header_mm,
            self.footer_mm,
            self.gutter_mm,
        ] {
            if !v.is_finite() || !(0.0..=500.0).contains(&v) {
                bail!("여백은 0–500 mm 범위여야 합니다.");
            }
        }
        if self.binding > 2 {
            bail!("지원하지 않는 제본 방향입니다.");
        }
        let (w, h) = if self.landscape {
            (self.height_mm, self.width_mm)
        } else {
            (self.width_mm, self.height_mm)
        };
        // Match RHWP's PageAreas: its current gutter is included in the horizontal box.
        if w - self.left_mm - self.right_mm - self.gutter_mm < 10.0
            || h - self.top_mm - self.bottom_mm - self.header_mm - self.footer_mm < 10.0
        {
            bail!("본문 너비와 높이를 각각 10 mm 이상 남겨 주세요.");
        }
        Ok(())
    }

    pub(crate) fn properties(&self) -> serde_json::Value {
        let hu = |v: f64| (v * 7200.0 / 25.4).round() as u32;
        serde_json::json!({"width":hu(self.width_mm),"height":hu(self.height_mm),
            "landscape":self.landscape,"marginLeft":hu(self.left_mm),"marginRight":hu(self.right_mm),
            "marginTop":hu(self.top_mm),"marginBottom":hu(self.bottom_mm),
            "marginHeader":hu(self.header_mm),"marginFooter":hu(self.footer_mm),
            "marginGutter":hu(self.gutter_mm),"binding":self.binding})
    }
}
