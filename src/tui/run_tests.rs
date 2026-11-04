// Losing the TUI's one-arena call warns nowhere, so its place before the first thread is pinned here.
#[test]
fn the_tui_keeps_one_malloc_arena_from_before_its_first_thread() {
    let source: String = include_str!("mod.rs").lines().filter(|line| !line.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
    let arena = source.find("crate::heap::one_arena();").expect("tui::run keeps one arena");
    let wire = source.find("wire::Wire::start(").expect("tui::run starts the backend wire");
    let gvfs = source.find("warm::start(").expect("tui::run starts the gvfs head start");
    assert!(arena < wire, "the arena is fixed before the wire's reader thread starts");
    assert!(arena < gvfs, "the arena is fixed before the gvfs head start can spawn its helper");
}

// The keyboard group lands before the first draw, each pass publishes what arrived before it paints, and a resume asks again.
#[test]
fn ey_tui_keycaps_run_wires_the_adapter_before_the_first_draw_and_on_resume() {
    let source: String = include_str!("mod.rs").lines().filter(|line| !line.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
    let at = |needle: &str| source.find(needle).unwrap_or_else(|| panic!("tui::run lacks {needle}"));
    let settle = at("keycaps.settle(&mut model, &map, keycaps::FIRST_WAIT);");
    let apply = at("keycaps.apply(&mut model, &map);");
    let resume = at("keycaps.resume();");
    assert!(at("keycaps::Keycaps::start()") < settle && at("model.hints = keymap::Hints::new(") < settle);
    assert!(settle < at("while !model.quit") && at("while !model.quit") < apply && apply < at("render::draw("));
    assert!(at("if terminal.take_resumed() {") < resume && resume < at("wire.drain_wake();"));
}
