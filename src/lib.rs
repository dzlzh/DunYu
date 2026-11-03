pub mod scroll;
use serde::{Deserialize, Serialize};
pub mod rest;
use std::{collections::HashSet, fs, io, path::Path};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Hotkey {
    pub modifiers: u32,
    pub key: u32,
}
impl Hotkey {
    pub fn new(modifiers: u32, key: u32) -> Self {
        Self { modifiers, key }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SummonRule {
    pub name: String,
    pub enabled: bool,
    pub executable: String,
    pub arguments: String,
    pub working_directory: String,
    pub process_path: String,
    pub class: String,
    pub title: String,
    pub hotkey: Hotkey,
    pub minimize: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Slot {
    pub bind: Hotkey,
    pub toggle: Hotkey,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppConfig {
    pub version: u32,
    pub rules: Vec<SummonRule>,
    pub slots: Vec<Slot>,
    pub opacity_down: Hotkey,
    pub opacity_up: Hotkey,
    pub opacity_reset: Hotkey,
    pub restore_all: Hotkey,
    pub step: u8,
    pub autostart: bool,
    #[serde(default)]
    pub hide_tray: bool,
    #[serde(default)]
    pub rest: rest::RestConfig,
}
impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: 1,
            rules: vec![],
            slots: (b'1'..=b'4')
                .map(|k| Slot {
                    bind: Hotkey::new(7, k.into()),
                    toggle: Hotkey::new(3, k.into()),
                })
                .collect(),
            opacity_down: Hotkey::new(3, 0x25),
            opacity_up: Hotkey::new(3, 0x27),
            opacity_reset: Hotkey::new(3, b'0'.into()),
            restore_all: Hotkey::new(3, 8),
            step: 10,
            autostart: false,
            hide_tray: false,
            rest: Default::default(),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Summon(usize),
    Bind(usize),
    Toggle(usize),
    Opacity(i8),
    Opaque,
    Restore,
}
impl AppConfig {
    pub fn bindings(&self) -> Vec<(Hotkey, Command)> {
        let mut b: Vec<_> = self
            .rules
            .iter()
            .enumerate()
            .filter(|(_, r)| r.enabled)
            .map(|(i, r)| (r.hotkey.clone(), Command::Summon(i)))
            .collect();
        for (i, s) in self.slots.iter().enumerate() {
            b.push((s.bind.clone(), Command::Bind(i)));
            b.push((s.toggle.clone(), Command::Toggle(i)));
        }
        b.extend([
            (self.opacity_down.clone(), Command::Opacity(-1)),
            (self.opacity_up.clone(), Command::Opacity(1)),
            (self.opacity_reset.clone(), Command::Opaque),
            (self.restore_all.clone(), Command::Restore),
        ]);
        b
    }
    pub fn validate(&self) -> Result<(), String> {
        self.rest.validate()?;
        if self.version != 1 {
            return Err("不支持的配置版本".into());
        }
        if !(1..=50).contains(&self.step) || self.slots.len() != 4 || self.rules.len() > 500 {
            return Err("步长应为 1–50，槽位必须为 4，规则最多 500".into());
        }
        let mut seen = HashSet::new();
        for (h, _) in self.bindings() {
            if h.modifiers == 0
                || h.modifiers & !15 != 0
                || !(8..=254).contains(&h.key)
                || [16, 17, 18, 91, 92, 0x7b].contains(&h.key)
            {
                return Err("快捷键需要修饰键和有效按键；F12 保留给系统".into());
            }
            if !seen.insert(h) {
                return Err("快捷键重复".into());
            }
        }
        for r in &self.rules {
            if r.executable.trim().is_empty() || r.process_path.trim().is_empty() {
                return Err("请填写启动路径和匹配进程完整路径（快捷方式须指定实际进程）".into());
            }
        }
        Ok(())
    }
}
pub fn matches(rule: &SummonRule, path: &str, class: &str, title: &str) -> bool {
    rule.process_path.to_lowercase() == path.to_lowercase()
        && (rule.class.is_empty() || rule.class == class)
        && title.contains(&rule.title)
}
pub mod policy;
pub use policy::*;
pub fn load(path: &Path) -> Result<AppConfig, String> {
    let c: AppConfig = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    c.validate()?;
    Ok(c)
}
pub fn atomic_json<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
    use std::io::Write;
    fs::create_dir_all(path.parent().unwrap())?;
    let temp = path.with_extension("tmp");
    let mut file = fs::File::create(&temp)?;
    file.write_all(&serde_json::to_vec_pretty(value)?)?;
    file.sync_all()?;
    drop(file);
    #[cfg(not(windows))]
    fs::rename(temp, path)?;
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let a: Vec<_> = temp.as_os_str().encode_wide().chain(Some(0)).collect();
        let b: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        if unsafe {
            windows_sys::Win32::Storage::FileSystem::MoveFileExW(
                a.as_ptr(),
                b.as_ptr(),
                windows_sys::Win32::Storage::FileSystem::MOVEFILE_REPLACE_EXISTING
                    | windows_sys::Win32::Storage::FileSystem::MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_and_conflicts() {
        let mut c = AppConfig::default();
        assert!(c.validate().is_ok());
        c.opacity_up = c.opacity_down.clone();
        assert!(c.validate().is_err());
    }
    #[test]
    fn version_and_step() {
        let mut c = AppConfig::default();
        c.version = 2;
        assert!(c.validate().is_err());
        c.version = 1;
        c.step = 0;
        assert!(c.validate().is_err());
    }
    #[test]
    fn toggle_states() {
        assert_eq!(toggle_action(true, true, false, false), ToggleAction::Hide);
        assert_eq!(
            toggle_action(false, true, false, false),
            ToggleAction::Activate
        );
        assert_eq!(
            toggle_action(true, true, true, false),
            ToggleAction::Activate
        );
        assert_eq!(
            toggle_action(true, true, false, true),
            ToggleAction::Minimize
        );
    }
    #[test]
    fn alpha_limits() {
        assert_eq!(opacity_step(26, -1, 50), 26);
        assert_eq!(opacity_step(250, 1, 10), 255);
        assert!(opacity_step(255, -1, 1) < 255);
    }
    #[test]
    fn serialization() {
        let c = AppConfig::default();
        let d: AppConfig = serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
        assert_eq!(c.bindings(), d.bindings());
    }
    #[test]
    fn old_config_keeps_tray_visible() {
        let mut value = serde_json::to_value(AppConfig::default()).unwrap();
        value.as_object_mut().unwrap().remove("hide_tray");
        let config: AppConfig = serde_json::from_value(value).unwrap();
        config.validate().unwrap();
        assert!(!config.hide_tray);
    }
    #[test]
    fn match_requires_process() {
        let r = SummonRule {
            name: "a".into(),
            enabled: true,
            executable: "a.exe".into(),
            arguments: String::new(),
            working_directory: String::new(),
            process_path: "C:\\a.exe".into(),
            class: "Editor".into(),
            title: "文档".into(),
            hotkey: Hotkey::new(3, 65),
            minimize: false,
        };
        assert!(matches(&r, "c:\\A.EXE", "Editor", "文档 1"));
        assert!(!matches(&r, "c:\\b.exe", "Editor", "文档 1"));
    }
}

#[cfg(test)]
mod persistence_tests {
    use super::*;
    #[test]
    fn atomic_config_and_corruption_preservation() {
        let dir = std::env::temp_dir().join(format!("dunyu-config-test-{}", std::process::id()));
        let path = dir.join("config.json");
        atomic_json(&path, &AppConfig::default()).unwrap();
        assert_eq!(load(&path).unwrap().step, 10);
        let mut changed = AppConfig::default();
        changed.step = 20;
        changed.hide_tray = true;
        atomic_json(&path, &changed).unwrap();
        assert_eq!(load(&path).unwrap().step, 20);
        assert!(load(&path).unwrap().hide_tray);
        changed.hide_tray = false;
        atomic_json(&path, &changed).unwrap();
        assert!(!load(&path).unwrap().hide_tray);
        fs::write(&path, b"broken config").unwrap();
        assert!(load(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"broken config");
        fs::remove_dir_all(dir).unwrap();
    }
}
