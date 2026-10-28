# Interrupts: the GICv2

An interrupt controller collects the interrupt lines of the devices and
signals the CPUs. The kernel handles interrupts without knowing which
controller the board has: the `intrinfo` section describes the controller
as ranges of *vectors*, with callouts to identify, acknowledge, mask and
unmask them. This chapter explains Arm's Generic Interrupt Controller
version 2, which the library supports, and how it is described to the
kernel.

## How a GICv2 works

A GICv2 (the GIC-400 is one) has two parts:

- the **distributor** (GICD), one for the system: it knows every
  interrupt, enables or disables it, gives it a priority and, for shared
  interrupts, chooses which CPUs receive it;
- one **CPU interface** (GICC) per CPU, at the same address on every CPU
  (each CPU sees its own): the CPU reads the *Interrupt Acknowledge
  Register* (`GICC_IAR`) to learn which interrupt it is taking, and writes
  the same value to the *End Of Interrupt Register* (`GICC_EOIR`) when it
  is done.

Interrupts have IDs, in three classes:

- **SGIs**, 0 to 15: software-generated, raised by writing the
  distributor's `GICD_SGIR` with a list of target CPUs. Operating systems
  use them as *inter-processor interrupts* (IPIs);
- **PPIs**, 16 to 31: private to each CPU, such as the CPU's own timer
  (the virtual timer is PPI 27);
- **SPIs**, 32 and up: shared, the devices' interrupts.

SGI and PPI registers in the distributor are *banked*: each CPU sees its
own copy. Reading `GICC_IAR` returns 1023 when there is nothing to take
(a *spurious* interrupt). A CPU interface only signals an interrupt whose
priority is higher (numerically lower) than its *priority mask*
(`GICC_PMR`).

## How the kernel sees interrupts

The kernel numbers interrupts with *vectors*, and `intrinfo` maps vector
ranges to controllers (Listing 8.1). An entry covers `num_vectors` vectors
from `vector_base`; a controller that is wired into another one's input
(*cascaded*) gives that input's vector in `cascade_vector`.

*Listing 8.1: An interrupt controller range, `IntrinfoEntry` (`src/abi/syspage.rs`).*

```rust
{{#include ../../src/abi/syspage.rs:intrinfo_entry}}
```

When an interrupt arrives, the kernel runs the `id` callout of each entry
to find the pending interrupt, handles it, and runs the matching `eoi`.
Speed matters here, so these two are not called but *copied* into the
kernel's interrupt entry code (Chapter 7); their `genflags` say which
values the kernel loads for them first. They work with fixed registers:

- `x19` returns the interrupt, relative to the entry's `vector_base`, or
  -1 if the entry has none pending; `eoi` gets it back in `x19`;
- `x20` holds the system page's address, `x21` the entry's, `x22` the
  interrupt's mask count, `x24` the CPU number, when `genflags` ask for
  them;
- the code must leave the kernel's other registers alone, and fall
  through its end.

The other interrupt callouts are ordinary functions. `mask` and `unmask`
take the system page and the vector (relative to the entry), and return
0. `config` returns flags for a vector, such as `INTR_CONFIG_FLAG_IPI` for
the one the kernel uses for IPIs.

## The GIC as two ranges

A GICv2 becomes two `intrinfo` entries:

- vectors 0-31, the SGIs and PPIs, with `local_stride` 0x8000 to tell the
  kernel they are per CPU;
- vectors 32 and up, the SPIs, as many as the distributor's `GICD_TYPER`
  says the GIC has.

Listing 8.2 sets the GIC up and describes it.

*Listing 8.2: Setting up the GIC, `gic_v2::init()` (`src/arch/aarch64/gic_v2.rs`).*

```rust
{{#include ../../src/arch/aarch64/gic_v2.rs:gic_init}}
```

The distributor is turned off while startup sets every SPI to disabled,
not pending, priority 0xa0, level-triggered, and routed to the boot CPU's
interface; the kernel enables and reroutes interrupts as drivers attach
to them. The registers get `asinfo` entries (`gicd`, `gicc`) and kernel
mappings for the callouts. The same function records the `send_ipi`
callout in the `smp` section.

## The callouts

Listing 8.3 shows the GIC's callouts. The two `id` callouts work as a
pair: the first, in the SGI/PPI entry, reads `GICC_IAR` once and keeps the
value in `x24`; it returns the ID if it is below 32. The second, in the SPI
entry, uses the same `x24` and returns the ID less 32, or -1 for the
spurious IDs 1022 and 1023. `eoi` writes `x24` back to `GICC_EOIR`.

*Listing 8.3: The GICv2 callouts (`src/arch/aarch64/gic_v2.rs`).*

```rust
{{#include ../../src/arch/aarch64/gic_v2.rs:gic_callouts}}
```

SGI 0 is the kernel's IPI. Its `GICC_IAR` value carries the sending CPU's
number in bits 10-12, and the PPI `id` callout ends it immediately with
that full value, so that the IPI can be raised again while the kernel
handles it; `eoi` then skips it.

`send_ipi(syspage, cpu, cmd, &ipicmds[cpu])` sets the command bits in the
target CPU's word with an exclusive load/store pair, and raises SGI 0 on
it only if the word was empty: a CPU with an IPI already pending will see
the new command too.

> [!NOTE]
> `send_ipi` targets CPU `cpu` by setting bit `16 + cpu` of `GICD_SGIR`: it
> assumes that a CPU's number in the kernel is its GIC CPU interface's
> number. `gic_map` records the interface of each CPU, and on boards where
> the two differ, `send_ipi` would have to read it.

## Each CPU's interface

Each CPU sets up its own interface when it starts (Listing 8.4). It finds
its interface number in `GICD_ITARGETSR0`, which reads as the reading
CPU's own interface, and records it in `gic_map`. Then it gives its SGIs
and PPIs priorities (0xc0 for the IPI, 0xa0 for the rest), enables the SGIs
and disables the PPIs (the kernel enables the timer's), sets the priority
mask to 0xf0, and enables the interface.

*Listing 8.4: Setting up a CPU's interface, `gic_v2::init_cpu()` (`src/arch/aarch64/gic_v2.rs`).*

```rust
{{#include ../../src/arch/aarch64/gic_v2.rs:gic_cpu_init}}
```

## Other controllers

A board may have more controllers: the BCM2711, for example, has an MSI
controller for PCI Express, cascaded into one of the GIC's SPIs. A board
adds such a controller as one more `intrinfo` entry with its own callouts,
and `cascade_vector` set to the SPI it signals through. The library
provides the GICv2 only; GICv3, with its per-CPU redistributors and
system-register CPU interface, would need its own module.
