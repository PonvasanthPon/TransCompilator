#!/usr/bin/env python3
"""Generate the Phase 5 hand-written tier corpora."""
import pathlib, shutil, json

ROOT = pathlib.Path(__file__).resolve().parent.parent
CORP = ROOT / "corpora"

def reset(tier):
    path = CORP / f"tier_{tier}"
    if path.exists():
        shutil.rmtree(path)
    path.mkdir(parents=True)
    return path

def write(tier, name, source):
    path = CORP / f"tier_{tier}" / name
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(source)
    return path

def flat_if(condition, body, message):
    return f"If {condition}.\n{body}\nStop with error \"{message}\".\nEnd the condition.\n"

def s1():
    reset("s1")
    for i in range(1, 66):
        amount = i + 3
        variant = i % 5
        if variant == 0:
            body = f'''A record Account has a monthly rent (a whole number), a remaining balance (a whole number), and a next link (a pointer to Account).
Keep a shared total spent as a whole number.
Let account be Account.
Update account so its monthly rent is {amount}, its remaining balance is account's remaining balance plus {amount}, and its next link is no address.
Add account's remaining balance to total spent.
If total spent is not {amount}.
Stop with error "bad total".
End the condition.
Give back total spent.
'''
        elif variant == 1:
            body = f'''To scale x (a decimal) by factor (a decimal), giving a decimal.
Give back x times factor.

Let result be scale 2.5 by {i}.0.
Say result.
Give back 0.
'''
        elif variant == 2:
            body = f'''Keep a shared bytes as a whole number.
Keep a shared bytes in use as a whole number.
Set bytes to {i}.
Set bytes in use to {i + 1}.
If bytes plus bytes in use is not {2 * i + 1}.
Stop with error "collision join".
End the condition.
Give back bytes plus bytes in use.
'''
        elif variant == 3:
            body = f'''A record Node has a size (a whole number) and a next link (a pointer to Node).
Keep a shared first node as Node.
Keep a shared second node as Node.
Set first node's size to {i}.
Set second node's size to {i + 1}.
Set first node's next link to address of second node.
Let p be address of first node.
Add p's next link's size to p's size.
If p's size is not {2 * i + 1}.
Stop with error "nested possessive".
End the condition.
Give back p's size.
'''
        else:
            body = f'''Let i be 0.
Let total be 0.
Until i is at least {i % 7 + 1}, add i to total, add 1 to i.
Give back total if total is less than 100, otherwise give back total minus 100.
'''
        source = "Use core, io.\n\n" + body
        write("s1", f"s1_{i:03d}.eng", source)

def s2():
    reset("s2")
    for i in range(1, 66):
        variant = i % 5
        if variant == 0:
            source = f'''Use io.
It was raining outside.
Let total be 0.
For every n from 1 to {i % 9 + 2}, add n to total.
Say total.
Give back 0.
'''
        elif variant == 1:
            source = f'''Use io.
Yesterday Ravi called me twice.
Put {i} in cash.
Add 10 to it.
Say it.
Give back 0.
'''
        elif variant == 2:
            source = f'''Use io.
A record Point has an x (a whole number) and a y (a whole number).
Let point be Point.
Set point's x to {i}.
Set point's y to {i + 2}.
Say the point's x.
Say point's y.
Give back 0.
'''
        elif variant == 3:
            source = f'''Use io.
Let a be {i}.
Let b be 0.
If a == 0 or b == 0, and a is less than 100, add 1 to b.
Say b.
Give back 0.
'''
        else:
            source = f'''Use io.
Let n be {i % 20 + 2}.
Let count be 0.
If no d from 2 to n - 1 satisfies n % d == 0.
Add 1 to count.
End the condition.
Say count.
Give back 0.
'''
        write("s2", f"s2_{i:03d}.eng", source)

if __name__ == "__main__":
    s1(); s2()
    print("generated", len(list((CORP/'tier_s1').glob('*.eng'))), len(list((CORP/'tier_s2').glob('*.eng'))))

def s3():
    reset("s3")
    sizes = [0,1,2,7,8,15,16,17,24,31,32,33,63,64,127,128,1024,4095,4096,4097,65504,65536,131072,262144]
    for i in range(1, 56):
        variant = i % 4
        size = sizes[i % len(sizes)]
        if variant == 0:
            body = f'''Let p be allocate {max(size, 1)} bytes.
If p is empty.
Stop with error "null allocation".
End the condition.
Let place be p as an unsigned number.
If place modulo 16 is not 0.
Stop with error "unaligned allocation".
End the condition.
Set p[0] to 65.
Set p[{max(size, 1) - 1}] to 66.
Release memory at p.
Let used be bytes in use.
If used is not 0.
Stop with error "allocation leak".
End the condition.
Give back 0.
'''
        elif variant == 1:
            body = '''Let first be allocate 64 bytes.
Let second be allocate 64 bytes.
Let third be allocate 64 bytes.
Let fourth be allocate 64 bytes.
Set first[0] to 1.
Set fourth[0] to 4.
Release memory at second.
Release memory at third.
Let merged be allocate 96 bytes.
If merged is empty.
Stop with error "coalesce null".
End the condition.
If first[0] is not 1.
Stop with error "coalesce corrupted first".
End the condition.
If fourth[0] is not 4.
Stop with error "coalesce corrupted fourth".
End the condition.
Release memory at first.
Release memory at fourth.
Release memory at merged.
Let used be bytes in use.
If used is not 0.
Stop with error "coalesce leak".
End the condition.
Give back 0.
'''
        elif variant == 2:
            body = f'''Let p be allocate 16 bytes.
Set p[0] to 65.
Set p[15] to 66.
Let q be resize memory at p from 16 to {max(size, 32)}.
If q is empty.
Stop with error "realloc null".
End the condition.
If q[0] is not 65.
Stop with error "realloc lost first".
End the condition.
If q[15] is not 66.
Stop with error "realloc lost last".
End the condition.
Release memory at q.
Let used be bytes in use.
If used is not 0.
Stop with error "realloc leak".
End the condition.
Give back 0.
'''
        else:
            body = '''Let i be 0.
While i is less than 60.
Let p be allocate 32 bytes.
Set p[0] to 7.
Release memory at p.
Add 1 to i.
End the loop.
Let p be allocate 1 bytes.
Release memory at p.
Set p to no address.
Release memory at p.
Let empty block be allocate 0 bytes.
Release memory at empty block.
Let used be bytes in use.
If used is not 0.
Stop with error "churn leak".
End the condition.
Give back 0.
'''
        write("s3", f"s3_{i:03d}.eng", "Use alloc, core.\n\n" + body)

def s4():
    reset("s4")
    ints = ["0","1","-1","9","10","-10","999999999999","9223372036854775807","-9223372036854775808","18446744073709551615"]
    floats = ["0.0","-0.0","-0.00004","0.00006","0.99996","9.99999","-9.99999","123456.7891","19.6350","1.4142","-4.7500"]
    for i in range(1, 61):
        variant = i % 5
        if variant == 0:
            value = ints[i % len(ints)]
            body = f"Say {value}.\nGive back 0.\n"
            source = "Use io.\n\n" + body
        elif variant == 1:
            value = floats[i % len(floats)]
            body = f"Say {value}.\nSay true.\nSay false.\nGive back 0.\n"
            source = "Use io.\n\n" + body
        elif variant == 2:
            a = (i * 7) % 100 - 50
            b = (i * 3) % 40 + 1
            body = f"Let a be read a whole number from input.\nLet b be read a whole number from input.\nSay a plus b.\nSay a minus b.\nGive back 0.\n"
            source = "Use io.\n\n" + body
            write("s4", f"s4_{i:03d}.stdin", f"{a}\n{b}\n")
        elif variant == 3:
            x = (i % 12) * 1.25
            body = f'''Let root be square root of {x}.
Say root.
Let power be pow_i64(2, {i % 10}).
Say power.
Let absolute be abs_i64(0 minus {i}).
Say absolute.
Let clamped be clamp_i64({i}, 3, 17).
Say clamped.
Give back 0.
'''
            source = "Use io, math.\n\n" + body
        else:
            body = f'''Let a be {i}.
Let b be {i % 7}.
Say a.
Say b.
Say a times b.
Say a divided by 3.
Say a modulo 3.
Give back 0.
'''
            source = "Use io.\n\n" + body
        write("s4", f"s4_{i:03d}.eng", source)

def s5():
    reset("s5")
    for i in range(1, 66):
        variant = i % 6
        if variant == 0:
            body = f'''Let a be create text from "hello" of length 5.
Let b be create text from " world" of length 6.
Let joined be join address of a and address of b.
Say address of joined.
Let length be length of address of joined.
Say length.
Release text address of a.
Release text address of b.
Release text address of joined.
Let used be bytes in use.
If used is not 0.
Stop with error "string leak".
End the condition.
Give back 0.
'''
            source = "Use alloc, core, io, string.\n\n" + body
        elif variant == 1:
            body = f'''Let nums be List of whole number.
Let i be 0.
While i is less than {i % 25 + 3}.
Push i to address of nums.
Add 1 to i.
End the loop.
Let count be count of address of nums.
Say count.
Let first be first item of address of nums.
Say first.
Let last be last item of address of nums.
Say last.
Free list address of nums.
Let used be bytes in use.
If used is not 0.
Stop with error "list leak".
End the condition.
Give back 0.
'''
            source = "Use alloc, core, io, list.\n\n" + body
        elif variant == 2:
            body = f'''Let values be List of decimal.
Push 1.5 to address of values.
Push 2.25 to address of values.
Push {i}.5 to address of values.
Let count be count of address of values.
Say count.
Let result be 0.0.
Set result to last item of address of values.
Say result.
Free list address of values.
Give back 0.
'''
            source = "Use alloc, core, io, list.\n\n" + body
        elif variant == 3:
            body = f'''A record Point has an x (a whole number) and a y (a whole number).
Let points be List of Point.
Let p be Point.
Set p's x to {i}.
Set p's y to {i + 1}.
Push p to address of points.
Let q be pop from address of points.
Say q's x.
Say q's y.
Free list address of points.
Give back 0.
'''
            source = "Use alloc, core, io, list.\n\n" + body
        elif variant == 4:
            body = f'''To compare left (a pointer to whole number) with right (a pointer to whole number), named compare, giving a whole number.
Let left value be value at left.
Let right value be value at right.
Give back -1 if left value is less than right value.
Give back 1 if left value is more than right value.
Give back 0.

Let nums be List of whole number.
Push {i % 5 + 5} to address of nums.
Push 1 to address of nums.
Push {i % 4 + 2} to address of nums.
Sort address of nums using function compare.
Say item 0 of nums's data.
Say item 2 of nums's data.
Free list address of nums.
Give back 0.
'''
            source = "Use alloc, core, io, list.\n\n" + body
        else:
            body = f'''Let a be create text from "repeat" of length 6.
Let b be copy text address of a.
Let same be text address of a matches text address of b.
If same.
Say 1.
End the condition.
Clear text address of b.
Let length be length of address of b.
Say length.
Release text address of b.
Release text address of a.
Give back 0.
'''
            source = "Use alloc, core, io, string.\n\n" + body
        write("s5", f"s5_{i:03d}.eng", source)

def s6():
    reset("s6")
    for i in range(1, 56):
        variant = i % 5
        name = f"p5_{i}.txt"
        nlen = len(name)
        if variant == 0:
            body = f'''Let name be create text from "{name}" of length {nlen}.
Save {i} to file address of name.
Let present be file exists address of name.
Say present.
Display file address of name.
Delete file address of name.
Let gone be file exists address of name.
Say gone.
Release text address of name.
Let used be bytes in use.
If used is not 0.
Stop with error "fs string leak".
End the condition.
Give back 0.
'''
        elif variant == 1:
            text = f"record{i}"
            body = f'''Let name be create text from "{name}" of length {nlen}.
Let text be create text from "{text}" of length {len(text)}.
Save address of text to file address of name.
Display file address of name.
Delete file address of name.
Release text address of text.
Release text address of name.
Let used be bytes in use.
If used is not 0.
Stop with error "fs content leak".
End the condition.
Give back 0.
'''
        elif variant == 2:
            body = f'''Let name be create text from "{name}" of length {nlen}.
Let text be create text from "ABCDEFGH" of length 8.
Let opened be file_open_write(address of name).
If not opened's is_ok.
Stop with error "open write failed".
End the condition.
Let handle be File.
Set handle to opened's ok_val.
Let written be file_write_bytes(address of handle, text's data, text's len).
If written is not 8.
Stop with error "short write".
End the condition.
file_close(address of handle).
Let reader be file_open_read(address of name).
If not reader's is_ok.
Stop with error "open read failed".
End the condition.
Let read handle be File.
Set read handle to reader's ok_val.
Let size be file_seek(address of read handle, 0, 2).
Say size.
file_seek(address of read handle, 0, 0).
Let buffer be block of 16 byte.
Let count be file_read_bytes(address of read handle, address of item 0 of buffer, 8).
Say count.
Say bytes address of item 0 of buffer of length count as an unsigned number.
file_close(address of read handle).
Delete file address of name.
Release text address of text.
Release text address of name.
Give back 0.
'''
        elif variant == 3:
            first = f"ab{i % 10}"
            second = f"cd{i % 10}"
            body = f'''Let name be create text from "{name}" of length {nlen}.
Let first be create text from "{first}" of length {len(first)}.
Let second be create text from "{second}" of length {len(second)}.
Save address of first to file address of name.
Let opened be file_open_append(address of name).
If not opened's is_ok.
Stop with error "append open failed".
End the condition.
Let handle be File.
Set handle to opened's ok_val.
file_write_bytes(address of handle, second's data, second's len).
file_close(address of handle).
Display file address of name.
Delete file address of name.
Release text address of first.
Release text address of second.
Release text address of name.
Give back 0.
'''
        else:
            body = f'''Let name be create text from "missing_{i}.txt" of length {len(f"missing_{i}.txt")}.
Let opened be file_open_read(address of name).
Say opened's is_ok.
If not opened's is_ok.
Let err be opened's err_val.
If err is at least 0.
Stop with error "missing file errno".
End the condition.
End the condition.
Release text address of name.
Give back 0.
'''
        write("s6", f"s6_{i:03d}.eng", "Use alloc, core, fs, io, string.\n\n" + body)

def s7():
    reset("s7")
    for i in range(1, 56):
        variant = i % 5
        if variant == 0:
            body = f'''Let table be IntMap of whole number.
Let k be 0.
While k is less than {i % 20 + 3}.
Put k times 2 into int map address of table for key k.
Add 1 to k.
End the loop.
Let found be look up {i % 20} in int map address of table.
If found.has_value, say 1, otherwise say 0.
Let count be count of int map address of table.
Say count.
Let removed be intmap_remove(address of table, {i % 20}).
Say removed.
Free int map address of table.
Let used be bytes in use.
If used is not 0.
Stop with error "intmap leak".
End the condition.
Give back 0.
'''
            source = "Use alloc, core, io, map.\n\n" + body
        elif variant == 1:
            body = f'''A record Point has an x (a whole number) and a y (a whole number).
Let table be IntMap of Point.
Let p be Point.
Set p's x to {i}.
Set p's y to {i + 1}.
Put p into int map address of table for key 1.
Let found be look up 1 in int map address of table.
If found.has_value, say 1, otherwise say 0.
Say found.value.x.
Say found.value.y.
Free int map address of table.
Give back 0.
'''
            source = "Use alloc, core, io, map.\n\n" + body
        elif variant == 2:
            body = f'''Let table be IntMap of decimal.
Put 1.5 into int map address of table for key 1.
Put 2.25 into int map address of table for key 2.
Let found be look up 2 in int map address of table.
If found.has_value, say 1, otherwise say 0.
Let result be 0.0.
Set result to found.value.
Say result.
Free int map address of table.
Give back 0.
'''
            source = "Use alloc, core, io, map.\n\n" + body
        elif variant == 3:
            body = f'''Let table be StrMap of whole number.
Let alpha be create text from "alpha{i}" of length {len(f"alpha{i}")}.
Let beta be create text from "beta{i}" of length {len(f"beta{i}")}.
Put {i} into text map address of table for text address of alpha.
Put {i + 1} into text map address of table for text address of beta.
Let found be look up text address of alpha in text map address of table.
If found.has_value, say 1, otherwise say 0.
Say found.value.
Let removed be strmap_remove(address of table, address of beta).
Say removed.
Free text map address of table.
Release text address of alpha.
Release text address of beta.
Let used be bytes in use.
If used is not 0.
Stop with error "strmap leak".
End the condition.
Give back 0.
'''
            source = "Use alloc, core, io, map, string.\n\n" + body
        else:
            body = f'''Let table be IntMap of whole number.
Let k be 0.
While k is less than 120.
Put k into int map address of table for key k.
Add 1 to k.
End the loop.
Let k be 0.
While k is less than 60.
Let removed be intmap_remove(address of table, k times 2).
Add 1 to k.
End the loop.
Let k be 120.
While k is less than 180.
Put k into int map address of table for key k.
Add 1 to k.
End the loop.
Let count be count of int map address of table.
Say count.
Free int map address of table.
Let used be bytes in use.
If used is not 0.
Stop with error "map reinsert leak".
End the condition.
Give back 0.
'''
            source = "Use alloc, core, io, map.\n\n" + body
        write("s7", f"s7_{i:03d}.eng", source)

def s8():
    reset("s8")
    archetypes = 10
    for i in range(1, 66):
        kind = (i - 1) % archetypes
        app = CORP / "tier_s8" / f"app_{i:03d}"
        app.mkdir(parents=True, exist_ok=True)
        q = i * 3 + 2
        r = i % 17 + 2
        if kind == 0:
            (app/"pricing.eng").write_text(f'''Publicly, to price given quantity as a whole number, rate as a whole number, named price_total, giving a whole number.
Give back quantity times rate.

Publicly, to tax given amount as a whole number, named tax_total, giving a whole number.
Give back amount times 5 divided by 100.
''')
            (app/"main.eng").write_text(f'''Use io and pricing.
Let total be price_total({q}, {r}).
Let tax be tax_total(total).
Say total plus tax.
Give back 0.
''')
        elif kind == 1:
            (app/"report.eng").write_text(f'''Publicly, to label given value as a whole number, named label_value, giving a whole number.
Give back value plus {i}.
''')
            (app/"main.eng").write_text(f'''Use io and report.
Let label be label_value({q}).
Say label.
Give back 0.
''')
        elif kind == 2:
            (app/"inventory.eng").write_text('''Publicly, to stock given count as a whole number, sold as a whole number, named stock_left, giving a whole number.
Give back count minus sold.
''')
            (app/"main.eng").write_text(f'''Use io and inventory.
Let left be stock_left({q + 20}, {r}).
Say left.
Give back 0.
''')
        elif kind == 3:
            (app/"model.eng").write_text('''A public record Point has an x (a whole number) and a y (a whole number).
''')
            (app/"geometry.eng").write_text('''Use model.
Publicly, to distance squared given p as a pointer to Point, named distance_squared, giving a whole number.
Give back p's x times p's x plus p's y times p's y.
''')
            (app/"main.eng").write_text(f'''Use geometry, io, and model.
Let p be Point.
Set p's x to {i % 9 + 1}.
Set p's y to {i % 7 + 1}.
Let distance be distance_squared(address of p).
Say distance.
Give back 0.
''')
        elif kind == 4:
            (app/"grades.eng").write_text('''Publicly, to average given total as a whole number, count as a whole number, named average, giving a whole number.
Give back total divided by count.
''')
            (app/"main.eng").write_text(f'''Use grades, io.
Let mean be average({q + 30}, {r}).
Say mean.
Give back 0.
''')
        elif kind == 5:
            (app/"budget.eng").write_text('''Publicly, to add interest given principal as a whole number, rate as a whole number, named add_interest, giving a whole number.
Give back principal plus principal times rate divided by 100.
''')
            (app/"main.eng").write_text(f'''Use budget, io.
Let amount be add_interest({q * 10}, {r}).
Say amount.
Give back 0.
''')
        elif kind == 6:
            (app/"primes.eng").write_text('''Publicly, to prime count given limit as a whole number, named prime_count, giving a whole number.
Let count be 0.
Let n be 2.
While n is at most limit.
Let prime be 1.
Let d be 2.
While d times d is at most n.
If n modulo d equals 0.
Set prime to 0.
Break.
End the condition.
Add 1 to d.
End the loop.
If prime equals 1, add 1 to count.
Add 1 to n.
End the loop.
Give back count.
''')
            (app/"main.eng").write_text(f'''Use io and primes.
Let count be prime_count({i % 20 + 5}).
Say count.
Give back 0.
''')
        elif kind == 7:
            (app/"texts.eng").write_text('''Use alloc, core, and string.
Publicly, to tag given s as a pointer to TcString, named tag_text, giving TcString.
Let prefix be create text from "#" of length 1.
Let joined be join address of prefix and s.
Release text address of prefix.
Give back joined.
''')
            (app/"main.eng").write_text(f'''Use alloc, core, io, string, and texts.
Let name be create text from "item{i}" of length {len(f"item{i}")}.
Let tagged be tag_text(address of name).
Say address of tagged.
Release text address of tagged.
Release text address of name.
Give back 0.
''')
        elif kind == 8:
            (app/"maphelp.eng").write_text('''Publicly, to doubled key given key as a whole number, named doubled_key, giving a whole number.
Give back key times 2.
''')
            (app/"main.eng").write_text(f'''Use io, map, and maphelp.
Let table be IntMap of whole number.
Let k be 0.
While k is less than 8.
Put doubled_key(k) into int map address of table for key k.
Add 1 to k.
End the loop.
Let found be look up {i % 8} in int map address of table.
If found.has_value, say found.value, otherwise say -1.
Let count be count of int map address of table.
Say count.
Free int map address of table.
Give back 0.
''')
        else:
            (app/"arena.eng").write_text('''Use alloc, core.
Publicly, to arena block given size as a whole number, named arena_block, giving a pointer to byte.
Give back allocate size as an unsigned number bytes.
''')
            (app/"main.eng").write_text(f'''Use alloc, arena, core, io, and list.
Let p be arena_block({i % 64 + 1}).
Set p[0] to 1.
Release memory at p.
Let nums be List of whole number.
Push {i} to address of nums.
Let count be count of address of nums.
Say count.
Free list address of nums.
Let used be bytes in use.
If used is not 0.
Stop with error "arena leak".
End the condition.
Give back 0.
''')

def s9():
    reset("s9")
    for i in range(1, 76):
        variant = i % 5
        if variant == 0:
            source = f'''Use io.
Yesterday I bought {i % 9 + 1} teas.
Let monthly budget be {i * 10}.
Let amount left be monthly budget minus {i * 3}.
Say amount left.
Give back 0.
'''
        elif variant == 1:
            source = f'''Use io and math.
The room is {i % 5 + 2} metres wide.
Let width be {i % 5 + 2}.0.
Let height be {i % 4 + 2}.0.
Let diagonal be square root of width times width plus height times height.
Say diagonal.
Give back 0.
'''
        elif variant == 2:
            source = f'''Use io.
First, we add the bill.
After that, we check the total.
Let total bill be {i * 7}.
Add {i % 13} to total bill.
Say total bill.
Give back 0.
'''
        elif variant == 3:
            source = f'''Use io.
It was raining outside.
It is 3 pm.
My brother drank {i % 4 + 1} coffees in {i % 3 + 1} days.
My sister bought fresh milk yesterday.
Give back 0.
'''
        else:
            source = f'''Use io.
Let total cost be {i * 12}.
Let people be {i % 6 + 2}.
Let share be total cost divided by people.
Say share.
The report is complete.
Give back 0.
'''
        write("s9", f"s9_{i:03d}.eng", source)
