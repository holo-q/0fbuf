use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{create_dir_all, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowMeta {
    pub pid: u32,
    pub session: Option<String>,
    pub created_at: String,
    /// If true, this window has been popped from the buffer and is now independent
    #[serde(default)]
    pub popped: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StateFile {
    pub windows: BTreeMap<u32, WindowMeta>, // key: XID
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InputState {
    pub move_flag: bool,
    pub pointer_x: i32,
    pub pointer_y: i32,
    pub last_mouse_at: Option<String>,
    pub last_key_at: Option<String>,
}

pub fn xdg_state_dir() -> PathBuf {
    if let Some(p) = std::env::var_os("XDG_STATE_HOME") {
        return PathBuf::from(p).join("0fbuf");
    }
    let base = dirs::home_dir().expect("HOME not set");
    base.join(".local/state/0fbuf")
}

pub fn state_path() -> PathBuf { xdg_state_dir().join("state.json") }
pub fn input_path() -> PathBuf { xdg_state_dir().join("input.json") }

pub fn load_state(path: &Path) -> StateFile {
    let mut s = StateFile::default();
    if let Ok(mut f) = File::open(path) {
        let mut buf = String::new();
        if f.read_to_string(&mut buf).is_ok() {
            if let Ok(parsed) = serde_json::from_str::<StateFile>(&buf) {
                s = parsed;
            }
        }
    }
    s
}

pub fn save_state(path: &Path, st: &StateFile) -> Result<()> {
    if let Some(dir) = path.parent() { create_dir_all(dir)?; }
    // Atomic write: write to temp file, then rename
    let tmp = path.with_extension("tmp");
    let mut f = File::create(&tmp)?;
    let data = serde_json::to_string_pretty(st)?;
    f.write_all(data.as_bytes())?;
    f.sync_all()?;
    drop(f);
    std::fs::rename(&tmp, path)?;
    Ok(())
}

pub fn load_input(path: &Path) -> InputState {
    let mut s = InputState::default();
    if let Ok(mut f) = File::open(path) {
        let mut buf = String::new();
        if f.read_to_string(&mut buf).is_ok() {
            if let Ok(parsed) = serde_json::from_str::<InputState>(&buf) {
                s = parsed;
            }
        }
    }
    s
}

pub fn save_input(path: &Path, st: &InputState) -> Result<()> {
    if let Some(dir) = path.parent() { create_dir_all(dir)?; }
    // Atomic write: write to temp file, then rename
    let tmp = path.with_extension("tmp");
    let mut f = File::create(&tmp)?;
    let data = serde_json::to_string_pretty(st)?;
    f.write_all(data.as_bytes())?;
    f.sync_all()?;
    drop(f);
    std::fs::rename(&tmp, path)?;
    Ok(())
}

pub fn now_rfc3339() -> String {
    OffsetDateTime::now_utc().format(&Rfc3339).unwrap_or_else(|_| "now".into())
}

/// Remove XIDs from state that no longer exist in the provided valid_xids set
pub fn cleanup_stale_state(st: &mut StateFile, valid_xids: &[u32]) {
    st.windows.retain(|xid, _| valid_xids.contains(xid));
}

/// Mark a window as popped from the buffer (becomes independent)
pub fn mark_popped(xid: u32) -> Result<()> {
    let path = state_path();
    let mut st = load_state(&path);
    if let Some(meta) = st.windows.get_mut(&xid) {
        meta.popped = true;
    }
    save_state(&path, &st)?;
    Ok(())
}

