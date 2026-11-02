//! Clock-driven break scheduling; no Windows APIs or wall-clock dependencies.
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct RestConfig {
    pub enabled: bool,
    pub short_enabled: bool,
    pub long_enabled: bool,
    pub long_interval_minutes: Option<u32>,
    pub interval_minutes: u32,
    pub short_seconds: u32,
    pub long_minutes: u32,
    pub long_every: u32,
    pub fullscreen: bool,
    pub short_auto_start: bool,
    pub long_auto_start: bool,
    pub natural_breaks: bool,
    pub defer_fullscreen: bool,
}
impl Default for RestConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            short_enabled: true,
            long_enabled: true,
            long_interval_minutes: None,
            interval_minutes: 20,
            short_seconds: 20,
            long_minutes: 5,
            long_every: 3,
            fullscreen: false,
            short_auto_start: true,
            long_auto_start: true,
            natural_breaks: true,
            defer_fullscreen: true,
        }
    }
}
impl RestConfig {
    pub fn active(&self) -> bool {
        self.enabled && (self.short_enabled || self.long_enabled)
    }
    // Old configurations are interpreted once as a time interval, never as completed rounds.
    pub fn long_interval(&self) -> u32 {
        self.long_interval_minutes.unwrap_or_else(|| {
            self.interval_minutes
                .saturating_mul(self.long_every)
                .clamp(1, 1440)
        })
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.interval_minutes == 0
            || self.interval_minutes > 1440
            || self.short_seconds == 0
            || self.short_seconds > 3600
            || self.long_minutes == 0
            || self.long_minutes > 120
            || self.long_interval() == 0
            || self.long_interval() > 1440
        {
            return Err(
                "休憩间隔为1～1440分钟，小憩为1～3600秒，舒展为1～120分钟，舒展间隔为1～1440分钟"
                    .into(),
            );
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Disabled,
    Working,
    Due,
    Warning,
    Awaiting,
    Resting,
    Snoozed,
    Paused,
}
#[derive(Clone, Debug)]
pub struct RestScheduler {
    pub config: RestConfig,
    pub phase: Phase,
    pub remaining_ms: u64,
    pub round: u32,
    pub resting_left: Option<u64>,
    last: u64,
    pause_until: u64,
    away_since: Option<u64>,
    blocked: bool,
    clocks: [u64; 2],
    active_long: bool,
}
impl RestScheduler {
    pub fn new(config: RestConfig, now: u64) -> Self {
        let mut s = Self {
            config,
            phase: Phase::Disabled,
            remaining_ms: 0,
            round: 0,
            resting_left: None,
            last: now,
            pause_until: 0,
            away_since: None,
            blocked: false,
            clocks: [0; 2],
            active_long: false,
        };
        s.restart(now);
        s
    }
    fn intervals(&self) -> [u64; 2] {
        [
            if self.config.short_enabled {
                self.config.interval_minutes as u64 * 60_000
            } else {
                u64::MAX
            },
            if self.config.long_enabled {
                self.config.long_interval() as u64 * 60_000
            } else {
                u64::MAX
            },
        ]
    }
    fn select(&mut self) {
        let next = self.clocks[1] <= self.clocks[0];
        if next != self.active_long {
            self.resting_left = None;
        }
        self.active_long = next;
        self.remaining_ms = self.clocks[usize::from(self.active_long)];
    }
    pub fn long(&self) -> bool {
        self.active_long
    }
    pub fn next_times(&self) -> [Option<u64>; 2] {
        self.clocks.map(|v| (v != u64::MAX).then_some(v))
    }
    pub fn auto_start(&self) -> bool {
        if self.long() {
            self.config.long_auto_start
        } else {
            self.config.short_auto_start
        }
    }
    pub fn duration(&self) -> u64 {
        if self.long() {
            self.config.long_minutes as u64 * 60_000
        } else {
            self.config.short_seconds as u64 * 1000
        }
    }
    pub fn restart(&mut self, now: u64) {
        self.last = now;
        self.clocks = self.intervals();
        self.select();
        self.resting_left = None;
        self.phase = if self.config.active() {
            Phase::Working
        } else {
            Phase::Disabled
        };
    }
    pub fn start(&mut self, now: u64) {
        if matches!(self.phase, Phase::Disabled | Phase::Paused | Phase::Resting) {
            return;
        }
        self.last = now;
        self.away_since = None;
        self.blocked = false;

        self.remaining_ms = self.resting_left.take().unwrap_or_else(|| self.duration());
        self.phase = Phase::Resting;
    }
    pub fn skip(&mut self, now: u64) {
        let kind = usize::from(self.active_long);
        self.round = self.round.wrapping_add(1);
        self.clocks[kind] = self.intervals()[kind];
        // A stretch also satisfies an already due short break; never show them back-to-back.
        if kind == 1 && self.clocks[0] <= 30_000 {
            self.clocks[0] = self.intervals()[0];
        }
        self.last = now;
        self.resting_left = None;
        self.select();
        self.phase = if self.config.active() {
            Phase::Working
        } else {
            Phase::Disabled
        };
    }
    pub fn snooze(&mut self, now: u64) {
        self.last = now;
        self.clocks[usize::from(self.active_long)] =
            if self.active_long { 300_000 } else { 120_000 };
        self.select();
        self.phase = Phase::Snoozed;
    }
    pub fn pause(&mut self, now: u64, minutes: u64) {
        self.restart(now);
        self.phase = Phase::Paused;
        self.pause_until = now.saturating_add(minutes * 60_000);
    }
    pub fn tick(&mut self, now: u64, away: bool, blocked: bool) {
        let now = now.max(self.last);
        let elapsed = now.saturating_sub(self.last);
        self.last = now;
        if self.phase == Phase::Disabled {
            return;
        }
        if away {
            self.away_since.get_or_insert(now);
        }
        let natural = self
            .away_since
            .is_some_and(|t| now.saturating_sub(t) >= 300_000);
        if self.phase == Phase::Paused {
            if now >= self.pause_until {
                self.restart(now);
            }
            if !away {
                if natural {
                    self.round = 0;
                }
                self.away_since = None;
            }
            return;
        }
        if away {
            if self.phase == Phase::Warning {
                self.phase = Phase::Due;
            }
            if self.phase == Phase::Resting {
                self.remaining_ms = self.remaining_ms.saturating_sub(elapsed);
                if self.remaining_ms == 0 {
                    self.skip(now);
                }
            }
            if natural {
                self.round = 0;
                self.restart(now);
            }
            return;
        }
        if self.away_since.take().is_some() {
            if natural {
                self.round = 0;
                self.restart(now);
            } else if self.phase == Phase::Resting {
                let left = self.remaining_ms.saturating_sub(elapsed);
                if left == 0 {
                    self.skip(now);
                } else {
                    self.resting_left = Some(left);
                    self.phase = Phase::Due;
                }
            }
            return;
        }
        self.blocked = blocked;
        if blocked && self.phase == Phase::Warning {
            self.phase = Phase::Due;
        }
        if self.phase != Phase::Resting {
            for clock in &mut self.clocks {
                if *clock != u64::MAX {
                    *clock = clock.saturating_sub(elapsed);
                }
            }
            // An unattended short reminder must not prevent the independent stretch alarm.
            if !self.active_long
                && self.clocks[1] <= 30_000
                && matches!(self.phase, Phase::Due | Phase::Warning | Phase::Awaiting)
            {
                self.active_long = true;
                self.remaining_ms = self.clocks[1];
                self.resting_left = None;
                self.phase = Phase::Due;
            }
        }
        match self.phase {
            Phase::Working | Phase::Snoozed => {
                self.select();
                if self.remaining_ms <= 30_000 {
                    self.phase = Phase::Due;
                }
            }
            Phase::Warning | Phase::Resting => {
                self.remaining_ms = self.remaining_ms.saturating_sub(elapsed);
                if self.remaining_ms == 0 {
                    if self.phase == Phase::Warning {
                        if self.auto_start() {
                            self.start(now);
                        } else {
                            self.phase = Phase::Awaiting;
                        }
                    } else {
                        self.skip(now);
                    }
                }
            }
            _ => {}
        }
    }
    pub fn show_due(&self, _now: u64) -> bool {
        matches!(self.phase, Phase::Due | Phase::Warning | Phase::Awaiting)
            && !self.blocked
            && self.away_since.is_none()
    }
    /// Arm only after the adapter successfully presents the warning.
    pub fn warning_presented(&mut self, now: u64) {
        if self.phase == Phase::Due && self.show_due(now) {
            self.phase = Phase::Warning;
            self.remaining_ms = 30_000;
            self.last = now;
        }
    }
    pub fn blocked(&self) -> bool {
        self.blocked
    }
    pub fn natural_reset(&mut self, now: u64) {
        self.round = 0;
        if self.phase != Phase::Paused {
            self.restart(now);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn scheduler() -> RestScheduler {
        RestScheduler::new(
            RestConfig {
                enabled: true,
                ..Default::default()
            },
            0,
        )
    }
    #[test]
    fn independent_intervals_and_stretch_priority() {
        let mut s = scheduler();
        for index in 0..3 {
            let now = s.last + 1_170_000;
            s.tick(now, false, false);
            assert_eq!(s.long(), index == 2);
            s.warning_presented(now);
            s.tick(now + 30_000, false, false);
            assert_eq!(s.phase, Phase::Resting);
            let end = now + 30_000 + s.duration();
            s.tick(end, false, false);
            assert_eq!(s.phase, Phase::Working);
        }
    }
    #[test]
    fn independently_disabled_and_legacy_conversion() {
        let mut c = RestConfig {
            enabled: true,
            short_enabled: false,
            long_interval_minutes: Some(7),
            ..Default::default()
        };
        let mut s = RestScheduler::new(c.clone(), 0);
        assert!(s.long());
        assert_eq!(s.remaining_ms, 420_000);
        s.tick(390_000, false, false);
        assert_eq!(s.phase, Phase::Due);
        c.long_enabled = false;
        assert_eq!(RestScheduler::new(c, 0).phase, Phase::Disabled);
        let old: RestConfig =
            serde_json::from_str(r#"{"enabled":true,"interval_minutes":15,"long_every":4}"#)
                .unwrap();
        assert_eq!(old.long_interval(), 60);
    }
    #[test]
    fn unattended_short_does_not_block_stretch() {
        let mut s = scheduler();
        s.config.short_auto_start = false;
        s.tick(1_170_000, false, false);
        s.warning_presented(1_170_000);
        s.tick(1_200_000, false, false);
        assert_eq!(s.phase, Phase::Awaiting);
        s.tick(3_570_000, false, false);
        assert!(s.long());
        assert_eq!(s.phase, Phase::Due);
    }
    #[test]
    fn snooze_skip_pause_and_resume() {
        let mut s = scheduler();
        s.snooze(0);
        s.tick(120_000, false, false);
        assert_eq!(s.phase, Phase::Due);
        s.snooze(120_000);
        assert_eq!(s.round, 0);
        s.skip(120_000);
        assert_eq!(s.round, 1);
        s.pause(120_000, 30);
        s.natural_reset(300_000);
        assert_eq!(s.phase, Phase::Paused);
        s.tick(1_920_000, false, false);
        assert_eq!(s.phase, Phase::Working);
    }
    #[test]
    fn away_and_fullscreen() {
        let mut s = scheduler();
        s.tick(1000, true, false);
        s.tick(301_000, true, false);
        s.tick(302_000, false, false);
        assert_eq!(s.remaining_ms, 1_200_000);
        s.tick(1_502_000, false, true);
        assert!(!s.show_due(1_502_000));
        s.tick(1_503_000, false, false);
        assert!(s.show_due(1_503_000));
        s.tick(1_533_000, false, false);
        assert!(s.show_due(1_533_000));
    }
    #[test]
    fn short_absence_and_rest_completion_during_lock() {
        let mut s = scheduler();
        s.tick(1000, false, false);
        let left = s.remaining_ms;
        s.tick(2000, true, false);
        s.tick(62000, false, false);
        assert_eq!(s.remaining_ms, left);
        s.start(62000);
        s.tick(63000, true, false);
        s.tick(90000, false, false);
        assert_eq!(s.phase, Phase::Working);
        assert_eq!(s.round, 1);
    }
    #[test]
    fn interrupted_rest_resumes_only_on_request() {
        let mut s = scheduler();
        s.start(0);
        s.tick(1000, true, false);
        s.tick(4000, false, false);
        assert_eq!(s.phase, Phase::Due);
        assert_eq!(s.resting_left, Some(16000));
        s.tick(10000, false, false);
        assert_eq!(s.resting_left, Some(16000));
        s.start(10000);
        assert_eq!(s.remaining_ms, 16000);
    }
    #[test]
    fn delayed_and_backward_ticks_never_duplicate_breaks() {
        let mut s = scheduler();
        s.tick(1000, false, false);
        let left = s.remaining_ms;
        s.tick(0, false, false);
        s.tick(1000, false, false);
        assert_eq!(s.remaining_ms, left);
        s.tick(9_000_000, false, false);
        assert_eq!(s.phase, Phase::Due);
        assert_eq!(s.round, 0);
        s.tick(10_000_000, false, false);
        assert_eq!(s.round, 0);
    }
    #[test]
    fn automatic_start_requires_visible_warning() {
        let mut s = scheduler();
        s.tick(1_169_000, false, false);
        assert_eq!(s.phase, Phase::Working);
        s.tick(1_170_000, false, false);
        assert_eq!(s.phase, Phase::Due);
        s.warning_presented(1_170_000);
        s.tick(1_199_000, false, false);
        assert_eq!(s.phase, Phase::Warning);
        s.tick(1_200_000, false, false);
        assert_eq!(s.phase, Phase::Resting);
        assert_eq!(s.remaining_ms, 20_000);
        s.tick(1_220_000, false, false);
        assert_eq!(s.round, 1);
    }
    #[test]
    fn interrupted_warning_gets_full_notice() {
        let mut s = scheduler();
        s.tick(1_170_000, false, false);
        s.warning_presented(1_170_000);
        s.tick(1_180_000, false, true);
        assert_eq!(s.phase, Phase::Due);
        assert!(!s.show_due(1_180_000));
        s.tick(1_300_000, false, false);
        s.warning_presented(1_300_000);
        assert_eq!(s.remaining_ms, 30_000);
        s.tick(1_301_000, true, false);
        s.tick(1_310_000, false, false);
        s.warning_presented(1_310_000);
        assert_eq!(s.remaining_ms, 30_000);
        s.tick(1_340_000, false, false);
        assert_eq!(s.phase, Phase::Resting);
    }
    #[test]
    fn independent_short_and_long_modes() {
        for (short, long) in [(false, false), (false, true), (true, false), (true, true)] {
            let mut s = scheduler();
            s.config.short_auto_start = short;
            s.config.long_auto_start = long;
            for (kind, automatic) in [(false, short), (true, long)] {
                s.clocks = if kind {
                    [u64::MAX, 1_200_000]
                } else {
                    [1_200_000, u64::MAX]
                };
                s.phase = Phase::Working;
                let now = s.last + 1_200_000;
                s.tick(now, false, false);
                s.warning_presented(now);
                s.tick(now + 30_000, false, false);
                assert_eq!(
                    s.phase,
                    if automatic {
                        Phase::Resting
                    } else {
                        Phase::Awaiting
                    }
                );
                if !automatic {
                    s.tick(now + 90_000, false, false);
                    assert_eq!(s.phase, Phase::Awaiting);
                    s.start(now + 90_000);
                    assert_eq!(s.phase, Phase::Resting);
                }
                s.skip(now + 90_000);
            }
            let saved = serde_json::to_string(&s.config).unwrap();
            assert_eq!(
                serde_json::from_str::<RestConfig>(&saved).unwrap(),
                s.config
            );
        }
    }
    #[test]
    fn old_app_config_gets_disabled_rest() {
        let mut v = serde_json::to_value(crate::AppConfig::default()).unwrap();
        v.as_object_mut().unwrap().remove("rest");
        let restored: crate::AppConfig = serde_json::from_value(v).unwrap();
        assert!(!restored.rest.enabled);
        assert!(restored.validate().is_ok());
    }
    #[test]
    fn explicit_start_after_away_stays_resting() {
        let mut s = scheduler();
        s.tick(1000, true, false);
        s.start(2000);
        s.tick(2000, false, false);
        assert_eq!(s.phase, Phase::Resting);
    }
    #[test]
    fn legacy_defaults_and_validation() {
        let c: RestConfig = serde_json::from_str("{}").unwrap();
        assert!(!c.enabled);
        assert!(c.validate().is_ok());
        assert!(
            RestConfig {
                interval_minutes: 0,
                ..c
            }
            .validate()
            .is_err()
        );
    }
}
