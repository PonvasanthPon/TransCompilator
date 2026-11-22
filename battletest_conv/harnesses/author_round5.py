#!/usr/bin/env python3
"""Author 128 brand-new Round 5 programs, the clean-round candidate."""
import json,pathlib,shutil
HERE=pathlib.Path(__file__).resolve().parent; ROOT=HERE.parent; REPO=ROOT.parent; R5=ROOT/"rounds"/"round_05"
BANDS={1:"band_1_basic",2:"band_2_casual",3:"band_3_lexical",4:"band_4_anaphora",5:"band_5_compound",6:"band_6_complex",7:"band_7_fullstack",8:"band_8_adversarial"}
MANIFEST=[]

def write(band,name,source,*,expect="run",prose=None):
    d=R5/BANDS[band];d.mkdir(parents=True,exist_ok=True);p=d/name;p.write_text(source)
    MANIFEST.append({"round":5,"band":f"B{band}","name":name,"program":str(p.relative_to(REPO)),"expect":expect,"prose":prose or [],"stdin":""})

def b1():
    for i in range(1,17):
        k=(i-1)%4
        if k==0:
            s=f'''Use io.
Pen is {12+i}.
Notebook is {28+i}.
Wallet is {260+i*5}.
Set wallet to ({220+i*4} plus 30), add pen to it, remove ({7+i} plus 3) from it, and say it.
If wallet is more than 200, say "stationery bought".
Say wallet.
Give back 0.
'''
        elif k==1:
            s=f'''Use io.
Rent is {400+i*8}.
Salary is {900+i*17}.
Remove rent from salary.
If salary is less than 600, say "tight month".
Otherwise salary is enough.
Show salary.
Give back 0.
'''.replace('Otherwise salary is enough.\n','')
        elif k==2:
            s=f'''Use io.
N is {2+i%5}.
Price is {40+i}.
Total is n times price.
Cash is {500+i*3}.
Remove total from cash.
If cash is less than 300, say "low cash".
Print cash.
Give back 0.
'''
        else:
            s=f'''Use io.
Bags is {3+i%6}.
Weight is 0.
For every bag from 1 to bags, add {5+i%4} to weight.
If weight is more than 25, say "heavy load".
Display weight.
Give back 0.
'''
        write(1,f"r5_b1_{i:02d}.eng",s)

def b2():
    stories=[("Anand","the express train",16,"It was misty at dawn.","My cousin painted the fence yesterday."),
             ("Deepa","the local bus",23,"It is 9 am.","The market was full of flowers."),
             ("Kumar","the ferry",12,"It was a sunny morning.","My aunt cooked rice and dal."),
             ("Bhavna","the office cab",18,"It is almost 5 pm.","The children finished their homework.")]
    for i in range(1,17):
        name,vehicle,wait,l1,l2=stories[(i-1)%4]
        cash=350+i*11;cost=9+i
        s=f'''Use io.

Yesterday {name} waited {wait} minutes for {vehicle}.
{l1}
Cash is {cash}.
{l2}
Add 75 to cash.
It was unusually quiet today.
Remove {cost} from cash.
Show cash.
Print cash.
It is almost dinner time.
Give back 0.
'''
        write(2,f"r5_b2_{i:02d}.eng",s,prose=[f"Yesterday {name} waited {wait} minutes for {vehicle}.",l1,l2,"It was unusually quiet today.","It is almost dinner time."])

def b3():
    for i in range(1,17):
        k=(i-1)%4
        if k==0:
            s=f'''Use alloc, core, io, string.
Let amount be {400+i} as an unsigned number.
Let rate be {i}.75.
Let enabled be true.
Let title be create text from "round five types" of length 16.
Say "synonym matrix".
Show amount.
Print rate.
Display enabled.
Show address of title.
Print address of title.
Display address of title.
Release text address of title.
Give back 0.
'''
        elif k==1:
            s=f'''Use io.
Cash is {1400+i*11}.
{80+i} is removed from cash.
{125+i} is added to cash.
Increase cash by {i+3}.
Decrease cash by {i%4+2}.
Withdraw {25+i} from cash.
Spend {12+i%6} from cash.
Display cash.
Give back 0.
'''
        elif k==2:
            s=f'''Use io.
Bank is {2100+i*19}.
Wallet is {95+i}.
Transfer {350+i} from bank to wallet.
Move {55+i} from bank to wallet.
Withdraw {30+i} from bank.
Show bank.
Print wallet.
Give back 0.
'''
        else:
            s=f'''Use io and math.
Number is {i*3.25+2}.
Root is square root of number.
Show root.
Display root.
If root is more than 2.5, say "root high".
Print root.
Give back 0.
'''
        write(3,f"r5_b3_{i:02d}.eng",s)

def b4():
    for i in range(1,17):
        k=(i-1)%4
        if k==0:
            s=f'''Use io.
A record Worker has a monthly wage (a whole number), a tax (a whole number), a total tax (a whole number), and a travel allowance (a whole number).
Let worker be Worker.
Update worker so its monthly wage is {8000+i*23}, its tax is {400+i}, its total tax is {1600+i*6}, and its travel allowance is {140+i}.
Let net wage be worker's monthly wage minus worker's total tax.
Say net wage.
Add worker's travel allowance to it.
Say it.
Remove {40+i} from it.
Say net wage.
Give back 0.
'''
        elif k==1:
            s=f'''Use io.
A record Point has an x (a whole number) and a y (a whole number).
A record Box has a width (a whole number) and a height (a whole number).
Let point be Point.
Let box be Box.
Update point so its x is {i+4} and its y is {i+8}.
Update box so its width is {i+12} and its height is {i+16}.
Let size be box's width times box's height.
Say size.
Add point's x to it.
Say it.
Add point's y to size.
Say size.
Give back 0.
'''
        elif k==2:
            s=f'''Use io.
A record Ticket has a seat price (a whole number), a count (a whole number), a fee (a whole number), and a total (a whole number).
Let ticket be Ticket.
Update ticket so its seat price is {80+i}, its count is {i%5+3}, its fee is {15+i}, and its total is ticket's seat price times ticket's count plus ticket's fee.
Let payable be ticket's total.
Say payable.
Update ticket so its count is ticket's count plus 2, and its total is ticket's seat price times ticket's count plus ticket's fee.
Add ticket's total to payable.
Say payable.
Give back 0.
'''
        else:
            s=f'''Use io.
A record Ledger has an opening (a whole number), a rate (a whole number), and a closing (a whole number).
Let ledger be Ledger.
Update ledger so its opening is {900+i*8}, its rate is {i%10+2}, and its closing is ledger's opening plus ledger's opening times ledger's rate divided by 100.
Let change be ledger's closing minus ledger's opening.
Say change.
Add ledger's rate to it.
Say it.
Add {i+5} to change.
Say change.
Give back 0.
'''
        write(4,f"r5_b4_{i:02d}.eng",s)

def b5():
    for i in range(1,17):
        k=(i-1)%4
        if k==0:
            s=f'''Use io.
Set cash to ({250+i*5} plus 35), add ({7+i} times 4) to it, remove ({i} plus 19) from it, and say it.
Remove (cash divided by 4) from cash, add 11 to it, and say cash.
Give back 0.
'''
        elif k==1:
            s=f'''Use io.
Put ({350+i*6} plus 45) in cash, add ({8+i} times 3) to it, remove ({i} plus 22) from it, and say it.
If cash is more than 350, and cash is less than 1000, say "inside range".
If cash is more than 1000, or cash is less than 250, say "outside range".
Say cash.
Give back 0.
'''
        elif k==2:
            s=f'''Use io.
Cash is {800+i*7}.
Add (cash divided by 8) to cash, remove (cash divided by 16) from cash, and say cash.
If cash is more than 800, and cash is less than 1500, say "medium cash".
Display cash.
Give back 0.
'''
        else:
            s=f'''Use io.
Let a be {i+1}.
Let b be {i+3}.
Let c be {i+5}.
Let d be {i+7}.
Let result be 0.
If a is more than 0, and b is more than 0, and c is more than 0, and d is more than 0, add 1 to result.
If a is more than 60 or b is more than 60, and c is more than 60, or d is more than 60, add 2 to result.
If it is false that a is more than 60 and b is more than 60, add 4 to result.
Print result.
Give back 0.
'''
        write(5,f"r5_b5_{i:02d}.eng",s)

def b6():
    for i in range(1,17):
        k=(i-1)%4
        if k==0:
            s=f'''Use io.
Let total be 0.
Let hits be 0.
For every i from 1 to {9+i}:
    For every j from 1 to 6:
        If i times j is more than {15+i}:
            Add j to hits.
        Otherwise:
            Add i to total.
Say total.
Say hits.
Give back 0.
'''
        elif k==1:
            start=50+i
            s=f'''Use io.
Let total be 0.
For every i from {start} to 1 by -3:
    If i is between 15 and 30:
        Continue.
    If i is less than 8:
        Break.
    Add i to total.
Say total.
Give back 0.
'''
        elif k==2:
            s=f'''Use io.
Define local values as array<whole number, 9>.
Set item 0 of values to {i}.
Set item 1 of values to {i+1}.
Set item 2 of values to {i+2}.
Set item 3 of values to {i+3}.
Set item 4 of values to {i+4}.
Set item 5 of values to {i+5}.
Set item 6 of values to {i+6}.
Set item 7 of values to {i+7}.
Set item 8 of values to {i+8}.
Let checks be 0.
If every x in values is more than 0, add 1 to checks.
If some x in values is more than {i+7}, add 2 to checks.
If no x in values is less than 0, add 4 to checks.
Say checks.
Give back 0.
'''
        else:
            s=f'''Use io.
Let total be 0.
Let i be 0.
Until i is at least {10+i%8}, add {i%5+2} to total, add 1 to i.
Let a be {i}.
Let b be {i+5}.
Let c be {i+9}.
Let answer be the sum of a and b and c, where a is {i*2}, b is {i*3}, and c is {i*6}.
Say total.
Say answer.
Give back 0.
'''
        write(6,f"r5_b6_{i:02d}.eng",s)

def b7():
    for i in range(1,17):
        k=(i-1)%4
        if k==0:
            s=f'''Use alloc, core, io, map.
Let table be IntMap of whole number.
Let k be 0.
While k is less than {380+i*5}.
Put (k times {i%9+2}) modulo 4001 into int map address of table for key k.
Add 1 to k.
End the loop.
Let found be look up {370+i} in int map address of table.
If found.has_value, say found.value, otherwise say -1.
Let size be count of int map address of table.
Say size.
Let removed be intmap_remove(address of table, {25+i}).
Say removed.
Free int map address of table.
Let used be bytes in use.
Say used.
Give back 0.
'''
        elif k==1:
            s=f'''Use alloc, core, io, map, string.
Keep a shared key store as array<TcString, 380>.
Let table be StrMap of whole number.
Let buffer be block of 16 byte.
Let k be 0.
While k is less than 380.
Let length be format k into address of item 0 of buffer.
Let key be create text from address of item 0 of buffer of length length.
Set item k of key store to key.
Put k into text map address of table for text address of item k of key store.
Add 1 to k.
End the loop.
Let size be count of text map address of table.
Say size.
Let found be look up text address of item {330+i} of key store in text map address of table.
If found.has_value, say found.value, otherwise say -1.
Free text map address of table.
Let j be 0.
While j is less than 380.
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
While i is less than {200+i}.
Push (i times {i%14+3}) modulo 503 to address of nums.
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
            size=24000+i*41
            s=f'''Use alloc, core, fs, io, string.
Let name be create text from "round_five_{i}.bin" of length {len(f"round_five_{i}.bin")}.
Let p be allocate {size} bytes.
Set p[0] to {i%250}.
Set p[{size-1}] to {i%200}.
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
Let buffer be block of 32 byte.
Let count read be file_read_bytes(address of read handle, address of item 0 of buffer, 32).
Say count read.
file_close(address of read handle).
Release memory at p.
Delete file address of name.
Release text address of name.
Let used be bytes in use.
Say used.
Give back 0.
'''
        write(7,f"r5_b7_{i:02d}.eng",s)

def b8():
    invalid=[
      ("r5_b8_01_unresolved_it.eng",'Use io.\nIf 4 is more than 3:\n    Let hidden be 9.\n    Say hidden.\nSay it.\nGive back 0.\n'),
      ("r5_b8_02_unresolved_its.eng",'Use io.\nA record Parcel has a weight (a whole number).\nSay its weight.\nGive back 0.\n'),
      ("r5_b8_03_zero_step.eng",'Use io.\nLet total be 0.\nFor every i from 1 to 8 by 0.\nAdd i to total.\nEnd the loop.\nSay total.\nGive back 0.\n'),
      ("r5_b8_04_missing_module.eng",'Use absent_module_five.\nGive back 0.\n'),
      ("r5_b8_05_unclosed_text.eng",'Use io.\nSay "never finished.\nGive back 0.\n'),
      ("r5_b8_06_recursive_record.eng",'Use io.\nA record Nesting has a child (a Nesting).\nLet nesting be Nesting.\nSay nesting\'s child.\nGive back 0.\n'),
      ("r5_b8_07_bad_update.eng",'Use io.\nA record Parcel has a weight (a whole number).\nLet parcel be Parcel.\nUpdate parcel so its height is 4.\nGive back 0.\n'),
      ("r5_b8_08_bad_put.eng",'Use io.\nPut 12 in 5.\nGive back 0.\n'),
      ("r5_b8_09_bad_arithmetic.eng",'Use io.\nLet x be 7 times "six".\nSay x.\nGive back 0.\n'),
      ("r5_b8_10_bad_condition.eng",'Use io.\nLet x be 9.\nIf x is at least or at most 4, add 1 to x.\nSay x.\nGive back 0.\n'),
      ("r5_b8_11_missing_phrase.eng",'Use io.\nLet x be absent_phrase_five(9, 10).\nSay x.\nGive back 0.\n'),
      ("r5_b8_12_empty_update.eng",'Use io.\nA record Parcel has a weight (a whole number).\nLet parcel be Parcel.\nUpdate parcel so its.\nGive back 0.\n'),
      ("r5_b8_13_deep_not.eng",'Use io.\nLet x be 1.\nIf ' + 'not ' * 52 + 'x equals 1, say 1.\nGive back 0.\n'),
      ("r5_b8_14_deep_parens.eng",'Use io.\nLet x be ' + '(' * 80 + '1' + ')' * 80 + '.\nSay x.\nGive back 0.\n'),
    ]
    for name,src in invalid:
        write(8,name,src,expect="clean_error")
    write(8,"r5_b8_15_oob_get_trap.eng",'''Use alloc, core, io, list.
Let nums be List of whole number.
Push 3 to address of nums.
Let value be list_get(address of nums, 5).
Say value.
Free list address of nums.
Give back 0.
''')
    write(8,"r5_b8_16_missing_file_trap.eng",'''Use alloc, core, fs, io, string.
Let name be create text from "definitely_absent_five.bin" of length 26.
Let opened be file_open_read(address of name).
If not opened's is_ok.
Stop with error "missing file".
End the condition.
Release text address of name.
Give back 0.
''')

def main():
    if R5.exists(): shutil.rmtree(R5)
    for d in BANDS.values(): (R5/d).mkdir(parents=True,exist_ok=True)
    b1();b2();b3();b4();b5();b6();b7();b8()
    (R5/"MANIFEST.json").write_text(json.dumps(MANIFEST,indent=2)+"\n")
    counts={b:sum(1 for m in MANIFEST if m["band"]==f"B{b}") for b in range(1,9)}
    print(json.dumps({"total":len(MANIFEST),"bands":counts},indent=2))
if __name__=="__main__": main()
