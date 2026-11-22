#!/usr/bin/env python3
"""Author the 128 fresh Round 1 programs across all eight English bands."""
import json, pathlib, shutil

ROOT = pathlib.Path(__file__).resolve().parent.parent
REPO = ROOT.parent
R1 = ROOT / "rounds" / "round_01"
BANDS = {
 1: ("band_1_basic", "Simple Basic & Non-Native English"),
 2: ("band_2_casual", "Casual Conversational & Storytelling"),
 3: ("band_3_lexical", "Lexical Variety, Passive & VerbNet Transfers"),
 4: ("band_4_anaphora", "ACE Anaphora, Multi-Word Names & Update"),
 5: ("band_5_compound", "Compound Sentences & Comma Precedence"),
 6: ("band_6_complex", "Complex & Compound-Complex Sentences"),
 7: ("band_7_fullstack", "Full-Stack std/ Multi-Module Systems"),
 8: ("band_8_adversarial", "Adversarial Ambiguity & Boundary Probes"),
}
MANIFEST = []

def write(band, name, source, *, expect="run", prose=None, stdin=""):
    directory = R1 / BANDS[band][0]
    directory.mkdir(parents=True, exist_ok=True)
    path = directory / name
    path.write_text(source)
    if stdin:
        path.with_suffix(".stdin").write_text(stdin)
    MANIFEST.append({"round": 1, "band": f"B{band}", "name": name,
                     "program": str(path.relative_to(REPO)), "expect": expect,
                     "prose": prose or [], "stdin": stdin})

def b1():
    for i in range(1, 17):
        kind = (i - 1) % 4
        if kind == 0:
            tea, snack, cash, spend = i + 4, i + 11, 400 + i * 7, i * 3
            src = f'''Use io.

Today shop busy.
Tea is {tea}.
Snacks is {snack}.
Cash is {cash}.
Add tea to cash.
Remove {spend} from cash.
If tea is more than 5, say "tea is costly".
Say cash.
Give back 0.
'''
        elif kind == 1:
            fare, pocket, threshold = i + 2, 100 + i * 5, 80 + i
            src = f'''Use io.

Busfare is {fare}.
Pocket is {pocket}.
Remove busfare from pocket.
If pocket more than {threshold}, add 10 to pocket.
Say pocket.
Give back 0.
'''
        elif kind == 2:
            a, b, threshold = i + 20, i + 33, 45 + i
            src = f'''Use io.

Exam is {a}.
Homework is {b}.
Total is exam plus homework.
Passes is 0.
If total is more than {threshold}, add 1 to passes.
Say total.
Say passes.
Give back 0.
'''
        else:
            bottles, litres = i % 5 + 2, i % 3 + 1
            src = f'''Use io.

Bottles is {bottles}.
Litres is 0.
For every bottle from 1 to bottles, add {litres} to litres.
Say litres.
Give back 0.
'''
        write(1, f"b1_{i:02d}.eng", src)

def b2():
    prose_pool = [
        "Yesterday Ravi called me twice.",
        "It was raining heavily.",
        "It is 3 pm.",
        "My brother drank 2 teas.",
        "We waited 20 minutes for the bus.",
        "The shop was very busy because of rain.",
        "My sister bought fresh milk yesterday.",
        "The kettle boiled for five minutes.",
        "The report was late.",
        "It is late.",
    ]
    for i in range(1, 17):
        cash = 200 + i * 13
        spend = i * 4
        first, second, third = prose_pool[i % 10], prose_pool[(i + 3) % 10], prose_pool[(i + 6) % 10]
        src = f'''Use io.

{first}
Cash is {cash}.
{second}
Add 50 to cash.
{third}
Remove {spend} from cash.
Say cash.
It is evening now.
Give back 0.
'''
        write(2, f"b2_{i:02d}.eng", src, prose=[first, second, third, "It is evening now."])

def b3():
    for i in range(1, 17):
        kind = (i - 1) % 4
        if kind == 0:
            cash, rem, add, factor, deduct = 500 + i * 9, 60 + i, 25 + i, i % 4 + 2, 30 + i
            src = f'''Use io.

Cash is {cash}.
{rem} is removed from cash.
{add} is added to cash.
Multiply cash by {factor}.
Deduct {deduct} from cash.
Say cash.
Give back 0.
'''
        elif kind == 1:
            bank, wallet = 700 + i * 11, 40 + i
            src = f'''Use io.

Bank is {bank}.
Wallet is {wallet}.
Transfer {100 + i} from bank to wallet.
Move {20 + i} from bank to wallet.
Show bank.
Display wallet.
Give back 0.
'''
        elif kind == 2:
            score, temp = 40 + i, 70 + i
            src = f'''Use io.

Score is {score}.
Temperature is {temp}.
Increase score by {i}.
Decrease temperature by {i % 7 + 1}.
Double score.
Halve temperature.
Spend {i % 5 + 2} from score.
Print score.
Show temperature.
Give back 0.
'''
        else:
            src = f'''Use alloc, core, io, string.

Let name be create text from "round one" of length 9.
Say {i}.
Show {i + 1}.
Print {i}.5.
Display true.
Say address of name.
Show address of name.
Release text address of name.
Give back 0.
'''
        write(3, f"b3_{i:02d}.eng", src)

def b4():
    for i in range(1, 17):
        kind = (i - 1) % 4
        if kind == 0:
            salary, tax, total_tax, bonus = 4200 + i * 17, 200 + i, 900 + i * 2, 50 + i
            src = f'''Use io.

A record Account has a monthly salary (a whole number), a tax (a whole number), and a total tax (a whole number).
Let account be Account.
Update account so its monthly salary is {salary}, its tax is {tax}, and its total tax is {total_tax}.
Let monthly budget be account's monthly salary minus account's total tax.
Say monthly budget.
Let tax be {tax}.
Add tax to monthly budget.
Say it.
Let bonus be {bonus}.
Add bonus to it.
Say monthly budget.
Give back 0.
'''
        elif kind == 1:
            x, y = i + 3, i + 8
            src = f'''Use io.

A record Point has an x (a whole number) and a y (a whole number).
Let point be Point.
Update point so its x is {x} and its y is {y}.
Let total be point's x plus point's y.
Say total.
Set point's x to {x + 1}.
Add point's x to total.
Say total.
Give back 0.
'''
        elif kind == 2:
            price, count = 20 + i, i % 5 + 2
            src = f'''Use io.

A record Purchase has a unit price (a whole number), a quantity (a whole number), and a total price (a whole number).
Let purchase be Purchase.
Update purchase so its unit price is {price}, its quantity is {count}, and its total price is purchase's unit price times purchase's quantity.
Let basket total be purchase's total price.
Say basket total.
Update purchase so its quantity is purchase's quantity plus 1, and its total price is purchase's unit price times purchase's quantity.
Add purchase's total price to basket total.
Say basket total.
Give back 0.
'''
        else:
            base, rate = 300 + i * 4, i % 6 + 1
            src = f'''Use io.

A record Ledger has a base (a whole number) and a rate (a whole number).
Let ledger be Ledger.
Update ledger so its base is {base} and its rate is {rate}.
Let result be ledger's base times ledger's rate.
Say result.
Let offset be ledger's rate plus 2.
Add offset to it.
Say result.
Give back 0.
'''
        write(4, f"b4_{i:02d}.eng", src)

def b5():
    for i in range(1, 17):
        kind = (i - 1) % 4
        if kind == 0:
            a, b, c = 100 + i, 40 + i, 15 + i
            src = f'''Use io.

Put {a} in cash, add {b} to it, remove {c} from it, and say it.
Give back 0.
'''
        elif kind == 1:
            cash, add, remove = 200 + i * 5, 30 + i, 10 + i
            src = f'''Use io.

Cash is {cash}.
Add {add} to cash, remove {remove} from cash, and say cash.
Give back 0.
'''
        elif kind == 2:
            a, b = 20 + i, 35 + i
            src = f'''Use io.

To smaller given a as a whole number, b as a whole number, named smaller, giving a whole number.
Give back a if a is less than b, otherwise give back b.

Say smaller({a}, {b}).
Give back 0.
'''
        else:
            a, b, c, d = i, i + 2, i + 4, i + 6
            src = f'''Use io.

Let a be {a}.
Let b be {b}.
Let c be {c}.
Let d be {d}.
Let result be 0.
If a is more than 0 or b is more than 0, and c is less than {c + 10}, or d equals {d}, add 1 to result.
Say result.
Give back 0.
'''
        write(5, f"b5_{i:02d}.eng", src)

def b6():
    for i in range(1, 17):
        kind = (i - 1) % 4
        if kind == 0:
            limit = 8 + i
            src = f'''Use io.

Let total be 0.
Let skipped be 0.
For every k from 1 to {limit}:
    If k is between 4 and {limit - 1} and k modulo 2 equals 0:
        Add k to total.
    Otherwise:
        Add 1 to skipped.
Say total.
Say skipped.
Give back 0.
'''
        elif kind == 1:
            start = 20 + i
            src = f'''Use io.

Let total be 0.
For every i from {start} to 2 by -2:
    If i equals {start - 6}:
        Continue.
    If i is less than 6:
        Break.
    Add i to total.
Say total.
Give back 0.
'''
        elif kind == 2:
            src = f'''Use io.

Let total be 0.
Let i be 0.
Until i is at least {5 + i % 6}, add {i % 3 + 1} to total, add 1 to i.
Let pass be 0.
If no d from 2 to total - 1 satisfies total modulo d equals 0.
Add 1 to pass.
End the condition.
Say total.
Say pass.
Give back 0.
'''
        else:
            src = f'''Use io.

Let a be {i}.
Let b be {i + 3}.
Let c be {i + 7}.
Let result be the sum of a and b and c, where a is {i * 2}, b is {i * 3}, and c is {i * 4}.
Say result.
Give back 0.
'''
        write(6, f"b6_{i:02d}.eng", src)

def b7():
    for i in range(1, 17):
        kind = (i - 1) % 4
        if kind == 0:
            src = f'''Use alloc, core, io.

Let round be 0.
While round is less than 24.
Let p be allocate {32 + i * 8} bytes.
Set p[0] to {i % 250}.
Let q be resize memory at p from {32 + i * 8} to {64 + i * 8}.
Set q[1] to {i % 200}.
Release memory at q.
Add 1 to round.
End the loop.
Let used be bytes in use.
Say used.
Give back 0.
'''
        elif kind == 1:
            n = 110 + i
            src = f'''Use alloc, core, io, list.

Let nums be List of whole number.
Let i be 0.
While i is less than {n}.
Push (i times {i % 7 + 2}) modulo 101 to address of nums.
Add 1 to i.
End the loop.
Let total be 0.
Let list size be count of address of nums.
Let idx be 0 as an unsigned number.
While idx is less than list size.
Let value be item idx of nums's data.
Add value to total.
Add 1 to idx.
End the loop.
Say list size.
Say total.
list_clear(address of nums).
Let remaining be count of address of nums.
Say remaining.
Free list address of nums.
Let used be bytes in use.
Say used.
Give back 0.
'''
        elif kind == 2:
            src = f'''Use alloc, core, io, map.

Let table be IntMap of whole number.
Let k be 0.
While k is less than {310 + i}.
Put (k times {i % 5 + 2}) modulo 997 into int map address of table for key k.
Add 1 to k.
End the loop.
Let found be look up {300 + i} in int map address of table.
If found.has_value, say found.value, otherwise say -1.
Let map size be count of int map address of table.
Say map size.
Let removed be intmap_remove(address of table, {5 + i}).
Say removed.
Free int map address of table.
Let used be bytes in use.
Say used.
Give back 0.
'''
        else:
            size = 9000 + i * 37
            src = f'''Use alloc, core, fs, io, string.

Let name be create text from "round_one_{i}.bin" of length {len(f"round_one_{i}.bin")}.
Let p be allocate {size} bytes.
Set p[0] to {i % 250}.
Set p[{size - 1}] to {i % 200}.
Let opened be file_open_write(address of name).
If not opened's is_ok.
Stop with error "open write failed".
End the condition.
Let handle be File.
Set handle to opened's ok_val.
Let written be file_write_bytes(address of handle, p, {size}).
Say written.
file_close(address of handle).
Let reader be file_open_read(address of name).
If not reader's is_ok.
Stop with error "open read failed".
End the condition.
Let read handle be File.
Set read handle to reader's ok_val.
Let size read be file_seek(address of read handle, 0, 2).
Say size read.
file_close(address of read handle).
Release memory at p.
Delete file address of name.
Release text address of name.
Let used be bytes in use.
Say used.
Give back 0.
'''
        write(7, f"b7_{i:02d}.eng", src)

def b8():
    invalid = [
        ("b8_01_unresolved_it.eng", 'Use io.\nSay it.\nGive back 0.\n'),
        ("b8_02_unresolved_its.eng", 'Use io.\nA record Point has an x (a whole number).\nSay its x.\nGive back 0.\n'),
        ("b8_03_unresolved_the.eng", 'Use io.\nSay the point\'s x.\nGive back 0.\n'),
        ("b8_04_zero_step.eng", 'Use io.\nLet total be 0.\nFor every i from 1 to 5 by 0.\nAdd i to total.\nEnd the loop.\nSay total.\nGive back 0.\n'),
        ("b8_05_deep_parens.eng", 'Use io.\nLet x be ' + '(' * 70 + '1' + ')' * 70 + '.\nSay x.\nGive back 0.\n'),
        ("b8_06_unclosed_quote.eng", 'Use io.\nLet s be create text from "broken of length 6.\nGive back 0.\n'),
        ("b8_07_bad_import.eng", 'Use definitely_missing_module.\nSay 1.\nGive back 0.\n'),
        ("b8_08_recursive_struct.eng", 'Use io.\nA record Node has a next (a Node) and a value (a whole number).\nLet node be Node.\nSay node\'s value.\nGive back 0.\n'),
        ("b8_09_bad_update.eng", 'Use io.\nA record Point has an x (a whole number).\nLet point be Point.\nUpdate point so its missing field is 1.\nGive back 0.\n'),
        ("b8_10_bad_condition.eng", 'Use io.\nLet x be 3.\nIf x is more than, add 1 to x.\nSay x.\nGive back 0.\n'),
        ("b8_11_conflicting_types.eng", 'Use io.\nLet x be 1.\nLet x be "text".\nSay x.\nGive back 0.\n'),
        ("b8_12_bad_phrase_call.eng", 'Use io.\nLet x be unknown_phrase(1, 2).\nSay x.\nGive back 0.\n'),
        ("b8_13_empty_update.eng", 'Use io.\nA record Point has an x (a whole number).\nLet point be Point.\nUpdate point so its.\nGive back 0.\n'),
        ("b8_14_deep_not.eng", 'Use io.\nLet x be 1.\nIf ' + 'not ' * 40 + 'x equals 1, say 1.\nGive back 0.\n'),
    ]
    for name, src in invalid:
        write(8, name, src, expect="run" if name == "b8_05_deep_parens.eng" else "clean_error")
    trap = '''Use alloc, core, io, list.

Let nums be List of whole number.
Push 5 to address of nums.
Say item 0 of nums's data.
Let value be list_get(address of nums, 3).
Say value.
Free list address of nums.
Give back 0.
'''
    write(8, "b8_15_list_oob_trap.eng", trap)
    trap2 = '''Use io and math.

Say sqrt(-1.0).
Give back 0.
'''
    write(8, "b8_16_negative_sqrt_trap.eng", trap2)

def main():
    if R1.exists():
        shutil.rmtree(R1)
    for band in BANDS.values():
        (R1 / band[0]).mkdir(parents=True, exist_ok=True)
    b1(); b2(); b3(); b4(); b5(); b6(); b7(); b8()
    (R1 / "MANIFEST.json").write_text(json.dumps(MANIFEST, indent=2) + "\n")
    counts = {b: sum(1 for m in MANIFEST if m["band"] == f"B{b}") for b in range(1, 9)}
    print(json.dumps({"total": len(MANIFEST), "bands": counts}, indent=2))

if __name__ == "__main__":
    main()
