//! Qubes GUI protocol library.  This provides only the protocol definition; it
//! does no I/O.
//!
//! # Transport and message format
//!
//! The Qubes OS GUI protocol is spoken over a vchan between two virtual
//! machines.  Each message is a C struct that is cast to a byte slice and sent
//! directly over the vchan, without any marshalling or unmarshalling steps.
//! This is safe because no GUI message has any padding bytes.  Similarly, the
//! receiver casts a C struct to a mutable byte slice and reads the bytes
//! directly into the struct.  This is safe because all possible bit patterns
//! are valid for every GUI message.  All messages are in native byte order,
//! which is little-endian for the only platform (amd64) supported by Qubes OS.
//!
//! This is very natural to implement in C, but is much less natural to
//! implement in Rust, as casting a struct reference to a byte slice is
//! `unsafe`.  To ensure that this does not cause security vulnerabilities,
//! this library uses the `qubes-castable` crate.  That crate provides a
//! `castable!` macro to define structs that can be safely casted to a byte
//! slice.  `castable!` guarantees that every struct it defines can be safely
//! cast to a byte slice and back; if it cannot, a compile-time error results.
//! Functions provided by the `qubes-castable` crate are used to perform the
//! conversions.  To ensure that they cannot be called on inappropriate types
//! (such as `bool`), they require the unsafe `Castable` trait to be implemented.
//! The `castable!` macro implements this trait for every type it defines, and
//! the `qubes-castable` crate implements it for all fixed-width primitive
//! integer types, `()`, and arrays of `Castable` objects (regardless of length).
//!
//! Both clients and servers MUST send each message atomically.  Specifically,
//! clients and servers MAY use blocking I/O to read each message.  Therefore,
//! messages MUST be finished soon after they have started, to avoid deadlocks.
//! This requirement is a consequence of how difficult asynchronous I/O is in C,
//! and of the desire to keep the code as simple as possible.  Implementations
//! in other languages, or which uses proper asynchronous I/O libraries, SHOULD
//! NOT have this limitation.
//!
//! # Shared memory
//!
//! The Qubes GUI protocol uses inter-qube shared memory for all images.  This
//! shared memory is not sanitized in any way whatsoever, and may be modified
//! by the other side at any time without synchronization.  Therefore, all
//! access to the shared memory is `unsafe`.  Or rather, it *would* be unsafe,
//! were it not that no such access is required at all!  This avoids requiring
//! any form of signal handling, which is both `unsafe` and ugly.
//!
//! # Differences from the reference implementation
//!
//! The reference implementation of the GUI protocol considers the GUI daemon
//! (the server) to be trusted, while the GUI agent is not trusted.  As such,
//! the GUI agent blindly trusts the GUI daemon, while the GUI daemon must
//! carefully validate all data from the GUI agent.
//!
//! The Rust implementation takes a different view: *Both* the client and server
//! consider the other to be untrusted, and all messages are strictly validated.
//! This is necessary to meet Rust safety requirements, and also makes bugs in
//! the server easier to detect.
//!
//! Additionally, the Rust protocol definition is far, *far* better documented,
//! and explicitly lists each reference to the X11 protocol specification.  A
//! future release will not depend on the X11 protocol specification at all,
//! even for documentation.

#![forbid(missing_docs)]

macro_rules! enum_const {
    (
        #[repr($t: ident)]
        $(#[$i: meta])*
        $p: vis enum $n: ident {
            $(
                $(#[$j: meta])*
                ($const_name: ident, $variant_name: ident) $(= $e: expr)?
            ),*$(,)?
        }
    ) => {
        $(#[$i])*
        #[repr($t)]
        #[non_exhaustive]
        $p enum $n {
            $(
                $(#[$j])*
                $variant_name $(= $e)?,
            )*
        }

        $(
            $(#[$j])*
            $p const $const_name: $t = $n::$variant_name as $t;
        )*
    }
}

enum_const! {
    #[repr(u32)]
    /// Message types
    pub enum Msg {
        /// A key has been pressed
        (MSG_KEYPRESS, KeyPress) = 124,
        /// A button has been pressed
        (MSG_BUTTON, Button),
        /// Motion has happened
        (MSG_MOTION, Motion),
        /// Crossing
        (MSG_CROSSING, Crossing),
        /// Daemon ⇒ agent: A window has just acquired focus.
        (MSG_FOCUS, Focus),
        /// Daemon ⇒ agent, obsolete.
        (MSG_RESIZE, Resize),
        /// Agent ⇒ daemon: Creates a window.
        (MSG_CREATE, Create),
        /// Agent ⇒ daemon: Destroys a window.
        (MSG_DESTROY, Destroy),
        /// Bidirectional: Map a window.
        (MSG_MAP, Map),
        /// Agent ⇒ daemon: Unmap a window
        (MSG_UNMAP, Unmap) = 133,
        /// Bidirectional: Configure a window
        (MSG_CONFIGURE, Configure),
        /// Ask dom0 (only!) to map the given amount of memory into composition
        /// buffer.  Deprecated.
        (MSG_MFNDUMP, MfnDump),
        /// Agent ⇒ daemon: Redraw given area of screen.
        (MSG_SHMIMAGE, ShmImage),
        /// Daemon ⇒ agent: Request that a window be destroyed.
        (MSG_CLOSE, Close),
        /// Daemon ⇒ agent, deprecated, DO NOT USE
        (MSG_EXECUTE, Execute),
        /// Daemon ⇒ agent: Request clipboard data.
        (MSG_CLIPBOARD_REQ, ClipboardReq),
        /// Agent ⇒ daemon: Reply with clipboard data.
        (MSG_CLIPBOARD_DATA, ClipboardData),
        /// Agent ⇒ daemon: Set the title of a window.  Called MSG_WMNAME in C.
        (MSG_SET_TITLE, SetTitle),
        /// Daemon ⇒ agent: Update the keymap
        (MSG_KEYMAP_NOTIFY, KeymapNotify),
        /// Agent ⇒ daemon: Dock a window
        (MSG_DOCK, Dock) = 143,
        /// Agent ⇒ daemon: Set window manager hints.
        (MSG_WINDOW_HINTS, WindowHints),
        /// Agent ⇒ daemon: Set window manager flags.
        (MSG_WINDOW_FLAGS, WindowFlags),
        /// Agent ⇒ daemon: Set window class.
        (MSG_WINDOW_CLASS, WindowClass),
        /// Agent ⇒ daemon: Send shared memory dump
        (MSG_WINDOW_DUMP, WindowDump),
        /// Agent ⇒ daemon: Set cursor type
        (MSG_CURSOR, Cursor),
    }
}

/// Flags for [`WindowHints`].  These are a bitmask.
pub enum WindowHintsFlags {
    /// User-specified position
    USPosition = 1 << 0,
    /// Program-specified position
    PPosition = 1 << 2,
    /// Minimum size is valid
    PMinSize = 1 << 4,
    /// Maximum size is valid
    PMaxSize = 1 << 5,
    /// Resize increment is valid
    PResizeInc = 1 << 6,
    /// Base size is valid
    PBaseSize = 1 << 8,
}

qubes_castable::castable! {
    /// A GUI message as it appears on the wire.  All fields are in native byte
    /// order.
    pub struct GUIMessageHeader {
        /// Type of the message
        ty: u32,
        /// Window to which the message is directed.
        ///
        /// For all messages *except* CREATE, the window MUST exist.  For CREATE,
        /// the window MUST NOT exist.
        window: u32,
        /// UNTRUSTED length value.  The GUI agent MAY use this to skip unknown
        /// message.  The GUI daemon MUST NOT use this to calculate the message
        /// length without sanitizing it first.
        untrusted_len: u32,
    }

    /// X and Y coordinates relative to the top-left of the screen
    pub struct Coordinates {
        /// X coordinate in pixels
        x: u32,
        /// Y coordinate in pixels
        y: u32,
    }

    /// Window size
    pub struct WindowSize {
        /// Width in pixels
        width: u32,
        /// Height in pixels
        height: u32,
    }

    /// A (x, y, width, height) tuple
    pub struct Rectangle {
        /// Coordinates of the top left corner of the rectangle
        top_left: Coordinates,
        /// Size of the rectangle
        size: WindowSize
    }

    /// Metadata about a mapping
    pub struct MapInfo {
        /// The window that this is `transient_for`, or 0 if there is no such
        /// window.  The semantics of `transient_for` are defined in the X11
        /// ICCCM (Inter-Client Communication Conventions Manual).
        transient_for: u32,
        /// If this is 1, then this window (usually a menu) should not be
        /// managed by the window manager.  If this is 0, the window should be
        /// managed by the window manager.  All other values are invalid.  The
        /// semantics of this flag are the same as the X11 override_redirect
        /// flag, which this is implemented in terms of.
        override_redirect: u32,
    }

    /// Create a window
    pub struct Create {
        /// Rectangle the window is to occupy
        rectangle: Rectangle,
        /// Parent window.  This must exist.
        parent: u32,
        /// If this is 1, then this window (usually a menu) should not be
        /// managed by the window manager.  If this is 0, the window should be
        /// managed by the window manager.  All other values are invalid.
        override_redirect: u32,
    }

    /// Keypress
    pub struct Keypress {
        /// The X11 type of key pressed
        ty: u32,
        /// Coordinates of the key press
        coordinates: Coordinates,
        /// X11 key press state
        state: u32,
        /// X11 key code
        keycode: u32,
    }

    /// Button press
    pub struct Button {
        /// X11 event type
        ty: u32,
        /// Coordinates of the button press
        coordinates: Coordinates,
        /// X11 event state
        state: u32,
        /// X11 button number
        button: u32,
    }

    /// Motion event
    pub struct Motion {
        /// Coordinates of the motion event
        coordinates: Coordinates,
        /// X11 event state
        state: u32,
        /// X11 is_hint flag
        is_hint: u32,
    }

    /// Configure event
    pub struct Configure {
        /// Desired rectangle position and size
        rectangle: Rectangle,
        /// If this is 1, then this window (usually a menu) should not be
        /// managed by the window manager.  If this is 0, the window should be
        /// managed by the window manager.  All other values are invalid.
        override_redirect: u32,
    }

    /// Update the given region of the window from the contents of shared memory
    pub struct ShmImage {
        /// Rectangle to update
        rectangle: Rectangle,
    }

    /// Focus event from GUI qube
    pub struct Focus {
        /// The X11 event type
        ty: u32,
        /// The X11 event mode; MUST be 0.
        mode: u32,
        /// The X11 event detail
        detail: u32,
    }


    /// Root window configuration
    pub struct XConf {
        /// Root window size
        size: WindowSize,
        /// X11 Depth of the root window
        depth: u32,
        /// Memory (in KiB) required by the root window, with at least 1 byte to spare
        mem: u32,
    }

    /// Set the window name
    pub struct WMName {
        /// NUL-terminated name
        data: [u8; 128],
    }

    /// daemon ⇒ agent: Keymap change notification
    pub struct KeymapNotify {
        /// X11 keymap returned by XQueryKeymap()
        keys: [u8; 32],
    }

    /// agent ⇒ daemon: Set window hints
    pub struct WindowHints {
        /// Which elements are valid?
        flags: u32,
        /// Minimum size
        min_size: WindowSize,
        /// Maximum size
        max_size: WindowSize,
        /// Size increment
        size_increment: WindowSize,
        /// Base size
        size_base: WindowSize,
    }

    /// Bidirectional: Set window flags
    pub struct WindowFlags {
        /// Flags to set
        set: u32,
        /// Flags to unset
        unset: u32,
    }

    /// agent ⇒ daemon: map mfns, deprecated
    pub struct ShmCmd {
        /// ID of the shared memory segment.  Unused; SHOULD be 0.
        shmid: u32,
        /// Width of the rectangle to update
        width: u32,
        /// Height of the rectangle to update
        height: u32,
        /// Bits per pixel; MUST be 24
        bpp: u32,
        /// Offset from first page.  MUST be less than 4096.
        off: u32,
        /// Number of pages to map.  These follow this struct.
        num_mfn: u32,
        /// Source domain ID.  Unused; SHOULD be 0.
        domid: u32,
    }

    /// agent ⇒ daemon: set window class
    pub struct WMClass {
        /// Window class
        res_class: [u8; 64],
        /// Window name
        res_name: [u8; 64],
    }

    /// agent ⇒ daemon: Header of a window dump message
    pub struct WindowDumpHeader {
        /// Type of message
        ty: u32,
        /// Width in pixels
        width: u32,
        /// Height in pixels
        height: u32,
        /// Bits per pixel.  MUST be 24.
        bpp: u32,
    }
}
