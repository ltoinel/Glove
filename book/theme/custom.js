// Glove documentation — sidebar enhancements & smooth scrolling
document.addEventListener('DOMContentLoaded', function () {
    // ── Smooth scroll for in-page anchor links ──
    document.querySelectorAll('a[href^="#"]').forEach(function (anchor) {
        anchor.addEventListener('click', function (e) {
            var target = document.querySelector(this.getAttribute('href'));
            if (target) {
                e.preventDefault();
                target.scrollIntoView({ behavior: 'smooth', block: 'start' });
            }
        });
    });

    // ── Brand mark at the top of the sidebar ──
    var scrollbox = document.querySelector('.sidebar .sidebar-scrollbox');
    if (scrollbox && !scrollbox.querySelector('.glove-brand')) {
        var root = (typeof path_to_root === 'string' && path_to_root) ? path_to_root : '';
        var brand = document.createElement('a');
        brand.className = 'glove-brand';
        brand.href = root + 'introduction.html';
        brand.innerHTML =
            '<span class="glove-brand-dot">' +
            '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round">' +
            '<circle cx="12" cy="10" r="3"/><path d="M12 21s-7-6.5-7-11a7 7 0 0 1 14 0c0 4.5-7 11-7 11z"/></svg>' +
            '</span>' +
            '<span class="glove-brand-text">Glove<span> docs</span></span>';
        scrollbox.insertBefore(brand, scrollbox.firstChild);
    }

    // ── Section icons (inline SVG, 13×13) ──
    var sectionIcons = {
        'getting started': '<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><polygon points="5 3 19 12 5 21 5 3"/></svg>',
        'architecture': '<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="7" height="7"/><rect x="14" y="3" width="7" height="7"/><rect x="3" y="14" width="7" height="7"/><rect x="14" y="14" width="7" height="7"/></svg>',
        'api reference': '<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><polyline points="16 18 22 12 16 6"/><polyline points="8 6 2 12 8 18"/></svg>',
        'operations': '<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 12h-4l-3 9L9 3l-3 9H2"/></svg>',
        'ile-de-france mobilités': '<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><path d="M2 12h20"/><path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"/></svg>',
        'contributing': '<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M16 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2"/><circle cx="8.5" cy="7" r="4"/><line x1="20" y1="8" x2="20" y2="14"/><line x1="23" y1="11" x2="17" y2="11"/></svg>',
    };

    // ── Collapsible sidebar sections ──
    var partTitles = document.querySelectorAll('.sidebar .chapter li.part-title');
    partTitles.forEach(function (title) {
        // Gather chapter-item siblings until the next part-title.
        var items = [];
        var sibling = title.nextElementSibling;
        while (sibling && !sibling.classList.contains('part-title')) {
            if (sibling.classList.contains('chapter-item') && sibling.querySelector('a')) {
                items.push(sibling);
            }
            sibling = sibling.nextElementSibling;
        }
        if (items.length === 0) return;

        var hasActive = items.some(function (item) { return item.querySelector('a.active'); });

        var wrapper = document.createElement('div');
        wrapper.className = 'sidebar-section-items';
        if (!hasActive) wrapper.classList.add('collapsed');

        title.parentNode.insertBefore(wrapper, items[0]);
        items.forEach(function (item) { wrapper.appendChild(item); });

        // Rebuild title: chevron + icon + label
        var titleText = title.textContent.trim();
        var icon = sectionIcons[titleText.toLowerCase()];

        var chevron = document.createElement('span');
        chevron.className = 'section-chevron';
        chevron.innerHTML =
            '<svg width="8" height="8" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="6 9 12 15 18 9"/></svg>';
        chevron.style.transform = hasActive ? 'rotate(0deg)' : 'rotate(-90deg)';

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

        title.style.cursor = 'pointer';
        title.addEventListener('click', function () {
            var collapsed = wrapper.classList.toggle('collapsed');
            chevron.style.transform = collapsed ? 'rotate(-90deg)' : 'rotate(0deg)';
        });
    });
});
