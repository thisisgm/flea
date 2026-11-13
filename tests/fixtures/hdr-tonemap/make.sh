#!/usr/bin/env bash
# Rebuilds the chart clips and their reference frames into a scratch directory: make.sh DIR.
# Clips are 1 s of one 10-bit frame in lossless HEVC (PQ with HDR10 metadata at 1000 nits, and HLG); references are ffmpeg libplacebo BT.709 renderings.
set -euo pipefail
out=${1:?scratch directory}
here=$(dirname "$0")
python3 -B "$here/chart.py" "$out/chart.pfm"
common="log-level=error:lossless=1:colorprim=bt2020:colormatrix=bt2020nc:range=limited"
encode() {
    ffmpeg -nostdin -v error -y -loop 1 -framerate 10 -i "$out/chart.pfm" -t 1 \
        -vf "zscale=tin=linear:pin=709:min=gbr:rin=full:t=$2:p=2020:m=2020_ncl:r=limited:npl=203,format=yuv420p10le" \
        -c:v libx265 -preset medium -x265-params "$common:transfer=$2$3" -tag:v hvc1 "$out/$1.mkv"
    ffmpeg -nostdin -v error -y -i "$out/$1.mkv" -frames:v 1 \
        -vf "libplacebo=colorspace=bt709:color_primaries=bt709:color_trc=bt709:range=pc:peak_detect=false" \
        -pix_fmt rgb24 "$out/$1-ref.png"
}
encode pq smpte2084 ":master-display=G(8500,39850)B(6550,2300)R(35400,14600)WP(15635,16450)L(10000000,50):max-cll=1000,400:hdr10=1"
encode hlg arib-std-b67 ""
