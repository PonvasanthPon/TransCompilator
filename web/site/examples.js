/* Verified natural-English presets. Every program's stdout matches native
   `tc interp` byte-for-byte when run through the wasm32 fallback parser. */
window.TC_EXAMPLES = [
  {
    id: "sum",
    group: "Natural English Basics",
    title: "Quick Natural Math",
    summary: "Spoken numbers, a polite question, and a where-clause.",
    code: [
      "A small arithmetic warm-up.",
      "",
      "Can you tell me the sum of a and b, where a is twelve and b is twenty-three?",
      ""
    ].join("\n")
  },
  {
    id: "contractions",
    group: "Natural English Basics",
    title: "Contractions & Everyday Predicates",
    summary: "Everyday contractions, parity, divisibility, and range checks.",
    code: [
      "Let n be 14.",
      "",
      "If n isn't zero and n is even, print \"even\".",
      "Otherwise, print \"odd\".",
      "",
      "If n doesn't equal 13, print \"not thirteen\".",
      "If n is divisible by 7, print \"divisible by seven\".",
      "If n is between 10 and 20, print \"in range\".",
      ""
    ].join("\n")
  },
  {
    id: "narrative",
    group: "Natural English Basics",
    title: "Narrative Prose & Safe Omission",
    summary: "Narrative sentences are omitted while list filtering executes.",
    code: [
      "She cleared the table after dinner.",
      "",
      "The old ledger sat on the shelf.",
      "",
      "Let prices be a list of 10, 20, and 30.",
      "For each price in prices, if price is at least twenty, show me price.",
      "Print \"Average: \" with the average of prices.",
      ""
    ].join("\n")
  },
  {
    id: "fibonacci",
    group: "Algorithms & Simulations",
    title: "Running Fibonacci Total",
    summary: "Generate a bounded Fibonacci sequence and wrap a running total.",
    code: [
      "Start with 1 and 2.",
      "Keep generating the next number by adding the previous two numbers together, as long as the new number doesn't exceed 200.",
      "For each number in this entire sequence, including the starting two, add it to a running total.",
      "Whenever that total goes over 100, subtract 100 from it and count an overflow.",
      "At the end, print the final running total and how many overflows happened.",
      ""
    ].join("\n")
  },
  {
    id: "boxes",
    group: "Algorithms & Simulations",
    title: "Two Boxes with Guarded Moves",
    summary: "A step-by-step transfer simulation with guards and a stop condition.",
    code: [
      "Imagine a red box starting with 5 and a blue box starting with 16.",
      "Walk through every number from 1 up to 40.",
      "If the number is a multiple of 3, move 4 from the blue box into the red box; otherwise, move 1 from the red box into the blue box.",
      "However, if a move would ever cause either box to drop below 5, skip it.",
      "Stop immediately as soon as the red box holds more than twice as much as the blue box.",
      "Finally, print the number where you stopped, followed by the amounts in the red box and the blue box.",
      ""
    ].join("\n")
  },
  {
    id: "primes",
    group: "Algorithms & Simulations",
    title: "Prime Finder",
    summary: "Natural quantifiers and nested checks discover primes below fifty.",
    code: [
      "Look at every number from 2 up to 50.",
      "For each number, check whether any smaller number starting from 2 divides into it evenly.",
      "If none of them do, print that number out and count it.",
      "Once you are done, print how many numbers were found.",
      ""
    ].join("\n")
  },
  {
    id: "collatz",
    group: "Algorithms & Simulations",
    title: "Collatz Sequence",
    summary: "A paragraph-scoped loop traces the Collatz sequence from twenty-seven.",
    code: [
      "Let current be 27, and let steps start at 0.",
      "",
      "Print \"Tracing sequence from \" with current.",
      "",
      "Repeat while current is greater than 1:",
      "If current is even, divide current by 2.",
      "Otherwise, multiply current by 3 and add 1 to current.",
      "Add 1 to steps, then print current.",
      "",
      "Print \"Reached 1 in \" with steps and \" steps.\"",
      ""
    ].join("\n")
  },
  {
    id: "range",
    group: "Algorithms & Simulations",
    title: "Range Filter & Largest Value",
    summary: "Filtering, branching arithmetic, and a tracked maximum.",
    code: [
      "Take all the numbers between 1 and 30.",
      "Skip any multiple of 4, unless it is also divisible by 6.",
      "For each remaining number, if it is odd, triple it and add 1; otherwise cut it in half.",
      "Print the final value only if it ends up greater than 20, and keep track of the largest value you printed.",
      "Once you finish going through all of them, print that largest value.",
      ""
    ].join("\n")
  },
  {
    id: "list-pass",
    group: "Lists & Strings",
    title: "Single Pass Over a List",
    summary: "Adjacent swaps, a swap counter, and divisibility compaction.",
    code: [
      "Start with the list of numbers 14, 7, 22, 9, 15, 4, and 18.",
      "Go through the list from left to right, comparing each number with the one immediately after it.",
      "Whenever the left number is larger than the right one, swap their positions and count a swap.",
      "After finishing that pass, remove any number from the list that is divisible by 3.",
      "Finally, print the remaining numbers in order, followed by the total number of swaps made.",
      ""
    ].join("\n")
  },
  {
    id: "aggregates",
    group: "Lists & Strings",
    title: "Inline List Aggregates & Filtering",
    summary: "Lists feed filtering, sum, average, minimum, maximum, and count.",
    code: [
      "Let prices be a list of 10, 20, and 30.",
      "",
      "For each price in prices, if price is at least twenty, show me price.",
      "",
      "Print \"Sum: \" with the sum of prices.",
      "Print \"Average: \" with the average of prices.",
      "Print \"Minimum: \" with the minimum of prices.",
      "Print \"Maximum: \" with the maximum of prices.",
      "Print \"Count: \" with the number of items in prices.",
      ""
    ].join("\n")
  },
  {
    id: "strings",
    group: "Lists & Strings",
    title: "Zero-Boilerplate Strings & Math",
    summary: "Trim, length, prefix, containment, and a square root with no setup.",
    code: [
      "Let title be \"  TransCompilator  \".",
      "Let clean be trim title.",
      "",
      "Print \"Raw length: \" with the length of title.",
      "Print \"Trimmed length: \" with the length of clean.",
      "Say clean.",
      "If clean starts with \"Trans\", print \"prefix ok\".",
      "If title contains \"Compilator\", print \"contains ok\".",
      "Print \"Root: \" with the square root of 144.",
      ""
    ].join("\n")
  },
  {
    id: "records",
    group: "Records, Functions & Anaphora",
    title: "Records & ACE its Anaphora",
    summary: "A record is created, updated, and changed through its fields.",
    code: [
      "A record Trip has a distance (a whole number) and a toll (a whole number).",
      "",
      "Create a Trip called pondy.",
      "Update pondy so its distance is 160 and its toll is 220.",
      "Add 15 to its toll.",
      "Print \"Distance: \" with pondy's distance.",
      "Print \"Toll: \" with pondy's toll.",
      ""
    ].join("\n")
  },
  {
    id: "function",
    group: "Records, Functions & Anaphora",
    title: "Custom Paragraph Function",
    summary: "A natural-language function doubles its argument and adds one.",
    code: [
      "To double and add one given x as a whole number, named boost, returning a whole number.",
      "Give back x times 2 plus 1.",
      "",
      "Print boost(20).",
      ""
    ].join("\n")
  },
  {
    id: "discourse",
    group: "Records, Functions & Anaphora",
    title: "Multi-Step Discourse & Plural Anaphora",
    summary: "Plural anaphora resolves across steps, followed by scalar updates.",
    code: [
      "First, let a be twelve and b be twenty-three.",
      "Print their sum.",
      "Print their product.",
      "Square a.",
      "Halve b.",
      "Print a.",
      "Print b.",
      ""
    ].join("\n")
  }
];
