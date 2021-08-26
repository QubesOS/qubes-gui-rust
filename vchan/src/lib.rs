/*
 * The Qubes OS Project, https://www.qubes-os.org
 *
 * Copyright (C) 2010  Rafal Wojtczuk  <rafal@invisiblethingslab.com>
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
use std::io::{Error, Read, Write};
use std::os::{raw::c_int, unix::prelude::RawFd};

/// Status of the channel
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum Status {
    /// Remote disconnected or remote domain dead
    Disconnected,
    /// Connected
    Connected,
    /// Server initialized, waiting for client to connect
    Waiting,
}

/// A wrapper around a Qubes vchan, which is a stream-oriented, inter-qube
/// communication channel.  This implementation uses the libvchan C library.
///
/// The `Read` implementation of [`Vchan`] does not read from the slice passed
/// to it, and is safe to call even if that slice is uninitialized memory.
#[derive(Debug)]
pub struct Vchan {
    inner: *mut vchan_sys::libvchan_t,
}

impl Vchan {
    /// Creates a listening vchan that listens from requests from the given domain
    /// on the given port.
    #[inline]
    pub fn server<T>(
        domain: T,
        port: c_int,
        read_min: usize,
        write_min: usize,
    ) -> Result<Self, Error>
    where
        u16: From<T>,
    {
        Self::server_inner(domain.into(), port, read_min, write_min)
    }

    fn server_inner(
        domain: u16,
        port: c_int,
        read_min: usize,
        write_min: usize,
    ) -> Result<Self, Error> {
        let ptr =
            unsafe { vchan_sys::libvchan_server_init(domain.into(), port, read_min, write_min) };
        if ptr.is_null() {
            Err(Error::last_os_error())
        } else {
            Ok(Self { inner: ptr })
        }
    }

    /// Creates a vchan that will connect to the given domain via the given port.
    #[inline]
    pub fn client<T>(domain: T, port: c_int) -> Result<Self, Error>
    where
        u16: From<T>,
    {
        Self::client_inner(domain.into(), port)
    }

    fn client_inner(domain: u16, port: c_int) -> Result<Self, Error> {
        let ptr = unsafe { vchan_sys::libvchan_client_init(domain.into(), port) };
        if ptr.is_null() {
            Err(Error::last_os_error())
        } else {
            Ok(Self { inner: ptr })
        }
    }

    /// Returns the underlying file descriptor.  The only valid use of this descriptor
    /// is to call `poll` or similar.
    pub fn fd(&self) -> RawFd {
        unsafe { vchan_sys::libvchan_fd_for_select(self.inner) }
    }

    /// Returns the status of this channel.
    pub fn status(&self) -> Status {
        match unsafe { vchan_sys::libvchan_is_open(self.inner) } {
            vchan_sys::VCHAN_DISCONNECTED => Status::Disconnected,
            vchan_sys::VCHAN_CONNECTED => Status::Connected,
            vchan_sys::VCHAN_WAITING => Status::Waiting,
            _ => panic!("bad return value from libvchan_is_open()"),
        }
    }

    /// Returns the amount of data that is ready, and thus can be read without
    /// blocking.
    pub fn data_ready(&self) -> usize {
        let s = unsafe { vchan_sys::libvchan_data_ready(self.inner) };
        assert!(s >= 0, "Number of bytes ready cannot be negative!");
        s as _
    }

    /// Returns the amount of data that can be written without blocking.
    pub fn buffer_space(&self) -> usize {
        let s = unsafe { vchan_sys::libvchan_buffer_space(self.inner) };
        assert!(
            s >= 0,
            "Number of bytes that can be sent cannot be negative!"
        );
        s as _
    }

    /// Wait for I/O in some direction to be possible.  This function is
    /// blocking, unless an event has happened on the file descriptor, in which
    /// case it does not block and clears the event pending flag.
    pub fn wait(&self) {
        unsafe { vchan_sys::libvchan_wait(self.inner) };
    }

    /// Block until the given buffer is full
    pub fn recv(&mut self, buffer: &mut [u8]) -> Result<usize, Error> {
        let res =
            unsafe { vchan_sys::libvchan_recv(self.inner, buffer.as_mut_ptr() as _, buffer.len()) };
        if res == -1 {
            Err(Error::last_os_error())
        } else {
            Ok(res as _)
        }
    }
}

impl Write for Vchan {
    fn write(&mut self, buffer: &[u8]) -> Result<usize, Error> {
        let res =
            unsafe { vchan_sys::libvchan_write(self.inner, buffer.as_ptr() as _, buffer.len()) };
        if res == -1 {
            Err(Error::last_os_error())
        } else {
            Ok(res as _)
        }
    }

    fn flush(&mut self) -> Result<(), Error> {
        Ok(())
    }
}

impl Read for Vchan {
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize, Error> {
        let res =
            unsafe { vchan_sys::libvchan_read(self.inner, buffer.as_mut_ptr() as _, buffer.len()) };
        if res == -1 {
            Err(Error::last_os_error())
        } else {
            Ok(res as _)
        }
    }
}

impl Drop for Vchan {
    fn drop(&mut self) {
        unsafe { vchan_sys::libvchan_close(self.inner) }
    }
}
