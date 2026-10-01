// Glove documentation — navigation enhancements on top of mdBook:
// sidebar sections, light/dark toggle, breadcrumb, page TOC, prev/next cards.
document.addEventListener('DOMContentLoaded', function () {
    var root = (typeof path_to_root === 'string' && path_to_root) ? path_to_root : '';
    var main = document.querySelector('#content main');

    var svg = function (paths, size) {
        return '<svg class="glove-ui" width="' + (size || 13) + '" height="' + (size || 13) + '" viewBox="0 0 24 24" fill="none" ' +
            'stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">' + paths + '</svg>';
    };

    // ── Brand mark at the top of the sidebar ──
    var scrollbox = document.querySelector('.sidebar .sidebar-scrollbox');
    if (scrollbox && !scrollbox.querySelector('.glove-brand')) {
        var brand = document.createElement('a');
        brand.className = 'glove-brand';
        brand.href = root + 'introduction.html';
        brand.innerHTML =
            '<span class="glove-brand-dot">' +
            svg('<circle cx="12" cy="10" r="3"/><path d="M12 21s-7-6.5-7-11a7 7 0 0 1 14 0c0 4.5-7 11-7 11z"/>', 15) +
            '</span><span class="glove-brand-text">Glove<span> docs</span></span>';
        scrollbox.insertBefore(brand, scrollbox.firstChild);
    }

    // ── Collapsible sidebar sections, with an icon per section ──
    var sectionIcons = {
        'getting started': '<polygon points="5 3 19 12 5 21 5 3"/>',
        'architecture': '<rect x="3" y="3" width="7" height="7"/><rect x="14" y="3" width="7" height="7"/><rect x="3" y="14" width="7" height="7"/><rect x="14" y="14" width="7" height="7"/>',
        'api reference': '<polyline points="16 18 22 12 16 6"/><polyline points="8 6 2 12 8 18"/>',
        'operations': '<path d="M22 12h-4l-3 9L9 3l-3 9H2"/>',
        'contributing': '<path d="M16 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2"/><circle cx="8.5" cy="7" r="4"/><line x1="20" y1="8" x2="20" y2="14"/><line x1="23" y1="11" x2="17" y2="11"/>',
        'ile-de-france mobilités': '<circle cx="12" cy="12" r="10"/><path d="M2 12h20"/><path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"/>',
        'sytadin': '<path d="M3 17l6-6 4 4 8-8"/><polyline points="14 7 21 7 21 14"/>',
    };
    var chevronSvg = svg('<polyline points="6 9 12 15 18 9"/>', 9);

    document.querySelectorAll('.sidebar .chapter li.part-title').forEach(function (title) {
        var items = [];
        var sibling = title.nextElementSibling;
        while (sibling && !sibling.classList.contains('part-title')) {
            if (sibling.classList.contains('chapter-item') && sibling.querySelector('a')) items.push(sibling);
            sibling = sibling.nextElementSibling;
        }
        if (items.length === 0) return;

        var hasActive = items.some(function (item) { return item.querySelector('a.active'); });
        var wrapper = document.createElement('div');
        wrapper.className = 'sidebar-section-items' + (hasActive ? '' : ' collapsed');
        title.parentNode.insertBefore(wrapper, items[0]);
        items.forEach(function (item) { wrapper.appendChild(item); });

        var label = title.textContent.trim();
        var chevron = document.createElement('span');
        chevron.className = 'section-chevron';
        chevron.innerHTML = chevronSvg;
        chevron.style.transform = hasActive ? 'rotate(0deg)' : 'rotate(-90deg)';

        title.textContent = '';
        title.appendChild(chevron);
        var icon = sectionIcons[label.toLowerCase()];
        if (icon) {
            var iconSpan = document.createElement('span');
            iconSpan.className = 'section-icon';
            iconSpan.innerHTML = svg(icon);
            title.appendChild(iconSpan);
        }
        var text = document.createElement('span');
        text.textContent = label;
        title.appendChild(text);
        title.dataset.label = label;

        title.style.cursor = 'pointer';
        title.setAttribute('role', 'button');
        title.setAttribute('aria-expanded', String(hasActive));
        title.addEventListener('click', function () {
            var collapsed = wrapper.classList.toggle('collapsed');
            chevron.style.transform = collapsed ? 'rotate(-90deg)' : 'rotate(0deg)';
            title.setAttribute('aria-expanded', String(!collapsed));
        });
    });

    // ── Light / dark toggle, driving mdBook's own theme switcher ──
    var rightButtons = document.querySelector('#menu-bar .right-buttons');
    if (rightButtons) {
        var toggle = document.createElement('button');
        toggle.id = 'glove-theme-toggle';
        toggle.type = 'button';
        toggle.className = 'icon-button';
        toggle.title = 'Toggle light / dark theme';
        toggle.setAttribute('aria-label', 'Toggle light / dark theme');
        toggle.innerHTML =
            '<span class="glove-sun">' + svg('<circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"/>', 17) + '</span>' +
            '<span class="glove-moon">' + svg('<path d="M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z"/>', 17) + '</span>';
        toggle.addEventListener('click', function () {
            var isLight = document.documentElement.classList.contains('light') ||
                document.documentElement.classList.contains('rust');
            var target = document.getElementById(isLight ? 'navy' : 'light');
            if (target) target.click();
        });
        rightButtons.insertBefore(toggle, rightButtons.firstChild);
    }

    // ── Search: Ctrl/Cmd+K opens it, like most documentation sites ──
    document.addEventListener('keydown', function (e) {
        if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
            var searchToggle = document.getElementById('search-toggle');
            if (searchToggle) {
                e.preventDefault();
                searchToggle.click();
            }
        }
    });
    var searchToggle = document.getElementById('search-toggle');
    if (searchToggle) searchToggle.title = 'Search (Ctrl+K or S)';

    if (!main) return;

    // ── Breadcrumb: section › page, read from the sidebar ──
    var activeLink = document.querySelector('.sidebar a.active');
    var h1 = main.querySelector('h1');
    if (activeLink && h1) {
        var section = activeLink.closest('.sidebar-section-items');
        var sectionTitle = section && section.previousElementSibling;
        if (sectionTitle && sectionTitle.dataset.label) {
            var crumb = document.createElement('nav');
            crumb.className = 'glove-breadcrumb';
            crumb.setAttribute('aria-label', 'Breadcrumb');
            var sectionSpan = document.createElement('span');
            sectionSpan.className = 'glove-breadcrumb-section';
            sectionSpan.textContent = sectionTitle.dataset.label;
            crumb.appendChild(sectionSpan);
            crumb.insertAdjacentHTML('beforeend', svg('<polyline points="9 18 15 12 9 6"/>', 11));
            var pageSpan = document.createElement('span');
            pageSpan.textContent = activeLink.textContent.replace(/^\s*[\d.]+\s*/, '').trim();
            crumb.appendChild(pageSpan);
            main.insertBefore(crumb, main.firstChild);
        }
    }

    // ── "On this page" TOC with scroll-spy ──
    var headings = Array.prototype.filter.call(main.querySelectorAll('h2[id], h3[id]'), function (h) {
        return !h.closest('.glove-hero');
    });
    if (headings.length >= 2) {
        var toc = document.createElement('nav');
        toc.className = 'glove-toc';
        toc.setAttribute('aria-label', 'On this page');
        var tocTitle = document.createElement('div');
        tocTitle.className = 'glove-toc-title';
        tocTitle.textContent = 'On this page';
        toc.appendChild(tocTitle);
        var list = document.createElement('ul');
        var linkFor = {};
        headings.forEach(function (h) {
            var li = document.createElement('li');
            var a = document.createElement('a');
            a.href = '#' + h.id;
            a.textContent = h.textContent.trim();
            if (h.tagName === 'H3') a.className = 'glove-toc-h3';
            li.appendChild(a);
            list.appendChild(li);
            linkFor[h.id] = a;
        });
        toc.appendChild(list);
        document.body.appendChild(toc);
        document.documentElement.classList.add('glove-has-toc');

        // The active entry is the last heading scrolled past the top band.
        var setActive = function () {
            var current = headings[0];
            var limit = 120;
            headings.forEach(function (h) {
                if (h.getBoundingClientRect().top <= limit) current = h;
            });
            // A short last section never reaches the top band: at the very
            // bottom of the page, it is the one being read.
            var atBottom = window.innerHeight + window.scrollY >= document.documentElement.scrollHeight - 4;
            if (atBottom) current = headings[headings.length - 1];
            Object.keys(linkFor).forEach(function (id) {
                linkFor[id].classList.toggle('active', id === current.id);
            });
        };
        var ticking = false;
        window.addEventListener('scroll', function () {
            if (!ticking) {
                window.requestAnimationFrame(function () { setActive(); ticking = false; });
                ticking = true;
            }
        }, { passive: true });
        setActive();
    }

    // ── Prev / next cards replacing the side arrows ──
    var titleFor = function (href) {
        var target = new URL(href, document.baseURI).pathname;
        var match = Array.prototype.find.call(document.querySelectorAll('.sidebar ol.chapter a[href]'), function (a) {
            return new URL(a.href, document.baseURI).pathname === target;
        });
        return match ? match.textContent.replace(/^\s*[\d.]+\s*/, '').trim() : '';
    };
    var prev = document.querySelector('.nav-chapters.previous');
    var next = document.querySelector('.nav-chapters.next');
    if (prev || next) {
        var pager = document.createElement('nav');
        pager.className = 'glove-pager';
        pager.setAttribute('aria-label', 'Previous and next pages');
        [[prev, 'previous', '← Previous'], [next, 'next', 'Next →']].forEach(function (entry) {
            var link = entry[0];
            if (!link) return;
            var card = document.createElement('a');
            card.className = 'glove-pager-card ' + entry[1];
            card.href = link.getAttribute('href');
            var label = document.createElement('span');
            label.className = 'glove-pager-label';
            label.textContent = entry[2];
            var name = document.createElement('span');
            name.className = 'glove-pager-title';
            name.textContent = titleFor(link.href) || link.getAttribute('href');
            card.appendChild(label);
            card.appendChild(name);
            pager.appendChild(card);
        });
        main.appendChild(pager);
    }
});
