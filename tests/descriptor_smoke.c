#include "rptadvsignaling/rptadvsignaling.h"

#include <assert.h>
#include <stddef.h>
#include <string.h>

struct trace {
  uint8_t values[2];
  unsigned count;
};

static int32_t write_byte(void *context, uint8_t value) {
  struct trace *trace = context;
  trace->values[trace->count++] = value;
  return 0;
}

int main(void) {
  const struct rptadv_signaling_descriptor *api = rptadv_signaling_descriptor();
  assert(api == rptadv_signaling_descriptor());
  assert(api->struct_size == sizeof(*api));
  assert(api->abi_version == RPTADV_SIGNALING_ABI_VERSION);
  assert(strcmp(api->capability_name, "rptadv.signaling") == 0);
  assert(api->create_pulse && api->destroy_pulse && api->publish_timed &&
         api->publish_scheduled && api->evaluate && api->set_binary_channel &&
         api->program_rtx && api->clear_rtx_transmit);

  struct rptadv_signaling_pulse *pulse = api->create_pulse();
  assert(pulse);
  assert(api->publish_timed(pulse, 1, 10, true) == RPTADV_SIGNALING_OK);
  assert(api->evaluate(pulse, 100, 0) == 1);
  assert(api->evaluate(pulse, 10000100, 0) == 0);
  api->destroy_pulse(pulse);
  api->destroy_pulse(NULL);

  struct trace trace = {{0}, 0};
  int32_t status = api->set_binary_channel(0x0f, 5, &trace, write_byte);
  assert(status == RPTADV_SIGNALING_OK);
  assert(trace.count == 2 && trace.values[0] == 0xff &&
         trace.values[1] == 0xaf);
  return 0;
}
