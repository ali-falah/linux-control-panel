<script lang="ts">
  import { onMount } from 'svelte';
  import { ChevronLeft, ChevronRight, Calendar } from '@lucide/svelte';

  interface Props {
    value?: string; // YYYY-MM-DD
    placeholder?: string;
    label?: string;
  }

  let {
    value = $bindable(''),
    placeholder = 'Select date...',
    label = ''
  }: Props = $props();

  let showCalendar = $state(false);
  let viewYear  = $state(new Date().getFullYear());
  let viewMonth = $state(new Date().getMonth()); // 0-indexed
  let containerRef = $state<HTMLDivElement | null>(null);

  onMount(() => {
    function handleOutsideClick(e: MouseEvent) {
      if (!showCalendar) return;
      if (e.target && !document.body.contains(e.target as Node)) return;
      if (containerRef && containerRef.contains(e.target as Node)) return;
      showCalendar = false;
    }
    document.addEventListener('click', handleOutsideClick);
    return () => document.removeEventListener('click', handleOutsideClick);
  });

  // Sync view when value changes externally
  $effect(() => {
    if (value) {
      const [y, m] = value.split('-').map(Number);
      if (!isNaN(y) && !isNaN(m)) {
        viewYear  = y;
        viewMonth = m - 1;
      }
    }
  });

  const MONTHS = [
    'January','February','March','April','May','June',
    'July','August','September','October','November','December'
  ];
  const DAYS = ['Su','Mo','Tu','We','Th','Fr','Sa'];

  function daysInMonth(y: number, m: number) {
    return new Date(y, m + 1, 0).getDate();
  }

  function formatDisplay(d: string) {
    if (!d) return '';
    const [y, m, day] = d.split('-');
    return `${m}/${day}/${y}`;
  }

  let grid = $derived.by(() => {
    const first    = new Date(viewYear, viewMonth, 1).getDay();
    const total    = daysInMonth(viewYear, viewMonth);
    const prevM    = viewMonth === 0 ? 11 : viewMonth - 1;
    const prevY    = viewMonth === 0 ? viewYear - 1 : viewYear;
    const prevTotal = daysInMonth(prevY, prevM);

    const cells: { day: number; type: 'prev' | 'cur' | 'next' }[] = [];

    for (let i = first - 1; i >= 0; i--)
      cells.push({ day: prevTotal - i, type: 'prev' });

    for (let d = 1; d <= total; d++)
      cells.push({ day: d, type: 'cur' });

    let nextDay = 1;
    while (cells.length < 42)
      cells.push({ day: nextDay++, type: 'next' });

    return cells;
  });

  function select(cell: { day: number; type: string }) {
    if (cell.type !== 'cur') return;
    const m = String(viewMonth + 1).padStart(2, '0');
    const d = String(cell.day).padStart(2, '0');
    value = `${viewYear}-${m}-${d}`;
    showCalendar = false;
  }

  function prevMonth() {
    if (viewMonth === 0) { viewMonth = 11; viewYear--; }
    else viewMonth--;
  }

  function nextMonth() {
    if (viewMonth === 11) { viewMonth = 0; viewYear++; }
    else viewMonth++;
  }

  function isSelected(cell: { day: number; type: string }) {
    if (!value || cell.type !== 'cur') return false;
    const [y, m, d] = value.split('-').map(Number);
    return y === viewYear && m === viewMonth + 1 && d === cell.day;
  }

  function isToday(cell: { day: number; type: string }) {
    if (cell.type !== 'cur') return false;
    const now = new Date();
    return now.getFullYear() === viewYear &&
           now.getMonth()    === viewMonth &&
           now.getDate()     === cell.day;
  }
</script>

<div bind:this={containerRef} class="dp-wrap">
  {#if label}
    <span class="dp-label">{label}</span>
  {/if}

  <!-- Trigger -->
  <button
    type="button"
    class="dp-trigger"
    class:open={showCalendar}
    onclick={() => showCalendar = !showCalendar}
  >
    <Calendar size={13} class="dp-icon" />
    <span class="dp-val">
      {value ? formatDisplay(value) : placeholder}
    </span>
  </button>

  <!-- Inline calendar (expands the parent popover) -->
  {#if showCalendar}
    <div class="dp-calendar" role="dialog">
      <!-- Header: month/year navigation -->
      <div class="dp-nav">
        <button type="button" class="dp-nav-btn" onclick={prevMonth}>
          <ChevronLeft size={14} />
        </button>
        <span class="dp-month-label">{MONTHS[viewMonth]} {viewYear}</span>
        <button type="button" class="dp-nav-btn" onclick={nextMonth}>
          <ChevronRight size={14} />
        </button>
      </div>

      <!-- Day-of-week headers -->
      <div class="dp-grid">
        {#each DAYS as d}
          <span class="dp-weekday">{d}</span>
        {/each}

        <!-- Day cells -->
        {#each grid as cell}
          <button
            type="button"
            class="dp-day"
            class:cur={cell.type === 'cur'}
            class:other={cell.type !== 'cur'}
            class:selected={isSelected(cell)}
            class:today={isToday(cell)}
            onclick={() => select(cell)}
            tabindex={cell.type === 'cur' ? 0 : -1}
          >
            {cell.day}
          </button>
        {/each}
      </div>
    </div>
  {/if}
</div>

<style>
  .dp-wrap {
    display: flex;
    flex-direction: column;
    gap: 4px;
    width: 100%;
  }

  .dp-label {
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--color-text-muted);
  }

  .dp-trigger {
    display: flex;
    align-items: center;
    gap: 7px;
    width: 100%;
    height: 32px;
    padding: 0 10px;
    background: rgba(255,255,255,0.05);
    border: 1px solid rgba(255,255,255,0.1);
    border-radius: 7px;
    color: var(--color-text-secondary);
    font-size: 12px;
    font-family: var(--font-sans);
    cursor: pointer;
    transition: border-color 0.15s, box-shadow 0.15s;
    text-align: left;
    box-sizing: border-box;
  }

  .dp-trigger:hover {
    border-color: rgba(0, 218, 243, 0.3);
  }

  .dp-trigger.open {
    border-color: var(--color-accent);
    box-shadow: 0 0 0 2px rgba(0, 218, 243, 0.12);
    color: var(--color-text-primary);
  }

  :global(.dp-icon) {
    color: var(--color-text-muted);
    flex-shrink: 0;
  }

  .dp-val {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* Calendar panel — renders inline, expands the parent popover */
  .dp-calendar {
    width: 100%;
    margin-top: 6px;
    background: rgba(0, 218, 243, 0.03);
    border: 1px solid rgba(0, 218, 243, 0.15);
    border-radius: 8px;
    padding: 8px;
    box-sizing: border-box;
    animation: dp-fade 0.1s ease;
  }

  @keyframes dp-fade {
    from { opacity: 0; transform: translateY(-4px); }
    to   { opacity: 1; transform: translateY(0); }
  }

  .dp-nav {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 8px;
  }

  .dp-nav-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    background: transparent;
    border: none;
    border-radius: 5px;
    color: var(--color-text-muted);
    cursor: pointer;
    transition: background 0.12s, color 0.12s;
  }

  .dp-nav-btn:hover {
    background: rgba(0, 218, 243, 0.1);
    color: var(--color-accent);
  }

  .dp-month-label {
    font-size: 12px;
    font-weight: 600;
    color: var(--color-text-primary);
    letter-spacing: 0.02em;
  }

  .dp-grid {
    display: grid;
    grid-template-columns: repeat(7, 1fr);
    gap: 2px;
  }

  .dp-weekday {
    font-size: 9px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--color-text-muted);
    text-align: center;
    padding: 3px 0;
  }

  .dp-day {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    aspect-ratio: 1;
    font-size: 11px;
    font-family: var(--font-sans);
    border: none;
    border-radius: 5px;
    background: transparent;
    cursor: pointer;
    transition: background 0.12s, color 0.12s;
  }

  .dp-day.other {
    color: rgba(255,255,255,0.15);
    cursor: default;
    pointer-events: none;
  }

  .dp-day.cur {
    color: var(--color-text-secondary);
  }

  .dp-day.cur:hover {
    background: rgba(0, 218, 243, 0.12);
    color: var(--color-accent);
  }

  .dp-day.today {
    color: var(--color-accent);
    font-weight: 700;
    border: 1px solid rgba(0, 218, 243, 0.3);
  }

  .dp-day.selected {
    background: var(--color-accent);
    color: #0a1628;
    font-weight: 700;
    border: none;
  }

  .dp-day.selected:hover {
    background: #00b9cf;
    color: #0a1628;
  }
</style>
