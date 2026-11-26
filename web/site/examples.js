/* Verified natural-English presets. Every program's stdout matches the native
   `tc interp` output byte-for-byte when run through the wasm32 fallback parser. */
window.TC_EXAMPLES = [
  {
    id: "sum",
    title: "Quick Natural Math",
    code: [
      "Can you tell me the sum of a and b, where a is twelve and b is twenty-three?",
      ""
    ].join("\n")
  },
  {
    id: "fibonacci",
    title: "Running Fibonacci Total",
    code: [
      "Start with 1 and 2. Keep generating the next number by adding the previous two numbers together, as long as the new number doesn't exceed 200. For each number in this entire sequence, including the starting two, add it to a running total. Whenever that total goes over 100, subtract 100 from it and count an overflow. At the end, print the final running total and how many overflows happened.",
      ""
    ].join("\n")
  },
  {
    id: "boxes",
    title: "Two Boxes with Guarded Moves",
    code: [
      "Imagine a red box starting with 5 and a blue box starting with 16. Walk through every number from 1 up to 40. If the number is a multiple of 3, move 4 from the blue box into the red box; otherwise, move 1 from the red box into the blue box. However, if a move would ever cause either box to drop below 5, skip it. Stop immediately as soon as the red box holds more than twice as much as the blue box. Finally, print the number where you stopped, followed by the amounts in the red box and the blue box.",
      ""
    ].join("\n")
  },
  {
    id: "list-pass",
    title: "Single Pass Over a List",
    code: [
      "Start with the list of numbers 14, 7, 22, 9, 15, 4, and 18. Go through the list from left to right, comparing each number with the one immediately after it. Whenever the left number is larger than the right one, swap their positions and count a swap. After finishing that pass, remove any number from the list that is divisible by 3. Finally, print the remaining numbers in order, followed by the total number of swaps made.",
      ""
    ].join("\n")
  },
  {
    id: "primes",
    title: "Prime Finder",
    code: [
      "Look at every number from 2 up to 50. For each number, check whether any smaller number starting from 2 divides into it evenly. If none of them do, print that number out and count it. Once you are done, print how many numbers were found.",
      ""
    ].join("\n")
  },
  {
    id: "collatz",
    title: "Collatz Sequence",
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
    title: "Range Filter and Largest Value",
    code: [
      "Take all the numbers between 1 and 30. Skip any multiple of 4, unless it is also divisible by 6. For each remaining number, if it is odd, triple it and add 1; otherwise cut it in half. Print the final value only if it ends up greater than 20, and keep track of the largest value you printed. Once you finish going through all of them, print that largest value.",
      ""
    ].join("\n")
  },
  {
    id: "narrative",
    title: "Narrative Prose and Diagnostics",
    code: [
      "She cleared the table after dinner.",
      "",
      "Let prices be a list of 10, 20, and 30.",
      "For each price in prices, if price is at least twenty, show me price.",
      "Print \"Average: \" with the average of prices.",
      ""
    ].join("\n")
  }
];
