//! Icon-derived sprite, positioned by the actual break progress.
use super::*;
use windows_sys::Win32::Graphics::GdiPlus::*;
struct Sprite {
    image: *mut GpImage,
    stream: *mut std::ffi::c_void,
    _theme: std::rc::Rc<theme::Theme>,
}
impl Drop for Sprite {
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
thread_local! { static SPRITE: RefCell<Option<Sprite>> = const { RefCell::new(None) }; }
pub(super) fn orbit(progress: f32) -> (f32, f32, f32) {
    let angle = -std::f32::consts::FRAC_PI_2 + progress.clamp(0., 1.) * std::f32::consts::TAU;
    (angle.cos(), angle.sin(), angle.to_degrees() + 90.)
}
pub(super) unsafe fn koi(g: *mut GpGraphics, center: (f32, f32), scale: f32, progress: f32) {
    SPRITE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let sprite = cache.get_or_insert_with(|| {
            let token = theme::Theme::new();
            let bytes = include_bytes!("../../resources/koi-swim.png");
            let stream = SHCreateMemStream(bytes.as_ptr(), bytes.len() as u32);
            let mut image = null_mut();
            if !stream.is_null() {
                GdipLoadImageFromStream(stream, &mut image);
            }
            Sprite {
                image,
                stream,
                _theme: token,
            }
        });
        if sprite.image.is_null() {
            return;
        }
        let (x, y, heading) = orbit(progress);
        let mut saved = 0;
        GdipSaveGraphics(g, &mut saved);
        GdipTranslateWorldTransform(
            g,
            center.0 + x * 130. * scale,
            center.1 + y * 130. * scale,
            MatrixOrderPrepend,
        );
        GdipRotateWorldTransform(g, heading + 40., MatrixOrderPrepend);
        GdipSetInterpolationMode(g, InterpolationModeHighQualityBicubic);
        let height = 64. * scale;
        let mut source_width = 0;
        let mut source_height = 0;
        GdipGetImageWidth(sprite.image, &mut source_width);
        GdipGetImageHeight(sprite.image, &mut source_height);
        // The neutral cell is bent continuously, avoiding abrupt pose/frame switches.
        let cell_width = source_width as f32 / 4.;
        let width = height * cell_width / source_height.max(1) as f32;
        let now = windows_sys::Win32::System::SystemInformation::GetTickCount64();
        let moving = rest_ui::motion_enabled();
        let time = if moving {
            (now % 1800) as f32 / 1800. * std::f32::consts::TAU
        } else {
            0.
        };
        let bend = |v: f32| {
            if !moving {
                return 0.;
            }
            let tail = ((v - 0.30) / 0.70).clamp(0., 1.);
            (time - tail * 2.5).sin() * tail * tail * 7. * scale
        };
        for band in 0..24 {
            let a = band as f32 / 24.;
            let b = (band + 1) as f32 / 24.;
            let x = -0.88 * width;
            let y = -0.24 * height;
            let points = [
                PointF {
                    X: x + bend(a),
                    Y: y + a * height,
                },
                PointF {
                    X: x + width + bend(a),
                    Y: y + a * height,
                },
                PointF {
                    X: x + bend(b),
                    Y: y + b * height + 0.25,
                },
            ];
            GdipDrawImagePointsRect(
                g,
                sprite.image,
                points.as_ptr(),
                3,
                0.,
                a * source_height as f32,
                cell_width,
                (b - a) * source_height as f32,
                UnitPixel,
                null(),
                0,
                null_mut(),
            );
        }
        GdipRestoreGraphics(g, saved);
    });
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn progress_uses_cardinal_points() {
        for (p, x, y) in [
            (0., 0., -1.),
            (0.25, 1., 0.),
            (0.5, 0., 1.),
            (0.75, -1., 0.),
            (1., 0., -1.),
        ] {
            let (actual_x, actual_y, _) = orbit(p);
            assert!((actual_x - x).abs() < 0.00001 && (actual_y - y).abs() < 0.00001);
        }
    }
}
