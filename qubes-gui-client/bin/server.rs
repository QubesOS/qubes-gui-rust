use qubes_gui_client::agent::DaemonToAgentEvent as Event;
use std::convert::TryInto;
use std::os::unix::io::AsRawFd as _;

fn main() -> std::io::Result<()> {
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
    let mut buf = vchan.alloc_buffer(width, height).unwrap();
    let mut shade = vec![0xFF00u32; (width * height / 2).try_into().unwrap()];
    buf.dump(vchan.client(), 50).unwrap();
    buf.write(
        qubes_castable::as_bytes(&shade[..]),
        (width * height).try_into().unwrap(),
    );
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
        vchan.client().wait();
        while let Some(ev) = vchan.client().next_event()? {
            match ev {
                Event::Motion { window: _, event } => println!("Motion event: {:?}", event),
                Event::Crossing { window: _, event } => println!("Crossing event: {:?}", event),
                Event::Close { window: _ } => {
                    println!("Got a close event, exiting!");
                    return Ok(());
                }
                Event::Keypress { window: _, event } => println!("Key pressed: {:?}", event),
                Event::Button { window: _, event } => println!("Button event: {:?}", event),
                Event::Copy => println!("clipboard data requested!"),
                Event::Paste { untrusted_data } => {
                    println!("clipboard paste, data {:?}", untrusted_data)
                }
                Event::Keymap { new_keymap } => println!("New keymap: {:?}", new_keymap),
                Event::Redraw {
                    window: _,
                    portion_to_redraw,
                } => println!("Map event: {:?}", portion_to_redraw),
                Event::Configure {
                    window,
                    new_size_and_position,
                } => {
                    println!("Configure event: {:?}", new_size_and_position);
                    let rectangle = new_size_and_position.rectangle;
                    let qubes_gui::WindowSize { width, height } = rectangle.size;
                    drop(std::mem::replace(
                        &mut buf,
                        vchan.alloc_buffer(width, height).unwrap(),
                    ));
                    shade.resize((width * height / 2).try_into().unwrap(), 0xFF00u32);
                    buf.write(
                        qubes_castable::as_bytes(&shade[..]),
                        (width * height / 4 * 4).try_into().unwrap(),
                    );
                    buf.dump(vchan.client(), window).unwrap();
                    let w = window.try_into().unwrap();
                    vchan.client().send(&new_size_and_position, w).unwrap();
                    vchan
                        .client()
                        .send(&qubes_gui::ShmImage { rectangle }, w)
                        .unwrap()
                }
                Event::Focus { window: _, event } => println!("Focus event: {:?}", event),
                Event::WindowFlags { window: _, flags } => {
                    println!("Window manager flags have changed: {:?}", flags)
                }
                _ => println!("Got an unknown event!"),
            }
        }
        let mut s = [libc::pollfd {
            fd: vchan.client().as_raw_fd(),
            events: libc::POLLIN | libc::POLLOUT | libc::POLLHUP | libc::POLLPRI,
            revents: 0,
        }];
        if unsafe { libc::poll(s.as_mut_ptr(), 1, -1) } != 1 {
            panic!("poll(2) failed")
        }
    }
}
