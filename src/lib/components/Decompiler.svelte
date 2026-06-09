<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen, TauriEvent } from '@tauri-apps/api/event';
  import { open as openDialog } from '@tauri-apps/plugin-dialog';

  let { isDragging = false } = $props();

  let inputPath = $state('');
  let outputPath = $state('');
  let isRunning = $state(false);
  let currentFile = $state('');
  let currentCount = $state(0);
  let totalFiles = $state(0);
  let logs = $state<string[]>([]);
  let logContainer: HTMLDivElement;

  interface ProgressPayload {
    file_name: string;
    current: number;
    total: number;
  }

  onMount(() => {
    let unlistenProgress: (() => void) | undefined;

    // Rust 백엔드로부터 실시간 진행 상황 이벤트 수신
    const setup = async () => {
      unlistenProgress = await listen<ProgressPayload>('decompile-progress', (event) => {
        const { file_name, current, total } = event.payload;
        currentFile = file_name;
        currentCount = current;
        totalFiles = total;
        addLog(`[${current}/${total}] 추출됨: ${file_name}`);
      });
    };

    setup();

    return () => {
      if (unlistenProgress) unlistenProgress();
    };
  });

  export function handleFileSelect(path: string) {
    if (isRunning) {
      alert("디컴파일 작업 중에는 파일을 변경할 수 없습니다.");
      return;
    }
    const lowerPath = path.toLowerCase();
    if (!lowerPath.endsWith('.rgss3a') && !lowerPath.endsWith('.rgssad') && !lowerPath.endsWith('.rgss2a')) {
      alert("RPG Maker 아카이브 파일(.rgss3a, .rgssad, .rgss2a)만 선택할 수 있습니다.");
      return;
    }
    inputPath = path;
    // 기본 아웃풋 폴더는 파일명_Extracted 형태로 유도
    const extIdx = path.lastIndexOf('.');
    if (extIdx !== -1) {
      outputPath = path.substring(0, extIdx) + '_Extracted';
    } else {
      outputPath = path + '_Extracted';
    }
  }

  async function browseFile() {
    const selected = await openDialog({
      multiple: false,
      filters: [{
        name: 'RPG Maker Archive',
        extensions: ['rgss3a', 'rgssad', 'rgss2a']
      }]
    });

    if (selected && typeof selected === 'string') {
      handleFileSelect(selected);
    }
  }

  async function browseOutputDir() {
    const selected = await openDialog({
      directory: true,
      multiple: false,
    });

    if (selected && typeof selected === 'string') {
      outputPath = selected;
    }
  }

  function addLog(message: string) {
    logs = [...logs, message];
    // 로그 추가 시 하단 스크롤 유지
    if (logContainer) {
      setTimeout(() => {
        logContainer.scrollTop = logContainer.scrollHeight;
      }, 30);
    }
  }

  async function startDecompile() {
    if (!inputPath || !outputPath) {
      alert("아카이브 파일과 추출할 경로를 선택해주세요.");
      return;
    }

    isRunning = true;
    currentFile = '';
    currentCount = 0;
    totalFiles = 0;
    logs = [];
    addLog("🕒 디컴파일 작업을 준비 중입니다...");
    addLog(`입력 파일: ${inputPath}`);
    addLog(`출력 경로: ${outputPath}`);

    try {
      await invoke('prevent_sleep');
      await invoke('decompile_rgss3a', { inputPath, outputPath });
      addLog("🎉 디컴파일이 성공적으로 완료되었습니다!");
      addLog(`결과 파일들이 '${outputPath}'에 안전하게 저장되었습니다.`);
    } catch (e) {
      addLog(`❌ [에러 발생] 디컴파일 실패: ${e}`);
      alert(`디컴파일 중 오류가 발생했습니다: ${e}`);
    } finally {
      await invoke('allow_sleep');
      isRunning = false;
    }
  }

  async function openOutputFolder() {
    if (!outputPath) return;
    try {
      // @tauri-apps/plugin-opener 대신 Rust 백엔드 open_folder 커맨드 사용
      await invoke('open_folder', { path: outputPath });
    } catch (e) {
      alert("폴더를 열 수 없습니다: " + e);
    }
  }

  let progressPercent = $derived(totalFiles > 0 ? Math.round((currentCount / totalFiles) * 100) : 0);

  export function getStatus() {
    return { isRunning };
  }
</script>

<div class="app-container" class:drag-active={isDragging}>
  {#if isDragging}
    <div class="drag-overlay">
      <h2>Drop .rgss3a / .rgssad / .rgss2a here</h2>
    </div>
  {/if}

  <header class="glass-panel header">
    <div class="header-actions">
      <button class="btn btn-success" onclick={startDecompile} disabled={!inputPath || isRunning}>
        {isRunning ? '디컴파일 중...' : '디컴파일 시작'}
      </button>
    </div>
  </header>

  <main class="main-content">
    <!-- 입력 및 제어 패널 -->
    <section class="control-panel glass-panel">
      <div class="form-group">
        <label for="archive-file">아카이브 파일 (.rgss3a)</label>
        <div class="input-with-button">
          <input 
            type="text" 
            id="archive-file" 
            placeholder="파일을 드래그해서 놓거나 선택하세요..." 
            bind:value={inputPath}
            readonly
          />
          <button class="btn btn-primary" onclick={browseFile} disabled={isRunning}>
            찾아보기
          </button>
        </div>
      </div>

      <div class="form-group">
        <label for="output-dir">추출 폴더 경로</label>
        <div class="input-with-button">
          <input 
            type="text" 
            id="output-dir" 
            placeholder="추출한 리소스가 저장될 폴더 경로..." 
            bind:value={outputPath}
            readonly
          />
          <button class="btn btn-primary" onclick={browseOutputDir} disabled={isRunning}>
            찾아보기
          </button>
        </div>
      </div>

      <!-- 진행률 표시 영역 -->
      {#if isRunning || totalFiles > 0}
        <div class="progress-section">
          <div class="progress-info">
            <span class="status-text">
              {#if isRunning && currentCount < totalFiles}
                파일 추출 중...
              {:else if currentCount === totalFiles && totalFiles > 0}
                추출 완료!
              {:else}
                대기 중
              {/if}
            </span>
            <span class="progress-ratio">{currentCount} / {totalFiles} ({progressPercent}%)</span>
          </div>

          <div class="progress-track">
            <div class="progress-bar" style="width: {progressPercent}%"></div>
          </div>

          {#if currentFile}
            <div class="current-file-text" title={currentFile}>
              현재: {currentFile}
            </div>
          {/if}
        </div>
      {/if}

      <!-- 디컴파일 완료 후 빠른 열기 버튼 -->
      {#if !isRunning && totalFiles > 0 && currentCount === totalFiles}
        <div class="success-actions">
          <button class="btn btn-folder-open" onclick={openOutputFolder}>
            📂 추출된 폴더 열기
          </button>
        </div>
      {/if}
    </section>

    <!-- 실시간 로그 뷰어 -->
    <section class="log-panel glass-panel">
      <h3>작업 로그</h3>
      <div class="log-console scrollbar-hidden" bind:this={logContainer}>
        {#each logs as log}
          <div class="log-line" class:error={log.startsWith('❌')} class:success={log.startsWith('🎉') || log.startsWith('=== 디컴파일 성공')}>
            {log}
          </div>
        {/each}
        {#if logs.length === 0}
          <div class="log-empty">대기 중... 디컴파일이 시작되면 상세 로그가 여기에 표시됩니다.</div>
        {/if}
      </div>
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
    position: relative;
    z-index: 10;

    .header-actions {
      display: flex;
      gap: 1rem;
    }
  }

  .btn {
    border-radius: 6px;
    padding: 0.6rem 1.2rem;
    font-weight: 500;
    cursor: pointer;
    border: none;
    transition: all 0.2s ease;

    &:disabled {
      opacity: 0.5;
      cursor: not-allowed;
    }
  }

  .btn-primary {
    background: rgba(59, 130, 246, 0.8);
    color: white;
    &:hover:not(:disabled) {
      background: rgba(59, 130, 246, 1);
    }
  }

  .btn-success {
    background: #10b981;
    color: white;
    font-size: 1rem;
    box-shadow: 0 4px 12px rgba(16, 185, 129, 0.2);
    &:hover:not(:disabled) {
      background: #059669;
      box-shadow: 0 4px 16px rgba(16, 185, 129, 0.4);
    }
  }

  .btn-folder-open {
    background: rgba(245, 158, 11, 0.2);
    border: 1px solid rgba(245, 158, 11, 0.4);
    color: #fbbf24;
    width: 100%;
    text-align: center;
    padding: 0.8rem;
    font-weight: 600;
    margin-top: 1rem;
    
    &:hover {
      background: rgba(245, 158, 11, 0.3);
      border-color: rgba(245, 158, 11, 0.6);
    }
  }

  .main-content {
    display: flex;
    flex: 1;
    gap: 1rem;
    min-height: 0;
  }

  .control-panel {
    flex: 1;
    display: flex;
    flex-direction: column;
    padding: 2rem;
    gap: 1.5rem;
    overflow-y: auto;
  }

  .form-group {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;

    label {
      font-size: 0.9rem;
      font-weight: 500;
      color: #94a3b8;
    }

    .input-with-button {
      display: flex;
      gap: 0.5rem;

      input {
        flex: 1;
        background: rgba(0, 0, 0, 0.2);
        border: 1px solid rgba(255, 255, 255, 0.1);
        border-radius: 6px;
        padding: 0.6rem 1rem;
        color: white;
        font-size: 0.95rem;

        &:focus {
          outline: none;
          border-color: var(--accent-color);
        }
      }
    }
  }

  .progress-section {
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
    margin-top: 1rem;
    background: rgba(255, 255, 255, 0.02);
    border: 1px solid rgba(255, 255, 255, 0.05);
    border-radius: 8px;
    padding: 1.2rem;

    .progress-info {
      display: flex;
      justify-content: space-between;
      font-size: 0.85rem;
      
      .status-text {
        font-weight: 600;
        color: var(--accent-color);
      }
      .progress-ratio {
        color: #94a3b8;
      }
    }

    .progress-track {
      height: 8px;
      background: rgba(0, 0, 0, 0.3);
      border-radius: 4px;
      overflow: hidden;
    }

    .progress-bar {
      height: 100%;
      background: linear-gradient(90deg, #60a5fa, #a78bfa);
      border-radius: 4px;
      transition: width 0.1s ease;
    }

    .current-file-text {
      font-size: 0.8rem;
      color: #64748b;
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
      margin-top: 0.2rem;
    }
  }

  .log-panel {
    flex: 1.2;
    display: flex;
    flex-direction: column;
    padding: 1.5rem;
    min-height: 0;

    h3 {
      font-size: 1.1rem;
      font-weight: 600;
      margin-bottom: 1rem;
      color: #94a3b8;
      border-bottom: 1px solid rgba(255, 255, 255, 0.05);
      padding-bottom: 0.5rem;
    }
  }

  .log-console {
    flex: 1;
    background: rgba(0, 0, 0, 0.4);
    border: 1px solid rgba(255, 255, 255, 0.05);
    border-radius: 8px;
    padding: 1rem;
    font-family: 'Fira Code', 'Courier New', Courier, monospace;
    font-size: 0.85rem;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    min-height: 0;

    .log-line {
      color: #cbd5e1;
      line-height: 1.4;
      word-break: break-all;

      &.success {
        color: #34d399; /* Green */
      }
      &.error {
        color: #f87171; /* Red */
      }
    }

    .log-empty {
      color: #64748b;
      text-align: center;
      margin-top: auto;
      margin-bottom: auto;
      font-style: italic;
    }
  }
</style>
