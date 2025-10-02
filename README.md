# 0fbuf — Zero-Frame Application Buffer

**Instant application windows. Zero perceived latency.**

Pre-warm GUI applications on a hidden workspace and activate them instantly on keypress. No spawn delay, no initialization wait — your terminal/browser/calculator is already there.

## Why?

**The problem:** Pressing a hotkey to launch a terminal takes 100-500ms. You feel it every time.

**The solution:** Pre-spawn terminals in the background. When you press the hotkey, a window that's *already running* gets moved to your workspace and focused. **Zero frames of delay.**

Think of it like application pooling for your desktop — same concept as connection pooling for databases.

## Features

- 🚀 **Zero-frame activation** - Windows appear instantly, no spawn delay
- 🔍 **Auto-detects terminals** - Works out-of-the-box with 13+ terminals (kitty, alacritty, wezterm, foot, st, urxvt, gnome-terminal, konsole, xfce4-terminal, tilix, terminology, xterm)
- 🎯 **Application-agnostic** - Pool any GUI app: terminals, browsers, calculators, file managers
- 🪟 **i3-style scratchpad** - Toggle/show/hide/cycle windows
- 🎨 **Smart mouse placement** - Windows follow your cursor when mouse-driven
- ⚙️ **TOML config** - Human-friendly configuration with inline comments
- 🎬 **Command injection** - Pre-configure windows on spawn (cd to directories, run commands)
- 🧹 **Self-maintaining** - Auto-refills pools, cleans up stale state

## Quick Start

### Install
```bash
cargo build --release
cp target/release/0fbuf ~/.local/bin/0fbuf
```

### First Run (Auto-Configuration)
```bash
# Creates config with your installed terminal detected
0fbuf config
```

### Start Daemon
```bash
0fbuf daemon
```

You'll see verbose output showing what's configured:
```
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
  0fbuf daemon starting
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
[0fbuf] Connecting to X11...

[pools] (2 configured)
  shell (class: 0fbuf-shell, min: 3, desktop: 9)
    spawn: kitty --class {class} --title {class} tmux new -A -s {session}
  scratch (class: 0fbuf-scratch, min: 1, desktop: 9)
    spawn: kitty --class {class} --title {class} tmux new -A -s {session}

[0fbuf] Pre-warming pools...
  ✓ shell pool: 3 windows ready
  ✓ scratch pool: 1 windows ready

[0fbuf] Daemon running (Ctrl+C to stop)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

### Bind Keyboard Shortcuts

**XFCE:** Settings → Keyboard → Application Shortcuts

- `Super+Space` → `0fbuf scratchpad shell --mouse`
- `Super+C` → `0fbuf scratchpad scratch --mouse`

**Test it:** Press `Super+Space` — terminal appears instantly!

## Supported Terminals (Auto-Detected)

When you run `0fbuf config`, it auto-detects and configures the best terminal installed:

✅ **kitty** (preferred)
✅ **alacritty**
✅ **wezterm**
✅ **foot** (Wayland-native)
✅ **st** (suckless terminal)
✅ **termite**
✅ **urxvt**
✅ **gnome-terminal**
✅ **konsole** (KDE)
✅ **xfce4-terminal**
✅ **tilix**
✅ **terminology**
✅ **xterm** (fallback)

**No manual configuration needed** - it just works!

## Configuration

### Format: TOML (Recommended)

Config files (in order of preference):
- `~/.config/0fbuf/pools.toml` ← **new default**
- `~/.config/0fbuf/config.toml`
- `~/.config/0fbuf/pools.json` (legacy)
- `~/.config/0fbuf/config.json` (legacy)

Run `0fbuf config` to edit in `$VISUAL`/`$EDITOR`.

### Example Config

```toml
[input]
mouse_recent_ms = 500      # Mouse mode threshold
key_suppress_ms = 400      # Keyboard suppresses mouse mode
input_poll_ms = 50         # Sampling rate
ensure_interval_ms = 800   # Pool refill check interval
motion_eps = 1             # Motion detection sensitivity

# Shell pool - general terminal use
[pools.shell]
name = "shell"
class = "0fbuf-shell"
min = 3                    # Keep 3 terminals pre-warmed
spawn_cmd = "kitty --class {class} --title {class} tmux new -A -s {session}"
reset_via_tmux = true
on_spawn = []

# Dev environment - pre-configured for projects
[pools.dev]
name = "dev"
class = "0fbuf-dev"
min = 2
spawn_cmd = "kitty --class {class} tmux new -A -s {session}"
on_spawn = [
    "sleep 0.2 && xdotool type --window {xid} 'cd ~/projects' && xdotool key Return"
]

# Browser pool
[pools.browser]
name = "browser"
class = "0fbuf-browser"
min = 1
spawn_cmd = "firefox --class {class} --new-instance"
reset_via_tmux = false

# Calculator scratchpad
[pools.calc]
name = "calc"
class = "0fbuf-calc"
min = 1
spawn_cmd = "gnome-calculator"
reset_via_tmux = false
```

### Command Placeholders

- `{class}` - WM_CLASS name
- `{session}` - Unique session ID
- `{xid}` - Window X ID (for xdotool)

## Commands

### Core Commands
```bash
0fbuf daemon              # Start pool manager + input monitor
0fbuf config              # Edit config in $EDITOR (auto-creates if missing)
0fbuf activate <pool>     # Show a window from pool (always new visible window)
0fbuf scratchpad <pool>   # i3-style scratchpad (toggle/show/hide/cycle)
```

### Scratchpad Actions
```bash
0fbuf scratchpad shell --action toggle   # Show if hidden, hide if visible (default)
0fbuf scratchpad shell --action show     # Always bring one window
0fbuf scratchpad shell --action hide     # Hide all visible windows
0fbuf scratchpad shell --action cycle    # Rotate through pool windows
```

### Utility Commands
```bash
0fbuf ensure <pool>       # Top-up pool to minimum
0fbuf list <pool>         # Debug: show all pool windows
0fbuf status              # Debug: show input state
```

### Flags
- `--mouse` - Position window near cursor (if mouse mode active)

## XFCE Setup

### 1. Add Spare Workspace
Settings → Workspaces → Add a new workspace (for the pool)

### 2. Autostart Daemon

**Option A: systemd (recommended)**
```bash
mkdir -p ~/.config/systemd/user
cat > ~/.config/systemd/user/0fbuf.service << 'EOF'
[Unit]
Description=0fbuf daemon

[Service]
ExecStart=/home/YOUR_USER/.local/bin/0fbuf daemon
Restart=on-failure

[Install]
WantedBy=default.target
EOF

systemctl --user daemon-reload
systemctl --user enable --now 0fbuf.service
```

**Option B: Desktop autostart**
```bash
mkdir -p ~/.config/autostart
cat > ~/.config/autostart/0fbuf.desktop << 'EOF'
[Desktop Entry]
Type=Application
Name=0fbuf
Exec=0fbuf daemon
X-GNOME-Autostart-enabled=true
EOF
```

### 3. Compositor Tweaks (Optional)
For truly instant appearance, disable window animations:
- Settings → Window Manager Tweaks → Compositor → Disable all animations

## How It Works

1. **Pool Management**: Daemon maintains N pre-spawned windows on a hidden workspace
2. **Input Monitoring**: Tracks mouse/keyboard to determine placement mode
3. **Instant Activation**: On hotkey, moves existing window to current workspace and focuses it
4. **Auto-Refill**: When pool depletes, daemon spawns replacements
5. **Smart Placement**: If mouse was recently moved, positions window near cursor

### Technical Details

- Uses EWMH (`_NET_WM_DESKTOP`, `_NET_ACTIVE_WINDOW`) for window management
- State tracking via `~/.local/state/0fbuf/state.json` and `input.json`
- Input heuristics: mouse mode = motion in last 500ms AND no key in last 400ms
- Pool windows use unique WM_CLASS per pool for isolation

## Use Cases

**Scratchpad terminals:**
```bash
Super+Space → instant terminal for quick commands
```

**Pre-configured dev environments:**
```toml
[pools.rails]
spawn_cmd = "kitty tmux new -s {session}"
on_spawn = ["xdotool type --window {xid} 'cd ~/rails-app && vim' && xdotool key Return"]
```

**Instant browsers for research:**
```toml
[pools.browser]
min = 2
spawn_cmd = "firefox --new-instance --class {class}"
```

**Always-available calculator:**
```bash
Super+C → calculator appears instantly
```

## Limitations

- **X11 only** (Wayland support planned but needs testing)
- **Terminal close detection**: Can't prevent users from closing pooled terminals; daemon respawns replacements
- **Workspace requirement**: Needs one dedicated "pool" workspace

## FAQ

**Q: Why does it need a spare workspace?**
A: Pool windows are hidden there. Using workspace 9/10 keeps them out of Alt+Tab but alive.

**Q: Does it work with Wayland?**
A: Code is written for it but untested. Contributions welcome!

**Q: Can I use it for non-terminal apps?**
A: Yes! Any GUI app works. Browsers, file managers, anything.

**Q: How much RAM does this use?**
A: Minimal. A pooled terminal uses the same RAM as a normal one. The daemon itself is ~2MB.

**Q: What if I close a pooled window?**
A: Daemon detects it and spawns a replacement within 800ms.

## Comparison

Like sxhkd/bspwm but for **application startup latency elimination**.

| Tool | Purpose | Impact |
|------|---------|--------|
| **sxhkd** | Hotkey responsiveness | Critical |
| **bspwm** | Window management speed | Critical |
| **0fbuf** | App startup elimination | **Critical** |

The difference between "press key → wait → terminal" and "press key → terminal is there" is *felt* the same way tiling WMs are felt.

## License

MIT (or your choice)

## Contributing

Issues and PRs welcome! Especially:
- Wayland backend implementation
- Additional terminal auto-detection
- Performance optimizations
