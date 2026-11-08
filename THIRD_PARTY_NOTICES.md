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

## Vendor runtimes (not bundled)

AMD ADLX (`amdadlx64.dll`, SDK MIT-licensed) and NVIDIA NVML (`nvml.dll`) are part of the
installed GPU drivers and are loaded from System32 at run time.

## Register layouts

Nuvoton NCT679x register addresses and scaling follow the Linux `nct6775` driver and
LibreHardwareMonitor (`Nct677X`, MPL-2.0); no source code from either is included.
