use std::ops::Deref;
use std::sync::{Arc, Mutex, OnceLock};
use std::sync::mpsc::Sender;

struct Inner<T> {
    value: OnceLock<T>,
    init: Mutex<Option<Box<dyn FnOnce() -> T + Send>>>,
}

// Clones share one initialization, including tables used by later worker threads.
pub struct Lazy<T>(Arc<Inner<T>>);

impl<T> Lazy<T> {
    pub fn new(init: impl FnOnce() -> T + Send + 'static) -> Self {
        Self(Arc::new(Inner { value: OnceLock::new(), init: Mutex::new(Some(Box::new(init))) }))
    }

    pub fn get(&self) -> Option<&T> {
        self.0.value.get()
    }
}

impl<T> From<T> for Lazy<T> {
    fn from(value: T) -> Self {
        Self(Arc::new(Inner { value: OnceLock::from(value), init: Mutex::new(None) }))
    }
}

impl<T> Clone for Lazy<T> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<T> Deref for Lazy<T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.0.value.get_or_init(|| {
            let init = self.0.init.lock().unwrap().take().expect("lazy initializer already consumed");
            init()
        })
    }
}

// Cloning a reply sender starts its forwarder before any worker can send a reply.
pub struct DeferredSender<T>(Lazy<Sender<T>>);

impl<T> DeferredSender<T> {
    pub fn new(init: impl FnOnce() -> Sender<T> + Send + 'static) -> Self {
        Self(Lazy::new(init))
    }

    pub fn clone(&self) -> Sender<T> {
        Sender::clone(self)
    }
}

impl<T> From<Sender<T>> for DeferredSender<T> {
    fn from(sender: Sender<T>) -> Self {
        Self(sender.into())
    }
}

impl<T> Deref for DeferredSender<T> {
    type Target = Sender<T>;

    fn deref(&self) -> &Sender<T> {
        &self.0
    }
}

// Mutable service state has one owner and initializes only when a request uses it.
pub struct Local<T> {
    value: OnceLock<T>,
    init: Option<fn() -> T>,
}

impl<T> Local<T> {
    pub fn new(init: fn() -> T) -> Self {
        Self { value: OnceLock::new(), init: Some(init) }
    }

    pub fn get_mut(&mut self) -> Option<&mut T> {
        self.value.get_mut()
    }

    // A reader that must not start the service sees it only once a request has.
    pub fn get(&self) -> Option<&T> {
        self.value.get()
    }
}

impl<T> From<T> for Local<T> {
    fn from(value: T) -> Self {
        Self { value: OnceLock::from(value), init: None }
    }
}

impl<T> Deref for Local<T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.value.get_or_init(|| (self.init.expect("missing service initializer"))())
    }
}

impl<T> std::ops::DerefMut for Local<T> {
    fn deref_mut(&mut self) -> &mut T {
        let _ = &**self;
        self.value.get_mut().unwrap()
    }
}
