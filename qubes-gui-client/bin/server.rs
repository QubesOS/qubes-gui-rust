use libc::{poll, pollfd};
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
            Poll::Ready(Ok(e)) => println!("Got an event from dom0: {:?}", e),
            Poll::Ready(Err(e)) => panic!("Got an error: {:?}", e),
        }
        let mut s = pollfd {
            fd: vchan.client().as_raw_fd(),
            events: 7,
            revents: 0,
        };
        if unsafe { poll(&mut s as *mut pollfd, 1, -1) } != 1 {
            panic!("poll(2) failed");
        }
    }
}
