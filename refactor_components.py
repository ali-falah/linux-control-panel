import re
import os

modules_dir = "src/lib/modules"
files_to_check = [
    "AppManager.svelte",
    "FlatpakRpm.svelte",
    "NetworkManager.svelte",
    "NginxManager.svelte",
    "ShellEnv.svelte",
    "DnfHistory.svelte",
    "StartupManager.svelte",
    "ServiceManager.svelte",
    "HostsManager.svelte",
    "CoprBrowser.svelte",
    "RepoManager.svelte"
]

def add_imports(content):
    imports_to_add = []
    if "SearchBar" not in content and 'class="search-bar"' in content:
        imports_to_add.append("import SearchBar from '../components/ui/SearchBar.svelte';")
    if "TabGroup" not in content and 'class="tab-bar"' in content:
        imports_to_add.append("import TabGroup from '../components/ui/TabGroup.svelte';")
    if "Select" not in content and ('<select' in content):
        imports_to_add.append("import Select from '../components/ui/Select.svelte';")

    if not imports_to_add:
        return content

    import_str = "\n  ".join(imports_to_add)
    # find the first import statement and prepend
    return re.sub(r'(import .*?;)', import_str + r'\n  \1', content, count=1)

def replace_search_bar(content):
    # Regex to match <div class="search-bar"...><Search.../><input bind:value={var}.../></div>
    def replacer(match):
        div_str = match.group(0)
        # Extract bind:value and placeholder
        bind_val = re.search(r'bind:value=\{([^}]+)\}', div_str)
        placeholder = re.search(r'placeholder="([^"]+)"', div_str)
        style = re.search(r'style="([^"]+)"', div_str)
        
        bind_str = f'bind:value={{{bind_val.group(1)}}}' if bind_val else ''
        place_str = f'placeholder="{placeholder.group(1)}"' if placeholder else 'placeholder="Search..."'
        style_str = f'style="{style.group(1)}"' if style else ''
        
        return f'<SearchBar {bind_str} {place_str} {style_str} />'

    return re.sub(r'<div class="search-bar"[^>]*>[\s\S]*?<input[^>]*>[\s\S]*?</div>', replacer, content)

def replace_tab_bar(content):
    # This one is tricky due to each blocks. I'll replace manually if needed, but let's try a heuristic
    # If the file has a static tab-bar with buttons, we'll leave it to be replaced manually or do simple ones.
    # We will just do a basic replace for known ones, but honestly, it's safer to do regex for <select> and <div class="search-bar"> first.
    return content

def replace_selects(content):
    # Replace <select ...> with <Select ...> and remove class="..."
    # Note: we need to make sure we don't break logic.
    def replacer(match):
        select_tag = match.group(1)
        # Replace 'select' with 'Select'
        select_tag = select_tag.replace('<select', '<Select', 1)
        # Remove class entirely as Select.svelte handles it
        select_tag = re.sub(r'class="[^"]+"', '', select_tag)
        return select_tag + match.group(2) + '</Select>'
        
    return re.sub(r'(<select[^>]*>)([\s\S]*?)</select>', replacer, content)

def remove_css(content):
    content = re.sub(r'\.search-bar \{[\s\S]*?\}\s*', '', content)
    content = re.sub(r'\.search-bar input \{[\s\S]*?\}\s*', '', content)
    content = re.sub(r'\.tab-bar \{[\s\S]*?\}\s*', '', content)
    content = re.sub(r'\.tab-btn \{[\s\S]*?\}\s*', '', content)
    content = re.sub(r'\.tab-btn:hover \{[\s\S]*?\}\s*', '', content)
    content = re.sub(r'\.tab-btn\.active \{[\s\S]*?\}\s*', '', content)
    return content

for file_name in files_to_check:
    path = os.path.join(modules_dir, file_name)
    if not os.path.exists(path):
        continue
    
    with open(path, "r") as f:
        content = f.read()

    original = content
    content = add_imports(content)
    content = replace_search_bar(content)
    content = replace_selects(content)
    content = remove_css(content)

    if content != original:
        with open(path, "w") as f:
            f.write(content)
        print(f"Refactored {file_name}")
