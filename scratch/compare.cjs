const fs = require('fs');

const sveltePath = "C:\\Users\\CHC\\Desktop\\tkr-extractor\\src\\lib\\components\\ScriptTranslator.svelte";
const cleanContent = fs.readFileSync(sveltePath, 'utf8').replace(/\r\n/g, '\n');

const step67TargetRaw = "\"  let ollamaModelName = $state('gemma4:e4b');\\n  let availableModels = $state<{name: string}[]>([]);\\n  let ollamaStatus = $state<'online' | 'offline' | 'checking'>('checking');\\n  let translationMode = $state('ollama'); // 'ollama' | 'gemini'\\n  let isExtracting = $state(false);\\n  let showDebugPanel = $state(false);\\n  let debugLogs = $state<string[]>([]);\\n\\n  function addLog(msg: string) {\\n    const time = new Date().toLocaleTimeString();\\n    debugLogs = [`[${time}] ${msg}`, ...debugLogs];\\n  }\\n\\n  function saveModelName() {\\n    localStorage.setItem('ollamaModelName', ollamaModelName);\\n    addLog(\\\"모델명 변경됨: \\\" + ollamaModelName);\\n  }\\n\\n  onMount(() => {\\n    ollamaModelName = localStorage.getItem('ollamaModelName') || 'gemma4:e4b';\\n    addLog(\\\"앱 초기화 완료. 모델명: \\\" + ollamaModelName);\\n\\n    const handleOutsideClick = (e: MouseEvent) => {\\n      const target = e.target as HTMLElement;\\n      if (showOpenMenu && !target.closest('.dropdown')) {\\n        showOpenMenu = false;\\n      }\\n    };\\n    window.addEventListener('click', handleOutsideClick);\\n\\n    const initOllama = async () => {\\n      try {\\n        const res = await fetch(\\\"http://127.0.0.1:11434/api/tags\\\");\\n        if (res.ok) {\\n          const data = await res.json();\\n          availableModels = data.models || [];\\n          if (availableModels.length > 0 && !localStorage.getItem('ollamaModelName')) {\\n            ollamaModelName = availableModels[0].name;\\n            localStorage.setItem('ollamaModelName', ollamaModelName);\\n          }\\n          ollamaStatus = 'online';\\n        } else {\\n          ollamaStatus = 'offline';\\n        }\\n      } catch (e) {\\n        addLog(\\\"Ollama 서버 연결 실패: 로컬 서버가 켜져 있는지 확인하세요.\\\");\\n        ollamaStatus = 'offline';\\n      }\\n    };\\n\\n    initOllama();\\n\\n    return () => {\\n      window.removeEventListener('click', handleOutsideClick);\\n    };\\n  });\"";

const step67Target = JSON.parse(step67TargetRaw).replace(/\r\n/g, '\n');

console.log("Clean content length:", cleanContent.length);
console.log("Step 67 target length:", step67Target.length);

// Let's find where "let ollamaModelName = $state('gemma4:e4b');" is in cleanContent
const searchStr = "let ollamaModelName = $state('gemma4:e4b');";
const idx = cleanContent.indexOf(searchStr);
console.log("Found search string at index:", idx);

if (idx !== -1) {
  // Extract the corresponding block of the same length from cleanContent
  const cleanBlock = cleanContent.substring(idx - 2, idx - 2 + step67Target.length);
  console.log("Clean block length:", cleanBlock.length);
  
  // Find first mismatch
  let mismatchIdx = -1;
  for (let i = 0; i < Math.min(cleanBlock.length, step67Target.length); i++) {
    if (cleanBlock[i] !== step67Target[i]) {
      mismatchIdx = i;
      break;
    }
  }
  
  if (mismatchIdx !== -1) {
    console.log("First mismatch at index:", mismatchIdx);
    console.log("Clean Content around mismatch:", JSON.stringify(cleanBlock.substring(mismatchIdx - 10, mismatchIdx + 20)));
    console.log("Step 67 Target around mismatch:", JSON.stringify(step67Target.substring(mismatchIdx - 10, mismatchIdx + 20)));
  } else {
    console.log("NO MISMATCH FOUND! They are identical!");
  }
}
