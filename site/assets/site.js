(function () {
    const canvas = document.querySelector("[data-starfield]");
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    let width = 0;
    let height = 0;
    let points = [];

    function resize() {
        const scale = Math.min(window.devicePixelRatio || 1, 2);
        width = window.innerWidth;
        height = window.innerHeight;
        canvas.width = Math.floor(width * scale);
        canvas.height = Math.floor(height * scale);
        canvas.style.width = width + "px";
        canvas.style.height = height + "px";
        ctx.setTransform(scale, 0, 0, scale, 0, 0);
        const count = Math.max(90, Math.floor((width * height) / 11500));
        points = Array.from({ length: count }, (_, index) => ({
            x: (index * 193.17) % width,
            y: (index * 91.73) % height,
            r: 0.45 + ((index * 13) % 7) / 9,
            drift: 0.12 + ((index * 5) % 10) / 80
        }));
    }

    function frame(time) {
        ctx.clearRect(0, 0, width, height);
        ctx.fillStyle = "rgba(255, 250, 229, 0.78)";
        for (const point of points) {
            const x = (point.x + Math.sin(time / 3200 + point.y) * 7) % width;
            const y = (point.y + time * point.drift / 100) % height;
            ctx.globalAlpha = 0.35 + Math.sin(time / 900 + point.x) * 0.18;
            ctx.beginPath();
            ctx.arc(x, y, point.r, 0, Math.PI * 2);
            ctx.fill();
        }
        ctx.globalAlpha = 1;
        requestAnimationFrame(frame);
    }

    resize();
    window.addEventListener("resize", resize, { passive: true });
    requestAnimationFrame(frame);
})();
