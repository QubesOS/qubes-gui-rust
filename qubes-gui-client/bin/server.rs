use std::convert::TryInto;
use std::task::Poll;

#[repr(C)]
#[allow(nonstandard_style)]
struct pollfd {
    fd: std::os::raw::c_int,
    events: std::os::raw::c_short,
    revents: std::os::raw::c_short,
}

#[link(name = "c", kind = "dylib")]
extern "C" {
    fn poll(
        data: *mut pollfd,
        size: std::os::raw::c_ulong,
        timeout: std::os::raw::c_int,
    ) -> std::os::raw::c_int;
}

fn main() {
    let (mut vchan, conf) = qubes_gui_client::Client::agent(0).unwrap();
    // we now have a vchan 🙂
    println!("🙂 Somebody connected to us, yay!");
    println!("Configuration parameters: {:?}", conf);
    println!("Creating window");
    vchan
        .send(
            &qubes_gui::Create {
                rectangle: qubes_gui::Rectangle {
                    top_left: qubes_gui::Coordinates { x: 50, y: 400 },
                    size: qubes_gui::WindowSize {
                        width: 300,
                        height: 100,
                    },
                },
                parent: None,
                override_redirect: 0,
            },
            50.try_into().unwrap(),
        )
        .unwrap();
    loop {
        match vchan.read_header() {
            Poll::Pending => {}
            Poll::Ready(Ok(e)) => println!("Got an event from dom0: {:?}", e),
            Poll::Ready(Err(e)) => panic!("Got an error: {:?}", e),
        }
        let mut s = pollfd {
            fd: vchan.as_raw_fd(),
            events: 7,
            revents: 0,
        };
        if unsafe { poll(&mut s as *mut pollfd, 1, -1) } != 1 {
            panic!("poll(2) failed");
        }
    }
}
