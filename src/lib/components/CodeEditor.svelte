<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { EditorView, basicSetup } from 'codemirror';
  import { EditorState } from '@codemirror/state';
  import { oneDark } from '@codemirror/theme-one-dark';

  interface Props {
    value?: string;
    readonly?: boolean;
    language?: string;
    height?: string;
    onchange?: (value: string) => void;
  }

  let { value = '', readonly = false, height = '300px', onchange }: Props = $props();

  let container: HTMLDivElement;
  let view: EditorView | null = null;

  onMount(() => {
    const extensions = [
      basicSetup,
      oneDark,
      EditorView.theme({
        '&': {
          fontSize: '12px',
          fontFamily: 'var(--font-mono)',
          height: height,
          borderRadius: '8px',
          overflow: 'hidden',
        },
        '.cm-scroller': {
          overflow: 'auto',
        },
        '.cm-content': {
          padding: '12px 0',
        },
        '&.cm-focused': {
          outline: 'none',
        },
      }),
    ];

    if (readonly) {
      extensions.push(EditorState.readOnly.of(true));
    } else {
      extensions.push(
        EditorView.updateListener.of((update) => {
          if (update.docChanged && onchange) {
            onchange(update.state.doc.toString());
          }
        })
      );
    }

    view = new EditorView({
      state: EditorState.create({
        doc: value,
        extensions,
      }),
      parent: container,
    });
  });

  $effect(() => {
    if (view && value !== view.state.doc.toString()) {
      view.dispatch({
        changes: {
          from: 0,
          to: view.state.doc.length,
          insert: value,
        },
      });
    }
  });

  onDestroy(() => {
    view?.destroy();
  });
</script>

<div bind:this={container} class="code-editor-wrap"></div>

<style>
  .code-editor-wrap {
    border: 1px solid var(--color-border);
    border-radius: 8px;
    overflow: hidden;
    transition: border-color 0.2s;
    height: 100%;
  }

  .code-editor-wrap:focus-within {
    border-color: var(--color-border-focus);
    box-shadow: 0 0 0 3px var(--color-accent-muted);
  }

  :global(.cm-editor) {
    background: var(--color-bg-surface) !important;
    color: var(--color-text-primary) !important;
  }

  :global(.cm-scroller) {
    font-family: var(--font-mono) !important;
  }

  :global(.cm-content) {
    color: var(--color-text-primary) !important;
    caret-color: var(--color-accent) !important;
  }

  :global(.cm-line) {
    color: var(--color-text-primary) !important;
  }

  :global(.cm-gutters) {
    background: var(--color-bg-base) !important;
    border-right: 1px solid var(--color-border) !important;
    color: var(--color-text-muted) !important;
  }

  :global(.cm-gutterElement) {
    color: var(--color-text-muted) !important;
  }

  :global(.cm-activeLine) {
    background: var(--color-bg-hover) !important;
  }

  :global(.cm-activeLineGutter) {
    background: var(--color-bg-hover) !important;
    color: var(--color-text-primary) !important;
    font-weight: 600 !important;
  }

  /* ── Component-Level Light Mode High Contrast Overrides ── */
  :global(html.light-mode .cm-editor) {
    background: #FFFFFF !important;
    color: #111827 !important;
  }

  :global(html.light-mode .cm-content),
  :global(html.light-mode .cm-line) {
    color: #111827 !important;
    font-weight: 450 !important;
  }

  :global(html.light-mode .cm-gutters) {
    background: #F8FAFB !important;
    border-right: 1px solid #E5E7EB !important;
    color: #4B5563 !important;
  }

  :global(html.light-mode .cm-gutterElement) {
    color: #4B5563 !important;
    font-weight: 500 !important;
  }

  :global(html.light-mode .cm-activeLine) {
    background: #F3F4F6 !important;
  }

  :global(html.light-mode .cm-activeLineGutter) {
    background: #E5E7EB !important;
    color: #111827 !important;
  }

  :global(html.light-mode .code-editor-wrap) {
    border-color: #E5E7EB !important;
  }
</style>
