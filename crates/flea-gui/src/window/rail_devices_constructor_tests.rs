//! Native list startup through Shell::new, without a synthetic Source or injected network readiness.
use super::*;
use crate::window::{
    locale::Snapshot,
    startup::{Arguments, Launch, Settings},
};

const NATIVE_BLOCKS: &str = r#"{"blockdevices":[
{"name":"nvme0n1","path":"/dev/nvme0n1","type":"disk","children":[{"name":"nvme0n1p1","type":"part","mountpoints":["/"],"fstype":"ext4"}]},
{"name":"sda1","path":"/dev/sda1","type":"part","label":"128GB","mountpoints":[null],"fstype":"crypto_LUKS","rm":true,"size":128000000000}
]}"#;

#[gpui::test]
fn rail_devices_real_constructor_draws_completed_devices_before_native_capture(
    cx: &mut TestAppContext,
) {
    let service = Rc::new(FakeDevices::default());
    let runner = service.clone();
    cx.update(|cx| {
        cx.set_global(crate::state::constructor_fixture::Services {
            backend: |_, cx| {
                let (backend, receiver) = crate::backend::Backend::test_channel();
                cx.on_release(move |_, _| drop(receiver)).detach();
                Ok(backend)
            },
            monitor: None,
        });
        cx.set_global(crate::g2::pl1::source::fixture::Services {
            runner,
            hostname: "minipc\n".into(),
        });
    });
    let state = flea::jsondoc::parse(r#"{"view":"list","density":"compact","places":{"rail":"shown","sidebarWidth":192,"favourites":[{"label":"Docs","path":"/fixture/Docs"}]}}"#).unwrap();
    let home = std::env::var("HOME").unwrap_or_else(|_| "/fixture/home".into());
    let launch = Launch::resolve(
        Settings::from_json(state, true),
        Arguments::default(),
        &home,
    );
    let locale = Snapshot {
        revision: 1,
        catalog: Arc::new(I18n::default()),
    };
    let (_sender, events) = async_channel::unbounded();
    let (shell, cx) =
        cx.add_window_view(|window, cx| Shell::new(launch, locale, events, window, cx));
    cx.run_until_parked();
    assert_eq!(
        service
            .jobs
            .borrow()
            .iter()
            .map(|(argv, _, _)| argv[0].as_str())
            .collect::<Vec<_>>(),
        ["lsblk", "gio"],
        "Shell::new must start the real rail source jobs"
    );
    service.answer("lsblk", 0, NATIVE_BLOCKS);
    service.answer("gio", 0, "");
    frame(cx);
    cx.executor().advance_clock(Duration::from_millis(350));
    frame(cx);
    shell.read_with(cx, |shell, cx| {
        let join = shell
            .rail
            .as_ref()
            .expect("the real constructor mounted rail");
        assert_eq!(
            join.devices
                .read(cx)
                .devices
                .entries
                .iter()
                .map(|row| row.label.as_str())
                .collect::<Vec<_>>(),
            ["minipc", "128GB"],
            "the device probe completed before the capture"
        );
        let producer = join.places.read(cx);
        assert!(producer.shown_entries().iter().any(|label| label == "Docs"));
        assert_eq!(
            producer.provider.drawn.last().unwrap().heading.as_ref(),
            "DEVICES",
            "completed devices must not wait for an absent network source"
        );
        let laid = producer.laid.borrow();
        assert!(laid.painted);
        assert!(laid.headers.iter().any(|header| header.text == "DEVICES"));
        let rows = laid
            .rows
            .iter()
            .filter(|row| row.label == "minipc" || row.label == "128GB")
            .map(|row| (row.label.as_str(), row.glyph))
            .collect::<Vec<_>>();
        assert_eq!(
            rows,
            [("minipc", "drive"), ("128GB", "lock")],
            "the actual rail must paint both native-scene rows"
        );
    });
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
