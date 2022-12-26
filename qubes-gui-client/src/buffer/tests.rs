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
 */

use super::*;
use std::cell::RefCell;
use std::rc::Rc;
struct MockVchan {
    read_buf: Vec<u8>,
    write_buf: Vec<u8>,
    buffer_space: usize,
    data_ready: usize,
    cursor: usize,
}

impl VchanMock for Rc<RefCell<MockVchan>> {
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
    fn send(&self, buffer: &[u8]) -> Result<(), vchan::Error> {
        let mut s = self.borrow_mut();
        assert!(
            buffer.len() <= s.buffer_space,
            "Agents never write more space than is available"
        );
        s.write_buf.extend_from_slice(buffer);
        s.buffer_space -= buffer.len();
        Ok(())
    }
    fn recv_into(&self, buffer: &mut Vec<u8>, bytes: usize) -> Result<(), vchan::Error> {
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
        s.cursor += bytes;
        s.data_ready -= bytes;
        Ok(())
    }
    fn recv_struct<T: Castable + Default>(&self) -> Result<T, vchan::Error> {
        let mut s = self.borrow_mut();
        let mut v: T = Default::default();
        assert!(
            s.read_buf.len() >= s.data_ready && s.read_buf.len() - s.data_ready >= s.cursor,
            "mock vchan internal bounds error: len is {} and ready is {} but cursor is {}",
            s.read_buf.len(),
            s.data_ready,
            s.cursor,
        );
        let b = v.as_mut_bytes();
        eprintln!("Reading {} bytes with {} ready", b.len(), s.data_ready);
        assert!(
            b.len() <= s.data_ready,
            "Agents never read more data than is available"
        );
        b.copy_from_slice(&s.read_buf[s.cursor..s.cursor + b.len()]);
        s.cursor += b.len();
        s.data_ready -= b.len();
        Ok(v)
    }
    fn discard(&self, bytes: usize) -> Result<(), vchan::Error> {
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
        s.data_ready -= bytes;
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
    let mut under_test = RawMessageStream::<Rc<RefCell<MockVchan>>> {
        vchan: Rc::new(RefCell::new(mock_vchan)),
        queue: Default::default(),
        state: ReadState::Connecting,
        buffer: vec![],
        did_reconnect: false,
        xconf: Default::default(),
        kind: Kind::Agent,
        domid: 0,
    };
    under_test.vchan.borrow_mut().buffer_space = 4;
    assert!(
        under_test.read_message().unwrap().is_none(),
        "no bytes to read"
    );
    under_test.vchan.borrow_mut().write_buf.clear();
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
    assert!(
        under_test.read_message().unwrap().is_none(),
        "no bytes to read"
    );
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
    let version = qubes_gui::XConfVersion {
        version: 0x10004,
        xconf: Default::default(),
    };
    under_test
        .vchan
        .borrow_mut()
        .read_buf
        .extend_from_slice(&version.as_bytes());
    under_test.vchan.borrow_mut().data_ready = 12;

    assert!(under_test.vchan.data_ready() < size_of::<qubes_gui::XConfVersion>());
    assert!(matches!(under_test.state, ReadState::Negotiating));
    assert!(
        under_test.read_message().unwrap().is_none(),
        "not enough bytes to read"
    );
    assert_eq!(under_test.vchan.borrow().data_ready, 12);
    assert!(matches!(under_test.state, ReadState::Negotiating));
    under_test.vchan.borrow_mut().data_ready += 8;
    under_test.vchan.borrow_mut().buffer_space = 8;
    assert!(
        under_test.read_message().unwrap().is_none(),
        "no bytes to read"
    );
    assert_eq!(under_test.vchan.borrow().data_ready, 0);
    assert!(matches!(under_test.state, ReadState::ReadingHeader));
    under_test.vchan.borrow_mut().buffer_space = 8;
    assert!(
        under_test.read_message().unwrap().is_none(),
        "no bytes to read"
    );
    under_test.vchan.borrow_mut().buffer_space = 8;
    assert!(
        under_test.read_message().unwrap().is_none(),
        "no bytes to read"
    );
    assert_eq!(
        under_test.vchan.borrow().write_buf,
        b"test1\0another alpha gamma delta gamma delta gamma delta",
        "correct data written"
    );
}

macro_rules! s {
    ($v: ty) => {
        ::std::mem::size_of::<$v>() as u32
    };
}

#[test]
fn vchan_reads() {
    use qubes_gui::WindowID;
    let mock_vchan = MockVchan {
        read_buf: vec![],
        write_buf: vec![],
        buffer_space: 0,
        data_ready: 0,
        cursor: 0,
    };
    let vchan = Rc::new(RefCell::new(mock_vchan));
    let mut under_test = RawMessageStream::<Rc<RefCell<MockVchan>>> {
        vchan: vchan.clone(),
        queue: Default::default(),
        state: ReadState::ReadingHeader,
        buffer: vec![],
        did_reconnect: false,
        xconf: Default::default(),
        domid: 0,
        kind: Kind::Agent,
    };
    let mut hdr = UntrustedHeader {
        untrusted_len: 1,
        ty: qubes_gui::MSG_MFNDUMP,
        window: 0.into(),
    };
    under_test
        .vchan
        .borrow_mut()
        .read_buf
        .extend_from_slice(hdr.as_bytes());
    under_test.vchan.borrow_mut().data_ready = 2;
    assert!(
        under_test.read_message().unwrap().is_none(),
        "not enough data"
    );
    assert!(matches!(under_test.state, ReadState::ReadingHeader));
    under_test.vchan.borrow_mut().data_ready = 12;
    assert!(under_test.read_message().is_err(), "bad header!");
    assert!(matches!(under_test.state, ReadState::Error));

    // Test that a header and partial body can be read in one go
    under_test.state = ReadState::ReadingHeader;
    hdr.ty = qubes_gui::MSG_CONFIGURE;
    hdr.untrusted_len = s!(qubes_gui::Configure);
    under_test.vchan.borrow_mut().data_ready = 13;
    under_test
        .vchan
        .borrow_mut()
        .read_buf
        .extend_from_slice(hdr.as_bytes());
    let c = qubes_gui::Configure {
        rectangle: qubes_gui::Rectangle {
            top_left: qubes_gui::Coordinates { x: 0, y: 0 },
            size: qubes_gui::WindowSize {
                width: 1,
                height: 1,
            },
        },
        override_redirect: 0,
    };
    vchan.borrow_mut().read_buf.extend_from_slice(c.as_bytes());
    assert!(
        under_test.read_message().unwrap().is_none(),
        "body not fully written yet!"
    );
    match under_test.state {
        ReadState::ReadingBody { header } => assert_eq!(header.inner(), hdr),
        e => panic!("Bad state {:?}!", e),
    }
    assert_eq!(under_test.buffer.len(), 1);
    assert_eq!(vchan.borrow_mut().data_ready, 0);

    // Test partial reads when there is already some data in the buffer
    vchan.borrow_mut().data_ready = 5;
    assert!(
        under_test.read_message().unwrap().is_none(),
        "body not fully written yet!"
    );
    match under_test.state {
        ReadState::ReadingBody { header } => assert_eq!(header.inner(), hdr),
        e => panic!("Bad state {:?}!", e),
    }
    assert_eq!(under_test.buffer.len(), 6);
    assert_eq!(vchan.borrow_mut().data_ready, 0);

    // Test completion of body
    vchan.borrow_mut().data_ready = s!(qubes_gui::Configure) as usize - 6;
    assert!(under_test.read_message().unwrap().is_some(), "have a body!");
    assert!(matches!(under_test.state, ReadState::ReadingHeader));
    assert_eq!(under_test.buffer.len(), s!(qubes_gui::Configure) as _);
    assert_eq!(vchan.borrow_mut().data_ready, 0);
}
