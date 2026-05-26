const fs = require('fs');
const readline = require('readline');

const logPath = "C:\\Users\\CHC\\.gemini\\antigravity\\brain\\a4f16834-7863-4cee-aafe-9de9ac034129\\.system_generated\\logs\\transcript.jsonl";

const rl = readline.createInterface({
  input: fs.createReadStream(logPath),
  output: process.stdout,
  terminal: false
});

const lineMap = {};

rl.on('line', (line) => {
  try {
    const data = JSON.parse(line);
    if (data.type === 'VIEW_FILE') {
      const content = data.content;
      const lines = content.split('\n');
      for (const l of lines) {
        const match = l.match(/^(\d+): (.*)$/);
        if (match) {
          const lineNum = parseInt(match[1], 10);
          const lineContent = match[2];
          lineMap[lineNum] = lineContent;
        }
      }
    }
  } catch(e) {}
});

rl.on('close', () => {
  const lineNumbers = Object.keys(lineMap).map(n => parseInt(n, 10)).sort((a, b) => a - b);
  const maxLine = Math.max(...lineNumbers);
  
  const missing = [];
  for (let i = 1; i <= maxLine; i++) {
    if (lineMap[i] === undefined) {
      missing.push(i);
    }
  }
  
  console.log("Max line number:", maxLine);
  console.log("Missing lines count:", missing.length);
  
  // Group missing lines into ranges for readability
  if (missing.length > 0) {
    const ranges = [];
    let start = missing[0];
    let prev = missing[0];
    for (let i = 1; i < missing.length; i++) {
      if (missing[i] === prev + 1) {
        prev = missing[i];
      } else {
        ranges.push(`${start}-${prev}`);
        start = missing[i];
        prev = missing[i];
      }
    }
    ranges.push(`${start}-${prev}`);
    console.log("Missing ranges:", ranges.join(", "));
  }
});
