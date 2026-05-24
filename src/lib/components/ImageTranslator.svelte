<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { open } from '@tauri-apps/plugin-dialog';
  import { onMount } from 'svelte';

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

  let selectedFolder = '';
  let images: ImageEntry[] = [];
  let selectedImage: ImageEntry | null = null;
  let isProcessing = false;
  let batchTotal = 0;
  let batchCompleted = 0;

  let ollamaModelName = 'gemma4:e4b';
  let totalImagesProcessed = 0;
  let showDebugPanel = false;
  let debugLogs: string[] = [];

  function addLog(msg: string) {
    const time = new Date().toLocaleTimeString();
    debugLogs = [`[${time}] ${msg}`, ...debugLogs];
  }

  onMount(() => {
    ollamaModelName = localStorage.getItem('ollamaModelName') || 'gemma4:e4b';
    totalImagesProcessed = parseInt(localStorage.getItem('totalImagesProcessed') || '0', 10);
    addLog("앱 초기화 완료. 물리 렌더링 모드 준비 완료. 모델명: " + ollamaModelName);
  });

  function saveModelName() {
    localStorage.setItem('ollamaModelName', ollamaModelName);
    alert('로컬 모델명이 저장되었습니다.');
    addLog("모델명 변경됨: " + ollamaModelName);
  }

  async function openFolder() {
    const selected = await open({
      directory: true,
      multiple: false
    });

    if (selected && typeof selected === 'string') {
      selectedFolder = selected;
      await loadImages(selected);
    }
  }

  async function loadImages(folder: string) {
    try {
      const paths: string[] = await invoke('get_images_in_folder', { folderPath: folder });
      images = paths.map(path => {
        const name = path.split('\\').pop()?.split('/').pop() || path;
        return {
          path,
          name,
          regions: [],
          status: 'pending'
        };
      });
      selectedImage = null;
      addLog(`${paths.length}개의 이미지를 불러왔습니다.`);
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

  async function processImageOllama(img: ImageEntry) {
    if (!ollamaModelName) {
      alert("Ollama 모델 이름을 입력해주세요!");
      return;
    }

    img.status = 'processing';
    images = [...images];
    if (selectedImage?.path === img.path) selectedImage = img;

    try {
      addLog(`[${img.name}] 1/3 로컬 파일 읽는 중...`);
      const bytes: number[] = await invoke('read_image_file', { path: img.path });
      const base64Data = bufferToBase64(bytes);

      const prompt = `Task: Translate Japanese game UI text to Korean.
1. Read ALL Japanese text in the image.
2. Translate the Japanese text into fluent Korean. DO NOT copy the Japanese text. You MUST translate it.
3. Find the bounding box coordinates [ymin, xmin, ymax, xmax] normalized to 0-1000 scale.
4. Return ONLY a valid JSON array.

Example output:
[
  {
    "ja_text": "未所持",
    "ko_text": "미보유",
    "box_2d": [750, 400, 800, 600]
  }
]

CRITICAL: "ko_text" MUST be in Korean (한국어). Do not leave it in Japanese.
Output ONLY the JSON array without any markdown or conversational text.`;

      addLog(`[${img.name}] 2/3 AI(${ollamaModelName})로 번역 요청...`);
      const startTime = performance.now();
      
      const response = await fetch("http://127.0.0.1:11434/api/generate", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          model: ollamaModelName,
          prompt: prompt,
          images: [base64Data],
          stream: false,
          format: "json"
        })
      });

      if (!response.ok) {
        throw new Error(`Ollama 통신 실패: ${response.status} ${response.statusText}`);
      }

      const result = await response.json();
      const endTime = performance.now();
      const responseText = result.response || "";
      addLog(`[${img.name}] 응답 수신 완료 (${((endTime-startTime)/1000).toFixed(1)}초). 원시 응답 텍스트:\n${responseText}`);
      
      let parsedRegions = [];
      try {
        const jsonMatch = responseText.match(/\[[\s\S]*\]|\{[\s\S]*\}/);
        const cleanedText = jsonMatch ? jsonMatch[0] : responseText;
        
        let jsonParsed = JSON.parse(cleanedText);
        
        if (!Array.isArray(jsonParsed) && typeof jsonParsed === 'object' && jsonParsed !== null) {
          jsonParsed = [jsonParsed];
        }
        
        if (Array.isArray(jsonParsed)) {
            parsedRegions = jsonParsed.map((item: any) => {
               let [ymin, xmin, ymax, xmax] = item.box_2d || [0, 0, 100, 100];
               
               ymin = Number(ymin); xmin = Number(xmin);
               ymax = Number(ymax); xmax = Number(xmax);

               return {
                 ja_text: item.ja_text || "",
                 ko_text: item.ko_text || "",
                 x: (xmin / 1000) * 100,
                 y: (ymin / 1000) * 100,
                 w: ((xmax - xmin) / 1000) * 100,
                 h: ((ymax - ymin) / 1000) * 100
               };
            });
            addLog(`[${img.name}] JSON 파싱 성공! ${parsedRegions.length}개 텍스트 찾음.`);
        }
        
        if (parsedRegions.length === 0) {
           throw new Error("텍스트를 찾지 못했거나 모델이 빈 배열을 반환했습니다.\n\n[모델 원시 응답]\n" + responseText);
        }
      } catch(parseErr) {
        console.error("JSON parsing error", responseText);
        addLog(`[${img.name}] JSON 파싱 실패! 에러: ` + String(parseErr));
        throw new Error("Ollama 모델이 올바른 JSON 규격(box_2d 포함)을 반환하지 않았습니다.\n\n[모델 원시 응답]\n" + responseText);
      }

      img.regions = parsedRegions;

      addLog(`[${img.name}] 3/3 번역 결과를 물리 이미지로 합성 및 저장하는 중...`);
      // 이미지 합성 및 저장 로직 호출 (Rust 백엔드)
      const dotIndex = img.path.lastIndexOf('.');
      const outputExt = dotIndex > -1 ? img.path.substring(dotIndex) : '.png';
      const outputBase = dotIndex > -1 ? img.path.substring(0, dotIndex) : img.path;
      const outputPath = `${outputBase}-translated${outputExt}`;

      await invoke('draw_and_save_image', {
        originalPath: img.path,
        outputPath: outputPath,
        regions: parsedRegions
      });

      img.translatedPath = outputPath;
      // UI 갱신을 위해 썸네일 캐시에 미리 빈 값 등록 (강제 리로드 효과)
      delete imageCache[outputPath];

      img.status = 'done';
      totalImagesProcessed++;
      localStorage.setItem('totalImagesProcessed', totalImagesProcessed.toString());
      addLog(`[${img.name}] 완료! 번역 이미지 저장됨: ${outputPath}`);
      
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
    batchTotal = pendingImages.length;
    batchCompleted = 0;

    for (const img of pendingImages) {
      await processImageOllama(img);
      batchCompleted++;
    }

    isProcessing = false;
    batchTotal = 0;
    batchCompleted = 0;
    alert("이미지 일괄 번역이 완료되었습니다!");
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
</script>

<div class="image-translator-container">
  <div class="top-bar">
    <button class="btn" on:click={openFolder} disabled={isProcessing}>
      이미지 폴더 선택
    </button>
    
    {#if selectedFolder}
      <span class="folder-path">{selectedFolder}</span>
    {/if}
    
    <div style="flex: 1;"></div>
    
    <div class="api-section">
      <div class="token-tracker">
        <span class="badge">누적 {totalImagesProcessed}장 처리</span>
        <span class="badge cost">100% 무료 로컬 구동 중 🚀</span>
      </div>
      <input type="text" bind:value={ollamaModelName} placeholder="Ollama 모델명 입력" class="api-input" />
      <button class="btn-small btn-save" on:click={saveModelName}>저장</button>
    </div>

    {#if isProcessing && batchTotal > 0}
      <span class="progress-text">
        {batchCompleted} / {batchTotal} 완료
      </span>
    {/if}
    
    <button class="btn btn-success" on:click={handleBatchTranslate} disabled={isProcessing || images.length === 0}>
      {isProcessing ? '로컬 일괄 처리 중...' : '이미지 일괄 번역'}
    </button>
  </div>

  <div class="main-workspace">
    <aside class="sidebar image-list glass-panel scrollbar-hidden">
      <h3>이미지 목록 ({images.length})</h3>
      {#if images.length === 0}
        <div class="empty-list">폴더를 선택해주세요.</div>
      {:else}
        <ul>
          {#each images as img}
            <li class:active={selectedImage?.path === img.path} on:click={() => selectImage(img)}>
              <div class="item-content">
                <span class="name" title={img.name}>{img.name}</span>
                <span class={`status-dot status-${img.status}`} title={getStatusLabel(img.status)}></span>
              </div>
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
            <button class="btn-small" on:click={() => processImageOllama(selectedImage)} disabled={isProcessing || !ollamaModelName}>
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
                  <button class="btn-small btn-copy" on:click={() => {
                    navigator.clipboard.writeText(selectedImage.errorMsg || '');
                    alert('에러 내용이 복사되었습니다.');
                  }}>에러 복사</button>
                </div>
                <div class="error-content">{selectedImage.errorMsg}</div>
              </div>
            {/if}
          </div>
        </div>
      {:else}
        <div class="empty-state"><p>왼쪽 목록에서 이미지를 선택해주세요.</p></div>
      {/if}

      <!-- 디버그 패널 토글 영역 -->
      <div class="debug-toggle-area" style="text-align: right; padding: 0.5rem;">
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
    </section>
  </div>
</div>

<style lang="scss">
  .image-translator-container {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  .top-bar {
    display: flex;
    align-items: center;
    gap: 1rem;
    padding: 1rem;
    margin-bottom: 1rem;
    background: rgba(15, 23, 42, 0.4);
    border-radius: 8px;
    border: 1px solid var(--border-color);
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
      
      li {
        padding: 0.75rem 1rem;
        cursor: pointer;
        border-radius: 6px;
        transition: all 0.2s ease;
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

        .text-overlay {
          position: absolute;
          display: flex;
          align-items: center;
          justify-content: center;
          background: rgba(0, 0, 0, 0.6);
          border-radius: 4px;
          overflow: hidden;
        }

        .ko-text {
          color: #ffffff;
          font-family: 'Malgun Gothic', 'Noto Sans KR', sans-serif;
          font-weight: 700;
          font-size: 100%;
          text-shadow: -1px -1px 0 #000, 1px -1px 0 #000, -1px 1px 0 #000, 1px 1px 0 #000;
          text-align: center;
          line-height: 1.1;
          white-space: pre-wrap;
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
</style>
