/*
 * The Qubes OS Project, http://www.qubes-os.org
 *
 * Copyright (C) 2010  Rafal Wojtczuk  <rafal@invisiblethingslab.com>
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
 *
 */

//! # Rust bindings to, and specification of, the Qubes OS GUI Protocol (QOGP).
//!
//! ## Transport and Terminology
//!
//! The Qubes OS GUI protocol is spoken over a vchan between two virtual
//! machines (VMs).  The VM providing GUI services is the client of this vchan,
//! while the VM that wishes to display its GUI is the server.  The component
//! that provides GUI services to other VMs is known as the *GUI daemon*, and
//! the component that the GUI daemon connects to is known as the *GUI agent*.
//!
//! ## Message format
//!
//! Each message is a C struct that is cast to a byte slice and sent
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
//! the server MAY use blocking I/O over the vchan.  The client MUST NOT block
//! on the server, to avoid deadlocks.  Therefore, the client should buffer its
//! messages and flush them at every opportunity.  This requirement is a
//! consequence of how difficult asynchronous I/O is in C, and of the desire to
//! keep the code as simple as possible.  Implementations in other languages, or
//! which uses proper asynchronous I/O libraries, SHOULD NOT have this
//! limitation.
//!
//! ## Window IDs
//!
//! The Qubes OS GUI protocol refers to each surface by a 32-bit unsigned window
//! ID.  Zero is reserved and means “no window”.  For instance, using zero for a
//! window’s parent means that the window does not have a parent.  Otherwise,
//! agents are free to choose any window ID they wish.  In particular, while X11
//! limits IDs to a maximum of 2²⁹ - 1, the Qubes OS GUI protocol imposes no
//! such restriction.
//!
//! It is a protocol error for an agent to send a message to a window that does
//! not exist, including a window which it has deleted.  Because of unavoidable
//! race conditions, agents may recieve events for windows they have already
//! destroyed.  Such messages MUST be ignored.
//!
//! ## Unrecognized messages
//!
//! GUI daemons MUST treat messages with an unknown type as a protocol error.
//! GUI agents MAY log the headers of such messages and MUST otherwise ignore
//! them.  The bodies of such messages MUST NOT be logged as they may contain
//! sensitive data.
//!
//! ## Shared memory
//!
//! The Qubes GUI protocol uses inter-qube shared memory for all images.  This
//! shared memory is not sanitized in any way whatsoever, and may be modified
//! by the other side at any time without synchronization.  Therefore, all
//! access to the shared memory is `unsafe`.  Or rather, it *would* be unsafe,
//! were it not that no such access is required at all!  This avoids requiring
//! any form of signal handling, which is both `unsafe` and ugly.
//!
//! ## Differences from the reference implementation
//!
//! The reference implementation of the GUI protocol considers the GUI daemon
//! (the server) to be trusted, while the GUI agent is not trusted.  As such,
//! the GUI agent blindly trusts the GUI daemon, while the GUI daemon must
//! carefully validate all data from the GUI agent.
//!
//! This Rust implementation takes a different view: *Both* the client and server
//! consider the other to be untrusted, and all messages are strictly validated.
//! This is necessary to meet Rust safety requirements, and also makes bugs in
//! the server easier to detect.
//!
//! Additionally, the Rust protocol definition is far, *far* better documented,
//! and explicitly lists each reference to the X11 protocol specification.  A
//! future release will not depend on the X11 protocol specification at all,
//! even for documentation.

#![forbid(missing_docs)]
#![no_std]
use core::convert::TryFrom;
use core::num::NonZeroU32;
use core::result::Result;

/// Arbitrary maximum size of a clipboard message
pub const MAX_CLIPBOARD_SIZE: u32 = 65000;

/// Arbitrary max window height
pub const MAX_WINDOW_HEIGHT: u32 = 6144;

/// Arbitrary max window width
pub const MAX_WINDOW_WIDTH: u32 = 16384;

/// Default cursor ID.
pub const CURSOR_DEFAULT: u32 = 0;

/// Flag that must be set to request an X11 cursor
pub const CURSOR_X11: u32 = 0x100;

/// Max X11 cursor that can be requested
pub const CURSOR_X11_MAX: u32 = 0x19a;

/// Bits-per-pixel of the dummy X11 framebuffer driver
pub const DUMMY_DRV_FB_BPP: u32 = 32;

/// Maximum size of a shared memory segment, in bytes
pub const MAX_WINDOW_MEM: u32 = MAX_WINDOW_WIDTH * MAX_WINDOW_HEIGHT * (DUMMY_DRV_FB_BPP / 8);

/// Number of bytes in a shared page
pub const XC_PAGE_SIZE: u32 = 1 << 12;

/// Maximum permissable number of shared memory pages in a single segment using
/// deprecated privcmd-based shared memory
pub const MAX_MFN_COUNT: u32 = (MAX_WINDOW_MEM + XC_PAGE_SIZE - 1) >> 12;

/// Maximum permissable number of shared memory pages in a single segment using
/// grant tables
pub const MAX_GRANT_REFS_COUNT: u32 = (MAX_WINDOW_MEM + XC_PAGE_SIZE - 1) >> 12;

/// GUI agent listening port
pub const LISTENING_PORT: i16 = 6000;

/// Type of grant refs dump messages
pub const WINDOW_DUMP_TYPE_GRANT_REFS: u32 = 0;

// This allows pattern-matching against constant values without a huge amount of
// boilerplate code.
macro_rules! enum_const {
    (
        #[repr($t: ty)]
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

        impl $crate::TryFrom::<$t> for $n {
            type Error = $t;
            #[allow(non_upper_case_globals)]
            #[inline]
            fn try_from(value: $t) -> $crate::Result<Self, $t> {
                match value {
                    $(
                        $const_name => return $crate::Result::Ok($n::$variant_name),
                    )*
                    other => $crate::Result::Err(other),
                }
            }
        }
    }
}

enum_const! {
    #[repr(u32)]
    /// Message types
    pub enum Msg {
        /// Daemon ⇒ agent: A key has been pressed
        (MSG_KEYPRESS, Keypress) = 124,
        /// Daemon ⇒ agent: A button has been pressed
        (MSG_BUTTON, Button),
        /// Daemon ⇒ agent: Pointer has moved.
        (MSG_MOTION, Motion),
        /// Daemon ⇒ agent: Pointer has crossed edge of window.
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
        /// Bidirectional: Clipboard data
        (MSG_CLIPBOARD_DATA, ClipboardData),
        /// Agent ⇒ daemon: Set the title of a window.  Called MSG_WMNAME in C.
        (MSG_SET_TITLE, SetTitle),
        /// Daemon ⇒ agent: Update the keymap
        (MSG_KEYMAP_NOTIFY, KeymapNotify),
        /// Agent ⇒ daemon: Dock a window
        (MSG_DOCK, Dock) = 143,
        /// Agent ⇒ daemon: Set window manager hints.
        (MSG_WINDOW_HINTS, WindowHints),
        /// Bidirectional: Set window manager flags.
        (MSG_WINDOW_FLAGS, WindowFlags),
        /// Agent ⇒ daemon: Set window class.
        (MSG_WINDOW_CLASS, WindowClass),
        /// Agent ⇒ daemon: Send shared memory dump
        (MSG_WINDOW_DUMP, WindowDump),
        /// Agent ⇒ daemon: Set cursor type
        (MSG_CURSOR, Cursor),
    }
}

enum_const! {
    #[repr(u32)]
    /// State of a button
    pub enum ButtonEvent {
        /// A button has been pressed
        (EV_BUTTON_PRESS, Press) = 4,
        /// A button has been released
        (EV_BUTTON_RELEASE, Release) = 5,
    }
}

enum_const! {
    #[repr(u32)]
    /// Key change event
    pub enum KeyEvent {
        /// The key was pressed
        (EV_KEY_PRESS, Press) = 2,
        /// The key was released
        (EV_KEY_RELEASE, Release) = 3,
    }
}

enum_const! {
    #[repr(u32)]
    /// Focus change event
    pub enum FocusEvent {
        /// The window now has focus
        (EV_FOCUS_IN, In) = 9,
        /// The window has lost focus
        (EV_FOCUS_OUT, Out) = 10,
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

/// Flags for [`WindowFlags`].  These are a bitmask.
pub enum WindowFlag {
    /// Fullscreen request.  This may or may not be honored.
    Fullscreen = 1 << 0,
    /// Demands attention
    DemandsAttention = 1 << 1,
    /// Minimize
    Minimize = 1 << 2,
}

/// Trait for Qubes GUI structs, specifying the message number.
pub trait Message: qubes_castable::Castable + core::default::Default {
    /// The kind of the message
    const KIND: Msg;
}

qubes_castable::castable! {
    /// A GUI message as it appears on the wire.  All fields are in native byte
    /// order.
    pub struct Header {
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

    /// Daemon ⇒ agent: Root window configuration; sent only at startup, without
    /// a header.
    pub struct XConf {
        /// Root window size
        size: WindowSize,
        /// X11 Depth of the root window
        depth: u32,
        /// Memory (in KiB) required by the root window, with at least 1 byte to spare
        mem: u32,
    }

    /// Bidirectional: Metadata about a mapping
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

    /// Agent ⇒ daemon: Create a window.  This should always be followed by a
    /// [`Configure`] message.  The window is not immediately mapped.
    pub struct Create {
        /// Rectangle the window is to occupy.  It is a protocol error for the
        /// width or height to be zero, for the width to exceed
        /// [`MAX_WINDOW_WIDTH`], or for the height to exceed [`MAX_WINDOW_HEIGHT`].
        rectangle: Rectangle,
        /// Parent window, or [`None`] if there is no parent window.  It is a
        /// protocol error to specify a parent window that does not exist.  The
        /// parent window (or lack theirof) cannot be changed after a window has
        /// been created.
        parent: Option<NonZeroU32>,
        /// If this is 1, then this window (usually a menu) should not be
        /// managed by the window manager.  If this is 0, the window should be
        /// managed by the window manager.  All other values are invalid.
        override_redirect: u32,
    }

    /// Daemon ⇒ agent: Keypress
    pub struct Keypress {
        /// The X11 type of key pressed.  MUST be 2 ([`EV_KEY_PRESS`]) or 3
        /// ([`EV_KEY_RELEASE`]).  Anything else is a protocol violation.
        ty: u32,
        /// Coordinates of the key press
        coordinates: Coordinates,
        /// X11 key press state
        state: u32,
        /// X11 key code
        keycode: u32,
    }

    /// Daemon ⇒ agent: Button press
    pub struct Button {
        /// The type of event.  MUST be 4 ([`EV_BUTTON_PRESS`]) or 5
        /// ([`EV_BUTTON_RELEASE`]).  Anything else is a protocol violation.
        ty: u32,
        /// Coordinates of the button press
        coordinates: Coordinates,
        /// Bitmask of modifier keys
        state: u32,
        /// X11 button number
        button: u32,
    }

    /// Daemon ⇒ agent: Motion event
    pub struct Motion {
        /// Coordinates of the motion event
        coordinates: Coordinates,
        /// Bitmask of buttons that are pressed
        state: u32,
        /// X11 is_hint flag
        is_hint: u32,
    }

    /// Daemon ⇒ agent: Crossing event
    pub struct Crossing {
        /// Type of the crossing
        ty: u32,
        /// Coordinates of the crossing
        coordinates: Coordinates,
        /// X11 state of the crossing
        state: u32,
        /// X11 mode of the crossing
        mode: u32,
        /// X11 detail of the crossing
        detail: u32,
        /// X11 focus of the crossing
        focus: u32,
    }

    /// Bidirectional: Configure event
    pub struct Configure {
        /// Desired rectangle position and size
        rectangle: Rectangle,
        /// If this is 1, then this window (usually a menu) should not be
        /// managed by the window manager.  If this is 0, the window should be
        /// managed by the window manager.  All other values are invalid.
        override_redirect: u32,
    }

    /// Agent ⇒ daemon: Update the given region of the window from the contents of shared memory
    pub struct ShmImage {
        /// Rectangle to update
        rectangle: Rectangle,
    }

    /// Daemon ⇒ agent: Focus event from GUI qube
    pub struct Focus {
        /// The type of event.  MUST be 9 ([`EV_FOCUS_IN`]) or 10
        /// ([`EV_FOCUS_OUT`]).  Anything else is a protocol error.
        ty: u32,
        /// The X11 event mode.  This is not used in the Qubes GUI protocol.
        /// Daemons MUST set this to 0 to avoid information leaks.  Agents MAY
        /// consider nonzero values to be a protocol error.
        mode: u32,
        /// The X11 event detail.  MUST be between 0 and 7 inclusive.
        detail: u32,
    }

    /// Agent ⇒ daemon: Set the window name
    pub struct WMName {
        /// NUL-terminated name
        data: [u8; 128],
    }

    /// Agent ⇒ daemon: Unmap the window.  Unmapping a window that is not
    /// currently mapped has no effect.
    pub struct Unmap {}

    /// Agent ⇒ daemon: Dock the window.  Docking an already-docked window has
    /// no effect.
    pub struct Dock {}

    /// Agent ⇒ daemon: Destroy the window.  The agent SHOULD NOT reuse the
    /// window ID for as long as possible to make races less likely.
    pub struct Destroy {}

    /// Daemon ⇒ agent: Keymap change notification
    pub struct KeymapNotify {
        /// X11 keymap returned by XQueryKeymap()
        keys: [u8; 32],
    }

    /// Agent ⇒ daemon: Set window hints
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

    /// Agent ⇒ daemon: map mfns, deprecated
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

    /// Agent ⇒ daemon: set window class
    pub struct WMClass {
        /// Window class
        res_class: [u8; 64],
        /// Window name
        res_name: [u8; 64],
    }

    /// Agent ⇒ daemon: Header of a window dump message
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

    /// Agent ⇒ daemon: Header of a window dump message
    pub struct Cursor {
        /// Type of cursor
        cursor: u32,
    }
}

macro_rules! impl_message {
    ($(($t: ty, $kind: expr),)+) => {
        $(impl Message for $t {
            const KIND: Msg = $kind;
        })+
    }
}

impl_message! {
    (MapInfo, Msg::Map),
    (Create, Msg::Create),
    (Keypress, Msg::Keypress),
    (Button, Msg::Button),
    (Motion, Msg::Motion),
    (Crossing, Msg::Crossing),
    (Configure, Msg::Configure),
    (ShmImage, Msg::ShmImage),
    (Focus, Msg::Focus),
    (WMName, Msg::SetTitle),
    (KeymapNotify, Msg::KeymapNotify),
    (WindowHints, Msg::WindowHints),
    (WindowFlags, Msg::WindowFlags),
    (ShmCmd, Msg::ShmImage),
    (WMClass, Msg::WindowClass),
    (WindowDumpHeader, Msg::WindowDump),
    (Cursor, Msg::Cursor),
    (Destroy, Msg::Destroy),
    (Dock, Msg::Dock),
    (Unmap, Msg::Unmap),
}

/// Gets the length limits of a message of a given type, or `None` for an
/// unknown message (for which there are no limits).
pub fn msg_length_limits(ty: u32) -> Option<core::ops::RangeInclusive<usize>> {
    use core::mem::size_of;
    Some(match Msg::try_from(ty).ok()? {
        Msg::ClipboardData => 0..=MAX_CLIPBOARD_SIZE as _,
        Msg::Button => size_of::<Button>()..=size_of::<Button>(),
        Msg::Keypress => size_of::<Keypress>()..=size_of::<Keypress>(),
        Msg::Motion => size_of::<Motion>()..=size_of::<Motion>(),
        Msg::Crossing => size_of::<Crossing>()..=size_of::<Crossing>(),
        Msg::Focus => size_of::<Focus>()..=size_of::<Focus>(),
        Msg::Create => size_of::<Create>()..=size_of::<Create>(),
        Msg::Destroy => 0..=0,
        Msg::Map => size_of::<MapInfo>()..=size_of::<MapInfo>(),
        Msg::Unmap => 0..=0,
        Msg::Configure => size_of::<Configure>()..=size_of::<Configure>(),
        Msg::MfnDump => 0..=4 * MAX_MFN_COUNT as usize,
        Msg::ShmImage => size_of::<ShmImage>()..=size_of::<ShmImage>(),
        Msg::Close => 0..=0,
        Msg::ClipboardReq => 0..=0,
        Msg::SetTitle => size_of::<WMName>()..=size_of::<WMName>(),
        Msg::KeymapNotify => size_of::<KeymapNotify>()..=size_of::<KeymapNotify>(),
        Msg::Dock => 0..=0,
        Msg::WindowHints => size_of::<WindowHints>()..=size_of::<KeymapNotify>(),
        Msg::WindowFlags => size_of::<WindowFlags>()..=size_of::<WindowFlags>(),
        Msg::WindowClass => size_of::<WMClass>()..=size_of::<WMClass>(),
        Msg::WindowDump => {
            size_of::<WindowDumpHeader>()
                ..=size_of::<WindowDumpHeader>() + size_of::<u32>() * MAX_GRANT_REFS_COUNT as usize
        }
        Msg::Cursor => size_of::<Cursor>()..=size_of::<Cursor>(),
        Msg::Execute | Msg::Resize => return None,
    })
}
