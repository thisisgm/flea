//! The #305 proof harness: a scratch lane with a stand-in terminal and editor in the opener's stand-in slot, and the production window the card opens in.
use crate::backend::Backend;
use crate::g2::open_with_properties::lane::{Stand, STAND};
use crate::navigation::folder_state::Writer;
use crate::state::Shell;
use crate::window::{
    locale::Snapshot,
    root::Root,
    startup::{Arguments, Launch, Settings},
};
use gpui::{AppContext as _, Entity, TestAppContext, VisualTestContext};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

const MARKER: &str = ".flea-test-sandbox";

// Lanes share the one stand-in slot, so only one lane is alive at a time.
static LANE_SERIAL: Mutex<()> = Mutex::new(());

fn stand_slot() -> MutexGuard<'static, Option<Stand>> {
    STAND.lock().unwrap_or_else(PoisonError::into_inner)
}

// FL-B03: the lane hands its stand-ins to the opener's slot and never sets the process environment, which every parallel test reads.
pub(super) struct Lane {
    root: PathBuf,
    // Last on purpose: drop empties the stand-in slot first, then the guard is released.
    _serial: MutexGuard<'static, ()>,
}

impl Lane {
    pub(super) fn new() -> Self {
        let serial = LANE_SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("flea-gh305.{}.{stamp}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join(MARKER), "").unwrap();
        let bin = root.join("bin");
        std::fs::create_dir(&bin).unwrap();
        let at = root.display();
        let terminal = format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" >> '{at}/terminal.argv'\ncase $1 in --dir=*) shift ;; esac\n[ \"$1\" = -- ] && shift\nexec \"$@\"\n"
        );
        let editor = format!(
            "#!/bin/sh\nn=$(ls '{at}' | grep -c '^captured\\.')\ncp \"$1\" '{at}'/captured.$n\n[ -f '{at}'/reply.$n ] && cp '{at}'/reply.$n \"$1\"\nexit 0\n"
        );
        for (name, text) in [("xdg-terminal-exec", terminal), ("editor", editor)] {
            let path = bin.join(name);
            std::fs::write(&path, text).unwrap();
            std::process::Command::new("chmod")
                .arg("755")
                .arg(&path)
                .status()
                .unwrap();
        }
        *stand_slot() = Some(Stand {
            editor: bin.join("editor").display().to_string(),
            base: root.display().to_string(),
            terminal: bin.join("xdg-terminal-exec").display().to_string(),
        });
        Self {
            root,
            _serial: serial,
        }
    }

    pub(super) fn reply(&self, at: usize, names: &[&str]) {
        let mut text = names.join("\n");
        text.push('\n');
        std::fs::write(self.root.join(format!("reply.{at}")), text).unwrap();
    }

    // The names file the editor was handed on its at-th launch, one name per line.
    pub(super) fn captured(&self, at: usize) -> Option<Vec<String>> {
        let text = std::fs::read_to_string(self.root.join(format!("captured.{at}"))).ok()?;
        Some(text.lines().map(str::to_string).collect())
    }

    pub(super) fn launches(&self) -> usize {
        std::fs::read_dir(&self.root)
            .unwrap()
            .filter(|entry| {
                entry
                    .as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with("captured.")
            })
            .count()
    }

    // The lane removes its own flea-rename dir after every launch.
    pub(super) fn scratch_left(&self) -> Vec<String> {
        std::fs::read_dir(&self.root)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with("flea-rename"))
            .collect()
    }
}

impl Drop for Lane {
    fn drop(&mut self) {
        *stand_slot() = None;
        let root: &Path = &self.root;
        let named = root
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with("flea-gh305."));
        if root.is_absolute() && named && root.join(MARKER).is_file() {
            // Only the lane's own entries go; a foreign one another test left stays, and so does the root.
            if let Ok(entries) = std::fs::read_dir(root) {
                for entry in entries.flatten() {
                    if !is_own(&entry.file_name().to_string_lossy()) {
                        continue;
                    }
                    let path = entry.path();
                    let _ = match entry.file_type() {
                        Ok(kind) if kind.is_dir() => std::fs::remove_dir_all(&path),
                        _ => std::fs::remove_file(&path),
                    };
                }
            }
            let _ = std::fs::remove_dir(root);
        }
    }
}

// The entries a lane makes: its marker, bin, terminal.argv, captured.N, reply.N and any flea-rename scratch.
fn is_own(name: &str) -> bool {
    name == MARKER
        || name == "bin"
        || name == "terminal.argv"
        || name.starts_with("captured.")
        || name.starts_with("reply.")
        || name.starts_with("flea-rename")
}

// The production window: Root mounts the Shell and the card layers over it, as the native launch does.
pub(super) fn rooted(cx: &mut TestAppContext) -> (Entity<Shell>, &mut VisualTestContext) {
    // Only the backend process is replaced, as test_shell::open replaces it.
    cx.update(|cx| {
        cx.set_global(crate::state::constructor_fixture::Services {
            backend: |_, cx| {
                let (backend, requests) = Backend::test_channel();
                cx.on_release(move |_, _| drop(requests)).detach();
                Ok(backend)
            },
            monitor: None,
        })
    });
    let document = flea::jsondoc::parse(
        r#"{"keys":"default","view":"list","places":{"rail":"shown","showHome":true,"showTrash":true}}"#,
    )
    .unwrap();
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".into());
    let launch = Launch::resolve(
        Settings::from_json(document),
        Arguments {
            path: Some("/files".into()),
            select: None,
        },
        &home,
    );
    let locale = Snapshot {
        revision: 1,
        catalog: std::sync::Arc::new(crate::test_catalogs::english()),
    };
    let (_sender, events) = async_channel::unbounded();
    let handle = cx.add_window(|window, cx| Root {
        shell: cx.new(|cx| {
            let shell = Shell::new(launch, locale, events, window, cx);
            window.focus(&shell.focus, cx);
            shell
        }),
    });
    let shell = handle.update(cx, |root, _, _| root.shell.clone()).unwrap();
    shell.update(cx, |shell, _| {
        *shell.panes[0].as_ref().unwrap().nav.writer.borrow_mut() = Writer::default();
    });
    let cx = VisualTestContext::from_window(handle.into(), cx).into_mut();
    cx.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(700.)));
    cx.run_until_parked();
    (shell, cx)
}

// FL-B03: a lane alive beside every other test leaves the process environment, and so temp_dir(), as it found them.
#[test]
fn a_lane_leaves_the_environment_every_parallel_test_reads() {
    let read = || {
        let vars = ["PATH", "EDITOR", "VISUAL", "TMPDIR"].map(std::env::var_os);
        (vars, std::env::temp_dir())
    };
    let before = read();
    let lane = Lane::new();
    assert_eq!(
        read(),
        before,
        "the lane's stand-ins live in the opener's slot"
    );
    let stand = stand_slot().clone().expect("the lane fills the slot");
    assert!(Path::new(&stand.base).starts_with(&lane.root));
    drop(lane);
    assert_eq!(read(), before);
    // Every lane empties the slot before it lets the next one start, so holding the turn shows it empty.
    let _turn = LANE_SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    assert!(stand_slot().is_none(), "and empties it when it ends");
}
