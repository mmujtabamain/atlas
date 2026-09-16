//! Thin X11 layer: find the app's window, read its pixels, inject input.
//!
//! Uses `x11rb`'s pure-Rust connection, so nothing here needs libxcb or a
//! window manager. Without a WM every app window is a direct child of the
//! root window, which keeps discovery simple: walk `QueryTree(root)`.

use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use log::{debug, info, trace};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    self, AtomEnum, ConnectionExt as _, ImageFormat, ImageOrder, MapState, Window, WindowClass,
};
use x11rb::protocol::xtest::ConnectionExt as _;
use x11rb::rust_connection::RustConnection;

use crate::frame::{Frame, PixelLayout};

/// What a caller can match a window on.
#[derive(Debug, Clone)]
pub enum WindowMatch {
    /// `_NET_WM_PID` equals this process id (gpui sets it on every window).
    Pid(u32),
    /// `_NET_WM_NAME` / `WM_NAME` contains this substring.
    TitleContains(String),
    /// An exact window id.
    Id(Window),
}

#[derive(Debug, Clone)]
pub struct WindowInfo {
    pub id: Window,
    pub title: String,
    pub pid: Option<u32>,
    pub x: i16,
    pub y: i16,
    pub width: u16,
    pub height: u16,
    pub depth: u8,
    pub override_redirect: bool,
}

pub struct X11 {
    conn: RustConnection,
    root: Window,
    screen_num: usize,
    atom_net_wm_pid: xproto::Atom,
    atom_net_wm_name: xproto::Atom,
    atom_utf8_string: xproto::Atom,
}

impl X11 {
    pub fn connect(display: &str) -> Result<X11> {
        let (conn, screen_num) = x11rb::connect(Some(display))
            .with_context(|| format!("connecting to X display {display}"))?;
        let root = conn.setup().roots[screen_num].root;
        let atom = |name: &[u8]| -> Result<xproto::Atom> {
            Ok(conn.intern_atom(false, name)?.reply()?.atom)
        };
        let atom_net_wm_pid = atom(b"_NET_WM_PID")?;
        let atom_net_wm_name = atom(b"_NET_WM_NAME")?;
        let atom_utf8_string = atom(b"UTF8_STRING")?;
        debug!(
            "connected to {display}: screen {screen_num}, root 0x{root:x}, {}x{}",
            conn.setup().roots[screen_num].width_in_pixels,
            conn.setup().roots[screen_num].height_in_pixels
        );
        Ok(X11 {
            conn,
            root,
            screen_num,
            atom_net_wm_pid,
            atom_net_wm_name,
            atom_utf8_string,
        })
    }

    /// All mapped, input/output top-level windows with a non-trivial size.
    pub fn top_level_windows(&self) -> Result<Vec<WindowInfo>> {
        let children = self.conn.query_tree(self.root)?.reply()?.children;
        let mut windows = Vec::new();
        for id in children {
            let Ok(attrs) = self.conn.get_window_attributes(id)?.reply() else {
                continue; // window vanished between QueryTree and here
            };
            if attrs.map_state != MapState::VIEWABLE || attrs.class != WindowClass::INPUT_OUTPUT {
                continue;
            }
            let Ok(geometry) = self.conn.get_geometry(id)?.reply() else {
                continue;
            };
            if geometry.width <= 1 || geometry.height <= 1 {
                continue;
            }
            windows.push(WindowInfo {
                id,
                title: self.window_title(id)?,
                pid: self.window_pid(id)?,
                x: geometry.x,
                y: geometry.y,
                width: geometry.width,
                height: geometry.height,
                depth: geometry.depth,
                override_redirect: attrs.override_redirect,
            });
        }
        Ok(windows)
    }

    fn window_pid(&self, id: Window) -> Result<Option<u32>> {
        let reply = self
            .conn
            .get_property(false, id, self.atom_net_wm_pid, AtomEnum::CARDINAL, 0, 1)?
            .reply()?;
        Ok(reply.value32().and_then(|mut values| values.next()))
    }

    fn window_title(&self, id: Window) -> Result<String> {
        let utf8 = self
            .conn
            .get_property(false, id, self.atom_net_wm_name, self.atom_utf8_string, 0, 1024)?
            .reply()?;
        if !utf8.value.is_empty() {
            return Ok(String::from_utf8_lossy(&utf8.value).into_owned());
        }
        let legacy = self
            .conn
            .get_property(false, id, AtomEnum::WM_NAME, AtomEnum::ANY, 0, 1024)?
            .reply()?;
        Ok(String::from_utf8_lossy(&legacy.value).into_owned())
    }

    /// Polls until a window matching `matcher` is mapped, or `timeout` passes.
    /// When several match (gpui popups share the app's pid) the largest
    /// non-override-redirect one wins: that is the main window.
    pub fn wait_for_window(&self, matcher: &WindowMatch, timeout: Duration) -> Result<WindowInfo> {
        let started = std::time::Instant::now();
        let mut logged = 0usize;
        loop {
            let windows = self.top_level_windows()?;
            if windows.len() != logged {
                for w in &windows {
                    trace!("window 0x{:x} pid={:?} {}x{} title={:?}", w.id, w.pid, w.width, w.height, w.title);
                }
                logged = windows.len();
            }
            let mut candidates: Vec<WindowInfo> = windows
                .into_iter()
                .filter(|w| match matcher {
                    WindowMatch::Pid(pid) => w.pid == Some(*pid),
                    WindowMatch::TitleContains(needle) => w.title.contains(needle.as_str()),
                    WindowMatch::Id(id) => w.id == *id,
                })
                .collect();
            candidates.sort_by_key(|w| (w.override_redirect, std::cmp::Reverse(w.width as u32 * w.height as u32)));
            if let Some(best) = candidates.into_iter().next() {
                info!(
                    "found window 0x{:x} {}x{} at ({},{}) depth {} title={:?} after {:?}",
                    best.id, best.width, best.height, best.x, best.y, best.depth, best.title, started.elapsed()
                );
                return Ok(best);
            }
            if started.elapsed() > timeout {
                let listing = self
                    .top_level_windows()?
                    .iter()
                    .map(|w| format!("0x{:x} pid={:?} {}x{} {:?}", w.id, w.pid, w.width, w.height, w.title))
                    .collect::<Vec<_>>()
                    .join(", ");
                bail!("no window matching {matcher:?} appeared within {timeout:?}; mapped windows: [{listing}]");
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// Re-reads geometry (gpui may resize right after mapping).
    pub fn refresh(&self, id: Window) -> Result<WindowInfo> {
        let geometry = self.conn.get_geometry(id)?.reply().context("window is gone")?;
        let attrs = self.conn.get_window_attributes(id)?.reply()?;
        Ok(WindowInfo {
            id,
            title: self.window_title(id)?,
            pid: self.window_pid(id)?,
            x: geometry.x,
            y: geometry.y,
            width: geometry.width,
            height: geometry.height,
            depth: geometry.depth,
            override_redirect: attrs.override_redirect,
        })
    }

    /// Reads the window's current pixels with `GetImage`.
    pub fn capture(&self, id: Window) -> Result<Frame> {
        let geometry = self.conn.get_geometry(id)?.reply().context("window is gone")?;
        let reply = self
            .conn
            .get_image(ImageFormat::Z_PIXMAP, id, 0, 0, geometry.width, geometry.height, !0)?
            .reply()
            .context("GetImage failed (window unmapped or off-screen?)")?;
        let layout = self.pixel_layout(reply.depth, reply.visual)?;
        trace!(
            "GetImage: {}x{} depth {} visual 0x{:x} -> {:?}, {} bytes",
            geometry.width, geometry.height, reply.depth, reply.visual, layout, reply.data.len()
        );
        Frame::from_zpixmap(geometry.width as u32, geometry.height as u32, &reply.data, layout)
    }

    fn pixel_layout(&self, depth: u8, visual_id: xproto::Visualid) -> Result<PixelLayout> {
        let setup = self.conn.setup();
        let bits_per_pixel = setup
            .pixmap_formats
            .iter()
            .find(|f| f.depth == depth)
            .map(|f| f.bits_per_pixel)
            .ok_or_else(|| anyhow!("server has no pixmap format for depth {depth}"))?;
        let screen = &setup.roots[self.screen_num];
        let visual = screen
            .allowed_depths
            .iter()
            .flat_map(|d| d.visuals.iter())
            .find(|v| v.visual_id == visual_id)
            .or_else(|| {
                screen
                    .allowed_depths
                    .iter()
                    .flat_map(|d| d.visuals.iter())
                    .find(|v| v.visual_id == screen.root_visual)
            })
            .ok_or_else(|| anyhow!("cannot find visual 0x{visual_id:x} on the screen"))?;
        Ok(PixelLayout {
            bits_per_pixel,
            lsb_first: setup.image_byte_order == ImageOrder::LSB_FIRST,
            red_mask: visual.red_mask,
            green_mask: visual.green_mask,
            blue_mask: visual.blue_mask,
        })
    }

    /// Moves the pointer to window-relative `(x, y)` without clicking (hover).
    pub fn hover(&self, id: Window, x: i16, y: i16) -> Result<()> {
        let pos = self.conn.translate_coordinates(id, self.root, x, y)?.reply()?;
        debug!("hover at window ({x},{y}) = root ({},{})", pos.dst_x, pos.dst_y);
        self.fake_input(xproto::MOTION_NOTIFY_EVENT, 0, pos.dst_x, pos.dst_y)
    }

    /// Parks the pointer in the bottom-right corner of the screen, outside a
    /// centred window, so nothing is hovered while we capture.
    pub fn park_pointer(&self) -> Result<()> {
        let screen = &self.conn.setup().roots[self.screen_num];
        let (x, y) = (screen.width_in_pixels as i16 - 1, screen.height_in_pixels as i16 - 1);
        debug!("parking pointer at root ({x},{y})");
        self.fake_input(xproto::MOTION_NOTIFY_EVENT, 0, x, y)
    }

    /// Moves the pointer to window-relative `(x, y)` and clicks button 1 via XTEST.
    pub fn click(&self, id: Window, x: i16, y: i16) -> Result<()> {
        let pos = self.conn.translate_coordinates(id, self.root, x, y)?.reply()?;
        debug!("click at window ({x},{y}) = root ({},{})", pos.dst_x, pos.dst_y);
        self.fake_input(xproto::MOTION_NOTIFY_EVENT, 0, pos.dst_x, pos.dst_y)?;
        std::thread::sleep(Duration::from_millis(30));
        self.fake_input(xproto::BUTTON_PRESS_EVENT, 1, pos.dst_x, pos.dst_y)?;
        std::thread::sleep(Duration::from_millis(30));
        self.fake_input(xproto::BUTTON_RELEASE_EVENT, 1, pos.dst_x, pos.dst_y)?;
        Ok(())
    }

    /// Presses button 1 at window-relative `(x1, y1)`, moves to `(x2, y2)` in
    /// a few increments and leaves the button held, so the app is mid-drag
    /// (its drag preview and drop indicator on screen) when the next step or
    /// the capture happens. [`X11::release`] ends it.
    pub fn drag_hold(&self, id: Window, x1: i16, y1: i16, x2: i16, y2: i16) -> Result<()> {
        let from = self.conn.translate_coordinates(id, self.root, x1, y1)?.reply()?;
        let to = self.conn.translate_coordinates(id, self.root, x2, y2)?.reply()?;
        debug!("drag from window ({x1},{y1}) to ({x2},{y2}) = root ({},{}) -> ({},{}), holding", from.dst_x, from.dst_y, to.dst_x, to.dst_y);
        self.fake_input(xproto::MOTION_NOTIFY_EVENT, 0, from.dst_x, from.dst_y)?;
        std::thread::sleep(Duration::from_millis(30));
        self.fake_input(xproto::BUTTON_PRESS_EVENT, 1, from.dst_x, from.dst_y)?;
        std::thread::sleep(Duration::from_millis(30));
        // Several motions rather than one jump: the app's drag threshold and
        // its drop zones both watch the pointer move.
        const STEPS: i32 = 8;
        for step in 1..=STEPS {
            let x = i32::from(from.dst_x) + (i32::from(to.dst_x) - i32::from(from.dst_x)) * step / STEPS;
            let y = i32::from(from.dst_y) + (i32::from(to.dst_y) - i32::from(from.dst_y)) * step / STEPS;
            self.fake_input(xproto::MOTION_NOTIFY_EVENT, 0, x as i16, y as i16)?;
            std::thread::sleep(Duration::from_millis(30));
        }
        Ok(())
    }

    /// Releases button 1 where the pointer is, dropping whatever a
    /// [`X11::drag_hold`] picked up.
    pub fn release(&self) -> Result<()> {
        let pointer = self.conn.query_pointer(self.root)?.reply()?;
        debug!("release at root ({},{})", pointer.root_x, pointer.root_y);
        self.fake_input(xproto::BUTTON_RELEASE_EVENT, 1, pointer.root_x, pointer.root_y)
    }

    /// Moves the pointer to window-relative `(x, y)` and turns the wheel:
    /// button 5 scrolls down, button 4 up, one press/release per notch.
    pub fn wheel(&self, id: Window, x: i16, y: i16, clicks: i32) -> Result<()> {
        let pos = self.conn.translate_coordinates(id, self.root, x, y)?.reply()?;
        let button = if clicks >= 0 { 5 } else { 4 };
        debug!("wheel {clicks} at window ({x},{y}) = root ({},{})", pos.dst_x, pos.dst_y);
        self.fake_input(xproto::MOTION_NOTIFY_EVENT, 0, pos.dst_x, pos.dst_y)?;
        std::thread::sleep(Duration::from_millis(30));
        for _ in 0..clicks.unsigned_abs() {
            self.fake_input(xproto::BUTTON_PRESS_EVENT, button, pos.dst_x, pos.dst_y)?;
            self.fake_input(xproto::BUTTON_RELEASE_EVENT, button, pos.dst_x, pos.dst_y)?;
            std::thread::sleep(Duration::from_millis(20));
        }
        Ok(())
    }

    /// Presses and releases the key for `keysym_name` via XTEST.
    pub fn key(&self, keysym_name: &str) -> Result<()> {
        let keysym = keysym_from_name(keysym_name)
            .ok_or_else(|| anyhow!("unknown keysym `{keysym_name}` (supported: named keys like Return/Escape/Tab/arrows/F1-F12 and single ASCII characters)"))?;
        let keycode = self.keycode_for(keysym)?;
        debug!("key {keysym_name}: keysym 0x{keysym:x} -> keycode {keycode}");
        self.fake_input(xproto::KEY_PRESS_EVENT, keycode, 0, 0)?;
        std::thread::sleep(Duration::from_millis(30));
        self.fake_input(xproto::KEY_RELEASE_EVENT, keycode, 0, 0)?;
        Ok(())
    }

    fn keycode_for(&self, keysym: u32) -> Result<u8> {
        let setup = self.conn.setup();
        let (min, max) = (setup.min_keycode, setup.max_keycode);
        let mapping = self.conn.get_keyboard_mapping(min, max - min + 1)?.reply()?;
        let per = mapping.keysyms_per_keycode as usize;
        for (index, syms) in mapping.keysyms.chunks(per).enumerate() {
            if syms.contains(&keysym) {
                return Ok(min + index as u8);
            }
        }
        bail!("keysym 0x{keysym:x} is not on the server's keyboard map")
    }

    fn fake_input(&self, event_type: u8, detail: u8, root_x: i16, root_y: i16) -> Result<()> {
        self.conn
            .xtest_fake_input(event_type, detail, x11rb::CURRENT_TIME, self.root, root_x, root_y, 0)?
            .check()
            .context("XTEST fake input (does the server have the XTEST extension?)")?;
        self.conn.flush()?;
        Ok(())
    }
}

/// A small keysym table: the named keys a screenshot script realistically
/// needs, plus printable ASCII (whose keysym equals its code point).
pub fn keysym_from_name(name: &str) -> Option<u32> {
    let named = match name {
        "Return" | "Enter" => 0xff0d,
        "Escape" | "Esc" => 0xff1b,
        "Tab" => 0xff09,
        "BackSpace" | "Backspace" => 0xff08,
        "Delete" => 0xffff,
        "space" | "Space" => 0x20,
        "Up" => 0xff52,
        "Down" => 0xff54,
        "Left" => 0xff51,
        "Right" => 0xff53,
        "Home" => 0xff50,
        "End" => 0xff57,
        "Page_Up" | "PageUp" => 0xff55,
        "Page_Down" | "PageDown" => 0xff56,
        _ => 0,
    };
    if named != 0 {
        return Some(named);
    }
    if let Some(n) = name.strip_prefix('F') {
        if let Ok(n) = n.parse::<u32>() {
            if (1..=12).contains(&n) {
                return Some(0xffbe + n - 1);
            }
        }
    }
    let mut chars = name.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if c.is_ascii_graphic() || c == ' ' => Some(c as u32),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keysym_table_covers_named_keys_function_keys_and_ascii() {
        assert_eq!(keysym_from_name("Return"), Some(0xff0d));
        assert_eq!(keysym_from_name("Escape"), Some(0xff1b));
        assert_eq!(keysym_from_name("F1"), Some(0xffbe));
        assert_eq!(keysym_from_name("F12"), Some(0xffc9));
        assert_eq!(keysym_from_name("F13"), None);
        assert_eq!(keysym_from_name("a"), Some(0x61));
        assert_eq!(keysym_from_name("/"), Some(0x2f));
        assert_eq!(keysym_from_name("ab"), None);
        assert_eq!(keysym_from_name("Bogus"), None);
        assert_eq!(keysym_from_name(""), None);
    }
}
