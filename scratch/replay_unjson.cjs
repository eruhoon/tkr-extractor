const fs = require('fs');
const readline = require('readline');

const logPath = "C:\\Users\\CHC\\.gemini\\antigravity\\brain\\a4f16834-7863-4cee-aafe-9de9ac034129\\.system_generated\\logs\\transcript.jsonl";
const sveltePath = "C:\\Users\\CHC\\Desktop\\tkr-extractor\\src\\lib\\components\\ScriptTranslator.svelte";

let content = fs.readFileSync(sveltePath, 'utf8');

const rl = readline.createInterface({
  input: fs.createReadStream(logPath),
  output: process.stdout,
  terminal: false
});

const edits = [];

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
            edits.push({
              step: data.step_index,
              name: tc.name,
              args: args
            });
          }
        }
      }
    }
  } catch(e) {}
});

function decodeLogString(str) {
  if (!str) return '';
  let val = str.trim();
  if (val.startsWith('"')) {
    try {
      // First level of JSON decode
      const escaped1 = val.replace(/\n/g, '\\n').replace(/\r/g, '\\r');
      let decoded1 = JSON.parse(escaped1);
      
      // If it still starts and ends with double quotes, it is double encoded!
      if (typeof decoded1 === 'string' && decoded1.startsWith('"') && decoded1.endsWith('"')) {
        try {
          const escaped2 = decoded1.replace(/\n/g, '\\n').replace(/\r/g, '\\r');
          return JSON.parse(escaped2);
        } catch (e2) {
          // If JSON parse fails on second level (due to unescaped quotes inside),
          // simply strip the outer quotes from decoded1.
          return decoded1.substring(1, decoded1.length - 1);
        }
      }
      return decoded1;
    } catch (e) {
      // Manual decode fallback
      let raw = str;
      if (raw.startsWith('"') && raw.endsWith('"')) {
        raw = raw.substring(1, raw.length - 1);
      }
      return raw
        .replace(/\\"/g, '"')
        .replace(/\\n/g, '\n')
        .replace(/\\r/g, '\r')
        .replace(/\\t/g, '\t')
        .replace(/\\\\/g, '\\');
    }
  }
  return str;
}

rl.on('close', () => {
  console.log(`Found ${edits.length} edits to replay.`);
  edits.sort((a, b) => a.step - b.step);
  
  let successCount = 0;
  for (const edit of edits) {
    if (edit.step >= 617) {
      console.log(`Skipping Step ${edit.step} (our own/broken edits)...`);
      continue;
    }
    
    console.log(`Replaying Step ${edit.step} (${edit.name})...`);
    if (edit.name === 'replace_file_content') {
      const target = decodeLogString(edit.args.TargetContent);
      const replacement = decodeLogString(edit.args.ReplacementContent);
      
      const normContent = content.replace(/\r\n/g, '\n');
      const normTarget = target.replace(/\r\n/g, '\n');
      const normReplacement = replacement.replace(/\r\n/g, '\n');
      
      if (normContent.includes(normTarget)) {
        content = normContent.replace(normTarget, normReplacement);
        console.log(`  Succeeded.`);
        successCount++;
      } else {
        console.log(`  FAILED: Target content not found!`);
        console.log(`  Target expected (first 100): ${JSON.stringify(target).substring(0, 100)}`);
      }
    } else if (edit.name === 'multi_replace_file_content') {
      let allChunksOk = true;
      let tempContent = content.replace(/\r\n/g, '\n');
      
      const chunks = edit.args.ReplacementChunks;
      for (const chunk of chunks) {
        const target = decodeLogString(chunk.TargetContent).replace(/\r\n/g, '\n');
        const replacement = decodeLogString(chunk.ReplacementContent).replace(/\r\n/g, '\n');
        
        if (tempContent.includes(target)) {
          tempContent = tempContent.replace(target, replacement);
        } else {
          console.log(`  Chunk FAILED in Step ${edit.step}: Target not found!`);
          console.log(`  Target expected (first 100): ${JSON.stringify(target).substring(0, 100)}`);
          allChunksOk = false;
        }
      }
      if (allChunksOk) {
        content = tempContent;
        console.log(`  Succeeded (all chunks).`);
        successCount++;
      } else {
        console.log(`  FAILED: One or more chunks failed.`);
      }
    }
  }
  
  console.log(`Replayed ${successCount}/${edits.length - 2} successfully.`);
  fs.writeFileSync(sveltePath, content, 'utf8');
  console.log("Saved reconstructed file to src/lib/components/ScriptTranslator.svelte");
});
