// PDF import: renders each page to a PNG at output resolution with pdf.js,
// loaded only when a PDF is imported. Off the live path; the outputs show
// the pages as ordinary presentation slides.

/** Pages are rendered to fit this box (the projection's resolution). */
const SLIDE = { width: 1920, height: 1080 };

export type PdfPage = { index: number; count: number; png: Blob };

export async function* renderPdf(data: Uint8Array): AsyncGenerator<PdfPage> {
  const pdfjs = await import('pdfjs-dist');
  pdfjs.GlobalWorkerOptions.workerSrc = (await import('pdfjs-dist/build/pdf.worker.min.mjs?url')).default;
  const document = await pdfjs.getDocument({
    data,
    standardFontDataUrl: '/pdfjs/standard_fonts/',
    cMapUrl: '/pdfjs/cmaps/',
    cMapPacked: true,
    wasmUrl: '/pdfjs/wasm/',
    isEvalSupported: false,
  }).promise;
  try {
    for (let index = 1; index <= document.numPages; index++) {
      const page = await document.getPage(index);
      const base = page.getViewport({ scale: 1 });
      const viewport = page.getViewport({ scale: Math.min(SLIDE.width / base.width, SLIDE.height / base.height) });
      const canvas = window.document.createElement('canvas');
      canvas.width = Math.round(viewport.width);
      canvas.height = Math.round(viewport.height);
      const context = canvas.getContext('2d');
      if (!context) throw new Error('Cannot draw the PDF page');
      // Transparent areas of a page are white on paper.
      context.fillStyle = '#fff';
      context.fillRect(0, 0, canvas.width, canvas.height);
      await page.render({ canvas, canvasContext: context, viewport }).promise;
      page.cleanup();
      const png = await new Promise<Blob>((resolve, reject) => canvas.toBlob(blob => blob ? resolve(blob) : reject(new Error('Cannot encode the PDF page')), 'image/png'));
      yield { index, count: document.numPages, png };
    }
  } finally {
    await document.destroy();
  }
}
