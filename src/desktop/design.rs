//! Measured rectangles in the 1536 × 1024 design, including its 48px caption.
//! Convert only at the window boundary; the content host begins at 114 client px.
pub(super) const SPLIT: f64 = 475. / 1435.;
pub(super) type Area = (i32, i32, i32, i32);
pub(super) struct Canvas {
    pub width: i32,
    pub height: i32,
    pub split: f64,
}
impl Canvas {
    fn convert(&self, r: Area, root: bool) -> Area {
        let (x, y, w, h) = r;
        let sx = self.width as f64 / 1536.;
        (
            (x as f64 * sx).round() as i32,
            ((y - if root { 48 } else { 162 }) as f64 / 1.5).round() as i32,
            (w as f64 * sx).round() as i32,
            (h as f64 / 1.5).round() as i32,
        )
    }
    fn delta(&self) -> i32 {
        ((self.split.clamp(0.25, 0.5) - SPLIT) * 1435.).round() as i32
    }
    pub fn cards(&self, page: usize) -> Vec<Area> {
        let d = self.delta();
        let rs = match page {
            0 => vec![(36, 169, 475 + d, 707), (541 + d, 169, 960 - d, 707)],
            1 => vec![
                (40, 261, 714, 270),
                (782, 261, 714, 270),
                (40, 557, 714, 270),
                (782, 557, 714, 270),
            ],
            2 => vec![
                (40, 260, 1457, 169),
                (40, 451, 1457, 296),
                (40, 769, 1457, 112),
            ],
            3 => vec![
                (38, 167, 1461, 765),
                (69, 327, 690, 277),
                (780, 327, 690, 277),
                (69, 623, 1402, 196),
            ],
            5 => vec![
                (36, 272, 910, 426),
                (974, 272, 528, 232),
                (974, 526, 528, 316),
            ],
            _ => vec![],
        };
        rs.into_iter()
            .map(|mut r| {
                if page == 0 {
                    r.3 += ((self.height - 651) as f64 * 1.5).round() as i32;
                }
                self.convert(r, false)
            })
            .collect()
    }
    pub fn control(&self, id: i32) -> Option<Area> {
        let r = match id {
            9 => (366, 69, 1136, 77),
            9002 => (44, 68, 285, 78),
            9001 => (42, 940, 1100, 50),
            113 | 330 | 506 | 918 => (1236, 912, 265, 68),
            7000 => (70, 200, 400, 67),
            114 => (577, 200, 880, 67),
            119 => (70, 290, 405, 460),
            100 => (0, 162, 405, 460),
            118 => (70, 550, 405, 45),
            120 => (511, 169, 30, 707),
            110 => (67, 773, 205, 61),
            111 => (288, 773, 192, 61),
            112 => (579, 284, 363, 59),
            6200 => (582, 365, 880, 36),
            200 => (581, 405, 884, 52),
            6201 => (582, 482, 880, 36),
            201 => (581, 523, 729, 52),
            115 => (1331, 520, 140, 58),
            6207 => (582, 599, 650, 36),
            207 => (581, 640, 657, 53),
            208 => (1269, 645, 198, 40),
            116 => (578, 722, 892, 62),
            202..=206 => (581, 846 + (id - 202) * 114, 884, 54),
            6202..=6206 => (582, 810 + (id - 6202) * 114, 880, 36),
            209 => (582, 1428, 884, 42),
            7001 | 7002 | 7012 => (49, 177, 1400, 68),
            350..=353 | 340..=343 | 320..=323 | 300..=307 | 360..=367 => {
                let (slot, kind, col) = match id {
                    350..=353 => (id - 350, 0, 0),
                    340..=343 => (id - 340, 1, 0),
                    320..=323 => (id - 320, 2, 0),
                    300..=307 => ((id - 300) / 2, 3, (id - 300) % 2),
                    _ => ((id - 360) / 2, 4, (id - 360) % 2),
                };
                let x = 40 + (slot % 2) * 742;
                let y = 261 + (slot / 2) * 296;
                match kind {
                    0 => (x + 38, y + 19, 450, 60),
                    1 => (x + 40, y + 80, 630, 38),
                    2 => (x + 514, y + 23, 176, 55),
                    3 => (x + 38 + col * 341, y + 182, 312, 52),
                    _ => (x + 38 + col * 341, y + 140, 320, 38),
                }
            }
            400 => (70, 288, 1380, 40),
            401 => (73, 345, 341, 61),
            7010 => (442, 355, 980, 40),
            405 => (76, 477, 1300, 78),
            402 => (76, 562, 1382, 44),
            406 => (76, 614, 400, 40),
            407 => (1100, 614, 360, 40),
            403 => (73, 663, 339, 61),
            404 => (75, 796, 366, 61),
            7003 => (70, 296, 840, 66),
            7040 => (1008, 296, 460, 66),
            7050 => (1007, 548, 460, 66),
            500..=503 => (281, 390 + (id - 500) * 71, 636, 50),
            6500..=6503 => (70, 396 + (id - 6500) * 71, 205, 40),
            7051 => (1009, 366, 460, 40),
            6504 => (1010, 425, 210, 40),
            504 => (1229, 419, 242, 52),
            505 => (1016, 627, 450, 42),
            507 => (1016, 687, 450, 42),
            7052 => (1016, 747, 450, 72),
            820 => (77, 184, 1300, 67),
            811 => (80, 249, 1300, 37),
            810 => (80, 283, 1300, 37),
            821 => (101, 336, 620, 60),
            822 => (812, 336, 620, 60),
            800 => (104, 404, 620, 40),
            817 => (815, 404, 620, 40),
            6801 => (105, 452, 255, 38),
            6802 => (105, 505, 255, 38),
            6804 => (820, 452, 255, 38),
            6803 => (820, 505, 255, 38),
            801 => (364, 448, 359, 42),
            802 => (364, 501, 359, 42),
            804 => (1080, 448, 359, 42),
            803 => (1080, 501, 359, 42),
            815 => (104, 550, 630, 40),
            816 => (820, 550, 630, 40),
            823 => (102, 630, 1300, 59),
            805..=807 => (104, 690 + (id - 805) * 39, 1320, 40),
            812 => (74, 842, 231, 60),
            813 => (322, 842, 230, 60),
            814 => (567, 842, 219, 60),
            809 => (803, 842, 228, 60),
            808 => (1226, 840, 250, 63),
            _ => return None,
        };
        let (mut x, y, mut w, h) = r;
        let d = self.delta();
        if matches!(id, 114 | 112 | 200..=207 | 6200..=6207 | 116 | 209) {
            x += d;
            w -= d;
        }
        if matches!(id, 7000 | 119 | 100 | 118) {
            w += d;
        }
        if id == 120 {
            x += d;
        }
        if matches!(id, 110 | 111) {
            w += d / 2;
            if id == 111 {
                x += d / 2;
            }
        }
        let root = matches!(id, 9 | 9002 | 9001 | 113 | 330 | 506 | 918);
        let mut area = self.convert((x, y, w, h), root);
        if matches!(id, 9001 | 113 | 330 | 506 | 918) {
            area.1 += self.height - 651;
        }
        if matches!(id, 119 | 100 | 120) {
            area.3 += self.height - 651;
        }
        if matches!(id, 110 | 111) {
            area.1 += self.height - 651;
        }
        if id == 118 {
            area.1 += (self.height - 651) / 2;
        }
        Some(area)
    }
}
/// Pixel rectangles for the empty-list illustration and its centered caption.
pub(super) fn empty_state(width: i32, height: i32, line: i32, dpi: u32) -> (Area, Area) {
    let px = |v: i32| v * dpi as i32 / 96;
    let text_height = line.min(height).max(1);
    let gap = px(12).min((height - text_height).max(0));
    let icon_height = px(68).min((height - text_height - gap).max(0));
    let icon_width = (icon_height * 84 / 68).min(width);
    let top = (height - icon_height - gap - text_height) / 2;
    (
        ((width - icon_width) / 2, top, icon_width, icon_height),
        (0, top + icon_height + gap, width, text_height),
    )
}
// The scrolling host and the fixed footer share the main window's coordinates.
pub(super) fn content_height(height: i32, page: usize, footer_top: i32) -> i32 {
    (height - if page == 3 { 100 } else { 140 })
        .min(footer_top - 4 - 76)
        .max(1)
}
// Native control layout metrics, in device pixels.
pub(super) fn navigation_item_width(client_width: i32, dpi: u32, count: i32) -> i32 {
    // Leave space for the native tab border and selected-item overhang.
    ((client_width - 8 * dpi as i32 / 96) / count.max(1)).max(1)
}
pub(super) fn edit_inset(height: i32, line_height: i32, dpi: u32) -> (i32, i32) {
    let available = (height - line_height - 2 * dpi as i32 / 96).max(0);
    (available / 2, available - available / 2)
}
pub(super) fn reference_layout(columns: i32, _page: usize, _advanced: bool) -> bool {
    columns == 2
}
// Visibility is application state; a reference rectangle must not override it.
pub(super) fn state_allows(id: i32, has_rules: bool, advanced: bool) -> bool {
    if id == 118 {
        return !has_rules;
    }
    if id == 100 {
        return has_rules;
    }
    advanced || !matches!(id,202..=206|6202..=6206|209)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_hint_tracks_rule_lifecycle() {
        for has_rules in [false, true, true, false] {
            assert_eq!(state_allows(118, has_rules, false), !has_rules);
        }
        assert!(!state_allows(202, false, false));
        assert!(state_allows(202, true, true));
    }
    #[test]
    fn scrolling_host_stays_above_save_button() {
        for height in [420, 651, 800] {
            let c = Canvas {
                width: 1024,
                height,
                split: SPLIT,
            };
            let footer = c.control(113).unwrap().1;
            assert!(
                76 + content_height(height, 0, footer) <= footer - 4,
                "scrolling content overlaps save button at height {height}"
            );
        }
    }
    #[test]
    fn empty_group_is_centered_and_inside_the_list() {
        for dpi in [96, 120, 144, 192] {
            for (w, h) in [(270, 307), (400, 500), (200, 64)] {
                let (w, h, line) = (w * dpi / 96, h * dpi / 96, 24 * dpi / 96);
                let (icon, text) = empty_state(w, h, line, dpi as u32);
                assert!((icon.0 * 2 + icon.2 - w).abs() <= 1);
                assert_eq!(text.0 * 2 + text.2, w);
                assert!((icon.1 + text.1 + text.3 - h).abs() <= 1);
                assert!(icon.1 >= 0 && text.1 + text.3 <= h);
            }
        }
    }
    #[test]
    fn measured_cards_match_reference() {
        let c = Canvas {
            width: 1024,
            height: 651,
            split: SPLIT,
        };
        assert_eq!(c.cards(0)[0], (24, 5, 317, 471));
        assert_eq!(c.cards(3)[1], (46, 110, 460, 185));
        assert_eq!(c.control(504), Some((819, 171, 161, 35)));
        assert!(c.control(808).unwrap().1 + c.control(808).unwrap().3 <= 551);
    }
}

#[cfg(test)]
mod regression_tests {
    use super::*;
    #[test]
    fn six_navigation_items_fit_the_host_at_every_width() {
        for width in [960, 1024, 1280, 1536] {
            for dpi in [96, 120, 144, 192] {
                let host = Canvas {
                    width,
                    height: 651,
                    split: SPLIT,
                }
                .control(9)
                .unwrap()
                .2 * dpi
                    / 96;
                let item = navigation_item_width(host, dpi as u32, 6);
                assert!(
                    item * 6 + 8 * dpi / 96 <= host,
                    "navigation overflow at width {width}, DPI {dpi}"
                );
            }
        }
    }
    #[test]
    fn rest_edit_has_room_for_the_entire_font_line() {
        for dpi in [96, 120, 144, 192] {
            let height = 28 * dpi / 96;
            let line_height = 21 * dpi / 96;
            let (top, bottom) = edit_inset(height, line_height, dpi as u32);
            assert!(
                height - top - bottom >= line_height,
                "clipped rest edit at DPI {dpi}"
            );
        }
    }
    #[test]
    fn advanced_options_keep_the_same_layout_mode() {
        assert_eq!(reference_layout(2, 0, false), reference_layout(2, 0, true));
    }
}
