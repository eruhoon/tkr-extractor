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
        // Match line number like "123: code..."
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
  console.log(`Total unique lines found: ${lineNumbers.length}`);
  console.log(`Max line number: ${maxLine}`);
  
  // Reconstruct file content
  const reconstructedLines = [];
  for (let i = 1; i <= maxLine; i++) {
    if (lineMap[i] !== undefined) {
      reconstructedLines.push(lineMap[i]);
    } else {
      console.log(`WARNING: Line ${i} is missing!`);
      reconstructedLines.push('');
    }
  }
  
  const reconstructedContent = reconstructedLines.join('\n');
  const targetPath = "C:\\Users\\CHC\\Desktop\\tkr-extractor\\src\\lib\\components\\ScriptTranslator.svelte";
  fs.writeFileSync(targetPath, reconstructedContent, 'utf8');
  console.log(`Saved reconstructed file to ${targetPath}`);
});
