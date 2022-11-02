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
#![forbid(clippy::all, improper_ctypes, improper_ctypes_definitions)]

use std::io::{ErrorKind, Read, Write};
use std::os::{raw::c_int, raw::c_void, unix::prelude::RawFd};

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

/// Error on a vchan
#[derive(Debug, Clone)]
pub enum Error {
    /// Failure allocating memory
    OutOfMemory(std::collections::TryReserveError),
    /// Vchan read error
    Read,
    /// Vchan write error
    Write,
    /// Cannot listen
    CannotListen,
    /// Cannot connect
    CannotConnect,
}

impl From<Error> for std::io::Error {
    fn from(t: Error) -> Self {
        Self::new(ErrorKind::Other, format!("{}", t))
    }
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::Read => write!(f, "Error during vchan read"),
            Error::Write => write!(f, "Error during vchan write"),
            Error::CannotListen => write!(f, "Cannot listen on vchan"),
            Error::CannotConnect => write!(f, "Cannot connect to vchan"),
            Error::OutOfMemory(e) => write!(f, "{}", e),
        }
    }
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

fn c_int_to_usize(i: c_int) -> usize {
    assert!(i >= 0, "c_int_to_usize passed negative number");
    // If u32 doesn’t actually fit in a usize, fail the build
    const _: () = assert!(c_int::MAX as usize as c_int == c_int::MAX);
    i as usize
}

impl Vchan {
    /// Creates a listening vchan that listens from requests from the given domain
    /// on the given port.
    #[inline]
    pub fn server(
        domain: impl Into<u16>,
        port: c_int,
        read_min: usize,
        write_min: usize,
    ) -> Result<Self, Error> {
        fn server_inner(
            domain: u16,
            port: c_int,
            read_min: usize,
            write_min: usize,
        ) -> Result<Vchan, Error> {
            let ptr = unsafe {
                vchan_sys::libvchan_server_init(domain.into(), port, read_min, write_min)
            };
            if ptr.is_null() {
                Err(Error::CannotListen)
            } else {
                Ok(Vchan { inner: ptr })
            }
        }
        server_inner(domain.into(), port, read_min, write_min)
    }

    /// Creates a vchan that will connect to the given domain via the given port.
    #[inline]
    pub fn client(domain: impl Into<u16>, port: c_int) -> Result<Self, Error> {
        fn client_inner(domain: u16, port: c_int) -> Result<Vchan, Error> {
            let ptr = unsafe { vchan_sys::libvchan_client_init(domain.into(), port) };
            if ptr.is_null() {
                Err(Error::CannotConnect)
            } else {
                Ok(Vchan { inner: ptr })
            }
        }
        client_inner(domain.into(), port)
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
        assert!(s >= 0, "Number of bytes ready to read cannot be negative!");
        c_int_to_usize(s)
    }

    /// Returns the amount of data that can be written without blocking.
    pub fn buffer_space(&self) -> usize {
        let s = unsafe { vchan_sys::libvchan_buffer_space(self.inner) };
        assert!(
            s >= 0,
            "Number of bytes that can be sent cannot be negative!"
        );
        c_int_to_usize(s)
    }

    /// Wait for I/O in some direction to be possible.  This function is
    /// blocking, unless an event has happened on the file descriptor, in which
    /// case it does not block and clears the event pending flag.
    pub fn wait(&self) {
        unsafe { vchan_sys::libvchan_wait(self.inner) };
    }

    /// Write the entire buffer
    pub fn send(&self, buffer: &[u8]) -> Result<(), Error> {
        assert!(
            buffer.len() <= c_int::MAX as usize,
            "sending {} bytes but INT_MAX is {}",
            buffer.len(),
            c_int::MAX
        );
        let res =
            unsafe { vchan_sys::libvchan_send(self.inner, buffer.as_ptr() as _, buffer.len()) };
        if res == -1 {
            Err(Error::Write)
        } else {
            assert!(res >= 0, "sent negative number of bytes?");
            assert_eq!(res as usize, buffer.len(), "libvchan_send short write?");
            Ok(())
        }
    }

    /// Block until the given buffer is full
    ///
    /// # Safety
    ///
    /// It must be permissable to write to the half-open range
    /// [ptr, ptr.add_size()).  It is _not_ necessary that the memory in this
    /// range be initialized, as this function will never read from it.  If this
    /// function returns successfully, the memory in the range *will* be
    /// initialized.
    unsafe fn unsafe_recv(&self, ptr: *mut c_void, size: usize) -> Result<(), Error> {
        // SAFETY: by the function's precondition, ptr can validly have size
        // bytes written to it.  By Rust's type safety, self.inner is a valid
        // vchan.
        let res = vchan_sys::libvchan_recv(self.inner, ptr, size);
        if res == -1 {
            Err(Error::Read)
        } else {
            assert!(res >= 0, "received negative number of bytes?");
            assert_eq!(res as usize, size, "libvchan_recv short read?");
            Ok(())
        }
    }

    /// Block until the given buffer is full
    pub fn recv(&self, buffer: &mut [u8]) -> Result<(), Error> {
        // SAFETY: buffer.as_mut_ptr() is a valid pointer to
        // buffer.len() bytes of memory
        unsafe { self.unsafe_recv(buffer.as_mut_ptr() as _, buffer.len()) }
    }

    /// Discard data from the vchan.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from the vchan failed.
    pub fn discard(&self, mut bytes: usize) -> Result<(), Error> {
        use std::mem::{size_of, MaybeUninit};
        let mut buf: MaybeUninit<[u8; 256]> = MaybeUninit::uninit();
        let _: [u8; size_of::<MaybeUninit<[u8; 256]>>()] = [0u8; 256];
        while bytes > 0 {
            let to_read = 256.min(bytes);
            unsafe { self.unsafe_recv(&mut buf as *mut _ as *mut _, to_read)? }
            bytes -= to_read;
        }
        Ok(())
    }

    /// Extend the vector with data from the vchan.
    ///
    /// # Errors
    ///
    /// Returns an error if the capacity overflows, allocating more memory for
    /// the buffer fails, or there is an error reading from the vchan.
    pub fn recv_into(&self, buffer: &mut Vec<u8>, bytes: usize) -> Result<(), Error> {
        buffer.try_reserve(bytes).map_err(Error::OutOfMemory)?;
        let buffer_len = buffer.len();
        // SAFETY: the unused bytes part of a vector can safely be written to,
        // if no Drop impls need to be called.  The necessary capacity was reserved above.
        unsafe { self.unsafe_recv(buffer.as_mut_ptr().add(buffer_len) as _, bytes)? }
        // SAFETY: the above code will fill the whole buffer on success
        unsafe { buffer.set_len(buffer_len + bytes) }
        Ok(())
    }

    /// Receive any [`qubes_castable::Castable`] struct.  Blocks until the read is complete.
    #[cfg(feature = "castable")]
    pub fn recv_struct<T: qubes_castable::Castable>(&self) -> Result<T, Error> {
        let mut datum = std::mem::MaybeUninit::<T>::uninit();
        // SAFETY: status.as_mut_ptr() is a valid pointer to size_of::<T>()
        // bytes of memory, and unsafe_recv() is okay with uninitialized memory.
        unsafe { self.unsafe_recv(datum.as_mut_ptr() as *mut _, std::mem::size_of::<T>()) }?;
        // SAFETY: libvchan_recv fully initialized the buffer, and a
        // Castable struct can have any byte pattern.
        unsafe { Ok(datum.assume_init()) }
    }
}

impl Write for Vchan {
    fn write(&mut self, buffer: &[u8]) -> Result<usize, std::io::Error> {
        let res =
            unsafe { vchan_sys::libvchan_write(self.inner, buffer.as_ptr() as _, buffer.len()) };
        if res == -1 {
            Err(std::io::Error::new(ErrorKind::Other, "vchan write error"))
        } else {
            assert!(res >= 0, "wrote negative number of bytes?");
            Ok(res as _)
        }
    }

    fn flush(&mut self) -> Result<(), std::io::Error> {
        Ok(())
    }
}

impl Read for Vchan {
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize, std::io::Error> {
        let res =
            unsafe { vchan_sys::libvchan_read(self.inner, buffer.as_mut_ptr() as _, buffer.len()) };
        if res == -1 {
            Err(std::io::Error::new(ErrorKind::Other, "vchan read error"))
        } else {
            assert!(res >= 0, "read negative number of bytes?");
            Ok(res as _)
        }
    }
}

impl Drop for Vchan {
    fn drop(&mut self) {
        unsafe { vchan_sys::libvchan_close(self.inner) }
    }
}
