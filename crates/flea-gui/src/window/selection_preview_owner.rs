//! Construction and timing for the retained preview owner.
use super::*;

impl SelectionPreview {
    pub(crate) fn new(
        port: Port,
        catalog: Arc<flea::i18n::I18n>,
        face: crate::list::Typeface,
        cx: &mut Context<Self>,
    ) -> Self {
        let view = Self {
            port,
            outbox: Rc::default(),
            pending: None,
            manual_subject: None,
            drawn: None,
            visible: false,
            mode: crate::window::startup::View::List,
            class: None,
            rect: Rc::default(),
            frame_rect: Rc::default(),
            play_rect: Rc::default(),
            width: 0.,
            height: 0.,
            catalog,
            face,
            corner: 0.,
            full_facts: false,
            automatic: true,
            media_backend: None,
            caches: Default::default(),
            loading_mark: Rc::default(),
            epoch: cx.background_executor().now(),
            timer: None,
            gif: None,
            retired: Vec::new(),
            focus: cx.focus_handle(),
            frames: Rc::default(),
            frame_feedback: None,
            previewer: (None, 0),
        };
        view.port.0.borrow_mut().clock = Some(observation::Clock {
            executor: cx.background_executor().clone(),
            origin: view.epoch,
        });
        view
    }
    pub(crate) fn now(&self, cx: &gpui::App) -> u64 {
        cx.background_executor()
            .now()
            .saturating_duration_since(self.epoch)
            .as_millis() as u64
    }
    pub fn mounted_kind(&self) -> Option<(&Kind, &Mounted)> {
        self.drawn.as_ref().map(|f| (&f.kind, &f.mounted))
    }

    /// Automatic loading off leaves the column empty until the selection is explicitly loaded.
    pub(crate) fn manual_empty(&self, frame: &Frame) -> bool {
        frame.manual && !self.automatic
    }
}
