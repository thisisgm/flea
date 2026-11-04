use super::*;
use flea::i18n::{I18n, Paths};
use flea_gui::backend::{Backend, Event, Located};
use flea_gui::navigation::{folder_state::Order, Listing, Refusal};
use std::{path::PathBuf, process::Command as Process};

fn root() -> PathBuf {
    let parent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.superpowers/g1-03");
    std::fs::create_dir_all(&parent).unwrap();
    let output = Process::new("mktemp")
        .arg("-d")
        .arg(parent.join("test.XXXXXXXX"))
        .output()
        .unwrap();
    assert!(output.status.success());
    let root = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
    assert!(root.is_absolute());
    std::fs::write(root.join(".flea-test-sandbox"), "g1-03\n").unwrap();
    root
}

fn next(backend: &Backend) -> Event {
    loop {
        match backend.replies.recv_blocking().unwrap() {
            Event::Reply(Reply::Other) => continue,
            event => return event,
        }
    }
}

#[test]
fn navigation_requests_reach_the_real_pipe_and_unsolicited_replies_come_back_typed() {
    let root = root();
    let script = r#"import json,pathlib,sys
root=pathlib.Path(sys.argv[1])
for line in sys.stdin:
 with (root/'requests').open('a') as output: output.write(line)
 request=json.loads(line)
 if request['c']=='list':
  print(json.dumps(dict(t='listed',path=request['path'],n=1)),flush=True)
  print(json.dumps(dict(t='rows',start=0,rows=[dict(n='a',d=False,s=0,m=0,p=420,i='text-x-generic')],listing=1)),flush=True)
  print(json.dumps(dict(t='changed',path=request['path'])),flush=True)
 if request['c']=='locate' and 'path' in request:
  print(json.dumps(dict(t='located',directory='/f/base',path=request['path'],index=42)),flush=True)
 if request['c']=='locate' and 'paths' in request:
  print(json.dumps(dict(t='located',directory='/f/base',id=request['id'],transferId=0,matches=[dict(path=request['paths'][0],index=7)],ok=True,error='')),flush=True)
 if request['c']=='paths':
  print(json.dumps(dict(t='paths',paths=['/f/base/a'])),flush=True)
 if request['c']=='quit': break
"#;
    let listing = Listing {
        path: "/f/ho\"me/".into(),
        first: 320,
        hidden: true,
        want_changed: false,
        order: Order {
            key: "mtime",
            desc: true,
        },
        folders_first: true,
        group_by_kind: false,
        hidden_last: true,
        select: Some("/f/ho\"me/a".into()),
    };
    let mut command = Process::new("python3");
    command.arg("-I").arg("-c").arg(script).arg(&root);
    let backend = Backend::start_with(command, flea_gui::backend::list_line(&listing)).unwrap();
    assert!(matches!(next(&backend), Event::Reply(Reply::Listed { .. })));
    assert!(matches!(next(&backend), Event::Reply(Reply::Rows { .. })));
    assert!(matches!(next(&backend), Event::Changed { path } if path == "/f/ho\"me/"));
    let order = Order {
        key: "size",
        desc: false,
    };
    backend
        .command(&Command::Sort {
            order,
            folders_first: false,
            group_by_kind: true,
            hidden_last: false,
        })
        .unwrap();
    backend
        .command(&Command::Locate {
            path: "/f/base/a".into(),
        })
        .unwrap();
    let Event::Located(single) = next(&backend) else {
        panic!()
    };
    assert_eq!(
        (
            single.directory.as_str(),
            single.path.as_str(),
            single.index,
            single.id
        ),
        ("/f/base", "/f/base/a", 42, None)
    );
    backend
        .command(&Command::LocateMany {
            paths: vec!["/f/base/b".into()],
            id: 2_147_483_649,
        })
        .unwrap();
    let Event::Located(many) = next(&backend) else {
        panic!()
    };
    assert_eq!(
        many,
        Located {
            directory: "/f/base".into(),
            path: String::new(),
            index: -1,
            id: Some(2_147_483_649),
            transfer: 0,
            ok: true,
            matches: vec![("/f/base/b".into(), 7)]
        }
    );
    backend
        .command(&Command::Paths(flea_gui::pane::requests::RowAction {
            listing: 1,
            rows: vec![0],
        }))
        .unwrap();
    assert!(matches!(next(&backend), Event::Paths(paths) if paths == ["/f/base/a"]));
    backend.command(&Command::Persist("{}".into())).unwrap();
    backend.quit();
    let drained = backend.drained();
    drained.recv_blocking().unwrap();
    let written = std::fs::read_to_string(root.join("requests")).unwrap();
    assert_eq!(
        written.lines().collect::<Vec<_>>(),
        [
            r#"{"c":"list","path":"/f/ho\"me/","first":320,"hidden":true,"wantChanged":false,"by":"mtime","desc":true,"foldersFirst":true,"groupByKind":false,"hiddenLast":true,"select":"/f/ho\"me/a"}"#,
            r#"{"c":"sort","by":"size","desc":false,"foldersFirst":false,"groupByKind":true,"hiddenLast":false}"#,
            r#"{"c":"locate","path":"/f/base/a"}"#,
            r#"{"c":"locate","paths":["/f/base/b"],"id":2147483649}"#,
            r#"{"c":"paths","listing":1,"rows":[0]}"#,
            r#"{"c":"quit"}"#,
        ],
        "persistence never reaches the backend pipe"
    );
}

#[test]
fn navigation_keys_resolve_with_english_fallback_and_follow_a_language_change() {
    let root = root();
    std::fs::create_dir_all(root.join("catalog/de")).unwrap();
    std::fs::create_dir_all(root.join("omarchy/default/i18n")).unwrap();
    std::fs::write(
        root.join("omarchy/default/i18n/locales.json"),
        r#"{"en":{},"de":{"name":"Deutsch"}}"#,
    )
    .unwrap();
    std::fs::write(root.join("language"), "en").unwrap();
    std::fs::write(
        root.join("catalog/de/flea.json"),
        r#"{"navigation.row_not_loaded":"Diese Zeile ist noch nicht geladen."}"#,
    )
    .unwrap();
    let mut catalog = I18n::load(Paths {
        omarchy: root.join("omarchy"),
        language_file: root.join("language"),
        catalogs: vec![root.join("catalog")],
    });
    assert_eq!(
        catalog.tr(Refusal::NotLoaded.key()),
        "That row has not loaded yet."
    );
    assert_eq!(
        catalog.tr("navigation.save_failed"),
        "The folder setting could not be saved."
    );
    std::fs::write(root.join("language.next"), "de\n").unwrap();
    std::fs::rename(root.join("language.next"), root.join("language")).unwrap();
    assert!(catalog.poll_changed());
    assert_eq!(
        catalog.tr(Refusal::NotLoaded.key()),
        "Diese Zeile ist noch nicht geladen."
    );
    assert_eq!(
        catalog.tr("navigation.save_failed"),
        "The folder setting could not be saved.",
        "English per-string fallback"
    );
    assert_eq!(
        catalog.tr(Refusal::Busy.key()),
        "A directory is already loading."
    );
}
