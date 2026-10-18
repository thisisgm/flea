#!/bin/bash
# Rebuild pinned vendor bundles byte for byte; the caller supplies npm and network.
set -u
cd "$(LC_ALL=C LANGUAGE= dirname "$0")" || exit 1

[ -f package.json ] || { echo "vendor-js: no package.json beside this script"; exit 1; }

# The rebuild is proved against the tracked bundles, so a missing one refuses before the download.
for target in ../../ui/vendor/math.mjs ../../ui/vendor/mermaid.mjs; do
    [ -f "$target" ] || {
        echo "vendor-js: missing tracked target $target"
        exit 1
    }
done

npm ci || exit 1

# Flea's patch to the pinned library (see ui/vendor/LICENSES); a reject, a fuzz or an offset hunk fails the build.
patched=$(LC_ALL=C LANGUAGE= patch -d node_modules/beautiful-mermaid -p1 --forward --batch --fuzz=0 --no-backup-if-mismatch < patches/beautiful-mermaid+1.1.3.patch 2>&1) || {
    printf '%s\n' "$patched"
    echo "vendor-js: the beautiful-mermaid patch did not apply cleanly"
    exit 1
}
printf '%s\n' "$patched"
if printf '%s\n' "$patched" | LC_ALL=C LANGUAGE= grep -qiE 'fuzz|offset|reject|failed'; then
    echo "vendor-js: the beautiful-mermaid patch applied with an offset, fuzz or reject"
    exit 1
fi

# Bundles use one exact command each: minified ES modules for quickjs-ng, neutral platform, es2017.
npx esbuild math-entry.mjs --bundle --format=esm --platform=neutral --target=es2017 --minify --outfile=math-bundle.mjs || exit 1
npx esbuild mermaid-entry.mjs --bundle --format=esm --platform=neutral --target=es2017 --minify --outfile=mermaid-bundle.mjs || exit 1

# The proof is byte equality, not eyeballing: a flag drift changes bytes.
LC_ALL=C LANGUAGE= cmp math-bundle.mjs ../../ui/vendor/math.mjs || {
    echo "vendor-js: math.mjs differs, copy the rebuilt file over ui/vendor/math.mjs"
    exit 1
}
LC_ALL=C LANGUAGE= cmp mermaid-bundle.mjs ../../ui/vendor/mermaid.mjs || {
    echo "vendor-js: mermaid.mjs differs, copy the rebuilt file over ui/vendor/mermaid.mjs"
    exit 1
}
echo "vendor-js: both bundles reproduce byte for byte"
