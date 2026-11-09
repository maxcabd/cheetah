#ifndef CHEETAH_FFI_H
#define CHEETAH_FFI_H

#include <stdint.h>

typedef void (*CheetahOpsCallback)(const char *json, void *context);
typedef void (*CheetahMeasureCallback)(
    const char *text,
    float size,
    int bold,
    float max_width,
    float *out_width,
    float *out_height,
    void *context
);

int64_t cheetah_new(
    const char *store_path,
    const char *gateway_url,
    CheetahOpsCallback ops,
    CheetahMeasureCallback measure,
    void *context
);

int64_t cheetah_new_app(
    const char *store_path,
    const char *gateway_url,
    const char *app,
    CheetahOpsCallback ops,
    CheetahMeasureCallback measure,
    void *context
);

void cheetah_resize(int64_t handle, float width, float height);
void cheetah_event(int64_t handle, uint32_t id, const char *kind, const char *payload);
void cheetah_lifecycle(int64_t handle, int active);
void cheetah_free(int64_t handle);

#endif
