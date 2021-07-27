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
//! A client for the Qubes OS GUI protocol.  This client is low-level.

#![forbid(missing_docs)]

use qubes_castable::Castable as _;
pub use qubes_gui;
use qubes_gui::NonClipboardMessage;
use std::cell::RefCell;
use std::io::Result;
use std::num::NonZeroU32;

mod buffer;

/// The entry-point to the library.
#[derive(Debug)]
pub struct Client {
    vchan: RefCell<buffer::Vchan>,
}

impl Client {
    /// Send a GUI message.  This never blocks; outgoing messages are queued
    /// until there is space in the vchan.
    pub fn send<T: qubes_gui::Message>(&self, message: &T, window: NonZeroU32) -> Result<()> {
        let header = qubes_gui::Header {
            ty: T::kind(),
            window: window.into(),
            untrusted_len: std::mem::size_of_val(message) as u32,
        };
        let mut vchan = self.vchan.borrow_mut();
        // FIXME this is slow
        vchan.write(header.as_bytes())?;
        vchan.write(message.as_bytes())?;
        Ok(())
    }

    /// If there is nothing to read, return `Ok(None)` immediately; otherwise,
    /// block until a message header has been read or an error (such as EOF)
    /// occurs.  If a message header is read successfully, `Ok(Some(r))` is
    /// returned, and `r` can be used to access the message body.  Otherwise,
    /// `Err` is returned.
    pub fn read_header<'a>(&'a mut self) -> Result<Option<(qubes_gui::Header, Vec<u8>)>> {
        self.vchan
            .borrow_mut()
            .read_header()
            .map(|s| s.map(|(a, b)| (a, b.to_owned())))
    }
}

/// Used to obtain the request body after a call to [`Client::read_header`].
pub struct Reader<'a> {
    client: &'a mut Client,
    header: qubes_gui::Header,
}

impl<'a> Reader<'a> {
    /// Returns the header that was read
    pub fn header(&self) -> qubes_gui::Header {
        self.header
    }

    /// Returns the type of message that was read
    pub fn ty(&self) -> u32 {
        self.header.ty
    }
}
