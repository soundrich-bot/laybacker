<script>
  import { invoke } from '@tauri-apps/api/core';
  import { getVersion } from '@tauri-apps/api/app';
  import { onMount } from 'svelte';
  let { ffmpegStatus } = $props();
  let showAbout = $state(false);
  let showHelp = $state(false);
  let appVersion = $state('');

  onMount(async () => {
    try {
      appVersion = await getVersion();
    } catch {
      /* version display is non-critical */
    }
  });

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
    const subject = encodeURIComponent(`Laybacker Feedback v${appVersion}`);
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
<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
{#if showAbout}
  <div class="about-overlay" onclick={closeAbout}></div>
{/if}
<svelte:window onkeydown={(e) => { if (e.key === 'Escape' && showHelp) showHelp = false; }} />

{#if showHelp}
  <div class="about-overlay" onclick={() => showHelp = false}></div>
{/if}

<header class="header">
  <div class="title-row">
    <button class="logo-btn" onclick={toggleAbout}>
      <h1 class="logo">LAYBACKER</h1>
    </button>
    <span class="tagline">batch relay sound to video</span>

    {#if showAbout}
      <div class="about-panel">
        <div class="about-version">v{appVersion}</div>
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
        <div class="about-divider"></div>
        <div class="about-disclaimer">This software is provided "as is" without warranty of any kind. Soundrich Ltd. accepts no liability for data loss, file corruption, or incorrect output. Always verify your output files before delivery or distribution. You are solely responsible for ensuring your media meets the required specifications.</div>
      </div>
    {/if}
  </div>
  <div class="status-row">
    <div class="help-wrapper">
      <button class="help-btn" onclick={(e) => { e.stopPropagation(); showHelp = !showHelp; }} title="How to use Laybacker">
        <svg width="18" height="18" viewBox="0 0 18 18" fill="none">
          <circle cx="9" cy="9" r="8" stroke="currentColor" stroke-width="1.5"/>
          <text x="9" y="13" text-anchor="middle" fill="currentColor" font-size="12" font-weight="700" font-family="sans-serif">?</text>
        </svg>
      </button>
      {#if showHelp}
        <div class="help-panel">
          <div class="help-head">
            <div class="help-title">HOW IT WORKS</div>
            <button class="help-close" onclick={() => showHelp = false} title="Close (Esc)">✕ CLOSE</button>
          </div>
          <div class="help-divider"></div>
          <p class="help-intro">Laybacker lays sound back onto picture in batches, checks and normalises audio for delivery, adds slates, and never touches your originals.</p>
          <ol class="help-steps">
            <li><strong>Drop files</strong> — Drag video and audio files onto the window, all at once or one at a time. Laybacker pairs them by duration and filename. Drop one mix with several videos to lay it onto every video. Drop a video on its own to make a ProRes working file or add a slate to it.</li>
            <li><strong>Check the pairs</strong> — Each card shows a video + audio pairing with play buttons to preview. If a pair is wrong, remove it and re-drop.</li>
            <li><strong>Filenames</strong> — The Smart Filename blends both names with duplicate information removed. Beside it: <strong>RENAME</strong> to type your own, <strong>DATE</strong> to stamp the time, and <strong>NAME ▾</strong> to use the audio filename, the video filename, bump the version number (v3 → v4) or go back to the smart blend. <strong>NAME ALL BY</strong> in the column header applies one choice to the whole batch (it's per batch — <strong>✕ CANCEL</strong> undoes it). Set your usual style under <strong>DEFAULT FILENAME</strong> in the settings cog. A name you've edited stays put whatever else you change; only the extension follows the format.</li>
            <li><strong>Levels &amp; QC</strong> — The QC bar holds one spec for the batch. Pick the reference: <strong>LUFS</strong> (level to a loudness target, with dBTP as a ceiling) or <strong>PEAK</strong> (set every file's true peak to the dBTP value, loudness ignored). <strong>RUN QC</strong> measures every file and shows its LUFS and true peak on the card. <strong>NORM</strong> on a file, or the big <strong>NORMALIZE</strong> button, levels to that spec.</li>
            <li><strong>6 Fr (broadcast silence)</strong> — <strong>6 Fr</strong> on a file (you'll be asked to confirm) or <strong>6 Fr ALL</strong> forces 6 frames of digital silence at the head and tail, with a short fade to avoid clicks, as UK broadcasters require. Turn on the <strong>6 Fr</strong> check in the QC bar to have RUN QC flag files with sound in those regions.</li>
            <li><strong>When the lengths don't match</strong> — The card shows an orange <strong>AUDIO +/−</strong> pill. On layback you'll be asked what to do. Audio longer than picture: <strong>cut</strong> it, <strong>fade</strong> it out over the last 12 frames, or <strong>freeze</strong> the final frame while the sound plays out. Audio shorter than picture: start the sound at the first frame, or line it up with the <strong>end</strong> — right for a picture that already has a slate at the front. Pictures slated by Laybacker remember their slate length, so the right choice is pre-selected.</li>
            <li><strong>Slates</strong> — <strong>SLATE ALL</strong> puts a text card at the front of every video, silent underneath, with the sound starting on the first frame of programme. Choose from fifteen <strong>fonts</strong> and four text sizes, or <strong>CHOOSE IMAGE…</strong> to use your own picture or logo — <strong>FIT</strong> shows it whole (with a 5–100% scale and a position grid), <strong>FILL</strong> covers the frame, and <strong>TEXT</strong> puts the words at the top, middle or bottom — or just <strong>drag any line of text</strong> on the preview to place it exactly (it snaps to centre; RESET POSITIONS puts it back). For a picture that already carries its own slate, switch to <strong>TEXT OVER PICTURE</strong>: the words are laid over the opening seconds instead of being added to the front, so the runtime and the sound don't move and versions still line up for A/B. Scrub <strong>PREVIEW FRAME</strong> to check the text clears what's already on screen. On H.264 files (most camera and NLE exports) only the slate itself is encoded and the programme is copied untouched, so a slate takes seconds rather than minutes; other codecs re-encode in full. In card mode the duration is <em>slate + black</em>: add a run of black after the card, or use a <strong>PREROLL</strong> preset (5s = 4s slate + 1s black). Each card's <strong>SLATE</strong> button tweaks one file's wording; a video dropped on its own gets a <strong>Slate</strong> button that renders a slated copy next to it, keeping its own soundtrack. Slates re-encode the video, so a progress bar shows.</li>
            <li><strong>Format</strong> — <strong>ORIGINAL</strong> leaves the video and audio untouched (fastest, no loss). <strong>H.264</strong> and <strong>AAC</strong> make smaller files. Original audio gives a <code>.mov</code>; AAC gives an <code>.mp4</code>.</li>
            <li><strong>Audio Only page</strong> — Drop audio on its own and Laybacker waits for the video, so a WAVs-then-MOVs drop from two folders needs no clicks. If there is no video, press <strong>AUDIO ONLY →</strong> to open a page built for sound on its own, in three sections. <strong>QC</strong> runs every check on every file and lays the results out in a table — level against your LUFS or true-peak spec, stereo & phase (dual mono, one-sided, anti-phase), 6 frames of silence at head and tail, <strong>clicks</strong> (edit pops, dropouts, pops at the head or tail), <strong>clipping</strong> and <strong>dropouts</strong>. Nothing to choose: read the list and disregard whatever doesn't apply to the job. Clicks are reported as <em>possible</em>, with a time for each: click one to listen back, since a sharp real transient can look like one. Three sensitivities. Clipping means runs pinned at full scale, or the flat tops left by audio clipped upstream and turned down afterwards; dropouts are digital silence of 50 ms or more inside the programme (silence at the head and tail is not a dropout). Both list where, with the same listen-back. <strong>FILE DELIVERABLES</strong> render new files to a spec, on every file, right now: <strong>NORMALISE ALL</strong>, <strong>6 Fr ALL</strong>, <strong>CLOCK ALL</strong> (10 seconds of silence at the head and 5 at the tail) and <strong>CONVERT ALL</strong> to the format, sample rate and bit depth set in the output bar (WAV, AIFF, FLAC, ALAC or AAC; 44.1k / 48k / 96k; 16, 24 or 32-bit float). <strong>FILE PROCESSING</strong> changes the file's shape: split a multichannel file into named mono stems (_L, _R, _C, _LFE, _Ls, _Rs), join a mono set back into a stereo, 5.1 or 7.1 file, fold surround to stereo or anything to mono, fade the ends, or trim head-and-tail silence. New files are always written beside the originals and the list reloads with them, re-measured. <strong>MULTIFUNCTION CHAIN</strong> runs one ordered set of steps on every file: shape (fold, trim, fade, split) → QC → 6 frames of silence and loudness, each set to check only, prompt-to-fix or fix-without-asking (a file that passes is never touched) → unfixable issues (dual mono, anti-phase, one-sided, a peak that can't meet the ceiling at target) which warn and let you pass or skip → clock → export file type → rename, including a custom pattern built from tokens like <code>{'{name}'}_{'{date}'}</code>. When a step needs a decision the run pauses and asks, with a tick box to answer once for the whole batch. Save a chain as a named preset and recall it from the PRESET menu. After a run, EXPORT REPORT saves a CSV of what happened to every file, with levels before and after. <strong>← LAYBACK</strong> takes you back to the layback page.</li>
            <li><strong>ProRes (working file)</strong> — On any video, <strong>ProRes</strong> makes an Apple ProRes 422 <code>.mov</code> copy next to the source — a smooth-playing guide picture for Pro Tools. It doesn't change your export. Choose the flavour (Proxy / LT / 422 / HQ) in the settings cog.</li>
            <li><strong>Where files go</strong> — Outputs are saved beside your audio files unless you pick a folder under <strong>SAVE TO</strong> in the settings cog. Your format, QC spec, default filename style and save folder are remembered between sessions.</li>
            <li><strong>Layback</strong> — Hit the green button. Each card shows progress, then <strong>SHOW</strong> to reveal the finished file in Finder. Laybacker always writes a new file; your originals are never touched.</li>
          </ol>
          <div class="help-divider"></div>
          <div class="help-note">All processing happens locally on your machine using FFmpeg. Nothing is uploaded anywhere.</div>
        </div>
      {/if}
    </div>
    {#if ffmpegStatus.available}
      <span class="status-badge good" title={ffmpegStatus.version}>FFmpeg OK</span>
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
    font-size: 29px;
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
    font-size: 14px;
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
    font-size: 12px;
    color: var(--text-secondary);
    letter-spacing: 0.03em;
  }

  .about-note {
    font-family: var(--font-body);
    font-size: 12px;
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
    font-size: 11px;
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

  :global(:root.tame) .donate-btn {
    background: rgba(224, 122, 95, 0.1);
    border-color: rgba(224, 122, 95, 0.35);
  }

  :global(:root.tame) .donate-btn:hover {
    background: rgba(224, 122, 95, 0.18);
    box-shadow: none;
  }

  .feedback-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    width: 100%;
    font-family: var(--font-display);
    font-size: 11px;
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

  :global(:root.tame) .feedback-btn {
    background: rgba(90, 138, 122, 0.08);
    border-color: rgba(90, 138, 122, 0.3);
  }

  :global(:root.tame) .feedback-btn:hover {
    background: rgba(90, 138, 122, 0.15);
    box-shadow: none;
  }

  .about-tech {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-muted);
    opacity: 0.6;
    letter-spacing: 0.05em;
  }

  .about-disclaimer {
    font-family: var(--font-body);
    font-size: 10px;
    color: var(--text-muted);
    line-height: 1.5;
    opacity: 0.5;
  }

  .tagline {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--text-muted);
    letter-spacing: 0.1em;
    text-transform: lowercase;
  }

  .status-row {
    display: flex;
    align-items: center;
    gap: var(--gap-sm);
  }

  .help-wrapper {
    position: relative;
  }

  .help-btn {
    background: none;
    border: none;
    color: var(--text-muted);
    cursor: pointer;
    padding: 2px;
    display: flex;
    align-items: center;
    transition: all 0.15s;
  }

  .help-btn:hover {
    color: var(--neon-cyan);
  }

  /* The help is a centred, scrollable sheet — it outgrew the little dropdown
     it started as, and was taller than the window with nowhere to scroll. */
  .help-panel {
    position: fixed;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    width: min(680px, calc(100vw - 48px));
    max-height: min(82vh, calc(100vh - 72px));
    overflow-y: auto;
    overscroll-behavior: contain;
    background: var(--bg-raised);
    border: 1px solid var(--border-accent);
    border-radius: var(--radius-md);
    padding: 18px 24px 22px;
    z-index: 300;
    display: flex;
    flex-direction: column;
    gap: 8px;
    box-shadow: 0 16px 48px rgba(0, 0, 0, 0.5);
    text-align: left;
    cursor: default;
  }

  .help-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    position: sticky;
    top: -18px; /* stays put while the steps scroll beneath it */
    margin: -18px -24px 0;
    padding: 18px 24px 8px;
    background: var(--bg-raised);
    z-index: 1;
  }

  .help-close {
    font-family: var(--font-mono);
    font-size: 12px;
    font-weight: 700;
    color: var(--text-muted);
    background: var(--cap-face);
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    padding: 3px 9px;
    cursor: pointer;
    transition: all 0.15s;
    box-shadow: var(--cap-shadow);
  }
  .help-close:hover { color: var(--text-primary); border-color: var(--neon-cyan); }
  .help-close:active { transform: translateY(1px); box-shadow: var(--cap-shadow-pressed); }

  .help-title {
    font-family: var(--font-display);
    font-size: 13px;
    letter-spacing: 0.12em;
    color: var(--neon-cyan);
  }

  .help-divider {
    height: 1px;
    background: var(--border-color);
    margin: 2px 0;
  }

  .help-intro {
    font-family: var(--font-body);
    font-size: 14px;
    color: var(--text-primary);
    margin: 0 0 12px 0;
    font-weight: 600;
  }

  .help-steps {
    font-family: var(--font-body);
    font-size: 13px;
    color: var(--text-secondary);
    line-height: 1.5;
    padding-left: 18px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin: 0;
  }

  .help-steps li strong {
    color: var(--text-primary);
  }

  .help-note {
    font-family: var(--font-body);
    font-size: 12px;
    color: var(--text-muted);
    line-height: 1.4;
  }

  .status-badge {
    font-family: var(--font-mono);
    font-size: 11px;
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

  :global(:root.tame) .status-badge.good {
    background: rgba(106, 154, 90, 0.15);
    border-color: rgba(106, 154, 90, 0.3);
  }

  :global(:root.tame) .status-badge.bad {
    background: rgba(224, 122, 95, 0.15);
    border-color: rgba(224, 122, 95, 0.3);
  }

  :global(:root.tame) .about-panel,
  :global(:root.tame) .help-panel {
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.08);
  }

</style>
