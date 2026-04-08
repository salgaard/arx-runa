// Add custom "Arx Runa" theme to mdBook theme selector

(() => {
    // Wait for DOM to load
    function init() {
        const themeList = document.getElementById('theme-list');
        const html = document.documentElement;
        
        if (!themeList) {
            console.log('Theme list not found, retrying...');
            setTimeout(init, 100);
            return;
        }

        // Create the Arx Runa theme button
        const arxRunaButton = document.createElement('button');
        arxRunaButton.id = 'arx-runa';
        arxRunaButton.className = 'theme';
        arxRunaButton.textContent = 'Arx Runa';
        
        // Insert it as the first option
        themeList.insertBefore(arxRunaButton, themeList.firstChild);
        
        // Add click handler
        arxRunaButton.addEventListener('click', () => {
            // Remove all theme classes
            const themeClasses = ['light', 'rust', 'coal', 'navy', 'ayu', 'arx-runa'];
            themeClasses.forEach(theme => html.classList.remove(theme));
            
            // Add arx-runa class
            html.classList.add('arx-runa');
            
            // Store preference
            try {
                localStorage.setItem('mdbook-theme', 'arx-runa');
            } catch (e) { }
            
            // Update active state
            document.querySelectorAll('.theme').forEach(btn => btn.classList.remove('active'));
            arxRunaButton.classList.add('active');
        });
        
        // Check if arx-runa is the stored/default theme
        const storedTheme = (() => {
            try {
                return localStorage.getItem('mdbook-theme');
            } catch (e) {
                return null;
            }
        })();
        
        if (storedTheme === 'arx-runa' || (!storedTheme && !html.className.match(/light|rust|coal|navy|ayu/))) {
            // Apply arx-runa theme
            html.classList.add('arx-runa');
            arxRunaButton.classList.add('active');
        }
    }
    
    // Start initialization
    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', init);
    } else {
        init();
    }
})();
