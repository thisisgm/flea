# OP1 typography regression fixtures

JetBrains Mono Regular and Bold 2.304, copyright 2020 JetBrains s.r.o., Apache License 2.0.
The files come unchanged from Debian fonts-jetbrains-mono 2.304+ds-5, installed under
/usr/share/fonts/truetype/jetbrains-mono. LICENSE-JetBrainsMono.txt contains the license.
Upstream: https://github.com/JetBrains/JetBrainsMono.

The Copy To HIG board specifies JetBrains Mono. These fixtures let West's real GPUI text
system measure and rasterize that family without silently falling back when the OEM
Nerd Font alias is absent. Tests publish only their own Typeface global; application
font selection is unchanged. Both fixtures have 1000 units per em, hhea ascent 1020,
descent -300, and zero line gap. The 14 px body therefore has ascent 14.28 and descent 4.2.

SHA256:

- Regular: 9fd21a629ec73a93d1665b6d8bbb73928040a290b49d2f6772505b8129e68cdc
- Bold: 10389fb114512e1df7ca54f791b9f83572e2785e9ff1946c8197d5e5faaafd7a

The fixtures are test inputs, not native screenshot acceptance. The actual OEM alias,
font version, display scale and presented pixels still require the controller's native pass.
