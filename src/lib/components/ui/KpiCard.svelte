<script lang="ts">
  import type { Component } from 'svelte';

  interface Props {
    icon?: Component<any>;
    value: string | number;
    label: string;
    subtext?: string;
    statusText?: string;
    statusType?: 'success' | 'warning' | 'error' | 'info' | 'muted';
    iconBg?: string;
    iconColor?: string;
    active?: boolean;
    onclick?: () => void;
    class?: string;
    style?: string;
    title?: string;
  }

  let {
    icon: IconComponent,
    value,
    label,
    subtext = '',
    statusText = '',
    statusType = 'muted',
    iconBg = 'rgba(0, 218, 243, 0.12)',
    iconColor = 'var(--color-accent)',
    active = false,
    onclick,
    class: className = '',
    style = '',
    title = ''
  }: Props = $props();

  const cardTooltip = $derived(
    title || (subtext ? `${label}: ${subtext}` : `${label}: ${value}`)
  );
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div 
  class="kpi-card {active ? 'active' : ''} {className}" 
  {style}
  title={cardTooltip}
  role={onclick ? 'button' : undefined}
  tabindex={onclick ? 0 : undefined}
  {onclick}
  onkeydown={(e) => { if (onclick && (e.key === 'Enter' || e.key === ' ')) { e.preventDefault(); onclick(); } }}
>
  {#if IconComponent}
    <div class="kpi-icon-wrap" style="background: {iconBg}; color: {iconColor};">
      <IconComponent size={16} />
    </div>
  {/if}

  <div class="kpi-content">
    <div class="kpi-val-row">
      <span class="kpi-value font-mono">{value}</span>
      {#if statusText}
        <span class="kpi-status-badge status-{statusType}">{statusText}</span>
      {/if}
    </div>
    <div class="kpi-label-row">
      <span class="kpi-label" title={label}>{label}</span>
      {#if subtext}
        <span class="kpi-subtext" title={subtext}>· {subtext}</span>
      {/if}
    </div>
  </div>
</div>

<style>
  .kpi-card {
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    padding: 8px 12px;
    display: flex;
    align-items: center;
    gap: 10px;
    cursor: default;
    transition: all 0.15s ease;
    user-select: none;
    min-height: 48px;
    box-sizing: border-box;
  }

  :global(html.light-mode) .kpi-card {
    background: #FFFFFF;
    border-color: #E2E8F0;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.04);
  }

  .kpi-card[role="button"] {
    cursor: pointer;
  }

  .kpi-card[role="button"]:hover {
    border-color: var(--color-accent);
    transform: translateY(-1px);
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.06);
  }

  .kpi-card.active {
    border-color: var(--color-accent);
    background: rgba(0, 218, 243, 0.05);
  }

  .kpi-icon-wrap {
    width: 30px;
    height: 30px;
    border-radius: 7px;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }

  .kpi-content {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
    flex: 1;
  }

  .kpi-val-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
  }

  .kpi-value {
    font-size: 15px;
    font-weight: 700;
    color: var(--color-text-primary);
    line-height: 1.1;
  }

  .kpi-label-row {
    display: flex;
    align-items: center;
    gap: 4px;
    overflow: hidden;
    white-space: nowrap;
  }

  .kpi-label {
    font-size: 11.5px;
    font-weight: 600;
    color: var(--color-text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
  }

  :global(html.light-mode) .kpi-label {
    color: #64748B;
  }

  .kpi-subtext {
    font-size: 10.5px;
    color: var(--color-text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .kpi-status-badge {
    font-size: 9.5px;
    font-weight: 700;
    padding: 1px 5px;
    border-radius: 4px;
    text-transform: uppercase;
    letter-spacing: 0.3px;
    line-height: 1.2;
  }

  .status-success {
    background: rgba(16, 185, 129, 0.15);
    color: var(--color-success);
  }

  .status-warning {
    background: rgba(245, 158, 11, 0.15);
    color: var(--color-warning);
  }

  .status-error {
    background: rgba(239, 68, 68, 0.15);
    color: var(--color-error);
  }

  .status-info {
    background: rgba(0, 218, 243, 0.15);
    color: var(--color-accent);
  }

  .status-muted {
    background: rgba(255, 255, 255, 0.08);
    color: var(--color-text-muted);
  }

  .font-mono {
    font-family: var(--font-mono);
  }
</style>
