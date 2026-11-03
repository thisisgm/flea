#!/bin/bash
# Rebuild pinned vendor bundles byte for byte; the caller supplies npm and network.
set -u

# This tool reads machine output and launches no application for the user.
export LC_ALL=C.UTF-8
unset LANGUAGE
cd "$(dirname "$0")" || exit 1

[ -f package.json ] || { echo "vendor-js: no package.json beside this script"; exit 1; }

# The rebuild is proved against the tracked bundles, so a missing one refuses before the download.
for target in ../../ui/vendor/math.mjs ../../ui/vendor/mermaid.mjs; do
    [ -f "$target" ] || {
        echo "vendor-js: missing tracked target $target"
        exit 1
    }
done

npm ci || exit 1

# Bundles use one exact command each: minified ES modules for quickjs-ng, neutral platform, es2017.
npx esbuild math-entry.mjs --bundle --format=esm --platform=neutral --target=es2017 --minify --outfile=math-bundle.mjs || exit 1
npx esbuild mermaid-entry.mjs --bundle --format=esm --platform=neutral --target=es2017 --minify --outfile=mermaid-bundle.mjs || exit 1

# The proof is byte equality, not eyeballing: a flag drift changes bytes.
cmp math-bundle.mjs ../../ui/vendor/math.mjs || {
    echo "vendor-js: math.mjs differs, copy the rebuilt file over ui/vendor/math.mjs"
    exit 1
}
cmp mermaid-bundle.mjs ../../ui/vendor/mermaid.mjs || {
    echo "vendor-js: mermaid.mjs differs, copy the rebuilt file over ui/vendor/mermaid.mjs"
    exit 1
}
echo "vendor-js: both bundles reproduce byte for byte"
