use super::*;
use std::ffi::c_void;

#[derive(Debug, PartialEq)]
enum Event {
    Write(u8),
    Delay(u32),
}

struct Trace {
    events: Vec<Event>,
    fail_at: Option<usize>,
    writes: usize,
}

extern "C" fn write(context: *mut c_void, value: u8) -> i32 {
    let trace = unsafe { &mut *context.cast::<Trace>() };
    let index = trace.writes;
    trace.writes += 1;
    if trace.fail_at == Some(index) {
        return -91;
    }
    trace.events.push(Event::Write(value));
    0
}

extern "C" fn delay(context: *mut c_void, multiplier: u32) {
    unsafe { &mut *context.cast::<Trace>() }
        .events
        .push(Event::Delay(multiplier));
}

#[test]
fn shared_signaling_matches_transport_trace() {
    let api = unsafe { &*rptadv_signaling_descriptor() };
    assert_eq!(
        api.struct_size as usize,
        std::mem::size_of::<SignalingDescriptor>()
    );
    assert_eq!(api.abi_version, 1);
    assert_eq!(
        unsafe { std::ffi::CStr::from_ptr(api.capability_name) }.to_bytes(),
        b"rptadv.signaling"
    );

    let pulse = (api.create_pulse)();
    assert!(!pulse.is_null());
    assert_eq!((api.publish_timed)(pulse, 1, 10, true), 0);
    assert_eq!((api.evaluate)(pulse, 5_000_000, 0), 1);
    assert_eq!((api.publish_scheduled)(pulse, 0x100, 0, 20), 0);
    assert_eq!((api.evaluate)(pulse, 5_500_000, 0), 0x101);
    assert_eq!((api.evaluate)(pulse, 15_000_000, 0), 0x100);
    assert_eq!((api.evaluate)(pulse, 25_500_000, 0), 0);
    (api.destroy_pulse)(pulse);

    let mut trace = Trace {
        events: Vec::new(),
        fail_at: None,
        writes: 0,
    };
    let context = (&mut trace as *mut Trace).cast::<c_void>();
    assert_eq!((api.set_binary_channel)(0x0f, 5, context, Some(write)), 0);
    assert_eq!(trace.events, [Event::Write(0xff), Event::Write(0xaf)]);
    trace.events.clear();
    trace.writes = 0;
    assert_eq!((api.clear_rtx_transmit)(0xff, context, Some(write)), 0);
    assert_eq!(trace.events, [Event::Write(0xe7)]);
    trace.events.clear();
    trace.writes = 0;
    let state = std::sync::atomic::AtomicBool::new(false);
    assert_eq!(
        parallel::program_rtx_with_state(
            &state,
            parallel::RtxRequest {
                output: 0xe0,
                rx_frequency_hz: 146_940_000,
                tx_frequency_hz: 146_340_000,
                transmitting: true,
                high_power: false
            },
            context,
            Some(write),
            Some(delay)
        ),
        0
    );
    assert_eq!(trace.writes, 129);
    assert_eq!(trace.events[1], Event::Delay(200));
    assert_eq!(trace.events.last(), Some(&Event::Write(0xe8)));
}

#[test]
fn pulse_publication_replaces_same_bit_but_preserves_disjoint_bit() {
    let api = unsafe { &*rptadv_signaling_descriptor() };
    let pulse = (api.create_pulse)();
    assert_eq!((api.publish_scheduled)(pulse, 1, 0, 10), 0);
    assert_eq!((api.publish_scheduled)(pulse, 2, 0, 20), 0);
    assert_eq!((api.evaluate)(pulse, 100, 0), 3);
    assert_eq!((api.publish_scheduled)(pulse, 1, 0, 5), 0);
    assert_eq!((api.evaluate)(pulse, 500, 0), 3);
    assert_eq!((api.evaluate)(pulse, 5_000_499, 0), 3);
    assert_eq!((api.evaluate)(pulse, 5_000_500, 0), 2);
    assert_eq!((api.publish_scheduled)(pulse, 0, 2, 0), 0);
    assert_eq!((api.evaluate)(pulse, 5_000_501, 0), 0);
    (api.destroy_pulse)(pulse);
}

#[test]
fn rtx_program_uses_long_first_delay_and_stops_on_failed_write() {
    let state = std::sync::atomic::AtomicBool::new(false);
    let mut trace = Trace {
        events: Vec::new(),
        fail_at: None,
        writes: 0,
    };
    let context = (&mut trace as *mut Trace).cast::<c_void>();
    assert_eq!(
        parallel::program_rtx_with_state(
            &state,
            parallel::RtxRequest {
                output: 0xe0,
                rx_frequency_hz: 146_940_000,
                tx_frequency_hz: 146_340_000,
                transmitting: true,
                high_power: true
            },
            context,
            Some(write),
            Some(delay)
        ),
        0
    );
    assert_eq!(trace.writes, 129);
    assert_eq!(trace.events[0], Event::Write(0xe0));
    assert_eq!(trace.events[1], Event::Delay(200));
    assert_eq!(trace.events.last(), Some(&Event::Write(0xe8)));
    trace.events.clear();
    trace.writes = 0;
    trace.fail_at = Some(64);
    assert_eq!(
        parallel::program_rtx_with_state(
            &state,
            parallel::RtxRequest {
                output: 0xe0,
                rx_frequency_hz: 146_940_000,
                tx_frequency_hz: 146_340_000,
                transmitting: true,
                high_power: false
            },
            context,
            Some(write),
            Some(delay)
        ),
        -5
    );
    assert_eq!(trace.writes, 65);
    assert_eq!(trace.events[1], Event::Delay(4));
    assert_eq!(trace.events.last(), Some(&Event::Write(0xe0)));
}

#[test]
fn timed_pulse_preserves_or_xor_cancel_and_observation_deadline() {
    let api = unsafe { &*rptadv_signaling_descriptor() };
    let pulse = (api.create_pulse)();
    assert_eq!((api.publish_timed)(pulse, 2, 10, false), 0);
    assert_eq!((api.evaluate)(pulse, 100, 1), 3);
    assert_eq!((api.evaluate)(pulse, 10_000_099, 1), 3);
    assert_eq!((api.evaluate)(pulse, 10_000_100, 1), 1);
    assert_eq!((api.publish_timed)(pulse, 2, 10, true), 0);
    assert_eq!((api.evaluate)(pulse, 11_000_000, 3), 1);
    assert_eq!((api.publish_timed)(pulse, 0, 0, false), 0);
    assert_eq!((api.evaluate)(pulse, 11_000_001, 3), 3);
    (api.destroy_pulse)(pulse);
}

#[test]
fn pulse_boundary_rejects_bad_publications_without_state_change() {
    let api = unsafe { &*rptadv_signaling_descriptor() };
    let pulse = (api.create_pulse)();
    assert_eq!((api.publish_timed)(std::ptr::null(), 1, 10, true), -1);
    assert_eq!((api.publish_scheduled)(std::ptr::null(), 1, 0, 10), -1);
    assert_eq!((api.publish_timed)(pulse, 1 << 31, 10, true), -1);
    assert_eq!((api.publish_scheduled)(pulse, 1, 1, 10), -1);
    assert_eq!((api.publish_scheduled)(pulse, 1, 0, 0), -1);
    assert_eq!((api.publish_scheduled)(pulse, 0, 1, 10), -1);
    assert_eq!((api.evaluate)(pulse, 1, 7), 7);
    assert_eq!((api.evaluate)(std::ptr::null_mut(), 1, 7), 7);
    (api.destroy_pulse)(pulse);
    (api.destroy_pulse)(std::ptr::null_mut());
}

#[test]
fn parallel_boundary_preserves_zero_rx_and_validates_callbacks() {
    let api = unsafe { &*rptadv_signaling_descriptor() };
    let mut trace = Trace {
        events: Vec::new(),
        fail_at: None,
        writes: 0,
    };
    let context = (&mut trace as *mut Trace).cast::<c_void>();
    assert_eq!((api.set_binary_channel)(0, 0, context, None), -1);
    assert_eq!((api.clear_rtx_transmit)(0, context, None), -1);
    assert_eq!(
        (api.program_rtx)(0, 0, 0, false, false, context, None, None),
        0
    );
    assert_eq!(
        (api.program_rtx)(
            0,
            146_940_000,
            146_340_000,
            false,
            false,
            context,
            None,
            Some(delay)
        ),
        -1
    );
    assert_eq!(
        (api.program_rtx)(
            0,
            146_940_000,
            146_340_000,
            false,
            false,
            context,
            Some(write),
            None
        ),
        -1
    );
    assert!(trace.events.is_empty());
    trace.fail_at = Some(0);
    assert_eq!((api.set_binary_channel)(0x0f, 5, context, Some(write)), -5);
    assert!(trace.events.is_empty());
    trace.writes = 0;
    assert_eq!((api.clear_rtx_transmit)(0xff, context, Some(write)), -5);
    assert!(trace.events.is_empty());
    trace.fail_at = Some(1);
    trace.writes = 0;
    assert_eq!((api.set_binary_channel)(0x0f, 5, context, Some(write)), -5);
    assert_eq!(trace.events, [Event::Write(0xff)]);
    trace.events.clear();
    trace.writes = 0;
    trace.fail_at = None;
    assert_eq!(
        (api.program_rtx)(
            0xe0,
            444_500_000,
            449_500_000,
            false,
            true,
            context,
            Some(write),
            Some(delay)
        ),
        0
    );
    assert_eq!(trace.writes, 129);
    assert_eq!(trace.events.last(), Some(&Event::Write(0xe0)));
}

#[test]
fn rtx_transport_trace_matches_original_word_and_bit_order() {
    let state = std::sync::atomic::AtomicBool::new(false);
    let mut trace = Trace {
        events: Vec::new(),
        fail_at: None,
        writes: 0,
    };
    let context = (&mut trace as *mut Trace).cast::<c_void>();
    assert_eq!(
        parallel::program_rtx_with_state(
            &state,
            parallel::RtxRequest {
                output: 0xe0,
                rx_frequency_hz: 146_940_000,
                tx_frequency_hz: 146_340_000,
                transmitting: true,
                high_power: true
            },
            context,
            Some(write),
            Some(delay)
        ),
        0
    );
    let mut expected = Vec::new();
    for (word, first_delay) in [(6_401_u32, 200), (117_032, 4)] {
        expected.push(Event::Write(0xe0));
        expected.push(Event::Delay(first_delay));
        for bit in (0..20).rev() {
            let data = if word & (1 << bit) != 0 { 0xe2 } else { 0xe0 };
            expected.extend([
                Event::Write(data),
                Event::Delay(1),
                Event::Write(data | 1),
                Event::Delay(1),
                Event::Write(data),
                Event::Delay(1),
            ]);
        }
        expected.extend([
            Event::Write(0xe0),
            Event::Write(0xe4),
            Event::Delay(1),
            Event::Write(0xe0),
        ]);
    }
    expected.push(Event::Write(0xe8));
    assert_eq!(trace.events, expected);
}

#[test]
fn rtx_partial_write_failures_stop_at_every_protocol_phase() {
    let request = parallel::RtxRequest {
        output: 0xe0,
        rx_frequency_hz: 444_500_000,
        tx_frequency_hz: 449_500_000,
        transmitting: false,
        high_power: false,
    };
    let state = std::sync::atomic::AtomicBool::new(true);
    let mut success = Trace {
        events: Vec::new(),
        fail_at: None,
        writes: 0,
    };
    let context = (&mut success as *mut Trace).cast::<c_void>();
    assert_eq!(
        parallel::program_rtx_with_state(&state, request, context, Some(write), Some(delay)),
        0
    );
    for failure in [0, 1, 2, 3, 61, 62, 63, 64, 65, 66, 127, 128] {
        let state = std::sync::atomic::AtomicBool::new(true);
        let mut trace = Trace {
            events: Vec::new(),
            fail_at: Some(failure),
            writes: 0,
        };
        let context = (&mut trace as *mut Trace).cast::<c_void>();
        let result =
            parallel::program_rtx_with_state(&state, request, context, Some(write), Some(delay));
        assert_eq!(result, -5, "failed write {failure}");
        assert_eq!(trace.writes, failure + 1);
        let cutoff = success
            .events
            .iter()
            .enumerate()
            .filter(|(_, event)| matches!(event, Event::Write(_)))
            .nth(failure)
            .expect("successful fixture has failed write")
            .0;
        assert_eq!(
            trace.events,
            success.events[..cutoff],
            "failed write {failure} advanced timing"
        );
        assert_eq!(
            trace
                .events
                .iter()
                .filter(|event| matches!(event, Event::Write(_)))
                .count(),
            failure
        );
    }
}
