import re

with open("src/lib/modules/FlatpakRpm.svelte", "r") as f:
    content = f.read()

imports = """import SearchBar from '../components/ui/SearchBar.svelte';
  import TabGroup from '../components/ui/TabGroup.svelte';"""
content = re.sub(r'(import Button from \'../components/ui/Button\.svelte\';)', imports + r'\n  \1', content)

search_html = """<SearchBar bind:value={searchQuery} placeholder="Search {activeTab === 'rpm' ? 'RPMs' : 'Flatpaks'}..." style="flex: 1; max-width: 300px;" />"""
content = re.sub(r'<div class="search-bar"[^>]*>[\s\S]*?<input bind:value={searchQuery}[^>]*>\s*</div>', search_html, content)

tab_html = """<TabGroup 
          tabs={[
            {id: 'rpm', label: 'RPM Packages'},
            {id: 'flatpak', label: 'Flatpak Apps'}
          ]}
          bind:activeTab={activeTab}
        />"""
content = re.sub(r'<div class="tab-bar">[\s\S]*?<button class="tab-btn \{activeTab === \'flatpak\' \? \'active\' \: \'\'\}" onclick=\{.*?\}\s*>[\s\S]*?</button>\s*</div>', tab_html, content)

with open("src/lib/modules/FlatpakRpm.svelte", "w") as f:
    f.write(content)
