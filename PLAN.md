# Design Plan: Private Rust Reimplementation of Noita

## Goal and Scope

The end goal is a private, personal Rust recreation of Noita itself, targeting 1:1 fidelity across its gameplay, systems, content, world behavior, and presentation. Treat this as reverse engineering of observable behavior, not an inspired-by game or a deliberately reduced game. Exact parity may not be achievable, but every system should be brought as close to the chosen reference build as practical, with discrepancies measured and documented rather than silently accepted. The milestones below are an incremental route toward that full-game target, not the final product scope.

Treat the installed game as a black-box behavior oracle. The current reference installation is `C:\Program Files (x86)\Steam\steamapps\common\Noita`. Record the game version/build and settings before collecting observations, since updates can change behavior. Use the running game for controlled experiments, screenshots/video comparisons, and timing observations; do not modify the installation or bypass protections. Keep the project private and build an independent implementation. Do not copy Noita code or assets into the project or distribute proprietary data. Local asset files may be useful as visual reference, but a Steam installation does not by itself grant permission to reuse or redistribute them; use only where separately authorized. Personal use does not automatically settle copyright, contract, or anti-circumvention questions in every jurisdiction; get jurisdiction-specific advice before sharing or publishing anything. This plan does not depend on decompiling the game or extracting its assets.

The full game is a large target. The practical approach is a sequence of measurable fidelity gates, beginning with a small side-by-side comparable scene and expanding until the complete game is represented. A vertical slice is an early validation milestone only; it is not a substitute for the 1:1 end goal.

## What Recent AI-Assisted Game Reimplementations Suggest

The public repository [Minecraft: Vibed Edition](https://github.com/Zytronium/minecraft_vibed_edition) describes itself as a Rust Minecraft clone made mostly with Claude. Its progress log is a useful reality check: a small chunk renderer came first, then terrain, chunks, and biomes; world generation differed dramatically between debug and release builds; larger world sizes initially crashed; camera movement needed repeated debugging; and save/load required focused work. It presents itself as a hobby experiment, not a complete commercial-scale recreation.

I could not verify a specific recent Rust Call of Duty port from repository search, so I do not use one as evidence here. The useful lesson from AI-assisted ports is the development shape: a visible vertical slice can arrive quickly, while behavioral edge cases, performance, persistence, and systematic testing take deliberate engineering.

## Reference Specification Process

Use the installed copy at `C:\Program Files (x86)\Steam\steamapps\common\Noita` as the reference. Choose and record its version/build, platform, resolution, graphics settings, and input setup. Do not mix observations across versions without labeling them. Keep a private behavior notebook with short, repeatable experiments; distinguish directly observed facts from guesses. Screenshots or short captures can document visible outcomes; keep reference media private and outside the reimplementation's distributable assets.

For each system:

1. Define a controlled setup and the smallest input sequence that exposes the behavior.
2. Capture the setup, inputs, timing, visible result, and relevant run conditions. Repeat the experiment to identify randomness or state dependencies.
3. Write the result as a behavioral rule with examples and unresolved questions.
4. Implement the rule behind a narrow API and add a regression test from the observation.
5. Compare a new capture side-by-side with the reference and refine the rule, not just the visual effect.

Do not assume undocumented seed behavior or determinism. If the reference can be configured to reproduce a run, record how; otherwise compare controlled local scenarios and label the remaining randomness.

### Fidelity Axes

- Character controls: acceleration, gravity, collision, jumping, aiming, damage, and death.
- Material simulation: movement, reactions, update cadence, boundaries, and interactions with actors.
- Wands and spells: selection, cast ordering, mana, delays, projectile behavior, modifiers, and triggers.
- World: dimensions, terrain composition, biome transitions, structures, and procedural variation.
- Run loop: starting state, shops/perks or equivalent progression, transitions, death, restart, and persistence.
- Presentation: pixel scale, camera follow, lighting, effects, sound timing, and UI response.

Keep a fidelity matrix with columns for observation, implementation status, test coverage, difference from reference, and confidence. Prioritize core interaction fidelity before broad content coverage.

## Executable Analysis Workflow (Ghidra)

The primary tool for reading `noita.exe` is Ghidra with the GhidraMCP plugin, running headless (no window) and
queried over HTTP. It replaces reading raw disassembly with `re_tools disasm`: decompiled pseudocode is far shorter,
shows control flow directly, and keeps analysis results in one database.

**What is set up**

- Ghidra 12.1.4 (`C:\Users\alfie\Documents\ghidra\ghidra_12.1.4_PUBLIC_20260921\ghidra_12.1.4_PUBLIC`), JDK 21, and
  GhidraMCP 7.0.0 (`C:\Users\alfie\Documents\ghidra\ghidraMCP\ghidra-mcp`).
- Project `noita_files/ghidra/noita.gpr` (git-ignored) containing `/noita.exe`, the repository copy of the executable
  (x86 32-bit, image base `0x00400000`, about 88,700 functions), fully auto-analyzed. Never analyze the Steam install.
- `scripts/ghidra-server.ps1` starts the headless server on `http://127.0.0.1:8089` with that program loaded. It is
  idempotent (reports "Already running" if up), `-Stop` stops it, and logs go to `noita_files/ghidra/server.log`.
  The VS Code task "Ghidra MCP: start headless server" (`.vscode/tasks.json`) runs it when the folder opens.
- The Ghidra GUI and the headless server cannot hold the project at the same time; stop the server before opening
  the GUI, and close the GUI before starting the server.
- `.vscode/mcp.json` registers the MCP bridge, but VS Code does not currently detect it; use HTTP directly.

**How to use it**

1. Check the server: `Invoke-WebRequest -UseBasicParsing http://127.0.0.1:8089/list_open_programs` must list
   `noita.exe`; otherwise run `scripts/ghidra-server.ps1`.
2. Decompile with `GET /get_functions?function=0x709fc0&fields=decompiled_code` (also `signature`, `callers`,
   `callees`, `xrefs`; `functions=` takes up to 20 comma-separated addresses). Request only the fields needed, save
   the response to a file under `$env:TEMP`, and read that file rather than echoing large JSON to the terminal.
3. Navigate with `GET /get_xrefs_to?address=`, `GET /search_strings?search_term=`, `GET /find_functions?name_pattern=`,
   `GET /get_function_call_graph`, and `GET /read_memory`. `GET /mcp/schema` lists all 191 endpoints and parameters.
4. Confirm exact details before porting. The decompiler output is a hypothesis: verify float constants, the order
   of RNG draws, signedness, and branch conditions that affect parity against the disassembly
   (`get_functions` with `include_disasm=true`, or `re_tools disasm`).
5. Record what was learned in Ghidra so later decompiles read better: `POST /rename_function` (old_name, new_name),
   `POST /set_comment` (address, comment), `POST /create_struct` for recovered layouts such as `CellData`, then
   `GET /save_program`. Use the README's names (e.g. `CFireCell::Update` for `0x709fc0`). The Ghidra database is not
   in git, so the README remains the durable record of recovered rules and addresses.
6. For small self-contained helpers (color darkening `0x70e760`, `Random` `0x4abc70`, position hash `0x70acc0`),
   `POST /emulate_function` (function, registers, memory) can run the original code to produce golden test values.

`re_tools` remains useful for RTTI vtable dumps, string cross-references, and quick byte or constant reads.

To rebuild the project from scratch (takes several minutes):

```powershell
& 'C:\Users\alfie\Documents\ghidra\ghidra_12.1.4_PUBLIC_20260921\ghidra_12.1.4_PUBLIC\support\analyzeHeadless.bat' `
  "$PWD\noita_files\ghidra" noita -import "$PWD\noita_files\install\noita.exe" -analysisTimeoutPerFile 10800
```

## First Comparable Slice

Build a fixed camera test cave and implement a player controller plus a small set of observed materials and one wand. Start with a minimal test scene that can answer: does the player move and collide similarly; does a falling material move and settle similarly; does liquid spread consistently; and does one spell alter terrain and actors in the same observable way?

Use a handful of materials, not the whole catalogue. Pick the exact materials and spell from repeatable reference experiments, then document the measured rules. The acceptance test is not merely “looks close”: the same controlled interaction should produce the same categories of cell movement, reaction, damage, and timing within stated tolerances.

## Technical Design

### Recommended Stack

- Rust stable and a Cargo workspace.
- Bevy for windowing, input, audio, UI, and ordinary game entities. Keep the material grid out of ECS; one entity per simulated pixel is the wrong starting abstraction.
- A custom `sim` crate for pixel/material state, update rules, reactions, and deterministic test hooks. Keep it independent of the game framework so it can be tested headlessly and benchmarked alone.
- Serde with RON or JSON for the reimplementation's own content definitions and save schema. These are implementation choices, not claims about Noita's internal formats.
- Criterion for microbenchmarks and property-based tests for simulation invariants.

Prototype low-resolution texture composition and nearest-neighbor scaling early. Benchmark texture upload in the actual selected Bevy version before committing to a rendering path.

### Workspace Layout

```text
crates/
  sim/          material grid, update rules, reactions, test hooks
  content/      reimplementation-owned definitions and save schema
  game/         Bevy app, player, enemies, spells, UI, audio
  tools/        observation fixtures, replay capture, debug overlays
assets/         original placeholder / replacement assets
tests/          controlled behavior scenarios and state comparisons
```

### Simulation Model

- Represent each cell with a compact material ID and only necessary per-cell state. Keep shared properties in material definitions.
- Partition the world into fixed-size chunks; benchmark a 64x64-cell starting point rather than assuming it is optimal. Track active/dirty areas so sleeping world regions do not receive needless full scans.
- Run the simulation on a fixed tick separate from rendering. Measure the reference's visible cadence; begin with a configurable tick rate and compare before locking in a value.
- Make update order and random inputs explicit. Provide a deterministic local mode for regression tests, but do not mistake it for the reference's unknown implementation.
- Define chunk-border ownership and conflict resolution. Stage multi-cell reactions before committing changes so ordering bugs are reproducible.
- Implement observed material categories in dependency order: static solids, granular motion, liquids, gases, then burning, heat, electricity, and chemistry as the comparison plan requires.
- Keep actors separate from material cells. Expose collision, contact, damage, force, and terrain-change APIs to the game layer.
- Save only reimplementation state with a versioned schema. Do not attempt to read or write Noita save files as part of the initial plan.

### Actor and Spell Model

Implement control and collision as tunable, observable systems rather than embedding constants throughout gameplay code. Store measured reference values, tolerances, and test scenarios alongside each behavior. Separate actor state from material state, and define the order in which actor movement, collision, spells, and cell simulation occur each tick.

Represent wand and spell rules as data owned by this project. Model cast ordering, mana, delays, projectile lifetimes, modifiers, and triggers as explicit testable stages. Spell effects call narrow simulation APIs for damage, heat, ignition, force, or terrain changes; never mutate the grid through arbitrary shared access. Add spell combinations only after the base ordering matches observed cases.

## Milestones

### 0. Baseline and Observation Harness

Record the reference version and settings. Create a Cargo workspace, CI checks, a minimal rendered window, a headless simulation test target, and a behavior notebook template. Prototype a single falling material and cross-chunk movement. Capture debug and release timings.

**Exit:** a repeatable reference experiment exists for player movement and falling material; the same scenario is expressed as a local automated test; release profiling is available.

### 1. Material Sandbox

Create a test cave, material brush, pause/step controls, tick display, active-chunk overlay, and state-hash/replay tooling. Add only material behaviors already specified by observations.

**Exit:** each implemented rule has a reference note and regression case; boundary and simultaneous-update cases are covered; a state can be reproduced locally from a captured input sequence.

### 2. Player and Combat Slice

Add player movement, collision, aiming, health, one enemy, one wand, and a small number of spells. Compare movement traces and controlled combat situations against the reference. Log each remaining mismatch.

**Exit:** the same defined scenario can be replayed in both builds, with agreed tolerances for movement, cast timing, collision, and material changes.

### 3. Systems Fidelity

Expand material interactions, wand behavior, enemies, pickups, and environmental effects in dependency order. Use experiments to decide priorities rather than expanding a feature checklist blindly.

**Exit:** major implemented interactions have repeatable tests; unresolved discrepancies are written down and ranked; performance remains measured in release mode.

### 4. World and Run Structure

Implement observed world dimensions and generation characteristics incrementally, then biome transitions, structures, run progression, death, and restart. Use original placeholder graphics and sound while validating behavior. Only consider replacing placeholders with other assets when their use is authorized.

**Exit:** a short run can be completed through its major transitions; generation cases are reproducible where the reference permits it; restart and persistence do not leak state.

### 5. Full-Game Fidelity and Parity Pass

Continue expanding systems and content until the full reference game is represented. Use the fidelity matrix to prioritize camera, effects, sound timing, UI, content breadth, and edge cases. Test against multiple reference scenarios, close discrepancies iteratively, and document remaining gaps. Treat complete 1:1 parity as the target, while reporting honestly where behavior remains approximate or unknown.

**Exit:** all chosen fidelity gates have pass/fail criteria, and remaining deviations are explicit rather than hidden behind a general “working” label.

## Testing and Performance Gates

- Unit tests for every implemented movement, material, reaction, cast-ordering, and resource rule.
- Deterministic reimplementation test: same initial state, test seed, and input sequence yield identical state hashes after N ticks.
- Property tests for invalid material IDs, out-of-bounds writes, duplicated cells, unbounded reaction loops, and save/load round trips.
- Golden scenarios derived from observations: store setup and input sequence, expected outcomes, timing tolerances, and known randomness.
- Integration tests for input-to-cast behavior, actor/material interaction, death/restart, and local save/load.
- Profile release builds from the first simulation prototype. Track p50/p95 tick time, active cell count, dirty chunks, render upload time, and memory.
- Define hardware, resolution, and frame-rate budgets after measuring the first prototype. Keep an explicit simulation headroom target under the fixed-tick deadline.

## AI-Assisted Development Rules

- Give the coding model one bounded system or bug, the relevant behavior notes, invariants, and a test target. Avoid prompts such as “recreate all of Noita.”
- Have the model state assumptions before implementation. Convert uncertain assumptions into observations or tests instead of letting guesses become de facto specifications.
- Require focused tests and benchmarks for behavior or performance changes. Review diffs; compilation is not evidence of behavioral fidelity.
- Keep a known-differences list and ask the model to fix one measured discrepancy at a time. Do not allow a broad rewrite to obscure which behavior changed.
- Keep a human responsible for architectural boundaries, reference observations, and final review. Document provenance and licenses for every dependency and asset.

## Main Risks and Responses

| Risk | Response |
| --- | --- |
| Full-game scope becomes unfinishable | Set fidelity gates; implement one comparable scenario at a time; defer breadth until core behavior is measured. |
| Observations are confounded by randomness or version changes | Pin and record the reference build/settings; repeat experiments; label unknowns and confidence. |
| A visual approximation hides incorrect simulation | Compare controlled outcomes and timings, not screenshots alone; add regression cases for each behavior. |
| Cell update order creates bias or edge bugs | Specify scheduling and conflict rules; test boundaries and simultaneous changes. |
| AI-generated code invents internal facts | Treat all unobserved behavior as a hypothesis; require reference evidence and tests. |
| Asset or code reuse raises legal or distribution problems | Keep implementation independent, project private, avoid circumvention and unauthorized extraction, and seek jurisdiction-specific legal advice before sharing. |

## Research Sources

- [Noita official site and FAQ](https://noitagame.com/): describes its observable premise and interactions, including liquids, gases, sand, rigid bodies, reactions, electricity, and thermodynamics.
- [Noita on Steam](https://store.steampowered.com/app/881100/Noita/): official feature overview for pixel simulation, spell creation, procedural environments, and roguelite structure.
- [Nolla Games: Falling Everything Engine](https://nollagames.com/fallingeverything): official engine page.
- [Minecraft: Vibed Edition repository](https://github.com/Zytronium/minecraft_vibed_edition): AI-assisted Rust game clone and firsthand development notes used for the project-planning observations.

Research checked September 30, 2026. Reference behavior and repository details can change between versions.