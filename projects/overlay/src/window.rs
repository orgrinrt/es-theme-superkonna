//! X11 overlay window using override-redirect for always-on-top without WM interaction.
//! Uses 32-bit ARGB visual for transparency. Supports runtime resize/reposition
//! for switching between popup (small, top-right) and menu (full-screen) modes.

use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

use log::debug;

pub struct OverlayWindow {
    conn: RustConnection,
    window: Window,
    gc: Gcontext,
    visible: bool,
    current_width: u16,
    current_height: u16,
    current_x: i16,
    current_y: i16,
    screen_width: u16,
    screen_height: u16,
}

impl OverlayWindow {
    pub fn new(width: u16, height: u16) -> Result<Self, String> {
        let (conn, screen_num) = RustConnection::connect(None).map_err(|e| format!("X11 connect: {e}"))?;
        let screen = &conn.setup().roots[screen_num];
        let screen_width = screen.width_in_pixels;
        let screen_height = screen.height_in_pixels;

        let (visual, depth) = find_argb_visual(screen).unwrap_or((screen.root_visual, screen.root_depth));

        let colormap = conn.generate_id().map_err(|e| e.to_string())?;
        conn.create_colormap(ColormapAlloc::NONE, colormap, screen.root, visual)
            .map_err(|e| e.to_string())?;

        let window = conn.generate_id().map_err(|e| e.to_string())?;
        // Position at origin for full-screen overlay; popup compositing handles placement
        let x = 0_i16;
        let y = 0_i16;

        let values = CreateWindowAux::new()
            .override_redirect(1)
            .background_pixel(0)
            .border_pixel(0)
            .colormap(colormap)
            .event_mask(EventMask::EXPOSURE | EventMask::STRUCTURE_NOTIFY);

        conn.create_window(depth, window, screen.root, x, y, width, height, 0, WindowClass::INPUT_OUTPUT, visual, &values)
            .map_err(|e| format!("create_window: {e}"))?;

        let atom_type = intern_atom(&conn, "_NET_WM_WINDOW_TYPE")?;
        let atom_notif = intern_atom(&conn, "_NET_WM_WINDOW_TYPE_NOTIFICATION")?;
        conn.change_property32(PropMode::REPLACE, window, atom_type, AtomEnum::ATOM, &[atom_notif])
            .map_err(|e| e.to_string())?;

        let atom_state = intern_atom(&conn, "_NET_WM_STATE")?;
        let atom_above = intern_atom(&conn, "_NET_WM_STATE_ABOVE")?;
        let atom_sticky = intern_atom(&conn, "_NET_WM_STATE_STICKY")?;
        conn.change_property32(PropMode::REPLACE, window, atom_state, AtomEnum::ATOM, &[atom_above, atom_sticky])
            .map_err(|e| e.to_string())?;

        conn.change_property8(PropMode::REPLACE, window, AtomEnum::WM_NAME, AtomEnum::STRING, b"superkonna-overlay")
            .map_err(|e| e.to_string())?;

        let gc = conn.generate_id().map_err(|e| e.to_string())?;
        conn.create_gc(gc, window, &CreateGCAux::new()).map_err(|e| e.to_string())?;
        conn.flush().map_err(|e| e.to_string())?;

        debug!("Window created: {width}x{height} at ({x},{y}), screen={screen_width}x{screen_height}");

        Ok(OverlayWindow {
            conn, window, gc, visible: false,
            current_width: width, current_height: height,
            current_x: x, current_y: y,
            screen_width, screen_height,
        })
    }

    pub fn screen_size(&self) -> (u16, u16) {
        (self.screen_width, self.screen_height)
    }

    /// Resize the window. Only sends X11 request if dimensions changed.
    pub fn resize(&mut self, width: u16, height: u16) {
        if width == self.current_width && height == self.current_height {
            return;
        }
        let values = ConfigureWindowAux::new().width(width as u32).height(height as u32);
        let _ = self.conn.configure_window(self.window, &values);
        self.current_width = width;
        self.current_height = height;
    }

    /// Reposition the window. Only sends X11 request if position changed.
    pub fn reposition(&mut self, x: i16, y: i16) {
        if x == self.current_x && y == self.current_y {
            return;
        }
        let values = ConfigureWindowAux::new().x(x as i32).y(y as i32);
        let _ = self.conn.configure_window(self.window, &values);
        self.current_x = x;
        self.current_y = y;
    }

    pub fn show(&mut self) {
        if !self.visible {
            let _ = self.conn.map_window(self.window);
            let _ = self.conn.flush();
            self.visible = true;
        }
    }

    pub fn hide(&mut self) {
        if self.visible {
            let _ = self.conn.unmap_window(self.window);
            let _ = self.conn.flush();
            self.visible = false;
        }
    }

    pub fn update_pixels(&self, pixels: &[u32], width: u16, height: u16) {
        let mut data = Vec::with_capacity(pixels.len() * 4);
        for &px in pixels {
            data.push((px & 0xFF) as u8);
            data.push(((px >> 8) & 0xFF) as u8);
            data.push(((px >> 16) & 0xFF) as u8);
            data.push(((px >> 24) & 0xFF) as u8);
        }

        let _ = self.conn.put_image(ImageFormat::Z_PIXMAP, self.window, self.gc, width, height, 0, 0, 0, 32, &data);
        let _ = self.conn.flush();
    }

    pub fn poll_events(&self) {
        while let Ok(Some(event)) = self.conn.poll_for_event() {
            match event {
                Event::Expose(_) => debug!("Expose event"),
                _ => {}
            }
        }
    }
}

fn find_argb_visual(screen: &Screen) -> Option<(Visualid, u8)> {
    for depth_info in &screen.allowed_depths {
        if depth_info.depth == 32 {
            for visual in &depth_info.visuals {
                if visual.class == VisualClass::TRUE_COLOR {
                    return Some((visual.visual_id, 32));
                }
            }
        }
    }
    None
}

fn intern_atom(conn: &RustConnection, name: &str) -> Result<Atom, String> {
    conn.intern_atom(false, name.as_bytes())
        .map_err(|e| e.to_string())?
        .reply()
        .map(|r| r.atom)
        .map_err(|e| e.to_string())
}
