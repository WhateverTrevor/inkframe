# Packaging for Windows

```sh
cargo xtask bundle-windows                          # target\release\Inkframe-<version>-windows-x64-setup.exe
cargo xtask bundle-windows --ffmpeg vendor\ffmpeg\ffmpeg.exe
```

This needs [Inno Setup 6](https://jrsoftware.org/isinfo.php)
(`winget install JRSoftware.InnoSetup`). GitHub's Windows build machines
already have it, and CI builds the installer on every push: download it from
the run's page under **Artifacts**.

The installer:

- installs for the current user without needing administrator rights, or
  for everyone on the PC if you choose that on the first page
- adds Inkframe to the Start menu, and optionally the desktop
- opens `.inkf` project files with Inkframe when they're double-clicked
- registers an uninstaller under Settings › Apps

`inkframe.iss` in this folder describes all of that. The executable's icon is
embedded at build time by `crates/inkframe-app/build.rs` from
`assets/icon/inkframe.ico`, which `cargo xtask icon` draws.

## Not signed yet

Windows SmartScreen warns about installers that aren't code-signed ("Windows
protected your PC"). Click **More info › Run anyway** to install. Signing
needs a code-signing certificate, which is a later packaging step.

## Bundling ffmpeg

Without `--ffmpeg`, Inkframe looks for ffmpeg in the `INKFRAME_FFMPEG`
variable and then on `PATH`. To bundle one, use a **static** build: a single
large `ffmpeg.exe` with no `avcodec-*.dll` files beside it. The packaging
task refuses a "shared" build, since only `ffmpeg.exe` is copied into the
installer. It needs the `prores_ks` and `libx264` encoders (HEVC with alpha
is macOS-only). Licensing works the same way as on macOS; see
[../macos/README.md](../macos/README.md#licensing).
