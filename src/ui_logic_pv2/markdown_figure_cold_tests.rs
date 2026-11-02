//! A queued real figure takes precedence over throwaway work for other document kinds.
use super::*;
use crate::ui_logic_pv2::markdown::markdown_figure_store::StoreEvent;

fn warmed() -> Service {
    let mut service = Service::default();
    service.warm(
        &[
            (FigureKind::Mermaid, "flowchart TD\n    A --> B".into()),
            (FigureKind::Math, "x^2".into()),
        ],
        &|_| Some(theme()),
        0,
    );
    service.take_commands();
    service
}

#[test]
fn sol_md_cold_pending_figure_starts_without_throwaway_renders() {
    for known_first in [false, true] {
        let mut service = warmed();
        let id = service.ask(FigureKind::Mermaid, "graph TD; X-->Y", true, &theme(), 1);
        service.take_commands();
        let known = StoreEvent::Known {
            id: service.warm_query,
            all: false,
        };
        if known_first {
            service.store_event(known.clone(), 2);
            assert!(service.take_commands().is_empty());
        }
        service.store_event(
            StoreEvent::Answered {
                id,
                svg: String::new(),
            },
            3,
        );
        let commands = service.take_commands();
        assert_eq!(commands, vec![Command::HelperSpawn(Vec::new())]);
        if !known_first {
            service.store_event(known, 4);
            assert!(service.take_commands().is_empty());
        }
        service.helper_started(5);
        assert_eq!(
            service.take_commands(),
            vec![Command::HelperWrite(request_line(
                id,
                FigureKind::Mermaid,
                "graph TD; X-->Y",
                true,
                &theme(),
            ))]
        );
        assert_eq!(service.sends, 1);
        assert_eq!(service.waiting_count(), 1);
    }
}

#[test]
fn sol_md_cold_idle_prepare_still_warms_document_kinds() {
    let mut service = warmed();
    service.store_event(
        StoreEvent::Known {
            id: service.warm_query,
            all: false,
        },
        1,
    );
    assert_eq!(
        service.take_commands(),
        vec![Command::HelperSpawn(vec!["mermaid", "math"])]
    );
    assert_eq!(service.sends, 0);
}
