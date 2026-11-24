#![allow(clippy::needless_raw_string_hashes)]

//! Massive human-English battle-test corpus.
//!
//! The discovery and held-out programs are executed through the IR
//! interpreter and a standalone static zero-libc ELF64 binary; algorithmic
//! programs are also checked for genuine runtime Core-7 loops/branches.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use tc_editor::runner::{RunResult, run_elf_with_parity};
use tc_ir::Stmt;
use tc_semantics::{SourceFile as SemanticSourceFile, compile_english_to_ir};
use tc_syntax::{SourceFile, segment_source, split_sentences, tokenize};

enum Expect {
    Output(&'static [&'static str]),
    Failure(&'static str),
}

struct Case {
    band: u8,
    held_out: bool,
    name: &'static str,
    source: &'static str,
    expect: Expect,
}

static CASES: &[Case] = &[
    Case {
        band: 1,
        held_out: false,
        name: "b1-contraction-even",
        source: r#"Let x be 7.
If x isn't even, add 1 to it.
Print x.
"#,
        expect: Expect::Output(&["8"]),
    },
    Case {
        band: 1,
        held_out: false,
        name: "b1-contraction-startswith",
        source: r#"Let word be "banana".
If word doesn't start with "b", print "nope".
Otherwise, print "ok".
"#,
        expect: Expect::Output(&["ok"]),
    },
    Case {
        band: 1,
        held_out: false,
        name: "b1-contraction-bigger",
        source: r#"Let x be 5.
If x isn't bigger than 3, print "small".
Otherwise, print "big".
"#,
        expect: Expect::Output(&["big"]),
    },
    Case {
        band: 1,
        held_out: false,
        name: "b1-contraction-its",
        source: r#"Let x be 4.
If it's bigger than 3, add 2 to x.
Print x.
"#,
        expect: Expect::Output(&["6"]),
    },
    Case {
        band: 1,
        held_out: false,
        name: "b1-contraction-endswith",
        source: r#"Let name be "abc".
If name doesn't end with "z", print name.
"#,
        expect: Expect::Output(&["abc"]),
    },
    Case {
        band: 1,
        held_out: false,
        name: "b1-contraction-notequal",
        source: r#"Let x be 12.
If x doesn't equal 10, print "different".
"#,
        expect: Expect::Output(&["different"]),
    },
    Case {
        band: 1,
        held_out: false,
        name: "b1-contraction-isnt10",
        source: r#"Let x be 12.
If x isn't 10, print "not ten".
"#,
        expect: Expect::Output(&["not ten"]),
    },
    Case {
        band: 1,
        held_out: false,
        name: "b1-contraction-contains",
        source: r#"Let word be "hello".
If word doesn't contain "z", print word.
"#,
        expect: Expect::Output(&["hello"]),
    },
    Case {
        band: 1,
        held_out: false,
        name: "b1-start-by-set",
        source: r#"Start by setting x to 10.
Go ahead and print x.
"#,
        expect: Expect::Output(&["10"]),
    },
    Case {
        band: 1,
        held_out: false,
        name: "b1-start-by-put-double",
        source: r#"Start by putting 3 in x.
Double x.
Go ahead and print x.
"#,
        expect: Expect::Output(&["6"]),
    },
    Case {
        band: 1,
        held_out: false,
        name: "b1-at-the-end",
        source: r#"Let total be 0.
For every n from 1 up to 4, add n to total.
At the end, show total.
"#,
        expect: Expect::Output(&["10"]),
    },
    Case {
        band: 1,
        held_out: false,
        name: "b1-when-thats-done",
        source: r#"Let count be 0.
For every n from 1 up to 3, add 1 to count.
When that's done, print count.
"#,
        expect: Expect::Output(&["3"]),
    },
    Case {
        band: 1,
        held_out: false,
        name: "b1-once-everything",
        source: r#"Let result be 5.
Once everything is finished, tell me the result.
"#,
        expect: Expect::Output(&["5"]),
    },
    Case {
        band: 1,
        held_out: false,
        name: "b1-else-branch",
        source: r#"Let x be 12.
If x doesn't equal 10, print "different".
Otherwise, print "same".
"#,
        expect: Expect::Output(&["different"]),
    },
    Case {
        band: 1,
        held_out: false,
        name: "b1-contains-else",
        source: r#"Let word be "hello".
If word doesn't contain "z", print "found".
Otherwise, print "missing".
"#,
        expect: Expect::Output(&["found"]),
    },
    Case {
        band: 2,
        held_out: false,
        name: "b2-sample-a1",
        source: r#"Take every number from 1 to 20. Skip any multiple of 4, unless it's also divisible by 6. For each remaining number, if it's odd, triple it and add 1. Otherwise, cut it in half. Print the final value only if it's over 15, and keep track of the largest value you printed. Once you're done, print that largest value.
"#,
        expect: Expect::Output(&["16", "22", "28", "34", "40", "46", "52", "58", "58"]),
    },
    Case {
        band: 2,
        held_out: false,
        name: "b2-sample-a2",
        source: r#"Take every number from 2 to 25. Skip any multiple of 5, unless it's also divisible by 10. For each remaining number, if it's odd, triple it and add 1. Otherwise, cut it in half. Print the final value only if it's over 12, and keep track of the largest value you printed. Once you're done, print that largest value.
"#,
        expect: Expect::Output(&["22", "28", "34", "40", "52", "58", "64", "70", "70"]),
    },
    Case {
        band: 2,
        held_out: false,
        name: "b2-sample-a3",
        source: r#"Take every number from 5 to 30. Skip any multiple of 3, unless it's also divisible by 9. For each remaining number, if it's odd, triple it and add 1. Otherwise, cut it in half. Print the final value only if it's over 20, and keep track of the largest value you printed. Once you're done, print that largest value.
"#,
        expect: Expect::Output(&[
            "22", "28", "34", "40", "52", "58", "70", "76", "82", "88", "88",
        ]),
    },
    Case {
        band: 2,
        held_out: false,
        name: "b2-sample-a4",
        source: r#"Take every number from 10 to 35. Skip any multiple of 4, unless it's also divisible by 8. For each remaining number, if it's odd, triple it and add 1. Otherwise, cut it in half. Print the final value only if it's over 25, and keep track of the largest value you printed. Once you're done, print that largest value.
"#,
        expect: Expect::Output(&[
            "34", "40", "46", "52", "58", "64", "70", "76", "82", "88", "94", "100", "106", "106",
        ]),
    },
    Case {
        band: 2,
        held_out: false,
        name: "b2-sample-a5",
        source: r#"Take every number from 3 to 18. Skip any multiple of 6, unless it's also divisible by 4. For each remaining number, if it's odd, triple it and add 1. Otherwise, cut it in half. Print the final value only if it's over 8, and keep track of the largest value you printed. Once you're done, print that largest value.
"#,
        expect: Expect::Output(&["10", "16", "22", "28", "34", "40", "46", "52", "52"]),
    },
    Case {
        band: 2,
        held_out: false,
        name: "b2-sample-b1",
        source: r#"Take every number from 1 to 15. Skip any multiple of 3. Print that number.
"#,
        expect: Expect::Output(&["1", "2", "4", "5", "7", "8", "10", "11", "13", "14"]),
    },
    Case {
        band: 2,
        held_out: false,
        name: "b2-sample-b2",
        source: r#"Take every number from 2 to 20. Skip any multiple of 4. Print that number.
"#,
        expect: Expect::Output(&[
            "2", "3", "5", "6", "7", "9", "10", "11", "13", "14", "15", "17", "18", "19",
        ]),
    },
    Case {
        band: 2,
        held_out: false,
        name: "b2-sample-b3",
        source: r#"Take every number from 5 to 25. Skip any multiple of 5. Print that number.
"#,
        expect: Expect::Output(&[
            "6", "7", "8", "9", "11", "12", "13", "14", "16", "17", "18", "19", "21", "22", "23",
            "24",
        ]),
    },
    Case {
        band: 2,
        held_out: false,
        name: "b2-sample-b4",
        source: r#"Take every number from 10 to 30. Skip any multiple of 7. Print that number.
"#,
        expect: Expect::Output(&[
            "10", "11", "12", "13", "15", "16", "17", "18", "19", "20", "22", "23", "24", "25",
            "26", "27", "29", "30",
        ]),
    },
    Case {
        band: 2,
        held_out: false,
        name: "b2-sample-b5",
        source: r#"Take every number from 1 to 12. Skip any multiple of 2. Print that number.
"#,
        expect: Expect::Output(&["1", "3", "5", "7", "9", "11"]),
    },
    Case {
        band: 2,
        held_out: false,
        name: "b2-sample-c1",
        source: r#"Look at every number from 1 up to 10. If it's even, cut it in half. Otherwise, bump it up by 3. Print the final value.
"#,
        expect: Expect::Output(&["4", "1", "6", "2", "8", "3", "10", "4", "12", "5"]),
    },
    Case {
        band: 2,
        held_out: false,
        name: "b2-sample-c2",
        source: r#"Look at every number from 2 up to 12. If it's even, cut it in half. Otherwise, bump it up by 3. Print the final value.
"#,
        expect: Expect::Output(&["1", "6", "2", "8", "3", "10", "4", "12", "5", "14", "6"]),
    },
    Case {
        band: 2,
        held_out: false,
        name: "b2-sample-c3",
        source: r#"Look at every number from 5 up to 15. If it's even, cut it in half. Otherwise, bump it up by 3. Print the final value.
"#,
        expect: Expect::Output(&["8", "3", "10", "4", "12", "5", "14", "6", "16", "7", "18"]),
    },
    Case {
        band: 2,
        held_out: false,
        name: "b2-sample-c4",
        source: r#"Look at every number from 3 up to 9. If it's even, cut it in half. Otherwise, bump it up by 3. Print the final value.
"#,
        expect: Expect::Output(&["6", "2", "8", "3", "10", "4", "12"]),
    },
    Case {
        band: 2,
        held_out: false,
        name: "b2-sample-c5",
        source: r#"Look at every number from 10 up to 20. If it's even, cut it in half. Otherwise, bump it up by 3. Print the final value.
"#,
        expect: Expect::Output(&["5", "14", "6", "16", "7", "18", "8", "20", "9", "22", "10"]),
    },
    Case {
        band: 3,
        held_out: false,
        name: "b3-seq1",
        source: r#"Start with 1 and 2. Keep generating the next number by adding the previous two numbers together, as long as the new number is at most 150. For each number in this entire sequence, including the starting two, add it to a running total. Whenever that total goes over 150, subtract 150 from it and count an overflow. At the end, print the final running total and how many overflows happened.
"#,
        expect: Expect::Output(&["75", "2"]),
    },
    Case {
        band: 3,
        held_out: false,
        name: "b3-seq2",
        source: r#"Begin with 2 and 3. Keep generating the next number by adding the previous two numbers together, as long as the new number doesn't exceed 120. For each number in this entire sequence, including the starting two, add it to a running total. Whenever that total goes over 150, subtract 150 from it and count an overflow. At the end, print the final running total and how many overflows happened.
"#,
        expect: Expect::Output(&["80", "1"]),
    },
    Case {
        band: 3,
        held_out: false,
        name: "b3-seq3",
        source: r#"Start with 1 and 4. Keep generating the next number by adding the previous two numbers together, as long as the new number doesn't exceed 100. For each number in this entire sequence, including the starting two, add it to a running total. Whenever that total goes over 150, subtract 150 from it and count an overflow. At the end, print the final running total and how many overflows happened.
"#,
        expect: Expect::Output(&["100", "1"]),
    },
    Case {
        band: 3,
        held_out: false,
        name: "b3-seq4",
        source: r#"Begin with 2 and 5. Keep generating the next number by adding the previous two numbers together, as long as the new number is at most 160. For each number in this entire sequence, including the starting two, add it to a running total. Whenever that total goes over 170, subtract 170 from it and count an overflow. At the end, print the final running total and how many overflows happened.
"#,
        expect: Expect::Output(&["168", "1"]),
    },
    Case {
        band: 3,
        held_out: false,
        name: "b3-seq5",
        source: r#"Start with 3 and 4. Keep generating the next number by adding the previous two numbers together, as long as the new number doesn't exceed 140. For each number in this entire sequence, including the starting two, add it to a running total. Whenever that total goes over 160, subtract 160 from it and count an overflow. At the end, print the final running total and how many overflows happened.
"#,
        expect: Expect::Output(&["158", "1"]),
    },
    Case {
        band: 3,
        held_out: false,
        name: "b3-seq6",
        source: r#"Begin with 1 and 3. Keep generating the next number by adding the previous two numbers together, as long as the new number doesn't exceed 200. For each number in this entire sequence, including the starting two, add it to a running total. Whenever that total goes over 180, subtract 180 from it and count an overflow. At the end, print the final running total and how many overflows happened.
"#,
        expect: Expect::Output(&["158", "2"]),
    },
    Case {
        band: 3,
        held_out: false,
        name: "b3-seq7",
        source: r#"Start with 2 and 4. Keep generating the next number by adding the previous two numbers together, as long as the new number is at most 90. For each number in this entire sequence, including the starting two, add it to a running total. Whenever that total goes over 150, subtract 150 from it and count an overflow. At the end, print the final running total and how many overflows happened.
"#,
        expect: Expect::Output(&["24", "1"]),
    },
    Case {
        band: 3,
        held_out: false,
        name: "b3-seq8",
        source: r#"Begin with 5 and 7. Keep generating the next number by adding the previous two numbers together, as long as the new number doesn't exceed 180. For each number in this entire sequence, including the starting two, add it to a running total. Whenever that total goes over 190, subtract 190 from it and count an overflow. At the end, print the final running total and how many overflows happened.
"#,
        expect: Expect::Output(&["146", "1"]),
    },
    Case {
        band: 3,
        held_out: false,
        name: "b3-seq9",
        source: r#"Start with 1 and 5. Keep generating the next number by adding the previous two numbers together, as long as the new number doesn't exceed 120. For each number in this entire sequence, including the starting two, add it to a running total. Whenever that total goes over 160, subtract 160 from it and count an overflow. At the end, print the final running total and how many overflows happened.
"#,
        expect: Expect::Output(&["144", "1"]),
    },
    Case {
        band: 3,
        held_out: false,
        name: "b3-seq10",
        source: r#"Begin with 3 and 5. Keep generating the next number by adding the previous two numbers together, as long as the new number is at most 100. For each number in this entire sequence, including the starting two, add it to a running total. Whenever that total goes over 150, subtract 150 from it and count an overflow. At the end, print the final running total and how many overflows happened.
"#,
        expect: Expect::Output(&["78", "1"]),
    },
    Case {
        band: 3,
        held_out: false,
        name: "b3-seq11",
        source: r#"Start with 4 and 6. Keep generating the next number by adding the previous two numbers together, as long as the new number doesn't exceed 150. For each number in this entire sequence, including the starting two, add it to a running total. Whenever that total goes over 170, subtract 170 from it and count an overflow. At the end, print the final running total and how many overflows happened.
"#,
        expect: Expect::Output(&["112", "1"]),
    },
    Case {
        band: 3,
        held_out: false,
        name: "b3-seq12",
        source: r#"Begin with 2 and 6. Keep generating the next number by adding the previous two numbers together, as long as the new number doesn't exceed 80. For each number in this entire sequence, including the starting two, add it to a running total. Whenever that total goes over 150, subtract 150 from it and count an overflow. At the end, print the final running total and how many overflows happened.
"#,
        expect: Expect::Output(&["146", "0"]),
    },
    Case {
        band: 3,
        held_out: false,
        name: "b3-seq13",
        source: r#"Start with 1 and 6. Keep generating the next number by adding the previous two numbers together, as long as the new number is at most 160. For each number in this entire sequence, including the starting two, add it to a running total. Whenever that total goes over 180, subtract 180 from it and count an overflow. At the end, print the final running total and how many overflows happened.
"#,
        expect: Expect::Output(&["178", "1"]),
    },
    Case {
        band: 3,
        held_out: false,
        name: "b3-seq14",
        source: r#"Begin with 7 and 8. Keep generating the next number by adding the previous two numbers together, as long as the new number doesn't exceed 60. For each number in this entire sequence, including the starting two, add it to a running total. Whenever that total goes over 150, subtract 150 from it and count an overflow. At the end, print the final running total and how many overflows happened.
"#,
        expect: Expect::Output(&["91", "0"]),
    },
    Case {
        band: 3,
        held_out: false,
        name: "b3-seq15",
        source: r#"Start with 2 and 7. Keep generating the next number by adding the previous two numbers together, as long as the new number doesn't exceed 110. For each number in this entire sequence, including the starting two, add it to a running total. Whenever that total goes over 160, subtract 160 from it and count an overflow. At the end, print the final running total and how many overflows happened.
"#,
        expect: Expect::Output(&["113", "1"]),
    },
    Case {
        band: 4,
        held_out: false,
        name: "b4-box1",
        source: r#"Imagine a red box starting with 5 and a blue box starting with 16. Walk through every number from 1 up to 40. If the number is a multiple of 3, move 4 from the blue box into the red box; otherwise move 1 from the red box into the blue box. However, if a move would ever cause either one to drop below 5, skip it. Stop immediately as soon as the red box holds more than twice as much as the blue box. Finally, print the number where you stopped, followed by the amounts in the red box and the blue box.
"#,
        expect: Expect::Output(&["12", "15", "6"]),
    },
    Case {
        band: 4,
        held_out: false,
        name: "b4-box2",
        source: r#"Imagine a green box starting with 10 and a orange box starting with 12. Walk through every number from 1 up to 30. If the number is a multiple of 3, move 4 from the orange box into the green box; otherwise move 1 from the green box into the orange box. However, if a move would ever cause either one to drop below 4, skip it. Stop immediately as soon as the green box holds more than twice as much as the orange box. Finally, print the number where you stopped, followed by the amounts in the green box and the orange box.
"#,
        expect: Expect::Output(&["9", "16", "6"]),
    },
    Case {
        band: 4,
        held_out: false,
        name: "b4-box3",
        source: r#"Imagine a savings jar starting with 8 and a wallet starting with 14. Walk through every number from 1 up to 30. If the number is a multiple of 3, move 3 from the wallet into the savings jar; otherwise move 1 from the savings jar into the wallet. However, if a move would ever cause either one to drop below 4, skip it. Stop immediately as soon as the savings jar holds more than twice as much as the wallet. Finally, print the number where you stopped, followed by the amounts in the savings jar and the wallet.
"#,
        expect: Expect::Output(&["21", "15", "7"]),
    },
    Case {
        band: 4,
        held_out: false,
        name: "b4-box4",
        source: r#"Imagine a tank a starting with 6 and a tank b starting with 16. Walk through every number from 1 up to 30. If the number is a multiple of 3, move 4 from the tank b into the tank a; otherwise move 1 from the tank a into the tank b. However, if a move would ever cause either one to drop below 5, skip it. Stop immediately as soon as the tank a holds more than twice as much as the tank b. Finally, print the number where you stopped, followed by the amounts in the tank a and the tank b.
"#,
        expect: Expect::Output(&["12", "15", "7"]),
    },
    Case {
        band: 4,
        held_out: false,
        name: "b4-box5",
        source: r#"Imagine a left bucket starting with 7 and a right bucket starting with 15. Walk through every number from 1 up to 30. If the number is a multiple of 4, transfer 5 from the right bucket to the left bucket; otherwise transfer 1 from the left bucket to the right bucket. However, if a move would ever cause either one to drop below 5, skip it. Stop immediately as soon as the left bucket holds more than twice as much as the right bucket. Finally, print the number where you stopped, followed by the amounts in the left bucket and the right bucket.
"#,
        expect: Expect::Output(&["16", "16", "6"]),
    },
    Case {
        band: 4,
        held_out: false,
        name: "b4-box6",
        source: r#"Imagine a warehouse a starting with 5 and a warehouse b starting with 16. Walk through every number from 1 up to 40. If the number is a multiple of 3, move 4 from the warehouse b into the warehouse a; otherwise move 1 from the warehouse a into the warehouse b. However, if a move would ever cause either one to drop below 5, skip it. Stop immediately as soon as the warehouse a holds more than twice as much as the warehouse b. Finally, print the number where you stopped, followed by the amounts in the warehouse a and the warehouse b.
"#,
        expect: Expect::Output(&["12", "15", "6"]),
    },
    Case {
        band: 4,
        held_out: false,
        name: "b4-box7",
        source: r#"Imagine a cookie jar starting with 10 and a bread box starting with 12. Walk through every number from 1 up to 30. If the number is a multiple of 3, move 4 from the bread box into the cookie jar; otherwise move 1 from the cookie jar into the bread box. However, if a move would ever cause either one to drop below 4, skip it. Stop immediately as soon as the cookie jar holds more than twice as much as the bread box. Finally, print the number where you stopped, followed by the amounts in the cookie jar and the bread box.
"#,
        expect: Expect::Output(&["9", "16", "6"]),
    },
    Case {
        band: 4,
        held_out: false,
        name: "b4-box8",
        source: r#"Imagine a north tank starting with 8 and a south tank starting with 14. Walk through every number from 1 up to 30. If the number is a multiple of 3, move 3 from the south tank into the north tank; otherwise move 1 from the north tank into the south tank. However, if a move would ever cause either one to drop below 4, skip it. Stop immediately as soon as the north tank holds more than twice as much as the south tank. Finally, print the number where you stopped, followed by the amounts in the north tank and the south tank.
"#,
        expect: Expect::Output(&["21", "15", "7"]),
    },
    Case {
        band: 4,
        held_out: false,
        name: "b4-box9",
        source: r#"Imagine a blue bin starting with 6 and a green bin starting with 16. Walk through every number from 1 up to 30. If the number is a multiple of 3, move 4 from the green bin into the blue bin; otherwise move 1 from the blue bin into the green bin. However, if a move would ever cause either one to drop below 5, skip it. Stop immediately as soon as the blue bin holds more than twice as much as the green bin. Finally, print the number where you stopped, followed by the amounts in the blue bin and the green bin.
"#,
        expect: Expect::Output(&["12", "15", "7"]),
    },
    Case {
        band: 4,
        held_out: false,
        name: "b4-box10",
        source: r#"Imagine a main vault starting with 7 and a petty cash starting with 15. Walk through every number from 1 up to 30. If the number is a multiple of 4, transfer 5 from the petty cash to the main vault; otherwise transfer 1 from the main vault to the petty cash. However, if a move would ever cause either one to drop below 5, skip it. Stop immediately as soon as the main vault holds more than twice as much as the petty cash. Finally, print the number where you stopped, followed by the amounts in the main vault and the petty cash.
"#,
        expect: Expect::Output(&["16", "16", "6"]),
    },
    Case {
        band: 4,
        held_out: false,
        name: "b4-box11",
        source: r#"Imagine a silo one starting with 5 and a silo two starting with 16. Walk through every number from 1 up to 40. If the number is a multiple of 3, move 4 from the silo two into the silo one; otherwise move 1 from the silo one into the silo two. However, if a move would ever cause either one to drop below 5, skip it. Stop immediately as soon as the silo one holds more than twice as much as the silo two. Finally, print the number where you stopped, followed by the amounts in the silo one and the silo two.
"#,
        expect: Expect::Output(&["12", "15", "6"]),
    },
    Case {
        band: 4,
        held_out: false,
        name: "b4-box12",
        source: r#"Imagine a red basket starting with 10 and a blue basket starting with 12. Walk through every number from 1 up to 30. If the number is a multiple of 3, move 4 from the blue basket into the red basket; otherwise move 1 from the red basket into the blue basket. However, if a move would ever cause either one to drop below 4, skip it. Stop immediately as soon as the red basket holds more than twice as much as the blue basket. Finally, print the number where you stopped, followed by the amounts in the red basket and the blue basket.
"#,
        expect: Expect::Output(&["9", "16", "6"]),
    },
    Case {
        band: 4,
        held_out: false,
        name: "b4-box13",
        source: r#"Imagine a coffee can starting with 8 and a tea tin starting with 14. Walk through every number from 1 up to 30. If the number is a multiple of 3, move 3 from the tea tin into the coffee can; otherwise move 1 from the coffee can into the tea tin. However, if a move would ever cause either one to drop below 4, skip it. Stop immediately as soon as the coffee can holds more than twice as much as the tea tin. Finally, print the number where you stopped, followed by the amounts in the coffee can and the tea tin.
"#,
        expect: Expect::Output(&["21", "15", "7"]),
    },
    Case {
        band: 4,
        held_out: false,
        name: "b4-box14",
        source: r#"Imagine a big crate starting with 6 and a small crate starting with 16. Walk through every number from 1 up to 30. If the number is a multiple of 3, move 4 from the small crate into the big crate; otherwise move 1 from the big crate into the small crate. However, if a move would ever cause either one to drop below 5, skip it. Stop immediately as soon as the big crate holds more than twice as much as the small crate. Finally, print the number where you stopped, followed by the amounts in the big crate and the small crate.
"#,
        expect: Expect::Output(&["12", "15", "7"]),
    },
    Case {
        band: 4,
        held_out: false,
        name: "b4-box15",
        source: r#"Imagine a front tank starting with 7 and a back tank starting with 15. Walk through every number from 1 up to 30. If the number is a multiple of 4, transfer 5 from the back tank to the front tank; otherwise transfer 1 from the front tank to the back tank. However, if a move would ever cause either one to drop below 5, skip it. Stop immediately as soon as the front tank holds more than twice as much as the back tank. Finally, print the number where you stopped, followed by the amounts in the front tank and the back tank.
"#,
        expect: Expect::Output(&["16", "16", "6"]),
    },
    Case {
        band: 5,
        held_out: false,
        name: "b5-list1",
        source: r#"Start with the list of numbers 14, 7, 22, 9, 15, 4, and 18. Go through the list from left to right, comparing each number with the one immediately after it. Whenever the left number is larger than the right one, swap their positions and count a swap. After finishing that pass, remove any number from the list that is divisible by 3. Finally, print the remaining numbers in order, followed by the total number of swaps made.
"#,
        expect: Expect::Output(&["7", "14", "4", "22", "5"]),
    },
    Case {
        band: 5,
        held_out: false,
        name: "b5-list2",
        source: r#"Start with the list of numbers 20, 5, 10, 8, 27, 3, and 11. Go through the list from left to right, comparing each number with the one immediately after it. Whenever the left number is larger than the right one, swap their positions and count a swap. After finishing that pass, remove any number from the list that is divisible by 4. Finally, print the remaining numbers in order, followed by the total number of swaps made.
"#,
        expect: Expect::Output(&["5", "10", "3", "11", "27", "5"]),
    },
    Case {
        band: 5,
        held_out: false,
        name: "b5-list3",
        source: r#"Start with the list of numbers 9, 3, 7, 12, 5, 18, and 6. Go through the list from left to right, comparing each number with the one immediately after it. Whenever the left number is larger than the right one, swap their positions and count a swap. After finishing that pass, remove any number from the list that is divisible by 3. Finally, print the remaining numbers in order, followed by the total number of swaps made.
"#,
        expect: Expect::Output(&["7", "5", "4"]),
    },
    Case {
        band: 5,
        held_out: false,
        name: "b5-list4",
        source: r#"Start with the list of numbers 25, 4, 16, 8, 3, 10, and 7. Go through the list from left to right, comparing each number with the one immediately after it. Whenever the left number is larger than the right one, swap their positions and count a swap. After finishing that pass, remove any number from the list that is divisible by 2. Finally, print the remaining numbers in order, followed by the total number of swaps made.
"#,
        expect: Expect::Output(&["3", "7", "25", "6"]),
    },
    Case {
        band: 5,
        held_out: false,
        name: "b5-list5",
        source: r#"Start with the list of numbers 11, 6, 14, 9, 20, 5, and 18. Go through the list from left to right, comparing each number with the one immediately after it. Whenever the left number is larger than the right one, swap their positions and count a swap. After finishing that pass, remove any number from the list that is divisible by 5. Finally, print the remaining numbers in order, followed by the total number of swaps made.
"#,
        expect: Expect::Output(&["6", "11", "9", "14", "18", "4"]),
    },
    Case {
        band: 5,
        held_out: false,
        name: "b5-list6",
        source: r#"Start with the numbers 14, 7, 22, 9, 15, 4, and 18. Compare each number with the next one, from left to right. If the left one is bigger than the right one, swap them and count a swap. After that pass, drop all numbers that are divisible by 3. At the end, print the remaining numbers, followed by how many swaps were made.
"#,
        expect: Expect::Output(&["7", "14", "4", "22", "5"]),
    },
    Case {
        band: 5,
        held_out: false,
        name: "b5-list7",
        source: r#"Start with the numbers 20, 5, 10, 8, 27, 3, and 11. Compare each number with the next one, from left to right. If the left one is bigger than the right one, swap them and count a swap. After that pass, drop all numbers that are divisible by 4. At the end, print the remaining numbers, followed by how many swaps were made.
"#,
        expect: Expect::Output(&["5", "10", "3", "11", "27", "5"]),
    },
    Case {
        band: 5,
        held_out: false,
        name: "b5-list8",
        source: r#"Start with the numbers 9, 3, 7, 12, 5, 18, and 6. Compare each number with the next one, from left to right. If the left one is bigger than the right one, swap them and count a swap. After that pass, drop all numbers that are divisible by 3. At the end, print the remaining numbers, followed by how many swaps were made.
"#,
        expect: Expect::Output(&["7", "5", "4"]),
    },
    Case {
        band: 5,
        held_out: false,
        name: "b5-list9",
        source: r#"Start with the numbers 25, 4, 16, 8, 3, 10, and 7. Compare each number with the next one, from left to right. If the left one is bigger than the right one, swap them and count a swap. After that pass, drop all numbers that are divisible by 2. At the end, print the remaining numbers, followed by how many swaps were made.
"#,
        expect: Expect::Output(&["3", "7", "25", "6"]),
    },
    Case {
        band: 5,
        held_out: false,
        name: "b5-list10",
        source: r#"Start with the numbers 11, 6, 14, 9, 20, 5, and 18. Compare each number with the next one, from left to right. If the left one is bigger than the right one, swap them and count a swap. After that pass, drop all numbers that are divisible by 5. At the end, print the remaining numbers, followed by how many swaps were made.
"#,
        expect: Expect::Output(&["6", "11", "9", "14", "18", "4"]),
    },
    Case {
        band: 5,
        held_out: false,
        name: "b5-list11",
        source: r#"Start with the list of numbers 14, 7, 22, 9, 15, 4, and 18. Go through the list from left to right, comparing each number with the one immediately after it. If the left number is larger than the right one, swap the two and count a swap. After finishing that pass, remove any number from the list that is divisible by 3. Finally, print the remaining numbers in order, followed by the total number of swaps made.
"#,
        expect: Expect::Output(&["7", "14", "4", "22", "5"]),
    },
    Case {
        band: 5,
        held_out: false,
        name: "b5-list12",
        source: r#"Start with the list of numbers 20, 5, 10, 8, 27, 3, and 11. Go through the list from left to right, comparing each number with the one immediately after it. If the left number is larger than the right one, swap the two and count a swap. After finishing that pass, remove any number from the list that is divisible by 4. Finally, print the remaining numbers in order, followed by the total number of swaps made.
"#,
        expect: Expect::Output(&["5", "10", "3", "11", "27", "5"]),
    },
    Case {
        band: 5,
        held_out: false,
        name: "b5-list13",
        source: r#"Start with the list of numbers 9, 3, 7, 12, 5, 18, and 6. Go through the list from left to right, comparing each number with the one immediately after it. If the left number is larger than the right one, swap the two and count a swap. After finishing that pass, remove any number from the list that is divisible by 3. Finally, print the remaining numbers in order, followed by the total number of swaps made.
"#,
        expect: Expect::Output(&["7", "5", "4"]),
    },
    Case {
        band: 5,
        held_out: false,
        name: "b5-list14",
        source: r#"Start with the list of numbers 25, 4, 16, 8, 3, 10, and 7. Go through the list from left to right, comparing each number with the one immediately after it. If the left number is larger than the right one, swap the two and count a swap. After finishing that pass, remove any number from the list that is divisible by 2. Finally, print the remaining numbers in order, followed by the total number of swaps made.
"#,
        expect: Expect::Output(&["3", "7", "25", "6"]),
    },
    Case {
        band: 5,
        held_out: false,
        name: "b5-list15",
        source: r#"Start with the list of numbers 11, 6, 14, 9, 20, 5, and 18. Go through the list from left to right, comparing each number with the one immediately after it. If the left number is larger than the right one, swap the two and count a swap. After finishing that pass, remove any number from the list that is divisible by 5. Finally, print the remaining numbers in order, followed by the total number of swaps made.
"#,
        expect: Expect::Output(&["6", "11", "9", "14", "18", "4"]),
    },
    Case {
        band: 6,
        held_out: false,
        name: "b6-bigger",
        source: r#"Let x be 7.
If x is bigger than 5, print "yes".
"#,
        expect: Expect::Output(&["yes"]),
    },
    Case {
        band: 6,
        held_out: false,
        name: "b6-smaller",
        source: r#"Let x be 3.
If x is smaller than 5, print "yes".
"#,
        expect: Expect::Output(&["yes"]),
    },
    Case {
        band: 6,
        held_out: false,
        name: "b6-larger",
        source: r#"Let x be 9.
If x is larger than 4, print "yes".
"#,
        expect: Expect::Output(&["yes"]),
    },
    Case {
        band: 6,
        held_out: false,
        name: "b6-higher",
        source: r#"Let x be 10.
If x is higher than 7, print "yes".
"#,
        expect: Expect::Output(&["yes"]),
    },
    Case {
        band: 6,
        held_out: false,
        name: "b6-lower",
        source: r#"Let x be 2.
If x is lower than 5, print "yes".
"#,
        expect: Expect::Output(&["yes"]),
    },
    Case {
        band: 6,
        held_out: false,
        name: "b6-no-bigger",
        source: r#"Let x be 5.
If x is no bigger than 5, print "yes".
"#,
        expect: Expect::Output(&["yes"]),
    },
    Case {
        band: 6,
        held_out: false,
        name: "b6-at-least",
        source: r#"Let x be 5.
If x is at least 5, print "yes".
"#,
        expect: Expect::Output(&["yes"]),
    },
    Case {
        band: 6,
        held_out: false,
        name: "b6-at-most",
        source: r#"Let x be 5.
If x is at most 5, print "yes".
"#,
        expect: Expect::Output(&["yes"]),
    },
    Case {
        band: 6,
        held_out: false,
        name: "b6-more-than-twice",
        source: r#"Let x be 11.
If x is more than twice 5, print "yes".
"#,
        expect: Expect::Output(&["yes"]),
    },
    Case {
        band: 6,
        held_out: false,
        name: "b6-less-than-half",
        source: r#"Let x be 4.
If x is less than half of 10, print "yes".
"#,
        expect: Expect::Output(&["yes"]),
    },
    Case {
        band: 6,
        held_out: false,
        name: "b6-cut-half",
        source: r#"Let x be 9.
Cut x in half.
Print x.
"#,
        expect: Expect::Output(&["4"]),
    },
    Case {
        band: 6,
        held_out: false,
        name: "b6-quarter",
        source: r#"Let x be 20.
Print a quarter of x.
"#,
        expect: Expect::Output(&["5"]),
    },
    Case {
        band: 6,
        held_out: false,
        name: "b6-percent",
        source: r#"Let x be 200.
Print 15 percent of x.
"#,
        expect: Expect::Output(&["30"]),
    },
    Case {
        band: 6,
        held_out: false,
        name: "b6-increase-decrease",
        source: r#"Let x be 10.
Increase x by 5.
Decrease x by 3.
Print x.
"#,
        expect: Expect::Output(&["12"]),
    },
    Case {
        band: 6,
        held_out: false,
        name: "b6-spoken-difference",
        source: r#"Let a be twenty-five.
Let b be one hundred and two.
Print the difference between b and a.
"#,
        expect: Expect::Output(&["77"]),
    },
    Case {
        band: 7,
        held_out: false,
        name: "b7-trim",
        source: r#"Let s be "  hello  ".
Print trim s.
"#,
        expect: Expect::Output(&["hello"]),
    },
    Case {
        band: 7,
        held_out: false,
        name: "b7-starts",
        source: r#"Let s be "hello".
If s starts with "he", print "yes".
"#,
        expect: Expect::Output(&["yes"]),
    },
    Case {
        band: 7,
        held_out: false,
        name: "b7-ends",
        source: r#"Let s be "hello".
If s ends with "lo", print "yes".
"#,
        expect: Expect::Output(&["yes"]),
    },
    Case {
        band: 7,
        held_out: false,
        name: "b7-contains",
        source: r#"Let s be "hello".
If s contains "ell", print "yes".
"#,
        expect: Expect::Output(&["yes"]),
    },
    Case {
        band: 7,
        held_out: false,
        name: "b7-not-contains",
        source: r#"Let s be "hello".
If s doesn't contain "zz", print "yes".
"#,
        expect: Expect::Output(&["yes"]),
    },
    Case {
        band: 7,
        held_out: false,
        name: "b7-length",
        source: r#"Let s be "hello".
Print the length of s.
"#,
        expect: Expect::Output(&["5"]),
    },
    Case {
        band: 7,
        held_out: false,
        name: "b7-sqrt",
        source: r#"Print the square root of 16.
"#,
        expect: Expect::Output(&["4.0000"]),
    },
    Case {
        band: 7,
        held_out: false,
        name: "b7-record",
        source: r#"A record Point has x (a whole number) and y (a whole number).
Create a Point called p.
Update p so its x is 3 and its y is 4.
Print p's x plus p's y.
"#,
        expect: Expect::Output(&["7"]),
    },
    Case {
        band: 7,
        held_out: false,
        name: "b7-record-its",
        source: r#"A record Point has x (a whole number) and y (a whole number).
Create a Point called p.
Update p so its x is 5 and its y is 2.
Print its x plus its y.
"#,
        expect: Expect::Output(&["7"]),
    },
    Case {
        band: 7,
        held_out: false,
        name: "b7-function",
        source: r#"To double given value as a whole number, named twice, returning a whole number.
Give back value plus value.

Print twice 6.
"#,
        expect: Expect::Output(&["12"]),
    },
    Case {
        band: 7,
        held_out: false,
        name: "b7-prime-discourse",
        source: r#"Look at every number from 2 up to 20. For each number, check whether any smaller number starting from 2 divides into it evenly. If none of them do, print that number out and count it. Once you're done, print how many numbers were found.
"#,
        expect: Expect::Output(&["2", "3", "5", "7", "11", "13", "17", "19", "8"]),
    },
    Case {
        band: 7,
        held_out: false,
        name: "b7-even-count",
        source: r#"Look at every number from 1 up to 10. If number is even, print it out and count it. Once you're done, print how many were found.
"#,
        expect: Expect::Output(&["2", "4", "6", "8", "10", "5"]),
    },
    Case {
        band: 7,
        held_out: false,
        name: "b7-list-count",
        source: r#"Let prices be a list of 10, 20, and 30.
Consider each price in prices. If price is greater than 15, print that price out and count it. After that, print how many were found.
"#,
        expect: Expect::Output(&["20", "30", "2"]),
    },
    Case {
        band: 7,
        held_out: false,
        name: "b7-trim-length",
        source: r#"Let name be "Ada".
Print the length of name plus 1.
"#,
        expect: Expect::Output(&["4"]),
    },
    Case {
        band: 7,
        held_out: false,
        name: "b7-multisentence-let",
        source: r#"Let x be 4.
If x is even, double x.
Otherwise, add 1 to x.
Print x.
"#,
        expect: Expect::Output(&["8"]),
    },
    Case {
        band: 8,
        held_out: false,
        name: "b8-narrative-1",
        source: r#"Yesterday she took all the numbers from the desk.
"#,
        expect: Expect::Output(&[]),
    },
    Case {
        band: 8,
        held_out: false,
        name: "b8-narrative-2",
        source: r#"He cut the apple in half after lunch.
"#,
        expect: Expect::Output(&[]),
    },
    Case {
        band: 8,
        held_out: false,
        name: "b8-narrative-3",
        source: r#"She skipped the meeting because it wasn't urgent.
"#,
        expect: Expect::Output(&[]),
    },
    Case {
        band: 8,
        held_out: false,
        name: "b8-narrative-4",
        source: r#"They moved the red box into the garage last night.
"#,
        expect: Expect::Output(&[]),
    },
    Case {
        band: 8,
        held_out: false,
        name: "b8-narrative-5",
        source: r#"He bumped his knee on the table.
"#,
        expect: Expect::Output(&[]),
    },
    Case {
        band: 8,
        held_out: false,
        name: "b8-narrative-contraction-1",
        source: r#"The old fisherman didn't catch anything today.
"#,
        expect: Expect::Output(&[]),
    },
    Case {
        band: 8,
        held_out: false,
        name: "b8-narrative-contraction-2",
        source: r#"Our neighbor's dog isn't very friendly.
"#,
        expect: Expect::Output(&[]),
    },
    Case {
        band: 8,
        held_out: false,
        name: "b8-narrative-contraction-3",
        source: r#"She can't find her keys again.
"#,
        expect: Expect::Output(&[]),
    },
    Case {
        band: 8,
        held_out: false,
        name: "b8-narrative-contraction-4",
        source: r#"That's the last time we try that route.
"#,
        expect: Expect::Output(&[]),
    },
    Case {
        band: 8,
        held_out: false,
        name: "b8-narrative-contraction-5",
        source: r#"We don't know what happened after midnight.
"#,
        expect: Expect::Output(&[]),
    },
    Case {
        band: 8,
        held_out: false,
        name: "b8-typo-print",
        source: r#"Printt 5.
"#,
        expect: Expect::Failure("did you mean"),
    },
    Case {
        band: 8,
        held_out: false,
        name: "b8-typo-show",
        source: r#"Let x be 5.
Please showw x.
"#,
        expect: Expect::Failure("did you mean"),
    },
    Case {
        band: 8,
        held_out: false,
        name: "b8-typo-display",
        source: r#"Let x be 5.
Displayy x.
"#,
        expect: Expect::Failure("did you mean"),
    },
    Case {
        band: 8,
        held_out: false,
        name: "b8-empty-while",
        source: r#"Let x be 5.
Repeat while x is greater than 1:

Print x.
"#,
        expect: Expect::Failure("empty loop body"),
    },
    Case {
        band: 8,
        held_out: false,
        name: "b8-empty-until",
        source: r#"Let x be 5.
Repeat until x is 1:

Print x.
"#,
        expect: Expect::Failure("empty loop body"),
    },
    Case {
        band: 1,
        held_out: true,
        name: "h1-isnt-div",
        source: r#"Let n be 9.
If n isn't divisible by 4, print "ok".
"#,
        expect: Expect::Output(&["ok"]),
    },
    Case {
        band: 1,
        held_out: true,
        name: "h1-doesnt-equal",
        source: r#"Let score be 12.
If score doesn't equal 10, add 5 to score.
Print score.
"#,
        expect: Expect::Output(&["17"]),
    },
    Case {
        band: 1,
        held_out: true,
        name: "h1-doesnt-end",
        source: r#"Let word be "pear".
If word doesn't end with "x", print word.
"#,
        expect: Expect::Output(&["pear"]),
    },
    Case {
        band: 1,
        held_out: true,
        name: "h1-start-by-total",
        source: r#"Start by setting total to 40.
Go ahead and print total.
"#,
        expect: Expect::Output(&["40"]),
    },
    Case {
        band: 2,
        held_out: true,
        name: "h2-range-a",
        source: r#"Take every number from 4 to 24. Skip any multiple of 5, unless it's also divisible by 10. For each remaining number, if it's odd, triple it and add 1. Otherwise, cut it in half. Print the final value only if it's over 18, and keep track of the largest value you printed. Once you're done, print that largest value.
"#,
        expect: Expect::Output(&["22", "28", "34", "40", "52", "58", "64", "70", "70"]),
    },
    Case {
        band: 2,
        held_out: true,
        name: "h2-range-b",
        source: r#"Go through all numbers between 6 and 20. Ignore numbers divisible by 4. Print that number.
"#,
        expect: Expect::Output(&[
            "6", "7", "9", "10", "11", "13", "14", "15", "17", "18", "19",
        ]),
    },
    Case {
        band: 2,
        held_out: true,
        name: "h2-range-c",
        source: r#"Look at every number from 4 up to 14. If it's even, cut it in half. Otherwise, bump it up by 7. Print the final value.
"#,
        expect: Expect::Output(&["2", "12", "3", "14", "4", "16", "5", "18", "6", "20", "7"]),
    },
    Case {
        band: 2,
        held_out: true,
        name: "h2-range-drop",
        source: r#"Take every number from 1 through 12. If it's even, drop it by 2. Otherwise, add 3 to it. Print the final value.
"#,
        expect: Expect::Output(&[
            "4", "0", "6", "2", "8", "4", "10", "6", "12", "8", "14", "10",
        ]),
    },
    Case {
        band: 3,
        held_out: true,
        name: "h3-seq1",
        source: r#"Start with 4 and 5. Keep generating the next number by adding the previous two numbers together, as long as the new number doesn't exceed 170. For each number in this entire sequence, including the starting two, add it to a running total. Whenever that total goes over 180, subtract 180 from it and count an overflow. At the end, print the final running total and how many overflows happened.
"#,
        expect: Expect::Output(&["46", "2"]),
    },
    Case {
        band: 3,
        held_out: true,
        name: "h3-seq2",
        source: r#"Begin with 1 and 7. Keep generating the next number by adding the previous two numbers together, as long as the new number is at most 140. For each number in this entire sequence, including the starting two, add it to a running total. Whenever that total goes over 160, subtract 160 from it and count an overflow. At the end, print the final running total and how many overflows happened.
"#,
        expect: Expect::Output(&["92", "1"]),
    },
    Case {
        band: 3,
        held_out: true,
        name: "h3-seq3",
        source: r#"Start with 3 and 6. Keep generating the next number by adding the previous two numbers together, as long as the new number doesn't exceed 200. For each number in this entire sequence, including the starting two, add it to a running total. Whenever that total goes over 190, subtract 190 from it and count an overflow. At the end, print the final running total and how many overflows happened.
"#,
        expect: Expect::Output(&["46", "2"]),
    },
    Case {
        band: 3,
        held_out: true,
        name: "h3-seq4",
        source: r#"Begin with 2 and 4. Keep generating the next number by adding the previous two numbers together, as long as the new number is at most 110. For each number in this entire sequence, including the starting two, add it to a running total. Whenever that total goes over 150, subtract 150 from it and count an overflow. At the end, print the final running total and how many overflows happened.
"#,
        expect: Expect::Output(&["134", "1"]),
    },
    Case {
        band: 4,
        held_out: true,
        name: "h4-box1",
        source: r#"Imagine a cookie jar starting with 5 and a bread box starting with 10. Walk through every number from 1 up to 40. If the number is a multiple of 3, move 3 from the bread box into the cookie jar; otherwise move 1 from the cookie jar into the bread box. However, if a move would ever cause either one to drop below 4, skip it. Stop immediately as soon as the cookie jar holds more than twice as much as the bread box. Finally, print the number where you stopped, followed by the amounts in the cookie jar and the bread box.
"#,
        expect: Expect::Output(&["15", "11", "4"]),
    },
    Case {
        band: 4,
        held_out: true,
        name: "h4-box2",
        source: r#"Imagine a main vault starting with 9 and a petty cash starting with 14. Walk through every number from 1 up to 30. If the number is a multiple of 3, transfer 3 from the petty cash to the main vault; otherwise transfer 1 from the main vault to the petty cash. However, if a move would ever cause either one to drop below 4, skip it. Stop immediately as soon as the main vault holds more than twice as much as the petty cash. Finally, print the number where you stopped, followed by the amounts in the main vault and the petty cash.
"#,
        expect: Expect::Output(&["21", "16", "7"]),
    },
    Case {
        band: 4,
        held_out: true,
        name: "h4-box3",
        source: r#"Imagine a front tank starting with 7 and a back tank starting with 15. Walk through every number from 1 up to 30. If the number is a multiple of 4, move 5 from the back tank into the front tank; otherwise move 1 from the front tank into the back tank. However, if a move would ever cause either one to drop below 5, skip it. Stop immediately as soon as the front tank holds more than twice as much as the back tank. Finally, print the number where you stopped, followed by the amounts in the front tank and the back tank.
"#,
        expect: Expect::Output(&["16", "16", "6"]),
    },
    Case {
        band: 4,
        held_out: true,
        name: "h4-box4",
        source: r#"Imagine a red basket starting with 6 and a blue basket starting with 13. Walk through every number from 1 up to 30. If the number is a multiple of 4, transfer 4 from the blue basket to the red basket; otherwise transfer 1 from the red basket to the blue basket. However, if a move would ever cause either one to drop below 4, skip it. Stop immediately as soon as the red basket holds more than twice as much as the blue basket. Finally, print the number where you stopped, followed by the amounts in the red basket and the blue basket.
"#,
        expect: Expect::Output(&["24", "13", "6"]),
    },
    Case {
        band: 5,
        held_out: true,
        name: "h5-list1",
        source: r#"Start with the values 30, 4, 17, 9, 22, 6, and 13. Compare each number with the next one, from left to right. If the left one is bigger than the right one, swap them and count a swap. After that pass, drop all numbers that are divisible by 3. At the end, print the remaining numbers, followed by how many swaps were made.
"#,
        expect: Expect::Output(&["4", "17", "22", "13", "6"]),
    },
    Case {
        band: 5,
        held_out: true,
        name: "h5-list2",
        source: r#"Start with the list of numbers 8, 21, 5, 14, 3, 19, and 11. Go through the list from left to right, comparing each number with the one immediately after it. If the left number is larger than the right one, swap the two and count a swap. After finishing that pass, remove any number from the list that is divisible by 4. Finally, print the remaining numbers in order, followed by the total number of swaps made.
"#,
        expect: Expect::Output(&["5", "14", "3", "19", "11", "21", "5"]),
    },
    Case {
        band: 5,
        held_out: true,
        name: "h5-list3",
        source: r#"Start with the items 12, 2, 15, 7, 20, 5, and 9. Compare each number with the next one, from left to right. If the left one is bigger than the right one, swap them and count a swap. After that pass, drop all numbers that are divisible by 2. At the end, print the remaining numbers, followed by how many swaps were made.
"#,
        expect: Expect::Output(&["7", "15", "5", "9", "4"]),
    },
    Case {
        band: 5,
        held_out: true,
        name: "h5-list4",
        source: r#"Start with the list of numbers 18, 6, 25, 3, 10, 14, and 8. Go through the list from left to right, comparing each number with the one immediately after it. If the left number is larger than the right one, swap the two and count a swap. After finishing that pass, remove any number from the list that is divisible by 5. Finally, print the remaining numbers in order, followed by the total number of swaps made.
"#,
        expect: Expect::Output(&["6", "18", "3", "14", "8", "5"]),
    },
    Case {
        band: 6,
        held_out: true,
        name: "h6-higher",
        source: r#"Let x be 14.
If x is higher than 10, print "yes".
"#,
        expect: Expect::Output(&["yes"]),
    },
    Case {
        band: 6,
        held_out: true,
        name: "h6-lower",
        source: r#"Let x be 3.
If x is lower than 8, print "yes".
"#,
        expect: Expect::Output(&["yes"]),
    },
    Case {
        band: 6,
        held_out: true,
        name: "h6-no-larger",
        source: r#"Let x be 6.
If x is no larger than 6, print "yes".
"#,
        expect: Expect::Output(&["yes"]),
    },
    Case {
        band: 6,
        held_out: true,
        name: "h6-third",
        source: r#"Let x be 21.
Print a third of x.
"#,
        expect: Expect::Output(&["7"]),
    },
    Case {
        band: 7,
        held_out: true,
        name: "h7-trim-print",
        source: r#"Let label be "  Ada  ".
Print trim label.
"#,
        expect: Expect::Output(&["Ada"]),
    },
    Case {
        band: 7,
        held_out: true,
        name: "h7-starts",
        source: r#"Let code be "abc123".
If code starts with "abc", print "ok".
"#,
        expect: Expect::Output(&["ok"]),
    },
    Case {
        band: 7,
        held_out: true,
        name: "h7-record",
        source: r#"A record Point has x (a whole number) and y (a whole number).
Create a Point called q.
Update q so its x is 6 and its y is 2.
Print q's x minus q's y.
"#,
        expect: Expect::Output(&["4"]),
    },
    Case {
        band: 7,
        held_out: true,
        name: "h7-discourse",
        source: r#"Look at every number from 3 up to 15. If number is divisible by 3, print it out and count it. Once you are done, print how many were found.
"#,
        expect: Expect::Output(&["3", "6", "9", "12", "15", "5"]),
    },
    Case {
        band: 8,
        held_out: true,
        name: "h8-narrative-1",
        source: r#"The train left the station before sunrise.
"#,
        expect: Expect::Output(&[]),
    },
    Case {
        band: 8,
        held_out: true,
        name: "h8-narrative-2",
        source: r#"She didn't answer the phone because she was asleep.
"#,
        expect: Expect::Output(&[]),
    },
    Case {
        band: 8,
        held_out: true,
        name: "h8-typo",
        source: r#"Let x be 5.
Showw x.
"#,
        expect: Expect::Failure("did you mean"),
    },
    Case {
        band: 8,
        held_out: true,
        name: "h8-empty",
        source: r#"Let x be 5.
Repeat while x is greater than 1:

Print x.
"#,
        expect: Expect::Failure("empty loop body"),
    },
];

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn stdout_lines(result: &RunResult) -> Vec<String> {
    let mut output = Vec::new();
    let mut in_stdout = false;
    for line in &result.lines {
        if line == "--- stdout ---" {
            in_stdout = true;
            continue;
        }
        if in_stdout && line.starts_with("--- ") {
            break;
        }
        if in_stdout {
            output.push(line.clone());
        }
    }
    output
}

fn temp_dir(tag: &str) -> PathBuf {
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let directory = std::env::temp_dir().join(format!(
        "tc_r1_battle_{}_{}_{tag}",
        std::process::id(),
        counter
    ));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("temp directory");
    directory
}

fn check_case(case: &Case) {
    match &case.expect {
        Expect::Output(expected) => {
            let result = run_elf_with_parity(case.source, None, "");
            assert!(
                result.ok,
                "{}: {}\n{}",
                case.name,
                result.summary,
                result.lines.join("\n")
            );
            assert_eq!(stdout_lines(&result), expected.to_vec(), "{}", case.name);
        }
        Expect::Failure(needle) => {
            let directory = temp_dir("failure");
            let program = directory.join("program.eng");
            fs::write(&program, case.source).expect("program");
            let output = Command::new(env!("CARGO_BIN_EXE_tc"))
                .arg("build")
                .arg(&program)
                .arg("-o")
                .arg(program.with_extension("elf"))
                .output()
                .expect("tc build");
            assert!(
                !output.status.success(),
                "{} compiled unexpectedly",
                case.name
            );
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(
                stderr
                    .to_ascii_lowercase()
                    .contains(&needle.to_ascii_lowercase()),
                "{}: missing `{needle}`:\n{stderr}",
                case.name
            );
            let _ = fs::remove_dir_all(&directory);
        }
    }
}

fn check_band(band: u8) {
    let mut count = 0;
    for case in CASES.iter().filter(|case| case.band == band) {
        check_case(case);
        count += 1;
    }
    assert!(count >= 15, "band {band} has only {count} cases");
}

#[test]
fn band_1_casual_openers_and_contractions() {
    check_band(1);
}

#[test]
fn band_2_range_filtering_and_transforms() {
    check_band(2);
}

#[test]
fn band_3_sequences_totals_and_overflows() {
    check_band(3);
}

#[test]
fn band_4_stateful_guarded_transfers() {
    check_band(4);
}

#[test]
fn band_5_list_passes_swaps_and_compaction() {
    check_band(5);
}

#[test]
fn band_6_everyday_math_and_comparisons() {
    check_band(6);
}

#[test]
fn band_7_strings_records_functions_discourse() {
    check_band(7);
}

#[test]
fn band_8_narrative_safety_errors_and_heuristics() {
    check_band(8);
}

#[test]
fn battle_test_table_has_discovery_and_held_out_cases() {
    let discovery = CASES.iter().filter(|case| !case.held_out).count();
    let held_out = CASES.iter().filter(|case| case.held_out).count();
    assert!(discovery >= 120, "only {discovery} discovery cases");
    assert!(held_out >= 30, "only {held_out} held-out cases");
}

fn count_control_flow(statements: &[Stmt], loops: &mut usize, branches: &mut usize) {
    for statement in statements {
        match statement {
            Stmt::Loop { body } => {
                *loops += 1;
                count_control_flow(body, loops, branches);
            }
            Stmt::If {
                then_body,
                else_body,
                ..
            } => {
                *branches += 1;
                count_control_flow(then_body, loops, branches);
                count_control_flow(else_body, loops, branches);
            }
            _ => {}
        }
    }
}

#[test]
fn algorithmic_battle_cases_lower_to_runtime_core7_control_flow() {
    for case in CASES
        .iter()
        .filter(|case| (2..=5).contains(&case.band))
        .filter(|case| matches!(case.expect, Expect::Output(_)))
    {
        let project =
            compile_english_to_ir(&[SemanticSourceFile::new("main.tc", case.source)], "main")
                .unwrap_or_else(|errors| panic!("{}: {errors:#?}", case.name));
        let entry = &project.module.functions[project.module.entry.index()];
        let mut loops = 0;
        let mut branches = 0;
        count_control_flow(&entry.body, &mut loops, &mut branches);
        assert!(loops > 0, "{}: no runtime Core-7 loop", case.name);
        assert!(branches > 0, "{}: no runtime Core-7 branch", case.name);
    }
}

#[test]
fn fuzz_100_malformed_full_lowering_seeds_do_not_panic() {
    let fragments = [
        "Let x be 1.",
        "If x is greater than 1",
        "Print x.",
        "x isn't 2",
        "Repeat while x is 1:",
        "For every n from 1 to 3",
        "add 1 to x",
        "Take every number from 1 to 5.",
        "Skip any multiple of 2.",
        "Start with 1 and 2.",
        "Move 4 from a into b.",
        "Print trim \"  x  \".",
        "A record Point has x (a whole number).",
        "Show \"unterminated",
        "((((((((((",
        "))))))))))",
        "If",
        "else",
        "return",
        "1 + + 2",
    ];
    let mut state = 0x9E37_79B9_7F4A_7C15_u64;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for _ in 0..100 {
        let length = (next() as usize % 6) + 1;
        let source = (0..length)
            .map(|_| fragments[next() as usize % fragments.len()])
            .collect::<Vec<_>>()
            .join("\n");
        let _ = compile_english_to_ir(&[SemanticSourceFile::new("main.tc", source)], "main");
    }
}

#[test]
fn fuzz_5000_malformed_syntax_seeds_do_not_panic() {
    let vocabulary = [
        "if",
        "while",
        "repeat",
        "let",
        "set",
        "for",
        "every",
        "(",
        ")",
        "{",
        "}",
        "[",
        "]",
        "\"",
        "'",
        ",",
        ".",
        ":",
        ";",
        "and",
        "or",
        "not",
        "x",
        "n",
        "1",
        "0",
        "on",
        "off",
        "list",
        "numbers",
        "divisible",
        "by",
        "move",
        "from",
        "to",
        "print",
        "say",
        "as",
        "the",
        "a",
        "sum",
        "of",
        "starts",
        "with",
        "contains",
        "!",
        "!=",
        "->",
        "empty",
        "repeat",
    ];
    let mut state = 0x2545_F491_4F6C_DD1D_u64;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for _ in 0..5000 {
        let length = (next() as usize % 28) + 1;
        let source = (0..length)
            .map(|_| vocabulary[next() as usize % vocabulary.len()])
            .collect::<Vec<_>>()
            .join(" ");
        let _ = tokenize(&source);
        let _ = split_sentences(&source);
        let file = SourceFile::new("fuzz.tc", source);
        let _ = segment_source(&file);
    }
}
