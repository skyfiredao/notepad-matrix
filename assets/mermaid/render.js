document.addEventListener("DOMContentLoaded", async () => {
  const results = [];
  try {
    mermaid.initialize({
      startOnLoad: false,
      layout: "elk",
      look: "classic",
      securityLevel: "strict",
      secure: ["secure", "securityLevel", "startOnLoad", "maxTextSize", "suppressErrorRendering", "maxEdges", "htmlLabels"],
      suppressErrorRendering: true,
      htmlLabels: false,
      flowchart: { htmlLabels: false, useMaxWidth: false },
      fontFamily: '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif',
      theme: window.__mermaidDark ? "dark" : "default",
    });
    await document.fonts.ready;
    for (let index = 0; index < window.__mermaidSources.length; index++) {
      const container = document.createElement("div");
      document.body.append(container);
      try {
        const { svg } = await mermaid.render(`diagram-${index}`, window.__mermaidSources[index], container);
        const element = new DOMParser().parseFromString(svg, "image/svg+xml").documentElement;
        const box = element.viewBox.baseVal;
        const width = box.width;
        const height = box.height;
        if (!(width > 0 && height > 0 && Number.isFinite(width) && Number.isFinite(height))) {
          throw new Error("Mermaid returned an invalid diagram size");
        }
        element.setAttribute("width", String(width));
        element.setAttribute("height", String(height));
        const url = URL.createObjectURL(new Blob([element.outerHTML], { type: "image/svg+xml;charset=utf-8" }));
        try {
          const image = new Image();
          image.src = url;
          await image.decode();
          const scale = Math.min(2, 8192 / width, 8192 / height, Math.sqrt(32_000_000 / (width * height)));
          const canvas = document.createElement("canvas");
          canvas.width = Math.max(1, Math.floor(width * scale));
          canvas.height = Math.max(1, Math.floor(height * scale));
          const context = canvas.getContext("2d");
          context.drawImage(image, 0, 0, canvas.width, canvas.height);
          results.push({ png: canvas.toDataURL("image/png").split(",")[1], width, height });
        } finally {
          URL.revokeObjectURL(url);
        }
      } catch (error) {
        results.push({ error: String(error.message || error) });
      } finally {
        container.remove();
      }
    }
    window.webkit.messageHandlers.mermaid.postMessage(JSON.stringify(results));
  } catch (error) {
    window.webkit.messageHandlers.mermaid.postMessage(JSON.stringify({ error: String(error.message || error) }));
  }
}, { once: true });
