use super::*;
use dunyu::message::{MessageConfig, ProtectedValue, Store};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::Duration;
use windows_sys::Win32::{Networking::WinHttp::*, Security::Cryptography::*};
pub const UPDATE: u32 = WM_APP + 8;

pub fn protect(input: &[u8], decrypt: bool) -> Result<Vec<u8>, String> {
    unsafe {
        let source = CRYPT_INTEGER_BLOB {
            cbData: input.len() as u32,
            pbData: input.as_ptr() as *mut u8,
        };
        let mut out: CRYPT_INTEGER_BLOB = std::mem::zeroed();
        let ok = if decrypt {
            CryptUnprotectData(
                &source,
                null_mut(),
                null(),
                null(),
                null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            )
        } else {
            CryptProtectData(
                &source,
                null(),
                null(),
                null(),
                null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            )
        };
        if ok == 0 {
            return Err("企业微信凭据无法加密或解密，请在当前Windows账户重新填写".into());
        }
        let bytes = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        LocalFree(out.pbData as _);
        Ok(bytes)
    }
}
pub fn encrypted(text: &str) -> Result<ProtectedValue, String> {
    if text.is_empty() {
        return Ok(ProtectedValue::default());
    }
    protect(text.as_bytes(), false).map(ProtectedValue::Encrypted)
}
fn plaintext(value: &ProtectedValue) -> Result<String, String> {
    match value {
        ProtectedValue::Encrypted(bytes) if bytes.is_empty() => Ok(String::new()),
        ProtectedValue::Encrypted(bytes) => {
            String::from_utf8(protect(bytes, true)?).map_err(|_| "企业微信凭据编码无效".into())
        }
        ProtectedValue::Legacy(_) => Err("旧凭据尚未加密，请先保存企业微信配置".into()),
    }
}
pub fn migrate(c: &mut MessageConfig) -> Result<bool, String> {
    let mut next = c.clone();
    let mut changed = false;
    for value in [
        &mut next.corp_id,
        &mut next.agent_id,
        &mut next.secret,
        &mut next.recipients,
    ] {
        if let ProtectedValue::Legacy(text) = value {
            *value = encrypted(text)?;
            changed = true;
        }
    }
    *c = next;
    Ok(changed)
}
pub fn validate_credentials(c: &MessageConfig) -> Result<(), String> {
    if plaintext(&c.corp_id)?.trim().is_empty()
        || plaintext(&c.secret)?.is_empty()
        || plaintext(&c.recipients)?.trim().is_empty()
        || plaintext(&c.agent_id)?
            .parse::<u32>()
            .ok()
            .filter(|v| *v > 0)
            .is_none()
    {
        return Err("请填写企业ID、正整数应用ID、Secret及接收成员".into());
    }
    Ok(())
}
struct Internet(*mut std::ffi::c_void);
impl Drop for Internet {
    fn drop(&mut self) {
        unsafe {
            WinHttpCloseHandle(self.0);
        }
    }
}
impl Internet {
    fn new(p: *mut std::ffi::c_void) -> Result<Self, String> {
        if p.is_null() {
            Err("无法创建网络请求".into())
        } else {
            Ok(Self(p))
        }
    }
}
fn request(path: &str, body: Option<&serde_json::Value>) -> Result<serde_json::Value, String> {
    unsafe {
        let session = Internet::new(WinHttpOpen(
            wide("DunYu").as_ptr(),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            null(),
            null(),
            0,
        ))?;
        WinHttpSetTimeouts(session.0, 5000, 5000, 10000, 10000);
        let connection = Internet::new(WinHttpConnect(
            session.0,
            wide("qyapi.weixin.qq.com").as_ptr(),
            443,
            0,
        ))?;
        let req = Internet::new(WinHttpOpenRequest(
            connection.0,
            wide(if body.is_some() { "POST" } else { "GET" }).as_ptr(),
            wide(path).as_ptr(),
            null(),
            null(),
            null(),
            WINHTTP_FLAG_SECURE,
        ))?;
        let policy = WINHTTP_OPTION_REDIRECT_POLICY_NEVER;
        WinHttpSetOption(
            req.0,
            WINHTTP_OPTION_REDIRECT_POLICY,
            &policy as *const _ as _,
            std::mem::size_of_val(&policy) as u32,
        );
        let bytes = body.map(|v| v.to_string().into_bytes()).unwrap_or_default();
        if WinHttpSendRequest(
            req.0,
            wide("Content-Type: application/json\r\n").as_ptr(),
            u32::MAX,
            bytes.as_ptr() as _,
            bytes.len() as u32,
            bytes.len() as u32,
            0,
        ) == 0
            || WinHttpReceiveResponse(req.0, null_mut()) == 0
        {
            return Err("网络请求失败或超时".into());
        }
        let mut code = 0u32;
        let mut size = 4u32;
        if WinHttpQueryHeaders(
            req.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            null(),
            &mut code as *mut _ as _,
            &mut size,
            null_mut(),
        ) == 0
            || code != 200
        {
            return Err(format!("企业微信HTTP错误：{code}"));
        }
        let mut result = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if Instant::now() >= deadline {
                return Err("读取响应超时".into());
            }
            let mut buf = [0u8; 4096];
            let mut read = 0;
            if WinHttpReadData(req.0, buf.as_mut_ptr() as _, buf.len() as u32, &mut read) == 0 {
                return Err("读取响应失败".into());
            }
            if read == 0 {
                break;
            }
            result.extend_from_slice(&buf[..read as usize]);
            if result.len() > 65536 {
                return Err("企业微信响应过大".into());
            }
        }
        serde_json::from_slice(&result).map_err(|_| "企业微信响应无效".into())
    }
}
fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
#[derive(Default)]
struct Sender {
    token: String,
    expires: Option<Instant>,
}
impl Sender {
    fn send_with(
        &mut self,
        c: &MessageConfig,
        text: &str,
        mut http: impl FnMut(&str, Option<&serde_json::Value>) -> Result<serde_json::Value, String>,
    ) -> Result<(), String> {
        for attempt in 0..2 {
            if self.expires.is_none_or(|t| t <= Instant::now()) {
                let secret = plaintext(&c.secret)?;
                let value = http(
                    &format!(
                        "/cgi-bin/gettoken?corpid={}&corpsecret={}",
                        encode(&plaintext(&c.corp_id)?),
                        encode(&secret)
                    ),
                    None,
                )?;
                if value["errcode"].as_i64() != Some(0) {
                    return Err(format!("获取凭据失败：{}", value["errcode"]));
                }
                self.token = value["access_token"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .ok_or("响应缺少凭据")?
                    .into();
                self.expires = Some(
                    Instant::now()
                        + Duration::from_secs(
                            value["expires_in"]
                                .as_u64()
                                .unwrap_or(0)
                                .saturating_sub(60)
                                .min(7200),
                        ),
                );
            }
            let response = http(
                &format!("/cgi-bin/message/send?access_token={}", encode(&self.token)),
                Some(
                    &serde_json::json!({"touser":plaintext(&c.recipients)?,"msgtype":"text","agentid":plaintext(&c.agent_id)?.parse::<u32>().map_err(|_|"应用ID无效")?,"text":{"content":text}}),
                ),
            )?;
            let code = response["errcode"].as_i64();
            if attempt == 0 && matches!(code, Some(40014 | 42001)) {
                self.expires = None;
                continue;
            }
            return if code == Some(0) && response["invaliduser"].as_str().unwrap_or("").is_empty() {
                Ok(())
            } else {
                Err(format!(
                    "企业微信发送失败或部分成员无效：{}",
                    response["errcode"]
                ))
            };
        }
        Err("企业微信凭据失效".into())
    }
}
#[derive(Default)]
pub struct Status {
    pending: usize,
    sent: usize,
    skipped: usize,
    failed: usize,
    detail: String,
}
#[derive(Default)]
pub struct Runtime {
    cancel: Option<Arc<AtomicBool>>,
    pub status: Arc<Mutex<Status>>,
    config: Option<MessageConfig>,
    thread: Option<std::thread::JoinHandle<()>>,
    test_cancel: Arc<AtomicBool>,
}
impl Runtime {
    pub fn text(&self) -> String {
        let s = self.status.lock().unwrap();
        format!(
            "待发 {} · 成功 {} · 跳过 {} · 失败 {}\n{}",
            s.pending, s.sent, s.skipped, s.failed, s.detail
        )
    }
    pub fn stop(&mut self) {
        self.test_cancel.store(true, Ordering::SeqCst);
        if let Some(c) = self.cancel.take() {
            c.store(true, Ordering::SeqCst);
        }
    }
    pub fn configure(&mut self, c: MessageConfig, host: HWND) {
        if self.config.as_ref() == Some(&c) {
            return;
        }
        self.stop();
        self.test_cancel = Arc::new(AtomicBool::new(false));
        self.config = Some(c.clone());
        self.status = Arc::new(Mutex::new(Status {
            detail: if c.enabled {
                "正在连接通知数据库"
            } else {
                "传讯已关闭"
            }
            .into(),
            ..Default::default()
        }));
        if !c.enabled {
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel = Some(cancel.clone());
        let status = self.status.clone();
        let host = host as usize;
        let previous = self.thread.take();
        self.thread = Some(std::thread::spawn(move || {
            if let Some(previous) = previous {
                let _ = previous.join();
            }
            run(c, cancel, status, host);
        }));
    }
    pub fn test(&self, c: MessageConfig, host: HWND) {
        let cancel = self.test_cancel.clone();
        let status = self.status.clone();
        let host = host as usize;
        std::thread::spawn(move || {
            let result = Sender::default().send_with(
                &c,
                "遁鱼传讯测试\n这是一条测试消息。",
                |path, body| {
                    if cancel.load(Ordering::SeqCst) {
                        Err("传讯已停止".into())
                    } else {
                        request(path, body)
                    }
                },
            );
            status.lock().unwrap().detail = match result {
                Ok(()) => "测试消息已发送".into(),
                Err(e) => e,
            };
            unsafe {
                PostMessageW(handle(host), UPDATE, 0, 0);
            }
        });
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        self.stop();
    }
}
fn run(c: MessageConfig, cancel: Arc<AtomicBool>, status: Arc<Mutex<Status>>, host: usize) {
    let update = |detail: String| {
        status.lock().unwrap().detail = detail;
        unsafe {
            PostMessageW(handle(host), UPDATE, 0, 0);
        }
    };
    if let Err(e) = validate_credentials(&c) {
        update(e);
        return;
    }
    let mut db = None;
    while !cancel.load(Ordering::SeqCst) {
        let result = (|| {
            let path = std::env::var_os("LOCALAPPDATA").ok_or("缺少LOCALAPPDATA")?;
            let store = Store::open(
                &PathBuf::from(path).join("Microsoft/Windows/Notifications/wpndatabase.db"),
            )
            .map_err(|_| "无法读取通知数据库")?;
            let max = store.maximum().map_err(|_| "无法初始化通知游标")?;
            Ok::<_, &str>((store, max))
        })();
        match result {
            Ok(value) => {
                db = Some(value);
                break;
            }
            Err(e) => update(e.into()),
        }
        if wait(&cancel, Duration::from_secs(c.poll_seconds.into())) {
            return;
        }
    }
    let Some((store, cursor)) = db else {
        return;
    };
    let mut sender = Sender::default();
    monitor(
        c,
        cancel,
        status,
        host,
        store,
        cursor,
        move |config, text, cancel| {
            sender.send_with(config, text, |path, body| {
                if cancel.load(Ordering::SeqCst) {
                    Err("传讯已停止".into())
                } else {
                    request(path, body)
                }
            })
        },
    );
}
#[allow(clippy::too_many_arguments)]
fn monitor(
    c: MessageConfig,
    cancel: Arc<AtomicBool>,
    status: Arc<Mutex<Status>>,
    host: usize,
    store: Store,
    mut cursor: i64,
    mut send: impl FnMut(&MessageConfig, &str, &AtomicBool) -> Result<(), String> + Send + 'static,
) {
    let update = |detail: String| {
        status.lock().unwrap().detail = detail;
        unsafe {
            PostMessageW(handle(host), UPDATE, 0, 0);
        }
    };
    let store = Arc::new(Mutex::new(store));
    let (tx, rx) = mpsc::sync_channel::<(i64, String, Instant)>(100);
    let worker_store = store.clone();
    let worker_cancel = cancel.clone();
    let worker_status = status.clone();
    let sender_config = c.clone();
    let worker = std::thread::spawn(move || {
        while !worker_cancel.load(Ordering::SeqCst) {
            let (id, text, due) = match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(v) => v,
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(_) => break,
            };
            if wait(
                &worker_cancel,
                due.saturating_duration_since(Instant::now()),
            ) {
                break;
            }
            let exists = worker_store.lock().unwrap().exists(id);
            if worker_cancel.load(Ordering::SeqCst) {
                break;
            }
            let result = match exists {
                Ok(true) => send(&sender_config, &text, &worker_cancel).map(|_| true),
                Ok(false) => Ok(false),
                Err(_) => Err("通知复查失败，已跳过".into()),
            };
            let mut s = worker_status.lock().unwrap();
            s.pending = s.pending.saturating_sub(1);
            match result {
                Ok(true) => s.sent += 1,
                Ok(false) => s.skipped += 1,
                Err(e) => {
                    s.failed += 1;
                    s.detail = e;
                }
            }
            unsafe {
                PostMessageW(handle(host), UPDATE, 0, 0);
            }
        }
    });
    update("正在监听；通知仍存在时转发".into());
    while !cancel.load(Ordering::SeqCst) {
        let rows = store.lock().unwrap().scan(cursor);
        match rows {
            Err(_) => update("通知数据库暂不可读，稍后重试".into()),
            Ok(rows) => {
                for (id, payload) in rows {
                    if cancel.load(Ordering::SeqCst) {
                        break;
                    }
                    if !c.matches(&String::from_utf8_lossy(&payload)) {
                        cursor = id;
                        continue;
                    }
                    match dunyu::message::content(&payload) {
                        Ok(text) => {
                            let mut s = status.lock().unwrap();
                            match tx.try_send((
                                id,
                                text,
                                Instant::now() + Duration::from_secs(c.delay_seconds.into()),
                            )) {
                                Ok(()) => {
                                    s.pending += 1;
                                    cursor = id;
                                }
                                Err(_) => break,
                            }
                        }
                        Err(_) => {
                            status.lock().unwrap().skipped += 1;
                            cursor = id;
                        }
                    }
                }
            }
        }
        unsafe {
            PostMessageW(handle(host), UPDATE, 0, 0);
        }
        if wait(&cancel, Duration::from_secs(c.poll_seconds.into())) {
            break;
        }
    }
    drop(tx);
    let _ = worker.join();
}
fn wait(cancel: &AtomicBool, duration: Duration) -> bool {
    let end = Instant::now() + duration;
    while !cancel.load(Ordering::SeqCst) {
        let remaining = end.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return false;
        }
        std::thread::sleep(remaining.min(Duration::from_millis(100)));
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn protected_secret_and_delivery_errors() {
        let secret = protect(b"unit-test-secret", false).unwrap();
        assert_ne!(secret, b"unit-test-secret");
        assert_eq!(protect(&secret, true).unwrap(), b"unit-test-secret");
        assert!(protect(b"broken", true).is_err());
        let c = MessageConfig {
            secret: ProtectedValue::Encrypted(secret),
            corp_id: encrypted("corp").unwrap(),
            agent_id: encrypted("1").unwrap(),
            recipients: encrypted("@all").unwrap(),
            ..Default::default()
        };
        assert!(
            !serde_json::to_string(&c)
                .unwrap()
                .contains("unit-test-secret")
        );
        let mut calls = 0;
        Sender::default()
            .send_with(&c, "test", |path, body| {
                calls += 1;
                if path.contains("gettoken") {
                    Ok(serde_json::json!({"errcode":0,"access_token":"token","expires_in":7200}))
                } else {
                    assert_eq!(body.unwrap()["touser"], "@all");
                    Ok(serde_json::json!({"errcode":if calls==2 {42001}else{0}}))
                }
            })
            .unwrap();
        assert_eq!(calls, 4);
        assert!(
            Sender::default()
                .send_with(&c, "test", |_, _| Err("timeout".into()))
                .is_err()
        );
        assert!(
            Sender::default()
                .send_with(&c, "test", |_, _| Ok(serde_json::json!({"errcode":40013})))
                .is_err()
        );
    }
    #[test]
    fn cancelled_wait_is_immediate() {
        let cancel = AtomicBool::new(true);
        let start = Instant::now();
        assert!(wait(&cancel, Duration::from_secs(3600)));
        assert!(start.elapsed() < Duration::from_secs(1));
    }
}

#[cfg(test)]
mod queue_tests {
    use super::*;
    #[test]
    fn delayed_recheck_history_and_backpressure() {
        let path =
            std::env::temp_dir().join(format!("dunyu-message-queue-{}.db", std::process::id()));
        let writer = rusqlite::Connection::open(&path).unwrap();
        writer.execute_batch("PRAGMA journal_mode=WAL;CREATE TABLE Notification(id INTEGER PRIMARY KEY, Payload BLOB, type TEXT);").unwrap();
        let payload = b"<toast><text>Telegram</text><text>hello</text></toast>";
        for id in 1..=105 {
            writer
                .execute(
                    "INSERT INTO Notification VALUES(?,?,'toast')",
                    rusqlite::params![id, payload.as_slice()],
                )
                .unwrap();
        }
        let store = Store::open(&path).unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let stop = cancel.clone();
        let status = Arc::new(Mutex::new(Status::default()));
        let stats = status.clone();
        let (tx, rx) = mpsc::channel();
        let c = MessageConfig {
            poll_seconds: 1,
            delay_seconds: 1,
            ..Default::default()
        };
        let thread = std::thread::spawn(move || {
            monitor(c, stop, stats, 0, store, 1, move |_, text, _| {
                tx.send(text.to_string()).unwrap();
                Ok(())
            })
        });
        let deadline = Instant::now() + Duration::from_secs(15);
        while status.lock().unwrap().pending == 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        writer
            .execute("DELETE FROM Notification WHERE id=2", [])
            .unwrap();
        let mut sent = 0;
        while sent < 103 && Instant::now() < deadline {
            if rx.recv_timeout(Duration::from_millis(100)).is_ok() {
                sent += 1;
            }
        }
        cancel.store(true, Ordering::SeqCst);
        thread.join().unwrap();
        assert_eq!(
            sent, 103,
            "queue overflow must not lose rows; history and removed rows must not send"
        );
        assert_eq!(status.lock().unwrap().skipped, 1);
        drop(writer);
        std::fs::remove_file(path).unwrap();
    }
}

#[cfg(test)]
mod migration_tests {
    use super::*;
    #[test]
    fn migration_preserves_secret_and_is_idempotent() {
        let secret = encrypted("existing-secret").unwrap();
        let mut c = MessageConfig {
            corp_id: ProtectedValue::Legacy("existing-corp".into()),
            agent_id: ProtectedValue::Legacy("42".into()),
            recipients: ProtectedValue::Legacy("existing-user".into()),
            secret: secret.clone(),
            ..Default::default()
        };
        assert!(migrate(&mut c).unwrap());
        assert_eq!(c.secret, secret);
        assert_eq!(plaintext(&c.corp_id).unwrap(), "existing-corp");
        assert_eq!(plaintext(&c.agent_id).unwrap(), "42");
        assert_eq!(plaintext(&c.recipients).unwrap(), "existing-user");
        validate_credentials(&c).unwrap();
        let before = c.clone();
        assert!(!migrate(&mut c).unwrap());
        assert_eq!(before, c);
        let json = serde_json::to_string(&c).unwrap();
        assert!(!json.contains("existing-"));
        c.agent_id = encrypted("not-a-number").unwrap();
        assert!(validate_credentials(&c).is_err());
        c.corp_id = ProtectedValue::Encrypted(vec![1, 2, 3]);
        assert!(validate_credentials(&c).is_err());
    }
}
