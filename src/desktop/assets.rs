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
    static MAIN: RefCell<Option<Image>> = const { RefCell::new(None) };
    static FULL: RefCell<Option<Image>> = const { RefCell::new(None) };
    static CARD: RefCell<Option<Image>> = const { RefCell::new(None) };
}
// Render clean source regions directly, retaining the original grain and colour.
pub(super) unsafe fn background(dc: HDC, width: i32, height: i32, scene: i32) {
    let (cache, bytes, source): (_, &[u8], _) = match scene {
        1 => (
            &FULL,
            include_bytes!("../../docs/design/rest-preview-fullscreen-static.png"),
            [20., 240., 400., 400.],
        ),
        2 => (
            &CARD,
            include_bytes!("../../docs/design/rest-preview-card.png"),
            [80., 190., 300., 250.],
        ),
        3 => (
            &MAIN,
            include_bytes!("../../docs/design/ui-settings.png"),
            [400., 285., 500., 90.],
        ),
        _ => (
            &MAIN,
            include_bytes!("../../docs/design/ui-settings.png"),
            [40., 720., 400., 180.],
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
                    [1332., 0., 250., 220.],
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
                    [1582., 90., 90., 130.],
                    false,
                );
                image.draw(
                    g,
                    [0., height as f32 - 200. * scale, 340. * scale, 200. * scale],
                    [0., 741., 340., 200.],
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
            Image::load(include_bytes!(
                "../../docs/design/rest-preview-fullscreen-static.png"
            ))
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
        image.draw(g, dest, [661., 261., 350., 350.], false);
        GdipDeletePath(path);
        GdipRestoreGraphics(g, state);
    });
}
// The original dry-brush underline; colour-key only removes its dark paper.
pub(super) unsafe fn stroke(dc: HDC, r: RECT, red: bool) {
    MAIN.with(|cache| {
        let mut cache = cache.borrow_mut();
        let image = cache.get_or_insert_with(|| {
            Image::load(include_bytes!("../../docs/design/ui-settings.png"))
        });
        let mut g = null_mut();
        if GdipCreateFromHDC(dc, &mut g) == 0 {
            let source = if red {
                [1345., 131., 151., 13.]
            } else {
                [42., 232., 153., 14.]
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
