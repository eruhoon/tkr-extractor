<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen, TauriEvent } from '@tauri-apps/api/event';
  import { open, save } from '@tauri-apps/plugin-dialog';
  import { initTokenizer, tokenize, translateSentenceOllama, type Token } from '$lib/translator';

  interface ScriptEntry {
    id: number;
    title: string;
    code: string;
  }

  interface ExtractedString {
    id: number;
    text: string;
    translatedText: string;
    originalCodeMatch: string;
    errorMsg?: string;
  }

  let { isDragging = false } = $props();

  let scripts = $state<ScriptEntry[]>([]);
  let selectedScript = $state<ScriptEntry | null>(null);
  let extractedStrings = $state<ExtractedString[]>([]);
  let isTranslating = $state(false);
  let batchTotal = $state(0);
  let batchCompleted = $state(0);
  let isLoading = $state(false);
  let tokenizerReady = $state(false);
  let originalFilePath = $state('');

  let ollamaModelName = $state('gemma4:e4b');
  let showDebugPanel = $state(false);
  let debugLogs = $state<string[]>([]);

  function addLog(msg: string) {
    const time = new Date().toLocaleTimeString();
    debugLogs = [`[${time}] ${msg}`, ...debugLogs];
  }

  function saveModelName() {
    localStorage.setItem('ollamaModelName', ollamaModelName);
    alert('로컬 모델명이 저장되었습니다.');
    addLog("모델명 변경됨: " + ollamaModelName);
  }

  onMount(async () => {
    ollamaModelName = localStorage.getItem('ollamaModelName') || 'gemma4:e4b';
    addLog("앱 초기화 완료. 모델명: " + ollamaModelName);
    try {
      await initTokenizer();
      tokenizerReady = true;
    } catch (e) {
      console.error("Tokenizer init failed", e);
      addLog("토크나이저 초기화 실패: " + String(e));
    }
  });

  export async function loadFile(path: string) {
    if (!path.endsWith('.rvdata') && !path.endsWith('.rvdata2')) {
      alert("Please select a .rvdata or .rvdata2 file.");
      return;
    }
    isLoading = true;
    originalFilePath = path;
    try {
      scripts = await invoke('parse_rvdata', { path });
      selectedScript = null;
      extractedStrings = [];
    } catch (e) {
      alert("Failed to load scripts: " + e);
    } finally {
      isLoading = false;
    }
  }

  async function openFile() {
    const selected = await open({
      multiple: false,
      filters: [{
        name: 'RPG Maker Scripts',
        extensions: ['rvdata', 'rvdata2']
      }]
    });

    if (selected && typeof selected === 'string') {
      await loadFile(selected);
    }
  }

  function selectScript(script: ScriptEntry) {
    selectedScript = script;
    // Extract strings containing Japanese characters
    const jpRegex = /(['"])(.*?[ぁ-んァ-ヶ一-龠]+.*?)\1/g;
    const matches = [];
    let match;
    let idCounter = 0;
    while ((match = jpRegex.exec(script.code)) !== null) {
        matches.push({ 
          id: idCounter++, 
          text: match[2],
          originalCodeMatch: match[0],
          translatedText: ''
        });
    }
    extractedStrings = matches;
  }

  async function handleTranslateRow(item: ExtractedString) {
    item.translatedText = "번역 중...";
    extractedStrings = [...extractedStrings];

    const prompt = `Translate the following Japanese game script text into Korean. 
Rules:
1. Maintain the meaning and nuance of the original Japanese.
2. Do not explain the translation, just output the exact Korean translation.
3. Keep proper nouns and context consistent if possible.

Original text: "${item.text}"`;

    addLog(`[번역 요청] 원문: "${item.text}" | 모델명: ${ollamaModelName}`);

    try {
      const response = await fetch("http://127.0.0.1:11434/api/generate", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          model: ollamaModelName,
          prompt: prompt,
          stream: false
        })
      });

      if (!response.ok) {
        throw new Error(`Ollama 통신 실패: ${response.status} ${response.statusText}`);
      }

      const result = await response.json();
      const responseText: string = result.response || "";
      const cleanText = responseText.trim().replace(/^["'](.*)["']$/, '$1');
      
      item.translatedText = cleanText;
      item.errorMsg = undefined;
      addLog(`[번역 완료] 결과: "${cleanText}"`);
    } catch(e: any) {
      console.error("Translation Error", e);
      const errMsg = e?.message || JSON.stringify(e) || String(e);
      item.translatedText = "번역 실패";
      item.errorMsg = errMsg;
      addLog(`[번역 실패] 오류: ${errMsg}`);
    } finally {
      extractedStrings = [...extractedStrings];
    }
  }

  async function handleBatchTranslate() {
    const itemsToTranslate = extractedStrings.filter(i => !i.translatedText || i.translatedText === "번역 실패");
    if (itemsToTranslate.length === 0) return;
    isTranslating = true;
    batchTotal = itemsToTranslate.length;
    batchCompleted = 0;
    for (let item of itemsToTranslate) {
      if (!item.translatedText || item.translatedText === "번역 실패") {
        await handleTranslateRow(item);
        batchCompleted++;
      }
    }
    isTranslating = false;
    batchTotal = 0;
    batchCompleted = 0;
  }

  async function saveFile() {
    if (!originalFilePath) return;

    // Apply translations to the currently selected script first
    applyTranslationsToScript();

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
          updatedScripts: scripts 
        });
        alert('성공적으로 저장되었습니다!');
      } catch (e) {
        alert('저장 실패: ' + e);
      }
    }
  }

  function applyTranslationsToScript() {
    if (!selectedScript) return;
    
    let newCode = selectedScript.code;
    for (const item of extractedStrings) {
      if (item.translatedText && item.translatedText !== "번역 중..." && item.translatedText !== "번역 실패") {
        // Find exactly the original quote match and replace its inner text
        // Note: Replacing strings in code can be tricky if there are duplicates, we'll replace globally for this snippet
        const originalQuote = item.originalCodeMatch[0]; // ' or "
        const replacement = originalQuote + item.translatedText + originalQuote;
        // Escape special chars for regex
        const escapeRegex = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
        newCode = newCode.replace(new RegExp(escapeRegex(item.originalCodeMatch), 'g'), replacement);
      }
    }
    selectedScript.code = newCode;
    
    // Update the scripts array
    const idx = scripts.findIndex(s => s.id === selectedScript!.id);
    if (idx !== -1) {
      scripts[idx] = selectedScript;
    }
  }

</script>

<div class="app-container" class:drag-active={isDragging}>
  {#if isDragging}
    <div class="drag-overlay">
      <h2>Drop Scripts.rvdata/2 here</h2>
    </div>
  {/if}

  <header class="glass-panel header">
    <h1>TKR Extractor (Ollama)</h1>
    <div class="header-actions">
      <div class="api-section">
        <input type="text" bind:value={ollamaModelName} placeholder="Ollama 모델명 입력" class="api-input" />
        <button class="btn-small btn-save" on:click={saveModelName}>저장</button>
      </div>
      <button class="btn" on:click={openFile} disabled={isLoading}>
        {isLoading ? '불러오는 중...' : '파일 열기 (.rvdata2)'}
      </button>
      <button class="btn btn-success" on:click={saveFile} disabled={scripts.length === 0}>
        복사본으로 변환/저장
      </button>
    </div>
  </header>

  <main class="main-content">
    <aside class="sidebar glass-panel scrollbar-hidden">
      <h3>Scripts ({scripts.length})</h3>
      <ul class="script-list">
        {#each scripts as script, idx}
          <li 
            class:active={selectedScript?.id === script.id} 
            on:click={() => {
              applyTranslationsToScript();
              selectScript(script);
            }}
          >
            <span class="id">{idx}</span>
            <span class="title">{script.title || 'Untitled'}</span>
          </li>
        {/each}
      </ul>
    </aside>

    <section class="content-area glass-panel scrollbar-hidden">
      {#if selectedScript}
        <div class="content-header">
          <h2>{selectedScript.title} - 추출된 문자열 ({extractedStrings.length})</h2>
          <div style="display: flex; align-items: center; gap: 1rem;">
            {#if isTranslating && batchTotal > 0}
              <span style="color: var(--accent-color); font-weight: bold;">
                {batchCompleted} / {batchTotal} 완료
              </span>
            {/if}
            <button class="btn" on:click={handleBatchTranslate} disabled={isTranslating || extractedStrings.length === 0}>
              {isTranslating ? '일괄 번역 중...' : '일괄 번역'}
            </button>
          </div>
        </div>
        
        <div class="strings-list">
          {#each extractedStrings as item}
            <div class="string-row" class:translating={item.translatedText === "번역 중..."}>
              <div class="original-column">
                <p class="original-text">{item.text}</p>
              </div>
              <div class="translate-column">
                <textarea 
                  class="trans-input" 
                  bind:value={item.translatedText} 
                  placeholder="번역된 내용이 여기에 표시됩니다..."
                  disabled={item.translatedText === "번역 중..."}
                ></textarea>
                {#if item.translatedText === "번역 실패" && item.errorMsg}
                  <div class="error-indicator" title="클릭하여 에러 메시지 복사" on:click={() => {
                    navigator.clipboard.writeText(item.errorMsg || '');
                    alert("에러 로그가 클립보드에 복사되었습니다!");
                  }}>
                    ⚠️ 에러 발생 (로그 복사)
                  </div>
                {/if}
              </div>
              <div class="action-column">
                <button class="btn-small" on:click={() => handleTranslateRow(item)} disabled={item.translatedText === "번역 중..."}>
                  {item.translatedText === "번역 중..." ? "번역 중" : "번역"}
                </button>
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
      {:else}
        <div class="empty-state">
          <p>왼쪽에서 스크립트를 선택해주세요.</p>
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
    padding: 1rem 2rem;
    
    h1 {
      font-size: 1.5rem;
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
    overflow-y: auto;
    
    h3 {
      padding: 1rem;
      border-bottom: 1px solid var(--border-color);
    }
    
    .script-list {
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
        
        &:hover {
          background: rgba(255, 255, 255, 0.05);
        }
        
        &.active {
          background: rgba(59, 130, 246, 0.2);
          border-left: 3px solid var(--accent-color);
        }
      }
    }
  }

  .content-area {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow-y: auto;
    padding: 1.5rem;
    
    .content-header {
      display: flex;
      justify-content: space-between;
      align-items: center;
      margin-bottom: 1rem;
      h2 { font-weight: 500; }
    }
    
    .empty-state {
      flex: 1;
      display: flex;
      align-items: center;
      justify-content: center;
      color: var(--text-secondary);
    }

    .strings-list {
      display: flex;
      flex-direction: column;
      gap: 0.5rem;
    }

    .string-row {
      display: grid;
      grid-template-columns: 1fr 1fr 100px;
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
          
          &:focus {
            outline: none;
            background: rgba(255, 255, 255, 0.02);
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

    .api-section {
      display: flex;
      align-items: center;
      gap: 0.5rem;
      margin-right: 1rem;
    }

    .api-input {
      background: rgba(0, 0, 0, 0.3);
      border: 1px solid rgba(255,255,255,0.1);
      color: white;
      padding: 0.4rem 0.8rem;
      border-radius: 4px;
      width: 200px;
      font-size: 0.85rem;
    }

    .api-input:focus {
      outline: 2px solid var(--accent-color);
    }

    .btn-save {
      background: #475569;
      &:hover { background: #334155; }
    }
  }
</style>
