/* Phase 22.2 — allocation-counting LD_PRELOAD shim.
 *
 * The crate forbids `unsafe`, so it cannot register a counting global allocator
 * itself. This shim interposes the glibc allocator the Rust std default already
 * uses, counts every allocation and its requested bytes, and prints one line to
 * stderr at process exit. It is measurement scaffolding only: it changes no
 * program behavior, only observes the allocator.
 *
 * Built and used inside the hard-capped doc-baseline service (never the host):
 *   cc -O2 -shared -fPIC -o /tmp/libmallocc.so tools/fixtures/phase22-2-malloc-count.c -ldl
 *   LD_PRELOAD=/tmp/libmallocc.so vole-document observe-batch ...
 *
 * glibc's `__libc_malloc`/`__libc_calloc`/`__libc_realloc`/`__libc_free` are
 * called directly (never via dlsym), so the interposer cannot recurse through the
 * dynamic linker during its own initialization.
 */
#define _GNU_SOURCE
#include <stddef.h>
#include <stdio.h>
#include <stdatomic.h>

extern void *__libc_malloc(size_t);
extern void *__libc_calloc(size_t, size_t);
extern void *__libc_realloc(void *, size_t);
extern void __libc_free(void *);

static _Atomic unsigned long long n_alloc = 0;
static _Atomic unsigned long long n_free = 0;
static _Atomic unsigned long long b_alloc = 0;

static inline void note(size_t bytes) {
    atomic_fetch_add(&n_alloc, 1);
    atomic_fetch_add(&b_alloc, bytes);
}

void *malloc(size_t n) {
    void *p = __libc_malloc(n);
    if (p) note(n);
    return p;
}

void *calloc(size_t a, size_t b) {
    void *p = __libc_calloc(a, b);
    if (p) note(a * b);
    return p;
}

void *realloc(void *old, size_t n) {
    void *p = __libc_realloc(old, n);
    if (p) note(n);
    return p;
}

void free(void *p) {
    if (p) atomic_fetch_add(&n_free, 1);
    __libc_free(p);
}

__attribute__((destructor)) static void report(void) {
    fprintf(stderr, "[malloc-count] allocs=%llu bytes=%llu frees=%llu\n",
            atomic_load(&n_alloc), atomic_load(&b_alloc), atomic_load(&n_free));
}
