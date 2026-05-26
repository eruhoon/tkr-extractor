<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen, TauriEvent } from '@tauri-apps/api/event';
  import { open, save, ask, message } from '@tauri-apps/plugin-dialog';
  import { fetch } from '@tauri-apps/plugin-http';
  import { translateSentenceOllama } from '$lib/translator';

  interface ScriptEntry {
    id: number;
    title: string;
    code: string;
    count?: number;
    translations?: { [key: string]: string };
  }

  interface ExtractedString {
    id: number;
    text: string;
    translatedText: string;
    originalCodeMatch: string;
    errorMsg?: string;
    validationError?: string;
  }

  let { isDragging = false } = $props();

  let originalFilePath = $state('');
  let scripts = $state<ScriptEntry[]>([]);
  let selectedSidebarItem = $state<SidebarItem | null>(null);
  let extractedStrings = $state<ExtractedString[]>([]);

  let isScriptsFile = $derived(
    originalFilePath.toLowerCase().split(/[/\\]/).pop()?.startsWith('scripts') ?? false
  );

  interface SidebarItem {
    id: string | number;
    title: string;
    count: number;
    completedCount: number;
    isGroup: boolean;
    scriptRef?: ScriptEntry;
    groupEntries?: ScriptEntry[];
  }

  let sidebarItems = $derived.by<SidebarItem[]>(() => {
    if (!scripts || scripts.length === 0) return [];

    if (isScriptsFile) {
      return scripts.map(s => {
        const completed = getCompletedCount(s, true);
        return {
          id: s.id,
          title: s.title,
          count: s.count || 0,
          completedCount: completed,
          isGroup: false,
          scriptRef: s
        };
      });
    } else {
      const groupsMap = new Map<string, ScriptEntry[]>();
      for (const s of scripts) {
        const key = getGroupKey(s.title);
        if (!groupsMap.has(key)) {
          groupsMap.set(key, []);
        }
        groupsMap.get(key)!.push(s);
      }

      const items: SidebarItem[] = [];

      // 전체 보기 가상 아이템 추가
      const totalCount = scripts.reduce((acc, curr) => acc + (curr.count || 0), 0);
      const totalCompleted = scripts.reduce((acc, curr) => acc + getCompletedCount(curr, false), 0);
      items.push({
        id: "ALL_ITEMS",
        title: "전체 보기",
        count: totalCount,
        completedCount: totalCompleted,
        isGroup: true,
        groupEntries: scripts
      });

      for (const [key, entries] of groupsMap.entries()) {
        const totalCount = entries.reduce((acc, curr) => acc + (curr.count || 0), 0);
        const completedCount = entries.reduce((acc, curr) => acc + getCompletedCount(curr, false), 0);
        items.push({
          id: key,
          title: key,
          count: totalCount,
          completedCount: completedCount,
          isGroup: true,
          groupEntries: entries
        });
      }
      return items;
    }
  });

  function getCompletedCount(s: ScriptEntry, isScripts: boolean): number {
    if (!s.translations) return 0;
    
    let completed = 0;
    if (isScripts) {
      const jpRegex = /'([^']*[ぁ-んァ-ヶ一-龠]+[^']*)'|"([^"]*[ぁ-んァ-ヶ一-龠]+[^"]*)"/g;
      let match;
      while ((match = jpRegex.exec(s.code)) !== null) {
        const text = match[1] || match[2] || '';
        const trans = s.translations[text];
        if (trans && trans !== "번역 중..." && trans !== "번역 실패" && trans.trim() !== "") {
          completed++;
        }
      }
    } else {
      const trans = s.translations[s.code];
      if (trans && trans !== "번역 중..." && trans !== "번역 실패" && trans.trim() !== "") {
        completed = 1;
      }
    }
    return completed;
  }

  function getGroupKey(title: string): string {
    let key = title.split(' - Line ')[0];
    key = key.replace(/ \[(Name|Description|Profile)\]/gi, '');
    return key;
  }
  let isTranslating = $state(false);
  let cancelRequested = $state(false);
  let translationAbortController = $state<AbortController | null>(null);
  const rowAbortControllers = new Map<number, AbortController>();
  let batchTotal = $state(0);
  let batchCompleted = $state(0);
  let isLoading = $state(false);
  let selectedFolder = $state('');
  let rvdataFiles = $state<string[]>([]);
  let selectedFileInFolder = $state('');

  let ollamaModelName = $state('gemma4:e4b');
  let availableModels = $state<{name: string}[]>([]);
  let ollamaStatus = $state<'online' | 'offline' | 'checking'>('checking');
  let translationMode = $state('ollama'); // 'ollama' | 'gemini'
  let isExtracting = $state(false);
  let showDebugPanel = $state(false);
  let debugLogs = $state<string[]>([]);

  // llama.cpp 관련
  const llamacppPresets = [
    { 
      id: 'llamacpp:gemma-4-E4B-it', 
      name: 'llama.cpp (Gemma-4 E4B-it)', 
      modelName: 'gemma-4-E4B-it',
      downloadUrl: 'https://huggingface.co/bartowski/google_gemma-4-E4B-it-GGUF/resolve/main/google_gemma-4-E4B-it-Q4_K_M.gguf'
    },
    { 
      id: 'llamacpp:gemma-4-E2B-it', 
      name: 'llama.cpp (Gemma-4 E2B-it)', 
      modelName: 'gemma-4-E2B-it',
      downloadUrl: 'https://huggingface.co/bartowski/google_gemma-4-E2B-it-GGUF/resolve/main/google_gemma-4-E2B-it-Q4_K_M.gguf'
    },
  ];
  let selectedModelId = $state('ollama:' + 'gemma4:e4b');
  let llamaServerPath = $state('');
  let isLlamaServerAvailable = $state(false);
  let llamaServerRunning = $state(false);
  // 'idle' | 'starting' | 'downloading' | 'ready' | 'error'
  let llamaServerStatus = $state<'idle' | 'starting' | 'downloading' | 'ready' | 'error'>('idle');
  let llamaCurrentModelId = $state(''); // 현재 서버에 로드된 모델 ID
  let downloadProgress = $state(0); // 다운로드 진행률

  function addLog(msg: string) {
    const time = new Date().toLocaleTimeString();
    debugLogs = [`[${time}] ${msg}`, ...debugLogs];
  }

  function saveModelName() {
    localStorage.setItem('ollamaModelName', ollamaModelName);
    addLog("모델명 변경됨: " + ollamaModelName);
  }

  async function updateLlamaAvailability() {
    try {
      isLlamaServerAvailable = await invoke<boolean>('check_llama_server_availability', { customPath: llamaServerPath || null });
    } catch {
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
      await updateLlamaAvailability();
    }
  }

  /** /health 폴링 — 최대 waitSec초 대기, ready 시 true 반환 */
  async function waitForLlamaReady(waitSec = 120): Promise<boolean> {
    const deadline = Date.now() + waitSec * 1000;
    while (Date.now() < deadline) {
      try {
        const res = await fetch('http://127.0.0.1:8080/health');
        if (res.ok) {
          const data = await res.json().catch(() => ({}));
          if (data.status === 'ok') return true;
          // 'loading model' 등은 계속 대기
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

  /** 모델 드롭다운 변경 시 호출 */
  async function onModelChange() {
    localStorage.setItem('selectedModelId', selectedModelId);
    if (selectedModelId.startsWith('ollama:')) {
      ollamaModelName = selectedModelId.slice(7);
      saveModelName();
      // llama 서버가 떠 있으면 종료
      if (llamaServerRunning) {
        try { await invoke('stop_llama_server'); } catch {}
        llamaServerRunning = false;
        llamaServerStatus = 'idle';
        llamaCurrentModelId = '';
      }
    } else if (selectedModelId.startsWith('llamacpp:')) {
      // 비동기로 서버 준비 (번역 시에도 waitForLlamaReady 호출하므로 선제적 시작)
      startLlamaIfNeeded(selectedModelId).catch(e => addLog(`[llama.cpp] 자동 시작 실패: ${e}`));
    }
  }


  onMount(() => {
    ollamaModelName = localStorage.getItem('ollamaModelName') || 'gemma4:e4b';
    llamaServerPath = localStorage.getItem('llamaServerPath') || '';
    const savedModel = localStorage.getItem('selectedModelId');
    if (savedModel) selectedModelId = savedModel;
    addLog("앱 초기화 완료. 모델명: " + ollamaModelName);



    const initOllama = async () => {
      try {
        const res = await fetch("http://127.0.0.1:11434/api/tags");
        if (res.ok) {
          const data = await res.json();
          availableModels = data.models || [];
          if (availableModels.length > 0 && !localStorage.getItem('ollamaModelName')) {
            ollamaModelName = availableModels[0].name;
            localStorage.setItem('ollamaModelName', ollamaModelName);
          }
          ollamaStatus = 'online';
        } else {
          ollamaStatus = 'offline';
        }
      } catch (e) {
        addLog("Ollama 서버 연결 실패: 로컬 서버가 켜져 있는지 확인하세요.");
        ollamaStatus = 'offline';
      }
    };

    initOllama();
    updateLlamaAvailability();

    // 앱 실행 시 이미 llama.cpp 모델이 선택되어 있으면 자동 실행 시도
    if (selectedModelId.startsWith('llamacpp:')) {
      startLlamaIfNeeded(selectedModelId).catch(e => addLog(`[llama.cpp] 초기 자동 시작 실패: ${e}`));
    }

    // Tauri 다운로드 이벤트 수신 등록
    let unlistenProgress: (() => void) | null = null;
    let unlistenComplete: (() => void) | null = null;
    let unlistenError: (() => void) | null = null;

    listen<any>('download-progress', (event) => {
      const payload = event.payload;
      if (selectedModelId.startsWith('llamacpp:')) {
        const preset = llamacppPresets.find(p => p.id === selectedModelId);
        if (preset && preset.modelName === payload.model_id) {
          downloadProgress = Math.round(payload.percentage);
        }
      }
    }).then(fn => { unlistenProgress = fn; });

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
    }).then(fn => { unlistenComplete = fn; });

    listen<[string, string]>('download-error', (event) => {
      const [modelId, errMsg] = event.payload;
      addLog(`[llama.cpp] 모델 다운로드 실패 (${modelId}): ${errMsg}`);
      if (selectedModelId.startsWith('llamacpp:')) {
        const preset = llamacppPresets.find(p => p.id === selectedModelId);
        if (preset && preset.modelName === modelId) {
          llamaServerStatus = 'error';
        }
      }
    }).then(fn => { unlistenError = fn; });

    return () => {
      if (unlistenProgress) unlistenProgress();
      if (unlistenComplete) unlistenComplete();
      if (unlistenError) unlistenError();
    };
  });

  let cacheData = $state<Record<string, string>>({});
  let glossaryData = $state<Record<string, string>>({});
  let showGlossaryPanel = $state(false);
  let newGlossaryJp = $state('');
  let newGlossaryKo = $state('');
  let stagedFilesMap = $state<Record<string, 'working' | 'completed' | 'none'>>({});

  async function checkFileCompletion(filePath: string): Promise<boolean> {
    const filename = filePath.split(/[/\\]/).pop() || '';
    const isScripts = filename.toLowerCase().startsWith('scripts');
    const stagedPath = filePath.replace(/\.(rvdata2?)$/i, '_staged.json');
    
    try {
      // 1. 원본 데이터 파싱 및 총 번역 대상 개수 계산
      const loaded: ScriptEntry[] = await invoke('parse_rvdata', { path: filePath });
      let totalCount = 0;
      
      for (const s of loaded) {
        if (isScripts) {
          const jpRegex = /'([^']*[ぁ-んァ-ヶ一-龠]+[^']*)'|"([^"]*[ぁ-んァ-ヶ一-龠]+[^"]*)"/g;
          const matches = s.code.match(jpRegex);
          if (matches) totalCount += matches.length;
        } else {
          if (s.code) totalCount += 1;
        }
      }
      
      if (totalCount === 0) return true; // 번역할 문장이 없으면 완료로 취급
      
      // 2. staged json 존재 여부 체크 및 번역된 개수 계산
      const exists: boolean = await invoke('check_file_exists', { path: stagedPath });
      if (!exists) return false;

      const jsonContent: string = await invoke('read_staged_json', { path: stagedPath });
      const stagedData = JSON.parse(jsonContent) || {};
      
      let completedCount = 0;
      for (const s of loaded) {
        const transMap = stagedData[s.id];
        if (!transMap) continue;
        
        if (isScripts) {
          const jpRegex = /'([^']*[ぁ-んァ-ヶ一-龠]+[^']*)'|"([^"]*[ぁ-んァ-ヶ一-龠]+[^"]*)"/g;
          let match;
          while ((match = jpRegex.exec(s.code)) !== null) {
            const text = match[1] || match[2] || '';
            const trans = transMap[text];
            if (trans && trans !== "번역 중..." && trans !== "번역 실패" && trans.trim() !== "") {
              completedCount++;
            }
          }
        } else {
          const trans = transMap[s.code];
          if (trans && trans !== "번역 중..." && trans !== "번역 실패" && trans.trim() !== "") {
            completedCount++;
          }
        }
      }
      
      return completedCount >= totalCount;
    } catch (e) {
      console.error("checkFileCompletion error", e);
      return false;
    }
  }

  async function updateStagedFilesMap() {
    if (!selectedFolder || rvdataFiles.length === 0) {
      stagedFilesMap = {};
      return;
    }
    const newMap: Record<string, 'working' | 'completed' | 'none'> = {};
    await Promise.all(
      rvdataFiles.map(async (filePath) => {
        const stagedPath = filePath.replace(/\.(rvdata2?)$/i, '_staged.json');
        try {
          const exists: boolean = await invoke('check_file_exists', { path: stagedPath });
          if (exists) {
            newMap[filePath] = 'working';
          } else {
            newMap[filePath] = 'none';
          }
        } catch (e) {
          newMap[filePath] = 'none';
        }
      })
    );
    stagedFilesMap = newMap;

    // 병렬로 완료 상태 체크 및 업데이트
    await Promise.all(
      rvdataFiles.map(async (filePath) => {
        if (stagedFilesMap[filePath] === 'working') {
          const isCompleted = await checkFileCompletion(filePath);
          if (isCompleted) {
            stagedFilesMap[filePath] = 'completed';
          }
        }
      })
    );
  }

  async function loadCache() {
    if (!selectedFolder) {
      cacheData = {};
      return;
    }
    const cachePath = selectedFolder + '/cache.json';
    try {
      const exists: boolean = await invoke('check_file_exists', { path: cachePath });
      if (exists) {
        const content: string = await invoke('read_staged_json', { path: cachePath });
        cacheData = JSON.parse(content) || {};
        addLog(`번역 캐시 로드 완료: ${Object.keys(cacheData).length}개 문장`);
      } else {
        cacheData = {};
      }
    } catch (e) {
      console.error("Failed to load cache", e);
      addLog("번역 캐시 로드 실패: " + String(e));
      cacheData = {};
    }
  }

  async function saveCache() {
    if (!selectedFolder) return;
    const cachePath = selectedFolder + '/cache.json';
    try {
      const content = JSON.stringify(cacheData, null, 2);
      await invoke('save_staged_json', { path: cachePath, content });
    } catch (e) {
      console.error("Failed to save cache", e);
      addLog("번역 캐시 저장 실패: " + String(e));
    }
  }

  async function loadGlossary() {
    if (!selectedFolder) {
      glossaryData = {};
      return;
    }
    const glossaryPath = selectedFolder + '/glossary.json';
    try {
      const exists: boolean = await invoke('check_file_exists', { path: glossaryPath });
      if (exists) {
        const content: string = await invoke('read_staged_json', { path: glossaryPath });
        glossaryData = JSON.parse(content) || {};
        addLog(`글로서리 로드 완료: ${Object.keys(glossaryData).length}개 단어`);
      } else {
        glossaryData = {};
      }
    } catch (e) {
      console.error("Failed to load glossary", e);
      addLog("글로서리 로드 실패: " + String(e));
      glossaryData = {};
    }
  }

  async function saveGlossary() {
    if (!selectedFolder) return;
    const glossaryPath = selectedFolder + '/glossary.json';
    try {
      const content = JSON.stringify(glossaryData, null, 2);
      await invoke('save_staged_json', { path: glossaryPath, content });
      addLog("글로서리 변경 완료 및 저장");
    } catch (e) {
      console.error("Failed to save glossary", e);
      addLog("글로서리 저장 실패: " + String(e));
    }
  }

  async function addGlossaryItem() {
    if (!selectedFolder) {
      alert("폴더를 먼저 선택하거나 열어주세요. 글로서리는 작업 폴더 하위에 저장됩니다.");
      return;
    }
    const jp = newGlossaryJp.trim();
    const ko = newGlossaryKo.trim();
    if (!jp || !ko) {
      alert("원문과 번역어 모두 입력해 주세요.");
      return;
    }
    
    glossaryData[jp] = ko;
    newGlossaryJp = '';
    newGlossaryKo = '';
    await saveGlossary();
  }

  async function addGlossaryDirect(jp: string, ko: string) {
    if (!selectedFolder) {
      alert("폴더를 먼저 선택하거나 열어주세요. 글로서리는 작업 폴더 하위에 저장됩니다.");
      return;
    }
    const cleanJp = jp.trim();
    const cleanKo = ko.trim();
    if (!cleanJp || !cleanKo) {
      alert("원문과 번역어 모두 입력되어 있어야 합니다.");
      return;
    }
    
    // 이미 존재하는 단어인지 체크
    if (glossaryData[cleanJp]) {
      const overwrite = await ask(
        `이미 용어 사전에 등록된 단어입니다.\n\n• 기존 번역: "${glossaryData[cleanJp]}"\n• 새 번역: "${cleanKo}"\n\n덮어쓰시겠습니까?`,
        { title: '용어 사전 덮어쓰기', kind: 'warning', okLabel: '예, 덮어쓰기', cancelLabel: '취소' }
      );
      if (!overwrite) return;
    }

    glossaryData[cleanJp] = cleanKo;
    await saveGlossary();
    alert(`용어 사전에 등록되었습니다: "${cleanJp}" ➔ "${cleanKo}"`);
  }

  async function removeGlossaryItem(jp: string) {
    if (!selectedFolder) return;
    delete glossaryData[jp];
    await saveGlossary();
  }

  async function applyFolderWideReplacement(originalText: string, newTranslatedText: string) {
    if (!selectedFolder) return;
    
    // 현재 화면의 변경 사항을 먼저 인메모리 scripts 배열에 동기화
    saveCurrentTranslations();

    isLoading = true;
    let matchCount = 0;
    let fileCount = 0;

    try {
      // 1. 현재 메모리 상의 파일에서 매칭 확인
      let currentMatches = 0;
      const filename = originalFilePath.split(/[/\\]/).pop() || '';
      const isScripts = filename.toLowerCase().startsWith('scripts');

      for (const s of scripts) {
        if (isScripts) {
          if (s.code && (s.code.includes(`'${originalText}'`) || s.code.includes(`"${originalText}"`))) {
            currentMatches++;
          }
        } else {
          if (s.code === originalText) {
            currentMatches++;
          }
        }
      }
      
      if (currentMatches > 0) {
        matchCount += currentMatches;
        fileCount++;
      }

      // 2. 폴더 내 다른 모든 _staged.json 파일들에서 매칭 확인
      if (rvdataFiles.length > 0) {
        for (const filePath of rvdataFiles) {
          if (filePath === originalFilePath) continue;

          const stagedPath = filePath.replace(/\.(rvdata2?)$/i, '_staged.json');
          const exists: boolean = await invoke('check_file_exists', { path: stagedPath });
          if (exists) {
            const content: string = await invoke('read_staged_json', { path: stagedPath });
            const stagedData = JSON.parse(content);
            
            let fileMatches = 0;
            for (const scriptId of Object.keys(stagedData)) {
              const translations = stagedData[scriptId];
              if (translations && translations[originalText] !== undefined) {
                fileMatches++;
              }
            }
            if (fileMatches > 0) {
              matchCount += fileMatches;
              fileCount++;
            }
          }
        }
      }
    } catch (err) {
      console.error("Failed to count occurrences in folder", err);
    } finally {
      isLoading = false;
    }

    const confirmReplace = await ask(
      `이 원문의 번역을 폴더 내 모든 파일 및 캐시에서 일괄적으로 변경하시겠습니까?\n\n` +
      `• 발견된 기존 번역/매칭 항목: ${matchCount}개 (대상 파일: ${fileCount}개)\n` +
      `• 원문: "${originalText}"\n` +
      `• 변경 번역: "${newTranslatedText}"`,
      { title: '폴더 내 일괄 변경 확인', kind: 'warning', okLabel: '예, 모두 변경', cancelLabel: '취소' }
    );
    
    if (!confirmReplace) return;

    isLoading = true;
    try {
      // 1. 캐시 업데이트
      cacheData[originalText] = newTranslatedText;
      await saveCache();

      // 2. 현재 메모리 상의 scripts 업데이트
      let currentFileUpdates = false;
      const filename = originalFilePath.split(/[/\\]/).pop() || '';
      const isScripts = filename.toLowerCase().startsWith('scripts');

      for (const s of scripts) {
        if (isScripts) {
          if (s.code && (s.code.includes(`'${originalText}'`) || s.code.includes(`"${originalText}"`))) {
            if (!s.translations) s.translations = {};
            s.translations[originalText] = newTranslatedText;
            currentFileUpdates = true;
          }
        } else {
          if (s.code === originalText) {
            if (!s.translations) s.translations = {};
            s.translations[s.code] = newTranslatedText;
            currentFileUpdates = true;
          }
        }
      }

      if (currentFileUpdates) {
        extractedStrings = extractedStrings.map(item => {
          if (item.text === originalText) {
            return { ...item, translatedText: newTranslatedText };
          }
          return item;
        });
        saveCurrentTranslations();
        await saveStagedFile();
      }

      // 3. 폴더 내 다른 모든 _staged.json 업데이트
      if (rvdataFiles.length > 0) {
        for (const filePath of rvdataFiles) {
          if (filePath === originalFilePath) continue;

          const stagedPath = filePath.replace(/\.(rvdata2?)$/i, '_staged.json');
          const exists: boolean = await invoke('check_file_exists', { path: stagedPath });
          if (exists) {
            const content: string = await invoke('read_staged_json', { path: stagedPath });
            const stagedData = JSON.parse(content);
            
            let fileChanged = false;
            for (const scriptId of Object.keys(stagedData)) {
              const translations = stagedData[scriptId];
              if (translations && translations[originalText] !== undefined) {
                translations[originalText] = newTranslatedText;
                fileChanged = true;
              }
            }

            if (fileChanged) {
              await invoke('save_staged_json', {
                path: stagedPath,
                content: JSON.stringify(stagedData, null, 2)
              });
            }
          }
        }
      }
      
      addLog(`[일괄 변경 완료] "${originalText}"의 번역을 모든 파일과 캐시에서 변경했습니다. (총 ${matchCount}개 변경 완료)`);
      alert(`성공적으로 일괄 변경되었습니다. (총 ${matchCount}개 변경 완료)`);
    } catch (err) {
      console.error("Global folder replace failed", err);
      alert("일괄 변경 중 오류가 발생했습니다: " + err);
    } finally {
      isLoading = false;
    }
  }

  export async function loadFile(path: string) {
    if (isTranslating || isLoading) {
      alert("작업 중이거나 로딩 중에는 다른 파일을 불러올 수 없습니다.");
      return;
    }
    isLoading = true;
    
    // Check if path is a directory (e.g. dragged folder)
    try {
      const isDir: boolean = await invoke('is_directory', { path });
      if (isDir) {
        selectedFolder = path;
        const files: string[] = await invoke('get_rvdata_in_folder', { folderPath: selectedFolder });
        rvdataFiles = files;
        selectedFileInFolder = '';
        scripts = [];
        extractedStrings = [];
        selectedSidebarItem = null;
        await loadCache();
        await loadGlossary();
        await updateStagedFilesMap();
        addLog(`폴더 로드 완료. ${files.length}개의 데이터 파일을 찾았습니다: ${path.split(/[/\\]/).pop()}`);
        isLoading = false;
        return;
      }
    } catch (err) {
      console.error("is_directory check failed", err);
    }

    if (!path.endsWith('.rvdata') && !path.endsWith('.rvdata2')) {
      alert(".rvdata 또는 .rvdata2 데이터 파일을 선택해 주세요.");
      isLoading = false;
      return;
    }
    
    let stagedPath = path.replace(/\.(rvdata2?)$/i, '_staged.json');
    let shouldApplyStaged = false;
    
    try {
      const hasStaged: boolean = await invoke('check_file_exists', { path: stagedPath });
      addLog(`임시 파일 감지 결과: ${hasStaged} | 경로: ${stagedPath.split(/[/\\]/).pop()}`);
      if (hasStaged) {
        const confirmLoad = await ask(
          `이전에 작업하던 임시 저장 파일(_staged)이 존재합니다. 이어서 작업하시겠습니까?\n\n임시 파일: ${stagedPath.split(/[/\\]/).pop()}`,
          { title: '임시 저장 불러오기', kind: 'info', okLabel: '이어서 작업', cancelLabel: '원본 불러오기' }
        );
        if (confirmLoad) {
          shouldApplyStaged = true;
          addLog("임시 번역 진행 상황(_staged.json)을 이어서 적용합니다.");
        } else {
          addLog("원본 파일을 새로 불러옵니다.");
        }
      }
    } catch (err) {
      console.error("Failed to check staged file", err);
      addLog("임시 파일 확인 실패: " + String(err));
    }

    originalFilePath = path;
    try {
      const filename = path.split(/[/\\]/).pop() || '';
      const isScripts = filename.toLowerCase().startsWith('scripts');
      
      const loaded: ScriptEntry[] = await invoke('parse_rvdata', { path });
      scripts = loaded.map(s => {
        let count = 0;
        if (isScripts) {
          const jpRegex = /'([^']*[ぁ-んァ-ヶ一-龠]+[^']*)'|"([^"]*[ぁ-んァ-ヶ一-龠]+[^"]*)"/g;
          const matches = s.code.match(jpRegex);
          count = matches ? matches.length : 0;
        } else {
          count = s.code ? 1 : 0;
        }
        return { ...s, count };
      });

      if (shouldApplyStaged) {
        try {
          const jsonContent: string = await invoke('read_staged_json', { path: stagedPath });
          const stagedData = JSON.parse(jsonContent);
          
          for (const s of scripts) {
            if (stagedData[s.id]) {
              s.translations = stagedData[s.id];
            }
          }
          addLog("임시 저장된 번역 데이터가 성공적으로 복구되었습니다.");
        } catch (jsonErr) {
          console.error("Failed to parse staged JSON", jsonErr);
          addLog("임시 저장 JSON 분석 실패: " + String(jsonErr));
          alert("임시 저장 파일 분석 실패: " + jsonErr);
        }
      }

      selectedSidebarItem = null;
      extractedStrings = [];

      if (!rvdataFiles.includes(path)) {
        selectedFolder = '';
        rvdataFiles = [];
        selectedFileInFolder = '';
      } else {
        selectedFileInFolder = path;
      }
    } catch (e) {
      alert("Failed to load scripts: " + e);
    } finally {
      isLoading = false;
    }
  }



  async function openFolder() {
    if (isTranslating || isLoading) {
      alert("작업 중이거나 로딩 중에는 다른 폴더를 열 수 없습니다.");
      return;
    }
    const selected = await open({
      directory: true,
      multiple: false
    });

    if (selected && typeof selected === 'string') {
      selectedFolder = selected;
      try {
        const files: string[] = await invoke('get_rvdata_in_folder', { folderPath: selectedFolder });
        rvdataFiles = files;
        selectedFileInFolder = '';
        scripts = [];
        extractedStrings = [];
        selectedSidebarItem = null;
        await loadCache();
        await loadGlossary();
        await updateStagedFilesMap();
        addLog(`폴더 선택 완료. ${files.length}개의 데이터 파일을 찾았습니다.`);
      } catch (e) {
        alert("폴더 로드 실패: " + e);
        addLog("폴더 로드 실패: " + String(e));
      }
    }
  }

  async function handleFolderFileChange(event: Event) {
    const selectEl = event.target as HTMLSelectElement;
    const path = selectEl.value;
    if (path) {
      await loadFile(path);
    }
  }

  function selectSidebarItem(item: SidebarItem) {
    selectedSidebarItem = item;
    const matches: ExtractedString[] = [];
    let idCounter = 0;
    let cacheApplied = false;

    if (!item.isGroup && item.scriptRef) {
      const script = item.scriptRef;
      const jpRegex = /'([^']*[ぁ-んァ-ヶ一-龠]+[^']*)'|"([^"]*[ぁ-んァ-ヶ一-龠]+[^"]*)"/g;
      let match;
      while ((match = jpRegex.exec(script.code)) !== null) {
          const text = match[1] || match[2] || '';
          let translatedText = script.translations?.[text] || '';
          if (!translatedText && cacheData[text]) {
            translatedText = cacheData[text];
            cacheApplied = true;
          }
          matches.push({ 
            id: idCounter++, 
            text: text,
            originalCodeMatch: match[0],
            translatedText: translatedText
          });
      }
    } else if (item.isGroup && item.groupEntries) {
      for (const s of item.groupEntries) {
        let translatedText = s.translations?.[s.code] || '';
        if (!translatedText && cacheData[s.code]) {
          translatedText = cacheData[s.code];
          cacheApplied = true;
        }
        matches.push({
          id: s.id,
          text: s.code,
          originalCodeMatch: s.code,
          translatedText: translatedText
        });
      }
    }
    extractedStrings = matches;

    if (cacheApplied) {
      saveCurrentTranslations();
      saveStagedFile();
    }
  }

  async function handleTranslateRow(item: ExtractedString, signal?: AbortSignal) {
    item.translatedText = "번역 중...";
    extractedStrings = [...extractedStrings];

    let glossaryRules = "";
    if (selectedFolder) {
      const matchedEntries = Object.entries(glossaryData).filter(([jp, ko]) => {
        return jp && ko && item.text.includes(jp);
      });
      if (matchedEntries.length > 0) {
        glossaryRules = "\nGlossary (Use these exact translations for the matches):\n" +
          matchedEntries.map(([jp, ko]) => `- "${jp}" -> "${ko}"`).join("\n") + "\n";
      }
    }

    const prompt = `Translate the following Japanese game script text into Korean. 
Rules:
1. Maintain the meaning and nuance of the original Japanese.
2. Do not explain the translation, just output the exact Korean translation.
3. Keep proper nouns and context consistent if possible.${glossaryRules}
4. Preserve all special characters, brackets (e.g. 「 and 」), ellipses (e.g. …… or ......), and punctuation marks from the original text exactly.
5. If the text looks like a source code comment (e.g. containing #) or variable assignment (e.g. containing =), preserve all symbols like #, =, and variable names exactly, only translating the Japanese text.

Original text: "${item.text}"`;

    const isLlamaCpp = selectedModelId.startsWith('llamacpp:');
    addLog(`[번역 요청] 원문: "${item.text}" | 모델: ${selectedModelId}`);

    try {
      let responseText = '';

      if (isLlamaCpp) {
        // llama.cpp 서버 (OpenAI 호환 API)
        await startLlamaIfNeeded(selectedModelId);
        const lcResponse = await fetch("http://127.0.0.1:8080/v1/chat/completions", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          signal,
          body: JSON.stringify({
            messages: [
              { role: "system", content: "You are a professional Japanese to Korean game script translator. Output ONLY the Korean translation, nothing else." },
              { role: "user", content: prompt }
            ],
            temperature: 0.3,
            stream: false
          })
        });
        if (!lcResponse.ok) {
          throw new Error(`llama.cpp 통신 실패: ${lcResponse.status} ${lcResponse.statusText}`);
        }
        const lcResult = await lcResponse.json();
        responseText = lcResult.choices?.[0]?.message?.content || '';
      } else {
        // Ollama API
        const ollamaModel = selectedModelId.startsWith('ollama:') ? selectedModelId.slice(7) : ollamaModelName;
        const response = await fetch("http://127.0.0.1:11434/api/generate", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          signal,
          body: JSON.stringify({
            model: ollamaModel,
            prompt: prompt,
            stream: false
          })
        });
        if (!response.ok) {
          throw new Error(`Ollama 통신 실패: ${response.status} ${response.statusText}`);
        }
        const result = await response.json();
        responseText = result.response || '';
      }
      
      // AI 응답 클리닝: "Korean translation: ..." 등 군더더기 제거
      let cleaned = responseText.trim();
      const prefixRegex = /^(korean\s+translation|translation|korean|한국어\s*번역|한글\s*번역|번역)\s*[:\-]\s*/i;
      for (let i = 0; i < 5; i++) {
        const before = cleaned;
        cleaned = cleaned.replace(prefixRegex, '').trim();
        cleaned = cleaned.replace(/^["\u201c](([\s\S]*))["\u201d]$/, '$1').trim();
        cleaned = cleaned.replace(/^\'(([\s\S]*))\'\ *$/, '$1').trim();
        if (cleaned === before) break;
      }

      // 원문의 앞쪽 공백(들여쓰기) 보존
      const leadingSpaces = item.text.match(/^\s*/)?.[0] || '';
      const cleanText = leadingSpaces + cleaned;
      
      item.translatedText = cleanText;
      item.errorMsg = undefined;
      validateRow(item);
      addLog(`[번역 완료] 결과: "${cleanText}"`);

      if (selectedFolder && cleanText && cleanText !== "번역 중..." && cleanText !== "번역 실패") {
        cacheData[item.text] = cleanText;
        await saveCache();
      }
    } catch(e: any) {
      if (e.name === 'AbortError' || (e.message && e.message.includes('abort'))) {
        item.translatedText = "번역 중단";
        addLog(`[번역 중단] 작업이 사용자에 의해 중지되었습니다.`);
        throw e; // 호출한 상위 함수로 AbortError 전파
      }
      console.error("Translation Error", e);
      const errMsg = e?.message || JSON.stringify(e) || String(e);
      item.translatedText = "번역 실패";
      item.errorMsg = errMsg;
      addLog(`[번역 실패] 오류: ${errMsg}`);
    } finally {
      extractedStrings = [...extractedStrings];
      saveCurrentTranslations();
      await saveStagedFile();
    }
  }

  async function saveStagedFile() {
    if (!originalFilePath) return;
    saveCurrentTranslations();

    const stagedData: Record<number, Record<string, string>> = {};
    for (const s of scripts) {
      if (s.translations && Object.keys(s.translations).length > 0) {
        stagedData[s.id] = s.translations;
      }
    }

    const stagedPath = originalFilePath.replace(/\.(rvdata2?)$/i, '_staged.json');
    const jsonContent = JSON.stringify(stagedData, null, 2);

    try {
      await invoke('save_staged_json', {
        path: stagedPath,
        content: jsonContent
      });
      const isCompleted = await checkFileCompletion(originalFilePath);
      stagedFilesMap[originalFilePath] = isCompleted ? 'completed' : 'working';
      addLog(`[임시 저장] 번역 진행 상황이 저장되었습니다: ${stagedPath.split(/[/\\]/).pop()}`);
    } catch (e) {
      console.error("임시 저장 실패", e);
      addLog("[임시 저장 실패] " + String(e));
    }
  }

  function validateRow(item: ExtractedString) {
    if (!item.translatedText || item.translatedText === "번역 중..." || item.translatedText === "번역 실패") {
      item.validationError = undefined;
      return;
    }

    const jpText = item.text;
    const koText = item.translatedText;

    // 1. 용어 사전 미참조 검증
    if (selectedFolder) {
      const matchedEntries = Object.entries(glossaryData).filter(([jp]) => jp && jpText.includes(jp));
      for (const [jp, ko] of matchedEntries) {
        if (!koText.includes(ko)) {
          item.validationError = `용어 사전 미참조 ("${jp}" → "${ko}")`;
          return;
        }
      }
    }

    // 2. 불완전 번역 (일본어 문자 잔존 여부)
    const hasJapanese = /[\u3040-\u309F\u30A0-\u30FF\u4E00-\u9FFF]/.test(koText);
    if (hasJapanese) {
      // 원문에도 한자가 있는 경우는 허용 (일본 고유명사 등)
      const jpOnlyKana = /[\u3040-\u309F\u30A0-\u30FF]/.test(koText);
      if (jpOnlyKana) {
        item.validationError = `번역 미완료 (일본어 문자 잔존)`;
        return;
      }
    }

    // 3. 불필요한 접두사 잔존 (Korean translation: 등)
    const prefixCheck = /^(korean\s+translation|translation|korean|한국어\s*번역|한글\s*번역)\s*[:\-]/i;
    if (prefixCheck.test(koText.trim())) {
      item.validationError = `불필요한 접두사 잔존`;
      return;
    }

    // 4. 개행 수 불일치
    const countNewlines = (s: string) => (s.match(/\n/g) || []).length;
    if (countNewlines(jpText) !== countNewlines(koText)) {
      item.validationError = `개행 수 불일치 (원문: ${countNewlines(jpText)}개, 번역: ${countNewlines(koText)}개)`;
      return;
    }

    item.validationError = undefined;
  }

  async function handleBatchValidate() {
    extractedStrings = extractedStrings.map(item => {
      validateRow(item);
      return { ...item };
    });
    const errorCount = extractedStrings.filter(i => i.validationError).length;
    addLog(`[일괄 검증 완료] 문제 항목 ${errorCount}개 발견`);

    if (errorCount === 0) {
      await message("검증 완료: 발견된 문제 항목이 없습니다! ✅", { title: "검증 완료", kind: "info" });
    } else {
      await message(`검증 완료: 총 ${errorCount}개의 문제 항목이 발견되었습니다. ⚠️\n(빨간색 테두리와 경고 아이콘을 확인하세요.)`, { title: "검증 완료", kind: "warning" });
    }
  }

  function cancelBatchTranslation() {
    cancelRequested = true;
    if (translationAbortController) {
      translationAbortController.abort();
      addLog("번역 중단 요청 전송됨...");
    }
  }

  async function handleBatchTranslate() {
    const itemsToTranslate = extractedStrings.filter(i =>
      !i.translatedText ||
      i.translatedText === "번역 실패" ||
      i.validationError !== undefined
    );
    if (itemsToTranslate.length === 0) return;
    isTranslating = true;
    cancelRequested = false;
    translationAbortController = new AbortController();
    batchTotal = itemsToTranslate.length;
    batchCompleted = 0;

    let cacheUpdates = false;

    try {
      await invoke('prevent_sleep');
      for (let item of itemsToTranslate) {
        if (cancelRequested) {
          addLog("사용자에 의해 일괄 번역이 중단되었습니다.");
          break;
        }

        // validationError가 있으면 캐시 없이 항상 재번역
        if (!item.validationError && selectedFolder && cacheData[item.text]) {
          item.translatedText = cacheData[item.text];
          item.errorMsg = undefined;
          batchCompleted++;
          cacheUpdates = true;
          addLog(`[캐시 적용] 원문: "${item.text}" -> "${cacheData[item.text]}"`);
          continue;
        }

        try {
          await handleTranslateRow(item, translationAbortController.signal);
        } catch (err: any) {
          if (err.name === 'AbortError' || (err.message && err.message.includes('abort'))) {
            addLog("일괄 번역이 즉시 중지되었습니다.");
            break;
          }
          throw err;
        }
        
        batchCompleted++;
        
        if (batchCompleted % 10 === 0) {
          await saveStagedFile();
        }
      }
      if (cacheUpdates) {
        extractedStrings = [...extractedStrings];
        saveCurrentTranslations();
      }
      await saveStagedFile();
    } catch (e) {
      console.error("Batch translation error", e);
    } finally {
      await invoke('allow_sleep');
      isTranslating = false;
      batchTotal = 0;
      batchCompleted = 0;
      cancelRequested = false;
      translationAbortController = null;
    }
  }

  async function saveFile() {
    if (!originalFilePath) return;

    // 현재 선택된 스크립트의 번역 상태를 먼저 저장
    saveCurrentTranslations();

    const filename = originalFilePath.split(/[/\\]/).pop() || '';
    const isScripts = filename.toLowerCase().startsWith('scripts');

    const updatedScripts = scripts.map(s => {
      if (!s.translations || Object.keys(s.translations).length === 0) {
        return { id: s.id, title: s.title, code: s.code };
      }

      let newCode = s.code;
      if (isScripts) {
        for (const [originalText, translatedText] of Object.entries(s.translations)) {
          if (translatedText && translatedText !== "번역 중..." && translatedText !== "번역 실패") {
            const escapeRegex = (str: string) => str.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
            const escapedText = escapeRegex(originalText);
            const singleQuoteRegex = new RegExp(`'${escapedText}'`, 'g');
            const doubleQuoteRegex = new RegExp(`"${escapedText}"`, 'g');
            newCode = newCode.replace(singleQuoteRegex, `'${translatedText}'`);
            newCode = newCode.replace(doubleQuoteRegex, `"${translatedText}"`);
          }
        }
      } else {
        const translatedText = s.translations[s.code];
        if (translatedText && translatedText !== "번역 중..." && translatedText !== "번역 실패") {
          newCode = translatedText;
        }
      }

      return {
        id: s.id,
        title: s.title,
        code: newCode
      };
    });

    const newPath = await save({
      defaultPath: originalFilePath.replace('.rvdata', '_translated.rvdata'),
      filters: [{
        name: 'RPG Maker Scripts',
        extensions: ['rvdata', 'rvdata2']
      }]
    });

    if (newPath) {
      try {
        await invoke('save_rvdata', { 
          originalPath: originalFilePath, 
          newPath: newPath, 
          updatedScripts: updatedScripts 
        });
        alert('성공적으로 저장되었습니다!');
      } catch (e) {
        alert('저장 실패: ' + e);
      }
    }
  }

  function saveCurrentTranslations() {
    if (!selectedSidebarItem) return;
    
    if (!selectedSidebarItem.isGroup && selectedSidebarItem.scriptRef) {
      const script = selectedSidebarItem.scriptRef;
      const transMap: { [key: string]: string } = {};
      for (const item of extractedStrings) {
        if (item.translatedText) {
          transMap[item.text] = item.translatedText;
        }
      }
      script.translations = transMap;
      
      const idx = scripts.findIndex(s => s.id === script.id);
      if (idx !== -1) {
        scripts[idx] = script;
      }
    } else if (selectedSidebarItem.isGroup && selectedSidebarItem.groupEntries) {
      for (const item of extractedStrings) {
        const scriptId = item.id;
        const idx = scripts.findIndex(s => s.id === scriptId);
        if (idx !== -1) {
          const s = scripts[idx];
          const transMap: { [key: string]: string } = {};
          if (item.translatedText) {
            transMap[s.code] = item.translatedText;
          }
          s.translations = transMap;
          scripts[idx] = s;
        }
      }
    }
  }

  async function handleManualTranslationChange(item: ExtractedString) {
    const cleanText = item.translatedText ? item.translatedText.trim() : '';
    saveCurrentTranslations();
    
    if (selectedFolder && cleanText && cleanText !== "번역 중..." && cleanText !== "번역 실패") {
      cacheData[item.text] = cleanText;
      await saveCache();
    }
    await saveStagedFile();
  }

  async function clickTranslateRow(item: ExtractedString) {
    if (item.translatedText === "번역 중...") {
      if (isTranslating && translationAbortController) {
        // 일괄 번역 중인 경우 -> 전체 일괄 번역 취소
        translationAbortController.abort();
        cancelRequested = true;
      } else {
        // 단일 번역 중인 경우 -> 단일 번역 취소
        const controller = rowAbortControllers.get(item.id);
        if (controller) {
          controller.abort();
          rowAbortControllers.delete(item.id);
        }
      }
      return;
    }

    // 번역 시작
    const controller = new AbortController();
    rowAbortControllers.set(item.id, controller);

    try {
      await handleTranslateRow(item, controller.signal);
    } catch (e: any) {
      if (e.name === 'AbortError' || (e.message && e.message.includes('abort'))) {
        console.log(`Row translation aborted for item ${item.id}`);
      } else {
        throw e;
      }
    } finally {
      rowAbortControllers.delete(item.id);
    }
  }

  export function getStatus() {
    return { isTranslating, isLoading };
  }
</script>

<div class="app-container" class:drag-active={isDragging}>
  {#if isDragging}
    <div class="drag-overlay">
      <h2>Drop .rvdata or .rvdata2 here</h2>
    </div>
  {/if}

  <header class="glass-panel header">
    <div class="api-section" style="display: flex; align-items: center; gap: 0.5rem;">
      <label style="font-weight: 500; color: var(--text-secondary); white-space: nowrap; margin: 0;">모델:</label>
      <select
        bind:value={selectedModelId}
        on:change={onModelChange}
        class="api-input"
        style="cursor: pointer;"
        disabled={isTranslating || llamaServerStatus === 'starting' || llamaServerStatus === 'downloading'}
      >
        <optgroup label="llama.cpp">
          {#each llamacppPresets as preset}
            <option value={preset.id}>{preset.name}{isLlamaServerAvailable ? '' : ' (실행파일 없음)'}</option>
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

      <!-- 신호등 상태 -->
      {#if selectedModelId.startsWith('llamacpp:')}
        <div class="llama-dot-wrap">
          <div
            class="llama-dot"
            class:llama-dot-ok={llamaServerStatus === 'ready'}
            class:llama-dot-warn={llamaServerStatus === 'starting'}
            class:llama-dot-download={llamaServerStatus === 'downloading'}
            class:llama-dot-err={llamaServerStatus === 'idle' || llamaServerStatus === 'error'}
            on:click={selectLlamaServerPath}
            role="button"
            tabindex="0"
            on:keydown={(e) => e.key === 'Enter' && selectLlamaServerPath()}
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
              <div class="llama-download-progress-bar-wrap" style="width: 100%; height: 6px; background: rgba(255,255,255,0.1); border-radius: 3px; overflow: hidden; margin: 0.5rem 0;">
                <div class="llama-download-progress-bar" style="width: {downloadProgress}%; height: 100%; background: #3b82f6; transition: width 0.3s ease;"></div>
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
          title={ollamaStatus === 'online' ? 'Ollama 서버 정상 연결됨' : (ollamaStatus === 'checking' ? 'Ollama 서버 확인 중...' : 'Ollama 서버 연결 실패')}
          style="width: 10px; height: 10px; border-radius: 50%; flex-shrink: 0; background-color: {ollamaStatus === 'online' ? '#10b981' : (ollamaStatus === 'checking' ? '#f59e0b' : '#ef4444')}; box-shadow: 0 0 5px {ollamaStatus === 'online' ? 'rgba(16,185,129,0.5)' : (ollamaStatus === 'checking' ? 'rgba(245,158,11,0.5)' : 'rgba(239,68,68,0.5)')}; margin-left: 0.2rem;"
        ></div>
      {/if}
    </div>

    <div class="header-actions" style="display: flex; align-items: center; gap: 0.8rem;">
      <button class="btn" on:click={openFolder} disabled={isLoading || isTranslating}>
        {isLoading ? '불러오는 중...' : '📂 폴더 열기'}
      </button>

      {#if rvdataFiles.length > 0}
        <select bind:value={selectedFileInFolder} on:change={handleFolderFileChange} class="api-input" style="width: auto; max-width: 200px; height: 38px; cursor: pointer; padding: 0.55rem 2rem 0.55rem 1rem;" disabled={isTranslating || isLoading}>
          <option value="">-- 파일 선택 --</option>
          {#each rvdataFiles as file}
            <option value={file}>
              {#if stagedFilesMap[file] === 'completed'}
                ✅ {file.split(/[/\\]/).pop()} (번역 완료)
              {:else}
                {stagedFilesMap[file] === 'working' ? '📝 ' : ''}{file.split(/[/\\]/).pop()}{stagedFilesMap[file] === 'working' ? ' (작업 중)' : ''}
              {/if}
            </option>
          {/each}
        </select>
      {/if}

      {#if selectedFolder}
        <span class="path-display" title={selectedFolder} style="font-size: 0.85rem; color: #94a3b8; max-width: 150px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 500;">
          📂 {selectedFolder.split(/[/\\]/).pop()}
        </span>
      {:else}
        {#if originalFilePath}
          <span class="path-display" title={originalFilePath} style="font-size: 0.85rem; color: #94a3b8; max-width: 150px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 500;">
            📄 {originalFilePath.split(/[/\\]/).pop()}
          </span>
        {/if}
      {/if}

      <button class="btn btn-success" on:click={saveFile} disabled={scripts.length === 0 || isTranslating || isLoading}>
        복사본으로 변환/저장
      </button>
    </div>
  </header>

  <main class="main-content" style="position: relative;">
    {#if isLoading}
      <div class="loading-overlay">
        <div class="spinner"></div>
        <p>데이터 파일을 분석하고 불러오는 중입니다...</p>
      </div>
    {/if}
    <aside class="sidebar glass-panel">
      <h3 class="sidebar-header">
        <span>대사 리스트</span>
        <span class="header-badge">{sidebarItems.filter(i => i.id !== "ALL_ITEMS").length}</span>
      </h3>
      <ul class="script-list">
        {#each sidebarItems as item, idx}
          <li 
            class:active={selectedSidebarItem?.id === item.id} 
            class:disabled={isTranslating}
            on:click={() => {
              if (isTranslating) return;
              saveCurrentTranslations();
              selectSidebarItem(item);
            }}
          >
            <span class="id">{idx}</span>
            <span class="title">{item.title || 'Untitled'}</span>
            {#if item.count !== undefined && item.count > 0}
              <span class="count-badge" class:fully-translated={item.completedCount === item.count}>
                {item.completedCount}/{item.count}
              </span>
            {/if}
          </li>
        {/each}
      </ul>
    </aside>

    <section class="content-area glass-panel">
      {#if selectedSidebarItem}
        <div class="content-header">
          <h2 style="display: inline-flex; align-items: center; gap: 0.5rem;">
            {selectedSidebarItem.title}
            {#if extractedStrings.length > 0}
              {@const completed = extractedStrings.filter(i => i.translatedText && i.translatedText !== "번역 중..." && i.translatedText !== "번역 실패" && i.translatedText.trim() !== "").length}
              <span class="count-badge" class:fully-translated={completed === extractedStrings.length}>
                {completed}/{extractedStrings.length}
              </span>
            {/if}
          </h2>
          <div style="display: flex; align-items: center; gap: 0.5rem;">
            <button class="btn" style="background-color: transparent; border: 1px solid #475569; color: #94a3b8; padding: 0 12px; height: 32px; font-size: 0.85rem;" on:click={() => showGlossaryPanel = !showGlossaryPanel}>
              {showGlossaryPanel ? '📖 용어 사전 닫기' : '📘 용어 사전 관리'}
            </button>
            {#if isTranslating && batchTotal > 0}
              <span style="color: var(--accent-color); font-weight: bold; white-space: nowrap; font-size: 0.9rem; margin-right: 0.5rem;">
                {batchCompleted} / {batchTotal} 완료
              </span>
              <button class="btn" style="background-color: #ef4444;" on:click={cancelBatchTranslation}>
                중단하기
              </button>
            {:else}
              <button class="btn" on:click={handleBatchTranslate} disabled={extractedStrings.length === 0}>
                일괄 번역
              </button>
              <button class="btn" style="background-color: #7c3aed;" on:click={handleBatchValidate} disabled={extractedStrings.length === 0 || isTranslating}>
                일괄 검증
              </button>
            {/if}
          </div>
        </div>
        
        <!-- 용어 사전 패널 (고정 헤더 아래에 고정 배치) -->
        {#if showGlossaryPanel}
        <div class="glossary-fixed-container">
          <div class="glossary-panel glass-panel">
            <div class="glossary-header">
              <h3>용어 사전 (Glossary)</h3>
              <span style="font-size: 0.8rem; color: #94a3b8; margin-left: 0.5rem;">
                * 등록된 고유명사는 AI 번역 시 일관성 있게 우선 적용됩니다.
              </span>
            </div>
            <div class="glossary-add-form">
              <input type="text" bind:value={newGlossaryJp} placeholder="일본어 원문 (예: 薬草)" class="glossary-input" />
              <span style="color: #64748b;">➔</span>
              <input type="text" bind:value={newGlossaryKo} placeholder="한국어 번역 (예: 약초)" class="glossary-input" />
              <button class="btn-small btn-add" on:click={addGlossaryItem}>추가</button>
            </div>
            <div class="glossary-content scrollbar-hidden">
              {#if Object.keys(glossaryData).length === 0}
                <div class="empty-glossary">등록된 용어가 없습니다. 폴더 단위 작업 시 등록해 보세요.</div>
              {:else}
                <table class="glossary-table">
                  <thead>
                    <tr>
                      <th>일본어 원문</th>
                      <th>한국어 번역</th>
                      <th style="width: 60px;">관리</th>
                    </tr>
                  </thead>
                  <tbody>
                    {#each Object.entries(glossaryData) as [jp, ko]}
                      <tr>
                        <td class="jp-val">{jp}</td>
                        <td class="ko-val">{ko}</td>
                        <td>
                          <button class="btn-delete" on:click={() => removeGlossaryItem(jp)}>🗑️</button>
                        </td>
                      </tr>
                    {/each}
                  </tbody>
                </table>
              {/if}
            </div>
          </div>
        </div>
        {/if}

        <div class="content-body scrollbar-hidden">
          <div class="strings-list">
            {#each extractedStrings as item}
              <div class="string-row" class:translating={item.translatedText === "번역 중..."}>
                <div class="original-column">
                  <p class="original-text">{item.text}</p>
                </div>
                <div class="translate-column">
                  <textarea 
                    class="trans-input" 
                    class:has-error={item.validationError}
                    bind:value={item.translatedText} 
                    placeholder="번역된 내용이 여기에 표시됩니다..."
                    disabled={item.translatedText === "번역 중..."}
                    on:blur={() => handleManualTranslationChange(item)}
                  ></textarea>
                  {#if item.validationError}
                    <div class="validation-error-indicator" title={item.validationError}>
                      ⚠️ {item.validationError}
                    </div>
                  {:else if item.translatedText === "번역 실패" && item.errorMsg}
                    <div class="error-indicator" title="클릭하여 에러 메시지 복사" on:click={() => {
                      navigator.clipboard.writeText(item.errorMsg || '');
                      alert("에러 로그가 클립보드에 복사되었습니다!");
                    }}>
                      ⚠️ 에러 발생 (로그 복사)
                    </div>
                  {/if}
                </div>
                <div class="action-column" style="display: flex; flex-direction: row; gap: 0.35rem; align-items: center; justify-content: center; width: 110px;">
                  <button 
                    class="btn-small btn-action-icon" 
                    title={item.translatedText === "번역 중..." ? "번역 중 (클릭 시 취소)" : "번역"} 
                    on:click={() => clickTranslateRow(item)} 
                  >
                    {#if item.translatedText === "번역 중..."}
                      ⏳
                    {:else}
                      🤖
                    {/if}
                  </button>
                  {#if selectedFolder && item.translatedText && item.translatedText !== "번역 중..." && item.translatedText !== "번역 실패"}
                    <button 
                      class="btn-small btn-action-icon btn-global-replace" 
                      style="background-color: #6366f1;" 
                      title="폴더 내 동일 대사 일괄 변경" 
                      on:click={() => applyFolderWideReplacement(item.text, item.translatedText)} 
                      disabled={isTranslating}
                    >
                      🌐
                    </button>
                    <button 
                      class="btn-small btn-action-icon btn-add-glossary" 
                      style="background-color: #0d9488;" 
                      title="용어 사전에 단어 등록" 
                      on:click={() => addGlossaryDirect(item.text, item.translatedText)} 
                      disabled={isTranslating}
                    >
                      📖
                    </button>
                  {/if}
                </div>
              </div>
            {/each}
            {#if extractedStrings.length === 0}
              <p class="empty">이 스크립트에는 추출할 일본어 문자열이 없습니다.</p>
            {/if}
          </div>

          <!-- 디버그 패널 토글 영역 -->
          <div class="debug-toggle-area" style="text-align: right; padding: 0.5rem; margin-top: 1rem;">
            <button class="btn-small" style="background-color: transparent; border: 1px solid #475569; color: #94a3b8;" on:click={() => showDebugPanel = !showDebugPanel}>
              {showDebugPanel ? '디버그 로그 숨기기' : '디버그 로그 보기'}
            </button>
          </div>

          <!-- 디버그 패널 -->
          {#if showDebugPanel}
          <div class="debug-panel glass-panel">
            <div class="debug-header">
              <h3>디버그 로그 (Ollama 통신 과정)</h3>
              <button class="btn-small btn-save" on:click={() => debugLogs = []}>로그 지우기</button>
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
        </div>
      {:else}
        <div class="empty-state" style="display: flex; flex-direction: column; gap: 0.5rem; text-align: center;">
          {#if selectedFolder && rvdataFiles.length === 0}
            <span style="font-size: 2rem;">📂</span>
            <p style="color: #ef4444; font-weight: 500;">선택한 폴더에 데이터 파일(.rvdata, .rvdata2)이 존재하지 않습니다.</p>
          {:else if originalFilePath && scripts.length > 0 && scripts.reduce((acc, curr) => acc + (curr.count || 0), 0) === 0}
            <span style="font-size: 2rem;">🔍</span>
            <p style="color: #fbbf24; font-weight: 500;">불러온 파일에 번역할 일본어 대사가 존재하지 않습니다.</p>
          {:else if selectedFolder && !selectedFileInFolder}
            <span style="font-size: 2rem;">📂</span>
            <p>상단에서 분석할 데이터 파일을 선택해주세요.</p>
          {:else}
            <span style="font-size: 2rem;">📄</span>
            <p>왼쪽 목록에서 번역할 스크립트/대사 항목을 선택해주세요.</p>
          {/if}
        </div>
      {/if}
    </section>
  </main>
</div>

<style lang="scss">
  .app-container {
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
    
    h1 {
      font-size: 1.2rem;
      font-weight: 600;
      background: linear-gradient(to right, #60a5fa, #a78bfa);
      -webkit-background-clip: text;
      color: transparent;
    }

    .header-actions {
      display: flex;
      gap: 1rem;
    }
  }

  .btn-success {
    background: #10b981;
    &:hover {
      background: #059669;
    }
    &:disabled {
      opacity: 0.4;
      cursor: not-allowed;
    }
  }

  .btn-small {
    background: var(--accent-color);
    color: white;
    border: none;
    padding: 0.25rem 0.5rem;
    border-radius: 4px;
    font-size: 0.8rem;
    cursor: pointer;
    &:hover { background: var(--accent-hover); }
  }

  .count-badge {
    background: rgba(59, 130, 246, 0.15);
    color: #60a5fa;
    font-size: 0.7rem;
    padding: 0.1rem 0.4rem;
    border-radius: 9999px;
    font-weight: 600;
    align-self: center;
    border: 1px solid rgba(59, 130, 246, 0.25);
    flex-shrink: 0;
    display: inline-block;
    line-height: 1;
    transition: all 0.3s ease;

    &.fully-translated {
      background: rgba(16, 185, 129, 0.15);
      color: #34d399;
      border-color: rgba(16, 185, 129, 0.25);
    }
  }

  .main-content {
    display: flex;
    flex: 1;
    gap: 1rem;
    min-height: 0;
  }

  .sidebar {
    width: 300px;
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    overflow: hidden;
    
    .sidebar-header {
      flex-shrink: 0;
      padding: 1rem;
      border-bottom: 1px solid var(--border-color);
      font-size: 1.05rem;
      font-weight: 500;
      display: flex;
      align-items: center;
      justify-content: space-between;

      .header-badge {
        background: rgba(255, 255, 255, 0.08);
        color: #94a3b8;
        font-size: 0.75rem;
        padding: 0.15rem 0.45rem;
        border-radius: 9999px;
        font-weight: 600;
        border: 1px solid rgba(255, 255, 255, 0.15);
      }
    }
    
    .script-list {
      flex: 1;
      overflow-y: auto;
      list-style: none;
      padding: 0.5rem;
      
      li {
        padding: 0.75rem 1rem;
        cursor: pointer;
        border-radius: 6px;
        transition: all 0.2s ease;
        display: flex;
        gap: 0.5rem;
        
        .id {
          color: var(--text-secondary);
          font-size: 0.8rem;
          min-width: 2rem;
        }

        .title {
          flex: 1;
          white-space: nowrap;
          overflow: hidden;
          text-overflow: ellipsis;
          margin-right: 0.5rem;
        }
        
        &:hover {
          background: rgba(255, 255, 255, 0.05);
        }
        
        &.active {
          background: rgba(59, 130, 246, 0.2);
          border-left: 3px solid var(--accent-color);
        }

        &.disabled {
          opacity: 0.4;
          cursor: not-allowed;
          pointer-events: none;
        }
      }
    }
  }

  .api-section {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin-right: 1rem;
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

    &:disabled {
      opacity: 0.5;
      cursor: not-allowed;
    }
  }

  .api-input:focus {
    outline: none;
    border-color: var(--accent-color);
    box-shadow: 0 0 0 2px rgba(59, 130, 246, 0.3);
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

  .content-area {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    padding: 0;
    
    .content-header {
      flex-shrink: 0;
      padding: 1rem 1.5rem;
      background: rgba(15, 23, 42, 0.9);
      backdrop-filter: blur(12px);
      border-bottom: 1px solid var(--border-color);
      z-index: 10;

      display: flex;
      justify-content: space-between;
      align-items: center;
      h2 { 
        font-size: 1.2rem; 
        font-weight: 500; 
      }
    }

    .glossary-fixed-container {
      flex-shrink: 0;
      padding: 1rem 1.5rem;
      background: rgba(15, 23, 42, 0.9);
      backdrop-filter: blur(12px);
      border-bottom: 1px solid var(--border-color);
      z-index: 9;
    }

    .content-body {
      flex: 1;
      overflow-y: auto;
      padding: 1.5rem;
      display: flex;
      flex-direction: column;
    }
    
    .empty-state {
      flex: 1;
      display: flex;
      align-items: center;
      justify-content: center;
      color: var(--text-secondary);
      padding: 1.5rem;
    }

    .strings-list {
      display: flex;
      flex-direction: column;
      gap: 0.5rem;
    }

    .string-row {
      display: grid;
      grid-template-columns: 1fr 1fr 110px;
      align-items: stretch;
      background: rgba(15, 23, 42, 0.5);
      border: 1px solid var(--border-color);
      border-radius: 8px;
      overflow: hidden;
      transition: all 0.3s ease;
      min-height: 80px;

      &.translating {
        border-color: var(--accent-color);
        box-shadow: 0 0 12px rgba(59, 130, 246, 0.4);
        opacity: 0.9;
        
        .translate-column {
          background: rgba(59, 130, 246, 0.05);
        }
      }

      .original-column {
        padding: 1rem;
        border-right: 1px solid var(--border-color);
        display: flex;
        align-items: center;
        word-break: break-all;

        .original-text {
          font-size: 1rem;
          line-height: 1.4;
          margin: 0;
        }
      }

      .translate-column {
        border-right: 1px solid var(--border-color);
        display: flex;
        position: relative;
        
        .trans-input {
          width: 100%;
          height: 100%;
          min-height: 80px;
          background: transparent;
          border: none;
          padding: 1rem;
          color: #34d399; /* Green accent */
          font-size: 1rem;
          font-family: inherit;
          resize: vertical;
          transition: background 0.2s;
          
          &:focus {
            outline: none;
            background: rgba(255, 255, 255, 0.02);
          }

          &.has-error {
            background: rgba(239, 68, 68, 0.12);
            color: #fca5a5;
          }
        }

        .error-indicator {
          position: absolute;
          bottom: 4px;
          right: 8px;
          background: rgba(239, 68, 68, 0.2);
          border: 1px solid rgba(239, 68, 68, 0.5);
          color: #f87171;
          font-size: 0.72rem;
          padding: 0.15rem 0.4rem;
          border-radius: 4px;
          cursor: pointer;
          transition: background 0.2s;
          z-index: 10;
          
          &:hover {
            background: rgba(239, 68, 68, 0.4);
          }
        }

        .validation-error-indicator {
          position: absolute;
          bottom: 4px;
          left: 8px;
          right: 8px;
          background: rgba(239, 68, 68, 0.18);
          border: 1px solid rgba(239, 68, 68, 0.5);
          color: #fca5a5;
          font-size: 0.7rem;
          padding: 0.15rem 0.5rem;
          border-radius: 4px;
          z-index: 10;
          white-space: nowrap;
          overflow: hidden;
          text-overflow: ellipsis;
        }
      }

      .action-column {
        display: flex;
        align-items: center;
        justify-content: center;
        padding: 0.5rem;
        background: rgba(0, 0, 0, 0.15);
      }
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

    .btn-save {
      background: #475569;
      &:hover { background: #334155; }
    }
  }

  .loading-overlay {
    position: absolute;
    top: 0; left: 0; right: 0; bottom: 0;
    background: rgba(15, 23, 42, 0.72);
    backdrop-filter: blur(4px);
    z-index: 99;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 1rem;
    border-radius: 8px;

    p {
      color: #94a3b8;
      font-size: 1rem;
      font-weight: 500;
      margin: 0;
    }
  }

  .dropdown-menu {
    animation: fadeIn 0.12s ease-out;

    .menu-item {
      background: transparent;
      border: none;
      color: #f1f5f9;
      padding: 0.55rem 1rem;
      text-align: left;
      cursor: pointer;
      border-radius: 4px;
      transition: background 0.2s;
      font-size: 0.9rem;
      display: flex;
      align-items: center;
      gap: 0.5rem;
      width: 100%;

      &:hover {
        background: rgba(255, 255, 255, 0.08);
      }
    }
  }

  @keyframes fadeIn {
    from { opacity: 0; transform: translateY(-5px); }
    to { opacity: 1; transform: translateY(0); }
  }

  .spinner {
    width: 40px;
    height: 40px;
    border: 3px solid rgba(255, 255, 255, 0.1);
    border-radius: 50%;
    border-top-color: var(--accent-color, #3b82f6);
    animation: spin 1s ease-in-out infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .glossary-panel {
    flex-shrink: 0;
    height: 250px;
    display: flex;
    flex-direction: column;
    background: rgba(15, 23, 42, 0.5);
    border: 1px solid var(--border-color);
    border-radius: 8px;
    overflow: hidden;
    padding: 1rem;
    gap: 0.75rem;

    .glossary-header {
      display: flex;
      align-items: center;
      h3 { margin: 0; font-size: 1rem; font-weight: 600; color: #f1f5f9; }
    }

    .glossary-add-form {
      display: flex;
      align-items: center;
      gap: 0.5rem;
      flex-shrink: 0;
    }

    .glossary-input {
      background: rgba(0, 0, 0, 0.3);
      border: 1px solid rgba(255, 255, 255, 0.1);
      color: white;
      padding: 0.4rem 0.8rem;
      border-radius: 4px;
      font-size: 0.85rem;
      flex: 1;
      
      &:focus {
        outline: 2px solid var(--accent-color);
      }
    }

    .btn-add {
      background: var(--accent-color);
      height: 32px;
      padding: 0 1rem;
      font-weight: 500;
      &:hover { background: var(--accent-hover); }
    }

    .glossary-content {
      flex: 1;
      overflow-y: auto;
      background: rgba(0, 0, 0, 0.2);
      border-radius: 6px;
      border: 1px solid rgba(255, 255, 255, 0.05);

      .empty-glossary {
        padding: 2rem;
        text-align: center;
        color: #64748b;
        font-size: 0.85rem;
      }
    }

    .glossary-table {
      width: 100%;
      border-collapse: collapse;
      font-size: 0.85rem;
      text-align: left;

      th, td {
        padding: 0.5rem 0.75rem;
        border-bottom: 1px solid rgba(255, 255, 255, 0.05);
      }

      th {
        color: #94a3b8;
        font-weight: 600;
        background: rgba(255, 255, 255, 0.02);
      }

      .jp-val {
        color: #94a3b8;
      }

      .ko-val {
        color: #34d399;
      }

      .btn-delete {
        background: transparent;
        border: none;
        cursor: pointer;
        padding: 0.2rem;
        border-radius: 4px;
        &:hover {
          background: rgba(239, 68, 68, 0.15);
        }
      }
    }
  }

  .btn-global-replace {
    transition: background-color 0.2s;
    &:hover {
      background-color: #4f46e5 !important;
    }
  }

  .btn-add-glossary {
    transition: background-color 0.2s;
    &:hover {
      background-color: #0f766e !important;
    }
  }

  .btn-action-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    padding: 0;
    font-size: 1rem;
    border-radius: 6px;
    transition: all 0.2s ease;
    flex-shrink: 0;
    
    &:hover:not(:disabled) {
      transform: scale(1.1);
    }
  }
</style>
