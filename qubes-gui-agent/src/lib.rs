/*
 * The Qubes OS Project, http://www.qubes-os.org
 *
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

//! High-level, agent-side bindings to the Qubes OS GUI Protocol.
//!
//! This provides a high-level API that is intended for direct consumption by
//! applications.  It relies on several other crates:
//!
//! - `qubes-gui` provides the protocol definition
//! - `qubes-gui-agent-proto` provides message decoding
//! - `qubes-gui-client` provides vchan handling
//! - `qubes-gui-gntalloc` manages shared memory
//!
//! In turn, this provides several useful abstractions:
//!
//! - An [`Agent`] struct with support for creating and destroying windows and
//!   buffers.
//! - Windows are represented as the [`Window`] struct, which manages the
//!   lifecycle of windows.  Window ID management is handled by the library and
//!   is transparent to the user.  Creating a window automatically sends a
//!   [`qubes_gui::Create`] message to the GUI daemon, and destroying one
//!   automatically sends a [`qubes_gui::Destroy`] message.
//! - When a window is destroyed, the child windows are automatically destroyed
//!   too.
//! - Messages are sent via methods on [`Window`] objects, instead of having to
//!   encode messages manually.  This leads to much cleaner code and prevents
//!   entire classes of bugs.  In particular, sending a message on an invalid
//!   window is prevented by the borrow checker.
//! - The common task of managing trees of windows is solved by the
//!   [`WindowTree`] abstraction.  This allows detaching a window from its
//!   parent, even though the Qubes OS GUI Protocol does not itself support
//!   this.  To work around this limitation, child windows are destroyed
//!   recursively, and then recreated.
//! - A [`Buffer`] abstraction provides for managed buffers.  Double-buffering
//!   is supported natively, making graphical glitches far, *far* less likely.
//!   Future improvements to the GUI protocol will make glitches impossible.
//! - Events are delivered as strongly typed enums, rather than as byte slices.
//!   Knowledge of the wire format is not required to interpret them.
