<script>
  import { invoke } from '@tauri-apps/api/core';
  let { ffmpegStatus } = $props();
  let showAbout = $state(false);

  const DONATE_URL = 'https://monzo.com/pay/r/soundrich-limited_2qhNYp1kvgAICv';
  const FEEDBACK_EMAIL = 'soundrich+laybacker@gmail.com';

  function toggleAbout(e) {
    e.stopPropagation();
    showAbout = !showAbout;
  }

  function closeAbout() {
    showAbout = false;
  }

  async function donate(e) {
    e.stopPropagation();
    try {
      await invoke('open_url', { url: DONATE_URL });
    } catch {
      window.location.href = DONATE_URL;
    }
  }

  async function sendFeedback(e) {
    e.stopPropagation();
    const subject = encodeURIComponent('Laybacker Feedback v0.1.0');
    const url = `mailto:${FEEDBACK_EMAIL}?subject=${subject}`;
    try {
      await invoke('open_url', { url });
    } catch {
      window.location.href = url;
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
{#if showAbout}
  <div class="about-overlay" onclick={closeAbout}></div>
{/if}

<header class="header">
  <div class="title-row">
    <button class="logo-btn" onclick={toggleAbout}>
      <h1 class="logo">LAYBACKER</h1>
    </button>
    <span class="tagline">batch relay sound to video</span>

    {#if showAbout}
      <div class="about-panel">
        <div class="about-version">v0.1.0</div>
        <div class="about-divider"></div>
        <div class="about-copy">&copy; 2026 Soundrich Ltd.</div>
        <div class="about-note">All media is processed locally on your machine. Nothing is uploaded.</div>
        <div class="about-divider"></div>
        <button class="donate-btn" onclick={donate}>
          <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
            <path d="M7 1.5C4 1.5 1.5 4.5 7 8.5C12.5 4.5 10 1.5 7 1.5Z" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"/>
          </svg>
          BUY US A COFFEE
        </button>
        <button class="feedback-btn" onclick={sendFeedback}>
          <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
            <path d="M2 2h10v7H5l-3 3V2z" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"/>
          </svg>
          SEND FEEDBACK
        </button>
        <div class="about-tech">Built with Tauri + Svelte + FFmpeg</div>
      </div>
    {/if}
  </div>
  <div class="status-row">
    {#if ffmpegStatus.available}
      <span class="status-badge good">FFmpeg OK</span>
    {:else}
      <span class="status-badge bad">FFmpeg Missing</span>
    {/if}
  </div>
</header>

<style>
  .header {
    padding: var(--gap-md) var(--gap-lg);
    border-bottom: 2px solid var(--neon-pink);
    background: linear-gradient(180deg, var(--bg-raised) 0%, var(--bg-dark) 100%);
    display: flex;
    justify-content: space-between;
    align-items: center;
    flex-shrink: 0;
  }

  .title-row {
    display: flex;
    align-items: baseline;
    gap: var(--gap-md);
    position: relative;
  }

  .logo-btn {
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
    outline: none;
  }

  .logo-btn:hover .logo {
    filter: brightness(1.3);
  }

  .logo {
    font-family: var(--font-display);
    font-size: 28px;
    letter-spacing: 0.15em;
    color: var(--neon-pink);
    margin: 0;
    line-height: 1;
    transition: opacity 0.2s;
  }

  /* Neon glow only in dark mode */
  :global(:root:not(.tame)) .logo {
    text-shadow:
      0 0 10px rgba(255, 46, 99, 0.5),
      0 0 40px rgba(255, 46, 99, 0.2);
  }

  .about-overlay {
    position: fixed;
    inset: 0;
    z-index: 99;
  }

  .about-panel {
    position: absolute;
    top: calc(100% + 12px);
    left: 0;
    background: var(--bg-raised);
    border: 1px solid var(--border-accent);
    border-radius: var(--radius-md);
    padding: 16px 20px;
    z-index: 100;
    min-width: 260px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.15);
  }

  .about-version {
    font-family: var(--font-display);
    font-size: 13px;
    letter-spacing: 0.12em;
    color: var(--neon-cyan);
  }

  .about-divider {
    height: 1px;
    background: var(--border-color);
    margin: 2px 0;
  }

  .about-copy {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-secondary);
    letter-spacing: 0.03em;
  }

  .about-note {
    font-family: var(--font-body);
    font-size: 11px;
    color: var(--text-muted);
    line-height: 1.4;
  }

  .donate-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    width: 100%;
    font-family: var(--font-display);
    font-size: 10px;
    letter-spacing: 0.12em;
    color: var(--neon-pink);
    background: rgba(255, 46, 99, 0.08);
    border: 1px solid rgba(255, 46, 99, 0.3);
    border-radius: var(--radius-sm);
    padding: 8px 12px;
    cursor: pointer;
    transition: all 0.15s;
  }

  .donate-btn:hover {
    background: rgba(255, 46, 99, 0.15);
    border-color: var(--neon-pink);
    box-shadow: 0 0 10px rgba(255, 46, 99, 0.15);
  }

  .feedback-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    width: 100%;
    font-family: var(--font-display);
    font-size: 10px;
    letter-spacing: 0.12em;
    color: var(--neon-cyan);
    background: rgba(0, 255, 255, 0.06);
    border: 1px solid rgba(0, 255, 255, 0.25);
    border-radius: var(--radius-sm);
    padding: 8px 12px;
    cursor: pointer;
    transition: all 0.15s;
  }

  .feedback-btn:hover {
    background: rgba(0, 255, 255, 0.12);
    border-color: var(--neon-cyan);
    box-shadow: 0 0 10px rgba(0, 255, 255, 0.12);
  }

  .about-tech {
    font-family: var(--font-mono);
    font-size: 9px;
    color: var(--text-muted);
    opacity: 0.6;
    letter-spacing: 0.05em;
  }

  .tagline {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-muted);
    letter-spacing: 0.1em;
    text-transform: lowercase;
  }

  .status-row {
    display: flex;
    align-items: center;
    gap: var(--gap-sm);
  }

  .status-badge {
    font-family: var(--font-mono);
    font-size: 10px;
    padding: 3px 8px;
    border-radius: var(--radius-sm);
    letter-spacing: 0.05em;
    text-transform: uppercase;
    font-weight: 700;
  }

  .status-badge.good {
    background: rgba(57, 255, 20, 0.15);
    color: var(--neon-green);
    border: 1px solid rgba(57, 255, 20, 0.3);
  }

  .status-badge.bad {
    background: rgba(255, 46, 99, 0.15);
    color: var(--neon-pink);
    border: 1px solid rgba(255, 46, 99, 0.3);
  }

</style>
