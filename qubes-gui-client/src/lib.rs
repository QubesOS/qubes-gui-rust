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

use qubes_castable::Castable as _;
pub use qubes_gui;
use std::collections::BTreeSet;
use std::convert::TryInto;
use std::io;
use std::num::NonZeroU32;
use std::task::Poll;

mod buffer;

/// The entry-point to the library.
#[derive(Debug)]
pub struct Client {
    raw: buffer::RawMessageStream<Option<vchan::Vchan>>,
    present_windows: BTreeSet<NonZeroU32>,
    agent: bool,
}

impl Client {
    /// Send a GUI message.  This never blocks; outgoing messages are queued
    /// until there is space in the vchan.
    pub fn send<T: qubes_gui::Message>(
        &mut self,
        message: &T,
        window: NonZeroU32,
    ) -> io::Result<()> {
        self.send_raw(message.as_bytes(), window, T::KIND as _)
    }

    /// Raw version of [`Client::send`].  Using [`Client::send`] is preferred
    /// where possible, as it automatically selects the correct message type.
    pub fn send_raw(&mut self, message: &[u8], window: NonZeroU32, ty: u32) -> io::Result<()> {
        let untrusted_len = message
            .len()
            .try_into()
            .expect("Message length must fit in a u32");
        let header = qubes_gui::Header {
            ty,
            window: window.into(),
            untrusted_len,
        };
        if self.agent {
            if header.ty == qubes_gui::Msg::Create as _ {
                assert!(
                    self.present_windows.insert(window),
                    "Creating window {} already in map!",
                    window
                );
            } else if header.ty == qubes_gui::Msg::Destroy as _ {
                assert!(
                    self.present_windows.remove(&window),
                    "Trying to delete window {} not in map!",
                    window
                );
            } else {
                assert!(
                    self.present_windows.contains(&window),
                    "Sending message on nonexistant window {}!",
                    window
                )
            }
        }
        // FIXME this is slow
        self.raw.write(header.as_bytes())?;
        self.raw.write(message)?;
        Ok(())
    }

    /// Even rawer version of [`Client::send`].  Using [`Client::send`] is
    /// preferred where possible, as it automatically selects the correct
    /// message type.  Otherwise, prefer [`Client::send_raw`], which at least
    /// ensures correct framing.
    pub fn send_raw_bytes(&mut self, msg: &[u8]) -> io::Result<()> {
        self.raw.write(msg)
    }

    /// Acknowledge an event (as reported by poll(2), epoll(2), or similar).
    /// Must be called before performing any I/O.
    pub fn wait(&mut self) {
        self.raw.wait()
    }

    /// If a message header is read successfully, `Poll::Ready(Ok(r))` is returned, and
    /// `r` can be used to access the message body.  If there is not enough data, `Poll::Pending`
    /// is returned.  `Poll::Ready(Err(_))` is returned if an error occurs.
    pub fn read_header(&mut self) -> Poll<io::Result<(qubes_gui::Header, &[u8])>> {
        match self.raw.read_header() {
            Ok(None) => Poll::Pending,
            Ok(Some((header, buffer))) => Poll::Ready(Ok((header, buffer))),
            Err(e) => Poll::Ready(Err(e)),
        }
    }

    /// Creates a daemon instance
    pub fn daemon(domain: u16, xconf: qubes_gui::XConfVersion) -> io::Result<Self> {
        Ok(Self {
            raw: buffer::RawMessageStream::daemon(domain, xconf)?,
            present_windows: Default::default(),
            agent: false,
        })
    }

    /// Creates an agent instance
    pub fn agent(domain: u16) -> io::Result<(Self, qubes_gui::XConfVersion)> {
        let (raw, conf) = buffer::RawMessageStream::agent(domain)?;
        let s = Self {
            raw,
            present_windows: Default::default(),
            agent: true,
        };
        Ok((s, conf))
    }

    /// Try to reconnect.  If this fails, the agent is no longer usable; future
    /// operations may panic.
    pub fn reconnect(&mut self) -> io::Result<()> {
        self.raw.reconnect()
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

impl std::os::unix::io::AsRawFd for Client {
    fn as_raw_fd(&self) -> std::os::raw::c_int {
        self.raw.as_raw_fd()
    }
}
