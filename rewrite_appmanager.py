import re

with open("src/lib/modules/AppManager.svelte", "r") as f:
    content = f.read()

# Add imports
imports = """import SearchBar from '../components/ui/SearchBar.svelte';
  import TabGroup from '../components/ui/TabGroup.svelte';
  import Select from '../components/ui/Select.svelte';"""
content = re.sub(r'(import { uiStore } from \'../stores/ui.svelte.ts\';)', imports + r'\n  \1', content)

# Replace Search bar
search_html = """<SearchBar bind:value={filter} placeholder="Search installed apps..." disabled={isUninstalling} style="flex: 1; max-width: 300px;" />"""
content = re.sub(r'<div class="search-bar"[^>]*>[\s\S]*?<input bind:value={filter}[^>]*>\s*</div>', search_html, content)

# Replace Tab Bar
tab_html = """<TabGroup 
        tabs={[
          {id: 'All', label: 'All Sources'},
          {id: 'RPM', label: 'RPM'},
          {id: 'Flatpak', label: 'Flatpak'}
        ]}
        bind:activeTab={sourceFilter}
        disabled={isUninstalling}
      />"""
content = re.sub(r'<div class="tab-bar"[^>]*>[\s\S]*?\{/each\}\s*</div>', tab_html, content)

# Replace Select
select_html = """<Select bind:value={sortBy} disabled={isUninstalling}>
          <option value="name">Name</option>
          <option value="size">Size</option>
          <option value="date">Install Date</option>
          <option value="source">Source</option>
        </Select>"""
content = re.sub(r'<select bind:value={sortBy} class="sort-select" disabled={isUninstalling}>[\s\S]*?</select>', select_html, content)

# Remove old CSS
content = re.sub(r'\.search-bar \{[\s\S]*?\}\s*', '', content)
content = re.sub(r'\.search-bar input \{[\s\S]*?\}\s*', '', content)
content = re.sub(r'\.sort-select \{[\s\S]*?\}\s*', '', content)
content = re.sub(r'\.sort-select option \{[\s\S]*?\}\s*', '', content)

with open("src/lib/modules/AppManager.svelte", "w") as f:
    f.write(content)
