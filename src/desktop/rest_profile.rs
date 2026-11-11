//! Opt-in wall-clock profiling, never linked into the normal application.
use super::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;
#[derive(Default)]
struct Profile {
    enabled: bool,
    window: String,
    values: BTreeMap<String, BTreeMap<&'static str, Vec<f64>>>,
}
thread_local! { static PROFILE: RefCell<Profile> = RefCell::new(Profile::default()); }
pub(super) fn enabled() -> bool {
    PROFILE.with(|p| p.borrow().enabled)
}
pub(super) fn start() {
    PROFILE.with(|p| {
        *p.borrow_mut() = Profile {
            enabled: true,
            ..Default::default()
        }
    });
}
pub(super) fn finish() -> Value {
    let p = PROFILE.with(|p| std::mem::take(&mut *p.borrow_mut()));
    let mut windows = serde_json::Map::new();
    for (window, stages) in p.values {
        let mut output = serde_json::Map::new();
        for (name, samples) in stages {
            let mut sorted = samples.clone();
            sorted.sort_by(f64::total_cmp);
            let n = sorted.len();
            output.insert(name.into(),json!({"count":n,"total_ms":samples.iter().sum::<f64>(),"median_ms":sorted[n/2],"p95_ms":sorted[(n*95).div_ceil(100)-1],"samples_ms":samples}));
        }
        windows.insert(window, Value::Object(output));
    }
    json!({"windows":windows,"timing":"inclusive wall time; do not add parent and child stages"})
}
pub(super) struct WindowScope(String);
pub(super) unsafe fn window(h: HWND) -> WindowScope {
    let old = PROFILE.with(|p| {
        let mut p = p.borrow_mut();
        if !p.enabled {
            return String::new();
        }
        let mut r: RECT = std::mem::zeroed();
        GetWindowRect(h, &mut r);
        std::mem::replace(
            &mut p.window,
            format!(
                "{},{},{}x{}@{}",
                r.left,
                r.top,
                r.right - r.left,
                r.bottom - r.top,
                GetDpiForWindow(h)
            ),
        )
    });
    WindowScope(old)
}
impl Drop for WindowScope {
    fn drop(&mut self) {
        PROFILE.with(|p| p.borrow_mut().window = std::mem::take(&mut self.0));
    }
}
pub(super) struct Stage(Option<(String, &'static str, Instant)>);
pub(super) fn stage(name: &'static str) -> Stage {
    Stage(PROFILE.with(|p| {
        let p = p.borrow();
        p.enabled.then(|| {
            (
                if p.window.is_empty() {
                    "host".into()
                } else {
                    p.window.clone()
                },
                name,
                Instant::now(),
            )
        })
    }))
}
impl Drop for Stage {
    fn drop(&mut self) {
        if let Some((window, name, start)) = self.0.take() {
            let ms = start.elapsed().as_secs_f64() * 1000.;
            PROFILE.with(|p| {
                p.borrow_mut()
                    .values
                    .entry(window)
                    .or_default()
                    .entry(name)
                    .or_default()
                    .push(ms)
            });
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inactive_and_nested_measurements() {
        drop(stage("off"));
        assert_eq!(finish()["windows"], json!({}));
        start();
        {
            let _total = stage("total");
            drop(stage("child"));
        }
        let report = finish();
        assert_eq!(report["windows"]["host"]["child"]["count"], 1);
        assert!(
            report["windows"]["host"]["total"]["total_ms"]
                .as_f64()
                .unwrap()
                >= report["windows"]["host"]["child"]["total_ms"]
                    .as_f64()
                    .unwrap()
        );
    }
}
