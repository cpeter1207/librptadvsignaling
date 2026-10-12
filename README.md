# librptadvsignaling

Portable GPIO pulse and parallel signaling component for rpt_advanced.

The version-one `rptadv.signaling` C descriptor owns opaque pulse state and
the RTX first-program delay state. A GPIO adapter supplies per-handle elapsed
monotonic nanoseconds to `evaluate`, and supplies pin-write and delay callbacks
to parallel operations. The provider does not open hardware devices or read a
clock. Pulse deadlines begin when the service owner observes a publication.

This implementation moves pulse publication/evaluation from
`rptadv-gpio-adapter/src/lib.rs` and binary-channel/RTX sequencing from
`rptadv-gpio-adapter/src/parallel.rs` at commit
`3e662f9be4f6b9731cc4380165afe998e3fda642`. The adapter retains device
ownership, output-pin validation, writes, timing, and error translation.

See `include/rptadvsignaling/rptadvsignaling.h` for lifecycle and ABI details.
