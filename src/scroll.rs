//! Pixel scrolling with retained fractional wheel input and a finite ease-out.
#[derive(Default, Debug)]
pub struct Motion {
    pub current: f64,
    pub target: f64,
    start: f64,
    began: u64,
}
impl Motion {
    pub fn snap(&mut self, value: f64) {
        self.current = value;
        self.target = value;
        self.start = value;
    }
    pub fn tick(&mut self, now: u64) -> bool {
        let t = (now.saturating_sub(self.began) as f64 / 160.).min(1.);
        self.current = self.start + (self.target - self.start) * (1. - (1. - t).powi(3));
        if (self.target - self.current).abs() < 0.01 {
            self.current = self.target;
        }
        self.current != self.target
    }
    pub fn add(&mut self, delta: f64, max: f64, now: u64, animate: bool) {
        self.tick(now);
        self.start = self.current;
        self.target = (self.target + delta).clamp(0., max.max(0.));
        self.began = now;
        if !animate {
            self.snap(self.target);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fractions_reversal_and_edges() {
        let mut s = Motion::default();
        for _ in 0..8 {
            s.add(9., 1000., 0, true);
        }
        assert_eq!(s.target, 72.);
        assert!(s.tick(80));
        assert!(s.current > 0. && s.current < 72.);
        s.add(-40., 1000., 80, true);
        s.tick(240);
        assert_eq!(s.current, 32.);
        s.add(-100., 1000., 240, true);
        s.tick(400);
        assert_eq!(s.current, 0.);
        s.add(2000., 100., 400, false);
        assert_eq!(s.current, 100.);
    }
}
