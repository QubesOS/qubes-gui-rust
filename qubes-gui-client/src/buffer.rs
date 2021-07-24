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

use std::collections::VecDeque;
use std::io::{Result, Write};

#[derive(Debug)]
pub(crate) struct Vchan {
    vchan: vchan::Vchan,
    queue: VecDeque<Vec<u8>>,
    offset: usize,
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
}
