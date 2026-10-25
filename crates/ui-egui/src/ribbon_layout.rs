//! Width planning adapted from WordCraft ui-egui/ribbon_fit.rs (MIT).
//! Copyright (c) 2026 ArtCraft Team and the WordCraft contributors.
//! See assets/licenses/LICENSE-WordCraft-MIT.txt.
pub const FULL: u8 = 0;
pub const COMPACT: u8 = 1;
pub const COLLAPSED: u8 = 2;
pub fn plan(groups: &[[f32; 3]], avail: f32) -> Vec<u8> {
    let width = |g: &[f32; 3], level: u8| g[usize::from(level)];
    let mut levels = vec![FULL; groups.len()];
    let mut total: f32 = groups.iter().map(|g| width(g, FULL)).sum();
    for target in [COMPACT, COLLAPSED] {
        for i in (0..groups.len()).rev() {
            if total <= avail {
                break;
            }
            let (Some(g), Some(level)) = (groups.get(i), levels.get_mut(i)) else {
                continue;
            };
            total += width(g, target) - width(g, *level);
            *level = target;
        }
    }
    for (g, level) in groups.iter().zip(levels.iter_mut()) {
        while *level > FULL {
            let wider = *level - 1;
            let next = total + width(g, wider) - width(g, *level);
            if !next.is_finite() || next > avail {
                break;
            }
            total = next;
            *level = wider;
        }
    }
    levels
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fit_from_right_and_restore_when_widened() {
        let g = [
            [220.0, 120.0, 70.0],
            [220.0, 120.0, 70.0],
            [220.0, 120.0, 70.0],
        ];
        assert_eq!(plan(&g, 700.0), [FULL, FULL, FULL]);
        let narrow = plan(&g, 250.0);
        assert!(
            g.iter()
                .zip(narrow.iter())
                .map(|(g, l)| g[usize::from(*l)])
                .sum::<f32>()
                <= 250.0
        );
        assert_eq!(plan(&[], 0.0), Vec::<u8>::new());
    }
}
