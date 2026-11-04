//! EY-564: a card's own connect request says its failure in the footer too, as v0.3.12 failMount does.
use super::{
    mount::{self, Mounts, Phase},
    request,
};
use crate::g2::pl1::process::Outcome;

#[test]
fn fix_fileops_2_a_card_request_failure_says_its_reason_in_the_footer() {
    let mut m = Mounts::default();
    m.open_share(request("smb://127.0.0.1/flea"), 0);
    let generation = m.generation;
    m.exited(generation, Phase::Mount, &Outcome::code(1), 10);
    let e = m.exited(generation, Phase::Info, &Outcome::code(1), 20);
    assert!(matches!(
        e.as_slice(),
        [
            mount::Effect::Completed {
                id: 1,
                error: Some("network.refused"),
                ..
            },
            mount::Effect::Say { note, origin: 7 }
        ] if note.key == "network.refused"
    ));
    m.open_share(request("smb://127.0.0.1/flea"), 30);
    let e = m.tick(15_030);
    assert!(matches!(
        e.as_slice(),
        [
            mount::Effect::Kill(_),
            mount::Effect::Completed {
                error: Some("network.no_answer"),
                ..
            },
            mount::Effect::Say { note, origin: 7 }
        ] if note.key == "network.no_answer"
    ));
}

#[test]
fn fix_fileops_2_a_card_mounted_as_failure_names_the_users_in_the_footer() {
    let mut m = Mounts::default();
    let mut r = request("smb://nas/test");
    r.mounted = true;
    m.open_share(r, 0);
    let g = m.generation;
    let path = "/run/user/1000/gvfs/smb-share:server=nas,share=test,user=zoe";
    let out = Outcome {
        stdout: format!("local path: {path}\n"),
        ..Outcome::default()
    };
    m.exited(g, Phase::Info, &out, 1);
    m.peeked(g, path, &[], true, "gm", 2);
    let names = [
        "smb-share:server=nas,share=test,user=dan".into(),
        "smb-share:server=nas,share=test,user=eve".into(),
    ];
    let e = m.peeked(g, "/run/user/1000/gvfs", &names, false, "gm", 3);
    assert!(
        matches!(
            e.as_slice(),
            [
                mount::Effect::Completed {
                    error: Some("network.mounted_as"),
                    args,
                    ..
                },
                mount::Effect::Say { note, origin: 7 }
            ] if args == &["dan, eve"] && note.key == "network.mounted_as" && note.args == ["dan, eve"]
        ),
        "{e:?}"
    );
}
