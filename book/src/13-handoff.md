# Into the Kernel

`Startup::boot()` is the last thing a board's `main()` calls. It does the
work that has to wait until the board has described everything: it loads
the kernel, starts the other CPUs, gives the system page its final memory,
and jumps. This chapter follows it.

## `boot()`

*Listing 13.1: Finishing the boot, `Startup::boot()` (`src/startup.rs`).*

```rust
{{#include ../../src/startup.rs:boot}}
```

The steps, in order:

1. **Defaults.** The host name, if the board or `-N` set none, and the
   page size in `system_private`.
2. **The image's parts** get `asinfo` entries: the image file system
   (`imagefs`), startup itself (`startup`) and the RAM the image takes
   beyond startup (`bootram`); and the device tree, if there is one
   (`fdt`), which is reserved now.
3. **The bootstrap executables** are loaded and linked (Chapter 11).
4. **The other CPUs** start and describe themselves (Chapter 12).
5. **The system page's memory** is allocated, now that the number of
   CPUs, the callouts and nearly all sections are known. The sections
   get room for the `sysram` entries still to come, and the kernel's spare
   room for asinfo entries (`ASINFO_SPARE`); then come the callouts' code
   and the cpupages, each part page aligned (Figure 6.1). The memory is
   mapped twice in the kernel's half: read-write for the kernel, read-only
   for processes; and the callouts' part again, read-only and executable.
6. **`sysram`.** Mapping was the last allocation, so what is free now is
   the kernel's (Chapter 5).
7. **The system page is written**: `system_private` gets the mappings'
   addresses, the callouts are copied and patched, the references in the
   sections are replaced by their addresses, the sections are laid out and
   copied, and each cpupage gets its CPU's number.
8. **The other CPUs enter `smp_spin`** (Chapter 12).
9. **The boot CPU jumps** into the first bootstrap executable.

> [!WARNING]
> Step 6 must come after the last allocation. Moving an allocation (a
> mapping, a page table) after it gives the kernel memory that startup
> uses.

## The jump

The jump itself is `vstart` (Listing 13.2), which every CPU runs, with its
own TTBR1. It invalidates the instruction cache and the TLBs, loads
`MAIR_EL1`, `TCR_EL1` and both table bases, sets `CPACR_EL1` to 0 so that
floating-point and SIMD instructions trap until the kernel has saved and
set up their state, and turns the MMU and caches on.

*Listing 13.2: Turning the MMU on and jumping, `vstart` (`src/arch/aarch64/handoff.rs`).*

```rust
{{#include ../../src/arch/aarch64/handoff.rs:vstart}}
```

The instruction after `msr sctlr_el1` is fetched with the MMU on, at the
same address: TTBR0's 1:1 map of startup makes it the same instruction.
The `br` then leaves startup's addresses for the kernel's half.

## The state the kernel starts in

Table 13.1 sums up what the kernel finds on its first instruction.

*Table 13.1: The CPU's state at the kernel's entry.*

| State | Value |
|---|---|
| Exception level | EL1, `SP_EL1`, all exceptions masked |
| `x0` | The system page's address in the kernel's half (`kern_syspageptr`) |
| `x1` | The CPU's number (0) |
| MMU, caches | On; `SCTLR_EL1 = 0x34d5d91d` |
| `TTBR1_EL1` | The CPU's level 0 table: the kernel, the system page, the callouts' devices |
| `TTBR0_EL1` | Startup's 1:1 map, ASID 0 |
| `MAIR_EL1`, `TCR_EL1` | As in Chapter 10 |
| `CPACR_EL1` | 0: FP, SIMD, SVE trap |
| `CNTKCTL_EL1` | EL0 may read the virtual counter |
| The other CPUs | In `smp_spin`, MMU on, each with its own level 0 table |
| The interrupt controller | On, every shared interrupt disabled and routed to CPU 0 |

From here, the kernel is in charge. It reads the system page, sets up
its own state, and calls the other CPUs in through `smp.start_addr`.
