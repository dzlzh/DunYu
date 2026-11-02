//! Geometry shared by the progress ring, fish and reference-canvas layout.
use std::f32::consts::{FRAC_PI_2, TAU};
pub(super) fn progress(duration: u64, remaining: u64, elapsed: u64) -> f32 {
    if duration == 0 {
        return 0.;
    }
    (1. - remaining.saturating_sub(elapsed) as f64 / duration as f64).clamp(0., 1.) as f32
}
pub(super) fn orbit(progress: f32) -> (f32, f32, f32) {
    let angle = -FRAC_PI_2 + progress.clamp(0., 1.) * TAU;
    (angle.cos(), angle.sin(), angle.to_degrees() + 90.)
}
pub(super) fn contain(source: [f32; 2], dest: [f32; 4]) -> [f32; 4] {
    let scale = (dest[2].max(0.) / source[0].max(1.)).min(dest[3].max(0.) / source[1].max(1.));
    let (w, h) = (source[0] * scale, source[1] * scale);
    [
        dest[0] + (dest[2] - w) / 2.,
        dest[1] + (dest[3] - h) / 2.,
        w,
        h,
    ]
}
#[derive(Clone, Copy)]
pub(super) struct Layout {
    pub scale: f32,
    pub x: f32,
    pub y: f32,
}
impl Layout {
    pub fn new(width: f32, height: f32) -> Self {
        let scale = (width / 1672.).min(height / 941.);
        Self {
            scale,
            x: (width - 1672. * scale) / 2.,
            y: (height - 941. * scale) / 2.,
        }
    }
    pub fn rect(self, x: f32, y: f32, w: f32, h: f32) -> [f32; 4] {
        [
            self.x + x * self.scale,
            self.y + y * self.scale,
            w * self.scale,
            h * self.scale,
        ]
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn progress_and_orbit_follow_each_duration() {
        for duration in [5000, 20000, 300000] {
            for (part, x, y) in [
                (0, 0., -1.),
                (1, 1., 0.),
                (2, 0., 1.),
                (3, -1., 0.),
                (4, 0., -1.),
            ] {
                let p = progress(duration, duration - duration * part / 4, 0);
                let (a, b, _) = orbit(p);
                assert!((a - x).abs() < 0.00001 && (b - y).abs() < 0.00001);
            }
        }
        assert_eq!(progress(0, 0, 0), 0.);
        assert_eq!(progress(5000, 9000, 0), 0.);
        assert_eq!(progress(5000, 100, 500), 1.);
    }
    #[test]
    fn artwork_preserves_ratio_and_stays_inside_slot() {
        for (w, h) in [
            (250., 200.),
            (290., 170.),
            (310., 170.),
            (250., 230.),
            (290., 190.),
            (280., 230.),
        ] {
            for scale in [1., 1.25, 1.5, 2.] {
                let r = contain([w, h], [0., 0., 36. * scale, 28. * scale]);
                assert!((r[2] / w - r[3] / h).abs() < 0.00001);
                assert!(
                    r[0] >= 0.
                        && r[1] >= 0.
                        && r[0] + r[2] <= 36. * scale + 0.001
                        && r[1] + r[3] <= 28. * scale + 0.001
                );
            }
        }
    }
    #[test]
    fn ring_is_round_on_every_monitor() {
        for (w, h) in [
            (1672., 941.),
            (1920., 1080.),
            (3440., 1440.),
            (1080., 1920.),
        ] {
            let r = Layout::new(w, h).rect(661., 261., 350., 350.);
            assert_eq!(r[2], r[3]);
            assert!((r[0] + r[2] / 2. - w / 2.).abs() < 0.01);
        }
    }
}
