// What an add or a start may never do: hold a request past its deadline, land a tree past its bounds, or read a file whole.
use super::plugins::Env;
use super::plugins_tests::{ask, code, env, get, plugin, READY};
use super::testdir::TestDir;
use std::time::{Duration, Instant};

fn add(env: &Env, source: &str) -> &'static str {
    code(
        ask(
            env,
            &format!(r#"{{"c":"plugins","op":"add","id":1,"source":"{source}"}}"#),
        )
        .0,
    )
}

#[test]
fn rev_st2_enable_answers_within_the_start_deadline_for_a_plugin_that_never_reads() {
    let dir = TestDir::new("rev-st2-hello");
    let mut env = env(&dir);
    env.ready_deadline = Duration::from_millis(500);
    let src = dir.dir("big");
    dir.dir("big/bin");
    let fields: Vec<String> = (0..24)
        .map(|n| format!(r#"{{"key":"t{n}","type":"string","label":"T{n}","defaultValue":""}}"#))
        .collect();
    dir.file("big/manifest.json", &format!(
        r#"{{"schemaVersion":1,"id":"me.big","name":"Big","version":"1.0.0","author":"me","description":"d","kinds":["menu-action"],"entryPoints":{{"run":"bin/run"}},"menuActions":[{{"id":"go","label":"Go"}}],"schema":[{}]}}"#,
        fields.join(",")));
    // Starts, never reads its input, and exits on its own after 20 s.
    dir.script("big/bin/run", "#!/bin/sh\nexec sleep 20\n");
    add(&env, &src.display().to_string());
    let long = "a".repeat(4096);
    for n in 0..24 {
        let (done, _) = ask(
            &env,
            &format!(
                r#"{{"c":"plugins","op":"set","id":2,"plugin":"me.big","key":"t{n}","value":"{long}"}}"#
            ),
        );
        assert_eq!(code(done), "", "setting t{n} stored");
    }
    let started = Instant::now();
    let (_, listing) = ask(
        &env,
        r#"{"c":"plugins","op":"enable","id":3,"plugin":"me.big"}"#,
    );
    let took = started.elapsed();
    assert!(
        took < Duration::from_secs(5),
        "enable held the plugins request for {took:?} with a 500 ms start deadline"
    );
    assert_eq!(
        get(&listing, "me.big").failure.clone().map(|f| f.code),
        Some("start")
    );
}

#[test]
fn st2_fix_a_tree_with_too_many_files_is_refused_before_the_plugin_dir_exists() {
    let dir = TestDir::new("st2-count");
    let env = env(&dir);
    let src = plugin(&dir, "src", "me.many", READY);
    dir.dir("src/many");
    for n in 0..4096 {
        dir.file(&format!("src/many/{n}"), "");
    }
    assert_eq!(add(&env, &src.display().to_string()), "write");
    assert!(!env.dir.exists(), "nothing landed in the plugins folder");
}

#[test]
fn st2_fix_a_tree_over_the_byte_cap_is_refused_before_the_plugin_dir_exists() {
    let dir = TestDir::new("st2-bytes");
    let env = env(&dir);
    let src = plugin(&dir, "src", "me.huge", READY);
    // Sparse, so the test itself costs no disk: only its length is past the cap.
    let blob = std::fs::File::create(dir.join("src/blob")).unwrap();
    blob.set_len(super::pluginfs::MAX_BYTES + 1).unwrap();
    assert_eq!(add(&env, &src.display().to_string()), "write");
    assert!(!env.dir.exists(), "nothing landed in the plugins folder");
}

// Hands out zeros and records the largest read it was asked for.
struct Counting {
    left: u64,
    reads: usize,
    widest: usize,
}

impl std::io::Read for Counting {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.reads += 1;
        self.widest = self.widest.max(buf.len());
        let n = buf.len().min(self.left as usize);
        buf[..n].fill(0);
        self.left -= n as u64;
        Ok(n)
    }
}

#[test]
fn st2_fix_a_file_copies_through_one_bounded_buffer() {
    let size = 8 << 20;
    let mut from = Counting {
        left: size,
        reads: 0,
        widest: 0,
    };
    let copied = super::pluginfs::copy_stream(&mut from, &mut std::io::sink()).unwrap();
    assert_eq!(copied, size);
    assert!(
        from.widest <= super::pluginfs::CHUNK,
        "one read asked for {} bytes",
        from.widest
    );
    assert!(
        from.reads as u64 >= size / super::pluginfs::CHUNK as u64,
        "{} reads for {size} bytes",
        from.reads
    );
    let dir = TestDir::new("st2-stream");
    let env = env(&dir);
    let src = plugin(&dir, "src", "me.large", READY);
    std::fs::File::create(dir.join("src/large"))
        .unwrap()
        .set_len(size)
        .unwrap();
    assert_eq!(add(&env, &src.display().to_string()), "");
    assert_eq!(
        std::fs::metadata(env.dir.join("me.large/large"))
            .unwrap()
            .len(),
        size
    );
}
