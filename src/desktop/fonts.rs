//! Process-private embedded fonts; no changes to the user's installed fonts.
use super::*;
struct Fonts(Vec<HANDLE>);
impl Drop for Fonts {
    fn drop(&mut self) {
        unsafe {
            for handle in &self.0 {
                RemoveFontMemResourceEx(*handle);
            }
        }
    }
}
thread_local! { static FONTS: RefCell<Option<Fonts>> = const { RefCell::new(None) }; }
pub(super) unsafe fn ensure() {
    FONTS.with(|cache| {
        if cache.borrow().is_some() {
            return;
        }
        let mut handles = Vec::new();
        for bytes in [
            include_bytes!("../../resources/optimized/DunYuSerifSC-Regular.otf").as_slice(),
            include_bytes!("../../resources/optimized/DunYuSerifSC-SemiBold.otf").as_slice(),
        ] {
            let mut count = 0;
            let handle =
                AddFontMemResourceEx(bytes.as_ptr() as _, bytes.len() as u32, null(), &mut count);
            if !handle.is_null() {
                handles.push(handle);
            }
        }
        *cache.borrow_mut() = Some(Fonts(handles));
    });
}

pub(super) const FACE: &str = "DunYu Serif SC";
mod subset {
    include!("../../resources/optimized/coverage.rs");
}
fn covered(text: &str) -> bool {
    text.chars().all(|c| {
        c.is_control()
            // These UI triangles were already absent from the original font;
            // retain the existing vector/system rendering for fixed captions.
            || matches!(c, '\u{25b4}' | '\u{25b8}' | '\u{25be}')
            || subset::COVERAGE
                .binary_search_by(|&(start, end)| {
                    if (c as u32) < start {
                        std::cmp::Ordering::Greater
                    } else if (c as u32) > end {
                        std::cmp::Ordering::Less
                    } else {
                        std::cmp::Ordering::Equal
                    }
                })
                .is_ok()
    })
}

/// Select the same face for measuring and drawing dynamic labels. A whole label
/// with characters outside our subset uses the existing system UI font, whose
/// Windows font linking remains available. No machine font settings are changed.
pub(super) struct Selection {
    dc: HDC,
    previous: HGDIOBJ,
    temporary: HFONT,
}
impl Drop for Selection {
    fn drop(&mut self) {
        unsafe {
            if !self.previous.is_null() {
                SelectObject(self.dc, self.previous);
            }
            if !self.temporary.is_null() {
                DeleteObject(self.temporary);
            }
        }
    }
}
pub(super) unsafe fn select(dc: HDC, h: HWND, text: &str) -> Selection {
    let original = SendMessageW(h, WM_GETFONT, 0, 0) as HFONT;
    let mut selected = original;
    let mut temporary = null_mut();
    if !covered(text) {
        let mut lf: LOGFONTW = std::mem::zeroed();
        if GetObjectW(
            original,
            std::mem::size_of::<LOGFONTW>() as i32,
            &mut lf as *mut _ as _,
        ) != 0
        {
            let len = lf.lfFaceName.iter().position(|&c| c == 0).unwrap_or(32);
            if String::from_utf16_lossy(&lf.lfFaceName[..len]).starts_with(FACE) {
                lf.lfFaceName = [0; 32];
                let face = wide("Microsoft YaHei UI");
                lf.lfFaceName[..face.len()].copy_from_slice(&face);
                temporary = CreateFontIndirectW(&lf);
                if !temporary.is_null() {
                    selected = temporary;
                }
            }
        }
    }
    Selection {
        dc,
        previous: SelectObject(dc, selected),
        temporary,
    }
}

#[cfg(test)]
pub(super) fn clear_test_cache() {
    let fonts = FONTS.with(|cache| cache.borrow_mut().take());
    drop(fonts);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn embedded_faces_and_dynamic_fallback() {
        assert!(covered("遁鱼 休憩：20 分钟 · Ctrl+Alt+1"));
        let rare = char::from_u32(0x9f98).unwrap().to_string();
        assert!(!covered(&rare));
        unsafe {
            let _graphics = theme::TestGraphicsScope::new();
            ensure();
            let h = CreateWindowExW(
                0,
                wide("STATIC").as_ptr(),
                wide("字体").as_ptr(),
                0,
                0,
                0,
                100,
                30,
                null_mut(),
                null_mut(),
                GetModuleHandleW(null()),
                null(),
            );
            assert!(!h.is_null());
            let _window = theme::TestWindow(h);
            let dc = GetDC(h);
            for weight in [400, 600] {
                let mut lf: LOGFONTW = std::mem::zeroed();
                lf.lfHeight = -20;
                lf.lfWeight = weight;
                let face = wide(FACE);
                lf.lfFaceName[..face.len()].copy_from_slice(&face);
                let font = CreateFontIndirectW(&lf);
                assert!(!font.is_null());
                SendMessageW(h, WM_SETFONT, font as usize, 0);
                for (label, expected) in [("遁鱼", FACE), (rare.as_str(), "Microsoft YaHei UI")] {
                    let _selection = select(dc, h, label);
                    let mut actual = [0u16; 64];
                    let len = GetTextFaceW(dc, 64, actual.as_mut_ptr());
                    assert!(len > 0);
                    let end = actual.iter().position(|&c| c == 0).unwrap_or(actual.len());
                    assert!(String::from_utf16_lossy(&actual[..end]).starts_with(expected));
                    let text = wide(label);
                    let mut glyphs = vec![0u16; text.len() - 1];
                    GetGlyphIndicesW(
                        dc,
                        text.as_ptr(),
                        glyphs.len() as i32,
                        glyphs.as_mut_ptr(),
                        GGI_MARK_NONEXISTING_GLYPHS,
                    );
                    assert!(!glyphs.contains(&0xffff));
                }
                SendMessageW(h, WM_SETFONT, 0, 0);
                DeleteObject(font);
            }
            ReleaseDC(h, dc);
            DestroyWindow(h);
        }
    }
}
