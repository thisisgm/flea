use super::*;

// Owly v0.4.2 browse exposes a service ID; AWDL matches only a name or the radio's MAC.
fn identity_cli(root: &TestDir) -> String {
    root.file(
        "browse",
        "55f6cafca1c6   \"iPhone\" ([fe80::a8bb:ccff:fedd:ee01%7]:8770)\n",
    );
    root.file("peers", "aa:bb:cc:dd:ee:01|iPhone\n");
    root.script(
        "owly",
        r#"#!/bin/sh
root=${0%/*}
case "$1" in
  browse) cat "$root/browse"; exit 0 ;;
  send) shift ;;
  *) exit 64 ;;
esac
printf '%s\n' "$@" >> "$root/sends"
case "$1" in
  --to=*) target=${1#--to=} ;;
  --mac=*) target=${1#--mac=}; direct=1 ;;
  *) exit 64 ;;
esac
shift
[ "$1" = -- ] || exit 64
if [ -f "$root/mdns" ] && [ "${direct:-0}" = 0 ]; then
  while IFS='|' read -r id mac name; do
    if [ "$target" = "$id" ] || [ "$target" = "$name" ]; then
      printf '%s\n' "$mac" > "$root/destination"
      printf 'delivered\n'
      exit 0
    fi
  done < "$root/mdns"
fi
printf '%s\n' 'not browsing mDNS: simulated failure' >&2
[ ! -f "$root/wait" ] || { mv "$root/wait" "$root/waiting"; exec sleep 30; }
while IFS='|' read -r mac name; do
  if [ "$target" = "$mac" ] || { [ "${direct:-0}" = 0 ] && [ "$target" = "$name" ]; }; then
    printf '%s\n' "$mac" > "$root/destination"
    printf 'delivered\n'
    exit 0
  fi
done < "$root/peers"
printf 'owly: no AWDL peer answered as "%s"\n' "$target" >&2
exit 2
"#,
    )
    .to_string_lossy()
    .into_owned()
}

fn attempt(root: &TestDir, program: &str, peer: &airdrop::Peer) -> (String, String) {
    let mut selected = send(vec![root
        .file("-photo.png", "private")
        .to_string_lossy()
        .into_owned()]);
    selected.peer = peer.id.clone();
    let (tx, _rx) = mpsc::channel();
    run(
        Some(program.to_string()),
        3,
        &selected,
        &AtomicBool::new(false),
        &tx,
    )
}

#[test]
fn service_id_selection_does_not_retry_duplicate_changed_unrelated_or_missing_peers_by_name() {
    for peers in [
        "aa:bb:cc:dd:ee:02|iPhone\naa:bb:cc:dd:ee:01|iPhone\n",
        "aa:bb:cc:dd:ee:02|iPhone\n",
        "aa:bb:cc:dd:ee:01|Different receiver\n",
        "aa:bb:cc:dd:ee:02|Unrelated Mac\n",
        "",
    ] {
        let root = TestDir::new("airdrop-identity-refusal");
        let program = identity_cli(&root);
        let peer = discovered(&program);
        root.file("peers", peers);
        let (code, reason) = attempt(&root, &program, &peer);
        assert_eq!(code, "failed");
        assert_eq!(reason, format!("no AWDL peer answered as \"{}\"", peer.id));
        assert!(!root.join("destination").exists(), "{peers}");
        let argv = std::fs::read_to_string(root.join("sends")).unwrap();
        assert_eq!(argv.lines().count(), 3, "no automatic retries: {argv}");
        assert_eq!(argv.lines().next(), Some("--to=55f6cafca1c6"));
        assert!(sent_line(7, &code, &reason).contains(r#""ok":false"#));
    }
}

#[test]
fn service_id_selects_the_exact_mdns_destination_even_when_names_are_duplicates() {
    let root = TestDir::new("airdrop-identity-exact");
    let program = identity_cli(&root);
    let peer = discovered(&program);
    root.file(
        "mdns",
        "unrelated|aa:bb:cc:dd:ee:02|iPhone\n55f6cafca1c6|aa:bb:cc:dd:ee:01|iPhone\n",
    );
    assert_eq!(
        attempt(&root, &program, &peer),
        ("delivered".into(), String::new())
    );
    assert_eq!(
        std::fs::read_to_string(root.join("destination")).unwrap(),
        "aa:bb:cc:dd:ee:01\n"
    );
}

// Fixture check: these selectors reach the wrong peer here, so the refusal tests above can go red.
#[test]
fn stand_in_delivers_to_the_wrong_peer_on_a_name_or_unchecked_mac_selector() {
    for (selector, peers, destination) in [
        (
            "--to=iPhone",
            "aa:bb:cc:dd:ee:02|iPhone\naa:bb:cc:dd:ee:01|iPhone\n",
            "aa:bb:cc:dd:ee:02\n",
        ),
        (
            "--mac=aa:bb:cc:dd:ee:01",
            "aa:bb:cc:dd:ee:01|Different receiver\n",
            "aa:bb:cc:dd:ee:01\n",
        ),
    ] {
        let root = TestDir::new("airdrop-identity-unsafe-selector");
        let program = identity_cli(&root);
        let peer = discovered(&program);
        assert_eq!(peer.name, "iPhone");
        root.file("peers", peers);
        let output = std::process::Command::new(program)
            .args(["send", selector, "--"])
            .arg(root.file("private.png", "private"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "the stand-in exposes this unsafe alternative"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("destination")).unwrap(),
            destination
        );
    }
}

#[test]
fn cancelling_after_mdns_failure_never_tries_another_destination() {
    let root = TestDir::new("airdrop-identity-cancel");
    let program = identity_cli(&root);
    let peer = discovered(&program);
    root.file("wait", "");
    let mut selected = send(vec![root
        .file("private.png", "private")
        .to_string_lossy()
        .into_owned()]);
    selected.peer = peer.id.clone();
    let cancel = std::sync::Arc::new(AtomicBool::new(false));
    let flag = std::sync::Arc::clone(&cancel);
    let waiting = root.join("waiting");
    // Cancel lands only once the stand-in has failed mDNS and is waiting, never on a timer.
    let stopper = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !waiting.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        flag.store(true, Ordering::Relaxed);
    });
    let (tx, _rx) = mpsc::channel();
    let started = Instant::now();
    assert_eq!(
        run(Some(program), 3, &selected, &cancel, &tx),
        ("cancelled".into(), String::new())
    );
    stopper.join().unwrap();
    assert!(started.elapsed() < Duration::from_secs(5));
    assert!(root.join("waiting").exists(), "cancel after mDNS failure");
    assert!(!root.join("destination").exists());
    assert_eq!(
        std::fs::read_to_string(root.join("sends")).unwrap(),
        format!("--to={}\n--\n{}\n", peer.id, selected.paths[0]),
        "one attempt, no destination tried after cancel"
    );
}

fn discovered(program: &str) -> airdrop::Peer {
    let argv = airdrop::browse_argv(program);
    let output = std::process::Command::new(&argv[0])
        .args(&argv[1..])
        .output()
        .unwrap();
    assert!(output.status.success());
    airdrop::browse_line(std::str::from_utf8(&output.stdout).unwrap()).unwrap()
}

#[test]
fn a_discovered_service_id_is_refused_when_only_its_awdl_peer_answers() {
    let root = TestDir::new("airdrop-identity-fallback");
    let program = identity_cli(&root);
    let peer = discovered(&program);
    assert_eq!(
        (peer.id.as_str(), peer.name.as_str()),
        ("55f6cafca1c6", "iPhone")
    );
    let mut selected = send(vec![root
        .file("-photo.png", "private")
        .to_string_lossy()
        .into_owned()]);
    selected.peer = peer.id.clone();
    let (tx, _rx) = mpsc::channel();
    let result = run(Some(program), 3, &selected, &AtomicBool::new(false), &tx);
    assert_eq!(
        result,
        (
            "failed".into(),
            format!("no AWDL peer answered as \"{}\"", peer.id)
        )
    );
    assert!(
        !root.join("destination").exists(),
        "no file may reach another peer"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("sends")).unwrap(),
        format!("--to={}\n--\n{}\n", peer.id, selected.paths[0]),
        "one exact attempt, without an automatic name retry"
    );
}
