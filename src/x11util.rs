use anyhow::{anyhow, Result};
use std::collections::HashSet;
use x11rb::connection::Connection as _;
use x11rb::protocol::xproto::*;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

#[derive(Debug, Clone)]
pub struct Atoms {
    pub net_client_list: Atom,
    pub net_active_window: Atom,
    pub net_wm_pid: Atom,
    pub net_wm_desktop: Atom,
    pub net_current_desktop: Atom,
    pub net_number_of_desktops: Atom,
    pub net_moveresize_window: Atom,
    pub net_wm_state: Atom,
    pub net_wm_state_hidden: Atom,
    pub net_wm_state_skip_taskbar: Atom,
    pub net_wm_state_skip_pager: Atom,
    pub net_wm_window_type: Atom,
    pub net_wm_window_type_utility: Atom,
    pub net_wm_window_type_notification: Atom,
    pub net_wm_window_type_splash: Atom,
    pub wm_class: Atom,
    pub wm_state: Atom,
    pub wm_change_state: Atom,
    pub utf8_string: Atom,
}

pub fn get_atoms(conn: &RustConnection) -> Result<Atoms> {
    let intern = |name: &str| -> Result<Atom> {
        Ok(conn.intern_atom(false, name.as_bytes())?.reply()?.atom)
    };
    Ok(Atoms {
        net_client_list: intern("_NET_CLIENT_LIST")?,
        net_active_window: intern("_NET_ACTIVE_WINDOW")?,
        net_wm_pid: intern("_NET_WM_PID")?,
        net_wm_desktop: intern("_NET_WM_DESKTOP")?,
        net_current_desktop: intern("_NET_CURRENT_DESKTOP")?,
        net_number_of_desktops: intern("_NET_NUMBER_OF_DESKTOPS")?,
        net_moveresize_window: intern("_NET_MOVERESIZE_WINDOW")?,
        net_wm_state: intern("_NET_WM_STATE")?,
        net_wm_state_hidden: intern("_NET_WM_STATE_HIDDEN")?,
        net_wm_state_skip_taskbar: intern("_NET_WM_STATE_SKIP_TASKBAR")?,
        net_wm_state_skip_pager: intern("_NET_WM_STATE_SKIP_PAGER")?,
        net_wm_window_type: intern("_NET_WM_WINDOW_TYPE")?,
        net_wm_window_type_utility: intern("_NET_WM_WINDOW_TYPE_UTILITY")?,
        net_wm_window_type_notification: intern("_NET_WM_WINDOW_TYPE_NOTIFICATION")?,
        net_wm_window_type_splash: intern("_NET_WM_WINDOW_TYPE_SPLASH")?,
        wm_class: intern("WM_CLASS")?,
        wm_state: intern("WM_STATE")?,
        wm_change_state: intern("WM_CHANGE_STATE")?,
        utf8_string: intern("UTF8_STRING")?,
    })
}

#[derive(Debug, Clone, Default)]
pub struct WinInfo {
    pub xid: u32,
    pub pid: Option<u32>,
    pub desktop: Option<u32>,
    pub class: Option<String>,
}

pub fn current_desktop(conn: &RustConnection, atoms: &Atoms, root: Window) -> Result<u32> {
    get_property_u32(conn, root, atoms.net_current_desktop)?.ok_or_else(|| anyhow!("_NET_CURRENT_DESKTOP not set"))
}

pub fn number_of_desktops(conn: &RustConnection, atoms: &Atoms, root: Window) -> Result<u32> {
    get_property_u32(conn, root, atoms.net_number_of_desktops)?.ok_or_else(|| anyhow!("_NET_NUMBER_OF_DESKTOPS not set"))
}

pub fn client_list(conn: &RustConnection, atoms: &Atoms, root: Window) -> Result<Vec<Window>> {
    let reply = conn.get_property(false, root, atoms.net_client_list, AtomEnum::WINDOW, 0, 4096)?.reply()?;
    let wins: Vec<u32> = reply.value32().map(|it| it.collect()).unwrap_or_default();
    Ok(wins)
}

pub fn get_property_u32(conn: &RustConnection, win: Window, atom: Atom) -> Result<Option<u32>> {
    let reply = conn.get_property(false, win, atom, AtomEnum::CARDINAL, 0, 1)?.reply()?;
    if reply.value_len == 0 { return Ok(None); }
    let mut bytes = [0u8; 4];
    bytes.copy_from_slice(&reply.value[0..4]);
    Ok(Some(u32::from_ne_bytes(bytes)))
}

pub fn get_property_string(conn: &RustConnection, win: Window, atom: Atom) -> Result<Option<String>> {
    let reply = conn.get_property(false, win, atom, AtomEnum::STRING, 0, 64)?.reply()?;
    if reply.value_len == 0 { return Ok(None); }
    let s = String::from_utf8_lossy(&reply.value).to_string();
    Ok(Some(s))
}

pub fn collect_class_windows(conn: &RustConnection, atoms: &Atoms, root: Window, class_key: &str) -> Result<Vec<WinInfo>> {
    let wins = client_list(conn, atoms, root)?;
    let mut out = Vec::new();
    for w in wins {
        let pid = get_property_u32(conn, w, atoms.net_wm_pid).ok().flatten();
        let desktop = get_property_u32(conn, w, atoms.net_wm_desktop).ok().flatten();
        let class_raw = get_property_string(conn, w, atoms.wm_class).ok().flatten();
        let mut include = false;
        if let Some(ref cr) = class_raw {
            let parts: Vec<&str> = cr.split('\0').filter(|s| !s.is_empty()).collect();
            include = parts.iter().any(|p| p.eq_ignore_ascii_case(class_key));
        }
        if include {
            out.push(WinInfo { xid: w, pid, desktop, class: class_raw });
        }
    }
    Ok(out)
}

pub fn move_to_desktop(conn: &RustConnection, atoms: &Atoms, root: Window, win: Window, desktop: u32) -> Result<()> {
    let data = [desktop, 2, 0, 0, 0];
    let msg = ClientMessageEvent::new(32, win, atoms.net_wm_desktop, data);
    conn.send_event(false, root, EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY, msg)?;
    conn.flush()?;
    Ok(())
}

pub fn focus_window(conn: &RustConnection, atoms: &Atoms, root: Window, win: Window) -> Result<()> {
    let data = [1u32, 0, 0, 0, 0];
    let msg = ClientMessageEvent::new(32, win, atoms.net_active_window, data);
    conn.send_event(false, root, EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY, msg)?;
    conn.flush()?;
    Ok(())
}

pub fn pointer_position(conn: &RustConnection, root: Window) -> Result<(i16, i16)> {
    let reply = conn.query_pointer(root)?.reply()?;
    Ok((reply.root_x, reply.root_y))
}

pub fn window_geometry(conn: &RustConnection, win: Window) -> Result<(u16, u16)> {
    let geo = conn.get_geometry(win)?.reply()?;
    Ok((geo.width, geo.height))
}

pub fn move_window_near_pointer(conn: &RustConnection, atoms: &Atoms, root: Window, win: Window) -> Result<()> {
    let (px, py) = pointer_position(conn, root)?;
    // Center window at pointer if we know size; else use a small offset
    let (w, h) = window_geometry(conn, win).unwrap_or((800, 600));
    let x = px as i32 - (w as i32 / 2);
    let y = py as i32 - (h as i32 / 2);
    // Request WM to move window
    let flags = 1 | 2; // X, Y
    let arr = [
        (1 /*gravity*/ << 12) | (flags & 0xFFF),
        x as u32,
        y as u32,
        0,
        0,
    ];
    let msg = ClientMessageEvent::new(32, win, atoms.net_moveresize_window, arr);
    conn.send_event(false, root, EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY, msg)?;
    conn.flush()?;
    Ok(())
}

pub fn active_window(conn: &RustConnection, atoms: &Atoms, root: Window) -> Result<Option<Window>> {
    let reply = conn.get_property(false, root, atoms.net_active_window, AtomEnum::WINDOW, 0, 1)?.reply()?;
    Ok(reply.value32().and_then(|mut it| it.next()))
}

/// Set window state to skip taskbar (helps reduce flicker during spawn)
pub fn set_skip_taskbar(conn: &RustConnection, atoms: &Atoms, root: Window, win: Window) -> Result<()> {
    // Add _NET_WM_STATE_SKIP_TASKBAR to window state
    let data = [1u32, atoms.net_wm_state_skip_taskbar, 0, 0, 0];
    let msg = ClientMessageEvent::new(32, win, atoms.net_wm_state, data);
    conn.send_event(false, root, EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY, msg)?;
    conn.flush()?;
    Ok(())
}

/// Ensure window is shown (not minimized/hidden)
pub fn ensure_window_shown(conn: &RustConnection, win: Window) -> Result<()> {
    // Map the window to ensure it's not withdrawn
    conn.map_window(win)?;
    conn.flush()?;
    Ok(())
}

/// Move window to center of screen (undo preload offscreen placement)
pub fn move_window_to_center(conn: &RustConnection, root: Window, win: Window) -> Result<()> {
    // Get screen dimensions
    let setup = conn.setup();
    let screen = &setup.roots[0];
    let screen_width = screen.width_in_pixels as i32;
    let screen_height = screen.height_in_pixels as i32;

    // Get window geometry
    let (w, h) = window_geometry(conn, win).unwrap_or((800, 600));

    // Calculate center position
    let x = (screen_width - w as i32) / 2;
    let y = (screen_height - h as i32) / 2;

    // Move window
    let values = ConfigureWindowAux::new().x(x).y(y);
    conn.configure_window(win, &values)?;
    conn.flush()?;
    Ok(())
}

/// Restore normal window state (undo preload invisibility hints)
pub fn restore_normal_state(conn: &RustConnection, atoms: &Atoms, root: Window, win: Window) -> Result<()> {
    // Remove _NET_WM_STATE_SKIP_TASKBAR and _NET_WM_STATE_SKIP_PAGER
    let data = [
        0u32, // _NET_WM_STATE_REMOVE
        atoms.net_wm_state_skip_taskbar,
        atoms.net_wm_state_skip_pager,
        0,
        0,
    ];
    let msg = ClientMessageEvent::new(32, win, atoms.net_wm_state, data);
    conn.send_event(false, root, EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY, msg)?;
    conn.flush()?;
    Ok(())
}

/// EXPERIMENT 1: Set window type to utility (often bypasses normal placement)
pub fn set_window_type_utility(conn: &RustConnection, atoms: &Atoms, win: Window) -> Result<()> {
    let data = atoms.net_wm_window_type_utility.to_ne_bytes();
    conn.change_property8(
        PropMode::REPLACE,
        win,
        atoms.net_wm_window_type,
        AtomEnum::ATOM,
        &data,
    )?;
    conn.flush()?;
    Ok(())
}

/// EXPERIMENT 2: Set window to IconicState (minimized from start)
pub fn set_iconic_state(conn: &RustConnection, atoms: &Atoms, root: Window, win: Window) -> Result<()> {
    // IconicState = 3, NormalState = 1
    let data = [3u32, 0, 0, 0, 0]; // IconicState
    let msg = ClientMessageEvent::new(32, win, atoms.wm_change_state, data);
    conn.send_event(false, root, EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY, msg)?;
    conn.flush()?;
    Ok(())
}

/// EXPERIMENT 3: Unmap window immediately after detecting it
pub fn unmap_window(conn: &RustConnection, win: Window) -> Result<()> {
    conn.unmap_window(win)?;
    conn.flush()?;
    Ok(())
}

/// EXPERIMENT 4: Move window off-screen immediately
pub fn move_offscreen(conn: &RustConnection, win: Window) -> Result<()> {
    // Move to coordinates way off screen
    let values = ConfigureWindowAux::new()
        .x(-10000)
        .y(-10000);
    conn.configure_window(win, &values)?;
    conn.flush()?;
    Ok(())
}

/// EXPERIMENT 5: Set window strut to position offscreen
pub fn set_offscreen_strut(conn: &RustConnection, _atoms: &Atoms, win: Window) -> Result<()> {
    let strut_atom = conn.intern_atom(false, b"_NET_WM_STRUT")?.reply()?.atom;
    // left, right, top, bottom - as bytes
    let data: Vec<u8> = vec![0u32, 0, 0, 0]
        .into_iter()
        .flat_map(|n| n.to_ne_bytes())
        .collect();
    conn.change_property8(
        PropMode::REPLACE,
        win,
        strut_atom,
        AtomEnum::CARDINAL,
        &data,
    )?;
    conn.flush()?;
    Ok(())
}

/// EXPERIMENT 6: Set all "invisible" window states
pub fn set_all_invisible_states(conn: &RustConnection, atoms: &Atoms, root: Window, win: Window) -> Result<()> {
    // Add multiple state atoms at once
    let data = [
        1u32, // _NET_WM_STATE_ADD
        atoms.net_wm_state_skip_taskbar,
        atoms.net_wm_state_skip_pager,
        0,
        0,
    ];
    let msg = ClientMessageEvent::new(32, win, atoms.net_wm_state, data);
    conn.send_event(false, root, EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY, msg)?;
    conn.flush()?;
    Ok(())
}

/// EXPERIMENT 7: Lower window to bottom of stack
pub fn lower_window(conn: &RustConnection, win: Window) -> Result<()> {
    let values = ConfigureWindowAux::new().stack_mode(StackMode::BELOW);
    conn.configure_window(win, &values)?;
    conn.flush()?;
    Ok(())
}

pub fn connect() -> Result<(RustConnection, usize, Window)> {
    let (conn, screen_num) = x11rb::connect(None)?;
    let root = conn.setup().roots[screen_num].root;
    Ok((conn, screen_num, root))
}

// Returns 8 groups: Shift, Lock, Control, Mod1..Mod5
pub fn modifier_groups(conn: &RustConnection) -> [HashSet<u8>; 8] {
    let mut groups: [HashSet<u8>; 8] = Default::default();
    if let Ok(cookie) = conn.get_modifier_mapping() {
        if let Ok(r) = cookie.reply() {
            let kpm = r.keycodes_per_modifier() as usize;
            for (i, chunk) in r.keycodes.chunks(kpm).take(8).enumerate() {
                let mut set = HashSet::new();
                for kc in chunk { if *kc != 0 { set.insert(*kc); } }
                groups[i] = set;
            }
        }
    }
    groups
}

// XI2 raw selection is not required; we sample pointer + keymap.
