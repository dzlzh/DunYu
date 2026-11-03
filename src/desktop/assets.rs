//! Embedded design artwork; decoded once and shared across desktop windows.
use super::*;
mod packed {
    include!("../../resources/optimized/regions.rs");
}
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
                #[cfg(test)]
                eprintln!("[graphics-cleanup] GdipDisposeImage begin");
                GdipDisposeImage(self.image);
                #[cfg(test)]
                eprintln!("[graphics-cleanup] GdipDisposeImage end");
            }
            if !self.stream.is_null() {
                #[cfg(test)]
                eprintln!("[graphics-cleanup] image stream Release begin");
                let table = *(self.stream as *mut *mut usize);
                let release: unsafe extern "system" fn(*mut std::ffi::c_void) -> u32 =
                    std::mem::transmute(*table.add(2));
                release(self.stream);
                #[cfg(test)]
                eprintln!("[graphics-cleanup] image stream Release end");
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
        packed::NAV_0,
        packed::NAV_1,
        packed::NAV_2,
        packed::NAV_3,
        packed::NAV_4,
        packed::NAV_5,
    ];
    let Some(source) = sources.get(index) else {
        return;
    };
    NAV.with(|cache| {
        let mut cache = cache.borrow_mut();
        let image = cache.get_or_insert_with(|| {
            Image::load(include_bytes!("../../resources/optimized/nav.png"))
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
    static MAIN: RefCell<Option<Image>> = const { RefCell::new(None) };
    static FULL: RefCell<Option<Image>> = const { RefCell::new(None) };
    static CARD: RefCell<Option<Image>> = const { RefCell::new(None) };
}
// Render clean source regions directly, retaining the original grain and colour.
pub(super) unsafe fn background(dc: HDC, width: i32, height: i32, scene: i32) {
    let (cache, bytes, source): (_, &[u8], _) = match scene {
        1 => (
            &FULL,
            include_bytes!("../../resources/optimized/full.png"),
            packed::FULL_PAPER,
        ),
        2 => (
            &CARD,
            include_bytes!("../../resources/optimized/card.png"),
            packed::CARD_PAPER,
        ),
        3 => (
            &MAIN,
            include_bytes!("../../resources/optimized/main.png"),
            packed::MAIN_SURFACE,
        ),
        _ => (
            &MAIN,
            include_bytes!("../../resources/optimized/main.png"),
            packed::MAIN_PAPER,
        ),
    };
    cache.with(|cache| {
        let mut cache = cache.borrow_mut();
        let image = cache.get_or_insert_with(|| Image::load(bytes));
        let mut g = null_mut();
        if GdipCreateFromHDC(dc, &mut g) == 0 {
            let mut y = 0.;
            while y < height as f32 {
                let mut x = 0.;
                while x < width as f32 {
                    image.draw(g, [x, y, source[2], source[3]], source, false);
                    x += source[2];
                }
                y += source[3];
            }
            if scene == 1 {
                let scale = (width as f32 / 1672.).min(height as f32 / 941.);
                image.draw(
                    g,
                    [width as f32 - 340. * scale, 0., 250. * scale, 220. * scale],
                    packed::FULL_TOP,
                    false,
                );
                image.draw(
                    g,
                    [
                        width as f32 - 90. * scale,
                        90. * scale,
                        90. * scale,
                        130. * scale,
                    ],
                    packed::FULL_RIGHT,
                    false,
                );
                image.draw(
                    g,
                    [0., height as f32 - 200. * scale, 340. * scale, 200. * scale],
                    packed::FULL_BOTTOM,
                    false,
                );
            }
            GdipDeleteGraphics(g);
        }
    });
}
pub(super) unsafe fn ring(g: *mut GpGraphics, dest: [f32; 4]) {
    FULL.with(|cache| {
        let mut cache = cache.borrow_mut();
        let image = cache.get_or_insert_with(|| {
            Image::load(include_bytes!("../../resources/optimized/full.png"))
        });
        let mut state = 0;
        GdipSaveGraphics(g, &mut state);
        let mut path = null_mut();
        GdipCreatePath(FillModeAlternate, &mut path);
        GdipAddPathEllipse(path, dest[0], dest[1], dest[2], dest[3]);
        let inset = dest[2] * 0.055;
        GdipAddPathEllipse(
            path,
            dest[0] + inset,
            dest[1] + inset,
            dest[2] - 2. * inset,
            dest[3] - 2. * inset,
        );
        GdipSetClipPath(g, path, CombineModeIntersect);
        image.draw(g, dest, packed::RING, false);
        GdipDeletePath(path);
        GdipRestoreGraphics(g, state);
    });
}
// The original dry-brush underline; colour-key only removes its dark paper.
pub(super) unsafe fn stroke(dc: HDC, r: RECT, red: bool) {
    MAIN.with(|cache| {
        let mut cache = cache.borrow_mut();
        let image = cache.get_or_insert_with(|| {
            Image::load(include_bytes!("../../resources/optimized/main.png"))
        });
        let mut g = null_mut();
        if GdipCreateFromHDC(dc, &mut g) == 0 {
            let source = if red {
                packed::STROKE_RED
            } else {
                packed::STROKE_INK
            };
            image.draw(
                g,
                [
                    r.left as f32,
                    r.top as f32,
                    (r.right - r.left) as f32,
                    (r.bottom - r.top) as f32,
                ],
                source,
                true,
            );
            GdipDeleteGraphics(g);
        }
    });
}

// Drop cached images/streams while ordinary thread execution is still active.
#[cfg(test)]
pub(super) fn clear_test_cache() {
    for (name, cache) in [
        ("NAV", &NAV),
        ("KOI", &KOI),
        ("MAIN", &MAIN),
        ("FULL", &FULL),
        ("CARD", &CARD),
    ] {
        eprintln!("[graphics-cleanup] image {name} begin");
        let image = cache.with(|cache| cache.borrow_mut().take());
        drop(image);
        eprintln!("[graphics-cleanup] image {name} end");
    }
}
