# Appendix B: Source Map

Every source file of the crate, and the chapter that explains it.

*Table B.1: The source files.*

| File | Contents | Chapter |
|---|---|---|
| `src/lib.rs` | The crate root: `entry!`, `crash!`, the panic handler | 4 |
| `src/startup.rs` | The `Startup` context and its operations, `boot()` | 4, 5, 13 |
| `src/global.rs` | `Global<T>`, startup's lock-free globals | 4 |
| `src/bootargs.rs` | The boot-arguments area mkifs fills in | 2 |
| `src/cmdline.rs` | Parsing the boot arguments; `getopt()`-style options | 4 |
| `src/console.rs` | The debug UARTs, `-D`, `kprintln!` | 4 |
| `src/pl011.rs` | Arm's PL011 UART as a debug device | 4 |
| `src/ram.rs` | Range sets and RAM bookkeeping | 5 |
| `src/syspage/mod.rs` | The system page builder, layout and writing | 6 |
| `src/syspage/asinfo.rs` | The asinfo tree | 5, 6 |
| `src/syspage/hwinfo.rs` | The hwinfo tree and its tags | 6 |
| `src/syspage/strings.rs` | The strings and typed strings sections | 6 |
| `src/callout.rs` | The callout framework: `callout!`, patching, output | 7 |
| `src/qtime.rs` | The qtime section and the timer period | 9 |
| `src/elf.rs` | Planning the loading of ELF segments | 11 |
| `src/ifs.rs` | The image file system; loading the bootstrap executables | 11 |
| `src/fdt.rs` | A flattened device tree reader | 14 |
| `src/abi/mod.rs` | The ABI types' module | 2, 6 |
| `src/abi/startup.rs` | The startup header and boot arguments | 2 |
| `src/abi/syspage.rs` | The system page and its sections | 6 |
| `src/abi/image.rs` | The image file system and bootstrap structures | 2, 11 |
| `src/abi/elf.rs` | ELF64 headers | 11 |
| `src/abi/layout_tests.rs` | The ABI types checked against `abi/layout.txt` | 15 |
| `src/arch/mod.rs` | The architectures | 1 |
| `src/arch/aarch64/mod.rs` | AArch64: per-CPU preparation | 3 |
| `src/arch/aarch64/entry.rs` | `_start`, the move to EL1 | 3 |
| `src/arch/aarch64/cache.rs` | Cleaning caches by set/way | 3 |
| `src/arch/aarch64/exceptions.rs` | Startup's exception vectors | 3 |
| `src/arch/aarch64/cpu.rs` | cpuinfo, cacheattr, ID registers, the timer | 9 |
| `src/arch/aarch64/gic_v2.rs` | The GICv2 and its callouts | 8 |
| `src/arch/aarch64/mmu.rs` | The kernel's page tables | 10 |
| `src/arch/aarch64/smp.rs` | Starting the secondary CPUs, `smp_spin` | 12 |
| `src/arch/aarch64/handoff.rs` | Turning the MMU on and jumping | 13 |
| `abi/layout.c` | The C layouts, as array sizes | 15 |
| `abi/layout.py` | Reading the layouts into `abi/layout.txt` | 15 |
| `abi/layout.txt` | The layouts of QNX's headers | 15 |
