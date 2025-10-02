use anyhow::{anyhow, Result};
use clap::Parser;
use std::process::Command;

use zerofbuf::cli::{Cli, Commands, ScratchpadAction};
use zerofbuf::config::{default_single_config, get_config_path, load_config};
use zerofbuf::daemon::run_daemon;
use zerofbuf::pools::{activate, ensure_pool, scratchpad_cycle, scratchpad_hide, scratchpad_show, scratchpad_toggle};
use zerofbuf::state::{input_path, load_input};
use zerofbuf::x11util as xu;

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Load config or default single pool, optionally skip config files
    let mut cfg = if cli.no_config {
        default_single_config()
    } else {
        load_config(cli.config.as_deref()).unwrap_or_else(default_single_config)
    };

    // Apply input overrides from CLI
    if let Some(v) = cli.input_opts.mouse_recent_ms { cfg.input.mouse_recent_ms = v; }
    if let Some(v) = cli.input_opts.key_suppress_ms { cfg.input.key_suppress_ms = v; }
    if let Some(v) = cli.input_opts.input_poll_ms { cfg.input.input_poll_ms = v; }
    if let Some(v) = cli.input_opts.ensure_interval_ms { cfg.input.ensure_interval_ms = v; }
    if let Some(v) = cli.input_opts.motion_eps { cfg.input.motion_eps = v; }

    // Prepare an overridable pool (default or named via --pool-name)
    let override_pool_name = cli
        .pool_opts
        .pool_name
        .as_deref()
        .unwrap_or(&cli.default_pool)
        .to_string();
    if !cfg.pools.contains_key(&override_pool_name) {
        // Seed from default template
        let mut seed = default_single_config().pools.into_iter().next().unwrap().1;
        seed.name = override_pool_name.clone();
        cfg.pools.insert(override_pool_name.clone(), seed);
    }
    if let Some(p) = cfg.pools.get_mut(&override_pool_name) {
        if let Some(ref v) = cli.pool_opts.class { p.class = v.clone(); }
        if let Some(v) = cli.pool_opts.min { p.min = v; }
        if let Some(ref v) = cli.pool_opts.spawn_cmd { p.spawn_cmd = v.clone(); }
        if let Some(v) = cli.pool_opts.pool_desktop { p.pool_desktop = Some(v); }
        if cli.pool_opts.no_reset { p.reset_via_tmux = false; }
        if let Some(ref v) = cli.pool_opts.reset_keys { p.reset_keys = Some(v.clone()); }
        if let Some(ref v) = cli.pool_opts.antiflicker_strategy { p.antiflicker_strategy = v.clone(); }
    }

    // Apply antiflicker override to ALL pools if specified (for daemon testing)
    if let Some(ref strategy) = cli.pool_opts.antiflicker_strategy {
        for pool in cfg.pools.values_mut() {
            pool.antiflicker_strategy = strategy.clone();
        }
    }

    match cli.command {
        Commands::Daemon => run_daemon(&cfg)?,
        Commands::Ensure(arg) => {
            let name = arg.pool.as_deref().unwrap_or(&cli.default_pool);
            let pool = cfg.pools.get(name).ok_or_else(|| anyhow!("unknown pool: {}", name))?;
            let (conn, _sn, root) = xu::connect()?;
            let atoms = xu::get_atoms(&conn)?;
            ensure_pool(pool, &conn, &atoms, root)?;
        }
        Commands::List(arg) => {
            let name = arg.pool.as_deref().unwrap_or(&cli.default_pool);
            let pool = cfg.pools.get(name).ok_or_else(|| anyhow!("unknown pool: {}", name))?;
            let (conn, _sn, root) = xu::connect()?;
            let atoms = xu::get_atoms(&conn)?;
            let cur = xu::current_desktop(&conn, &atoms, root)?;
            let desktops = xu::number_of_desktops(&conn, &atoms, root)?;
            let pool_desktop = pool.pool_desktop.unwrap_or_else(|| desktops.saturating_sub(1));
            let wins = xu::collect_class_windows(&conn, &atoms, root, &pool.class)?;
            println!("current_desktop={} pool_desktop={}", cur, pool_desktop);
            for w in wins { println!("xid=0x{:<8x} pid={:<6?} desktop={:<3?} class={:?}", w.xid, w.pid, w.desktop, w.class); }
        }
        Commands::Toggle(arg) => {
            let name = arg.pool.as_deref().unwrap_or(&cli.default_pool);
            let pool = cfg.pools.get(name).ok_or_else(|| anyhow!("unknown pool: {}", name))?;
            let (conn, _sn, root) = xu::connect()?;
            let atoms = xu::get_atoms(&conn)?;
            let cur = xu::current_desktop(&conn, &atoms, root)?;
            let desktops = xu::number_of_desktops(&conn, &atoms, root)?;
            let pool_desktop = pool.pool_desktop.unwrap_or_else(|| desktops.saturating_sub(1));
            let wins = xu::collect_class_windows(&conn, &atoms, root, &pool.class)?;
            if let Some(w) = wins.iter().find(|w| w.desktop == Some(cur)) {
                xu::move_to_desktop(&conn, &atoms, root, w.xid, pool_desktop)?;
                println!("hid");
            } else {
                let _ = activate(pool, &conn, &atoms, root)?;
                println!("shown");
            }
        }
        Commands::Activate(arg) => {
            let name = arg.pool.as_deref().unwrap_or(&cli.default_pool);
            let pool = cfg.pools.get(name).ok_or_else(|| anyhow!("unknown pool: {}", name))?;
            let (conn, _sn, root) = xu::connect()?;
            let atoms = xu::get_atoms(&conn)?;
            let w = activate(pool, &conn, &atoms, root)?;
            if arg.mouse {
                // Only place near mouse if daemon suggests mouse-mode
                let input = load_input(&input_path());
                if input.move_flag { let _ = xu::move_window_near_pointer(&conn, &atoms, root, w.xid); }
            }
        }
        Commands::Scratchpad(arg) => {
            let name = arg.pool.as_deref().unwrap_or(&cli.default_pool);
            let pool = cfg.pools.get(name).ok_or_else(|| anyhow!("unknown pool: {}", name))?;
            let (conn, _sn, root) = xu::connect()?;
            let atoms = xu::get_atoms(&conn)?;

            let result = match arg.action {
                ScratchpadAction::Show => {
                    let w = scratchpad_show(pool, &conn, &atoms, root, arg.mouse)?;
                    format!("shown window 0x{:x}", w.xid)
                }
                ScratchpadAction::Hide => {
                    let count = scratchpad_hide(pool, &conn, &atoms, root)?;
                    format!("hidden {} windows", count)
                }
                ScratchpadAction::Toggle => {
                    scratchpad_toggle(pool, &conn, &atoms, root, arg.mouse)?
                }
                ScratchpadAction::Cycle => {
                    scratchpad_cycle(pool, &conn, &atoms, root, arg.mouse)?
                }
            };
            println!("{}", result);
        }
        Commands::Reset(arg) => {
            // Optional: could send tmux reset per saved sessions in state.json
            let _ = arg; // Not implemented here to keep focus on input feature
        }
        Commands::Status => {
            let st = load_input(&input_path());
            println!("move_flag={} pointer=({}, {}) last_mouse={:?} last_key={:?}", st.move_flag, st.pointer_x, st.pointer_y, st.last_mouse_at, st.last_key_at);
        }
        Commands::Config => {
            let config_path = get_config_path(cli.config.as_deref());

            // Create config directory if it doesn't exist
            if let Some(parent) = config_path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            // Create sample config if file doesn't exist
            if !config_path.exists() {
                let cfg = default_single_config();
                let sample = if config_path.extension().and_then(|s| s.to_str()) == Some("toml") {
                    toml::to_string_pretty(&cfg)?
                } else {
                    serde_json::to_string_pretty(&cfg)?
                };
                std::fs::write(&config_path, sample)?;
                println!("Created new config at: {}", config_path.display());
            }

            // Determine editor (respect standard UNIX precedence)
            let editor = std::env::var("VISUAL")
                .or_else(|_| std::env::var("EDITOR"))
                .unwrap_or_else(|_| "vi".to_string());

            println!("Opening {} with {}", config_path.display(), editor);

            // Launch editor
            let status = Command::new(&editor)
                .arg(&config_path)
                .status()?;

            if !status.success() {
                return Err(anyhow!("Editor exited with status: {}", status));
            }

            // Validate config after editing
            match load_config(cli.config.as_deref()) {
                Some(_) => println!("✓ Config validated successfully"),
                None => {
                    eprintln!("⚠ Warning: Config may have syntax errors. Run '0fbuf daemon' to check.");
                }
            }
        }
        Commands::Strategies => {
            println!("Available anti-flicker strategies:\n");
            println!("  none       - No anti-flicker (baseline - let it flash)");
            println!("  all        - Try everything at once (DEFAULT - most aggressive)");
            println!("  unmap      - Immediately unmap window");
            println!("  offscreen  - Move to off-screen coordinates (-10000, -10000)");
            println!("  utility    - Set window type to UTILITY");
            println!("  iconic     - Start in minimized (IconicState)");
            println!("  lower      - Lower to bottom of window stack");
            println!("  combo1     - unmap + offscreen");
            println!("  combo2     - utility + lower + skip_taskbar");
            println!("  combo3     - iconic + offscreen");
            println!("\nUsage:");
            println!("  Config file:  antiflicker_strategy = \"combo2\"");
            println!("  CLI override: 0fbuf --antiflicker offscreen daemon");
            println!("\nTest each strategy and find what works best for your WM!");
            println!("See ANTIFLICKER_RESEARCH.md for detailed explanations.");
        }
    }

    Ok(())
}
