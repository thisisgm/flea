// The Mermaid test corpus, one render through the real figure path, and a label's measured width; the SVG readers live in mermaid-svg.mjs.
import { absolute, importFile, repoRoot, resolve } from "./js-runtime.mjs";

export * from "./mermaid-svg.mjs";

export const ARROW_LABEL_PAD = 8;
// A face is a per-character advance table for printable ASCII, 32 to 126, in thousandths of an em, regular and bold.
const TABLE_FIRST = 32;
const TABLE_LENGTH = 95;
const THOUSAND = 1000;
function flat(em) {
    return Array(TABLE_LENGTH).fill(Math.round(em * THOUSAND));
}
// A proportional face: narrow strokes, narrow punctuation, wide capitals and the widest letters, so a mean over the table fits none of them.
const NARROW_GLYPHS = "il.,:;'|!`jI";
const SLIM_GLYPHS = "frt()[]{} -\"/\\";
const WIDEST_GLYPHS = "mwMW@%";
const GLYPH_EM = { narrow: 0.25, slim: 0.36, widest: 0.92, capital: 0.68, digit: 0.62, other: 0.56 };
const BOLD_WIDENING = 1.1;
function proportional() {
    return Array.from({ length: TABLE_LENGTH }, (unused, i) => {
        const ch = String.fromCharCode(TABLE_FIRST + i);
        const em = NARROW_GLYPHS.includes(ch) ? GLYPH_EM.narrow : SLIM_GLYPHS.includes(ch) ? GLYPH_EM.slim : WIDEST_GLYPHS.includes(ch) ? GLYPH_EM.widest
            : /[A-Z]/.test(ch) ? GLYPH_EM.capital : /[0-9]/.test(ch) ? GLYPH_EM.digit : GLYPH_EM.other;
        return Math.round(em * THOUSAND);
    });
}
// The faces the fit checks run at: JetBrains Mono, a narrower monospace, a monospace whose bold is wider, and a proportional face.
const regularProportional = proportional();
export const FACES = [
    { name: "mono 0.6", regular: flat(0.6), bold: flat(0.6) },
    { name: "mono 0.5", regular: flat(0.5), bold: flat(0.5) },
    { name: "mono 0.6 bold 0.7", regular: flat(0.6), bold: flat(0.7) },
    { name: "proportional", regular: regularProportional, bold: regularProportional.map((v) => Math.round(v * BOLD_WIDENING)) }
];
export const theme = { bg: "#101315", fg: "#c0caf5", accent: "#7aa2f7", font: "monospace", bodyPx: 13 };

// Graphs whose edges the layering check reads back; fixture 10 is first, then mermaid's own docs examples, cycles, and the state, class and ER layouts.
export const layered = [
    ["fixture 10", "flowchart TD\nA[Start] --> B{Is it working?}\nB -->|Yes| C[Ship it]\nB -->|No| D[Debug]\nD --> B\nC --> E((Done))"],
    ["docs loop", "flowchart TD\nA[Start] --> B{Is it?}\nB -->|Yes| C[OK]\nC --> D[Rethink]\nD --> B\nB -->|No| E[End]"],
    ["docs subgraphs", "flowchart TB\nc1 --> a2\nsubgraph one\na1 --> a2\nend\nsubgraph two\nb1 --> b2\nend\nsubgraph three\nc1 --> c2\nend"],
    ["nested subgraphs", "flowchart TB\nsubgraph outer [Outer]\nsubgraph inner [Inner]\na1 --> a2\nend\na2 --> b1\nb1 --> a1\nend\ns --> a1\nb1 --> t"],
    ["branches and a join", "flowchart TD\nS --> A\nA --> B\nA --> C\nB --> D\nC --> D\nD --> A\nD --> E"],
    ["late source", "flowchart TD\nB --> C\nC --> D\nD --> B\nA --> C"],
    ["left to right back edges", "flowchart LR\nA --> B --> C --> D\nD --> B\nC --> A"],
    ["bottom to top triangle", "flowchart BT\nX --> Y\nY --> Z\nZ --> X\nY --> W"],
    ["two loops one entry", "flowchart RL\nIn --> P\nP --> Q\nQ --> P\nQ --> R\nR --> S\nS --> Q\nS --> Out"],
    ["plain dag", "flowchart TD\nA --> B\nA --> C\nB --> D\nC --> D"],
    ["docs link length", "flowchart TD\nA[Start] --> B{Is it?}\nB -->|Yes| C[OK]\nC --> D[Rethink]\nD --> B\nB ----->|No| E[End]"],
    ["wide and narrow glyphs", "flowchart LR\nA[WWW MMM WWW] --> B[illi lilii lil]\nB --> C[你好 世界]\nC --> A"],
    ["state loop", "stateDiagram-v2\nStart --> Check\nCheck --> Ship\nCheck --> Debug\nDebug --> Check\nShip --> End"],
    ["state late source", "stateDiagram-v2\nB --> C\nC --> D\nD --> B\nA --> C"],
    ["class loop", "classDiagram\nA --> B\nB --> C\nB --> D\nD --> B\nC --> E"],
    ["class late source", "classDiagram\nB --> C\nC --> D\nD --> B\nA --> C"],
    ["er loop", "erDiagram\nA ||--o{ B : a\nB ||--o{ C : b\nB ||--o{ D : c\nD ||--o{ B : d\nC ||--o{ E : e"],
    ["er late source", "erDiagram\nB ||--o{ C : a\nC ||--o{ D : b\nD ||--o{ B : c\nA ||--o{ C : d"]
];

// Notes in every form, over two and three lifelines, beside the first and the last, and over one.
export const notes = ["notes in every form",
    "sequenceDiagram\nparticipant A as Alice\nparticipant B as Bob\nparticipant C as Carol\nA->>C: hello\nNote over A,C: spans all three\nNote over A,B: spans two\nNote left of A: left of Alice\nNote right of C: right of Carol\nNote over B: only Bob\nNote right of A: right of Alice\nNote over B,C: a long note over two lifelines that needs them far apart"];

export const sequences = [
    ["fixture 11", "sequenceDiagram\nparticipant A as Alice\nparticipant B as Bob\nA->>B: Hello Bob, how are you?\nB-->>A: Fine, thanks\nA->>B: See you later"],
    ["long message and a note", "sequenceDiagram\nparticipant A as Alice\nparticipant B as Bob\nA->>B: A very long message that needs the lifelines far apart\nNote over A,B: A note over both lifelines\nB-->>A: ok"],
    ["wide and narrow glyphs", "sequenceDiagram\nparticipant W as WWW MMM\nparticipant I as illi lilii\nW->>I: WMWMW MWMWM WMWM\nI-->>W: iiiiillll lllii\nNote over W,I: WMWM MWMW iiii llll"],
    ["three participants and a self message", "sequenceDiagram\nparticipant U as User\nparticipant S as Server\nparticipant D as Database\nU->>S: request the report\nS->>S: validate the session token\nS->>D: select rows\nD-->>U: rows straight back to the user"]
];

// Other kinds join the fit check only; the layering reference is flowchart syntax.
export const others = [
    ["state", "stateDiagram-v2\n[*] --> Still\nStill --> Moving: go and keep going\nMoving --> Still\nMoving --> Crash\nCrash --> [*]"],
    ["bold headers", "classDiagram\nclass AVeryLongClassNameWiderThanTheMinimumBox {\n+id\n}\nAVeryLongClassNameWiderThanTheMinimumBox --> B"],
    ["bold entity header", "erDiagram\nA_VERY_LONG_ENTITY_NAME_FOR_THE_HEADER ||--o{ B : has\nA_VERY_LONG_ENTITY_NAME_FOR_THE_HEADER {\nint id PK\n}"],
    ["bold subgraph title", "flowchart TD\nsubgraph t [A subgraph title much wider than its one small node]\nx\nend"],
    ["flattened frames, docs example", "flowchart TB\nc1 --> a2\nsubgraph one\na1 --> a2\nend\nsubgraph two\nb1 --> b2\nend\nsubgraph three\nc1 --> c2\nend"],
    ["flattened nested frames", "flowchart TB\nsubgraph outer\nsubgraph inner\nm --> k\nend\nn --> m\nend\nk --> z"],
    ["left to right subgraph title", "flowchart LR\nsubgraph t [A subgraph title much wider than its two small nodes]\nx --> y\nend"],
    ["right to left subgraph title", "flowchart RL\nsubgraph t [A subgraph title much wider than its two small nodes]\nx --> y\nend\ny --> z"],
    ["class", "classDiagram\nclass Animal {\n+String name\n+makeSound() void\n}\nclass Duck {\n+swim() void\n}\nAnimal <|-- Duck : extends"],
    ["er", "erDiagram\nCUSTOMER ||--o{ ORDER : places\nCUSTOMER {\nstring name PK\nstring email\n}\nORDER {\nint number PK\n}"]
];

// Load the renderer pair of one tree, so the same checks run on a scratch copy of an older commit.
export async function load(root) {
    const base = root ? absolute(root) : repoRoot(import.meta.url);
    const worker = await importFile(resolve(base, "ui/js/FigureWorker.mjs"));
    const api = await importFile(resolve(base, "ui/vendor/mermaid.mjs"));
    return (source, face, extra) => worker.renderFigure("mermaid", source, false, { ...theme, ...extra, advances: face.regular, boldAdvances: face.bold }, api);
}

// Sample input: "WM" at 10 px on a face whose W and M are 0.92 em reaches 18.4; a character past ASCII counts the table's mean, a CJK one twice that.
export function labelWidth(string, size, face, bold) {
    const table = bold ? face.bold : face.regular;
    const mean = table.reduce((sum, v) => sum + v, 0) / table.length;
    const CJK_FIRST = 0x2e80;
    let total = 0;
    for (const ch of string) {
        const code = ch.codePointAt(0);
        total += code >= TABLE_FIRST && code < TABLE_FIRST + TABLE_LENGTH ? table[code - TABLE_FIRST] : code >= CJK_FIRST ? 2 * mean : mean;
    }
    return total / THOUSAND * size;
}
// A text's [left, right] by anchor and measured width.
export function extent(t, face) {
    const w = labelWidth(t.string, t.size, face, t.bold);
    return t.anchor === "middle" ? [t.x - w / 2, t.x + w / 2] : t.anchor === "end" ? [t.x - w, t.x] : [t.x, t.x + w];
}
