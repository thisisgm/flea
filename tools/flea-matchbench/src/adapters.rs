// The six library adapters and Flea's own scorer behind one trait, plus the shared scoring helpers.
use std::sync::Arc;

// One entrant: load builds whatever the library keeps, query answers corpus indices best first plus the total match count.
pub trait Adapter {
    fn load(&mut self, corpus: Arc<Vec<String>>);
    fn query(&mut self, text: &str, limit: usize, append: bool) -> (Vec<u32>, usize);
}

// Scores every candidate on up to `threads` scoped threads for libraries that ship no parallel entry point.
fn chunked<F, S>(corpus: &[String], threads: usize, make: &F) -> Vec<(i64, u32)>
where
    F: Fn() -> S + Sync,
    S: FnMut(&str) -> Option<i64>,
{
    let chunk = corpus.len().div_ceil(threads.max(1)).max(1);
    let mut parts: Vec<Vec<(i64, u32)>> = Vec::new();
    std::thread::scope(|scope| {
        let handles: Vec<_> = corpus
            .chunks(chunk)
            .enumerate()
            .map(|(n, slice)| {
                scope.spawn(move || {
                    let mut score = make();
                    let base = (n * chunk) as u32;
                    let mut hits = Vec::new();
                    for (i, candidate) in slice.iter().enumerate() {
                        if let Some(s) = score(candidate) {
                            hits.push((s, base + i as u32));
                        }
                    }
                    hits
                })
            })
            .collect();
        for handle in handles {
            parts.push(handle.join().expect("scoring thread panicked"));
        }
    });
    parts.concat()
}

// Keeps the best `limit` hits in order: score descending, then the library's tie rule.
fn top<C>(mut hits: Vec<(i64, u32)>, limit: usize, cmp: C) -> (Vec<u32>, usize)
where
    C: Fn(&(i64, u32), &(i64, u32)) -> std::cmp::Ordering,
{
    let total = hits.len();
    if limit < hits.len() {
        hits.select_nth_unstable_by(limit, &cmp);
        hits.truncate(limit);
    }
    hits.sort_unstable_by(&cmp);
    (hits.into_iter().map(|(_, i)| i).collect(), total)
}

fn by_score_then_index(a: &(i64, u32), b: &(i64, u32)) -> std::cmp::Ordering {
    b.0.cmp(&a.0).then(a.1.cmp(&b.1))
}

// frizbee and neo_frizbee share one API; the macro keeps the two adapters identical apart from the crate.
macro_rules! frizbee_adapter {
    ($name:ident, $krate:ident) => {
        struct $name {
            corpus: Arc<Vec<String>>,
            threads: usize,
            typo: bool,
        }

        impl Adapter for $name {
            fn load(&mut self, corpus: Arc<Vec<String>>) {
                self.corpus = corpus;
            }

            fn query(&mut self, text: &str, limit: usize, _append: bool) -> (Vec<u32>, usize) {
                let mut config = $krate::Config::default();
                if self.typo {
                    // The typo profile allows one typo per four needle characters, the rule frizbee's README shows.
                    config.max_typos = Some((text.chars().count() / 4) as u16);
                }
                let mut matcher = $krate::Matcher::new(text, &config);
                let matches = matcher.match_list_parallel(&self.corpus[..], self.threads);
                let total = matches.len();
                (
                    matches.into_iter().take(limit).map(|m| m.index).collect(),
                    total,
                )
            }
        }
    };
}

frizbee_adapter!(Frizbee, frizbee);
frizbee_adapter!(NeoFrizbee, neo_frizbee);

struct Nucleo {
    engine: Option<nucleo::Nucleo<u32>>,
    threads: usize,
    last: String,
}

impl Nucleo {
    // Ticks until the worker pool reports no work left, the point nucleo's own snapshot is final.
    fn settle(engine: &mut nucleo::Nucleo<u32>) {
        while engine.tick(10).running {}
    }
}

impl Adapter for Nucleo {
    fn load(&mut self, corpus: Arc<Vec<String>>) {
        let config = nucleo::Config::DEFAULT.match_paths();
        let mut engine = nucleo::Nucleo::new(config, Arc::new(|| {}), Some(self.threads), 1);
        let injector = engine.injector();
        for (i, candidate) in corpus.iter().enumerate() {
            injector.push(i as u32, |_, columns| {
                columns[0] = candidate.as_str().into()
            });
        }
        Self::settle(&mut engine);
        self.engine = Some(engine);
    }

    fn query(&mut self, text: &str, limit: usize, append: bool) -> (Vec<u32>, usize) {
        let engine = self.engine.as_mut().expect("query before load");
        // Append is nucleo's incremental path: a query that only extends the last one filters the previous matches.
        let append = append && text.starts_with(self.last.as_str()) && !self.last.is_empty();
        engine.pattern.reparse(
            0,
            text,
            nucleo::pattern::CaseMatching::Smart,
            nucleo::pattern::Normalization::Smart,
            append,
        );
        Self::settle(engine);
        self.last = text.to_string();
        let snapshot = engine.snapshot();
        let total = snapshot.matched_item_count() as usize;
        let take = limit.min(total) as u32;
        (
            snapshot
                .matched_items(..take)
                .map(|item| *item.data)
                .collect(),
            total,
        )
    }
}

struct FuzzyMatcher {
    corpus: Arc<Vec<String>>,
    threads: usize,
}

impl Adapter for FuzzyMatcher {
    fn load(&mut self, corpus: Arc<Vec<String>>) {
        self.corpus = corpus;
    }

    fn query(&mut self, text: &str, limit: usize, _append: bool) -> (Vec<u32>, usize) {
        use fuzzy_matcher::FuzzyMatcher as _;
        let make = || {
            let matcher = fuzzy_matcher::skim::SkimMatcherV2::default();
            move |candidate: &str| matcher.fuzzy_match(candidate, text)
        };
        top(
            chunked(&self.corpus, self.threads, &make),
            limit,
            by_score_then_index,
        )
    }
}

struct SublimeFuzzy {
    corpus: Arc<Vec<String>>,
    threads: usize,
}

impl Adapter for SublimeFuzzy {
    fn load(&mut self, corpus: Arc<Vec<String>>) {
        self.corpus = corpus;
    }

    fn query(&mut self, text: &str, limit: usize, _append: bool) -> (Vec<u32>, usize) {
        let make = || {
            |candidate: &str| sublime_fuzzy::best_match(text, candidate).map(|m| m.score() as i64)
        };
        top(
            chunked(&self.corpus, self.threads, &make),
            limit,
            by_score_then_index,
        )
    }
}

struct FleaCurrent {
    corpus: Arc<Vec<String>>,
    threads: usize,
}

impl Adapter for FleaCurrent {
    fn load(&mut self, corpus: Arc<Vec<String>>) {
        self.corpus = corpus;
    }

    fn query(&mut self, text: &str, limit: usize, _append: bool) -> (Vec<u32>, usize) {
        let make = || {
            let mut fuzzy = crate::backend::fuzzy::Fuzzy::new(text);
            move |candidate: &str| fuzzy.score(candidate).map(i64::from)
        };
        let corpus = self.corpus.clone();
        // Flea's own result order: score, then shorter path, then name order.
        let order = |a: &(i64, u32), b: &(i64, u32)| {
            crate::backend::fuzzy::rank_order(
                a.0 as i32,
                &corpus[a.1 as usize],
                b.0 as i32,
                &corpus[b.1 as usize],
            )
        };
        top(chunked(&self.corpus, self.threads, &make), limit, order)
    }
}

// Entrant name, crate version, licence and the parallel mode the adapter uses, the identity printed at boot.
pub const ENTRANTS: &[(&str, &str, &str, &str)] = &[
    (
        "frizbee",
        "frizbee 0.13.0",
        "MIT",
        "library match_list_parallel",
    ),
    (
        "neo_frizbee",
        "neo_frizbee 0.13.3",
        "MIT",
        "library match_list_parallel",
    ),
    (
        "nucleo",
        "nucleo 0.5.0 (nucleo-matcher 0.3.1)",
        "MPL-2.0",
        "library worker pool",
    ),
    (
        "fuzzy-matcher",
        "fuzzy-matcher 0.3.7 SkimMatcherV2",
        "MIT",
        "harness chunked scoped threads",
    ),
    (
        "sublime_fuzzy",
        "sublime_fuzzy 0.7.0 best_match",
        "Apache-2.0",
        "harness chunked scoped threads",
    ),
    (
        "flea-current",
        "flea src/backend/fuzzy.rs at build tree",
        "MIT",
        "harness chunked scoped threads",
    ),
];

pub fn make(name: &str, profile: &str, threads: usize) -> Result<Box<dyn Adapter>, String> {
    let empty = Arc::new(Vec::new());
    let typo = match (name, profile) {
        ("frizbee" | "neo_frizbee", "typo") => true,
        (_, "native") => false,
        _ => return Err(format!("unknown profile {profile} for {name}")),
    };
    Ok(match name {
        "frizbee" => Box::new(Frizbee {
            corpus: empty,
            threads,
            typo,
        }),
        "neo_frizbee" => Box::new(NeoFrizbee {
            corpus: empty,
            threads,
            typo,
        }),
        "nucleo" => Box::new(Nucleo {
            engine: None,
            threads,
            last: String::new(),
        }),
        "fuzzy-matcher" => Box::new(FuzzyMatcher {
            corpus: empty,
            threads,
        }),
        "sublime_fuzzy" => Box::new(SublimeFuzzy {
            corpus: empty,
            threads,
        }),
        "flea-current" => Box::new(FleaCurrent {
            corpus: empty,
            threads,
        }),
        _ => return Err(format!("unknown entrant {name}")),
    })
}
