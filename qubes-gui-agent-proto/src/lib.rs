/*
 * The Qubes OS Project, https://www.qubes-os.org
 *
 * Copyright (C) 2021  Demi Marie Obenour  <demi@invisiblethingslab.com>
 *
 * This program is free software; you can redistribute it and/or
 * modify it under the terms of the GNU General Public License
 * as published by the Free Software Foundation; either version 2
 * of the License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program; if not, write to the Free Software
 * Foundation, Inc., 51 Franklin Street, Fifth Floor, Boston, MA  02110-1301, USA.
 *
 */

#![no_std]
//! Agent-side parser for Qubes OS GUI Protocol
//!
//! This implements agent-side parsing for Qubes OS GUI messages.  It performs
//! no I/O.

use core::convert::TryInto as _;
use qubes_castable::Castable as _;

/// Errors when parsing an agent-side Qubes OS GUI Protocol message.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Error {
    /// Invalid UTF-8
    BadUTF8(core::str::Utf8Error),
    /// Invalid key event type
    BadKeypress {
        /// The type provided by the GUI daemon
        ty: u32,
    },
    /// Invalid button event status
    BadButton {
        /// The type provided by the GUI daemon
        ty: u32,
    },
    /// Invalid focus event status
    BadFocus {
        /// The type provided by the GUI daemon
        ty: u32,
    },
}

/// An event that a GUI agent must handle
#[non_exhaustive]
pub enum DaemonToAgentEvent<'a> {
    /// The pointer has moved
    Motion {
        /// The contents of the event
        event: qubes_gui::Motion,
    },
    /// The pointer has entered or left a window.
    Crossing {
        /// The contents of the event
        event: qubes_gui::Crossing,
    },
    /// The user wishes to close a window
    Close,
    /// A key has been pressed or released
    Keypress {
        /// The contents of the event
        event: qubes_gui::Keypress,
    },
    /// A button has been pressed or released
    Button {
        /// The contents of the event
        event: qubes_gui::Button,
    },
    /// The GUI daemon has requested the contents of the clipboard.  The agent
    /// is expected to send a [`MSG_CLIPBOARD_DATA`] message with the
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
        /// The portion of the window to redraw
        portion_to_redraw: qubes_gui::MapInfo,
    },
    /// A window has been moved and/or resized.
    Configure {
        /// The contents of the event
        new_size_and_position: qubes_gui::Configure,
    },
    /// A window has gained or lost focus
    Focus {
        /// The contents of the event
        event: qubes_gui::Focus,
    },
    /// Window manager flags have changed
    WindowFlags {
        /// The contents of the event
        flags: qubes_gui::WindowFlags,
    },
    /// GUI daemon confirms window destruciton; window ID may be reused
    Destroy,
}

impl<'a> DaemonToAgentEvent<'a> {
    /// Parse a Qubes OS GUI message from the GUI daemon
    ///
    /// # Panics
    ///
    /// Will panic if the length of the message does not match the length in the
    /// header.  May (or may not!) panic if the length of the message is not valid.
    /// The caller is expected to have validated this earlier, using
    /// [`qubes_gui::msg_length_limits`].
    ///
    /// # Return
    ///
    /// Returns `Ok(Some(window, event))` on success.  Returns `Ok(None)` if the
    /// message number is not known.
    ///
    /// # Errors
    ///
    /// Fails if the given GUI message cannot be parsed.
    pub fn parse(header: qubes_gui::Header, body: &'a [u8]) -> Result<Option<(u32, Self)>, Error> {
        assert_eq!(
            header.untrusted_len.try_into(),
            Ok(body.len()),
            "Wrong body length provided!"
        );
        let window = header.window;
        let res = match header.ty {
            qubes_gui::MSG_MOTION => {
                let mut event = qubes_gui::Motion::default();
                event.as_mut_bytes().copy_from_slice(body);
                DaemonToAgentEvent::Motion { event }
            }
            qubes_gui::MSG_CROSSING => {
                let mut event = qubes_gui::Crossing::default();
                event.as_mut_bytes().copy_from_slice(body);
                DaemonToAgentEvent::Crossing { event }
            }
            qubes_gui::MSG_CLOSE => DaemonToAgentEvent::Close,
            qubes_gui::MSG_KEYPRESS => {
                let mut event = qubes_gui::Keypress::default();
                event.as_mut_bytes().copy_from_slice(body);
                DaemonToAgentEvent::Keypress { event }
            }
            qubes_gui::MSG_BUTTON => {
                let mut event = qubes_gui::Button::default();
                event.as_mut_bytes().copy_from_slice(body);
                DaemonToAgentEvent::Button { event }
            }
            qubes_gui::MSG_CLIPBOARD_REQ => DaemonToAgentEvent::Copy,
            qubes_gui::MSG_CLIPBOARD_DATA => {
                let untrusted_data = core::str::from_utf8(body).map_err(Error::BadUTF8)?;
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
                DaemonToAgentEvent::Redraw { portion_to_redraw }
            }
            qubes_gui::MSG_CONFIGURE => {
                let mut new_size_and_position = qubes_gui::Configure::default();
                new_size_and_position.as_mut_bytes().copy_from_slice(body);
                DaemonToAgentEvent::Configure {
                    new_size_and_position,
                }
            }
            qubes_gui::MSG_FOCUS => {
                let mut event = qubes_gui::Focus::default();
                event.as_mut_bytes().copy_from_slice(body);
                DaemonToAgentEvent::Focus { event }
            }
            qubes_gui::MSG_WINDOW_FLAGS => {
                let mut flags = qubes_gui::WindowFlags::default();
                flags.as_mut_bytes().copy_from_slice(body);
                DaemonToAgentEvent::WindowFlags { flags }
            }
            qubes_gui::MSG_DESTROY => DaemonToAgentEvent::Destroy,
            _ => return Ok(None),
        };
        Ok(Some((window, res)))
    }
}
