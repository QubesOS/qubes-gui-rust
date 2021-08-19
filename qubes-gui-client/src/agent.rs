//! Grant-table manipulation code

use std::convert::TryInto;
use std::io;
use std::mem::size_of;
use std::os::unix::io::AsRawFd as _;
use std::rc::{Rc, Weak};

type DomID = u16;

/// A GUI agent instance
pub struct Agent {
    inner: super::Client,
    alloc: Rc<std::fs::File>,
    conf: qubes_gui::XConf,
    peer: DomID,
}

/// A window sent to the GUI daemon
pub struct Buffer {
    inner: Vec<u64>,
    alloc: Weak<std::fs::File>,
    ptr: *mut libc::c_void,
    offset: u64,
    grefs: u32,
    width: u32,
    height: u32,
}

impl Buffer {
    /// Obtains a slice containing the exported grant references
    pub fn grants(&self) -> &[u32] {
        unsafe {
            std::slice::from_raw_parts((self.inner.as_ptr() as *const u32).add(4), self.grefs as _)
        }
    }

    /// Sends the contents of the window to the GUI agent
    pub fn dump(&self, client: &mut super::Client, window: u32) -> io::Result<()> {
        let total_length =
            4usize * self.grefs as usize + std::mem::size_of::<qubes_gui::WindowDumpHeader>();
        let header = qubes_gui::Header {
            ty: qubes_gui::MSG_WINDOW_DUMP,
            window,
            untrusted_len: total_length.try_into().expect("bug"),
        };
        assert!(self.inner.capacity() * std::mem::size_of::<u64>() >= total_length as _);
        let msg = unsafe {
            std::slice::from_raw_parts(self.inner.as_ptr() as *const u8, total_length as _)
        };
        client
            .vchan
            .write(qubes_castable::Castable::as_bytes(&header))?;
        client.vchan.write(msg)?;
        Ok(())
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        let p = ioctl_gntalloc_dealloc_gref {
            index: self.offset,
            count: self.grefs,
        };
        assert!(self.ptr as usize % 4096 == 0, "Unaligned pointer???");
        let len: usize = self
            .grefs
            .checked_mul(qubes_gui::XC_PAGE_SIZE)
            .expect("grefs is bounded due to the width and height limits, so this cannot fail; qed")
            .try_into()
            .unwrap();
        if unsafe { libc::munmap(self.ptr, len) } != 0 {
            panic!(
                "the inputs are correct, and this is not punching a hole in an \
                 existing mapping, so munmap() cannot fail; qed; error {}",
                io::Error::last_os_error()
            )
        }
        if let Some(alloc) = self.alloc.upgrade() {
            unsafe {
                assert_eq!(
                    libc::ioctl(alloc.as_raw_fd(), IOCTL_GNTALLOC_DEALLOC_GREF, &p),
                    0,
                    "Releasing a grant reference never fails; qed",
                );
            }
        } // otherwise, the kernel has done the cleanup when the FD was closed
    }
}

#[repr(C)]
#[allow(nonstandard_style)]
struct ioctl_gntalloc_alloc_gref {
    domid: u16,
    flags: u16,
    count: u32,
    index: u64,
    gref_ids: [u32; 1],
}

#[repr(C)]
#[allow(nonstandard_style)]
struct ioctl_gntalloc_dealloc_gref {
    index: u64,
    count: u32,
}

impl Agent {
    /// Allocate a buffer to share with the GUI daemon.
    pub fn alloc_buffer(&mut self, width: u32, height: u32) -> io::Result<Buffer> {
        let _: [u8; 0] = [0u8; if size_of::<u32>() > size_of::<usize>() {
            1
        } else {
            0
        }];
        if width > qubes_gui::MAX_WINDOW_WIDTH || height > qubes_gui::MAX_WINDOW_HEIGHT {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "Window dimensions {}x{} too large (limit is {}x{})",
                    width,
                    height,
                    qubes_gui::MAX_WINDOW_WIDTH,
                    qubes_gui::MAX_WINDOW_HEIGHT
                ),
            ));
        }
        if width == 0 || height == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "Window dimensions {}x{} not valid: neither width nor height may be zero",
                    width, height,
                ),
            ));
        }
        assert_eq!(qubes_gui::XC_PAGE_SIZE % 4, 0);
        let pixels_per_gref = qubes_gui::XC_PAGE_SIZE / 4;
        let grefs = width
            .checked_mul(height)
            .expect("excessive width or height detected above")
            .checked_add(pixels_per_gref - 1)
            .expect("excessive width or height detected above")
            / pixels_per_gref;
        assert!(
            grefs <= qubes_gui::MAX_GRANT_REFS_COUNT,
            "excessive width or height detected above"
        );
        let mut channels: Vec<u64> = Vec::with_capacity((grefs as usize + 5) / 2);
        unsafe {
            let ptr = channels.as_mut_ptr() as *mut u8;
            std::ptr::write(ptr as *mut u16, self.peer);
            std::ptr::write((ptr as *mut u16).add(1), 1);
            std::ptr::write((ptr as *mut u32).add(1), grefs);
            if (grefs & 1) != 0 {
                assert_eq!(channels.capacity() * 2, grefs as usize + 5);
                std::ptr::write((ptr as *mut u32).add(grefs as usize + 4), 0)
            } else {
                assert_eq!(channels.capacity() * 2, grefs as usize + 4);
            }
            let res = libc::ioctl(
                self.alloc.as_raw_fd(),
                IOCTL_GNTALLOC_ALLOC_GREF,
                ptr as *mut ioctl_gntalloc_alloc_gref,
            );
            if res != 0 {
                assert_eq!(res, -1, "invalid return value from ioctl()");
                return Err(io::Error::last_os_error());
            }
            let offset = std::ptr::read((ptr as *const u64).offset(1));
            let ptr = libc::mmap(
                std::ptr::null_mut(),
                (width * height * 4)
                    .try_into()
                    .expect("u32 is smaller than usize; qed"),
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                self.alloc.as_raw_fd(),
                offset as i64,
            );
            if ptr == libc::MAP_FAILED {
                let p = ioctl_gntalloc_dealloc_gref {
                    index: offset,
                    count: grefs,
                };
                assert_eq!(
                    libc::ioctl(self.alloc.as_raw_fd(), IOCTL_GNTALLOC_DEALLOC_GREF, &p),
                    0,
                    "Failed to release grant references"
                );
                Err(io::Error::last_os_error())
            } else {
                let channel_ptr = channels.as_mut_ptr() as *mut u32;
                // overwrite the struct passed to Linux, which is no longer
                // needed, with the GUI message
                std::ptr::write(channel_ptr, qubes_gui::WINDOW_DUMP_TYPE_GRANT_REFS);
                std::ptr::write(channel_ptr.add(1), width);
                std::ptr::write(channel_ptr.add(2), height);
                std::ptr::write(channel_ptr.add(3), 24);
                Ok(Buffer {
                    inner: channels,
                    alloc: Rc::downgrade(&self.alloc),
                    ptr,
                    offset,
                    width,
                    height,
                    grefs,
                })
            }
        }
    }

    /// Obtains a reference to the client
    pub fn client(&mut self) -> &mut super::Client {
        &mut self.inner
    }

    /// Obtains the configuration provided by the GUI daemon
    pub fn conf(&self) -> qubes_gui::XConf {
        self.conf
    }
}

/// Creates a GUI agent
pub fn new(peer: DomID) -> io::Result<Agent> {
    let alloc = Rc::new(
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/xen/gntalloc")?,
    );
    let (inner, conf) = super::Client::agent(peer)?;
    Ok(Agent {
        alloc,
        inner,
        conf,
        peer,
    })
}

const IOCTL_GNTALLOC_ALLOC_GREF: std::os::raw::c_ulong = 0x184705;
const IOCTL_GNTALLOC_DEALLOC_GREF: std::os::raw::c_ulong = 0x104706;
