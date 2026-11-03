// The one runtime seam of the figure tests: they run under node, and under quickjs-ng where the CI image has no node.
const underNode = typeof process !== "undefined" && process.versions !== undefined && process.versions.node !== undefined;
const std = underNode ? null : await import("qjs:std");
const os = underNode ? null : await import("qjs:os");
const fs = underNode ? await import("node:fs") : null;

// Sample input: the fixtures directory answers its file names in either engine.
export function readDir(path) {
    if (underNode)
        return fs.readdirSync(path);
    const out = os.readdir(path);
    return Array.isArray(out[0]) ? out[0] : out;
}
const url = underNode ? await import("node:url") : null;
const childProcess = underNode ? await import("node:child_process") : null;

// The command line after the script name.
export const argv = underNode ? process.argv.slice(2) : scriptArgs.slice(1);

// Sample input: node reports file:///tmp/a%20b/x.mjs for /tmp/a b/x.mjs; quickjs-ng reports file:///tmp/a b/x.mjs, raw, so only node's form is decoded.
export function pathOfUrl(fileUrl) {
    return underNode ? url.fileURLToPath(fileUrl) : fileUrl.replace(/^file:\/\//, "");
}

// Sample input: ("/a/b", "../c/./d") answers /a/c/d; an absolute second part wins.
export function resolve(base, part) {
    const out = part.startsWith("/") ? [] : base.split("/").filter((s) => s !== "");
    for (const piece of part.split("/")) {
        if (piece === "..")
            out.pop();
        else if (piece !== "" && piece !== ".")
            out.push(piece);
    }
    return "/" + out.join("/");
}

// An argument path made absolute against the working directory.
export function absolute(path) {
    const here = underNode ? process.cwd() : os.getcwd()[0];
    return resolve(here, path);
}

// The directory of the tests folder's parent, wherever it is checked out, spaces and all.
export function repoRoot(metaUrl) {
    return resolve(pathOfUrl(metaUrl), "../..");
}

export function readText(path) {
    if (underNode)
        return fs.readFileSync(path, "utf8");
    const text = std.loadFile(path);
    if (text === null)
        throw new Error("cannot read " + path);
    return text;
}

// Import a module by absolute path; node needs a file URL, which escapes the path once.
export async function importFile(path) {
    return underNode ? await import(url.pathToFileURL(path).href) : await import(path);
}

// Sample input: TZ=Pacific/Tongatapu answers true under node and under quickjs-ng when its std names setenv.
// The zone must be set before the first Date use, where both engines read it.
function setZone(zone) {
    if (underNode) {
        process.env.TZ = zone;
        return true;
    }
    if (std !== null && typeof std.setenv === "function") {
        std.setenv("TZ", zone);
        return true;
    }
    return false;
}

// Sample input: forcing the zone answers true under node, and under quickjs-ng when its std names setenv.
// The figure helper formats in UTC, so a Gantt render asking for one day runs in a UTC zone.
export function forceUtcZone() {
    return setZone("UTC");
}

// Run this script again with arguments under a time bound, zone set first, answering {status, output}; 0 is a clean exit.
export function runSelf(metaUrl, args, boundSeconds, zone) {
    const script = pathOfUrl(metaUrl);
    if (underNode) {
        const env = zone ? { ...process.env, TZ: zone } : process.env;
        const child = childProcess.spawnSync(process.execPath, [script, ...args], { timeout: boundSeconds * 1000, encoding: "utf8", env });
        return { status: child.error ? -1 : child.status, output: child.error ? child.error.code : child.stdout.trim() };
    }
    const exe = os.readlink("/proc/self/exe")[0];
    const [readEnd, writeEnd] = os.pipe();
    const command = zone
        ? ["env", "TZ=" + zone, "timeout", String(boundSeconds), exe, script, ...args]
        : ["timeout", String(boundSeconds), exe, script, ...args];
    const status = os.exec(command, { block: true, stdout: writeEnd, stderr: writeEnd });
    os.close(writeEnd);
    const reader = std.fdopen(readEnd, "r");
    const output = reader.readAsString().trim();
    reader.close();
    return { status, output };
}

// End the run with the failure count as its status.
export function finish(failures) {
    if (underNode)
        process.exitCode = failures > 0 ? 1 : 0;
    else if (failures > 0)
        std.exit(1);
}
