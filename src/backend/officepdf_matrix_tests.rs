use super::*;
use std::collections::{BTreeMap, BTreeSet};
use std::process::{Command, Stdio};

// The observer must not follow an unexpectedly large process tree without a bound.
const MAX_OBSERVED_PROCESSES: usize = 512;
// Short conversions need multiple mapping samples before the converter exits.
const MAPPING_SAMPLE_INTERVAL: Duration = Duration::from_millis(5);

// The supervisor reports cache visibility before and after the real converter while its private tmpfs still exists.
const OBSERVE_CACHE: &str = r#"
import json, os, subprocess, sys
def cache(path):
    return {'exists': os.path.isdir(path), 'writable': os.access(path, os.W_OK),
            'files': sorted(os.listdir(path)) if os.path.isdir(path) else []}
paths = ['/var/cache/fontconfig', '/.cache/fontconfig']
private = os.environ.get('XDG_CACHE_HOME', '')
if private:
    paths.append(private + '/fontconfig')
before = {path: cache(path) for path in paths}
converted = subprocess.run(sys.argv[1:], stdin=subprocess.DEVNULL)
after = {path: cache(path) for path in paths}
catalog = subprocess.run(['/usr/bin/fc-list', '--format', '%{file}\n'], capture_output=True, text=True)
files = sorted(set(catalog.stdout.splitlines()))
print('OFFICE_CACHE ' + json.dumps({'before': before, 'after': after,
      'font_files': len(files), 'ttc_files': sum(p.lower().endswith('.ttc') for p in files),
      'font_catalog_exit': catalog.returncode,
      'writable_cache_files': len(after.get(private + '/fontconfig', {}).get('files', []))}), flush=True)
sys.exit(converted.returncode if converted.returncode >= 0 else 128 - converted.returncode)
"#;

#[derive(Default)]
struct FontMaps {
    bytes: u64,
    count: usize,
    ttc: usize,
    vm_peak: u64,
    files: BTreeMap<String, (usize, u64)>,
}

fn descendants(leader: u32) -> BTreeSet<u32> {
    let mut pending = vec![leader];
    let mut seen = BTreeSet::new();
    while let Some(pid) = pending.pop() {
        if !seen.insert(pid) || seen.len() > MAX_OBSERVED_PROCESSES {
            continue;
        }
        let Ok(tasks) = std::fs::read_dir(format!("/proc/{pid}/task")) else {
            continue;
        };
        for task in tasks.flatten() {
            let Ok(children) = std::fs::read_to_string(task.path().join("children")) else {
                continue;
            };
            // Sample input: /proc/123/task/123/children contains "456 789 ", each pid followed by a space and no newline.
            pending.extend(
                children
                    .split_whitespace()
                    .filter_map(|p| p.parse::<u32>().ok()),
            );
        }
    }
    seen
}

fn font_maps(pid: u32) -> Option<FontMaps> {
    let proc = PathBuf::from(format!("/proc/{pid}"));
    if std::fs::read_link(proc.join("exe")).ok()?.file_name()? != "soffice.bin" {
        return None;
    }
    let mut snapshot = FontMaps::default();
    // Sample input: 7f000000-7f001000 r--p 00000000 08:01 123 /usr/share/fonts/example.ttc
    for line in std::fs::read_to_string(proc.join("maps")).ok()?.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.len() < 6 {
            continue;
        }
        let name = fields[5..].join(" ");
        let suffix = Path::new(&name)
            .extension()
            .and_then(|x| x.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !["ttf", "ttc", "otf", "otc"].contains(&suffix.as_str()) {
            continue;
        }
        let (start, end) = fields[0].split_once('-')?;
        let bytes = u64::from_str_radix(end, 16).ok()? - u64::from_str_radix(start, 16).ok()?;
        let entry = snapshot.files.entry(name).or_default();
        entry.0 += 1;
        entry.1 += bytes;
        snapshot.bytes += bytes;
        snapshot.count += 1;
        snapshot.ttc += usize::from(suffix == "ttc");
    }
    // Sample input: VmPeak:    123456 kB
    snapshot.vm_peak = std::fs::read_to_string(proc.join("status"))
        .ok()?
        .lines()
        .find_map(|line| {
            line.strip_prefix("VmPeak:")?
                .split_whitespace()
                .next()?
                .parse::<u64>()
                .ok()
        })
        .unwrap_or(0)
        * 1024;
    Some(snapshot)
}

fn matrix_cell(
    d: &TestDir,
    input: &Path,
    program: &str,
    bytes: u64,
    private_cache: bool,
    run: usize,
) -> bool {
    let work = Work::new(d.path(), "office-matrix").unwrap();
    d.assert_contains(&work.dir);
    let mut inner = vec![
        "/usr/bin/python3".to_string(),
        "-c".to_string(),
        OBSERVE_CACHE.to_string(),
    ];
    inner.extend(argv(program, input, &work.dir));
    let mut full = sandbox::wrap_office(&inner, input, &work.dir);
    let cap = full
        .iter_mut()
        .find(|argument| argument.starts_with("--as="))
        .unwrap();
    assert_eq!(
        cap, "--as=8589934592",
        "the matrix must start with the production Office cap"
    );
    *cap = format!("--as={bytes}");
    if private_cache {
        let at = full.len() - inner.len();
        full.splice(
            at..at,
            [
                "--setenv".into(),
                "XDG_CACHE_HOME".into(),
                "/tmp/flea-office-cache".into(),
            ],
        );
    }
    sandbox::add_status(&mut full, inner.len());
    let began = Instant::now();
    let mut jailed = crate::backend::jail::spawn_jailed(&full, |command| {
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
    })
    .unwrap();
    let mut stdout = jailed.child.stdout.take().unwrap();
    let output = std::thread::spawn(move || {
        let mut text = String::new();
        stdout.read_to_string(&mut text).unwrap();
        text
    });
    let errors = spawn_stderr_reader(jailed.child.stderr.take());
    let mut peak = FontMaps::default();
    let mut samples = 0;
    let status = loop {
        if let Some(status) = jailed.child.try_wait().unwrap() {
            break status;
        }
        if began.elapsed() > CONVERT_TIMEOUT {
            crate::backend::jail::kill_tree(&mut jailed);
            panic!("office matrix exceeded its wall deadline, AS={bytes}, cache={private_cache}, run={run}");
        }
        for pid in descendants(jailed.child.id()) {
            if let Some(snapshot) = font_maps(pid) {
                samples += 1;
                if snapshot.bytes >= peak.bytes {
                    peak = snapshot;
                }
            }
        }
        std::thread::sleep(MAPPING_SAMPLE_INTERVAL);
    };
    let output = output.join().unwrap();
    let errors = errors.join().unwrap();
    let pdf =
        produced_pdf(&work.dir).is_some_and(|p| std::fs::read(p).unwrap().starts_with(b"%PDF-"));
    eprintln!("OFFICE_MATRIX as={bytes} cache={private_cache} run={run} exit={status} pdf={pdf} elapsed_ms={} font_maps={} font_bytes={} ttc_maps={} vm_peak={} samples={samples}",
        began.elapsed().as_millis(), peak.count, peak.bytes, peak.ttc, peak.vm_peak);
    for (name, (count, mapped)) in peak.files {
        eprintln!("OFFICE_FONT as={bytes} cache={private_cache} run={run} maps={count} bytes={mapped} path={name:?}");
    }
    eprintln!("{output}");
    eprintln!("OFFICE_STDERR as={bytes} cache={private_cache} run={run}: {errors}");
    if bytes == sandbox::OFFICE_ADDRESS_SPACE_BYTES {
        assert!(
            status.success() && pdf,
            "a production-cap office conversion failed: {errors}"
        );
    }
    if pdf {
        assert!(
            status.success(),
            "a valid PDF requires a successful converter: {errors}"
        );
        assert!(
            samples > 0,
            "the live mapping observer must see soffice.bin"
        );
        assert!(
            output.contains("OFFICE_CACHE "),
            "the inside-box cache observer must answer"
        );
        if private_cache {
            assert!(
                !output.contains("\"writable_cache_files\": 0"),
                "a writable private cache must receive fontconfig entries"
            );
        }
    }
    pdf
}

#[test]
#[ignore = "native office limit/cache measurement, explicitly run through the host broker"]
fn boxed_office_limit_and_cache_matrix() {
    let program = find_program(PROGRAM, &std::env::var("PATH").unwrap_or_default())
        .expect("LibreOffice is required");
    assert!(
        !skipped(),
        "the native office matrix requires the real sandbox"
    );
    let d = TestDir::new("officepdf-matrix");
    d.assert_contains(d.path());
    let generated = Command::new("python3")
        .args(["-B", "tests/fixtures/office/generate.py"])
        .arg(d.path())
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let input = d.join("report.docx");
    for private_cache in [false, true] {
        for bytes in [
            64 * 1024 * 1024,
            1024 * 1024 * 1024,
            2 * 1024 * 1024 * 1024,
            4 * 1024 * 1024 * 1024,
        ] {
            for run in 1..=3 {
                let pdf = matrix_cell(&d, &input, &program, bytes, private_cache, run);
                if bytes == 64 * 1024 * 1024 {
                    assert!(!pdf, "the deliberately starved control must fail");
                }
            }
        }
    }
}
