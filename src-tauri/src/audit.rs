//! 读取记录：谁、什么时候、从哪读了哪个条目名。
//!
//! 目的只有一个——**出事之后能查到**。所以这里的每一条都尽量来自**服务端**，
//! 而不是调用方自己的说法：
//!
//! - `at` / `pid` / `process`：服务端自己取的（系统时间、`GetNamedPipeClientProcessId`、
//!   再由 PID 查进程名）。调用方改不了。
//! - `name` / `project` / `environment` / `cwd` / `client`：**调用方自报**，
//!   只能当线索，不能当证据。要看这几列，请对照 `process` 一起看。
//!
//! 边界（重要）：
//!
//! - 管道 ACL 只允许当前用户和 SYSTEM，所以「调用方」永远是这台机器上的某个进程，
//!   **回答不了「是不是你本人」**，也拦不住它。这是事后追查工具，不是防护工具。
//! - 只记**条目名**，绝不记密钥值。
//! - 只放内存，不落盘：重启即失，也不会有明文元数据留在磁盘上。
//!   管道只在应用运行时存在，所以这个取舍几乎不损失什么。
//! - 锁定时读不到：条目名和未锁定时能看到的名称是同一类信息，锁着就不给。

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// 内存里最多留多少条。超了丢最旧的。
pub const MAX_EVENTS: usize = 200;

/// 一次读取（或一次读取尝试）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEvent {
    /// 服务端时间，Unix 毫秒。
    pub at: i64,
    /// 服务端的本地时间，形如 `2026-11-12 10:30:15`，给人看的。
    pub at_local: String,
    /// 客户端进程 PID，服务端从管道句柄直接问内核要，调用方改不了。
    pub pid: Option<u32>,
    /// 客户端进程名，由 PID 反查；查不到就是 `None`。
    pub process: Option<String>,
    /// 被请求的**条目名**（不是值）。
    pub name: String,
    /// 调用方自报的项目。
    pub project: Option<String>,
    /// 调用方自报的环境。
    pub environment: Option<String>,
    /// 调用方自报的工作目录。
    pub cwd: Option<String>,
    /// 调用方自报的程序标识（例如 `lvault`）。
    pub client: Option<String>,
    /// 结果：`ok` / `not_found` / `disabled` / `vault_locked` …
    pub outcome: String,
}

impl AuditEvent {
    /// 现在这一刻，客户端还没说话之前的时间戳。
    pub fn now() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis() as i64)
            .unwrap_or(0)
    }
}

/// 服务端的本地时间。记录发生在服务端，所以时间也以服务端为准。
#[cfg(windows)]
pub fn local_time_string() -> String {
    #[repr(C)]
    struct WinSystemTime {
        year: u16,
        month: u16,
        day_of_week: u16,
        day: u16,
        hour: u16,
        minute: u16,
        second: u16,
        milliseconds: u16,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetLocalTime(systemtime: *mut WinSystemTime) -> i32;
    }

    unsafe {
        let mut time = WinSystemTime {
            year: 0,
            month: 0,
            day_of_week: 0,
            day: 0,
            hour: 0,
            minute: 0,
            second: 0,
            milliseconds: 0,
        };
        GetLocalTime(&mut time);
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            time.year, time.month, time.day, time.hour, time.minute, time.second
        )
    }
}

#[cfg(not(windows))]
pub fn local_time_string() -> String {
    String::new()
}

/// 环形缓冲：只留最近 [`MAX_EVENTS`] 条。
#[derive(Debug, Default)]
pub struct AuditLog {
    events: Mutex<VecDeque<AuditEvent>>,
}

impl AuditLog {
    pub fn new() -> Self {
        Self {
            events: Mutex::new(VecDeque::new()),
        }
    }

    /// 记一条。记录本身失败绝不能影响读取主流程，所以这里吞掉所有错误。
    pub fn record(&self, event: AuditEvent) {
        let Ok(mut events) = self.events.lock() else {
            return;
        };
        if events.len() >= MAX_EVENTS {
            events.pop_front();
        }
        events.push_back(event);
    }

    /// 取最近的记录，**新的在前**。`limit` 为 0 表示不限。
    pub fn events(&self, limit: usize) -> Vec<AuditEvent> {
        let Ok(events) = self.events.lock() else {
            return Vec::new();
        };
        let mut collected: Vec<AuditEvent> = events.iter().rev().cloned().collect();
        if limit > 0 && collected.len() > limit {
            collected.truncate(limit);
        }
        collected
    }
}

/// 由 PID 反查进程名（只取文件名，不含路径）。
#[cfg(windows)]
pub fn process_name(pid: u32) -> Option<String> {
    use std::ffi::c_void;

    // PROCESS_QUERY_LIMITED_INFORMATION：足够查可执行文件路径，不需要 PROCESS_ALL_ACCESS。
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;

    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(desired_access: u32, inherit_handle: i32, process_id: u32) -> *mut c_void;
        fn CloseHandle(object: *mut c_void) -> i32;
        fn QueryFullProcessImageNameW(
            process: *mut c_void,
            flags: u32,
            exe_name: *mut u16,
            size: *mut u32,
        ) -> i32;
    }

    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return None;
        }
        let mut buffer = vec![0u16; 260];
        let mut size = buffer.len() as u32;
        let queried = QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut size);
        CloseHandle(handle);
        if queried == 0 {
            return None;
        }
        let path = String::from_utf16_lossy(&buffer[..size as usize]);
        let name = path.rsplit(['\\', '/']).next().unwrap_or("").trim();
        if name.is_empty() {
            None
        } else {
            Some(name.to_owned())
        }
    }
}

#[cfg(not(windows))]
pub fn process_name(_pid: u32) -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(name: &str, outcome: &str) -> AuditEvent {
        AuditEvent {
            at: AuditEvent::now(),
            at_local: local_time_string(),
            pid: Some(4242),
            process: Some("demo.exe".to_owned()),
            name: name.to_owned(),
            project: None,
            environment: None,
            cwd: None,
            client: Some("lvault".to_owned()),
            outcome: outcome.to_owned(),
        }
    }

    #[test]
    fn keeps_the_newest_events_first() {
        let log = AuditLog::new();
        log.record(event("FIRST", "ok"));
        log.record(event("SECOND", "ok"));
        let events = log.events(0);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].name, "SECOND");
        assert_eq!(events[1].name, "FIRST");
    }

    #[test]
    fn honours_the_limit() {
        let log = AuditLog::new();
        for index in 0..5 {
            log.record(event(&format!("KEY{index}"), "ok"));
        }
        let events = log.events(2);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].name, "KEY4");
        assert_eq!(events[1].name, "KEY3");
    }

    #[test]
    fn drops_the_oldest_when_full() {
        let log = AuditLog::new();
        for index in 0..(MAX_EVENTS + 10) {
            log.record(event(&format!("KEY{index}"), "ok"));
        }
        let events = log.events(0);
        assert_eq!(events.len(), MAX_EVENTS);
        assert_eq!(events[0].name, format!("KEY{}", MAX_EVENTS + 9));
        assert_eq!(
            events[MAX_EVENTS - 1].name,
            format!("KEY{}", MAX_EVENTS + 9 - (MAX_EVENTS - 1))
        );
    }

    #[test]
    fn never_stores_a_secret_value() {
        let log = AuditLog::new();
        let mut event = event("API_KEY", "ok");
        event.project = Some("web-app".to_owned());
        log.record(event);
        let text = format!("{:?}", log.events(0));
        assert!(!text.contains("sk-live"));
    }

    #[test]
    fn resolves_our_own_process_name() {
        // The test binary is a real running process, so this exercises the same
        // path a client connection takes.
        let pid = std::process::id();
        let name = process_name(pid);
        if cfg!(windows) {
            let name = name.expect("应当能查到自己的进程名");
            assert!(name.to_lowercase().ends_with(".exe"), "{name}");
        }
    }

    #[test]
    fn unknown_pid_has_no_name() {
        // PID 0 is the system idle process and cannot be opened with the
        // limited query right; either way we must not panic.
        let _ = process_name(0);
    }
}
