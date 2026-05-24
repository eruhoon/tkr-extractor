<script lang="ts">
  import ScriptTranslator from '$lib/components/ScriptTranslator.svelte';
  import ImageTranslator from '$lib/components/ImageTranslator.svelte';

  let activeTab: 'script' | 'image' = 'script';
</script>

<div class="app-layout">
  <nav class="tabs-nav glass-panel">
    <div class="tabs-container">
      <button 
        class="tab-btn" 
        class:active={activeTab === 'script'} 
        on:click={() => activeTab = 'script'}
      >
        <span class="icon">📄</span> 스크립트 번역
      </button>
      <button 
        class="tab-btn" 
        class:active={activeTab === 'image'} 
        on:click={() => activeTab = 'image'}
      >
        <span class="icon">🖼️</span> 이미지 번역
      </button>
    </div>
  </nav>

  <div class="tab-content">
    {#if activeTab === 'script'}
      <ScriptTranslator />
    {:else if activeTab === 'image'}
      <ImageTranslator />
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
