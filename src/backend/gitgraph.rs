// Git branch/commit graph for the listing: lane layout ported from Switchboard's
// `_assign_graph_lanes` (gitk / GitKraken style). Contestable worktree/run overlays stay out.
use crate::json::escape;
use crate::backend::opsreq::OpMsg;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;
use std::time::Duration;

const GIT_TIMEOUT: Duration = Duration::from_secs(10);
const DEFAULT_LIMIT: usize = 100;
const MAX_LIMIT: usize = 500;

#[derive(Clone, Debug, PartialEq)]
pub struct Edge {
    pub from: usize,
    pub to: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Commit {
    pub hash: String,
    pub short: String,
    pub parents: Vec<String>,
    pub refs: Vec<String>,
    pub subject: String,
    pub date: String,
    pub lane: usize,
    pub through: Vec<usize>,
    pub edges: Vec<Edge>,
    pub width: usize,
    pub is_head: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Status {
    pub path: String,
    pub repo: bool,
    pub root: String,
    pub branch: String,
    pub head: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Graph {
    pub path: String,
    pub root: String,
    pub head: String,
    pub branch: String,
    pub commits: Vec<Commit>,
    pub error: String,
}

// Sample input: a directory with .git answers true; a plain folder answers false.
pub fn is_git_repo(directory: &Path) -> bool {
    git_ok(directory, &["rev-parse", "--is-inside-work-tree"])
        .map(|out| out.trim().eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

pub fn repo_root(directory: &Path) -> Option<String> {
    git_ok(directory, &["rev-parse", "--show-toplevel"]).map(|out| out.trim().to_string())
}

pub fn head_hash(directory: &Path) -> Option<String> {
    git_ok(directory, &["rev-parse", "HEAD"]).map(|out| out.trim().to_string())
}

pub fn branch_name(directory: &Path) -> String {
    git_ok(directory, &["rev-parse", "--abbrev-ref", "HEAD"])
        .map(|out| out.trim().to_string())
        .unwrap_or_default()
}

pub fn status_of(path: &Path) -> Status {
    let path_s = path.to_string_lossy().to_string();
    if !is_git_repo(path) {
        return Status { path: path_s, repo: false, root: String::new(), branch: String::new(), head: String::new() };
    }
    let root = repo_root(path).unwrap_or_else(|| path_s.clone());
    let head = head_hash(Path::new(&root)).unwrap_or_default();
    let branch = branch_name(Path::new(&root));
    Status {
        path: path_s,
        repo: true,
        root,
        branch,
        head: if head.len() > 12 { head[..12].to_string() } else { head },
    }
}

pub fn graph_of(path: &Path, limit: usize) -> Graph {
    let path_s = path.to_string_lossy().to_string();
    let limit = limit.clamp(1, MAX_LIMIT);
    if !is_git_repo(path) {
        return Graph {
            path: path_s,
            root: String::new(),
            head: String::new(),
            branch: String::new(),
            commits: Vec::new(),
            error: "not a git repository".into(),
        };
    }
    let root = repo_root(path).unwrap_or_else(|| path_s.clone());
    let root_path = PathBuf::from(&root);
    let head = head_hash(&root_path).unwrap_or_default();
    let branch = branch_name(&root_path);
    let pretty = "%H%x00%P%x00%D%x00%s%x00%cI%x00%h";
    let count = format!("--max-count={}", limit);
    let out = match git_ok(&root_path, &["log", "--all", "--date-order", &count, &format!("--pretty=format:{}", pretty)]) {
        Some(text) => text,
        None => {
            return Graph {
                path: path_s,
                root,
                head: short_head(&head),
                branch,
                commits: Vec::new(),
                error: "git log failed".into(),
            };
        }
    };
    let mut commits = parse_log(&out);
    assign_lanes(&mut commits);
    for c in &mut commits {
        c.is_head = !head.is_empty() && c.hash == head;
    }
    Graph {
        path: path_s,
        root,
        head: short_head(&head),
        branch,
        commits,
        error: String::new(),
    }
}

// Async answers so a slow git never stalls the listing loop; rides OpMsg::Meta like jump.
pub fn request_status(id: usize, path: String, replies: Sender<OpMsg>) {
    std::thread::spawn(move || {
        let status = status_of(Path::new(&path));
        let _ = replies.send(OpMsg::Meta { line: status_line(id, &status) });
    });
}

pub fn request_graph(id: usize, path: String, limit: usize, replies: Sender<OpMsg>) {
    std::thread::spawn(move || {
        let graph = graph_of(Path::new(&path), if limit == 0 { DEFAULT_LIMIT } else { limit });
        let _ = replies.send(OpMsg::Meta { line: graph_line(id, &graph) });
    });
}

pub fn status_line(id: usize, status: &Status) -> String {
    format!(
        r#"{{"t":"gitstatus","id":{},"path":"{}","repo":{},"root":"{}","branch":"{}","head":"{}"}}"#,
        id,
        escape(&status.path),
        status.repo,
        escape(&status.root),
        escape(&status.branch),
        escape(&status.head)
    )
}

pub fn graph_line(id: usize, graph: &Graph) -> String {
    let commits: Vec<String> = graph.commits.iter().map(commit_json).collect();
    format!(
        r#"{{"t":"gitgraph","id":{},"path":"{}","root":"{}","head":"{}","branch":"{}","error":"{}","commits":[{}]}}"#,
        id,
        escape(&graph.path),
        escape(&graph.root),
        escape(&graph.head),
        escape(&graph.branch),
        escape(&graph.error),
        commits.join(",")
    )
}

fn commit_json(c: &Commit) -> String {
    let parents: Vec<_> = c.parents.iter().map(|p| format!("\"{}\"", escape(p))).collect();
    let refs: Vec<_> = c.refs.iter().map(|r| format!("\"{}\"", escape(r))).collect();
    let through: Vec<_> = c.through.iter().map(|n| n.to_string()).collect();
    let edges: Vec<_> = c.edges.iter().map(|e| format!(r#"{{"from":{},"to":{}}}"#, e.from, e.to)).collect();
    format!(
        concat!(
            r#"{{"hash":"{}","short":"{}","parents":[{}],"refs":[{}],"subject":"{}","date":"{}","#,
            r#""lane":{},"through":[{}],"edges":[{}],"width":{},"is_head":{}}}"#
        ),
        escape(&c.hash),
        escape(&c.short),
        parents.join(","),
        refs.join(","),
        escape(&c.subject),
        escape(&c.date),
        c.lane,
        through.join(","),
        edges.join(","),
        c.width,
        c.is_head
    )
}

fn short_head(head: &str) -> String {
    if head.len() > 12 { head[..12].to_string() } else { head.to_string() }
}

fn git_ok(directory: &Path, args: &[&str]) -> Option<String> {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(directory).args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = cmd.spawn().ok()?;
    let mut pipe = child.stdout.take()?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        use std::io::Read;
        let mut buf = String::new();
        let _ = pipe.read_to_string(&mut buf);
        let _ = tx.send(buf);
    });
    let text = match rx.recv_timeout(GIT_TIMEOUT) {
        Ok(text) => text,
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
    };
    let status = child.wait().ok()?;
    if !status.success() {
        return None;
    }
    Some(text)
}

// Mirror of Switchboard `_parse_git_refs`: drop HEAD deco and ref prefixes.
pub fn parse_refs(raw: &str) -> Vec<String> {
    let mut refs = Vec::new();
    for part in raw.split(',') {
        let mut name = part.trim();
        if let Some(rest) = name.strip_prefix("HEAD -> ") {
            name = rest;
        } else if let Some(rest) = name.strip_prefix("tag: ") {
            name = rest;
        } else if let Some(rest) = name.strip_prefix("refs/heads/") {
            name = rest;
        } else if let Some(rest) = name.strip_prefix("refs/remotes/") {
            name = rest;
        } else if let Some(rest) = name.strip_prefix("refs/tags/") {
            name = rest;
        }
        if !name.is_empty() && name != "HEAD" {
            refs.push(name.to_string());
        }
    }
    refs
}

pub fn parse_log(out: &str) -> Vec<Commit> {
    let mut commits = Vec::new();
    for line in out.lines() {
        let parts: Vec<&str> = line.split('\0').collect();
        if parts.len() < 6 {
            continue;
        }
        let full = parts[0].to_string();
        let parents = parts[1].split_whitespace().filter(|p| !p.is_empty()).map(|p| p.to_string()).collect();
        let refs = parse_refs(parts[2]);
        let subject = parts[3].to_string();
        let date = parts[4].to_string();
        let short = if parts[5].is_empty() {
            if full.len() >= 8 { full[..8].to_string() } else { full.clone() }
        } else {
            parts[5].to_string()
        };
        commits.push(Commit {
            hash: full,
            short,
            parents,
            refs,
            subject,
            date,
            lane: 0,
            through: Vec::new(),
            edges: Vec::new(),
            width: 1,
            is_head: false,
        });
    }
    commits
}

// Newest-first lane layout (gitk / GitKraken style). Port of Switchboard `_assign_graph_lanes`.
pub fn assign_lanes(commits: &mut [Commit]) {
    let present: std::collections::HashSet<String> = commits.iter().map(|c| c.hash.clone()).collect();
    let mut lanes: Vec<Option<String>> = Vec::new();

    for c in commits.iter_mut() {
        let incoming: Vec<usize> = lanes.iter().enumerate()
            .filter_map(|(i, v)| if v.as_deref() == Some(c.hash.as_str()) { Some(i) } else { None })
            .collect();
        let col = if let Some(&first) = incoming.first() {
            first
        } else {
            occupy(&mut lanes, &c.hash)
        };
        c.lane = col;
        c.through = lanes.iter().enumerate()
            .filter_map(|(i, v)| if v.is_some() { Some(i) } else { None })
            .collect();
        let parents: Vec<String> = c.parents.iter()
            .filter(|p| present.contains(*p))
            .cloned()
            .collect();
        let first = parents.first().cloned();
        for &i in &incoming {
            lanes[i] = None;
        }
        while col >= lanes.len() {
            lanes.push(None);
        }
        let mut edges = Vec::new();
        if let Some(ref first_hash) = first {
            lanes[col] = Some(first_hash.clone());
            edges.push(Edge { from: col, to: col });
        }
        for &i in incoming.iter().skip(1) {
            edges.push(Edge { from: i, to: col });
        }
        for p in parents.iter().skip(1) {
            let dest = lanes.iter().enumerate()
                .find_map(|(i, v)| if v.as_deref() == Some(p.as_str()) { Some(i) } else { None })
                .unwrap_or_else(|| occupy(&mut lanes, p));
            edges.push(Edge { from: col, to: dest });
        }
        c.parents = parents;
        c.edges = edges;
        c.width = lanes.len().max(col + 1);
    }
}

fn occupy(lanes: &mut Vec<Option<String>>, hash: &str) -> usize {
    for (i, v) in lanes.iter_mut().enumerate() {
        if v.is_none() {
            *v = Some(hash.to_string());
            return i;
        }
    }
    lanes.push(Some(hash.to_string()));
    lanes.len() - 1
}

#[cfg(test)]
#[path = "gitgraph_tests.rs"]
mod tests;
