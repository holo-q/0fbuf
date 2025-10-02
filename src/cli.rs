use clap::{Parser, Subcommand, Args};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "0fbuf", author, version, about = "Zero-frame terminal buffer for XFCE/X11", long_about = None)]
pub struct Cli {
    /// Path to pools JSON config (defaults to ~/.config/0fbuf/pools.json)
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,

    /// Ignore config files and use only CLI defaults/overrides
    #[arg(long, global = true, default_value_t = false)]
    pub no_config: bool,

    /// Default pool name when none specified
    #[arg(long, global = true, default_value = "default")]
    pub default_pool: String,

    /// CLI overrides for the default (or named) pool
    #[command(flatten)]
    pub pool_opts: PoolOpts,

    /// CLI overrides for input heuristics and scheduling
    #[command(flatten)]
    pub input_opts: InputOpts,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Run background daemon (pool maintainer + input monitor)
    Daemon,

    /// Activate a ready window from a pool
    Activate(ActivateArgs),

    /// Legacy toggle behaviour (hide/show)
    Toggle(PoolArg),

    /// Scratchpad-style window management
    Scratchpad(ScratchpadArgs),

    /// Ensure pool is topped up
    Ensure(PoolArg),

    /// List pool windows
    List(PoolArg),

    /// Reset tmux sessions for a pool
    Reset(PoolArg),

    /// Print input monitor status (move flag, pointer)
    Status,

    /// Edit configuration file in $EDITOR
    Config,

    /// List available anti-flicker strategies
    Strategies,
}

#[derive(Args, Debug)]
pub struct PoolArg {
    /// Pool name
    pub pool: Option<String>,
}

#[derive(Args, Debug)]
pub struct ActivateArgs {
    /// Pool name
    pub pool: Option<String>,

    /// Prefer positioning near the mouse if the daemon indicates mouse mode
    #[arg(long)]
    pub mouse: bool,
}

#[derive(Args, Debug)]
pub struct ScratchpadArgs {
    /// Pool name
    pub pool: Option<String>,

    /// Action: show (bring visible), hide (send to pool), toggle (auto), cycle (next window)
    #[arg(long, default_value = "toggle")]
    pub action: ScratchpadAction,

    /// Prefer positioning near the mouse if the daemon indicates mouse mode
    #[arg(long)]
    pub mouse: bool,
}

#[derive(Debug, Clone, clap::ValueEnum)]
pub enum ScratchpadAction {
    Show,
    Hide,
    Toggle,
    Cycle,
}

#[derive(Args, Debug, Default)]
pub struct PoolOpts {
    /// Override: pool name to use/define when --no-config
    #[arg(long)]
    pub pool_name: Option<String>,

    /// Override: WM_CLASS to match
    #[arg(long)]
    pub class: Option<String>,

    /// Override: minimum warm windows to keep
    #[arg(long)]
    pub min: Option<usize>,

    /// Override: spawn command template (supports {class},{session})
    #[arg(long = "cmd")]
    pub spawn_cmd: Option<String>,

    /// Override: pool desktop index (0-based); default last workspace
    #[arg(long = "pool-desktop")]
    pub pool_desktop: Option<u32>,

    /// Disable tmux reset logic for this pool
    #[arg(long = "no-reset", default_value_t = false)]
    pub no_reset: bool,

    /// Keys to send on reset (tmux send-keys syntax)
    #[arg(long = "reset-keys")]
    pub reset_keys: Option<String>,

    /// Anti-flicker strategy: none, all, unmap, offscreen, utility, iconic, lower, combo1, combo2, combo3
    #[arg(long = "antiflicker")]
    pub antiflicker_strategy: Option<String>,
}

#[derive(Args, Debug, Default)]
pub struct InputOpts {
    /// Mouse considered "recent" if moved within this many ms
    #[arg(long = "mouse-recent-ms")]
    pub mouse_recent_ms: Option<u64>,

    /// Keyboard suppresses mouse mode if a non-modifier key within this many ms
    #[arg(long = "key-suppress-ms")]
    pub key_suppress_ms: Option<u64>,

    /// Input sampling and status write interval (ms)
    #[arg(long = "input-poll-ms")]
    pub input_poll_ms: Option<u64>,

    /// Pool ensure interval (ms)
    #[arg(long = "ensure-interval-ms")]
    pub ensure_interval_ms: Option<u64>,

    /// Motion epsilon in pixels
    #[arg(long = "motion-eps")]
    pub motion_eps: Option<i16>,
}
