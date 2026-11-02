//! Design-provided natural swim cycle, following the countdown progress.
use super::*;
use windows_sys::Win32::Graphics::GdiPlus::*;
pub(super) unsafe fn koi(
    g: *mut GpGraphics,
    center: (f32, f32),
    scale: f32,
    progress: f32,
    elapsed: u64,
    moving: bool,
) {
    let (x, y, heading) = super::swim::orbit(progress);
    let mut saved = 0;
    GdipSaveGraphics(g, &mut saved);
    GdipTranslateWorldTransform(
        g,
        center.0 + x * 175. * scale,
        center.1 + y * 175. * scale,
        MatrixOrderPrepend,
    );
    GdipRotateWorldTransform(g, heading + 90., MatrixOrderPrepend);
    GdipScaleWorldTransform(g, scale, scale, MatrixOrderPrepend);
    assets::koi(g, elapsed, moving);
    GdipRestoreGraphics(g, saved);
}

#[cfg(test)]
mod tests {
    use super::*;
    unsafe fn frame(ms: u64, moving: bool) -> Vec<u8> {
        frame_at(
            ms,
            moving,
            super::swim::progress(12000, 12000 - ms.min(12000), 0),
        )
    }
    unsafe fn frame_at(ms: u64, moving: bool, progress: f32) -> Vec<u8> {
        let _theme = theme::Theme::new();
        let mut pixels = vec![0u8; 400 * 400 * 4];
        let mut bitmap = null_mut();
        // GDI+ PixelFormat32bppARGB: alpha, GDI, canonical, 32 bits, index 10.
        assert_eq!(
            GdipCreateBitmapFromScan0(400, 400, 1600, 0x26200a, pixels.as_mut_ptr(), &mut bitmap),
            0
        );
        let mut g = null_mut();
        assert_eq!(GdipGetImageGraphicsContext(bitmap as _, &mut g), 0);
        GdipGraphicsClear(g, 0xff061e1d);
        koi(g, (200., 200.), 0.8, progress, ms, moving);
        GdipDeleteGraphics(g);
        GdipDisposeImage(bitmap as _);
        pixels
    }
    #[test]
    fn rendered_frames_move_loop_and_freeze() {
        unsafe {
            let a = frame(0, true);
            assert!(a.chunks_exact(4).any(|p| p[..3] != [0x1d, 0x1e, 0x06]));
            assert_ne!(a, frame(300, true));
            assert_ne!(a, frame(6000, true));
            assert_eq!(frame_at(0, false, 0.5), frame_at(5000, false, 0.5));
            if std::env::var_os("DUNYU_UI_SCREENSHOTS").is_some() {
                std::fs::create_dir_all("target/ui-preview").unwrap();
                for i in 0..12 {
                    let pixels = frame(i * 100, true);
                    let mut bmp = vec![0u8; 54];
                    bmp[..2].copy_from_slice(b"BM");
                    bmp[2..6].copy_from_slice(&(54 + pixels.len() as u32).to_le_bytes());
                    bmp[10..14].copy_from_slice(&54u32.to_le_bytes());
                    bmp[14..18].copy_from_slice(&40u32.to_le_bytes());
                    bmp[18..22].copy_from_slice(&400i32.to_le_bytes());
                    bmp[22..26].copy_from_slice(&(-400i32).to_le_bytes());
                    bmp[26..28].copy_from_slice(&1u16.to_le_bytes());
                    bmp[28..30].copy_from_slice(&32u16.to_le_bytes());
                    bmp.extend_from_slice(&pixels);
                    std::fs::write(format!("target/ui-preview/koi-motion-{i:02}.bmp"), bmp)
                        .unwrap();
                }
            }
        }
    }
}
