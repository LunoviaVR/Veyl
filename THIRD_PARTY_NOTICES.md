# Third-party notices

## PawnIO modules (bundled)

`crates/windows-platform/pawnio/IntelMSR.bin` and `LpcIO.bin` are unmodified, signed
builds from PawnIO.Modules release 0.2.11 (https://github.com/namazso/PawnIO.Modules,
release zip SHA-256 43608cb89bc84247fef1368a139013f7d043e17db6d6c8dfc9b46bf0905a81f4).
They are licensed under the GNU LGPL 2.1 (see `crates/windows-platform/pawnio/COPYING`);
source code: https://github.com/namazso/PawnIO.Modules.

## Intel PresentMon (bundled)

`src-tauri/resources/PresentMon-2.6.0-x64.exe` is the unmodified, Intel-signed console application
from PresentMon release 2.6.0 (https://github.com/GameTechDev/PresentMon, SHA-256
b2a706bc6ad475749e3b7e3409263aa1e6906d45bdcf993f6dbc0f660188f1af). Copyright (C) 2017-2024
Intel Corporation, MIT License (see `src-tauri/resources/PresentMon-LICENSE.txt`, installed
next to it in Veyl's `tools` folder).

## PawnIO setup (bundled)

`src-tauri/resources/PawnIO_setup.exe` is the unmodified, signed PawnIO 2.2.0 setup
(https://github.com/namazso/PawnIO.Setup, SHA-256
1f519a22e47187f70a1379a48ca604981c4fcf694f4e65b734aaa74a9fba3032), installed in Veyl's `tools`
folder and run only when the user clicks Install PawnIO. It installs the PawnIO driver (GPL-2.0,
see `src-tauri/resources/PawnIO-LICENSE.txt`, installed next to it; source code:
https://github.com/namazso/PawnIO) and PawnIOLib.dll (LGPL-2.1), which Veyl loads at run time
from the PawnIO install folder.

## Apple-Style RTSS Overlays (bundled)

`public/overlay-themes/` holds the 1920×1080 overlay layouts (`layout.ovl`) and image atlases
(`atlas.png`) of the Essential, Laptop and TV designs from
https://github.com/realHfde/Apple-Style-Inspired-RTSS-Overlay, unmodified apart from the file
names. Copyright (c) 2026 Faisal, MIT License (see `public/overlay-themes/LICENSE`). Veyl draws
them with its own renderer and readings; RivaTuner Statistics Server is not used.
`public/overlay-themes/cassette/` is not part of that work: it is Veyl's own Cassette Futurism
design, in the same file format.

## Inter font (bundled)

The overlay designs are set in Inter by Rasmus Andersson (https://rsms.me/inter/), bundled from
the `@fontsource/inter` package (Latin, weights 700 and 800) under the SIL Open Font License 1.1
(`node_modules/@fontsource/inter/LICENSE`).

## VT323 font (bundled)

The Cassette Futurism theme sets its large readouts, and the overlay design of the same name all
its text, in VT323 by The VT323 Project Authors (Peter Hull), bundled from the
`@fontsource/vt323` package (Latin, weight 400) under the SIL Open Font License 1.1
(`node_modules/@fontsource/vt323/LICENSE`).

## Intel PresentMon (downloaded on request, not bundled)

When the user clicks **Install PresentMon**, Veyl downloads `PresentMon-2.6.0-x64.exe`
(MIT License, https://github.com/GameTechDev/PresentMon) from its GitHub release and keeps
it only if its size and SHA-256 match the published values
(`b2a706bc6ad475749e3b7e3409263aa1e6906d45bdcf993f6dbc0f660188f1af`). It runs as a separate
program; its license is distributed with the release on GitHub.

## Vendor runtimes (not bundled)

AMD ADLX (`amdadlx64.dll`, SDK MIT-licensed) and NVIDIA NVML (`nvml.dll`) are part of the
installed GPU drivers and are loaded from System32 at run time.

## Register layouts

Nuvoton NCT679x register addresses and scaling follow the Linux `nct6775` driver and
LibreHardwareMonitor (`Nct677X`, MPL-2.0); no source code from either is included.
