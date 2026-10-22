use std::env;
use std::io::{self, Write};
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::process;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

const PACKET_SIZE: usize = 1024; // below MTU 1500 to avoid fragmentation
const SAMPLE: f64 = 0.5; // realtime speed sampling interval (seconds)
const H: usize = 10; // graph height (rows)
const W: usize = 56; // graph width (braille columns = 112 data points)
const FLUSH_EVERY: u64 = 4096; // check stop/duration every N packets, not every packet

// braille dot bits per (left/right column, row from bottom)
const DOT_COL: [[u16; 4]; 2] = [[0x40, 0x04, 0x02, 0x01], [0x80, 0x20, 0x10, 0x08]];

static STOP: AtomicBool = AtomicBool::new(false);

// std has no signal API; the `signal` symbol is already linked from libc on unix
unsafe extern "C" {
    fn signal(signum: i32, handler: extern "C" fn(i32)) -> usize;
}

extern "C" fn on_sigint(_: i32) {
    STOP.store(true, Ordering::Relaxed);
}

fn usage() {
    println!(
        "usage: udp-speedtest [--size <bytes>] <gateway-ip> <port> <duration-seconds, 0=unlimited>"
    );
}

fn graph(history: &[f64]) -> Vec<String> {
    let cols = W * 2;
    let window = &history[history.len().saturating_sub(cols)..];
    let peak = window.iter().cloned().fold(f64::MIN, f64::max);
    let ceiling = (peak / 10.0).ceil().max(1.0) * 10.0;

    let mut rows = vec![format!(
        "\x1b[90m{} {:>5}\x1b[0m",
        "─".repeat(W),
        ceiling as u64
    )];
    let skip = cols - window.len(); // window is always right-aligned
    for r in 0..H {
        let mut line = String::with_capacity(W * 3);
        for c in 0..W {
            let mut cell = 0u16;
            for (k, col) in DOT_COL.iter().enumerate() {
                if let Some(&v) = (c * 2 + k).checked_sub(skip).and_then(|i| window.get(i)) {
                    let level = (v / ceiling * (H * 4 - 1) as f64 + 0.5) as usize;
                    for (b, &bit) in col.iter().enumerate() {
                        if level >= (H - 1 - r) * 4 + b {
                            cell |= bit;
                        }
                    }
                }
            }
            line.push(char::from_u32(0x2800 + cell as u32).unwrap());
        }
        rows.push(format!("\x1b[32m{}\x1b[0m", line));
    }
    rows.push(format!("\x1b[90m{} {:>5}\x1b[0m", "─".repeat(W), 0));
    rows
}

fn render(history: &[f64], cur: f64, avg: f64, min: f64, max: f64, elapsed: f64, painted: bool) {
    let mut lines = graph(history);
    lines.push(format!(
        "\x1b[1;32m {:7.1} Mbps \x1b[0m│ min {:5.1}  avg {:5.1}  max {:5.1} │ {:4.1}s",
        cur, min, avg, max, elapsed
    ));
    let mut out = String::new();
    if painted {
        out.push_str(&format!("\x1b[{}A", lines.len()));
    }
    for line in &lines {
        out.push_str("\x1b[K");
        out.push_str(line);
        out.push('\n');
    }
    print!("{}", out);
    let _ = io::stdout().flush();
}

fn sender(addr: SocketAddr, counters: Arc<Vec<AtomicU64>>, idx: usize, size: usize) {
    let sock = UdpSocket::bind("0.0.0.0:0").expect("bind udp socket");
    let buf = vec![0xA5u8; size];
    let mut local: u64 = 0;
    loop {
        for _ in 0..FLUSH_EVERY {
            if sock.send_to(&buf, addr).is_ok() {
                local += 1;
            }
        }
        counters[idx].fetch_add(local, Ordering::Relaxed);
        local = 0;
        if STOP.load(Ordering::Relaxed) {
            break;
        }
    }
}

fn main() {
    let mut args: Vec<String> = env::args().skip(1).collect();
    let mut size = PACKET_SIZE;
    if let Some(i) = args.iter().position(|a| a == "--size") {
        size = args.get(i + 1).and_then(|v| v.parse().ok()).unwrap_or(0);
        if size == 0 || size > 65507 {
            eprintln!("--size must be 1..=65507 (UDP payload bytes)");
            process::exit(1);
        }
        args.drain(i..=i + 1);
    }
    if args.len() != 3 {
        usage();
        process::exit(1);
    }
    let port: u16 = args[1].parse().unwrap_or_else(|_| {
        usage();
        process::exit(1);
    });
    let duration: u64 = args[2].parse().unwrap_or_else(|_| {
        usage();
        process::exit(1);
    });
    let addr: SocketAddr = (args[0].as_str(), port)
        .to_socket_addrs()
        .ok()
        .and_then(|mut a| a.next())
        .unwrap_or_else(|| {
            eprintln!("cannot resolve {}", args[0]);
            process::exit(1);
        });

    unsafe { signal(2, on_sigint) };

    let n = thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let counters: Arc<Vec<AtomicU64>> = Arc::new((0..n).map(|_| AtomicU64::new(0)).collect());
    println!("{} threads → {}", n, addr);

    let mut handles = Vec::new();
    for i in 0..n {
        let counters = Arc::clone(&counters);
        handles.push(thread::spawn(move || sender(addr, counters, i, size)));
    }

    let start = Instant::now();
    let mut last = start;
    let mut last_total: u64 = 0;
    let mut history: Vec<f64> = Vec::new();
    let mut painted = false;
    loop {
        if STOP.load(Ordering::Relaxed) {
            break;
        }
        thread::sleep(Duration::from_millis(50));
        let now = Instant::now();
        let elapsed = now.duration_since(start).as_secs_f64();
        if duration > 0 && elapsed >= duration as f64 {
            STOP.store(true, Ordering::Relaxed);
            break;
        }
        if now.duration_since(last).as_secs_f64() >= SAMPLE {
            let total: u64 = counters.iter().map(|c| c.load(Ordering::Relaxed)).sum();
            let dt = now.duration_since(last).as_secs_f64();
            let cur = (total - last_total) as f64 * size as f64 * 8.0 / dt / 1e6;
            history.push(cur);
            if history.len() > W * 2 {
                history.remove(0);
            }
            let avg = total as f64 * size as f64 * 8.0 / elapsed / 1e6;
            let min = history.iter().cloned().fold(f64::MAX, f64::min);
            let max = history.iter().cloned().fold(f64::MIN, f64::max);
            render(&history, cur, avg, min, max, elapsed, painted);
            painted = true;
            last_total = total;
            last = now;
        }
    }

    for h in handles {
        h.join().expect("sender thread panicked");
    }
    let total: u64 = counters.iter().map(|c| c.load(Ordering::Relaxed)).sum();
    let elapsed = start.elapsed().as_secs_f64();
    let avg = total as f64 * size as f64 * 8.0 / elapsed / 1e6;
    let (min, max) = match (
        history.iter().cloned().fold(f64::MAX, f64::min),
        history.iter().cloned().fold(f64::MIN, f64::max),
    ) {
        (f64::MAX, f64::MIN) => (avg, avg),
        (min, max) => (min, max),
    };
    println!(
        "{} packets in {:.1}s | min {:.1}, avg {:.1}, max {:.1} Mbps",
        total, elapsed, min, avg, max
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line_body(row: &str) -> &str {
        row.trim_start_matches("\x1b[32m")
            .trim_start_matches("\x1b[90m")
            .trim_end_matches("\x1b[0m")
    }

    #[test]
    fn graph_scales_to_multiple_of_ten_with_borders() {
        let rows = graph(&[166.5]);
        assert_eq!(rows.len(), H + 2);
        assert!(line_body(&rows[0]).ends_with(&format!("{:>5}", 170)));
        assert!(line_body(rows.last().unwrap()).ends_with(&format!("{:>5}", 0)));
    }

    #[test]
    fn graph_area_fills_from_bottom() {
        let rows = graph(&[50.0, 100.0]);
        let last_cell = |r: usize| line_body(&rows[r]).chars().last().unwrap();
        assert_eq!(last_cell(H), '⣿'); // bottom row is full
        assert_ne!(last_cell(1), '⣿'); // top row only has the right column filled
    }

    #[test]
    fn window_right_aligned_when_short() {
        let rows = graph(&[100.0]);
        assert!(line_body(&rows[1]).starts_with('⠀')); // left columns are empty
    }
}
