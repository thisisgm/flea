//! A 100,000-name folder shaped like a real one, and the keys typed over it, for the differential and cost tests.

pub(super) struct Rng(pub(super) u64);

impl Rng {
    pub(super) fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    pub(super) fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[self.below(items.len())]
    }
}

const WORDS: &[&str] = &[
    "report", "invoice", "notes", "meeting", "budget", "project", "draft", "final", "summary", "plan", "design", "review", "photo",
    "scan", "contract", "letter", "resume", "presentation", "slides", "agenda", "minutes", "receipt", "statement", "tax", "payroll",
    "schedule", "proposal", "quote", "order", "shipping", "backup", "archive", "export", "import", "data", "config", "settings",
    "readme", "license", "changelog", "todo", "ideas", "journal", "diary", "travel", "vacation", "family", "wedding", "birthday",
    "party", "recipe", "workout", "music", "playlist", "podcast", "episode", "lecture", "homework", "assignment", "thesis", "chapter",
    "paper", "research", "analysis", "results", "figures", "table", "chart", "dashboard", "quarterly", "annual", "monthly", "weekly",
    "daily", "client", "customer", "vendor", "supplier", "inventory", "sales", "marketing", "campaign", "brand", "logo", "banner",
    "mockup", "wireframe", "prototype", "sprint", "release", "build", "deploy", "server", "database", "network", "security", "audit",
    "compliance", "policy", "handbook", "manual", "guide", "tutorial", "course", "training", "onboarding", "interview", "feedback",
    "survey", "old", "new", "copy", "v2", "rev", "2024", "2025", "2026", "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug",
    "sep", "oct", "nov", "dec", "home", "garden", "kitchen", "car", "insurance", "bank", "loan", "mortgage", "health", "doctor",
];
const EXT: &[&str] = &[
    "pdf", "docx", "xlsx", "pptx", "txt", "md", "jpg", "jpeg", "png", "gif", "mp4", "mov", "mp3", "flac", "zip", "tar.gz", "rs", "py",
    "js", "ts", "json", "csv", "html", "svg", "odt", "epub", "iso", "log", "pdf", "txt", "jpg", "png", "md", "docx",
];
const ARTISTS: &[&str] = &["Daft Punk", "Radiohead", "Nina Simone", "Aphex Twin", "Bjork", "Massive Attack", "Portishead", "Bonobo"];
const ACCENTED: &[&str] = &[
    "Café notes.md", "Résumé 2024.pdf", "Übersicht Projekt.xlsx", "会议记录 2025.docx", "Отчёт за квартал.pdf", "ΣΗΜΕΙΩΣΕΙΣ.txt",
    "Fotos Málaga.zip", "naïve plan.txt", "Größe Tabelle.csv", "東京 旅行.jpg",
];
// Rare words planted a few times, so a typo of one has only a few close matches.
const RARE: &[&str] = &["kubernetes", "photosynthesis", "spreadsheet", "chrysanthemum", "fahrenheit"];

fn joined(rng: &mut Rng, words: usize) -> String {
    let sep = rng.pick(&[" ", "_", "-", "", " "]);
    let parts: Vec<String> = (0..words)
        .map(|_| {
            let word = rng.pick(WORDS);
            if sep.is_empty() {
                let mut chars = word.chars();
                chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
            } else {
                word.to_string()
            }
        })
        .collect();
    parts.join(sep)
}

/// 100,000 names shaped like a real large folder: documents, camera dumps, music, code, folders, long titles, ids, accents.
pub(super) fn realistic_names() -> Vec<String> {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let mut names = Vec::with_capacity(100_000);
    for row in 0..100_000usize {
        let kind = rng.below(100);
        let name = match kind {
            0..=34 => {
                let words = 1 + rng.below(5);
                let mut name = joined(&mut rng, words);
                if rng.below(4) == 0 {
                    name.push_str(&format!(" 20{:02}-{:02}-{:02}", 18 + rng.below(9), 1 + rng.below(12), 1 + rng.below(28)));
                }
                if rng.below(8) == 0 {
                    name.push_str(&format!(" ({})", 1 + rng.below(4)));
                }
                format!("{name}.{}", rng.pick(EXT))
            }
            35..=54 => match rng.below(4) {
                0 => format!("IMG_20{:02}{:02}{:02}_{:02}{:02}{:02}.jpg", 18 + rng.below(9), 1 + rng.below(12), 1 + rng.below(28), rng.below(24), rng.below(60), rng.below(60)),
                1 => format!("PXL_20{:02}{:02}{:02}_{:02}{:02}{:02}{:03}.jpg", 18 + rng.below(9), 1 + rng.below(12), 1 + rng.below(28), rng.below(24), rng.below(60), rng.below(60), rng.below(1000)),
                2 => format!("DSC{:05}.JPG", rng.below(100_000)),
                _ => format!("Screenshot from 20{:02}-{:02}-{:02} {:02}-{:02}-{:02}.png", 18 + rng.below(9), 1 + rng.below(12), 1 + rng.below(28), rng.below(24), rng.below(60), rng.below(60)),
            },
            55..=64 => {
                let words = 1 + rng.below(4);
                format!("{:02} - {} - {}.flac", 1 + rng.below(20), rng.pick(ARTISTS), joined(&mut rng, words))
            }
            65..=74 => {
                let words = 1 + rng.below(3);
                let stem: String = joined(&mut rng, words).replace([' ', '-'], "_").to_lowercase();
                format!("{stem}.{}", rng.pick(&["rs", "py", "ts", "js", "go", "c", "h", "java", "kt", "swift"]))
            }
            75..=84 => {
                let words = 1 + rng.below(3);
                joined(&mut rng, words)
            }
            85..=89 => {
                let words = 6 + rng.below(9);
                let title: Vec<&str> = (0..words).map(|_| rng.pick(WORDS)).collect();
                format!("{} [{}p].{}", title.join(" "), rng.pick(&["720", "1080", "2160"]), rng.pick(&["mp4", "mkv", "webm"]))
            }
            90..=94 => format!("{:016x}.{}", rng.next(), rng.pick(&["tmp", "bin", "dat", "part", "cache"])),
            95..=97 => format!("{} {}", rng.pick(ACCENTED), row % 97),
            _ => {
                let words = 1 + rng.below(3);
                format!("{}.{}", joined(&mut rng, words), rng.pick(EXT))
            }
        };
        names.push(name);
    }
    // A few planted rare names, a typo of which finds only a handful of close matches.
    for (k, rare) in RARE.iter().enumerate() {
        for copy in 0..3 {
            let at = (k * 7919 + copy * 31_337 + 11) % names.len();
            names[at] = format!("{rare} {} notes.md", copy + 1);
        }
    }
    names
}

/// Typing sequences: every prefix is one keystroke, as the line asks the backend once per key.
pub(super) fn realistic_keys() -> Vec<String> {
    let typed = [
        "report", "IMG_2025", "meeting notes", "kuberentes", "meeting notse", "quartelry report", "zqxj", "*.pdf", "IMG_*_14",
        "*report*", "Screenshot from 2025",
    ];
    let mut keys = Vec::new();
    for word in typed {
        let chars: Vec<char> = word.chars().collect();
        for end in 1..=chars.len() {
            keys.push(chars[..end].iter().collect());
        }
    }
    keys
}
