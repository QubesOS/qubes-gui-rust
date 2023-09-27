# Rust libraries for the Qubes OS GUI Protocol

This provides Rust libraries for the Qubes OS GUI Protocol.  While the existing
[agent] and [daemon] have served Qubes OS users well, they are monolithic C
codebases and are not suitable for use as libraries.  Additionally, they are
very much tied to the X Window System, whereas the future of FLOSS graphics is
Wayland.  Finally, the Qubes OS GUI protocol is underspecified, so these
libraries aim to provide a definition of the protocol that is independent of any
particular implementation.

## Organization

To maximize portability and build-time parallelism, and to prevent circular
dependencies, these libraries are broken up into several different crates.  Each
crate serves a specific purpose, and some are reusable outside of Qubes OS.

### Qubes-Castable

qubes-castable is a core crate that provides support for _castable_ structs ―
that is, structs that can safely be converted to a byte slice.  It is not
specific to Qubes OS in any way.  Unlike the rest of the code, this crate is
dual-licensed under the MIT License and the Apache License, Version 2.0.  This
is the same license used by the Rust Programming Language itself.  It is
`#[no_std]` and depends only on libcore, so it can be used anywhere.

### qubes-gui

This provides the definition of the Qubes OS GUI Protocol.  It is designed to
stand alone, independently of the C definition and the X Window System.  As
such, it is mostly comments and type definitions.  Unlike `qubes-castable`,
and like the rest of this repository, it is licensed under the GNU General
Public License, version 2.0, or (at your option) any later version.  It, too, is
`#[no_std]` with no dependencies beyond libcore.

### qubes-gui-agent-proto

This small `#[no_std]` crate provides message parsing support for GUI agents.
See its documentation for details.

### qubes-gui-daemon-proto (not yet written)

This small `#[no_std]` crate provides message parsing support for GUI daemons.
See its documentation for details.

### vchan-sys

This provides raw, unsafe Rust bindings to the C libvchan library.  It is not
intended to be used directly; most code should use the safe bindings in the
`vchan` crate.  It relies on the Rust standard library, but this requirement
can be lifted without too much difficulty.

### vchan

This is a safe wrapper around `vchan-sys`.  It relies on the Rust standard
library, especially for traits such as `Read` and `Write`.

### qubes-gui-connection

This crate provides support for non-blocking I/O with the GUI daemon.  It
implements a simple state machine for message parsing, and provides buffering
of outgoing messages to prevent deadlocks.  Currently, this buffer is not
bounded, but that will change in the future.

### qubes-demo-agent

This is a demo GUI agent.  It just draws a single resizable window and logs
events that it receives.
