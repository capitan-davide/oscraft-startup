# Writing a Board Crate

The library does the generic work; a board crate supplies the facts and
the code that only the board knows. This chapter lists what a board must
provide, the order to call the library in, and how to build the result.

## What a board provides

Table 14.1 is the board's part, with what the Raspberry Pi 4's board
crate does for each, as an example.

*Table 14.1: What a board provides.*

| What | Library interface | Example (Raspberry Pi 4) |
|---|---|---|
| Its debug UARTs, for startup and the kernel | `DebugDriver`s, `Startup::init_debug()` | The mini-UART (default), two PL011s |
| Its options | `Startup::options()` | `-W` for the watchdog |
| The number of CPUs, and how to release one | `Startup::init_smp()` | 4 cores, the firmware's spin table |
| The RAM, and what is not free | `Startup::add_ram()`, `ram.reserve()`, `syspage.as_add_containing()` | Size from the board revision; the GPU's RAM; the firmware's page |
| The interrupt controller | `gic_v2::init()` | The GIC-400's addresses |
| The tick's interrupt | `Startup::init_qtime()` | PPI 27 |
| The CPUs' speed | `Startup::init_boot_cpu()` | From the firmware's mailbox |
| Reboot | a callout in `callout.reboot` | The watchdog, set to expire at once |
| Names | `syspage.add_typed_string()` | Board, maker, SoC, serial number |
| Devices for drivers | `syspage.hwinfo` | Ethernet, watchdog, the SD card's DMA window |

## The order

The calls depend on each other, and Listing 14.1 shows the order that
works, as the board's `main()`.

*Listing 14.1: The skeleton of a board's `main()`.*

```rust
use oscraft_startup::arch::aarch64::gic_v2;
use oscraft_startup::console::DebugDriver;
use oscraft_startup::{Startup, entry};

static DEBUG: [DebugDriver; 1] = [MY_UART];  // a driver and its callouts

fn main(s: &mut Startup) -> ! {
    s.init_debug(&DEBUG);                     // 1. output first
    for opt in s.options("") {                // 2. options (-v, -P, ...)
        s.handle_common_option(&opt);
    }
    s.init_smp(NUM_CPUS, release_cpu);        // 3. before init_mmu()
    s.add_ram(RAM_BASE, RAM_SIZE);            // 4. RAM, before allocating
    s.init_mmu();                             // 5. first allocation
    gic_v2::init(s, GICD, GICC);              // 6. maps registers
    s.init_qtime(27);
    s.init_boot_cpu(CPU_MHZ);                 // 7. after the GIC
    let sp = &mut s.syspage;
    hwinfo::add_default(&mut sp.hwinfo, &mut sp.strings);
    sp.hwinfo.finish();
    s.boot()                                  // 8. never returns
}

entry!(main);
```

The constraints behind it:

- `init_smp()` comes before `init_mmu()`, which gives each CPU a page
  table;
- every `add_ram()`, `ram.reserve()` and `ram.avoid()` comes before the
  first allocation, `init_mmu()`, or an allocation may land on memory that
  is not free;
- `gic_v2::init()` and `map_device()` need the page tables;
- `init_boot_cpu()` needs the GIC, since it sets up the boot CPU's
  interface;
- hwinfo ends with `finish()`, and everything ends with `boot()`.

## The callouts a board writes

A board writes callouts with `callout!` (Chapter 7) for its own devices:
its debug UART's `display_char`, `poll_key` and `break_detect` (unless
the UART is a PL011, for which the library has them), and usually a
reboot. They follow the rules of Chapter 7: patch the device's virtual
address in, use nothing but the arguments, the system page and the
patched values.

> [!TIP]
> Write the startup driver for the UART first, and check that startup's
> own messages appear with `-v`; then write the callouts, which do the
> same in assembly. A board that prints nothing after "Starting next
> program" usually has a broken `display_char` callout.

## Building

A board crate is a `no_std` static library (`crate-type = ["staticlib"]`)
for `aarch64-unknown-none-softfloat`. The startup program mkifs takes is a
partial link of it, with the library's linker script (Chapter 2):

```text
cargo build --release -p my-board --target aarch64-unknown-none-softfloat
ld.lld -r -u _start -T path/to/oscraft-startup/startup.ld \
    -o startup-my-board target/aarch64-unknown-none-softfloat/release/libmy_board.a
```

mkifs then finds `startup-my-board` through its search path, as it would
QNX's, and the build file names it in its bootstrap section.

How a board releases its CPUs is entirely its `release()` function: a
board whose firmware implements PSCI makes its `CPU_ON` call there. An
interrupt controller is another matter: the library has the GICv2 only,
and a board with a GICv3 needs a GICv3 module first, next to
`src/arch/aarch64/gic_v2.rs`.
