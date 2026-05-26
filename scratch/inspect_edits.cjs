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
    if (data.type === 'PLANNER_RESPONSE' && data.tool_calls) {
      for (const tc of data.tool_calls) {
        if (tc.name === 'replace_file_content' || tc.name === 'multi_replace_file_content') {
          let args = tc.args;
          if (typeof args === 'string') {
            try { args = JSON.parse(args); } catch(e) {}
          }
          if (args && args.TargetFile && args.TargetFile.includes("ScriptTranslator.svelte")) {
            console.log(`Step ${data.step_index}: ${tc.name}`);
            if (tc.name === 'replace_file_content') {
              console.log(`  Start: ${args.StartLine}, End: ${args.EndLine}`);
              console.log(`  Target (first 50 char): ${JSON.stringify(args.TargetContent).substring(0, 80)}`);
              console.log(`  Replacement (first 50 char): ${JSON.stringify(args.ReplacementContent).substring(0, 80)}`);
              if (args.TargetContent.includes('...') || args.ReplacementContent.includes('...')) {
                console.log(`  WARNING: MAY BE TRUNCATED!`);
              }
            } else {
              console.log(`  Chunks count: ${args.ReplacementChunks ? args.ReplacementChunks.length : 0}`);
            }
          }
        }
      }
    }
  } catch(e) {
    console.error(e);
  }
});
