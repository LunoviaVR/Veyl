# Third-party notices

## PawnIO modules (bundled)

`crates/windows-platform/pawnio/IntelMSR.bin` and `LpcIO.bin` are unmodified, signed
builds from PawnIO.Modules release 0.2.11 (https://github.com/namazso/PawnIO.Modules,
release zip SHA-256 43608cb89bc84247fef1368a139013f7d043e17db6d6c8dfc9b46bf0905a81f4).
They are licensed under the GNU LGPL 2.1 (see `crates/windows-platform/pawnio/COPYING`);
source code: https://github.com/namazso/PawnIO.Modules.

## PawnIO driver and PawnIOLib (not bundled)

The PawnIO driver (GPL-2.0 with an exception for programs using its device interface)
and PawnIOLib.dll (LGPL-2.1) are installed separately by the user from https://pawnio.eu
and loaded at run time from the PawnIO install folder.

## Apple-Style RTSS Overlays (bundled)

`public/overlay-themes/` holds the 1920×1080 overlay layouts (`layout.ovl`) and image atlases
(`atlas.png`) of the Essential, Laptop and TV designs from
https://github.com/realHfde/Apple-Style-Inspired-RTSS-Overlay, unmodified apart from the file
names. Copyright (c) 2026 Faisal, MIT License (see `public/overlay-themes/LICENSE`). Veyl draws
them with its own renderer and readings; RivaTuner Statistics Server is not used.

## Inter font (bundled)

The overlay designs are set in Inter by Rasmus Andersson (https://rsms.me/inter/), bundled from
the `@fontsource/inter` package (Latin, weights 700 and 800) under the SIL Open Font License 1.1
(`node_modules/@fontsource/inter/LICENSE`).

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
