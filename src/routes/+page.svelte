<script lang="ts">
  import { onMount } from 'svelte';
  import { listen, TauriEvent } from '@tauri-apps/api/event';
  import Decompiler from '$lib/components/Decompiler.svelte';
  import ScriptTranslator from '$lib/components/ScriptTranslator.svelte';
  import ImageTranslator from '$lib/components/ImageTranslator.svelte';

  let activeTab: 'decompile' | 'script' | 'image' = $state('decompile');
  let isDragging = $state(false);

  // 컴포넌트 인스턴스 바인딩을 위한 변수
  let decompilerRef = $state<any>(null);
  let scriptTranslatorRef = $state<any>(null);
  let imageTranslatorRef = $state<any>(null);

  let showSettingsModal = $state(false);
  let confirmStagedLoad = $state(false);
  let translationConcurrency = $state(1);
  let enableGroupTranslation = $state(true);

  onMount(() => {
    confirmStagedLoad = localStorage.getItem('confirmStagedLoad') === 'true';
    enableGroupTranslation = localStorage.getItem('enableGroupTranslation') !== 'false';
    const savedConcurrency = localStorage.getItem('translationConcurrency');
    if (savedConcurrency) {
      translationConcurrency = parseInt(savedConcurrency, 10) || 1;
    }
    let unlistenDragEnter: (() => void) | undefined;
    let unlistenDragLeave: (() => void) | undefined;
    let unlistenDrop: (() => void) | undefined;

    // 윈도우 전역에서 드래그 앤 드롭을 단 한 번만 구독
    const setup = async () => {
      unlistenDragEnter = await listen(TauriEvent.DRAG_ENTER, () => {
        isDragging = true;
      });
      unlistenDragLeave = await listen(TauriEvent.DRAG_LEAVE, () => {
        isDragging = false;
      });
      unlistenDrop = await listen<{ paths: string[] }>(TauriEvent.DRAG_DROP, (event) => {
        isDragging = false;
        const paths = event.payload.paths;
        if (paths && paths.length > 0) {
          handleGlobalDrop(paths[0]);
        }
      });
    };

    setup();

    return () => {
      if (unlistenDragEnter) unlistenDragEnter();
      if (unlistenDragLeave) unlistenDragLeave();
      if (unlistenDrop) unlistenDrop();
    };
  });

  function changeTab(tab: 'decompile' | 'script' | 'image') {
    if (activeTab === 'decompile' && decompilerRef?.getStatus()?.isRunning) {
      alert("디컴파일 작업 중에는 다른 탭으로 이동할 수 없습니다.");
      return;
    }
    if (activeTab === 'script') {
      const status = scriptTranslatorRef?.getStatus();
      if (status?.isTranslating || status?.isLoading) {
        alert("번역 작업 중이거나 로딩 중에는 다른 탭으로 이동할 수 없습니다.");
        return;
      }
    }
    if (activeTab === 'image' && imageTranslatorRef?.getStatus()?.isProcessing) {
      alert("이미지 번역 작업 중에는 다른 탭으로 이동할 수 없습니다.");
      return;
    }
    activeTab = tab;
  }

  function handleGlobalDrop(path: string) {
    // 현재 활성화된 탭 컴포넌트의 핸들러만 트리거
    if (activeTab === 'decompile' && decompilerRef) {
      if (decompilerRef.getStatus()?.isRunning) {
        alert("작업 중에는 새로운 파일을 로드할 수 없습니다.");
        return;
      }
      decompilerRef.handleFileSelect(path);
    } else if (activeTab === 'script' && scriptTranslatorRef) {
      const status = scriptTranslatorRef.getStatus();
      if (status?.isTranslating || status?.isLoading) {
        alert("작업 중에는 새로운 파일을 로드할 수 없습니다.");
        return;
      }
      scriptTranslatorRef.loadFile(path);
    } else if (activeTab === 'image' && imageTranslatorRef) {
      if (imageTranslatorRef.getStatus()?.isProcessing) {
        alert("작업 중에는 새로운 파일을 로드할 수 없습니다.");
        return;
      }
      imageTranslatorRef.handleFileOrFolderSelect(path);
    }
  }

  function toggleConfirmStagedLoad() {
    localStorage.setItem('confirmStagedLoad', confirmStagedLoad ? 'true' : 'false');
  }

  function toggleEnableGroupTranslation() {
    localStorage.setItem('enableGroupTranslation', enableGroupTranslation ? 'true' : 'false');
  }

  function saveConcurrency() {
    localStorage.setItem('translationConcurrency', translationConcurrency.toString());
  }
</script>

<div class="app-layout">
  <nav class="tabs-nav glass-panel">
    <div class="tabs-container">
      <button 
        class="tab-btn" 
        class:active={activeTab === 'decompile'} 
        onclick={() => changeTab('decompile')}
      >
        <span class="icon">📦</span> RGSS3A 디컴파일
      </button>
      <button 
        class="tab-btn" 
        class:active={activeTab === 'script'} 
        onclick={() => changeTab('script')}
      >
        <span class="icon">📄</span> 스크립트 번역
      </button>
      <button 
        class="tab-btn" 
        class:active={activeTab === 'image'} 
        onclick={() => changeTab('image')}
      >
        <span class="icon">🖼️</span> 이미지 번역
      </button>
    </div>

    <button 
      class="settings-btn" 
      onclick={() => showSettingsModal = true}
      title="전체 설정"
    >
      <span class="icon">⚙️</span>
    </button>
  </nav>

  <!-- 설정 모달 -->
  {#if showSettingsModal}
    <div 
      class="modal-backdrop" 
      onclick={(e) => { if (e.target === e.currentTarget) showSettingsModal = false; }} 
      onkeydown={(e) => { if (e.key === 'Escape') showSettingsModal = false; }}
      role="button" 
      tabindex="-1"
    >
      <div class="modal-content glass-panel" role="dialog" aria-modal="true">
        <div class="modal-header">
          <h2>⚙️ 전체 설정</h2>
          <button class="close-btn" onclick={() => showSettingsModal = false}>✕</button>
        </div>
        <div class="modal-body">
          <div class="setting-item">
            <div class="setting-info">
              <span class="setting-title">임시 저장 발견 시 매번 확인</span>
              <span class="setting-desc">비활성화 시 임시 저장 파일(_staged.json)이 있을 때 묻지 않고 자동으로 이어서 작업합니다.</span>
            </div>
            <label class="switch" for="confirm-staged-load-checkbox">
              <input 
                id="confirm-staged-load-checkbox"
                type="checkbox" 
                bind:checked={confirmStagedLoad} 
                onchange={toggleConfirmStagedLoad}
              >
              <span class="slider round"></span>
            </label>
          </div>

          <div class="setting-item" style="border-top: 1px solid rgba(255, 255, 255, 0.05); padding-top: 1rem;">
            <div class="setting-info">
              <span class="setting-title">동시 번역 프로세스 개수 (병렬 처리)</span>
              <span class="setting-desc">일괄 번역 시 동시에 실행할 작업 개수입니다. 로컬 LLM 사양(VRAM)이 충분할 때만 값을 높이세요.</span>
            </div>
            <select 
              id="concurrency-select"
              bind:value={translationConcurrency}
              onchange={saveConcurrency}
              style="width: 100px; height: 32px; background: rgba(15, 23, 42, 0.6); border: 1px solid rgba(255, 255, 255, 0.15); border-radius: 4px; color: #f1f5f9; padding: 0 0.5rem; cursor: pointer;"
            >
              <option value={1}>1 (순차)</option>
              <option value={2}>2</option>
              <option value={3}>3</option>
              <option value={4}>4</option>
              <option value={5}>5</option>
            </select>
          </div>

          <div class="setting-item" style="border-top: 1px solid rgba(255, 255, 255, 0.05); padding-top: 1rem;">
            <div class="setting-info">
              <span class="setting-title">연속 대사 묶어서 번역 (컨텍스트 유지)</span>
              <span class="setting-desc">활성화 시 앞뒤 문맥(Context)을 파악하여 덩어리째 자연스럽게 번역합니다. 비활성화 시 각 대사를 개별적으로 번역합니다.</span>
            </div>
            <label class="switch" for="enable-group-translation-checkbox">
              <input 
                id="enable-group-translation-checkbox"
                type="checkbox" 
                bind:checked={enableGroupTranslation} 
                onchange={toggleEnableGroupTranslation}
              >
              <span class="slider round"></span>
            </label>
          </div>
        </div>
      </div>
    </div>
  {/if}

  <div class="tab-content">
    <!-- 드래그 오버레이 활성 탭 및 바인딩 전달 -->
    {#if activeTab === 'decompile'}
      <Decompiler 
        bind:this={decompilerRef} 
        isDragging={isDragging && activeTab === 'decompile'} 
      />
    {:else if activeTab === 'script'}
      <ScriptTranslator 
        bind:this={scriptTranslatorRef} 
        isDragging={isDragging && activeTab === 'script'} 
        enableGroupTranslation={enableGroupTranslation}
      />
    {:else if activeTab === 'image'}
      <ImageTranslator 
        bind:this={imageTranslatorRef} 
        isDragging={isDragging && activeTab === 'image'} 
      />
    {/if}
  </div>
</div>

<style lang="scss">
  :global(*) {
    box-sizing: border-box;
  }

  :global(body) {
    margin: 0;
    padding: 0;
    font-family: 'Pretendard', -apple-system, BlinkMacSystemFont, system-ui, Roboto, 'Helvetica Neue', 'Segoe UI', 'Apple SD Gothic Neo', 'Noto Sans KR', 'Malgun Gothic', 'Apple Color Emoji', 'Segoe UI Emoji', 'Segoe UI Symbol', sans-serif;
    background: linear-gradient(135deg, #0f172a, #1e293b);
    color: #f1f5f9;
    height: 100vh;
    overflow: hidden;
  }

  .app-layout {
    display: flex;
    flex-direction: column;
    height: 100vh;
    padding: 1rem;
    gap: 1rem;
  }

  .tabs-nav {
    display: flex;
    justify-content: center;
    position: relative;
    padding: 0.5rem;
    border-radius: 8px;
    border: 1px solid rgba(255, 255, 255, 0.1);
    background: rgba(15, 23, 42, 0.6);
  }

  .tabs-container {
    display: flex;
    gap: 0.5rem;
    background: rgba(0, 0, 0, 0.2);
    padding: 0.25rem;
    border-radius: 8px;
  }

  .settings-btn {
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid rgba(255, 255, 255, 0.1);
    color: var(--text-secondary);
    cursor: pointer;
    transition: all 0.25s cubic-bezier(0.4, 0, 0.2, 1);
    display: flex;
    align-items: center;
    justify-content: center;
    position: absolute;
    right: 1rem;
    top: 50%;
    transform: translateY(-50%);
    width: 38px;
    height: 38px;
    border-radius: 6px;

    .icon {
      font-size: 1.25rem;
      display: inline-block;
      transition: transform 0.25s cubic-bezier(0.4, 0, 0.2, 1);
    }

    &:hover {
      background: rgba(255, 255, 255, 0.12);
      border-color: rgba(255, 255, 255, 0.25);
      color: white;

      .icon {
        transform: rotate(45deg);
      }
    }
    
    &:active {
      transform: translateY(-50%) scale(0.92);
    }
  }

  /* 모달 스타일 */
  .modal-backdrop {
    position: fixed;
    top: 0;
    left: 0;
    width: 100vw;
    height: 100vh;
    background: rgba(15, 23, 42, 0.65);
    backdrop-filter: blur(8px);
    z-index: 1000;
    display: flex;
    align-items: center;
    justify-content: center;
    animation: fadeIn 0.2s ease-out;
  }

  .modal-content {
    background: rgba(30, 41, 59, 0.85);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 12px;
    width: 480px;
    max-width: 90%;
    padding: 1.5rem;
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.5);
    transform: translateY(0);
    animation: slideUp 0.25s cubic-bezier(0.16, 1, 0.3, 1);
    
    .modal-header {
      display: flex;
      justify-content: space-between;
      align-items: center;
      margin-bottom: 1.5rem;
      border-bottom: 1px solid rgba(255, 255, 255, 0.1);
      padding-bottom: 0.75rem;

      h2 {
        margin: 0;
        font-size: 1.25rem;
        font-weight: 600;
        color: #f8fafc;
      }

      .close-btn {
        background: transparent;
        border: none;
        color: #94a3b8;
        font-size: 1.2rem;
        cursor: pointer;
        transition: color 0.2s;

        &:hover {
          color: white;
        }
      }
    }
  }

  .modal-body {
    display: flex;
    flex-direction: column;
    gap: 1.25rem;
  }

  .setting-item {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0.5rem 0;

    .setting-info {
      display: flex;
      flex-direction: column;
      gap: 0.25rem;
      max-width: 80%;

      .setting-title {
        font-size: 0.95rem;
        font-weight: 500;
        color: #e2e8f0;
      }

      .setting-desc {
        font-size: 0.82rem;
        color: #94a3b8;
        line-height: 1.4;
      }
    }
  }

  /* 스위치 토글 디자인 */
  .switch {
    position: relative;
    display: inline-block;
    width: 44px;
    height: 24px;
    flex-shrink: 0;

    input {
      opacity: 0;
      width: 0;
      height: 0;
    }
  }

  .slider {
    position: absolute;
    cursor: pointer;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background-color: #475569;
    transition: .3s cubic-bezier(0.4, 0, 0.2, 1);
    border-radius: 24px;

    &::before {
      position: absolute;
      content: "";
      height: 18px;
      width: 18px;
      left: 3px;
      bottom: 3px;
      background-color: white;
      transition: .3s cubic-bezier(0.4, 0, 0.2, 1);
      border-radius: 50%;
      box-shadow: 0 1px 3px rgba(0,0,0,0.4);
    }
  }

  input:checked + .slider {
    background-color: #3b82f6;
  }

  input:focus + .slider {
    box-shadow: 0 0 1px #3b82f6;
  }

  input:checked + .slider::before {
    transform: translateX(20px);
  }

  @keyframes fadeIn {
    from { opacity: 0; }
    to { opacity: 1; }
  }

  @keyframes slideUp {
    from { transform: translateY(12px); opacity: 0; }
    to { transform: translateY(0); opacity: 1; }
  }

  .tab-btn {
    background: transparent;
    border: none;
    color: var(--text-secondary);
    padding: 0.5rem 1.5rem;
    border-radius: 6px;
    font-size: 0.95rem;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.2s ease;
    display: flex;
    align-items: center;
    gap: 0.5rem;

    &:hover {
      color: white;
    }

    &.active {
      background: var(--accent-color);
      color: white;
      box-shadow: 0 2px 4px rgba(0, 0, 0, 0.2);
    }
  }

  .tab-content {
    flex: 1;
    overflow: hidden;
    position: relative;
  }
</style>
