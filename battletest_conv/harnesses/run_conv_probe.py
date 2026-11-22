#!/usr/bin/env python3
"""Core convergence probe: exact ELF64 == reference IR interpreter parity."""
import argparse, json, os, pathlib, re, shutil, signal, subprocess, sys, tempfile, time

REPO = pathlib.Path("/home/ponvasanth/tc_r1")
TC = REPO / "target/debug/tc"

class Timeout(Exception):
    pass

def run(cmd, cwd, stdin=b"", timeout=15):
    proc = subprocess.Popen(
        cmd, cwd=str(cwd), stdin=subprocess.PIPE, stdout=subprocess.PIPE,
        stderr=subprocess.PIPE, start_new_session=True,
    )
    try:
        out, err = proc.communicate(stdin, timeout=timeout)
    except subprocess.TimeoutExpired:
        try:
            os.killpg(proc.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        out, err = proc.communicate()
        raise Timeout(f"timeout after {timeout}s: {' '.join(map(str, cmd))}")
    return proc.returncode, out, err

def tree(root):
    files = {}
    for path in sorted(pathlib.Path(root).rglob("*")):
        if path.is_file():
            files[str(path.relative_to(root))] = path.read_bytes()
    return files

def static_check(elf):
    ldd = run(["ldd", str(elf)], elf.parent, timeout=5)
    dyn = run(["readelf", "-d", str(elf)], elf.parent, timeout=5)
    seg = run(["readelf", "-l", str(elf)], elf.parent, timeout=5)
    ldd_text = (ldd[1] + ldd[2]).decode(errors="replace")
    dyn_text = (dyn[1] + dyn[2]).decode(errors="replace")
    seg_text = seg[1].decode(errors="replace")
    return {
        "ldd_static": "not a dynamic executable" in ldd_text,
        "no_dynamic_section": "There is no dynamic section" in dyn_text,
        "no_interp": not any("INTERP" in line for line in seg_text.splitlines()),
    }

def trace_sentences(program):
    code, out, err = run([str(TC), "trace", str(program)], pathlib.Path(program).resolve().parent, timeout=30)
    return code, out.decode(errors="replace"), err.decode(errors="replace")

def probe(program, stdin=b"", timeout=20, compile_only=False, check_static=False):
    program = pathlib.Path(program).resolve()
    source_dir = program.parent
    tmp = pathlib.Path(tempfile.mkdtemp(prefix="convprobe_", dir="/tmp/opencode"))
    elf_dir, interp_dir = tmp / "elf", tmp / "interp"
    elf_dir.mkdir(); interp_dir.mkdir()
    elf = tmp / "program.elf"
    started = time.time()
    try:
        code, out, err = run([str(TC), "build", str(program), "-o", str(elf)], source_dir, timeout=timeout)
        if code != 0:
            return {"program": str(program), "ok": False, "stage": "build", "exit_code": code,
                    "stdout": out.decode(errors="replace"), "stderr": err.decode(errors="replace"),
                    "elapsed": time.time() - started, "panic": b"panicked at" in err}
        if compile_only:
            return {"program": str(program), "ok": True, "stage": "build", "elapsed": time.time() - started}
        ecode, eout, eerr = run([str(elf)], elf_dir, stdin, timeout)
        icode, iout, ierr = run([str(TC), "interp", "--cwd", str(interp_dir), str(program)], interp_dir, stdin, timeout)
        efiles, ifiles = tree(elf_dir), tree(interp_dir)
        ok = (ecode == icode and eout == iout and eerr == ierr and efiles == ifiles)
        result = {
            "program": str(program), "ok": ok, "stage": "run",
            "elf_exit": ecode, "interp_exit": icode,
            "stdout_match": eout == iout, "stderr_match": eerr == ierr,
            "files_match": efiles == ifiles,
            "stdout": eout.decode(errors="replace"), "stderr": eerr.decode(errors="replace"),
            "elf_stderr": eerr.decode(errors="replace"), "interp_stderr": ierr.decode(errors="replace"),
            "elapsed": time.time() - started,
            "panic": b"panicked at" in eerr or b"panicked at" in ierr,
        }
        if check_static:
            result["static"] = static_check(elf)
            result["ok"] = result["ok"] and all(result["static"].values())
        return result
    except Timeout as exc:
        return {"program": str(program), "ok": False, "stage": "timeout", "error": str(exc),
                "elapsed": time.time() - started}
    except Exception as exc:
        return {"program": str(program), "ok": False, "stage": "harness", "error": repr(exc),
                "elapsed": time.time() - started}
    finally:
        shutil.rmtree(tmp, ignore_errors=True)

def probe_clean_error(program, timeout=20):
    """A deliberately invalid program must fail cleanly, with a located diagnostic."""
    program = pathlib.Path(program).resolve()
    tmp = pathlib.Path(tempfile.mkdtemp(prefix="converr_", dir="/tmp/opencode"))
    elf = tmp / "should_not_exist.elf"
    started = time.time()
    try:
        code, out, err = run([str(TC), "build", str(program), "-o", str(elf)], program.parent, timeout=timeout)
        text = err.decode(errors="replace")
        panicked = "panicked at" in text
        located = bool(re.search(r"tc build: .*?:\d+:\d+:", text))
        structured_import = bool(re.search(r"tc build: imported module `[^`]+` was not found", text))
        clean = code != 0 and (located or structured_import) and not panicked and not elf.exists()
        return {"program": str(program), "ok": clean, "stage": "clean_error",
                "exit_code": code, "stderr": text, "stdout": out.decode(errors="replace"),
                "located": located, "structured_import": structured_import, "panic": panicked, "elapsed": time.time() - started}
    except Timeout as exc:
        return {"program": str(program), "ok": False, "stage": "timeout", "error": str(exc)}
    except Exception as exc:
        return {"program": str(program), "ok": False, "stage": "harness", "error": repr(exc)}
    finally:
        shutil.rmtree(tmp, ignore_errors=True)

def probe_omitted(program, prose_lines, stdin=b"", timeout=20):
    """Verify each declared prose line is COMMENT_OMITTED, and parity still holds."""
    base = probe(program, stdin, timeout)
    if not base.get("ok"):
        return base
    code, text, err = trace_sentences(program)
    if code != 0:
        return {"program": str(program), "ok": False, "stage": "trace", "trace_stderr": err}
    lines = text.splitlines()
    missing = []
    for sentence in prose_lines:
        target = sentence.strip().rstrip(".")
        found = False
        for index, line in enumerate(lines):
            if line.strip().rstrip(".") == target:
                prev = lines[index - 1] if index else ""
                if "COMMENT_OMITTED" in prev:
                    found = True
                    break
        if not found:
            missing.append(sentence)
    return {"program": str(program), "ok": not missing, "stage": "omitted",
            "missing": missing, "base": base}

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("program")
    parser.add_argument("--stdin", default="")
    parser.add_argument("--timeout", type=float, default=20)
    parser.add_argument("--compile-only", action="store_true")
    parser.add_argument("--static", action="store_true")
    parser.add_argument("--expect-error", action="store_true")
    args = parser.parse_args()
    if args.expect_error:
        result = probe_clean_error(args.program, args.timeout)
    else:
        result = probe(args.program, args.stdin.encode(), args.timeout, args.compile_only, args.static)
    print(json.dumps(result, ensure_ascii=False))
    return 0 if result.get("ok") else 1

if __name__ == "__main__":
    raise SystemExit(main())
