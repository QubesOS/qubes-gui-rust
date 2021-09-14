//! GUI agent dispatch logic
//!
//! This contains a trait that a GUI agent must implement, which includes
//! callbacks for the various messages an agent must handle.  It also includes
//! dispatch logic for incoming messages.

use qubes_castable::Castable as _;
use qubes_gui::DaemonToAgentEvent;
mod io;
// FIXME move this into separate modules
pub use io::*;

impl super::Client {
    /// Dispatch events received by this [`super::Client`]
    ///
    /// # Panics
    ///
    /// Panics if called on a daemon instance.
    pub fn next_event(&mut self) -> std::io::Result<Option<DaemonToAgentEvent>> {
        assert!(self.agent, "Called next_event on a daemon instance!");
        let (header, body) = match self.vchan.read_header() {
            Ok(None) => return Ok(None),
            Err(e) => return Err(e),
            Ok(Some(s)) => s,
        };
        let window = header.window;
        loop {
            break Ok(Some(match header.ty {
                qubes_gui::MSG_MOTION => {
                    let mut event = qubes_gui::Motion::default();
                    event.as_mut_bytes().copy_from_slice(body);
                    DaemonToAgentEvent::Motion { window, event }
                }
                qubes_gui::MSG_CROSSING => {
                    let mut event = qubes_gui::Crossing::default();
                    event.as_mut_bytes().copy_from_slice(body);
                    DaemonToAgentEvent::Crossing { window, event }
                }
                qubes_gui::MSG_CLOSE => DaemonToAgentEvent::Close { window },
                qubes_gui::MSG_KEYPRESS => {
                    let mut event = qubes_gui::Keypress::default();
                    event.as_mut_bytes().copy_from_slice(body);
                    DaemonToAgentEvent::Keypress { window, event }
                }
                qubes_gui::MSG_BUTTON => {
                    let mut event = qubes_gui::Button::default();
                    event.as_mut_bytes().copy_from_slice(body);
                    DaemonToAgentEvent::Button { window, event }
                }
                qubes_gui::MSG_CLIPBOARD_REQ => DaemonToAgentEvent::Copy,
                qubes_gui::MSG_CLIPBOARD_DATA => {
                    let untrusted_data = std::str::from_utf8(body).map_err(|e| {
                        std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())
                    })?;
                    DaemonToAgentEvent::Paste { untrusted_data }
                }
                qubes_gui::MSG_KEYMAP_NOTIFY => {
                    let mut new_keymap = qubes_gui::KeymapNotify::default();
                    new_keymap.as_mut_bytes().copy_from_slice(body);
                    DaemonToAgentEvent::Keymap { new_keymap }
                }
                qubes_gui::MSG_MAP => {
                    let mut portion_to_redraw = qubes_gui::MapInfo::default();
                    portion_to_redraw.as_mut_bytes().copy_from_slice(body);
                    DaemonToAgentEvent::Redraw {
                        window,
                        portion_to_redraw,
                    }
                }
                qubes_gui::MSG_CONFIGURE => {
                    let mut new_size_and_position = qubes_gui::Configure::default();
                    new_size_and_position.as_mut_bytes().copy_from_slice(body);
                    DaemonToAgentEvent::Configure {
                        window,
                        new_size_and_position,
                    }
                }
                qubes_gui::MSG_FOCUS => {
                    let mut event = qubes_gui::Focus::default();
                    event.as_mut_bytes().copy_from_slice(body);
                    DaemonToAgentEvent::Focus { window, event }
                }
                qubes_gui::MSG_WINDOW_FLAGS => {
                    let mut flags = qubes_gui::WindowFlags::default();
                    flags.as_mut_bytes().copy_from_slice(body);
                    DaemonToAgentEvent::WindowFlags { window, flags }
                }
                _ => continue,
            }));
        }
    }
}
