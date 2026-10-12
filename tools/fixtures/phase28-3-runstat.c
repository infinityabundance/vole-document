/* Phase 28.3 — exec + resource-statistics runner (measurement scaffolding).
 *
 * The phase-22.2 profiling court used `/usr/bin/time -v`, but the court must be
 * runnable in the `dev` lane (which lacks `time`, `strace`, `python3`, `jq`), so
 * this tiny helper forks/execs the target, waits for it, and prints one
 * machine-readable stderr line with wall time and the child's rusage:
 *
 *   runstat wall_us=.. user_us=.. sys_us=.. maxrss_kb=.. minflt=.. majflt=.. rc=..
 *
 * `ru_maxrss` for RUSAGE_CHILDREN is a high-water mark; because each measured run
 * is a fresh process it is that process's peak RSS. The child's stdout/stderr are
 * inherited unchanged, so the target's answers and its own diagnostics pass
 * through untouched.
 *
 * `RUNSTAT_PRELOAD` (if set) is exported into the *child only* as `LD_PRELOAD`,
 * so the allocation-counting shim loads in the target and not in this runner
 * (otherwise its own few allocations would interleave a second shim line).
 *
 * Built inside the pinned, hard-capped lane (never the host):
 *   cc -O2 -o /tmp/runstat tools/fixtures/phase28-3-runstat.c
 */
#define _GNU_SOURCE
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <sys/wait.h>
#include <sys/resource.h>
#include <time.h>

int main(int argc, char **argv) {
    int i = 1;
    if (argc > 1 && strcmp(argv[1], "--") == 0) i = 2;
    if (i >= argc) {
        fprintf(stderr, "usage: runstat [--] cmd [args...]\n");
        return 2;
    }
    const char *preload = getenv("RUNSTAT_PRELOAD");

    struct rusage rb, ra;
    getrusage(RUSAGE_CHILDREN, &rb);

    struct timespec t0, t1;
    clock_gettime(CLOCK_MONOTONIC, &t0);

    pid_t pid = fork();
    if (pid == 0) {
        if (preload && *preload) setenv("LD_PRELOAD", preload, 1);
        execvp(argv[i], &argv[i]);
        _exit(127);
    }

    int st = 0;
    if (waitpid(pid, &st, 0) < 0) { perror("waitpid"); return 2; }

    clock_gettime(CLOCK_MONOTONIC, &t1);
    getrusage(RUSAGE_CHILDREN, &ra);

    long long wall = (t1.tv_sec - t0.tv_sec) * 1000000LL
                   + (t1.tv_nsec - t0.tv_nsec) / 1000;
    long long ut = (ra.ru_utime.tv_sec - rb.ru_utime.tv_sec) * 1000000LL
                 + (ra.ru_utime.tv_usec - rb.ru_utime.tv_usec);
    long long sy = (ra.ru_stime.tv_sec - rb.ru_stime.tv_sec) * 1000000LL
                 + (ra.ru_stime.tv_usec - rb.ru_stime.tv_usec);
    int rc = WIFEXITED(st) ? WEXITSTATUS(st) : 128 + WTERMSIG(st);

    fprintf(stderr,
            "runstat wall_us=%lld user_us=%lld sys_us=%lld maxrss_kb=%ld minflt=%ld majflt=%ld rc=%d\n",
            wall, ut, sy, ra.ru_maxrss,
            ra.ru_minflt - rb.ru_minflt, ra.ru_majflt - rb.ru_majflt, rc);
    return rc;
}
