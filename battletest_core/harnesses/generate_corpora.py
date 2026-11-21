#!/usr/bin/env python3
"""Generate the 420+ hand-authored adversarial Core/English corpus.

Programs are grouped into per-tier directories.  Every spec records an exact
expected exit code and output; the probe harness asserts ELF64 == interpreter
parity in addition to those expectations.
"""

from __future__ import annotations

import base64
import json
import shutil
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CORPORA = ROOT / "corpora"
SPECS = CORPORA / "specs.jsonl"
PROGRAMS: list[dict] = []


def low(value: int) -> int:
    return value & 0xFF


def wrap_signed(value: int, bits: int) -> int:
    mask = (1 << bits) - 1
    value &= mask
    if value & (1 << (bits - 1)):
        value -= 1 << bits
    return value


def wrap_unsigned(value: int, bits: int) -> int:
    return value & ((1 << bits) - 1)


def add_program(
    tier: str,
    ident: str,
    files: dict[str, str],
    exit_code: int,
    *,
    stdout: bytes = b"",
    args: list[str] | None = None,
    stdin: bytes = b"",
    kind: str = "run",
    error_contains: str | None = None,
    comments: list[str] | None = None,
) -> None:
    directory = CORPORA / f"tier_{tier.lower()}" / ident
    if directory.exists():
        shutil.rmtree(directory)
    directory.mkdir(parents=True)
    for name, content in files.items():
        path = directory / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")
    spec = {
        "id": ident,
        "tier": tier,
        "dir": str(directory.relative_to(ROOT)),
        "entry": "main.tc",
        "kind": kind,
        "exit": exit_code,
        "stdout_b64": base64.b64encode(stdout).decode("ascii"),
        "args": args or [],
        "stdin_b64": base64.b64encode(stdin).decode("ascii"),
    }
    if error_contains:
        spec["error_contains"] = error_contains
    if comments:
        spec["comments"] = comments
    PROGRAMS.append(spec)


def main_program(text: str) -> dict[str, str]:
    return {"main.tc": text}


# ---------------------------------------------------------------------------
# Tier C1 — scalar widths, wraparound, bitwise, IEEE-754
# ---------------------------------------------------------------------------

def tier_c1() -> None:
    widths = [("i8", 8), ("i16", 16), ("i32", 32), ("i64", 64)]
    for width, bits in widths:
        for op_name, op_symbol in [("add", "+"), ("sub", "-"), ("mul", "*")]:
            a = 7 if bits > 8 else 7
            b = 3
            value = wrap_signed(
                a + b if op_name == "add" else a - b if op_name == "sub" else a * b,
                bits,
            )
            add_program(
                "C1",
                f"C1-sign-{width}-{op_name}",
                main_program(
                    f"Define local x as {width}.\nSet x to {a}.\n"
                    f"Set x to x {op_symbol} {b}.\nReturn (x as i64) bitand 255.\n"
                ),
                low(value),
            )

    for width, bits in [("u8", 8), ("u16", 16), ("u32", 32), ("u64", 64)]:
        for op_name, op_symbol in [("add", "+"), ("sub", "-"), ("mul", "*")]:
            a = 200 if bits > 8 else 200
            b = 3
            if op_name == "sub":
                a, b = a, b
            value = wrap_unsigned(
                a + b if op_name == "add" else a - b if op_name == "sub" else a * b,
                bits,
            )
            add_program(
                "C1",
                f"C1-uns-{width}-{op_name}",
                main_program(
                    f"Define local x as {width}.\nSet x to {a}.\n"
                    f"Set x to x {op_symbol} {b}.\n"
                    f"Return (x bitand 255) as i64.\n"
                ),
                low(value),
            )

    overflow_cases = [
        ("i8", "127", "+ 1", wrap_signed(128, 8), 8),
        ("i16", "32767", "+ 1", wrap_signed(32768, 16), 16),
        ("i32", "2147483647", "+ 1", wrap_signed(2147483648, 32), 32),
        ("i64", "9223372036854775807", "+ 1", wrap_signed(1 << 63, 64), 64),
        ("u8", "255", "+ 1", wrap_unsigned(256, 8), 8),
        ("u16", "65535", "+ 1", wrap_unsigned(65536, 16), 16),
        ("u32", "4294967295", "+ 1", wrap_unsigned(1 << 32, 32), 32),
        ("u64", "18446744073709551615", "+ 1", 0, 64),
    ]
    for width, start, operation, value, bits in overflow_cases:
        signed = width.startswith("i")
        if signed:
            body = (
                f"Define local x as {width}.\nSet x to {start}.\n"
                f"Set x to x {operation}.\n"
                "Return (((x as i64) bitand 255) + (((x as i64) shr 8) bitand 255)) bitand 255.\n"
            )
            expected = low(low(value) + low(value >> 8))
        else:
            body = (
                f"Define local x as {width}.\nSet x to {start}.\n"
                f"Set x to x {operation}.\n"
                "Return ((x bitand 255) + ((x shr 8) bitand 255)) as i64.\n"
            )
            expected = low(low(value) + low(value >> 8))
        add_program("C1", f"C1-wrap-{width}", main_program(body), expected)
        _ = bits

    negative_cases = [
        ("C1-div-neg7-2", "-7 / 2", wrap_signed(-3, 64)),
        ("C1-mod-neg7-2", "-7 % 2", wrap_signed(-1, 64)),
        ("C1-div-7-neg2", "7 / -2", wrap_signed(-3, 64)),
        ("C1-mod-7-neg2", "7 % -2", wrap_signed(1, 64)),
        ("C1-div-neg7-neg2", "-7 / -2", 3),
        ("C1-mod-neg7-neg2", "-7 % -2", wrap_signed(-1, 64)),
    ]
    for ident, expression, value in negative_cases:
        add_program(
            "C1",
            ident,
            main_program(
                f"Define local x as i64.\nSet x to {expression.replace(' / ', ' / ').replace(' % ', ' mod ')}.\n"
                "Return (x bitand 255) as i64.\n"
            ),
            low(value),
        )

    shift_cases = [
        ("C1-shl-i64-63", "i64", "1", "shl", "63", wrap_signed(1 << 63, 64), "signed"),
        ("C1-shr-i64-neg8", "i64", "-8", "shr", "1", -4, "signed"),
        ("C1-shr-u64-msb", "u64", "0x8000000000000000", "shr", "63", 1, "unsigned"),
        ("C1-shr-u32-msb", "u32", "0x80000000", "shr", "31", 1, "unsigned"),
        ("C1-shr-u8-msb", "u8", "0x80", "shr", "7", 1, "unsigned"),
        ("C1-shr-i16-neg2", "i16", "-2", "shr", "1", -1, "signed"),
    ]
    for ident, width, value, op, count, result, sign in shift_cases:
        body = (
            f"Define local x as {width}.\nDefine local n as {width}.\n"
            f"Set x to {value}.\nSet n to {count}.\nSet x to x {op} n.\n"
        )
        if sign == "signed":
            body += "Return (((x as i64) bitand 255) + (((x as i64) shr 8) bitand 255)) bitand 255.\n"
            expected = low(low(result) + low(result >> 8))
        else:
            body += "Return ((x bitand 255) + ((x shr 8) bitand 255)) as i64.\n"
            expected = low(low(result) + low(result >> 8))
        add_program("C1", ident, main_program(body), expected)

    narrow_cases = [
        ("i8", "-5", -5, 5),
        ("u8", "250", 250, 1),
        ("i16", "-1000", -1000, 24),
        ("u16", "60000", 60000, 96),
        ("i32", "-100000", -100000, 96),
        ("u32", "4000000000", 4000000000, 0),
    ]
    for width, literal, value, extra in narrow_cases:
        signed = width.startswith("i")
        if signed:
            body = (
                f"Define struct S with v as {width}.\ns is S.\n"
                f"Set s.v to {literal}.\n"
                f"Return ((s.v as i64) + {extra}) bitand 255.\n"
            )
            expected = low(value + extra)
        else:
            body = (
                f"Define struct S with v as {width}.\ns is S.\n"
                f"Set s.v to {literal}.\n"
                f"Return ((s.v as i64) + {extra}) bitand 255.\n"
            )
            expected = low(value + extra)
        add_program("C1", f"C1-narrow-{width}", main_program(body), expected)

    cast_cases = [
        ("C1-cast-i8-i64", "i8", "-5", -5, 251),
        ("C1-cast-u8-i64", "u8", "250", 250, 250),
        ("C1-cast-i64-i8", "i64", "300", 44, 44),
        ("C1-cast-u32-i64", "u32", "4000000000", 4000000000, low(4000000000 >> 8)),
    ]
    for ident, source, literal, value, expected in cast_cases:
        if source == "u32":
            body = (
                f"Define local x as {source}.\nSet x to {literal}.\n"
                "Return (((x as i64) shr 8) bitand 255).\n"
            )
        elif source == "i64":
            body = (
                f"Define local x as {source}.\nSet x to {literal}.\n"
                "Define local y as i8.\nSet y to x as i8.\n"
                "Return (y as i64) bitand 255.\n"
            )
        else:
            body = (
                f"Define local x as {source}.\nSet x to {literal}.\n"
                "Return (x as i64) bitand 255.\n"
            )
        add_program("C1", ident, main_program(body), expected)
        _ = value

    add_program(
        "C1",
        "C1-f64-negzero-bits",
        main_program(
            "Define union F with f as f64 and u as u64.\nx is F.\n"
            "Set x.f to -0.0.\nReturn (x.u shr 63) as i64.\n"
        ),
        1,
    )
    add_program(
        "C1",
        "C1-f64-nan-eq",
        main_program(
            "Define union F with f as f64 and u as u64.\nx is F.\n"
            "Set x.f to 0.0 / 0.0.\nIf x.f is x.f:\n    Return 1.\nReturn 0.\n"
        ),
        0,
    )
    add_program(
        "C1",
        "C1-f64-nan-ne",
        main_program(
            "Define union F with f as f64 and u as u64.\nx is F.\n"
            "Set x.f to 0.0 / 0.0.\nIf x.f is not x.f:\n    Return 1.\nReturn 0.\n"
        ),
        1,
    )
    add_program(
        "C1",
        "C1-u64-f64-msb",
        main_program(
            "Define union F with u as u64 and f as f64.\nx is F.\n"
            "Set x.u to 18446744073709551615.\nSet x.f to x.u as f64.\n"
            "Return ((x.u shr 52) as i64) bitand 255.\n"
        ),
        0x43F & 0xFF,
    )
    add_program(
        "C1",
        "C1-i64-min-div-neg1",
        main_program(
            "x is -9223372036854775807 - 1.\n"
            "If x / -1 is x:\n    Return 1.\nReturn 0.\n"
        ),
        1,
    )
    add_program(
        "C1",
        "C1-i64-min-mod-neg1",
        main_program(
            "x is -9223372036854775807 - 1.\n"
            "If x mod -1 is 0:\n    Return 1.\nReturn 0.\n"
        ),
        1,
    )


def write_specs() -> None:
    CORPORA.mkdir(parents=True, exist_ok=True)
    with SPECS.open("w", encoding="utf-8") as handle:
        for spec in PROGRAMS:
            handle.write(json.dumps(spec, sort_keys=True) + "\n")
    print(f"generated {len(PROGRAMS)} programs")
    by_tier: dict[str, int] = {}
    for spec in PROGRAMS:
        by_tier[spec["tier"]] = by_tier.get(spec["tier"], 0) + 1
    print(" ".join(f"{tier}={count}" for tier, count in sorted(by_tier.items())))


def main() -> int:
    tier_c1()
    tier_c2()
    tier_c3()
    tier_c4()
    tier_c5()
    tier_c6()
    tier_c7()
    write_specs()
    return 0


# ---------------------------------------------------------------------------
# Tier C2 — control flow, scoping, short-circuit, nesting
# ---------------------------------------------------------------------------

def tier_c2() -> None:
    loop_cases = [
        (3, 4, 2), (4, 3, 3), (5, 5, 2), (6, 4, 5), (4, 6, 3),
        (3, 7, 4), (7, 3, 2), (5, 4, 4), (6, 6, 2), (8, 3, 3),
        (4, 8, 5), (5, 7, 3), (7, 5, 4), (6, 7, 2), (9, 4, 6),
    ]
    for index, (outer, inner, modulus) in enumerate(loop_cases, 1):
        total = 0
        i = 0
        while True:
            i += 1
            if i >= outer:
                break
            j = 0
            while True:
                j += 1
                if j >= inner:
                    break
                if (i + j) % modulus == 0:
                    continue
                total += 1
        body = (
            "total is 0.\ni is 0.\n"
            "Loop:\n"
            "    i += 1.\n"
            f"    If i is at least {outer}, Break.\n"
            "    j is 0.\n"
            "    Loop:\n"
            "        j += 1.\n"
            f"        If j is at least {inner}, Break.\n"
            f"        If (i + j) mod {modulus} is 0, Continue.\n"
            "        total += 1.\n"
            "Return total.\n"
        )
        add_program("C2", f"C2-nested-loop-{index:02d}", main_program(body), low(total))

    break_continue = [
        (
            "C2-break-inner",
            "a is 0.\nb is 0.\n"
            "Loop:\n    a += 1.\n    If a is at least 3, Break.\n"
            "    Loop:\n        b += 1.\n        If b is at least 2, Break.\n"
            "Return a * 10 + b.\n",
            33,
        ),
        (
            "C2-continue-inner",
            "total is 0.\ni is 0.\n"
            "Loop:\n    i += 1.\n    If i is at least 6, Break.\n"
            "    If i mod 2 is 0, Continue.\n    total += i.\n"
            "Return total.\n",
            9,
        ),
        (
            "C2-three-level",
            "total is 0.\ni is 0.\n"
            "Loop:\n    i += 1.\n    If i is at least 3, Break.\n"
            "    j is 0.\n    Loop:\n        j += 1.\n        If j is at least 3, Break.\n"
            "        k is 0.\n        Loop:\n            k += 1.\n"
            "            If k is at least 3, Break.\n            total += 1.\n"
            "Return total.\n",
            8,
        ),
        (
            "C2-break-only-innermost",
            "outer is 0.\ninner is 0.\n"
            "Loop:\n    outer += 1.\n    If outer is at least 2, Break.\n"
            "    Loop:\n        inner += 1.\n        Break.\n"
            "Return outer * 10 + inner.\n",
            21,
        ),
        (
            "C2-continue-outer",
            "total is 0.\ni is 0.\n"
            "Loop:\n    i += 1.\n    If i is at least 5, Break.\n"
            "    If i is 2, Continue.\n    If i is 4, Continue.\n    total += i.\n"
            "Return total.\n",
            4,
        ),
    ]
    for ident, body, expected in break_continue:
        add_program("C2", ident, main_program(body), expected)

    early_return = [
        ("C2-return-early-5", 5, 50),
        ("C2-return-early-9", 9, 90),
        ("C2-return-nested-12", 12, 112),
        ("C2-return-nested-20", 20, 120),
        ("C2-return-nested-50", 50, 150),
    ]
    for ident, n, expected in early_return:
        if n <= 9:
            body = (
                "Define function f taking n as i64 returning i64:\n"
                "    i is 0.\n    Loop:\n        i += 1.\n"
                "        If i is at least n:\n            Return i * 10.\n"
                "    Return 0.\n"
                f"Return f({n}).\n"
            )
        else:
            body = (
                "Define function f taking n as i64 returning i64:\n"
                "    i is 0.\n    Loop:\n        i += 1.\n        j is 0.\n"
                "        Loop:\n            j += 1.\n"
                "            If i * j is at least n:\n                Return i * 100 + j.\n"
                "        If i is at least 10:\n            Break.\n"
                "    Return 0.\n"
                f"Return f({n}).\n"
            )
        add_program("C2", ident, main_program(body), expected)

    short_circuit = [
        ("C2-sc-and-false", "If inc(0) and inc(1):\n    Return 99.\nReturn calls.\n", 1),
        ("C2-sc-and-true", "If inc(1) and inc(1):\n    Return calls.\nReturn 0.\n", 2),
        ("C2-sc-or-true", "If inc(1) or inc(0):\n    Return calls.\nReturn 0.\n", 1),
        ("C2-sc-or-false", "If inc(0) or inc(1):\n    Return calls.\nReturn 0.\n", 2),
        ("C2-sc-mixed-1", "If (inc(0) and inc(1)) or inc(1):\n    Return calls.\nReturn 0.\n", 2),
        ("C2-sc-mixed-2", "If (inc(1) or inc(0)) and inc(0):\n    Return 99.\nReturn calls.\n", 2),
        ("C2-sc-not", "If not inc(0) and inc(1):\n    Return calls.\nReturn 0.\n", 2),
        ("C2-sc-mixed-3", "If inc(0) or (inc(1) and inc(0)):\n    Return 99.\nReturn calls.\n", 3),
        ("C2-sc-yes", "If yes() and inc(0):\n    Return 99.\nReturn calls.\n", 2),
        ("C2-sc-double", "If inc(1) and inc(1) and inc(0):\n    Return 99.\nReturn calls.\n", 3),
    ]
    prefix = (
        "Define global calls as i64.\n"
        "Define function inc taking v as i64 returning bool:\n"
        "    calls += 1.\n    Return v is not 0.\n"
        "Define function yes returning bool:\n"
        "    calls += 1.\n    Return true.\n"
    )
    for ident, tail, expected in short_circuit:
        add_program("C2", ident, main_program(prefix + tail), expected)

    add_program(
        "C2",
        "C2-null-short-circuit",
        main_program(
            "Define local p as ptr<i64>.\n"
            "If p is not null and *p is 42:\n    Return 99.\nReturn 1.\n"
        ),
        1,
    )
    add_program(
        "C2",
        "C2-null-short-circuit-nonnull",
        main_program(
            "x is 42.\nDefine local p as ptr<i64>.\nSet p to address of x.\n"
            "If p is not null and *p is 42:\n    Return 99.\nReturn 1.\n"
        ),
        99,
    )

    shadow_programs = [
        (7, ["x is 1.5.", "x is true.", "x is 9."]),
        (11, ["x is true.", "x is 2.5.", "x is 4.", "x is false."]),
        (3, ["x is 1.5.", "x is 9.", "x is true.", "x is 6.", "x is 2.5."]),
        (21, ["x is 2.5.", "x is 4.", "x is true."]),
        (5, ["x is 1.5.", "x is 9."]),
    ]
    for index, (outer, assignments) in enumerate(shadow_programs, 1):
        body = f"x is {outer}.\n"
        indent = ""
        for value in assignments:
            body += f"{indent}If true:\n"
            indent += "    "
            body += f"{indent}{value}\n"
        body += "Return x.\n"
        add_program("C2", f"C2-shadow-{index}", main_program(body), outer)

    where_programs = [
        (
            "C2-where-untaken",
            "x is 0.\nIf false:\n    Add 10 to x where x is 5.\nReturn x.\n",
            0,
        ),
        (
            "C2-where-taken-shadow",
            "x is 0.\ntotal is 0.\nIf true:\n    Add x to total where x is 6.\nReturn x + total.\n",
            6,
        ),
        (
            "C2-where-multi",
            "Return sum of a and b where a is 3 and b is 4.\n",
            7,
        ),
        (
            "C2-where-in-loop",
            "total is 0.\ni is 0.\nLoop:\n    i += 1.\n    If i is at least 3, Break.\n"
            "    Add y to total where y is i * 10.\nReturn total.\n",
            30,
        ),
        (
            "C2-where-if-else",
            "x is 0.\nIf x is 0:\n    Add 5 to x where x is 1.\nElse:\n    Add 9 to x.\nReturn x.\n",
            0,
        ),
    ]
    for ident, body, expected in where_programs:
        add_program("C2", ident, main_program(body), expected)

    unreachable = [
        (
            "C2-unreachable-return",
            "Define function f returning i64:\n    Return 5.\n    Return 9.\nReturn f().\n",
            5,
        ),
        (
            "C2-unreachable-after-break",
            "x is 0.\nLoop:\n    x += 1.\n    Break.\n    x += 100.\nReturn x.\n",
            1,
        ),
        (
            "C2-unreachable-top",
            "Return 5.\nReturn 9.\n",
            5,
        ),
    ]
    for ident, body, expected in unreachable:
        add_program("C2", ident, main_program(body), expected)


# ---------------------------------------------------------------------------
# Tier C3 — System V ABI, spillover, SRET aggregates, indirect calls
# ---------------------------------------------------------------------------

def tier_c3() -> None:
    for count in [0, 1, 2, 6, 7, 8, 10, 12]:
        if count == 0:
            body = "Define function f returning i64:\n    Return 7.\nReturn f().\n"
            expected = 7
        else:
            params = " and ".join(f"a{i} as i64" for i in range(count))
            expression = " + ".join(f"a{i}" for i in range(count))
            call = ", ".join(str(i + 1) for i in range(count))
            body = (
                f"Define function f taking {params} returning i64:\n"
                f"    Return {expression}.\nReturn f({call}).\n"
            )
            expected = count * (count + 1) // 2
        add_program("C3", f"C3-int-args-{count:02d}", main_program(body), low(expected))

    for count in [1, 8, 9, 10, 12]:
        params = " and ".join(f"f{i} as f64" for i in range(count))
        expression = " + ".join(f"f{i}" for i in range(count))
        call = ", ".join("1.5" for _ in range(count))
        body = (
            f"Define function f taking {params} returning i64:\n"
            f"    Return ({expression}) as i64.\nReturn f({call}).\n"
        )
        add_program("C3", f"C3-float-args-{count:02d}", main_program(body), int(1.5 * count))

    mixed = (
        "x is 0.\n"
        "Define function mixed taking a as i8 and b as f32 and c as i64 and d as f64 "
        "and e as u16 and f as f64 and g as ptr<i64> and h as i32 and i as f32 "
        "and j as u64 and k as f64 and l as i64 returning i64:\n"
        "    Return (a as i64) + (b as i64) + c + (d as i64) + (e as i64) "
        "+ (f as i64) + (h as i64) + (i as i64) + (j as i64) + (k as i64) + l.\n"
        "Return mixed(1 as i8, 2.5 as f32, 3, 4.5, 5 as u16, 6.5, address of x, -8 as i32, 9.5 as f32, 10 as u64, 11.5, 12).\n"
    )
    add_program("C3", "C3-mixed-12", main_program(mixed), 55)

    nested = [
        (
            "C3-nested-calls-1",
            "Define function f taking a as i64 returning i64:\n    Return a + 1.\n"
            "Define function g taking a as i64 and b as i64 returning i64:\n    Return a * b.\n"
            "Define function h taking a as i64 and b as i64 and c as i64 returning i64:\n"
            "    Return a + b + c.\n"
            "Return f(g(2, 3) + h(4, 5, 6)).\n",
            22,
        ),
        (
            "C3-nested-calls-2",
            "Define function f taking a as i64 returning i64:\n    Return a + 1.\n"
            "Return f(f(f(1))).\n",
            4,
        ),
        (
            "C3-nested-calls-3",
            "Define function g taking a as i64 and b as i64 returning i64:\n    Return a * b.\n"
            "Return g(g(2, 3), g(4, 5)).\n",
            120,
        ),
        (
            "C3-nested-calls-4",
            "Define function h taking a as i64 and b as i64 and c as i64 returning i64:\n"
            "    Return a + b + c.\n"
            "Return h(h(1, 2, 3), h(4, 5, 6), h(7, 8, 9)).\n",
            45,
        ),
    ]
    for ident, body, expected in nested:
        add_program("C3", ident, main_program(body), low(expected))

    sizes = [1, 3, 7, 8, 9, 12, 16, 17, 24, 32, 48, 64, 128, 256]
    for size in sizes:
        if size == 1:
            definition = "Define struct S1 with tag as u8.\n"
            sum_body = (
                "Define function sum taking s as S1 returning i64:\n"
                "    Return s.tag as i64.\n"
                "s is S1.\nSet s.tag to 5.\nReturn sum(s).\n"
            )
            make_body = (
                "Define function make taking t as u8 returning S1:\n"
                "    r is S1.\n    Set r.tag to t.\n    Return r.\n"
                "r is make(5).\nReturn r.tag as i64.\n"
            )
            expected = 5
        else:
            last = size - 2
            definition = f"Define struct S{size} with tag as u8 and data as array<u8, {size - 1}>.\n"
            sum_body = (
                f"Define function sum taking s as S{size} returning i64:\n"
                f"    Return (s.tag as i64) + (s.data[0] as i64) + (s.data[{last}] as i64).\n"
                f"s is S{size}.\nSet s.tag to 5.\nSet s.data[0] to 3.\n"
                f"Set s.data[{last}] to 4.\nReturn sum(s).\n"
            )
            make_body = (
                f"Define function make taking t as u8 returning S{size}:\n"
                f"    r is S{size}.\n    Set r.tag to t.\n    Set r.data[0] to 3.\n"
                f"    Set r.data[{last}] to 4.\n    Return r.\n"
                f"r is make(5).\nReturn (r.tag as i64) + (r.data[0] as i64) + (r.data[{last}] as i64).\n"
            )
            expected = 12
        add_program("C3", f"C3-sret-size-{size:03d}-pass", main_program(definition + sum_body), expected)
        add_program("C3", f"C3-sret-size-{size:03d}-return", main_program(definition + make_body), expected)

    fnptr = [
        (
            "C3-fnptr-local",
            "Define function add taking a as i64 and b as i64 returning i64:\n    Return a + b.\n"
            "fp is function add.\nReturn fp(2, 3).\n",
            5,
        ),
        (
            "C3-fnptr-struct",
            "Define function add taking a as i64 and b as i64 returning i64:\n    Return a + b.\n"
            "Define struct Ops with cmp as fn(i64, i64) -> i64.\n"
            "ops is Ops.\nSet ops.cmp to function add.\nReturn ops.cmp(2, 3).\n",
            5,
        ),
        (
            "C3-fnptr-array",
            "Define function add taking a as i64 and b as i64 returning i64:\n    Return a + b.\n"
            "Define function mul taking a as i64 and b as i64 returning i64:\n    Return a * b.\n"
            "Define global table as array<fn(i64, i64) -> i64, 2>.\n"
            "Set table[0] to function add.\nSet table[1] to function mul.\n"
            "Return table[0](2, 3) + table[1](4, 5).\n",
            25,
        ),
        (
            "C3-fnptr-global",
            "Define function add taking a as i64 and b as i64 returning i64:\n    Return a + b.\n"
            "Define global fp as fn(i64, i64) -> i64.\nSet fp to function add.\nReturn fp(6, 7).\n",
            13,
        ),
        (
            "C3-fnptr-eight-args",
            "Define function sum8 taking a as i64 and b as i64 and c as i64 and d as i64 "
            "and e as i64 and f as i64 and g as i64 and h as i64 returning i64:\n"
            "    Return a + b + c + d + e + f + g + h.\n"
            "Define global fp as fn(i64, i64, i64, i64, i64, i64, i64, i64) -> i64.\n"
            "Set fp to function sum8.\nReturn fp(1, 2, 3, 4, 5, 6, 7, 8).\n",
            36,
        ),
    ]
    for ident, body, expected in fnptr:
        add_program("C3", ident, main_program(body), low(expected))

    recursion = [
        (
            "C3-recursion-sum-2000",
            "Define function total taking n as i64 returning i64:\n"
            "    If n is 0:\n        Return 0.\n    Return n + total(n - 1).\n"
            "Return total(2000).\n",
            low(2000 * 2001 // 2),
        ),
        (
            "C3-ackermann-3-4",
            "Define function ack taking m as i64 and n as i64 returning i64:\n"
            "    If m is 0:\n        Return n + 1.\n"
            "    If n is 0:\n        Return ack(m - 1, 1).\n"
            "    Return ack(m - 1, ack(m, n - 1)).\n"
            "Return ack(3, 4).\n",
            125,
        ),
        (
            "C3-mutual-3way",
            "Define function a taking n as i64 returning i64:\n"
            "    If n is 0:\n        Return 1.\n    Return b(n - 1).\n"
            "Define function b taking n as i64 returning i64:\n"
            "    If n is 0:\n        Return 1.\n    Return c(n - 1).\n"
            "Define function c taking n as i64 returning i64:\n"
            "    If n is 0:\n        Return 1.\n    Return a(n - 1).\n"
            "Return a(9).\n",
            1,
        ),
    ]
    for ident, body, expected in recursion:
        add_program("C3", ident, main_program(body), expected)

# ---------------------------------------------------------------------------
# Tier C4 — layout, unions, MemCopy, mmap, syscalls, allocator
# ---------------------------------------------------------------------------

def tier_c4() -> None:
    layout_cases = [
        (
            "C4-layout-odd1",
            "Define struct Odd1 with a as u8 and b as i64 and c as u8.\n",
            24,
            8,
        ),
        (
            "C4-layout-odd2",
            "Define struct Odd2 with a as u8 and b as u16 and c as u8 and d as u32 and e as u8.\n",
            16,
            4,
        ),
        (
            "C4-layout-nested",
            "Define struct Odd1 with a as u8 and b as i64 and c as u8.\n"
            "Define struct Nested with tag as u8 and inner as Odd1 and tail as u16.\n",
            40,
            8,
        ),
        (
            "C4-layout-packed3",
            "Define struct Packed3 with a as u8 and b as u8 and c as u8.\n",
            3,
            1,
        ),
        (
            "C4-layout-three-u16",
            "Define struct Three with a as u16 and b as u16 and c as u16.\n",
            6,
            2,
        ),
        (
            "C4-layout-i32-tail",
            "Define struct Four with a as i32 and b as i32 and c as i32 and d as i8.\n",
            16,
            4,
        ),
    ]
    for ident, definitions, size, align in layout_cases:
        name = definitions.split("Define struct ")[-1].split()[0]
        body = definitions + (
            f"Return ((size of {name}) as i64) + ((align of {name}) as i64).\n"
        )
        add_program("C4", ident, main_program(body), size + align)

    field_cases = [
        (
            "C4-fields-odd1",
            "Define struct Odd1 with a as u8 and b as i64 and c as u8.\nx is Odd1.\n"
            "Set x.a to 1.\nSet x.b to 2.\nSet x.c to 3.\n"
            "Return (x.a as i64) + x.b + (x.c as i64).\n",
            6,
        ),
        (
            "C4-fields-odd2",
            "Define struct Odd2 with a as u8 and b as u16 and c as u8 and d as u32 and e as u8.\n"
            "x is Odd2.\nSet x.a to 1.\nSet x.b to 2.\nSet x.c to 3.\n"
            "Set x.d to 4.\nSet x.e to 5.\n"
            "Return (x.a as i64) + (x.b as i64) + (x.c as i64) + (x.d as i64) + (x.e as i64).\n",
            15,
        ),
        (
            "C4-fields-nested",
            "Define struct Odd1 with a as u8 and b as i64 and c as u8.\n"
            "Define struct Nested with tag as u8 and inner as Odd1 and tail as u16.\n"
            "x is Nested.\nSet x.tag to 1.\nSet x.inner.a to 2.\nSet x.inner.b to 3.\n"
            "Set x.inner.c to 4.\nSet x.tail to 5.\n"
            "Return (x.tag as i64) + (x.inner.a as i64) + x.inner.b + (x.inner.c as i64) + (x.tail as i64).\n",
            15,
        ),
        (
            "C4-fields-global",
            "Define struct Odd1 with a as u8 and b as i64 and c as u8.\n"
            "Define global g as Odd1.\nSet g.a to 7.\nSet g.b to 8.\nSet g.c to 9.\n"
            "Return (g.a as i64) + g.b + (g.c as i64).\n",
            24,
        ),
        (
            "C4-fields-pointer",
            "Define struct Odd1 with a as u8 and b as i64 and c as u8.\n"
            "g is Odd1.\nDefine local p as ptr<Odd1>.\nSet p to address of g.\n"
            "Set p.a to 4.\nSet p.b to 5.\nSet p.c to 6.\n"
            "Return (g.a as i64) + g.b + (g.c as i64).\n",
            15,
        ),
        (
            "C4-fields-array-element",
            "Define struct Odd1 with a as u8 and b as i64 and c as u8.\n"
            "Define global arr as array<Odd1, 4>.\nSet arr[2].a to 3.\nSet arr[2].b to 4.\n"
            "Set arr[2].c to 5.\n"
            "Return (arr[2].a as i64) + arr[2].b + (arr[2].c as i64).\n",
            12,
        ),
        (
            "C4-fields-union",
            "Define union U with f as f64 and u as u64.\nx is U.\nSet x.f to 1.5.\n"
            "Return ((x.u shr 52) as i64) bitand 255.\n",
            255,
        ),
        (
            "C4-fields-union-f32",
            "Define union U with f as f32 and u as u32.\nx is U.\nSet x.f to 1.5.\n"
            "Return ((x.u shr 24) as i64) bitand 255.\n",
            63,
        ),
    ]
    for ident, body, expected in field_cases:
        add_program("C4", ident, main_program(body), expected)

    stride_cases = [
        (16, 4, lambda i: i * 3),
        (8, 7, lambda i: i + 1),
        (32, 2, lambda i: i * i),
        (4, 11, lambda i: i % 3),
        (12, 5, lambda i: i * 2),
        (7, 9, lambda i: 10 - i),
    ]
    for index, (count, stride, value) in enumerate(stride_cases, 1):
        expected = sum(int(value(i)) for i in range(count)) & 0xFF
        body = (
            "Define struct Odd1 with a as u8 and b as i64 and c as u8.\n"
            f"Define global arr as array<Odd1, {count}>.\n"
            "i is 0.\ntotal is 0.\n"
            "Loop:\n"
            f"    If i is at least {count}, Break.\n"
            "    Set arr[i].a to i as u8.\n"
            "    Set arr[i].b to i * 3.\n"
            "    Set arr[i].c to (i + 1) as u8.\n"
            "    total += arr[i].b.\n"
            "    i += 1.\n"
            "Return total bitand 255.\n"
        )
        expected = sum(i * 3 for i in range(count)) & 0xFF
        add_program("C4", f"C4-stride-{index}", main_program(body), expected)
        _ = stride

    memcopy_sizes = [0, 1, 7, 8, 15, 16, 63, 64, 257, 1024]
    for size in memcopy_sizes:
        if size == 0:
            body = (
                "Define struct Empty with data as array<u8, 0>.\n"
                "src is Empty.\ndst is Empty.\n"
                "Copy 0 bytes from address of src to address of dst.\nReturn 0.\n"
            )
            expected = 0
        elif size == 1:
            body = (
                "Define struct Buf1 with data as array<u8, 1>.\n"
                "src is Buf1.\ndst is Buf1.\nSet src.data[0] to 9.\n"
                "Copy 1 bytes from address of src to address of dst.\n"
                "Return dst.data[0] as i64.\n"
            )
            expected = 9
        else:
            last = size - 1
            body = (
                f"Define struct Buf{size} with data as array<u8, {size}>.\n"
                f"src is Buf{size}.\ndst is Buf{size}.\n"
                "Set src.data[0] to 1.\n"
                f"Set src.data[{last}] to 2.\n"
                f"Copy {size} bytes from address of src to address of dst.\n"
                f"Return (dst.data[0] as i64) + (dst.data[{last}] as i64).\n"
            )
            expected = 3
        add_program("C4", f"C4-memcopy-{size:04d}", main_program(body), expected)

    mmap_cases = [
        (
            "C4-mmap-single-page",
            "Define global p as ptr<u8>.\n"
            "Set p to syscall(9, 0, 8192, 3, 0x22, -1, 0) as u64 as ptr<u8>.\n"
            "Set p[0] to 11.\nSet p[4096] to 22.\nSet p[8191] to 33.\n"
            "Return (p[0] as i64) + (p[4096] as i64) + (p[8191] as i64).\n",
            66,
        ),
        (
            "C4-mmap-eight-pages",
            "Define global ptrs as array<ptr<u8>, 8>.\ni is 0.\n"
            "Loop:\n    If i is at least 8, Break.\n"
            "    ptrs[i] = syscall(9, 0, 4096, 3, 0x22, -1, 0) as u64 as ptr<u8>.\n"
            "    Set ptrs[i][0] to (i + 1) as u8.\n"
            "    i += 1.\n"
            "total is 0.\ni is 0.\n"
            "Loop:\n    If i is at least 8, Break.\n"
            "    total += ptrs[i][0] as i64.\n    i += 1.\n"
            "Return total.\n",
            36,
        ),
        (
            "C4-mmap-munmap-preserves",
            "Define global a as ptr<u8>.\nDefine global b as ptr<u8>.\n"
            "Set a to syscall(9, 0, 4096, 3, 0x22, -1, 0) as u64 as ptr<u8>.\n"
            "Set b to syscall(9, 0, 4096, 3, 0x22, -1, 0) as u64 as ptr<u8>.\n"
            "Set a[0] to 7.\nSet b[0] to 9.\n"
            "syscall(11, a, 4096).\n"
            "Return (b[0] as i64) + 1.\n",
            10,
        ),
        (
            "C4-mmap-reallocate",
            "Define global p as ptr<u8>.\n"
            "Set p to syscall(9, 0, 4096, 3, 0x22, -1, 0) as u64 as ptr<u8>.\n"
            "Set p[0] to 42.\n"
            "syscall(11, p, 4096).\n"
            "Set p to syscall(9, 0, 4096, 3, 0x22, -1, 0) as u64 as ptr<u8>.\n"
            "Set p[0] to 5.\nReturn p[0] as i64.\n",
            5,
        ),
        (
            "C4-mmap-large-page",
            "Define global p as ptr<u8>.\n"
            "Set p to syscall(9, 0, 65536, 3, 0x22, -1, 0) as u64 as ptr<u8>.\n"
            "Set p[0] to 1.\nSet p[32768] to 2.\nSet p[65535] to 3.\n"
            "Return (p[0] as i64) + (p[32768] as i64) + (p[65535] as i64).\n",
            6,
        ),
        (
            "C4-mmap-pointer-array",
            "Define global ptrs as array<ptr<u8>, 4>.\ni is 0.\n"
            "Loop:\n    If i is at least 4, Break.\n"
            "    ptrs[i] = syscall(9, 0, 4096, 3, 0x22, -1, 0) as u64 as ptr<u8>.\n"
            "    Set ptrs[i][i] to (i + 10) as u8.\n    i += 1.\n"
            "i is 0.\ntotal is 0.\n"
            "Loop:\n    If i is at least 4, Break.\n"
            "    total += ptrs[i][i] as i64.\n    i += 1.\n"
            "Return total.\n",
            10 + 11 + 12 + 13,
        ),
        (
            "C4-mmap-overlap-free",
            "Define global a as ptr<u8>.\nDefine global b as ptr<u8>.\n"
            "Set a to syscall(9, 0, 4096, 3, 0x22, -1, 0) as u64 as ptr<u8>.\n"
            "Set b to syscall(9, 0, 8192, 3, 0x22, -1, 0) as u64 as ptr<u8>.\n"
            "Set a[0] to 5.\nSet b[0] to 6.\nSet b[4096] to 7.\n"
            "Return (a[0] as i64) + (b[0] as i64) + (b[4096] as i64).\n",
            18,
        ),
        (
            "C4-mmap-eight-kilobyte",
            "Define global p as ptr<u8>.\n"
            "Set p to syscall(9, 0, 8192, 3, 0x22, -1, 0) as u64 as ptr<u8>.\n"
            "Set p[0] to 3.\nSet p[8191] to 4.\n"
            "Return (p[0] as i64) + (p[8191] as i64).\n",
            7,
        ),
    ]
    for ident, body, expected in mmap_cases:
        add_program("C4", ident, main_program(body), expected)

    add_program(
        "C4",
        "C4-stdout-write",
        main_program(
            "syscall(1, 1, bytes_ptr(\"bt-out\\n\"), bytes_len(\"bt-out\\n\")).\nReturn 0.\n"
        ),
        0,
        stdout=b"bt-out\n",
    )
    add_program(
        "C4",
        "C4-stderr-write",
        main_program(
            "syscall(1, 2, bytes_ptr(\"bt-err\\n\"), bytes_len(\"bt-err\\n\")).\nReturn 0.\n"
        ),
        0,
    )
    add_program(
        "C4",
        "C4-stdin-echo",
        main_program(
            "Define global buf as ptr<u8>.\n"
            "Set buf to syscall(9, 0, 64, 3, 0x22, -1, 0) as u64 as ptr<u8>.\n"
            "n is syscall(0, 0, buf, 16) as i64.\n"
            "syscall(1, 1, buf, n as u64).\nReturn n.\n"
        ),
        3,
        stdout=b"abc",
        stdin=b"abc",
    )
    add_program(
        "C4",
        "C4-stdin-two-chunks",
        main_program(
            "Define global buf as ptr<u8>.\n"
            "Set buf to syscall(9, 0, 64, 3, 0x22, -1, 0) as u64 as ptr<u8>.\n"
            "a is syscall(0, 0, buf, 2) as i64.\n"
            "b is syscall(0, 0, address of buf[2], 2) as i64.\n"
            "syscall(1, 1, buf, (a + b) as u64).\nReturn a + b.\n"
        ),
        4,
        stdout=b"abcd",
        stdin=b"abcd",
    )
    add_program(
        "C4",
        "C4-argc",
        main_program(
            "Define function main taking argc as i64 and argv as ptr<ptr<u8>> "
            "and envp as ptr<ptr<u8>> returning i64:\n"
            "    Return argc.\n"
        ),
        3,
        args=["alpha", "beta"],
    )
    add_program(
        "C4",
        "C4-argv-byte",
        main_program(
            "Define function main taking argc as i64 and argv as ptr<ptr<u8>> "
            "and envp as ptr<ptr<u8>> returning i64:\n"
            "    Return argv[1][0] as i64.\n"
        ),
        65,
        args=["A"],
    )
    add_program(
        "C4",
        "C4-argv-sum",
        main_program(
            "Define function main taking argc as i64 and argv as ptr<ptr<u8>> "
            "and envp as ptr<ptr<u8>> returning i64:\n"
            "    Return (argv[1][0] as i64) + (argv[2][0] as i64).\n"
        ),
        131,
        args=["A", "B"],
    )

    allocator_template = '''
Define global heap as ptr<u8>.
Define global bump as i64.
Define global free_count as i64.
Define global block_size as i64.
Define global free_list as array<i64, 64>.
Define global offsets as array<i64, 64>.

Define function alloc taking size as i64 returning i64:
    If free_count is more than 0:
        free_count -= 1.
        Return free_list[free_count].
    offset is bump.
    bump += size.
    Return offset.

Define function release taking offset as i64 returning void:
    free_list[free_count] = offset.
    free_count += 1.
    Return.

Set heap to syscall(9, 0, 65536, 3, 0x22, -1, 0) as u64 as ptr<u8>.
Set bump to 0.
Set free_count to 0.
Set block_size to {block_size}.

i is 0.
Loop:
    If i is at least {count}, Break.
    offsets[i] = alloc(block_size).
    Set heap[offsets[i]] to i as u8.
    i += 1.

i is {count} - 1.
Loop:
    If i is less than 0, Break.
    If i mod 2 is 0:
        release(offsets[i]).
    i -= 1.

j is 0.
Loop:
    If j is at least {half}, Break.
    slot is alloc(block_size).
    If slot is not offsets[j * 2]:
        Return 99.
    Set heap[slot] to (100 + j) as u8.
    If heap[slot] is not ((100 + j) as u8):
        Return 98.
    j += 1.

i is 0.
total is 0.
Loop:
    If i is at least {count}, Break.
    If i mod 2 is 1:
        total += heap[offsets[i]] as i64.
    i += 1.
Return (total + 1) bitand 255.
'''
    allocator_cases = [
        (32, 16, 32),
        (64, 16, 32),
        (128, 16, 32),
        (32, 8, 16),
        (64, 32, 64),
        (128, 32, 64),
    ]
    for index, (block_size, half, count) in enumerate(allocator_cases, 1):
        expected = (sum(range(1, count, 2)) + 1) & 0xFF
        body = allocator_template.format(block_size=block_size, half=half, count=count)
        add_program("C4", f"C4-allocator-{index}", main_program(body), expected)

    extra = [
        (
            "C4-extra-array-size",
            "Return (size of array<u16, 7>) as i64.\n",
            14,
        ),
        (
            "C4-extra-struct-align",
            "Define struct S with a as u8 and b as u32.\nReturn (align of S) as i64.\n",
            4,
        ),
        (
            "C4-extra-memcopy-union",
            "Define union U with f as f64 and u as u64.\na is U.\nb is U.\nSet a.f to 2.5.\n"
            "Copy 8 bytes from address of a to address of b.\n"
            "Return b.u as i64 bitand 255.\n",
            0,
        ),
        (
            "C4-extra-odd-array-bytes",
            "Define struct Odd1 with a as u8 and b as i64 and c as u8.\n"
            "Return (size of array<Odd1, 4>) as i64.\n",
            96,
        ),
        (
            "C4-extra-generic-size",
            "Define struct Pair of T and U with first as T and second as U.\n"
            "Return (size of Pair<i64, f64>) as i64.\n",
            16,
        ),
        (
            "C4-extra-union-bytes",
            "Define union U with f as f64 and u as u64 and bytes as array<u8, 8>.\n"
            "x is U.\nSet x.f to 1.5.\n"
            "Return (x.bytes[6] as i64) + (x.bytes[7] as i64).\n",
            55,
        ),
        (
            "C4-extra-memcopy-padding",
            "Define struct Odd1 with a as u8 and b as i64 and c as u8.\n"
            "src is Odd1.\ndst is Odd1.\nSet src.a to 1.\nSet src.b to 2.\nSet src.c to 3.\n"
            "Copy 24 bytes from address of src to address of dst.\n"
            "Return (dst.a as i64) + dst.b + (dst.c as i64).\n",
            6,
        ),
        (
            "C4-extra-argv-four",
            "Define function main taking argc as i64 and argv as ptr<ptr<u8>> "
            "and envp as ptr<ptr<u8>> returning i64:\n"
            "    Return argc.\n",
            5,
            ["a", "b", "c", "d"],
        ),
        (
            "C4-extra-mmap-unaligned",
            "Define global p as ptr<u8>.\n"
            "Set p to syscall(9, 0, 4096, 3, 0x22, -1, 0) as u64 as ptr<u8>.\n"
            "Set p[1] to 9.\nSet p[3] to 10.\n"
            "Return (p[1] as i64) + (p[3] as i64).\n",
            19,
        ),
    ]
    for entry in extra:
        ident, body, expected, *rest = entry
        add_program(
            "C4",
            ident,
            main_program(body),
            expected,
            args=rest[0] if rest else None,
        )

# ---------------------------------------------------------------------------
# Tier C5 — type checker, modules, generics, phrase dispatch
# ---------------------------------------------------------------------------

def tier_c5() -> None:
    point = "Define struct Point with x as i64 and y as i64.\n"
    struct_cases = [
        (
            "C5-pair-i64",
            "Define struct Pair of T and U with first as T and second as U.\n"
            "p is Pair<i64, i64>.\nSet p.first to 7.\nSet p.second to 8.\n"
            "Return p.first + p.second.\n",
            15,
        ),
        (
            "C5-pair-f64-u8",
            "Define struct Pair of T and U with first as T and second as U.\n"
            "p is Pair<f64, u8>.\nSet p.first to 1.5.\nSet p.second to 9.\n"
            "Return (p.first as i64) + (p.second as i64).\n",
            10,
        ),
        (
            "C5-nested-pair",
            "Define struct Pair of T and U with first as T and second as U.\n"
            "p is Pair<i64, Pair<f64, u8>>.\nSet p.first to 3.\n"
            "Set p.second.first to 1.5.\nSet p.second.second to 7.\n"
            "Return p.first + (p.second.first as i64) + (p.second.second as i64).\n",
            11,
        ),
        (
            "C5-box-box-box-point",
            point
            + "Define struct Box of T with value as T.\n"
            "b is Box<Box<Box<Point>>>.\nSet b.value.value.value.x to 5.\n"
            "Set b.value.value.value.y to 6.\n"
            "Return b.value.value.value.x + b.value.value.value.y.\n",
            11,
        ),
        (
            "C5-triple",
            "Define struct Triple of T and U and V with a as T and b as U and c as V.\n"
            "t is Triple<i64, i64, i64>.\nSet t.a to 1.\nSet t.b to 2.\nSet t.c to 3.\n"
            "Return t.a + t.b + t.c.\n",
            6,
        ),
        (
            "C5-result-i64",
            "Define struct Result of T and E with is_ok as bool and ok as T and err as E.\n"
            "r is Result<i64, i64>.\nSet r.is_ok to true.\nSet r.ok to 7.\nSet r.err to 0.\n"
            "If r.is_ok:\n    Return r.ok.\nReturn r.err.\n",
            7,
        ),
        (
            "C5-result-f64",
            "Define struct Result of T and E with is_ok as bool and ok as T and err as E.\n"
            "r is Result<f64, i64>.\nSet r.is_ok to true.\nSet r.ok to 2.5.\nSet r.err to 0.\n"
            "Return r.ok as i64.\n",
            2,
        ),
        (
            "C5-result-point",
            point
            + "Define struct Result of T and E with is_ok as bool and ok as T and err as E.\n"
            "r is Result<Point, i64>.\nSet r.is_ok to true.\nSet r.ok.x to 4.\n"
            "Set r.ok.y to 5.\nSet r.err to 0.\nReturn r.ok.x + r.ok.y.\n",
            9,
        ),
        (
            "C5-option-i64",
            "Define struct Option of T with has_val as bool and val as T.\n"
            "o is Option<i64>.\nSet o.has_val to true.\nSet o.val to 7.\n"
            "If o.has_val:\n    Return o.val.\nReturn 0.\n",
            7,
        ),
        (
            "C5-option-f64",
            "Define struct Option of T with has_val as bool and val as T.\n"
            "o is Option<f64>.\nSet o.has_val to true.\nSet o.val to 3.5.\n"
            "Return o.val as i64.\n",
            3,
        ),
        (
            "C5-option-point",
            point
            + "Define struct Option of T with has_val as bool and val as T.\n"
            "o is Option<Point>.\nSet o.has_val to true.\nSet o.val.x to 8.\n"
            "Set o.val.y to 9.\nReturn o.val.x + o.val.y.\n",
            17,
        ),
        (
            "C5-pair-u8-u16",
            "Define struct Pair of T and U with first as T and second as U.\n"
            "p is Pair<u8, u16>.\nSet p.first to 250.\nSet p.second to 60000.\n"
            "Return ((p.first as i64) + (p.second as i64)) bitand 255.\n",
            90,
        ),
        (
            "C5-pair-bool-i64",
            "Define struct Pair of T and U with first as T and second as U.\n"
            "p is Pair<bool, i64>.\nSet p.first to true.\nSet p.second to 9.\n"
            "If p.first:\n    Return p.second.\nReturn 0.\n",
            9,
        ),
        (
            "C5-pair-point-point",
            point
            + "Define struct Pair of T and U with first as T and second as U.\n"
            "p is Pair<Point, Point>.\nSet p.first.x to 1.\nSet p.first.y to 2.\n"
            "Set p.second.x to 3.\nSet p.second.y to 4.\n"
            "Return p.first.x + p.first.y + p.second.x + p.second.y.\n",
            10,
        ),
        (
            "C5-pair-nested-f64",
            "Define struct Pair of T and U with first as T and second as U.\n"
            "p is Pair<Pair<i64, i64>, f64>.\nSet p.first.first to 2.\n"
            "Set p.first.second to 3.\nSet p.second to 1.5.\n"
            "Return p.first.first + p.first.second + (p.second as i64).\n",
            6,
        ),
        (
            "C5-pair-ptr-i64",
            "Define struct Pair of T and U with first as T and second as U.\n"
            "x is 7.\np is Pair<ptr<i64>, i64>.\nSet p.first to address of x.\n"
            "Set p.second to 3.\nReturn (*p.first) + p.second.\n",
            10,
        ),
        (
            "C5-pair-i64-f64",
            "Define struct Pair of T and U with first as T and second as U.\n"
            "p is Pair<i64, f64>.\nSet p.first to 6.\nSet p.second to 2.5.\n"
            "Return p.first + (p.second as i64).\n",
            8,
        ),
    ]
    for ident, body, expected in struct_cases:
        add_program("C5", ident, main_program(body), expected)

    function_cases = [
        (
            "C5-generic-id",
            "Define function id of T taking x as T returning T:\n    Return x.\n"
            "Return id(42).\n",
            42,
        ),
        (
            "C5-generic-id-f64",
            "Define function id of T taking x as T returning T:\n    Return x.\n"
            "Return id(3.5) as i64.\n",
            3,
        ),
        (
            "C5-generic-wrap",
            "Define struct Pair of T and U with first as T and second as U.\n"
            "Define function wrap of T taking x as T returning Pair<T, i64>:\n"
            "    r is Pair<T, i64>.\n    Set r.first to x.\n    Set r.second to 5.\n"
            "    Return r.\n"
            "p is wrap(7).\nReturn p.first + p.second.\n",
            12,
        ),
        (
            "C5-generic-nested-calls",
            "Define function inner of T taking x as T returning T:\n    Return x.\n"
            "Define function middle of T taking x as T returning T:\n    Return inner(x).\n"
            "Return middle(9).\n",
            9,
        ),
        (
            "C5-generic-pair-f64",
            "Define struct Pair of T and U with first as T and second as U.\n"
            "Define function make_pair of T taking x as T returning Pair<T, i64>:\n"
            "    r is Pair<T, i64>.\n    Set r.first to x.\n    Set r.second to 2.\n"
            "    Return r.\n"
            "p is make_pair(2.5).\nReturn (p.first as i64) + p.second.\n",
            4,
        ),
        (
            "C5-generic-first",
            "Define struct Pair of T and U with first as T and second as U.\n"
            "Define function first_of of T and U taking p as Pair<T, U> returning T:\n"
            "    Return p.first.\n"
            "p is Pair<i64, f64>.\nSet p.first to 11.\nSet p.second to 2.5.\n"
            "Return first_of(p).\n",
            11,
        ),
        (
            "C5-generic-swap-pair",
            "Define struct Pair of T and U with first as T and second as U.\n"
            "Define function swap_pair of T and U taking p as Pair<T, U> returning Pair<U, T>:\n"
            "    r is Pair<U, T>.\n    Set r.first to p.second.\n    Set r.second to p.first.\n"
            "    Return r.\n"
            "p is Pair<i64, f64>.\nSet p.first to 4.\nSet p.second to 2.5.\n"
            "q is swap_pair(p).\nReturn (q.first as i64) + q.second.\n",
            6,
        ),
        (
            "C5-generic-read-ptr",
            "Define function read_ptr of T taking p as ptr<T> returning T:\n"
            "    Return *p.\n"
            "x is 7.\nReturn read_ptr(address of x).\n",
            7,
        ),
        (
            "C5-generic-read-ptr-f64",
            "Define function read_ptr of T taking p as ptr<T> returning T:\n"
            "    Return *p.\n"
            "x is 2.5.\nReturn read_ptr(address of x) as i64.\n",
            2,
        ),
        (
            "C5-generic-option",
            "Define struct Option of T with has_val as bool and val as T.\n"
            "Define function some of T taking x as T returning Option<T>:\n"
            "    r is Option<T>.\n    Set r.has_val to true.\n    Set r.val to x.\n"
            "    Return r.\n"
            "o is some(7).\nIf o.has_val:\n    Return o.val.\nReturn 0.\n",
            7,
        ),
    ]
    for ident, body, expected in function_cases:
        add_program("C5", ident, main_program(body), expected)

    comparer = [
        (
            "C5-comparer-i64",
            "Define struct Comparer of T with cmp as fn(ptr<T>, ptr<T>) -> i64.\n"
            "Define function cmp_i64 taking a as ptr<i64> and b as ptr<i64> returning i64:\n"
            "    Return (*a) - (*b).\n"
            "c is Comparer<i64>.\nSet c.cmp to function cmp_i64.\n"
            "x is 9.\ny is 4.\nReturn c.cmp(address of x, address of y).\n",
            5,
        ),
        (
            "C5-comparer-f64",
            "Define struct Comparer of T with cmp as fn(ptr<T>, ptr<T>) -> i64.\n"
            "Define function cmp_f64 taking a as ptr<f64> and b as ptr<f64> returning i64:\n"
            "    Return ((*a) - (*b)) as i64.\n"
            "c is Comparer<f64>.\nSet c.cmp to function cmp_f64.\n"
            "x is 2.5.\ny is 1.0.\nReturn c.cmp(address of x, address of y).\n",
            1,
        ),
        (
            "C5-comparer-global",
            "Define struct Comparer of T with cmp as fn(ptr<T>, ptr<T>) -> i64.\n"
            "Define function cmp_i64 taking a as ptr<i64> and b as ptr<i64> returning i64:\n"
            "    Return (*a) - (*b).\n"
            "Define global c as Comparer<i64>.\nSet c.cmp to function cmp_i64.\n"
            "x is 3.\ny is 10.\nReturn c.cmp(address of x, address of y) + 10.\n",
            3,
        ),
    ]
    for ident, body, expected in comparer:
        add_program("C5", ident, main_program(body), expected)

    def dag(base: int, a1: int, a2: int, b1: int, b2: int) -> tuple[list[tuple[str, str]], int]:
        d = (
            "Public global base as i64.\n"
            "Public function plus_base taking x as i64 returning i64:\n"
            "    Return x + base.\n"
            "Public struct Pair of T and U with first as T and second as U.\n"
        )
        c = (
            "Import d.\n"
            "Public function c_plus taking x as i64 returning i64:\n"
            "    Return d.plus_base(x) + 1.\n"
        )
        a = (
            "Import c.\nImport d.\n"
            "Public function a_sum taking x as i64 returning i64:\n"
            "    r is d.Pair<i64, i64>.\n"
            f"    Set r.first to {a1}.\n    Set r.second to {a2}.\n"
            "    Return d.plus_base(x) + r.first + r.second.\n"
        )
        b = (
            "Import c.\nImport d.\n"
            "Public function b_sum taking x as i64 returning i64:\n"
            "    r is d.Pair<i64, i64>.\n"
            f"    Set r.first to {b1}.\n    Set r.second to {b2}.\n"
            "    Return d.plus_base(x) + r.first + r.second.\n"
        )
        main = (
            "Import a.\nImport b.\nImport d.\n"
            f"d.base is {base}.\n"
            "Return a.a_sum(5) + b.b_sum(3).\n"
        )
        expected = (base + 5 + a1 + a2) + (base + 3 + b1 + b2)
        return [("d.tc", d), ("c.tc", c), ("a.tc", a), ("b.tc", b), ("main.tc", main)], expected

    for index, parameters in enumerate(
        [(10, 1, 2, 3, 4), (100, 1, 2, 3, 4), (10, 5, 6, 7, 8), (7, 0, 0, 0, 0), (20, 2, 3, 4, 5)],
        1,
    ):
        files, expected = dag(*parameters)
        add_program("C5", f"C5-dag-{index}", dict(files), low(expected))

    overload_files = {
        "a.tc": "Public function emit taking x as i64 returning i64:\n    Phrase: \"emit <x>\"\n    Return x.\n",
        "b.tc": "Public function emit taking x as f64 returning i64:\n    Phrase: \"emit <x>\"\n    Return x as i64.\n",
        "c.tc": "Public function emit taking x as ptr<u8> returning i64:\n    Phrase: \"emit <x>\"\n    Return 1.\n",
    }
    for index, tail in enumerate(
        [
            "Emit 42.\nEmit 3.5.\nEmit bytes_ptr(\"x\").\nReturn 0.\n",
            "Emit 7.\nReturn 1.\n",
            "Emit 2.5.\nReturn 2.\n",
            "Emit bytes_ptr(\"hi\").\nReturn 3.\n",
            "Emit 1.\nEmit 2.\nEmit 3.5.\nReturn 4.\n",
        ],
        1,
    ):
        files = dict(overload_files)
        files["main.tc"] = "Import a.\nImport b.\nImport c.\n" + tail
        expected = int(tail.rsplit("Return ", 1)[1].split(".")[0])
        add_program("C5", f"C5-phrase-overload-{index}", files, expected)

    negative = [
        (
            "C5-neg-cycle-2",
            {
                "main.tc": "Import a.\nReturn 0.\n",
                "a.tc": "Import main.\nPublic function f returning i64:\n    Return 0.\n",
            },
            "circular import",
        ),
        (
            "C5-neg-cycle-3",
            {
                "main.tc": "Import a.\nReturn 0.\n",
                "a.tc": "Import b.\nPublic function f returning i64:\n    Return 0.\n",
                "b.tc": "Import c.\nPublic function g returning i64:\n    Return 0.\n",
                "c.tc": "Import a.\nPublic function h returning i64:\n    Return 0.\n",
            },
            "circular import",
        ),
        (
            "C5-neg-cycle-4",
            {
                "main.tc": "Import a.\nReturn 0.\n",
                "a.tc": "Import b.\nPublic function f returning i64:\n    Return 0.\n",
                "b.tc": "Import c.\nPublic function g returning i64:\n    Return 0.\n",
                "c.tc": "Import d.\nPublic function h returning i64:\n    Return 0.\n",
                "d.tc": "Import a.\nPublic function i returning i64:\n    Return 0.\n",
            },
            "circular import",
        ),
        (
            "C5-neg-private-struct",
            {
                "main.tc": "Import a.\ns is a.Secret.\nReturn 0.\n",
                "a.tc": "Define struct Secret with x as i64.\n",
            },
            "private",
        ),
        (
            "C5-neg-private-function",
            {
                "main.tc": "Import a.\nReturn a.secret().\n",
                "a.tc": "Define function secret returning i64:\n    Return 1.\n",
            },
            "private",
        ),
        (
            "C5-neg-private-global",
            {
                "main.tc": "Import a.\nReturn a.g.\n",
                "a.tc": "Define global g as i64.\n",
            },
            "private",
        ),
        (
            "C5-neg-ambiguous-call",
            {
                "main.tc": "Import a.\nImport b.\nReturn add(1, 2).\n",
                "a.tc": "Public function add taking x as i64 and y as i64 returning i64:\n    Return x + y.\n",
                "b.tc": "Public function add taking x as i64 and y as i64 returning i64:\n    Return x - y.\n",
            },
            "matches 2 overloads",
        ),
        (
            "C5-neg-invalid-phrase-param",
            {
                "main.tc": "Define function f taking x as i64 returning i64:\n    Phrase: \"emit <missing>\"\n    Return x.\nReturn 0.\n",
            },
            "phrase",
        ),
        (
            "C5-neg-duplicate-phrase-param",
            {
                "main.tc": "Define function f taking x as i64 returning i64:\n    Phrase: \"emit <x> <x>\"\n    Return x.\nReturn 0.\n",
            },
            "phrase",
        ),
        (
            "C5-neg-generic-arity",
            {
                "main.tc": "Define struct Pair of T and U with first as T and second as U.\n"
                "p is Pair<i64>.\nReturn 0.\n",
            },
            "type arguments",
        ),
        (
            "C5-neg-recursive-generic",
            {
                "main.tc": "Define struct Box of T with value as T.\n"
                "Define function boom of T taking x as T returning T:\n"
                "    y is boom(address of x).\n    Return x.\n"
                "Return boom(1).\n",
            },
            "",
        ),
        (
            "C5-neg-recursive-struct",
            {
                "main.tc": "Define struct Bad with next as Bad.\nb is Bad.\nReturn 0.\n",
            },
            "",
        ),
        (
            "C5-neg-return-mismatch",
            {
                "main.tc": "Define function f returning i64:\n    Return true.\nReturn f().\n",
            },
            "expected I64",
        ),
        (
            "C5-neg-assign-mismatch",
            {
                "main.tc": "Define local x as i64.\nSet x to true.\nReturn 0.\n",
            },
            "expected I64",
        ),
        (
            "C5-neg-call-mismatch",
            {
                "main.tc": "Define function f taking x as i64 returning i64:\n    Return x.\n"
                "Return f(true).\n",
            },
            "no overload",
        ),
        (
            "C5-neg-undefined-variable",
            {
                "main.tc": "Return missing.\n",
            },
            "not in scope",
        ),
        (
            "C5-neg-undefined-function",
            {
                "main.tc": "Return missing().\n",
            },
            "not found",
        ),
        (
            "C5-neg-undefined-type",
            {
                "main.tc": "Define local x as Nope.\nReturn 0.\n",
            },
            "not found",
        ),
        (
            "C5-neg-break-outside",
            {
                "main.tc": "Break.\nReturn 0.\n",
            },
            "break outside",
        ),
        (
            "C5-neg-continue-outside",
            {
                "main.tc": "Continue.\nReturn 0.\n",
            },
            "continue outside",
        ),
        (
            "C5-neg-invalid-main",
            {
                "main.tc": "Define function main taking x as i64 returning i64:\n    Return x.\n",
            },
            "entry must be",
        ),
        (
            "C5-neg-missing-module",
            {
                "main.tc": "Import nonexistent.\nReturn 0.\n",
            },
            "not found",
        ),
        (
            "C5-neg-ambiguous-phrase",
            {
                "main.tc": "Import a.\nImport b.\nEmit 42.\nReturn 0.\n",
                "a.tc": "Public function emit taking x as i64 returning i64:\n    Phrase: \"emit <x>\"\n    Return x.\n",
                "b.tc": "Public function emit taking x as i64 returning i64:\n    Phrase: \"emit <x>\"\n    Return x.\n",
            },
            "ambiguous Phrase",
        ),
    ]
    for ident, files, needle in negative:
        add_program("C5", ident, files, 0, kind="error", error_contains=needle)

# ---------------------------------------------------------------------------
# Tier C6 — multi-verb WordNet/VerbNet linguistic programs
# ---------------------------------------------------------------------------

class Vars:
    def __init__(self, a: int, b: int, c: int):
        self.a = a
        self.b = b
        self.c = c

    def apply(self, op: tuple) -> str:
        kind = op[0]
        amount = op[1] if len(op) > 1 else None
        if kind == "add_a":
            self.a += amount
            return f"Add {amount} to a."
        if kind == "remove_b":
            self.b -= amount
            return f"Remove {amount} from b."
        if kind == "mul_c":
            self.c *= amount
            return f"Multiply c by {amount}."
        if kind == "double_a":
            self.a *= 2
            return "Double a."
        if kind == "halve_c":
            self.c = int(self.c / 2)
            return "Halve c."
        if kind == "inc_b":
            self.b += amount
            return f"Increase b by {amount}."
        if kind == "dec_a":
            self.a -= amount
            return f"Decrease a by {amount}."
        if kind == "neg_a":
            self.a = -self.a
            return "Negate a."
        if kind == "transfer_ab":
            self.a -= amount
            self.b += amount
            return f"Transfer {amount} from a to b."
        if kind == "move_bc":
            self.b -= amount
            self.c += amount
            return f"Move {amount} from b to c."
        if kind == "append_a":
            self.a += amount
            return f"Append {amount} to a."
        if kind == "div_c":
            self.c = int(self.c / amount)
            return f"Divide c by {amount}."
        if kind == "deduct_b":
            self.b -= amount
            return f"Deduct {amount} from b."
        if kind == "withdraw_b":
            self.b -= amount
            return f"Withdraw {amount} from b."
        if kind == "spend_b":
            self.b -= amount
            return f"Spend {amount} from b."
        if kind == "take_b":
            self.b -= amount
            return f"Take {amount} from b."
        if kind == "subtract_b":
            self.b -= amount
            return f"Subtract {amount} from b."
        if kind == "assign_a":
            self.a = amount
            return f"Assign a to {amount}."
        if kind == "put_a":
            self.a = amount
            return f"Put {amount} in a."
        if kind == "set_a":
            self.a = amount
            return f"Set a to {amount}."
        if kind == "keep_a":
            self.a = amount
            return f"Keep {amount} in a."
        if kind == "store_a":
            self.a = amount
            return f"Store {amount} in a."
        if kind == "inc_a":
            self.a += amount
            return f"Increase a by {amount}."
        if kind == "dec_b":
            self.b -= amount
            return f"Decrease b by {amount}."
        if kind == "move_ba":
            self.b -= amount
            self.a += amount
            return f"Move {amount} from b to a."
        if kind == "transfer_ba":
            self.b -= amount
            self.a += amount
            return f"Transfer {amount} from b to a."
        if kind == "halve_a":
            self.a = int(self.a / 2)
            return "Halve a."
        if kind == "double_b":
            self.b *= 2
            return "Double b."
        if kind == "neg_b":
            self.b = -self.b
            return "Negate b."
        if kind == "mul_b":
            self.b *= amount
            return f"Multiply b by {amount}."
        if kind == "div_b":
            self.b = int(self.b / amount)
            return f"Divide b by {amount}."
        if kind == "add_c":
            self.c += amount
            return f"Add {amount} to c."
        if kind == "remove_c":
            self.c -= amount
            return f"Remove {amount} from c."
        raise ValueError(op)


def chain_program(start: tuple[int, int, int], ops: list[tuple]) -> tuple[str, int]:
    values = Vars(*start)
    lines = [f"a is {start[0]}.", f"b is {start[1]}.", f"c is {start[2]}."]
    for op in ops:
        lines.append(values.apply(op))
    lines.append("Return (a + b + c) bitand 255.")
    return "\n".join(lines) + "\n", (values.a + values.b + values.c) & 0xFF


def tier_c6() -> None:
    chains = [
        ((100, 200, 10), [("add_a", 5), ("remove_b", 30), ("mul_c", 3), ("halve_c", 0), ("transfer_ab", 10)]),
        ((50, 60, 8), [("double_a",), ("deduct_b", 10), ("div_c", 2), ("move_bc", 5), ("append_a", 2)]),
        ((80, 90, 12), [("inc_b", 7), ("dec_a", 3), ("neg_a",), ("withdraw_b", 20), ("mul_c", 2)]),
        ((70, 40, 9), [("transfer_ba", 15), ("spend_b", 5), ("take_b", 3), ("subtract_b", 2), ("halve_a",)]),
        ((30, 100, 6), [("assign_a", 42), ("put_a", 11), ("set_a", 13), ("keep_a", 17), ("store_a", 19)]),
        ((90, 80, 15), [("inc_a", 10), ("dec_b", 20), ("double_b",), ("div_b", 4), ("add_c", 5)]),
        ((60, 50, 20), [("remove_c", 3), ("append_a", 4), ("move_ba", 6), ("neg_b",), ("mul_b", 3)]),
        ((25, 75, 5), [("add_a", 25), ("remove_b", 25), ("mul_c", 4), ("halve_c",), ("transfer_ab", 10)]),
        ((120, 140, 30), [("deduct_b", 40), ("withdraw_b", 20), ("subtract_b", 10), ("spend_b", 5), ("take_b", 5)]),
        ((10, 20, 40), [("double_a",), ("double_b",), ("halve_c",), ("div_c", 2), ("add_c", 7)]),
    ]
    for index, (start, ops) in enumerate(chains, 1):
        body, expected = chain_program(start, ops)
        add_program("C6", f"C6-chain-{index:02d}", main_program(body), expected)

    pairs = [
        ("add", "Add 5 to x.", "5 is added to x.", 10, 15),
        ("remove", "Remove 30 from x.", "30 is removed from x.", 100, 70),
        ("multiply", "Multiply x by 3.", "x is multiplied by 3.", 10, 30),
        ("divide", "Divide x by 2.", "x is divided by 2.", 10, 5),
        ("deduct", "Deduct 20 from x.", "20 is deducted from x.", 100, 80),
        ("subtract", "Subtract 15 from x.", "15 is subtracted from x.", 100, 85),
        ("increase", "Increase x by 7.", "x is increased by 7.", 10, 17),
        ("decrease", "Decrease x by 4.", "x is decreased by 4.", 10, 6),
    ]
    for name, active, passive, start, expected in pairs:
        add_program(
            "C6",
            f"C6-pair-{name}-active",
            main_program(f"x is {start}.\n{active}\nReturn x.\n"),
            low(expected),
        )
        add_program(
            "C6",
            f"C6-pair-{name}-passive",
            main_program(f"x is {start}.\n{passive}\nReturn x.\n"),
            low(expected),
        )
    add_program(
        "C6",
        "C6-pair-transfer-active",
        main_program(
            "a is 100.\nb is 20.\nTransfer 30 from a to b.\nReturn a + b.\n"
        ),
        low(120),
    )
    add_program(
        "C6",
        "C6-pair-transfer-passive",
        main_program(
            "a is 100.\nb is 20.\n30 is transferred from a to b.\nReturn a + b.\n"
        ),
        low(120),
    )

    where_cases = [
        ("Return sum of a and b where a is 2 and b is 5.\n", 7),
        ("Return a * b where a is 3 and b is 4.\n", 12),
        ("total is 0.\nIf true:\n    Add y to total where y is 9.\nReturn total.\n", 9),
        (
            "total is 0.\ni is 0.\nLoop:\n    i += 1.\n    If i is at least 3, Break.\n"
            "    Add y to total where y is i * 10.\nReturn total.\n",
            30,
        ),
        (
            "Define function f taking x as i64 returning i64:\n"
            "    Return x + y where y is 5.\nReturn f(7).\n",
            12,
        ),
        (
            "x is 0.\nIf false:\n    Add 10 to x where x is 5.\nReturn x + 1.\n",
            1,
        ),
    ]
    for index, (body, expected) in enumerate(where_cases, 1):
        add_program("C6", f"C6-where-{index}", main_program(body), expected)

    control_cases = [
        (
            "C6-control-repeat",
            "x is 0.\nRepeat 3 times:\n    Add 5 to x.\nReturn x.\n",
            15,
        ),
        (
            "C6-control-if",
            "x is 10.\nIf x more than 5:\n    Remove 3 from x.\nElse:\n    Add 3 to x.\nReturn x.\n",
            7,
        ),
        (
            "C6-control-while",
            "x is 0.\nWhile x less than 4:\n    Add 2 to x.\nReturn x.\n",
            4,
        ),
        (
            "C6-control-loop",
            "x is 20.\nLoop:\n    Remove 3 from x.\n    If x is at most 10, Break.\nReturn x.\n",
            8,
        ),
        (
            "C6-control-function",
            "Define function f taking x as i64 returning i64:\n    Multiply x by 3.\n    Return x.\nReturn f(7).\n",
            21,
        ),
        (
            "C6-control-nested",
            "i is 0.\ntotal is 0.\nRepeat 3 times:\n    If i is less than 2:\n        Add 10 to total.\n    Else:\n        Add 20 to total.\n    i += 1.\nReturn total.\n",
            40,
        ),
    ]
    for ident, body, expected in control_cases:
        add_program("C6", ident, main_program(body), expected)

    all_ops = [
        ("add_a", 5), ("remove_b", 7), ("mul_c", 2), ("double_a",), ("halve_c",),
        ("inc_b", 3), ("dec_a", 2), ("transfer_ab", 4), ("move_bc", 6), ("append_a", 8),
        ("div_c", 3), ("deduct_b", 5), ("withdraw_b", 2), ("spend_b", 1), ("take_b", 1),
        ("subtract_b", 1), ("assign_a", 20), ("put_a", 21), ("set_a", 22), ("keep_a", 23),
    ]
    for index in range(1, 5):
        rotated = all_ops[index:] + all_ops[:index]
        body, expected = chain_program((100, 200, 30 + index), rotated)
        add_program("C6", f"C6-all-verbs-{index}", main_program(body), expected)

# ---------------------------------------------------------------------------
# Tier C7 — casual, natural, non-native English and prose safety
# ---------------------------------------------------------------------------

def tier_c7() -> None:
    executable: list[tuple[str, str, int]] = []

    for index in range(10):
        cash = 100 + index * 10
        removed = 10 + index
        added = 5 + index
        expected = (cash - removed + added) & 0xFF
        if index % 4 == 0:
            body = f"cash is {cash}. Remove {removed} from cash. Add {added} to cash. Return cash.\n"
        elif index % 4 == 1:
            body = (
                f"cash is {cash}.\nRemove {removed} from cash.\nAdd {added} to cash.\n"
                "Return cash.\n"
            )
        elif index % 4 == 2:
            body = (
                f"1. cash is {cash}.\n2. Remove {removed} from cash.\n"
                f"3. Add {added} to cash.\n4. Return cash.\n"
            )
        else:
            body = (
                f"cash is {cash}; Remove {removed} from cash; "
                f"Add {added} to cash; Return cash.\n"
            )
        executable.append((f"C7-exec-assign-{index + 1:02d}", body, expected))

    for index in range(10):
        salary = 1000 + index * 50
        removed = 100 + index * 5
        added = 50 + index * 3
        expected = (salary - removed + added) & 0xFF
        if index % 2 == 0:
            body = (
                f"salary is {salary}. {removed} is removed from salary. "
                f"salary is increased by {added}. Return salary.\n"
            )
        else:
            body = (
                f"First, salary is {salary}.\nThen, {removed} is removed from salary.\n"
                f"After that, salary is increased by {added}.\nFinally, Return salary.\n"
            )
        executable.append((f"C7-exec-passive-{index + 1:02d}", body, expected))

    for index in range(10):
        value = 10 + index
        threshold = 5 + index % 3
        delta = 2 + index % 4
        if index % 5 == 0:
            body = (
                f"x is {value}.\nIf x more than {threshold}:\n    Add {delta} to x.\n"
                "Else:\n    Remove 1 from x.\nReturn x.\n"
            )
            expected = value + delta if value > threshold else value - 1
        elif index % 5 == 1:
            body = (
                f"x is {value}.\nWhile x less than {value + 6}:\n    Add 2 to x.\n"
                "Return x.\n"
            )
            expected = value + 6
            if (expected - value) % 2 != 0:
                expected += 2
        elif index % 5 == 2:
            body = (
                f"x is {value}.\nRepeat 3 times:\n    Add {delta} to x.\nReturn x.\n"
            )
            expected = value + 3 * delta
        elif index % 5 == 3:
            body = (
                f"x is {value}.\nLoop:\n    Remove 2 from x.\n"
                f"    If x is at most {value - 4}, Break.\nReturn x.\n"
            )
            expected = value - 4
        else:
            body = (
                f"i is 0.\ntotal is 0.\nRepeat 3 times:\n"
                f"    If i is less than 2:\n        Add {delta} to total.\n"
                f"    Else:\n        Add {delta + 5} to total.\n    i += 1.\n"
                "Return total.\n"
            )
            expected = 2 * delta + delta + 5
        executable.append((f"C7-exec-control-{index + 1:02d}", body, expected & 0xFF))

    for index in range(10):
        a = 2 + index
        b = 3 + index * 2
        if index % 5 == 0:
            body = f"Return sum of a and b where a is {a} and b is {b}.\n"
            expected = a + b
        elif index % 5 == 1:
            body = f"Return a * b where a is {a} and b is {b}.\n"
            expected = a * b
        elif index % 5 == 2:
            body = f"Return {10 + index} percent of {100 + index * 10}.\n"
            expected = ((10 + index) * (100 + index * 10)) // 100
        elif index % 5 == 3:
            body = (
                f"x is {a + b}.\nIf x is between {a} and {a + b + 5}, Add 2 to x.\n"
                "Return x.\n"
            )
            expected = a + b + 2
        else:
            body = (
                f"a is {a}.\nb is {b}.\n"
                f"Transfer {a} from a to b.\nMove {b // 2} from b to a.\n"
                "Return a + b.\n"
            )
            expected = a + b
        executable.append((f"C7-exec-expr-{index + 1:02d}", body, expected & 0xFF))

    for index in range(10):
        bank = 200 + index * 10
        wallet = 20 + index
        amount = 30 + index
        body = (
            f"First, bank is {bank}.\nAfter that, wallet is {wallet}.\n"
            f"Then, transfer {amount} from bank to wallet.\n"
            "Finally, return bank + wallet.\n"
        )
        executable.append((f"C7-exec-transfer-{index + 1:02d}", body, (bank + wallet) & 0xFF))

    for ident, body, expected in executable:
        add_program("C7", ident, main_program(body), expected)

    prose_sentences = [
        "Today shop was very busy because of rain.",
        "My brother came in evening and drank 2 teas.",
        "Yesterday Ravi called me twice and said petrol is 105 rupees.",
        "I waited 20 minutes for bus.",
        "I had 2 cups of coffee.",
        "I am 25.",
        "He is 30 years old.",
        "She bought 3 kilos of rice for 180 rupees.",
        "I went to the store and made a grocery list.",
        "Let us check how much we spent today.",
        "Today the weather is nice.",
        "My uncle said petrol is very costly.",
        "I have 3 books.",
        "We bought 2 tickets for 300 rupees.",
        "The bus was late.",
        "It was raining yesterday.",
        "My mother cooked rice.",
        "I drank 2 teas.",
        "She waited 20 minutes.",
        "The shop was very busy because of rain.",
        "Let's check the total.",
        "We spent 200 rupees on food.",
        "He bought a new phone.",
        "She was happy.",
    ]
    bases = [
        ("x is 10.\nAdd 5 to x.\nReturn x.\n", 15),
        ("cash is 100.\nRemove 30 from cash.\nReturn cash.\n", 70),
        ("x is 7.\nIf true:\n    Add 3 to x.\nReturn x.\n", 10),
        ("i is 0.\nRepeat 3 times:\n    Add 2 to i.\nReturn i.\n", 6),
        ("Return sum of a and b where a is 2 and b is 5.\n", 7),
    ]
    for index in range(50):
        base, expected = bases[index % len(bases)]
        selected = [
            prose_sentences[(index + offset) % len(prose_sentences)]
            for offset in range(4 + index % 3)
        ]
        comments = [text[:-1] if text.endswith(".") else text for text in selected]
        prose = " ".join(selected)
        if index % 2 == 0:
            body = prose + "\n" + base
        else:
            body = base + prose + "\n"
        add_program(
            "C7",
            f"C7-prose-{index + 1:02d}",
            main_program(body),
            expected,
            comments=comments,
        )

    unsupported = [
        "Start a new thread.",
        "Draw a window.",
        "Match with a regular expression.",
        "Connect to a TCP socket.",
        "Download a file from the internet.",
        "Send an email.",
        "Open a database.",
        "Compile another program.",
        "Render a 3D scene.",
        "Play music.",
        "Translate this document.",
        "Recognize speech.",
        "Train a model.",
        "Scan the network.",
        "Run a web server.",
    ]
    for index, directive in enumerate(unsupported, 1):
        add_program(
            "C7",
            f"C7-unsupported-{index:02d}",
            main_program(directive + "\nReturn 0.\n"),
            0,
            kind="error",
            error_contains="Unsupported",
        )

if __name__ == "__main__":
    raise SystemExit(main())
