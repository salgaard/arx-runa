(() => {
    // Initialize Mermaid with dark theme (coal theme is locked)
    mermaid.initialize({ startOnLoad: true, theme: 'dark' });

    // Add zoom/pan functionality to all Mermaid diagrams
    function addZoomToDiagrams() {
        const diagrams = document.querySelectorAll('.mermaid svg');
        diagrams.forEach((svg, index) => {
            // Skip if already wrapped
            if (svg.parentElement.classList.contains('diagram-wrapper')) {
                return;
            }

            // Wrap SVG in a container
            const wrapper = document.createElement('div');
            wrapper.className = 'diagram-wrapper';
            wrapper.style.cssText = 'border: 1px solid #444; margin: 1em 0; overflow: hidden; cursor: grab; position: relative;';

            // Add controls
            const controls = document.createElement('div');
            controls.className = 'diagram-controls';
            controls.style.cssText = 'position: absolute; top: 10px; right: 10px; z-index: 1000; display: flex; gap: 5px;';

            const btnStyle = 'background: #2563eb; color: white; border: none; border-radius: 4px; padding: 5px 10px; cursor: pointer; font-size: 14px;';

            const zoomInBtn = document.createElement('button');
            zoomInBtn.textContent = '+';
            zoomInBtn.title = 'Zoom In';
            zoomInBtn.style.cssText = btnStyle;

            const zoomOutBtn = document.createElement('button');
            zoomOutBtn.textContent = '-';
            zoomOutBtn.title = 'Zoom Out';
            zoomOutBtn.style.cssText = btnStyle;

            const resetBtn = document.createElement('button');
            resetBtn.textContent = '⟲';
            resetBtn.title = 'Reset View';
            resetBtn.style.cssText = btnStyle;

            const fullscreenBtn = document.createElement('button');
            fullscreenBtn.textContent = '⛶';
            fullscreenBtn.title = 'Fullscreen';
            fullscreenBtn.style.cssText = btnStyle;

            controls.appendChild(zoomInBtn);
            controls.appendChild(zoomOutBtn);
            controls.appendChild(resetBtn);
            controls.appendChild(fullscreenBtn);

            // Insert wrapper and move SVG into it
            svg.parentNode.insertBefore(wrapper, svg);
            wrapper.appendChild(controls);
            wrapper.appendChild(svg);

            // Setup pan/zoom state
            let scale = 1;
            let translateX = 0;
            let translateY = 0;
            let isDragging = false;
            let startX, startY;

            const updateTransform = () => {
                svg.style.transform = `translate(${translateX}px, ${translateY}px) scale(${scale})`;
                svg.style.transformOrigin = '0 0';
                svg.style.transition = isDragging ? 'none' : 'transform 0.2s ease';
            };

            // Zoom controls
            zoomInBtn.onclick = () => {
                scale = Math.min(scale * 1.2, 5);
                updateTransform();
            };

            zoomOutBtn.onclick = () => {
                scale = Math.max(scale / 1.2, 0.5);
                updateTransform();
            };

            resetBtn.onclick = () => {
                scale = 1;
                translateX = 0;
                translateY = 0;
                updateTransform();
            };

            fullscreenBtn.onclick = () => {
                if (!document.fullscreenElement) {
                    wrapper.requestFullscreen().catch(err => {
                        console.log('Fullscreen not supported');
                    });
                } else {
                    document.exitFullscreen();
                }
            };

            // Mouse wheel zoom
            wrapper.addEventListener('wheel', (e) => {
                e.preventDefault();
                const delta = e.deltaY > 0 ? 0.9 : 1.1;
                scale = Math.max(0.5, Math.min(5, scale * delta));
                updateTransform();
            });

            // Pan with mouse drag
            wrapper.addEventListener('mousedown', (e) => {
                if (e.target === svg || svg.contains(e.target)) {
                    isDragging = true;
                    startX = e.clientX - translateX;
                    startY = e.clientY - translateY;
                    wrapper.style.cursor = 'grabbing';
                }
            });

            document.addEventListener('mousemove', (e) => {
                if (isDragging) {
                    translateX = e.clientX - startX;
                    translateY = e.clientY - startY;
                    updateTransform();
                }
            });

            document.addEventListener('mouseup', () => {
                if (isDragging) {
                    isDragging = false;
                    wrapper.style.cursor = 'grab';
                }
            });

            // Touch support for mobile
            let lastTouchDistance = 0;

            wrapper.addEventListener('touchstart', (e) => {
                if (e.touches.length === 1) {
                    isDragging = true;
                    startX = e.touches[0].clientX - translateX;
                    startY = e.touches[0].clientY - translateY;
                } else if (e.touches.length === 2) {
                    const dx = e.touches[0].clientX - e.touches[1].clientX;
                    const dy = e.touches[0].clientY - e.touches[1].clientY;
                    lastTouchDistance = Math.sqrt(dx * dx + dy * dy);
                }
            });

            wrapper.addEventListener('touchmove', (e) => {
                e.preventDefault();
                if (e.touches.length === 1 && isDragging) {
                    translateX = e.touches[0].clientX - startX;
                    translateY = e.touches[0].clientY - startY;
                    updateTransform();
                } else if (e.touches.length === 2) {
                    const dx = e.touches[0].clientX - e.touches[1].clientX;
                    const dy = e.touches[0].clientY - e.touches[1].clientY;
                    const distance = Math.sqrt(dx * dx + dy * dy);

                    if (lastTouchDistance > 0) {
                        const delta = distance / lastTouchDistance;
                        scale = Math.max(0.5, Math.min(5, scale * delta));
                        updateTransform();
                    }

                    lastTouchDistance = distance;
                }
            });

            wrapper.addEventListener('touchend', () => {
                isDragging = false;
                lastTouchDistance = 0;
            });
        });
    }

    // Wait for Mermaid to render, then add zoom
    const observer = new MutationObserver((mutations) => {
        addZoomToDiagrams();
    });

    // Start observing after initial load
    window.addEventListener('load', () => {
        addZoomToDiagrams();
        observer.observe(document.body, { childList: true, subtree: true });
    });
})();
