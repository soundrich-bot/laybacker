<script>
  let {
    settings,
    onSettingsChange,
    tameMode = false,
    onToggleTame,
    timestampFormat = 'YYYYMMDD_HHmm',
    onTimestampFormatChange,
    proresProfile = 'lt',
    onProresProfileChange,
  } = $props();

  let showSettings = $state(false);

  function updateSetting(key, value) {
    onSettingsChange({ ...settings, [key]: value });
  }

  const formats = [
    { value: 'YYYYMMDD_HHmm',    label: '20260325_1430', desc: 'YYYYMMDD' },
    { value: 'YYYY-MM-DD_HH-mm', label: '2026-03-25_14-30', desc: 'YYYY-MM-DD' },
    { value: 'DD-MM-YYYY_HH-mm', label: '25-03-2026_14-30', desc: 'DD-MM-YYYY' },
    { value: 'MMDDYYYY_HHmm',    label: '03252026_1430', desc: 'MMDDYYYY' },
  ];

  const proresFlavors = [
    { value: 'proxy', label: 'ProRes 422 Proxy' },
    { value: 'lt',    label: 'ProRes 422 LT' },
    { value: '422',   label: 'ProRes 422' },
    { value: 'hq',    label: 'ProRes 422 HQ' },
  ];
</script>

<div class="settings-bar">
  <!-- Video codec -->
  <div class="setting-group">
    <span class="setting-label" title="Video codec — Original copies the stream, H.264 re-encodes">VIDEO</span>
    <div class="setting-options">
      <button
        class="option-btn"
        class:active={settings.videoCodec === 'original'}
        onclick={() => updateSetting('videoCodec', 'original')}
        title="Copy video stream as-is (fastest, no quality loss)"
      >
        ORIGINAL
      </button>
      <button
        class="option-btn"
        class:active={settings.videoCodec === 'h264'}
        onclick={() => updateSetting('videoCodec', 'h264')}
        title="Re-encode video as H.264 (smaller file, slower)"
      >
        H.264
      </button>
    </div>
  </div>

  <!-- Audio format -->
  <div class="setting-group">
    <span class="setting-label" title="Audio format — Original copies the audio stream, AAC re-encodes">AUDIO</span>
    <div class="setting-options">
      <button
        class="option-btn"
        class:active={settings.audioFormat === 'original'}
        onclick={() => updateSetting('audioFormat', 'original')}
        title="Copy audio stream as-is (fastest, no quality loss)"
      >
        ORIGINAL
      </button>
      <button
        class="option-btn"
        class:active={settings.audioFormat === 'aac'}
        onclick={() => updateSetting('audioFormat', 'aac')}
        title="Re-encode audio as AAC (smaller file)"
      >
        AAC
      </button>
    </div>
    {#if settings.audioFormat === 'aac'}
      <select
        class="bitrate-select"
        value={settings.aacBitrate}
        onchange={(e) => updateSetting('aacBitrate', parseInt(e.target.value))}
        title="AAC bitrate — higher is better quality"
      >
        <option value={128000}>128 kbps</option>
        <option value={192000}>192 kbps</option>
        <option value={256000}>256 kbps</option>
        <option value={320000}>320 kbps</option>
      </select>
    {/if}
  </div>

  <!-- Spacer -->
  <div class="settings-spacer"></div>

  <!-- Settings cog with dropdown -->
  <div class="cog-wrapper">
    <button
      class="cog-btn"
      class:open={showSettings}
      onclick={() => showSettings = !showSettings}
      title="Settings"
    >
      <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
        <path d="M6.7 1h2.6l.4 2.1a5.5 5.5 0 0 1 1.3.8l2-.8 1.3 2.2-1.7 1.3a5.6 5.6 0 0 1 0 1.5l1.7 1.3-1.3 2.2-2-.8a5.5 5.5 0 0 1-1.3.8L9.3 15H6.7l-.4-2.1a5.5 5.5 0 0 1-1.3-.8l-2 .8L1.7 10.7l1.7-1.3a5.6 5.6 0 0 1 0-1.5L1.7 6.6 3 4.4l2 .8a5.5 5.5 0 0 1 1.3-.8L6.7 1z" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round"/>
        <circle cx="8" cy="8" r="2" stroke="currentColor" stroke-width="1.2"/>
      </svg>
    </button>
    {#if showSettings}
      <div class="cog-dropdown">
        <button
          class="dropdown-item"
          class:active={tameMode}
          onclick={onToggleTame}
          title="Reduce neon intensity for sensitive eyes"
        >
          MY EYES
        </button>
        <div class="dropdown-divider"></div>
        <span class="dropdown-label">DATE FORMAT</span>
        {#each formats as fmt}
          <button
            class="dropdown-item"
            class:active={timestampFormat === fmt.value}
            onclick={() => onTimestampFormatChange(fmt.value)}
          >
            {fmt.label}
          </button>
        {/each}
        <div class="dropdown-divider"></div>
        <span class="dropdown-label" title="Codec for the ProRes button on each video">PRORES WORKING FILE</span>
        {#each proresFlavors as fl}
          <button
            class="dropdown-item"
            class:active={proresProfile === fl.value}
            onclick={() => onProresProfileChange(fl.value)}
          >
            {fl.label}
          </button>
        {/each}
      </div>
    {/if}
  </div>
</div>

<style>
  .settings-bar {
    padding: var(--gap-sm) var(--gap-lg);
    border-top: 1px solid var(--border-color);
    display: flex;
    flex-wrap: wrap;
    gap: var(--gap-lg);
    align-items: center;
    flex-shrink: 0;
    background: var(--bg-panel);
  }

  .setting-group {
    display: flex;
    align-items: center;
    gap: var(--gap-sm);
  }

  .setting-label {
    font-family: var(--font-display);
    font-size: 11px;
    letter-spacing: 0.15em;
    color: var(--text-muted);
    min-width: 50px;
    cursor: help;
  }

  .setting-options {
    display: flex;
    gap: 2px;
    background: var(--bg-dark);
    border-radius: var(--radius-sm);
    padding: 2px;
    border: 1px solid var(--border-color);
  }

  .option-btn {
    font-family: var(--font-mono);
    font-size: 12px;
    font-weight: 700;
    padding: 4px 12px;
    border: none;
    border-radius: 3px;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    transition: all 0.15s;
    letter-spacing: 0.05em;
  }

  .option-btn.active {
    background: var(--neon-pink);
    color: var(--bg-dark);
  }

  .option-btn:hover:not(.active) {
    color: var(--text-primary);
    background: var(--bg-raised);
  }

  .bitrate-select {
    margin-left: var(--gap-xs);
  }

  .settings-spacer {
    flex: 1;
  }

  .cog-wrapper {
    position: relative;
  }

  .cog-btn {
    background: none;
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    color: var(--text-muted);
    padding: 5px 7px;
    cursor: pointer;
    transition: all 0.15s;
    display: flex;
    align-items: center;
  }

  .cog-btn:hover {
    color: var(--text-secondary);
    border-color: var(--border-accent);
  }

  .cog-btn.open {
    color: var(--neon-cyan);
    border-color: rgba(8, 247, 254, 0.3);
  }

  :global(:root.tame) .cog-btn.open {
    border-color: rgba(90, 138, 122, 0.35);
  }

  .cog-dropdown {
    position: absolute;
    right: 0;
    bottom: calc(100% + 6px);
    background: var(--bg-raised);
    border: 1px solid var(--border-accent);
    border-radius: var(--radius-sm);
    padding: 4px;
    z-index: 10;
    white-space: nowrap;
    min-width: 160px;
  }

  .dropdown-label {
    display: block;
    font-family: var(--font-display);
    font-size: 9px;
    letter-spacing: 0.15em;
    color: var(--text-muted);
    padding: 4px 12px 2px;
    opacity: 0.6;
  }

  .dropdown-divider {
    height: 1px;
    background: var(--border-color);
    margin: 4px 8px;
  }

  .dropdown-item {
    display: block;
    width: 100%;
    text-align: left;
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.05em;
    padding: 5px 12px;
    border-radius: 3px;
    border: none;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    transition: all 0.15s;
  }

  .dropdown-item:hover {
    color: var(--text-secondary);
    background: var(--bg-panel-hover);
  }

  .dropdown-item.active {
    background: rgba(8, 247, 254, 0.08);
    color: var(--neon-cyan);
  }

  :global(:root.tame) .dropdown-item.active {
    background: rgba(90, 138, 122, 0.1);
  }
</style>
