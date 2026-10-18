use super::*;

fn allowed(tid: &str) -> String {
    std::fs::read_to_string(format!("/proc/{tid}/status")).unwrap().lines()
        .find_map(|line| line.strip_prefix("Cpus_allowed_list:")).unwrap().trim().to_string()
}

fn pin(tid: &str, cpus: &str) {
    assert!(Command::new("taskset").args(["-pc", cpus, tid]).stdout(Stdio::null()).status().unwrap().success());
    assert_eq!(allowed(tid), cpus);
}

fn runtime(tid: &str) -> u64 {
    std::fs::read_to_string(format!("/proc/{tid}/schedstat")).unwrap()
        .split_whitespace().next().unwrap().parse().unwrap()
}

struct Restore {
    tid: String,
    cpus: String,
}

impl Drop for Restore {
    fn drop(&mut self) {
        let _ = Command::new("taskset").args(["-pc", &self.cpus, &self.tid]).stdout(Stdio::null()).status();
    }
}

#[test]
#[ignore = "controlled CPU-placement diagnostic for the unchanged real empty-window transport"]
fn empty_window_transport_probe() {
    let host = std::fs::read_to_string("/proc/sys/kernel/hostname").unwrap();
    eprintln!("empty-window-transport host={}", host.trim());
    let tid = std::fs::read_link("/proc/thread-self").unwrap().file_name().unwrap().to_str().unwrap().to_string();
    let restore = Restore { cpus: allowed(&tid), tid };
    let cpus: Vec<u32> = restore.cpus.split(',').flat_map(|part| {
        let (first, last) = part.split_once('-').unwrap_or((part, part));
        first.parse::<u32>().unwrap()..=last.parse::<u32>().unwrap()
    }).take(3).collect();
    assert_eq!(cpus.len(), 3, "the placement diagnostic needs three allowed CPUs");
    let d = TestDir::new("empty-window-transport");
    let mut backend = Backend::start(&d);
    let empty = r#"{"c":"window","start":0,"count":0}"#;
    backend.ask(empty);
    let main = backend.child.id().to_string();
    let readers: Vec<String> = std::fs::read_dir(format!("/proc/{main}/task")).unwrap()
        .map(|entry| entry.unwrap().file_name().to_str().unwrap().to_string())
        .filter(|tid| tid != &main).collect();
    assert_eq!(readers.len(), 1, "only the stdin reader exists");
    let reader = &readers[0];
    pin(&restore.tid, &cpus[0].to_string());
    for (label, main_cpu, reader_cpu) in [("same-cpu", cpus[0], cpus[0]), ("separate-cpus", cpus[1], cpus[2])] {
        pin(&main, &main_cpu.to_string());
        pin(reader, &reader_cpu.to_string());
        for _ in 0..10 { backend.ask(empty); }
        let before = runtime(&main) + runtime(reader);
        let mut times = Vec::with_capacity(1000);
        for _ in 0..1000 {
            let at = Instant::now();
            let reply = backend.ask(empty);
            times.push(at.elapsed().as_nanos());
            assert!(reply.contains(r#""rows":[],"kinds":[],"ms":0.000,"listing":0"#));
        }
        let cpu = runtime(&main) + runtime(reader) - before;
        times.sort_unstable();
        eprintln!("empty-window-transport placement={label} client_cpu={} main_cpu={main_cpu} reader_cpu={reader_cpu} n=1000 p50_us={:.3} p95_us={:.3} backend_cpu_us_per_request={:.3}",
            cpus[0], times[499] as f64 / 1000.0, times[949] as f64 / 1000.0, cpu as f64 / 1_000_000.0);
    }
}
