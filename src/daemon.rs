use crate::config::{Config, get_config_path, load_config};
use crate::pools::ensure_pool;
use crate::state::{cleanup_stale_state, input_path, load_state, save_input, save_state, state_path, InputState};
use crate::x11util as xu;
use anyhow::Result;
use notify::{Config as NotifyConfig, RecommendedWatcher, RecursiveMode, Watcher};
use signal_hook::consts::signal::*;
use signal_hook::iterator::Signals;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::sleep;
use std::time::{Duration, Instant};
use x11rb::protocol::xproto::ConnectionExt as _;

const CLEANUP_INTERVAL_MS: u64 = 30_000; // Clean up stale XIDs every 30 seconds

pub fn run_daemon(cfg: &Config) -> Result<()> {
    eprintln!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    eprintln!("  0fbuf daemon starting");
    eprintln!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // Setup signal handler for graceful shutdown
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    let mut signals = Signals::new([SIGTERM, SIGINT])?;
    thread::spawn(move || {
        if let Some(sig) = signals.forever().next() {
            eprintln!("\n[0fbuf] Received signal {:?}, shutting down...", sig);
            r.store(false, Ordering::Relaxed);
        }
    });

    // Config file watching
    let config_path = get_config_path(None);
    let config = Arc::new(Mutex::new(cfg.clone()));
    let config_for_watcher = config.clone();
    let config_reload_flag = Arc::new(AtomicBool::new(false));
    let reload_flag_for_watcher = config_reload_flag.clone();

    // Setup file watcher for config changes
    let watch_path = config_path.clone();
    thread::spawn(move || {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher: RecommendedWatcher = Watcher::new(
            tx,
            NotifyConfig::default().with_poll_interval(Duration::from_secs(2)),
        ).unwrap();

        if let Err(e) = watcher.watch(&watch_path, RecursiveMode::NonRecursive) {
            eprintln!("[0fbuf] Failed to watch config file: {}", e);
            return;
        }

        for res in rx {
            match res {
                Ok(_event) => {
                    // Config file changed, reload it
                    if let Some(new_cfg) = load_config(Some(&watch_path)) {
                        *config_for_watcher.lock().unwrap() = new_cfg;
                        reload_flag_for_watcher.store(true, Ordering::Relaxed);
                        eprintln!("[0fbuf] Config reloaded from {}", watch_path.display());
                    }
                }
                Err(e) => eprintln!("[0fbuf] Watch error: {}", e),
            }
        }
    });

    // One connection for pool maintainer
    eprintln!("[0fbuf] Connecting to X11...");
    let (conn_p, _sn, root_p) = xu::connect()?;
    let atoms_p = xu::get_atoms(&conn_p)?;

    // Another connection for input tracking
    let (conn_i, _sn2, root_i) = xu::connect()?;

    // Modifier keycodes set (grouped to distinguish Shift/Lock vs shortcut modifiers)
    let mod_groups = xu::modifier_groups(&conn_i);

    // Print initial config
    {
        let cfg = config.lock().unwrap();
        eprintln!("\n[input]");
        eprintln!("  mouse_recent_ms    = {}", cfg.input.mouse_recent_ms);
        eprintln!("  key_suppress_ms    = {}", cfg.input.key_suppress_ms);
        eprintln!("  input_poll_ms      = {}", cfg.input.input_poll_ms);
        eprintln!("  ensure_interval_ms = {}", cfg.input.ensure_interval_ms);
        eprintln!("  motion_eps         = {}", cfg.input.motion_eps);

        // Print pool configurations
        let desktops = xu::number_of_desktops(&conn_p, &atoms_p, root_p)?;
        eprintln!("\n[pools] ({} configured)", cfg.pools.len());
        for (name, pool) in &cfg.pools {
            let pool_desktop = pool.pool_desktop.unwrap_or_else(|| desktops.saturating_sub(1));
            eprintln!("  {} (class: {}, min: {}, desktop: {})",
                name, pool.class, pool.min, pool_desktop);
            eprintln!("    spawn: {}", pool.spawn_cmd);
            if !pool.on_spawn.is_empty() {
                eprintln!("    on_spawn: {} commands", pool.on_spawn.len());
            }
        }

        // Initial pool ensure
        eprintln!("\n[0fbuf] Pre-warming pools...");
        for pool in cfg.pools.values() {
            match ensure_pool(pool, &conn_p, &atoms_p, root_p) {
                Ok(_) => {
                    let wins = xu::collect_class_windows(&conn_p, &atoms_p, root_p, &pool.class)?;
                    let pool_desktop = pool.pool_desktop.unwrap_or_else(|| desktops.saturating_sub(1));
                    let count = wins.iter().filter(|w| w.desktop == Some(pool_desktop)).count();
                    eprintln!("  ✓ {} pool: {} windows ready", pool.name, count);
                }
                Err(e) => eprintln!("  ✗ {} pool failed: {}", pool.name, e),
            }
        }
    }

    // Input tracking state
    let mut ist = InputState { move_flag: false, pointer_x: 0, pointer_y: 0, last_mouse_at: None, last_key_at: None };
    let mut last_mouse = Instant::now() - Duration::from_secs(10);
    let mut last_key = Instant::now() - Duration::from_secs(10);
    // Initialize pointer position
    if let Ok((x, y)) = xu::pointer_position(&conn_i, root_i) { ist.pointer_x = x as i32; ist.pointer_y = y as i32; }

    // Pool maintainer scheduling
    let mut last_ensure = Instant::now() - Duration::from_secs(60);

    // State cleanup scheduling
    let mut last_cleanup = Instant::now() - Duration::from_secs(60);
    let state_file = state_path();

    // Input monitor loop
    let input_file = input_path();

    eprintln!("\n[0fbuf] Daemon running (Ctrl+C to stop)");
    eprintln!("  State files:");
    eprintln!("    - {}", state_file.display());
    eprintln!("    - {}", input_file.display());
    eprintln!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");
    let mut x11_error_count = 0;
    const MAX_X11_ERRORS: u32 = 10;

    while running.load(Ordering::Relaxed) {
        // Check if config was reloaded
        if config_reload_flag.load(Ordering::Relaxed) {
            config_reload_flag.store(false, Ordering::Relaxed);
            let cfg = config.lock().unwrap();
            eprintln!("\n[0fbuf] Config reloaded - re-ensuring pools...");
            for pool in cfg.pools.values() {
                if let Err(e) = ensure_pool(pool, &conn_p, &atoms_p, root_p) {
                    eprintln!("[0fbuf] Failed to ensure pool '{}': {}", pool.name, e);
                }
            }
        }

        // Get current config values
        let (motion_eps, mouse_recent_ms, key_suppress_ms, input_poll_ms, ensure_interval_ms) = {
            let cfg = config.lock().unwrap();
            (cfg.input.motion_eps, cfg.input.mouse_recent_ms, cfg.input.key_suppress_ms,
             cfg.input.input_poll_ms, cfg.input.ensure_interval_ms)
        };

        // Sample pointer position; detect recent motion
        match xu::pointer_position(&conn_i, root_i) {
            Ok((x, y)) => {
                x11_error_count = 0; // Reset error count on success
                let dx = x - ist.pointer_x as i16;
                let dy = y - ist.pointer_y as i16;
                if dx.abs() > motion_eps || dy.abs() > motion_eps {
                    ist.pointer_x = x as i32; ist.pointer_y = y as i32; last_mouse = Instant::now();
                    // Only allocate timestamp string when motion actually detected
                    ist.last_mouse_at = Some(crate::state::now_rfc3339());
                }
            }
            Err(e) => {
                x11_error_count += 1;
                eprintln!("[0fbuf] X11 error reading pointer: {} (count: {})", e, x11_error_count);
                if x11_error_count >= MAX_X11_ERRORS {
                    eprintln!("[0fbuf] Too many X11 errors, exiting");
                    break;
                }
            }
        }

        // Keypress sampling using XQueryKeymap: count key activity when a non-modifier key is down without shortcut modifiers
        match conn_i.query_keymap() {
            Ok(reply) => match reply.reply() {
                Ok(r) => {
                    x11_error_count = 0; // Reset error count on success
            let mut any_shortcut_mod = false;
            // Shortcut modifiers = Control (index 2) and Mod1..Mod5 (3..7)
            'mods: for (i, byte) in r.keys.iter().enumerate() {
                if *byte == 0 { continue; }
                for b in 0..8 {
                    if (byte & (1u8<<b)) != 0 {
                        let kc = (i as u8) * 8 + b + 8;
                        if mod_groups[2].contains(&kc) || mod_groups[3].contains(&kc) || mod_groups[4].contains(&kc) || mod_groups[5].contains(&kc) || mod_groups[6].contains(&kc) || mod_groups[7].contains(&kc) {
                            any_shortcut_mod = true; break 'mods;
                        }
                    }
                }
            }
            'keys: for (i, byte) in r.keys.iter().enumerate() {
                if *byte == 0 { continue; }
                for b in 0..8 {
                    if (byte & (1u8<<b)) != 0 {
                        let kc = (i as u8) * 8 + b + 8;
                        // Consider it typing only if this key is not any modifier and no shortcut modifier is held
                        let is_any_mod = mod_groups.iter().any(|set| set.contains(&kc));
                        if !is_any_mod && !any_shortcut_mod {
                            last_key = Instant::now();
                            // Only allocate timestamp string when key actually pressed
                            ist.last_key_at = Some(crate::state::now_rfc3339());
                            break 'keys;
                        }
                    }
                }
            }
                }
                Err(e) => {
                    x11_error_count += 1;
                    eprintln!("[0fbuf] X11 error in query_keymap reply: {} (count: {})", e, x11_error_count);
                    if x11_error_count >= MAX_X11_ERRORS {
                        eprintln!("[0fbuf] Too many X11 errors, exiting");
                        break;
                    }
                }
            }
            Err(e) => {
                x11_error_count += 1;
                eprintln!("[0fbuf] X11 error querying keymap: {} (count: {})", e, x11_error_count);
                if x11_error_count >= MAX_X11_ERRORS {
                    eprintln!("[0fbuf] Too many X11 errors, exiting");
                    break;
                }
            }
        }

        // Compute move_flag
        let now = Instant::now();
        let mouse_recent = now.duration_since(last_mouse) <= Duration::from_millis(mouse_recent_ms.max(50));
        let key_recent = now.duration_since(last_key) <= Duration::from_millis(key_suppress_ms.max(50));
        ist.move_flag = mouse_recent && !key_recent;

        // Periodic pool ensure
        let ms_ensure = ensure_interval_ms.max(50);
        if Instant::now().duration_since(last_ensure) >= Duration::from_millis(ms_ensure) {
            let cfg = config.lock().unwrap();
            for pool in cfg.pools.values() {
                if let Err(e) = ensure_pool(pool, &conn_p, &atoms_p, root_p) {
                    eprintln!("[0fbuf] Failed to ensure pool '{}': {}", pool.name, e);
                }
            }
            last_ensure = Instant::now();
        }

        // Persist lightweight input state periodically
        let _ = save_input(&input_file, &ist);

        // Periodic state cleanup: remove XIDs that no longer exist
        if Instant::now().duration_since(last_cleanup) >= Duration::from_millis(CLEANUP_INTERVAL_MS) {
            let mut st = load_state(&state_file);
            let all_wins = xu::client_list(&conn_p, &atoms_p, root_p).unwrap_or_default();
            cleanup_stale_state(&mut st, &all_wins);
            let _ = save_state(&state_file, &st);
            last_cleanup = Instant::now();
        }

        sleep(Duration::from_millis(input_poll_ms.max(10)));
    }

    eprintln!("[0fbuf] Daemon stopped gracefully");
    Ok(())
}
