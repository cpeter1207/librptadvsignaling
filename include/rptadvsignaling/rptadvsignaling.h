/** @file
 * @brief Portable GPIO pulse and parallel signaling descriptor ABI.
 */
#ifndef RPTADV_SIGNALING_H
#define RPTADV_SIGNALING_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define RPTADV_SIGNALING_ABI_VERSION 1U /**< Initial signaling ABI. */
#define RPTADV_SIGNALING_OK 0 /**< Operation completed. */
#define RPTADV_SIGNALING_INVALID_ARGUMENT -1 /**< Missing handle/callback or invalid mask. */
#define RPTADV_SIGNALING_IO_ERROR -5 /**< Adapter write callback failed. */

/** @brief Opaque pulse state. One service owner evaluates; publishers may publish concurrently. */
struct rptadv_signaling_pulse;

/** @brief Adapter-owned pin write. Zero succeeds; nonzero stops the sequence. */
typedef int32_t (*rptadv_signaling_write)(void *context, uint8_t value);
/** @brief Adapter-owned RTX settling delay, measured in legacy bit-time units. */
typedef void (*rptadv_signaling_delay)(void *context, uint32_t multiplier);

/** @brief Immutable version-one operation table. */
struct rptadv_signaling_descriptor {
  uint32_t struct_size; /**< Size of this table. */
  uint32_t abi_version; /**< Must equal RPTADV_SIGNALING_ABI_VERSION. */
  const char *capability_name; /**< Stable `rptadv.signaling` identity. */

  /** @brief Allocate pulse state during setup. Return -1 for a null output pointer. */
  int32_t (*create_pulse)(struct rptadv_signaling_pulse **out);
  /** @brief Release after all publishers and the service owner have stopped; null is accepted. */
  void (*destroy_pulse)(struct rptadv_signaling_pulse *pulse);
  /** @brief Publish one timed OR or XOR mask; duration zero cancels. Bit 31 is reserved.
   * Any nonzero invert value means true.
   */
  int32_t (*publish_timed)(const struct rptadv_signaling_pulse *pulse,
                           uint32_t mask, uint32_t duration_ms, uint32_t invert);
  /** @brief Publish independent XOR bits; schedule and cancel must be disjoint. */
  int32_t (*publish_scheduled)(const struct rptadv_signaling_pulse *pulse,
                               uint32_t schedule, uint32_t cancel,
                               uint32_t duration_ms);
  /** @brief Observe publications and apply live pulses to baseline at elapsed monotonic ns.
   * Deadlines start at observation; calls for one pulse require one service owner.
   */
  uint32_t (*evaluate)(struct rptadv_signaling_pulse *pulse, uint64_t now_ns,
                       uint32_t baseline);
  /** @brief Write active-low binary channel selection; adapter validates pin ownership. */
  int32_t (*set_binary_channel)(uint8_t output, uint8_t channel, void *context,
                                rptadv_signaling_write write);
  /** @brief Program both RTX words and final transmit state.
   * First-process serial settling uses 200 units, later words 4. Power stays low.
   * The adapter supplies the current output byte, pin write and delay operations.
   * Any nonzero transmitting or high_power value means true.
   */
  int32_t (*program_rtx)(uint8_t output, uint32_t rx_frequency_hz,
                         uint32_t tx_frequency_hz, uint32_t transmitting,
                         uint32_t high_power, void *context,
                         rptadv_signaling_write write,
                         rptadv_signaling_delay delay);
  /** @brief Clear RTX TX and power bits with one adapter-owned write. */
  int32_t (*clear_rtx_transmit)(uint8_t output, void *context,
                                rptadv_signaling_write write);
};

/** @brief Return the immutable descriptor; validate size, version and identity. */
const struct rptadv_signaling_descriptor *rptadv_signaling_descriptor(void);

#ifdef __cplusplus
}
#endif
#endif
