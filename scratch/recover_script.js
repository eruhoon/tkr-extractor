const fs = require('fs');
const readline = require('readline');

const logPath = "C:\\Users\\CHC\\.gemini\\antigravity\\brain\\a4f16834-7863-4cee-aafe-9de9ac034129\\.system_generated\\logs\\transcript.jsonl";

const rl = readline.createInterface({
  input: fs.createReadStream(logPath),
  output: process.stdout,
  terminal: false
});

rl.on('line', (line) => {
  try {
    const data = JSON.parse(line);
    
    // Check if the step has tool calls
    if (data.tool_calls) {
      for (const tc of data.tool_calls) {
        let args = tc.args;
        if (typeof args === 'string') {
          try {
            args = JSON.parse(args);
          } catch(e) {}
        }
        if (args && typeof args === 'object') {
          const filePath = args.AbsolutePath || args.TargetFile;
          if (filePath && filePath.includes("ScriptTranslator.svelte")) {
            console.log(`Step ${data.step_index}: ${tc.name}`);
            if (data.output) {
              console.log(`  Output length: ${data.output.length}`);
              // Print first 100 characters of output to see if it contains file contents
              console.log(`  Snippet: ${data.output.substring(0, 150).replace(/\n/g, ' ')}`);
            }
          }
        }
      }
    }
  } catch(e) {}
});
