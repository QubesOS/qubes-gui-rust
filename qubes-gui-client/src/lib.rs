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
use std::io;
use std::num::NonZeroU32;
use std::task::Poll;

pub mod agent;

mod buffer;

/// The entry-point to the library.
#[derive(Debug)]
pub struct Client {
    vchan: buffer::Vchan,
}

impl Client {
    /// Send a GUI message.  This never blocks; outgoing messages are queued
    /// until there is space in the vchan.
    pub fn send<T: qubes_gui::Message>(
        &mut self,
        message: &T,
        window: NonZeroU32,
    ) -> io::Result<()> {
        let header = qubes_gui::Header {
            ty: T::kind(),
            window: window.into(),
            untrusted_len: std::mem::size_of_val(message) as u32,
        };
        // FIXME this is slow
        self.vchan.write(header.as_bytes())?;
        self.vchan.write(message.as_bytes())?;
        Ok(())
    }

    /// Window dump

    /// If a message header is read successfully, `Poll::Ready(Ok(r))` is returned, and
    /// `r` can be used to access the message body.  If there is not enough data, `Poll::Pending`
    /// is returned.  `Poll::Ready(Err(_))` is returned if an error occurs.
    pub fn read_header<'a>(&'a mut self) -> Poll<io::Result<(qubes_gui::Header, &'a [u8])>> {
        match self.vchan.read_header() {
            Ok(None) => Poll::Pending,
            Ok(Some((header, buffer))) => Poll::Ready(Ok((header, buffer))),
            Err(e) => Poll::Ready(Err(e)),
        }
    }

    /// Creates an daemon instance
    pub fn daemon(domain: u16) -> io::Result<Self> {
        let vchan = buffer::Vchan::daemon(domain)?;
        Ok(Self { vchan })
    }

    /// Creates a agent instance
    pub fn agent(domain: u16) -> io::Result<(Self, qubes_gui::XConf)> {
        let (vchan, conf) = buffer::Vchan::agent(domain)?;
        Ok((Self { vchan }, conf))
    }

    /// Gets the raw file descriptor
    pub fn as_raw_fd(&self) -> std::os::raw::c_int {
        self.vchan.as_raw_fd()
    }
}
