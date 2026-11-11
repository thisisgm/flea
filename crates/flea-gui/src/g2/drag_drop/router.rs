//! The fork's native drag callbacks hold one handler each, so one router installs them once per process and asks every registered client in turn; the first that claims an event answers it.
use gpui::{App, NativeDragAction, NativeDragActions, NativeDragEvent};
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::sync::Arc;

/// A drag entering a window, before its bytes are read.
pub struct Offered<'a> {
    pub window: u64,
    /// The pointer at Enter, window-relative, as Native.position.
    pub position: (f32, f32),
    pub types: &'a [String],
}

/// A drag entering (dropped false) or dropped on a window, with the representations already read.
pub struct Native<'a> {
    pub window: u64,
    pub position: (f32, f32),
    pub dropped: bool,
    pub types: &'a [String],
    data: &'a dyn Fn(&str) -> Option<Arc<[u8]>>,
    pub action: Option<NativeDragAction>,
    pub ctrl: bool,
    pub shift: bool,
}

impl<'a> Native<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        window: u64,
        position: (f32, f32),
        dropped: bool,
        types: &'a [String],
        data: &'a dyn Fn(&str) -> Option<Arc<[u8]>>,
        action: Option<NativeDragAction>,
        ctrl: bool,
        shift: bool,
    ) -> Self {
        Self {
            window,
            position,
            dropped,
            types,
            data,
            action,
            ctrl,
            shift,
        }
    }

    pub fn get(&self, mime: &str) -> Option<Arc<[u8]>> {
        (self.data)(mime)
    }
}

/// A destination or source policy; None leaves the event to the next client.
pub trait Client {
    fn offer(&self, offered: &Offered<'_>) -> Option<NativeDragActions>;
    fn land(&self, native: &Native<'_>) -> Option<bool>;
    fn source(&self, _event: &NativeDragEvent) {}
}

fn refused() -> NativeDragActions {
    NativeDragActions {
        copy: false,
        move_: false,
        preferred: NativeDragAction::Move,
    }
}

/// The registered clients, held weakly so a closed window's client leaves with its registration.
#[derive(Default)]
pub struct Router {
    clients: RefCell<Vec<(u64, Weak<dyn Client>)>>,
    next: Cell<u64>,
}

/// Keeps a client registered until dropped.
pub struct Registration {
    router: Weak<Router>,
    id: u64,
    // Holds the client alive for exactly as long as the registration.
    _client: Rc<dyn Client>,
}

impl Drop for Registration {
    fn drop(&mut self) {
        if let Some(router) = self.router.upgrade() {
            router.clients.borrow_mut().retain(|(id, _)| *id != self.id);
        }
    }
}

impl Router {
    fn live(&self) -> Vec<Rc<dyn Client>> {
        self.clients
            .borrow()
            .iter()
            .filter_map(|(_, client)| client.upgrade())
            .collect()
    }

    /// Registers a client on a router that is not shared; tests use it directly.
    pub fn register(self: &Rc<Self>, client: Rc<dyn Client>) -> Registration {
        let id = self.next.get() + 1;
        self.next.set(id);
        self.clients.borrow_mut().push((id, Rc::downgrade(&client)));
        Registration {
            router: Rc::downgrade(self),
            id,
            _client: client,
        }
    }

    pub fn offer(&self, offered: &Offered<'_>) -> NativeDragActions {
        self.live()
            .iter()
            .find_map(|client| client.offer(offered))
            .unwrap_or_else(refused)
    }

    pub fn land(&self, native: &Native<'_>) -> bool {
        self.live()
            .iter()
            .find_map(|client| client.land(native))
            .unwrap_or(false)
    }

    pub fn source(&self, event: &NativeDragEvent) {
        for client in self.live() {
            client.source(event);
        }
    }
}

thread_local! {
    static SHARED: RefCell<Option<Rc<Router>>> = const { RefCell::new(None) };
}

/// The process router, installing the fork's three callbacks the first time it is asked for.
pub fn shared(cx: &App) -> Rc<Router> {
    if let Some(router) = SHARED.with(|shared| shared.borrow().clone()) {
        return router;
    }
    let router = Rc::new(Router::default());
    SHARED.with(|shared| *shared.borrow_mut() = Some(router.clone()));
    let offers = Rc::downgrade(&router);
    cx.on_native_drag_offer(move |offer| {
        offers.upgrade().map_or_else(refused, |router| {
            router.offer(&Offered {
                window: offer.window.window_id().as_u64(),
                position: (f32::from(offer.position.x), f32::from(offer.position.y)),
                types: &offer.offered_types,
            })
        })
    });
    let drops = Rc::downgrade(&router);
    cx.on_native_drop(move |event| {
        let Some(router) = drops.upgrade() else {
            return false;
        };
        let data = |mime: &str| event.data.get(mime);
        router.land(&Native::new(
            event.window.window_id().as_u64(),
            (f32::from(event.position.x), f32::from(event.position.y)),
            event.dropped,
            &event.offered_types,
            &data,
            event.action,
            event.modifiers.control,
            event.modifiers.shift,
        ))
    });
    let sources = Rc::downgrade(&router);
    cx.on_native_drag(move |event| {
        if let Some(router) = sources.upgrade() {
            router.source(&event);
        }
    });
    router
}
