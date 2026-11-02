//! FIX-LIST 1101: a cut row's scissors at rest in List.
use super::*;

/// EY-376: at rest in List, a short cut name's scissors sit one gap after the name, not in a strip beside Size.
#[test]
fn fixlist_ey376_cut_scissors_sit_right_after_the_name_at_rest() {
    let mut cx = bench::real_cx();
    let handle = open(&mut cx);
    frame(&mut cx, handle);
    clipboard(&mut cx, handle, "cut");
    let painted = frame(&mut cx, handle);
    let marks: Vec<_> = painted.iter().filter(|p| p.mark == CUT_MARK).collect();
    assert_eq!(marks.len(), ROWS, "every cut row paints its scissors");
    handle
        .update(&mut cx, |shell, _, _| {
            let list = shell.panes[0].as_ref().unwrap().list.borrow();
            let (gap, clip) = (list.drawn.metrics.gap, list.drawn.metrics.clip);
            let names = list.drawn.bounds.left() + px(list.drawn.layout.name.x);
            let column = px(list.drawn.layout.name.width);
            let mut short = 0;
            for mark in marks {
                let centre = mark.bounds.center().y;
                let name = painted
                    .iter()
                    .find(|p| {
                        p.run != 0
                            && p.bounds.left() == names
                            && p.bounds.top() <= centre
                            && p.bounds.bottom() >= centre
                    })
                    .expect("the cut row paints its name");
                if name.bounds.size.width + px(gap + clip) > column {
                    continue;
                }
                short += 1;
                assert_eq!(
                    mark.bounds.left(),
                    name.bounds.right() + px(gap),
                    "the scissors follow the name by one gap"
                );
            }
            assert!(
                short >= 2,
                "the fixture needs short names to prove the slot"
            );
        })
        .unwrap();
}
