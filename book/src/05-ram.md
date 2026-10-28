# Physical Memory

Startup needs memory of its own (for page tables, the system page, the
kernel's writable data), and it must tell the kernel which memory is free
for it to manage. This chapter shows how the library keeps track of both.

## What the kernel needs to know

QNX's kernel takes its free memory from the system page's `asinfo`
section (Chapter 6): every entry named `sysram` is RAM it may hand out.
Everything else is either not RAM, or RAM that something already uses:

- the **image**: startup itself, and the image file system, whose files
  the kernel serves straight from where they are;
- **startup's allocations**: the page tables, the system page, the
  writable parts of the kernel;
- RAM **owned by devices**: on the Raspberry Pi, the part of RAM the GPU
  keeps for itself; in general, buffers that firmware set up;
- RAM a board keeps for its own reasons, such as firmware code that the
  secondary CPUs still run.

The kernel never sees startup's bookkeeping: it sees the `sysram` entries
that are left at the end.

## Ranges

All of this is about sets of address ranges. `RangeSet` (Listing 5.1)
keeps disjoint, half-open ranges `[start, end)` sorted, and merges ranges
that touch, so that the free RAM is always the fewest, largest ranges.

*Listing 5.1: Adding a range, `RangeSet::insert()` (`src/ram.rs`).*

```rust
{{#include ../../src/ram.rs:rangeset_insert}}
```

`partition_point` finds the first range that ends at or after the new
one's start; every range from there that starts at or before its end
merges into it. The array shifts to close the gap or make room. Removing
a range works the other way: it trims the ranges it overlaps, and splits
the one it falls inside.

The sets live in fixed arrays (32 ranges), since startup has no heap. A
board with more separate RAM ranges than that would stop with a message.

## Three kinds of bookkeeping

`Ram` keeps two sets, and offers three operations on them:

- `add(start, size)`: there is RAM here;
- `reserve(start, size)`: this RAM is in use for good. It leaves the free
  set, so neither startup nor the kernel gets it;
- `avoid(start, size)`: startup must not allocate from this RAM, but the
  kernel may have it. The ranges go to the second set, which only
  allocations look at.

*Avoiding* is for memory that startup needs to stay intact while it runs,
but that is free afterwards. A compressed image, for one, would be avoided
while it is unpacked. The device tree the boot loader passed is avoided
from the moment the `-u` option names it (Listing 5.2): the options are
parsed before the board reports its RAM, so it cannot be reserved yet.
`boot()` reserves it, and gives it an `fdt` asinfo entry for the drivers
that read it.

*Listing 5.2: Taking note of the device tree, `Startup::set_fdt()` (`src/startup.rs`).*

```rust
{{#include ../../src/startup.rs:set_fdt}}
```

Where the device tree is depends on the boot loader. With `-u reg`, its
address is in `x0` at entry, where the Raspberry Pi's firmware puts it.
With `-u arg`, the boot loader called startup like a C program,
`main(argc, argv)`, with the address as the first argument: U-Boot's `go`
command does this. Listing 5.3 reads both.

*Listing 5.3: Where `-u` says the device tree is, `Startup::fdt_address()` (`src/startup.rs`).*

```rust
{{#include ../../src/startup.rs:fdt_address}}
```

## Allocating

Startup allocates with `Ram::alloc()` (Listing 5.4): the lowest address
that fits the size and alignment, outside the avoided ranges.

*Listing 5.4: Allocating RAM, `Ram::alloc()` (`src/ram.rs`).*

```rust
{{#include ../../src/ram.rs:ram_alloc}}
```

For each free range, the loop steps over the avoided ranges inside it and
tries each gap in turn. What it allocates is reserved: it leaves the free
set.

Allocating from the bottom keeps startup's allocations together, below
the image on most boards, and leaves the rest of RAM in one piece for the
kernel. QNX's startup library does the same.

> [!WARNING]
> `alloc()` is the only way startup gets memory, and nothing frees it. Any
> allocation after `boot()` has turned the free set into `sysram` entries
> (Chapter 13) would hand the kernel memory that startup also uses.

## Adding RAM

A board reports its RAM with `Startup::add_ram()` (Listing 5.5). The
first call also builds the root of the `asinfo` tree, `memory`, which
spans the CPU's whole physical address range, and its child `below4G`.
Every call adds a `ram` entry, and reserves the image's RAM, which
the startup header gives (`ram_paddr`, `ram_size`).

*Listing 5.5: Adding RAM, `Startup::add_ram()` (`src/startup.rs`).*

```rust
{{#include ../../src/startup.rs:add_ram}}
```

The size of `memory` comes from the CPU: `ID_AA64MMFR0_EL1.PARange` gives
the number of physical address bits (44 on a Cortex-A72).

A board then reserves what its devices own and adds the names drivers
look for. On the Raspberry Pi 4, for example, the GPU's RAM is reserved
and named `vcram`, and the RAM below 1 GB is named `below1G` for the SoC's
older DMA engines, which cannot reach further. Each is added under `ram`
with `as_add_containing()` (Chapter 6).

## From free RAM to `sysram`

When `boot()` has made its last allocation, it walks the free set and
adds each range as `sysram` under the most specific entry that contains
it. On a 2 GB Raspberry Pi 4 the result looks like this (the output of
`pidin syspage=asinfo` on the running system, abridged):

```text
0000) 0000000000000000-00000fffffffffff o:ffff /memory
0018) 0000000000000000-00000000ffffffff o:0000 /memory/below4G
0030) 0000000000000000-000000007fffffff o:0018 /memory/below4G/ram
0048) 000000003c000000-000000003fffffff o:0030 /memory/below4G/ram/vcram
0060) 0000000000000000-000000003bffffff o:0030 /memory/below4G/ram/below1G
...
0108) 000000000000c000-000000000007ffff o:0060 /memory/below4G/ram/below1G/sysram
0120) 00000000003ce000-000000001effffff o:0060 /memory/below4G/ram/below1G/sysram
0138) 000000001f00e000-000000003bffffff o:0060 /memory/below4G/ram/below1G/sysram
0150) 0000000040000000-000000007e7eafff o:0030 /memory/below4G/ram/sysram
```

The gaps are the board's first page (its firmware) and startup's
allocations, up to `0xb140` (the kernel rounds the range up to `0xc000`),
the image
(`0x80fa0` to `0x3cd043`), the device tree (at `0x1f000000`) and the GPU's
RAM. The kernel rounds the ranges to pages, and takes a little from the
top of the last one for itself, which is why it ends below `0x7fffffff`
although startup handed it all.
