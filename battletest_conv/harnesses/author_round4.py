#!/usr/bin/env python3
"""Author 128 brand-new Round 4 programs, the clean-round candidate."""
import json, pathlib, shutil

HERE=pathlib.Path(__file__).resolve().parent; ROOT=HERE.parent; REPO=ROOT.parent; R4=ROOT/"rounds"/"round_04"
BANDS={1:"band_1_basic",2:"band_2_casual",3:"band_3_lexical",4:"band_4_anaphora",5:"band_5_compound",6:"band_6_complex",7:"band_7_fullstack",8:"band_8_adversarial"}
MANIFEST=[]

def write(band,name,source,*,expect="run",prose=None):
    d=R4/BANDS[band]; d.mkdir(parents=True,exist_ok=True); p=d/name; p.write_text(source)
    MANIFEST.append({"round":4,"band":f"B{band}","name":name,"program":str(p.relative_to(REPO)),"expect":expect,"prose":prose or [],"stdin":""})

def b1():
    for i in range(1,17):
        k=(i-1)%4
        if k==0:
            s=f'''Use io.
Bread is {18+i}.
Cake is {35+i}.
Purse is {450+i*3}.
Set purse to ({400+i*4} plus 25), add bread to it, remove ({12+i} plus 5) from it, and say it.
If purse is more than 400, say "bakery paid".
Say purse.
Give back 0.
'''
        elif k==1:
            s=f'''Use io.
Data is {3+i%5}.
Pack is {40+i}.
Credit is {5+i}.
Add pack to data.
If data is more than 8, say "data full".
Data left is data minus {i%3+1}.
Say data.
Say data left.
Give back 0.
'''
        elif k==2:
            s=f'''Use io.
Fee is {120+i*3}.
Months is {3+i%5}.
Total is fee times months.
Grant is {500+i*2}.
Remove total from grant.
If grant is less than 100, say "need aid".
Show grant.
Give back 0.
'''
        else:
            s=f'''Use io.
Cans is {5+i%7}.
Litres is 0.
For every can from 1 to cans, add {2+i%3} to litres.
If litres is more than 20, say "tank ready".
Print litres.
Give back 0.
'''
        write(1,f"r4_b1_{i:02d}.eng",s)

def b2():
    stories=[("Ravi","the morning bus",19,"It was cloudy in the morning.","My aunt made soup yesterday."),
             ("Priya","the evening train",26,"It is 8 am.","The radio played old songs."),
             ("Mohan","the airport shuttle",13,"It was a quiet afternoon.","My cousin fixed the bicycle."),
             ("Latha","the school van",21,"It is almost midnight.","The garden looked fresh.")]
    for i in range(1,17):
        name,vehicle,wait,l1,l2=stories[(i-1)%4]
        purse=300+i*8; fare=6+i
        s=f'''Use io.

Yesterday {name} waited {wait} minutes for {vehicle}.
{l1}
Purse is {purse}.
{l2}
Add 60 to purse.
It was very cold outside.
Remove {fare} from purse.
Show purse.
Print purse.
It is bedtime now.
Give back 0.
'''
        write(2,f"r4_b2_{i:02d}.eng",s,prose=[f"Yesterday {name} waited {wait} minutes for {vehicle}.",l1,l2,"It was very cold outside.","It is bedtime now."])

def b3():
    for i in range(1,17):
        k=(i-1)%4
        if k==0:
            s=f'''Use alloc, core, io, string.
Let total be {300+i} as an unsigned number.
Let share be {i}.5.
Let valid be true.
Let tag be create text from "round four types" of length 16.
Say "types begin".
Display total.
Show share.
Print valid.
Display address of tag.
Print address of tag.
Show address of tag.
Release text address of tag.
Give back 0.
'''
        elif k==1:
            s=f'''Use io.
Cash is {1200+i*9}.
{70+i} is removed from cash.
{90+i} is added to cash.
Increase cash by {i+2}.
Decrease cash by {i%3+1}.
Deduct {15+i} from cash.
Display cash.
Print cash.
Say "cash done".
Give back 0.
'''
        elif k==2:
            s=f'''Use io.
Bank is {1800+i*17}.
Wallet is {80+i}.
Transfer {300+i} from bank to wallet.
Move {45+i} from bank to wallet.
Withdraw {22+i} from bank.
Spend {8+i%9} from wallet.
Show bank.
Display wallet.
Print wallet.
Give back 0.
'''
        else:
            s=f'''Use io and math.
Value is {i*2.5+0.5}.
Root is square root of value.
Show root.
Display root.
If root is more than 2.0, say "root large".
Print root.
Give back 0.
'''
        write(3,f"r4_b3_{i:02d}.eng",s)

def b4():
    for i in range(1,17):
        k=(i-1)%4
        if k==0:
            s=f'''Use io.
A record Employee has a monthly pay (a whole number), a tax (a whole number), a total tax (a whole number), and a bonus (a whole number).
Let employee be Employee.
Update employee so its monthly pay is {7000+i*29}, its tax is {350+i}, its total tax is {1400+i*5}, and its bonus is {120+i}.
Let take home be employee's monthly pay minus employee's total tax.
Say take home.
Add employee's bonus to it.
Say it.
Remove {30+i} from it.
Say take home.
Give back 0.
'''
        elif k==1:
            s=f'''Use io.
A record Point has an x (a whole number) and a y (a whole number).
A record Circle has a radius (a whole number) and a centre x (a whole number).
Let point be Point.
Let circle be Circle.
Update point so its x is {i+3} and its y is {i+6}.
Update circle so its radius is {i+9} and its centre x is {i+12}.
Let combined be point's x plus circle's centre x.
Say combined.
Add point's y to it.
Say it.
Add circle's radius to combined.
Say combined.
Give back 0.
'''
        elif k==2:
            s=f'''Use io.
A record Order has a unit cost (a whole number), a count (a whole number), a rebate (a whole number), and a net cost (a whole number).
Let order be Order.
Update order so its unit cost is {70+i}, its count is {i%6+4}, its rebate is {12+i}, and its net cost is order's unit cost times order's count minus order's rebate.
Let payable be order's net cost.
Say payable.
Update order so its count is order's count plus 4, and its net cost is order's unit cost times order's count minus order's rebate.
Add order's net cost to payable.
Say payable.
Give back 0.
'''
        else:
            s=f'''Use io.
A record Ledger has a start (a whole number), a rate (a whole number), and a result (a whole number).
Let ledger be Ledger.
Update ledger so its start is {800+i*7}, its rate is {i%9+2}, and its result is ledger's start times ledger's rate.
Let total be ledger's result.
Say total.
Add ledger's rate to it.
Say it.
Add {i+4} to total.
Say total.
Give back 0.
'''
        write(4,f"r4_b4_{i:02d}.eng",s)

def b5():
    for i in range(1,17):
        k=(i-1)%4
        if k==0:
            s=f'''Use io.
Set cash to ({200+i*4} plus 30), add ({8+i} times 3) to it, remove ({i} plus 13) from it, and say it.
Remove (cash divided by 5) from cash, add 7 to it, and say cash.
Give back 0.
'''
        elif k==1:
            s=f'''Use io.
Put ({300+i*5} plus 40) in cash, add ({9+i} times 2) to it, remove ({i} plus 17) from it, and say it.
If cash is more than 300, and cash is less than 900, say "middle total".
If cash is more than 900, or cash is less than 200, say "outer total".
Say cash.
Give back 0.
'''
        elif k==2:
            s=f'''Use io.
Cash is {700+i*6}.
Add (cash divided by 10) to cash, remove (cash divided by 20) from cash, and say cash.
If cash is more than 700, and cash is less than 1200, say "inside band".
Display cash.
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
If a is more than 50 or b is more than 50, and c is more than 50, or d is more than 50, add 2 to result.
If it is false that a is more than 50 and b is more than 50, add 4 to result.
Print result.
Give back 0.
'''
        write(5,f"r4_b5_{i:02d}.eng",s)

def b6():
    for i in range(1,17):
        k=(i-1)%4
        if k==0:
            s=f'''Use io.
Let total be 0.
Let hits be 0.
For every i from 1 to {8+i}:
    For every j from 1 to 5:
        If i plus j is more than {11+i}:
            Add j to hits.
        Otherwise:
            Add i to total.
Say total.
Say hits.
Give back 0.
'''
        elif k==1:
            start=45+i
            s=f'''Use io.
Let total be 0.
For every i from {start} to 1 by -2:
    If i is between 12 and 28:
        Continue.
    If i is less than 7:
        Break.
    Add i to total.
Say total.
Give back 0.
'''
        elif k==2:
            s=f'''Use io.
Define local marks as array<whole number, 8>.
Set item 0 of marks to {i}.
Set item 1 of marks to {i+2}.
Set item 2 of marks to {i+4}.
Set item 3 of marks to {i+6}.
Set item 4 of marks to {i+8}.
Set item 5 of marks to {i+10}.
Set item 6 of marks to {i+12}.
Set item 7 of marks to {i+14}.
Let flags be 0.
If every x in marks is more than 0, add 1 to flags.
If some x in marks is more than {i+13}, add 2 to flags.
If no x in marks is less than 0, add 4 to flags.
Say flags.
Give back 0.
'''
        else:
            s=f'''Use io.
Let total be 0.
Let i be 0.
Until i is at least {9+i%7}, add {i%4+2} to total, add 1 to i.
Let a be {i}.
Let b be {i+4}.
Let c be {i+8}.
Let answer be the sum of a and b and c, where a is {i*2}, b is {i*3}, and c is {i*5}.
Say total.
Say answer.
Give back 0.
'''
        write(6,f"r4_b6_{i:02d}.eng",s)

def b7():
    for i in range(1,17):
        k=(i-1)%4
        if k==0:
            s=f'''Use alloc, core, io, map.
Let table be IntMap of whole number.
Let k be 0.
While k is less than {360+i*4}.
Put (k times {i%8+2}) modulo 3001 into int map address of table for key k.
Add 1 to k.
End the loop.
Let found be look up {350+i} in int map address of table.
If found.has_value, say found.value, otherwise say -1.
Let size be count of int map address of table.
Say size.
Let removed be intmap_remove(address of table, {20+i}).
Say removed.
Free int map address of table.
Let used be bytes in use.
Say used.
Give back 0.
'''
        elif k==1:
            s=f'''Use alloc, core, io, map, string.
Keep a shared key store as array<TcString, 360>.
Let table be StrMap of whole number.
Let buffer be block of 16 byte.
Let k be 0.
While k is less than 360.
Let length be format k into address of item 0 of buffer.
Let key be create text from address of item 0 of buffer of length length.
Set item k of key store to key.
Put k into text map address of table for text address of item k of key store.
Add 1 to k.
End the loop.
Let size be count of text map address of table.
Say size.
Let found be look up text address of item {320+i} of key store in text map address of table.
If found.has_value, say found.value, otherwise say -1.
Free text map address of table.
Let j be 0.
While j is less than 360.
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
While i is less than {180+i}.
Push (i times {i%12+3}) modulo 401 to address of nums.
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
            size=20000+i*37
            s=f'''Use alloc, core, fs, io, string.
Let name be create text from "round_four_{i}.bin" of length {len(f"round_four_{i}.bin")}.
Let p be allocate {size} bytes.
Set p[0] to {i%255}.
Set p[{size-1}] to {i%205}.
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
Let buffer be block of 16 byte.
Let count read be file_read_bytes(address of read handle, address of item 0 of buffer, 16).
Say count read.
file_close(address of read handle).
Release memory at p.
Delete file address of name.
Release text address of name.
Let used be bytes in use.
Say used.
Give back 0.
'''
        write(7,f"r4_b7_{i:02d}.eng",s)

def b8():
    invalid=[
      ("r4_b8_01_unresolved_it.eng",'Use io.\nIf 3 is more than 2:\n    Let inner be 7.\n    Say inner.\nSay it.\nGive back 0.\n'),
      ("r4_b8_02_unresolved_its.eng",'Use io.\nA record Crate has a weight (a whole number).\nSay its weight.\nGive back 0.\n'),
      ("r4_b8_03_zero_step.eng",'Use io.\nLet total be 0.\nFor every i from 1 to 7 by 0.\nAdd i to total.\nEnd the loop.\nSay total.\nGive back 0.\n'),
      ("r4_b8_04_missing_module.eng",'Use absent_module_four.\nGive back 0.\n'),
      ("r4_b8_05_unclosed_text.eng",'Use io.\nSay "unfinished text.\nGive back 0.\n'),
      ("r4_b8_06_recursive_record.eng",'Use io.\nA record Loop has a child (a Loop).\nLet loop be Loop.\nSay loop\'s child.\nGive back 0.\n'),
      ("r4_b8_07_bad_update.eng",'Use io.\nA record Crate has a weight (a whole number).\nLet crate be Crate.\nUpdate crate so its volume is 9.\nGive back 0.\n'),
      ("r4_b8_08_bad_put.eng",'Use io.\nPut 9 in 2.\nGive back 0.\n'),
      ("r4_b8_09_bad_arithmetic.eng",'Use io.\nLet x be 8 minus "two".\nSay x.\nGive back 0.\n'),
      ("r4_b8_10_bad_condition.eng",'Use io.\nLet x be 6.\nIf x is less than or more than 3, add 1 to x.\nSay x.\nGive back 0.\n'),
      ("r4_b8_11_missing_phrase.eng",'Use io.\nLet x be absent_phrase_four(7, 8).\nSay x.\nGive back 0.\n'),
      ("r4_b8_12_empty_update.eng",'Use io.\nA record Crate has a weight (a whole number).\nLet crate be Crate.\nUpdate crate so its.\nGive back 0.\n'),
      ("r4_b8_13_deep_not.eng",'Use io.\nLet x be 1.\nIf ' + 'not ' * 50 + 'x equals 1, say 1.\nGive back 0.\n'),
      ("r4_b8_14_deep_parens.eng",'Use io.\nLet x be ' + '(' * 150 + '1' + ')' * 150 + '.\nSay x.\nGive back 0.\n'),
    ]
    for name,src in invalid:
        write(8,name,src,expect="clean_error")
    write(8,"r4_b8_15_oob_get_trap.eng",'''Use alloc, core, io, list.
Let nums be List of whole number.
Push 8 to address of nums.
Let value be list_get(address of nums, 2).
Say value.
Free list address of nums.
Give back 0.
''')
    write(8,"r4_b8_16_empty_last_trap.eng",'''Use alloc, core, io, list.
Let nums be List of whole number.
Let value be last item of address of nums.
Say value.
Free list address of nums.
Give back 0.
''')

def main():
    if R4.exists(): shutil.rmtree(R4)
    for d in BANDS.values(): (R4/d).mkdir(parents=True,exist_ok=True)
    b1();b2();b3();b4();b5();b6();b7();b8()
    (R4/"MANIFEST.json").write_text(json.dumps(MANIFEST,indent=2)+"\n")
    counts={b:sum(1 for m in MANIFEST if m["band"]==f"B{b}") for b in range(1,9)}
    print(json.dumps({"total":len(MANIFEST),"bands":counts},indent=2))
if __name__=="__main__": main()
