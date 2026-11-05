<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";

  type Phase = "idle" | "recording" | "transcribing" | "pasting" | "error";

  let phase = $state<Phase>("idle");
  let message = $state<string | null>(null);
  let startedAt = $state(0);
  let now = $state(Date.now());

  const elapsed = $derived(
    phase === "recording" ? Math.max(0, Math.floor((now - startedAt) / 1000)) : 0,
  );
  const timer = $derived(
    `${Math.floor(elapsed / 60)}:${String(elapsed % 60).padStart(2, "0")}`,
  );

  onMount(() => {
    invoke<Phase>("current_phase").then((p) => (phase = p));
    const tick = setInterval(() => (now = Date.now()), 250);
    const unlisten = listen<{ phase: Phase; message: string | null }>("state", (e) => {
      if (e.payload.phase === "recording" && phase !== "recording") startedAt = Date.now();
      phase = e.payload.phase;
      message = e.payload.message;
    });
    return () => {
      clearInterval(tick);
      unlisten.then((f) => f());
    };
  });
</script>

<div class="pill" data-phase={phase}>
  <div class="indicator">
    {#if phase === "recording"}
      <span class="dot"></span>
      <div class="bars">
        {#each Array(5) as _, i}<span style="animation-delay:{i * 0.12}s"></span>{/each}
      </div>
    {:else if phase === "transcribing" || phase === "pasting"}
      <span class="spinner"></span>
    {:else if phase === "error"}
      <span class="err">!</span>
    {/if}
  </div>

  <div class="label">
    {#if phase === "recording"}{timer}
    {:else if phase === "transcribing"}Transcribing…
    {:else if phase === "pasting"}Pasting…
    {:else if phase === "error"}{message ?? "Error"}
    {:else}Ready{/if}
  </div>

  <div class="actions">
    {#if phase === "recording"}
      <button title="Stop" class="stop" onclick={() => invoke("toggle")}><span></span></button>
    {/if}
    <button title="Cancel" onclick={() => invoke("cancel")}>✕</button>
    <button title="Settings" onclick={() => invoke("show_settings")}>⚙</button>
  </div>
</div>

<style>
  :global(html), :global(body) {
    margin: 0;
    background: transparent;
    overflow: hidden;
    user-select: none;
    font-family: Inter, "Vazirmatn", system-ui, sans-serif;
  }
  .pill {
    box-sizing: border-box;
    height: 56px;
    margin: 8px;
    padding: 0 10px 0 16px;
    display: flex;
    align-items: center;
    gap: 12px;
    border-radius: 28px;
    background: rgba(20, 20, 24, 0.92);
    border: 1px solid rgba(255, 255, 255, 0.08);
    color: #f2f2f5;
    font-size: 14px;
  }
  .indicator { display: flex; align-items: center; gap: 8px; min-width: 24px; }
  .dot {
    width: 10px; height: 10px; border-radius: 50%;
    background: #ff4d5e;
    animation: pulse 1.2s ease-in-out infinite;
  }
  .bars { display: flex; align-items: center; gap: 3px; height: 22px; }
  .bars span {
    width: 3px; height: 100%; border-radius: 2px; background: #9fa8ff;
    animation: wave 0.9s ease-in-out infinite;
  }
  .spinner {
    width: 16px; height: 16px; border-radius: 50%;
    border: 2px solid rgba(255,255,255,0.2); border-top-color: #9fa8ff;
    animation: spin 0.8s linear infinite;
  }
  .err {
    width: 18px; height: 18px; border-radius: 50%; background: #ff4d5e;
    display: grid; place-items: center; font-weight: 700; font-size: 12px;
  }
  .label {
    flex: 1; font-variant-numeric: tabular-nums;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .actions { display: flex; gap: 4px; }
  button {
    width: 32px; height: 32px; border-radius: 50%; border: none;
    background: rgba(255,255,255,0.06); color: #d8d8e0;
    font-size: 14px; cursor: pointer; display: grid; place-items: center;
  }
  button:hover { background: rgba(255,255,255,0.14); }
  .stop span { width: 10px; height: 10px; border-radius: 2px; background: #ff4d5e; }
  @keyframes pulse { 50% { opacity: 0.35; } }
  @keyframes wave { 0%,100% { transform: scaleY(0.3); } 50% { transform: scaleY(1); } }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
