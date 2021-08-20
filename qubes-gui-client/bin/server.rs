use libc::{poll, pollfd};
use qubes_castable::Castable;
use std::convert::TryInto;
// use std::fs::File;
// use std::os::raw::{c_int, c_short, c_ulong};
// use std::os::unix::io::AsRawFd as _;
use std::task::Poll;

fn main() {
    let (width, height) = (0x200, 0x100);
    // let buffer: Vec<u32> = vec![0; (width * height).try_into().unwrap()];
    let mut vchan = qubes_gui_client::agent::new(0).unwrap();
    // we now have a vchan 🙂
    println!("🙂 Somebody connected to us, yay!");
    println!("Configuration parameters: {:?}", vchan.conf());
    println!("Creating window");
    vchan
        .client()
        .send(
            &qubes_gui::Create {
                rectangle: qubes_gui::Rectangle {
                    top_left: qubes_gui::Coordinates { x: 50, y: 400 },
                    size: qubes_gui::WindowSize { width, height },
                },
                parent: None,
                override_redirect: 0,
            },
            50.try_into().unwrap(),
        )
        .unwrap();
    let buf = vchan.alloc_buffer(width, height).unwrap();
    println!("Grant references: {:#?}", buf.grants());
    buf.dump(vchan.client(), 50).unwrap();
    vchan
        .client()
        .send(
            &qubes_gui::MapInfo {
                override_redirect: 0,
                transient_for: 0,
            },
            50.try_into().unwrap(),
        )
        .unwrap();
    loop {
        match vchan.client().read_header() {
            Poll::Pending => {}
            Poll::Ready(Ok((e, body))) => match e.ty {
                qubes_gui::MSG_MOTION => {
                    let mut m = qubes_gui::Motion::default();
                    m.as_mut_bytes().copy_from_slice(body);
                    println!("Motion event: {:?}", m)
                }
                qubes_gui::MSG_CROSSING => {
                    let mut m = qubes_gui::Crossing::default();
                    m.as_mut_bytes().copy_from_slice(body);
                    println!("Crossing event: {:?}", m)
                }
                qubes_gui::MSG_CLOSE => {
                    assert!(body.is_empty());
                    println!("Got a close event, exiting!");
                    return;
                }
                qubes_gui::MSG_KEYPRESS => {
                    let mut m = qubes_gui::Button::default();
                    m.as_mut_bytes().copy_from_slice(body);
                    println!("Key pressed: {:?}", m);
                }
                qubes_gui::MSG_BUTTON => {
                    let mut m = qubes_gui::Button::default();
                    m.as_mut_bytes().copy_from_slice(body);
                    println!("Button event: {:?}", m);
                }
                qubes_gui::MSG_CLIPBOARD_REQ => println!("clipboard data requested!"),
                qubes_gui::MSG_CLIPBOARD_DATA => println!("clipboard data reply!"),
                qubes_gui::MSG_KEYMAP_NOTIFY => {
                    let mut m = qubes_gui::KeymapNotify::default();
                    m.as_mut_bytes().copy_from_slice(body);
                    println!("Keymap notification: {:?}", m);
                }
                qubes_gui::MSG_MAP => {
                    let mut m = qubes_gui::MapInfo::default();
                    m.as_mut_bytes().copy_from_slice(body);
                    println!("Map event: {:?}", m);
                }
                qubes_gui::MSG_CONFIGURE => {
                    let mut m = qubes_gui::Configure::default();
                    m.as_mut_bytes().copy_from_slice(body);
                    println!("Configure event: {:?}", m);
                }
                qubes_gui::MSG_FOCUS => {
                    let mut m = qubes_gui::Focus::default();
                    m.as_mut_bytes().copy_from_slice(body);
                    println!("Focus event: {:?}", m);
                }
                _ => println!("Got an event! Header {:?}, body {:?}", e, body),
            },
            Poll::Ready(Err(e)) => panic!("Got an error: {:?}", e),
        }
        let mut s = [libc::pollfd {
            fd: vchan.client().as_raw_fd(),
            events: libc::POLLIN | libc::POLLOUT | libc::POLLHUP | libc::POLLPRI,
            revents: 0,
        }];
        if unsafe { poll(s.as_mut_ptr(), 1, -1) } != 1 {
            panic!("poll(2) failed")
        }
    }
}
