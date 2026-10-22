# udp-speedtest

One-sided UDP throughput test for your LAN. Point it at your gateway and it
blasts UDP packets at it for N seconds — no iperf server, no second device.
Built for the common case where the machine running the test is the only
device on the network and the router can't run any tools.

## Run without installing

```bash
curl -fsSL https://raw.githubusercontent.com/dxnz-id/udp-speedtest/main/run.sh | sh -s -- 192.168.1.1 9999 10
```

Downloads the release binary for your platform (checksum-verified) and runs
it. Re-runs use the cached copy; a new release is picked up automatically.
Everything it writes lives in `~/.cache/udp-speedtest` — delete that folder
and nothing of this tool remains. Linux and macOS only; Windows users grab
the zip from the releases page.

## Build

```bash
cargo build --release
```

Zero dependencies, standard library only.

## Usage

```bash
./target/release/udp-speedtest <target-ip> <port> <duration-seconds>
```

```bash
./target/release/udp-speedtest 192.168.1.1 9999 10   # 10 second test
./target/release/udp-speedtest 192.168.1.1 9999 0    # unlimited, Ctrl+C to stop
./target/release/udp-speedtest --size 1472 192.168.1.1 9999 10
```

Use any unused UDP port on the target. `--size` sets the UDP payload in
bytes (default 1024, max 65507). Larger packets carry more throughput per
syscall, which matters on older CPUs that cap out on packets-per-second —
1472 (a full 1500-MTU Ethernet frame) can reach noticeably higher Mbps on
the same link. Above 1472 packets fragment on standard networks.

## Output

A braille area graph (btop style) redrawn every 0.5s, with the current rate
live underneath. Top border is the scale (rounded up to a multiple of 10),
bottom border is 0:

```
4 threads → 192.168.1.1:9999
──────────────────────────────────────────────────────   170
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢠⡄
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢀⣴⣿⣿⡄
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢀⣴⣾⣿⣿⣿⣿⣷
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢀⣠⣴⣿⣿⣿⣿⣿⣿⣿⣿
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢀⣠⣴⣾⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢀⣠⣴⣾⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿
⠀⠀⠀⠀⠀⠀⢀⣠⣴⣾⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿
⣀⣠⣴⣾⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿
⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿
⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿
──────────────────────────────────────────────────────     0
   166.5 Mbps │ min  66.1  avg 115.7  max 166.5 │  1.5s
192512 packets in 10.0s | min 46.6, avg 108.0, max 170.2 Mbps
```

Ctrl+C during an unlimited run prints the same summary.

## How it works

One sender thread per CPU core, each with its own UDP socket, running a
tight send loop. The main thread only samples counters and draws the graph,
so rendering never eats into the measured rate. Stop flag and counters are
plain atomics — no locks, no shared buffers.

## Notes

- Measures **this machine → target** only. The reverse direction needs a
  receiver on this side, which is the thing this tool exists to avoid.
- A single thread already saturates gigabit; extra threads only pay off at
  2.5G and above. If all cores hit 100% while throughput stalls, the CPU is
  the limit, not the link.
- A target that rejects the port (ICMP port unreachable) fails some sends
  and the counted rate reads low. Pick another port if results look wrong.

## License

MIT
