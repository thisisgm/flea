use super::*;
use crate::backend::testdir::TestDir;
use std::path::Path;
use std::process::{Command, Stdio};

#[test]
fn parse_refs_drops_head_and_prefixes() {
    assert_eq!(parse_refs("HEAD -> main, origin/main, tag: v1.0"),
               vec!["main", "origin/main", "v1.0"]);
    assert_eq!(parse_refs("HEAD"), Vec::<String>::new());
    assert_eq!(parse_refs(""), Vec::<String>::new());
}

#[test]
fn linear_history_stays_on_one_lane() {
    let a = "aaa111";
    let b = "bbb222";
    let c = "ccc333";
    let log = format!(
        "{a}\0{b}\0HEAD -> main\0tip\02026-01-03T00:00:00+00:00\0aaa1\n\
         {b}\0{c}\0\0mid\02026-01-02T00:00:00+00:00\0bbb2\n\
         {c}\0\0\0root\02026-01-01T00:00:00+00:00\0ccc3"
    );
    let mut commits = parse_log(&log);
    assign_lanes(&mut commits);
    assert_eq!(commits.len(), 3);
    assert_eq!(commits[0].lane, 0);
    assert_eq!(commits[1].lane, 0);
    assert_eq!(commits[2].lane, 0);
    assert_eq!(commits[0].refs, vec!["main"]);
    assert_eq!(commits[0].edges, vec![Edge { from: 0, to: 0 }]);
    assert!(commits[2].edges.is_empty());
}

#[test]
fn merge_commit_keeps_second_parent_edge() {
    // Newest first: merge M of A and B, then A, then B.
    let m = "mmm000";
    let a = "aaa111";
    let b = "bbb222";
    let log = format!(
        "{m}\0{a} {b}\0HEAD -> main\0merge\02026-01-04T00:00:00+00:00\0mmm0\n\
         {a}\0\0\0left\02026-01-03T00:00:00+00:00\0aaa1\n\
         {b}\0\0\0right\02026-01-02T00:00:00+00:00\0bbb2"
    );
    let mut commits = parse_log(&log);
    assign_lanes(&mut commits);
    assert_eq!(commits[0].lane, 0);
    assert!(commits[0].edges.iter().any(|e| e.from == 0 && e.to != 0) || commits[0].edges.len() >= 2,
            "merge should emit a second-parent edge: {:?}", commits[0].edges);
    assert!(commits[0].width >= 2);
}

#[test]
fn real_repo_status_and_graph() {
    let dir = TestDir::new("gitgraph");
    let root = dir.path();
    run(root, &["git", "init", "-b", "main"]);
    run(root, &["git", "config", "user.email", "flea@test"]);
    run(root, &["git", "config", "user.name", "Flea"]);
    std::fs::write(root.join("a.txt"), "one\n").unwrap();
    run(root, &["git", "add", "a.txt"]);
    run(root, &["git", "-c", "commit.gpgsign=false", "commit", "-m", "first"]);
    std::fs::write(root.join("a.txt"), "two\n").unwrap();
    run(root, &["git", "add", "a.txt"]);
    run(root, &["git", "-c", "commit.gpgsign=false", "commit", "-m", "second"]);

    let status = status_of(root);
    assert!(status.repo);
    assert_eq!(status.branch, "main");
    assert!(!status.head.is_empty());
    assert_eq!(Path::new(&status.root), root);

    let graph = graph_of(root, 10);
    assert!(graph.error.is_empty(), "{}", graph.error);
    assert_eq!(graph.commits.len(), 2);
    assert_eq!(graph.commits[0].subject, "second");
    assert!(graph.commits[0].is_head);
    assert!(!graph.commits[1].is_head);
    assert_eq!(graph.commits[0].lane, 0);
    let line = graph_line(7, &graph);
    assert!(line.contains(r#""t":"gitgraph""#));
    assert!(line.contains(r#""id":7"#));
    assert!(line.contains("second"));
}

#[test]
fn merge_in_real_repo_opens_a_second_lane() {
    let dir = TestDir::new("gitgraph-merge");
    let root = dir.path();
    run(root, &["git", "init", "-b", "main"]);
    run(root, &["git", "config", "user.email", "flea@test"]);
    run(root, &["git", "config", "user.name", "Flea"]);
    std::fs::write(root.join("a.txt"), "base\n").unwrap();
    run(root, &["git", "add", "a.txt"]);
    run(root, &["git", "-c", "commit.gpgsign=false", "commit", "-m", "base"]);
    run(root, &["git", "checkout", "-b", "feature"]);
    std::fs::write(root.join("a.txt"), "feature\n").unwrap();
    run(root, &["git", "add", "a.txt"]);
    run(root, &["git", "-c", "commit.gpgsign=false", "commit", "-m", "on feature"]);
    run(root, &["git", "checkout", "main"]);
    std::fs::write(root.join("b.txt"), "main\n").unwrap();
    run(root, &["git", "add", "b.txt"]);
    run(root, &["git", "-c", "commit.gpgsign=false", "commit", "-m", "on main"]);
    run(root, &["git", "-c", "commit.gpgsign=false", "merge", "--no-ff", "-m", "merge feature", "feature"]);

    let graph = graph_of(root, 500);
    assert!(graph.error.is_empty(), "{}", graph.error);
    assert!(graph.commits.len() >= 4, "expected merge history, got {}", graph.commits.len());
    let max_width = graph.commits.iter().map(|c| c.width).max().unwrap_or(1);
    assert!(max_width >= 2, "expected multi-lane graph, max width {max_width}; lanes={:?}",
            graph.commits.iter().map(|c| (c.short.as_str(), c.lane, c.width, c.edges.len())).collect::<Vec<_>>());
    assert!(graph.commits.iter().any(|c| c.parents.len() > 1), "expected a merge commit with two parents");
}

#[test]
fn plain_folder_is_not_a_repo() {
    let dir = TestDir::new("gitgraph");
    let status = status_of(dir.path());
    assert!(!status.repo);
    let graph = graph_of(dir.path(), 10);
    assert_eq!(graph.error, "not a git repository");
    assert!(graph.commits.is_empty());
}

fn run(dir: &Path, argv: &[&str]) {
    let status = Command::new(argv[0])
        .args(&argv[1..])
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("spawn");
    assert!(status.success(), "{argv:?} failed");
}
