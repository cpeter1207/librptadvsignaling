//! Binary-channel and RTX sequencing moved from rptadv-gpio-adapter/src/parallel.rs
//! at 3e662f9be4f6b9731cc4380165afe998e3fda642.

use super::{Delay, INVALID_ARGUMENT, IO_ERROR, Write};
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};

const RTX_REGISTER_BITS: u32 = 20;
const RTX_CLOCK_MASK: u8 = 0x01;
const RTX_DATA_MASK: u8 = 0x02;
const RTX_ENABLE_MASK: u8 = 0x04;
const RTX_TRANSMIT_MASK: u8 = 0x08;
const RTX_TRANSMIT_POWER_MASK: u8 = 0x10;
const RTX_CONTROL_MASK: u8 =
    RTX_CLOCK_MASK | RTX_DATA_MASK | RTX_ENABLE_MASK | RTX_TRANSMIT_MASK | RTX_TRANSMIT_POWER_MASK;
const BINARY_CHANNEL_MASK: u8 = 0xf0;
static RTX_SERIAL_INITIALIZED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy)]
pub(super) struct RtxRequest {
    pub output: u8,
    pub rx_frequency_hz: u32,
    pub tx_frequency_hz: u32,
    pub transmitting: bool,
    pub high_power: bool,
}

/// The established reference-divider and synthesizer words.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RtxWords {
    reference: u32,
    synthesizer: u32,
}

fn rtx_words(rx_frequency_hz: u32, tx_frequency_hz: u32, transmitting: bool) -> RtxWords {
    let (reference_frequency_hz, step_frequency_hz, receiver_if_frequency_hz) =
        if rx_frequency_hz > 200_000_000 {
            (16_012_500, 12_500, 21_400_000)
        } else {
            (16_000_000, 5_000, 10_700_000)
        };
    let synthesizer_frequency_hz = if transmitting {
        tx_frequency_hz
    } else {
        rx_frequency_hz.wrapping_sub(receiver_if_frequency_hz)
    };
    let word = (synthesizer_frequency_hz / step_frequency_hz).wrapping_shl(1);
    RtxWords {
        reference: (reference_frequency_hz / step_frequency_hz).wrapping_shl(1) | 1,
        synthesizer: (word & 0xffff_ff80)
            .wrapping_shl(1)
            .wrapping_add(word & 0x7f),
    }
}

fn rtx_initial_delay_multiplier_for(initialized: &AtomicBool) -> u32 {
    if initialized.swap(true, Ordering::AcqRel) {
        4
    } else {
        200
    }
}

fn rtx_shift_word(
    output: &mut u8,
    word: u32,
    initial_delay_multiplier: u32,
    write: &mut impl FnMut(u8) -> Result<(), i32>,
    delay: &mut impl FnMut(u32),
) -> Result<(), i32> {
    *output &= !RTX_CONTROL_MASK;
    write(*output)?;
    delay(initial_delay_multiplier);
    for bit_index in (0..RTX_REGISTER_BITS).rev() {
        let bit = 1_u32 << bit_index;
        if word & bit != 0 {
            *output |= RTX_DATA_MASK;
        } else {
            *output &= !RTX_DATA_MASK;
        }
        write(*output)?;
        delay(1);
        *output |= RTX_CLOCK_MASK;
        write(*output)?;
        delay(1);
        *output &= !RTX_CLOCK_MASK;
        write(*output)?;
        delay(1);
    }
    *output &= !(RTX_CLOCK_MASK | RTX_DATA_MASK);
    write(*output)?;
    *output |= RTX_ENABLE_MASK;
    write(*output)?;
    delay(1);
    *output &= !RTX_ENABLE_MASK;
    write(*output)
}

fn write_byte(context: *mut c_void, write: Write, value: u8) -> Result<(), i32> {
    if write.is_some_and(|callback| callback(context, value) == 0) {
        Ok(())
    } else {
        Err(IO_ERROR)
    }
}

/// Latch an active-low four-bit binary channel. Pin ownership is adapter policy.
pub(super) extern "C" fn set_binary_channel(
    output: u8,
    channel: u8,
    context: *mut c_void,
    write: Write,
) -> i32 {
    if write.is_none() {
        return INVALID_ARGUMENT;
    }
    let mut output = output | BINARY_CHANNEL_MASK;
    if write_byte(context, write, output).is_err() {
        return IO_ERROR;
    }
    output &= !channel.wrapping_shl(4);
    if write_byte(context, write, output).is_err() {
        return IO_ERROR;
    }
    0
}

/// Program both RTX words and final TX selection using adapter-owned callbacks.
pub(super) extern "C" fn program_rtx(
    output: u8,
    rx_frequency_hz: u32,
    tx_frequency_hz: u32,
    transmitting: bool,
    high_power: bool,
    context: *mut c_void,
    write: Write,
    delay: Delay,
) -> i32 {
    program_rtx_with_state(
        &RTX_SERIAL_INITIALIZED,
        RtxRequest {
            output,
            rx_frequency_hz,
            tx_frequency_hz,
            transmitting,
            high_power,
        },
        context,
        write,
        delay,
    )
}

pub(super) fn program_rtx_with_state(
    initialized: &AtomicBool,
    request: RtxRequest,
    context: *mut c_void,
    write: Write,
    delay: Delay,
) -> i32 {
    if request.rx_frequency_hz == 0 {
        return 0;
    }
    let (Some(write), Some(delay)) = (write, delay) else {
        return INVALID_ARGUMENT;
    };
    // The legacy protocol accepts this request but deliberately holds TXPWR low.
    let _ = request.high_power;
    let words = rtx_words(
        request.rx_frequency_hz,
        request.tx_frequency_hz,
        request.transmitting,
    );
    let mut output = request.output;
    let mut write = |value| write_byte(context, Some(write), value);
    let mut delay = |multiplier| delay(context, multiplier);
    if rtx_shift_word(
        &mut output,
        words.reference,
        rtx_initial_delay_multiplier_for(initialized),
        &mut write,
        &mut delay,
    )
    .is_err()
        || rtx_shift_word(
            &mut output,
            words.synthesizer,
            rtx_initial_delay_multiplier_for(initialized),
            &mut write,
            &mut delay,
        )
        .is_err()
    {
        return IO_ERROR;
    }
    output &= !(RTX_CLOCK_MASK | RTX_DATA_MASK | RTX_ENABLE_MASK);
    if request.transmitting {
        output &= !RTX_TRANSMIT_POWER_MASK;
        output |= RTX_TRANSMIT_MASK;
    } else {
        output &= !(RTX_TRANSMIT_MASK | RTX_TRANSMIT_POWER_MASK);
    }
    if write(output).is_ok() { 0 } else { IO_ERROR }
}

/// Clear only RTX transmit and power bits without serial traffic.
pub(super) extern "C" fn clear_rtx_transmit(output: u8, context: *mut c_void, write: Write) -> i32 {
    if write.is_none() {
        return INVALID_ARGUMENT;
    }
    let output = output & !(RTX_TRANSMIT_MASK | RTX_TRANSMIT_POWER_MASK);
    if write_byte(context, write, output).is_ok() {
        0
    } else {
        IO_ERROR
    }
}
