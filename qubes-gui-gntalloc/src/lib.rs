//! Grant-table manipulation code

#![forbid(clippy::all)]
use std::io;
use std::mem::size_of;
use std::os::unix::io::AsRawFd as _;
use std::rc::{Rc, Weak};

type DomID = u16;

/// A GUI agent buffer allocator
pub struct Allocator {
    alloc: Rc<std::fs::File>,
    peer: DomID,
}

/// A buffer sent to the GUI daemon
pub struct Buffer {
    /// The GUI message.  Logically, this is a [`qubes_gui::WindowDumpHeader`] followed by an array
    /// of u32, but it is a `Vec<u64>` for alignment reasons.
    message: Vec<u64>,
    /// The underlying file used for ioctl calls.  This is necessary for cleanup
    /// in the destructor.  If the file is closed, the kernel will handle
    /// cleanup, so this is a weak reference.
    alloc: Weak<std::fs::File>,
    /// The memory-mapped buffer.
    ptr: *mut libc::c_void,
    /// The offset to be passed to [`libc::mmap`].
    offset: u64,
    /// The window dimensions.
    dimensions: dimensions::WindowDimensions,
}

mod dimensions {
    use qubes_castable::static_assert;
    use std::io;
    use std::mem::size_of;
    pub(super) struct WindowDimensions {
        width: u32,
        height: u32,
    }

    impl WindowDimensions {
        pub fn new(width: u32, height: u32) -> io::Result<Self> {
            static_assert!(size_of::<u32>() < size_of::<usize>());
            static_assert!(
                (u32::MAX / 4) / qubes_gui::MAX_WINDOW_WIDTH > qubes_gui::MAX_WINDOW_HEIGHT
            );
            static_assert!(
                qubes_gui::MAX_WINDOW_WIDTH * qubes_gui::MAX_WINDOW_HEIGHT * 4
                    < u32::MAX - qubes_gui::XC_PAGE_SIZE
            );
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
            debug_assert!(self.width <= qubes_gui::MAX_WINDOW_WIDTH, "checked earlier");
            debug_assert!(
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
        // SAFETY: the buffer is valid for the specified bytes.
        unsafe {
            std::slice::from_raw_parts(
                (self.message.as_ptr() as *const u32).add(HEADER_U32S),
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

        // SAFETY: Bounds were checked above.
        unsafe {
            std::ptr::copy(
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
        assert!(self.message.capacity() * std::mem::size_of::<u64>() >= total_length as _);
        unsafe { std::slice::from_raw_parts(self.message.as_ptr() as *const u8, total_length as _) }
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        let p = ioctl_gntalloc_dealloc_gref {
            index: self.offset,
            count: self.dimensions.grefs(),
        };
        assert!(self.ptr as usize % 4096 == 0, "Unaligned pointer???");
        // SAFETY: the munmap parameters are correct
        if unsafe { libc::munmap(self.ptr, self.dimensions.buffer_size()) } != 0 {
            panic!(
                "the inputs are correct, and this is not punching a hole in an \
                 existing mapping, so munmap() cannot fail; qed; error {}",
                io::Error::last_os_error()
            )
        }
        if let Some(alloc) = self.alloc.upgrade() {
            // SAFETY: the ioctl parameters are correct
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
    gref_ids: [u32; 0],
}

/// Indicates that a mapping should be writable
const GNTALLOC_FLAG_WRITABLE: u16 = 1;

/// The size of the header
const HEADER_U32S: usize = size_of::<ioctl_gntalloc_alloc_gref>() / size_of::<u32>();

#[repr(C)]
#[allow(nonstandard_style)]
struct ioctl_gntalloc_dealloc_gref {
    index: u64,
    count: u32,
}

impl Allocator {
    /// Allocate a buffer to share with the GUI daemon.
    pub fn alloc_buffer(&mut self, width: u32, height: u32) -> io::Result<Buffer> {
        let dimensions = dimensions::WindowDimensions::new(width, height)?;
        assert_eq!(qubes_gui::XC_PAGE_SIZE % 4, 0);
        let grefs = dimensions.grefs();
        let mut message: Vec<u64> = Vec::with_capacity((grefs as usize + 5) / 2);
        unsafe {
            let ptr = message.as_mut_ptr() as *mut ioctl_gntalloc_alloc_gref;
            // SAFETY: ptr points to a sufficiently large, properly-aligned buffer.
            std::ptr::write(
                ptr,
                ioctl_gntalloc_alloc_gref {
                    domid: self.peer,
                    flags: GNTALLOC_FLAG_WRITABLE,
                    count: grefs,
                    index: 0,
                    gref_ids: [],
                },
            );
            // Initialize the last u32 if needed
            if (grefs & 1) != 0 {
                assert_eq!(message.capacity() * 2, grefs as usize + HEADER_U32S + 1);
                // SAFETY: ptr points to a sufficiently large, properly-aligned buffer.
                std::ptr::write((ptr as *mut u32).add(grefs as usize + HEADER_U32S), 0)
            } else {
                assert_eq!(message.capacity() * 2, grefs as usize + HEADER_U32S);
            }
            // SAFETY: the ioctl parameters are correct.
            let res = libc::ioctl(
                self.alloc.as_raw_fd(),
                IOCTL_GNTALLOC_ALLOC_GREF,
                ptr as *mut ioctl_gntalloc_alloc_gref,
            );
            if res != 0 {
                assert_eq!(res, -1, "invalid return value from ioctl()");
                return Err(io::Error::last_os_error());
            }
            // SAFETY: the buffer has now been fully initialized and the length
            // is equal to the capacity.
            message.set_len(message.capacity());
            // SAFETY: ptr is correct.
            let offset = (*ptr).index;
            // SAFETY: mmap parameters are correct.
            let ptr = libc::mmap(
                std::ptr::null_mut(),
                dimensions.buffer_size(),
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                self.alloc.as_raw_fd(),
                offset as libc::off_t,
            );
            if ptr == libc::MAP_FAILED {
                let p = ioctl_gntalloc_dealloc_gref {
                    index: offset,
                    count: grefs,
                };
                assert_eq!(
                    // SAFETY: the ioctl parameters are correct.
                    libc::ioctl(self.alloc.as_raw_fd(), IOCTL_GNTALLOC_DEALLOC_GREF, &p),
                    0,
                    "Failed to release grant references"
                );
                Err(io::Error::last_os_error())
            } else {
                // overwrite the struct passed to Linux, which is no longer
                // needed, with the GUI message
                std::ptr::write(
                    message.as_mut_ptr() as *mut _,
                    qubes_gui::WindowDumpHeader {
                        ty: qubes_gui::WINDOW_DUMP_TYPE_GRANT_REFS,
                        width,
                        height,
                        bpp: 24,
                    },
                );
                Ok(Buffer {
                    message,
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
pub fn new(peer: DomID) -> io::Result<Allocator> {
    let alloc = Rc::new(
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/xen/gntalloc")?,
    );
    Ok(Allocator { alloc, peer })
}

const IOCTL_GNTALLOC_ALLOC_GREF: std::os::raw::c_ulong = 0x184705;
const IOCTL_GNTALLOC_DEALLOC_GREF: std::os::raw::c_ulong = 0x104706;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gref_limits() {
        let max_dims = dimensions::WindowDimensions::new(
            qubes_gui::MAX_WINDOW_WIDTH,
            qubes_gui::MAX_WINDOW_HEIGHT,
        )
        .unwrap();
        assert!(dimensions::WindowDimensions::new(
            qubes_gui::MAX_WINDOW_WIDTH + 1,
            qubes_gui::MAX_WINDOW_HEIGHT
        )
        .is_err());
        assert!(dimensions::WindowDimensions::new(
            qubes_gui::MAX_WINDOW_WIDTH,
            qubes_gui::MAX_WINDOW_HEIGHT + 1
        )
        .is_err());
        assert_eq!(max_dims.grefs(), qubes_gui::MAX_GRANT_REFS_COUNT);
    }
}
