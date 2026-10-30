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


if __name__ == "__main__":
    {"set_seed": set_seed, "random": random_golden, "noise": noise_golden, "noise2d": noise2d_golden, "warp": warp_golden, "wang": wang_golden}[sys.argv[1]]()
