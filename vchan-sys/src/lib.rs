/*
 * The Qubes OS Project, https://www.qubes-os.org
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
#![forbid(clippy::all)]

#[repr(C)]
#[allow(non_camel_case_types)]
pub struct libvchan_t {
    _unused: [u8; 0],
}
use std::os::raw::{c_int, c_void};

/* return values from libvchan_is_open */
/* remote disconnected or remote domain dead */
pub const VCHAN_DISCONNECTED: c_int = 0;
/* connected */
pub const VCHAN_CONNECTED: c_int = 1;
/* vchan server initialized, waiting for client to connect */
pub const VCHAN_WAITING: c_int = 2;

#[link(name = "vchan-xen")]
extern "C" {
    pub fn libvchan_server_init(
        domain: c_int,
        port: c_int,
        read_min: usize,
        write_min: usize,
    ) -> *mut libvchan_t;
    pub fn libvchan_client_init(domain: c_int, port: c_int) -> *mut libvchan_t;
    pub fn libvchan_write(ctrl: *mut libvchan_t, data: *const c_void, size: usize) -> c_int;
    pub fn libvchan_send(ctrl: *mut libvchan_t, data: *const c_void, size: usize) -> c_int;
    pub fn libvchan_read(ctrl: *mut libvchan_t, data: *mut c_void, size: usize) -> c_int;
    pub fn libvchan_recv(ctrl: *mut libvchan_t, data: *mut c_void, size: usize) -> c_int;
    pub fn libvchan_wait(ctrl: *mut libvchan_t) -> c_int;
    pub fn libvchan_close(ctrl: *mut libvchan_t);
    pub fn libvchan_fd_for_select(ctrl: *const libvchan_t) -> c_int;
    pub fn libvchan_is_open(ctrl: *const libvchan_t) -> c_int;
    pub fn libvchan_data_ready(ctrl: *const libvchan_t) -> c_int;
    pub fn libvchan_buffer_space(ctrl: *const libvchan_t) -> c_int;
}
