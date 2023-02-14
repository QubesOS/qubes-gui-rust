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
#![forbid(unconditional_recursion)]
#![forbid(clippy::all)]

pub use buffer::Buffer;
use qubes_castable::Castable as _;
pub use qubes_gui;
use std::convert::TryInto;
use std::io;
use std::task::Poll;

mod buffer;

/// The entry-point to the library.
#[derive(Debug)]
pub struct Connection {
    raw: buffer::RawMessageStream<Option<vchan::Vchan>>,
}

impl Connection {
    /// Send a GUI message.  This never blocks; outgoing messages are queued
    /// until there is space in the vchan.
    pub fn send<T: qubes_gui::Message>(
        &mut self,
        message: &T,
        window: qubes_gui::WindowID,
    ) -> io::Result<()> {
        self.send_raw(message.as_bytes(), window, T::KIND as _)
    }

    /// Raw version of [`Connection::send`].  Using [`Connection::send`] is preferred
    /// where possible, as it automatically selects the correct message type.
    pub fn send_raw(
        &mut self,
        message: &[u8],
        window: qubes_gui::WindowID,
        ty: u32,
    ) -> io::Result<()> {
        let untrusted_len = message
            .len()
            .try_into()
            .expect("Message length must fit in a u32");
        let header = qubes_gui::UntrustedHeader {
            ty,
            window,
            untrusted_len,
        };
        header
            .validate_length()
            .unwrap()
            .expect("Sending unknown message!");
        // FIXME this is slow
        self.raw.write(header.as_bytes())?;
        self.raw.write(message)?;
        Ok(())
    }

    /// Even rawer version of [`Connection::send`].  Using [`Connection::send`] is
    /// preferred where possible, as it automatically selects the correct
    /// message type.  Otherwise, prefer [`Connection::send_raw`], which at least
    /// ensures correct framing.
    pub fn send_raw_bytes(&mut self, msg: &[u8]) -> io::Result<()> {
        self.raw.write(msg).map_err(From::from)
    }

    /// Acknowledge an event (as reported by poll(2), epoll(2), or similar).
    /// Must be called before performing any I/O.
    pub fn wait(&mut self) {
        self.raw.wait()
    }

    /// If a complete message has been buffered, returns `Ok(Some(msg))`.  If
    /// more data needs to arrive, returns `Ok(None)`.  If an error occurs,
    /// `Err` is returned, and the stream is placed in an error state.  If the
    /// stream is in an error state, all further functions will fail.
    pub fn read_message(&mut self) -> Poll<io::Result<Buffer<'_>>> {
        match self.raw.read_message() {
            Ok(None) => Poll::Pending,
            Ok(Some(v)) => Poll::Ready(Ok(v)),
            Err(e) => Poll::Ready(Err(e)),
        }
    }

    /// Creates a daemon instance
    pub fn daemon(domain: u16, xconf: qubes_gui::XConf) -> io::Result<Self> {
        Ok(Self {
            raw: buffer::RawMessageStream::daemon(domain, xconf)?,
        })
    }

    /// Creates an agent instance
    pub fn agent(domain: u16) -> io::Result<Self> {
        Ok(Self {
            raw: buffer::RawMessageStream::agent(domain)?,
        })
    }

    /// Try to reconnect.  If this fails, the agent is no longer usable; future
    /// operations may panic.
    pub fn reconnect(&mut self) -> io::Result<()> {
        self.raw.reconnect().map_err(From::from)
    }

    /// Gets and clears the “did_reconnect” flag
    pub fn reconnected(&mut self) -> bool {
        self.raw.reconnected()
    }

    /// Returns true if a reconnection is needed.
    pub fn needs_reconnect(&self) -> bool {
        self.raw.needs_reconnect()
    }
}

impl std::os::unix::io::AsRawFd for Connection {
    fn as_raw_fd(&self) -> std::os::raw::c_int {
        self.raw.as_raw_fd()
    }
}
