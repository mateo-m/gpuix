---
'@gpuix/native': minor
'@gpuix/react': minor
---

Ship one prebuilt binary per OS, and pack the example app as an archive.

`@gpuix/native` now builds for the architecture each OS is mostly used on, and nothing else. Every extra target is a full gpui build, and gpui is most of the release wall clock, so six targets made every release twice as slow for platforms almost nobody installed.

| OS | Target | Renderer |
| --- | --- | --- |
| macOS | `aarch64-apple-darwin` | Metal |
| Linux | `x86_64-unknown-linux-gnu` | Vulkan / wgpu |
| Windows | `x86_64-pc-windows-msvc` | Direct3D |

Intel macOS, arm64 Linux and arm64 Windows have no prebuilt binary now. Open an issue if you need one back.

The standalone **chat** example also changes shape. A GitHub release asset is served as raw bytes, so a download lost the executable bit and, on macOS and Linux, arrived with no extension at all. Those two platforms now ship a `.tar.gz`, which keeps the mode and names itself. Windows keeps the `.exe`, which needs no unpacking.

```bash
tar -xzf example-chat-aarch64-apple-darwin.tar.gz
./example-chat-aarch64-apple-darwin
```

No `chmod` step, and the download is about 2.5x smaller: 83 MB of binary compresses to 33 MB.
