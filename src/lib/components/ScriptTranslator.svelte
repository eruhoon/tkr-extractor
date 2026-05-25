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
    count?: number;
    translations?: { [key: string]: string };
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
      const jpRegex = /(['"])(.*?[ぁ-んァ-ヶ一-龠]+.*?)\1/g;
      let match;
      while ((match = jpRegex.exec(s.code)) !== null) {
        const text = match[2];
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
      alert(".rvdata 또는 .rvdata2 데이터 파일을 선택해 주세요.");
      return;
    }
    isLoading = true;
    originalFilePath = path;
    try {
      const filename = path.split(/[/\\]/).pop() || '';
      const isScripts = filename.toLowerCase().startsWith('scripts');
      
      const loaded: ScriptEntry[] = await invoke('parse_rvdata', { path });
      scripts = loaded.map(s => {
        let count = 0;
        if (isScripts) {
          const jpRegex = /(['"])(.*?[ぁ-んァ-ヶ一-龠]+.*?)\1/g;
          const matches = s.code.match(jpRegex);
          count = matches ? matches.length : 0;
        } else {
          count = s.code ? 1 : 0;
        }
        return { ...s, count };
      });
      selectedSidebarItem = null;
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

  function selectSidebarItem(item: SidebarItem) {
    selectedSidebarItem = item;
    const matches: ExtractedString[] = [];
    let idCounter = 0;

    if (!item.isGroup && item.scriptRef) {
      const script = item.scriptRef;
      const jpRegex = /(['"])(.*?[ぁ-んァ-ヶ一-龠]+.*?)\1/g;
      let match;
      while ((match = jpRegex.exec(script.code)) !== null) {
          const text = match[2];
          const translatedText = script.translations?.[text] || '';
          matches.push({ 
            id: idCounter++, 
            text: text,
            originalCodeMatch: match[0],
            translatedText: translatedText
          });
      }
    } else if (item.isGroup && item.groupEntries) {
      for (const s of item.groupEntries) {
        const translatedText = s.translations?.[s.code] || '';
        matches.push({
          id: s.id,
          text: s.code,
          originalCodeMatch: s.code,
          translatedText: translatedText
        });
      }
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
      saveCurrentTranslations();
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

</script>

<div class="app-container" class:drag-active={isDragging}>
  {#if isDragging}
    <div class="drag-overlay">
      <h2>Drop .rvdata or .rvdata2 here</h2>
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
        {isLoading ? '불러오는 중...' : '데이터 파일 열기 (.rvdata2)'}
      </button>
      <button class="btn btn-success" on:click={saveFile} disabled={scripts.length === 0}>
        복사본으로 변환/저장
      </button>
    </div>
  </header>

  <main class="main-content">
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

    <section class="content-area glass-panel scrollbar-hidden">
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
    padding: 0.8rem 1.5rem;
    
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
      h2 { 
        font-size: 1.2rem; 
        font-weight: 500; 
      }
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
