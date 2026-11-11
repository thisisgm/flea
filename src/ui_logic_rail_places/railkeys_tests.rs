// Ported from tests/js/railkeys.js; the release and its sentence are the device unit's, so the stub records the row asked.
use super::railkeys::*;

struct Rail {
    rows: usize,
    releasable: Vec<usize>,
    answer: Activation,
    focus: Focus,
    activated: Vec<usize>,
    menus: usize,
    raised: Vec<usize>,
    released: Vec<usize>,
    trash: Option<usize>,
    escaped: bool,
    added: usize,
    renamed: Vec<usize>,
}

fn rail(rows: usize) -> Rail {
    Rail {
        rows,
        releasable: Vec::new(),
        answer: Activation::Waiting,
        focus: Focus::Rail,
        activated: Vec::new(),
        menus: 0,
        raised: Vec::new(),
        released: Vec::new(),
        trash: None,
        escaped: false,
        added: 0,
        renamed: Vec::new(),
    }
}

impl RailHost for Rail {
    fn len(&self) -> usize {
        self.rows
    }
    fn releasable(&self, index: usize) -> bool {
        self.releasable.contains(&index)
    }
    fn activate(&mut self, index: usize) -> Activation {
        self.activated.push(index);
        self.answer
    }
    fn status_escape(&mut self) -> bool {
        self.escaped
    }
    fn focus(&self) -> Focus {
        self.focus
    }
    fn set_focus(&mut self, focus: Focus) {
        self.focus = focus;
    }
    fn add_network(&mut self) {
        self.added += 1;
    }
    fn start_rename(&mut self, index: usize) {
        self.renamed.push(index);
    }
    fn is_trash(&self, index: usize) -> bool {
        self.trash == Some(index)
    }
    fn open_cursor_menu(&mut self) {
        self.menus += 1;
    }
    fn raise_menu(&mut self, index: usize) {
        self.raised.push(index);
    }
    fn release(&mut self, index: usize) {
        self.released.push(index);
    }
}

fn at(index: usize) -> RailCursor {
    RailCursor {
        index,
        ..RailCursor::default()
    }
}

#[test]
fn g_and_capital_g_reach_the_ends_and_a_key_shows_the_highlight() {
    let mut host = rail(4);
    let mut cursor = at(2);
    cursor.act("cursorFirst", RawKey::Other, false, &mut host);
    assert_eq!(cursor.index, 0, "g takes the rail to its first row");
    cursor.act("cursorLast", RawKey::Other, false, &mut host);
    assert_eq!(cursor.index, 3, "G takes the rail to its last row");
    cursor.act("cursorDown", RawKey::Other, false, &mut host);
    assert_eq!(cursor.index, 3, "down stops on the last row");
    cursor.act("cursorUp", RawKey::Other, false, &mut host);
    assert_eq!(cursor.index, 2);
    let mut empty = at(0);
    empty.act("cursorLast", RawKey::Other, false, &mut rail(0));
    empty.act("cursorDown", RawKey::Other, false, &mut rail(0));
    assert_eq!(empty.index, 0, "an empty rail has no last row to reach");
    let mut pressed = RailCursor {
        pointer_focus: true,
        ..at(0)
    };
    pressed.act("cursorDown", RawKey::Other, false, &mut rail(3));
    assert!(
        !pressed.pointer_focus,
        "a rail key after a pointer press shows the highlight again"
    );
}

#[test]
fn an_open_hands_focus_to_the_folder_it_opened() {
    let mut opening = rail(2);
    opening.answer = Activation::Opened;
    let mut cursor = at(1);
    cursor.act("open", RawKey::Enter, false, &mut opening);
    assert_eq!(
        opening.activated,
        [1],
        "Enter or l on a rail row opens that row"
    );
    assert_eq!(
        (opening.focus, cursor.focus_on_open),
        (Focus::List, false),
        "and moves focus into the folder it opened"
    );
    let mut mounting = rail(2);
    let mut cursor = at(1);
    cursor.act("open", RawKey::Enter, false, &mut mounting);
    assert_eq!(
        (
            mounting.activated.as_slice(),
            mounting.focus,
            cursor.focus_on_open
        ),
        (&[1][..], Focus::Rail, true),
        "an unmounted row arms the claim"
    );
    cursor.act("cursorUp", RawKey::Other, false, &mut mounting);
    assert!(
        !cursor.focus_on_open,
        "another rail key withdraws that claim"
    );
    let mut failing = rail(2);
    failing.answer = Activation::Failed;
    let mut cursor = at(1);
    cursor.act("open", RawKey::Enter, false, &mut failing);
    assert_eq!(
        (cursor.focus_on_open, failing.focus),
        (false, Focus::Rail),
        "a mount that fails at once spends the claim"
    );
    let mut ready = rail(2);
    let mut cursor = at(1);
    cursor.act("open", RawKey::Enter, false, &mut ready);
    let mut focus = ready.focus;
    assert!(
        open_from(Some(&mut cursor), Some(&mut focus)),
        "the share's open lands"
    );
    assert_eq!(
        (focus, cursor.focus_on_open),
        (Focus::List, false),
        "and focus follows it"
    );
    let mut bare = rail(0);
    at(0).act("open", RawKey::Enter, false, &mut bare);
    assert!(
        bare.activated.is_empty() && bare.focus == Focus::Rail,
        "an empty rail opens nothing and keeps focus"
    );
}

#[test]
fn landing_and_messages_spend_the_claim() {
    let mut claimed = RailCursor {
        focus_on_open: true,
        ..at(0)
    };
    let mut focus = Focus::Rail;
    claimed.landed(&mut focus);
    assert_eq!(
        (focus, claimed.focus_on_open),
        (Focus::List, false),
        "the open that lands takes focus off the rail"
    );
    let mut stale = RailCursor {
        focus_on_open: true,
        ..at(0)
    };
    let mut moved = Focus::Elsewhere;
    stale.landed(&mut moved);
    assert_eq!(
        (moved, stale.focus_on_open),
        (Focus::Elsewhere, false),
        "a landing after the user left moves nothing"
    );
    let mut unclaimed = Focus::Rail;
    at(0).landed(&mut unclaimed);
    assert_eq!(
        unclaimed,
        Focus::Rail,
        "an open with no claim leaves focus where it is"
    );
    let mut orphan = RailCursor {
        focus_on_open: true,
        ..at(0)
    };
    assert!(
        !open_from(Some(&mut orphan), None),
        "an open with no pane opens nothing"
    );
    assert!(!orphan.focus_on_open, "and spends the claim");
    let mut hidden = Focus::Rail;
    assert!(
        open_from(None, Some(&mut hidden)),
        "an open with no rail opens all the same"
    );
    assert_eq!(hidden, Focus::Rail, "and moves no focus");
    assert!(
        !open_from(None, None),
        "neither pane nor rail opens nothing"
    );
    let mut failing = RailCursor {
        focus_on_open: true,
        ..at(0)
    };
    failing.messaged(false);
    assert!(failing.focus_on_open, "an ordinary message keeps the claim");
    failing.messaged(true);
    assert!(!failing.focus_on_open, "a mount error spends the claim");
    let mut after = Focus::Rail;
    assert!(
        open_from(Some(&mut failing), Some(&mut after)) && after == Focus::Rail,
        "a later open leaves focus on the rail"
    );
}

#[test]
fn menu_eject_and_the_mark_go_to_the_device_side() {
    let mut host = rail(2);
    host.trash = Some(0);
    at(0).act("menu", RawKey::Other, false, &mut host);
    assert_eq!(
        (host.menus, host.raised.len()),
        (1, 0),
        "Trash keeps its own cursor menu"
    );
    at(1).act("menu", RawKey::Other, false, &mut host);
    assert_eq!(
        host.raised,
        [1],
        "any other row raises its menu or the sentence why not"
    );
    let mut empty = rail(0);
    at(0).act("menu", RawKey::Other, false, &mut empty);
    assert!(
        empty.raised.is_empty() && empty.menus == 0,
        "an empty rail answers nothing at all"
    );
    at(1).act("eject", RawKey::Other, false, &mut host);
    assert_eq!(host.released, [1], "ctrl e releases the cursor row");
    let mut marks = rail(2);
    marks.releasable = vec![1];
    let mut cursor = at(0);
    cursor.act("open", RawKey::Right, false, &mut marks);
    assert!(
        !cursor.mark_focused && marks.activated == [0],
        "Right on a row with no mark opens it"
    );
    let mut cursor = at(1);
    cursor.act("open", RawKey::Right, false, &mut marks);
    assert!(
        cursor.mark_focused && marks.activated == [0],
        "Right on a releasable row moves onto its mark"
    );
    cursor.act("open", RawKey::Enter, false, &mut marks);
    assert!(
        !cursor.mark_focused && marks.released == [1] && marks.activated == [0],
        "Enter on the mark releases"
    );
    cursor.act("open", RawKey::Right, false, &mut marks);
    cursor.act("escape", RawKey::Escape, false, &mut marks);
    assert!(
        !cursor.mark_focused && marks.focus == Focus::Rail,
        "Escape leaves the mark and stays on the rail"
    );
    cursor.act("open", RawKey::Right, true, &mut marks);
    assert!(
        !cursor.mark_focused,
        "a modified Right never takes the mark"
    );
}

#[test]
fn escape_rename_and_add_network() {
    let mut host = rail(3);
    at(0).act("escape", RawKey::Escape, false, &mut host);
    assert_eq!(host.focus, Focus::List, "Escape hands focus to the list");
    host.focus = Focus::Rail;
    host.escaped = true;
    at(0).act("escape", RawKey::Escape, false, &mut host);
    assert_eq!(
        host.focus,
        Focus::Rail,
        "the status bar's Escape goes first"
    );
    at(2).act("rename", RawKey::Other, false, &mut host);
    at(0).act("addNetwork", RawKey::Other, false, &mut host);
    assert_eq!((host.renamed.as_slice(), host.added), (&[2][..], 1));
}

#[test]
fn the_highlight_draws_only_while_the_keyboard_holds_the_rail() {
    assert!(
        !cursor_drawn(false, false, false),
        "an unfocused rail draws no cursor"
    );
    assert!(
        cursor_drawn(true, false, false),
        "a focused rail draws its cursor"
    );
    assert!(
        !cursor_drawn(false, true, false),
        "a right click's open menu draws no highlight"
    );
    assert!(
        !cursor_drawn(true, true, false),
        "a keyboard-opened menu draws no highlight"
    );
    assert!(
        !cursor_drawn(true, false, true),
        "a pointer press draws no highlight"
    );
}
