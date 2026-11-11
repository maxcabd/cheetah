#ifndef CHEETAH_FFI_H
#define CHEETAH_FFI_H

#include <stddef.h>
#include <stdint.h>

typedef void (*CheetahOpsCallback)(const uint8_t *bytes, size_t length, void *context);
typedef void (*CheetahMeasureCallback)(
    const char *text,
    float size,
    int bold,
    float max_width,
    float *out_width,
    float *out_height,
    void *context
);
typedef void (*CheetahMeasureBatchCallback)(
    const uint8_t *requests,
    size_t length,
    float *out,
    size_t out_length,
    void *context
);

int64_t cheetah_new(
    const char *store_path,
    const char *gateway_url,
    CheetahOpsCallback ops,
    CheetahMeasureCallback measure,
    CheetahMeasureBatchCallback measure_batch,
    void *context
);

int64_t cheetah_new_app(
    const char *store_path,
    const char *gateway_url,
    const char *app,
    CheetahOpsCallback ops,
    CheetahMeasureCallback measure,
    CheetahMeasureBatchCallback measure_batch,
    void *context
);

void cheetah_resize(int64_t handle, float width, float height);
void cheetah_event(int64_t handle, uint32_t id, const char *kind, const char *payload);
void cheetah_event_code(int64_t handle, uint32_t id, uint32_t kind_code, const char *payload);
void cheetah_lifecycle(int64_t handle, int active);
void cheetah_free(int64_t handle);

#endif
