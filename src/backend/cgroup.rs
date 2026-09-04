use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

const CGROUP_ROOT: &str = "/sys/fs/cgroup";
const EMPTY_WAIT: Duration = Duration::from_secs(2);
const POLL_STEP: Duration = Duration::from_millis(10);

#[derive(Clone, Copy)]
pub struct Limits {
    pub high: u64,
    pub max: u64,
    pub swap_max: u64,
}

pub const LIMITS: Limits = Limits { high: 768 * 1024 * 1024, max: 1024 * 1024 * 1024, swap_max: 0 };

#[derive(Default, Debug, PartialEq)]
pub struct Events {
    pub max: u64,
    pub oom: u64,
    pub oom_kill: u64,
    pub peak: u64,
}

pub struct Scope {
    root: PathBuf,
    next: u64,
}

pub struct Job {
    path: PathBuf,
}

impl Scope {
    pub fn enter() -> io::Result<Scope> {
        let text = fs::read_to_string("/proc/self/cgroup")?;
        let relative = unified_path(&text)?;
        let root = descendant(Path::new(CGROUP_ROOT), &relative)?;
        let controllers = fs::read_to_string(root.join("cgroup.controllers"))?;
        if !controllers.split_whitespace().any(|c| c == "memory") {
            return Err(io::Error::other("the scope does not delegate the memory controller"));
        }
        let supervisor = root.join("supervisor");
        fs::create_dir(&supervisor)?;
        fs::write(supervisor.join("cgroup.procs"), std::process::id().to_string())?;
        fs::write(root.join("cgroup.subtree_control"), "+memory")?;
        Ok(Scope { root, next: 0 })
    }

    pub fn job(&mut self, limits: Limits) -> io::Result<Job> {
        let name = self.next_job_name()?;
        let path = self.root.join(name);
        fs::create_dir(&path)?;
        let configured = (|| {
            fs::write(path.join("memory.high"), limits.high.to_string())?;
            fs::write(path.join("memory.max"), limits.max.to_string())?;
            fs::write(path.join("memory.swap.max"), limits.swap_max.to_string())?;
            fs::write(path.join("memory.oom.group"), "1")?;
            Ok::<(), io::Error>(())
        })();
        if let Err(error) = configured {
            let _ = fs::remove_dir(&path);
            return Err(error);
        }
        Ok(Job { path })
    }

    pub(crate) fn job_leaf_count(&self) -> io::Result<usize> {
        let mut count = 0;
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() && entry.file_name().to_string_lossy().starts_with("job-") {
                count += 1;
            }
        }
        Ok(count)
    }

    fn next_job_name(&mut self) -> io::Result<String> {
        let name = format!("job-{}", self.next);
        self.next = self.next.checked_add(1).ok_or_else(|| io::Error::other("cgroup job counter overflow"))?;
        Ok(name)
    }
}

impl Job {
    pub fn place(&self, pid: u32) -> io::Result<()> {
        fs::write(self.path.join("cgroup.procs"), pid.to_string())
    }

    pub fn finish(self) -> io::Result<Events> {
        let mut first_error = fs::write(self.path.join("cgroup.kill"), "1").err();
        let events = read_events(&self.path)
            .map_err(|error| {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            })
            .ok();
        let deadline = Instant::now() + EMPTY_WAIT;
        loop {
            match fs::read_to_string(self.path.join("cgroup.events")) {
                Ok(text) if populated(&text) == Some(false) => break,
                Ok(_) if Instant::now() < deadline => std::thread::sleep(POLL_STEP),
                Ok(_) => {
                    first_error.get_or_insert_with(|| io::Error::other("cgroup stayed populated after kill"));
                    break;
                }
                Err(error) => {
                    first_error.get_or_insert(error);
                    break;
                }
            }
        }
        if let Err(error) = fs::remove_dir(&self.path) {
            first_error.get_or_insert(error);
        }
        match (first_error, events) {
            (Some(error), _) => Err(error),
            (None, Some(events)) => Ok(events),
            (None, None) => Err(io::Error::other("cgroup accounting was unavailable")),
        }
    }
}

fn unified_path(text: &str) -> io::Result<PathBuf> {
    let mut found = text.lines().filter_map(|line| line.strip_prefix("0::"));
    let one = found.next().ok_or_else(|| io::Error::other("no unified cgroup entry"))?;
    if found.next().is_some() {
        return Err(io::Error::other("more than one unified cgroup entry"));
    }
    let path = PathBuf::from(one);
    if safe_absolute(&path) {
        Ok(path)
    } else {
        Err(io::Error::other("unsafe unified cgroup path"))
    }
}

fn descendant(root: &Path, path: &Path) -> io::Result<PathBuf> {
    if !safe_absolute(path) {
        return Err(io::Error::other("cgroup path is not an absolute descendant"));
    }
    Ok(root.join(path.strip_prefix("/").map_err(io::Error::other)?))
}

fn safe_absolute(path: &Path) -> bool {
    path.is_absolute()
        && path.components().all(|part| matches!(part, Component::RootDir | Component::Normal(_)))
        && path.to_str().is_some_and(|raw| raw.split('/').all(|part| part != "." && part != ".."))
}

fn parse_counter(text: &str, wanted: &str) -> io::Result<u64> {
    text.lines()
        .find_map(|line| {
            let (name, value) = line.split_once(' ')?;
            (name == wanted).then(|| value.parse::<u64>().ok()).flatten()
        })
        .ok_or_else(|| io::Error::other(format!("missing cgroup counter {wanted}")))
}

fn populated(text: &str) -> Option<bool> {
    text.lines().find_map(|line| match line {
        "populated 0" => Some(false),
        "populated 1" => Some(true),
        _ => None,
    })
}

fn read_events(path: &Path) -> io::Result<Events> {
    let text = fs::read_to_string(path.join("memory.events.local"))?;
    let peak = fs::read_to_string(path.join("memory.peak"))?.trim().parse().map_err(io::Error::other)?;
    Ok(Events { max: parse_counter(&text, "max")?, oom: parse_counter(&text, "oom")?, oom_kill: parse_counter(&text, "oom_kill")?, peak })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cgroup_selects_the_one_unified_line_and_ignores_v1() {
        let got = unified_path("1:net_cls:/\n0::/user.slice/a.scope\n").unwrap();
        assert_eq!(got, PathBuf::from("/user.slice/a.scope"));
        assert!(unified_path("1:net_cls:/\n").is_err());
        assert!(unified_path("0::/a\n0::/b\n").is_err());
    }

    #[test]
    fn cgroup_rejects_paths_that_can_escape_the_mount() {
        for path in ["relative", "/a/../b", "/a/./b"] {
            assert!(descendant(Path::new(CGROUP_ROOT), Path::new(path)).is_err(), "accepted {path}");
        }
        assert_eq!(descendant(Path::new(CGROUP_ROOT), Path::new("/a/b")).unwrap(), PathBuf::from("/sys/fs/cgroup/a/b"));
    }

    #[test]
    fn cgroup_events_are_read_by_exact_name() {
        let text = "low 1\nhigh 2\nmax 3\noom 4\noom_kill 5\noom_group_kill 6\n";
        assert_eq!(parse_counter(text, "max").unwrap(), 3);
        assert_eq!(parse_counter(text, "oom").unwrap(), 4);
        assert_eq!(parse_counter(text, "oom_kill").unwrap(), 5);
        assert!(parse_counter(text, "missing").is_err());
    }

    #[test]
    fn cgroup_job_names_are_local_and_monotonic() {
        let mut scope = Scope { root: PathBuf::from("/tmp/not-used"), next: 7 };
        let first = scope.next_job_name().unwrap();
        let second = scope.next_job_name().unwrap();
        assert_eq!((first.as_str(), second.as_str()), ("job-7", "job-8"));
    }
}
