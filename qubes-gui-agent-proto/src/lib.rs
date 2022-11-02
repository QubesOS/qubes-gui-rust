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
#![forbid(clippy::all)]
//! Agent-side parser for Qubes OS GUI Protocol
//!
//! This implements agent-side parsing for Qubes OS GUI messages.  It performs
//! no I/O.

use core::convert::TryInto as _;
use qubes_castable::Castable;

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

/// A GUI protocol event
#[non_exhaustive]
pub enum Event<'a> {
    /// Daemon ⇒ agent: A key has been pressed or released
    Keypress(qubes_gui::Keypress),
    /// Daemon ⇒ agent: A button has been pressed or released
    Button(qubes_gui::Button),
    /// Daemon ⇒ agent: The pointer has moved
    Motion(qubes_gui::Motion),
    /// Daemon ⇒ agent: The pointer has entered or left a window.
    Crossing(qubes_gui::Crossing),
    /// Daemon ⇒ agent: A window has just acquired focus.
    Focus(qubes_gui::Focus),
    /// Daemon ⇒ agent, obsolete.
    Resize(qubes_gui::Rectangle),
    /// Agent ⇒ daemon: Create a window
    Create(qubes_gui::Create),
    /// Bidirectional: Agent wishes to destroy a window, or daemon confirms
    /// window destruction.
    Destroy,
    /// Bidirectional: The agent must redraw a portion of the display,
    /// or the agent requests that a window be mapped on screen.
    Redraw(qubes_gui::MapInfo),
    /// Agent ⇒ daemon: Unmap a window
    Unmap,
    /// Bidrectional: A window has been moved and/or resized.
    Configure(qubes_gui::Configure),
    /// Ask dom0 (qubes_gui::only!) to map the given amount of memory into composition
    /// buffer.  Deprecated.
    MfnDump(qubes_gui::ShmCmd),
    /// Agent ⇒ daemon: Redraw given area of screen.
    ShmImage(qubes_gui::ShmImage),
    /// Daemon ⇒ agent: The user wishes to close a window
    Close,
    /// Daemon ⇒ agent: Request clipboard data.  The agent is expected to send a
    /// [`qubes_gui::MSG_CLIPBOARD_DATA`] message with the requested data.
    ClipboardReq,
    /// Agent ⇒ daemon: Set the contents of the clipboard.  The contents of the
    /// clipboard are not trusted.
    ClipboardData {
        /// UNTRUSTED (though valid UTF-8) clipboard data!
        untrusted_data: &'a str,
    },
    /// Agent ⇒ daemon: Set the title of a window.  Called MSG_WMNAME in C.
    SetTitle(&'a str),
    /// Daemon ⇒ agent: Update the keymap.
    Keymap(qubes_gui::KeymapNotify),
    /// Agent ⇒ daemon: Dock a window
    Dock,
    /// Agent ⇒ daemon: Set window manager hints.
    WindowHints(qubes_gui::WindowHints),
    /// Bidirectional: Set window manager flags.
    WindowFlags(qubes_gui::WindowFlags),
    /// Agent ⇒ daemon: Set window class.
    WindowClass(qubes_gui::WMClass),
    /// Agent ⇒ daemon: Send shared memory dump.
    WindowDump(qubes_gui::WindowDumpHeader),
    /// Agent ⇒ daemon: Set cursor type.
    Cursor(qubes_gui::Cursor),
}

impl<'a> Event<'a> {
    /// Parse a Qubes OS GUI message from the GUI daemon
    ///
    /// # Panics
    ///
    /// Will panic if the length of the message does not match the length in the
    /// header.
    ///
    /// # Return
    ///
    /// Returns `Ok(Some(window, event))` on success.  Returns `Ok(None)` if
    /// the message is one that should only be sent by an agent.
    ///
    /// # Errors
    ///
    /// Fails if the given GUI message cannot be parsed.
    pub fn parse(
        header: qubes_gui::Header,
        body: &'a [u8],
    ) -> Result<Option<(qubes_gui::WindowID, Self)>, Error> {
        use qubes_gui::Msg;
        assert_eq!(header.len(), body.len(), "Wrong body length provided!");
        let window = header.untrusted_window();
        let ty = header
            .ty()
            .try_into()
            .expect("validated by Header::validate_length()");
        let res = match ty {
            Msg::Motion => Event::Motion(Castable::from_bytes(body)),
            Msg::Crossing => Event::Crossing(Castable::from_bytes(body)),
            Msg::Close => Event::Close,
            Msg::Keypress => Event::Keypress(Castable::from_bytes(body)),
            Msg::Button => Event::Button(Castable::from_bytes(body)),
            Msg::ClipboardReq => Event::ClipboardReq,
            Msg::ClipboardData => {
                let untrusted_data = core::str::from_utf8(body).map_err(Error::BadUTF8)?;
                Event::ClipboardData { untrusted_data }
            }
            Msg::KeymapNotify => Event::Keymap(Castable::from_bytes(body)),
            Msg::Map => Event::Redraw(Castable::from_bytes(body)),
            Msg::Unmap => Event::Configure(Castable::from_bytes(body)),
            Msg::Focus => Event::Focus(Castable::from_bytes(body)),
            Msg::WindowFlags => Event::WindowFlags(Castable::from_bytes(body)),
            Msg::Destroy => Event::Destroy,
            // Agent ⇒ daemon messages
            Msg::Resize
            | Msg::Create
            | Msg::Configure
            | Msg::MfnDump
            | Msg::ShmImage
            | Msg::Execute
            | Msg::SetTitle
            | Msg::Dock
            | Msg::WindowHints
            | Msg::WindowClass
            | Msg::WindowDump
            | Msg::Cursor => return Ok(None),
            _ => return Ok(None),
        };
        Ok(Some((window, res)))
    }
}
