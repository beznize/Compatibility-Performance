# DexNom Runtime

DexNom Runtime is a community fork of Marvisak's VIctory for Windows x64. It runs an Xbox Series executable from your own game installation and translates its GPU work for Direct3D 12. It builds on the Radeon runtime tested at `e061c69032b84ec6e2ec17340469cd689500dafd` and adds fixes for static sampler selection, wave64 lane masks and integer render targets. Game files, executables, saves and generated shader caches stay on your own computer.

## Run it on your PC

The Windows x64 prerelease includes the three compiled programs. Extract the ZIP into a writable folder and open `Play-RTX.cmd` to choose your game executable. Run `Play-RTX.cmd -Warm` to build the caches for your installation and GPU. The opening sequence is still unstable on the RTX 4070 SUPER. [Download the Windows x64 prerelease](https://github.com/beznize/Compatibility-Performance/releases) · [RTX benchmark](BENCHMARK.md)

To build and install from source, follow these steps.

1. Use 64-bit Windows and a current Direct3D 12 driver. The game uses mesh shaders and Shader Model 6.6. The Radeon gameplay results and known issues are listed below.
2. Install Rust for Windows and Visual Studio Build Tools. In the Visual Studio installer, select **Desktop development with C++**, including the x64 MSVC toolset and a Windows 10 or 11 SDK. [Rust installer](https://rustup.rs/) · [Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)
3. Download this repository using **Code → Download ZIP**, extract the ZIP to a writable folder, and double-click **Setup.cmd**. Select the Series X game executable inside your own extracted game installation. Keep its accompanying files together. Setup downloads the pinned build dependencies, verifies the BCPack format tables, and compiles the three Windows programs locally. [Download source ZIP](https://github.com/beznize/Compatibility-Performance/archive/refs/heads/main.zip)
4. Open **DexNom Runtime** from the Windows Start menu. Use a controller for the game's first-time setup. The launcher and your local data are in `%LOCALAPPDATA%\DexNom Runtime`. Saves are in `state`, shader caches in `cache`, and per-run diagnostics in `logs`. The file `logs\latest.txt` points to the latest game log.

To update, close the runtime, extract the new source ZIP, and run **Setup.cmd** again. It reuses the saved game path and the same save directory. To select a different installation, pass its executable path to Setup from PowerShell:

```powershell
.\Setup.cmd -GamePath "D:\My Games\Series X\game_scarlett_master.exe"
```

The warm-up scans the game directory for shader containers and archived graphics and compute states. Play can request additional combinations of shaders and rendering state. If Windows reports a missing `VCRUNTIME140.dll`, install Microsoft's x64 redistributable. [Microsoft Visual C++ Redistributable](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist)

For a manual build, run `.\Initialize-FormatTables.ps1`, then `cargo build --locked --release -p victory --features diagnostics --bins`. The release binaries are in `target\release`. Place all three together with the verified file from `crates/victory-formats/src/bcpack-tables.bin` outside the checkout and pass the game executable path to `victory-play.exe`. Custom data and cache directories are documented in the configuration example. [Configuration example](victory.example.toml)

## RTX 4070 SUPER test results

One opening on the RTX 4070 SUPER requested 83,557 distinct pipeline states. A clean archive pass translated 25,730 of 25,758 recognized shaders with strict wave64 handling. It found 28 compute translation failures, six further compute driver refusals and four graphics driver refusals. The run exited after 205 seconds and measured 0.3 FPS across 12 render-heavy intervals. The benchmark includes the archived graphics inventory, hardware checks and commands for auditing your own run. [RTX pipeline coverage and measurements](BENCHMARK.md#rtx-pipeline-coverage-audit)

## Try experimental ray tracing

After running **Setup.cmd**, open **DexNom Runtime (Experimental RT)** from the Windows Start menu. This starts the game with the acceleration-structure builder, ray traversal and lighting resolve enabled. The shortcut uses your existing saves. Open **DexNom Runtime** for the usual renderer.

To start the same RT configuration from PowerShell:

```powershell
& "$env:LOCALAPPDATA\DexNom Runtime\victory-play.exe" --rt
```

Chapter 1 City produced in-range ray and triangle intersections on an RX 7900 XTX. The game builds and traverses its acceleration structures through translated Direct3D 12 compute shaders. Frame times can still reach several seconds. Interior lighting, shadows and performance vary by scene. [Ray-tracing implementation](docs/fork-ray-tracing.md) · [City testing](BENCHMARK.md)

## Changes in this snapshot

- GPU readback identifies changed pages and combines adjacent copy ranges. This targets CPU/GPU synchronization and transfer overhead.
- Ray-structure builder dispatches now honor the game's thread counts and mask excess lanes in the final group. Read-only traversal no longer declares the entire guest memory directory writable when the shader sends its output to another buffer or texture.
- Texture handling searches for contiguous descriptor slots and admits already-resident textures under memory pressure. Earlier changes address publication order, placed-texture ownership and supported render-XOR layouts.
- Shaders can choose between two static samplers across a uniform branch. This recovered 40 shader variants from the tested archive. Another fix recovered one voxel visibility shader by recognizing a saved and restored wave64 lane mask. Mixed render targets retain their individual write masks when an integer target bypasses blending.
- The runtime preserves the game's vertex output interpolation and fixes output signature ordering. All 2,191 archive vertex shaders passed the Microsoft DXIL validator, including 18 water shaders. New textures and retries share the upload budget, and a previously refused buffer view is checked again when its backing allocation changes.
- The runtime resolves guest addresses from ray-tracing descriptors, preserves raw buffer offsets and prepares the memory each command list reads. Texture uploads can proceed while Windows remaps unrelated memory. In-game ray-traced lighting is still experimental.
- Five additional shadow compute shaders compile and create pipelines on the RX 7900 XTX. The translated ray-node regression also exercises three reflections through the guest-page directory. Changes confined to the standalone execution tests now preserve cached shader translations.
- Shader translation preserves the guest wave width for lane permutations and handles heap-indexed texture-size queries. Save handling flushes copied blobs before publication and can recover a complete checkpoint.

The source ZIP builds with the Windows setup script. The test report covers shader validation, a Radeon pipeline check, gameplay frame times and the remaining interior lighting issue. [Test and benchmark report](BENCHMARK.md)

### Testing CPU submission workers

For a local experiment, open PowerShell and run the following after setup. The process uses four CPU workers for eligible upload batches and can record independent transition command lists on the shared worker pool. The final GPU queue, fence signals and resource transitions retain the game's submission order. Compare the same save and camera after the initial streaming period. The default launcher uses the baseline settings.

```powershell
$env:VICTORY_FILL_WORKERS = '4'
$env:VICTORY_PARALLEL_PRELUDES = '1'
& "$env:LOCALAPPDATA\DexNom Runtime\victory-play.exe"
```

## Credits and licensing

- Marvisak and the VIctory contributors created the original runtime. [Marvisak/VIctory](https://github.com/Marvisak/VIctory)
- ZSlayerHQ and the contributors to the victory-world line provided the rendering baseline used here, including the Radeon work from triippywhale and asdaasdsaddf. [victory-world-runtime](https://github.com/ZSlayerHQ/victory-world-runtime) · [Integrated revision](https://github.com/ZSlayerHQ/victory-world-runtime/commit/99f8834aa16b98d7f3943b8de262a8d4857bda7b)
- The XCL contributors published an independent Xbox Series compatibility layer. Their work informed our investigation of shader translation, CPU/GPU memory synchronization and performance. [xcl-dev/xcl](https://github.com/xcl-dev/xcl)

The upstream README preserves the original credits and historical test results. [Upstream README](README.upstream.md)

VIctory source is licensed under GPL-3.0-or-later. The upstream project identifies the required BCPack format tables as separately supplied data outside that license. This source archive omits that file. The setup script verifies a local or upstream copy before use. [License](LICENSE) · [Upstream asset policy and table notice](docs/asset-policy.md)
