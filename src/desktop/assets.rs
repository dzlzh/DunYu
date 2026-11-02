//! Embedded design artwork; decoded once and shared across desktop windows.
use super::*;
use windows_sys::Win32::Graphics::GdiPlus::*;
struct Image {
    image: *mut GpImage,
    stream: *mut std::ffi::c_void,
    _theme: std::rc::Rc<theme::Theme>,
}
impl Image {
    unsafe fn load(bytes: &[u8]) -> Self {
        let theme = theme::Theme::new();
        let stream = SHCreateMemStream(bytes.as_ptr(), bytes.len() as u32);
        let mut image = null_mut();
        if !stream.is_null() {
            GdipLoadImageFromStream(stream, &mut image);
        }
        Self {
            image,
            stream,
            _theme: theme,
        }
    }
    unsafe fn draw(&self, g: *mut GpGraphics, dest: [f32; 4], source: [f32; 4], key: bool) {
        if self.image.is_null() {
            return;
        }
        let mut attributes = null_mut();
        if key {
            GdipCreateImageAttributes(&mut attributes);
            if !attributes.is_null() {
                GdipSetImageAttributesColorKeys(
                    attributes,
                    ColorAdjustTypeBitmap,
                    1,
                    0xff000000,
                    0xff404040,
                );
            }
        }
        GdipSetInterpolationMode(g, InterpolationModeHighQualityBicubic);
        let dest = super::swim::contain([source[2], source[3]], dest);
        GdipDrawImageRectRect(
            g,
            self.image,
            dest[0],
            dest[1],
            dest[2],
            dest[3],
            source[0],
            source[1],
            source[2],
            source[3],
            UnitPixel,
            attributes,
            0,
            null_mut(),
        );
        if !attributes.is_null() {
            GdipDisposeImageAttributes(attributes);
        }
    }
}
impl Drop for Image {
    fn drop(&mut self) {
        unsafe {
            if !self.image.is_null() {
                GdipDisposeImage(self.image);
            }
            if !self.stream.is_null() {
                let table = *(self.stream as *mut *mut usize);
                let release: unsafe extern "system" fn(*mut std::ffi::c_void) -> u32 =
                    std::mem::transmute(*table.add(2));
                release(self.stream);
            }
        }
    }
}
thread_local! {
    static NAV: RefCell<Option<Image>> = const { RefCell::new(None) };
    static KOI: RefCell<Option<Image>> = const { RefCell::new(None) };
}
pub(super) unsafe fn navigation(dc: HDC, r: RECT, index: usize) {
    let sources = [
        [230., 150., 250., 200.],
        [630., 180., 290., 170.],
        [1040., 180., 310., 170.],
        [230., 470., 250., 230.],
        [630., 510., 290., 190.],
        [1060., 480., 280., 230.],
    ];
    let Some(source) = sources.get(index) else {
        return;
    };
    NAV.with(|cache| {
        let mut cache = cache.borrow_mut();
        let image = cache.get_or_insert_with(|| {
            Image::load(include_bytes!("../../resources/nav-icons-ink.png"))
        });
        let mut g = null_mut();
        if GdipCreateFromHDC(dc, &mut g) == 0 {
            image.draw(
                g,
                [
                    r.left as f32,
                    r.top as f32,
                    (r.right - r.left) as f32,
                    (r.bottom - r.top) as f32,
                ],
                *source,
                true,
            );
            GdipDeleteGraphics(g);
        }
    });
}
pub(super) unsafe fn koi(g: *mut GpGraphics, elapsed: u64, moving: bool) {
    KOI.with(|cache| {
        let mut cache = cache.borrow_mut();
        let image = cache.get_or_insert_with(|| {
            Image::load(include_bytes!("../../resources/koi-natural-frames.png"))
        });
        let frame = if moving { (elapsed % 2000) / 40 } else { 0 };
        image.draw(
            g,
            [-48., -48., 96., 96.],
            [0., frame as f32 * 160., 160., 160.],
            false,
        );
    });
}
thread_local! {
    static PAPER: RefCell<Option<Image>> = const { RefCell::new(None) };
    static RING: RefCell<Option<Image>> = const { RefCell::new(None) };
}
pub(super) unsafe fn background(dc: HDC, width: i32, height: i32) {
    PAPER.with(|cache| {
        let mut cache = cache.borrow_mut();
        let image = cache
            .get_or_insert_with(|| Image::load(include_bytes!("../../resources/paper-teal.png")));
        let mut g = null_mut();
        if GdipCreateFromHDC(dc, &mut g) == 0 {
            let scale = (width as f32 / 1671.).max(height as f32 / 941.);
            image.draw(
                g,
                [
                    (width as f32 - 1671. * scale) / 2.,
                    (height as f32 - 941. * scale) / 2.,
                    1671. * scale,
                    941. * scale,
                ],
                [0., 0., 1671., 941.],
                false,
            );
            GdipDeleteGraphics(g);
        }
    });
}
pub(super) unsafe fn ring(g: *mut GpGraphics, dest: [f32; 4]) {
    RING.with(|cache| {
        let mut cache = cache.borrow_mut();
        let image = cache
            .get_or_insert_with(|| Image::load(include_bytes!("../../resources/ink-ring.png")));
        image.draw(g, dest, [118., 140., 1000., 1000.], false);
    });
}
