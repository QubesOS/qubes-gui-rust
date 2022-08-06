//! Grant-table manipulation code

use std::io;
use std::os::unix::io::AsRawFd as _;
use std::rc::{Rc, Weak};

type DomID = u16;

/// A GUI agent instance
pub struct Agent {
    alloc: Rc<std::fs::File>,
    peer: DomID,
}

/// A window sent to the GUI daemon
pub struct Buffer {
    inner: Vec<u64>,
    alloc: Weak<std::fs::File>,
    ptr: *mut libc::c_void,
    offset: u64,
    dimensions: dimensions::WindowDimensions,
}

mod dimensions {
    use std::io;
    use std::mem::size_of;
    pub(super) struct WindowDimensions {
        width: u32,
        height: u32,
    }

    impl WindowDimensions {
        pub fn new(width: u32, height: u32) -> io::Result<Self> {
            let _: [u8; 0] = [0u8; (size_of::<u32>() > size_of::<usize>()) as usize
                + ((u32::MAX / 4) / qubes_gui::MAX_WINDOW_WIDTH <= qubes_gui::MAX_WINDOW_HEIGHT)
                    as usize];
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
            Ok(Self { width, height })
        }

        pub fn buffer_size(&self) -> usize {
            assert!(self.width <= qubes_gui::MAX_WINDOW_WIDTH, "checked earlier");
            assert!(
                self.height <= qubes_gui::MAX_WINDOW_HEIGHT,
                "checked earlier"
            );
            (self.width * self.height * 4) as usize
        }

        pub fn grefs(&self) -> u32 {
            (self.buffer_size() as u32 + qubes_gui::XC_PAGE_SIZE - 1) / qubes_gui::XC_PAGE_SIZE
        }

        pub fn width(&self) -> u32 {
            self.width
        }

        pub fn height(&self) -> u32 {
            self.height
        }
    }
}

impl Buffer {
    /// Obtains a slice containing the exported grant references
    pub fn grants(&self) -> &[u32] {
        unsafe {
            std::slice::from_raw_parts(
                (self.inner.as_ptr() as *const u32).add(4),
                self.dimensions.grefs() as _,
            )
        }
    }

    /// Returns the width (in pixels) of this buffer
    pub fn width(&self) -> u32 {
        self.dimensions.width()
    }

    /// Returns the height (in pixels) of this buffer
    pub fn height(&self) -> u32 {
        self.dimensions.height()
    }

    /// Overwrite the specified offset in the buffer
    ///
    /// # Panics
    ///
    /// Panics if the offset is out of bounds.
    pub fn write(&self, buffer: &[u8], offset: usize) {
        let upper_bound = buffer
            .len()
            .checked_add(offset)
            .expect("offset + buffer length overflows");
        assert!(
            upper_bound <= self.dimensions.buffer_size(),
            "Copying to out of bounds memory"
        );
        assert!(buffer.len() % 4 == 0, "Copying fractional pixels");
        assert!(offset % 4 == 0, "Offset not integer pixel");

        unsafe {
            std::ptr::copy_nonoverlapping(
                buffer.as_ptr(),
                self.ptr.add(offset) as *mut u8,
                buffer.len(),
            )
        }
    }

    /// Returns the message (to send to the GUI daemon) as a byte slice
    pub fn msg(&self) -> &[u8] {
        let total_length = self.dimensions.grefs() * 4
            + (std::mem::size_of::<qubes_gui::WindowDumpHeader>() as u32);
        assert!(self.inner.capacity() * std::mem::size_of::<u64>() >= total_length as _);
        unsafe { std::slice::from_raw_parts(self.inner.as_ptr() as *const u8, total_length as _) }
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        let p = ioctl_gntalloc_dealloc_gref {
            index: self.offset,
            count: self.dimensions.grefs(),
        };
        assert!(self.ptr as usize % 4096 == 0, "Unaligned pointer???");
        if unsafe { libc::munmap(self.ptr, self.dimensions.buffer_size()) } != 0 {
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

// Indicates that a mapping should be writable
const GNTALLOC_FLAG_WRITABLE: u16 = 1;

#[repr(C)]
#[allow(nonstandard_style)]
struct ioctl_gntalloc_dealloc_gref {
    index: u64,
    count: u32,
}

impl Agent {
    /// Allocate a buffer to share with the GUI daemon.
    pub fn alloc_buffer(&mut self, width: u32, height: u32) -> io::Result<Buffer> {
        let dimensions = dimensions::WindowDimensions::new(width, height)?;
        assert_eq!(qubes_gui::XC_PAGE_SIZE % 4, 0);
        let grefs = dimensions.grefs();
        assert!(
            grefs <= qubes_gui::MAX_GRANT_REFS_COUNT,
            "excessive width or height detected above"
        );
        let mut channels: Vec<u64> = Vec::with_capacity((grefs as usize + 5) / 2);
        unsafe {
            let ptr = channels.as_mut_ptr() as *mut ioctl_gntalloc_alloc_gref;
            std::ptr::write(
                ptr,
                ioctl_gntalloc_alloc_gref {
                    domid: self.peer,
                    flags: GNTALLOC_FLAG_WRITABLE,
                    count: grefs,
                    index: 0,
                    gref_ids: [0],
                },
            );
            // Initialize the last u32 if needed
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
            channels.set_len(channels.capacity());
            let offset = (*ptr).index;
            let ptr = libc::mmap(
                std::ptr::null_mut(),
                dimensions.buffer_size(),
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
                    dimensions,
                })
            }
        }
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
    Ok(Agent { alloc, peer })
}

const IOCTL_GNTALLOC_ALLOC_GREF: std::os::raw::c_ulong = 0x184705;
const IOCTL_GNTALLOC_DEALLOC_GREF: std::os::raw::c_ulong = 0x104706;
