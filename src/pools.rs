use crate::config::PoolConfig;
use crate::state::{load_state, now_rfc3339, mark_popped, save_state, state_path, WindowMeta};
use crate::x11util as xu;
use anyhow::{anyhow, Result};
use rand::{distributions::Alphanumeric, Rng};
use shell_words::split as shell_split;
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};
use x11rb::rust_connection::RustConnection;
use x11rb::protocol::xproto::*;

const WINDOW_SPAWN_TIMEOUT_SECS: u64 = 3;
const WINDOW_POLL_INTERVAL_MS: u64 = 30;
const ON_SPAWN_DELAY_MS: u64 = 100; // Wait for window to be fully initialized

/// Apply anti-flicker strategy to newly spawned window
fn apply_antiflicker_strategy(strategy: &str, conn: &RustConnection, atoms: &xu::Atoms, root: Window, xid: u32) {
    match strategy {
        "none" => {
            // Do nothing - let it flash
        }
        "all" => {
            // Try everything at once
            let _ = xu::set_window_type_utility(conn, atoms, xid);
            let _ = xu::set_all_invisible_states(conn, atoms, root, xid);
            let _ = xu::lower_window(conn, xid);
            let _ = xu::move_offscreen(conn, xid);
        }
        "unmap" => {
            // Immediately unmap the window
            let _ = xu::unmap_window(conn, xid);
        }
        "offscreen" => {
            // Move offscreen only
            let _ = xu::move_offscreen(conn, xid);
        }
        "utility" => {
            // Set window type to utility
            let _ = xu::set_window_type_utility(conn, atoms, xid);
        }
        "iconic" => {
            // Start in iconic (minimized) state
            let _ = xu::set_iconic_state(conn, atoms, root, xid);
        }
        "lower" => {
            // Lower to bottom of stack
            let _ = xu::lower_window(conn, xid);
        }
        "combo1" => {
            // Unmap + move offscreen
            let _ = xu::unmap_window(conn, xid);
            let _ = xu::move_offscreen(conn, xid);
        }
        "combo2" => {
            // Utility + lower + skip states
            let _ = xu::set_window_type_utility(conn, atoms, xid);
            let _ = xu::set_all_invisible_states(conn, atoms, root, xid);
            let _ = xu::lower_window(conn, xid);
        }
        "combo3" => {
            // Iconic + offscreen
            let _ = xu::set_iconic_state(conn, atoms, root, xid);
            let _ = xu::move_offscreen(conn, xid);
        }
        _ => {
            eprintln!("[0fbuf] Unknown antiflicker_strategy: {}, using 'all'", strategy);
            let _ = xu::set_window_type_utility(conn, atoms, xid);
            let _ = xu::set_all_invisible_states(conn, atoms, root, xid);
            let _ = xu::lower_window(conn, xid);
            let _ = xu::move_offscreen(conn, xid);
        }
    }
}

fn execute_on_spawn(commands: &[String], class: &str, session: &str, xid: u32) -> Result<()> {
    if commands.is_empty() {
        return Ok(());
    }

    // Small delay to let the window fully initialize
    sleep(Duration::from_millis(ON_SPAWN_DELAY_MS));

    for cmd_template in commands {
        let cmd = cmd_template
            .replace("{class}", class)
            .replace("{session}", session)
            .replace("{xid}", &format!("0x{:x}", xid));

        let status = Command::new("sh")
            .arg("-c")
            .arg(&cmd)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();

        match status {
            Ok(s) if s.success() => {},
            Ok(s) => eprintln!("[0fbuf] on_spawn command failed: {} (exit: {:?})", cmd, s.code()),
            Err(e) => eprintln!("[0fbuf] on_spawn command error: {} ({})", cmd, e),
        }
    }

    Ok(())
}

fn gen_session_id() -> String {
    let rand: String = rand::thread_rng().sample_iter(&Alphanumeric).take(6).map(char::from).collect();
    let t = now_rfc3339().replace(':', "");
    format!("0fbuf-{}-{}", t, rand)
}

fn default_spawn_cmd(class: &str, session: &str) -> Vec<String> {
    let tmpl = format!("xterm -class {class} -T {class} -e tmux new -A -s {session}");
    shell_split(&tmpl).unwrap()
}

fn parse_cmd(template: &str, session: &str, class: &str) -> Result<Vec<String>> {
    let filled = template.replace("{session}", session).replace("{class}", class);
    shell_split(&filled).map_err(|e| anyhow!("Failed to parse spawn command '{}': {}", template, e))
}

pub fn spawn_one(pool: &PoolConfig, conn: &RustConnection, atoms: &xu::Atoms, root: Window, pool_desktop: u32) -> Result<xu::WinInfo> {
    let session = gen_session_id();
    let argv = if pool.spawn_cmd.trim().is_empty() {
        default_spawn_cmd(&pool.class, &session)
    } else {
        parse_cmd(&pool.spawn_cmd, &session, &pool.class).unwrap_or_else(|e| {
            eprintln!("[0fbuf] Warning: {}, using default", e);
            default_spawn_cmd(&pool.class, &session)
        })
    };
    let (prog, args) = argv.split_first().ok_or_else(|| anyhow!("empty spawn cmd"))?;

    // Determine preload library path
    let preload_lib = dirs::home_dir()
        .map(|h| h.join(".local/lib/lib0fbuf_preload.so"))
        .and_then(|p| if p.exists() { Some(p) } else { None });

    // Spawn process with LD_PRELOAD if available
    // The terminal emulator will manage its own lifecycle
    let mut cmd = Command::new(prog);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    // Set environment for preload library
    if let Some(ref lib_path) = preload_lib {
        cmd.env("LD_PRELOAD", lib_path);
        cmd.env("OFBUF_POOL_DESKTOP", pool_desktop.to_string());
        eprintln!("[0fbuf] Using preload: {}", lib_path.display());
    }

    let child = cmd.spawn()?;
    let pid = child.id();

    // Prevent zombie by forgetting the Child handle
    // The spawned terminal becomes a background daemon
    std::mem::forget(child);

    // Poll to find the window by _NET_WM_PID and class
    let start = Instant::now();
    let xid = loop {
        let wins = xu::collect_class_windows(conn, atoms, root, &pool.class)?;
        if let Some(w) = wins.into_iter().find(|w| w.pid == Some(pid)) {
            break w.xid;
        }
        if start.elapsed() > Duration::from_secs(WINDOW_SPAWN_TIMEOUT_SECS) {
            return Err(anyhow!("timeout waiting for window (pid {})", pid));
        }
        sleep(Duration::from_millis(WINDOW_POLL_INTERVAL_MS));
    };

    // If preload library was NOT used, apply fallback anti-flicker strategy
    if preload_lib.is_none() {
        apply_antiflicker_strategy(&pool.antiflicker_strategy, conn, atoms, root, xid);
        // Move to pool desktop immediately to minimize visible time
        xu::move_to_desktop(conn, atoms, root, xid, pool_desktop)?;
    }
    // If preload WAS used, the window is already on the correct desktop with correct properties

    // Update state mapping
    let p = state_path();
    let mut st = load_state(&p);
    st.windows.insert(
        xid,
        WindowMeta {
            pid,
            session: Some(session.clone()),
            created_at: now_rfc3339(),
            popped: false, // Window starts in buffer, not popped
        },
    );
    save_state(&p, &st)?;

    // Execute on_spawn commands
    execute_on_spawn(&pool.on_spawn, &pool.class, &session, xid)?;

    Ok(xu::WinInfo { xid, pid: Some(pid), desktop: Some(pool_desktop), class: Some(pool.class.clone()) })
}

pub fn ensure_pool(pool: &PoolConfig, conn: &RustConnection, atoms: &xu::Atoms, root: Window) -> Result<()> {
    let desktops = xu::number_of_desktops(conn, atoms, root)?;
    let pool_desktop = pool.pool_desktop.unwrap_or_else(|| desktops.saturating_sub(1));

    // Validate pool desktop is within range
    if pool_desktop >= desktops {
        return Err(anyhow!(
            "pool_desktop {} is out of range (only {} desktops available)",
            pool_desktop,
            desktops
        ));
    }

    let wins = xu::collect_class_windows(conn, atoms, root, &pool.class)?;

    // Load state to check which windows are popped
    let st = load_state(&state_path());

    // Count only non-popped windows on the pool desktop
    let have: usize = wins
        .iter()
        .filter(|w| {
            w.desktop == Some(pool_desktop) &&
            !st.windows.get(&w.xid).map_or(false, |meta| meta.popped)
        })
        .count();

    if have < pool.min {
        let needed = pool.min - have;
        eprintln!("[0fbuf] Pool '{}' needs {} more windows (have {}, need {})",
                  pool.name, needed, have, pool.min);
        for _ in 0..needed {
            let _ = spawn_one(pool, conn, atoms, root, pool_desktop);
        }
    }
    Ok(())
}

/// Hide a specific window by moving it to the pool desktop
pub fn hide_window(pool: &PoolConfig, conn: &RustConnection, atoms: &xu::Atoms, root: Window, xid: u32) -> Result<()> {
    let desktops = xu::number_of_desktops(conn, atoms, root)?;
    let pool_desktop = pool.pool_desktop.unwrap_or_else(|| desktops.saturating_sub(1));

    if pool_desktop >= desktops {
        return Err(anyhow!(
            "pool_desktop {} is out of range (only {} desktops available)",
            pool_desktop,
            desktops
        ));
    }

    xu::move_to_desktop(conn, atoms, root, xid, pool_desktop)?;
    Ok(())
}

/// Scratchpad show: bring one window from pool to current desktop (or spawn)
pub fn scratchpad_show(pool: &PoolConfig, conn: &RustConnection, atoms: &xu::Atoms, root: Window, apply_mouse: bool) -> Result<xu::WinInfo> {
    let w = activate(pool, conn, atoms, root)?;
    // activate() already calls restore_normal_state, ensure_window_shown
    if apply_mouse {
        let input = crate::state::load_input(&crate::state::input_path());
        if input.move_flag {
            let _ = xu::move_window_near_pointer(conn, atoms, root, w.xid);
        }
    }
    Ok(w)
}

/// Scratchpad hide: send all visible windows from pool to pool desktop
pub fn scratchpad_hide(pool: &PoolConfig, conn: &RustConnection, atoms: &xu::Atoms, root: Window) -> Result<usize> {
    let cur = xu::current_desktop(conn, atoms, root)?;
    let desktops = xu::number_of_desktops(conn, atoms, root)?;
    let pool_desktop = pool.pool_desktop.unwrap_or_else(|| desktops.saturating_sub(1));
    let wins = xu::collect_class_windows(conn, atoms, root, &pool.class)?;

    let mut hidden = 0;
    for w in wins.iter().filter(|w| w.desktop == Some(cur)) {
        xu::move_to_desktop(conn, atoms, root, w.xid, pool_desktop)?;
        hidden += 1;
    }

    Ok(hidden)
}

/// Scratchpad toggle: if any visible, hide all; otherwise show one
pub fn scratchpad_toggle(pool: &PoolConfig, conn: &RustConnection, atoms: &xu::Atoms, root: Window, apply_mouse: bool) -> Result<String> {
    let cur = xu::current_desktop(conn, atoms, root)?;
    let wins = xu::collect_class_windows(conn, atoms, root, &pool.class)?;
    let visible = wins.iter().filter(|w| w.desktop == Some(cur)).count();

    if visible > 0 {
        let count = scratchpad_hide(pool, conn, atoms, root)?;
        Ok(format!("hidden {} windows", count))
    } else {
        let w = scratchpad_show(pool, conn, atoms, root, apply_mouse)?;
        Ok(format!("shown window 0x{:x}", w.xid))
    }
}

/// Scratchpad cycle: if visible, hide it and show next; otherwise show one
pub fn scratchpad_cycle(pool: &PoolConfig, conn: &RustConnection, atoms: &xu::Atoms, root: Window, apply_mouse: bool) -> Result<String> {
    let cur = xu::current_desktop(conn, atoms, root)?;
    let desktops = xu::number_of_desktops(conn, atoms, root)?;
    let pool_desktop = pool.pool_desktop.unwrap_or_else(|| desktops.saturating_sub(1));
    let wins = xu::collect_class_windows(conn, atoms, root, &pool.class)?;

    // Load state to check which windows are popped
    let st = load_state(&state_path());

    // Find currently visible window
    if let Some(visible_win) = wins.iter().find(|w| w.desktop == Some(cur)) {
        // Hide the current one
        xu::move_to_desktop(conn, atoms, root, visible_win.xid, pool_desktop)?;

        // Find next non-popped window in pool (or wrap around)
        let pool_wins: Vec<_> = wins.iter().filter(|w| {
            w.desktop == Some(pool_desktop) &&
            !st.windows.get(&w.xid).map_or(false, |meta| meta.popped)
        }).collect();
        if let Some(next) = pool_wins.first() {
            // Pop this window from the pool
            let _ = mark_popped(next.xid);

            xu::restore_normal_state(conn, atoms, root, next.xid)?;
            xu::move_window_to_center(conn, root, next.xid)?;
            xu::move_to_desktop(conn, atoms, root, next.xid, cur)?;
            xu::ensure_window_shown(conn, next.xid)?;
            xu::focus_window(conn, atoms, root, next.xid)?;

            if apply_mouse {
                let input = crate::state::load_input(&crate::state::input_path());
                if input.move_flag {
                    let _ = xu::move_window_near_pointer(conn, atoms, root, next.xid);
                }
            }

            Ok(format!("cycled to 0x{:x}", next.xid))
        } else {
            Ok("hidden last window".into())
        }
    } else {
        // No visible window, just show one
        let w = scratchpad_show(pool, conn, atoms, root, apply_mouse)?;
        Ok(format!("shown window 0x{:x}", w.xid))
    }
}

pub fn activate(pool: &PoolConfig, conn: &RustConnection, atoms: &xu::Atoms, root: Window) -> Result<xu::WinInfo> {
    let cur = xu::current_desktop(conn, atoms, root)?;
    let desktops = xu::number_of_desktops(conn, atoms, root)?;
    let pool_desktop = pool.pool_desktop.unwrap_or_else(|| desktops.saturating_sub(1));

    // Validate pool desktop is within range
    if pool_desktop >= desktops {
        return Err(anyhow!(
            "pool_desktop {} is out of range (only {} desktops available)",
            pool_desktop,
            desktops
        ));
    }

    let wins = xu::collect_class_windows(conn, atoms, root, &pool.class)?;

    // Load state to check which windows are released
    let st = load_state(&state_path());

    // Find a non-released window from the pool
    if let Some(w) = wins.into_iter().find(|w| {
        w.desktop == Some(pool_desktop) &&
        !st.windows.get(&w.xid).map_or(false, |meta| meta.released)
    }) {
        // Release this window from the pool (it becomes independent)
        let _ = release_window(w.xid);

        // Restore window to normal state (undo preload invisibility)
        xu::restore_normal_state(conn, atoms, root, w.xid)?;
        // Move window to visible position (undo preload offscreen placement)
        xu::move_window_to_center(conn, root, w.xid)?;
        xu::move_to_desktop(conn, atoms, root, w.xid, cur)?;
        xu::ensure_window_shown(conn, w.xid)?;
        xu::focus_window(conn, atoms, root, w.xid)?;

        eprintln!("[0fbuf] Released window 0x{:x} from pool - now independent", w.xid);
        return Ok(w);
    }
    let w = spawn_one(pool, conn, atoms, root, pool_desktop)?;
    xu::restore_normal_state(conn, atoms, root, w.xid)?;
    xu::move_window_to_center(conn, root, w.xid)?;
    xu::move_to_desktop(conn, atoms, root, w.xid, cur)?;
    xu::ensure_window_shown(conn, w.xid)?;
    xu::focus_window(conn, atoms, root, w.xid)?;
    Ok(w)
}

