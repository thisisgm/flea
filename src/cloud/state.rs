// Queue truth is mount-scoped. Global accounting errors are deliberately not file verdicts.
use crate::json::escape;
use crate::jsondoc::Json;

pub(super) struct Snapshot {
    pub state: &'static str,
    pub path: String,
    pub mount: String,
    pub reason: String,
    pub queued: u64,
    pub uploading: u64,
    pub retrying: u64,
    pub errors: u64,
    pub progress_known: bool,
    pub bytes: u64,
    pub total: u64,
    pub speed: f64,
    pub name: String,
}

impl Snapshot {
    pub fn new(state: &'static str) -> Self {
        Self { state, path: String::new(), mount: String::new(), reason: String::new(),
            queued: 0, uploading: 0, retrying: 0, errors: 0, progress_known: false,
            bytes: 0, total: 0, speed: 0.0, name: String::new() }
    }
    pub fn unavailable(reason: &str) -> Self {
        let mut result = Self::new("unavailable"); result.reason = reason.into(); result
    }
    pub fn json(&self) -> String {
        format!(r#"{{"state":"{}","path":"{}","mount":"{}","reason":"{}","queued":{},"uploading":{},"retrying":{},"errors":{},"progressKnown":{},"bytes":{},"total":{},"speed":{},"name":"{}"}}"#,
            self.state, escape(&self.path), escape(&self.mount), escape(&self.reason), self.queued,
            self.uploading, self.retrying, self.errors, self.progress_known, self.bytes, self.total,
            self.speed, escape(&self.name))
    }
}

fn integer(value: &Json, key: &str) -> Result<u64, &'static str> {
    value.get(key).and_then(Json::as_f64)
        .filter(|n| n.is_finite() && *n >= 0.0 && *n <= 9_007_199_254_740_991.0 && n.fract() == 0.0)
        .map(|n| n as u64).ok_or("Incomplete or invalid upload counters")
}

pub(super) fn summarize(stats: &Json, queue: &Json, transfers: Option<&Json>) -> Result<Snapshot, &'static str> {
    let cache = stats.get("diskCache").ok_or("Upload monitoring requires the rclone VFS disk cache")?;
    let queued = integer(cache, "uploadsQueued")?;
    let uploading = integer(cache, "uploadsInProgress")?;
    let errors = integer(cache, "erroredFiles")?;
    let full = cache.get("outOfSpace").and_then(Json::as_bool).ok_or("Missing cache health status")?;
    let items = queue.get("queue").and_then(Json::as_array).ok_or("Upload queue is unavailable")?;
    let mut result = Snapshot::new("idle");
    result.errors = errors;
    let mut active = Vec::new();
    for item in items {
        let name = item.get("name").and_then(Json::as_str).ok_or("Invalid upload queue entry")?;
        let size = integer(item, "size")?;
        let tries = integer(item, "tries")?;
        let running = item.get("uploading").and_then(Json::as_bool).ok_or("Missing upload state")?;
        if running { result.uploading += 1; active.push((name, size)); }
        else { result.queued += 1; }
        if tries > u64::from(running) { result.retrying += 1; }
    }
    result.state = if full || errors > 0 { "error" }
        else if result.retrying > 0 { "retrying" }
        else if result.uploading > 0 { "uploading" }
        else if result.queued > 0 { "pending" }
        // Snapshots are sequential, not atomic. A disagreement must not look idle.
        else if queued > 0 || uploading > 0 { "checking" }
        else { "idle" };
    if full { result.reason = "rclone cache is out of space".into(); }
    else if errors > 0 { result.reason = "rclone reports cache errors".into(); }
    if let Some((name, _)) = active.first() { result.name = name.chars().take(256).collect(); }
    if let Some(samples) = transfers.and_then(|t| t.get("transferring")).and_then(Json::as_array) {
        if let Some(source) = stats.get("fs").and_then(Json::as_str) {
            progress(&mut result, &active, samples, source);
        }
    }
    Ok(result)
}

fn progress(result: &mut Snapshot, active: &[(&str, u64)], samples: &[Json], source: &str) {
    if active.is_empty() { return; }
    let (mut completed, mut expected, mut rate) = (0u64, 0u64, 0.0);
    for &(name, size) in active {
        let found: Vec<_> = samples.iter().filter(|s| s.get("name").and_then(Json::as_str) == Some(name)).collect();
        // Two matching accounting records (e.g. reading and writing the same name) are ambiguous.
        if found.len() != 1 { return; }
        let sample = found[0];
        let (Ok(bytes), Ok(total)) = (integer(sample, "bytes"), integer(sample, "size")) else { return; };
        if total != size || bytes > total || sample.get("dstFs").and_then(Json::as_str) != Some(source) { return; }
        completed = completed.saturating_add(bytes);
        expected = expected.saturating_add(total);
        let speed = sample.get("speedAvg").and_then(Json::as_f64).unwrap_or(0.0);
        if !speed.is_finite() || speed < 0.0 { return; }
        rate += speed;
        if !rate.is_finite() { return; }
    }
    result.bytes = completed;
    result.total = expected;
    result.speed = rate;
    result.progress_known = expected > 0;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jsondoc::parse;

    fn stats(queued: u64, uploading: u64, errors: u64) -> Json {
        parse(&format!(r#"{{"fs":"remote:","diskCache":{{"uploadsQueued":{},"uploadsInProgress":{},"erroredFiles":{},"outOfSpace":false}}}}"#, queued, uploading, errors)).unwrap()
    }
    fn queue(running: bool, tries: u64) -> Json {
        parse(&format!(r#"{{"queue":[{{"name":"photo.bin","size":1000,"uploading":{},"tries":{}}}]}}"#, running, tries)).unwrap()
    }
    #[test]
    fn queue_and_cache_states_are_distinct_and_never_claim_synced() {
        let empty = parse(r#"{"queue":[]}"#).unwrap();
        assert_eq!(summarize(&stats(0, 0, 0), &empty, None).unwrap().state, "idle");
        assert_eq!(summarize(&stats(1, 0, 0), &queue(false, 0), None).unwrap().state, "pending");
        assert_eq!(summarize(&stats(0, 1, 0), &queue(true, 1), None).unwrap().state, "uploading");
        assert_eq!(summarize(&stats(1, 0, 0), &queue(false, 1), None).unwrap().state, "retrying");
        assert_eq!(summarize(&stats(0, 1, 0), &queue(true, 2), None).unwrap().state, "retrying");
        assert_eq!(summarize(&stats(0, 0, 1), &empty, None).unwrap().state, "error");
        assert_eq!(summarize(&stats(1, 0, 0), &empty, None).unwrap().state, "checking");
        assert!(summarize(&parse("{}").unwrap(), &empty, None).is_err());
        assert!(summarize(&stats(0, 0, 0), &parse("{}").unwrap(), None).is_err());
    }
    #[test]
    fn progress_needs_unique_matching_queue_and_accounting_records() {
        let sample = r#"{"name":"photo.bin","dstFs":"remote:","size":1000,"bytes":250,"speedAvg":20}"#;
        let transfers = parse(&format!(r#"{{"transferring":[{}]}}"#, sample)).unwrap();
        let got = summarize(&stats(0, 1, 0), &queue(true, 1), Some(&transfers)).unwrap();
        assert!(got.progress_known); assert_eq!((got.bytes, got.total), (250, 1000));
        let duplicate = parse(&format!(r#"{{"transferring":[{},{}]}}"#, sample, sample)).unwrap();
        assert!(!summarize(&stats(0, 1, 0), &queue(true, 1), Some(&duplicate)).unwrap().progress_known);
        let download = parse(&format!(r#"{{"transferring":[{}]}}"#, sample.replace("remote:", "local:"))).unwrap();
        assert!(!summarize(&stats(0, 1, 0), &queue(true, 1), Some(&download)).unwrap().progress_known);
        let unrelated = parse(r#"{"errors":500,"transferring":[{"name":"other.bin","bytes":5,"size":1000}]}"#).unwrap();
        let got = summarize(&stats(0, 1, 0), &queue(true, 1), Some(&unrelated)).unwrap();
        assert!(!got.progress_known); assert_eq!(got.errors, 0);
    }
    #[test]
    fn malformed_counters_are_not_an_empty_queue() {
        for bad in ["null", "-1", "0.5", "9007199254740992", "\"0\""] {
            let s = parse(&format!(r#"{{"fs":"remote:","diskCache":{{"uploadsQueued":{},"uploadsInProgress":0,"erroredFiles":0,"outOfSpace":false}}}}"#, bad)).unwrap();
            assert!(summarize(&s, &parse(r#"{"queue":[]}"#).unwrap(), None).is_err());
        }
    }
}
