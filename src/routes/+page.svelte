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
  }

  let scripts: ScriptEntry[] = [];
  let selectedScript: ScriptEntry | null = null;
  let extractedStrings: ExtractedString[] = [];
  let isTranslating = false;
  let batchTotal = 0;
  let batchCompleted = 0;
  let isLoading = false;
  let tokenizerReady = false;
  let isDragging = false;
  let originalFilePath: string = '';

  onMount(async () => {
    try {
      await initTokenizer();
      tokenizerReady = true;
    } catch (e) {
      console.error("Tokenizer init failed", e);
    }

    const unlistenDragEnter = await listen(TauriEvent.DRAG_ENTER, () => isDragging = true);
    const unlistenDragLeave = await listen(TauriEvent.DRAG_LEAVE, () => isDragging = false);
    const unlistenDrop = await listen<{ paths: string[] }>(TauriEvent.DRAG_DROP, (event) => {
      isDragging = false;
      const paths = event.payload.paths;
      if (paths && paths.length > 0) {
        loadFile(paths[0]);
      }
    });

    return () => {
      unlistenDragEnter();
      unlistenDragLeave();
      unlistenDrop();
    };
  });

  async function loadFile(path: string) {
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
    try {
        const trans = await translateSentenceOllama(item.text);
        item.translatedText = trans;
    } catch(e) {
        console.error("Translation Error", e);
        item.translatedText = "번역 실패";
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
                <button class="btn-small" on:click={() => handleTranslateRow(item)} disabled={item.translatedText === "번역 중..."}>
                  {item.translatedText === "번역 중..." ? "번역 중" : "번역"}
                </button>
              </div>
              <div class="translate-column">
                <textarea 
                  class="trans-input" 
                  bind:value={item.translatedText} 
                  placeholder="번역된 내용이 여기에 표시됩니다..."
                  disabled={item.translatedText === "번역 중..."}
                ></textarea>
              </div>
            </div>
          {/each}
          {#if extractedStrings.length === 0}
            <p class="empty">이 스크립트에는 추출할 일본어 문자열이 없습니다.</p>
          {/if}
        </div>
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
    height: 100vh;
    padding: 1rem;
    gap: 1rem;
    position: relative;
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
      display: flex;
      background: rgba(15, 23, 42, 0.5);
      border: 1px solid var(--border-color);
      border-radius: 8px;
      overflow: hidden;
      transition: all 0.3s ease;

      &.translating {
        border-color: var(--accent-color);
        box-shadow: 0 0 12px rgba(59, 130, 246, 0.4);
        opacity: 0.9;
        
        .translate-column {
          background: rgba(59, 130, 246, 0.05);
        }
      }

      .original-column {
        flex: 1;
        padding: 1rem;
        border-right: 1px solid var(--border-color);
        display: flex;
        flex-direction: column;
        justify-content: space-between;
        gap: 0.5rem;

        .original-text {
          font-size: 1rem;
          line-height: 1.4;
          word-break: break-all;
        }
      }

      .translate-column {
        flex: 1;
        
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
      }
    }
  }
</style>
