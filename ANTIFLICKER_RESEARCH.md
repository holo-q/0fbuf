# Anti-Flicker Deep Research & Strategy Guide

## The Problem

When 0fbuf pre-warms windows for the pool, there's a race condition:

```
Timeline (microseconds):
0ms   → Application spawns (spawn_cmd executes)
50ms  → Window created by app
100ms → Window MAPPED by app ← FLASH HAPPENS HERE
120ms → WM sees MapRequest event
140ms → WM places window on current desktop
160ms → WM makes window visible
200ms → Our code detects window (via pid + class match)
210ms → Our code moves window to pool desktop

The window is visible for ~100ms between mapping and our intervention.
```

## Experimental Strategies Implemented

### Strategy: `"none"`
**What it does:** No anti-flicker measures
**Use case:** Baseline testing - see the raw flash
**Effectiveness:** 0/10 - Full flash visible

### Strategy: `"all"` (DEFAULT)
**What it does:** Throws everything at the problem simultaneously:
- Sets window type to UTILITY (bypass normal placement)
- Sets SKIP_TASKBAR + SKIP_PAGER states
- Lowers window to bottom of stack
- Moves window to off-screen coordinates (-10000, -10000)

**Use case:** Maximum aggression - try everything at once
**Effectiveness:** TBD - test this first
**WM compatibility:** Should work on most EWMH-compliant WMs

### Strategy: `"unmap"`
**What it does:** Immediately unmaps the window after detection
**Use case:** Nuclear option - make it invisible
**Effectiveness:** TBD
**Risk:** Window might not re-map correctly when activated

### Strategy: `"offscreen"`
**What it does:** Moves window to coordinates way off screen (-10000, -10000)
**Use case:** Let window exist but be invisible
**Effectiveness:** TBD
**WM compatibility:** Should work universally

### Strategy: `"utility"`
**What it does:** Sets `_NET_WM_WINDOW_TYPE` to `_NET_WM_WINDOW_TYPE_UTILITY`
**Use case:** Tell WM this isn't a normal window
**Effectiveness:** TBD
**WM compatibility:** EWMH-compliant WMs only

### Strategy: `"iconic"`
**What it does:** Sends `WM_CHANGE_STATE` message to set IconicState (minimized)
**Use case:** Start window minimized from birth
**Effectiveness:** TBD
**WM compatibility:** Works with ICCCM-compliant WMs
**Risk:** Window might stay minimized when activated

### Strategy: `"lower"`
**What it does:** Configures window stack mode to BELOW
**Use case:** Push window to bottom of Z-order
**Effectiveness:** TBD
**WM compatibility:** Universal

### Strategy: `"combo1"`
**What it does:** `unmap` + `offscreen`
**Use case:** Double tap - make it invisible AND move it
**Effectiveness:** TBD

### Strategy: `"combo2"`
**What it does:** `utility` + `lower` + `skip_taskbar`
**Use case:** Mark as background utility, hide from taskbar, lower stack
**Effectiveness:** TBD

### Strategy: `"combo3"`
**What it does:** `iconic` + `offscreen`
**Use case:** Minimize it AND move it off-screen
**Effectiveness:** TBD

## How to Test

1. **Stop daemon:** `pkill 0fbuf`

2. **Edit your config:** `0fbuf config`
   ```toml
   [pools.shell]
   antiflicker_strategy = "offscreen"  # Try each strategy
   ```

3. **Restart daemon:** `0fbuf daemon`

4. **Watch the pool refill** - observe if windows flash during pre-warming

5. **Test each strategy systematically:**
   - none (baseline)
   - all
   - unmap
   - offscreen
   - utility
   - iconic
   - lower
   - combo1
   - combo2
   - combo3

6. **Document results for your WM** (XFCE/xfwm4 in your case)

## WM-Specific Behavior Hypothesis

### XFCE/xfwm4
- Uses EWMH + some custom behavior
- Compositor (xfwm4 --replace) may cause additional delays
- Window placement policy might override our hints
- **Hypothesis:** `offscreen` or `combo2` likely to work best

### i3/Sway
- Tiling WMs have different window lifecycle
- Might be more responsive to our hints
- **Hypothesis:** `unmap` or `utility` should work well

### KDE/kwin
- Heavy compositor involvement
- More sophisticated window placement
- **Hypothesis:** `combo2` with UTILITY type

### GNOME/mutter
- Modern compositor architecture
- Wayland vs X11 differences
- **Hypothesis:** `combo1` or `combo3`

## Next Level: Deep WM Spelunking

If NONE of these strategies work perfectly for xfwm4, here's the nuclear research plan:

### Phase 1: Source Code Analysis (xfwm4)
1. **Clone xfwm4 source:**
   ```bash
   git clone https://gitlab.xfce.org/xfce/xfwm4.git
   cd xfwm4
   ```

2. **Key files to analyze:**
   - `src/events.c` - Event handling (MapRequest, MapNotify)
   - `src/placement.c` - Window placement logic
   - `src/netwm.c` - EWMH property handling
   - `src/compositor.c` - Compositing pipeline
   - `src/client.c` - Client window lifecycle

3. **Questions to answer:**
   - When does xfwm4 make a window visible?
   - Can we set properties BEFORE mapping that xfwm4 respects?
   - Does xfwm4 have any undocumented hints?
   - Is there a "spawn on desktop N" property we can set?

### Phase 2: Comparative WM Research
Clone and analyze 10+ WM codebases:

```bash
# Tiling WMs
git clone https://github.com/i3/i3.git
git clone https://github.com/swaywm/sway.git
git clone https://github.com/baskerville/bspwm.git

# Floating WMs
git clone https://gitlab.xfce.org/xfce/xfwm4.git
git clone https://github.com/openbox/openbox.git
git clone https://github.com/awesomeWM/awesome.git

# Compositors
git clone https://gitlab.freedesktop.org/xorg/app/picom.git
git clone https://github.com/hyprwm/Hyprland.git

# Desktop environments
git clone https://gitlab.gnome.org/GNOME/mutter.git
git clone https://invent.kde.org/plasma/kwin.git
```

**Research matrix:**
- How does each WM handle MapRequest?
- What hints do they respect?
- Can windows be spawned invisible?
- What's the compositor's role?
- Are there per-WM hacks we can apply?

### Phase 3: X11 Protocol Level Intercept
If all else fails, go deeper:

1. **XCB Event Interception:**
   - Hook into XCB before windows are mapped
   - Intercept MapRequest events
   - Modify window properties before WM sees them

2. **LD_PRELOAD Wrapper:**
   - Create LD_PRELOAD library that wraps XMapWindow
   - Force windows to spawn on specific desktop from birth
   - Inject into spawned terminals

3. **WM Extension/Plugin:**
   - Write xfwm4 plugin that knows about 0fbuf
   - Plugin intercepts pool windows and places them directly
   - Zero-latency perfect placement

## The Ultimate Solution: WM-Aware Mode

**Concept:** 0fbuf detects your WM and applies WM-specific strategies automatically.

```toml
[pools.shell]
antiflicker_strategy = "auto"  # Detects WM and picks best strategy
```

**Implementation:**
```rust
fn detect_wm() -> &'static str {
    // Check _NET_SUPPORTING_WM_CHECK
    // Read WM_NAME from supporting window
    // Return: "xfwm4", "i3", "kwin", "mutter", etc.
}

fn auto_strategy(wm: &str) -> &'static str {
    match wm {
        "xfwm4" => "combo2",
        "i3" => "unmap",
        "kwin" => "combo3",
        "mutter" => "combo1",
        _ => "all",
    }
}
```

## Advanced Techniques (If Standard Approaches Fail)

### Technique 1: Pre-Map Property Setting
**Concept:** Set properties on window BEFORE it maps
**How:** Monitor for CreateNotify events, set properties immediately
**Challenge:** Requires event loop integration in daemon

### Technique 2: Terminal-Level Control
**Concept:** Pass flags to terminal that make it spawn hidden
**Example:** Some terminals support `--hidden` or `--iconic` flags
**Research needed:** Check kitty, alacritty, wezterm documentation

### Technique 3: CRIU-Style State Capture
**Concept:** Instead of pre-warming, checkpoint a running terminal
**How:** Use CRIU to checkpoint terminal + tmux session, restore on demand
**Complexity:** HIGH - but potentially zero flash if restore is fast enough

### Technique 4: Wayland-Specific Solution
**Concept:** On Wayland, we have better control
**Advantage:** xdg-shell protocol supports layer-shell, explicit positioning
**Implementation:** Separate Wayland backend for 0fbuf

## Recommended Testing Order

1. **Start here:** `antiflicker_strategy = "all"`
2. **If still flashing:** Try `"combo1"`
3. **If still flashing:** Try `"unmap"`
4. **If still flashing:** Try `"offscreen"`
5. **If STILL flashing:** Time for Phase 1 (xfwm4 source analysis)
6. **If that fails:** Time for Phase 2 (comparative WM research)
7. **Nuclear option:** Phase 3 (protocol-level intercept)

## Expected Outcomes

**Best case:** One of the 10 strategies eliminates flash completely
**Good case:** Flash reduced from 100ms → 10-20ms (barely perceptible)
**Acceptable:** Flash reduced by 50%+ (noticeable improvement)
**Research needed:** No strategy works → time to dive into WM code

## Data Collection

When testing, note:
1. **Flash duration:** "Brief", "Noticeable", "Long", "None"
2. **Flash location:** "Center screen", "Top-left", "Random"
3. **Window behavior:** "Works normally", "Stays minimized", "Flickers"
4. **Activation latency:** Does it affect speed when activating?

## Contributing

Found a strategy that works perfectly for your WM? Document it:
```
WM: xfwm4 4.18.0
Strategy: combo2
Flash: None detected
Compositor: On
Notes: Works perfectly with xfwm4 compositor enabled
```

## Future Work

- [ ] Auto-detection of optimal strategy per WM
- [ ] xfwm4 source code deep dive
- [ ] Comparative WM research (10+ WMs)
- [ ] Terminal-level spawn control research
- [ ] Wayland backend implementation
- [ ] WM plugin system (for ultimate control)
- [ ] CRIU checkpoint/restore experimentation
- [ ] Create per-WM configuration profiles

---

**The goal:** Zero-frame perfection. Every window appears exactly where it should, exactly when called, with ZERO perceived latency.

This is the difference between "pretty good" and "godmode".
