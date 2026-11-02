# Packaging for Linux

```sh
cargo xtask bundle-linux                       # target/release/Inkframe-<version>-linux-x86_64.AppImage
cargo xtask bundle-linux --ffmpeg vendor/ffmpeg/ffmpeg --rhubarb vendor/rhubarb
```

This needs [appimagetool](https://github.com/AppImage/appimagetool/releases)
on `PATH` (or passed with `--appimagetool`). CI builds the AppImage on every
push, on the oldest Ubuntu GitHub offers: download it from the run's page
under **Artifacts**.

An AppImage is one file that runs on most distributions without being
installed. To open it, make it executable and run it, from a terminal:

```sh
chmod +x Inkframe-*-linux-x86_64.AppImage
./Inkframe-*-linux-x86_64.AppImage
```

or from the file manager (Properties › Permissions › "Allow executing file
as program", then double-click). To put Inkframe in the applications menu,
use a tool such as
[Gear Lever](https://flathub.org/apps/it.mijorus.gearlever) or
[AppImageLauncher](https://github.com/TheAssassin/AppImageLauncher).

`inkframe.desktop` in this folder is the launcher's name, icon and
categories. The icon is drawn by `cargo xtask icon`'s code at build time.

## What's inside

Only Inkframe's own files: the editor, the sample projects, the licences,
and optionally ffmpeg and Rhubarb. The system libraries it loads (the C
library, ALSA for sound, the graphics drivers) are expected on the
computer, as they are on any desktop. Because the C library's version
matters, the AppImage runs on Linuxes at least as new as the one it was
built on: Ubuntu 22.04 for CI's builds.

## Bundling ffmpeg

Use a **static** build, such as
[martin-riedl.de's](https://ffmpeg.martin-riedl.de), which CI uses. A distribution's
own ffmpeg is split into libraries that come and go with its packages, so
the packaging task refuses one that loads anything beyond the C library.
It needs the `prores_ks` and `libx264` encoders (HEVC with alpha is
macOS-only). Licensing works the same way as on macOS; see
[../macos/README.md](../macos/README.md#licensing).

## Bundling Rhubarb Lip Sync

Unpack Rhubarb's Linux release and pass its folder with `--rhubarb`, as on
the other platforms.
