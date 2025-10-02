use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

pub const DEFAULT_CLASS: &str = "0fbuf";
pub const DEFAULT_MIN: usize = 5;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolConfig {
    pub name: String,
    pub class: String,
    pub min: usize,
    pub spawn_cmd: String,
    pub pool_desktop: Option<u32>,
    pub reset_via_tmux: bool,
    pub reset_keys: Option<String>,
    /// Commands to run after spawning (executed via shell)
    /// Supports same placeholders as spawn_cmd: {class}, {session}, {xid}
    #[serde(default)]
    pub on_spawn: Vec<String>,
    /// Anti-flicker strategy: "none", "all", "unmap", "offscreen", "utility", "iconic", "lower", "combo1"
    #[serde(default = "default_antiflicker")]
    pub antiflicker_strategy: String,
}

fn default_antiflicker() -> String {
    "all".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputConfig {
    pub mouse_recent_ms: u64,
    pub key_suppress_ms: u64,
    pub input_poll_ms: u64,
    pub ensure_interval_ms: u64,
    pub motion_eps: i16,
}

impl Default for InputConfig {
    fn default() -> Self {
        Self {
            mouse_recent_ms: 500,
            key_suppress_ms: 400,
            input_poll_ms: 50,
            ensure_interval_ms: 800,
            motion_eps: 1,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    pub pools: BTreeMap<String, PoolConfig>,
    #[serde(default)]
    pub input: InputConfig,
}

/// Detect which terminals are installed and return the best one
fn detect_terminal() -> &'static str {
    let terminals = [
        ("kitty", "kitty --class {class} --title {class} tmux new -A -s {session}"),
        ("alacritty", "alacritty --class {class} -t {class} -e tmux new -A -s {session}"),
        ("wezterm", "wezterm start --class {class} -- tmux new -A -s {session}"),
        ("foot", "foot --app-id={class} --title={class} tmux new -A -s {session}"),
        ("st", "st -c {class} -t {class} -e tmux new -A -s {session}"),
        ("termite", "termite --class={class} --title={class} -e 'tmux new -A -s {session}'"),
        ("urxvt", "urxvt -name {class} -title {class} -e tmux new -A -s {session}"),
        ("gnome-terminal", "gnome-terminal --class={class} --title={class} -- tmux new -A -s {session}"),
        ("konsole", "konsole --new-tab -p tabtitle={class} -e tmux new -A -s {session}"),
        ("xfce4-terminal", "xfce4-terminal --class {class} --title {class} --disable-server --hide-menubar -e 'tmux new -A -s {session}'"),
        ("tilix", "tilix --class={class} --title={class} -e 'tmux new -A -s {session}'"),
        ("terminology", "terminology --class={class} --title={class} -e 'tmux new -A -s {session}'"),
        ("xterm", "xterm -class {class} -T {class} -e tmux new -A -s {session}"),
    ];

    for (cmd, template) in &terminals {
        if std::process::Command::new("which")
            .arg(cmd)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return template;
        }
    }

    // Ultimate fallback
    "xterm -class {class} -T {class} -e tmux new -A -s {session}"
}

pub fn default_single_config() -> Config {
    let detected_terminal = detect_terminal();

    let mut pools = BTreeMap::new();

    // Shell pool - auto-detected terminal
    pools.insert(
        "shell".into(),
        PoolConfig {
            name: "shell".into(),
            class: "0fbuf-shell".into(),
            min: 3,
            spawn_cmd: detected_terminal.into(),
            pool_desktop: None,
            reset_via_tmux: true,
            reset_keys: Some("C-c reset Enter".into()),
            on_spawn: vec![],
            antiflicker_strategy: "all".into(),
        },
    );

    // Quick scratch terminal
    pools.insert(
        "scratch".into(),
        PoolConfig {
            name: "scratch".into(),
            class: "0fbuf-scratch".into(),
            min: 1,
            spawn_cmd: detected_terminal.into(),
            pool_desktop: None,
            reset_via_tmux: true,
            reset_keys: Some("C-c reset Enter".into()),
            on_spawn: vec![],
            antiflicker_strategy: "all".into(),
        },
    );

    Config { pools, input: InputConfig::default() }
}

pub fn get_config_path(path: Option<&Path>) -> PathBuf {
    let p_opt = path.map(PathBuf::from).or_else(|| {
        std::env::var_os("OFB_CONFIG").map(PathBuf::from).or_else(|| {
            if let Some(home) = dirs::home_dir() {
                // Prefer .toml, then .json (check both config and pools names)
                let config_toml = home.join(".config/0fbuf/config.toml");
                let pools_toml = home.join(".config/0fbuf/pools.toml");
                let config_json = home.join(".config/0fbuf/config.json");
                let pools_json = home.join(".config/0fbuf/pools.json");

                if config_toml.exists() {
                    Some(config_toml)
                } else if pools_toml.exists() {
                    Some(pools_toml)
                } else if config_json.exists() {
                    Some(config_json)
                } else if pools_json.exists() {
                    Some(pools_json)
                } else {
                    // Default to pools.toml for new configs
                    Some(pools_toml)
                }
            } else {
                None
            }
        })
    });

    p_opt.map(|p| expand_tilde(&p)).unwrap_or_else(|| {
        // Default to pools.toml if no existing config found
        dirs::home_dir()
            .map(|h| h.join(".config/0fbuf/pools.toml"))
            .unwrap_or_else(|| PathBuf::from("pools.toml"))
    })
}

pub fn load_config(path: Option<&Path>) -> Option<Config> {
    let path = get_config_path(path);
    let mut f = File::open(&path).ok()?;
    let mut buf = String::new();
    f.read_to_string(&mut buf).ok()?;

    // Try TOML first, fall back to JSON
    if path.extension().and_then(|s| s.to_str()) == Some("toml") {
        toml::from_str::<Config>(&buf).ok()
    } else {
        serde_json::from_str::<Config>(&buf).ok()
    }
}

fn expand_tilde(p: &Path) -> PathBuf {
    if let Some(s) = p.to_str() {
        if s.starts_with("~/") {
            if let Some(h) = dirs::home_dir() {
                return PathBuf::from(s.replacen("~", &h.to_string_lossy(), 1));
            }
        }
    }
    p.to_path_buf()
}
