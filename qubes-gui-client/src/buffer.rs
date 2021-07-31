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

use qubes_castable::Castable as _;
use qubes_gui::Header;
use std::collections::VecDeque;
use std::io::{Error, ErrorKind, Read, Result, Write};
use std::mem::size_of;

#[derive(Debug)]
enum ReadState {
    ReadingHeader,
    ReadingBody(Header, usize),
    Discard(usize),
}

#[derive(Debug)]
pub(crate) struct Vchan {
    vchan: vchan::Vchan,
    queue: VecDeque<Vec<u8>>,
    offset: usize,
    state: ReadState,
    header: Header,
    buffer: Vec<u8>,
}

impl Vchan {
    fn write_slice(vchan: &mut vchan::Vchan, slice: &[u8]) -> Result<usize> {
        let space = vchan.buffer_space();
        if space == 0 {
            Ok(0)
        } else {
            let to_write = space.min(slice.len());
            vchan.write(&slice[..to_write])
        }
    }

    fn drain(&mut self) -> Result<usize> {
        let mut written = 0;
        loop {
            let front: &mut _ = match self.queue.front_mut() {
                None => break Ok(written),
                Some(e) => e,
            };
            let to_write = &front[self.offset..];
            if to_write.is_empty() {
                self.queue.pop_front();
                self.offset = 0;
                continue;
            }
            let written_this_time = Self::write_slice(&mut self.vchan, to_write)?;
            written += written_this_time;
            self.offset += written_this_time;
            if written_this_time < to_write.len() {
                break Ok(written);
            }
        }
    }

    pub fn write(&mut self, buf: &[u8]) -> Result<()> {
        self.drain()?;
        if !self.queue.is_empty() {
            self.queue.push_back(buf.to_owned());
            return Ok(());
        }
        assert_eq!(self.offset, 0);
        let written = Self::write_slice(&mut self.vchan, buf)?;
        if written != buf.len() {
            assert!(written < buf.len());
            self.queue.push_back(buf[written..].to_owned());
        }
        Ok(())
    }

    /// If there is nothing to read, return `Ok(None)` immediately; otherwise,
    /// returns `Ok(Some(msg))` if a complete message has been read, or `Err`
    /// if something went wrong.
    pub fn read_header(&mut self) -> Result<Option<(Header, &[u8])>> {
        self.drain()?;
        let mut ready = self.vchan.data_ready();
        loop {
            if ready == 0 {
                break Ok(None);
            }
            match self.state {
                ReadState::ReadingHeader if ready >= size_of::<Header>() => {
                    let mut header = <Header as Default>::default();
                    if self.vchan.recv(header.as_mut_bytes())? != size_of::<Header>() {
                        break Err(Error::new(
                            ErrorKind::UnexpectedEof,
                            "Failed to read a full message header",
                        ));
                    }
                    ready -= size_of::<Header>();
                    match qubes_gui::check_message_length(header.ty, header.untrusted_len) {
                        None => self.state = ReadState::Discard(header.untrusted_len as _),
                        Some(Ok(())) => {
                            // length was sanitized above
                            self.buffer.resize(header.untrusted_len as _, 0);
                            self.state = ReadState::ReadingBody(header, 0)
                        }
                        Some(Err(())) => {
                            break Err(Error::new(
                                ErrorKind::InvalidData,
                                "Incoming packet has invalid size",
                            ))
                        }
                    }
                }
                ReadState::ReadingHeader => break Ok(None),
                ReadState::Discard(len) => {
                    self.buffer.resize(256.min(len).max(self.buffer.len()), 0);
                    let buf_len = self.buffer.len();
                    let bytes_read = self
                        .vchan
                        .read(&mut self.buffer[..ready.min(len.min(buf_len) as usize)])?;
                    if len == bytes_read {
                        self.state = ReadState::ReadingHeader
                    } else if bytes_read == 0 {
                        break Err(Error::new(ErrorKind::UnexpectedEof, "EOF on the vchan"));
                    } else {
                        assert!(len > bytes_read);
                        self.state = ReadState::Discard(len - bytes_read)
                    }
                }
                ReadState::ReadingBody(header, read_so_far) => {
                    let buffer_len = self.buffer.len();
                    let to_read = ready.min(buffer_len - read_so_far);
                    let bytes_read = self
                        .vchan
                        .read(&mut self.buffer[read_so_far..read_so_far + to_read])?;
                    if bytes_read == to_read {
                        self.state = ReadState::ReadingHeader;
                        break Ok(Some((header, &self.buffer[..])));
                    } else if bytes_read == 0 {
                        break Err(Error::new(ErrorKind::UnexpectedEof, "EOF on the vchan"));
                    } else {
                        assert!(to_read > bytes_read);
                        self.state = ReadState::ReadingBody(header, read_so_far + bytes_read)
                    }
                }
            }
        }
    }
}
