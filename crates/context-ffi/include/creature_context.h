/* creature-context — core C ABI.
 *
 * The engine-facing "port": embed creature-context in a native app instead of
 * shelling out to the CLI. Link the `creature_context_ffi` cdylib and include
 * this header. All functions are thread-hostile per project (open one at a time)
 * but safe to call from any single thread.
 *
 * Return conventions: int32/int64 functions return 0 / a non-negative count on
 * success and a negative code on error (-1 bad argument, -2 engine error,
 * -99 internal panic). Pointer-returning functions return NULL on error; free
 * any non-NULL string with cc_string_free.
 */
#ifndef CREATURE_CONTEXT_H
#define CREATURE_CONTEXT_H

#include <stdbool.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Initialize a project's identity at `root` (once, before scanning). */
int32_t cc_init(const char *root);

/* Index `root` and persist the Atlas (SQLite cache + portable ATLAS.idx). */
int32_t cc_scan(const char *root);

/* Render <root>/ATLAS.png. `galaxy` = circle-packed tree layout (else square).
 * Returns the entity count on success, negative on error. */
int64_t cc_map(const char *root, bool galaxy);

/* Status JSON {"snapshot","entities","edges"} as a heap string, or NULL.
 * Free the result with cc_string_free. */
char *cc_status(const char *root);

/* Free a string returned by this library. */
void cc_string_free(char *ptr);

#ifdef __cplusplus
}
#endif

#endif /* CREATURE_CONTEXT_H */
