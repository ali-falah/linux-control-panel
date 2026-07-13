export function tableFeatures(node: HTMLTableElement) {
  function init() {
    const headers = Array.from(node.querySelectorAll('thead th')) as HTMLTableCellElement[];
    
    headers.forEach((th, index) => {
      // 1. Setup Resize Handle
      if (!th.querySelector('.resize-handle')) {
        const handle = document.createElement('div');
        handle.className = 'resize-handle';
        th.appendChild(handle);
        // th is already position: sticky; which makes it positioned for absolute children
        
        let startX = 0;
        let startWidth = 0;
        
        function onMouseMove(e: MouseEvent) {
          requestAnimationFrame(() => {
            const newWidth = Math.max(30, startWidth + (e.pageX - startX));
            th.style.width = `${newWidth}px`;
            th.style.minWidth = `${newWidth}px`;
            th.style.maxWidth = `${newWidth}px`;
          });
        }
        
        function onMouseUp() {
          handle.classList.remove('active');
          document.removeEventListener('mousemove', onMouseMove);
          document.removeEventListener('mouseup', onMouseUp);
          document.body.style.cursor = '';
        }
        
        handle.addEventListener('mousedown', (e) => {
          e.stopPropagation(); // prevent sort
          e.preventDefault(); // prevent text selection
          startX = e.pageX;
          startWidth = th.offsetWidth;
          handle.classList.add('active');
          document.body.style.cursor = 'col-resize';
          document.addEventListener('mousemove', onMouseMove);
          document.addEventListener('mouseup', onMouseUp);
        });
      }

      // 2. Setup Sort Indicator
      if (!th.classList.contains('th-sortable') && th.textContent?.trim()) {
        th.classList.add('th-sortable');
        const indicator = document.createElement('span');
        indicator.className = 'sort-indicator';
        indicator.innerHTML = '↕';
        th.appendChild(indicator);

        th.addEventListener('click', () => {
          const tbody = node.querySelector('tbody');
          if (!tbody) return;
          
          const isAsc = th.classList.contains('sort-asc');
          
          // Reset all headers
          headers.forEach(h => {
            h.classList.remove('sort-asc', 'sort-desc');
            const ind = h.querySelector('.sort-indicator');
            if (ind) ind.innerHTML = '↕';
          });
          
          const dir = isAsc ? -1 : 1;
          if (!isAsc) {
            th.classList.add('sort-asc');
            indicator.innerHTML = '↑';
          } else {
            th.classList.add('sort-desc');
            indicator.innerHTML = '↓';
          }

          const rows = Array.from(tbody.querySelectorAll('tr'));
          rows.sort((a, b) => {
            const cellsA = a.querySelectorAll('td');
            const cellsB = b.querySelectorAll('td');
            if (!cellsA[index] || !cellsB[index]) return 0;
            const valA = cellsA[index].textContent?.trim() || '';
            const valB = cellsB[index].textContent?.trim() || '';
            
            // Try numeric sort first if both look like numbers (ignoring units like %, MB, GB)
            const cleanA = valA.replace(/[^\d.-]/g, '');
            const cleanB = valB.replace(/[^\d.-]/g, '');
            const numA = parseFloat(cleanA);
            const numB = parseFloat(cleanB);
            const isNumA = !isNaN(numA) && isFinite(numA) && cleanA !== '';
            const isNumB = !isNaN(numB) && isFinite(numB) && cleanB !== '';
            
            if (isNumA && isNumB) {
               return (numA - numB) * dir;
            }
            return valA.localeCompare(valB) * dir;
          });

          // Re-append sorted rows to DOM (browser handles this efficiently)
          rows.forEach(r => tbody.appendChild(r));
        });
      }
    });
  }

  // Initial setup
  init();

  // Watch for dynamic table content changes (like tabs switching or rows loading)
  const observer = new MutationObserver((mutations) => {
    let shouldInit = false;
    for (const m of mutations) {
       if (m.type === 'childList') {
         // If a new thead is added or we get new headers, re-init
         for (const node of m.addedNodes) {
           if (node.nodeName === 'THEAD' || node.nodeName === 'TR' || node.nodeName === 'TH') {
             shouldInit = true;
           }
         }
       }
    }
    if (shouldInit) {
      setTimeout(init, 0);
    }
  });
  
  observer.observe(node, { childList: true, subtree: true });

  return {
    destroy() {
      observer.disconnect();
    }
  };
}
