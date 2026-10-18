//! Read-only whole-process-tree capture, run externally after a native scene has settled.
#[path = "../src/perf/memory_probe.rs"]
pub mod memory_probe;

use memory_probe::{account, parse_rollup, parse_smaps, total, Memory};
use std::{collections::BTreeMap, error::Error, fmt::Write, fs, path::Path};

type Identity = (u32, u64);

fn identity(pid: u32) -> Result<Identity, Box<dyn Error>> {
    let text = fs::read_to_string(format!("/proc/{pid}/stat"))?;
    let (_, fields) = text.rsplit_once(')').ok_or("invalid proc stat")?;
    let fields: Vec<_> = fields.split_whitespace().collect();
    if fields.first() == Some(&"Z") {
        return Err("process exited before sampling".into());
    }
    Ok((
        fields.get(1).ok_or("missing PPID")?.parse()?,
        fields.get(19).ok_or("missing start time")?.parse()?,
    ))
}

fn descendants(root: u32) -> Result<BTreeMap<u32, Identity>, Box<dyn Error>> {
    let root_identity = identity(root)?;
    let mut all = BTreeMap::new();
    for entry in fs::read_dir("/proc")? {
        let entry = entry?;
        if let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        {
            if let Ok(id) = identity(pid) {
                all.insert(pid, id);
            }
        }
    }
    let mut kept = BTreeMap::from([(root, root_identity)]);
    loop {
        let count = kept.len();
        for (&pid, &id) in &all {
            if kept.contains_key(&id.0) {
                kept.insert(pid, id);
            }
        }
        if kept.len() == count {
            break;
        }
    }
    Ok(kept)
}

fn escaped(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

fn row(table: &mut String, pid: u32, kind: &str, class: &str, memory: Memory) {
    writeln!(
        table,
        "{pid}\t{kind}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
        escaped(class),
        memory.rss_kib,
        memory.pss_kib,
        memory.private_clean_kib,
        memory.private_dirty_kib,
        memory.uss_kib(),
        memory.private_hugetlb_kib
    )
    .expect("writing to a string");
}

fn capture(root: u32, out: &Path) -> Result<(), Box<dyn Error>> {
    let parent = out
        .parent()
        .ok_or("sample needs a marked parent")?
        .canonicalize()?;
    if !parent.join(".flea-test-sandbox").is_file() || !out.is_absolute() {
        return Err("sample must be absolute with a marked parent".into());
    }
    fs::create_dir(out)?;
    let before = descendants(root)?;
    let mut tree = Memory::default();
    let mut table = String::from("pid\tkind\tclass\trss_kib\tpss_kib\tprivate_clean_kib\tprivate_dirty_kib\tuss_kib\tprivate_hugetlb_kib\n");
    for (&pid, &id) in &before {
        if identity(pid)? != id {
            return Err("PID changed before sampling".into());
        }
        let proc = format!("/proc/{pid}");
        let dir = out.join(pid.to_string());
        fs::create_dir(&dir)?;
        let exe = fs::read_link(format!("{proc}/exe"))?;
        let exe = exe.to_str().ok_or("non-UTF-8 executable path")?;
        fs::write(
            dir.join("identity.txt"),
            format!(
                "pid={pid}\nppid={}\nstart_ticks={}\nexe={exe}\n",
                id.0, id.1
            ),
        )?;
        fs::write(dir.join("cgroup"), fs::read(format!("{proc}/cgroup"))?)?;
        let rollup_before = fs::read_to_string(format!("{proc}/smaps_rollup"))?;
        let smaps = fs::read_to_string(format!("{proc}/smaps"))?;
        let rollup_after = fs::read_to_string(format!("{proc}/smaps_rollup"))?;
        fs::write(dir.join("smaps_rollup.before"), &rollup_before)?;
        fs::write(dir.join("smaps"), &smaps)?;
        fs::write(dir.join("smaps_rollup.after"), &rollup_after)?;
        let mappings = parse_smaps(&smaps)?;
        let classes = account(&mappings, exe)?;
        for (class, memory) in &classes {
            row(&mut table, pid, "mapping", class, *memory);
        }
        let sum = total(&classes)?;
        tree = tree.checked_add(sum)?;
        row(&mut table, pid, "smaps", "total", sum);
        row(
            &mut table,
            pid,
            "rollup_before",
            "total",
            parse_rollup(&rollup_before)?,
        );
        row(
            &mut table,
            pid,
            "rollup_after",
            "total",
            parse_rollup(&rollup_after)?,
        );
        fs::write(dir.join("mapping-count"), format!("{}\n", mappings.len()))?;
        let fds = dir.join("fdinfo");
        fs::create_dir(&fds)?;
        for entry in fs::read_dir(format!("{proc}/fdinfo"))? {
            let entry = entry?;
            let text = fs::read_to_string(entry.path())?;
            if text.lines().any(|line| line.starts_with("drm-driver:")) {
                fs::write(fds.join(entry.file_name()), text)?;
            }
        }
        if identity(pid)? != id {
            return Err("PID changed during sampling".into());
        }
    }
    if before != descendants(root)? {
        return Err("process tree changed during sampling; discard this sample".into());
    }
    row(&mut table, 0, "tree_smaps", "total", tree);
    fs::write(out.join("tree.tsv"), table)?;
    println!(
        "MEMORY_PROBE PASS processes={} tree_pss_kib={} tree_uss_kib={}",
        before.len(),
        tree.pss_kib,
        tree.uss_kib()
    );
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: memory-probe GUI_PID ABSOLUTE_NEW_SAMPLE_DIRECTORY".into());
    }
    let root: u32 = args[0].parse()?;
    if root == 0 {
        return Err("PID must be positive".into());
    }
    capture(root, Path::new(&args[1]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    #[test]
    fn a_real_process_tree_retains_raw_evidence_and_refuses_overwrite() {
        let parent = std::env::current_dir().unwrap().join(".superpowers/g1-15");
        fs::create_dir_all(&parent).unwrap();
        let output = Command::new("/usr/bin/mktemp")
            .args(["-d", parent.join("probe.XXXXXXXX").to_str().unwrap()])
            .output()
            .unwrap();
        assert!(output.status.success());
        let root = String::from_utf8(output.stdout).unwrap();
        let root = Path::new(root.trim());
        fs::write(root.join(".flea-test-sandbox"), "").unwrap();
        let mut child = Command::new("/usr/bin/sleep")
            .arg("60")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let sample = root.join("sample");
        let result = capture(std::process::id(), &sample);
        child.kill().unwrap();
        child.wait().unwrap();
        result.unwrap();
        let table = fs::read_to_string(sample.join("tree.tsv")).unwrap();
        assert!(table.contains(&format!("{}\tsmaps\ttotal\t", child.id())));
        assert!(sample
            .join(child.id().to_string())
            .join("smaps_rollup.before")
            .is_file());
        assert!(sample
            .join(std::process::id().to_string())
            .join("smaps")
            .is_file());
        assert!(table.contains("0\ttree_smaps\ttotal\t"));
        assert!(capture(std::process::id(), &sample).is_err());
    }
}
