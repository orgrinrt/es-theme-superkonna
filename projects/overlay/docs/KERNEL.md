# Kernel Customization — Research

## TL;DR

**Base kernel: Fork CachyOS's PKGBUILD**, not upstream vanilla. Gets us BORE + sched-ext +
ThinLTO + AutoFDO for free. Apply our own `.config` overrides on top (built-in HID, strip
modules, dm-verity, debug elimination). Current build targets **x86-64-v2** with
`-march=ivybridge` (2012 Mac Mini). Future upgrade to **v3** (AVX2) when hardware allows
PS3/Switch emulation. If CachyOS dies, patches are well-documented — migrate to upstream +
cherry-picks.

Three tiers of optimization on top. **Tier 1** (sysctl tuning from Zig init) costs zero
kernel work and delivers 80% of the gains. **Tier 2** (custom kernel config) gives
deterministic latency and smaller image. **Tier 3** (patches + custom sched-ext scheduler)
is where the real magic lives — a ~60 LOC BPF scheduler tuned for exactly "one game + one
compositor + one audio server" outperforms any general-purpose scheduler because we **know**
the entire workload.

Additional boot parameter: `mitigations=off` for ~5-15% performance gain. Acceptable on
an appliance with no browser, no shell, and games sandboxed via Landlock/seccomp.

Also incorporates lessons from SDWEAK (Steam Deck optimization toolkit) and CryoUtilities.
Most of their tricks are universally applicable — GPU FIFO scheduling, debug overhead
elimination, THP shrinker tuning, MGLRU runtime params, Kyber I/O scheduler for SSDs,
and ZRAM with LZ4. The Van Gogh-specific stuff (MES firmware, PPT limits, voltage curves)
doesn't apply to us.

---

## Prior Art: What Gaming Distros Do

### Valve's jupiter/neptune kernel

Steam Deck ships a custom kernel based on upstream stable with:

| Patch | Purpose |
|-------|---------|
| Async pageflip | Non-blocking display updates for gamescope |
| HDR / color management | Gamescope HDR passthrough (AMD-specific) |
| futex2 / `FUTEX_WAIT_MULTIPLE` | Proton/Wine game compatibility |
| Custom AMD GPU patches | Power management, fan curves, display quirks |
| USB controller quirks | Steam Deck controller, xpad fixes |
| fsync | Fast user-space mutex for Wine/Proton |

Valve's patches are hardware-specific to the Deck. We cherry-pick the
gamescope-relevant ones (HDR, async pageflip, fsync) and skip the
Deck-specific AMD BIOS/fan/power stuff.

### CachyOS patchset (shopping list)

CachyOS maintains the most comprehensive gaming kernel patchset.
It's our starting menu — pick what fits:

| Patch | What it does | Include? |
|-------|-------------|----------|
| **BORE** | Burst-Oriented Response Enhancer on EEVDF | Yes — frame time consistency |
| **sched-ext** | eBPF scheduler infrastructure (mainline 6.12+) | Yes — custom scheduler |
| **le9** | Protect file-backed pages from OOM | Yes — prevents game texture eviction |
| **MGLRU** | Google's multi-generation LRU for page reclaim | Yes — 40% less kswapd CPU |
| **HDR** | Gamescope HDR/color management patches | Yes — HDR passthrough |
| **maple tree** | VMA rbtree → maple tree (upstream) | Kernel default now |
| **BBRv3** | TCP congestion control | Yes — game streaming/downloads |
| **Clear Linux patches** | CPU-specific optimizations | Maybe — depends on target |
| **AutoFDO + LTO** | Profile-guided + link-time optimization | Maybe — slower builds |
| **ZSTD compression** | ZSTD for kernel + initramfs | Yes — faster boot |
| **ACS override** | PCIe isolation for GPU passthrough | No — not a VM host |
| **NVIDIA patches** | Proprietary driver fixes | Conditional — NVIDIA sysext only |

### SteamFork / ChimeraOS / Bazzite

All use variants of the CachyOS or Valve kernel. None do custom sched-ext
schedulers — they use generic `scx_lavd` or `scx_rusty`. Our advantage: we know
the exact workload profile.

### SDWEAK / CryoUtilities / PowerTools

Steam Deck optimization tools operating at different layers:

**SDWEAK** ([Taskerer/SDWEAK](https://github.com/Taskerer/SDWEAK)) — the most
comprehensive. Applies kernel module params, sysfs tuning, udev rules, process
priority (ananicy-cpp), and optionally a custom kernel (linux-charcoal). Most
of its tricks are universally applicable — we adopt the good ones below.

**CryoUtilities** ([CryoByte33](https://github.com/CryoByte33/steam-deck-utilities)) —
simpler, focuses on 7 memory/swap tunables. Written in Go. Notable for recommending
`swappiness=1` and `compaction_proactiveness=0` (we agree on both).

**PowerTools** — Decky plugin for per-game runtime tuning (CPU online/offline, frequency
scaling, GPU clocks). The concept of per-game profiles is interesting but we handle
this through our sched-ext scheduler + GPU sysfs writes from loisto-shell.

What's Van Gogh-specific (skip): `amdgpu mes/uni_mes/mes_kiq` (firmware-dependent),
PPT limits, voltage curves, display overclock, `ryzenadj` undervolting.

What's universal (adopt): GPU FIFO scheduling, `moverate`, debug overhead elimination,
THP shrinker, MGLRU runtime params, Kyber I/O scheduler, ZRAM+LZ4, autogroup disable,
memlock limits, block device tuning.

---

## Kernel Selection

### The variant landscape

Every "gaming kernel" converges on the same core recipe: PREEMPT + HZ_1000 +
interactive scheduler tuning + sched-ext. The real differentiators are the
**compiler pipeline** and **architecture targeting**.

| Kernel | Scheduler | HZ | Preemption | LTO | sched-ext | BORE | Arch pkg | Maintenance |
|--------|-----------|-----|-----------|-----|-----------|------|----------|-------------|
| **Upstream stable** | EEVDF | 250 | VOLUNTARY | No | Yes (6.12+) | No | Official | Excellent |
| **linux-zen** | EEVDF (tuned) | 1000 | PREEMPT | No | Yes | No | Official | Excellent |
| **linux-cachyos** | BORE/EEVDF | 1000 | PREEMPT | ThinLTO+AutoFDO | Yes | Yes | CachyOS repo | Excellent |
| **linux-lts** | EEVDF | 300 | VOLUNTARY | No | 6.12 only | No | Official | Excellent |
| **linux-rt** | EEVDF | varies | PREEMPT_RT | No | Yes | Via CachyOS | AUR | Good |
| **linux-clear** | EEVDF | varies | varies | No | Depends | No | AUR (dying) | Dead |
| **Neptune/Jupiter** | EEVDF | N/A | N/A | No | No | No | AUR | Opaque |
| **linux-charcoal** | EEVDF | 1000 | PREEMPT | LLVM LTO | No | No | N/A | Single person |
| **Liquorix** | PDS | 1000 | PREEMPT | No | Yes | No | AUR | Single person |
| **Xanmod** | EEVDF | 1000 | PREEMPT | ThinLTO | Yes | AUR variant | AUR | Single person |
| **linux-hardened** | EEVDF | 300 | VOLUNTARY | No | No | No | Official | Good |

### Verdict on each

**linux-zen** — The safe conservative choice. Official Arch package, maintained by an Arch
developer (Jan Steffens / `@heftig`). Gets you PREEMPT + HZ_1000 + `-O3` + interactive
CFS tuning (3ms latency, 0.3ms granularity, `defer+madvise` THP, BFQ default, IRQ
threading enabled). Doesn't have BORE or LTO. Think of it as "upstream with sensible
gaming defaults."

**linux-cachyos** — The clear winner. Everything we need:
- **BORE** on EEVDF (burst-oriented scheduling, ~5-10% improvement in 1% low FPS)
- **sched-ext** (for our custom BPF scheduler)
- **ThinLTO + AutoFDO + Propeller** (most aggressive optimization pipeline available)
- **x86-64-v3 builds** (AVX2 targeting, 5-20% uplift vs generic x86-64)
- **le9, MGLRU, HDR, Steam Deck patches** already carried
- Active team maintenance, tracks upstream within days
- Available as Arch pacman repo (not just AUR)

BORE scheduler tunables (exposed via sysctl when `CONFIG_SCHED_BORE=y`):
```
sched_burst_penalty_offset = 24         # base penalty offset
sched_burst_penalty_scale = 1536        # penalty scaling factor
sched_burst_smoothness = 1              # burst score smoothing
sched_burst_cache_lifetime = 75000000   # 75ms burst score cache
sched_burst_fork_atavistic = 2          # inherit burst score from parent
sched_burst_exclude_kthreads = 1        # don't penalize kernel threads
```

**linux-lts** — Bad fit for gaming. GPU drivers lag, Mesa integration misses features,
new hardware enablement delayed. Valve learned this — they jumped SteamOS from 6.1
to 6.5 to 6.11 because driver support matters more than kernel stability for gaming.

**linux-rt (PREEMPT_RT)** — Overkill and counterproductive. Full RT converts all
spinlocks to sleeping mutexes and threads all IRQs. Dramatically reduces worst-case
latency (294x improvement) but **decreases throughput**. CachyOS docs explicitly state:
"The realtime kernel does NOT improve gaming performance due to increased preemption."
RT is for robotics and industrial control. Use PREEMPT (full preemption without RT) —
you get most of the latency benefits without the throughput cost.

**linux-clear** — Dead. Intel killed Clear Linux. The useful bits (AutoFDO methodology)
have been adopted by CachyOS with better results. Intel-specific, server/cloud focus.

**Neptune/Jupiter** — Interesting reference but not a base. Patches are tightly coupled
to Steam Deck hardware. Opaque development (no public issue tracker, no public CI).
The useful bits (HDR, VRR, fsync) are now upstream in 6.12+ or carried by CachyOS.

**linux-charcoal** (sdtweak's kernel) — Strictly inferior to CachyOS for our purposes.
Based on Valve's neptune 6.11, single maintainer (V10lator). Does 1000Hz + LLVM LTO +
`-O3` + Zen 2 targeting + disabled mitigations + NTSYNC. But no BORE, no sched-ext, no
AutoFDO, and the interesting bits are Van Gogh-specific overclocks (3.5→4.2 GHz, 30→50W
PPT) that don't apply to other hardware.

**Liquorix** — Uses PDS (Process Deadline Scheduler) instead of BORE. 2ms timeslice,
1000Hz, hard preemption, split lock detection disabled, BBR2, Kyber default. Single
maintainer (Steven Barrett, who also co-maintains Zen). AUR-only. CachyOS offers the
same or better with better packaging.

**Xanmod** — Carries MGLRU, BBRv3, le9, Clear Linux patches, CPU arch options. Available
in stable/edge/LTS/RT tracks. BORE available as AUR variant. ThinLTO builds. Single
maintainer (Alexandre Frade). Solid but CachyOS covers the same ground with more features
and team maintenance.

**linux-hardened** — Wrong preemption model (VOLUNTARY) and timer (300Hz) for gaming.
But the ASLR improvements and `kptr_restrict=2` are genuinely useful for an
internet-connected appliance. Cherry-pick the hardening patches rather than using
as a base. CachyOS offers a `linux-cachyos-hardened` variant that combines BORE +
hardening.

### Decision: Fork CachyOS PKGBUILD

Don't build from scratch. CachyOS's PKGBUILD is well-structured for forking:

```
CachyOS PKGBUILD (linux-cachyos-bore + sched-ext + lto variant)
  + our .config overrides:
    - Built-in HID controllers (HID_SONY=y, HID_MICROSOFT=y, etc.)
    - Strip unused modules (NFS=n, CIFS=n, ISDN=n, etc.)
    - dm-verity (DM_VERITY=y, DM_VERITY_VERIFY_ROOTHASH_SIG=y)
    - Debug elimination (FTRACE=n, KPROBES=n, PROFILING=n)
    - Kyber + BFQ I/O schedulers built-in
    - ZRAM + LZ4
    - Landlock security
  + our additional patches if needed:
    - xpadneo (Xbox Wireless BT)
    - any custom hardware quirks
  = loisto-kernel
```

**What this gets us for free** (vs doing it ourselves):
- BORE scheduler with tested tunables
- sched-ext infrastructure
- ThinLTO + AutoFDO + Propeller compiler pipeline
- le9, MGLRU tuning patches, HDR/gamescope patches
- Architecture-specific builds (x86-64-v3)
- Fast upstream tracking (days, not weeks)
- Steam Deck compatibility patches

**What we customize on top:**
- Module selection (our minimalist `.config`)
- dm-verity support
- Built-in HID drivers
- Debug/profiling stripped out
- Boot parameters (`mitigations=off`, USB polling, etc.)

**If CachyOS dies:** The patches are all individually documented on their GitHub.
Migration path: upstream stable + cherry-pick BORE + le9 + HDR patches + set up
our own LTO build. Loss of AutoFDO/Propeller (those require CachyOS's profiling
infrastructure) but that's a nice-to-have, not critical.

**Impact on minimalism: none.** The kernel is just the kernel — it doesn't pull in
systemd, dbus, or any userspace. Whether we use CachyOS patches or upstream vanilla,
our runtime stack is still 4 processes at idle. The CachyOS base gives us the
**patches and compiler pipeline**, not their module selection or userspace.

### Version strategy

**Track latest stable, not LTS.**

| Reason | Detail |
|--------|--------|
| GPU drivers | AMD/Intel open-source drivers target mainline. New GPU features (HDR, VRR) land in recent kernels. Mesa requires recent kernel features. |
| Game compatibility | Driver devs work on new games against mainline. Proton/Wine benefits from recent futex/sync improvements. |
| Hardware enablement | New GPUs, controllers, displays get support in mainline first. LTS backports are selective and delayed. |
| Industry precedent | Valve jumped SteamOS from 6.1 → 6.5 → 6.11 in two years. They track stable, not LTS. |

Recommended: Track latest stable (currently 6.18), rebase within 1-2 weeks of
upstream point releases. Keep one LTS kernel as a RAUC fallback boot option.

### x86-64 microarchitecture levels

| Level | Required ISA | Min hardware | Notes |
|-------|-------------|-------------|-------|
| v1 (generic) | SSE2 | Any x86-64 | What everyone ships by default |
| **v2 (current target)** | **SSE4.2, POPCNT, AVX** | **Nehalem/2009** | **Current: 2012 Mac Mini (Ivy Bridge)** |
| v3 (future target) | AVX, AVX2, BMI1/2, FMA | Haswell/2013 | Future upgrade. 5-20% uplift. |
| v4 | AVX-512 | Skylake-X/Zen4 | Xanmod says "no kernel benefit" — gains are userspace |

**Current target: x86-64-v2** with `-march=ivybridge`. The initial hardware is a 2012
Mac Mini (Ivy Bridge, HD 4000 iGPU). Ivy Bridge has AVX but not AVX2, which puts it
at v2. Using `-march=ivybridge` instead of generic v2 enables Ivy Bridge-specific
optimizations (ERMS, FSGSBASE, RDRAND, F16C) that generic v2 misses.

**Future target: x86-64-v3** when upgrading to a more capable machine (PS3/Switch
emulation requires significantly more CPU/GPU). At that point, switch the build to
`-march=x86-64-v3` (or a specific microarch like `znver4`) to pick up the 5-20% uplift
from AVX2 across the entire package set.

**What v2 loses vs v3:**
- The 5-20% figure is for the entire package set (Mesa, emulators, everything). The
  **kernel itself** loses very little — hot paths (scheduler, page fault, syscall dispatch)
  don't use SIMD. Xanmod explicitly states "no kernel benefit" from v4/AVX-512.
- The real loss is in userspace: Mesa shader compilation, libretro core inner loops,
  video decode. But on Ivy Bridge with HD 4000 iGPU, GPU is the bottleneck for anything
  beyond 16-bit era emulation anyway.
- **ThinLTO and AutoFDO still help equally on v2.** Cross-module inlining, dead code
  elimination, and profile-guided code layout are architecture-independent.

**Migration path:** The v2→v3 switch is a single variable change in the PKGBUILD
(`_processor_opt`), a full rebuild, and a RAUC image update. No code changes needed.

CachyOS's PKGBUILD supports per-microarch builds via `_processor_opt`. Set to
`ivybridge` for current, `x86-64-v3` (or specific microarch) for future.

Detection at build time: `gcc -march=ivybridge -Q --help=target`
Detection at runtime: `/lib/ld-linux-x86-64.so.2 --help | grep supported`

### Compiler pipeline

**Clang with ThinLTO is the better choice for kernel compilation.**

| Technique | What it does | Impact |
|-----------|-------------|--------|
| **ThinLTO** | Link-time optimization with parallel compilation | Measurable IPC improvement. Best-tested Clang target. |
| **AutoFDO** | Profile-guided optimization using hardware PMU sampling (Intel LBR / AMD IBS) | ~3% on HPC, up to 10% on microbenchmarks |
| **Propeller** (Clang 19+) | Code layout optimization guided by profiles | Complementary to AutoFDO |
| **kCFI** | Kernel Control Flow Integrity | Security hardening, only available with LLVM. Free with Clang. |
| **`-O3`** | Aggressive optimization (vs default `-O2`) | Zen and CachyOS both use this |

GCC vs Clang benchmarks (Phoronix, GCC 15 vs Clang 20, Zen 5): Results are mixed
per-workload, but Clang with ThinLTO tends to win on aggregate for interactive workloads.
CachyOS uses Clang by default for their `-lto` variants.

**Practical concern:** Some out-of-tree modules (ZFS, NVIDIA proprietary) may have Clang
issues. We don't use ZFS, and NVIDIA goes in a sysext (can be built separately with GCC).

### CPU mitigations

**`mitigations=off`** disables all Spectre/Meltdown/MDS/etc. mitigations on the kernel
command line. Performance impact: **~5-15% depending on workload**, with syscall-heavy
workloads benefiting most.

| Factor | Assessment |
|--------|-----------|
| **Threat model** | Appliance runs known software. No browser, no shell, no user-uploaded code at kernel level. Games run via emulators which are an abstraction layer, not direct CPU access. |
| **Attack surface** | Network stack (iwd, game downloads) and USB (controllers). No local untrusted code execution. No JavaScript JIT. No sandboxed tabs. |
| **Sandboxing** | Games/emulators run under Landlock + seccomp. Even if an emulator is compromised, kernel exploits through Spectre would need to bypass both layers. |
| **Precedent** | linux-charcoal, CachyOS (optional), most Steam Deck tuning guides recommend this. Valve doesn't ship it off by default but doesn't prevent it. |
| **Risk** | A crafted ROM or game mod could theoretically exploit Spectre to read kernel memory. Practical risk is very low for a device that plays games from known sources. |

**Recommendation:** Ship with `mitigations=off` on the kernel command line. The
performance gain is significant and the threat model is favorable. If a future
exploit emerges targeting gaming appliances specifically, push an update that
removes the flag — it's a single kernel parameter change, not a rebuild.

Additional mitigation-adjacent flags worth considering:
```
# Kernel command line
mitigations=off                    # disable all CPU vulnerability mitigations
split_lock_detect=off              # don't trap split-lock instructions (causes stutter in some games)
tsc=reliable                       # trust TSC for timekeeping (skip expensive calibration)
nowatchdog                         # disable kernel watchdog (we have our own via /dev/watchdog)
```

`split_lock_detect=off` deserves special mention: some older games and emulators trigger
split-lock exceptions on modern CPUs. The kernel by default traps and warns/slows these.
On a gaming appliance, just let them through — the performance penalty of a split lock
is microseconds, the penalty of the kernel trapping and logging it is milliseconds.

### NTSYNC

NTSYNC (NT synchronization primitives) is a kernel module that provides native
implementations of Windows synchronization objects (mutexes, semaphores, events)
for Wine/Proton. Without it, Wine emulates these in userspace with futex — functional
but slower.

Status: Merged upstream in kernel 6.14+. charcoal carried it early. CachyOS includes
it. On our kernel version (tracking latest stable), it should be available.

```kconfig
CONFIG_NTSYNC=y                    # or =m, loaded when Wine/Proton runs
```

Impact: Measurable FPS improvement in some Windows games running under Proton,
particularly games that heavily use Windows threading primitives.

---

## Tier 1: Sysctl Tuning (Zero Kernel Work)

Applied at boot by our Zig init before launching any services. These are
`/proc/sys/` and `/sys/` writes — no kernel patching needed.

### Memory

```
vm.swappiness = 1                          # almost never swap (games need RAM)
vm.compaction_proactiveness = 0            # disable proactive compaction (latency spikes)
vm.compact_unevictable_allowed = 0         # don't compact unevictable pages
vm.page_lock_unfairness = 8               # more unfair retries before fairness (sdtweak: 8 > default 5)
vm.watermark_boost_factor = 0              # disable watermark boosting entirely (sdtweak)
vm.watermark_scale_factor = 125            # larger gap between watermarks, kswapd starts earlier
vm.zone_reclaim_mode = 0                   # don't reclaim from local NUMA zone
vm.min_free_kbytes = 131072                # 128MB free reserve prevents emergency reclaim stalls
vm.dirty_bytes = 419430400                 # 400MB dirty page threshold (sdtweak)
vm.dirty_background_bytes = 209715200      # 200MB background writeback threshold (sdtweak)
vm.dirty_expire_centisecs = 1500           # dirty pages expire at 15s (default 30s)
vm.dirty_writeback_centisecs = 1500        # writeback thread wakes every 15s (batched)
vm.page-cluster = 0                        # read 1 page from swap, no prefetch (bad locality)
vm.vfs_cache_pressure = 66                 # keep dentry/inode caches longer
vm.stat_interval = 15                      # VM stats update every 15s not 1s
```

**Note on watermarks:** `watermark_boost_factor=0` (sdtweak) + `watermark_scale_factor=125`
makes kswapd activate earlier and more predictably. The default behavior has kswapd
"boosting" watermarks reactively, causing sudden reclaim stalls that drop frames.
Disabling boost + widening the scale means steady, predictable background reclaim.

### Transparent Huge Pages

```
/sys/kernel/mm/transparent_hugepage/enabled = always
/sys/kernel/mm/transparent_hugepage/defrag = defer+madvise
/sys/kernel/mm/transparent_hugepage/shmem_enabled = advise
/sys/kernel/mm/transparent_hugepage/khugepaged/defrag = 1
/sys/kernel/mm/transparent_hugepage/khugepaged/max_ptes_none = 409
/sys/kernel/mm/transparent_hugepage/khugepaged/max_ptes_swap = 128
/sys/kernel/mm/transparent_hugepage/khugepaged/pages_to_scan = 2048
/sys/kernel/mm/transparent_hugepage/khugepaged/scan_sleep_millisecs = 5000
/sys/kernel/mm/transparent_hugepage/khugepaged/alloc_sleep_millisecs = 50000
/sys/kernel/mm/ksm/run = 0
```

THP gives ~5-15% FPS improvement on games with large working sets.

**THP shrinker trick (from sdtweak):** `max_ptes_none=409` (default 511) tells
khugepaged to split any huge page where >409/512 sub-pages (80%) are zero-filled.
This prevents the classic `THP=always` memory waste where a huge page is allocated
for a few KB of actual data. With this, we can keep `khugepaged/defrag=1` and
`defrag=defer+madvise` instead of disabling defrag entirely — the shrinker
reclaims wasteful huge pages, while defrag promotes worthwhile ones.

This is smarter than CryoUtilities' approach (`defrag=0`, `khugepaged/defrag=0`)
which prevents THP waste by never defragging, at the cost of fewer huge pages.
The sdtweak approach gets both: huge pages where they help, reclamation where they don't.

KSM (Kernel Same-page Merging) is disabled — it scans for duplicate pages to
deduplicate, which wastes CPU on a single-app system.

### MGLRU Runtime

```
/sys/kernel/mm/lru_gen/enabled = 7          # all 3 feature bits (base + mm_struct + nonresident)
/sys/kernel/mm/lru_gen/min_ttl_ms = 200     # pages live at least 200ms before eviction
```

`enabled=7` is a bitmask: bit 0 = base MGLRU, bit 1 = per-mm_struct tracking,
bit 2 = nonresident page tracking. All three should be on.

`min_ttl_ms=200` prevents thrashing where pages get evicted and immediately
faulted back — particularly important for emulators that access large ROM
files in patterns the kernel can't predict.

### Network

```
net.ipv4.tcp_congestion_control = bbr      # BBR for game downloads + streaming
net.core.default_qdisc = fq               # fair queueing for BBR
net.ipv4.tcp_fastopen = 3                  # TFO for API calls (client+server)
net.core.netdev_max_backlog = 16384        # larger NIC rx queue
```

### I/O

Scheduler and tuning varies by device type. The init detects transport
and applies the right profile:

**NVMe:**
```
scheduler = kyber                          # latency-targeted scheduler (not "none")
read_ahead_kb = 1024                       # 1MB read-ahead
wbt_lat_usec = 999                         # write-back throttle at ~1ms
nr_requests = 2048                         # deep I/O queue
iostats = 0                                # disable I/O accounting (saves CPU)
add_random = 0                             # don't feed I/O timing to entropy pool
iosched/write_lat_nsec = 6000000           # Kyber: 6ms write latency target
iosched/read_lat_nsec = 1200000            # Kyber: 1.2ms read latency target
```

**SATA SSD:**
```
scheduler = kyber                          # same as NVMe
read_ahead_kb = 2048                       # larger read-ahead (slower than NVMe)
iostats = 0
add_random = 0
```

**SD card / USB storage:**
```
scheduler = bfq                            # BFQ for slow devices with seek penalty
read_ahead_kb = 2048                       # large read-ahead for slow media
rq_affinity = 2                            # force completion on same CPU as submission
wbt_lat_usec = 2000                        # 2ms write-back throttle
iostats = 0
add_random = 0
iosched/slice_idle = 0                     # no idle waiting
iosched/back_seek_penalty = 1              # minimal back-seek penalty
iosched/fifo_expire_sync = 100             # sync deadline 100ms
iosched/fifo_expire_async = 200            # async deadline 200ms
```

**Why Kyber over "none" for NVMe:** Previously we had `scheduler=none` for NVMe.
sdtweak uses Kyber instead — it adds latency targets that prevent write storms
(game saves, shader cache writes) from starving read I/O (texture loading).
The overhead is negligible on NVMe but the latency guarantees matter for gaming.

### Filesystem

```
fs.aio-max-nr = 131072                     # higher async I/O limit
fs.inotify.max_user_watches = 65536        # for our Zig device watcher
fs.pipe-max-size = 2097152                 # 2MB pipe buffer
```

Mount `/home` (data partition) with `noatime` — disables access time tracking,
saves one metadata write per file read.

### GPU

#### Module parameters (modprobe)

```
gpu_sched.sched_policy = 0                 # FIFO scheduling (not round-robin)
amdgpu.moverate = 128                      # 128 MB/s VRAM↔GTT migration (default: 8 MB/s)
amdgpu.lbpw = 0                            # disable Load Balancing Per Watt
```

**`sched_policy=0` (FIFO)** is the most impactful GPU tuning for an appliance.
The DRM GPU scheduler defaults to round-robin fairness between GPU clients. On a
desktop with browser + compositor + game, round-robin is sensible. On a console
where there's ONE GPU consumer (gamescope → game), FIFO means submissions are
processed in order with zero fairness overhead. This applies to any GPU using the
DRM scheduler (amdgpu, i915, nouveau).

**`moverate=128`** raises the VRAM↔GTT buffer migration rate from 8 MB/s to
128 MB/s — a 16x speedup for texture streaming when the GPU driver moves buffers
between video memory and system memory. Default 8 MB/s is absurdly conservative.

**`lbpw=0`** disables power-balancing on AMD GPUs, prioritizing throughput over
efficiency. On a plugged-in console, we want maximum GPU performance.

#### Runtime sysfs

```
/sys/class/drm/card0/device/power_dpm_force_performance_level = high
```

Lock GPU clocks high during gameplay. loisto-shell sets `high` on game launch,
`auto` on return to menu to save power on portable setups.

### Scheduler

```
kernel.sched_autogroup_enabled = 0         # disable per-TTY cgroup autogroup
```

Autogroup creates per-TTY scheduling cgroups for "desktop fairness" — makes
sense on a multi-user desktop, makes zero sense on an appliance with one app.
Disabling it gives the scheduler (and our sched-ext BPF) direct control over
all tasks without cgroup interference.

### Debug Overhead Elimination

On a dedicated appliance, all profiling/debugging infrastructure is dead weight:

```
kernel.perf_cpu_time_max_percent = 1       # limit perf overhead to 1%
kernel.perf_event_max_sample_rate = 1      # effectively disable perf sampling
kernel.perf_event_max_contexts_per_stack = 1
kernel.perf_event_max_stack = 1
kernel.nmi_watchdog = 0                    # disable NMI watchdog (1 NMI/sec/core saved)
kernel.soft_watchdog = 0                   # disable soft lockup detector
kernel.watchdog = 0                        # disable kernel watchdog (we have our own)
kernel.timer_migration = 0                 # don't migrate timers between cores
kernel.ftrace_enabled = 0                  # disable function tracer
kernel.core_pattern = /dev/null            # discard core dumps
kernel.printk_devkmsg = off                # disable /dev/kmsg writes
kernel.io_delay_type = 3                   # udelay instead of port 0x80 writes
debug.exception-trace = 0                  # disable exception trace logging
dev.hpet.max-user-freq = 2048             # higher HPET timer frequency for user processes
```

**Why disable the kernel watchdog when we have our own?** The kernel's soft/NMI
watchdog fires periodic interrupts to detect lockups. We have a Zig init writing
to `/dev/watchdog` (hardware watchdog) which serves the same purpose for full
system hangs, plus RAUC boot assessment for rollback. The kernel watchdog's
per-second NMI on every core is pure overhead on an appliance.

### ZRAM

ZRAM provides compressed swap in RAM — much faster than disk swap, and LZ4
compression is nearly free on modern CPUs:

```
# Configured via zram-generator or Zig init at boot:
# - Size: 2x physical RAM
# - Algorithm: lz4
# - Priority: 100 (higher than any disk swap)
```

Combined with `swappiness=1` and `page-cluster=0`, the system almost never
swaps, but when it does (memory pressure from demanding emulators), it swaps
to compressed RAM rather than hitting disk.

### Resource Limits

```
# /etc/security/limits.d/ or set from Zig init
* hard memlock 2147484                     # ~2GB memlock limit (default: 64KB)
* soft memlock 2147484
```

GPU drivers need to pin buffer memory via `mlock()`. The default 64KB limit
is absurdly low — games with large VRAM allocations hit it immediately.

### Zig init integration

```zig
const tuning = .{
    // --- Memory ---
    .{ "/proc/sys/vm/swappiness", "1" },
    .{ "/proc/sys/vm/compaction_proactiveness", "0" },
    .{ "/proc/sys/vm/compact_unevictable_allowed", "0" },
    .{ "/proc/sys/vm/page_lock_unfairness", "8" },
    .{ "/proc/sys/vm/watermark_boost_factor", "0" },
    .{ "/proc/sys/vm/watermark_scale_factor", "125" },
    .{ "/proc/sys/vm/zone_reclaim_mode", "0" },
    .{ "/proc/sys/vm/min_free_kbytes", "131072" },
    .{ "/proc/sys/vm/dirty_bytes", "419430400" },
    .{ "/proc/sys/vm/dirty_background_bytes", "209715200" },
    .{ "/proc/sys/vm/dirty_expire_centisecs", "1500" },
    .{ "/proc/sys/vm/dirty_writeback_centisecs", "1500" },
    .{ "/proc/sys/vm/page-cluster", "0" },
    .{ "/proc/sys/vm/vfs_cache_pressure", "66" },
    .{ "/proc/sys/vm/stat_interval", "15" },

    // --- THP ---
    .{ "/sys/kernel/mm/transparent_hugepage/enabled", "always" },
    .{ "/sys/kernel/mm/transparent_hugepage/defrag", "defer+madvise" },
    .{ "/sys/kernel/mm/transparent_hugepage/shmem_enabled", "advise" },
    .{ "/sys/kernel/mm/transparent_hugepage/khugepaged/defrag", "1" },
    .{ "/sys/kernel/mm/transparent_hugepage/khugepaged/max_ptes_none", "409" },
    .{ "/sys/kernel/mm/transparent_hugepage/khugepaged/max_ptes_swap", "128" },
    .{ "/sys/kernel/mm/transparent_hugepage/khugepaged/pages_to_scan", "2048" },
    .{ "/sys/kernel/mm/transparent_hugepage/khugepaged/scan_sleep_millisecs", "5000" },
    .{ "/sys/kernel/mm/transparent_hugepage/khugepaged/alloc_sleep_millisecs", "50000" },
    .{ "/sys/kernel/mm/ksm/run", "0" },

    // --- MGLRU ---
    .{ "/sys/kernel/mm/lru_gen/enabled", "7" },
    .{ "/sys/kernel/mm/lru_gen/min_ttl_ms", "200" },

    // --- Scheduler ---
    .{ "/proc/sys/kernel/sched_autogroup_enabled", "0" },

    // --- Debug overhead elimination ---
    .{ "/proc/sys/kernel/perf_cpu_time_max_percent", "1" },
    .{ "/proc/sys/kernel/perf_event_max_sample_rate", "1" },
    .{ "/proc/sys/kernel/perf_event_max_contexts_per_stack", "1" },
    .{ "/proc/sys/kernel/perf_event_max_stack", "1" },
    .{ "/proc/sys/kernel/nmi_watchdog", "0" },
    .{ "/proc/sys/kernel/soft_watchdog", "0" },
    .{ "/proc/sys/kernel/watchdog", "0" },
    .{ "/proc/sys/kernel/timer_migration", "0" },
    .{ "/proc/sys/kernel/ftrace_enabled", "0" },
    .{ "/proc/sys/kernel/core_pattern", "/dev/null" },
    .{ "/proc/sys/kernel/printk_devkmsg", "off" },
    .{ "/proc/sys/kernel/io_delay_type", "3" },
    .{ "/proc/sys/debug/exception-trace", "0" },
    .{ "/proc/sys/dev/hpet/max-user-freq", "2048" },
    .{ "/sys/class/rtc/rtc0/max_user_freq", "2048" },

    // --- Network ---
    .{ "/proc/sys/net/ipv4/tcp_congestion_control", "bbr" },
    .{ "/proc/sys/net/core/default_qdisc", "fq" },
    .{ "/proc/sys/net/ipv4/tcp_fastopen", "3" },
    .{ "/proc/sys/net/core/netdev_max_backlog", "16384" },

    // --- Filesystem ---
    .{ "/proc/sys/fs/aio-max-nr", "131072" },
    .{ "/proc/sys/fs/inotify/max_user_watches", "65536" },
    .{ "/proc/sys/fs/pipe-max-size", "2097152" },

    // --- Input ---
    .{ "/sys/module/usbhid/parameters/jspoll", "1" },
    .{ "/sys/module/usbhid/parameters/kbpoll", "1" },
    .{ "/sys/module/usbhid/parameters/mousepoll", "1" },
};

fn apply_tuning() void {
    for (tuning) |t| {
        const fd = std.posix.open(t[0], .{ .ACCMODE = .WRONLY }, 0) catch continue;
        defer std.posix.close(fd);
        _ = std.posix.write(fd, t[1]) catch {};
    }
}

fn set_io_schedulers() void {
    // Detect device type and apply appropriate scheduler + tuning
    var dir = std.fs.openDirAbsolute("/sys/block", .{ .iterate = true }) catch return;
    defer dir.close();
    var iter = dir.iterate();
    while (iter.next() catch null) |entry| {
        const is_nvme = std.mem.startsWith(u8, entry.name, "nvme");
        const is_mmc = std.mem.startsWith(u8, entry.name, "mmcblk");
        const is_zram = std.mem.startsWith(u8, entry.name, "zram");

        if (is_zram) continue; // ZRAM doesn't need a scheduler

        const sched = if (is_mmc) "bfq" else "kyber";
        var buf: [128]u8 = undefined;

        // Set scheduler
        const sched_path = std.fmt.bufPrint(&buf, "/sys/block/{s}/queue/scheduler", .{entry.name}) catch continue;
        write_sysfs(sched_path, sched);

        // Common: disable iostats and entropy feeding
        write_sysfs_fmt(&buf, "/sys/block/{s}/queue/iostats", .{entry.name}, "0");
        write_sysfs_fmt(&buf, "/sys/block/{s}/queue/add_random", .{entry.name}, "0");

        if (is_nvme) {
            write_sysfs_fmt(&buf, "/sys/block/{s}/queue/read_ahead_kb", .{entry.name}, "1024");
            write_sysfs_fmt(&buf, "/sys/block/{s}/queue/wbt_lat_usec", .{entry.name}, "999");
            write_sysfs_fmt(&buf, "/sys/block/{s}/queue/nr_requests", .{entry.name}, "2048");
            // Kyber latency targets
            write_sysfs_fmt(&buf, "/sys/block/{s}/queue/iosched/write_lat_nsec", .{entry.name}, "6000000");
            write_sysfs_fmt(&buf, "/sys/block/{s}/queue/iosched/read_lat_nsec", .{entry.name}, "1200000");
        } else if (is_mmc) {
            write_sysfs_fmt(&buf, "/sys/block/{s}/queue/read_ahead_kb", .{entry.name}, "2048");
            write_sysfs_fmt(&buf, "/sys/block/{s}/queue/rq_affinity", .{entry.name}, "2");
            write_sysfs_fmt(&buf, "/sys/block/{s}/queue/wbt_lat_usec", .{entry.name}, "2000");
            // BFQ tuning for slow media
            write_sysfs_fmt(&buf, "/sys/block/{s}/queue/iosched/slice_idle", .{entry.name}, "0");
            write_sysfs_fmt(&buf, "/sys/block/{s}/queue/iosched/back_seek_penalty", .{entry.name}, "1");
            write_sysfs_fmt(&buf, "/sys/block/{s}/queue/iosched/fifo_expire_sync", .{entry.name}, "100");
            write_sysfs_fmt(&buf, "/sys/block/{s}/queue/iosched/fifo_expire_async", .{entry.name}, "200");
        } else {
            // SATA SSD or other: Kyber with moderate read-ahead
            write_sysfs_fmt(&buf, "/sys/block/{s}/queue/read_ahead_kb", .{entry.name}, "2048");
        }
    }
}

fn write_sysfs(path: []const u8, value: []const u8) void {
    const fd = std.posix.open(path, .{ .ACCMODE = .WRONLY }, 0) catch return;
    defer std.posix.close(fd);
    _ = std.posix.write(fd, value) catch {};
}
```

This runs before any service starts. Total cost: ~2ms of boot time.

---

## Tier 2: Custom Kernel Config

Build a custom kernel from upstream stable + selected patches. This gives:
- Smaller image (strip unused drivers/features)
- Deterministic latency (PREEMPT, HZ_1000)
- Built-in gaming subsystems (no module loading delays)

### Essential CONFIG options

```kconfig
# Preemption — mandatory for audio/frame timing
CONFIG_PREEMPT=y
CONFIG_HZ_1000=y
CONFIG_NO_HZ_FULL=y
CONFIG_IRQ_FORCED_THREADING=y

# Scheduler
CONFIG_SCHED_CLASS_EXT=y          # sched-ext eBPF scheduler support
CONFIG_SCHED_BORE=y               # BORE on EEVDF (from CachyOS patch)

# Memory
CONFIG_LRU_GEN=y                  # MGLRU
CONFIG_LRU_GEN_ENABLED=y          # Enable by default
CONFIG_TRANSPARENT_HUGEPAGE=y
CONFIG_TRANSPARENT_HUGEPAGE_ALWAYS=y
CONFIG_ZRAM=y                     # Compressed swap (better than disk swap)
CONFIG_ZSWAP=n                    # Conflicts with ZRAM approach
CONFIG_ZSMALLOC=y

# dm-verity (mandatory for immutable root)
CONFIG_DM_VERITY=y
CONFIG_DM_VERITY_VERIFY_ROOTHASH_SIG=y

# GPU — build everything in, no module delay
CONFIG_DRM=y
CONFIG_DRM_AMDGPU=y               # AMD (most common gaming GPU)
CONFIG_DRM_I915=y                 # Intel iGPU
CONFIG_DRM_NOUVEAU=n              # Use NVIDIA proprietary via sysext
CONFIG_DRM_SIMPLEDRM=y            # Fallback framebuffer

# Input — built-in for zero-delay controller recognition
CONFIG_HID=y
CONFIG_HID_SONY=y                 # DualShock/DualSense
CONFIG_HID_MICROSOFT=y            # Xbox controllers
CONFIG_HID_NINTENDO=y             # Switch Pro, Joy-Con
CONFIG_HID_STEAM=y                # Steam Controller
CONFIG_USB_XHCI_HCD=y
CONFIG_USB_HID=y
CONFIG_INPUT_JOYDEV=y
CONFIG_INPUT_EVDEV=y
CONFIG_INPUT_FF_MEMLESS=y         # Force feedback

# Bluetooth — module (loaded on-demand for pairing)
CONFIG_BT=m
CONFIG_BT_HCIBTUSB=m
CONFIG_BT_HIDP=m

# Network — iwd needs these
CONFIG_CFG80211=y
CONFIG_IWLWIFI=y                  # Intel WiFi
CONFIG_ATH11K=m                   # Qualcomm WiFi (module)
CONFIG_ATH12K=m
CONFIG_BRCMFMAC=m                 # Broadcom WiFi (module)
CONFIG_NET_SCH_FQ=y               # Fair queueing for BBR
CONFIG_TCP_CONG_BBR=y

# Audio — PipeWire uses ALSA directly
CONFIG_SND=y
CONFIG_SND_HDA_INTEL=y
CONFIG_SND_USB_AUDIO=y
CONFIG_SND_SOC=y
CONFIG_SND_TIMER=y
CONFIG_HIGH_RES_TIMERS=y          # Precise audio scheduling

# Filesystem
CONFIG_SQUASHFS=y                 # Squashfs base image
CONFIG_SQUASHFS_ZSTD=y
CONFIG_OVERLAY_FS=y               # Overlay for writable layer
CONFIG_EROFS_FS=y                 # sysext images use erofs
CONFIG_VFAT_FS=y                  # ESP
CONFIG_EXT4_FS=y                  # Data partition
CONFIG_TMPFS=y

# Security — minimal but correct
CONFIG_SECURITY_LANDLOCK=y        # Sandbox games/emulators
CONFIG_SECCOMP=y
CONFIG_SECCOMP_FILTER=y

# eBPF — for sched-ext and network tuning
CONFIG_BPF=y
CONFIG_BPF_SYSCALL=y
CONFIG_BPF_JIT=y
CONFIG_BPF_JIT_ALWAYS_ON=y

# I/O schedulers
CONFIG_MQ_DEADLINE=y              # Fallback scheduler
CONFIG_IOSCHED_BFQ=y              # BFQ for SD cards / slow storage
CONFIG_IOSCHED_KYBER=y            # Kyber for NVMe / SSD (latency-targeted)

# ZRAM
CONFIG_ZRAM=y
CONFIG_CRYPTO_LZ4=y              # LZ4 compression for ZRAM
CONFIG_CRYPTO_LZ4HC=n             # Don't need high-compression variant

# Boot
CONFIG_EFI=y
CONFIG_EFI_STUB=y
CONFIG_KERNEL_ZSTD=y              # ZSTD kernel compression

# Strip out what we don't need
CONFIG_SOUND_OSS_CORE=n
CONFIG_NFS_FS=n
CONFIG_CIFS=n
CONFIG_9P_FS=n
CONFIG_INFINIBAND=n
CONFIG_HAMRADIO=n
CONFIG_CAN=n
CONFIG_ISDN=n
CONFIG_ATM=n
CONFIG_WIRELESS_EXT=n             # Legacy wireless (iwd doesn't need it)
CONFIG_STAGING=n
CONFIG_DEBUG_INFO_DWARF5=n        # Save ~200MB in build
CONFIG_FTRACE=n                   # No function tracer (disabled at runtime anyway)
CONFIG_KPROBES=n                  # No kernel probes
CONFIG_PROFILING=n                # No profiling support
```

### What this buys

| Metric | Generic distro kernel | Custom loisto kernel |
|--------|----------------------|---------------------|
| Installed size | ~120MB | ~40-50MB |
| Module count | ~6000 | ~200 |
| Boot modules loaded | ~80 | ~10 (mostly built-in) |
| HID controller latency | Module load + udev | Zero (built-in) |
| Preemption | `PREEMPT_DYNAMIC` (voluntary default) | `PREEMPT` (full, always) |
| Timer frequency | 250Hz | 1000Hz |

---

## Tier 3: Patches & Custom Code

### BORE Scheduler

Burst-Oriented Response Enhancer. Patches EEVDF (the default CFS
replacement since 6.6) to track per-task burstiness and give higher
priority to tasks with bursty CPU usage patterns.

**Why it matters for gaming:**
- Game render threads are inherently bursty (compute frame → wait for vsync)
- BORE recognizes this pattern and prioritizes the render thread during its burst
- Result: more consistent frame times, fewer scheduling-induced frame drops
- ~5-10% improvement in 1% low FPS in benchmarks

Available as a patch from CachyOS. Minimal diff (~500 lines). Actively
maintained against upstream kernel releases.

### MGLRU (Multi-Generation LRU)

Google's page reclaim improvement (mainline since 6.1, but benefits from
CachyOS's tuning patches). Uses multiple generations to track page aging
instead of the traditional active/inactive lists.

**Impact:**
- 40% less kswapd CPU usage under memory pressure
- 18% less rendering latency during page reclaim
- Games with large texture sets (emulated PS2/GC/Wii) benefit most
- Critical for 8GB RAM systems running demanding emulators

Enable with `CONFIG_LRU_GEN=y` + `CONFIG_LRU_GEN_ENABLED=y`.

### le9 (Low-Memory Killer Protection)

Prevents the OOM killer from evicting file-backed pages (game textures,
shader caches, emulator ROMs) under memory pressure. Instead of killing
the game, the kernel preserves the working set.

**Without le9:** High memory pressure → kswapd evicts game textures →
game stutters reloading them → OOM kills the game.

**With le9:** High memory pressure → kswapd targets anonymous pages
(less critical) → game keeps running, maybe slower but alive.

### HDR / Gamescope patches

AMD-focused HDR and color management patches from Valve. Required for:
- HDR passthrough to HDR displays
- Color space conversion (BT.2020 ↔ BT.709)
- Gamescope's internal HDR compositing

These patches track Valve's `jupiter` branch and are rebased by CachyOS.
Without them, gamescope works but HDR output is unavailable.

### Async pageflip

Allows gamescope to submit pageflips without blocking on the previous
frame's completion. Reduces input-to-display latency by one frame in
some scenarios. From Valve's kernel.

### fsync / futex_waitv

Fast user-space mutex for Wine/Proton. `futex_waitv` is mainline since
5.16, but the `FUTEX_WAIT_MULTIPLE` variant used by older Proton builds
needs a patch. Modern Proton uses `futex_waitv` — check if the patch is
still needed when we ship.

---

## Custom sched-ext Scheduler

This is the standout opportunity. sched-ext (mainline since 6.12) lets you
load custom CPU schedulers at runtime as eBPF programs. No kernel recompile
to iterate.

### Why this is powerful for loisto

General-purpose schedulers (CFS, BORE, EEVDF) optimize for fairness across
unknown workloads. But we **know** our workload exactly:

```
PID 1: loisto-init      → near-zero CPU, sleeps on epoll
PID 2: iwd              → near-zero CPU, wakes on WiFi events
PID 3: pipewire         → periodic ~5ms bursts every 5.3ms (audio callback)
PID 4: gamescope        → periodic GPU submit + pageflip
PID 5+: game/emulator   → the main CPU consumer
```

A scheduler tuned for "audio > compositor > game > everything else" with
known task classification outperforms any general-purpose heuristic.

### Architecture

```
User space (Rust loader)          Kernel (BPF program)
┌─────────────────────┐          ┌──────────────────────┐
│ scx_loisto           │  load   │ scx_loisto.bpf.c      │
│                      │────────▶│                        │
│ - classify tasks     │         │ - 4 priority queues    │
│ - monitor latency    │ maps   │ - strict priority      │
│ - adjust weights     │◀──────▶│ - per-task accounting  │
│ - report stats       │         │ - core pinning hints   │
└─────────────────────┘          └──────────────────────┘
```

### BPF scheduler code

```c
// scx_loisto.bpf.c — Custom gaming appliance scheduler
#include <scx/common.bpf.h>

char _license[] SEC("license") = "GPL";

// Four dispatch queues, strict priority order
#define DSQ_AUDIO   0   // PipeWire, audio threads
#define DSQ_COMP    1   // Gamescope compositor
#define DSQ_GAME    2   // Game/emulator threads
#define DSQ_OTHER   3   // Everything else (iwd, init, helpers)

// Time slices per tier (nanoseconds)
#define SLICE_AUDIO   1000000   //  1ms — audio must never miss a callback
#define SLICE_COMP    2000000   //  2ms — compositor needs steady frames
#define SLICE_GAME    5000000   //  5ms — game gets the bulk
#define SLICE_OTHER   1000000   //  1ms — background tasks get scraps

// Known comm prefixes for classification
// (updated via BPF map if we need runtime changes)
struct {
    __uint(type, BPF_MAP_TYPE_HASH);
    __uint(max_entries, 64);
    __type(key, char[16]);
    __type(value, u32);
} task_class_map SEC(".maps");

static int classify(struct task_struct *p)
{
    u32 *tier;
    char comm[16];

    // Check explicit classification map first
    bpf_probe_read_kernel_str(comm, sizeof(comm), p->comm);
    tier = bpf_map_lookup_elem(&task_class_map, comm);
    if (tier)
        return *tier;

    // Heuristic fallback: real-time priority → audio
    if (p->policy == SCHED_FIFO || p->policy == SCHED_RR)
        return DSQ_AUDIO;

    // Nice value classification
    if (p->static_prio < 120)  // nice < 0
        return DSQ_GAME;

    return DSQ_OTHER;
}

s32 BPF_STRUCT_OPS(loisto_enqueue, struct task_struct *p, u64 enq_flags)
{
    int tier = classify(p);

    u64 slice;
    switch (tier) {
    case DSQ_AUDIO: slice = SLICE_AUDIO; break;
    case DSQ_COMP:  slice = SLICE_COMP;  break;
    case DSQ_GAME:  slice = SLICE_GAME;  break;
    default:        slice = SLICE_OTHER;  break;
    }

    scx_bpf_dispatch(p, tier, slice, enq_flags);
    return 0;
}

void BPF_STRUCT_OPS(loisto_dispatch, s32 cpu, struct task_struct *prev)
{
    // Strict priority: drain higher queues first
    if (scx_bpf_consume(DSQ_AUDIO))
        return;
    if (scx_bpf_consume(DSQ_COMP))
        return;
    if (scx_bpf_consume(DSQ_GAME))
        return;
    scx_bpf_consume(DSQ_OTHER);
}

SCX_OPS_DEFINE(loisto_ops,
    .enqueue   = (void *)loisto_enqueue,
    .dispatch  = (void *)loisto_dispatch,
    .name      = "loisto",
);
```

### Rust userspace loader

The BPF program is loaded by a small Rust binary using `scx_utils`:

```rust
// scx_loisto/src/main.rs
use scx_utils::prelude::*;

fn main() -> anyhow::Result<()> {
    let mut skel = ScxLoistoBpf::open()?.load()?;

    // Pre-populate task classification map
    let classifications = [
        ("pipewire",     DSQ_AUDIO),
        ("pw-audio",     DSQ_AUDIO),
        ("gamescope",    DSQ_COMP),
        ("Xwayland",     DSQ_COMP),
        // Game processes get classified by nice value or
        // by loisto-shell writing to the map at launch time
    ];

    for (comm, tier) in &classifications {
        let mut key = [0u8; 16];
        key[..comm.len()].copy_from_slice(comm.as_bytes());
        skel.maps_mut().task_class_map().update(&key, &tier.to_ne_bytes(), MapFlags::ANY)?;
    }

    let mut ops = skel.attach()?;

    // loisto-shell can update the map at runtime when launching a game:
    // write the emulator/game process name → DSQ_GAME

    // Block until scheduler is unloaded
    ops.run()?;
    Ok(())
}
```

### Dynamic classification

When loisto-shell launches a game, it writes the process name to the
BPF map so the scheduler knows to classify it as `DSQ_GAME`. This
avoids hardcoding every emulator name:

```rust
// In loisto-shell, after spawning emulator
fn register_game_process(comm: &str) -> Result<()> {
    // Write to /sys/fs/bpf/loisto/task_class_map via libbpf
    // or via a Unix socket to scx_loisto
    let key = format!("{:\0<16}", &comm[..comm.len().min(15)]);
    bpf_map_update(&key, &DSQ_GAME.to_ne_bytes())?;
    Ok(())
}
```

---

## Input Latency

### USB polling rate

Default USB polling is 125Hz (8ms). For all input devices, set 1000Hz (1ms)
via the Zig init tuning array (`jspoll=1`, `kbpoll=1`, `mousepoll=1`).

Also settable as kernel boot parameters: `usbhid.jspoll=1 usbhid.kbpoll=1 usbhid.mousepoll=1`

### Built-in HID drivers

By building `HID_SONY`, `HID_MICROSOFT`, `HID_NINTENDO`, `HID_STEAM`
directly into the kernel (not as modules), controllers are recognized
at device enumeration — no udev delay, no module loading.

### xpadneo (Xbox Wireless via Bluetooth)

The in-tree `xpad` driver doesn't support Xbox Series X/S controllers
via Bluetooth well. `xpadneo` is an out-of-tree driver that handles:
- Xbox Wireless protocol over Bluetooth
- Proper rumble motor mapping
- Trigger rumble
- Battery reporting

Include as a kernel patch or build as a module in a sysext.

---

## Audio Latency

PipeWire on our stack (no WirePlumber, no PulseAudio) runs with ALSA
backend directly. Kernel support needed:

| Config | Effect |
|--------|--------|
| `CONFIG_PREEMPT=y` | Preemptible kernel — audio threads can preempt anything |
| `CONFIG_HZ_1000=y` | 1ms timer resolution — matches PipeWire quantum |
| `CONFIG_HIGH_RES_TIMERS=y` | Sub-ms timer accuracy |
| `CONFIG_IRQ_FORCED_THREADING=y` | All IRQs in threads — can be scheduled/prioritized |
| `CONFIG_NO_HZ_FULL=y` | No timer ticks on cores running audio (with isolcpus) |

With these + sched-ext `DSQ_AUDIO` priority, PipeWire can maintain
stable ~5ms latency (1024 samples @ 48kHz) without xruns.

For pro audio use cases (music production, rhythm games), drop to
~2.7ms (128 samples @ 48kHz) — the sched-ext scheduler guarantees
the audio thread always runs within its deadline.

---

## Watchdog Integration

Our Zig init already acts as PID 1 supervisor. Kernel watchdog adds
hardware-level recovery for full system hangs:

### Software watchdog

```kconfig
CONFIG_SOFT_WATCHDOG=y
```

```zig
// In Zig init: open /dev/watchdog, write every 10s
fn watchdog_loop() void {
    const fd = std.posix.open("/dev/watchdog", .{ .ACCMODE = .WRONLY }, 0) catch return;
    while (true) {
        _ = std.posix.write(fd, "1") catch {};
        std.time.sleep(10 * std.time.ns_per_s);
    }
}
```

If the init crashes or the system locks up, the watchdog timer expires
after 30s (configurable) and reboots to the A/B fallback slot.

### RAUC integration

RAUC's boot assessment protocol uses a boot counter in the bootloader:
1. Fresh update → counter = 3
2. Each boot attempt decrements counter
3. If loisto-init reaches healthy state → mark slot "good" (counter = 0)
4. If 3 boots fail → RAUC switches to previous slot

The watchdog ensures even a hard lockup (kernel panic, GPU hang)
triggers a reboot, which decrements the counter and eventually
triggers rollback.

---

## Kernel Build Pipeline

### Directory structure

```
loisto-kernel/
├── PKGBUILD                    # Forked from CachyOS linux-cachyos-bore
├── config                      # Our .config overrides (merged on top of CachyOS base)
├── config-base                 # CachyOS's base config (pulled from their repo)
├── extra-patches/
│   └── 0001-xpadneo.patch      # Our additional patches (on top of CachyOS's)
├── sched/
│   ├── scx_loisto.bpf.c        # Custom sched-ext BPF scheduler
│   └── scx_loisto/              # Rust loader (Cargo project)
│       ├── Cargo.toml
│       └── src/main.rs
├── build.sh                    # Build wrapper (calls makepkg or direct make)
├── update-cachyos.sh           # Pull latest CachyOS PKGBUILD + patches
├── cmdline.txt                 # Kernel command line parameters
└── sysctl.conf                 # Reference (actual tuning lives in Zig init)
```

### Kernel command line (`cmdline.txt`)

```
mitigations=off
split_lock_detect=off
tsc=reliable
nowatchdog
quiet
loglevel=3
usbhid.jspoll=1 usbhid.kbpoll=1 usbhid.mousepoll=1
```

Embedded in the UKI (Unified Kernel Image) at build time via `ukify`.

### build.sh overview

```bash
#!/usr/bin/env bash
set -euo pipefail

JOBS=$(nproc)

# 1. Start from CachyOS's prepared source tree
#    (PKGBUILD handles: fetch kernel, apply CachyOS patches, apply BORE,
#     apply le9, apply HDR patches, apply sched-ext patches, etc.)
source PKGBUILD
prepare  # CachyOS's prepare() applies all their patches

# 2. Apply our additional patches on top
for patch in extra-patches/*.patch; do
    [ -f "$patch" ] && patch -p1 < "$patch"
done

# 3. Merge our config overrides onto CachyOS's base config
#    CachyOS config: PREEMPT=y, HZ_1000=y, BORE=y, SCX=y, LTO_CLANG_THIN=y
#    Our overrides: built-in HID, strip modules, dm-verity, debug off, etc.
#    Architecture: -march=ivybridge (current), switch to x86-64-v3 on upgrade
cp config-base .config
scripts/kconfig/merge_config.sh -m .config ../config
make olddefconfig

# 4. Build with Clang + ThinLTO (CachyOS default)
make CC=clang LD=ld.lld LLVM=1 LLVM_IAS=1 \
     -j"$JOBS" bzImage modules

# 5. Install to staging
INSTALL_PATH=../out/boot make install
INSTALL_MOD_PATH=../out make modules_install

# 6. Strip modules
find ../out/lib/modules -name '*.ko' -exec strip --strip-debug {} \;

echo "Kernel built: $(make kernelrelease)"
```

### update-cachyos.sh

```bash
#!/usr/bin/env bash
set -euo pipefail
# Pull latest CachyOS kernel source + patches
# Run this when CachyOS releases a new version

CACHYOS_REPO="https://github.com/CachyOS/linux-cachyos"
BRANCH="master"

# Fetch their PKGBUILD and patch set
curl -sL "${CACHYOS_REPO}/archive/refs/heads/${BRANCH}.tar.gz" | tar xz
cp linux-cachyos-${BRANCH}/linux-cachyos/PKGBUILD ./PKGBUILD.upstream
cp linux-cachyos-${BRANCH}/linux-cachyos/config ./config-base

# Show what changed
diff -u PKGBUILD PKGBUILD.upstream || true
echo "Review changes, update PKGBUILD, then run build.sh"
```

### Our config overrides (`config`)

This is a kconfig fragment merged on top of CachyOS's base. Only contains
our deviations — everything else inherits from CachyOS (BORE, sched-ext,
LTO, PREEMPT, HZ_1000, etc.):

```kconfig
# --- Built-in HID (zero-delay controller recognition) ---
CONFIG_HID=y
CONFIG_HID_SONY=y
CONFIG_HID_MICROSOFT=y
CONFIG_HID_NINTENDO=y
CONFIG_HID_STEAM=y
CONFIG_USB_HID=y
CONFIG_INPUT_JOYDEV=y
CONFIG_INPUT_EVDEV=y
CONFIG_INPUT_FF_MEMLESS=y

# --- dm-verity (immutable root) ---
CONFIG_DM_VERITY=y
CONFIG_DM_VERITY_VERIFY_ROOTHASH_SIG=y

# --- Strip debug/profiling ---
CONFIG_FTRACE=n
CONFIG_KPROBES=n
CONFIG_PROFILING=n
CONFIG_DEBUG_INFO_DWARF5=n

# --- I/O schedulers ---
CONFIG_IOSCHED_BFQ=y
CONFIG_IOSCHED_KYBER=y

# --- ZRAM ---
CONFIG_ZRAM=y
CONFIG_CRYPTO_LZ4=y

# --- NTSYNC (Wine/Proton) ---
CONFIG_NTSYNC=y

# --- Security ---
CONFIG_SECURITY_LANDLOCK=y
CONFIG_SECCOMP=y
CONFIG_SECCOMP_FILTER=y

# --- Filesystem ---
CONFIG_SQUASHFS=y
CONFIG_SQUASHFS_ZSTD=y
CONFIG_OVERLAY_FS=y
CONFIG_EROFS_FS=y

# --- Strip what we don't need ---
CONFIG_SOUND_OSS_CORE=n
CONFIG_NFS_FS=n
CONFIG_CIFS=n
CONFIG_9P_FS=n
CONFIG_INFINIBAND=n
CONFIG_HAMRADIO=n
CONFIG_CAN=n
CONFIG_ISDN=n
CONFIG_ATM=n
CONFIG_WIRELESS_EXT=n
CONFIG_STAGING=n
```

### CI integration

```
image build pipeline:
  1. Build kernel (loisto-kernel/build.sh) — CachyOS base + our overrides
  2. Build Zig init (loisto-init/)
  3. Build Rust apps (loisto-shell, scx_loisto, romhoard)
  4. Assemble rootfs (phases/)
  5. Generate dm-verity hash tree
  6. Build UKI with cmdline.txt (ukify)
  7. Assemble disk image (repart)
  8. Sign RAUC bundle
  9. Upload artifacts
```

Kernel builds are cached by `CACHYOS_VERSION + sha256(config extra-patches/*)`.
Only rebuilds when CachyOS updates, our config changes, or extra patches change.

### Update workflow

1. CachyOS releases new kernel (tracks upstream stable within days)
2. Run `update-cachyos.sh` — pulls new PKGBUILD + config-base
3. Review diff, resolve any conflicts with our overrides
4. Run `build.sh` — builds with our customizations on top
5. Test in QEMU/UTM
6. Push → CI builds full image → RAUC update bundle

---

## Decision Matrix

| Decision | Choice | Rationale |
|----------|--------|-----------|
| **Kernel base** | CachyOS PKGBUILD fork | BORE + sched-ext + LTO + AutoFDO for free. Well-maintained. |
| **Arch level** | x86-64-v2 + `-march=ivybridge` now; v3 on upgrade | 2012 Mac Mini is Ivy Bridge (no AVX2). v3 is the future target. |
| **Compiler** | Clang + ThinLTO | Better interactive performance than GCC -O2. kCFI bonus. |
| **Mitigations** | `mitigations=off` | ~5-15% gain. Appliance threat model is favorable. |
| **Version track** | Latest stable | GPU drivers and game compat require recent kernels. |
| **Fallback** | LTS kernel in RAUC B slot | Recovery option if latest stable has regressions. |

| Tier | Effort | Impact | When |
|------|--------|--------|------|
| **0: Kernel selection** | Fork PKGBUILD (~2 hours) | High — BORE + LTO + v3 from day 1 | Day 1 |
| **1: Sysctl tuning** | ~60 LOC Zig | High — 80% of runtime gains for free | Day 1 |
| **2: Config overrides** | ~50 line kconfig fragment | Medium — strip modules, built-in HID, dm-verity | Day 1 |
| **3: Boot params** | 1 line | Medium — mitigations=off, split_lock_detect=off | Day 1 |
| **4: Custom sched-ext** | ~200 LOC BPF+Rust, iterate at runtime | High — the "unfair advantage" | Month 1 |
| **5: HDR patches** | Already in CachyOS | Medium — just enable in config when needed | When HDR needed |

### Recommended order

1. Fork CachyOS PKGBUILD, apply our `.config` overrides, build (Day 1)
2. Zig init applies sysctl tuning at boot (Day 1)
3. Boot with `mitigations=off split_lock_detect=off tsc=reliable` (Day 1)
4. Iterate sched-ext BPF scheduler once the base system is stable (Month 1)
5. Enable HDR patches in config when HDR display support is requested
6. Evaluate AutoFDO profiling pipeline if build infra allows (nice-to-have)

---

## Sources

### Kernel variants
- [CachyOS kernel PKGBUILD](https://github.com/CachyOS/linux-cachyos) — our base
- [CachyOS kernel documentation](https://wiki.cachyos.org/features/kernel/)
- [CachyOS optimized repos](https://wiki.cachyos.org/features/optimized_repos/) — x86-64-v3/v4 packages
- [CachyOS AutoFDO blog](https://cachyos.org/blog/2411-kernel-autofdo/)
- [CachyOS sched-ext tutorial](https://wiki.cachyos.org/configuration/sched-ext/)
- [Zen kernel](https://github.com/zen-kernel/zen-kernel) — conservative alternative
- [Zen kernel feature list](https://github.com/zen-kernel/zen-kernel/wiki/Detailed-Feature-List)
- [Xanmod kernel](https://xanmod.org/)
- [Liquorix kernel](https://liquorix.net/)
- [Valve neptune kernel](https://github.com/ValveSoftware/steamos_kernel)
- [linux-hardened](https://github.com/anthraxx/linux-hardened)
- [Arch kernel wiki](https://wiki.archlinux.org/title/Kernel)

### Patches and schedulers
- [CachyOS kernel patches](https://github.com/CachyOS/kernel-patches)
- [BORE scheduler](https://github.com/firelzrd/bore-scheduler)
- [sched-ext / scx](https://github.com/sched-ext/scx)
- [scx_lavd (gaming scheduler)](https://github.com/sched-ext/scx/tree/main/scheds/rust/scx_lavd)
- [MGLRU (LWN)](https://lwn.net/Articles/894859/)
- [le9 patch](https://github.com/hakavlad/le9-patch)
- [xpadneo driver](https://github.com/atar-axis/xpadneo)
- [eBPF scheduler (LWN)](https://lwn.net/Articles/922405/)
- [zbpf — Zig eBPF framework](https://github.com/tw4452852/zbpf)

### Steam Deck tuning tools
- [SDWEAK](https://github.com/Taskerer/SDWEAK) — comprehensive optimization toolkit
- [linux-charcoal](https://github.com/V10lator/linux-charcoal) — SDWEAK's custom kernel
- [CryoUtilities](https://github.com/CryoByte33/steam-deck-utilities) — memory/swap tuning
- [CryoUtilities tweak explanations](https://github.com/CryoByte33/steam-deck-utilities/blob/main/docs/tweak-explanation.md)
- [ananicy-cpp](https://github.com/EvoXCX/ananicy-cpp) — process priority daemon
- [CachyOS ananicy-rules](https://github.com/CachyOS/ananicy-rules)

### Compiler and architecture
- [AutoFDO kernel docs](https://docs.kernel.org/dev-tools/autofdo.html)
- [GCC 15 vs Clang 20 benchmarks (Phoronix)](https://www.phoronix.com/review/clang20-gcc15-amd-znver5)
- [CachyOS x86-64-v3/v4 benchmarks (Phoronix)](https://www.phoronix.com/review/cachyos-x86-64-v3-v4)
- [Linux kernel LTS policy (2-year)](https://www.linux.org/threads/linux-kernel-lts-cut-from-6-years-to-2-years.46803/)
- [Linux kernel EOL dates](https://endoflife.date/linux)

### Subsystem documentation
- [Kyber I/O scheduler (LWN)](https://lwn.net/Articles/720675/)
- [Linux kernel sysctl docs](https://docs.kernel.org/admin-guide/sysctl/)
- [Linux amdgpu module params](https://docs.kernel.org/gpu/amdgpu/module-parameters.html)
- [DRM GPU scheduler](https://docs.kernel.org/gpu/drm-mm.html#gpu-scheduler)
- [Arch kernel compilation guide](https://wiki.archlinux.org/title/Kernel/Traditional_compilation)
- [PipeWire low latency guide](https://gitlab.freedesktop.org/pipewire/pipewire/-/wikis/Performance-tuning)
