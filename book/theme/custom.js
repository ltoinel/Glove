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

        // Add toggle chevron to title
        var chevron = document.createElement('span');
        chevron.className = 'section-chevron';
        chevron.textContent = hasActive ? '▾' : '▸';
        title.insertBefore(chevron, title.firstChild);

        // Make title clickable
        title.style.cursor = 'pointer';
        title.addEventListener('click', function () {
            var isCollapsed = wrapper.classList.toggle('collapsed');
            chevron.textContent = isCollapsed ? '▸' : '▾';
        });
    });
});
