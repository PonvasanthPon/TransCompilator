#!/usr/bin/env python3
"""Author 128 brand-new Round 2 programs, heavily stressing Round 1 fix areas."""
import json, pathlib, shutil

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parent
REPO = ROOT.parent
R2 = ROOT / "rounds" / "round_02"
BANDS = {
 1: "band_1_basic", 2: "band_2_casual", 3: "band_3_lexical", 4: "band_4_anaphora",
 5: "band_5_compound", 6: "band_6_complex", 7: "band_7_fullstack", 8: "band_8_adversarial",
}
MANIFEST = []

def write(band, name, source, *, expect="run", prose=None, stdin=""):
    directory = R2 / BANDS[band]
    directory.mkdir(parents=True, exist_ok=True)
    path = directory / name
    path.write_text(source)
    if stdin:
        path.with_suffix(".stdin").write_text(stdin)
    MANIFEST.append({"round": 2, "band": f"B{band}", "name": name,
                     "program": str(path.relative_to(REPO)), "expect": expect,
                     "prose": prose or [], "stdin": stdin})

def b1():
    for i in range(1, 17):
        kind = (i - 1) % 4
        if kind == 0:
            tomato, potato, wallet = 12 + i, 20 + i, 280 + i * 6
            src = f'''Use io.
Tomato is {tomato}.
Potato is {potato}.
Wallet is {wallet}.
Add tomato to wallet.
Remove {35 + i} from wallet.
If wallet is more than 250, say "enough cash".
Say wallet.
Give back 0.
'''
        elif kind == 1:
            balance, topup = 40 + i, 60 + i * 2
            src = f'''Use io.
Balance is {balance}.
Topup is {topup}.
Add topup to balance.
If balance is less than 100, say "low balance".
If balance is at least 100, say "active line".
Say balance.
Give back 0.
'''
        elif kind == 2:
            base, km = 30 + i, 4 + i % 5
            src = f'''Use io.
Base is {base}.
Km is {km}.
Total is base plus km times 12.
Pay is 500.
Remove total from pay.
Say total.
If total is less than 100, say "short trip".
Say pay.
Give back 0.
'''
        else:
            exam, project, total = 50 + i, 20 + i, 0
            src = f'''Use io.
Exam is {exam}.
Project is {project}.
Total is exam plus project.
If total is more than 70, say "pass".
If total is at most 70, say "retry".
Passes is 0.
If total is more than 70, add 1 to passes.
Say total.
Say passes.
Give back 0.
'''
        write(1, f"r2_b1_{i:02d}.eng", src)

def b2():
    stories = [
        ("Meena", "the train", 15, "It was very windy yesterday."),
        ("Arun", "the bus", 22, "The tea was hot."),
        ("Kala", "the ferry", 9, "It is 6 pm."),
        ("Vikram", "the metro", 18, "My cousin bought mangoes yesterday."),
    ]
    for i in range(1, 17):
        name, vehicle, wait, line = stories[(i - 1) % 4]
        cash = 180 + i * 7
        spend = 3 + i
        src = f'''Use io.

Yesterday {name} waited {wait} minutes for {vehicle}.
{line}
Start cash is {cash}.
It was a long day.
Add 45 to cash.
The market was crowded.
Remove {spend} from cash.
Show cash.
It is evening now.
Give back 0.
'''
        write(2, f"r2_b2_{i:02d}.eng", src,
              prose=[f"Yesterday {name} waited {wait} minutes for {vehicle}.", line, "It was a long day.", "The market was crowded.", "It is evening now."])

def b3():
    for i in range(1, 17):
        kind = (i - 1) % 4
        if kind == 0:
            src = f'''Use alloc, core, io, string.

Let amount be {100 + i} as an unsigned number.
Let ratio be {i}.25.
Let flag be true.
Let label be create text from "round two output" of length 16.
Say {i}.
Show {i + 10}.
Print {i}.75.
Display flag.
Show amount.
Print ratio.
Display address of label.
Release text address of label.
Give back 0.
'''
        elif kind == 1:
            cash, rem, add = 650 + i * 13, 90 + i, 45 + i
            src = f'''Use io.

Cash is {cash}.
{rem} is removed from cash.
{add} is added to cash.
Increase cash by {i}.
Decrease cash by {i % 5 + 1}.
Double cash.
Halve cash.
Show cash.
Print cash.
Display cash.
Give back 0.
'''
        elif kind == 2:
            src = f'''Use io.

Bank is {800 + i * 11}.
Wallet is {60 + i}.
Transfer {150 + i} from bank to wallet.
Move {25 + i} from bank to wallet.
Withdraw {10 + i} from bank.
Spend {5 + i % 6} from wallet.
Show bank.
Display wallet.
Give back 0.
'''
        else:
            src = f'''Use io.

Score is {40 + i}.
Say "score card".
If score is more than 30, say "scored".
Add {i} to score.
Print score.
Display score.
Give back 0.
'''
        write(3, f"r2_b3_{i:02d}.eng", src)

def b4():
    for i in range(1, 17):
        kind = (i - 1) % 4
        if kind == 0:
            salary, tax, total_tax = 5000 + i * 21, 250 + i, 1000 + i * 3
            src = f'''Use io.

A record Account has a monthly salary (a whole number), a tax (a whole number), and a total tax (a whole number).
Let account be Account.
Update account so its monthly salary is {salary}, its tax is {tax}, and its total tax is {total_tax}.
Let monthly budget be account's monthly salary minus account's total tax.
Say monthly budget.
Add {50 + i} to it.
Say it.
Remove {20 + i} from it.
Say it.
Give back 0.
'''
        elif kind == 1:
            px, py, qx, qy = i + 1, i + 4, i + 7, i + 10
            src = f'''Use io.

A record Point has an x (a whole number) and a y (a whole number).
Let first point be Point.
Let second point be Point.
Update first point so its x is {px} and its y is {py}.
Update second point so its x is {qx} and its y is {qy}.
Let total be first point's x plus second point's y.
Say total.
Add first point's y to it.
Say total.
Give back 0.
'''
        elif kind == 2:
            price, qty, rate = 30 + i, i % 6 + 2, i % 4 + 1
            src = f'''Use io.

A record Invoice has a unit price (a whole number), a quantity (a whole number), a tax rate (a whole number), and a total cost (a whole number).
Let invoice be Invoice.
Update invoice so its unit price is {price}, its quantity is {qty}, its tax rate is {rate}, and its total cost is invoice's unit price times invoice's quantity.
Let final cost be invoice's total cost plus invoice's total cost times invoice's tax rate divided by 100.
Say final cost.
Update invoice so its quantity is invoice's quantity plus 2, and its total cost is invoice's unit price times invoice's quantity.
Add invoice's total cost to final cost.
Say final cost.
Give back 0.
'''
        else:
            base, rate, offset = 400 + i * 9, i % 7 + 1, i + 3
            src = f'''Use io.

A record Ledger has a base (a whole number) and a rate (a whole number).
Let ledger be Ledger.
Update ledger so its base is {base} and its rate is {rate}.
Let result be ledger's base times ledger's rate.
Say result.
Add ledger's rate to result.
Say it.
Add {offset} to it.
Say result.
Give back 0.
'''
        write(4, f"r2_b4_{i:02d}.eng", src)

def b5():
    for i in range(1, 17):
        kind = (i - 1) % 4
        if kind == 0:
            a, b, c, d = 150 + i, 60 + i, 25 + i, 10 + i
            src = f'''Use io.

Put {a} in cash, add {b} to it, remove {c} from it, remove {d} from it, and say it.
Give back 0.
'''
        elif kind == 1:
            a, b, c, d = 240 + i * 4, 35 + i, 12 + i, 8 + i
            src = f'''Use io.

Set cash to {a}, add {b} to it, remove {c} from it, and say it.
Set cash to cash plus {d}, add {b} to it, and say cash.
Give back 0.
'''
        elif kind == 2:
            cash, add, remove = 300 + i * 3, 55 + i, 20 + i
            src = f'''Use io.

Cash is {cash}.
Add {add} to cash, remove {remove} from cash, add 5 to cash, and say cash.
Remove 10 from cash, add 15 to cash, and show cash.
Print cash.
Give back 0.
'''
        else:
            a, b, c, d = i, i + 3, i + 6, i + 9
            src = f'''Use io.

Let a be {a}.
Let b be {b}.
Let c be {c}.
Let d be {d}.
Let result be 0.
If a is more than 0, and b is more than 0, and c is less than {c + 20}, and d is less than {d + 20}, add 1 to result.
If a is more than 100 or b is more than 100, and c is more than 0, or d is more than 0, add 2 to result.
Say result.
Give back 0.
'''
        write(5, f"r2_b5_{i:02d}.eng", src)

def b6():
    for i in range(1, 17):
        kind = (i - 1) % 4
        if kind == 0:
            src = f'''Use io.

Let total be 0.
Let hits be 0.
For every i from 1 to {9 + i}:
    For every j from 1 to 4:
        If i times j is more than {12 + i}:
            Add 1 to hits.
        Otherwise:
            Add i plus j to total.
Say total.
Say hits.
Give back 0.
'''
        elif kind == 1:
            start = 30 + i
            src = f'''Use io.

Let total be 0.
For every i from {start} to 3 by -3:
    If i is between 8 and 20:
        Continue.
    If i is less than 5:
        Break.
    Add i to total.
Say total.
Give back 0.
'''
        elif kind == 2:
            src = f'''Use io.

Define local samples as array<whole number, 6>.
Set item 0 of samples to {i}.
Set item 1 of samples to {i + 2}.
Set item 2 of samples to {i + 4}.
Set item 3 of samples to {i + 6}.
Set item 4 of samples to {i + 8}.
Set item 5 of samples to {i + 10}.
Let result be 0.
If every x in samples is more than 0, add 1 to result.
If some x in samples is more than {i + 9}, add 2 to result.
If no x in samples is less than 0, add 4 to result.
Say result.
Give back 0.
'''
        else:
            src = f'''Use io.

Let total be 0.
Let i be 0.
Until i is at least {6 + i % 5}, add {i % 4 + 1} to total, add 1 to i.
Let a be {i}.
Let b be {i + 5}.
Let c be {i + 9}.
Let result be the sum of a and b and c, where a is {i * 2}, b is {i * 3}, and c is {i * 4}.
Say total.
Say result.
Give back 0.
'''
        write(6, f"r2_b6_{i:02d}.eng", src)

def b7():
    for i in range(1, 17):
        kind = (i - 1) % 4
        if kind == 0:
            src = f'''Use alloc, core, io, map.

Let table be IntMap of whole number.
Let k be 0.
While k is less than {330 + i * 2}.
Put (k times {i % 9 + 2}) modulo 1009 into int map address of table for key k.
Add 1 to k.
End the loop.
Let found be look up {320 + i} in int map address of table.
If found.has_value, say found.value, otherwise say -1.
Let map size be count of int map address of table.
Say map size.
Let removed be intmap_remove(address of table, {12 + i}).
Say removed.
Free int map address of table.
Let used be bytes in use.
Say used.
Give back 0.
'''
        elif kind == 1:
            src = f'''Use alloc, core, io, map, string.

Keep a shared key store as array<TcString, 300>.
Let table be StrMap of whole number.
Let buffer be block of 16 byte.
Let k be 0.
While k is less than 300.
Let length be format k into address of item 0 of buffer.
Let key be create text from address of item 0 of buffer of length length.
Set item k of key store to key.
Put k as a whole number into text map address of table for text address of item k of key store.
Add 1 to k.
End the loop.
Let text size be count of text map address of table.
Say text size.
Let found be look up text address of item {250 + i} of key store in text map address of table.
If found.has_value, say found.value, otherwise say -1.
Free text map address of table.
Let j be 0.
While j is less than 300.
Release text address of item j of key store.
Add 1 to j.
End the loop.
Let used be bytes in use.
Say used.
Give back 0.
'''
        elif kind == 2:
            count = 130 + i
            src = f'''Use alloc, core, io, list.

Let nums be List of whole number.
Let i be 0.
While i is less than {count}.
Push (i times {i % 11 + 3}) modulo 211 to address of nums.
Add 1 to i.
End the loop.
Let total be 0.
Let idx be 0 as an unsigned number.
Let list size be count of address of nums.
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
        else:
            size = 12000 + i * 29
            src = f'''Use alloc, core, fs, io, string.

Let name be create text from "round_two_{i}.bin" of length {len(f"round_two_{i}.bin")}.
Let p be allocate {size} bytes.
Set p[0] to {i % 250}.
Set p[{size - 1}] to {i % 220}.
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
file_seek(address of read handle, 0, 0).
Let buffer be block of 4 byte.
Let count read be file_read_bytes(address of read handle, address of item 0 of buffer, 4).
Say count read.
file_close(address of read handle).
Release memory at p.
Delete file address of name.
Release text address of name.
Let remaining bytes be bytes in use.
Say remaining bytes.
Give back 0.
'''
        write(7, f"r2_b7_{i:02d}.eng", src)

def b8():
    invalid = [
        ("r2_b8_01_unresolved_it.eng", 'Use io.\nIf 1 is more than 0.\nLet y be 2.\nEnd the condition.\nSay it.\nGive back 0.\n'),
        ("r2_b8_02_unresolved_its.eng", 'Use io.\nA record Node has a value (a whole number).\nSay its value.\nGive back 0.\n'),
        ("r2_b8_03_zero_step.eng", 'Use io.\nLet total be 0.\nFor every i from 1 to 5 by 0.\nAdd i to total.\nEnd the loop.\nSay total.\nGive back 0.\n'),
        ("r2_b8_04_missing_module.eng", 'Use absent_module_two.\nSay 1.\nGive back 0.\n'),
        ("r2_b8_05_unclosed_text.eng", 'Use io.\nSay "never closed.\nGive back 0.\n'),
        ("r2_b8_06_recursive_record.eng", 'Use io.\nA record Chain has a next (a Chain).\nLet chain be Chain.\nSay chain\'s next.\nGive back 0.\n'),
        ("r2_b8_07_bad_update_field.eng", 'Use io.\nA record Point has an x (a whole number).\nLet point be Point.\nUpdate point so its y is 4.\nGive back 0.\n'),
        ("r2_b8_08_bad_put_target.eng", 'Use io.\nPut 10 in 3.\nGive back 0.\n'),
        ("r2_b8_09_bad_arithmetic.eng", 'Use io.\nLet x be "text" plus 3.\nSay x.\nGive back 0.\n'),
        ("r2_b8_10_bad_condition.eng", 'Use io.\nLet x be 4.\nIf x is more than and less than 9, add 1 to x.\nSay x.\nGive back 0.\n'),
        ("r2_b8_11_missing_phrase.eng", 'Use io.\nLet x be missing_phrase_two(1, 2, 3).\nSay x.\nGive back 0.\n'),
        ("r2_b8_12_empty_update.eng", 'Use io.\nA record Point has an x (a whole number).\nLet point be Point.\nUpdate point so its.\nGive back 0.\n'),
        ("r2_b8_13_deep_not.eng", 'Use io.\nLet x be 1.\nIf ' + 'not ' * 45 + 'x equals 1, say 1.\nGive back 0.\n'),
        ("r2_b8_14_deep_parens.eng", 'Use io.\nLet x be ' + '(' * 95 + '1' + ')' * 95 + '.\nSay x.\nGive back 0.\n'),
    ]
    for name, src in invalid:
        write(8, name, src, expect="run" if "deep_parens" in name else "clean_error")
    write(8, "r2_b8_15_empty_pop_trap.eng", '''Use alloc, core, io, list.

Let nums be List of whole number.
Let value be pop from address of nums.
Say value.
Free list address of nums.
Give back 0.
''')
    write(8, "r2_b8_16_oob_set_trap.eng", '''Use alloc, core, io, list.

Let nums be List of whole number.
Push 4 to address of nums.
list_set(address of nums, 4, 9).
Free list address of nums.
Give back 0.
''')

def main():
    if R2.exists():
        shutil.rmtree(R2)
    for directory in BANDS.values():
        (R2 / directory).mkdir(parents=True, exist_ok=True)
    b1(); b2(); b3(); b4(); b5(); b6(); b7(); b8()
    (R2 / "MANIFEST.json").write_text(json.dumps(MANIFEST, indent=2) + "\n")
    counts = {b: sum(1 for m in MANIFEST if m["band"] == f"B{b}") for b in range(1, 9)}
    print(json.dumps({"total": len(MANIFEST), "bands": counts}, indent=2))

if __name__ == "__main__":
    main()
