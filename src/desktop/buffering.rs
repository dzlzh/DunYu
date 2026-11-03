//! Window-tree redraw transactions and owned offscreen GDI resources.
use super::*;

pub(super) struct RedrawBatch {
    window: HWND,
    damage: HWND,
    active: bool,
    region: Option<RECT>,
}
impl RedrawBatch {
    pub(super) unsafe fn new(window: HWND, damage: HWND) -> Self {
        // The outer transaction clears WS_VISIBLE. Nested transactions and
        // originally hidden windows must never re-enable it themselves.
        let active = GetWindowLongPtrW(window, GWL_STYLE) as u32 & WS_VISIBLE != 0;
        if active {
            SendMessageW(window, WM_SETREDRAW, 0, 0);
        }
        Self {
            window,
            damage,
            active,
            region: None,
        }
    }
}
impl RedrawBatch {
    pub(super) unsafe fn region(window: HWND, region: RECT) -> Self {
        let mut batch = Self::new(window, window);
        batch.region = Some(region);
        batch
    }
}
impl Drop for RedrawBatch {
    fn drop(&mut self) {
        unsafe {
            if self.active && IsWindow(self.window) != 0 {
                SendMessageW(self.window, WM_SETREDRAW, 1, 0);
                RedrawWindow(
                    self.damage,
                    self.region.as_ref().map_or(null(), |r| r as *const RECT),
                    null_mut(),
                    RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN,
                );
            }
        }
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug)]
struct ReleaseOutcome {
    restored_bitmap: bool,
    delete_bitmap: BOOL,
    delete_dc: BOOL,
}

// Observe the owned allocation, not a stale GDI handle (whose type can remain
// queryable after deletion on Windows). No assertions run inside Drop/Win32 callbacks.
#[cfg(test)]
#[derive(Clone, Default)]
pub(super) struct ReleaseProbe(std::rc::Rc<std::cell::Cell<Option<ReleaseOutcome>>>);
#[cfg(test)]
impl ReleaseProbe {
    pub(super) fn assert_released(&self, context: &str) {
        let outcome = self
            .0
            .get()
            .unwrap_or_else(|| panic!("{context}: BackBuffer destructor did not run"));
        assert!(
            outcome.restored_bitmap,
            "{context}: SelectObject did not restore the original bitmap: {outcome:?}"
        );
        assert_ne!(
            outcome.delete_bitmap, 0,
            "{context}: DeleteObject(bitmap) failed: {outcome:?}"
        );
        assert_ne!(
            outcome.delete_dc, 0,
            "{context}: DeleteDC failed: {outcome:?}"
        );
    }
}

pub(super) struct BackBuffer {
    dc: HDC,
    bitmap: HBITMAP,
    original: HGDIOBJ,
    size: (i32, i32),
    #[cfg(test)]
    release_probe: Option<ReleaseProbe>,
}
impl BackBuffer {
    #[cfg(test)]
    pub(super) fn dc_for_test(&self) -> HDC {
        self.dc
    }

    #[cfg(test)]
    pub(super) fn observe_release(&mut self) -> ReleaseProbe {
        self.release_probe
            .get_or_insert_with(ReleaseProbe::default)
            .clone()
    }

    pub(super) unsafe fn prepare(
        cache: &mut Option<Self>,
        target: HDC,
        width: i32,
        height: i32,
    ) -> Option<HDC> {
        let size = (width.max(1), height.max(1));
        if let Some(buffer) = cache.as_ref().filter(|b| b.size == size) {
            return Some(buffer.dc);
        }
        #[cfg(test)]
        let _allocation = super::ui::diagnostics::measure("buffer.allocate");
        let dc = CreateCompatibleDC(target);
        if dc.is_null() {
            return None;
        }
        let bitmap = CreateCompatibleBitmap(target, size.0, size.1);
        if bitmap.is_null() {
            DeleteDC(dc);
            return None;
        }
        let original = SelectObject(dc, bitmap);
        if original.is_null() || original as isize == -1 {
            DeleteObject(bitmap);
            DeleteDC(dc);
            return None;
        }
        *cache = Some(Self {
            dc,
            bitmap,
            original,
            size,
            #[cfg(test)]
            release_probe: None,
        });
        Some(dc)
    }
}
impl Drop for BackBuffer {
    fn drop(&mut self) {
        unsafe {
            let _restored = SelectObject(self.dc, self.original);
            let _delete_bitmap = DeleteObject(self.bitmap);
            let _delete_dc = DeleteDC(self.dc);
            #[cfg(test)]
            if let Some(probe) = &self.release_probe {
                probe.0.set(Some(ReleaseOutcome {
                    restored_bitmap: _restored == self.bitmap,
                    delete_bitmap: _delete_bitmap,
                    delete_dc: _delete_dc,
                }));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_redraw_and_hidden_windows_keep_visibility() {
        unsafe {
            let h = CreateWindowExW(
                WS_EX_COMPOSITED,
                wide("STATIC").as_ptr(),
                wide("").as_ptr(),
                WS_POPUP,
                0,
                0,
                100,
                100,
                null_mut(),
                null_mut(),
                GetModuleHandleW(null()),
                null(),
            );
            assert!(!h.is_null());
            drop(RedrawBatch::new(h, h));
            assert_eq!(GetWindowLongPtrW(h, GWL_STYLE) as u32 & WS_VISIBLE, 0);
            ShowWindow(h, SW_SHOWNOACTIVATE);
            {
                let _outer = RedrawBatch::new(h, h);
                {
                    let _inner = RedrawBatch::new(h, h);
                }
                assert_eq!(GetWindowLongPtrW(h, GWL_STYLE) as u32 & WS_VISIBLE, 0);
            }
            assert_ne!(GetWindowLongPtrW(h, GWL_STYLE) as u32 & WS_VISIBLE, 0);
            let _ = std::panic::catch_unwind(|| {
                let _batch = RedrawBatch::new(h, h);
                panic!("test cleanup");
            });
            assert_ne!(GetWindowLongPtrW(h, GWL_STYLE) as u32 & WS_VISIBLE, 0);
            DestroyWindow(h);
        }
    }
    #[test]
    fn buffer_reuses_size_replaces_and_releases_gdi_objects() {
        unsafe {
            let screen = GetDC(null_mut());
            assert!(!screen.is_null());
            let mut cache = None;
            let first = BackBuffer::prepare(&mut cache, screen, 80, 40).unwrap();
            let bitmap = cache.as_ref().unwrap().bitmap;
            let replaced = cache.as_mut().unwrap().observe_release();
            assert_eq!(BackBuffer::prepare(&mut cache, screen, 80, 40), Some(first));
            assert_eq!(cache.as_ref().unwrap().bitmap, bitmap);
            assert!(
                replaced.0.get().is_none(),
                "same-size reuse released a live buffer"
            );
            assert_ne!(
                BackBuffer::prepare(&mut cache, screen, 120, 60).unwrap(),
                first
            );
            let final_buffer = cache.as_mut().unwrap().observe_release();
            assert!(final_buffer.0.get().is_none());
            drop(cache);
            ReleaseDC(null_mut(), screen);
            replaced.assert_released("buffer replaced on resize");
            final_buffer.assert_released("final buffer dropped");
        }
    }
}
