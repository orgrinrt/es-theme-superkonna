# Immutable Appliance OS — Research

## TL;DR

Custom convention-based build system (shell scripts, not mkosi) producing a
**dm-verity protected `/usr/`** with **RAUC** (`efi` backend, CLI mode) for A/B
atomic delta updates and **sysext** for modular driver/emulator packs. Base
packages from Arch Linux via `pacstrap`. UKI via `ukify` directly. **No systemd
at runtime** — a custom **Zig init** (~300 LOC) is PID 1. No dbus. WiFi via
**iwd**, audio via **PipeWire standalone** (no WirePlumber). No mkosi, no Python
at runtime. Boot directly to gamescope on TTY. Entire OS defined in git as
`image.conf` + phased build scripts, built in GitHub Actions, produces a
flashable `.img` under 500MB.

---

## Design Goals

| Goal | Meaning |
|------|---------|
| **Immutable root** | Read-only base filesystem, no runtime modifications |
| **Atomic updates** | Whole-image swap with A/B slots, automatic rollback on failure |
| **No escape hatch** | No TTY login, no shell, no desktop for end users |
| **No Python** | Batocera's configgen is Python; we replace it with Rust |
| **Console-like UX** | Download update → verify → reboot → done (like Switch/PS5/Xbox) |
| **Minimal footprint** | Base image under 500MB, idle RAM under 300MB |
| **Dev mode** | Hidden toggle enables SSH + TTY2 for developers |
| **CI-built** | `git push` → GitHub Actions → flashable image |

---

## Boot Architecture: gamescope on TTY

Regardless of which approach is chosen, the boot-to-app flow is the same. gamescope
runs as an embedded compositor directly on TTY1 using DRM/KMS — no display manager,
no Xorg, no Wayland compositor chain.

```
BIOS/UEFI → systemd-boot → kernel → Zig init (PID 1)
    → gamescope --backend drm -- loisto-shell
```

### Zig init (PID 1)

A custom Zig init (~300 LOC) replaces systemd as PID 1. It handles:

- Mount pseudofilesystems (`/proc`, `/sys`, `/dev`, `/tmp`)
- Mount persistent partitions (`/var`, `/data`)
- Bind-mount persistent state dirs (iwd, bluetooth)
- Set hostname, seed urandom
- Start `pipewire` (audio)
- Start `iwd` + `dhcpcd` (networking)
- Start `gamescope --backend drm -- loisto-shell` as the session
- Reap zombies (PID 1 responsibility)
- Handle SIGTERM/SIGINT for clean shutdown

No systemd, no dbus, no service manager. The init starts a fixed set of
processes in a fixed order. See INIT.md (pending) for the full design.

### Lockdown — disable all escape hatches

No gettys, no emergency shells, no rescue mode — these simply do not exist.
The Zig init only starts the processes listed above. There is no shell, no
login prompt, and no service manager that could spawn them. Ctrl+Alt+Del is
ignored (the init does not register a handler for it).

### Dev mode toggle

A hidden key combo in loisto-shell sets a flag in `/data/system/dev-mode`. On next
boot, the Zig init checks that flag and spawns `getty` on TTY2 + starts an SSH
daemon. Another combo disables it. This provides developer access without
compromising the end-user experience.

---

## Approach Comparison

Eight approaches were evaluated. Here's the summary:

| Approach | Image Size | Build Complexity | Updates | Rollback | Runtime Python |
|----------|-----------|-----------------|---------|----------|---------------|
| **Raw squashfs + RAUC** | ~300MB | Low | Good (A/B + casync delta) | Good | No |
| **mkosi + systemd ecosystem** | ~400MB | Medium | Good (sysupdate) | Good (boot assessment) | No |
| **NixOS + impermanence** | ~600MB | High (Nix lang) | Good (generations) | Excellent | No |
| **Buildroot** | ~200MB | Medium-High | Basic | Basic | No |
| **OSTree / rpm-ostree** | ~1GB | Medium | Excellent (delta) | Excellent | Yes (Fedora base) |
| **Yocto** | ~300MB | High | Good (RAUC/SWUpdate) | Good | No |
| **Container-based** | N/A | N/A | N/A | N/A | N/A (wrong workload) |
| **sysext (addon only)** | N/A | Low | Per-extension | Per-extension | No |

---

## Recommended: Raw squashfs + RAUC

### Why this approach

This is what SteamOS and Batocera both do, simplified and stripped to essentials.
The mental model is dead simple: the entire OS is a single compressed file. Updates
swap that file. Rollback restores the previous file. RAUC handles the hard parts
(signing, verification, slot management, rollback logic).

### Partition layout

```
GPT Partition Table:
  #1  ESP          256MB    FAT32     Bootloader + kernel + initrd
  #2  rootfs-A     1.5GB    (raw)     Active squashfs image
  #3  rootfs-B     1.5GB    (raw)     Inactive slot (for updates)
  #4  var          256MB    ext4      System state (logs, machine-id, iwd)
  #5  data         remainder ext4     ROMs, saves, BIOS, config

Total OS overhead: ~3.5GB
Remaining: user data
```

**Decision**: The 5-partition layout above is what RAUC A/B needs and is the
committed design. A simpler 2-partition variant was considered but rejected
because RAUC's `efi` bootloader backend requires dedicated slot partitions.

### How immutability works

```
kernel → initramfs:
  1. mount squashfs as /lower (read-only)
  2. mount tmpfs as /upper (writable, in-memory)
  3. mount overlayfs: lower=/lower, upper=/upper → merged root
  4. mount /data partition for persistent storage
  5. bind-mount persistent dirs (iwd, Bluetooth)
  6. switch_root → Zig init → gamescope → loisto-shell
```

All system changes are in-memory (tmpfs upper) and lost on reboot. Only `/data/`
persists. This means:

- Rebooting is a factory reset of the system state
- User data (ROMs, saves, configs) is never touched by updates
- A corrupted system state is fixed by rebooting

### The initramfs init script

The initramfs uses a busybox shell script (or a small Zig binary) to mount
filesystems, verify dm-verity, and switch_root into the real rootfs where the
Zig init takes over as PID 1.

```bash
#!/bin/busybox sh
# /init inside the initramfs

/bin/busybox mount -t proc proc /proc
/bin/busybox mount -t sysfs sysfs /sys
/bin/busybox mount -t devtmpfs devtmpfs /dev

/bin/busybox modprobe squashfs
/bin/busybox modprobe overlay
/bin/busybox modprobe ext4
/bin/busybox modprobe vfat
/bin/busybox modprobe dm-verity 2>/dev/null

# Mount boot partition (contains squashfs)
/bin/busybox mkdir -p /boot
/bin/busybox mount -t vfat /dev/disk/by-label/LOISTO-BOOT /boot

# Determine which slot to boot (RAUC efi backend reads EFI variables)
SLOT=$(/bin/busybox cat /boot/active-slot 2>/dev/null || echo "a")

# dm-verity verification (veritysetup is standalone, no systemd needed)
# veritysetup open /dev/disk/by-partlabel/rootfs-${SLOT} verified-root \
#     /dev/disk/by-partlabel/rootfs-${SLOT}-verity $ROOTHASH

# Mount squashfs as read-only lower
/bin/busybox mkdir -p /lower
/bin/busybox mount -t squashfs -o ro "/boot/loisto-${SLOT}.squashfs" /lower

# Writable upper layer (tmpfs, in-memory)
/bin/busybox mkdir -p /upper /work
/bin/busybox mount -t tmpfs -o size=256M tmpfs /upper
/bin/busybox mkdir -p /upper/upper /upper/work

# Overlay root
/bin/busybox mkdir -p /newroot
/bin/busybox mount -t overlay overlay \
    -o lowerdir=/lower,upperdir=/upper/upper,workdir=/upper/work \
    /newroot

# Persistent data
/bin/busybox mkdir -p /newroot/data
/bin/busybox mount -t ext4 /dev/disk/by-label/LOISTO-DATA /newroot/data

# Bind persistent state (iwd for WiFi, bluez for Bluetooth)
for dir in var/lib/iwd var/lib/bluetooth; do
    /bin/busybox mkdir -p "/newroot/data/system/$dir" "/newroot/$dir"
    /bin/busybox mount --bind "/newroot/data/system/$dir" "/newroot/$dir"
done

# switch_root → Zig init (PID 1)
# Dev mode flag is checked by the Zig init, not here
exec /bin/busybox switch_root /newroot /sbin/init
```

### Building the squashfs image

```bash
#!/bin/bash
# build-rootfs.sh — produce the loisto OS image
set -euo pipefail

ROOTFS="/tmp/loisto-rootfs"
OUTPUT="loisto.squashfs"

# Bootstrap minimal Arch
mkdir -p "$ROOTFS"
pacstrap -c "$ROOTFS" \
    base linux linux-firmware \
    mesa vulkan-radeon vulkan-intel libva-mesa-driver intel-media-driver \
    pipewire pipewire-alsa \
    gamescope libinput \
    iwd dhcpcd bluez \
    mpv \
    rauc \
    cryptsetup     # provides veritysetup

# Install our binaries
install -m755 target/release/loisto-shell    "$ROOTFS/usr/bin/"
install -m755 target/release/loisto-updater  "$ROOTFS/usr/bin/"
install -m755 target/release/loisto-configgen "$ROOTFS/usr/bin/"
install -m755 target/release/loisto-frontend "$ROOTFS/usr/bin/"
install -m755 zig-out/bin/loisto-init        "$ROOTFS/sbin/init"

# Install libretro cores
mkdir -p "$ROOTFS/usr/lib/libretro"
cp cores/*.so "$ROOTFS/usr/lib/libretro/"

# Install standalone emulators
cp emulators/* "$ROOTFS/usr/bin/"

# No systemd — the Zig init is installed as /sbin/init above.
# No gettys, no service units, no masking needed.

# Strip unnecessary files
rm -rf "$ROOTFS"/usr/share/{doc,man,info,locale,i18n,gtk-doc}
rm -rf "$ROOTFS"/usr/lib/python*
rm -rf "$ROOTFS"/usr/lib/perl*
rm -rf "$ROOTFS"/var/cache/pacman
rm -rf "$ROOTFS"/usr/include

# Build squashfs with maximum compression
mksquashfs "$ROOTFS" "$OUTPUT" \
    -comp zstd \
    -Xcompression-level 19 \
    -b 256K \
    -no-exports \
    -noappend

echo "Built: $OUTPUT ($(du -h "$OUTPUT" | cut -f1))"
```

### Building the bootable disk image

```bash
#!/bin/bash
# build-disk.sh — produce a flashable .img
set -euo pipefail

IMG="loisto.img"

truncate -s 8G "$IMG"

sgdisk -Z "$IMG"
sgdisk -n 1:0:+256M  -t 1:EF00 -c 1:"LOISTO-BOOT" "$IMG"
sgdisk -n 2:0:+1536M -t 2:8300 -c 2:"rootfs-A" "$IMG"
sgdisk -n 3:0:+1536M -t 3:8300 -c 3:"rootfs-B" "$IMG"
sgdisk -n 4:0:+256M  -t 4:8300 -c 4:"LOISTO-VAR" "$IMG"
sgdisk -n 5:0:0      -t 5:8300 -c 5:"LOISTO-DATA" "$IMG"

LOOP=$(losetup -Pf --show "$IMG")

mkfs.vfat -n "LOISTO-BOOT" "${LOOP}p1"
# Partitions 2+3: written as raw squashfs images, not formatted
dd if=loisto.squashfs of="${LOOP}p2" bs=4M
mkfs.ext4 -L "LOISTO-VAR" "${LOOP}p4"
mkfs.ext4 -L "LOISTO-DATA" "${LOOP}p5"

# Populate boot partition
mkdir -p /tmp/boot-mnt
mount "${LOOP}p1" /tmp/boot-mnt

cp vmlinuz /tmp/boot-mnt/
cp initramfs.img /tmp/boot-mnt/
echo "a" > /tmp/boot-mnt/active-slot

# systemd-boot
mkdir -p /tmp/boot-mnt/EFI/BOOT /tmp/boot-mnt/loader/entries
cp /usr/lib/systemd/boot/efi/systemd-bootx64.efi /tmp/boot-mnt/EFI/BOOT/BOOTX64.EFI

cat > /tmp/boot-mnt/loader/entries/loisto.conf << 'EOF'
title   Loisto
linux   /vmlinuz
initrd  /initramfs.img
options quiet loglevel=0 vt.global_cursor_default=0
EOF

umount /tmp/boot-mnt

# Create default data structure
mkdir -p /tmp/data-mnt
mount "${LOOP}p5" /tmp/data-mnt
mkdir -p /tmp/data-mnt/{roms,saves,bios,config,system/{var/lib/iwd,var/lib/bluetooth}}
umount /tmp/data-mnt

losetup -d "$LOOP"
echo "Image built: $IMG — flash with: dd if=$IMG of=/dev/sdX bs=4M status=progress"
```

### Update flow

RAUC manages the A/B slots:

```ini
# /etc/rauc/system.conf
[system]
compatible=loisto-amd64
bootloader=efi
variant-name=loisto

[keyring]
path=/etc/rauc/keyring.pem

[slot.rootfs.0]
device=/dev/disk/by-partlabel/rootfs-A
type=raw
bootname=A

[slot.rootfs.1]
device=/dev/disk/by-partlabel/rootfs-B
type=raw
bootname=B
```

From the user's perspective:

```
Loisto shell shows: "System update available (v1.3.0)"
    ↓
User presses A to install
    ↓
loisto-updater downloads loisto-v1.3.0.raucb
    ↓
rauc install loisto-v1.3.0.raucb (CLI one-shot, no dbus)
    ↓
RAUC verifies signature, writes to inactive slot, updates EFI variables
    ↓
Reboot prompt → system boots into new version
    ↓
If boot fails → RAUC efi backend reads EFI variables, rolls back to previous slot
```

Build-side:

```bash
# Create RAUC bundle for distribution
rauc bundle \
    --cert=signing-cert.pem \
    --key=signing-key.pem \
    bundle-dir/ \
    loisto-update-v1.3.0.raucb
```

### Delta updates with casync

Full squashfs downloads are ~300MB. casync chunks the image into content-addressed
blocks, so updates only download changed chunks (~20-50MB typical):

```bash
# Server: create casync chunk index alongside the image
casync make loisto.squashfs.caibx loisto.squashfs \
    --store=https://updates.loisto.dev/chunks/

# Client: download only changed chunks
casync extract loisto.squashfs.caibx /dev/disk/by-partlabel/rootfs-B \
    --seed=/dev/disk/by-partlabel/rootfs-A \
    --store=https://updates.loisto.dev/chunks/
```

### State management

| Path | Partition | Persistence | Contents |
|------|-----------|-------------|----------|
| `/` | overlayfs (squashfs + tmpfs) | Lost on reboot | System runtime state |
| `/var/` | ext4 (var partition) | Survives reboot | Logs, machine-id, iwd networks |
| `/data/roms/` | ext4 (data partition) | Permanent | Game ROMs |
| `/data/saves/` | ext4 (data partition) | Permanent | Save states, SRAM |
| `/data/bios/` | ext4 (data partition) | Permanent | BIOS/firmware files |
| `/data/config/` | ext4 (data partition) | Permanent | User preferences |
| `/data/media/` | ext4 (data partition) | Permanent | Downloaded media |
| `/data/cache/` | ext4 (data partition) | Clearable | Metadata cache, thumbnails |

Updates never touch `/data/`. A factory reset wipes `/var/` and
`/data/config/` but preserves ROMs, saves, and BIOS files.

---

## Alternative: mkosi + systemd Ecosystem

If the raw approach proves too manual, mkosi provides a structured way to build
the same kind of image with dm-verity, Unified Kernel Images (UKI), and
systemd-sysupdate out of the box.

### Configuration

```ini
# mkosi.conf
[Distribution]
Distribution=arch

[Output]
ImageId=loisto
ImageVersion=1.0.0
Format=disk
CompressOutput=zstd
Verity=yes
UnifiedKernelImages=yes

[Content]
Packages=
    base
    linux
    linux-firmware
    mesa vulkan-radeon vulkan-intel libva-mesa-driver
    pipewire pipewire-alsa
    gamescope libinput
    iwd bluez
    mpv

RemovePackages=python perl man-db
CleanPackageMetadata=yes
RemoveFiles=/usr/share/doc /usr/share/man /usr/share/info /usr/share/locale

Bootable=yes
Bootloader=systemd-boot
```

```bash
mkosi build   # → loisto_1.0.0.raw (GPT image with dm-verity)
mkosi qemu    # → test in VM
```

### systemd-sysupdate handles A/B

```ini
# /usr/lib/sysupdate.d/loisto.conf
[Transfer]
ProtectVersion=%A

[Source]
Type=url-file
Path=https://updates.loisto.dev/
MatchPattern=loisto_@v.usr.raw.zst

[Target]
Type=partition
Path=auto
MatchPattern=loisto_@v
MatchPartitionType=usr
```

### Boot assessment (automatic rollback)

RAUC's `efi` bootloader backend handles boot assessment by reading/writing EFI
variables directly (no systemd boot counters). After an update, RAUC marks the
new slot as "pending". On successful boot, `rauc status mark-good` confirms it.
If the system fails to mark-good, the next boot falls back to the previous slot.

### Tradeoffs vs raw squashfs

| Aspect | Raw squashfs + RAUC | mkosi + systemd |
|--------|--------------------|-----------------|
| Image size | ~300MB | ~400-600MB |
| Build tool | Shell scripts | mkosi (Python) |
| Update mgmt | RAUC | systemd-sysupdate |
| Verified boot | Optional (add dm-verity later) | Built-in (dm-verity + UKI) |
| Rollback | RAUC efi backend (EFI variables) | systemd-boot assessment |
| Partitioning | Build-time (systemd-repart at build) | systemd-repart (build or first boot) |
| Learning curve | Low | Medium (systemd ecosystem) |

---

## Alternative: NixOS + Impermanence

The most principled approach. The entire OS is declared in `.nix` files —
`configuration.nix` is the single source of truth.

### Key configuration

```nix
{
  # Root is tmpfs — wiped every boot
  fileSystems."/" = {
    device = "none";
    fsType = "tmpfs";
    options = [ "defaults" "size=512M" "mode=755" ];
  };

  # Impermanence: what survives reboots
  environment.persistence."/data/persist" = {
    directories = [
      "/var/lib/iwd"
      "/var/lib/bluetooth"
      "/var/log"
    ];
    files = [ "/etc/machine-id" ];
  };

  # Boot straight to gamescope
  systemd.services.loisto-session = {
    after = [ "multi-user.target" ];
    wantedBy = [ "graphical.target" ];
    serviceConfig = {
      User = "loisto";
      ExecStart = "${pkgs.gamescope}/bin/gamescope --backend drm -- loisto-shell";
      Restart = "on-failure";
    };
  };

  # Strip everything unnecessary
  documentation.enable = false;
  services.xserver.enable = false;
}
```

### Tradeoffs

- **Strongest guarantee**: same config = same output, every time, everywhere
- **Best rollback**: every generation kept until garbage collected
- **Steepest learning curve**: Nix language is unfamiliar to most
- **Largest images**: ~600MB+ due to Nix store overhead
- **GPU drivers can be tricky** on NixOS (Mesa/Vulkan configuration)

---

## Modular Extensions with sysext

Regardless of base approach, **sysext** enables modular updates to `/usr/`
without rebuilding the entire image. Despite the `systemd-` prefix, sysext
works standalone — it just needs erofs/squashfs + overlayfs (kernel features):

```bash
# Build a sysext image for Mesa driver updates
mkdir -p mesa-ext/usr/lib64/dri
cp updated-mesa-libs/* mesa-ext/usr/lib64/dri/
mksquashfs mesa-ext /var/lib/extensions/mesa-gpu.raw -comp zstd

# Build a sysext for an emulator pack
mkdir -p emu-ext/usr/lib/libretro
cp updated-cores/*.so emu-ext/usr/lib/libretro/
mksquashfs emu-ext /var/lib/extensions/emulators.raw -comp zstd

# Activate
systemd-sysext merge
```

This means we can ship:
- **Base image** (kernel, Zig init, gamescope, loisto binaries) — updates rarely
- **GPU driver extension** — updates when Mesa releases
- **Emulator pack extension** — updates when cores/emulators release
- **Media extension** — mpv + codecs

Each extension updates independently without touching the base. Extensions can be
dm-verity signed for integrity.

---

## Update Delivery Options

| Method | Bandwidth | Complexity | Notes |
|--------|-----------|------------|-------|
| Whole image | ~300MB per update | Low | Simple, works everywhere |
| casync chunks | ~20-50MB typical | Medium | Content-addressed delta, SteamOS uses this |
| bsdiff/zchunk | ~20-50MB typical | Medium | Binary diff, RAUC supports this |
| OSTree static deltas | ~10-30MB typical | High | Best compression, complex infrastructure |

**Progression:**
1. **MVP**: Whole image download (~300MB). Simple, reliable.
2. **V2**: casync for delta updates (~20-50MB). Good enough.
3. **V3**: If needed, bsdiff or zchunk for even smaller deltas.

**Rugix** (Rust-native OTA framework) is worth watching — it handles delta updates
for embedded Linux and aligns with loisto's Rust stack.

---

## Recovery

### If both A/B slots are bad

**Option 1: USB recovery** (recommended for V1)
- Flash a recovery image to USB
- Boot from USB, reflash internal storage
- Takes no disk space, requires physical access

**Option 2: Recovery partition** (for V2)
- 50MB partition with minimal kernel + initrd + recovery squashfs
- Can download and flash a new OS image over network
- Automatic: if both slots fail boot counter, chainload recovery

### Factory reset

- Delete `/data/config/` and `/var/` contents
- Reboot → system returns to first-boot wizard
- ROMs, saves, BIOS files preserved (unless full wipe requested)

---

## First-Boot Flow

```
1. Flash image to storage (dd, Etcher, or custom installer)
2. First boot:
   a. Kernel + initramfs load
   b. initramfs checks /data partition:
      - If missing: format with default directory structure
      - If present: mount as-is
   c. switch_root → Zig init → gamescope session
   d. loisto-shell detects no /data/config/setup-complete:
      - Launch first-boot wizard (inside gamescope)
      - WiFi setup, controller pairing, language, timezone
      - Write /data/config/setup-complete
   e. Normal operation begins
```

---

## Driver Stack

All approaches use the same upstream packages:

| Component | Package (Arch) | Size (approx) |
|-----------|---------------|----------------|
| Kernel + firmware | `linux` + `linux-firmware` | ~100MB (trimmable to ~60MB) |
| Mesa + Vulkan (AMD) | `mesa` + `vulkan-radeon` | ~60MB |
| Mesa + Vulkan (Intel) | `mesa` + `vulkan-intel` | ~50MB |
| VA-API | `libva-mesa-driver` / `intel-media-driver` | ~20MB |
| NVIDIA (proprietary) | `nvidia` | ~200MB |
| PipeWire | `pipewire` + `pipewire-alsa` | ~15MB |
| libinput | `libinput` | ~5MB |
| Bluetooth | `bluez` (transient, for pairing only) | ~10MB |
| Networking | `iwd` + `dhcpcd` | ~5MB |

Batocera ships the exact same upstream packages compiled into Buildroot.
There is zero driver advantage to using Batocera as a base.

**Firmware trimming**: `linux-firmware` is ~800MB uncompressed. For a
known-hardware image (e.g., only AMD GPUs), strip to just `amdgpu/` firmware
(~30MB). For a generic image, keep all firmware but compress the squashfs.

---

## CI Pipeline

```yaml
# .github/workflows/build.yml
name: Build Loisto Image
on:
  push:
    tags: ['v*']

jobs:
  build:
    runs-on: ubuntu-latest
    container:
      image: archlinux:latest
      options: --privileged  # needed for losetup, mount

    steps:
      - uses: actions/checkout@v4

      - name: Install build tools
        run: pacman -Sy --noconfirm arch-install-scripts squashfs-tools gptfdisk dosfstools e2fsprogs

      - name: Build Rust binaries
        run: |
          # Cross-compile or build in container
          cargo build --release

      - name: Build squashfs
        run: ./build-rootfs.sh

      - name: Build disk image
        run: ./build-disk.sh

      - name: Upload release
        uses: softprops/action-gh-release@v1
        with:
          files: loisto.img.zst
```

---

## gamescope Integration Detail

### The STEAM_GAME X11 property

gamescope uses the `STEAM_GAME` X11 property to identify the primary application.
Without it, gamescope may not give focus or may show a blank screen.

```rust
// Set STEAM_GAME property on our X11 window
use x11rb::protocol::xproto::*;

let steam_game_atom = conn.intern_atom(false, b"STEAM_GAME")?.reply()?.atom;
conn.change_property32(
    PropMode::REPLACE,
    window_id,
    steam_game_atom,
    AtomEnum::CARDINAL,
    &[769],  // any non-zero value; 769 is conventional for non-Steam apps
)?;
```

### Session script

```bash
#!/bin/bash
# /usr/share/gamescope-session.d/loisto
export CLIENTCMD="/usr/bin/loisto-shell"

# GPU detection
if lspci | grep -qi nvidia; then
    export GAMESCOPE_ARGS="--backend drm --prefer-vk-device nvidia"
else
    export GAMESCOPE_ARGS="--backend drm"
fi

export PIPEWIRE_RUNTIME_DIR=/run/user/$(id -u)
```

---

## Size Budget

| Component | Compressed (squashfs zstd) |
|-----------|---------------------------|
| Kernel | ~10MB |
| Firmware (trimmed AMD+Intel) | ~30MB |
| Mesa + Vulkan + VA-API | ~50MB |
| PipeWire + pipewire-alsa | ~8MB |
| Zig init + base utilities | ~5MB |
| gamescope | ~5MB |
| iwd + dhcpcd + bluez | ~8MB |
| mpv | ~8MB |
| Loisto binaries (all) | ~20MB |
| Libretro cores (30 cores) | ~80MB |
| Standalone emulators (5) | ~40MB |
| **Total** | **~250MB** |

With all firmware (for generic hardware support): **~350-450MB**.

---

## Implementation Phases

1. **Phase 1: VM prototype** — Manual Arch install in QEMU, configure gamescope
   session, validate boot-to-app flow works

2. **Phase 2: Build scripts** — `build-rootfs.sh` + `build-disk.sh`, produce
   flashable `.img` from CI

3. **Phase 3: Real hardware** — Test on Steam Deck, mini PC, or similar.
   Validate drivers, controllers, audio, WiFi, Bluetooth

4. **Phase 4: RAUC integration** — A/B slots, signed updates, automatic rollback

5. **Phase 5: sysext** — Modular driver and emulator pack updates

6. **Phase 6: Delta updates** — casync or similar for bandwidth-efficient updates

7. **Phase 7: Installer** — Simple TUI for first-time setup (partition, flash, configure)

---

## Sources

- [SteamOS partition teardown](https://github.com/randombk/steamos-teardown/blob/master/docs/partitions.md)
- [How I forked SteamOS (iliana.fyi)](https://iliana.fyi/blog/build-your-own-steamos-updates/)
- [Fitting Everything Together (Lennart Poettering)](https://0pointer.net/blog/fitting-everything-together.html)
- [mkosi — systemd image builder](https://github.com/systemd/mkosi)
- [mkosi reintroduction](https://0pointer.net/blog/a-re-introduction-to-mkosi-a-tool-for-generating-os-images.html)
- [RAUC — robust auto-update controller](https://github.com/rauc/rauc)
- [Rugix — Rust OTA for embedded Linux](https://github.com/rugix/rugix)
- [casync — content-addressable data sync](https://github.com/systemd/casync)
- [systemd-sysext documentation](https://www.freedesktop.org/software/systemd/man/latest/systemd-sysext.html)
- [systemd-sysupdate documentation](https://www.freedesktop.org/software/systemd/man/latest/systemd-sysupdate.html)
- [NixOS impermanence](https://wiki.nixos.org/wiki/Impermanence)
- [Nixiosk — NixOS kiosk appliances](https://github.com/matthewbauer/nixiosk)
- [Batocera architecture](https://wiki.batocera.org/batocera.linux_architecture)
- [Buildroot manual](https://buildroot.org/downloads/manual/manual.html)
- [Yocto read-only rootfs](https://docs.yoctoproject.org/5.0.6/dev-manual/read-only-rootfs.html)
- [Flatcar sysext](https://www.flatcar.org/blog/2024/04/os-innovation-with-systemd-sysext/)
- [gamescope](https://github.com/ValveSoftware/gamescope)
- [gamescope-session](https://github.com/ChimeraOS/gamescope-session)
- [ChimeraOS](https://github.com/ChimeraOS/chimeraos)
- [frzr update system](https://github.com/ChimeraOS/frzr)
- [dm-verity (ArchWiki)](https://wiki.archlinux.org/title/Dm-verity)
- [verity-squash-root](https://github.com/brandsimon/verity-squash-root)
- [Universal Blue image template](https://github.com/ublue-os/image-template)
- [Bazzite](https://github.com/ublue-os/bazzite)
- [Kairos](https://kairos.io/)
- [gamescope TTY setup guide](https://github.com/shahnawazshahin/steam-using-gamescope-guide)

---

## Addendum: Final Architecture Decision

### What we chose

**squashfs + dm-verity + RAUC**, with a **custom convention-based build system**
inspired by mkosi's structure but implemented as plain shell scripts.

We do NOT use mkosi. We borrow its *design paradigm*:

- Convention-based directory layout (predictable, auditable)
- Split build phases (each phase is a standalone script)
- dm-verity for cryptographic rootfs integrity
- systemd-repart configs for BUILD TIME partitioning (just INI files, not runtime)
- UKI optionally via `ukify` directly (it's a standalone tool)
- sysext for modular driver/emulator extensions (kernel overlayfs, no systemd needed)

We pair this with RAUC for updates because:

- Delta updates via block-hash-index (systemd-sysupdate has none)
- 8+ years of production deployments in industrial/automotive/medical
- CLI mode (`rauc install`) — no dbus daemon needed at runtime
- `efi` bootloader backend reads/writes EFI variables directly for boot assessment
- Recovery partition support built-in

### What we borrow from mkosi (the paradigm, not the tool)

| mkosi concept | Our implementation |
|---------------|-------------------|
| `mkosi.conf` (declarative packages) | `image.conf` — shell-sourceable config (`PACKAGES=`, `STRIP_PATHS=`, etc.) |
| `mkosi.extra/` (overlay directory) | `overlay/` — files copied verbatim into rootfs |
| `mkosi.repart/` (partition definitions) | `repart.d/` — real systemd-repart configs, used at first boot |
| `mkosi.postinst.chroot` | `hooks/post-install.sh` — runs in chroot after pacstrap |
| `mkosi build` | `./build.sh` — sources `image.conf`, runs phases in order |
| `mkosi qemu` | `./test-vm.sh` — launches QEMU (Linux) or UTM (macOS) with the built image |
| `mkosi shell` | `./enter-chroot.sh` — chroots into the built rootfs for inspection |
| Split artifacts | Build produces: `rootfs.img`, `rootfs.verity`, `rootfs.verity-sig`, `loisto.efi` (UKI) |

### Build system layout

```
loisto-image/
├── image.conf                  # Package list, version, strip paths, image ID
├── build.sh                    # Master build script — sources image.conf, runs phases
├── test-vm.sh                  # Launch built image in QEMU/UTM
├── enter-chroot.sh             # Interactive chroot into rootfs for debugging
├── bundle.sh                   # Create RAUC bundle from split artifacts
│
├── phases/                     # Each phase is a standalone script
│   ├── 00-bootstrap.sh         # pacstrap base packages into $ROOTFS
│   ├── 10-install-packages.sh  # Install additional packages from image.conf
│   ├── 20-install-loisto.sh    # Copy our Rust binaries, cores, emulators
│   ├── 30-configure.sh         # Create users, install Zig init as /sbin/init
│   ├── 40-strip.sh             # Remove docs, man pages, includes, firmware trim
│   ├── 50-verity.sh            # veritysetup format → rootfs.verity + root hash
│   ├── 60-uki.sh               # ukify build → loisto.efi (embed verity root hash)
│   └── 70-disk.sh              # Assemble final GPT disk image
│
├── overlay/                    # Files copied verbatim into rootfs
│   └── usr/
│       ├── bin/
│       │   └── loisto-shell    # (copied from cargo build output)
│       ├── lib/
│       │   └── libretro/       # .so cores
│       └── share/
│           └── factory/
│               └── etc/        # Factory defaults for /etc
│
├── repart.d/                   # systemd-repart configs (used at IMAGE BUILD TIME only)
│   ├── 00-esp.conf
│   ├── 10-usr-a.conf
│   ├── 11-usr-a-verity.conf
│   ├── 12-usr-a-verity-sig.conf
│   ├── 20-usr-b.conf           # Inactive slot for updates
│   ├── 21-usr-b-verity.conf
│   ├── 22-usr-b-verity-sig.conf
│   ├── 30-var.conf
│   └── 40-data.conf            # Grows to fill remaining space
│
├── rauc/                       # RAUC configuration
│   ├── system.conf             # Slot definitions, bootloader type
│   └── keyring.pem             # Public key for bundle verification
│
├── sysext/                     # Extension image build configs
│   ├── gpu-mesa.conf           # Mesa/Vulkan driver extension
│   ├── emulators.conf          # Libretro cores + standalone emulators
│   └── build-sysext.sh         # Builds .raw extension images
│
└── keys/                       # Signing keys (gitignored, CI secrets)
    ├── rauc-release.key
    ├── rauc-release.crt
    ├── secureboot.key
    └── secureboot.crt
```

### image.conf

Shell-sourceable, not INI. Keeps it simple and directly usable by the phase scripts.

```bash
# loisto-image/image.conf

IMAGE_ID="loisto"
IMAGE_VERSION="0.1.0"
ARCH="x86_64"
COMPATIBLE="loisto-console"    # RAUC compatible string

# Base packages (Arch)
PACKAGES=(
    base linux linux-firmware
    # GPU
    mesa vulkan-radeon vulkan-intel libva-mesa-driver intel-media-driver
    # Audio (PipeWire standalone, no WirePlumber, no PulseAudio compat)
    pipewire pipewire-alsa
    # Compositor
    gamescope
    # Input
    libinput
    # Network (iwd for WiFi, dhcpcd for DHCP, no NetworkManager)
    iwd dhcpcd bluez
    # Media
    mpv
    # Updates
    rauc
    # Verity + boot tools
    cryptsetup     # provides veritysetup
)

# Paths to strip from rootfs
STRIP_PATHS=(
    usr/share/doc
    usr/share/man
    usr/share/info
    usr/share/locale
    usr/share/i18n
    usr/share/gtk-doc
    usr/include
    usr/lib/python*
    usr/lib/perl*
    var/cache/pacman
)

# Firmware to keep (everything else stripped from linux-firmware)
# Empty = keep all firmware
FIRMWARE_KEEP=(
    amdgpu
    i915
    iwlwifi
    rtl_nic
    rtlwifi
    ath10k
    ath11k
    brcm
    intel
)

# Partition sizes
ESP_SIZE="256M"
USR_SIZE="2G"
VAR_SIZE="256M"
# DATA_SIZE = remainder
```

### Master build script

```bash
#!/bin/bash
# loisto-image/build.sh
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/image.conf"

export ROOTFS="${ROOTFS:-/tmp/loisto-rootfs-$$}"
export OUTPUT="${OUTPUT:-$SCRIPT_DIR/output}"
export SCRIPT_DIR

mkdir -p "$OUTPUT"

# Run each phase in order
for phase in "$SCRIPT_DIR"/phases/[0-9]*.sh; do
    echo "=== Phase: $(basename "$phase") ==="
    bash "$phase"
done

echo "=== Build complete ==="
echo "  Disk image: $OUTPUT/${IMAGE_ID}-${IMAGE_VERSION}.img"
echo "  RAUC bundle: $OUTPUT/${IMAGE_ID}-${IMAGE_VERSION}.raucb"
```

### Key phase examples

**Phase 00: Bootstrap**

```bash
#!/bin/bash
# phases/00-bootstrap.sh
set -euo pipefail
source "$SCRIPT_DIR/image.conf"

mkdir -p "$ROOTFS"
pacstrap -c "$ROOTFS" "${PACKAGES[@]}"
```

**Phase 20: Install loisto binaries + overlay**

```bash
#!/bin/bash
# phases/20-install-loisto.sh
set -euo pipefail

# Copy overlay directory verbatim
cp -a "$SCRIPT_DIR/overlay/"* "$ROOTFS/"

# repart configs are used at build time only (not shipped to device)

# Copy RAUC config
mkdir -p "$ROOTFS/etc/rauc"
cp "$SCRIPT_DIR/rauc/system.conf" "$ROOTFS/etc/rauc/"
cp "$SCRIPT_DIR/rauc/keyring.pem" "$ROOTFS/etc/rauc/"
```

**Phase 40: Strip**

```bash
#!/bin/bash
# phases/40-strip.sh
set -euo pipefail
source "$SCRIPT_DIR/image.conf"

for pattern in "${STRIP_PATHS[@]}"; do
    rm -rf "$ROOTFS"/$pattern
done

# Firmware trimming
if [ ${#FIRMWARE_KEEP[@]} -gt 0 ]; then
    FW_DIR="$ROOTFS/usr/lib/firmware"
    FW_TMP=$(mktemp -d)
    for fw in "${FIRMWARE_KEEP[@]}"; do
        [ -e "$FW_DIR/$fw" ] && cp -a "$FW_DIR/$fw" "$FW_TMP/"
    done
    rm -rf "$FW_DIR"/*
    cp -a "$FW_TMP"/* "$FW_DIR/"
    rm -rf "$FW_TMP"
fi
```

**Phase 50: dm-verity**

```bash
#!/bin/bash
# phases/50-verity.sh
set -euo pipefail
source "$SCRIPT_DIR/image.conf"

USR_IMG="$OUTPUT/rootfs.img"

# Create ext4 image from rootfs /usr
mke2fs -d "$ROOTFS/usr" -t ext4 -r 1 -b 4096 \
    -L "loisto-usr" "$USR_IMG" "${USR_SIZE}"
tune2fs -O read-only "$USR_IMG"

# Generate dm-verity hash tree + root hash
veritysetup format "$USR_IMG" "$OUTPUT/rootfs.verity" \
    | tee "$OUTPUT/verity-info.txt"

ROOTHASH=$(grep "Root hash:" "$OUTPUT/verity-info.txt" | awk '{print $3}')
echo "$ROOTHASH" > "$OUTPUT/roothash.txt"
echo "dm-verity root hash: $ROOTHASH"
```

**Phase 60: UKI (optional, for verified boot)**

```bash
#!/bin/bash
# phases/60-uki.sh
set -euo pipefail
source "$SCRIPT_DIR/image.conf"

ROOTHASH=$(cat "$OUTPUT/roothash.txt")
KERNEL="$ROOTFS/usr/lib/modules/*/vmlinuz"  # glob to find kernel
INITRD="$OUTPUT/initramfs.img"              # built by mkinitcpio in phase 30

# Build kernel cmdline with embedded verity hash
# No systemd.verity_* params — our initramfs calls veritysetup directly
CMDLINE="ro quiet loglevel=0 vt.global_cursor_default=0"
CMDLINE="$CMDLINE loisto.verity_data=/dev/disk/by-partlabel/usr-a"
CMDLINE="$CMDLINE loisto.verity_hash=/dev/disk/by-partlabel/usr-a-verity"
CMDLINE="$CMDLINE loisto.roothash=$ROOTHASH"

echo "$CMDLINE" > "$OUTPUT/cmdline.txt"

# Build UKI with ukify (standalone systemd tool, no mkosi needed)
ukify build \
    --linux="$KERNEL" \
    --initrd="$INITRD" \
    --cmdline=@"$OUTPUT/cmdline.txt" \
    --os-release=@"$ROOTFS/usr/lib/os-release" \
    --output="$OUTPUT/${IMAGE_ID}-${IMAGE_VERSION}.efi"

# Sign for Secure Boot (if keys available)
if [ -f "$SCRIPT_DIR/keys/secureboot.key" ]; then
    sbsign --key "$SCRIPT_DIR/keys/secureboot.key" \
           --cert "$SCRIPT_DIR/keys/secureboot.crt" \
           --output "$OUTPUT/${IMAGE_ID}-${IMAGE_VERSION}.efi" \
           "$OUTPUT/${IMAGE_ID}-${IMAGE_VERSION}.efi"
fi
```

### test-vm.sh (cross-platform)

```bash
#!/bin/bash
# loisto-image/test-vm.sh
set -euo pipefail
source "$(dirname "$0")/image.conf"

IMG="output/${IMAGE_ID}-${IMAGE_VERSION}.img"

case "$(uname -s)" in
    Linux)
        qemu-system-x86_64 \
            -enable-kvm \
            -m 4G \
            -drive file="$IMG",format=raw,if=virtio \
            -device virtio-gpu-pci \
            -device virtio-keyboard-pci \
            -device virtio-mouse-pci \
            -device virtio-net-pci,netdev=net0 \
            -netdev user,id=net0 \
            -bios /usr/share/edk2/x64/OVMF.fd \
            -serial stdio
        ;;
    Darwin)
        # UTM CLI (if installed)
        if command -v utmctl &>/dev/null; then
            echo "Use UTM GUI to import: $IMG"
            echo "Or use QEMU via Homebrew:"
        fi
        # QEMU on macOS (Homebrew, no KVM, slower)
        qemu-system-x86_64 \
            -m 4G \
            -drive file="$IMG",format=raw,if=virtio \
            -device virtio-gpu-pci \
            -bios /opt/homebrew/share/qemu/edk2-x86_64-code.fd \
            -serial stdio
        ;;
esac
```

### Why this works better than mkosi for us

| Concern | mkosi | Our approach |
|---------|-------|-------------|
| **Stability** | Config format changes across versions | Shell scripts don't break |
| **Debuggability** | Python + nspawn + repart internals | `set -x` and read output |
| **macOS dev** | Cannot build on macOS (needs systemd-nspawn) | pacstrap works in Docker; test-vm.sh handles both platforms |
| **Dependencies** | Python 3.11+, systemd 256+, nspawn | bash, pacstrap, mksquashfs, veritysetup, QEMU |
| **Transparency** | Abstraction hides what's happening | Every byte placement is visible in the phase script |
| **dm-verity** | Built-in | We call `veritysetup` directly — same result, 5 lines of shell |
| **UKI** | Built-in | We call `ukify` directly — same result, 10 lines of shell |
| **systemd-repart** | mkosi calls it at build time | We call it at build time only; not shipped to device |
| **sysext** | Native (dm-verity `/usr/`) | Same — our `/usr/` is dm-verity protected, sysext works identically |

The key realization: mkosi's "magic" is just calling `veritysetup`, `ukify`, `sbsign`,
`mke2fs`, and `systemd-repart` in the right order. Those tools are standalone. We call
them ourselves and gain full visibility into what's happening.

### What we still use from the systemd ecosystem (directly, no mkosi)

| Tool | Purpose | When |
|------|---------|------|
| `veritysetup` | Generate dm-verity hash tree | Build time (standalone binary) |
| `ukify` | Build Unified Kernel Image | Build time (standalone tool) |
| `sbsign` | Sign UKI for Secure Boot | Build time |
| `systemd-repart` | Partition the disk image | Build time only (not shipped) |
| `systemd-sysext` | Merge modular extensions onto `/usr/` | Runtime (standalone, needs only erofs + overlayfs) |
| `systemd-boot` | UEFI boot manager | Runtime (EFI binary, not a systemd daemon) |
| RAUC | A/B update management, `efi` backend for boot assessment | Runtime (CLI: `rauc install`, no dbus) |

### Summary

```
Build (CI/dev machine):           Device (runtime):
┌─────────────────────────┐       ┌─────────────────────────────┐
│ image.conf              │       │ systemd-boot                │
│   ↓                     │       │   ↓                         │
│ phases/00-bootstrap.sh  │       │ UKI (signed, verity hash)   │
│ phases/10-packages.sh   │       │   ↓                         │
│ phases/20-loisto.sh     │       │ dm-verity on /usr            │
│ phases/30-configure.sh  │       │   ↓                         │
│ phases/40-strip.sh      │       │ Zig init → gamescope → app  │
│ phases/50-verity.sh     │       │                             │
│ phases/60-uki.sh        │       │ rauc install (CLI one-shot) │
│ phases/70-disk.sh       │       │   ↓                         │
│   ↓                     │       │ Delta update (5-50MB)       │
│ bundle.sh → .raucb      │──────→│   ↓                         │
│                         │       │ Write inactive slot          │
│ test-vm.sh → QEMU/UTM  │       │   ↓                         │
└─────────────────────────┘       │ Health check → mark-good    │
                                  └─────────────────────────────┘
```

### Additional sources

- [veritysetup(8)](https://man7.org/linux/man-pages/man8/veritysetup.8.html)
- [ukify(1)](https://www.freedesktop.org/software/systemd/man/latest/ukify.html)
- [sbsign(1)](https://man.archlinux.org/man/sbsign.1)
- [systemd-repart(8)](https://www.freedesktop.org/software/systemd/man/latest/systemd-repart.html)
- [RAUC Integration Guide](https://rauc.readthedocs.io/en/latest/integration.html)
- [RAUC Adaptive Updates](https://rauc.readthedocs.io/en/latest/advanced.html#adaptive-updates)
- [ParticleOS](https://github.com/systemd/particleos) — reference for the conventions we adapt
- [SteamOS partition teardown](https://github.com/randombk/steamos-teardown/blob/master/docs/partitions.md)
- [mkosi first impressions (2025)](https://blog.wang-lu.com/2025/08/mkosi-first-impressions.html) — why we decided against mkosi itself
