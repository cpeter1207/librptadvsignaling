//! Lock-free pulse publication and service-owned evaluation moved from
//! rptadv-gpio-adapter/src/lib.rs at 3e662f9be4f6b9731cc4380165afe998e3fda642.

use super::INVALID_ARGUMENT;
use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

const TIMED_PULSE_INVERT_BASELINE_BIT: u32 = 1 << 31;
const BITS: usize = u32::BITS as usize;

/// Service-owner state for one timed output pulse.
struct TimedPulseState {
    observed_generation: u64,
    active_pulse_mask: u32,
    pulse_inverts_baseline: bool,
    pulse_deadline: Option<u64>,
}

impl TimedPulseState {
    const fn new() -> Self {
        Self {
            observed_generation: 0,
            active_pulse_mask: 0,
            pulse_inverts_baseline: false,
            pulse_deadline: None,
        }
    }

    fn observe_request(&mut self, now: u64, generation: u64, request: u64) {
        if generation == self.observed_generation {
            return;
        }
        self.observed_generation = generation;
        let request_bits = request as u32;
        let duration_milliseconds = (request >> 32) as u32;
        if duration_milliseconds == 0 {
            self.active_pulse_mask = 0;
            self.pulse_inverts_baseline = false;
            self.pulse_deadline = None;
            return;
        }
        self.active_pulse_mask = request_bits & !TIMED_PULSE_INVERT_BASELINE_BIT;
        self.pulse_inverts_baseline = request_bits & TIMED_PULSE_INVERT_BASELINE_BIT != 0;
        self.pulse_deadline =
            Some(now.saturating_add(u64::from(duration_milliseconds) * 1_000_000));
    }

    fn apply_to(&mut self, baseline: u32, now: u64) -> u32 {
        if self.pulse_deadline.is_some_and(|deadline| now >= deadline) {
            self.active_pulse_mask = 0;
            self.pulse_inverts_baseline = false;
            self.pulse_deadline = None;
        }
        if self.pulse_inverts_baseline {
            baseline ^ self.active_pulse_mask
        } else {
            baseline | self.active_pulse_mask
        }
    }
}

/// Service-owned independent per-bit deadlines.
struct ScheduledPulseState {
    observed_generations: [u64; BITS],
    deadlines: [Option<u64>; BITS],
}

impl ScheduledPulseState {
    const fn new() -> Self {
        Self {
            observed_generations: [0; BITS],
            deadlines: [None; BITS],
        }
    }

    fn observe_requests(
        &mut self,
        now: u64,
        durations: &[AtomicU32; BITS],
        generations: &[AtomicU64; BITS],
    ) {
        for bit in 0..BITS {
            let generation = generations[bit].load(Ordering::Acquire);
            if generation != self.observed_generations[bit] {
                self.observed_generations[bit] = generation;
                let duration = durations[bit].load(Ordering::Acquire);
                self.deadlines[bit] =
                    (duration != 0).then(|| now.saturating_add(u64::from(duration) * 1_000_000));
            }
        }
    }

    fn apply_to(&mut self, baseline: u32, now: u64) -> u32 {
        let mut active_mask = 0;
        for bit in 0..BITS {
            if self.deadlines[bit].is_some_and(|deadline| now >= deadline) {
                self.deadlines[bit] = None;
            }
            if self.deadlines[bit].is_some() {
                active_mask |= 1_u32 << bit;
            }
        }
        baseline ^ active_mask
    }
}

/// One publisher-shared, service-owner-exclusive pulse handle.
pub struct Pulse {
    pulse_request: AtomicU64,
    pulse_generation: AtomicU64,
    state: UnsafeCell<TimedPulseState>,
    durations: [AtomicU32; BITS],
    generations: [AtomicU64; BITS],
    scheduled: UnsafeCell<ScheduledPulseState>,
}

// SAFETY: publishers access atomics only; the caller gives evaluate one exclusive service owner.
unsafe impl Sync for Pulse {}

impl Pulse {
    fn new() -> Self {
        Self {
            pulse_request: AtomicU64::new(0),
            pulse_generation: AtomicU64::new(0),
            state: UnsafeCell::new(TimedPulseState::new()),
            durations: std::array::from_fn(|_| AtomicU32::new(0)),
            generations: std::array::from_fn(|_| AtomicU64::new(0)),
            scheduled: UnsafeCell::new(ScheduledPulseState::new()),
        }
    }
}

pub(super) extern "C" fn create() -> *mut Pulse {
    Box::into_raw(Box::new(Pulse::new()))
}

pub(super) extern "C" fn destroy(handle: *mut Pulse) {
    if !handle.is_null() {
        // SAFETY: the caller stops publishers and the service owner before destruction.
        drop(unsafe { Box::from_raw(handle) });
    }
}

pub(super) extern "C" fn publish_timed(
    handle: *const Pulse,
    mask: u32,
    duration: u32,
    invert: bool,
) -> i32 {
    let Some(pulse) = (unsafe { handle.as_ref() }) else {
        return INVALID_ARGUMENT;
    };
    if mask & TIMED_PULSE_INVERT_BASELINE_BIT != 0 {
        return INVALID_ARGUMENT;
    }
    let request = (u64::from(duration) << 32)
        | u64::from(mask)
        | (u64::from(invert) * u64::from(TIMED_PULSE_INVERT_BASELINE_BIT));
    pulse.pulse_request.store(request, Ordering::Release);
    pulse.pulse_generation.fetch_add(1, Ordering::Release);
    0
}

pub(super) extern "C" fn publish_scheduled(
    handle: *const Pulse,
    schedule: u32,
    cancel: u32,
    duration: u32,
) -> i32 {
    let Some(pulse) = (unsafe { handle.as_ref() }) else {
        return INVALID_ARGUMENT;
    };
    if schedule & cancel != 0 || (schedule != 0) != (duration != 0) {
        return INVALID_ARGUMENT;
    }
    for bit in 0..BITS {
        let bit_mask = 1_u32 << bit;
        if schedule & bit_mask != 0 || cancel & bit_mask != 0 {
            pulse.durations[bit].store(
                if schedule & bit_mask != 0 {
                    duration
                } else {
                    0
                },
                Ordering::Release,
            );
            pulse.generations[bit].fetch_add(1, Ordering::Release);
        }
    }
    0
}

pub(super) extern "C" fn evaluate(handle: *mut Pulse, now: u64, baseline: u32) -> u32 {
    let Some(pulse) = (unsafe { handle.as_ref() }) else {
        return baseline;
    };
    let generation = pulse.pulse_generation.load(Ordering::Acquire);
    let request = pulse.pulse_request.load(Ordering::Acquire);
    // SAFETY: the C contract reserves evaluation of this handle to one service owner.
    let state = unsafe { &mut *pulse.state.get() };
    state.observe_request(now, generation, request);
    // SAFETY: the same sole owner evaluates the independent scheduled state.
    let scheduled = unsafe { &mut *pulse.scheduled.get() };
    scheduled.observe_requests(now, &pulse.durations, &pulse.generations);
    scheduled.apply_to(state.apply_to(baseline, now), now)
}
