# The Virtio Ecosystem: From Hardware Emulation to MicroVM Density

## Narrative Summary (1-minute read)

Virtio is the paravirtualized I/O standard that makes modern cloud computing possible.
Before virtio, every hypervisor invented its own device model, guests ran slow emulated
hardware, and driver effort was duplicated across Xen, KVM, VMware, and others. Rusty
Russell (IBM/Linux) designed virtio in 2007-2008 as a common abstraction: a simple
ring-buffer contract between guest driver and host backend. OASIS standardized it.
Today, virtio defines 19+ device types (network, block, GPU, filesystem, pmem, sound,
etc.), underpins KVM/QEMU, and is the sole I/O layer in lightweight VMMs like
Firecracker and cloud-hypervisor. Virtio-pmem is particularly interesting for microVM
density: it maps a host file directly into guest physical memory, enabling DAX
(zero-copy, no page-cache duplication) for read-only filesystems like erofs --- exactly
the trick microvm.nix uses to share /nix/store across hundreds of VMs.

## What Changed

This document is a new research synthesis. No code changes.

## What To Do Next

Evaluate virtio-pmem + erofs/FSDAX as a replacement for virtiofs for the /nix/store
share in our cloud-hypervisor topology, which would eliminate the virtiofsd process,
survive snapshot/restore (unlike virtiofs), and reduce per-VM memory overhead.

---

## 1. History of Virtualized I/O

### 1.1 Full Hardware Emulation (the QEMU era)

The earliest approach to guest I/O was **full emulation**: the hypervisor presents a
virtual copy of a real hardware device (e.g., an Intel e1000 NIC, an IDE disk controller)
and traps every port-I/O or MMIO access the guest driver makes. QEMU, written by Fabrice
Bellard starting in 2003, is the canonical example.

The guest runs an unmodified driver (the same driver it would use on bare metal). Every
register read/write exits from the guest into the hypervisor, which interprets the
operation in software. This is architecturally clean --- the guest needs no modification
--- but painfully slow. A single network packet might require dozens of VM exits for
descriptor ring manipulation, interrupt acknowledgment, and DMA setup. Each exit costs
hundreds to thousands of CPU cycles.

Performance numbers from the era: emulated e1000 NICs achieved roughly 1/3 to 1/2 the
throughput of native hardware. Emulated IDE controllers were worse. The bottleneck was
not the I/O itself but the trap-and-emulate overhead on every register access.

### 1.2 Xen's Split-Driver Model (2003-2007)

Xen, developed at Cambridge and first released in 2003, took a different approach:
**paravirtualization**. Rather than emulating real hardware, Xen defined abstract device
interfaces and required guests to be modified to use them.

Xen's architecture uses a **split-driver model**:

- A **frontend driver** runs in the guest domain (DomU). It speaks an abstract protocol,
  not a hardware register interface.
- A **backend driver** runs in the privileged domain (Dom0). It translates abstract
  requests into real hardware operations.
- Frontend and backend communicate through **shared memory pages** and an **event
  channel** (a lightweight notification mechanism), mediated by Xen's grant table system.

The key data structure is the **I/O ring**: a circular buffer in shared memory where the
frontend posts requests and the backend posts responses. This is a direct ancestor of
virtio's virtqueue.

Xen's paravirtualized drivers achieved near-native performance. The problem was
**fragmentation**: Xen's device protocols were Xen-specific. KVM had its own approach.
VMware had vmxnet. Hyper-V had its own VSP/VSC (Virtualization Service Provider/Client)
model. Every hypervisor reinvented the same wheel, and driver authors had to support
each one separately.

### 1.3 Virtio: One Ring to Rule Them All (2007-2008)

**Rusty Russell**, an Australian kernel developer at IBM working on the lguest
hypervisor, recognized this fragmentation problem. His 2008 paper, "virtio: towards a
de-facto standard for virtual I/O devices," laid out the case:

> There are a wide variety of virtualization platforms... each has their own approaches
> to device emulation and communication between the guest operating system and the
> hypervisor, resulting in a lack of interoperability between virtualization environments.

Russell designed virtio as a **transport-agnostic paravirtualized I/O framework** with
three layers:

1. **Device abstraction**: A common configuration space and feature negotiation protocol.
2. **Virtqueue transport**: The ring-buffer mechanism for passing data between guest and
   host (described in detail in Section 3).
3. **Device-specific protocols**: Per-device-type definitions (network, block, etc.)
   built on top of the common transport.

The initial implementation landed in Linux 2.6.24 (early 2008) with drivers for
virtio-net and virtio-blk. KVM adopted it immediately. The design was intentionally
simple: Russell's lguest was a pedagogical hypervisor, and virtio inherited that clarity.

Early benchmarks showed 2-3x better network throughput compared to emulated devices,
with dramatically lower CPU utilization.

### 1.4 OASIS Standardization (2012-present)

By 2012, virtio had become the de facto standard for KVM I/O, but it was still a
"Linux kernel implementation" rather than a formal specification. Red Hat, IBM, and
others pushed for formal standardization through OASIS (Organization for the Advancement
of Structured Information Standards).

Key milestones:

- **2012**: OASIS Virtual I/O Device (VIRTIO) Technical Committee formed.
- **2016**: **Virtio 1.0** ratified as an OASIS standard. Defined the split virtqueue,
  PCI/MMIO/channel I/O transport bindings, and the core device types (net, blk, console,
  entropy, balloon, SCSI).
- **2019**: **Virtio 1.1** introduced **packed virtqueues** (a major performance
  optimization, see Section 3) and added in-order completion.
- **2022**: **Virtio 1.2** added new device types including filesystem, pmem, and sound.
- **2024**: **Virtio 1.3** (committee specification draft) added GPIO, PMEM, I2C adapter,
  and SCMI devices.

### 1.5 Key Organizations and Their Roles

| Organization | Contribution |
|---|---|
| **IBM** | Rusty Russell's employer; created lguest and virtio. Major contributor to KVM. |
| **Red Hat** | Primary driver of OASIS standardization. Major contributor to QEMU, libvirt, vhost, and vDPA. Maintains most virtio drivers in Linux. |
| **Google** | Developed crosvm (Rust VMM for ChromeOS). Major contributor to virtio-fs and virtio-gpu. |
| **Intel** | Created cloud-hypervisor (Rust VMM). Contributed vhost-user, VFIO, and hardware virtio offload (vDPA). Major role in virtio-pmem. |
| **Amazon** | Created Firecracker (Rust microVM). Drove the "minimal virtio device set" philosophy for serverless density. |
| **Microsoft** | Added virtio support to Hyper-V. Cross-pollinated with their own VSP/VSC model. |
| **ARM** | Virtio as the standard I/O model for Arm virtualization (replacing platform-specific device trees). |

---

## 2. Complete Inventory of Virtio Device Types

The following table lists all device types defined in the OASIS virtio specification
(through v1.3). Device type IDs are the numeric identifiers used in PCI configuration
space and MMIO device registers.

| ID | Device | Description |
|----|--------|-------------|
| 1 | **Network Device** | Virtual Ethernet adapter. Supports checksum offload, TSO/GSO, multiqueue, RSS, and header hashing. The most mature and heavily optimized virtio device. |
| 2 | **Block Device** | Virtual disk. Exposes a block device with read/write/flush semantics. Supports discard, write-zeroes, and multi-queue. The workhorse of VM storage. |
| 3 | **Console Device** | Virtual serial console. Provides one or more ports for text I/O between guest and host. Used for VM serial consoles and structured communication channels. |
| 4 | **Entropy Device** | Random number source. Provides cryptographically secure random bytes to the guest from the host's entropy pool. Critical for VM boot-time randomness. |
| 5 | **Traditional Memory Balloon Device** | Allows the host to reclaim guest memory dynamically. The guest "inflates" the balloon (returning pages to the host) or "deflates" it (reclaiming pages). Used for memory overcommit. |
| 6 | **SCSI Host Device** | Virtual SCSI host bus adapter. Passes SCSI commands through to host storage, supporting the full SCSI command set including advanced features like persistent reservations. |
| 7 | **GPU Device** | Virtual graphics adapter. Supports 2D/3D rendering, cursor operations, and resource management. Used by virgl (OpenGL passthrough) and Venus (Vulkan passthrough). |
| 8 | **Input Device** | Virtual input device (keyboard, mouse, tablet). Provides HID-like event delivery for pointing devices and keyboards without emulating specific hardware. |
| 9 | **Crypto Device** | Virtual cryptographic accelerator. Offloads symmetric and asymmetric crypto operations, hash computations, and MAC operations to the host. |
| 10 | **Socket Device** (vsock) | Host-guest communication channel using the AF_VSOCK socket family. Provides connection-oriented (stream) and connectionless (datagram) communication without requiring network configuration. |
| 11 | **File System Device** (virtiofs) | Shared filesystem using the FUSE protocol over virtqueues. Enables host directory sharing with metadata consistency, POSIX semantics, and optional DAX for zero-copy file access. |
| 12 | **RPMB Device** | Replay Protected Memory Block. Provides authenticated and replay-protected storage, typically used for secure key storage and anti-rollback protection. |
| 13 | **IOMMU Device** | Virtual I/O Memory Management Unit. Provides DMA remapping and isolation for other virtio devices, enabling secure device assignment and nested virtualization. |
| 14 | **Sound Device** | Virtual sound card. Supports PCM playback/capture streams, jack notifications, and channel maps. Designed for desktop and embedded virtualization. |
| 15 | **Memory Device** (virtio-mem) | Dynamic memory add/remove at page-block granularity. Unlike the balloon (which reclaims existing guest pages), virtio-mem can hot-add or hot-remove memory regions. Preferred over balloon for modern workloads. |
| 16 | **I2C Adapter Device** | Virtual I2C bus controller. Passes I2C transactions between guest and host, used for embedded and IoT virtualization scenarios. |
| 17 | **SCMI Device** | System Control and Management Interface. Provides access to platform management functions (clock, power domain, sensor, reset) through ARM's SCMI protocol. |
| 18 | **GPIO Device** | Virtual General-Purpose I/O controller. Exposes GPIO lines to the guest, used for embedded virtualization and hardware-in-the-loop simulation. |
| 19 | **PMEM Device** | Virtual persistent memory (see Section 4 for deep dive). Maps a host file as a persistent memory region in the guest, enabling DAX and bypassing the guest page cache entirely. |

**Note on PCI Device IDs**: Legacy virtio devices use PCI vendor 0x1AF4 with device IDs
0x1000-0x107F. Modern (virtio 1.0+) devices use device IDs calculated as 0x1040 + device
type ID (e.g., virtio-net = 0x1041, virtio-blk = 0x1042).

---

## 3. The Virtqueue Abstraction

The virtqueue is virtio's core innovation --- a lock-free, shared-memory communication
channel between guest driver ("driver") and host backend ("device"). Understanding
virtqueues is understanding virtio.

### 3.1 The Split Virtqueue (virtio 1.0)

The original virtqueue design splits the data structure into three regions in
guest-physical memory:

```
  Guest Memory Layout (Split Virtqueue)
  =====================================

  +---------------------------+
  | Descriptor Table          |  Array of (addr, len, flags, next) entries
  | [0] addr=0x1000 len=256   |  Each descriptor points to a guest memory buffer
  | [1] addr=0x2000 len=4096  |  Descriptors can be chained (scatter-gather)
  | [2] addr=0x3000 len=512   |
  | ...                       |
  +---------------------------+
  | Available Ring            |  Written by DRIVER (guest)
  | flags | idx               |  idx = next entry driver will write
  | ring[0]=2, ring[1]=0, ... |  ring[] = descriptor chain heads
  +---------------------------+
  | Used Ring                 |  Written by DEVICE (host)
  | flags | idx               |  idx = next entry device will write
  | ring[0]={id=2,len=256}    |  ring[] = completed descriptor chains + bytes written
  +---------------------------+
```

**The data flow**:

1. **Driver posts a request**: The driver fills one or more descriptor table entries with
   buffer addresses (scatter-gather list). It writes the head descriptor index into the
   next slot of the **available ring** and increments the available ring's `idx`.

2. **Driver kicks the device**: The driver writes to a notification register (PCI doorbell
   or MMIO register), telling the device "there are new buffers to process."

3. **Device processes the request**: The device reads the available ring, follows the
   descriptor chain, performs the I/O (e.g., reads from disk, sends a network packet),
   and writes any response data into the device-writable portions of the buffers.

4. **Device returns the buffer**: The device writes the descriptor chain head index and
   the number of bytes written into the next slot of the **used ring** and increments its
   `idx`.

5. **Device notifies the driver**: The device raises an interrupt (or MSI-X) to tell the
   guest "I completed some requests."

6. **Driver reclaims the buffer**: The driver reads the used ring, processes the
   completed I/O, and recycles the descriptors.

**Why this is elegant**:

- **Zero-copy in the common case**: The device reads/writes directly from/to guest
  memory buffers. No intermediate copies through the hypervisor.
- **Batch processing**: Multiple requests can be posted before a single kick, and
  multiple completions returned before a single interrupt. This amortizes notification
  cost.
- **Lock-free**: The driver only writes the available ring; the device only writes the
  used ring. No locks needed.
- **Scatter-gather**: Descriptor chaining supports complex I/O patterns (e.g., a network
  packet header in one buffer, payload in another).

### 3.2 The Packed Virtqueue (virtio 1.1)

The split virtqueue has a performance problem: **memory scattering**. The descriptor
table, available ring, and used ring are in three separate memory regions. Processing a
single I/O request requires accessing all three, which causes:

- **CPU cache thrashing**: Three separate cache lines touched per I/O operation.
- **PCIe bus overhead**: For hardware virtio implementations (SR-IOV, vDPA), each memory
  region access is a separate PCIe transaction.

The **packed virtqueue** (introduced in virtio 1.1) merges all three structures into a
**single descriptor ring**:

```
  Guest Memory Layout (Packed Virtqueue)
  ======================================

  +--------------------------------------+
  | Descriptor Ring (single array)       |
  | [0] addr | len | id | flags         |
  | [1] addr | len | id | flags         |  Flags include AVAIL and USED bits
  | [2] addr | len | id | flags         |  that replace the separate rings
  | ...                                  |
  +--------------------------------------+
  | Driver Event Suppression             |  Controls when device notifies driver
  +--------------------------------------+
  | Device Event Suppression             |  Controls when driver notifies device
  +--------------------------------------+
```

The key insight is embedding **availability and usage status directly in each
descriptor's flags**. Two flag bits --- AVAIL and USED --- replace the entire available
and used rings. A **wrap counter** tracks ring wraparound: when the ring wraps, the
expected polarity of the AVAIL/USED bits flips.

**To post a request**: The driver writes a descriptor with AVAIL=1, USED=0 (adjusted for
wrap counter).

**To complete a request**: The device sets USED=1 (matching the current wrap counter
polarity).

This means a single descriptor is touched in a single cache line for both posting and
completion. For hardware implementations, this reduces PCIe transactions dramatically.

Benchmarks showed 10-24% throughput improvement for virtio-net with packed virtqueues,
with the largest gains under high IOPS workloads.

### 3.3 Notification Suppression

Both virtqueue formats support **notification suppression** --- the ability for either
side to say "don't notify me, I'm polling." This enables adaptive polling: under high
load, the driver polls the used ring directly (avoiding interrupt overhead), and under
low load, it re-enables interrupts. This is analogous to Linux's NAPI for physical NICs.

---

## 4. Virtio-PMEM: Deep Dive

### 4.1 What It Is

Virtio-pmem is a paravirtualized persistent memory device. It exposes a region of host
memory (backed by a regular file) to the guest as if it were a physical NVDIMM
(Non-Volatile Dual In-line Memory Module). The guest kernel registers it with the
nvdimm/pmem subsystem and can use it with DAX-capable filesystems.

The key idea: instead of the guest issuing block I/O requests through a virtqueue (as
with virtio-blk), the guest **directly maps the host file into its physical address
space** as a PCI BAR (Base Address Register). Reads and writes become simple memory
loads and stores --- no virtqueue overhead, no guest page cache, no block layer.

### 4.2 How It Differs from Virtio-BLK

| Property | virtio-blk | virtio-pmem |
|---|---|---|
| **I/O path** | Block requests through virtqueue | Direct memory-mapped access (load/store) |
| **Guest page cache** | Yes, guest caches data in its own page cache | No, guest maps host pages directly via DAX |
| **Memory overhead** | Data is duplicated: once in host page cache, once in guest page cache | Single copy in host page cache, mapped into guest |
| **Virtqueue usage** | All data flows through virtqueues | Virtqueue used only for flush/sync commands |
| **Image format** | Supports raw, qcow2, etc. | Raw only (must be directly mmappable) |
| **Write persistence** | Handled by block layer flush | Requires explicit fsync/msync from guest userspace |
| **CPU cost per I/O** | VM exit per virtqueue kick | No VM exit for reads/writes (just memory access) |

### 4.3 DAX (Direct Access) and How It Works

DAX is a Linux kernel feature that allows filesystems to map storage device pages
directly into userspace address space, bypassing the page cache entirely.

**Without DAX** (traditional path):
```
Application buffer <--copy-- Page Cache <--copy-- Block Device
```

**With DAX**:
```
Application mmap region == Storage Device Pages (same physical pages)
```

For virtio-pmem, DAX means the guest filesystem maps the virtio-pmem region (which is
the host file's pages) directly into guest userspace. The data path is:

```
Guest application mmap'd region
        |
        v
Guest physical address (PCI BAR of virtio-pmem device)
        |
        v  (EPT/NPT translation)
Host virtual address (mmap of backing file)
        |
        v
Host page cache / filesystem
```

There is exactly **one copy** of the data in memory (the host page cache), accessible
by both host and guest through hardware address translation.

### 4.4 FSDAX vs DevDAX

These are two modes for exposing persistent memory regions in Linux:

**FSDAX (Filesystem DAX)**: The default and most common mode. Creates a block device
(`/dev/pmemX`) that supports DAX-capable filesystems. You format it with ext4 or xfs,
mount with `-o dax`, and applications use regular file operations. The filesystem
handles metadata; DAX handles data.

- Block device: `/dev/pmem0`, `/dev/pmem1`, etc.
- Supports: ext2, ext4, xfs, virtiofs, erofs
- Use case: General-purpose storage with zero-copy file access

**DevDAX (Device DAX)**: Provides raw character device access (`/dev/dax0.0`) without
any filesystem. Applications mmap the entire device directly. No filesystem metadata, no
file boundaries --- just a flat memory region.

- Character device: `/dev/dax0.0`, `/dev/dax0.1`, etc.
- No filesystem support (cannot mkfs)
- Use case: Databases that manage their own storage layout (e.g., SAP HANA),
  RDMA registration, VM memory backing

For microVM use cases, **FSDAX is the relevant mode** because we want to mount a
filesystem (erofs) on the pmem device with DAX enabled.

### 4.5 EROFS + FSDAX on Virtio-PMEM

This is the high-density configuration that projects like microvm.nix use:

1. **Build time**: Package the guest filesystem (e.g., /nix/store closure) as an
   **erofs image** (Enhanced Read-Only File System). erofs is designed for read-only use
   cases with minimal overhead and supports FSDAX natively.

2. **Host side**: The VMM (cloud-hypervisor, crosvm, QEMU) maps the erofs image file as
   a virtio-pmem device. The file is mmap'd and exposed as a PCI BAR to the guest.

3. **Guest side**: The guest kernel detects the virtio-pmem device, registers it as
   `/dev/pmem0`, and mounts the erofs filesystem with DAX:
   ```
   mount -t erofs -o dax /dev/pmem0 /nix/store
   ```

4. **At runtime**: When the guest reads `/nix/store/some-package/bin/program`, the read
   goes through erofs (which translates file offset to pmem offset), through FSDAX
   (which maps the pmem page directly), through EPT (which translates guest-physical to
   host-virtual), and hits the host page cache for the erofs image file. **No data is
   copied. No guest page cache is consumed.**

**Why this is powerful for density**:

- The host page cache for the erofs image is **shared across all VMs** that use the same
  image file. If 100 VMs mount the same /nix/store erofs image, there is exactly **one
  copy** in host memory, mapped into 100 guest address spaces.
- Unlike virtiofs, there is **no userspace daemon** (virtiofsd) per VM.
- Unlike virtio-blk, there is **no guest page cache duplication**.
- Unlike 9pfs, there is **no protocol overhead** for each file operation.

**Caveat**: When each VM gets its own erofs image (different NixOS closures), the host
page cache is per-file, so identical store paths in different images are **not
deduplicated** at the page cache level. The mm-template API and KSM (Kernel Same-page
Merging) are being explored to address this.

### 4.6 Cloud-Hypervisor Support

Cloud-hypervisor has supported virtio-pmem since v0.7.0 (early 2020). The `--pmem` flag
syntax:

```bash
cloud-hypervisor \
  --pmem file=/path/to/image.erofs,discard_writes=on \
  --pmem file=/path/to/data.img
```

Parameters:
- `file=<path>`: Path to the backing file (must be raw format).
- `size=<bytes>`: Optional; auto-detected from file size if omitted.
- `discard_writes=on`: Makes the device effectively read-only by discarding all writes
  (writes succeed from the guest's perspective but are not persisted). Useful for
  read-only root filesystems.
- `mergeable=on`: Previously allowed page merging; removed in v25.0.

**Snapshot/restore**: Cloud-hypervisor supports snapshot and restore of VMs with
virtio-pmem devices. Since the pmem device is backed by a host file (not by in-VM
state), the snapshot captures the device configuration but the data is in the backing
file. This is a significant advantage over virtiofs, which **cannot survive
snapshot/restore** due to the virtiofsd process state and FUSE session being lost
(cloud-hypervisor issue #6931).

This is directly relevant to the ChoirOS topology: our current virtiofs share for
/nix/store breaks on VM snapshot/restore. Replacing it with virtio-pmem + erofs would
eliminate that failure mode.

### 4.7 Memory Accounting Implications

Virtio-pmem has nuanced memory accounting:

**Host side**:
- The backing file's pages are in the host page cache.
- Guest access "pins" these pages (elevates reference count), but the host can reclaim
  them under memory pressure --- they can be re-read from the backing file.
- The pmem region is **not** counted against the VM's RAM allocation. It is additional
  address space mapped from the host file.
- RSS for the VMM process includes the pmem mapping, which can be misleading. The actual
  memory cost depends on how many pages are actively accessed.

**Guest side**:
- With FSDAX, the guest does **not** allocate page cache for pmem-backed files. This is
  the whole point: memory savings.
- Guest RAM (the memory= allocation) is used only for application data, kernel
  structures, and non-DAX I/O. The pmem-backed filesystem "appears" to use guest memory
  from the address space perspective, but the physical pages belong to the host page
  cache.

**Density calculation**: For a 500 MB erofs /nix/store image shared across 50 VMs:
- **Virtio-blk**: 500 MB host page cache + 50 x (working set in guest page cache) = 500
  MB + ~5 GB = ~5.5 GB
- **Virtio-pmem with DAX**: 500 MB host page cache (shared) = **500 MB total**

This is a 10x memory reduction for the filesystem layer alone.

---

## 5. The Broader Virtio Ecosystem

Virtio is one layer in a stack of technologies. Here is how the pieces fit together:

### 5.1 Vhost: Kernel-Side Acceleration

In the default QEMU model, virtqueue processing happens in the QEMU userspace process.
Every I/O operation requires: guest VM exit -> KVM -> QEMU (userspace) -> kernel
(for real I/O) -> back to QEMU -> KVM -> guest. That is two unnecessary
userspace/kernel transitions per I/O.

**Vhost** moves virtqueue processing into the **host kernel**. The vhost-net kernel
module, for example, processes virtio-net virtqueues directly in kernel space, forwarding
packets to the host's network stack without ever entering QEMU. The I/O path becomes:
guest VM exit -> KVM -> vhost-net (kernel) -> host network stack.

This eliminates two context switches per I/O and improves throughput by 20-40%.

### 5.2 Vhost-User: Userspace Backend Protocol

**Vhost-user** takes the vhost concept in the opposite direction: it moves virtqueue
processing to a **separate userspace process** (not QEMU, not the kernel). QEMU and the
backend process communicate over a Unix socket, sharing the guest memory regions via
file descriptor passing.

Use cases:
- **virtiofsd**: The virtiofs daemon processes filesystem requests in a dedicated
  sandboxed process.
- **DPDK**: High-performance networking stacks process virtio-net queues directly,
  bypassing the kernel entirely.
- **SPDK**: High-performance storage stacks process virtio-blk/scsi queues.

The vhost-user protocol defines how to set up virtqueue memory mappings, negotiate
features, and exchange notifications between QEMU and the backend process.

### 5.3 VFIO: Device Passthrough

**VFIO** (Virtual Function I/O) is a kernel framework for passing physical hardware
devices directly to VMs. Instead of emulating a device, the real hardware is assigned to
the guest, which accesses it at near-native speed.

VFIO handles:
- **IOMMU programming**: Ensuring the device can only DMA to guest-owned memory.
- **Interrupt remapping**: Routing device interrupts to the correct guest.
- **PCI configuration space mediation**: Allowing the guest to configure the device
  safely.

VFIO is orthogonal to virtio: it passes **real** hardware, while virtio provides
**virtual** devices. But they interact in the vDPA world.

### 5.4 vDPA: Hardware Virtio Acceleration

**vDPA** (Virtio Data Path Acceleration) bridges the gap between virtio and hardware.
A vDPA device is physical hardware that implements the **virtio data plane** (virtqueues)
directly, while the control plane (feature negotiation, configuration) is handled by
software.

The insight: if hardware implements virtqueues natively, the guest can use standard
virtio drivers (no special driver needed), but I/O bypasses the hypervisor entirely ---
the hardware processes virtqueue entries by DMA, just like a physical NIC or disk
controller.

vDPA devices appear as standard virtio devices to the guest. The host uses the vDPA
kernel framework (extending the vhost ioctl interface) to configure the hardware. This
enables live migration (the guest sees a standard virtio device) while achieving
near-VFIO performance.

Examples: Mellanox ConnectX NICs (mlx5_vdpa), Intel Infrastructure Processing Units.

### 5.5 The Stack, Visualized

```
                     Guest VM
                 +--------------+
                 | Application  |
                 | ------       |
                 | virtio-net   |  Standard virtio guest drivers
                 | virtio-blk   |  (same driver regardless of backend)
                 +--------------+
                       |
                  virtqueue
                       |
    -+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-
    Backend options (choose one per device):

    [QEMU emulation]    Slowest. virtqueue processed in QEMU userspace.
    [vhost-net]         Kernel module processes virtqueue. ~30% faster.
    [vhost-user]        Separate userspace process (DPDK, virtiofsd).
    [vDPA hardware]     Physical hardware processes virtqueue by DMA.
    [VFIO passthrough]  Physical device assigned directly (not virtio).
```

---

## 6. Why This Matters for MicroVM Density

### 6.1 The Overhead Reduction Timeline

Each generation of virtualization technology eliminated a layer of overhead:

**Generation 1: Full Emulation (QEMU, ~2003)**
- Emulates complete PC hardware: PIIX chipset, i440FX/Q35, ACPI, PCI bus, etc.
- Every device register access = VM exit + software interpretation
- Boot time: seconds to tens of seconds
- Memory overhead: 100+ MB for QEMU process
- Per-VM cost: high (heavy QEMU process, large guest image)

**Generation 2: KVM + Virtio (~2008)**
- Hardware-assisted virtualization (VT-x/AMD-V) eliminates most CPU emulation
- Virtio replaces emulated devices with paravirtualized ones (fewer VM exits)
- vhost moves hot-path I/O into kernel (fewer context switches)
- Boot time: seconds
- Memory overhead: ~40 MB QEMU + guest RAM
- Per-VM cost: medium (QEMU still has legacy device emulation code loaded)

**Generation 3: MicroVMs --- Firecracker and Cloud-Hypervisor (~2018)**
- Written in Rust. Minimal codebase (Firecracker: ~50K LoC vs QEMU: ~2M LoC)
- No legacy device emulation. Only virtio devices.
- Firecracker's device model: virtio-net, virtio-blk, virtio-vsock, serial, keyboard
  controller (for shutdown). That is **five** emulated devices total.
- No PCI bus (Firecracker uses MMIO transport). No ACPI. No USB. No VGA.
- Custom "microvm" machine type (upstream in QEMU too, since 4.0)
- Boot time: **~125 ms** (kernel to init)
- Memory overhead: **<5 MB** for VMM process
- Per-VM cost: low
- Density: **thousands of VMs per host**

**Generation 4: MicroVMs + Virtio-PMEM + Snapshot/Restore (~2020-present)**
- Virtio-pmem eliminates guest page cache for root filesystem
- erofs + FSDAX enables zero-copy read-only root from shared host file
- VM snapshots enable **sub-second cold start** (restore from snapshot vs. boot kernel)
- Memory accounting: shared page cache across VMs, not duplicated per VM
- Cloud-hypervisor restore time: **~2-5 seconds** (including full VM state)
- Firecracker snapshot restore: **<100 ms** to first instruction

### 6.2 The microvm.nix Pattern

microvm.nix (the NixOS microVM framework) exemplifies this density optimization:

1. **Build**: A NixOS configuration is evaluated. The /nix/store closure (only the paths
   needed for that VM's configuration) is packed into an **erofs image**.

2. **Boot**: The VMM loads the erofs image as a virtio-pmem device. The guest mounts it
   with FSDAX. The guest has a read-only root filesystem with zero memory duplication.

3. **State**: Mutable state (if any) goes on a separate virtio-blk device. The root
   filesystem is immutable and reproducible.

4. **Density**: Multiple VMs sharing the same NixOS configuration share the same erofs
   image file, and therefore share the same host page cache pages. The per-VM memory
   cost is only the VM's RAM allocation plus unique mutable state.

microvm.nix defaults to erofs over squashfs for runtime performance. erofs supports
FSDAX directly; squashfs does not.

### 6.3 Relevance to ChoirOS

Our current cloud-hypervisor topology uses:
- **virtiofs** for /nix/store (read-only share via virtiofsd process)
- **virtio-blk** for mutable data (data.img on host btrfs)

Known issues with this topology:
- virtiofs **cannot survive VM snapshot/restore** (cloud-hypervisor #6931)
- virtiofsd is a separate process per VM (memory + CPU overhead)
- Guest page cache may duplicate data already in host page cache

A virtio-pmem + erofs alternative would:
- Survive snapshot/restore (backing file is host-side, no daemon state)
- Eliminate the virtiofsd process
- Enable FSDAX zero-copy access to /nix/store
- Share host page cache across VMs with identical store closures

The tradeoff: we must build erofs images at deploy time (instead of mounting the host
/nix/store directly), and updates require rebuilding and redistributing the image. For
immutable NixOS closures, this is a natural fit.

---

## Sources

- [Rusty Russell, "virtio: towards a de-facto standard for virtual I/O devices" (2008)](https://ozlabs.org/~rusty/virtio-spec/virtio-paper.pdf)
- [IBM Developer: Virtio - An I/O virtualization framework for Linux](https://developer.ibm.com/articles/l-virtio/)
- [OASIS Virtual I/O Device (VIRTIO) TC](https://www.oasis-open.org/committees/tc_home.php?wg_abbrev=virtio)
- [OASIS VIRTIO Specification v1.3 (Committee Specification Draft)](https://docs.oasis-open.org/virtio/virtio/v1.3/csd01/virtio-v1.3-csd01.html)
- [OASIS VIRTIO Specification v1.2](https://docs.oasis-open.org/virtio/virtio/v1.2/csd01/virtio-v1.2-csd01.html)
- [Standardizing virtio (LWN.net)](https://lwn.net/Articles/580186/)
- [Red Hat: Virtqueues and virtio ring - how the data travels](https://www.redhat.com/en/blog/virtqueues-and-virtio-ring-how-data-travels)
- [Red Hat: Packed virtqueue - how to reduce overhead with virtio](https://www.redhat.com/en/blog/packed-virtqueue-how-reduce-overhead-virtio)
- [Red Hat: Virtio devices and drivers overview](https://www.redhat.com/en/blog/virtio-devices-and-drivers-overview-headjack-and-phone)
- [Red Hat: Introduction to vDPA kernel framework](https://www.redhat.com/en/blog/introduction-vdpa-kernel-framework)
- [Oracle: Introduction to VirtIO, Part 2 - Vhost](https://blogs.oracle.com/linux/introduction-to-virtio-part-2-vhost)
- [Stefan Hajnoczi: On unifying vhost-user and VIRTIO](http://blog.vmsplice.net/2020/09/on-unifying-vhost-user-and-virtio.html)
- [KVM virtio-pmem device (LWN.net)](https://lwn.net/Articles/776292/)
- [virtio pmem driver (LWN.net)](https://lwn.net/Articles/791687/)
- [QEMU: VirtIO Persistent Memory documentation](https://www.qemu.org/docs/master/system/devices/virtio/virtio-pmem.html)
- [crosvm: VirtIO Pmem](https://crosvm.dev/book/devices/pmem/basic.html)
- [Linux Kernel: Direct Access for files (DAX)](https://docs.kernel.org/filesystems/dax.html)
- [Linux Kernel: EROFS documentation](https://docs.kernel.org/filesystems/erofs.html)
- [virtiofs: shared file system for virtual machines](https://virtio-fs.gitlab.io/)
- [virtiofs DAX support (LWN.net)](https://lwn.net/Articles/828371/)
- [cloud-hypervisor device model](https://github.com/cloud-hypervisor/cloud-hypervisor/blob/main/docs/device_model.md)
- [cloud-hypervisor snapshot/restore](https://github.com/cloud-hypervisor/cloud-hypervisor/blob/main/docs/snapshot_restore.md)
- [cloud-hypervisor virtio-pmem issue #68](https://github.com/cloud-hypervisor/cloud-hypervisor/issues/68)
- [cloud-hypervisor virtiofs restore issue #6931](https://github.com/cloud-hypervisor/cloud-hypervisor/issues/6931)
- [Firecracker microVM](https://firecracker-microvm.github.io/)
- [Firecracker: Lightweight Virtualization for Serverless Applications (NSDI '20)](https://assets.amazon.science/96/c6/302e527240a3b1f86c86c3e8fc3d/firecracker-lightweight-virtualization-for-serverless-applications.pdf)
- [microvm.nix](https://github.com/microvm-nix/microvm.nix)
- [Xen split driver model (InformIT)](https://www.informit.com/articles/article.aspx?p=1160234)
- [Xen and Paravirtualization (Saferwall)](https://docs.saferwall.com/blog/virtualization-internals-part-3-xen-and-paravirtualization/)
- [Red Hat: FSDAX configuration](https://docs.redhat.com/en/documentation/red_hat_enterprise_linux/7/html/storage_administration_guide/configuring-persistent-memory-for-file-system-direct-access-dax)
- [ndctl: Managing Namespaces (FSDAX vs DevDAX)](https://docs.pmem.io/ndctl-user-guide/managing-namespaces)
- [rust-vmm/vm-virtio](https://github.com/rust-vmm/vm-virtio)
- [Project ACRN: Virtio Devices High-Level Design](https://projectacrn.github.io/latest/developer-guides/hld/hld-virtio-devices.html)
