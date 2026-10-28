# Preface

This book explains `oscraft-startup`, a library for writing the *startup
program* of a QNX OS 8.0 system, in Rust. Startup is the first program
that runs from a QNX boot image. It finds out what the board is,
describes it to the kernel in a structure called the *system page*, sets
up the MMU and the CPUs, and starts the kernel. QNX writes its startup
programs in C, on top of a library of its own, `libstartup.a`.
`oscraft-startup` takes that library's place: a board crate written with
it boots QNX's own, unmodified kernel.

The book is for readers who know systems programming and some Rust, but
not necessarily QNX or the AArch64 architecture. Each chapter first
explains the hardware or QNX concept it depends on (exception levels,
translation tables, interrupt controllers, the system page) and only
then shows how the library implements it. Code listings are taken from
the source when the book is built, so they always show the code as it
is.

The book is about the library alone. It mentions boards, mostly the
Raspberry Pi 4 with its BCM2711 SoC, only as examples of the decisions a
board has to make; the board's code has a book of its own.

## How the book is organized

**Part I** is background: what startup does and why it exists, and what
the boot image that holds it looks like.

**Part II** follows the library in the order a boot runs: the first
instructions, the context a board works with, physical memory, the system
page and its sections, the callouts, the interrupt controller, the CPUs
and timer, the kernel's address space, loading the kernel, starting the
other CPUs, and the jump into the kernel.

**Part III** is about using the library: writing a board crate, and
testing startup without and with hardware.

The appendices list the command-line options, map every source file to
the chapter that explains it, and define the terms.

## Conventions

- Paths are relative to the crate's root: `src/startup.rs`.
- *Listings* are code from the source; *figures* are diagrams; both are
  numbered per chapter.
- QNX's C names (`struct syspage_entry`, `init_smp()`) appear where a
  Rust item mirrors them, so that QNX's documentation can be read
  alongside.

> [!NOTE]
> Notes give background that is useful but not needed to follow the text.

> [!WARNING]
> Warnings point out things that are easy to get wrong when changing the
> code.
