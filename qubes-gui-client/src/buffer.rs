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
//! A wrapper around vchans that provides a write buffer.  Used to prevent
//! deadlocks.

use qubes_castable::Castable;
use qubes_gui::{Header, UntrustedHeader};
use std::collections::VecDeque;
use std::io::{self, Error, ErrorKind};
use std::mem::size_of;

/// Protocol state
#[derive(Debug)]
enum ReadState {
    /// Currently connecting
    Connecting,
    /// Reading X11 configuration
    ReadingXConf,
    /// Reading a message header
    ReadingHeader,
    /// Reading a message body
    ReadingBody { header: Header },
    /// Discarding data from an unknown message
    Discard(usize),
    /// Something went wrong.  Terminal state.
    Error,
}

// Trait for a vchan, for unit-testing
pub(crate) trait VchanMock
where
    Self: Sized,
{
    fn buffer_space(&self) -> usize;
    fn recv_into(&self, buf: &mut Vec<u8>, bytes: usize) -> io::Result<()>;
    fn recv_struct<T: Castable + Default>(&self) -> io::Result<T>;
    fn send(&self, buf: &[u8]) -> io::Result<()>;
    fn wait(&self);
    fn data_ready(&self) -> usize;
    fn status(&self) -> vchan::Status;
    fn discard(&self, bytes: usize) -> io::Result<()>;
}

impl VchanMock for Option<vchan::Vchan> {
    fn discard(&self, bytes: usize) -> io::Result<()> {
        vchan::Vchan::discard(self.as_ref().unwrap(), bytes)
    }
    fn buffer_space(&self) -> usize {
        vchan::Vchan::buffer_space(self.as_ref().unwrap())
    }
    fn recv_into(&self, buf: &mut Vec<u8>, bytes: usize) -> io::Result<()> {
        vchan::Vchan::recv_into(self.as_ref().unwrap(), buf, bytes)
    }
    fn recv_struct<T: Castable>(&self) -> io::Result<T> {
        vchan::Vchan::recv_struct(self.as_ref().unwrap())
    }
    fn send(&self, buf: &[u8]) -> io::Result<()> {
        vchan::Vchan::send(self.as_ref().unwrap(), buf)
    }
    fn wait(&self) {
        vchan::Vchan::wait(self.as_ref().unwrap())
    }
    fn data_ready(&self) -> usize {
        vchan::Vchan::data_ready(self.as_ref().unwrap())
    }
    fn status(&self) -> vchan::Status {
        self.as_ref()
            .map(vchan::Vchan::status)
            .unwrap_or(vchan::Status::Disconnected)
    }
}

#[derive(Debug)]
pub(crate) struct RawMessageStream<T: VchanMock> {
    /// Vchan
    vchan: T,
    /// Write buffer
    queue: VecDeque<u8>,
    /// State of the read state machine
    state: ReadState,
    /// Read buffer
    buffer: Vec<u8>,
    /// Was reconnect successful?
    did_reconnect: bool,
    /// Configuration from the daemon
    xconf: qubes_gui::XConfVersion,
    /// Peer domain ID
    domid: u16,
}

#[inline(always)]
fn u32_to_usize(i: u32) -> usize {
    // If u32 doesn’t actually fit in a usize, fail the build
    let [] = [0; if u32::MAX as usize as u32 == u32::MAX {
        0
    } else {
        1
    }];
    i as usize
}

impl<T: VchanMock> RawMessageStream<T> {
    /// Attempts to write as much of `slice` as possible to the `vchan`.  Never
    /// blocks.  Returns the number of bytes written.
    ///
    /// # Errors
    ///
    /// Fails if writing to the vchan fails.
    fn write_slice(vchan: &mut T, slice: &[u8]) -> io::Result<usize> {
        let space = vchan.buffer_space();
        if space == 0 {
            Ok(0)
        } else {
            let to_write = space.min(slice.len());
            vchan.send(&slice[..to_write])?;
            Ok(to_write)
        }
    }

    /// Write as much of the buffered data as possible without blocking.
    /// Returns the number of bytes successfully written.
    fn flush_pending_writes(&mut self) -> io::Result<usize> {
        let mut written = 0;
        loop {
            let (front, back) = self.queue.as_slices();
            let to_write = if front.is_empty() {
                if back.is_empty() {
                    break Ok(written);
                }
                back
            } else {
                front
            };
            let written_this_time = Self::write_slice(&mut self.vchan, to_write)?;
            if written_this_time == 0 {
                break Ok(written);
            }
            written += written_this_time;
            for _ in 0..written_this_time {
                let _ = self.queue.pop_front();
            }
        }
    }

    /// Write as much of the buffered data to the vchan as possible.  Queue the
    /// rest in an internal buffer.
    ///
    /// # Errors
    ///
    /// Fails if there is an I/O error on the vchan.
    pub fn write(&mut self, buf: &[u8]) -> io::Result<()> {
        self.flush_pending_writes()?;
        if !self.queue.is_empty() {
            self.queue.extend(buf);
            return Ok(());
        }
        let written = Self::write_slice(&mut self.vchan, buf)?;
        if written != buf.len() {
            assert!(written < buf.len());
            self.queue.extend(&buf[written..]);
        }
        Ok(())
    }

    #[inline]
    fn recv_into(&mut self, bytes: usize) -> io::Result<()> {
        self.vchan.recv_into(&mut self.buffer, bytes).map_err(|e| {
            self.state = ReadState::Error;
            e
        })
    }

    #[inline]
    fn recv_struct<U: Castable + Default>(&mut self) -> io::Result<U> {
        self.vchan.recv_struct().map_err(|e| {
            self.state = ReadState::Error;
            e
        })
    }

    /// Acknowledge an event on the vchan.
    pub fn wait(&mut self) {
        self.vchan.wait()
    }

    /// Check for a reconnection, consuming the pending reconnection state.
    pub fn reconnected(&mut self) -> bool {
        std::mem::replace(&mut self.did_reconnect, false)
    }

    fn fail(&mut self, kind: ErrorKind, msg: &str) -> io::Result<Option<(Header, &[u8])>> {
        self.state = ReadState::Error;
        Err(Error::new(kind, msg))
    }

    /// If a complete message has been buffered, returns `Ok(Some(msg))`.  If
    /// more data needs to arrive, returns `Ok(None)`.  If an error occurs,
    /// `Err` is returned, and the stream is placed in an error state.  If the
    /// stream is in an error state, all further functions will fail.
    pub fn read_message<'a, 'b: 'a>(&'b mut self) -> io::Result<Option<(Header, &'a [u8])>> {
        const SIZE_OF_XCONF: usize = size_of::<qubes_gui::XConfVersion>();
        if let Err(e) = self.flush_pending_writes() {
            self.state = ReadState::Error;
            return Err(e);
        }
        let process_so_far = |s: &'a mut Self, header: Header, ready: usize| {
            let to_read = header.len() - s.buffer.len();
            s.recv_into(to_read.min(ready))?;
            if ready >= to_read {
                s.state = ReadState::ReadingHeader;
                Ok(Some((header, &s.buffer[..])))
            } else {
                s.state = ReadState::ReadingBody { header };
                Ok(None)
            }
        };
        let set_discard = |s: usize| match s {
            0 => ReadState::ReadingHeader,
            s => ReadState::Discard(s),
        };
        loop {
            let ready = self.vchan.data_ready();
            match self.state {
                ReadState::Connecting => match self.vchan.status() {
                    vchan::Status::Waiting => return Ok(None),
                    vchan::Status::Connected => {
                        self.state = ReadState::ReadingXConf;
                    }
                    vchan::Status::Disconnected => {
                        break self.fail(ErrorKind::Other, "vchan connection refused");
                    }
                },
                ReadState::Error => break self.fail(ErrorKind::Other, "Already in error state"),
                ReadState::ReadingXConf if ready >= SIZE_OF_XCONF => {
                    self.xconf = self.recv_struct()?;
                    self.state = ReadState::ReadingHeader;
                }
                ReadState::ReadingHeader if ready >= size_of::<Header>() => {
                    // Reset buffer to 0 bytes
                    self.buffer.clear();
                    let header: UntrustedHeader = self.recv_struct()?;
                    match header.validate_length() {
                        Err(e) => {
                            // bad length, bail out
                            self.state = ReadState::Error;
                            break Err(Error::new(ErrorKind::InvalidData, format!("{}", e)));
                        }
                        Ok(Some(header)) => {
                            break process_so_far(self, header, ready - size_of::<Header>());
                        }
                        Ok(None) => self.state = set_discard(u32_to_usize(header.untrusted_len)),
                    }
                }
                ReadState::ReadingHeader | ReadState::ReadingXConf => break Ok(None),
                ReadState::Discard(untrusted_len) => {
                    let untrusted_to_discard = ready.min(untrusted_len);
                    match self.vchan.discard(untrusted_to_discard) {
                        Err(e) => {
                            self.state = ReadState::Error;
                            break Err(e);
                        }
                        Ok(()) => self.state = set_discard(untrusted_len - untrusted_to_discard),
                    }
                }
                ReadState::ReadingBody { header } => {
                    break process_so_far(self, header, ready);
                }
            }
        }
    }

    pub fn needs_reconnect(&self) -> bool {
        self.vchan.status() == vchan::Status::Disconnected
    }
}

impl RawMessageStream<Option<vchan::Vchan>> {
    pub fn agent(domain: u16) -> io::Result<(Self, qubes_gui::XConfVersion)> {
        let vchan = vchan::Vchan::server(domain, qubes_gui::LISTENING_PORT.into(), 4096, 4096)?;
        loop {
            match vchan.status() {
                vchan::Status::Waiting => vchan.wait(),
                vchan::Status::Connected => break,
                vchan::Status::Disconnected => {
                    return Err(Error::new(
                        ErrorKind::Other,
                        "Didn’t get a connection from the GUI daemon",
                    ))
                }
            }
        }
        let mut res = Self {
            vchan: Some(vchan),
            queue: Default::default(),
            state: ReadState::ReadingHeader,
            buffer: vec![],
            did_reconnect: false,
            domid: domain,
            xconf: Default::default(),
        };
        res.write(qubes_gui::PROTOCOL_VERSION.as_bytes())?;
        res.flush_pending_writes()?;
        res.vchan
            .as_mut()
            .expect("Set to Some above; qed")
            .recv(res.xconf.as_mut_bytes())?;
        let xconf = res.xconf;
        Ok((res, xconf))
    }

    pub fn daemon(domain: u16, xconf: qubes_gui::XConfVersion) -> io::Result<Self> {
        Ok(Self {
            vchan: Some(vchan::Vchan::client(
                domain,
                qubes_gui::LISTENING_PORT.into(),
            )?),
            queue: Default::default(),
            state: ReadState::ReadingHeader,
            buffer: vec![],
            did_reconnect: false,
            domid: domain,
            xconf,
        })
    }

    pub fn reconnect(&mut self) -> io::Result<()> {
        self.vchan = None;
        self.vchan = Some(vchan::Vchan::server(
            self.domid,
            qubes_gui::LISTENING_PORT.into(),
            4096,
            4096,
        )?);
        self.queue.clear();
        self.buffer.clear();
        self.state = ReadState::Connecting;
        self.vchan.send(((1u32 << 16) | 3u32).as_bytes())?;
        Ok(())
    }

    pub fn as_raw_fd(&self) -> std::os::raw::c_int {
        self.vchan.as_ref().unwrap().fd()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    struct MockVchan {
        read_buf: Vec<u8>,
        write_buf: Vec<u8>,
        buffer_space: usize,
        data_ready: usize,
        cursor: usize,
    }

    impl VchanMock for RefCell<MockVchan> {
        fn wait(&self) {}
        fn status(&self) -> vchan::Status {
            vchan::Status::Connected
        }
        fn data_ready(&self) -> usize {
            self.borrow().data_ready
        }
        fn buffer_space(&self) -> usize {
            self.borrow().buffer_space
        }
        fn send(&self, buffer: &[u8]) -> io::Result<()> {
            let mut s = self.borrow_mut();
            assert!(
                buffer.len() <= s.buffer_space,
                "Agents never write more space than is available"
            );
            s.write_buf.extend_from_slice(buffer);
            s.buffer_space -= buffer.len();
            Ok(())
        }
        fn recv_into(&self, buffer: &mut Vec<u8>, bytes: usize) -> io::Result<()> {
            let mut s = self.borrow_mut();
            assert!(
                s.read_buf.len() >= s.data_ready && s.read_buf.len() - s.data_ready >= s.cursor,
                "mock vchan internal bounds error"
            );
            assert!(
                bytes <= s.data_ready,
                "Agents never read more data than is available"
            );
            buffer.extend_from_slice(&s.read_buf[s.cursor..s.cursor + bytes]);
            s.cursor += buffer.len();
            Ok(())
        }
        fn recv_struct<T: Castable + Default>(&self) -> io::Result<T> {
            let mut s = self.borrow_mut();
            let mut v: T = Default::default();
            assert!(
                s.read_buf.len() >= s.data_ready && s.read_buf.len() - s.data_ready >= s.cursor,
                "mock vchan internal bounds error"
            );
            let b = v.as_mut_bytes();
            assert!(
                b.len() <= s.data_ready,
                "Agents never read more data than is available"
            );
            b.copy_from_slice(&s.read_buf[s.cursor..s.cursor + b.len()]);
            s.cursor += b.len();
            Ok(v)
        }
        fn discard(&self, bytes: usize) -> io::Result<()> {
            let mut s = self.borrow_mut();
            assert!(
                s.read_buf.len() >= s.data_ready && s.read_buf.len() - s.data_ready >= s.cursor,
                "mock vchan internal bounds error"
            );
            assert!(
                bytes <= s.data_ready,
                "Agents never read more data than is available"
            );
            s.cursor += bytes;
            Ok(())
        }
    }
    #[test]
    fn vchan_writes() {
        let mock_vchan = MockVchan {
            read_buf: vec![],
            write_buf: vec![],
            buffer_space: 0,
            data_ready: 0,
            cursor: 0,
        };
        let mut under_test = RawMessageStream::<RefCell<MockVchan>> {
            vchan: RefCell::new(mock_vchan),
            queue: Default::default(),
            state: ReadState::ReadingHeader,
            buffer: vec![],
            did_reconnect: false,
            xconf: Default::default(),
            domid: 0,
        };
        under_test.write(b"test1").unwrap();
        assert_eq!(under_test.queue.len(), 5, "message queued");
        assert_eq!(under_test.queue, *b"test1");
        assert_eq!(under_test.vchan.borrow().write_buf, b"", "no bytes written");
        under_test.vchan.borrow_mut().buffer_space = 3;
        under_test
            .flush_pending_writes()
            .expect("drained successfully");
        assert_eq!(under_test.queue.len(), 2);
        assert_eq!(under_test.queue, *b"t1");
        assert_eq!(under_test.vchan.borrow().write_buf, b"tes");
        assert_eq!(under_test.vchan.borrow().buffer_space, 0);
        under_test.vchan.borrow_mut().buffer_space = 4;
        under_test.write(b"\0another alpha").unwrap();
        assert_eq!(under_test.queue.len(), 12);
        assert_eq!(under_test.vchan.borrow().write_buf, b"test1\0a");
        assert_eq!(
            under_test.queue, *b"nother alpha",
            "only the minimum number of bytes stored"
        );
        under_test.vchan.borrow_mut().buffer_space = 2;
        under_test
            .flush_pending_writes()
            .expect("drained successfully");
        assert_eq!(under_test.vchan.borrow().write_buf, b"test1\0ano");
        assert_eq!(under_test.vchan.borrow().buffer_space, 0);
        under_test.vchan.borrow_mut().buffer_space = 7;
        assert_eq!(under_test.read_message().unwrap(), None, "no bytes to read");
        assert_eq!(under_test.vchan.borrow().buffer_space, 0);
        assert_eq!(under_test.vchan.borrow().write_buf, b"test1\0another al");
        assert_eq!(under_test.queue.len(), 3);
        assert_eq!(under_test.queue, *b"pha");
        under_test.vchan.borrow_mut().buffer_space = 8;
        under_test.write(b" gamma delta").expect("write works");
        assert_eq!(
            under_test.vchan.borrow().write_buf,
            b"test1\0another alpha gamm"
        );
        under_test.write(b" gamma delta").expect("write works");
        under_test.write(b" gamma delta").expect("write works");
        under_test.vchan.borrow_mut().buffer_space = 8;
        assert_eq!(under_test.read_message().unwrap(), None, "no bytes to read");
        under_test.vchan.borrow_mut().buffer_space = 8;
        assert_eq!(under_test.read_message().unwrap(), None, "no bytes to read");
        under_test.vchan.borrow_mut().buffer_space = 8;
        assert_eq!(under_test.read_message().unwrap(), None, "no bytes to read");
        under_test.vchan.borrow_mut().buffer_space = 8;
        assert_eq!(under_test.read_message().unwrap(), None, "no bytes to read");
        assert_eq!(
            under_test.vchan.borrow().write_buf,
            b"test1\0another alpha gamma delta gamma delta gamma delta",
            "correct data written"
        );
    }
}
