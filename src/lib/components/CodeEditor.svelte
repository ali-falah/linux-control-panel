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
  }

  .code-editor-wrap:focus-within {
    border-color: var(--color-border-focus);
    box-shadow: 0 0 0 3px var(--color-accent-muted);
  }

  :global(.cm-editor) {
    background: var(--color-bg-surface) !important;
  }

  :global(.cm-gutters) {
    background: rgba(255,255,255,0.02) !important;
    border-right: 1px solid var(--color-border) !important;
  }
</style>
