# noita-rust

A private Rust port of Noita. [PLAN.md](PLAN.md) is the project brief; [ROADMAP.md](ROADMAP.md) tracks progress
toward the full port. Behavior is ported from the local reference
install (data files, executable analysis, and controlled observation); Noita code and assets are never committed.
**Current state: Milestone 0 (repository and observation harness).** No gameplay yet.

## Reference files

The Steam install (`C:\Program Files (x86)\Steam\steamapps\common\Noita`) is read-only. Copies live in the
git-ignored `noita_files/`, which is the only place reference files may be modified or analyzed destructively.

```powershell
# copy (read-only from the install)
$src = "C:\Program Files (x86)\Steam\steamapps\common\Noita"
New-Item -ItemType Directory noita_files\install\data -Force
foreach ($f in "data\data.wak","noita.exe","_version_hash.txt","_branch.txt","_release_notes.txt") {
  Copy-Item "$src\$f" "noita_files\install\$f" }
# extract data.wak -> noita_files/data/...
cargo run -p noita_data --bin wak_extract
# material display names (optional): the English strings are a loose file, not in data.wak
New-Item -ItemType Directory noita_files\data\translations -Force
Copy-Item "$src\data\translations\common.csv" noita_files\data\translations\common.csv
```

Tests in `crates/noita_data/tests/reference_*.rs` check parsed reference data and skip if `noita_files/` is empty.

## Layout

```text
crates/sim/                  headless material grid (no Bevy); tests in crates/sim/tests/
crates/sim/examples/         text-mode visual harness and tick timing
crates/game/                 Bevy 0.19 window: sim world, reference materials, player with its reference sprite
crates/actors/               character controller and animation stack ported from the exe (headless)
crates/box2d/                Rust port of Box2D 2.3.1 (the engine Noita ships); source fetched from the public repo
crates/noita_data/           readers for reference data: data.wak, materials.xml, sprite XML, sim table conversion
crates/re_tools/             read-only analysis of noita_files/install/noita.exe (vtable, disasm, calls, xref, grep)
crates/probe/                noita_probe: read-only reader for the running game's cell grid (see "Runtime probe")
crates/worldgen/             world generation: herringbone Wang tile maps (more stages to come)
crates/script/               LuaJIT runtime (mlua, vendored) for the reference scripts; wand engine running gun.lua
scripts/ghidra-server.ps1    starts the headless Ghidra server used for decompilation (see PLAN.md)
docs/                        behavior notebook template
noita_files/                 local copies of Noita game files (git-ignored; never commit)
noita_files/ghidra/          Ghidra project with the analyzed noita.exe (git-ignored)
.github/workflows/ci.yml     fmt + clippy + test on Linux and Windows
```

## Commands

Windows requires the MSVC linker (Visual Studio Build Tools, "Desktop development with C++").

```sh
cargo test --workspace                                  # all tests
cargo test -p sim --test sand_rules                     # ported sand rules only
cargo test -p sim --test liquid_rules                   # ported liquid rules only
cargo fmt --all --check                                 # formatting (CI)
cargo clippy --workspace --all-targets -- -D warnings   # lints (CI)
cargo run -p sim --example falling_sand                 # print grid each tick until settled
cargo run -p sim --example tick_timing                  # debug tick timing
cargo run -p sim --example tick_timing --release        # release tick timing
cargo run -p game                                       # Bevy sandbox window (needs extracted noita_files)
cargo test -p sim                                       # sim only (fast; no Bevy build)
cargo run -p re_tools --release -- vtable noita_files\install\noita.exe ".?AVCLiquidCell@grid@@"
scripts\ghidra-server.ps1                               # start headless Ghidra (decompiler) on 127.0.0.1:8089
```

Executable analysis goes through Ghidra first; see "Executable Analysis Workflow" in [PLAN.md](PLAN.md).

### Runtime probe (`noita_probe`)

Reads the running reference game's memory (open for reading only; nothing is written, injected or hooked) to
compare the port against the real simulation. Start Noita, then:

```sh
cargo run -p noita_probe -- status                       # pid, frame, GridWorld pointer, loaded bounds
cargo run -p noita_probe -- grid -32 -32 64 64 --tsv a.tsv   # ASCII map plus one line per occupied cell
cargo run -p noita_probe -- cell 12 -40                  # one cell with raw bytes
cargo run -p noita_probe -- rng                          # frame counter and the global rand() state
cargo run -p noita_probe -- ages -320 -192 640 512       # update-stamp age of moving cells per 64x64 region
cargo run -p noita_probe -- peek 0x35ab2950 0x40         # raw dwords at an address
cargo run -p noita_probe -- scan 0x1017b24               # addresses holding a dword (here: the worker vtable)
cargo run -p noita_probe -- workers                      # every region worker: origin, RNG, full counter, dirty rect
cargo run -p noita_probe -- follow 0 0 128 128 out --frames 120   # a TSV per game frame
```

The image is fixed-base (no ASLR), so addresses are absolute: the game state pointer at `0x122374C` (first dword is
the frame, `+0x10` the `GridWorld`; `+0x14` is a smaller world-info object), the chunk table at `GridWorld+0x508` (512x512 chunk pointers indexed from chunk
256; each chunk's first dword points at 512x512 cell pointers), loaded bounds at `+0x520`, the material names at
`CellFactory+4` (`GridWorld+0x53C`), the cell's `CellData` at `+0x14` (material id at `+0x30`), and the class by
vtable (liquid `0x100BB90`, gas `0x1007BCC`, fire `0x10096E0`, solid `0xFF8A6C`). `status` warns when the object
it finds is not a `GridWorld`; `--grid 0xADDR` overrides it. The offsets are from the decompile and have been
exercised against a mock memory image only; confirm with `status` against the live game first. Dumps taken while the
game runs can tear (cells move during the read); pause the game (Esc) for a coherent snapshot. Engine material ids
do not match the port's `materials.xml` order, so compare by name.

Game controls: A/D walk, W jump/levitate, S crouch, mouse aims (sets facing), Space pause/resume, `.` step one
frame while paused, R reset, 1/2/3 choose the left-mouse brush (sand/water/rock_static), right mouse rock_static,
middle mouse erase. The collapsible debug menu (top left) lists every material by category (Sand, Liquid, Static,
Gas, Fire, Solid, Other); click one to make it the left-mouse brush, scroll with the wheel, and painting is ignored
while the cursor is over the menu. The title shows the frame, brush and remaining levitation energy; frame-time diagnostics log to
the console once per second.
On Linux, Bevy needs `libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev`.

## Established reference facts

Reference build: `_version_hash.txt` = `9dbd52ced019a643169a2db02f46c77f8766c6e5`, `_branch.txt` = `master`,
latest release notes dated Aug 12 2024. Source column: **data** = game data file, **exe** = strings/RTTI in `noita.exe`.

| Fact | Source |
| --- | --- |
| `data.wak`: header `u32 0, u32 count, u32 toc_end, u32 0`; entries `u32 offset, u32 size, u32 name_len, name`; uncompressed; 14,745 files | data |
| `materials.xml`: 468 `CellData`/`CellDataChild` definitions; a child may precede its parent (`skullrock` → `the_end`); `meat_pumpkin` and `rock_box2d` are each defined twice (engine resolution unknown) | data |
| `sand`: `cell_type="liquid"`, `liquid_sand="1"`, `density="6"`, `liquid_gravity="0.9"`, graphics color `ffb89e57`, texture `data/materials_gfx/sand.png` | data |
| `rock_static`: `cell_type="liquid"`, `liquid_static="1"`, `liquid_sand="1"`, `solid_static_type="1"`, `density="10"` | data |
| `cell_type` values in use: `liquid`, `gas`, `solid`, `fire`; `air` has no `cell_type` | data |
| Sand and static terrain are liquid cells with flags, not separate cell classes; cell classes are `grid::CLiquidCell` and `grid::CSolidCell` | data + exe |
| Simulation code lives in `source/grid/gridworld.cpp`, `gridworld_threaded.cpp`, `cells/cellfactory.cpp`, `cells/csolidcell.cpp`; built with VC12 | exe |
| Cells are allocated in 64x64 blocks (`Allocate64x64`); streaming uses 512x512 chunks (`WorldBroadChunk512`); updates are tracked as AABBs (`IMPL_AddUpdateAABB`) | exe |
| Full `CellData` field list (e.g. `liquid_viscosity`, `liquid_flow_speed`, `gas_*_speed`, `solid_*`) is in the exe's reflection strings | exe |
| `noita.exe` is PE32 x86 (image base `0x400000`), unpacked, with MSVC RTTI; `CLiquidCell` vftable `0x100bb90` (41 slots) | exe |

### `CellData` layout (serializer `0x6fbac6..`) and constructor defaults (`0x4ac9f3..`)

| Offset | Field | Default |
| --- | --- | --- |
| `+0x154` | `lifetime` (int) | 0 |
| `+0x15C` | `density` (f32) | 1.0 |
| `+0x160` / `+0x161` / `+0x162` / `+0x163` | `liquid_sand` / `liquid_slime` / `liquid_static` / `liquid_stains_self` (bool) | 0 |
| `+0x164` | `liquid_sticks_to_ceiling` (int) | 0 |
| `+0x168` | `liquid_gravity` (f32) | 0.5 |
| `+0x16C` | `liquid_viscosity` (int) | 50 |
| `+0x170` | `liquid_stains` (int) | 0 |
| `+0x188` / `+0x18C` | `liquid_damping` / `liquid_flow_speed` (f32) | 0.8 / 0.9 |
| `+0x190` | `liquid_sand_never_box2d` (bool) | 0 |
| `+0x10D` | `slippery` (bool) | 0 |
| `+0x8C` | `cell_holes_in_texture` (bool): the cell constructor (`0x7048c0`) builds nothing where the texture alpha is 0 (3 reference materials: `steelmoss_static`, `vine`, `root`) | 0 |
| `+0x104` / `+0x108` / `+0x158` | `durability` / `crackability` / `hp` (int) | 0 / 0 / 100 |

Fire-related `CellData` attributes (`burnable`, `on_fire`, `fire_hp`, `autoignition_temperature`,
`temperature_of_fire`, `generates_smoke`, `generates_flames`, `requires_oxygen`, and the three `on_fire_*_material`
names) are also carried into `MaterialProps`. Absent values take the CellData constructor defaults (`0x4ac88e`):
`burnable +0x8E` and `on_fire +0x8F` 0 (the dword store `+0x8C = 0x100` also sets unidentified byte `+0x8D` to 1),
`fire_hp +0x90` 0, `autoignition_temperature +0x94`
100, `+0x98` 0, `temperature_of_fire +0x9C` 10, `generates_smoke +0xA0` 0, `generates_flames +0xA4` 30,
`requires_oxygen +0xA8` 1; each `on_fire_*_material` name is followed by its resolved ID (convert `+0xAC`/`+0xC4`,
flame `+0xC8`/`+0xE0` default `"fire"`, smoke `+0xE4`/`+0xFC` default `"smoke"`). The flame and smoke spawners
(`0x70f8c0`, `0x70f800`) read `+0xE0` and `+0xFC`. Liquid fire-HP conversion and the fire-cell update are ported (see
the rules below); the generic reaction engine (`0x70e8c0`, `0x70ebf0`) is ported too. The reference `fire` material has
`on_fire=1` and `temperature_of_fire=100`, while `spark` shares `cell_type="fire"` but has `on_fire=0` and
temperature 0; `fire_blue` inherits the former settings. Fire intensity (`CFireCell +0x11`) gates a random neighbor
interaction in `0x70f9e0`, which then dispatches through target-cell virtual methods, so the flag is not a complete
spread rule by itself. The liquid target method (`0x70cdf0`) calls ignition helper `0x70f5e0`, which gates on the
target's `CellData+0x98` before setting its burning flag and intensity. This is distinct from `fire_hp` (`+0x90`),
`autoignition_temperature` (`+0x94`), `temperature_of_fire` (`+0x9C`), and `always_ignites_damagemodel` (`+0x268`);
the `+0x98` field is not serialized: after loading, `0x6fe541` sets it to `100 - autoignition_temperature`. The base-cell
initializer (`0x70f450`) copies `temperature_of_fire` to `+0x11` and initializes
the active flag at `+0x10` from `CellData.on_fire`; the derived `CFireCell` initializer (`0x709ef0`) forces the flag
on, which is why even the non-igniting `spark` cell starts active with intensity 0. The sim now carries that initial
state. Gas and liquid updates run the shared burn hook (`0x70fb10`, `Cell_BurnHook`) for burning cells:
`rand() % 101 < generates_flames` rolls an amount (`rand()`, unused by workers) and a flame spawn (`0x70f8c0`);
with `requires_oxygen` the intensity then drops by 1 (no flame placed) or resets to `temperature_of_fire`, and the
cell stops burning at intensity < 1; `rand() % 101 < generates_smoke` does the same for smoke (`0x70f800`,
material `+0xFC`, no intensity change); a cell still burning then spreads fire (`0x70f9e0`) and marks itself. A spawn
picks `rand() % 3` for the cell up-left, above or up-right, needs it movable and empty, and queues a SetCell without
replace (workers' vfn `0x20` returns null, so the amount stamp on the new cell never happens). The `CellData+0x100`
`ExplosionConfig` roll is ported (see Explosions below). Unported: the spawn's heat aggregates (`worker+0x170/+0x1B8`) and queue
`+0x4D8` (vfn `0x3c`). Static-liquid fire-HP conversion runs only for burning cells;
see the liquid update rule below. ChangeMaterial (cell vt[38], `0x70f360`) returns for the same material, queues a
SetCell (world vfn `0x20`, applied after all regions update) for air, a different cell type (cell vt[1] vs target
`CellData+0x38`), or a burning cell (vt[29] `0x70f5d0` reads `+0x10`) whose target is not `burnable` (`+0x8E`);
otherwise it changes the cell in place (liquid vt[13] `0x70d0b0`: material, static flag from `liquid_static`, color),
keeping its burning state. The gas update's heat block (`+0x2C > 0`: heat falls by `rand() % 3`, is mirrored into the
fire intensity, and may spread fire) is not ported because no code sets a gas cell's heat.

The full serializer field order and offsets (from `lifetime` through reactions) is dumped to
`noita_files/re/celldata_fields.txt` by `re_tools disasm 0x6fba80 9000` filtered on field-name pushes.
The materials reader exposes top-level reaction records in source order, preserving raw attributes and the optional
nested `ExplosionConfig`. `sim::Materials::with_reactions` builds the reference lookup tables from them (see the
reaction rules below). Normalized, inherited material tags are available on `MaterialProps` for selectors such as
`[water]`.

### Reactions (ported in `crates/sim`, recovered with Ghidra)

- **Tables** (`Reactions_BuildAll` `0x709990`, `Reactions_Expand` `0x703790`, `Reactions_Add` `0x7065c0`): each
  `<Reaction>` becomes 0x44-byte records in the normal table (CellFactory `+0x2C`) or, with `fast_reaction`, the fast
  table (`+0x58`), indexed by (input1, input2): the reacting cell and its neighbor. Tag inputs expand to every
  material with the tag, walking CellFactory's name-ordered material map; an output equal to an input tag keeps the
  matched material. `[any_liquid]` is synthesized as liquid and not `liquid_sand` (`0x7061f0`). Identical records are
  not added twice (`0x709e10`). Each input1 material keeps its largest record probability in `CellData+0x228`
  (normal) or `+0x22C` (fast). `probability` is the XML percent times 100 (f32, truncated), compared against
  `rand() % 10001`. `direction` maps top/bottom/left/right to 0..3 (`+0x38`, -1 none). Record fields: `+0x04`
  probability, `+0x08`/`+0x0C` inputs, `+0x10`/`+0x14` outputs, `+0x18`/`+0x1C`/`+0x20` third-cell flag, input and
  output, `+0x28` `req_lifetime`, `+0x2C`/`+0x2D` blob radii, `+0x30` lonely pixels, `+0x31` `convert_all`, `+0x34`
  entity, `+0x3C` explosion, `+0x40` audio. The 4864 reacting pairs of the reference data are checked in
  `crates/noita_data/tests/reference_materials.rs`. The exact expansion nesting when one record has several tags is an
  approximation (first tag field first, recursively).
- **Lookup** (`0x706cc0`, fast `0x706d30`): a pair's list yields its only record, or the record at `roll % count`.
- **Dispatcher** (`Cell_ReactionDispatch` `0x70e8c0`): `roll = rand() % 10001`; if `roll <= +0x228`, step to a
  neighbor with `rand() & 3` (left/right/up/down, `0x4abca0`); if movable, look up (own, neighbor or air) and apply
  the record when `roll <= probability`. Fast reactions (`0x70eaf0`) do the same with the fast table and `+0x22C`,
  returning first when `+0x22C <= 0`.
- **Check** (`Cell_CheckReactions` `0x70f160`, cell vt[35]): a cell that is not burning dispatches; a burning
  `on_fire` material dispatches when `rand() % 101 < 50`. Otherwise it reacts as the `fire` material (CellFactory
  `+0xA4`): `rand() % 10001 <= fire's +0x228`, a random neighbor via worker vfn +4, `rand() % 1025` picks the
  (fire, neighbor) record, and `rand() % (max + 1) < probability` converts the neighbor to output2 and, unless output1
  is fire, the cell to output1.
- **Apply** (`Reaction_Apply` `0x70ebf0`): stop if `req_lifetime > 0` and the cell's age (liquids and fire: frames
  since creation; gases: moves) is lower, or if `direction` excludes the neighbor's side. A third input steps
  `rand() & 3` from the cell (mirrored if it hits the neighbor), must be movable and hold input3, and becomes output3.
  The neighbor becomes output2 (a SetCell when it is empty and output2 is not air) and the cell output1, or air when
  `destroy_horizontally_lonely_pixels` is set and both horizontal neighbors are empty. Changes go through
  ChangeMaterial.
- **Blobs**: a nonzero `blob_radius1/2` (`+0x2C`/`+0x2D`, signed char) replaces that cell's change with a 0x1C-byte
  record queued on the worker (world vfn `0x30`, `World_QueueBlob` `0x72b250`; skipped when restricted and the output
  equals the input). The per-worker flush `0x72abf0` applies queued SetCells (`+0x49C`) and then blobs (`+0x4B4`)
  through `GridWorld` vt[12] `0x71ab50`: unrestricted blobs SetCell (replace) every cell with `dx² + dy² < r²`, row
  by row; restricted ones (`blob_restrict_to_input_material1/2`, `+0x2E`/`+0x2F`) need the center to hold the input,
  change it, then cast `(int)(r·2π + 0.5)` f32 rays of `r` unit steps from angle 0 that convert input cells, pass
  through output cells and stop at anything else. ChangeMaterial on `GridWorld` applies at once. With a
  `cosmetic_particle` (`+0x24`) each converted cell draws two global `rand()`s (velocity `(20r - 10, 10r - 20)`, second
  draw first in x) and creates a speck through `0x716ce0` (life 0.5..1.2 s, no collision, grid check on) at the cell, or
  at `floor(ray position) + 0.5` for rays; the blob then sets swirl 0.25/0.25, acceleration `(0, -5)`, mode 2 and
  `fade_rate = -1 / lifetime`. `World::take_specks` hands them to `Particles::update`.
- **SetCell** (`0x71b140`): queued changes carry a replace flag. ChangeMaterial and lifetime removal replace (the
  occupant is removed first, marking `(x, y-1)`); a reaction output into empty space does not, and is dropped if the
  cell was filled before the flush.
- **Call sites**: static liquids react on half of their 20% full-update rolls (`rand() % 101 < 50`, only with
  `+0x228 > 0`) and otherwise run fast reactions when `+0x22C > 0`, returning before the burning branch; moved
  liquids check reactions on full updates and run fast reactions otherwise (after the burn hook and fire-HP roll);
  gases that did not move do the same at the end of `CGasCell_Move` (`0x70a810`); fire cells check reactions on the
  15/16 of tails that do not remove them.
- **Required reactions** (`<ReqReaction>`, `Reactions_AddRequired` `0x706da0`, per-material list at CellFactory
  `+0x84`, maximum probability in `CellData+0x230`): a record is kept only when its output1 differs from input1.
  The dispatcher (`0x70e8c0`) calls `Cell_RequiredReactions` (vt[37] `0x70ef90`) after its roll when
  `0 < +0x230` and `roll < +0x230`, and a change there ends the dispatch. The candidates are the records with
  `roll <= probability`; each neighbor in the order up, down, left, right removes the first record whose input2 it
  holds (air for an empty cell). A record whose `direction` is another side stops that neighbor's scan. The surviving
  record with the highest probability (first wins ties) changes the cell to its output1. So "grass requires air"
  turns a sealed cell into soil. Fast reactions (`0x70eaf0`) never run them. The freezing/melting rolls in the
  dispatcher read worker bytes `+0x452`/`+0x453`, but no instruction in the exe writes them (byte-pattern search;
  only the dispatcher reads them), so they never fire and are left out.
- **Unported**:
  `convert_all` (world vfn `0x34`; unused by the reference data), entity spawns, audio, and a liquid queue gated by
  `CellData+0x21C`
  (`0x70cae5`, world vfn `0x88`). A reaction's attached `ExplosionConfig` is ported (see Explosions).

### Recovered rules (ported in `crates/sim`)

Cell fields: `+0x18` x, `+0x1C` y, `+0x20` state (1 falling, 2 flowing, 3 resting, 8 fast fall), `+0x21` rest
counter, `+0x22` static flag (from `liquid_static`, 0x70aeac), `+0x23` frames falling (0xFF once landed), `+0x24`
velocity, `+0x28` horizontal flow, `+0x2C` last update frame. The `CLiquidCell` constructor (0x70ae67) starts cells
at state 3 with zero velocity/flow. Empty space is a null cell pointer; a lookup outside the world also yields null.
`y` grows downward (grid index `(y<<9)|x` in 512 chunks, `0x89c2d0`).

- **RNG**: MSVC `rand()` (`s = s*0x343FD + 0x269EC3`, output `(s>>16)&0x7FFF`), state held by the world object;
  `Random(min, max) = min + rand() % (max - min + 1)` (0x4abc70).
- **Update** (`CLiquidCell` vt[21] `0x70c6e0`): static cells (`+0x22`) take a separate path: on full frames,
  `rand() % 101 < 20`, then with `liquid_sticks_to_ceiling > 0` the stuck-cell check below. Burning static cells
  (vt[29]) then run shared burn hook `0x70fb10`; when worker byte `+0x451` is nonzero, `rand() % 100 + 1 < [+0x451]`
  calls cell vt[33] (unported; the byte has no direct writer found). Then, if `fire_hp >= 0`: `rand() % (fire_hp + 1)
  == 0` converts to `on_fire_flame_material` (`+0xE0`) via ChangeMaterial; otherwise `Random(0, fire_hp) < 30`
  and an empty random neighbor also convert. The neighbor comes from worker vfn `+4` (`0x743e80`): `rand() & 3` picks
  left/right/up/down (0..3); if that cell is not movable the opposite one is used; if neither is movable the result is
  empty. Non-burning static cells on full frames call `0x70ad30` (true when all four orthogonal neighbors are movable
  and empty) and, if isolated, ChangeMaterial to air. Other cells: skip if
  resting with rest >= 50 (unless full) or
  already updated this frame; stamp frame; when `CellData+0x170` (`liquid_stains`) is set, call `0x70ce40` (stain a
  neighbor, see "Stains"); a liquid whose age (`frame - birth`) has reached `lifetime` (`+0x154`, when > 0)
  becomes air (ChangeMaterial); sand and slime update the rest counter (rest += 1 while <= 50 if resting,
  else 0); sand runs `0x70b2d0`; slime marks its cell, runs `0x70b7c0`, and marks its final cell; other liquids
  run liquid flow (`0x70bdc0`). Post-movement burning cells call shared burn hook `0x70fb10`, then `fire_hp >= 0` and
  `Random(0, fire_hp) == 0` convert them to the flame material (`0x70cab3`, no exposed-cell roll); the hook's
  no-flame/no-smoke oxygen decay branch is ported.
- **Fire** (`CFireCell` vt[21] `0x709fc0`; the worker has no per-cell frame guard, so a rising fire cell updates
  again in its new row):
  - Spread (`0x70f9e0`), run twice: stop if `rand() % 101 > intensity` (`+0x11`, signed); pick a neighbor with
    `rand() & 3` (left, right, up, down); it must be movable and occupied. Sand-like targets (vt[25]:
    `liquid_sand` for liquids, false for gas/fire) are darkened with factor `rand()/32767 * 0.001 + 0.0045`. If the
    target is burnable (vt[28]: `burnable` for liquids/gases, false for fire) and not burning (vt[29]), it is
    ignited (vt[30]) with the fire's intensity and marked. Ignition (`0x70f5e0`) succeeds when `rand() % 101 <
    100 - autoignition_temperature`, setting the burning flag and intensity. A liquid (`0x70cdf0`) then becomes its
    flame material if burning with `fire_hp == 0`; a gas (`0x70aa80`) becomes its flame material either way.
  - Movement, when `rand() % 101 < 35`: rise into an empty movable cell above (vt[18] `0x70a3b0`, marking the new
    position) and stop. Otherwise stop on a further `rand() % 101 < 35`; else try sideways, first left when
    `rand() % 101 < 50`, then the other side, moving into an empty movable cell and stopping. If an occupied cell
    above is sand-like and `rand() % 101 < 20`, it is darkened by `f = rand()/32767 * 0.15 + 0.2`, then for
    `i = 2 .. Random(1, 6) + 1` the cell at `(x + rand() % 3 - 1, y - i)` is darkened by `f *= rand()/32767 * 0.2 +
    0.6` while movable and sand-like.
  - Tail (without a move): mark the cell; with `rand() & 15 == 0` ChangeMaterial to air, otherwise the reaction check
    (vt[35], unported).
  - Darkening (`0x70e760`): unchanged if the brightest RGB channel is below 19; else each channel becomes
    `max(trunc(c * (1 - f)), 19)` in f32, keeping alpha.
- **Sticky sand** (`liquid_sticks_to_ceiling > 0`, start of `0x70b2d0`): a position hash (`0x70acc0`: `s = x +
  0x67577458 + 91y; a = ((s >> 13) ^ s) * 0x343FD + 0x269EC3; v = ((a >> ((x & 15) + 1)) ^ a) & 0x7FFF`; odd rows
  give `5 - v % 3`, even rows `7 + v % 3`) picks the side: 5 right, 7 left, otherwise up. If that neighbor is sand
  with support type 1 (`0x70ac50`), the cell becomes static with state 5/6/7 (stuck right/up/left). Support type
  (vt[27] `0x70b160`): static = 1; `liquid_sand_never_box2d` or non-sand = 0; sticky sand stuck or settled (state 3,
  rest >= 20) = 1; other settled sand = 2. A stuck cell (static path) comes loose if its neighbor is outside the
  world or no longer holds it (`0x70ac80`: stuck liquid or support type 1), or, when it holds, with chance
  `sticks/101` if the cell below is empty.
- **Slime** (`0x70b7c0`): pick a crawl side (`rand() & 1`: 1 = left first): the first side whose neighbor is
  occupied and whose diagonal-down cell is displaceable sets `dir` and `flow = dir`. If `(x+dir, y+1)` is
  displaceable: wake above, swap; stop if the new cell above holds slime (vt[26] `0x70b120`: static, or settled sand
  unless `never_box2d`); start falling (`vel = 0`, state 1) or accelerate `vel += gravity/4` with extra moves like
  sand but re-rolling `dir` when `rand() & 0x7F < 12`. Otherwise: stop when `liquid_viscosity < 100` and `rand() %
  101 < viscosity`; try diagonals (left first); else creep sideways only towards an empty side when the other is
  occupied or outside the world (`flow += 0.9` per update, step at `|flow| >= 1`, state 2); with both sides occupied
  rest (state 3). Both sides open: no movement.
- **Sand movement** (`0x70b2d0`):
  1. If falling: stall this frame when `rand()/32767 < (1 - liquid_gravity) * 0.5`.
  2. If `(x, y+1)` is displaceable: wake `(x, y-1)`, swap down. If already falling, `vel += liquid_gravity * 0.25`,
     then while `vel - 1 - 2k > 0` make extra moves to `(x+dx, y+1)`; for sand `dx` is kept when
     `rand() & 0x7F >= 64`, else re-rolled from `rand() & 7` (1 → -1, 2 → +1, else 0). A move blocked by an
     occupied cell sets `vel = 0` (an empty but out-of-bounds target keeps it). If not already falling:
     `state = 1`, `vel = 0`. Either way, landing on a moving liquid deletes the mover (below).
  3. Else if `liquid_viscosity < 100` and `rand() % 101 < liquid_viscosity`: try `(x-1, y+1)` then `(x+1, y+1)`.
  4. Else if both horizontal neighbours are occupied: `state = 3`.
- **Displaceable** (`0x70b200`): empty → yes; out of world → no; gas or fire cell → yes (no RNG draw);
  `rand() % 101 < 75` → no; both `liquid_sand` → no; target not a liquid (solid) → no; else mover `density` >
  target `density`. Cell kinds (vt[1]): 1 liquid, 2 gas, 3 solid, 4 fire; vt[22]/vt[23]/vt[24] test liquid/solid/gas.
- **Liquid flow** (`0x70bdc0`), for non-sand liquids such as water:
  1. Fall: same as sand's step 2 without the stall, plus `frames_falling += 1`; extra-move `dx` follows the sign of
     `flow` when `|flow| >= 1`. Extra moves blocked by an occupied in-bounds cell set `frames_falling = 0xFF` and
     `vel = 0`; the same `vel = 0` applies when the cell below was not displaceable to begin with.
  2. If the cell below is not displaceable and is not a liquid still moving down: when exactly one side is open
     (right checked first), wake the opposite side, `flow += ±liquid_flow_speed`, and step that way once
     `|flow| >= 1`, then `flow *= liquid_damping`.
  3. While `|flow| >= 1` and the next cell sideways is displaceable: step, `flow *= liquid_damping`.
  4. If nothing moved and the cell below is blocked: pick `Random(0, 1)` direction, try that diagonal-down then the
     other; set `flow = ±1` on success.
  5. Moved: `rest = 0`, wake the cell below, state 1 (8 if `vel >= 4`) after a downward move, else state 2. Not
     moved: state 3, `rest += 1` up to 5 (so liquids never sleep).
- **Swap** (`0x70cc00`): mover and target exchange slots; mover's rest counter decrements. **Wake** (`0x70abf0`):
  if the cell is within the movable area and occupied, mark it and call its vt[17] (liquids: rest = min(rest, 45),
  `0x70d180`; gas, fire and solid cells: nothing).
- **Falling onto a moving liquid deletes the mover** (sand `0x70b5e8`, liquids `0x70c0c0`): when the blocking cell
  is a liquid (vt[22]) whose last update frame (`+0x2C`) is older than the mover's and whose `vel > 0`, i.e. still
  falling and not yet updated this frame, the mover clears its grid slot (`0x89c0d0`) and passes itself to world
  vfn `0x4C` (`0x72b350`, queue `+0x4F0`). After the worker update (`0x72aa40`) each queued cell goes to
  `GridWorld` vt[19] (`0x71f7d0`), which with no body argument queues it at `+0x5A4`; `0x7183f2` then calls the
  cell's vt[39] (`0x70af50`, releases `+0x3C`) and deletes it. Nothing is marked and no particle is made, so the
  material is lost. Sand checks every blocked extra move and, on its first fall, the cell below its new position;
  other liquids check only when their `vel` was `>= liquid_gravity/4` at the start of the update. The cell below is
  normally updated first (bottom-to-top scans), so this mostly hits streams falling into a region that updates
  later, and cells that stuck mid-fall (static cells keep `vel` and are not stamped). Before this check, liquids
  with `15 < frames_falling < 0xFF` and `liquid_stains_self == 0` add x/y, floating sample total, and integer event
  count to per-worker splash totals (`+0xE0..+0xEC`). The sim records these on the blocked extra-step path and exposes
  per-region draining through `World::take_splash_stats`; the reference audio consumer remains unidentified.
- **Gas cells** (`CGasCell`; fields `+0x18` sideways direction, `+0x1C` last update frame, `+0x20/+0x24` x/y,
  `+0x28` rest counter, `+0x2C` heat, `+0x34` age; `CellData` `lifetime +0x154`, `gas_speed`,
  `gas_upwards_speed`, `gas_horizontal_speed`, `gas_downwards_speed` bytes `+0x194..+0x197`, defaults 50, 100,
  100, 90):
  - Constructor (`0x70a4a0`): direction -1, or +1 when `rand() % 101 < 50`; with `lifetime > 0`, `v = max(int(lifetime
    * 0.3), 1)` and age `= rand() % (2v + 1) - v`. Both draws use the RNG of the world object passed in.
  - Update (vt[21] `0x70a670`): skip if already updated this frame, else stamp it. If `lifetime > 0` and age >=
    `lifetime`: queue a change to material 0 at the cell's position (vt[38] `0x70f360` → world vfn `0x20`,
    `0x72b170`) and stop. Skip if the rest counter is >= 100 on a non-full update. `liquid_static` gases set rest to
    100 and do not move; others stop when `rand() % 101 > gas_speed`, marking themselves only if the cell above is
    swappable. Then the heat/burning hooks (the ported burn hook), then
    movement. A burning gas marks its cell after the hook (`0x70a670`).
  - Movement (`0x70a810`): `rand() % 101 < gas_upwards_speed` → move up if swappable; `rand() % 101 <
    gas_horizontal_speed` → move towards the direction if swappable, else the other way (reversing the direction);
    `rand() % 101 < gas_downwards_speed` → move down only into empty space; otherwise `rest += 1` up to 100. After
    a failed move (or for static gas), a gas with rest < 100 marks itself if the cell above is swappable; then
    vt[35] on full updates or vt[36] when `CellData+0x22C > 0` (burning, not ported).
  - Swappable (`0x70a5a0`): empty → yes; solid or liquid → no; gas of the same material → no; `rand() % 101 <
    75` → no; not a gas (fire) → no; else the target is denser than the mover.
  - Move (vt[18] `0x70aad0`): swap slots; a vertical move resets the rest counter; age += 1; mark the new position
    only. Age therefore counts the cell's own moves (vt[16] returns it; liquids instead return `frame - birth`), so
    gas that cannot move never expires.
  - Queued material changes (worker `+0x49C`) are applied after every worker has updated (`0x728450` →
    `0x72ac60` → `GridWorld` vt[8] `0x71b2d0` → `0x71b140`): with no material and replace set, whatever occupies the
    position by then is removed (vt[17] `0x71b400` → `0x71b480`), marking `(x, y-1)` and `(x, y)`.
- **World update** (`GridWorldThreaded`): the world is split into 64x64 regions (manager `0x7259e0`: shift 6), one
  worker object each with its own dirty rect (`+0x480..+0x48C`), RNG (`+0x4`, seeded with the world seed `^ 0xC`,
  `0x729948`) and full counter (`+0x460`, starting at `global_rand() % 11`, `0x729eb0`). Each frame the scheduler
  (`0x726340`):
  1. keeps workers whose dirty rect is non-empty; when a worker's rect grown by 1 px reaches outside its region, the
     grown rect is passed to `IMPL_AddUpdateAABB` (`0x728a90`, refused above 100 regions), which clamps it to each
     other overlapped region, grows that worker's rect by the clamped corners, and activates it;
  2. sweeps the update window: the window rect (`GridWorld` vfn `0x5c`) is cut into 64x64 tiles (`0x1225d4c`), listed
     in an `std::shuffle` order over the CRT `rand()` (`0x996a20`), and each frame the next tile's region gets its
     full-scan flag and is activated, so every region is rescanned once per cycle even when nothing marked it (this
     is what lets slow reactions and unsettled cells in idle regions fire);
  3. picks one active worker with the global `rand()` (`0x120741C`) and sets its full-scan flag (`+0x46C`, `0x7271cb`),
     so every active region is scanned whole every few frames;
  4. updates the workers on threads (dependency order not recovered). Regions outside the update window (around the
     camera in the real game) are never swept; the port treats the whole world as the window. Measured on the
     running game with `noita_probe`: the window rect is `x, y, w, h` at `GridWorld+0x4A8` (here -192, -64, 559, 370,
     so 9 x 6 = 54 tiles and a 54-frame sweep cycle), and moving cells outside it keep update stamps thousands of
     frames old while those inside are 0-90 frames old (resting sand is only stamped on `full` updates, so some
     inside regions read older).
  The live game keeps one worker per region in a 32 x 32 grid (origins x -1536..1472, y -1024..1472; found by
  scanning for the `GridWorldThreaded` vtable `0x1017B24`, 1024 objects of `0x5C8` bytes). The layout in the port is
  confirmed: RNG state `+0x4`, full counter `+0x460` (cycles 0-10), full-scan flag `+0x46C`, origin `+0x470/+0x474`,
  dirty rect `+0x480..+0x48C`. An empty rect is stored as min = origin + 64, max = origin (the port's emptier
  encoding behaves the same after clamping to the region). Region RNGs are all different now (they started alike and
  advanced), so the world seed cannot be read back from them.
  A worker update (`0x72a150`) scans its dirty rect (or the whole region when flagged), clamped to its own region,
  **bottom-to-top**, each row in a direction chosen by its own `rand() & 1` (1 = left-to-right), calling
  `CLiquidCell::Update(frame, full, worker, grid)`; the rect is reset first (`0x72b040`). Marks (world vfn `0x14`,
  `0x72b080`) grow the updating worker's rect even outside its region: sand stall, wake (the woken cell), swap
  (mover's new position), liquid moved (itself; its wake marks the cell below), liquid resting with `rest < 5`.
  Resting sand does not mark itself, so settled areas are only rescanned by the sweep until disturbed. Every 11th update sets
  `full`, which only bypasses the sand sleep check. Bounds checks (world vfn `0x74`, `0x72b390`) allow moves within
  the region grown by 32 px on each side where that area is loaded (`0x725f40`, rect `+0x550`, copied to `+0x560`).
- **Stains** (`0x70ce40`, called for moving non-static liquids with `liquid_stains != 0`, after the frame stamp): draws
  `rand()` from the region RNG and stops unless `(rand() & 0x7F) >= 0x5F` (about 26% proceed); a second draw `% 3`
  picks the cell left, right or below. The neighbor must be in the movable area, occupied, not a gas or fire cell
  and `stainable` (`+0x8D`, default 1; seven reference materials set 0). A liquid neighbor is stained only when it
  is `liquid_sand`, or when it is the same material and the stainer has `liquid_stains_self` (`+0x163`). The
  stainer's color is `liquid_stains_custom_color` (`+0x174`, hex) when nonzero, else its own. Bits of
  `liquid_stains`: 1 moves the neighbor's color 10% toward 0.7 of its base color (`+0x34`, if different), 2 moves it
  20% toward the stain color, 4 moves it 20% toward `lerp(stain, base, 0.5)` (if different). The blend
  (`0xddcd40`) is f32, truncating per channel, and keeps the second color's alpha. Cells carry a base color
  that stains pull the current color back toward; colors are stored ARGB in the port.
- **Cell color**: `SetMaterial` (vt[13] `0x70d0b0`) stores `GetColor(x, y, CellData)` (`0x7044a0`) on the cell, so the
  color travels with it. `GetColor` returns Graphics `color` (`CellData+0x64`) when there is no texture
  (`+0x88`); otherwise it samples the texture tiled, `tex[x mod w, y mod h]` (`0x7042c0`). With `randomize_colors`
  (`+0x6C`) the coordinates are first replaced by a hash (no reference material sets it; not ported). Graphics
  block: `texture_file` `+0x4C`, `color` `+0x64`, `randomize_colors` `+0x6C` (parser `0x700fb0`). When loading,
  a zero Graphics color is replaced by `wang_color` (`+0x40`, `0x6fae61`); 52 reference materials (steam, lava,
  blood, smoke, the magic liquids, ...) have no Graphics block and rely on it.

### Recovered rules: characters (ported in `crates/actors`)

Parameters come from `data/entities/player_base.xml`; fields it leaves unset use the defaults in
`tools_modding/component_documentation.txt` (copied to `noita_files/install/tools_modding/`). Component layouts (from
serializers `0x9ec700`, `0x69dc00`; privates follow the documented order):
`CharacterPlatformingComponent` `velocity_min/max_x/y` (lens) `+0x48/+0x54/+0x60/+0x6C`, `run_velocity +0x78`,
`fly_velocity_x +0x84`, `jump_velocity_x/y +0x90/+0x94`, `jump_keydown_buffer +0x98`, `fly_speed_max_up/down`
(lens) `+0x9C/+0xA8`, `fly_speed_mult +0xB4`, `fly_speed_change_spd +0xB8`, `fly_model_player +0xBC`,
`fly_smooth_y +0xBD`, `accel_x +0xC0`, `accel_x_air +0xC4`, `pixel_gravity +0xC8`, `swim_idle/down/up_buoyancy_coeff
+0xCC/+0xD0/+0xD4`, `swim_drag +0xD8`, `swim_extra_horizontal_drag +0xDC`, `mouse_look +0xE0`, `mouse_look_buffer
+0xE4`, `keyboard_look +0xE8`, `turning_buffer +0xEC`, `mExAnimationPos +0x134`, `mFramesInAirCounter +0x13C`,
`mFlyThrottle +0x150`, `mSmoothedFlyingTargetY +0x154`, `mJetpackEmitting +0x158`, `mFramesNotSwimming +0x160`,
`mFramesSwimming +0x164`, `mShouldCrouch +0x168`; `CharacterDataComponent` `platforming_type +0x48`, AABB
`+0x4C/+0x58/+0x64/+0x70`, `buoyancy_check_offset_y +0x80`, `fly_time_max` (lens) `+0x8C`, `fly_recharge_spd
+0x98`, `fly_recharge_spd_ground +0x9C`, `flying_needs_recharge +0xA0`, `flying_in_air_wait_frames +0xA4`,
`flying_recharge_removal_frames +0xA8`, `climb_over_y +0xAC`, `is_on_ground +0xB8`, `is_on_slippery_ground +0xB9`,
`ground_stickyness +0xBC`, `mFramesOnGround +0x100`, `mLastFrameOnGround +0x104`, `mVelocity +0x108/+0x10C`,
`mFlyingTimeLeft +0x110`, `mCollidedHorizontally +0x114`; `ControlsComponent` button pairs `mButtonDown*`/
`mButtonFrame*` from Fire `+0x48` (Fire has an extra `mButtonLastFrameFire`), so Left `+0x74`, Right `+0x7C`, Up
`+0x84`, Down `+0x8C`, Run `+0x9C`, Fly `+0xA4`; `mFlyingTargetY +0x12C`, `mAimingVector +0x130`, `mMousePosition
+0x158`. `CellData.platform_type` is `+0x3C` (default -1). Component lookups by type name: `0x915400` Controls,
`0x91C2A0` CharacterData, `0xD0F100` CharacterPlatforming.

- **Player controls** (`PlatformShooterPlayerSystem`, `0xa8d7f0`): a button's frame is stamped on the frame it goes
  down. `mFlyingTargetY = y - 10` every frame. The key config has only `key_up` (W) for vertical movement; the
  platforming code accepts Up or Fly for jumping, and both are driven by W here.
- **Velocity** (`CharacterPlatformingSystem` `0xa7ea90`), in order:
  1. Jump: on ground and (Up or Fly held and pressed within `jump_keydown_buffer` frames): `vy = jump_velocity_y`,
     `vx += jump_velocity_x` if Right held, `-=` if Left held.
  2. Fly held: on the frame after the press, `mSmoothedFlyingTargetY = mFlyingTargetY`. Unless
     `flying_needs_recharge` and `mFlyingTimeLeft <= 0`: with `fly_model_player = 0` (the player),
     `target = clamp(fly_speed_mult * (smoothedY - y) - gravity*dt, -fly_speed_max_up, fly_speed_max_down)` (smoothed
     by 5%/frame only with `fly_smooth_y`), `vy += fly_speed_change_spd * (target - vy)`, snapping on overshoot.
     With `fly_model_player = 1`, a throttle ramps by `-fly_speed_change_spd/60` per frame, clamped the same way,
     and `vy += throttle/60`.
  3. Fly not held: energy `+= fly_recharge_spd_ground * dt` on ground, or `+= fly_recharge_spd * dt` in air once
     more than `flying_in_air_wait_frames` have passed since the Fly press. Not flying: `throttle -= dt * throttle *
     20`.
  4. Drain `dt` of energy while Fly is held, or within `flying_recharge_removal_frames` of the press while moving up;
     only with energy left, `flying_needs_recharge`, and not in liquid.
  5. `vy += pixel_gravity * dt`. Horizontal target `clamp(Fly held ? fly_velocity_x : run_velocity, min_x, max_x)`,
     doubled with Run, signed by Left (priority) / Right. In the air with Fly held: `vx += (target - vx) *
     accel_x_air` (no snap); otherwise `accel_x` with overshoot snap.
  6. Liquid (probe cell at `(int x, int y + buoyancy_check_offset_y)` is a `CLiquidCell` (vt[22]) whose material is
     not `liquid_sand` (vt[25])): `vy = swim_drag * (vy - buoyancy * gravity*dt)` with buoyancy `swim_down` (Down),
     `swim_up` (Fly) or `swim_idle`; `vx *= swim_drag * swim_extra_horizontal_drag`. Tracks frames (not) swimming.
  7. `ground_stickyness > 0`: each axis loses `min(|v| * 0.3, stickyness / 3)`. Clamp only `vy`.
- **Energy clamp** (per-component pass `0xa811b0`): with `flying_needs_recharge`, `mFlyingTimeLeft = min(it,
  fly_time_max)` (the component default is 1000, so the first frame clamps it to 3 s for the player).
- **Facing and animation** (same function, `0xa7f8c4..`): movement `d = pos - mExAnimationPos` (last frame's
  move). `mouse_look`: flip scale.x when the cursor is more than `mouse_look_buffer` behind the facing side;
  `keyboard_look`: flip toward a single held direction; else flip by velocity past `turning_buffer`, at most every
  `turn_animation_frames_between`. Moving means `|dx| > 0.1`. In air: swimming (`mFramesNotSwimming < 6`) plays
  `swim_move`/`swim_idle` after 3 air frames; with Fly held after 10 air frames `fly_move` (movement agrees with the
  aim side) or `fly_move_backwards` → `fly_move` → `walk`, idle `fly_idle` → `stand`; else `jump_up` if rising
  within 20 frames of last ground contact, `jump_fall` if not rising after 4 air frames. On ground: `land` at
  priority 1 after more than 7 air frames; then `walk`/`walk_backwards` (movement vs facing), `run` with Run, or
  `stand`, each with a `_crouched` variant while Down is held (posture changes at most every 10 frames).
- **Animation stack** (`SpriteAnimatorSystem::PlayAnimation` `0xc1bf70`, resolve `0xc1c350`): if the sprite lacks
  the name, use the fallback at priority 0, or fail. A name already on the stack is a no-op. Otherwise insert after
  the last state with priority <= the new one; equal priority replaces it. States carry a follow-up name (priority
  0). `StopAnimation` (`0xc1c1d0`) removes by name. When the sprite's animation finishes (`0xc1b820`), the top state
  is popped and its follow-up, else the finished `RectAnimation`'s `next_animation`, is played; the sprite then
  shows the top state and restarts only when the name changes.
- **Slippery ground**: collision copies the hit cell's `slippery` (`CellData+0x10D`) into `is_on_slippery_ground`
  when stepping up onto it or bumping a ceiling; the flag is cleared each move. On it, `vx += (target - vx) * accel_x
  * (0.15 steering, 0.085 coasting)`, then `+1.5` if only the left foot cell `(x + aabb_min_x, y + aabb_max_y + 1)`
  is occupied, `-1.5` if only the right. Animations treat held direction as moving; releasing into a slide (`|dx| >
  0.2`, ignoring a direction pressed this or last frame) plays `slide_start` then `slide`, and stopping plays
  `slide_end` (priority 1) then `stand`.
- **Collision movement** (platforming_type 2, `0xa659c0`): clear ground/wall flags, then `MoveToward`
  (`0xa64850`) steps toward `pos + vel * dt` at most 1 px at a time through `TryMove` (`0xa64460`), stopping at the
  first failure. After moving, a grounded character moving down gets `vy = 60`, unless it has a platforming
  component with `mFramesNotSwimming <= 0`. Then `mFramesOnGround`/`mLastFrameOnGround` update (`0xa65d76`).
- **TryMove**: if the AABB at the new position hits a blocking cell (first in row-major order over the floored AABB,
  `0x640270`): (1) moving down and the hit is within `climb_over_y` of the feet: rise above it if clear, set
  `is_on_ground`; (2) moving up and the hit is at the head: `vy = 0`, push below it; (3) wall contact on the moving
  side: `vx = 0`, push out, set `mCollidedHorizontally`; otherwise fail. Consequence (reproduced, not a port bug):
  walking into a wall taller than `climb_over_y`, the row-major search finds the wall first, the step-up test fails,
  and the wall push-out overlaps the floor, so the move fails and `is_on_ground` stays false for several frames at a
  time (109 of 240 frames grounded in `cargo run -p actors --example wall_trace`). A W press in those frames starts
  levitation instead of a jump.
- **Blocking cell** (`0x63ef70`): collision type (`CLiquidCell` vt[11] `0x70b0b0`) is 1, i.e. `liquid_sand` and
  (static or resting) and `platform_type != 0`; and one 4-neighbour is also type 1. Isolated pixels and non-sand
  liquids never block.

### Recovered rules: particle cells (ported in `crates/sim`, `sim::particles` and `sim::noise`)

`grid::GridParticles` (GridWorld `+0x540`) moves flying cells and specks, then lands them again. Records are 0x70 bytes.

- **RNG**: a global Park-Miller generator (`0x1222388`, 16807 mod 2^31-1, state kept in a double; `0x44d0e0` float draw
  `min + (float)(state * 4.656612875e-10) * (max - min)`, `0x44d140` int draw). Allocating a record (`0x712010`) draws
  once and ignores the result.
- **Defaults** (`0x712010`): alpha 0.5, acceleration `(0, 10)`, density 1000, 20 bounces, lifetime 1, attractor -1,
  `+0x6B = 0x83` (has cell, collides, bit 7).
- **Integrator** (`0x713e20`): age++; alpha += `+0x4C * dt` clamped to 0..1; attractor pull
  `vel += (target - pos) * dt * strength`; acceleration (rising mode 1 uses a quarter of it); swirl
  `amp > 0`: `n = simplex3(x' * f, y' * f, time)`, `a = (float)n * pi`, `vy += cos(a) * amp`, `vx -= sin(a) * amp`
  with `(x', y')` the new position; damping `v -= v * d * dt` (flag `0x40` and `|v|^2 < 0.01` kills); lifetime (flag
  `0x20`); a rising particle that is not from a cell and falls becomes normal; mode 2 teleports; otherwise the collision
  stepper runs and the trailing position is `new + normalize(old - new) * speed * dt`.
- **Simplex noise** (`0x872560`): Gustavson 3D with the Perlin permutation at `0xFDF530`, 12 edge gradients,
  `0.6 - |d|^2` falloff and a `32.0` scale (the 0.6 radius leaves tiny seams of about 3e-4).
- **Collision stepper** (`0x712390`): dies outside the update window; without the collide flag it just moves; else it
  walks up to 60 unit steps from the last visited cell, looking at the material in each new cell. Empty cells and gas
  or fire pass (a rising particle stops being rising). Liquid or solid cells: a rising particle passes only while it is
  lighter (cell particles: or the same material) and loses a bounce; a normal particle with the bounce flag (30% of
  cell particles; always for material specks) reflects along the dominant axis, adds `float(-10, 10)` when `|vx| < 10`,
  scales by `float(0.1, 0.3)`, may reroll its lifetime (`float(0.5, 1.5)` when above 1 and `|v|^2 < 1600`) and loses the
  bounce flag below `|v|^2 = 100`; otherwise it dies. A particle that dies stays at its last free position.
- **Creation**: `AddAsParticle` (`0x7168c0`) copies the cell's color and density, centers on the cell (`+0.5`), draws
  once for the 30% bounce flag and, when rising, sets density 0 if `vy > -50` and with 50% chance. `0x716ce0` makes a
  cell-less speck with random lifetime `float(0.5, 1.5)`, flags `0x20|0x40`, and clears bit 7 with 75% chance.
  `0x717670` turns a movable liquid grid cell (not static, not sand) into a rising particle.
- **Dead pass** (`0x716600`): cell particles older than 5 frames inside a hittable box die; dead particles are removed
  by moving the last record into the gap; a particle with a cell sets the cell position to its truncated position and
  hands it to the landing queue (deadline arming when `+0x6C & 2`).
- **Landing** (`0x71a590`, `0x71fde0`, `0x71fba0`): handed-off cells go to an immediate list. The delayed list is retried
  from a random start within a budget (max 300 when an immediate list exists, halved down to 20 on failure, 500 after
  any success; entries past their deadline are discarded; at most 100000 kept). Landing scans up to 128 rows upward from
  the cell's position for an empty cell or a lighter non-solid one (the latter is displaced and re-queued); a solid cell
  ends the scan; blocked cells more than 5 rows up drift a column (`rand() % 101 < 20`, then `< 50` picks right).
  Failed immediate cells become delayed; displaced cells become the next immediate list.
- Not ported: render lists, hit effects (audio, gore), the spark list, force rectangles, the `liquid_stains == 2` hook
  (`0x715ca0`), the camera-local snapshot grid (the live world is read), and the joint-break caller (`0x719670`).

### Recovered rules: explosions (ported in `crates/sim`, `sim::particles::ExplosionConfig` / `Particles::explode`)

`ConfigExplosion` is `0x657960` (18 KB; decompile it with `/force_decompile`). Only its cell effects are ported.

- **Config** (`ExplosionConfig`, serializer `0x4bb980`, constructor defaults `0x4b2780`): `+0x08` radius 20, `+0x78`
  hole 1, `+0x79` destroy non-platform solids 1, `+0x8C` `hole_destroy_liquid` 0, `+0x8D` `hole_destroy_physics_dynamic`
  1, `+0x90` `create_cell_material` `fire`, `+0xA8` its percent 5, `+0xB0` `spark_material` `spark`, `+0xC8`..`+0xDA`
  material sparks, `+0xDB` sparks 1 with counts 7..20 and inner radius 0.333, `+0xE8` stains 1, `+0xEC` stain radius 0
  (= 0.4 radius), `+0xF0` `ray_energy` 20000, `+0xF4` `max_durability_to_destroy` 10, `+0x10C..+0x120`
  `cell_explosion_power/radius_min/radius_max/damage_required/probability` (1 / 5 / 150 / 100 / 0). Cells carry `hp`
  (`Cell+4`, from `CellData+0x158` `hp`, default 100) and materials a `durability` (`+0x104`, default 0).
- **Triggers**: a burning cell whose material has an `ExplosionConfig` (burn hook `0x70fb10`, only while no explosion is
  queued) rolls `(rand/32767) * min(damage_required + 0.5, 100) >= damage_required`, then radius
  `min(sqrt(power * (rand/32767 * 0.5 + 0.75)), 150)`; a reaction with a config (`0x70ebf0`) rolls
  `rand/32767 < cell_explosion_probability` and a radius in `[radius_min, radius_max]`. Both queue the config copy
  (`never_cache` cleared) on the worker (`0x72b3d0`); the flush (`0x72abf0`) resets the counter. The dispatcher
  (`0x657410`, `Particles::run_explosions`) runs one new explosion per frame immediately and defers the rest to the next
  frame start (`0x657300`) unless `never_cache`; delay ids and timers are not ported.
- **Rays** (`0x656b10`): 360 rays from the center toward `center - R (cos t, sin t)`. A ray steps one cell at a time
  (at most 0x800); only sand (`liquid_sand`) and solid cells interact: `take = min(hp, energy)`, the ray's absorb value is
  the largest take, and it stops when the energy is gone or the cell's durability exceeds `max_durability_to_destroy`
  (consuming one global `rand()`, whose parity adds 1 to the returned squared distance).
- **Cell loop**: after a one-time 360-entry stain table (`0x12059f0`, 360 `rand()` draws) and per-explosion draws for a
  stain offset (`% 181`) and a jag phase, every cell of the square of half-width `(int)(R + stain)` draws three `rand()`s
  (velocity jitter x, y and speed `100..200`) whatever it holds. Its ray index is
  `clamp(floor(atan2(dy, dx) * 57.2958 + 179 +- 0.5), 0, 359)`; explosions above radius 60 shorten each ray by
  `sin(...)^2 * min(0.35 * len2, 100)`. An empty cell inside the ray may be filled with `create_cell_material`. A
  sand or solid cell outside the hole (or beyond its ray) within the stain radius is darkened by
  `(0.5..0.7) * table * t^2` when its material is stainable (`Color_Darken` `0x70e760`). A cell inside the hole loses
  the ray's absorb value from `hp` (if durable enough); at `hp < 1` sand, solids and (with `hole_destroy_liquid`) liquids
  are destroyed: a 5..10 s material speck with the cell's color is thrown, then the cell becomes `create_cell_material`
  (`create_cell_probability`), or `explosion_dirt` (sand, global `0x1207028`, 0 in the image), or is removed (waking the
  cell above). Other liquids are not removed: `create_cell_probability` may replace them, else they become rising
  particles with upward velocity (`0x717670`).
- **Sparks**: material sparks per destroyed cell (`+0xD8`, off by default; the "real" cell variant is not ported), and
  the `sparks_enabled` block places `spark_material` specks (velocity 4 * offset, life 5..10 s) where the ray reached.
- Not ported: entity damage, knockback, physics bodies, cracks, electricity, light, camera shake, audio, pixel sprites,
  the gore branch (`CellData+0x1C4`, unidentified), vegetation shake and the stats vector; their `rand()` draws are
  therefore missing, so RNG streams only match the reference up to the first such effect.

### Recovered rules: script RNG (ported in `crates/sim`, `sim::random`)

The `Random` class is one `double` holding a Park-Miller state. The game has two global instances, both zero before use:
`0x1222388` (particles, `0x44d0e0` / `0x44d140`) and `0x1221d18` (the `Random*` script functions, plus engine rolls such
as the Sampo orb `Random(0, 999) == 999` in `0x78e640`).

- **Step** (`0x485a60`): `s = (int)state; v = s * 16807 - (s / 127773) * 0x7FFFFFFF` in wrapping 32-bit arithmetic,
  `+0x7FFFFFFF` when `v < 1`. Draws: unit `v * 4.656612875e-10` (double); float `(float)(v * 4.656612875e-10) *
  (max - min) + min` (`0x485ab0`); int `min - (int)((max - min + 1) * (v * -4.656612875e-10))` (`0x485b10`).
- **`SetRandomSeed(x, y)`** (`0x7c9300` -> `0x485890`): the seed is `world_seed (0x1205004) + NEW_GAME_PLUS_COUNT
  (0x1205024)`. With `t = seed ^ 0x93262E6F`: `a = (t & 0xFFF) + x`, `b = ((t >> 12) & 0xFFF) + y`;
  `ia = (int)(a * 134217727)` (CRT double-to-int64, low 32 bits, `0xdfdfa1`);
  `ib = (int)((|b| < 102400 && |a| > 1) ? (b * 3483.328 + (uint)ia) * b : b * 134217727)`;
  `h = mix(ia, ib, seed)` where `mix` is Bob Jenkins' lookup2 96-bit mix returning `c` (`0x8720c0`);
  `state = (uint)h / 4294967295 * 2147483639 + 1` (halved if it reached 2147483647), then one step plus `seed & 3` more.
- **Lua functions**: `Random()` / `Randomf()` return `(float)(v * 4.656612875e-10)`; `Random(a)` is `int(0, a)`, `Random(a,
  b)` is `int(a, b)`; `Randomf(a)` is `float(0, a)`, `Randomf(a, b)` is `float(a, b)`. `ProceduralRandom*` seed a stack-local
  generator for `(x, y)` (the shared state is untouched): 2 args give the unit double (`ProceduralRandomi` rounds it half
  away from zero, i.e. 0 or 1), 3 args use `min = 0`, 4 args `(min, max)`; `ProceduralRandom` with 3 args is an int and
  with 4 args a float, `Procedural...f` always float, `...i` always int.
- **`RandomDistribution`** (`0x7ca080`, `0x844100`): uniform when `sharpness == 0`. Otherwise `m = (mean - min) / (max -
  min)` (float) and up to 100 rejection rounds: candidate `x` and roll `r` are two consecutive draws (the global keeps
  only the second step), accept `x` when `(1 - |x - m|) * baseline > r`, or when `|x - m| < 0.5` and
  `(float)pow((float)sin((0.5 - m + x) * 3.1415), sharpness) >= r`; after 100 rejections one more draw is returned.
  The int variant returns `roundf(x * span) + min`, the float variant `x * span + min`. Defaults: sharpness 1,
  baseline 0.005.
- `set_seed` is derived from the disassembly and not yet compared against a running game; the world-seed generator
  (`0x883540`, time-seeded) and the entity-side users of these draws are not ported.

### Recovered rules: Wang tile maps (ported in `crates/worldgen`, `worldgen::wang`)

Each biome's terrain starts as a herringbone Wang tile map. The exe compiles Sean Barrett's public-domain
`stb_herringbone_wang_tile.h` (source: nothings/stb) with these differences: the scratch arrays `c_color` / `v_color` /
`h_color` live inside the tileset struct (pitch 106 / 105 / 106), `STB_HBWANG_RAND()` is a per-thread `Random` and the
limit is 100x100 tiles (`"increase STB_HBWANG_MAX_X/Y"`, string `0xfea8c0`).

- **Functions**: `stbhw_generate_image` `0x866b90`, `stbhw_build_tileset_from_image` `0x867250` (count helper
  `0x8663b0`), `stbhw__choose_tile` `0x866970`, `stbhw__change_color` `0x866a80`, blitters `0x866830` / `0x8668d0`.
  All match the stock source (edge branch first in the exe; the corner branch includes the 3x2 repetition reduction
  with `weights == NULL`).
- **RNG** (`0x866120`): `rand() = (int)(next * 4.656612875e-10 * 2147483645)` on a `Random` built with the double
  constructor `0x44d070` (state = seed, halved once if >= 2^31-1, then one step). The seed lives in `0x1207018`;
  setting it frees the thread's generator so the next draw starts a fresh stream. A seed of 0.0 keeps the old stream.
- **Template header**: the last 9 bytes of the image's first row, read backwards and XORed with `i * 55`; `header[7]
  == 0xC0` means corner type (`num_color[4]`, `vary_x`, `vary_y`, `short_side_len` at 0..6), otherwise edge type (6 colors,
  then `vary_x`, `vary_y`, `short_side_len`). Tiles are `2n x n` and `n x 2n` blocks inside a 1-pixel border, pitch `n + 3`.
  Images are loaded as RGB (3 channels). All 31 corner and 2 edge templates in `data/wang_tiles` parse (coalmine: corner,
  colors 1/2/1/2, side 13, 72+72 tiles).
- **Biome wrapper** (`0x867770`, `0x867500`): builds the tileset, seeds, generates `height + 4` rows and discards the first
  four rows; pixels are stored as ARGB with alpha 255 into the biome image. The biome driver (`0x879bf0`) retries up to
  100 times with a new seed drawn from its own Park-Miller chain (biome seed `+0x4C`) until the validity check `0x8771a0`
  accepts it; then every non-black pixel looks up its color in a spawn-function table (`FUN_00876aa0`) with a draw per
  pixel.
- Not ported: the biome driver chain (seeds, retries, validity check), the color to material / spawn mapping (`0x87a900`),
  the biome map, pixel scenes, background layers.

### Recovered rules: Lua runtime and wand engine (ported in `crates/script`)

The exe embeds LuaJIT (`lua51.dll`; scripts use `bit.band`). Scripts load through a virtual file system rooted at the
data folder; `dofile_once` is Lua code inside the exe (string `0x1028ac8`, copied verbatim, plus a `do_mod_appends` hook
that is a no-op without mods). Script API functions are native handlers registered one by one (registration table at
`0x7ea410` for the random functions, `0xb294a0` for the gun functions).

- **Wand engine**: the reference `data/scripts/gun/gun.lua` runs unchanged; the C++ half is only a command recorder.
  `BeginProjectile` / `EndProjectile` / `BeginTriggerTimer|HitWorld|Death` / `EndTrigger` / `SetProjectileConfigs` append
  records (0x25c bytes at `0x12093f8`) in call order (`0xb28160`, `0xb282f0`, `0xb283e0`, `0xb28510`, `0xb28600`,
  `0xb286f0`, `0xb287e0`); `RegisterGunAction` (`0xb27ff0`, 65 positional fields in `gunaction_generated.lua` order)
  stores the modifiers of the shot just drawn; `StartReload` (`0xb28900`) stores the reload time; `ActionUsed`
  (`0xb289f0`), `OnActionPlayed` (`0xb28b10`, progress flags `action_<id>`), `LogAction`, `BaabInstruction`,
  `OnNotEnoughManaForAction` are bookkeeping. `ActionUsesRemainingChanged` (`0xb28930`) returns true unless the game
  state check `0xc4a870` fails. The defaults of `ConfigGun` / `ConfigGunActionInfo` live in the generated Lua files.
- The C++ side drives `gun.lua` through `_set_gun` / `_set_gun2` (values in `__globaldata`), `_clear_deck`,
  `_add_card_to_deck(id, inventoryitem_id, uses_remaining, is_identified)`, `_play_permanent_card`, `_start_shot(mana)`,
  `_draw_actions_for_shot(can_reload)` and `_handle_reload()` (returns the mana left). `script::gun::Gun` wraps these and
  returns the recorded `GunCommand` list; tests cover single spells, modifiers, draw-many, triggers, mana, reload time and
  the frame-seeded shuffle against the real `gun_actions.lua`.
- Not ported: turning the commands into projectile entities (needs the entity/component layer), the progress flags,
  per-item use bookkeeping, and every other script API function (`EntityGetFirstComponent`, `GameGetFrameNum` is a
  settable counter, ...). Spell actions that call those will fail until the functions exist.

### Not yet ported from these functions

- Characters: knockback / precision jumping (`+0x140..`), the slippery game effect `0x16`, the crouch ceiling
  sensor, AI `mJumpVelocity`, game effects that gate flying
  (`0x51`), jetpack particles and audio, liquid splash events, `effect_hit_ground`, the Lua-visible
  `mLookOverride*`, the player arm/cape/hotspots, and the system order within a frame (local order: controls,
  platforming, collision, energy clamp).
- Sprites: the rect-animation frame timing (locally a frame lasts `frame_wait` seconds of 1/60 s ticks and
  non-looping animations hold their last frame when finished).
- Cells: splash-stat audio consumption (the sim records worker totals `+0xE0..+0xEC`); burning hooks; the
  seed of the world RNG used to construct cells outside a worker update; the reaction pieces listed under Reactions;
  solids; the order in which regions update (the
  threaded dependency scheduling in `0x726340`), which regions count as loaded, and the global RNG's seed; the
  simulation rate.

## Design choices (local, not claims about the reference)

- `sim` has no dependencies and knows nothing about rendering, so it can be tested and profiled headlessly. Material
  tables are passed in; `noita_data::sim_materials` builds one from `materials.xml`.
- The world is a rectangle of square chunks (tests use small chunks; timing uses 64x64). Cells are stored per chunk;
  `get`/`set` take world coordinates. `get` returns `None` and `set` returns `Err(OutOfBounds)` outside the world.
- `step()` runs the recovered scheduler over every region of the world (all counted as loaded; the margin test is
  per axis against the world edges). Regions update in four checkerboard passes (even/even, odd/even, even/odd,
  odd/odd), each in active-list order, standing in for the unrecovered thread ordering; the global RNG is seeded
  with the world seed. `World::set` marks the placed cell in its region; `mark_area` goes through
  `IMPL_AddUpdateAABB`. A new world scans every region in full once. Queued removals are applied after all passes,
  region by region in active-list order. Gas cells placed with `World::set` draw their constructor rolls from a
  separate RNG seeded with the world seed. Tests that exercise cell rules independently of
  scheduling use `run_active`, which marks everything each frame. `cargo run -p sim --example stuck_water --release`
  reports liquid left hanging over air (none once the recovered sharing and random full scans are in).
- `World::state_hash()` is FNV-1a over dimensions, all RNG states and per-cell state in world row-major order.
- `game` runs `sim` on Bevy's fixed timestep (60 Hz, unrecovered) and rewrites the whole RGBA texture on change,
  drawing each cell's stored color (from its material texture or flat color) alpha-blended over a placeholder
  background. Textures are decoded at startup from `noita_files/data/materials_gfx/`.

## Local timings (tick_timing, 512x512 world, 64x64 chunks, ~43.7k sand, 300 ticks)

Measured 2026-09-30, rustc 1.98.1. Full-world scan every frame (no dirty rects yet):

| Rule | Profile | p50 | p95 | max |
| --- | --- | --- | --- | --- |
| placeholder (straight down) | debug | 23.3 ms | 28.6 ms | 49.6 ms |
| placeholder (straight down) | release | 0.60 ms | 0.80 ms | 1.13 ms |
| ported sand | release | 4.45 ms | 5.29 ms | 6.95 ms |

Use release builds for any performance judgement. These timings are the local baseline only; they aren't compared with the reference.

## Open questions

- World-level update order across 64x64 blocks and threads (`GridWorldThreaded`), and the simulation rate.
- How does the engine treat the duplicate `meat_pumpkin` / `rock_box2d` definitions? Material id order?
- Player movement: not yet located in the exe (`platformshooterplayer_system.cpp`, `playercollision_system.cpp`).

## Pending reference experiments (manual; record each with the notebook template)

Record the build first: the Steam build ID and in-game version string, resolution, graphics settings, and input
device. Capture with screen recording at 60 fps or more, at native resolution, without mods. Measure by stepping
through frames against a fixed landmark, because the camera moves.

1. **Sand fall cadence.** Find loose sand in the first area and remove the support under it with a digging spell.
   Over the first 30 frames of the fall, record the pixels fallen per frame. Also record whether grains slide
   diagonally when they land, and whether left and right sliding look symmetric. Repeat 3 times.
2. **Walk from standstill.** On flat ground, hold right for 2 s, then release. Record horizontal pixels per frame
   until top speed is reached, and after release until stopped. Repeat for left.
3. **Jump.** Tap jump from standstill on flat ground. Record the apex height in pixels, the frames to apex, and the
   frames to landing. Repeat with jump held.

These confirm ported rules against the running game (e.g. the 5% stall and `+0.225`/frame acceleration for sand) and
give walk/jump targets until the player controller is recovered from the exe.

## Milestone 0 status

| Item | Status |
| --- | --- |
| Cargo workspace, CI (fmt/clippy/test) | done |
| Headless sim crate, sand rule ported from the exe, boundary + cross-chunk tests | done |
| Behavior notebook template | done: [docs/behavior-notebook-template.md](docs/behavior-notebook-template.md) |
| Debug/release tick timing | done: `tick_timing` example (numbers above) |
| Text-mode visual harness | done: `falling_sand` example |
| Minimal rendered window (Bevy) with reference materials | done (confirmed running by the user) |
| Reference build recorded; data.wak extraction; materials.xml parsed and checked | done |
| Reference experiments for falling material and player movement | **not started** (sand is ported; capture confirms it) |
