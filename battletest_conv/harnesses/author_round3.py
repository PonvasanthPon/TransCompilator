#!/usr/bin/env python3
"""Author 128 brand-new Round 3 programs; target all prior fix neighborhoods."""
import json, pathlib, shutil

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parent
REPO = ROOT.parent
R3 = ROOT / "rounds" / "round_03"
BANDS = {1:"band_1_basic",2:"band_2_casual",3:"band_3_lexical",4:"band_4_anaphora",5:"band_5_compound",6:"band_6_complex",7:"band_7_fullstack",8:"band_8_adversarial"}
MANIFEST=[]

def write(band,name,source,*,expect="run",prose=None,stdin=""):
    d=R3/BANDS[band]; d.mkdir(parents=True,exist_ok=True); p=d/name; p.write_text(source)
    if stdin: p.with_suffix(".stdin").write_text(stdin)
    MANIFEST.append({"round":3,"band":f"B{band}","name":name,"program":str(p.relative_to(REPO)),"expect":expect,"prose":prose or [],"stdin":stdin})

def b1():
    for i in range(1,17):
        k=(i-1)%4
        if k==0:
            s=f'''Use io.
Milk is {24+i}.
Curd is {30+i}.
Purse is {320+i*4}.
Set purse to {300+i*5}, add milk to it, remove {18+i} from it, and say it.
If purse is more than 300, say "enough for milk".
Say purse.
Give back 0.
'''
        elif k==1:
            s=f'''Use io.
Data is {2+i%4}.
Pack is {50+i}.
Balance is {10+i}.
Add pack to data.
If data is more than 6, say "data loaded".
Data left is data minus {i%3+1}.
Say data.
Say data left.
Give back 0.
'''
        elif k==2:
            s=f'''Use io.
Fine is {15+i}.
Days is {3+i%5}.
Charge is fine times days.
Deposit is {100+i*2}.
Remove charge from deposit.
If deposit is less than 50, say "pay more".
Say charge.
Say deposit.
Give back 0.
'''
        else:
            s=f'''Use io.
Crates is {4+i%6}.
Boxes is 0.
For every crate from 1 to crates, add {2+i%4} to boxes.
If boxes is more than 12, say "full load".
Say boxes.
Give back 0.
'''
        write(1,f"r3_b1_{i:02d}.eng",s)

def b2():
    lines=[
      ("Sita","the night train",17,"It was foggy that morning.","My uncle visited us yesterday."),
      ("Gopal","the city bus",25,"It is 7 am.","The bakery smelled wonderful."),
      ("Nila","the auto",11,"It was a warm afternoon.","My neighbour watered the plants."),
      ("Hari","the tram",14,"It is nearly noon.","The children played outside."),
    ]
    for i in range(1,17):
        name,vehicle,wait,l1,l2=lines[(i-1)%4]
        wallet=220+i*9; fare=4+i
        s=f'''Use io.

Yesterday {name} waited {wait} minutes for {vehicle}.
{l1}
Wallet is {wallet}.
{l2}
Add 35 to wallet.
It is getting late.
Remove {fare} from wallet.
Show wallet.
Print wallet.
It is dinner time.
Give back 0.
'''
        write(2,f"r3_b2_{i:02d}.eng",s,prose=[f"Yesterday {name} waited {wait} minutes for {vehicle}.",l1,l2,"It is getting late.","It is dinner time."])

def b3():
    for i in range(1,17):
        k=(i-1)%4
        if k==0:
            s=f'''Use alloc, core, io, string.

Let count be {200+i} as an unsigned number.
Let fraction be {i}.125.
Let ready be false.
Let text be create text from "round three text" of length 16.
Say "output matrix".
Show count.
Print fraction.
Display ready.
Show address of text.
Print address of text.
Display address of text.
Release text address of text.
Give back 0.
'''
        elif k==1:
            s=f'''Use io.
Cash is {900+i*7}.
{50+i} is removed from cash.
{80+i} is added to cash.
Increase cash by {i+1}.
Decrease cash by {i%4+1}.
Withdraw {20+i} from cash.
Show cash.
Display cash.
Give back 0.
'''
        elif k==2:
            s=f'''Use io.
Bank is {1500+i*13}.
Wallet is {70+i}.
Transfer {240+i} from bank to wallet.
Move {35+i} from bank to wallet.
Spend {11+i%7} from wallet.
Print bank.
Show wallet.
Say "transfer done".
Give back 0.
'''
        else:
            s=f'''Use io and math.
Root is square root of {i*1.25+1}.
Print root.
Display root.
If root is more than 1.0, say "root above one".
Show root.
Give back 0.
'''
        write(3,f"r3_b3_{i:02d}.eng",s)

def b4():
    for i in range(1,17):
        k=(i-1)%4
        if k==0:
            s=f'''Use io.
A record Account has a monthly salary (a whole number), a tax (a whole number), a total tax (a whole number), and a credit (a whole number).
Let account be Account.
Update account so its monthly salary is {6000+i*31}, its tax is {300+i}, its total tax is {1200+i*4}, and its credit is {90+i}.
Let monthly budget be account's monthly salary minus account's total tax.
Say monthly budget.
Add account's credit to it.
Say it.
Remove {25+i} from it.
Say monthly budget.
Give back 0.
'''
        elif k==1:
            s=f'''Use io.
A record Point has an x (a whole number) and a y (a whole number).
A record Rect has a width (a whole number) and a height (a whole number).
Let point be Point.
Let rect be Rect.
Update point so its x is {i+2} and its y is {i+5}.
Update rect so its width is {i+7} and its height is {i+11}.
Let area be rect's width times rect's height.
Say area.
Add point's x to it.
Say it.
Add point's y to area.
Say area.
Give back 0.
'''
        elif k==2:
            s=f'''Use io.
A record Purchase has a unit price (a whole number), a quantity (a whole number), a discount (a whole number), and a net total (a whole number).
Let purchase be Purchase.
Update purchase so its unit price is {60+i}, its quantity is {i%7+3}, its discount is {10+i}, and its net total is purchase's unit price times purchase's quantity minus purchase's discount.
Let amount due be purchase's net total.
Say amount due.
Update purchase so its quantity is purchase's quantity plus 3, and its net total is purchase's unit price times purchase's quantity minus purchase's discount.
Add purchase's net total to amount due.
Say amount due.
Give back 0.
'''
        else:
            s=f'''Use io.
A record Ledger has a start (a whole number), a rate (a whole number), and a result (a whole number).
Let ledger be Ledger.
Update ledger so its start is {700+i*6}, its rate is {i%8+2}, and its result is ledger's start times ledger's rate.
Let total be ledger's result.
Say total.
Add ledger's rate to it.
Say it.
Add {i+2} to total.
Say total.
Give back 0.
'''
        write(4,f"r3_b4_{i:02d}.eng",s)

def b5():
    for i in range(1,17):
        k=(i-1)%4
        if k==0:
            s=f'''Use io.
Set cash to ({100+i*2} plus 20), add ({5+i} times 2) to it, remove ({i} plus 7) from it, and say it.
Set cash to (cash times 2), remove (cash divided by 4) from it, add 9 to it, and say cash.
Give back 0.
'''
        elif k==1:
            s=f'''Use io.
Put ({200+i*3} plus 50) in cash, add ({7+i} times 3) to it, remove ({i} plus 11) from it, and say it.
If cash is more than 200, say "high total".
Otherwise is not used here.
Give back 0.
'''
            # remove deliberately invalid line before writing
            s=s.replace('Otherwise is not used here.\n','')
        elif k==2:
            s=f'''Use io.
Cash is {500+i*5}.
Add ({i} plus 10) to cash, remove ({i} plus 2) from cash, add (3 times {i%5+2}) to cash, and say cash.
If cash is more than 500, and cash is less than 900, say "middle band".
If cash is more than 900, or cash is less than 300, say "outer band".
Show cash.
Give back 0.
'''
        else:
            s=f'''Use io.
Let a be {i}.
Let b be {i+2}.
Let c be {i+4}.
Let d be {i+6}.
Let result be 0.
If a is more than 0, and b is more than 0, and c is more than 0, and d is more than 0, add 1 to result.
If a is more than 100 or b is more than 100, and c is more than 100, or d is more than 100, add 2 to result.
If it is false that a is more than 100 and b is more than 100, add 4 to result.
Say result.
Give back 0.
'''
        write(5,f"r3_b5_{i:02d}.eng",s)

def b6():
    for i in range(1,17):
        k=(i-1)%4
        if k==0:
            s=f'''Use io.
Let total be 0.
Let hits be 0.
For every i from 1 to {7+i}:
    For every j from 1 to 4:
        For every k from 1 to 3:
            If i plus j plus k is more than {10+i}:
                Add 1 to hits.
            Otherwise:
                Add 1 to total.
Say total.
Say hits.
Give back 0.
'''
        elif k==1:
            start=40+i
            s=f'''Use io.
Let total be 0.
For every i from {start} to 2 by -2:
    If i is between 10 and 24:
        Continue.
    If i is less than 6:
        Break.
    Add i to total.
Say total.
Give back 0.
'''
        elif k==2:
            s=f'''Use io.
Define local values as array<whole number, 7>.
Set item 0 of values to {i}.
Set item 1 of values to {i+1}.
Set item 2 of values to {i+2}.
Set item 3 of values to {i+3}.
Set item 4 of values to {i+4}.
Set item 5 of values to {i+5}.
Set item 6 of values to {i+6}.
Let status be 0.
If every x in values is more than 0, add 1 to status.
If some x in values is more than {i+5}, add 2 to status.
If no x in values is less than 0, add 4 to status.
While some x in values is more than {i+20}.
Add 1 to status.
Break.
End the loop.
Say status.
Give back 0.
'''
        else:
            s=f'''Use io.
Let total be 0.
Let i be 0.
Until i is at least {8+i%6}, add {i%3+2} to total, add 1 to i.
Let a be {i}.
Let b be {i+3}.
Let c be {i+6}.
Let result be the sum of a and b and c, where a is {i*2}, b is {i*3}, and c is {i*4}.
Say total.
Say result.
Give back 0.
'''
        write(6,f"r3_b6_{i:02d}.eng",s)

def b7():
    for i in range(1,17):
        k=(i-1)%4
        if k==0:
            s=f'''Use alloc, core, io, map.
Let table be IntMap of whole number.
Let k be 0.
While k is less than {350+i*3}.
Put (k times {i%7+3}) modulo 2011 into int map address of table for key k.
Add 1 to k.
End the loop.
Let found be look up {340+i} in int map address of table.
If found.has_value, say found.value, otherwise say -1.
Let size be count of int map address of table.
Say size.
Let removed be intmap_remove(address of table, {15+i}).
Say removed.
Free int map address of table.
Let used be bytes in use.
Say used.
Give back 0.
'''
        elif k==1:
            s=f'''Use alloc, core, io, map, string.
Keep a shared key store as array<TcString, 340>.
Let table be StrMap of whole number.
Let buffer be block of 16 byte.
Let k be 0.
While k is less than 340.
Let length be format k into address of item 0 of buffer.
Let key be create text from address of item 0 of buffer of length length.
Set item k of key store to key.
Put k into text map address of table for text address of item k of key store.
Add 1 to k.
End the loop.
Let size be count of text map address of table.
Say size.
Let found be look up text address of item {300+i} of key store in text map address of table.
If found.has_value, say found.value, otherwise say -1.
Free text map address of table.
Let j be 0.
While j is less than 340.
Release text address of item j of key store.
Add 1 to j.
End the loop.
Let used be bytes in use.
Say used.
Give back 0.
'''
        elif k==2:
            s=f'''Use alloc, core, io, list.
Let nums be List of whole number.
Let i be 0.
While i is less than {160+i}.
Push (i times {i%13+2}) modulo 307 to address of nums.
Add 1 to i.
End the loop.
Let total be 0.
Let idx be 0 as an unsigned number.
Let size be count of address of nums.
While idx is less than size.
Let value be item idx of nums's data.
Add value to total.
Add 1 to idx.
End the loop.
Say size.
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
            size=16000+i*31
            s=f'''Use alloc, core, fs, io, string.
Let name be create text from "round_three_{i}.bin" of length {len(f"round_three_{i}.bin")}.
Let p be allocate {size} bytes.
Set p[0] to {i%255}.
Set p[{size-1}] to {i%210}.
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
Let buffer be block of 8 byte.
Let count read be file_read_bytes(address of read handle, address of item 0 of buffer, 8).
Say count read.
file_close(address of read handle).
Release memory at p.
Delete file address of name.
Release text address of name.
Let used be bytes in use.
Say used.
Give back 0.
'''
        write(7,f"r3_b7_{i:02d}.eng",s)

def b8():
    invalid=[
      ("r3_b8_01_unresolved_it.eng", 'Use io.\nIf 2 is more than 1:\n    Let inner be 5.\n    Say inner.\nSay it.\nGive back 0.\n'),
      ("r3_b8_02_unresolved_its.eng", 'Use io.\nA record Box has a size (a whole number).\nSay its size.\nGive back 0.\n'),
      ("r3_b8_03_zero_step.eng", 'Use io.\nLet total be 0.\nFor every i from 1 to 9 by 0.\nAdd i to total.\nEnd the loop.\nSay total.\nGive back 0.\n'),
      ("r3_b8_04_missing_module.eng", 'Use absent_module_three.\nGive back 0.\n'),
      ("r3_b8_05_unclosed_text.eng", 'Use io.\nSay "not finished.\nGive back 0.\n'),
      ("r3_b8_06_recursive_record.eng", 'Use io.\nA record Nest has a child (a Nest).\nLet nest be Nest.\nSay nest\'s child.\nGive back 0.\n'),
      ("r3_b8_07_bad_update.eng", 'Use io.\nA record Tiny has a count (a whole number).\nLet tiny be Tiny.\nUpdate tiny so its amount is 3.\nGive back 0.\n'),
      ("r3_b8_08_bad_put.eng", 'Use io.\nPut 4 in 8.\nGive back 0.\n'),
      ("r3_b8_09_bad_arithmetic.eng", 'Use io.\nLet x be 3 plus "four".\nSay x.\nGive back 0.\n'),
      ("r3_b8_10_bad_condition.eng", 'Use io.\nLet x be 5.\nIf x is more than or less than 8, add 1 to x.\nSay x.\nGive back 0.\n'),
      ("r3_b8_11_missing_phrase.eng", 'Use io.\nLet x be absent_phrase_three(4, 5).\nSay x.\nGive back 0.\n'),
      ("r3_b8_12_empty_update.eng", 'Use io.\nA record Tiny has a count (a whole number).\nLet tiny be Tiny.\nUpdate tiny so its.\nGive back 0.\n'),
      ("r3_b8_13_deep_not.eng", 'Use io.\nLet x be 1.\nIf ' + 'not ' * 48 + 'x equals 1, say 1.\nGive back 0.\n'),
      ("r3_b8_14_deep_parens.eng", 'Use io.\nLet x be ' + '(' * 120 + '1' + ')' * 120 + '.\nSay x.\nGive back 0.\n'),
    ]
    for name,src in invalid:
        write(8,name,src,expect="run" if "deep_parens" in name else "clean_error")
    write(8,"r3_b8_15_oob_list_trap.eng",'''Use alloc, core, io, list.
Let nums be List of whole number.
Push 1 to address of nums.
Let value be item 2 of nums's data.
Say value.
Free list address of nums.
Give back 0.
''')
    write(8,"r3_b8_16_empty_first_trap.eng",'''Use alloc, core, io, list.
Let nums be List of whole number.
Let value be first item of address of nums.
Say value.
Free list address of nums.
Give back 0.
''')

def main():
    if R3.exists(): shutil.rmtree(R3)
    for d in BANDS.values(): (R3/d).mkdir(parents=True,exist_ok=True)
    b1();b2();b3();b4();b5();b6();b7();b8()
    (R3/"MANIFEST.json").write_text(json.dumps(MANIFEST,indent=2)+"\n")
    counts={b:sum(1 for m in MANIFEST if m["band"]==f"B{b}") for b in range(1,9)}
    print(json.dumps({"total":len(MANIFEST),"bands":counts},indent=2))
if __name__=="__main__": main()
