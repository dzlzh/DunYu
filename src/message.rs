//! Notification filtering and the read-only Windows notification store.
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Legacy plaintext is read only; serialization never writes it back out.
#[derive(Clone, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum ProtectedValue {
    Encrypted(Vec<u8>),
    Legacy(String),
}
impl Default for ProtectedValue {
    fn default() -> Self {
        Self::Encrypted(vec![])
    }
}
impl std::fmt::Debug for ProtectedValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[protected]")
    }
}
impl Serialize for ProtectedValue {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Encrypted(bytes) => bytes.serialize(s),
            Self::Legacy(_) => Err(serde::ser::Error::custom(
                "旧凭据尚未加密，请先保存企业微信配置",
            )),
        }
    }
}
impl ProtectedValue {
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Encrypted(v) => v.is_empty(),
            Self::Legacy(v) => v.trim().is_empty(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct MessageConfig {
    pub enabled: bool,
    pub keywords: Vec<String>,
    pub poll_seconds: u32,
    pub delay_seconds: u32,
    pub corp_id: ProtectedValue,
    pub agent_id: ProtectedValue,
    pub secret: ProtectedValue,
    pub recipients: ProtectedValue,
}
impl Default for MessageConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            keywords: vec!["telegram".into()],
            poll_seconds: 10,
            delay_seconds: 60,
            corp_id: Default::default(),
            agent_id: Default::default(),
            secret: Default::default(),
            recipients: Default::default(),
        }
    }
}
impl MessageConfig {
    pub fn credentials_complete(&self) -> bool {
        [
            &self.corp_id,
            &self.agent_id,
            &self.secret,
            &self.recipients,
        ]
        .iter()
        .all(|v| !v.is_empty())
    }

    pub fn validate(&self) -> Result<(), String> {
        if !(1..=3600).contains(&self.poll_seconds) || self.delay_seconds > 3600 {
            return Err("检查间隔须为1～3600秒，合并等待须为0～3600秒".into());
        }
        if self.enabled
            && (self.keywords.iter().all(|s| s.trim().is_empty()) || !self.credentials_complete())
        {
            return Err("启用传讯需要关键词、企业ID、有效应用ID、Secret及接收成员".into());
        }
        Ok(())
    }
    pub fn matches(&self, payload: &str) -> bool {
        let text = payload.to_lowercase();
        self.keywords
            .iter()
            .any(|s| !s.trim().is_empty() && text.contains(&s.trim().to_lowercase()))
    }
}
pub fn keywords(text: &str) -> Vec<String> {
    let mut result: Vec<String> = vec![];
    for word in text.lines().map(str::trim).filter(|s| !s.is_empty()) {
        if !result
            .iter()
            .any(|s| s.to_lowercase() == word.to_lowercase())
        {
            result.push(word.into());
        }
    }
    result
}
pub fn content(payload: &[u8]) -> Result<String, String> {
    use quick_xml::{Reader, events::Event};
    let mut reader = Reader::from_reader(payload);
    let mut texts = Vec::new();
    let mut current = String::new();
    let mut inside = false;
    let mut depth = 0usize;
    loop {
        match reader.read_event().map_err(|_| "通知XML无效")? {
            Event::Start(e) => {
                depth += 1;
                if e.local_name().as_ref() == b"text" {
                    inside = true;
                    current.clear();
                }
            }
            Event::End(e) => {
                depth = depth.saturating_sub(1);
                if e.local_name().as_ref() == b"text" {
                    inside = false;
                    if !current.trim().is_empty() {
                        texts.push(current.trim().to_string());
                    }
                }
            }
            Event::Text(e) if inside => current.push_str(&e.decode().map_err(|_| "通知编码无效")?),
            Event::CData(e) if inside => current.push_str(&e.decode().map_err(|_| "通知编码无效")?),
            Event::GeneralRef(e) if inside => {
                let name = e.decode().map_err(|_| "通知编码无效")?;
                let encoded = format!("&{name};");
                current
                    .push_str(&quick_xml::escape::unescape(&encoded).map_err(|_| "通知实体无效")?);
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if depth != 0 || texts.is_empty() {
        return Err("通知没有有效文本".into());
    }
    Ok(limit_text(&texts.join("\n")))
}
pub fn limit_text(text: &str) -> String {
    const MARK: &str = "\n[已截断]";
    if text.len() <= 2048 {
        return text.into();
    }
    let mut end = 2048 - MARK.len();
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{MARK}", &text[..end])
}
/// Queue timestamps define batch membership, independently of worker scheduling.
pub struct PendingNotification {
    pub id: i64,
    pub text: String,
    pub received_at: std::time::Instant,
}

/// Retains the first notification outside the window for the next batch.
pub fn collect_batch(
    rx: &std::sync::mpsc::Receiver<PendingNotification>,
    next: &mut Option<PendingNotification>,
    delay: std::time::Duration,
    cancel: &std::sync::atomic::AtomicBool,
) -> Option<Vec<PendingNotification>> {
    use std::sync::{atomic::Ordering, mpsc};
    use std::time::{Duration, Instant};
    let tick = Duration::from_millis(100);
    let first = loop {
        if cancel.load(Ordering::SeqCst) {
            return None;
        }
        if let Some(first) = next.take() {
            break first;
        }
        match rx.recv_timeout(tick) {
            Ok(first) => break first,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(_) => return None,
        }
    };
    // Zero means snapshot the currently queued notifications without waiting.
    let deadline = if delay.is_zero() {
        Instant::now()
    } else {
        first.received_at + delay
    };
    let mut batch = vec![first];
    loop {
        if cancel.load(Ordering::SeqCst) {
            return None;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        let received = if remaining.is_zero() {
            rx.try_recv().ok()
        } else {
            match rx.recv_timeout(remaining.min(tick)) {
                Ok(item) => Some(item),
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(_) => None,
            }
        };
        let Some(item) = received else { break };
        if item.received_at >= deadline {
            *next = Some(item);
            break;
        }
        batch.push(item);
    }
    Some(batch)
}

pub fn batch_content(texts: &[String]) -> String {
    if texts.len() == 1 {
        return limit_text(&texts[0]);
    }
    if texts.is_empty() {
        return String::new();
    }
    let mut text = format!("通知汇总（共 {} 条）", texts.len());
    for (index, item) in texts.iter().enumerate() {
        use std::fmt::Write;
        let _ = write!(text, "\n\n{}. {}", index + 1, item);
        if text.len() > 2048 {
            break;
        }
    }
    const MARK: &str = "\n[内容已截断，请到原应用查看]";
    if text.len() > 2048 || texts.iter().any(|t| t.ends_with("[已截断]")) {
        let mut end = text.len().min(2048 - MARK.len());
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        text.push_str(MARK);
    }
    text
}

pub struct Store(pub rusqlite::Connection);
impl Store {
    pub fn open(path: &Path) -> Result<Self, rusqlite::Error> {
        let db = rusqlite::Connection::open_with_flags(
            path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        db.busy_timeout(std::time::Duration::from_millis(250))?;
        Ok(Self(db))
    }
    pub fn maximum(&self) -> Result<i64, rusqlite::Error> {
        self.0
            .query_row("SELECT COALESCE(MAX(id),0) FROM Notification", [], |r| {
                r.get(0)
            })
    }
    pub fn scan(&self, after: i64) -> Result<Vec<(i64, Vec<u8>)>, rusqlite::Error> {
        self.0.prepare("SELECT id, CAST(COALESCE(Payload, '') AS BLOB) FROM Notification WHERE id > ? AND type = 'toast' ORDER BY id ASC LIMIT 100")?
            .query_map([after], |r| Ok((r.get(0)?, r.get(1)?)))?.collect()
    }
    pub fn exists(&self, id: i64) -> Result<bool, rusqlite::Error> {
        self.0.query_row(
            "SELECT EXISTS(SELECT 1 FROM Notification WHERE id=?)",
            [id],
            |r| r.get(0),
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn filtering_and_xml() {
        let c = MessageConfig {
            keywords: keywords(" Telegram \n\nTELEGRAM\n微信"),
            ..Default::default()
        };
        assert_eq!(c.keywords.len(), 2);
        assert!(c.matches("app=TeLeGrAm"));
        assert!(!c.matches("other"));
        assert_eq!(content("<toast><visual><binding><text>标题 &amp; 内容</text><text><![CDATA[正文]]></text></binding></visual></toast>".as_bytes()).unwrap(), "标题 & 内容\n正文");
        assert!(content(b"<toast><text>oops").is_err());
        assert!(content(b"<toast/>").is_err());
        let long = limit_text(&"汉".repeat(900));
        assert!(long.len() <= 2048);
        assert!(long.ends_with("[已截断]"));
    }
    #[test]
    fn defaults_and_validation() {
        let c: MessageConfig = serde_json::from_str("{}").unwrap();
        assert!(!c.enabled);
        c.validate().unwrap();
        let mut c = c;
        c.enabled = true;
        assert!(c.validate().is_err());
        c.enabled = false;
        c.poll_seconds = 0;
        assert!(c.validate().is_err());
        let mut old = serde_json::to_value(crate::AppConfig::default()).unwrap();
        old.as_object_mut().unwrap().remove("message");
        assert!(
            !serde_json::from_value::<crate::AppConfig>(old)
                .unwrap()
                .message
                .enabled
        );
    }
    #[test]
    fn live_readonly_wal() {
        let path =
            std::env::temp_dir().join(format!("dunyu-notifications-{}.db", std::process::id()));
        let writer = rusqlite::Connection::open(&path).unwrap();
        writer.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE Notification(id INTEGER PRIMARY KEY, Payload BLOB, type TEXT);").unwrap();
        let reader = Store::open(&path).unwrap();
        assert_eq!(reader.maximum().unwrap(), 0);
        writer
            .execute(
                "INSERT INTO Notification VALUES(1,?,'toast')",
                [b"<toast/>".as_slice()],
            )
            .unwrap();
        assert_eq!(reader.scan(0).unwrap().len(), 1);
        assert!(reader.exists(1).unwrap());
        assert!(reader.0.execute("DELETE FROM Notification", []).is_err());
        writer.execute("DELETE FROM Notification", []).unwrap();
        assert!(!reader.exists(1).unwrap());
        drop(reader);
        drop(writer);
        std::fs::remove_file(path).unwrap();
    }
}

#[cfg(test)]
mod credential_tests {
    use super::*;
    #[test]
    fn legacy_is_readable_but_never_serialized_or_debugged() {
        let c: MessageConfig=serde_json::from_str(r#"{"corp_id":"private-corp","agent_id":"17","secret":[1,2,3],"recipients":"private-member"}"#).unwrap();
        assert!(serde_json::to_string(&c).is_err());
        assert!(!format!("{c:?}").contains("private-"));
        assert!(matches!(c.secret, ProtectedValue::Encrypted(_)));
        let encrypted = ProtectedValue::Encrypted(vec![0, 10, 255]);
        assert_eq!(
            serde_json::from_str::<ProtectedValue>(&serde_json::to_string(&encrypted).unwrap())
                .unwrap(),
            encrypted
        );
    }
}

#[cfg(test)]
mod batch_tests {
    use super::*;
    use std::sync::{atomic::AtomicBool, mpsc};
    use std::time::{Duration, Instant};

    fn item(id: i64, received_at: Instant) -> PendingNotification {
        PendingNotification {
            id,
            text: format!("message {id}"),
            received_at,
        }
    }

    #[test]
    fn fixed_window_uses_enqueue_time_and_retains_boundary_item() {
        let (tx, rx) = mpsc::channel();
        let start = Instant::now() - Duration::from_secs(121);
        for (id, offset) in [(1, 0), (2, 2), (3, 59), (4, 60), (5, 61)] {
            tx.send(item(id, start + Duration::from_secs(offset)))
                .unwrap();
        }
        let mut next = None;
        let cancel = AtomicBool::new(false);
        let delay = Duration::from_secs(60);
        let first = collect_batch(&rx, &mut next, delay, &cancel).unwrap();
        assert_eq!(first.iter().map(|n| n.id).collect::<Vec<_>>(), [1, 2, 3]);
        let second = collect_batch(&rx, &mut next, delay, &cancel).unwrap();
        assert_eq!(second.iter().map(|n| n.id).collect::<Vec<_>>(), [4, 5]);
    }

    #[test]
    fn single_notification_waits_until_the_first_deadline() {
        let (tx, rx) = mpsc::channel();
        let start = Instant::now();
        let delay = Duration::from_millis(20);
        tx.send(item(1, start)).unwrap();
        let batch = collect_batch(&rx, &mut None, delay, &AtomicBool::new(false)).unwrap();
        assert_eq!(batch.len(), 1);
        assert!(start.elapsed() >= delay);
    }

    #[test]
    fn zero_delay_drains_available_notifications() {
        let (tx, rx) = mpsc::channel();
        let before = Instant::now() - Duration::from_secs(1);
        tx.send(item(1, before)).unwrap();
        tx.send(item(2, before)).unwrap();
        let batch = collect_batch(&rx, &mut None, Duration::ZERO, &AtomicBool::new(false)).unwrap();
        assert_eq!(batch.len(), 2);
    }

    #[test]
    fn cancellation_discards_the_open_batch() {
        use std::sync::{Arc, atomic::Ordering};
        let (tx, rx) = mpsc::sync_channel(0);
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let worker = std::thread::spawn(move || {
            collect_batch(&rx, &mut None, Duration::from_secs(60), &worker_cancel)
        });
        // A rendezvous guarantees the collector has received the first item.
        tx.send(item(1, Instant::now())).unwrap();
        cancel.store(true, Ordering::SeqCst);
        assert!(worker.join().unwrap().is_none());
    }

    #[test]
    fn summary_preserves_order_count_and_single_message_format() {
        assert_eq!(batch_content(&[]), "");
        assert_eq!(batch_content(&["hello".into()]), "hello");
        assert_eq!(
            batch_content(&["first".into(), "second".into()]),
            "通知汇总（共 2 条）\n\n1. first\n\n2. second"
        );
    }

    #[test]
    fn long_summary_is_one_utf8_message_with_omission_notice() {
        let text = batch_content(&[
            format!("汉{}", char::from_u32(0x1f41f).unwrap()).repeat(500),
            "second".into(),
            "third".into(),
        ]);
        assert!(text.len() <= 2048);
        assert!(text.starts_with("通知汇总（共 3 条）"));
        assert!(text.ends_with("[内容已截断，请到原应用查看]"));
        let text = batch_content(&[limit_text(&"汉".repeat(900)), "second".into()]);
        assert!(text.len() <= 2048);
        assert!(text.ends_with("[内容已截断，请到原应用查看]"));
    }
}
