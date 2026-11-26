# RTX 4070 SUPER testing, 26 November 2026

Tests used Windows x64, an RTX 4070 SUPER with 12 GB, a Core i5-12600KF, and 16 GB of system RAM. Each launch used a private copy of the same user state and a 300-second cutoff. The launch configuration preserved the title's resolution and features. The menu, opening sequence and scene progressed at different rates between runs.

| Subsystem | Status | Verified improvement | Next blocker |
| --- | --- | --- | --- |
| Offline shader translation | Fixed for this archive | 25,745 translated and 13 refused became 25,758 translated and zero refused. | Titles and shaders outside the offline scanner still need runtime coverage. |
| RTX compute pipelines | Partial | 2,700 pipelines and 31 refusals became 2,728 pipelines and six refusals. Full scans took 510 and 311 seconds with different cache states. | Six multi-wave shaders require 64-lane semantics on 32-lane hardware. |
| Ray-tracing builder | Partial | All 53 pipeline objects were created and builder dispatches were recorded. | Light transport, reflections and shadows still require image comparison. |
| Packed lighting and texture storage | Partial | The card preserved every tested RGB9E5 lighting word across two mips, two slices and three writer changes. It also returned 65 distinct streamed texels across a small-texture heap rollover. | Full-scene faces and streamed materials remain unverified. |
| Stability and frame pacing | Open | Earlier five-minute runs measured 2.85 FPS at baseline and 2.70 FPS on an intermediate build, using the median of interval reports averaging at least 500 draws per frame. | Those runs had incomplete builder support. Later full-builder tests with a five-minute limit ended early, including at 160 and 190 seconds. |

The six remaining driver refusals cover two particle defragmentation pipelines, two effect-building pipelines and two triangle-collection pipelines. The frame-rate numbers above are interval statistics across changing content. The project has no verified same-scene gameplay FPS gain for this GPU.

## RTX pipeline coverage audit

An instrumented opening on the physical RTX 4070 SUPER requested **83,557 distinct pipeline states** and **19,105 distinct shaders**. All of those shaders appeared in the offline inventory. Of the pipeline states, 55,661 were absent from the offline journals used for that comparison. Attachment formats, blending, shader combinations and other draw state contribute to pipeline identity.

| Subsystem | Status | Verified improvement | Next blocker |
| --- | --- | --- | --- |
| Offline discovery | Expanded | The scanner opened 109 files totaling 224,001,114,630 bytes. Two shader databases and a packed pipeline table described 25,471 graphics records and 2,693 compute records. | The databases also contain 195 distinct unsupported geometry-stage containers. Other scenes and opaque archive formats remain unmeasured. |
| Shader translation | Strict wave64 handling | A clean rebuild translated 25,730 of 25,758 recognized vertex, pixel, mesh and compute containers. It exposed 28 compute shaders that previously relied on approximate half-wave operations. | Fourteen divergent reductions, seven cross-half mask exchanges and seven first-active-lane reads require exact lowering. The 195 geometry containers are outside this denominator. |
| Offline compute | Partial | The RTX driver created 2,700 pipelines. The archive separately identified 2,682 distinct compute containers from 2,693 records. | Six submitted shaders require 64-lane execution on a 32-lane device, 28 failed translation first, and four archived records still lack a matching container. |
| Archived graphics | Partial | The engine's effect declarations and the native shader signatures in its executable and libraries identify 25,465 of 25,471 records. The updated driver pass created 25,461 of those records. | Six graphics records lack a matching vertex or mesh container. Four additional records blend into RGB9E5. The RTX reports sampling support for this format and lacks native render-target and blend support. Exact ordered blending is outstanding. |
| Runtime creation and execution | Measured for one opening | Of 83,557 requested states, the runtime recorded host creation for 2,566 and rejection for 18. Commands for 1,683 distinct states were submitted and 1,678 reached a completed fence. | 80,973 requested states had no creation outcome by exit. The command journal recorded 7,716 missing commands across 64 states. A fence confirms queue progress. Pixel values need an independent check. |
| Faces and animation | Execution traced | An effect-name search associated nine face or mouth states and 83 skin or skeletal states with fence-confirmed work. Zero states in those two sets logged a missing command. | Shader inputs, skeletal poses, morph weights, speech timing and final lighting need a synchronized scene capture. |
| Lighting, ray tracing and streaming | Partial | Of the name-matched states, 251 lighting or shadow states, 327 ray-tracing or reflection states and 13 streaming-related states reached a completed fence. | Fifteen lighting or shadow states and four ray-tracing or reflection states recorded missing commands. The association with the visible defects remains unverified. |
| Stability and frame pacing | Opening still unstable | With full RT and a 300-second cutoff, the instrumented process exited at 205.0 seconds. Median reported performance across 12 render-heavy intervals was 0.3 FPS and 3,727 ms/frame. Frame preparation took 2,087 ms, including 1,266 ms in copies. | Shared-buffer allocations failed before the device error. The run provides no verified FPS increase. |

The comparison used the compute warm-up and the 25,450-record graphics precompile recorded for the opening build. The complete runtime journal contained 465,050 ordered events. Subsystem groups use shader and effect names, can overlap and cannot identify a scene's final color or an animation pose. The benchmark harness opened the game's original files read-only and verified the original file manifest after exit.

To record another comparison, give each process its own new audit file. A full warm-up reads the specified directory, translates shaders, and attempts archived graphics and compute states. The report lists requests, compilation results, missing draw or dispatch commands, and completed GPU fences.

~~~powershell
$env:VICTORY_PIPELINE_AUDIT = "$PWD\offline.tsv"
.\victory.exe warm $privateGameDir
$env:VICTORY_PIPELINE_AUDIT = "$PWD\opening.tsv"
.\Play-RTX.cmd
Remove-Item Env:\VICTORY_PIPELINE_AUDIT
.\victory.exe pipeline-report --offline offline.tsv --runtime opening.tsv --details pipeline-coverage.tsv
~~~

The older prerelease results below used a translator that accepted approximate half-wave operations.

## Windows x64 prerelease verification

The packaged programs were tested on the physical RTX 4070 SUPER with driver 617.42. The archive scan followed the file links in the launcher's private game folder. It translated all 25,758 shaders in 329 seconds and built 2,728 compute pipelines. Six compute pipeline creations still failed. The game opened the resulting 501,630,004-byte driver library at startup. Graphics pipelines that depend on the game's draw state still need a first encounter or a previous run on the same driver.

| Test | Settings and sample | Result |
| --- | --- | --- |
| Opening sequence | Full RT requested, parallel preparation, a limit of 32 outstanding lists, and 4 MiB guest-memory windows. The run had a 300-second cutoff. | 0.4 FPS and 2,445.8 ms per frame across 11 render-heavy interval reports, using the median of each metric. The process exited after 200.8 seconds. |
| Memory and frame pacing | In those intervals, the median time preparing a frame was 1,324.3 ms, including 891.0 ms in copies. Driver pipeline waits were 0.0 ms in the last complete counter report. | Guest-memory windows held 5,928 MiB in the last detailed memory report. Shared-buffer allocations failed and the RT builder subsequently lost a binding. Direct3D 12 eventually reported `E_OUTOFMEMORY` when closing a command list. |
| Texture and lighting readback | Physical GPU tests covered a packed RGB9E5 texture with two mips and two array slices, and 65 small textures spanning two allocations. | Every tested value matched. The GPU crate recorded 196 passing tests and three ignored tests. The opening captures did not establish that facial lighting, missing materials, or lip sync were repaired. |

The frame reports above include opening and loading transitions. Each qualifying interval averaged at least 500 draws per frame. Frame time is the median of the reported interval means. Individual frame-time percentiles and same-scene gameplay speedup are still unmeasured.

## Radeon runtime testing, 25 November 2026

## Experimental RT in Chapter 1 City

On an RX 7900 XTX, the runtime reached the outdoor City scene from a copied
autosave with the dynamic structure builder, traversal and lighting resolve
enabled. After GPU completion, a counter on the selected traversal shader
recorded 6,074,512 active node visits and 848 finite, in-range triangle
candidates by frame 1,204. These are cumulative counts for one shader.
They establish that rays intersect scene geometry. Verification of lighting
on individual objects and of every bounce is still open.

The CPU previously declared every mapped guest memory window writable even
when a traversal shader only read through the page directory. The corrected
translator distinguishes guest-memory writes from output to a separate UAV.
Two runs used the same City autosave on the same card, with all three RT
stages and the intersection counter enabled. The sample covers frames 31–80
after the first heavy scene frame in each run.

| City measurement | Before write tracking fix | With write tracking fix |
| --- | ---: | ---: |
| Mean frame time | 3,469 ms | 969 ms |
| Average FPS (50 frames / elapsed time) | 0.29 | 1.03 |
| Median frame time | 3,355 ms | 936 ms |
| 95th-percentile frame time | 4,457 ms | 1,214 ms |
| Guest memory outside registered buffers compared per frame | 12,793 MB | 2,655 MB |
| Wait per frame for guest memory reads | 2,036 ms | 95 ms |
| Draws per frame | 12,583 | 12,520 |

The updated run passed five million cumulative in-range triangle candidates
by frame 1,265. This is one before-and-after measurement on one system.
City can still exhaust the page directory's 1,023 backing-copy slots. A
missing mapping can return zero or lose a write.

## Previous Radeon test results

The Windows diagnostics release at `e061c69032b84ec6e2ec17340469cd689500dafd` built with the pinned toolchain. It includes GPU readback changes from `6a4e2b1` and descriptor allocation changes from `e061c69`.

| Check | Result |
| --- | --- |
| GPU library tests on the D3D12 WARP software adapter | 163 passed, 0 failed, 3 ignored |
| Pixel shaders previously refused in the mission capture | All 11 translated, passed DXIL validation and produced test pipelines |
| Upstream repository asset-policy scan | 182 tracked files passed |

In a preliminary Chapter 1 City comparison, the old and new readback paths used the same checkpoint, shader cache and renderer settings. Across 93 matched indoor presents, frame rate was 3.52 and 4.05 FPS respectively. The 95th-percentile frame time was 432 and 352 ms. Two comparisons of the outdoor camera gave different results, ranging from an 18% loss to an 8% gain. The indoor figure currently has one run of the new build.

The City interior can render with an overexposed doorway and a silhouetted character. The ray-tracing structure builder is disabled by default. Whole-game shader coverage and correct indoor lighting remain open.

## Subsequent shader and pipeline checks

- Before the static sampler fix, the archive scan translated 25,702 shaders and refused 56. Offline tests of those 56 recovered 37 pixel shaders and three mesh shaders. All 40 passed Microsoft's DXIL validator. A synthetic shader run on WARP returned the expected texels for wrap and clamp across four alternating selections.
- A full archive run with the sampler change translated 25,742 of 25,758 shaders. A follow-up test recovered one more pixel shader used for voxel visibility. The corrected translator recognized both halves of a restored wave64 lane mask. Retesting all 56 original refusals produced 41 validated shaders and 15 remaining refusals. The visual effect in the City interior still needs a gameplay comparison.
- On an RX 7900 XTX the original windshield-wiper pipeline failed creation with blending enabled on an integer target. The corrected pipeline created successfully with the same shaders and framebuffer formats. The other active targets retained their blending state and all three write masks were preserved.
- A clean Windows installation built all three programs, resolved all static imports of the tested game executable, and executed a synthetic title through the Start-menu shortcut. Re-running setup preserved existing state.

## City streaming and CPU submission

- Of 2,191 distinct vertex shaders in the tested archive, 39 originally emitted bytecode that failed the Microsoft DXIL validator. Correcting signature order and respecting the declared interpolation validated all 2,191. Eighteen of the corrected shaders rasterize water or its proxy geometry. The 92 shaders selected by temporal and anti-aliasing names also passed validation.
- In the City capture, the native temporal resolve dispatched 320 by 180 groups of 8 by 8 threads. The output was 2560 by 1440. Both bloom and the final composite consumed that output. Temporal blend weights and visual quality across the whole game remain unverified.
- One fixed-camera waterfront capture showed dark materials while thousands of texture slots waited for upload. The queue regression test verifies that new textures get service alongside retries even when the per-pass limit is one. An interior lighting defect persisted after the queue drained.
- In a CPU-only release benchmark of 28,279 small upload reads batched 512 at a time, four workers took 11.67 ms compared with 17.64 ms for one. A separate WARP regression submitted eight interdependent command lists and verified all 128 intermediate texture readbacks with and without parallel prelude recording. These tests measure CPU work and command ordering. Gameplay FPS depends on the scene.

## City save reload and dark objects

During a City save reload, the waterfront house, dock and trees gained detail over 18 seconds. The number of texture views due for upload fell from 10,577 to 5,916 to 36. At the middle sample, shader pipeline creation had already caught up and uploads still averaged 391 MB per frame. The translation cache was cold after a translator change. These measurements include diagnostic captures.

Six seconds before a later patio screenshot, the tabletop item and upright weight plate were already dark in both native TAA outputs. Both objects had similar colors in the final image and the screenshot. That capture did not record the incoming scene color. The same interval averaged 317 skipped ray-tracing structure-builder dispatches per frame, with zero pipelines being built. The game's ray-traced lighting remains incomplete, and the cause of these two objects' color is unresolved.

On an RX 7900 XTX, a standalone DXR 1.1 reflection test hit the first triangle at 2 units, the second at 3 units, and the first again at 3 units. The escaping ray and an outside control ray both missed. All 52 compute shaders from the game's ray-tracing builder also validated and created pipelines on this card. Gameplay needs additional verification of the geometry those shaders build, the rays that traverse it and the resulting lighting.

Five previously refused shadow compute shaders now pass Microsoft's DXIL validator. The runtime created all five compute pipelines on the RX 7900 XTX. Their lighting output still needs comparison in the affected scene.

A second test ran a translated Xbox 64-bit ray-node instruction on the RX 7900 XTX and WARP. Both returned hits at 2, 3 and 3 units across two mirrored triangles, followed by a miss. A closer triangle outside the linked tree was ignored, and a deliberately unmapped guest page produced the expected fault. The GPU read nodes through the runtime's guest-page directory. The test driver used the returned distance and barycentric coordinates to construct each reflected ray between dispatches.

The shader cache now fingerprints 30 translation source files. A controlled change to the host test harness or the standalone intersection tests preserved that fingerprint. Changing either the lowering code or the intersection translator changed it. The revised key requires a fresh translation on first use.

One City texture staging sample spent 846 ms waiting on the global memory mapping lock. The updated staging path takes a lock over the source address range. In a test with a one-second remap, staging an unrelated texture took 77 microseconds. A texture inside the affected range waited for the remap and retained the expected texels.
