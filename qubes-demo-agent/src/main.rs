use qubes_gui_agent_proto::Event;
use std::convert::TryInto;
use std::os::unix::io::AsRawFd as _;
use std::task::Poll;

fn main() -> std::io::Result<()> {
    let (width, height) = (0x200, 0x100);
    let mut connection = qubes_gui_gntalloc::new(0).unwrap();
    let (mut vchan, conf) = qubes_gui_client::Client::agent(0).unwrap();
    // we now have a vchan 🙂
    println!("🙂 Somebody connected to us, yay!");
    println!("Configuration parameters: {:?}", conf);
    println!("Creating window");
    let window = 50.try_into().unwrap();
    let create = qubes_gui::Create {
        rectangle: qubes_gui::Rectangle {
            top_left: qubes_gui::Coordinates { x: 50, y: 400 },
            size: qubes_gui::WindowSize { width, height },
        },
        parent: None,
        override_redirect: 0,
    };
    vchan.send(&create, window).unwrap();
    let mut buf = connection.alloc_buffer(width, height).unwrap();
    let mut shade = vec![0xFF00u32; (width * height / 2).try_into().unwrap()];
    buf.write(
        qubes_castable::as_bytes(&shade[..]),
        (width * height).try_into().unwrap(),
    );
    vchan
        .send_raw(buf.msg(), window, qubes_gui::Msg::WindowDump as _)
        .unwrap();
    let title = b"Qubes Demo Rust GUI Agent";
    let mut title_buf = [0u8; 128];
    title_buf[..title.len()].copy_from_slice(title);
    vchan
        .send_raw(&title_buf, window, qubes_gui::Msg::SetTitle as _)
        .unwrap();
    vchan
        .send(
            &qubes_gui::MapInfo {
                override_redirect: 0,
                transient_for: 0,
            },
            window,
        )
        .unwrap();
    vchan.wait();
    loop {
        let (window, e) = loop {
            match vchan.read_message().map(Result::unwrap) {
                Poll::Pending => vchan.wait(),
                Poll::Ready((hdr, body)) => match Event::parse(hdr, body).unwrap() {
                    None => {}
                    Some(ev) => break ev,
                },
            }
        };
        match e {
            Event::Motion(event) => println!("Motion event: {:?}", event),
            Event::Crossing(event) => println!("Crossing event: {:?}", event),
            Event::Close => {
                println!("Got a close event, exiting!");
                return Ok(());
            }
            Event::Keypress(event) => println!("Key pressed: {:?}", event),
            Event::Button(event) => println!("Button event: {:?}", event),
            Event::ClipboardReq => println!("clipboard data requested!"),
            Event::ClipboardData { untrusted_data } => {
                println!("clipboard paste, data {:?}", untrusted_data)
            }
            Event::Keymap(new_keymap) => println!("New keymap: {:?}", new_keymap),
            Event::Redraw(portion_to_redraw) => println!("Map event: {:?}", portion_to_redraw),
            Event::Configure(new_size_and_position) => {
                println!("Configure event: {:?}", new_size_and_position);
                let rectangle = new_size_and_position.rectangle;
                let qubes_gui::WindowSize { width, height } = rectangle.size;
                drop(std::mem::replace(
                    &mut buf,
                    connection.alloc_buffer(width, height).unwrap(),
                ));
                shade.resize((width * height / 2).try_into().unwrap(), 0xFF00u32);
                buf.write(
                    qubes_castable::as_bytes(&shade[..]),
                    (width * height / 4 * 4).try_into().unwrap(),
                );
                vchan
                    .send_raw(
                        buf.msg(),
                        50.try_into().unwrap(),
                        qubes_gui::Msg::WindowDump as _,
                    )
                    .unwrap();

                vchan.send(&new_size_and_position, window).unwrap();
                vchan
                    .send(&qubes_gui::ShmImage { rectangle }, window)
                    .unwrap()
            }
            Event::Focus(event) => println!("Focus event: {:?}", event),
            Event::WindowFlags(flags) => {
                println!("Window manager flags have changed: {:?}", flags)
            }
            _ => println!("Got an unknown event!"),
        }
        let mut s = [libc::pollfd {
            fd: vchan.as_raw_fd(),
            events: libc::POLLIN | libc::POLLOUT | libc::POLLHUP | libc::POLLPRI,
            revents: 0,
        }];
        if unsafe { libc::poll(s.as_mut_ptr(), 1, -1) } != 1 {
            panic!("poll(2) failed")
        }
    }
}
