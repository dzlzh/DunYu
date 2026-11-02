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
            include_bytes!("../../resources/fonts/SourceHanSerifSC-Regular.otf").as_slice(),
            include_bytes!("../../resources/fonts/SourceHanSerifSC-SemiBold.otf").as_slice(),
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
