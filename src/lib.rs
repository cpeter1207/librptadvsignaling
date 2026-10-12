//! Portable GPIO pulse and parallel signaling descriptor ABI.
//!
//! Pulse publication is lock-free. One service owner evaluates each handle and
//! supplies elapsed monotonic nanoseconds from that handle's clock origin.

#![deny(warnings)]
#![cfg_attr(coverage, feature(coverage_attribute))]

mod parallel;
mod pulse;

#[cfg(test)]
#[cfg_attr(coverage, coverage(off))]
mod tests;

use std::ffi::{c_char, c_void};

const ABI_VERSION: u32 = 1;
const CAPABILITY_NAME: &[u8] = b"rptadv.signaling\0";
const INVALID_ARGUMENT: i32 = -1;
const IO_ERROR: i32 = -5;

type Write = Option<extern "C" fn(*mut c_void, u8) -> i32>;
type Delay = Option<extern "C" fn(*mut c_void, u32)>;

/// Immutable version-one C-compatible operation table.
#[repr(C)]
pub struct SignalingDescriptor {
    struct_size: u32,
    abi_version: u32,
    capability_name: *const c_char,
    create_pulse: extern "C" fn() -> *mut pulse::Pulse,
    destroy_pulse: extern "C" fn(*mut pulse::Pulse),
    publish_timed: extern "C" fn(*const pulse::Pulse, u32, u32, bool) -> i32,
    publish_scheduled: extern "C" fn(*const pulse::Pulse, u32, u32, u32) -> i32,
    evaluate: extern "C" fn(*mut pulse::Pulse, u64, u32) -> u32,
    set_binary_channel: extern "C" fn(u8, u8, *mut c_void, Write) -> i32,
    program_rtx: extern "C" fn(u8, u32, u32, bool, bool, *mut c_void, Write, Delay) -> i32,
    clear_rtx_transmit: extern "C" fn(u8, *mut c_void, Write) -> i32,
}

// SAFETY: the table and pointed-to NUL-terminated static identity are immutable.
unsafe impl Sync for SignalingDescriptor {}

static DESCRIPTOR: SignalingDescriptor = SignalingDescriptor {
    struct_size: std::mem::size_of::<SignalingDescriptor>() as u32,
    abi_version: ABI_VERSION,
    capability_name: CAPABILITY_NAME.as_ptr().cast(),
    create_pulse: pulse::create,
    destroy_pulse: pulse::destroy,
    publish_timed: pulse::publish_timed,
    publish_scheduled: pulse::publish_scheduled,
    evaluate: pulse::evaluate,
    set_binary_channel: parallel::set_binary_channel,
    program_rtx: parallel::program_rtx,
    clear_rtx_transmit: parallel::clear_rtx_transmit,
};

/// Return the immutable descriptor; callers validate identity, size and ABI.
#[unsafe(no_mangle)]
pub extern "C" fn rptadv_signaling_descriptor() -> *const SignalingDescriptor {
    &DESCRIPTOR
}
