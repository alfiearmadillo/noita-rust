"""Runs functions of the reference exe in Ghidra's emulator (GhidraMCP /emulate_function) and
writes golden vectors for the Rust ports. Needs scripts/ghidra-server.ps1 running.

    python scripts/emulate_golden.py set_seed   # crates/sim/tests/data/set_seed_golden.csv
"""

import json
import os
import random
import struct
import sys
import urllib.request

URL = "http://127.0.0.1:8089/emulate_function"
STACK = 0x7FFF0000  # the endpoint puts the return sentinel at [ESP]
SCRATCH = 0x20000000
# Default x87 control word / MXCSR: the CRT checks them to decide whether to raise FP exceptions.
FPU = {"FPUControlWord": 0x27F, "MXCSR": 0x1F80}


def emulate(function, registers, args=(), read=(), max_steps=20000, ret="EAX", mem=()):
    # cdecl/fastcall stack arguments start after the return address.
    memory = [{"address": hex(a), "hex": d.hex()} for a, d in mem]
    if args:
        data = b"".join(struct.pack("<I", a & 0xFFFFFFFF) for a in args)
        memory.append({"address": hex(STACK + 4), "hex": data.hex()})
    body = {
        "function": hex(function),
        "registers": {k: hex(v) for k, v in {**FPU, **registers}.items()},
        "memory": memory,
        "max_steps": max_steps,
        "return_registers": ret,
        "read_memory_after": [{"address": hex(a), "length": n} for a, n in read],
    }
    req = urllib.request.Request(
        URL, json.dumps(body).encode(), {"Content-Type": "application/json"}
    )
    with urllib.request.urlopen(req, timeout=120) as r:
        out = json.load(r)
    if not out.get("success"):
        raise RuntimeError(f"{hex(function)} {registers} {args}: {out}")
    return out


def bits(x):
    return struct.unpack("<Q", struct.pack("<d", x))[0]


def set_seed_cases():
    rng = random.Random(20260101)
    cases = []
    for seed in (0, 1, 2, 3, 12345, 0x22246, -7, 2**31 - 1, -(2**31)):
        for x, y in ((0.0, 0.0), (1.0, 1.0), (-3.5, 17.25), (512.0, -512.0)):
            cases.append((seed, x, y))
    for _ in range(300):
        seed = rng.randrange(-(2**31), 2**31)
        x = rng.choice([rng.uniform(-70000, 70000), float(rng.randrange(-40000, 40000))])
        y = rng.choice([rng.uniform(-40000, 40000), float(rng.randrange(-40000, 40000))])
        cases.append((seed, x, y))
    # Large values exercise the |b| >= 102400 branch; beyond about 6.8e10 the CRT raises an FP exception.
    for seed, x, y in ((5, 1e9, 3.0), (6, -1e9, -5e5), (7, 0.5, 2e5), (8, 6e10, 2e5)):
        cases.append((seed, x, y))
    return cases


def set_seed():
    path = "crates/sim/tests/data/set_seed_golden.csv"
    lines = ["# seed,x_bits,y_bits,state_bits: Random::SetRandomSeed (0x485890) in Ghidra's emulator"]
    for seed, x, y in set_seed_cases():
        regs = {"ECX": SCRATCH, "XMM2": bits(x), "XMM3": bits(y), "XMM0": bits(x), "XMM1": bits(y)}
        out = emulate(0x485890, regs, args=[seed], read=[(SCRATCH, 8)])
        state = int.from_bytes(bytes.fromhex(out["memory"][0]["hex"]), "little")
        lines.append(f"{seed},{bits(x):#018x},{bits(y):#018x},{state:#018x}")
    import os

    os.makedirs("crates/sim/tests/data", exist_ok=True)
    with open(path, "w", newline="\n") as f:
        f.write("\n".join(lines) + "\n")
    print(f"wrote {len(lines) - 1} vectors to {path}")


def f32(x):
    return struct.unpack("<I", struct.pack("<f", x))[0]


def rf32(x):
    return struct.unpack("<f", struct.pack("<f", x))[0]


def signed32(v):
    v &= 0xFFFFFFFF
    return v - (1 << 32) if v >= 1 << 31 else v


def low(hexstr, nbits):
    return int(hexstr, 16) & ((1 << nbits) - 1)


def state_mem(addr, st):
    return [(addr, struct.pack("<d", float(st)))]


def read_state(out):
    return int(struct.unpack("<d", bytes.fromhex(out["memory"][0]["hex"]))[0])


def random_golden():
    rng = random.Random(77)
    rows = ["# kind,fields: the Random class and helpers run in Ghidra's emulator (see crates/sim/tests/random_golden.rs)"]
    edge_states = [0, 1, 2, 16807, 2**31 - 2, 2**31 - 3]

    def states(n):
        return edge_states + [rng.randrange(1, 2**31 - 1) for _ in range(n)]

    for st in states(120):  # Next(min, max) 0x485b10
        lo, hi = sorted((rng.randrange(-5000, 5000), rng.randrange(-5000, 5000)))
        out = emulate(0x485B10, {"ECX": SCRATCH}, args=[lo, hi], read=[(SCRATCH, 8)], mem=state_mem(SCRATCH, st))
        rows.append(f"int,{st},{lo},{hi},{signed32(low(out['registers']['EAX'], 32))},{read_state(out)}")
    for st in states(80):  # NextF(min, max) 0x485ab0
        lo, hi = rf32(rng.uniform(-100, 100)), rf32(rng.uniform(-100, 100))
        regs = {"ECX": SCRATCH, "XMM1": f32(lo), "XMM2": f32(hi)}
        out = emulate(0x485AB0, regs, ret="XMM0", read=[(SCRATCH, 8)], mem=state_mem(SCRATCH, st))
        rows.append(f"float,{st},{f32(lo):#010x},{f32(hi):#010x},{low(out['registers']['XMM0'], 32):#010x},{read_state(out)}")
    for st in states(60):  # Next() 0x485a60
        out = emulate(0x485A60, {"ECX": SCRATCH}, ret="XMM0", read=[(SCRATCH, 8)], mem=state_mem(SCRATCH, st))
        rows.append(f"unit,{st},{low(out['registers']['XMM0'], 64):#018x},{read_state(out)}")
    # Seeds past 2^32 halve to a value cvttsd2si cannot represent, which Ghidra's emulator converts differently from hardware.
    seeds = [0.0, 1.0, 5.0, 12345.0, 2147483646.0, 2147483647.0, 4e9, 4294967295.0]
    seeds += [float(rng.randrange(1, 2**32)) for _ in range(60)] + [rng.uniform(0, 2**32) for _ in range(20)]
    for s in seeds:  # Random(double seed) 0x44d070
        out = emulate(0x44D070, {"ECX": SCRATCH, "XMM1": bits(s)}, read=[(SCRATCH, 8)])
        rows.append(f"from_seed,{bits(s):#018x},{read_state(out)}")
    for _ in range(40):  # Jenkins mix 0x8720c0
        a, b, c = (rng.randrange(0, 2**32) for _ in range(3))
        out = emulate(0x8720C0, {"ECX": a, "EDX": b}, args=[c])
        rows.append(f"mix,{a},{b},{c},{low(out['registers']['EAX'], 32)}")
    # sin/pow are DLL imports the emulator cannot run; baseline 2 accepts the first candidate with
    # |x - mean| < 0.5 without reaching them, so the draw order and loop are still checked.
    two = f32(2.0)
    for st in states(60):  # sampler 0x844100
        mean, sharp = rf32(rng.uniform(-0.2, 1.2)), rf32(rng.uniform(0.1, 6.0))
        regs = {"XMM0": f32(mean), "XMM1": f32(sharp), "XMM2": two}
        out = emulate(0x844100, regs, ret="XMM0", read=[(0x1221D18, 8)], mem=state_mem(0x1221D18, st), max_steps=100000)
        rows.append(f"dist,{st},{f32(mean):#010x},{f32(sharp):#010x},{two:#010x},{low(out['registers']['XMM0'], 32):#010x},{read_state(out)}")
    for st in states(50):  # RandomDistributionf core 0x8442f0
        lo, hi = sorted((rf32(rng.uniform(-50, 50)), rf32(rng.uniform(-50, 50))))
        mean = rf32(rng.uniform(lo, hi))
        sharp = rng.choice([0.0, 0.0, rf32(rng.uniform(0.1, 5.0))])
        regs = {"XMM0": f32(lo), "XMM1": f32(hi), "XMM2": f32(mean), "XMM3": f32(sharp)}
        out = emulate(0x8442F0, regs, args=[two], ret="XMM0", read=[(0x1221D18, 8)], mem=state_mem(0x1221D18, st), max_steps=100000)
        rows.append(f"distf,{st},{f32(lo):#010x},{f32(hi):#010x},{f32(mean):#010x},{f32(sharp):#010x},{two:#010x},{low(out['registers']['XMM0'], 32):#010x},{read_state(out)}")
    import os

    os.makedirs("crates/sim/tests/data", exist_ok=True)
    with open("crates/sim/tests/data/random_golden.csv", "w", newline="\n") as f:
        f.write("\n".join(rows) + "\n")
    print(f"wrote {len(rows) - 1} vectors to crates/sim/tests/data/random_golden.csv")


def noise_golden():
    rng = random.Random(5)
    rows = ["# x_bits,y_bits,z_bits,result_bits: simplex3 (0x872560) in Ghidra's emulator"]
    cases = [(0.0, 0.0, 0.0), (1.0, 1.0, 1.0), (0.5, 0.5, 0.5), (-1.5, 2.25, -0.75)]
    for _ in range(400):
        scale = rng.choice([1.0, 10.0, 300.0])
        cases.append(tuple(rng.uniform(-scale, scale) for _ in range(3)))
    for x, y, z in cases:
        regs = {"XMM0": bits(x), "XMM1": bits(y), "XMM2": bits(z)}
        out = emulate(0x872560, regs, ret="XMM0", max_steps=100000)
        rows.append(f"{bits(x):#018x},{bits(y):#018x},{bits(z):#018x},{low(out['registers']['XMM0'], 64):#018x}")
    with open("crates/sim/tests/data/noise_golden.csv", "w", newline="\n") as f:
        f.write("\n".join(rows) + "\n")
    print(f"wrote {len(rows) - 1} vectors to crates/sim/tests/data/noise_golden.csv")


def noise2d_golden():
    rng = random.Random(9)
    rows = ["# fn,x_bits,y_bits,result_bits: simplex2 0x872d40, perlin2 0x872a20, value2 0x872be0 (f32) in Ghidra's emulator"]
    cases = [(0.0, 0.0), (1.0, 1.0), (-1.0, -1.0), (0.5, -0.5), (255.0, 256.0), (-256.5, 3.25)]
    for _ in range(300):
        scale = rng.choice([1.0, 8.0, 200.0, 5000.0])
        x, y = rf32(rng.uniform(-scale, scale)), rf32(rng.uniform(-scale, scale))
        cases.append((float(round(x)) if rng.random() < 0.15 else x, y))
    for name, addr in (("simplex2", 0x872D40), ("perlin2", 0x872A20), ("value2", 0x872BE0)):
        for x, y in cases:
            out = emulate(addr, {"XMM0": f32(x), "XMM1": f32(y)}, ret="XMM0", max_steps=100000)
            rows.append(f"{name},{f32(x):#010x},{f32(y):#010x},{low(out['registers']['XMM0'], 32):#010x}")
    with open("crates/sim/tests/data/noise2d_golden.csv", "w", newline="\n") as f:
        f.write("\n".join(rows) + "\n")
    print(f"wrote {len(rows) - 1} vectors to crates/sim/tests/data/noise2d_golden.csv")


def simplex2d_golden():
    import math

    rng = random.Random(31)
    rows = ["# x_bits,y_bits,result_bits: 0x872300 (f64 2D simplex) in Ghidra's emulator"]
    # The skew constants are filled by static initializers (0x406fc0, 0x406ff0) in the real process.
    consts = [
        (0x1207BE0, struct.pack("<d", (math.sqrt(3.0) - 1.0) * 0.5)),
        (0x1207CD0, struct.pack("<d", (3.0 - math.sqrt(3.0)) / 6.0)),
    ]
    cases = [(0.0, 0.0), (1.0, 1.0), (0.5, 0.5), (-1.5, 2.25), (255.0, 256.0), (-0.001, 0.001)]
    for _ in range(400):
        scale = rng.choice([0.5, 4.0, 40.0, 3000.0])
        cases.append((rng.uniform(-scale, scale), rng.uniform(-scale, scale)))
    for x, y in cases:
        out = emulate(0x872300, {"XMM0": bits(x), "XMM1": bits(y)}, ret="XMM0", max_steps=100000, mem=consts)
        rows.append(f"{bits(x):#018x},{bits(y):#018x},{low(out['registers']['XMM0'], 64):#018x}")
    with open("crates/sim/tests/data/simplex2d_golden.csv", "w", newline="\n") as f:
        f.write("\n".join(rows) + "\n")
    print(f"wrote {len(rows) - 1} vectors to crates/sim/tests/data/simplex2d_golden.csv")


TS_BASE = 0x50000000
TLS_PAGE = 0x40000000
OUT_BASE = 0x60000000


def tile_pixel(tile_index, x, y, width):
    # The golden files carry no game art: tiles are filled with a pattern derived from their index.
    return bytes([tile_index & 255, (tile_index >> 8) & 255, (y * width + x) & 255])


def enumerate_tiles(is_corner, nc, vx, vy):
    """The tile slots of stbhw__process_template in order: (h tiles, v tiles) as [a..f] lists."""

    def rows(ranges, variants):
        out = []
        for _ in range(variants):
            for f in ranges[5]:
                for e in ranges[4]:
                    for d in ranges[3]:
                        for c in ranges[2]:
                            for b in ranges[1]:
                                for a in ranges[0]:
                                    out.append([a, b, c, d, e, f])
        return out

    al = lambda n: range(n)
    one = lambda v: range(v, v + 1)
    h, v = [], []
    if is_corner:
        for k in range(nc[2]):
            for j in range(nc[1]):
                for i in range(nc[0]):
                    for _ in range(vy):
                        h += rows([al(nc[1]), al(nc[2]), al(nc[3]), one(i), one(j), one(k)], vx)
        for k in range(nc[3]):
            for j in range(nc[0]):
                for i in range(nc[1]):
                    for _ in range(vx):
                        v += rows([al(nc[0]), al(nc[3]), al(nc[2]), one(i), one(j), one(k)], vy)
    else:
        for k in range(nc[3]):
            for j in range(nc[4]):
                for i in range(nc[2]):
                    for _ in range(vy):
                        h += rows([al(nc[2]), one(k), al(nc[1]), one(j), al(nc[0]), one(i)], vx)
        for k in range(nc[3]):
            for j in range(nc[4]):
                for i in range(nc[5]):
                    for _ in range(vx):
                        v += rows([al(nc[0]), one(i), al(nc[1]), one(j), al(nc[5]), one(k)], vy)
    return h, v


def wang_run(ts, seed, w, h):
    """Runs stbhw_generate_image (0x866b90) in the emulator on a tileset built in emulator memory."""
    is_corner, nc, side, htiles, vtiles = ts
    mem = []
    # tiles: 6 color bytes then pixels, laid out one after another
    tile_addr = 0x53000000
    h_ptrs, v_ptrs = [], []
    blob = bytearray()
    for idx, (lst, ptrs, tw, th) in enumerate(((htiles, h_ptrs, 2 * side, side), (vtiles, v_ptrs, side, 2 * side))):
        for n, colors in enumerate(lst):
            gi = n if lst is htiles else len(htiles) + n
            ptrs.append(tile_addr + len(blob))
            blob += bytes(c & 255 for c in colors)
            for y in range(th):
                for x in range(tw):
                    blob += tile_pixel(gi, x, y, tw)
    mem.append((tile_addr, bytes(blob)))
    mem.append((0x51000000, b"".join(struct.pack("<I", p) for p in h_ptrs)))
    mem.append((0x52000000, b"".join(struct.pack("<I", p) for p in v_ptrs)))
    header = struct.pack("<I6iIIIIIII", is_corner, *nc, side, 0x51000000, 0x52000000, len(htiles), len(htiles), len(vtiles), len(vtiles))
    mem.append((TS_BASE, header))
    # fs:[0x2c] -> [TLS_PAGE] -> S; [S+0x14] -> the Random object holding the double state
    state = from_seed_state(seed)
    S, R = TLS_PAGE + 0x1000, TLS_PAGE + 0x2000
    mem.append((0x2C, struct.pack("<I", TLS_PAGE)))
    mem.append((TLS_PAGE, struct.pack("<I", S)))
    mem.append((S + 0x14, struct.pack("<I", R)))
    mem.append((R, struct.pack("<d", float(state))))
    # memset is a DLL import (thunk 0xdfe27e, IAT 0xf07318); this stub keeps EDI and returns the destination.
    memset = bytes.fromhex("57" "8b7c2408" "0fb64424 0c" "69c001010101" "8b4c2410" "89ca" "c1e902" "f3ab" "89d1" "83e103" "f3aa" "8b442408" "5f" "c3")
    mem.append((0x30001000, memset))
    mem.append((0xF07318, struct.pack("<I", 0x30001000)))
    out = emulate(0x866B90, {"ECX": TS_BASE}, args=[OUT_BASE, w * 3, w, h], read=[(OUT_BASE, w * h * 3)], mem=mem, max_steps=100000)
    return int(out["registers"]["EAX"], 16) & 0xFFFFFFFF, bytes.fromhex(out["memory"][0]["hex"])


def from_seed_state(seed):
    """Random::Random(double) (0x44d070): the seed (halved once past 2^31-1), then one step."""
    s = seed * 0.5 if seed >= 2147483647.0 else seed
    s = int(s)
    q = int(s / 127773)
    v = (s * 16807 - q * 2147483647) & 0xFFFFFFFF
    v = v - (1 << 32) if v >= 1 << 31 else v
    return v if v >= 1 else v + 2147483647


def wang_golden():
    import subprocess
    import tempfile

    rng = random.Random(3)
    tilesets = []
    for name in ("endgame", "water", "the_sky", "town_under", "tutorial", "sandcave"):
        path = os.path.join(tempfile.gettempdir(), "gh", f"wangdump_{name}.png.txt")
        lines = open(path).read().split("\n")
        head = [int(t) for t in lines[0].split()]
        colors = [[int(t) for t in l.split()] for l in lines[1 : 1 + head[8] + head[9]]]
        tilesets.append((name, (head[0], head[1:7], head[7], colors[: head[8]], colors[head[8] :])))
    for name, is_corner, nc, vx, vy in (
        ("edge_a", 0, [2, 1, 1, 1, 1, 1], 1, 1),
        ("edge_b", 0, [2, 2, 1, 2, 1, 1], 1, 1),
        ("edge_c", 0, [1, 1, 2, 1, 1, 2], 1, 1),
        ("corner_s", 1, [1, 2, 1, 2, 0, 0], 1, 1),
    ):
        h, v = enumerate_tiles(is_corner, nc, vx, vy)
        tilesets.append((name, (is_corner, nc, 4, h, v)))
    lines = ["# tileset <name> <corner> <nc0..nc5> <side> <nh> <nv> then one line of colors per tile; case <name> <seed bits> <w> <h> <ok> <hex>"]
    cases = 0
    for name, ts in tilesets:
        is_corner, nc, side, ht, vt = ts
        lines.append(f"tileset {name} {is_corner} {' '.join(map(str, nc))} {side} {len(ht)} {len(vt)}")
        lines += [" ".join(map(str, c)) for c in ht + vt]
        for attempt in range(3):
            seed = float(rng.randrange(1, 2**32))
            w, h = side * rng.randrange(3, 6) + rng.randrange(0, side), side * rng.randrange(2, 5) + rng.randrange(0, side)
            result = None
            while result is None:
                try:
                    result = wang_run(ts, seed, w, h)
                except RuntimeError as ex:
                    if "max_steps" not in str(ex):
                        raise
                    if w <= side * 2 and h <= side * 2:
                        break
                    w, h = max(side * 2, w - side), max(side * 2, h - side)
            if result is None:
                print(f"{name}: too slow even at the smallest size")
                continue
            ok, data = result
            lines.append(f"case {name} {bits(seed):#018x} {w} {h} {ok} {data.hex()}")
            cases += 1
    os.makedirs("crates/worldgen/tests/data", exist_ok=True)
    with open("crates/worldgen/tests/data/wang_golden.txt", "w", newline="\n") as f:
        f.write("\n".join(lines) + "\n")
    print(f"wrote {cases} cases")


def warp_golden():
    rng = random.Random(11)
    rows = ["# off_x_bits,off_y_bits,strength_bits,x_bits,y_bits,u_bits,v_bits: world_to_wang (0x87ce50) in Ghidra's emulator"]
    # The exe calls floor through a DLL import; the stub (integer floor, returns in ST0) replaces its IAT target.
    stub = bytes.fromhex("f20f10442404" "f20f2cc0" "f20f2ac8" "660f2fc8" "7601" "48" "f20f2ac0" "f20f114424f8" "dd4424f8" "c3")
    for _ in range(500):
        off_x, off_y = rng.uniform(-100000, 100000), 7168.0
        strength = rng.choice([0.0, -1.0, 0.5, 1.0, 2.0, rf32(rng.uniform(0.1, 4.0))])
        x, y = rng.uniform(-40000, 40000), rng.uniform(-10000, 40000)
        if rng.random() < 0.2:
            x, y = float(round(x)), float(round(y))
        mem = [(0x30000000, stub), (0xF07320, struct.pack("<I", 0x30000000)),
               (0x1206FB0, struct.pack("<d", off_x)), (0x12051B0, struct.pack("<d", off_y))]
        regs = {"ECX": SCRATCH, "XMM1": f32(strength), "XMM2": bits(x), "XMM3": bits(y)}
        out = emulate(0x87CE50, regs, read=[(SCRATCH, 8)], mem=mem, max_steps=100000)
        u, v = struct.unpack("<II", bytes.fromhex(out["memory"][0]["hex"]))
        rows.append(f"{bits(off_x):#018x},{bits(off_y):#018x},{f32(strength):#010x},{bits(x):#018x},{bits(y):#018x},{u:#010x},{v:#010x}")
    with open("crates/worldgen/tests/data/warp_golden.csv", "w", newline="\n") as f:
        f.write("\n".join(rows) + "\n")
    print(f"wrote {len(rows) - 1} vectors to crates/worldgen/tests/data/warp_golden.csv")


def simplex_consts():
    import math

    return [
        (0x1207BE0, struct.pack("<d", (math.sqrt(3.0) - 1.0) * 0.5)),
        (0x1207CD0, struct.pack("<d", (3.0 - math.sqrt(3.0)) / 6.0)),
    ]


def pack_f(v):
    return struct.pack("<f", v)


def make_rule(rng, i, material=None):
    if True:
        r = {
            "material": 100 + i if material is None else material,
            "min": rf32(rng.uniform(-0.2, 0.7)),
            "add_perlin": int(rng.random() < 0.3),
            "sx": rf32(rng.uniform(0.001, 0.2)), "sy": rf32(rng.uniform(0.001, 0.2)),
            "is_rare": int(rng.random() < 0.5),
            "limit_y": int(rng.random() < 0.4),
            "use_perlin": int(rng.random() < 0.4), "use_fbm": int(rng.random() < 0.5),
            "use_polka": int(rng.random() < 0.6),
            "rsx": rf32(rng.uniform(0.002, 0.4)), "rsy": rf32(rng.uniform(0.002, 0.4)),
            "by_seed": int(rng.random() < 0.4),
            "ox": rf32(rng.uniform(-50, 50)), "oy": rf32(rng.uniform(-50, 50)),
            "prl": rf32(rng.uniform(0.05, 0.5)), "prh": rf32(rng.uniform(0.3, 0.95)),
            "boxed": int(rng.random() < 0.5), "prob": rf32(rng.uniform(0.1, 0.95)),
            "reqmin": rf32(rng.uniform(-0.4, 0.4)), "reqmax": rf32(rng.choice([0.4, 0.8, 10.0])),
            "is_poly": int(rng.random() < 0.25),
        }
        r["max"] = rf32(r["min"] + rng.uniform(0.05, 0.9))
        a = rng.uniform(-1500, 1500)
        r["lmin"] = rf32(a)
        r["lmax"] = rf32(a + rng.uniform(100, 4000))
        npts = rng.randrange(3, 8)
        cx, cy = rng.uniform(0.3, 0.7), rng.uniform(0.3, 0.7)
        r["poly"] = [(rf32(cx + rng.uniform(-0.45, 0.45)), rf32(cy + rng.uniform(-0.45, 0.45))) for _ in range(npts)]
        return r


def rule_line(r):
    flat = [r["material"], f32(r["min"]), f32(r["max"]), r["add_perlin"], f32(r["sx"]), f32(r["sy"]),
            r["is_rare"], r["limit_y"], f32(r["lmin"]), f32(r["lmax"]), r["use_perlin"], r["use_fbm"],
            r["use_polka"], f32(r["rsx"]), f32(r["rsy"]), r["by_seed"], f32(r["ox"]), f32(r["oy"]),
            f32(r["prl"]), f32(r["prh"]), r["boxed"], f32(r["prob"]), f32(r["reqmin"]), f32(r["reqmax"]),
            r["is_poly"], len(r["poly"])]
    flat += [f32(c) for p in r["poly"] for c in p]
    return "rule " + " ".join(str(v) if isinstance(v, int) and v < 1000 else f"{v:#x}" for v in flat)


def rule_bytes(r, poly_addr):
    if True:
        b = bytearray(0x84)
        struct.pack_into("<I", b, 4, r["material"])
        struct.pack_into("<ff", b, 0x24, r["min"], r["max"])
        b[0x2C] = r["add_perlin"]
        struct.pack_into("<ff", b, 0x30, r["sx"], r["sy"])
        b[0x38], b[0x39] = r["is_rare"], r["limit_y"]
        struct.pack_into("<ff", b, 0x3C, r["lmin"], r["lmax"])
        b[0x44], b[0x45], b[0x46] = r["use_perlin"], r["use_fbm"], r["use_polka"]
        struct.pack_into("<ff", b, 0x48, r["rsx"], r["rsy"])
        b[0x50] = r["by_seed"]
        struct.pack_into("<ff", b, 0x54, r["ox"], r["oy"])
        struct.pack_into("<ff", b, 0x5C, r["prl"], r["prh"])
        b[0x64] = r["boxed"]
        struct.pack_into("<fff", b, 0x68, r["prob"], r["reqmin"], r["reqmax"])
        b[0x74] = r["is_poly"]
        n = len(r["poly"])
        struct.pack_into("<II", b, 0x78, poly_addr, poly_addr + 8 * n)
        return bytes(b)


def rules_golden():
    """Biome material rules: RuleList::pick (0x86d2a0), with its noise helpers, in the emulator."""
    rng = random.Random(404)
    consts = simplex_consts()
    base, rules_at, poly_at = 0x50000000, 0x50001000, 0x50100000
    f = pack_f
    out_lines = ["# lists and cases for RuleList::pick (0x86d2a0) run in Ghidra's emulator"]
    cases = 0
    for list_id in range(70):
        lo, hi = rf32(rng.uniform(-0.3, 0.2)), rf32(rng.uniform(0.9, 1.6))
        seed = rng.choice([0, 1, 12345, rng.randrange(-(2**31), 2**31), rng.randrange(0, 2**31)])
        rules = [make_rule(rng, i) for i in range(rng.randrange(1, 6))]
        mem = list(consts)
        for i, r in enumerate(rules):
            pa = poly_at + i * 0x100
            mem.append((rules_at + i * 0x84, rule_bytes(r, pa)))
            mem.append((pa, b"".join(f(px) + f(py) for px, py in r["poly"])))
        mem.append((base, f(lo) + f(hi) + struct.pack("<II", rules_at, rules_at + 0x84 * len(rules))))
        mem.append((0x1205004, struct.pack("<i", seed)))
        out_lines.append(f"list {list_id} {f32(lo):#010x} {f32(hi):#010x} {seed} {len(rules)}")
        for r in rules:
            out_lines.append(rule_line(r))
        for _ in range(14):
            x = rng.choice([rng.randrange(-30000, 30000), rng.randrange(-600, 600)])
            y = rng.choice([rng.randrange(-5000, 40000), rng.randrange(-600, 3000)])
            v = rf32(rng.uniform(-0.1, 1.3))
            regs = {"ECX": base, "XMM3": f32(v), "XMM2": f32(v)}
            res = emulate(0x86D2A0, regs, args=[x, y], mem=mem, max_steps=100000)
            out_lines.append(f"case {x} {y} {f32(v):#010x} {low(res['registers']['EAX'], 32)}")
            cases += 1
    with open("crates/worldgen/tests/data/rules_golden.txt", "w", newline="\n") as fh:
        fh.write("\n".join(out_lines) + "\n")
    print(f"wrote {cases} cases")


def terrain_golden():
    """Wang map samplers (0x8712d0, 0x870e60, 0x871160, 0x870f80) and the blend 0x87d010."""
    rng = random.Random(808)
    wo, fdata, mdata = 0x50000000, 0x50100000, 0x50200000
    rows = ["# images, sampler cases and blend vectors run in Ghidra's emulator (crates/worldgen/tests/terrain_golden.rs)"]
    cases = 0
    for img_id in range(60):
        w, h = rng.randrange(1, 14), rng.randrange(1, 14)
        floats = [rng.choice([0.0, 0.25, 0.5, 0.5, 0.75, 1.0, rf32(rng.random())]) for _ in range(w * h)]
        mats = [rng.choice([0, 0, 1, 2, 3]) for _ in range(w * h)]
        hdr = lambda data: struct.pack("<IIIIIIII", w, h, w * h, 0, 0, 0, data, w * h)
        mem = [(wo, hdr(fdata) + hdr(mdata)),
               (fdata, b"".join(struct.pack("<f", v) for v in floats)),
               (mdata, b"".join(struct.pack("<H", v) for v in mats))]
        rows.append(f"image {img_id} {w} {h} " + " ".join(f"{f32(v):#x}" for v in floats) + " " + " ".join(map(str, mats)))
        for _ in range(14):
            u = rng.choice([rng.uniform(-2 * w, 3 * w), float(rng.randrange(-w, 2 * w)), rng.randrange(-2 * w, 3 * w) + 0.5, rng.uniform(0, w)])
            v = rng.choice([rng.uniform(-2 * h, 3 * h), float(rng.randrange(-h, 2 * h)), rng.randrange(-2 * h, 3 * h) + 0.5, rng.uniform(0, h)])
            for name, addr, ret in (("round", 0x8712D0, "EAX"), ("smooth", 0x870E60, "XMM0"),
                                    ("mode1", 0x871160, "XMM0"), ("mode2", 0x870F80, "XMM0")):
                out = emulate(addr, {"ECX": wo, "XMM1": bits(u), "XMM2": bits(v)}, ret=ret, mem=mem, max_steps=20000)
                res = low(out["registers"][ret], 32)
                rows.append(f"case {img_id} {name} {bits(u):#x} {bits(v):#x} {res:#x}")
                cases += 1
    for _ in range(300):  # 0x87d010
        x = rng.choice([rng.uniform(-30000, 30000), float(rng.randrange(-5000, 5000))])
        y = rng.choice([rng.uniform(-5000, 30000), float(rng.randrange(-5000, 5000))])
        base = rf32(rng.choice([rng.uniform(-0.5, 1.5), rng.random(), 0.5, 1.0]))
        out = emulate(0x87D010, {"XMM0": bits(x), "XMM1": bits(y), "XMM2": f32(base)}, ret="XMM0", max_steps=20000)
        rows.append(f"blend {bits(x):#x} {bits(y):#x} {f32(base):#x} {low(out['registers']['XMM0'], 32):#x}")
        cases += 1
    with open("crates/worldgen/tests/data/terrain_golden.txt", "w", newline="\n") as fh:
        fh.write("\n".join(rows) + "\n")
    print(f"wrote {cases} cases")


def material_at_golden():
    """The whole Wang-biome material lookup (0x87d0e0, biome type 2) in the emulator."""
    rng = random.Random(1212)
    W, MGR, GRID, REGION, RULES, WANG = 0x51000000, 0x51100000, 0x51200000, 0x52000000, 0x53000000, 0x50000000
    CB = 0x70000000
    fdata, mdata, rules_at, poly_at = 0x50100000, 0x50200000, 0x53001000, 0x53100000
    # `floor` and `round` are DLL imports: integer stubs returning in ST0 replace their IAT targets.
    floor_stub = bytes.fromhex("f20f10442404" "f20f2cc0" "f20f2ac8" "660f2fc8" "7601" "48" "f20f2ac0" "f20f114424f8" "dd4424f8" "c3")
    round_stub = bytes.fromhex("f20f10442404" "660f28c8" "660f540d00400030" "660f560d10400030"
                               "f20f58c1" "f20f2cc0" "f20f2ac0" "f20f114424f8" "dd4424f8" "c3")
    stubs = [(0x30000000, floor_stub), (0xF07320, struct.pack("<I", 0x30000000)),
             (0x30003000, round_stub), (0xF07430, struct.pack("<I", 0x30003000)),
             (0x30004000, struct.pack("<QQ", 0x8000000000000000, 0) + struct.pack("<QQ", 0x3FE0000000000000, 0))]
    rows = ["# Wang biome lookups (0x87d0e0) run in Ghidra's emulator (crates/worldgen/tests/material_at_golden.rs)"]
    cases = 0
    for bid in range(40):
        w, h = rng.randrange(5, 17), rng.randrange(5, 17)
        floats = [rng.choice([0.0, 0.25, 0.5, 0.75, 1.0, 1.0, 1.0, rf32(rng.random())]) for _ in range(w * h)]
        mats = [rng.choice([0, 0, 1, 2, 3, 4, 5]) for _ in range(w * h)]
        off = (rng.uniform(-100000, 100000), 7168.0)
        seed = rng.choice([0, 12345, rng.randrange(-(2**31), 2**31)])
        cells = {i: (rf32(rng.choice([1.0, 0.0, 0.5, 2.0])), rf32(rng.choice([0.5, 0.5, 0.25, 0.3, 0.7])), rng.choice([0, 1, 2, 0]))
                 for i in range(1, 5)}
        lo, hi = rf32(rng.uniform(-0.3, 0.2)), rf32(rng.uniform(0.9, 1.6))
        rules = [make_rule(rng, i, material=0x10000000 + 100 + i) for i in range(rng.randrange(1, 6))]
        hdr = lambda data: struct.pack("<IIIIIIII", w, h, w * h, 0, 0, 0, data, w * h)
        mem = list(stubs) + simplex_consts() + [
            (WANG, hdr(fdata) + hdr(mdata)),
            (fdata, b"".join(struct.pack("<f", v) for v in floats)),
            (mdata, b"".join(struct.pack("<H", v) for v in mats)),
            (0x1206FB0, struct.pack("<d", off[0])), (0x12051B0, struct.pack("<d", off[1])),
            (0x1205004, struct.pack("<i", seed)),
            (W + 0x28, struct.pack("<I", MGR)), (W + 0x38, struct.pack("<dd", 0.0, 0.0)),
            (W + 0x50, struct.pack("<II", 3, 3)), (W + 0x68, struct.pack("<I", GRID)), (W + 0x6C, struct.pack("<I", 1)),
            (MGR + 0x18, struct.pack("<I", CB)),
            (GRID, struct.pack("<I", REGION) * 9),
            (REGION + 4, struct.pack("<I", 2)), (REGION + 0x1D4, struct.pack("<I", WANG)), (REGION + 0x2A4, struct.pack("<I", RULES)),
            (RULES, pack_f(lo) + pack_f(hi) + struct.pack("<II", rules_at, rules_at + 0x84 * len(rules))),
        ]
        for i, (s, c, t) in cells.items():
            mem.append((CB + i * 0x290 + 0x234, struct.pack("<ffi", s, c, t)))
        for i, r in enumerate(rules):
            pa = poly_at + i * 0x100
            mem.append((rules_at + i * 0x84, rule_bytes(r, pa)))
            mem.append((pa, b"".join(pack_f(px) + pack_f(py) for px, py in r["poly"])))
        rows.append(f"biome {bid} {bits(off[0]):#x} {bits(off[1]):#x} {seed} {w} {h} "
                    + " ".join(f"{f32(v):#x}" for v in floats) + " " + " ".join(map(str, mats)))
        rows.append(f"cells {bid} " + " ".join(f"{f32(s):#x} {f32(c):#x} {t}" for s, c, t in cells.values()))
        rows.append(f"rules {bid} {f32(lo):#x} {f32(hi):#x} {len(rules)}")
        rows += [rule_line(r) for r in rules]
        for _ in range(24):
            x = rng.choice([rng.uniform(-300, 600), float(rng.randrange(-300, 600))])
            y = rng.choice([rng.uniform(-300, 600), float(rng.randrange(-300, 600))])
            out = emulate(0x87D0E0, {"ECX": W, "XMM1": bits(x), "XMM2": bits(y)}, mem=mem, max_steps=100000)
            eax = low(out["registers"]["EAX"], 32)
            kind = "T" if CB < eax < CB + 5 * 0x290 else "R"
            val = (eax - CB) // 0x290 if kind == "T" else eax
            rows.append(f"case {bid} {bits(x):#x} {bits(y):#x} {kind} {val}")
            cases += 1
    with open("crates/worldgen/tests/data/material_at_golden.txt", "w", newline="\n") as fh:
        fh.write("\n".join(rows) + "\n")
    print(f"wrote {cases} cases")


def convert_golden():
    """The Wang region image conversion (0x8704c0): colors to float and material images."""
    rng = random.Random(4242)
    THIS, SRC, SDATA, FDATA, TAB = 0x50000000, 0x51000000, 0x51100000, 0x52000000, 0x30012000
    BUMP, HEAP = 0x6FFF0000, 0x70000000
    callback = 0x30011000
    # color callback (cdecl, one argument): air color -> 0, else first table entry from 1, else -1
    T = TAB
    code = bytearray()
    code += bytes.fromhex("8b442404" "25ffffff00")
    code += b"\x8b\x15" + struct.pack("<I", T + 4) + bytes.fromhex("81e2ffffff00" "39d0" "7503" "31c0" "c3")
    code += bytes.fromhex("b901000000")
    code += b"\x3b\x0d" + struct.pack("<I", T) + bytes.fromhex("7d17")
    code += b"\x8b\x14\x8d" + struct.pack("<I", T + 4) + bytes.fromhex("81e2ffffff00" "39d0" "7403" "41" "ebe4")
    code += bytes.fromhex("89c8" "c3" "b8ffffffff" "c3")
    malloc = bytes.fromhex("a1") + struct.pack("<I", BUMP) + bytes.fromhex("8b4c2404" "83c10f" "83e1f0" "010d") + struct.pack("<I", BUMP) + b"\xc3"
    stubs = [
        (0x30010000, malloc), (0xF074F0, struct.pack("<I", 0x30010000)), (0xF07500, struct.pack("<I", 0x30010000)),
        (0x30010100, b"\xc3"), (0xF074F4, struct.pack("<I", 0x30010100)), (0xF07504, struct.pack("<I", 0x30010100)),
        (0xF07318, struct.pack("<I", 0x30001000)),
        (callback, bytes(code)),
        (BUMP, struct.pack("<I", HEAP)),
        (0, bytes(16)),  # fs:[0]
    ]
    # memset(dst, value, size) -> dst, preserving callee-saved registers (as in the wang harness)
    memset = bytes.fromhex("57" "8b7c2408" "8a44240c" "8b4c2410" "f3aa" "8b442408" "5f" "c3")
    stubs.append((0x30001000, memset))
    pal = [(0, 0, 0x42), (255, 0, 0), (0, 255, 0), (0, 0, 255), (10, 200, 30), (200, 200, 0), (7, 7, 7)]
    other = [(0x33, 0x66, 0x99), (0x10, 0x20, 0x30), (0xEE, 0xEE, 0xEE), (255, 255, 254), (1, 0, 0), (0, 0, 1), (0, 1, 0)]
    rows = ["# 0x8704c0 run in Ghidra's emulator (crates/worldgen/tests/convert_golden.rs): table | pixels | floats | materials"]
    word = lambda c: 0xFF000000 | c[2] << 16 | c[1] << 8 | c[0]
    for case in range(60):
        w, h = rng.randrange(2, 9), rng.randrange(2, 9)
        table = rng.sample(pal, rng.randrange(2, len(pal) + 1))
        pool = [(0, 0, 0), (255, 255, 255)] + table * 2 + other
        pixels = [rng.choice(pool) for _ in range(w * h)]
        mem = list(stubs) + [
            (THIS, struct.pack("<IIIIIIII", w, h, w * h, 0, 0, 0, FDATA, w * h)),
            (THIS + 0xB0, struct.pack("<II", callback, 0)),
            (SRC, struct.pack("<IIIIIIII", w, h, w * h, 0, 0, 0, SDATA, w * h)),
            (SDATA, b"".join(struct.pack("<I", word(c)) for c in pixels)),
            (FDATA, bytes(4 * w * h)),
            (TAB, struct.pack("<I", len(table)) + b"".join(struct.pack("<I", word(c)) for c in table)),
        ]
        out = emulate(0x8704C0, {"ECX": THIS}, args=[SRC, 0, 1, 0, 0, w, h], mem=mem, max_steps=100000,
                      read=[(THIS, 0xC0), (FDATA, 4 * w * h), (HEAP, 0x4000)])
        blocks = [bytes.fromhex(m["hex"]) for m in out["memory"]]
        this, floats, heap = blocks
        ptr16 = struct.unpack_from("<I", this, 0x38)[0]
        if not HEAP <= ptr16 < HEAP + 0x4000 - 2 * w * h:
            raise RuntimeError(f"unexpected Image16 pointer {ptr16:#x}")
        m16 = struct.unpack_from(f"<{w * h}H", heap, ptr16 - HEAP)
        fl = struct.unpack_from(f"<{w * h}I", floats)
        rows.append(f"case {case} {w} {h} " + " ".join(f"{c[0]:02x}{c[1]:02x}{c[2]:02x}" for c in table) + " | "
                    + " ".join(f"{c[0]:02x}{c[1]:02x}{c[2]:02x}" for c in pixels) + " | "
                    + " ".join(f"{v:x}" for v in fl) + " | " + " ".join(map(str, m16)))
    with open("crates/worldgen/tests/data/convert_golden.txt", "w", newline="\n") as fh:
        fh.write("\n".join(rows) + "\n")
    print(f"wrote {len(rows) - 1} cases")


if __name__ == "__main__":
    {"set_seed": set_seed, "random": random_golden, "noise": noise_golden, "noise2d": noise2d_golden, "warp": warp_golden, "wang": wang_golden, "simplex2d": simplex2d_golden, "rules": rules_golden, "terrain": terrain_golden, "material_at": material_at_golden, "convert": convert_golden}[sys.argv[1]]()
