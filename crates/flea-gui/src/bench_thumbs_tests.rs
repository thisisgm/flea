use super::*;
use gpui::{FrameTerminal, PresentationTime};

fn state() -> State {
    State {
        jobs: vec![Job {
            kind: "thumbnail",
            path: "/fixture/a.jpg".into(),
            start: Duration::from_secs(1),
            worker_ms: Some(25.),
            done: false,
        }],
        draws: [(
            7,
            vec![Paint {
                job: 0,
                width: 256,
                height: 170,
                bounds: [0., 0., 256., 170.],
            }],
        )]
        .into(),
        ..Default::default()
    }
}

fn terminal(id: u64, clock: Option<u32>) -> FrameFeedbackEvent {
    FrameFeedbackEvent::Terminal(FrameTerminal {
        frame_id: id,
        outcome: FrameOutcome::Presented(PresentationTime {
            timestamp: Some(Duration::from_secs(2)),
            clock_id: clock,
        }),
    })
}

#[test]
fn bench_thumbs_reader_requires_exact_successful_submission_and_monotonic_presentation() {
    let mut state = state();
    state.receive(terminal(9, Some(1)));
    assert!(!state.jobs[0].done, "a paint is not a submission");
    state.receive(FrameFeedbackEvent::Submitted {
        draw_id: 8,
        frame_id: 9,
    });
    state.receive(terminal(9, Some(1)));
    assert!(
        !state.jobs[0].done,
        "another draw cannot complete this paint"
    );
    state.receive(FrameFeedbackEvent::Submitted {
        draw_id: 7,
        frame_id: 10,
    });
    assert!(!state.jobs[0].done, "submission is not presentation");
    state.receive(terminal(10, None));
    assert!(!state.jobs[0].done, "missing native clock is unavailable");
    state.receive(FrameFeedbackEvent::Submitted {
        draw_id: 7,
        frame_id: 11,
    });
    state.receive(terminal(11, Some(2)));
    assert!(!state.jobs[0].done, "a different clock cannot time the job");
    state.receive(FrameFeedbackEvent::Submitted {
        draw_id: 7,
        frame_id: 12,
    });
    state.receive(terminal(12, Some(1)));
    assert!(state.jobs[0].done);
}

#[test]
fn bench_thumbs_reader_discarded_frame_cannot_complete_decoded_output() {
    let mut state = state();
    state.receive(FrameFeedbackEvent::Submitted {
        draw_id: 7,
        frame_id: 9,
    });
    state.receive(FrameFeedbackEvent::Terminal(FrameTerminal {
        frame_id: 9,
        outcome: FrameOutcome::Discarded,
    }));
    assert!(!state.jobs[0].done);
    assert!(state.pending.is_empty());
}
