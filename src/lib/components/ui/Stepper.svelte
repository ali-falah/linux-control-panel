<script lang="ts">
  import { Check } from '@lucide/svelte';

  interface StepItem {
    id?: string | number;
    label: string;
    description?: string;
  }

  interface Props {
    steps: (string | StepItem)[];
    currentStep?: number;
    allowClick?: boolean;
    onchange?: (stepIndex: number) => void;
    class?: string;
  }

  let {
    steps = [],
    currentStep = $bindable(1),
    allowClick = true,
    onchange,
    class: className = ''
  }: Props = $props();

  function handleStepClick(stepNum: number) {
    if (!allowClick) return;
    currentStep = stepNum;
    if (onchange) onchange(stepNum);
  }
</script>

<div class="ui-stepper-bar {className}" role="navigation" aria-label="Progress Stepper">
  {#each steps as step, idx}
    {@const stepNum = idx + 1}
    {@const isCurrent = currentStep === stepNum}
    {@const isCompleted = currentStep > stepNum}
    {@const label = typeof step === 'string' ? step : step.label}

    {#if idx > 0}
      <div class="step-connector" class:active={currentStep >= stepNum}></div>
    {/if}

    <button
      type="button"
      class="step-indicator"
      class:active={isCurrent}
      class:completed={isCompleted}
      onclick={() => handleStepClick(stepNum)}
      disabled={!allowClick}
    >
      <div class="step-num">
        {#if isCompleted}
          <Check size={11} strokeWidth={2.8} />
        {:else}
          <span>{stepNum}</span>
        {/if}
      </div>
      <span class="step-label">{label}</span>
    </button>
  {/each}
</div>

<style>
  .ui-stepper-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 16px;
    background: var(--color-bg-surface);
    border-bottom: 1px solid var(--color-border-subtle);
    gap: 8px;
    user-select: none;
  }

  .step-indicator {
    display: flex;
    align-items: center;
    gap: 8px;
    background: transparent;
    border: 1px solid transparent;
    cursor: pointer;
    padding: 5px 10px;
    border-radius: 20px;
    transition: all 0.18s cubic-bezier(0.4, 0, 0.2, 1);
    font-family: inherit;
    white-space: nowrap;
  }

  .step-indicator:disabled {
    cursor: default;
  }

  .step-indicator:hover:not(:disabled):not(.active) {
    background: var(--color-bg-hover);
  }

  .step-num {
    width: 20px;
    height: 20px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 11px;
    font-weight: 600;
    background: var(--color-bg-raised);
    border: 1px solid var(--color-border);
    color: var(--color-text-muted);
    transition: all 0.18s cubic-bezier(0.4, 0, 0.2, 1);
    flex-shrink: 0;
  }

  .step-label {
    font-size: 12px;
    font-weight: 500;
    color: var(--color-text-muted);
    transition: color 0.18s ease;
  }

  /* ── Active State ── */
  .step-indicator.active {
    background: var(--color-accent-muted, rgba(0, 218, 243, 0.08));
    border-color: rgba(0, 218, 243, 0.25);
  }

  .step-indicator.active .step-num {
    background: var(--color-accent);
    color: #ffffff;
    border-color: var(--color-accent);
    font-weight: 700;
    box-shadow: 0 0 10px var(--color-accent-glow, rgba(0, 218, 243, 0.35));
  }

  .step-indicator.active .step-label {
    color: var(--color-text-primary);
    font-weight: 600;
  }

  /* ── Completed State ── */
  .step-indicator.completed .step-num {
    background: rgba(16, 185, 129, 0.15);
    color: #10b981;
    border-color: rgba(16, 185, 129, 0.4);
  }

  .step-indicator.completed .step-label {
    color: var(--color-text-secondary);
  }

  /* ── Connector Line ── */
  .step-connector {
    flex: 1;
    height: 1.5px;
    background: var(--color-border-subtle);
    border-radius: 1px;
    margin: 0 4px;
    transition: background 0.25s ease;
  }

  .step-connector.active {
    background: var(--color-accent);
    opacity: 0.6;
  }
</style>
