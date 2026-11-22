#!/usr/bin/env python3
"""Deterministic per-round convergence fuzzer (four disjoint engines)."""
import argparse, concurrent.futures, datetime, json, pathlib, random, socket, string, subprocess, sys, tempfile

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parent
REPO = pathlib.Path("/home/ponvasanth/tc_r1")
ROUND = 1
TC = REPO / "target/debug/tc"
sys.path.insert(0, str(HERE))
from run_conv_probe import probe

PROSE = [
    "It was raining outside.",
    "It is 3 pm.",
    "Yesterday Ravi called me twice.",
    "My brother drank 2 teas.",
    "I bought 6 coffees in 3 days.",
    "The shop was very busy because of rain.",
    "My sister bought fresh milk yesterday.",
    "The report was late.",
    "We walked home in the evening.",
    "The weather was pleasant.",
    "Ravi waited near the station.",
    "The kettle boiled for five minutes.",
]

def source1(seed):
    rng = random.Random(seed * 7919 + 11)
    kind = seed % 8
    if kind == 0:
        x = round(rng.uniform(0.01, 200.0), 4)
        n = rng.randint(-100, 100)
        lo, hi = sorted((rng.randint(-50, 0), rng.randint(1, 100)))
        return f"Use io and math.\n\nLet x be {x}.\nSay x.\nLet root be square root of {abs(x)}.\nSay root.\nSay abs_i64(0 minus {n}).\nSay clamp_i64({n}, {lo}, {hi}).\nGive back 0.\n"
    if kind == 1:
        n, k = rng.randint(1, 40), rng.randint(1, 9)
        modules = ["alloc", "core", "io", "list", "string", "map"][: min(3 + ROUND, 6)]
        return f"Use {', '.join(modules)}.\n\nLet nums be List of whole number.\nLet i be 0.\nWhile i is less than {n}.\nPush i times {k} to address of nums.\nAdd 1 to i.\nEnd the loop.\nLet count be count of address of nums.\nSay count.\nLet popped be pop from address of nums.\nSay popped.\nFree list address of nums.\nGive back 0.\n"
    if kind == 2:
        n, k = rng.randint(1, 80), rng.randint(0, 79)
        return f"Use alloc, core, io, map.\n\nLet table be IntMap of whole number.\nLet i be 0.\nWhile i is less than {n}.\nPut i times 2 into int map address of table for key i.\nAdd 1 to i.\nEnd the loop.\nLet found be look up {k} in int map address of table.\nIf found.has_value, say found.value, otherwise say -1.\nLet count be count of int map address of table.\nSay count.\nFree int map address of table.\nGive back 0.\n"
    if kind == 3:
        text = "".join(rng.choice(string.ascii_letters) for _ in range(rng.randint(1, 20)))
        other = "".join(rng.choice(string.ascii_letters) for _ in range(rng.randint(1, 20)))
        return f"Use alloc, core, io, string.\n\nLet a be create text from \"{text}\" of length {len(text)}.\nLet b be create text from \"{other}\" of length {len(other)}.\nLet c be join address of a and address of b.\nSay address of c.\nLet n be length of address of c.\nSay n.\nRelease text address of a.\nRelease text address of b.\nRelease text address of c.\nGive back 0.\n"
    if kind == 4:
        name = f"f{seed}.txt"
        return f"Use alloc, core, fs, io, string.\n\nLet name be create text from \"{name}\" of length {len(name)}.\nSave {rng.randint(-1000, 1000)} to file address of name.\nDisplay file address of name.\nDelete file address of name.\nRelease text address of name.\nGive back 0.\n"
    if kind == 5:
        size = rng.randint(1, 500); size2 = rng.randint(max(16, size), 9000)
        return f"Use alloc, core, io.\n\nA record Chunk has a size (an unsigned number), a free flag (an unsigned number), and a next link (a pointer to Chunk).\nLet chunk be Chunk.\nUpdate chunk so its size is {size}, its free flag is 1, and its next link is no address.\nSay chunk's size as a whole number.\nLet p be allocate {size} bytes.\nSet p[0] to {rng.randint(0, 250)}.\nLet q be resize memory at p from {size} to {size2}.\nRelease memory at q.\nLet used be bytes in use.\nIf used is not 0, stop with error \"leak\".\nGive back 0.\n"
    if kind == 6:
        n = rng.randint(1, 30)
        return f"Use io.\n\nLet total be 0.\nLet n be 1.\nWhile n is at most {n}.\nAdd n to total.\nAdd 1 to n.\nEnd the loop.\nSay total.\nGive back 0.\n"
    n = rng.randint(1, 20)
    if seed % 2 == 0:
        return f"Use io.\n\nLet i be 0.\nUntil i is at least {n}, add 1 to i.\nGive back i if i is less than 10, otherwise give back 10.\n"
    depth = min(3 + ROUND, 7)
    expr = str(rng.randint(1, 30))
    for _ in range(depth):
        expr = f"({expr} plus {rng.randint(1, 9)})"
    return f"Use io.\nLet total be {expr}.\nSay total.\nGive back 0.\n"

def pair2(seed):
    rng = random.Random(seed * 104729 + 3)
    kind = seed % 6
    if kind == 0:
        a, b = rng.randint(1, 99), rng.randint(1, 99)
        canonical = f"Use io.\n\nTo pick given a as a whole number, b as a whole number, named pick, returning a whole number.\nIf a < b:\n    Give back a.\nGive back b.\n\nSay pick({a}, {b}).\nReturn 0.\n"
        version2 = f"Use io.\n\nTo pick a and b (whole numbers), named pick, giving a whole number.\nGive back a if a is less than b, otherwise give back b.\n\nSay pick({a}, {b}).\nGive back 0.\n"
    elif kind == 1:
        w, h = rng.randint(-50, 50), rng.randint(-50, 50)
        canonical = f"Use io.\n\nA record Box has a width as a whole number and a height as a whole number.\nLet box be Box.\nSet box.width to {w}.\nSet box.height to {h}.\nSay box.width + box.height.\nReturn 0.\n"
        version2 = f"Use io.\n\nA record Box has a width (a whole number) and a height (a whole number).\nLet box be Box.\nUpdate box so its width is {w} and its height is {h}.\nSay box's width plus box's height.\nGive back 0.\n"
    elif kind == 2:
        n = rng.randint(1, 30)
        canonical = f"Use io.\n\nLet i be 0.\nWhile i < {n}:\n    Add 1 to i.\nSay i.\nReturn 0.\n"
        version2 = f"Use io.\n\nLet i be 0.\nUntil i is at least {n}, add 1 to i.\nSay i.\nGive back 0.\n"
    elif kind == 3:
        v = rng.randint(-1000, 1000)
        canonical = f"Use io.\n\nDefine global bytes_in_use as a whole number.\nSet bytes_in_use to {v}.\nSay bytes_in_use.\nReturn 0.\n"
        version2 = f"Use io.\n\nKeep a shared bytes in use as a whole number.\nSet bytes in use to {v}.\nSay bytes in use.\nGive back 0.\n"
    elif kind == 4:
        v = rng.randint(-500, 500)
        canonical = f"Use io and list.\n\nDefine local nums as List<i64>.\npush {v} to address of nums.\nSay list_get(address of nums, 0).\nlist_free(address of nums).\nReturn 0.\n"
        version2 = f"Use io and list.\n\nLet nums be List of whole number.\nPush {v} to address of nums.\nSay item 0 of nums's data.\nFree list address of nums.\nGive back 0.\n"
    else:
        text = "".join(rng.choice(string.ascii_letters) for _ in range(rng.randint(1, 12)))
        canonical = f"Use io and string.\n\ns is str_from_bytes(bytes_ptr(\"{text}\"), bytes_len(\"{text}\")).\nSay address of s.\nstr_free(address of s).\nReturn 0.\n"
        version2 = f"Use io and string.\n\nLet s be create text from \"{text}\" of length {len(text)}.\nSay address of s.\nRelease text address of s.\nGive back 0.\n"
    return canonical, version2

def base3(seed):
    rng = random.Random(seed * 15485863 + 5)
    a = rng.randint(-30, 30); b = rng.randint(-30, 30)
    return f"Use io.\n\nLet total be {a}.\nAdd {b} to total.\nLet doubled be total times 2.\nSay doubled.\nGive back 0.\n"

def injected3(seed):
    rng = random.Random(seed * 32452843 + 7)
    base = base3(seed)
    count = rng.randint(3, 8)
    lines = [rng.choice(PROSE) for _ in range(count)]
    header, *body = base.splitlines()
    return header + "\n\n" + "\n".join(lines) + "\n\n" + "\n".join(body) + "\n", lines

def malformed4(seed):
    rng = random.Random(seed * 49979687 + 13)
    base = source1(seed)
    kind = seed % 8
    if kind == 0:
        return base[: rng.randint(1, max(1, len(base) - 1))]
    if kind == 1:
        lines = base.splitlines()
        if len(lines) > 2:
            lines.pop(rng.randrange(len(lines)))
        return "\n".join(lines)
    if kind == 2:
        chars = list(base)
        for _ in range(rng.randint(1, 12)):
            if chars:
                chars[rng.randrange(len(chars))] = rng.choice("()\"#")
        return "".join(chars)
    if kind == 3:
        return base + "\nUpdate x so its\n"
    if kind == 4:
        return "Use io.\nLet x be " + "(" * 70 + "1" + ")" * 70 + ".\nSay x.\nGive back 0.\n"
    if kind == 5:
        return "Use io.\nSay " + "9" * 300 + ".\nGive back 0.\n"
    if kind == 6:
        return "Use io.\nLet s be \"" + "word " * 1200 + "\".\nSay 0.\nGive back 0.\n"
    if kind == 7 and seed % 3 == 0:
        return 'Use io.\nLet x be அம்மா.\nSay x.\nGive back 0.\n'
    if kind == 7 and seed % 3 == 1:
        return 'Use io.\nSay "🚀 அம்மா ❤".\nGive back 0.\n'
    if kind == 7 and seed % 3 == 2:
        return "Use io.\nLet x be " + "(" * min(50 + ROUND * 5, 95) + "1" + ")" * min(50 + ROUND * 5, 95) + ".\nSay x.\nGive back 0.\n"
    return "Use missing_module.\nSay 1.\nGive back 0.\n"

def trace_omitted(program, lines):
    result = subprocess.run([str(TC), "trace", str(program)], cwd=str(REPO), stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=30)
    text = result.stdout.decode(errors="replace").splitlines()
    for sentence in lines:
        found = False
        target = sentence.strip().rstrip(".")
        for index, trace_line in enumerate(text):
            if trace_line.strip().rstrip(".") == target:
                previous = text[index - 1] if index else ""
                if "COMMENT_OMITTED" in previous:
                    found = True
                    break
        if not found:
            return False, sentence
    return True, ""

def engine_one(seed):
    source = source1(seed)
    path = pathlib.Path(tempfile.mkdtemp(prefix="p5f1_", dir="/tmp/opencode")) / f"seed_{seed}.eng"
    path.write_text(source)
    result = probe(path, b"", 20)
    result["engine"] = 1; result["seed"] = seed
    return result

def engine_two(seed):
    canonical, version2 = pair2(seed)
    tmp = pathlib.Path(tempfile.mkdtemp(prefix="p5f2_", dir="/tmp/opencode"))
    cpath, vpath = tmp/"canonical.eng", tmp/"version2.eng"
    cpath.write_text(canonical); vpath.write_text(version2)
    c = probe(cpath, b"", 20); v = probe(vpath, b"", 20)
    ok = (c.get("ok") and v.get("ok") and c.get("stdout") == v.get("stdout")
          and c.get("stderr") == v.get("stderr") and c.get("elf_exit") == v.get("elf_exit"))
    return {"engine":2,"seed":seed,"ok":ok,"canonical":c,"version2":v,"program":str(cpath)}

def engine_three(seed):
    base = base3(seed)
    injected, lines = injected3(seed)
    tmp = pathlib.Path(tempfile.mkdtemp(prefix="p5f3_", dir="/tmp/opencode"))
    bpath, ipath = tmp/"base.eng", tmp/"injected.eng"
    bpath.write_text(base); ipath.write_text(injected)
    b = probe(bpath, b"", 20); i = probe(ipath, b"", 20)
    omitted, missing = trace_omitted(ipath, lines) if i.get("ok") else (False, "injected compile failed")
    ok = (b.get("ok") and i.get("ok") and b.get("stdout") == i.get("stdout")
          and b.get("stderr") == i.get("stderr") and b.get("elf_exit") == i.get("elf_exit") and omitted)
    return {"engine":3,"seed":seed,"ok":ok,"base":b,"injected":i,"program":str(ipath),"missing":missing}

def engine_four(seed):
    source = malformed4(seed)
    tmp = pathlib.Path(tempfile.mkdtemp(prefix="p5f4_", dir="/tmp/opencode"))
    path = tmp/f"seed_{seed}.eng"; path.write_text(source)
    result = probe(path, b"", 8, compile_only=True)
    code = result.get("exit_code", 0)
    ok = result.get("stage") != "timeout" and not result.get("panic") and not result.get("error") and code >= 0
    return {"engine":4,"seed":seed,"ok":ok,"program":str(path),"result":result}

ENGINES = {1: ("fuzz_diff", engine_one), 2: ("fuzz_paraphrase", engine_two),
           3: ("fuzz_prose", engine_three), 4: ("fuzz_crash", engine_four)}

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--round", type=int, required=True)
    parser.add_argument("--engine", required=True, choices=["diff", "paraphrase", "prose", "crash"])
    parser.add_argument("--seed-start", type=int, required=True)
    parser.add_argument("--count", type=int, required=True)
    parser.add_argument("--jobs", type=int, default=12)
    parser.add_argument("--batch", type=int, default=50)
    args = parser.parse_args()
    global ROUND
    ROUND = args.round
    engine_numbers = {"diff": 1, "paraphrase": 2, "prose": 3, "crash": 4}
    args.engine = engine_numbers[args.engine]
    expected = {1: (1, 1000, 1000), 2: (1001, 750, 750), 3: (1751, 750, 750), 4: (2501, 500, 500)}[args.engine]
    base = 10_000 + (args.round - 1) * 3_000
    if args.seed_start != base + expected[0] or args.count != expected[1]:
        raise SystemExit(f"round {args.round} engine {args.engine} requires --seed-start {base + expected[0]} --count {expected[1]}")
    metric, worker = ENGINES[args.engine]
    failures = []
    started = datetime.datetime.now()
    seeds = list(range(args.seed_start, args.seed_start + args.count))
    with concurrent.futures.ProcessPoolExecutor(max_workers=args.jobs) as pool:
        for base in range(0, len(seeds), args.batch):
            batch = seeds[base:base+args.batch]
            batch_start = datetime.datetime.now()
            results = list(pool.map(worker, batch))
            batch_fail = [r for r in results if not r.get("ok")]
            failures.extend(batch_fail)
            summary = {"round":args.round,"engine":args.engine,"metric":metric,"start":batch[0],"end":batch[-1],
                       "passed":len(batch)-len(batch_fail),"failed":len(batch_fail),
                       "elapsed_seconds":round((datetime.datetime.now()-batch_start).total_seconds(),2),
                       "timestamp":datetime.datetime.now().astimezone().isoformat(timespec="seconds")}
            with (ROOT/"FUZZ_CONV_LOG.jsonl").open("a") as handle:
                handle.write(json.dumps(summary)+"\n")
                for failure in batch_fail:
                    handle.write(json.dumps({"failure":failure})+"\n")
            state = json.loads((ROOT/"STATE.json").read_text())
            metrics = state["round_metrics"]
            metrics[f"{metric}_done"] = min(metrics.get(f"{metric}_target", 0), metrics.get(f"{metric}_done", 0) + len(batch))
            state["cumulative_metrics"]["total_fuzz_seeds_tested"] = state["cumulative_metrics"].get("total_fuzz_seeds_tested", 0) + len(batch)
            state["current_band"] = f"FUZZ E{args.engine}"
            state["current_activity"] = f"Round {args.round} — fuzz engine {args.engine}: seeds {batch[0]}..{batch[-1]}"
            state["updated_at"] = summary["timestamp"]
            (ROOT/"STATE.json").write_text(json.dumps(state,indent=2)+"\n")
            print(f"engine {args.engine} seeds {batch[0]}..{batch[-1]} passed {len(batch)-len(batch_fail)}/{len(batch)}", flush=True)
    faildir = ROOT/"rounds"/f"round_{args.round:02d}"/"fuzz_failures"; faildir.mkdir(parents=True, exist_ok=True)
    for failure in failures:
        (faildir/f"fuzz_e{failure['engine']}_{failure['seed']}.json").write_text(json.dumps(failure,indent=2))
    elapsed = (datetime.datetime.now()-started).total_seconds()
    with (ROOT/"JOURNAL.md").open("a") as handle:
        handle.write(f"- {datetime.datetime.now().astimezone().isoformat(timespec='seconds')} — Round {args.round} fuzz engine {args.engine} seeds {args.seed_start}..{seeds[-1]}: {len(seeds)-len(failures)}/{len(seeds)} in {elapsed:.1f}s.\n")
    print(json.dumps({"engine":args.engine,"total":len(seeds),"passed":len(seeds)-len(failures),"failed":len(failures),"elapsed":round(elapsed,1)},indent=2))
    return 0 if not failures else 1

if __name__ == "__main__":
    raise SystemExit(main())
