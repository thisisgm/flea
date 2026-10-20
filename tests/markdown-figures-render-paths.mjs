// A link's marks: Pele draws each as a plain shape beside its line, and the post-pass gives an unstyled link's marks the accent.
import { FACES, layered, load, others, sequences, theme } from "./mermaid-corpus.mjs";
import { argv, finish } from "./js-runtime.mjs";
import { postMermaid } from "../ui/js/FigureWorker.mjs";

let checks = 0;
let failures = 0;
function check(ok, why) {
    checks++;
    if (!ok) {
        failures++;
        console.log("FAIL " + why);
    }
}

const render = await load(argv[0]);
const line = theme.fg;
const marks = (svg) => svg.match(/<[a-z]+\b[^<>]*\sclass="pele-marker"[^<>]*>/g) || [];
// Every end mark Mermaid has for a flowchart, a class, a state, an ER and a sequence link, beside the corpus diagrams.
const ends = [
    ["flowchart ends", "flowchart LR\na <--> b\nb o--o c\nc x--x d\nd <-.-> e\ne <==> f"],
    ["class ends", "classDiagram\nA <|-- B\nC *-- D\nE o-- F\nG <-- H\nI ..|> J\nK ..> L"],
    ["state ends", "stateDiagram-v2\n[*] --> A\nA --> B\nB --> A\nB --> [*]"],
    ["er ends", "erDiagram\nA ||--o{ B : has\nC }|..|| D : uses\nE |o--|{ F : keeps"],
    ["sequence ends", "sequenceDiagram\nA->>B: solid\nB-->>A: dotted\nA-xB: cross\nA-)B: async\nA->B: open"]
];
for (const [name, source] of [...layered, ...sequences, ...others, ...ends]) {
    const svg = render(source, FACES[0]);
    check(!/\smarker-(?:start|mid|end)=|<marker\b|<polyline\b/.test(svg), name + ": no SVG marker and no polyline");
    const drawn = marks(svg);
    check(drawn.every((m) => !m.includes('"' + line + '"')), name + ": no mark draws in the line colour " + drawn.filter((m) => m.includes('"' + line + '"')).join(" "));
    check(drawn.every((m) => m.includes('"' + theme.accent + '"')), name + ": every mark draws in the accent");
    if (ends.some((e) => e[0] === name))
        check(drawn.length >= 4, name + ": draws its marks, " + drawn.length);
}

// A link a user style colours keeps its marks in that colour, and the unstyled link beside it takes the accent.
const styled = render("flowchart LR\na --x b\nb --o c\nc --> d\nlinkStyle 0 stroke:#ff0000\nlinkStyle 1 stroke:#00ff00", FACES[0]);
const groups = styled.match(/<g class="pele-edge"[^<>]*>[\s\S]*?<\/g>/g) || [];
check(groups.length === 3, "three links, " + groups.length);
check(/stroke="#ff0000"/.test(groups[0] || "") && marks(groups[0] || "").length === 1 && !marks(groups[0] || "").join("").includes(theme.accent), "the red link's cross inherits red");
check(marks(groups[1] || "").join("").includes('fill="#00ff00"') && !marks(groups[1] || "").join("").includes(theme.accent), "the green link's circle stays green");
check(marks(groups[2] || "").join("").includes('fill="' + theme.accent + '"'), "the unstyled link's head takes the accent");

// The rule on hand-built shapes: a filled head, a stroked mark that inherits, a hollow ring, and a mark outside any link stay apart.
const edges = '<g class="pele-edges" fill="none" stroke="#565f89">';
const hand = postMermaid('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 40" width="120" height="40">' + edges
    + '<g class="pele-edge" data-id="e"><path d="M1,20L80,20"/><path class="pele-marker" d="M88,20L80,23L80,17Z" fill="#565f89" stroke="none"/>'
    + '<path class="pele-marker" d="M4,16L4,24"/><circle class="pele-marker" cx="8" cy="20" r="3" fill="#101315"/></g></g>'
    + '<circle class="pele-marker pele-start" cx="100" cy="20" r="3" fill="#565f89"/></svg>', theme);
check(hand.includes('<path class="pele-marker" d="M88,20L80,23L80,17Z" fill="#7aa2f7" stroke="none"/>'), "a filled head takes the accent fill and keeps stroke none");
check(hand.includes('<path class="pele-marker" d="M4,16L4,24" stroke="#7aa2f7"/>'), "a stroked mark that inherits the link stroke gets the accent stroke");
check(hand.includes('<circle class="pele-marker" cx="8" cy="20" r="3" fill="#101315" stroke="#7aa2f7"/>'), "a hollow ring keeps its ground fill and gets the accent stroke");
check(hand.includes('<circle class="pele-marker pele-start" cx="100" cy="20" r="3" fill="#565f89"/>'), "a mark outside any link is left alone");
const accentless = postMermaid('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" width="20" height="20">' + edges
    + '<g class="pele-edge"><path class="pele-marker" d="M9,9L1,1L1,9Z" fill="#565f89" stroke="none"/></g></g></svg>', { ...theme, accent: "" });
check(accentless.includes('fill="' + theme.fg + '"'), "with no accent in the theme a mark takes the foreground, as the bundle's accent role does");

console.log(`MARKDOWN_PATHS ${checks} checks, ${failures} failed`);
finish(failures);
