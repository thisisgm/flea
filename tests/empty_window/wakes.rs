use super::*;

// Per-thread context switches, voluntary and not, keyed by thread id.
fn switches(pid: u32) -> Vec<(String, u64, u64)> {
    let mut all = Vec::new();
    for entry in std::fs::read_dir(format!("/proc/{pid}/task")).unwrap() {
        let tid = entry.unwrap().file_name().to_str().unwrap().to_string();
        let status = std::fs::read_to_string(format!("/proc/{pid}/task/{tid}/status")).unwrap();
        let field = |key: &str| status.lines().find_map(|line| line.strip_prefix(key)).unwrap().trim().parse::<u64>().unwrap();
        all.push((tid, field("voluntary_ctxt_switches:"), field("nonvoluntary_ctxt_switches:")));
    }
    all
}

// Switches since the first snapshot: the main thread's voluntary ones, then every other thread's total.
fn woken(pid: u32, before: &[(String, u64, u64)]) -> (u64, u64) {
    let main = pid.to_string();
    let (mut own, mut others) = (0, 0);
    for (tid, voluntary, forced) in switches(pid) {
        let (was_voluntary, was_forced) = before.iter().find(|(t, _, _)| *t == tid).map_or((0, 0), |(_, v, f)| (*v, *f));
        if tid == main { own += voluntary - was_voluntary } else { others += voluntary - was_voluntary + forced - was_forced }
    }
    (own, others)
}

// Resident memory by mapping: each named file, the heap, the stack, and every anonymous mapping by size, with the mapped size.
fn mappings(pid: u32) -> Vec<(String, usize, usize, usize)> {
    let body = std::fs::read_to_string(format!("/proc/{pid}/smaps")).unwrap();
    let mut groups: Vec<(String, usize, usize, usize)> = Vec::new();
    let mut current = String::new();
    for line in body.lines() {
        let mut fields = line.split_whitespace();
        let first = fields.next().unwrap_or("");
        if first.contains('-') && !first.ends_with(':') {
            let perms = fields.next().unwrap_or("");
            let name = fields.nth(3).unwrap_or("");
            let (start, end) = first.split_once('-').unwrap();
            let kib = (usize::from_str_radix(end, 16).unwrap() - usize::from_str_radix(start, 16).unwrap()) / 1024;
            current = if name.is_empty() { format!("anon {perms} {kib}k") } else { format!("{} {perms}", name.rsplit('/').next().unwrap()) };
            if !groups.iter().any(|(n, _, _, _)| *n == current) { groups.push((current.clone(), 0, 0, 0)); }
            groups.iter_mut().find(|(n, _, _, _)| *n == current).unwrap().3 += kib;
        } else if let Some(value) = line.strip_prefix("Pss:").or_else(|| line.strip_prefix("Rss:")) {
            let kib: usize = value.split_whitespace().next().unwrap().parse().unwrap();
            let group = groups.iter_mut().find(|(n, _, _, _)| *n == current).unwrap();
            if line.starts_with("Pss:") { group.1 += kib } else { group.2 += kib }
        }
    }
    groups.retain(|(_, _, rss, _)| *rss > 0);
    groups.sort_by_key(|group| std::cmp::Reverse(group.1));
    groups
}

// Answered requests per gate run; the takeover helper waking for one of them under host load stays inside the slack below.
const REQUESTS: u64 = 500;

// The request path wakes one thread per request, the loop that answers it: the parked helper never runs, and a reader thread
// handing each line on would add one wake per request.
#[test]
fn an_empty_window_wakes_only_the_loop_thread() {
    let d = TestDir::new("empty-window-wakes");
    let mut backend = Backend::start(&d);
    let empty = r#"{"c":"window","start":999,"count":0}"#;
    for _ in 0..10 { backend.ask(empty); }
    let pid = backend.child.id();
    let before = switches(pid);
    for _ in 0..REQUESTS {
        assert_eq!(backend.ask(empty), r#"{"t":"rows","start":0,"rows":[],"kinds":[],"ms":0.000,"listing":0}"#);
    }
    let (own, others) = woken(pid, &before);
    eprintln!("empty-window wakes requests={REQUESTS} loop_sleeps={own} other_thread_switches={others}");
    assert!(others * 10 <= REQUESTS, "{others} switches of other threads in {REQUESTS} requests: a thread besides the loop handles them");
    assert!((own + others) * 10 <= REQUESTS * 11, "{own} loop sleeps and {others} other switches in {REQUESTS} requests: more than one wake per request");
    backend.quit();
}

#[test]
#[ignore = "real executable batches: empty-window latency, wakeups per request and idle memory by mapping"]
fn m9z_probe_batch() {
    let host = std::fs::read_to_string("/proc/sys/kernel/hostname").unwrap();
    let empty = r#"{"c":"window","start":999,"count":0}"#;
    for batch in 1..=2 {
        for round in 1..=5 {
            let d = TestDir::new("empty-window-batch");
            let mut backend = Backend::start(&d);
            for _ in 0..10 { backend.ask(empty); }
            let pid = backend.child.id();
            let before = switches(pid);
            let mut times = Vec::with_capacity(1000);
            for _ in 0..1000 {
                let at = Instant::now();
                let reply = backend.ask(empty);
                times.push(at.elapsed().as_nanos());
                assert!(reply.contains(r#""rows":[],"kinds":[]"#), "{reply}");
            }
            let (own, others) = woken(pid, &before);
            let memory = backend.memory();
            times.sort_unstable();
            eprintln!("m9z host={} batch={batch} round={round} n=1000 p50_us={:.3} p95_us={:.3} range_us={:.3}..{:.3} main_sleeps={own} other_switches={others} threads={} pss_kib={} rss_kib={}",
                host.trim(), times[499] as f64 / 1000.0, times[949] as f64 / 1000.0, times[0] as f64 / 1000.0, times[999] as f64 / 1000.0,
                before.len(), memory.0, memory.1);
            if batch == 2 && round == 5 {
                for (name, pss, rss, size) in mappings(pid) { eprintln!("m9z idle mapping pss_kib={pss} rss_kib={rss} size_kib={size} {name}"); }
            }
            backend.quit();
        }
    }
}
