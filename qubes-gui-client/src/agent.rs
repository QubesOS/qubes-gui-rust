//! GUI agent dispatch logic
//!
//! This contains a trait that a GUI agent must implement, which includes
//! callbacks for the various messages an agent must handle.  It also includes
//! dispatch logic for incoming messages.

use qubes_castable::Castable as _;
mod io;
// FIXME move this into separate modules
pub use io::*;

/// An event that a GUI agent must handle
#[non_exhaustive]
pub enum DaemonToAgentEvent<'a> {
    /// The pointer has moved
    Motion {
        /// The window the event is sent to
        window: u32,
        /// The contents of the event
        event: qubes_gui::Motion,
    },
    /// The pointer has entered or left a window.
    Crossing {
        /// The window the event is sent to
        window: u32,
        /// The contents of the event
        event: qubes_gui::Crossing,
    },
    /// The user wishes to close a window
    Close {
        /// The window the event is sent to
        window: u32,
    },
    /// A key has been pressed or released
    Keypress {
        /// The window the event is sent to
        window: u32,
        /// The contents of the event
        event: qubes_gui::Keypress,
    },
    /// A button has been pressed or released
    Button {
        /// The window the event is sent to
        window: u32,
        /// The contents of the event
        event: qubes_gui::Button,
    },
    /// The GUI daemon has requested the contents of the clipboard.  The agent
    /// is expected to send a [`qubes_gui::MSG_CLIPBOARD_DATA`] message with the
    /// requested data.
    Copy,
    /// Set the contents of the clipboard.
    Paste {
        /// The pasted data, which is not trusted
        untrusted_data: &'a str,
    },
    /// The keymap has changed.
    Keymap {
        /// The new keymap
        new_keymap: qubes_gui::KeymapNotify,
    },
    /// The agent must redraw a portion of the display
    Redraw {
        /// The window that needs to be redrawn
        window: u32,
        /// The portion of the window to redraw
        portion_to_redraw: qubes_gui::MapInfo,
    },
    /// A window has been moved and/or resized.
    Configure {
        /// The window the event was sent to
        window: u32,
        /// The contents of the event
        new_size_and_position: qubes_gui::Configure,
    },
    /// A window has gained or lost focus
    Focus {
        /// The window the event was sent to
        window: u32,
        /// The contents of the event
        event: qubes_gui::Focus,
    },
    /// Window manager flags have changed
    WindowFlags {
        /// The window the event was sent to
        window: u32,
        /// The contents of the event
        flags: qubes_gui::WindowFlags,
    },
}

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
