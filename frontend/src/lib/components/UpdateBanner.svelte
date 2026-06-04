<script>
  import { onMount } from 'svelte';
  import { check } from '@tauri-apps/plugin-updater';
  import { relaunch } from '@tauri-apps/plugin-process';

  let update = $state(null);       // the Update object from check()
  let version = $state('');
  let notes = $state('');
  let state = $state('hidden');     // hidden | available | downloading | ready | error
  let pct = $state(0);
  let showNotes = $state(false);

  onMount(async () => {
    // Wait a moment so it doesn't fight the app's own startup work.
    try {
      const u = await check();
      if (u && u.available) {
        update = u;
        version = u.version;
        notes = (u.body || '').trim();
        state = 'available';
      }
    } catch (e) {
      // Offline or no endpoint — updates are optional, fail silently.
      console.warn('[updater] check failed:', e);
    }
  });

  async function install() {
    if (!update) return;
    state = 'downloading';
    let total = 0;
    let got = 0;
    try {
      await update.downloadAndInstall((ev) => {
        switch (ev.event) {
          case 'Started':
            total = ev.data?.contentLength ?? 0;
            break;
          case 'Progress':
            got += ev.data?.chunkLength ?? 0;
            pct = total ? Math.min(100, Math.round((got / total) * 100)) : 0;
            break;
          case 'Finished':
            pct = 100;
            break;
        }
      });
      state = 'ready';
      await relaunch();
    } catch (e) {
      console.error('[updater] install failed:', e);
      state = 'error';
    }
  }

  function dismiss() {
    state = 'hidden';
  }
</script>

{#if state !== 'hidden'}
  <div class="update-bar" class:error={state === 'error'}>
    <div class="update-main">
      <span class="update-spark">✦</span>

      {#if state === 'available'}
        <span class="update-text">
          <b>Laybacker {version}</b> is available
          {#if notes}
            · <button class="link" onclick={() => showNotes = !showNotes}>{showNotes ? 'hide' : "what's new"}</button>
          {/if}
        </span>
        <div class="update-actions">
          <button class="btn-update" onclick={install}>UPDATE &amp; RESTART</button>
          <button class="btn-later" onclick={dismiss}>Later</button>
        </div>
      {:else if state === 'downloading'}
        <span class="update-text">Downloading update… {pct}%</span>
        <div class="dl-bar"><div class="dl-fill" style="width:{pct}%"></div></div>
      {:else if state === 'ready'}
        <span class="update-text">Update ready — restarting…</span>
      {:else if state === 'error'}
        <span class="update-text">Update failed. <a class="link" href="https://laybacker.com" target="_blank" rel="noreferrer">Download it manually →</a></span>
        <button class="btn-later" onclick={dismiss}>Dismiss</button>
      {/if}
    </div>

    {#if state === 'available' && showNotes && notes}
      <pre class="update-notes">{notes}</pre>
    {/if}
  </div>
{/if}

<style>
  .update-bar {
    flex-shrink: 0;
    background: linear-gradient(90deg, rgba(8,247,254,0.10), rgba(57,255,20,0.06));
    border-bottom: 1px solid rgba(8,247,254,0.3);
    padding: 8px var(--gap-lg);
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--text-primary);
  }
  .update-bar.error {
    background: rgba(255, 46, 99, 0.08);
    border-color: rgba(255, 46, 99, 0.3);
  }
  .update-main {
    display: flex;
    align-items: center;
    gap: var(--gap-sm);
  }
  .update-spark { color: var(--neon-cyan); }
  .update-text { flex: 1; min-width: 0; }
  .update-text b { color: var(--neon-cyan); }
  .update-actions { display: flex; align-items: center; gap: var(--gap-sm); flex-shrink: 0; }
  .btn-update {
    font-family: var(--font-display);
    font-size: 11px;
    letter-spacing: 0.08em;
    color: var(--bg-dark);
    background: var(--neon-cyan);
    border: none;
    border-radius: var(--radius-sm);
    padding: 5px 12px;
    cursor: pointer;
    font-weight: 700;
  }
  .btn-update:hover { filter: brightness(1.1); }
  .btn-later {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-muted);
    background: none;
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    padding: 4px 10px;
    cursor: pointer;
  }
  .btn-later:hover { color: var(--text-secondary); border-color: var(--border-accent); }
  .link {
    color: var(--neon-cyan);
    background: none;
    border: none;
    padding: 0;
    font: inherit;
    cursor: pointer;
    text-decoration: underline;
  }
  .dl-bar {
    width: 140px;
    height: 5px;
    background: var(--bg-dark);
    border-radius: 3px;
    overflow: hidden;
    flex-shrink: 0;
  }
  .dl-fill { height: 100%; background: var(--neon-cyan); transition: width 0.2s; }
  .update-notes {
    margin: 8px 0 2px;
    padding: 8px 10px;
    background: var(--bg-dark);
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    max-height: 140px;
    overflow-y: auto;
    white-space: pre-wrap;
    word-break: break-word;
    font-size: 11px;
    color: var(--text-secondary);
    line-height: 1.5;
  }
</style>
