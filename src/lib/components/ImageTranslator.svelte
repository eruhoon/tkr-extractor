<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { open, ask, message } from '@tauri-apps/plugin-dialog';
  import { onMount } from 'svelte';
  import { fetch } from '@tauri-apps/plugin-http';

  interface OcrTextRegion {
    ja_text: string;
    ko_text: string;
    x: number;
    y: number;
    w: number;
    h: number;
  }

  interface ImageEntry {
    path: string;
    name: string;
    regions: OcrTextRegion[];
    status: 'pending' | 'processing' | 'done' | 'error';
    errorMsg?: string;
    translatedPath?: string;
  }

  let { isDragging = false } = $props();

  let selectedFolder = $state('');
  let translationMap = $state<Record<string, string>>({});
  let images = $state<ImageEntry[]>([]);
  let selectedImage = $state<ImageEntry | null>(null);
  let isProcessing = $state(false);
  let cancelRequested = $state(false);
  let batchTotal = $state(0);
  let batchCompleted = $state(0);

  let ollamaModelName = $state('gemma4:e4b');
  let availableModels = $state<{name: string}[]>([]);
  let ollamaStatus = $state<'online' | 'offline' | 'checking'>('checking');
  let totalImagesProcessed = $state(0);
  let showDebugPanel = $state(false);
  let debugLogs = $state<string[]>([]);
  let geminiApiKey = $state('');
  function saveGeminiApiKey() {
    localStorage.setItem('geminiApiKey', geminiApiKey);
    addLog("Gemini API Key 저장됨");
  }

  function addLog(msg: string) {
    const time = new Date().toLocaleTimeString();
    debugLogs = [`[${time}] ${msg}`, ...debugLogs];
  }

  let selectedModelId = $state('llamacpp:gemma-4-E4B-it');
  let llamaServerPath = $state('');
  let isLlamaServerAvailable = $state(false);

  async function updateAvailability() {
    try {
      isLlamaServerAvailable = await invoke('check_llama_server_availability', { customPath: llamaServerPath || null });
    } catch (e) {
      isLlamaServerAvailable = false;
    }
  }

  async function selectLlamaServerPath() {
    const isWindows = navigator.userAgent.includes('Windows') || navigator.userAgent.includes('Win32');
    const selected = await open({
      multiple: false,
      filters: isWindows 
        ? [{ name: 'llama-server Executable', extensions: ['exe', 'bat', 'cmd'] }]
        : [] // macOS/Linux에서는 확장자가 없는 바이너리 파일을 선택할 수 있도록 필터 해제
    });
    if (selected && typeof selected === 'string') {
      llamaServerPath = selected;
      localStorage.setItem('llamaServerPath', selected);
      addLog(`llama-server 경로 설정됨: ${selected}`);
      alert(`llama-server 경로가 설정되었습니다:\n${selected}`);
      await updateAvailability();
    }
  }

  const llamacppPresets = [
    {
      id: 'llamacpp:gemma-4-E4B-it',
      name: 'llama.cpp (Gemma-4 E4B-it)',
      modelName: 'gemma-4-E4B-it',
      downloadUrl: 'https://huggingface.co/lmstudio-community/gemma-4-E4B-it-GGUF/resolve/main/gemma-4-E4B-it-Q4_K_M.gguf'
    },
    {
      id: 'llamacpp:gemma-4-E2B-it',
      name: 'llama.cpp (Gemma-4 E2B-it)',
      modelName: 'gemma-4-E2B-it',
      downloadUrl: 'https://huggingface.co/lmstudio-community/gemma-4-E2B-it-GGUF/resolve/main/gemma-4-E2B-it-Q4_K_M.gguf'
    }
  ];

  let llamaServerRunning = $state(false);
  // 'idle' | 'starting' | 'downloading' | 'ready' | 'error'
  let llamaServerStatus = $state<'idle' | 'starting' | 'downloading' | 'ready' | 'error'>('idle');
  let llamaCurrentModelId = $state(''); // 현재 서버에 로드된 모델 ID
  let downloadProgress = $state(0); // 다운로드 진행률

  /** /health 폴링 — 최대 waitSec초 대기, ready 시 true 반환 */
  async function waitForLlamaReady(waitSec = 120): Promise<boolean> {
    const deadline = Date.now() + waitSec * 1000;
    while (Date.now() < deadline) {
      try {
        const res = await fetch('http://127.0.0.1:8080/health');
        if (res.ok) {
          const data = await res.json().catch(() => ({}));
          if (data.status === 'ok') return true;
        }
      } catch { /* 서버 아직 미기동 */ }
      await new Promise(r => setTimeout(r, 1500));
    }
    return false;
  }

  async function startLlamaIfNeeded(modelId: string) {
    if (!modelId.startsWith('llamacpp:')) return;
    const preset = llamacppPresets.find(p => p.id === modelId);
    if (!preset) return;

    // 이미 같은 모델로 ready 상태면 패스
    if (llamaCurrentModelId === modelId && llamaServerStatus === 'ready') return;

    try {
      // 1. 로컬에 모델이 존재하는지 확인
      addLog(`[llama.cpp] 모델 파일 검사 중... (${preset.modelName})`);
      const exists = await invoke<boolean>('check_model_exists', { modelId: preset.modelName });

      if (!exists) {
        addLog(`[llama.cpp] 모델 파일이 없습니다: ${preset.modelName}`);
        
        const confirmed = await ask(
          `이 작업을 진행하려면 '${preset.name}' 모델 파일(약 3.5GB~5.3GB)이 필요합니다.\n다운로드하시겠습니까?\n\n(다운로드는 백그라운드에서 진행되며 완료 후 서버가 시작됩니다)`,
          { title: '모델 다운로드 필요', kind: 'info', okLabel: '다운로드 시작', cancelLabel: '취소' }
        );

        if (!confirmed) {
          llamaServerStatus = 'error';
          addLog('[llama.cpp] 모델 다운로드가 사용자에 의해 취소되었습니다.');
          return;
        }

        llamaServerStatus = 'downloading';
        downloadProgress = 0;
        addLog(`[llama.cpp] 모델 다운로드 시작: ${preset.downloadUrl}`);
        await invoke('download_model', { modelId: preset.modelName, url: preset.downloadUrl });
        return; // 다운로드 이벤트 리스너가 완료 후 서버를 시작해 줍니다.
      }

      llamaServerStatus = 'starting';
      addLog(`[llama.cpp] 서버 시작 중... (${preset.modelName})`);
      await invoke('start_llama_server', { modelId: preset.modelName, customPath: llamaServerPath || null });

      addLog('[llama.cpp] 서버 프로세스 시작됨 — 모델 로딩 대기 중...');
      const ready = await waitForLlamaReady(120);
      if (!ready) {
        llamaServerStatus = 'error';
        throw new Error('llama-server가 120초 내에 준비되지 않았습니다.');
      }

      llamaServerRunning = true;
      llamaCurrentModelId = modelId;
      llamaServerStatus = 'ready';
      addLog('[llama.cpp] 서버 준비 완료 ✅');
    } catch (e) {
      llamaServerStatus = 'error';
      throw new Error(`llama-server 시작 실패: ${e}`);
    }
  }

  /** Ollama 서버 연결 확인 및 모델 목록 가져오기 */
  async function initOllama() {
    ollamaStatus = 'checking';
    try {
      const models = await invoke<{ name: string }[]>("get_ollama_models");
      availableModels = models;
      ollamaStatus = 'online';
    } catch (e) {
      addLog("Ollama 서버 연결 실패: 로컬 서버가 켜져 있는지 확인하세요.");
      ollamaStatus = 'offline';
    }
  }

  async function handleModelChange() {
    localStorage.setItem('selectedModelId_image', selectedModelId);
    addLog("모델 선택 변경됨: " + selectedModelId);
    
    if (selectedModelId.startsWith('ollama:')) {
      ollamaModelName = selectedModelId.replace('ollama:', '');
      localStorage.setItem('ollamaModelName_image', ollamaModelName);
      // llama 서버가 떠 있으면 종료
      if (llamaServerRunning) {
        try { await invoke('stop_llama_server'); } catch {}
        llamaServerRunning = false;
        llamaServerStatus = 'idle';
        llamaCurrentModelId = '';
      }
    } else if (selectedModelId.startsWith('llamacpp:')) {
      // 비동기로 서버 준비 (선제적 시작)
      startLlamaIfNeeded(selectedModelId).catch(e => addLog(`[llama.cpp] 자동 시작 실패: ${e}`));
    } else if (selectedModelId.startsWith('gemini:')) {
      // llama 서버가 떠 있으면 종료
      if (llamaServerRunning) {
        try { await invoke('stop_llama_server'); } catch {}
        llamaServerRunning = false;
        llamaServerStatus = 'idle';
        llamaCurrentModelId = '';
      }
    }
  }

  onMount(() => {
    ollamaModelName = localStorage.getItem('ollamaModelName_image') || localStorage.getItem('ollamaModelName') || 'gemma4:e4b';
    selectedModelId = localStorage.getItem('selectedModelId_image') || localStorage.getItem('selectedModelId') || 'gemini:gemini-2.5-flash';
    llamaServerPath = localStorage.getItem('llamaServerPath') || '';
    geminiApiKey = localStorage.getItem('geminiApiKey') || '';
    totalImagesProcessed = parseInt(localStorage.getItem('totalImagesProcessed') || '0', 10);
    
    // Svelte select가 바인딩을 리셋하지 않도록, 저장된 모델이 있으면 availableModels에 미리 추가해둠
    if (selectedModelId && selectedModelId.startsWith('ollama:')) {
      const savedModelName = selectedModelId.replace('ollama:', '');
      availableModels = [{ name: savedModelName }];
    }
    
    addLog("앱 초기화 완료. 현재 모델: " + selectedModelId);
    updateAvailability();


    initOllama();

    // 앱 실행 시 이미 llama.cpp 모델이 선택되어 있으면 자동 실행 시도
    if (selectedModelId.startsWith('llamacpp:')) {
      startLlamaIfNeeded(selectedModelId).catch(e => addLog(`[llama.cpp] 초기 자동 시작 실패: ${e}`));
    }

    let progressUnsub: (() => void) | null = null;
    let completeUnsub: (() => void) | null = null;
    let errorUnsub: (() => void) | null = null;

    listen<any>('download-progress', (event) => {
      const payload = event.payload;
      if (selectedModelId.startsWith('llamacpp:')) {
        const preset = llamacppPresets.find(p => p.id === selectedModelId);
        if (preset && preset.modelName === payload.model_id) {
          downloadProgress = Math.round(payload.percentage);
        }
      }
    }).then(unsub => progressUnsub = unsub);

    listen<string>('download-complete', (event) => {
      const modelId = event.payload;
      addLog(`[llama.cpp] 모델 다운로드 완료: ${modelId}`);
      if (selectedModelId.startsWith('llamacpp:')) {
        const preset = llamacppPresets.find(p => p.id === selectedModelId);
        if (preset && preset.modelName === modelId) {
          downloadProgress = 100;
          // 다운로드 완료되었으므로 자동으로 서버 구동 재시도
          startLlamaIfNeeded(selectedModelId).catch(e => addLog(`[llama.cpp] 다운로드 후 자동 시작 실패: ${e}`));
        }
      }
    }).then(unsub => completeUnsub = unsub);

    listen<[string, string]>('download-error', (event) => {
      const [modelId, err] = event.payload;
      addLog(`[llama.cpp] 모델 다운로드 실패 (${modelId}): ${err}`);
      if (selectedModelId.startsWith('llamacpp:')) {
        const preset = llamacppPresets.find(p => p.id === selectedModelId);
        if (preset && preset.modelName === modelId) {
          llamaServerStatus = 'error';
        }
      }
    }).then(unsub => errorUnsub = unsub);

    return () => {
      if (progressUnsub) progressUnsub();
      if (completeUnsub) completeUnsub();
      if (errorUnsub) errorUnsub();
    };
  });

  function saveModelName() {
    localStorage.setItem('ollamaModelName_image', ollamaModelName);
    addLog("모델명 변경됨: " + ollamaModelName);
  }

  async function openFolder() {
    if (isProcessing) {
      alert("번역 처리 중에는 다른 폴더를 열 수 없습니다.");
      return;
    }
    const selected = await open({
      directory: true,
      multiple: false
    });

    if (selected && typeof selected === 'string') {
      selectedFolder = selected;
      await loadImages(selected);
    }
  }

  async function openSelectedFolder() {
    if (!selectedFolder) return;
    try {
      await invoke('open_folder', { path: selectedFolder });
    } catch (e) {
      alert("폴더를 열 수 없습니다: " + e);
    }
  }

  export async function handleFileOrFolderSelect(path: string) {
    if (isProcessing) {
      alert("번역 처리 중에는 파일을 변경할 수 없습니다.");
      return;
    }

    try {
      const isDir: boolean = await invoke('is_directory', { path });
      if (isDir) {
        selectedFolder = path;
        await loadImages(path);
        return;
      }
    } catch (err) {
      console.error("is_directory check failed", err);
    }

    const lowerPath = path.toLowerCase();
    const isImage = lowerPath.endsWith('.png') || lowerPath.endsWith('.jpg') || lowerPath.endsWith('.jpeg') || lowerPath.endsWith('.bmp');

    if (!isImage) {
      alert("이미지 파일(.png, .jpg, .jpeg, .bmp) 또는 이미지 폴더만 선택할 수 있습니다.");
      return;
    }

    const lastSlash = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
    selectedFolder = lastSlash !== -1 ? path.substring(0, lastSlash) : path;
    await loadImages(path);
  }

  async function loadTranslationMap(folder: string) {
    if (!folder) {
      translationMap = {};
      return;
    }
    const mapPath = folder + '/translation_map.json';
    try {
      const exists: boolean = await invoke('check_file_exists', { path: mapPath });
      if (exists) {
        const content: string = await invoke('read_staged_json', { path: mapPath });
        translationMap = JSON.parse(content) || {};
        addLog(`로컬 치환 사전 로드 완료: ${Object.keys(translationMap).length}개 단어`);
      } else {
        const defaultMap = {
          "未所持": "미소지",
          "設定": "설정",
          "決定": "결정",
          "戻る": "돌아가기",
          "確認": "확인"
        };
        const content = JSON.stringify(defaultMap, null, 2);
        await invoke('save_staged_json', { path: mapPath, content });
        translationMap = defaultMap;
        addLog("로컬 치환 사전이 없어 기본 사전을 자동 생성했습니다.");
      }
    } catch (e) {
      console.error("Failed to load translation map", e);
      addLog("로컬 치환 사전 로드 실패: " + String(e));
      translationMap = {};
    }
  }

  async function loadImages(folder: string) {
    let folderPath = folder;
    try {
      const isDir: boolean = await invoke('is_directory', { path: folder });
      if (!isDir) {
        const lastSlash = Math.max(folder.lastIndexOf('/'), folder.lastIndexOf('\\'));
        folderPath = lastSlash !== -1 ? folder.substring(0, lastSlash) : folder;
      }
    } catch (e) {}

    try {
      const paths: string[] = await invoke('get_images_in_folder', { folderPath: folderPath });
      images = paths
        .filter(path => {
          const lower = path.toLowerCase();
          return !lower.includes('-translated') && !lower.includes('_translated');
        })
        .map(path => {
          const name = path.split('\\').pop()?.split('/').pop() || path;
          return {
            path,
            name,
            regions: [],
            status: 'pending'
          };
        });
      selectedImage = null;
      addLog(`${images.length}개의 이미지를 불러왔습니다.`);
      await loadTranslationMap(folderPath);
    } catch (e) {
      alert("Failed to load images: " + e);
      addLog("이미지 로드 실패: " + String(e));
    }
  }

  function bufferToBase64(buffer: number[]): string {
    const bytes = new Uint8Array(buffer);
    let binary = '';
    const len = bytes.byteLength;
    for (let i = 0; i < len; i++) {
        binary += String.fromCharCode(bytes[i]);
    }
    return window.btoa(binary);
  }

  async function upscaleBase64IfNeeded(base64Data: string): Promise<string> {
    return new Promise((resolve) => {
      const img = new Image();
      img.onload = () => {
        if (img.width >= 1024 || img.height >= 1024) {
          resolve(base64Data);
          return;
        }

        const minDim = Math.min(img.width, img.height);
        if (minDim <= 0) {
          resolve(base64Data);
          return;
        }

        const scale = Math.max(2, Math.min(8, Math.ceil(1024 / minDim)));
        const canvas = document.createElement('canvas');
        canvas.width = img.width * scale;
        canvas.height = img.height * scale;
        const ctx = canvas.getContext('2d');
        if (!ctx) {
          resolve(base64Data);
          return;
        }

        ctx.imageSmoothingEnabled = false;
        ctx.drawImage(img, 0, 0, canvas.width, canvas.height);
        
        const dataUrl = canvas.toDataURL('image/png');
        resolve(dataUrl.split(',')[1]);
      };
      img.onerror = () => {
        resolve(base64Data);
      };
      img.src = 'data:image/png;base64,' + base64Data;
    });
  }

  async function processImageOllama(img: ImageEntry) {
    if (!ollamaModelName && !selectedModelId.startsWith('llamacpp:')) {
      alert("모델을 선택해주세요!");
      return;
    }

    img.status = 'processing';
    images = [...images];
    if (selectedImage?.path === img.path) selectedImage = img;

    try {
      addLog(`[${img.name}] 1/3 로컬 파일 읽는 중...`);
      const bytes: number[] = await invoke('read_image_file', { path: img.path });
      const base64Data = bufferToBase64(bytes);

      addLog(`[${img.name}] OCR 인식률 개선을 위한 이미지 전처리 중...`);
      const vlmBase64 = await upscaleBase64IfNeeded(base64Data);

      const prompt = `Context Image Filename: ${img.name}

Task: Translate Japanese game UI text in the image to Korean.
1. Read ALL Japanese text visually present in the image.
2. Translate the Japanese text into fluent Korean. DO NOT copy the Japanese text. You MUST translate it.
3. Find the bounding box coordinates [ymin, xmin, ymax, xmax] of the visible text normalized to 0-1000 scale.
4. Return ONLY a valid JSON array.

Example output format:
[
  {
    "ja_text": "未所持",
    "ko_text": "미보유",
    "box_2d": [750, 400, 800, 600]
  }
]

CRITICAL: "ko_text" MUST be in Korean (한국어). Do not leave it in Japanese.
Output ONLY the JSON array without any markdown or conversational text.`;

      const modelLabel = selectedModelId.startsWith('llamacpp:') 
        ? selectedModelId 
        : (selectedModelId.startsWith('gemini:') ? `Gemini(${selectedModelId.replace('gemini:', '')})` : `Ollama(${ollamaModelName})`);
      addLog(`[${img.name}] 2/3 AI(${modelLabel})로 번역 요청...`);
      const startTime = performance.now();

      if (selectedModelId.startsWith('llamacpp:')) {
        await startLlamaIfNeeded(selectedModelId);
      }
      
      let response;
      if (selectedModelId.startsWith('llamacpp:')) {
        response = await fetch("http://127.0.0.1:8080/completion", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({
            prompt: prompt,
            image_data: [
              {
                data: vlmBase64,
                id: 1
              }
            ],
            n_predict: 1024,
            stream: false
          })
        });
      } else if (selectedModelId.startsWith('gemini:')) {
        const modelName = selectedModelId.replace('gemini:', '');
        const apiKey = geminiApiKey || localStorage.getItem('geminiApiKey') || '';
        if (!apiKey) {
          throw new Error("Gemini API Key가 설정되지 않았습니다. 모델 드롭다운 우측 입력창에 API Key를 등록해 주세요.");
        }
        response = await fetch(`https://generativelanguage.googleapis.com/v1beta/models/${modelName}:generateContent?key=${apiKey}`, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({
            contents: [
              {
                parts: [
                  { text: prompt },
                  {
                    inlineData: {
                      mimeType: "image/png",
                      data: vlmBase64
                    }
                  }
                ]
              }
            ],
            generationConfig: {
              responseMimeType: "application/json",
              responseSchema: {
                type: "ARRAY",
                items: {
                  type: "OBJECT",
                  properties: {
                    ja_text: {
                      type: "STRING",
                      description: "Original Japanese text extracted from the image"
                    },
                    ko_text: {
                      type: "STRING",
                      description: "Korean translation of the Japanese text"
                    },
                    box_2d: {
                      type: "ARRAY",
                      description: "Bounding box coordinates [ymin, xmin, ymax, xmax] strictly enclosing the original Japanese text, normalized to 0-1000",
                      items: {
                        type: "INTEGER"
                      }
                    }
                  },
                  required: ["ja_text", "ko_text", "box_2d"]
                }
              },
              temperature: 0.1
            }
          })
        });
      } else {
        response = await fetch("http://127.0.0.1:11434/api/generate", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({
            model: ollamaModelName,
            system: "You are an expert OCR and Japanese to Korean translator. Extract text and translate it into fluent Korean. Output ONLY valid JSON array without markdown blocks.",
            prompt: prompt,
            images: [vlmBase64],
            options: {
              num_ctx: 4096,
              num_predict: 1024,
              temperature: 0.0
            },
            stream: false,
            format: "json"
          })
        });
      }

      if (!response.ok) {
        throw new Error(`API 통신 실패: ${response.status} ${response.statusText}`);
      }

      const result = await response.json();
      const endTime = performance.now();
      let responseText = "";
      if (selectedModelId.startsWith('gemini:')) {
        responseText = result.candidates?.[0]?.content?.parts?.[0]?.text || "";
      } else {
        responseText = selectedModelId.startsWith('llamacpp:')
          ? (result.content || "")
          : (result.response || "");
      }

      addLog(`[${img.name}] 응답 수신 완료 (${((endTime-startTime)/1000).toFixed(1)}초). 원시 응답 텍스트:\n${responseText}`);
      
      let parsedRegions: OcrTextRegion[] = [];
      try {
        let jsonParsed: any = null;
        try {
          const jsonMatch = responseText.match(/\[[\s\S]*\]|\{[\s\S]*\}/);
          const cleanedText = jsonMatch ? jsonMatch[0] : responseText;
          jsonParsed = JSON.parse(cleanedText);
        } catch (initialErr) {
          console.warn("Initial JSON parse failed, attempting fallback extraction.", initialErr);
          let extractedObjects: any[] = [];
          let depth = 0;
          let start = -1;
          let inString = false;
          let escape = false;

          for (let i = 0; i < responseText.length; i++) {
            const char = responseText[i];
            if (escape) { escape = false; continue; }
            if (char === '\\') { escape = true; continue; }
            if (char === '"') { inString = !inString; continue; }
            
            if (!inString) {
              if (char === '{') {
                if (depth === 0) start = i;
                depth++;
              } else if (char === '}') {
                depth--;
                if (depth === 0 && start !== -1) {
                  try {
                    const obj = JSON.parse(responseText.substring(start, i + 1));
                    extractedObjects.push(obj);
                  } catch (e) {}
                  start = -1;
                } else if (depth < 0) {
                  depth = 0;
                }
              }
            }
          }
          if (extractedObjects.length > 0) {
            jsonParsed = extractedObjects;
          } else {
            throw initialErr;
          }
        }
        
        if (!Array.isArray(jsonParsed) && typeof jsonParsed === 'object' && jsonParsed !== null) {
          jsonParsed = [jsonParsed];
        }
        
        if (Array.isArray(jsonParsed)) {
            parsedRegions = jsonParsed
              .filter((item: any) => item && Array.isArray(item.box_2d) && item.box_2d.length === 4)
              .map((item: any) => {
                 let [ymin, xmin, ymax, xmax] = item.box_2d;
                 
                 ymin = Number(ymin); xmin = Number(xmin);
                 ymax = Number(ymax); xmax = Number(xmax);

                 const jaText = item.ja_text || "";
                 let koText = item.ko_text || "";

                 // 로컬 치환 사전에 존재하는 원문인 경우 번역문을 덮어씌움
                 for (const key of Object.keys(translationMap)) {
                   if (jaText.includes(key)) {
                     koText = translationMap[key];
                     addLog(`[치환 사전 적용] "${jaText}" ➔ "${koText}" (키워드: "${key}")`);
                     break;
                   }
                 }

                 return {
                   ja_text: jaText,
                   ko_text: koText,
                   x: (xmin / 1000) * 100,
                   y: (ymin / 1000) * 100,
                   w: ((xmax - xmin) / 1000) * 100,
                   h: ((ymax - ymin) / 1000) * 100
                 };
              });
            addLog(`[${img.name}] JSON 파싱 성공! (복구 포함) ${parsedRegions.length}개 텍스트 찾음.`);
            console.log("[Parsed Regions]", parsedRegions);
        }
      } catch(parseErr) {
        console.error("JSON parsing error", responseText);
        addLog(`[${img.name}] JSON 파싱 실패! 에러: ` + String(parseErr));
        throw new Error("AI 모델이 올바른 JSON 규격(box_2d 포함)을 반환하지 않았습니다.\n\n[모델 원시 응답]\n" + responseText);
      }

      img.regions = parsedRegions;

      const dotIndex = img.path.lastIndexOf('.');
      const outputExt = dotIndex > -1 ? img.path.substring(dotIndex) : '.png';
      const outputBase = dotIndex > -1 ? img.path.substring(0, dotIndex) : img.path;
      const outputPath = `${outputBase}-translated${outputExt}`;

      if (parsedRegions.length === 0) {
        addLog(`[${img.name}] 감지된 텍스트가 없어 원본 이미지를 그대로 복사합니다.`);
        await invoke('copy_file', { src: img.path, dest: outputPath });
        img.translatedPath = outputPath;
        delete imageCache[outputPath];
        img.status = 'done';
        totalImagesProcessed++;
        localStorage.setItem('totalImagesProcessed', totalImagesProcessed.toString());
      } else {
        addLog(`[${img.name}] 3/3 번역 결과를 물리 이미지로 합성 및 저장하는 중...`);
        await invoke('draw_and_save_image', {
          originalPath: img.path,
          outputPath: outputPath,
          regions: parsedRegions
        });

        img.translatedPath = outputPath;
        delete imageCache[outputPath];
        img.status = 'done';
        totalImagesProcessed++;
        localStorage.setItem('totalImagesProcessed', totalImagesProcessed.toString());
        addLog(`[${img.name}] 완료! 번역 이미지 저장됨: ${outputPath}`);
      }

      if (selectedFolder) {
        invoke('save_staged_json', {
          path: selectedFolder + '/debug_image_log.json',
          content: JSON.stringify({
            imageName: img.name,
            responseText: responseText,
            parsedRegions: parsedRegions,
            debugLogs: debugLogs
          }, null, 2)
        }).catch(e => console.error(e));
      }
      
      
    } catch (e: any) {
      img.status = 'error';
      img.errorMsg = String(e);
      console.error(e);
      addLog(`[${img.name}] 에러 발생: ` + String(e));
    } finally {
      images = [...images];
      if (selectedImage?.path === img.path) selectedImage = img;
    }
  }

  async function handleBatchTranslate() {
    const pendingImages = images.filter(i => i.status === 'pending' || i.status === 'error');
    if (pendingImages.length === 0) return;

    isProcessing = true;
    cancelRequested = false;
    batchTotal = pendingImages.length;
    batchCompleted = 0;

    try {
      await invoke('prevent_sleep');

      if (selectedModelId.startsWith('llamacpp:')) {
        await startLlamaIfNeeded(selectedModelId);
      }

      const savedConcurrency = localStorage.getItem('translationConcurrency');
      const concurrency = savedConcurrency ? (parseInt(savedConcurrency, 10) || 1) : 1;

      let currentImageIndex = 0;

      const worker = async () => {
        while (currentImageIndex < pendingImages.length && !cancelRequested) {
          const imgIndex = currentImageIndex++;
          if (imgIndex >= pendingImages.length) break;
          const img = pendingImages[imgIndex];
          if (!img) break;

          try {
            await processImageOllama(img);
            batchCompleted++;
          } catch (e) {
            console.error(`Error processing image ${img.name}`, e);
          }
        }
      };

      const workers = Array(Math.min(concurrency, pendingImages.length))
        .fill(null)
        .map(() => worker());

      await Promise.all(workers);

    } catch (e) {
      console.error("Batch image translation error", e);
    } finally {
      await invoke('allow_sleep');
      isProcessing = false;
      batchTotal = 0;
      batchCompleted = 0;
      if (cancelRequested) {
        addLog("이미지 일괄 번역이 중단되었습니다.");
        alert("이미지 일괄 번역이 중단되었습니다.");
      } else {
        addLog("이미지 일괄 번역이 완료되었습니다!");
        alert("이미지 일괄 번역이 완료되었습니다!");
      }
      cancelRequested = false;

      // 일괄 번역 완료 후에도 llama-server를 자동으로 종료하지 않고 백그라운드에 유지하여
      // 다음 연속 번역 요청 및 스크립트 번역기 탭에서의 재가동 시간을 단축시킵니다.
    }
  }


  function getStatusLabel(status: string) {
    switch(status) {
      case 'pending': return '대기 중';
      case 'processing': return '로컬 AI 렌더링 중...';
      case 'done': return '완료';
      case 'error': return '오류 발생';
      default: return status;
    }
  }

  function selectImage(img: ImageEntry) {
    selectedImage = img;
  }

  const imageCache: Record<string, string> = {};

  async function getImageUrl(path: string): Promise<string> {
    if (imageCache[path]) return imageCache[path];
    try {
      const bytes: number[] = await invoke('read_image_file', { path });
      const uint8 = new Uint8Array(bytes);
      let mime = 'image/png';
      if (path.toLowerCase().endsWith('.jpg') || path.toLowerCase().endsWith('.jpeg')) mime = 'image/jpeg';
      else if (path.toLowerCase().endsWith('.bmp')) mime = 'image/bmp';
      
      const blob = new Blob([uint8], { type: mime });
      const url = URL.createObjectURL(blob);
      imageCache[path] = url;
      return url;
    } catch (e) {
      console.error("Failed to load image:", e);
      return "";
    }
  }

  export function getStatus() {
    return { isProcessing };
  }
</script>

<div class="image-translator-container" class:drag-active={isDragging}>
  {#if isDragging}
    <div class="drag-overlay">
      <h2>Drop Image files or Folders here</h2>
    </div>
  {/if}
  <header class="glass-panel header">
    <div class="api-section" style="display: flex; align-items: center; gap: 0.5rem;">
      <label for="model-selector-image" style="font-weight: 500; color: var(--text-secondary); white-space: nowrap; margin: 0;">모델:</label>
      <select id="model-selector-image" bind:value={selectedModelId} onchange={handleModelChange} class="api-input" style="cursor: pointer;" disabled={isProcessing}>
        <optgroup label="Cloud API (추천)">
          <option value="gemini:gemini-2.5-flash">Gemini 2.5 Flash (초고화질 OCR)</option>
          <option value="gemini:gemini-1.5-flash">Gemini 1.5 Flash (초고화질 OCR)</option>
          <option value="gemini:gemini-2.5-pro">Gemini 2.5 Pro (전문가급 OCR)</option>
        </optgroup>
        <optgroup label="llama.cpp">
          {#each llamacppPresets as preset}
            <option value={preset.id}>{preset.name}</option>
          {/each}
        </optgroup>
        {#if availableModels.length > 0}
          <optgroup label="Ollama">
            {#each availableModels as model}
              <option value={'ollama:' + model.name}>Ollama ({model.name})</option>
            {/each}
          </optgroup>
        {/if}
      </select>

      {#if selectedModelId.startsWith('gemini:')}
        <input
          type="password"
          placeholder="Gemini API Key 입력"
          bind:value={geminiApiKey}
          onchange={saveGeminiApiKey}
          class="api-input"
          style="width: 160px; font-size: 0.8rem; padding: 0.55rem 0.75rem;"
          disabled={isProcessing}
        />
      {/if}

      <!-- 신호등 상태 -->
      {#if selectedModelId.startsWith('llamacpp:')}
        <div class="llama-dot-wrap">
          <div
            class="llama-dot"
            class:llama-dot-ok={llamaServerStatus === 'ready'}
            class:llama-dot-warn={llamaServerStatus === 'starting'}
            class:llama-dot-download={llamaServerStatus === 'downloading'}
            class:llama-dot-err={llamaServerStatus === 'idle' || llamaServerStatus === 'error'}
            onclick={selectLlamaServerPath}
            role="button"
            tabindex="0"
            onkeydown={(e) => e.key === 'Enter' && selectLlamaServerPath()}
          ></div>
          <div class="llama-tooltip"
            class:llama-tooltip-ok={llamaServerStatus === 'ready'}
            class:llama-tooltip-warn={llamaServerStatus === 'starting'}
            class:llama-tooltip-download={llamaServerStatus === 'downloading'}
            class:llama-tooltip-err={llamaServerStatus === 'idle' || llamaServerStatus === 'error'}
          >
            <div class="llama-tooltip-row">
              <span class="llama-tooltip-key">상태</span>
              <span class="llama-tooltip-val">
                {#if llamaServerStatus === 'ready'}✅ 서버 준비 완료
                {:else if llamaServerStatus === 'starting'}⏳ 모델 로딩 중...
                {:else if llamaServerStatus === 'downloading'}📥 다운로드 중 ({downloadProgress}%)
                {:else if llamaServerStatus === 'error'}❌ 시작 실패
                {:else}⚪ 미시작
                {/if}
              </span>
            </div>
            {#if llamaServerStatus === 'downloading'}
              <div class="llama-download-progress-bar-wrap">
                <div class="llama-download-progress-bar" style:width="{downloadProgress}%"></div>
              </div>
            {/if}
            <div class="llama-tooltip-row">
              <span class="llama-tooltip-key">경로</span>
              <span class="llama-tooltip-val llama-tooltip-path">{llamaServerPath || '미설정 (시스템 PATH 탐색)'}</span>
            </div>
            {#if !isLlamaServerAvailable}
              <div class="llama-tooltip-hint">클릭하여 llama-server 경로 지정</div>
            {:else if llamaServerStatus === 'downloading'}
              <div class="llama-tooltip-hint">다운로드 완료 대기 중...</div>
            {:else if llamaServerStatus !== 'ready'}
              <div class="llama-tooltip-hint">모델 선택 시 자동 시작됩니다</div>
            {:else}
              <div class="llama-tooltip-hint">클릭하여 경로 재지정</div>
            {/if}
          </div>
        </div>
      {:else}
        <!-- Ollama 신호등 -->
        <div
          class="status-dot"
          title={ollamaStatus === 'online' ? 'Ollama 서버 정상 연결됨 (클릭하여 재연결 시도)' : (ollamaStatus === 'checking' ? 'Ollama 서버 확인 중...' : 'Ollama 서버 연결 실패 (클릭하여 재연결 시도)')}
          style="width: 10px; height: 10px; border-radius: 50%; flex-shrink: 0; cursor: pointer; background-color: {ollamaStatus === 'online' ? '#10b981' : (ollamaStatus === 'checking' ? '#f59e0b' : '#ef4444')}; box-shadow: 0 0 5px {ollamaStatus === 'online' ? 'rgba(16,185,129,0.5)' : (ollamaStatus === 'checking' ? 'rgba(245,158,11,0.5)' : 'rgba(239,68,68,0.5)')}; margin-left: 0.2rem;"
          onclick={initOllama}
          role="button"
          tabindex="0"
          onkeydown={(e) => e.key === 'Enter' && initOllama()}
        ></div>
      {/if}
    </div>

    <div class="header-actions" style="display: flex; align-items: center; gap: 0.8rem;">
      <button class="btn" onclick={openFolder} disabled={isProcessing}>
        이미지 폴더 선택
      </button>

      {#if selectedFolder}
        <button class="btn" style="background-color: rgba(245, 158, 11, 0.2); border: 1px solid rgba(245, 158, 11, 0.4); color: #fbbf24;" onclick={openSelectedFolder}>
          📂 폴더 열기
        </button>
        <span class="folder-path" title={selectedFolder} style="font-size: 0.85rem; color: #94a3b8; max-width: 150px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 500;">
          📂 {selectedFolder.split(/[/\\]/).pop()}
        </span>
      {/if}

      {#if isProcessing && batchTotal > 0}
        <span class="progress-text" style="color: var(--accent-color); font-weight: bold; white-space: nowrap; font-size: 0.9rem;">
          {batchCompleted} / {batchTotal} 완료
        </span>
        <button class="btn btn-danger" style="background-color: #ef4444;" onclick={() => cancelRequested = true}>
          중단하기
        </button>
      {:else}
        <button class="btn btn-success" onclick={handleBatchTranslate} disabled={images.length === 0}>
          이미지 일괄 번역
        </button>
      {/if}
    </div>
  </header>

  <div class="main-workspace">
    <aside class="sidebar image-list glass-panel scrollbar-hidden">
      <h3>이미지 목록 ({images.length})</h3>
      {#if images.length === 0}
        <div class="empty-list">폴더를 선택해주세요.</div>
      {:else}
        <ul>
          {#each images as img}
            <li class="image-item-wrap">
              <button 
                type="button" 
                class="image-item-btn" 
                class:active={selectedImage?.path === img.path} 
                onclick={() => selectImage(img)}
              >
                <div class="item-content">
                  <span class="name" title={img.name}>{img.name}</span>
                  <span class={`status-dot status-${img.status}`} title={getStatusLabel(img.status)}></span>
                </div>
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </aside>

    <section class="preview-area glass-panel">
      {#if selectedImage}
        <div class="preview-header">
          <h2>{selectedImage.name}</h2>
          <div class="header-actions">
            <span class={`status-badge status-${selectedImage.status}`}>
              {getStatusLabel(selectedImage.status)}
            </span>
            <button class="btn-small" onclick={() => { if (selectedImage) processImageOllama(selectedImage); }} disabled={isProcessing || !ollamaModelName}>
              현재 이미지 번역
            </button>
          </div>
        </div>

        <div class="preview-panels">
          <div class="panel original-panel">
            <h3>원본 이미지</h3>
            <div class="img-container">
              {#await getImageUrl(selectedImage.path)}
                <div class="placeholder loading"><div class="spinner"></div><p>이미지 불러오는 중...</p></div>
              {:then url}
                {#if url}
                  <div class="render-wrapper">
                    <img src={url} alt="Original" class="base-img" />
                  </div>
                {:else}
                  <div class="placeholder error">이미지를 불러올 수 없습니다.</div>
                {/if}
              {/await}
            </div>
          </div>

          <div class="panel overlay-panel">
            <h3>번역 완료된 이미지 파일</h3>
            <div class="img-container">
              {#if selectedImage.status === 'done' && selectedImage.translatedPath}
                {#await getImageUrl(selectedImage.translatedPath)}
                  <div class="placeholder loading"><div class="spinner"></div><p>번역 이미지 로드 중...</p></div>
                {:then url}
                  {#if url}
                    <div class="render-wrapper">
                      <img src={url} alt="Translated" class="base-img" />
                    </div>
                  {:else}
                    <div class="placeholder error">저장된 번역 이미지를 찾을 수 없습니다.</div>
                  {/if}
                {/await}
              {:else if selectedImage.status === 'processing'}
                <div class="placeholder loading"><div class="spinner"></div><p>물리 이미지 렌더링 중...</p></div>
              {:else}
                <div class="placeholder">번역을 실행하면 결과 이미지가 표시됩니다.</div>
              {/if}
            </div>
            {#if selectedImage.status === 'error'}
              <div class="error-msg-overlay">
                <div class="error-header">
                  <strong>오류 발생 / 파싱 실패</strong>
                  <button class="btn-small btn-copy" onclick={() => {
                    if (selectedImage) {
                      navigator.clipboard.writeText(selectedImage.errorMsg || '');
                      alert('에러 내용이 복사되었습니다.');
                    }
                  }}>에러 복사</button>
                </div>
                <div class="error-content">{selectedImage.errorMsg}</div>
              </div>
            {/if}
          </div>
        </div>
      {:else}
        <div class="empty-state" style="display: flex; flex-direction: column; gap: 0.5rem; text-align: center;">
          {#if selectedFolder && images.length === 0}
            <span style="font-size: 2rem;">🖼️</span>
            <p style="color: #ef4444; font-weight: 500;">선택한 폴더에 이미지 파일(.png, .jpg, .jpeg)이 존재하지 않습니다.</p>
          {:else}
            <span style="font-size: 2rem;">🖼️</span>
            <p>왼쪽 목록에서 이미지를 선택해주세요.</p>
          {/if}
        </div>
      {/if}

      <!-- 디버그 패널 토글 영역 -->
      <div class="debug-toggle-area" style="text-align: right; padding: 0.5rem;">
        <button class="btn-small" style="background-color: transparent; border: 1px solid #475569; color: #94a3b8;" onclick={() => showDebugPanel = !showDebugPanel}>
          {showDebugPanel ? '디버그 로그 숨기기' : '디버그 로그 보기'}
        </button>
      </div>

      <!-- 디버그 패널 -->
      {#if showDebugPanel}
      <div class="debug-panel glass-panel">
        <div class="debug-header">
          <h3>디버그 로그 (Ollama 통신 과정)</h3>
          <button class="btn-small btn-save" onclick={() => debugLogs = []}>로그 지우기</button>
        </div>
        <div class="debug-content">
          {#if debugLogs.length === 0}
            <div class="log-entry" style="color: #64748b;">로그가 없습니다.</div>
          {:else}
            {#each debugLogs as log}
              <div class="log-entry">{log}</div>
            {/each}
          {/if}
        </div>
      </div>
      {/if}
    </section>
  </div>
</div>

<!-- 기존에 존재하던 다운로드 모달은 시스템 ask 대화상자로 대체되었습니다. -->

<style lang="scss">
  .image-translator-container {
    display: flex;
    flex-direction: column;
    height: 100%;
    position: relative;
    gap: 1rem;
  }

  .drag-overlay {
    position: absolute;
    top: 0; left: 0; right: 0; bottom: 0;
    background: rgba(59, 130, 246, 0.2);
    backdrop-filter: blur(4px);
    z-index: 100;
    display: flex;
    align-items: center;
    justify-content: center;
    border: 4px dashed var(--accent-color);
    border-radius: 12px;
    pointer-events: none;

    h2 {
      font-size: 2rem;
      color: white;
      text-shadow: 0 2px 4px rgba(0,0,0,0.5);
    }
  }

  .header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0.8rem 1.5rem;
    position: relative;
    z-index: 10;
    
    .header-actions {
      display: flex;
      align-items: center;
      gap: 0.8rem;
    }
  }

  .api-section {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .token-tracker {
    display: flex;
    gap: 0.5rem;
    margin-right: 1rem;
    font-size: 0.8rem;
  }

  .badge {
    background: rgba(255, 255, 255, 0.1);
    padding: 0.2rem 0.5rem;
    border-radius: 4px;
    color: #e2e8f0;
  }

  .badge.cost {
    background: rgba(59, 130, 246, 0.2);
    color: #60a5fa;
    font-weight: bold;
    border: 1px solid rgba(96, 165, 250, 0.3);
  }

  .api-input {
    background: rgba(15, 23, 42, 0.6);
    backdrop-filter: blur(8px);
    border: 1px solid rgba(255, 255, 255, 0.15);
    color: #f1f5f9;
    padding: 0.55rem 2rem 0.55rem 1rem;
    border-radius: 6px;
    width: 240px;
    font-size: 0.9rem;
    height: 38px;
    box-sizing: border-box;
    transition: all 0.2s ease;
    appearance: none;
    -webkit-appearance: none;
    -moz-appearance: none;
    background-image: url("data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' width='24' height='24' viewBox='0 0 24 24' fill='none' stroke='%23cbd5e1' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'><polyline points='6 9 12 15 18 9'></polyline></svg>");
    background-repeat: no-repeat;
    background-position: right 12px center;
    background-size: 16px;
    cursor: pointer;

    option {
      background-color: #1e293b;
      color: #f1f5f9;
    }
  }

  .api-input:focus {
    outline: none;
    border-color: var(--accent-color);
    box-shadow: 0 0 0 2px rgba(59, 130, 246, 0.3);
  }

  .folder-path {
    font-size: 0.9rem;
    color: var(--text-secondary);
    max-width: 300px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .progress-text {
    color: var(--accent-color);
    font-weight: bold;
  }

  .main-workspace {
    display: flex;
    flex: 1;
    gap: 1rem;
    min-height: 0;
  }

  .sidebar {
    width: 250px;
    display: flex;
    flex-direction: column;
    overflow-y: auto;
    
    h3 {
      padding: 1rem;
      border-bottom: 1px solid var(--border-color);
      margin: 0;
      font-size: 1rem;
    }

    .empty-list {
      padding: 2rem 1rem;
      text-align: center;
      color: var(--text-secondary);
      font-size: 0.9rem;
    }
    
    ul {
      list-style: none;
      padding: 0.5rem;
      margin: 0;
      
      .image-item-wrap {
        padding: 0;
        margin: 0;
      }
      
      .image-item-btn {
        width: 100%;
        background: transparent;
        border: none;
        color: inherit;
        font: inherit;
        text-align: left;
        padding: 0.75rem 1rem;
        cursor: pointer;
        border-radius: 6px;
        transition: all 0.2s ease;
        display: block;
        margin-bottom: 0.25rem;
        
        .item-content {
          display: flex;
          justify-content: space-between;
          align-items: center;
          gap: 0.5rem;
        }

        .name {
          font-size: 0.85rem;
          white-space: nowrap;
          overflow: hidden;
          text-overflow: ellipsis;
        }
        
        &:hover {
          background: rgba(255, 255, 255, 0.05);
        }
        
        &.active {
          background: rgba(59, 130, 246, 0.2);
          border-left: 3px solid var(--accent-color);
          border-radius: 0 6px 6px 0;
        }
      }
    }
  }

  .status-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;

    &.status-pending { background: #64748b; }
    &.status-processing { background: #3b82f6; box-shadow: 0 0 8px rgba(59, 130, 246, 0.5); }
    &.status-done { background: #10b981; }
    &.status-error { background: #ef4444; }
  }

  .preview-area {
    flex: 1;
    display: flex;
    flex-direction: column;
    padding: 1.5rem;
    overflow: hidden;

    .preview-header {
      display: flex;
      justify-content: space-between;
      align-items: center;
      margin-bottom: 1rem;
      
      h2 { 
        margin: 0;
        font-size: 1.25rem;
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
      }

      .header-actions {
        display: flex;
        align-items: center;
        gap: 1rem;
      }
    }

    .preview-panels {
      display: flex;
      gap: 1rem;
      flex: 1;
      min-height: 0;
    }

    .panel {
      flex: 1;
      display: flex;
      flex-direction: column;
      background: rgba(0, 0, 0, 0.2);
      border-radius: 8px;
      border: 1px solid var(--border-color);
      overflow: hidden;
      position: relative;

      h3 {
        margin: 0;
        padding: 0.75rem 1rem;
        font-size: 1rem;
        background: rgba(15, 23, 42, 0.6);
        border-bottom: 1px solid var(--border-color);
        text-align: center;
        color: var(--text-secondary);
      }

      .img-container {
        flex: 1;
        display: flex;
        align-items: center;
        justify-content: center;
        padding: 1rem;
        overflow: auto;
        position: relative;

        .render-wrapper {
          position: relative;
          display: inline-block;
          line-height: 0;
        }

        .base-img {
          display: block;
          max-width: none;
        }

        .placeholder {
          display: flex;
          flex-direction: column;
          align-items: center;
          justify-content: center;
          color: var(--text-secondary);
          text-align: center;
          gap: 1rem;

          &.error { color: #f87171; }
        }
      }

      .error-msg-overlay {
        position: absolute;
        bottom: 0; left: 0; right: 0;
        background: rgba(239, 68, 68, 0.95);
        color: white;
        padding: 0.75rem;
        font-size: 0.85rem;
        text-align: left;
        max-height: 40%;
        overflow-y: auto;
        display: flex;
        flex-direction: column;
        gap: 0.5rem;
        border-top: 2px solid #b91c1c;

        .error-header {
          display: flex;
          justify-content: space-between;
          align-items: center;
          border-bottom: 1px solid rgba(255, 255, 255, 0.3);
          padding-bottom: 0.25rem;
        }

        .btn-copy {
          background: #7f1d1d;
          padding: 0.2rem 0.5rem;
          font-size: 0.75rem;
          border-radius: 4px;
          &:hover { background: #450a0a; }
        }

        .error-content {
          white-space: pre-wrap;
          word-break: break-all;
          font-family: monospace;
          font-size: 0.8rem;
        }
      }
    }

    .empty-state {
      flex: 1;
      display: flex;
      align-items: center;
      justify-content: center;
      color: var(--text-secondary);
    }

    .debug-panel {
      margin-top: 1rem;
      flex-shrink: 0;
      height: 180px;
      display: flex;
      flex-direction: column;
      background: rgba(0, 0, 0, 0.4);
      border: 1px solid var(--border-color);
      border-radius: 8px;
      overflow: hidden;

      .debug-header {
        display: flex;
        justify-content: space-between;
        align-items: center;
        padding: 0.5rem 1rem;
        background: rgba(15, 23, 42, 0.8);
        border-bottom: 1px solid var(--border-color);
        
        h3 { margin: 0; font-size: 0.9rem; color: #94a3b8; }
      }

      .debug-content {
        flex: 1;
        overflow-y: auto;
        padding: 0.75rem;
        font-family: 'Consolas', 'Courier New', monospace;
        font-size: 0.8rem;
        color: #a7f3d0;
        
        .log-entry {
          margin-bottom: 0.4rem;
          white-space: pre-wrap;
          word-break: break-all;
          border-bottom: 1px dashed rgba(255, 255, 255, 0.1);
          padding-bottom: 0.2rem;
        }
      }
    }
  }

  .status-badge {
    font-size: 0.75rem;
    padding: 0.25rem 0.75rem;
    border-radius: 12px;
    font-weight: 500;
    
    &.status-pending { background: rgba(100, 116, 139, 0.2); color: #cbd5e1; }
    &.status-processing { background: rgba(59, 130, 246, 0.2); color: #60a5fa; }
    &.status-done { background: rgba(16, 185, 129, 0.2); color: #34d399; }
    &.status-error { background: rgba(239, 68, 68, 0.2); color: #f87171; }
  }

  .btn {
    background: var(--accent-color);
    color: white;
    border: none;
    padding: 0.5rem 1rem;
    border-radius: 6px;
    cursor: pointer;
    font-weight: 500;
    transition: background 0.2s;
    &:hover { background: var(--accent-hover); }
    &:disabled { opacity: 0.5; cursor: not-allowed; }
  }

  .btn-success {
    background: #10b981;
    &:hover { background: #059669; }
  }

  .btn-small {
    background: var(--accent-color);
    color: white;
    border: none;
    padding: 0.4rem 0.8rem;
    border-radius: 4px;
    font-size: 0.85rem;
    cursor: pointer;
    transition: background 0.2s;
    &:hover { background: var(--accent-hover); }
    &:disabled { opacity: 0.5; cursor: not-allowed; }
  }

  .btn-save {
    background: #475569;
    &:hover { background: #334155; }
  }

  .spinner {
    width: 30px;
    height: 30px;
    border: 3px solid rgba(59, 130, 246, 0.3);
    border-radius: 50%;
    border-top-color: #3b82f6;
    animation: spin 1s ease-in-out infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }


  .llama-dot {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    flex-shrink: 0;
    cursor: pointer;
    transition: transform 0.15s ease, box-shadow 0.2s ease;
    margin-left: 0.2rem;

    &:hover {
      transform: scale(1.3);
    }
  }

  .llama-dot-ok {
    background-color: #10b981;
    box-shadow: 0 0 6px rgba(16, 185, 129, 0.7);
  }

  .llama-dot-warn {
    background-color: #f59e0b;
    box-shadow: 0 0 6px rgba(245, 158, 11, 0.7);
    animation: pulse-amber 1s ease-in-out infinite;
  }

  .llama-dot-download {
    background-color: #3b82f6;
    box-shadow: 0 0 6px rgba(59, 130, 246, 0.7);
    animation: pulse-blue 1.2s ease-in-out infinite;
  }

  .llama-dot-err {
    background-color: #ef4444;
    box-shadow: 0 0 6px rgba(239, 68, 68, 0.7);
    animation: pulse-red 1.5s ease-in-out infinite;
  }

  @keyframes pulse-amber {
    0%, 100% { box-shadow: 0 0 4px rgba(245, 158, 11, 0.4); }
    50% { box-shadow: 0 0 12px rgba(245, 158, 11, 0.9); }
  }

  @keyframes pulse-blue {
    0%, 100% { box-shadow: 0 0 4px rgba(59, 130, 246, 0.4); }
    50% { box-shadow: 0 0 12px rgba(59, 130, 246, 0.9); }
  }

  @keyframes pulse-red {
    0%, 100% { box-shadow: 0 0 4px rgba(239, 68, 68, 0.5); }
    50% { box-shadow: 0 0 12px rgba(239, 68, 68, 0.9); }
  }

  .llama-dot-wrap {
    position: relative;
    display: inline-flex;
    align-items: center;
    margin-left: 0.2rem;

    &:hover .llama-tooltip {
      opacity: 1;
      transform: translateX(-50%) translateY(0);
      pointer-events: auto;
    }
  }

  .llama-tooltip {
    position: absolute;
    top: calc(100% + 10px);
    left: 50%;
    transform: translateX(-50%) translateY(-4px);
    opacity: 0;
    pointer-events: none;
    transition: opacity 0.18s ease, transform 0.18s ease;
    z-index: 200;
    min-width: 220px;
    padding: 0.6rem 0.75rem;
    border-radius: 8px;
    backdrop-filter: blur(12px);
    font-size: 0.78rem;
    white-space: nowrap;
    box-shadow: 0 4px 16px rgba(0,0,0,0.5);

    /* 말풍선 꼬리 (위쪽) */
    &::after {
      content: '';
      position: absolute;
      bottom: 100%;
      left: 50%;
      transform: translateX(-50%);
      border: 6px solid transparent;
    }

    &.llama-tooltip-ok {
      background: rgba(6, 30, 20, 0.92);
      border: 1px solid rgba(16, 185, 129, 0.4);
      &::after { border-bottom-color: rgba(16, 185, 129, 0.4); }
    }
    &.llama-tooltip-warn {
      background: rgba(30, 20, 6, 0.92);
      border: 1px solid rgba(245, 158, 11, 0.4);
      &::after { border-bottom-color: rgba(245, 158, 11, 0.4); }
    }
    &.llama-tooltip-download {
      background: rgba(6, 18, 30, 0.92);
      border: 1px solid rgba(59, 130, 246, 0.4);
      &::after { border-bottom-color: rgba(59, 130, 246, 0.4); }
    }
    &.llama-tooltip-err {
      background: rgba(30, 6, 6, 0.92);
      border: 1px solid rgba(239, 68, 68, 0.4);
      &::after { border-bottom-color: rgba(239, 68, 68, 0.4); }
    }
  }

  .llama-tooltip-row {
    display: flex;
    align-items: baseline;
    gap: 0.5rem;
    margin-bottom: 0.25rem;
  }

  .llama-tooltip-key {
    font-size: 0.68rem;
    font-weight: 700;
    color: #64748b;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    flex-shrink: 0;
    width: 28px;
  }

  .llama-tooltip-val {
    color: #e2e8f0;
    font-size: 0.78rem;
  }

  .llama-tooltip-path {
    color: #94a3b8;
    font-size: 0.72rem;
    max-width: 180px;
    overflow: hidden;
    text-overflow: ellipsis;
    display: block;
  }

  .llama-tooltip-hint {
    margin-top: 0.4rem;
    padding-top: 0.35rem;
    border-top: 1px solid rgba(255,255,255,0.08);
    font-size: 0.68rem;
    color: #64748b;
    text-align: center;
  }

  .llama-download-progress-bar-wrap {
    width: 100%;
    height: 6px;
    background: rgba(255, 255, 255, 0.1);
    border-radius: 3px;
    overflow: hidden;
    margin: 0.5rem 0;
  }

  .llama-download-progress-bar {
    height: 100%;
    background: #3b82f6;
    transition: width 0.3s ease;
  }
</style>
