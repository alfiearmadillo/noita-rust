# Roadmap: current state to full 1:1 port

Living checklist; update it in the same change that moves an item. Design and process live in [PLAN.md](PLAN.md);
recovered rules and addresses live in [README.md](README.md). Every item follows the same loop: recover the behavior
from the exe/data, port it, test it, then compare against the running game (notebook template).

Status: `[x]` done, `[~]` partly done (see note), `[ ]` not started.

## Done so far

- [x] Workspace, CI, headless `sim` crate, observation notebook template
- [x] Reference file workflow: `noita_files/` copies, `data.wak` extraction, component and Lua API docs, `noita_dev.exe`
- [x] RE tooling (`re_tools`: vtable, disasm, calls, xref, grep, str)
- [x] Ghidra headless decompiler over HTTP (`scripts/ghidra-server.ps1`); the primary way to read the exe, see
  "Executable Analysis Workflow" in [PLAN.md](PLAN.md)
- [x] Materials: `materials.xml` parsing with inheritance, textures, cell colors

## Phase 1: Simulation core (material sandbox)

1. [~] Liquids: sand, liquid flow, slime, sticky sand (`liquid_sticks_to_ceiling`), displacement, swap/wake, deletion
    onto a still-moving liquid (world vfn `0x4C`), splash aggregates, and the no-emission oxygen-decay branch for
    active liquid cells ported. Burning-liquid fire-HP conversion to the flame material (static and moved cells) and
    removal of isolated non-burning static cells on full updates are also ported, as are the reaction call sites.
    Missing: audio consumption and the `CellData+0x21C` queue (`liquid_stains` is ported).
    Liquid `lifetime` (age >= lifetime becomes air, checked after the frame guard) is ported.
2. [~] Gases: rising, sideways and downward movement, gas-gas density swaps, liquids falling through gas, lifetime
    (counted in moves) with deferred removal, and oxygen-driven intensity decay for active gases with no flame/smoke
    outputs ported. Missing: the construction RNG seed for
    cells placed outside an update.
3. [~] Fire: fire-related `CellData` fields reach the simulation table; construction initializes the burning flag and
    intensity from `on_fire` / `temperature_of_fire`, with `CFireCell` forcing its active flag. The no-emission oxygen
    decay branch is ported for gas and liquid cells, and burning-liquid fire-HP conversion is ported. Fire-cell updates
    (`0x709fc0`) are ported: two spread attempts (`0x70f9e0`: darkening sand-like neighbors and igniting burnable
    liquids/gases via `100 - autoignition_temperature`), rising and sideways movement, darkening sand above, and the
    1/16 removal, and the reaction check on the other 15/16. The shared burn hook (flame/smoke emission, oxygen
    intensity, spread) is ported. Missing: the hook's particle-effect roll and heat aggregates, and
    heat, and ignition of solids/bodies.
4. [ ] Solids and rigid bodies: `CSolidCell`, Box2D-equivalent physics, falling rocks/wood/crates, destructible terrain
   breaking into bodies. Recovered so far: the engine uses Box2D 2.3.x (public source, port from it); a `CSolidCell`
   is one pixel of a body image stamped into the grid (`0x71bdb0`); SetCell with a solid material builds a 1x1 body
   (`0x71bd10`); solid cell update `0x70e060` (burning, darkening, burn-out). Slices: (a) `box2d` crate: math, polygon
   and circle shapes, manifolds, contacts, island solver, bodies/fixtures; (b) body image to polygons (terrain
   code `0x811260`) and stamping pixels into the grid; (c) body-vs-grid collision and carving; (d) solid cell update,
   burning and `liquid_sand` to body conversion; (e) joints. Slice (a) status: `crates/box2d` has math, settings,
   circle/polygon/edge/chain shapes, manifolds (incl. edge vs circle/polygon with adjacency), GJK distance, time of
   impact, dynamic tree, broad phase, bodies, fixtures,
   contacts, the sequential-impulse solver with block solver, islands with sleeping, continuous collision (TOI),
   queries and begin/end contact events (29 tests incl. the Box2D HelloWorld drop, stacking, no tunneling, chains).
   Noita's Box2D is a modified 2.3.x: vectors, positions, velocities and angles are doubles, the f32 constants
   are widened (0.2f is 0.20000000298...), some locals stay float (`C` in the position solvers, the TOI target) and
   `Sweep::Normalize` uses a double 2*pi; the crate uses `Real = f64` accordingly. Next: audit each ported function
   against the exe for those float narrowings (contact solver, island, TOI, shapes; the edge collision code is stock
   2.3.1 and unaudited), then joints and a contact filter hook. Terrain bodies (`0x811260`) turn marching-squares
   polylines into one body per chunk with thin quad polygon fixtures (no edge/chain shapes); polygon `Set` is audited.
5. [~] Reactions (alchemy): the reference tables (normal and fast, tag expansion, per-material maximum probability),
    lookup, dispatcher, burning-cell path, record application (outputs, third input, direction, `req_lifetime`,
    lonely pixels), blobs and the liquid, gas and fire call sites are ported (see "Reactions" in `README.md`). Missing:
    blob particles, `convert_all`, entity spawns, audio, explosions, and an exact check of tag-expansion nesting for
    records with several tags. Required reactions are ported; the spontaneous freezing/melting rolls are dead code
    (their worker bytes are never written).
6. [ ] Other cell behaviors: lifetime, electricity/conductivity, freezing/melting, burning states, particle
   cells. Particle cells (flying pixels, 0x70-byte records in two pools of up to 100000, created by `0x7168c0`
   from a grid cell or by `0x716ce0`/`0x716af0` from a material and velocity, landed by `0x71fba0`/`0x71fde0`) are
   what explosions, joint breaks and blob effects emit. Ported in `sim::particles`: record defaults, the integrator
   (`0x713e20`, simplex swirl in `sim::noise`), the collision stepper (`0x712390`), `AddAsParticle` and the material
   creator, the dead-particle pass (`0x716600`) and the landing queue (`0x71a590`, `0x71fde0`, `0x71fba0`). Still
   missing: the explosion and joint-break callers, force rectangles, the render lists and the `liquid_stains == 2`
   hook; the engine's camera-local material grid is read from the live world instead. Layout notes are in the repo
   memory (`particles.md`).
7. [~] Scheduler: 64x64 worker regions with per-region dirty rects, RNGs and full counters, rect sharing across
    borders (`IMPL_AddUpdateAABB`), the random full-region scan, the per-frame shuffled sweep of every region in the
    update window, and the ±32 px movable margin ported. Missing: the
   threaded update order between regions, chunk streaming/loading, the global RNG seed, the confirmed simulation
   rate. Checkpoint: deterministic material-scene replays match the reference frame by frame.

## Phase 2: Engine skeleton

8. [ ] Entity/component system: entity XML loading, `<Base>` inheritance, tags, children, transforms, recovered
   system update order.
9. [ ] Component set: fields and defaults from `component_documentation.txt`, ported system by system as needed.
10. [ ] Lua runtime (LuaJIT / Lua 5.1) and the API from `lua_api_documentation.txt`; all script hooks.
11. [ ] RNG: world seed, `SetRandomSeed(x, y)` and every RNG path (needed for identical wands, spells, biomes,
    secrets).
12. [ ] Messages/events: damage, death, collision, pickup.

## Phase 3: Player and combat slice

13. [~] Player: walking, jumping, levitation, swimming, slippery ground, facing, body animation choice, animation
    stack with follow-ups and finish handling, and reference sprite ported. Missing: knockback/precision jump, crouch
    sensor and hitbox, kick, eating, arm and hotspots, cape (verlet), camera (follow, aim offset, shake), sprite
    frame timing.
14. [ ] Damage model: HP, damage types and multipliers, fall damage, fire, drowning, material damage, i-frames,
    blood/gore, ragdolls.
15. [ ] Projectiles: motion, cell/entity collision, lifetime, bounces, triggers.
16. [ ] Explosions: terrain carving by durability, physics impulses, damage falloff, light/shake. Recovered so
    far: 15 reference materials carry a CellData `<ExplosionConfig>` (`+0x100`) rolled from the burn hook (`0x70fb10`)
    and 3 reactions carry one; records (config `0x168` bytes plus position) queue on the worker (`0x72b3d0`), flush
    through `GridWorld` vt[32] (`0x721350`) and `0x657410` into the 18 KB `0x657960` (decompiles with
    `/force_decompile`; it casts 360 rays, then damages entities and bodies and emits particle cells).
17. [ ] Wands: `AbilityComponent` gun stats, cast delay, recharge, mana, spread, shuffle, spells per cast.
18. [ ] Spells: run the reference `gun.lua` / `gun_actions.lua` / extra modifiers in the Lua runtime; all ~400 spells
    with modifiers, triggers, multicasts, wrapping and draw/shuffle bugs.
19. [ ] Items and inventory: wands, flasks (material inventory, spray, throw), pickups, quick/full inventory, wand
    editing.
20. [ ] Enemy AI: AI components, pathfinding grid, herd relations; enemies from `data/entities/animals`.
    Checkpoint: first comparable slice (explore a prebuilt area, fight enemies, use wands).

## Phase 4: World and run structure

21. [ ] World generation: Wang tiles, biome map, pixel scenes, biome Lua spawn functions, parallel worlds.
22. [ ] World streaming: chunk load/unload, world size, wrapping, edges.
23. [ ] Rendering: backgrounds/parallax, sky, lighting, fog of war, emissive materials, glow, visibility-affecting
    post effects.
24. [ ] Holy Mountains: shops, wand-edit altar, perk altar, stealing collapse, Steve.
25. [ ] Perks: all perks (mostly Lua plus game effects).
26. [ ] Status effects: game effect system (wet, on fire, polymorph, stains, ingestion, ...) and status UI.
27. [ ] Bosses: Kolmi and end fight, other bosses, arenas and special scripts.
28. [ ] Progression: death, victory, new game+, orbs, endings.

## Phase 5: Full-game parity

29. [ ] UI/HUD: health, mana, flight bar, wand/inventory UI, pause menu, options, progress/bestiary, localization
    CSVs.
30. [ ] Audio: FMOD banks, sound events, music system, positional audio.
31. [ ] Save/load: world and player state, session format, save on quit.
32. [ ] Secrets and meta: orbs, per-seed alchemy recipes, cauldron, eye messages, fishing and statue puzzles, Sampo,
    ending variants, achievements/pillars, spell unlocks, daily and custom seeds.
33. [ ] Mod support (decision pending): mod init hooks, file overrides, appended scripts.
34. [ ] Performance: release profiling against the reference target (60 fps at default resolution).
35. [ ] Parity pass: scripted side-by-side replays with the same seed and inputs; fix or list every difference,
    including reproduced reference bugs.

## Ongoing

- Every ported rule gets a README entry with its exe address and a test.
- Behavior that can't be recovered statically is observed in the running game and recorded with the notebook template.
- Extend `re_tools` when needed (function-level decompiler view, replay
  capture). `noita_probe` (read-only runtime reader, see `README.md`) exists; next: load its dumps into `sim` to
  compare a pile or reaction scene frame by frame, and read the per-region RNGs.

## Biggest risks

- Physics (item 4): matching Box2D behavior exactly.
- Lua runtime and API (items 10, 18): a large surface, but it unlocks most content at once.
- World generation and RNG (items 11, 21): identical seeds are required for secrets to line up.
