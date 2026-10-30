"""Runs functions of the reference exe in Ghidra's emulator (GhidraMCP /emulate_function) and
writes golden vectors for the Rust ports. Needs scripts/ghidra-server.ps1 running.

    python scripts/emulate_golden.py set_seed   # crates/sim/tests/data/set_seed_golden.csv
"""

import json
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


if __name__ == "__main__":
    {"set_seed": set_seed, "random": random_golden}[sys.argv[1]]()
