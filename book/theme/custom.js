// Smooth scroll for anchor links
document.addEventListener('DOMContentLoaded', function () {
    document.querySelectorAll('a[href^="#"]').forEach(function (anchor) {
        anchor.addEventListener('click', function (e) {
            var target = document.querySelector(this.getAttribute('href'));
            if (target) {
                e.preventDefault();
                target.scrollIntoView({ behavior: 'smooth', block: 'start' });
            }
        });
    });

    // Section icons (inline SVG, 14x14)
    var sectionIcons = {
        'getting started': '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="5 3 19 12 5 21 5 3"/></svg>',
        'architecture': '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="7" height="7"/><rect x="14" y="3" width="7" height="7"/><rect x="3" y="14" width="7" height="7"/><rect x="14" y="14" width="7" height="7"/></svg>',
        'api reference': '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="16 18 22 12 16 6"/><polyline points="8 6 2 12 8 18"/></svg>',
        'operations': '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 12h-4l-3 9L9 3l-3 9H2"/></svg>',
        'contributing': '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M16 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2"/><circle cx="8.5" cy="7" r="4"/><line x1="20" y1="8" x2="20" y2="14"/><line x1="23" y1="11" x2="17" y2="11"/></svg>',
    };

    // Collapsible sidebar sections
    var partTitles = document.querySelectorAll('.sidebar .chapter li.part-title');
    partTitles.forEach(function (title) {
        // Collect all chapter-item siblings until the next part-title or end
        var items = [];
        var sibling = title.nextElementSibling;
        while (sibling && !sibling.classList.contains('part-title')) {
            if (sibling.classList.contains('chapter-item') && sibling.querySelector('a')) {
                items.push(sibling);
            }
            sibling = sibling.nextElementSibling;
        }

        if (items.length === 0) return;

        // Check if this section contains the active page
        var hasActive = items.some(function (item) {
            return item.querySelector('a.active');
        });

        // Create a wrapper div for the items
        var wrapper = document.createElement('div');
        wrapper.className = 'sidebar-section-items';
        if (!hasActive) {
            wrapper.classList.add('collapsed');
        }

        // Move items into wrapper
        title.parentNode.insertBefore(wrapper, items[0]);
        items.forEach(function (item) {
            wrapper.appendChild(item);
        });

        // Add icon + toggle chevron to title
        var titleText = title.textContent.trim();
        var icon = sectionIcons[titleText.toLowerCase()];

        var chevron = document.createElement('span');
        chevron.className = 'section-chevron';
        chevron.textContent = hasActive ? '▾' : '▸';

        // Rebuild title content: chevron + icon + text
        title.textContent = '';
        title.appendChild(chevron);
        if (icon) {
            var iconSpan = document.createElement('span');
            iconSpan.className = 'section-icon';
            iconSpan.innerHTML = icon;
            title.appendChild(iconSpan);
        }
        var textSpan = document.createElement('span');
        textSpan.textContent = titleText;
        title.appendChild(textSpan);

        // Make title clickable
        title.style.cursor = 'pointer';
        title.addEventListener('click', function () {
            var isCollapsed = wrapper.classList.toggle('collapsed');
            chevron.textContent = isCollapsed ? '▸' : '▾';
        });
    });
});
