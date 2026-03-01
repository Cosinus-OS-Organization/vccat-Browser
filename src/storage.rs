use std::path::PathBuf;
use std::fs;
use serde::{Deserialize, Serialize};

pub fn data_dir() -> PathBuf {
    let base = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
    let d = base.join("vccat-browser");
    fs::create_dir_all(&d).ok();
    d
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Session {
    pub tabs: Vec<String>,
    pub active: usize,
}

pub fn save_session(session: &Session) {
    if let Ok(s) = serde_json::to_string_pretty(session) {
        fs::write(data_dir().join("session.json"), s).ok();
    }
}

pub fn load_session() -> Session {
    let path = data_dir().join("session.json");
    if path.exists() {
        if let Ok(s) = fs::read_to_string(&path) {
            if let Ok(sess) = serde_json::from_str::<Session>(&s) {
                if !sess.tabs.is_empty() { return sess; }
            }
        }
    }
    Session { tabs: vec!["vccat:home".into()], active: 0 }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HistoryEntry {
    pub url: String,
    pub title: String,
    pub timestamp: u64,
}

pub fn load_history() -> Vec<HistoryEntry> {
    let path = data_dir().join("history.json");
    if path.exists() {
        if let Ok(s) = fs::read_to_string(&path) {
            if let Ok(h) = serde_json::from_str::<Vec<HistoryEntry>>(&s) {
                return h;
            }
        }
    }
    vec![]
}

pub fn save_history(history: &[HistoryEntry]) {
    let slice = if history.len() > 5000 { &history[history.len()-5000..] } else { history };
    if let Ok(s) = serde_json::to_string_pretty(slice) {
        fs::write(data_dir().join("history.json"), s).ok();
    }
}

pub fn append_history(history: &mut Vec<HistoryEntry>, url: &str, title: &str) {
    if url == "about:blank" || url.starts_with("vccat:") || url.is_empty() { return; }
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs()).unwrap_or(0);
    if let Some(last) = history.last() { if last.url == url { return; } }
    history.push(HistoryEntry { url: url.into(), title: title.into(), timestamp: ts });
    save_history(history);
}

pub fn webview_data_dir() -> PathBuf {
    let d = data_dir().join("webview-data");
    fs::create_dir_all(&d).ok();
    d
}