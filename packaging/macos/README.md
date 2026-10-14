# Packaging for macOS

```sh
cargo xtask bundle-macos                         # target/release/Inkframe.app
cargo xtask bundle-macos --dmg                   # …plus a drag-to-install .dmg
cargo xtask bundle-macos --ffmpeg path/to/ffmpeg --dmg
```

The app is built for the Mac you run this on (Apple Silicon on an M-series
Mac). It's signed *ad hoc*, which is enough to run it on the Mac that built
it. Other Macs will say the developer can't be verified until the app is
signed with a Developer ID and notarized by Apple. That's the next packaging
step, and it needs an Apple Developer account.

`Info.plist` in this folder is the bundle's description; the version is
filled in from `Cargo.toml`. The icon is drawn in code in
`xtask/src/icon.rs`, and `cargo xtask icon` redraws the PNGs in
`assets/icon/` after you change it.

## Bundling ffmpeg

Without `--ffmpeg`, the app looks for ffmpeg the same way `cargo run` does:
the `INKFRAME_FFMPEG` variable, then `PATH`, then Homebrew's usual folders.
That's fine for your own Mac. To give the app to someone who doesn't have
ffmpeg, bundle one.

The ffmpeg you bundle must be **self-contained**: a single file that only
loads libraries that come with macOS. Homebrew's ffmpeg isn't (it's split
across a dozen libraries in `/opt/homebrew`), and the packaging task will
refuse it and list what it depends on. Use a static build instead, either one
you compile or a trusted prebuilt one for your architecture.

Put it at `vendor/ffmpeg/ffmpeg`, which git ignores. CI bundles the static
macOS arm64 build from [ffmpeg.martin-riedl.de](https://ffmpeg.martin-riedl.de)
(ffmpeg 9.0.2); its download URL and SHA-256 are pinned at the top of
`.github/workflows/ci.yml`.
Check the download against the SHA-256 checksum published beside it:

```sh
shasum -a 256 ffmpeg.zip     # compare with ffmpeg.zip.sha256
cargo xtask bundle-macos --ffmpeg vendor/ffmpeg/ffmpeg --dmg
```

Then run the export tests through the bundled copy, which encodes every
format for real and checks the results with ffprobe:

```sh
INKFRAME_FFMPEG=$PWD/target/release/Inkframe.app/Contents/MacOS/ffmpeg cargo test -p inkframe-export
```

It needs these encoders, which `ffmpeg -encoders` lists:

| Encoder | Used for |
|---|---|
| `prores_ks` | ProRes 4444, 4444 XQ and 422 HQ |
| `libx264` | H.264 MP4 |
| `hevc_videotoolbox` | HEVC with alpha |
| `pcm_s16le`, `aac` | Sound in .mov and .mp4 |

### Licensing

Inkframe stays MIT/Apache-2.0: it runs ffmpeg as a separate program and
never links to it. ffmpeg keeps its own licence, which depends on how it was
built. A build that includes x264 is GPL. Distributing it means passing that
licence on and making ffmpeg's source available. The packaging task copies
the licence the binary reports (`ffmpeg -L`) and its exact version into
`Inkframe.app/Contents/Resources/licenses/ffmpeg/`.
