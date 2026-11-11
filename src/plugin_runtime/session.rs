use super::{process::Process, protocol};
use crate::jsondoc::Json;
use crate::ui_logic_plugin_settings::manifest::{self, Manifest};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc, Mutex,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[derive(Clone)]
pub struct Config {
    pub folder: PathBuf,
    pub manifest: Manifest,
    pub hello: Json,
    pub ready_timeout: Duration,
    pub request_timeout: Duration,
    pub idle_timeout: Duration,
}

impl Config {
    pub(super) fn entry(&self) -> Result<PathBuf, String> {
        if !manifest::is_inner_path(&self.manifest.run) {
            return Err("unsafe plugin entry".into());
        }
        let base = self.folder.canonicalize().map_err(|e| e.to_string())?;
        let path = base
            .join(&self.manifest.run)
            .canonicalize()
            .map_err(|e| e.to_string())?;
        let meta = std::fs::metadata(&path).map_err(|e| e.to_string())?;
        if !path.starts_with(&base) || !meta.is_file() || meta.permissions().mode() & 0o111 == 0 {
            return Err("plugin entry must be an executable inside its folder".into());
        }
        Ok(path)
    }
}

#[derive(Debug)]
pub struct Reply {
    pub message: Json,
    pub progress: Vec<Json>,
}

struct Call {
    message: Option<Json>,
    answer: mpsc::SyncSender<Event>,
}

enum Event {
    Progress(Json),
    Finished(Result<Reply, String>),
}

struct Inner {
    requests: mpsc::SyncSender<Call>,
    stop: Arc<AtomicBool>,
    busy: AtomicBool,
    failure: Arc<Mutex<Option<String>>>,
    worker: Mutex<Option<JoinHandle<()>>>,
    timeout: Duration,
}

#[derive(Clone)]
pub struct Runtime(Arc<Inner>);

impl Runtime {
    pub fn new(config: Config) -> Self {
        let (requests, receive) = mpsc::sync_channel(1);
        let stop = Arc::new(AtomicBool::new(false));
        let failure = Arc::new(Mutex::new(None));
        let stopped = stop.clone();
        let failed = failure.clone();
        let timeout = config.ready_timeout + config.request_timeout + Duration::from_secs(1);
        let worker = thread::spawn(move || work(config, receive, stopped, failed));
        Self(Arc::new(Inner {
            requests,
            stop,
            failure,
            busy: AtomicBool::new(false),
            worker: Mutex::new(Some(worker)),
            timeout,
        }))
    }

    pub fn ensure_ready(&self) -> Result<(), String> {
        self.call(None, &mut |_| {}).map(|_| ())
    }

    pub fn request(&self, message: Json) -> Result<Reply, String> {
        self.call(Some(message), &mut |_| {})
    }

    pub fn request_stream(
        &self,
        message: Json,
        progress: &mut dyn FnMut(Json),
    ) -> Result<Reply, String> {
        self.call(Some(message), progress)
    }

    pub fn failure(&self) -> Option<String> {
        self.0
            .failure
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    fn call(&self, message: Option<Json>, progress: &mut dyn FnMut(Json)) -> Result<Reply, String> {
        if self.0.stop.load(Ordering::Acquire) {
            return Err("plugin stopped".into());
        }
        if self.0.busy.swap(true, Ordering::AcqRel) {
            return Err("plugin busy".into());
        }
        let (answer, response) = mpsc::sync_channel(8);
        let result = self
            .0
            .requests
            .try_send(Call { message, answer })
            .map_err(|_| "plugin stopped".to_string())
            .and_then(|_| {
                let deadline = Instant::now() + self.0.timeout;
                loop {
                    match response.recv_timeout(deadline.saturating_duration_since(Instant::now()))
                    {
                        Ok(Event::Progress(message)) => progress(message),
                        Ok(Event::Finished(result)) => return result,
                        Err(error) => {
                            let stopped = self.0.stop.swap(true, Ordering::AcqRel);
                            return Err(match (stopped, error) {
                                (true, _) => "plugin stopped",
                                (false, mpsc::RecvTimeoutError::Disconnected) => {
                                    "plugin worker disconnected"
                                }
                                (false, mpsc::RecvTimeoutError::Timeout) => {
                                    "plugin worker timed out"
                                }
                            }
                            .to_string());
                        }
                    }
                }
            });
        self.0.busy.store(false, Ordering::Release);
        result
    }

    pub fn shutdown(&self) {
        self.0.stop.store(true, Ordering::Release);
        self.0.wake();
        self.0.join();
    }
}

impl Inner {
    fn wake(&self) {
        let (answer, _) = mpsc::sync_channel(1);
        let _ = self.requests.try_send(Call {
            message: None,
            answer,
        });
    }

    fn join(&self) {
        if let Some(worker) = self.worker.lock().unwrap_or_else(|p| p.into_inner()).take() {
            let deadline = Instant::now() + Duration::from_secs(1);
            while !worker.is_finished() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(5));
            }
            if worker.is_finished() {
                let _ = worker.join();
            }
        }
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.wake();
        self.join();
    }
}

fn work(
    config: Config,
    requests: mpsc::Receiver<Call>,
    stop: Arc<AtomicBool>,
    failure: Arc<Mutex<Option<String>>>,
) {
    // No restart budget: the owner marks a plugin failed on its first failure, so only a new request starts it again.
    let mut process: Option<Process> = None;
    let mut last_use = Instant::now();
    while !stop.load(Ordering::Acquire) {
        if process.as_mut().is_some_and(Process::exited) {
            process = None;
            *failure.lock().unwrap_or_else(|p| p.into_inner()) = Some("plugin exited".into());
        }
        if process.is_some() && last_use.elapsed() >= config.idle_timeout {
            process = None;
        }
        let interval = if process.is_some() {
            Duration::from_millis(100)
        } else {
            Duration::from_secs(60)
        };
        let call = match requests.recv_timeout(interval) {
            Ok(call) => call,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(_) => break,
        };
        if stop.load(Ordering::Acquire) {
            break;
        }
        let expected = match call
            .message
            .as_ref()
            .map(|r| protocol::operation(&config.manifest, r))
            .transpose()
        {
            Ok(expected) => expected,
            Err(error) => {
                send(
                    &call.answer,
                    Event::Finished(Err(error)),
                    Instant::now() + Duration::from_secs(1),
                    &stop,
                );
                continue;
            }
        };
        let started = if process.is_none() {
            Process::start(&config, &stop).map(|p| {
                process = Some(p);
            })
        } else {
            Ok(())
        };
        let result = started.and_then(|_| {
            exchange(
                process.as_mut().unwrap(),
                call.message.as_ref(),
                expected,
                &config,
                &stop,
                &call.answer,
            )
        });
        if let Err(error) = &result {
            process = None;
            *failure.lock().unwrap_or_else(|p| p.into_inner()) = Some(error.clone());
        } else {
            *failure.lock().unwrap_or_else(|p| p.into_inner()) = None;
        }
        last_use = Instant::now();
        send(
            &call.answer,
            Event::Finished(result),
            Instant::now() + Duration::from_secs(1),
            &stop,
        );
    }
}

fn exchange(
    process: &mut Process,
    request: Option<&Json>,
    expected: Option<&str>,
    config: &Config,
    stop: &AtomicBool,
    events: &mpsc::SyncSender<Event>,
) -> Result<Reply, String> {
    let Some(request) = request else {
        return Ok(Reply {
            message: Json::Null,
            progress: Vec::new(),
        });
    };
    let deadline = Instant::now() + config.request_timeout;
    process.write(request, deadline, stop)?;
    let mut progress = Vec::new();
    let mut progress_bytes = 0;
    loop {
        let message = process.read(deadline, stop)?;
        if protocol::response(&message, expected.unwrap_or("done"))? {
            return Ok(Reply { message, progress });
        }
        progress_bytes += crate::ui_logic_plugin_settings::wire::compact(&message).len();
        if progress.len() == protocol::MAX_PROGRESS || progress_bytes > protocol::MAX_PROGRESS_BYTES
        {
            return Err("too many plugin progress messages".into());
        }
        if !send(events, Event::Progress(message.clone()), deadline, stop) {
            return Err("plugin progress consumer stopped".into());
        }
        progress.push(message);
    }
}

fn send(
    events: &mpsc::SyncSender<Event>,
    mut event: Event,
    deadline: Instant,
    stop: &AtomicBool,
) -> bool {
    loop {
        match events.try_send(event) {
            Ok(()) => return true,
            Err(mpsc::TrySendError::Disconnected(_)) => return false,
            Err(mpsc::TrySendError::Full(held)) => event = held,
        }
        if stop.load(Ordering::Acquire) || Instant::now() >= deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(test)]
#[path = "session_call_tests.rs"]
mod call_tests;
