//! GUI agent dispatch logic
//!
//! This contains a trait that a GUI agent must implement, which includes
//! callbacks for the various messages an agent must handle.  It also includes
//! dispatch logic for incoming messages.

use qubes_castable::Castable as _;
mod io;
// FIXME move this into separate modules
pub use io::*;

/// The trait that a GUI agent must implement.
pub trait AgentTrait {
    /// Called when a motion event is received on the vchan
    fn motion(&mut self, window: u32, event: qubes_gui::Motion) -> std::io::Result<()>;

    /// The pointer has entered or left a window.
    fn crossing(&mut self, window: u32, event: qubes_gui::Crossing) -> std::io::Result<()>;

    /// The user wishes to close a window
    fn close(&mut self, window: u32) -> std::io::Result<()>;

    /// A key has been pressed or released
    fn keypress(&mut self, window: u32, event: qubes_gui::Keypress) -> std::io::Result<()>;

    /// A button has been pressed or released
    fn button(&mut self, window: u32, button: qubes_gui::Button) -> std::io::Result<()>;

    /// The GUI daemon has requested the contents of the clipboard.  The agent
    /// is expected to send a [`qubes_gui::MSG_CLIPBOARD_DATA`] message with the
    /// requested data.
    fn copy(&mut self) -> std::io::Result<()>;

    /// Set the contents of the clipboard.
    fn paste(&mut self, paste_buffer: &str) -> std::io::Result<()>;

    /// The keymap has changed.
    fn keymap(&mut self, keymap: qubes_gui::KeymapNotify) -> std::io::Result<()>;

    /// The agent must redraw a portion of the display
    fn redraw(&mut self, window: u32, portion_to_redraw: qubes_gui::MapInfo)
        -> std::io::Result<()>;

    /// A window has been moved and/or resized.
    fn configure(
        &mut self,
        window: u32,
        new_size_and_positon: qubes_gui::Configure,
    ) -> std::io::Result<()>;

    /// A window has gained or lost focus
    fn focus(&mut self, window: u32, event: qubes_gui::Focus) -> std::io::Result<()>;

    /// Window manager flags have changed
    fn window_flags(&mut self, window: u32, flags: qubes_gui::WindowFlags) -> std::io::Result<()>;
}

impl super::Client {
    /// Dispatch events received by this [`super::Client`]
    ///
    /// # Panics
    ///
    /// Panics if called on a daemon instance.
    pub fn dispatch_events(&mut self, implementation: &mut dyn AgentTrait) -> std::io::Result<()> {
        self.wait();
        loop {
            let (header, body) = match self.vchan.read_header() {
                Ok(None) => break Ok(()),
                Err(e) => break Err(e),
                Ok(Some(s)) => s,
            };
            match header.ty {
                qubes_gui::MSG_MOTION => {
                    let mut m = qubes_gui::Motion::default();
                    m.as_mut_bytes().copy_from_slice(body);
                    implementation.motion(header.window, m)?
                }
                qubes_gui::MSG_CROSSING => {
                    let mut m = qubes_gui::Crossing::default();
                    m.as_mut_bytes().copy_from_slice(body);
                    implementation.crossing(header.window, m)?
                }
                qubes_gui::MSG_CLOSE => {
                    assert!(body.is_empty());
                    implementation.close(header.window)?
                }
                qubes_gui::MSG_KEYPRESS => {
                    let mut m = qubes_gui::Keypress::default();
                    m.as_mut_bytes().copy_from_slice(body);
                    implementation.keypress(header.window, m)?
                }
                qubes_gui::MSG_BUTTON => {
                    let mut m = qubes_gui::Button::default();
                    m.as_mut_bytes().copy_from_slice(body);
                    implementation.button(header.window, m)?
                }
                qubes_gui::MSG_CLIPBOARD_REQ => implementation.copy()?,
                qubes_gui::MSG_CLIPBOARD_DATA => {
                    implementation.paste(std::str::from_utf8(body).map_err(|e| {
                        std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())
                    })?)?
                }
                qubes_gui::MSG_KEYMAP_NOTIFY => {
                    let mut m = qubes_gui::KeymapNotify::default();
                    m.as_mut_bytes().copy_from_slice(body);
                    implementation.keymap(m)?
                }
                qubes_gui::MSG_MAP => {
                    let mut m = qubes_gui::MapInfo::default();
                    m.as_mut_bytes().copy_from_slice(body);
                    implementation.redraw(header.window, m)?
                }
                qubes_gui::MSG_CONFIGURE => {
                    let mut m = qubes_gui::Configure::default();
                    m.as_mut_bytes().copy_from_slice(body);
                    implementation.configure(header.window, m)?
                }
                qubes_gui::MSG_FOCUS => {
                    let mut m = qubes_gui::Focus::default();
                    m.as_mut_bytes().copy_from_slice(body);
                    implementation.focus(header.window, m)?
                }
                qubes_gui::MSG_WINDOW_FLAGS => {
                    let mut m = qubes_gui::WindowFlags::default();
                    m.as_mut_bytes().copy_from_slice(body);
                    implementation.window_flags(header.window, m)?
                }
                _ => {}
            }
        }
    }
}
