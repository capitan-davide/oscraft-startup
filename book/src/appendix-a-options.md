# Appendix A: Startup Options

Startup's options come from the build file's bootstrap section, after the
startup program's name. The library parses the letters of QNX's startup
library, so that existing build files work; Table A.1 lists the ones it
acts on. The others (`-A`, `-C`, `-c`, `-D`, `-E`, `-e`, `-F`, `-f`, `-H`,
`-I`, `-i`, `-j`, `-K`, `-M`, `-o`, `-Q`, `-R`, `-S`, `-T`, `-U`, `-X`,
`-Z`) are accepted and ignored. A board adds its own letters; the
Raspberry Pi 4's, for example, adds `-W`.

*Table A.1: The options the library acts on.*

| Option | Meaning |
|---|---|
| `-v` | More output on the debug console; repeat for more (`-vvv` lists the RAM given to the kernel) |
| `-P n` | Start at most `n` CPUs |
| `-N name` | The host name (`_CS_HOSTNAME`); `localhost` by default |
| `-r addr,size` | Keep `size` bytes at `addr` from the kernel and from startup's allocations |
| `-u reg` | The device tree's address is in `x0` at entry |
| `-u addr` | The device tree is at `addr` |

Sizes and addresses are numbers in C's notation (`0x1000`, `4096`,
`010`), and sizes may end in `k`, `m` or `g`.

> [!NOTE]
> QNX's `-D` selects the debug device by name from a board's list. The
> board crate chooses its debug device in code instead, so the library
> ignores `-D`.
